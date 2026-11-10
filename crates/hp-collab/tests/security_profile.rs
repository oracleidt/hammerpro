use std::time::{Duration, Instant};

use hp_collab::document::ValidatedSnapshot;
use hp_collab::moderation::AccessPolicy;
use hp_collab::transport::{HostOptions, JoinOptions};
use hp_collab::{Change, Command, ConnectionMode, Event, Invitation, Room, Session, Snapshot};
use zeroize::Zeroizing;

const PASSWORD: &str = "latency-profile-password";

fn wait<T>(session: &Session, limit: Duration, what: &str, mut found: impl FnMut(Event) -> Option<T>) -> T {
    let started = Instant::now();
    loop {
        while let Ok(event) = session.events.try_recv() {
            if let Event::Error(error) = &event {
                panic!("{what}: {error}");
            }
            if let Some(value) = found(event) {
                return value;
            }
        }
        assert!(started.elapsed() < limit, "timeout: {what}");
        std::thread::sleep(Duration::from_micros(200));
    }
}

fn join(invitation: &Invitation, port: u16, name: &str, identity: u8) -> Session {
    Session::join(
        invitation.clone(),
        name.into(),
        Zeroizing::new(PASSWORD.into()),
        JoinOptions {
            identity: Zeroizing::new([identity; 32]),
            session_name: "Latency".into(),
            direct_address: Some(([127, 0, 0, 1], port).into()),
        },
    )
    .unwrap()
}

fn ready(session: &Session) -> (u32, u32, Snapshot) {
    wait(session, Duration::from_secs(600), "join", |event| match event {
        Event::Ready { participant, id_start, map, .. } => Some((participant, id_start, map.into_snapshot())),
        _ => None,
    })
}

