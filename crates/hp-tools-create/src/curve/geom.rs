use glam::{DVec2, DVec3};
use hp_app::ViewKind;
use hp_geom::{GeomError, hull_solid};
use hp_vmf::Side;

pub const MIN_SUBDIVISIONS: u32 = 2;
pub const MAX_SUBDIVISIONS: u32 = 4096;
pub const MIN_THICKNESS: f64 = 1.0;
pub const MAX_THICKNESS: f64 = 1024.0;
pub const MIN_HEIGHT: f64 = 1.0;
pub const MAX_HEIGHT: f64 = 16384.0;
pub const MAX_CURVE_POINTS: usize = 1024;
pub const MIN_SEGMENT: f64 = 1.0;
pub const MIN_EDGE: f64 = 0.25;
pub const MAX_COORD: f64 = hp_geom::MAX_COORD;
const MITER_LIMIT_COS: f64 = 0.125;
const SAMPLES_PER_SEGMENT: usize = 8;
const MIN_SAMPLES_PER_SPAN: usize = 8;
const MAX_DENSE_SAMPLES: usize = 262_144;

pub fn clamp_subdivisions(n: i64) -> u32 {
    n.clamp(i64::from(MIN_SUBDIVISIONS), i64::from(MAX_SUBDIVISIONS)) as u32
}

pub fn clamp_thickness(t: f64) -> f64 {
    if t.is_finite() { t.clamp(MIN_THICKNESS, MAX_THICKNESS) } else { MIN_THICKNESS }
}

pub fn clamp_height(h: f64) -> f64 {
    if h.is_finite() { h.clamp(MIN_HEIGHT, MAX_HEIGHT) } else { MIN_HEIGHT }
}

pub fn snap_point(p: DVec2, grid: f64) -> DVec2 {
    if !(grid.is_finite() && grid > 0.0 && p.is_finite()) {
        return p;
    }
    let s = (p / grid).round() * grid;
    DVec2::new(s.x + 0.0, s.y + 0.0)
}

pub fn clean_points(points: &[DVec2]) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::with_capacity(points.len());
    for p in points.iter().filter(|p| p.is_finite()) {
        if out.last().is_none_or(|q| q.distance_squared(*p) > 1e-12) {
            out.push(*p);
        }
    }
    out
}

fn knot(a: DVec2, b: DVec2) -> f64 {
    a.distance(b).sqrt().max(1e-9)
}

