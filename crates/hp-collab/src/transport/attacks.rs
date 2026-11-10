use std::time::{Duration, Instant};

use iroh::EndpointAddr;
use serde::Deserialize;
use zeroize::Zeroizing;

use super::frame::{Frame, decode, discard_body, edit_sequence, read_frame, read_size, write_frame};
use super::guest::GuestCore;
use super::handshake::{Gate, MAX_HANDSHAKES_PER_ADDRESS, PEER_FRAME_LIMIT, Request};
use super::host::{Network, PRESENCE_PER_SECOND};
use super::queue::Kind;
use super::tests::{
    FakePeer, Host, RawClient, chained, direct_endpoint, fixture_map, joining, joining_from, joining_with, point_edit,
    start_direct_host, test_invitation, wait_for, welcome_of,
};
use super::*;
use crate::document::{Change, ID_STRIDE, Snapshot};
use crate::moderation::AccessPolicy;
use crate::protocol::{
    ALPN, EditRefusal, Invitation, LEGACY_ALPN, LEGACY_REFUSAL, Outcome, Participant, Refusal, Refused, Rejection,
    Transcript, VERSION, Wire,
};

fn request(invitation: &Invitation, password: &str, name: &str, device_secret: u8, binding: [u8; 32]) -> Request {
    let nonce = [11; 32];
    let keys = invitation.keys(password);
    let device_key = iroh::SecretKey::from_bytes(&[device_secret; 32]);
    let device = device_key.public().to_string();
    let client = "client".to_owned();
    let transcript = Transcript {
        binding: &binding,
        nonce: &nonce,
        host: &invitation.endpoint,
        client: &client,
        name,
        device: &device,
    };
    let proof = keys.password_proof(&transcript);
    Request {
        version: VERSION,
        name: name.into(),
        session_name: invitation.room.name.clone(),
        signature: device_key.sign(&proof).to_bytes().to_vec(),
        invitation: keys.invitation_proof(&transcript),
        device,
        proof,
        binding,
        nonce,
        client,
    }
}

fn refusal(result: std::result::Result<u64, Refused>) -> Option<Refusal> {
    result.err().map(|refused| refused.refusal)
}

#[test]
fn wrong_passwords_lock_out_and_are_counted_apart_from_garbage() {
    let now = Instant::now();
    let invitation = test_invitation();
    let mut gate = Gate::new(invitation.clone(), "right-password", now);
    assert_eq!(gate.verify(&request(&invitation, "right-password", "Alice", 1, [0; 32]), now), Ok(0));
    let mut stranger = invitation.clone();
    stranger.token = [99; 32];
    for _ in 0..50 {
        assert_eq!(
            refusal(gate.verify(&request(&stranger, "right-password", "Eve", 2, [0; 32]), now)),
            Some(Refusal::AuthenticationFailed)
        );
    }
    assert_eq!((gate.invalid_attempts, gate.failed_passwords), (50, 0));
    assert!(gate.limiter.locked(now).is_none());
    for attempt in 0..5 {
        assert_eq!(
            refusal(gate.verify(&request(&invitation, &format!("guess-{attempt:04}"), "Eve", 2, [0; 32]), now)),
            Some(Refusal::AuthenticationFailed)
        );
    }
    assert_eq!(gate.failed_passwords, 5);
    let locked = gate.verify(&request(&invitation, "right-password", "Alice", 1, [0; 32]), now).unwrap_err();
    assert_eq!(locked.refusal, Refusal::TooManyAttempts);
    assert_eq!(locked.seconds, Some(30));
    assert_eq!(gate.failed_passwords, 5, "guesses during a lockout are not even checked");
    let later = now + Duration::from_secs(31);
    assert_eq!(gate.verify(&request(&invitation, "right-password", "Alice", 1, [0; 32]), later), Ok(0));
    gate.rotate("another-password", later).unwrap();
    assert!(gate.limiter.locked(later).is_none());
    assert_eq!(
        refusal(gate.verify(&request(&invitation, "right-password", "Alice", 1, [0; 32]), later)),
        Some(Refusal::AuthenticationFailed)
    );
    let rotated = gate.invitation.clone();
    assert_eq!(gate.verify(&request(&rotated, "another-password", "Alice", 1, [0; 32]), later), Ok(1));
}

#[test]
fn admitted_installations_reconnect_while_an_invitation_holder_keeps_guessing() {
    let start = Instant::now();
    let invitation = test_invitation();
    let mut gate = Gate::new(invitation.clone(), "right-password", start);
    let alice = |gate: &mut Gate, now| gate.verify(&request(&invitation, "right-password", "Alice", 1, [0; 32]), now);
    assert_eq!(alice(&mut gate, start), Ok(0));
    gate.admit_device(&iroh::SecretKey::from_bytes(&[1; 32]).public().to_string());
    let (mut guesses, mut admitted, mut tries) = (0, 0, 0);
    for second in 0..3600u64 {
        let now = start + Duration::from_secs(second);
        if gate.limiter.locked(now).is_none() {
            guesses += 1;
            let _ = gate.verify(&request(&invitation, &format!("guess-{second:05}"), "Eve", 2, [0; 32]), now);
        }
        if second % 10 == 0 {
            tries += 1;
            admitted += usize::from(alice(&mut gate, now).is_ok());
        }
    }
    assert!(guesses >= 15 && gate.limiter.locked(start + Duration::from_secs(3599)).is_some());
    assert_eq!((admitted, tries), (360, 360), "an admitted installation is never paused");
    let bob = request(&invitation, "right-password", "Bob", 3, [0; 32]);
    let now = start + Duration::from_secs(3599);
    assert_eq!(gate.verify(&bob, now).map_err(|refused| refused.refusal), Err(Refusal::TooManyAttempts));
    let stranger = request(&invitation, "wrong-password", "Alice", 1, [0; 32]);
    assert_eq!(
        gate.verify(&stranger, now).map_err(|refused| refused.refusal),
        Err(Refusal::AuthenticationFailed),
        "an admitted installation still needs the right password"
    );
    gate.forget_device(&iroh::SecretKey::from_bytes(&[1; 32]).public().to_string());
    assert_eq!(alice(&mut gate, now).map_err(|refused| refused.refusal), Err(Refusal::TooManyAttempts));
}

#[test]
fn the_host_marks_admitted_installations_for_the_password_pause() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let invitation = host.gate.lock().unwrap().invitation.clone();
    let device = iroh::SecretKey::from_bytes(&[1; 32]).public().to_string();
    let mut alice = host.join("a", &device, "Alice");
    let (participant, _, _) = welcome_of(&mut alice);
    let now = host.now;
    for attempt in 0..6 {
        let guess = request(&invitation, &format!("guess-{attempt}"), "Eve", 2, [0; 32]);
        let _ = host.gate.lock().unwrap().verify(&guess, now);
    }
    let returning = request(&invitation, "test-password", "Alice", 1, [0; 32]);
    assert_eq!(host.gate.lock().unwrap().verify(&returning, now), Ok(0));
    host.command(Command::Kick(participant));
    assert_eq!(
        host.gate.lock().unwrap().verify(&returning, now).map_err(|refused| refused.refusal),
        Err(Refusal::TooManyAttempts),
        "a removed installation is a stranger again"
    );
}

