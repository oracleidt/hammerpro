use glam::{DVec2, Vec3A};
use rayon::prelude::*;

use super::RadQuality;
use super::bake::{Ctx, FaceLight};
use super::props::{CLASS_PROP, CLASS_WORLD, NO_PROP};
use super::surface::{LitFace, inside};

pub const DEFAULT_RADIUS: f32 = 96.0;
pub const DEFAULT_STRENGTH: f32 = 0.75;
pub const RADIUS_RANGE: (f32, f32) = (4.0, 1024.0);
pub const SAMPLES_RANGE: (u32, u32) = (4, 512);
const INTERPOLATE: f32 = 0.02;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AoOptions {
    pub enabled: Option<bool>,
    pub by_default: bool,
    pub radius: Option<f32>,
    pub strength: Option<f32>,
    pub samples: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ao {
    pub radius: f32,
    pub strength: f32,
    dirs: Vec<Vec3A>,
}

fn default_samples(q: RadQuality) -> u32 {
    match q {
        RadQuality::Draft => 16,
        RadQuality::Normal => 32,
        RadQuality::Final => 64,
    }
}

fn key_f32(world: Option<&hp_bsp::Entity>, key: &str) -> Option<f32> {
    world.and_then(|e| e.get(key)).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite())
}

impl AoOptions {
    pub(crate) fn resolve(&self, quality: RadQuality, world: Option<&hp_bsp::Entity>) -> Option<Ao> {
        let enabled = self
            .enabled
            .or_else(|| key_f32(world, "_ao").map(|v| v != 0.0))
            .unwrap_or(self.by_default && quality != RadQuality::Draft);
        if !enabled {
            return None;
        }
        let radius = self
            .radius
            .or_else(|| key_f32(world, "_aoradius"))
            .filter(|r| r.is_finite() && *r > 0.0)
            .unwrap_or(DEFAULT_RADIUS);
        let strength = self
            .strength
            .or_else(|| key_f32(world, "_aostrength"))
            .filter(|s| s.is_finite())
            .unwrap_or(DEFAULT_STRENGTH);
        let samples = self
            .samples
            .or_else(|| key_f32(world, "_aosamples").map(|v| v.max(0.0) as u32))
            .unwrap_or(default_samples(quality));
        let strength = strength.clamp(0.0, 1.0);
        if strength <= 0.0 {
            return None;
        }
        Some(Ao::new(
            radius.clamp(RADIUS_RANGE.0, RADIUS_RANGE.1),
            strength,
            samples.clamp(SAMPLES_RANGE.0, SAMPLES_RANGE.1) as usize,
        ))
    }
}

pub(crate) struct Factors {
    pub flat: Vec<Vec<f32>>,
    pub disp: Vec<Vec<f32>>,
}

pub(crate) fn resolve_and_log(
    opts: &super::RadOptions,
    world: Option<&hp_bsp::Entity>,
    log: &mut dyn FnMut(&str),
) -> Option<Ao> {
    let ao = opts.ao.resolve(opts.quality, world)?;
    if opts.bounces == 0 {
        log("ambient occlusion: off (it darkens the bounced light, and -bounce 0 leaves none)");
        return None;
    }
    log(&format!("ambient occlusion: radius {} units, strength {}, {} rays", ao.radius, ao.strength, ao.samples()));
    Some(ao)
}

fn basis(n: Vec3A) -> (Vec3A, Vec3A) {
    let a = if n.x.abs() > 0.9 { Vec3A::Y } else { Vec3A::X };
    let t = n.cross(a).normalize_or(Vec3A::Y);
    (t, n.cross(t))
}

impl Ao {
    pub(crate) fn new(radius: f32, strength: f32, samples: usize) -> Ao {
        let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
        let n = samples.max(1);
        let dirs = (0..n)
            .map(|i| {
                let u = (i as f32 + 0.5) / n as f32;
                let r = u.sqrt();
                let phi = golden * i as f32;
                Vec3A::new(r * phi.cos(), r * phi.sin(), (1.0 - u).max(0.0).sqrt())
            })
            .collect();
        Ao { radius, strength, dirs }
    }

    pub(crate) fn samples(&self) -> usize {
        self.dirs.len()
    }

