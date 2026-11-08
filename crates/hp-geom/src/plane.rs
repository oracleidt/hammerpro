use glam::DVec3;
use hp_vmf::Side;

use crate::util::clean_coord;
use crate::{PLANE_EPS, Ray};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaneSide {
    Front,
    Back,
    On,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub normal: DVec3,
    pub dist: f64,
}

impl Plane {
    pub fn new(normal: DVec3, dist: f64) -> Option<Self> {
        let n = normal.try_normalize()?;
        dist.is_finite().then_some(Self { normal: n, dist })
    }

    pub fn from_normal_point(normal: DVec3, point: DVec3) -> Option<Self> {
        let n = normal.try_normalize()?;
        let dist = n.dot(point);
        dist.is_finite().then_some(Self { normal: n, dist })
    }

    pub fn from_points(a: DVec3, b: DVec3, c: DVec3) -> Option<Self> {
        let e1 = a - b;
        let e2 = c - b;
        let n = e1.cross(e2);
        let len = n.length();
        let scale = e1.length() * e2.length();
        if !len.is_finite() || len <= 1e-12 || len <= scale * 1e-10 {
            return None;
        }
        let normal = n / len;
        let dist = normal.dot(a);
        dist.is_finite().then_some(Self { normal, dist })
    }

    #[inline]
    pub fn from_side(side: &Side) -> Option<Self> {
        let [a, b, c] = side.plane;
        Self::from_points(a, b, c)
    }

    #[inline]
    pub fn distance(&self, p: DVec3) -> f64 {
        self.normal.dot(p) - self.dist
    }

    #[inline]
    pub fn flip(&self) -> Self {
        Self { normal: -self.normal, dist: -self.dist }
    }

    #[inline]
    pub fn offset(&self, offset: f64) -> Self {
        Self { normal: self.normal, dist: self.dist + offset }
    }

    #[inline]
    pub fn classify(&self, p: DVec3) -> PlaneSide {
        self.classify_eps(p, PLANE_EPS)
    }

    #[inline]
    pub fn classify_eps(&self, p: DVec3, eps: f64) -> PlaneSide {
        let d = self.distance(p);
        if d > eps {
            PlaneSide::Front
        } else if d < -eps {
            PlaneSide::Back
        } else {
            PlaneSide::On
        }
    }

    #[inline]
    pub fn project(&self, p: DVec3) -> DVec3 {
        p - self.normal * self.distance(p)
    }

    pub fn dominant_axis(&self) -> usize {
        let a = self.normal.abs();
        if a.z >= a.x && a.z >= a.y {
            2
        } else if a.x >= a.y {
            0
        } else {
            1
        }
    }

    pub fn approx_eq(&self, other: &Plane, normal_eps: f64, dist_eps: f64) -> bool {
        (self.normal - other.normal).abs().max_element() <= normal_eps && (self.dist - other.dist).abs() <= dist_eps
    }

    pub fn intersect_ray(&self, ray: &Ray) -> Option<f64> {
        let denom = self.normal.dot(ray.dir);
        if denom.abs() < 1e-15 {
            return None;
        }
        let t = -self.distance(ray.origin) / denom;
        t.is_finite().then_some(t)
    }

    pub fn intersect_three(a: &Plane, b: &Plane, c: &Plane) -> Option<DVec3> {
        let bc = b.normal.cross(c.normal);
        let denom = a.normal.dot(bc);
        if denom.abs() < 1e-12 {
            return None;
        }
        let p = (bc * a.dist + c.normal.cross(a.normal) * b.dist + a.normal.cross(b.normal) * c.dist) / denom;
        p.is_finite().then_some(p)
    }

    pub fn to_points_at(&self, anchor: DVec3) -> [DVec3; 3] {
        let k = self.dominant_axis();
        let j = (0..3)
            .filter(|i| *i != k)
            .min_by(|a, b| self.normal[*a].abs().total_cmp(&self.normal[*b].abs()))
            .unwrap_or((k + 2) % 3);
        let i = 3 - k - j;
        let (n, dist) = if self.normal[k] < 0.0 { (-self.normal, -self.dist) } else { (self.normal, self.dist) };
        if !anchor.is_finite() || !n.is_finite() || n[k].abs() <= 1e-12 || !dist.is_finite() {
            return self.to_points();
        }
        let anchor = self.project(anchor);
        let make = |a: f64, b: f64| {
            let mut p = anchor;
            p[i] += a;
            p[j] += b;
            p[k] = (dist - n[i] * p[i] - n[j] * p[j]) / n[k];
            p
        };
        let points = [make(128.0, 128.0), make(128.0, -128.0), make(-256.0, 0.0)];
        if (points[0] - points[1]).cross(points[2] - points[1]).dot(self.normal) >= 0.0 {
            points
        } else {
            [points[1], points[0], points[2]]
        }
    }

