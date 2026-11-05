use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ahash::{AHashMap, AHashSet};
use glam::{DVec3, Vec3};
use hp_app::runtime::render::{DetailPreviewReport, DetailPreviewRequest, DetailTypeReport};
use hp_bspc::detail_props::{self, DetailDict, DetailLump, ScatterSurface, kind};
use hp_editor::{Document, Owner};
use hp_render::{DetailSprite, DetailSpriteOrientation, ModelInstance, ObjectFlags, Renderer};

use crate::AssetManager;
use crate::models::entity_matrix;

const MAX_SPRITES: usize = 60_000;
const MAX_MODELS: usize = 1_500;
const SURFACE_CAP: usize = 20_000;
const RECENTER: f32 = 96.0;
const MARGIN: f32 = 64.0;
const BUDGET: Duration = Duration::from_millis(5);
const PARTIAL_UPLOAD: Duration = Duration::from_millis(300);
const CACHE_LIMIT: usize = 6_000;
const REPORTED_TYPES: usize = 64;
const DEFAULT_MATERIAL: &str = "detail/detailsprites";
const SPRITE_OBJECT: u32 = u32::MAX - (1 << 21) - (5 << 20);
const MODEL_OBJECT: u32 = u32::MAX - (1 << 21) - (6 << 20);

struct Surface {
    key: u64,
    type_name: String,
    surface: ScatterSurface,
    mins: Vec3,
    maxs: Vec3,
    seed: u32,
}

#[derive(Default)]
struct Generated {
    sprites: Vec<DetailSprite>,
    models: Vec<(String, glam::DMat4)>,
}

#[derive(Clone)]
pub(crate) struct PreviewModel {
    path: String,
    transform: glam::DMat4,
}

#[derive(Default)]
pub(crate) struct DetailPreview {
    revision: Option<u64>,
    fs_gen: u64,
    surfaces: Vec<Surface>,
    displacements: usize,
    brush_faces: usize,
    types: Vec<DetailTypeReport>,
    type_cache: AHashMap<String, Option<String>>,
    dict: Option<Arc<DetailDict>>,
    dict_key: Option<(String, u64)>,
    dict_file: Option<String>,
    dict_error: Option<String>,
    sprite_material: String,
    cache: AHashMap<u64, Arc<Generated>>,
    anchor: Option<[f32; 3]>,
    pending: bool,
    uploaded: bool,
    uploaded_sig: Option<u64>,
    last_upload: Option<Instant>,
    models: Vec<PreviewModel>,
    models_sig: u64,
    models_pending: bool,
    fade: [f32; 2],
    sprites: usize,
    capped: bool,
    surfaces_in_range: usize,
}

fn hash_of(f: impl FnOnce(&mut DefaultHasher)) -> u64 {
    let mut h = DefaultHasher::new();
    f(&mut h);
    h.finish()
}

