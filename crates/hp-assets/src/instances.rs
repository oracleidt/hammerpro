use std::path::{Path, PathBuf};
use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use glam::{DMat3, DMat4, Vec3};
use hp_app::runtime::instances::{self as rt, InstanceCache, InstanceFile, InstanceScene};
use hp_app::settings::InstanceDisplay;
use hp_editor::Id;
use hp_editor::instances::{
    ExpandOptions, Expansion, InstanceSpec, LoadRequest, Loaded, expand, is_hidden_object, is_instance,
};
use hp_geom::Aabb;
use hp_render::{MarkTri, MaterialId, MeshBatch, ModelInstance, ObjectFlags, ObjectMesh};

use crate::AssetManager;
use crate::entities::{BOX_ALPHA, WORLD_COLOR, brush_entity_color, entity_desc, entity_object};
use crate::instance_decor::{DecorPlacement, LocalDecor, PlacedDecor, local_decor, place_decor};
use crate::meshes::box_mesh;
use crate::models::entity_matrix;
use crate::producer::{DerivedBuild, DerivedObject, DerivedProducer, ProducerChanges, ProducerCx};

pub(crate) fn producers() -> Vec<Box<dyn DerivedProducer>> {
    vec![Box::new(InstanceProducer::new(InstanceCache::shared())), Box::new(ContextProducer::default())]
}

fn content_flags(display: InstanceDisplay) -> ObjectFlags {
    match display {
        InstanceDisplay::Tinted => ObjectFlags::INSTANCE,
        _ => ObjectFlags::empty(),
    }
}

const DECOR_KEY_BASE: u64 = 0x4000;

const MODEL_TINT: [f32; 4] = [0.62, 0.86, 1.0, 1.0];

#[allow(dead_code)]
pub(crate) fn entity_bounds(am: &AssetManager, e: &hp_vmf::Entity) -> Option<Aabb> {
    if !is_instance(e) || rt::scene().display == InstanceDisplay::Hidden {
        return None;
    }
    let main = rt::main_map();
    rt::instance_bounds(&InstanceCache::shared(), e, main.as_deref(), am.fgd.as_deref())
}

#[derive(Default)]
struct WireGroup {
    color: [u8; 3],
    batches: Vec<MeshBatch>,
    edges: Vec<[f32; 3]>,
    detail_edges: Vec<[f32; 3]>,
    source_edges: Vec<[f32; 3]>,
    marks: Vec<MarkTri>,
}

impl WireGroup {
    fn into_mesh(self) -> Option<ObjectMesh> {
        let WireGroup { color, batches, edges, detail_edges, source_edges, marks } = self;
        if batches.is_empty() && edges.is_empty() {
            return None;
        }
        let (aabb_min, aabb_max) = mesh_bounds(&batches, &edges);
        Some(ObjectMesh { batches, edges, detail_edges, source_edges, marks, aabb_min, aabb_max, wire_color: color })
    }
}

fn walk_marks() -> bool {
    hp_app::runtime::render::render_display().disp_walkable
}

fn group_of(groups: &mut Vec<WireGroup>, color: [u8; 3]) -> &mut WireGroup {
    let i = match groups.iter().position(|g| g.color == color) {
        Some(i) => i,
        None => {
            groups.push(WireGroup { color, ..WireGroup::default() });
            groups.len() - 1
        }
    };
    &mut groups[i]
}

struct LocalMesh {
    groups: Vec<WireGroup>,
}

#[derive(Default)]
struct LocalMeshes {
    map: AHashMap<u64, Arc<LocalMesh>>,
    walk: bool,
}

