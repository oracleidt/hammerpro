use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ahash::{AHashMap, AHashSet};
use hp_app::runtime::helpers::scene_display;
use hp_app::runtime::render::{RenderDisplay, render_display};
use hp_app::settings::StaticPropShadows;
use hp_app::state::{LightPreviewReport, SceneOptions, SunReport};
use hp_editor::{Changes, Document, Id, ObjRef, Owner, SolidNode};
use hp_render::{
    LightStats, MaterialId, ModelId, ModelInstance, ModelPart, ObjectFlags, ObjectMesh, PreviewLight, Renderer,
    SpriteInstance, SunParams,
};
use rayon::prelude::*;

use crate::AssetManager;
use crate::decals::DecalProducer;
use crate::entities::{
    BOX_ALPHA, DEFAULT_EDITOR_COLOR, EntityDesc, WORLD_COLOR, brush_entity_color, class_of, entity_desc, entity_object,
    object_entity, render_tint,
};
use crate::material::MatKind;
use crate::meshes::{SeamNormals, box_mesh, solid_mesh_seams};
use crate::models::{RenderModel, entity_matrix, model_data_key};
use crate::producer::{Derived, DerivedProducer, ProducerChanges};
use crate::render::{CollisionState, EnvState, SkyRoom};

const PARALLEL_SOLIDS: usize = 32;
const DEFAULT_AMBIENT: [f32; 3] = [0.06, 0.06, 0.07];
pub(crate) const PROGRESSIVE_MIN: usize = 16_384;
const UPLOAD_BATCH: usize = 256;
const PREP_SLICE: usize = 2_048;
const UPLOAD_BUDGET: Duration = Duration::from_millis(6);
const NO_TINT: [f32; 4] = [1.0; 4];
const CUBEMAP_BUDGET: Duration = Duration::from_millis(3);
pub(crate) const MAX_PARKED: usize = 8;

pub(crate) struct EntState {
    pub desc: EntityDesc,
    pub model: Option<ModelId>,
    pub sprite: Option<MaterialId>,
}

pub(crate) fn producers() -> Vec<Box<dyn DerivedProducer>> {
    let mut list: Vec<Box<dyn DerivedProducer>> = vec![Box::new(DecalProducer::default())];
    list.extend(crate::helpers::producers());
    list.extend(crate::instances::producers());
    list.extend(crate::render::producers());
    list.extend(crate::lights::producers());
    list
}

#[derive(Default)]
pub(crate) struct SceneSync {
    pub opts: Option<SceneOptions>,
    pub selected: AHashSet<u32>,
    pub ents: AHashMap<Id, EntState>,
    pub derived: Derived,
    pub instances_dirty: bool,
    pub sprites_dirty: bool,
    pub lights_dirty: bool,
    pub force_entities: bool,
    pub initialized: bool,
    pub display: Option<RenderDisplay>,
    pub editor_objects: Option<bool>,
    pub game_look: bool,
    pub upload_queue: VecDeque<Id>,
    pub upload_pending: AHashSet<Id>,
    pub upload_total: usize,
    pub prep: Option<UploadPrep>,
    pub env: EnvState,
    pub collision: CollisionState,
    pub shadow_hulls: crate::shadow_hulls::ShadowHulls,
    pub no_progressive: bool,
    pub revision: Option<u64>,
    pub epoch_floor: u64,
    pub cube_assign: Option<Arc<crate::cubemaps::CubeAssign>>,
    pub buried_lights: AHashSet<Id>,
    pub buried_pending: bool,
    pub light_targets: AHashSet<String>,
    pub lighting_origin_names: AHashSet<String>,
    pub light_mode: Option<(bool, bool)>,
    pub light_report: LightPreviewReport,
}

fn light_report(stats: LightStats, hdr: bool, sun: Option<SunReport>) -> LightPreviewReport {
    LightPreviewReport {
        lights: stats.lights,
        directional: stats.directional,
        left_out: stats.left_out,
        crowded_clusters: stats.crowded_clusters,
        max_per_cluster: hp_render::MAX_PER_CLUSTER,
        sky_occlusion: stats.sky_occlusion,
        sky_layers: stats.sky_layers,
        hdr,
        sun,
    }
}

#[derive(Default)]
pub(crate) struct UploadPrep {
    ids: Vec<Id>,
    next: usize,
    keyed: Vec<([i32; 9], Id)>,
    tris: AHashMap<MaterialId, u64>,
}

pub(crate) struct ParkedSync {
    revision: u64,
    sync: SceneSync,
    render: hp_render::ParkedScene,
    generation: u64,
}

impl ParkedSync {
    pub(crate) fn render_bytes(&self) -> u64 {
        self.render.gpu_bytes()
    }

    pub(crate) fn drop_in_background(self) {
        self.render.drop_in_background();
    }
}

fn solid_flags(doc: &Document, id: Id, d: &RenderDisplay, room: Option<&SkyRoom>) -> ObjectFlags {
    let mut f = ObjectFlags::empty();
    if room.is_some_and(|r| r.solids.contains(&id)) {
        f |= ObjectFlags::SKY_3D;
    }
    if doc.is_solid_selected(id) {
        f |= ObjectFlags::SELECTED;
    }
    if !doc.is_solid_visible(id) {
        f |= ObjectFlags::HIDDEN;
    }
    if doc.is_solid_cordoned_out(id) {
        f |= ObjectFlags::DIMMED;
    }
    if let Some(n) = doc.scene().solid(id) {
        let has_disp = n.disp.iter().any(Option::is_some);
        if n.solid.sides.iter().any(|side| crate::lights::is_sky_material(&side.material)) {
            f |= ObjectFlags::NO_SHADOW;
        }
        if let Owner::Entity(e) = n.owner
            && let Some(en) = doc.scene().entity(e)
        {
            if !en.entity.classname.eq_ignore_ascii_case("func_detail") {
                f |= ObjectFlags::BRUSH_ENTITY;
            }
            if crate::lights::brush_casts_no_shadow(&en.entity) {
                f |= ObjectFlags::NO_SHADOW;
            }
            if !d.fog_volume_texture && crate::fog_volumes::is_fog_volume(&en.entity.classname) {
                f |= ObjectFlags::NO_3D;
            }
        }
        if d.highlight_invalid && !n.geom.valid {
            f |= ObjectFlags::INVALID;
        }
        if has_disp {
            f |= ObjectFlags::DISPLACEMENT;
            if !d.displacements {
                f |= ObjectFlags::HIDDEN;
            }
            if !d.displacements_in_2d {
                f |= ObjectFlags::NO_2D;
            }
        } else if d.displacements_only {
            f |= ObjectFlags::HIDDEN;
        }
    }
    f
}

fn disp_neighbours(doc: &Document, ids: &[Id]) -> Vec<Id> {
    let scene = doc.scene();
    let given: AHashSet<Id> = ids.iter().copied().collect();
    let mut out: Vec<Id> = Vec::new();
    for id in ids {
        let Some(n) = scene.solid(*id).filter(|n| n.has_disp()) else { continue };
        for other in doc.solids_in_box(&n.bounds.expand(1.0)) {
            if !given.contains(&other) && !out.contains(&other) && scene.solid(other).is_some_and(SolidNode::has_disp) {
                out.push(other);
            }
        }
    }
    out
}

