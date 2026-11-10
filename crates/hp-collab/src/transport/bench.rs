use std::hint::black_box;
use std::time::{Duration, Instant};

use super::frame::{Frame, decode};
use super::guest::GuestCore;
use super::handshake::{Gate, Request};
use super::tests::{FakePeer, Host, fixture_map, point_edit, test_invitation, welcome_of};
use super::*;
use crate::document::Snapshot;
use crate::moderation::AccessPolicy;
use crate::protocol::{Participant, Transcript, Wire};

fn per_call(iterations: u32, mut run: impl FnMut()) -> f64 {
    for _ in 0..iterations / 10 {
        run();
    }
    let started = Instant::now();
    for _ in 0..iterations {
        run();
    }
    started.elapsed().as_secs_f64() * 1e6 / f64::from(iterations)
}

fn rows() -> Vec<Participant> {
    (1..=8)
        .map(|id| Participant {
            id,
            name: format!("Mapper{id}"),
            role: if id == 1 { Role::Host } else { Role::Editor },
            presence: Presence {
                position: [f64::from(id) * 64.0, 12.5, 300.0],
                direction: [0.0, 1.0, 0.0],
                selected: (0..10).map(|key| format!("solid:{}", key + id * 100)).collect(),
                ..Presence::default()
            },
            latency_ms: 40,
        })
        .collect()
}

