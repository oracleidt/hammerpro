use glam::{DMat4, DVec3};

#[derive(Clone)]
pub struct Polyhedron {
    pub faces: Vec<Vec<DVec3>>,
}

impl Polyhedron {
    pub fn transformed(&self, matrix: DMat4) -> Self {
        Self {
            faces: self.faces.iter().map(|face| face.iter().map(|p| matrix.transform_point3(*p)).collect()).collect(),
        }
    }
    pub fn clipped(&self, normal: DVec3, distance: f64) -> Self {
        let mut faces = Vec::new();
        let mut cap = Vec::<DVec3>::new();
        for face in &self.faces {
            let Some(mut a) = face.last().copied() else { continue };
            let mut da = normal.dot(a) - distance;
            let mut result = Vec::new();
            for &b in face {
                let db = normal.dot(b) - distance;
                if (da <= 0.0) != (db <= 0.0) {
                    let p = a + (b - a) * (da / (da - db));
                    result.push(p);
                    if cap.iter().all(|q| q.distance_squared(p) > 1e-14) {
                        cap.push(p);
                    }
                }
                if db <= 0.0 {
                    result.push(b);
                }
                a = b;
                da = db;
            }
            if result.len() >= 3 {
                faces.push(result);
            }
        }
        if cap.len() >= 3 {
            let center = cap.iter().copied().sum::<DVec3>() / cap.len() as f64;
            let u = normal.cross(if normal.z.abs() < 0.9 { DVec3::Z } else { DVec3::X }).normalize();
            let v = normal.cross(u);
            cap.sort_by(|a, b| {
                let a = *a - center;
                let b = *b - center;
                a.dot(v).atan2(a.dot(u)).total_cmp(&b.dot(v).atan2(b.dot(u)))
            });
            faces.push(cap);
        }
        Self { faces }
    }
    pub fn volume_center(&self) -> Option<(f64, DVec3)> {
        let points: Vec<_> = self.faces.iter().flatten().copied().collect();
        if points.is_empty() {
            return None;
        }
        let reference = points.iter().copied().sum::<DVec3>() / points.len() as f64;
        let mut volume = 0.0;
        let mut moment = DVec3::ZERO;
        for face in &self.faces {
            let Some(&a) = face.first() else { continue };
            for pair in face[1..].windows(2) {
                let (b, c) = (pair[0], pair[1]);
                let signed = (a - reference).dot((b - reference).cross(c - reference)) / 6.0;
                volume += signed;
                moment += (reference + a + b + c) * (signed / 4.0);
            }
        }
        (volume.abs() > 1e-10 && volume.is_finite()).then(|| (volume.abs(), moment / volume))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cube() -> Polyhedron {
        let p: Vec<_> = (0..8)
            .map(|i| {
                DVec3::new(
                    if i & 1 == 0 { -1.0 } else { 1.0 },
                    if i & 2 == 0 { -1.0 } else { 1.0 },
                    if i & 4 == 0 { -1.0 } else { 1.0 },
                )
            })
            .collect();
        Polyhedron {
            faces: [[0, 4, 6, 2], [1, 3, 7, 5], [0, 1, 5, 4], [2, 6, 7, 3], [0, 2, 3, 1], [4, 5, 7, 6]]
                .map(|f| f.map(|i| p[i]).to_vec())
                .to_vec(),
        }
    }
    #[test]
    fn clipping_measures_actual_displaced_volume_and_center() {
        let cube = cube();
        let (volume, center) = cube.volume_center().unwrap();
        assert!((volume - 8.0).abs() < 1e-10);
        assert!(center.length() < 1e-10);
        let half = cube.clipped(DVec3::Z, 0.0);
        let (volume, center) = half.volume_center().unwrap();
        assert!((volume - 4.0).abs() < 1e-10, "{volume}");
        assert!(center.distance(DVec3::new(0.0, 0.0, -0.5)) < 1e-10, "{center}");
        let quarter = half.clipped(DVec3::X, 0.0);
        let (volume, center) = quarter.volume_center().unwrap();
        assert!((volume - 2.0).abs() < 1e-10);
        assert!(center.distance(DVec3::new(-0.5, 0.0, -0.5)) < 1e-10);
        assert!(cube.clipped(DVec3::Z, -2.0).volume_center().is_none());
    }
    #[test]
    fn rotated_hulls_clip_by_actual_faces_in_large_map_coordinates() {
        let matrix = DMat4::from_rotation_translation(
            glam::DQuat::from_rotation_x(0.7),
            DVec3::new(100000.0, -200000.0, 300000.0),
        );
        let cube = cube().transformed(matrix);
        let (volume, center) = cube.clipped(DVec3::Z, 300000.0).volume_center().unwrap();
        assert!((volume - 4.0).abs() < 1e-7, "{volume}");
        assert!((center.x - 100000.0).abs() < 1e-7);
        assert!(center.z < 300000.0);
    }
}
