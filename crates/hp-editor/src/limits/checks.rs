use std::collections::{HashMap, HashSet};

use glam::DVec3;
use hp_geom::{Aabb, MAX_COORD, PLANE_EPS};

use super::leak::{LeakReport, SEALING_ENTITY_CLASSES};
use super::snapshot::{MapSnapshot, SolidRec};
use crate::duplicates::{
    CODE_BRUSH, CODE_CONTAINED, CODE_ENTITY, ContentClass, OwnerKey, Redundant, SolidInfo, entity_keys, find_redundant,
    is_detail_class,
};
use crate::problems::{MaterialKind, Problem, Severity, parent_name};
use crate::scene::Owner;
use crate::selection::{FaceRef, ObjRef};
use crate::side_index::{MAX_SIDE_LIST, parse_side_list};
use crate::{Id, IdSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolUse {
    Nodraw,
    Sky,
    Clip,
    Trigger,
    Hint,
    Skip,
    AreaPortal,
    Occluder,
    Origin,
    NonSolid,
    Opaque,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MaterialFacts {
    pub missing: bool,
    pub tool: Option<ToolUse>,
    pub water: bool,
    pub translucent: bool,
}

impl MaterialFacts {
    pub fn kind(&self) -> MaterialKind {
        if self.missing {
            MaterialKind::Missing
        } else if self.tool.is_some() {
            MaterialKind::Tool
        } else if self.water {
            MaterialKind::Water
        } else {
            MaterialKind::Regular
        }
    }

    pub fn seals(&self) -> bool {
        match self.tool {
            None => !self.water && !self.translucent,
            Some(t) => matches!(t, ToolUse::Nodraw | ToolUse::Sky | ToolUse::Opaque),
        }
    }

    pub fn transparent(&self) -> bool {
        self.water || self.translucent
    }

    pub fn lightmapped(&self) -> bool {
        self.tool.is_none() && !self.water
    }

    pub fn sky(&self) -> bool {
        self.tool == Some(ToolUse::Sky)
    }

    pub fn guess(name: &str) -> MaterialFacts {
        let m = crate::glob::normalize_material(name).to_ascii_lowercase();
        let tool = m.strip_prefix("tools/").map(|t| match t {
            "toolsnodraw" => ToolUse::Nodraw,
            "toolsclip" | "toolsplayerclip" | "toolsnpcclip" | "toolsgrenadeclip" | "toolsdroneclip" => ToolUse::Clip,
            "toolstrigger" => ToolUse::Trigger,
            "toolshint" => ToolUse::Hint,
            "toolsskip" => ToolUse::Skip,
            "toolsareaportal" => ToolUse::AreaPortal,
            "toolsoccluder" => ToolUse::Occluder,
            "toolsorigin" => ToolUse::Origin,
            "toolsblack" | "toolsblack_noportal" => ToolUse::Opaque,
            t if t.starts_with("toolsskybox") => ToolUse::Sky,
            _ => ToolUse::NonSolid,
        });
        let water = tool.is_none() && (m.starts_with("liquids/") || m.contains("/water") || m.starts_with("water"));
        MaterialFacts { missing: false, tool, water, translucent: false }
    }

    pub fn from_kind(name: &str, kind: MaterialKind) -> MaterialFacts {
        match kind {
            MaterialKind::Missing => MaterialFacts { missing: true, ..MaterialFacts::default() },
            MaterialKind::Tool => {
                let mut f = MaterialFacts::guess(name);
                if f.tool.is_none() {
                    f.tool = Some(ToolUse::NonSolid);
                }
                f.water = false;
                f
            }
            MaterialKind::Water => MaterialFacts { water: true, ..MaterialFacts::default() },
            MaterialKind::Regular => MaterialFacts::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKey {
    Model,
    Sound,
    Particle,
    Material,
}

pub trait CheckKnowledge {
    fn class_exists(&self, _class: &str) -> Option<bool> {
        None
    }
    fn is_brush_class(&self, _class: &str) -> Option<bool> {
        None
    }
    fn is_target_key(&self, _class: &str, _key: &str) -> Option<bool> {
        None
    }
    fn class_has_input(&self, _class: &str, _input: &str) -> Option<bool> {
        None
    }
    fn asset_key(&self, _class: &str, _key: &str) -> Option<AssetKey> {
        None
    }
    fn material(&self, _name: &str) -> Option<MaterialFacts> {
        None
    }
    fn model_exists(&self, _path: &str) -> Option<bool> {
        None
    }
    fn sound_exists(&self, _name: &str) -> Option<bool> {
        None
    }
    fn particle_exists(&self, _name: &str) -> Option<bool> {
        None
    }
    fn extra_target_names(&self) -> &[String] {
        &[]
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoKnowledge;

impl CheckKnowledge for NoKnowledge {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckOptions {
    pub max_switchable_lights: usize,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self { max_switchable_lights: hp_bsp::limits::SWITCHED_LIGHTS.max_usize() }
    }
}

const COMMON_TARGET_KEYS: &[&str] = &["target", "parentname", "lasertarget"];

pub(crate) const OVERLAY_CLASSES: &[&str] = &["info_overlay", "info_overlay_transition"];

pub const MAX_OVERLAY_FACES: usize = hp_bsp::limits::OVERLAY_FACES.max_usize();

pub const MAX_BRUSH_SIDES: usize = 128;

pub const MAX_KEY_LEN: usize = 31;
pub const MAX_VALUE_LEN: usize = 1023;

pub const MICRO_VOLUME: f64 = 1.0;

fn has_side_list(class: &str) -> bool {
    OVERLAY_CLASSES.iter().any(|c| class.eq_ignore_ascii_case(c)) || class.eq_ignore_ascii_case("env_cubemap")
}

pub(crate) const PLAYER_SPAWNS: &[&str] = &[
    "info_player_start",
    "info_player_deathmatch",
    "info_player_combine",
    "info_player_rebel",
    "info_player_counterterrorist",
    "info_player_terrorist",
    "info_player_teamspawn",
    "info_player_axis",
    "info_player_allies",
    "gmod_player_start",
    "info_player_coop",
    "info_survivor_position",
];

const POINT_EXCEPTIONS: &[&str] = &[
    "func_instance",
    "func_instance_parms",
    "func_instance_origin",
    "func_instance_io_proxy",
    "func_useableladder",
    "func_ladderendpoint",
    "func_fish_pool",
    "func_orator",
    "func_portal_orientation",
    "trigger_relay",
    "trigger_auto",
];

pub(crate) fn guess_brush_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    (c.starts_with("func_") || c.starts_with("trigger_")) && !POINT_EXCEPTIONS.contains(&c.as_str())
}

const ENTITY_TOOL_MATERIALS: &[&str] = &["tools/toolstrigger", "tools/toolsareaportal", "tools/toolsoccluder"];

const MODEL_CLASSES: &[&str] = &[
    "prop_static",
    "prop_detail",
    "prop_dynamic",
    "prop_dynamic_override",
    "prop_dynamic_ornament",
    "prop_physics",
    "prop_physics_override",
    "prop_physics_multiplayer",
    "prop_physics_respawnable",
    "prop_ragdoll",
    "prop_door_rotating",
];

const SINGLETONS: &[&str] =
    &["light_environment", "shadow_control", "env_tonemap_controller", "water_lod_control", "env_skypaint"];

const UNIVERSAL_INPUTS: &[&str] = &[
    "kill",
    "killhierarchy",
    "addoutput",
    "fireuser1",
    "fireuser2",
    "fireuser3",
    "fireuser4",
    "use",
    "setparent",
    "setparentattachment",
    "setparentattachmentmaintainoffset",
    "clearparent",
    "dispatcheffect",
    "dispatchresponse",
    "addcontext",
    "removecontext",
    "clearcontext",
    "alpha",
    "alternativesorting",
    "color",
    "setteam",
    "setdamagefilter",
    "enabledamageforces",
    "disabledamageforces",
    "enableshadow",
    "disableshadow",
    "runscriptfile",
    "runscriptcode",
    "callscriptfunction",
    "setlocalorigin",
    "setlocalangles",
];

fn origin_must_be_open(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    matches!(c.as_str(), "light" | "light_spot" | "light_dynamic" | "env_cubemap" | "gmod_player_start" | "info_node")
        || c.starts_with("info_player_")
        || c.starts_with("npc_")
        || c.starts_with("item_")
        || c.starts_with("weapon_")
}

fn is_door_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.starts_with("func_door") || c == "prop_door_rotating" || c == "func_movelinear" || c == "func_brush"
}

fn fallback_asset_key(class: &str, key: &str) -> Option<AssetKey> {
    let (c, k) = (class.to_ascii_lowercase(), key.to_ascii_lowercase());
    match k.as_str() {
        "model" if c.starts_with("prop_") || c.starts_with("npc_") || c == "cycler" => Some(AssetKey::Model),
        "message" if c == "ambient_generic" => Some(AssetKey::Sound),
        "effect_name" if c == "info_particle_system" => Some(AssetKey::Particle),
        _ => None,
    }
}

fn is_special_target(target: &str) -> bool {
    target.starts_with('!') || target.is_empty()
}

pub(crate) fn target_exists(names: &HashSet<String>, target: &str) -> bool {
    let t = target.trim();
    if is_special_target(t) {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    if let Some(prefix) = lower.strip_suffix('*') {
        return names.iter().any(|n| n.starts_with(prefix));
    }
    names.contains(&lower)
}

fn is_model_path(model: &str) -> bool {
    let lower = model.trim().replace('\\', "/").to_ascii_lowercase();
    lower.len() > "models/.mdl".len()
        && lower.starts_with("models/")
        && lower.ends_with(".mdl")
        && !lower.contains(':')
        && !lower.split('/').any(|part| part == "..")
}

fn has_mdl_extension(model: &str) -> bool {
    model.len() > 4 && model.get(model.len() - 4..).is_some_and(|x| x.eq_ignore_ascii_case(".mdl"))
}

pub fn is_bad_content_path(path: &str) -> bool {
    let p = path.trim();
    !p.is_empty() && p.chars().any(|c| c == ' ' || !c.is_ascii() || c.is_ascii_control())
}

fn faces_objects(faces: &[FaceRef]) -> Vec<ObjRef> {
    let mut seen = HashSet::new();
    faces.iter().filter(|f| seen.insert(f.solid)).map(|f| ObjRef::Solid(f.solid)).collect()
}

fn problem(severity: Severity, code: &'static str, message: String, objects: Vec<ObjRef>, fixable: bool) -> Problem {
    Problem::new(severity, code, message, objects, fixable)
}

fn face_problem(
    severity: Severity,
    code: &'static str,
    message: String,
    faces: Vec<FaceRef>,
    fixable: bool,
) -> Problem {
    Problem { severity, code, message, objects: faces_objects(&faces), faces, fixable, args: Vec::new() }
}

pub(crate) struct Materials<'a> {
    snap: &'a MapSnapshot,
    k: &'a dyn CheckKnowledge,
    cache: Vec<Option<Option<MaterialFacts>>>,
}

impl<'a> Materials<'a> {
    pub(crate) fn new(snap: &'a MapSnapshot, k: &'a dyn CheckKnowledge) -> Self {
        Self { snap, k, cache: vec![None; snap.materials.len()] }
    }

    pub(crate) fn known(&mut self, id: u32) -> Option<MaterialFacts> {
        let i = id as usize;
        if i >= self.cache.len() {
            return None;
        }
        if let Some(c) = self.cache[i] {
            return c;
        }
        let f = self.k.material(self.snap.material_display(id));
        self.cache[i] = Some(f);
        f
    }

    pub(crate) fn facts(&mut self, id: u32) -> MaterialFacts {
        self.known(id).unwrap_or_else(|| MaterialFacts::guess(self.snap.material_key(id)))
    }
}

fn target_names(snap: &MapSnapshot, k: &dyn CheckKnowledge) -> HashSet<String> {
    let extra = k.extra_target_names();
    let mut names = HashSet::with_capacity(snap.entities.len() * 2 + extra.len());
    for r in &snap.entities {
        if let Some(t) = r.entity.get("targetname") {
            names.insert(t.to_ascii_lowercase());
        }
        names.insert(r.entity.classname.to_ascii_lowercase());
    }
    names.extend(extra.iter().map(|n| n.trim().to_ascii_lowercase()));
    names
}

pub fn check_snapshot(snap: &MapSnapshot, k: &dyn CheckKnowledge, opts: &CheckOptions) -> Vec<Problem> {
    let mut out = Vec::new();
    if snap.is_empty() {
        out.push(problem(Severity::Info, "empty_map", "The map is empty".to_owned(), Vec::new(), false));
        return out;
    }
    let mut mats = Materials::new(snap, k);
    let names = target_names(snap, k);
    check_solids(snap, &mut mats, &mut out);
    check_ids(snap, &mut out);
    check_classes(snap, k, &names, &mut out);
    check_outputs(snap, k, &names, &mut out);
    check_placement(snap, &mut mats, &mut out);
    check_editor_refs(snap, &mut out);
    check_target_keys(snap, k, &names, &mut out);
    check_side_lists(snap, &mut out);
    check_assets(snap, k, &mut mats, &mut out);
    check_entity_rules(snap, k, opts, &mut mats, &mut out);
    check_sky_cameras(snap, &mut out);
    check_redundant(snap, &mut mats, &mut out);
    out.sort_by_key(|p| std::cmp::Reverse(p.severity));
    out
}

fn content_class(
    snap: &MapSnapshot,
    s: &SolidRec,
    facts: &mut dyn FnMut(u32) -> MaterialFacts,
) -> Option<ContentClass> {
    if s.has_disp() || !s.geom.valid {
        return None;
    }
    let all: Vec<MaterialFacts> = s.sides.iter().map(|x| facts(x.material)).collect();
    if all.iter().all(|f| f.seals() && !f.transparent()) {
        return Some(ContentClass::Solid);
    }
    let first = s.sides.first()?.material;
    let same = s.sides.iter().all(|x| snap.material_key(x.material) == snap.material_key(first));
    (same && all.iter().all(|f| f.tool == Some(ToolUse::Clip))).then_some(ContentClass::Tool(first))
}

pub fn redundant_in(snap: &MapSnapshot, facts: &mut dyn FnMut(u32) -> MaterialFacts) -> Redundant {
    let details: HashSet<Id> =
        snap.entities.iter().filter(|r| is_detail_class(&r.entity.classname)).map(|r| r.entity.id).collect();
    let solids: Vec<SolidInfo<'_>> = snap
        .solids
        .iter()
        .map(|s| SolidInfo {
            id: s.id,
            owner: OwnerKey::of(s.owner, |e| details.contains(&e)),
            shape: s.shape,
            bounds: s.bounds,
            geom: &s.geom,
            class: content_class(snap, s, facts),
            volume: s.volume,
        })
        .collect();
    let mut shapes: HashMap<Id, Vec<u64>> = HashMap::new();
    for s in &snap.solids {
        if let Owner::Entity(e) = s.owner {
            shapes.entry(e).or_default().push(s.shape);
        }
    }
    let entities = entity_keys(
        snap.entities.iter().map(|r| (r.entity.id, &r.entity, shapes.get(&r.entity.id).map_or(&[][..], Vec::as_slice))),
    );
    find_redundant(&solids, &entities)
}

fn pairs(groups: &[(Id, Vec<Id>)]) -> Vec<String> {
    groups.iter().flat_map(|(keep, copies)| copies.iter().map(move |c| format!("{c}={keep}"))).collect()
}

fn check_redundant(snap: &MapSnapshot, mats: &mut Materials<'_>, out: &mut Vec<Problem>) {
    let found = redundant_in(snap, &mut |m| mats.facts(m));
    let brushes = found.objects(CODE_BRUSH);
    if !brushes.is_empty() {
        out.push(
            problem(
                Severity::Warning,
                CODE_BRUSH,
                format!("{} brushes are exact copies of another brush (same shape, same entity)", brushes.len()),
                brushes,
                true,
            )
            .with_args(pairs(&found.brushes)),
        );
    }
    let inner = found.objects(CODE_CONTAINED);
    if !inner.is_empty() {
        out.push(
            problem(
                Severity::Info,
                CODE_CONTAINED,
                format!("{} brushes are fully inside another solid brush of the same entity", inner.len()),
                inner,
                true,
            )
            .with_args(found.contained.iter().map(|(outer, inner)| format!("{inner}={outer}")).collect()),
        );
    }
    let ents = found.objects(CODE_ENTITY);
    if !ents.is_empty() {
        out.push(
            problem(
                Severity::Warning,
                CODE_ENTITY,
                format!(
                    "{} entities are stacked copies of another entity (same class, model, angles and origin)",
                    ents.len()
                ),
                ents,
                true,
            )
            .with_args(pairs(&found.entities)),
        );
    }
}

fn check_sky_cameras(snap: &MapSnapshot, out: &mut Vec<Problem>) {
    let cams: Vec<&hp_vmf::Entity> = snap
        .entities
        .iter()
        .map(|r| &r.entity)
        .filter(|e| crate::sky::is_sky_camera(e) && !snap.hidden_entities.contains(&e.id))
        .collect();
    for e in &cams {
        let scale = e.get("scale").unwrap_or_default().trim();
        if crate::sky::parse_sky_scale(scale).is_none() {
            out.push(
                problem(
                    Severity::Error,
                    "sky_camera_bad_scale",
                    format!("sky_camera {}: scale \"{scale}\" is not a whole number of at least 1", e.id),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), scale.chars().take(32).collect()]),
            );
        }
    }
    let mut per_player: Vec<ObjRef> = Vec::new();
    for r in &snap.entities {
        let e = &r.entity;
        if !crate::sky::is_skybox_switch(e) || snap.hidden_entities.contains(&e.id) {
            continue;
        }
        if e.get("mode").and_then(|m| m.trim().parse::<i32>().ok()) == Some(1) {
            per_player.push(ObjRef::Entity(e.id));
        }
        let target = crate::sky::switch_target(e);
        let found = !target.is_empty()
            && cams.iter().any(|c| c.get("targetname").is_some_and(|n| n.trim().eq_ignore_ascii_case(target)));
        if !found {
            out.push(
                problem(
                    Severity::Error,
                    "skybox_switch_bad_target",
                    format!("Skybox switch {} names no sky_camera ('{target}')", e.id),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), target.chars().take(64).collect()]),
            );
        }
    }
    if !per_player.is_empty() {
        let n = per_player.len();
        out.push(problem(
            Severity::Info,
            "skybox_switch_per_player",
            format!(
                "{n} per player skybox switch(es): drawn on each client; an addon that also draws the sky can conflict"
            ),
            per_player,
            false,
        ));
    }
    if let [.., game] = cams.as_slice()
        && cams.len() > 1
    {
        let objects: Vec<ObjRef> = std::iter::once(game.id)
            .chain(cams.iter().map(|e| e.id).filter(|id| *id != game.id))
            .map(ObjRef::Entity)
            .collect();
        out.push(
            problem(
                Severity::Info,
                "sky_camera_multiple",
                format!(
                    "{} sky_camera entities: the game shows the 3D skybox of the last one in the map ({})",
                    cams.len(),
                    game.id
                ),
                objects,
                false,
            )
            .with_args(vec![cams.len().to_string(), game.id.to_string()]),
        );
    }
}

fn check_solids(snap: &MapSnapshot, mats: &mut Materials<'_>, out: &mut Vec<Problem>) {
    for s in &snap.solids {
        if let Some(reason) = &s.invalid_reason {
            out.push(problem(
                Severity::Error,
                "invalid_solid",
                format!("Solid {} is invalid: {reason}", s.id),
                vec![ObjRef::Solid(s.id)],
                true,
            ));
        }
    }
    let faces_of = |pick: fn(&SolidRec) -> &[u16]| -> Vec<FaceRef> {
        snap.solids.iter().flat_map(|s| pick(s).iter().map(move |&i| FaceRef::new(s.id, i as usize))).collect()
    };
    let bad_disp = faces_of(|s| &s.disp_bad);
    if !bad_disp.is_empty() {
        out.push(face_problem(
            Severity::Error,
            "disp_not_quad",
            format!(
                "{} displacements cannot be built: their face does not have exactly 4 corners or their power is not 1 to 4",
                bad_disp.len()
            ),
            bad_disp,
            true,
        ));
    }
    let mut disp_ents: Vec<(Id, Vec<ObjRef>)> = Vec::new();
    for s in snap.solids.iter().filter(|s| s.has_disp()) {
        if let Owner::Entity(e) = s.owner {
            match disp_ents.iter_mut().find(|(id, _)| *id == e) {
                Some((_, v)) => v.push(ObjRef::Solid(s.id)),
                None => disp_ents.push((e, vec![ObjRef::Solid(s.id)])),
            }
        }
    }
    for (e, solids) in disp_ents {
        let class = snap.entities.iter().find(|r| r.entity.id == e).map_or("", |r| r.entity.classname.as_str());
        out.push(
            problem(
                Severity::Error,
                "disp_on_brush_entity",
                format!("Entity {e} ({class}) has displacements: VBSP does not support them on brush entities"),
                solids,
                true,
            )
            .with_args(vec![e.to_string(), class.to_owned()]),
        );
    }
    let many: Vec<ObjRef> =
        snap.solids.iter().filter(|s| s.sides.len() > MAX_BRUSH_SIDES).map(|s| ObjRef::Solid(s.id)).collect();
    if !many.is_empty() {
        out.push(problem(
            Severity::Error,
            "too_many_sides",
            format!("{} solids have more than {MAX_BRUSH_SIDES} sides", many.len()),
            many,
            false,
        ));
    }
    let micro: Vec<ObjRef> =
        snap.solids.iter().filter(|s| s.geom.valid && s.volume < MICRO_VOLUME).map(|s| ObjRef::Solid(s.id)).collect();
    if !micro.is_empty() {
        out.push(problem(
            Severity::Warning,
            "micro_brush",
            format!("{} solids are smaller than {MICRO_VOLUME} cubic unit (VBSP removes them)", micro.len()),
            micro,
            true,
        ));
    }
    for tool in ENTITY_TOOL_MATERIALS {
        let objs: Vec<ObjRef> = snap
            .solids
            .iter()
            .filter(|s| s.owner == Owner::World)
            .filter(|s| s.sides.iter().any(|x| snap.material_key(x.material) == *tool))
            .map(|s| ObjRef::Solid(s.id))
            .collect();
        if !objs.is_empty() {
            out.push(problem(
                Severity::Warning,
                "tool_texture_on_world",
                format!("{} world solids use {tool}, which only works on brush entities", objs.len()),
                objs,
                false,
            ));
        }
    }
    let origin: Vec<ObjRef> = snap
        .solids
        .iter()
        .filter(|s| s.owner == Owner::World)
        .filter(|s| s.sides.iter().any(|x| mats.facts(x.material).tool == Some(ToolUse::Origin)))
        .map(|s| ObjRef::Solid(s.id))
        .collect();
    if !origin.is_empty() {
        out.push(problem(
            Severity::Error,
            "origin_brush_in_world",
            format!("{} world solids are origin brushes, which only work in brush entities", origin.len()),
            origin,
            true,
        ));
    }
    let dups = faces_of(|s| &s.dup_planes);
    if !dups.is_empty() {
        out.push(face_problem(
            Severity::Warning,
            "duplicate_plane",
            format!("{} faces lie on the same plane as another face of their solid", dups.len()),
            dups,
            true,
        ));
    }
    let axes = faces_of(|s| &s.bad_axes);
    if !axes.is_empty() {
        out.push(face_problem(
            Severity::Warning,
            "invalid_texture_axes",
            format!("{} faces have texture axes parallel to the face normal", axes.len()),
            axes,
            true,
        ));
    }
}

fn check_ids(snap: &MapSnapshot, out: &mut Vec<Problem>) {
    let (renumbered, count) = &snap.renumbered;
    if *count > 0 {
        out.push(problem(
            Severity::Info,
            "renumbered_ids",
            format!("{count} objects had duplicate ids and were renumbered when the map was loaded"),
            renumbered.clone(),
            false,
        ));
    }
    let mut seen: HashSet<Id> = HashSet::with_capacity(snap.solids.len() + snap.entities.len() + 1);
    seen.insert(snap.world.id);
    let mut dup_objects = Vec::new();
    for s in &snap.solids {
        if !seen.insert(s.id) {
            dup_objects.push(ObjRef::Solid(s.id));
        }
    }
    for e in &snap.entities {
        if !seen.insert(e.entity.id) {
            dup_objects.push(ObjRef::Entity(e.entity.id));
        }
    }
    let group_dups = snap.groups.iter().filter(|g| seen.contains(*g)).count();
    if !dup_objects.is_empty() || group_dups > 0 {
        out.push(problem(
            Severity::Warning,
            "duplicate_object_id",
            format!("{} objects share their id with another object", dup_objects.len() + group_dups),
            dup_objects,
            true,
        ));
    }
    let mut side_ids: HashSet<Id> = HashSet::new();
    let mut dup_faces = Vec::new();
    for s in &snap.solids {
        for (i, x) in s.sides.iter().enumerate() {
            if !side_ids.insert(x.id) {
                dup_faces.push(FaceRef::new(s.id, i));
            }
        }
    }
    if !dup_faces.is_empty() {
        out.push(face_problem(
            Severity::Warning,
            "duplicate_side_id",
            format!("{} faces share their id with another face", dup_faces.len()),
            dup_faces,
            true,
        ));
    }
}

fn check_classes(snap: &MapSnapshot, k: &dyn CheckKnowledge, _names: &HashSet<String>, out: &mut Vec<Problem>) {
    let mut by_class: Vec<(String, Vec<ObjRef>)> = Vec::new();
    for r in &snap.entities {
        let class = &r.entity.classname;
        if k.class_exists(class) != Some(false) || PLAYER_SPAWNS.iter().any(|c| class.eq_ignore_ascii_case(c)) {
            continue;
        }
        match by_class.iter_mut().find(|(c, _)| c.eq_ignore_ascii_case(class)) {
            Some((_, v)) => v.push(ObjRef::Entity(r.entity.id)),
            None => by_class.push((class.clone(), vec![ObjRef::Entity(r.entity.id)])),
        }
    }
    for (class, objs) in by_class {
        out.push(problem(
            Severity::Warning,
            "unknown_class",
            format!("Unknown entity class '{class}' ({} entities)", objs.len()),
            objs,
            false,
        ));
    }
    for (r, &solids) in snap.entities.iter().zip(&snap.entity_solids) {
        let e = &r.entity;
        let brush = k.is_brush_class(&e.classname).unwrap_or_else(|| guess_brush_class(&e.classname));
        if brush && solids == 0 {
            out.push(problem(
                Severity::Error,
                "brush_entity_without_solids",
                format!("Brush entity {} ({}) has no solids", e.id, e.classname),
                vec![ObjRef::Entity(e.id)],
                true,
            ));
        }
    }
    let has_spawn =
        snap.entities.iter().any(|r| PLAYER_SPAWNS.iter().any(|c| r.entity.classname.eq_ignore_ascii_case(c)));
    if !has_spawn {
        out.push(problem(
            Severity::Warning,
            "no_player_spawn",
            "The map has no player spawn (info_player_start)".to_owned(),
            Vec::new(),
            false,
        ));
    }
    for class in SINGLETONS {
        let objs: Vec<ObjRef> = snap
            .entities
            .iter()
            .filter(|r| r.entity.classname.eq_ignore_ascii_case(class))
            .map(|r| ObjRef::Entity(r.entity.id))
            .collect();
        if objs.len() > 1 {
            out.push(
                problem(
                    Severity::Warning,
                    "duplicate_singleton",
                    format!("{} {class} entities: the engine uses only one", objs.len()),
                    objs,
                    false,
                )
                .with_args(vec![(*class).to_owned()]),
            );
        }
    }
}

fn missing_target_args(targets: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = targets.iter().map(|t| t.trim().to_ascii_lowercase()).collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn check_outputs(snap: &MapSnapshot, k: &dyn CheckKnowledge, names: &HashSet<String>, out: &mut Vec<Problem>) {
    let world = &snap.world;
    let world_missing: Vec<&str> =
        world.connections.iter().filter(|c| !target_exists(names, &c.target)).map(|c| c.target.as_str()).collect();
    if !world_missing.is_empty() {
        out.push(
            problem(
                Severity::Warning,
                "missing_output_target",
                format!("The world has {} outputs targeting entities that do not exist", world_missing.len()),
                vec![ObjRef::Entity(world.id)],
                true,
            )
            .with_args(missing_target_args(&world_missing)),
        );
    }
    let mut classes_of: HashMap<String, Vec<&str>> = HashMap::new();
    for r in &snap.entities {
        let class = r.entity.classname.as_str();
        if let Some(t) = r.entity.get("targetname") {
            classes_of.entry(t.to_ascii_lowercase()).or_default().push(class);
        }
        classes_of.entry(r.entity.classname.to_ascii_lowercase()).or_default().push(class);
    }
    for r in &snap.entities {
        let e = &r.entity;
        let targets: Vec<&str> =
            e.connections.iter().filter(|c| !target_exists(names, &c.target)).map(|c| c.target.as_str()).collect();
        if !targets.is_empty() {
            out.push(
                problem(
                    Severity::Warning,
                    "missing_output_target",
                    format!(
                        "Entity {} ({}) has {} outputs targeting entities that do not exist: {}",
                        e.id,
                        e.classname,
                        targets.len(),
                        targets.join(", ")
                    ),
                    vec![ObjRef::Entity(e.id)],
                    true,
                )
                .with_args(missing_target_args(&targets)),
            );
        }
        let mut unknown: Vec<String> = Vec::new();
        for c in &e.connections {
            let target = c.target.trim();
            let input = c.input.trim();
            if is_special_target(target)
                || target.ends_with('*')
                || input.is_empty()
                || UNIVERSAL_INPUTS.iter().any(|u| input.eq_ignore_ascii_case(u))
            {
                continue;
            }
            let Some(classes) = classes_of.get(&target.to_ascii_lowercase()) else { continue };
            let answers: Vec<Option<bool>> = classes.iter().map(|cl| k.class_has_input(cl, input)).collect();
            if !answers.is_empty() && answers.iter().all(|a| *a == Some(false)) {
                let text = format!("{target}.{input}");
                if !unknown.contains(&text) {
                    unknown.push(text);
                }
            }
        }
        if !unknown.is_empty() {
            out.push(
                problem(
                    Severity::Warning,
                    "unknown_input",
                    format!(
                        "Entity {} ({}) fires inputs its targets do not have: {}",
                        e.id,
                        e.classname,
                        unknown.join(", ")
                    ),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), e.classname.clone(), unknown.join(", ")]),
            );
        }
    }
}

