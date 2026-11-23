use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use glam::Vec3A;
use hp_raytrace::LightKind;
use rayon::prelude::*;

use super::scene::{KIND_LIGHT, KIND_PROP, KIND_SKY, NONE, SRGB, Scene};

const EPS: f32 = 0.125;
const FAR: f32 = 131_072.0;
const MAX_LAYERS: usize = 24;
const MAX_DEPTH: u32 = 3;
const WATER_F0: f32 = 0.02;
pub(crate) const SHADE_BUDGET: u32 = 16;
const BRIGHTEST: f32 = 65_504.0 * 255.0;

pub(crate) type Rgb2 = [Vec3A; 2];
type Pick = (Rgb2, Vec3A, f32, f32, bool);

thread_local! {
    static PICKS: RefCell<Vec<Pick>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Limits {
    pub soft: Duration,
    pub hard: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Outcome {
    Full,
    Partial { least: u32, of: u32 },
    Skipped { faces_done: usize },
}

pub(crate) fn clean(c: Vec3A) -> Vec3A {
    let f = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, BRIGHTEST) };
    Vec3A::new(f(c.x), f(c.y), f(c.z))
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

pub(crate) fn strata_order(n: usize) -> Vec<usize> {
    let n = n.max(1);
    let mut stride = ((n as f32 * 0.618).round() as usize).max(1);
    while gcd(stride, n) != 1 {
        stride += 1;
    }
    (0..n).map(|k| (k * stride) % n).collect()
}

pub(crate) fn pass_ranges(n: usize) -> Vec<std::ops::Range<usize>> {
    let n = n.max(1);
    let first = (n / 8).clamp(1, 4);
    let chunk = (n - first).div_ceil(4).max(1);
    let mut out = Vec::with_capacity(5);
    out.push(0..first);
    let mut at = first;
    while at < n {
        out.push(at..(at + chunk).min(n));
        at += chunk;
    }
    out
}

pub(crate) fn face_dir(face: usize, s: f32, t: f32) -> Vec3A {
    match face {
        0 => Vec3A::new(1.0, -t, -s),
        1 => Vec3A::new(-1.0, -t, s),
        2 => Vec3A::new(s, 1.0, t),
        3 => Vec3A::new(s, -1.0, -t),
        4 => Vec3A::new(s, -t, 1.0),
        _ => Vec3A::new(-s, -t, -1.0),
    }
}

#[cfg(test)]
pub(crate) fn cube_lookup(d: Vec3A) -> (usize, f32, f32) {
    let a = d.abs();
    let (face, sc, tc, ma) = if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 { (0, -d.z, -d.y, a.x) } else { (1, d.z, -d.y, a.x) }
    } else if a.y >= a.z {
        if d.y > 0.0 { (2, d.x, d.z, a.y) } else { (3, d.x, -d.z, a.y) }
    } else if d.z > 0.0 {
        (4, d.x, -d.y, a.z)
    } else {
        (5, -d.x, -d.y, a.z)
    };
    let ma = ma.max(1e-12);
    (face, (sc / ma + 1.0) * 0.5, (tc / ma + 1.0) * 0.5)
}

pub(crate) fn sky_lookup(d: Vec3A) -> (usize, f32, f32) {
    let a = d.abs();
    let (face, s, t) = if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 { (0, -d.y / a.x, d.z / a.x) } else { (1, d.y / a.x, d.z / a.x) }
    } else if a.y >= a.z {
        if d.y > 0.0 { (2, d.x / a.y, d.z / a.y) } else { (3, -d.x / a.y, d.z / a.y) }
    } else if d.z > 0.0 {
        (4, -d.y / a.z, -d.x / a.z)
    } else {
        (5, -d.y / a.z, d.x / a.z)
    };
    (face, (s + 1.0) * 0.5, (1.0 - t) * 0.5)
}

pub(crate) struct Rng {
    state: u32,
    shades: u32,
}

impl Rng {
    pub fn new(a: u32, b: u32, c: u32) -> Rng {
        let mut r = Rng {
            state: a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B) ^ c.wrapping_mul(0xC2B2_AE35),
            shades: SHADE_BUDGET,
        };
        r.next();
        r
    }

    pub fn next(&mut self) -> f32 {
        self.state = self.state.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        let w = ((self.state >> ((self.state >> 28) + 4)) ^ self.state).wrapping_mul(277_803_737);
        ((w >> 22) ^ w) as f32 * (1.0 / 4_294_967_296.0)
    }

    pub fn refill(&mut self) {
        self.shades = SHADE_BUDGET;
    }

    pub fn spend(&mut self) -> bool {
        if self.shades == 0 {
            return false;
        }
        self.shades -= 1;
        true
    }
}

