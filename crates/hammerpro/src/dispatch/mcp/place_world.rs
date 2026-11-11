use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use glam::{DMat3, DMat4, DVec3};
use hp_app::AppState;
use hp_editor::{IdSet, ObjRef, Owner};
use hp_geom::Aabb;
use hp_shape::{ModelShape, TriMesh};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MeshChoice {
    Render,
    Collision,
}

impl MeshChoice {
    pub(super) fn name(self) -> &'static str {
        match self {
            MeshChoice::Render => "render",
            MeshChoice::Collision => "collision",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ModelRef {
    pub(super) path: String,
    pub(super) body: u32,
    pub(super) matrix: DMat4,
    pub(super) fallback: Option<Aabb>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Key {
    Solid(u32),
    Entity(u32),
    Group(u32),
}

impl Key {
    pub(super) fn id(self) -> u32 {
        match self {
            Key::Solid(i) | Key::Entity(i) | Key::Group(i) => i,
        }
    }

    pub(super) fn kind(self) -> &'static str {
        match self {
            Key::Solid(_) => "solid",
            Key::Entity(_) => "entity",
            Key::Group(_) => "group",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Obj {
    pub(super) key: Key,
    pub(super) kind: &'static str,
    pub(super) class: String,
    pub(super) meshes: Vec<TriMesh>,
    pub(super) models: Vec<ModelRef>,
    pub(super) origin: Option<DVec3>,
    pub(super) angles: DVec3,
    pub(super) scale: f64,
    pub(super) hint: Aabb,
    pub(super) refs: Vec<ObjRef>,
    pub(super) pending: bool,
    pub(super) solid_ids: Vec<u32>,
}

impl Obj {
    pub(super) fn model_path(&self) -> Option<&str> {
        self.models.first().map(|m| m.path.as_str())
    }

    pub(super) fn rotation(&self) -> DMat3 {
        DMat3::from_quat(hp_geom::angles_to_quat(self.angles))
    }
}

const HIDDEN_TOOLS: &[&str] = &[
    "tools/toolstrigger",
    "tools/toolsclip",
    "tools/toolsplayerclip",
    "tools/toolsnpcclip",
    "tools/toolsgrenadeclip",
    "tools/toolsdotted",
    "tools/toolshint",
    "tools/toolsskip",
    "tools/toolsinvisible",
    "tools/toolsinvisibleladder",
    "tools/toolsareaportal",
    "tools/toolsoccluder",
    "tools/toolsfog",
    "tools/toolsblocklight",
    "tools/toolsblock_los",
    "tools/toolsblockbullets",
    "tools/toolsblockbullets2",
    "tools/toolsorigin",
    "tools/toolscontrolclip",
    "tools/toolsstairs",
];

const NON_SOLID_CLASSES: &[&str] = &[
    "trigger_",
    "func_areaportal",
    "func_occluder",
    "func_viscluster",
    "func_precipitation",
    "func_dustmotes",
    "func_dustcloud",
    "func_smokevolume",
    "func_clip_vphysics",
    "func_ladderendpoint",
    "func_nav_",
    "func_buyzone",
    "func_bomb_target",
    "func_hostage_rescue",
    "func_fish_pool",
    "env_",
    "color_correction_volume",
    "fog_volume",
    "postprocess_controller",
];

const OVERLAY_CLASSES: &[&str] = &["info_overlay", "info_overlay_transition"];

pub(super) fn hidden_material(m: &str) -> bool {
    let m = m.trim().replace('\\', "/").to_ascii_lowercase();
    HIDDEN_TOOLS.contains(&m.as_str())
}

fn nodraw(m: &str) -> bool {
    m.trim().replace('\\', "/").eq_ignore_ascii_case("tools/toolsnodraw")
}

pub(super) fn non_solid_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    NON_SOLID_CLASSES.iter().any(|p| if p.ends_with('_') { c.starts_with(p) } else { c == *p })
}

fn disp_mesh(m: &hp_geom::DispMesh, side: usize, xf: Option<&DMat4>) -> TriMesh {
    let mut out = TriMesh::new();
    out.verts = match xf {
        Some(x) => m.positions.iter().map(|p| x.transform_point3(*p)).collect(),
        None => m.positions.clone(),
    };
    let facing = match xf {
        Some(x) => x.transform_vector3(m.face_normal),
        None => m.face_normal,
    };
    for t in m.indices.chunks_exact(3) {
        if t.iter().all(|&i| (i as usize) < out.verts.len()) {
            out.add_tri_facing([t[0], t[1], t[2]], facing, side as u32);
        }
    }
    out
}

fn brush_meshes(
    sides: &[hp_vmf::Side],
    geom: &hp_geom::SolidGeom,
    disps: &[Option<Arc<hp_geom::DispMesh>>],
    xf: Option<&DMat4>,
    include_tools: bool,
) -> (Vec<TriMesh>, bool) {
    if sides.iter().any(|s| s.disp.is_some()) {
        let mut out = Vec::new();
        for (i, side) in sides.iter().enumerate() {
            if let Some(d) = disps.get(i).and_then(Option::as_ref) {
                out.push(disp_mesh(d, i, xf));
            } else if let Some(info) = &side.disp
                && let Some(face) = geom.face_for_side(i)
                && let Some(d) = hp_geom::build_disp(&face.vertices, info)
            {
                out.push(disp_mesh(&d, i, xf));
            }
        }
        return (out.into_iter().filter(|m| !m.is_empty()).collect(), true);
    }
    let visible = sides.iter().any(|s| !hidden_material(&s.material) && !nodraw(&s.material));
    let mut m = TriMesh::new();
    let mut all = true;
    for f in &geom.faces {
        let mat = sides.get(f.side_index).map_or("", |s| s.material.as_str());
        let skip = !include_tools && (hidden_material(mat) || (!visible && nodraw(mat)));
        if skip {
            all = false;
            continue;
        }
        let (pts, n): (Vec<DVec3>, DVec3) = match xf {
            Some(x) => {
                (f.vertices.iter().map(|p| x.transform_point3(*p)).collect(), x.transform_vector3(f.plane.normal))
            }
            None => (f.vertices.clone(), f.plane.normal),
        };
        m.add_polygon_facing(&pts, n, f.side_index as u32);
    }
    m.closed = all && geom.valid;
    (if m.is_empty() { Vec::new() } else { vec![m] }, false)
}

pub(super) fn model_of(state: &AppState, e: &hp_vmf::Entity) -> Option<String> {
    let fixed = state
        .assets
        .fgd()
        .and_then(|f| f.class(&e.classname))
        .and_then(|c| c.studio())
        .and_then(|s| s.model)
        .filter(|m| is_model_path(m));
    fixed
        .or_else(|| e.get("model").filter(|m| is_model_path(m)).map(str::to_owned))
        .map(|m| m.trim().replace('\\', "/").to_ascii_lowercase())
}

pub(super) fn entity_scale(e: &hp_vmf::Entity) -> f64 {
    let read = |k: &str| e.get(k).and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s > 0.0);
    read("modelscale").or_else(|| read("uniformscale")).map_or(1.0, |s| s.min(1.0e4))
}

pub(super) fn entity_body(e: &hp_vmf::Entity) -> u32 {
    let read = |k: &str| {
        e.get(k).and_then(|s| {
            let s = s.trim();
            s.parse::<i64>().ok().or_else(|| s.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| f as i64))
        })
    };
    read("body").or_else(|| read("SetBodyGroup")).unwrap_or(0).clamp(0, i64::from(u32::MAX)) as u32
}

pub(super) fn model_matrix(origin: DVec3, angles: DVec3, scale: f64) -> DMat4 {
    let s = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    DMat4::from_scale_rotation_translation(DVec3::splat(s), hp_geom::angles_to_quat(angles), origin)
}

fn overlay_quad(e: &hp_vmf::Entity) -> Option<TriMesh> {
    let v = |k: &str| e.get(k).and_then(hp_vmf::parse_vec3).filter(|p| p.is_finite());
    let (o, u, w) = (v("BasisOrigin")?, v("BasisU")?, v("BasisV")?);
    let n = v("BasisNormal").unwrap_or_else(|| u.cross(w));
    let pts: Vec<DVec3> = (0..4).filter_map(|i| v(&format!("uv{i}"))).map(|c| o + u * c.x + w * c.y).collect();
    if pts.len() != 4 {
        return None;
    }
    let mut m = TriMesh::new();
    m.add_polygon_facing(&pts, n, 0);
    (!m.is_empty()).then_some(m)
}

fn instance_parts(
    state: &AppState,
    e: &hp_vmf::Entity,
    include_tools: bool,
) -> Option<(Vec<TriMesh>, Vec<ModelRef>, bool)> {
    if !hp_editor::instances::is_instance(e) {
        return None;
    }
    let main = hp_app::runtime::instances::main_map();
    let cache = hp_app::runtime::instances::InstanceCache::shared();
    let exp = hp_app::runtime::instances::expand_one(&cache, e, main.as_deref(), state.assets.fgd(), true);
    let mut meshes = Vec::new();
    let mut models = Vec::new();
    for piece in &exp.pieces {
        let m = piece.fixup.matrix;
        let mut solids: Vec<&hp_vmf::Solid> = piece.map.world.solids.iter().collect();
        for ent in &piece.map.entities {
            if hp_editor::instances::is_instance(ent) || non_solid_class(&ent.classname) {
                continue;
            }
            solids.extend(ent.solids.iter());
            if ent.solids.is_empty()
                && let Some(path) = model_of(state, ent)
            {
                let mut fixed = ent.clone();
                piece.fixup.fix_entity(&mut fixed, None);
                let origin = fixed.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
                let angles = fixed.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
                models.push(ModelRef {
                    path,
                    body: entity_body(&fixed),
                    matrix: model_matrix(origin, angles, entity_scale(&fixed)),
                    fallback: None,
                });
            }
        }
        for s in solids {
            let geom = hp_geom::build_solid(&s.sides);
            let (ms, _) = brush_meshes(&s.sides, &geom, &[], Some(&m), include_tools);
            meshes.extend(ms);
        }
    }
    Some((meshes, models, exp.is_pending()))
}

fn bounds_of(meshes: &[TriMesh]) -> Aabb {
    meshes.iter().fold(Aabb::EMPTY, |b, m| b.union(&m.bounds()))
}

fn is_model_path(p: &str) -> bool {
    let p = p.trim();
    p.len() > 4 && p.get(p.len() - 4..).is_some_and(|e| e.eq_ignore_ascii_case(".mdl"))
}

const MODEL_REACH: f64 = 1024.0;

pub(super) fn solid_obj(state: &AppState, id: u32, include_tools: bool) -> Option<Obj> {
    let node = state.doc.scene().solid(id)?;
    let (meshes, disp) = brush_meshes(&node.solid.sides, &node.geom, &node.disp, None, include_tools);
    Some(Obj {
        key: Key::Solid(id),
        kind: if disp { "displacement" } else { "solid" },
        class: match node.owner {
            Owner::World => "worldspawn".to_owned(),
            Owner::Entity(e) => state.doc.scene().entity(e).map(|n| n.entity.classname.clone()).unwrap_or_default(),
        },
        hint: node.bounds,
        meshes,
        models: Vec::new(),
        origin: None,
        angles: DVec3::ZERO,
        scale: 1.0,
        refs: vec![ObjRef::Solid(id)],
        pending: false,
        solid_ids: vec![id],
    })
}

pub(super) fn entity_obj(state: &AppState, id: u32, include_tools: bool) -> Option<Obj> {
    let scene = state.doc.scene();
    let e = &scene.entity(id)?.entity;
    let origin = e.origin().filter(|o| o.is_finite());
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let mut obj = Obj {
        key: Key::Entity(id),
        kind: "point_entity",
        class: e.classname.clone(),
        meshes: Vec::new(),
        models: Vec::new(),
        origin,
        angles,
        scale: entity_scale(e),
        hint: Aabb::EMPTY,
        refs: vec![ObjRef::Entity(id)],
        pending: false,
        solid_ids: Vec::new(),
    };
    if scene.is_brush_entity(id) {
        let mut disp = false;
        for sid in scene.entity_solid_ids(id) {
            let Some(node) = scene.solid(sid) else { continue };
            let (ms, d) = brush_meshes(&node.solid.sides, &node.geom, &node.disp, None, include_tools);
            disp |= d;
            obj.meshes.extend(ms);
            obj.solid_ids.push(sid);
            obj.hint = obj.hint.union(&node.bounds);
        }
        obj.kind = if disp { "displacement" } else { "brush_entity" };
        return Some(obj);
    }
    if let Some((meshes, models, pending)) = instance_parts(state, e, include_tools) {
        obj.kind = "instance";
        obj.hint = bounds_of(&meshes);
        if let Some(o) = origin {
            obj.hint = obj.hint.union(&Aabb::new(o, o).expand(if models.is_empty() { 0.0 } else { MODEL_REACH }));
        }
        obj.meshes = meshes;
        obj.models = models;
        obj.pending = pending;
        if obj.meshes.is_empty() && obj.models.is_empty() {
            let b = state
                .assets
                .entity_bounds(e)
                .unwrap_or_else(|| Aabb::new(origin.unwrap_or_default(), origin.unwrap_or_default()).expand(8.0));
            obj.meshes.push(TriMesh::convex_box(b.min, b.max, 0));
            obj.hint = b;
        }
        return Some(obj);
    }
    if OVERLAY_CLASSES.iter().any(|c| e.classname.eq_ignore_ascii_case(c))
        && let Some(q) = overlay_quad(e)
    {
        obj.kind = "overlay";
        obj.hint = q.bounds();
        obj.meshes.push(q);
        return Some(obj);
    }
    if let Some(path) = model_of(state, e) {
        let o = origin.unwrap_or(DVec3::ZERO);
        obj.kind = if e.classname.to_ascii_lowercase().starts_with("prop_") { "prop" } else { "model_entity" };
        obj.models.push(ModelRef {
            path,
            body: entity_body(e),
            matrix: model_matrix(o, angles, obj.scale),
            fallback: state.assets.entity_bounds(e).filter(|b| !b.is_empty()),
        });
        let reach = Aabb::new(o, o).expand(MODEL_REACH * obj.scale.max(1.0));
        obj.hint = state.assets.entity_bounds(e).map_or(reach, |b| b.union(&Aabb::new(o, o).expand(64.0)));
        obj.hint = obj.hint.union(&Aabb::new(o, o).expand(256.0 * obj.scale.max(1.0)));
        return Some(obj);
    }
    let o = origin.unwrap_or(DVec3::ZERO);
    let b = state.assets.entity_bounds(e).filter(|b| !b.is_empty()).unwrap_or_else(|| Aabb::new(o, o).expand(8.0));
    obj.meshes.push(TriMesh::convex_box(b.min, b.max, 0));
    obj.hint = b;
    Some(obj)
}

pub(super) fn obj_for(state: &AppState, r: ObjRef, include_tools: bool) -> Option<Obj> {
    match r {
        ObjRef::Solid(id) => solid_obj(state, id, include_tools),
        ObjRef::Entity(id) => entity_obj(state, id, include_tools),
    }
}

pub(super) fn compound(state: &AppState, key: Key, refs: &[ObjRef], include_tools: bool) -> Option<Obj> {
    let parts: Vec<Obj> = refs.iter().filter_map(|r| obj_for(state, *r, include_tools)).collect();
    if parts.len() == 1 && !matches!(key, Key::Group(_)) {
        return parts.into_iter().next();
    }
    parts.first()?;
    let mut out = Obj {
        key,
        kind: "group",
        class: String::new(),
        meshes: Vec::new(),
        models: Vec::new(),
        origin: None,
        angles: DVec3::ZERO,
        scale: 1.0,
        hint: Aabb::EMPTY,
        refs: refs.to_vec(),
        pending: false,
        solid_ids: Vec::new(),
    };
    for p in parts {
        out.meshes.extend(p.meshes);
        out.models.extend(p.models);
        out.hint = out.hint.union(&p.hint);
        out.pending |= p.pending;
        out.solid_ids.extend(p.solid_ids);
    }
    Some(out)
}

pub(super) fn group_obj(state: &AppState, gid: u32, include_tools: bool) -> Option<Obj> {
    let scene = state.doc.scene();
    scene.group(gid)?;
    let members = scene.group_members(gid);
    compound(state, Key::Group(gid), &members, include_tools)
}

pub(super) fn moving_sets(state: &AppState, refs: &[ObjRef]) -> (IdSet, IdSet) {
    let scene = state.doc.scene();
    (scene.expand_solids(refs).into_iter().collect(), scene.expand_entities(refs).into_iter().collect())
}

pub(super) struct EnvOptions {
    pub(super) include_tools: bool,
    pub(super) include_points: bool,
    pub(super) region: Option<Aabb>,
}

pub(super) fn environment(state: &AppState, skip_solids: &IdSet, skip_entities: &IdSet, opts: &EnvOptions) -> Vec<Obj> {
    let scene = state.doc.scene();
    let near = |b: &Aabb| opts.region.is_none_or(|r| !b.is_empty() && b.intersects(&r));
    let mut out = Vec::new();
    for (id, node) in scene.solids() {
        if node.owner != Owner::World || skip_solids.contains(id) || !near(&node.bounds) {
            continue;
        }
        if let Some(o) = solid_obj(state, *id, opts.include_tools)
            && !o.meshes.is_empty()
        {
            out.push(o);
        }
    }
    for (id, node) in scene.entities() {
        if skip_entities.contains(id) {
            continue;
        }
        let e = &node.entity;
        if !opts.include_tools && non_solid_class(&e.classname) {
            continue;
        }
        if scene.is_brush_entity(*id) {
            let sids: Vec<u32> = scene.entity_solid_ids(*id).into_iter().filter(|s| !skip_solids.contains(s)).collect();
            if sids.is_empty() {
                continue;
            }
            let mut o = Obj {
                key: Key::Entity(*id),
                kind: "brush_entity",
                class: e.classname.clone(),
                meshes: Vec::new(),
                models: Vec::new(),
                origin: e.origin(),
                angles: DVec3::ZERO,
                scale: 1.0,
                hint: Aabb::EMPTY,
                refs: vec![ObjRef::Entity(*id)],
                pending: false,
                solid_ids: Vec::new(),
            };
            for sid in sids {
                let Some(sn) = scene.solid(sid) else { continue };
                if !near(&sn.bounds) {
                    continue;
                }
                let (ms, d) = brush_meshes(&sn.solid.sides, &sn.geom, &sn.disp, None, opts.include_tools);
                if d {
                    o.kind = "displacement";
                }
                o.meshes.extend(ms);
                o.solid_ids.push(sid);
                o.hint = o.hint.union(&sn.bounds);
            }
            if !o.meshes.is_empty() {
                out.push(o);
            }
            continue;
        }
        let model = model_of(state, e).is_some();
        let instance = hp_editor::instances::is_instance(e);
        let overlay = OVERLAY_CLASSES.iter().any(|c| e.classname.eq_ignore_ascii_case(c));
        if !(model || instance || (opts.include_points && !overlay)) {
            continue;
        }
        if let Some(o) = entity_obj(state, *id, opts.include_tools)
            && near(&o.hint)
        {
            out.push(o);
        }
    }
    out
}

type CacheKey = (String, u32);

struct ShapeCache {
    generation: u64,
    map: HashMap<CacheKey, Result<Arc<ModelShape>, String>>,
}

static SHAPES: Mutex<Option<ShapeCache>> = Mutex::new(None);
static PINNED: Mutex<Option<HashMap<CacheKey, Arc<ModelShape>>>> = Mutex::new(None);

const MAX_SHAPES: usize = 768;

#[cfg(test)]
pub(super) fn pin_shape(path: &str, body: u32, shape: ModelShape) {
    let mut p = PINNED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    p.get_or_insert_with(HashMap::new).insert((path.to_owned(), body), Arc::new(shape));
}

pub(super) fn cached_shape(fs: Option<&hp_fs::GameFs>, path: &str, body: u32) -> Option<Arc<ModelShape>> {
    let key = (path.to_owned(), body);
    if let Some(s) = PINNED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().and_then(|p| p.get(&key))
    {
        return Some(Arc::clone(s));
    }
    let generation = fs?.generation();
    let c = SHAPES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let cache = c.as_ref().filter(|c| c.generation == generation)?;
    cache.map.get(&key).and_then(|r| r.as_ref().ok()).cloned()
}

pub(super) fn model_shape(fs: Option<&hp_fs::GameFs>, path: &str, body: u32) -> Result<Arc<ModelShape>, String> {
    let key = (path.to_owned(), body);
    if let Some(s) = PINNED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().and_then(|p| p.get(&key))
    {
        return Ok(Arc::clone(s));
    }
    let Some(fs) = fs else { return Err("the game content is not loaded".to_owned()) };
    let generation = fs.generation();
    {
        let mut c = SHAPES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let cache = c.get_or_insert_with(|| ShapeCache { generation, map: HashMap::new() });
        if cache.generation != generation {
            cache.generation = generation;
            cache.map.clear();
        }
        if let Some(r) = cache.map.get(&key) {
            return r.clone();
        }
    }
    let loaded = hp_shape::load_shape(path, |p| fs.read(p), body).map(Arc::new);
    let mut c = SHAPES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(cache) = c.as_mut()
        && cache.generation == generation
    {
        if cache.map.len() >= MAX_SHAPES {
            cache.map.clear();
        }
        cache.map.insert(key, loaded.clone());
    }
    loaded
}

#[derive(Clone, Debug, Default)]
pub(super) struct Resolved {
    pub(super) pieces: Vec<TriMesh>,
    pub(super) notes: Vec<String>,
    pub(super) shapes: Vec<(Arc<ModelShape>, DMat4)>,
}

impl Resolved {
    pub(super) fn compound(&self) -> TriMesh {
        let mut m = TriMesh::new();
        for p in &self.pieces {
            m.append(p);
        }
        m.closed = self.pieces.len() == 1 && self.pieces[0].closed;
        m
    }

    pub(super) fn bounds(&self) -> Aabb {
        bounds_of(&self.pieces)
    }
}

pub(super) fn resolve(obj: &Obj, fs: Option<&hp_fs::GameFs>, choice: MeshChoice) -> Resolved {
    let mut out = Resolved { pieces: obj.meshes.clone(), ..Resolved::default() };
    for m in &obj.models {
        match model_shape(fs, &m.path, m.body) {
            Ok(shape) => {
                if choice == MeshChoice::Collision && !shape.collision.is_empty() {
                    out.pieces.extend(shape.collision.iter().map(|p| p.transformed(&m.matrix)));
                } else {
                    if choice == MeshChoice::Collision {
                        out.notes.push(format!("{}: no collision model (.phy), the render mesh is used", m.path));
                    }
                    out.pieces.push(shape.render.transformed(&m.matrix));
                }
                out.shapes.push((shape, m.matrix));
            }
            Err(e) => {
                out.notes.push(format!("{}: {e}; its bounding box is used", m.path));
                let o = m.matrix.transform_point3(DVec3::ZERO);
                let b = m.fallback.unwrap_or_else(|| Aabb::new(o, o).expand(8.0));
                out.pieces.push(TriMesh::convex_box(b.min, b.max, 0));
            }
        }
    }
    if obj.pending {
        out.notes.push("the instance file is still loading; call again for its full shape".to_owned());
    }
    out
}

pub(super) struct World {
    pub(super) objs: Vec<Obj>,
    pub(super) resolved: Vec<Resolved>,
}

impl World {
    pub(super) fn build(objs: Vec<Obj>, fs: Option<&hp_fs::GameFs>, choice: MeshChoice, region: Option<Aabb>) -> World {
        let mut keep = Vec::new();
        let mut resolved = Vec::new();
        for o in objs {
            let r = resolve(&o, fs, choice);
            let b = r.bounds();
            if b.is_empty() || region.is_some_and(|reg| !b.intersects(&reg)) {
                continue;
            }
            keep.push(o);
            resolved.push(r);
        }
        World { objs: keep, resolved }
    }

    pub(super) fn meshes(&self) -> (Vec<&TriMesh>, Vec<usize>) {
        let mut out = Vec::new();
        let mut owner = Vec::new();
        for (i, r) in self.resolved.iter().enumerate() {
            for p in &r.pieces {
                out.push(p);
                owner.push(i);
            }
        }
        (out, owner)
    }
}
