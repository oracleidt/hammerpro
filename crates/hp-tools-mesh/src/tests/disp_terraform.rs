use super::*;
use crate::disp::terraform::{ErodeKind, StampBlend, StampShape, TerraOp, mask};

fn heights(state: &AppState, f: FaceRef) -> Vec<f64> {
    mesh(state, f).positions.iter().map(|p| p.z).collect()
}

fn setup(op: TerraOp) -> (AppState, DisplacementTool, FaceRef) {
    let (mut state, mut tool, face) = floor_with_disp(4);
    state.doc.clear_face_selection();
    tool.interaction = Interaction::Terraform;
    tool.terraform.op = op;
    tool.brush.falloff = Falloff::Constant;
    tool.brush.radius = 120.0;
    tool.brush.axis = PaintAxis::Z;
    (state, tool, face)
}

fn stroke(state: &mut AppState, tool: &mut DisplacementTool, points: &[DVec3], invert: bool) -> usize {
    state.frame.dt = 0.05;
    tool.begin_stroke(state, 0);
    let mut n = 0;
    for p in points {
        n += tool.paint_at_mods(state, *p, Modifiers { shift: invert, ..Modifiers::default() });
    }
    tool.end_stroke(state);
    n
}

fn top(state: &AppState, f: FaceRef) -> f64 {
    mesh(state, f).base_positions.first().map_or(0.0, |p| p.z)
}

fn shape(state: &mut AppState, tool: &mut DisplacementTool, face: FaceRef, s: StampShape, height: f64) {
    let op = tool.terraform.op;
    tool.terraform.op = TerraOp::Stamp;
    tool.terraform.stamp_shape = s;
    tool.terraform.stamp_height = height;
    tool.terraform.stamp_blend = StampBlend::Add;
    tool.terraform.stamp_rotation = 0.0;
    let c = center(state, face);
    stroke(state, tool, &[c], false);
    tool.terraform.op = op;
}

fn at(state: &AppState, f: FaceRef, x: f64, y: f64) -> f64 {
    let m = mesh(state, f);
    let mut best = (f64::INFINITY, 0.0);
    for (b, p) in m.base_positions.iter().zip(&m.positions) {
        let d = (b.x - x).hypot(b.y - y);
        if d < best.0 {
            best = (d, p.z);
        }
    }
    best.1
}

fn center(state: &AppState, f: FaceRef) -> DVec3 {
    DVec3::new(0.0, 0.0, top(state, f))
}

#[test]
fn stamps_raise_their_shape_once_per_click_and_rotate() {
    let (mut state, mut tool, face) = setup(TerraOp::Stamp);
    let base = top(&state, face);
    tool.terraform.stamp_shape = StampShape::Mountain;
    tool.terraform.stamp_height = 200.0;
    tool.terraform.stamp_blend = StampBlend::Max;
    let steps = undo_steps(&state);
    let c = center(&state, face);
    stroke(&mut state, &mut tool, &[c, c, c], false);
    assert_eq!(undo_steps(&state), steps + 1, "one click, one undo step");
    let peak = at(&state, face, 0.0, 0.0) - base;
    assert!(peak > 100.0 && peak <= 200.0 + 1e-6, "peak {peak}");
    assert!(at(&state, face, 60.0, 0.0) - base < peak, "the flanks are lower");
    stroke(&mut state, &mut tool, &[c], false);
    assert!((at(&state, face, 0.0, 0.0) - base - peak).abs() < peak * 0.05, "max does not pile up");

    let (mut state, mut tool, face) = setup(TerraOp::Stamp);
    tool.terraform.stamp_shape = StampShape::Ridge;
    tool.terraform.stamp_height = 100.0;
    tool.terraform.stamp_blend = StampBlend::Add;
    let c = center(&state, face);
    stroke(&mut state, &mut tool, &[c], false);
    let along_x = at(&state, face, 70.0, 0.0) - base;
    let along_y = at(&state, face, 0.0, 50.0) - base;
    assert!(along_y > 20.0 && along_x.abs() < 1e-6, "{along_x} {along_y}");
    state.doc.undo();
    tool.terraform.stamp_rotation = 90.0;
    stroke(&mut state, &mut tool, &[c], false);
    let along_x = at(&state, face, 50.0, 0.0) - base;
    let along_y = at(&state, face, 0.0, 70.0) - base;
    assert!(along_x > 20.0 && along_y.abs() < 1e-6, "rotated: {along_x} {along_y}");
    state.doc.undo();
    stroke(&mut state, &mut tool, &[c], true);
    assert!(at(&state, face, 0.0, 0.0) - base < -20.0, "Shift digs");
}

