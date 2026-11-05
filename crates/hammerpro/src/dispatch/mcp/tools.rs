use std::path::{Path, PathBuf};

use glam::DVec3;
use hp_app::settings::CompilePresetSetting;
use hp_app::state::AssetStatus;
use hp_app::{AppState, ViewKind};
use hp_editor::{Document, FaceRef, ObjRef, Owner, SelectOp};
use hp_mcp::protocol::ToolResult;
use hp_mcp::tools::{MAX_LIST, MAX_SCREENSHOT, MAX_VMF_TEXT, MIN_SCREENSHOT};
use serde_json::{Map, Value, json};

pub(super) const MAX_COORD: f64 = 131_072.0;
pub(super) const MAX_NAME: usize = 255;
pub(super) const MAX_VALUE: usize = 1023;
pub(super) const MAX_IDS: usize = 10_000;
const SCREENSHOT_VIEW: u32 = 0x4D43_5000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AppOp {
    Compile { preset: Option<CompilePresetSetting>, launch: bool },
    Open(PathBuf),
}

pub(crate) struct AppFn(pub(crate) AppWork);

pub(crate) type AppWork = Box<dyn FnOnce(&mut crate::app::HammerProApp, &egui::Context) -> ToolResult>;

impl std::fmt::Debug for AppFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppFn")
    }
}

#[derive(Debug)]
pub(crate) enum Outcome {
    Done(ToolResult),
    App(AppOp),
    AppFn(AppFn),
    Ui(super::automation::UiOp),
    Worker(Worker),
}

pub(crate) type ApplyFn = Box<dyn FnOnce(&mut AppState) -> Option<ToolResult> + Send>;

pub(crate) struct Worker(std::sync::mpsc::Receiver<(ToolResult, Option<ApplyFn>)>);

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Worker")
    }
}

impl Worker {
    pub(crate) fn spawn(tool: &'static str, f: impl FnOnce() -> ToolResult + Send + 'static) -> Outcome {
        Self::spawn_then(tool, move || (f(), None))
    }

    pub(crate) fn spawn_then(
        tool: &'static str,
        f: impl FnOnce() -> (ToolResult, Option<ApplyFn>) + Send + 'static,
    ) -> Outcome {
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name(format!("mcp-{tool}")).spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| {
                (ToolResult::error(format!("{tool}: internal error (see the Hammer Pro log)")), None)
            });
            let _ = tx.send(r);
        });
        match spawned {
            Ok(_) => Outcome::Worker(Worker(rx)),
            Err(e) => Outcome::Done(ToolResult::error(format!("{tool}: cannot start a worker thread: {e}"))),
        }
    }

    pub(crate) fn poll(&self, state: &mut AppState) -> Option<ToolResult> {
        match self.0.try_recv() {
            Ok((result, apply)) => Some(match apply {
                Some(f) => f(state).unwrap_or(result),
                None => result,
            }),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(ToolResult::error("the worker stopped")),
        }
    }

    #[cfg(test)]
    pub(crate) fn wait(self, state: &mut AppState, timeout: std::time::Duration) -> ToolResult {
        match self.0.recv_timeout(timeout) {
            Ok((result, apply)) => match apply {
                Some(f) => f(state).unwrap_or(result),
                None => result,
            },
            Err(_) => ToolResult::error("the worker did not answer"),
        }
    }
}

pub(crate) struct Domain {
    pub(crate) category: &'static str,
    pub(crate) specs: &'static [hp_mcp::tools::ToolSpec],
    pub(crate) run: fn(&mut AppState, &str, &Value) -> Option<Outcome>,
}

pub(crate) const DOMAINS: &[&Domain] = &[
    &super::tools_edit::DOMAIN,
    &super::tools_weld::DOMAIN,
    &super::tools_loops::DOMAIN,
    &super::tools_pipe::DOMAIN,
    &super::tools_slice::DOMAIN,
    &super::tools_road::DOMAIN,
    &super::tools_roadside::DOMAIN,
    &super::tools_groups::DOMAIN,
    &super::tools_grids::DOMAIN,
    &super::tools_disp::DOMAIN,
    &super::tools_slope::DOMAIN,
    &super::tools_terrain::DOMAIN,
    &super::tools_meshdisp::DOMAIN,
    &super::tools_instances::DOMAIN,
    &super::tools_entities::DOMAIN,
    &super::tools_assets::DOMAIN,
    &super::tools_compile::DOMAIN,
    &super::tools_compilers::DOMAIN,
    &super::tools_health::DOMAIN,
    &super::tools_content::DOMAIN,
    &super::tools_docs::DOMAIN,
    &super::tools_history::DOMAIN,
    &super::tools_overlays::DOMAIN,
    &super::tools_particles::DOMAIN,
    &super::tools_paths::DOMAIN,
    &super::tools_sky::DOMAIN,
    &super::tools_fog::DOMAIN,
    &super::tools_detail::DOMAIN,
    &super::tools_fgd::DOMAIN,
    &super::tools_games::DOMAIN,
    &super::tools_discord::DOMAIN,
    &super::tools_update::DOMAIN,
    &super::tools_blender::DOMAIN,
    &super::tools_toolopts::DOMAIN,
    &super::tools_actions::DOMAIN,
    &super::tools_ui::DOMAIN,
    &super::tools_settings::DOMAIN,
    &super::tools_views::DOMAIN,
    &super::tools_gpu::DOMAIN,
    &super::tools_storage::DOMAIN,
    &super::tools_sniffer::DOMAIN,
    &super::tools_shader::DOMAIN,
    &super::tools_vtf::DOMAIN,
    &super::tools_media::DOMAIN,
    &super::tools_help::DOMAIN,
];

pub(crate) fn all_specs() -> impl Iterator<Item = &'static hp_mcp::tools::ToolSpec> {
    hp_mcp::tools::EDITOR_TOOLS.iter().chain(DOMAINS.iter().flat_map(|d| d.specs.iter()))
}

pub(crate) fn all_definitions() -> Vec<Value> {
    all_specs().map(hp_mcp::tools::ToolSpec::definition).collect()
}

impl From<ToolResult> for Outcome {
    fn from(r: ToolResult) -> Self {
        Outcome::Done(r)
    }
}

pub(super) type R<T> = Result<T, String>;

pub(crate) fn undo_label(tool: &str) -> String {
    format!("MCP: {tool}")
}

pub(crate) fn run(state: &mut AppState, name: &str, args: &Value) -> Outcome {
    state.doc.end_selection_burst();
    let outcome = run_tool(state, name, args);
    state.doc.end_selection_burst();
    outcome
}

fn run_tool(state: &mut AppState, name: &str, args: &Value) -> Outcome {
    let result = match name {
        "get_map_info" => no_args(args).map(|()| get_map_info(state)),
        "list_entities" => list_entities(state, args),
        "get_entity" => get_entity(state, args),
        "create_entity" => create_entity(state, args),
        "set_entity_keyvalues" => set_entity_keyvalues(state, args),
        "delete_objects" => delete_objects(state, args),
        "list_solids" => list_solids(state, args),
        "create_brush" => create_brush(state, args),
        "apply_material" => apply_material(state, args),
        "move_objects" => move_objects(state, args),
        "rotate_objects" => rotate_objects(state, args),
        "select_objects" => select_objects(state, args),
        "get_selection" => no_args(args).map(|()| get_selection(state)),
        "clear_selection" => no_args(args).map(|()| clear_selection(state)),
        "search_materials" => search_assets(state, args, false),
        "search_models" => search_assets(state, args, true),
        "get_vmf_text" => get_vmf_text(state, args),
        "paste_vmf_text" => paste_vmf_text(state, args),
        "check_problems" => check_problems(state, args),
        "compile_map" => {
            return compile_map(state, args).unwrap_or_else(|e| ToolResult::error(format!("{name}: {e}")).into());
        }
        "get_compile_status" => no_args(args).map(|()| compile_status(state)),
        "save_map" => save_map(state, args),
        "open_map" => return open_map(args).unwrap_or_else(|e| ToolResult::error(format!("{name}: {e}")).into()),
        "undo" => no_args(args).and_then(|()| undo_redo(state, true)),
        "redo" => no_args(args).and_then(|()| undo_redo(state, false)),
        "set_camera" => set_camera(state, args),
        "screenshot_view" => screenshot_view(state, args),
        other => match DOMAINS.iter().find_map(|d| (d.run)(state, other, args)) {
            Some(outcome) => return outcome,
            None => Err(format!("unknown tool {other}")),
        },
    };
    Outcome::Done(result.unwrap_or_else(|e| ToolResult::error(format!("{name}: {e}"))))
}

