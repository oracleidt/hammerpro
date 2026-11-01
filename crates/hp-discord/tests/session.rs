use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hp_discord::frame::{self, Frame, Opcode};
use hp_discord::{Activity, Conn, Connector, Link, Options, Presence, Session, Wanted};
use serde_json::Value;

#[derive(Default)]
struct Server {
    up: bool,
    refuse: bool,
    reject_activity: bool,
    generation: u32,
    connects: usize,
    received: Vec<Frame>,
    inbox: Vec<u8>,
    outbox: Vec<u8>,
}

impl Server {
    fn restart(&mut self) {
        self.generation += 1;
        self.up = true;
        self.inbox.clear();
        self.outbox.clear();
    }

    fn payloads(&self, op: Opcode) -> Vec<Value> {
        self.received.iter().filter(|f| f.op == op).map(|f| serde_json::from_str(&f.payload).unwrap()).collect()
    }

    fn activities(&self) -> Vec<Value> {
        self.payloads(Opcode::Frame).into_iter().map(|v| v["args"]["activity"].clone()).collect()
    }
}

type Shared = Arc<Mutex<Server>>;

struct MockConnector(Shared);
struct MockConn(Shared, u32);

impl Connector for MockConnector {
    fn connect(&mut self) -> io::Result<Box<dyn Conn>> {
        let mut s = self.0.lock().unwrap();
        if !s.up {
            return Err(io::ErrorKind::NotFound.into());
        }
        s.connects += 1;
        s.inbox.clear();
        s.outbox.clear();
        Ok(Box::new(MockConn(self.0.clone(), s.generation)))
    }
}

impl MockConn {
    fn check(&self, s: &Server) -> io::Result<()> {
        if s.up && s.generation == self.1 { Ok(()) } else { Err(io::ErrorKind::BrokenPipe.into()) }
    }
}

impl Conn for MockConn {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut s = self.0.lock().unwrap();
        self.check(&s)?;
        s.inbox.extend_from_slice(bytes);
        while let Some((f, used)) = frame::decode(&s.inbox).unwrap() {
            s.inbox.drain(..used);
            if f.op == Opcode::Handshake {
                let answer = if s.refuse {
                    frame::encode(Opcode::Close, "{\"code\":4000,\"message\":\"Invalid Client ID\"}")
                } else {
                    frame::encode(Opcode::Frame, "{\"cmd\":\"DISPATCH\",\"evt\":\"READY\",\"data\":{\"v\":1}}")
                };
                s.outbox.extend(answer);
            } else if f.op == Opcode::Frame && f.payload.contains("SET_ACTIVITY") {
                let nonce = serde_json::from_str::<Value>(&f.payload).unwrap()["nonce"].clone();
                let answer = if s.reject_activity {
                    serde_json::json!({ "cmd": "SET_ACTIVITY", "evt": "ERROR", "nonce": nonce,
                        "data": { "code": 4000, "message": "child \"activity\" fails" } })
                } else {
                    serde_json::json!({ "cmd": "SET_ACTIVITY", "evt": null, "nonce": nonce, "data": {} })
                };
                s.outbox.extend(frame::encode(Opcode::Frame, &answer.to_string()));
            }
            s.received.push(f);
        }
        Ok(())
    }

    fn read_ready(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut s = self.0.lock().unwrap();
        self.check(&s)?;
        let n = buf.len().min(s.outbox.len());
        buf[..n].copy_from_slice(&s.outbox[..n]);
        s.outbox.drain(..n);
        Ok(n)
    }
}

const ID: &str = "1234567890123456789";

fn opts() -> Options {
    Options {
        throttle: Duration::from_secs(15),
        retry_min: Duration::from_secs(5),
        retry_max: Duration::from_secs(60),
        handshake_timeout: Duration::from_millis(200),
        poll: Duration::from_secs(5),
    }
}

fn server(up: bool) -> Shared {
    Arc::new(Mutex::new(Server { up, ..Server::default() }))
}

fn activity(map: &str, tool: &str) -> Activity {
    Activity {
        details: format!("Editing {map}"),
        state: tool.into(),
        start: Some(1_700_000_000),
        large_image: "hammerpro".into(),
        large_text: "Hammer Pro 1.0.0".into(),
        ..Activity::default()
    }
}

fn wanted(a: Activity) -> Wanted {
    Wanted { client_id: ID.into(), activity: Some(a) }
}