#[test]
fn terrace_steps_the_heights() {
    let (mut state, mut tool, face) = setup(TerraOp::Terrace);
    shape(&mut state, &mut tool, face, StampShape::Hill, 200.0);
    tool.terraform.terrace_step = 50.0;
    tool.terraform.terrace_sharpness = 1.0;
    tool.terraform.terrace_offset = top(&state, face);
    tool.terraform.strength = 1.0;
    let c = center(&state, face);
    let before = heights(&state, face);
    stroke(&mut state, &mut tool, &vec![c; 40], false);
    let after = heights(&state, face);
    let base = top(&state, face);
    let m = mesh(&state, face);
    let mut checked = 0;
    for ((b, h0), h1) in m.base_positions.iter().zip(&before).zip(&after) {
        if b.truncate().length() < 100.0 && (h0 - base) > 5.0 {
            let level = (h1 - base) / 50.0;
            assert!((level - level.round()).abs() < 0.08, "{h0} -> {h1}");
            checked += 1;
        }
    }
    assert!(checked > 20);
}

#[test]
fn noise_paints_world_space_detail_within_its_amplitude() {
    let (mut state, mut tool, face) = setup(TerraOp::Noise);
    tool.terraform.noise_amplitude = 40.0;
    tool.terraform.noise_scale = 64.0;
    tool.terraform.strength = 1.0;
    let base = top(&state, face);
    let c = center(&state, face);
    stroke(&mut state, &mut tool, &vec![c; 60], false);
    let first = heights(&state, face);
    let spread = first.iter().map(|h| (h - base).abs()).fold(0.0, f64::max);
    assert!(spread > 5.0 && spread <= 40.0 + 1e-6, "{spread}");
    state.doc.undo();
    stroke(&mut state, &mut tool, &vec![c; 60], false);
    assert_eq!(first, heights(&state, face), "same seed, same noise");
}

#[test]
fn cliff_sharpens_and_plateau_flattens_tops() {
    let (mut state, mut tool, face) = setup(TerraOp::Cliff);
    shape(&mut state, &mut tool, face, StampShape::Hill, 150.0);
    let base = top(&state, face);
    let roughness = |state: &AppState| {
        let m = mesh(state, face);
        let row = m.row_len;
        (0..m.positions.len())
            .filter(|k| k % row + 1 < row)
            .map(|k| (m.positions[k + 1].z - m.positions[k].z).powi(2))
            .sum::<f64>()
    };
    let before = roughness(&state);
    let peak = at(&state, face, 0.0, 0.0) - base;
    tool.terraform.cliff_strength = 1.0;
    let c = center(&state, face);
    stroke(&mut state, &mut tool, &vec![c; 10], false);
    assert!(roughness(&state) > before * 1.05, "the slopes get steeper");
    assert!(at(&state, face, 0.0, 0.0) - base <= peak + 1e-6, "never above the summit");

    let (mut state, mut tool, face) = setup(TerraOp::Plateau);
    shape(&mut state, &mut tool, face, StampShape::Hill, 150.0);
    tool.terraform.strength = 1.0;
    let base = top(&state, face);
    let level_point = DVec3::new(60.0, 0.0, at(&state, face, 60.0, 0.0));
    let level = level_point.z;
    let low = at(&state, face, 100.0, 0.0);
    stroke(&mut state, &mut tool, &[level_point; 12], false);
    let summit = at(&state, face, 0.0, 0.0);
    assert!(summit < level + 0.15 * (150.0 + base - level), "summit {summit} level {level}");
    assert!((at(&state, face, 100.0, 0.0) - low).abs() < 1e-6, "below the level stays");
}

