use std::collections::HashMap;
use std::sync::Arc;

use hp_editor::limits::world::{BspWorld, DetailOrientation, WorldVertex};
use hp_editor::limits::world_light::{self, EmitKind, LightMode};
use hp_render::{
    CompiledBatch, CompiledBlock, CompiledLightMode, CompiledLightmap, CompiledVertex, CubemapId, CubemapImage,
    CubemapProbe, MeshBatch, ModelId, ModelInstance, ModelLighting, ModelLocalLight, NO_CUBEMAP, ObjectFlags,
    ObjectMesh, Renderer, Vertex,
};

use crate::AssetManager;

mod sky3d;

const WORLD_OBJECT_BASE: u32 = u32::MAX - (1 << 21);
const MAX_GROUPS: usize = (1 << 20) - 64;
const WIRE_COLOR: [u8; 3] = [235, 235, 235];

type CachedLighting = (ModelId, [[f32; 4]; 4], Option<[f32; 3]>, ModelLighting);
type WantedWorld = (Arc<BspWorld>, Option<Arc<Vec<bool>>>);

#[derive(Default)]
pub(crate) struct PlayWorld {
    wanted: Option<WantedWorld>,
    uploaded: Option<(Arc<BspWorld>, Vec<u8>)>,
    applied: Option<Option<Arc<Vec<bool>>>>,
    detail_models: Vec<DetailModel>,
    detail_fade: [f32; 2],
    detail_models_pending: bool,
    static_props: HashMap<(String, [i32; 3]), usize>,
    model_lights: HashMap<u32, CachedLighting>,
    model_lights_sent: Vec<ModelLighting>,
    model_lights_source: usize,
    baked: Option<(Arc<world_light::PropLighting>, Arc<StaticPropIndex>)>,
    baked_region: Option<[[f64; 3]; 2]>,
    pub(crate) entity_blocks: HashMap<hp_vmf::Id, u32>,
    style_values: Vec<f32>,
    styles_sent: Option<(usize, Vec<f32>)>,
    ropes: Option<PlayRopes>,
    cubes: HashMap<[i32; 3], CubemapId>,
    owned_cubes: Vec<CubemapId>,
    pub(crate) cube_sources: Option<(usize, usize)>,
    drawn_cubes: Vec<([i32; 3], Option<CubemapId>)>,
    sky_bounds: Option<(glam::DVec3, glam::DVec3)>,
    sky_groups: Vec<bool>,
}

struct PlayRopes {
    model: Option<ModelId>,
    colors: Vec<[u8; 4]>,
}

struct DetailModel {
    path: String,
    transform: glam::DMat4,
    group: usize,
    tint: [f32; 4],
}

type StaticPropIndex = HashMap<(String, [i32; 3]), usize>;

fn in_region(region: Option<[[f64; 3]; 2]>, transform: &[[f32; 4]; 4]) -> bool {
    region.is_none_or(|[lo, hi]| (0..3).all(|k| (lo[k]..=hi[k]).contains(&f64::from(transform[3][k]))))
}

fn same_vis(a: &Option<Arc<Vec<bool>>>, b: &Option<Arc<Vec<bool>>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
        _ => false,
    }
}

pub(crate) fn group_drawn(i: usize, drawn: Option<&Vec<bool>>) -> bool {
    drawn.is_none_or(|d| d.get(i).copied().unwrap_or(true))
}

impl AssetManager {
    pub(crate) fn set_play_world_impl(&mut self, world: Option<Arc<BspWorld>>, visible: Option<Arc<Vec<bool>>>) {
        self.play_world.wanted = world.map(|w| (w, visible));
    }