    fn occlusion(&self, ctx: &Ctx<'_>, pos: Vec3A, normal: Vec3A, surface: Vec3A, skip: u32) -> f32 {
        let (t, b) = basis(normal);
        let (mut sum, mut count) = (0.0f32, 0u32);
        let inv = 1.0 / self.radius;
        for d in &self.dirs {
            let dir = t * d.x + b * d.y + normal * d.z;
            if dir.dot(surface) <= 0.01 {
                continue;
            }
            count += 1;
            let hit = if skip == NO_PROP {
                ctx.shadow.closest_in(CLASS_WORLD | CLASS_PROP, pos, dir, self.radius, |_| true)
            } else {
                ctx.shadow.closest_in(CLASS_WORLD | CLASS_PROP, pos, dir, self.radius, |p| ctx.props.prop_of(p) != skip)
            };
            if let Some(h) = hit {
                let x = 1.0 - (h.t * inv).clamp(0.0, 1.0);
                sum += x;
            }
        }
        if count == 0 { 0.0 } else { sum / count as f32 }
    }

    pub(crate) fn vertex(&self, ctx: &Ctx<'_>, pos: Vec3A, normal: Vec3A, prop: u32) -> f32 {
        let n = normal.normalize_or_zero();
        if n == Vec3A::ZERO || !pos.is_finite() {
            return 1.0;
        }
        1.0 - self.strength * self.occlusion(ctx, pos + n, n, n, prop)
    }

    pub(crate) fn faces(&self, ctx: &Ctx<'_>, light: &[FaceLight]) -> Factors {
        let all: Vec<Vec<f32>> = ctx
            .faces
            .par_iter()
            .zip(light)
            .map(|(f, l)| {
                let k = self.face(ctx, f, l);
                ctx.work.add(f.luxels() as u64);
                k
            })
            .collect();
        let mut out = Factors { flat: Vec::with_capacity(all.len()), disp: Vec::with_capacity(all.len()) };
        for (f, k) in ctx.faces.iter().zip(all) {
            if f.disp.is_some() {
                out.flat.push(Vec::new());
                out.disp.push(k);
            } else {
                out.flat.push(k);
                out.disp.push(Vec::new());
            }
        }
        out
    }

    fn face(&self, ctx: &Ctx<'_>, f: &LitFace, l: &FaceLight) -> Vec<f32> {
        let lux = f.luxels();
        let mut occ = vec![0.0f32; lux];
        let mut valid = vec![false; lux];
        let mut normals = vec![Vec3A::ZERO; lux];
        let flat_normal = f.normal.as_vec3a();
        let mut points = vec![None; lux];
        for t in 0..f.h {
            for s in 0..f.w {
                let i = t * f.w + s;
                if l.dark.get(i).copied().unwrap_or(false) {
                    continue;
                }
                points[i] = point(ctx, f, s, t);
            }
        }
        let trace = |i: usize, occ: &mut [f32], valid: &mut [bool], normals: &mut [Vec3A]| {
            if let Some((pos, normal)) = points[i] {
                let surface = if f.disp.is_some() { normal } else { flat_normal };
                occ[i] = self.occlusion(ctx, pos, normal, surface, NO_PROP);
                valid[i] = true;
                normals[i] = normal;
            }
        };
        for t in (0..f.h).step_by(2) {
            for s in (0..f.w).step_by(2) {
                trace(t * f.w + s, &mut occ, &mut valid, &mut normals);
            }
        }
        for t in 0..f.h {
            for s in 0..f.w {
                let i = t * f.w + s;
                if (s % 2 == 0 && t % 2 == 0) || points[i].is_none() {
                    continue;
                }
                let offsets: &[(i64, i64)] = match (s % 2, t % 2) {
                    (1, 0) => &[(-1, 0), (1, 0)],
                    (0, 1) => &[(0, -1), (0, 1)],
                    _ => &[(-1, -1), (1, -1), (-1, 1), (1, 1)],
                };
                let (mut lo, mut hi, mut sum, mut n) = (f32::MAX, f32::MIN, 0.0f32, 0usize);
                for (ds, dt) in offsets {
                    let (x, y) = (s as i64 + ds, t as i64 + dt);
                    if x < 0 || y < 0 || x >= f.w as i64 || y >= f.h as i64 {
                        break;
                    }
                    let j = y as usize * f.w + x as usize;
                    if !valid[j] {
                        break;
                    }
                    (lo, hi, sum, n) = (lo.min(occ[j]), hi.max(occ[j]), sum + occ[j], n + 1);
                }
                if n == offsets.len() && hi - lo <= INTERPOLATE {
                    occ[i] = sum / n as f32;
                    valid[i] = true;
                    normals[i] = points[i].map_or(Vec3A::ZERO, |p| p.1);
                } else {
                    trace(i, &mut occ, &mut valid, &mut normals);
                }
            }
        }
        let occ = filter(f.w, f.h, &occ, &valid, &normals);
        occ.into_iter().map(|o| 1.0 - self.strength * o).collect()
    }
}

