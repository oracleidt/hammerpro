use std::time::{Duration, Instant};

use hp_collab::transport::{HostOptions, JoinOptions};
use hp_collab::{Change, Command, ConnectionMode, Event, Invitation, Room, Session, Snapshot};
use zeroize::Zeroizing;

fn map_from_env() -> Vec<(String, Snapshot)> {
    let mut maps = Vec::new();
    let mut text = String::from("world {\"id\" \"1\" \"classname\" \"worldspawn\"");
    for index in 0..12_000u32 {
        let id = index + 100;
        let side = 100_000 + index * 16;
        let x = (index % 110) * 96;
        let y = (index / 110) * 96;
        text.push_str(&format!("solid {{\"id\" \"{id}\""));
        let planes = [
            format!("({x} {} 64) ({} {} 64) ({} {y} 64)", y + 64, x + 64, y + 64, x + 64),
            format!("({x} {y} 0) ({} {y} 0) ({} {} 0)", x + 64, x + 64, y + 64),
            format!("({} {} 64) ({x} {} 64) ({x} {} 0)", x + 64, y + 64, y + 64, y + 64),
            format!("({} {y} 0) ({x} {y} 0) ({x} {y} 64)", x + 64),
            format!("({} {} 0) ({} {y} 0) ({} {y} 64)", x + 64, y + 64, x + 64, x + 64),
            format!("({x} {} 64) ({x} {y} 64) ({x} {y} 0)", y + 64),
        ];
        for (offset, plane) in planes.iter().enumerate() {
            text.push_str(&format!(
                "side {{\"id\" \"{}\" \"plane\" \"{plane}\" \"material\" \"dev/reflectivity_50\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" \"rotation\" \"0\" \"lightmapscale\" \"16\" \"smoothing_groups\" \"0\"}}",
                side + offset as u32
            ));
        }
        text.push('}');
    }
    text.push_str("} entity {\"id\" \"20\" \"classname\" \"prop_dynamic\" \"origin\" \"0 0 0\" \"model\" \"models/props_c17/oildrum001.mdl\"}");
    maps.push(("12000 brushes".to_owned(), Snapshot::from_vmf(&text).unwrap()));
    for path in std::env::var("HP_TEAM_MAPS").unwrap_or_default().split(';').filter(|path| !path.is_empty()) {
        let map = hp_vmf::load(std::path::Path::new(path)).unwrap();
        let text = hp_vmf::write(&map);
        maps.push((path.to_owned(), Snapshot::from_vmf(&text).unwrap()));
    }
    maps
}

