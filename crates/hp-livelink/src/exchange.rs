use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant, SystemTime};

use crate::protocol::{
    COMMAND_FILE, CommandFile, ENTITIES_FILE, GAME_FILE, GameEntities, GameReport, MAX_FILE, Op, Queued, SESSION_FILE,
    SessionFile, VERSION, parse_entities, parse_report,
};

pub const POLL: Duration = Duration::from_millis(30);
pub const BEAT: Duration = Duration::from_millis(1000);
pub const MAX_QUEUE: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Queue {
    next: u64,
    items: Vec<Queued>,
}

fn merge(old: &Op, new: Op) -> Op {
    match (old, new) {
        (Op::Set { kv: old_kv, .. }, Op::Set { key, target, kv }) => {
            let mut merged = old_kv.clone();
            merged.extend(kv);
            Op::Set { key, target, kv: merged }
        }
        (_, new) => new,
    }
}

impl Queue {
    pub fn push(&mut self, op: Op) -> u64 {
        let mut op = op;
        let same_target = |old: &Op, new: &Op| match (old, new) {
            (Op::Set { target: a, .. }, Op::Set { target: b, .. }) => a == b,
            _ => true,
        };
        if let Some(slot) = op.slot()
            && let Some(pos) =
                self.items.iter().position(|q| q.op.slot().as_ref() == Some(&slot) && same_target(&q.op, &op))
        {
            let old = self.items.remove(pos);
            op = merge(&old.op, op);
        }
        self.next += 1;
        self.items.push(Queued { seq: self.next, op });
        if self.items.len() > MAX_QUEUE {
            let extra = self.items.len() - MAX_QUEUE;
            self.items.drain(..extra);
        }
        self.next
    }

    pub fn ack(&mut self, seq: u64) -> usize {
        let before = self.items.len();
        self.items.retain(|q| q.seq > seq);
        before - self.items.len()
    }

    pub fn drop_map_ops(&mut self) -> usize {
        let before = self.items.len();
        self.items.retain(|q| !q.op.needs_map());
        before - self.items.len()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn items(&self) -> &[Queued] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn last_seq(&self) -> u64 {
        self.next
    }
}

pub fn unix_secs() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for salt in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u128(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("no folder"))?;
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    let mut last = None;
    for _ in 0..20 {
        match std::fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(3));
            }
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

fn read_small(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_FILE {
        return None;
    }
    std::fs::read(path).ok()
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

#[derive(Clone, Debug)]
pub enum ToWorker {
    Ops(Vec<Op>),
    Load(String),
    Follow(bool),
    Labels(BTreeMap<String, String>),
    Stop,
}

#[derive(Clone, Debug)]
pub enum FromWorker {
    Report { report: GameReport, latency: Option<Duration>, pending: usize },
    Entities(GameEntities),
    WriteFailed(String),
    Stopped,
}

pub struct Exchange {
    pub dir: PathBuf,
    pub token: String,
    tx: Sender<ToWorker>,
    rx: Receiver<FromWorker>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for Exchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Exchange").field("dir", &self.dir).finish_non_exhaustive()
    }
}

struct Worker {
    dir: PathBuf,
    token: String,
    queue: Queue,
    sent: BTreeMap<u64, Instant>,
    load: String,
    follow: bool,
    labels: BTreeMap<String, String>,
    last_beat: Option<Instant>,
    game_stamp: Option<(SystemTime, u64)>,
    ents_stamp: Option<(SystemTime, u64)>,
    dirty: bool,
    out: Sender<FromWorker>,
    wake: Box<dyn Fn() + Send>,
}

impl Worker {
    fn write_session(&mut self) {
        let file = SessionFile {
            v: VERSION,
            token: &self.token,
            beat: unix_secs(),
            follow: self.follow,
            labels: &self.labels,
        };
        if let Ok(bytes) = serde_json::to_vec(&file)
            && let Err(e) = write_atomic(&self.dir.join(SESSION_FILE), &bytes)
        {
            let _ = self.out.send(FromWorker::WriteFailed(e.to_string()));
            (self.wake)();
        }
        self.last_beat = Some(Instant::now());
    }

    fn write_commands(&mut self) {
        let file = CommandFile { v: VERSION, token: &self.token, load: &self.load, ops: self.queue.items() };
        match serde_json::to_vec(&file) {
            Ok(bytes) => match write_atomic(&self.dir.join(COMMAND_FILE), &bytes) {
                Ok(()) => self.dirty = false,
                Err(e) => {
                    let _ = self.out.send(FromWorker::WriteFailed(e.to_string()));
                    (self.wake)();
                }
            },
            Err(e) => {
                let _ = self.out.send(FromWorker::WriteFailed(e.to_string()));
            }
        }
    }

