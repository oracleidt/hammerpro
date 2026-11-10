use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr};
use tokio::sync::watch;
use zeroize::Zeroizing;

use super::frame::{Frame, decode, read_frame, write_frame};
use super::handshake::{ClientHandshake, Gate};
use super::host::{HostCore, Joining, Link, Network};
use super::queue::{Kind, PEER_BACKLOG_BYTES, PeerQueue};
use super::*;
use crate::document::{Snapshot, ValidatedSnapshot};
use crate::moderation::AccessPolicy;
use crate::protocol::{ALPN, Invitation, Refusal, Room, Wire};

#[derive(Clone, Default)]
pub(crate) struct FakeLink {
    pub(crate) closed: Arc<Mutex<Option<Refusal>>>,
    pub(crate) generation: usize,
    pub(crate) ip: Option<IpAddr>,
    pub(crate) direct: usize,
}

impl Link for FakeLink {
    fn close(&self, refusal: Refusal) {
        *self.closed.lock().unwrap() = Some(refusal);
    }

    fn generation(&self) -> usize {
        self.generation
    }

    fn latency_ms(&self) -> u64 {
        12
    }

    fn observed_ip(&self) -> Option<IpAddr> {
        self.ip
    }

    fn direct_paths(&self) -> usize {
        self.direct
    }
}

pub(crate) struct FakePeer {
    pub(crate) id: String,
    pub(crate) queue: Arc<PeerQueue>,
    pub(crate) participants: watch::Receiver<Option<Frame>>,
    pub(crate) link: FakeLink,
}

impl FakePeer {
    pub(crate) fn frames(&mut self) -> Vec<(Wire, Kind, usize)> {
        let mut frames = Vec::new();
        while let Some(item) = self.queue.try_pop() {
            let kind = item.kind;
            for frame in item.frames() {
                let bytes = frame.payload();
                frames.push((decode(&bytes).expect("host frames decode"), kind, bytes.len()));
            }
        }
        frames
    }

    pub(crate) fn closed(&self) -> Option<Refusal> {
        (*self.link.closed.lock().unwrap()).or_else(|| self.queue.refused().map(|refused| refused.refusal))
    }
}

pub(crate) fn joining(id: &str, device: &str, name: &str, epoch: u64) -> (Joining, FakePeer) {
    joining_from(id, device, name, epoch, None)
}

pub(crate) fn joining_from(id: &str, device: &str, name: &str, epoch: u64, ip: Option<IpAddr>) -> (Joining, FakePeer) {
    joining_with(id, device, name, epoch, ip, PEER_BACKLOG_BYTES)
}

pub(crate) fn joining_with(
    id: &str,
    device: &str,
    name: &str,
    epoch: u64,
    ip: Option<IpAddr>,
    backlog: usize,
) -> (Joining, FakePeer) {
    let queue = Arc::new(PeerQueue::new(backlog));
    let (participants, participants_rx) = watch::channel(None);
    let link = FakeLink { ip, generation: 7, ..FakeLink::default() };
    (
        Joining {
            id: id.into(),
            device: device.into(),
            name: name.into(),
            epoch,
            link: Box::new(link.clone()),
            queue: queue.clone(),
            participants,
        },
        FakePeer { id: id.into(), queue, participants: participants_rx, link },
    )
}

pub(crate) fn fixture_map() -> Snapshot {
    Snapshot::from_vmf(
        "world {\"id\" \"1\" \"classname\" \"worldspawn\"} entity {\"id\" \"2\" \"classname\" \"light\" \"origin\" \"0 0 0\"} entity {\"id\" \"3\" \"classname\" \"info_target\" \"origin\" \"8 0 0\"}",
    )
    .unwrap()
}

pub(crate) fn test_invitation() -> Invitation {
    Invitation {
        version: protocol::INVITATION_FORMAT,
        endpoint: iroh::SecretKey::from_bytes(&[3; 32]).public().to_string(),
        relay: None,
        token: [5; 32],
        room: Room { name: "Test".into() },
        mode: ConnectionMode::Direct,
    }
}

pub(crate) struct Host {
    pub(crate) core: HostCore,
    pub(crate) events: SessionEvents,
    pub(crate) gate: Arc<Mutex<Gate>>,
    pub(crate) now: Instant,
}

