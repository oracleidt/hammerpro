use glam::DVec3;

use super::error::RoadsideError;
use super::junction::{Sides, keep_ranges, plan, side_index};
use super::profile::Profile;
use super::spine::{MIN_STRIP, Spine};
use super::sweep::sweep;

pub const MAX_GAP: f64 = 512.0;
pub const MAX_RADIUS: f64 = 512.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Feature {
    pub sides: Sides,
    pub gap: f64,
    pub radius: f64,
    pub profile: Profile,
}

const MAX_SPLITS: usize = 64;
const CROSS_MARGIN: f64 = 1.0;

fn crossing(a: &Spine, b: &Spine) -> Option<f64> {
    for i in 0..a.segments() {
        let (p, r) = (a.stations[i], a.stations[i + 1] - a.stations[i]);
        for j in 0..b.segments() {
            let (q, s) = (b.stations[j], b.stations[j + 1] - b.stations[j]);
            let den = r.x * s.y - r.y * s.x;
            if den.abs() < 1e-9 {
                continue;
            }
            let d = q - p;
            let t = (d.x * s.y - d.y * s.x) / den;
            let u = (d.x * r.y - d.y * r.x) / den;
            if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
                continue;
            }
            let at_a = a.cumulative()[i] + r.length() * t;
            let at_b = b.cumulative()[j] + s.length() * u;
            let inside = |at: f64, sp: &Spine| at > CROSS_MARGIN && at < sp.length() - CROSS_MARGIN;
            if inside(at_a, a) && inside(at_b, b) {
                return Some(at_b);
            }
        }
    }
    None
}

pub fn split_crossings(spines: &[Spine]) -> Vec<Spine> {
    let mut list: Vec<Spine> = spines.to_vec();
    for _ in 0..MAX_SPLITS {
        let mut found = None;
        'scan: for a in 0..list.len() {
            for b in 0..list.len() {
                if a != b
                    && let Some(at) = crossing(&list[a], &list[b])
                {
                    found = Some((b, at));
                    break 'scan;
                }
            }
        }
        let Some((b, at)) = found else { break };
        let len = list[b].length();
        match (list[b].sub(0.0, at), list[b].sub(at, len)) {
            (Some(first), Some(second)) => {
                list[b] = first;
                list.insert(b + 1, second);
            }
            _ => break,
        }
    }
    list
}

pub fn build(spines: &[Spine], feature: &Feature) -> Result<Vec<Vec<DVec3>>, RoadsideError> {
    feature.profile.validate()?;
    if !(feature.gap.is_finite() && (0.0..=MAX_GAP).contains(&feature.gap)) {
        return Err(RoadsideError::BadValue("the gap to the road must be 0 to 512 units"));
    }
    if !(feature.radius.is_finite() && (0.0..=MAX_RADIUS).contains(&feature.radius)) {
        return Err(RoadsideError::BadValue("the corner radius must be 0 to 512 units"));
    }
    if spines.is_empty() {
        return Err(RoadsideError::Nothing);
    }
    let split = split_crossings(spines);
    let spines = split.as_slice();
    let layout = plan(spines, feature.sides, feature.gap, feature.radius)?;
    let mut out: Vec<Vec<DVec3>> = Vec::new();
    for (i, spine) in spines.iter().enumerate() {
        for side in [1.0, -1.0] {
            if !feature.sides.has(side) {
                continue;
            }
            let base = spine.half_width + feature.gap;
            for (a, b) in keep_ranges(spine.length(), &layout.removed[i][side_index(side)]) {
                if b - a < MIN_STRIP {
                    continue;
                }
                let part = spine.sub(a, b).ok_or(RoadsideError::TooShort)?;
                out.extend(sweep(&part, side, base, &feature.profile)?);
            }
        }
    }
    for c in &layout.corners {
        for k in 0..c.steps() {
            for poly in feature.profile.polys(None) {
                let mut pts = Vec::with_capacity(poly.len() * 2);
                for (o, y) in &poly {
                    for j in [k, k + 1] {
                        let p = c.point(j, *o);
                        pts.push(DVec3::new(p.x, p.y, c.z_ref(j) + y));
                    }
                }
                if crate::convex_hull(&pts).is_ok() {
                    out.push(pts);
                }
            }
        }
    }
    if out.is_empty() {
        return Err(RoadsideError::Nothing);
    }
    let limit = crate::MAX_COORD;
    if out.iter().flatten().any(|p| !p.is_finite() || p.abs().max_element() > limit) {
        return Err(RoadsideError::OutOfRange);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;
    use crate::roadside::profile::SidewalkProfile;

    fn line(a: (f64, f64), b: (f64, f64)) -> Spine {
        let pts = vec![DVec2::new(a.0, a.1), DVec2::new(b.0, b.1)];
        Spine::new(pts, vec![0.0; 2], vec![0.0; 2], 32.0, 0.0).unwrap()
    }

    fn feature(sides: Sides) -> Feature {
        Feature { sides, gap: 0.0, radius: 16.0, profile: Profile::Sidewalk(SidewalkProfile::default()) }
    }

    #[test]
    fn single_road_gets_one_strip_per_side() {
        let out = build(&[line((0.0, 0.0), (200.0, 0.0))], &feature(Sides::Both)).unwrap();
        assert_eq!(out.len(), 2);
        let one = build(&[line((0.0, 0.0), (200.0, 0.0))], &feature(Sides::Left)).unwrap();
        assert_eq!(one.len(), 1);
        assert!(one[0].iter().all(|p| p.y > 0.0));
    }

    #[test]
    fn t_junction_adds_corner_pieces_and_leaves_the_mouth_open() {
        let main = line((-200.0, 0.0), (200.0, 0.0));
        let side = line((0.0, 300.0), (0.0, 0.0));
        let out = build(&[main, side], &feature(Sides::Both)).unwrap();
        let inside_mouth = out.iter().flatten().filter(|p| p.x.abs() < 30.0 && p.y > 0.0 && p.y < 90.0).count();
        assert_eq!(inside_mouth, 0, "the mouth of the side road must stay open");
        assert!(out.len() > 6);
    }

    #[test]
    fn two_roads_crossing_make_an_x_junction_with_open_mouths() {
        let a = line((-300.0, 0.0), (300.0, 0.0));
        let b = line((0.0, -300.0), (0.0, 300.0));
        assert_eq!(split_crossings(&[a.clone(), b.clone()]).len(), 3);
        let out = build(&[a, b], &feature(Sides::Both)).unwrap();
        let bad = |p: &DVec3| (p.x.abs() < 30.0 && p.y.abs() < 100.0) || (p.y.abs() < 30.0 && p.x.abs() < 100.0);
        assert_eq!(out.iter().flatten().filter(|p| bad(p)).count(), 0);
        assert!(out.len() > 16);
    }

    #[test]
    fn roads_that_do_not_cross_are_left_alone() {
        let a = line((-300.0, 0.0), (300.0, 0.0));
        let b = line((-300.0, 200.0), (300.0, 200.0));
        assert_eq!(split_crossings(&[a, b]).len(), 2);
    }

    #[test]
    fn bad_values_are_refused() {
        let s = [line((0.0, 0.0), (200.0, 0.0))];
        let mut f = feature(Sides::Both);
        f.gap = -1.0;
        assert!(build(&s, &f).is_err());
        f.gap = 0.0;
        f.radius = f64::NAN;
        assert!(build(&s, &f).is_err());
        assert!(build(&[], &feature(Sides::Both)).is_err());
    }
}