pub(super) struct Args<'a> {
    pub(super) map: &'a Map<String, Value>,
}

pub(super) fn args<'a>(v: &'a Value, allowed: &[&str]) -> R<Args<'a>> {
    let map = v.as_object().ok_or("arguments must be an object")?;
    if let Some(k) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(if allowed.is_empty() {
            format!("this tool takes no arguments (got \"{k}\")")
        } else {
            format!("unknown argument \"{k}\" (expected: {})", allowed.join(", "))
        });
    }
    Ok(Args { map })
}

pub(super) fn no_args(v: &Value) -> R<()> {
    args(v, &[]).map(|_| ())
}

pub(super) fn coord_ok(v: f64) -> bool {
    v.is_finite() && v.abs() <= MAX_COORD
}

impl Args<'_> {
    pub(super) fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key).filter(|v| !v.is_null())
    }

    pub(super) fn opt_u64(&self, key: &str, min: u64, max: u64) -> R<Option<u64>> {
        let Some(v) = self.get(key) else { return Ok(None) };
        let n = v.as_u64().or_else(|| v.as_f64().filter(|f| f.fract() == 0.0 && *f >= 0.0).map(|f| f as u64));
        match n {
            Some(n) if (min..=max).contains(&n) => Ok(Some(n)),
            _ => Err(format!("{key} must be an integer from {min} to {max}")),
        }
    }

    pub(super) fn id(&self, key: &str) -> R<u32> {
        self.opt_u64(key, 1, u64::from(u32::MAX))?.map(|n| n as u32).ok_or_else(|| format!("{key} is required"))
    }

    pub(super) fn opt_f64(&self, key: &str, min: f64, max: f64) -> R<Option<f64>> {
        let Some(v) = self.get(key) else { return Ok(None) };
        match v.as_f64() {
            Some(f) if f.is_finite() && (min..=max).contains(&f) => Ok(Some(f)),
            _ => Err(format!("{key} must be a number from {min} to {max}")),
        }
    }

    pub(super) fn opt_bool(&self, key: &str) -> R<Option<bool>> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(format!("{key} must be true or false")),
        }
    }

    pub(super) fn opt_str(&self, key: &str, max: usize) -> R<Option<&str>> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::String(s)) if s.len() <= max => Ok(Some(s.as_str())),
            Some(Value::String(_)) => Err(format!("{key} is too long (at most {max} bytes)")),
            Some(_) => Err(format!("{key} must be a string")),
        }
    }

    pub(super) fn str(&self, key: &str, max: usize) -> R<&str> {
        self.opt_str(key, max)?.ok_or_else(|| format!("{key} is required"))
    }

    pub(super) fn opt_enum(&self, key: &str, values: &[&str]) -> R<Option<String>> {
        let Some(s) = self.opt_str(key, 64)? else { return Ok(None) };
        let s = s.to_ascii_lowercase();
        if values.contains(&s.as_str()) {
            Ok(Some(s))
        } else {
            Err(format!("{key} must be one of: {}", values.join(", ")))
        }
    }

    pub(super) fn opt_vec3(&self, key: &str) -> R<Option<DVec3>> {
        let Some(v) = self.get(key) else { return Ok(None) };
        let bad = || format!("{key} must be [x, y, z] with numbers from -{MAX_COORD} to {MAX_COORD}");
        let a = v.as_array().filter(|a| a.len() == 3).ok_or_else(bad)?;
        let mut out = [0.0; 3];
        for (o, x) in out.iter_mut().zip(a) {
            *o = x.as_f64().filter(|f| coord_ok(*f)).ok_or_else(bad)?;
        }
        Ok(Some(DVec3::from_array(out)))
    }

    pub(super) fn vec3(&self, key: &str) -> R<DVec3> {
        self.opt_vec3(key)?.ok_or_else(|| format!("{key} is required"))
    }

    pub(super) fn ids(&self, key: &str) -> R<Vec<u32>> {
        let Some(v) = self.get(key) else { return Ok(Vec::new()) };
        let a = v.as_array().ok_or_else(|| format!("{key} must be an array of ids"))?;
        if a.len() > MAX_IDS {
            return Err(format!("{key}: at most {MAX_IDS} ids per call"));
        }
        a.iter()
            .map(|x| {
                x.as_u64()
                    .filter(|n| (1..=u64::from(u32::MAX)).contains(n))
                    .map(|n| n as u32)
                    .ok_or_else(|| format!("{key} must contain positive integer ids"))
            })
            .collect()
    }

    pub(super) fn strings(&self, key: &str, max_items: usize) -> R<Vec<String>> {
        let Some(v) = self.get(key) else { return Ok(Vec::new()) };
        let a = v
            .as_array()
            .filter(|a| a.len() <= max_items)
            .ok_or_else(|| format!("{key} must be an array of at most {max_items} strings"))?;
        a.iter().map(|x| x.as_str().map(str::to_owned).ok_or_else(|| format!("{key} must contain strings"))).collect()
    }

    pub(super) fn keyvalues(&self, key: &str) -> R<Vec<(String, String)>> {
        let Some(v) = self.get(key) else { return Ok(Vec::new()) };
        let m = v.as_object().ok_or_else(|| format!("{key} must be an object of keyvalues"))?;
        if m.len() > 1000 {
            return Err(format!("{key}: at most 1000 keyvalues"));
        }
        m.iter()
            .map(|(k, v)| {
                let value = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => if *b { "1" } else { "0" }.to_owned(),
                    _ => return Err(format!("{key}.{k} must be a string")),
                };
                check_key(k)?;
                check_value(k, &value)?;
                Ok((k.clone(), value))
            })
            .collect()
    }
}

pub(super) fn check_key(k: &str) -> R<()> {
    if k.is_empty() || k.len() > 63 || !k.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '{' && c != '}') {
        return Err(format!("invalid key \"{k}\" (1 to 63 visible characters, no quotes or braces)"));
    }
    if k.eq_ignore_ascii_case("id") {
        return Err("the id key cannot be set".to_owned());
    }
    Ok(())
}

pub(super) fn check_value(k: &str, v: &str) -> R<()> {
    if v.len() > MAX_VALUE {
        return Err(format!("the value of {k} is longer than {MAX_VALUE} characters"));
    }
    if v.contains(['"', '\n', '\r', '\0']) {
        return Err(format!("the value of {k} contains a quote or a line break, which VMF files cannot store"));
    }
    Ok(())
}

pub(super) fn check_class(c: &str) -> R<()> {
    if c.is_empty() || c.len() > 63 || !c.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("invalid class name \"{c}\""));
    }
    Ok(())
}

pub(super) fn material_name(m: &str) -> R<String> {
    let m = m.trim().replace('\\', "/").to_ascii_lowercase();
    let m = m.strip_prefix("materials/").unwrap_or(&m);
    let m = m.strip_suffix(".vmt").unwrap_or(m);
    if m.is_empty() || m.len() > MAX_NAME || m.contains(['"', '\n', '\r', '\0', ' ']) || m.contains("..") {
        return Err(format!("invalid material name \"{m}\""));
    }
    Ok(m.to_owned())
}

