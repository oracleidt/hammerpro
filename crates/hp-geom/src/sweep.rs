use glam::DVec3;

use crate::{Aabb, SolidGeom};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepHit {
    pub fraction: f64,
    pub normal: DVec3,
    pub start_solid: bool,
}

fn separated(
    start: DVec3,
    delta: DVec3,
    half: DVec3,
    axes: impl Iterator<Item = (DVec3, f64, f64)>,
) -> Option<SweepHit> {
    if !start.is_finite() || !delta.is_finite() || !half.is_finite() || half.min_element() < 0.0 {
        return None;
    }
    let mut enter = f64::NEG_INFINITY;
    let mut leave = f64::INFINITY;
    let mut normal = DVec3::ZERO;
    let mut inside = true;
    for (axis, mut lo, mut hi) in axes {
        let radius = axis.abs().dot(half);
        lo -= radius;
        hi += radius;
        let position = axis.dot(start);
        let velocity = axis.dot(delta);
        inside &= position > lo && position < hi;
        if velocity.abs() < 1e-14 {
            if position < lo || position > hi {
                return None;
            }
            continue;
        }
        let a = (lo - position) / velocity;
        let b = (hi - position) / velocity;
        let first = a.min(b);
        if first > enter {
            enter = first;
            normal = axis * -velocity.signum();
        }
        leave = leave.min(a.max(b));
        if enter > leave || leave < 0.0 || enter > 1.0 {
            return None;
        }
    }
    if inside {
        return Some(SweepHit { fraction: 0.0, normal, start_solid: true });
    }
    (enter >= 0.0 && enter <= leave && enter <= 1.0 && normal.length_squared() > 0.5).then_some(SweepHit {
        fraction: enter,
        normal,
        start_solid: false,
    })
}

fn project(axis: DVec3, points: impl Iterator<Item = DVec3>) -> (DVec3, f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for point in points {
        let p = axis.dot(point);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (axis, lo, hi)
}

#[derive(Clone, Debug)]
pub struct SweepHull {
    bounds: Aabb,
    axes: Vec<(DVec3, f64, f64)>,
}

impl SweepHull {
    pub fn new(solid: &SolidGeom) -> Self {
        let mut axes: Vec<DVec3> = Vec::new();
        let mut push = |axis: DVec3| {
            if let Some(axis) = axis.try_normalize()
                && !axes.iter().any(|old| old.dot(axis).abs() > 1.0 - 1e-12)
            {
                axes.push(axis);
            }
        };
        if solid.valid {
            for face in &solid.faces {
                push(face.plane.normal);
            }
            let axial = [DVec3::X, DVec3::Y, DVec3::Z];
            for axis in axial {
                push(axis);
            }
            for face in &solid.faces {
                for (i, point) in face.vertices.iter().enumerate() {
                    let edge = face.vertices[(i + 1) % face.vertices.len()] - *point;
                    for axis in axial {
                        push(edge.cross(axis));
                    }
                }
            }
        }
        Self { bounds: solid.aabb, axes: axes.into_iter().map(|a| project(a, solid.vertices())).collect() }
    }

    pub fn trace(&self, start: DVec3, end: DVec3, half: DVec3) -> Option<SweepHit> {
        if self.axes.is_empty() || !Aabb::new(start, end).expand(half.max_element()).intersects(&self.bounds) {
            return None;
        }
        separated(start, end - start, half, self.axes.iter().copied())
    }
}

pub fn sweep_box_triangle(start: DVec3, end: DVec3, half: DVec3, triangle: [DVec3; 3]) -> Option<SweepHit> {
    if triangle.iter().any(|p| !p.is_finite()) {
        return None;
    }
    let edges = [triangle[1] - triangle[0], triangle[2] - triangle[1], triangle[0] - triangle[2]];
    let normal = edges[0].cross(edges[1]).try_normalize()?;
    let axial = [DVec3::X, DVec3::Y, DVec3::Z];
    let axes = std::iter::once(normal)
        .chain(axial)
        .chain(edges.into_iter().flat_map(|edge| axial.map(|axis| edge.cross(axis))));
    separated(
        start,
        end - start,
        half,
        axes.filter_map(|a| a.try_normalize()).map(|a| project(a, triangle.into_iter())),
    )
}

pub fn sweep_box_solid(start: DVec3, end: DVec3, half: DVec3, solid: &SolidGeom) -> Option<SweepHit> {
    SweepHull::new(solid).trace(start, end, half)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> [DVec3; 3] {
        [DVec3::new(-100.0, -100.0, 0.0), DVec3::new(100.0, -100.0, 0.0), DVec3::new(0.0, 100.0, 0.0)]
    }

    #[test]
    fn continuous_hulls_hit_faces_and_edges_without_tunneling() {
        let hit = sweep_box_triangle(DVec3::Z * 1000.0, -DVec3::Z * 1000.0, DVec3::splat(2.0), floor()).unwrap();
        assert!((hit.fraction - 0.499).abs() < 1e-12);
        assert_eq!(hit.normal, DVec3::Z);
        assert!(!hit.start_solid);
        assert!(
            sweep_box_triangle(
                DVec3::new(102.01, -100.0, 10.0),
                DVec3::new(102.01, -100.0, -10.0),
                DVec3::splat(2.0),
                floor()
            )
            .is_none()
        );
        assert!(
            sweep_box_triangle(
                DVec3::new(101.0, -100.0, 10.0),
                DVec3::new(101.0, -100.0, -10.0),
                DVec3::splat(2.0),
                floor()
            )
            .is_some()
        );
    }

    #[test]
    fn swept_hulls_respect_slopes_and_reject_invalid_input() {
        let triangle = floor().map(|p| DVec3::new(p.x, p.y, p.x));
        let hit = sweep_box_triangle(DVec3::Z * 10.0, -DVec3::Z * 10.0, DVec3::splat(1.0), triangle).unwrap();
        assert!((hit.fraction - 0.4).abs() < 1e-12);
        assert!(hit.normal.distance(DVec3::new(-1.0, 0.0, 1.0).normalize()) < 1e-12);
        assert!(sweep_box_triangle(DVec3::NAN, DVec3::ZERO, DVec3::ONE, triangle).is_none());
        assert!(sweep_box_triangle(DVec3::Z, -DVec3::Z, -DVec3::ONE, triangle).is_none());
        assert!(sweep_box_triangle(DVec3::Z, -DVec3::Z, DVec3::ONE, [DVec3::ZERO; 3]).is_none());
    }
}
