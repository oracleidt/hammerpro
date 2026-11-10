use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use iroh::Endpoint;
use iroh::endpoint::{Connection, IncomingAddr, RecvStream, SendStream};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, mpsc as channel, watch};
use zeroize::Zeroizing;

use super::frame::{
    Frame, MapTransfer, decode, discard_body, edit_sequence, read_body, read_frame, read_size, write_frame,
};
use super::host::{Joining, Network};
use super::queue::{Body, Kind, Outgoing, PEER_BACKLOG_BYTES, PeerQueue};
use super::{DEADLINE, HANDSHAKE_DEADLINE, INITIAL_JOIN_DEADLINE, MAX_FRAME, WRITE_STALL};
use crate::moderation::{BanList, Bucket, PasswordLimiter, Window};
use crate::protocol::{
    self, ConnectionMode, Invitation, Keys, LEGACY_ALPN, LEGACY_REFUSAL, LegacyWire, Refusal, Refused, Transcript, Wire,
};

pub(crate) const MAX_HANDSHAKES: usize = 16;
pub(crate) const MAX_HANDSHAKES_PER_ADDRESS: usize = 4;
pub(crate) const PEER_QUEUE_BYTES: usize = 24 * 1024 * 1024;
pub(crate) const PEER_FRAME_LIMIT: usize = MAX_FRAME;
pub(crate) const MESSAGE_OVERHEAD: usize = 1024;
pub(crate) const PEER_MESSAGES_PER_SECOND: f64 = 64.0;
pub(crate) const PEER_MESSAGE_BURST: f64 = 128.0;
pub(crate) const PEER_BYTES_PER_SECOND: f64 = 32.0 * 1024.0 * 1024.0;
pub(crate) const PEER_BYTE_BURST: f64 = 64.0 * 1024.0 * 1024.0;
pub(crate) const PINGS_PER_SECOND: u32 = 4;
const MAX_PACE: Duration = Duration::from_secs(20);
const HEARTBEAT: Duration = Duration::from_secs(5);
const ACCEPT_PACING: Duration = Duration::from_millis(50);
const HELLO_LIMIT: usize = 128;
const AUTHENTICATE_LIMIT: usize = 2048;

pub(crate) struct Gate {
    pub(crate) invitation: Invitation,
    keys: Keys,
    pub(crate) epoch: u64,
    pub(crate) limiter: PasswordLimiter,
    pub(crate) bans: BanList,
    pub(crate) failed_passwords: u64,
    pub(crate) invalid_attempts: u64,
    handshakes: HashMap<IpAddr, usize>,
    admitted: HashSet<String>,
}

pub(crate) fn lock(gate: &Mutex<Gate>) -> Result<MutexGuard<'_, Gate>> {
    gate.lock().map_err(|_| anyhow::anyhow!("Session unavailable"))
}

pub(crate) struct Request {
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) session_name: String,
    pub(crate) device: String,
    pub(crate) signature: Vec<u8>,
    pub(crate) invitation: [u8; 32],
    pub(crate) proof: [u8; 32],
    pub(crate) binding: [u8; 32],
    pub(crate) nonce: [u8; 32],
    pub(crate) client: String,
}

impl Gate {
    pub(crate) fn new(invitation: Invitation, password: &str, now: Instant) -> Self {
        let keys = invitation.keys(password);
        Self {
            invitation,
            keys,
            epoch: 0,
            limiter: PasswordLimiter::new(now),
            bans: BanList::default(),
            failed_passwords: 0,
            invalid_attempts: 0,
            handshakes: HashMap::new(),
            admitted: HashSet::new(),
        }
    }

    pub(crate) fn admit_device(&mut self, device: &str) {
        self.admitted.insert(device.to_owned());
    }

    pub(crate) fn forget_device(&mut self, device: &str) {
        self.admitted.remove(device);
    }

    pub(crate) fn rotate(&mut self, password: &str, now: Instant) -> Result<String> {
        self.invitation.token = protocol::random_secret()?;
        self.keys = self.invitation.keys(password);
        self.epoch += 1;
        self.limiter = PasswordLimiter::new(now);
        self.admitted.clear();
        self.invitation.encode()
    }

