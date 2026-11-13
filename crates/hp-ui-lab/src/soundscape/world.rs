use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::DVec3;
use hp_app::AppState;
use hp_kv::Block;
use hp_soundscape::library::{Library, Origin, manifest_files, map_file_name};
use hp_soundscape::rules::{Emitter, EmitterKind, parse_radius};
use hp_soundscape::{game_sounds::GameSounds, soundlevel};

use crate::media;

pub const MANIFEST: &str = "scripts/soundscapes_manifest.txt";
pub const SOUNDS_MANIFEST: &str = "scripts/game_sounds_manifest.txt";
const MAX_FILE_BYTES: usize = 4 << 20;

pub fn map_name(state: &AppState) -> Option<String> {
    let stem = state.doc.path()?.file_stem()?.to_str()?.to_ascii_lowercase();
    let safe = !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    safe.then_some(stem)
}

pub fn map_rel(state: &AppState) -> Option<String> {
    map_name(state).map(|m| map_file_name(&m))
}

pub fn map_file(state: &AppState, root: Option<&Path>) -> Option<PathBuf> {
    let rel = map_rel(state)?;
    let root = match root {
        Some(r) => r.to_path_buf(),
        None => media::default_root(state)?,
    };
    Some(root.join(rel))
}

pub fn legacy_file(state: &AppState) -> Option<PathBuf> {
    let map = state.doc.path()?;
    let name = map_name(state)?;
    Some(map.with_file_name(format!("soundscapes_{name}.txt")))
}

