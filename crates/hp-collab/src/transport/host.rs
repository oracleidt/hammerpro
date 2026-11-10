use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use iroh::Endpoint;
use iroh::endpoint::{Connection, presets};
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, mpsc as channel, watch};
use zeroize::Zeroizing;

use super::frame::{FAST, Frame, MapTransfer};
use super::handshake::{Gate, PEER_FRAME_LIMIT, accept_loop, lock};
use super::queue::{Body, Kind, Outgoing, PeerQueue, Pushed};
use super::{APPROVAL_DEADLINE, Command, Commands, DEADLINE, Editor, Event, HostOptions, event};
use crate::document::{Change, ID_STRIDE, OutsideRange, Snapshot, ValidatedSnapshot, expand_changes, patch_changes};
use crate::moderation::{AccessPolicy, AccessState, MAX_BAN_SECONDS, PendingRow, Window};
use crate::protocol::{
    self, ALPN, ConnectionMode, EditRefusal, Invitation, LEGACY_ALPN, MAX_PARTICIPANTS, Outcome, Participant, Presence,
    Refusal, Refused, Rejection, Role, Wire,
};

pub(crate) const MAX_DEVICES: usize = 256;
pub(crate) const RANGE_LIMIT: u32 = 1_990_000_000;
pub(crate) const PRESENCE_PER_SECOND: u32 = 15;

pub(crate) trait Link: Send + Sync {
    fn close(&self, refusal: Refusal);
    fn generation(&self) -> usize;
    fn latency_ms(&self) -> u64;
    fn observed_ip(&self) -> Option<IpAddr>;
    fn direct_paths(&self) -> usize;
}

impl Link for Connection {
    fn close(&self, refusal: Refusal) {
        Connection::close(self, refusal.close_code().into(), refusal.id().as_bytes());
    }

    fn generation(&self) -> usize {
        self.stable_id()
    }

    fn latency_ms(&self) -> u64 {
        self.paths().iter().find(|path| path.is_selected()).map_or(0, |path| path.rtt().as_millis() as u64)
    }

    fn observed_ip(&self) -> Option<IpAddr> {
        self.paths().iter().find_map(|path| match path.remote_addr() {
            iroh::TransportAddr::Ip(address) => Some(address.ip()),
            _ => None,
        })
    }

    fn direct_paths(&self) -> usize {
        self.paths().iter().filter(|path| path.remote_addr().is_ip()).count()
    }
}

pub(crate) struct Joining {
    pub(crate) id: String,
    pub(crate) device: String,
    pub(crate) name: String,
    pub(crate) epoch: u64,
    pub(crate) link: Box<dyn Link>,
    pub(crate) queue: Arc<PeerQueue>,
    pub(crate) participants: watch::Sender<Option<Frame>>,
}

impl Joining {
    fn refuse(&self, refused: Refused) {
        let frame = Frame::encode(&Wire::Closed(refused.clone())).ok();
        let fallback = frame.is_none();
        self.queue.refuse(refused.clone(), frame);
        if fallback {
            self.link.close(refused.refusal);
        }
    }
}

impl Drop for Joining {
    fn drop(&mut self) {
        self.queue.detach();
    }
}

pub(crate) enum Network {
    Joined(Box<Joining>),
    Message { id: String, generation: usize, message: Wire, bytes: u64, permit: Vec<OwnedSemaphorePermit> },
    Oversized { id: String, generation: usize, sequence: u64, size: usize },
    Flooded { id: String, generation: usize },
    Left { id: String, generation: usize },
    Sent(u64),
}

struct Deferred {
    sequence: u64,
    changes: Vec<Change>,
    refused: Option<Rejection>,
    _permit: Vec<OwnedSemaphorePermit>,
}

struct Peer {
    joining: Joining,
    participant: Participant,
    id_start: u32,
    sequence: u64,
    presence: Window,
    deferred: VecDeque<Deferred>,
    flooded: bool,
}

struct Waiting {
    joining: Joining,
    since: Instant,
}

#[derive(Clone)]
struct Device {
    ranges: Vec<u32>,
    role: Role,
}

#[derive(Serialize)]
enum AppliedRef<'a> {
    Applied { author: u32, sequence: u64, revision: u64, changes: &'a [Change] },
}