    pub(crate) fn set_baked_prop_lighting_impl(
        &mut self,
        props: Option<Arc<world_light::PropLighting>>,
        region: Option<[[f64; 3]; 2]>,
    ) {
        let same = match (&self.play_world.baked, &props) {
            (Some((a, _)), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same && self.play_world.baked_region == region {
            return;
        }
        self.play_world.baked_region = region;
        self.play_world.baked = props.map(|p| {
            let index = Arc::new(world_light::static_prop_index(&p.static_props));
            (p, index)
        });
        self.scene.instances_dirty = true;
    }

    pub(crate) fn play_world_shown(&self) -> bool {
        self.play_world.wanted.is_some()
    }

    pub(crate) fn sync_play_world(&mut self, doc: &hp_editor::Document, renderer: &mut Renderer, rebuilt: bool) {
        if rebuilt {
            self.play_world.uploaded = None;
            self.play_world.applied = None;
        }
        let same_world = match (&self.play_world.wanted, &self.play_world.uploaded) {
            (Some((w, _)), Some((u, _))) => Arc::ptr_eq(w, u),
            (None, None) => true,
            _ => false,
        };
        if !same_world {
            if let Some((old, objects)) = self.play_world.uploaded.take() {
                for k in 0..old.static_props.len().min(MAX_STATIC_PROPS) {
                    renderer.remove_object(static_prop_object(k));
                }
                for (i, o) in objects.iter().enumerate() {
                    if o & 1 != 0 {
                        renderer.remove_object(face_object(i));
                    }
                    if o & 2 != 0 {
                        renderer.remove_object(disp_object(i));
                    }
                    if o & 4 != 0 {
                        renderer.remove_object(detail_object(i));
                    }
                }
                for k in 0..self.play_world.detail_models.len() {
                    renderer.remove_object(detail_model_object(k));
                }
                renderer.set_detail_sprites(0, &[]);
                renderer.set_detail_fade(None);
                if let Some(m) = self.play_world.ropes.take().and_then(|r| r.model) {
                    renderer.destroy_model(m);
                }
                renderer.set_compiled_objects(Default::default());
                renderer.clear_compiled_lighting();
                for id in self.play_world.owned_cubes.drain(..) {
                    renderer.destroy_cubemap(id);
                }
                self.play_world.cubes.clear();
                self.play_world.drawn_cubes.clear();
                self.play_world.cube_sources = None;
                renderer.set_model_cubemaps(None);
            }
            self.play_world.static_props.clear();
            self.play_world.entity_blocks.clear();
            self.play_world.model_lights.clear();
            if !self.play_world.model_lights_sent.is_empty() {
                self.play_world.model_lights_sent.clear();
                renderer.set_model_lighting(&[]);
            }
            if self.play_world.baked.is_some() {
                self.scene.instances_dirty = true;
            }
            if !self.play_world.detail_models.is_empty() {
                self.play_world.detail_models.clear();
                self.scene.instances_dirty = true;
            }
            self.play_world.applied = None;
            if let Some((world, _)) = self.play_world.wanted.clone() {
                let objects = self.upload_world(doc, renderer, &world);
                self.play_world.uploaded = Some((world, objects));
                self.play_world.sky_bounds = None;
                self.play_world.styles_sent = None;
            }
        }
        let Some((world, visible)) = self.play_world.wanted.clone() else { return };
        let Some((_, objects)) = &self.play_world.uploaded else { return };
        if self.play_world.applied.as_ref().is_some_and(|a| same_vis(a, &visible)) {
            return;
        }
        let before = self.play_world.applied.clone();
        let mut changed: Vec<Option<bool>> = vec![None; objects.len()];
        for (i, o) in objects.iter().enumerate().take(world.groups.len()) {
            let hidden = !self.group_visible(i, visible.as_deref());
            if let Some(b) = &before
                && self.group_visible(i, b.as_deref()) != hidden
            {
                continue;
            }
            changed[i] = Some(hidden);
            if o & 1 != 0 {
                renderer.set_object_flags(face_object(i), object_flags(false, hidden));
            }
            if o & 2 != 0 {
                renderer.set_object_flags(disp_object(i), object_flags(true, hidden));
            }
            if o & 4 != 0 {
                renderer.set_object_flags(detail_object(i), object_flags(false, hidden));
            }
        }
        for (k, m) in self.play_world.detail_models.iter().enumerate() {
            if let Some(&Some(hidden)) = changed.get(m.group) {
                renderer.set_object_flags(detail_model_object(k), object_flags(false, hidden));
            }
        }
        for (k, p) in world.static_props.iter().take(MAX_STATIC_PROPS).enumerate() {
            if let Some(&Some(hidden)) = changed.get(p.group as usize) {
                renderer.set_object_flags(static_prop_object(k), self.static_prop_flags(p, hidden));
            }
        }
        self.play_world.applied = Some(visible);
    }

    fn upload_world(&mut self, doc: &hp_editor::Document, renderer: &mut Renderer, world: &BspWorld) -> Vec<u8> {
        let start = std::time::Instant::now();
        let materials: Vec<hp_render::MaterialId> =
            world.materials.iter().map(|name| self.world_material(name)).collect();
        self.world_cubemaps(renderer, world);
        let cubes = &self.play_world.cubes;
        let material_cubes: Vec<u16> = world
            .material_cubemaps
            .iter()
            .map(|o| o.and_then(|o| cubes.get(&o)).map_or(NO_CUBEMAP, |c| c.raw()))
            .collect();
        let cube_of = |m: u32| material_cubes.get(m as usize).copied().unwrap_or(NO_CUBEMAP);
        let count = world.groups.len().min(MAX_GROUPS);
        let mut objects = vec![0u8; count];
        let mut blocks: Vec<CompiledBlock> = Vec::new();
        for (i, group) in world.groups.iter().take(count).enumerate() {
            for disp in [false, true] {
                let mut lit: Vec<CompiledBatch> = Vec::new();
                let mut batches: Vec<MeshBatch> = Vec::new();
                let mut edges: Vec<[f32; 3]> = Vec::new();
                let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
                for &f in &group.faces {
                    let Some(face) = world.faces.get(f as usize) else { continue };
                    if face.is_disp() != disp {
                        continue;
                    }
                    let Some(&material) = materials.get(face.material as usize) else { continue };
                    let verts = world.face_vertices(face);
                    if verts.is_empty() {
                        continue;
                    }
                    let tris = BspWorld::face_triangles(face);
                    let batch = match batches.iter().position(|b| b.material == material) {
                        Some(b) => &mut batches[b],
                        None => {
                            batches.push(MeshBatch { material, ..Default::default() });
                            batches.last_mut().expect("just pushed")
                        }
                    };
                    let base = batch.vertices.len() as u32;
                    let normals =
                        if disp { disp_normals(verts, &tris, face.normal) } else { vec![face.normal; verts.len()] };
                    batch.vertices.extend(verts.iter().zip(normals).map(|(v, normal)| Vertex {
                        pos: v.pos,
                        normal,
                        uv: v.uv,
                        alpha: v.alpha,
                    }));
                    let cube = cube_of(face.material);
                    batch.cubemaps.resize(base as usize, NO_CUBEMAP);
                    batch.cubemaps.resize(batch.vertices.len(), cube);
                    if verts.iter().any(|v| v.multiblend.is_some()) {
                        batch.blend.resize(base as usize, hp_render::BlendVertex::default());
                        batch.blend.extend(verts.iter().map(|v| {
                            v.multiblend.map_or_else(hp_render::BlendVertex::default, hp_render::BlendVertex::painted)
                        }));
                    }
                    batch.indices.extend(tris.iter().map(|t| base + t));
                    let lit_batch = match lit.iter().position(|b| b.material == material) {
                        Some(b) => &mut lit[b],
                        None => {
                            lit.push(CompiledBatch { material, ..Default::default() });
                            lit.last_mut().expect("just pushed")
                        }
                    };
                    let lit_base = lit_batch.vertices.len() as u32;
                    let first = batch.vertices.len() - verts.len();
                    lit_batch.vertices.extend(
                        verts
                            .iter()
                            .zip(&batch.vertices[first..])
                            .map(|(v, r)| compiled_vertex(v, r.normal, face.light, cube)),
                    );
                    lit_batch.indices.extend(tris.iter().map(|t| lit_base + t));
                    for v in verts {
                        for k in 0..3 {
                            min[k] = min[k].min(v.pos[k]);
                            max[k] = max[k].max(v.pos[k]);
                        }
                    }
                    if !disp {
                        for k in 0..verts.len() {
                            edges.push(verts[k].pos);
                            edges.push(verts[(k + 1) % verts.len()].pos);
                        }
                    }
                }
                let id = if disp { disp_object(i) } else { face_object(i) };
                if batches.is_empty() {
                    continue;
                }
                objects[i] |= if disp { 2 } else { 1 };
                let mesh = ObjectMesh {
                    batches,
                    edges,
                    aabb_min: min,
                    aabb_max: max,
                    wire_color: WIRE_COLOR,
                    ..Default::default()
                };
                renderer.upsert_object(id, mesh);
                renderer.set_object_flags(id, object_flags(disp, false));
                blocks.push(CompiledBlock { object: id, batches: lit });
            }
        }
        let scene = doc.scene();
        let mut first_brush: HashMap<hp_vmf::Id, hp_vmf::Id> = HashMap::new();
        for (id, n) in scene.solids() {
            if let hp_editor::Owner::Entity(e) = n.owner {
                let b = first_brush.entry(e).or_insert(*id);
                *b = (*b).min(*id);
            }
        }
        self.play_world.entity_blocks.clear();
        for em in &world.entity_models {
            let Some(&object) = first_brush.get(&em.id) else { continue };
            let mut lit: Vec<CompiledBatch> = Vec::new();
            for face in world.entity_faces.get(em.faces.start as usize..em.faces.end as usize).unwrap_or_default() {
                let Some(&material) = materials.get(face.material as usize) else { continue };
                let verts = world.face_vertices(face);
                if verts.is_empty() {
                    continue;
                }
                let batch = match lit.iter().position(|b| b.material == material) {
                    Some(b) => &mut lit[b],
                    None => {
                        lit.push(CompiledBatch { material, ..Default::default() });
                        lit.last_mut().expect("just pushed")
                    }
                };
                let base = batch.vertices.len() as u32;
                let cube = cube_of(face.material);
                batch.vertices.extend(verts.iter().map(|v| compiled_vertex(v, face.normal, face.light, cube)));
                batch.indices.extend(BspWorld::face_triangles(face).iter().map(|t| base + t));
            }
            if !lit.is_empty() {
                blocks.push(CompiledBlock { object, batches: lit });
                self.play_world.entity_blocks.insert(em.id, object);
            }
        }
        let compiled: ahash::AHashSet<u32> = scene
            .solids()
            .iter()
            .filter(|(_, n)| {
                matches!(n.owner, hp_editor::Owner::Entity(e) if self.play_world.entity_blocks.contains_key(&e))
            })
            .map(|(id, _)| *id)
            .collect();
        renderer.set_compiled_objects(compiled);
        let mode = match world.light.mode {
            LightMode::Fullbright => CompiledLightMode::Fullbright,
            LightMode::Ldr => CompiledLightMode::Ldr,
            LightMode::Hdr => CompiledLightMode::Hdr,
        };
        let maps: Vec<CompiledLightmap<'_>> = world
            .light
            .images
            .iter()
            .map(|m| CompiledLightmap {
                width: m.width,
                height: m.height,
                texels: &m.texels,
                styles: &m.styles,
                bumped: m.bumped,
            })
            .collect();
        let stats = renderer.set_compiled_lighting(mode, &maps, &blocks);
        log::info!(
            "player view: compiled lighting {} ({} lightmaps, {} dropped, atlas {} x {} x {})",
            world.light.mode.name(),
            stats.lightmaps,
            stats.dropped,
            stats.atlas_size,
            stats.atlas_size,
            stats.atlas_layers
        );
        self.play_world.static_props = world_light::static_prop_index(&world.light.props.static_props);
        for (id, object) in &self.play_world.entity_blocks {
            if let Some(m) = self.play_transforms.get(id) {
                renderer.set_compiled_transform(*object, Some(m.as_mat4().to_cols_array_2d()));
            }
        }
        let mut sprites = Vec::with_capacity(world.details.len());
        for d in &world.details {
            let g = d.group as usize;
            let Some(o) = objects.get_mut(g) else { continue };
            *o |= 4;
            sprites.push(hp_render::DetailSprite {
                origin: d.origin,
                right: d.right,
                up: d.up,
                ul: d.ul,
                lr: d.lr,
                tex: d.tex,
                color: d.color,
                orientation: match d.orientation {
                    DetailOrientation::Normal => hp_render::DetailSpriteOrientation::Fixed,
                    DetailOrientation::ScreenAligned => hp_render::DetailSpriteOrientation::ScreenAligned,
                    DetailOrientation::Vertical => hp_render::DetailSpriteOrientation::Vertical,
                },
                sway: 0,
                object: detail_object(g),
            });
        }
        let material =
            if world.detail_material.trim().is_empty() { DETAIL_MATERIAL } else { world.detail_material.as_str() };
        let material = self.world_material(material);
        renderer.set_detail_sprites(material, &sprites);
        let fade = world.detail_fade;
        let fade_set = fade[0] > 0.0;
        renderer.set_detail_fade(fade_set.then_some(fade));
        self.play_world.detail_fade = if fade_set { fade } else { hp_editor::limits::world::DETAIL_FADE_DEFAULT };
        self.play_world.detail_models.clear();
        for m in world.detail_models.iter().take(MAX_DETAIL_MODELS) {
            let g = m.group as usize;
            let Some(o) = objects.get_mut(g) else { continue };
            *o |= 4;
            let v = |a: [f32; 3]| glam::DVec3::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]));
            let transform = crate::models::entity_matrix(v(m.origin), v(m.angles), 1.0);
            let tint = detail_tint(m.light);
            renderer
                .set_object_flags(detail_model_object(self.play_world.detail_models.len()), object_flags(false, false));
            self.play_world.detail_models.push(DetailModel { path: m.model.clone(), transform, group: g, tint });
        }
        if let Some(m) = self.play_world.ropes.take().and_then(|r| r.model) {
            renderer.destroy_model(m);
        }
        renderer.set_object_flags(rope_object(), object_flags(false, false));
        for (k, p) in world.static_props.iter().take(MAX_STATIC_PROPS).enumerate() {
            renderer.set_object_flags(static_prop_object(k), self.static_prop_flags(p, false));
        }
        self.scene.instances_dirty = true;
        for (i, o) in objects.iter().enumerate() {
            if o & 4 != 0 {
                renderer.set_object_flags(detail_object(i), object_flags(false, false));
            }
        }
        log::info!(
            "player view: compiled world uploaded in {:?} ({} faces, {} groups, {} materials)",
            start.elapsed(),
            world.faces.len(),
            count,
            world.materials.len()
        );
        objects
    }
}