fn v3(v: DVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn right_up(a: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (sp, cp) = a[0].to_radians().sin_cos();
    let (sy, cy) = a[1].to_radians().sin_cos();
    let (sr, cr) = a[2].to_radians().sin_cos();
    let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    (right, up)
}

fn rotate_about(v: [f32; 3], axis: [f32; 3], degrees: f32) -> [f32; 3] {
    let (v, k) = (Vec3::from(v), Vec3::from(axis).normalize_or_zero());
    let (s, c) = degrees.to_radians().sin_cos();
    (v * c + k.cross(v) * s + k * k.dot(v) * (1.0 - c)).to_array()
}

pub(crate) fn sprites_of(lump: &DetailLump) -> Vec<DetailSprite> {
    let mut out = Vec::with_capacity(lump.objects.len());
    for (i, o) in lump.objects.iter().enumerate() {
        if o.kind == kind::MODEL {
            continue;
        }
        let Some(d) = lump.sprites.get(usize::from(o.dict)) else { continue };
        let origin = o.origin.to_array();
        let angles = o.angles.to_array();
        if !origin.iter().chain(&angles).all(|v| v.is_finite()) {
            continue;
        }
        let scale = if o.scale.is_finite() { o.scale } else { 1.0 };
        let (right, up) = right_up(angles);
        let (u0, u1) = if i % 2 == 1 { (d[4], d[6]) } else { (d[6], d[4]) };
        let orientation = match o.orientation {
            1 => DetailSpriteOrientation::ScreenAligned,
            2 => DetailSpriteOrientation::Vertical,
            _ => DetailSpriteOrientation::Fixed,
        };
        let quads: &[f32] = match o.kind {
            kind::SHAPE_CROSS => &[0.0, 90.0],
            kind::SHAPE_TRI => &[0.0, 60.0, 120.0],
            _ => &[0.0],
        };
        let base = f32::from(o.shape_angle) * 360.0 / 256.0;
        for q in quads {
            let turn = if quads.len() > 1 { base + q } else { 0.0 };
            out.push(DetailSprite {
                origin,
                right: rotate_about(right, up, turn),
                up,
                ul: [d[0] * scale, d[1] * scale],
                lr: [d[2] * scale, d[3] * scale],
                tex: [u0, d[5], u1, d[7]],
                orientation,
                sway: o.sway,
                color: [255; 4],
                object: SPRITE_OBJECT,
            });
        }
    }
    out
}

fn models_of(lump: &DetailLump) -> Vec<(String, glam::DMat4)> {
    lump.objects
        .iter()
        .filter(|o| o.kind == kind::MODEL)
        .filter_map(|o| {
            let name = lump.models.get(usize::from(o.dict))?.to_ascii_lowercase();
            let (p, a) = (o.origin, o.angles);
            if name.is_empty() || !p.is_finite() || !a.is_finite() {
                return None;
            }
            let origin = DVec3::new(p.x.into(), p.y.into(), p.z.into());
            let angles = DVec3::new(a.x.into(), a.y.into(), a.z.into());
            Some((name, entity_matrix(origin, angles, 1.0)))
        })
        .collect()
}

fn generate(s: &Surface, dict: &DetailDict) -> Generated {
    let Some(t) = dict.find(&s.type_name) else { return Generated::default() };
    let lump = detail_props::scatter(&s.surface, t, s.seed, SURFACE_CAP, &|m| !m.trim().is_empty());
    Generated { sprites: sprites_of(&lump), models: models_of(&lump) }
}

fn read_game_file(fs: &hp_fs::GameFs, path: &str) -> Option<Vec<u8>> {
    if let Some(bytes) = fs.read(path) {
        return Some(bytes);
    }
    let rel = std::path::Path::new(path.trim_start_matches(['/', '\\']));
    if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return None;
    }
    let sources = fs.sources();
    let dirs = [hp_fs::SourceOrigin::Extra, hp_fs::SourceOrigin::GameLoose]
        .iter()
        .flat_map(|o| sources.iter().filter(move |s| s.origin == *o).map(|s| s.path.clone()))
        .chain(fs.game_dir().map(std::path::Path::to_path_buf));
    dirs.filter(|d| d.is_dir()).find_map(|d| std::fs::read(d.join(rel)).ok())
}

fn box_distance2(mins: Vec3, maxs: Vec3, p: Vec3) -> f32 {
    let d = (mins - p).max(p - maxs).max(Vec3::ZERO);
    d.length_squared()
}

impl AssetManager {
    fn detail_type_of(&mut self, material: &str) -> Option<String> {
        let key = hp_vmt::normalize_material_name(material);
        if let Some(hit) = self.detail_preview.type_cache.get(&key) {
            return hit.clone();
        }
        let found = self.fs.as_ref().and_then(|fs| fs.material(&key)).and_then(|m| {
            let m = m.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
            m.get("%detailtype").map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
        });
        self.detail_preview.type_cache.insert(key, found.clone());
        found
    }