#[test]
fn proofs_are_bound_to_the_channel_nonce_client_name_and_device_key() {
    let now = Instant::now();
    let invitation = test_invitation();
    let mut gate = Gate::new(invitation.clone(), "right-password", now);
    let good = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    let mut replayed = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    replayed.binding = [2; 32];
    let mut other_nonce = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    other_nonce.nonce = [3; 32];
    let mut other_client = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    other_client.client = "attacker".into();
    let mut renamed = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    renamed.name = "Mallory".into();
    let mut stolen_device = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    stolen_device.device = iroh::SecretKey::from_bytes(&[8; 32]).public().to_string();
    let mut bad_signature = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    bad_signature.signature[0] ^= 1;
    let mut short_signature = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    short_signature.signature.truncate(10);
    let mut old_version = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    old_version.version = 1;
    let mut long_device = request(&invitation, "right-password", "Alice", 1, [1; 32]);
    long_device.device = "a".repeat(5000);
    for forged in [
        replayed,
        other_nonce,
        other_client,
        renamed,
        stolen_device,
        bad_signature,
        short_signature,
        old_version,
        long_device,
    ] {
        assert_eq!(refusal(gate.verify(&forged, now)), Some(Refusal::AuthenticationFailed));
    }
    assert_eq!(gate.failed_passwords, 0);
    assert_eq!(gate.verify(&good, now), Ok(0));
}

#[test]
fn handshake_slots_are_limited_per_address() {
    let mut gate = Gate::new(test_invitation(), "right-password", Instant::now());
    let attacker: std::net::IpAddr = "198.51.100.7".parse().unwrap();
    for _ in 0..MAX_HANDSHAKES_PER_ADDRESS {
        assert!(gate.admit_address(Some(attacker)));
    }
    assert!(!gate.admit_address(Some(attacker)));
    assert!(gate.admit_address(Some("198.51.100.8".parse().unwrap())));
    assert!(gate.admit_address(None));
}

#[test]
fn viewers_cannot_edit_and_their_selection_locks_nothing() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy { read_only_guests: true, ..AccessPolicy::default() });
    let mut viewer = host.join("viewer", "device-v", "Viewer");
    let (participant, _, _) = welcome_of(&mut viewer);
    assert_eq!(host.core.role_of(participant), Some(Role::Viewer));
    host.advance(Duration::ZERO);
    let shared = viewer.participants.borrow().clone().expect("participants frame");
    let Wire::Participants(rows) = decode(&shared.payload()).unwrap() else { panic!("participants") };
    assert_eq!(
        rows.iter().map(|row| (row.id, row.role)).collect::<Vec<_>>(),
        [(1, Role::Host), (participant, Role::Viewer)]
    );
    host.send(&viewer, Wire::Presence(Presence { selected: vec!["entity:2".into()], ..Presence::default() }));
    assert_eq!(host.core.locked_count(1), 0);
    host.send(&viewer, Wire::Edit { sequence: 1, changes: point_edit(&map, "entity:2", "64 0 0") });
    let frames = viewer.frames();
    assert!(
        matches!(&frames[..], [(Wire::Rejected { sequence: 1, rejection, revision: 0 }, _, _)] if rejection.code == EditRefusal::ReadOnly)
    );
    host.command(Command::Edit { sequence: 1, changes: point_edit(&map, "entity:2", "32 0 0") });
    assert!(host.drain().iter().any(|event| matches!(event, Event::Applied { author: 1, .. })));
    host.command(Command::SetRole { participant, role: Role::Editor });
    host.send(&viewer, Wire::Presence(Presence { selected: vec!["entity:2".into()], ..Presence::default() }));
    assert_eq!(host.core.locked_count(1), 1);
    host.command(Command::SetRole { participant, role: Role::Host });
    assert_eq!(host.core.role_of(participant), Some(Role::Editor));
}

#[test]
fn rejected_edits_never_carry_the_map_back() {
    let mut text =
        String::from("world {\"id\" \"1\"} entity {\"id\" \"2\" \"classname\" \"info_target\" \"origin\" \"0 0 0\"");
    for index in 0..64 {
        text.push_str(&format!("\"field{index}\" \"{}\"", "x".repeat(16_000)));
    }
    text.push('}');
    let map = Snapshot::from_vmf(&text).unwrap();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    let (_, _, welcome) = welcome_of(&mut peer);
    assert_eq!(welcome, map);
    let mut stale = point_edit(&map, "entity:2", "8 0 0");
    stale[0].before = Some([0; 32]);
    for sequence in 1..=20 {
        host.send(&peer, Wire::Edit { sequence, changes: stale.clone() });
    }
    let frames = peer.frames();
    assert_eq!(frames.len(), 20, "every refusal reaches the participant");
    assert!(frames.iter().all(|(message, _, size)| matches!(message, Wire::Rejected { .. }) && *size < 512));
    assert_eq!(peer.closed(), None);
    assert_eq!(host.core.peer_count(), 1);
}

#[test]
fn a_guest_whose_welcome_is_still_being_written_keeps_up_through_its_backlog() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut editor = host.join("editor", "device-e", "Editor");
    welcome_of(&mut editor);
    let joiner = host.join("joiner", "device-j", "Joiner");
    let mut state = map.clone();
    for sequence in 1..=200 {
        host.send(
            &editor,
            Wire::Edit { sequence, changes: chained(&mut state, "entity:2", &format!("{sequence} 0 0")) },
        );
        editor.frames();
    }
    assert_eq!(joiner.closed(), None);
    let mut joiner = joiner;
    let frames = joiner.frames();
    assert_eq!(frames[0].1, Kind::Welcome);
    let revisions: Vec<u64> = frames
        .iter()
        .filter_map(|(message, _, _)| match message {
            Wire::Applied { revision, .. } => Some(*revision),
            _ => None,
        })
        .collect();
    assert_eq!(revisions, (1..=200).collect::<Vec<_>>());
}

