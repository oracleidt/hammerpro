use glam::DVec3;
use hp_geom::{Aabb, GeomError, convex_hull};

/// Editable collision control points and the validated convex surface they generate.
/// Keep control-point order stable while dragging; hull reconstruction may reorder its own vertices.
#[derive(Clone, Debug, PartialEq)]
pub struct EditableCollision {
    pub points: Vec<DVec3>,
    triangles: Vec<[DVec3; 3]>,
}

impl EditableCollision {
    pub fn new(points: Vec<DVec3>) -> Result<Self, GeomError> {
        let triangles = Self::build(&points)?;
        Ok(Self { points, triangles })
    }

    pub fn from_triangles(triangles: &[[DVec3; 3]]) -> Result<Self, GeomError> {
        let mut points: Vec<DVec3> = Vec::new();
        for &point in triangles.iter().flatten() {
            if !points.iter().any(|old| old.distance_squared(point) < 1e-12) {
                points.push(point);
            }
        }
        Self::new(points)
    }

    pub fn bounds(&self) -> Aabb {
        Aabb::from_points(self.points.iter().copied())
    }

    pub fn box_at(bounds: Aabb) -> Result<Self, GeomError> {
        let points = (0..8)
            .map(|index| {
                DVec3::new(
                    if index & 1 == 0 { bounds.min.x } else { bounds.max.x },
                    if index & 2 == 0 { bounds.min.y } else { bounds.max.y },
                    if index & 4 == 0 { bounds.min.z } else { bounds.max.z },
                )
            })
            .collect();
        Self::new(points)
    }

    pub fn triangles(&self) -> &[[DVec3; 3]] {
        &self.triangles
    }

    pub fn set_point(&mut self, index: usize, point: DVec3) -> Result<(), GeomError> {
        let Some(old) = self.points.get(index) else {
            return Err(GeomError::InvalidInput("collision point index".into()));
        };
        if *old == point {
            return Ok(());
        }
        let mut points = self.points.clone();
        points[index] = point;
        let triangles = Self::build(&points)?;
        self.points = points;
        self.triangles = triangles;
        Ok(())
    }

    fn build(points: &[DVec3]) -> Result<Vec<[DVec3; 3]>, GeomError> {
        if points.iter().any(|p| !p.is_finite()) {
            return Err(GeomError::InvalidInput("non-finite collision point".into()));
        }
        let hull = convex_hull(points)?;
        let mut triangles = Vec::new();
        for face in &hull.faces {
            let polygon = hull.face_polygon(face);
            for index in 1..polygon.len().saturating_sub(1) {
                triangles.push([polygon[0], polygon[index], polygon[index + 1]]);
            }
        }
        Ok(triangles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_a_control_point_changes_the_exported_surface_without_reordering_handles() {
        let bounds = Aabb::new(DVec3::splat(-16.0), DVec3::splat(16.0));
        let mut piece = EditableCollision::box_at(bounds).unwrap();
        let original = piece.clone();
        piece.set_point(7, DVec3::new(32.0, 32.0, 48.0)).unwrap();
        assert_eq!(&piece.points[..7], &original.points[..7]);
        assert_eq!(piece.bounds().max, DVec3::new(32.0, 32.0, 48.0));
        assert!(piece.triangles().iter().flatten().any(|p| *p == piece.points[7]));
        assert_ne!(piece.triangles(), original.triangles());
        let valid = piece.clone();
        assert!(piece.set_point(7, DVec3::splat(f64::NAN)).is_err());
        assert_eq!(piece, valid, "a failed drag cannot damage the last valid collision");
        assert!(EditableCollision::new(vec![DVec3::ZERO; 8]).is_err());
    }
}