fn median(values: &mut [f64]) -> (f64, f64) {
    values.sort_by(f64::total_cmp);
    (values[values.len() / 2], values[(values.len() * 9 / 10).min(values.len() - 1)])
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

fn point_key(map: &Snapshot) -> String {
    let owners: std::collections::BTreeSet<u32> = map.0.values().map(|unit| unit.owner).collect();
    map.0
        .iter()
        .find(|(key, unit)| {
            key.strip_prefix("entity:").and_then(|id| id.parse::<u32>().ok()).is_some_and(|id| !owners.contains(&id))
                && unit.vmf.contains("\"origin\" \"")
                && hp_kv::parse(&unit.vmf).is_ok_and(|root| {
                    root.children
                        .iter()
                        .all(|block| block.children.iter().all(|child| !child.name.eq_ignore_ascii_case("group")))
                })
        })
        .map(|(key, _)| key.clone())
        .expect("a point entity with an origin")
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

#[test]
#[ignore = "profile: HP_BENCH_MAP=<file.vmf> cargo test -p hp-collab --test security_profile -- --ignored --nocapture"]
fn team_mapping_latency_profile() {
    let path = std::env::var("HP_BENCH_MAP").expect("HP_BENCH_MAP");
    let mut root = hp_kv::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    clean(&mut root);
    let text = hp_kv::write(&root);
    drop(root);
    let started = Instant::now();
    let map = Snapshot::from_vmf(&text).expect("shareable map");
    drop(text);
    let snapshot_ms = started.elapsed().as_secs_f64() * 1000.0;
    let world = map.0.get("world").unwrap().clone();
    let world_id: u32 =
        world.vmf.split("\"id\" \"").nth(1).and_then(|rest| rest.split('"').next()).unwrap().parse().unwrap();
    let started = Instant::now();
    let host = Session::host(
        map,
        "Host".into(),
        Zeroizing::new(PASSWORD.into()),
        HostOptions {
            room: Room { name: "Latency".into() },
            mode: ConnectionMode::Direct,
            port: 0,
            policy: AccessPolicy::default(),
        },
    )
    .unwrap();
    let (invitation, port) = wait(&host, Duration::from_secs(600), "host", |event| match event {
        Event::Ready { invitation: Some(code), direct_addresses, .. } => Some((
            Invitation::decode(&code).unwrap(),
            direct_addresses.iter().find(|address| address.is_ipv4()).unwrap().port(),
        )),
        _ => None,
    });
    let host_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let guest = join(&invitation, port, "Guest", 1);
    let (participant, id_start, map) = ready(&guest);
    let join_ms = started.elapsed().as_secs_f64() * 1000.0;
    let repeat: usize = std::env::var("HP_BENCH_JOINS").ok().and_then(|value| value.parse().ok()).unwrap_or(0);
    let mut joins = Vec::new();
    for index in 0..repeat {
        let started = Instant::now();
        let extra = join(&invitation, port, &format!("Again{index}"), 100 + index as u8);
        ready(&extra);
        joins.push(started.elapsed().as_secs_f64() * 1000.0);
        drop(extra);
        std::thread::sleep(Duration::from_millis(500));
    }
    if !joins.is_empty() {
        let (middle, _) = median(&mut joins);
        println!(
            "JOINS map={} count={} median_ms={middle:.0} all={joins:.0?}",
            std::path::Path::new(&path).file_name().unwrap().to_string_lossy(),
            joins.len()
        );
        drop(guest);
        host.send(Command::Stop).unwrap();
        return;
    }
    let started = Instant::now();
    let others: Vec<Session> = (2..5).map(|index| join(&invitation, port, &format!("Guest{index}"), index)).collect();
    for other in &others {
        ready(other);
    }
    let join3_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut canonical = ValidatedSnapshot::new(map).unwrap();
    let key = point_key(&canonical);
    let mut point = Vec::new();
    for step in 1..=30u64 {
        let changes = moved(&canonical, &key, step as usize);
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        guest.send(Command::Edit { sequence: step, changes }).unwrap();
        let applied = wait(&guest, Duration::from_secs(120), "point edit", |event| match event {
            Event::Applied { author, sequence, changes, .. } if author == participant && sequence == step => {
                Some(changes)
            }
            Event::Rejected { rejection, .. } => panic!("point edit rejected: {rejection}"),
            _ => None,
        });
        point.push(started.elapsed().as_secs_f64() * 1000.0);
        canonical.apply(&applied).unwrap();
    }
    let mut brush = Vec::new();
    for index in 0..5u64 {
        let sequence = 100 + index;
        let change = new_box(world_id, id_start + 10 * index as u32);
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        guest.send(Command::Edit { sequence, changes: vec![change] }).unwrap();
        let applied = wait(&guest, Duration::from_secs(300), "brush edit", |event| match event {
            Event::Applied { author, sequence: done, changes, .. } if author == participant && done == sequence => {
                Some(changes)
            }
            Event::Rejected { rejection, .. } => panic!("brush edit rejected: {rejection}"),
            _ => None,
        });
        brush.push(started.elapsed().as_secs_f64() * 1000.0);
        canonical.apply(&applied).unwrap();
    }
    let mut stale = moved(&canonical, &key, 999);
    stale[0].before = Some([0; 32]);
    let mut rejected = Vec::new();
    let mut traffic = 0;
    for index in 0..5u64 {
        let sequence = 200 + index;
        let before_traffic = wait(&guest, Duration::from_secs(10), "traffic", |event| match event {
            Event::Traffic { received, .. } => Some(received),
            _ => None,
        });
        let started = Instant::now();
        guest.send(Command::Edit { sequence, changes: stale.clone() }).unwrap();
        wait(&guest, Duration::from_secs(300), "rejection", |event| match event {
            Event::Rejected { sequence: done, .. } if done == sequence => Some(()),
            _ => None,
        });
        rejected.push(started.elapsed().as_secs_f64() * 1000.0);
        let after_traffic = wait(&guest, Duration::from_secs(10), "traffic", |event| match event {
            Event::Traffic { received, .. } => Some(received),
            _ => None,
        });
        traffic = traffic.max(after_traffic - before_traffic);
    }
    let (point_median, point_p90) = median(&mut point);
    let (brush_median, brush_p90) = median(&mut brush);
    let (rejected_median, _) = median(&mut rejected);
    println!(
        "LATENCY map={} units={} world_bytes={} snapshot_ms={snapshot_ms:.0} host_ready_ms={host_ms:.0} join_ms={join_ms:.0} join3_concurrent_ms={join3_ms:.0} point_edit_median_ms={point_median:.2} point_edit_p90_ms={point_p90:.2} brush_edit_median_ms={brush_median:.1} brush_edit_p90_ms={brush_p90:.1} rejection_median_ms={rejected_median:.1} rejection_received_bytes={traffic}",
        std::path::Path::new(&path).file_name().unwrap().to_string_lossy(),
        canonical.0.len(),
        world.vmf.len(),
    );
    drop(others);
    drop(guest);
    host.send(Command::Stop).unwrap();
}