fn check_placement(snap: &MapSnapshot, mats: &mut Materials<'_>, out: &mut Vec<Problem>) {
    let mut world_bounds = Aabb::EMPTY;
    for s in &snap.solids {
        if s.owner == Owner::World && s.geom.valid {
            world_bounds = world_bounds.union(&s.bounds);
        }
    }
    let limit = Aabb::new(DVec3::splat(-MAX_COORD), DVec3::splat(MAX_COORD));
    let mut out_of_bounds = Vec::new();
    let mut outside_map = Vec::new();
    for (r, &solids) in snap.entities.iter().zip(&snap.entity_solids) {
        if solids > 0 {
            continue;
        }
        let Some(o) = r.entity.origin() else { continue };
        if !limit.contains_point(o) {
            out_of_bounds.push(ObjRef::Entity(r.entity.id));
        } else if !world_bounds.is_empty() && !world_bounds.expand(1.0).contains_point(o) {
            outside_map.push(ObjRef::Entity(r.entity.id));
        }
    }
    let solid_limit = limit.expand(PLANE_EPS);
    let solids_out: Vec<ObjRef> = snap
        .solids
        .iter()
        .filter(|s| s.geom.valid && !s.bounds.is_empty() && !solid_limit.contains_aabb(&s.bounds))
        .map(|s| ObjRef::Solid(s.id))
        .collect();
    if !solids_out.is_empty() {
        out.push(problem(
            Severity::Error,
            "solid_out_of_bounds",
            format!("{} solids are outside the maximum map size (+/-{MAX_COORD} units)", solids_out.len()),
            solids_out,
            false,
        ));
    }
    if !out_of_bounds.is_empty() {
        out.push(problem(
            Severity::Error,
            "entity_out_of_bounds",
            format!("{} entities are outside the maximum map size (+/-{MAX_COORD} units)", out_of_bounds.len()),
            out_of_bounds,
            false,
        ));
    }
    if !outside_map.is_empty() {
        out.push(problem(
            Severity::Warning,
            "entity_outside_map",
            format!("{} entities are outside the world brushes and may cause leaks", outside_map.len()),
            outside_map,
            false,
        ));
    }
    let points: Vec<(Id, DVec3)> = snap
        .entities
        .iter()
        .zip(&snap.entity_solids)
        .filter(|(r, n)| **n == 0 && origin_must_be_open(&r.entity.classname))
        .filter_map(|(r, _)| Some((r.entity.id, r.entity.origin()?)))
        .collect();
    if points.is_empty() {
        return;
    }
    const CELL: f64 = 512.0;
    let cell_of = |p: DVec3| ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64, (p.z / CELL).floor() as i64);
    let mut wanted: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, (_, p)) in points.iter().enumerate() {
        wanted.entry(cell_of(*p)).or_default().push(i);
    }
    let mut inside = vec![false; points.len()];
    for s in &snap.solids {
        if s.owner != Owner::World || !s.geom.valid || s.has_disp() {
            continue;
        }
        let (a, b) = (cell_of(s.bounds.min), cell_of(s.bounds.max));
        let cells = (b.0 - a.0 + 1) * (b.1 - a.1 + 1) * (b.2 - a.2 + 1);
        let candidates: Vec<usize> = if cells > 64 {
            (0..points.len()).filter(|&i| s.bounds.contains_point(points[i].1)).collect()
        } else {
            let mut v = Vec::new();
            for x in a.0..=b.0 {
                for y in a.1..=b.1 {
                    for z in a.2..=b.2 {
                        if let Some(list) = wanted.get(&(x, y, z)) {
                            v.extend(list.iter().copied());
                        }
                    }
                }
            }
            v
        };
        if candidates.is_empty() {
            continue;
        }
        if !super::leak::sides_seal(s.sides.iter().map(|x| mats.facts(x.material))) {
            continue;
        }
        for i in candidates {
            if !inside[i] && s.geom.faces.iter().all(|f| f.plane.distance(points[i].1) < -0.5) {
                inside[i] = true;
            }
        }
    }
    let objs: Vec<ObjRef> =
        points.iter().zip(&inside).filter(|(_, in_)| **in_).map(|((id, _), _)| ObjRef::Entity(*id)).collect();
    if !objs.is_empty() {
        out.push(problem(
            Severity::Warning,
            "entity_in_solid",
            format!("{} entities are inside solid brushes", objs.len()),
            objs,
            false,
        ));
    }
}

