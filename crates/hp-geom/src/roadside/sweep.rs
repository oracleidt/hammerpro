use glam::{DVec2, DVec3};

use super::error::RoadsideError;
use super::profile::Profile;
use super::spine::{Spine, perp_left};

pub const MIN_EDGE: f64 = 0.25;
const MITER_LIMIT_COS: f64 = 0.125;

pub fn mitre_vectors(spine: &Spine) -> Result<Vec<DVec2>, RoadsideError> {
    let n = spine.stations.len();
    let dirs: Vec<DVec2> = (0..n - 1).map(|i| spine.chord_dir(i)).collect();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let v = if i == 0 {
            perp_left(dirs[0])
        } else if i == n - 1 {
            perp_left(dirs[n - 2])
        } else {
            let (a, b) = (perp_left(dirs[i - 1]), perp_left(dirs[i]));
            let sum = a + b;
            let len = sum.length();
            let m = if len > 1e-9 { sum / len } else { b };
            let cos = m.dot(b);
            if cos < MITER_LIMIT_COS {
                return Err(RoadsideError::SharpBend);
            }
            m / cos
        };
        out.push(v);
    }
    Ok(out)
}

pub fn station_z(spine: &Spine, i: usize, side: f64) -> f64 {
    spine.top[i] + spine.edge_shift(spine.roll[i], side)
}

pub fn sweep(spine: &Spine, side: f64, base: f64, profile: &Profile) -> Result<Vec<Vec<DVec3>>, RoadsideError> {
    let mv = mitre_vectors(spine)?;
    let near = side * base;
    let far = side * (base + profile.extent());
    let mut folded = 0;
    for i in 0..spine.segments() {
        let d = spine.chord_dir(i);
        for u in [near, far] {
            let a = spine.stations[i] + mv[i] * u;
            let b = spine.stations[i + 1] + mv[i + 1] * u;
            if (b - a).dot(d) < MIN_EDGE {
                folded += 1;
            }
        }
    }
    if folded > 0 {
        return Err(RoadsideError::Folded(folded));
    }
    let at = |i: usize, o: f64, y: f64| -> DVec3 {
        let p = spine.stations[i] + mv[i] * (side * (base + o));
        DVec3::new(p.x, p.y, station_z(spine, i, side) + y)
    };
    let mut out = Vec::new();
    for seg in 0..spine.segments() {
        for poly in profile.polys(Some(seg)) {
            let mut pts = Vec::with_capacity(poly.len() * 2);
            for (o, y) in &poly {
                pts.push(at(seg, *o, *y));
                pts.push(at(seg + 1, *o, *y));
            }
            out.push(pts);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> Spine {
        let st = vec![DVec2::new(0.0, 0.0), DVec2::new(100.0, 0.0), DVec2::new(200.0, 0.0)];
        Spine::new(st, vec![10.0; 3], vec![0.0; 3], 32.0, 0.0).unwrap()
    }

    #[test]
    fn straight_strip_sits_beside_the_road() {
        let p = Profile::Sidewalk(Default::default());
        let pieces = sweep(&straight(), 1.0, 32.0, &p).unwrap();
        assert_eq!(pieces.len(), 2);
        let pts = &pieces[0];
        let ys: Vec<f64> = pts.iter().map(|p| p.y).collect();
        assert!(ys.iter().all(|y| (32.0..=96.0).contains(y)));
        let zs: Vec<f64> = pts.iter().map(|p| p.z).collect();
        assert_eq!(zs.iter().cloned().fold(f64::MIN, f64::max), 16.0);
        assert_eq!(zs.iter().cloned().fold(f64::MAX, f64::min), -6.0);
    }

    #[test]
    fn right_side_goes_negative() {
        let p = Profile::Sidewalk(Default::default());
        let pieces = sweep(&straight(), -1.0, 32.0, &p).unwrap();
        assert!(pieces[0].iter().all(|p| (-96.0..=-32.0).contains(&p.y)));
    }

    #[test]
    fn banking_lifts_the_edge_reference() {
        let st = vec![DVec2::new(0.0, 0.0), DVec2::new(100.0, 0.0)];
        let s = Spine::new(st, vec![0.0; 2], vec![10.0; 2], 32.0, 0.0).unwrap();
        let p = Profile::Sidewalk(Default::default());
        let left = sweep(&s, 1.0, 32.0, &p).unwrap();
        let right = sweep(&s, -1.0, 32.0, &p).unwrap();
        let max_z = |v: &Vec<Vec<DVec3>>| v[0].iter().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!(max_z(&left) > max_z(&right) + 10.0);
    }

    #[test]
    fn tight_inner_bend_is_refused() {
        let st = vec![DVec2::new(0.0, 0.0), DVec2::new(50.0, 0.0), DVec2::new(50.0, 50.0)];
        let s = Spine::new(st, vec![0.0; 3], vec![0.0; 3], 8.0, 0.0).unwrap();
        let p = Profile::Sidewalk(Default::default());
        assert!(sweep(&s, 1.0, 8.0, &p).is_err());
        assert!(sweep(&s, -1.0, 8.0, &p).is_ok());
    }
}