impl LocalMeshes {
    fn get(&mut self, cx: &mut ProducerCx<'_>, file: &InstanceFile, walk: bool) -> Arc<LocalMesh> {
        if self.walk != walk {
            self.map.clear();
            self.walk = walk;
        }
        if let Some(m) = self.map.get(&file.revision) {
            return Arc::clone(m);
        }
        let mut names: AHashSet<&str> = AHashSet::new();
        for s in &file.solids {
            for side in &s.node.solid.sides {
                names.insert(side.material.as_str());
            }
        }
        let ids: AHashMap<&str, MaterialId> = names.into_iter().map(|n| (n, cx.assets.world_material(n))).collect();
        let material_of = |m: &str| ids.get(m).copied().unwrap_or(0);
        let fgd = cx.assets.fgd.clone();
        let mut groups: Vec<WireGroup> = Vec::new();
        let numbers = crate::meshes::face_numbers();
        for s in &file.solids {
            let wire = match &s.owner_class {
                None => WORLD_COLOR,
                Some(class) => {
                    let e = hp_vmf::Entity {
                        classname: class.clone(),
                        editor: hp_vmf::EditorInfo { color: s.owner_color, ..hp_vmf::EditorInfo::default() },
                        ..hp_vmf::Entity::default()
                    };
                    brush_entity_color(&e, crate::entities::class_of(fgd.as_deref(), &e))
                }
            };
            let mesh = crate::meshes::solid_mesh_seams(
                &s.node,
                &|side: &hp_vmf::Side| material_of(&side.material),
                wire,
                walk,
                None,
                Some(&numbers),
            );
            let g = group_of(&mut groups, wire);
            append_batches(&mut g.batches, mesh.batches);
            g.edges.extend(mesh.edges);
            g.detail_edges.extend(mesh.detail_edges);
            g.source_edges.extend(mesh.source_edges);
            g.marks.extend(mesh.marks);
        }
        let mesh = Arc::new(LocalMesh { groups });
        if self.map.len() > 256 {
            self.map.clear();
        }
        self.map.insert(file.revision, Arc::clone(&mesh));
        mesh
    }

    fn clear(&mut self) {
        self.map.clear();
    }
}

fn place_mesh(groups: &mut Vec<WireGroup>, local: &LocalMesh, m: &DMat4, cubes: &crate::cubemaps::CubeAssign) {
    for lg in &local.groups {
        let g = group_of(groups, lg.color);
        place_batches(&mut g.batches, &lg.batches, m, cubes);
        g.edges.extend(lg.edges.iter().map(|p| place_point(m, *p)));
        g.detail_edges.extend(lg.detail_edges.iter().map(|p| place_point(m, *p)));
        g.marks.extend(lg.marks.iter().map(|t| MarkTri { p: t.p.map(|p| place_point(m, p)), class: t.class }));
    }
}

#[derive(Default)]
struct Shown {
    loading: bool,
    keys: Vec<u64>,
    signature: u64,
    models: Vec<ModelInstance>,
    lights: Vec<crate::lights::SceneLight>,
    waiting: Vec<String>,
    decor_pending: bool,
    decals: bool,
}

pub(crate) struct InstanceProducer {
    cache: InstanceCache,
    shown: AHashMap<Id, Shown>,
    dirty: AHashSet<Id>,
    meshes: LocalMeshes,
    generation: u64,
    scene: Option<InstanceScene>,
    main: Option<PathBuf>,
    lists_dirty: bool,
    walk: bool,
    cubes: Option<Arc<crate::cubemaps::CubeAssign>>,
    decor: AHashMap<u64, Option<Arc<LocalDecor>>>,
}

impl InstanceProducer {
    fn new(cache: InstanceCache) -> Self {
        Self {
            cache,
            shown: AHashMap::new(),
            dirty: AHashSet::new(),
            meshes: LocalMeshes::default(),
            generation: 0,
            scene: None,
            main: None,
            lists_dirty: false,
            walk: false,
            cubes: None,
            decor: AHashMap::new(),
        }
    }

    fn all_instances<'a>(cx: &'a ProducerCx<'a>) -> impl Iterator<Item = Id> + 'a {
        cx.doc.scene().entities().values().filter(|n| is_instance(&n.entity)).map(|n| n.entity.id)
    }