pub(super) fn objects(doc: &Document, a: &Args<'_>) -> R<Vec<ObjRef>> {
    let mut out: Vec<ObjRef> = Vec::new();
    for id in a.ids("ids")? {
        let obj = if doc.scene().entity(id).is_some() {
            ObjRef::Entity(id)
        } else if doc.scene().solid(id).is_some() {
            ObjRef::Solid(id)
        } else {
            return Err(format!("no entity or solid has the id {id}"));
        };
        if !out.contains(&obj) {
            out.push(obj);
        }
    }
    for id in a.ids("solid_ids")? {
        if doc.scene().solid(id).is_none() {
            return Err(format!("no solid has the id {id}"));
        }
        if !out.contains(&ObjRef::Solid(id)) {
            out.push(ObjRef::Solid(id));
        }
    }
    Ok(out)
}

pub(super) fn obj_json(o: ObjRef) -> Value {
    match o {
        ObjRef::Solid(id) => json!({ "id": id, "kind": "solid" }),
        ObjRef::Entity(id) => json!({ "id": id, "kind": "entity" }),
    }
}

pub(super) fn v3(v: DVec3) -> Value {
    let r = |x: f64| (x * 1000.0).round() / 1000.0;
    json!([r(v.x), r(v.y), r(v.z)])
}

pub(super) fn bounds_json(b: Option<hp_geom::Aabb>) -> Value {
    match b.filter(|b| !b.is_empty()) {
        Some(b) => json!({ "min": v3(b.min), "max": v3(b.max) }),
        None => Value::Null,
    }
}

pub(super) fn edit<T>(
    state: &mut AppState,
    tool: &str,
    f: impl FnOnce(&mut Document) -> hp_editor::EditResult<T>,
) -> R<T> {
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    let label = undo_label(tool);
    state.doc.transaction(label, f).map_err(|e| e.to_string())
}

pub(super) fn entity_bounds(state: &AppState) -> impl Fn(&hp_vmf::Entity) -> Option<hp_geom::Aabb> + '_ {
    move |e| state.assets.entity_bounds(e)
}

pub(super) fn asset_status(state: &AppState) -> &'static str {
    match state.assets.status() {
        AssetStatus::NotConfigured => "game not configured",
        AssetStatus::Mounting => "loading",
        AssetStatus::Ready => "ready",
        AssetStatus::Failed(_) => "failed",
    }
}

fn get_map_info(state: &AppState) -> ToolResult {
    let doc = &state.doc;
    let s = doc.stats();
    let eb = entity_bounds(state);
    let cam = &state.cameras.cam3d;
    let world = doc.scene().world();
    ToolResult::json(json!({
        "map": state.map_title(),
        "path": doc.path().map(|p| p.display().to_string()),
        "unsaved_changes": doc.is_dirty(),
        "open_maps": state.runtime.workflow.tabs.len().max(1),
        "world": { "id": world.id, "skyname": world.get("skyname"), "keyvalues": world.props.len() },
        "stats": {
            "solids": s.solids,
            "world_solids": s.world_solids,
            "faces": s.faces,
            "entities": s.entities,
            "point_entities": s.point_entities,
            "brush_entities": s.brush_entities,
            "displacements": s.displacements,
            "materials": s.unique_materials,
            "groups": s.groups,
            "visgroups": s.visgroups,
            "invalid_solids": s.invalid_solids,
        },
        "bounds": bounds_json(s.bounds),
        "selection": {
            "objects": doc.selection().objects.len(),
            "faces": doc.selection().faces.len(),
            "bounds": bounds_json(doc.selection_bounds(&eb)),
        },
        "undo": doc.undo_label(),
        "redo": doc.redo_label(),
        "grid_size": state.grid_size(),
        "current_material": state.current_material,
        "camera_3d": { "position": v3(cam.pos), "yaw": cam.yaw_deg, "pitch": cam.pitch_deg },
        "game_content": {
            "status": asset_status(state),
            "materials": state.assets.materials().len(),
            "models": state.assets.models().len(),
            "entity_classes": state.assets.fgd().map_or(0, |f| f.len()),
        },
        "compile": compile_json(state),
    }))
}

fn list_entities(state: &AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["classname", "targetname", "limit", "offset"])?;
    let class = a.opt_str("classname", MAX_NAME)?;
    let name = a.opt_str("targetname", MAX_NAME)?;
    let limit = a.opt_u64("limit", 1, MAX_LIST)?.unwrap_or(100) as usize;
    let offset = a.opt_u64("offset", 0, u64::from(u32::MAX))?.unwrap_or(0) as usize;
    let matches = |p: Option<&str>, text: &str| p.is_none_or(|p| hp_editor::glob_match(p, text));
    let mut total = 0usize;
    let mut out = Vec::new();
    for (id, node) in doc_entities(&state.doc) {
        let e = &node.entity;
        if !matches(class, &e.classname) || !matches(name, e.get("targetname").unwrap_or("")) {
            continue;
        }
        if total >= offset && out.len() < limit {
            out.push(json!({
                "id": id,
                "classname": e.classname,
                "targetname": e.get("targetname"),
                "origin": e.get("origin"),
                "solids": state.doc.scene().entity_solid_count(id),
            }));
        }
        total += 1;
    }
    Ok(ToolResult::json(json!({ "total": total, "offset": offset, "entities": out })))
}

pub(super) fn doc_entities(doc: &Document) -> impl Iterator<Item = (u32, &hp_editor::EntityNode)> {
    doc.scene().entities().iter().map(|(id, n)| (*id, n))
}

pub(super) fn entity_json(e: &hp_vmf::Entity, solids: Vec<u32>, bounds: Option<hp_geom::Aabb>) -> Value {
    let mut kv = Map::new();
    for (k, v) in &e.props {
        kv.entry(k.clone()).or_insert_with(|| Value::String(v.clone()));
    }
    let outputs: Vec<Value> = e
        .connections
        .iter()
        .map(|c| {
            json!({
                "output": c.output,
                "target": c.target,
                "input": c.input,
                "parameter": c.param,
                "delay": c.delay,
                "times_to_fire": c.times,
            })
        })
        .collect();
    json!({
        "id": e.id,
        "classname": e.classname,
        "keyvalues": kv,
        "outputs": outputs,
        "solids": solids,
        "bounds": bounds_json(bounds),
    })
}

fn get_entity(state: &AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["id"])?;
    let id = a.id("id")?;
    let scene = state.doc.scene();
    if scene.world().id == id {
        let world_solids: Vec<u32> =
            scene.solids().iter().filter(|(_, n)| n.owner == Owner::World).map(|(id, _)| *id).take(10_000).collect();
        return Ok(ToolResult::json(entity_json(scene.world(), world_solids, state.doc.stats().bounds)));
    }
    let node = scene.entity(id).ok_or_else(|| format!("no entity has the id {id}"))?;
    let bounds = scene.object_bounds(ObjRef::Entity(id), &entity_bounds(state));
    Ok(ToolResult::json(entity_json(&node.entity, scene.entity_solid_ids(id), bounds)))
}

