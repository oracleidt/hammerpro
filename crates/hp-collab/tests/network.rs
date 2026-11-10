use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use hp_collab::moderation::AccessPolicy;
use hp_collab::transport::{HostOptions, JoinOptions};
use hp_collab::{Change, Command, ConnectionMode, Event, Invitation, Presence, Room, Session, Snapshot};
use zeroize::Zeroizing;

#[derive(Default)]
struct Observed {
    ready: bool,
    closed: bool,
    map: Option<Snapshot>,
    invitation: Option<String>,
    participant: u32,
    id_start: u32,
    participants: usize,
    rejected: u64,
    bans: usize,
    errors: Vec<String>,
    sent: u64,
    received: u64,
    direct_addresses: Vec<std::net::SocketAddr>,
    privacy_samples: usize,
    pending: BTreeMap<u64, Vec<Change>>,
    progress: u64,
}

fn edit(
    sessions: &[Session],
    observed: &mut BTreeMap<usize, Observed>,
    index: usize,
    sequence: u64,
    changes: Vec<Change>,
) {
    observed.entry(index).or_default().pending.insert(sequence, changes.clone());
    sessions[index].send(Command::Edit { sequence, changes }).unwrap();
}

fn pump(sessions: &[Session], observed: &mut BTreeMap<usize, Observed>) {
    for (index, session) in sessions.iter().enumerate() {
        let state = observed.entry(index).or_default();
        while let Ok(event) = session.events.try_recv() {
            match event {
                Event::Reset { map, .. } | Event::Resync { map, .. } => state.map = Some(map.into_snapshot()),
                Event::Ready { invitation, participant, id_start, map, direct_addresses, .. } => {
                    state.ready = true;
                    state.participant = participant;
                    state.id_start = id_start;
                    state.invitation = invitation;
                    state.map = Some(map.into_snapshot());
                    state.direct_addresses = direct_addresses;
                }
                Event::Applied { author, sequence, mut changes, .. } => {
                    let own = if author == state.participant { state.pending.remove(&sequence) } else { None };
                    if changes.is_empty() {
                        changes = own.expect("acknowledged changes");
                    }
                    let map = state.map.as_mut().unwrap();
                    hp_collab::document::expand_changes(&mut changes, map).unwrap();
                    map.apply(&changes).unwrap();
                }
                Event::Rejected { sequence, .. } => {
                    state.rejected += 1;
                    state.pending.remove(&sequence);
                }
                Event::JoinProgress { units, .. } => state.progress = units,
                Event::Participants(rows) => state.participants = rows.len(),
                Event::Bans(rows) => state.bans = rows.len(),
                Event::Invitation(invitation) => state.invitation = Some(invitation),
                Event::Error(error) => state.errors.push(error),
                Event::Denied(refused) => state.errors.push(refused.to_string()),
                Event::Access(_) | Event::Waiting => {}
                Event::Closed => state.closed = true,
                Event::Traffic { sent, received } => {
                    state.sent = sent;
                    state.received = received;
                }
                Event::NetworkPrivacy { relay_only, published_ip_count, direct_path_count } => {
                    if relay_only {
                        assert_eq!(published_ip_count, 0);
                        assert_eq!(direct_path_count, 0);
                    }
                    state.privacy_samples += 1;
                }
            }
        }
    }
}