fn check_editor_refs(snap: &MapSnapshot, out: &mut Vec<Problem>) {
    let mut missing_group = Vec::new();
    let mut missing_vis = Vec::new();
    for s in &snap.solids {
        if s.group.is_some_and(|g| !snap.groups.contains(&g)) {
            missing_group.push(ObjRef::Solid(s.id));
        }
        if !s.visgroups.iter().all(|v| snap.visgroups.contains(v)) {
            missing_vis.push(ObjRef::Solid(s.id));
        }
    }
    for r in &snap.entities {
        let e = &r.entity.editor;
        if e.group_id.is_some_and(|g| !snap.groups.contains(&g)) {
            missing_group.push(ObjRef::Entity(r.entity.id));
        }
        if !e.visgroup_ids.iter().all(|v| snap.visgroups.contains(v)) {
            missing_vis.push(ObjRef::Entity(r.entity.id));
        }
    }
    if !missing_group.is_empty() {
        out.push(problem(
            Severity::Warning,
            "missing_group",
            format!("{} objects belong to a group that does not exist", missing_group.len()),
            missing_group,
            true,
        ));
    }
    if !missing_vis.is_empty() {
        out.push(problem(
            Severity::Warning,
            "missing_visgroup",
            format!("{} objects belong to a visgroup that does not exist", missing_vis.len()),
            missing_vis,
            true,
        ));
    }
}

