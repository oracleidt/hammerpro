use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use iroh::endpoint::{ConnectOptions, Connection, ConnectionError, SendStream, TransportErrorCode, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use tokio::sync::{Semaphore, mpsc as channel, watch};
use zeroize::Zeroizing;

use super::frame::{FAST, Frame, decode, read_body, read_header, write_frame};
use super::handshake::{ClientHandshake, PEER_FRAME_LIMIT};
use super::{
    APPROVAL_DEADLINE, Command, Commands, DEADLINE, Editor, Event, JoinOptions, MAX_FRAME, WRITE_STALL, event,
};
use crate::document::{ID_STRIDE, MAX_MAP_BYTES, MAX_UNITS, Snapshot, ValidatedSnapshot};
use crate::protocol::{
    self, ALPN, ConnectionMode, Invitation, LEGACY_ALPN, MAX_PARTICIPANTS, Outcome, Refusal, Refused, Rejection, Wire,
};

pub(crate) const GUEST_QUEUE_BYTES: usize = 64 * 1024 * 1024;
const PING_INTERVAL: Duration = Duration::from_secs(2);
const BUSY_RECHECK: Duration = Duration::from_millis(20);
const CONNECT_ATTEMPTS: u32 = 4;
const REFUSED_RETRY: Duration = Duration::from_millis(750);
const PACK_INLINE: usize = 1024 * 1024;

pub(crate) enum Transfer {
    Join { participant: u32, id_start: u32 },
    Reset,
    Resync(Vec<Outcome>),
}

struct Receiving {
    transfer: Transfer,
    revision: u64,
    units: u64,
    size: usize,
    map: Snapshot,
}

pub(crate) struct Finished {
    transfer: Transfer,
    revision: u64,
    map: Snapshot,
}

impl Finished {
    pub(crate) fn validate(self) -> Result<Event> {
        let (revision, map) = (self.revision, ValidatedSnapshot::new(self.map)?);
        Ok(match self.transfer {
            Transfer::Join { participant, id_start } => {
                Event::Ready { invitation: None, participant, id_start, revision, map, direct_addresses: Vec::new() }
            }
            Transfer::Reset => Event::Reset { revision, map },
            Transfer::Resync(outcomes) => Event::Resync { revision, outcomes, map },
        })
    }
}

pub(crate) enum Received {
    Event(Box<Event>),
    Map(Finished),
}

pub(crate) struct GuestCore {
    pub(crate) revision: u64,
    pub(crate) received: u64,
    participant: u32,
    receiving: Option<Receiving>,
}

impl GuestCore {
    pub(crate) fn start(message: Wire) -> Result<Self> {
        match message {
            Wire::MapStart { participant, id_start, revision, units, bytes } => {
                ensure!(participant >= 2, "Invalid host message");
                ensure!(
                    id_start >= ID_STRIDE && id_start % ID_STRIDE == 0 && id_start < super::host::RANGE_LIMIT,
                    "Invalid host message"
                );
                let mut core = Self { revision, received: 0, participant, receiving: None };
                core.receive(Transfer::Join { participant, id_start }, revision, units, bytes)?;
                Ok(core)
            }
            Wire::Closed(refused) => Err(refused.sanitized().into()),
            _ => bail!(Refused::new(Refusal::Protocol)),
        }
    }

    #[cfg(test)]
    pub(crate) fn at(revision: u64) -> Self {
        Self { revision, received: 0, participant: 2, receiving: None }
    }

    fn receive(&mut self, transfer: Transfer, revision: u64, units: u64, bytes: u64) -> Result<()> {
        ensure!(
            usize::try_from(units).is_ok_and(|units| units <= MAX_UNITS)
                && usize::try_from(bytes).is_ok_and(|bytes| bytes <= MAX_FRAME),
            "Invalid map transfer"
        );
        self.receiving = Some(Receiving { transfer, revision, units, size: 0, map: Snapshot::default() });
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn handle(&mut self, message: Wire, host_latency: u64) -> Result<Option<Event>> {
        Ok(match self.accept(message, host_latency)? {
            Some(Received::Event(event)) => Some(*event),
            Some(Received::Map(finished)) => Some(finished.validate()?),
            None => None,
        })
    }

    fn part(&mut self, message: Wire) -> Result<Option<Received>> {
        let Some(receiving) = self.receiving.as_mut() else { bail!(Refused::new(Refusal::Protocol)) };
        match message {
            Wire::MapPart(part) => {
                for (key, unit) in part.0 {
                    receiving.size += key.len() + unit.vmf.len() + 4;
                    ensure!(receiving.map.0.insert(key, unit).is_none(), "Duplicate map unit");
                }
                ensure!(
                    receiving.map.0.len() as u64 <= receiving.units && receiving.size <= MAX_MAP_BYTES,
                    "Invalid map transfer"
                );
                let joining = matches!(receiving.transfer, Transfer::Join { .. });
                let progress = Event::JoinProgress {
                    units: receiving.map.0.len() as u64,
                    total: receiving.units,
                    bytes: self.received,
                };
                Ok(joining.then(|| Received::Event(Box::new(progress))))
            }
            Wire::MapEnd => {
                ensure!(receiving.map.0.len() as u64 == receiving.units, "Incomplete map transfer");
                let Receiving { transfer, revision, map, .. } =
                    self.receiving.take().ok_or_else(|| anyhow::anyhow!("Invalid map transfer"))?;
                self.revision = revision;
                Ok(Some(Received::Map(Finished { transfer, revision, map })))
            }
            Wire::Closed(refused) => Err(refused.sanitized().into()),
            _ => bail!(Refused::new(Refusal::Protocol)),
        }
    }

    pub(crate) fn accept(&mut self, message: Wire, host_latency: u64) -> Result<Option<Received>> {
        if self.receiving.is_some() {
            return self.part(message);
        }
        let value = match message {
            Wire::Reset { revision, units, bytes } => {
                ensure!(revision == self.revision.wrapping_add(1), "Session revision mismatch");
                self.receive(Transfer::Reset, revision, units, bytes)?;
                return Ok(None);
            }
            Wire::Resync { revision, outcomes, units, bytes } => {
                ensure!(revision >= self.revision, "Session revision mismatch");
                let outcomes = outcomes
                    .into_iter()
                    .map(|mut outcome| {
                        outcome.rejection = outcome.rejection.map(Rejection::sanitized);
                        outcome
                    })
                    .collect();
                self.receive(Transfer::Resync(outcomes), revision, units, bytes)?;
                return Ok(None);
            }
            Wire::Acked { sequence, revision } => {
                ensure!(revision == self.revision.wrapping_add(1), "Session revision mismatch");
                self.revision = revision;
                Event::Applied { author: self.participant, sequence, revision, changes: Vec::new() }
            }
            Wire::Applied { author, sequence, revision, changes } => {
                ensure!(revision == self.revision.wrapping_add(1), "Session revision mismatch");
                ensure!(
                    author >= 1
                        && !changes.is_empty()
                        && changes.iter().all(|change| protocol::valid_change_key(&change.key)),
                    "Invalid host message"
                );
                self.revision = revision;
                Event::Applied { author, sequence, revision, changes }
            }
            Wire::Rejected { sequence, rejection, revision } => {
                ensure!(revision == self.revision, "Session revision mismatch");
                Event::Rejected { sequence, rejection: rejection.sanitized(), revision }
            }
            Wire::Participants(mut rows) => {
                let ids: BTreeSet<u32> = rows.iter().map(|row| row.id).collect();
                ensure!(
                    !rows.is_empty()
                        && rows.len() <= MAX_PARTICIPANTS
                        && ids.len() == rows.len()
                        && rows[0].id == 1
                        && rows.iter().all(protocol::Participant::validate),
                    "Invalid participants"
                );
                rows[0].latency_ms = host_latency.min(protocol::MAX_LATENCY_MS);
                Event::Participants(rows)
            }
            Wire::Pong(_) => return Ok(None),
            Wire::Closed(refused) => return Err(refused.sanitized().into()),
            _ => bail!(Refused::new(Refusal::Protocol)),
        };
        Ok(Some(Received::Event(Box::new(value))))
    }
}

pub(crate) fn closed_by_host(connection: &Connection) -> Option<Refused> {
    match connection.close_reason()? {
        ConnectionError::ApplicationClosed(close) => {
            let code = close.error_code.into_inner();
            Refusal::from_close_code(code)
                .or_else(|| (code == 0 && close.reason.as_ref() == b"Host ended the session").then_some(Refusal::Ended))
                .or_else(|| (close.reason.as_ref() == b"Host rotated access credentials").then_some(Refusal::Revoked))
                .map(Refused::new)
        }
        _ => None,
    }
}

fn latency(connection: &Connection) -> u64 {
    connection.paths().iter().find(|path| path.is_selected()).map_or(0, |path| path.rtt().as_millis() as u64)
}

pub(crate) fn refused_by_host(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<ConnectionError>().is_some_and(|error| {
            matches!(error, ConnectionError::ConnectionClosed(close) if close.error_code == TransportErrorCode::CONNECTION_REFUSED)
        }) || cause.to_string().ends_with(&TransportErrorCode::CONNECTION_REFUSED.to_string())
    })
}