impl Host {
    pub(crate) fn new(map: Snapshot, policy: AccessPolicy) -> Self {
        let now = Instant::now();
        let gate = Arc::new(Mutex::new(Gate::new(test_invitation(), "test-password", now)));
        let (editor, events) = test_editor(1 << 16);
        let core = HostCore::new(
            gate.clone(),
            editor,
            ConnectionMode::Direct,
            ValidatedSnapshot::new(map).unwrap(),
            "Host".into(),
            policy,
            now,
        )
        .unwrap();
        Self { core, events, gate, now }
    }

    pub(crate) fn join(&mut self, id: &str, device: &str, name: &str) -> FakePeer {
        let (joining, peer) = joining(id, device, name, self.gate.lock().unwrap().epoch);
        self.core.network(Network::Joined(Box::new(joining)), self.now).unwrap();
        peer
    }

    pub(crate) fn send(&mut self, peer: &FakePeer, message: Wire) {
        self.deliver(peer, message);
        self.work();
    }

    pub(crate) fn deliver(&mut self, peer: &FakePeer, message: Wire) {
        self.core
            .network(
                Network::Message { id: peer.id.clone(), generation: 7, message, bytes: 64, permit: Vec::new() },
                self.now,
            )
            .unwrap();
    }

    pub(crate) fn work(&mut self) {
        while self.core.has_work() {
            self.core.work().unwrap();
        }
    }

    pub(crate) fn command(&mut self, command: Command) {
        assert!(self.core.command(command, self.now).unwrap());
    }

    pub(crate) fn advance(&mut self, duration: Duration) {
        self.now += duration;
        self.core.tick(self.now, 0).unwrap();
    }

