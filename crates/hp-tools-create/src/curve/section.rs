use glam::{DVec2, DVec3};
use hp_app::ViewKind;

pub const MAX_ROLL: f64 = 60.0;
pub const MAX_CURB_WIDTH: f64 = 256.0;
pub const MAX_CURB_HEIGHT: f64 = 256.0;
pub const MAX_DITCH_DEPTH: f64 = 512.0;
pub const MAX_AUTO_GAIN: f64 = 90.0;
pub const MAX_AUTO_LIMIT: f64 = 45.0;
pub const MAX_TOTAL_WIDTH: f64 = 2048.0;
pub const AUTO_RADIUS: f64 = 1000.0;
const SMOOTH_PASSES: usize = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    #[default]
    Flat,
    Curb,
    Ditch,
}

impl Profile {
    pub const ALL: [Profile; 3] = [Profile::Flat, Profile::Curb, Profile::Ditch];

    pub fn name(self) -> &'static str {
        match self {
            Profile::Flat => "flat",
            Profile::Curb => "curb",
            Profile::Ditch => "ditch",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub profile: Profile,
    pub curb_width: f64,
    pub curb_height: f64,
    pub ditch_depth: f64,
    pub roll_start: f64,
    pub roll_end: f64,
    pub auto_gain: f64,
    pub auto_limit: f64,
}

impl Default for Section {
    fn default() -> Self {
        Self {
            profile: Profile::Flat,
            curb_width: 8.0,
            curb_height: 8.0,
            ditch_depth: 8.0,
            roll_start: 0.0,
            roll_end: 0.0,
            auto_gain: 0.0,
            auto_limit: 8.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    pub from: f64,
    pub to: f64,
    pub top_from: f64,
    pub top_to: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SectionError {
    DitchTooDeep,
}

impl std::fmt::Display for SectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SectionError::DitchTooDeep => write!(f, "the ditch depth must be at least 1 unit under the height"),
        }
    }
}

fn finite_clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_finite() { v.clamp(lo, hi) } else { lo }
}

impl Section {
    pub fn clamped(self) -> Self {
        Self {
            profile: self.profile,
            curb_width: finite_clamp(self.curb_width, 1.0, MAX_CURB_WIDTH),
            curb_height: finite_clamp(self.curb_height, 0.0, MAX_CURB_HEIGHT),
            ditch_depth: finite_clamp(self.ditch_depth, 0.0, MAX_DITCH_DEPTH),
            roll_start: finite_clamp(self.roll_start, -MAX_ROLL, MAX_ROLL),
            roll_end: finite_clamp(self.roll_end, -MAX_ROLL, MAX_ROLL),
            auto_gain: finite_clamp(self.auto_gain, 0.0, MAX_AUTO_GAIN),
            auto_limit: finite_clamp(self.auto_limit, 0.0, MAX_AUTO_LIMIT),
        }
    }

    pub fn strip_count(&self) -> usize {
        match self.profile {
            Profile::Flat => 1,
            Profile::Curb => 3,
            Profile::Ditch => 2,
        }
    }

    pub fn total_width(&self, thickness: f64) -> f64 {
        let s = self.clamped();
        let extra = if s.profile == Profile::Curb { s.curb_width * 2.0 } else { 0.0 };
        (thickness + extra).min(MAX_TOTAL_WIDTH)
    }

    pub fn has_bank(&self) -> bool {
        self.roll_start != 0.0 || self.roll_end != 0.0 || (self.auto_gain != 0.0 && self.auto_limit != 0.0)
    }

