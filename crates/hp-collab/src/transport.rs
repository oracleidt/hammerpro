use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use anyhow::{Result, ensure};
use tokio::sync::{mpsc as channel, watch};
use zeroize::Zeroizing;

use crate::document::{Change, MAX_MAP_BYTES, Snapshot, ValidatedSnapshot};
use crate::moderation::{AccessPolicy, AccessState, BanRow, MAX_BAN_SECONDS};
use crate::protocol::{
    self, ConnectionMode, Invitation, MAX_REASON_CHARS, Outcome, Participant, Presence, Refused, Rejection, Role, Room,
};

mod frame;
mod guest;
mod handshake;
mod host;
mod queue;

pub(crate) const MAX_FRAME: usize = MAX_MAP_BYTES + 16 * 1024 * 1024;
pub(crate) const DEADLINE: Duration = Duration::from_secs(30);
pub(crate) const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(15);
pub(crate) const INITIAL_JOIN_DEADLINE: Duration = Duration::from_secs(300);
pub(crate) const APPROVAL_DEADLINE: Duration = Duration::from_secs(120);
pub(crate) const EDITOR_BACKLOG_EVENTS: usize = 4096;
pub(crate) const EDITOR_BACKLOG_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const WRITE_STALL: Duration = Duration::from_secs(600);

pub type Waker = Arc<dyn Fn() + Send + Sync>;
pub type SnapshotResult = std::result::Result<Vec<crate::snapshots::SnapshotRow>, String>;

pub enum Command {
    Restore { map: Snapshot, store: crate::snapshots::SnapshotStore },
    Snapshot { store: crate::snapshots::SnapshotStore, done: mpsc::SyncSender<SnapshotResult> },
    Edit { sequence: u64, changes: Vec<Change> },
    Presence(Presence),
    Rotate(Zeroizing<String>),
    Kick(u32),
    Ban { participant: u32, seconds: Option<u64>, reason: String },
    Unban(u64),
    SetRole { participant: u32, role: Role },
    Admit(u32),
    Decline(u32),
    Policy(AccessPolicy),
    Stop,
}

pub enum Event {
    Reset {
        revision: u64,
        map: ValidatedSnapshot,
    },
    Ready {
        invitation: Option<String>,
        participant: u32,
        id_start: u32,
        revision: u64,
        map: ValidatedSnapshot,
        direct_addresses: Vec<std::net::SocketAddr>,
    },
    Applied {
        author: u32,
        sequence: u64,
        revision: u64,
        changes: Vec<Change>,
    },
    Rejected {
        sequence: u64,
        rejection: Rejection,
        revision: u64,
    },
    Resync {
        revision: u64,
        outcomes: Vec<Outcome>,
        map: ValidatedSnapshot,
    },
    Participants(Vec<Participant>),
    Bans(Vec<BanRow>),
    Access(AccessState),
    Invitation(String),
    Traffic {
        sent: u64,
        received: u64,
    },
    NetworkPrivacy {
        relay_only: bool,
        published_ip_count: usize,
        direct_path_count: usize,
    },
    Waiting,
    Denied(Refused),
    Error(String),
    Closed,
    JoinProgress {
        units: u64,
        total: u64,
        bytes: u64,
    },
}

pub struct Session {
    commands: channel::UnboundedSender<Command>,
    presence: watch::Sender<Option<Presence>>,
    pub events: SessionEvents,
    waker: Arc<OnceLock<Waker>>,
}

pub(crate) struct Commands {
    reliable: channel::UnboundedReceiver<Command>,
    presence: watch::Receiver<Option<Presence>>,
    presence_open: bool,
}

impl Commands {
    pub(crate) async fn next(&mut self) -> Option<Command> {
        loop {
            tokio::select! {
                biased;
                command = self.reliable.recv() => return command,
                changed = self.presence.changed(), if self.presence_open => {
                    if changed.is_err() {
                        self.presence_open = false;
                    } else if let Some(presence) = self.presence.borrow_and_update().clone() {
                        return Some(Command::Presence(presence));
                    }
                }
            }
        }
    }
}

