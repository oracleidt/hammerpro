use std::path::PathBuf;
use std::time::{Duration, Instant};

use hp_livelink::protocol::{COMMAND_FILE, ENTITIES_FILE, GAME_FILE, SESSION_FILE};
use serde_json::{Value, json};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hp_livelink_app_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("garrysmod")).expect("scratch");
    dir
}

fn write(path: &std::path::Path, v: &Value) {
    hp_livelink::exchange::write_atomic(path, &serde_json::to_vec(v).unwrap()).unwrap();
}

fn read(path: &std::path::Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn wait(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    panic!("timed out");
}

fn test_state() -> AppState {
    let state = hp_app::testing::test_state();
    hp_app::i18n::register(super::tr::TR);
    state
}

fn tick(state: &mut AppState) {
    super::tick(state, &TickInput { compiling: false });
}

#[test]
fn snapshots_mark_brush_entities_and_follow_brushes() {
    let state = test_state();
    let snap = snapshot(&state);
    assert!(snap.ents[&6].brush, "the trigger owns a solid");
    assert!(!snap.ents[&5].brush);
    assert_eq!(snap.ents[&5].name(), "main_light");
    assert_eq!(snap.ents[&6].io.len(), 1);
    assert_ne!(snap.brush_sig, 0);
}

#[test]
fn the_link_needs_a_saved_gmod_map() {
    let _lock = hp_app::testing::lang_lock();
    let mut state = test_state();
    assert!(start(&mut state).unwrap_err().contains("Save"));
    assert!(!stop(&mut state));
    assert!(teleport(&mut state, None, true).is_err());
    assert!(fire(&mut state, "door", "Open", "", 0.0).is_err());
    assert_eq!(status_json(&state)["state"], "off");
}

#[test]
fn the_app_link_drives_a_simulated_game() {
    let _lock = hp_app::testing::lang_lock();
    let root = scratch("drive");
    let data = hp_livelink::addon::data_dir(&root);
    let mut state = test_state();
    state.settings.game.gmod_root = Some(root.clone());
    state.doc.set_path(Some(root.join("gm_livetest.vmf")));
    let text = start(&mut state).expect("start");
    assert!(text.contains("gm_livetest"));
    assert!(hp_livelink::addon::is_installed(&root));
    assert_eq!(status_json(&state)["state"], "waiting");
    let token = state.runtime.livelink.session.as_ref().unwrap().token().to_owned();
    wait(|| data.join(SESSION_FILE).is_file());
    write(
        &data.join(GAME_FILE),
        &json!({ "token": token, "ack": 0, "load": "gm_livetest:7", "map": "gm_livetest", "beat": 1, "addon": "mounted" }),
    );
    wait(|| {
        tick(&mut state);
        state.runtime.livelink.is_connected()
    });
    write(
        &data.join(ENTITIES_FILE),
        &json!({ "token": token, "load": "gm_livetest:7", "rev": 1, "ents": [
            { "i": 1240, "c": "info_player_start", "n": "", "o": [0, -128, 0], "m": "", "h": 4 },
            { "i": 1241, "c": "light", "n": "main_light", "o": [0, 0, 96], "m": "", "h": 5 },
            { "i": 1242, "c": "trigger_multiple", "n": "", "o": [0, 0, 0], "m": "*1", "h": 6 }
        ]}),
    );
    wait(|| {
        tick(&mut state);
        state.runtime.livelink.session.as_ref().unwrap().matches.is_some()
    });
    state.doc.edit_entity(4, |e| e.set("origin", "32 -128 0")).expect("edit");
    state.doc.edit_entity(5, |e| e.set("_light", "255 0 0 300")).expect("edit");
    state.doc.select(&[hp_editor::ObjRef::Entity(4)], hp_editor::SelectOp::Replace);
    push_now(&mut state);
    tick(&mut state);
    let mut ops = Vec::new();
    wait(|| {
        if let Some(v) = read(&data.join(COMMAND_FILE)) {
            ops = v["ops"].as_array().cloned().unwrap_or_default();
        }
        ops.len() >= 3
    });
    let kinds: Vec<&str> = ops.iter().filter_map(|o| o["op"].as_str()).collect();
    assert!(kinds.contains(&"set") && kinds.contains(&"light") && kinds.contains(&"highlight"), "{kinds:?}");
    let set = ops.iter().find(|o| o["op"] == "set").unwrap();
    assert_eq!(set["target"], json!({ "i": 1240 }));
    assert_eq!(set["kv"]["origin"], "32 -128 0");
    let light = ops.iter().find(|o| o["op"] == "light").unwrap();
    assert_eq!(light["light"]["color"], json!([255.0, 0.0, 0.0]));
    let hl = ops.iter().find(|o| o["op"] == "highlight").unwrap();
    assert_eq!(hl["items"][0]["target"], json!({ "i": 1240 }));
    fire(&mut state, "main_light", "TurnOff", "", 0.5).expect("fire");
    fire(&mut state, "#5", "TurnOn", "", 0.0).expect("fire by id");
    assert!(fire(&mut state, "x", "Command", "quit", 0.0).is_err());
    assert!(fire(&mut state, "#999", "Open", "", 0.0).is_err());
    teleport(&mut state, Some(([1.0, 2.0, 3.0], [0.0, 90.0, 0.0])), true).expect("teleport");
    back(&mut state).expect("back");
    wait(|| {
        read(&data.join(COMMAND_FILE))
            .and_then(|v| {
                v["ops"]
                    .as_array()
                    .map(|a| a.iter().filter(|o| o["op"] == "fire").count() == 2 && a.iter().any(|o| o["op"] == "back"))
            })
            .unwrap_or(false)
    });
    let st = status_json(&state);
    assert_eq!(st["state"], "connected");
    assert_eq!(st["matched_entities"], 3);
    assert!(st["needs_compile"].as_array().unwrap().iter().any(|r| r == "lighting"));
    state.settings.livelink.camera_from_game = true;
    tick(&mut state);
    wait(|| read(&data.join(SESSION_FILE)).is_some_and(|v| v["follow"] == true));
    write(
        &data.join(GAME_FILE),
        &json!({ "token": token, "ack": 1, "load": "gm_livetest:7", "map": "gm_livetest", "beat": 2, "eye": [10, 20, 30], "ang": [15, 45, 0] }),
    );
    wait(|| {
        tick(&mut state);
        state.cameras.cam3d.pos.x == 10.0
    });
    assert_eq!(state.cameras.cam3d.pitch_deg, -15.0);
    assert_eq!(state.cameras.cam3d.yaw_deg, 45.0);
    assert!(stop(&mut state));
    assert!(!data.join(SESSION_FILE).exists());
    remove_addon(&mut state).expect("remove");
    assert!(!hp_livelink::addon::is_present(&root));
    let _ = std::fs::remove_dir_all(&root);
}