    pub fn strips(&self, thickness: f64) -> Vec<Strip> {
        let s = self.clamped();
        let strip = |from, to, top_from, top_to| Strip { from, to, top_from, top_to };
        match s.profile {
            Profile::Flat => vec![strip(-1.0, 1.0, 0.0, 0.0)],
            Profile::Curb => {
                let total = s.total_width(thickness);
                let road = (thickness / total).clamp(0.0, 1.0);
                let h = s.curb_height;
                vec![strip(-1.0, -road, h, h), strip(-road, road, 0.0, 0.0), strip(road, 1.0, h, h)]
            }
            Profile::Ditch => {
                let d = s.ditch_depth;
                vec![strip(-1.0, 0.0, 0.0, -d), strip(0.0, 1.0, -d, 0.0)]
            }
        }
    }

    pub fn check(&self, height: f64) -> Result<(), SectionError> {
        let s = self.clamped();
        if s.profile == Profile::Ditch && s.ditch_depth > height - 1.0 {
            return Err(SectionError::DitchTooDeep);
        }
        Ok(())
    }
}

pub fn curvature(centers: &[DVec2]) -> Vec<f64> {
    let n = centers.len();
    if n < 3 {
        return vec![0.0; n];
    }
    let dirs: Vec<DVec2> = centers.windows(2).map(|w| (w[1] - w[0]).normalize_or_zero()).collect();
    let lens: Vec<f64> = centers.windows(2).map(|w| w[0].distance(w[1])).collect();
    let mut k = vec![0.0; n];
    for i in 1..n - 1 {
        let (a, b) = (dirs[i - 1], dirs[i]);
        let angle = a.perp_dot(b).atan2(a.dot(b));
        let arc = (lens[i - 1] + lens[i]) * 0.5;
        k[i] = if arc > 1e-9 { angle / arc } else { 0.0 };
    }
    k[0] = k[1];
    k[n - 1] = k[n - 2];
    for _ in 0..SMOOTH_PASSES {
        let prev = k.clone();
        for i in 1..n - 1 {
            k[i] = (prev[i - 1] + 2.0 * prev[i] + prev[i + 1]) * 0.25;
        }
    }
    k
}

pub fn station_rolls(centers: &[DVec2], section: &Section) -> Vec<f64> {
    let s = section.clamped();
    let n = centers.len();
    if n == 0 {
        return Vec::new();
    }
    let mut cumulative = Vec::with_capacity(n);
    let mut total = 0.0;
    cumulative.push(0.0);
    for w in centers.windows(2) {
        total += w[0].distance(w[1]);
        cumulative.push(total);
    }
    let kappa = if s.auto_gain > 0.0 && s.auto_limit > 0.0 { curvature(centers) } else { vec![0.0; n] };
    (0..n)
        .map(|i| {
            let t = if total > 1e-9 { cumulative[i] / total } else { 0.0 };
            let manual = s.roll_start + (s.roll_end - s.roll_start) * t;
            let auto = (-s.auto_gain * kappa[i] * AUTO_RADIUS).clamp(-s.auto_limit, s.auto_limit);
            (manual + auto).clamp(-MAX_ROLL, MAX_ROLL)
        })
        .collect()
}

pub fn bank_shift(fraction: f64, half_width: f64, roll_degrees: f64) -> f64 {
    fraction * half_width * roll_degrees.to_radians().tan()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub quad: usize,
    pub corners: [DVec3; 8],
}

#[derive(Clone, Copy)]
pub struct PieceInput<'a> {
    pub kind: ViewKind,
    pub centers: &'a [DVec2],
    pub quads: &'a [[DVec2; 4]],
    pub depth: (f64, f64),
    pub thickness: f64,
    pub section: &'a Section,
}

