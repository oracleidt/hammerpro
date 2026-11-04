use hp_geom::{BlendFalloff, BlendMode, RampPivot};

use super::disp_live::{max_crack, quad_tiles};
use super::*;
use crate::disp::blend::{self, BlendOptions};
use crate::disp::slope::{self, SelectMatch, SlopeOp, SlopeOptions};

fn raise(state: &mut AppState, face: FaceRef, height: f64) {
    let mut d = state.doc.face_side(face).and_then(|s| s.disp.as_deref().cloned()).expect("disp");
    d.distances.iter_mut().for_each(|v| *v = height);
    state.doc.set_disp_data("Raise", vec![(face, d)]).expect("raise");
}

fn tiles() -> (AppState, Vec<FaceRef>) {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let faces = quad_tiles(&mut state, &mut tool, [3, 3, 3, 3]);
    (state, faces)
}

fn heights_on_x(state: &AppState, f: FaceRef, x: f64) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> =
        positions_where(state, f, |b| (b.x - x).abs() < 1e-6).into_iter().map(|(b, p)| (b.y, p.z)).collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

fn blend_opts(width: f64) -> BlendOptions {
    BlendOptions { width, falloff: BlendFalloff::Linear, mode: BlendMode::Average, ..BlendOptions::default() }
}

#[test]
fn blending_two_tiles_meets_in_the_middle_in_one_undo_step() {
    let (mut state, faces) = tiles();
    raise(&mut state, faces[1], 100.0);
    let (a, b) = (faces[0], faces[1]);
    state.doc.select_faces(&[a, b], SelectOp::Replace);
    let steps = undo_steps(&state);
    let sets = vec![vec![a], vec![b]];
    assert!(blend::apply(&mut state, &sets, &blend_opts(96.0)) > 0);
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(state.doc.undo_label(), Some("Blend displacements"));
    let left = heights_on_x(&state, a, 0.0);
    let right = heights_on_x(&state, b, 0.0);
    assert_eq!(left.len(), right.len());
    for ((_, za), (_, zb)) in left.iter().zip(&right) {
        assert!((za - zb).abs() < 1e-6 && (za - 50.0).abs() < 1e-6, "{za} {zb}");
    }
    let far = positions_where(&state, a, |p| p.x < -200.0);
    assert!(far.iter().all(|(_, p)| p.z.abs() < 1e-9), "outside the zone");
    assert!(max_crack(&state, &[a, b]) < 1e-6);
    state.doc.undo();
    assert!(heights_on_x(&state, a, 0.0).iter().all(|(_, z)| z.abs() < 1e-9));
    assert!(heights_on_x(&state, b, 0.0).iter().all(|(_, z)| (z - 100.0).abs() < 1e-9));
}

#[test]
fn blending_blends_the_alpha_across_the_zone() {
    let (mut state, faces) = tiles();
    let (a, b) = (faces[0], faces[1]);
    let mut d = state.doc.face_side(b).and_then(|s| s.disp.as_deref().cloned()).expect("disp");
    d.alphas.iter_mut().for_each(|v| *v = 255.0);
    state.doc.set_disp_data("Alpha", vec![(b, d)]).expect("alpha");
    let sets = vec![vec![a], vec![b]];
    assert!(blend::apply(&mut state, &sets, &blend_opts(64.0)) > 0);
    fn at(state: &AppState, f: FaceRef) -> f64 {
        let m = mesh(state, f);
        let d = state.doc.face_side(f).and_then(|s| s.disp.as_deref().cloned()).expect("disp");
        m.base_positions.iter().zip(&d.alphas).find(|(p, _)| p.x.abs() < 1e-6).map(|(_, a)| *a).expect("edge")
    }
    assert!((at(&state, a) - 127.5).abs() < 1e-6 && (at(&state, b) - 127.5).abs() < 1e-6);
    let mut off = blend_opts(64.0);
    off.alpha = false;
    state.doc.undo();
    blend::apply(&mut state, &sets, &off);
    assert!(at(&state, a).abs() < 1e-6 && (at(&state, b) - 255.0).abs() < 1e-6);
}

#[test]
fn partition_splits_by_solid_and_blending_needs_two_sets() {
    let (mut state, faces) = tiles();
    state.doc.select_faces(&faces, SelectOp::Replace);
    assert_eq!(blend::partition(&state.doc, &faces).len(), 4);
    assert_eq!(blend::partition(&state.doc, &faces[..1]).len(), 1);
    let steps = undo_steps(&state);
    assert_eq!(blend::apply(&mut state, &[faces[..1].to_vec()], &BlendOptions::default()), 0);
    assert_eq!(undo_steps(&state), steps);
}

