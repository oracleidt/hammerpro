use std::time::{Duration, Instant};

use ahash::{AHashMap, AHashSet};
use glam::Vec3;
use hp_render::{MaterialId, ModelId, ModelInstance, ModelPart, ObjectId, OrthoAxis, RenderMode, ViewKind, ViewParams};

pub(crate) const MIN_DIM: u32 = 128;
pub(crate) const MARGIN: f32 = 2.0;
pub(crate) const VIEW_TTL: u64 = 120;
pub(crate) const ESCAPE_TTL: u64 = 240;
pub(crate) const UPDATE_INTERVAL: Duration = Duration::from_millis(200);
pub(crate) const LOWER_AFTER: Duration = Duration::from_secs(2);
pub(crate) const MAX_STREAMING: usize = 24;
pub(crate) const MIN_VIEW_HEIGHT: f32 = 1080.0;
const AGGRESSION_STEPS: [f32; 5] = [1.0, 2.0, 4.0, 8.0, 16.0];
const DENSITY_FLOOR: f32 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Density {
    Scale(f32),
    Uv(f32),
    Unknown,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Placement {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub mats: Vec<(MaterialId, Density)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ModelMeta {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub mats: Vec<(MaterialId, f32)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SkyFrame {
    pub origin: Vec3,
    pub inv_scale: f32,
    pub objects: AHashSet<ObjectId>,
}

impl SkyFrame {
    fn eye(&self, eye: Vec3) -> Vec3 {
        self.origin + eye * self.inv_scale
    }
}

#[derive(Default)]
pub(crate) struct ScenePlacements {
    pub objects: AHashMap<ObjectId, Placement>,
    pub instances: Vec<(ObjectId, Placement)>,
    pub unknown_models: bool,
    pub revision: u64,
}

impl ScenePlacements {
    pub(crate) fn set_object(&mut self, id: ObjectId, placement: Placement) {
        if self.objects.get(&id) != Some(&placement) {
            self.objects.insert(id, placement);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub(crate) fn forget(&mut self, id: ObjectId) {
        if self.objects.remove(&id).is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.objects.clear();
        self.instances.clear();
        self.unknown_models = false;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn set_instances(&mut self, models: &AHashMap<ModelId, ModelMeta>, list: &[ModelInstance]) {
        self.set_instance_metas(list.iter().map(|inst| (inst.object, models.get(&inst.model), &inst.transform)));
    }

    pub(crate) fn set_instance_metas<'a>(
        &mut self,
        items: impl Iterator<Item = (ObjectId, Option<&'a ModelMeta>, &'a [[f32; 4]; 4])>,
    ) {
        let mut unknown = false;
        let mut out = Vec::new();
        for (object, meta, transform) in items {
            match meta {
                Some(meta) => out.extend(instance_placement(meta, transform).map(|p| (object, p))),
                None => unknown = true,
            }
        }
        if out != self.instances || unknown != self.unknown_models {
            self.instances = out;
            self.unknown_models = unknown;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn all(&self) -> impl Iterator<Item = (ObjectId, &Placement)> {
        self.objects.iter().map(|(id, p)| (*id, p)).chain(self.instances.iter().map(|(id, p)| (*id, p)))
    }
}

fn instance_placement(meta: &ModelMeta, m: &[[f32; 4]; 4]) -> Option<Placement> {
    let mat = glam::Mat4::from_cols_array_2d(m);
    if !mat.is_finite() {
        return None;
    }
    let scale =
        [mat.x_axis, mat.y_axis, mat.z_axis].iter().map(|c| c.truncate().length()).fold(f32::INFINITY, f32::min);
    let (lo, hi) = (Vec3::from(meta.min), Vec3::from(meta.max));
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let p = mat.transform_point3(c);
        min = min.min(p);
        max = max.max(p);
    }
    let mats = meta
        .mats
        .iter()
        .map(|&(id, density)| {
            let d = if scale > 1e-6 && scale.is_finite() && density > 0.0 {
                Density::Uv(density / scale)
            } else {
                Density::Unknown
            };
            (id, d)
        })
        .collect();
    Some(Placement { min: min.to_array(), max: max.to_array(), mats })
}

pub(crate) fn model_meta(parts: &[ModelPart]) -> Option<ModelMeta> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut mats: Vec<(MaterialId, f32)> = Vec::new();
    for part in parts {
        for v in &part.vertices {
            let p = Vec3::from(v.pos);
            if p.is_finite() {
                min = min.min(p);
                max = max.max(p);
            }
        }
        let density = uv_density(part);
        match mats.iter_mut().find(|(id, _)| *id == part.material) {
            Some(slot) => slot.1 = slot.1.min(density),
            None => mats.push((part.material, density)),
        }
    }
    (min.x <= max.x).then(|| ModelMeta { min: min.to_array(), max: max.to_array(), mats })
}

pub(crate) fn uv_density(part: &ModelPart) -> f32 {
    let mut tris: Vec<(f32, f32)> = Vec::with_capacity(part.indices.len() / 3);
    let mut total = 0.0f32;
    for t in part.indices.chunks_exact(3) {
        let v = |i: u32| part.vertices.get(i as usize);
        let (Some(a), Some(b), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) else { continue };
        let e1 = Vec3::from(b.pos) - Vec3::from(a.pos);
        let e2 = Vec3::from(c.pos) - Vec3::from(a.pos);
        let n = e1.cross(e2);
        let area = n.length() * 0.5;
        let len1 = e1.length();
        if !(area.is_finite() && area > 1e-8 && len1 > 1e-8) {
            continue;
        }
        let x = e1 / len1;
        let y = n.cross(e1).normalize_or_zero();
        let (ex, ey) = (e2.dot(x), e2.dot(y));
        let det = len1 * ey;
        if det.abs() <= 1e-12 {
            continue;
        }
        let ua = glam::Vec2::from(a.uv);
        let (d1, d2) = (glam::Vec2::from(b.uv) - ua, glam::Vec2::from(c.uv) - ua);
        let p = d1.x * ey / det;
        let q = (d2.x * len1 - d1.x * ex) / det;
        let r = (d1.y * ey) / det;
        let w = (d2.y * len1 - d1.y * ex) / det;
        let trace = p * p + q * q + r * r + w * w;
        let cross = p * w - q * r;
        let sigma = ((trace - (trace * trace - 4.0 * cross * cross).max(0.0).sqrt()) * 0.5).max(0.0).sqrt();
        if sigma.is_finite() {
            tris.push((sigma, area));
            total += area;
        }
    }
    if tris.is_empty() || total <= 0.0 {
        return 0.0;
    }
    let floor = total / tris.len() as f32 * DENSITY_FLOOR;
    let least = tris.iter().filter(|(_, area)| *area >= floor).map(|(sigma, _)| *sigma).fold(f32::INFINITY, f32::min);
    if least.is_finite() { least } else { 0.0 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CamKind {
    Persp { eye: Vec3, px: f32 },
    Ortho { axes: (usize, usize), lo: [f32; 2], hi: [f32; 2], px: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewCam {
    kind: CamKind,
    frame: u64,
}

fn samples_textures(mode: RenderMode) -> bool {
    matches!(mode, RenderMode::Textured | RenderMode::TexturedWire | RenderMode::Lit | RenderMode::Unshaded)
}

pub(crate) fn view_cam(params: &ViewParams, frame: u64) -> Option<ViewCam> {
    if !samples_textures(params.mode) {
        return None;
    }
    let [w, h] = params.size;
    if w == 0 || h == 0 {
        return None;
    }
    let kind = match params.kind {
        ViewKind::Perspective { eye, fov_y_deg, .. } => {
            let eye = Vec3::from(eye);
            let fov = fov_y_deg.clamp(1.0, 179.0).to_radians();
            let px = 2.0 * (fov * 0.5).tan() / (h as f32).max(MIN_VIEW_HEIGHT);
            if !eye.is_finite() || !px.is_finite() || px <= 0.0 {
                return None;
            }
            CamKind::Persp { eye, px }
        }
        ViewKind::Ortho { axis, center, pixels_per_unit } => {
            if !(pixels_per_unit.is_finite() && pixels_per_unit > 0.0 && center.iter().all(|c| c.is_finite())) {
                return None;
            }
            let axes = match axis {
                OrthoAxis::Top => (0, 1),
                OrthoAxis::Front => (1, 2),
                OrthoAxis::Side => (0, 2),
            };
            let half = [w as f32 * 0.5 / pixels_per_unit, h as f32 * 0.5 / pixels_per_unit];
            let c = [center[axes.0], center[axes.1]];
            CamKind::Ortho {
                axes,
                lo: [c[0] - half[0], c[1] - half[1]],
                hi: [c[0] + half[0], c[1] + half[1]],
                px: 1.0 / pixels_per_unit,
            }
        }
    };
    Some(ViewCam { kind, frame })
}

impl ViewCam {
    fn footprint(&self, min: &[f32; 3], max: &[f32; 3]) -> Option<f32> {
        match self.kind {
            CamKind::Persp { eye, px } => Some(box_distance(eye, min, max) * px),
            CamKind::Ortho { axes, lo, hi, px } => {
                let inside =
                    min[axes.0] <= hi[0] && max[axes.0] >= lo[0] && min[axes.1] <= hi[1] && max[axes.1] >= lo[1];
                inside.then_some(px)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Need {
    pub k: f32,
    pub dim: f32,
    pub full: bool,
    pub placed: bool,
}

impl Default for Need {
    fn default() -> Self {
        Self { k: f32::INFINITY, dim: 0.0, full: false, placed: false }
    }
}

impl Need {
    fn add(&mut self, footprint: Option<f32>, density: Density) {
        self.placed = true;
        let Some(fp) = footprint else { return };
        match density {
            Density::Scale(s) if s.is_finite() && s > 0.0 => {
                if fp <= 0.0 {
                    self.full = true;
                } else {
                    self.k = self.k.min(fp / s);
                }
            }
            Density::Uv(u) if u.is_finite() && u > 0.0 => {
                if fp <= 0.0 {
                    self.full = true;
                } else {
                    self.dim = self.dim.max(1.0 / (fp * u));
                }
            }
            _ => self.full = true,
        }
    }

    pub(crate) fn cap(&self, full: u32, factor: f32, aggression: f32) -> u32 {
        let full = full.max(1);
        if self.full || !self.placed {
            return full;
        }
        let factor = if factor.is_finite() { factor.max(1.0) } else { return full };
        let mut want = MIN_DIM.min(full);
        if self.k.is_finite() {
            let ratio = self.k * aggression / (MARGIN * factor);
            let drop = if ratio > 1.0 { ratio.log2().floor() as u32 } else { 0 };
            want = want.max(full.checked_shr(drop).unwrap_or(0).max(1));
        }
        if self.dim > 0.0 {
            let d = (self.dim * MARGIN * factor / aggression).ceil();
            if !d.is_finite() || d >= full as f32 {
                return full;
            }
            want = want.max((d as u32).max(1).next_power_of_two());
        }
        want.min(full)
    }
}

fn box_distance(eye: Vec3, min: &[f32; 3], max: &[f32; 3]) -> f32 {
    let lo = Vec3::from(*min);
    let hi = Vec3::from(*max);
    (lo - eye).max(eye - hi).max(Vec3::ZERO).length()
}

pub(crate) fn needs(
    placements: &ScenePlacements,
    views: &[ViewCam],
    count: usize,
    sky: Option<&SkyFrame>,
) -> Vec<Need> {
    let mut out = vec![Need::default(); count + 1];
    for (object, p) in placements.all() {
        let in_sky = sky.filter(|s| s.objects.contains(&object));
        let fp = views
            .iter()
            .flat_map(|v| {
                let sky_fp = match (in_sky, v.kind) {
                    (Some(s), CamKind::Persp { eye, px }) => Some(box_distance(s.eye(eye), &p.min, &p.max) * px),
                    _ => None,
                };
                [v.footprint(&p.min, &p.max), sky_fp]
            })
            .flatten()
            .fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.min(b))));
        for &(id, density) in &p.mats {
            if let Some(n) = out.get_mut(id as usize) {
                n.add(fp, density);
            }
        }
    }
    out
}

pub(crate) const PROBE_RADIUS: f32 = 4096.0;
const PROBE_CELL: f32 = 1024.0;
const LARGE_CELLS: i64 = 64;

pub(crate) fn probe_needs(placements: &ScenePlacements, probes: &[([f32; 3], u32)], count: usize) -> Vec<Need> {
    let mut out = vec![Need::default(); count + 1];
    if probes.is_empty() {
        return out;
    }
    let all: Vec<&Placement> = placements.all().map(|(_, p)| p).collect();
    let cell = |v: f32| (v / PROBE_CELL).floor() as i64;
    let mut grid: AHashMap<(i64, i64), Vec<u32>> = AHashMap::new();
    let mut large: Vec<u32> = Vec::new();
    for (i, p) in all.iter().enumerate() {
        let (x0, x1, y0, y1) = (cell(p.min[0]), cell(p.max[0]), cell(p.min[1]), cell(p.max[1]));
        if !(p.min.iter().chain(&p.max).all(|v| v.is_finite())) || (x1 - x0 + 1) * (y1 - y0 + 1) > LARGE_CELLS {
            large.push(i as u32);
            continue;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                grid.entry((x, y)).or_default().push(i as u32);
            }
        }
    }
    let reach = (PROBE_RADIUS / PROBE_CELL).ceil() as i64;
    let mut seen: Vec<u32> = vec![u32::MAX; all.len()];
    for (n, &(origin, size)) in probes.iter().enumerate() {
        let eye = Vec3::from(origin);
        if !eye.is_finite() || size == 0 {
            continue;
        }
        let cam = ViewCam { kind: CamKind::Persp { eye, px: 2.0 / size as f32 }, frame: 0 };
        let (cx, cy) = (cell(origin[0]), cell(origin[1]));
        let mut visit = |i: u32, out: &mut Vec<Need>| {
            let Some(slot) = seen.get_mut(i as usize) else { return };
            if *slot == n as u32 {
                return;
            }
            *slot = n as u32;
            let p = all[i as usize];
            let fp = cam.footprint(&p.min, &p.max);
            if fp.is_some_and(|f| f / cam_px(&cam) <= PROBE_RADIUS) {
                for &(id, density) in &p.mats {
                    if let Some(need) = out.get_mut(id as usize) {
                        need.add(fp, density);
                    }
                }
            }
        };
        for x in cx - reach..=cx + reach {
            for y in cy - reach..=cy + reach {
                if let Some(list) = grid.get(&(x, y)) {
                    for &i in list {
                        visit(i, &mut out);
                    }
                }
            }
        }
        for &i in &large {
            visit(i, &mut out);
        }
    }
    out
}

fn cam_px(cam: &ViewCam) -> f32 {
    match cam.kind {
        CamKind::Persp { px, .. } | CamKind::Ortho { px, .. } => px,
    }
}

pub(crate) fn merge_needs(into: &mut [Need], other: &[Need]) {
    for (a, b) in into.iter_mut().zip(other) {
        a.k = a.k.min(b.k);
        a.dim = a.dim.max(b.dim);
        a.full |= b.full;
        a.placed |= b.placed;
    }
}

pub(crate) fn texture_bytes(bytes: u64, top: u32, dim: u32) -> u64 {
    if top == 0 {
        return bytes;
    }
    let ratio = f64::from(dim) / f64::from(top);
    (bytes as f64 * ratio * ratio) as u64
}

pub(crate) fn aggression_for(budget: u64, mut total_at: impl FnMut(f32) -> u64) -> f32 {
    if budget == 0 {
        return 1.0;
    }
    for a in AGGRESSION_STEPS {
        if total_at(a) <= budget {
            return a;
        }
    }
    AGGRESSION_STEPS[AGGRESSION_STEPS.len() - 1]
}

pub(crate) type ProbeCache = ((u64, u64), std::sync::Arc<Vec<Need>>);

#[derive(Default)]
pub(crate) struct Residency {
    pub views: AHashMap<u32, ViewCam>,
    pub models: AHashMap<ModelId, ModelMeta>,
    pub escaped: AHashMap<MaterialId, u64>,
    pub models_escaped: Option<u64>,
    pub preload_until: Option<Instant>,
    pub budget: u64,
    pub last: Option<Instant>,
    pub aggression: f32,
    pub report: hp_app::runtime::TextureStreaming,
    pub probes: Option<ProbeCache>,
    pub fresh: Option<(Instant, u64)>,
}

impl Residency {
    pub(crate) fn note_view(&mut self, view: u32, params: &ViewParams, frame: u64) {
        match view_cam(params, frame) {
            Some(cam) => {
                self.views.insert(view, cam);
            }
            None => {
                self.views.remove(&view);
            }
        }
    }

    pub(crate) fn live_views(&mut self, frame: u64) -> Vec<ViewCam> {
        self.views.retain(|_, v| frame.saturating_sub(v.frame) <= VIEW_TTL);
        let mut ids: Vec<u32> = self.views.keys().copied().collect();
        ids.sort_unstable();
        ids.iter().filter_map(|id| self.views.get(id).copied()).collect()
    }

    pub(crate) fn escape(&mut self, id: MaterialId, frame: u64) {
        self.escaped.insert(id, frame);
    }

    pub(crate) fn escaped_now(&mut self, frame: u64) -> AHashSet<MaterialId> {
        self.escaped.retain(|_, f| frame.saturating_sub(*f) <= ESCAPE_TTL);
        self.escaped.keys().copied().collect()
    }

    pub(crate) fn prune_models(&mut self, live: &AHashSet<ModelId>) {
        self.models.retain(|id, _| live.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_render::Vertex;

    fn params(size: [u32; 2], kind: ViewKind, mode: RenderMode) -> ViewParams {
        ViewParams {
            size,
            kind,
            mode,
            grid: None,
            background: [0.0; 3],
            selection_color: [0.0; 4],
            show_sprites: true,
            show_models: true,
            msaa: false,
            game_pixel_grid: false,
        }
    }

    fn persp_kind(eye: [f32; 3]) -> ViewKind {
        ViewKind::Perspective {
            eye,
            forward: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 60.0,
            near: 1.0,
            far: 30000.0,
        }
    }

    fn persp(eye: [f32; 3]) -> ViewCam {
        view_cam(&params([1600, 900], persp_kind(eye), RenderMode::Textured), 1).unwrap()
    }

    fn brush(at: f32, id: MaterialId, scale: f32) -> Placement {
        Placement { min: [at, -64.0, -64.0], max: [at + 128.0, 64.0, 64.0], mats: vec![(id, Density::Scale(scale))] }
    }

    #[test]
    fn near_surfaces_keep_full_resolution_and_far_ones_drop_whole_mips() {
        let mut scene = ScenePlacements::default();
        scene.set_object(1, brush(10.0, 1, 0.25));
        scene.set_object(2, brush(20_000.0, 2, 0.25));
        scene.set_object(3, brush(-64.0, 3, 0.25));
        let views = [persp([0.0, 0.0, 0.0])];
        let n = needs(&scene, &views, 3, None);
        assert_eq!(n[3].cap(2048, 1.0, 1.0), 2048, "the camera stands inside the brush");
        assert_eq!(n[1].cap(2048, 1.0, 1.0), 2048, "10 units away a texel is bigger than a pixel");
        let far = n[2].cap(2048, 1.0, 1.0);
        assert!((MIN_DIM..2048).contains(&far), "{far}");
        let px = 2.0 * (30f32.to_radians()).tan() / MIN_VIEW_HEIGHT;
        let k = 20_000.0 * px / 0.25;
        let expected = 2048 >> ((k / MARGIN).log2().floor() as u32);
        assert_eq!(far, expected.max(MIN_DIM));
        assert_eq!(n[2].cap(2048, 64.0, 1.0), 2048, "a dense texture transform keeps the detail");
        assert_eq!(Need::default().cap(512, 1.0, 1.0), 512, "unplaced materials keep everything");
    }

    #[test]
    fn the_sampled_mip_is_never_dropped() {
        for d in [100.0f32, 700.0, 3000.0, 9000.0, 40_000.0] {
            for scale in [0.05f32, 0.25, 1.0, 4.0] {
                let mut scene = ScenePlacements::default();
                scene.set_object(1, brush(d, 1, scale));
                let n = needs(&scene, &[persp([0.0; 3])], 1, None);
                let cap = n[1].cap(2048, 1.0, 1.0);
                let px = 2.0 * (30f32.to_radians()).tan() / 900.0 * d;
                let texels_per_pixel = px / scale;
                let sampled = 2048.0 / texels_per_pixel.max(1.0);
                assert!(cap as f32 >= (sampled * 1.999).min(2048.0) || cap == 2048, "d {d} scale {scale} cap {cap}");
            }
        }
    }

    #[test]
    fn unknown_and_untextured_views_keep_full_quality() {
        let mut scene = ScenePlacements::default();
        scene.set_object(1, Placement { min: [9000.0; 3], max: [9100.0; 3], mats: vec![(1, Density::Unknown)] });
        let n = needs(&scene, &[persp([0.0; 3])], 1, None);
        assert_eq!(n[1].cap(1024, 1.0, 1.0), 1024);
        assert!(view_cam(&params([100, 100], persp_kind([0.0; 3]), RenderMode::Wireframe), 0).is_none());
    }

    #[test]
    fn ortho_views_only_see_their_rectangle() {
        let kind = ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0, 0.0, 0.0], pixels_per_unit: 0.5 };
        let cam = view_cam(&params([800, 600], kind, RenderMode::Textured), 1).unwrap();
        assert_eq!(cam.footprint(&[0.0; 3], &[10.0; 3]), Some(2.0));
        assert_eq!(cam.footprint(&[5000.0, 0.0, 0.0], &[5010.0, 10.0, 10.0]), None);
        let mut scene = ScenePlacements::default();
        scene.set_object(1, brush(5000.0, 1, 0.25));
        let n = needs(&scene, &[cam], 1, None);
        assert!(n[1].placed);
        assert_eq!(n[1].cap(1024, 1.0, 1.0), MIN_DIM, "out of every view: smallest size");
    }

    fn quad(size: f32, uv: f32, material: MaterialId) -> ModelPart {
        let v = |x: f32, y: f32| Vertex { pos: [x * size, y * size, 0.0], uv: [x * uv, y * uv], ..Default::default() };
        ModelPart {
            material,
            vertices: vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            tangents: Vec::new(),
        }
    }

    #[test]
    fn model_density_follows_texture_coordinates_and_instance_scale() {
        let meta = model_meta(&[quad(64.0, 1.0, 7), quad(8.0, 1.0, 8)]).unwrap();
        assert_eq!(meta.min, [0.0; 3]);
        assert_eq!(meta.max, [64.0, 64.0, 0.0]);
        let d7 = meta.mats.iter().find(|m| m.0 == 7).unwrap().1;
        assert!((d7 - 1.0 / 64.0).abs() < 1e-6, "{d7}");
        let mut stretched = quad(64.0, 1.0, 9);
        for v in &mut stretched.vertices {
            v.uv[0] *= 4.0;
        }
        let d9 = uv_density(&stretched);
        assert!((d9 - 1.0 / 64.0).abs() < 1e-6, "the most magnified direction counts: {d9}");
        let mut detailed = quad(64.0, 1.0, 10);
        detailed.vertices.extend([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]].map(|[x, y]| hp_render::Vertex {
            pos: [x * 2.0, y * 2.0, 1.0],
            uv: [x, y],
            ..Default::default()
        }));
        detailed.indices.extend([4, 5, 6]);
        assert!((uv_density(&detailed) - 1.0 / 64.0).abs() < 1e-6, "the most magnified triangle counts");
        let mut flat = quad(64.0, 1.0, 11);
        let first = flat.vertices[0].uv;
        for v in &mut flat.vertices {
            v.uv = first;
        }
        assert_eq!(uv_density(&flat), 0.0, "a single texel stretched over a face needs the full texture");
        let mut scene = ScenePlacements::default();
        let twice = glam::Mat4::from_scale_rotation_translation(
            Vec3::splat(2.0),
            glam::Quat::IDENTITY,
            Vec3::new(5000.0, 0.0, 0.0),
        )
        .to_cols_array_2d();
        let identity = glam::Mat4::IDENTITY.to_cols_array_2d();
        scene.set_instance_metas([(1, Some(&meta), &twice), (2, None, &identity)].into_iter());
        assert!(scene.unknown_models);
        assert_eq!(scene.instances.len(), 1);
        let p = &scene.instances[0].1;
        assert_eq!(p.min, [5000.0, 0.0, 0.0]);
        assert_eq!(p.max, [5128.0, 128.0, 0.0]);
        let Density::Uv(u) = p.mats[0].1 else { panic!() };
        assert!((u - 1.0 / 128.0).abs() < 1e-6);
        let n = needs(&scene, &[persp([0.0; 3])], 8, None);
        let cap = n[7].cap(2048, 1.0, 1.0);
        assert!(cap < 2048, "{cap}");
        assert_eq!(n[7].cap(2048, 1.0, 1e6), MIN_DIM);
    }

    #[test]
    fn the_3d_skybox_counts_where_it_is_drawn() {
        let mut scene = ScenePlacements::default();
        scene.set_object(5, brush(-12_000.0, 1, 0.25));
        scene.set_object(6, brush(-12_000.0, 2, 0.25));
        let views = [persp([0.0; 3])];
        let plain = needs(&scene, &views, 2, None);
        assert!(plain[1].cap(2048, 1.0, 1.0) < 2048, "far from the camera");
        let mut objects = AHashSet::new();
        objects.insert(5);
        let sky = SkyFrame { origin: Vec3::new(-11_950.0, 0.0, 0.0), inv_scale: 1.0 / 16.0, objects };
        let n = needs(&scene, &views, 2, Some(&sky));
        assert_eq!(n[1].cap(2048, 1.0, 1.0), 2048, "the skybox brush is drawn right around the camera");
        assert_eq!(n[2].cap(2048, 1.0, 1.0), plain[2].cap(2048, 1.0, 1.0), "not in the skybox");
    }

    #[test]
    fn cubemap_probes_keep_the_detail_their_faces_sample() {
        let mut scene = ScenePlacements::default();
        scene.set_object(1, brush(20_000.0, 1, 0.25));
        scene.set_object(2, brush(40_000.0, 2, 0.25));
        let probes = [([20_100.0, 0.0, 0.0], 128)];
        let probe = probe_needs(&scene, &probes, 2);
        assert!(probe[1].full, "the probe stands inside the first brush");
        assert!(!probe[2].placed, "the second brush is beyond the probe radius");
        let mut n = needs(&scene, &[persp([0.0; 3])], 2, None);
        let far = n[1].cap(2048, 1.0, 1.0);
        assert!(far < 2048);
        merge_needs(&mut n, &probe);
        assert_eq!(n[1].cap(2048, 1.0, 1.0), 2048);
        assert_eq!(n[2].cap(2048, 1.0, 1.0), needs(&scene, &[persp([0.0; 3])], 2, None)[2].cap(2048, 1.0, 1.0));
        let wide =
            Placement { min: [-1.0e5, -1.0e5, 0.0], max: [1.0e5, 1.0e5, 8.0], mats: vec![(1, Density::Scale(4.0))] };
        scene.set_object(3, wide);
        assert!(probe_needs(&scene, &probes, 2)[1].placed, "huge placements are always checked");
    }

    #[test]
    fn budgets_pick_the_mildest_reduction_that_fits() {
        assert_eq!(aggression_for(0, |_| u64::MAX), 1.0);
        assert_eq!(aggression_for(100, |a| (400.0 / a) as u64), 4.0);
        assert_eq!(aggression_for(1, |_| 50), 16.0);
        assert_eq!(texture_bytes(1000, 1024, 512), 250);
        assert_eq!(texture_bytes(250, 512, 1024), 1000, "a reduced texture estimates its full size");
        assert_eq!(texture_bytes(1000, 0, 512), 1000);
    }

    #[test]
    fn placements_track_revisions_and_views_expire() {
        let mut scene = ScenePlacements::default();
        let r0 = scene.revision;
        scene.set_object(1, brush(0.0, 1, 0.25));
        let r1 = scene.revision;
        assert_ne!(r0, r1);
        scene.set_object(1, brush(0.0, 1, 0.25));
        assert_eq!(scene.revision, r1, "identical placements do not trigger an update");
        scene.forget(1);
        assert!(scene.objects.is_empty());
        let mut r = Residency::default();
        r.note_view(0, &params([10, 10], persp_kind([0.0; 3]), RenderMode::Textured), 5);
        assert_eq!(r.live_views(5).len(), 1);
        assert!(r.live_views(5 + VIEW_TTL + 1).is_empty());
        r.escape(4, 10);
        assert!(r.escaped_now(10 + ESCAPE_TTL).contains(&4));
        assert!(r.escaped_now(11 + ESCAPE_TTL).is_empty());
    }
}
