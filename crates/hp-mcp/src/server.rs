use std::io::{self, BufReader};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::protocol::{self, Frame, ServerInfo, Session, ToolHost, ToolResult};
use crate::registry::{self, InstanceRecord};

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_AUTH_BYTES: usize = 1024;
const REWAKE: Duration = Duration::from_millis(200);

pub type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct ServerConfig {
    pub dir: PathBuf,
    pub map_title: String,
    pub call_timeout: Duration,
    pub max_connections: usize,
    pub max_message: usize,
    pub waker: Option<Waker>,
    pub tools: Vec<Value>,
}

impl ServerConfig {
    pub fn new(map_title: impl Into<String>) -> Self {
        Self {
            dir: registry::instances_dir(),
            map_title: map_title.into(),
            call_timeout: Duration::from_secs(120),
            max_connections: 8,
            max_message: crate::MAX_MESSAGE_BYTES,
            waker: None,
            tools: crate::tools::editor_tool_definitions(),
        }
    }
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("dir", &self.dir)
            .field("map_title", &self.map_title)
            .field("max_connections", &self.max_connections)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct ToolRequest {
    pub name: String,
    pub arguments: Value,
    pub version: &'static str,
    reply: SyncSender<ToolResult>,
}

impl ToolRequest {
    pub fn new(name: impl Into<String>, arguments: Value) -> (Self, Receiver<ToolResult>) {
        let (tx, rx) = mpsc::sync_channel(1);
        (Self { name: name.into(), arguments, version: protocol::LATEST_VERSION, reply: tx }, rx)
    }

    pub fn respond(self, result: ToolResult) {
        let _ = self.reply.send(result);
    }
}

struct Shared {
    stop: AtomicBool,
    clients: AtomicUsize,
    calls: AtomicU64,
    next_conn: AtomicU64,
    conns: Mutex<Vec<(u64, TcpStream)>>,
}

pub struct McpServer {
    record: InstanceRecord,
    dir: PathBuf,
    shared: Arc<Shared>,
    requests: Receiver<ToolRequest>,
    accept: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for McpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServer").field("port", &self.record.port).field("pid", &self.record.pid).finish()
    }
}

