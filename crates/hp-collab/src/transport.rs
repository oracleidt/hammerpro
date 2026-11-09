use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};
use iroh::{
    Endpoint, EndpointAddr, EndpointId,
    endpoint::{Connection, RecvStream, SendStream, presets},
};
use tokio::sync::{Semaphore, mpsc as channel};
use zeroize::Zeroizing;

use crate::document::{Change, ID_STRIDE, MAX_CHANGE_BYTES, MAX_MAP_BYTES, Snapshot};
use crate::moderation::{BanList, BanRow};
use crate::protocol::{self, ConnectionMode, Invitation, Participant, Presence, Room, Wire};

const MAX_FRAME: usize = MAX_MAP_BYTES + 16 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(30);

pub enum Command {
    Restore { map: Snapshot, store: crate::snapshots::SnapshotStore },
    Edit { sequence: u64, changes: Vec<Change> },
    Presence(Presence),
    Rotate(Zeroizing<String>),
    Kick(u32),
    Ban { participant: u32, seconds: Option<u64>, reason: String },
    Unban(u64),
    Stop,
}

pub enum Event {
    Reset {
        revision: u64,
        map: Snapshot,
    },
    Ready {
        invitation: Option<String>,
        participant: u32,
        id_start: u32,
        revision: u64,
        map: Snapshot,
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
        reason: String,
        revision: u64,
        map: Snapshot,
    },
    Participants(Vec<Participant>),
    Bans(Vec<BanRow>),
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
    Error(String),
    Closed,
}

pub struct Session {
    commands: channel::Sender<Command>,
    pub events: mpsc::Receiver<Event>,
}

pub struct HostOptions {
    pub room: Room,
    pub mode: ConnectionMode,
    pub port: u16,
}

pub struct JoinOptions {
    pub identity: Zeroizing<[u8; 32]>,
    pub session_name: String,
    pub direct_address: Option<std::net::SocketAddr>,
}

impl Session {
    pub fn host(map: Snapshot, name: String, password: Zeroizing<String>, options: HostOptions) -> Result<Self> {
        map.validate()?;
        ensure!(protocol::valid_name(&name), "Invalid participant name");
        ensure!((8..=256).contains(&password.len()), "Use at least 8 password characters");
        options.room.validate()?;
        Self::spawn(move |rx, tx| Box::pin(host(map, name, password, options, rx, tx)))
    }

    pub fn join(
        invitation: Invitation,
        name: String,
        password: Zeroizing<String>,
        options: JoinOptions,
    ) -> Result<Self> {
        ensure!(protocol::valid_name(&name) && (8..=256).contains(&password.len()), "Invalid participant details");
        ensure!(invitation.room.name == options.session_name, "Session name does not match the invitation");
        ensure!(
            invitation.mode != ConnectionMode::Direct || options.direct_address.is_some(),
            "Direct mode requires the host IP and port"
        );
        Self::spawn(move |rx, tx| Box::pin(join(invitation, name, password, options, rx, tx)))
    }

    fn spawn<F>(run: F) -> Result<Self>
    where
        F: FnOnce(
                channel::Receiver<Command>,
                mpsc::SyncSender<Event>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
            + Send
            + 'static,
    {
        let (commands, rx) = channel::channel(32);
        let (tx, events) = mpsc::sync_channel(64);
        std::thread::Builder::new().name("map-collaboration".into()).spawn(move || {
            match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
                Ok(runtime) => {
                    if let Err(error) = runtime.block_on(run(rx, tx.clone())) {
                        let _ = tx.try_send(Event::Error(error.to_string()));
                    }
                }
                Err(error) => {
                    let _ = tx.try_send(Event::Error(error.to_string()));
                }
            }
            let _ = tx.try_send(Event::Closed);
        })?;
        Ok(Self { commands, events })
    }

    pub fn send(&self, command: Command) -> Result<()> {
        if let Command::Rotate(password) = &command {
            ensure!((8..=256).contains(&password.len()), "Use at least 8 password characters");
        }
        self.commands.try_send(command).map_err(|_| anyhow::anyhow!("Session busy or disconnected"))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.commands.try_send(Command::Stop);
    }
}

