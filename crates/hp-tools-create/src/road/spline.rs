use glam::DVec2;

use crate::curve::geom::catmull_rom;

const MIN_PER_SPAN: usize = 8;
const MAX_PER_SPAN: usize = 256;
const SAMPLE_UNITS: f64 = 6.0;

pub struct Dense {
    pub points: Vec<DVec2>,
    pub control: Vec<usize>,
    pub cumulative: Vec<f64>,
}

impl Dense {
    pub fn length(&self) -> f64 {
        self.cumulative.last().copied().unwrap_or(0.0)
    }

    pub fn control_arc(&self, k: usize) -> f64 {
        self.control.get(k).and_then(|i| self.cumulative.get(*i)).copied().unwrap_or(0.0)
    }

    pub fn tangent_at_control(&self, k: usize) -> Option<DVec2> {
        let i = *self.control.get(k)?;
        let a = self.points.get(i.saturating_sub(1))?;
        let b = self.points.get((i + 1).min(self.points.len() - 1))?;
        (*b - *a).try_normalize()
    }

    pub fn nearest(&self, target: DVec2) -> Option<(f64, DVec2, f64)> {
        let mut best: Option<(f64, DVec2, f64)> = None;
        for (i, w) in self.points.windows(2).enumerate() {
            let d = w[1] - w[0];
            let len2 = d.length_squared();
            let f = if len2 > 1e-18 { ((target - w[0]).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let on = w[0] + d * f;
            let dist = on.distance(target);
            if best.is_none_or(|b| dist < b.2) {
                best = Some((self.cumulative[i] + d.length() * f, on, dist));
            }
        }
        best
    }

    pub fn span_of_arc(&self, s: f64) -> usize {
        let mut span = 0;
        for k in 1..self.control.len() {
            if self.control_arc(k) <= s {
                span = k;
            }
        }
        span.min(self.control.len().saturating_sub(2))
    }
}

fn phantom(first: DVec2, second: DVec2, dir: Option<DVec2>) -> DVec2 {
    match dir.and_then(|d| d.try_normalize()) {
        Some(d) => first - d * first.distance(second).max(1e-6),
        None => first * 2.0 - second,
    }
}

pub fn dense(points: &[DVec2], start_dir: Option<DVec2>, end_dir: Option<DVec2>) -> Dense {
    let n = points.len();
    let mut out = Vec::new();
    let mut control = Vec::with_capacity(n);
    if n == 0 {
        return Dense { points: out, control, cumulative: Vec::new() };
    }
    out.push(points[0]);
    control.push(0);
    if n >= 2 {
        let straight = n == 2 && start_dir.is_none() && end_dir.is_none();
        let before = phantom(points[0], points[1], start_dir);
        let after = phantom(points[n - 1], points[n - 2], end_dir.map(|d| -d));
        let at = |i: isize| -> DVec2 {
            if i < 0 {
                before
            } else if i as usize >= n {
                after
            } else {
                points[i as usize]
            }
        };
        for s in 0..n - 1 {
            let chord = points[s].distance(points[s + 1]);
            let per =
                if straight { 1 } else { ((chord / SAMPLE_UNITS).ceil() as usize).clamp(MIN_PER_SPAN, MAX_PER_SPAN) };
            let i = s as isize;
            let quad = [at(i - 1), at(i), at(i + 1), at(i + 2)];
            for k in 1..=per {
                let p = if k == per { points[s + 1] } else { catmull_rom(quad, k as f64 / per as f64) };
                if p.is_finite() && out.last().is_none_or(|q| q.distance_squared(p) > 1e-18) {
                    out.push(p);
                }
            }
            control.push(out.len() - 1);
        }
    }
    let mut cumulative = Vec::with_capacity(out.len());
    let mut total = 0.0;
    cumulative.push(0.0);
    for w in out.windows(2) {
        total += w[0].distance(w[1]);
        cumulative.push(total);
    }
    Dense { points: out, control, cumulative }
}

#[derive(Clone, Debug)]
pub struct Elevation {
    s: Vec<f64>,
    z: Vec<f64>,
    d: Vec<f64>,
}

impl Elevation {
    pub fn new(s: &[f64], z: &[f64], start_slope: Option<f64>, end_slope: Option<f64>) -> Self {
        let n = s.len().min(z.len());
        let mut s: Vec<f64> = s[..n].to_vec();
        for i in 1..n {
            if s[i] <= s[i - 1] + 1e-6 {
                s[i] = s[i - 1] + 1e-6;
            }
        }
        let z = z[..n].to_vec();
        let mut d = vec![0.0; n];
        if n >= 2 {
            let h: Vec<f64> = (0..n - 1).map(|i| s[i + 1] - s[i]).collect();
            let delta: Vec<f64> = (0..n - 1).map(|i| (z[i + 1] - z[i]) / h[i]).collect();
            if n == 2 {
                d[0] = delta[0];
                d[1] = delta[0];
            } else {
                for i in 1..n - 1 {
                    if delta[i - 1] * delta[i] <= 0.0 {
                        d[i] = 0.0;
                    } else {
                        let w1 = 2.0 * h[i] + h[i - 1];
                        let w2 = h[i] + 2.0 * h[i - 1];
                        d[i] = (w1 + w2) / (w1 / delta[i - 1] + w2 / delta[i]);
                    }
                }
                d[0] = end_slope_estimate(h[0], h[1], delta[0], delta[1]);
                d[n - 1] = end_slope_estimate(h[n - 2], h[n - 3], delta[n - 2], delta[n - 3]);
            }
            if let Some(v) = start_slope {
                d[0] = v;
            }
            if let Some(v) = end_slope {
                d[n - 1] = v;
            }
        }
        Self { s, z, d }
    }

    pub fn at(&self, s: f64) -> f64 {
        let n = self.s.len();
        match n {
            0 => return 0.0,
            1 => return self.z[0],
            _ => {}
        }
        if s <= self.s[0] {
            return self.z[0] + self.d[0] * (s - self.s[0]);
        }
        if s >= self.s[n - 1] {
            return self.z[n - 1] + self.d[n - 1] * (s - self.s[n - 1]);
        }
        let i = self.s.partition_point(|x| *x <= s).saturating_sub(1).min(n - 2);
        let h = self.s[i + 1] - self.s[i];
        let t = (s - self.s[i]) / h;
        let (t2, t3) = (t * t, t * t * t);
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        h00 * self.z[i] + h10 * h * self.d[i] + h01 * self.z[i + 1] + h11 * h * self.d[i + 1]
    }

    pub fn slope_at(&self, s: f64) -> f64 {
        let e = 0.5;
        (self.at(s + e) - self.at(s - e)) / (2.0 * e)
    }
}

fn end_slope_estimate(h0: f64, h1: f64, d0: f64, d1: f64) -> f64 {
    let v = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
    if v.signum() != d0.signum() {
        0.0
    } else if d0.signum() != d1.signum() && v.abs() > 3.0 * d0.abs() {
        3.0 * d0
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn dense_passes_through_the_control_points() {
        let pts = [DVec2::new(0.0, 0.0), DVec2::new(200.0, 50.0), DVec2::new(400.0, -30.0)];
        let d = dense(&pts, None, None);
        assert_eq!(d.control.len(), 3);
        for (k, p) in pts.iter().enumerate() {
            assert!(d.points[d.control[k]].distance(*p) < 1e-9);
        }
        assert!(d.length() > 400.0);
        let straight = dense(&pts[..2], None, None);
        assert_eq!(straight.points.len(), 2);
    }

    #[test]
    fn end_directions_bend_the_ends() {
        let pts = [DVec2::new(0.0, 0.0), DVec2::new(200.0, 0.0)];
        let d = dense(&pts, Some(DVec2::new(1.0, 1.0)), Some(DVec2::new(1.0, -1.0)));
        assert!(d.points.len() > 8);
        let t = d.tangent_at_control(0).unwrap();
        assert!(t.y > 0.1, "{t:?}");
        assert!(d.points.iter().any(|p| p.y.abs() > 1.0));
    }

    #[test]
    fn elevation_hits_the_knots_and_never_overshoots() {
        let s = [0.0, 100.0, 200.0, 300.0];
        let z = [0.0, 50.0, 50.0, 0.0];
        let e = Elevation::new(&s, &z, None, None);
        for (a, b) in s.iter().zip(z.iter()) {
            assert!((e.at(*a) - b).abs() < 1e-9);
        }
        for k in 0..=300 {
            let v = e.at(f64::from(k));
            assert!((-1e-9..=50.0 + 1e-9).contains(&v), "{k}: {v}");
        }
        let flat = Elevation::new(&[0.0, 10.0], &[5.0, 5.0], None, None);
        assert_eq!(flat.at(3.0), 5.0);
        let forced = Elevation::new(&[0.0, 10.0], &[0.0, 0.0], Some(1.0), None);
        assert!(forced.slope_at(0.5) > 0.5);
    }
}
