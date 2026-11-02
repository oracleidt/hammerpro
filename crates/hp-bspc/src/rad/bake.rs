use glam::{DVec2, Vec3A};
use hp_light::bvh::Bvh;
use rayon::prelude::*;

use super::RadQuality;
use super::data::MapData;
use super::lights::{Kind, Lights};
use super::surface::{LitFace, SurfTri, bump_normals, inside};

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

#[derive(Clone, Copy, Debug)]
pub(crate) struct Params {
    pub sky_dirs: usize,
    pub sun_samples: usize,
    pub supersample_passes: usize,
    pub bounce_step: usize,
    pub bounce_rays: usize,
    pub leaf_samples: usize,
    pub leaf_dirs: usize,
}

impl Params {
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
            },
            RadQuality::Normal => Params {
                sky_dirs: 162,
                sun_samples: 16,
                supersample_passes: 2,
                bounce_step: 2,
                bounce_rays: 64,
                leaf_samples: 128,
                leaf_dirs: 162,
            },
            RadQuality::Final => Params {
                sky_dirs: 512,
                sun_samples: 30,
                supersample_passes: 4,
                bounce_step: 1,
                bounce_rays: 128,
                leaf_samples: 128,
                leaf_dirs: 162,
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
    pub values: Vec<Vec3A>,
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
    pub ao: Option<super::ao::Ao>,
    pub blockers: &'a Bvh,
    pub work: &'a super::progress::Work,
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
        let h = self.surf.closest(from, dir, MAX_TRACE, |_| true)?;
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
        let Some(h) = self.surf.closest(from, dir, MAX_TRACE, |_| true) else { return SurfHit::None };
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
                let u = if f.w > 1 { s / (f.w - 1) as f64 } else { 0.5 };
                let v = if f.h > 1 { t / (f.h - 1) as f64 } else { 0.5 };
                let (p, n) = d.sample(DVec2::new(u, v), 1.0, f.normal);
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
        Sample { pos: pos.as_vec3a(), normals, cluster: self.map.cluster_at(pos) }
    }

    fn candidates(&self, f: &LitFace) -> (Vec<(usize, usize)>, Vec<u8>) {
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
            let slot = match styles.iter().position(|s| *s == l.style) {
                Some(k) => k,
                None if styles.len() < 4 => {
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
        let (cand, styles) = self.candidates(f);
        let slots = styles.len();
        let nn = if f.bump { 4 } else { 1 };
        let lux = f.luxels();
        let stride = self.passes * slots * nn;
        let mut acc = vec![Vec3A::ZERO; stride * lux];
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
        let sky_lights = super::sky::lights_of(self, &cand);
        let part = if sky_lights.is_empty() { Part::All } else { Part::Direct };
        let mut samples: Vec<Option<Sample>> = Vec::with_capacity(lux);
        for i in 0..lux {
            if dark[i] {
                samples.push(None);
                continue;
            }
            let sp = self.sample(f, centers[i].x, centers[i].y);
            self.gather(&cand, slots, nn, &sp, part, &mut acc[i * stride..(i + 1) * stride]);
            samples.push(Some(sp));
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
            .collect();
        let kept = keep.len();
        let mut values = vec![Vec3A::ZERO; self.passes * kept * nn * lux];
        for p in 0..self.passes {
            for (ks, &slot) in keep.iter().enumerate() {
                for n in 0..nn {
                    let dst = ((p * kept + ks) * nn + n) * lux;
                    let src = (p * slots + slot) * nn + n;
                    for i in 0..lux {
                        values[dst + i] = acc[i * stride + src];
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

pub(crate) fn lookup(buf: &[Vec3A], w: usize, h: usize, s: f32, t: f32) -> Vec3A {
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

struct Gather {
    step: usize,
    gw: usize,
    gh: usize,
    xs: Vec<usize>,
    ys: Vec<usize>,
    normals: Vec<[Vec3A; 4]>,
    hits: Vec<(u32, u16, u16)>,
    luxels: usize,
}

fn axis_points(n: usize, step: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).step_by(step.max(1)).collect();
    if v.last() != Some(&(n - 1)) {
        v.push(n - 1);
    }
    v
}

fn cosine_dir(n: Vec3A, k: usize, count: usize, seed: u32) -> Vec3A {
    let side = (count as f32).sqrt().ceil().max(1.0) as usize;
    let (gx, gy) = (k % side, k / side);
    let r1 = unit(hash(seed, k as u32, 1));
    let r2 = unit(hash(seed, k as u32, 2));
    let u1 = ((gx as f32 + r1) / side as f32).min(0.9999);
    let u2 = ((gy as f32 + r2) / (count.div_ceil(side)) as f32).min(0.9999);
    let (t, b) = basis(n);
    let r = u1.sqrt();
    let phi = std::f32::consts::TAU * u2;
    let z = (1.0 - u1).max(0.0).sqrt();
    (t * (r * phi.cos()) + b * (r * phi.sin()) + n * z).normalize_or(n)
}

const Q: f32 = 32.0;

impl Ctx<'_> {
    fn gather_grid(&self, fi: usize, step: usize, rays: usize) -> Gather {
        let f = &self.faces[fi];
        let xs = axis_points(f.w, step);
        let ys = axis_points(f.h, step);
        let (gw, gh) = (xs.len(), ys.len());
        let mut normals = Vec::with_capacity(gw * gh);
        let mut hits = Vec::with_capacity(gw * gh * rays);
        for (gy, &t) in ys.iter().enumerate() {
            for (gx, &s) in xs.iter().enumerate() {
                let c = if f.disp.is_some() {
                    DVec2::new(s as f64, t as f64)
                } else {
                    let (c, piece) = f.flat_sample(s, t);
                    if piece.is_empty() { c } else { self.open_point(f, c, &piece).unwrap_or(c) }
                };
                let sp = self.sample(f, c.x, c.y);
                normals.push(sp.normals);
                let seed = hash(fi as u32, (gy * gw + gx) as u32, 0x5eed);
                let props_block = !self.props.inside_prop(self.shadow, sp.pos, sp.normals[0]);
                for k in 0..rays {
                    let dir = cosine_dir(sp.normals[0], k, rays, seed);
                    let hit = match self.trace_surface_with(sp.pos, dir, props_block) {
                        SurfHit::Face(i, s, t, _) => {
                            let q = |v: f32| (v.max(0.0) * Q).min(65535.0) as u16;
                            (i, q(s), q(t))
                        }
                        _ => (u32::MAX, 0, 0),
                    };
                    hits.push(hit);
                }
            }
        }
        Gather { step: step.max(1), gw, gh, xs, ys, normals, hits, luxels: f.luxels() }
    }

    pub fn bounce(
        &self,
        lights: &mut [FaceLight],
        bounces: u32,
        scale: Option<&[Vec<f32>]>,
        log: &mut dyn FnMut(&str),
    ) -> (f64, f64) {
        if bounces == 0 || self.faces.is_empty() {
            return (0.0, 0.0);
        }
        let rays = self.params.bounce_rays;
        let total: usize = self.faces.iter().map(|f| f.luxels()).sum();
        let mut step = self.params.bounce_step;
        while step < 16 && total / (step * step) * rays * 8 > 1 << 29 {
            step *= 2;
        }
        let clock = std::time::Instant::now();
        self.work.begin(super::progress::Stage::BounceRays, total as u64);
        let grids: Vec<Gather> = (0..self.faces.len())
            .into_par_iter()
            .map(|i| self.gather_grid(i, step, rays))
            .inspect(|g| self.work.add(g.luxels as u64))
            .collect();
        let points: usize = grids.iter().map(|g| g.gw * g.gh).sum();
        let gather_time = clock.elapsed().as_secs_f64();
        log(&format!("bounce: {points} gather points, {rays} rays each, traced in {} ms", clock.elapsed().as_millis()));
        let clock = std::time::Instant::now();
        let passes = self.passes;
        let direct: Vec<Vec<&[Vec3A]>> = lights
            .iter()
            .zip(self.faces)
            .map(|(l, f)| {
                let lux = f.luxels();
                (0..passes).map(|p| &l.values[l.index(p, 0, 0, lux)..l.index(p, 0, 0, lux) + lux]).collect()
            })
            .collect();
        let normals: Vec<usize> = lights.iter().map(|l| l.normals).collect();
        let mut emit: Vec<Vec<Vec<Vec3A>>> = Vec::new();
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
                    for gp in 0..npts {
                        let seed = hash(fi as u32, gp as u32, 0x5eed);
                        let normals = &g.normals[gp];
                        for k in 0..rays {
                            let (hi, s, t) = g.hits[gp * rays + k];
                            let hi = hi as usize;
                            let Some(hf) = self.faces.get(hi) else { continue };
                            let (s, t) = (f32::from(s) / Q, f32::from(t) / Q);
                            let mut wts = [1.0f32; 4];
                            if nn > 1 {
                                let dir = cosine_dir(normals[0], k, rays, seed);
                                let d0 = dir.dot(normals[0]).max(1e-4);
                                for n in 1..nn {
                                    wts[n] = dir.dot(normals[n]).max(0.0) / d0;
                                }
                            }
                            let cell = if b == 0 { None } else { Some(grid_cell(&grids[hi], s, t)) };
                            for p in 0..passes {
                                let v = match cell {
                                    None => lookup(direct[hi][p], hf.w, hf.h, s, t),
                                    Some(c) => c.sample(&emit[hi][p], grids[hi].gw),
                                } * hf.reflectivity;
                                for n in 0..nn {
                                    pts[(p * nn + n) * npts + gp] += v * wts[n];
                                }
                            }
                        }
                    }
                    for v in &mut pts {
                        *v /= rays as f32;
                    }
                    pts
                })
                .collect();
            let mut added = 0.0f32;
            emit = recv
                .iter()
                .zip(&normals)
                .map(|(r, nn)| {
                    let npts = r.len() / (passes * nn);
                    (0..passes).map(|p| r[p * nn * npts..p * nn * npts + npts].to_vec()).collect()
                })
                .collect();
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
        drop(direct);
        lights.par_iter_mut().enumerate().for_each(|(fi, fl)| {
            let f = &self.faces[fi];
            let g = &grids[fi];
            let nn = fl.normals;
            let (lux, npts) = (f.luxels(), g.gw * g.gh);
            let k = scale.and_then(|s| s.get(fi)).map_or(&[][..], Vec::as_slice);
            let k = if k.len() == lux { k } else { &[] };
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
                            let ao = k.get(t * f.w + s).copied().unwrap_or(1.0);
                            fl.values[base + t * f.w + s] += top.lerp(bottom, fy) * ao;
                        }
                    }
                }
            }
        });
        (gather_time, clock.elapsed().as_secs_f64())
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

fn grid_cell(g: &Gather, s: f32, t: f32) -> Cell {
    let (x0, x1, fx) = span_f(&g.xs, g.step, s);
    let (y0, y1, fy) = span_f(&g.ys, g.step, t);
    Cell { x: [x0, x1], y: [y0, y1], f: [fx, fy] }
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

fn span(points: &[usize], x: usize) -> (usize, usize, f32) {
    let k = points.partition_point(|p| *p <= x).saturating_sub(1);
    let k1 = (k + 1).min(points.len() - 1);
    if k1 == k || points[k1] == points[k] {
        return (k, k, 0.0);
    }
    (k, k1, (x - points[k]) as f32 / (points[k1] - points[k]) as f32)
}

type GridPoint = (i64, i64, i64);

pub(crate) fn stitch(faces: &[LitFace], lights: &mut [FaceLight]) {
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
                    cells.entry((r, f.mins[0] + s as i32, f.mins[1] + t as i32)).or_default().push((fi, t * f.w + s));
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
                            .map(|(&(fi, i), slot)| lights[fi].index(p, slot.unwrap_or(0), n, faces[fi].luxels()) + i)
                            .collect();
                        let sum: Vec3A = shared.iter().zip(&ks).map(|(&(fi, _), &k)| lights[fi].values[k]).sum();
                        let mean = sum / shared.len() as f32;
                        for (&(fi, _), &k) in shared.iter().zip(&ks) {
                            lights[fi].values[k] = mean;
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn lit_index(n_faces: usize, faces: &[LitFace]) -> Vec<Option<u32>> {
    let mut v = vec![None; n_faces];
    for (i, f) in faces.iter().enumerate() {
        if let Some(slot) = v.get_mut(f.face) {
            *slot = Some(i as u32);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let buf = vec![Vec3A::ZERO, Vec3A::ONE, Vec3A::ONE * 2.0, Vec3A::ONE * 3.0];
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
