use glam::DVec3;

pub const JOIN_DISTANCE: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShoreStrip {
    pub start: DVec3,
    pub end: DVec3,
    pub toward_water: DVec3,
    pub along: f64,
}

impl ShoreStrip {
    pub fn length(&self) -> f64 {
        (self.end - self.start).length()
    }
}

pub fn plane_section(poly: &[DVec3], normal: DVec3, dist: f64) -> Option<(DVec3, DVec3)> {
    let mut pts: Vec<DVec3> = Vec::with_capacity(2);
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (da, db) = (normal.dot(a) - dist, normal.dot(b) - dist);
        if da.abs() < 1e-6 {
            pts.push(a);
        } else if (da < 0.0) != (db < 0.0) && db.abs() >= 1e-6 {
            pts.push(a + (b - a) * (da / (da - db)));
        }
    }
    pts.dedup_by(|a, b| (*a - *b).length_squared() < 1e-10);
    let (first, last) = (*pts.first()?, *pts.last()?);
    let mut best = (first, last, (last - first).length_squared());
    for i in 0..pts.len() {
        for j in i + 1..pts.len() {
            let d = (pts[j] - pts[i]).length_squared();
            if d > best.2 {
                best = (pts[i], pts[j], d);
            }
        }
    }
    (best.2 > 1e-6).then_some((best.0, best.1))
}

pub fn shore_strips(lands: &[(&[DVec3], DVec3)], water_normal: DVec3, water_dist: f64) -> Vec<ShoreStrip> {
    let segments: Vec<(DVec3, DVec3, DVec3)> = lands
        .iter()
        .filter_map(|(poly, land_normal)| {
            let (a, b) = plane_section(poly, water_normal, water_dist)?;
            let toward_water = (*land_normal - water_normal * land_normal.dot(water_normal)).try_normalize()?;
            let water_left = water_normal.cross(b - a).dot(toward_water) >= 0.0;
            Some(if water_left { (a, b, toward_water) } else { (b, a, toward_water) })
        })
        .collect();
    let joins = |p: DVec3, q: DVec3| (p - q).length_squared() <= JOIN_DISTANCE * JOIN_DISTANCE;
    let mut used = vec![false; segments.len()];
    let mut out = Vec::with_capacity(segments.len());
    let mut along = 0.0;
    while let Some(first) = (0..segments.len()).find(|&i| !used[i]) {
        let head = (0..segments.len())
            .filter(|&i| !used[i])
            .find(|&i| !(0..segments.len()).any(|j| j != i && !used[j] && joins(segments[j].1, segments[i].0)))
            .unwrap_or(first);
        let mut cur = Some(head);
        while let Some(i) = cur {
            used[i] = true;
            let (start, end, toward_water) = segments[i];
            out.push(ShoreStrip { start, end, toward_water, along });
            along += (end - start).length();
            cur = (0..segments.len()).find(|&j| !used[j] && joins(segments[j].0, end));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(x0: f64, y0: f64, x1: f64, y1: f64, outward: DVec3) -> (Vec<DVec3>, DVec3) {
        (
            vec![
                DVec3::new(x0, y0, 0.0),
                DVec3::new(x0, y0, 128.0),
                DVec3::new(x1, y1, 128.0),
                DVec3::new(x1, y1, 0.0),
            ],
            outward,
        )
    }

    #[test]
    fn strips_keep_the_water_on_their_left() {
        let (poly, n) = wall(0.0, 256.0, 0.0, -256.0, DVec3::NEG_X);
        let strips = shore_strips(&[(&poly, n)], DVec3::Z, 64.0);
        assert_eq!(strips.len(), 1);
        let s = strips[0];
        assert_eq!(s.start, DVec3::new(0.0, -256.0, 64.0));
        assert_eq!(s.end, DVec3::new(0.0, 256.0, 64.0));
        assert_eq!(s.toward_water, DVec3::NEG_X);
        assert!(DVec3::Z.cross((s.end - s.start).normalize()).dot(s.toward_water) > 0.99);
        assert_eq!(s.along, 0.0);
    }

    #[test]
    fn strips_follow_the_shore_whatever_the_side_order() {
        let a = wall(0.0, 0.0, 0.0, 100.0, DVec3::NEG_X);
        let b = wall(0.0, 100.0, -50.0, 150.0, DVec3::new(-1.0, -1.0, 0.0).normalize());
        let c = wall(-50.0, 150.0, -50.0, 300.0, DVec3::NEG_X);
        let lands: Vec<(&[DVec3], DVec3)> = vec![(&c.0, c.1), (&a.0, a.1), (&b.0, b.1)];
        let strips = shore_strips(&lands, DVec3::Z, 32.0);
        assert_eq!(strips.len(), 3);
        for pair in strips.windows(2) {
            assert!((pair[0].end - pair[1].start).length() < 1e-9, "{pair:?}");
            assert!((pair[1].along - (pair[0].along + pair[0].length())).abs() < 1e-9);
        }
        assert_eq!(strips[0].start, DVec3::new(0.0, 0.0, 32.0));
        assert_eq!(strips[2].end, DVec3::new(-50.0, 300.0, 32.0));
        for s in &strips {
            assert!(DVec3::Z.cross(s.end - s.start).dot(s.toward_water) > 0.0, "{s:?}");
        }
    }

    #[test]
    fn a_closed_shore_and_a_dry_face_are_handled() {
        let square = [
            wall(-64.0, -64.0, 64.0, -64.0, DVec3::NEG_Y),
            wall(64.0, -64.0, 64.0, 64.0, DVec3::X),
            wall(64.0, 64.0, -64.0, 64.0, DVec3::Y),
            wall(-64.0, 64.0, -64.0, -64.0, DVec3::NEG_X),
        ];
        let lands: Vec<(&[DVec3], DVec3)> = square.iter().map(|(p, n)| (p.as_slice(), *n)).collect();
        let strips = shore_strips(&lands, DVec3::Z, 16.0);
        assert_eq!(strips.len(), 4);
        let total: f64 = strips.iter().map(ShoreStrip::length).sum();
        assert_eq!(total, 512.0);
        assert!((strips[3].end - strips[0].start).length() < 1e-9, "the loop closes");
        assert!(shore_strips(&lands, DVec3::Z, 512.0).is_empty(), "the water plane misses every face");
    }
}