const DETAIL_MATERIAL: &str = "detail/detailsprites";

fn detail_object(i: usize) -> u32 {
    WORLD_OBJECT_BASE - (1 << 20) + i as u32
}

const MAX_DETAIL_MODELS: usize = 1 << 20;

fn detail_model_object(k: usize) -> u32 {
    WORLD_OBJECT_BASE - (2 << 20) + k as u32
}

pub(crate) fn detail_tint(light: [f32; 3]) -> [f32; 4] {
    let srgb = |l: f32| {
        let l = if l.is_finite() { l.clamp(0.0, 1.0) } else { 0.0 };
        let l = if l.powf(1.0 / 2.2) >= 0.95 { 1.0 } else { l };
        if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }
    };
    [srgb(light[0]), srgb(light[1]), srgb(light[2]), 1.0]
}

fn static_prop_matrix(p: &hp_editor::limits::world::StaticPropPlacement) -> glam::DMat4 {
    let v = |a: [f32; 3]| glam::DVec3::new(f64::from(a[0]), f64::from(a[1]), f64::from(a[2]));
    crate::models::entity_matrix(v(p.origin), v(p.angles), 1.0)
}

const MAX_STATIC_PROPS: usize = 1 << 20;

fn static_prop_object(k: usize) -> u32 {
    WORLD_OBJECT_BASE - (3 << 20) + k as u32
}