#[test]
fn a_participant_that_falls_too_far_behind_is_resynchronized_with_its_own_outcomes() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let epoch = host.gate.lock().unwrap().epoch;
    let (joining, mut slow) = joining_with("slow", "device-s", "Slow", epoch, None, 4096);
    host.core.network(Network::Joined(Box::new(joining)), host.now).unwrap();
    let mut fast = host.join("fast", "device-f", "Fast");
    welcome_of(&mut fast);
    let mut state = map.clone();
    host.send(&slow, Wire::Edit { sequence: 1, changes: chained(&mut state, "entity:3", "100 0 0") });
    let mut stale = point_edit(&map, "entity:3", "200 0 0");
    stale[0].before = Some([0; 32]);
    host.send(&slow, Wire::Edit { sequence: 2, changes: stale });
    for sequence in 1..=40 {
        host.send(&fast, Wire::Edit { sequence, changes: chained(&mut state, "entity:2", &format!("{sequence} 0 0")) });
        fast.frames();
    }
    assert_eq!(slow.closed(), None, "a slow participant is never kicked");
    assert!(host.access().resyncs >= 1);
    let frames = slow.frames();
    assert_eq!(frames[0].1, Kind::Welcome);
    assert_eq!(
        frames.iter().filter(|(message, _, _)| matches!(message, Wire::Resync { .. })).count(),
        1,
        "one pending resync at most"
    );
    let Some((Wire::Resync { outcomes, .. }, Kind::Resync, _)) =
        frames.iter().find(|(message, _, _)| matches!(message, Wire::Resync { .. }))
    else {
        panic!("resync")
    };
    assert_eq!(outcomes[0], Outcome { sequence: 1, rejection: None });
    assert_eq!(outcomes[1].sequence, 2);
    assert_eq!(outcomes[1].rejection.as_ref().map(|rejection| rejection.code), Some(EditRefusal::Document));
    let mut frames = frames.into_iter();
    let mut core = GuestCore::start(frames.next().unwrap().0).unwrap();
    let mut guest = None;
    for (message, _, _) in frames {
        match core.handle(message, 0).unwrap() {
            Some(Event::Ready { map, .. } | Event::Resync { map, .. }) => guest = Some(map),
            Some(Event::Applied { mut changes, .. }) => {
                let map = guest.as_mut().expect("map first");
                crate::document::expand_changes(&mut changes, map).unwrap();
                map.apply(&changes).unwrap();
            }
            _ => {}
        }
    }
    assert_eq!(core.revision, host.core.revision(), "the guest ends at the host revision");
    assert_eq!(guest.as_deref(), Some(host.core.snapshot()), "and with the host map");
}

#[test]
fn oversized_edit_frames_get_a_precise_rejection_and_keep_the_participant() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    welcome_of(&mut peer);
    host.core
        .network(
            Network::Oversized { id: "peer".into(), generation: 7, sequence: 4, size: PEER_FRAME_LIMIT + 10 },
            host.now,
        )
        .unwrap();
    host.work();
    let frames = peer.frames();
    let [(Wire::Rejected { sequence: 4, rejection, .. }, _, _)] = &frames[..] else { panic!("rejection") };
    assert_eq!(rejection.code, EditRefusal::TooLarge);
    assert!(rejection.detail.contains("MiB"), "{}", rejection.detail);
    host.send(&peer, Wire::Edit { sequence: 5, changes: point_edit(&map, "entity:2", "8 0 0") });
    assert!(matches!(&peer.frames()[..], [(Wire::Acked { sequence: 5, .. }, _, _)]));
    host.core
        .network(Network::Oversized { id: "peer".into(), generation: 7, sequence: 5, size: usize::MAX }, host.now)
        .unwrap();
    assert_eq!(host.core.peer_count(), 0, "a replayed sequence is still a protocol error");
    for (prefix, expected) in [
        (&b"{\"Edit\":{\"sequence\":42,\"changes\":["[..], Some(42)),
        (b"{\"Edit\":{\"sequence\":18446744073709551615,", Some(u64::MAX)),
        (b"{\"Edit\":{\"sequence\":4", None),
        (b"{\"Presence\":{\"sequence\":4,", None),
        (b"", None),
    ] {
        assert_eq!(edit_sequence(prefix), expected);
    }
}

#[tokio::test]
async fn oversized_frames_are_skipped_without_holding_them_in_memory() {
    let server = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .clear_address_lookup()
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let client = direct_endpoint().await;
    let port = server.addr().ip_addrs().find(|address| address.is_ipv4()).unwrap().port();
    let address = EndpointAddr::new(server.id()).with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
    let mut payload = b"{\"Edit\":{\"sequence\":77,\"changes\":[".to_vec();
    payload.resize(3 * 1024 * 1024, b' ');
    let sender = async {
        let connection = client.connect(address, ALPN).await.unwrap();
        let (mut send, _) = connection.open_bi().await.unwrap();
        send.write_all(&(payload.len() as u32).to_be_bytes()).await.unwrap();
        send.write_all(&payload).await.unwrap();
        write_frame(&mut send, &Frame::encode(&Wire::Ping(5)).unwrap(), DEADLINE).await.unwrap();
        send.finish().unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(5), send.stopped()).await;
    };
    let receiver = async {
        let connection = server.accept().await.unwrap().await.unwrap();
        let (_, mut recv) = connection.accept_bi().await.unwrap();
        let header = read_size(&mut recv, DEADLINE).await.unwrap();
        assert_eq!(header.size, payload.len());
        let prefix = discard_body(&mut recv, &header, DEADLINE).await.unwrap();
        assert_eq!(edit_sequence(&prefix), Some(77));
        let next = read_frame(&mut recv, 1024, DEADLINE, DEADLINE).await.unwrap();
        assert!(matches!(decode(&next).unwrap(), Wire::Ping(5)), "the stream stays usable after a skipped frame");
    };
    tokio::join!(sender, receiver);
    client.close().await;
    server.close().await;
}

#[test]
fn edit_floods_are_queued_and_served_in_turn_instead_of_refused() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut flood = host.join("flood", "device-f", "Flood");
    welcome_of(&mut flood);
    let mut calm = host.join("calm", "device-c", "Calm");
    let (quiet, _, _) = welcome_of(&mut calm);
    let mut state = map.clone();
    for sequence in 1..=500 {
        host.deliver(
            &flood,
            Wire::Edit { sequence, changes: chained(&mut state, "entity:3", &format!("{sequence} 0 0")) },
        );
    }
    host.deliver(&calm, Wire::Edit { sequence: 1, changes: chained(&mut state, "entity:2", "1 2 3") });
    let own = chained(&mut state, "entity:2", "9 9 9");
    assert_eq!(host.core.deferred_count(), 501);
    host.core.work().unwrap();
    host.core.work().unwrap();
    host.command(Command::Edit { sequence: 1, changes: own });
    let authors: Vec<u32> = host
        .drain()
        .into_iter()
        .filter_map(|event| match event {
            Event::Applied { author, .. } => Some(author),
            _ => None,
        })
        .collect();
    assert!(authors.contains(&quiet), "a quiet participant is served in turn, not after the flood: {authors:?}");
    assert_eq!(authors.last(), Some(&1), "the host's own edit runs between two queued guest edits");
    host.work();
    assert_eq!(host.core.deferred_count(), 0);
    let frames = flood.frames();
    let acked = frames.iter().filter(|(message, _, _)| matches!(message, Wire::Acked { .. })).count();
    let applied = frames.iter().filter(|(message, _, _)| matches!(message, Wire::Applied { .. })).count();
    assert_eq!((acked, applied), (500, 1 + 1), "its own edits are acknowledged, the others arrive in full");
    assert!(flood.frames().is_empty());
    assert_eq!(host.core.peer_count(), 2);
    host.core.network(Network::Flooded { id: "flood".into(), generation: 7 }, host.now).unwrap();
    host.core.network(Network::Flooded { id: "flood".into(), generation: 7 }, host.now).unwrap();
    assert_eq!(host.access().rate_limited, 1, "a slowed participant is counted once and stays connected");
    assert_eq!(host.core.peer_count(), 2);
}