fn read_text_file(path: &Path) -> Result<Option<String>, String> {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > MAX_FILE_BYTES as u64 => Err(format!("{} is larger than 4 MB", path.display())),
        Ok(_) => std::fs::read(path).map(|b| Some(hp_kv::decode(&b).0.into_owned())).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapFile {
    pub path: Option<PathBuf>,
    pub loaded_from: Option<PathBuf>,
    pub blocks: Vec<Block>,
    pub legacy: bool,
    pub error: Option<String>,
}

pub fn read_map_file(state: &AppState, root: Option<&Path>) -> MapFile {
    let path = map_file(state, root);
    let mut out = MapFile { path: path.clone(), ..MapFile::default() };
    let mut candidates: Vec<(PathBuf, bool)> = Vec::new();
    if let Some(p) = &path {
        candidates.push((p.clone(), false));
    }
    if let Some(l) = legacy_file(state) {
        candidates.push((l, true));
    }
    for (p, legacy) in candidates {
        match read_text_file(&p) {
            Ok(Some(text)) => {
                out.blocks = hp_soundscape::script::parse_file(&text);
                out.loaded_from = Some(p);
                out.legacy = legacy;
                return out;
            }
            Ok(None) => {}
            Err(e) => {
                out.error = Some(e);
                return out;
            }
        }
    }
    if let (Some(rel), Some(fs)) = (map_rel(state), state.assets.fs())
        && let Some(bytes) = fs.read(&rel).filter(|b| b.len() <= MAX_FILE_BYTES)
    {
        out.blocks = hp_soundscape::script::parse_file(&hp_kv::decode(&bytes).0);
    }
    out
}

pub fn current_map_file(state: &AppState) -> MapFile {
    let unsaved = map_rel(state).and_then(|rel| state.runtime.lab.soundscape.unsaved.get(&rel));
    match unsaved {
        Some(text) => MapFile {
            path: map_file(state, None),
            loaded_from: None,
            blocks: hp_soundscape::script::parse_file(text),
            legacy: false,
            error: None,
        },
        None => read_map_file(state, None),
    }
}

pub fn game_texts(state: &AppState) -> Vec<(String, String)> {
    let Some(fs) = state.assets.fs() else { return Vec::new() };
    let read = |rel: &str| fs.read(rel).filter(|b| b.len() <= MAX_FILE_BYTES).map(|b| hp_kv::decode(&b).0.into_owned());
    let Some(manifest) = read(MANIFEST) else { return Vec::new() };
    let own = map_rel(state);
    manifest_files(&manifest)
        .into_iter()
        .filter(|f| own.as_deref() != Some(f.as_str()))
        .filter_map(|f| read(&f).map(|t| (f, t)))
        .collect()
}

pub fn build_library(game: &[(String, String)], map_rel: Option<&str>, map_blocks: &[Block]) -> Library {
    let mut lib = Library::default();
    for (file, text) in game {
        lib.add_file(file, text, Origin::Game, false);
    }
    let rel = map_rel.unwrap_or("scripts/soundscapes_map.txt");
    lib.add_blocks(rel, map_blocks.to_vec(), Origin::Map, true);
    lib
}

pub fn is_game_file(game: &[(String, String)], rel: &str) -> bool {
    game.iter().any(|(f, _)| f.eq_ignore_ascii_case(rel))
}

#[derive(Default)]
pub struct LibraryCache {
    game_key: Option<(u64, Option<String>)>,
    key: Option<(u64, u64, Option<String>)>,
    game: Arc<Vec<(String, String)>>,
    game_lib: Arc<Library>,
    map: MapFile,
    library: Arc<Library>,
}

impl LibraryCache {
    pub fn get(&mut self, state: &AppState) -> Arc<Library> {
        let rel = map_rel(state);
        let generation = state.assets.generation();
        let game_key = (generation, rel.clone());
        if self.game_key.as_ref() != Some(&game_key) {
            self.game = Arc::new(game_texts(state));
            let mut lib = Library::default();
            for (file, text) in self.game.iter() {
                lib.add_file(file, text, Origin::Game, false);
            }
            self.game_lib = Arc::new(lib);
            self.game_key = Some(game_key);
            self.key = None;
        }
        let key = (generation, state.runtime.lab.soundscape.library_serial, rel);
        if self.key.as_ref() != Some(&key) {
            self.map = current_map_file(state);
            let mut lib = (*self.game_lib).clone();
            lib.add_blocks(
                key.2.as_deref().unwrap_or("scripts/soundscapes_map.txt"),
                self.map.blocks.clone(),
                Origin::Map,
                true,
            );
            self.library = Arc::new(lib);
            self.key = Some(key);
        }
        Arc::clone(&self.library)
    }
}

pub fn write_map_file(path: &Path, blocks: &[Block]) -> Result<(), String> {
    let text = hp_soundscape::script::write_file(blocks);
    let dir = path.parent().ok_or("the soundscape file has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("bad file name")?;
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("cannot write {}: {e}", path.display()));
    }
    Ok(())
}