fn cosine(n: Vec3A, r1: f32, r2: f32) -> Vec3A {
    let a = if n.x.abs() > 0.9 { Vec3A::Y } else { Vec3A::X };
    let t = n.cross(a).normalize();
    let b = n.cross(t);
    let phi = std::f32::consts::TAU * r1;
    let r = r2.sqrt();
    (t * (phi.cos() * r) + b * (phi.sin() * r) + n * (1.0 - r2).max(0.0).sqrt()).normalize_or(n)
}

fn luminance(c: Vec3A) -> f32 {
    c.x * 0.2126 + c.y * 0.7152 + c.z * 0.0722
}

fn decode(data: &[u8], at: usize) -> Vec3A {
    let Some(b) = data.get(at..at + 4) else { return Vec3A::ZERO };
    let s = 2f32.powi(i32::from(b[3] as i8));
    Vec3A::new(f32::from(b[0]), f32::from(b[1]), f32::from(b[2])) * s
}

fn bary(uv: &[[f32; 2]; 3], u: f32, v: f32) -> [f32; 2] {
    let w = 1.0 - u - v;
    [uv[0][0] * w + uv[1][0] * u + uv[2][0] * v, uv[0][1] * w + uv[1][1] * u + uv[2][1] * v]
}

impl Scene {
    fn usable(&self) -> bool {
        !self.bvh.nodes.is_empty()
    }

    fn texel(&self, material: u32, uv: [f32; 2]) -> (Vec3A, f32) {
        let Some(m) = self.materials.get(material as usize) else { return (Vec3A::splat(0.5), 1.0) };
        match self.textures.get(m.texture as usize) {
            Some(t) => {
                let p = t.sample(uv);
                let c = Vec3A::new(SRGB[p[0] as usize], SRGB[p[1] as usize], SRGB[p[2] as usize]);
                (c * m.tint, f32::from(p[3]) / 255.0)
            }
            None => (m.average, 1.0),
        }
    }

    fn passes_alpha(&self, id: u32, u: f32, v: f32) -> bool {
        let Some(tri) = self.tris.get(id as usize) else { return true };
        if tri.material == NONE {
            return true;
        }
        let Some(m) = self.materials.get(tri.material as usize) else { return true };
        let Some(r) = m.alpha_test else { return true };
        self.texel(tri.material, bary(&tri.uv, u, v)).1 >= r
    }

    fn blocks(&self, id: u32, u: f32, v: f32) -> bool {
        let Some(tri) = self.tris.get(id as usize) else { return false };
        if tri.kind == KIND_LIGHT {
            return false;
        }
        if tri.kind == KIND_SKY {
            return true;
        }
        let Some(m) = self.materials.get(tri.material as usize) else { return true };
        if m.translucent || m.additive || m.water.is_some() {
            return false;
        }
        self.passes_alpha(id, u, v)
    }

    fn lightmap(&self, face: u32, lm: [f32; 2], pass: usize) -> Vec3A {
        let Some(f) = self.faces.get(face as usize) else { return Vec3A::ZERO };
        let data = &self.lighting[pass];
        if f.w == 0 || f.h == 0 {
            return Vec3A::ZERO;
        }
        let fx = lm[0].clamp(0.0, (f.w - 1) as f32);
        let fy = lm[1].clamp(0.0, (f.h - 1) as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(f.w as usize - 1), (y0 + 1).min(f.h as usize - 1));
        let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
        let w = f.w as usize;
        let mut sum = Vec3A::ZERO;
        for &slot in &f.slots[pass] {
            let at = |x: usize, y: usize| decode(data, slot as usize + (y * w + x) * 4);
            let top = at(x0, y0) * (1.0 - ax) + at(x1, y0) * ax;
            let bottom = at(x0, y1) * (1.0 - ax) + at(x1, y1) * ax;
            sum += top * (1.0 - ay) + bottom * ay;
        }
        sum
    }

    pub(crate) fn sky(&self, d: Vec3A, pass: usize) -> Vec3A {
        let Some(faces) = &self.sky[pass] else { return Vec3A::ZERO };
        let (face, u, v) = sky_lookup(d);
        faces.get(face).map_or(Vec3A::ZERO, |f| f.sample(u, v))
    }

