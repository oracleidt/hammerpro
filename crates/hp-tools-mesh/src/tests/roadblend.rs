use hp_geom::roadside::{AlphaMode, BlendShape, HeightMode, Spine};

use super::*;
use crate::disp::roadblend::{UNDO_BLEND, blend_terrain, preview};

fn road(z: f64, half: f64) -> Vec<Spine> {
    let st = vec![DVec2::new(-300.0, 0.0), DVec2::new(300.0, 0.0)];
    vec![Spine::new(st, vec![z; 2], vec![0.0; 2], half, 0.0).unwrap()]
}

fn heights(state: &AppState, f: FaceRef) -> Vec<DVec3> {
    let node = state.doc.scene().solid(f.solid).unwrap();
    node.disp_mesh(f.side).unwrap().positions.clone()
}

fn shape() -> BlendShape {
    BlendShape { shoulder: 96.0, sink: 0.0, ..Default::default() }
}

#[test]
fn terrain_follows_the_road_with_a_smooth_shoulder_in_one_undo_step() {
    let (mut state, _, face) = floor_with_disp(4);
    let steps = undo_steps(&state);
    let report = blend_terrain(&mut state.doc, &road(40.0, 32.0), &shape()).unwrap();
    assert_eq!(report.faces, 1);
    assert!(report.vertices_moved > 20);
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(state.doc.undo_label(), Some(UNDO_BLEND));
    let pts = heights(&state, face);
    for p in &pts {
        if p.y.abs() <= 32.0 {
            assert!((p.z - 40.0).abs() < 1e-6, "{p:?}");
        } else if p.y.abs() >= 128.0 {
            assert!(p.z.abs() < 1e-6, "{p:?}");
        } else {
            assert!(p.z > 0.0 && p.z < 40.0, "{p:?}");
        }
    }
    state.doc.undo();
    assert!(heights(&state, face).iter().all(|p| p.z.abs() < 1e-9));
}

#[test]
fn modes_strength_and_sink_change_the_result() {
    let (mut state, _, face) = floor_with_disp(4);
    let up = BlendShape { mode: HeightMode::SinkOnly, ..shape() };
    let report = blend_terrain(&mut state.doc, &road(40.0, 32.0), &up).unwrap();
    assert_eq!(report.vertices_moved, 0);
    let half = BlendShape { strength: 0.5, sink: 4.0, ..shape() };
    blend_terrain(&mut state.doc, &road(40.0, 32.0), &half).unwrap();
    let pts = heights(&state, face);
    let on_road = pts.iter().filter(|p| p.y.abs() < 1.0).map(|p| p.z).fold(f64::MIN, f64::max);
    assert!((on_road - 18.0).abs() < 1e-6, "{on_road}");
}

#[test]
fn texture_alpha_goes_toward_the_road_material_across_the_shoulder() {
    let (mut state, _, face) = floor_with_disp(4);
    let s = BlendShape { alpha: AlphaMode::TwoWay, alpha_shoulder: 64.0, ..shape() };
    blend_terrain(&mut state.doc, &road(10.0, 32.0), &s).unwrap();
    let pts = heights(&state, face);
    let a = alphas(&state, face);
    for (p, a) in pts.iter().zip(&a) {
        if p.y.abs() <= 32.0 {
            assert!((a - 255.0).abs() < 1e-6, "{p:?} {a}");
        } else if p.y.abs() >= 96.0 {
            assert!(a.abs() < 1e-6, "{p:?} {a}");
        }
    }
    assert!(a.iter().any(|v| *v > 1.0 && *v < 254.0));
    let wrong = BlendShape { alpha: AlphaMode::TwoWay, road_channel: 3, ..shape() };
    assert!(blend_terrain(&mut state.doc, &road(10.0, 32.0), &wrong).is_err());
}

#[test]
fn four_way_blend_writes_multiblend_rows_for_the_chosen_channel() {
    let (mut state, _, face) = floor_with_disp(3);
    let s = BlendShape { alpha: AlphaMode::FourWay, road_channel: 2, alpha_shoulder: 64.0, ..shape() };
    blend_terrain(&mut state.doc, &road(10.0, 32.0), &s).unwrap();
    let disp = state.doc.face_side(face).and_then(|s| s.disp.as_deref().cloned()).unwrap();
    let mb = hp_vmf::multiblend::MultiBlend::of(&disp).expect("multiblend rows");
    let pts = heights(&state, face);
    for (p, w) in pts.iter().zip(&mb.weights) {
        if p.y.abs() <= 32.0 {
            assert!((w[2] - 1.0).abs() < 1e-6, "{w:?}");
        }
    }
}

#[test]
fn nothing_near_the_road_is_a_clean_error_and_far_displacements_are_untouched() {
    let (mut state, _, face) = floor_with_disp(3);
    let far = vec![
        Spine::new(vec![DVec2::new(4000.0, 4000.0), DVec2::new(4400.0, 4000.0)], vec![0.0; 2], vec![0.0; 2], 32.0, 0.0)
            .unwrap(),
    ];
    let steps = undo_steps(&state);
    assert!(blend_terrain(&mut state.doc, &far, &shape()).is_err());
    assert_eq!(undo_steps(&state), steps);
    assert!(heights(&state, face).iter().all(|p| p.z.abs() < 1e-9));
    assert!(blend_terrain(&mut state.doc, &[], &shape()).is_err());
    let bad = BlendShape { strength: 3.0, ..shape() };
    assert!(blend_terrain(&mut state.doc, &road(5.0, 32.0), &bad).is_err());
}

#[test]
fn the_preview_covers_the_affected_area_and_changes_nothing() {
    let (state, _, face) = floor_with_disp(4);
    let before = state.doc.revision();
    let p = preview(&state.doc, &road(40.0, 32.0), &shape()).unwrap();
    assert!(p.faces == 1 && p.tris.len() > 50 && !p.lines.is_empty() && !p.truncated);
    assert!(p.tris.iter().flat_map(|t| t.0).all(|v| v[1].abs() <= 128.0 + 16.0));
    assert!(p.tris.iter().flat_map(|t| t.0).any(|v| (v[2] - 40.5).abs() < 1e-3));
    assert_eq!(state.doc.revision(), before);
    assert!(heights(&state, face).iter().all(|p| p.z.abs() < 1e-9));
    assert!(preview(&state.doc, &[], &shape()).unwrap().is_empty());
}