fn list_solids(state: &AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["material", "entity", "min", "max", "limit", "offset"])?;
    let material = a.opt_str("material", MAX_NAME)?.map(|m| m.replace('\\', "/"));
    let owner = a.opt_u64("entity", 0, u64::from(u32::MAX))?;
    let boxed = match (a.opt_vec3("min")?, a.opt_vec3("max")?) {
        (Some(min), Some(max)) => Some(hp_geom::Aabb::new(min, max)),
        (None, None) => None,
        _ => return Err("min and max go together".to_owned()),
    };
    let limit = a.opt_u64("limit", 1, MAX_LIST)?.unwrap_or(100) as usize;
    let offset = a.opt_u64("offset", 0, u64::from(u32::MAX))?.unwrap_or(0) as usize;
    let mut total = 0usize;
    let mut out = Vec::new();
    for (id, node) in state.doc.scene().solids().iter() {
        let owner_id = match node.owner {
            Owner::World => 0,
            Owner::Entity(e) => e,
        };
        if owner.is_some_and(|o| o != u64::from(owner_id)) {
            continue;
        }
        if let Some(b) = &boxed
            && !(node.bounds.min.cmple(b.max).all() && node.bounds.max.cmpge(b.min).all())
        {
            continue;
        }
        let sides = &node.solid.sides;
        if let Some(m) = &material
            && !sides.iter().any(|s| hp_editor::glob_match(m, &s.material))
        {
            continue;
        }
        if total >= offset && out.len() < limit {
            let mut mats: Vec<&str> = sides.iter().map(|s| s.material.as_str()).collect();
            mats.sort_unstable_by_key(|m| m.to_ascii_lowercase());
            mats.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            let faces: Vec<Value> =
                sides.iter().enumerate().map(|(i, s)| json!({ "side": i, "material": s.material })).collect();
            out.push(json!({
                "id": id,
                "entity": owner_id,
                "bounds": bounds_json(Some(node.bounds)),
                "materials": mats,
                "faces": faces,
            }));
        }
        total += 1;
    }
    Ok(ToolResult::json(json!({ "total": total, "offset": offset, "solids": out })))
}

fn get_selection(state: &AppState) -> ToolResult {
    let doc = &state.doc;
    let objs: Vec<Value> = doc.selected_objects().into_iter().take(MAX_LIST as usize).map(obj_json).collect();
    let faces: Vec<Value> = doc
        .selected_faces()
        .into_iter()
        .take(MAX_LIST as usize)
        .map(|f| json!({ "solid": f.solid, "side": f.side }))
        .collect();
    let mut out = json!({
        "objects": objs,
        "object_count": doc.selection().objects.len(),
        "faces": faces,
        "face_count": doc.selection().faces.len(),
        "bounds": bounds_json(doc.selection_bounds(&entity_bounds(state))),
    });
    if let (Some(map), Value::Object(extra)) = (out.as_object_mut(), super::tools_groups::element_json(state)) {
        map.extend(extra);
    }
    ToolResult::json(out)
}

fn search_assets(state: &AppState, v: &Value, models: bool) -> R<ToolResult> {
    let a = args(v, &["query", "limit"])?;
    let query = a.str("query", MAX_NAME)?.trim().replace('\\', "/").to_ascii_lowercase();
    let limit = a.opt_u64("limit", 1, 500)?.unwrap_or(50) as usize;
    let list = if models { state.assets.models() } else { state.assets.materials() };
    let glob = query.contains(['*', '?']);
    let mut total = 0usize;
    let mut out = Vec::new();
    for entry in list.iter() {
        let hit = if glob { hp_editor::glob_match(&query, &entry.name) } else { entry.name.contains(&query) };
        if hit {
            if out.len() < limit {
                out.push(Value::String(entry.name.clone()));
            }
            total += 1;
        }
    }
    let key = if models { "models" } else { "materials" };
    Ok(ToolResult::json(json!({ "total": total, key: out, "game_content": asset_status(state) })))
}

fn get_vmf_text(state: &AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["ids", "solid_ids"])?;
    let mut objs = objects(&state.doc, &a)?;
    if objs.is_empty() {
        objs = state.doc.selected_objects();
    }
    if objs.is_empty() {
        return Err("nothing selected and no ids given".to_owned());
    }
    let text = state.doc.copy(&objs);
    if text.len() > MAX_VMF_TEXT {
        return Err(format!("the VMF text is {} bytes (limit {MAX_VMF_TEXT}): ask for fewer objects", text.len()));
    }
    Ok(ToolResult::text(text))
}

fn check_problems(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["limit"])?;
    let limit = a.opt_u64("limit", 1, MAX_LIST)?.unwrap_or(100) as usize;
    let (problems, _) = super::tools_health::current_problems(state);
    let list: Vec<Value> = problems
        .iter()
        .take(limit)
        .map(|p| {
            let severity = match p.severity {
                hp_editor::Severity::Info => "info",
                hp_editor::Severity::Warning => "warning",
                hp_editor::Severity::Error => "error",
            };
            let objs: Vec<Value> = p.objects.iter().take(50).map(|o| obj_json(*o)).collect();
            json!({ "severity": severity, "code": p.code, "message": p.message, "objects": objs, "fixable": p.fixable })
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": problems.len(), "problems": list })))
}

pub(super) fn compile_json(state: &AppState) -> Value {
    let rt = &state.runtime.compile;
    let progress = |p: &hp_app::runtime::CompileProgress| json!({ "step": p.step, "fraction": p.fraction, "elapsed_s": p.elapsed_s });
    json!({
        "running": rt.progress.is_some(),
        "progress": rt.progress.as_ref().map(progress),
        "background": rt.background.as_ref().map(progress),
        "last_result": match rt.badge {
            Some(hp_app::runtime::CompileBadge::Leak) => "leak",
            Some(hp_app::runtime::CompileBadge::Failed) => "failed",
            None => "ok or none",
        },
    })
}

fn compile_status(state: &AppState) -> ToolResult {
    let mut v = compile_json(state);
    v["map"] = json!(state.doc.path().map(|p| p.display().to_string()));
    v["note"] =
        json!("get_compile_log reads the log and its problems (the Compile window of Hammer Pro shows it too).");
    ToolResult::json(v)
}

fn create_entity(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["classname", "origin", "angles", "keyvalues"])?;
    let class = a.str("classname", MAX_NAME)?.trim().to_owned();
    check_class(&class)?;
    let origin = a.vec3("origin")?;
    let mut kv: Vec<(String, String)> =
        a.keyvalues("keyvalues")?.into_iter().filter(|(k, _)| !k.eq_ignore_ascii_case("classname")).collect();
    if let Some(ang) = a.opt_vec3("angles")? {
        kv.retain(|(k, _)| !k.eq_ignore_ascii_case("angles"));
        kv.push(("angles".to_owned(), format!("{} {} {}", fmt_num(ang.x), fmt_num(ang.y), fmt_num(ang.z))));
    }
    kv.retain(|(k, _)| !k.eq_ignore_ascii_case("origin"));
    let id = edit(state, "create_entity", |doc| doc.create_point_entity(&class, origin, &kv))?;
    let mut out = json!({ "id": id, "classname": class, "origin": v3(origin) });
    if state.assets.fgd().is_some_and(|f| !f.is_empty() && !f.contains(&class)) {
        out["warning"] = json!(format!("{class} is not a class of the loaded entity definitions"));
    }
    Ok(ToolResult::json(out))
}

pub(super) fn fmt_num(x: f64) -> String {
    let r = (x * 1000.0).round() / 1000.0;
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}

fn set_entity_keyvalues(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["id", "set", "remove"])?;
    let id = a.id("id")?;
    let set = a.keyvalues("set")?;
    let remove = a.strings("remove", 1000)?;
    for k in &remove {
        check_key(k)?;
    }
    if set.is_empty() && remove.is_empty() {
        return Err("give keyvalues to set or keys to remove".to_owned());
    }
    let is_world = state.doc.scene().world().id == id;
    if !is_world && state.doc.scene().entity(id).is_none() {
        return Err(format!("no entity has the id {id}"));
    }
    if is_world
        && (set.iter().any(|(k, _)| k.eq_ignore_ascii_case("classname"))
            || remove.iter().any(|k| k.eq_ignore_ascii_case("classname")))
    {
        return Err("the class of the world cannot change".to_owned());
    }
    if remove.iter().any(|k| k.eq_ignore_ascii_case("classname")) {
        return Err("classname cannot be removed".to_owned());
    }
    let origin = !is_world
        && set.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("classname")
                && hp_app::runtime::helpers::brush_entity_needs_origin(state.assets.fgd(), v)
        })
        && !remove.iter().any(|k| k.eq_ignore_ascii_case("origin"));
    edit(state, "set_entity_keyvalues", |doc| {
        for (k, val) in &set {
            if is_world {
                doc.set_world_key(k, val)?;
            } else if k.eq_ignore_ascii_case("classname") {
                check_class(val).map_err(hp_editor::EditError::Invalid)?;
                doc.set_entity_class(&[id], val)?;
            } else {
                doc.set_entity_key(&[id], k, val)?;
            }
        }
        for k in &remove {
            if is_world {
                doc.remove_world_key(k)?;
            } else {
                doc.remove_entity_key(&[id], k)?;
            }
        }
        if origin {
            doc.add_missing_origins(&[id])?;
        }
        Ok(())
    })?;
    let scene = state.doc.scene();
    let e = if is_world { scene.world() } else { &scene.entity(id).ok_or("the entity is gone")?.entity };
    let kv: Map<String, Value> = e.props.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
    Ok(ToolResult::json(json!({ "id": id, "classname": e.classname, "keyvalues": kv })))
}

