use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::exchange::write_atomic;
use crate::protocol::{COMMAND_FILE, ENTITIES_FILE, GAME_FILE, SESSION_FILE};
use crate::{EntSnap, Event, LinkState, Session, Snapshot, addon};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hp_livelink_e2e_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("garrysmod")).expect("scratch");
    dir
}

fn snap(class: &str, kv: &[(&str, &str)]) -> EntSnap {
    EntSnap {
        class: class.into(),
        kv: kv.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
        io: Vec::new(),
        brush: false,
    }
}

fn read_json(path: &std::path::Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn pump(session: &mut Session, current: &Snapshot, until: impl Fn(&Session, &[Event]) -> bool) -> Vec<Event> {
    let end = Instant::now() + Duration::from_secs(5);
    let mut all = Vec::new();
    while Instant::now() < end {
        let events = session.handle(current);
        all.extend(events);
        if until(session, &all) {
            return all;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out, events so far: {all:?}");
}

#[test]
fn a_session_matches_entities_and_sends_edits_to_the_game() {
    let root = scratch("session");
    let data = addon::data_dir(&root);
    let mut base = Snapshot::default();
    base.ents.insert(10, snap("prop_physics", &[("origin", "0 0 0"), ("model", "models/a.mdl")]));
    base.ents.insert(11, snap("func_door", &[("targetname", "door")]));
    base.ents.get_mut(&11).unwrap().brush = true;
    let mut session =
        Session::start(root.clone(), "gm_test".into(), base.clone(), BTreeMap::new(), false, Box::new(|| {}))
            .expect("start");
    assert!(addon::is_installed(&root));
    assert_eq!(session.state(), LinkState::WaitingForGame);
    let end = Instant::now() + Duration::from_secs(5);
    let token = loop {
        if let Some(v) = read_json(&data.join(SESSION_FILE)) {
            break v["token"].as_str().expect("token").to_owned();
        }
        assert!(Instant::now() < end, "no session file");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(token, session.token());
    let ents = json!({ "token": token, "load": "gm_test:1", "rev": 1, "ents": [
        { "i": 1300, "c": "prop_physics", "n": "", "o": [0, 0, 0], "m": "models/a.mdl", "h": 10 },
        { "i": 1301, "c": "func_door", "n": "door", "o": [0, 0, 0], "m": "*1", "h": 11 }
    ]});
    write_atomic(&data.join(ENTITIES_FILE), &serde_json::to_vec(&ents).unwrap()).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    session.handle(&base);
    assert!(session.matches.is_none(), "an entity list read before the first report waits for it");
    let report =
        json!({ "token": token, "ack": 0, "load": "gm_test:1", "map": "gm_test", "beat": 1, "addon": "mounted" });
    write_atomic(&data.join(GAME_FILE), &serde_json::to_vec(&report).unwrap()).unwrap();
    let events = pump(&mut session, &base, |s, _| s.is_connected() && s.matches.is_some());
    assert!(events.contains(&Event::Connected { map: "gm_test".into() }));
    assert_eq!(session.matches.as_ref().unwrap().by_id.len(), 2);
    let mut moved = base.clone();
    moved.ents.get_mut(&10).unwrap().kv.insert("origin".into(), "64 0 0".into());
    moved.ents.get_mut(&11).unwrap().kv.insert("rendercolor".into(), "255 0 0".into());
    moved.brush_sig = 99;
    session.update(&moved);
    assert!(session.status.needs_compile.contains(&crate::Reason::Brushes));
    assert_eq!(session.status.changed, 2);
    let end = Instant::now() + Duration::from_secs(5);
    let cmd = loop {
        if let Some(v) = read_json(&data.join(COMMAND_FILE))
            && v["ops"].as_array().is_some_and(|a| a.len() == 2)
        {
            break v;
        }
        assert!(Instant::now() < end, "no commands");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(cmd["load"], "gm_test:1");
    assert_eq!(
        cmd["ops"][0],
        json!({ "seq": 1, "op": "set", "key": 10, "target": { "i": 1300 }, "kv": { "origin": "64 0 0" } })
    );
    assert_eq!(cmd["ops"][1]["target"], json!({ "i": 1301 }));
    let ack = json!({ "token": token, "ack": 2, "load": "gm_test:1", "map": "gm_test", "beat": 2,
        "results": [{ "seq": 2, "ok": false, "msg": "entity not found in game" }],
        "eye": [1, 2, 3], "ang": [0, 90, 0] });
    write_atomic(&data.join(GAME_FILE), &serde_json::to_vec(&ack).unwrap()).unwrap();
    let events = pump(&mut session, &moved, |s, _| s.status.pending == 0 && !s.status.errors.is_empty());
    assert!(events.contains(&Event::Eye { origin: [1.0, 2.0, 3.0], angles: [0.0, 90.0, 0.0] }));
    assert!(session.status.latency.is_some());
    assert_eq!(session.status.errors.len(), 1);
    let reload = json!({ "token": token, "ack": 2, "load": "gm_test:2", "map": "gm_test", "beat": 3 });
    write_atomic(&data.join(GAME_FILE), &serde_json::to_vec(&reload).unwrap()).unwrap();
    let events = pump(&mut session, &moved, |_, e| e.iter().any(|e| matches!(e, Event::NewLoad { .. })));
    assert!(events.contains(&Event::NewLoad { map: "gm_test".into(), rebased: false }));
    assert!(session.matches.is_none(), "a new load waits for its entity list");
    let other = json!({ "token": token, "ack": 2, "load": "gm_test:2", "map": "gm_other", "beat": 4 });
    write_atomic(&data.join(GAME_FILE), &serde_json::to_vec(&other).unwrap()).unwrap();
    pump(&mut session, &moved, |s, _| s.state() == LinkState::OtherMap);
    session.stop();
    assert!(!data.join(SESSION_FILE).exists());
    addon::remove(&root).expect("remove");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_new_build_rebases_on_the_compiled_snapshot() {
    let root = scratch("rebase");
    let data = addon::data_dir(&root);
    let base = Snapshot::default();
    let mut session =
        Session::start(root.clone(), "gm_b".into(), base.clone(), BTreeMap::new(), false, Box::new(|| {}))
            .expect("start");
    let mut compiled = Snapshot::default();
    compiled.ents.insert(1, snap("info_target", &[("origin", "0 0 0")]));
    session.note_compile_started(compiled.clone());
    std::thread::sleep(Duration::from_millis(20));
    let maps = root.join("garrysmod").join("maps");
    std::fs::create_dir_all(&maps).unwrap();
    std::fs::write(maps.join("gm_b.bsp"), b"VBSP").unwrap();
    let token = session.token().to_owned();
    for (load, beat) in [("gm_b:1", 1), ("gm_b:2", 2)] {
        let report = json!({ "token": token, "ack": 0, "load": load, "map": "gm_b", "beat": beat });
        write_atomic(&data.join(GAME_FILE), &serde_json::to_vec(&report).unwrap()).unwrap();
        pump(&mut session, &compiled, |s, _| s.status.load == load);
    }
    assert_eq!(session.baseline, compiled);
    assert!(session.compile_snapshot.is_none());
    session.stop();
    addon::remove(&root).expect("remove");
    let _ = std::fs::remove_dir_all(&root);
}