async fn write(stream: &mut SendStream, message: &Wire) -> Result<u64> {
    let bytes = serde_json::to_vec(message)?;
    ensure!(bytes.len() <= MAX_FRAME, "Outgoing message too large");
    stream.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    tokio::time::timeout(DEADLINE, stream.write_all(&bytes)).await??;
    Ok(bytes.len() as u64 + 4)
}

async fn read(stream: &mut RecvStream, limit: usize) -> Result<(Wire, u64)> {
    let mut size = [0; 4];
    tokio::time::timeout(DEADLINE, stream.read_exact(&mut size)).await??;
    let size = u32::from_be_bytes(size) as usize;
    ensure!(size > 0 && size <= limit, "Incoming message too large");
    let mut bytes = vec![0; size];
    tokio::time::timeout(DEADLINE, stream.read_exact(&mut bytes)).await??;
    Ok((serde_json::from_slice(&bytes)?, size as u64 + 4))
}

fn event(tx: &mpsc::SyncSender<Event>, value: Event) -> Result<()> {
    tx.try_send(value).map_err(|_| anyhow::anyhow!("Editor is not consuming session updates"))
}

struct Authentication {
    invitation: Invitation,
    key: Zeroizing<[u8; 32]>,
    epoch: u64,
}

enum Network {
    Joined {
        id: String,
        device: String,
        name: String,
        epoch: u64,
        connection: Connection,
        outgoing: channel::Sender<Wire>,
    },
    Message {
        id: String,
        generation: usize,
        message: Wire,
        bytes: u64,
    },
    Left {
        id: String,
        generation: usize,
    },
    Sent(u64),
}