fn rope_object() -> u32 {
    WORLD_OBJECT_BASE - (4 << 20)
}

fn rope_light(world: &BspWorld, p: [f32; 3]) -> [f32; 3] {
    let props = &world.light.props;
    if world.light.mode == LightMode::Fullbright {
        return [1.0; 3];
    }
    let lit = props.model_lighting(p);
    let mut out = [0.0f32; 3];
    for side in &lit.ambient {
        for k in 0..3 {
            out[k] += side[k] / 6.0;
        }
    }
    for l in &lit.lights {
        let scale = if l.style == 0 { 1.0 } else { props.style_scale(usize::from(l.style)) };
        let (ratio, to_light) = match l.kind {
            EmitKind::Sky | EmitKind::Surface => (1.0, l.dir.map(|v| -v)),
            _ => {
                let d: [f32; 3] = std::array::from_fn(|k| l.origin[k] - p[k]);
                let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1.0);
                if l.radius > 0.0 && dist > l.radius {
                    continue;
                }
                let [c, li, q] = l.attenuation;
                let mut f = 1.0 / (c + li * dist + q * dist * dist).max(1e-6);
                if l.kind == EmitKind::Spot {
                    let s = -(d[0] * l.dir[0] + d[1] * l.dir[1] + d[2] * l.dir[2]) / dist;
                    let [inner, outer, exponent] = l.cone;
                    if s <= outer {
                        continue;
                    }
                    if s <= inner {
                        let mut m = ((s - outer) / (inner - outer).max(1e-6)).clamp(0.0, 1.0);
                        if exponent != 0.0 && exponent != 1.0 {
                            m = m.powf(exponent);
                        }
                        f *= m;
                    }
                }
                (f, d.map(|v| v / dist))
            }
        };
        let facing = (to_light[0].abs() + to_light[1].abs() + to_light[2].abs()) / 6.0;
        for (o, c) in out.iter_mut().zip(l.color) {
            *o += c * ratio * scale * facing;
        }
    }
    out.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 })
}

fn vhv_color(c: [f32; 3]) -> [u8; 4] {
    let byte = |v: f32| (v.max(0.0).powf(1.0 / 2.2) * 0.5 * 255.0).round().clamp(0.0, 255.0) as u8;
    [byte(c[2]), byte(c[1]), byte(c[0]), 255]
}

fn static_prop_index(object: u32) -> Option<usize> {
    let base = static_prop_object(0);
    (base..base + MAX_STATIC_PROPS as u32).contains(&object).then(|| (object - base) as usize)
}

fn detail_model_index(object: u32) -> Option<usize> {
    let base = detail_model_object(0);
    (base..base + MAX_DETAIL_MODELS as u32).contains(&object).then(|| (object - base) as usize)
}

impl AssetManager {
    fn group_visible(&self, i: usize, visible: Option<&Vec<bool>>) -> bool {
        group_drawn(i, visible) || self.play_world.sky_groups.get(i).copied().unwrap_or(false)
    }

    fn static_prop_flags(&self, p: &hp_editor::limits::world::StaticPropPlacement, hidden: bool) -> ObjectFlags {
        let in_room = self.scene.env.room.as_ref().is_some_and(|r| sky3d::in_room(p.origin, p.origin, r.bounds));
        let mut flags = object_flags(false, hidden && !in_room);
        if in_room {
            flags |= ObjectFlags::SKY_3D;
        }
        flags
    }

    pub(crate) fn sync_sky_props(&mut self, renderer: &mut Renderer) {
        let bounds = self.scene.env.room.as_ref().map(|r| r.bounds);
        if bounds == self.play_world.sky_bounds {
            return;
        }
        self.play_world.sky_bounds = bounds;
        let Some(world) = self.play_world.uploaded.as_ref().map(|(w, _)| Arc::clone(w)) else {
            self.play_world.sky_groups.clear();
            return;
        };
        self.play_world.sky_groups =
            world.groups.iter().map(|g| bounds.is_some_and(|room| sky3d::in_room(g.min, g.max, room))).collect();
        let visible = self.play_world.applied.clone().flatten();
        for (k, p) in world.static_props.iter().take(MAX_STATIC_PROPS).enumerate() {
            let hidden = !self.group_visible(p.group as usize, visible.as_deref());
            renderer.set_object_flags(static_prop_object(k), self.static_prop_flags(p, hidden));
        }
        self.play_world.applied = None;
    }

    pub(crate) fn play_detail_instances(&mut self, renderer: &mut Renderer) -> Vec<hp_render::ModelInstance> {
        let list = std::mem::take(&mut self.play_world.detail_models);
        let mut out = Vec::with_capacity(list.len());
        let mut pending = false;
        let [dist, fade] = self.play_world.detail_fade;
        for (k, m) in list.iter().enumerate() {
            match self.render_model(renderer, &m.path, 0) {
                Some(model) => out.push(hp_render::ModelInstance {
                    model,
                    transform: m.transform.as_mat4().to_cols_array_2d(),
                    object: detail_model_object(k),
                    tint: m.tint,
                    fade: hp_render::ModelInstance::detail_fade(dist, fade),
                }),
                None => pending |= self.model_loading(&m.path),
            }
        }
        self.play_world.detail_models = list;
        let world = self.play_world.uploaded.as_ref().map(|(w, _)| Arc::clone(w));
        for (k, p) in world.iter().flat_map(|w| w.static_props.iter().take(MAX_STATIC_PROPS).enumerate()) {
            match self.render_model(renderer, &p.model, p.skin) {
                Some(model) => out.push(hp_render::ModelInstance {
                    model,
                    transform: static_prop_matrix(p).as_mat4().to_cols_array_2d(),
                    object: static_prop_object(k),
                    tint: [1.0; 4],
                    fade: p.fade,
                }),
                None => pending |= self.model_loading(&p.model),
            }
        }
        if let Some(world) = &world {
            if self.play_world.ropes.is_none() {
                self.play_world.ropes = self.play_ropes(renderer, world);
                pending |= self.play_world.ropes.is_none();
            }
            if let Some(model) = self.play_world.ropes.as_ref().and_then(|r| r.model) {
                out.push(hp_render::ModelInstance {
                    model,
                    transform: glam::Mat4::IDENTITY.to_cols_array_2d(),
                    object: rope_object(),
                    tint: [1.0; 4],
                    fade: [0.0; 2],
                });
            }
        }
        self.play_world.detail_models_pending = pending;
        out
    }