pub fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(format!("no random source: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn same_token(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl McpServer {
    pub fn start(config: ServerConfig) -> io::Result<Self> {
        let token = new_token()?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let addr = listener.local_addr()?;
        let pid = std::process::id();
        let now = registry::now_millis();
        let record = InstanceRecord {
            pid,
            port: addr.port(),
            token: token.clone(),
            map_title: config.map_title.clone(),
            started: now / 1000,
            last_focused: now,
            version: crate::VERSION.to_owned(),
        };
        registry::write_record(&config.dir, &record)?;
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            clients: AtomicUsize::new(0),
            calls: AtomicU64::new(0),
            next_conn: AtomicU64::new(0),
            conns: Mutex::new(Vec::new()),
        });
        let (tx, rx) = mpsc::channel();
        let accept = {
            let shared = shared.clone();
            let config = config.clone();
            std::thread::Builder::new()
                .name("hp-mcp accept".into())
                .spawn(move || accept_loop(listener, token, config, shared, tx))?
        };
        log::info!("MCP server listening on {addr}");
        Ok(Self { record, dir: config.dir, shared, requests: rx, accept: Some(accept) })
    }

    pub fn port(&self) -> u16 {
        self.record.port
    }

    pub fn token(&self) -> &str {
        &self.record.token
    }

    pub fn clients(&self) -> usize {
        self.shared.clients.load(Ordering::Relaxed)
    }

    pub fn calls(&self) -> u64 {
        self.shared.calls.load(Ordering::Relaxed)
    }

    pub fn try_recv(&self) -> Option<ToolRequest> {
        self.requests.try_recv().ok()
    }

    pub fn record(&self) -> &InstanceRecord {
        &self.record
    }

    pub fn update_record(&mut self, map_title: &str, focused: bool) -> io::Result<()> {
        let mut changed = false;
        if self.record.map_title != map_title {
            map_title.clone_into(&mut self.record.map_title);
            changed = true;
        }
        if focused {
            self.record.last_focused = registry::now_millis();
            changed = true;
        }
        if changed { registry::write_record(&self.dir, &self.record) } else { Ok(()) }
    }

    pub fn stop(&mut self) {
        if self.shared.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Ok(conns) = self.shared.conns.lock() {
            for (_, s) in conns.iter() {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
        let woke = TcpStream::connect_timeout(
            &std::net::SocketAddr::from((Ipv4Addr::LOCALHOST, self.record.port)),
            Duration::from_millis(500),
        )
        .is_ok();
        if let Some(handle) = self.accept.take()
            && woke
        {
            let _ = handle.join();
        }
        registry::remove_record(&self.dir, self.record.pid);
        log::info!("MCP server stopped");
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        self.stop();
    }
}

const ACCEPT_RETRY: Duration = Duration::from_millis(50);

fn accept_loop(
    listener: TcpListener,
    token: String,
    config: ServerConfig,
    shared: Arc<Shared>,
    tx: Sender<ToolRequest>,
) {
    while !shared.stop.load(Ordering::SeqCst) {
        let stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                log::warn!("MCP server: accept failed: {e}");
                std::thread::sleep(ACCEPT_RETRY);
                continue;
            }
        };
        if shared.stop.load(Ordering::SeqCst) {
            let _ = stream.shutdown(Shutdown::Both);
            break;
        }
        if !stream.peer_addr().is_ok_and(|a| a.ip().is_loopback()) {
            let _ = stream.shutdown(Shutdown::Both);
            continue;
        }
        let open = shared.conns.lock().map_or(0, |c| c.len());
        if open >= config.max_connections {
            let mut s = stream;
            let _ = protocol::write_message(&mut s, &json!({ "auth": "denied", "reason": "too many connections" }));
            let _ = s.shutdown(Shutdown::Both);
            continue;
        }
        let id = shared.next_conn.fetch_add(1, Ordering::Relaxed);
        if let (Ok(clone), Ok(mut conns)) = (stream.try_clone(), shared.conns.lock()) {
            conns.push((id, clone));
        }
        let (token, config, shared2, tx) = (token.clone(), config.clone(), shared.clone(), tx.clone());
        let spawned = std::thread::Builder::new().name("hp-mcp connection".into()).spawn(move || {
            serve_connection(stream, &token, &config, &shared2, tx);
            if let Ok(mut conns) = shared2.conns.lock() {
                conns.retain(|(c, _)| *c != id);
            }
        });
        if spawned.is_err()
            && let Ok(mut conns) = shared.conns.lock()
        {
            conns.retain(|(c, _)| *c != id);
        }
    }
}

fn authenticate(stream: &TcpStream, reader: &mut BufReader<TcpStream>, token: &str) -> bool {
    let _ = stream.set_read_timeout(Some(AUTH_TIMEOUT));
    let ok = match protocol::read_message(reader, MAX_AUTH_BYTES) {
        Ok(Frame::Message(line)) => serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|v| v.get("token").and_then(Value::as_str).map(|t| same_token(t, token)))
            .unwrap_or(false),
        _ => false,
    };
    let mut w = stream;
    let answer = if ok {
        json!({ "auth": "ok", "pid": std::process::id(), "name": crate::SERVER_NAME, "version": crate::VERSION })
    } else {
        json!({ "auth": "denied", "reason": "a valid token must be the first message" })
    };
    let _ = protocol::write_message(&mut w, &answer);
    let _ = stream.set_read_timeout(None);
    ok
}