async fn handshake(
    send: &mut SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    client: ClientHandshake<'_>,
    editor: &Editor,
) -> Result<(Wire, u64, u64)> {
    let mut sent = write_frame(send, &Frame::encode(&Wire::Hello { version: protocol::VERSION })?, DEADLINE).await?;
    let header = read_header(recv, 2048, DEADLINE).await?;
    let nonce = match decode(&read_body(recv, &header, DEADLINE).await?)? {
        Wire::Challenge { nonce } => nonce,
        Wire::Closed(refused) => return Err(refused.sanitized().into()),
        _ => bail!(Refused::new(Refusal::Protocol)),
    };
    let mut received = header.wire_bytes();
    sent += write_frame(send, &Frame::encode(&client.authenticate(&nonce)?)?, DEADLINE).await?;
    let mut header = DEADLINE;
    loop {
        let head = read_header(recv, MAX_FRAME, header).await?;
        let message = decode(&read_body(recv, &head, DEADLINE).await?)?;
        received += head.wire_bytes();
        if matches!(message, Wire::Waiting) {
            ensure!(header == DEADLINE, "Invalid host message");
            event(editor, Event::Waiting)?;
            header = APPROVAL_DEADLINE + DEADLINE;
            continue;
        }
        return Ok((message, sent, received));
    }
}

