use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_app::{AppState, Severity, t, tf};
use hp_editor::ObjRef;
use hp_livelink::protocol::{HighlightItem, Op, Target};
use hp_livelink::{Event, LinkState, Session, Snapshot, ToWorker, addon, brush_signature};
use serde_json::{Value, json};

pub mod tr;
pub mod ui;

pub const SNAPSHOT_EVERY: Duration = Duration::from_millis(80);
pub const CAMERA_EVERY: Duration = Duration::from_millis(50);
pub const INJECT_AFTER: Duration = Duration::from_secs(3);
pub const INJECT_EVERY: Duration = Duration::from_secs(20);
pub const MAX_HIGHLIGHT: usize = 256;
const ROOT_ENV: &str = "HAMMERPRO_LIVELINK_ROOT";

pub fn game_root(state: &AppState) -> Option<PathBuf> {
    if hp_compile::sandbox::is_sandboxed() {
        return std::env::var_os(ROOT_ENV).filter(|v| !v.is_empty()).map(PathBuf::from);
    }
    if let Some(root) = state.assets.fs().and_then(hp_fs::GameFs::root) {
        return Some(root.to_path_buf());
    }
    let configured = state.settings.game.gmod_root.as_deref().filter(|p| !p.as_os_str().is_empty())?;
    let is_mod_folder = configured.file_name().is_some_and(|n| n == "garrysmod");
    Some(if is_mod_folder { configured.parent().unwrap_or(configured) } else { configured }.to_path_buf())
}

pub fn map_name(state: &AppState) -> Option<String> {
    state.doc.path().and_then(Path::file_stem).map(|s| s.to_string_lossy().into_owned())
}

pub fn snapshot(state: &AppState) -> Snapshot {
    let scene = state.doc.scene();
    let mut owners = BTreeSet::new();
    let mut solids = Vec::with_capacity(scene.solids().len());
    for node in scene.solids().values() {
        let owner = match node.owner {
            hp_editor::Owner::Entity(id) => {
                owners.insert(id);
                id
            }
            hp_editor::Owner::World => 0,
        };
        solids.push((&node.solid, owner));
    }
    let sig = brush_signature(solids.into_iter());
    Snapshot::from_entities(scene.entities().values().map(|n| (&n.entity, owners.contains(&n.entity.id))), sig)
}

