use std::collections::HashMap;

use crate::Plane;
use glam::DVec3;

#[derive(Default)]
pub struct PlanePointCache {
    entries: HashMap<[i64; 4], Vec<(Plane, [DVec3; 3])>>,
}

impl PlanePointCache {
    pub fn points_at(&mut self, plane: Plane, anchor: DVec3) -> [DVec3; 3] {
        let reverse = plane.normal[plane.dominant_axis()] < 0.0;
        let canonical = if reverse { Plane { normal: -plane.normal, dist: -plane.dist } } else { plane };
        let key = [
            (canonical.normal.x * 1e7).round() as i64,
            (canonical.normal.y * 1e7).round() as i64,
            (canonical.normal.z * 1e7).round() as i64,
            (canonical.dist * 1e4).round() as i64,
        ];
        let find = |entries: &[(Plane, [DVec3; 3])]| {
            entries
                .iter()
                .find(|(old, _)| {
                    old.normal.distance(canonical.normal) < 1e-8 && (old.dist - canonical.dist).abs() < 1e-5
                })
                .map(|(_, points)| *points)
        };
        let exact = self.entries.get(&key).and_then(|entries| find(entries));
        let mut result = exact;
        if result.is_none() {
            'search: for a in -1..=1 {
                for b in -1..=1 {
                    for c in -1..=1 {
                        for d in -1..=1 {
                            if a == 0 && b == 0 && c == 0 && d == 0 {
                                continue;
                            }
                            let nearby = [key[0] + a, key[1] + b, key[2] + c, key[3] + d];
                            if let Some(points) = self.entries.get(&nearby).and_then(|entries| find(entries)) {
                                result = Some(points);
                                break 'search;
                            }
                        }
                    }
                }
            }
        }
        let points = result.unwrap_or_else(|| {
            let points = canonical.to_points_at(anchor);
            self.entries.entry(key).or_default().push((canonical, points));
            points
        });
        if reverse { [points[1], points[0], points[2]] } else { points }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_and_coplanar_faces_use_identical_float_points_with_opposite_winding() {
        let mut cache = PlanePointCache::default();
        let normal = DVec3::new(1.0, 0.005, 0.0).normalize();
        let plane = Plane { normal, dist: 12000.0 };
        let a = cache.points_at(plane, DVec3::splat(12000.0));
        let b = cache.points_at(Plane { normal: -normal, dist: -12000.0 }, DVec3::splat(12128.0));
        assert_eq!(a, [b[1], b[0], b[2]]);
        let c = cache.points_at(Plane { dist: 12000.0 + 1e-8, ..plane }, DVec3::splat(11900.0));
        assert_eq!(a, c);
        let other = cache.points_at(Plane { dist: 12001.0, ..plane }, DVec3::splat(12000.0));
        assert_ne!(a, other);
    }
}