    fn update(&mut self, cx: &mut ProducerCx<'_>, id: Id, scene: InstanceScene, out: &mut DerivedBuild) {
        let entity = cx.doc.scene().entity(id).map(|n| &n.entity).filter(|e| is_instance(e));
        let Some(entity) = entity.filter(|_| scene.display != InstanceDisplay::Hidden) else {
            self.forget(id, out);
            return;
        };
        let fgd = cx.assets.fgd.clone();
        let exp = rt::expand_one(&self.cache, entity, cx.doc.path(), fgd.as_deref(), true);
        if exp.is_pending() && self.shown.contains_key(&id) {
            return;
        }
        let files: Vec<Option<Arc<InstanceFile>>> = exp.pieces.iter().map(|p| self.cache.file(&p.path)).collect();
        let walk = walk_marks();
        let signature = signature(&exp, &files, scene, cx, walk);
        if self.shown.get(&id).is_some_and(|s| {
            s.signature == signature && s.waiting.is_empty() && !s.decor_pending && s.loading == exp.is_pending()
        }) {
            return;
        }
        let mut groups: Vec<WireGroup> = Vec::new();
        let cubes = cx.assets.editor_cubes.assign();
        for (piece, file) in exp.pieces.iter().zip(&files) {
            let Some(file) = file else { continue };
            let local = self.meshes.get(cx, file, walk);
            place_mesh(&mut groups, &local, &piece.fixup.matrix, &cubes);
        }
        let mut models = Vec::new();
        let mut lights = Vec::new();
        let mut waiting = Vec::new();
        let tint = match scene.display {
            InstanceDisplay::Tinted => MODEL_TINT,
            _ => [1.0; 4],
        };
        let kinds = |c: &str, k: &str| fgd.as_deref().and_then(|f| rt::key_fixup(f, c, k));
        let mut decor_out = PlacedDecor::default();
        let show_overlays = hp_app::runtime::helpers::scene_display().overlays;
        if self.decor.len() > 256 {
            self.decor.clear();
        }
        for (piece, file) in exp.pieces.iter().zip(&files) {
            let Some(file) = file else { continue };
            let decor = self
                .decor
                .entry(file.revision)
                .or_insert_with(|| local_decor(&file.map, fgd.as_deref()).map(Arc::new))
                .clone();
            let Some(decor) = decor else { continue };
            let at = DecorPlacement {
                matrix: piece.fixup.matrix,
                fixup: Some(&piece.fixup),
                kinds: Some(&kinds),
                overlays: show_overlays,
                parent: Some(cx.doc),
            };
            place_decor(cx.assets, &file.map, &decor, &at, &mut decor_out);
        }
        for piece in &exp.pieces {
            for e in piece.point_entities(Some(&kinds), true) {
                let desc = entity_desc(&e, fgd.as_deref());
                if let Some(l) = desc.light {
                    lights.push(l);
                }
                let model = match &desc.model {
                    Some((path, skin)) if cx.opts.show_models => {
                        let m = cx.assets.render_model(cx.renderer, path, *skin);
                        if m.is_none() && model_loading(cx.assets, path) {
                            waiting.push(path.clone());
                        }
                        m
                    }
                    _ => None,
                };
                match model {
                    Some(model) => {
                        let m = entity_matrix(desc.origin, desc.angles, desc.scale);
                        let t = [desc.tint[0] * tint[0], desc.tint[1] * tint[1], desc.tint[2] * tint[2], desc.tint[3]];
                        models.push(ModelInstance {
                            model,
                            transform: m.as_mat4().to_cols_array_2d(),
                            object: entity_object(id),
                            tint: t,
                            fade: desc.fade,
                        });
                    }
                    None => {
                        let color = desc.color;
                        let [r, g, b] = desc.color;
                        let mat = cx.assets.flat_material([r, g, b, BOX_ALPHA]);
                        let (min, max) = (desc.origin + desc.bounds.0, desc.origin + desc.bounds.1);
                        let bm = box_mesh(min, max, mat, desc.sprite.is_none() || !cx.opts.show_helpers, color);
                        let g = group_of(&mut groups, color);
                        append_batches(&mut g.batches, bm.batches);
                        g.edges.extend(bm.edges);
                    }
                }
            }
        }
        let old = self.shown.remove(&id).unwrap_or_default();
        let mut keys = Vec::with_capacity(groups.len());
        for (k, group) in groups.into_iter().enumerate() {
            let Some(mesh) = group.into_mesh() else { continue };
            let key = u64::from(id) | ((k as u64) << 32);
            keys.push(key);
            out.upsert.push(DerivedObject { key, mesh, owner: Some(id), flags: content_flags(scene.display) });
        }
        for (k, mesh) in decor_out.meshes.into_iter().enumerate() {
            let key = u64::from(id) | ((DECOR_KEY_BASE + k as u64) << 32);
            keys.push(key);
            out.upsert.push(DerivedObject {
                key,
                mesh,
                owner: Some(id),
                flags: content_flags(scene.display) | ObjectFlags::NO_GAME_WIRE,
            });
        }
        for k in old.keys.iter().filter(|k| !keys.contains(k)) {
            out.remove.push(*k);
        }
        if old.models != models || old.lights != lights {
            self.lists_dirty = true;
        }
        let (decor_pending, decals) = (decor_out.pending, decor_out.decals);
        let shown =
            Shown { loading: exp.is_pending(), keys, signature, models, lights, waiting, decor_pending, decals };
        self.shown.insert(id, shown);
    }