    fn play_ropes(&mut self, renderer: &mut Renderer, world: &BspWorld) -> Option<PlayRopes> {
        use crate::helpers::ropes;
        let list: Vec<(&hp_vmf::Entity, glam::DVec3)> =
            world.rope_entities.iter().filter_map(|e| Some((e, e.origin().filter(|o| o.is_finite())?))).collect();
        let segs = ropes::list_segments(&list);
        let mut sizes = Vec::with_capacity(segs.len());
        for s in &segs {
            let params = ropes::RopeParams::of(list[s.start].0);
            let info = self.material_info_impl(&params.material)?;
            let height = if info.missing || info.width == 0 || info.height == 0 {
                ropes::MISSING_SIZE
            } else {
                f64::from(info.height)
            };
            sizes.push((params, height));
        }
        if segs.is_empty() {
            return Some(PlayRopes { model: None, colors: Vec::new() });
        }
        let mut parts: Vec<(hp_render::ModelPart, Vec<[u8; 4]>)> = Vec::new();
        for (s, (params, height)) in segs.iter().zip(sizes) {
            let a = list[s.start].1;
            let b = s.end.map(|i| list[i].1);
            let (nodes, length) = ropes::rest_nodes(a, b, &params);
            let pts = ropes::rope_points(&nodes, &params, length);
            let (vertices, indices) = ropes::rope_tube(&pts, params.width, 1.0, 1.0 / height);
            if vertices.is_empty() {
                continue;
            }
            let lights: Vec<[f32; 3]> = nodes.iter().map(|n| rope_light(world, n.as_vec3().to_array())).collect();
            let ring = vertices.len() / pts.points.len().max(1);
            let mut colors = Vec::with_capacity(vertices.len());
            for at in &pts.at_node {
                let i = (at.floor() as usize).min(lights.len() - 1);
                let j = (i + 1).min(lights.len() - 1);
                let t = (*at - i as f64) as f32;
                let c: [f32; 3] = std::array::from_fn(|k| lights[i][k] + (lights[j][k] - lights[i][k]) * t);
                colors.extend(std::iter::repeat_n(vhv_color(c), ring));
            }
            let material = self.asset_material(&params.material, crate::material::MatKind::Model);
            let slot = match parts.iter().position(|(p, _)| p.material == material) {
                Some(k) => k,
                None => {
                    let part = hp_render::ModelPart { material, ..Default::default() };
                    parts.push((part, Vec::new()));
                    parts.len() - 1
                }
            };
            let (part, part_colors) = &mut parts[slot];
            let base = part.vertices.len() as u32;
            part.vertices.extend(vertices);
            part.indices.extend(indices.iter().map(|i| base + i));
            part_colors.extend(colors);
        }
        let colors: Vec<[u8; 4]> = parts.iter().flat_map(|(_, c)| c.iter().copied()).collect();
        let parts: Vec<hp_render::ModelPart> = parts.into_iter().map(|(p, _)| p).collect();
        let model = (!parts.is_empty()).then(|| renderer.create_model(&parts));
        Some(PlayRopes { model, colors })
    }

    pub(crate) fn play_world_active(&self) -> bool {
        self.play_world.uploaded.is_some() || self.play_world.wanted.is_some()
    }

    pub(crate) fn model_loading(&self, path: &str) -> bool {
        matches!(self.models.map.get(path), None | Some(crate::models::ModelEntry::Loading))
    }

    pub(crate) fn poll_detail_models(&mut self) {
        if self.play_world.detail_models_pending {
            self.scene.instances_dirty = true;
        }
    }
}

impl AssetManager {
    pub(crate) fn play_model_lighting(
        &mut self,
        renderer: &mut Renderer,
        list: &[ModelInstance],
        origins: &ahash::AHashMap<u32, [f32; 3]>,
    ) {
        let world = self.play_world.uploaded.as_ref().map(|(w, _)| Arc::clone(w));
        let world = world.filter(|w| w.light.mode != LightMode::Fullbright);
        let baked = if world.is_none() { self.play_world.baked.clone() } else { None };
        let source = match (&world, &baked) {
            (Some(w), _) => Arc::as_ptr(w) as usize,
            (None, Some((p, _))) => Arc::as_ptr(p) as usize,
            (None, None) => {
                if !self.play_world.model_lights_sent.is_empty() {
                    self.play_world.model_lights_sent.clear();
                    self.play_world.model_lights.clear();
                    renderer.set_model_lighting(&[]);
                }
                return;
            }
        };
        if self.play_world.model_lights_source != source {
            self.play_world.model_lights_source = source;
            self.play_world.model_lights.clear();
        }
        let world_index = world.as_ref().map(|_| std::mem::take(&mut self.play_world.static_props));
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::with_capacity(list.len());
        for inst in list {
            let entity = inst.object & crate::entities::ENTITY_BIT != 0;
            if !(entity || inst.object == crate::player::PLAYER_OBJECT) || !seen.insert(inst.object) {
                continue;
            }
            let at = origins.get(&inst.object).copied();
            let fresh = self
                .play_world
                .model_lights
                .get(&inst.object)
                .is_some_and(|(m, t, o, _)| *m == inst.model && *t == inst.transform && *o == at);
            if fresh {
                continue;
            }
            let lighting = match (&world, &world_index, &baked) {
                (Some(w), Some(index), _) => self.instance_lighting(Some(w), &w.light.props, index, inst, at),
                (None, _, Some(_)) if !in_region(self.play_world.baked_region, &inst.transform) => continue,
                (None, _, Some((props, index))) => self.instance_lighting(None, props, index, inst, at),
                _ => continue,
            };
            self.play_world.model_lights.insert(inst.object, (inst.model, inst.transform, at, lighting));
        }
        if let Some(index) = world_index {
            self.play_world.static_props = index;
        }
        self.play_world.model_lights.retain(|o, _| seen.contains(o));
        let mut out: Vec<ModelLighting> = self.play_world.model_lights.values().map(|(_, _, _, l)| l.clone()).collect();
        out.sort_by_key(|l| l.object);
        if out != self.play_world.model_lights_sent {
            renderer.set_model_lighting(&out);
            self.play_world.model_lights_sent = out;
        }
    }

