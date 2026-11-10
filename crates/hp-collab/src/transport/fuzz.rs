use std::time::Duration;

use super::frame::{Frame, decode, grow};
use super::guest::GuestCore;
use super::host::Network;
use super::tests::{FakePeer, Host, fixture_map, joining};
use super::*;
use crate::document::{Change, Snapshot, Unit};
use crate::moderation::AccessPolicy;
use crate::protocol::{Participant, Refusal, Wire};

pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub(crate) fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }

    pub(crate) fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    pub(crate) fn float(&mut self) -> f64 {
        match self.below(10) {
            0 => f64::NAN,
            1 => f64::INFINITY,
            2 => f64::NEG_INFINITY,
            3 => -0.0,
            4 => 1e300,
            5 => 1_000_000.5,
            _ => (self.next() as i64 % 2_000_000) as f64 / 3.0,
        }
    }

    pub(crate) fn number(&mut self) -> u64 {
        match self.below(6) {
            0 => 0,
            1 => u64::MAX,
            2 => u64::from(u32::MAX),
            3 => self.below(10),
            _ => self.next() >> self.below(64),
        }
    }

    pub(crate) fn text(&mut self, max: usize) -> String {
        let length = self.below(max as u64 + 1) as usize;
        (0..length)
            .map(|_| match self.below(12) {
                0 => '\n',
                1 => '\0',
                2 => '"',
                3 => '{',
                4 => '}',
                5 => 'é',
                6 => 'Ж',
                7 => '\u{202e}',
                _ => (b'a' + self.below(26) as u8) as char,
            })
            .collect()
    }

    pub(crate) fn key(&mut self) -> String {
        match self.below(9) {
            0 => "world".into(),
            1 => "metadata".into(),
            2 => format!("entity:{}", self.below(6)),
            3 => format!("solid:{}", self.below(6) + 1_000_000 * self.below(4)),
            4 => format!("solid:0{}", self.below(9)),
            5 => self.text(80),
            6 => format!("entity:{}", self.number()),
            _ => format!("entity:{}", 2 + self.below(2)),
        }
    }

    pub(crate) fn vmf(&mut self) -> String {
        let id = match self.below(4) {
            0 => self.number().to_string(),
            1 => "2".into(),
            2 => (2_000_000 + self.below(10)).to_string(),
            _ => self.below(5).to_string(),
        };
        let class = self.pick(&["light", "info_target", "lua_run", "prop_dynamic", "func_instance", "worldspawn"]);
        let mut text = match self.below(6) {
            0 => format!("solid {{\"id\" \"{id}\""),
            1 => format!("world {{\"id\" \"{id}\""),
            _ => format!("entity {{\"id\" \"{id}\" \"classname\" \"{class}\""),
        };
        for _ in 0..self.below(5) {
            let key = self.pick(&["origin", "angles", "model", "file", "targetname", "plane", "id", "x"]);
            let value = match self.below(5) {
                0 => format!("{} {} {}", self.float(), self.float(), self.float()),
                1 => "../secret.mdl".into(),
                2 => "C:/x.vmf".into(),
                _ => self.text(24),
            };
            text.push_str(&format!(" \"{key}\" \"{value}\""));
        }
        if self.chance(30) {
            text.push_str(&format!(
                " side {{\"id\" \"{}\" \"plane\" \"(0 0 0) (64 0 0) (64 64 0)\" \"material\" \"x\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\"}}",
                self.below(3_000_000)
            ));
        }
        if self.chance(10) {
            text.push_str(&"x {".repeat(self.below(200) as usize));
        }
        if !self.chance(5) {
            text.push('}');
        }
        text
    }

    pub(crate) fn unit(&mut self) -> Unit {
        Unit { owner: self.below(4) as u32, hidden: self.chance(20), vmf: self.vmf() }
    }

    pub(crate) fn changes(&mut self, map: &Snapshot) -> Vec<Change> {
        let count = if self.chance(3) { 9000 } else { self.below(4) as usize };
        (0..count)
            .map(|_| {
                let key = self.key();
                let before = if self.chance(70) {
                    map.0.get(&key).map(crate::document::digest)
                } else if self.chance(50) {
                    None
                } else {
                    Some([self.below(255) as u8; 32])
                };
                let after = if self.chance(80) {
                    Some(match map.0.get(&key) {
                        Some(unit) if self.chance(60) => {
                            let mut unit = unit.clone();
                            unit.vmf = unit.vmf.replace("0 0 0", &format!("{} 0 0", self.float()));
                            unit
                        }
                        _ => self.unit(),
                    })
                } else {
                    None
                };
                Change { key, before, after }
            })
            .collect()
    }

    pub(crate) fn presence(&mut self) -> Presence {
        let selected = (0..if self.chance(2) { 2100 } else { self.below(4) as usize }).map(|_| self.key()).collect();
        Presence {
            position: [self.float(), self.float(), self.float()],
            direction: [self.float(), self.float(), self.float()],
            selected,
            ..Presence::default()
        }
    }

    pub(crate) fn participant(&mut self) -> Participant {
        Participant {
            id: self.number() as u32,
            name: if self.chance(70) { format!("P{}", self.below(9)) } else { self.text(16) },
            role: *self.pick(&[Role::Host, Role::Editor, Role::Viewer]),
            presence: self.presence(),
            latency_ms: self.number(),
        }
    }

    pub(crate) fn snapshot(&mut self) -> Snapshot {
        let mut map = fixture_map();
        for _ in 0..self.below(3) {
            map.0.insert(self.key(), self.unit());
        }
        map
    }

    pub(crate) fn wire(&mut self, map: &Snapshot) -> Wire {
        match self.below(18) {
            0 => Wire::Hello { version: self.number() as u32 },
            1 => Wire::Challenge { nonce: [self.below(256) as u8; 32] },
            2 => Wire::Authenticate {
                version: self.number() as u32,
                name: self.text(14),
                session_name: self.text(50),
                device: self.text(70),
                signature: (0..self.below(80)).map(|_| self.below(256) as u8).collect(),
                invitation: [self.below(256) as u8; 32],
                proof: [self.below(256) as u8; 32],
            },
            3 => Wire::Waiting,
            4 => Wire::MapStart {
                participant: self.number() as u32,
                id_start: self.number() as u32,
                revision: self.number(),
                units: self.below(6),
                bytes: self.number(),
            },
            5 | 6 => Wire::Edit { sequence: self.number(), changes: self.changes(map) },
            7 => Wire::Applied {
                author: self.number() as u32,
                sequence: self.number(),
                revision: self.number(),
                changes: self.changes(map),
            },
            8 => Wire::Rejected {
                sequence: self.number(),
                rejection: protocol::Rejection {
                    code: *self.pick(&protocol::EditRefusal::ALL),
                    detail: self.text(600),
                },
                revision: self.number(),
            },
            9 => Wire::Presence(self.presence()),
            10 => Wire::Participants((0..self.below(10)).map(|_| self.participant()).collect()),
            11 if self.chance(50) => {
                Wire::Reset { revision: self.number(), units: self.below(6), bytes: self.number() }
            }
            11 => Wire::Resync {
                revision: self.number(),
                outcomes: (0..self.below(4))
                    .map(|_| protocol::Outcome {
                        sequence: self.number(),
                        rejection: self.chance(50).then(|| protocol::Rejection {
                            code: *self.pick(&protocol::EditRefusal::ALL),
                            detail: self.text(400),
                        }),
                    })
                    .collect(),
                units: self.below(6),
                bytes: self.number(),
            },
            12 => Wire::Ping(self.number()),
            13 => Wire::Pong(self.number()),
            15 => Wire::MapPart(self.snapshot()),
            16 => Wire::MapEnd,
            17 => Wire::Acked { sequence: self.number(), revision: self.number() },
            _ => Wire::Closed(protocol::Refused {
                refusal: *self.pick(&Refusal::ALL),
                detail: self.text(900),
                seconds: (self.chance(50)).then(|| self.number()),
            }),
        }
    }

    pub(crate) fn mutate(&mut self, mut bytes: Vec<u8>) -> Vec<u8> {
        for _ in 0..1 + self.below(4) {
            if bytes.is_empty() {
                bytes.push(b'{');
            }
            let at = self.below(bytes.len() as u64) as usize;
            match self.below(7) {
                0 => bytes[at] ^= 1 << self.below(8),
                1 => bytes.insert(at, self.below(256) as u8),
                2 => {
                    let end = (at + 1 + self.below(16) as usize).min(bytes.len());
                    bytes.drain(at..end);
                }
                3 => {
                    let end = (at + 1 + self.below(32) as usize).min(bytes.len());
                    let copy = bytes[at..end].to_vec();
                    bytes.splice(at..at, copy);
                }
                4 => bytes.truncate(at),
                5 => {
                    let digits = self.number().to_string().into_bytes();
                    bytes.splice(at..at, digits);
                }
                _ => {
                    let tokens: [&[u8]; 6] = [b"null", b"[[[[[[", b"{\"\":", b"1e999", b"-1", b"\"\\u0000\""];
                    let token = *self.pick(&tokens);
                    bytes.splice(at..at, token.iter().copied());
                }
            }
        }
        bytes
    }
}