    fn forget(&mut self, id: Id, out: &mut DerivedBuild) {
        if let Some(old) = self.shown.remove(&id) {
            out.remove.extend(old.keys);
            if !old.models.is_empty() || !old.lights.is_empty() {
                self.lists_dirty = true;
            }
        }
    }
}

impl DerivedProducer for InstanceProducer {
    fn preparing(&self) -> bool {
        self.shown.values().any(|shown| shown.loading || !shown.waiting.is_empty()) || !self.dirty.is_empty()
    }
    fn name(&self) -> &'static str {
        "instances"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        let scene = rt::scene();
        let generation = self.cache.generation();
        let main = cx.doc.path().map(Path::to_path_buf);
        if changes.full {
            self.shown.clear();
            self.meshes.clear();
            self.decor.clear();
            self.dirty.clear();
            self.lists_dirty = true;
        }
        if changes.entities {
            self.meshes.clear();
            self.decor.clear();
            for s in self.shown.values_mut() {
                s.signature = 0;
            }
        }
        let walk = walk_marks();
        let cubes = cx.assets.editor_cubes.assign();
        let cubes_changed = self.cubes.as_ref().is_none_or(|c| !Arc::ptr_eq(c, &cubes));
        let everything = changes.full
            || changes.entities
            || self.walk != walk
            || self.scene != Some(scene)
            || self.generation != generation
            || self.main != main
            || cubes_changed;
        if everything {
            self.cubes = Some(cubes);
            self.walk = walk;
            self.scene = Some(scene);
            self.generation = generation;
            self.main = main;
            self.dirty.extend(Self::all_instances(cx));
            self.dirty.extend(self.shown.keys().copied());
        } else {
            let doc = changes.doc;
            if doc.visibility_changed || doc.visgroups_changed {
                self.lists_dirty |= self.shown.values().any(|s| !s.lights.is_empty());
            }
            if !doc.solids_changed.is_empty() || !doc.solids_removed.is_empty() {
                for (id, s) in self.shown.iter_mut().filter(|(_, s)| s.decals) {
                    s.signature = 0;
                    self.dirty.insert(*id);
                }
            }
            for id in doc.entities_changed.iter().chain(&doc.entities_removed) {
                if self.shown.contains_key(id) || cx.doc.scene().entity(*id).is_some_and(|n| is_instance(&n.entity)) {
                    self.dirty.insert(*id);
                }
            }
        }
        for (id, s) in &self.shown {
            if s.decor_pending || s.waiting.iter().any(|p| !model_loading(cx.assets, p)) {
                self.dirty.insert(*id);
            }
        }
        !self.dirty.is_empty() || self.lists_dirty
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut out = DerivedBuild::default();
        let scene = self.scene.unwrap_or_else(rt::scene);
        let mut ids: Vec<Id> = self.dirty.drain().collect();
        ids.sort_unstable();
        for id in ids {
            self.update(cx, id, scene, &mut out);
        }
        if self.lists_dirty {
            self.lists_dirty = false;
            let mut ids: Vec<&Id> = self.shown.keys().collect();
            ids.sort_unstable();
            out.models = Some(ids.iter().flat_map(|id| self.shown[*id].models.iter().copied()).collect());
            out.lights = Some(
                ids.iter()
                    .filter(|id| cx.doc.is_entity_visible(***id))
                    .flat_map(|id| self.shown[*id].lights.iter().copied())
                    .collect(),
            );
        }
        out
    }
}