    pub(crate) fn admit_address(&mut self, ip: Option<IpAddr>) -> bool {
        let Some(ip) = ip else { return true };
        let count = self.handshakes.entry(ip).or_default();
        if *count >= MAX_HANDSHAKES_PER_ADDRESS {
            return false;
        }
        *count += 1;
        true
    }

    fn release_address(&mut self, ip: IpAddr) {
        if let Some(count) = self.handshakes.get_mut(&ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.handshakes.remove(&ip);
            }
        }
    }

    pub(crate) fn verify(&mut self, request: &Request, now: Instant) -> std::result::Result<u64, Refused> {
        let failed = |gate: &mut Self| {
            gate.invalid_attempts += 1;
            Refused::new(Refusal::AuthenticationFailed)
        };
        if request.version != protocol::VERSION
            || !protocol::valid_name(&request.name)
            || !protocol::valid_device(&request.device)
            || request.session_name != self.invitation.room.name
        {
            return Err(failed(self));
        }
        let transcript = Transcript {
            binding: &request.binding,
            nonce: &request.nonce,
            host: &self.invitation.endpoint,
            client: &request.client,
            name: &request.name,
            device: &request.device,
        };
        if !protocol::verify_proof(&self.keys.invitation_proof(&transcript), &request.invitation) {
            return Err(failed(self));
        }
        let signed = request
            .device
            .parse::<iroh::PublicKey>()
            .ok()
            .zip(<[u8; 64]>::try_from(request.signature.as_slice()).ok())
            .is_some_and(|(key, signature)| {
                key.verify(&request.proof, &iroh::Signature::from_bytes(&signature)).is_ok()
            });
        if !signed {
            return Err(failed(self));
        }
        if !self.admitted.contains(&request.device)
            && let Some(left) = self.limiter.locked(now)
        {
            return Err(Refused::with(Refusal::TooManyAttempts, "", Some(left.as_secs().max(1))));
        }
        let expected = self.keys.password_proof(&Transcript {
            binding: &request.binding,
            nonce: &request.nonce,
            host: &self.invitation.endpoint,
            client: &request.client,
            name: &request.name,
            device: &request.device,
        });
        if !protocol::verify_proof(&expected, &request.proof) {
            self.limiter.fail(now);
            self.failed_passwords += 1;
            return Err(Refused::new(Refusal::AuthenticationFailed));
        }
        Ok(self.epoch)
    }
}

struct AddressSlot {
    gate: Arc<Mutex<Gate>>,
    ip: Option<IpAddr>,
}

pub(crate) struct HandshakeSlot {
    _permit: OwnedSemaphorePermit,
    _address: AddressSlot,
}

impl Drop for AddressSlot {
    fn drop(&mut self) {
        if let Some(ip) = self.ip
            && let Ok(mut gate) = self.gate.lock()
        {
            gate.release_address(ip);
        }
    }
}

pub(crate) fn binding(connection: &Connection) -> Result<[u8; 32]> {
    let mut output = [0; 32];
    connection
        .export_keying_material(&mut output, protocol::BINDING_LABEL, protocol::ALPN)
        .map_err(|_| anyhow::anyhow!("Secure channel binding unavailable"))?;
    Ok(output)
}

pub(crate) async fn accept_loop(
    endpoint: Endpoint,
    gate: Arc<Mutex<Gate>>,
    network: channel::Sender<Network>,
    mode: ConnectionMode,
) {
    let permits = Arc::new(Semaphore::new(MAX_HANDSHAKES));
    let large = Arc::new(Semaphore::new(MAX_FRAME));
    while let Some(incoming) = endpoint.accept().await {
        let ip = match incoming.remote_addr() {
            IncomingAddr::Ip(address) => Some(address.ip()),
            _ => None,
        };
        if !gate.lock().is_ok_and(|mut gate| gate.admit_address(ip)) {
            incoming.refuse();
            continue;
        }
        let slot = AddressSlot { gate: gate.clone(), ip };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            incoming.refuse();
            continue;
        };
        let (gate, network, large) = (gate.clone(), network.clone(), large.clone());
        tokio::spawn(async move {
            let held = HandshakeSlot { _permit: permit, _address: slot };
            let Ok(Ok(connection)) = tokio::time::timeout(HANDSHAKE_DEADLINE, incoming).await else { return };
            let retained = connection.clone();
            if serve_peer(connection, gate, network, mode, held, large).await.is_err() {
                retained.close(1u32.into(), b"Authentication or protocol error");
            }
        });
        tokio::time::sleep(ACCEPT_PACING).await;
    }
}

