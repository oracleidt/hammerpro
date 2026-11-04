use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub origin: DVec3,
    pub normal: DVec3,
}

impl Plane {
    pub fn signed_distance(&self, p: DVec3) -> f64 {
        (p - self.origin).dot(self.normal)
    }
}

fn basis(up: DVec3) -> (DVec3, DVec3) {
    let helper = if up.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = up.cross(helper).normalize_or_zero();
    (u, up.cross(u).normalize_or_zero())
}

pub fn pinch_target(pos: DVec3, center: DVec3, plane_normal: DVec3, step: f64) -> DVec3 {
    let to = center - pos;
    let flat = to - plane_normal * to.dot(plane_normal);
    let len = flat.length();
    if !step.is_finite() || !len.is_finite() || len < 1e-9 {
        return pos;
    }
    let travel = if step >= 0.0 { step.min(len) } else { step };
    pos + flat / len * travel
}

pub fn inflate_target(pos: DVec3, normal: DVec3, step: f64) -> DVec3 {
    match normal.try_normalize() {
        Some(n) if step.is_finite() => pos + n * step,
        _ => pos,
    }
}

pub fn grab_target(original: DVec3, delta: DVec3, weight: f64) -> DVec3 {
    if !delta.is_finite() || !weight.is_finite() {
        return original;
    }
    original + delta * weight
}

pub fn view_plane_point(origin: DVec3, dir: DVec3, anchor: DVec3, forward: DVec3) -> Option<DVec3> {
    let denom = dir.dot(forward);
    if !denom.is_finite() || denom.abs() < 1e-6 {
        return None;
    }
    let t = (anchor - origin).dot(forward) / denom;
    (t.is_finite() && t > 0.0).then(|| origin + dir * t)
}

pub fn fit_plane(samples: &[(DVec3, f64)], up: DVec3) -> Option<Plane> {
    let up = up.try_normalize()?;
    let total: f64 = samples.iter().map(|(_, w)| *w).filter(|w| w.is_finite() && *w > 0.0).sum();
    if total < 1e-12 {
        return None;
    }
    let origin = samples.iter().filter(|(_, w)| *w > 0.0).fold(DVec3::ZERO, |acc, (p, w)| acc + *p * *w) / total;
    if !origin.is_finite() {
        return None;
    }
    let (u, v) = basis(up);
    let (mut suu, mut suv, mut svv, mut suh, mut svh) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (p, w) in samples.iter().filter(|(_, w)| *w > 0.0) {
        let d = *p - origin;
        let (x, y, h) = (d.dot(u), d.dot(v), d.dot(up));
        suu += w * x * x;
        suv += w * x * y;
        svv += w * y * y;
        suh += w * x * h;
        svh += w * y * h;
    }
    let det = suu * svv - suv * suv;
    let (a, b) = if det.abs() > 1e-9 * (suu * svv).abs().max(1e-12) {
        ((suh * svv - svh * suv) / det, (svh * suu - suh * suv) / det)
    } else {
        (0.0, 0.0)
    };
    let normal = (up - u * a - v * b).try_normalize()?;
    Some(Plane { origin, normal })
}

