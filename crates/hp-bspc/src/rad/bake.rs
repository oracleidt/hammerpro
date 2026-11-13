pub(super) mod incremental;
pub(super) mod styles;

use glam::{DVec2, Vec3A};
use hp_light::bvh::{ALL_CLASSES, Bvh};
use rayon::prelude::*;

use super::RadQuality;
use super::data::MapData;
use super::lights::{Kind, Lights};
use super::surface::{LitFace, OPAQUE_SURFACES, SurfTri, bump_normals, inside};

pub(crate) const MAX_TRACE: f32 = 56_755.84;
const EQUAL_EPSILON: f32 = 0.001;
const DIST_EPSILON: f32 = 0.031_25;
const SUPERSAMPLE_GRADIENT: f32 = 0.0625;
const SUPERSAMPLE_ORDER: [(usize, usize); 25] = {
    let mut out = [(0usize, 0usize); 25];
    (out[0], out[1], out[2], out[3], out[4]) = ((0, 0), (4, 0), (0, 4), (4, 4), (2, 2));
    let mut k = PROBES;
    let mut i = 0;
    while i < 25 {
        let (x, y) = (i % 5, i / 5);
        let probe = (x == 0 || x == 4) && (y == 0 || y == 4) || (x == 2 && y == 2);
        if !probe {
            out[k] = (x, y);
            k += 1;
        }
        i += 1;
    }
    out
};
const PROBES: usize = 5;
const BOUNCE_TAIL: f32 = 0.25;
const SUN_PROBES: usize = 6;
const SUN_STRIDE: usize = 7;
const MAX_STYLES: usize = 4;
const MAX_CANDIDATE_STYLES: usize = 16;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Params {
    pub sky_dirs: usize,
    pub sun_samples: usize,
    pub supersample_passes: usize,
    pub bounce_step: usize,
    pub bounce_rays: usize,
    pub leaf_samples: usize,
    pub leaf_dirs: usize,
    pub style_bounce: bool,
}

impl Params {
    pub fn at_least(self, floor: &Params) -> Params {
        Params {
            sky_dirs: self.sky_dirs.max(floor.sky_dirs),
            sun_samples: self.sun_samples.max(floor.sun_samples),
            supersample_passes: self.supersample_passes,
            bounce_step: self.bounce_step.min(floor.bounce_step),
            bounce_rays: self.bounce_rays.max(floor.bounce_rays),
            leaf_samples: self.leaf_samples,
            leaf_dirs: self.leaf_dirs,
            style_bounce: self.style_bounce,
        }
    }

    pub fn of(q: RadQuality) -> Params {
        match q {
            RadQuality::Draft => Params {
                sky_dirs: 48,
                sun_samples: 4,
                supersample_passes: 1,
                bounce_step: 4,
                bounce_rays: 32,
                leaf_samples: 1,
                leaf_dirs: 64,
                style_bounce: true,
            },
            RadQuality::Normal => Params {
                sky_dirs: 162,
                sun_samples: 16,
                supersample_passes: 2,
                bounce_step: 2,
                bounce_rays: 64,
                leaf_samples: 128,
                leaf_dirs: 162,
                style_bounce: true,
            },
            RadQuality::Final => Params {
                sky_dirs: 512,
                sun_samples: 30,
                supersample_passes: 4,
                bounce_step: 1,
                bounce_rays: 128,
                leaf_samples: 128,
                leaf_dirs: 162,
                style_bounce: true,
            },
        }
    }
}

pub(crate) fn hash(a: u32, b: u32, c: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77) ^ c.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

pub(crate) fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

pub(crate) fn sphere_dirs(n: usize) -> Vec<Vec3A> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..n)
        .map(|i| {
            let z = 1.0 - (i as f32 + 0.5) * 2.0 / n as f32;
            let r = (1.0 - z * z).max(0.0).sqrt();
            let phi = golden * i as f32;
            Vec3A::new(r * phi.cos(), r * phi.sin(), z)
        })
        .collect()
}

