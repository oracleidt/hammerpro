use std::collections::BTreeMap;

use glam::DVec3;
use hp_render::{MaterialId, MeshBatch, ObjectMesh, Vertex};

use super::build::Road;

pub(super) fn mesh(road: &Road, mut material: impl FnMut(&str) -> MaterialId) -> ObjectMesh {
    let mut batches = BTreeMap::<MaterialId, MeshBatch>::new();
    let mut edges = Vec::new();
    for piece in &road.pieces {
        let corners = &piece.corners;
        let faces: Vec<Vec<usize>> = match corners.len() {
            8 => [[4, 6, 7, 5], [0, 1, 3, 2], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]]
                .into_iter()
                .map(Vec::from)
                .collect(),
            6 => vec![vec![3, 4, 5], vec![0, 2, 1], vec![0, 1, 4, 3], vec![1, 2, 5, 4], vec![2, 0, 3, 5]],
            _ => continue,
        };
        let center = corners.iter().sum::<DVec3>() / corners.len() as f64;
        let id = material(&piece.material);
        let batch = batches.entry(id).or_insert_with(|| MeshBatch { material: id, ..Default::default() });
        for face in &faces {
            for i in 1..face.len() - 1 {
                let mut points = [corners[face[0]], corners[face[i]], corners[face[i + 1]]];
                let mut normal = (points[1] - points[0]).cross(points[2] - points[0]).normalize_or_zero();
                if normal == DVec3::ZERO {
                    continue;
                }
                if normal.dot(points.iter().sum::<DVec3>() / 3.0 - center) < 0.0 {
                    points.swap(1, 2);
                    normal = -normal;
                }
                let base = batch.vertices.len() as u32;
                for p in points {
                    let uv = [
                        (p.dot(piece.uv.u_axis) / road.meta.tex_scale + piece.uv.u_shift) as f32,
                        (p.dot(piece.uv.v_axis) / road.meta.tex_scale + piece.uv.v_shift) as f32,
                    ];
                    batch.vertices.push(Vertex {
                        pos: p.as_vec3().to_array(),
                        normal: normal.as_vec3().to_array(),
                        uv,
                        alpha: 0.0,
                    });
                }
                batch.indices.extend([base, base + 1, base + 2]);
            }
        }
        let top = &faces[0];
        for i in 0..top.len() {
            edges
                .extend([corners[top[i]].as_vec3().to_array(), corners[top[(i + 1) % top.len()]].as_vec3().to_array()]);
        }
    }
    let bounds = road.bounds().unwrap_or_default();
    ObjectMesh {
        batches: batches.into_values().collect(),
        edges,
        aabb_min: bounds.min.as_vec3().to_array(),
        aabb_max: bounds.max.as_vec3().to_array(),
        wire_color: [220; 3],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::road::{RoadEnv, RoadMeta, RoadPoint, build};

    #[test]
    fn textured_preview_covers_the_road_with_finite_vertices_and_upward_top_faces() {
        let meta = RoadMeta {
            points: vec![RoadPoint::new(0.0, 0.0, 0.0), RoadPoint::new(800.0, 0.0, 256.0)],
            ..Default::default()
        };
        let road = build(&meta, &RoadEnv::default()).unwrap();
        let visual = mesh(&road, |_| 7);
        assert_eq!(visual.batches.len(), 1);
        let batch = &visual.batches[0];
        assert_eq!(batch.vertices.len(), road.pieces.len() * 36);
        assert_eq!(batch.indices.len(), batch.vertices.len());
        for piece in batch.vertices.chunks_exact(36) {
            assert!(piece[..6].iter().all(|v| v.normal[2] > 0.0));
        }
        assert!(
            batch.vertices.iter().all(|v| v
                .pos
                .iter()
                .chain(v.uv.iter())
                .chain(v.normal.iter())
                .all(|v| v.is_finite()))
        );
        assert_eq!(visual.aabb_max[2], 256.0);
    }
}
