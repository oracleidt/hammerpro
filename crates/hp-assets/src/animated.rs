use std::sync::Arc;
use std::time::Instant;

use ahash::{AHashMap, AHashSet};
use glam::{Mat4, Vec3, Vec4};
use hp_app::runtime::render::RenderDisplay;
use hp_render::{ModelId, ModelInstance, ModelPart, Renderer, Vertex, ViewKind, ViewParams};
use hp_vmf::Entity;
use rayon::prelude::*;

use crate::AssetManager;
use crate::material::MatKind;
use crate::models::{ModelEntry, PreparedModel, model_data_key};
use crate::particle_model_draw::{ANIMATION_KEY, AnimationSource};

type SkinnedGroup = (ModelId, (usize, f32), Option<(Vec<Vertex>, Vec<[f32; 4]>)>);

const VERTEX_BUDGET: usize = 600_000;
const MIN_STEP: f64 = 1.0 / 60.0;
const SHADOW_INTERVAL: f64 = 0.1;
const VIEW_TTL: u64 = 3;
const TOUCH_INTERVAL: u64 = 60;
const BOUNDS_PAD: f32 = 0.1;
const STILL_CLASSES: [&str; 6] =
    ["prop_static", "prop_detail", "prop_ragdoll", "static_prop", "prop_detail_sprite", "info_overlay"];
const IDLE_PREFIXES: [&str; 3] = ["npc_", "monster_", "generic_"];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnimSpec {
    pub preferred: Option<String>,
    pub idle_first: bool,
    pub rate: f32,
    pub cycle: f32,
}