fn check_target_keys(snap: &MapSnapshot, k: &dyn CheckKnowledge, names: &HashSet<String>, out: &mut Vec<Problem>) {
    for r in &snap.entities {
        let e = &r.entity;
        let mut bad: Vec<String> = Vec::new();
        let mut keys: Vec<String> = Vec::new();
        for (key, v) in &e.props {
            if v.trim().is_empty() {
                continue;
            }
            if crate::sky::is_skybox_switch(e) && key.eq_ignore_ascii_case("skycamera") {
                continue;
            }
            let is_target = k
                .is_target_key(&e.classname, key)
                .unwrap_or_else(|| COMMON_TARGET_KEYS.iter().any(|c| key.eq_ignore_ascii_case(c)));
            let target = if key.eq_ignore_ascii_case("parentname") { parent_name(v) } else { v.as_str() };
            if is_target && !target_exists(names, target) {
                bad.push(format!("{key} = '{v}'"));
                keys.push(format!("{key}={v}"));
            }
        }
        if !bad.is_empty() {
            out.push(
                problem(
                    Severity::Warning,
                    "missing_target",
                    format!("Entity {} ({}) names entities that do not exist: {}", e.id, e.classname, bad.join(", ")),
                    vec![ObjRef::Entity(e.id)],
                    true,
                )
                .with_args(keys),
            );
        }
    }
}

