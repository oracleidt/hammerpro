use super::disp_live::{max_crack, quad_tiles};
use super::*;
use crate::disp::level::{SampleInto, brush_from_values, level_axis, pick_surface, sample_report, slope_grade};
use crate::disp::paint::{MAX_RADIUS, level_height, smooth_passes};

fn above(x: f64, y: f64) -> ViewInfo {
    view3d(DVec3::new(x, y, 600.0), 0.0, -89.0)
}

fn right_click(tool: &mut DisplacementTool, state: &mut AppState, view: &ViewInfo, ctrl: bool) {
    let c = view.rect.center();
    let mods = Modifiers { ctrl, command: ctrl, ..Modifiers::default() };
    let press = PointerFrame { pos: Some(c), right_pressed: true, right_down: true, mods, ..PointerFrame::default() };
    assert_eq!(tool.handle_pointer(state, view, press), ToolFlow::Handled);
    let release = PointerFrame { pos: Some(c), mods, ..PointerFrame::default() };
    tool.handle_pointer(state, view, release);
}

fn hit_under(state: &AppState, view: &ViewInfo) -> DVec3 {
    let (o, d) = view.ray(view.rect.center());
    pick_surface(state, &Ray::new(o, d)).expect("surface").point
}

fn raise_bump(state: &mut AppState, tool: &mut DisplacementTool, at: DVec3, height: f64) {
    tool.interaction = Interaction::Geometry;
    tool.brush.geo_effect = GeoEffect::Set;
    tool.brush.target_height = height;
    tool.brush.radius = 200.0;
    tool.brush.falloff = Falloff::Constant;
    tool.begin_stroke(state, 0);
    assert!(tool.paint_at(state, at, false) > 0);
    tool.end_stroke(state);
}

#[test]
fn smooth_passes_split_the_strength() {
    assert_eq!(smooth_passes(0.05), (0.05, 1));
    let (a, n) = smooth_passes(3.2);
    assert_eq!(n, 4);
    assert!((a - 0.8).abs() < 1e-12);
    let (a, n) = smooth_passes(1000.0);
    assert_eq!(n, 16);
    assert_eq!(a, 1.0);
    assert_eq!(smooth_passes(f64::NAN), (0.0, 1));
}

#[test]
fn level_height_interpolates_and_clamps() {
    let a = DVec3::new(0.0, 0.0, 10.0);
    let b = DVec3::new(100.0, 0.0, 60.0);
    assert!((level_height(a, b, None, DVec3::Z, DVec3::new(50.0, 30.0, -5.0)) - 35.0).abs() < 1e-9);
    assert!((level_height(a, b, None, DVec3::Z, DVec3::new(-50.0, 0.0, 0.0)) - 10.0).abs() < 1e-9);
    assert!((level_height(a, b, None, DVec3::Z, DVec3::new(500.0, 0.0, 0.0)) - 60.0).abs() < 1e-9);
    assert_eq!(level_height(a, b, Some(7.0), DVec3::Z, DVec3::ZERO), 7.0);
    let (deg, pct, run) = slope_grade(a, b, DVec3::Z).expect("grade");
    assert!((pct - 50.0).abs() < 1e-9 && (run - 100.0).abs() < 1e-9);
    assert!((deg - 26.565).abs() < 1e-3);
    assert!(slope_grade(a, a + DVec3::Z, DVec3::Z).is_none());
}