    fn instance_lighting(
        &mut self,
        world: Option<&BspWorld>,
        props: &world_light::PropLighting,
        index: &StaticPropIndex,
        inst: &ModelInstance,
        lighting_origin: Option<[f32; 3]>,
    ) -> ModelLighting {
        if world.is_some() && inst.object == rope_object() {
            let colors = self.play_world.ropes.as_ref().map(|r| r.colors.clone()).unwrap_or_default();
            return ModelLighting {
                object: inst.object,
                model: Some(inst.model),
                vertex_colors: colors,
                ambient: [[0.0; 3]; 6],
                lights: Vec::new(),
                styled_lights: Vec::new(),
            };
        }
        let m = glam::Mat4::from_cols_array_2d(&inst.transform);
        let origin = m.w_axis.truncate();
        let detail = world.and_then(|_| detail_model_index(inst.object));
        let path = if let Some(k) = detail {
            self.play_world.detail_models.get(k).map(|m| m.path.clone())
        } else if let (Some(world), Some(k)) = (world, static_prop_index(inst.object)) {
            world.static_props.get(k).map(|p| p.model.clone())
        } else if inst.object & crate::entities::ENTITY_BIT != 0 {
            let id = inst.object & !crate::entities::ENTITY_BIT;
            self.scene.ents.get(&id).and_then(|s| s.desc.model.as_ref()).map(|(p, _)| p.clone())
        } else {
            None
        };
        let data = path.as_deref().and_then(|p| self.model_data(p));
        let prop = path
            .as_deref()
            .filter(|_| detail.is_none())
            .and_then(|p| world_light::find_static_prop(&props.static_props, index, p, origin.to_array()));
        let center = if inst.object == crate::player::PLAYER_OBJECT {
            glam::Vec3::new(0.0, 0.0, 36.0)
        } else {
            data.as_ref().map_or(glam::Vec3::ZERO, |d| ((d.info.bounds_min + d.info.bounds_max) * 0.5).as_vec3())
        };
        let lighting_origin = match (prop.and_then(|p| p.lighting_origin), lighting_origin) {
            (Some(o), _) | (None, Some(o)) => o,
            (None, None) => m.transform_point3(center).to_array(),
        };
        let lit = props.model_lighting(lighting_origin);
        let colors = prop.and_then(|p| p.colors.as_ref()).and_then(|colors| {
            let data = data.as_ref()?;
            let mut out = Vec::new();
            for part in &data.parts {
                if part.color_index.len() != part.vertices.len() {
                    return None;
                }
                for &i in &part.color_index {
                    out.push(*colors.get(i as usize)?);
                }
            }
            if let Some(key) = path.as_deref()
                && let Some((_, lods)) = self.lods.data.get(key)
            {
                for parts in &lods.lods {
                    for part in parts.iter().filter(|p| p.choice == 0) {
                        if part.color_index.len() != part.vertices.len() {
                            return None;
                        }
                        for &i in &part.color_index {
                            out.push(*colors.get(i as usize)?);
                        }
                    }
                }
            }
            (!out.is_empty()).then_some(out)
        });
        ModelLighting {
            object: inst.object,
            model: colors.as_ref().map(|_| inst.model),
            vertex_colors: colors.unwrap_or_default(),
            ambient: lit.ambient,
            lights: lit.lights.iter().map(local_light).collect(),
            styled_lights: lit.styled.iter().map(local_light).collect(),
        }
    }
}

impl AssetManager {
    pub(crate) fn set_light_styles_impl(&mut self, values: &[f32]) {
        if self.play_world.style_values != values {
            self.play_world.style_values = values.to_vec();
        }
    }

    pub(crate) fn sync_light_styles(&mut self, renderer: &mut Renderer) {
        let uploaded = self.play_world.uploaded.as_ref().map(|(w, o)| (Arc::clone(w), o.len()));
        let Some((world, groups)) = uploaded else {
            if self.play_world.styles_sent.take().is_some() {
                renderer.set_light_styles(&[]);
            }
            return;
        };
        let values: Vec<f32> = if self.play_world.style_values.is_empty() {
            let t = &world.light.styles;
            t.values(0.0, &t.initial()).to_vec()
        } else {
            self.play_world.style_values.clone()
        };
        let key = Arc::as_ptr(&world) as usize;
        let before = match &self.play_world.styles_sent {
            Some((k, v)) if *k == key && *v == values => return,
            Some((k, v)) if *k == key => Some(v.clone()),
            _ => None,
        };
        renderer.set_light_styles(&values);
        let changed = |s: u8| before.as_ref().is_none_or(|b| b.get(usize::from(s)) != values.get(usize::from(s)));
        let slots = |kept: &mut dyn Iterator<Item = bool>| -> Vec<Option<u32>> {
            let mut n = 0u32;
            kept.map(|k| {
                k.then(|| {
                    n += 1;
                    n - 1
                })
            })
            .collect()
        };
        let sprites = slots(&mut world.details.iter().map(|d| (d.group as usize) < groups));
        let models = slots(&mut world.detail_models.iter().map(|m| (m.group as usize) < groups));
        let mut colors = Vec::new();
        let mut tinted = false;
        for d in &world.light.detail_styles {
            if !changed(0) && !d.styles.iter().any(|(s, _)| changed(*s)) {
                continue;
            }
            let c = d.color(&values);
            let slot = if d.model { models.get(d.index as usize) } else { sprites.get(d.index as usize) };
            let Some(&Some(slot)) = slot else { continue };
            if !d.model {
                colors.push((slot, c));
            } else if let Some(m) = self.play_world.detail_models.get_mut(slot as usize) {
                m.tint = [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0, 1.0];
                tinted = true;
            }
        }
        renderer.set_detail_sprite_colors(&colors);
        if tinted {
            self.scene.instances_dirty = true;
        }
        self.play_world.styles_sent = Some((key, values));
    }
}

fn local_light(l: &world_light::LocalLight) -> ModelLocalLight {
    ModelLocalLight {
        kind: match l.kind {
            EmitKind::Spot => 1,
            EmitKind::Sky | EmitKind::Surface => 2,
            _ => 0,
        },
        origin: l.origin,
        dir: l.dir,
        color: l.color,
        attenuation: l.attenuation,
        cone: l.cone,
        radius: l.radius,
        style: u32::from(l.style),
    }
}

