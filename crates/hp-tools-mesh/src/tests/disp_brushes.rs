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
        ("sculpt", "relax"),
        ("relax_strength", "0.75"),
        ("flatten_plane", "fixed"),
        ("flatten_strength", "0.25"),
    ] {
        assert!(tool.set_option(&mut state, k, v), "{k} {v}");
    }
    assert!(!tool.set_option(&mut state, "flatten_plane", "tilted"));
    assert!(!tool.set_option(&mut state, "flatten_strength", "2"));
    assert!(!tool.set_option(&mut state, "relax_strength", "2"));
    for op in [SculptOp::Pinch, SculptOp::Grab, SculptOp::Inflate, SculptOp::Flatten, SculptOp::Relax] {
        tool.brush.sculpt.op = op;
        run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
        assert!(tool.status_hint(&state).is_some());
    }
    assert!(tool.set_option(&mut state, "mode", "geometry"));
    assert!(tool.set_option(&mut state, "effect", "relax"));
    tool.brush.sculpt.op = SculptOp::Push;
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    assert!(tool.status_hint(&state).is_some());
}

fn distorted_surface(state: &mut AppState, face: FaceRef, sloped: bool) {
    let source = mesh(state, face);
    let mut disp = (**state.doc.face_side(face).and_then(|s| s.disp.as_ref()).unwrap()).clone();
    let row = source.row_len;
    for (k, base) in source.base_positions.iter().enumerate() {
        let (x, y) = (k % row, k / row);
        let inside = x > 0 && x + 1 < row && y > 0 && y + 1 < row;
        let offset = if inside {
            DVec3::new(if x % 2 == 0 { 18.0 } else { -18.0 }, if y % 2 == 0 { 12.0 } else { -12.0 }, 0.0)
        } else {
            DVec3::ZERO
        };
        disp.offsets[k] = offset;
        if sloped {
            disp.distances[k] = 100.0 + (base.x + offset.x) * 0.2;
        }
        disp.alphas[k] = k as f64 * 2.0;
    }
    state.doc.set_disp_data("distorted surface", vec![(face, disp)]).unwrap();
}

fn edge_variance(surface: &hp_geom::DispMesh) -> f64 {
    let row = surface.row_len;
    let mut lengths = Vec::new();
    for k in 0..surface.positions.len() {
        if k % row + 1 < row {
            lengths.push(surface.positions[k].distance(surface.positions[k + 1]));
        }
        if k / row + 1 < row {
            lengths.push(surface.positions[k].distance(surface.positions[k + row]));
        }
    }
    let average = lengths.iter().sum::<f64>() / lengths.len() as f64;
    lengths.iter().map(|n| (n - average).powi(2)).sum::<f64>() / lengths.len() as f64
}

#[test]
fn relax_redistributes_the_surface_without_changing_borders_topology_or_alpha() {
    for (sloped, mode) in [
        (false, Interaction::Sculpt),
        (true, Interaction::Sculpt),
        (false, Interaction::Geometry),
        (true, Interaction::Geometry),
    ] {
        let (mut state, mut tool, face) = setup(SculptOp::Relax);
        tool.interaction = mode;
        tool.brush.geo_effect = GeoEffect::Relax;
        if mode == Interaction::Geometry {
            tool.brush.sculpt.op = SculptOp::Push;
        }
        tool.brush.radius = 800.0;
        tool.brush.sculpt.relax_strength = 1.0;
        distorted_surface(&mut state, face, sloped);
        let before = mesh(&state, face);
        let alphas = state.doc.face_side(face).unwrap().disp.as_ref().unwrap().alphas.clone();
        let original = vmf(&state);
        state.doc.clear_history();
        stroke(&mut state, &mut tool, &vec![DVec3::new(0.0, 0.0, if sloped { 100.0 } else { 0.0 }); 40], false);
        let after = mesh(&state, face);
        assert!(
            edge_variance(&after) < edge_variance(&before) * 0.2,
            "{sloped}: {} -> {}",
            edge_variance(&before),
            edge_variance(&after)
        );
        assert_eq!(after.indices, before.indices);
        assert_eq!(state.doc.face_side(face).unwrap().disp.as_ref().unwrap().alphas, alphas);
        for (k, p) in after.positions.iter().enumerate() {
            let row = after.row_len;
            if k % row == 0 || k % row + 1 == row || k / row == 0 || k / row + 1 == row {
                assert_eq!(*p, before.positions[k]);
            }
            assert!(p.is_finite());
            assert!(
                (p.z - if sloped { 100.0 + p.x * 0.2 } else { 0.0 }).abs() < 1e-7,
                "stays on the reference surface: {p}"
            );
        }
        for tri in after.indices.chunks_exact(3) {
            let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let old = (before.positions[b] - before.positions[a]).cross(before.positions[c] - before.positions[a]);
            let new = (after.positions[b] - after.positions[a]).cross(after.positions[c] - after.positions[a]);
            assert!(old.dot(new) > 0.0 && new.length_squared() > 1e-6);
        }
        assert_eq!(undo_steps(&state), 1);
        assert!(state.doc.undo());
        assert_eq!(vmf(&state), original);
        assert!(state.doc.redo());
        assert_eq!(mesh(&state, face).positions, after.positions);
    }
}