fn point(ctx: &Ctx<'_>, f: &LitFace, s: usize, t: usize) -> Option<(Vec3A, Vec3A)> {
    if let Some(d) = &f.disp {
        let step = |i: usize, n: usize| if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
        let (p, n) = d.sample(DVec2::new(step(s, f.w), step(t, f.h)), 1.0, f.normal);
        if !p.is_finite() || ctx.in_brush(p) {
            return None;
        }
        return Some((p.as_vec3a(), n.as_vec3a()));
    }
    let open = |c: DVec2| {
        let p = f.plane_point(c.x, c.y);
        (p.is_finite() && !ctx.in_brush(p + f.normal)).then_some(p)
    };
    let middle = DVec2::new(s as f64, t as f64);
    let at = if f.poly2.len() >= 3 && f.frame.is_some() && inside(&f.poly2, middle) {
        open(middle).or_else(|| open(f.flat_sample(s, t).0))
    } else {
        open(f.flat_sample(s, t).0)
    }?;
    let n = f.phong(at).as_vec3a();
    Some(((at + f.normal).as_vec3a(), n))
}

fn filter(w: usize, h: usize, occ: &[f32], valid: &[bool], normals: &[Vec3A]) -> Vec<f32> {
    let lux = w * h;
    if lux == 0 || !valid.iter().any(|v| *v) {
        return vec![0.0; lux];
    }
    let mut val = occ.to_vec();
    let mut have = valid.to_vec();
    let mut nrm = normals.to_vec();
    while have.iter().any(|v| !*v) {
        let (prev_val, prev_have, prev_nrm) = (val.clone(), have.clone(), nrm.clone());
        let mut changed = false;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if prev_have[i] {
                    continue;
                }
                let (mut sum, mut n, mut ns) = (0.0f32, 0u32, Vec3A::ZERO);
                for (dx, dy) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if prev_have[j] {
                        sum += prev_val[j];
                        ns += prev_nrm[j];
                        n += 1;
                    }
                }
                if n > 0 {
                    val[i] = sum / n as f32;
                    nrm[i] = ns.normalize_or_zero();
                    have[i] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut out = vec![0.0f32; lux];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let (mut sum, mut total) = (0.0f32, 0.0f32);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    let tent = ((2 - dx.abs()) * (2 - dy.abs())) as f32;
                    let alike = nrm[i].dot(nrm[j]).max(0.0).powi(8);
                    let wgt = tent * if i == j { 1.0 } else { alike };
                    sum += val[j] * wgt;
                    total += wgt;
                }
            }
            out[i] = if total > 0.0 { (sum / total).clamp(0.0, 1.0) } else { val[i] };
        }
    }
    out
}

pub(crate) fn disp_before(ctx: &Ctx<'_>, light: &[FaceLight]) -> Vec<Vec<Vec3A>> {
    ctx.faces
        .iter()
        .zip(light)
        .map(|(f, l)| {
            if f.disp.is_none() {
                return Vec::new();
            }
            let lux = f.luxels();
            let mut out = Vec::with_capacity(ctx.passes * l.normals * lux);
            for p in 0..ctx.passes {
                for n in 0..l.normals {
                    let at = l.index(p, 0, n, lux);
                    out.extend_from_slice(l.values.get(at..at + lux).unwrap_or(&[]));
                }
            }
            out
        })
        .collect()
}

pub(crate) fn apply_disps(ctx: &Ctx<'_>, light: &mut [FaceLight], before: &[Vec<Vec3A>], factors: &[Vec<f32>]) {
    light.par_iter_mut().enumerate().for_each(|(fi, l)| {
        let (Some(b), Some(k), Some(f)) = (before.get(fi), factors.get(fi), ctx.faces.get(fi)) else { return };
        let lux = f.luxels();
        if b.is_empty() || k.len() != lux || b.len() != ctx.passes * l.normals * lux {
            return;
        }
        for p in 0..ctx.passes {
            for n in 0..l.normals {
                let at = l.index(p, 0, n, lux);
                let base = &b[(p * l.normals + n) * lux..(p * l.normals + n + 1) * lux];
                for i in 0..lux {
                    let v = &mut l.values[at + i];
                    *v = base[i] + (*v - base[i]) * k[i];
                }
            }
        }
    });
}