async fn connect(endpoint: &Endpoint, address: EndpointAddr) -> Result<Connection> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let result = tokio::time::timeout(DEADLINE, async {
            endpoint
                .connect_with_opts(
                    address.clone(),
                    ALPN,
                    ConnectOptions::new().with_additional_alpns(vec![LEGACY_ALPN.to_vec()]),
                )
                .await?
                .await
                .map_err(anyhow::Error::from)
        })
        .await
        .context("Connecting to the session timed out")
        .and_then(|result| result);
        match result {
            Ok(connection) => return Ok(connection),
            Err(error) if refused_by_host(&error) => {
                if attempt >= CONNECT_ATTEMPTS {
                    bail!(Refused::new(Refusal::Busy));
                }
                tokio::time::sleep(REFUSED_RETRY * attempt).await;
            }
            Err(error) => return Err(error.context("Could not connect to the session")),
        }
    }
}

pub(crate) async fn run(
    invitation: Invitation,
    name: String,
    password: Zeroizing<String>,
    options: JoinOptions,
    mut commands: Commands,
    editor: Editor,
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
            .max_idle_timeout(Some(DEADLINE.try_into()?))
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
    let connection = match connect(&endpoint, address).await {
        Ok(connection) => connection,
        Err(error) => {
            endpoint.close().await;
            return Err(error);
        }
    };
    if connection.alpn() == LEGACY_ALPN {
        connection.close(0u32.into(), b"Update the host to Hammer Pro 2.0");
        endpoint.close().await;
        bail!(Refused::new(Refusal::HostOutdated));
    }
    let result = session(&endpoint, &connection, invitation, name, password, options, &mut commands, &editor)
        .await
        .map_err(|error| match closed_by_host(&connection) {
            Some(refused) if error.downcast_ref::<Refused>().is_none() => refused.into(),
            _ => error,
        });
    connection.close(0u32.into(), b"Participant left");
    endpoint.close().await;
    result
}

async fn write_loop(
    mut send: SendStream,
    mut frames: channel::UnboundedReceiver<Frame>,
    mut presence: watch::Receiver<Option<Frame>>,
    sent: Arc<AtomicU64>,
    start: Instant,
) -> Result<()> {
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let frame = tokio::select! {
            biased;
            frame = frames.recv() => match frame { Some(frame) => frame, None => return Ok(()) },
            changed = presence.changed() => {
                if changed.is_err() { return Ok(()); }
                let Some(frame) = presence.borrow_and_update().clone() else { continue };
                frame
            }
            _ = ping.tick() => Frame::encode(&Wire::Ping(start.elapsed().as_millis() as u64))?,
        };
        let bytes = write_frame(&mut send, &frame, WRITE_STALL).await?;
        sent.fetch_add(bytes, Ordering::Relaxed);
    }
}