    pub fn to_points(&self) -> [DVec3; 3] {
        const SPREAD: f64 = 128.0;
        let n = self.normal;
        let k = self.dominant_axis();
        let i = (k + 1) % 3;
        let j = (k + 2) % 3;
        let nk = n[k];
        if nk.abs() <= 1e-12 || nk.is_nan() || !self.dist.is_finite() {
            return [DVec3::ZERO, DVec3::X, DVec3::Y];
        }
        let origin = n * self.dist;
        let bi = origin[i].round();
        let bj = origin[j].round();
        let make = |ci: f64, cj: f64| {
            let ck = (self.dist - n[i] * ci - n[j] * cj) / nk;
            let mut v = DVec3::ZERO;
            v[i] = ci;
            v[j] = cj;
            v[k] = clean_coord(ck);
            v
        };
        let p0 = make(bi, bj);
        let p1 = make(bi + SPREAD, bj);
        let p2 = make(bi, bj + SPREAD);
        let cand = [p1, p0, p2];
        if (cand[0] - cand[1]).cross(cand[2] - cand[1]).dot(n) >= 0.0 { cand } else { [p2, p0, p1] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchored_plane_points_preserve_the_center_and_reverse_consistently() {
        let anchor = DVec3::new(12000.25, -11999.75, 320.125);
        for normal in [DVec3::new(0.001, 1.0, 0.0), DVec3::new(0.002, -0.001, 1.0)] {
            let normal = normal.normalize();
            let plane = Plane { normal, dist: normal.dot(anchor) };
            let points = plane.to_points_at(anchor);
            assert!((points.iter().sum::<DVec3>() / 3.0).distance(anchor) < 1e-9);
            let restored = Plane::from_points(points[0], points[1], points[2]).unwrap();
            assert!(restored.normal.distance(normal) < 1e-10);
            assert!(points.iter().all(|p| plane.distance(*p).abs() < 1e-9));
            let inverse = Plane { normal: -normal, dist: -plane.dist }.to_points_at(anchor);
            assert_eq!(points, [inverse[1], inverse[0], inverse[2]]);
        }
    }

    fn v(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x, y, z)
    }

    #[test]
    fn sample_cube_top_face_points_up() {
        let p =
            Plane::from_points(v(-480.0, 480.0, 16.0), v(-352.0, 480.0, 16.0), v(-352.0, 352.0, 16.0)).expect("plane");
        assert_eq!(p.normal, DVec3::Z);
        assert_eq!(p.dist, 16.0);
        let b = Plane::from_points(v(-480.0, 352.0, 8.0), v(-352.0, 352.0, 8.0), v(-352.0, 480.0, 8.0)).expect("plane");
        assert_eq!(b.normal, -DVec3::Z);
        assert_eq!(b.dist, -8.0);
    }

    #[test]
    fn degenerate_points() {
        assert!(Plane::from_points(DVec3::ZERO, DVec3::ZERO, DVec3::X).is_none());
        assert!(Plane::from_points(DVec3::ZERO, DVec3::X, DVec3::X * 2.0).is_none());
        assert!(Plane::from_points(v(f64::NAN, 0.0, 0.0), DVec3::X, DVec3::Y).is_none());
        assert!(Plane::from_points(v(0.0, 0.0, 0.0), v(10000.0, 1e-9, 0.0), v(20000.0, 0.0, 0.0)).is_none());
    }

    #[test]
    fn classify_and_flip() {
        let p = Plane::new(DVec3::Z, 10.0).expect("plane");
        assert_eq!(p.classify(v(0.0, 0.0, 11.0)), PlaneSide::Front);
        assert_eq!(p.classify(v(0.0, 0.0, 9.0)), PlaneSide::Back);
        assert_eq!(p.classify(v(5.0, 5.0, 10.005)), PlaneSide::On);
        let f = p.flip();
        assert_eq!(f.classify(v(0.0, 0.0, 11.0)), PlaneSide::Back);
        assert!((p.distance(v(1.0, 2.0, 3.0)) + 7.0).abs() < 1e-12);
    }

    #[test]
    fn to_points_round_trip() {
        let planes = [
            Plane::new(DVec3::Z, 16.0),
            Plane::new(-DVec3::X, 352.0),
            Plane::new(v(1.0, 1.0, 0.0), 64.0),
            Plane::new(v(0.3, -0.2, 0.9), -1234.5),
            Plane::new(v(-0.7, 0.1, -0.1), 16000.0),
        ];
        for p in planes.into_iter().flatten() {
            let pts = p.to_points();
            let q = Plane::from_points(pts[0], pts[1], pts[2]).expect("round trip");
            assert!(p.approx_eq(&q, 1e-9, 1e-7), "{p:?} vs {q:?}");
        }
        let axial = Plane::new(DVec3::NEG_Y, -40.0).expect("plane").to_points();
        for pt in axial {
            assert_eq!(pt, pt.round());
        }
    }

    #[test]
    fn three_planes() {
        let a = Plane::new(DVec3::X, 1.0).expect("plane");
        let b = Plane::new(DVec3::Y, 2.0).expect("plane");
        let c = Plane::new(DVec3::Z, 3.0).expect("plane");
        let p = Plane::intersect_three(&a, &b, &c).expect("point");
        assert!((p - v(1.0, 2.0, 3.0)).length() < 1e-12);
        assert!(Plane::intersect_three(&a, &a, &c).is_none());
    }
}
