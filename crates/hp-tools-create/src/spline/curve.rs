use glam::{DQuat, DVec2, DVec3};

use super::model::{Interp, SplineMeta, SplinePoint};

pub type GroundFn<'a> = dyn Fn(DVec2, f64) -> Option<f64> + 'a;

const SAMPLE_UNITS: f64 = 8.0;
const MIN_PER_SPAN: usize = 4;
const MAX_PER_SPAN: usize = 128;
const MAX_SAMPLES: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub pos: DVec3,
    pub s: f64,
    pub roll: f64,
    pub width: f64,
    pub span: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub pos: DVec3,
    pub tangent: DVec3,
    pub right: DVec3,
    pub up: DVec3,
    pub roll: f64,
    pub width: f64,
    pub s: f64,
}

impl Frame {
    pub fn local(&self, x: f64, z: f64) -> DVec3 {
        self.pos + self.right * x + self.up * z
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub samples: Vec<Sample>,
    pub closed: bool,
    pub control_s: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CurveError {
    TooFewPoints,
    TooLong,
}

impl std::fmt::Display for CurveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CurveError::TooFewPoints => write!(f, "a spline needs at least two different points"),
            CurveError::TooLong => write!(f, "the spline is too long"),
        }
    }
}

fn centripetal(p: [DVec3; 4], u: f64) -> DVec3 {
    let knot = |a: DVec3, b: DVec3| a.distance(b).max(1e-6).sqrt();
    let t0 = 0.0;
    let t1 = t0 + knot(p[0], p[1]);
    let t2 = t1 + knot(p[1], p[2]);
    let t3 = t2 + knot(p[2], p[3]);
    let t = t1 + (t2 - t1) * u;
    let lerp = |a: DVec3, b: DVec3, ta: f64, tb: f64| {
        let d = tb - ta;
        if d.abs() < 1e-12 { a } else { a * ((tb - t) / d) + b * ((t - ta) / d) }
    };
    let a1 = lerp(p[0], p[1], t0, t1);
    let a2 = lerp(p[1], p[2], t1, t2);
    let a3 = lerp(p[2], p[3], t2, t3);
    let b1 = lerp(a1, a2, t0, t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

fn bezier(p0: DVec3, c0: DVec3, c1: DVec3, p1: DVec3, u: f64) -> DVec3 {
    let v = 1.0 - u;
    p0 * (v * v * v) + c0 * (3.0 * v * v * u) + c1 * (3.0 * v * u * u) + p1 * (u * u * u)
}

pub struct Controls<'a> {
    pub points: &'a [SplinePoint],
    pub closed: bool,
}

impl Controls<'_> {
    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn spans(&self) -> usize {
        let n = self.points.len();
        if n < 2 {
            0
        } else if self.closed {
            n
        } else {
            n - 1
        }
    }

    pub fn at(&self, i: isize) -> DVec3 {
        let n = self.points.len() as isize;
        if self.closed {
            return self.points[i.rem_euclid(n) as usize].pos;
        }
        if i < 0 {
            let a = self.points[0].pos;
            let b = self.points[1.min(n as usize - 1)].pos;
            return a * 2.0 - b;
        }
        if i >= n {
            let a = self.points[n as usize - 1].pos;
            let b = self.points[(n as usize).saturating_sub(2)].pos;
            return a * 2.0 - b;
        }
        self.points[i as usize].pos
    }

    pub fn auto_tangent(&self, i: usize) -> DVec3 {
        let i = i as isize;
        (self.at(i + 1) - self.at(i - 1)) * 0.5
    }

    pub fn handle_out(&self, i: usize) -> DVec3 {
        self.points[i].handle_out.unwrap_or_else(|| self.auto_tangent(i) / 3.0)
    }

    pub fn handle_in(&self, i: usize) -> DVec3 {
        self.points[i].handle_in.unwrap_or_else(|| -self.auto_tangent(i) / 3.0)
    }

    pub fn next(&self, i: usize) -> usize {
        (i + 1) % self.points.len()
    }

    pub fn eval(&self, interp: Interp, span: usize, u: f64) -> DVec3 {
        let j = self.next(span);
        match interp {
            Interp::CatmullRom => {
                let i = span as isize;
                centripetal([self.at(i - 1), self.at(i), self.at(i + 1), self.at(i + 2)], u)
            }
            Interp::Bezier => {
                let p0 = self.points[span].pos;
                let p1 = self.points[j].pos;
                bezier(p0, p0 + self.handle_out(span), p1 + self.handle_in(j), p1, u)
            }
        }
    }
}

pub fn dense_points(meta: &SplineMeta) -> Vec<(DVec3, usize, f64)> {
    let c = Controls { points: &meta.points, closed: meta.is_closed() };
    let mut out = Vec::new();
    if c.len() < 2 {
        return out;
    }
    out.push((c.points[0].pos, 0, 0.0));
    for span in 0..c.spans() {
        let a = c.points[span].pos;
        let b = c.points[c.next(span)].pos;
        let chord = a.distance(b);
        let per = ((chord / SAMPLE_UNITS).ceil() as usize).clamp(MIN_PER_SPAN, MAX_PER_SPAN);
        for k in 1..=per {
            let u = k as f64 / per as f64;
            let p = if k == per { b } else { c.eval(meta.interp, span, u) };
            if p.is_finite() {
                out.push((p, span, u));
            }
        }
    }
    out
}

