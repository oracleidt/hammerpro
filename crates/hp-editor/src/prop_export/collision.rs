use glam::DVec3;
use hp_geom::{Aabb, GeomError, convex_hull};

#[derive(Clone, Debug, PartialEq)]
pub struct EditableCollision {
    pub points: Vec<DVec3>,
    triangles: Vec<[DVec3; 3]>,
    faces: Vec<Vec<usize>>,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CollisionEditError {
    #[error("the move would fold the piece: a vertex would end up inside it or on another one")]
    Folded,
    #[error(transparent)]
    Geometry(#[from] GeomError),
}

type Built = (Vec<[DVec3; 3]>, Vec<Vec<usize>>, Vec<bool>);

impl EditableCollision {
    pub fn new(points: Vec<DVec3>) -> Result<Self, GeomError> {
        let (_, _, used) = Self::build(&points)?;
        let kept: Vec<DVec3> = points.iter().zip(&used).filter(|(_, u)| **u).map(|(p, _)| *p).collect();
        let (triangles, faces, _) = Self::build(&kept)?;
        Ok(Self { points: kept, triangles, faces })
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

    pub fn faces(&self) -> &[Vec<usize>] {
        &self.faces
    }

    pub fn edges(&self) -> Vec<[usize; 2]> {
        let mut edges: Vec<[usize; 2]> = Vec::new();
        for face in &self.faces {
            for (i, &a) in face.iter().enumerate() {
                let b = face[(i + 1) % face.len()];
                let edge = [a.min(b), a.max(b)];
                if a != b && !edges.contains(&edge) {
                    edges.push(edge);
                }
            }
        }
        edges
    }

    pub fn center_of(&self, indices: &[usize]) -> Option<DVec3> {
        let points: Vec<DVec3> = indices.iter().filter_map(|&i| self.points.get(i).copied()).collect();
        (!points.is_empty() && points.len() == indices.len())
            .then(|| points.iter().copied().sum::<DVec3>() / points.len() as f64)
    }

    pub fn set_point(&mut self, index: usize, point: DVec3) -> Result<(), CollisionEditError> {
        let Some(old) = self.points.get(index) else {
            return Err(GeomError::InvalidInput("collision point index".into()).into());
        };
        let delta = point - *old;
        self.move_points(&[index], delta)
    }

    pub fn move_points(&mut self, indices: &[usize], delta: DVec3) -> Result<(), CollisionEditError> {
        if indices.iter().any(|&i| i >= self.points.len()) {
            return Err(GeomError::InvalidInput("collision point index".into()).into());
        }
        if delta == DVec3::ZERO {
            return Ok(());
        }
        let mut points = self.points.clone();
        for &i in indices {
            points[i] += delta;
        }
        let (triangles, faces, used) = Self::build(&points)?;
        if used.iter().any(|u| !u) {
            return Err(CollisionEditError::Folded);
        }
        self.points = points;
        self.triangles = triangles;
        self.faces = faces;
        Ok(())
    }

    fn build(points: &[DVec3]) -> Result<Built, GeomError> {
        if points.iter().any(|p| !p.is_finite()) {
            return Err(GeomError::InvalidInput("non-finite collision point".into()));
        }
        let hull = convex_hull(points)?;
        let index_of = |p: DVec3| {
            points
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.distance_squared(p).total_cmp(&b.1.distance_squared(p)))
                .map(|(i, _)| i)
        };
        let remap: Vec<Option<usize>> = hull.points.iter().map(|p| index_of(*p)).collect();
        let mut used = vec![false; points.len()];
        let mut triangles = Vec::new();
        let mut faces = Vec::new();
        for face in &hull.faces {
            let polygon = hull.face_polygon(face);
            for index in 1..polygon.len().saturating_sub(1) {
                triangles.push([polygon[0], polygon[index], polygon[index + 1]]);
            }
            let ids: Vec<usize> = face.indices.iter().filter_map(|&i| remap.get(i).copied().flatten()).collect();
            for &i in &ids {
                used[i] = true;
            }
            faces.push(ids);
        }
        Ok((triangles, faces, used))
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

    #[test]
    fn a_box_has_twelve_edges_and_six_faces_and_moving_an_edge_moves_both_vertices() {
        let mut piece = EditableCollision::box_at(Aabb::new(DVec3::ZERO, DVec3::splat(32.0))).unwrap();
        assert_eq!(piece.faces().len(), 6);
        assert!(piece.faces().iter().all(|f| f.len() == 4));
        let edges = piece.edges();
        assert_eq!(edges.len(), 12);
        let top = *edges
            .iter()
            .find(|e| piece.points[e[0]].z == 32.0 && piece.points[e[1]].z == 32.0 && piece.points[e[0]].x == 32.0)
            .unwrap();
        let before = piece.clone();
        piece.move_points(&top, DVec3::new(16.0, 0.0, 16.0)).unwrap();
        for i in 0..8 {
            let expected =
                if top.contains(&i) { before.points[i] + DVec3::new(16.0, 0.0, 16.0) } else { before.points[i] };
            assert_eq!(piece.points[i], expected);
        }
        assert_eq!(piece.edges().len(), 12, "the box stays a hexahedron with a slanted top");
        let roof = piece.faces().iter().find(|f| f.iter().all(|&i| piece.points[i].z >= 32.0)).unwrap().clone();
        let lifted = piece.clone();
        piece.move_points(&roof, DVec3::Z * 8.0).unwrap();
        assert!(roof.iter().all(|&i| piece.points[i].z == lifted.points[i].z + 8.0));
    }

    #[test]
    fn a_move_that_would_fold_the_piece_is_refused_and_keeps_the_last_valid_shape() {
        let mut piece = EditableCollision::box_at(Aabb::new(DVec3::ZERO, DVec3::splat(32.0))).unwrap();
        let valid = piece.clone();
        let corner = piece.points.iter().position(|p| *p == DVec3::splat(32.0)).unwrap();
        assert_eq!(piece.set_point(corner, DVec3::splat(16.0)), Err(CollisionEditError::Folded));
        assert_eq!(piece, valid);
        let other = piece.points.iter().position(|p| *p == DVec3::new(0.0, 32.0, 32.0)).unwrap();
        assert_eq!(piece.set_point(corner, piece.points[other]), Err(CollisionEditError::Folded));
        assert!(piece.move_points(&[99], DVec3::X).is_err());
        let with_inner = EditableCollision::new(vec![
            DVec3::ZERO,
            DVec3::X * 10.0,
            DVec3::Y * 10.0,
            DVec3::Z * 10.0,
            DVec3::splat(1.0),
        ])
        .unwrap();
        assert_eq!(with_inner.points.len(), 4, "points inside the hull are not handles");
    }
}