    fn closest(&self, o: Vec3A, d: Vec3A, tmin: f32, primary: bool) -> Option<hp_raytrace::bvh::Hit> {
        let accept = |id: u32, u: f32, v: f32| {
            let Some(tri) = self.tris.get(id as usize) else { return false };
            if tri.kind == KIND_LIGHT {
                return primary;
            }
            self.passes_alpha(id, u, v)
        };
        self.bvh.closest(o, d, tmin, FAR, &accept, &|_| 0)
    }

    fn occluded(&self, o: Vec3A, d: Vec3A, dist: f32) -> bool {
        self.bvh.any(o, d, 0.0, dist, &|id, u, v, _| self.blocks(id, u, v))
    }

    fn sun_reaches(&self, o: Vec3A, d: Vec3A) -> bool {
        let accept = |id: u32, u: f32, v: f32| self.blocks(id, u, v);
        match self.bvh.closest(o, d, 0.0, FAR, &accept, &|_| 0) {
            None => true,
            Some(h) => self.tris.get(h.id as usize).is_some_and(|t| t.kind == KIND_SKY),
        }
    }

    fn direct(&self, p: Vec3A, n: Vec3A, rng: &mut Rng) -> Rgb2 {
        let contribution = |l: &hp_raytrace::Light| -> Option<(Rgb2, Vec3A, f32)> {
            match l.kind {
                LightKind::Sun => {
                    let to = -Vec3A::from_array(l.direction).normalize_or_zero();
                    let c = n.dot(to);
                    (c > 0.0).then(|| ([0, 1].map(|k| Vec3A::from_array(l.intensity[k]) * c), to, FAR))
                }
                LightKind::Point | LightKind::Spot { .. } => {
                    let delta = Vec3A::from_array(l.position) - p;
                    let dist = delta.length();
                    if dist <= 1e-3 {
                        return None;
                    }
                    let to = delta / dist;
                    let c = n.dot(to);
                    if c <= 0.0 {
                        return None;
                    }
                    let f = l.falloff(to, dist) * c;
                    (f > 0.0).then(|| ([0, 1].map(|k| Vec3A::from_array(l.intensity[k]) * f), to, dist))
                }
                _ => None,
            }
        };
        let weight = |c: &Rgb2| luminance(c[0]).max(luminance(c[1])).max(0.0);
        let chosen = PICKS.with_borrow_mut(|picks| {
            picks.clear();
            let mut total = 0.0f32;
            for l in &self.lights {
                if let Some((c, to, dist)) = contribution(l) {
                    let w = weight(&c);
                    total += w;
                    if w > 0.0 {
                        picks.push((c, to, dist, w, l.kind == LightKind::Sun));
                    }
                }
            }
            if total <= 0.0 || !total.is_finite() {
                return None;
            }
            let target = rng.next() * total;
            let mut run = 0.0f32;
            let mut chosen = None;
            for p in picks.iter() {
                run += p.3;
                chosen = Some((*p, total));
                if run >= target {
                    break;
                }
            }
            chosen
        });
        let Some(((c, to, dist, w, sun), total)) = chosen else { return [Vec3A::ZERO; 2] };
        let from = p + n * EPS;
        let lit = if sun { self.sun_reaches(from, to) } else { !self.occluded(from, to, (dist - 1.0).max(0.0)) };
        if !lit {
            return [Vec3A::ZERO; 2];
        }
        let scale = total / w;
        [c[0] * scale, c[1] * scale]
    }

    fn irradiance(&self, p: Vec3A, n: Vec3A, depth: u32, rng: &mut Rng) -> Rgb2 {
        if !rng.spend() {
            return [self.ambient[0] * 0.5, self.ambient[1] * 0.5];
        }
        let mut e = self.direct(p, n, rng);
        if depth < MAX_DEPTH {
            let d = cosine(n, rng.next(), rng.next());
            let l = self.trace(p + n * EPS, d, depth + 1, rng, true);
            e[0] += l[0];
            e[1] += l[1];
        } else {
            e[0] += self.ambient[0] * 0.5;
            e[1] += self.ambient[1] * 0.5;
        }
        e
    }