fn delete_objects(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["ids", "solid_ids"])?;
    let objs = objects(&state.doc, &a)?;
    if objs.is_empty() {
        return Err("give ids or solid_ids".to_owned());
    }
    let n = edit(state, "delete_objects", |doc| doc.delete(&objs))?;
    Ok(ToolResult::json(json!({ "deleted": n })))
}

fn create_brush(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["shape", "min", "max", "material", "sides", "entity_class", "keyvalues"])?;
    let shape = a.opt_enum(
        "shape",
        &[
            "block",
            "wedge",
            "cylinder",
            "spike",
            "sphere",
            "arch",
            "torus",
            "stairs",
            "spiral_stairs",
            "ramp",
            "pillar",
            "doorway",
        ],
    )?;
    let kind = match shape.as_deref().unwrap_or("block") {
        "wedge" => hp_geom::Primitive::Wedge,
        "cylinder" => hp_geom::Primitive::Cylinder,
        "spike" => hp_geom::Primitive::Spike,
        "sphere" => hp_geom::Primitive::Sphere,
        "arch" => hp_geom::Primitive::Arch,
        "torus" => hp_geom::Primitive::Torus,
        "stairs" => hp_geom::Primitive::Stairs,
        "spiral_stairs" => hp_geom::Primitive::SpiralStairs,
        "ramp" => hp_geom::Primitive::Ramp,
        "pillar" => hp_geom::Primitive::Pillar,
        "doorway" => hp_geom::Primitive::Doorway,
        _ => hp_geom::Primitive::Block,
    };
    let (p, q) = (a.vec3("min")?, a.vec3("max")?);
    let bounds = hp_geom::Aabb::new(p, q);
    let size = bounds.size();
    if size.min_element() < 1.0 {
        return Err("the box must be at least 1 unit on every axis".to_owned());
    }
    if size.max_element() > 65_536.0 {
        return Err("the box is larger than 65536 units".to_owned());
    }
    let material = match a.opt_str("material", MAX_NAME)? {
        Some(m) => material_name(m)?,
        None => state.current_material.clone(),
    };
    let sides = a.opt_u64("sides", 3, u64::from(hp_geom::MAX_PRIMITIVE_SIDES))?.unwrap_or(8) as u32;
    let class = a.opt_str("entity_class", MAX_NAME)?.map(|c| c.trim().to_owned());
    if let Some(c) = &class {
        check_class(c)?;
    }
    let kv = a.keyvalues("keyvalues")?;
    if class.is_none() && !kv.is_empty() {
        return Err("keyvalues need entity_class".to_owned());
    }
    let params = hp_geom::PrimitiveParams { kind, bounds, material: material.clone(), sides, ..Default::default() };
    let (solids, entity) = edit(state, "create_brush", |doc| match &class {
        Some(class) => {
            let mut next = doc.peek_next_side_id();
            let pieces = hp_geom::make_primitive(&params, &mut || {
                let id = next;
                next = next.saturating_add(1);
                id
            })?;
            let id = doc.create_brush_entity(class, &kv, pieces)?;
            let solids = doc.scene().entity_solid_ids(id);
            Ok((solids, Some(id)))
        }
        None => {
            let ids = doc.create_primitive(&params, Owner::World)?;
            let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
            doc.select(&objs, SelectOp::Replace);
            Ok((ids, None))
        }
    })?;
    let mut out = json!({
        "solid_ids": solids,
        "entity_id": entity,
        "material": material,
        "bounds": bounds_json(Some(bounds)),
    });
    if let Some(w) = material_warning(state, &material) {
        out["warning"] = json!(w);
    }
    Ok(ToolResult::json(out))
}

pub(super) fn material_warning(state: &AppState, material: &str) -> Option<String> {
    if state.assets.status() != AssetStatus::Ready {
        return None;
    }
    let found = state.assets.materials().iter().any(|m| m.name.eq_ignore_ascii_case(material));
    (!found).then(|| format!("the material {material} was not found in the game content (search_materials)"))
}

fn apply_material(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["material", "ids", "solid_ids", "faces"])?;
    let material = material_name(a.str("material", MAX_NAME)?)?;
    let objs = objects(&state.doc, &a)?;
    let mut faces = Vec::new();
    if let Some(list) = a.get("faces") {
        let list = list.as_array().filter(|l| l.len() <= MAX_IDS).ok_or("faces must be an array of {solid, side}")?;
        for f in list {
            let fa = args(f, &["solid", "side"]).map_err(|e| format!("faces: {e}"))?;
            let solid = fa.id("solid")?;
            let side = fa.opt_u64("side", 0, 1000)?.ok_or("faces: side is required")? as usize;
            let node = state.doc.scene().solid(solid).ok_or_else(|| format!("no solid has the id {solid}"))?;
            if side >= node.solid.sides.len() {
                return Err(format!("solid {solid} has {} sides (side {side} does not exist)", node.solid.sides.len()));
            }
            faces.push(FaceRef::new(solid, side));
        }
    }
    if objs.is_empty() && faces.is_empty() {
        return Err("give ids, solid_ids or faces".to_owned());
    }
    let n = edit(state, "apply_material", |doc| {
        let mut n = 0;
        if !objs.is_empty() {
            n += doc.apply_material_to_objects(&objs, &material)?;
        }
        if !faces.is_empty() {
            n += doc.apply_material(&faces, &material)?;
        }
        Ok(n)
    })?;
    let mut out = json!({ "faces_changed": n, "material": material });
    if let Some(w) = material_warning(state, &material) {
        out["warning"] = json!(w);
    }
    Ok(ToolResult::json(out))
}

fn move_objects(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["ids", "solid_ids", "offset"])?;
    let objs = objects(&state.doc, &a)?;
    let offset = a.vec3("offset")?;
    if objs.is_empty() {
        return Err("give ids or solid_ids".to_owned());
    }
    edit(state, "move_objects", |doc| doc.translate(&objs, offset))?;
    let b = state.doc.scene().objects_bounds(&objs, &entity_bounds(state));
    Ok(ToolResult::json(json!({ "moved": objs.len(), "bounds": bounds_json(b) })))
}

fn rotate_objects(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["ids", "solid_ids", "axis", "degrees", "pivot"])?;
    let objs = objects(&state.doc, &a)?;
    if objs.is_empty() {
        return Err("give ids or solid_ids".to_owned());
    }
    let axis = match a.opt_enum("axis", &["x", "y", "z"])?.as_deref() {
        Some("x") => DVec3::X,
        Some("y") => DVec3::Y,
        _ => DVec3::Z,
    };
    let degrees = a.opt_f64("degrees", -360.0, 360.0)?.ok_or("degrees is required")?;
    let pivot = match a.opt_vec3("pivot")? {
        Some(p) => p,
        None => state
            .doc
            .scene()
            .objects_bounds(&objs, &entity_bounds(state))
            .filter(|b| !b.is_empty())
            .map(|b| b.center())
            .ok_or("the objects have no bounds: give a pivot")?,
    };
    edit(state, "rotate_objects", |doc| doc.rotate_axis(&objs, pivot, axis, degrees))?;
    let b = state.doc.scene().objects_bounds(&objs, &entity_bounds(state));
    Ok(ToolResult::json(json!({ "rotated": objs.len(), "pivot": v3(pivot), "bounds": bounds_json(b) })))
}