pub fn pieces(input: &PieceInput<'_>) -> Vec<Piece> {
    let PieceInput { kind, centers, quads, depth, thickness, section } = *input;
    let (lo, hi) = (depth.0.min(depth.1), depth.0.max(depth.1));
    let half = section.total_width(thickness) * 0.5;
    let strips = section.strips(thickness);
    let rolls = station_rolls(centers, section);
    let mut out = Vec::with_capacity(quads.len() * strips.len());
    for (qi, q) in quads.iter().enumerate() {
        let (right, left) = ([q[0], q[1]], [q[3], q[2]]);
        let at = |station: usize, f: f64, base: f64, top: f64| {
            let u = (f + 1.0) * 0.5;
            let p = right[station].lerp(left[station], u);
            let shift = rolls.get(qi + station).map_or(0.0, |r| bank_shift(f, half, *r));
            kind.from_2d(p, base + top + shift)
        };
        for s in &strips {
            let corners = [
                at(0, s.from, lo, 0.0),
                at(0, s.to, lo, 0.0),
                at(1, s.from, lo, 0.0),
                at(1, s.to, lo, 0.0),
                at(0, s.from, hi, s.top_from),
                at(0, s.to, hi, s.top_to),
                at(1, s.from, hi, s.top_from),
                at(1, s.to, hi, s.top_to),
            ];
            out.extend(convex_pieces(qi, corners));
        }
    }
    out
}