pub(crate) fn summary(factors: &Factors) -> (f32, f32) {
    let (mut sum, mut n, mut min) = (0.0f64, 0usize, 1.0f32);
    for v in factors.flat.iter().chain(&factors.disp).flatten() {
        sum += f64::from(*v);
        n += 1;
        min = min.min(*v);
    }
    (if n == 0 { 1.0 } else { (sum / n as f64) as f32 }, min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_are_cosine_weighted() {
        let ao = Ao::new(64.0, 0.75, 256);
        assert_eq!(ao.samples(), 256);
        let mean: f32 = ao.dirs.iter().map(|d| d.z).sum::<f32>() / 256.0;
        assert!((mean - 2.0 / 3.0).abs() < 0.01, "{mean}");
        assert!(ao.dirs.iter().all(|d| (d.length() - 1.0).abs() < 1e-4 && d.z > 0.0));
        let sum: Vec3A = ao.dirs.iter().copied().sum();
        assert!(sum.x.abs() < 3.0 && sum.y.abs() < 3.0, "{sum}");
    }

    #[test]
    fn settings_follow_command_line_then_world_then_quality() {
        let world = |pairs: &[(&str, &str)]| hp_bsp::Entity {
            pairs: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
        };
        assert!(AoOptions::default().resolve(RadQuality::Final, None).is_none());
        let none = AoOptions { by_default: true, ..AoOptions::default() };
        assert!(none.resolve(RadQuality::Draft, None).is_none());
        let n = none.resolve(RadQuality::Normal, None).expect("on");
        assert_eq!((n.radius, n.strength, n.samples()), (DEFAULT_RADIUS, DEFAULT_STRENGTH, 32));
        assert_eq!(none.resolve(RadQuality::Final, None).map(|a| a.samples()), Some(64));
        let w = world(&[("_ao", "1"), ("_aoradius", "128"), ("_aostrength", "0.5"), ("_aosamples", "20")]);
        let a = none.resolve(RadQuality::Draft, Some(&w)).expect("on by key");
        assert_eq!((a.radius, a.strength, a.samples()), (128.0, 0.5, 20));
        assert!(none.resolve(RadQuality::Final, Some(&world(&[("_ao", "0")]))).is_none());
        let cli = AoOptions {
            enabled: Some(true),
            radius: Some(32.0),
            strength: Some(2.0),
            samples: Some(1),
            by_default: true,
        };
        let a = cli.resolve(RadQuality::Draft, Some(&world(&[("_ao", "0"), ("_aoradius", "500")]))).expect("on");
        assert_eq!((a.radius, a.strength, a.samples()), (32.0, 1.0, SAMPLES_RANGE.0 as usize));
        let off = AoOptions { enabled: Some(false), ..none.clone() };
        assert!(off.resolve(RadQuality::Final, Some(&world(&[("_ao", "1")]))).is_none());
        assert!(none.resolve(RadQuality::Final, Some(&world(&[("_aostrength", "0")]))).is_none());
    }

    #[test]
    fn filter_fills_holes_and_smooths() {
        let (w, h) = (4, 3);
        let valid: Vec<bool> = (0..w * h).map(|i| i % w != 0).collect();
        let mut occ = vec![0.2f32; w * h];
        occ[w + 2] = 1.0;
        let out = filter(w, h, &occ, &valid, &vec![Vec3A::Z; w * h]);
        assert!(out.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(out[w + 2] < 0.7 && out[w + 2] > 0.3, "{}", out[w + 2]);
        assert!(out[0] > 0.15 && out[0] < 0.6, "filled from its neighbours: {}", out[0]);
        assert_eq!(filter(2, 2, &[1.0; 4], &[false; 4], &[Vec3A::Z; 4]), vec![0.0; 4]);
        let normals = [Vec3A::Z, Vec3A::X];
        let out = filter(2, 1, &[0.0, 1.0], &[true, true], &normals);
        assert!(out[0] < 0.01 && out[1] > 0.99, "{out:?}");
    }
}