fn seam_normals(doc: &Document, nodes: &[(Id, &SolidNode)]) -> Option<SeamNormals> {
    if !nodes.iter().any(|(_, n)| n.has_disp()) {
        return None;
    }
    let scene = doc.scene();
    let mut seams = SeamNormals::default();
    let mut seen: AHashSet<Id> = AHashSet::new();
    for (_, n) in nodes {
        if !n.has_disp() {
            continue;
        }
        for other in doc.solids_in_box(&n.bounds.expand(1.0)) {
            if !seen.insert(other) {
                continue;
            }
            if let Some(o) = scene.solid(other) {
                for m in o.disp.iter().flatten() {
                    seams.add(m);
                }
            }
        }
    }
    Some(seams)
}

pub(crate) fn entity_flags(doc: &Document, id: Id) -> ObjectFlags {
    let mut f = ObjectFlags::empty();
    if doc.selection().contains(ObjRef::Entity(id)) {
        f |= ObjectFlags::SELECTED;
    }
    if !doc.is_entity_visible(id) {
        f |= ObjectFlags::HIDDEN;
    }
    if doc.is_entity_cordoned_out(id) {
        f |= ObjectFlags::DIMMED;
    }
    f
}

fn point_entity_flags(doc: &Document, id: Id, room: Option<&SkyRoom>, props: StaticPropShadows) -> ObjectFlags {
    let mut f = entity_flags(doc, id);
    if room.is_some_and(|r| r.entities.contains(&id)) {
        f |= ObjectFlags::SKY_3D;
    }
    if let Some(n) = doc.scene().entity(id) {
        if crate::lights::casts_no_shadow(&n.entity) || !crate::lights::casts_game_shadow(&n.entity) {
            f |= ObjectFlags::NO_SHADOW;
        }
        if n.entity.classname.eq_ignore_ascii_case("prop_static") {
            match props {
                StaticPropShadows::None => f |= ObjectFlags::NO_SHADOW,
                StaticPropShadows::Collision => f |= ObjectFlags::HULL_SHADOW,
                StaticPropShadows::Mesh => {}
            }
        }
        if hp_app::runtime::helpers::fish::is_fish_pool(&n.entity.classname) {
            f |= ObjectFlags::NO_PROBE;
        }
    }
    f
}

fn flags_of(
    doc: &Document,
    derived: &Derived,
    rid: u32,
    d: &RenderDisplay,
    room: Option<&SkyRoom>,
) -> Option<ObjectFlags> {
    if let Some(flags) = derived.flags_of(doc, rid) {
        return flags;
    }
    match object_entity(rid) {
        Some(id) => doc.scene().entity(id).is_some().then(|| point_entity_flags(doc, id, room, d.prop_shadows)),
        None => doc.scene().solid(rid).is_some().then(|| solid_flags(doc, rid, d, room)),
    }
}

fn selected_objects(doc: &Document) -> AHashSet<u32> {
    let scene = doc.scene();
    let sel = doc.selection();
    let mut out = AHashSet::with_capacity(sel.objects.len());
    for obj in &sel.objects {
        match *obj {
            ObjRef::Solid(id) => {
                out.insert(id);
            }
            ObjRef::Entity(e) => {
                if scene.is_brush_entity(e) {
                    out.extend(scene.entity_solid_ids(e));
                } else {
                    out.insert(entity_object(e));
                }
            }
        }
    }
    out
}

fn visgroup_color(doc: &Document, node: &SolidNode) -> Option<[u8; 3]> {
    let own = node.solid.editor.visgroup_ids.first();
    let from_entity = || match node.owner {
        Owner::Entity(e) => doc.scene().entity(e).and_then(|n| n.entity.editor.visgroup_ids.first().copied()),
        Owner::World => None,
    };
    own.copied().or_else(from_entity).and_then(|id| doc.visgroup(id)).map(|g| g.color)
}

fn solid_look(
    doc: &Document,
    node: &SolidNode,
    fgd: Option<&hp_fgd::ResolvedFgd>,
    d: &RenderDisplay,
) -> ([u8; 3], [f32; 4]) {
    let (wire, tint) = match node.owner {
        Owner::World => (WORLD_COLOR, NO_TINT),
        Owner::Entity(e) => doc.scene().entity(e).map_or((DEFAULT_EDITOR_COLOR, NO_TINT), |n| {
            let tint = if d.render_effects { render_tint(&n.entity) } else { NO_TINT };
            (brush_entity_color(&n.entity, class_of(fgd, &n.entity)), tint)
        }),
    };
    let wire = if d.visgroup_colors { visgroup_color(doc, node).unwrap_or(wire) } else { wire };
    (wire, tint)
}

fn object_color(doc: &Document, node: &SolidNode, classic: [u8; 3]) -> [u8; 3] {
    hp_editor::display_color::solid_chosen_color(doc.scene(), node).unwrap_or(classic)
}

pub(crate) fn smoothing_color(bits: u32) -> [u8; 4] {
    if bits == 0 {
        return [150, 150, 150, 255];
    }
    let h = (bits.wrapping_mul(2_654_435_761) >> 8) as f32 / (1u32 << 24) as f32 * 6.0;
    let (s, v) = (0.6f32, 0.95f32);
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    [c(r), c(g), c(b), 255]
}

fn spatial_key(n: &SolidNode) -> [i32; 9] {
    let cell = |v: f64, size: f64| if v.is_finite() { (v / size).floor().clamp(-1.0e6, 1.0e6) as i32 } else { 0 };
    let c = if n.bounds.is_empty() { glam::DVec3::ZERO } else { n.bounds.center() };
    let mut key = [0i32; 9];
    for (level, size) in [65536.0, 8192.0, 2048.0].into_iter().enumerate() {
        for axis in 0..3 {
            key[level * 3 + axis] = cell(c[axis], size);
        }
    }
    key
}

struct BuiltSolid {
    id: Id,
    mesh: ObjectMesh,
    wire: [u8; 3],
    classic: [u8; 3],
    flags: ObjectFlags,
    tint: [f32; 4],
}

fn transform_mesh(mesh: &mut hp_render::ObjectMesh, m: &glam::DMat4) {
    let point = |p: [f32; 3]| m.transform_point3(glam::DVec3::from(p.map(f64::from))).as_vec3().to_array();
    let mut min = glam::Vec3::splat(f32::MAX);
    let mut max = glam::Vec3::splat(f32::MIN);
    for b in &mut mesh.batches {
        for v in &mut b.vertices {
            v.pos = point(v.pos);
            let n = m.transform_vector3(glam::DVec3::from(v.normal.map(f64::from))).normalize_or_zero();
            v.normal = n.as_vec3().to_array();
            min = min.min(glam::Vec3::from(v.pos));
            max = max.max(glam::Vec3::from(v.pos));
        }
    }
    for e in mesh.edges.iter_mut().chain(mesh.detail_edges.iter_mut()) {
        *e = point(*e);
        min = min.min(glam::Vec3::from(*e));
        max = max.max(glam::Vec3::from(*e));
    }
    for t in &mut mesh.marks {
        t.p = t.p.map(point);
    }
    if min.x <= max.x {
        mesh.aabb_min = min.to_array();
        mesh.aabb_max = max.to_array();
    }
}