fn catmull_rom(p: [DVec2; 4], u: f64) -> DVec2 {
    let t0 = 0.0;
    let t1 = t0 + knot(p[0], p[1]);
    let t2 = t1 + knot(p[1], p[2]);
    let t3 = t2 + knot(p[2], p[3]);
    let t = t1 + (t2 - t1) * u;
    let lerp = |a: DVec2, b: DVec2, ta: f64, tb: f64| a * ((tb - t) / (tb - ta)) + b * ((t - ta) / (tb - ta));
    let a1 = lerp(p[0], p[1], t0, t1);
    let a2 = lerp(p[1], p[2], t1, t2);
    let a3 = lerp(p[2], p[3], t2, t3);
    let b1 = lerp(a1, a2, t0, t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

pub fn dense_spline(points: &[DVec2], segments: u32) -> Vec<DVec2> {
    let pts = clean_points(points);
    let n = pts.len();
    if n < 3 {
        return pts;
    }
    let spans = n - 1;
    let wanted = SAMPLES_PER_SEGMENT * segments as usize / spans + MIN_SAMPLES_PER_SPAN;
    let per_span = wanted.clamp(MIN_SAMPLES_PER_SPAN, (MAX_DENSE_SAMPLES / spans).max(MIN_SAMPLES_PER_SPAN));
    let at = |i: isize| -> DVec2 {
        if i < 0 {
            pts[0] * 2.0 - pts[1]
        } else if i as usize >= n {
            pts[n - 1] * 2.0 - pts[n - 2]
        } else {
            pts[i as usize]
        }
    };
    let mut out = Vec::with_capacity(spans * per_span + 1);
    out.push(pts[0]);
    for s in 0..spans {
        let i = s as isize;
        let quad = [at(i - 1), at(i), at(i + 1), at(i + 2)];
        for k in 1..=per_span {
            let p = if k == per_span { pts[s + 1] } else { catmull_rom(quad, k as f64 / per_span as f64) };
            if p.is_finite() && out.last().is_none_or(|q| q.distance_squared(p) > 1e-18) {
                out.push(p);
            }
        }
    }
    out
}

pub fn polyline_length(points: &[DVec2]) -> f64 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

pub fn resample(points: &[DVec2], segments: u32) -> Vec<DVec2> {
    let dense = dense_spline(points, segments);
    if dense.len() < 2 || segments == 0 {
        return dense;
    }
    let mut cumulative = Vec::with_capacity(dense.len());
    let mut total = 0.0;
    cumulative.push(0.0);
    for w in dense.windows(2) {
        total += w[0].distance(w[1]);
        cumulative.push(total);
    }
    let mut out = Vec::with_capacity(segments as usize + 1);
    out.push(dense[0]);
    let mut j = 0usize;
    for k in 1..segments {
        let target = total * f64::from(k) / f64::from(segments);
        while j + 2 < cumulative.len() && cumulative[j + 1] < target {
            j += 1;
        }
        let (l0, l1) = (cumulative[j], cumulative[j + 1]);
        let f = if l1 > l0 { ((target - l0) / (l1 - l0)).clamp(0.0, 1.0) } else { 0.0 };
        out.push(dense[j].lerp(dense[j + 1], f));
    }
    out.push(dense[dense.len() - 1]);
    out
}

#[derive(Clone, Debug, PartialEq)]
pub enum CurveError {
    TooFewPoints,
    TooShort { segment: f64 },
    Folded(Vec<usize>),
    OutOfRange,
}

impl std::fmt::Display for CurveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CurveError::TooFewPoints => write!(f, "the curve needs at least 2 different points"),
            CurveError::TooShort { segment } => write!(
                f,
                "the segments would be {segment:.3} units long, under {MIN_SEGMENT}: lower the subdivisions or draw a longer curve"
            ),
            CurveError::Folded(list) => write!(
                f,
                "{} segment(s) fold on the inside of a tight bend: lower the thickness, the subdivisions or open the bend",
                list.len()
            ),
            CurveError::OutOfRange => write!(f, "the curve goes outside the map (coordinates up to {MAX_COORD})"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveLayout {
    pub centers: Vec<DVec2>,
    pub quads: Vec<[DVec2; 4]>,
    pub folded: Vec<usize>,
    pub length: f64,
}

impl CurveLayout {
    pub fn check(&self) -> Result<(), CurveError> {
        if self.folded.is_empty() { Ok(()) } else { Err(CurveError::Folded(self.folded.clone())) }
    }
}

fn left(d: DVec2) -> DVec2 {
    DVec2::new(-d.y, d.x)
}

pub fn layout(points: &[DVec2], segments: u32, thickness: f64) -> Result<CurveLayout, CurveError> {
    let pts = clean_points(points);
    if pts.len() < 2 {
        return Err(CurveError::TooFewPoints);
    }
    let segments = segments.clamp(MIN_SUBDIVISIONS, MAX_SUBDIVISIONS);
    let half = clamp_thickness(thickness) * 0.5;
    let centers = resample(&pts, segments);
    let length = polyline_length(&centers);
    let n = centers.len() - 1;
    let shortest = centers.windows(2).map(|w| w[0].distance(w[1])).fold(f64::INFINITY, f64::min);
    if n == 0 || shortest.is_nan() || shortest < MIN_SEGMENT {
        return Err(CurveError::TooShort { segment: if shortest.is_finite() { shortest } else { 0.0 } });
    }
    let dirs: Vec<DVec2> = centers.windows(2).map(|w| (w[1] - w[0]).normalize()).collect();
    let mut sharp = vec![false; n + 1];
    let mut offsets: Vec<DVec2> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let off = if i == 0 {
            left(dirs[0]) * half
        } else if i == n {
            left(dirs[n - 1]) * half
        } else {
            let (a, b) = (left(dirs[i - 1]), left(dirs[i]));
            let sum = a + b;
            let len = sum.length();
            let m = if len > 1e-9 { sum / len } else { b };
            let cos = m.dot(b);
            if cos < MITER_LIMIT_COS {
                sharp[i] = true;
                m * half
            } else {
                m * (half / cos)
            }
        };
        offsets.push(off);
    }
    let mut quads = Vec::with_capacity(n);
    let mut folded = Vec::new();
    for i in 0..n {
        let (p, q) = (centers[i], centers[i + 1]);
        let (l0, l1) = (p + offsets[i], q + offsets[i + 1]);
        let (r0, r1) = (p - offsets[i], q - offsets[i + 1]);
        let d = dirs[i];
        let ok = !sharp[i] && !sharp[i + 1] && (l1 - l0).dot(d) >= MIN_EDGE && (r1 - r0).dot(d) >= MIN_EDGE;
        if !ok {
            folded.push(i);
        }
        quads.push([r0, r1, l1, l0]);
    }
    let all = quads.iter().flatten().chain(centers.iter());
    if all.clone().any(|p| !p.is_finite() || p.abs().max_element() > MAX_COORD) {
        return Err(CurveError::OutOfRange);
    }
    Ok(CurveLayout { centers, quads, folded, length })
}

pub fn quad_corners(kind: ViewKind, quad: &[DVec2; 4], depth: (f64, f64)) -> [DVec3; 8] {
    let at = |p: DVec2, d: f64| kind.from_2d(p, d);
    let [a, b, c, e] = *quad;
    [
        at(a, depth.0),
        at(b, depth.0),
        at(e, depth.0),
        at(c, depth.0),
        at(a, depth.1),
        at(b, depth.1),
        at(e, depth.1),
        at(c, depth.1),
    ]
}

pub fn is_convex_quad(quad: &[DVec2; 4]) -> bool {
    let mut sign = 0.0f64;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let c = quad[(i + 2) % 4];
        let cross = (b - a).perp_dot(c - b);
        if cross.abs() < 1e-9 {
            return false;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

pub struct CurveSpec<'a> {
    pub kind: ViewKind,
    pub points: &'a [DVec2],
    pub segments: u32,
    pub thickness: f64,
    pub depth: (f64, f64),
    pub material: &'a str,
    pub tex_scale: f64,
    pub lightmap_scale: i32,
}

#[derive(Debug)]
pub enum BuildError {
    Curve(CurveError),
    Height,
    Geom(GeomError),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::Curve(e) => e.fmt(f),
            BuildError::Height => write!(f, "the height must be {MIN_HEIGHT} to {MAX_HEIGHT} units"),
            BuildError::Geom(e) => e.fmt(f),
        }
    }
}

pub fn build_brushes(
    spec: &CurveSpec<'_>,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<Vec<Side>>, BuildError> {
    if !spec.kind.is_2d() {
        return Err(BuildError::Curve(CurveError::TooFewPoints));
    }
    let (lo, hi) = (spec.depth.0.min(spec.depth.1), spec.depth.0.max(spec.depth.1));
    let h = hi - lo;
    if !(h.is_finite() && (MIN_HEIGHT..=MAX_HEIGHT).contains(&h)) || lo.abs().max(hi.abs()) > MAX_COORD {
        return Err(BuildError::Height);
    }
    let layout = layout(spec.points, spec.segments, spec.thickness).map_err(BuildError::Curve)?;
    layout.check().map_err(BuildError::Curve)?;
    layout
        .quads
        .iter()
        .map(|q| {
            let corners = quad_corners(spec.kind, q, (lo, hi));
            hull_solid(&corners, spec.material, spec.tex_scale, spec.lightmap_scale, next_side_id)
                .map_err(BuildError::Geom)
        })
        .collect()
}