fn basis(n: Vec3A) -> (Vec3A, Vec3A) {
    let a = if n.x.abs() > 0.9 { Vec3A::Y } else { Vec3A::X };
    let t = n.cross(a).normalize_or(Vec3A::Y);
    (t, n.cross(t))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub pos: Vec3A,
    pub normals: [Vec3A; 4],
    pub cluster: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SurfHit {
    None,
    Sky,
    Face(u32, f32, f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Part {
    All,
    Direct,
    Ambient,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FaceLight {
    pub styles: Vec<u8>,
    pub normals: usize,
    pub values: Vec<glam::Vec3>,
    pub dark: Vec<bool>,
}

impl FaceLight {
    pub fn index(&self, pass: usize, slot: usize, n: usize, luxels: usize) -> usize {
        ((pass * self.styles.len() + slot) * self.normals + n) * luxels
    }
}

pub(crate) struct Ctx<'a> {
    pub map: &'a MapData,
    pub faces: &'a [LitFace],
    pub shadow: &'a Bvh,
    pub surf: &'a Bvh,
    pub surf_info: &'a [SurfTri],
    pub lights: &'a Lights,
    pub passes: usize,
    pub params: Params,
    pub light_rows: Vec<Option<Vec<u8>>>,
    pub sky_dirs: Vec<Vec3A>,
    pub sun_dirs: Vec<Vec3A>,
    pub cameras: Vec<(Vec3A, f32)>,
    pub camera_areas: Vec<bool>,
    pub props: &'a super::props::Occluders,
    pub blockers: &'a Bvh,
    pub work: &'a super::progress::Work,
    pub bounce_budget: usize,
    pub bounce_tile_budget: usize,
    pub water: &'a Bvh,
    pub water_reflectivity: Vec3A,
    pub dispchop: f64,
}

pub(crate) const WATER_BIT: u32 = 1 << 31;
const CONTENTS_WATER: i32 = 0x20;

pub(crate) fn split_hit(hi: u32) -> (usize, bool) {
    if hi == u32::MAX { (usize::MAX, false) } else { ((hi & !WATER_BIT) as usize, hi & WATER_BIT != 0) }
}

pub(crate) fn sky_cameras(map: &MapData) -> (Vec<(Vec3A, f32)>, Vec<bool>) {
    let mut cams = Vec::new();
    let mut areas = Vec::new();
    for e in &map.entities {
        if !e.classname().eq_ignore_ascii_case("sky_camera") {
            continue;
        }
        let Some(o) = e.origin() else { continue };
        let origin = glam::DVec3::from_array(o);
        let scale = e.get("scale").and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(0.0);
        if !(scale > 0.0 && scale.is_finite()) {
            continue;
        }
        if let Some(a) = map.area_at(origin) {
            if areas.len() <= a {
                areas.resize(a + 1, false);
            }
            areas[a] = true;
        }
        cams.push((origin.as_vec3a(), 1.0 / scale));
    }
    (cams, areas)
}

pub(crate) fn row_sees(row: Option<&Vec<u8>>, cluster: i32) -> bool {
    match row {
        Some(r) if cluster >= 0 => r.get(cluster as usize >> 3).is_none_or(|b| b & (1 << (cluster & 7)) != 0),
        _ => true,
    }
}

impl Ctx<'_> {
    pub fn rows(map: &MapData, lights: &Lights, sky_clusters: &[i32]) -> Vec<Option<Vec<u8>>> {
        lights
            .list
            .iter()
            .map(|l| {
                if map.vis.clusters == 0 {
                    return None;
                }
                let mut row = map.vis.row(l.cluster)?.to_vec();
                if matches!(l.kind, Kind::Sun | Kind::SkyAmbient) {
                    for c in sky_clusters {
                        if let Some(r) = map.vis.row(*c) {
                            for (a, b) in row.iter_mut().zip(r) {
                                *a |= *b;
                            }
                        }
                    }
                }
                Some(row)
            })
            .collect()
    }

    pub fn sees_sky(&self, from: Vec3A, dir: Vec3A) -> f32 {
        self.sees_sky_skip(from, dir, super::props::NO_PROP)
    }

    pub fn sees_sky_skip(&self, from: Vec3A, dir: Vec3A, skip: u32) -> f32 {
        let mut occlusion = 1.0 - self.props.sky_visible(self.shadow, from, dir, skip);
        if occlusion >= 1.0 {
            return 0.0;
        }
        if self.cameras.is_empty() {
            return 1.0 - occlusion;
        }
        thread_local! {
            static AREA: std::cell::Cell<([u32; 3], Option<usize>)> = const { std::cell::Cell::new(([u32::MAX; 3], None)) };
        }
        let key = [from.x.to_bits(), from.y.to_bits(), from.z.to_bits()];
        let area = match AREA.with(std::cell::Cell::get) {
            (k, a) if k == key => a,
            _ => {
                let a = self.map.area_at(from.as_dvec3());
                AREA.with(|c| c.set((key, a)));
                a
            }
        };
        if area.is_none_or(|a| self.camera_areas.get(a).copied().unwrap_or(false)) {
            return 1.0 - occlusion;
        }
        for (origin, to_sky) in &self.cameras {
            occlusion += 1.0 - self.props.sky_visible(self.shadow, *origin + from * *to_sky, dir, skip);
        }
        (1.0 - occlusion).clamp(0.0, 1.0)
    }

    pub fn trace_surface(&self, from: Vec3A, dir: Vec3A) -> SurfHit {
        self.trace_surface_with(from, dir, false)
    }

    pub fn trace_surface_with(&self, from: Vec3A, dir: Vec3A, props_block: bool) -> SurfHit {
        self.trace_surface_if(from, dir, &|t| t > 1.0 && self.blocked(from, dir, t - 0.5, props_block))
    }

    fn blocked(&self, from: Vec3A, dir: Vec3A, dist: f32, props_block: bool) -> bool {
        use super::props::{CLASS_ALPHA, CLASS_PROP};
        self.blockers.occluded(from, dir, dist, |_| true)
            || (props_block && self.shadow.occluded_in(CLASS_PROP | CLASS_ALPHA, from, dir, dist, |_| true))
    }

    pub fn trace_sky_kind(&self, from: Vec3A, dir: Vec3A) -> Option<bool> {
        let h = self.surf.closest_in(OPAQUE_SURFACES, from, dir, MAX_TRACE, |_| true)?;
        if h.t > 1.0 && self.blocked(from, dir, h.t - 0.5, true) {
            return None;
        }
        match self.surf_info.get(h.payload as usize) {
            Some(SurfTri::Sky) => Some(false),
            Some(SurfTri::Sky2D) => Some(true),
            _ => None,
        }
    }

    pub fn trace_surface_if(&self, from: Vec3A, dir: Vec3A, blocked: &dyn Fn(f32) -> bool) -> SurfHit {
        self.trace_surface_in(ALL_CLASSES, from, dir, blocked)
    }

    fn trace_bounce(&self, from: Vec3A, dir: Vec3A, props_block: bool) -> SurfHit {
        let blocked = |t: f32| t > 1.0 && self.blocked(from, dir, t - 0.5, props_block);
        self.trace_surface_in(OPAQUE_SURFACES, from, dir, &blocked)
    }

    fn trace_surface_in(&self, mask: u8, from: Vec3A, dir: Vec3A, blocked: &dyn Fn(f32) -> bool) -> SurfHit {
        let Some(h) = self.surf.closest_in(mask, from, dir, MAX_TRACE, |_| true) else { return SurfHit::None };
        if blocked(h.t) {
            return SurfHit::None;
        }
        match self.surf_info.get(h.payload as usize) {
            Some(SurfTri::Sky | SurfTri::Sky2D) => SurfHit::Sky,
            Some(SurfTri::Flat(i)) => {
                let Some(f) = self.faces.get(*i as usize) else { return SurfHit::None };
                if dir.dot(f.normal.as_vec3a()) >= 0.0 {
                    return SurfHit::None;
                }
                let p = (from + dir * h.t).as_dvec3();
                let c = f.luxel_coords(p);
                SurfHit::Face(*i, c.x as f32, c.y as f32, h.t)
            }
            Some(SurfTri::Disp(i, uv)) => {
                if !h.front {
                    return SurfHit::None;
                }
                let Some(f) = self.faces.get(*i as usize) else { return SurfHit::None };
                let w0 = 1.0 - h.u - h.v;
                let u = uv[0][0] * w0 + uv[1][0] * h.u + uv[2][0] * h.v;
                let v = uv[0][1] * w0 + uv[1][1] * h.u + uv[2][1] * h.v;
                SurfHit::Face(*i, u * (f.w as f32 - 1.0), v * (f.h as f32 - 1.0), h.t)
            }
            _ => SurfHit::None,
        }
    }

    pub fn sample(&self, f: &LitFace, s: f64, t: f64) -> Sample {
        let (pos, normal) = match &f.disp {
            Some(d) => {
                let (p, n) = d.sample(disp_uv(f, s, t), 1.0, f.normal);
                (p + f.normal, n)
            }
            None => {
                let p = f.plane_point(s, t);
                (p + f.normal, f.phong(p))
            }
        };
        let pos = if pos.is_finite() && pos.abs().max_element() < 1.0e6 {
            pos
        } else {
            f.poly.iter().copied().sum::<glam::DVec3>() / f.poly.len().max(1) as f64 + f.normal
        };
        let mut normals = [normal.as_vec3a(); 4];
        if f.bump {
            let b = bump_normals(f.tex[0], f.tex[1], f.normal, normal);
            for k in 0..3 {
                normals[k + 1] = b[k].as_vec3a();
            }
        }
        Sample { pos: pos.as_vec3a(), normals, cluster: self.map.vis_cluster_at(pos) }
    }

    fn candidates(&self, f: &LitFace, clusters: &[i32]) -> (Vec<(usize, usize)>, Vec<u8>) {
        let (lo, hi) = (f.bounds.0.as_vec3a(), f.bounds.1.as_vec3a());
        let d = f.poly.first().map_or(0.0, |p| p.dot(f.normal));
        let mut styles = vec![0u8];
        let mut out = Vec::new();
        for (i, l) in self.lights.list.iter().enumerate() {
            match l.kind {
                Kind::Point | Kind::Spot { .. } | Kind::Surface => {
                    let closest = l.origin.clamp(lo, hi);
                    if closest.distance(l.origin) > l.reach {
                        continue;
                    }
                    if l.kind == Kind::Surface {
                        let (a, b) = ((lo - 1.0 - l.origin) * l.normal, (hi + 1.0 - l.origin) * l.normal);
                        if a.max(b).element_sum() <= 0.0 {
                            continue;
                        }
                    }
                    if f.disp.is_none() && f.vnormals.is_none() && l.origin.as_dvec3().dot(f.normal) - d <= 1.0 {
                        continue;
                    }
                }
                Kind::Sun | Kind::SkyAmbient => {}
            }
            let row = self.light_rows.get(i).and_then(Option::as_ref);
            if !clusters.is_empty() && !clusters.iter().any(|c| row_sees(row, *c)) {
                continue;
            }
            let slot = match styles.iter().position(|s| *s == l.style) {
                Some(k) => k,
                None if styles.len() < MAX_CANDIDATE_STYLES => {
                    styles.push(l.style);
                    styles.len() - 1
                }
                None => continue,
            };
            out.push((i, slot));
        }
        (out, styles)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn gather(
        &self,
        cand: &[(usize, usize)],
        slots: usize,
        nn: usize,
        sp: &Sample,
        part: Part,
        out: &mut [Vec3A],
    ) {
        self.gather_sun(cand, slots, nn, sp, part, (0, 1), out);
    }

    #[allow(clippy::too_many_arguments)]
    fn gather_sun(
        &self,
        cand: &[(usize, usize)],
        slots: usize,
        nn: usize,
        sp: &Sample,
        part: Part,
        sun: (usize, usize),
        out: &mut [Vec3A],
    ) {
        let n0 = sp.normals[0];
        for &(li, slot) in cand {
            let l = &self.lights.list[li];
            let ambient = l.kind == Kind::SkyAmbient;
            if (part == Part::Direct && ambient) || (part == Part::Ambient && !ambient) {
                continue;
            }
            if !row_sees(self.light_rows.get(li).and_then(Option::as_ref), sp.cluster) {
                continue;
            }
            let mut f = [0.0f32; 4];
            match l.kind {
                Kind::Point | Kind::Spot { .. } => {
                    let delta = l.origin - sp.pos;
                    let dist = delta.length();
                    if dist.is_nan() || dist <= 1e-4 || dist > l.reach {
                        continue;
                    }
                    let dir = delta / dist;
                    let dot = dir.dot(n0);
                    if dot <= 0.0 {
                        continue;
                    }
                    let mut fall = l.falloff(dir, dist);
                    if fall > 0.0 {
                        fall *= self.props.visible(self.shadow, sp.pos, dir, dist, super::props::NO_PROP);
                    }
                    if fall <= 0.0 {
                        continue;
                    }
                    f[0] = dot * fall;
                    for (fv, nrm) in f.iter_mut().zip(&sp.normals).take(nn).skip(1) {
                        *fv = dir.dot(*nrm).max(0.0) * fall;
                    }
                }
                Kind::Surface => {
                    let delta = l.origin - sp.pos;
                    let dist = delta.length();
                    if dist.is_nan() || dist <= 1e-4 || dist > l.reach {
                        continue;
                    }
                    let dir = delta / dist;
                    let dot = dir.dot(n0);
                    if dot <= 0.0 {
                        continue;
                    }
                    let mut fall = l.falloff(dir, dist);
                    if fall <= 0.0 {
                        continue;
                    }
                    let to = l.origin + l.normal * DIST_EPSILON - sp.pos;
                    let len = to.length();
                    if len > 1e-4 {
                        fall *= self.props.visible(self.shadow, sp.pos, to / len, len, super::props::NO_PROP);
                        if fall <= 0.0 {
                            continue;
                        }
                    }
                    f[0] = dot * fall;
                    for (fv, nrm) in f.iter_mut().zip(&sp.normals).take(nn).skip(1) {
                        *fv = dir.dot(*nrm).max(0.0) * fall;
                    }
                }
                Kind::Sun => {
                    let dot = -n0.dot(l.normal);
                    if dot <= 0.0 {
                        continue;
                    }
                    let count = if self.lights.sun_extent > 0.0 { self.sun_dirs.len().max(1) } else { 1 };
                    let (first, step) = if sun.0 < count { (sun.0, sun.1.max(1)) } else { (0, 1) };
                    let (mut seen, mut traced) = (0.0f32, 0u32);
                    let all = step == 1 && first == 0 && count > SUN_PROBES && count % SUN_STRIDE != 0;
                    for (j, k) in (first..count).step_by(step).enumerate() {
                        let k = if all { j * SUN_STRIDE % count } else { k };
                        let mut dir = -l.normal;
                        if k > 0 {
                            dir = (dir + self.sun_dirs[k] * self.lights.sun_extent).normalize_or(dir);
                        }
                        seen += self.sees_sky(sp.pos, dir);
                        traced += 1;
                        if all && traced as usize == SUN_PROBES && (seen == 0.0 || seen == traced as f32) {
                            break;
                        }
                    }
                    let see = seen / traced.max(1) as f32;
                    if see <= 0.0 {
                        continue;
                    }
                    f[0] = dot * see;
                    for (fv, nrm) in f.iter_mut().zip(&sp.normals).take(nn).skip(1) {
                        let c = (-nrm.dot(l.normal)).max(0.0);
                        *fv = (c + c * c) * 0.5 * see;
                    }
                }
                Kind::SkyAmbient => {
                    let mut sum = [0.0f32; 4];
                    let mut hits = [0u32; 4];
                    let mut sumdot = 0.0f32;
                    for d in &self.sky_dirs {
                        let dot0 = n0.dot(*d);
                        if dot0 <= EQUAL_EPSILON {
                            continue;
                        }
                        sumdot += dot0;
                        hits[0] += 1;
                        let mut dots = [dot0, 0.0, 0.0, 0.0];
                        for n in 1..nn {
                            let v = sp.normals[n].dot(*d);
                            if v > EQUAL_EPSILON {
                                dots[n] = v;
                                hits[n] += 1;
                            }
                        }
                        let vis = self.sees_sky(sp.pos, *d);
                        if vis > 0.0 {
                            for n in 0..nn {
                                sum[n] += dots[n] * vis;
                            }
                        }
                    }
                    if hits[0] == 0 || sumdot <= 0.0 {
                        continue;
                    }
                    for n in 0..nn {
                        if hits[n] > 0 {
                            f[n] = sum[n] / (sumdot * hits[n] as f32 / hits[0] as f32);
                        }
                    }
                }
            }
            if f[0] <= 0.0 {
                continue;
            }
            for p in 0..self.passes {
                let i = l.intensity.get(p).copied().unwrap_or(Vec3A::ZERO);
                for n in 0..nn {
                    if f[n] > 0.0 {
                        out[(p * slots + slot) * nn + n] += i * f[n];
                    }
                }
            }
        }
    }

    pub fn direct(&self, f: &LitFace) -> FaceLight {
        let nn = if f.bump { 4 } else { 1 };
        let lux = f.luxels();
        let mut pieces: Vec<Vec<DVec2>> = Vec::new();
        let mut centers: Vec<DVec2> = Vec::with_capacity(lux);
        let mut dark = vec![false; lux];
        for t in 0..f.h {
            for s in 0..f.w {
                if f.disp.is_some() {
                    centers.push(super::disp::sample_coord(f, s, t));
                    pieces.push(Vec::new());
                } else {
                    let (c, piece) = f.flat_sample(s, t);
                    let middle = DVec2::new(s as f64, t as f64);
                    let on_face = f.poly2.len() >= 3 && inside(&f.poly2, middle);
                    let open_middle = if f.poly2.len() < 3 || f.frame.is_none() {
                        false
                    } else if on_face {
                        !self.in_brush(f.plane_point(middle.x, middle.y) + f.normal)
                    } else {
                        self.reaches_any(f, c, middle)
                    };
                    if f.poly2.len() < 3 || f.frame.is_none() {
                        centers.push(c);
                    } else if open_middle {
                        centers.push(middle);
                    } else if !piece.is_empty() {
                        match self.open_point(f, c, &piece) {
                            Some(p) => centers.push(p),
                            None => {
                                dark[centers.len()] = true;
                                centers.push(c);
                            }
                        }
                    } else {
                        dark[centers.len()] = true;
                        centers.push(c);
                    }
                    pieces.push(piece);
                }
            }
        }
        let samples: Vec<Option<Sample>> =
            (0..lux).map(|i| if dark[i] { None } else { Some(self.sample(f, centers[i].x, centers[i].y)) }).collect();
        let mut clusters: Vec<i32> = samples.iter().flatten().map(|sp| sp.cluster).collect();
        clusters.extend(f.poly.iter().map(|v| self.map.vis_cluster_at(*v + f.normal)));
        clusters.sort_unstable();
        clusters.dedup();
        clusters.retain(|c| *c >= 0);
        let (cand, styles) = self.candidates(f, &clusters);
        let slots = styles.len();
        let stride = self.passes * slots * nn;
        let mut acc = vec![Vec3A::ZERO; stride * lux];
        let sky_lights = super::sky::lights_of(self, &cand);
        let part = if sky_lights.is_empty() { Part::All } else { Part::Direct };
        for (i, sp) in samples.iter().enumerate() {
            if let Some(sp) = sp {
                self.gather(&cand, slots, nn, sp, part, &mut acc[i * stride..(i + 1) * stride]);
            }
        }
        let sky = if sky_lights.is_empty() {
            Vec::new()
        } else {
            super::sky::face(self, &sky_lights, (slots, nn, f.w), &samples, &mut acc)
        };
        if f.disp.is_none() && self.params.supersample_passes > 0 && !cand.is_empty() {
            let moved: Vec<bool> =
                (0..lux).map(|i| centers[i] != DVec2::new((i % f.w) as f64, (i / f.w) as f64)).collect();
            self.supersample(f, &cand, slots, nn, &pieces, &dark, (&sky, &moved), &mut acc);
        }
        let keep: Vec<usize> = (0..slots)
            .filter(|&slot| {
                slot == 0
                    || (0..lux).any(|i| {
                        (0..self.passes)
                            .any(|p| (0..nn).any(|n| acc[i * stride + (p * slots + slot) * nn + n].max_element() > 0.0))
                    })
            })
            .take(MAX_STYLES)
            .collect();
        let kept = keep.len();
        let mut values = vec![glam::Vec3::ZERO; self.passes * kept * nn * lux];
        for p in 0..self.passes {
            for (ks, &slot) in keep.iter().enumerate() {
                for n in 0..nn {
                    let dst = ((p * kept + ks) * nn + n) * lux;
                    let src = (p * slots + slot) * nn + n;
                    for i in 0..lux {
                        values[dst + i] = acc[i * stride + src].into();
                    }
                }
            }
        }
        FaceLight { styles: keep.iter().map(|&s| styles[s]).collect(), normals: nn, values, dark }
    }

    pub(crate) fn in_brush(&self, p: glam::DVec3) -> bool {
        let head = self.map.models.first().map_or(-1, |m| m.head);
        let Some(leaf) = self.map.point_leaf(head, p).and_then(|l| self.map.leafs.get(l)) else { return false };
        if leaf.contents & super::data::CONTENTS_SOLID != 0 {
            return true;
        }
        (leaf.first_brush..leaf.first_brush + leaf.num_brushes).any(|k| {
            let Some(b) = self.map.leafbrushes.get(k).and_then(|b| self.map.brushes.get(usize::from(*b))) else {
                return false;
            };
            if b.contents & super::data::MASK_OPAQUE == 0 || b.num_sides == 0 {
                return false;
            }
            self.map.sides.get(b.first_side..b.first_side + b.num_sides).is_some_and(|sides| {
                sides
                    .iter()
                    .all(|sd| self.map.planes.get(sd.plane).is_some_and(|pl| p.dot(pl.normal) - pl.dist < -0.01))
            })
        })
    }

    fn open_point(&self, f: &LitFace, c: DVec2, piece: &[DVec2]) -> Option<DVec2> {
        let open = |q: DVec2| !self.in_brush(f.plane_point(q.x, q.y) + f.normal);
        if f.frame.is_none() || open(c) {
            return Some(c);
        }
        let (lo, hi) = piece
            .iter()
            .fold((DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)), |(a, b), p| (a.min(*p), b.max(*p)));
        let mut sum = DVec2::ZERO;
        let mut n = 0.0;
        for gy in 0..4 {
            for gx in 0..4 {
                let q = lo + (hi - lo) * DVec2::new((gx as f64 + 0.5) / 4.0, (gy as f64 + 0.5) / 4.0);
                if inside(piece, q) && open(q) {
                    sum += q;
                    n += 1.0;
                }
            }
        }
        (n > 0.0).then(|| sum / n)
    }

    fn reaches(&self, f: &LitFace, from: DVec2, to: DVec2) -> bool {
        let a = f.plane_point(from.x, from.y) + f.normal;
        let b = f.plane_point(to.x, to.y) + f.normal;
        if !(a.is_finite() && b.is_finite()) {
            return false;
        }
        let head = self.map.models.first().map_or(-1, |m| m.head);
        if self
            .map
            .point_leaf(head, b)
            .and_then(|l| self.map.leafs.get(l))
            .is_some_and(|l| l.contents & super::data::CONTENTS_SOLID != 0)
        {
            return false;
        }
        let d = (b - a).as_vec3a();
        let len = d.length();
        if len <= 1e-3 {
            return true;
        }
        use super::props::{CLASS_SKY, CLASS_WORLD};
        !self.shadow.occluded_in(CLASS_WORLD | CLASS_SKY, a.as_vec3a(), d / len, len, |_| true)
    }

    fn reaches_any(&self, f: &LitFace, near: DVec2, to: DVec2) -> bool {
        if self.in_brush(f.plane_point(to.x, to.y) + f.normal) {
            return false;
        }
        if self.reaches(f, near, to) {
            return true;
        }
        let n = f.poly2.len().max(1) as f64;
        let middle = f.poly2.iter().copied().sum::<DVec2>() / n;
        if self.reaches(f, middle, to) {
            return true;
        }
        f.poly2.iter().any(|v| self.reaches(f, *v + (middle - *v) * 0.1, to))
    }

    #[allow(clippy::too_many_arguments)]
    fn supersample(
        &self,
        f: &LitFace,
        cand: &[(usize, usize)],
        slots: usize,
        nn: usize,
        pieces: &[Vec<DVec2>],
        dark: &[bool],
        (sky, moved): (&[Vec3A], &[bool]),
        acc: &mut [Vec3A],
    ) {
        let (w, h) = (f.w, f.h);
        let stride = self.passes * slots * nn;
        let mut done = dark.to_vec();
        let per = slots * nn;
        let intensity = |acc: &[Vec3A], i: usize, k: usize| -> f32 {
            let v = acc[i * stride + k];
            ((v.x + v.y + v.z) / 3.0 / 256.0).max(0.0).powf(1.0 / 2.2)
        };
        let (mut direct, mut tmp, mut sum) =
            (vec![Vec3A::ZERO; stride], vec![Vec3A::ZERO; stride], vec![Vec3A::ZERO; stride]);
        let mut ambient = vec![Vec3A::ZERO; stride];
        let mut probe = vec![Vec3A::ZERO; PROBES * stride];
        let mut points: Vec<(Sample, f32, usize)> = Vec::with_capacity(25);
        let sun_groups = (self.sun_dirs.len() / 4).clamp(1, 4);
        for _pass in 0..self.params.supersample_passes {
            let mut todo = Vec::new();
            for (i, _) in done.iter().enumerate().filter(|(_, d)| !**d) {
                let (s, t) = ((i % w) as i64, (i / w) as i64);
                let mut grad = 0.0f32;
                for k in 0..per {
                    let c = intensity(acc, i, k);
                    for (ds, dt) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                        let (x, y) = (s + ds, t + dt);
                        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                            continue;
                        }
                        grad = grad.max((c - intensity(acc, (y as usize) * w + x as usize, k)).abs());
                    }
                }
                if grad >= SUPERSAMPLE_GRADIENT {
                    todo.push(i);
                }
            }
            if todo.is_empty() {
                break;
            }
            for i in todo {
                done[i] = true;
                let c = DVec2::new((i % w) as f64, (i / w) as f64);
                let piece = &pieces[i];
                let piece_middle = piece.iter().copied().sum::<DVec2>() / piece.len().max(1) as f64;
                points.clear();
                for (k, &(gx, gy)) in SUPERSAMPLE_ORDER.iter().enumerate() {
                    let o = |g: usize| (g as f64 - 2.0) * 0.4;
                    let (off, weight) = (DVec2::new(o(gx), o(gy)), ((1.0 - o(gx).abs()) * (1.0 - o(gy).abs())) as f32);
                    let p = c + off;
                    let keep = piece.is_empty() || {
                        let open = !self.in_brush(f.plane_point(p.x, p.y) + f.normal);
                        open && (inside(&f.poly2, p) || self.reaches(f, piece_middle, p))
                    };
                    if keep {
                        points.push((self.sample(f, p.x, p.y), weight, k));
                    }
                }
                let nd: f32 = points.iter().map(|p| p.1).sum();
                let probes = points.iter().take_while(|p| p.2 < PROBES).count();
                direct.fill(Vec3A::ZERO);
                for one in cand.chunks(1) {
                    if self.lights.list.get(one[0].0).is_none_or(|l| l.kind == Kind::SkyAmbient) {
                        continue;
                    }
                    let light_at = |(sp, _, k): &(Sample, f32, usize), tmp: &mut [Vec3A]| {
                        tmp.fill(Vec3A::ZERO);
                        self.gather_sun(one, slots, nn, sp, Part::Direct, (k % sun_groups, sun_groups), tmp);
                    };
                    let mut probe_weight = 0.0f32;
                    sum.fill(Vec3A::ZERO);
                    for (j, pt) in points[..probes].iter().enumerate() {
                        light_at(pt, &mut tmp);
                        for (a, v) in sum.iter_mut().zip(&tmp) {
                            *a += *v * pt.1;
                        }
                        probe_weight += pt.1;
                        probe[j * stride..(j + 1) * stride].copy_from_slice(&tmp);
                    }
                    if probes >= 3 && super::sky::agree(&probe, stride, &[0, 1, 2, 3, 4][..probes]) {
                        let scale = nd / probe_weight;
                        for (a, v) in direct.iter_mut().zip(&sum) {
                            *a += *v * scale;
                        }
                        continue;
                    }
                    for pt in &points[probes..] {
                        light_at(pt, &mut tmp);
                        for (a, v) in sum.iter_mut().zip(&tmp) {
                            *a += *v * pt.1;
                        }
                    }
                    for (a, v) in direct.iter_mut().zip(&sum) {
                        *a += *v;
                    }
                }
                ambient.fill(Vec3A::ZERO);
                let mut na = 0.0f32;
                let sky_edge = !sky.is_empty()
                    && (moved.get(i) == Some(&true) || {
                        let (s, t) = ((i % w) as i64, (i / w) as i64);
                        (0..per).any(|k| {
                            let c = intensity(sky, i, k);
                            [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)].iter().any(
                                |(ds, dt)| {
                                    let (x, y) = (s + ds, t + dt);
                                    x >= 0
                                        && y >= 0
                                        && x < w as i64
                                        && y < h as i64
                                        && !dark[y as usize * w + x as usize]
                                        && (c - intensity(sky, y as usize * w + x as usize, k)).abs()
                                            >= SUPERSAMPLE_GRADIENT
                                },
                            )
                        })
                    });
                if sky_edge && !piece.is_empty() {
                    for gy in 0..2 {
                        for gx in 0..2 {
                            let p = c + DVec2::new(gx as f64 * 0.5 - 0.25, gy as f64 * 0.5 - 0.25);
                            if !inside(piece, p) || self.in_brush(f.plane_point(p.x, p.y) + f.normal) {
                                continue;
                            }
                            let sp = self.sample(f, p.x, p.y);
                            self.gather(cand, slots, nn, &sp, Part::Ambient, &mut ambient);
                            na += 1.0;
                        }
                    }
                }
                if nd > 0.0 {
                    for k in 0..stride {
                        let sky_k = if na > 0.0 {
                            ambient[k] / na
                        } else {
                            sky.get(i * stride + k).copied().unwrap_or(Vec3A::ZERO)
                        };
                        acc[i * stride + k] = direct[k] / nd + sky_k;
                    }
                }
            }
        }
    }
}