pub fn labels() -> BTreeMap<String, String> {
    [
        ("preview", t!("livelink.game.preview")),
        ("removed", t!("livelink.game.removed")),
        ("ghost", t!("livelink.game.ghost")),
        ("hud", t!("livelink.game.hud")),
        ("hud_previews", t!("livelink.game.hud_previews")),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

pub fn start(state: &mut AppState) -> Result<String, String> {
    if state.runtime.livelink.session.is_some() {
        return Ok(t!("livelink.already_on").to_owned());
    }
    if !state.settings.game.is_gmod() {
        return Err(t!("livelink.only_gmod").to_owned());
    }
    let Some(map) = map_name(state) else { return Err(t!("livelink.not_saved").to_owned()) };
    if hp_compile::sandbox::is_sandboxed() && std::env::var_os(ROOT_ENV).is_none() {
        return Err(hp_compile::sandbox::REFUSED.to_owned());
    }
    let Some(root) = game_root(state) else { return Err(t!("livelink.no_game").to_owned()) };
    let snap = snapshot(state);
    let wake = state.runtime.livelink.wake.clone();
    let wake: Box<dyn Fn() + Send> = Box::new(move || {
        if let Some(ctx) = &wake {
            ctx.request_repaint();
        }
    });
    let follow = state.settings.livelink.camera_from_game;
    let session = Session::start(root.clone(), map.clone(), snap.clone(), labels(), follow, wake)
        .map_err(|e| tf!("livelink.start_failed", e))?;
    let rt = &mut state.runtime.livelink;
    rt.session = Some(session);
    rt.current = Some(snap);
    rt.snapshot_revision = state.doc.revision();
    rt.highlight_sig = 0;
    rt.camera_sent = None;
    rt.camera_from_game_applied = None;
    rt.panel_open = true;
    let text = tf!("livelink.started", map);
    rt.note = Some(text.clone());
    Ok(text)
}

pub fn stop(state: &mut AppState) -> bool {
    let Some(session) = state.runtime.livelink.session.take() else { return false };
    let root = session.root.clone();
    session.stop();
    state.runtime.livelink.current = None;
    if state.settings.livelink.remove_addon_on_stop
        && let Err(e) = addon::remove(&root)
    {
        state.log(Severity::Warning, tf!("livelink.remove_failed", e));
    }
    state.runtime.livelink.note = Some(t!("livelink.stopped").to_owned());
    true
}

pub fn remove_addon(state: &mut AppState) -> Result<String, String> {
    stop(state);
    let Some(root) = game_root(state) else { return Err(t!("livelink.no_game").to_owned()) };
    if hp_compile::sandbox::is_sandboxed() && std::env::var_os(ROOT_ENV).is_none() {
        return Err(hp_compile::sandbox::REFUSED.to_owned());
    }
    addon::remove(&root).map_err(|e| tf!("livelink.remove_failed", e))?;
    let text = t!("livelink.removed").to_owned();
    state.runtime.livelink.note = Some(text.clone());
    Ok(text)
}

fn require_connected(state: &AppState) -> Result<(), String> {
    match state.runtime.livelink.session.as_ref().map(Session::state) {
        None => Err(t!("livelink.off").to_owned()),
        Some(LinkState::Connected) => Ok(()),
        Some(LinkState::OtherMap) => Err(t!("livelink.other_map").to_owned()),
        Some(_) => Err(t!("livelink.no_game_link").to_owned()),
    }
}

fn push(state: &mut AppState, ops: Vec<Op>) {
    if let Some(s) = state.runtime.livelink.session.as_mut() {
        s.push(ops);
    }
}

pub fn camera_pose(state: &AppState) -> ([f64; 3], [f64; 3]) {
    let cam = &state.cameras.cam3d;
    (cam.pos.to_array(), [-cam.pitch_deg, cam.yaw_deg, 0.0])
}

pub fn teleport(state: &mut AppState, pose: Option<([f64; 3], [f64; 3])>, noclip: bool) -> Result<String, String> {
    require_connected(state)?;
    let (origin, angles) = pose.unwrap_or_else(|| camera_pose(state));
    if !origin.iter().chain(&angles).all(|v| v.is_finite()) {
        return Err(t!("livelink.bad_position").to_owned());
    }
    push(state, vec![Op::Teleport { origin, angles, noclip }]);
    Ok(tf!("livelink.teleported", format!("{:.0} {:.0} {:.0}", origin[0], origin[1], origin[2])))
}

pub fn back(state: &mut AppState) -> Result<String, String> {
    require_connected(state)?;
    push(state, vec![Op::Back]);
    Ok(t!("livelink.sent_back").to_owned())
}

pub fn resync(state: &mut AppState) -> Result<String, String> {
    let Some(s) = state.runtime.livelink.session.as_mut() else { return Err(t!("livelink.off").to_owned()) };
    s.resync();
    state.runtime.livelink.highlight_sig = 0;
    Ok(t!("livelink.resynced").to_owned())
}

pub fn mark_compiled(state: &mut AppState) -> Result<String, String> {
    let snap = snapshot(state);
    let Some(s) = state.runtime.livelink.session.as_mut() else { return Err(t!("livelink.off").to_owned()) };
    s.mark_compiled(snap);
    Ok(t!("livelink.marked").to_owned())
}

pub fn entity_target(state: &AppState, id: u32) -> Option<Target> {
    let s = state.runtime.livelink.session.as_ref()?;
    s.planner.target_of(&s.baseline, s.matches.as_ref()?, id)
}

pub fn fire(state: &mut AppState, target: &str, input: &str, param: &str, delay: f64) -> Result<String, String> {
    require_connected(state)?;
    let target = target.trim();
    let input = input.trim();
    if target.is_empty() {
        return Err(t!("livelink.fire_no_target").to_owned());
    }
    if !hp_livelink::protocol::input_allowed(input) {
        return Err(tf!("livelink.fire_bad_input", input));
    }
    if param.len() > 512 || param.contains(['\n', '\r']) || !(0.0..=3600.0).contains(&delay) {
        return Err(t!("livelink.fire_bad_param").to_owned());
    }
    let by_id = target.strip_prefix('#').and_then(|n| n.parse::<u32>().ok());
    let op = match by_id {
        Some(id) => {
            let Some(t) = entity_target(state, id) else { return Err(tf!("livelink.fire_not_matched", id)) };
            Op::Fire { name: None, target: Some(t), input: input.to_owned(), param: param.to_owned(), delay }
        }
        None => Op::Fire {
            name: Some(target.to_owned()),
            target: None,
            input: input.to_owned(),
            param: param.to_owned(),
            delay,
        },
    };
    push(state, vec![op]);
    Ok(tf!("livelink.fired", input, target))
}

fn highlight_items(state: &AppState) -> Vec<HighlightItem> {
    let scene = state.doc.scene();
    let mut out = Vec::new();
    for obj in state.doc.selection().objects.iter().take(MAX_HIGHLIGHT) {
        match *obj {
            ObjRef::Entity(id) => {
                let Some(node) = scene.entity(id) else { continue };
                let static_prop = node.entity.classname.eq_ignore_ascii_case("prop_static");
                let bounds = state.entity_bounds(&node.entity).or_else(|| {
                    let mut b: Option<hp_geom::Aabb> = None;
                    for s in scene.solids().values() {
                        if s.owner == hp_editor::Owner::Entity(id) {
                            b = Some(b.map_or(s.bounds, |a| a.union(&s.bounds)));
                        }
                    }
                    b
                });
                let Some(b) = bounds else { continue };
                out.push(HighlightItem {
                    target: if static_prop { None } else { entity_target(state, id) },
                    ghost: static_prop.then_some(id),
                    mins: b.min.to_array(),
                    maxs: b.max.to_array(),
                });
            }
            ObjRef::Solid(id) => {
                if let Some(s) = scene.solid(id) {
                    out.push(HighlightItem {
                        target: None,
                        ghost: None,
                        mins: s.bounds.min.to_array(),
                        maxs: s.bounds.max.to_array(),
                    });
                }
            }
        }
    }
    out
}

fn highlight_signature(items: &[HighlightItem]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    items.len().hash(&mut h);
    for i in items {
        format!("{:?}", i.target).hash(&mut h);
        i.ghost.hash(&mut h);
        for v in i.mins.iter().chain(&i.maxs) {
            v.to_bits().hash(&mut h);
        }
    }
    h.finish() | 1
}

pub struct TickInput {
    pub compiling: bool,
}

fn refresh_snapshot(state: &mut AppState, force: bool) -> bool {
    let rev = state.doc.revision();
    let rt = &state.runtime.livelink;
    let due = rt.last_snapshot.is_none_or(|t| t.elapsed() >= SNAPSHOT_EVERY);
    if !force && (rev == rt.snapshot_revision || !due) {
        return false;
    }
    let snap = snapshot(state);
    let rt = &mut state.runtime.livelink;
    rt.snapshot_revision = rev;
    rt.last_snapshot = Some(Instant::now());
    let changed = rt.current.as_ref() != Some(&snap);
    rt.current = Some(snap);
    changed || force
}

pub fn push_now(state: &mut AppState) {
    refresh_snapshot(state, true);
    let current = state.runtime.livelink.current.take().unwrap_or_default();
    if let Some(s) = state.runtime.livelink.session.as_mut() {
        s.previews = state.settings.livelink.light_previews;
        s.update(&current);
    }
    state.runtime.livelink.current = Some(current);
}

fn on_event(state: &mut AppState, event: Event) {
    match event {
        Event::Connected { map } => state.notify(Severity::Success, tf!("livelink.connected", map)),
        Event::NewLoad { map, rebased } => {
            let text = if rebased { tf!("livelink.reloaded_compiled", map) } else { tf!("livelink.reloaded", map) };
            state.log(Severity::Info, text);
        }
        Event::Matched { matched, missing } => state.log(Severity::Info, tf!("livelink.matched", matched, missing)),
        Event::Eye { origin, angles } => {
            if state.settings.livelink.camera_from_game && origin.iter().chain(&angles).all(|v| v.is_finite()) {
                let cam = &mut state.cameras.cam3d;
                cam.pos = glam::DVec3::from_array(origin);
                cam.pitch_deg = -angles[0];
                cam.yaw_deg = angles[1];
                state.runtime.livelink.camera_from_game_applied = Some(camera_pose(state));
            }
        }
        Event::Error(e) => state.log(Severity::Warning, tf!("livelink.game_error", e)),
        Event::Stopped => {}
    }
}

fn inject_if_needed(state: &mut AppState) {
    if !state.settings.livelink.load_into_running_game {
        return;
    }
    let Some(s) = state.runtime.livelink.session.as_ref() else { return };
    let waiting = matches!(s.state(), LinkState::WaitingForGame | LinkState::Lost);
    let quiet_for = s.status.last_report.map_or(s.started.elapsed(), |t| t.elapsed());
    if !waiting || quiet_for < INJECT_AFTER || s.injected_at.is_some_and(|t| t.elapsed() < INJECT_EVERY) {
        return;
    }
    let root = s.root.clone();
    let Some(game) = hp_compile::launch::process::find_game(&root).filter(|g| g.is_ready()) else { return };
    if let Some(s) = state.runtime.livelink.session.as_mut() {
        s.injected_at = Some(Instant::now());
    }
    match hp_compile::launch::process::send_command_line(&game, &addon::inject_command()) {
        Ok(()) => state.log(Severity::Info, t!("livelink.injected")),
        Err(e) => state.log(Severity::Warning, tf!("livelink.inject_failed", e)),
    }
}

pub fn tick(state: &mut AppState, input: &TickInput) {
    if state.runtime.livelink.session.is_none() {
        state.runtime.livelink.was_compiling = input.compiling;
        return;
    }
    if input.compiling && !state.runtime.livelink.was_compiling {
        let snap = snapshot(state);
        if let Some(s) = state.runtime.livelink.session.as_mut() {
            s.note_compile_started(snap);
        }
    }
    state.runtime.livelink.was_compiling = input.compiling;
    if let Some(map) = map_name(state)
        && let Some(s) = state.runtime.livelink.session.as_mut()
        && !s.map.eq_ignore_ascii_case(&map)
    {
        s.map = map;
    }
    let changed = refresh_snapshot(state, false);
    let current = state.runtime.livelink.current.take().unwrap_or_default();
    let events = state.runtime.livelink.session.as_mut().map(|s| s.handle(&current)).unwrap_or_default();
    let reconnected = events.iter().any(|e| matches!(e, Event::Matched { .. } | Event::Connected { .. }));
    if changed && let Some(s) = state.runtime.livelink.session.as_mut() {
        s.previews = state.settings.livelink.light_previews;
        s.update(&current);
    }
    state.runtime.livelink.current = Some(current);
    for e in events {
        on_event(state, e);
    }
    if reconnected {
        state.runtime.livelink.highlight_sig = 0;
    }
    follow_flag(state);
    if state.runtime.livelink.is_connected() {
        sync_highlight(state);
        sync_camera(state);
    }
    inject_if_needed(state);
}

fn follow_flag(state: &mut AppState) {
    let want = state.settings.livelink.camera_from_game;
    if let Some(s) = state.runtime.livelink.session.as_mut()
        && s.follow != want
    {
        s.follow = want;
        s.send(ToWorker::Follow(want));
    }
}

fn sync_highlight(state: &mut AppState) {
    let items = if state.settings.livelink.highlight_selection { highlight_items(state) } else { Vec::new() };
    let sig = if items.is_empty() { 2 } else { highlight_signature(&items) };
    if sig == state.runtime.livelink.highlight_sig {
        return;
    }
    state.runtime.livelink.highlight_sig = sig;
    push(state, vec![Op::Highlight { items }]);
}

fn sync_camera(state: &mut AppState) {
    if !state.settings.livelink.camera_to_game || state.settings.livelink.camera_from_game {
        return;
    }
    let (origin, angles) = camera_pose(state);
    if let Some((o, a, t)) = state.runtime.livelink.camera_sent {
        let moved = (0..3).any(|i| (o[i] - origin[i]).abs() > 0.25) || (0..2).any(|i| (a[i] - angles[i]).abs() > 0.1);
        if !moved || t.elapsed() < CAMERA_EVERY {
            return;
        }
    }
    state.runtime.livelink.camera_sent = Some((origin, angles, Instant::now()));
    push(state, vec![Op::Teleport { origin, angles, noclip: true }]);
}

pub fn state_text(state: Option<LinkState>) -> &'static str {
    match state {
        None => t!("livelink.state.off"),
        Some(LinkState::WaitingForGame) => t!("livelink.state.waiting"),
        Some(LinkState::Connected) => t!("livelink.state.connected"),
        Some(LinkState::OtherMap) => t!("livelink.state.other_map"),
        Some(LinkState::Lost) => t!("livelink.state.lost"),
    }
}

pub fn reason_text(r: hp_livelink::Reason) -> &'static str {
    use hp_livelink::Reason as R;
    match r {
        R::Brushes => t!("livelink.reason.brushes"),
        R::Lighting => t!("livelink.reason.lighting"),
        R::StaticProps => t!("livelink.reason.static_props"),
        R::BrushEntities => t!("livelink.reason.brush_entities"),
        R::RemovedOutputs => t!("livelink.reason.removed_outputs"),
        R::ClassChanged => t!("livelink.reason.class_changed"),
        R::NotInGame => t!("livelink.reason.not_in_game"),
    }
}

