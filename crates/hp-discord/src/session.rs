use std::io;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::activity::{Activity, handshake_json, set_activity_json};
use crate::frame::{self, Opcode};
use crate::transport::{Conn, Connector};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wanted {
    pub client_id: String,
    pub activity: Option<Activity>,
}

impl Wanted {
    pub fn is_idle(&self) -> bool {
        self.client_id.is_empty() || self.activity.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Link {
    #[default]
    Idle,
    Connecting,
    NotRunning,
    Connected,
    Refused(String),
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub throttle: Duration,
    pub retry_min: Duration,
    pub retry_max: Duration,
    pub handshake_timeout: Duration,
    pub poll: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            throttle: Duration::from_secs(15),
            retry_min: Duration::from_secs(5),
            retry_max: Duration::from_secs(20),
            handshake_timeout: Duration::from_secs(3),
            poll: Duration::from_secs(5),
        }
    }
}

pub struct Session {
    connector: Box<dyn Connector>,
    opts: Options,
    conn: Option<Box<dyn Conn>>,
    conn_id: String,
    inbox: Vec<u8>,
    wanted: Wanted,
    sent: Option<Activity>,
    sent_at: Option<Instant>,
    retry_at: Option<Instant>,
    backoff: Duration,
    nonce: u64,
    pid: u32,
    link: Link,
}

impl Session {
    pub fn new(connector: Box<dyn Connector>, opts: Options) -> Self {
        Self {
            connector,
            opts,
            conn: None,
            conn_id: String::new(),
            inbox: Vec::new(),
            wanted: Wanted::default(),
            sent: None,
            sent_at: None,
            retry_at: None,
            backoff: opts.retry_min,
            nonce: 0,
            pid: std::process::id(),
            link: Link::Idle,
        }
    }

    pub fn set(&mut self, wanted: Wanted) {
        self.wanted = wanted;
    }

    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    pub fn sent(&self) -> Option<&Activity> {
        self.sent.as_ref()
    }

    pub fn link(&self) -> &Link {
        &self.link
    }

    pub fn step(&mut self, now: Instant) -> Option<Instant> {
        if self.conn.is_some()
            && let Err(e) = self.drain()
        {
            log::debug!("discord: connection lost: {e}");
            self.lost(now);
        }
        if self.wanted.is_idle() {
            self.close();
            self.backoff = self.opts.retry_min;
            self.retry_at = None;
            self.link = Link::Idle;
            return None;
        }
        if self.conn.is_some() && self.conn_id != self.wanted.client_id {
            self.close();
            self.retry_at = None;
        }
        if self.conn.is_none() {
            if let Some(at) = self.retry_at.filter(|at| now < *at) {
                return Some(at);
            }
            if self.link == Link::Idle {
                self.link = Link::Connecting;
            }
            if let Err(e) = self.open() {
                log::debug!("discord: not connected: {e}");
                self.conn = None;
                self.link = match e.kind() {
                    io::ErrorKind::PermissionDenied => Link::Refused(e.to_string()),
                    _ => Link::NotRunning,
                };
                let at = now + self.backoff;
                self.retry_at = Some(at);
                self.backoff = (self.backoff * 2).min(self.opts.retry_max);
                return Some(at);
            }
            self.backoff = self.opts.retry_min;
            self.retry_at = None;
            self.link = Link::Connected;
        }
        let activity = self.wanted.activity.clone()?;
        if self.sent.as_ref() != Some(&activity) {
            if let (Some(before), Some(at)) = (&self.sent, self.sent_at) {
                let free_at = at + self.opts.throttle;
                if !activity.is_new_map(before) && now < free_at {
                    return Some(free_at.min(now + self.opts.poll));
                }
            }
            if let Err(e) = self.send_activity(Some(&activity)) {
                log::debug!("discord: update failed: {e}");
                self.lost(now);
                return self.retry_at;
            }
            self.sent = Some(activity);
            self.sent_at = Some(now);
        }
        Some(now + self.opts.poll)
    }

    pub fn close(&mut self) {
        if self.conn.is_some() {
            let _ = self.send_activity(None);
            if let Some(conn) = self.conn.as_mut() {
                let _ = conn.write_all(&frame::encode(Opcode::Close, "{}"));
            }
        }
        self.conn = None;
        self.sent = None;
        self.sent_at = None;
        self.inbox.clear();
    }

    fn lost(&mut self, now: Instant) {
        self.conn = None;
        self.link = Link::NotRunning;
        self.sent = None;
        self.sent_at = None;
        self.inbox.clear();
        self.retry_at = Some(now + self.opts.retry_min);
    }

