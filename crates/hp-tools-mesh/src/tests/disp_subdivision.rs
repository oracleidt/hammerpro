use super::*;
use crate::disp::{SubdivisionShape, commands};

fn full_cube(state: &mut AppState, center: DVec3, power: u32) -> (u32, Vec<FaceRef>) {
    let id = block(state, center - DVec3::splat(64.0), center + DVec3::splat(64.0));
    state.doc.clear_face_selection();
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    let mut tool = DisplacementTool::new();
    assert!(tool.set_option(state, "full_displacement", "on"));
    assert_eq!(tool.create(state, power), 6);
    let faces = (0..6).map(|side| FaceRef::new(id, side)).collect();
    (id, faces)
}

#[test]
fn subdivision_moves_three_valence_corners_of_a_complete_block() {
    let mut state = state_with(&[]);
    let (_, faces) = full_cube(&mut state, DVec3::ZERO, 3);
    state.doc.select_faces(&faces, SelectOp::Replace);
    assert!(commands::subdivide(&mut state) > 0);
    for face in &faces {
        let m = mesh(&state, *face);
        for (base, p) in m.base_positions.iter().zip(&m.positions) {
            if base.abs().min_element() == 64.0 {
                assert!(base.distance(*p) > 0.1, "cube corner remains fixed: {base}");
            }
            assert!(p.is_finite());
        }
        for other in &faces {
            assert_seam_closed(&state, *face, *other);
        }
    }
}

#[test]
fn full_displacement_sphere_keeps_seams_attributes_and_undoes_as_one_edit() {
    for power in 2..=4 {
        let mut state = state_with(&[]);
        let center = DVec3::new(512.0, -768.0, 160.0);
        let (id, faces) = full_cube(&mut state, center, power);
        state
            .doc
            .edit_solid(id, |solid| {
                for side in &mut solid.sides {
                    let d = side.disp.as_mut().unwrap();
                    d.flags = hp_geom::DISP_FLAG_NO_PHYSICS;
                    for (k, alpha) in d.alphas.iter_mut().enumerate() {
                        *alpha = (k % 256) as f64;
                    }
                }
            })
            .unwrap();
        let alphas: Vec<_> =
            faces.iter().map(|f| state.doc.face_side(*f).unwrap().disp.as_ref().unwrap().alphas.clone()).collect();
        state.doc.clear_face_selection();
        state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
        state.doc.clear_history();
        let before = vmf(&state);
        assert!(commands::subdivide_with_shape(&mut state, SubdivisionShape::Sphere, 1.0) > 0);
        assert_eq!(state.doc.history_len().0, 1);
        let after = vmf(&state);
        for (face, alpha) in faces.iter().zip(&alphas) {
            for p in mesh(&state, *face).positions {
                assert!((p.distance(center) - 64.0).abs() < 1e-7, "power {power}: {p}");
            }
            let d = state.doc.face_side(*face).unwrap().disp.as_ref().unwrap();
            assert_eq!(&d.alphas, alpha);
            assert_eq!(d.flags, hp_geom::DISP_FLAG_NO_PHYSICS);
            assert!(d.normals.iter().all(|n| n.is_finite()));
            assert_tags_current(&state, *face);
            for other in &faces {
                assert_seam_closed(&state, *face, *other);
            }
        }
        let reopened = hp_editor::Document::from_map(hp_vmf::parse(&after).unwrap());
        for face in &faces {
            let loaded = reopened.scene().solid(id).unwrap().disp_mesh(face.side).unwrap();
            assert!(loaded.positions.iter().all(|p| (p.distance(center) - 64.0).abs() < 1e-5));
        }
        assert!(state.doc.undo());
        assert_eq!(vmf(&state), before);
        assert!(state.doc.redo());
        assert_eq!(vmf(&state), after);
    }
}

#[test]
fn sphere_shapes_separate_blocks_independently_even_when_their_corners_touch() {
    let mut state = state_with(&[]);
    let (a, _) = full_cube(&mut state, DVec3::ZERO, 3);
    let (b, _) = full_cube(&mut state, DVec3::splat(128.0), 3);
    state.doc.clear_face_selection();
    state.doc.select(&[ObjRef::Solid(a), ObjRef::Solid(b)], SelectOp::Replace);
    assert!(commands::subdivide_with_shape(&mut state, SubdivisionShape::Sphere, 1.0) > 0);
    for (id, center) in [(a, DVec3::ZERO), (b, DVec3::splat(128.0))] {
        for side in 0..6 {
            assert!(
                mesh(&state, FaceRef::new(id, side)).positions.iter().all(|p| (p.distance(center) - 64.0).abs() < 1e-7)
            );
        }
    }
}

#[test]
fn sphere_strength_blends_shape_and_rejects_zero_and_nonfinite_amounts() {
    let mut state = state_with(&[]);
    let (_, faces) = full_cube(&mut state, DVec3::ZERO, 2);
    state.doc.clear_history();
    let before = vmf(&state);
    for amount in [0.0, f64::NAN, f64::INFINITY] {
        assert_eq!(commands::subdivide_with_shape(&mut state, SubdivisionShape::Sphere, amount), 0);
        assert_eq!(vmf(&state), before);
    }
    assert_eq!(state.doc.history_len().0, 0);
    let original = mesh(&state, faces[0]).positions;
    assert!(commands::subdivide_with_shape(&mut state, SubdivisionShape::Sphere, 0.5) > 0);
    for (p, q) in original.iter().zip(mesh(&state, faces[0]).positions) {
        let expected = p.lerp(p.normalize() * 64.0, 0.5);
        assert!(q.distance(expected) < 1e-7);
    }
}

#[test]
fn sphere_sews_mixed_power_faces_and_preserves_blend_rows() {
    let mut state = state_with(&[]);
    let (id, faces) = full_cube(&mut state, DVec3::ZERO, 2);
    state.doc.resample_displacements(&[faces[0]], 4).unwrap();
    state
        .doc
        .edit_solid(id, |solid| {
            for side in &mut solid.sides {
                let d = side.disp.as_mut().unwrap();
                let mut blend = hp_vmf::multiblend::MultiBlend::new(d.alphas.len());
                for weights in &mut blend.weights {
                    *weights = [0.1, 0.2, 0.3, 0.4];
                }
                assert!(blend.write_to(d));
            }
        })
        .unwrap();
    let rows: Vec<_> = faces
        .iter()
        .map(|f| hp_vmf::multiblend::MultiBlend::of(state.doc.face_side(*f).unwrap().disp.as_ref().unwrap()).unwrap())
        .collect();
    state.doc.select_faces(&faces, SelectOp::Replace);
    assert!(commands::subdivide_with_shape(&mut state, SubdivisionShape::Sphere, 1.0) > 0);
    for (face, original) in faces.iter().zip(rows) {
        let disp = state.doc.face_side(*face).unwrap().disp.as_ref().unwrap();
        assert_eq!(hp_vmf::multiblend::MultiBlend::of(disp).unwrap(), original);
        for other in &faces {
            assert_seam_closed(&state, *face, *other);
        }
        assert!(mesh(&state, *face).positions.iter().all(|p| p.is_finite() && p.length() <= 64.0 + 1e-7));
    }
}
