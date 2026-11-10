use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use super::tests::{RawClient, chained, fixture_map, start_direct_host, wait_for};
use super::*;
use crate::document::Snapshot;
use crate::moderation::AccessPolicy;
use crate::protocol::{ALPN, EditRefusal, Invitation, MAX_PARTICIPANTS, Wire};

fn join_direct(invitation: &Invitation, port: u16, password: &str, name: &str, identity: u8) -> Session {
    Session::join(
        invitation.clone(),
        name.into(),
        Zeroizing::new(password.into()),
        JoinOptions {
            identity: Zeroizing::new([identity; 32]),
            session_name: invitation.room.name.clone(),
            direct_address: Some(([127, 0, 0, 1], port).into()),
        },
    )
    .unwrap()
}

fn ready(session: &Session, description: &str) -> u32 {
    let mut participant = 0;
    wait_for(session, description, |event| {
        assert!(!matches!(event, Event::Denied(_) | Event::Error(_) | Event::Closed), "{description}: refused");
        if let Event::Ready { participant: id, .. } = event {
            participant = *id;
        }
        participant != 0
    });
    participant
}

fn big_map(fields: usize) -> Snapshot {
    let mut text = String::from("world {\"id\" \"1\"} entity {\"id\" \"2\" \"classname\" \"info_target\"");
    for index in 0..fields {
        text.push_str(&format!("\"field{index}\" \"{}\"", "x".repeat(16_000)));
    }
    text.push_str("} entity {\"id\" \"3\" \"classname\" \"info_target\" \"origin\" \"8 0 0\"}");
    Snapshot::from_vmf(&text).unwrap()
}

#[test]
fn every_participant_slot_can_be_used_from_one_address_in_direct_mode() {
    let password = "one-address-password";
    let (host, invitation, port) = start_direct_host(fixture_map(), password, AccessPolicy::default());
    let mut guests = Vec::new();
    for index in 0..5 {
        let guest = join_direct(&invitation, port, password, &format!("Seq{index}"), 10 + index);
        ready(&guest, &format!("sequential guest {index}"));
        guests.push(guest);
    }
    let late: Vec<Session> = (0..MAX_PARTICIPANTS - 6)
        .map(|index| join_direct(&invitation, port, password, &format!("Par{index}"), 40 + index as u8))
        .collect();
    for (index, guest) in late.iter().enumerate() {
        ready(guest, &format!("simultaneous guest {index}"));
    }
    guests.extend(late);
    assert_eq!(guests.len(), MAX_PARTICIPANTS - 1);
    wait_for(
        &host,
        "full session",
        |event| matches!(event, Event::Participants(rows) if rows.len() == MAX_PARTICIPANTS),
    );
    let extra = join_direct(&invitation, port, password, "Extra", 99);
    wait_for(
        &extra,
        "full",
        |event| matches!(event, Event::Denied(refused) if refused.refusal == crate::protocol::Refusal::Full),
    );
    drop(guests);
    host.send(Command::Stop).unwrap();
}

#[test]
fn a_guest_that_finds_every_handshake_slot_of_its_address_taken_gets_a_translated_busy_refusal() {
    let password = "busy-password";
    let (host, invitation, port) = start_direct_host(fixture_map(), password, AccessPolicy::default());
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    let silent: Vec<RawClient> = runtime.block_on(async {
        let mut clients = Vec::new();
        for _ in 0..super::handshake::MAX_HANDSHAKES_PER_ADDRESS {
            clients.push(RawClient::connect(&invitation, port, ALPN).await);
        }
        clients
    });
    let started = Instant::now();
    let refused = join_direct(&invitation, port, password, "Late", 5);
    wait_for(&refused, "busy refusal", |event| {
        assert!(!matches!(event, Event::Ready { .. }), "joined while every slot of the address was taken");
        matches!(event, Event::Denied(refused) if refused.refusal == crate::protocol::Refusal::Busy)
    });
    assert!(started.elapsed() >= Duration::from_secs(4), "the guest retried before giving up");
    runtime.block_on(async {
        for client in silent {
            client.finish().await;
        }
    });
    let guest = join_direct(&invitation, port, password, "Late", 5);
    ready(&guest, "join once the slots are free");
    assert!(super::guest::refused_by_host(&anyhow::anyhow!(
        "Could not connect: aborted by peer: the server refused to accept a new connection"
    )));
    assert!(!super::guest::refused_by_host(&anyhow::anyhow!("Connecting to the session timed out")));
    drop(guest);
    host.send(Command::Stop).unwrap();
}