#[test]
fn handshake_then_activity() {
    let srv = server(true);
    let mut s = Session::new(Box::new(MockConnector(srv.clone())), opts());
    s.set(wanted(activity("gm_city.vmf", "Block")));
    s.step(Instant::now());
    assert!(s.is_connected());
    let srv = srv.lock().unwrap();
    let hs = srv.payloads(Opcode::Handshake);
    assert_eq!(hs, vec![serde_json::json!({ "v": 1, "client_id": ID })]);
    let frames = srv.payloads(Opcode::Frame);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["cmd"], "SET_ACTIVITY");
    assert_eq!(frames[0]["args"]["pid"], std::process::id());
    assert!(frames[0]["nonce"].is_string());
    assert_eq!(frames[0]["args"]["activity"]["details"], "Editing gm_city.vmf");
}

#[test]
fn throttles_but_shows_map_switches_at_once() {
    let srv = server(true);
    let mut s = Session::new(Box::new(MockConnector(srv.clone())), opts());
    let t0 = Instant::now();
    s.set(wanted(activity("a.vmf", "Block")));
    s.step(t0);
    s.set(wanted(activity("a.vmf", "Clip")));
    let next = s.step(t0 + Duration::from_secs(1)).unwrap();
    assert!(next <= t0 + Duration::from_secs(15));
    assert_eq!(srv.lock().unwrap().activities().len(), 1);
    s.step(t0 + Duration::from_secs(10));
    assert_eq!(srv.lock().unwrap().activities().len(), 1);
    s.step(t0 + Duration::from_secs(15));
    assert_eq!(srv.lock().unwrap().activities()[1]["state"], "Clip");
    s.set(wanted(activity("b.vmf", "Clip")));
    s.step(t0 + Duration::from_secs(16));
    let acts = srv.lock().unwrap().activities();
    assert_eq!(acts.len(), 3);
    assert_eq!(acts[2]["details"], "Editing b.vmf");
    s.step(t0 + Duration::from_secs(60));
    assert_eq!(srv.lock().unwrap().activities().len(), 3);
}

#[test]
fn quiet_retries_and_reconnects() {
    let srv = server(false);
    let mut s = Session::new(Box::new(MockConnector(srv.clone())), opts());
    let t0 = Instant::now();
    s.set(wanted(activity("a.vmf", "Block")));
    assert_eq!(s.step(t0), Some(t0 + Duration::from_secs(5)));
    assert_eq!(s.step(t0 + Duration::from_secs(1)), Some(t0 + Duration::from_secs(5)));
    let t1 = t0 + Duration::from_secs(5);
    assert_eq!(s.step(t1), Some(t1 + Duration::from_secs(10)));
    assert!(!s.is_connected());
    assert_eq!(s.link(), &Link::NotRunning);
    srv.lock().unwrap().restart();
    s.step(t1 + Duration::from_secs(9));
    assert!(!s.is_connected(), "waits for the retry time");
    let t2 = t1 + Duration::from_secs(10);
    s.step(t2);
    assert!(s.is_connected());
    assert_eq!(s.link(), &Link::Connected);
    assert_eq!(srv.lock().unwrap().activities().len(), 1);
    srv.lock().unwrap().restart();
    s.step(t2 + Duration::from_secs(1));
    assert!(!s.is_connected());
    s.step(t2 + Duration::from_secs(6));
    assert!(s.is_connected());
    let srv = srv.lock().unwrap();
    assert_eq!(srv.connects, 2);
    assert_eq!(srv.activities().len(), 2);
}

#[test]
fn refused_handshake_retries() {
    let srv = server(true);
    srv.lock().unwrap().refuse = true;
    let mut s = Session::new(Box::new(MockConnector(srv.clone())), opts());
    let t0 = Instant::now();
    s.set(wanted(activity("a.vmf", "Block")));
    assert_eq!(s.step(t0), Some(t0 + Duration::from_secs(5)));
    assert!(!s.is_connected());
    assert_eq!(s.link(), &Link::Refused("Invalid Client ID (4000)".into()));
    assert!(srv.lock().unwrap().activities().is_empty());
}

