use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

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

pub fn user_agent() -> String {
    format!("HammerPro/{} (auto update)", env!("CARGO_PKG_VERSION"))
}

pub fn get_text(url: &str, max_bytes: u64) -> Result<String, UpdateError> {
    let mut stream = open(url, "application/vnd.github+json, application/json")?;
    let mut data = Vec::new();
    (&mut stream).take(max_bytes + 1).read_to_end(&mut data).map_err(net)?;
    if data.len() as u64 > max_bytes {
        return Err(UpdateError::Network("the answer is too large".into()));
    }
    stream.finish()?;
    String::from_utf8(data).map_err(|_| UpdateError::Network("the answer is not text".into()))
}

pub fn download_to(
    url: &str,
    dest: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<u64, UpdateError> {
    let mut stream = open(url, "application/octet-stream")?;
    let total = stream.length();
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = dest.with_extension("part");
    let result = copy_stream(&mut stream, &part, total, cancel, progress);
    match result {
        Ok(done) => {
            if let Some(total) = total
                && total != done
            {
                let _ = std::fs::remove_file(&part);
                return Err(UpdateError::Network(format!("incomplete download ({done} of {total} bytes)")));
            }
            let _ = std::fs::remove_file(dest);
            std::fs::rename(&part, dest)?;
            Ok(done)
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
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<u64, UpdateError> {
    let mut file = std::fs::File::create(part)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    progress(0, total);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(UpdateError::Cancelled);
        }
        let n = stream.read(&mut buf).map_err(net)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        progress(done, total);
    }
    file.flush()?;
    stream.finish()?;
    Ok(done)
}

fn net(e: std::io::Error) -> UpdateError {
    UpdateError::Network(e.to_string())
}

pub fn open(url: &str, accept: &str) -> Result<Stream, UpdateError> {
    let parsed = Url::parse(url).ok_or_else(|| UpdateError::Network(format!("not an http(s) address: {url}")))?;
    Stream::connect(&parsed, accept)
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
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPTION_SECURE_PROTOCOLS,
        WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpAddRequestHeaders,
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData,
        WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
    };

    use super::{Url, user_agent};
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
        pub fn connect(url: &Url, accept: &str) -> Result<Stream, UpdateError> {
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
                if status != 200 {
                    return Err(UpdateError::Http(status));
                }
                Ok(Stream { request, _connect: connect, _session: session, status, length })
            }
        }

        pub fn status(&self) -> u32 {
            self.status
        }

        pub fn length(&self) -> Option<u64> {
            self.length
        }

        pub fn finish(&mut self) -> Result<(), UpdateError> {
            Ok(())
        }
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

    use super::{Url, user_agent};
    use crate::error::UpdateError;

    pub struct Stream {
        child: Child,
        stdout: std::process::ChildStdout,
    }

    impl Stream {
        pub fn connect(url: &Url, accept: &str) -> Result<Stream, UpdateError> {
            let full = format!("{}://{}:{}{}", if url.secure { "https" } else { "http" }, url.host, url.port, url.path);
            let mut child = Command::new("curl")
                .args(["-sS", "-L", "-f", "--max-time", "600", "-A", &user_agent(), "-H", &format!("Accept: {accept}")])
                .arg(full)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| UpdateError::Network(format!("curl: {e}")))?;
            let stdout = child.stdout.take().ok_or_else(|| UpdateError::Network("curl: no output".into()))?;
            Ok(Stream { child, stdout })
        }

        pub fn status(&self) -> u32 {
            200
        }

        pub fn length(&self) -> Option<u64> {
            None
        }

        pub fn finish(&mut self) -> Result<(), UpdateError> {
            let status = self.child.wait().map_err(|e| UpdateError::Network(e.to_string()))?;
            if status.success() { Ok(()) } else { Err(UpdateError::Network(format!("curl failed ({status})"))) }
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