fn select_objects(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["ids", "solid_ids", "mode"])?;
    let objs = objects(&state.doc, &a)?;
    let op = match a.opt_enum("mode", &["replace", "add", "remove"])?.as_deref() {
        Some("add") => SelectOp::Add,
        Some("remove") => SelectOp::Remove,
        _ => SelectOp::Replace,
    };
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    state.doc.select(&objs, op);
    Ok(ToolResult::json(json!({ "selected": state.doc.selection().objects.len() })))
}

fn clear_selection(state: &mut AppState) -> ToolResult {
    state.doc.clear_selection();
    state.doc.clear_face_selection();
    ToolResult::json(json!({ "selected": 0 }))
}

fn paste_vmf_text(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["text", "offset"])?;
    let text = a.str("text", MAX_VMF_TEXT)?;
    let offset = a.opt_vec3("offset")?.unwrap_or(DVec3::ZERO);
    Document::clipboard_bounds(text).map_err(|e| format!("the text has no map objects: {e}"))?;
    let objs = edit(state, "paste_vmf_text", |doc| doc.paste(text, offset))?;
    let ids: Vec<Value> = objs.iter().take(MAX_LIST as usize).map(|o| obj_json(*o)).collect();
    Ok(ToolResult::json(json!({ "pasted": objs.len(), "objects": ids })))
}

fn undo_redo(state: &mut AppState, undo: bool) -> R<ToolResult> {
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    let label = if undo { state.doc.undo_label() } else { state.doc.redo_label() }.map(str::to_owned);
    let Some(label) = label else {
        return Err(if undo { "nothing to undo" } else { "nothing to redo" }.to_owned());
    };
    let done = if undo { state.doc.undo() } else { state.doc.redo() };
    if !done {
        return Err("the step could not be applied".to_owned());
    }
    let key = if undo { "undone" } else { "redone" };
    Ok(ToolResult::json(json!({ key: label, "undo": state.doc.undo_label(), "redo": state.doc.redo_label() })))
}

fn compile_map(state: &AppState, v: &Value) -> R<Outcome> {
    let a = args(v, &["preset", "launch_game"])?;
    let preset = a.opt_enum("preset", &["draft", "normal", "final"])?.map(|p| match p.as_str() {
        "draft" => CompilePresetSetting::Draft,
        "final" => CompilePresetSetting::Final,
        _ => CompilePresetSetting::Normal,
    });
    let launch = a.opt_bool("launch_game")?.unwrap_or(false);
    if state.doc.path().is_none() {
        return Err("the map has no file yet: save it first (save_map, or File > Save as in Hammer Pro)".to_owned());
    }
    if state.runtime.compile.progress.is_some() {
        return Err("a compile is already running (get_compile_status)".to_owned());
    }
    Ok(Outcome::App(AppOp::Compile { preset, launch }))
}

pub(super) fn is_vmf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf"))
}

fn save_map(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["path", "overwrite"])?;
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    let current = state.doc.path().map(Path::to_path_buf);
    let target = match a.opt_str("path", 260)? {
        None => current.clone().ok_or(
            "the map has no file yet: save it once from Hammer Pro (File > Save as), then save_map can save it and \
             new versions next to it",
        )?,
        Some(name) => {
            let folder = current.as_ref().and_then(|p| p.parent()).ok_or(
                "the map has no file yet: save it once from Hammer Pro (File > Save as); new files go in its folder",
            )?;
            folder.join(vmf_file_name(name, folder)?)
        }
    };
    if !is_vmf(&target) {
        return Err("only .vmf files can be saved".to_owned());
    }
    let same = current.as_ref().is_some_and(|c| crate::tabs::same_file(c, &target));
    if target.exists() && !same && !overwrite {
        return Err(format!("{} exists: pass overwrite true to replace it", target.display()));
    }
    if !crate::files::save_map_to(state, &target) {
        return Err(format!("saving {} failed (see the Hammer Pro console)", target.display()));
    }
    Ok(ToolResult::json(json!({ "saved": target.display().to_string() })))
}

pub(super) fn vmf_file_name(name: &str, folder: &Path) -> R<String> {
    let name = name.trim();
    let candidate = Path::new(name);
    let file = if candidate.is_absolute() {
        let parent = candidate.parent().ok_or("invalid path")?;
        if !crate::tabs::same_file(parent, folder) {
            return Err(format!("new files must be in the folder of the current map ({})", folder.display()));
        }
        candidate.file_name().and_then(|n| n.to_str()).ok_or("invalid file name")?.to_owned()
    } else {
        name.to_owned()
    };
    let bad = file.is_empty()
        || file.len() > 200
        || file.starts_with('.')
        || file.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || file.chars().any(char::is_control)
        || file.contains("..");
    if bad {
        return Err(format!("\"{file}\" is not a plain file name"));
    }
    if !is_vmf(Path::new(&file)) {
        return Err("the file name must end with .vmf".to_owned());
    }
    Ok(file)
}

fn open_map(v: &Value) -> R<Outcome> {
    let a = args(v, &["path"])?;
    let path = PathBuf::from(a.str("path", 1024)?.trim());
    if !path.is_absolute() {
        return Err("give the full path of the .vmf file".to_owned());
    }
    if !is_vmf(&path) {
        return Err("only .vmf files can be opened".to_owned());
    }
    if !path.is_file() {
        return Err(format!("{} does not exist", path.display()));
    }
    Ok(Outcome::App(AppOp::Open(path)))
}

fn set_camera(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["position", "yaw", "pitch", "look_at", "center_2d", "zoom_2d"])?;
    let position = a.opt_vec3("position")?;
    let yaw = a.opt_f64("yaw", -360.0, 360.0)?;
    let pitch = a.opt_f64("pitch", -89.0, 89.0)?;
    let look_at = a.opt_vec3("look_at")?;
    let center = a.opt_vec3("center_2d")?;
    let zoom = a.opt_f64("zoom_2d", hp_app::view::Camera2d::MIN_ZOOM, hp_app::view::Camera2d::MAX_ZOOM)?;
    if a.map.is_empty() {
        return Err("give position, yaw, pitch, look_at, center_2d or zoom_2d".to_owned());
    }
    if look_at.is_some() && (yaw.is_some() || pitch.is_some()) {
        return Err("look_at sets yaw and pitch: give one or the other".to_owned());
    }
    let cams = &mut state.cameras;
    if let Some(p) = position {
        cams.cam3d.pos = p;
    }
    if let Some(y) = yaw {
        cams.cam3d.yaw_deg = y.rem_euclid(360.0);
    }
    if let Some(p) = pitch {
        cams.cam3d.pitch_deg = p;
    }
    if let Some(t) = look_at {
        cams.cam3d.look_at(t);
    }
    if let Some(c) = center {
        cams.center_2d(c);
    }
    if let Some(z) = zoom {
        for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
            cams.cam2d_mut(kind).zoom = z;
        }
    }
    let c = &cams.cam3d;
    Ok(ToolResult::json(json!({
        "camera_3d": { "position": v3(c.pos), "yaw": c.yaw_deg, "pitch": c.pitch_deg },
        "zoom_2d": cams.top.zoom,
    })))
}

