use glam::DVec3;
use hp_vmf::multiblend::MultiBlend;
use hp_vmf::{DispInfo, Side};

use super::*;
use crate::csg::clip_solid_traced;
use crate::test_support::{hammer_box, id_counter};
use crate::{build_disp, create_disp};

const SIZE: f64 = 512.0;

fn height(p: DVec3) -> f64 {
    (p.x * 0.013).sin() * 40.0 + (p.y * 0.021).cos() * 25.0 + p.x * 0.05
}

fn terrain() -> Vec<Side> {
    let mut sides = hammer_box(DVec3::new(0.0, 0.0, -64.0), DVec3::new(SIZE, SIZE, 0.0));
    let geom = build_solid(&sides);
    let face = geom.face_for_side(0).unwrap().vertices.clone();
    let mut disp = create_disp(&face, 3).unwrap();
    let mesh = build_disp(&face, &disp).unwrap();
    for (k, base) in mesh.base_positions.iter().enumerate() {
        disp.distances[k] = height(*base);
        disp.normals[k] = DVec3::Z;
        disp.alphas[k] = (base.x / SIZE * 255.0).clamp(0.0, 255.0);
        disp.offsets[k] = DVec3::new(0.0, 0.0, (base.y * 0.01).sin());
    }
    disp.flags = 2;
    disp.elevation = 3.0;
    let mut blend = MultiBlend::new(81);
    for (k, base) in mesh.base_positions.iter().enumerate() {
        let t = base.y / SIZE;
        blend.weights[k] = [1.0 - t, t, 0.0, 0.0];
    }
    blend.write_to(&mut disp);
    sides[0].disp = Some(Box::new(disp));
    sides
}

fn disp_faces(pieces: &[ClipPiece]) -> Vec<(Vec<DVec3>, DispInfo)> {
    let mut out = Vec::new();
    for p in pieces {
        let geom = build_solid(&p.sides);
        assert!(geom.valid);
        for f in &geom.faces {
            if let Some(d) = &p.sides[f.side_index].disp {
                out.push((f.vertices.clone(), (**d).clone()));
            }
        }
    }
    out
}

fn cut(sides: &[Side], plane: Plane) -> Vec<ClipPiece> {
    let mut ids = id_counter(100);
    let (front, back) = clip_solid_traced(sides, &plane, &mut ids).unwrap();
    let pieces: Vec<ClipPiece> = [back, front].into_iter().flatten().collect();
    keep_cut_displacements(sides, pieces, &mut ids)
}

fn original_face(sides: &[Side]) -> Vec<DVec3> {
    build_solid(sides).face_for_side(0).unwrap().vertices.clone()
}

fn assert_on_surface(sides: &[Side], faces: &[(Vec<DVec3>, DispInfo)]) {
    let face = original_face(sides);
    let disp = sides[0].disp.as_ref().unwrap();
    for (poly, d) in faces {
        let mesh = build_disp(poly, d).expect("the new face carries a valid displacement");
        for (p, base) in mesh.positions.iter().zip(&mesh.base_positions) {
            let want = displaced_surface_point(&face, disp, *base).unwrap();
            assert!(p.distance(want) < 1e-6, "{p} vs {want}");
        }
    }
}

#[test]
fn a_cut_along_a_grid_line_keeps_every_original_vertex() {
    let sides = terrain();
    let original = build_disp(&original_face(&sides), sides[0].disp.as_ref().unwrap()).unwrap();
    let pieces = cut(&sides, Plane::new(DVec3::X, 256.0).unwrap());
    let faces = disp_faces(&pieces);
    assert_eq!(faces.len(), 2);
    for (poly, d) in &faces {
        assert_eq!(poly.len(), 4);
        assert_eq!(d.power, 3);
        assert_eq!(d.flags, 2);
        assert_eq!(d.elevation, 3.0);
        assert!(hp_vmf::multiblend::has_multiblend(d));
    }
    assert_on_surface(&sides, &faces);
    let meshes: Vec<_> = faces.iter().map(|(p, d)| build_disp(p, d).unwrap()).collect();
    for p in &original.positions {
        assert!(
            meshes.iter().any(|m| m.positions.iter().any(|q| q.distance(*p) < 1e-6)),
            "original vertex {p} is kept"
        );
    }
    let left = &faces.iter().find(|(p, _)| p.iter().all(|v| v.x <= 256.0 + 1e-6)).unwrap().1;
    let weights = MultiBlend::of(left).unwrap();
    assert!(weights.weights.iter().all(|w| (w.iter().sum::<f64>() - 1.0).abs() < 1e-9));
}