    pub(crate) fn drain(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    pub(crate) fn access(&self) -> crate::moderation::AccessState {
        self.core.access_state(self.now).unwrap()
    }
}

pub(crate) fn welcome_of(peer: &mut FakePeer) -> (u32, u32, Snapshot) {
    let frames = peer.frames();
    let welcome: Vec<(Wire, Kind, usize)> = frames.into_iter().filter(|(_, kind, _)| *kind == Kind::Welcome).collect();
    let Some((Wire::MapStart { participant, id_start, units, .. }, _, _)) = welcome.first() else { panic!("welcome") };
    let map = map_of(&welcome);
    assert_eq!(map.0.len() as u64, *units);
    (*participant, *id_start, map)
}

pub(crate) fn map_of(frames: &[(Wire, Kind, usize)]) -> Snapshot {
    let mut map = Snapshot::default();
    for (message, _, _) in frames {
        if let Wire::MapPart(part) = message {
            map.0.extend(part.0.clone());
        }
    }
    assert!(frames.iter().any(|(message, _, _)| matches!(message, Wire::MapEnd)), "complete map transfer");
    map
}

pub(crate) fn point_edit(map: &Snapshot, key: &str, origin: &str) -> Vec<crate::Change> {
    let mut changed = map.clone();
    let unit = changed.0.get_mut(key).unwrap();
    let start = unit.vmf.find("\"origin\" \"").unwrap() + 10;
    let end = start + unit.vmf[start..].find('"').unwrap();
    unit.vmf.replace_range(start..end, origin);
    map.diff(&changed)
}

pub(crate) fn chained(map: &mut Snapshot, key: &str, origin: &str) -> Vec<crate::Change> {
    let changes = point_edit(map, key, origin);
    map.0.insert(key.into(), changes[0].after.clone().unwrap());
    changes
}

pub(crate) async fn direct_endpoint() -> Endpoint {
    Endpoint::builder(presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .clear_address_lookup()
        .bind()
        .await
        .unwrap()
}

pub(crate) fn start_direct_host(map: Snapshot, password: &str, policy: AccessPolicy) -> (Session, Invitation, u16) {
    let host = Session::host(
        map,
        "Host".into(),
        Zeroizing::new(password.into()),
        HostOptions { room: Room { name: "Test".into() }, mode: ConnectionMode::Direct, port: 0, policy },
    )
    .unwrap();
    let started = Instant::now();
    loop {
        assert!(started.elapsed() < DEADLINE);
        if let Ok(Event::Ready { invitation: Some(code), direct_addresses, .. }) = host.events.try_recv() {
            let port = direct_addresses.iter().find(|address| address.is_ipv4()).unwrap().port();
            return (host, Invitation::decode(&code).unwrap(), port);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) struct RawClient {
    pub(crate) endpoint: Endpoint,
    pub(crate) connection: Connection,
    pub(crate) send: SendStream,
    pub(crate) recv: RecvStream,
}

impl RawClient {
    pub(crate) async fn connect(invitation: &Invitation, port: u16, alpn: &[u8]) -> Self {
        let endpoint = direct_endpoint().await;
        let address = EndpointAddr::new(invitation.endpoint.parse().unwrap())
            .with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
        let connection = endpoint.connect(address, alpn).await.unwrap();
        let (send, recv) = connection.open_bi().await.unwrap();
        Self { endpoint, connection, send, recv }
    }

    pub(crate) async fn write(&mut self, message: &Wire) {
        write_frame(&mut self.send, &Frame::encode(message).unwrap(), DEADLINE).await.unwrap();
    }

    pub(crate) async fn read(&mut self) -> anyhow::Result<Wire> {
        decode(&read_frame(&mut self.recv, MAX_FRAME, DEADLINE, DEADLINE).await?)
    }

    pub(crate) async fn authenticate(
        &mut self,
        invitation: &Invitation,
        password: &str,
        name: &str,
        identity: [u8; 32],
    ) {
        self.write(&Wire::Hello { version: protocol::VERSION }).await;
        let Wire::Challenge { nonce } = self.read().await.unwrap() else { panic!("challenge") };
        let password = Zeroizing::new(password.to_owned());
        let identity = Zeroizing::new(identity);
        let message = ClientHandshake {
            connection: &self.connection,
            invitation,
            password: &password,
            identity: &identity,
            client: self.endpoint.id().to_string(),
            name: name.into(),
            session_name: invitation.room.name.clone(),
        }
        .authenticate(&nonce)
        .unwrap();
        self.write(&message).await;
    }

    pub(crate) async fn finish(self) {
        self.connection.close(0u32.into(), b"Test complete");
        self.endpoint.close().await;
    }
}

pub(crate) fn wait_for(session: &Session, description: &str, mut found: impl FnMut(&Event) -> bool) -> Vec<Event> {
    let started = Instant::now();
    let mut seen = Vec::new();
    loop {
        while let Ok(event) = session.events.try_recv() {
            let done = found(&event);
            seen.push(event);
            if done {
                return seen;
            }
        }
        assert!(started.elapsed() < Duration::from_secs(45), "timeout: {description}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[tokio::test]
async fn a_progressing_frame_can_exceed_one_payload_idle_timeout() {
    let server = Endpoint::builder(presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .clear_address_lookup()
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let client = direct_endpoint().await;
    let port = server.addr().ip_addrs().find(|address| address.is_ipv4()).unwrap().port();
    let address = EndpointAddr::new(server.id()).with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
    let sender = async {
        let connection = client.connect(address, ALPN).await.unwrap();
        let (mut send, _) = connection.open_bi().await.unwrap();
        let bytes = serde_json::to_vec(&Wire::Ping(97)).unwrap();
        send.write_all(&(bytes.len() as u32).to_be_bytes()).await.unwrap();
        for byte in bytes {
            send.write_all(&[byte]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        send.finish().unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), send.stopped()).await;
    };
    let receiver = async {
        let connection = server.accept().await.unwrap().await.unwrap();
        let (_, mut recv) = connection.accept_bi().await.unwrap();
        let start = Instant::now();
        let bytes = read_frame(&mut recv, 1024, Duration::from_secs(2), Duration::from_millis(200)).await.unwrap();
        assert!(matches!(decode(&bytes).unwrap(), Wire::Ping(97)));
        assert!(start.elapsed() > Duration::from_millis(200));
        assert_eq!(recv.read(&mut [0; 1]).await.unwrap(), None);
    };
    tokio::join!(sender, receiver);
    client.close().await;
    server.close().await;
}

#[test]
fn editor_updates_coalesce_without_displacing_ordered_map_changes() {
    let (editor, events) = test_editor(2);
    for index in 0..256 {
        event(&editor, Event::Traffic { sent: index, received: index + 1 }).unwrap();
        event(&editor, Event::Participants(Vec::new())).unwrap();
        event(&editor, Event::Bans(Vec::new())).unwrap();
        event(&editor, Event::Access(Default::default())).unwrap();
        event(&editor, Event::NetworkPrivacy { relay_only: true, published_ip_count: 0, direct_path_count: 0 })
            .unwrap();
    }
    for sequence in 1..=2 {
        event(&editor, Event::Applied { author: 1, sequence, revision: sequence, changes: Vec::new() }).unwrap();
    }
    assert_eq!(editor.latest.lock().unwrap().len(), 5);
    for (event, expected) in events.try_iter().take(2).zip(1..=2) {
        assert!(matches!(event, Event::Applied { sequence, .. } if sequence == expected));
    }
    assert!(matches!(events.try_recv().unwrap(), Event::Participants(_)));
    assert!(matches!(events.try_recv().unwrap(), Event::Traffic { sent: 255, received: 256 }));
    event(&editor, Event::Closed).unwrap();
    assert!(matches!(events.try_recv().unwrap(), Event::Closed));
    assert!(events.try_recv().is_err());
}

#[test]
fn a_slow_editor_applies_backpressure_instead_of_ending_the_session() {
    let (editor, events) = test_editor_limited(1 << 16, 16 * 1024 * 1024);
    let unit = crate::document::Unit { owner: 0, hidden: false, vmf: "x".repeat(1024 * 1024) };
    let change = crate::Change { key: "entity:2".into(), before: None, after: Some(unit) };
    let mut accepted = 0;
    while !editor.busy() {
        event(&editor, Event::Applied { author: 2, sequence: 1, revision: 1, changes: vec![change.clone()] }).unwrap();
        accepted += 1;
        assert!(accepted < 1000);
    }
    assert!((15..=16).contains(&accepted), "{accepted}");
    for _ in 0..4 {
        event(&editor, Event::Applied { author: 2, sequence: 1, revision: 1, changes: vec![change.clone()] }).unwrap();
    }
    event(&editor, Event::Error("still delivered".into())).unwrap();
    let mut received = 0;
    while let Ok(event) = events.try_recv() {
        received += 1;
        if received == accepted + 5 {
            assert!(matches!(event, Event::Error(_)));
        }
    }
    assert_eq!(received, accepted + 5);
    assert_eq!(editor.queued.load(std::sync::atomic::Ordering::Acquire), 0);
    assert!(!editor.busy());
    let (editor, events) = test_editor(3);
    for sequence in 0..3 {
        event(&editor, Event::Applied { author: 2, sequence, revision: 1, changes: Vec::new() }).unwrap();
    }
    assert!(editor.busy(), "the event count is bounded too");
    events.try_recv().unwrap();
    assert!(!editor.busy());
}

#[test]
fn host_editor_backlog_pauses_guest_edits_until_the_editor_catches_up() {
    let map = fixture_map();
    let now = Instant::now();
    let gate = Arc::new(Mutex::new(Gate::new(test_invitation(), "test-password", now)));
    let (editor, events) = test_editor(4);
    let core = HostCore::new(
        gate.clone(),
        editor,
        ConnectionMode::Direct,
        ValidatedSnapshot::new(map.clone()).unwrap(),
        "Host".into(),
        AccessPolicy::default(),
        now,
    )
    .unwrap();
    let mut host = Host { core, events, gate, now };
    let mut peer = host.join("peer", "device", "Guest");
    welcome_of(&mut peer);
    let mut state = map.clone();
    for sequence in 1..=10 {
        host.deliver(
            &peer,
            Wire::Edit { sequence, changes: chained(&mut state, "entity:2", &format!("{sequence} 0 0")) },
        );
    }
    host.work();
    assert_eq!(host.core.revision(), 4, "the host stops taking edits while its editor is behind");
    assert_eq!(host.core.deferred_count(), 6);
    assert_eq!(host.core.peer_count(), 1);
    for _ in 0..4 {
        host.events.try_recv().unwrap();
    }
    host.work();
    assert_eq!(host.core.revision(), 8);
    host.drain();
    host.work();
    assert_eq!(host.core.revision(), 10);
    assert!(peer.frames().iter().filter(|(message, _, _)| matches!(message, Wire::Acked { .. })).count() == 10);
}

#[test]
fn commands_never_fail_while_the_session_task_is_busy_and_presence_keeps_the_latest_value() {
    let (commands, presence, mut receiver) = command_channel();
    for sequence in 0..10_000 {
        presence.send(Some(Presence { position: [sequence as f64, 0.0, 0.0], ..Presence::default() })).unwrap();
        if sequence % 100 == 0 {
            commands.send(Command::Edit { sequence, changes: Vec::new() }).unwrap();
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
        let mut edits = 0;
        let mut positions = Vec::new();
        while let Ok(Some(command)) = tokio::time::timeout(Duration::from_millis(50), receiver.next()).await {
            match command {
                Command::Edit { .. } => edits += 1,
                Command::Presence(presence) => positions.push(presence.position[0]),
                _ => panic!("unexpected command"),
            }
        }
        assert_eq!(edits, 100);
        assert_eq!(positions, vec![9999.0], "presence is a latest value, never a backlog");
    });
}

#[test]
fn shared_welcome_and_reset_frames_match_the_serde_encoding() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    let (participant, id_start, map) = welcome_of(&mut peer);
    assert_eq!(map, fixture_map());
    assert_eq!(participant, 2);
    assert_eq!(id_start, host.core.host_range + crate::document::ID_STRIDE);
    assert!(host.core.map_cached());
    host.advance(Duration::ZERO);
    assert!(!host.core.map_cached(), "the shared map frame is released once every guest has it");
    let mut second = host.join("peer2", "device2", "Guest2");
    let mut third = host.join("peer3", "device3", "Guest3");
    host.advance(Duration::ZERO);
    assert!(host.core.map_cached(), "frames still waiting to be sent keep sharing one serialization");
    welcome_of(&mut second);
    welcome_of(&mut third);
    host.advance(Duration::ZERO);
    assert!(!host.core.map_cached());
    let restored = Snapshot::from_vmf("world {\"id\" \"1\" \"classname\" \"worldspawn\"}").unwrap();
    let root = std::env::temp_dir().join(format!("hp-collab-reset-{}", protocol::random_secret().unwrap()[0]));
    let store = crate::snapshots::SnapshotStore::new(root.clone());
    host.command(Command::Restore { map: restored.clone(), store });
    let frames = peer.frames();
    assert!(matches!(&frames[0].0, Wire::Reset { revision: 1, .. }));
    assert_eq!(map_of(&frames), restored);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn slow_initial_download_keeps_the_session_and_latest_presence() {
    let mut text = String::from("world {\"id\" \"1\"} entity {\"id\" \"2\" \"classname\" \"info_target\"");
    for index in 0..2100 {
        text.push_str(&format!("\"field{index}\" \"{}\"", "x".repeat(16_384)));
    }
    text.push('}');
    let map = Snapshot::from_vmf(&text).unwrap();
    let password = "slow-download-password";
    let (host, invitation, port) = start_direct_host(map.clone(), password, AccessPolicy::default());
    let mut client = RawClient::connect(&invitation, port, ALPN).await;
    client.authenticate(&invitation, password, "Guest", [72; 32]).await;
    for index in 0..80 {
        host.send(Command::Presence(Presence { position: [index as f64, 0.0, 0.0], ..Default::default() })).unwrap();
        while let Ok(event) = host.events.try_recv() {
            assert!(!matches!(event, Event::Error(_) | Event::Closed));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let Wire::MapStart { units, .. } = client.read().await.unwrap() else { panic!("map transfer") };
    let mut received = Snapshot::default();
    loop {
        match client.read().await.unwrap() {
            Wire::MapPart(part) => received.0.extend(part.0),
            Wire::MapEnd => break,
            _ => panic!("unexpected map transfer message"),
        }
    }
    assert_eq!(units, map.0.len() as u64);
    assert_eq!(received, map);
    client.write(&Wire::Ping(57)).await;
    let (mut pong, mut presence) = (false, false);
    while !pong || !presence {
        match client.read().await.unwrap() {
            Wire::Pong(57) => pong = true,
            Wire::Participants(rows) => {
                presence |= rows.iter().any(|row| row.id == 1 && row.presence.position[0] == 79.0)
            }
            _ => panic!("unexpected message"),
        }
    }
    client.finish().await;
}

#[test]
fn legitimate_direct_session_with_approval_and_viewer_roles_still_converges() {
    let map = fixture_map();
    let policy = AccessPolicy { approval: true, read_only_guests: true, ..AccessPolicy::default() };
    let (host, invitation, port) = start_direct_host(map.clone(), "approval-password", policy);
    let guest = Session::join(
        invitation.clone(),
        "Guest".into(),
        Zeroizing::new("approval-password".into()),
        JoinOptions {
            identity: Zeroizing::new([41; 32]),
            session_name: "Test".into(),
            direct_address: Some(([127, 0, 0, 1], port).into()),
        },
    )
    .unwrap();
    wait_for(&guest, "waiting room", |event| matches!(event, Event::Waiting));
    let mut pending = None;
    wait_for(&host, "pending row", |event| {
        if let Event::Access(state) = event
            && let Some(row) = state.pending.first()
        {
            pending = Some(row.id);
        }
        pending.is_some()
    });
    host.send(Command::Admit(pending.unwrap())).unwrap();
    let mut participant = 0;
    wait_for(&guest, "admitted", |event| {
        if let Event::Ready { participant: id, .. } = event {
            participant = *id;
            true
        } else {
            false
        }
    });
    let changes = point_edit(&map, "entity:2", "16 0 0");
    guest.send(Command::Edit { sequence: 1, changes: changes.clone() }).unwrap();
    wait_for(
        &guest,
        "viewer rejection",
        |event| matches!(event, Event::Rejected { sequence: 1, rejection, .. } if rejection.code == protocol::EditRefusal::ReadOnly),
    );
    host.send(Command::SetRole { participant, role: Role::Editor }).unwrap();
    wait_for(
        &guest,
        "editor role",
        |event| matches!(event, Event::Participants(rows) if rows.iter().any(|row| row.id == participant && row.role == Role::Editor)),
    );
    guest.send(Command::Edit { sequence: 2, changes }).unwrap();
    wait_for(
        &host,
        "edit applied on host",
        |event| matches!(event, Event::Applied { author, .. } if *author == participant),
    );
    wait_for(&guest, "edit acknowledged", |event| matches!(event, Event::Applied { sequence: 2, .. }));
    drop(guest);
    host.send(Command::Stop).unwrap();
    wait_for(&host, "host closed", |event| matches!(event, Event::Closed));
}

#[test]
fn guests_see_a_translatable_refusal_when_the_host_ends_or_bans() {
    let (host, invitation, port) = start_direct_host(fixture_map(), "refusal-password", AccessPolicy::default());
    let join = |identity: u8, name: &str| {
        Session::join(
            invitation.clone(),
            name.into(),
            Zeroizing::new("refusal-password".into()),
            JoinOptions {
                identity: Zeroizing::new([identity; 32]),
                session_name: "Test".into(),
                direct_address: Some(([127, 0, 0, 1], port).into()),
            },
        )
        .unwrap()
    };
    let guest = join(1, "Guest");
    let mut participant = 0;
    wait_for(&guest, "ready", |event| {
        if let Event::Ready { participant: id, .. } = event {
            participant = *id;
        }
        participant != 0
    });
    host.send(Command::Ban { participant, seconds: Some(60), reason: "griefing".into() }).unwrap();
    wait_for(
        &guest,
        "ban refusal",
        |event| matches!(event, Event::Denied(refused) if refused.refusal == Refusal::Banned && refused.detail == "griefing"),
    );
    let again = join(1, "Guest");
    wait_for(
        &again,
        "ban persists",
        |event| matches!(event, Event::Denied(refused) if refused.refusal == Refusal::Banned),
    );
    host.send(Command::Unban(1)).unwrap();
    wait_for(&host, "unbanned", |event| matches!(event, Event::Bans(rows) if rows.is_empty()));
    let wrong = Session::join(
        invitation.clone(),
        "Other".into(),
        Zeroizing::new("wrong-password".into()),
        JoinOptions {
            identity: Zeroizing::new([2; 32]),
            session_name: "Test".into(),
            direct_address: Some(([127, 0, 0, 1], port).into()),
        },
    )
    .unwrap();
    wait_for(
        &wrong,
        "wrong password",
        |event| matches!(event, Event::Denied(refused) if refused.refusal == Refusal::AuthenticationFailed),
    );
    let other = join(3, "Third");
    wait_for(&other, "ready", |event| matches!(event, Event::Ready { .. }));
    host.send(Command::Stop).unwrap();
    wait_for(&other, "ended", |event| matches!(event, Event::Denied(refused) if refused.refusal == Refusal::Ended));
}

#[test]
#[ignore = "prints the size of the participant frames with and without activity, chat and ping"]
fn presence_frame_sizes_profile() {
    let rows = |activity: bool, note: bool| -> Vec<Participant> {
        (1..=8)
            .map(|id| Participant {
                id,
                role: if id == 1 { Role::Host } else { Role::Editor },
                name: format!("Mapper{id}"),
                latency_ms: 40 + u64::from(id),
                presence: Presence {
                    position: [1024.5 * f64::from(id), -2048.25, 320.0],
                    direction: [0.6, 0.8, 0.0],
                    selected: (0..10).map(|index| format!("solid:{}", 100_000 * id + index)).collect(),
                    notes: if note {
                        vec![crate::protocol::Note {
                            serial: 3,
                            text: "Can someone check the lighting near the north door?".into(),
                            marker: Some([512.0, 256.0, 64.0]),
                        }]
                    } else {
                        Vec::new()
                    },
                    activity: if activity {
                        crate::protocol::Activity { upload: 1200, download: 3500, round_trip_ms: 42 }
                    } else {
                        crate::protocol::Activity::default()
                    },
                },
            })
            .collect()
    };
    for (label, activity, note) in [
        ("camera and selection", false, false),
        ("plus activity", true, false),
        ("plus a chat line with a ping", true, true),
    ] {
        let frame = Frame::pack(&Wire::Participants(rows(activity, note)), super::frame::FAST).unwrap();
        let single = Frame::pack(&Wire::Presence(rows(activity, note).remove(0).presence), super::frame::FAST).unwrap();
        eprintln!(
            "8 participants, {label}: Participants frame {} bytes, one Presence frame {} bytes",
            frame.wire_bytes(),
            single.wire_bytes()
        );
    }
}

#[test]
fn compressed_frames_round_trip_and_lying_sizes_are_refused() {
    use super::frame::{FAST, edit_sequence, inflate, inflate_prefix};
    let unit = crate::document::Unit { owner: 0, hidden: false, vmf: "x".repeat(100_000) };
    let change = crate::Change { key: "entity:2".into(), before: None, after: Some(unit) };
    let frame = Frame::pack(&Wire::Edit { sequence: 77, changes: vec![change] }, FAST).unwrap();
    assert!(frame.compressed() && frame.wire_bytes() < 10_000, "{}", frame.wire_bytes());
    assert!(matches!(decode(&frame.payload()).unwrap(), Wire::Edit { sequence: 77, .. }));
    let raw = frame.concat();
    let json = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    assert!(inflate(&raw[4..], json - 1).is_err() && inflate(&raw[4..], json + 1).is_err());
    assert_eq!(edit_sequence(&inflate_prefix(&raw[4..raw.len().min(4100)])), Some(77));
    assert!(!Frame::pack(&Wire::Ping(1), FAST).unwrap().compressed());
}

#[test]
fn large_maps_stream_in_compressed_parts_that_rebuild_the_map() {
    let mut text = String::from("world {\"id\" \"1\" \"classname\" \"worldspawn\"}");
    for id in 2..4002 {
        text.push_str(&format!(
            "entity {{\"id\" \"{id}\" \"classname\" \"info_target\" \"origin\" \"{id} 0 0\" \"targetname\" \"{}\"}}",
            "spot".repeat(64)
        ));
    }
    let map = Snapshot::from_vmf(&text).unwrap();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    let item = peer.queue.try_pop().unwrap();
    let frames = item.frames();
    assert!(frames.len() > 3, "several parts");
    assert!(frames[1..frames.len() - 1].iter().all(Frame::compressed));
    let total: u64 = frames.iter().map(Frame::wire_bytes).sum();
    assert!(total * 4 < serde_json::to_vec(&map).unwrap().len() as u64, "compressed transfer {total}");
    let decoded: Vec<(Wire, Kind, usize)> =
        frames.iter().map(|frame| (decode(&frame.payload()).unwrap(), Kind::Welcome, 0)).collect();
    assert_eq!(map_of(&decoded), map);
    assert!(peer.frames().is_empty());
}