    pub(crate) fn trace(&self, origin: Vec3A, d: Vec3A, depth: u32, rng: &mut Rng, bounce: bool) -> Rgb2 {
        let mut acc = [Vec3A::ZERO; 2];
        if !self.usable() {
            return acc;
        }
        let mut thr = Vec3A::ONE;
        let mut o = origin;
        let mut in_sky = false;
        let sky_of = |dir: Vec3A| -> Rgb2 { if bounce { self.ambient } else { [self.sky(dir, 0), self.sky(dir, 1)] } };
        for _ in 0..MAX_LAYERS {
            let Some(h) = self.closest(o, d, 0.0, !bounce) else {
                let s = sky_of(d);
                acc[0] += thr * s[0];
                acc[1] += thr * s[1];
                break;
            };
            let tri = self.tris[h.id as usize];
            let p = o + d * h.t;
            if tri.kind == KIND_LIGHT {
                if let Some(s) = self.spheres.get(tri.face as usize) {
                    let w = match s.light.kind {
                        LightKind::Spot { stopdot, stopdot2, .. } => {
                            let dot = (-d).dot(Vec3A::from_array(s.light.direction).normalize_or_zero());
                            if dot <= stopdot2 {
                                0.0
                            } else if dot >= stopdot || stopdot <= stopdot2 {
                                1.0
                            } else {
                                (dot - stopdot2) / (stopdot - stopdot2)
                            }
                        }
                        _ => 1.0,
                    };
                    if w > 0.0 {
                        acc[0] += thr * s.radiance[0] * w;
                        acc[1] += thr * s.radiance[1] * w;
                        break;
                    }
                }
                o = p + d * EPS;
                continue;
            }
            if tri.kind == KIND_SKY {
                if let (false, Some((cam, scale))) = (in_sky, self.camera) {
                    in_sky = true;
                    o = cam + p / scale + d * (EPS / scale);
                    continue;
                }
                let s = sky_of(d);
                acc[0] += thr * s[0];
                acc[1] += thr * s[1];
                break;
            }
            let Some(m) = self.materials.get(tri.material as usize) else { break };
            let geo = self.normals[h.id as usize];
            let facing = d.dot(geo) < 0.0;
            if !facing && !m.two_sided {
                o = p + d * EPS;
                continue;
            }
            let (albedo, alpha) = self.texel(tri.material, bary(&tri.uv, h.u, h.v));
            let surface: Rgb2 = if tri.kind == KIND_PROP {
                let pn = self.prop_normals.get(tri.face as usize);
                let w = 1.0 - h.u - h.v;
                let mut n = pn.map_or(geo, |pn| (pn[0] * w + pn[1] * h.u + pn[2] * h.v).normalize_or(geo));
                if n.dot(d) > 0.0 {
                    n = -n;
                }
                let e = self.irradiance(p - d * EPS, n, depth, rng);
                let glow = if m.selfillum { albedo * (alpha * 255.0) } else { Vec3A::ZERO };
                [albedo * e[0] + glow, albedo * e[1] + glow]
            } else if let Some(fog) = m.water {
                let n = if facing { geo } else { -geo };
                let cos = (-d).dot(n).clamp(0.0, 1.0);
                let f = WATER_F0 + (1.0 - WATER_F0) * (1.0 - cos).powi(5);
                let r = d - n * (2.0 * d.dot(n));
                let refl = if depth == 0 && !bounce {
                    self.trace(p + n * EPS, r, depth + 1, rng, false)
                } else {
                    [self.sky(r, 0), self.sky(r, 1)]
                };
                let below = if depth == 0 && !bounce {
                    self.trace(p + d * EPS, d, depth + 1, rng, false)
                } else {
                    [Vec3A::ZERO; 2]
                };
                let peak = fog.max_element().max(1e-3);
                let tint = (fog / peak).clamp(Vec3A::ZERO, Vec3A::ONE);
                [0, 1].map(|k| refl[k] * f + (below[k] * tint * 0.6 + fog * 255.0 * 0.05) * (1.0 - f))
            } else if m.unlit || tri.face == NONE {
                let c = albedo * 255.0;
                [c, c]
            } else {
                let glow = if m.selfillum { albedo * (alpha * 255.0) } else { Vec3A::ZERO };
                [0, 1].map(|k| albedo * self.lightmap(tri.face, bary(&tri.lm, h.u, h.v), k) + glow)
            };
            if m.additive {
                acc[0] += thr * surface[0];
                acc[1] += thr * surface[1];
                o = p + d * EPS;
                continue;
            }
            if m.translucent && m.water.is_none() {
                let a = (alpha * m.alpha).clamp(0.0, 1.0);
                acc[0] += thr * surface[0] * a;
                acc[1] += thr * surface[1] * a;
                thr *= 1.0 - a;
                if thr.max_element() < 0.01 {
                    break;
                }
                o = p + d * EPS;
                continue;
            }
            acc[0] += thr * surface[0];
            acc[1] += thr * surface[1];
            break;
        }
        acc
    }