pub(crate) struct HostCore {
    gate: Arc<Mutex<Gate>>,
    editor: Editor,
    mode: ConnectionMode,
    map: ValidatedSnapshot,
    revision: u64,
    pub(crate) host_range: u32,
    next_range: u32,
    next_participant: u32,
    peers: BTreeMap<String, Peer>,
    pending: BTreeMap<u32, Waiting>,
    devices: BTreeMap<String, Device>,
    policy: AccessPolicy,
    issued: Instant,
    host_row: Participant,
    shared: Option<(u64, Arc<Snapshot>)>,
    refused_joins: u64,
    rate_limited: u64,
    resyncs: u64,
    turn: Option<String>,
    last_participants: Vec<Participant>,
    sent: u64,
    received: u64,
    last_traffic: Option<Instant>,
}

pub(crate) fn range_start(map: &ValidatedSnapshot) -> Result<u32> {
    let highest = map.highest_id().max(1);
    let start = (highest / ID_STRIDE + 1) * ID_STRIDE;
    ensure!(start < 1_900_000_000, "Map identifiers too large for a session");
    Ok(start)
}

fn document(error: anyhow::Error) -> Rejection {
    match error.downcast_ref::<OutsideRange>() {
        Some(outside) if outside.side => Rejection::new(EditRefusal::SideOutsideRange, outside.id.to_string()),
        Some(outside) => Rejection::new(EditRefusal::OutsideRange, outside.id.to_string()),
        None => Rejection::new(EditRefusal::Document, format!("{error:#}")),
    }
}