impl AssetManager {
    pub(crate) fn sync_scene_impl(&mut self, doc: &mut Document, renderer: &mut Renderer, opts: &SceneOptions) {
        self.note_gpu(renderer);
        self.poll_results();
        let restored =
            (doc.pending_changes().everything || !self.scene.initialized) && self.restore_scene(doc, renderer);
        let prev = self.scene.opts;
        self.mats.set_hide_tools(!opts.show_tool_textures);
        if prev.is_none_or(|p| p.show_models != opts.show_models || p.show_helpers != opts.show_helpers) {
            self.scene.force_entities = true;
        }
        if prev.is_none_or(|p| p.lighting_preview != opts.lighting_preview) {
            self.scene.lights_dirty = true;
        }
        self.scene.opts = Some(*opts);
        let display = render_display();
        let scene_display = scene_display();
        let editor_objects = scene_display.editor_objects && !scene_display.game_look;
        let prev_display = self.scene.display.replace(display);
        if self.scene.editor_objects.replace(editor_objects).is_some_and(|e| e != editor_objects) {
            self.scene.force_entities = true;
        }
        if std::mem::replace(&mut self.scene.game_look, scene_display.game_look) != scene_display.game_look {
            self.scene.force_entities = true;
        }
        let remesh_disps = prev_display.is_some_and(|p| p.disp_walkable != display.disp_walkable);
        let (remesh, reflag) = match prev_display {
            Some(p) => (
                p.smoothing_groups != display.smoothing_groups || p.visgroup_colors != display.visgroup_colors,
                p.highlight_invalid != display.highlight_invalid
                    || p.prop_shadows != display.prop_shadows
                    || p.fog_volume_texture != display.fog_volume_texture
                    || p.displacements != display.displacements
                    || p.displacements_in_2d != display.displacements_in_2d
                    || p.displacements_only != display.displacements_only,
            ),
            None => (false, false),
        };
        if prev_display.is_some_and(|p| {
            p.render_effects != display.render_effects || p.object_colors_3d != display.object_colors_3d
        }) {
            self.scene.force_entities = true;
        }
        if prev_display.is_some_and(|p| p.hidden_tools != display.hidden_tools) {
            self.refresh_tool_materials();
        }
        if prev_display.is_some_and(|p| p.normal_maps != display.normal_maps) {
            self.refresh_bump_materials(renderer, display.normal_maps);
        }
        if prev_display.is_some_and(|p| p.cubemaps != display.cubemaps) {
            self.refresh_envmap_materials(renderer, display.cubemaps);
        }
        let entities_forced = self.scene.force_entities;

        let mut changes = doc.take_changes();
        if restored {
            changes.everything = false;
        }
        let doc: &Document = doc;
        let full = changes.everything || !self.scene.initialized;
        let cube_rebuild = self.sync_editor_cubes(doc, renderer, &changes, full || restored);
        if full {
            self.full_rebuild(doc, renderer, opts, &changes, &display);
        } else {
            if !cube_rebuild.is_empty() {
                let removed: AHashSet<Id> = changes.solids_removed.iter().copied().collect();
                let ids: Vec<Id> = cube_rebuild.into_iter().filter(|id| !removed.contains(id)).collect();
                self.rebuild_solids(doc, renderer, &ids, &display);
            }
            for &id in &changes.solids_removed {
                renderer.remove_object(id);
                self.scene.selected.remove(&id);
                self.scene.upload_pending.remove(&id);
            }
            if remesh {
                let ids: Vec<Id> = doc.scene().solids().keys().copied().collect();
                self.rebuild_solids(doc, renderer, &ids, &display);
            } else if remesh_disps {
                let scene = doc.scene();
                let ids: Vec<Id> = scene
                    .solids()
                    .iter()
                    .filter(|(id, n)| changes.solids_changed.contains(*id) || n.disp.iter().any(Option::is_some))
                    .map(|(id, _)| *id)
                    .collect();
                self.rebuild_solids(doc, renderer, &ids, &display);
            } else if !changes.solids_changed.is_empty() {
                let mut ids: Vec<Id> = changes.solids_changed.iter().copied().collect();
                let more = disp_neighbours(doc, &ids);
                ids.extend(more);
                self.rebuild_solids(doc, renderer, &ids, &display);
            }
            for &id in &changes.entities_removed {
                self.remove_entity(renderer, id);
            }
            if !self.scene.force_entities {
                for &id in &changes.entities_changed {
                    self.update_entity(doc, renderer, id, opts, &display);
                }
            }
            if changes.visibility_changed || changes.visgroups_changed || changes.groups_changed || reflag {
                self.refresh_all_flags(doc, renderer, &display);
            } else if changes.selection_changed {
                self.sync_selection(doc, renderer, &display);
            }
            if changes.visgroups_changed && display.visgroup_colors && !remesh {
                self.refresh_wire_colors(doc, renderer, &display);
            }
        }
        if std::mem::take(&mut self.game_hidden_dirty) {
            renderer.set_game_hidden(self.game_hidden.iter().copied().collect());
        }
        if std::mem::take(&mut self.play_hidden_dirty) {
            renderer.set_play_hidden(self.play_hidden.iter().copied().collect());
        }
        self.sync_play_world(doc, renderer, full);
        self.sync_light_styles(renderer);
        self.poll_detail_models();
        let moved = std::mem::take(&mut self.play_dirty);
        for id in &moved {
            if let Some(&object) = self.play_world.entity_blocks.get(id) {
                let m = self.play_transforms.get(id).map(|m| m.as_mat4().to_cols_array_2d());
                renderer.set_compiled_transform(object, m);
            }
        }
        if !moved.is_empty() && !full {
            let ids: Vec<Id> = doc
                .scene()
                .solids()
                .iter()
                .filter(|(_, n)| matches!(n.owner, Owner::Entity(e) if moved.contains(&e)))
                .map(|(id, _)| *id)
                .collect();
            if !ids.is_empty() {
                self.rebuild_solids(doc, renderer, &ids, &display);
            }
            self.scene.instances_dirty = true;
            let derived = &self.scene.derived;
            let ents = &self.scene.ents;
            let owned = |id: &Id| !derived.owned(*id).is_empty();
            if moved.iter().any(|id| ents.get(id).is_some_and(|s| s.sprite.is_some()) || owned(id)) {
                self.scene.sprites_dirty = true;
            }
            if moved.iter().any(|id| ents.get(id).is_some_and(|s| s.desc.light.is_some())) {
                self.scene.lights_dirty = true;
            }
        }
        let map_changed = full
            || changes.world_changed
            || !changes.solids_changed.is_empty()
            || !changes.solids_removed.is_empty()
            || !changes.entities_changed.is_empty()
            || !changes.entities_removed.is_empty();
        let (sky_solids, sky_entities) = self.sync_environment(doc, renderer, &display, map_changed);
        let shown_changed = changes.visibility_changed || changes.visgroups_changed || changes.groups_changed;
        self.sync_fog_volumes(doc, renderer, &display, map_changed || shown_changed);
        let room = self.scene.env.room.as_ref();
        for id in sky_solids {
            if doc.scene().solid(id).is_some() {
                renderer.set_object_flags(id, solid_flags(doc, id, &display, room));
            }
        }
        for id in sky_entities {
            if self.scene.ents.contains_key(&id) {
                renderer.set_object_flags(entity_object(id), point_entity_flags(doc, id, room, display.prop_shadows));
            }
        }
        self.advance_upload(doc, renderer, &display);
        if self.scene.force_entities {
            self.scene.force_entities = false;
            let ids: Vec<Id> = doc.scene().entities().keys().copied().collect();
            for id in ids {
                self.update_entity(doc, renderer, id, opts, &display);
            }
        }
        self.realize_loaded_models(doc, renderer, opts, &display);
        self.request_model_lods();
        self.apply_model_lods(renderer);
        if !full {
            let pc = ProducerChanges { doc: &changes, full: false, entities: entities_forced };
            self.apply_producers(doc, renderer, opts, &pc);
            let owners: Vec<Id> = changes.entities_changed.iter().copied().collect();
            self.scene.derived.refresh_owner_flags(doc, renderer, &owners);
        }
        self.rebuild_instances(doc, renderer);
        self.rebuild_sprites(renderer);
        if opts.lighting_preview && (map_changed || self.scene.buried_pending || self.scene.lights_dirty) {
            self.update_buried_lights(doc);
        }
        self.rebuild_lights(doc, renderer, opts, &display);
        self.sync_collision(renderer, &display);
        self.sync_shadow_hulls(renderer, &display);
        self.mats.apply_dirty(renderer);
        self.scene.revision = Some(doc.revision());
        renderer.set_cubemap_auto(display.cubemaps_auto);
        if display.cubemaps {
            renderer.step_cubemaps(CUBEMAP_BUDGET);
        }
    }