#[test]
#[ignore = "profile: cargo test -p hp-collab --lib per_message_security_cost -- --ignored --nocapture"]
fn per_message_security_cost() {
    let map = fixture_map();
    let presence = Presence {
        position: [512.0, -64.0, 128.0],
        direction: [0.0, 0.7, 0.7],
        selected: (0..10).map(|key| format!("solid:{}", key + 100)).collect(),
        ..Presence::default()
    };
    let changes = point_edit(&map, "entity:2", "64 32 16");
    let applied = Wire::Applied { author: 2, sequence: 1, revision: 1, changes };
    let applied_bytes = serde_json::to_vec(&applied).unwrap();
    let presence_wire = Wire::Presence(presence.clone());
    let participants = Wire::Participants(rows());
    let participants_bytes = serde_json::to_vec(&participants).unwrap();
    let mut lines = Vec::new();
    lines.push(format!(
        "presence encode+decode {:.2} us, presence checks {:.3} us",
        per_call(20_000, || {
            let frame = Frame::encode(black_box(&presence_wire)).unwrap();
            black_box(decode(&frame.payload()).unwrap());
        }),
        per_call(200_000, || {
            black_box(black_box(&presence).validate());
        })
    ));
    lines.push(format!(
        "applied point edit ({} bytes) encode+decode {:.2} us",
        applied_bytes.len(),
        per_call(20_000, || {
            let frame = Frame::encode(black_box(&applied)).unwrap();
            black_box(decode(&frame.payload()).unwrap());
        })
    ));
    let decode_applied = per_call(100_000, || {
        black_box(decode(black_box(&applied_bytes)).unwrap());
    });
    let checked_applied = per_call(100_000, || {
        let mut core = GuestCore::at(0);
        black_box(core.handle(decode(black_box(&applied_bytes)).unwrap(), 1).unwrap());
    });
    let decode_rows = per_call(20_000, || {
        black_box(decode(black_box(&participants_bytes)).unwrap());
    });
    let checked_rows = per_call(20_000, || {
        let mut core = GuestCore::at(0);
        black_box(core.handle(decode(black_box(&participants_bytes)).unwrap(), 1).unwrap());
    });
    lines.push(format!(
        "guest checks: applied message {:.3} us over its {decode_applied:.2} us decode, 8 participant rows {:.2} us over {decode_rows:.2} us",
        (checked_applied - decode_applied).max(0.0),
        (checked_rows - decode_rows).max(0.0)
    ));
    lines.push(format!(
        "broadcast to 7 guests: one serialization per guest {:.2} us, one shared frame {:.2} us",
        per_call(20_000, || {
            for _ in 0..7 {
                black_box(serde_json::to_vec(black_box(&applied)).unwrap());
            }
        }),
        per_call(20_000, || {
            let frame = Frame::encode(black_box(&applied)).unwrap();
            for _ in 0..7 {
                black_box(frame.clone());
            }
        })
    ));
    let invitation = test_invitation();
    let keys = invitation.keys("benchmark-password");
    let device_key = iroh::SecretKey::from_bytes(&[4; 32]);
    let device = device_key.public().to_string();
    let (binding, nonce) = ([1; 32], [2; 32]);
    let transcript = Transcript {
        binding: &binding,
        nonce: &nonce,
        host: &invitation.endpoint,
        client: "client",
        name: "Guest",
        device: &device,
    };
    lines.push(format!(
        "join authentication on the guest: proofs and signature {:.1} us",
        per_call(2_000, || {
            let proof = keys.password_proof(black_box(&transcript));
            black_box(keys.invitation_proof(&transcript));
            black_box(device_key.sign(&proof));
        })
    ));
    let proof = keys.password_proof(&transcript);
    let request = Request {
        version: protocol::VERSION,
        name: "Guest".into(),
        session_name: invitation.room.name.clone(),
        signature: device_key.sign(&proof).to_bytes().to_vec(),
        invitation: keys.invitation_proof(&transcript),
        device: device.clone(),
        proof,
        binding,
        nonce,
        client: "client".into(),
    };
    let mut gate = Gate::new(invitation.clone(), "benchmark-password", Instant::now());
    lines.push(format!(
        "join authentication on the host: all checks {:.1} us",
        per_call(2_000, || {
            black_box(gate.verify(black_box(&request), Instant::now()).unwrap());
        })
    ));
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut peers: Vec<FakePeer> = (0..7)
        .map(|index| {
            let mut peer = host.join(&format!("p{index}"), &format!("d{index}"), &format!("P{index}"));
            welcome_of(&mut peer);
            peer
        })
        .collect();
    let mut index = 0u64;
    let presence_cost = per_call(20_000, || {
        index += 1;
        let mut moving = presence.clone();
        moving.position[0] = index as f64;
        host.send(&peers[0], Wire::Presence(moving));
        host.now += Duration::from_millis(70);
    });
    lines.push(format!("host handling of one presence update {presence_cost:.2} us"));
    let mut snapshot: Snapshot = map.clone();
    let mut sequence = 0;
    let edit_cost = per_call(2_000, || {
        sequence += 1;
        let changes = point_edit(&snapshot, "entity:2", &format!("{sequence} 0 0"));
        snapshot.0.insert("entity:2".into(), changes[0].after.clone().unwrap());
        host.send(&peers[1], Wire::Edit { sequence, changes });
        host.now += Duration::from_millis(20);
        for peer in &mut peers {
            while peer.queue.try_pop().is_some() {}
        }
        while host.events.try_recv().is_ok() {}
    });
    lines.push(format!("host handling of one point edit with its broadcast to 7 guests {edit_cost:.1} us"));
    let selected: Vec<Vec<String>> =
        (0..7).map(|peer| (0..2048).map(|key| format!("solid:{}", key + 10_000 * (peer + 1))).collect()).collect();
    let keys_in_edit: Vec<String> = (0..8192).map(|key| format!("entity:{}", 500_000 + key)).collect();
    lines.push(format!(
        "selection lock check of 8192 changes against 7 x 2048 selections: nested scan {:.1} ms, set {:.2} ms",
        per_call(3, || {
            for key in &keys_in_edit {
                black_box(selected.iter().any(|rows| rows.contains(key)));
            }
        }) / 1000.0,
        per_call(30, || {
            let locked: std::collections::HashSet<&str> =
                selected.iter().flat_map(|rows| rows.iter().map(String::as_str)).collect();
            for key in &keys_in_edit {
                black_box(locked.contains(key.as_str()));
            }
        }) / 1000.0
    ));
    for line in &lines {
        println!("COST {line}");
    }
}