fn skew(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> f64 {
    let n = (c - a).cross(d - b);
    if n.length() <= 1e-12 { 0.0 } else { (n.normalize().dot(b - a)).abs() }
}

fn is_planar(c: &[DVec3; 8]) -> bool {
    let scale = c.iter().fold(1.0f64, |m, p| m.max(p.abs().max_element()));
    let tolerance = (scale * 1e-12).max(1e-7) * 0.5;
    skew(c[0], c[1], c[3], c[2]) <= tolerance && skew(c[4], c[5], c[7], c[6]) <= tolerance
}

fn prism(quad: usize, c: &[DVec3; 8], tri: [usize; 3]) -> Piece {
    let [a, b, d] = tri;
    Piece { quad, corners: [c[a], c[b], c[d], c[d], c[a + 4], c[b + 4], c[d + 4], c[d + 4]] }
}

fn area(c: &[DVec3; 8], tri: [usize; 3]) -> f64 {
    let [a, b, d] = tri;
    (c[b] - c[a]).cross(c[d] - c[a]).length()
}

fn convex_pieces(quad: usize, corners: [DVec3; 8]) -> Vec<Piece> {
    if is_planar(&corners) {
        return vec![Piece { quad, corners }];
    }
    let first = [[0, 1, 3], [0, 3, 2]];
    let second = [[0, 1, 2], [1, 3, 2]];
    let worst = |split: &[[usize; 3]; 2]| split.iter().map(|t| area(&corners, *t)).fold(f64::MAX, f64::min);
    let split = if worst(&first) >= worst(&second) { first } else { second };
    split.iter().map(|t| prism(quad, &corners, *t)).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::curve::geom::{CurveLayout, layout};

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    fn input<'a>(l: &'a CurveLayout, section: &'a Section, thickness: f64) -> PieceInput<'a> {
        PieceInput { kind: ViewKind::Top, centers: &l.centers, quads: &l.quads, depth: (0.0, 32.0), thickness, section }
    }

    #[test]
    fn piece_count_follows_the_profile() {
        let pts = [v(0.0, 0.0), v(256.0, 0.0), v(512.0, 128.0)];
        for (profile, per) in [(Profile::Flat, 1), (Profile::Curb, 3), (Profile::Ditch, 2)] {
            let section = Section { profile, ..Section::default() };
            let l = layout(&pts, 10, section.total_width(32.0)).unwrap();
            let list = pieces(&input(&l, &section, 32.0));
            assert_eq!(list.len(), 10 * per);
            assert_eq!(section.strip_count(), per);
        }
    }

    #[test]
    fn curb_adds_width_and_height_on_both_sides() {
        let section = Section { profile: Profile::Curb, curb_width: 6.0, curb_height: 5.0, ..Section::default() };
        assert_eq!(section.total_width(32.0), 44.0);
        let l = layout(&[v(0.0, 0.0), v(100.0, 0.0)], 2, 44.0).unwrap();
        let list = pieces(&input(&l, &section, 32.0));
        let first: Vec<&Piece> = list.iter().filter(|p| p.quad == 0).collect();
        assert_eq!(first.len(), 3);
        let top = |p: &Piece| p.corners[4..].iter().map(|c| c.z).fold(f64::MIN, f64::max);
        assert_eq!(top(first[0]), 37.0);
        assert_eq!(top(first[1]), 32.0);
        assert_eq!(top(first[2]), 37.0);
        let ys: Vec<f64> = first.iter().flat_map(|p| p.corners.iter().map(|c| c.y)).collect();
        let (min, max) = ys.iter().fold((f64::MAX, f64::MIN), |(a, b), y| (a.min(*y), b.max(*y)));
        assert!((max - min - 44.0).abs() < 1e-9);
    }

    #[test]
    fn ditch_is_a_v_and_is_refused_when_too_deep() {
        let section = Section { profile: Profile::Ditch, ditch_depth: 10.0, ..Section::default() };
        let l = layout(&[v(0.0, 0.0), v(100.0, 0.0)], 2, 40.0).unwrap();
        let list = pieces(&input(&l, &section, 40.0));
        let first: Vec<&Piece> = list.iter().filter(|p| p.quad == 0).collect();
        assert_eq!(first.len(), 2);
        let lowest_top = |p: &Piece| p.corners[4..].iter().map(|c| c.z).fold(f64::MAX, f64::min);
        assert_eq!(lowest_top(first[0]), 22.0);
        assert_eq!(lowest_top(first[1]), 22.0);
        assert!(section.check(32.0).is_ok());
        assert!(section.check(10.5).is_err());
    }

    #[test]
    fn joints_are_continuous() {
        let pts = [v(0.0, 0.0), v(200.0, 50.0), v(400.0, -30.0), v(600.0, 100.0)];
        let section = Section {
            profile: Profile::Curb,
            roll_start: 6.0,
            roll_end: -9.0,
            auto_gain: 20.0,
            auto_limit: 10.0,
            ..Section::default()
        };
        let l = layout(&pts, 24, section.total_width(48.0)).unwrap();
        let list = pieces(&input(&l, &section, 48.0));
        let points =
            |quad: usize| -> Vec<DVec3> { list.iter().filter(|p| p.quad == quad).flat_map(|p| p.corners).collect() };
        for i in 0..23 {
            let (a, b) = (l.quads[i][1], l.quads[i][2]);
            let on_edge = |c: &DVec3| {
                let p = DVec2::new(c.x, c.y);
                let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                p.distance(a + (b - a) * t) < 1e-9
            };
            let (this, next) = (points(i), points(i + 1));
            for c in this.iter().filter(|c| on_edge(c)) {
                assert!(next.iter().any(|q| q.distance(*c) < 1e-9), "joint {i}");
            }
            for c in next.iter().filter(|c| on_edge(c)) {
                assert!(this.iter().any(|q| q.distance(*c) < 1e-9), "joint {i}");
            }
        }
    }

    #[test]
    fn banked_pieces_are_convex_with_planar_faces() {
        let pts = [v(0.0, 0.0), v(200.0, 50.0), v(400.0, -30.0), v(600.0, 100.0)];
        for profile in Profile::ALL {
            let section = Section {
                profile,
                roll_start: 6.0,
                roll_end: -9.0,
                auto_gain: 20.0,
                auto_limit: 10.0,
                ..Section::default()
            };
            let l = layout(&pts, 24, section.total_width(48.0)).unwrap();
            let list = pieces(&input(&l, &section, 48.0));
            assert!(list.len() >= 24 * section.strip_count());
            for piece in &list {
                let mut next = 1u32;
                let sides = hp_geom::hull_solid(&piece.corners, "m", 0.25, 16, &mut || {
                    next += 1;
                    next
                })
                .unwrap();
                assert!(sides.len() <= 6, "{profile:?}: {} faces", sides.len());
            }
        }
    }

    #[test]
    fn flat_straight_roads_stay_in_one_piece_per_strip() {
        let section = Section { roll_start: 8.0, roll_end: 8.0, ..Section::default() };
        let l = layout(&[v(0.0, 0.0), v(300.0, 0.0)], 6, 40.0).unwrap();
        assert_eq!(pieces(&input(&l, &section, 40.0)).len(), 6);
        let twisted = Section { roll_start: 8.0, roll_end: -8.0, ..Section::default() };
        assert!(pieces(&input(&l, &twisted, 40.0)).len() > 6);
    }

    #[test]
    fn manual_roll_is_interpolated_along_the_curve() {
        let centers: Vec<DVec2> = (0..=10).map(|i| v(f64::from(i) * 10.0, 0.0)).collect();
        let section = Section { roll_start: 10.0, roll_end: -10.0, ..Section::default() };
        let r = station_rolls(&centers, &section);
        assert_eq!(r.len(), 11);
        assert!((r[0] - 10.0).abs() < 1e-9);
        assert!(r[5].abs() < 1e-9);
        assert!((r[10] + 10.0).abs() < 1e-9);
        assert!((r[2] - 6.0).abs() < 1e-9);
    }

    #[test]
    fn auto_bank_follows_curvature_and_is_limited() {
        let arc: Vec<DVec2> = (0..=20).map(|k| DVec2::from_angle(f64::from(k) * 0.05) * 500.0).collect();
        let soft = Section { auto_gain: 4.0, auto_limit: 20.0, ..Section::default() };
        let r = station_rolls(&arc, &soft);
        assert!(r[10] < 0.0, "a left turn lowers the left side");
        assert!((r[10] + 8.0).abs() < 0.2, "radius 500 with gain 4 gives about 8 degrees: {}", r[10]);
        let hard = Section { auto_gain: 40.0, auto_limit: 3.0, ..Section::default() };
        let r = station_rolls(&arc, &hard);
        assert!(r.iter().all(|x| (x + 3.0).abs() < 1e-9));
        let mirrored: Vec<DVec2> = arc.iter().map(|p| v(p.x, -p.y)).collect();
        let r = station_rolls(&mirrored, &soft);
        assert!(r[10] > 0.0);
        let straight: Vec<DVec2> = (0..=10).map(|i| v(f64::from(i) * 10.0, 0.0)).collect();
        assert!(station_rolls(&straight, &soft).iter().all(|x| x.abs() < 1e-9));
        assert!(station_rolls(&arc, &Section::default()).iter().all(|x| *x == 0.0));
    }

    #[test]
    fn bank_raises_one_side_and_lowers_the_other() {
        let section = Section { roll_start: 10.0, roll_end: 10.0, ..Section::default() };
        let l = layout(&[v(0.0, 0.0), v(100.0, 0.0)], 2, 40.0).unwrap();
        let list = pieces(&input(&l, &section, 40.0));
        let expected = 20.0 * 10.0f64.to_radians().tan();
        let p = &list[0];
        let (a, b) = (p.corners[0], p.corners[1]);
        assert!((a.z.abs() - expected).abs() < 1e-9);
        assert!((a.z + b.z).abs() < 1e-9);
        assert!((p.corners[4].z - p.corners[0].z - 32.0).abs() < 1e-9);
    }

    #[test]
    fn values_are_clamped_and_parsed() {
        let s = Section { roll_start: 500.0, auto_limit: f64::NAN, curb_width: 0.0, ..Section::default() }.clamped();
        assert_eq!(s.roll_start, MAX_ROLL);
        assert_eq!(s.auto_limit, 0.0);
        assert_eq!(s.curb_width, 1.0);
        assert_eq!(Profile::parse(" Curb "), Some(Profile::Curb));
        assert_eq!(Profile::parse("x"), None);
        assert!(!Section::default().has_bank());
    }
}