    fn take(&mut self, msg: ToWorker) -> bool {
        match msg {
            ToWorker::Ops(ops) => {
                let now = Instant::now();
                for op in ops {
                    let seq = self.queue.push(op);
                    self.sent.insert(seq, now);
                }
                self.dirty = true;
            }
            ToWorker::Load(load) => {
                if load != self.load {
                    self.load = load;
                    self.queue.drop_map_ops();
                    self.dirty = true;
                }
            }
            ToWorker::Follow(on) => {
                self.follow = on;
                self.write_session();
            }
            ToWorker::Labels(labels) => {
                self.labels = labels;
                self.write_session();
            }
            ToWorker::Stop => return false,
        }
        true
    }

    fn poll_game(&mut self) {
        let path = self.dir.join(GAME_FILE);
        let now = stamp(&path);
        if now.is_some() && now != self.game_stamp {
            self.game_stamp = now;
            if let Some(report) = read_small(&path).and_then(|b| parse_report(&b, &self.token)) {
                let latency = self.sent.range(..=report.ack).next_back().map(|(_, t)| t.elapsed());
                if self.queue.ack(report.ack) > 0 {
                    self.dirty = true;
                }
                self.sent = self.sent.split_off(&(report.ack + 1));
                let pending = self.queue.len();
                let _ = self.out.send(FromWorker::Report { report, latency, pending });
                (self.wake)();
            }
        }
        let path = self.dir.join(ENTITIES_FILE);
        let now = stamp(&path);
        if now.is_some() && now != self.ents_stamp {
            self.ents_stamp = now;
            if let Some(ents) = read_small(&path).and_then(|b| parse_entities(&b, &self.token)) {
                let _ = self.out.send(FromWorker::Entities(ents));
                (self.wake)();
            }
        }
    }

