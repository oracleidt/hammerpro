use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use glam::{DVec2, DVec3};
use hp_editor::prefab::SprinkleItem;
use hp_editor::{EntityRay, Id, ObjRef, PickOptions};
use hp_geom::{Aabb, Ray};
use serde::{Deserialize, Serialize};

use crate::scatter::{Rng, SCALE_KEY};
use crate::state::AppState;

pub const LAYER_PREFIX: &str = "Foliage: ";
pub const DETAIL_OVERRIDE_KEY: &str = "hp_detailtype";
pub const DETAIL_OVERRIDE_NONE: &str = "none";
pub const MAX_ITEMS: usize = 64;
pub const MAX_PALETTES: usize = 64;
pub const MAX_POINTS: usize = 4096;
pub const MAX_PER_DAB: usize = 2000;
pub const DENSITY_AREA: f64 = 256.0 * 256.0;
pub const RADIUS_RANGE: std::ops::RangeInclusive<f64> = 4.0..=8192.0;
pub const DENSITY_RANGE: std::ops::RangeInclusive<f64> = 0.01..=500.0;
pub const DEFAULT_PALETTE: &str = "Foliage";
pub const SURFACE_CLASSES: [&str; 4] = ["func_detail", "func_brush", "func_wall", "func_lod"];
const FALLBACK_RADIUS: f64 = 8.0;
const HULL_SAMPLES: usize = 8;
const CONTACT_SHARE: f64 = 0.03;
const CONTACT_MIN: f64 = 0.5;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FoliageItem {
    pub model: String,
    pub classname: String,
    pub weight: f64,
    pub enabled: bool,
}

impl Default for FoliageItem {
    fn default() -> Self {
        Self { model: String::new(), classname: "prop_static".to_owned(), weight: 1.0, enabled: true }
    }
}

impl FoliageItem {
    pub fn new(model: &str) -> Self {
        Self { model: normalize_model(model), ..Self::default() }
    }