#[test]
fn boosted_smoothing_is_much_stronger() {
    let spike = |boost: f64, ctrl: bool| -> f64 {
        let (mut state, mut tool, face) = floor_with_disp(3);
        raise_bump(&mut state, &mut tool, DVec3::ZERO, 0.0);
        tool.brush.radius = 8.0;
        tool.brush.target_height = 64.0;
        tool.begin_stroke(&mut state, 0);
        tool.paint_at(&mut state, DVec3::ZERO, false);
        tool.end_stroke(&mut state);
        let peak = |s: &AppState| distances(s, face).iter().copied().fold(0.0f64, f64::max);
        assert!(peak(&state) > 63.0);
        tool.brush.geo_effect = GeoEffect::Smooth;
        tool.brush.radius = 128.0;
        tool.brush.smooth = 0.5;
        tool.brush.smooth_boost = boost;
        state.frame.dt = 1.0 / 60.0;
        let mods = Modifiers { ctrl, command: ctrl, ..Modifiers::default() };
        tool.begin_stroke(&mut state, 0);
        tool.paint_at_mods(&mut state, DVec3::ZERO, mods);
        tool.end_stroke(&mut state);
        peak(&state)
    };
    let plain = spike(1.0, false);
    let boosted = spike(64.0, false);
    let key = spike(1.0, true);
    assert!(plain > 50.0, "one plain dab barely smooths: {plain}");
    assert!(boosted < plain * 0.5, "the multiplier smooths far more: {boosted} vs {plain}");
    assert!(key < plain - 5.0, "Ctrl boosts the dab: {key} vs {plain}");
}

#[test]
fn right_click_samples_the_target_height_and_right_drag_still_resizes() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    state.doc.clear_face_selection();
    raise_bump(&mut state, &mut tool, DVec3::ZERO, 48.0);
    tool.brush.target_height = 0.0;
    let view = above(0.0, 0.0);
    let expected = 48.0;
    assert!((hit_under(&state, &view).z - expected).abs() < 1e-3);
    let radius = tool.brush.radius;
    right_click(&mut tool, &mut state, &view, false);
    assert!((tool.brush.target_height - expected).abs() < 1e-3, "{} {expected}", tool.brush.target_height);
    assert!(!tool.brush.height_world, "a displacement was under the point: height above its face");
    assert_eq!(tool.brush.radius, radius);
    assert!(!tool.is_painting());
    let _ = face;

    tool.brush.target_height = 5.0;
    let c = view.rect.center();
    let press = PointerFrame { pos: Some(c), right_pressed: true, right_down: true, ..PointerFrame::default() };
    tool.handle_pointer(&mut state, &view, press);
    let drag = PointerFrame { pos: Some(c + vec2(150.0, 0.0)), right_down: true, ..PointerFrame::default() };
    tool.handle_pointer(&mut state, &view, drag);
    tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(c + vec2(150.0, 0.0)), ..PointerFrame::default() });
    assert_eq!(tool.brush.radius, (radius * 2.0).round());
    assert_eq!(tool.brush.target_height, 5.0, "a right drag does not sample");

    tool.brush.geo_effect = GeoEffect::Raise;
    tool.brush.radius = radius;
    right_click(&mut tool, &mut state, &view, false);
    assert_eq!(tool.brush.target_height, 5.0, "raise has no level: nothing sampled");
}

#[test]
fn sampling_a_brush_switches_to_a_world_height() {
    let (mut state, mut tool, _) = floor_with_disp(2);
    block(&mut state, DVec3::new(600.0, 600.0, 0.0), DVec3::new(700.0, 700.0, 77.0));
    tool.brush.geo_effect = GeoEffect::Set;
    let view = above(650.0, 650.0);
    right_click(&mut tool, &mut state, &view, false);
    assert!(tool.brush.height_world, "no displacement under the brush top");
    assert!((tool.brush.target_height - 77.0).abs() < 1e-6, "{}", tool.brush.target_height);
}