fn cases(normal: usize) -> usize {
    std::env::var("HP_FUZZ_CASES").ok().and_then(|value| value.parse().ok()).unwrap_or(normal)
}

fn frame_decoder(cases: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    let map = fixture_map();
    let mut decoded = 0;
    for _ in 0..cases {
        let bytes = if rng.chance(20) {
            (0..rng.below(300)).map(|_| rng.below(256) as u8).collect()
        } else {
            let message = rng.wire(&map);
            let bytes = serde_json::to_vec(&message).unwrap();
            if rng.chance(15) { bytes } else { rng.mutate(bytes) }
        };
        if let Ok(message) = decode(&bytes) {
            decoded += 1;
            let mut core = GuestCore::at(rng.number());
            let _ = core.handle(message, rng.number());
        }
    }
    assert!(decoded > cases / 20, "the generator must also produce valid messages ({decoded} of {cases})");
}

fn guest_sequences(cases: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    let map = fixture_map();
    for _ in 0..cases / 20 {
        let first = if rng.chance(50) {
            Wire::MapStart { participant: 2, id_start: 2_000_000, revision: 0, units: map.0.len() as u64, bytes: 0 }
        } else {
            rng.wire(&map)
        };
        let Ok(mut core) = GuestCore::start(first) else { continue };
        let joined = core.handle(Wire::MapPart(map.clone()), 0).and_then(|_| core.handle(Wire::MapEnd, 0));
        if !matches!(joined, Ok(Some(Event::Ready { .. }))) {
            continue;
        }
        let mut revision = core.revision;
        for _ in 0..20 {
            let message = rng.wire(&map);
            match core.handle(message, rng.number()) {
                Ok(Some(Event::Applied { revision: new, .. } | Event::Reset { revision: new, .. })) => {
                    assert_eq!(new, revision + 1);
                    revision = new;
                }
                Ok(Some(Event::Resync { revision: new, outcomes, map })) => {
                    assert!(new >= revision);
                    map.validate().unwrap();
                    for rejection in outcomes.iter().filter_map(|outcome| outcome.rejection.as_ref()) {
                        assert!(rejection.detail.chars().count() <= protocol::MAX_REASON_CHARS);
                        assert!(!rejection.detail.chars().any(char::is_control));
                    }
                    revision = new;
                }
                Ok(Some(Event::Rejected { rejection, .. })) => {
                    assert!(rejection.detail.chars().count() <= protocol::MAX_REASON_CHARS);
                    assert!(!rejection.detail.chars().any(char::is_control));
                }
                Ok(Some(Event::Participants(rows))) => {
                    assert!(rows.len() <= protocol::MAX_PARTICIPANTS && rows.iter().all(Participant::validate));
                }
                Ok(_) => {}
                Err(error) => {
                    if let Some(refused) = error.downcast_ref::<protocol::Refused>() {
                        assert!(refused.detail.chars().count() <= protocol::MAX_REASON_CHARS);
                    }
                    break;
                }
            }
            assert_eq!(core.revision, revision);
        }
    }
}