#[derive(Default)]
struct ContextProducer {
    meshes: LocalMeshes,
    signature: Option<u64>,
    layout: Option<u64>,
    keys: Vec<u64>,
    walk: bool,
    decor: AHashMap<u64, Option<Arc<LocalDecor>>>,
    pending: bool,
}

fn context_signature(ctx: &rt::InstanceContext, generation: u64, doc_path: Option<&Path>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ctx.parent.hash(&mut h);
    ctx.entity.hash(&mut h);
    generation.hash(&mut h);
    doc_path.hash(&mut h);
    hp_app::runtime::helpers::scene_display().overlays.hash(&mut h);
    h.finish()
}

fn context_layout(
    cx: &ProducerCx<'_>,
    cache: &InstanceCache,
    ctx: &rt::InstanceContext,
) -> Option<Vec<(Arc<InstanceFile>, DMat4)>> {
    let parent = ctx.parent.to_string_lossy().into_owned();
    let req = LoadRequest { file: &parent, containing: None, depth: 0 };
    let Loaded::Ready { path, map } = cache.load(&req, Some(&ctx.parent)) else { return None };
    let file = cache.file(&path)?;
    let inst = map.entities.iter().find(|e| e.id == ctx.entity && is_instance(e))?;
    let inverse = InstanceSpec::of(inst)?.matrix().inverse();
    if !inverse.is_finite() {
        return None;
    }
    let mut out = vec![(file, inverse)];
    let others: Vec<&hp_vmf::Entity> = map
        .entities
        .iter()
        .filter(|e| is_instance(e) && e.id != ctx.entity && !is_hidden_object(e.hidden, &e.editor))
        .collect();
    if !others.is_empty() {
        let fgd = cx.assets.fgd.clone();
        let kinds = |c: &str, k: &str| fgd.as_deref().and_then(|f| rt::key_fixup(f, c, k));
        let opts =
            ExpandOptions { skip_hidden: true, kinds: Some(&kinds), main_map: Some(&ctx.parent), first_auto_index: 1 };
        let mut source = cache.source(Some(&ctx.parent));
        for exp in expand(&others, &mut source, &opts) {
            for piece in &exp.pieces {
                if let Some(f) = cache.file(&piece.path) {
                    out.push((f, inverse * piece.fixup.matrix));
                }
            }
        }
    }
    Some(out)
}

fn layout_signature(layout: &[(Arc<InstanceFile>, DMat4)]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (f, m) in layout {
        f.revision.hash(&mut h);
        for v in m.to_cols_array() {
            v.to_bits().hash(&mut h);
        }
    }
    h.finish()
}