#[test]
fn presence_floods_are_dropped_and_invalid_selection_disconnects() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    let (participant, _, _) = welcome_of(&mut peer);
    for index in 0..100 {
        host.send(&peer, Wire::Presence(Presence { position: [index as f64, 0.0, 0.0], ..Presence::default() }));
        host.send(&peer, Wire::Ping(index));
    }
    let presence = host.core.presence_of(participant).unwrap();
    assert_eq!(presence.position[0], f64::from(PRESENCE_PER_SECOND - 1));
    assert_eq!(host.core.peer_count(), 1);
    for key in ["world", "metadata", "solid:0", "entity:007"] {
        let mut host = Host::new(fixture_map(), AccessPolicy::default());
        let mut peer = host.join("peer", "device", "Guest");
        welcome_of(&mut peer);
        host.send(&peer, Wire::Presence(Presence { selected: vec![key.into()], ..Presence::default() }));
        assert_eq!(host.core.peer_count(), 0, "{key}");
        assert!(
            matches!(peer.frames().last(), Some((Wire::Closed(refused), Kind::Closing, _)) if refused.refusal == Refusal::Protocol)
        );
    }
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    welcome_of(&mut peer);
    for message in [
        Wire::Hello { version: 2 },
        Wire::MapStart { participant: 9, id_start: 0, revision: 0, units: 0, bytes: 0 },
        Wire::Applied { author: 1, sequence: 1, revision: 1, changes: Vec::new() },
    ] {
        let mut host = Host::new(fixture_map(), AccessPolicy::default());
        let mut peer = host.join("peer", "device", "Guest");
        welcome_of(&mut peer);
        host.send(&peer, message);
        assert_eq!(host.core.peer_count(), 0);
    }
    host.send(&peer, Wire::Presence(Presence { position: [f64::NAN, 0.0, 0.0], ..Presence::default() }));
    assert_eq!(host.core.peer_count(), 0);
}

#[test]
fn locked_sessions_and_expired_invitations_only_readmit_known_devices() {
    let mut host = Host::new(fixture_map(), AccessPolicy { invitation_minutes: 10, ..AccessPolicy::default() });
    let mut first = host.join("a1", "device-a", "Alice");
    welcome_of(&mut first);
    host.core.network(Network::Left { id: "a1".into(), generation: 7 }, host.now).unwrap();
    host.advance(Duration::from_secs(601));
    assert_eq!(host.access().new_devices_seconds, Some(0));
    let mut stranger = host.join("b1", "device-b", "Bob");
    assert!(
        matches!(stranger.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Locked)
    );
    let mut back = host.join("a2", "device-a", "Alice");
    welcome_of(&mut back);
    host.command(Command::Policy(AccessPolicy { locked: true, ..AccessPolicy::default() }));
    let mut other = host.join("c1", "device-c", "Carol");
    assert!(matches!(other.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Locked));
    assert_eq!(host.access().refused_joins, 2);
    host.command(Command::Policy(AccessPolicy::default()));
    let mut carol = host.join("c2", "device-c", "Carol");
    welcome_of(&mut carol);
}

#[test]
fn approval_waiting_room_admits_declines_and_times_out() {
    let mut host = Host::new(fixture_map(), AccessPolicy { approval: true, ..AccessPolicy::default() });
    let mut alice = host.join("a", "device-a", "Alice");
    assert!(matches!(&alice.frames()[..], [(Wire::Waiting, Kind::Normal, _)]));
    let pending = host.access().pending;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].name, "Alice");
    let mut twin = host.join("t", "device-t", "ALICE");
    assert!(
        matches!(twin.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::NicknameTaken)
    );
    host.command(Command::Admit(pending[0].id));
    let (participant, _, _) = welcome_of(&mut alice);
    assert_eq!(participant, pending[0].id);
    host.core.network(Network::Left { id: "a".into(), generation: 7 }, host.now).unwrap();
    let mut again = host.join("a2", "device-a", "Alice");
    welcome_of(&mut again);
    let mut bob = host.join("b", "device-b", "Bob");
    bob.frames();
    host.command(Command::Decline(host.access().pending[0].id));
    assert!(
        matches!(bob.frames().last(), Some((Wire::Closed(refused), Kind::Closing, _)) if refused.refusal == Refusal::Declined)
    );
    let mut carol = host.join("c", "device-c", "Carol");
    carol.frames();
    host.advance(APPROVAL_DEADLINE);
    assert!(
        matches!(carol.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::ApprovalTimeout)
    );
    assert!(host.access().pending.is_empty());
    let mut dave = host.join("d", "device-d", "Dave");
    dave.frames();
    let id = host.access().pending[0].id;
    host.command(Command::Ban { participant: id, seconds: None, reason: "unknown".into() });
    assert!(matches!(dave.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Banned));
    let mut dave_again = host.join("d2", "device-d", "Dave");
    assert!(
        matches!(dave_again.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Banned)
    );
}

#[test]
fn kick_forgets_the_device_and_rotation_revokes_everyone() {
    let mut host = Host::new(fixture_map(), AccessPolicy { approval: true, ..AccessPolicy::default() });
    let mut alice = host.join("a", "device-a", "Alice");
    alice.frames();
    host.command(Command::Admit(host.access().pending[0].id));
    let (participant, _, _) = welcome_of(&mut alice);
    host.command(Command::Kick(participant));
    assert!(
        matches!(alice.frames().last(), Some((Wire::Closed(refused), Kind::Closing, _)) if refused.refusal == Refusal::Removed)
    );
    let mut back = host.join("a2", "device-a", "Alice");
    assert!(matches!(&back.frames()[..], [(Wire::Waiting, _, _)]));
    host.command(Command::Admit(host.access().pending[0].id));
    welcome_of(&mut back);
    let mut waiting = host.join("b", "device-b", "Bob");
    waiting.frames();
    host.command(Command::Rotate(Zeroizing::new("rotated-password".into())));
    for peer in [&mut back, &mut waiting] {
        assert!(
            matches!(peer.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Revoked)
        );
    }
    assert!(host.drain().iter().any(|event| matches!(event, Event::Invitation(_))));
    let (stale, mut stale_peer) = joining("old", "device-a", "Alice", 0);
    host.core.network(Network::Joined(Box::new(stale)), host.now).unwrap();
    assert!(
        matches!(stale_peer.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Revoked)
    );
    let mut fresh = host.join("a3", "device-a", "Alice");
    assert!(matches!(&fresh.frames()[..], [(Wire::Waiting, _, _)]), "rotation forgets admitted devices");
}