fn host_sequences(cases: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    for round in 0..cases / 50 {
        let policy = AccessPolicy {
            locked: rng.chance(10),
            approval: rng.chance(30),
            read_only_guests: rng.chance(30),
            invitation_minutes: if rng.chance(20) { 1 } else { 0 },
            refuse_scripts: rng.chance(30),
        };
        let mut host = Host::new(fixture_map(), policy);
        let mut peers: Vec<FakePeer> = Vec::new();
        let mut applied = 0u64;
        for step in 0..50 {
            let epoch = host.gate.lock().unwrap().epoch;
            match rng.below(10) {
                0 => {
                    let id = format!("peer{}", rng.below(12));
                    let device = format!("device{}", rng.below(6));
                    let name = if rng.chance(80) { format!("N{}", rng.below(10)) } else { rng.text(14) };
                    let epoch = if rng.chance(90) { epoch } else { epoch + 1 };
                    let (joining, peer) = joining(&id, &device, &name, epoch);
                    host.core.network(Network::Joined(Box::new(joining)), host.now).unwrap();
                    peers.push(peer);
                }
                1..=5 if !peers.is_empty() => {
                    let index = rng.below(peers.len() as u64) as usize;
                    let id = peers[index].id.clone();
                    let message = if rng.chance(5) {
                        Network::Oversized { id, generation: 7, sequence: rng.number(), size: rng.number() as usize }
                    } else {
                        let message = rng.wire(host.core.snapshot());
                        Network::Message { id, generation: 7, message, bytes: 100, permit: Vec::new() }
                    };
                    host.core.network(message, host.now).unwrap();
                    if rng.chance(70) {
                        host.work();
                    }
                }
                6 => {
                    let participant = rng.below(12) as u32;
                    let command = match rng.below(9) {
                        0 => Command::Kick(participant),
                        1 => Command::Ban {
                            participant,
                            seconds: rng.chance(50).then(|| rng.number()),
                            reason: rng.text(300),
                        },
                        2 => Command::Unban(rng.below(4)),
                        3 => {
                            Command::SetRole { participant, role: *rng.pick(&[Role::Host, Role::Editor, Role::Viewer]) }
                        }
                        4 => Command::Admit(participant),
                        5 => Command::Decline(participant),
                        6 => Command::Policy(AccessPolicy {
                            locked: rng.chance(30),
                            approval: rng.chance(30),
                            read_only_guests: rng.chance(30),
                            invitation_minutes: rng.number() as u32,
                            refuse_scripts: rng.chance(30),
                        }),
                        7 => Command::Presence(rng.presence()),
                        _ => Command::Edit { sequence: step, changes: rng.changes(host.core.snapshot()) },
                    };
                    assert!(host.core.command(command, host.now).unwrap());
                }
                7 if !peers.is_empty() => {
                    let index = rng.below(peers.len() as u64) as usize;
                    let peer = peers.remove(index);
                    let message = if rng.chance(50) {
                        Network::Left { id: peer.id.clone(), generation: 7 }
                    } else {
                        Network::Flooded { id: peer.id.clone(), generation: 7 }
                    };
                    host.core.network(message, host.now).unwrap();
                }
                8 => {
                    host.now += Duration::from_secs(rng.below(90));
                    host.core.tick(host.now, 0).unwrap();
                }
                _ => {
                    if rng.chance(5) {
                        host.command(Command::Rotate(zeroize::Zeroizing::new(format!("password-{round}-{step}"))));
                    }
                }
            }
            for event in host.drain() {
                if let Event::Applied { revision, .. } | Event::Reset { revision, .. } = event {
                    applied += 1;
                    assert_eq!(revision, applied);
                }
            }
            assert!(host.core.peer_count() + host.core.pending_count() < protocol::MAX_PARTICIPANTS);
            assert_eq!(host.core.revision(), applied);
            for peer in &mut peers {
                for (message, _, _) in peer.frames() {
                    if let Wire::Rejected { rejection, .. } = message {
                        assert!(rejection.detail.chars().count() <= protocol::MAX_REASON_CHARS);
                    }
                }
            }
        }
        host.core.snapshot().validate().unwrap();
    }
}

