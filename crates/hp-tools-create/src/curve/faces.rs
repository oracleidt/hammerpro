use glam::{DVec2, DVec3};
use hp_app::ViewKind;

use super::geom::{MAX_COORD, MIN_HEIGHT, clamp_thickness};

pub const ARCH_POINTS: usize = 9;
pub const MIN_SPAN: f64 = 2.0;
pub const MIN_RISE: f64 = 1.0;
const AXIS_TOLERANCE: f64 = 0.01;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Inner,
    Center,
    Outer,
}

impl Align {
    pub const ALL: [Align; 3] = [Align::Inner, Align::Center, Align::Outer];

    pub fn name(self) -> &'static str {
        match self {
            Align::Inner => "inner",
            Align::Center => "center",
            Align::Outer => "outer",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name().eq_ignore_ascii_case(s.trim()))
    }

    fn sign(self) -> f64 {
        match self {
            Align::Inner => 1.0,
            Align::Center => 0.0,
            Align::Outer => -1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FacePick {
    pub point: DVec3,
    pub normal: DVec3,
    pub polygon: Vec<DVec3>,
}

impl FacePick {
    pub fn new(point: DVec3, normal: DVec3, polygon: Vec<DVec3>) -> Self {
        Self { point, normal, polygon }
    }

    pub fn top(&self) -> f64 {
        self.polygon.iter().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max)
    }

    pub fn bottom(&self) -> f64 {
        self.polygon.iter().map(|p| p.z).fold(f64::INFINITY, f64::min)
    }

    fn range(&self, axis: usize) -> (f64, f64) {
        self.polygon.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p[axis]), hi.max(p[axis])))
    }

    fn plane_at(&self, axis: usize) -> f64 {
        let (lo, hi) = self.range(axis);
        if lo.is_finite() && hi.is_finite() { (lo + hi) * 0.5 } else { self.point[axis] }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArchError {
    NotVertical,
    NotAligned,
    TooNarrow,
    TooFlat,
    NoDepth,
    OutOfRange,
}

impl std::fmt::Display for ArchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchError::NotVertical => {
                write!(f, "pick vertical faces (the sides of the opening), not floors or ceilings")
            }
            ArchError::NotAligned => {
                write!(f, "the two faces must face along the same axis, X or Y (rotated walls are not supported)")
            }
            ArchError::TooNarrow => write!(f, "the two faces must be at least {MIN_SPAN} units apart"),
            ArchError::TooFlat => {
                write!(f, "the middle point must be at least {MIN_RISE} unit above or under the ends")
            }
            ArchError::NoDepth => write!(f, "the faces must be at least {MIN_HEIGHT} unit deep"),
            ArchError::OutOfRange => write!(f, "the arch goes outside the map (coordinates up to {MAX_COORD})"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchPlan {
    pub kind: ViewKind,
    pub points: Vec<DVec2>,
    pub depth: (f64, f64),
    pub cap: DVec2,
    pub span: f64,
    pub ends: (DVec2, DVec2),
}

pub fn span_axis(normal: DVec3) -> Result<usize, ArchError> {
    let n = normal.try_normalize().ok_or(ArchError::NotVertical)?;
    if n.z.abs() > AXIS_TOLERANCE {
        return Err(ArchError::NotVertical);
    }
    if n.y.abs() <= AXIS_TOLERANCE {
        Ok(0)
    } else if n.x.abs() <= AXIS_TOLERANCE {
        Ok(1)
    } else {
        Err(ArchError::NotAligned)
    }
}

pub fn kind_for_span(axis: usize) -> ViewKind {
    if axis == 0 { ViewKind::Side } else { ViewKind::Front }
}

pub fn opening_depth(a: &FacePick, b: &FacePick, axis: usize) -> Result<(f64, f64), ArchError> {
    let d = 1 - axis;
    let (a0, a1) = a.range(d);
    let (b0, b1) = b.range(d);
    let (lo, hi) = (a0.max(b0), a1.min(b1));
    let (lo, hi) = if hi - lo >= MIN_HEIGHT { (lo, hi) } else { (a0.min(b0), a1.max(b1)) };
    if !(lo.is_finite() && hi.is_finite()) || hi - lo < MIN_HEIGHT {
        return Err(ArchError::NoDepth);
    }
    Ok((lo, hi))
}

pub fn default_apex(a: &FacePick, b: &FacePick) -> f64 {
    let Ok(axis) = span_axis(a.normal) else { return (a.point.z + b.point.z) * 0.5 + 64.0 };
    let span = (b.plane_at(axis) - a.plane_at(axis)).abs();
    (a.point.z + b.point.z) * 0.5 + (span * 0.5).max(MIN_RISE)
}

pub fn plan_arch(a: &FacePick, b: &FacePick, apex: f64, thickness: f64, align: Align) -> Result<ArchPlan, ArchError> {
    let axis = span_axis(a.normal)?;
    if span_axis(b.normal)? != axis {
        return Err(ArchError::NotAligned);
    }
    let kind = kind_for_span(axis);
    let (xa, xb) = (a.plane_at(axis), b.plane_at(axis));
    let span = (xb - xa).abs();
    if !span.is_finite() || span < MIN_SPAN {
        return Err(ArchError::TooNarrow);
    }
    let depth = opening_depth(a, b, axis)?;
    let (za, zb) = (a.point.z, b.point.z);
    let rise = apex - (za + zb) * 0.5;
    if !rise.is_finite() || rise.abs() < MIN_RISE {
        return Err(ArchError::TooFlat);
    }
    let t = clamp_thickness(thickness);
    let off = align.sign() * t * 0.5;
    let s = (xb - xa).signum();
    let half = span * 0.5;
    let mid = (xa + xb) * 0.5;
    let n = ARCH_POINTS - 1;
    let points: Vec<DVec2> = (0..=n)
        .map(|k| {
            let th = std::f64::consts::PI * k as f64 / n as f64;
            let (sin, cos) = th.sin_cos();
            let base = za + (zb - za) * (1.0 - cos) * 0.5;
            let p = DVec2::new(mid - s * half * cos, base + rise * sin);
            let normal = DVec2::new(-s * cos / half, sin / rise).try_normalize().unwrap_or(DVec2::new(-s * cos, 0.0));
            let q = p + normal * off;
            match k {
                0 => DVec2::new(xa + off * -s, za),
                k if k == n => DVec2::new(xb + off * s, zb),
                _ => q,
            }
        })
        .collect();
    if points.iter().any(|p| !p.is_finite() || p.abs().max_element() > MAX_COORD) {
        return Err(ArchError::OutOfRange);
    }
    Ok(ArchPlan { kind, points, depth, cap: DVec2::X, span, ends: (DVec2::new(xa, za), DVec2::new(xb, zb)) })
}

pub fn edge_height(face: &FacePick, z: f64, tolerance: f64) -> Option<f64> {
    face.polygon
        .iter()
        .map(|v| ((v.z - z).abs(), v.z))
        .filter(|(d, _)| *d <= tolerance)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, v)| v)
}
