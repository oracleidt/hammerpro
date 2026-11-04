use super::*;
use crate::disp::sculpt::SculptOp;

fn heights(state: &AppState, f: FaceRef) -> Vec<f64> {
    mesh(state, f).positions.iter().map(|p| p.z).collect()
}

fn setup(op: SculptOp) -> (AppState, DisplacementTool, FaceRef) {
    let (state, mut tool, face) = floor_with_disp(3);
    tool.interaction = Interaction::Sculpt;
    tool.brush.sculpt.op = op;
    tool.brush.falloff = Falloff::Constant;
    tool.brush.radius = 80.0;
    tool.brush.strength = 100.0;
    (state, tool, face)
}

fn stroke(state: &mut AppState, tool: &mut DisplacementTool, points: &[DVec3], invert: bool) {
    state.frame.dt = 0.1;
    tool.begin_stroke(state, 0);
    for p in points {
        tool.paint_at(state, *p, invert);
    }
    tool.end_stroke(state);
}

#[test]
fn pinch_pulls_toward_the_center_and_ctrl_spreads() {
    let (mut state, mut tool, face) = setup(SculptOp::Pinch);
    let before = mesh(&state, face).positions.clone();
    let steps = undo_steps(&state);
    stroke(&mut state, &mut tool, &[DVec3::ZERO, DVec3::ZERO], false);
    assert_eq!(undo_steps(&state), steps + 1, "one stroke, one step");
    let after = mesh(&state, face).positions.clone();
    let mut moved = 0;
    for (a, b) in before.iter().zip(&after) {
        assert!((a.z - b.z).abs() < 1e-9, "stays in the surface plane");
        if a.truncate().length() < 80.0 && a.truncate().length() > 1.0 {
            assert!(b.truncate().length() < a.truncate().length(), "{a} -> {b}");
            moved += 1;
        }
    }
    assert!(moved > 0);
    let (mut state, mut tool, face) = setup(SculptOp::Pinch);
    let before = mesh(&state, face).positions.clone();
    stroke(&mut state, &mut tool, &[DVec3::ZERO], true);
    let after = mesh(&state, face).positions.clone();
    assert!(before.iter().zip(&after).any(|(a, b)| b.truncate().length() > a.truncate().length() + 1.0));
}

#[test]
fn inflate_moves_along_the_surface_normal() {
    let (mut state, mut tool, face) = setup(SculptOp::Inflate);
    stroke(&mut state, &mut tool, &[DVec3::ZERO], false);
    let up = heights(&state, face).iter().copied().fold(f64::MIN, f64::max);
    assert!((up - 10.0).abs() < 1e-6, "strength 100 for 0.1 s: {up}");
    let (mut state, mut tool, face) = setup(SculptOp::Inflate);
    stroke(&mut state, &mut tool, &[DVec3::ZERO], true);
    let down = heights(&state, face).iter().copied().fold(f64::MAX, f64::min);
    assert!((down + 10.0).abs() < 1e-6, "{down}");
}

#[test]
fn grab_follows_the_mouse_for_the_whole_stroke() {
    let (mut state, mut tool, face) = setup(SculptOp::Grab);
    tool.brush.radius = 60.0;
    let held: Vec<usize> = mesh(&state, face)
        .base_positions
        .iter()
        .enumerate()
        .filter(|(_, b)| b.truncate().length() < 60.0)
        .map(|(k, _)| k)
        .collect();
    assert!(!held.is_empty());
    let steps = undo_steps(&state);
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.paint_at(&mut state, DVec3::new(0.0, 0.0, 30.0), false);
    tool.paint_at(&mut state, DVec3::new(250.0, 0.0, 30.0), false);
    tool.end_stroke(&mut state);
    assert_eq!(undo_steps(&state), steps + 1);
    let m = mesh(&state, face);
    for k in held {
        let b = m.base_positions[k];
        assert!((m.positions[k] - (b + DVec3::new(250.0, 0.0, 30.0))).length() < 1e-6, "{k} stays attached");
    }
}