    fn sync_editor_cubes(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        changes: &Changes,
        everything: bool,
    ) -> Vec<Id> {
        let all = everything || changes.visibility_changed || changes.visgroups_changed;
        let list: Vec<Id> = changes.entities_changed.iter().chain(&changes.entities_removed).copied().collect();
        if all {
            self.editor_cubes.sync(doc, renderer, None);
        } else if !list.is_empty() {
            self.editor_cubes.sync(doc, renderer, Some(&list));
        }
        if all || !list.is_empty() || !changes.solids_changed.is_empty() || !changes.solids_removed.is_empty() {
            self.editor_cubes.sync_parallax(doc, renderer);
        }
        if everything {
            renderer.rebake_cubemaps();
        }
        let new = self.editor_cubes.assign();
        let old = match &self.scene.cube_assign {
            Some(old) if Arc::ptr_eq(old, &new) => return Vec::new(),
            Some(old) => Arc::clone(old),
            None => Arc::default(),
        };
        self.scene.cube_assign = Some(Arc::clone(&new));
        if changes.everything || !self.scene.initialized || *old == *new {
            return Vec::new();
        }
        let scene = doc.scene();
        let mut ids: Vec<Id> = scene
            .solids()
            .par_iter()
            .filter(|(_, n)| old.solid_faces(n) != new.solid_faces(n))
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub(crate) fn park_scene_impl(&mut self, doc: &mut Document, renderer: &mut Renderer) {
        if !self.scene.initialized || doc.pending_changes().everything {
            return;
        }
        if !doc.pending_changes().is_empty()
            && let Some(opts) = self.scene.opts
        {
            self.sync_scene_impl(doc, renderer, &opts);
        }
        let revision = doc.revision();
        if self.scene.revision != Some(revision) {
            return;
        }
        let mut sync = std::mem::take(&mut self.scene);
        std::mem::swap(&mut self.scene.collision, &mut sync.collision);
        self.scene.no_progressive = sync.no_progressive;
        let render = renderer.park_scene();
        self.parked.retain(|p| p.revision != revision);
        self.parked.push(ParkedSync { revision, sync, render, generation: self.generation });
        self.enforce_parked_limit();
    }

    fn restore_scene(&mut self, doc: &Document, renderer: &mut Renderer) -> bool {
        let revision = doc.revision();
        if self.scene.initialized && self.scene.revision == Some(revision) {
            return false;
        }
        let Some(i) = self.parked.iter().position(|p| p.revision == revision) else { return false };
        let ParkedSync { sync, render, generation, .. } = self.parked.remove(i);
        let mut live = std::mem::replace(&mut self.scene, sync);
        std::mem::swap(&mut self.scene.collision, &mut live.collision);
        self.scene.no_progressive = live.no_progressive;
        renderer.restore_scene(render).drop_in_background();
        let model_gone = self.scene.ents.values().any(|e| e.model.is_some_and(|m| !renderer.has_model(m)));
        if generation != self.generation || model_gone || live.force_entities {
            self.scene.force_entities = true;
        }
        true
    }

    pub(crate) fn gc_epoch(&self) -> u64 {
        self.parked.iter().map(|p| p.sync.epoch_floor).fold(self.mats.scene_epoch, u64::min)
    }

    pub fn solids_to_upload(&self) -> usize {
        self.scene.upload_pending.len()
    }

    pub fn upload_preparing(&self) -> bool {
        self.scene.prep.is_some()
    }

    pub fn set_progressive_upload(&mut self, on: bool) {
        self.scene.no_progressive = !on;
    }

    fn apply_producers(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        opts: &SceneOptions,
        changes: &ProducerChanges<'_>,
    ) {
        let dirty = self.run_producers(doc, renderer, opts, changes);
        self.scene.sprites_dirty |= dirty.sprites;
        self.scene.instances_dirty |= dirty.models;
        self.scene.lights_dirty |= dirty.lights;
    }

    fn full_rebuild(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        opts: &SceneOptions,
        changes: &Changes,
        display: &RenderDisplay,
    ) {
        self.mats.scene_epoch += 1;
        self.scene.epoch_floor = self.mats.scene_epoch;
        renderer.park_scene().drop_in_background();
        self.scene.selected.clear();
        self.scene.ents.clear();
        self.scene.derived.clear();
        self.scene.upload_queue.clear();
        self.scene.upload_pending.clear();
        self.scene.prep = None;
        self.scene.instances_dirty = true;
        self.scene.sprites_dirty = true;
        self.scene.lights_dirty = true;
        let scene = doc.scene();
        let ids: Vec<Id> = scene.solids().keys().copied().collect();
        self.scene.upload_total = ids.len();
        if ids.len() > PROGRESSIVE_MIN && !self.scene.no_progressive {
            self.scene.upload_pending.extend(ids.iter().copied());
            self.scene.prep = Some(UploadPrep { ids, ..UploadPrep::default() });
        } else {
            let nodes: Vec<&SolidNode> = scene.solids().values().collect();
            self.ensure_world_materials(nodes.par_iter().copied());
            let built = self.build_solids(doc, &ids, display, true);
            self.upload_solids(renderer, built);
        }
        let ids: Vec<Id> = scene.entities().keys().copied().collect();
        for id in ids {
            self.update_entity(doc, renderer, id, opts, display);
        }
        self.scene.force_entities = false;
        self.scene.initialized = true;
        self.apply_producers(doc, renderer, opts, &ProducerChanges { doc: changes, full: true, entities: true });
        if self.scene.prep.is_none() {
            self.collect_garbage(renderer);
            self.collect_model_variants(renderer);
        }
    }

    fn advance_upload(&mut self, doc: &Document, renderer: &mut Renderer, display: &RenderDisplay) {
        let start = Instant::now();
        if self.scene.prep.is_some() && !self.prepare_upload(doc, renderer, display, start) {
            return;
        }
        if self.scene.upload_queue.is_empty() {
            return;
        }
        let mut batch: Vec<Id> = Vec::with_capacity(UPLOAD_BATCH);
        while !self.scene.upload_queue.is_empty() {
            batch.clear();
            while batch.len() < UPLOAD_BATCH
                && let Some(id) = self.scene.upload_queue.pop_front()
            {
                if self.scene.upload_pending.remove(&id) {
                    batch.push(id);
                }
            }
            let built = self.build_solids(doc, &batch, display, false);
            self.upload_solids(renderer, built);
            renderer.flush_on_this_thread();
            if start.elapsed() >= UPLOAD_BUDGET {
                break;
            }
        }
        if self.scene.upload_queue.is_empty() {
            self.scene.upload_pending.clear();
            renderer.set_bulk_upload(false);
        }
    }

    fn prepare_upload(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        display: &RenderDisplay,
        start: Instant,
    ) -> bool {
        let Some(mut prep) = self.scene.prep.take() else { return true };
        let scene = doc.scene();
        let mut side_mats: Vec<MaterialId> = Vec::new();
        while prep.next < prep.ids.len() {
            let end = (prep.next + PREP_SLICE).min(prep.ids.len());
            for i in prep.next..end {
                let id = prep.ids[i];
                if !self.scene.upload_pending.contains(&id) {
                    continue;
                }
                let Some(n) = scene.solid(id) else { continue };
                prep.keyed.push((spatial_key(n), id));
                side_mats.clear();
                let mut last: Option<(&str, MaterialId)> = None;
                for side in &n.solid.sides {
                    let m = match last {
                        Some((name, m)) if name == side.material => m,
                        _ => self.world_material(&side.material),
                    };
                    last = Some((side.material.as_str(), m));
                    side_mats.push(m);
                }
                let has_disp = n.disp.iter().any(Option::is_some);
                for face in &n.geom.faces {
                    let Some(&m) = side_mats.get(face.side_index) else { continue };
                    let tris = if has_disp {
                        n.disp_mesh(face.side_index).map_or(0, |d| d.indices.len() / 3)
                    } else {
                        face.vertices.len().saturating_sub(2)
                    };
                    *prep.tris.entry(m).or_default() += tris as u64;
                }
            }
            prep.next = end;
            if prep.next < prep.ids.len() && start.elapsed() >= UPLOAD_BUDGET {
                self.scene.prep = Some(prep);
                return false;
            }
        }
        if !display.smoothing_groups {
            let hints: Vec<(MaterialId, u64)> = prep.tris.into_iter().collect();
            renderer.hint_material_triangles(&hints);
        }
        renderer.set_bulk_upload(true);
        prep.keyed.sort_unstable();
        self.scene.upload_queue.extend(prep.keyed.into_iter().map(|(_, id)| id));
        self.collect_garbage(renderer);
        self.collect_model_variants(renderer);
        true
    }

    fn ensure_world_materials<'a>(&mut self, solids: impl ParallelIterator<Item = &'a SolidNode>) {
        let names: AHashSet<&'a str> = {
            solids
                .fold(AHashSet::new, |mut set, n| {
                    for side in &n.solid.sides {
                        set.insert(side.material.as_str());
                    }
                    set
                })
                .reduce(AHashSet::new, |mut a, b| {
                    a.extend(b);
                    a
                })
        };
        for name in names {
            self.world_material(name);
        }
    }

    fn build_solids(&mut self, doc: &Document, ids: &[Id], display: &RenderDisplay, parallel: bool) -> Vec<BuiltSolid> {
        let scene = doc.scene();
        let nodes: Vec<(Id, &SolidNode)> = ids.iter().filter_map(|id| scene.solid(*id).map(|n| (*id, n))).collect();
        if nodes.is_empty() {
            return Vec::new();
        }
        if parallel {
            self.ensure_world_materials(nodes.par_iter().map(|(_, n)| *n));
        } else {
            for (_, n) in &nodes {
                for side in &n.solid.sides {
                    self.world_material(&side.material);
                }
            }
        }
        let mut smoothing: AHashMap<u32, MaterialId> = AHashMap::new();
        if display.smoothing_groups {
            for (_, n) in &nodes {
                for side in &n.solid.sides {
                    if !smoothing.contains_key(&side.smoothing_groups) {
                        let id = self.flat_material(smoothing_color(side.smoothing_groups));
                        smoothing.insert(side.smoothing_groups, id);
                    }
                }
            }
        }
        let fgd = self.fgd.clone();
        let mut looks: AHashMap<Option<Id>, ([u8; 3], [f32; 4])> = AHashMap::new();
        for (_, n) in &nodes {
            let key = match n.owner {
                Owner::World => None,
                Owner::Entity(e) => Some(e),
            };
            if !display.visgroup_colors {
                looks.entry(key).or_insert_with(|| solid_look(doc, n, fgd.as_deref(), display));
            }
        }
        let raw = &self.mats.raw_world;
        let material_of = |side: &hp_vmf::Side| {
            if display.smoothing_groups {
                smoothing.get(&side.smoothing_groups).copied().unwrap_or(0)
            } else {
                raw.get(side.material.as_str()).copied().unwrap_or(0)
            }
        };
        let room = self.scene.env.room.as_ref();
        let play = &self.play_transforms;
        let seams = seam_normals(doc, &nodes);
        let assign = self.scene.cube_assign.clone().unwrap_or_else(|| self.editor_cubes.assign());
        let cubes = |side: &hp_vmf::Side, face: &hp_geom::FaceGeom| assign.of_face(side, face);
        let build = |(id, n): &(Id, &SolidNode)| {
            let owner = match n.owner {
                Owner::World => None,
                Owner::Entity(e) => Some(e),
            };
            let (classic, tint) = match looks.get(&owner) {
                Some(l) => *l,
                None => solid_look(doc, n, fgd.as_deref(), display),
            };
            let wire = object_color(doc, n, classic);
            let flags = solid_flags(doc, *id, display, room);
            let mut mesh = solid_mesh_seams(n, &material_of, wire, display.disp_walkable, seams.as_ref(), Some(&cubes));
            if let Some(m) = owner.and_then(|e| play.get(&e)) {
                transform_mesh(&mut mesh, m);
            }
            BuiltSolid { id: *id, mesh, wire, classic, flags, tint }
        };
        if parallel && nodes.len() >= PARALLEL_SOLIDS {
            nodes.par_iter().map(build).collect()
        } else {
            nodes.iter().map(build).collect()
        }
    }

    fn upload_solids(&mut self, renderer: &mut Renderer, built: Vec<BuiltSolid>) {
        for b in built {
            renderer.upsert_object(b.id, b.mesh);
            renderer.set_object_wire_colors(b.id, b.wire, b.classic);
            renderer.set_object_flags(b.id, b.flags);
            renderer.set_object_tint(b.id, b.tint);
            if b.flags.contains(ObjectFlags::SELECTED) {
                self.scene.selected.insert(b.id);
            } else {
                self.scene.selected.remove(&b.id);
            }
        }
    }

    fn rebuild_solids(&mut self, doc: &Document, renderer: &mut Renderer, ids: &[Id], display: &RenderDisplay) {
        for id in ids {
            self.scene.upload_pending.remove(id);
        }
        let built = self.build_solids(doc, ids, display, true);
        self.upload_solids(renderer, built);
    }

    fn refresh_wire_colors(&mut self, doc: &Document, renderer: &mut Renderer, display: &RenderDisplay) {
        let fgd = self.fgd.clone();
        let looks: Vec<(Id, [u8; 3], [u8; 3])> = doc
            .scene()
            .solids()
            .par_iter()
            .map(|(id, n)| {
                let classic = solid_look(doc, n, fgd.as_deref(), display).0;
                (*id, object_color(doc, n, classic), classic)
            })
            .collect();
        for (id, wire, classic) in looks {
            renderer.set_object_wire_colors(id, wire, classic);
        }
    }

    fn remove_entity(&mut self, renderer: &mut Renderer, id: Id) {
        if let Some(old) = self.scene.ents.remove(&id) {
            self.scene.instances_dirty |= old.model.is_some();
            self.scene.sprites_dirty |= old.sprite.is_some();
            self.scene.lights_dirty |= old.desc.light.is_some() || !self.scene.light_targets.is_empty();
        }
        let obj = entity_object(id);
        renderer.remove_object(obj);
        self.scene.selected.remove(&obj);
    }

    fn update_entity(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        id: Id,
        opts: &SceneOptions,
        display: &RenderDisplay,
    ) {
        let scene = doc.scene();
        let Some(node) = scene.entity(id) else {
            self.remove_entity(renderer, id);
            return;
        };
        let fgd = self.fgd.clone();
        if let Some(name) = node.entity.get("targetname") {
            self.scene.lights_dirty |= self.scene.light_targets.contains(name);
            self.scene.instances_dirty |= self.scene.lighting_origin_names.contains(name);
        }
        if scene.is_brush_entity(id) {
            if self.scene.ents.contains_key(&id) {
                self.remove_entity(renderer, id);
            }
            for sid in scene.entity_solid_ids(id) {
                if let Some(n) = scene.solid(sid) {
                    let (classic, tint) = solid_look(doc, n, fgd.as_deref(), display);
                    renderer.set_object_wire_colors(sid, object_color(doc, n, classic), classic);
                    renderer.set_object_tint(sid, tint);
                }
            }
            return;
        }
        let mut desc = entity_desc(&node.entity, fgd.as_deref());
        if !display.render_effects {
            desc.tint[3] = 1.0;
            desc.body = 0;
            desc.anim = None;
        }
        let flags = point_entity_flags(doc, id, self.scene.env.room.as_ref(), display.prop_shadows);
        self.realize_entity(renderer, id, desc, flags, opts);
    }

    fn realize_entity(
        &mut self,
        renderer: &mut Renderer,
        id: Id,
        desc: EntityDesc,
        flags: ObjectFlags,
        opts: &SceneOptions,
    ) {
        let obj = entity_object(id);
        let game_look = self.scene.game_look;
        let model = match (&desc.model, opts.show_models) {
            (Some((path, _)), true) if game_look && path.starts_with("models/editor/") => None,
            (Some((path, skin)), true) => self.render_model_for(renderer, path, *skin, desc.body, desc.anim.as_deref()),
            _ => None,
        };
        let editor_objects = self.scene.editor_objects.unwrap_or(true);
        let sprite = match (&desc.sprite, opts.show_helpers && (editor_objects || !desc.sprite_is_icon)) {
            (Some(s), true) => {
                let kind = if desc.sprite_additive { MatKind::SpriteAdditive } else { MatKind::Sprite };
                Some(self.asset_material(s, kind))
            }
            _ => None,
        };
        if model.is_some() || game_look {
            renderer.remove_object(obj);
        } else {
            let fill = if self.scene.display.is_some_and(|d| d.object_colors_3d) { desc.color } else { desc.classic };
            let [r, g, b] = fill;
            let mat = self.flat_material([r, g, b, BOX_ALPHA]);
            let (min, max) = (desc.origin + desc.bounds.0, desc.origin + desc.bounds.1);
            renderer.upsert_object(obj, box_mesh(min, max, mat, sprite.is_none(), desc.color));
        }
        renderer.set_object_wire_colors(obj, desc.color, desc.classic);
        renderer.set_object_flags(obj, flags);
        if flags.contains(ObjectFlags::SELECTED) {
            self.scene.selected.insert(obj);
        } else {
            self.scene.selected.remove(&obj);
        }
        let lit = desc.light.is_some();
        let old = self.scene.ents.insert(id, EntState { desc, model, sprite });
        let (old_model, old_sprite, old_lit) =
            old.map_or((false, false, false), |o| (o.model.is_some(), o.sprite.is_some(), o.desc.light.is_some()));
        self.scene.instances_dirty |= old_model || model.is_some();
        self.scene.sprites_dirty |= old_sprite || sprite.is_some();
        self.scene.lights_dirty |= old_lit || lit;
    }

    pub(crate) fn render_model(&mut self, renderer: &mut Renderer, path: &str, skin: u32) -> Option<ModelId> {
        if let Some(id) = self.reuse_render_model(&(path.to_owned(), skin)) {
            return id;
        }
        let data = self.model_data(path)?;
        let skin = if (skin as usize) < data.skin_count() { skin } else { 0 };
        let key = (path.to_owned(), skin);
        if let Some(id) = self.reuse_render_model(&key) {
            return id;
        }
        let parts: Vec<ModelPart> = data
            .parts
            .iter()
            .map(|p| {
                let name = data.part_material(skin as usize, p.skin_ref).to_owned();
                ModelPart {
                    material: self.asset_material(&name, MatKind::Model),
                    vertices: p.vertices.clone(),
                    indices: p.indices.clone(),
                    tangents: p.tangents.clone(),
                }
            })
            .collect();
        let materials = parts.iter().map(|p| p.material).collect();
        let id = (!parts.is_empty()).then(|| self.create_render_model(renderer, path, &data, skin, None, parts));
        self.models.render.insert(key, RenderModel { id, materials, epoch: self.mats.scene_epoch });
        id
    }

    fn render_model_for(
        &mut self,
        renderer: &mut Renderer,
        path: &str,
        skin: u32,
        body: u32,
        anim: Option<&str>,
    ) -> Option<ModelId> {
        if body == 0 && anim.is_none() {
            return self.render_model(renderer, path, skin);
        }
        let key = model_data_key(path, anim);
        let data = self.model_data(&key)?;
        let body = data.body_key(body);
        if body == 0 && key == path {
            return self.render_model(renderer, path, skin);
        }
        let skin = if (skin as usize) < data.skin_count() { skin } else { 0 };
        let vkey = (key, skin, body);
        let epoch = self.mats.scene_epoch;
        if let Some((m, made_from)) = self.models.variants.get_mut(&vkey)
            && made_from.ptr_eq(&Arc::downgrade(&data))
        {
            m.epoch = epoch;
            let (id, materials) = (m.id, m.materials.clone());
            for mat in materials {
                self.touch_material(mat);
            }
            return id;
        }
        if let Some((old, _)) = self.models.variants.remove(&vkey)
            && let Some(id) = old.id
        {
            renderer.destroy_model(id);
        }
        let parts: Vec<ModelPart> = data
            .parts_for_body(body)
            .into_iter()
            .map(|p| {
                let name = data.part_material(skin as usize, p.skin_ref).to_owned();
                ModelPart {
                    material: self.asset_material(&name, MatKind::Model),
                    vertices: p.vertices.clone(),
                    indices: p.indices.clone(),
                    tangents: p.tangents.clone(),
                }
            })
            .collect();
        let materials = parts.iter().map(|p| p.material).collect();
        let id =
            (!parts.is_empty()).then(|| self.create_render_model(renderer, &vkey.0, &data, skin, Some(body), parts));
        self.models.variants.insert(vkey, (RenderModel { id, materials, epoch }, Arc::downgrade(&data)));
        id
    }

    fn collect_model_variants(&mut self, renderer: &mut Renderer) {
        let epoch = self.gc_epoch();
        self.models.variants.retain(|_, (m, _)| {
            if m.epoch >= epoch {
                return true;
            }
            if let Some(id) = m.id {
                renderer.destroy_model(id);
            }
            false
        });
    }

    fn reuse_render_model(&mut self, key: &(String, u32)) -> Option<Option<ModelId>> {
        let epoch = self.mats.scene_epoch;
        let m = self.models.render.get_mut(key)?;
        m.epoch = epoch;
        let (id, materials) = (m.id, m.materials.clone());
        for mat in materials {
            self.touch_material(mat);
        }
        Some(id)
    }

    fn realize_loaded_models(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        opts: &SceneOptions,
        display: &RenderDisplay,
    ) {
        if self.models.newly_ready.is_empty() {
            return;
        }
        let ready = std::mem::take(&mut self.models.newly_ready);
        if !opts.show_models {
            return;
        }
        let waits = |s: &EntState| {
            s.model.is_none()
                && s.desc
                    .model
                    .as_ref()
                    .is_some_and(|(p, _)| ready.contains(&model_data_key(p, s.desc.anim.as_deref())))
        };
        let ids: Vec<Id> = self.scene.ents.iter().filter(|(_, s)| waits(s)).map(|(id, _)| *id).collect();
        for id in ids {
            if let Some(state) = self.scene.ents.get(&id) {
                let mut desc = state.desc.clone();
                if !display.render_effects {
                    desc.tint[3] = 1.0;
                }
                let flags = point_entity_flags(doc, id, self.scene.env.room.as_ref(), display.prop_shadows);
                self.realize_entity(renderer, id, desc, flags, opts);
            }
        }
    }

    fn selected_render_objects(&self, doc: &Document) -> AHashSet<u32> {
        let mut out = selected_objects(doc);
        for obj in &doc.selection().objects {
            if let ObjRef::Entity(e) = *obj {
                out.extend(self.scene.derived.owned(e).iter().copied());
            }
        }
        out
    }

    fn sync_selection(&mut self, doc: &Document, renderer: &mut Renderer, display: &RenderDisplay) {
        let new = self.selected_render_objects(doc);
        let changed: Vec<u32> = self.scene.selected.symmetric_difference(&new).copied().collect();
        let room = self.scene.env.room.as_ref();
        for rid in changed {
            if let Some(f) = flags_of(doc, &self.scene.derived, rid, display, room) {
                renderer.set_object_flags(rid, f);
            }
        }
        self.scene.selected = new;
    }

    fn refresh_all_flags(&mut self, doc: &Document, renderer: &mut Renderer, display: &RenderDisplay) {
        let room = self.scene.env.room.as_ref();
        let flags: Vec<(Id, ObjectFlags)> =
            doc.scene().solids().par_iter().map(|(id, _)| (*id, solid_flags(doc, *id, display, room))).collect();
        for (id, f) in flags {
            renderer.set_object_flags(id, f);
        }
        let ents: Vec<Id> = self.scene.ents.keys().copied().collect();
        for id in ents {
            renderer.set_object_flags(entity_object(id), point_entity_flags(doc, id, room, display.prop_shadows));
        }
        self.scene.derived.refresh_all_flags(doc, renderer);
        self.scene.selected = self.selected_render_objects(doc);
        self.scene.lights_dirty = true;
    }

    fn rebuild_instances(&mut self, doc: &Document, renderer: &mut Renderer) {
        if !self.scene.instances_dirty {
            return;
        }
        self.scene.instances_dirty = false;
        let mut names: Option<AHashMap<String, glam::DVec3>> = None;
        let mut origins: AHashMap<u32, [f32; 3]> = AHashMap::new();
        let mut wanted: AHashSet<String> = AHashSet::new();
        for (id, s) in &self.scene.ents {
            if s.model.is_none() {
                continue;
            }
            let Some(e) = doc.scene().entity(*id).map(|n| &n.entity) else { continue };
            let Some(name) = e.get("lightingorigin").map(str::trim).filter(|n| !n.is_empty()) else { continue };
            wanted.insert(name.to_owned());
            if let Some(o) = names.get_or_insert_with(|| target_origins(doc)).get(name) {
                origins.insert(entity_object(*id), o.as_vec3().to_array());
            }
        }
        self.scene.lighting_origin_names = wanted;
        let player = self.player_instance(renderer);
        let details = self.play_detail_instances(renderer);
        let list: Vec<ModelInstance> = self
            .scene
            .ents
            .iter()
            .filter_map(|(id, s)| {
                let model = s.model?;
                if self.play_hidden.contains(&entity_object(*id)) {
                    return None;
                }
                let mut m = entity_matrix(s.desc.origin, s.desc.angles, s.desc.scale);
                if let Some(t) = self.play_transforms.get(id) {
                    m = *t * m;
                }
                Some(ModelInstance {
                    model,
                    transform: m.as_mat4().to_cols_array_2d(),
                    object: entity_object(*id),
                    tint: s.desc.tint,
                    fade: s.desc.fade,
                })
            })
            .chain(self.scene.derived.models().map(|m| {
                let mut m = *m;
                if let Some(t) = self.scene.derived.owner_of(m.object).and_then(|o| self.play_transforms.get(&o)) {
                    m.transform = (t.as_mat4() * glam::Mat4::from_cols_array_2d(&m.transform)).to_cols_array_2d();
                }
                m
            }))
            .chain(player)
            .chain(details)
            .collect();
        self.play_model_lighting(renderer, &list, &origins);
        renderer.set_model_instances(&list);
    }

    fn rebuild_sprites(&mut self, renderer: &mut Renderer) {
        if !self.scene.sprites_dirty {
            return;
        }
        self.scene.sprites_dirty = false;
        let list: Vec<SpriteInstance> = self
            .scene
            .ents
            .iter()
            .filter_map(|(id, s)| {
                let pos = self.play_transforms.get(id).map_or(s.desc.origin, |m| m.transform_point3(s.desc.origin));
                Some(SpriteInstance {
                    pos: pos.as_vec3().to_array(),
                    size: s.desc.sprite_size,
                    material: s.sprite?,
                    color: s.desc.sprite_color,
                    object: entity_object(*id),
                })
            })
            .chain(self.scene.derived.sprites().map(|sp| {
                let mut sp = *sp;
                if let Some(t) = self.scene.derived.owner_of(sp.object).and_then(|o| self.play_transforms.get(&o)) {
                    sp.pos = t.as_mat4().transform_point3(glam::Vec3::from(sp.pos)).to_array();
                }
                sp
            }))
            .collect();
        renderer.set_sprites(&list);
    }

    fn rebuild_lights(&mut self, doc: &Document, renderer: &mut Renderer, opts: &SceneOptions, d: &RenderDisplay) {
        let mode = (d.light_hdr, d.sky_occlusion);
        if self.scene.light_mode != Some(mode) {
            self.scene.light_mode = Some(mode);
            self.scene.lights_dirty = true;
        }
        if !self.scene.lights_dirty {
            let r = &self.scene.light_report;
            self.scene.light_report = light_report(renderer.light_stats(), r.hdr, r.sun);
            return;
        }
        self.scene.lights_dirty = false;
        let hdr = d.light_hdr;
        if !opts.lighting_preview {
            renderer.set_lights(&[], [0.0; 3]);
            renderer.set_sun(SunParams::default());
            self.scene.light_targets.clear();
            self.scene.light_report = light_report(renderer.light_stats(), hdr, None);
            return;
        }
        let mut ids: Vec<Id> =
            self.scene.ents.iter().filter(|(_, s)| s.desc.light.is_some()).map(|(id, _)| *id).collect();
        ids.sort_unstable();
        let mut names: Option<AHashMap<String, glam::DVec3>> = None;
        let mut targets: AHashSet<String> = AHashSet::new();
        let mut lights: Vec<PreviewLight> = Vec::with_capacity(ids.len());
        let mut sun: Option<(u8, crate::lights::SceneLight)> = None;
        for id in ids {
            let hidden = renderer.object_flags(entity_object(id)).is_some_and(|f| f.contains(ObjectFlags::HIDDEN));
            let Some(mut light) = self.scene.ents.get(&id).and_then(|s| s.desc.light) else { continue };
            if hidden {
                continue;
            }
            let node = doc.scene().entity(id);
            if let Some(e) = node.map(|n| &n.entity)
                && let Some(t) = crate::lights::target_name(e)
            {
                targets.insert(t.to_owned());
                let names = names.get_or_insert_with(|| target_origins(doc));
                if let (Some(to), Some(from)) = (names.get(t), e.origin())
                    && let Some(dir) = crate::lights::aimed_direction(from, *to)
                {
                    light = light.aimed(dir);
                }
            }
            if light.sun.is_some() {
                let rank =
                    u8::from(!node.is_some_and(|n| n.entity.classname.eq_ignore_ascii_case("light_environment")));
                if sun.is_none_or(|(r, _)| rank < r) {
                    sun = Some((rank, light));
                }
                continue;
            }
            if self.scene.buried_lights.contains(&id) {
                continue;
            }
            if let Some(m) = self.play_transforms.get(&id) {
                light = light.moved(m);
            }
            lights.extend(light.pick(hdr));
        }
        for light in self.scene.derived.lights() {
            if light.sun.is_some() {
                if sun.is_none() {
                    sun = Some((2, *light));
                }
                continue;
            }
            lights.extend(light.pick(hdr));
        }
        let sun = sun.map(|(_, s)| s);
        let ambient = sun.and_then(|s| s.ambient(hdr));
        let sun_light = sun.and_then(|s| s.pick(hdr)).filter(|l| l.intensity > 0.0 && l.color.iter().any(|c| *c > 0.0));
        if let Some(l) = sun_light {
            lights.insert(0, l);
        }
        let spread = sun.and_then(|s| s.sun).map_or(0.0, |s| s.spread_deg);
        renderer.set_lights(&lights, ambient.unwrap_or(DEFAULT_AMBIENT));
        renderer.set_sun(SunParams { spread_deg: spread, sky_ambient: d.sky_occlusion && ambient.is_some() });
        self.scene.light_targets = targets;
        let report = sun.map(|s| SunReport {
            direction: sun_light.and_then(|l| match l.kind {
                hp_render::LightKind::Directional { dir } => Some(dir),
                _ => None,
            }),
            color: sun_light.map_or([0.0; 3], |l| l.color.map(|c| c * l.intensity)),
            ambient: ambient.unwrap_or([0.0; 3]),
            spread_deg: spread,
            origin: s.pos().unwrap_or([0.0; 3]),
        });
        self.scene.light_report = light_report(renderer.light_stats(), hdr, report);
    }
}