const PATCH_WEIGHTS: [f32; 5] = [0.5, 1.0, 1.0, 1.0, 0.5];
const BLUR_RAYS: f32 = 576.0;

fn blur_radius(rays: usize, step: usize) -> usize {
    let density = rays.max(1) as f32 / (step.max(1) * step.max(1)) as f32;
    ((BLUR_RAYS / density).sqrt().round() as usize).clamp(4, 8)
}

fn blur_weights(points: &[usize], radius: usize) -> Vec<Vec<(usize, f32)>> {
    points
        .iter()
        .map(|&p| {
            let mut row: Vec<(usize, f32)> = points
                .iter()
                .enumerate()
                .filter_map(|(k, &q)| {
                    let d = p.abs_diff(q);
                    (d <= radius).then(|| (k, (radius + 1 - d) as f32))
                })
                .collect();
            let total: f32 = row.iter().map(|r| r.1).sum();
            for r in &mut row {
                r.1 /= total;
            }
            row
        })
        .collect()
}

type BlurRow = Vec<(usize, f32)>;

fn blur_grid(plane: &mut [Vec3A], keep: &[bool], (wx, wy): (&[BlurRow], &[BlurRow]), tmp: &mut Vec<(Vec3A, f32)>) {
    let gw = wx.len();
    if gw == 0 || plane.len() != gw * wy.len() || keep.len() != plane.len() {
        return;
    }
    tmp.clear();
    for (row, kept) in plane.chunks_exact(gw).zip(keep.chunks_exact(gw)) {
        tmp.extend(wx.iter().map(|ws| {
            ws.iter().filter(|&&(k, _)| kept[k]).fold((Vec3A::ZERO, 0.0), |(v, t), &(k, w)| (v + row[k] * w, t + w))
        }));
    }
    for (y, ws) in wy.iter().enumerate() {
        for x in 0..gw {
            let (v, t) = ws
                .iter()
                .fold((Vec3A::ZERO, 0.0), |(v, t), &(k, w)| (v + tmp[k * gw + x].0 * w, t + tmp[k * gw + x].1 * w));
            if t > 0.0 {
                plane[y * gw + x] = v / t;
            }
        }
    }
}