impl DerivedProducer for ContextProducer {
    fn name(&self) -> &'static str {
        "instance_context"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        if changes.full {
            self.keys.clear();
            self.signature = None;
            self.layout = None;
            self.meshes.clear();
            self.decor.clear();
        }
        let walk = walk_marks();
        if changes.entities || self.walk != walk {
            self.meshes.clear();
            self.decor.clear();
            self.signature = None;
            self.layout = None;
            self.walk = walk;
        }
        if self.pending {
            self.layout = None;
            return true;
        }
        let wanted = rt::context().filter(|_| rt::scene().show_context);
        let signature =
            wanted.as_ref().map(|c| context_signature(c, InstanceCache::shared().generation(), cx.doc.path()));
        signature != self.signature || (signature.is_none() && !self.keys.is_empty())
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut out = DerivedBuild::default();
        let cache = InstanceCache::shared();
        let wanted = rt::context().filter(|_| rt::scene().show_context);
        let Some(ctx) = wanted else {
            out.remove.append(&mut self.keys);
            self.pending = false;
            self.signature = None;
            self.layout = None;
            return out;
        };
        self.signature = Some(context_signature(&ctx, cache.generation(), cx.doc.path()));
        let layout = context_layout(cx, &cache, &ctx).unwrap_or_default();
        let signature = layout_signature(&layout) ^ u64::from(hp_app::runtime::helpers::scene_display().overlays);
        if self.layout == Some(signature) {
            return out;
        }
        self.layout = Some(signature);
        let mut groups: Vec<WireGroup> = Vec::new();
        let mut decor_out = PlacedDecor::default();
        let show_overlays = hp_app::runtime::helpers::scene_display().overlays;
        let fgd = cx.assets.fgd.clone();
        if self.decor.len() > 256 {
            self.decor.clear();
        }
        for (file, m) in &layout {
            let local = self.meshes.get(cx, file, self.walk);
            place_mesh(&mut groups, &local, m, &crate::cubemaps::CubeAssign::default());
            let decor = self
                .decor
                .entry(file.revision)
                .or_insert_with(|| local_decor(&file.map, fgd.as_deref()).map(Arc::new))
                .clone();
            if let Some(decor) = decor {
                let at = DecorPlacement { matrix: *m, fixup: None, kinds: None, overlays: show_overlays, parent: None };
                place_decor(cx.assets, &file.map, &decor, &at, &mut decor_out);
            }
        }
        self.pending = decor_out.pending;
        let mut keys = Vec::with_capacity(groups.len());
        for (k, group) in groups.into_iter().enumerate() {
            let Some(mesh) = group.into_mesh() else { continue };
            let key = k as u64;
            keys.push(key);
            out.upsert.push(DerivedObject { key, mesh, owner: None, flags: ObjectFlags::INSTANCE });
        }
        for (k, mesh) in decor_out.meshes.into_iter().enumerate() {
            let key = DECOR_KEY_BASE << 32 | k as u64;
            keys.push(key);
            out.upsert.push(DerivedObject {
                key,
                mesh,
                owner: None,
                flags: ObjectFlags::INSTANCE | ObjectFlags::NO_GAME_WIRE,
            });
        }
        for k in self.keys.iter().filter(|k| !keys.contains(k)) {
            out.remove.push(*k);
        }
        self.keys = keys;
        out
    }
}

fn signature(
    exp: &Expansion,
    files: &[Option<Arc<InstanceFile>>],
    scene: InstanceScene,
    cx: &ProducerCx<'_>,
    walk: bool,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    scene.display.hash(&mut h);
    walk.hash(&mut h);
    (cx.opts.show_models, cx.opts.show_helpers, hp_app::runtime::helpers::scene_display().overlays).hash(&mut h);
    cx.assets.fgd.as_ref().map(|f| Arc::as_ptr(f) as usize).hash(&mut h);
    (Arc::as_ptr(&cx.assets.editor_cubes.assign()) as usize).hash(&mut h);
    for (p, f) in exp.pieces.iter().zip(files) {
        f.as_ref().map(|f| f.revision).hash(&mut h);
        for v in p.fixup.matrix.to_cols_array() {
            v.to_bits().hash(&mut h);
        }
        p.fixup.name.hash(&mut h);
        p.fixup.replaces.hash(&mut h);
    }
    exp.pieces.len().hash(&mut h);
    h.finish()
}

fn append_batches(dst: &mut Vec<MeshBatch>, src: Vec<MeshBatch>) {
    for b in src {
        match dst.iter_mut().find(|d| d.material == b.material) {
            Some(d) => {
                let base = u32::try_from(d.vertices.len()).unwrap_or(u32::MAX);
                if !d.cubemaps.is_empty() || !b.cubemaps.is_empty() {
                    d.cubemaps.resize(d.vertices.len(), hp_render::NO_CUBEMAP);
                    let mut c = b.cubemaps;
                    c.resize(b.vertices.len(), hp_render::NO_CUBEMAP);
                    d.cubemaps.extend(c);
                }
                d.vertices.extend(b.vertices);
                d.indices.extend(b.indices.iter().map(|i| base.saturating_add(*i)));
            }
            None => dst.push(b),
        }
    }
}

fn place_point(m: &DMat4, p: [f32; 3]) -> [f32; 3] {
    m.transform_point3(Vec3::from(p).as_dvec3()).as_vec3().to_array()
}