#[test]
fn blending_different_powers_keeps_the_seam_closed() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let faces = quad_tiles(&mut state, &mut tool, [2, 4, 3, 3]);
    raise(&mut state, faces[1], 80.0);
    let sets = vec![vec![faces[0]], vec![faces[1]]];
    assert!(blend::apply(&mut state, &sets, &blend_opts(128.0)) > 0);
    assert!(max_crack(&state, &faces[..2]) < 1e-6, "{}", max_crack(&state, &faces[..2]));
    let mut opts = blend_opts(128.0);
    opts.match_powers = true;
    state.doc.undo();
    let steps = undo_steps(&state);
    assert!(blend::apply(&mut state, &sets, &opts) > 0);
    assert_eq!(undo_steps(&state), steps + 1);
    let powers: Vec<u32> = faces[..2]
        .iter()
        .filter_map(|f| state.doc.face_side(*f).and_then(|s| s.disp.as_ref()).map(|d| d.power))
        .collect();
    assert_eq!(powers, vec![4, 4]);
    assert!(max_crack(&state, &faces[..2]) < 1e-6);
    state.doc.undo();
    let back: Vec<u32> = faces[..2]
        .iter()
        .filter_map(|f| state.doc.face_side(*f).and_then(|s| s.disp.as_ref()).map(|d| d.power))
        .collect();
    assert_eq!(back, vec![2, 4]);
}

#[test]
fn the_zone_preview_lists_the_vertices_the_blend_moves() {
    let (mut state, faces) = tiles();
    raise(&mut state, faces[1], 100.0);
    let sets = vec![vec![faces[0]], vec![faces[1]]];
    let dots = blend::preview(&state.doc, &sets, &blend_opts(64.0));
    assert!(!dots.is_empty() && dots.iter().all(|(_, w)| *w > 0.0 && *w <= 1.0));
    assert!(dots.iter().any(|(_, w)| (*w - 1.0).abs() < 1e-9));
    assert!(dots.iter().all(|(p, _)| p.x.abs() <= 64.0 + 1e-6));
}

fn spiked() -> (AppState, FaceRef) {
    let (mut state, _, face) = floor_with_disp(3);
    state.doc.select_faces(&[face], SelectOp::Replace);
    let mut d = state.doc.face_side(face).and_then(|s| s.disp.as_deref().cloned()).expect("disp");
    d.distances[40] = 150.0;
    state.doc.set_disp_data("Spike", vec![(face, d)]).expect("spike");
    (state, face)
}

#[test]
fn limiting_the_slope_respects_the_angle_in_one_undo_step() {
    let (mut state, face) = spiked();
    let before = slope::measure(&state.doc, &[face]).expect("measure");
    assert!(before.stats.max > 60.0);
    let steps = undo_steps(&state);
    let opts = SlopeOptions { max_angle: 25.0, ..SlopeOptions::default() };
    assert!(slope::apply(&mut state, SlopeOp::Limit, &opts) > 0);
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(state.doc.undo_label(), Some("Limit displacement slope"));
    let after = slope::measure(&state.doc, &[face]).expect("measure");
    assert!(after.stats.max <= 25.0 + 0.01, "{}", after.stats.max);
    state.doc.undo();
    assert!((slope::measure(&state.doc, &[face]).expect("measure").stats.max - before.stats.max).abs() < 1e-9);
}

#[test]
fn a_ramp_is_an_exact_plane_over_sewn_tiles() {
    let (mut state, faces) = tiles();
    state.doc.select_faces(&faces, SelectOp::Replace);
    let opts = SlopeOptions {
        ramp_angle: 12.0,
        ramp_direction: 90.0,
        ramp_pivot: RampPivot::Lowest,
        ..SlopeOptions::default()
    };
    assert!(slope::apply(&mut state, SlopeOp::Ramp, &opts) > 0);
    assert_eq!(state.doc.undo_label(), Some("Displacement ramp"));
    let m = slope::measure(&state.doc, &faces).expect("measure");
    assert!((m.angle - 12.0).abs() < 1e-6 && (m.direction - 90.0).abs() < 1e-6, "{m:?}");
    assert!((m.stats.min - 12.0).abs() < 1e-6 && (m.stats.max - 12.0).abs() < 1e-6);
    assert!(max_crack(&state, &faces) < 1e-6);
    let lowest = faces.iter().flat_map(|f| mesh(&state, *f).positions).map(|p| p.z).fold(f64::INFINITY, f64::min);
    assert!(lowest.abs() < 1e-6, "{lowest}");
}

#[test]
fn a_uniform_slope_keeps_the_mean_angle_and_direction() {
    let (mut state, face) = spiked();
    let before = slope::measure(&state.doc, &[face]).expect("measure");
    assert!(slope::apply(&mut state, SlopeOp::Uniform, &SlopeOptions::default()) > 0);
    let after = slope::measure(&state.doc, &[face]).expect("measure");
    assert!((after.angle - before.angle).abs() < 1e-6);
    assert!((after.stats.max - after.stats.min).abs() < 1e-6);
}