fn check_side_lists(snap: &MapSnapshot, out: &mut Vec<Problem>) {
    let mut sides: Option<HashSet<Id>> = None;
    for r in &snap.entities {
        let e = &r.entity;
        if !has_side_list(&e.classname) {
            continue;
        }
        let sides =
            sides.get_or_insert_with(|| snap.solids.iter().flat_map(|s| s.sides.iter().map(|x| x.id)).collect());
        let (mut listed, mut missing) = (0usize, 0usize);
        let mut on_side_list = 0usize;
        for (key, v) in &e.props {
            let is_sides = key.eq_ignore_ascii_case("sides");
            if !(is_sides || key.eq_ignore_ascii_case("sides2")) {
                continue;
            }
            for tok in v.split_ascii_whitespace() {
                listed += 1;
                if tok.parse::<Id>().map_or(true, |id| !sides.contains(&id)) {
                    missing += 1;
                }
            }
            if is_sides {
                on_side_list = parse_side_list(v).len();
            }
        }
        let overlay = OVERLAY_CLASSES.iter().any(|c| e.classname.eq_ignore_ascii_case(c));
        if overlay && on_side_list > MAX_OVERLAY_FACES {
            let shown =
                if on_side_list >= MAX_SIDE_LIST { format!("{MAX_SIDE_LIST}+") } else { on_side_list.to_string() };
            out.push(
                problem(
                    Severity::Error,
                    "overlay_too_many_faces",
                    format!("Overlay {} ({}) is on {shown} faces, {MAX_OVERLAY_FACES} at most", e.id, e.classname),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), e.classname.clone(), shown]),
            );
        }
        if missing == 0 && !(overlay && listed == 0) {
            continue;
        }
        let none_left = listed == missing;
        let (severity, message) = if overlay && none_left {
            (Severity::Error, format!("Overlay {} ({}) is on no existing face", e.id, e.classname))
        } else {
            (Severity::Warning, format!("Entity {} ({}) lists {missing} faces that do not exist", e.id, e.classname))
        };
        out.push(problem(severity, "overlay_missing_faces", message, vec![ObjRef::Entity(e.id)], true));
    }
}

fn check_assets(snap: &MapSnapshot, k: &dyn CheckKnowledge, mats: &mut Materials<'_>, out: &mut Vec<Problem>) {
    let mut model_seen: HashMap<String, Option<bool>> = HashMap::new();
    for r in &snap.entities {
        let e = &r.entity;
        let model = e.get("model").map_or("", str::trim);
        let needs_model = MODEL_CLASSES.iter().any(|c| e.classname.eq_ignore_ascii_case(c));
        let missing = if is_model_path(model) {
            let path = model.replace('\\', "/");
            let found = *model_seen.entry(path.to_ascii_lowercase()).or_insert_with(|| k.model_exists(&path));
            found == Some(false)
        } else {
            needs_model || has_mdl_extension(model)
        };
        if missing {
            let compiled = ["prop_static", "prop_detail"].iter().any(|c| e.classname.eq_ignore_ascii_case(c));
            let severity = if compiled { Severity::Error } else { Severity::Warning };
            let shown = if model.is_empty() { "\"\"" } else { model };
            out.push(problem(
                severity,
                "missing_model",
                format!("Entity {} ({}) uses a model that does not exist: {shown}", e.id, e.classname),
                vec![ObjRef::Entity(e.id)],
                false,
            ));
        }
        let mut missing_sounds: Vec<&str> = Vec::new();
        let mut missing_particles: Vec<&str> = Vec::new();
        let mut bad_paths: Vec<&str> = Vec::new();
        for (key, v) in &e.props {
            let v = v.trim();
            if v.is_empty() {
                continue;
            }
            let Some(kind) = k.asset_key(&e.classname, key).or_else(|| fallback_asset_key(&e.classname, key)) else {
                continue;
            };
            match kind {
                AssetKey::Sound => {
                    if k.sound_exists(v) == Some(false) {
                        missing_sounds.push(v);
                    }
                    if v.contains(['/', '\\', '.']) && is_bad_content_path(v) {
                        bad_paths.push(v);
                    }
                }
                AssetKey::Particle => {
                    if k.particle_exists(v) == Some(false) {
                        missing_particles.push(v);
                    }
                }
                AssetKey::Model | AssetKey::Material => {
                    if is_bad_content_path(v) {
                        bad_paths.push(v);
                    }
                }
            }
        }
        if !missing_sounds.is_empty() {
            out.push(
                problem(
                    Severity::Warning,
                    "missing_sound",
                    format!(
                        "Entity {} ({}) uses sounds that do not exist: {}",
                        e.id,
                        e.classname,
                        missing_sounds.join(", ")
                    ),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), e.classname.clone(), missing_sounds.join(", ")]),
            );
        }
        if !missing_particles.is_empty() {
            out.push(
                problem(
                    Severity::Warning,
                    "missing_particle",
                    format!(
                        "Entity {} ({}) uses particle systems that do not exist: {}",
                        e.id,
                        e.classname,
                        missing_particles.join(", ")
                    ),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), e.classname.clone(), missing_particles.join(", ")]),
            );
        }
        if !bad_paths.is_empty() {
            out.push(
                problem(
                    Severity::Warning,
                    "bad_content_path",
                    format!(
                        "Entity {} ({}) uses paths with spaces or special characters: {}",
                        e.id,
                        e.classname,
                        bad_paths.join(", ")
                    ),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), e.classname.clone(), bad_paths.join(", ")]),
            );
        }
    }

    let mut missing_faces = Vec::new();
    let mut missing_names: Vec<u32> = Vec::new();
    let mut bad_faces = Vec::new();
    let mut bad_names: Vec<u32> = Vec::new();
    let mut mixed = Vec::new();
    for s in &snap.solids {
        let (mut water, mut solid) = (false, false);
        for (i, x) in s.sides.iter().enumerate() {
            if is_bad_content_path(snap.material_key(x.material)) {
                bad_faces.push(FaceRef::new(s.id, i));
                if !bad_names.contains(&x.material) {
                    bad_names.push(x.material);
                }
            }
            match mats.known(x.material).map(|f| f.kind()) {
                Some(MaterialKind::Missing) => {
                    missing_faces.push(FaceRef::new(s.id, i));
                    if !missing_names.contains(&x.material) {
                        missing_names.push(x.material);
                    }
                }
                Some(MaterialKind::Water) => water = true,
                Some(MaterialKind::Regular) => solid = true,
                Some(MaterialKind::Tool) | None => {}
            }
        }
        if water && solid {
            mixed.push(ObjRef::Solid(s.id));
        }
    }
    let list = |ids: &[u32]| {
        let shown: Vec<&str> = ids.iter().take(5).map(|&m| snap.material_display(m)).collect();
        let more = if ids.len() > shown.len() { ", ..." } else { "" };
        format!("{}{more}", shown.join(", "))
    };
    if !missing_faces.is_empty() {
        let message = format!(
            "{} faces use {} materials that do not exist: {}",
            missing_faces.len(),
            missing_names.len(),
            list(&missing_names)
        );
        out.push(face_problem(Severity::Warning, "missing_material", message, missing_faces, false));
    }
    if !bad_faces.is_empty() {
        let shown = list(&bad_names);
        let message = format!("{} faces use materials with spaces or special characters: {shown}", bad_faces.len());
        out.push(
            face_problem(Severity::Warning, "bad_material_path", message, bad_faces, false).with_args(vec![shown]),
        );
    }
    if !mixed.is_empty() {
        out.push(problem(
            Severity::Warning,
            "mixed_face_contents",
            format!("{} solids mix water and solid faces (one content per brush)", mixed.len()),
            mixed,
            false,
        ));
    }
}