#[test]
fn erosion_brushes_reshape_the_terrain() {
    let (mut state, mut tool, face) = setup(TerraOp::Erode);
    shape(&mut state, &mut tool, face, StampShape::Mountain, 260.0);
    let before = heights(&state, face);
    tool.terraform.erode_kind = ErodeKind::Hydraulic;
    tool.terraform.erode_rate = 4000.0;
    tool.brush.radius = 200.0;
    let c = center(&state, face);
    let n = stroke(&mut state, &mut tool, &vec![c; 20], false);
    assert!(n > 0);
    let after = heights(&state, face);
    let lowered = before.iter().zip(&after).filter(|(a, b)| **b < **a - 0.01).count();
    let raised = before.iter().zip(&after).filter(|(a, b)| **b > **a + 0.01).count();
    assert!(lowered > 10 && raised > 3, "digs and deposits: {lowered} {raised}");

    let (mut state, mut tool, face) = setup(TerraOp::Erode);
    shape(&mut state, &mut tool, face, StampShape::Mountain, 400.0);
    let peak = at(&state, face, 0.0, 0.0);
    tool.terraform.erode_kind = ErodeKind::Thermal;
    tool.terraform.erode_talus = 20.0;
    tool.terraform.erode_rate = 10000.0;
    tool.terraform.erode_strength = 1.0;
    let c = center(&state, face);
    stroke(&mut state, &mut tool, &vec![c; 20], false);
    assert!(at(&state, face, 0.0, 0.0) < peak - 5.0, "the steep summit crumbles");
}

#[test]
fn clone_copies_the_source_region() {
    let (mut state, mut tool, face) = setup(TerraOp::Clone);
    let base = top(&state, face);
    tool.terraform.op = TerraOp::Stamp;
    tool.terraform.stamp_shape = StampShape::Hill;
    tool.terraform.stamp_height = 100.0;
    tool.brush.radius = 60.0;
    stroke(&mut state, &mut tool, &[DVec3::new(-120.0, 0.0, base)], false);
    let source_peak = at(&state, face, -128.0, 0.0) - base;
    assert!(source_peak > 80.0);
    tool.terraform.op = TerraOp::Clone;
    tool.terraform.clone_source = Some(DVec3::new(-128.0, 0.0, base));
    tool.terraform.strength = 1.0;
    tool.brush.radius = 50.0;
    let target = DVec3::new(128.0, 0.0, base);
    stroke(&mut state, &mut tool, &vec![target; 30], false);
    let copy = at(&state, face, 128.0, 0.0) - base;
    assert!((copy - source_peak).abs() < source_peak * 0.15, "copy {copy} source {source_peak}");
    tool.terraform.clone_source = None;
    assert_eq!(stroke(&mut state, &mut tool, &[target], false), 0, "no source, nothing painted");
}

#[test]
fn the_mask_protects_from_brushes_and_commands() {
    let (mut state, mut tool, face) = setup(TerraOp::Mask);
    tool.terraform.strength = 1.0;
    let c = center(&state, face);
    stroke(&mut state, &mut tool, &vec![c; 30], false);
    let d = state.doc.face_side(face).and_then(|s| s.disp.as_deref()).cloned().expect("disp");
    let m = mask::mask_of(&d).expect("mask painted");
    assert!(m.iter().any(|v| *v > 0.99));
    assert!(state.doc.to_vmf_string().contains(mask::MASK_BLOCK), "saved in the map");

    tool.interaction = Interaction::Geometry;
    tool.brush.geo_effect = GeoEffect::Raise;
    tool.brush.strength = 500.0;
    tool.brush.radius = 230.0;
    let before = heights(&state, face);
    stroke(&mut state, &mut tool, &[c; 5], false);
    let after = heights(&state, face);
    let mesh_now = mesh(&state, face);
    for (k, (a, b)) in before.iter().zip(&after).enumerate() {
        if m[k] > 0.999 {
            assert!((a - b).abs() < 1e-6, "masked vertex {k} moved {a} -> {b}");
        }
    }
    let free = mesh_now
        .base_positions
        .iter()
        .zip(before.iter().zip(&after))
        .filter(|(p, (a, b))| p.truncate().length() > 140.0 && p.truncate().length() < 200.0 && b > a)
        .count();
    assert!(free > 4, "the free vertices still rise");

    state.doc.select_faces(&[face], SelectOp::Replace);
    let before = heights(&state, face);
    tool.terrain.noise_kind = crate::disp::erode::NoiseKind::Fractal;
    crate::disp::erode::apply(&mut state, crate::disp::erode::TerrainOp::Thermal, &tool.terrain);
    crate::disp::commands::scale(&mut state, 3.0);
    let after = heights(&state, face);
    for (k, (a, b)) in before.iter().zip(&after).enumerate() {
        if m[k] > 0.999 {
            assert!((a - b).abs() < 1e-6, "whole-selection commands respect the mask ({k})");
        }
    }
    assert!(tool.set_option(&mut state, "mask_invert", "selection"));
    let inverted =
        mask::mask_of(state.doc.face_side(face).and_then(|s| s.disp.as_deref()).expect("disp")).expect("inverted");
    assert!(inverted.iter().zip(&m).all(|(a, b)| (a + b - 1.0).abs() < 0.002));
    assert!(tool.set_option(&mut state, "mask_clear", "all"));
    assert!(mask::mask_of(state.doc.face_side(face).and_then(|s| s.disp.as_deref()).expect("disp")).is_none());
}