#[test]
fn a_path_gets_a_constant_grade_and_needs_two_points() {
    let (mut state, faces) = tiles();
    state.doc.select_faces(&faces, SelectOp::Replace);
    let steps = undo_steps(&state);
    assert_eq!(slope::apply(&mut state, SlopeOp::Path, &SlopeOptions::default()), 0);
    assert_eq!(undo_steps(&state), steps);
    let opts = SlopeOptions {
        path: vec![DVec3::new(-200.0, -128.0, 0.0), DVec3::new(200.0, -128.0, 100.0)],
        path_width: 40.0,
        path_falloff: BlendFalloff::Linear,
        ..SlopeOptions::default()
    };
    assert!(slope::apply(&mut state, SlopeOp::Path, &opts) > 0);
    assert_eq!(state.doc.undo_label(), Some("Displacement slope along a path"));
    for f in &faces[..2] {
        for (b, p) in positions_where(&state, *f, |b| (b.y + 128.0).abs() < 1e-6 && b.x.abs() <= 200.0) {
            let expect = 100.0 * (b.x + 200.0) / 400.0;
            assert!((p.z - expect).abs() < 1e-6, "{b:?} {p:?} {expect}");
        }
    }
    assert!(max_crack(&state, &faces) < 1e-6);
}

#[test]
fn selecting_by_slope_picks_the_steep_displacements() {
    let (mut state, faces) = tiles();
    let mut d = state.doc.face_side(faces[2]).and_then(|s| s.disp.as_deref().cloned()).expect("disp");
    d.distances[40] = 200.0;
    state.doc.set_disp_data("Spike", vec![(faces[2], d)]).expect("spike");
    state.doc.clear_selection();
    let mut opts = SlopeOptions { select_angle: 45.0, ..SlopeOptions::default() };
    assert_eq!(slope::select_by_slope(&mut state, &opts), 1);
    assert_eq!(state.doc.selected_faces(), vec![faces[2]]);
    opts.select_above = false;
    opts.select_match = SelectMatch::All;
    state.doc.clear_selection();
    assert_eq!(slope::select_by_slope(&mut state, &opts), 3);
    opts.select_angle = 89.0;
    opts.select_above = true;
    state.doc.clear_selection();
    let steps = undo_steps(&state);
    assert_eq!(slope::select_by_slope(&mut state, &opts), 0);
    assert_eq!(undo_steps(&state), steps);
}

#[test]
fn slope_and_blend_options_round_trip() {
    let mut s = SlopeOptions::default();
    for (k, v) in [
        ("slope_max_angle", "30"),
        ("slope_terrace", "on"),
        ("slope_ramp_pivot", "highest"),
        ("slope_ramp_direction", "450"),
        ("slope_path", "0 0 0; 10 20 30"),
        ("slope_path_falloff", "ease_out"),
        ("slope_select_mode", "below"),
        ("slope_select_op", "add"),
    ] {
        assert!(s.set(k, v), "{k}");
    }
    assert_eq!(s.ramp_direction, 90.0);
    assert_eq!(s.path.len(), 2);
    assert_eq!(SlopeOptions::from_values(&s.values()), s);
    for (k, v) in
        [("slope_max_angle", "90"), ("slope_path", "1 2"), ("slope_ramp_pivot", "x"), ("slope_select_op", "toggle")]
    {
        assert!(!s.set(k, v), "{k} {v}");
    }
    assert_eq!(slope::parse_path(&slope::path_text(&s.path)), Some(s.path.clone()));
}

#[test]
fn the_displacement_tool_exposes_and_saves_the_new_options() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    assert!(tool.set_option(&mut state, "blend_width", "300"));
    assert!(tool.set_option(&mut state, "slope_max_angle", "33"));
    let values = tool.option_values();
    assert_eq!(BlendOptions::from_values(&values).width, 300.0);
    assert_eq!(SlopeOptions::from_values(&values).max_angle, 33.0);
    assert!(!tool.set_option(&mut state, "blend_mode", "nope"));
}

#[test]
fn limiting_across_a_cracked_seam_still_respects_the_angle() {
    let (mut state, faces) = tiles();
    raise(&mut state, faces[1], 100.0);
    let pair = [faces[0], faces[1]];
    state.doc.select_faces(&pair, SelectOp::Replace);
    assert!(max_crack(&state, &pair) > 50.0);
    let steps = undo_steps(&state);
    let opts = SlopeOptions { max_angle: 25.0, ..SlopeOptions::default() };
    assert!(slope::apply(&mut state, SlopeOp::Limit, &opts) > 0);
    assert_eq!(undo_steps(&state), steps + 1);
    assert!(max_crack(&state, &pair) < 1e-6);
    let worst = pair
        .iter()
        .flat_map(|f| {
            let m = mesh(&state, *f);
            hp_geom::triangle_inclines(&m.positions, &m.indices)
        })
        .fold(0.0f64, f64::max);
    assert!(worst <= 25.0 + 0.01, "{worst}");
    state.doc.undo();
    assert!(max_crack(&state, &pair) > 50.0);
}