    pub fn usable(&self) -> bool {
        self.enabled && !self.model.trim().is_empty() && self.weight.is_finite() && self.weight > 0.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FoliagePalette {
    pub name: String,
    pub items: Vec<FoliageItem>,
}

impl Default for FoliagePalette {
    fn default() -> Self {
        Self { name: DEFAULT_PALETTE.to_owned(), items: Vec::new() }
    }
}

impl FoliagePalette {
    pub fn layer_name(&self) -> String {
        layer_name(&self.name)
    }

    pub fn add_model(&mut self, model: &str) -> bool {
        let m = normalize_model(model);
        if m.is_empty() || self.items.len() >= MAX_ITEMS || self.items.iter().any(|i| i.model.eq_ignore_ascii_case(&m))
        {
            return false;
        }
        self.items.push(FoliageItem::new(&m));
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FoliageMode {
    #[default]
    Paint,
    Erase,
    DetailAlpha,
    DetailFace,
}

impl FoliageMode {
    pub const ALL: [FoliageMode; 4] =
        [FoliageMode::Paint, FoliageMode::Erase, FoliageMode::DetailAlpha, FoliageMode::DetailFace];

    pub fn name(self) -> &'static str {
        match self {
            FoliageMode::Paint => "paint",
            FoliageMode::Erase => "erase",
            FoliageMode::DetailAlpha => "detail_alpha",
            FoliageMode::DetailFace => "detail_face",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FoliageSettings {
    pub palettes: Vec<FoliagePalette>,
    pub active: usize,
    pub mode: FoliageMode,
    pub radius: f64,
    pub density: f64,
    pub spacing: f64,
    pub scale_min: f64,
    pub scale_max: f64,
    pub random_yaw: bool,
    pub align: f64,
    pub slope_min: f64,
    pub slope_max: f64,
    pub height_limit: bool,
    pub height_min: f64,
    pub height_max: f64,
    pub avoid_overlap: bool,
    pub drop_to_surface: bool,
    pub random_skin: bool,
    pub on_props: bool,
    pub sink: f64,
    pub detail_strength: f64,
    pub detail_type: String,
}

impl Default for FoliageSettings {
    fn default() -> Self {
        Self {
            palettes: vec![FoliagePalette::default()],
            active: 0,
            mode: FoliageMode::Paint,
            radius: 256.0,
            density: 4.0,
            spacing: 0.0,
            scale_min: 1.0,
            scale_max: 1.0,
            random_yaw: true,
            align: 0.0,
            slope_min: 0.0,
            slope_max: 45.0,
            height_limit: false,
            height_min: -16384.0,
            height_max: 16384.0,
            avoid_overlap: true,
            drop_to_surface: true,
            random_skin: false,
            on_props: true,
            sink: 0.0,
            detail_strength: 64.0,
            detail_type: String::new(),
        }
    }
}

impl FoliageSettings {
    pub fn palette(&self) -> Option<&FoliagePalette> {
        self.palettes.get(self.active).or(self.palettes.first())
    }

    pub fn palette_mut(&mut self) -> &mut FoliagePalette {
        if self.palettes.is_empty() {
            self.palettes.push(FoliagePalette::default());
        }
        if self.active >= self.palettes.len() {
            self.active = 0;
        }
        &mut self.palettes[self.active]
    }

    pub fn radius(&self) -> f64 {
        clamp_or(self.radius, RADIUS_RANGE, 256.0)
    }

    pub fn density(&self) -> f64 {
        clamp_or(self.density, DENSITY_RANGE, 4.0)
    }

    pub fn scale_range(&self) -> (f64, f64) {
        let lo = clamp_or(self.scale_min, 0.01..=100.0, 1.0);
        let hi = clamp_or(self.scale_max, 0.01..=100.0, lo).max(lo);
        (lo, hi)
    }

    pub fn slope_range(&self) -> (f64, f64) {
        let lo = clamp_or(self.slope_min, 0.0..=180.0, 0.0);
        let hi = clamp_or(self.slope_max, 0.0..=180.0, 90.0).max(lo);
        (lo, hi)
    }

    pub fn validate(&self) -> Result<(), String> {
        let nums = [
            self.radius,
            self.density,
            self.spacing,
            self.scale_min,
            self.scale_max,
            self.align,
            self.slope_min,
            self.slope_max,
            self.height_min,
            self.height_max,
            self.sink,
            self.detail_strength,
        ];
        if nums.iter().any(|v| !v.is_finite()) {
            return Err("every foliage option must be a finite number".to_owned());
        }
        if self.spacing < 0.0 {
            return Err("spacing cannot be negative".to_owned());
        }
        if self.height_limit && self.height_max < self.height_min {
            return Err("height_max is below height_min".to_owned());
        }
        Ok(())
    }
}

fn clamp_or(v: f64, range: std::ops::RangeInclusive<f64>, fallback: f64) -> f64 {
    if v.is_finite() { v.clamp(*range.start(), *range.end()) } else { fallback }
}

pub fn normalize_model(model: &str) -> String {
    let m = model.trim().replace('\\', "/");
    let m = m.trim_start_matches('/');
    m.to_owned()
}

pub fn layer_name(palette: &str) -> String {
    format!("{LAYER_PREFIX}{}", palette.trim())
}

pub fn is_tool_material(m: &str) -> bool {
    m.trim().get(..6).is_some_and(|p| p.eq_ignore_ascii_case("tools/"))
}

pub fn is_paint_surface(scene: &hp_editor::Scene, obj: ObjRef) -> bool {
    let ObjRef::Solid(id) = obj else { return false };
    let Some(n) = scene.solid(id) else { return false };
    let owner_ok = match n.owner {
        hp_editor::Owner::World => true,
        hp_editor::Owner::Entity(e) => scene
            .entity(e)
            .is_some_and(|e| SURFACE_CLASSES.iter().any(|c| e.entity.classname.trim().eq_ignore_ascii_case(c))),
    };
    owner_ok && n.solid.sides.iter().any(|s| !is_tool_material(&s.material))
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub hull: Vec<DVec2>,
    pub contact: Vec<DVec2>,
    pub radius: f64,
    pub lift: f64,
    pub height: f64,
    pub skins: u32,
    pub mesh: Option<Arc<hp_shape::TriMesh>>,
}

impl ModelInfo {
    pub fn from_points(points: &[DVec3], skins: u32) -> Option<ModelInfo> {
        let pts: Vec<DVec3> = points.iter().copied().filter(|p| p.is_finite()).collect();
        if pts.is_empty() {
            return None;
        }
        let flat: Vec<DVec2> = pts.iter().map(|p| p.truncate()).collect();
        let hull = hp_shape::hull_2d(&flat);
        let radius = flat.iter().map(|p| p.length()).fold(0.0, f64::max);
        let lo = pts.iter().map(|p| p.z).fold(f64::INFINITY, f64::min);
        let hi = pts.iter().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max);
        let eps = ((hi - lo) * CONTACT_SHARE).max(CONTACT_MIN);
        let low: Vec<DVec2> = pts.iter().filter(|p| p.z <= lo + eps).map(|p| p.truncate()).collect();
        let contact = if low.len() >= 3 { hp_shape::hull_2d(&low) } else { low };
        Some(ModelInfo {
            hull,
            contact,
            radius: radius.max(0.5),
            lift: -lo,
            height: (hi - lo).max(0.0),
            skins: skins.max(1),
            mesh: None,
        })
    }

    pub fn from_mesh(mesh: hp_shape::TriMesh, skins: u32) -> Option<ModelInfo> {
        let mut info = ModelInfo::from_points(&mesh.used_vertices(), skins)?;
        info.mesh = Some(Arc::new(mesh));
        Some(info)
    }

    pub fn contact_samples(&self) -> Vec<DVec2> {
        let src = if self.contact.is_empty() { &self.hull } else { &self.contact };
        let step = (src.len() / HULL_SAMPLES).max(1);
        let mut out: Vec<DVec2> = src.iter().step_by(step).copied().collect();
        if !src.is_empty() {
            out.push(centroid(src));
        }
        out
    }

    pub fn fallback() -> ModelInfo {
        let r = FALLBACK_RADIUS;
        ModelInfo {
            hull: vec![DVec2::new(-r, -r), DVec2::new(r, -r), DVec2::new(r, r), DVec2::new(-r, r)],
            contact: vec![DVec2::new(-r, -r), DVec2::new(r, -r), DVec2::new(r, r), DVec2::new(-r, r)],
            radius: r * std::f64::consts::SQRT_2,
            lift: 0.0,
            height: 2.0 * r,
            skins: 1,
            mesh: None,
        }
    }
}

type InfoCache = Option<(u64, HashMap<String, Option<Arc<ModelInfo>>>)>;

static INFOS: Mutex<InfoCache> = Mutex::new(None);

pub fn model_info(fs: Option<&hp_fs::GameFs>, model: &str) -> Option<Arc<ModelInfo>> {
    let fs = fs?;
    let key = normalize_model(model).to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    let generation = fs.generation();
    {
        let mut guard = INFOS.lock().unwrap_or_else(PoisonError::into_inner);
        let cache = guard.get_or_insert_with(|| (generation, HashMap::new()));
        if cache.0 != generation {
            *cache = (generation, HashMap::new());
        }
        if let Some(hit) = cache.1.get(&key) {
            return hit.clone();
        }
    }
    let skins = fs
        .read(&key)
        .and_then(|b| hp_mdl::read_dependencies(&b).ok())
        .map_or(1, |d| u32::try_from(d.skin_families.len()).unwrap_or(1));
    let info = hp_shape::load_shape(&key, |p| fs.read(p), 0)
        .ok()
        .and_then(|shape| ModelInfo::from_mesh(shape.render, skins))
        .map(Arc::new);
    let mut guard = INFOS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(cache) = guard.as_mut().filter(|c| c.0 == generation) {
        cache.1.insert(key, info.clone());
    }
    info
}

fn rotate(p: DVec2, yaw_deg: f64) -> DVec2 {
    let (s, c) = yaw_deg.to_radians().sin_cos();
    DVec2::new(p.x * c - p.y * s, p.x * s + p.y * c)
}

fn z_range(info: &ModelInfo, origin: DVec3, angles: DVec3, scale: f64) -> (f64, f64) {
    match &info.mesh {
        Some(mesh) => {
            let q = hp_geom::angles_to_quat(angles);
            mesh.used_vertices()
                .iter()
                .map(|v| (q * (*v * scale) + origin).z)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), z| (a.min(z), b.max(z)))
        }
        None => {
            let lo = origin.z - info.lift * scale;
            (lo, lo + info.height * scale)
        }
    }
}

fn footprint_of(info: &ModelInfo, origin: DVec3, angles: DVec3, scale: f64) -> Vec<DVec2> {
    let flat = angles.x.abs() < 1e-9 && angles.z.abs() < 1e-9;
    match &info.mesh {
        Some(mesh) if !flat => {
            let q = hp_geom::angles_to_quat(angles);
            let pts: Vec<DVec2> = mesh.used_vertices().iter().map(|v| (q * (*v * scale) + origin).truncate()).collect();
            hp_shape::hull_2d(&pts)
        }
        _ => placed_poly(info, origin, angles.y, scale),
    }
}

fn placed_poly(info: &ModelInfo, origin: DVec3, yaw: f64, scale: f64) -> Vec<DVec2> {
    info.hull.iter().map(|p| rotate(*p * scale, yaw) + origin.truncate()).collect()
}

fn axes_separate(a: &[DVec2], b: &[DVec2]) -> bool {
    let n = a.len();
    for i in 0..n {
        let e = a[(i + 1) % n] - a[i];
        let axis = DVec2::new(-e.y, e.x);
        if axis.length_squared() < 1e-12 {
            continue;
        }
        let (mut a0, mut a1) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in a {
            let d = p.dot(axis);
            a0 = a0.min(d);
            a1 = a1.max(d);
        }
        let (mut b0, mut b1) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in b {
            let d = p.dot(axis);
            b0 = b0.min(d);
            b1 = b1.max(d);
        }
        if a1 <= b0 + 1e-9 || b1 <= a0 + 1e-9 {
            return true;
        }
    }
    false
}

pub fn polygons_overlap(a: &[DVec2], b: &[DVec2]) -> bool {
    match (a.len(), b.len()) {
        (0, _) | (_, 0) => false,
        (1, _) | (_, 1) | (2, _) | (_, 2) => {
            let (ca, cb) = (centroid(a), centroid(b));
            ca.distance(cb) < 1e-6
        }
        _ => !axes_separate(a, b) && !axes_separate(b, a),
    }
}

fn centroid(p: &[DVec2]) -> DVec2 {
    p.iter().copied().sum::<DVec2>() / p.len().max(1) as f64
}

#[derive(Clone, Debug)]
struct Occupant {
    zlo: f64,
    zhi: f64,
    id: Option<Id>,
    pos: DVec3,
    radius: f64,
    poly: Vec<DVec2>,
    layer: bool,
    alive: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Occupancy {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
    items: Vec<Occupant>,
    max_radius: f64,
}

impl Occupancy {
    fn new(cell: f64) -> Self {
        Self { cell: if cell.is_finite() && cell >= 8.0 { cell } else { 64.0 }, ..Self::default() }
    }

    fn key(&self, x: f64, y: f64) -> (i64, i64) {
        let k = |v: f64| {
            let c = (v / self.cell).floor();
            if c.is_finite() { c.clamp(-1e12, 1e12) as i64 } else { 0 }
        };
        (k(x), k(y))
    }

    fn insert(&mut self, o: Occupant) -> usize {
        let i = self.items.len();
        let k = self.key(o.pos.x, o.pos.y);
        if o.radius.is_finite() {
            self.max_radius = self.max_radius.max(o.radius);
        }
        self.cells.entry(k).or_default().push(i);
        self.items.push(o);
        i
    }

    fn near(&self, p: DVec3, reach: f64) -> Vec<usize> {
        let r = reach.max(0.0);
        let (x0, y0) = self.key(p.x - r, p.y - r);
        let (x1, y1) = self.key(p.x + r, p.y + r);
        let mut out = Vec::new();
        if (x1 - x0 + 1).saturating_mul(y1 - y0 + 1) > 1_000_000 {
            return (0..self.items.len()).filter(|i| self.items[*i].alive).collect();
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(list) = self.cells.get(&(x, y)) {
                    out.extend(list.iter().copied().filter(|i| self.items[*i].alive));
                }
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.items.iter().filter(|o| o.alive).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn kill(&mut self, id: Id) {
        for o in &mut self.items {
            if o.id == Some(id) {
                o.alive = false;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    pub point: DVec3,
    pub normal: DVec3,
    pub prop: Option<Id>,
}

pub trait FoliageWorld {
    fn surface(&self, ray: &Ray, max: f64, props: &[Id]) -> Option<SurfaceHit>;
    fn model(&self, model: &str) -> Option<Arc<ModelInfo>>;
    fn obstacles(&self, _bounds: Aabb) -> Vec<(Option<Id>, hp_shape::TriMesh)> {
        Vec::new()
    }
    fn prop_ray(&self, _ray: &Ray, _max: f64, _id: Id) -> Option<DVec3> {
        None
    }
}

const ABOVE: f64 = 16384.0;
const PENETRATION: f64 = 0.05;

fn open_above(world: &dyn FoliageWorld, hit: &SurfaceHit) -> bool {
    let Some(id) = hit.prop else { return true };
    let ray = Ray::new(hit.point + DVec3::Z * ABOVE, DVec3::NEG_Z);
    world.prop_ray(&ray, ABOVE + 1.0, id).is_some_and(|p| p.distance(hit.point) < 0.5)
}

pub struct StateWorld<'a> {
    pub state: &'a AppState,
}

impl FoliageWorld for StateWorld<'_> {
    fn surface(&self, ray: &Ray, max: f64, props: &[Id]) -> Option<SurfaceHit> {
        let state = self.state;
        let scene = state.doc.scene();
        let filter = |o: ObjRef| is_paint_surface(scene, o) && state.doc.is_visible(o);
        let face_hidden = |m: &str| is_tool_material(m);
        let opts = PickOptions {
            include_entities: false,
            max_distance: max,
            filter: Some(&filter),
            face_hidden: Some(&face_hidden),
            ..PickOptions::default()
        };
        let mut best = state
            .doc
            .pick_ray(ray, &opts, &|e| state.assets.entity_bounds(e))
            .filter(|h| h.point.is_finite())
            .map(|h| {
                (h.t, SurfaceHit { point: h.point, normal: h.normal.try_normalize().unwrap_or(DVec3::Z), prop: None })
            });
        for id in props {
            let Some(n) = scene.entity(*id) else { continue };
            let hit = match state.assets.entity_ray(&n.entity, ray) {
                EntityRay::Hit { t, normal } => Some((t, normal)),
                EntityRay::Miss => None,
                EntityRay::Bounds => state.assets.entity_bounds(&n.entity).and_then(|b| {
                    let t = b.ray_intersect(ray)?;
                    Some((t, box_normal(&b, ray.at(t))))
                }),
            };
            let Some((t, normal)) = hit.filter(|(t, n)| t.is_finite() && *t >= 0.0 && *t <= max && n.is_finite())
            else {
                continue;
            };
            if best.as_ref().is_none_or(|b| t < b.0) {
                let normal = normal.try_normalize().unwrap_or(DVec3::Z);
                best = Some((t, SurfaceHit { point: ray.at(t), normal, prop: Some(*id) }));
            }
        }
        best.map(|b| b.1)
    }

    fn model(&self, model: &str) -> Option<Arc<ModelInfo>> {
        model_info(self.state.assets.fs(), model)
    }

    fn prop_ray(&self, ray: &Ray, max: f64, id: Id) -> Option<DVec3> {
        let state = self.state;
        let n = state.doc.scene().entity(id)?;
        let t = match state.assets.entity_ray(&n.entity, ray) {
            EntityRay::Hit { t, .. } => Some(t),
            EntityRay::Miss => None,
            EntityRay::Bounds => state.assets.entity_bounds(&n.entity).and_then(|b| b.ray_intersect(ray)),
        }?;
        (t.is_finite() && t >= 0.0 && t <= max).then(|| ray.at(t))
    }

    fn obstacles(&self, bounds: Aabb) -> Vec<(Option<Id>, hp_shape::TriMesh)> {
        let state = self.state;
        let scene = state.doc.scene();
        let mut ground = hp_shape::TriMesh::new();
        for n in scene.solids().values() {
            let obj = ObjRef::Solid(n.solid.id);
            if !n.bounds.intersects(&bounds) || !is_paint_surface(scene, obj) || !state.doc.is_visible(obj) {
                continue;
            }
            for (idx, side) in n.solid.sides.iter().enumerate() {
                if let Some(mesh) = n.disp.get(idx).and_then(Option::as_ref) {
                    for tri in mesh.indices.chunks_exact(3) {
                        let p = |i: u32| mesh.positions.get(i as usize).copied();
                        if let (Some(a), Some(b), Some(c)) = (p(tri[0]), p(tri[1]), p(tri[2])) {
                            let ia = ground.add_vertex(a);
                            let ib = ground.add_vertex(b);
                            let ic = ground.add_vertex(c);
                            ground.add_tri([ia, ib, ic], 0);
                        }
                    }
                } else if !is_tool_material(&side.material)
                    && let Some(face) = n.geom.face_for_side(idx)
                {
                    ground.add_polygon(&face.vertices, 0);
                }
            }
        }
        let mut out = vec![(None, ground)];
        for n in scene.entities().values() {
            let e = &n.entity;
            if scene.is_brush_entity(e.id) || !is_prop_entity(e) || !state.doc.is_visible(ObjRef::Entity(e.id)) {
                continue;
            }
            let Some(origin) = e.origin().filter(|o| o.is_finite()) else { continue };
            let b = state.assets.entity_bounds(e);
            let near = match b {
                Some(b) => b.intersects(&bounds),
                None => bounds.expand(256.0).intersects(&Aabb::new(origin, origin)),
            };
            if !near {
                continue;
            }
            let model = e.get("model").map(normalize_model).unwrap_or_default();
            match model_info(state.assets.fs(), &model).and_then(|i| i.mesh.clone()) {
                Some(mesh) => {
                    let ang = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
                    let m = glam::DMat4::from_scale_rotation_translation(
                        DVec3::splat(entity_scale(e)),
                        hp_geom::angles_to_quat(ang),
                        origin,
                    );
                    out.push((Some(e.id), mesh.transformed(&m)));
                }
                None => {
                    if let Some(b) = b {
                        out.push((Some(e.id), hp_shape::TriMesh::convex_box(b.min, b.max, 0)));
                    }
                }
            }
        }
        out
    }
}

fn box_normal(b: &Aabb, p: DVec3) -> DVec3 {
    let c = b.center();
    let h = (b.max - b.min) * 0.5;
    let d = (p - c) / h.max(DVec3::splat(1e-6));
    let a = d.abs();
    if a.z >= a.x && a.z >= a.y {
        DVec3::new(0.0, 0.0, d.z.signum())
    } else if a.x >= a.y {
        DVec3::new(d.x.signum(), 0.0, 0.0)
    } else {
        DVec3::new(0.0, d.y.signum(), 0.0)
    }
}

pub fn is_prop_entity(e: &hp_vmf::Entity) -> bool {
    let c = e.classname.trim().to_ascii_lowercase();
    (c.starts_with("prop_") || c == "func_physbox_multiplayer")
        && e.get("model").is_some_and(|m| m.trim().to_ascii_lowercase().ends_with(".mdl"))
}

pub fn layer_ids(doc: &hp_editor::Document, palette: Option<&str>) -> Vec<Id> {
    fn walk(list: &[hp_vmf::VisGroup], want: &Option<String>, out: &mut Vec<Id>) {
        for g in list {
            let name = g.name.trim();
            let hit = match want {
                Some(w) => name.eq_ignore_ascii_case(w),
                None => {
                    name.len() >= LAYER_PREFIX.len() && name[..LAYER_PREFIX.len()].eq_ignore_ascii_case(LAYER_PREFIX)
                }
            };
            if hit {
                out.push(g.id);
            }
            walk(&g.children, want, out);
        }
    }
    let want = palette.map(layer_name);
    let mut out = Vec::new();
    walk(doc.scene().visgroups(), &want, &mut out);
    out
}

fn entity_in(e: &hp_vmf::Entity, groups: &[Id]) -> bool {
    e.editor.visgroup_ids.iter().any(|v| groups.contains(v))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DabStats {
    pub placed: usize,
    pub removed: usize,
    pub tries: usize,
    pub missed: usize,
    pub slope: usize,
    pub height: usize,
    pub spacing: usize,
    pub overlap: usize,
    pub support: usize,
}

impl DabStats {
    pub fn add(&mut self, o: &DabStats) {
        self.placed += o.placed;
        self.removed += o.removed;
        self.tries += o.tries;
        self.missed += o.missed;
        self.slope += o.slope;
        self.height += o.height;
        self.spacing += o.spacing;
        self.overlap += o.overlap;
        self.support += o.support;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub item: usize,
    pub origin: DVec3,
    pub angles: DVec3,
    pub scale: f64,
    pub skin: u32,
}

pub struct FoliageStroke {
    pub settings: FoliageSettings,
    pub palette: FoliagePalette,
    pub erase: bool,
    pub erase_all_layers: bool,
    occupancy: Occupancy,
    layer_groups: Vec<Id>,
    rng: Rng,
    pub placed_ids: Vec<Id>,
    pub removed_ids: Vec<Id>,
    pub stats: DabStats,
    infos: HashMap<String, Arc<ModelInfo>>,
}

impl FoliageStroke {
    pub fn begin(state: &AppState, settings: &FoliageSettings, erase: bool, seed: u64) -> FoliageStroke {
        let name = settings.palette().map(|p| p.name.clone()).unwrap_or_default();
        let groups = layer_ids(&state.doc, Some(&name));
        Self::begin_with_groups(state, settings, erase, seed, groups)
    }

    pub fn begin_with_groups(
        state: &AppState,
        settings: &FoliageSettings,
        erase: bool,
        seed: u64,
        layer_groups: Vec<Id>,
    ) -> FoliageStroke {
        let palette = settings.palette().cloned().unwrap_or_default();
        let world = StateWorld { state };
        let mut infos = HashMap::new();
        for item in palette.items.iter().filter(|i| i.usable()) {
            let key = item.model.to_ascii_lowercase();
            let info = world.model(&item.model).unwrap_or_else(|| Arc::new(ModelInfo::fallback()));
            infos.insert(key, info);
        }
        let mut stroke = FoliageStroke::new(settings.clone(), palette, erase, seed, layer_groups, infos);
        let scene = state.doc.scene();
        let mut ids: Vec<Id> = scene.entities().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(n) = scene.entity(id) else { continue };
            let e = &n.entity;
            if scene.is_brush_entity(id) || !is_prop_entity(e) || !state.doc.is_visible(ObjRef::Entity(id)) {
                continue;
            }
            let Some(origin) = e.origin().filter(|o| o.is_finite()) else { continue };
            let layer = entity_in(e, &stroke.layer_groups);
            let model = e.get("model").map(normalize_model).unwrap_or_default().to_ascii_lowercase();
            let known = stroke.infos.get(&model).cloned().or_else(|| world.model(&model));
            let mut zr = (f64::NEG_INFINITY, f64::INFINITY);
            let poly = match known {
                Some(info) => {
                    let ang = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
                    zr = z_range(&info, origin, ang, entity_scale(e));
                    footprint_of(&info, origin, ang, entity_scale(e))
                }
                None => match state.assets.entity_bounds(e).filter(|b| b.min.is_finite() && b.max.is_finite()) {
                    Some(b) => vec![
                        DVec2::new(b.min.x, b.min.y),
                        DVec2::new(b.max.x, b.min.y),
                        DVec2::new(b.max.x, b.max.y),
                        DVec2::new(b.min.x, b.max.y),
                    ],
                    None => placed_poly(&ModelInfo::fallback(), origin, 0.0, 1.0),
                },
            };
            if !zr.0.is_finite()
                && let Some(b) = state.assets.entity_bounds(e).filter(|b| b.min.is_finite() && b.max.is_finite())
            {
                zr = (b.min.z, b.max.z);
            }
            stroke.track_z(Some(id), origin, poly, layer, zr.0, zr.1);
        }
        stroke
    }

    pub fn new(
        settings: FoliageSettings,
        palette: FoliagePalette,
        erase: bool,
        seed: u64,
        layer_groups: Vec<Id>,
        infos: HashMap<String, Arc<ModelInfo>>,
    ) -> FoliageStroke {
        let cell = settings.radius().clamp(32.0, 1024.0) * 0.5;
        FoliageStroke {
            settings,
            palette,
            erase,
            erase_all_layers: false,
            occupancy: Occupancy::new(cell),
            layer_groups,
            rng: Rng(seed),
            placed_ids: Vec::new(),
            removed_ids: Vec::new(),
            stats: DabStats::default(),
            infos,
        }
    }

    fn track_z(&mut self, id: Option<Id>, pos: DVec3, poly: Vec<DVec2>, layer: bool, zlo: f64, zhi: f64) {
        let c = if poly.is_empty() { pos.truncate() } else { centroid(&poly) };
        let radius = poly.iter().map(|p| p.distance(c)).fold(0.0, f64::max) + c.distance(pos.truncate());
        self.occupancy.insert(Occupant { zlo, zhi, id, pos, radius, poly, layer, alive: true });
    }

    pub fn tracked(&self) -> usize {
        self.occupancy.len()
    }

    pub fn layer_groups(&self) -> &[Id] {
        &self.layer_groups
    }

    pub fn model_report(&self) -> Vec<(String, bool, f64, f64)> {
        self.palette
            .items
            .iter()
            .filter(|i| i.usable())
            .map(|i| {
                let info = self.info(&i.model);
                (i.model.clone(), info.mesh.is_some(), info.radius, info.height)
            })
            .collect()
    }

    fn info(&self, model: &str) -> Arc<ModelInfo> {
        self.infos.get(&model.to_ascii_lowercase()).cloned().unwrap_or_else(|| Arc::new(ModelInfo::fallback()))
    }

    fn basis(n: DVec3) -> (DVec3, DVec3) {
        let t = if n.z.abs() < 0.9 { DVec3::Z.cross(n) } else { DVec3::X.cross(n) };
        let t = t.try_normalize().unwrap_or(DVec3::X);
        (t, n.cross(t))
    }

    fn in_disc(center: DVec3, normal: DVec3, radius: f64, p: DVec3) -> bool {
        let d = p - center;
        let along = d.dot(normal);
        (d - normal * along).length() <= radius && along.abs() <= radius * 2.0
    }

    pub fn erase_dab(&mut self, center: DVec3, normal: DVec3) -> Vec<Id> {
        let radius = self.settings.radius();
        let n = normal.try_normalize().unwrap_or(DVec3::Z);
        let mut out = Vec::new();
        for i in self.occupancy.near(center, radius) {
            let o = &self.occupancy.items[i];
            let owned = o.layer || self.erase_all_layers;
            if !owned || !Self::in_disc(center, n, radius, o.pos) {
                continue;
            }
            if let Some(id) = o.id {
                out.push(id);
            }
            self.occupancy.items[i].alive = false;
        }
        out.sort_unstable();
        out.dedup();
        self.stats.removed += out.len();
        self.removed_ids.extend_from_slice(&out);
        out
    }

    pub fn paint_dab(&mut self, world: &dyn FoliageWorld, center: DVec3, normal: DVec3) -> Vec<Planned> {
        let items: Vec<usize> = (0..self.palette.items.len()).filter(|i| self.palette.items[*i].usable()).collect();
        if items.is_empty() || !center.is_finite() {
            return Vec::new();
        }
        let s = self.settings.clone();
        let radius = s.radius();
        let n = normal.try_normalize().filter(|n| n.is_finite()).unwrap_or(DVec3::Z);
        let (t1, t2) = Self::basis(n);
        let area = std::f64::consts::PI * radius * radius;
        let target = s.density() * area / DENSITY_AREA;
        let have = self
            .occupancy
            .near(center, radius)
            .into_iter()
            .filter(|i| {
                let o = &self.occupancy.items[*i];
                o.layer && Self::in_disc(center, n, radius, o.pos)
            })
            .count() as f64;
        let missing = target - have;
        if missing <= 0.0 {
            return Vec::new();
        }
        let mut want = missing.floor() as usize;
        if self.rng.next() < missing.fract() {
            want += 1;
        }
        let want = want.min(MAX_PER_DAB);
        if want == 0 {
            return Vec::new();
        }
        let near_props: Vec<Id> = if s.on_props {
            self.occupancy
                .near(center, radius * 1.5)
                .into_iter()
                .filter_map(|i| {
                    let o = &self.occupancy.items[i];
                    (!o.layer).then_some(o.id).flatten()
                })
                .collect()
        } else {
            Vec::new()
        };
        let total: f64 = items.iter().map(|i| self.palette.items[*i].weight).sum();
        let (slope_lo, slope_hi) = s.slope_range();
        let (scale_lo, scale_hi) = s.scale_range();
        let align = if s.align.is_finite() { s.align.clamp(0.0, 1.0) } else { 0.0 };
        let mut out = Vec::new();
        let attempts = (want * 12).clamp(16, MAX_PER_DAB * 4);
        let mut stats = DabStats::default();
        let give_up = want.clamp(32, 160);
        let mut last_hit = 0usize;
        let mut env: Option<Vec<(Option<Id>, hp_shape::TriMesh, Aabb)>> = None;
        let mut dab_meshes: Vec<hp_shape::TriMesh> = Vec::new();
        for _ in 0..attempts {
            if out.len() >= want || stats.tries - last_hit > give_up {
                break;
            }
            stats.tries += 1;
            let (r, a) = (radius * self.rng.next().sqrt(), self.rng.next() * std::f64::consts::TAU);
            let p = center + t1 * (r * a.cos()) + t2 * (r * a.sin());
            let ray = Ray::new(p + n * radius, -n);
            let Some(hit) = world.surface(&ray, radius * 2.5, &near_props) else {
                stats.missed += 1;
                continue;
            };
            if !open_above(world, &hit) {
                stats.support += 1;
                continue;
            }
            let slope = hit.normal.z.clamp(-1.0, 1.0).acos().to_degrees();
            if slope + 1e-6 < slope_lo || slope - 1e-6 > slope_hi {
                stats.slope += 1;
                continue;
            }
            if s.height_limit && (hit.point.z < s.height_min || hit.point.z > s.height_max) {
                stats.height += 1;
                continue;
            }
            let mut pick = self.rng.next() * total;
            let item = items
                .iter()
                .copied()
                .find(|i| {
                    pick -= self.palette.items[*i].weight;
                    pick < 0.0
                })
                .unwrap_or(items[items.len() - 1]);
            let info = self.info(&self.palette.items[item].model);
            let scale = if scale_hi > scale_lo { scale_lo + self.rng.next() * (scale_hi - scale_lo) } else { scale_lo };
            let yaw = if s.random_yaw { (self.rng.next() * 360.0).floor() } else { 0.0 };
            let skin_roll = self.rng.next();
            let up = (DVec3::Z * (1.0 - align) + hit.normal * align).try_normalize().unwrap_or(DVec3::Z);
            let angles = hp_geom::clean_angles(if align > 0.0 {
                hp_geom::align_angles_to_normal(DVec3::new(0.0, yaw, 0.0), up)
            } else {
                DVec3::new(0.0, yaw, 0.0)
            });
            let poly = footprint_of(&info, hit.point, angles, scale);
            let reach = info.radius * scale + s.spacing.max(0.0) + self.occupancy.max_radius + 1.0;
            let neighbours = self.occupancy.near(hit.point, reach);
            if s.spacing > 0.0
                && neighbours.iter().any(|i| {
                    let o = &self.occupancy.items[*i];
                    o.layer && o.pos.distance(hit.point) < s.spacing
                })
            {
                stats.spacing += 1;
                continue;
            }
            if s.avoid_overlap
                && neighbours.iter().any(|i| {
                    let o = &self.occupancy.items[*i];
                    if hit.prop.is_some() && o.id == hit.prop {
                        return false;
                    }
                    let close = o.pos.truncate().distance(hit.point.truncate()) < o.radius + info.radius * scale;
                    let slack = info.radius * scale * 2.0 + 8.0;
                    let (lo, hi) = (hit.point.z - slack, hit.point.z + info.height * scale + slack);
                    let vertical = lo <= o.zhi + 4.0 && hi >= o.zlo - 4.0;
                    close && vertical && polygons_overlap(&poly, &o.poly)
                })
            {
                stats.overlap += 1;
                continue;
            }
            let rot = hp_geom::angles_to_quat(angles);
            let base = hit.point + up * (info.lift * scale);
            let reach_up = (info.radius + info.height) * scale + 16.0;
            let mut top: f64 = f64::NEG_INFINITY;
            let mut supported = true;
            for q in info.contact_samples() {
                let wp = base + rot * DVec3::new(q.x * scale, q.y * scale, -info.lift * scale);
                let probe = Ray::new(wp + up * reach_up, -up);
                let ok = world.surface(&probe, reach_up * 2.0, &near_props).filter(|h| {
                    if !open_above(world, h) {
                        return false;
                    }
                    let sl = h.normal.z.clamp(-1.0, 1.0).acos().to_degrees();
                    let slope_ok = sl + 1e-6 >= slope_lo && sl - 1e-6 <= slope_hi;
                    let height_ok = !s.height_limit || (h.point.z >= s.height_min && h.point.z <= s.height_max);
                    slope_ok && height_ok
                });
                match ok {
                    Some(h) => top = top.max((h.point - wp).dot(up)),
                    None => {
                        supported = false;
                        break;
                    }
                }
            }
            if !supported {
                stats.support += 1;
                continue;
            }
            let mut origin = hit.point;
            let mut placed_mesh = None;
            if s.drop_to_surface {
                origin = base + up * top.max(-info.radius * scale);
                if let Some(mesh) = &info.mesh {
                    let m = glam::DMat4::from_scale_rotation_translation(DVec3::splat(scale), rot, DVec3::ZERO);
                    let moving = mesh.transformed(&m);
                    if env.is_none() {
                        let lo = center - DVec3::splat(radius * 2.0 + reach_up);
                        let hi = center + DVec3::splat(radius * 2.0 + reach_up);
                        env = Some(
                            world
                                .obstacles(Aabb::new(lo, hi))
                                .into_iter()
                                .map(|(id, m)| {
                                    let b = m.bounds();
                                    (id, m, b)
                                })
                                .collect(),
                        );
                    }
                    let mb = moving.bounds();
                    let (fx0, fy0) = (origin.x + mb.min.x - 1.0, origin.y + mb.min.y - 1.0);
                    let (fx1, fy1) = (origin.x + mb.max.x + 1.0, origin.y + mb.max.y + 1.0);
                    let under = |b: &Aabb| !(b.max.x < fx0 || b.min.x > fx1 || b.max.y < fy0 || b.min.y > fy1);
                    let refs: Vec<&hp_shape::TriMesh> = env
                        .iter()
                        .flatten()
                        .filter(|(_, _, b)| under(b))
                        .map(|(_, m, _)| m)
                        .chain(dab_meshes.iter().filter(|m| under(&m.bounds())))
                        .collect();
                    let support: Vec<&hp_shape::TriMesh> = env
                        .iter()
                        .flatten()
                        .filter(|(id, _, b)| (id.is_none() || *id == hit.prop) && under(b))
                        .map(|(_, m, _)| m)
                        .collect();
                    let ceiling = highest_under(
                        &support,
                        origin.truncate() + mb.min.truncate(),
                        origin.truncate() + mb.max.truncate(),
                    )
                    .unwrap_or(origin.z);
                    let start_z = (ceiling - mb.min.z + 1.0).max(origin.z + 1.0);
                    let start = DVec3::new(origin.x, origin.y, start_z);
                    let span = start_z - origin.z + reach_up * 2.0;
                    match hp_shape::sweep(&moving, start, &refs, DVec3::NEG_Z, 0.0, span) {
                        Some(c) => origin = start - DVec3::Z * c.t,
                        None => {
                            stats.support += 1;
                            continue;
                        }
                    }
                    let placed = moving.translated(origin);
                    let ext = (mb.max.z - mb.min.z) + 1.0;
                    let cuts = |d: Option<(f64, usize)>| d.is_some_and(|(depth, _)| depth > PENETRATION);
                    let props_near: Vec<&hp_shape::TriMesh> = env
                        .iter()
                        .flatten()
                        .filter(|(id, _, b)| id.is_some() && under(b))
                        .map(|(_, m, _)| m)
                        .chain(dab_meshes.iter().filter(|m| under(&m.bounds())))
                        .collect();
                    let sunk = cuts(hp_shape::exit_depth(&placed, DVec3::ZERO, &refs, DVec3::NEG_Z, ext))
                        || props_near.iter().any(|m| {
                            let b = m.bounds();
                            cuts(hp_shape::exit_depth(
                                m,
                                DVec3::ZERO,
                                &[&placed],
                                DVec3::NEG_Z,
                                b.max.z - b.min.z + 1.0,
                            ))
                        });
                    if sunk {
                        stats.overlap += 1;
                        continue;
                    }
                    placed_mesh = Some(moving);
                }
            }
            origin += up * s.sink;
            if !origin.is_finite() {
                continue;
            }
            let skin = if s.random_skin && info.skins > 1 {
                ((skin_roll * f64::from(info.skins)).floor() as u32).min(info.skins - 1)
            } else {
                0
            };
            if let Some(m) = placed_mesh {
                dab_meshes.push(m.translated(origin));
            }
            let zr = z_range(&info, origin, angles, scale);
            self.track_z(None, origin, poly, true, zr.0, zr.1);
            last_hit = stats.tries;
            out.push(Planned { item, origin, angles, scale, skin });
        }
        stats.placed = out.len();
        self.stats.add(&stats);
        out
    }

    pub fn items_of(&self, planned: &[Planned]) -> Vec<SprinkleItem> {
        planned
            .iter()
            .filter_map(|p| {
                let item = self.palette.items.get(p.item)?;
                let mut kv = vec![
                    ("model".to_owned(), normalize_model(&item.model)),
                    ("angles".to_owned(), hp_vmf::fmt_vec3(p.angles)),
                ];
                if (p.scale - 1.0).abs() > 1e-6 {
                    kv.push((SCALE_KEY.to_owned(), format!("{:.3}", p.scale)));
                }
                if p.skin > 0 {
                    kv.push(("skin".to_owned(), p.skin.to_string()));
                }
                let class = if item.classname.trim().is_empty() { "prop_static" } else { item.classname.trim() };
                Some(SprinkleItem { classname: class.to_owned(), origin: p.origin, keyvalues: kv })
            })
            .collect()
    }

    pub fn note_ids(&mut self, planned: usize, ids: &[Id]) {
        let start = self.occupancy.items.len().saturating_sub(planned);
        for (o, id) in self.occupancy.items[start..].iter_mut().zip(ids) {
            o.id = Some(*id);
        }
        self.placed_ids.extend_from_slice(ids);
    }

    pub fn forget(&mut self, id: Id) {
        self.occupancy.kill(id);
    }
}

fn highest_under(meshes: &[&hp_shape::TriMesh], lo: DVec2, hi: DVec2) -> Option<f64> {
    let mut best: Option<f64> = None;
    for m in meshes {
        for t in &m.tris {
            let (Some(a), Some(b), Some(c)) =
                (m.verts.get(t[0] as usize), m.verts.get(t[1] as usize), m.verts.get(t[2] as usize))
            else {
                continue;
            };
            let (x0, x1) = (a.x.min(b.x).min(c.x), a.x.max(b.x).max(c.x));
            let (y0, y1) = (a.y.min(b.y).min(c.y), a.y.max(b.y).max(c.y));
            if x1 < lo.x || x0 > hi.x || y1 < lo.y || y0 > hi.y {
                continue;
            }
            let z = a.z.max(b.z).max(c.z);
            best = Some(best.map_or(z, |v: f64| v.max(z)));
        }
    }
    best
}

fn entity_scale(e: &hp_vmf::Entity) -> f64 {
    let read = |k: &str| e.get(k).and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s > 0.0);
    read(SCALE_KEY).or_else(|| read("uniformscale")).map_or(1.0, |s| s.min(1.0e4))
}

pub fn apply_paint(
    state: &mut AppState,
    stroke: &mut FoliageStroke,
    center: DVec3,
    normal: DVec3,
) -> Result<usize, String> {
    let planned = {
        let world = StateWorld { state };
        stroke.paint_dab(&world, center, normal)
    };
    if planned.is_empty() {
        return Ok(0);
    }
    let items = stroke.items_of(&planned);
    let ids = state.doc.sprinkle(&items, &[]).map_err(|e| e.to_string())?;
    stroke.note_ids(planned.len(), &ids);
    let refs: Vec<ObjRef> = ids.iter().map(|i| ObjRef::Entity(*i)).collect();
    match stroke.layer_groups.first().copied().filter(|g| state.doc.visgroup(*g).is_some()) {
        Some(g) => {
            state.doc.add_to_visgroup(g, &refs).map_err(|e| e.to_string())?;
        }
        None => {
            let g = state.doc.create_visgroup(&stroke.palette.layer_name(), None, &refs).map_err(|e| e.to_string())?;
            stroke.layer_groups.insert(0, g);
        }
    }
    Ok(ids.len())
}

pub fn apply_erase(
    state: &mut AppState,
    stroke: &mut FoliageStroke,
    center: DVec3,
    normal: DVec3,
) -> Result<usize, String> {
    let ids = stroke.erase_dab(center, normal);
    if ids.is_empty() {
        return Ok(0);
    }
    state.doc.sprinkle(&[], &ids).map_err(|e| e.to_string())?;
    Ok(ids.len())
}

pub fn surface_below(state: &AppState, point: DVec3, height: f64) -> Option<SurfaceHit> {
    let world = StateWorld { state };
    let h = if height.is_finite() && height > 0.0 { height } else { 64.0 };
    world.surface(&Ray::new(point + DVec3::Z * h, DVec3::NEG_Z), f64::INFINITY, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat {
        z: f64,
        info: Arc<ModelInfo>,
        bumps: Vec<(DVec2, f64, f64)>,
    }

    impl FoliageWorld for Flat {
        fn surface(&self, ray: &Ray, max: f64, _props: &[Id]) -> Option<SurfaceHit> {
            if ray.dir.z >= -1e-9 {
                return None;
            }
            let mut z = self.z;
            for (c, r, h) in &self.bumps {
                let t = (self.z - ray.origin.z) / ray.dir.z;
                let p = ray.origin + ray.dir * t;
                if p.truncate().distance(*c) < *r {
                    z = z.max(self.z + h);
                }
            }
            let t = (z - ray.origin.z) / ray.dir.z;
            (t >= 0.0 && t <= max).then(|| SurfaceHit { point: ray.at(t), normal: DVec3::Z, prop: None })
        }

        fn model(&self, _model: &str) -> Option<Arc<ModelInfo>> {
            Some(Arc::clone(&self.info))
        }
    }

    struct Slope {
        k: f64,
        half: f64,
        info: Arc<ModelInfo>,
        props: Vec<(Id, hp_shape::TriMesh)>,
    }

    impl Slope {
        fn z(&self, x: f64) -> f64 {
            self.k * x
        }
        fn normal(&self) -> DVec3 {
            DVec3::new(-self.k, 0.0, 1.0).normalize()
        }
    }

    impl FoliageWorld for Slope {
        fn surface(&self, ray: &Ray, max: f64, props: &[Id]) -> Option<SurfaceHit> {
            let n = self.normal();
            let denom = n.dot(ray.dir);
            let mut best = None;
            if denom.abs() > 1e-9 {
                let t = (DVec3::ZERO - ray.origin).dot(n) / denom;
                let p = ray.at(t);
                let inside = p.x.abs() <= self.half && p.y.abs() <= self.half;
                if t >= 0.0 && t <= max && inside {
                    best = Some((t, SurfaceHit { point: p, normal: n, prop: None }));
                }
            }
            for (id, m) in self.props.iter().filter(|(id, _)| props.contains(id)) {
                if let Some(h) = hp_shape::ray_cast(&[m], ray.origin, ray.dir, max)
                    && best.as_ref().is_none_or(|b: &(f64, SurfaceHit)| h.t < b.0)
                {
                    let normal = if h.normal.dot(ray.dir) > 0.0 { -h.normal } else { h.normal };
                    best = Some((h.t, SurfaceHit { point: ray.at(h.t), normal, prop: Some(*id) }));
                }
            }
            best.map(|b| b.1)
        }

        fn model(&self, _model: &str) -> Option<Arc<ModelInfo>> {
            Some(Arc::clone(&self.info))
        }

        fn prop_ray(&self, ray: &Ray, max: f64, id: Id) -> Option<DVec3> {
            let (_, m) = self.props.iter().find(|(i, _)| *i == id)?;
            hp_shape::ray_cast(&[m], ray.origin, ray.dir, max).map(|h| ray.at(h.t))
        }

        fn obstacles(&self, _bounds: Aabb) -> Vec<(Option<Id>, hp_shape::TriMesh)> {
            let h = self.half;
            let mut m = hp_shape::TriMesh::new();
            let pts = [
                DVec3::new(-h, -h, self.z(-h)),
                DVec3::new(h, -h, self.z(h)),
                DVec3::new(h, h, self.z(h)),
                DVec3::new(-h, h, self.z(-h)),
            ];
            m.add_polygon(&pts, 0);
            let mut out = vec![(None, m)];
            out.extend(self.props.iter().map(|(id, m)| (Some(*id), m.clone())));
            out
        }
    }

    fn boulder() -> Arc<ModelInfo> {
        let mut mesh = hp_shape::TriMesh::convex_box(DVec3::new(-12.0, -8.0, -3.0), DVec3::new(12.0, 8.0, 14.0), 0);
        let tip = mesh.add_vertex(DVec3::new(0.0, 0.0, -6.0));
        let a = mesh.add_vertex(DVec3::new(-4.0, -4.0, -3.0));
        let b = mesh.add_vertex(DVec3::new(4.0, 0.0, -3.0));
        mesh.add_tri([a, b, tip], 0);
        Arc::new(ModelInfo::from_mesh(mesh, 1).expect("info"))
    }

    fn slope_stroke(info: &Arc<ModelInfo>, settings: FoliageSettings) -> FoliageStroke {
        let mut palette = FoliagePalette { name: "Boulders".to_owned(), items: Vec::new() };
        palette.add_model("models/boulder.mdl");
        let mut infos = HashMap::new();
        infos.insert("models/boulder.mdl".to_owned(), Arc::clone(info));
        FoliageStroke::new(settings, palette, false, 21, Vec::new(), infos)
    }

    #[test]
    fn props_rest_on_the_real_mesh_without_floating_or_sinking() {
        let info = boulder();
        assert_eq!(info.lift, 6.0);
        assert!(info.contact.len() <= 3, "the contact patch is the tip: {:?}", info.contact);
        let world = Slope { k: 0.3, half: 4000.0, info: Arc::clone(&info), props: Vec::new() };
        let settings = FoliageSettings { density: 8.0, radius: 300.0, slope_max: 30.0, ..FoliageSettings::default() };
        let mut st = slope_stroke(&info, settings);
        let placed = st.paint_dab(&world, DVec3::ZERO, world.normal());
        assert!(placed.len() > 5, "{}", placed.len());
        let mesh = info.mesh.as_ref().expect("mesh");
        for p in &placed {
            let m = glam::DMat4::from_scale_rotation_translation(
                DVec3::splat(p.scale),
                hp_geom::angles_to_quat(p.angles),
                p.origin,
            );
            let gap =
                mesh.transformed(&m).used_vertices().iter().map(|v| v.z - world.z(v.x)).fold(f64::INFINITY, f64::min);
            assert!(gap.abs() < 0.05, "the lowest point touches the slope, gap {gap} at {:?}", p.origin);
        }
    }

    #[test]
    fn a_tall_neighbour_does_not_lift_props_onto_it() {
        let info = boulder();
        let pillar = hp_shape::TriMesh::convex_box(DVec3::new(50.0, -10.0, 0.0), DVec3::new(70.0, 10.0, 500.0), 0);
        let world = Slope { k: 0.0, half: 4000.0, info: Arc::clone(&info), props: vec![(7, pillar)] };
        let settings = FoliageSettings { density: 200.0, radius: 120.0, ..FoliageSettings::default() };
        let mut st = slope_stroke(&info, settings);
        let rect =
            vec![DVec2::new(50.0, -10.0), DVec2::new(70.0, -10.0), DVec2::new(70.0, 10.0), DVec2::new(50.0, 10.0)];
        st.track_z(Some(7), DVec3::new(60.0, 0.0, 0.0), rect, false, f64::NEG_INFINITY, f64::INFINITY);
        let placed = st.paint_dab(&world, DVec3::ZERO, DVec3::Z);
        assert!(placed.len() > 3, "{}", placed.len());
        for p in &placed {
            assert!((p.origin.z - info.lift * p.scale).abs() < 0.05, "a prop left the ground: {:?}", p.origin);
        }
    }

    #[test]
    fn props_painted_around_and_on_a_rock_never_cut_into_it() {
        let info = boulder();
        let mut rock = hp_shape::TriMesh::new();
        let pts = [
            DVec3::new(-60.0, -40.0, 0.0),
            DVec3::new(60.0, -50.0, 0.0),
            DVec3::new(70.0, 45.0, 0.0),
            DVec3::new(-55.0, 50.0, 0.0),
            DVec3::new(-20.0, -10.0, 70.0),
            DVec3::new(25.0, 5.0, 90.0),
        ];
        let v: Vec<u32> = pts.iter().map(|p| rock.add_vertex(*p)).collect();
        for t in [[0, 1, 4], [1, 5, 4], [1, 2, 5], [2, 3, 5], [3, 4, 5], [3, 0, 4], [0, 3, 2], [0, 2, 1]] {
            rock.add_tri([v[t[0]], v[t[1]], v[t[2]]], 0);
        }
        let world = Slope { k: 0.1, half: 4000.0, info: Arc::clone(&info), props: vec![(7, rock.clone())] };
        let settings = FoliageSettings { density: 120.0, radius: 160.0, slope_max: 89.0, ..FoliageSettings::default() };
        let mut st = slope_stroke(&info, settings);
        let hull: Vec<DVec2> = hp_shape::hull_2d(&pts.iter().map(|p| p.truncate()).collect::<Vec<_>>());
        st.track_z(Some(7), DVec3::ZERO, hull, false, f64::NEG_INFINITY, f64::INFINITY);
        let mesh = info.mesh.as_ref().expect("mesh");
        let mut on_rock = 0;
        for c in [DVec3::ZERO, DVec3::new(90.0, 0.0, 9.0), DVec3::new(-90.0, 30.0, -9.0)] {
            for p in st.paint_dab(&world, c, DVec3::Z) {
                let m = glam::DMat4::from_scale_rotation_translation(
                    DVec3::splat(p.scale),
                    hp_geom::angles_to_quat(p.angles),
                    p.origin,
                );
                let placed = mesh.transformed(&m);
                if p.origin.z > 20.0 {
                    on_rock += 1;
                }
                let depth = hp_shape::exit_depth(&placed, DVec3::ZERO, &[&rock], DVec3::NEG_Z, 200.0);
                assert!(
                    depth.is_none_or(|(d, _)| d < 0.05),
                    "a prop cuts into the rock by {depth:?} at {:?}",
                    p.origin
                );
                let back = hp_shape::exit_depth(&rock, DVec3::ZERO, &[&placed], DVec3::NEG_Z, 200.0);
                assert!(
                    back.is_none_or(|(d, _)| d < 0.05),
                    "the rock is sunk into a prop by {back:?} at {:?}",
                    p.origin
                );
            }
        }
        assert!(on_rock > 0, "some props are painted on the rock");
    }

    #[test]
    fn nothing_is_painted_inside_or_under_a_tall_prop() {
        let info = boulder();
        let mut tower = hp_shape::TriMesh::convex_box(DVec3::new(-80.0, -80.0, 0.0), DVec3::new(80.0, 80.0, 600.0), 0);
        let floor = [
            DVec3::new(-80.0, -80.0, 150.0),
            DVec3::new(80.0, -80.0, 150.0),
            DVec3::new(80.0, 80.0, 150.0),
            DVec3::new(-80.0, 80.0, 150.0),
        ];
        tower.add_polygon(&floor, 0);
        let world = Slope { k: 0.0, half: 4000.0, info: Arc::clone(&info), props: vec![(7, tower)] };
        let settings = FoliageSettings { density: 150.0, radius: 200.0, ..FoliageSettings::default() };
        let mut st = slope_stroke(&info, settings);
        let rect =
            vec![DVec2::new(-80.0, -80.0), DVec2::new(80.0, -80.0), DVec2::new(80.0, 80.0), DVec2::new(-80.0, 80.0)];
        st.track_z(Some(7), DVec3::new(0.0, 0.0, 300.0), rect, false, 0.0, 600.0);
        let placed = st.paint_dab(&world, DVec3::ZERO, DVec3::Z);
        assert!(!placed.is_empty());
        for p in &placed {
            let inside = p.origin.x.abs() < 80.0 && p.origin.y.abs() < 80.0;
            assert!(!inside || p.origin.z > 599.0, "a prop was placed inside the tower at {:?}", p.origin);
        }
    }

    #[test]
    fn a_footprint_over_an_edge_is_not_placed() {
        let info = rock();
        let world = Slope { k: 0.0, half: 100.0, info: Arc::clone(&info), props: Vec::new() };
        let settings = FoliageSettings { density: 400.0, radius: 160.0, ..FoliageSettings::default() };
        let mut st = stroke(settings, &info);
        let placed = st.paint_dab(&world, DVec3::ZERO, DVec3::Z);
        assert!(!placed.is_empty());
        assert!(st.stats.support > 0, "some footprints hang over the edge");
        for p in &placed {
            for q in info.contact_samples() {
                let w = p.origin + hp_geom::angles_to_quat(p.angles) * DVec3::new(q.x, q.y, 0.0) * p.scale;
                assert!(w.x.abs() <= 100.0 + 1e-6 && w.y.abs() <= 100.0 + 1e-6, "unsupported contact at {w:?}");
            }
        }
        let steep = Slope { k: 1.0, half: 4000.0, info: Arc::clone(&info), props: Vec::new() };
        let mut st = stroke(FoliageSettings { slope_max: 30.0, ..FoliageSettings::default() }, &info);
        assert!(st.paint_dab(&steep, DVec3::ZERO, steep.normal()).is_empty());
    }

    fn rock() -> Arc<ModelInfo> {
        let pts: Vec<DVec3> = [(-10.0, -10.0, -4.0), (10.0, -10.0, -4.0), (10.0, 10.0, 12.0), (-10.0, 10.0, 0.0)]
            .iter()
            .map(|(x, y, z)| DVec3::new(*x, *y, *z))
            .collect();
        Arc::new(ModelInfo::from_points(&pts, 3).expect("info"))
    }

    fn stroke(settings: FoliageSettings, info: &Arc<ModelInfo>) -> FoliageStroke {
        let mut palette = FoliagePalette { name: "Rocks".to_owned(), items: Vec::new() };
        palette.add_model("models/rock.mdl");
        palette.add_model("models\\grass.mdl");
        let mut infos = HashMap::new();
        infos.insert("models/rock.mdl".to_owned(), Arc::clone(info));
        infos.insert("models/grass.mdl".to_owned(), Arc::clone(info));
        FoliageStroke::new(settings, palette, false, 11, Vec::new(), infos)
    }

    #[test]
    fn model_info_reads_the_hull_and_the_lift() {
        let info = rock();
        assert_eq!(info.lift, 4.0);
        assert_eq!(info.height, 16.0);
        assert_eq!(info.skins, 3);
        assert_eq!(info.hull.len(), 4);
        assert!((info.radius - 200f64.sqrt()).abs() < 1e-9);
        assert!(ModelInfo::from_points(&[], 1).is_none());
    }

    #[test]
    fn convex_polygons_overlap() {
        let sq = |x: f64, y: f64, r: f64| {
            vec![DVec2::new(x - r, y - r), DVec2::new(x + r, y - r), DVec2::new(x + r, y + r), DVec2::new(x - r, y + r)]
        };
        assert!(polygons_overlap(&sq(0.0, 0.0, 10.0), &sq(15.0, 0.0, 10.0)));
        assert!(!polygons_overlap(&sq(0.0, 0.0, 10.0), &sq(25.0, 0.0, 10.0)));
        let diamond: Vec<DVec2> = sq(0.0, 0.0, 10.0).iter().map(|p| rotate(*p, 45.0)).collect();
        assert!(!polygons_overlap(&diamond, &sq(23.0, 23.0, 10.0)), "the corners miss");
        assert!(polygons_overlap(&diamond, &sq(18.0, 0.0, 5.0)));
    }

    #[test]
    fn density_spacing_and_overlap() {
        let info = rock();
        let world = Flat { z: 0.0, info: Arc::clone(&info), bumps: Vec::new() };
        let settings = FoliageSettings { radius: 256.0, density: 20.0, ..FoliageSettings::default() };
        let mut s = stroke(settings, &info);
        let first = s.paint_dab(&world, DVec3::ZERO, DVec3::Z);
        let area = std::f64::consts::PI * 256.0 * 256.0;
        let expected = 20.0 * area / DENSITY_AREA;
        assert!((first.len() as f64) > expected * 0.6 && (first.len() as f64) <= expected + 1.0, "{}", first.len());
        let again = s.paint_dab(&world, DVec3::ZERO, DVec3::Z);
        assert!(again.len() <= 2, "a second dab at the same place keeps the density: {}", again.len());
        for (i, a) in first.iter().enumerate() {
            assert!(a.origin.truncate().length() <= 256.0 + 1e-6);
            assert!((a.origin.z - info.lift).abs() < 1e-9, "dropped so the lowest point touches: {:?}", a.origin);
            let pa = placed_poly(&info, DVec3::new(a.origin.x, a.origin.y, 0.0), a.angles.y, a.scale);
            for b in &first[i + 1..] {
                let pb = placed_poly(&info, DVec3::new(b.origin.x, b.origin.y, 0.0), b.angles.y, b.scale);
                assert!(!polygons_overlap(&pa, &pb), "props overlap");
            }
        }
        let items = s.items_of(&first);
        assert!(items.iter().all(|i| i.classname == "prop_static"));
        assert!(items.iter().any(|i| i.keyvalues.iter().any(|(k, v)| k == "model" && v == "models/grass.mdl")));

        let spaced =
            FoliageSettings { density: 50.0, spacing: 80.0, avoid_overlap: false, ..FoliageSettings::default() };
        let mut s = stroke(spaced, &info);
        let p = s.paint_dab(&world, DVec3::ZERO, DVec3::Z);
        assert!(p.len() > 3);
        for (i, a) in p.iter().enumerate() {
            for b in &p[i + 1..] {
                assert!(a.origin.distance(b.origin) >= 80.0 - 1e-9);
            }
        }
    }

    #[test]
    fn a_big_neighbour_is_seen_from_far_away() {
        let info = rock();
        let world = Flat { z: 0.0, info: Arc::clone(&info), bumps: Vec::new() };
        let settings = FoliageSettings { radius: 300.0, density: 80.0, ..FoliageSettings::default() };
        let mut s = stroke(settings, &info);
        let big: Vec<DVec2> = (0..12)
            .map(|i| {
                let a = f64::from(i) / 12.0 * std::f64::consts::TAU;
                DVec2::new(a.cos(), a.sin()) * 220.0
            })
            .collect();
        s.track_z(Some(9999), DVec3::ZERO, big.clone(), false, f64::NEG_INFINITY, f64::INFINITY);
        let placed = s.paint_dab(&world, DVec3::ZERO, DVec3::Z);
        assert!(!placed.is_empty());
        for p in &placed {
            let poly = placed_poly(&info, DVec3::new(p.origin.x, p.origin.y, 0.0), p.angles.y, p.scale);
            assert!(!polygons_overlap(&poly, &big), "a prop at {:?} overlaps the big footprint", p.origin);
        }
    }

    #[test]
    fn limits_scale_skins_and_erase() {
        let info = rock();
        let world = Flat { z: 100.0, info: Arc::clone(&info), bumps: Vec::new() };
        let high =
            FoliageSettings { height_limit: true, height_min: 200.0, height_max: 300.0, ..FoliageSettings::default() };
        let mut s = stroke(high, &info);
        assert!(s.paint_dab(&world, DVec3::new(0.0, 0.0, 100.0), DVec3::Z).is_empty());
        assert!(s.stats.height > 0);
        let steep = FoliageSettings { slope_min: 10.0, slope_max: 40.0, ..FoliageSettings::default() };
        let mut s = stroke(steep, &info);
        assert!(s.paint_dab(&world, DVec3::new(0.0, 0.0, 100.0), DVec3::Z).is_empty());
        assert!(s.stats.slope > 0);

        let varied = FoliageSettings {
            scale_min: 0.5,
            scale_max: 1.5,
            random_skin: true,
            density: 10.0,
            drop_to_surface: false,
            sink: -2.0,
            ..FoliageSettings::default()
        };
        let mut s = stroke(varied, &info);
        let p = s.paint_dab(&world, DVec3::new(0.0, 0.0, 100.0), DVec3::Z);
        assert!(!p.is_empty());
        assert!(p.iter().all(|a| (0.5..=1.5).contains(&a.scale) && a.skin < 3));
        assert!(p.iter().any(|a| a.skin > 0));
        assert!(p.iter().all(|a| (a.origin.z - 98.0).abs() < 1e-9));
        let items = s.items_of(&p);
        assert!(items.iter().all(|i| i.keyvalues.iter().any(|(k, _)| k == "modelscale")));

        let ids: Vec<Id> = (100..100 + p.len() as Id).collect();
        s.note_ids(p.len(), &ids);
        let gone = s.erase_dab(DVec3::new(0.0, 0.0, 100.0), DVec3::Z);
        assert_eq!(gone, ids);
        assert!(s.erase_dab(DVec3::new(0.0, 0.0, 100.0), DVec3::Z).is_empty());
    }

    #[test]
    fn dropping_follows_the_lowest_ground_under_the_hull() {
        let info = rock();
        let world = Flat { z: 0.0, info: Arc::clone(&info), bumps: vec![(DVec2::ZERO, 2000.0, 8.0)] };
        let settings = FoliageSettings { density: 2.0, random_yaw: false, ..FoliageSettings::default() };
        let mut s = stroke(settings, &info);
        let p = s.paint_dab(&world, DVec3::new(0.0, 0.0, 8.0), DVec3::Z);
        assert!(!p.is_empty());
        assert!(p.iter().all(|a| (a.origin.z - (8.0 + info.lift)).abs() < 1e-9));
    }

    fn painted_state() -> (AppState, FoliageSettings) {
        let mut state = crate::testing::test_state();
        let mut settings = FoliageSettings { density: 6.0, radius: 200.0, ..FoliageSettings::default() };
        settings.palette_mut().add_model("models/props_junk/rock001a.mdl");
        state.doc.begin("Foliage paint");
        let mut stroke = FoliageStroke::begin(&state, &settings, false, 5);
        let n = apply_paint(&mut state, &mut stroke, DVec3::ZERO, DVec3::Z).expect("paint");
        let m = apply_paint(&mut state, &mut stroke, DVec3::new(120.0, 0.0, 0.0), DVec3::Z).expect("paint");
        assert!(state.doc.commit());
        assert!(n > 0 && n + m == stroke.placed_ids.len(), "{n} {m}");
        (state, settings)
    }

    #[test]
    fn a_stroke_is_one_undo_step_in_a_layer_visgroup() {
        let (mut state, settings) = painted_state();
        let groups = layer_ids(&state.doc, Some(DEFAULT_PALETTE));
        assert_eq!(groups.len(), 1);
        assert_eq!(state.doc.visgroup(groups[0]).map(|g| g.name.clone()), Some("Foliage: Foliage".to_owned()));
        let members = state.doc.visgroup_members(groups[0]).len();
        assert!(members > 0);
        let (undo, _) = state.doc.history_len();
        state.doc.begin("Foliage erase");
        let mut erase = FoliageStroke::begin(&state, &settings, true, 9);
        assert!(erase.tracked() >= members);
        let gone = apply_erase(&mut state, &mut erase, DVec3::ZERO, DVec3::Z).expect("erase");
        assert!(state.doc.commit());
        assert!(gone > 0 && gone <= members);
        assert_eq!(state.doc.history_len().0, undo + 1);
        assert!(state.doc.undo());
        assert_eq!(state.doc.visgroup_members(groups[0]).len(), members);
        assert!(state.doc.undo());
        assert!(layer_ids(&state.doc, None).is_empty(), "undoing the paint stroke removes the layer");
    }

    #[test]
    fn settings_names_and_validation() {
        assert_eq!(FoliageMode::from_name("DETAIL_alpha"), Some(FoliageMode::DetailAlpha));
        assert_eq!(FoliageMode::from_name("x"), None);
        let mut s = FoliageSettings::default();
        assert!(s.validate().is_ok());
        s.spacing = -1.0;
        assert!(s.validate().is_err());
        s.spacing = 0.0;
        s.height_limit = true;
        s.height_min = 10.0;
        s.height_max = 0.0;
        assert!(s.validate().is_err());
        let mut p = FoliagePalette::default();
        assert!(p.add_model(" models\\a.mdl "));
        assert!(!p.add_model("MODELS/A.MDL"));
        assert_eq!(p.items[0].model, "models/a.mdl");
        assert_eq!(layer_name(" Trees "), "Foliage: Trees");
    }
}