fn wait_ready(session: &Session) -> (Option<String>, Vec<std::net::SocketAddr>, Snapshot) {
    let started = Instant::now();
    loop {
        while let Ok(event) = session.events.try_recv() {
            match event {
                Event::Ready { invitation, direct_addresses, map, .. } => {
                    return (invitation, direct_addresses, map.into_snapshot());
                }
                Event::Error(error) => panic!("{error}"),
                _ => {}
            }
        }
        assert!(started.elapsed() < Duration::from_secs(300), "session not ready");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn edit_of(map: &Snapshot, key: &str) -> Vec<Change> {
    let unit = &map.0[key];
    let after = if key.starts_with("solid:") {
        let mut solid = unit.to_solid().unwrap();
        for side in &mut solid.sides {
            for point in &mut side.plane {
                point.x += 16.0;
            }
        }
        hp_collab::Unit::from_solid(&solid, unit.owner).unwrap()
    } else {
        let mut entity = unit.to_entity().unwrap();
        let origin = entity.origin().unwrap_or_default() + glam::DVec3::new(16.0, 0.0, 0.0);
        entity.set("origin", format!("{} {} {}", origin.x, origin.y, origin.z));
        hp_collab::Unit::from_entity(&entity, false).unwrap()
    };
    vec![Change { key: key.into(), before: Some(hp_collab::document::digest(unit)), after: Some(after) }]
}

#[test]
#[ignore = "profiles host processing and join on large maps with several participants"]
fn host_processing_and_join_profile() {
    for (label, map) in map_from_env() {
        let started = Instant::now();
        let host = Session::host(
            map.clone(),
            "Host".into(),
            Zeroizing::new("profile-password".into()),
            HostOptions {
                room: Room { name: "Profile".into() },
                mode: ConnectionMode::Direct,
                port: 0,
                policy: Default::default(),
            },
        )
        .unwrap();
        let (invitation, addresses, _) = wait_ready(&host);
        let host_ready = started.elapsed();
        let invitation = Invitation::decode(&invitation.unwrap()).unwrap();
        let port = addresses.iter().find(|address| address.is_ipv4()).unwrap().port();
        let mut guests = Vec::new();
        let mut joins = Vec::new();
        for index in 0..4 {
            let started = Instant::now();
            let guest = Session::join(
                invitation.clone(),
                format!("Guest{index}"),
                Zeroizing::new("profile-password".into()),
                JoinOptions {
                    identity: Zeroizing::new(hp_collab::protocol::random_secret().unwrap()),
                    session_name: "Profile".into(),
                    direct_address: Some(([127, 0, 0, 1], port).into()),
                },
            )
            .unwrap();
            let (_, _, received) = wait_ready(&guest);
            assert_eq!(received.0.len(), map.0.len());
            joins.push(started.elapsed().as_secs_f64() * 1000.0);
            guests.push(guest);
        }
        let key = map.0.keys().find(|key| key.starts_with("solid:")).unwrap().clone();
        let prop = map
            .0
            .iter()
            .find(|(key, unit)| key.starts_with("entity:") && unit.vmf.contains("\"origin\""))
            .map(|(key, _)| key.clone());
        let mut current = map.clone();
        let mut timings = Vec::new();
        for step in 0..10u64 {
            let target = if step % 2 == 0 { prop.clone().unwrap_or(key.clone()) } else { key.clone() };
            let changes = edit_of(&current, &target);
            current.apply(&changes).unwrap();
            let started = Instant::now();
            host.send(Command::Edit { sequence: step + 1, changes }).unwrap();
            let mut applied = false;
            while !applied {
                while let Ok(event) = host.events.try_recv() {
                    match event {
                        Event::Applied { sequence, .. } if sequence == step + 1 => applied = true,
                        Event::Rejected { rejection, .. } => panic!("{rejection}"),
                        _ => {}
                    }
                }
                assert!(started.elapsed() < Duration::from_secs(120));
                std::thread::sleep(Duration::from_micros(200));
            }
            let host_done = started.elapsed();
            let mut remaining: Vec<usize> = (0..guests.len()).collect();
            while !remaining.is_empty() {
                remaining.retain(|index| {
                    let mut done = false;
                    while let Ok(event) = guests[*index].events.try_recv() {
                        if let Event::Applied { sequence, author: 1, .. } = event
                            && sequence == step + 1
                        {
                            done = true;
                        }
                    }
                    !done
                });
                assert!(started.elapsed() < Duration::from_secs(120));
                std::thread::sleep(Duration::from_micros(200));
            }
            timings.push((host_done.as_secs_f64() * 1000.0, started.elapsed().as_secs_f64() * 1000.0));
            if step == 0 {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let host_ms: Vec<f64> = timings.iter().map(|(host, _)| (host * 100.0).round() / 100.0).collect();
        let all_ms: Vec<f64> = timings.iter().map(|(_, all)| (all * 100.0).round() / 100.0).collect();
        eprintln!(
            "{label}: {} units, host ready {:.0} ms, 4 joins {:?} ms, host apply per edit {:?} ms, edit on 4 guests {:?} ms",
            map.0.len(),
            host_ready.as_secs_f64() * 1000.0,
            joins.iter().map(|value| value.round()).collect::<Vec<_>>(),
            host_ms,
            all_ms
        );
        drop(guests);
        drop(host);
    }
}
