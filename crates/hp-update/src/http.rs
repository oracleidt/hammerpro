use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::error::UpdateError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl Url {
    pub fn parse(text: &str) -> Option<Url> {
        let (scheme, rest) = text.trim().split_once("://")?;
        let secure = match scheme.to_ascii_lowercase().as_str() {
            "https" => true,
            "http" => false,
            _ => return None,
        };
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        if authority.is_empty() || authority.contains('@') {
            return None;
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host, port.parse::<u16>().ok().filter(|p| *p != 0)?),
            None => (authority, if secure { 443 } else { 80 }),
        };
        if host.is_empty() || !host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) {
            return None;
        }
        let tail = tail.split('#').next().unwrap_or("");
        let path = if tail.is_empty() {
            "/".to_owned()
        } else if tail.starts_with('?') {
            format!("/{tail}")
        } else {
            tail.to_owned()
        };
        Some(Url { secure, host: host.to_owned(), port, path })
    }
}

pub const MAX_REDIRECTS: usize = 5;

pub fn user_agent() -> String {
    format!("HammerPro/{} (auto update)", env!("CARGO_PKG_VERSION"))
}

pub enum Reply {
    Body(Stream),
    Redirect(String),
}

pub fn resolve_location(base: &Url, location: &str) -> Option<String> {
    let location = location.trim();
    if location.is_empty() || location.chars().any(char::is_control) {
        return None;
    }
    let lower = location.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return Some(location.to_owned());
    }
    let scheme = if base.secure { "https" } else { "http" };
    if let Some(rest) = location.strip_prefix("//") {
        return Some(format!("{scheme}://{rest}"));
    }
    if location.starts_with('/') {
        let default = if base.secure { 443 } else { 80 };
        let authority = if base.port == default { base.host.clone() } else { format!("{}:{}", base.host, base.port) };
        return Some(format!("{scheme}://{authority}{location}"));
    }
    None
}

pub fn open(url: &str, accept: &str, trusted: &dyn Fn(&Url) -> bool) -> Result<Stream, UpdateError> {
    let mut current = url.trim().to_owned();
    for _ in 0..=MAX_REDIRECTS {
        let parsed =
            Url::parse(&current).ok_or_else(|| UpdateError::Network(format!("not an http(s) address: {current}")))?;
        if !trusted(&parsed) {
            return Err(UpdateError::Network(format!("the address is not trusted: {}", parsed.host)));
        }
        match Stream::connect(&parsed, accept)? {
            Reply::Body(stream) => return Ok(stream),
            Reply::Redirect(location) => {
                current = resolve_location(&parsed, &location)
                    .ok_or_else(|| UpdateError::Network("the server sent an unusable redirection".into()))?;
            }
        }
    }
    Err(UpdateError::Network(format!("more than {MAX_REDIRECTS} redirections")))
}

fn check_final(stream: &mut Stream, trusted: &dyn Fn(&Url) -> bool) -> Result<(), UpdateError> {
    match stream.finish()? {
        Some(last) => match Url::parse(&last) {
            Some(url) if trusted(&url) => Ok(()),
            _ => Err(UpdateError::Network(format!("the address is not trusted: {last}"))),
        },
        None => Ok(()),
    }
}