#[allow(clippy::too_many_arguments)]
async fn session(
    endpoint: &Endpoint,
    connection: &Connection,
    invitation: Invitation,
    name: String,
    password: Zeroizing<String>,
    options: JoinOptions,
    commands: &mut Commands,
    editor: &Editor,
) -> Result<()> {
    let (mut send, mut recv) = connection.open_bi().await?;
    if invitation.mode == ConnectionMode::Relay {
        ensure!(
            endpoint.addr().ip_addrs().next().is_none()
                && connection.paths().iter().all(|path| path.remote_addr().is_relay()),
            "Direct connections are disabled"
        );
    }
    let client = ClientHandshake {
        connection,
        invitation: &invitation,
        password: &password,
        identity: &options.identity,
        client: endpoint.id().to_string(),
        name,
        session_name: options.session_name,
    };
    let (welcome_message, sent, received) = handshake(&mut send, &mut recv, client, editor).await?;
    drop(password);
    let mut core = GuestCore::start(welcome_message)?;
    core.received = received;
    let start = Instant::now();
    let sent = Arc::new(AtomicU64::new(sent));
    let (outgoing, frames) = channel::unbounded_channel();
    let (presence, presence_frames) = watch::channel(None);
    let mut writer = tokio::spawn(write_loop(send, frames, presence_frames, sent.clone(), start));
    let mut tick = tokio::time::interval(PING_INTERVAL);
    let (incoming_tx, mut incoming) = channel::channel(64);
    let budget = Arc::new(Semaphore::new(GUEST_QUEUE_BYTES));
    let reader = tokio::spawn(async move {
        loop {
            let result = async {
                let header = read_header(&mut recv, MAX_FRAME, DEADLINE).await?;
                let permit = budget
                    .clone()
                    .acquire_many_owned(u32::try_from(header.largest().min(GUEST_QUEUE_BYTES)).unwrap_or(u32::MAX))
                    .await?;
                let message = decode(&read_body(&mut recv, &header, DEADLINE).await?)?;
                Ok::<_, anyhow::Error>((message, header.wire_bytes(), permit))
            }
            .await;
            let failed = result.is_err();
            if incoming_tx.send(result).await.is_err() || failed {
                break;
            }
        }
    });
    let result = async {
        loop {
            tokio::select! {
                command = commands.next() => match command {
                    None | Some(Command::Stop) => break,
                    Some(Command::Edit { sequence, changes }) => {
                        let bytes = serde_json::to_vec(&Wire::Edit { sequence, changes })?;
                        if bytes.len() > PEER_FRAME_LIMIT {
                            let rejection = Rejection::too_large(bytes.len(), PEER_FRAME_LIMIT);
                            event(editor, Event::Rejected { sequence, rejection, revision: core.revision })?;
                        } else {
                            let frame = if bytes.len() < PACK_INLINE {
                                Frame::pack_json(bytes, FAST)?
                            } else {
                                tokio::task::spawn_blocking(move || Frame::pack_json(bytes, FAST)).await??
                            };
                            outgoing.send(frame).map_err(|_| anyhow::anyhow!("Session send stopped"))?;
                        }
                    }
                    Some(Command::Presence(update)) if update.validate() => {
                        presence.send_replace(Some(Frame::pack(&Wire::Presence(update), FAST)?));
                    }
                    _ => {}
                },
                message = incoming.recv(), if !editor.busy() => {
                    let message = message.ok_or_else(|| anyhow::anyhow!("Host disconnected"))?;
                    let (message, bytes, permit) = message?;
                    core.received += bytes;
                    let value = core.accept(message, latency(connection))?;
                    drop(permit);
                    match value {
                        Some(Received::Event(value)) => event(editor, *value)?,
                        Some(Received::Map(finished)) => {
                            let value = tokio::task::spawn_blocking(move || finished.validate()).await??;
                            event(editor, value)?;
                        }
                        None => {}
                    }
                }
                () = tokio::time::sleep(BUSY_RECHECK), if editor.busy() => {}
                finished = &mut writer => {
                    finished.context("Session send stopped")??;
                    bail!("Session send stopped");
                }
                _ = tick.tick() => {
                    event(editor, Event::Traffic { sent: sent.load(Ordering::Relaxed), received: core.received })?;
                    let direct_path_count = connection.paths().iter().filter(|path| path.remote_addr().is_ip()).count();
                    if invitation.mode == ConnectionMode::Relay {
                        ensure!(direct_path_count == 0 && endpoint.addr().ip_addrs().next().is_none(), "Direct path detected in relay-only session");
                    }
                    event(editor, Event::NetworkPrivacy {
                        relay_only: invitation.mode == ConnectionMode::Relay,
                        published_ip_count: endpoint.addr().ip_addrs().count(),
                        direct_path_count,
                    })?;
                }
            }
        }
        Ok(())
    }
    .await;
    reader.abort();
    writer.abort();
    result
}
