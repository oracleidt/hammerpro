use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::addon;
use crate::diff::{Planner, Reason, needs_compile};
use crate::exchange::{Exchange, FromWorker, ToWorker};
use crate::matching::{Matches, match_entities};
use crate::protocol::{GameEntities, Op};
use crate::snapshot::Snapshot;

pub const CONNECTED_FOR: Duration = Duration::from_secs(3);
pub const MAX_ERRORS: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    WaitingForGame,
    Connected,
    OtherMap,
    Lost,
}

impl LinkState {
    pub fn id(self) -> &'static str {
        match self {
            LinkState::WaitingForGame => "waiting",
            LinkState::Connected => "connected",
            LinkState::OtherMap => "other_map",
            LinkState::Lost => "lost",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub game_map: String,
    pub load: String,
    pub addon: String,
    pub latency: Option<Duration>,
    pub latencies: VecDeque<Duration>,
    pub pending: usize,
    pub sent: u64,
    pub errors: VecDeque<String>,
    pub skipped: BTreeMap<u32, String>,
    pub needs_compile: BTreeSet<Reason>,
    pub changed: usize,
    pub last_report: Option<Instant>,
    pub ever_connected: bool,
    pub game_entities: usize,
    pub created: u32,
    pub ghosts: u32,
    pub lights: u32,
    pub eye: Option<([f64; 3], [f64; 3])>,
    pub write_error: Option<String>,
    pub errors_seen_seq: u64,
}

impl Status {
    pub fn average_latency(&self) -> Option<Duration> {
        if self.latencies.is_empty() {
            return None;
        }
        let total: Duration = self.latencies.iter().sum();
        Some(total / u32::try_from(self.latencies.len()).unwrap_or(1))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Connected { map: String },
    NewLoad { map: String, rebased: bool },
    Matched { matched: usize, missing: usize },
    Eye { origin: [f64; 3], angles: [f64; 3] },
    Error(String),
    Stopped,
}

#[derive(Debug)]
pub struct Session {
    pub root: PathBuf,
    pub map: String,
    pub started: Instant,
    pub baseline: Snapshot,
    pub baseline_time: SystemTime,
    pub compile_snapshot: Option<(Snapshot, SystemTime)>,
    pub planner: Planner,
    pub matches: Option<Matches>,
    pub status: Status,
    pub injected_at: Option<Instant>,
    pub follow: bool,
    pub previews: bool,
    pending_ents: Option<GameEntities>,
    exchange: Option<Exchange>,
}

pub fn bsp_path(root: &Path, map: &str) -> PathBuf {
    root.join("garrysmod").join("maps").join(format!("{map}.bsp"))
}

impl Session {
    pub fn start(
        root: PathBuf,
        map: String,
        current: Snapshot,
        labels: BTreeMap<String, String>,
        follow: bool,
        wake: Box<dyn Fn() + Send>,
    ) -> std::io::Result<Self> {
        addon::install(&root)?;
        let exchange = Exchange::start(addon::data_dir(&root), labels, follow, wake)?;
        Ok(Self {
            root,
            map,
            started: Instant::now(),
            baseline: current,
            baseline_time: SystemTime::now(),
            compile_snapshot: None,
            planner: Planner::default(),
            matches: None,
            status: Status::default(),
            injected_at: None,
            follow,
            previews: true,
            pending_ents: None,
            exchange: Some(exchange),
        })
    }

    pub fn token(&self) -> &str {
        self.exchange.as_ref().map(|e| e.token.as_str()).unwrap_or("")
    }

    pub fn state(&self) -> LinkState {
        let fresh = self.status.last_report.is_some_and(|t| t.elapsed() < CONNECTED_FOR);
        if !fresh {
            return if self.status.ever_connected { LinkState::Lost } else { LinkState::WaitingForGame };
        }
        if !self.status.game_map.eq_ignore_ascii_case(&self.map) {
            return LinkState::OtherMap;
        }
        LinkState::Connected
    }

    pub fn is_connected(&self) -> bool {
        self.state() == LinkState::Connected
    }

    pub fn send(&self, msg: ToWorker) {
        if let Some(e) = &self.exchange {
            e.send(msg);
        }
    }

    pub fn push(&mut self, ops: Vec<Op>) {
        self.status.sent += ops.len() as u64;
        if let Some(e) = &self.exchange {
            e.push(ops);
        }
    }

    pub fn note_compile_started(&mut self, snapshot: Snapshot) {
        self.compile_snapshot = Some((snapshot, SystemTime::now()));
    }

    pub fn mark_compiled(&mut self, snapshot: Snapshot) {
        self.baseline = snapshot;
        self.baseline_time = SystemTime::now();
        self.planner.reset();
        self.matches = None;
        self.push(vec![Op::Reset, Op::Entities]);
    }