fn patch_light(src: &[glam::Vec3], w: usize, h: usize, g: &Gather, out: &mut [Vec3A]) {
    if let Some(groups) = &g.patches {
        let field = grouped_light(src, groups);
        kernel_light(&field, w, h, g, out);
        return;
    }
    kernel_light(src, w, h, g, out);
}

fn kernel_light(src: &[glam::Vec3], w: usize, h: usize, g: &Gather, out: &mut [Vec3A]) {
    for (gy, &y) in g.ys.iter().enumerate() {
        for (gx, &x) in g.xs.iter().enumerate() {
            let (mut sum, mut total) = (Vec3A::ZERO, 0.0f32);
            for (dy, wy) in PATCH_WEIGHTS.iter().enumerate() {
                let Some(yy) = (y + dy).checked_sub(2).filter(|v| *v < h) else { continue };
                for (dx, wx) in PATCH_WEIGHTS.iter().enumerate() {
                    let Some(xx) = (x + dx).checked_sub(2).filter(|v| *v < w) else { continue };
                    let Some(v) = src.get(yy * w + xx) else { continue };
                    sum += Vec3A::from(*v) * (wx * wy);
                    total += wx * wy;
                }
            }
            if let Some(o) = out.get_mut(gy * g.gw + gx) {
                *o = if total > 0.0 { sum / total } else { Vec3A::ZERO };
            }
        }
    }
}

pub(crate) fn lookup(buf: &[glam::Vec3], w: usize, h: usize, s: f32, t: f32) -> Vec3A {
    if buf.len() < w * h || w == 0 || h == 0 || !(s.is_finite() && t.is_finite()) {
        return Vec3A::ZERO;
    }
    let x = s.clamp(0.0, (w - 1) as f32);
    let y = t.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize| Vec3A::from(buf[y * w + x]);
    let top = at(x0, y0).lerp(at(x1, y0), fx);
    let bottom = at(x0, y1).lerp(at(x1, y1), fx);
    top.lerp(bottom, fy)
}

struct Gather {
    step: usize,
    gw: usize,
    gh: usize,
    xs: Vec<usize>,
    ys: Vec<usize>,
    normals: Vec<[Vec3A; 4]>,
    hits: super::tiles::Hits,
    luxels: usize,
    patches: Option<PatchGroups>,
    buried: Vec<bool>,
}

impl Gather {
    fn keep(&self, f: &LitFace, fl: &FaceLight) -> Vec<bool> {
        let mut k = 0usize;
        let mut out = Vec::with_capacity(self.gw * self.gh);
        for &y in &self.ys {
            for &x in &self.xs {
                let dark = fl.dark.get(y * f.w + x).copied().unwrap_or(false);
                out.push(!dark && !self.buried.get(k).copied().unwrap_or(false));
                k += 1;
            }
        }
        out
    }
}

struct PatchGroups {
    of_luxel: Vec<u32>,
    scale: Vec<f32>,
    members: Vec<Vec<u32>>,
}

const DARK_SAMPLE: f32 = 1.0;
const BURIED_SHARE: usize = 4;

fn grouped_light(src: &[glam::Vec3], groups: &PatchGroups) -> Vec<glam::Vec3> {
    let avg: Vec<Vec3A> = groups
        .members
        .iter()
        .map(|m| {
            let (sum, n) = m
                .iter()
                .filter_map(|&k| src.get(k as usize).map(|v| Vec3A::from(*v)))
                .filter(|v| v.element_sum() / 3.0 >= DARK_SAMPLE)
                .fold((Vec3A::ZERO, 0usize), |(s, n), v| (s + v, n + 1));
            if n > 0 { sum / n as f32 } else { Vec3A::ZERO }
        })
        .collect();
    groups
        .of_luxel
        .iter()
        .map(|&p| {
            let p = p as usize;
            (avg.get(p).copied().unwrap_or(Vec3A::ZERO) * groups.scale.get(p).copied().unwrap_or(1.0)).into()
        })
        .collect()
}

fn axis_points(n: usize, step: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).step_by(step.max(1)).collect();
    if v.last() != Some(&(n - 1)) {
        v.push(n - 1);
    }
    v
}

fn cosine_dir(n: Vec3A, k: usize, count: usize, seed: u32) -> Vec3A {
    let (t, b) = basis(n);
    cosine_dir_in(&[t, b, n], k, count, seed)
}

fn cosine_dir_in(&[t, b, n]: &[Vec3A; 3], k: usize, count: usize, seed: u32) -> Vec3A {
    let side = (count as f32).sqrt().ceil().max(1.0) as usize;
    let (gx, gy) = (k % side, k / side);
    let r1 = unit(hash(seed, k as u32, 1));
    let r2 = unit(hash(seed, k as u32, 2));
    let u1 = ((gx as f32 + r1) / side as f32).min(0.9999);
    let u2 = ((gy as f32 + r2) / (count.div_ceil(side)) as f32).min(0.9999);
    let r = u1.sqrt();
    let phi = std::f32::consts::TAU * u2;
    let z = (1.0 - u1).max(0.0).sqrt();
    (t * (r * phi.cos()) + b * (r * phi.sin()) + n * z).normalize_or(n)
}