impl HostCore {
    pub(crate) fn new(
        gate: Arc<Mutex<Gate>>,
        editor: Editor,
        mode: ConnectionMode,
        map: ValidatedSnapshot,
        name: String,
        policy: AccessPolicy,
        now: Instant,
    ) -> Result<Self> {
        let host_range = range_start(&map)?;
        Ok(Self {
            gate,
            editor,
            mode,
            map,
            revision: 0,
            host_range,
            next_range: host_range + ID_STRIDE,
            next_participant: 2,
            peers: BTreeMap::new(),
            pending: BTreeMap::new(),
            devices: BTreeMap::new(),
            policy,
            issued: now,
            host_row: Participant { id: 1, name, role: Role::Host, presence: Presence::default(), latency_ms: 0 },
            shared: None,
            refused_joins: 0,
            rate_limited: 0,
            resyncs: 0,
            turn: None,
            last_participants: Vec::new(),
            sent: 0,
            received: 0,
            last_traffic: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> &Snapshot {
        self.map.snapshot()
    }

    pub(crate) fn validated(&self) -> &ValidatedSnapshot {
        &self.map
    }

    fn rows(&self) -> Vec<Participant> {
        std::iter::once(self.host_row.clone()).chain(self.peers.values().map(|peer| peer.participant.clone())).collect()
    }

    fn expired(&self, now: Instant) -> bool {
        self.policy.invitation_minutes > 0
            && now.saturating_duration_since(self.issued)
                >= Duration::from_secs(u64::from(self.policy.invitation_minutes) * 60)
    }

    fn shared_map(&mut self) -> Arc<Snapshot> {
        if let Some((revision, map)) = &self.shared
            && *revision == self.revision
        {
            return map.clone();
        }
        let map = Arc::new(self.map.snapshot().clone());
        self.shared = Some((self.revision, map.clone()));
        map
    }

    fn sizes(&self) -> (u64, u64) {
        let sizes = self.map.sizes();
        (sizes.units as u64, sizes.encoded_bytes as u64)
    }

    fn welcome_transfer(&mut self, participant: u32, id_start: u32) -> Result<MapTransfer> {
        let (units, bytes) = self.sizes();
        let start = Wire::MapStart { participant, id_start, revision: self.revision, units, bytes };
        MapTransfer::new(&start, self.shared_map())
    }

    fn reset_transfer(&mut self) -> Result<MapTransfer> {
        let (units, bytes) = self.sizes();
        MapTransfer::new(&Wire::Reset { revision: self.revision, units, bytes }, self.shared_map())
    }

    fn resync_transfer(&mut self, outcomes: &[Outcome]) -> Result<MapTransfer> {
        let (units, bytes) = self.sizes();
        let start = Wire::Resync { revision: self.revision, outcomes: outcomes.to_vec(), units, bytes };
        MapTransfer::new(&start, self.shared_map())
    }

    fn push(&mut self, id: &str, item: Outgoing) -> Result<()> {
        let Some(peer) = self.peers.get(id) else { return Ok(()) };
        if let Pushed::Behind(outcomes) = peer.joining.queue.push(item) {
            let transfer = self.resync_transfer(&outcomes)?;
            if let Some(peer) = self.peers.get(id) {
                peer.joining.queue.push(Outgoing::map(transfer, Kind::Resync, outcomes));
                self.resyncs += 1;
            }
        }
        Ok(())
    }

    fn broadcast(&mut self, frame: &Frame, author: Option<(u32, u64, &Frame)>) -> Result<()> {
        let targets: Vec<(String, u32)> =
            self.peers.iter().map(|(id, peer)| (id.clone(), peer.participant.id)).collect();
        for (id, participant) in targets {
            let (frame, outcomes) = match author {
                Some((author, sequence, acknowledgement)) if author == participant => {
                    (acknowledgement.clone(), vec![Outcome { sequence, rejection: None }])
                }
                _ => (frame.clone(), Vec::new()),
            };
            self.push(&id, Outgoing { body: Body::Frame(frame), kind: Kind::Normal, outcomes })?;
        }
        Ok(())
    }

    fn broadcast_reset(&mut self) -> Result<()> {
        let targets: Vec<String> = self.peers.keys().cloned().collect();
        for id in targets {
            let transfer = self.reset_transfer()?;
            self.push(&id, Outgoing::map(transfer, Kind::Resync, Vec::new()))?;
        }
        Ok(())
    }

    fn drop_peer(&mut self, id: &str, refused: Refused) {
        if let Some(peer) = self.peers.remove(id) {
            peer.joining.refuse(refused);
        }
    }

    fn locked_keys(&self, author: u32) -> HashMap<&str, &str> {
        std::iter::once(&self.host_row)
            .chain(self.peers.values().map(|peer| &peer.participant))
            .filter(|row| row.id != author && row.role.can_edit())
            .flat_map(|row| row.presence.selected.iter().map(|key| (key.as_str(), row.name.as_str())))
            .collect()
    }

    fn apply(&mut self, changes: &[Change], author: u32, range: u32) -> std::result::Result<(), Rejection> {
        if changes.is_empty() {
            return Err(Rejection::new(EditRefusal::InvalidEdit, ""));
        }
        let first = if author == 1 { 1 } else { range };
        let span = first..range.saturating_add(ID_STRIDE);
        let in_range = |id: u32| span.contains(&id);
        {
            let locked = self.locked_keys(author);
            for change in changes {
                if !protocol::valid_change_key(&change.key) {
                    return Err(Rejection::new(EditRefusal::InvalidKey, &change.key));
                }
                if let Some(name) = locked.get(change.key.as_str()) {
                    return Err(Rejection::new(EditRefusal::Selected, format!("{} ({name})", change.key)));
                }
                if change.before.is_none()
                    && let Some(id) = protocol::object_number(&change.key)
                    && !in_range(id)
                {
                    return Err(Rejection::new(EditRefusal::OutsideRange, &change.key));
                }
            }
        }
        if self.policy.refuse_scripts
            && let Some(problem) = crate::document::script_refusal(changes)
        {
            return Err(Rejection::new(EditRefusal::Document, problem));
        }
        self.map.apply_from(changes, span).map_err(document)
    }

    fn applied(&mut self, author: u32, sequence: u64, changes: Vec<Change>, patched: Vec<Change>) -> Result<()> {
        self.revision += 1;
        let revision = self.revision;
        let frame = Frame::pack(&AppliedRef::Applied { author, sequence, revision, changes: &patched }, FAST)?;
        drop(patched);
        let acknowledgement = Frame::encode(&Wire::Acked { sequence, revision })?;
        self.broadcast(&frame, Some((author, sequence, &acknowledgement)))?;
        event(&self.editor, Event::Applied { author, sequence, revision, changes })
    }

    fn reject(&mut self, id: &str, sequence: u64, rejection: Rejection) -> Result<()> {
        let rejection = rejection.sanitized();
        let frame = Frame::encode(&Wire::Rejected { sequence, rejection: rejection.clone(), revision: self.revision })?;
        let outcomes = vec![Outcome { sequence, rejection: Some(rejection) }];
        self.push(id, Outgoing { body: Body::Frame(frame), kind: Kind::Normal, outcomes })
    }

    fn find_peer(&self, participant: u32) -> Option<String> {
        self.peers.iter().find(|(_, peer)| peer.participant.id == participant).map(|(id, _)| id.clone())
    }

    fn forget(&mut self, device: &str) -> Result<()> {
        self.devices.remove(device);
        lock(&self.gate)?.forget_device(device);
        Ok(())
    }

    pub(crate) fn command(&mut self, command: Command, now: Instant) -> Result<bool> {
        match command {
            Command::Stop => return Ok(false),
            Command::Restore { map: restored, store } => {
                let name = lock(&self.gate)?.invitation.room.name.clone();
                let backup = restored
                    .validate()
                    .and_then(|()| store.save(self.map.snapshot(), &name, self.revision).map(|_| ()));
                if let Err(error) = backup {
                    event(&self.editor, Event::Error(format!("Restore cancelled; safety snapshot failed: {error}")))?;
                    return Ok(true);
                }
                self.map = ValidatedSnapshot::new(restored)?;
                self.revision += 1;
                self.broadcast_reset()?;
                event(&self.editor, Event::Reset { revision: self.revision, map: self.map.clone() })?;
            }
            Command::Snapshot { store, done } => {
                let name = lock(&self.gate)?.invitation.room.name.clone();
                let (copy, at) = (self.map.snapshot().clone(), self.revision);
                let saving = std::thread::Builder::new().name("team-snapshot".into()).spawn(move || {
                    let result = store.save(&copy, &name, at).and_then(|_| store.list()).map_err(|e| e.to_string());
                    let _ = done.send(result);
                });
                if let Err(error) = saving {
                    event(&self.editor, Event::Error(format!("Session snapshot failed: {error}")))?;
                }
            }
            Command::Presence(presence) => {
                if presence.validate() {
                    self.host_row.presence = presence;
                }
            }
            Command::Kick(participant) => {
                if let Some(id) = self.find_peer(participant)
                    && let Some(peer) = self.peers.remove(&id)
                {
                    self.forget(&peer.joining.device)?;
                    peer.joining.refuse(Refused::new(Refusal::Removed));
                } else if let Some(waiting) = self.pending.remove(&participant) {
                    waiting.joining.refuse(Refused::new(Refusal::Declined));
                }
            }
            Command::Ban { participant, seconds, reason } => {
                let reason = protocol::sanitize(reason.trim());
                let reason = if reason.is_empty() { "Banned by the host".to_owned() } else { reason };
                let seconds = seconds.map(|seconds| seconds.clamp(1, MAX_BAN_SECONDS));
                let target = match self.find_peer(participant).and_then(|id| self.peers.remove(&id)) {
                    Some(peer) => Some((peer.joining, peer.participant.name)),
                    None => self.pending.remove(&participant).map(|waiting| {
                        let name = waiting.joining.name.clone();
                        (waiting.joining, name)
                    }),
                };
                if let Some((joining, name)) = target {
                    let ip = joining.link.observed_ip();
                    lock(&self.gate)?.bans.add(
                        joining.device.clone(),
                        ip,
                        name,
                        reason.clone(),
                        seconds.map(Duration::from_secs),
                        now,
                    );
                    self.forget(&joining.device)?;
                    joining.refuse(Refused::with(Refusal::Banned, &reason, seconds));
                }
            }
            Command::Unban(id) => lock(&self.gate)?.bans.remove(id),
            Command::Rotate(password) => {
                if let Err(error) = self.rotate(&password, now) {
                    event(&self.editor, Event::Error(error.to_string()))?;
                }
            }
            Command::Edit { sequence, changes } => {
                let range = self.host_range;
                let patched = patch_changes(&changes, self.map.snapshot());
                match self.apply(&changes, 1, range) {
                    Ok(()) => self.applied(1, sequence, changes, patched)?,
                    Err(rejection) => {
                        event(&self.editor, Event::Rejected { sequence, rejection, revision: self.revision })?
                    }
                }
            }
            Command::SetRole { participant, role } => {
                if role != Role::Host
                    && let Some(id) = self.find_peer(participant)
                    && let Some(peer) = self.peers.get_mut(&id)
                {
                    peer.participant.role = role;
                    if let Some(device) = self.devices.get_mut(&peer.joining.device) {
                        device.role = role;
                    }
                }
            }
            Command::Admit(participant) => {
                if let Some(waiting) = self.pending.remove(&participant) {
                    self.admit(participant, waiting.joining, now)?;
                }
            }
            Command::Decline(participant) => {
                if let Some(waiting) = self.pending.remove(&participant) {
                    waiting.joining.refuse(Refused::new(Refusal::Declined));
                }
            }
            Command::Policy(policy) => {
                if policy.validate().is_ok() {
                    self.policy = policy;
                }
            }
        }
        Ok(true)
    }

    fn rotate(&mut self, password: &Zeroizing<String>, now: Instant) -> Result<()> {
        ensure!((8..=256).contains(&password.len()), "Use at least 8 password characters");
        let encoded = lock(&self.gate)?.rotate(password, now)?;
        for (_, peer) in std::mem::take(&mut self.peers) {
            peer.joining.refuse(Refused::new(Refusal::Revoked));
        }
        for (_, waiting) in std::mem::take(&mut self.pending) {
            waiting.joining.refuse(Refused::new(Refusal::Revoked));
        }
        self.devices.clear();
        self.issued = now;
        event(&self.editor, Event::Invitation(encoded))
    }

    fn name_taken(&self, name: &str) -> bool {
        self.host_row.name.eq_ignore_ascii_case(name)
            || self.peers.values().any(|peer| peer.participant.name.eq_ignore_ascii_case(name))
            || self.pending.values().any(|waiting| waiting.joining.name.eq_ignore_ascii_case(name))
    }

    fn joined(&mut self, joining: Joining, now: Instant) -> Result<()> {
        let replaced = self
            .peers
            .iter()
            .find(|(_, peer)| {
                peer.joining.device == joining.device && peer.participant.name.eq_ignore_ascii_case(&joining.name)
            })
            .map(|(id, _)| id.clone());
        if let Some(id) = replaced
            && let Some(peer) = self.peers.remove(&id)
        {
            peer.joining.link.close(Refusal::Replaced);
        }
        let ip = joining.link.observed_ip();
        let (banned, epoch) = {
            let mut gate = lock(&self.gate)?;
            (gate.bans.denied(&joining.device, ip, now), gate.epoch)
        };
        let known = self.devices.contains_key(&joining.device);
        let refusal = if let Some((reason, seconds)) = banned {
            Some(Refused::with(Refusal::Banned, &reason, seconds))
        } else if joining.epoch != epoch {
            Some(Refused::new(Refusal::Revoked))
        } else if self.peers.len() + self.pending.len() >= MAX_PARTICIPANTS - 1
            || self.peers.contains_key(&joining.id)
            || self.pending.values().any(|waiting| waiting.joining.id == joining.id)
            || (!known && (self.devices.len() >= MAX_DEVICES || self.next_range >= RANGE_LIMIT))
        {
            Some(Refused::new(Refusal::Full))
        } else if self.name_taken(&joining.name) {
            Some(Refused::new(Refusal::NicknameTaken))
        } else if !known && (self.policy.locked || self.expired(now)) {
            Some(Refused::new(Refusal::Locked))
        } else {
            None
        };
        if let Some(refused) = refusal {
            self.refused_joins += 1;
            joining.refuse(refused);
            return Ok(());
        }
        let participant = self.next_participant;
        self.next_participant = self.next_participant.checked_add(1).context("Session participant limit reached")?;
        if !known && self.policy.approval {
            let frame = Frame::encode(&Wire::Waiting)?;
            if joining.queue.push(Outgoing::new(frame, Kind::Normal)) == Pushed::Queued {
                self.pending.insert(participant, Waiting { joining, since: now });
            }
            return Ok(());
        }
        self.admit(participant, joining, now)
    }

    fn admit(&mut self, participant: u32, joining: Joining, now: Instant) -> Result<()> {
        let mut device = self.devices.get(&joining.device).cloned().unwrap_or(Device {
            ranges: Vec::new(),
            role: if self.policy.read_only_guests { Role::Viewer } else { Role::Editor },
        });
        let in_use: BTreeSet<u32> = self
            .peers
            .values()
            .filter(|peer| peer.joining.device == joining.device)
            .map(|peer| peer.id_start)
            .collect();
        let id_start = match device.ranges.iter().find(|range| !in_use.contains(range)) {
            Some(range) => *range,
            None => {
                if self.next_range >= RANGE_LIMIT {
                    joining.refuse(Refused::new(Refusal::Full));
                    return Ok(());
                }
                let range = self.next_range;
                self.next_range += ID_STRIDE;
                device.ranges.push(range);
                range
            }
        };
        let transfer = self.welcome_transfer(participant, id_start)?;
        if joining.queue.push(Outgoing::map(transfer, Kind::Welcome, Vec::new())) != Pushed::Queued {
            return Ok(());
        }
        lock(&self.gate)?.admit_device(&joining.device);
        let role = device.role;
        self.devices.insert(joining.device.clone(), device);
        let participant = Participant {
            id: participant,
            name: joining.name.clone(),
            role,
            presence: Presence::default(),
            latency_ms: 0,
        };
        self.peers.insert(
            joining.id.clone(),
            Peer {
                joining,
                participant,
                id_start,
                sequence: 0,
                presence: Window::new(now),
                deferred: VecDeque::new(),
                flooded: false,
            },
        );
        self.share_participants()
    }

    pub(crate) fn network(&mut self, message: Network, now: Instant) -> Result<()> {
        match message {
            Network::Joined(joining) => self.joined(*joining, now)?,
            Network::Left { id, generation } => {
                if self.peers.get(&id).is_some_and(|peer| peer.joining.link.generation() == generation) {
                    self.peers.remove(&id);
                }
                self.pending.retain(|_, waiting| {
                    !(waiting.joining.id == id && waiting.joining.link.generation() == generation)
                });
            }
            Network::Sent(bytes) => self.sent = self.sent.saturating_add(bytes),
            Network::Flooded { id, generation } => {
                if let Some(peer) = self.peers.get_mut(&id)
                    && peer.joining.link.generation() == generation
                    && !peer.flooded
                {
                    peer.flooded = true;
                    self.rate_limited += 1;
                }
            }
            Network::Oversized { id, generation, sequence, size } => {
                self.received = self.received.saturating_add(size as u64 + 4);
                let refused = Rejection::too_large(size, PEER_FRAME_LIMIT);
                self.defer(&id, generation, sequence, Vec::new(), Some(refused), Vec::new());
            }
            Network::Message { id, generation, message, bytes, permit } => {
                self.received = self.received.saturating_add(bytes);
                self.message(&id, generation, message, permit, now);
            }
        }
        Ok(())
    }

    fn message(&mut self, id: &str, generation: usize, message: Wire, permit: Vec<OwnedSemaphorePermit>, now: Instant) {
        let Some(peer) = self.peers.get_mut(id) else { return };
        if peer.joining.link.generation() != generation {
            return;
        }
        match message {
            Wire::Presence(presence) => {
                if !presence.validate() {
                    self.drop_peer(id, Refused::new(Refusal::Protocol));
                } else if peer.presence.allow(now, PRESENCE_PER_SECOND, Duration::from_secs(1)) {
                    peer.participant.presence = presence;
                }
            }
            Wire::Ping(_) => {}
            Wire::Edit { sequence, changes } => self.defer(id, generation, sequence, changes, None, permit),
            _ => self.drop_peer(id, Refused::new(Refusal::Protocol)),
        }
    }

    fn defer(
        &mut self,
        id: &str,
        generation: usize,
        sequence: u64,
        changes: Vec<Change>,
        refused: Option<Rejection>,
        permit: Vec<OwnedSemaphorePermit>,
    ) {
        let Some(peer) = self.peers.get_mut(id) else { return };
        if peer.joining.link.generation() != generation {
            return;
        }
        if sequence <= peer.sequence {
            self.drop_peer(id, Refused::new(Refusal::Protocol));
            return;
        }
        peer.sequence = sequence;
        peer.deferred.push_back(Deferred { sequence, changes, refused, _permit: permit });
    }

    pub(crate) fn has_work(&self) -> bool {
        !self.editor.busy() && self.peers.values().any(|peer| !peer.deferred.is_empty())
    }

    pub(crate) fn work(&mut self) -> Result<()> {
        let waiting: Vec<&String> =
            self.peers.iter().filter(|(_, peer)| !peer.deferred.is_empty()).map(|(id, _)| id).collect();
        let next = match &self.turn {
            Some(turn) => waiting.iter().find(|id| **id > turn).or(waiting.first()),
            None => waiting.first(),
        };
        let Some(id) = next.map(|id| (*id).clone()) else { return Ok(()) };
        self.turn = Some(id.clone());
        let Some(peer) = self.peers.get_mut(&id) else { return Ok(()) };
        let Some(mut item) = peer.deferred.pop_front() else { return Ok(()) };
        let (author, range, role) = (peer.participant.id, peer.id_start, peer.participant.role);
        if let Some(refused) = item.refused {
            return self.reject(&id, item.sequence, refused);
        }
        if !role.can_edit() {
            return self.reject(&id, item.sequence, Rejection::new(EditRefusal::ReadOnly, ""));
        }
        if let Err(error) = expand_changes(&mut item.changes, self.map.snapshot()) {
            return self.reject(&id, item.sequence, document(error));
        }
        let patched = patch_changes(&item.changes, self.map.snapshot());
        match self.apply(&item.changes, author, range) {
            Ok(()) => self.applied(author, item.sequence, std::mem::take(&mut item.changes), patched),
            Err(rejection) => self.reject(&id, item.sequence, rejection),
        }
    }

    pub(crate) fn access_state(&self, now: Instant) -> Result<AccessState> {
        let mut gate = lock(&self.gate)?;
        let lockout_seconds = gate.limiter.locked(now).map(|left| left.as_secs().max(1));
        let new_devices_seconds = (self.policy.invitation_minutes > 0).then(|| {
            (self.issued + Duration::from_secs(u64::from(self.policy.invitation_minutes) * 60))
                .saturating_duration_since(now)
                .as_secs()
        });
        Ok(AccessState {
            policy: self.policy,
            pending: self
                .pending
                .iter()
                .map(|(id, waiting)| PendingRow {
                    id: *id,
                    name: waiting.joining.name.clone(),
                    waiting_seconds: now.saturating_duration_since(waiting.since).as_secs(),
                })
                .collect(),
            failed_passwords: gate.failed_passwords,
            invalid_attempts: gate.invalid_attempts,
            refused_joins: self.refused_joins,
            rate_limited: self.rate_limited,
            resyncs: self.resyncs,
            lockout_seconds,
            new_devices_seconds,
            known_devices: self.devices.len(),
        })
    }

    pub(crate) fn tick(&mut self, now: Instant, published_ips: usize) -> Result<()> {
        for peer in self.peers.values_mut() {
            peer.participant.latency_ms = peer.joining.link.latency_ms().min(protocol::MAX_LATENCY_MS);
        }
        let direct_path_count: usize = self.peers.values().map(|peer| peer.joining.link.direct_paths()).sum();
        if self.mode == ConnectionMode::Relay {
            ensure!(direct_path_count == 0 && published_ips == 0, "Direct path detected in relay-only session");
        }
        let expired: Vec<u32> = self
            .pending
            .iter()
            .filter(|(_, waiting)| now.saturating_duration_since(waiting.since) >= APPROVAL_DEADLINE)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some(waiting) = self.pending.remove(&id) {
                waiting.joining.refuse(Refused::new(Refusal::ApprovalTimeout));
            }
        }
        if self.shared.as_ref().is_some_and(|(revision, map)| *revision != self.revision || Arc::strong_count(map) == 1)
        {
            self.shared = None;
        }
        self.share_participants()?;
        if self.last_traffic.is_none_or(|last| now.saturating_duration_since(last) >= Duration::from_secs(1)) {
            event(&self.editor, Event::Traffic { sent: self.sent, received: self.received })?;
            let bans = lock(&self.gate)?.bans.rows(now);
            event(&self.editor, Event::Bans(bans))?;
            event(
                &self.editor,
                Event::NetworkPrivacy {
                    relay_only: self.mode == ConnectionMode::Relay,
                    published_ip_count: published_ips,
                    direct_path_count,
                },
            )?;
            event(&self.editor, Event::Access(self.access_state(now)?))?;
            self.last_traffic = Some(now);
        }
        Ok(())
    }

    fn share_participants(&mut self) -> Result<()> {
        let participants = self.rows();
        if participants != self.last_participants {
            let frame = Frame::pack(&Wire::Participants(participants.clone()), FAST)?;
            for peer in self.peers.values() {
                peer.joining.participants.send_replace(Some(frame.clone()));
            }
            event(&self.editor, Event::Participants(participants.clone()))?;
            self.last_participants = participants;
        }
        Ok(())
    }

    pub(crate) fn shutdown(&mut self) {
        for (_, peer) in std::mem::take(&mut self.peers) {
            peer.joining.link.close(Refusal::Ended);
        }
        for (_, waiting) in std::mem::take(&mut self.pending) {
            waiting.joining.link.close(Refusal::Ended);
        }
    }

    #[cfg(test)]
    pub(crate) fn peer_count(&self) -> usize {
        self.peers.len()
    }

    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(test)]
    pub(crate) fn map_cached(&self) -> bool {
        self.shared.is_some()
    }

    #[cfg(test)]
    pub(crate) fn deferred_count(&self) -> usize {
        self.peers.values().map(|peer| peer.deferred.len()).sum()
    }

    #[cfg(test)]
    pub(crate) fn presence_of(&self, participant: u32) -> Option<Presence> {
        self.peers
            .values()
            .find(|peer| peer.participant.id == participant)
            .map(|peer| peer.participant.presence.clone())
    }

    #[cfg(test)]
    pub(crate) fn locked_count(&self, author: u32) -> usize {
        self.locked_keys(author).len()
    }

    #[cfg(test)]
    pub(crate) fn try_apply(&mut self, changes: &[Change], author: u32, range: u32) -> Result<(), Rejection> {
        self.apply(changes, author, range)
    }

    #[cfg(test)]
    pub(crate) fn role_of(&self, participant: u32) -> Option<Role> {
        self.peers.values().find(|peer| peer.participant.id == participant).map(|peer| peer.participant.role)
    }

    #[cfg(test)]
    pub(crate) fn range_of(&self, participant: u32) -> Option<u32> {
        self.peers.values().find(|peer| peer.participant.id == participant).map(|peer| peer.id_start)
    }
}

pub(crate) async fn run(
    map: ValidatedSnapshot,
    name: String,
    password: Zeroizing<String>,
    options: HostOptions,
    mut commands: Commands,
    editor: Editor,
) -> Result<()> {
    let builder = match options.mode {
        ConnectionMode::Relay => Endpoint::builder(presets::N0).clear_ip_transports(),
        ConnectionMode::Direct => Endpoint::builder(presets::Minimal)
            .relay_mode(iroh::RelayMode::Disabled)
            .clear_address_lookup()
            .bind_addr(std::net::SocketAddr::from(([0, 0, 0, 0], options.port)))?,
    }
    .alpns(vec![ALPN.to_vec(), LEGACY_ALPN.to_vec()])
    .transport_config(
        iroh::endpoint::QuicTransportConfig::builder()
            .max_concurrent_bidi_streams(1u32.into())
            .max_concurrent_uni_streams(0u32.into())
            .max_idle_timeout(Some(DEADLINE.try_into()?))
            .build(),
    );
    let endpoint = builder.bind().await?;
    if options.mode == ConnectionMode::Relay {
        ensure!(endpoint.addr().ip_addrs().next().is_none(), "Direct address publication is disabled");
        tokio::time::timeout(DEADLINE, endpoint.online())
            .await
            .context("Relay unavailable: cannot create a session until a relay is reachable")?;
    }
    let relay = endpoint.addr().relay_urls().next().map(ToString::to_string);
    ensure!(
        options.mode != ConnectionMode::Relay || relay.is_some(),
        "Relay unavailable: no relay address was assigned"
    );
    let invitation = Invitation {
        version: protocol::INVITATION_FORMAT,
        endpoint: endpoint.id().to_string(),
        relay,
        token: protocol::random_secret()?,
        room: options.room,
        mode: options.mode,
    };
    let encoded = invitation.encode()?;
    let gate = Arc::new(Mutex::new(Gate::new(invitation, &password, Instant::now())));
    drop(password);
    let (network, mut network_rx) = channel::channel(128);
    let mut core =
        HostCore::new(gate.clone(), editor.clone(), options.mode, map, name, options.policy, Instant::now())?;
    let acceptor = tokio::spawn(accept_loop(endpoint.clone(), gate, network, options.mode));
    event(
        &editor,
        Event::Ready {
            invitation: Some(encoded),
            participant: 1,
            id_start: core.host_range,
            revision: 0,
            map: core.validated().clone(),
            direct_addresses: endpoint.addr().ip_addrs().copied().collect(),
        },
    )?;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let result = async {
        loop {
            tokio::select! {
                biased;
                command = commands.next() => match command {
                    None => break,
                    Some(command) => if !core.command(command, Instant::now())? { break },
                },
                message = network_rx.recv() => match message {
                    Some(message) => core.network(message, Instant::now())?,
                    None => bail!("Session network closed"),
                },
                _ = tick.tick() => core.tick(Instant::now(), endpoint.addr().ip_addrs().count())?,
                () = std::future::ready(()), if core.has_work() => core.work()?,
            }
        }
        Ok(())
    }
    .await;
    acceptor.abort();
    core.shutdown();
    endpoint.close().await;
    result
}