#[test]
fn relax_keeps_working_while_the_mouse_is_held_still() {
    for mode in [Interaction::Geometry, Interaction::Sculpt] {
        let (mut state, mut tool, face) = setup(SculptOp::Relax);
        tool.interaction = mode;
        tool.brush.geo_effect = GeoEffect::Relax;
        if mode == Interaction::Geometry {
            tool.brush.sculpt.op = SculptOp::Push;
        }
        distorted_surface(&mut state, face, false);
        tool.brush.radius = 800.0;
        tool.brush.continuous = false;
        state.frame.dt = 0.05;
        let view = view3d(DVec3::new(0.0, -300.0, 300.0), 90.0, -45.0);
        let position = view.world_to_screen(DVec3::ZERO).unwrap();
        let input = PointerFrame {
            pos: Some(position),
            pressed: true,
            down: true,
            mods: Modifiers::NONE,
            ..Default::default()
        };
        tool.handle_pointer(&mut state, &view, input);
        let first = mesh(&state, face);
        for _ in 0..20 {
            state.frame.index += 1;
            state.frame.time += 0.05;
            tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, ..input });
        }
        assert!(tool.is_painting());
        tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, down: false, ..input });
        assert!(edge_variance(&mesh(&state, face)) < edge_variance(&first) * 0.7);
    }
}

#[test]
fn relax_preserves_a_sharp_ridge_on_a_power_four_surface() {
    let (mut state, mut tool, face) = floor_with_disp(4);
    tool.interaction = Interaction::Sculpt;
    tool.brush.sculpt.op = SculptOp::Relax;
    tool.brush.sculpt.relax_strength = 1.0;
    tool.brush.radius = 1000.0;
    tool.brush.falloff = Falloff::Constant;
    let source = mesh(&state, face);
    let mut disp = (**state.doc.face_side(face).unwrap().disp.as_ref().unwrap()).clone();
    for (k, base) in source.base_positions.iter().enumerate() {
        let row = source.row_len;
        let (x, y) = (k % row, k / row);
        if x > 0 && x + 1 < row && y > 0 && y + 1 < row {
            disp.offsets[k] = DVec3::new(
                if base.x.abs() < 1e-8 {
                    0.0
                } else if x % 2 == 0 {
                    6.0
                } else {
                    -6.0
                },
                if y % 2 == 0 { 4.0 } else { -4.0 },
                0.0,
            );
        }
        disp.distances[k] = 10.0 + (base.x + disp.offsets[k].x).abs() * 1.2;
    }
    state.doc.set_disp_data("ridge", vec![(face, disp)]).unwrap();
    let before = mesh(&state, face);
    stroke(&mut state, &mut tool, &[DVec3::new(0.0, 0.0, 10.0); 40], false);
    let after = mesh(&state, face);
    assert!(before.positions.iter().zip(&after.positions).any(|(a, b)| a.distance(*b) > 1e-4));
    for (a, b) in before.positions.iter().zip(&after.positions) {
        assert!((b.z - 10.0 - b.x.abs() * 1.2).abs() < 1e-6, "reference surface {a} -> {b}");
        if a.x.abs() < 1e-8 {
            assert_eq!(*a, *b, "the ridge is pinned");
        }
    }
    assert_eq!(after.indices, before.indices);
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
