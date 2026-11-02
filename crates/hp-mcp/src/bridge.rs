use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use crate::protocol::{self, Frame, ServerInfo, Session, ToolHost, ToolResult};
use crate::registry::{self, InstanceRecord};
use crate::tools::ToolSpec;

const MAX_ANSWER_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct BridgeOptions {
    pub dir: PathBuf,
    pub pinned: Option<u32>,
    pub call_timeout: Duration,
    pub connect_timeout: Duration,
    pub tools: Vec<ToolSpec>,
}

impl BridgeOptions {
    pub fn from_env() -> Self {
        Self {
            dir: registry::instances_dir(),
            pinned: std::env::var(crate::INSTANCE_ENV).ok().and_then(|v| v.trim().parse().ok()),
            call_timeout: Duration::from_secs(150),
            connect_timeout: Duration::from_secs(3),
            tools: crate::tools::EDITOR_TOOLS.to_vec(),
        }
    }
}

pub fn run_stdio() -> i32 {
    run_stdio_with(BridgeOptions::from_env())
}

pub fn run_stdio_with(options: BridgeOptions) -> i32 {
    let stdin = io::stdin();
    let stdout = io::stdout();
    match run(stdin.lock(), stdout.lock(), options) {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(io::stderr(), "hammer-pro mcp bridge: {e}");
            1
        }
    }
}

pub fn run<R: BufRead, W: Write>(mut input: R, mut output: W, options: BridgeOptions) -> io::Result<()> {
    let mut session = Session::new(ServerInfo::hammer_pro());
    let mut host = BridgeHost::new(options);
    loop {
        let frame = protocol::read_message(&mut input, crate::MAX_MESSAGE_BYTES)?;
        if frame == Frame::Eof {
            return Ok(());
        }
        if let Some(answer) = session.handle_frame(&frame, &mut host) {
            protocol::write_message(&mut output, &answer)?;
        }
    }
}

struct Link {
    record: InstanceRecord,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    next_id: u64,
}

impl Link {
    fn open(record: InstanceRecord, version: &str, options: &BridgeOptions) -> Result<Self, String> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, record.port));
        let (pid, port) = (record.pid, record.port);
        let fail =
            |e: &dyn std::fmt::Display| format!("Cannot reach the Hammer Pro window (process {pid}, port {port}): {e}");
        let stream = TcpStream::connect_timeout(&addr, options.connect_timeout).map_err(|e| fail(&e))?;
        stream.set_read_timeout(Some(options.call_timeout)).map_err(|e| fail(&e))?;
        let _ = stream.set_nodelay(true);
        let reader = BufReader::new(stream.try_clone().map_err(|e| fail(&e))?);
        let mut link = Self { record, reader, writer: stream, next_id: 1 };
        link.send(&json!({ "token": link.record.token })).map_err(|e| fail(&e))?;
        let auth = link.recv().map_err(|e| fail(&e))?;
        if auth.get("auth").and_then(Value::as_str) != Some("ok") {
            let reason = auth.get("reason").and_then(Value::as_str).unwrap_or("refused");
            return Err(fail(&reason));
        }
        let init = link.request(
            "initialize",
            json!({
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": { "name": "hammer-pro-bridge", "version": crate::VERSION },
            }),
        )?;
        if init.get("result").is_none() {
            return Err(fail(&"initialize failed"));
        }
        link.send(&protocol::notification("notifications/initialized")).map_err(|e| fail(&e))?;
        Ok(link)
    }

    fn send(&mut self, v: &Value) -> io::Result<()> {
        protocol::write_message(&mut self.writer, v)
    }

    fn recv(&mut self) -> io::Result<Value> {
        match protocol::read_message(&mut self.reader, MAX_ANSWER_BYTES)? {
            Frame::Message(text) => serde_json::from_str(&text).map_err(io::Error::other),
            Frame::Eof => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the window closed the connection")),
            _ => Err(io::Error::other("unreadable answer")),
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let pid = self.record.pid;
        self.send(&protocol::request(json!(id), method, params))
            .map_err(|e| format!("Lost the connection to Hammer Pro (process {pid}): {e}"))?;
        loop {
            let msg = self.recv().map_err(|e| format!("Lost the connection to Hammer Pro (process {pid}): {e}"))?;
            if msg.get("id") == Some(&json!(id)) {
                return Ok(msg);
            }
        }
    }
}

struct BridgeHost {
    options: BridgeOptions,
    link: Option<Link>,
}

impl BridgeHost {
    fn new(options: BridgeOptions) -> Self {
        Self { options, link: None }
    }

    fn choose(&self) -> Result<InstanceRecord, String> {
        let live = registry::live_instances(&self.options.dir);
        match self.options.pinned {
            Some(pid) => live.into_iter().find(|r| r.pid == pid).ok_or_else(|| {
                format!(
                    "The Hammer Pro window with process id {pid} does not have MCP switched on (or is closed). Call \
                     list_instances, then use_instance."
                )
            }),
            None => live.into_iter().next().ok_or_else(|| crate::NO_INSTANCE_TEXT.to_owned()),
        }
    }