pub fn flatten_target(pos: DVec3, plane: &Plane, amount: f64) -> DVec3 {
    if !amount.is_finite() {
        return pos;
    }
    let t = amount.clamp(-1.0, 1.0);
    pos - plane.normal * (plane.signed_distance(pos) * t)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    #[test]
    fn pinch_moves_toward_the_center_in_the_plane_and_never_overshoots() {
        let p = pinch_target(DVec3::new(10.0, 0.0, 5.0), DVec3::ZERO, DVec3::Z, 4.0);
        assert!((p - DVec3::new(6.0, 0.0, 5.0)).length() < 1e-12);
        let p = pinch_target(DVec3::new(3.0, 0.0, 5.0), DVec3::ZERO, DVec3::Z, 50.0);
        assert!((p - DVec3::new(0.0, 0.0, 5.0)).length() < 1e-12);
        let p = pinch_target(DVec3::new(10.0, 0.0, 5.0), DVec3::ZERO, DVec3::Z, -4.0);
        assert!((p - DVec3::new(14.0, 0.0, 5.0)).length() < 1e-12, "negative spreads");
        assert_eq!(pinch_target(DVec3::new(0.0, 0.0, 5.0), DVec3::ZERO, DVec3::Z, 4.0), DVec3::new(0.0, 0.0, 5.0));
    }

    #[test]
    fn inflate_follows_the_vertex_normal() {
        let n = DVec3::new(0.0, 3.0, 4.0);
        let p = inflate_target(DVec3::ZERO, n, 5.0);
        assert!((p - DVec3::new(0.0, 3.0, 4.0)).length() < 1e-12);
        assert_eq!(inflate_target(DVec3::ONE, DVec3::ZERO, 5.0), DVec3::ONE);
        assert_eq!(inflate_target(DVec3::ONE, DVec3::Z, f64::NAN), DVec3::ONE);
    }

    #[test]
    fn view_plane_point_keeps_the_depth_of_the_anchor() {
        let forward = DVec3::X;
        let anchor = DVec3::new(100.0, 0.0, 0.0);
        let p = view_plane_point(DVec3::ZERO, DVec3::new(1.0, 0.5, -0.25).normalize(), anchor, forward).unwrap();
        assert!((p.x - 100.0).abs() < 1e-9);
        assert!((p.y - 50.0).abs() < 1e-9 && (p.z + 25.0).abs() < 1e-9);
        let q = view_plane_point(DVec3::ZERO, DVec3::X, anchor, forward).unwrap();
        assert!((q - anchor).length() < 1e-9);
        assert!(view_plane_point(DVec3::ZERO, DVec3::Y, anchor, forward).is_none());
        assert!(view_plane_point(DVec3::ZERO, -DVec3::X, anchor, forward).is_none());
    }

    #[test]
    fn grab_scales_the_drag_by_the_weight() {
        let p = grab_target(DVec3::new(1.0, 2.0, 3.0), DVec3::new(10.0, 0.0, -4.0), 0.5);
        assert!((p - DVec3::new(6.0, 2.0, 1.0)).length() < 1e-12);
        assert_eq!(grab_target(DVec3::ONE, DVec3::new(f64::NAN, 0.0, 0.0), 1.0), DVec3::ONE);
    }

    #[test]
    fn fit_plane_recovers_a_tilted_plane() {
        let mut samples = Vec::new();
        for i in -3..=3 {
            for j in -3..=3 {
                let (x, y) = (f64::from(i) * 8.0, f64::from(j) * 8.0);
                samples.push((DVec3::new(x, y, 0.5 * x + 7.0), 1.0));
            }
        }
        let plane = fit_plane(&samples, DVec3::Z).unwrap();
        let expected = DVec3::new(-0.5, 0.0, 1.0).normalize();
        assert!((plane.normal - expected).length() < 1e-9, "{:?}", plane.normal);
        for (p, _) in &samples {
            assert!(plane.signed_distance(*p).abs() < 1e-9);
        }
    }

    #[test]
    fn fit_plane_of_a_line_or_nothing_falls_back_or_fails() {
        assert!(fit_plane(&[], DVec3::Z).is_none());
        assert!(fit_plane(&[(DVec3::ONE, 0.0)], DVec3::Z).is_none());
        let plane = fit_plane(&[(DVec3::new(1.0, 0.0, 4.0), 1.0)], DVec3::Z).unwrap();
        assert!((plane.normal - DVec3::Z).length() < 1e-12);
        assert!((plane.origin.z - 4.0).abs() < 1e-12);
    }

    #[test]
    fn flatten_moves_onto_the_plane_by_the_amount() {
        let plane = Plane { origin: DVec3::new(0.0, 0.0, 10.0), normal: DVec3::Z };
        assert!((flatten_target(DVec3::new(2.0, 3.0, 20.0), &plane, 0.5).z - 15.0).abs() < 1e-12);
        assert!((flatten_target(DVec3::new(2.0, 3.0, 20.0), &plane, 1.0).z - 10.0).abs() < 1e-12);
        assert!((flatten_target(DVec3::new(2.0, 3.0, 20.0), &plane, 5.0).z - 10.0).abs() < 1e-12, "no overshoot");
        assert!((flatten_target(DVec3::new(2.0, 3.0, 20.0), &plane, -0.5).z - 25.0).abs() < 1e-12, "away");
    }

    #[test]
    fn weighted_plane_ignores_zero_weights() {
        let samples = [
            (DVec3::new(0.0, 0.0, 0.0), 1.0),
            (DVec3::new(10.0, 0.0, 0.0), 1.0),
            (DVec3::new(0.0, 10.0, 0.0), 1.0),
            (DVec3::new(5.0, 5.0, 500.0), 0.0),
        ];
        let plane = fit_plane(&samples, DVec3::Z).unwrap();
        assert!((plane.normal - DVec3::Z).length() < 1e-9);
    }
}