fn invitation_decoder(cases: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    let valid = super::tests::test_invitation().encode().unwrap();
    let json = serde_json::to_vec(&super::tests::test_invitation()).unwrap();
    for _ in 0..cases {
        let text = match rng.below(3) {
            0 => String::from_utf8_lossy(&rng.mutate(valid.clone().into_bytes())).into_owned(),
            1 => {
                use base64::Engine;
                let bytes = rng.mutate(json.clone());
                format!("hpjoin1:{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
            }
            _ => rng.text(3000),
        };
        let _ = protocol::Invitation::decode(&text);
    }
}

fn snapshot_decoder(cases: usize, seed: u64) {
    let mut rng = Rng::new(seed);
    let base = fixture_map().to_vmf().unwrap().into_bytes();
    for _ in 0..cases / 4 {
        let text = String::from_utf8_lossy(&rng.mutate(base.clone())).into_owned();
        if let Ok(map) = Snapshot::from_vmf(&text) {
            map.validate().unwrap();
        }
    }
}

#[test]
fn frame_decoder_never_panics_on_random_or_mutated_messages() {
    frame_decoder(cases(4000), 1);
}

#[test]
fn guest_handler_survives_random_host_message_sequences() {
    guest_sequences(cases(4000), 2);
}

#[test]
fn host_handler_survives_random_peers_messages_and_commands() {
    host_sequences(cases(3000), 3);
}

#[test]
fn invitation_and_snapshot_decoders_survive_mutations() {
    invitation_decoder(cases(4000), 4);
    snapshot_decoder(cases(4000), 5);
}

#[test]
#[ignore = "long fuzz run: HP_FUZZ_CASES scales it (default 200000 cases per target), HP_FUZZ_SEED replays a seed"]
fn long_fuzz_of_every_decoder_and_handler() {
    let count = std::env::var("HP_FUZZ_CASES").ok().and_then(|value| value.parse().ok()).unwrap_or(200_000);
    let seed = std::env::var("HP_FUZZ_SEED").ok().and_then(|value| value.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_nanos() as u64)
            ^ u64::from(std::process::id())
    });
    eprintln!("long fuzz seed {seed}, {count} cases per target");
    frame_decoder(count, seed);
    guest_sequences(count, seed + 1);
    host_sequences(count, seed + 2);
    invitation_decoder(count, seed + 3);
    snapshot_decoder(count, seed + 4);
}