    fn link(&mut self, version: &str) -> Result<&mut Link, String> {
        if let Some(link) = &self.link
            && !registry::pid_alive(link.record.pid)
        {
            self.link = None;
        }
        if self.link.is_none() {
            let record = self.choose()?;
            let link = Link::open(record, version, &self.options)?;
            let _ = writeln!(io::stderr(), "hammer-pro mcp bridge: connected to process {}", link.record.pid);
            self.link = Some(link);
        }
        self.link.as_mut().ok_or_else(|| crate::NO_INSTANCE_TEXT.to_owned())
    }

    fn forward(&mut self, name: &str, arguments: Value, version: &'static str) -> ToolResult {
        let params = json!({ "name": name, "arguments": arguments });
        let mut attempts = 0;
        loop {
            attempts += 1;
            let link = match self.link(version) {
                Ok(l) => l,
                Err(e) => return ToolResult::error(e),
            };
            let pid = link.record.pid;
            let id = link.next_id;
            link.next_id += 1;
            if link.send(&protocol::request(json!(id), "tools/call", params.clone())).is_err() {
                self.link = None;
                if attempts < 2 {
                    continue;
                }
                return ToolResult::error(format!("Lost the connection to Hammer Pro (process {pid})."));
            }
            let answer = loop {
                match link.recv() {
                    Ok(msg) if msg.get("id") == Some(&json!(id)) => break Ok(msg),
                    Ok(_) => {}
                    Err(e) => break Err(e),
                }
            };
            return match answer {
                Err(e) => {
                    self.link = None;
                    ToolResult::error(format!("Lost the connection to Hammer Pro (process {pid}) during {name}: {e}"))
                }
                Ok(msg) => match msg.get("result").and_then(ToolResult::from_value) {
                    Some(result) => result,
                    None => {
                        let text = msg.pointer("/error/message").and_then(Value::as_str).unwrap_or("no result");
                        ToolResult::error(format!("Hammer Pro answered with an error: {text}"))
                    }
                },
            };
        }
    }

    fn list_instances(&mut self) -> ToolResult {
        let live = registry::live_instances(&self.options.dir);
        let current = self.link.as_ref().map(|l| l.record.pid).or(self.options.pinned);
        let list: Vec<Value> = live
            .iter()
            .map(|r| {
                json!({
                    "pid": r.pid,
                    "map": r.map_title,
                    "version": r.version,
                    "last_focused_unix_ms": r.last_focused,
                    "current": Some(r.pid) == current,
                })
            })
            .collect();
        let default = live.first().map(|r| r.pid);
        ToolResult::json(json!({
            "instances": list,
            "current": current,
            "default": default,
            "note": if live.is_empty() { crate::NO_INSTANCE_TEXT } else { "The tools talk to \"current\" (or to \"default\", the most recently focused window, before the first call)." },
        }))
    }

    fn use_instance(&mut self, arguments: &Value, version: &str) -> ToolResult {
        let Some(pid) = arguments.get("pid").and_then(Value::as_u64).and_then(|p| u32::try_from(p).ok()) else {
            return ToolResult::error("use_instance: pid must be a process id from list_instances");
        };
        if let Some(extra) = arguments.as_object().and_then(|o| o.keys().find(|k| *k != "pid")) {
            return ToolResult::error(format!("use_instance: unknown argument \"{extra}\""));
        }
        let previous = (self.options.pinned, self.link.take());
        self.options.pinned = Some(pid);
        match self.link(version) {
            Ok(link) => {
                let map = link.record.map_title.clone();
                ToolResult::json(json!({ "pid": pid, "map": map, "connected": true }))
            }
            Err(e) => {
                (self.options.pinned, self.link) = previous;
                ToolResult::error(e)
            }
        }
    }
}

impl ToolHost for BridgeHost {
    fn list_tools(&mut self) -> Vec<Value> {
        self.options.tools.iter().chain(crate::tools::BRIDGE_TOOLS).map(ToolSpec::definition).collect()
    }