fn target_origins(doc: &Document) -> AHashMap<String, glam::DVec3> {
    let mut nodes: Vec<(&Id, &hp_vmf::Entity)> = doc.scene().entities().iter().map(|(id, n)| (id, &n.entity)).collect();
    nodes.sort_unstable_by_key(|(id, _)| **id);
    let mut out = AHashMap::new();
    for (_, e) in nodes {
        if let (Some(name), Some(origin)) = (e.get("targetname"), e.origin().filter(|o| o.is_finite())) {
            out.entry(name.to_owned()).or_insert(origin);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solids_get_an_object_color_and_a_classic_color() {
        use hp_editor::display_color::DisplayColor;
        let mut s = hp_app::testing::test_state();
        let doc = &mut s.doc;
        let look = |doc: &Document, id: Id, d: &RenderDisplay| {
            let n = doc.scene().solid(id).unwrap();
            let classic = solid_look(doc, n, None, d).0;
            (object_color(doc, n, classic), classic)
        };
        let plain = RenderDisplay::default();
        assert_eq!(look(doc, 2, &plain), (WORLD_COLOR, WORLD_COLOR));
        assert_eq!(look(doc, 7, &plain), ([220, 30, 220], [220, 30, 220]));
        doc.set_display_color(&[ObjRef::Solid(2)], DisplayColor::Set([40, 165, 225]), false).unwrap();
        assert_eq!(look(doc, 2, &plain), ([40, 165, 225], WORLD_COLOR));
        doc.set_display_color(&[ObjRef::Entity(6)], DisplayColor::Set([50, 180, 80]), false).unwrap();
        assert_eq!(look(doc, 7, &plain).0, [50, 180, 80]);
        let vg = doc.create_visgroup("walls", None, &[ObjRef::Solid(2), ObjRef::Solid(3)]).unwrap();
        doc.set_visgroup_color(vg, [200, 200, 0]).unwrap();
        let vis = RenderDisplay { visgroup_colors: true, ..RenderDisplay::default() };
        assert_eq!(look(doc, 3, &vis), ([200, 200, 0], [200, 200, 0]));
        assert_eq!(look(doc, 2, &vis), ([40, 165, 225], [200, 200, 0]), "a chosen color beats the visgroup");
        doc.set_display_color(&[ObjRef::Solid(2)], DisplayColor::Reset, false).unwrap();
        assert_eq!(look(doc, 2, &plain), (WORLD_COLOR, WORLD_COLOR));
    }

    #[test]
    fn smoothing_colors_are_distinct_and_opaque() {
        assert_eq!(smoothing_color(0), [150, 150, 150, 255]);
        let colors: AHashSet<[u8; 4]> = (1..=32u32).map(|b| smoothing_color(1 << (b - 1))).collect();
        assert!(colors.len() >= 30, "{} distinct colors", colors.len());
        assert!(colors.iter().all(|c| c[3] == 255));
        assert_eq!(smoothing_color(u32::MAX)[3], 255);
    }
}