#[test]
fn a_guest_edit_above_the_session_limits_is_rejected_with_a_reason_and_the_guest_stays() {
    let password = "large-edit-password";
    let map = fixture_map();
    let (host, invitation, port) = start_direct_host(map.clone(), password, AccessPolicy::default());
    let guest = join_direct(&invitation, port, password, "Paster", 7);
    let mut id_start = 0;
    wait_for(&guest, "ready", |event| {
        if let Event::Ready { id_start: start, .. } = event {
            id_start = *start;
        }
        id_start != 0
    });
    let changes: Vec<crate::Change> = (0..8000u32)
        .map(|index| {
            let id = id_start + index + 10;
            crate::Change {
                key: format!("entity:{id}"),
                before: None,
                after: Some(crate::document::Unit {
                    owner: 0,
                    hidden: false,
                    vmf: format!(
                        "entity {{\"id\" \"{id}\" \"classname\" \"info_target\" \"origin\" \"{index} 0 0\" \"comment\" \"{}\"}}",
                        "p".repeat(1200)
                    ),
                }),
            }
        })
        .collect();
    let bytes = serde_json::to_vec(&changes).unwrap().len();
    assert!(bytes > 10 * 1024 * 1024, "{bytes}");
    guest.send(Command::Edit { sequence: 1, changes }).unwrap();
    let mut outcome = None;
    wait_for(&guest, "large edit outcome", |event| {
        assert!(!matches!(event, Event::Error(_) | Event::Closed | Event::Denied(_)), "the guest was disconnected");
        match event {
            Event::Rejected { sequence: 1, rejection, .. } => outcome = Some(Some(rejection.clone())),
            Event::Applied { sequence: 1, .. } => outcome = Some(None),
            _ => {}
        }
        outcome.is_some()
    });
    if bytes > crate::document::MAX_CHANGE_BYTES {
        assert!(matches!(outcome, Some(Some(_))), "the document size limit answers with a rejection");
    }
    if let Some(Some(rejection)) = &outcome {
        assert_eq!(rejection.code, EditRefusal::Document, "{rejection}");
        assert!(!rejection.detail.is_empty());
    }
    let mut state = map.clone();
    guest.send(Command::Edit { sequence: 2, changes: chained(&mut state, "entity:2", "4 5 6") }).unwrap();
    wait_for(&guest, "next edit", |event| matches!(event, Event::Applied { sequence: 2, .. }));
    wait_for(&host, "host applied", |event| matches!(event, Event::Applied { sequence: 2, .. }));
    drop(guest);
    host.send(Command::Stop).unwrap();
}

#[test]
fn a_guest_still_downloading_the_map_receives_every_edit_made_meanwhile() {
    let map = big_map(600);
    let password = "download-password";
    let (host, invitation, port) = start_direct_host(map.clone(), password, AccessPolicy::default());
    let editor = join_direct(&invitation, port, password, "Editor", 3);
    ready(&editor, "editor");
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    let mut client = runtime.block_on(async {
        let mut client = RawClient::connect(&invitation, port, ALPN).await;
        client.authenticate(&invitation, password, "Joiner", [61; 32]).await;
        client
    });
    let mut state = map.clone();
    for sequence in 1..=25 {
        editor
            .send(Command::Edit { sequence, changes: chained(&mut state, "entity:3", &format!("{sequence} 1 0")) })
            .unwrap();
        wait_for(&editor, "ack", |event| matches!(event, Event::Applied { sequence: done, .. } if *done == sequence));
    }
    let started = Instant::now();
    let (welcome_revision, revisions, received) = runtime.block_on(async {
        let mut revisions = Vec::new();
        let mut welcomed: Option<(u64, Snapshot)> = None;
        let mut receiving = None;
        while revisions.last() != Some(&25) {
            match tokio::time::timeout(Duration::from_secs(30), client.read()).await.unwrap().unwrap() {
                Wire::MapStart { revision, .. } => receiving = Some((revision, Snapshot::default())),
                Wire::MapPart(part) => receiving.as_mut().expect("map start first").1.0.extend(part.0),
                Wire::MapEnd => welcomed = receiving.take(),
                Wire::Applied { revision, mut changes, .. } => {
                    let (_, map) = welcomed.as_mut().expect("welcome first");
                    crate::document::expand_changes(&mut changes, map).unwrap();
                    map.apply(&changes).unwrap();
                    revisions.push(revision);
                }
                Wire::Pong(_) | Wire::Participants(_) => {}
                other => panic!("unexpected {}", serde_json::to_string(&other).unwrap()),
            }
        }
        let (revision, map) = welcomed.unwrap();
        (revision, revisions, map)
    });
    assert!(started.elapsed() < Duration::from_secs(60));
    assert!(welcome_revision < 25);
    assert_eq!(revisions, ((welcome_revision + 1)..=25).collect::<Vec<_>>());
    assert_eq!(received, state, "the late guest converges on the shared map");
    runtime.block_on(client.finish());
    drop(editor);
    host.send(Command::Stop).unwrap();
}