#[test]
fn world_reference_levels_every_displacement_to_one_height() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let low = block(&mut state, DVec3::new(-256.0, -128.0, -16.0), DVec3::new(0.0, 128.0, 0.0));
    let high = block(&mut state, DVec3::new(0.0, -128.0, 16.0), DVec3::new(256.0, 128.0, 32.0));
    let faces = [side_facing(&state, low, DVec3::Z), side_facing(&state, high, DVec3::Z)];
    make_disps(&mut state, &mut tool, &faces, 2);
    state.doc.clear_face_selection();
    tool.brush.geo_effect = GeoEffect::Set;
    tool.brush.height_world = true;
    tool.brush.target_height = 50.0;
    tool.brush.radius = 1024.0;
    tool.brush.spatial = true;
    tool.brush.falloff = Falloff::Constant;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::new(0.0, 0.0, 0.0), false);
    tool.end_stroke(&mut state);
    for f in faces {
        for p in mesh(&state, f).positions {
            assert!((p.z - 50.0).abs() < 1e-6, "{p}");
        }
    }
    tool.brush.height_world = false;
    tool.brush.target_height = 10.0;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::new(0.0, 0.0, 0.0), false);
    tool.end_stroke(&mut state);
    assert!(mesh(&state, faces[0]).positions.iter().all(|p| (p.z - 10.0).abs() < 1e-6));
    assert!(mesh(&state, faces[1]).positions.iter().all(|p| (p.z - 42.0).abs() < 1e-6), "Hammer: above each face");
}

#[test]
fn slope_brush_makes_a_ramp_across_sewn_tiles() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let faces = quad_tiles(&mut state, &mut tool, [3, 3, 3, 3]);
    state.doc.clear_face_selection();
    tool.interaction = Interaction::Geometry;
    tool.brush.geo_effect = GeoEffect::Slope;
    tool.brush.radius = 300.0;
    tool.brush.falloff = Falloff::Constant;
    tool.brush.slope_strength = 1.0;
    tool.brush.autosew = true;
    let steps = undo_steps(&state);

    tool.begin_stroke(&mut state, 0);
    assert_eq!(tool.paint_at(&mut state, DVec3::ZERO, false), 0, "no start point yet");
    tool.end_stroke(&mut state);
    assert!(tool.brush.slope_end.is_none());

    tool.brush.slope_start = Some(DVec3::new(-200.0, 0.0, 0.0));
    tool.brush.slope_end = Some(DVec3::new(200.0, 0.0, 100.0));
    tool.begin_stroke(&mut state, 0);
    for x in [-150.0, -50.0, 50.0, 150.0] {
        tool.paint_at(&mut state, DVec3::new(x, 0.0, 0.0), false);
    }
    tool.end_stroke(&mut state);
    assert_eq!(undo_steps(&state), steps + 1, "one stroke, one undo step");
    for f in &faces {
        for p in mesh(&state, *f).positions {
            if p.x.abs() <= 120.0 && p.y.abs() <= 120.0 {
                let want = ((p.x + 200.0) / 400.0).clamp(0.0, 1.0) * 100.0;
                assert!((p.z - want).abs() < 1e-6, "{p} wants {want}");
            }
        }
    }
    assert!(max_crack(&state, &faces) < 1e-6, "sewn tiles stay closed");
}

#[test]
fn slope_points_from_right_clicks_and_the_end_from_the_stroke() {
    let (mut state, mut tool, _) = floor_with_disp(3);
    state.doc.clear_face_selection();
    tool.brush.geo_effect = GeoEffect::Slope;
    let first = above(-100.0, 0.0);
    right_click(&mut tool, &mut state, &first, false);
    let a = tool.brush.slope_start.expect("start");
    assert!((a - hit_under(&state, &first)).length() < 1e-6);
    assert!(tool.brush.slope_end.is_none());
    let second = above(100.0, 0.0);
    right_click(&mut tool, &mut state, &second, false);
    let b = tool.brush.slope_end.expect("end");
    assert!((b - hit_under(&state, &second)).length() < 1e-6);

    tool.brush.slope_start = Some(DVec3::new(a.x, a.y, 40.0));
    right_click(&mut tool, &mut state, &first, true);
    let moved = tool.brush.slope_start.expect("start");
    assert!((moved.z - 40.0).abs() < 1e-6, "Ctrl keeps the height: {moved}");
    assert_eq!(tool.brush.slope_end, Some(b));

    right_click(&mut tool, &mut state, &second, false);
    assert!(tool.brush.slope_start.is_some() && tool.brush.slope_end.is_none(), "a third click starts over");

    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::new(-20.0, 10.0, 0.0), false);
    tool.end_stroke(&mut state);
    let end = tool.brush.slope_end.expect("end taken where painting started");
    assert!((end - DVec3::new(-20.0, 10.0, 0.0)).length() < 1e-6);

    assert!(tool.cancel(&mut state), "Esc clears the points");
    assert!(tool.brush.slope_start.is_none() && tool.brush.slope_end.is_none());
}