#[test]
fn reconnect_storms_reuse_one_identifier_range_and_replace_stale_connections() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let mut first = host.join("a0", "device-a", "Alice");
    let (_, range, _) = welcome_of(&mut first);
    for index in 1..3000 {
        let mut peer = host.join(&format!("a{index}"), "device-a", "Alice");
        let (_, again, _) = welcome_of(&mut peer);
        assert_eq!(again, range);
    }
    assert_eq!(host.core.peer_count(), 1);
    assert_eq!(first.closed(), Some(Refusal::Replaced));
    let mut bob = host.join("b", "device-b", "Bob");
    let (_, bob_range, _) = welcome_of(&mut bob);
    assert_eq!(bob_range, range + ID_STRIDE);
    let mut thief = host.join("x", "device-x", "Bob");
    assert!(
        matches!(thief.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::NicknameTaken)
    );
    assert_eq!(host.core.peer_count(), 2);
}

#[test]
fn sessions_refuse_participants_beyond_the_limit_and_devices_beyond_the_budget() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let mut peers: Vec<FakePeer> = Vec::new();
    for index in 0..protocol::MAX_PARTICIPANTS - 1 {
        let mut peer = host.join(&format!("p{index}"), &format!("d{index}"), &format!("P{index}"));
        welcome_of(&mut peer);
        peers.push(peer);
    }
    let mut extra = host.join("extra", "d-extra", "Extra");
    assert!(matches!(extra.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Full));
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    for index in 0..super::host::MAX_DEVICES {
        let mut peer = host.join("p", &format!("device{index}"), "Guest");
        welcome_of(&mut peer);
        host.core.network(Network::Left { id: "p".into(), generation: 7 }, host.now).unwrap();
    }
    let mut late = host.join("p", "device-late", "Guest");
    assert!(matches!(late.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Full));
}

#[test]
fn two_live_connections_of_one_installation_get_distinct_identifier_ranges() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let mut alice = host.join("a", "shared-device", "Alice");
    let (alice_id, alice_range, _) = welcome_of(&mut alice);
    let mut bob = host.join("b", "shared-device", "Bob");
    let (bob_id, bob_range, _) = welcome_of(&mut bob);
    assert_ne!(alice_range, bob_range);
    assert_eq!(host.core.peer_count(), 2);
    assert_eq!((host.core.range_of(alice_id), host.core.range_of(bob_id)), (Some(alice_range), Some(bob_range)));
    host.core.network(Network::Left { id: "a".into(), generation: 7 }, host.now).unwrap();
    let mut carol = host.join("c", "shared-device", "Carol");
    let (_, carol_range, _) = welcome_of(&mut carol);
    assert_eq!(carol_range, alice_range, "a free range of the installation is reused");
    let mut bob_again = host.join("b2", "shared-device", "Bob");
    let (_, again, _) = welcome_of(&mut bob_again);
    assert_eq!(again, bob_range, "a reconnection of the same nickname replaces it and keeps its range");
    assert_eq!(bob.closed(), Some(Refusal::Replaced));
    let mut dave = host.join("d", "other-device", "Dave");
    let (_, dave_range, _) = welcome_of(&mut dave);
    assert!(dave_range > alice_range.max(bob_range));
}

#[test]
fn replayed_or_reordered_edit_sequences_disconnect() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    welcome_of(&mut peer);
    host.send(&peer, Wire::Edit { sequence: 5, changes: point_edit(&map, "entity:2", "1 0 0") });
    assert!(matches!(&peer.frames()[..], [(Wire::Acked { sequence: 5, revision: 1 }, _, _)]));
    host.send(&peer, Wire::Edit { sequence: 5, changes: point_edit(&map, "entity:2", "2 0 0") });
    assert_eq!(host.core.peer_count(), 0);
    assert!(
        matches!(peer.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Protocol)
    );
}

#[test]
fn edits_outside_the_participant_range_or_with_bad_keys_are_rejected() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    let (_, range, _) = welcome_of(&mut peer);
    let unit = |id: u32| crate::document::Unit {
        owner: 0,
        hidden: false,
        vmf: format!("entity {{\"id\" \"{id}\" \"classname\" \"info_target\" \"origin\" \"0 0 0\"}}"),
    };
    let cases = [
        (format!("entity:{}", host.core.host_range + 5), EditRefusal::OutsideRange),
        ("entity:0004".into(), EditRefusal::InvalidKey),
        ("World".into(), EditRefusal::InvalidKey),
    ];
    for (sequence, (key, expected)) in cases.iter().enumerate() {
        let id = protocol::object_number(key).unwrap_or(4);
        let changes = vec![Change { key: key.clone(), before: None, after: Some(unit(id)) }];
        host.send(&peer, Wire::Edit { sequence: sequence as u64 + 1, changes });
        assert!(
            matches!(peer.frames().last(), Some((Wire::Rejected { rejection, .. }, _, _)) if rejection.code == *expected && rejection.detail == *key),
            "{key}"
        );
    }
    host.send(&peer, Wire::Edit { sequence: 9, changes: Vec::new() });
    assert!(
        matches!(peer.frames().last(), Some((Wire::Rejected { rejection, .. }, _, _)) if rejection.code == EditRefusal::InvalidEdit)
    );
    let changes = vec![Change { key: format!("entity:{}", range + 5), before: None, after: Some(unit(range + 5)) }];
    host.send(&peer, Wire::Edit { sequence: 10, changes });
    assert!(matches!(peer.frames().last(), Some((Wire::Acked { .. }, _, _))));
    let side = |solid: u32, side: u32| crate::document::Unit {
        owner: 1,
        hidden: false,
        vmf: format!(
            "solid {{\"id\" \"{solid}\" side {{\"id\" \"{side}\" \"plane\" \"(0 0 0) (0 64 0) (64 64 0)\" \"material\" \"x\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\"}}}}"
        ),
    };
    let key = format!("solid:{}", range + 6);
    let foreign = vec![Change { key: key.clone(), before: None, after: Some(side(range + 6, 77)) }];
    host.send(&peer, Wire::Edit { sequence: 11, changes: foreign });
    assert!(
        matches!(peer.frames().last(), Some((Wire::Rejected { rejection, .. }, _, _)) if rejection.code == EditRefusal::SideOutsideRange && rejection.detail == "77")
    );
    let owned = vec![Change { key, before: None, after: Some(side(range + 6, range + 7)) }];
    host.send(&peer, Wire::Edit { sequence: 12, changes: owned });
    assert!(matches!(peer.frames().last(), Some((Wire::Acked { .. }, _, _))));
}

