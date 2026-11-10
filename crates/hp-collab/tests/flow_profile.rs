use std::time::{Duration, Instant};

use hp_collab::moderation::AccessPolicy;
use hp_collab::transport::{HostOptions, JoinOptions};
use hp_collab::{Change, Command, ConnectionMode, Event, Invitation, Presence, Room, Session, Snapshot};
use zeroize::Zeroizing;

const PASSWORD: &str = "flow-profile-password";
const BURST: u64 = 12;

fn join(invitation: &Invitation, port: u16, name: &str, identity: u8) -> Session {
    Session::join(
        invitation.clone(),
        name.into(),
        Zeroizing::new(PASSWORD.into()),
        JoinOptions {
            identity: Zeroizing::new([identity; 32]),
            session_name: "Flow".into(),
            direct_address: Some(([127, 0, 0, 1], port).into()),
        },
    )
    .unwrap()
}

fn ready(session: &Session) -> (u32, u32, Snapshot) {
    let started = Instant::now();
    loop {
        while let Ok(event) = session.events.try_recv() {
            match event {
                Event::Ready { participant, id_start, map, .. } => return (participant, id_start, map.into_snapshot()),
                Event::Error(error) => panic!("join: {error}"),
                _ => {}
            }
        }
        assert!(started.elapsed() < Duration::from_secs(600), "join timeout");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn safe(value: &str) -> bool {
    !value.contains([':', char::from(0), char::from(92)])
        && !value.starts_with('/')
        && !value.split('/').any(|part| part == "..")
}

fn clean(block: &mut hp_kv::Block) {
    const SCRIPTED: [&str; 4] = ["lua_run", "logic_script", "point_servercommand", "point_clientcommand"];
    block.children.retain(|child| {
        !child.get("classname").is_some_and(|class| SCRIPTED.iter().any(|name| class.eq_ignore_ascii_case(name)))
    });
    let connections = block.name.eq_ignore_ascii_case("connections");
    block.props.retain_mut(|(key, value)| {
        let lower = key.to_ascii_lowercase();
        if connections {
            let input = value.split([',', char::from(27)]).nth(1).unwrap_or("").to_ascii_lowercase();
            return !matches!(input.as_str(), "command" | "runscriptcode" | "runscriptfile" | "runpassedcode");
        }
        if matches!(
            lower.as_str(),
            "file"
                | "model"
                | "material"
                | "texture"
                | "detailmaterial"
                | "skyname"
                | "vscripts"
                | "vscript_init_code"
                | "message"
        ) {
            *value = value.replace(char::from(92), "/");
            return safe(value);
        }
        true
    });
    for child in &mut block.children {
        clean(child);
    }
}

fn point_keys(map: &Snapshot) -> Vec<String> {
    let owners: std::collections::BTreeSet<u32> = map.0.values().map(|unit| unit.owner).collect();
    map.0
        .iter()
        .filter(|(key, unit)| {
            key.strip_prefix("entity:").and_then(|id| id.parse::<u32>().ok()).is_some_and(|id| !owners.contains(&id))
                && unit.vmf.contains("\"origin\" \"")
                && hp_kv::parse(&unit.vmf).is_ok_and(|root| {
                    root.children
                        .iter()
                        .all(|block| block.children.iter().all(|child| !child.name.eq_ignore_ascii_case("group")))
                })
        })
        .map(|(key, _)| key.clone())
        .take(2)
        .collect()
}

fn moved(map: &Snapshot, key: &str, step: usize) -> Vec<Change> {
    let mut changed = map.clone();
    let unit = changed.0.get_mut(key).unwrap();
    let start = unit.vmf.find("\"origin\" \"").unwrap() + 10;
    let end = start + unit.vmf[start..].find('"').unwrap();
    unit.vmf.replace_range(start..end, &format!("{} 0 64", step * 8));
    vec![Change {
        key: key.into(),
        before: map.0.get(key).map(hp_collab::document::digest),
        after: changed.0.remove(key),
    }]
}

fn new_box(owner: u32, id: u32) -> Change {
    let points = [
        [[0, 64, 64], [64, 64, 64], [64, 0, 64]],
        [[0, 0, 0], [64, 0, 0], [64, 64, 0]],
        [[64, 64, 64], [0, 64, 64], [0, 64, 0]],
        [[64, 0, 0], [0, 0, 0], [0, 0, 64]],
        [[64, 64, 0], [64, 0, 0], [64, 0, 64]],
        [[0, 64, 64], [0, 0, 64], [0, 0, 0]],
    ];
    let mut vmf = format!("solid\n{{\n\"id\" \"{id}\"\n");
    for (index, plane) in points.iter().enumerate() {
        let coords =
            plane.iter().map(|point| format!("({} {} {})", point[0], point[1], point[2])).collect::<Vec<_>>().join(" ");
        vmf.push_str(&format!("side\n{{\n\"id\" \"{}\"\n\"plane\" \"{coords}\"\n\"material\" \"dev/dev_measuregeneric01b\"\n\"uaxis\" \"[1 0 0 0] 0.25\"\n\"vaxis\" \"[0 -1 0 0] 0.25\"\n}}\n", id + index as u32 + 1));
    }
    vmf.push_str("}\n");
    Change { key: format!("solid:{id}"), before: None, after: Some(hp_collab::Unit { owner, hidden: false, vmf }) }
}

#[derive(Default)]
struct Outcomes {
    applied: u64,
    rejected: u64,
    last: Option<Duration>,
    errors: Vec<String>,
}

fn collect(session: &Session, author: u32, wanted: &[u64], started: Instant, outcomes: &mut Outcomes) {
    while let Ok(event) = session.events.try_recv() {
        match event {
            Event::Applied { author: who, sequence, .. } if who == author && wanted.contains(&sequence) => {
                outcomes.applied += 1;
                outcomes.last = Some(started.elapsed());
            }
            Event::Rejected { sequence, .. } if wanted.contains(&sequence) => {
                outcomes.rejected += 1;
                outcomes.last = Some(started.elapsed());
            }
            Event::Error(error) => outcomes.errors.push(error),
            Event::Closed => outcomes.errors.push("closed".into()),
            _ => {}
        }
    }
}

#[test]
#[ignore = "profile: HP_BENCH_MAP=<file.vmf> cargo test -p hp-collab --test flow_profile -- --ignored --nocapture"]
fn team_mapping_flow_profile() {
    let path = std::env::var("HP_BENCH_MAP").expect("HP_BENCH_MAP");
    let mut root = hp_kv::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    clean(&mut root);
    let text = hp_kv::write(&root);
    drop(root);
    let map = Snapshot::from_vmf(&text).expect("shareable map");
    drop(text);
    let world_vmf = map.0.get("world").unwrap().vmf.clone();
    let world_id: u32 =
        world_vmf.split("\"id\" \"").nth(1).and_then(|rest| rest.split('"').next()).unwrap().parse().unwrap();
    let host = Session::host(
        map,
        "Host".into(),
        Zeroizing::new(PASSWORD.into()),
        HostOptions {
            room: Room { name: "Flow".into() },
            mode: ConnectionMode::Direct,
            port: 0,
            policy: AccessPolicy::default(),
        },
    )
    .unwrap();
    let (invitation, port) = loop {
        if let Ok(Event::Ready { invitation: Some(code), direct_addresses, .. }) = host.events.try_recv() {
            break (
                Invitation::decode(&code).unwrap(),
                direct_addresses.iter().find(|address| address.is_ipv4()).unwrap().port(),
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let burst = join(&invitation, port, "Burst", 1);
    let (burst_id, burst_range, map) = ready(&burst);
    let quiet = join(&invitation, port, "Quiet", 2);
    let (quiet_id, _, _) = ready(&quiet);
    while host.events.try_recv().is_ok() {}
    let keys = point_keys(&map);
    let guest_edit = moved(&map, &keys[0], 1);
    let host_edit = moved(&map, &keys[1], 2);
    let wanted: Vec<u64> = (1..=BURST).collect();
    let started = Instant::now();
    for sequence in 1..=BURST {
        burst
            .send(Command::Edit { sequence, changes: vec![new_box(world_id, burst_range + 10 * sequence as u32)] })
            .unwrap();
    }
    let (mut burst_outcomes, mut quiet_outcomes, mut host_outcomes) =
        (Outcomes::default(), Outcomes::default(), Outcomes::default());
    let (mut presence_sent, mut presence_failed) = (0u32, 0u32);
    let mut last_presence = Instant::now();
    let (mut quiet_sent, mut host_sent) = (None, None);
    while (burst_outcomes.applied + burst_outcomes.rejected < BURST
        || quiet_outcomes.last.is_none()
        || host_outcomes.last.is_none())
        && burst_outcomes.errors.is_empty()
        && quiet_outcomes.errors.is_empty()
    {
        assert!(started.elapsed() < Duration::from_secs(900), "flow profile timeout");
        if quiet_sent.is_none() && started.elapsed() >= Duration::from_millis(500) {
            quiet.send(Command::Edit { sequence: 1, changes: guest_edit.clone() }).unwrap();
            quiet_sent = Some(Instant::now());
        }
        if host_sent.is_none() && started.elapsed() >= Duration::from_millis(500) {
            host_sent = Some(Instant::now());
            if host.send(Command::Edit { sequence: 1, changes: host_edit.clone() }).is_err() {
                host_outcomes.errors.push("host edit refused by the session queue".into());
                host_outcomes.last = Some(Duration::ZERO);
            }
        }
        if last_presence.elapsed() >= Duration::from_millis(100) {
            last_presence = Instant::now();
            presence_sent += 1;
            let presence = Presence { position: [f64::from(presence_sent), 0.0, 0.0], ..Presence::default() };
            if host.send(Command::Presence(presence)).is_err() {
                presence_failed += 1;
            }
        }
        collect(&burst, burst_id, &wanted, started, &mut burst_outcomes);
        if let Some(sent) = quiet_sent {
            collect(&quiet, quiet_id, &[1], sent, &mut quiet_outcomes);
        }
        if let Some(sent) = host_sent {
            collect(&host, 1, &[1], sent, &mut host_outcomes);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let ms = |outcomes: &Outcomes| outcomes.last.map_or(f64::NAN, |last| last.as_secs_f64() * 1000.0);
    println!(
        "FLOW map={} burst={BURST} burst_applied={} burst_rejected={} burst_ms={:.0} quiet_point_edit_ms={:.0} quiet_rejected={} host_edit_ms={:.0} presence_failed={presence_failed}/{presence_sent} errors={:?}",
        std::path::Path::new(&path).file_name().unwrap().to_string_lossy(),
        burst_outcomes.applied,
        burst_outcomes.rejected,
        ms(&burst_outcomes),
        ms(&quiet_outcomes),
        quiet_outcomes.rejected,
        ms(&host_outcomes),
        [burst_outcomes.errors, quiet_outcomes.errors, host_outcomes.errors].concat(),
    );
    drop(burst);
    drop(quiet);
    host.send(Command::Stop).ok();
}

#[test]
#[ignore = "profile: HP_BENCH_MAP=<file.vmf> HP_BENCH_GUESTS=<n> cargo test -p hp-collab --test flow_profile -- --ignored --nocapture"]
fn team_mapping_fan_out_profile() {
    let path = std::env::var("HP_BENCH_MAP").expect("HP_BENCH_MAP");
    let guests: usize = std::env::var("HP_BENCH_GUESTS").ok().and_then(|value| value.parse().ok()).unwrap_or(7);
    let mut root = hp_kv::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    clean(&mut root);
    let text = hp_kv::write(&root);
    drop(root);
    let map = Snapshot::from_vmf(&text).expect("shareable map");
    drop(text);
    let host = Session::host(
        map,
        "Host".into(),
        Zeroizing::new(PASSWORD.into()),
        HostOptions {
            room: Room { name: "Flow".into() },
            mode: ConnectionMode::Direct,
            port: 0,
            policy: AccessPolicy::default(),
        },
    )
    .unwrap();
    let (invitation, port) = loop {
        if let Ok(Event::Ready { invitation: Some(code), direct_addresses, .. }) = host.events.try_recv() {
            break (
                Invitation::decode(&code).unwrap(),
                direct_addresses.iter().find(|address| address.is_ipv4()).unwrap().port(),
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let started = Instant::now();
    let sessions: Vec<Session> =
        (0..guests).map(|index| join(&invitation, port, &format!("Guest{index}"), index as u8 + 1)).collect();
    let mut joined: Vec<(u32, Snapshot)> = Vec::new();
    for session in &sessions {
        let (participant, _, map) = ready(session);
        joined.push((participant, map));
    }
    let join_all_ms = started.elapsed().as_secs_f64() * 1000.0;
    let author = joined[0].0;
    let mut canonical = joined[0].1.clone();
    let key = point_keys(&canonical)[0].clone();
    let (mut round_trips, mut fan_outs) = (Vec::new(), Vec::new());
    for step in 1..=20u64 {
        for session in &sessions {
            while session.events.try_recv().is_ok() {}
        }
        let changes = moved(&canonical, &key, step as usize);
        canonical.0.insert(key.clone(), changes[0].after.clone().unwrap());
        let sent = Instant::now();
        sessions[0].send(Command::Edit { sequence: step, changes }).unwrap();
        let mut pending: Vec<bool> = vec![true; sessions.len()];
        let mut round_trip = None;
        while pending.iter().any(|waiting| *waiting) {
            assert!(sent.elapsed() < Duration::from_secs(120), "fan out timeout");
            for (index, session) in sessions.iter().enumerate() {
                while let Ok(event) = session.events.try_recv() {
                    if let Event::Applied { author: who, sequence, .. } = event
                        && who == author
                        && sequence == step
                    {
                        pending[index] = false;
                        if index == 0 {
                            round_trip = Some(sent.elapsed().as_secs_f64() * 1000.0);
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_micros(200));
        }
        fan_outs.push(sent.elapsed().as_secs_f64() * 1000.0);
        round_trips.push(round_trip.unwrap());
        std::thread::sleep(Duration::from_millis(50));
    }
    round_trips.sort_by(f64::total_cmp);
    fan_outs.sort_by(f64::total_cmp);
    println!(
        "FLOW map={} guests={guests} join_all_ms={join_all_ms:.0} author_round_trip_median_ms={:.2} all_guests_median_ms={:.2} all_guests_p90_ms={:.2}",
        std::path::Path::new(&path).file_name().unwrap().to_string_lossy(),
        round_trips[round_trips.len() / 2],
        fan_outs[fan_outs.len() / 2],
        fan_outs[fan_outs.len() * 9 / 10],
    );
    drop(sessions);
    host.send(Command::Stop).ok();
}