fn check_entity_rules(
    snap: &MapSnapshot,
    k: &dyn CheckKnowledge,
    opts: &CheckOptions,
    mats: &mut Materials<'_>,
    out: &mut Vec<Problem>,
) {
    let world = &snap.world;
    let long = |e: &hp_vmf::Entity| -> Vec<String> {
        let mut v = Vec::new();
        for (key, value) in &e.props {
            if key.len() > MAX_KEY_LEN || value.len() > MAX_VALUE_LEN {
                v.push(key.chars().take(40).collect());
            }
        }
        for c in &e.connections {
            let len = c.target.len() + c.input.len() + c.param.len() + 24;
            if c.output.len() > MAX_KEY_LEN || len > MAX_VALUE_LEN {
                v.push(c.output.chars().take(40).collect());
            }
        }
        v
    };
    for e in std::iter::once(&**world).chain(snap.entities.iter().map(|r| &r.entity)) {
        let keys = long(e);
        if !keys.is_empty() {
            out.push(problem(
                Severity::Error,
                "key_too_long",
                format!(
                    "Entity {} ({}) has keys or values longer than VBSP reads ({MAX_KEY_LEN} and {MAX_VALUE_LEN} characters): {}",
                    e.id,
                    e.classname,
                    keys.join(", ")
                ),
                vec![ObjRef::Entity(e.id)],
                false,
            )
            .with_args(vec![e.id.to_string(), e.classname.clone(), keys.join(", ")]));
        }
    }
    let named: Vec<ObjRef> = snap
        .entities
        .iter()
        .filter(|r| ["light", "light_spot"].iter().any(|c| r.entity.classname.eq_ignore_ascii_case(c)))
        .filter(|r| r.entity.get("targetname").is_some_and(|t| !t.trim().is_empty()))
        .map(|r| ObjRef::Entity(r.entity.id))
        .collect();
    if named.len() > opts.max_switchable_lights {
        out.push(
            problem(
                Severity::Error,
                "too_many_named_lights",
                format!(
                    "{} named lights are switchable, the compiler accepts {} at most",
                    named.len(),
                    opts.max_switchable_lights
                ),
                named,
                false,
            )
            .with_args(vec![opts.max_switchable_lights.to_string()]),
        );
    }
    check_areaportals(snap, mats, out);
    let has_sun = snap.entities.iter().any(|r| r.entity.classname.eq_ignore_ascii_case("light_environment"));
    if !has_sun {
        let sky: Vec<ObjRef> = snap
            .solids
            .iter()
            .filter(|s| s.owner == Owner::World && s.sides.iter().any(|x| mats.facts(x.material).sky()))
            .map(|s| ObjRef::Solid(s.id))
            .collect();
        if !sky.is_empty() {
            out.push(problem(
                Severity::Warning,
                "sky_without_sun",
                "The map has sky faces but no light_environment: no sun or sky light".to_owned(),
                sky,
                false,
            ));
        }
    }
    let _ = k;
}

fn door_starts_open(e: &hp_vmf::Entity) -> bool {
    let num = |k: &str| e.get(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
    num("spawnpos") != 0 || (e.classname.to_ascii_lowercase().starts_with("func_door") && num("spawnflags") & 1 != 0)
}

fn check_areaportals(snap: &MapSnapshot, mats: &mut Materials<'_>, out: &mut Vec<Problem>) {
    let portals: Vec<(usize, &hp_vmf::Entity, u32)> = snap
        .entities
        .iter()
        .zip(&snap.entity_solids)
        .enumerate()
        .filter(|(_, (r, _))| SEALING_ENTITY_CLASSES.iter().any(|c| r.entity.classname.eq_ignore_ascii_case(c)))
        .map(|(i, (r, n))| (i, &r.entity, *n))
        .collect();
    if portals.is_empty() {
        return;
    }
    let mut bounds_of: HashMap<Id, Vec<Aabb>> = HashMap::new();
    let mut water: Vec<Aabb> = Vec::new();
    let occluders: HashSet<Id> = snap
        .entities
        .iter()
        .filter(|r| r.entity.classname.eq_ignore_ascii_case("func_occluder"))
        .map(|r| r.entity.id)
        .collect();
    let mut occluder_bounds: Vec<Aabb> = Vec::new();
    for s in &snap.solids {
        if !s.geom.valid {
            continue;
        }
        match s.owner {
            Owner::Entity(e) => {
                bounds_of.entry(e).or_default().push(s.geom.aabb);
                if occluders.contains(&e) {
                    occluder_bounds.push(s.geom.aabb);
                }
            }
            Owner::World => {
                if s.sides.iter().any(|x| mats.facts(x.material).water) {
                    water.push(s.geom.aabb);
                }
            }
        }
    }
    let crosses = |a: &Aabb, b: &Aabb| {
        a.max.x > b.min.x + 0.01
            && a.min.x < b.max.x - 0.01
            && a.max.y > b.min.y + 0.01
            && a.min.y < b.max.y - 0.01
            && a.max.z > b.min.z + 0.01
            && a.min.z < b.max.z - 0.01
    };
    for (_, e, solids) in portals {
        if solids > 1 {
            out.push(
                problem(
                    Severity::Error,
                    "areaportal_brushes",
                    format!("Areaportal {} ({}) has {solids} brushes, exactly one is allowed", e.id, e.classname),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), e.classname.clone(), solids.to_string()]),
            );
        }
        let mine = bounds_of.get(&e.id).map_or(&[][..], Vec::as_slice);
        if mine.iter().any(|b| water.iter().any(|w| crosses(b, w))) {
            out.push(
                problem(
                    Severity::Warning,
                    "areaportal_water",
                    format!("Areaportal {} ({}) crosses a water surface: split it in two", e.id, e.classname),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), e.classname.clone()]),
            );
        }
        if mine.iter().any(|b| occluder_bounds.iter().any(|o| crosses(b, o))) {
            out.push(
                problem(
                    Severity::Warning,
                    "areaportal_occluder",
                    format!("Areaportal {} ({}) crosses an occluder", e.id, e.classname),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), e.classname.clone()]),
            );
        }
        if !e.classname.eq_ignore_ascii_case("func_areaportal") {
            continue;
        }
        let Some(target) = e.get("target").map(str::trim).filter(|t| !t.is_empty()) else { continue };
        let linked: Vec<&hp_vmf::Entity> = snap
            .entities
            .iter()
            .map(|o| &o.entity)
            .filter(|o| o.get("targetname").is_some_and(|n| n.trim().eq_ignore_ascii_case(target)))
            .collect();
        let doors: Vec<&&hp_vmf::Entity> = linked.iter().filter(|o| is_door_class(&o.classname)).collect();
        if !linked.is_empty() && doors.is_empty() {
            let classes: Vec<&str> = linked.iter().map(|o| o.classname.as_str()).collect();
            out.push(
                problem(
                    Severity::Warning,
                    "areaportal_not_door",
                    format!("Areaportal {} is linked to {target}, which is not a door: {}", e.id, classes.join(", ")),
                    vec![ObjRef::Entity(e.id)],
                    false,
                )
                .with_args(vec![e.id.to_string(), target.to_owned(), classes.join(", ")]),
            );
            continue;
        }
        let portal_open = e.get("StartOpen").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(1) != 0;
        if let Some(door) = doors.first()
            && door_starts_open(door) != portal_open
        {
            let (p, d) = if portal_open { ("open", "closed") } else { ("closed", "open") };
            out.push(
                problem(
                    Severity::Warning,
                    "areaportal_door_state",
                    format!("Areaportal {} starts {p} but its door {target} starts {d}", e.id),
                    vec![ObjRef::Entity(e.id), ObjRef::Entity(door.id)],
                    true,
                )
                .with_args(vec![
                    e.id.to_string(),
                    target.to_owned(),
                    u8::from(portal_open).to_string(),
                ]),
            );
        }
    }
}

const DETAIL_SMALL: f64 = 64.0;
const DETAIL_COMPLEX: f64 = 256.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DetailReason {
    Small,
    Complex,
}

pub fn detail_reason(extent: f64, faces: usize, axial: bool) -> Option<DetailReason> {
    if !extent.is_finite() || extent < 0.0 {
        return None;
    }
    if extent <= DETAIL_SMALL {
        Some(DetailReason::Small)
    } else if (faces > 6 || !axial) && extent <= DETAIL_COMPLEX {
        Some(DetailReason::Complex)
    } else {
        None
    }
}

pub fn detail_candidates(snap: &MapSnapshot, report: &LeakReport, k: &dyn CheckKnowledge) -> Vec<Id> {
    if report.leaks() || report.incomplete || report.no_world {
        return Vec::new();
    }
    let hull: IdSet = report.hull.iter().copied().collect();
    let mut mats = Materials::new(snap, k);
    let mut out = Vec::new();
    for s in &snap.solids {
        if s.owner != Owner::World
            || !s.geom.valid
            || s.has_disp()
            || hull.contains(&s.id)
            || !snap.is_visible(ObjRef::Solid(s.id))
        {
            continue;
        }
        if !s.sides.iter().all(|x| {
            let f = mats.facts(x.material);
            f.seals() && !f.sky()
        }) {
            continue;
        }
        let size = s.geom.aabb.size();
        let extent = size.x.max(size.y).max(size.z);
        if detail_reason(extent, s.geom.faces.len(), s.axial).is_some() {
            out.push(s.id);
        }
    }
    out
}