const Q: f32 = 32.0;
const PLANE_TEST_EPSILON: f64 = 0.01;

fn disp_uv(f: &LitFace, s: f64, t: f64) -> DVec2 {
    let u = if f.w > 1 { s / (f.w - 1) as f64 } else { 0.5 };
    let v = if f.h > 1 { t / (f.h - 1) as f64 } else { 0.5 };
    DVec2::new(u, v)
}

impl Ctx<'_> {
    fn gather_grid(&self, fi: usize, step: usize, rays: usize, store: &super::tiles::Store) -> Result<Gather, String> {
        let f = &self.faces[fi];
        let xs = axis_points(f.w, step);
        let ys = axis_points(f.h, step);
        let (gw, gh) = (xs.len(), ys.len());
        let mut normals = Vec::with_capacity(gw * gh);
        let mut points = Vec::with_capacity(gw * gh);
        let patches = super::disp::patches(f, self.dispchop);
        for (gy, &t) in ys.iter().enumerate() {
            for (gx, &s) in xs.iter().enumerate() {
                let c = if f.disp.is_some() {
                    DVec2::new(s as f64, t as f64)
                } else {
                    let (c, piece) = f.flat_sample(s, t);
                    if piece.is_empty() { c } else { self.open_point(f, c, &piece).unwrap_or(c) }
                };
                let mut sp = self.sample(f, c.x, c.y);
                let at = super::disp::patch_index(&patches, disp_uv(f, c.x, c.y));
                let (pos, origin) = match at.and_then(|i| patches.get(i)) {
                    Some(patch) => {
                        sp.normals[0] = patch.normal.as_vec3a();
                        ((patch.origin + patch.normal).as_vec3a(), patch.origin)
                    }
                    None => (sp.pos, sp.pos.as_dvec3() - f.normal),
                };
                normals.push(sp.normals);
                let seed = hash(fi as u32, (gy * gw + gx) as u32, 0x5eed);
                let props_block = f.disp.is_some() || !self.props.inside_prop(self.shadow, pos, sp.normals[0]);
                let (t, b) = basis(sp.normals[0]);
                points.push((pos, origin, [t, b, sp.normals[0]], seed, props_block));
            }
        }
        let mut hits = vec![(u32::MAX, 0, 0); points.len() * rays];
        let own_disp = f.disp.is_some();
        let mut lost = vec![0usize; points.len()];
        for k in 0..rays {
            for (p, (pos, origin, frame, seed, props_block)) in points.iter().enumerate() {
                let dir = cosine_dir_in(frame, k, rays, *seed);
                let q = |v: f32| (v.max(0.0) * Q).min(65535.0) as u16;
                let mut hit = self.trace_bounce(*pos, dir, *props_block);
                if !own_disp && matches!(hit, SurfHit::None) && self.lost_ray(*pos, dir) {
                    lost[p] += 1;
                }
                if let SurfHit::Face(i, ..) = hit
                    && !self.patch_sees(fi, own_disp, i as usize, *origin)
                {
                    hit = SurfHit::None;
                }
                if let Some((i, s, t)) = self.water_mirror(*pos, dir, &hit, *props_block) {
                    hits[p * rays + k] = (i | WATER_BIT, q(s), q(t));
                } else if let SurfHit::Face(i, s, t, _) = hit {
                    hits[p * rays + k] = (i, q(s), q(t));
                }
            }
        }
        let groups = (!patches.is_empty()).then(|| {
            let (of_luxel, scale) = super::disp::luxel_patches(f, &patches);
            PatchGroups { of_luxel, scale, members: super::disp::patch_members(f, &patches) }
        });
        Ok(Gather {
            step: step.max(1),
            gw,
            gh,
            xs,
            ys,
            normals,
            hits: store.keep(hits)?,
            luxels: f.luxels(),
            patches: groups,
            buried: lost.iter().map(|&n| n * BURIED_SHARE > rays).collect(),
        })
    }

    fn lost_ray(&self, from: Vec3A, dir: Vec3A) -> bool {
        match self.surf.closest_in(OPAQUE_SURFACES, from, dir, MAX_TRACE, |_| true) {
            None => true,
            Some(h) => matches!(self.surf_info.get(h.payload as usize), Some(SurfTri::Disp(..))) && !h.front,
        }
    }

    fn patch_sees(&self, fi: usize, own_disp: bool, hit: usize, origin: glam::DVec3) -> bool {
        if hit == fi {
            return !own_disp;
        }
        match self.faces.get(hit).and_then(super::disp::root_plane) {
            Some((normal, dist)) => origin.dot(normal) > dist + PLANE_TEST_EPSILON,
            None => true,
        }
    }

    fn water_mirror(&self, pos: Vec3A, dir: Vec3A, hit: &SurfHit, props_block: bool) -> Option<(u32, f32, f32)> {
        if self.water_reflectivity == Vec3A::ZERO {
            return None;
        }
        let far = match hit {
            SurfHit::Face(_, _, _, t) => *t,
            _ => MAX_TRACE,
        };
        let head = self.map.models.first().map_or(-1, |m| m.head);
        let leaf = self.map.point_leaf(head, (pos + dir).as_dvec3()).and_then(|l| self.map.leafs.get(l))?;
        if leaf.contents & CONTENTS_WATER == 0 {
            return None;
        }
        let w = self.water.closest(pos, dir, far, |_| true)?;
        let face = self.map.faces.get(w.payload as usize)?;
        let n = self.map.face_plane(face)?.normal.as_vec3a();
        let at = pos + dir * w.t;
        let r = dir - n * (2.0 * dir.dot(n));
        match self.trace_bounce(at + r * DIST_EPSILON, r, props_block) {
            SurfHit::Face(i, s, t, _) if i & WATER_BIT == 0 => Some((i, s, t)),
            _ => None,
        }
    }

    pub fn bounce(
        &self,
        lights: &mut [FaceLight],
        bounces: u32,
        log: &mut dyn FnMut(&str),
    ) -> Result<(f64, f64), String> {
        if bounces == 0 || self.faces.is_empty() {
            return Ok((0.0, 0.0));
        }
        let rays = self.params.bounce_rays;
        let total: usize = self.faces.iter().map(|f| f.luxels()).sum();
        let mut step = self.params.bounce_step;
        while step < 16 && total / (step * step) * rays * 8 > self.bounce_budget {
            step *= 2;
        }
        let clock = std::time::Instant::now();
        self.work.begin(super::progress::Stage::BounceRays, total as u64);
        let store = super::tiles::Store::new(self.bounce_tile_budget);
        let grids: Vec<Gather> = (0..self.faces.len())
            .into_par_iter()
            .map(|i| self.gather_grid(i, step, rays, &store))
            .inspect(|g| {
                if let Ok(g) = g {
                    self.work.add(g.luxels as u64);
                }
            })
            .collect::<Vec<Result<Gather, String>>>()
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let (memory, disk) = store.usage();
        if disk > 0 {
            log(&format!("bounce ray cache: {} MB in memory, {} MB in temporary tiles", memory >> 20, disk >> 20));
        }
        let points: usize = grids.iter().map(|g| g.gw * g.gh).sum();
        let gather_time = clock.elapsed().as_secs_f64();
        log(&format!("bounce: {points} gather points, {rays} rays each, traced in {} ms", clock.elapsed().as_millis()));
        let clock = std::time::Instant::now();
        let passes = self.passes;
        let normals: Vec<usize> = lights.iter().map(|l| l.normals).collect();
        let mut emit_at = Vec::with_capacity(grids.len());
        let mut size = 0usize;
        for g in &grids {
            emit_at.push(size);
            size += passes * g.gw * g.gh;
        }
        let targets: Vec<Target> = grids
            .iter()
            .zip(self.faces)
            .zip(&emit_at)
            .map(|((g, f), &emit)| Target {
                emit,
                npts: g.gw * g.gh,
                gw: g.gw,
                x: Axis::of(&g.xs, g.step),
                y: Axis::of(&g.ys, g.step),
                reflectivity: f.reflectivity,
            })
            .collect();
        let mut emit: Vec<Vec3A> = vec![Vec3A::ZERO; size];
        {
            let mut parts: Vec<&mut [Vec3A]> = Vec::with_capacity(grids.len());
            let mut rest = emit.as_mut_slice();
            for g in &grids {
                let (head, tail) = rest.split_at_mut(passes * g.gw * g.gh);
                parts.push(head);
                rest = tail;
            }
            let faces = self.faces;
            let lit: &[FaceLight] = lights;
            parts.into_par_iter().enumerate().for_each(|(fi, out)| {
                let (g, f, l) = (&grids[fi], &faces[fi], &lit[fi]);
                let (lux, npts) = (f.luxels(), g.gw * g.gh);
                for p in 0..passes {
                    let at = l.index(p, 0, 0, lux);
                    patch_light(&l.values[at..at + lux], f.w, f.h, g, &mut out[p * npts..(p + 1) * npts]);
                }
            });
        }
        let mut total: Vec<Vec<Vec3A>> =
            grids.iter().zip(&normals).map(|(g, nn)| vec![Vec3A::ZERO; passes * nn * g.gw * g.gh]).collect();
        let mut last_added = 0.0f32;
        self.work.begin(super::progress::Stage::Bounces, u64::from(bounces.min(1000)));
        for b in 0..bounces {
            self.work.add(1);
            let recv: Vec<Vec<Vec3A>> = (0..self.faces.len())
                .into_par_iter()
                .map(|fi| {
                    let g = &grids[fi];
                    let nn = normals[fi];
                    let npts = g.gw * g.gh;
                    let mut pts = vec![Vec3A::ZERO; passes * nn * npts];
                    for tile in g.hits.tiles(rays * 128) {
                        let (start, hits) = tile?;
                        for point in 0..hits.len() / rays {
                            let gp = start / rays + point;
                            let seed = hash(fi as u32, gp as u32, 0x5eed);
                            let normals = &g.normals[gp];
                            let frame = (nn > 1).then(|| {
                                let (t, b) = basis(normals[0]);
                                [t, b, normals[0]]
                            });
                            for k in 0..rays {
                                let (hi, s, t) = hits[point * rays + k];
                                let (hi, mirrored) = split_hit(hi);
                                let Some(hf) = targets.get(hi) else { continue };
                                let refl =
                                    if mirrored { hf.reflectivity * self.water_reflectivity } else { hf.reflectivity };
                                let (s, t) = (f32::from(s) / Q, f32::from(t) / Q);
                                let mut wts = [1.0f32; 4];
                                if let Some(frame) = &frame {
                                    let dir = cosine_dir_in(frame, k, rays, seed);
                                    let d0 = dir.dot(normals[0]).max(1e-4);
                                    for n in 1..nn {
                                        wts[n] = dir.dot(normals[n]).max(0.0) / d0;
                                    }
                                }
                                let cell = hf.cell(s, t);
                                for p in 0..passes {
                                    let at = hf.emit + p * hf.npts;
                                    let v = cell.sample(&emit[at..at + hf.npts], hf.gw) * refl;
                                    for n in 0..nn {
                                        pts[(p * nn + n) * npts + gp] += v * wts[n];
                                    }
                                }
                            }
                        }
                    }
                    for v in &mut pts {
                        *v /= rays as f32;
                    }
                    Ok(pts)
                })
                .collect::<Vec<Result<Vec<Vec3A>, String>>>()
                .into_iter()
                .collect::<Result<Vec<_>, String>>()?;
            let mut added = 0.0f32;
            for ((r, nn), t) in recv.iter().zip(&normals).zip(&targets) {
                for p in 0..passes {
                    emit[t.emit + p * t.npts..t.emit + (p + 1) * t.npts]
                        .copy_from_slice(&r[p * nn * t.npts..p * nn * t.npts + t.npts]);
                }
            }
            for (t, r) in total.iter_mut().zip(&recv) {
                for (a, v) in t.iter_mut().zip(r) {
                    *a += *v;
                    added = added.max(v.max_element());
                }
            }
            log(&format!("Bounce #{}: added up to {added:.3}", b + 1));
            if added < 0.01 {
                break;
            }
            let left = bounces - 1 - b;
            if added < BOUNCE_TAIL && b > 0 && left > 0 && last_added > 0.0 {
                let r = (added / last_added).clamp(0.0, 0.9);
                let tail = r * (1.0 - r.powi(left.min(1000) as i32)) / (1.0 - r);
                for (t, r) in total.iter_mut().zip(&recv) {
                    for (a, v) in t.iter_mut().zip(r) {
                        *a += *v * tail;
                    }
                }
                log(&format!("Bounces left: their light estimated at {tail:.2} times the last pass's"));
                break;
            }
            last_added = added;
        }
        let radius = blur_radius(rays, step);
        total.par_iter_mut().zip(&grids).zip(lights.par_iter()).zip(self.faces).for_each(|(((planes, g), fl), f)| {
            let (wx, wy) = (blur_weights(&g.xs, radius), blur_weights(&g.ys, radius));
            let keep = g.keep(f, fl);
            let mut tmp = Vec::new();
            for plane in planes.chunks_exact_mut(g.gw * g.gh) {
                blur_grid(plane, &keep, (&wx, &wy), &mut tmp);
            }
        });
        lights.par_iter_mut().enumerate().for_each(|(fi, fl)| {
            let f = &self.faces[fi];
            let g = &grids[fi];
            let nn = fl.normals;
            let (lux, npts) = (f.luxels(), g.gw * g.gh);
            for p in 0..passes {
                for n in 0..nn {
                    let src = &total[fi][(p * nn + n) * npts..(p * nn + n + 1) * npts];
                    let base = fl.index(p, 0, n, lux);
                    for t in 0..f.h {
                        let (y0, y1, fy) = span(&g.ys, t);
                        for s in 0..f.w {
                            let (x0, x1, fx) = span(&g.xs, s);
                            let at = |x: usize, y: usize| src[y * g.gw + x];
                            let top = at(x0, y0).lerp(at(x1, y0), fx);
                            let bottom = at(x0, y1).lerp(at(x1, y1), fx);
                            let v = &mut fl.values[base + t * f.w + s];
                            *v = (Vec3A::from(*v) + top.lerp(bottom, fy)).into();
                        }
                    }
                }
            }
        });
        let fields = if self.params.style_bounce {
            styles::direct_fields(lights, self.faces, passes)
        } else {
            Default::default()
        };
        if !fields.is_empty() {
            let touched = styles::bounce_styles(
                self.faces,
                &grids,
                rays,
                passes,
                lights,
                &fields,
                bounces,
                self.water_reflectivity,
            )?;
            log(&format!("bounced the light of {} switchable styles onto {} faces", fields.len(), touched.len()));
        }
        Ok((gather_time, clock.elapsed().as_secs_f64()))
    }
}