    fn refresh_detail_dict(&mut self, doc: &Document) {
        let world = doc.scene().world();
        let wanted = world.get("detailvbsp").map(str::trim).filter(|s| !s.is_empty()).unwrap_or("detail.vbsp");
        let generation = self.fs.as_ref().map_or(0, |fs| fs.generation());
        let key = (wanted.to_owned(), generation);
        let material = world.get("detailmaterial").map(str::trim).filter(|s| !s.is_empty()).unwrap_or(DEFAULT_MATERIAL);
        self.detail_preview.sprite_material = material.to_owned();
        if self.detail_preview.dict_key.as_ref() == Some(&key) {
            return;
        }
        let dp = &mut self.detail_preview;
        dp.dict_key = Some(key);
        dp.dict = None;
        dp.dict_file = None;
        dp.dict_error = None;
        dp.revision = None;
        dp.cache.clear();
        let mut names = vec![wanted.to_owned()];
        for fallback in ["detail.vbsp", "scripts/detail.vbsp"] {
            if !names.iter().any(|n| n.eq_ignore_ascii_case(fallback)) {
                names.push(fallback.to_owned());
            }
        }
        let Some(fs) = self.fs.as_ref() else {
            dp.dict_error = Some("no game mounted".to_owned());
            return;
        };
        let Some((name, bytes)) = names.into_iter().find_map(|n: String| read_game_file(fs, &n).map(|b| (n, b))) else {
            dp.dict_error = Some(format!("{wanted} not found in the game content"));
            return;
        };
        dp.dict_file = Some(name);
        match detail_props::parse_detail_vbsp(&String::from_utf8_lossy(&bytes)) {
            Ok(d) => dp.dict = Some(Arc::new(d)),
            Err(e) => dp.dict_error = Some(e),
        }
    }