pub fn get_text(url: &str, max_bytes: u64, trusted: &dyn Fn(&Url) -> bool) -> Result<String, UpdateError> {
    let mut stream = open(url, "application/vnd.github+json, application/json", trusted)?;
    if stream.length().is_some_and(|n| n > max_bytes) {
        return Err(UpdateError::Network("the answer is too large".into()));
    }
    let mut data = Vec::new();
    (&mut stream).take(max_bytes + 1).read_to_end(&mut data).map_err(net)?;
    if data.len() as u64 > max_bytes {
        return Err(UpdateError::Network("the answer is too large".into()));
    }
    check_final(&mut stream, trusted)?;
    String::from_utf8(data).map_err(|_| UpdateError::Network("the answer is not text".into()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    pub bytes: u64,
    pub sha256: String,
}

pub struct Limits<'a> {
    pub max_bytes: u64,
    pub cancel: &'a AtomicBool,
    pub trusted: &'a dyn Fn(&Url) -> bool,
}

pub fn download_to(
    url: &str,
    dest: &Path,
    limits: &Limits<'_>,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Fetched, UpdateError> {
    let mut stream = open(url, "application/octet-stream", limits.trusted)?;
    let total = stream.length();
    if let Some(total) = total
        && total > limits.max_bytes
    {
        return Err(UpdateError::Network(format!("the file is too large ({total} bytes)")));
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = dest.with_extension("part");
    let result = copy_stream(&mut stream, &part, total, limits, progress)
        .and_then(|fetched| check_final(&mut stream, limits.trusted).map(|()| fetched));
    match result {
        Ok(fetched) => {
            if let Some(total) = total
                && total != fetched.bytes
            {
                let _ = std::fs::remove_file(&part);
                return Err(UpdateError::Network(format!("incomplete download ({} of {total} bytes)", fetched.bytes)));
            }
            let _ = std::fs::remove_file(dest);
            std::fs::rename(&part, dest)?;
            Ok(fetched)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

fn copy_stream(
    stream: &mut Stream,
    part: &Path,
    total: Option<u64>,
    limits: &Limits<'_>,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Fetched, UpdateError> {
    let mut file = std::fs::File::create(part)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    progress(0, total);
    loop {
        if limits.cancel.load(Ordering::Relaxed) {
            return Err(UpdateError::Cancelled);
        }
        let n = stream.read(&mut buf).map_err(net)?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > limits.max_bytes {
            return Err(UpdateError::Network(format!("the file is larger than {} bytes", limits.max_bytes)));
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        progress(done, total);
    }
    file.flush()?;
    Ok(Fetched { bytes: done, sha256: hex(&hasher.finalize()) })
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> Result<String, UpdateError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn net(e: std::io::Error) -> UpdateError {
    UpdateError::Network(e.to_string())
}

#[cfg(windows)]
pub use win::Stream;

#[cfg(not(windows))]
pub use curl::Stream;

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::io::Read;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_REDIRECT_POLICY,
        WINHTTP_OPTION_REDIRECT_POLICY_NEVER, WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_QUERY_CONTENT_LENGTH,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_LOCATION, WINHTTP_QUERY_STATUS_CODE, WinHttpAddRequestHeaders,
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData,
        WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
    };

    use super::{Reply, Url, user_agent};
    use crate::error::UpdateError;

    const ADD_HEADER: u32 = 0x2000_0000;
    const TLS_1_2: u32 = 0x0000_0800;
    const TLS_1_3: u32 = 0x0000_2000;

    struct Handle(*mut c_void);

    unsafe impl Send for Handle {}

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    pub struct Stream {
        request: Handle,
        _connect: Handle,
        _session: Handle,
        status: u32,
        length: Option<u64>,
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_error(what: &str) -> UpdateError {
        UpdateError::Network(format!("{what}: {}", std::io::Error::last_os_error()))
    }

    impl Stream {
        pub fn connect(url: &Url, accept: &str) -> Result<Reply, UpdateError> {
            unsafe {
                let agent = wide(&user_agent());
                let session =
                    Handle(WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0));
                if session.0.is_null() {
                    return Err(last_error("WinHttpOpen"));
                }
                WinHttpSetTimeouts(session.0, 10_000, 10_000, 30_000, 30_000);
                let protocols = TLS_1_2 | TLS_1_3;
                WinHttpSetOption(
                    session.0,
                    WINHTTP_OPTION_SECURE_PROTOCOLS,
                    (&raw const protocols).cast(),
                    size_of::<u32>() as u32,
                );
                let host = wide(&url.host);
                let connect = Handle(WinHttpConnect(session.0, host.as_ptr(), url.port, 0));
                if connect.0.is_null() {
                    return Err(last_error("WinHttpConnect"));
                }
                let verb = wide("GET");
                let path = wide(&url.path);
                let flags = if url.secure { WINHTTP_FLAG_SECURE } else { 0 };
                let request =
                    Handle(WinHttpOpenRequest(connect.0, verb.as_ptr(), path.as_ptr(), null(), null(), null(), flags));
                if request.0.is_null() {
                    return Err(last_error("WinHttpOpenRequest"));
                }
                let never = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
                if WinHttpSetOption(
                    request.0,
                    WINHTTP_OPTION_REDIRECT_POLICY,
                    (&raw const never).cast(),
                    size_of::<u32>() as u32,
                ) == 0
                {
                    return Err(last_error("WinHttpSetOption"));
                }
                let header = wide(&format!("Accept: {accept}"));
                WinHttpAddRequestHeaders(request.0, header.as_ptr(), u32::MAX, ADD_HEADER);
                if WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) == 0 {
                    return Err(last_error("WinHttpSendRequest"));
                }
                if WinHttpReceiveResponse(request.0, null_mut()) == 0 {
                    return Err(last_error("WinHttpReceiveResponse"));
                }
                let status = query_number(request.0, WINHTTP_QUERY_STATUS_CODE).unwrap_or(0) as u32;
                let length = query_number(request.0, WINHTTP_QUERY_CONTENT_LENGTH);
                if matches!(status, 301 | 302 | 303 | 307 | 308) {
                    return query_text(request.0, WINHTTP_QUERY_LOCATION)
                        .map(Reply::Redirect)
                        .ok_or_else(|| UpdateError::Network("a redirection without a location".into()));
                }
                if status != 200 {
                    return Err(UpdateError::Http(status));
                }
                Ok(Reply::Body(Stream { request, _connect: connect, _session: session, status, length }))
            }
        }

        pub fn status(&self) -> u32 {
            self.status
        }

        pub fn length(&self) -> Option<u64> {
            self.length
        }

        pub fn finish(&mut self) -> Result<Option<String>, UpdateError> {
            Ok(None)
        }
    }

    unsafe fn query_text(request: *mut c_void, what: u32) -> Option<String> {
        let mut buf = vec![0u16; 16 * 1024];
        let mut size = (buf.len() * 2) as u32;
        let ok =
            unsafe { WinHttpQueryHeaders(request, what, null(), buf.as_mut_ptr().cast(), &raw mut size, null_mut()) };
        if ok == 0 {
            return None;
        }
        let len = (size as usize / 2).min(buf.len());
        String::from_utf16(&buf[..len]).ok()
    }

    unsafe fn query_number(request: *mut c_void, what: u32) -> Option<u64> {
        let mut value: u32 = 0;
        let mut size = size_of::<u32>() as u32;
        let ok = unsafe {
            WinHttpQueryHeaders(
                request,
                what | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&raw mut value).cast(),
                &raw mut size,
                null_mut(),
            )
        };
        (ok != 0).then_some(u64::from(value))
    }

    impl Read for Stream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let want = buf.len().min(1 << 20) as u32;
            let mut got: u32 = 0;
            let ok = unsafe { WinHttpReadData(self.request.0, buf.as_mut_ptr().cast(), want, &raw mut got) };
            if ok == 0 { Err(std::io::Error::last_os_error()) } else { Ok(got as usize) }
        }
    }
}

#[cfg(not(windows))]
mod curl {
    use std::io::Read;
    use std::process::{Child, Command, Stdio};

    use super::{Reply, Url, user_agent};
    use crate::error::UpdateError;

    pub struct Stream {
        child: Child,
        stdout: std::process::ChildStdout,
        stderr: Option<std::process::ChildStderr>,
    }

    impl Stream {
        pub fn connect(url: &Url, accept: &str) -> Result<Reply, UpdateError> {
            let full = format!("{}://{}:{}{}", if url.secure { "https" } else { "http" }, url.host, url.port, url.path);
            let mut cmd = Command::new("curl");
            cmd.args(["-sS", "-L", "-f", "--max-redirs", "5", "--max-time", "600", "-A", &user_agent()]).args([
                "-H",
                &format!("Accept: {accept}"),
                "-w",
                "%{stderr}\n%{url_effective}\n",
            ]);
            if url.secure {
                cmd.args(["--proto", "=https", "--proto-redir", "=https"]);
            }
            let mut child = cmd
                .arg(full)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| UpdateError::Network(format!("curl: {e}")))?;
            let stdout = child.stdout.take().ok_or_else(|| UpdateError::Network("curl: no output".into()))?;
            let stderr = child.stderr.take();
            Ok(Reply::Body(Stream { child, stdout, stderr }))
        }

        pub fn status(&self) -> u32 {
            200
        }

        pub fn length(&self) -> Option<u64> {
            None
        }

        pub fn finish(&mut self) -> Result<Option<String>, UpdateError> {
            let mut text = String::new();
            if let Some(mut err) = self.stderr.take() {
                let _ = (&mut err).take(64 * 1024).read_to_string(&mut text);
            }
            let status = self.child.wait().map_err(|e| UpdateError::Network(e.to_string()))?;
            if !status.success() {
                return Err(UpdateError::Network(format!("curl failed ({status})")));
            }
            Ok(text.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_owned()))
        }
    }

    impl Read for Stream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.stdout.read(buf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls() {
        let u = Url::parse("https://api.github.com/repos/o/r/releases/latest").unwrap();
        assert!(u.secure);
        assert_eq!((u.host.as_str(), u.port, u.path.as_str()), ("api.github.com", 443, "/repos/o/r/releases/latest"));
        let u = Url::parse("http://127.0.0.1:8123/api?x=1#frag").unwrap();
        assert!(!u.secure);
        assert_eq!((u.host.as_str(), u.port, u.path.as_str()), ("127.0.0.1", 8123, "/api?x=1"));
        assert_eq!(Url::parse("http://localhost").unwrap().path, "/");
        assert_eq!(Url::parse("https://h?q=1").unwrap().path, "/?q=1");
        for bad in [
            "",
            "ftp://a/b",
            "http://",
            "http://u@h/x",
            "http://h:0/x",
            "http://h:99999/x",
            "http://[::1]/x",
            "file:///c",
        ] {
            assert!(Url::parse(bad).is_none(), "{bad}");
        }
    }
}