    pub(crate) fn render(
        &self,
        origin: Vec3A,
        size: u32,
        samples: u32,
        seed: u32,
        limits: Limits,
    ) -> ([[Vec<Vec3A>; 6]; 2], Outcome) {
        let start = Instant::now();
        let size = size.max(1) as usize;
        let grid = (samples.max(1) as f32).sqrt().ceil() as usize;
        let n = grid * grid;
        let order = strata_order(n);
        let passes = pass_ranges(n);
        let mut acc: Vec<(Rgb2, u32)> = vec![([Vec3A::ZERO; 2], 0); 6 * size * size];
        for (pi, range) in passes.iter().enumerate() {
            let limit = if pi == 0 { limits.hard } else { limits.soft.min(limits.hard) };
            if pi > 0 && start.elapsed() >= limit {
                break;
            }
            let cut = AtomicBool::new(false);
            let strata = &order[range.clone()];
            let pass_seed = seed ^ (pi as u32).wrapping_mul(0x632B_E5AB);
            acc.par_chunks_mut(size).enumerate().for_each(|(row, cells)| {
                let (face, y) = (row / size, row % size);
                for (x, cell) in cells.iter_mut().enumerate() {
                    let mut rng = Rng::new(pass_seed, row as u32, x as u32);
                    for &k in strata {
                        if cut.load(Ordering::Relaxed) {
                            return;
                        }
                        if start.elapsed() >= limit {
                            cut.store(true, Ordering::Relaxed);
                            return;
                        }
                        rng.refill();
                        let jx = ((k % grid) as f32 + rng.next()) / grid as f32;
                        let jy = ((k / grid) as f32 + rng.next()) / grid as f32;
                        let s = 2.0 * (x as f32 + jx) / size as f32 - 1.0;
                        let t = 2.0 * (y as f32 + jy) / size as f32 - 1.0;
                        let d = face_dir(face, s, t).normalize();
                        let c = self.trace(origin, d, 0, &mut rng, false);
                        cell.0[0] += clean(c[0]);
                        cell.0[1] += clean(c[1]);
                        cell.1 += 1;
                    }
                }
            });
            if cut.into_inner() {
                break;
            }
        }
        let face_px = size * size;
        let least = acc.iter().map(|c| c.1).min().unwrap_or(0);
        let mut out: [[Vec<Vec3A>; 6]; 2] = Default::default();
        if least == 0 {
            let faces_done = acc.chunks(face_px).filter(|f| f.iter().all(|c| c.1 > 0)).count();
            return (out, Outcome::Skipped { faces_done });
        }
        for (i, (sum, count)) in acc.into_iter().enumerate() {
            let face = i / face_px;
            let k = count as f32 * 255.0;
            out[0][face].push(sum[0] / k);
            out[1][face].push(sum[1] / k);
        }
        let outcome = if least as usize >= n { Outcome::Full } else { Outcome::Partial { least, of: n as u32 } };
        (out, outcome)
    }
}

impl Scene {
    pub(crate) fn render_sky(&self, size: u32) -> [[Vec<Vec3A>; 6]; 2] {
        let size = size.max(1) as usize;
        let grid = 4usize;
        let mut out: [[Vec<Vec3A>; 6]; 2] = Default::default();
        #[allow(clippy::needless_range_loop)]
        for face in 0..6 {
            for y in 0..size {
                for x in 0..size {
                    let mut sum = [Vec3A::ZERO; 2];
                    for k in 0..grid * grid {
                        let s = 2.0 * (x as f32 + ((k % grid) as f32 + 0.5) / grid as f32) / size as f32 - 1.0;
                        let t = 2.0 * (y as f32 + ((k / grid) as f32 + 0.5) / grid as f32) / size as f32 - 1.0;
                        let d = face_dir(face, s, t).normalize();
                        sum[0] += self.sky(d, 0);
                        sum[1] += self.sky(d, 1);
                    }
                    let n = (grid * grid) as f32 * 255.0;
                    out[0][face].push(sum[0] / n);
                    out[1][face].push(sum[1] / n);
                }
            }
        }
        out
    }
}