    fn rebuild_detail_surfaces(&mut self, doc: &Document) {
        let Some(dict) = self.detail_preview.dict.clone() else {
            let dp = &mut self.detail_preview;
            dp.surfaces.clear();
            dp.types.clear();
            dp.displacements = 0;
            dp.brush_faces = 0;
            dp.anchor = None;
            return;
        };
        let mut surfaces = Vec::new();
        let mut counts: Vec<(String, String, usize)> = Vec::new();
        let (mut displacements, mut brush_faces) = (0usize, 0usize);
        let scene = doc.scene();
        let mut ids: Vec<_> = scene.solids().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(node) = scene.solid(id) else { continue };
            let in_world = match node.owner {
                Owner::World => true,
                Owner::Entity(e) => {
                    scene.entity(e).is_some_and(|n| n.entity.classname.eq_ignore_ascii_case("func_detail"))
                }
            };
            if !in_world || !doc.is_solid_visible(id) {
                continue;
            }
            for (idx, side) in node.solid.sides.iter().enumerate() {
                let Some(type_name) = self.detail_type_of(&side.material) else { continue };
                match counts.iter_mut().find(|c| c.0.eq_ignore_ascii_case(&side.material) && c.1 == type_name) {
                    Some(c) => c.2 += 1,
                    None => counts.push((side.material.to_ascii_lowercase(), type_name.clone(), 1)),
                }
                if dict.find(&type_name).is_none() {
                    continue;
                }
                let (surface, mins, maxs) = match node.disp.get(idx).and_then(Option::as_ref) {
                    Some(mesh) => {
                        let verts: Vec<Vec3> = mesh.positions.iter().map(|p| v3(*p)).collect();
                        let base: Vec<Vec3> = mesh.base_positions.iter().map(|p| v3(*p)).collect();
                        let alphas: Vec<f32> = mesh.alphas.iter().map(|a| *a as f32).collect();
                        displacements += 1;
                        let (mins, maxs) = (v3(mesh.aabb.min), v3(mesh.aabb.max));
                        let power = side.disp.as_ref().map_or(0, |d| d.power);
                        (ScatterSurface::Disp { power, base, verts, alphas }, mins, maxs)
                    }
                    None => {
                        let Some(face) = node.geom.face_for_side(idx) else { continue };
                        let points: Vec<Vec3> = face.vertices.iter().map(|p| v3(*p)).collect();
                        if points.len() < 3 {
                            continue;
                        }
                        brush_faces += 1;
                        let mins = points.iter().fold(Vec3::splat(f32::MAX), |m, p| m.min(*p));
                        let maxs = points.iter().fold(Vec3::splat(f32::MIN), |m, p| m.max(*p));
                        (ScatterSurface::Face { points, normal: v3(face.normal()) }, mins, maxs)
                    }
                };
                if !(mins.is_finite() && maxs.is_finite()) {
                    continue;
                }
                let key = hash_of(|h| {
                    side.id.hash(h);
                    type_name.hash(h);
                    match &surface {
                        ScatterSurface::Face { points, .. } => {
                            for p in points {
                                p.to_array().map(f32::to_bits).hash(h);
                            }
                        }
                        ScatterSurface::Disp { verts, alphas, .. } => {
                            for p in verts {
                                p.to_array().map(f32::to_bits).hash(h);
                            }
                            for a in alphas {
                                a.to_bits().hash(h);
                            }
                        }
                    }
                });
                let seed = (hash_of(|h| (side.id, id).hash(h)) & 0x7fff_ffff) as u32;
                surfaces.push(Surface { key, type_name, surface, mins, maxs, seed });
            }
        }
        let dp = &mut self.detail_preview;
        dp.types = counts
            .into_iter()
            .take(REPORTED_TYPES)
            .map(|(material, detail_type, surfaces)| DetailTypeReport {
                known: dict.find(&detail_type).is_some_and(|t| !t.groups.is_empty()),
                material,
                detail_type,
                surfaces,
            })
            .collect();
        dp.surfaces = surfaces;
        dp.displacements = displacements;
        dp.brush_faces = brush_faces;
        dp.anchor = None;
        let live: AHashSet<u64> = dp.surfaces.iter().map(|s| s.key).collect();
        dp.cache.retain(|k, _| live.contains(k));
    }

    fn detail_fade_of(doc: &Document) -> [f32; 2] {
        let controller = doc
            .scene()
            .entities()
            .values()
            .map(|n| &n.entity)
            .find(|e| e.classname.eq_ignore_ascii_case("env_detail_controller"))
            .map(|e| {
                let f = |k: &str, d: f32| e.get(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d);
                (f("fademindist", 400.0), f("fademaxdist", 1200.0))
            });
        hp_editor::limits::world::detail_fade(controller)
    }

    fn clear_detail_preview(&mut self, renderer: &mut Renderer) {
        let dp = &mut self.detail_preview;
        if dp.uploaded {
            renderer.set_detail_sprites(0, &[]);
            renderer.set_detail_fade(None);
            dp.uploaded = false;
        }
        dp.uploaded_sig = None;
        dp.sprites = 0;
        dp.anchor = None;
        dp.pending = false;
        dp.surfaces_in_range = 0;
        if !dp.models.is_empty() {
            dp.models.clear();
            dp.models_sig = 0;
            self.scene.instances_dirty = true;
        }
    }

    pub(crate) fn sync_detail_preview_impl(
        &mut self,
        doc: &Document,
        renderer: &mut Renderer,
        req: &DetailPreviewRequest,
    ) -> DetailPreviewReport {
        if self.play_world_active() {
            let dp = &mut self.detail_preview;
            dp.uploaded = false;
            dp.uploaded_sig = None;
            dp.anchor = None;
            dp.models.clear();
            return DetailPreviewReport { enabled: req.enabled, ..Default::default() };
        }
        if !req.enabled {
            self.clear_detail_preview(renderer);
            return DetailPreviewReport::default();
        }
        self.refresh_detail_dict(doc);
        let generation = self.fs.as_ref().map_or(0, |fs| fs.generation());
        if self.detail_preview.fs_gen != generation {
            self.detail_preview.fs_gen = generation;
            self.detail_preview.type_cache.clear();
            self.detail_preview.revision = None;
        }
        if self.detail_preview.revision != Some(doc.revision()) {
            self.detail_preview.revision = Some(doc.revision());
            self.rebuild_detail_surfaces(doc);
            let fade = Self::detail_fade_of(doc);
            if self.detail_preview.fade != fade {
                self.detail_preview.fade = fade;
                self.detail_preview.anchor = None;
                self.detail_preview.uploaded_sig = None;
            }
        }
        let eye = Vec3::from(req.eye);
        let moved = self.detail_preview.anchor.is_none_or(|a| Vec3::from(a).distance(eye) > RECENTER);
        if moved || self.detail_preview.pending {
            self.select_detail_surfaces(eye, renderer);
        }
        if self.detail_preview.models_pending {
            self.scene.instances_dirty = true;
        }
        let dp = &self.detail_preview;
        DetailPreviewReport {
            enabled: true,
            drawn: dp.uploaded && (dp.sprites > 0 || !dp.models.is_empty()),
            dict_file: dp.dict_file.clone(),
            dict_types: dp.dict.as_ref().map_or(0, |d| d.types.len()),
            dict_error: dp.dict_error.clone(),
            sprite_material: dp.sprite_material.clone(),
            surfaces: dp.surfaces.len(),
            displacements: dp.displacements,
            brush_faces: dp.brush_faces,
            surfaces_in_range: dp.surfaces_in_range,
            sprites: dp.sprites,
            models: dp.models.len(),
            capped: dp.capped,
            view_distance: dp.fade[0],
            fade_distance: dp.fade[1],
            types: dp.types.clone(),
        }
    }

    fn select_detail_surfaces(&mut self, eye: Vec3, renderer: &mut Renderer) {
        let Some(dict) = self.detail_preview.dict.clone() else {
            self.clear_detail_preview(renderer);
            return;
        };
        let started = Instant::now();
        let radius = self.detail_preview.fade[0] + MARGIN;
        let r2 = radius * radius;
        let mut near: Vec<(f32, usize)> = self
            .detail_preview
            .surfaces
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let d = box_distance2(s.mins, s.maxs, eye);
                (d <= r2).then_some((d, i))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut selection = Vec::with_capacity(near.len());
        let mut done = true;
        let mut sprites = 0usize;
        let mut models = 0usize;
        for (n, (_, i)) in near.iter().enumerate() {
            let dp = &mut self.detail_preview;
            let key = dp.surfaces[*i].key;
            if !dp.cache.contains_key(&key) {
                if n > 0 && started.elapsed() > BUDGET {
                    done = false;
                    break;
                }
                let g = generate(&dp.surfaces[*i], &dict);
                dp.cache.insert(key, Arc::new(g));
            }
            let g = &dp.cache[&key];
            if sprites >= MAX_SPRITES && models >= MAX_MODELS {
                break;
            }
            sprites += g.sprites.len();
            models += g.models.len();
            selection.push(key);
        }
        let dp = &mut self.detail_preview;
        dp.pending = !done;
        dp.surfaces_in_range = near.len();
        if done {
            dp.anchor = Some(eye.to_array());
        }
        if dp.cache.len() > CACHE_LIMIT {
            let keep: AHashSet<u64> = selection.iter().copied().collect();
            dp.cache.retain(|k, _| keep.contains(k));
        }
        let due = done || dp.last_upload.is_none_or(|t| t.elapsed() > PARTIAL_UPLOAD);
        if !due {
            return;
        }
        let sig = hash_of(|h| {
            selection.hash(h);
            dp.fade.map(f32::to_bits).hash(h);
        });
        dp.capped = sprites > MAX_SPRITES || models > MAX_MODELS || selection.len() < near.len() && done;
        if dp.uploaded_sig == Some(sig) {
            return;
        }
        let mut all: Vec<DetailSprite> = Vec::with_capacity(sprites.min(MAX_SPRITES));
        let mut placed: Vec<PreviewModel> = Vec::new();
        for key in &selection {
            let g = &dp.cache[key];
            let room = MAX_SPRITES - all.len();
            all.extend(g.sprites.iter().take(room).copied());
            for (path, transform) in &g.models {
                if placed.len() < MAX_MODELS {
                    placed.push(PreviewModel { path: path.clone(), transform: *transform });
                }
            }
        }
        dp.uploaded_sig = Some(sig);
        dp.last_upload = Some(Instant::now());
        dp.sprites = all.len();
        let model_sig = hash_of(|h| {
            for m in &placed {
                m.path.hash(h);
                m.transform.to_cols_array().map(f64::to_bits).hash(h);
            }
        });
        let models_changed = dp.models_sig != model_sig;
        dp.models_sig = model_sig;
        dp.models = placed;
        let material_name = dp.sprite_material.clone();
        let fade = dp.fade;
        let material = self.world_material(&material_name);
        renderer.set_object_flags(SPRITE_OBJECT, ObjectFlags::NO_2D);
        renderer.set_object_flags(MODEL_OBJECT, ObjectFlags::NO_2D);
        renderer.set_detail_sprites(material, &all);
        renderer.set_detail_fade(Some(fade));
        self.detail_preview.uploaded = true;
        if models_changed {
            self.scene.instances_dirty = true;
        }
    }

    pub(crate) fn detail_preview_instances(&mut self, renderer: &mut Renderer) -> Vec<ModelInstance> {
        let list = std::mem::take(&mut self.detail_preview.models);
        let [dist, fade] = self.detail_preview.fade;
        let mut out = Vec::with_capacity(list.len());
        let mut pending = false;
        for m in &list {
            match self.render_model(renderer, &m.path, 0) {
                Some(model) => out.push(ModelInstance {
                    model,
                    transform: m.transform.as_mat4().to_cols_array_2d(),
                    object: MODEL_OBJECT,
                    tint: [1.0; 4],
                    fade: ModelInstance::detail_fade(dist, fade),
                }),
                None => pending |= self.model_loading(&m.path),
            }
        }
        self.detail_preview.models = list;
        self.detail_preview.models_pending = pending;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_bspc::detail_props::parse_detail_vbsp;

    #[test]
    fn sprites_follow_the_lump() {
        let dict = parse_detail_vbsp(
            "detail { g { \"density\" \"100000\" G { \"alpha\" \"1\" A { \"sprite\" \"0 0 64 32 512\" \"sprite_shape\" \"cross\" \"sway\" \"1\" } } } }",
        )
        .unwrap();
        let surface = ScatterSurface::Face {
            points: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 128.0, 0.0),
                Vec3::new(128.0, 128.0, 0.0),
                Vec3::new(128.0, 0.0, 0.0),
            ],
            normal: Vec3::Z,
        };
        let lump = detail_props::scatter(&surface, dict.find("g").unwrap(), 5, 100, &|_| true);
        assert!(!lump.objects.is_empty());
        let sprites = sprites_of(&lump);
        assert_eq!(sprites.len(), lump.objects.len() * 2, "a cross is two quads");
        assert!(sprites.iter().all(|s| s.sway == 255 && s.object == SPRITE_OBJECT));
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        assert!(dot(sprites[0].right, sprites[1].right).abs() < 1e-3, "the quads cross at 90 degrees");
    }

    #[test]
    fn distance_to_a_box() {
        let (a, b) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0));
        assert_eq!(box_distance2(a, b, Vec3::new(5.0, 5.0, 5.0)), 0.0);
        assert_eq!(box_distance2(a, b, Vec3::new(13.0, 5.0, 14.0)), 25.0);
    }
}
