use super::error::RoadsideError;

pub type Poly = Vec<(f64, f64)>;

pub const MAX_WIDTH: f64 = 512.0;
pub const MAX_HEIGHT: f64 = 256.0;
pub const MAX_DEPTH: f64 = 256.0;
pub const ARC_STEPS: usize = 4;
const MIN_PIECE: f64 = 0.05;
const FOUNDATION: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SidewalkProfile {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    pub bevel: f64,
    pub slope: f64,
}

impl Default for SidewalkProfile {
    fn default() -> Self {
        Self { width: 64.0, height: 6.0, depth: 16.0, bevel: 0.0, slope: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurbProfile {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    pub bevel: f64,
    pub rounded: bool,
    pub both_edges: bool,
}

impl Default for CurbProfile {
    fn default() -> Self {
        Self { width: 8.0, height: 6.0, depth: 8.0, bevel: 0.0, rounded: false, both_edges: false }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GutterShape {
    #[default]
    V,
    U,
}

impl GutterShape {
    pub fn name(self) -> &'static str {
        match self {
            GutterShape::V => "v",
            GutterShape::U => "u",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "v" => Some(GutterShape::V),
            "u" => Some(GutterShape::U),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GutterProfile {
    pub width: f64,
    pub depth: f64,
    pub shape: GutterShape,
    pub floor_width: f64,
    pub slot: bool,
    pub slot_width: f64,
    pub slot_depth: f64,
    pub slot_every: u32,
}

impl Default for GutterProfile {
    fn default() -> Self {
        Self {
            width: 16.0,
            depth: 3.0,
            shape: GutterShape::V,
            floor_width: 6.0,
            slot: false,
            slot_width: 4.0,
            slot_depth: 12.0,
            slot_every: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    Sidewalk(SidewalkProfile),
    Curb(CurbProfile),
    Gutter(GutterProfile),
}

fn fin(v: f64, lo: f64, hi: f64, what: &'static str) -> Result<f64, RoadsideError> {
    if v.is_finite() && (lo..=hi).contains(&v) { Ok(v) } else { Err(RoadsideError::BadValue(what)) }
}

fn quarter(cx: f64, cy: f64, r: f64, from_deg: f64, to_deg: f64) -> Poly {
    (0..=ARC_STEPS)
        .map(|i| {
            let a = (from_deg + (to_deg - from_deg) * i as f64 / ARC_STEPS as f64).to_radians();
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

impl Profile {
    pub fn extent(&self) -> f64 {
        match self {
            Profile::Sidewalk(p) => p.width,
            Profile::Curb(p) => p.width,
            Profile::Gutter(p) => p.width,
        }
    }

    pub fn validate(&self) -> Result<(), RoadsideError> {
        match self {
            Profile::Sidewalk(p) => {
                fin(p.width, 1.0, MAX_WIDTH, "the sidewalk width must be 1 to 512 units")?;
                fin(p.height, 0.0, MAX_HEIGHT, "the sidewalk height must be 0 to 256 units")?;
                fin(p.depth, 1.0, MAX_DEPTH, "the sidewalk depth under the road must be 1 to 256 units")?;
                fin(p.bevel, 0.0, p.height.min(p.width), "the sidewalk bevel must stay under its height and width")?;
                fin(p.slope, -p.width, p.width, "the sidewalk slope must stay within its width")?;
            }
            Profile::Curb(p) => {
                fin(p.width, 1.0, MAX_WIDTH, "the curb width must be 1 to 512 units")?;
                fin(p.height, 1.0, MAX_HEIGHT, "the curb height must be 1 to 256 units")?;
                fin(p.depth, 1.0, MAX_DEPTH, "the curb depth under the road must be 1 to 256 units")?;
                let limit = p.height.min(if p.both_edges { p.width * 0.5 } else { p.width });
                fin(p.bevel, 0.0, limit, "the curb bevel must stay under its height and width")?;
            }
            Profile::Gutter(p) => {
                fin(p.width, 2.0, MAX_WIDTH, "the gutter width must be 2 to 512 units")?;
                fin(p.depth, 0.5, MAX_DEPTH, "the gutter depth must be 0.5 to 256 units")?;
                fin(p.floor_width, 0.0, p.width, "the gutter floor must stay within its width")?;
                if p.slot {
                    let room = if p.shape == GutterShape::U { p.floor_width } else { p.width * 0.5 };
                    fin(p.slot_width, 0.5, room.max(0.5), "the drain slot must be narrower than the gutter floor")?;
                    fin(p.slot_depth, 1.0, MAX_DEPTH, "the drain slot depth must be 1 to 256 units")?;
                    fin(f64::from(p.slot_every), 1.0, 1024.0, "the drain slot spacing must be 1 to 1024 pieces")?;
                }
            }
        }
        Ok(())
    }

    pub fn polys(&self, segment: Option<usize>) -> Vec<Poly> {
        match self {
            Profile::Sidewalk(p) => vec![sidewalk_poly(p)],
            Profile::Curb(p) => vec![curb_poly(p)],
            Profile::Gutter(p) => gutter_polys(p, segment),
        }
    }
}

fn sidewalk_poly(p: &SidewalkProfile) -> Poly {
    let top = |o: f64| p.height + p.slope * o / p.width;
    let mut out = vec![(0.0, -p.depth)];
    if p.bevel > 1e-9 {
        out.push((0.0, top(0.0) - p.bevel));
        out.push((p.bevel, top(p.bevel)));
    } else {
        out.push((0.0, top(0.0)));
    }
    out.push((p.width, top(p.width)));
    out.push((p.width, -p.depth));
    out
}

fn curb_poly(p: &CurbProfile) -> Poly {
    let size = if p.rounded && p.bevel <= 1e-9 { (p.height.min(p.width) * 0.5).min(p.height) } else { p.bevel };
    let size = if p.both_edges { size.min(p.width * 0.5) } else { size.min(p.width) };
    let mut out = vec![(0.0, -p.depth)];
    if size <= 1e-9 {
        out.push((0.0, p.height));
    } else if p.rounded {
        out.extend(quarter(size, p.height - size, size, 180.0, 90.0));
    } else {
        out.push((0.0, p.height - size));
        out.push((size, p.height));
    }
    if p.both_edges && size > 1e-9 {
        if p.rounded {
            out.extend(quarter(p.width - size, p.height - size, size, 90.0, 0.0));
        } else {
            out.push((p.width - size, p.height));
            out.push((p.width, p.height - size));
        }
    } else {
        out.push((p.width, p.height));
    }
    out.push((p.width, -p.depth));
    out
}

fn block(o0: f64, o1: f64, top0: f64, top1: f64, base: f64) -> Option<Poly> {
    (o1 - o0 >= MIN_PIECE).then(|| vec![(o0, base), (o0, top0), (o1, top1), (o1, base)])
}

fn gutter_polys(p: &GutterProfile, segment: Option<usize>) -> Vec<Poly> {
    let slot_here = p.slot && segment.is_some_and(|s| (s + 1) % p.slot_every.max(1) as usize == 0);
    let base = -(p.depth + if p.slot { p.slot_depth } else { 0.0 } + FOUNDATION);
    let mid = p.width * 0.5;
    let (a, b) = match p.shape {
        GutterShape::V => (mid, mid),
        GutterShape::U => ((p.width - p.floor_width) * 0.5, (p.width + p.floor_width) * 0.5),
    };
    let floor = -p.depth;
    let slope = |o: f64| -> f64 {
        if o <= a && a > 1e-9 {
            floor * (o / a)
        } else if o >= b && p.width - b > 1e-9 {
            floor * ((p.width - o) / (p.width - b))
        } else {
            floor
        }
    };
    let mut out: Vec<Poly> = Vec::new();
    if slot_here {
        let half = p.slot_width * 0.5;
        let (c0, c1) = (mid - half, mid + half);
        let pit = -(p.depth + p.slot_depth);
        out.extend(block(0.0, c0.min(a), 0.0, slope(c0.min(a)), base));
        if p.shape == GutterShape::U {
            out.extend(block(a, c0, floor, floor, base));
            out.extend(block(c1, b, floor, floor, base));
        }
        out.extend(block(c0, c1, pit, pit, base));
        out.extend(block(c1.max(b), p.width, slope(c1.max(b)), 0.0, base));
    } else {
        out.extend(block(0.0, a, 0.0, floor, base));
        if p.shape == GutterShape::U {
            out.extend(block(a, b, floor, floor, base));
        }
        out.extend(block(b, p.width, floor, 0.0, base));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convex(poly: &Poly) -> bool {
        let n = poly.len();
        let mut sign = 0.0f64;
        for i in 0..n {
            let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
            let cross = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0);
            if cross.abs() < 1e-9 {
                continue;
            }
            if sign == 0.0 {
                sign = cross.signum();
            } else if cross.signum() != sign {
                return false;
            }
        }
        true
    }

    #[test]
    fn every_profile_piece_is_convex() {
        let polys = [
            Profile::Sidewalk(SidewalkProfile { bevel: 2.0, slope: 1.5, ..Default::default() }).polys(None),
            Profile::Curb(CurbProfile { bevel: 2.0, rounded: true, both_edges: true, ..Default::default() })
                .polys(None),
            Profile::Curb(CurbProfile { bevel: 3.0, ..Default::default() }).polys(None),
            Profile::Gutter(GutterProfile::default()).polys(None),
            Profile::Gutter(GutterProfile { shape: GutterShape::U, ..Default::default() }).polys(None),
            Profile::Gutter(GutterProfile { shape: GutterShape::U, slot: true, ..Default::default() }).polys(Some(3)),
            Profile::Gutter(GutterProfile { slot: true, ..Default::default() }).polys(Some(3)),
        ];
        for list in polys {
            assert!(!list.is_empty());
            for poly in list {
                assert!(convex(&poly), "{poly:?}");
            }
        }
    }

    #[test]
    fn slot_only_on_every_nth_piece() {
        let g = Profile::Gutter(GutterProfile { slot: true, slot_every: 4, ..Default::default() });
        assert_eq!(g.polys(Some(0)).len(), 2);
        assert_eq!(g.polys(Some(3)).len(), 3);
        assert_eq!(g.polys(None).len(), 2);
    }

    #[test]
    fn validation_refuses_bad_numbers() {
        assert!(Profile::Sidewalk(SidewalkProfile { width: 0.0, ..Default::default() }).validate().is_err());
        assert!(Profile::Sidewalk(SidewalkProfile { bevel: 50.0, ..Default::default() }).validate().is_err());
        assert!(Profile::Curb(CurbProfile { height: f64::NAN, ..Default::default() }).validate().is_err());
        assert!(
            Profile::Gutter(GutterProfile { slot: true, slot_width: 9.0, ..Default::default() }).validate().is_err()
        );
        assert!(Profile::Gutter(GutterProfile::default()).validate().is_ok());
    }
}