async fn send_refusal(send: &mut SendStream, refused: Refused) {
    let Ok(frame) = Frame::encode(&Wire::Closed(refused)) else { return };
    if write_frame(send, &frame, Duration::from_secs(3)).await.is_ok() && send.finish().is_ok() {
        let _ = tokio::time::timeout(Duration::from_secs(2), send.stopped()).await;
    }
}

pub(crate) async fn refuse_legacy(connection: &Connection) -> Result<()> {
    tokio::time::timeout(HANDSHAKE_DEADLINE, async {
        let (mut send, mut recv) = connection.accept_bi().await?;
        read_frame(&mut recv, HELLO_LIMIT, DEADLINE, DEADLINE).await?;
        let challenge = Frame::serialize(&LegacyWire::Challenge { nonce: protocol::random_secret()? }, 2048)?;
        write_frame(&mut send, &challenge, DEADLINE).await?;
        read_frame(&mut recv, AUTHENTICATE_LIMIT, DEADLINE, DEADLINE).await?;
        let closed = Frame::serialize(&LegacyWire::Closed(LEGACY_REFUSAL.into()), 2048)?;
        write_frame(&mut send, &closed, DEADLINE).await?;
        send.finish()?;
        let _ = tokio::time::timeout(Duration::from_secs(3), send.stopped()).await;
        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("Handshake timed out")??;
    connection.close(0u32.into(), b"Update Hammer Pro to 2.0");
    Ok(())
}

struct Authenticated {
    send: SendStream,
    recv: RecvStream,
    name: String,
    device: String,
    epoch: u64,
}

async fn authenticate(
    connection: &Connection,
    gate: &Mutex<Gate>,
    mode: ConnectionMode,
) -> Result<std::result::Result<Authenticated, (SendStream, Refused)>> {
    let (mut send, mut recv) = connection.accept_bi().await?;
    if let Some(ip) = super::host::Link::observed_ip(connection) {
        let banned = lock(gate)?.bans.ip_denied(ip, Instant::now());
        if let Some((reason, seconds)) = banned {
            return Ok(Err((send, Refused::with(Refusal::Banned, &reason, seconds))));
        }
    }
    let Wire::Hello { version } = decode(&read_frame(&mut recv, HELLO_LIMIT, DEADLINE, DEADLINE).await?)? else {
        return Ok(Err((send, Refused::new(Refusal::Protocol))));
    };
    if version != protocol::VERSION {
        let refusal = if version > protocol::VERSION { Refusal::HostOutdated } else { Refusal::Protocol };
        return Ok(Err((send, Refused::new(refusal))));
    }
    let nonce = protocol::random_secret()?;
    write_frame(&mut send, &Frame::encode(&Wire::Challenge { nonce })?, DEADLINE).await?;
    let message = decode(&read_frame(&mut recv, AUTHENTICATE_LIMIT, DEADLINE, DEADLINE).await?)?;
    let Wire::Authenticate { version, name, session_name, device, signature, invitation, proof } = message else {
        return Ok(Err((send, Refused::new(Refusal::Protocol))));
    };
    if mode == ConnectionMode::Relay && !connection.paths().iter().all(|path| path.remote_addr().is_relay()) {
        return Ok(Err((send, Refused::new(Refusal::Protocol))));
    }
    let request = Request {
        version,
        name,
        session_name,
        device,
        signature,
        invitation,
        proof,
        binding: binding(connection)?,
        nonce,
        client: connection.remote_id().to_string(),
    };
    let verdict = lock(gate)?.verify(&request, Instant::now());
    Ok(match verdict {
        Ok(epoch) => Ok(Authenticated { send, recv, name: request.name, device: request.device, epoch }),
        Err(refused) => Err((send, refused)),
    })
}

pub(crate) async fn serve_peer(
    connection: Connection,
    gate: Arc<Mutex<Gate>>,
    network: channel::Sender<Network>,
    mode: ConnectionMode,
    held: HandshakeSlot,
    large: Arc<Semaphore>,
) -> Result<()> {
    if connection.alpn() == LEGACY_ALPN {
        return refuse_legacy(&connection).await;
    }
    let verdict = tokio::time::timeout(HANDSHAKE_DEADLINE, authenticate(&connection, &gate, mode))
        .await
        .context("Handshake timed out")??;
    drop(held);
    let Authenticated { mut send, mut recv, name, device, epoch } = match verdict {
        Ok(authenticated) => authenticated,
        Err((mut send, refused)) => {
            send_refusal(&mut send, refused).await;
            bail!("Authentication refused");
        }
    };
    let id = connection.remote_id().to_string();
    let generation = connection.stable_id();
    let queue = Arc::new(PeerQueue::new(PEER_BACKLOG_BYTES));
    let (participants, mut participant_updates) = watch::channel(None);
    let (pong, mut pong_updates) = watch::channel(None);
    let budget = Arc::new(Semaphore::new(PEER_QUEUE_BYTES));
    let welcome_sent = Notify::new();
    network
        .send(Network::Joined(Box::new(Joining {
            id: id.clone(),
            device,
            name,
            epoch,
            link: Box::new(connection.clone()),
            queue: queue.clone(),
            participants,
        })))
        .await?;
    let reader = async {
        welcome_sent.notified().await;
        let mut first = true;
        let start = Instant::now();
        let mut messages = Bucket::new(start, PEER_MESSAGE_BURST, PEER_MESSAGES_PER_SECOND);
        let mut bytes = Bucket::new(start, PEER_BYTE_BURST, PEER_BYTES_PER_SECOND);
        let mut pings = Window::new(start);
        let mut flooded = false;
        loop {
            let header = if first { INITIAL_JOIN_DEADLINE } else { DEADLINE };
            let header = read_size(&mut recv, header).await?;
            first = false;
            let now = Instant::now();
            let wait = messages.charge(now, 1.0).max(bytes.charge(now, header.wire_bytes() as f64));
            if !wait.is_zero() {
                if !flooded {
                    flooded = true;
                    network.send(Network::Flooded { id: id.clone(), generation }).await?;
                }
                tokio::time::sleep(wait.min(MAX_PACE)).await;
            }
            let size = header.largest();
            if size > PEER_FRAME_LIMIT {
                let prefix = discard_body(&mut recv, &header, DEADLINE).await?;
                let Some(sequence) = edit_sequence(&prefix) else {
                    return Ok::<Option<Refused>, anyhow::Error>(Some(Refused::new(Refusal::Protocol)));
                };
                network.send(Network::Oversized { id: id.clone(), generation, sequence, size }).await?;
                continue;
            }
            let mut permit = Vec::with_capacity(2);
            if size > PEER_QUEUE_BYTES {
                permit.push(large.clone().acquire_many_owned(u32::try_from(size).unwrap_or(u32::MAX)).await?);
            }
            let reserved = (size + MESSAGE_OVERHEAD).min(PEER_QUEUE_BYTES);
            permit.push(budget.clone().acquire_many_owned(u32::try_from(reserved).unwrap_or(u32::MAX)).await?);
            let Ok(message) = read_body(&mut recv, &header, DEADLINE).await.and_then(|payload| decode(&payload)) else {
                return Ok(Some(Refused::new(Refusal::Protocol)));
            };
            if let Wire::Ping(value) = message {
                if pings.allow(Instant::now(), PINGS_PER_SECOND, Duration::from_secs(1)) {
                    pong.send_replace(Some(value));
                }
                continue;
            }
            network
                .send(Network::Message { id: id.clone(), generation, message, bytes: header.wire_bytes(), permit })
                .await?;
        }
    };
    let writer = async {
        let beat = Frame::encode(&Wire::Pong(0))?;
        let mut heartbeat = tokio::time::interval(HEARTBEAT);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.tick().await;
        loop {
            let item = tokio::select! {
                biased;
                item = queue.pop() => match item { Some(item) => item, None => break },
                changed = participant_updates.changed() => {
                    if changed.is_err() { break; }
                    let Some(frame) = participant_updates.borrow_and_update().clone() else { continue };
                    Outgoing::new(frame, Kind::Normal)
                }
                changed = pong_updates.changed() => {
                    if changed.is_err() { break; }
                    let Some(value) = *pong_updates.borrow_and_update() else { continue };
                    Outgoing::new(Frame::encode(&Wire::Pong(value))?, Kind::Normal)
                }
                _ = heartbeat.tick() => Outgoing::new(beat.clone(), Kind::Normal),
            };
            heartbeat.reset();
            let Outgoing { body, kind, .. } = item;
            let frame = match body {
                Body::Frame(frame) => frame,
                Body::Map(transfer) => {
                    if !send_map(&mut send, transfer, &queue, &network).await? {
                        break;
                    }
                    if kind == Kind::Welcome {
                        welcome_sent.notify_one();
                    }
                    continue;
                }
            };
            let bytes = if kind == Kind::Closing {
                write_frame(&mut send, &frame, DEADLINE).await?
            } else {
                tokio::select! {
                    written = write_frame(&mut send, &frame, WRITE_STALL) => written?,
                    () = queue.cancelled() => break,
                }
            };
            if kind == Kind::Welcome {
                welcome_sent.notify_one();
            }
            network.send(Network::Sent(bytes)).await?;
            if kind == Kind::Closing {
                send.finish()?;
                let _ = tokio::time::timeout(Duration::from_secs(3), send.stopped()).await;
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    let refused = tokio::select! {
        result = reader => result.ok().flatten(),
        result = writer => { let _ = result; None }
    };
    match refused.or_else(|| queue.refused()) {
        Some(refused) => connection.close(refused.refusal.close_code().into(), refused.refusal.id().as_bytes()),
        None => connection.close(0u32.into(), b"Session ended"),
    }
    let _ = network.send(Network::Left { id, generation }).await;
    Ok(())
}

async fn send_map(
    send: &mut SendStream,
    transfer: MapTransfer,
    queue: &PeerQueue,
    network: &channel::Sender<Network>,
) -> Result<bool> {
    let (frames, mut produced) = channel::channel(4);
    let producer = tokio::task::spawn_blocking(move || transfer.produce(|frame| frames.blocking_send(frame).is_ok()));
    let finished = tokio::select! {
        result = async {
            while let Some(frame) = produced.recv().await {
                let bytes = write_frame(send, &frame?, WRITE_STALL).await?;
                network.send(Network::Sent(bytes)).await?;
            }
            Ok::<(), anyhow::Error>(())
        } => { result?; true }
        () = queue.cancelled() => false,
    };
    drop(produced);
    let _ = producer.await;
    Ok(finished)
}

pub(crate) struct ClientHandshake<'a> {
    pub(crate) connection: &'a Connection,
    pub(crate) invitation: &'a Invitation,
    pub(crate) password: &'a Zeroizing<String>,
    pub(crate) identity: &'a Zeroizing<[u8; 32]>,
    pub(crate) client: String,
    pub(crate) name: String,
    pub(crate) session_name: String,
}

impl ClientHandshake<'_> {
    pub(crate) fn device_key(&self) -> iroh::SecretKey {
        let seed = Zeroizing::new(*blake3::keyed_hash(self.identity, self.invitation.endpoint.as_bytes()).as_bytes());
        iroh::SecretKey::from_bytes(&seed)
    }

    pub(crate) fn authenticate(&self, nonce: &[u8; 32]) -> Result<Wire> {
        let keys = self.invitation.keys(self.password);
        let device_key = self.device_key();
        let device = device_key.public().to_string();
        let binding = binding(self.connection)?;
        let transcript = Transcript {
            binding: &binding,
            nonce,
            host: &self.invitation.endpoint,
            client: &self.client,
            name: &self.name,
            device: &device,
        };
        let proof = keys.password_proof(&transcript);
        Ok(Wire::Authenticate {
            version: protocol::VERSION,
            name: self.name.clone(),
            session_name: self.session_name.clone(),
            signature: device_key.sign(&proof).to_bytes().to_vec(),
            invitation: keys.invitation_proof(&transcript),
            device,
            proof,
        })
    }
}