fn screenshot_view(state: &mut AppState, v: &Value) -> R<ToolResult> {
    use hp_ui_viewports::params::{RenderConfig, view_info, view_options, view_params};
    let a = args(v, &["view", "width", "height"])?;
    let (kind, label) = match a.opt_enum("view", &["3d", "top", "front", "side"])?.as_deref() {
        Some("top") => (ViewKind::Top, "top"),
        Some("front") => (ViewKind::Front, "front"),
        Some("side") => (ViewKind::Side, "side"),
        _ => (ViewKind::Perspective, "3d"),
    };
    let w = a.opt_u64("width", MIN_SCREENSHOT, MAX_SCREENSHOT)?.unwrap_or(1280) as u32;
    let h = a.opt_u64("height", MIN_SCREENSHOT, MAX_SCREENSHOT)?.unwrap_or(720) as u32;
    let opts = state.scene_options();
    let cfg = RenderConfig::from_state(state);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w as f32, h as f32));
    let view = view_info(0, kind, rect, &state.cameras, &cfg, 1.0);
    let AppState { assets, doc, gfx, .. } = state;
    let gfx = gfx.as_mut().ok_or("the 3D renderer is not ready yet")?;
    assets.sync_scene(doc, gfx, &opts);
    let params = view_params(&view, &cfg);
    let mut options = view_options(&view, &cfg);
    if kind == ViewKind::Perspective && state.runtime.lab.player.active {
        options.compiled_lighting =
            state.settings.render.player_compiled_lighting && state.runtime.health.world.is_some();
        options.fog_volume_inside = options.fog_volumes;
        options.fog_volumes = false;
        if let Some(from) = state.ui.focused_view {
            gfx.renderer.inherit_tone(from as u32, SCREENSHOT_VIEW);
        }
    }
    gfx.renderer.set_tone_settings(state.settings.render.tone());
    gfx.renderer.set_view_options(SCREENSHOT_VIEW, options);
    gfx.renderer.render_view(SCREENSHOT_VIEW, &params, &hp_render::Overlay::default());
    let pixels = gfx.renderer.read_view_rgba8(SCREENSHOT_VIEW);
    gfx.renderer.remove_view(SCREENSHOT_VIEW);
    let (pw, ph, mut px) = pixels.map_err(|e| format!("reading the image back failed: {e}"))?;
    for p in px.chunks_exact_mut(4) {
        p[3] = 255;
    }
    let png = encode_png(pw, ph, px)?;
    Ok(ToolResult::png(&png, format!("{label} view, {pw}x{ph} pixels, map {}", state.map_title())))
}