pub fn check_target(state: &AppState, game: &[(String, String)], path: &Path) -> Result<(), String> {
    let rel = map_rel(state).ok_or("save the map first: the soundscape file is named after it")?;
    if is_game_file(game, &rel) {
        return Err(format!("{rel} is a soundscape file of the game: it is never overwritten"));
    }
    let lower = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    if !lower.ends_with(&rel) {
        return Err(format!("{} is not the soundscape file of this map ({rel})", path.display()));
    }
    if manifest_lists(state, &rel) && path.exists() && game_copy(state, &rel).is_some_and(|g| same_file(&g, path)) {
        return Err(format!(
            "{} is the game's own soundscape file for this map: it is never overwritten, pick another folder",
            path.display()
        ));
    }
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn game_copy(state: &AppState, rel: &str) -> Option<PathBuf> {
    state.assets.fs()?.disk_path(rel)
}

pub fn manifest_lists(state: &AppState, rel: &str) -> bool {
    let Some(fs) = state.assets.fs() else { return false };
    let Some(bytes) = fs.read(MANIFEST).filter(|b| b.len() <= MAX_FILE_BYTES) else { return false };
    manifest_files(&hp_kv::decode(&bytes).0).iter().any(|f| f.eq_ignore_ascii_case(rel))
}

pub fn targets(state: &AppState) -> HashMap<String, DVec3> {
    let mut list: Vec<(u32, String, DVec3)> = Vec::new();
    for (id, node) in state.doc.scene().entities() {
        let e = &node.entity;
        if let (Some(name), Some(o)) = (e.get("targetname"), e.origin()) {
            let n = name.trim().to_ascii_lowercase();
            if !n.is_empty() {
                list.push((*id, n, o));
            }
        }
    }
    list.sort_by_key(|(id, _, _)| *id);
    let mut out = HashMap::new();
    for (_, n, o) in list {
        out.entry(n).or_insert(o);
    }
    out
}

fn flag(e: &hp_vmf::Entity, key: &str) -> i64 {
    e.get(key).and_then(|v| v.trim().parse::<f64>().ok()).map_or(0, |v| v as i64)
}

fn position_names(e: &hp_vmf::Entity) -> Vec<(u8, String)> {
    (0..hp_soundscape::script::MAX_POSITIONS)
        .filter_map(|i| {
            e.get(&format!("position{i}")).map(str::trim).filter(|t| !t.is_empty()).map(|t| (i, t.to_owned()))
        })
        .collect()
}

pub fn collect_emitters(state: &AppState) -> Vec<Emitter> {
    let names = targets(state);
    let scene = state.doc.scene();
    let mut ids: Vec<u32> = scene
        .entities()
        .iter()
        .filter(|(_, n)| EmitterKind::from_class(&n.entity.classname).is_some())
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    let first_named = |name: &str| -> Option<u32> {
        let n = name.trim().to_ascii_lowercase();
        let mut best: Option<u32> = None;
        for (id, node) in scene.entities() {
            if node.entity.get("targetname").is_some_and(|t| t.trim().eq_ignore_ascii_case(&n))
                && best.is_none_or(|b| *id < b)
            {
                best = Some(*id);
            }
        }
        best
    };
    let mut out = Vec::new();
    for id in ids {
        let Some(node) = scene.entity(id) else { continue };
        let e = &node.entity;
        let Some(kind) = EmitterKind::from_class(&e.classname) else { continue };
        let origin = e.origin().unwrap_or_default();
        let mut soundscape = e.get("soundscape").unwrap_or("").trim().to_owned();
        let mut source = e;
        let mut main = None;
        if kind == EmitterKind::Proxy {
            let target = e.get("mainsoundscapename").unwrap_or("").trim().to_owned();
            match first_named(&target).and_then(|m| scene.entity(m).map(|n| (m, n))) {
                Some((m, n)) if EmitterKind::from_class(&n.entity.classname).is_some() => {
                    main = Some(m);
                    soundscape = n.entity.get("soundscape").unwrap_or("").trim().to_owned();
                    source = &n.entity;
                }
                _ => soundscape.clear(),
            }
        }
        let positions = position_names(source)
            .into_iter()
            .map(|(i, t)| {
                let at = names.get(&t.to_ascii_lowercase()).map(|p| p.to_array());
                (i, t, at)
            })
            .collect();
        out.push(Emitter {
            id,
            kind,
            origin: origin.to_array(),
            radius: parse_radius(e.get("radius")),
            soundscape,
            enabled: flag(e, "startdisabled") == 0,
            targetname: e.get("targetname").unwrap_or("").trim().to_owned(),
            positions,
            main,
        });
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub id: u32,
    pub soundscape: String,
    pub triggerable: Option<u32>,
    pub enabled: bool,
    pub bounds: hp_geom::Aabb,
    pub brushes: Vec<(hp_geom::Aabb, Vec<hp_geom::Plane>)>,
}

pub const TRIGGER_CLASS: &str = "trigger_soundscape";

pub fn collect_triggers(state: &AppState) -> Vec<Trigger> {
    let scene = state.doc.scene();
    let mut out = Vec::new();
    for (id, node) in scene.entities() {
        let e = &node.entity;
        if !e.classname.eq_ignore_ascii_case(TRIGGER_CLASS) {
            continue;
        }
        let name = e.get("soundscape").unwrap_or("").trim().to_owned();
        let mut first: Option<(u32, bool)> = None;
        for (oid, other) in scene.entities() {
            if !name.is_empty()
                && other.entity.get("targetname").is_some_and(|t| t.trim().eq_ignore_ascii_case(&name))
                && first.is_none_or(|(f, _)| *oid < f)
            {
                let ok = other.entity.classname.eq_ignore_ascii_case("env_soundscape_triggerable");
                first = Some((*oid, ok));
            }
        }
        let mut brushes = Vec::new();
        let mut bounds = hp_geom::Aabb::EMPTY;
        for n in scene.solids().values() {
            if n.owner == hp_editor::Owner::Entity(*id) {
                let planes: Vec<hp_geom::Plane> = n.geom.faces.iter().map(|f| f.plane).collect();
                if !planes.is_empty() {
                    bounds.extend(n.bounds.min);
                    bounds.extend(n.bounds.max);
                    brushes.push((n.bounds, planes));
                }
            }
        }
        out.push(Trigger {
            id: *id,
            soundscape: name,
            triggerable: first.filter(|(_, ok)| *ok).map(|(i, _)| i),
            enabled: flag(e, "startdisabled") == 0,
            bounds,
            brushes,
        });
    }
    out.sort_by_key(|t| t.id);
    out
}

pub fn box_touches(t: &Trigger, min: DVec3, max: DVec3) -> bool {
    let overlap = |b: &hp_geom::Aabb| {
        b.min.x <= max.x
            && b.max.x >= min.x
            && b.min.y <= max.y
            && b.max.y >= min.y
            && b.min.z <= max.z
            && b.max.z >= min.z
    };
    t.enabled
        && overlap(&t.bounds)
        && t.brushes.iter().any(|(b, planes)| {
            overlap(b)
                && planes.iter().all(|p| {
                    let corner = DVec3::new(
                        if p.normal.x > 0.0 { min.x } else { max.x },
                        if p.normal.y > 0.0 { min.y } else { max.y },
                        if p.normal.z > 0.0 { min.z } else { max.z },
                    );
                    p.normal.dot(corner) - p.dist < 0.0
                })
        })
}

pub fn player_box(feet: DVec3, crouched: bool) -> (DVec3, DVec3) {
    let h = if crouched { 36.0 } else { 72.0 };
    (feet + DVec3::new(-16.0, -16.0, 0.0), feet + DVec3::new(16.0, 16.0, h))
}

pub fn touching(triggers: &[Trigger], min: DVec3, max: DVec3) -> Vec<u32> {
    triggers.iter().filter(|t| box_touches(t, min, max)).map(|t| t.id).collect()
}

pub fn trigger_links(triggers: &[Trigger]) -> Vec<hp_soundscape::TriggerLink> {
    triggers.iter().map(|t| hp_soundscape::TriggerLink { trigger: t.id, triggerable: t.triggerable }).collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ambient {
    pub id: u32,
    pub origin: [f64; 3],
    pub radius: f64,
    pub message: String,
    pub volume: f32,
    pub pitch: f32,
    pub soundlevel: f32,
    pub everywhere: bool,
    pub start_silent: bool,
    pub looped: bool,
    pub source: Option<String>,
}

pub const AMBIENT_CLASS: &str = "ambient_generic";

pub fn collect_ambients(state: &AppState) -> Vec<Ambient> {
    let names = targets(state);
    let mut out = Vec::new();
    for (id, node) in state.doc.scene().entities() {
        let e = &node.entity;
        if !e.classname.eq_ignore_ascii_case(AMBIENT_CLASS) {
            continue;
        }
        let flags = flag(e, "spawnflags");
        let everywhere = flags & 1 != 0;
        let radius =
            e.get("radius").and_then(|r| r.trim().parse::<f64>().ok()).filter(|r| r.is_finite()).unwrap_or(1250.0);
        let source = e.get("sourceentityname").map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
        let origin = source
            .as_ref()
            .and_then(|s| names.get(&s.to_ascii_lowercase()))
            .copied()
            .or_else(|| e.origin())
            .unwrap_or_default();
        let health = e.get("health").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(10.0);
        let pitch = e.get("pitch").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(100.0);
        out.push(Ambient {
            id: *id,
            origin: origin.to_array(),
            radius: radius.max(0.0),
            message: e.get("message").unwrap_or("").trim().to_owned(),
            volume: (health / 10.0).clamp(0.0, 1.0),
            pitch: pitch.clamp(1.0, 255.0),
            soundlevel: soundlevel::from_radius(radius as f32, everywhere),
            everywhere,
            start_silent: flags & 16 != 0,
            looped: flags & 32 == 0,
            source,
        });
    }
    out.sort_by_key(|a| a.id);
    out
}

const SIGHT_PASSES: &[&str] =
    &["tools/toolsplayerclip", "tools/toolsnpcclip", "tools/toolsclip", "tools/toolsblocklos", "tools/toolsblocklight"];

pub fn blocks_sight(material: &str) -> bool {
    let m = material.to_ascii_lowercase().replace('\\', "/");
    !hp_ui_viewports::player::non_solid_material(&m) && !SIGHT_PASSES.contains(&m.as_str())
}

fn solid_owner(scene: &hp_editor::Scene, n: &hp_editor::SolidNode) -> bool {
    match n.owner {
        hp_editor::Owner::World => true,
        hp_editor::Owner::Entity(e) => scene.entity(e).is_none_or(|en| {
            let class = en.entity.classname.to_ascii_lowercase();
            let never = class == "func_brush" && en.entity.get("solidity").is_some_and(|v| v.trim() == "1");
            !never && !hp_ui_viewports::player::non_solid_class(&class)
        }),
    }
}

pub fn sight_brush(scene: &hp_editor::Scene, n: &hp_editor::SolidNode) -> bool {
    solid_owner(scene, n) && n.solid.sides.iter().any(|s| blocks_sight(&s.material))
}

pub fn start_solid(state: &AppState, p: DVec3) -> bool {
    let scene = state.doc.scene();
    scene.solids().iter().any(|(_, n)| {
        n.bounds.min.x <= p.x
            && n.bounds.max.x >= p.x
            && n.bounds.min.y <= p.y
            && n.bounds.max.y >= p.y
            && n.bounds.min.z <= p.z
            && n.bounds.max.z >= p.z
            && n.disp.iter().all(Option::is_none)
            && !hp_ui_viewports::player::no_surface_left(n)
            && sight_brush(scene, n)
            && !n.geom.faces.is_empty()
            && n.geom.faces.iter().all(|f| f.plane.normal.dot(p) - f.plane.dist <= START_SOLID_EPSILON)
    })
}

pub const START_SOLID_EPSILON: f64 = 1e-3;
const HIT_EPSILON: f64 = 0.031_25;

fn prop_hit(props: &hp_app::runtime::soundscape::PropHulls, ray: &hp_geom::Ray, max: f64) -> bool {
    props.bodies.iter().any(|b| {
        let aabb = hp_geom::Aabb { min: DVec3::from_array(b.min), max: DVec3::from_array(b.max) };
        hp_geom::ray_aabb(ray, &aabb).is_some_and(|t| t <= max)
            && b.tris.iter().any(|t| {
                hp_geom::ray_triangle(
                    ray,
                    DVec3::from_array(t[0]),
                    DVec3::from_array(t[1]),
                    DVec3::from_array(t[2]),
                    false,
                )
                .is_some_and(|d| d > HIT_EPSILON && d < max)
            })
    })
}

pub fn sight_fn(state: &AppState) -> impl Fn(DVec3, DVec3) -> bool + '_ {
    let props = std::sync::Arc::clone(&state.runtime.lab.soundscape.props);
    move |from: DVec3, to: DVec3| {
        if start_solid(state, from) {
            return false;
        }
        let d = to - from;
        let len = d.length();
        if !len.is_finite() || len < HIT_EPSILON {
            return true;
        }
        let scene = state.doc.scene();
        let solid = |o: hp_editor::ObjRef| match o {
            hp_editor::ObjRef::Solid(id) => {
                scene.solid(id).is_some_and(|n| !hp_ui_viewports::player::no_surface_left(n) && sight_brush(scene, n))
            }
            hp_editor::ObjRef::Entity(_) => false,
        };
        let opts = hp_editor::PickOptions {
            include_entities: false,
            ignore_hidden: false,
            max_distance: len,
            filter: Some(&solid),
            ..hp_editor::PickOptions::default()
        };
        let ray = hp_geom::Ray::new(from, d / len);
        let brush = state
            .doc
            .pick_ray_all(&ray, &opts, &|e| state.entity_bounds(e))
            .iter()
            .any(|h| h.t.is_finite() && h.t > HIT_EPSILON && h.t < len);
        !brush && !prop_hit(&props, &ray, len)
    }
}

pub fn refresh_props(state: &mut AppState) {
    let key = (state.doc.revision(), state.assets.generation());
    let current = &state.runtime.lab.soundscape.props;
    if current.key == Some(key) && current.waiting == 0 {
        return;
    }
    let mut list: Vec<(u32, String, DVec3, DVec3, f64)> = Vec::new();
    for (id, node) in state.doc.scene().entities() {
        let e = &node.entity;
        if !e.classname.eq_ignore_ascii_case("prop_static") || e.get("solid").is_some_and(|v| v.trim() == "0") {
            continue;
        }
        let Some(model) =
            e.get("model").map(|m| m.trim().replace('\\', "/").to_ascii_lowercase()).filter(|m| !m.is_empty())
        else {
            continue;
        };
        let Some(origin) = e.origin().filter(|o| o.is_finite()) else { continue };
        let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
        let scale = e
            .get("modelscale")
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0);
        list.push((*id, model, origin, angles, scale));
    }
    let mut out = hp_app::runtime::soundscape::PropHulls { key: Some(key), ..Default::default() };
    for (id, model, origin, angles, scale) in list {
        match state.assets.collision_triangles(&model) {
            None => out.waiting += 1,
            Some(None) => {}
            Some(Some(tris)) => {
                let m = glam::DMat4::from_scale_rotation_translation(
                    DVec3::splat(scale),
                    hp_geom::angles_to_quat(angles),
                    origin,
                );
                let mut min = DVec3::splat(f64::INFINITY);
                let mut max = DVec3::splat(f64::NEG_INFINITY);
                let tris: Vec<[[f64; 3]; 3]> = tris
                    .iter()
                    .map(|t| {
                        t.map(|v| {
                            let p = m.transform_point3(DVec3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2])));
                            min = min.min(p);
                            max = max.max(p);
                            p.to_array()
                        })
                    })
                    .collect();
                if !tris.is_empty() {
                    out.bodies.push(hp_app::runtime::soundscape::PropHull {
                        id,
                        min: min.to_array(),
                        max: max.to_array(),
                        tris,
                    });
                }
            }
        }
    }
    state.runtime.lab.soundscape.props = std::sync::Arc::new(out);
}