    fn run(mut self, rx: Receiver<ToWorker>) {
        self.write_session();
        self.write_commands();
        loop {
            match rx.recv_timeout(POLL) {
                Ok(msg) => {
                    if !self.take(msg) {
                        break;
                    }
                    let mut alive = true;
                    while let Ok(more) = rx.try_recv() {
                        if !self.take(more) {
                            alive = false;
                            break;
                        }
                    }
                    if !alive {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if self.dirty {
                self.write_commands();
            }
            if self.last_beat.is_none_or(|t| t.elapsed() >= BEAT) {
                self.write_session();
            }
            self.poll_game();
        }
        self.finish();
    }

    fn finish(&mut self) {
        self.queue.clear();
        self.queue.push(Op::Reset);
        self.write_commands();
        let deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < deadline {
            std::thread::sleep(POLL);
            let path = self.dir.join(GAME_FILE);
            if let Some(report) = read_small(&path).and_then(|b| parse_report(&b, &self.token))
                && report.ack >= self.queue.last_seq()
            {
                break;
            }
        }
        for f in [SESSION_FILE, COMMAND_FILE] {
            let _ = std::fs::remove_file(self.dir.join(f));
        }
        let _ = self.out.send(FromWorker::Stopped);
        (self.wake)();
    }
}

impl Exchange {
    pub fn start(
        dir: PathBuf,
        labels: BTreeMap<String, String>,
        follow: bool,
        wake: Box<dyn Fn() + Send>,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        for f in [GAME_FILE, ENTITIES_FILE, COMMAND_FILE] {
            let _ = std::fs::remove_file(dir.join(f));
        }
        let token = new_token();
        let (tx, worker_rx) = channel();
        let (worker_tx, rx) = channel();
        let worker = Worker {
            dir: dir.clone(),
            token: token.clone(),
            queue: Queue::default(),
            sent: BTreeMap::new(),
            load: String::new(),
            follow,
            labels,
            last_beat: None,
            game_stamp: None,
            ents_stamp: None,
            dirty: true,
            out: worker_tx,
            wake,
        };
        let handle = std::thread::Builder::new().name("hp-livelink".into()).spawn(move || worker.run(worker_rx))?;
        Ok(Self { dir, token, tx, rx, handle: Some(handle) })
    }

    pub fn send(&self, msg: ToWorker) {
        let _ = self.tx.send(msg);
    }

    pub fn push(&self, ops: Vec<Op>) {
        if !ops.is_empty() {
            self.send(ToWorker::Ops(ops));
        }
    }

    pub fn poll(&self) -> Vec<FromWorker> {
        self.rx.try_iter().collect()
    }

    pub fn stop(mut self) {
        let _ = self.tx.send(ToWorker::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Exchange {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = self.tx.send(ToWorker::Stop);
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Target;

    fn set(key: u32, k: &str, v: &str) -> Op {
        let mut kv = BTreeMap::new();
        kv.insert(k.to_owned(), v.to_owned());
        Op::Set { key, target: Target::Map(key + 100), kv }
    }

    #[test]
    fn queue_coalesces_by_slot_and_keeps_order() {
        let mut q = Queue::default();
        q.push(set(1, "origin", "0 0 0"));
        q.push(Op::Fire {
            name: Some("d".into()),
            target: None,
            input: "Open".into(),
            param: String::new(),
            delay: 0.0,
        });
        q.push(set(1, "angles", "0 90 0"));
        assert_eq!(q.len(), 2);
        assert_eq!(q.items()[0].op.kind(), "fire");
        match &q.items()[1].op {
            Op::Set { kv, .. } => assert_eq!(kv.len(), 2, "keys of both edits are kept"),
            other => panic!("{other:?}"),
        }
        assert_eq!(q.items()[1].seq, 3);
        q.push(Op::Teleport { origin: [0.0; 3], angles: [0.0; 3], noclip: true });
        q.push(Op::Teleport { origin: [1.0; 3], angles: [0.0; 3], noclip: true });
        assert_eq!(q.len(), 3);
        assert_eq!(q.ack(3), 2);
        assert_eq!(q.len(), 1);
        assert_eq!(q.last_seq(), 5);
        q.push(Op::Io { key: 1, target: Target::Map(4), add: vec![] });
        q.push(Op::Remove { key: 2, target: Target::Own(2) });
        assert_eq!(q.drop_map_ops(), 1);
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn tokens_are_long_and_different() {
        let a = new_token();
        assert_eq!(a.len(), 32);
        assert_ne!(a, new_token());
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_livelink_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn wait_for(ex: &Exchange, mut f: impl FnMut(&FromWorker) -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            for m in ex.poll() {
                if f(&m) {
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn the_worker_writes_session_and_commands_and_reads_acks() {
        let dir = scratch("worker");
        let mut labels = BTreeMap::new();
        labels.insert("preview".to_owned(), "Preview".to_owned());
        let ex = Exchange::start(dir.clone(), labels, false, Box::new(|| {})).expect("start");
        let end = Instant::now() + Duration::from_secs(5);
        while !dir.join(SESSION_FILE).is_file() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
        let session: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(SESSION_FILE)).expect("session")).expect("json");
        assert_eq!(session["token"], ex.token.as_str());
        assert_eq!(session["labels"]["preview"], "Preview");
        ex.send(ToWorker::Load("gm_x:1".into()));
        ex.push(vec![set(1, "origin", "1 2 3"), Op::Entities]);
        let mut cmd = serde_json::Value::Null;
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if let Ok(bytes) = std::fs::read(dir.join(COMMAND_FILE))
                && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && v["ops"].as_array().is_some_and(|a| a.len() == 2)
            {
                cmd = v;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(cmd["load"], "gm_x:1");
        assert_eq!(cmd["ops"][0]["op"], "set");
        let report = serde_json::json!({ "token": ex.token, "ack": 2, "load": "gm_x:1", "map": "gm_x" });
        write_atomic(&dir.join(GAME_FILE), &serde_json::to_vec(&report).unwrap()).unwrap();
        let got = wait_for(
            &ex,
            |m| matches!(m, FromWorker::Report { report, latency: Some(_), pending: 0 } if report.ack == 2),
        );
        assert!(got, "the ack arrives with a latency");
        let wrong = serde_json::json!({ "token": "nope", "load": "x", "ents": [] });
        write_atomic(&dir.join(ENTITIES_FILE), &serde_json::to_vec(&wrong).unwrap()).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert!(!ex.poll().iter().any(|m| matches!(m, FromWorker::Entities(_))), "other tokens are ignored");
        let good = serde_json::json!({ "token": ex.token, "load": "gm_x:1", "rev": 1, "ents": [{ "i": 3, "c": "prop_physics", "o": [0, 0, 0] }] });
        write_atomic(&dir.join(ENTITIES_FILE), &serde_json::to_vec(&good).unwrap()).unwrap();
        assert!(wait_for(&ex, |m| matches!(m, FromWorker::Entities(e) if e.ents.len() == 1)));
        ex.stop();
        assert!(!dir.join(SESSION_FILE).exists(), "stopping ends the session");
        assert!(!dir.join(COMMAND_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