fn face_object(i: usize) -> u32 {
    WORLD_OBJECT_BASE + 2 * i as u32
}

fn disp_object(i: usize) -> u32 {
    WORLD_OBJECT_BASE + 2 * i as u32 + 1
}

fn compiled_vertex(v: &WorldVertex, normal: [f32; 3], light: u32, cubemap: u16) -> CompiledVertex {
    let blend = v.multiblend.map_or_else(hp_render::BlendVertex::default, hp_render::BlendVertex::painted);
    CompiledVertex { pos: v.pos, normal, uv: v.uv, alpha: v.alpha, lm: v.lm, lightmap: light, cubemap, blend }
}

const MAX_WORLD_CUBE: u32 = 256;

pub(crate) fn decode_world_cubemap(bytes: &[u8], hdr: bool) -> Option<(u32, Vec<[f32; 3]>)> {
    let info = hp_vtf::read_info(bytes).ok()?;
    if !info.is_cubemap() {
        return None;
    }
    let mip = info.mip_for_max_dimension(MAX_WORLD_CUBE);
    let mut size = 0;
    let mut out = Vec::new();
    for face in 0..6 {
        let level = hp_vtf::raw_level(bytes, mip, 0, face).ok()?;
        if level.width != level.height || level.width == 0 || (size != 0 && level.width != size) {
            return None;
        }
        size = level.width;
        let texels = if hdr {
            crate::render::hdr_texels(&level, false)?
        } else {
            let img = level.to_rgba8().ok()?;
            let lin = |c: u8| {
                let c = f32::from(c) / 255.0;
                if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
            };
            img.pixels.chunks_exact(4).map(|p| [lin(p[0]), lin(p[1]), lin(p[2])]).collect()
        };
        out.extend(texels);
    }
    let lit = out.iter().any(|c| c.iter().any(|v| *v > 1e-4));
    lit.then_some((size, out))
}

impl AssetManager {
    fn world_cubemaps(&mut self, renderer: &mut Renderer, world: &BspWorld) {
        let hdr = world.light.mode == LightMode::Hdr;
        let (mut built, mut drawn) = (0, 0);
        for s in &world.cubemaps {
            if self.play_world.cubes.contains_key(&s.origin) {
                continue;
            }
            let data = if hdr {
                s.hdr.as_ref().map(|d| (d, true)).or(s.ldr.as_ref().map(|d| (d, false)))
            } else {
                s.ldr.as_ref().map(|d| (d, false)).or(s.hdr.as_ref().map(|d| (d, true)))
            };
            let image = data.and_then(|(d, is_hdr)| decode_world_cubemap(d, is_hdr));
            let origin = s.origin.map(|v| v as f32);
            let editor = self.editor_cubes.probe_at(s.origin);
            let parallax = s.parallax.map(|(rows, origin)| hp_render::parallax::ParallaxBox { rows, origin });
            let id = match image {
                Some((size, texels)) => {
                    let n = (size * size) as usize;
                    let faces: [&[[f32; 3]]; 6] = std::array::from_fn(|f| &texels[f * n..(f + 1) * n]);
                    let id = renderer.create_cubemap_image(&CubemapImage { size, faces, origin: Some(origin) });
                    if let Some(id) = id {
                        self.play_world.owned_cubes.push(id);
                        renderer.set_cubemap_parallax(id, parallax);
                        built += 1;
                    }
                    self.play_world.drawn_cubes.push((s.origin, editor));
                    id
                }
                None => {
                    drawn += 1;
                    let id = match editor {
                        Some(id) => Some(id),
                        None => {
                            let size = if (1..=16).contains(&s.size) { 1u32 << (s.size - 1) } else { 0 };
                            let id = renderer.create_cubemap_probe(&CubemapProbe { origin, size, sides: Vec::new() });
                            if let Some(id) = id {
                                self.play_world.owned_cubes.push(id);
                                renderer.set_cubemap_parallax(id, parallax);
                            }
                            id
                        }
                    };
                    self.play_world.drawn_cubes.push((s.origin, id));
                    id
                }
            };
            if let Some(id) = id {
                self.play_world.cubes.insert(s.origin, id);
            }
        }
        let ids: Vec<CubemapId> = self.play_world.cubes.values().copied().collect();
        renderer.set_model_cubemaps(Some(&ids));
        self.play_world.cube_sources = Some((built, drawn));
        log::info!("player view: {} cubemaps built in game, {} drawn by the editor", built, drawn);
    }

    pub(crate) fn play_cubemap_sources(&self) -> Option<(usize, usize)> {
        self.play_world.cube_sources
    }

    pub(crate) fn play_cubemap_targets(&self) -> Vec<([i32; 3], Option<CubemapId>)> {
        self.play_world.drawn_cubes.clone()
    }
}

fn object_flags(disp: bool, hidden: bool) -> ObjectFlags {
    let mut flags = ObjectFlags::NO_2D | ObjectFlags::COMPILED_LIGHT;
    if disp {
        flags |= ObjectFlags::DISPLACEMENT;
    }
    if hidden {
        flags |= ObjectFlags::HIDDEN;
    }
    flags
}

