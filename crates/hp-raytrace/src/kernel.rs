use glam::Vec3A;

use crate::bvh::{Bvh, Hit};
use crate::sampling::{self, Rng, cone, cosine, luminance, r1, r2};
use crate::scene::{LightKind, MAX_NORMALS, MAX_PASSES, MAX_SLOTS, MaterialKind, NONE, Scene};

pub const MAX_TRACE: f32 = 56_755.84;
pub const ACC: usize = MAX_PASSES * MAX_SLOTS * MAX_NORMALS;
pub const SURFACE_EPSILON: f32 = 0.031_25;
pub const BOUNCE_OFFSET: f32 = 0.125;
pub const MAX_CROSSINGS: u32 = 8;
pub const DIM_POINT: u32 = 0;
pub const DIM_SKY: u32 = 1;
pub const DIM_AO: u32 = 2;
pub const DIM_PATH: u32 = 3;
pub const DIM_AO_REACH: u32 = 4;
pub const CONTACT_REACH: f32 = 1.5;
pub const DIM_LIGHT: u32 = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Attr {
    pub material: u32,
    pub surface: u32,
    pub uv: [[f32; 2]; 3],
    pub lm: [[f32; 2]; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SurfInfo {
    pub width: u32,
    pub height: u32,
    pub normals: u32,
    pub slots: u32,
    pub styles: [u8; MAX_SLOTS],
    pub first_luxel: u32,
    pub lights: (u32, u32),
    pub in_skybox: bool,
    pub cache: u32,
}

impl SurfInfo {
    pub fn slot_of(&self, style: u8) -> Option<usize> {
        self.styles[..self.slots as usize].iter().position(|s| *s == style)
    }

    pub fn luxels(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    pub seed: u32,
    pub bounces: u32,
    pub sky_samples: u32,
    pub style_bounce: bool,
    pub ao_radius: f32,
    pub contact_ao: bool,
    pub clamp: f32,
}

pub struct Packed {
    pub opaque: Bvh,
    pub clear: Bvh,
    pub attrs: Vec<Attr>,
    pub surfaces: Vec<SurfInfo>,
    pub bounce_lights: Vec<u32>,
    pub bounce_cdf: Vec<f32>,
    pub sky_light: Option<u32>,
    pub cache: Vec<[f32; 3]>,
    pub cache_valid: Vec<u8>,
    pub passes: usize,
}

fn region_angle(n0: Vec3A, nk: Vec3A) -> f32 {
    let c = n0.dot(nk).clamp(-1.0, 1.0);
    (std::f32::consts::TAU - 2.0 * c.acos()).max(1e-3)
}

impl Packed {
    pub fn new(scene: &Scene) -> Packed {
        let mut opaque_tris = Vec::new();
        let mut opaque_ids = Vec::new();
        let mut clear_tris = Vec::new();
        let mut clear_ids = Vec::new();
        let mut attrs = Vec::with_capacity(scene.triangles.len());
        for (i, t) in scene.triangles.iter().enumerate() {
            let a = Vec3A::from_array(t.p[0]);
            let b = Vec3A::from_array(t.p[1]);
            let c = Vec3A::from_array(t.p[2]);
            let n = (b - a).cross(c - a);
            let len = n.length();
            if len.is_nan() || len <= 1e-9 || !len.is_finite() {
                attrs.push(Attr::default());
                continue;
            }
            attrs.push(Attr {
                material: t.material,
                surface: t.surface,
                uv: t.uv,
                lm: t.lm,
                normal: (n / len).to_array(),
            });
            let kind = scene.materials.get(t.material as usize).map(|m| m.kind).unwrap_or_default();
            match kind {
                MaterialKind::Water => {}
                MaterialKind::Translucent => {
                    clear_tris.push(t.p);
                    clear_ids.push(i as u32);
                }
                _ => {
                    opaque_tris.push(t.p);
                    opaque_ids.push(i as u32);
                }
            }
        }
        let (opaque, clear) =
            rayon::join(|| Bvh::build(&opaque_tris, &opaque_ids), || Bvh::build(&clear_tris, &clear_ids));
        let mut cache_at = 0u32;
        let passes = scene.passes as usize;
        let surfaces: Vec<SurfInfo> = scene
            .surfaces
            .iter()
            .map(|s| {
                let mut styles = [0u8; MAX_SLOTS];
                for (k, st) in s.styles.iter().take(MAX_SLOTS).enumerate() {
                    styles[k] = *st;
                }
                let info = SurfInfo {
                    width: s.width,
                    height: s.height,
                    normals: s.normals,
                    slots: s.styles.len().clamp(1, MAX_SLOTS) as u32,
                    styles,
                    first_luxel: s.first_luxel,
                    lights: s.lights,
                    in_skybox: s.in_skybox,
                    cache: cache_at,
                };
                cache_at += (passes * info.slots as usize * info.luxels()) as u32;
                info
            })
            .collect();
        let mut bounce_lights = Vec::new();
        let mut bounce_cdf = Vec::new();
        let mut total = 0.0f32;
        for (i, l) in scene.lights.iter().enumerate() {
            if l.kind == LightKind::SkyAmbient {
                continue;
            }
            let w = l.intensity[0][0].max(l.intensity[0][1]).max(l.intensity[0][2]).max(0.0);
            let w = if l.kind == LightKind::Sun { w * 4.0 } else { w };
            if w > 0.0 && w.is_finite() {
                total += w;
                bounce_lights.push(i as u32);
                bounce_cdf.push(total);
            }
        }
        if total > 0.0 {
            for c in &mut bounce_cdf {
                *c /= total;
            }
        }
        Packed {
            opaque,
            clear,
            attrs,
            surfaces,
            bounce_lights,
            bounce_cdf,
            sky_light: scene.lights.iter().position(|l| l.kind == LightKind::SkyAmbient).map(|i| i as u32),
            cache: vec![[0.0; 3]; cache_at as usize],
            cache_valid: Vec::new(),
            passes,
        }
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

pub struct Ctx<'a> {
    pub scene: &'a Scene,
    pub packed: &'a Packed,
    pub tuning: Tuning,
}

#[derive(Clone, Copy)]
pub struct Acc {
    pub v: [Vec3A; ACC],
}

impl Default for Acc {
    fn default() -> Self {
        Acc { v: [Vec3A::ZERO; ACC] }
    }
}

#[inline]
fn at(nslots: usize, nn: usize, pass: usize, slot: usize, n: usize) -> usize {
    (pass * nslots + slot) * nn + n
}

impl Ctx<'_> {
    fn material(&self, id: u32) -> (&crate::scene::Material, &Attr) {
        let a = &self.packed.attrs[id as usize];
        (&self.scene.materials[a.material as usize], a)
    }

    fn alpha_at(&self, id: u32, u: f32, v: f32) -> f32 {
        let (m, a) = self.material(id);
        if m.alpha == NONE {
            return 1.0;
        }
        let w = 1.0 - u - v;
        let tu = a.uv[0][0] * w + a.uv[1][0] * u + a.uv[2][0] * v;
        let tv = a.uv[0][1] * w + a.uv[1][1] * u + a.uv[2][1] * v;
        self.scene.alpha_textures[m.alpha as usize].sample(tu, tv)
    }

    fn solid(&self, id: u32, u: f32, v: f32) -> bool {
        let (m, _) = self.material(id);
        if m.alpha == NONE {
            return true;
        }
        self.alpha_at(id, u, v) >= m.alpha_cutoff
    }

    fn priority(&self, id: u32) -> u32 {
        let (m, a) = self.material(id);
        match m.kind {
            MaterialKind::Blocker => 0,
            MaterialKind::Sky => 1,
            _ if a.surface != NONE => 3,
            _ => 2,
        }
    }

    pub fn transmit(&self, o: Vec3A, d: Vec3A, tmax: f32) -> Vec3A {
        if self.packed.clear.tris.is_empty() {
            return Vec3A::ONE;
        }
        let t = std::cell::Cell::new(Vec3A::ONE);
        self.packed.clear.any(o, d, 1e-3, tmax, &|id, u, v, _| {
            let (m, _) = self.material(id);
            let a = self.alpha_at(id, u, v);
            let tr = Vec3A::from_array(m.transmit);
            let f = Vec3A::ONE - (Vec3A::ONE - tr) * a;
            let now = t.get() * f;
            t.set(now);
            now.max_element() < 1e-3
        });
        let r = t.get();
        if r.max_element() < 1e-3 { Vec3A::ZERO } else { r }
    }

    pub fn visible(&self, o: Vec3A, d: Vec3A, dist: f32) -> Vec3A {
        if dist <= 1e-3 {
            return Vec3A::ONE;
        }
        if self.packed.opaque.any(o, d, 1e-3, dist, &|id, u, v, _| self.solid(id, u, v)) {
            return Vec3A::ZERO;
        }
        self.transmit(o, d, dist)
    }

    fn sky_once(&self, o: Vec3A, d: Vec3A) -> Vec3A {
        let Some(h) = self.closest(o, d, MAX_TRACE) else { return Vec3A::ZERO };
        let (m, _) = self.material(h.id);
        if m.kind != MaterialKind::Sky {
            return Vec3A::ZERO;
        }
        self.transmit(o, d, h.t)
    }

    pub fn sky_visible(&self, o: Vec3A, d: Vec3A, in_skybox: bool) -> Vec3A {
        let mut v = self.sky_once(o, d);
        if v.max_element() <= 0.0 || in_skybox {
            return v;
        }
        for c in &self.scene.sky_cameras {
            let p = Vec3A::from_array(c.origin) + o * c.inv_scale;
            v *= self.sky_once(p, d);
            if v.max_element() <= 0.0 {
                return Vec3A::ZERO;
            }
        }
        v
    }

    pub fn closest(&self, o: Vec3A, d: Vec3A, tmax: f32) -> Option<Hit> {
        self.packed.opaque.closest(o, d, 1e-3, tmax, &|id, u, v| self.solid(id, u, v), &|id| self.priority(id))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn light_contrib(
        &self,
        li: usize,
        pos: Vec3A,
        normals: &[Vec3A; MAX_NORMALS],
        nn: usize,
        jitter: (f32, f32),
        in_skybox: bool,
        out: &mut [Vec3A; MAX_NORMALS],
    ) -> bool {
        let l = &self.scene.lights[li];
        let n0 = normals[0];
        *out = [Vec3A::ZERO; MAX_NORMALS];
        match l.kind {
            LightKind::Point | LightKind::Spot { .. } | LightKind::Surface => {
                let mut lp = Vec3A::from_array(l.position);
                let center = lp;
                if l.size > 0.0 {
                    let to = (center - pos).normalize_or(Vec3A::Z);
                    let (t, b) = sampling::frame(to);
                    let (dx, dy) = sampling::disk(jitter.0, jitter.1);
                    lp += (t * dx + b * dy) * l.size;
                }
                let delta = center - pos;
                let dist = delta.length();
                if dist.is_nan() || dist <= 1e-4 || dist > l.reach {
                    return false;
                }
                let dir = delta / dist;
                if dir.dot(n0) <= 0.0 {
                    return false;
                }
                let fall = l.falloff(dir, dist);
                if fall <= 0.0 {
                    return false;
                }
                let target = if l.kind == LightKind::Surface {
                    lp + Vec3A::from_array(l.direction) * SURFACE_EPSILON
                } else {
                    lp
                };
                let to = target - pos;
                let len = to.length();
                let sdir = if len > 1e-4 { to / len } else { dir };
                let vis = if len > 1e-4 { self.visible(pos, sdir, len) } else { Vec3A::ONE };
                if vis.max_element() <= 0.0 {
                    return false;
                }
                let v = vis * fall;
                out[0] = v * dir.dot(n0);
                for k in 1..nn {
                    out[k] = v * dir.dot(normals[k]).max(0.0);
                }
                true
            }
            LightKind::Sun => {
                let axis = -Vec3A::from_array(l.direction);
                let dir = cone(axis, l.size, jitter.0, jitter.1);
                let dot = dir.dot(n0);
                if dot <= 0.0 {
                    return false;
                }
                let vis = self.sky_visible(pos, dir, in_skybox);
                if vis.max_element() <= 0.0 {
                    return false;
                }
                out[0] = vis * dot;
                for k in 1..nn {
                    let c = normals[k].dot(dir).max(0.0);
                    out[k] = vis * ((c + c * c) * 0.5);
                }
                true
            }
            LightKind::SkyAmbient => false,
        }
    }

    pub fn direct_sample(&self, si: usize, luxel: u32, k: u32, acc: &mut Acc) -> f32 {
        let s = &self.packed.surfaces[si];
        let lux = &self.scene.luxels[luxel as usize];
        if lux.points == 0 {
            return 1.0;
        }
        let seed = self.tuning.seed;
        let pi = (r1(k, seed, luxel, DIM_POINT) * lux.points as f32) as u32;
        let p = &self.scene.points[(lux.first_point + pi.min(lux.points - 1)) as usize];
        let pos = Vec3A::from_array(p.pos);
        let normals = p.normals.map(Vec3A::from_array);
        let nn = s.normals as usize;
        let ns = s.slots as usize;
        let passes = self.packed.passes;
        let mut f = [Vec3A::ZERO; MAX_NORMALS];
        for j in 0..s.lights.1 {
            let li = self.scene.light_lists[(s.lights.0 + j) as usize] as usize;
            let l = &self.scene.lights[li];
            let Some(slot) = s.slot_of(l.style) else { continue };
            if l.kind == LightKind::SkyAmbient {
                self.sky_ambient(li, pos, &normals, nn, s.in_skybox, luxel, k, &mut f);
            } else {
                let jitter = r2(k, seed, luxel, DIM_LIGHT + j);
                if !self.light_contrib(li, pos, &normals, nn, jitter, s.in_skybox, &mut f) {
                    continue;
                }
            }
            if f[..nn].iter().all(|v| v.max_element() <= 0.0) {
                continue;
            }
            for (pass, intensity) in l.intensity.iter().enumerate().take(passes) {
                let i = Vec3A::from_array(*intensity);
                for (n, fv) in f.iter().enumerate().take(nn) {
                    acc.v[at(ns, nn, pass, slot, n)] += i * *fv;
                }
            }
        }
        if self.tuning.ao_radius > 0.0 {
            let (u, v) = r2(k, seed, luxel, DIM_AO);
            let d = cosine(normals[0], u, v);
            let base = pos - normals[0] * 0.75;
            let reach = if self.tuning.contact_ao {
                let w = r1(k, seed, luxel, DIM_AO_REACH);
                self.tuning.ao_radius * CONTACT_REACH * (1.0 - (1.0 - w).max(0.0).sqrt())
            } else {
                self.tuning.ao_radius
            };
            if reach > 0.25 && self.packed.opaque.any(base, d, 0.25, reach, &|id, u, v, _| self.solid(id, u, v)) {
                0.0
            } else {
                1.0
            }
        } else {
            1.0
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sky_ambient(
        &self,
        _li: usize,
        pos: Vec3A,
        normals: &[Vec3A; MAX_NORMALS],
        nn: usize,
        in_skybox: bool,
        luxel: u32,
        k: u32,
        out: &mut [Vec3A; MAX_NORMALS],
    ) {
        *out = [Vec3A::ZERO; MAX_NORMALS];
        let count = self.tuning.sky_samples.max(1);
        let n0 = normals[0];
        for j in 0..count {
            let (u, v) = r2(k * count + j, self.tuning.seed, luxel, DIM_SKY);
            let d = if nn > 1 {
                let lobe = ((v * 4.0) as usize).min(3);
                let vv = v * 4.0 - lobe as f32;
                cosine(normals[lobe], u, vv)
            } else {
                cosine(n0, u, v)
            };
            let c0 = d.dot(n0);
            if c0 <= 0.0 {
                continue;
            }
            let vis = self.sky_visible(pos, d, in_skybox);
            if vis.max_element() <= 0.0 {
                continue;
            }
            let w = vis / count as f32;
            if nn == 1 {
                out[0] += w;
            } else {
                let cs = [c0, d.dot(normals[1]).max(0.0), d.dot(normals[2]).max(0.0), d.dot(normals[3]).max(0.0)];
                let sum: f32 = cs.iter().sum();
                if sum <= 1e-6 {
                    continue;
                }
                for n in 0..4 {
                    let omega = region_angle(n0, normals[n]);
                    out[n] += w * (2.0 / omega) * std::f32::consts::TAU * 2.0 * cs[n] / sum;
                }
            }
        }
    }

    pub fn cache_lookup(&self, si: usize, s: f32, t: f32, pass: usize, slot: usize) -> Vec3A {
        let info = &self.packed.surfaces[si];
        let (w, h) = (info.width as usize, info.height as usize);
        if w == 0 || h == 0 {
            return Vec3A::ZERO;
        }
        let x = s.clamp(0.0, (w - 1) as f32);
        let y = t.clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let base = info.cache as usize + (pass * info.slots as usize + slot) * w * h;
        let valid = |i: usize| self.packed.cache_valid.get(info.first_luxel as usize + i).is_none_or(|v| *v != 0);
        let mut sum = Vec3A::ZERO;
        let mut wsum = 0.0f32;
        for (xx, yy, wt) in
            [(x0, y0, (1.0 - fx) * (1.0 - fy)), (x1, y0, fx * (1.0 - fy)), (x0, y1, (1.0 - fx) * fy), (x1, y1, fx * fy)]
        {
            let i = yy * w + xx;
            if wt > 0.0 && valid(i) {
                sum += Vec3A::from_array(self.packed.cache[base + i]) * wt;
                wsum += wt;
            }
        }
        if wsum > 1e-6 { sum / wsum } else { Vec3A::ZERO }
    }

    fn bounce_nee(
        &self,
        pos: Vec3A,
        n: Vec3A,
        rng: &mut Rng,
        in_skybox: bool,
        out: &mut [[Vec3A; MAX_SLOTS]; MAX_PASSES],
        origin: &SurfInfo,
    ) {
        let passes = self.packed.passes;
        let normals = [n; MAX_NORMALS];
        let mut f = [Vec3A::ZERO; MAX_NORMALS];
        if !self.packed.bounce_lights.is_empty() {
            let x = rng.uniform();
            let k = self.packed.bounce_cdf.partition_point(|c| *c < x).min(self.packed.bounce_cdf.len() - 1);
            let prev = if k == 0 { 0.0 } else { self.packed.bounce_cdf[k - 1] };
            let pdf = (self.packed.bounce_cdf[k] - prev).max(1e-8);
            let li = self.packed.bounce_lights[k] as usize;
            let l = &self.scene.lights[li];
            if let Some(slot) = origin.slot_of(l.style).filter(|_| l.style == 0 || self.tuning.style_bounce)
                && self.light_contrib(li, pos, &normals, 1, (rng.uniform(), rng.uniform()), in_skybox, &mut f)
            {
                for (pass, o) in out.iter_mut().enumerate().take(passes) {
                    o[slot] += Vec3A::from_array(l.intensity[pass]) * f[0] / pdf;
                }
            }
        }
        if let Some(li) = self.packed.sky_light {
            let l = &self.scene.lights[li as usize];
            let slot = origin.slot_of(l.style).filter(|_| l.style == 0 || self.tuning.style_bounce);
            let d = cosine(n, rng.uniform(), rng.uniform());
            if let Some(slot) = slot {
                let vis = self.sky_visible(pos, d, in_skybox);
                if vis.max_element() > 0.0 {
                    for (pass, o) in out.iter_mut().enumerate().take(passes) {
                        o[slot] += Vec3A::from_array(l.intensity[pass]) * vis;
                    }
                }
            }
        }
    }

    #[allow(clippy::needless_range_loop)]
    pub fn indirect_sample(&self, si: usize, luxel: u32, k: u32, acc: &mut Acc) {
        let s = &self.packed.surfaces[si];
        let lux = &self.scene.luxels[luxel as usize];
        if lux.points == 0 || self.tuning.bounces == 0 {
            return;
        }
        let seed = self.tuning.seed;
        let pi = (r1(k, seed ^ 0x9e37, luxel, DIM_POINT) * lux.points as f32) as u32;
        let p = &self.scene.points[(lux.first_point + pi.min(lux.points - 1)) as usize];
        let normals = p.normals.map(Vec3A::from_array);
        let n0 = normals[0];
        let nn = s.normals as usize;
        let ns = s.slots as usize;
        let passes = self.packed.passes;
        let (u, v) = r2(k, seed, luxel, DIM_PATH);
        let dir = if nn > 1 {
            let lobe = ((v * 4.0) as usize).min(3);
            cosine(normals[lobe], u, v * 4.0 - lobe as f32)
        } else {
            cosine(n0, u, v)
        };
        let c0 = dir.dot(n0);
        if c0 <= 0.0 {
            return;
        }
        let mut w = [0.0f32; MAX_NORMALS];
        if nn == 1 {
            w[0] = 1.0;
        } else {
            let cs = [c0, dir.dot(normals[1]).max(0.0), dir.dot(normals[2]).max(0.0), dir.dot(normals[3]).max(0.0)];
            let sum: f32 = cs.iter().sum();
            if sum <= 1e-6 {
                return;
            }
            for n in 0..4 {
                w[n] = 4.0 * cs[n] / sum;
            }
        }
        let mut rng = Rng::new(seed, luxel, k.wrapping_mul(0x2545_f491) ^ 0x51ed);
        let mut origin = Vec3A::from_array(p.pos);
        let mut d = dir;
        let mut thr = Vec3A::ONE;
        let in_skybox = s.in_skybox;
        let mut sample = [[Vec3A::ZERO; MAX_SLOTS]; MAX_PASSES];
        for depth in 0..self.tuning.bounces {
            let Some(h) = self.closest(origin, d, MAX_TRACE) else { break };
            thr *= self.transmit(origin, d, h.t);
            if thr.max_element() <= 0.0 {
                break;
            }
            let (m, a) = self.material(h.id);
            if matches!(m.kind, MaterialKind::Sky | MaterialKind::Blocker) {
                break;
            }
            let gn = Vec3A::from_array(a.normal);
            let front = d.dot(gn) < 0.0;
            if !front && !m.two_sided {
                break;
            }
            let facing = if front { gn } else { -gn };
            let hit = origin + d * h.t;
            let albedo = Vec3A::from_array(m.albedo);
            let mut add = [[Vec3A::ZERO; MAX_SLOTS]; MAX_PASSES];
            if a.surface != NONE {
                let hs = &self.packed.surfaces[a.surface as usize];
                let bw = 1.0 - h.u - h.v;
                let ls = a.lm[0][0] * bw + a.lm[1][0] * h.u + a.lm[2][0] * h.v;
                let lt = a.lm[0][1] * bw + a.lm[1][1] * h.u + a.lm[2][1] * h.v;
                for hslot in 0..hs.slots as usize {
                    let style = hs.styles[hslot];
                    if style != 0 && !self.tuning.style_bounce {
                        continue;
                    }
                    let Some(slot) = s.slot_of(style) else { continue };
                    for (pass, o) in add.iter_mut().enumerate().take(passes) {
                        o[slot] += self.cache_lookup(a.surface as usize, ls, lt, pass, hslot);
                    }
                }
            } else {
                self.bounce_nee(hit + facing * BOUNCE_OFFSET, facing, &mut rng, in_skybox, &mut add, s);
            }
            let scale = thr * albedo;
            for pass in 0..passes {
                for slot in 0..ns {
                    sample[pass][slot] += add[pass][slot] * scale;
                }
            }
            thr = scale;
            if depth >= 1 {
                let q = thr.max_element().min(0.95);
                if q <= 0.0 || rng.uniform() >= q {
                    break;
                }
                thr /= q;
            }
            origin = hit + facing * BOUNCE_OFFSET;
            d = cosine(facing, rng.uniform(), rng.uniform());
        }
        let clamp = self.tuning.clamp;
        for pass in 0..passes {
            for slot in 0..ns {
                let mut c = sample[pass][slot];
                let l = luminance(c);
                if clamp > 0.0 && l > clamp {
                    c *= clamp / l;
                }
                for n in 0..nn {
                    acc.v[at(ns, nn, pass, slot, n)] += c * w[n];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Light, Luxel, Material, Point, Surface, Triangle};

    fn quad(scene: &mut Scene, c: [Vec3A; 4], material: u32, surface: u32) {
        for t in [[c[0], c[1], c[2]], [c[0], c[2], c[3]]] {
            scene.triangles.push(Triangle { p: t.map(|v| v.to_array()), material, surface, ..Triangle::default() });
        }
    }

    fn cube_scene(albedo: f32) -> Scene {
        let mut scene = Scene { passes: 1, ..Scene::default() };
        scene.materials.push(Material { albedo: [albedo; 3], ..Material::default() });
        let s = 100.0f32;
        let v = |x: f32, y: f32, z: f32| Vec3A::new(x, y, z) * s;
        let faces = [
            [v(0., 0., 0.), v(0., 1., 0.), v(1., 1., 0.), v(1., 0., 0.)],
            [v(0., 0., 1.), v(1., 0., 1.), v(1., 1., 1.), v(0., 1., 1.)],
            [v(0., 0., 0.), v(1., 0., 0.), v(1., 0., 1.), v(0., 0., 1.)],
            [v(0., 1., 0.), v(0., 1., 1.), v(1., 1., 1.), v(1., 1., 0.)],
            [v(0., 0., 0.), v(0., 0., 1.), v(0., 1., 1.), v(0., 1., 0.)],
            [v(1., 0., 0.), v(1., 1., 0.), v(1., 1., 1.), v(1., 0., 1.)],
        ];
        let mid = Vec3A::splat(s * 0.5);
        for (i, f) in faces.iter().enumerate() {
            let inward = (f[1] - f[0]).cross(f[2] - f[0]).dot(mid - f[0]) > 0.0;
            let f = &if inward { *f } else { [f[0], f[3], f[2], f[1]] };
            quad(&mut scene, *f, 0, i as u32);
            scene.surfaces.push(Surface {
                width: 1,
                height: 1,
                normals: 1,
                styles: vec![0],
                first_luxel: i as u32,
                lights: (0, 0),
                in_skybox: false,
                minlight: 0.0,
            });
            let n = (f[1] - f[0]).cross(f[2] - f[0]).normalize();
            let center = (f[0] + f[2]) * 0.5;
            scene.luxels.push(Luxel { first_point: i as u32, points: 1 });
            scene.points.push(Point { pos: (center + n * 1.0).to_array(), normals: [n.to_array(); 4] });
        }
        scene
    }

    #[test]
    fn closed_box_conserves_energy_like_a_furnace() {
        for albedo in [0.3f32, 0.5, 0.7] {
            let scene = cube_scene(albedo);
            let mut packed = Packed::new(&scene);
            packed.cache = vec![[1.0; 3]; packed.cache_len()];
            packed.cache_valid = vec![1; scene.luxels.len()];
            let ctx = Ctx {
                scene: &scene,
                packed: &packed,
                tuning: Tuning {
                    seed: 3,
                    bounces: 64,
                    sky_samples: 1,
                    style_bounce: true,
                    ao_radius: 0.0,
                    contact_ao: false,
                    clamp: 0.0,
                },
            };
            let n = 40_000u32;
            let mut acc = Acc::default();
            for k in 0..n {
                ctx.indirect_sample(0, 0, k, &mut acc);
            }
            let got = acc.v[0].x / n as f32;
            let want = albedo / (1.0 - albedo);
            assert!((got - want).abs() < want * 0.03, "albedo {albedo}: {got} vs {want}");
        }
    }

    #[test]
    fn sun_size_softens_shadow_edges() {
        let mut scene = Scene { passes: 1, ..Scene::default() };
        scene.materials.push(Material::default());
        scene.materials.push(Material { kind: MaterialKind::Sky, ..Material::default() });
        let v = |x: f32, y: f32, z: f32| Vec3A::new(x, y, z);
        quad(
            &mut scene,
            [v(-50.0, -500.0, 100.0), v(-50.0, 500.0, 100.0), v(500.0, 500.0, 100.0), v(500.0, -500.0, 100.0)],
            0,
            NONE,
        );
        quad(
            &mut scene,
            [
                v(-5000.0, -5000.0, 3000.0),
                v(5000.0, -5000.0, 3000.0),
                v(5000.0, 5000.0, 3000.0),
                v(-5000.0, 5000.0, 3000.0),
            ],
            1,
            NONE,
        );
        let profile = |size: f32| -> Vec<f32> {
            let mut s = scene.clone();
            s.lights = vec![Light {
                kind: LightKind::Sun,
                direction: [0.0, 0.0, -1.0],
                size,
                intensity: [[1.0; 3]; 2],
                ..Light::default()
            }];
            let packed = Packed::new(&s);
            let ctx = Ctx {
                scene: &s,
                packed: &packed,
                tuning: Tuning {
                    seed: 1,
                    bounces: 0,
                    sky_samples: 1,
                    style_bounce: true,
                    ao_radius: 0.0,
                    contact_ao: false,
                    clamp: 0.0,
                },
            };
            (0..21)
                .map(|i| {
                    let x = -100.0 + i as f32 * 5.0;
                    let mut sum = 0.0;
                    for k in 0..256u32 {
                        let mut f = [Vec3A::ZERO; MAX_NORMALS];
                        let j = r2(k, 9, i, 0);
                        if ctx.light_contrib(0, v(x, 0.0, 1.0), &[Vec3A::Z; 4], 1, j, false, &mut f) {
                            sum += f[0].x;
                        }
                    }
                    sum / 256.0
                })
                .collect()
        };
        let hard = profile(0.0);
        let soft = profile(0.25);
        assert!(hard.iter().all(|v| *v < 1e-3 || *v > 0.999), "{hard:?}");
        let partial = soft.iter().filter(|v| **v > 0.05 && **v < 0.95).count();
        assert!(partial >= 6, "{soft:?}");
        assert!(soft[0] > 0.95 && soft[20] < 0.01, "{soft:?}");
    }
}