impl Path {
    pub fn build(meta: &SplineMeta, ground: Option<&GroundFn<'_>>) -> Result<Path, CurveError> {
        let c = Controls { points: &meta.points, closed: meta.is_closed() };
        let distinct = meta.points.windows(2).any(|w| w[0].pos.distance_squared(w[1].pos) > 1e-6);
        if c.len() < 2 || !distinct {
            return Err(CurveError::TooFewPoints);
        }
        let raw = dense_points(meta);
        if raw.len() > MAX_SAMPLES {
            return Err(CurveError::TooLong);
        }
        let mut samples: Vec<Sample> = Vec::with_capacity(raw.len());
        let n = c.len();
        let mut control_s = vec![0.0; n];
        let mut seen = vec![false; n];
        let mut total = 0.0;
        for (pos, span, u) in raw {
            let mut pos = pos;
            if let Some(g) = ground
                && meta.follow_terrain
                && let Some(z) = g(DVec2::new(pos.x, pos.y), pos.z)
            {
                pos.z = z + meta.terrain_offset;
            }
            let j = c.next(span);
            let (a, b) = (&c.points[span], &c.points[j]);
            let roll = a.roll + (b.roll - a.roll) * u;
            let width = a.width + (b.width - a.width) * u;
            if let Some(last) = samples.last() {
                let d = last.pos.distance(pos);
                if d < 1e-6 {
                    if u >= 1.0 && !seen[j] {
                        seen[j] = true;
                        control_s[j] = total;
                    }
                    continue;
                }
                total += d;
            } else {
                seen[0] = true;
            }
            samples.push(Sample { pos, s: total, roll, width, span });
            if u >= 1.0 && !seen[j] {
                seen[j] = true;
                control_s[j] = total;
            }
        }
        if samples.len() < 2 || total < 1e-3 {
            return Err(CurveError::TooFewPoints);
        }
        Ok(Path { samples, closed: c.closed, control_s })
    }

    pub fn length(&self) -> f64 {
        self.samples.last().map_or(0.0, |s| s.s)
    }

    fn index_at(&self, s: f64) -> usize {
        let i = self.samples.partition_point(|x| x.s <= s);
        i.saturating_sub(1).min(self.samples.len().saturating_sub(2))
    }

    fn tangent_near(&self, i: usize) -> DVec3 {
        let n = self.samples.len();
        let a = self.samples[i].pos;
        let b = self.samples[(i + 1).min(n - 1)].pos;
        (b - a).try_normalize().unwrap_or(DVec3::X)
    }

    pub fn at(&self, s: f64) -> Frame {
        let len = self.length();
        let s = if self.closed && len > 0.0 { s.rem_euclid(len) } else { s.clamp(0.0, len) };
        let i = self.index_at(s);
        let (a, b) = (self.samples[i], self.samples[(i + 1).min(self.samples.len() - 1)]);
        let d = b.s - a.s;
        let f = if d > 1e-12 { ((s - a.s) / d).clamp(0.0, 1.0) } else { 0.0 };
        let pos = a.pos.lerp(b.pos, f);
        let roll = a.roll + (b.roll - a.roll) * f;
        let width = a.width + (b.width - a.width) * f;
        let t_here = self.tangent_near(i);
        let t_prev = if i > 0 { self.tangent_near(i - 1) } else { t_here };
        let t_next = if i + 1 < self.samples.len() - 1 { self.tangent_near(i + 1) } else { t_here };
        let tangent = if f < 0.5 {
            t_prev.lerp(t_here, f + 0.5).try_normalize().unwrap_or(t_here)
        } else {
            t_here.lerp(t_next, f - 0.5).try_normalize().unwrap_or(t_here)
        };
        frame_from(pos, tangent, roll, width, s)
    }

    pub fn nearest(&self, target: DVec3) -> Option<(f64, DVec3, f64, usize)> {
        let mut best: Option<(f64, DVec3, f64, usize)> = None;
        for w in self.samples.windows(2) {
            let d = w[1].pos - w[0].pos;
            let len2 = d.length_squared();
            let f = if len2 > 1e-18 { ((target - w[0].pos).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let on = w[0].pos + d * f;
            let dist = on.distance(target);
            if best.is_none_or(|b| dist < b.2) {
                best = Some((w[0].s + (w[1].s - w[0].s) * f, on, dist, w[0].span));
            }
        }
        best
    }

    pub fn polyline(&self) -> Vec<DVec3> {
        self.samples.iter().map(|s| s.pos).collect()
    }
}

pub fn frame_from(pos: DVec3, tangent: DVec3, roll: f64, width: f64, s: f64) -> Frame {
    let tangent = tangent.try_normalize().unwrap_or(DVec3::X);
    let right = tangent.cross(DVec3::Z).try_normalize().unwrap_or_else(|| tangent.cross(DVec3::Y).normalize());
    let up = right.cross(tangent).normalize();
    let q = DQuat::from_axis_angle(tangent, roll.to_radians());
    Frame { pos, tangent, right: q * right, up: q * up, roll, width, s }
}

pub fn sag_depth(distance: f64, slack: f64) -> f64 {
    if distance <= 1e-6 || slack <= 0.0 {
        return 0.0;
    }
    (3.0 * distance * slack / 8.0).sqrt()
}

pub fn sag_curve(a: DVec3, b: DVec3, slack: f64, steps: usize) -> Vec<DVec3> {
    let steps = steps.max(1);
    let depth = sag_depth(a.distance(b), slack);
    (0..=steps)
        .map(|k| {
            let u = k as f64 / steps as f64;
            a.lerp(b, u) - DVec3::Z * (4.0 * depth * u * (1.0 - u))
        })
        .collect()
}