pub(crate) fn command_channel() -> (channel::UnboundedSender<Command>, watch::Sender<Option<Presence>>, Commands) {
    let (commands, reliable) = channel::unbounded_channel();
    let (presence, presence_updates) = watch::channel(None);
    (commands, presence, Commands { reliable, presence: presence_updates, presence_open: true })
}

pub struct SessionEvents {
    reliable: mpsc::Receiver<(Event, usize)>,
    latest: Arc<Mutex<BTreeMap<u8, Event>>>,
    queued: Arc<AtomicUsize>,
    count: Arc<AtomicUsize>,
}

impl SessionEvents {
    pub fn try_iter(&self) -> impl Iterator<Item = Event> + '_ {
        std::iter::from_fn(|| self.try_recv().ok())
    }

    pub fn try_recv(&self) -> std::result::Result<Event, mpsc::TryRecvError> {
        match self.reliable.try_recv().map(|(event, bytes)| {
            self.queued.fetch_sub(bytes, Ordering::AcqRel);
            self.count.fetch_sub(1, Ordering::AcqRel);
            event
        }) {
            Ok(Event::Closed) => {
                if let Ok(mut latest) = self.latest.lock() {
                    latest.clear();
                }
                Ok(Event::Closed)
            }
            Ok(event) => Ok(event),
            Err(error) => {
                self.latest.lock().ok().and_then(|mut latest| latest.pop_first()).map(|(_, event)| event).ok_or(error)
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct Editor {
    reliable: mpsc::Sender<(Event, usize)>,
    latest: Arc<Mutex<BTreeMap<u8, Event>>>,
    queued: Arc<AtomicUsize>,
    count: Arc<AtomicUsize>,
    events: usize,
    bytes: usize,
    waker: Arc<OnceLock<Waker>>,
}

impl Editor {
    pub(crate) fn busy(&self) -> bool {
        self.count.load(Ordering::Acquire) >= self.events || self.queued.load(Ordering::Acquire) >= self.bytes
    }
}

fn queue_pair(events: usize, bytes: usize) -> (Editor, SessionEvents) {
    let (reliable, receiver) = mpsc::channel();
    let latest = Arc::new(Mutex::new(BTreeMap::new()));
    let queued = Arc::new(AtomicUsize::new(0));
    let count = Arc::new(AtomicUsize::new(0));
    let waker = Arc::new(OnceLock::new());
    (
        Editor { reliable, latest: latest.clone(), queued: queued.clone(), count: count.clone(), events, bytes, waker },
        SessionEvents { reliable: receiver, latest, queued, count },
    )
}

fn event_bytes(value: &Event) -> usize {
    match value {
        Event::Reset { map, .. } | Event::Ready { map, .. } | Event::Resync { map, .. } => map.sizes().unit_bytes,
        Event::Applied { changes, .. } => changes
            .iter()
            .map(|change| change.key.len() + change.after.as_ref().map_or(0, |unit| unit.vmf.len()) + 64)
            .sum(),
        _ => 64,
    }
}

pub struct HostOptions {
    pub room: Room,
    pub mode: ConnectionMode,
    pub port: u16,
    pub policy: AccessPolicy,
}

pub struct JoinOptions {
    pub identity: Zeroizing<[u8; 32]>,
    pub session_name: String,
    pub direct_address: Option<std::net::SocketAddr>,
}

fn valid_password(password: &str) -> bool {
    (8..=256).contains(&password.len())
}

impl Session {
    pub fn host(map: Snapshot, name: String, password: Zeroizing<String>, options: HostOptions) -> Result<Self> {
        Self::host_validated(ValidatedSnapshot::new(map)?, name, password, options)
    }

    pub fn host_validated(
        map: ValidatedSnapshot,
        name: String,
        password: Zeroizing<String>,
        options: HostOptions,
    ) -> Result<Self> {
        ensure!(protocol::valid_name(&name), "Invalid participant name");
        ensure!(valid_password(&password), "Use at least 8 password characters");
        options.room.validate()?;
        options.policy.validate()?;
        Self::spawn(move |rx, tx| Box::pin(host::run(map, name, password, options, rx, tx)))
    }

    pub fn join(
        invitation: Invitation,
        name: String,
        password: Zeroizing<String>,
        options: JoinOptions,
    ) -> Result<Self> {
        ensure!(protocol::valid_name(&name) && valid_password(&password), "Invalid participant details");
        ensure!(invitation.room.name == options.session_name, "Session name does not match the invitation");
        ensure!(
            invitation.mode != ConnectionMode::Direct || options.direct_address.is_some(),
            "Direct mode requires the host IP and port"
        );
        Self::spawn(move |rx, tx| Box::pin(guest::run(invitation, name, password, options, rx, tx)))
    }

    fn spawn<F>(run: F) -> Result<Self>
    where
        F: FnOnce(Commands, Editor) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
            + Send
            + 'static,
    {
        let (commands, presence, rx) = command_channel();
        let (tx, events) = queue_pair(EDITOR_BACKLOG_EVENTS, EDITOR_BACKLOG_BYTES);
        let waker = tx.waker.clone();
        std::thread::Builder::new().name("map-collaboration".into()).spawn(move || {
            match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
                Ok(runtime) => {
                    if let Err(error) = runtime.block_on(run(rx, tx.clone())) {
                        let _ = event(&tx, failure(error));
                    }
                }
                Err(error) => {
                    let _ = event(&tx, Event::Error(error.to_string()));
                }
            }
            let _ = event(&tx, Event::Closed);
        })?;
        Ok(Self { commands, presence, events, waker })
    }

    pub fn set_waker(&self, waker: Waker) {
        let _ = self.waker.set(waker);
    }

    pub fn send(&self, command: Command) -> Result<()> {
        match &command {
            Command::Rotate(password) => ensure!(valid_password(password), "Use at least 8 password characters"),
            Command::Ban { seconds, reason, .. } => ensure!(
                reason.chars().count() <= MAX_REASON_CHARS
                    && seconds.is_none_or(|seconds| (1..=MAX_BAN_SECONDS).contains(&seconds)),
                "Invalid ban duration or reason"
            ),
            Command::Policy(policy) => policy.validate()?,
            _ => {}
        }
        let sent = match command {
            Command::Presence(presence) => self.presence.send(Some(presence)).is_ok(),
            command => self.commands.send(command).is_ok(),
        };
        ensure!(sent, "Session disconnected");
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
    }
}

fn failure(error: anyhow::Error) -> Event {
    match error.downcast::<Refused>() {
        Ok(refused) => Event::Denied(refused.sanitized()),
        Err(error) => Event::Error(protocol::sanitize_limit(&format!("{error:#}"), 1024)),
    }
}

pub(crate) fn event(tx: &Editor, value: Event) -> Result<()> {
    let key = match &value {
        Event::Participants(_) => Some(0),
        Event::Traffic { .. } => Some(1),
        Event::Bans(_) => Some(2),
        Event::NetworkPrivacy { .. } => Some(3),
        Event::Access(_) => Some(4),
        Event::JoinProgress { .. } => Some(5),
        _ => None,
    };
    let result = if let Some(key) = key {
        tx.latest.lock().map_err(|_| anyhow::anyhow!("Session updates unavailable"))?.insert(key, value);
        Ok(())
    } else {
        let bytes = event_bytes(&value);
        tx.queued.fetch_add(bytes, Ordering::AcqRel);
        tx.count.fetch_add(1, Ordering::AcqRel);
        tx.reliable.send((value, bytes)).map_err(|_| {
            tx.queued.fetch_sub(bytes, Ordering::AcqRel);
            tx.count.fetch_sub(1, Ordering::AcqRel);
            anyhow::anyhow!("Session closed by the editor")
        })
    };
    if let Some(waker) = tx.waker.get() {
        waker();
    }
    result
}

#[cfg(test)]
pub(crate) fn test_editor(events: usize) -> (Editor, SessionEvents) {
    queue_pair(events, EDITOR_BACKLOG_BYTES)
}

#[cfg(test)]
pub(crate) fn test_editor_limited(events: usize, bytes: usize) -> (Editor, SessionEvents) {
    queue_pair(events, bytes)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fuzz;

#[cfg(test)]
mod attacks;

#[cfg(test)]
mod bench;

#[cfg(test)]
mod sessions;