pub fn floor_below(state: &AppState, p: DVec3, reach: f64) -> Option<f64> {
    let scene = state.doc.scene();
    let solid = |o: hp_editor::ObjRef| match o {
        hp_editor::ObjRef::Solid(id) => scene.solid(id).is_some_and(|n| {
            let owner_ok = match n.owner {
                hp_editor::Owner::World => true,
                hp_editor::Owner::Entity(e) => {
                    scene.entity(e).is_none_or(|en| !hp_ui_viewports::player::non_solid_class(&en.entity.classname))
                }
            };
            owner_ok && n.solid.sides.iter().any(|s| !hp_ui_viewports::player::non_solid_material(&s.material))
        }),
        hp_editor::ObjRef::Entity(_) => false,
    };
    let opts = hp_editor::PickOptions {
        include_entities: false,
        ignore_hidden: false,
        max_distance: reach,
        filter: Some(&solid),
        ..hp_editor::PickOptions::default()
    };
    let ray = hp_geom::Ray::new(p, DVec3::NEG_Z);
    state
        .doc
        .pick_ray(&ray, &opts, &|e| state.entity_bounds(e))
        .filter(|h| h.t.is_finite() && h.t <= reach)
        .map(|h| p.z - h.t)
}

pub fn game_sounds(state: &AppState) -> GameSounds {
    let mut g = GameSounds::default();
    let Some(fs) = state.assets.fs() else { return g };
    let read = |rel: &str| fs.read(rel).filter(|b| b.len() <= MAX_FILE_BYTES).map(|b| hp_kv::decode(&b).0.into_owned());
    if let Some(manifest) = read(SOUNDS_MANIFEST) {
        let Ok(root) = hp_kv::parse(&manifest) else { return g };
        for block in &root.children {
            for (k, v) in &block.props {
                if (k.eq_ignore_ascii_case("precache_file") || k.eq_ignore_ascii_case("preload_file"))
                    && let Some(text) = read(&v.trim().replace('\\', "/").to_ascii_lowercase())
                {
                    g.add_file(&text, false);
                }
            }
        }
    }
    if let Some(m) = map_name(state)
        && let Some(text) = read(&format!("maps/{m}_level_sounds.txt"))
    {
        g.add_file(&text, true);
    }
    g
}