    fn open(&mut self) -> io::Result<()> {
        self.inbox.clear();
        let mut conn = self.connector.connect()?;
        conn.write_all(&frame::encode(Opcode::Handshake, &handshake_json(&self.wanted.client_id)))?;
        self.conn = Some(conn);
        self.conn_id = self.wanted.client_id.clone();
        let deadline = Instant::now() + self.opts.handshake_timeout;
        loop {
            for f in self.read_frames()? {
                match f.op {
                    Opcode::Frame if f.payload.contains("\"READY\"") => return Ok(()),
                    Opcode::Close => return Err(io::Error::new(io::ErrorKind::PermissionDenied, refusal(&f.payload))),
                    _ => {}
                }
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn drain(&mut self) -> io::Result<()> {
        for f in self.read_frames()? {
            match f.op {
                Opcode::Ping => self.write(Opcode::Pong, &f.payload)?,
                Opcode::Close => return Err(io::Error::other(format!("closed by Discord: {}", f.payload))),
                Opcode::Frame => self.answer(&f.payload),
                _ => {}
            }
        }
        Ok(())
    }

    fn answer(&mut self, payload: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else { return };
        if v.get("evt").and_then(|e| e.as_str()) == Some("ERROR") {
            log::debug!("discord: {payload}");
            let data = v.get("data").map(|d| d.to_string()).unwrap_or_default();
            self.link = Link::Refused(refusal(&data));
        } else if v.get("cmd").and_then(|c| c.as_str()) == Some("SET_ACTIVITY") {
            self.link = Link::Connected;
        }
    }

    fn read_frames(&mut self) -> io::Result<Vec<frame::Frame>> {
        let Some(conn) = self.conn.as_mut() else {
            return Err(io::ErrorKind::NotConnected.into());
        };
        let mut buf = [0u8; 4096];
        loop {
            let n = conn.read_ready(&mut buf)?;
            if n == 0 {
                break;
            }
            self.inbox.extend_from_slice(&buf[..n]);
        }
        let mut frames = Vec::new();
        while let Some((f, used)) = frame::decode(&self.inbox).map_err(io::Error::other)? {
            self.inbox.drain(..used);
            frames.push(f);
        }
        Ok(frames)
    }

    fn write(&mut self, op: Opcode, payload: &str) -> io::Result<()> {
        match self.conn.as_mut() {
            Some(conn) => conn.write_all(&frame::encode(op, payload)),
            None => Err(io::ErrorKind::NotConnected.into()),
        }
    }

    fn send_activity(&mut self, activity: Option<&Activity>) -> io::Result<()> {
        self.nonce += 1;
        let payload = set_activity_json(self.pid, activity, &self.nonce.to_string());
        self.write(Opcode::Frame, &payload)
    }
}

fn refusal(payload: &str) -> String {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| {
            let message = v.get("message").and_then(|m| m.as_str())?.to_owned();
            Some(match v.get("code").and_then(|c| c.as_i64()) {
                Some(code) => format!("{message} ({code})"),
                None => message,
            })
        })
        .unwrap_or_else(|| payload.trim().to_owned())
}

enum Msg {
    Set(Box<Wanted>),
    Stop,
}

pub struct Presence {
    pending: Option<(Box<dyn Connector>, Options)>,
    tx: Option<mpsc::Sender<Msg>>,
    done: Option<mpsc::Receiver<()>>,
    last: Wanted,
    link: Arc<Mutex<Link>>,
}

impl Presence {
    pub fn discord() -> Self {
        Self::new(Box::new(crate::transport::SystemConnector::discord()), Options::default())
    }

    pub fn new(connector: Box<dyn Connector>, opts: Options) -> Self {
        Self {
            pending: Some((connector, opts)),
            tx: None,
            done: None,
            last: Wanted::default(),
            link: Arc::new(Mutex::new(Link::Idle)),
        }
    }

    pub fn link(&self) -> Link {
        self.link.lock().map(|l| l.clone()).unwrap_or_default()
    }

    pub fn set(&mut self, wanted: Wanted) {
        if wanted == self.last {
            return;
        }
        self.last = wanted.clone();
        if wanted.is_idle()
            && let Ok(mut l) = self.link.lock()
        {
            *l = Link::Idle;
        }
        if self.tx.is_none() {
            if wanted.is_idle() {
                return;
            }
            self.spawn();
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Set(Box::new(wanted)));
        }
    }

    fn spawn(&mut self) {
        let Some((connector, opts)) = self.pending.take() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let link = self.link.clone();
        if let Ok(mut l) = link.lock() {
            *l = Link::Connecting;
        }
        let spawned = std::thread::Builder::new().name("hp-discord".into()).spawn(move || {
            let mut session = Session::new(connector, opts);
            let mut next: Option<Instant> = None;
            loop {
                let msg = match next {
                    None => rx.recv().unwrap_or(Msg::Stop),
                    Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                        Ok(m) => m,
                        Err(RecvTimeoutError::Timeout) => {
                            next = session.step(Instant::now());
                            publish(&link, &session);
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => Msg::Stop,
                    },
                };
                match msg {
                    Msg::Set(w) => session.set(*w),
                    Msg::Stop => {
                        session.close();
                        let _ = done_tx.send(());
                        return;
                    }
                }
                next = session.step(Instant::now());
                publish(&link, &session);
            }
        });
        if spawned.is_ok() {
            self.tx = Some(tx);
            self.done = Some(done_rx);
        }
    }
}

fn publish(link: &Mutex<Link>, session: &Session) {
    if let Ok(mut l) = link.lock()
        && *l != *session.link()
    {
        *l = session.link().clone();
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Msg::Stop);
            if let Some(done) = self.done.take() {
                let _ = done.recv_timeout(Duration::from_millis(500));
            }
        }
    }
}