#[test]
fn terraform_options_round_trip_through_scripts() {
    let (mut state, mut tool, _) = setup(TerraOp::Erode);
    for (k, v) in [
        ("mode", "terraform"),
        ("terraform", "stamp"),
        ("stamp", "volcano"),
        ("stamp_rotation", "45"),
        ("stamp_blend", "replace"),
        ("erosion_brush", "thermal"),
        ("paint_noise", "ridged"),
        ("plateau_level", "fixed"),
        ("clone_source", "1 2 3"),
        ("mask_show", "off"),
    ] {
        assert!(tool.set_option(&mut state, k, v), "{k} {v}");
    }
    let values = tool.option_values();
    let get = |k: &str| values.iter().find(|o| o.key == k).map(|o| o.value.clone());
    assert_eq!(get("mode").as_deref(), Some("terraform"));
    assert_eq!(get("terraform").as_deref(), Some("stamp"));
    assert_eq!(get("stamp").as_deref(), Some("volcano"));
    assert_eq!(get("stamp_rotation").as_deref(), Some("45"));
    assert_eq!(get("clone_source").as_deref(), Some("1 2 3"));
    assert!(!tool.set_option(&mut state, "terraform", "bogus"));
    assert!(!tool.set_option(&mut state, "stamp_image", "E:/does/not/exist.png"));
    assert_eq!(state.settings.mesh.disp_tool.get("terraform").map(String::as_str), Some("stamp"));
}

#[test]
fn a_short_click_stamps_and_wheel_rotates() {
    let (mut state, mut tool, face) = setup(TerraOp::Stamp);
    tool.terraform.stamp_shape = StampShape::Hill;
    tool.terraform.stamp_height = 120.0;
    let view = view3d(DVec3::new(0.0, 0.0, 600.0), 0.0, -89.0);
    let c = view.rect.center();
    let press = PointerFrame { pos: Some(c), pressed: true, down: true, ..PointerFrame::default() };
    tool.handle_pointer(&mut state, &view, press);
    tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, down: false, ..press });
    assert!(at(&state, face, 0.0, 0.0) > 100.0, "the click stamped a hill");
    assert_eq!(tool.terraform.stamp_rotation, 0.0);
    let flow = tool.wheel(&mut state, &view, 2.0, Modifiers { shift: true, ..Modifiers::default() });
    assert_eq!(flow, ToolFlow::Handled);
    assert!((tool.terraform.stamp_rotation - 30.0).abs() < 1e-9);
    assert_eq!(tool.wheel(&mut state, &view, 1.0, Modifiers::NONE), ToolFlow::Ignored, "the plain wheel still zooms");
}

#[test]
fn long_cliff_and_erosion_strokes_stay_bounded() {
    for op in [TerraOp::Cliff, TerraOp::Erode] {
        let (mut state, mut tool, face) = setup(op);
        shape(&mut state, &mut tool, face, StampShape::Mountain, 300.0);
        tool.terraform.cliff_strength = 1.0;
        tool.terraform.erode_rate = 10000.0;
        tool.terraform.erode_strength = 1.0;
        let (lo, hi) =
            heights(&state, face).iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), h| (a.min(*h), b.max(*h)));
        let c = center(&state, face);
        stroke(&mut state, &mut tool, &vec![c; 300], false);
        for h in heights(&state, face) {
            assert!(h >= lo - 60.0 && h <= hi + 60.0, "{op:?} {h} outside {lo}..{hi}");
        }
    }
}