#[test]
fn off_clears_and_closes() {
    let srv = server(true);
    let mut s = Session::new(Box::new(MockConnector(srv.clone())), opts());
    let t0 = Instant::now();
    s.set(wanted(activity("a.vmf", "Block")));
    s.step(t0);
    s.set(Wanted { client_id: ID.into(), activity: None });
    assert_eq!(s.step(t0 + Duration::from_secs(1)), None);
    assert!(!s.is_connected());
    assert_eq!(s.link(), &Link::Idle);
    let srv_ = srv.lock().unwrap();
    let acts = srv_.activities();
    assert_eq!(acts.len(), 2);
    assert!(acts[1].is_null(), "the activity is cleared");
    assert_eq!(srv_.received.last().unwrap().op, Opcode::Close);
    drop(srv_);
    s.set(wanted(activity("a.vmf", "Block")));
    s.step(t0 + Duration::from_secs(2));
    assert!(s.is_connected());
    s.set(Wanted { client_id: "9876543210987654321".into(), activity: Some(activity("a.vmf", "Block")) });
    s.step(t0 + Duration::from_secs(3));
    let hs = srv.lock().unwrap().payloads(Opcode::Handshake);
    assert_eq!(hs.last().unwrap()["client_id"], "9876543210987654321");
}

#[test]
fn answers_pings() {
    let srv = server(true);
    let mut s = Session::new(Box::new(MockConnector(srv.clone())), opts());
    let t0 = Instant::now();
    s.set(wanted(activity("a.vmf", "Block")));
    s.step(t0);
    srv.lock().unwrap().outbox.extend(frame::encode(Opcode::Ping, "{\"n\":1}"));
    s.step(t0 + Duration::from_secs(5));
    assert!(s.is_connected());
    let srv = srv.lock().unwrap();
    let pong = srv.received.iter().find(|f| f.op == Opcode::Pong).unwrap();
    assert_eq!(pong.payload, "{\"n\":1}");
}

fn wait_for(srv: &Shared, what: impl Fn(&Server) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if what(&srv.lock().unwrap()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn background_thread() {
    let srv = server(true);
    let mut p = Presence::new(Box::new(MockConnector(srv.clone())), opts());
    p.set(Wanted::default());
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(srv.lock().unwrap().connects, 0);
    assert_eq!(p.link(), Link::Idle);
    p.set(wanted(activity("a.vmf", "Block")));
    assert!(wait_for(&srv, |s| s.activities().len() == 1));
    let end = Instant::now() + Duration::from_secs(5);
    while p.link() != Link::Connected && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(p.link(), Link::Connected);
    drop(p);
    assert!(wait_for(&srv, |s| s.received.last().is_some_and(|f| f.op == Opcode::Close)));
    assert!(srv.lock().unwrap().activities()[1].is_null());
}

#[test]
fn background_thread_waits_for_discord() {
    let srv = server(false);
    let o = Options { retry_min: Duration::from_millis(30), retry_max: Duration::from_millis(60), ..opts() };
    let mut p = Presence::new(Box::new(MockConnector(srv.clone())), o);
    p.set(wanted(activity("a.vmf", "Block")));
    let end = Instant::now() + Duration::from_secs(5);
    while p.link() != Link::NotRunning && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(p.link(), Link::NotRunning);
    srv.lock().unwrap().restart();
    assert!(wait_for(&srv, |s| s.activities().len() == 1), "connects once Discord starts");
    let end = Instant::now() + Duration::from_secs(5);
    while p.link() != Link::Connected && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(p.link(), Link::Connected);
    p.set(Wanted::default());
    assert_eq!(p.link(), Link::Idle);
}

#[test]
fn rejected_activity_shows_in_the_link() {
    let srv = server(true);
    srv.lock().unwrap().reject_activity = true;
    let mut s = Session::new(Box::new(MockConnector(srv.clone())), opts());
    let t0 = Instant::now();
    s.set(wanted(activity("a.vmf", "Block")));
    s.step(t0);
    assert_eq!(s.link(), &Link::Connected);
    s.step(t0 + Duration::from_secs(5));
    assert!(s.is_connected());
    assert_eq!(s.link(), &Link::Refused("child \"activity\" fails (4000)".into()));
    srv.lock().unwrap().reject_activity = false;
    s.set(wanted(activity("b.vmf", "Block")));
    s.step(t0 + Duration::from_secs(6));
    s.step(t0 + Duration::from_secs(11));
    assert_eq!(s.link(), &Link::Connected);
}