pub(super) fn encode_png(w: u32, h: u32, rgba: Vec<u8>) -> R<Vec<u8>> {
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("bad image size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(state: &mut AppState, name: &str, args: Value) -> ToolResult {
        match run(state, name, &args) {
            Outcome::Done(r) => r,
            Outcome::App(op) => panic!("{name} asked the app for {op:?}"),
            Outcome::AppFn(_) => panic!("{name} asked the app to run a closure"),
            Outcome::Ui(op) => panic!("{name} asked the app for {op:?}"),
            Outcome::Worker(w) => w.wait(state, std::time::Duration::from_secs(60)),
        }
    }

    fn ok(state: &mut AppState, name: &str, args: Value) -> Value {
        let r = call(state, name, args);
        assert!(!r.is_error, "{name}: {}", r.first_text());
        r.structured.clone().unwrap_or_else(|| Value::String(r.first_text().to_owned()))
    }

    fn err(state: &mut AppState, name: &str, args: Value) -> String {
        let r = call(state, name, args);
        assert!(r.is_error, "{name} should fail: {}", r.first_text());
        r.first_text().to_owned()
    }

    #[test]
    fn every_tool_is_implemented() {
        let mut state = hp_app::testing::test_state();
        for t in hp_mcp::tools::EDITOR_TOOLS {
            let r = run(&mut state, t.name, &json!({ "zzz": 1 }));
            if let Outcome::Done(r) = r {
                assert!(!r.first_text().contains("unknown tool"), "{}", t.name);
            }
        }
    }

    #[test]
    fn brushes_entities_and_undo() {
        let mut state = hp_app::testing::test_state();
        let info = ok(&mut state, "get_map_info", json!({}));
        let base = info["stats"]["solids"].as_u64().unwrap();
        assert_eq!(base as usize, state.doc.stats().solids);

        let b = ok(
            &mut state,
            "create_brush",
            json!({ "min": [0, 0, 0], "max": [128, 64, 32], "material": "materials/Custom/MyTextures/brick01.vmt" }),
        );
        let solid = b["solid_ids"][0].as_u64().unwrap() as u32;
        assert_eq!(b["material"], "custom/mytextures/brick01");
        assert!(b.get("warning").is_none(), "the mock content has this material");
        assert_eq!(state.doc.undo_label(), Some("MCP: create_brush"));
        assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(solid)]);

        let e = ok(
            &mut state,
            "create_entity",
            json!({ "classname": "prop_static", "origin": [64, 32, 32], "angles": [0, 90, 0], "keyvalues": { "model": "models/props_c17/oildrum001.mdl", "skin": 1 } }),
        );
        let ent = e["id"].as_u64().unwrap() as u32;
        let got = ok(&mut state, "get_entity", json!({ "id": ent }));
        assert_eq!(got["keyvalues"]["angles"], "0 90 0");
        assert_eq!(got["keyvalues"]["skin"], "1");
        assert_eq!(got["keyvalues"]["origin"], "64 32 32");

        ok(
            &mut state,
            "set_entity_keyvalues",
            json!({ "id": ent, "set": { "targetname": "drum" }, "remove": ["skin"] }),
        );
        let got = ok(&mut state, "get_entity", json!({ "id": ent }));
        assert_eq!(got["keyvalues"]["targetname"], "drum");
        assert!(got["keyvalues"].get("skin").is_none());
        assert_eq!(state.doc.undo_label(), Some("MCP: set_entity_keyvalues"));

        let list = ok(&mut state, "list_entities", json!({ "classname": "prop_*" }));
        assert_eq!(list["total"], 1);
        assert_eq!(list["entities"][0]["targetname"], "drum");
        let solids = ok(&mut state, "list_solids", json!({ "material": "custom/*" }));
        assert_eq!(solids["total"], 1);
        assert_eq!(solids["solids"][0]["faces"].as_array().unwrap().len(), 6);

        ok(&mut state, "move_objects", json!({ "ids": [solid], "offset": [16, 0, 0] }));
        let b = state.doc.scene().solid_bounds(solid).unwrap();
        assert_eq!(b.min.x, 16.0);
        ok(&mut state, "rotate_objects", json!({ "ids": [solid], "axis": "z", "degrees": 90 }));
        assert_eq!(state.doc.undo_label(), Some("MCP: rotate_objects"));

        ok(
            &mut state,
            "apply_material",
            json!({ "material": "tools/toolsnodraw", "faces": [{ "solid": solid, "side": 0 }] }),
        );
        let w = ok(&mut state, "apply_material", json!({ "material": "nope/missing", "ids": [solid] }));
        assert!(w["warning"].as_str().unwrap().contains("not found"));

        let u = ok(&mut state, "undo", json!({}));
        assert_eq!(u["undone"], "MCP: apply_material");
        ok(&mut state, "redo", json!({}));
        let deleted = ok(&mut state, "delete_objects", json!({ "ids": [ent] }));
        assert_eq!(deleted["deleted"], 1);
        assert!(state.doc.scene().entity(ent).is_none());
        ok(&mut state, "undo", json!({}));
        assert!(state.doc.scene().entity(ent).is_some());

        let fd = ok(
            &mut state,
            "create_brush",
            json!({ "shape": "cylinder", "sides": 12, "min": [200, 0, 0], "max": [264, 64, 128], "entity_class": "func_detail" }),
        );
        let fid = fd["entity_id"].as_u64().unwrap() as u32;
        assert_eq!(state.doc.scene().entity(fid).unwrap().entity.classname, "func_detail");
        assert_eq!(fd["solid_ids"].as_array().unwrap().len(), 1);

        let world = state.doc.scene().world().id;
        ok(&mut state, "set_entity_keyvalues", json!({ "id": world, "set": { "skyname": "sky_day01_01" } }));
        assert_eq!(state.doc.scene().world().get("skyname"), Some("sky_day01_01"));
        err(&mut state, "set_entity_keyvalues", json!({ "id": world, "set": { "classname": "x" } }));
    }

    #[test]
    fn arguments_are_validated() {
        let mut state = hp_app::testing::test_state();
        for (tool, args) in [
            ("create_brush", json!({ "min": [0, 0, 0], "max": [0, 10, 10] })),
            ("create_brush", json!({ "min": [0, 0, 0], "max": [1e9, 10, 10] })),
            ("create_brush", json!({ "min": [0, 0], "max": [10, 10, 10] })),
            ("create_brush", json!({ "min": [0, 0, "a"], "max": [10, 10, 10] })),
            ("create_brush", json!({ "min": [0, 0, 0], "max": [10, 10, 10], "shape": "cube" })),
            ("create_brush", json!({ "min": [0, 0, 0], "max": [10, 10, 10], "sides": 1000 })),
            ("create_brush", json!({ "min": [0, 0, 0], "max": [10, 10, 10], "material": "a\"b" })),
            ("create_brush", json!({ "min": [0, 0, 0], "max": [10, 10, 10], "keyvalues": { "a": "b" } })),
            ("create_brush", json!({ "min": [0, 0, 0], "max": [10, 10, 10], "colour": 1 })),
            ("create_entity", json!({ "classname": "bad class", "origin": [0, 0, 0] })),
            ("create_entity", json!({ "classname": "light", "origin": [0, 0, 0], "keyvalues": { "a": "x\ny" } })),
            ("create_entity", json!({ "classname": "light", "origin": [0, 0, 0], "keyvalues": { "id": "5" } })),
            ("create_entity", json!({ "classname": "light", "origin": [0, 0, 0], "keyvalues": { "k": [1] } })),
            ("get_entity", json!({ "id": 999_999 })),
            ("get_entity", json!({ "id": -3 })),
            ("delete_objects", json!({ "ids": [123_456] })),
            ("delete_objects", json!({})),
            ("move_objects", json!({ "ids": [1], "offset": [0, 0, 0] })),
            ("rotate_objects", json!({ "degrees": 45 })),
            ("list_entities", json!({ "limit": 0 })),
            ("list_entities", json!({ "limit": 100000 })),
            ("get_map_info", json!({ "x": 1 })),
            ("save_map", json!({})),
            ("paste_vmf_text", json!({ "text": "hello" })),
            ("set_camera", json!({})),
            ("set_camera", json!({ "pitch": 120 })),
            ("set_camera", json!({ "look_at": [0, 0, 0], "yaw": 5 })),
            ("screenshot_view", json!({ "width": 5000 })),
            ("undo", json!({})),
            ("redo", json!({})),
            ("get_vmf_text", json!({})),
            ("search_materials", json!({})),
            ("open_map", json!({ "path": "relative.vmf" })),
            ("open_map", json!({ "path": "C:/Windows/win.ini" })),
            ("compile_map", json!({})),
        ] {
            let r = run(&mut state, tool, &args);
            let Outcome::Done(r) = r else { panic!("{tool} {args} should fail") };
            assert!(r.is_error, "{tool} {args} should fail: {}", r.first_text());
            assert!(r.first_text().starts_with(tool), "{}", r.first_text());
        }
        assert!(!state.doc.can_undo(), "failed calls leave no undo step");
        err(&mut state, "set_camera", json!({ "position": [1e300, 0, 0] }));
    }

    #[test]
    fn selection_vmf_text_and_problems() {
        let mut state = hp_app::testing::test_state();
        let base = state.doc.stats().solids;
        let b = ok(&mut state, "create_brush", json!({ "min": [0, 0, 0], "max": [64, 64, 64] }));
        let solid = b["solid_ids"][0].as_u64().unwrap();
        ok(&mut state, "clear_selection", json!({}));
        assert_eq!(ok(&mut state, "get_selection", json!({}))["object_count"], 0);
        ok(&mut state, "select_objects", json!({ "ids": [solid] }));
        let sel = ok(&mut state, "get_selection", json!({}));
        assert_eq!(sel["objects"][0]["kind"], "solid");
        let text = ok(&mut state, "get_vmf_text", json!({}));
        let text = text.as_str().unwrap().to_owned();
        assert!(text.contains("solid"));
        let p = ok(&mut state, "paste_vmf_text", json!({ "text": text, "offset": [128, 0, 0] }));
        assert_eq!(p["pasted"], 1);
        assert_eq!(state.doc.undo_label(), Some("MCP: paste_vmf_text"));
        assert_eq!(state.doc.stats().solids, base + 2);
        let probs = ok(&mut state, "check_problems", json!({ "limit": 10 }));
        assert!(probs["total"].as_u64().is_some());
        let mats = ok(&mut state, "search_materials", json!({ "query": "tools/*" }));
        assert_eq!(mats["total"], 2);
        let models = ok(&mut state, "search_models", json!({ "query": "oildrum" }));
        assert_eq!(models["models"][0], "models/props_c17/oildrum001.mdl");
        let cam =
            ok(&mut state, "set_camera", json!({ "position": [0, -256, 64], "look_at": [0, 0, 0], "zoom_2d": 0.5 }));
        assert!((cam["camera_3d"]["yaw"].as_f64().unwrap() - 90.0).abs() < 1e-6);
        assert_eq!(state.cameras.front.zoom, 0.5);
        assert!(err(&mut state, "screenshot_view", json!({ "view": "top" })).contains("renderer"));
        let status = ok(&mut state, "get_compile_status", json!({}));
        assert_eq!(status["running"], false);
    }

    #[test]
    fn app_operations() {
        let mut state = hp_app::testing::test_state();
        let dir = std::env::temp_dir().join(format!("hp_mcp_tools_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let map = dir.join("gm_mcp.vmf");
        std::fs::write(&map, "world\n{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n}\n").unwrap();
        let r = run(&mut state, "open_map", &json!({ "path": map.display().to_string() }));
        assert!(matches!(r, Outcome::App(AppOp::Open(p)) if p == map));
        state.doc.set_path(Some(map.clone()));
        ok(&mut state, "create_brush", json!({ "min": [0, 0, 0], "max": [64, 64, 64] }));
        ok(&mut state, "save_map", json!({}));
        assert!(std::fs::read_to_string(&map).unwrap().contains("solid"));
        ok(&mut state, "save_map", json!({ "path": "gm_mcp_v2.vmf" }));
        assert!(dir.join("gm_mcp_v2.vmf").exists());
        assert_eq!(state.doc.path(), Some(dir.join("gm_mcp_v2.vmf").as_path()));
        assert!(err(&mut state, "save_map", json!({ "path": "gm_mcp.vmf" })).contains("overwrite"));
        ok(&mut state, "save_map", json!({ "path": "gm_mcp.vmf", "overwrite": true }));
        for bad in ["../evil.vmf", "sub/x.vmf", "x.txt", "C:/elsewhere/x.vmf", "..vmf", "con:x.vmf"] {
            err(&mut state, "save_map", json!({ "path": bad }));
        }
        let r = run(&mut state, "compile_map", &json!({ "preset": "draft" }));
        assert!(matches!(r, Outcome::App(AppOp::Compile { preset: Some(CompilePresetSetting::Draft), launch: false })));
        let r = run(&mut state, "compile_map", &json!({ "launch_game": true }));
        assert!(matches!(r, Outcome::App(AppOp::Compile { preset: None, launch: true })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_labels_are_translated() {
        hp_app::register_translations();
        for t in hp_mcp::tools::EDITOR_TOOLS.iter().filter(|t| {
            matches!(t.kind, hp_mcp::tools::ToolKind::Edit | hp_mcp::tools::ToolKind::Destructive)
                && !matches!(t.name, "undo" | "redo")
        }) {
            let key = hp_ui_shell::menu::undo_key(&undo_label(t.name));
            assert!(hp_app::i18n::has_key(&key), "missing {key}");
        }
        assert!(hp_app::i18n::has_key(&hp_ui_shell::menu::undo_key(&undo_label("run_action"))));
    }
}