fn until(
    sessions: &[Session],
    observed: &mut BTreeMap<usize, Observed>,
    description: &str,
    predicate: impl Fn(&BTreeMap<usize, Observed>) -> bool,
) {
    let start = Instant::now();
    loop {
        pump(sessions, observed);
        if predicate(observed) {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(45),
            "Timeout: {description}; errors: {:?}",
            observed.iter().map(|(id, row)| (*id, &row.errors)).collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn guest(invitation: &str, nickname: &str, password: &str, key: [u8; 32]) -> Session {
    Session::join(
        Invitation::decode(invitation).unwrap(),
        nickname.into(),
        Zeroizing::new(password.into()),
        JoinOptions { identity: Zeroizing::new(key), session_name: "Five Mapper Test".into(), direct_address: None },
    )
    .unwrap()
}

fn add_box(base: &Snapshot, id: u32, displacement: bool) -> Snapshot {
    let points = [
        [[0, 64, 64], [64, 64, 64], [64, 0, 64]],
        [[0, 0, 0], [64, 0, 0], [64, 64, 0]],
        [[64, 64, 64], [0, 64, 64], [0, 64, 0]],
        [[64, 0, 0], [0, 0, 0], [0, 0, 64]],
        [[64, 64, 0], [64, 0, 0], [64, 0, 64]],
        [[0, 64, 64], [0, 0, 64], [0, 0, 0]],
    ];
    let mut vmf = format!("world {{\"id\" \"1\" solid {{\"id\" \"{id}\"");
    for (index, plane) in points.iter().enumerate() {
        let coords =
            plane.iter().map(|point| format!("({} {} {})", point[0], point[1], point[2])).collect::<Vec<_>>().join(" ");
        vmf.push_str(&format!("side {{\"id\" \"{}\" \"plane\" \"{coords}\" \"material\" \"dev/dev_measuregeneric01b\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\"}}", id + index as u32 + 1));
    }
    vmf.push_str("}}");
    let mut solid = hp_vmf::parse(&vmf).unwrap().world.solids.remove(0);
    if displacement {
        let mut normal = solid.sides[0].plane[0];
        normal.x = 0.0;
        normal.y = 0.0;
        normal.z = 1.0;
        solid.sides[0].disp = Some(Box::new(hp_vmf::DispInfo::new_flat(2, solid.sides[0].plane[0], normal)));
        solid.sides[0].disp.as_mut().unwrap().distances[12] = 16.0;
    }
    let mut map = hp_vmf::parse(&base.to_vmf().unwrap()).unwrap();
    map.world.solids.push(solid);
    let generated = Snapshot::from_vmf(&hp_vmf::write(&map)).unwrap();
    let key = format!("solid:{id}");
    let mut result = base.clone();
    result.0.insert(key.clone(), generated.0[&key].clone());
    result
}

#[test]
#[ignore = "requires access to public iroh discovery and relay services"]
fn five_participants_over_public_relay_edits_locks_moderation_rotation_and_revocation() {
    let map = Snapshot::from_vmf("world {\"id\" \"1\" \"classname\" \"worldspawn\"} entity {\"id\" \"2\" \"classname\" \"light\" \"origin\" \"0 0 0\"} entity {\"id\" \"3\" \"classname\" \"light\"}").unwrap();
    let password = "private-test-password";
    let host = Session::host(
        map.clone(),
        "Host".into(),
        Zeroizing::new(password.into()),
        HostOptions {
            room: Room { name: "Five Mapper Test".into() },
            mode: ConnectionMode::Relay,
            port: 0,
            policy: AccessPolicy::default(),
        },
    )
    .unwrap();
    let mut sessions = vec![host];
    let mut observed = BTreeMap::<usize, Observed>::new();
    until(&sessions, &mut observed, "host online", |rows| rows.get(&0).is_some_and(|row| row.ready));
    let invitation = observed[&0].invitation.clone().unwrap();
    let mut identities = Vec::new();
    for index in 1..=4 {
        let identity = hp_collab::protocol::random_secret().unwrap();
        identities.push(identity);
        sessions.push(guest(&invitation, &format!("Mapper{index}"), password, identity));
        until(&sessions, &mut observed, "guest admitted", |rows| rows.get(&index).is_some_and(|row| row.ready));
    }
    until(&sessions, &mut observed, "five visible participants", |rows| {
        rows.get(&0).is_some_and(|row| row.participants == 5)
    });
    sessions[1]
        .send(Command::Presence(Presence {
            position: [64.0, 32.0, 16.0],
            direction: [1.0, 0.0, 0.0],
            selected: vec!["entity:2".into()],
            ..Presence::default()
        }))
        .unwrap();
    let wait = Instant::now();
    while wait.elapsed() < Duration::from_millis(500) {
        pump(&sessions, &mut observed);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut changed = map.clone();
    changed.0.get_mut("entity:2").unwrap().vmf = changed.0["entity:2"].vmf.replace("0 0 0", "64 0 0");
    edit(&sessions, &mut observed, 2, 1, map.diff(&changed));
    until(&sessions, &mut observed, "selection lock enforced", |rows| {
        rows.get(&2).is_some_and(|row| row.rejected == 1)
    });
    let start = Instant::now();
    edit(&sessions, &mut observed, 1, 1, map.diff(&changed));
    until(&sessions, &mut observed, "all maps converge", |rows| {
        (0..5).all(|index| rows.get(&index).and_then(|row| row.map.as_ref()) == Some(&changed))
    });
    eprintln!("Relay-only edit propagated to five participants in {} ms", start.elapsed().as_millis());
    let brush = add_box(&changed, observed[&1].id_start, false);
    let displacement = add_box(&changed, observed[&2].id_start, true);
    let changes_a = changed.diff(&brush);
    let changes_b = changed.diff(&displacement);
    edit(&sessions, &mut observed, 1, 2, changes_a.clone());
    edit(&sessions, &mut observed, 2, 2, changes_b.clone());
    changed.apply(&changes_a).unwrap();
    changed.apply(&changes_b).unwrap();
    until(&sessions, &mut observed, "simultaneous brush and displacement creations converge", |rows| {
        (0..5).all(|index| rows.get(&index).and_then(|row| row.map.as_ref()) == Some(&changed))
    });
    let parsed = hp_vmf::parse(&changed.to_vmf().unwrap()).unwrap();
    assert_eq!(parsed.world.solids.len(), 2);
    assert!(
        parsed.world.solids.iter().any(|solid| solid
            .sides
            .iter()
            .any(|side| side.disp.as_ref().is_some_and(|disp| disp.distances[12] == 16.0)))
    );
    let snapshot_root = std::env::temp_dir().join(format!(
        "hp-network-snapshots-{}-{}",
        std::process::id(),
        hp_collab::protocol::random_secret().unwrap()[0]
    ));
    let store = hp_collab::snapshots::SnapshotStore::new(snapshot_root.clone());
    sessions[0].send(Command::Restore { map: map.clone(), store: store.clone() }).unwrap();
    let before_restore = changed;
    changed = map.clone();
    until(&sessions, &mut observed, "host snapshot restore reaches every participant", |rows| {
        (0..5).all(|index| rows.get(&index).and_then(|row| row.map.as_ref()) == Some(&changed))
    });
    let saved = store.list().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(store.load(&saved[0].filename).unwrap(), before_restore);
    let banned = observed[&4].participant;
    sessions[0]
        .send(Command::Ban { participant: banned, seconds: Some(60), reason: "test moderation".into() })
        .unwrap();
    until(&sessions, &mut observed, "guest banned", |rows| {
        rows.get(&4).is_some_and(|row| row.closed) && rows[&0].bans == 1
    });
    sessions.push(guest(&invitation, "Mapper4", password, identities[3]));
    until(&sessions, &mut observed, "ban blocks reconnect", |rows| rows.get(&5).is_some_and(|row| row.closed));
    assert!(!observed[&5].ready);
    assert!(observed[&5].errors.iter().any(|error| error.contains("Banned") && error.contains("test moderation")));
    sessions[0].send(Command::Unban(1)).unwrap();
    until(&sessions, &mut observed, "unban", |rows| rows[&0].bans == 0);
    sessions.push(guest(&invitation, "Mapper4", password, identities[3]));
    until(&sessions, &mut observed, "unbanned guest admitted", |rows| rows.get(&6).is_some_and(|row| row.ready));
    sessions[0].send(Command::Rotate(Zeroizing::new("new-private-test-password".into()))).unwrap();
    until(&sessions, &mut observed, "credentials rotated", |rows| rows[&0].invitation.as_ref() != Some(&invitation));
    until(&sessions, &mut observed, "rotation disconnects existing guests", |rows| {
        [1, 2, 3, 6].iter().all(|index| rows.get(index).is_some_and(|row| row.closed))
    });
    sessions.push(guest(&invitation, "OldInvite", password, hp_collab::protocol::random_secret().unwrap()));
    until(&sessions, &mut observed, "old invitation rejected", |rows| rows.get(&7).is_some_and(|row| row.closed));
    assert!(!observed[&7].ready);
    let current = observed[&0].invitation.clone().unwrap();
    sessions.push(guest(
        &current,
        "NewInvite",
        "new-private-test-password",
        hp_collab::protocol::random_secret().unwrap(),
    ));
    until(&sessions, &mut observed, "new invitation works", |rows| rows.get(&8).is_some_and(|row| row.ready));
    assert_eq!(observed[&8].map.as_ref(), Some(&changed));
    assert!(observed[&0].direct_addresses.is_empty());
    assert!(observed[&0].privacy_samples > 0 && observed[&1].privacy_samples > 0);
    eprintln!("Host traffic: {} bytes sent, {} bytes received", observed[&0].sent, observed[&0].received);
    sessions[0].send(Command::Stop).unwrap();
    until(&sessions, &mut observed, "host shutdown disconnects guests", |rows| {
        rows.get(&8).is_some_and(|row| row.closed)
    });
    for row in store.list().unwrap() {
        std::fs::remove_file(snapshot_root.join(row.filename)).unwrap();
    }
    std::fs::remove_file(snapshot_root.join("snapshots.lock")).unwrap();
    std::fs::remove_dir(snapshot_root).unwrap();
}

#[test]
fn direct_connection_uses_explicit_address_and_same_authenticated_protocol() {
    let map = Snapshot::from_vmf("world {\"id\" \"1\" \"classname\" \"worldspawn\"}").unwrap();
    let host = Session::host(
        map.clone(),
        "Host".into(),
        Zeroizing::new("local-test-password".into()),
        HostOptions {
            room: Room { name: "Local".into() },
            mode: ConnectionMode::Direct,
            port: 0,
            policy: AccessPolicy::default(),
        },
    )
    .unwrap();
    let mut sessions = vec![host];
    let mut observed = BTreeMap::<usize, Observed>::new();
    until(&sessions, &mut observed, "direct host ready", |rows| rows.get(&0).is_some_and(|row| row.ready));
    let invitation = Invitation::decode(observed[&0].invitation.as_ref().unwrap()).unwrap();
    assert_eq!(invitation.mode, ConnectionMode::Direct);
    assert!(invitation.relay.is_none());
    let port = observed[&0].direct_addresses.iter().find(|address| address.is_ipv4()).unwrap().port();
    sessions.push(
        Session::join(
            invitation,
            "Guest".into(),
            Zeroizing::new("local-test-password".into()),
            JoinOptions {
                identity: Zeroizing::new(hp_collab::protocol::random_secret().unwrap()),
                session_name: "Local".into(),
                direct_address: Some(([127, 0, 0, 1], port).into()),
            },
        )
        .unwrap(),
    );
    until(&sessions, &mut observed, "direct guest ready", |rows| rows.get(&1).is_some_and(|row| row.ready));
    assert_eq!(observed[&1].map.as_ref(), Some(&map));
    sessions[0].send(Command::Stop).unwrap();
    until(&sessions, &mut observed, "direct shutdown", |rows| rows.get(&1).is_some_and(|row| row.closed));
}

#[test]
fn a_participant_rejoining_from_the_same_installation_replaces_its_stale_connection_and_keeps_its_range() {
    let map = Snapshot::from_vmf("world {\"id\" \"1\" \"classname\" \"worldspawn\"}").unwrap();
    let host = Session::host(
        map,
        "Host".into(),
        Zeroizing::new("rejoin-test-password".into()),
        HostOptions {
            room: Room { name: "Rejoin".into() },
            mode: ConnectionMode::Direct,
            port: 0,
            policy: AccessPolicy::default(),
        },
    )
    .unwrap();
    let mut sessions = vec![host];
    let mut observed = BTreeMap::<usize, Observed>::new();
    until(&sessions, &mut observed, "host ready", |rows| rows.get(&0).is_some_and(|row| row.ready));
    let invitation = observed[&0].invitation.clone().unwrap();
    let port = observed[&0].direct_addresses.iter().find(|address| address.is_ipv4()).unwrap().port();
    let join = |key: [u8; 32]| {
        Session::join(
            Invitation::decode(&invitation).unwrap(),
            "Guest".into(),
            Zeroizing::new("rejoin-test-password".into()),
            JoinOptions {
                identity: Zeroizing::new(key),
                session_name: "Rejoin".into(),
                direct_address: Some(([127, 0, 0, 1], port).into()),
            },
        )
        .unwrap()
    };
    sessions.push(join([31; 32]));
    until(&sessions, &mut observed, "first connection ready", |rows| rows.get(&1).is_some_and(|row| row.ready));
    let first_range = observed[&1].id_start;
    sessions.push(join([31; 32]));
    until(&sessions, &mut observed, "second connection from the same installation", |rows| {
        rows.get(&2).is_some_and(|row| row.ready) && rows.get(&1).is_some_and(|row| row.closed)
    });
    assert_eq!(observed[&2].id_start, first_range, "the rejoining installation keeps its identifier range");
    assert_ne!(observed[&2].participant, observed[&1].participant);
    sessions.push(join([32; 32]));
    until(&sessions, &mut observed, "another installation with the same nickname", |rows| {
        rows.get(&3).is_some_and(|row| row.closed)
    });
    assert!(!observed[&3].ready, "a different installation cannot take a nickname in use");
    assert!(observed[&3].errors.iter().any(|error| error.contains("Nickname already in use")));
    until(&sessions, &mut observed, "participants after rejoin", |rows| {
        rows.get(&0).is_some_and(|row| row.participants == 2)
    });
}

#[test]
#[ignore = "allocates and transfers an 84 MiB map"]
fn large_map_can_be_hosted_transferred_and_restored_above_the_previous_limits() {
    let payload = "x".repeat(16_384);
    let mut text = String::from(
        "world {\"id\" \"1\" \"classname\" \"worldspawn\"} entity {\"id\" \"2\" \"classname\" \"info_target\"",
    );
    for index in 0..5376 {
        text.push_str(&format!("\"field{index}\" \"{payload}\"\n"));
    }
    text.push('}');
    assert!(text.len() > 80 * 1024 * 1024);
    let map = Snapshot::from_vmf(&text).unwrap();
    drop(text);
    assert_eq!(hp_collab::document::MAX_MAP_BYTES, 500 * 1024 * 1024);
    let host = Session::host(
        map.clone(),
        "Host".into(),
        Zeroizing::new("large-map-password".into()),
        HostOptions {
            room: Room { name: "Large map".into() },
            mode: ConnectionMode::Direct,
            port: 0,
            policy: AccessPolicy::default(),
        },
    )
    .unwrap();
    let mut sessions = vec![host];
    let mut observed = BTreeMap::<usize, Observed>::new();
    until(&sessions, &mut observed, "large host ready", |rows| rows.get(&0).is_some_and(|row| row.ready));
    let invitation = Invitation::decode(observed[&0].invitation.as_ref().unwrap()).unwrap();
    let port = observed[&0].direct_addresses.iter().find(|address| address.is_ipv4()).unwrap().port();
    sessions.push(
        Session::join(
            invitation,
            "Guest".into(),
            Zeroizing::new("large-map-password".into()),
            JoinOptions {
                identity: Zeroizing::new(hp_collab::protocol::random_secret().unwrap()),
                session_name: "Large map".into(),
                direct_address: Some(([127, 0, 0, 1], port).into()),
            },
        )
        .unwrap(),
    );
    until(&sessions, &mut observed, "large map transferred", |rows| rows.get(&1).is_some_and(|row| row.ready));
    assert_eq!(observed[&1].map.as_ref(), Some(&map));
    assert_eq!(observed[&1].progress, map.0.len() as u64);
    let root = std::env::temp_dir().join(format!("hp-collab-large-snapshot-{}", std::process::id()));
    let store = hp_collab::snapshots::SnapshotStore::new(root.clone());
    let saved = store.save(&map, "Large map", 0).unwrap();
    assert_eq!(store.load(&saved.filename).unwrap(), map);
    std::fs::remove_file(root.join(saved.filename)).unwrap();
    std::fs::remove_file(root.join("snapshots.lock")).unwrap();
    std::fs::remove_dir(root).unwrap();
    drop(sessions);
    drop(observed);
    drop(map);
    let too_large = " ".repeat(hp_collab::document::MAX_MAP_BYTES + 1);
    assert!(Snapshot::from_vmf(&too_large).unwrap_err().to_string().contains("size limit"));
}