#[test]
fn flatten_toward_the_target_plane() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    state.doc.clear_face_selection();
    tool.interaction = Interaction::Sculpt;
    tool.brush.sculpt.op = crate::disp::sculpt::SculptOp::Flatten;
    tool.brush.sculpt.flatten_target = true;
    tool.brush.sculpt.flatten_strength = 1.0;
    tool.brush.height_world = true;
    tool.brush.target_height = -24.0;
    tool.brush.radius = 2000.0;
    tool.brush.falloff = Falloff::Constant;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    for _ in 0..40 {
        tool.paint_at(&mut state, DVec3::ZERO, false);
    }
    tool.end_stroke(&mut state);
    assert!(mesh(&state, face).positions.iter().all(|p| (p.z + 24.0).abs() < 1e-3));
    assert_eq!(tool.sample_slot(), Some(SampleInto::Target));
}

#[test]
fn alpha_set_samples_the_alpha() {
    let (mut state, mut tool, face) = floor_with_disp(2);
    let mut d = (**state.doc.face_side(face).and_then(|s| s.disp.as_ref()).expect("disp")).clone();
    d.alphas = vec![200.0; 25];
    state.doc.set_disp_data("alpha", vec![(face, d)]).expect("alpha");
    tool.interaction = Interaction::Alpha;
    tool.brush.alpha_effect = crate::AlphaEffect::Set;
    right_click(&mut tool, &mut state, &above(0.0, 0.0), false);
    assert_eq!(tool.brush.alpha_value, 200.0);
}

#[test]
fn new_options_round_trip() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    for (k, v) in [
        ("effect", "slope"),
        ("smooth_boost", "12"),
        ("height_reference", "world"),
        ("slope_brush_start", "1 2 3"),
        ("slope_brush_end", "4 5 6"),
        ("slope_brush_strength", "0.25"),
        ("flatten_plane", "target"),
        ("radius", "16384"),
    ] {
        assert!(tool.set_option(&mut state, k, v), "{k} {v}");
    }
    assert!(!tool.set_option(&mut state, "radius", "20000"));
    assert!(!tool.set_option(&mut state, "smooth_boost", "0.5"));
    assert!(!tool.set_option(&mut state, "slope_brush_start", "1 2"));
    let values = tool.option_values();
    let get = |k: &str| values.iter().find(|o| o.key == k).map(|o| o.value.clone()).unwrap_or_default();
    assert_eq!(get("effect"), "slope");
    assert_eq!(get("smooth_boost"), "12");
    assert_eq!(get("height_reference"), "world");
    assert_eq!(get("slope_brush_start"), "1 2 3");
    assert_eq!(get("flatten_plane"), "target");
    assert_eq!(get("radius"), "16384");
    let b = brush_from_values(&values);
    assert!(b.height_world);
    assert_eq!(b.slope_end, Some(DVec3::new(4.0, 5.0, 6.0)));
    assert_eq!(b.radius, MAX_RADIUS);
    assert!(tool.set_option(&mut state, "slope_brush_start", "none"));
    assert!(tool.brush.slope_start.is_none());
    let mut restored = DisplacementTool::new();
    state.settings.mesh.disp_tool.clear();
    tool.save_settings(&mut state);
    restored.sync_settings(&mut state);
    assert_eq!(restored.brush, tool.brush);
}