    pub fn resync(&mut self) {
        self.planner.reset();
        self.matches = None;
        self.push(vec![Op::Reset, Op::Entities]);
    }

    fn error(&mut self, text: String) {
        self.status.errors.push_back(text);
        while self.status.errors.len() > MAX_ERRORS {
            self.status.errors.pop_front();
        }
    }

    fn rebase_for_new_build(&mut self) -> bool {
        let built = std::fs::metadata(bsp_path(&self.root, &self.map)).and_then(|m| m.modified()).ok();
        let Some(built) = built else { return false };
        match self.compile_snapshot.take() {
            Some((snap, t)) if built >= t => {
                self.baseline = snap;
                self.baseline_time = t;
                true
            }
            other => {
                self.compile_snapshot = other;
                false
            }
        }
    }

    fn on_entities(&mut self, ents: GameEntities, current: &Snapshot, out: &mut Vec<Event>) {
        if ents.load != self.status.load {
            self.pending_ents = Some(ents);
            return;
        }
        self.pending_ents = None;
        let m = match_entities(&self.baseline, &ents.ents);
        out.push(Event::Matched { matched: m.by_id.len(), missing: m.missing.len() });
        self.status.game_entities = ents.ents.len();
        self.matches = Some(m);
        self.planner.reset();
        self.update(current);
    }

    pub fn handle(&mut self, current: &Snapshot) -> Vec<Event> {
        let msgs = self.exchange.as_ref().map(Exchange::poll).unwrap_or_default();
        let mut out = Vec::new();
        for msg in msgs {
            match msg {
                FromWorker::Report { report, latency, pending } => {
                    let first = !self.status.ever_connected;
                    self.status.ever_connected = true;
                    self.status.last_report = Some(Instant::now());
                    self.status.pending = pending;
                    self.status.write_error = None;
                    if let Some(l) = latency {
                        self.status.latency = Some(l);
                        self.status.latencies.push_back(l);
                        while self.status.latencies.len() > 20 {
                            self.status.latencies.pop_front();
                        }
                    }
                    for r in &report.results {
                        if !r.ok && r.seq > self.status.errors_seen() {
                            self.error(r.msg.clone());
                            out.push(Event::Error(r.msg.clone()));
                        }
                    }
                    if let Some(max) = report.results.iter().map(|r| r.seq).max() {
                        self.status.set_errors_seen(max);
                    }
                    self.status.game_map = report.map.clone();
                    self.status.addon = report.addon.clone();
                    self.status.created = report.created;
                    self.status.ghosts = report.ghosts;
                    self.status.lights = report.lights;
                    if let Some((origin, angles)) = report.eye
                        && self.status.eye != report.eye
                    {
                        self.status.eye = report.eye;
                        out.push(Event::Eye { origin, angles });
                    }
                    if report.load != self.status.load {
                        self.status.load = report.load.clone();
                        self.send(ToWorker::Load(report.load.clone()));
                        self.planner.reset();
                        self.matches = None;
                        let rebased = self.rebase_for_new_build();
                        if first {
                            out.push(Event::Connected { map: report.map.clone() });
                        } else {
                            out.push(Event::NewLoad { map: report.map.clone(), rebased });
                        }
                        if let Some(ents) = self.pending_ents.take() {
                            self.on_entities(ents, current, &mut out);
                        }
                    }
                }
                FromWorker::Entities(ents) => self.on_entities(ents, current, &mut out),
                FromWorker::WriteFailed(e) => {
                    if self.status.write_error.as_deref() != Some(e.as_str()) {
                        self.error(e.clone());
                        out.push(Event::Error(e.clone()));
                    }
                    self.status.write_error = Some(e);
                }
                FromWorker::Stopped => out.push(Event::Stopped),
            }
        }
        out
    }

    pub fn update(&mut self, current: &Snapshot) {
        self.status.needs_compile = needs_compile(&self.baseline, current, self.matches.as_ref());
        self.status.changed = self.baseline.changed_entities(current);
        if !self.is_connected() {
            return;
        }
        let Some(matches) = self.matches.as_ref() else { return };
        let mut plan = self.planner.plan(&self.baseline, current, matches);
        if !self.previews {
            plan.ops.retain(|op| !matches!(op, Op::Light { light: Some(_), .. }));
        }
        for (id, why) in plan.skipped {
            self.status.skipped.insert(id, why);
        }
        self.status.skipped.retain(|id, _| current.ents.get(id) != self.baseline.ents.get(id));
        self.push(plan.ops);
    }

    pub fn stop(mut self) {
        if let Some(e) = self.exchange.take() {
            e.stop();
        }
    }
}

impl Status {
    fn errors_seen(&self) -> u64 {
        self.errors_seen_seq
    }

    fn set_errors_seen(&mut self, seq: u64) {
        self.errors_seen_seq = self.errors_seen_seq.max(seq);
    }
}
