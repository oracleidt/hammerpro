use std::io::{self, Read, Write};
use std::path::PathBuf;

pub trait Conn: Send {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn read_ready(&mut self, buf: &mut [u8]) -> io::Result<usize>;
}

pub trait Connector: Send {
    fn connect(&mut self) -> io::Result<Box<dyn Conn>>;
}

#[derive(Clone, Debug)]
pub struct SystemConnector {
    paths: Vec<PathBuf>,
}

impl SystemConnector {
    pub fn discord() -> Self {
        Self { paths: discord_paths() }
    }

    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

impl Connector for SystemConnector {
    fn connect(&mut self) -> io::Result<Box<dyn Conn>> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, "Discord is not running");
        for path in &self.paths {
            match open(path) {
                Ok(conn) => return Ok(conn),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}

#[cfg(windows)]
fn discord_paths() -> Vec<PathBuf> {
    (0..10).map(|i| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{i}"))).collect()
}

#[cfg(not(windows))]
fn discord_paths() -> Vec<PathBuf> {
    let dir = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .find_map(|k| std::env::var_os(k).filter(|v| !v.is_empty()))
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    (0..10).map(|i| dir.join(format!("discord-ipc-{i}"))).collect()
}

#[cfg(windows)]
fn open(path: &std::path::Path) -> io::Result<Box<dyn Conn>> {
    let file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
    Ok(Box::new(PipeConn(file)))
}

#[cfg(windows)]
struct PipeConn(std::fs::File);

#[cfg(windows)]
impl Conn for PipeConn {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write_all(bytes)?;
        self.0.flush()
    }

    fn read_ready(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        use std::os::windows::io::AsRawHandle;
        let mut avail = 0u32;
        let ok = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                self.0.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut avail,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if avail == 0 || buf.is_empty() {
            return Ok(0);
        }
        let n = buf.len().min(avail as usize);
        match self.0.read(&mut buf[..n])? {
            0 => Err(io::ErrorKind::UnexpectedEof.into()),
            n => Ok(n),
        }
    }
}

#[cfg(unix)]
fn open(path: &std::path::Path) -> io::Result<Box<dyn Conn>> {
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
    Ok(Box::new(SocketConn(stream)))
}

#[cfg(unix)]
struct SocketConn(std::os::unix::net::UnixStream);

#[cfg(unix)]
impl Conn for SocketConn {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.0.write_all(bytes)
    }

    fn read_ready(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.set_nonblocking(true)?;
        let read = self.0.read(buf);
        self.0.set_nonblocking(false)?;
        match read {
            Ok(0) if !buf.is_empty() => Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(e),
        }
    }
}

#[cfg(not(any(windows, unix)))]
fn open(_path: &std::path::Path) -> io::Result<Box<dyn Conn>> {
    Err(io::ErrorKind::Unsupported.into())
}