fn place_batches(dst: &mut Vec<MeshBatch>, src: &[MeshBatch], m: &DMat4, cubes: &crate::cubemaps::CubeAssign) {
    let rot = DMat3::from_mat4(*m);
    let placed: Vec<MeshBatch> = src
        .iter()
        .map(|b| MeshBatch {
            material: b.material,
            vertices: b
                .vertices
                .iter()
                .map(|v| {
                    let mut v = *v;
                    v.pos = place_point(m, v.pos);
                    v.normal = (rot * Vec3::from(v.normal).as_dvec3()).as_vec3().to_array();
                    v
                })
                .collect(),
            indices: b.indices.clone(),
            blend: Vec::new(),
            cubemaps: b.cubemaps.clone(),
        })
        .collect();
    let mut placed = placed;
    crate::meshes::assign_placed_faces(&mut placed, cubes);
    append_batches(dst, placed);
}

fn mesh_bounds(batches: &[MeshBatch], edges: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in batches.iter().flat_map(|b| b.vertices.iter().map(|v| v.pos)).chain(edges.iter().copied()) {
        let p = Vec3::from(p);
        if p.is_finite() {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if lo.cmple(hi).all() { (lo.to_array(), hi.to_array()) } else { ([0.0; 3], [0.0; 3]) }
}

fn model_loading(am: &AssetManager, path: &str) -> bool {
    matches!(am.models.map.get(path), Some(crate::models::ModelEntry::Loading))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use hp_render::Vertex;

    fn batch(material: MaterialId, n: usize) -> MeshBatch {
        MeshBatch {
            material,
            vertices: vec![Vertex { pos: [1.0, 0.0, 0.0], normal: [1.0, 0.0, 0.0], uv: [3.0, 4.0], alpha: 0.0 }; n],
            indices: (0..n as u32).collect(),
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }
    }

    #[test]
    fn batches_merge_and_move() {
        let mut dst = vec![batch(1, 3)];
        append_batches(&mut dst, vec![batch(1, 3), batch(2, 3)]);
        assert_eq!(dst.len(), 2);
        assert_eq!(dst[0].vertices.len(), 6);
        assert_eq!(dst[0].indices[3..], [3, 4, 5]);
        let m =
            DMat4::from_mat3_translation(hp_geom::angle_matrix(DVec3::new(0.0, 90.0, 0.0)), DVec3::new(10.0, 0.0, 0.0));
        let mut placed = Vec::new();
        place_batches(&mut placed, &[batch(5, 1)], &m, &crate::cubemaps::CubeAssign::default());
        let v = placed[0].vertices[0];
        assert!((Vec3::from(v.pos) - Vec3::new(10.0, 1.0, 0.0)).length() < 1e-5, "{:?}", v.pos);
        assert!((Vec3::from(v.normal) - Vec3::Y).length() < 1e-5);
        assert_eq!(v.uv, [3.0, 4.0], "textures follow the brush");
        assert_eq!(mesh_bounds(&placed, &[]), ([10.0, 1.0, 0.0], [10.0, 1.0, 0.0]));
        assert_eq!(mesh_bounds(&[], &[]), ([0.0; 3], [0.0; 3]));
    }

    #[test]
    fn displacement_lines_and_marks_follow_the_instance() {
        let class = hp_render::WalkClass::Walkable;
        let local = LocalMesh {
            groups: vec![WireGroup {
                color: [1, 2, 3],
                batches: vec![batch(1, 3)],
                edges: vec![[0.0; 3], [1.0, 0.0, 0.0]],
                detail_edges: vec![[0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
                source_edges: Vec::new(),
                marks: vec![MarkTri { p: [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], class }],
            }],
        };
        let m = DMat4::from_translation(DVec3::new(100.0, 0.0, 0.0));
        let mut groups = Vec::new();
        place_mesh(&mut groups, &local, &m, &crate::cubemaps::CubeAssign::default());
        let mesh = groups.pop().and_then(WireGroup::into_mesh).expect("mesh");
        assert_eq!(mesh.detail_edges, vec![[100.0, 1.0, 0.0], [101.0, 1.0, 0.0]], "inner lines kept and moved");
        assert_eq!(mesh.marks.len(), 1);
        assert_eq!(mesh.marks[0].p[1], [101.0, 0.0, 0.0]);
        assert_eq!(mesh.marks[0].class, class);
        assert_eq!(mesh.wire_color, [1, 2, 3]);
        assert!(WireGroup::default().into_mesh().is_none());
    }
}