#[test]
fn edits_of_thousands_of_objects_are_shared_without_a_count_limit() {
    let mut text = String::from("world {\"id\" \"1\" \"classname\" \"worldspawn\"}");
    for id in 2..9002 {
        text.push_str(&format!("entity {{\"id\" \"{id}\" \"classname\" \"info_target\" \"origin\" \"0 0 0\"}}"));
    }
    let map = Snapshot::from_vmf(&text).unwrap();
    let mut moved = map.clone();
    for unit in moved.0.values_mut().filter(|unit| unit.vmf.contains("info_target")) {
        unit.vmf = unit.vmf.replace("\"0 0 0\"", "\"64 0 0\"");
    }
    let changes = map.diff(&moved);
    assert_eq!(changes.len(), 9000);
    let mut guest = GuestCore::at(0);
    let applied = Wire::Applied { author: 2, sequence: 1, revision: 1, changes: changes.clone() };
    let Ok(Some(Event::Applied { changes: received, .. })) =
        guest.handle(decode(&serde_json::to_vec(&applied).unwrap()).unwrap(), 0)
    else {
        panic!("guest")
    };
    assert_eq!(received.len(), 9000, "guests take any number of changes");
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    welcome_of(&mut peer);
    let range = host.core.host_range;
    match host.core.try_apply(&changes, 1, range) {
        Ok(()) => {}
        Err(rejection) => {
            assert_eq!(rejection.code, EditRefusal::Document, "only the map rules may refuse: {rejection}")
        }
    }
    host.send(&peer, Wire::Edit { sequence: 1, changes: moved.diff(&map) });
    assert!(matches!(
        &peer.frames()[..],
        [(Wire::Acked { .. }, _, _)]
            | [(Wire::Rejected { rejection: Rejection { code: EditRefusal::Document, .. }, .. }, _, _)]
    ));
    assert_eq!(host.core.peer_count(), 1);
}

#[test]
fn selection_lock_checks_stay_linear_with_many_selected_objects() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    for index in 0..protocol::MAX_PARTICIPANTS - 1 {
        let mut peer = host.join(&format!("p{index}"), &format!("d{index}"), &format!("P{index}"));
        welcome_of(&mut peer);
        let selected = (0..protocol::MAX_SELECTED).map(|id| format!("solid:{}", id + 10_000 * (index + 1))).collect();
        host.send(&peer, Wire::Presence(Presence { selected, ..Presence::default() }));
    }
    assert_eq!(host.core.locked_count(1), (protocol::MAX_PARTICIPANTS - 1) * protocol::MAX_SELECTED);
    let changes: Vec<Change> = (0..8192)
        .map(|index| Change { key: format!("entity:{}", 500_000 + index), before: Some([1; 32]), after: None })
        .collect();
    let started = Instant::now();
    let range = host.core.host_range;
    assert!(host.core.try_apply(&changes, 1, range).is_err());
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_millis(250), "lock check took {elapsed:?}");
}

#[test]
fn ban_reasons_in_any_language_never_end_the_session() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let mut peer = host.join("p", "device", "Guest");
    let (participant, _, _) = welcome_of(&mut peer);
    let reason = "Нарушение правил сеанса ".repeat(10);
    assert!(reason.len() > 256 && reason.chars().count() <= 256);
    host.command(Command::Ban { participant, seconds: Some(u64::MAX), reason: format!("{reason}\n") });
    let Some((Wire::Closed(refused), Kind::Closing, _)) = peer.frames().pop() else { panic!("ban") };
    assert_eq!(refused.refusal, Refusal::Banned);
    assert!(!refused.detail.contains('\n'));
    assert_eq!(refused.seconds, Some(crate::moderation::MAX_BAN_SECONDS));
}

#[test]
fn banned_addresses_are_matched_even_with_a_new_device() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let ip = Some("203.0.113.9".parse().unwrap());
    let epoch = host.gate.lock().unwrap().epoch;
    let (joining, mut peer) = joining_from("p", "device-1", "Guest", epoch, ip);
    host.core.network(Network::Joined(Box::new(joining)), host.now).unwrap();
    let (participant, _, _) = welcome_of(&mut peer);
    host.command(Command::Ban { participant, seconds: None, reason: String::new() });
    let (joining, mut peer) = joining_from("q", "device-2", "Other", epoch, ip);
    host.core.network(Network::Joined(Box::new(joining)), host.now).unwrap();
    assert!(
        matches!(peer.frames().last(), Some((Wire::Closed(refused), _, _)) if refused.refusal == Refusal::Banned && refused.detail == "Banned by the host")
    );
}