fn disp_normals(verts: &[hp_editor::limits::world::WorldVertex], tris: &[u32], face_normal: [f32; 3]) -> Vec<[f32; 3]> {
    let mut acc = vec![[0.0f32; 3]; verts.len()];
    for t in tris.chunks_exact(3) {
        let (Some(a), Some(b), Some(c)) =
            (verts.get(t[0] as usize), verts.get(t[1] as usize), verts.get(t[2] as usize))
        else {
            continue;
        };
        let u = [b.pos[0] - a.pos[0], b.pos[1] - a.pos[1], b.pos[2] - a.pos[2]];
        let v = [c.pos[0] - a.pos[0], c.pos[1] - a.pos[1], c.pos[2] - a.pos[2]];
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        for &i in t {
            if let Some(s) = acc.get_mut(i as usize) {
                s[0] += n[0];
                s[1] += n[1];
                s[2] += n[2];
            }
        }
    }
    let sum = acc.iter().fold([0.0f32; 3], |s, n| [s[0] + n[0], s[1] + n[1], s[2] + n[2]]);
    let side =
        if sum[0] * face_normal[0] + sum[1] * face_normal[1] + sum[2] * face_normal[2] < 0.0 { -1.0 } else { 1.0 };
    acc.into_iter()
        .map(|n| {
            let n = [n[0] * side, n[1] * side, n[2] * side];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-6 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0, 0.0, 1.0] }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baked_region_holds_its_models() {
        let at =
            |x: f32, y: f32, z: f32| [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, y, z, 1.0]];
        assert!(in_region(None, &at(1.0e6, 0.0, 0.0)), "a whole map bake lights every model");
        let r = Some([[-100.0, -100.0, -100.0], [100.0, 100.0, 100.0]]);
        assert!(in_region(r, &at(0.0, 50.0, -100.0)));
        assert!(!in_region(r, &at(0.0, 150.0, 0.0)));
        assert!(!in_region(r, &at(f32::NAN, 0.0, 0.0)));
    }

    #[test]
    fn built_cubemaps_are_read_and_placeholders_left_out() {
        let faces: [Vec<[f32; 3]>; 6] = std::array::from_fn(|f| vec![[0.5, f as f32 * 0.4, 3.0]; 16]);
        let ldr = hp_bsp::cubemaps::cubemap_vtf(4, &faces, false);
        let hdr = hp_bsp::cubemaps::cubemap_vtf(4, &faces, true);
        let (size, texels) = decode_world_cubemap(&hdr, true).expect("hdr");
        assert_eq!((size, texels.len()), (4, 96));
        assert!((texels[16 * 5][2] - 3.0).abs() < 0.01, "HDR above white: {:?}", texels[16 * 5]);
        assert!((texels[16 * 2][1] - 0.8).abs() < 0.01);
        let (_, texels) = decode_world_cubemap(&ldr, false).expect("ldr");
        assert!((texels[0][0] - 0.5).abs() < 0.01, "sRGB decoded: {:?}", texels[0]);
        assert!((texels[0][2] - 1.0).abs() < 0.01, "clamped to white");
        let black: [Vec<[f32; 3]>; 6] = std::array::from_fn(|_| vec![[0.0; 3]; 16]);
        assert!(decode_world_cubemap(&hp_bsp::cubemaps::cubemap_vtf(4, &black, false), false).is_none());
        assert!(decode_world_cubemap(b"not a vtf", false).is_none());
    }

    #[test]
    fn groups_follow_the_drawn_list() {
        assert!(group_drawn(3, None));
        assert!(!group_drawn(1, Some(&vec![true, false])));
        assert!(group_drawn(0, Some(&vec![true, false])));
        assert!(group_drawn(9, Some(&vec![false])), "past the list");
    }

    #[test]
    fn displacement_normals_face_like_their_base_face() {
        use hp_editor::limits::world::{BspWorld, WorldFace, WorldVertex};
        let mut verts = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                verts.push(WorldVertex {
                    pos: [j as f32 * 8.0, i as f32 * 8.0, 0.0],
                    uv: [0.0; 2],
                    alpha: 0.0,
                    lm: [0.0; 2],
                    multiblend: None,
                });
            }
        }
        let face = WorldFace {
            material: 0,
            first: 0,
            count: 9,
            disp_power: 1,
            sky: false,
            normal: [0.0, 0.0, 1.0],
            dist: 0.0,
            light: hp_editor::limits::world::NO_LIGHTMAP,
        };
        let tris = BspWorld::face_triangles(&face);
        for n in disp_normals(&verts, &tris, face.normal) {
            assert!(n[2] > 0.99, "{n:?}");
        }
        for n in disp_normals(&verts, &tris, [0.0, 0.0, -1.0]) {
            assert!(n[2] < -0.99, "{n:?}");
        }
    }

    #[test]
    fn world_objects_split_faces_and_displacements() {
        assert_eq!(face_object(0), WORLD_OBJECT_BASE);
        assert_eq!(disp_object(0), WORLD_OBJECT_BASE + 1);
        assert_eq!(face_object(3), WORLD_OBJECT_BASE + 6);
        assert!(disp_object(MAX_GROUPS - 1) < crate::player::PLAYER_OBJECT);
        assert!(detail_object(MAX_GROUPS - 1) < face_object(0));
        assert!(detail_model_object(MAX_DETAIL_MODELS - 1) < detail_object(0));
        assert!(detail_model_object(0) > crate::entities::ENTITY_BIT | 0x7000_0000);
        assert_eq!(detail_model_index(detail_model_object(5)), Some(5));
        assert_eq!(detail_model_index(detail_model_object(MAX_DETAIL_MODELS - 1)), Some(MAX_DETAIL_MODELS - 1));
        assert_eq!(detail_model_index(detail_object(0)), None);
        assert_eq!(detail_model_index(crate::player::PLAYER_OBJECT), None);
        assert!(static_prop_object(MAX_STATIC_PROPS - 1) < detail_model_object(0));
        assert!(static_prop_object(0) > crate::entities::ENTITY_BIT | 0x7000_0000);
        assert_eq!(static_prop_index(static_prop_object(3)), Some(3));
        assert_eq!(static_prop_index(detail_model_object(0)), None);
        assert_eq!(detail_model_index(static_prop_object(0)), None);
        let p = hp_editor::limits::world::StaticPropPlacement {
            origin: [10.0, 0.0, 0.0],
            angles: [0.0, 90.0, 0.0],
            ..Default::default()
        };
        let m = static_prop_matrix(&p);
        assert!((m.transform_point3(glam::DVec3::X) - glam::DVec3::new(10.0, 1.0, 0.0)).length() < 1e-9, "yaw 90");
    }

    #[test]
    fn detail_models_take_the_light_the_client_gives_them() {
        let lin = |s: f32| if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) };
        let t = detail_tint([0.25, 0.5, 0.0]);
        assert!((lin(t[0]) - 0.25).abs() < 1e-4 && (lin(t[1]) - 0.5).abs() < 1e-4 && t[2] == 0.0, "{t:?}");
        assert_eq!(t[3], 1.0);
        let bright = detail_tint([3.0, 0.93, 0.5]);
        assert!(bright[0] > 0.9999 && bright[1] > 0.9999 && bright[2] < 0.8, "{bright:?}");
        assert_eq!(detail_tint([f32::NAN, -1.0, f32::INFINITY]), [0.0, 0.0, 0.0, 1.0]);
        assert!(object_flags(true, false).contains(ObjectFlags::DISPLACEMENT));
        assert!(!object_flags(false, false).contains(ObjectFlags::DISPLACEMENT));
        assert!(object_flags(false, true).contains(ObjectFlags::HIDDEN | ObjectFlags::NO_2D));
        assert!(object_flags(true, false).contains(ObjectFlags::COMPILED_LIGHT), "lit by its lightmaps");
    }
}