fn bump(state: &mut AppState, face: FaceRef) {
    let mut disp = (**state.doc.face_side(face).and_then(|s| s.disp.as_ref()).unwrap()).clone();
    for k in (0..disp.distances.len()).step_by(2) {
        disp.distances[k] = 20.0;
    }
    state.doc.set_disp_data("bumps", vec![(face, disp)]).unwrap();
}

fn spread(state: &AppState, face: FaceRef) -> f64 {
    let h = heights(state, face);
    h.iter().copied().fold(f64::MIN, f64::max) - h.iter().copied().fold(f64::MAX, f64::min)
}

#[test]
fn flatten_levels_toward_the_plane_average_or_fixed() {
    for fixed in [false, true] {
        let (mut state, mut tool, face) = setup(SculptOp::Flatten);
        tool.brush.radius = 400.0;
        tool.brush.sculpt.flatten_strength = 1.0;
        tool.brush.sculpt.flatten_fixed = fixed;
        bump(&mut state, face);
        let before = spread(&state, face);
        assert!(before > 19.0);
        let steps = undo_steps(&state);
        stroke(&mut state, &mut tool, &[DVec3::ZERO, DVec3::ZERO, DVec3::ZERO], false);
        assert_eq!(undo_steps(&state), steps + 1);
        assert!(spread(&state, face) < before * 0.1, "fixed {fixed}: {before} -> {}", spread(&state, face));
    }
}

#[test]
fn flatten_ctrl_exaggerates() {
    let (mut state, mut tool, face) = setup(SculptOp::Flatten);
    tool.brush.radius = 400.0;
    bump(&mut state, face);
    let before = spread(&state, face);
    stroke(&mut state, &mut tool, &[DVec3::ZERO], true);
    assert!(spread(&state, face) > before);
}

#[test]
fn the_new_brushes_are_scriptable_and_the_panel_renders() {
    let (mut state, mut tool, _) = setup(SculptOp::Push);
    for (k, v) in [
        ("sculpt", "pinch"),
        ("sculpt", "grab"),
        ("sculpt", "inflate"),
        ("sculpt", "flatten"),
        ("flatten_plane", "fixed"),
        ("flatten_strength", "0.25"),
    ] {
        assert!(tool.set_option(&mut state, k, v), "{k} {v}");
    }
    assert!(!tool.set_option(&mut state, "flatten_plane", "tilted"));
    assert!(!tool.set_option(&mut state, "flatten_strength", "2"));
    for op in [SculptOp::Pinch, SculptOp::Grab, SculptOp::Inflate, SculptOp::Flatten] {
        tool.brush.sculpt.op = op;
        run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
        assert!(tool.status_hint(&state).is_some());
    }
}

#[test]
fn grab_drags_in_the_screen_plane_at_the_depth_of_the_grab_point() {
    let (mut state, mut tool, face) = setup(SculptOp::Grab);
    tool.brush.radius = 60.0;
    let view = view3d(DVec3::new(0.0, -300.0, 300.0), 90.0, -45.0);
    let c = view.rect.center();
    let at = |p: Pos2, pressed: bool, down: bool| PointerFrame {
        pos: Some(p),
        pressed,
        down,
        mods: Modifiers::NONE,
        ..PointerFrame::default()
    };
    let top_before = heights(&state, face).iter().copied().fold(f64::MIN, f64::max);
    let steps = undo_steps(&state);
    tool.handle_pointer(&mut state, &view, at(c, true, true));
    for k in 0..8u8 {
        state.frame.index += 1;
        state.frame.dt = 0.1;
        let p = Pos2::new(c.x, c.y - 8.0 * f32::from(k));
        tool.handle_pointer(&mut state, &view, at(p, false, true));
    }
    let end = Pos2::new(c.x, c.y - 56.0);
    tool.handle_pointer(&mut state, &view, at(end, false, false));
    assert_eq!(undo_steps(&state), steps + 1);
    let top_after = heights(&state, face).iter().copied().fold(f64::MIN, f64::max);
    assert!(
        top_after > top_before + 10.0,
        "the mouse moved up the screen, the terrain rises: {top_before} -> {top_after}"
    );
}