#[test]
fn a_hostile_host_cannot_push_inconsistent_state_to_guests() {
    let map = fixture_map();
    let units = map.0.len() as u64;
    for bad in [
        Wire::MapStart { participant: 1, id_start: 2_000_000, revision: 0, units, bytes: 0 },
        Wire::MapStart { participant: 2, id_start: 2_000_001, revision: 0, units, bytes: 0 },
        Wire::MapStart { participant: 2, id_start: 0, revision: 0, units, bytes: 0 },
        Wire::MapStart { participant: 2, id_start: 1_995_000_000, revision: 0, units, bytes: 0 },
        Wire::MapStart { participant: 2, id_start: 2_000_000, revision: 0, units: u64::MAX, bytes: 0 },
        Wire::Applied { author: 1, sequence: 1, revision: 1, changes: Vec::new() },
        Wire::Waiting,
    ] {
        assert!(GuestCore::start(bad).is_err());
    }
    let start = || Wire::MapStart { participant: 2, id_start: 2_000_000, revision: 5, units, bytes: 0 };
    let mut core = GuestCore::start(start()).unwrap();
    assert!(core.handle(Wire::Ping(1), 0).is_err(), "nothing else interrupts a map transfer");
    let mut core = GuestCore::start(start()).unwrap();
    assert!(core.handle(Wire::MapPart(map.clone()), 0).is_ok());
    assert!(core.handle(Wire::MapPart(map.clone()), 0).is_err(), "duplicate units are refused");
    let mut core = GuestCore::start(Wire::MapStart {
        participant: 2,
        id_start: 2_000_000,
        revision: 5,
        units: units - 1,
        bytes: 0,
    })
    .unwrap();
    assert!(core.handle(Wire::MapPart(map.clone()), 0).is_err(), "more units than announced are refused");
    let mut core = GuestCore::start(Wire::MapStart {
        participant: 2,
        id_start: 2_000_000,
        revision: 5,
        units: units + 1,
        bytes: 0,
    })
    .unwrap();
    core.handle(Wire::MapPart(map.clone()), 0).unwrap();
    assert!(core.handle(Wire::MapEnd, 0).is_err(), "an incomplete map is refused");
    let mut core = GuestCore::start(start()).unwrap();
    assert!(matches!(
        core.handle(Wire::MapPart(map.clone()), 0),
        Ok(Some(Event::JoinProgress { units: got, total, .. })) if got == units && total == units
    ));
    assert!(matches!(core.handle(Wire::MapEnd, 0), Ok(Some(Event::Ready { revision: 5, participant: 2, .. }))));
    let edit = point_edit(&map, "entity:2", "4 0 0");
    assert!(core.handle(Wire::Applied { author: 1, sequence: 1, revision: 7, changes: edit.clone() }, 0).is_err());
    let mut core = GuestCore::at(5);
    assert!(core.handle(Wire::Applied { author: 0, sequence: 1, revision: 6, changes: edit.clone() }, 0).is_err());
    let mut core = GuestCore::at(5);
    let bad_key = vec![Change { key: "../x".into(), before: None, after: None }];
    assert!(core.handle(Wire::Applied { author: 1, sequence: 1, revision: 6, changes: bad_key }, 0).is_err());
    let mut core = GuestCore::at(5);
    let rejection = Rejection::new(EditRefusal::Document, "x");
    assert!(core.handle(Wire::Rejected { sequence: 1, rejection, revision: 4 }, 0).is_err());
    let mut core = GuestCore::at(5);
    let hostile = Rejection { code: EditRefusal::Document, detail: format!("\u{1b}[31m{}", "y".repeat(10_000)) };
    let Ok(Some(Event::Rejected { rejection, .. })) =
        core.handle(Wire::Rejected { sequence: 1, rejection: hostile.clone(), revision: 5 }, 0)
    else {
        panic!("rejected")
    };
    assert_eq!(rejection.detail.chars().count(), protocol::MAX_REASON_CHARS);
    assert!(!rejection.detail.contains('\u{1b}'));
    let mut core = GuestCore::at(5);
    assert!(core.handle(Wire::Resync { revision: 4, outcomes: Vec::new(), units, bytes: 0 }, 0).is_err());
    let mut core = GuestCore::at(5);
    assert!(matches!(core.handle(Wire::Resync { revision: 9, outcomes: Vec::new(), units: 0, bytes: 0 }, 0), Ok(None)));
    assert!(core.handle(Wire::MapEnd, 0).is_err(), "an empty map is refused");
    let mut core = GuestCore::at(5);
    let outcomes = vec![Outcome { sequence: 3, rejection: Some(hostile) }];
    assert!(matches!(core.handle(Wire::Resync { revision: 9, outcomes, units, bytes: 0 }, 0), Ok(None)));
    assert!(matches!(core.handle(Wire::MapPart(map.clone()), 0), Ok(None)));
    let Ok(Some(Event::Resync { revision: 9, outcomes, .. })) = core.handle(Wire::MapEnd, 0) else { panic!("resync") };
    assert_eq!(core.revision, 9);
    assert_eq!(outcomes[0].rejection.as_ref().unwrap().detail.chars().count(), protocol::MAX_REASON_CHARS);
    assert!(core.handle(Wire::Applied { author: 1, sequence: 1, revision: 10, changes: edit.clone() }, 0).is_ok());
    let host =
        Participant { id: 1, name: "Host".into(), role: Role::Host, presence: Presence::default(), latency_ms: 0 };
    let guest = Participant { id: 2, role: Role::Editor, name: "Guest".into(), ..host.clone() };
    for rows in [
        vec![guest.clone()],
        vec![host.clone(), guest.clone(), guest.clone()],
        vec![host.clone(), Participant { role: Role::Host, ..guest.clone() }],
        vec![host.clone(), Participant { latency_ms: u64::MAX, ..guest.clone() }],
        vec![host.clone(); 9],
        Vec::new(),
    ] {
        assert!(GuestCore::at(5).handle(Wire::Participants(rows), 0).is_err());
    }
    let Ok(Some(Event::Participants(rows))) = GuestCore::at(5).handle(Wire::Participants(vec![host, guest]), 42) else {
        panic!("participants")
    };
    assert_eq!(rows[0].latency_ms, 42);
    let closed = Refused { refusal: Refusal::Banned, detail: "z".repeat(5000), seconds: Some(u64::MAX) };
    let Err(error) = GuestCore::at(5).handle(Wire::Closed(closed), 0) else { panic!("closed") };
    let refused = error.downcast_ref::<Refused>().unwrap();
    assert_eq!(refused.detail.chars().count(), protocol::MAX_REASON_CHARS);
    assert!(refused.seconds.unwrap() < u64::MAX);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum LegacyHostMessage {
    Challenge { nonce: [u8; 32] },
    Closed(String),
}

#[tokio::test]
async fn legacy_clients_receive_a_multilingual_update_message() {
    let (host, invitation, port) = start_direct_host(fixture_map(), "legacy-password", AccessPolicy::default());
    let mut client = RawClient::connect(&invitation, port, LEGACY_ALPN).await;
    let hello = Frame::serialize(&serde_json::json!({ "Hello": { "version": 1 } }), 128).unwrap();
    write_frame(&mut client.send, &hello, DEADLINE).await.unwrap();
    let bytes = read_frame(&mut client.recv, 2048, DEADLINE, DEADLINE).await.unwrap();
    let LegacyHostMessage::Challenge { nonce } = serde_json::from_slice(&bytes).unwrap() else { panic!("challenge") };
    let authenticate = serde_json::json!({ "Authenticate": { "version": 1, "name": "Old", "session_name": "Test",
        "device": "x", "signature": [], "proof": nonce } });
    write_frame(&mut client.send, &Frame::serialize(&authenticate, 2048).unwrap(), DEADLINE).await.unwrap();
    let bytes = read_frame(&mut client.recv, 4096, DEADLINE, DEADLINE).await.unwrap();
    let LegacyHostMessage::Closed(reason) = serde_json::from_slice(&bytes).unwrap() else { panic!("closed") };
    assert_eq!(reason, LEGACY_REFUSAL);
    assert!(reason.contains("Hammer Pro 2.0") && reason.contains("mets à jour") && reason.contains("обновите"));
    client.finish().await;
    drop(host);
}

#[test]
fn guests_report_an_outdated_host_cleanly() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    let old_host = runtime.block_on(async {
        iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
            .relay_mode(iroh::RelayMode::Disabled)
            .clear_address_lookup()
            .alpns(vec![LEGACY_ALPN.to_vec()])
            .bind()
            .await
            .unwrap()
    });
    let port = old_host.addr().ip_addrs().find(|address| address.is_ipv4()).unwrap().port();
    let accepting = old_host.clone();
    runtime.spawn(async move {
        while let Some(incoming) = accepting.accept().await {
            if let Ok(connection) = incoming.await {
                tokio::spawn(async move {
                    let _ = connection.accept_bi().await;
                    connection.closed().await;
                });
            }
        }
    });
    let invitation = Invitation { endpoint: old_host.id().to_string(), ..test_invitation() };
    let guest = Session::join(
        invitation,
        "Guest".into(),
        Zeroizing::new("any-password".into()),
        JoinOptions {
            identity: Zeroizing::new([1; 32]),
            session_name: "Test".into(),
            direct_address: Some(([127, 0, 0, 1], port).into()),
        },
    )
    .unwrap();
    wait_for(
        &guest,
        "outdated host",
        |event| matches!(event, Event::Denied(refused) if refused.refusal == Refusal::HostOutdated),
    );
    runtime.block_on(old_host.close());
}

