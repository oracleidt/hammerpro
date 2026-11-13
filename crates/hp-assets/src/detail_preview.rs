use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ahash::{AHashMap, AHashSet};
use glam::{DVec3, Vec3};
use hp_app::runtime::render::{DetailPreviewReport, DetailPreviewRequest, DetailSplit, DetailTypeReport};
use hp_bspc::detail_props::{self, DetailDict, DetailLump, GaussState, ScatterSurface, kind};
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
    gauss: GaussState,
}

#[derive(Default)]
struct Generated {
    sprites: Vec<DetailSprite>,
    models: Vec<(String, glam::DMat4)>,
    gauss_out: GaussState,
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
    disabled: usize,
    split: Option<Arc<DetailSplit>>,
    exact: bool,
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
    let Some(t) = dict.find(&s.type_name) else { return Generated { gauss_out: s.gauss, ..Generated::default() } };
    let (lump, gauss_out) =
        detail_props::scatter_chained(&s.surface, t, s.seed, SURFACE_CAP, &|m| !m.trim().is_empty(), s.gauss);
    Generated { sprites: sprites_of(&lump), models: models_of(&lump), gauss_out }
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

#[derive(Clone, Debug)]
pub struct DetailSurface {
    pub solid: u32,
    pub side_index: usize,
    pub side_id: u32,
    pub material: String,
    pub type_name: String,
    pub overridden: bool,
    pub surface: ScatterSurface,
    pub mins: Vec3,
    pub maxs: Vec3,
    pub seed: u32,
    pub key: u64,
    pub order: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CollectedSurfaces {
    pub surfaces: Vec<DetailSurface>,
    pub counts: Vec<(String, String, usize)>,
    pub displacements: usize,
    pub brush_faces: usize,
    pub disabled: usize,
    pub exact: bool,
}

pub fn split_signature(doc: &Document) -> u64 {
    let scene = doc.scene();
    let mut ids: Vec<_> = scene.solids().keys().copied().collect();
    ids.sort_unstable();
    hash_of(|h| {
        for id in ids {
            let Some(n) = scene.solid(id) else { continue };
            id.hash(h);
            doc.is_solid_visible(id).hash(h);
            match n.owner {
                Owner::World => 0u32.hash(h),
                Owner::Entity(e) => {
                    e.hash(h);
                    if let Some(en) = scene.entity(e) {
                        en.entity.classname.to_ascii_lowercase().hash(h);
                        for (k, v) in &en.entity.props {
                            k.hash(h);
                            v.hash(h);
                        }
                    }
                }
            }
            for side in &n.solid.sides {
                side.id.hash(h);
                for p in side.plane {
                    p.to_array().map(f64::to_bits).hash(h);
                }
                side.material.to_ascii_lowercase().hash(h);
                for (k, v) in &side.extra_props {
                    k.hash(h);
                    v.hash(h);
                }
                for a in [side.uaxis, side.vaxis] {
                    a.axis.to_array().map(f64::to_bits).hash(h);
                    a.shift.to_bits().hash(h);
                    a.scale.to_bits().hash(h);
                }
                side.lightmap_scale.hash(h);
                match &side.disp {
                    Some(d) => {
                        d.power.hash(h);
                        d.start_position.to_array().map(f64::to_bits).hash(h);
                    }
                    None => 0u32.hash(h),
                }
            }
        }
    })
}

pub fn split_from_bsp(bsp: &hp_bsp::Bsp, signature: u64) -> DetailSplit {
    let split = detail_props::brush_fragments(bsp);
    DetailSplit {
        signature,
        options: 0,
        external: false,
        fragments: split
            .fragments
            .into_iter()
            .map(|f| hp_app::runtime::render::SplitFragment {
                face: f.face,
                side: f.side,
                material: f.material,
                points: f.points.iter().map(|p| p.to_array()).collect(),
                origin: f.origin.to_array(),
            })
            .collect(),
        disp_faces: split.disp_faces,
    }
}

pub fn compute_split(
    doc: &Document,
    materials: &dyn hp_bspc::MaterialSource,
    opts: &hp_bspc::CompileOptions,
) -> Result<DetailSplit, String> {
    let signature = split_signature(doc);
    let map = doc.compile_snapshot(&hp_editor::compile_export::CompileExport::default()).map;
    split_of_map(&map, signature, materials, opts)
}

pub fn split_of_map(
    map: &hp_vmf::Map,
    signature: u64,
    materials: &dyn hp_bspc::MaterialSource,
    opts: &hp_bspc::CompileOptions,
) -> Result<DetailSplit, String> {
    let out =
        hp_bspc::compile(map, materials, &hp_bspc::props::NoModels, opts, &mut |_| {}).map_err(|e| e.to_string())?;
    let bsp = hp_bsp::Bsp::from_vec(out.bsp).map_err(|e| e.to_string())?;
    Ok(split_from_bsp(&bsp, signature))
}

pub fn generate_ordered(surfaces: &[DetailSurface], dict: &DetailDict, cap: usize) -> Vec<DetailLump> {
    let mut gauss = GaussState::default();
    surfaces
        .iter()
        .map(|s| match dict.find(&s.type_name) {
            Some(t) => {
                let (lump, out) =
                    detail_props::scatter_chained(&s.surface, t, s.seed, cap, &|m| !m.trim().is_empty(), gauss);
                gauss = out;
                lump
            }
            None => DetailLump::default(),
        })
        .collect()
}

pub fn seed_of_side(side_id: u32) -> u32 {
    side_id & 0xffff
}

pub fn generate_lump(surface: &DetailSurface, dict: &DetailDict, cap: usize) -> DetailLump {
    match dict.find(&surface.type_name) {
        Some(t) => detail_props::scatter(&surface.surface, t, surface.seed, cap, &|m| !m.trim().is_empty()),
        None => DetailLump::default(),
    }
}

pub fn material_detail_type(fs: &hp_fs::GameFs, material: &str) -> Option<String> {
    let key = hp_vmt::normalize_material_name(material);
    let m = fs.material(&key)?.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
    m.get("%detailtype").map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

pub fn detail_vbsp_name(world: &hp_vmf::Entity) -> String {
    world.get("detailvbsp").map(str::trim).filter(|s| !s.is_empty()).unwrap_or("detail.vbsp").to_owned()
}

pub fn load_detail_dict(fs: &hp_fs::GameFs, wanted: &str) -> Result<(String, DetailDict), String> {
    let mut names = vec![wanted.to_owned()];
    for fallback in ["detail.vbsp", "scripts/detail.vbsp"] {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(fallback)) {
            names.push(fallback.to_owned());
        }
    }
    let Some((name, bytes)) = names.into_iter().find_map(|n: String| read_game_file(fs, &n).map(|b| (n, b))) else {
        return Err(format!("{wanted} not found in the game content"));
    };
    let dict = detail_props::parse_detail_vbsp(&String::from_utf8_lossy(&bytes))?;
    Ok((name, dict))
}

fn surface_key(side_id: u32, type_name: &str, seed: u32, surface: &ScatterSurface) -> u64 {
    hash_of(|h| {
        side_id.hash(h);
        type_name.hash(h);
        seed.hash(h);
        match surface {
            ScatterSurface::Face { points, .. } => {
                for p in points {
                    p.to_array().map(f32::to_bits).hash(h);
                }
            }
            ScatterSurface::Fragment { points, origin } => {
                origin.to_array().map(f32::to_bits).hash(h);
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
    })
}

pub fn collect_surfaces(
    doc: &Document,
    type_of: &mut dyn FnMut(&str) -> Option<String>,
    known: &dyn Fn(&str) -> bool,
    split: Option<&DetailSplit>,
) -> CollectedSurfaces {
    let mut out = CollectedSurfaces::default();
    let split = split.filter(|s| s.signature == split_signature(doc));
    out.exact = split.is_some_and(|s| !s.external);
    let scene = doc.scene();
    let mut ids: Vec<_> = scene.solids().keys().copied().collect();
    ids.sort_unstable();
    let mut sides_by_key: AHashMap<u16, (u32, usize, u32)> = AHashMap::new();
    let mut overrides: AHashMap<u16, detail_props::DetailOverride> = AHashMap::new();
    for id in &ids {
        let Some(node) = scene.solid(*id) else { continue };
        for (idx, side) in node.solid.sides.iter().enumerate() {
            let k = side.id as u16;
            sides_by_key.entry(k).or_insert((*id, idx, side.id));
            if let Some(o) = detail_props::side_override(side) {
                overrides.insert(k, o);
            }
        }
    }
    let mut disp_order: AHashMap<u16, Vec<usize>> = AHashMap::new();
    if let Some(sp) = split {
        for (face, key) in sp.disp_faces.iter().rev() {
            disp_order.entry(*key).or_default().push(*face);
        }
        for f in &sp.fragments {
            let type_name = match overrides.get(&f.side) {
                Some(detail_props::DetailOverride::Off) => {
                    out.disabled += 1;
                    continue;
                }
                Some(detail_props::DetailOverride::Type(t)) => t.clone(),
                None => match type_of(&f.material) {
                    Some(t) => t,
                    None => continue,
                },
            };
            match out.counts.iter_mut().find(|c| c.0.eq_ignore_ascii_case(&f.material) && c.1 == type_name) {
                Some(c) => c.2 += 1,
                None => out.counts.push((f.material.to_ascii_lowercase(), type_name.clone(), 1)),
            }
            if !known(&type_name) {
                continue;
            }
            let origin = Vec3::from(f.origin);
            let points: Vec<Vec3> = f.points.iter().map(|p| Vec3::from(*p)).collect();
            let mins = points.iter().fold(Vec3::splat(f32::MAX), |m, p| m.min(*p + origin));
            let maxs = points.iter().fold(Vec3::splat(f32::MIN), |m, p| m.max(*p + origin));
            if !(mins.is_finite() && maxs.is_finite()) {
                continue;
            }
            out.brush_faces += 1;
            let (solid, side_index, side_id) = sides_by_key.get(&f.side).copied().unwrap_or((0, 0, u32::from(f.side)));
            let surface = ScatterSurface::Fragment { points, origin };
            let seed = u32::from(f.side);
            let key = surface_key(side_id ^ ((f.face as u32) << 16), &type_name, seed, &surface);
            out.surfaces.push(DetailSurface {
                solid,
                side_index,
                side_id,
                material: f.material.clone(),
                overridden: overrides.contains_key(&f.side),
                type_name,
                surface,
                mins,
                maxs,
                seed,
                key,
                order: f.face,
            });
        }
    }
    let mut next_order = usize::MAX / 2;
    for id in ids {
        let Some(node) = scene.solid(id) else { continue };
        let in_world = match node.owner {
            Owner::World => true,
            Owner::Entity(e) => scene.entity(e).is_some_and(|n| n.entity.classname.eq_ignore_ascii_case("func_detail")),
        };
        if !in_world || !doc.is_solid_visible(id) {
            continue;
        }
        for (idx, side) in node.solid.sides.iter().enumerate() {
            let (type_name, overridden) = match detail_props::side_override(side) {
                Some(detail_props::DetailOverride::Off) => {
                    out.disabled += 1;
                    continue;
                }
                Some(detail_props::DetailOverride::Type(t)) => (t, true),
                None => match type_of(&side.material) {
                    Some(t) => (t, false),
                    None => continue,
                },
            };
            match out.counts.iter_mut().find(|c| c.0.eq_ignore_ascii_case(&side.material) && c.1 == type_name) {
                Some(c) => c.2 += 1,
                None => out.counts.push((side.material.to_ascii_lowercase(), type_name.clone(), 1)),
            }
            if !known(&type_name) {
                continue;
            }
            let is_disp = node.disp.get(idx).and_then(Option::as_ref).is_some();
            if split.is_some() && !is_disp {
                continue;
            }
            let (surface, mins, maxs) = match node.disp.get(idx).and_then(Option::as_ref) {
                Some(mesh) => {
                    let verts: Vec<Vec3> = mesh.positions.iter().map(|p| v3(*p)).collect();
                    let base: Vec<Vec3> = mesh.base_positions.iter().map(|p| v3(*p)).collect();
                    let alphas: Vec<f32> = mesh.alphas.iter().map(|a| *a as f32).collect();
                    out.displacements += 1;
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
                    out.brush_faces += 1;
                    let mins = points.iter().fold(Vec3::splat(f32::MAX), |m, p| m.min(*p));
                    let maxs = points.iter().fold(Vec3::splat(f32::MIN), |m, p| m.max(*p));
                    (ScatterSurface::Face { points, normal: v3(face.normal()) }, mins, maxs)
                }
            };
            if !(mins.is_finite() && maxs.is_finite()) {
                continue;
            }
            let seed = seed_of_side(side.id);
            let key = surface_key(side.id, &type_name, seed, &surface);
            let order = match disp_order.get_mut(&(side.id as u16)).and_then(Vec::pop) {
                Some(face) => face,
                None => {
                    next_order += 1;
                    next_order
                }
            };
            out.surfaces.push(DetailSurface {
                solid: id,
                side_index: idx,
                side_id: side.id,
                material: side.material.clone(),
                type_name,
                overridden,
                surface,
                mins,
                maxs,
                seed,
                key,
                order,
            });
        }
    }
    out.surfaces.sort_by_key(|s| s.order);
    out
}

impl AssetManager {
    fn detail_type_of(&mut self, material: &str) -> Option<String> {
        let key = hp_vmt::normalize_material_name(material);
        if let Some(hit) = self.detail_preview.type_cache.get(&key) {
            return hit.clone();
        }
        let found = self.fs.as_ref().and_then(|fs| material_detail_type(fs, &key));
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
        let Some(fs) = self.fs.as_ref() else {
            dp.dict_error = Some("no game mounted".to_owned());
            return;
        };
        match load_detail_dict(fs, wanted) {
            Ok((name, d)) => {
                dp.dict_file = Some(name);
                dp.dict = Some(Arc::new(d));
            }
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
        let split = self.detail_preview.split.clone();
        let mut lookup = |m: &str| self.detail_type_of(m);
        let found = collect_surfaces(doc, &mut lookup, &|t| dict.find(t).is_some(), split.as_deref());
        let counts = found.counts;
        let (displacements, brush_faces) = (found.displacements, found.brush_faces);
        self.detail_preview.disabled = found.disabled;
        self.detail_preview.exact = found.exact;
        let chained = found.surfaces.iter().any(|d| dict.find(&d.type_name).is_some_and(detail_props::uses_gaussian));
        let mut gauss = GaussState::default();
        let mut surfaces: Vec<Surface> = Vec::with_capacity(found.surfaces.len());
        for d in found.surfaces {
            let key = if chained { hash_of(|h| (d.key, gauss.bits()).hash(h)) } else { d.key };
            let s = Surface {
                key,
                type_name: d.type_name,
                surface: d.surface,
                mins: d.mins,
                maxs: d.maxs,
                seed: d.seed,
                gauss: if chained { gauss } else { GaussState::default() },
            };
            if chained {
                let g = match self.detail_preview.cache.get(&key) {
                    Some(g) => Arc::clone(g),
                    None => {
                        let g = Arc::new(generate(&s, &dict));
                        self.detail_preview.cache.insert(key, Arc::clone(&g));
                        g
                    }
                };
                gauss = g.gauss_out;
            }
            surfaces.push(s);
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
        let same_split = match (&self.detail_preview.split, &req.split) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same_split {
            self.detail_preview.split = req.split.clone();
            self.detail_preview.revision = None;
        }
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
            type_names: dp.dict.as_ref().map(|d| d.types.iter().map(|t| t.name.clone()).collect()).unwrap_or_default(),
            disabled_faces: dp.disabled,
            exact: dp.exact,
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

    const PARITY_VBSP: &str = "detail { grass { \"density\" \"4000\" Low { \"alpha\" \"0\" A { \"sprite\" \"0 0 64 32 512\" \"amount\" \"1\" } } High { \"alpha\" \"1\" B { \"sprite\" \"64 0 64 32 512\" \"amount\" \"1\" \"sprite_shape\" \"cross\" } } } }";

    struct ParityMats;

    impl hp_bspc::materials::MaterialSource for ParityMats {
        fn facts(&self, name: &str) -> hp_bspc::materials::MaterialFacts {
            let mut f = hp_bspc::materials::MaterialFacts::default();
            if name.to_ascii_lowercase().contains("blendgrass") {
                f.detail_type = Some("grass".to_owned());
            }
            f
        }

        fn read_file(&self, path: &str) -> Option<Vec<u8>> {
            path.eq_ignore_ascii_case("detail.vbsp").then(|| PARITY_VBSP.as_bytes().to_vec())
        }
    }

    fn parity_doc(material: &str, extra: Option<&str>) -> hp_editor::Document {
        let mut state = hp_app::testing::test_state();
        let face = hp_editor::FaceRef::new(2, 0);
        state.doc.create_displacements(&[face], 3).expect("displacement");
        let mut info = *state.doc.scene().solid(2).and_then(|n| n.solid.sides[0].disp.clone()).expect("disp");
        let row = info.row_len();
        for k in 0..info.alphas.len() {
            let (i, j) = (k / row, k % row);
            info.alphas[k] = if j * 2 >= row { 255.0 } else { 40.0 * (i % 3) as f64 };
            info.distances[k] = ((i * 7 + j * 3) % 5) as f64 * 6.0;
        }
        state.doc.set_disp_data("test", vec![(face, info)]).expect("data");
        let extra = extra.map(str::to_owned);
        let material = material.to_owned();
        state
            .doc
            .edit_solid(2, |s| {
                s.sides[0].material = material;
                if let Some(v) = extra {
                    s.sides[0].extra_props.push((detail_props::OVERRIDE_KEY.to_owned(), v));
                }
            })
            .expect("material");
        state.doc
    }

    fn compile_doc(doc: &hp_editor::Document) -> hp_bsp::Bsp {
        let map = doc.compile_snapshot(&hp_editor::compile_export::CompileExport::default()).map;
        let mut log = Vec::new();
        let out = hp_bspc::compile(
            &map,
            &ParityMats,
            &hp_bspc::props::NoModels,
            &hp_bspc::CompileOptions::default(),
            &mut |l| log.push(l.to_owned()),
        )
        .unwrap_or_else(|e| panic!("compile: {e}\n{}", log.join("\n")));
        hp_bsp::Bsp::from_vec(out.bsp).expect("bsp")
    }

    type ObjKey = (u8, [i64; 3], [i64; 3], i64);

    fn obj_key(o: &detail_props::DetailObject) -> ObjKey {
        let r = |v: f32| (f64::from(v) * 64.0).round() as i64;
        (
            o.kind,
            [r(o.origin.x), r(o.origin.y), r(o.origin.z)],
            [r(o.angles.x), r(o.angles.y), r(o.angles.z)],
            r(o.scale),
        )
    }

    fn compiled_objects(doc: &hp_editor::Document) -> Vec<ObjKey> {
        let bsp = compile_doc(doc);
        let lumps = bsp.game_lumps().expect("game lumps");
        let data = lumps.get(detail_props::DPRP).map(|l| l.data().expect("data").into_owned()).unwrap_or_default();
        let lump = if data.is_empty() { DetailLump::default() } else { DetailLump::parse(&data).expect("dprp") };
        let mut v: Vec<ObjKey> = lump.objects.iter().map(obj_key).collect();
        v.sort_unstable();
        v
    }

    fn lookup(m: &str) -> Option<String> {
        m.to_ascii_lowercase().contains("blendgrass").then(|| "grass".to_owned())
    }

    fn preview_objects(doc: &hp_editor::Document, vbsp: &str, split: bool) -> (Vec<ObjKey>, CollectedSurfaces) {
        let dict = detail_props::parse_detail_vbsp(vbsp).expect("vbsp");
        let split = split.then(|| compute_split(doc, &ParityMats, &Default::default()).expect("split"));
        let found = collect_surfaces(doc, &mut lookup, &|t| dict.find(t).is_some(), split.as_ref());
        let mut v: Vec<ObjKey> = generate_ordered(&found.surfaces, &dict, 65_535)
            .iter()
            .flat_map(|l| l.objects.iter().map(obj_key).collect::<Vec<_>>())
            .collect();
        v.sort_unstable();
        (v, found)
    }

    fn preview_origins(doc: &hp_editor::Document) -> Vec<ObjKey> {
        preview_objects(doc, PARITY_VBSP, false).0
    }

    fn compiled_origins(doc: &hp_editor::Document) -> Vec<ObjKey> {
        compiled_objects(doc)
    }

    #[test]
    fn the_preview_places_what_the_compiler_emits() {
        let doc = parity_doc("nature/blendgrass01", None);
        let compiled = compiled_origins(&doc);
        let preview = preview_origins(&doc);
        assert!(compiled.len() > 100, "{}", compiled.len());
        assert_eq!(preview.len(), compiled.len());
        assert_eq!(preview, compiled);

        let off = parity_doc("nature/blendgrass01", Some("none"));
        assert!(compiled_origins(&off).is_empty());
        assert!(preview_origins(&off).is_empty());

        let forced = parity_doc("dev/dev_measuregeneric01b", Some("grass"));
        let c = compiled_origins(&forced);
        assert_eq!(c, compiled, "an override places the same props as the material type");
        assert_eq!(preview_origins(&forced), c);
    }

    fn block(id: &mut u32, lo: [f64; 3], hi: [f64; 3], top: &str, other: &str) -> String {
        let [x0, y0, z0] = lo;
        let [x1, y1, z1] = hi;
        let planes = [
            ([x0, y1, z1], [x1, y1, z1], [x1, y0, z1], top),
            ([x0, y0, z0], [x1, y0, z0], [x1, y1, z0], other),
            ([x0, y1, z1], [x0, y0, z1], [x0, y0, z0], other),
            ([x1, y1, z0], [x1, y0, z0], [x1, y0, z1], other),
            ([x1, y1, z1], [x0, y1, z1], [x0, y1, z0], other),
            ([x1, y0, z0], [x0, y0, z0], [x0, y0, z1], other),
        ];
        *id += 1;
        let mut out = format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n");
        for (a, b, c, m) in planes {
            *id += 1;
            let v = |p: [f64; 3]| format!("({} {} {})", p[0], p[1], p[2]);
            out.push_str(&format!(
                "\t\tside {{ \"id\" \"{id}\" \"plane\" \"{} {} {}\" \"material\" \"{m}\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" \"rotation\" \"0\" \"lightmapscale\" \"16\" \"smoothing_groups\" \"0\" }}\n",
                v(a),
                v(b),
                v(c)
            ));
        }
        out.push_str("\t}\n");
        out
    }

    fn split_map() -> (hp_editor::Document, u32) {
        let wall = "dev/dev_measuregeneric01b";
        let mut id = 1u32;
        let mut solids = String::new();
        solids.push_str(&block(&mut id, [-512.0, -512.0, -16.0], [512.0, 512.0, 0.0], "nature/blendgrass01", wall));
        solids.push_str(&block(&mut id, [-64.0, -700.0, 0.0], [64.0, 700.0, 96.0], wall, wall));
        solids.push_str(&block(&mut id, [200.0, 200.0, 0.0], [260.0, 260.0, 40.0], wall, wall));
        solids.push_str(&block(&mut id, [-300.0, -300.0, -8.0], [-200.0, 100.0, 24.0], "nature/blendgrass01", wall));
        let disp_solid = id + 1;
        solids.push_str(&block(&mut id, [600.0, -128.0, -16.0], [856.0, 128.0, 0.0], "nature/blendgrass01", wall));
        let text = format!(
            "versioninfo\n{{\n\t\"mapversion\" \"1\"\n}}\nworld\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n{solids}}}\nentity\n{{\n\t\"id\" \"{}\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"-400 -400 32\"\n}}\n",
            id + 1
        );
        let mut doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("vmf"));
        doc.create_displacements(&[hp_editor::FaceRef::new(disp_solid, 0)], 3).expect("disp");
        let mut info = *doc.scene().solid(disp_solid).and_then(|n| n.solid.sides[0].disp.clone()).expect("disp");
        let row = info.row_len();
        for k in 0..info.alphas.len() {
            info.alphas[k] = if (k % row) * 2 >= row { 255.0 } else { 0.0 };
            info.distances[k] = ((k * 7) % 5) as f64 * 4.0;
        }
        doc.set_disp_data("test", vec![(hp_editor::FaceRef::new(disp_solid, 0), info)]).expect("data");
        (doc, disp_solid)
    }

    const SCALED_VBSP: &str = "detail { grass { \"density\" \"2500\" Low { \"alpha\" \"0\" A { \"sprite\" \"0 0 64 32 512\" \"spriterandomscale\" \"0.3\" \"amount\" \"0.6\" } B { \"sprite\" \"64 0 64 32 512\" \"sprite_shape\" \"cross\" \"amount\" \"1\" } } High { \"alpha\" \"1\" C { \"sprite\" \"128 0 64 32 512\" \"spriterandomscale\" \"0.5\" \"amount\" \"1\" } } } }";

    struct ScaledMats;

    impl hp_bspc::materials::MaterialSource for ScaledMats {
        fn facts(&self, name: &str) -> hp_bspc::materials::MaterialFacts {
            ParityMats.facts(name)
        }

        fn read_file(&self, path: &str) -> Option<Vec<u8>> {
            path.eq_ignore_ascii_case("detail.vbsp").then(|| SCALED_VBSP.as_bytes().to_vec())
        }
    }

    #[test]
    fn split_brush_faces_match_the_compiler_prop_for_prop() {
        let (doc, _) = split_map();
        for vbsp in [PARITY_VBSP, SCALED_VBSP] {
            let compiled = {
                let map = doc.compile_snapshot(&hp_editor::compile_export::CompileExport::default()).map;
                let mats: &dyn hp_bspc::MaterialSource = if vbsp == PARITY_VBSP { &ParityMats } else { &ScaledMats };
                let out = hp_bspc::compile(&map, mats, &hp_bspc::props::NoModels, &Default::default(), &mut |_| {})
                    .expect("compile");
                let bsp = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp");
                let lumps = bsp.game_lumps().expect("game lumps");
                let data =
                    lumps.get(detail_props::DPRP).map(|l| l.data().expect("data").into_owned()).unwrap_or_default();
                let lump = DetailLump::parse(&data).expect("dprp");
                let mut v: Vec<ObjKey> = lump.objects.iter().map(obj_key).collect();
                v.sort_unstable();
                v
            };
            let (exact, found) = preview_objects(&doc, vbsp, true);
            assert!(found.exact);
            let floor_pieces = found.surfaces.iter().filter(|s| s.solid == 2 && s.side_index == 0).count();
            assert!(floor_pieces > 2, "the floor top is split by the brushes on it: {floor_pieces}");
            assert!(compiled.len() > 500, "{}", compiled.len());
            assert_eq!(exact.len(), compiled.len());
            assert_eq!(exact, compiled, "the split preview equals the compile prop for prop");
            let (whole, approx) = preview_objects(&doc, vbsp, false);
            assert!(!approx.exact);
            assert_ne!(whole, compiled, "without the split the whole faces differ, which is why the split is needed");
        }
    }

    #[test]
    fn the_split_follows_the_compile_options() {
        let (mut doc, _) = split_map();
        doc.edit_solid(2, |s| {
            for side in &mut s.sides {
                side.lightmap_scale = 10;
            }
        })
        .expect("lightmap scale");
        let opts = hp_bspc::CompileOptions { no_subdiv: true, ..hp_bspc::CompileOptions::default() };
        let map = doc.compile_snapshot(&hp_editor::compile_export::CompileExport::default()).map;
        let out = hp_bspc::compile(&map, &ParityMats, &hp_bspc::props::NoModels, &opts, &mut |_| {}).expect("compile");
        let bsp = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp");
        let lumps = bsp.game_lumps().expect("game lumps");
        let data = lumps.get(detail_props::DPRP).map(|l| l.data().expect("data").into_owned()).unwrap_or_default();
        let mut compiled: Vec<ObjKey> = DetailLump::parse(&data).expect("dprp").objects.iter().map(obj_key).collect();
        compiled.sort_unstable();
        let dict = detail_props::parse_detail_vbsp(PARITY_VBSP).expect("vbsp");
        let preview = |split: &DetailSplit| {
            let found = collect_surfaces(&doc, &mut lookup, &|t| dict.find(t).is_some(), Some(split));
            let mut v: Vec<ObjKey> = generate_ordered(&found.surfaces, &dict, 65_535)
                .iter()
                .flat_map(|l| l.objects.iter().map(obj_key).collect::<Vec<_>>())
                .collect();
            v.sort_unstable();
            (v, found.surfaces.len())
        };
        let with_flag = compute_split(&doc, &ParityMats, &opts).expect("split");
        let default = compute_split(&doc, &ParityMats, &Default::default()).expect("split");
        let (exact, pieces_flag) = preview(&with_flag);
        let (other, pieces_default) = preview(&default);
        assert!(pieces_default > pieces_flag, "-nosubdiv cuts the big faces less: {pieces_default} vs {pieces_flag}");
        assert_eq!(exact, compiled, "split with -nosubdiv equals a -nosubdiv compile prop for prop");
        assert_ne!(other, compiled, "a split with the default options would not match");
        let mut external = with_flag.clone();
        external.external = true;
        assert!(!collect_surfaces(&doc, &mut lookup, &|t| dict.find(t).is_some(), Some(&external)).exact);
    }

    #[test]
    fn a_stale_split_is_not_used() {
        let (mut doc, _) = split_map();
        let split = compute_split(&doc, &ParityMats, &Default::default()).expect("split");
        let dict = detail_props::parse_detail_vbsp(PARITY_VBSP).expect("vbsp");
        assert!(collect_surfaces(&doc, &mut lookup, &|t| dict.find(t).is_some(), Some(&split)).exact);
        doc.edit_solid(9, |s| s.sides[0].material = "nature/blendgrass01".to_owned()).expect("edit");
        let found = collect_surfaces(&doc, &mut lookup, &|t| dict.find(t).is_some(), Some(&split));
        assert!(!found.exact, "an edited brush makes the split stale");
    }

    #[test]
    fn distance_to_a_box() {
        let (a, b) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0));
        assert_eq!(box_distance2(a, b, Vec3::new(5.0, 5.0, 5.0)), 0.0);
        assert_eq!(box_distance2(a, b, Vec3::new(13.0, 5.0, 14.0)), 25.0);
    }
}