pub(crate) fn anim_spec(e: &Entity) -> Option<AnimSpec> {
    let class = e.classname.trim().to_ascii_lowercase();
    if STILL_CLASSES.contains(&class.as_str()) {
        return None;
    }
    let number = |key: &str| e.get(key).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite());
    let preferred = e.get("DefaultAnim").map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    Some(AnimSpec {
        preferred,
        idle_first: IDLE_PREFIXES.iter().any(|p| class.starts_with(p)),
        rate: number("playbackrate").unwrap_or(1.0).clamp(-100.0, 100.0),
        cycle: number("cycle").map_or(0.0, |c| c.rem_euclid(1.0)),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GroupKey {
    path: String,
    skin: u32,
    body: u32,
    sequence: usize,
    rate: u32,
    cycle: u32,
}

struct Group {
    id: ModelId,
    data: Arc<PreparedModel>,
    source: Arc<AnimationSource>,
    anim: hp_mdl::AnimRef,
    meshes: Vec<usize>,
    vertices: usize,
    rate: f32,
    cycle: f32,
    born: f64,
    posed: Option<(usize, u32)>,
    boxes: Vec<(Vec3, Vec3)>,
    stale: u32,
    materials: Vec<u32>,
    local: (Vec3, Vec3),
}

impl Group {
    fn target(&self, now: f64) -> (usize, f32) {
        let clip = self.anim.clip;
        let elapsed = ((now - self.born).max(0.0) as f32) * self.rate;
        clip.frame_at(elapsed + self.cycle * clip.duration())
    }

    fn finished(&self, now: f64) -> bool {
        let clip = self.anim.clip;
        if self.rate == 0.0 {
            return true;
        }
        if clip.looping {
            return false;
        }
        let elapsed = ((now - self.born).max(0.0) as f32) * self.rate + self.cycle * clip.duration();
        elapsed >= clip.duration() || elapsed <= 0.0 && self.rate < 0.0
    }
}

pub(crate) enum Animated {
    Model(ModelId),
    Waiting,
    Still,
}

#[derive(Default)]
pub(crate) struct AnimatedModels {
    groups: AHashMap<GroupKey, Group>,
    ids: AHashMap<ModelId, GroupKey>,
    views: AHashMap<u32, (Mat4, Vec3, u64)>,
    clock: Option<Instant>,
    last_shadow: f64,
    last_step: f64,
    fresh: bool,
    tick: u64,
    pub animating: bool,
    pub updated: u64,
}

impl AnimatedModels {
    fn now(&mut self) -> f64 {
        self.clock.get_or_insert_with(Instant::now).elapsed().as_secs_f64()
    }

    pub(crate) fn note_view(&mut self, view: u32, params: &ViewParams, frame: u64) {
        match params.kind {
            ViewKind::Perspective { eye, .. } if params.size[0] > 0 && params.size[1] > 0 => {
                let m = Mat4::from_cols_array_2d(&params.view_proj());
                self.views.insert(view, (m, Vec3::from(eye), frame));
            }
            _ => {
                self.views.remove(&view);
            }
        }
    }

    fn source_for(&self, path: &str) -> Option<Arc<PreparedModel>> {
        self.groups.values().find(|g| g.data_path_is(path)).map(|g| Arc::clone(&g.data))
    }
}

impl Group {
    fn data_path_is(&self, path: &str) -> bool {
        self.data.info.path.eq_ignore_ascii_case(path)
    }
}

pub(crate) fn frustum(m: &Mat4) -> [Vec4; 6] {
    let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
    let norm = |p: Vec4| {
        let l = p.truncate().length();
        if l > 1e-20 { p / l } else { Vec4::new(0.0, 0.0, 0.0, 1.0) }
    };
    [norm(r3 + r0), norm(r3 - r0), norm(r3 + r1), norm(r3 - r1), norm(r2), norm(r3 - r2)]
}

pub(crate) fn box_in_frustum(planes: &[Vec4; 6], lo: Vec3, hi: Vec3) -> bool {
    planes.iter().all(|p| {
        let n = p.truncate();
        let v = Vec3::new(
            if n.x >= 0.0 { hi.x } else { lo.x },
            if n.y >= 0.0 { hi.y } else { lo.y },
            if n.z >= 0.0 { hi.z } else { lo.z },
        );
        n.dot(v) + p.w >= 0.0
    })
}

fn box_distance(eye: Vec3, lo: Vec3, hi: Vec3) -> f32 {
    (eye.clamp(lo, hi) - eye).length()
}

fn update_interval(distance: f32) -> u32 {
    match distance {
        d if d < 2048.0 => 1,
        d if d < 6144.0 => 2,
        d if d < 16384.0 => 4,
        _ => 8,
    }
}

fn world_box(local: (Vec3, Vec3), m: &Mat4) -> (Vec3, Vec3) {
    let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for c in 0..8 {
        let p = Vec3::new(
            if c & 1 != 0 { local.1.x } else { local.0.x },
            if c & 2 != 0 { local.1.y } else { local.0.y },
            if c & 4 != 0 { local.1.z } else { local.0.z },
        );
        let w = m.transform_point3(p);
        lo = lo.min(w);
        hi = hi.max(w);
    }
    (lo, hi)
}

fn skin_group(group: &Group, frame: usize, frac: f32, time: f32) -> Option<(Vec<Vertex>, Vec<[f32; 4]>)> {
    let set = group.source.set.as_ref()?;
    let matrices = set.skin_layered(&group.anim, frame, frac, time).or_else(|| set.skin(&group.anim, frame, frac))?;
    let mut vertices = Vec::with_capacity(group.vertices);
    let mut tangents = Vec::with_capacity(group.vertices);
    let mut all_tangents = true;
    for &index in &group.meshes {
        let mesh = group.source.meshes.get(index)?;
        let has_tangents = mesh.tangents.len() == mesh.vertices.len();
        all_tangents &= has_tangents;
        for (i, v) in mesh.vertices.iter().enumerate() {
            let tangent = if has_tangents { mesh.tangents.get(i).copied() } else { None };
            let (pos, normal, tangent) = match mesh.bone_weights.get(i) {
                Some(w) => hp_mdl::pose_vertex_with_tangent(&matrices, w, v.pos, v.normal, tangent),
                None => (v.pos, v.normal, tangent),
            };
            vertices.push(Vertex { pos, normal, uv: v.uv, alpha: 1.0 });
            if let Some(t) = tangent {
                tangents.push(t);
            }
        }
    }
    if !all_tangents {
        tangents.clear();
    }
    Some((vertices, tangents))
}

fn local_bounds(parts: &[ModelPart], sequence: Option<&hp_mdl::Sequence>) -> (Vec3, Vec3) {
    let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for v in parts.iter().flat_map(|p| &p.vertices) {
        let p = Vec3::from(v.pos);
        if p.is_finite() {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if let Some(s) = sequence {
        let (a, b) = (Vec3::from(s.bbox_min), Vec3::from(s.bbox_max));
        if a.is_finite() && b.is_finite() && a.cmplt(b).all() {
            lo = lo.min(a);
            hi = hi.max(b);
        }
    }
    if !lo.cmple(hi).all() {
        return (Vec3::splat(-16.0), Vec3::splat(16.0));
    }
    let pad = (hi - lo).max_element().max(8.0) * BOUNDS_PAD;
    (lo - Vec3::splat(pad), hi + Vec3::splat(pad))
}

impl AssetManager {
    pub(crate) fn animated_model(
        &mut self,
        renderer: &mut Renderer,
        path: &str,
        skin: u32,
        body: u32,
        spec: &AnimSpec,
    ) -> Animated {
        let still = |static_prop: bool, frames: &[u32]| static_prop || frames.iter().all(|f| *f <= 1);
        match self.models.map.get(path) {
            Some(ModelEntry::Ready(d)) if still(d.info.is_static_prop, &d.info.sequence_frames) => {
                return Animated::Still;
            }
            Some(ModelEntry::Cached(i)) if still(i.is_static_prop, &i.sequence_frames) => return Animated::Still,
            Some(ModelEntry::Failed(_)) => return Animated::Still,
            Some(ModelEntry::Ready(_) | ModelEntry::Cached(_)) => {}
            None | Some(ModelEntry::Loading) => return Animated::Waiting,
        }
        let key = model_data_key(path, Some(ANIMATION_KEY));
        let data = match self.models.map.get(&key) {
            Some(ModelEntry::Ready(d)) => {
                let d = Arc::clone(d);
                self.models.last_used.insert(key.clone(), self.shared.current_frame());
                d
            }
            Some(ModelEntry::Failed(_)) => return Animated::Still,
            _ => match self.anim_models.source_for(path) {
                Some(d) => d,
                None => match self.model_data(&key) {
                    Some(d) => d,
                    None if matches!(self.models.map.get(&key), Some(ModelEntry::Failed(_))) => return Animated::Still,
                    None => return Animated::Waiting,
                },
            },
        };
        let Some(source) = data.particle_animation.clone() else { return Animated::Still };
        let Some(set) = source.set.clone() else { return Animated::Still };
        let Some(sequence) = set.default_sequence(spec.preferred.as_deref(), spec.idle_first) else {
            return Animated::Still;
        };
        let Some(anim) = set.sequence_at(sequence, &[]) else { return Animated::Still };
        if anim.clip.frames <= 1 {
            return Animated::Still;
        }
        let skin = if (skin as usize) < data.skin_count() { skin } else { 0 };
        let body = data.body_key(body);
        let gkey = GroupKey {
            path: path.to_owned(),
            skin,
            body,
            sequence,
            rate: spec.rate.to_bits(),
            cycle: spec.cycle.to_bits(),
        };
        if let Some(g) = self.anim_models.groups.get(&gkey)
            && Arc::ptr_eq(&g.source, &source)
            && renderer.has_model(g.id)
        {
            let (id, materials) = (g.id, g.materials.clone());
            for m in materials {
                self.touch_material(m);
            }
            return Animated::Model(id);
        }
        if let Some(old) = self.anim_models.groups.remove(&gkey) {
            self.anim_models.ids.remove(&old.id);
            renderer.destroy_model(old.id);
            self.residency.models.remove(&old.id);
        }
        let choices = data.body_choices(body);
        let mut parts = Vec::new();
        let mut meshes = Vec::new();
        for (index, mesh) in source.meshes.iter().enumerate() {
            if choices.get(mesh.body_part).copied().unwrap_or(0) as usize != mesh.model_index || mesh.indices.is_empty()
            {
                continue;
            }
            let material = self.asset_material(data.part_material(skin as usize, mesh.skin_ref), MatKind::Model);
            let tangents = if mesh.tangents.len() == mesh.vertices.len() { mesh.tangents.clone() } else { Vec::new() };
            parts.push(ModelPart {
                material,
                vertices: mesh
                    .vertices
                    .iter()
                    .map(|v| Vertex { pos: v.pos, normal: v.normal, uv: v.uv, alpha: 1.0 })
                    .collect(),
                indices: mesh.indices.clone(),
                tangents,
            });
            meshes.push(index);
        }
        if parts.is_empty() {
            return Animated::Still;
        }
        let vertices = parts.iter().map(|p| p.vertices.len()).sum();
        let local = local_bounds(&parts, set.sequence_info(sequence));
        let materials = parts.iter().map(|p| p.material).collect();
        let id = renderer.create_model(&parts);
        if renderer.model_vertex_count(id) != vertices {
            renderer.destroy_model(id);
            return Animated::Still;
        }
        renderer.set_model_bounds(id, local.0.to_array(), local.1.to_array());
        if let Some(meta) = crate::residency::model_meta(&parts) {
            self.residency.models.insert(id, meta);
        }
        let born = self.anim_models.now();
        self.anim_models.fresh = true;
        self.anim_models.ids.insert(id, gkey.clone());
        self.anim_models.groups.insert(
            gkey,
            Group {
                id,
                data,
                source,
                anim,
                meshes,
                vertices,
                rate: spec.rate,
                cycle: spec.cycle,
                born,
                posed: None,
                boxes: Vec::new(),
                stale: u32::MAX / 2,
                materials,
                local,
            },
        );
        Animated::Model(id)
    }

    pub(crate) fn place_animated_models(&mut self, renderer: &mut Renderer, list: &[ModelInstance]) {
        let a = &mut self.anim_models;
        if a.groups.is_empty() {
            return;
        }
        let mut live: AHashSet<ModelId> = self.scene.ents.values().filter_map(|s| s.model).collect();
        for parked in &self.parked {
            live.extend(parked.models());
        }
        let dead: Vec<GroupKey> =
            a.groups.iter().filter(|(_, g)| !live.contains(&g.id)).map(|(k, _)| k.clone()).collect();
        for key in dead {
            if let Some(g) = a.groups.remove(&key) {
                a.ids.remove(&g.id);
                renderer.destroy_model(g.id);
                self.residency.models.remove(&g.id);
            }
        }
        for g in a.groups.values_mut() {
            g.boxes.clear();
        }
        for inst in list {
            let Some(key) = a.ids.get(&inst.model) else { continue };
            let Some(g) = a.groups.get_mut(key) else { continue };
            let m = Mat4::from_cols_array_2d(&inst.transform);
            if m.is_finite() {
                g.boxes.push(world_box(g.local, &m));
            }
        }
        a.fresh = true;
    }

    pub(crate) fn tick_animated_models(&mut self, renderer: &mut Renderer, display: &RenderDisplay) {
        let shown = self.scene.opts.is_some_and(|o| o.show_models);
        let frame = self.shared.current_frame();
        let a = &mut self.anim_models;
        if !display.animate_models || !shown || a.groups.is_empty() {
            a.animating = false;
            return;
        }
        let now = a.now();
        if now - a.last_step < MIN_STEP && !a.fresh {
            return;
        }
        a.last_step = now;
        a.fresh = false;
        a.tick += 1;
        a.views.retain(|_, v| frame.saturating_sub(v.2) <= VIEW_TTL);
        let views: Vec<([Vec4; 6], Vec3)> = a.views.values().map(|v| (frustum(&v.0), v.1)).collect();
        let mut animating = false;
        let mut due: Vec<(f32, GroupKey, (usize, f32))> = Vec::new();
        for (key, g) in a.groups.iter_mut() {
            let (distance, visible) = if views.is_empty() {
                (0.0, !g.boxes.is_empty())
            } else {
                let mut best = f32::INFINITY;
                for &(lo, hi) in &g.boxes {
                    for (planes, eye) in &views {
                        if box_in_frustum(planes, lo, hi) {
                            best = best.min(box_distance(*eye, lo, hi));
                        }
                    }
                }
                (best, best.is_finite())
            };
            if !visible {
                g.stale = g.stale.saturating_add(1);
                continue;
            }
            let target = g.target(now);
            let moving = g.posed != Some((target.0, target.1.to_bits()));
            animating |= moving || !g.finished(now);
            if !moving {
                continue;
            }
            g.stale = g.stale.saturating_add(1);
            if g.stale >= update_interval(distance) {
                due.push((distance, key.clone(), target));
            }
        }
        a.animating = animating;
        if due.is_empty() {
            return;
        }
        due.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut budget = 0usize;
        let mut chosen: Vec<(&Group, (usize, f32))> = Vec::new();
        for (_, key, target) in &due {
            let Some(g) = a.groups.get(key) else { continue };
            if !chosen.is_empty() && budget + g.vertices > VERTEX_BUDGET {
                break;
            }
            budget += g.vertices;
            chosen.push((g, *target));
        }
        let skinned: Vec<SkinnedGroup> = chosen
            .par_iter()
            .map(|(g, target)| (g.id, *target, skin_group(g, target.0, target.1, (now - g.born).max(0.0) as f32)))
            .collect();
        let mut updated = 0;
        for (id, target, result) in skinned {
            let Some(key) = a.ids.get(&id).cloned() else { continue };
            let Some(g) = a.groups.get_mut(&key) else { continue };
            g.posed = Some((target.0, target.1.to_bits()));
            g.stale = 0;
            if let Some((vertices, tangents)) = result
                && renderer.update_model_vertices(id, &vertices, &tangents)
            {
                updated += 1;
            }
        }
        if updated > 0 {
            a.updated += updated;
            let shadows = now - a.last_shadow >= SHADOW_INTERVAL;
            if shadows {
                a.last_shadow = now;
            }
            renderer.note_animated_models(shadows);
        }
        if a.tick.is_multiple_of(TOUCH_INTERVAL) {
            let paths: AHashSet<String> =
                a.groups.keys().map(|k| model_data_key(&k.path, Some(ANIMATION_KEY))).collect();
            let materials: Vec<u32> = a.groups.values().flat_map(|g| g.materials.iter().copied()).collect();
            for p in paths {
                self.models.last_used.insert(p, frame);
            }
            for m in materials {
                self.touch_material(m);
            }
        }
    }

    pub(crate) fn models_animating(&self) -> bool {
        self.anim_models.animating
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(class: &str, kv: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: class.to_owned(), ..Entity::default() };
        for (k, v) in kv {
            e.props.push(((*k).to_owned(), (*v).to_owned()));
        }
        e
    }

    #[test]
    fn static_props_stay_still_and_dynamic_ones_read_their_keys() {
        assert!(anim_spec(&entity("prop_static", &[("DefaultAnim", "idle")])).is_none());
        assert!(anim_spec(&entity("prop_ragdoll", &[])).is_none());
        let s =
            anim_spec(&entity("prop_dynamic", &[("DefaultAnim", " spin "), ("playbackrate", "2"), ("cycle", "1.25")]))
                .expect("animates");
        assert_eq!(s.preferred.as_deref(), Some("spin"));
        assert!(!s.idle_first);
        assert_eq!(s.rate, 2.0);
        assert!((s.cycle - 0.25).abs() < 1e-6);
        let npc = anim_spec(&entity("npc_combine_s", &[])).expect("animates");
        assert!(npc.idle_first && npc.preferred.is_none() && npc.rate == 1.0 && npc.cycle == 0.0);
        let bad =
            anim_spec(&entity("prop_physics", &[("playbackrate", "nan"), ("DefaultAnim", "  ")])).expect("animates");
        assert!(bad.preferred.is_none() && bad.rate == 1.0);
    }

    #[test]
    fn frustum_keeps_what_the_camera_sees() {
        let params = ViewParams {
            size: [300, 200],
            kind: ViewKind::Perspective {
                eye: [0.0; 3],
                forward: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 60.0,
                near: 4.0,
                far: 10_000.0,
            },
            mode: hp_render::RenderMode::Textured,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: true,
            show_models: true,
            msaa: false,
            game_pixel_grid: false,
        };
        let planes = frustum(&Mat4::from_cols_array_2d(&params.view_proj()));
        assert!(box_in_frustum(&planes, Vec3::new(500.0, -10.0, -10.0), Vec3::new(520.0, 10.0, 10.0)));
        assert!(!box_in_frustum(&planes, Vec3::new(-520.0, -10.0, -10.0), Vec3::new(-500.0, 10.0, 10.0)));
        assert!(!box_in_frustum(&planes, Vec3::new(500.0, 2000.0, -10.0), Vec3::new(520.0, 2100.0, 10.0)));
        assert_eq!(update_interval(100.0), 1);
        assert!(update_interval(20_000.0) > update_interval(3000.0));
    }

    #[test]
    fn looping_and_one_shot_timing() {
        let clip = hp_mdl::AnimClip { sequence: 0, frames: 11, fps: 10.0, looping: false };
        assert_eq!(clip.frame_at(0.55).0, 5);
        assert_eq!(clip.frame_at(5.0), (10, 0.0));
    }
}