async fn serve_peer(
    connection: Connection,
    auth: Arc<Mutex<Authentication>>,
    network: channel::Sender<Network>,
) -> Result<()> {
    let (mut send, mut recv) = tokio::time::timeout(Duration::from_secs(10), connection.accept_bi()).await??;
    let (Wire::Hello { version: protocol::VERSION }, _) = read(&mut recv, 128).await? else {
        bail!("Unsupported protocol");
    };
    let nonce = protocol::random_secret()?;
    write(&mut send, &Wire::Challenge { nonce }).await?;
    let (message, _) = tokio::time::timeout(Duration::from_secs(10), read(&mut recv, 2048)).await??;
    let Wire::Authenticate { version, name, session_name, device, signature, proof } = message else {
        bail!("Authentication required");
    };
    ensure!(version == protocol::VERSION && protocol::valid_name(&name), "Authentication failed");
    let id = connection.remote_id().to_string();
    let generation = connection.stable_id();
    let device_id: iroh::PublicKey = device.parse()?;
    let signature: [u8; 64] = signature.try_into().map_err(|_| anyhow::anyhow!("Invalid device signature"))?;
    device_id.verify(&proof, &iroh::Signature::from_bytes(&signature))?;
    let epoch = {
        let auth = auth.lock().map_err(|_| anyhow::anyhow!("Session unavailable"))?;
        ensure!(
            auth.invitation.mode != ConnectionMode::Relay
                || connection.paths().iter().all(|path| path.remote_addr().is_relay()),
            "Direct connections are disabled in relay mode"
        );
        ensure!(auth.invitation.room.name == session_name, "Authentication failed");
        let expected = protocol::proof(&auth.key, &nonce, &auth.invitation.endpoint, &id, &name);
        ensure!(protocol::verify_proof(&expected, &proof), "Authentication failed");
        auth.epoch
    };
    let (outgoing, mut outgoing_rx) = channel::channel(8);
    network
        .send(Network::Joined { id: id.clone(), device, name, epoch, connection: connection.clone(), outgoing })
        .await?;
    let reader = async {
        let mut window = Instant::now();
        let (mut messages, mut bytes) = (0, 0);
        loop {
            let (message, size) = read(&mut recv, MAX_CHANGE_BYTES + 64 * 1024).await?;
            if window.elapsed() >= Duration::from_secs(1) {
                window = Instant::now();
                messages = 0;
                bytes = 0;
            }
            messages += 1;
            bytes += size;
            ensure!(messages <= 64 && bytes <= 16 * 1024 * 1024, "Session rate limit exceeded");
            network.send(Network::Message { id: id.clone(), generation, message, bytes: size }).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let writer = async {
        while let Some(message) = outgoing_rx.recv().await {
            let closing = matches!(message, Wire::Closed(_));
            let bytes = write(&mut send, &message).await?;
            network.send(Network::Sent(bytes)).await?;
            if closing {
                send.finish()?;
                let _ = tokio::time::timeout(Duration::from_secs(3), send.stopped()).await;
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::select! { result = reader => { let _ = result; }, result = writer => { let _ = result; } }
    connection.close(0u32.into(), b"Session ended");
    let _ = network.send(Network::Left { id, generation }).await;
    Ok(())
}

struct Peer {
    device: String,
    participant: Participant,
    connection: Connection,
    outgoing: channel::Sender<Wire>,
    id_start: u32,
    sequence: u64,
}

fn broadcast(peers: &mut BTreeMap<String, Peer>, make: impl Fn() -> Wire) {
    peers.retain(|_, peer| {
        if peer.outgoing.try_send(make()).is_ok() {
            true
        } else {
            peer.connection.close(1u32.into(), b"Slow participant");
            false
        }
    });
}

fn rows(host: &Participant, peers: &BTreeMap<String, Peer>) -> Vec<Participant> {
    std::iter::once(host.clone()).chain(peers.values().map(|peer| peer.participant.clone())).collect()
}

fn apply(map: &mut Snapshot, changes: &[Change], author: u32, range: u32, participants: &[Participant]) -> Result<()> {
    for change in changes {
        ensure!(
            !participants.iter().any(|peer| peer.id != author && peer.presence.selected.contains(&change.key)),
            "Object selected by another participant"
        );
        if change.before.is_none()
            && let Some(id) = change.key.split(':').nth(1)
        {
            let id: u32 = id.parse()?;
            ensure!((range..range + ID_STRIDE).contains(&id), "Object identifier is outside the participant range");
        }
    }
    let mut updated = map.clone();
    updated.apply(changes)?;
    let identifiers = |map: &Snapshot| -> Result<BTreeSet<u32>> {
        let parsed = hp_vmf::parse(&map.to_vmf()?)?;
        Ok(std::iter::once(&parsed.world)
            .chain(&parsed.entities)
            .flat_map(|entity| entity.solids.iter().flat_map(|solid| solid.sides.iter().map(|side| side.id)))
            .collect())
    };
    let old = identifiers(map)?;
    for id in identifiers(&updated)?.difference(&old) {
        ensure!((range..range + ID_STRIDE).contains(id), "Side identifier is outside the participant range");
    }
    *map = updated;
    Ok(())
}

fn range_start(map: &Snapshot) -> Result<u32> {
    let text = map.to_vmf()?;
    let parsed = hp_vmf::parse(&text)?;
    let mut highest = 1;
    for entity in std::iter::once(&parsed.world).chain(&parsed.entities) {
        highest = highest.max(entity.id);
        for solid in &entity.solids {
            highest = highest.max(solid.id);
            for side in &solid.sides {
                highest = highest.max(side.id);
            }
        }
    }
    let start = (highest / ID_STRIDE + 1) * ID_STRIDE;
    ensure!(start < 1_900_000_000, "Map identifiers too large for a session");
    Ok(start)
}

async fn host(
    mut map: Snapshot,
    name: String,
    password: Zeroizing<String>,
    options: HostOptions,
    mut commands: channel::Receiver<Command>,
    editor: mpsc::SyncSender<Event>,
) -> Result<()> {
    let builder = match options.mode {
        ConnectionMode::Relay => Endpoint::builder(presets::N0).clear_ip_transports(),
        ConnectionMode::Direct => Endpoint::builder(presets::Minimal)
            .relay_mode(iroh::RelayMode::Disabled)
            .clear_address_lookup()
            .bind_addr(std::net::SocketAddr::from(([0, 0, 0, 0], options.port)))?,
    }
    .alpns(vec![protocol::ALPN.to_vec()])
    .transport_config(
        iroh::endpoint::QuicTransportConfig::builder()
            .max_concurrent_bidi_streams(1u32.into())
            .max_concurrent_uni_streams(0u32.into())
            .build(),
    );
    let endpoint = builder.bind().await?;
    if options.mode == ConnectionMode::Relay {
        ensure!(endpoint.addr().ip_addrs().next().is_none(), "Direct address publication is disabled");
        let _ = tokio::time::timeout(Duration::from_secs(10), endpoint.online()).await;
    }
    let relay = endpoint.addr().relay_urls().next().map(ToString::to_string);
    let invitation = Invitation {
        version: protocol::VERSION,
        endpoint: endpoint.id().to_string(),
        relay,
        token: protocol::random_secret()?,
        room: options.room,
        mode: options.mode,
    };
    let key = invitation.credential(&password);
    let auth = Arc::new(Mutex::new(Authentication { invitation: invitation.clone(), key, epoch: 0 }));
    let (network, mut network_rx) = channel::channel(128);
    let accepting_endpoint = endpoint.clone();
    let auth_accept = auth.clone();
    let acceptor = tokio::spawn(async move {
        let permits = Arc::new(Semaphore::new(16));
        while let Some(incoming) = accepting_endpoint.accept().await {
            let Ok(permit) = permits.clone().try_acquire_owned() else {
                incoming.refuse();
                continue;
            };
            let (auth, network) = (auth_accept.clone(), network.clone());
            tokio::spawn(async move {
                let _permit = permit;
                if let Ok(Ok(connection)) = tokio::time::timeout(Duration::from_secs(10), incoming).await {
                    let retained = connection.clone();
                    if serve_peer(connection, auth, network).await.is_err() {
                        retained.close(1u32.into(), b"Authentication or protocol error");
                    }
                }
            });
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    let host_range = range_start(&map)?;
    let mut next_range = host_range + ID_STRIDE;
    let mut next_participant = 2;
    let mut revision = 0;
    let mut peers = BTreeMap::<String, Peer>::new();
    let mut bans = BanList::default();
    let mut host_row = Participant { id: 1, name, presence: Presence::default(), latency_ms: 0 };
    event(
        &editor,
        Event::Ready {
            invitation: Some(invitation.encode()?),
            participant: 1,
            id_start: host_range,
            revision,
            map: map.clone(),
            direct_addresses: endpoint.addr().ip_addrs().copied().collect(),
        },
    )?;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let (mut sent, mut received) = (0, 0);
    let mut last_participants = Vec::new();
    let mut last_traffic = Instant::now();
    let result = async {
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    None | Some(Command::Stop) => break,
                    Some(Command::Restore { map: restored, store }) => {
                        let name = auth.lock().map_err(|_| anyhow::anyhow!("Session unavailable"))?.invitation.room.name.clone();
                        let backup = restored.validate().and_then(|()| store.save(&map, &name, revision).map(|_| ()));
                        if let Err(error) = backup { event(&editor, Event::Error(format!("Restore cancelled; safety snapshot failed: {error}")))?; continue; }
                        map = restored;
                        revision += 1;
                        broadcast(&mut peers, || Wire::Reset { revision, map: map.clone() });
                        event(&editor, Event::Reset { revision, map: map.clone() })?;
                    }
                    Some(Command::Presence(presence)) => { if presence.validate() { host_row.presence = presence; } }
                    Some(Command::Kick(id)) => { peers.retain(|_, peer| { if peer.participant.id == id { let _ = peer.outgoing.try_send(Wire::Closed("Removed by the host. You may reconnect with valid credentials.".into())); false } else { true } }); }
                    Some(Command::Ban { participant, seconds, reason }) => {
                        ensure!(reason.len() <= 256 && !reason.chars().any(char::is_control) && seconds.is_none_or(|seconds| (1..=604800).contains(&seconds)), "Invalid ban duration or reason");
                        if let Some(endpoint) = peers.iter().find(|(_, peer)| peer.participant.id == participant).map(|(id, _)| id.clone())
                            && let Some(peer) = peers.remove(&endpoint) {
                                let reason = if reason.trim().is_empty() { "Banned by the host".into() } else { reason };
                                let ip = observed_ip(&peer.connection);
                                bans.add(peer.device.clone(), ip, peer.participant.name, reason, seconds.map(Duration::from_secs), Instant::now());
                                let reason = bans.denied(&peer.device, ip, Instant::now()).unwrap_or_else(|| "Banned".into());
                                let _ = peer.outgoing.try_send(Wire::Closed(reason));
                        }
                    }
                    Some(Command::Unban(id)) => { bans.remove(id); }
                    Some(Command::Rotate(password)) => {
                        ensure!((8..=256).contains(&password.len()), "Use at least 8 password characters");
                        let encoded = {
                            let mut auth = auth.lock().map_err(|_| anyhow::anyhow!("Session unavailable"))?;
                            auth.invitation.token = protocol::random_secret()?;
                            auth.key = auth.invitation.credential(&password);
                            auth.epoch += 1;
                            auth.invitation.encode()?
                        };
                        for (_, peer) in std::mem::take(&mut peers) { peer.connection.close(0u32.into(), b"Host rotated access credentials"); }
                        event(&editor, Event::Invitation(encoded))?;
                    }
                    Some(Command::Edit { sequence, changes }) => {
                        match apply(&mut map, &changes, 1, host_range, &rows(&host_row, &peers)) {
                            Ok(()) => { revision += 1; broadcast(&mut peers, || Wire::Applied { author: 1, sequence, revision, changes: changes.clone() }); event(&editor, Event::Applied { author: 1, sequence, revision, changes })?; }
                            Err(error) => event(&editor, Event::Rejected { sequence, reason: error.to_string(), revision, map: map.clone() })?,
                        }
                    }
                },
                message = network_rx.recv() => match message {
                    Some(Network::Joined { id, device, name, epoch, connection, outgoing }) => {
                        let current_epoch = auth.lock().map_err(|_| anyhow::anyhow!("Session unavailable"))?.epoch;
                        if let Some(reason) = bans.denied(&device, observed_ip(&connection), Instant::now()) { let _ = outgoing.try_send(Wire::Closed(reason)); continue; }
                        if peers.len() >= protocol::MAX_PARTICIPANTS - 1 || peers.contains_key(&id) || epoch != current_epoch || next_range >= 1_990_000_000 { let _ = outgoing.try_send(Wire::Closed("Session full or access revoked".into())); continue; }
                        if peers.values().any(|peer| peer.participant.name.eq_ignore_ascii_case(&name)) || host_row.name.eq_ignore_ascii_case(&name) { let _ = outgoing.try_send(Wire::Closed("Nickname already in use".into())); continue; }
                        let participant = next_participant; next_participant += 1;
                        let id_start = next_range; next_range += ID_STRIDE;
                        outgoing.try_send(Wire::Welcome { participant, id_start, revision, map: map.clone() })?;
                        peers.insert(id, Peer { device, participant: Participant { id: participant, name, presence: Presence::default(), latency_ms: 0 }, connection, outgoing, id_start, sequence: 0 });
                    }
                    Some(Network::Left { id, generation }) => { if peers.get(&id).is_some_and(|peer| peer.connection.stable_id() == generation) { peers.remove(&id); } }
                    Some(Network::Sent(bytes)) => sent += bytes,
                    Some(Network::Message { id, generation, message, bytes }) => {
                        received += bytes;
                        let Some(peer) = peers.get_mut(&id) else { continue; };
                        if peer.connection.stable_id() != generation { continue; }
                        match message {
                            Wire::Presence(presence) if presence.validate() => { peer.participant.presence = presence; }
                            Wire::Ping(value) => { let _ = peer.outgoing.try_send(Wire::Pong(value)); }
                            Wire::Edit { sequence, changes } => {
                                if sequence <= peer.sequence { peer.connection.close(1u32.into(), b"Invalid sequence"); continue; }
                                peer.sequence = sequence;
                                let (author, range) = (peer.participant.id, peer.id_start);
                                match apply(&mut map, &changes, author, range, &rows(&host_row, &peers)) {
                                    Ok(()) => { revision += 1; broadcast(&mut peers, || Wire::Applied { author, sequence, revision, changes: changes.clone() }); event(&editor, Event::Applied { author, sequence, revision, changes })?; }
                                    Err(error) => { if let Some(peer) = peers.get(&id) { let _ = peer.outgoing.try_send(Wire::Rejected { sequence, reason: error.to_string(), revision, map: map.clone() }); } }
                                }
                            }
                            _ => { peer.connection.close(1u32.into(), b"Invalid session message"); }
                        }
                    }
                    None => bail!("Session network closed"),
                },
                _ = tick.tick() => {
                    for peer in peers.values_mut() { peer.participant.latency_ms = latency(&peer.connection); }
                    let direct_path_count = peers.values().flat_map(|peer| peer.connection.paths().iter().map(|path| usize::from(path.remote_addr().is_ip())).collect::<Vec<_>>()).sum();
                    if options.mode == ConnectionMode::Relay { ensure!(direct_path_count == 0 && endpoint.addr().ip_addrs().next().is_none(), "Direct path detected in relay-only session"); }
                    let participants = rows(&host_row, &peers);
                    if participants != last_participants {
                        broadcast(&mut peers, || Wire::Participants(participants.clone()));
                        event(&editor, Event::Participants(participants.clone()))?;
                        last_participants = participants;
                    }
                    if last_traffic.elapsed() >= Duration::from_secs(1) {
                        event(&editor, Event::Traffic { sent, received })?;
                        event(&editor, Event::Bans(bans.rows(Instant::now())))?;
                        event(&editor, Event::NetworkPrivacy { relay_only: options.mode == ConnectionMode::Relay, published_ip_count: endpoint.addr().ip_addrs().count(), direct_path_count })?;
                        last_traffic = Instant::now();
                    }
                }
            }
        }
        Ok(())
    }.await;
    acceptor.abort();
    for (_, peer) in peers {
        peer.connection.close(0u32.into(), b"Host ended the session");
    }
    endpoint.close().await;
    result
}

async fn join(
    invitation: Invitation,
    name: String,
    password: Zeroizing<String>,
    options: JoinOptions,
    mut commands: channel::Receiver<Command>,
    editor: mpsc::SyncSender<Event>,
) -> Result<()> {
    let builder = match invitation.mode {
        ConnectionMode::Relay => Endpoint::builder(presets::N0).clear_ip_transports(),
        ConnectionMode::Direct => {
            Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::Disabled).clear_address_lookup()
        }
    }
    .transport_config(
        iroh::endpoint::QuicTransportConfig::builder()
            .max_concurrent_bidi_streams(0u32.into())
            .max_concurrent_uni_streams(0u32.into())
            .build(),
    );
    let endpoint = builder.bind().await?;
    let id: EndpointId = invitation.endpoint.parse()?;
    let mut address = EndpointAddr::new(id);
    if let Some(direct) = options.direct_address
        && invitation.mode == ConnectionMode::Direct
    {
        address = address.with_ip_addr(direct);
    }
    if let Some(relay) = &invitation.relay {
        address = address.with_relay_url(relay.parse()?);
    }
    let connection = tokio::time::timeout(DEADLINE, endpoint.connect(address, protocol::ALPN)).await??;
    let (mut send, mut recv) = connection.open_bi().await?;
    if invitation.mode == ConnectionMode::Relay {
        ensure!(
            endpoint.addr().ip_addrs().next().is_none()
                && connection.paths().iter().all(|path| path.remote_addr().is_relay()),
            "Direct connections are disabled"
        );
    }
    write(&mut send, &Wire::Hello { version: protocol::VERSION }).await?;
    let (Wire::Challenge { nonce }, _) = read(&mut recv, 2048).await? else {
        bail!("Invalid session handshake");
    };
    let proof = protocol::proof(
        &invitation.credential(&password),
        &nonce,
        &invitation.endpoint,
        &endpoint.id().to_string(),
        &name,
    );
    let device_seed = blake3::keyed_hash(&options.identity, invitation.endpoint.as_bytes());
    let device_key = iroh::SecretKey::from_bytes(device_seed.as_bytes());
    let device = device_key.public().to_string();
    let signature = device_key.sign(&proof).to_bytes().to_vec();
    let mut sent = write(
        &mut send,
        &Wire::Authenticate {
            version: protocol::VERSION,
            name,
            session_name: options.session_name,
            device,
            signature,
            proof,
        },
    )
    .await?;
    let (welcome, received) = read(&mut recv, MAX_FRAME).await?;
    if let Wire::Closed(reason) = &welcome {
        bail!("{reason}");
    }
    let Wire::Welcome { participant, id_start, revision, map } = welcome else {
        bail!("Invitation or password rejected");
    };
    map.validate()?;
    event(
        &editor,
        Event::Ready { invitation: None, participant, id_start, revision, map, direct_addresses: Vec::new() },
    )?;
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    let start = Instant::now();
    let mut received = received;
    let mut last_revision = revision;
    let (incoming_tx, mut incoming) = channel::channel(16);
    let reader = tokio::spawn(async move {
        loop {
            let result = read(&mut recv, MAX_FRAME).await;
            let failed = result.is_err();
            if incoming_tx.send(result).await.is_err() || failed {
                break;
            }
        }
    });
    let result = async {
        loop {
            tokio::select! {
                command = commands.recv() => {
                    let message = match command {
                        None | Some(Command::Stop) => break,
                        Some(Command::Edit { sequence, changes }) => Wire::Edit { sequence, changes },
                        Some(Command::Presence(presence)) if presence.validate() => Wire::Presence(presence),
                        _ => continue,
                    };
                    sent += write(&mut send, &message).await?;
                }
                message = incoming.recv() => {
                    let (message, bytes) = message.ok_or_else(|| anyhow::anyhow!("Host disconnected"))??; received += bytes;
                    let value = match message {
                        Wire::Reset { revision, map } => { ensure!(revision == last_revision + 1, "Session revision mismatch"); map.validate()?; last_revision = revision; Event::Reset { revision, map } }
                        Wire::Applied { author, sequence, revision, changes } => { ensure!(revision == last_revision + 1, "Session revision mismatch"); last_revision = revision; Event::Applied { author, sequence, revision, changes } }
                        Wire::Rejected { sequence, reason, revision, map } => { map.validate()?; last_revision = revision; Event::Rejected { sequence, reason, revision, map } }
                        Wire::Participants(mut rows) => { ensure!(rows.len() <= protocol::MAX_PARTICIPANTS && rows.iter().all(|row| protocol::valid_name(&row.name) && row.presence.validate()), "Invalid participants"); for row in &mut rows { if row.id == 1 { row.latency_ms = latency(&connection); } } Event::Participants(rows) }
                        Wire::Pong(_) => continue,
                        Wire::Closed(reason) => bail!("{reason}"),
                        _ => bail!("Invalid host message"),
                    };
                    event(&editor, value)?;
                }
                _ = tick.tick() => {
                    sent += write(&mut send, &Wire::Ping(start.elapsed().as_millis() as u64)).await?;
                    event(&editor, Event::Traffic { sent, received })?;
                    let direct_path_count = connection.paths().iter().filter(|path| path.remote_addr().is_ip()).count();
                    if invitation.mode == ConnectionMode::Relay { ensure!(direct_path_count == 0 && endpoint.addr().ip_addrs().next().is_none(), "Direct path detected in relay-only session"); }
                    event(&editor, Event::NetworkPrivacy { relay_only: invitation.mode == ConnectionMode::Relay, published_ip_count: endpoint.addr().ip_addrs().count(), direct_path_count })?;
                }
            }
        }
        Ok(())
    }.await;
    reader.abort();
    connection.close(0u32.into(), b"Participant left");
    endpoint.close().await;
    result
}

fn latency(connection: &Connection) -> u64 {
    connection.paths().iter().find(|path| path.is_selected()).map_or(0, |path| path.rtt().as_millis() as u64)
}

fn observed_ip(connection: &Connection) -> Option<std::net::IpAddr> {
    connection.paths().iter().find_map(|path| match path.remote_addr() {
        iroh::TransportAddr::Ip(address) => Some(address.ip()),
        _ => None,
    })
}