    fn call_tool(&mut self, name: &str, arguments: Value, version: &'static str) -> Option<ToolResult> {
        match name {
            "list_instances" => Some(match arguments.as_object().and_then(|o| o.keys().next()) {
                Some(extra) => {
                    ToolResult::error(format!("list_instances: this tool takes no arguments (got \"{extra}\")"))
                }
                None => self.list_instances(),
            }),
            "use_instance" => Some(self.use_instance(&arguments, version)),
            _ if self.options.tools.iter().any(|t| t.name == name) => Some(self.forward(name, arguments, version)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::server::{McpServer, ServerConfig};

    fn options(dir: &std::path::Path) -> BridgeOptions {
        BridgeOptions {
            dir: dir.to_path_buf(),
            pinned: None,
            call_timeout: Duration::from_secs(20),
            connect_timeout: Duration::from_secs(3),
            tools: crate::tools::EDITOR_TOOLS.to_vec(),
        }
    }

    fn lines(messages: &[Value]) -> Cursor<Vec<u8>> {
        let mut out = Vec::new();
        for m in messages {
            protocol::write_message(&mut out, m).unwrap();
        }
        Cursor::new(out)
    }

    fn answers(out: &[u8]) -> Vec<Value> {
        String::from_utf8(out.to_vec())
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).expect("only JSON lines on stdout"))
            .collect()
    }

    fn init() -> Value {
        json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
    }

    fn call(id: u64, name: &str, args: Value) -> Value {
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
    }

    #[test]
    fn without_a_window_it_still_answers() {
        let dir = registry::temp_dir("bridge_none");
        let input = lines(&[
            init(),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            call(2, "get_map_info", json!({})),
            call(3, "list_instances", json!({})),
            call(4, "use_instance", json!({"pid": 4_000_000_001u64})),
            call(5, "no_such_tool", json!({})),
        ]);
        let mut out = Vec::new();
        run(input, &mut out, options(&dir)).unwrap();
        let a = answers(&out);
        assert_eq!(a.len(), 6, "{a:?}");
        assert_eq!(a[0]["result"]["protocolVersion"], "2025-06-18");
        let tools = a[1]["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "create_brush"));
        assert!(tools.iter().any(|t| t["name"] == "list_instances"));
        assert_eq!(a[2]["result"]["isError"], true);
        assert!(a[2]["result"]["content"][0]["text"].as_str().unwrap().contains("MCP server switched on"));
        assert_eq!(a[3]["result"]["structuredContent"]["instances"], json!([]));
        assert_eq!(a[4]["result"]["isError"], true);
        assert_eq!(a[5]["error"]["code"], protocol::codes::INVALID_PARAMS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn end_to_end_through_a_window() {
        let dir = registry::temp_dir("bridge_e2e");
        let mut cfg = ServerConfig::new("gm_bridge.vmf");
        cfg.dir = dir.clone();
        let server = McpServer::start(cfg).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ui = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut undo_count = 0;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    match server.try_recv() {
                        Some(req) => {
                            let result = match req.name.as_str() {
                                "get_map_info" => ToolResult::json(json!({ "map": "gm_bridge.vmf", "solids": 3 })),
                                "undo" => {
                                    undo_count += 1;
                                    ToolResult::text(format!("undone {undo_count}"))
                                }
                                "screenshot_view" => ToolResult::png(&[0x89, b'P', b'N', b'G'], "3d view"),
                                other => ToolResult::error(format!("not in this test: {other}")),
                            };
                            req.respond(result);
                        }
                        None => std::thread::sleep(Duration::from_millis(2)),
                    }
                }
                drop(server);
            })
        };
        let input = lines(&[
            init(),
            call(1, "list_instances", json!({})),
            call(2, "get_map_info", json!({})),
            call(3, "undo", json!({})),
            call(4, "screenshot_view", json!({"view":"3d"})),
            call(5, "use_instance", json!({"pid": std::process::id()})),
            call(6, "undo", json!({})),
            json!({"jsonrpc":"2.0","id":7,"method":"ping"}),
        ]);
        let mut out = Vec::new();
        run(input, &mut out, options(&dir)).unwrap();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        ui.join().unwrap();
        let a = answers(&out);
        assert_eq!(a.len(), 8);
        let inst = &a[1]["result"]["structuredContent"];
        assert_eq!(inst["default"], std::process::id());
        assert_eq!(inst["instances"][0]["map"], "gm_bridge.vmf");
        assert!(inst["instances"][0].get("port").is_none());
        assert!(!serde_json::to_string(&a[1]).unwrap().contains("token"), "the token never reaches the client");
        assert_eq!(a[2]["result"]["structuredContent"]["solids"], 3);
        assert_eq!(a[3]["result"]["content"][0]["text"], "undone 1");
        assert_eq!(a[4]["result"]["content"][0]["type"], "image");
        assert_eq!(a[4]["result"]["content"][0]["data"], crate::b64::encode(&[0x89, b'P', b'N', b'G']));
        assert_eq!(a[5]["result"]["structuredContent"]["pid"], std::process::id());
        assert_eq!(a[6]["result"]["content"][0]["text"], "undone 2");
        assert_eq!(a[7]["result"], json!({}));
        let input = lines(&[init(), call(1, "get_map_info", json!({}))]);
        let mut out = Vec::new();
        run(input, &mut out, options(&dir)).unwrap();
        let a = answers(&out);
        assert!(a[1]["result"]["content"][0]["text"].as_str().unwrap().contains("MCP server switched on"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_and_broken_input() {
        let dir = registry::temp_dir("bridge_bad");
        let mut data = b"{oops\n".to_vec();
        data.extend(std::iter::repeat_n(b'x', crate::MAX_MESSAGE_BYTES + 10));
        data.extend_from_slice(b"\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n");
        let mut out = Vec::new();
        run(Cursor::new(data), &mut out, options(&dir)).unwrap();
        let a = answers(&out);
        assert_eq!(a.len(), 3);
        assert_eq!(a[0]["error"]["code"], protocol::codes::PARSE_ERROR);
        assert_eq!(a[1]["error"]["code"], protocol::codes::INVALID_REQUEST);
        assert_eq!(a[2]["result"], json!({}));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