pub fn leak_problems(snap: &MapSnapshot, report: &LeakReport, candidates: &[Id]) -> Vec<Problem> {
    let mut out = Vec::new();
    if report.incomplete {
        out.push(problem(
            Severity::Info,
            "leak_check_incomplete",
            "The map is too complex for the instant leak check: compile to find leaks".to_owned(),
            Vec::new(),
            false,
        ));
        return out;
    }
    if report.no_world {
        return out;
    }
    if let Some(leak) = &report.leak {
        let class = snap
            .entities
            .iter()
            .find(|r| r.entity.id == leak.entity)
            .map_or_else(String::new, |r| r.entity.classname.clone());
        out.push(
            problem(
                Severity::Error,
                "leak",
                format!(
                    "The map leaks: {} entities reach the void, the nearest is entity {} ({class})",
                    report.leaking.len().max(1),
                    leak.entity
                ),
                vec![ObjRef::Entity(leak.entity)],
                false,
            )
            .with_args(vec![
                leak.entity.to_string(),
                class.clone(),
                report.leaking.len().max(1).to_string(),
            ]),
        );
        return out;
    }
    if !candidates.is_empty() {
        out.push(problem(
            Severity::Info,
            "detail_candidate",
            format!(
                "{} world brushes could be func_detail (small or complex, not part of the hull of the map): VVIS runs faster",
                candidates.len()
            ),
            candidates.iter().map(|id| ObjRef::Solid(*id)).collect(),
            true,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::tests::box_sides;

    #[test]
    fn detail_reasons() {
        assert_eq!(detail_reason(64.0, 6, true), Some(DetailReason::Small));
        assert_eq!(detail_reason(200.0, 8, true), Some(DetailReason::Complex));
        assert_eq!(detail_reason(200.0, 6, false), Some(DetailReason::Complex));
        assert_eq!(detail_reason(200.0, 6, true), None, "a plain box stays structural");
        assert_eq!(detail_reason(300.0, 20, false), None, "too big");
        assert_eq!(detail_reason(f64::NAN, 6, true), None);
        assert_eq!(detail_reason(f64::INFINITY, 6, true), None);
        assert_eq!(detail_reason(-1.0, 6, true), None);
    }

    struct Know;

    impl CheckKnowledge for Know {
        fn class_exists(&self, class: &str) -> Option<bool> {
            Some(!class.eq_ignore_ascii_case("weird_class"))
        }
        fn class_has_input(&self, class: &str, input: &str) -> Option<bool> {
            class.eq_ignore_ascii_case("logic_relay").then(|| input.eq_ignore_ascii_case("Trigger"))
        }
        fn material(&self, name: &str) -> Option<MaterialFacts> {
            let mut f = MaterialFacts::guess(name);
            f.missing = name.to_ascii_lowercase().starts_with("missing/");
            Some(f)
        }
        fn sound_exists(&self, name: &str) -> Option<bool> {
            Some(!name.contains("nothing"))
        }
        fn particle_exists(&self, name: &str) -> Option<bool> {
            Some(name != "no_fire")
        }
    }

    fn codes(p: &[Problem]) -> Vec<&'static str> {
        p.iter().map(|x| x.code).collect()
    }

    fn base() -> Document {
        let mut doc = Document::new_empty();
        doc.create_solid(box_sides(DVec3::splat(-512.0), DVec3::new(512.0, 512.0, -480.0), "BRICK/WALL"), Owner::World)
            .expect("floor");
        doc.create_point_entity("info_player_start", DVec3::new(0.0, 0.0, -400.0), &[]).expect("spawn");
        doc
    }

    #[test]
    fn sky_camera_checks() {
        let mut doc = base();
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let day =
            doc.create_point_entity("sky_camera", DVec3::new(0.0, 0.0, 2000.0), &[kv("scale", "16")]).expect("day");
        let run = |doc: &Document| check_snapshot(&MapSnapshot::capture(doc), &Know, &CheckOptions::default());
        let p = run(&doc);
        assert!(
            !codes(&p).iter().any(|c| c.starts_with("sky_camera") || *c == "duplicate_singleton"),
            "{:?}",
            codes(&p)
        );
        let night =
            doc.create_point_entity("sky_camera", DVec3::new(0.0, 0.0, 3000.0), &[kv("scale", "0")]).expect("night");
        let p = run(&doc);
        let multiple = p.iter().find(|x| x.code == "sky_camera_multiple").expect("several sky cameras");
        assert_eq!(multiple.severity, Severity::Info);
        assert_eq!(multiple.objects, vec![ObjRef::Entity(night), ObjRef::Entity(day)], "the game's camera first");
        assert_eq!(multiple.args, vec!["2".to_owned(), night.to_string()]);
        let bad = p.iter().find(|x| x.code == "sky_camera_bad_scale").expect("scale 0");
        assert_eq!((bad.severity, bad.objects.clone()), (Severity::Error, vec![ObjRef::Entity(night)]));
        assert!(!codes(&p).contains(&"duplicate_singleton"));
        doc.set_entity_key(&[night], "targetname", "night").expect("name");
        let ok = doc
            .create_point_entity(crate::sky::SWITCH_CLASS, DVec3::ZERO, &[kv("skycamera", "NIGHT")])
            .expect("switch");
        let bad =
            doc.create_point_entity(crate::sky::SWITCH_CLASS, DVec3::ZERO, &[kv("skycamera", "dusk")]).expect("switch");
        let empty = doc.create_point_entity(crate::sky::SWITCH_CLASS, DVec3::ZERO, &[]).expect("switch");
        let p = run(&doc);
        let flagged: Vec<ObjRef> =
            p.iter().filter(|x| x.code == "skybox_switch_bad_target").flat_map(|x| x.objects.clone()).collect();
        assert_eq!(flagged, vec![ObjRef::Entity(bad), ObjRef::Entity(empty)]);
        assert!(!flagged.contains(&ObjRef::Entity(ok)));
        assert!(
            !p.iter().any(|x| x.code == "missing_target" && x.objects.contains(&ObjRef::Entity(bad))),
            "one report"
        );
    }

    #[test]
    fn new_entity_checks() {
        let mut doc = base();
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let relay = doc.create_point_entity("logic_relay", DVec3::ZERO, &[kv("targetname", "relay")]).expect("relay");
        let button = doc.create_point_entity("logic_auto", DVec3::ZERO, &[]).expect("auto");
        doc.add_connection(&[button], &hp_vmf::Connection::parse("OnMapSpawn", "relay,Explode,,0,-1")).expect("io");
        doc.add_connection(&[button], &hp_vmf::Connection::parse("OnMapSpawn", "relay,Trigger,,0,-1")).expect("io");
        doc.add_connection(&[button], &hp_vmf::Connection::parse("OnMapSpawn", "relay,Kill,,0,-1")).expect("io");
        let long_value = "x".repeat(1100);
        let long = doc.create_point_entity("info_target", DVec3::ZERO, &[kv("note", &long_value)]).expect("long");
        let sun1 = doc.create_point_entity("light_environment", DVec3::ZERO, &[]).expect("sun");
        let sun2 = doc.create_point_entity("light_environment", DVec3::ONE, &[]).expect("sun");
        let snd = doc
            .create_point_entity("ambient_generic", DVec3::ZERO, &[kv("message", "ambient/nothing here.wav")])
            .expect("sound");
        let fire =
            doc.create_point_entity("info_particle_system", DVec3::ZERO, &[kv("effect_name", "no_fire")]).expect("fx");
        let snap = MapSnapshot::capture(&doc);
        let p = check_snapshot(&snap, &Know, &CheckOptions::default());
        let find = |code: &str| p.iter().find(|x| x.code == code).unwrap_or_else(|| panic!("{code}: {:?}", codes(&p)));
        let inp = find("unknown_input");
        assert_eq!(inp.objects, vec![ObjRef::Entity(button)]);
        assert!(inp.message.ends_with("relay.Explode"), "{}", inp.message);
        let _ = relay;
        assert_eq!(find("key_too_long").objects, vec![ObjRef::Entity(long)]);
        assert_eq!(find("duplicate_singleton").objects, vec![ObjRef::Entity(sun1), ObjRef::Entity(sun2)]);
        assert_eq!(find("missing_sound").objects, vec![ObjRef::Entity(snd)]);
        assert_eq!(find("missing_particle").objects, vec![ObjRef::Entity(fire)]);
        assert_eq!(find("bad_content_path").objects, vec![ObjRef::Entity(snd)]);
        let bare = check_snapshot(&snap, &NoKnowledge, &CheckOptions::default());
        for code in ["unknown_input", "missing_sound", "missing_particle"] {
            assert!(!codes(&bare).contains(&code), "{code}");
        }
    }

    #[test]
    fn new_solid_checks() {
        let mut doc = base();
        let d =
            doc.create_solid(box_sides(DVec3::ZERO, DVec3::splat(64.0), "BRICK/WALL"), Owner::World).expect("solid");
        doc.create_displacements(&[crate::FaceRef::new(d, 0)], 2).expect("disp");
        let ent = doc.tie_to_entity(&[ObjRef::Solid(d)], "func_wall", &[]).expect("tie");
        let tiny = doc
            .create_solid(box_sides(DVec3::splat(100.0), DVec3::splat(100.5), "BRICK/WALL"), Owner::World)
            .expect("tiny");
        let origin = doc
            .create_solid(box_sides(DVec3::splat(200.0), DVec3::splat(216.0), "TOOLS/TOOLSORIGIN"), Owner::World)
            .expect("origin");
        let sky = doc
            .create_solid(
                box_sides(DVec3::new(-512.0, -512.0, 480.0), DVec3::splat(512.0), "TOOLS/TOOLSSKYBOX"),
                Owner::World,
            )
            .expect("sky");
        let lamp = doc.create_point_entity("light", DVec3::new(0.0, 0.0, -496.0), &[]).expect("light");
        let snap = MapSnapshot::capture(&doc);
        let p = check_snapshot(&snap, &Know, &CheckOptions::default());
        let find = |code: &str| p.iter().find(|x| x.code == code).unwrap_or_else(|| panic!("{code}: {:?}", codes(&p)));
        let dp = find("disp_on_brush_entity");
        assert_eq!(dp.objects, vec![ObjRef::Solid(d)]);
        assert!(dp.message.contains(&format!("Entity {ent} (func_wall)")));
        assert_eq!(find("micro_brush").objects, vec![ObjRef::Solid(tiny)]);
        assert_eq!(find("origin_brush_in_world").objects, vec![ObjRef::Solid(origin)]);
        assert_eq!(find("sky_without_sun").objects, vec![ObjRef::Solid(sky)]);
        assert_eq!(find("entity_in_solid").objects, vec![ObjRef::Entity(lamp)]);
        assert!(p.windows(2).all(|w| w[0].severity >= w[1].severity));
    }

    #[test]
    fn overlays_on_too_many_faces_and_named_lights() {
        let mut doc = base();
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let ids: Vec<String> = (1..=70).map(|i| i.to_string()).collect();
        let ov = doc.create_point_entity("info_overlay", DVec3::ZERO, &[kv("sides", &ids.join(" "))]).expect("overlay");
        for i in 0..3 {
            doc.create_point_entity("light", DVec3::ZERO, &[kv("targetname", &format!("l{i}"))]).expect("light");
        }
        let snap = MapSnapshot::capture(&doc);
        let p = check_snapshot(&snap, &NoKnowledge, &CheckOptions { max_switchable_lights: 2 });
        assert!(p.iter().any(|x| x.code == "overlay_too_many_faces" && x.objects == vec![ObjRef::Entity(ov)]));
        assert!(p.iter().any(|x| x.code == "too_many_named_lights" && x.objects.len() == 3));
    }

    #[test]
    fn areaportal_rules() {
        let mut doc = base();
        let a = doc
            .create_solid(box_sides(DVec3::ZERO, DVec3::splat(8.0), "TOOLS/TOOLSAREAPORTAL"), Owner::World)
            .expect("a");
        let b = doc
            .create_solid(box_sides(DVec3::splat(16.0), DVec3::splat(24.0), "TOOLS/TOOLSAREAPORTAL"), Owner::World)
            .expect("b");
        let kv = vec![("target".to_owned(), "lamp".to_owned())];
        let portal = doc.tie_to_entity(&[ObjRef::Solid(a), ObjRef::Solid(b)], "func_areaportal", &kv).expect("tie");
        doc.create_point_entity("info_target", DVec3::ZERO, &[("targetname".to_owned(), "lamp".to_owned())])
            .expect("t");
        let snap = MapSnapshot::capture(&doc);
        let p = check_snapshot(&snap, &NoKnowledge, &CheckOptions::default());
        assert!(p.iter().any(|x| x.code == "areaportal_brushes" && x.objects == vec![ObjRef::Entity(portal)]));
        assert!(p.iter().any(|x| x.code == "areaportal_not_door"));
    }

    #[test]
    fn areaportal_door_state_water_and_occluders() {
        let mut doc = base();
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let a = doc
            .create_solid(box_sides(DVec3::ZERO, DVec3::splat(8.0), "TOOLS/TOOLSAREAPORTAL"), Owner::World)
            .expect("a");
        let portal = doc
            .tie_to_entity(&[ObjRef::Solid(a)], "func_areaportal", &[kv("target", "door1"), kv("StartOpen", "0")])
            .expect("tie");
        let d = doc
            .create_solid(box_sides(DVec3::splat(100.0), DVec3::splat(108.0), "BRICK/WALL"), Owner::World)
            .expect("d");
        let door = doc
            .tie_to_entity(&[ObjRef::Solid(d)], "func_door", &[kv("targetname", "door1"), kv("spawnpos", "1")])
            .expect("door");
        doc.create_solid(
            box_sides(DVec3::new(-4.0, -4.0, -4.0), DVec3::new(12.0, 12.0, 4.0), "liquids/water_river"),
            Owner::World,
        )
        .expect("water");
        let o = doc
            .create_solid(box_sides(DVec3::splat(2.0), DVec3::splat(6.0), "TOOLS/TOOLSOCCLUDER"), Owner::World)
            .expect("occluder");
        doc.tie_to_entity(&[ObjRef::Solid(o)], "func_occluder", &[]).expect("tie");
        let snap = MapSnapshot::capture(&doc);
        let p = check_snapshot(&snap, &Know, &CheckOptions::default());
        let state = p.iter().find(|x| x.code == "areaportal_door_state").expect("state");
        assert_eq!(state.objects, vec![ObjRef::Entity(portal), ObjRef::Entity(door)]);
        assert!(state.fixable);
        assert!(p.iter().any(|x| x.code == "areaportal_water"));
        assert!(p.iter().any(|x| x.code == "areaportal_occluder"));
        doc.fix_problem(state).expect("fix");
        let n = doc.scene().entity(portal).expect("portal");
        assert_eq!(n.entity.get("StartOpen"), Some("1"));
    }

    #[test]
    fn leak_and_detail_candidates() {
        use super::super::leak::{LeakPath, LeakReport};
        let mut doc = base();
        let small =
            doc.create_solid(box_sides(DVec3::ZERO, DVec3::splat(32.0), "BRICK/WALL"), Owner::World).expect("s");
        let big = doc
            .create_solid(box_sides(DVec3::splat(100.0), DVec3::splat(400.0), "BRICK/WALL"), Owner::World)
            .expect("b");
        let snap = MapSnapshot::capture(&doc);
        let sealed = LeakReport { hull: vec![snap.solids[0].id], ..LeakReport::default() };
        let candidates = detail_candidates(&snap, &sealed, &NoKnowledge);
        assert_eq!(candidates, vec![small]);
        let p = leak_problems(&snap, &sealed, &candidates);
        let d = p.iter().find(|x| x.code == "detail_candidate").expect("detail");
        assert_eq!(d.objects, vec![ObjRef::Solid(small)]);
        assert!(d.fixable);
        let _ = big;
        let spawn = snap.entities[0].entity.id;
        let leaking = LeakReport {
            leak: Some(LeakPath { entity: spawn, points: vec![DVec3::ZERO, DVec3::ONE] }),
            leaking: vec![spawn],
            ..LeakReport::default()
        };
        assert!(detail_candidates(&snap, &leaking, &NoKnowledge).is_empty());
        let p = leak_problems(&snap, &leaking, &[]);
        assert_eq!(codes(&p), vec!["leak"]);
        assert!(p[0].message.contains("(info_player_start)"));
        let p = leak_problems(&snap, &LeakReport { incomplete: true, ..LeakReport::default() }, &[]);
        assert_eq!(codes(&p), vec!["leak_check_incomplete"]);
    }

    #[test]
    fn material_facts() {
        assert!(MaterialFacts::guess("TOOLS/TOOLSNODRAW").seals());
        assert!(MaterialFacts::guess("tools/toolsskybox2d").sky());
        assert!(!MaterialFacts::guess("tools/toolsclip").seals());
        assert!(!MaterialFacts::guess("tools/toolsinvisible").seals());
        assert!(!MaterialFacts::guess("liquids/water_01").seals());
        assert!(!MaterialFacts::guess("liquids/water_01").lightmapped());
        assert!(MaterialFacts::guess("brick/wall").lightmapped());
        assert!(!MaterialFacts { translucent: true, ..MaterialFacts::default() }.seals());
        assert_eq!(MaterialFacts::from_kind("tools/toolshint", MaterialKind::Tool).tool, Some(ToolUse::Hint));
        assert_eq!(MaterialFacts::from_kind("x", MaterialKind::Missing).kind(), MaterialKind::Missing);
        assert!(is_bad_content_path("models/my props/a.mdl"));
        assert!(is_bad_content_path("models/caf\u{e9}/a.mdl"));
        assert!(!is_bad_content_path("models/props/a.mdl"));
    }
}
