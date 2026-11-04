use glam::{DVec2, DVec3};

use crate::blend::BlendFalloff;

pub const SOURCE_WALKABLE_DEGREES: f64 = 45.573;
pub const MAX_RAMP_DEGREES: f64 = 89.0;
pub const MAX_LIMIT_SWEEPS: usize = 6000;
pub const GRADIENT_BANDS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RampPivot {
    #[default]
    Average,
    Lowest,
    Highest,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SlopeStats {
    pub triangles: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneFit {
    pub centroid: DVec2,
    pub mean_height: f64,
    pub gradient: DVec2,
}

impl PlaneFit {
    pub fn angle_degrees(&self) -> f64 {
        self.gradient.length().atan().to_degrees()
    }

    pub fn azimuth_degrees(&self) -> f64 {
        if self.gradient.length() < 1e-12 {
            return 0.0;
        }
        self.gradient.y.atan2(self.gradient.x).to_degrees().rem_euclid(360.0)
    }

    pub fn height_at(&self, p: DVec2) -> f64 {
        self.mean_height + self.gradient.dot(p - self.centroid)
    }
}

pub fn triangle_gradient(a: (DVec2, f64), b: (DVec2, f64), c: (DVec2, f64)) -> Option<DVec2> {
    let (e1, e2) = (b.0 - a.0, c.0 - a.0);
    let det = e1.x * e2.y - e1.y * e2.x;
    if det.abs() < 1e-9 {
        return None;
    }
    let (d1, d2) = (b.1 - a.1, c.1 - a.1);
    Some(DVec2::new((d1 * e2.y - d2 * e1.y) / det, (d2 * e1.x - d1 * e2.x) / det))
}

pub fn incline_degrees(normal_z: f64) -> f64 {
    normal_z.abs().clamp(0.0, 1.0).acos().to_degrees()
}

pub fn up_angle_degrees(normal_z: f64) -> f64 {
    normal_z.clamp(-1.0, 1.0).acos().to_degrees()
}

fn triangle_normal_z(positions: &[DVec3], t: &[u32]) -> Option<f64> {
    let p = |k: u32| positions.get(k as usize).copied();
    let (a, b, c) = (p(*t.first()?)?, p(*t.get(1)?)?, p(*t.get(2)?)?);
    (b - a).cross(c - a).try_normalize().map(|n| n.z)
}

pub fn triangle_up_angles(positions: &[DVec3], indices: &[u32]) -> Vec<f64> {
    indices.chunks_exact(3).map(|t| triangle_normal_z(positions, t).map_or(90.0, up_angle_degrees)).collect()
}

pub fn triangle_inclines(positions: &[DVec3], indices: &[u32]) -> Vec<f64> {
    indices.chunks_exact(3).map(|t| triangle_normal_z(positions, t).map_or(90.0, incline_degrees)).collect()
}

pub fn slope_band(up_angle: f64, limit: f64) -> usize {
    if up_angle > 90.0 {
        return GRADIENT_BANDS - 1;
    }
    let limit = limit.clamp(1.0, 89.0);
    let r = up_angle / limit;
    match r {
        r if r < 0.25 => 0,
        r if r < 0.5 => 1,
        r if r < 0.75 => 2,
        r if r <= 1.0 => 3,
        r if r < 1.25 => 4,
        r if r < 1.5 => 5,
        _ => 6,
    }
}

pub fn slope_band_range(band: usize, limit: f64) -> (f64, f64) {
    let limit = limit.clamp(1.0, 89.0);
    let edges = [0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5];
    match band {
        0..=5 => (edges[band] * limit, edges[band + 1] * limit),
        6 => (1.5 * limit, 90.0),
        _ => (90.0, 180.0),
    }
}

#[derive(Clone, Debug, Default)]
pub struct SlopeField {
    pub xy: Vec<DVec2>,
    pub h: Vec<f64>,
    pub tris: Vec<[usize; 3]>,
}

fn gradient_at(xy: &[DVec2], h: &[f64], t: &[usize; 3]) -> Option<DVec2> {
    let at = |i: usize| Some((*xy.get(i)?, *h.get(i)?));
    triangle_gradient(at(t[0])?, at(t[1])?, at(t[2])?)
}

impl SlopeField {
    fn gradient_of(&self, t: &[usize; 3]) -> Option<DVec2> {
        gradient_at(&self.xy, &self.h, t)
    }

    pub fn slopes_degrees(&self) -> Vec<f64> {
        self.tris.iter().filter_map(|t| self.gradient_of(t)).map(|g| g.length().atan().to_degrees()).collect()
    }

    pub fn stats(&self) -> SlopeStats {
        let s = self.slopes_degrees();
        if s.is_empty() {
            return SlopeStats::default();
        }
        SlopeStats {
            triangles: s.len(),
            min: s.iter().copied().fold(f64::INFINITY, f64::min),
            max: s.iter().copied().fold(0.0, f64::max),
            mean: s.iter().sum::<f64>() / s.len() as f64,
        }
    }

    pub fn max_slope_degrees(&self) -> f64 {
        self.stats().max
    }

    pub fn fit_plane(&self) -> Option<PlaneFit> {
        let n = self.xy.len().min(self.h.len());
        if n < 3 {
            return None;
        }
        let used: Vec<usize> = {
            let mut seen = vec![false; n];
            for t in &self.tris {
                for &i in t {
                    if let Some(s) = seen.get_mut(i) {
                        *s = true;
                    }
                }
            }
            (0..n).filter(|i| seen[*i]).collect()
        };
        if used.len() < 3 {
            return None;
        }
        let count = used.len() as f64;
        let centroid = used.iter().map(|&i| self.xy[i]).sum::<DVec2>() / count;
        let mean_height = used.iter().map(|&i| self.h[i]).sum::<f64>() / count;
        let (mut sxx, mut sxy, mut syy, mut sxz, mut syz) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &i in &used {
            let d = self.xy[i] - centroid;
            let z = self.h[i] - mean_height;
            sxx += d.x * d.x;
            sxy += d.x * d.y;
            syy += d.y * d.y;
            sxz += d.x * z;
            syz += d.y * z;
        }
        let det = sxx * syy - sxy * sxy;
        if det.abs() < 1e-9 * (sxx * syy).abs().max(1e-12) {
            return None;
        }
        let gradient = DVec2::new((sxz * syy - syz * sxy) / det, (syz * sxx - sxz * sxy) / det);
        Some(PlaneFit { centroid, mean_height, gradient })
    }

    pub fn terrace(&mut self, step: f64, strength: f64) {
        if !(step.is_finite() && step > 1e-6 && strength.is_finite() && strength > 0.0) {
            return;
        }
        let strength = strength.min(1.0);
        let base = self.h.iter().copied().fold(f64::INFINITY, f64::min);
        if !base.is_finite() {
            return;
        }
        for h in &mut self.h {
            let snapped = base + ((*h - base) / step).round() * step;
            *h += (snapped - *h) * strength;
        }
    }

    pub fn limit(&mut self, max_degrees: f64) -> usize {
        let max_degrees = max_degrees.clamp(0.0, MAX_RAMP_DEGREES);
        let cap = max_degrees.to_radians().tan();
        let aim = cap * 0.999;
        let mut sweeps = 0;
        for sweep in 0..MAX_LIMIT_SWEEPS {
            let mut touched = false;
            for t in &self.tris {
                let Some(g) = gradient_at(&self.xy, &self.h, t) else { continue };
                let m = g.length();
                if m <= cap {
                    continue;
                }
                touched = true;
                let g2 = g * (aim / m);
                let c = t.map(|i| self.xy[i]).iter().sum::<DVec2>() / 3.0;
                let hc = t.map(|i| self.h[i]).iter().sum::<f64>() / 3.0;
                for &i in t {
                    self.h[i] = hc + g2.dot(self.xy[i] - c);
                }
            }
            sweeps = sweep + 1;
            if !touched {
                break;
            }
        }
        let worst = self.slopes_degrees().into_iter().fold(0.0, f64::max).to_radians().tan();
        if worst > cap && worst.is_finite() {
            let n = self.h.len().max(1) as f64;
            let mean = self.h.iter().sum::<f64>() / n;
            let k = aim / worst;
            for h in &mut self.h {
                *h = mean + (*h - mean) * k;
            }
        }
        sweeps
    }

    pub fn ramp(&mut self, degrees: f64, azimuth: f64, pivot: RampPivot, strength: f64, detail: f64) -> bool {
        let n = self.h.len().min(self.xy.len());
        if n == 0 || !degrees.is_finite() || !azimuth.is_finite() {
            return false;
        }
        let fit = self.fit_plane();
        let tan = degrees.clamp(-MAX_RAMP_DEGREES, MAX_RAMP_DEGREES).to_radians().tan();
        let dir = DVec2::new(azimuth.to_radians().cos(), azimuth.to_radians().sin());
        let c = self.xy.iter().take(n).sum::<DVec2>() / n as f64;
        let q: Vec<f64> = self.xy.iter().take(n).map(|p| (dir * tan).dot(*p - c)).collect();
        let reduce = |f: fn(f64, f64) -> f64, init: f64, v: &[f64]| v.iter().copied().fold(init, f);
        let off = match pivot {
            RampPivot::Average => self.h.iter().sum::<f64>() / n as f64 - q.iter().sum::<f64>() / n as f64,
            RampPivot::Lowest => reduce(f64::min, f64::INFINITY, &self.h) - reduce(f64::min, f64::INFINITY, &q),
            RampPivot::Highest => {
                reduce(f64::max, f64::NEG_INFINITY, &self.h) - reduce(f64::max, f64::NEG_INFINITY, &q)
            }
        };
        let strength = strength.clamp(0.0, 1.0);
        let detail = detail.clamp(0.0, 1.0);
        for (i, qi) in q.iter().enumerate() {
            let mut plane = qi + off;
            if detail > 0.0
                && let Some(f) = &fit
            {
                plane += detail * (self.h[i] - f.height_at(self.xy[i]));
            }
            self.h[i] += (plane - self.h[i]) * strength;
        }
        true
    }

    pub fn uniform(&mut self, strength: f64, detail: f64) -> Option<PlaneFit> {
        let fit = self.fit_plane()?;
        self.ramp(fit.angle_degrees(), fit.azimuth_degrees(), RampPivot::Average, strength, detail);
        Some(fit)
    }

    pub fn along_path(
        &mut self,
        path: &[DVec2],
        ends: Option<(f64, f64)>,
        width: f64,
        falloff: BlendFalloff,
        strength: f64,
    ) -> usize {
        let width = width.max(1e-3);
        let strength = strength.clamp(0.0, 1.0);
        let mut cum = vec![0.0];
        for w in path.windows(2) {
            cum.push(cum.last().copied().unwrap_or(0.0) + w[0].distance(w[1]));
        }
        let total = cum.last().copied().unwrap_or(0.0);
        if path.len() < 2 || total < 1e-6 {
            return 0;
        }
        let nearest_height = |p: DVec2| {
            self.xy
                .iter()
                .zip(&self.h)
                .min_by(|a, b| a.0.distance_squared(p).total_cmp(&b.0.distance_squared(p)))
                .map(|(_, h)| *h)
        };
        let (h0, h1) = match ends {
            Some(e) => e,
            None => match (nearest_height(path[0]), nearest_height(path[path.len() - 1])) {
                (Some(a), Some(b)) => (a, b),
                _ => return 0,
            },
        };
        let mut changed = 0;
        for i in 0..self.xy.len().min(self.h.len()) {
            let p = self.xy[i];
            let mut best = (f64::INFINITY, 0.0);
            for (si, w) in path.windows(2).enumerate() {
                let d = w[1] - w[0];
                let len2 = d.length_squared();
                let t = if len2 > 1e-12 { ((p - w[0]).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let dist = (w[0] + d * t).distance(p);
                if dist < best.0 {
                    best = (dist, cum[si] + t * len2.sqrt());
                }
            }
            if best.0 >= width {
                continue;
            }
            let f = falloff.eval(1.0 - best.0 / width) * strength;
            let target = h0 + (h1 - h0) * (best.1 / total);
            let new = self.h[i] + (target - self.h[i]) * f;
            if (new - self.h[i]).abs() > 1e-9 {
                self.h[i] = new;
                changed += 1;
            }
        }
        changed
    }

    pub fn smooth(&mut self, passes: u32, strength: f64) {
        let n = self.h.len();
        let mut links: Vec<Vec<usize>> = vec![Vec::new(); n];
        for t in &self.tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if a < n && b < n {
                    if !links[a].contains(&b) {
                        links[a].push(b);
                    }
                    if !links[b].contains(&a) {
                        links[b].push(a);
                    }
                }
            }
        }
        let strength = strength.clamp(0.0, 1.0);
        for _ in 0..passes.min(1000) {
            let old = self.h.clone();
            for (i, list) in links.iter().enumerate() {
                if list.is_empty() {
                    continue;
                }
                let avg = list.iter().map(|&j| old[j]).sum::<f64>() / list.len() as f64;
                self.h[i] = old[i] + (avg - old[i]) * strength;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize, step: f64, height: impl Fn(f64, f64) -> f64) -> SlopeField {
        let mut f = SlopeField::default();
        for j in 0..n {
            for i in 0..n {
                let (x, y) = (i as f64 * step, j as f64 * step);
                f.xy.push(DVec2::new(x, y));
                f.h.push(height(x, y));
            }
        }
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = j * n + i;
                f.tris.push([a, a + n, a + 1]);
                f.tris.push([a + 1, a + n, a + n + 1]);
            }
        }
        f
    }

    #[test]
    fn limiting_respects_the_maximum_angle() {
        let mut f = grid(17, 16.0, |x, y| if x > 120.0 { 300.0 } else { 0.0 } + (y * 0.37).sin() * 20.0);
        let before = f.max_slope_degrees();
        assert!(before > 60.0);
        let mean = f.h.iter().sum::<f64>() / f.h.len() as f64;
        f.limit(30.0);
        assert!(f.max_slope_degrees() <= 30.0 + 1e-6, "{}", f.max_slope_degrees());
        let after = f.h.iter().sum::<f64>() / f.h.len() as f64;
        assert!((after - mean).abs() < 5.0, "{mean} {after}");
    }

    #[test]
    fn a_gentle_surface_is_left_alone() {
        let mut f = grid(9, 16.0, |x, _| x * 0.2);
        let before = f.h.clone();
        f.limit(30.0);
        assert_eq!(before, f.h);
    }

    #[test]
    fn terraces_make_flat_treads_and_limited_risers() {
        let mut f = grid(17, 16.0, |x, _| x * 0.9);
        f.terrace(64.0, 1.0);
        f.limit(60.0);
        assert!(f.max_slope_degrees() <= 60.0 + 1e-6);
        let flat = f.slopes_degrees().iter().filter(|s| **s < 5.0).count();
        assert!(flat > f.tris.len() / 4, "{flat}");
    }

    #[test]
    fn a_ramp_is_an_exact_plane() {
        let mut f = grid(9, 16.0, |x, y| (x * 0.05).sin() * 40.0 + y * 0.1);
        assert!(f.ramp(20.0, 90.0, RampPivot::Average, 1.0, 0.0));
        let fit = f.fit_plane().expect("fit");
        assert!((fit.angle_degrees() - 20.0).abs() < 1e-6, "{}", fit.angle_degrees());
        assert!((fit.azimuth_degrees() - 90.0).abs() < 1e-6);
        for s in f.slopes_degrees() {
            assert!((s - 20.0).abs() < 1e-6, "{s}");
        }
    }

    #[test]
    fn pivots_keep_the_mean_the_floor_or_the_top() {
        let base = grid(9, 16.0, |x, y| 100.0 + (x + y) * 0.1);
        let (lo, hi) = (
            base.h.iter().copied().fold(f64::INFINITY, f64::min),
            base.h.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        let mean = base.h.iter().sum::<f64>() / base.h.len() as f64;
        let mut a = base.clone();
        a.ramp(30.0, 0.0, RampPivot::Average, 1.0, 0.0);
        assert!((a.h.iter().sum::<f64>() / a.h.len() as f64 - mean).abs() < 1e-9);
        let mut b = base.clone();
        b.ramp(30.0, 0.0, RampPivot::Lowest, 1.0, 0.0);
        assert!((b.h.iter().copied().fold(f64::INFINITY, f64::min) - lo).abs() < 1e-9);
        let mut c = base.clone();
        c.ramp(30.0, 0.0, RampPivot::Highest, 1.0, 0.0);
        assert!((c.h.iter().copied().fold(f64::NEG_INFINITY, f64::max) - hi).abs() < 1e-9);
    }

    #[test]
    fn detail_keeps_the_relief_on_the_ramp() {
        let mut f = grid(9, 16.0, |x, y| ((x * 0.07).sin() + (y * 0.05).cos()) * 10.0);
        f.ramp(15.0, 45.0, RampPivot::Average, 1.0, 1.0);
        let fit = f.fit_plane().expect("fit");
        assert!((fit.angle_degrees() - 15.0).abs() < 1e-6);
        assert!(f.slopes_degrees().iter().any(|s| (s - 15.0).abs() > 0.5));
    }

    #[test]
    fn uniform_slope_keeps_the_direction_and_angle() {
        let mut f = grid(9, 16.0, |x, y| x * 0.3 + (y * 0.2).sin() * 15.0);
        let before = f.fit_plane().expect("fit");
        f.uniform(1.0, 0.0);
        let after = f.fit_plane().expect("fit");
        assert!((before.angle_degrees() - after.angle_degrees()).abs() < 1e-6);
        assert!((before.azimuth_degrees() - after.azimuth_degrees()).abs() < 1e-6);
        let s = f.stats();
        assert!((s.max - s.min).abs() < 1e-6);
    }

    #[test]
    fn a_path_gets_a_constant_grade() {
        let mut f = grid(17, 16.0, |_, y| (y * 0.1).sin() * 30.0);
        let path = [DVec2::new(0.0, 128.0), DVec2::new(256.0, 128.0)];
        f.along_path(&path, Some((0.0, 100.0)), 24.0, BlendFalloff::Linear, 1.0);
        for i in 0..17 {
            let node = 8 * 17 + i;
            assert!((f.h[node] - 100.0 * (i as f64 / 16.0)).abs() < 1e-9, "{i}: {}", f.h[node]);
        }
        assert_eq!(f.h[0], 0.0);
    }

    #[test]
    fn smoothing_reduces_the_relief() {
        let mut f = grid(9, 16.0, |x, y| if (x + y) % 32.0 < 16.0 { 10.0 } else { -10.0 });
        let before = f.h.iter().map(|h| h * h).sum::<f64>();
        f.smooth(4, 0.5);
        assert!(f.h.iter().map(|h| h * h).sum::<f64>() < before);
    }

    #[test]
    fn bands_follow_the_limit() {
        assert_eq!(slope_band(0.0, 45.0), 0);
        assert_eq!(slope_band(45.0, 45.0), 3);
        assert_eq!(slope_band(46.0, 45.0), 4);
        assert_eq!(slope_band(80.0, 45.0), 6);
        assert_eq!(slope_band(120.0, 45.0), 7);
        let (lo, hi) = slope_band_range(3, 40.0);
        assert!((lo - 30.0).abs() < 1e-9 && (hi - 40.0).abs() < 1e-9);
        assert!((incline_degrees(0.7) - SOURCE_WALKABLE_DEGREES).abs() < 1e-5);
    }

    #[test]
    fn triangle_angles_match_the_normals() {
        let p = [DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 10.0, 0.0), DVec3::new(10.0, 0.0, 10.0)];
        let up = triangle_up_angles(&p, &[0, 2, 1]);
        assert!((up[0] - 45.0).abs() < 1e-9);
        let flip = triangle_up_angles(&p, &[0, 1, 2]);
        assert!((flip[0] - 135.0).abs() < 1e-9);
        assert!((triangle_inclines(&p, &[0, 1, 2])[0] - 45.0).abs() < 1e-9);
    }
}