#[derive(Clone, Copy, Debug)]
struct Cell {
    x: [usize; 2],
    y: [usize; 2],
    f: [f32; 2],
}

impl Cell {
    fn sample(&self, buf: &[Vec3A], gw: usize) -> Vec3A {
        let at = |x: usize, y: usize| buf.get(y * gw + x).copied().unwrap_or(Vec3A::ZERO);
        let top = at(self.x[0], self.y[0]).lerp(at(self.x[1], self.y[0]), self.f[0]);
        let bottom = at(self.x[0], self.y[1]).lerp(at(self.x[1], self.y[1]), self.f[0]);
        top.lerp(bottom, self.f[1])
    }
}

#[derive(Clone, Copy, Debug)]
struct Axis {
    step: usize,
    last: usize,
    end: usize,
}

impl Axis {
    fn of(points: &[usize], step: usize) -> Axis {
        Axis { step: step.max(1), last: points.len() - 1, end: points[points.len() - 1] }
    }

    fn point(&self, k: usize) -> usize {
        if k == self.last { self.end } else { k * self.step }
    }

    fn span(&self, x: f32) -> (usize, usize, f32) {
        if self.last == 0 || !x.is_finite() {
            return (0, 0, 0.0);
        }
        let x = x.clamp(0.0, self.end as f32);
        let k = ((x / self.step as f32) as usize).min(self.last - 1);
        let (a, b) = (self.point(k) as f32, self.point(k + 1) as f32);
        (k, k + 1, if b > a { ((x - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 })
    }
}

#[derive(Clone, Copy, Debug)]
struct Target {
    emit: usize,
    npts: usize,
    gw: usize,
    x: Axis,
    y: Axis,
    reflectivity: Vec3A,
}

impl Target {
    fn cell(&self, s: f32, t: f32) -> Cell {
        let (x0, x1, fx) = self.x.span(s);
        let (y0, y1, fy) = self.y.span(t);
        Cell { x: [x0, x1], y: [y0, y1], f: [fx, fy] }
    }
}

pub(crate) fn lookup_a(buf: &[Vec3A], w: usize, h: usize, s: f32, t: f32) -> Vec3A {
    if buf.len() < w * h || w == 0 || h == 0 || !(s.is_finite() && t.is_finite()) {
        return Vec3A::ZERO;
    }
    let x = s.clamp(0.0, (w - 1) as f32);
    let y = t.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize| buf[y * w + x];
    let top = at(x0, y0).lerp(at(x1, y0), fx);
    let bottom = at(x0, y1).lerp(at(x1, y1), fx);
    top.lerp(bottom, fy)
}

fn grid_cell(g: &Gather, s: f32, t: f32) -> Cell {
    let (x0, x1, fx) = grid_span(&g.xs, g.step, s);
    let (y0, y1, fy) = grid_span(&g.ys, g.step, t);
    Cell { x: [x0, x1], y: [y0, y1], f: [fx, fy] }
}

fn grid_span(points: &[usize], step: usize, x: f32) -> (usize, usize, f32) {
    let last = points.len() - 1;
    if last == 0 || !x.is_finite() {
        return (0, 0, 0.0);
    }
    let x = x.clamp(0.0, points[last] as f32);
    let k = ((x / step.max(1) as f32) as usize).min(last - 1);
    let (a, b) = (points[k] as f32, points[k + 1] as f32);
    (k, k + 1, if b > a { ((x - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 })
}

fn span(points: &[usize], x: usize) -> (usize, usize, f32) {
    let k = points.partition_point(|p| *p <= x).saturating_sub(1);
    let k1 = (k + 1).min(points.len() - 1);
    if k1 == k || points[k1] == points[k] {
        return (k, k, 0.0);
    }
    (k, k1, (x - points[k]) as f32 / (points[k1] - points[k]) as f32)
}

type GridPoint = (i64, i64, i64);

pub(crate) fn fill_dark(faces: &[LitFace], lights: &mut [FaceLight]) {
    lights.par_iter_mut().zip(faces).for_each(|(l, f)| {
        let lux = f.luxels();
        if l.dark.len() != lux || !l.dark.contains(&true) || !l.dark.contains(&false) {
            return;
        }
        let (w, h) = (f.w, f.h);
        let planes = l.values.len() / lux;
        let mut filled: Vec<bool> = l.dark.iter().map(|d| !d).collect();
        loop {
            let mut next: Vec<(usize, Vec<glam::Vec3>)> = Vec::new();
            for i in (0..lux).filter(|&i| !filled[i]) {
                let (x, y) = ((i % w) as isize, (i / w) as isize);
                let near: Vec<usize> = (-1..=1isize)
                    .flat_map(|dy| (-1..=1isize).map(move |dx| (x + dx, y + dy)))
                    .filter(|&(nx, ny)| nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h)
                    .map(|(nx, ny)| ny as usize * w + nx as usize)
                    .filter(|&k| filled[k])
                    .collect();
                if near.is_empty() {
                    continue;
                }
                let avg = (0..planes)
                    .map(|p| near.iter().map(|&k| l.values[p * lux + k]).sum::<glam::Vec3>() / near.len() as f32)
                    .collect();
                next.push((i, avg));
            }
            if next.is_empty() {
                break;
            }
            for (i, avg) in next {
                for (p, v) in avg.into_iter().enumerate() {
                    l.values[p * lux + i] = v;
                }
                filled[i] = true;
            }
        }
    });
}

pub(crate) fn stitch(faces: &[LitFace], lights: &mut [FaceLight]) {
    use ahash::AHashMap as HashMap;
    let q = |v: f64, k: f64| (v * k).round() as i64;
    let mut groups: HashMap<Vec<i64>, Vec<usize>> = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        if f.disp.is_some()
            || f.frame.is_none()
            || f.poly.len() < 3
            || lights.get(i).is_none_or(|l| l.dark.len() != f.luxels())
        {
            continue;
        }
        let d = f.poly[0].dot(f.normal);
        let mut key = vec![q(f.normal.x, 1e4), q(f.normal.y, 1e4), q(f.normal.z, 1e4), q(d, 100.0)];
        key.extend(f.lm.iter().flatten().map(|v| q(*v, 1e5)));
        key.extend([q(f.offset.x, 100.0), q(f.offset.y, 100.0), q(f.offset.z, 100.0)]);
        groups.entry(key).or_default().push(i);
    }
    let lists: Vec<Vec<usize>> = groups.into_values().filter(|l| l.len() > 1).collect();
    let cells: Vec<Vec<(usize, usize)>> = lists.par_iter().flat_map_iter(|list| shared_cells(faces, list)).collect();
    for shared in &cells {
        if shared.iter().any(|&(fi, i)| lights[fi].dark[i]) {
            continue;
        }
        let (f0, _) = shared[0];
        let nn = lights[f0].normals;
        if shared.iter().any(|&(fi, _)| lights[fi].normals != nn) {
            continue;
        }
        let passes = lights[f0].values.len() / (lights[f0].styles.len() * nn * faces[f0].luxels()).max(1);
        for style in lights[f0].styles.clone() {
            let slots: Vec<Option<usize>> =
                shared.iter().map(|&(fi, _)| lights[fi].styles.iter().position(|s| *s == style)).collect();
            if slots.iter().any(Option::is_none) {
                continue;
            }
            for p in 0..passes {
                for n in 0..nn {
                    let ks: Vec<usize> = shared
                        .iter()
                        .zip(&slots)
                        .map(|(&(fi, i), slot)| lights[fi].index(p, slot.unwrap_or(0), n, faces[fi].luxels()) + i)
                        .collect();
                    let sum: Vec3A =
                        shared.iter().zip(&ks).map(|(&(fi, _), &k)| Vec3A::from(lights[fi].values[k])).sum();
                    let mean = sum / shared.len() as f32;
                    for (&(fi, _), &k) in shared.iter().zip(&ks) {
                        lights[fi].values[k] = mean.into();
                    }
                }
            }
        }
    }
}

fn shared_cells(faces: &[LitFace], list: &[usize]) -> Vec<Vec<(usize, usize)>> {
    use ahash::AHashMap as HashMap;
    let q = |v: f64| (v * 8.0).round() as i64;
    let point = |p: glam::DVec3| (q(p.x), q(p.y), q(p.z));
    let mut parent: Vec<usize> = (0..list.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut edges: HashMap<(GridPoint, GridPoint), usize> = HashMap::new();
    for (k, &fi) in list.iter().enumerate() {
        let poly = &faces[fi].poly;
        for e in 0..poly.len() {
            let (a, b) = (point(poly[e]), point(poly[(e + 1) % poly.len()]));
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(&other) = edges.get(&key) {
                let (ra, rb) = (root(&mut parent, k), root(&mut parent, other));
                parent[ra] = rb;
            } else {
                edges.insert(key, k);
            }
        }
    }
    let rect = |k: usize| {
        let f = &faces[list[k]];
        (f.mins[0], f.mins[1], f.mins[0] + f.w as i32, f.mins[1] + f.h as i32)
    };
    let roots: Vec<usize> = (0..list.len()).map(|k| root(&mut parent, k)).collect();
    let mut order: Vec<usize> = (0..list.len()).collect();
    order.sort_unstable_by_key(|&k| (roots[k], rect(k).0, k));
    let mut hits: Vec<(usize, i32, i32, usize)> = Vec::new();
    for (i, &a) in order.iter().enumerate() {
        let (ax0, ay0, ax1, ay1) = rect(a);
        for &b in &order[i + 1..] {
            let (bx0, by0, bx1, by1) = rect(b);
            if roots[b] != roots[a] || bx0 >= ax1 {
                break;
            }
            let (y0, y1) = (ay0.max(by0), ay1.min(by1));
            let (x0, x1) = (ax0.max(bx0), ax1.min(bx1));
            for y in y0..y1 {
                for x in x0..x1 {
                    hits.push((roots[a], x, y, a));
                    hits.push((roots[a], x, y, b));
                }
            }
        }
    }
    hits.sort_unstable();
    hits.dedup();
    let mut out = Vec::new();
    let mut i = 0;
    while i < hits.len() {
        let (r, x, y, _) = hits[i];
        let mut j = i;
        let mut cell = Vec::new();
        while j < hits.len() && (hits[j].0, hits[j].1, hits[j].2) == (r, x, y) {
            let k = hits[j].3;
            let f = &faces[list[k]];
            let idx = (y - f.mins[1]) as usize * f.w + (x - f.mins[0]) as usize;
            cell.push((list[k], idx));
            j += 1;
        }
        if cell.len() > 1 {
            out.push(cell);
        }
        i = j;
    }
    out
}

pub(crate) fn lit_index<'a>(n_faces: usize, faces: impl Iterator<Item = &'a LitFace>) -> Vec<Option<u32>> {
    let mut v = vec![None; n_faces];
    for (i, f) in faces.enumerate() {
        if let Some(slot) = v.get_mut(f.face) {
            *slot = Some(i as u32);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference_stitch(faces: &[LitFace], lights: &mut [FaceLight]) {
        use std::collections::HashMap;
        let q = |v: f64, k: f64| (v * k).round() as i64;
        let mut groups: HashMap<Vec<i64>, Vec<usize>> = HashMap::new();
        for (i, f) in faces.iter().enumerate() {
            if f.disp.is_some()
                || f.frame.is_none()
                || f.poly.len() < 3
                || lights.get(i).is_none_or(|l| l.dark.len() != f.luxels())
            {
                continue;
            }
            let d = f.poly[0].dot(f.normal);
            let mut key = vec![q(f.normal.x, 1e4), q(f.normal.y, 1e4), q(f.normal.z, 1e4), q(d, 100.0)];
            key.extend(f.lm.iter().flatten().map(|v| q(*v, 1e5)));
            key.extend([q(f.offset.x, 100.0), q(f.offset.y, 100.0), q(f.offset.z, 100.0)]);
            groups.entry(key).or_default().push(i);
        }
        let point = |p: glam::DVec3| (q(p.x, 8.0), q(p.y, 8.0), q(p.z, 8.0));
        for list in groups.values().filter(|l| l.len() > 1) {
            let mut parent: Vec<usize> = (0..list.len()).collect();
            fn root(parent: &mut [usize], mut i: usize) -> usize {
                while parent[i] != i {
                    parent[i] = parent[parent[i]];
                    i = parent[i];
                }
                i
            }
            let mut edges: HashMap<(GridPoint, GridPoint), usize> = HashMap::new();
            for (k, &fi) in list.iter().enumerate() {
                let poly = &faces[fi].poly;
                for e in 0..poly.len() {
                    let (a, b) = (point(poly[e]), point(poly[(e + 1) % poly.len()]));
                    let key = if a < b { (a, b) } else { (b, a) };
                    if let Some(&other) = edges.get(&key) {
                        let (ra, rb) = (root(&mut parent, k), root(&mut parent, other));
                        parent[ra] = rb;
                    } else {
                        edges.insert(key, k);
                    }
                }
            }
            let mut cells: HashMap<(usize, i32, i32), Vec<(usize, usize)>> = HashMap::new();
            for (k, &fi) in list.iter().enumerate() {
                let r = root(&mut parent, k);
                let f = &faces[fi];
                for t in 0..f.h {
                    for s in 0..f.w {
                        cells
                            .entry((r, f.mins[0] + s as i32, f.mins[1] + t as i32))
                            .or_default()
                            .push((fi, t * f.w + s));
                    }
                }
            }
            for shared in cells.values().filter(|c| c.len() > 1) {
                if shared.iter().any(|&(fi, i)| lights[fi].dark[i]) {
                    continue;
                }
                let (f0, _) = shared[0];
                let nn = lights[f0].normals;
                if shared.iter().any(|&(fi, _)| lights[fi].normals != nn) {
                    continue;
                }
                let passes = lights[f0].values.len() / (lights[f0].styles.len() * nn * faces[f0].luxels()).max(1);
                for style in lights[f0].styles.clone() {
                    let slots: Vec<Option<usize>> =
                        shared.iter().map(|&(fi, _)| lights[fi].styles.iter().position(|s| *s == style)).collect();
                    if slots.iter().any(Option::is_none) {
                        continue;
                    }
                    for p in 0..passes {
                        for n in 0..nn {
                            let ks: Vec<usize> = shared
                                .iter()
                                .zip(&slots)
                                .map(|(&(fi, i), slot)| {
                                    lights[fi].index(p, slot.unwrap_or(0), n, faces[fi].luxels()) + i
                                })
                                .collect();
                            let sum: Vec3A =
                                shared.iter().zip(&ks).map(|(&(fi, _), &k)| Vec3A::from(lights[fi].values[k])).sum();
                            let mean = sum / shared.len() as f32;
                            for (&(fi, _), &k) in shared.iter().zip(&ks) {
                                lights[fi].values[k] = mean.into();
                            }
                        }
                    }
                }
            }
        }
    }

    fn square(face: usize, x0: f64, y0: f64, size: f64, z: f64) -> LitFace {
        let poly = vec![
            glam::DVec3::new(x0, y0, z),
            glam::DVec3::new(x0 + size, y0, z),
            glam::DVec3::new(x0 + size, y0 + size, z),
            glam::DVec3::new(x0, y0 + size, z),
        ];
        let mins = [(x0 / 16.0).floor() as i32, (y0 / 16.0).floor() as i32];
        let w = (size / 16.0) as usize + 1;
        LitFace {
            face,
            w,
            h: w,
            bump: false,
            normal: glam::DVec3::Z,
            tex: [glam::DVec3::X, glam::DVec3::Y],
            frame: Some((glam::DVec3::new(0.0, 0.0, z), glam::DVec3::X * 16.0, glam::DVec3::Y * 16.0)),
            mins,
            poly2: poly
                .iter()
                .map(|p| DVec2::new(p.x / 16.0 - f64::from(mins[0]), p.y / 16.0 - f64::from(mins[1])))
                .collect(),
            bounds: (poly[0], poly[2]),
            poly,
            disp: None,
            reflectivity: Vec3A::splat(0.5),
            minlight: 0.0,
            lm: [[1.0 / 16.0, 0.0, 0.0, 0.0], [0.0, 1.0 / 16.0, 0.0, 0.0]],
            offset: glam::DVec3::ZERO,
            vnormals: None,
        }
    }

    #[test]
    fn stitching_matches_the_reference_on_shared_and_overlapping_edges() {
        let mut faces = Vec::new();
        for gy in 0..3 {
            for gx in 0..4 {
                faces.push(square(faces.len(), gx as f64 * 64.0, gy as f64 * 64.0, 64.0, 0.0));
            }
        }
        faces.push(square(faces.len(), 32.0, 32.0, 64.0, 0.0));
        faces.push(square(faces.len(), 0.0, 0.0, 128.0, 64.0));
        faces.push(square(faces.len(), 128.0, 0.0, 128.0, 64.0));
        faces.push(square(faces.len(), 256.0, 256.0, 32.0, 0.0));
        let mut seed = 99u32;
        let mut rnd = || {
            seed = hash(seed, 3, 7);
            unit(seed)
        };
        let lights: Vec<FaceLight> = faces
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let lux = f.luxels();
                let styles = if i % 5 == 3 { vec![0, 32] } else { vec![0] };
                let normals = if i % 7 == 2 { 4 } else { 1 };
                let passes = 2;
                let values = (0..passes * styles.len() * normals * lux)
                    .map(|_| glam::Vec3::new(rnd() * 300.0, rnd() * 300.0, rnd() * 300.0))
                    .collect();
                let dark = (0..lux).map(|_| rnd() < 0.05).collect();
                FaceLight { styles, normals, values, dark }
            })
            .collect();
        let mut want = lights.clone();
        reference_stitch(&faces, &mut want);
        let mut got = lights.clone();
        stitch(&faces, &mut got);
        let changed = want.iter().zip(&lights).filter(|(a, b)| a.values != b.values).count();
        assert!(changed > 6, "the seams were stitched ({changed} faces changed)");
        for (i, (a, b)) in want.iter().zip(&got).enumerate() {
            assert!(a.values == b.values, "face {i} differs");
        }
    }

    #[test]
    fn supersample_order_covers_the_grid() {
        let mut seen = [false; 25];
        for (x, y) in SUPERSAMPLE_ORDER {
            assert!(!seen[y * 5 + x]);
            seen[y * 5 + x] = true;
        }
        assert!(seen.iter().all(|s| *s));
        assert_eq!(SUPERSAMPLE_ORDER[..PROBES], [(0, 0), (4, 0), (0, 4), (4, 4), (2, 2)]);
    }

    fn span_f(points: &[usize], step: usize, x: f32) -> (usize, usize, f32) {
        let last = points.len() - 1;
        if last == 0 || !x.is_finite() {
            return (0, 0, 0.0);
        }
        let x = x.clamp(0.0, points[last] as f32);
        let k = ((x / step.max(1) as f32) as usize).min(last - 1);
        let (a, b) = (points[k] as f32, points[k + 1] as f32);
        (k, k + 1, if b > a { ((x - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 })
    }

    #[test]
    fn axis_spans_match_the_point_lists() {
        for n in 1..40 {
            for step in [1usize, 2, 3, 4, 8, 16] {
                let pts = axis_points(n, step);
                let axis = Axis::of(&pts, step);
                for (k, point) in pts.iter().enumerate() {
                    assert_eq!(axis.point(k), *point);
                }
                for i in -8..(n as i32 * 8 + 16) {
                    let x = i as f32 / 8.0 + 0.03;
                    assert_eq!(axis.span(x), span_f(&pts, step, x), "n {n} step {step} x {x}");
                }
                assert_eq!(axis.span(f32::NAN), span_f(&pts, step, f32::NAN));
                assert_eq!(axis.span(f32::INFINITY), span_f(&pts, step, f32::INFINITY));
            }
        }
    }

    #[test]
    fn dark_luxels_take_the_light_of_their_neighbours() {
        let f = square(0, 0.0, 0.0, 64.0, 0.0);
        let lux = f.luxels();
        let mut dark = vec![false; lux];
        for i in 0..f.w {
            dark[i] = true;
            dark[i * f.w + f.w - 1] = true;
        }
        let values: Vec<glam::Vec3> = (0..2 * lux)
            .map(|i| if dark[i % lux] { glam::Vec3::ZERO } else { glam::Vec3::splat(10.0 + (i / lux) as f32) })
            .collect();
        let mut lights = vec![FaceLight { styles: vec![0, 32], normals: 1, values, dark: dark.clone() }];
        fill_dark(std::slice::from_ref(&f), &mut lights);
        for (i, v) in lights[0].values.iter().enumerate() {
            assert_eq!(*v, glam::Vec3::splat(10.0 + (i / lux) as f32), "luxel {i}");
        }
        assert_eq!(lights[0].dark, dark);
        let all = vec![true; lux];
        let mut black = vec![FaceLight { styles: vec![0], normals: 1, values: vec![glam::Vec3::ZERO; lux], dark: all }];
        fill_dark(std::slice::from_ref(&f), &mut black);
        assert!(black[0].values.iter().all(|v| *v == glam::Vec3::ZERO));
    }

    #[test]
    fn bounce_sources_spread_hot_spots_over_a_patch_and_keep_flat_light() {
        let (w, h) = (9usize, 9usize);
        let grid = |step: usize| {
            let (xs, ys) = (axis_points(w, step), axis_points(h, step));
            Gather {
                step,
                gw: xs.len(),
                gh: ys.len(),
                xs,
                ys,
                normals: Vec::new(),
                hits: super::super::tiles::Store::new(1 << 20).keep(Vec::new()).expect("empty hits"),
                luxels: w * h,
                patches: None,
                buried: Vec::new(),
            }
        };
        let flat = vec![glam::Vec3::splat(7.0); w * h];
        let g = grid(2);
        let mut out = vec![Vec3A::ZERO; g.gw * g.gh];
        patch_light(&flat, w, h, &g, &mut out);
        assert!(out.iter().all(|v| (*v - Vec3A::splat(7.0)).abs().max_element() < 1e-5));
        let mut spot = vec![glam::Vec3::ZERO; w * h];
        spot[4 * w + 4] = glam::Vec3::splat(1600.0);
        let g = grid(1);
        let mut out = vec![Vec3A::ZERO; g.gw * g.gh];
        patch_light(&spot, w, h, &g, &mut out);
        assert!((out[4 * w + 4].x - 100.0).abs() < 1e-3, "{}", out[4 * w + 4].x);
        assert!((out[4 * w + 6].x - 50.0).abs() < 1e-3);
        assert!((out[6 * w + 6].x - 25.0).abs() < 1e-3);
        assert_eq!(out[4 * w + 7].x, 0.0);
        let total: f32 = out.iter().map(|v| v.x).sum();
        assert!((total - 1600.0).abs() < 1e-2, "{total}");
    }

    #[test]
    fn bounce_smoothing_scales_with_the_ray_density_and_keeps_flat_light() {
        assert_eq!(blur_radius(64, 2), 6);
        assert_eq!(blur_radius(128, 1), 4);
        assert_eq!(blur_radius(32, 4), 8);
        assert_eq!(blur_radius(64, 1), 4);
        let points = axis_points(17, 2);
        let weights = blur_weights(&points, 4);
        for row in &weights {
            assert!((row.iter().map(|r| r.1).sum::<f32>() - 1.0).abs() < 1e-5);
        }
        assert_eq!(weights[4].iter().map(|r| r.0).collect::<Vec<_>>(), vec![2, 3, 4, 5, 6]);
        let gw = points.len();
        let all = vec![true; gw * gw];
        let mut flat = vec![Vec3A::splat(3.0); gw * gw];
        let mut tmp = Vec::new();
        blur_grid(&mut flat, &all, (&weights, &weights), &mut tmp);
        assert!(flat.iter().all(|v| (*v - Vec3A::splat(3.0)).abs().max_element() < 1e-5));
        let mut holed = vec![Vec3A::splat(3.0); gw * gw];
        let mut keep = all.clone();
        for k in [0, 4 * gw + 4, 4 * gw + 5, gw * gw - 1] {
            holed[k] = Vec3A::ZERO;
            keep[k] = false;
        }
        blur_grid(&mut holed, &keep, (&weights, &weights), &mut tmp);
        assert!(holed.iter().all(|v| (*v - Vec3A::splat(3.0)).abs().max_element() < 1e-5), "blind points are skipped");
        let mut spot = vec![Vec3A::ZERO; gw * gw];
        spot[4 * gw + 4] = Vec3A::splat(169.0);
        blur_grid(&mut spot, &all, (&weights, &weights), &mut tmp);
        assert!((spot[4 * gw + 4].x - 25.0).abs() < 1e-3, "{}", spot[4 * gw + 4].x);
        assert!((spot[4 * gw + 5].x - 15.0).abs() < 1e-3);
        assert_eq!(spot[4 * gw + 7].x, 0.0);
        let sum: f32 = spot.iter().map(|v| v.x).sum();
        assert!((sum - 169.0).abs() < 1e-2, "{sum}");
    }

    #[test]
    fn helpers() {
        let pts = axis_points(10, 4);
        assert_eq!(pts, vec![0, 4, 8, 9]);
        assert_eq!(span(&pts, 6), (1, 2, 0.5));
        assert_eq!(span(&pts, 9), (3, 3, 0.0));
        assert_eq!(axis_points(1, 2), vec![0]);
        assert_eq!(span_f(&pts, 4, 6.0), (1, 2, 0.5));
        assert_eq!(span_f(&pts, 4, 9.5), (2, 3, 1.0));
        assert_eq!(span_f(&pts, 4, 8.5), (2, 3, 0.5));
        assert_eq!(span_f(&pts, 4, 0.0), (0, 1, 0.0));
        assert_eq!(span_f(&[0], 4, 3.0), (0, 0, 0.0));
        assert_eq!(span_f(&pts, 4, f32::NAN), (0, 0, 0.0));
        let buf = vec![glam::Vec3::ZERO, glam::Vec3::ONE, glam::Vec3::ONE * 2.0, glam::Vec3::ONE * 3.0];
        assert!((lookup(&buf, 2, 2, 0.5, 0.5).x - 1.5).abs() < 1e-6);
        assert_eq!(lookup(&buf, 2, 2, f32::NAN, 0.0), Vec3A::ZERO);
        let dirs = sphere_dirs(162);
        let sum: Vec3A = dirs.iter().copied().sum();
        assert!(sum.length() < 1.0);
        let n = Vec3A::new(0.0, 0.6, 0.8);
        let mut avg = 0.0;
        for k in 0..256 {
            let d = cosine_dir(n, k, 256, 7);
            assert!(d.dot(n) >= 0.0);
            avg += d.dot(n);
        }
        assert!((avg / 256.0 - 2.0 / 3.0).abs() < 0.03, "{}", avg / 256.0);
    }
}