#[test]
fn sample_report_measures_from_the_face_or_the_world() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    raise_bump(&mut state, &mut tool, DVec3::ZERO, 30.0);
    let view = above(0.0, 0.0);
    let (o, d) = view.ray(view.rect.center());
    let hit = pick_surface(&state, &Ray::new(o, d)).expect("hit");
    assert_eq!(hit.disp, Some(face));
    let mut brush = crate::BrushSettings::default();
    let r = sample_report(&state.doc, hit, &brush);
    assert!((r.face_height.expect("face") - 30.0).abs() < 1e-6);
    assert!(!r.world);
    brush.height_world = true;
    let r = sample_report(&state.doc, hit, &brush);
    assert!(r.world && (r.target - hit.point.z).abs() < 1e-9);
    assert_eq!(level_axis(&brush, None), DVec3::Z);
}

#[test]
fn a_huge_brush_on_many_tiles_stays_fast() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let mut faces = Vec::new();
    for i in 0..8 {
        for j in 0..8 {
            let min = DVec3::new(f64::from(i) * 512.0 - 2048.0, f64::from(j) * 512.0 - 2048.0, -16.0);
            let id = block(&mut state, min, min + DVec3::new(512.0, 512.0, 16.0));
            faces.push(side_facing(&state, id, DVec3::Z));
        }
    }
    make_disps(&mut state, &mut tool, &faces, 4);
    state.doc.clear_face_selection();
    tool.brush.radius = MAX_RADIUS;
    tool.brush.geo_effect = GeoEffect::Smooth;
    tool.brush.smooth_boost = 64.0;
    tool.brush.autosew = true;
    let start = std::time::Instant::now();
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.brush.geo_effect = GeoEffect::Raise;
    let n = tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.end_stroke(&mut state);
    assert_eq!(n, 64 * 289, "every vertex of the 64 tiles is under the brush");
    let spent = start.elapsed();
    assert!(spent.as_secs_f64() < 20.0, "{spent:?}");
    let view = above(0.0, 0.0);
    let start = std::time::Instant::now();
    tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(view.rect.center()), ..PointerFrame::default() });
    assert!(start.elapsed().as_secs_f64() < 5.0, "cursor ring {:?}", start.elapsed());
}

#[test]
fn a_sample_point_set_from_outside_shows_without_the_cursor() {
    let (mut state, mut tool, _) = floor_with_disp(3);
    raise_bump(&mut state, &mut tool, DVec3::ZERO, 30.0);
    tool.brush.target_height = 30.0;
    let view = above(0.0, 0.0);
    let count = |tool: &DisplacementTool, state: &AppState| {
        let mut out = hp_render::Overlay::default();
        tool.overlay(state, &view, &mut out);
        out.lines_on_top.len()
    };
    let before = count(&tool, &state);
    assert!(tool.set_option(&mut state, "sample_point", "0 0 30"));
    assert_eq!(tool.sample_point(), Some(DVec3::new(0.0, 0.0, 30.0)));
    let (src, target, axis) = tool.source_level(&state.doc).expect("level marker");
    assert_eq!(src, DVec3::new(0.0, 0.0, 30.0));
    assert!((target.z - 30.0).abs() < 1e-6 && axis == DVec3::Z);
    assert!(count(&tool, &state) > before, "the green ring is drawn with no cursor");
    tool.brush.geo_effect = GeoEffect::Raise;
    assert!(tool.source_level(&state.doc).is_none(), "only in level modes");
    assert!(tool.set_option(&mut state, "sample_point", "none"));
    assert_eq!(tool.sample_point(), None);

    tool.brush.geo_effect = GeoEffect::Slope;
    let before = count(&tool, &state);
    assert!(tool.set_option(&mut state, "slope_brush_start", "-100 0 0"));
    assert!(tool.set_option(&mut state, "slope_brush_end", "100 0 50"));
    assert!(count(&tool, &state) > before, "typed slope points draw their line with no cursor");
}