#[test]
fn receive_buffers_grow_with_received_bytes_only() {
    let size = 8 * 1024 * 1024;
    let mut bytes = Vec::with_capacity(size.min(super::frame::INITIAL_CAPACITY));
    let chunk = vec![7u8; 1200];
    let mut reallocations = 0;
    while bytes.len() < size {
        let take = chunk.len().min(size - bytes.len());
        let capacity = bytes.capacity();
        let before = bytes.len();
        grow(&mut bytes, take, size);
        reallocations += usize::from(bytes.capacity() != capacity);
        bytes.extend_from_slice(&chunk[..take]);
        if before + take <= super::frame::TRUSTED_PREFIX {
            assert!(bytes.capacity() <= (bytes.len() * 2).max(super::frame::INITIAL_CAPACITY));
        } else {
            assert_eq!(bytes.capacity(), size);
        }
    }
    assert!(reallocations <= 6, "{reallocations}");
    let small = 70_000;
    let mut bytes = Vec::with_capacity(small.min(super::frame::INITIAL_CAPACITY));
    while bytes.len() < small {
        let take = 1200.min(small - bytes.len());
        grow(&mut bytes, take, small);
        bytes.extend(std::iter::repeat_n(1u8, take));
    }
    assert_eq!(bytes.capacity(), small);
    assert!(Frame::from_parts(vec![bytes::Bytes::from(vec![0; MAX_FRAME + 1])]).is_err());
}