pub fn ambient_wave(message: &str, sounds: &GameSounds) -> Option<String> {
    let m = message.trim();
    if m.is_empty() {
        return None;
    }
    if media::is_sound_ext(m) {
        return hp_soundscape::script::wave_path(m);
    }
    sounds.get(m).and_then(|s| s.waves.first()).and_then(|w| hp_soundscape::script::wave_path(w))
}

pub fn color_for(name: &str) -> [f32; 4] {
    if name.trim().is_empty() {
        return [0.6, 0.6, 0.6, 1.0];
    }
    let mut h: u32 = 2_166_136_261;
    for b in name.trim().bytes() {
        h ^= u32::from(b.to_ascii_lowercase());
        h = h.wrapping_mul(16_777_619);
    }
    let hue = (h % 360) as f32 / 60.0;
    let (s, v) = (0.65, 1.0);
    let c = v * s;
    let x = c * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m, 1.0]
}

pub const AMBIENT_COLOR: [f32; 4] = [0.35, 0.85, 1.0, 1.0];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_stable_and_distinct() {
        assert_eq!(color_for("a.b"), color_for("A.B"));
        assert_ne!(color_for("gm_x.cave"), color_for("gm_x.forest"));
        assert_eq!(color_for(""), [0.6, 0.6, 0.6, 1.0]);
        for c in color_for("x")[..3].iter() {
            assert!((0.0..=1.0).contains(c));
        }
    }

    #[test]
    fn game_files_are_detected() {
        let game = vec![("scripts/soundscapes.txt".to_owned(), "\"A\" { \"dsp\" \"1\" }".to_owned())];
        assert!(is_game_file(&game, "Scripts/Soundscapes.txt"));
        let lib = build_library(
            &game,
            Some("scripts/soundscapes_gm_x.txt"),
            &[{
                let mut b = Block::new("gm_x.a");
                b.set("dsp", "1");
                b
            }],
        );
        assert_eq!(lib.len(), 2);
        assert_eq!(lib.get("gm_x.a").map(|e| e.origin), Some(Origin::Map));
    }
}