pub fn status_json(state: &AppState) -> Value {
    let settings = &state.settings.livelink;
    let root = game_root(state);
    let addon_installed = root.as_deref().is_some_and(addon::is_installed);
    let base = json!({
        "settings": {
            "camera_to_game": settings.camera_to_game,
            "camera_from_game": settings.camera_from_game,
            "highlight_selection": settings.highlight_selection,
            "light_previews": settings.light_previews,
            "load_into_running_game": settings.load_into_running_game,
            "remove_addon_on_stop": settings.remove_addon_on_stop,
        },
        "game_folder": root.as_ref().map(|p| p.display().to_string()),
        "addon_installed": addon_installed,
        "note": state.runtime.livelink.note,
    });
    let Some(s) = state.runtime.livelink.session.as_ref() else {
        let mut v = base;
        v["on"] = json!(false);
        v["state"] = json!("off");
        return v;
    };
    let st = &s.status;
    let ms = |d: Option<Duration>| d.map(|d| (d.as_secs_f64() * 10_000.0).round() / 10.0);
    let matches = s.matches.as_ref();
    let mut v = base;
    v["on"] = json!(true);
    v["state"] = json!(s.state().id());
    v["map"] = json!(s.map);
    v["game_map"] = json!(st.game_map);
    v["addon_mode"] = json!(st.addon);
    v["latency_ms"] = json!(ms(st.latency));
    v["average_latency_ms"] = json!(ms(st.average_latency()));
    v["pending_operations"] = json!(st.pending);
    v["operations_sent"] = json!(st.sent);
    v["changed_since_compile"] = json!(st.changed);
    v["needs_compile"] = json!(st.needs_compile.iter().map(|r| r.id()).collect::<Vec<_>>());
    v["game_entities"] = json!(st.game_entities);
    v["matched_entities"] = json!(matches.map(|m| m.by_id.len()));
    v["unmatched_entities"] = json!(matches.map(|m| m.missing.clone()));
    v["matched_by"] = json!(matches.map(|m| json!({
        "hammerid": m.by_hammerid,
        "name": m.by_name,
        "origin": m.by_origin,
        "brush_order": m.by_order,
    })));
    v["skipped"] = json!(st.skipped.iter().map(|(id, why)| json!({ "id": id, "why": why })).collect::<Vec<_>>());
    v["created_in_game"] = json!(st.created);
    v["static_prop_previews"] = json!(st.ghosts);
    v["light_previews"] = json!(st.lights);
    v["player_eye"] = json!(st.eye.map(|(o, a)| json!({ "origin": o, "angles": a })));
    v["errors"] = json!(st.errors.iter().rev().take(20).collect::<Vec<_>>());
    v["write_error"] = json!(st.write_error);
    v
}

#[cfg(test)]
mod tests;