#[tokio::test]
async fn newer_hello_versions_and_garbage_get_a_clean_refusal_and_the_host_keeps_serving() {
    let (host, invitation, port) = start_direct_host(fixture_map(), "garbage-password", AccessPolicy::default());
    let mut future = RawClient::connect(&invitation, port, ALPN).await;
    future.write(&Wire::Hello { version: VERSION + 1 }).await;
    let Wire::Closed(refused) = future.read().await.unwrap() else { panic!("closed") };
    assert_eq!(refused.refusal, Refusal::HostOutdated);
    future.finish().await;
    let mut garbage = RawClient::connect(&invitation, port, ALPN).await;
    garbage.send.write_all(&[0, 0, 0, 20]).await.unwrap();
    garbage.send.write_all(b"not json at all!!!!!").await.unwrap();
    assert!(garbage.read().await.is_err());
    garbage.finish().await;
    let mut huge = RawClient::connect(&invitation, port, ALPN).await;
    huge.send.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
    assert!(huge.read().await.is_err());
    huge.finish().await;
    let mut client = RawClient::connect(&invitation, port, ALPN).await;
    client.authenticate(&invitation, "garbage-password", "Guest", [9; 32]).await;
    assert!(matches!(client.read().await.unwrap(), Wire::MapStart { .. }));
    client.send.write_all(&[0, 0, 0, 0]).await.unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(10), client.connection.closed()).await.unwrap();
    assert!(format!("{closed:?}").contains("Session ended") || format!("{closed:?}").contains("rror"));
    client.finish().await;
    drop(host);
}

#[tokio::test]
async fn silent_connections_are_dropped_by_the_handshake_deadline() {
    let (host, invitation, port) = start_direct_host(fixture_map(), "silent-password", AccessPolicy::default());
    let endpoint = direct_endpoint().await;
    let address = EndpointAddr::new(invitation.endpoint.parse().unwrap())
        .with_ip_addr(std::net::SocketAddr::from(([127, 0, 0, 1], port)));
    let connection = endpoint.connect(address, ALPN).await.unwrap();
    let (mut send, _recv) = connection.open_bi().await.unwrap();
    send.write_all(&[0, 0]).await.unwrap();
    let started = Instant::now();
    tokio::time::timeout(HANDSHAKE_DEADLINE + Duration::from_secs(10), connection.closed()).await.unwrap();
    assert!(started.elapsed() <= HANDSHAKE_DEADLINE + Duration::from_secs(5));
    endpoint.close().await;
    drop(host);
}

#[test]
fn every_refusal_round_trips_through_close_codes() {
    for refusal in Refusal::ALL {
        assert_eq!(Refusal::from_close_code(u64::from(refusal.close_code())), Some(refusal));
    }
    assert_eq!(Refusal::from_close_code(0), None);
    assert_eq!(Refusal::from_close_code(1), None);
    assert_eq!(Refusal::from_close_code(u64::MAX), None);
    let snapshot = fixture_map();
    let shared = serde_json::to_vec(&Wire::Applied {
        author: 3,
        sequence: 4,
        revision: 5,
        changes: snapshot.diff(&Snapshot::default()),
    })
    .unwrap();
    assert!(matches!(decode(&shared).unwrap(), Wire::Applied { author: 3, .. }));
}

#[test]
fn invalid_moderation_commands_never_end_the_host_session() {
    let mut host = Host::new(fixture_map(), AccessPolicy::default());
    let mut peer = host.join("p", "device", "Guest");
    welcome_of(&mut peer);
    host.command(Command::Rotate(Zeroizing::new("short".into())));
    assert!(host.drain().iter().any(|event| matches!(event, Event::Error(_))));
    host.command(Command::Policy(AccessPolicy { invitation_minutes: u32::MAX, ..AccessPolicy::default() }));
    assert_eq!(host.access().policy, AccessPolicy::default());
    for command in [
        Command::Kick(99),
        Command::Ban { participant: 99, seconds: Some(0), reason: "\n".into() },
        Command::Unban(99),
        Command::SetRole { participant: 1, role: Role::Viewer },
        Command::Admit(99),
        Command::Decline(99),
    ] {
        host.command(command);
    }
    assert_eq!(host.core.peer_count(), 1);
    assert_eq!(host.core.role_of(2), Some(Role::Editor));
}

#[test]
fn script_entities_are_shared_unless_the_host_refuses_them() {
    let map = fixture_map();
    let mut host = Host::new(map.clone(), AccessPolicy::default());
    let mut peer = host.join("peer", "device", "Guest");
    let (_, range, _) = welcome_of(&mut peer);
    let lua = |id: u32| {
        let vmf =
            format!("entity {{\"id\" \"{id}\" \"classname\" \"lua_run\" \"Code\" \"print(1)\" \"origin\" \"0 0 0\"}}");
        vec![Change {
            key: format!("entity:{id}"),
            before: None,
            after: Some(crate::document::Unit { owner: 0, hidden: false, vmf }),
        }]
    };
    host.send(&peer, Wire::Edit { sequence: 1, changes: lua(range + 1) });
    assert!(matches!(peer.frames().last(), Some((Wire::Acked { sequence: 1, .. }, _, _))));
    assert!(host.core.snapshot().0.contains_key(&format!("entity:{}", range + 1)));
    host.command(Command::Policy(AccessPolicy { refuse_scripts: true, ..AccessPolicy::default() }));
    host.send(&peer, Wire::Edit { sequence: 2, changes: lua(range + 2) });
    let expected =
        format!("entity:{}: class \"lua_run\" refused: script and command entities cannot be shared", range + 2);
    assert!(
        matches!(peer.frames().last(), Some((Wire::Rejected { rejection, .. }, _, _)) if rejection.code == EditRefusal::Document && rejection.detail == expected),
        "{expected}"
    );
    assert_eq!(host.core.peer_count(), 1, "a refused script never ends the session");
    host.command(Command::Policy(AccessPolicy::default()));
    host.drain();
    host.command(Command::Edit { sequence: 1, changes: lua(9) });
    assert!(
        host.drain().iter().any(|event| matches!(event, Event::Applied { author: 1, .. })),
        "the host shares an object of the original map that it kept local until now"
    );
    host.send(&peer, Wire::Edit { sequence: 3, changes: lua(10) });
    assert!(
        matches!(peer.frames().last(), Some((Wire::Rejected { rejection, .. }, _, _)) if rejection.code == EditRefusal::OutsideRange)
    );
}