#[test]
fn a_cut_across_grid_lines_resamples_at_high_power_without_cracks() {
    let sides = terrain();
    let pieces = cut(&sides, Plane::new(DVec3::X, 200.0).unwrap());
    let faces = disp_faces(&pieces);
    assert_eq!(faces.len(), 2);
    assert!(faces.iter().all(|(_, d)| d.power == 4));
    assert_on_surface(&sides, &faces);
    let seam = |m: &crate::DispMesh| -> Vec<DVec3> {
        m.positions.iter().zip(&m.base_positions).filter(|(_, b)| (b.x - 200.0).abs() < 1e-6).map(|(p, _)| *p).collect()
    };
    let a = seam(&build_disp(&faces[0].0, &faces[0].1).unwrap());
    let b = seam(&build_disp(&faces[1].0, &faces[1].1).unwrap());
    assert_eq!(a.len(), 17);
    assert_eq!(a.len(), b.len());
    for p in &a {
        assert!(b.iter().any(|q| q.distance(*p) < 1e-9), "seam vertex {p} is shared");
    }
}

#[test]
fn a_diagonal_cut_turns_every_piece_into_quads_that_meet() {
    let sides = terrain();
    let normal = DVec3::new(1.0, 1.0, 0.0).normalize();
    let plane = Plane::new(normal, normal.dot(DVec3::new(120.0, 0.0, 0.0))).unwrap();
    let pieces = cut(&sides, plane);
    assert!(pieces.len() >= 3, "the pentagon side is split into two quads");
    let faces = disp_faces(&pieces);
    assert_eq!(faces.len(), pieces.len());
    assert!(faces.iter().all(|(p, _)| p.len() == 4));
    assert_on_surface(&sides, &faces);
    let on_cut = |m: &crate::DispMesh| -> Vec<DVec3> {
        m.positions
            .iter()
            .zip(&m.base_positions)
            .filter(|(_, b)| plane.distance(**b).abs() < 1e-6)
            .map(|(p, _)| *p)
            .collect()
    };
    let meshes: Vec<_> = faces.iter().map(|(p, d)| build_disp(p, d).unwrap()).collect();
    let small = meshes.iter().find(|m| m.base_positions.iter().all(|b| plane.distance(*b) <= 1e-6)).unwrap();
    let small_seam = on_cut(small);
    assert!(small_seam.len() >= 17);
    for p in &small_seam {
        let shared =
            meshes.iter().filter(|m| !std::ptr::eq(*m, small)).any(|m| on_cut(m).iter().any(|q| q.distance(*p) < 1e-9));
        assert!(shared, "seam vertex {p} is shared by the other side");
    }
}

#[test]
fn pieces_without_displacements_are_untouched() {
    let sides = hammer_box(DVec3::ZERO, DVec3::splat(64.0));
    let mut ids = id_counter(100);
    let (front, back) = clip_solid_traced(&sides, &Plane::new(DVec3::X, 20.0).unwrap(), &mut ids).unwrap();
    let pieces: Vec<ClipPiece> = [back, front].into_iter().flatten().collect();
    let kept = keep_cut_displacements(&sides, pieces.clone(), &mut ids);
    assert_eq!(kept, pieces);
}

#[test]
fn power_follows_the_grid() {
    let sides = terrain();
    let quarter = cut(&sides, Plane::new(DVec3::X, 128.0).unwrap());
    assert!(disp_faces(&quarter).iter().all(|(_, d)| d.power == 4), "6 cells on one side is not a power of two");
    let face = original_face(&sides);
    let src = Source::new(&face, sides[0].disp.as_ref().unwrap()).unwrap();
    let rect = |x0: f64, x1: f64, y0: f64, y1: f64| {
        vec![DVec3::new(x0, y1, 0.0), DVec3::new(x1, y1, 0.0), DVec3::new(x1, y0, 0.0), DVec3::new(x0, y0, 0.0)]
    };
    assert_eq!(src.preferred_power(&rect(0.0, 64.0, 0.0, 512.0)), 3);
    assert_eq!(src.preferred_power(&rect(0.0, 128.0, 0.0, 128.0)), 2);
    assert_eq!(src.preferred_power(&rect(0.0, 100.0, 0.0, 512.0)), 4);
}