fn serve_connection(stream: TcpStream, token: &str, config: &ServerConfig, shared: &Shared, tx: Sender<ToolRequest>) {
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_half);
    if !authenticate(&stream, &mut reader, token) {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    shared.clients.fetch_add(1, Ordering::Relaxed);
    wake(config);
    let mut host = ChannelHost { tx, config, shared };
    let mut session = Session::new(ServerInfo::hammer_pro());
    let mut writer = &stream;
    loop {
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
        let frame = match protocol::read_message(&mut reader, config.max_message) {
            Ok(Frame::Eof) | Err(_) => break,
            Ok(frame) => frame,
        };
        let too_large = frame == Frame::TooLarge;
        if let Some(answer) = session.handle_frame(&frame, &mut host)
            && protocol::write_message(&mut writer, &answer).is_err()
        {
            break;
        }
        if too_large {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
    shared.clients.fetch_sub(1, Ordering::Relaxed);
    wake(config);
}

fn wake(config: &ServerConfig) {
    if let Some(w) = &config.waker {
        w();
    }
}

struct ChannelHost<'a> {
    tx: Sender<ToolRequest>,
    config: &'a ServerConfig,
    shared: &'a Shared,
}

impl ToolHost for ChannelHost<'_> {
    fn list_tools(&mut self) -> Vec<Value> {
        self.config.tools.clone()
    }

    fn call_tool(&mut self, name: &str, arguments: Value, version: &'static str) -> Option<ToolResult> {
        if !self.config.tools.iter().any(|t| t.get("name").and_then(Value::as_str) == Some(name)) {
            return None;
        }
        self.shared.calls.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = mpsc::sync_channel(1);
        let request = ToolRequest { name: name.to_owned(), arguments, version, reply };
        if self.tx.send(request).is_err() {
            return Some(ToolResult::error("The Hammer Pro MCP server stopped."));
        }
        wake(self.config);
        let deadline = Instant::now() + self.config.call_timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Some(ToolResult::error(format!(
                    "Hammer Pro did not answer within {} s (a dialog may be open, or the window is busy). Try again.",
                    self.config.call_timeout.as_secs()
                )));
            }
            match answer.recv_timeout(left.min(REWAKE)) {
                Ok(result) => return Some(result),
                Err(mpsc::RecvTimeoutError::Timeout) => wake(self.config),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Some(ToolResult::error("The Hammer Pro MCP server stopped before answering."));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, Write};
    use std::net::SocketAddr;

    use super::*;

    fn config(dir: &std::path::Path) -> ServerConfig {
        let mut c = ServerConfig::new("gm_test.vmf");
        c.dir = dir.to_path_buf();
        c.call_timeout = Duration::from_secs(5);
        c
    }

    struct Client {
        reader: BufReader<TcpStream>,
        writer: TcpStream,
    }

    impl Client {
        fn connect(port: u16) -> Self {
            let s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            Self { reader: BufReader::new(s.try_clone().unwrap()), writer: s }
        }
        fn send(&mut self, v: &Value) {
            protocol::write_message(&mut self.writer, v).unwrap();
        }
        fn send_raw(&mut self, text: &str) -> io::Result<()> {
            self.writer.write_all(text.as_bytes())
        }
        fn recv(&mut self) -> Option<Value> {
            match protocol::read_message(&mut self.reader, 64 * 1024 * 1024) {
                Ok(Frame::Message(t)) => serde_json::from_str(&t).ok(),
                _ => None,
            }
        }
        fn closed(&mut self) -> bool {
            let mut line = String::new();
            matches!(self.reader.read_line(&mut line), Ok(0) | Err(_))
        }
    }

    fn serve_requests(server: &McpServer, n: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut done = 0;
        while done < n && std::time::Instant::now() < deadline {
            match server.try_recv() {
                Some(req) => {
                    let name = req.name.clone();
                    let args = req.arguments.clone();
                    req.respond(ToolResult::json(json!({ "tool": name, "args": args })));
                    done += 1;
                }
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    #[test]
    fn token_is_required() {
        let dir = registry::temp_dir("server_token");
        let mut server = McpServer::start(config(&dir)).unwrap();
        let rec = registry::read_record(&registry::record_path(&dir, std::process::id())).unwrap();
        assert_eq!(rec.port, server.port());
        assert_eq!(rec.token, server.token());
        assert_eq!(rec.map_title, "gm_test.vmf");

        let mut c = Client::connect(server.port());
        c.send(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}));
        assert_eq!(c.recv().unwrap()["auth"], "denied");
        assert!(c.closed());

        let mut c = Client::connect(server.port());
        c.send(&json!({ "token": "00".repeat(32) }));
        assert_eq!(c.recv().unwrap()["auth"], "denied");
        assert!(c.closed());

        let mut c = Client::connect(server.port());
        c.send_raw("hello\n").unwrap();
        assert_eq!(c.recv().unwrap()["auth"], "denied");
        assert!(c.closed());

        let mut c = Client::connect(server.port());
        c.send(&json!({ "token": server.token() }));
        let auth = c.recv().unwrap();
        assert_eq!(auth["auth"], "ok");
        assert_eq!(auth["pid"], std::process::id());
        c.send(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}));
        let r = c.recv().unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        c.send(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        let r = c.recv().unwrap();
        assert!(r["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "create_brush"));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while server.clients() != 1 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(server.clients(), 1);

        c.send(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_map_info","arguments":{}}}));
        serve_requests(&server, 1);
        let r = c.recv().unwrap();
        assert_eq!(r["id"], 3);
        assert_eq!(r["result"]["structuredContent"]["tool"], "get_map_info");
        c.send(&json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_instances"}}));
        assert_eq!(c.recv().unwrap()["error"]["code"], protocol::codes::INVALID_PARAMS);
        assert!(server.try_recv().is_none());
        assert_eq!(server.calls(), 1);

        server.stop();
        assert!(c.closed(), "stopping closes the connections");
        assert!(!registry::record_path(&dir, std::process::id()).exists(), "the record is removed");
        assert!(TcpStream::connect_timeout(&rec_addr(rec.port), Duration::from_millis(500)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn rec_addr(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    #[test]
    fn size_and_connection_limits() {
        let dir = registry::temp_dir("server_limits");
        let mut cfg = config(&dir);
        cfg.max_message = 1000;
        cfg.max_connections = 2;
        let server = McpServer::start(cfg).unwrap();
        let mut a = Client::connect(server.port());
        a.send(&json!({ "token": server.token() }));
        assert_eq!(a.recv().unwrap()["auth"], "ok");
        let mut b = Client::connect(server.port());
        b.send(&json!({ "token": server.token() }));
        assert_eq!(b.recv().unwrap()["auth"], "ok");
        let mut c = Client::connect(server.port());
        assert_eq!(c.recv().unwrap()["auth"], "denied");
        assert!(c.closed());
        let big = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"x\":\"{}\"}}\n", "x".repeat(5000));
        let _ = a.send_raw(&big);
        let r = a.recv().unwrap();
        assert_eq!(r["error"]["code"], protocol::codes::INVALID_REQUEST);
        assert!(a.closed());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let mut d = Client::connect(server.port());
            d.send(&json!({ "token": server.token() }));
            if d.recv().is_some_and(|v| v["auth"] == "ok") {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "slot never freed");
            std::thread::sleep(Duration::from_millis(20));
        }
        drop(server);
        assert!(!registry::record_path(&dir, std::process::id()).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unanswered_calls_time_out() {
        let dir = registry::temp_dir("server_timeout");
        let mut cfg = config(&dir);
        cfg.call_timeout = Duration::from_millis(200);
        let server = McpServer::start(cfg).unwrap();
        let mut c = Client::connect(server.port());
        c.send(&json!({ "token": server.token() }));
        assert_eq!(c.recv().unwrap()["auth"], "ok");
        c.send(&json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"undo"}}));
        let r = c.recv().unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("did not answer"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_follow_the_window() {
        let dir = registry::temp_dir("server_record");
        let mut server = McpServer::start(config(&dir)).unwrap();
        let before = server.record().last_focused;
        std::thread::sleep(Duration::from_millis(5));
        server.update_record("gm_other.vmf", true).unwrap();
        let rec = registry::read_record(&registry::record_path(&dir, std::process::id())).unwrap();
        assert_eq!(rec.map_title, "gm_other.vmf");
        assert!(rec.last_focused > before);
        assert_eq!(new_token().unwrap().len(), 64);
        assert_ne!(new_token().unwrap(), new_token().unwrap());
        assert!(same_token("abc", "abc") && !same_token("abc", "abd") && !same_token("abc", "ab"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
