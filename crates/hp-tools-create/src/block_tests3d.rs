use egui::{Modifiers, Vec2};
use glam::DVec3;
use hp_app::Tool;
use hp_app::state::AppState;
use hp_geom::{Aabb, Primitive};
use hp_render::Overlay;

use super::{BlockTool, Limit, Phase3d, axis_label};
use crate::common::Mouse;
use crate::tests_support::{info_3d, info_3d_looking_down, settle};

fn brush(state: &mut AppState, min: [f64; 3], max: [f64; 3]) {
    let mut maker = BlockTool::new();
    maker.create(state, Aabb { min: DVec3::from(min), max: DVec3::from(max) }).expect("brush");
}

fn draw_footprint(
    tool: &mut BlockTool,
    state: &mut AppState,
    view: &hp_app::ViewInfo,
    a: DVec3,
    c: DVec3,
) -> egui::Pos2 {
    let a = view.world_to_screen(a).expect("a");
    let c = view.world_to_screen(c).expect("c");
    tool.handle_mouse(state, view, &Mouse::drag_start(a, a));
    tool.handle_mouse(state, view, &Mouse::drag(a, c));
    tool.handle_mouse(state, view, &Mouse::drag_end(a, c));
    c
}

fn room(state: &mut AppState) {
    brush(state, [-256.0, -256.0, -16.0], [256.0, 256.0, 0.0]);
    brush(state, [-256.0, -256.0, 128.0], [256.0, 256.0, 144.0]);
}

fn overlaps(a: &Aabb, b: &Aabb) -> bool {
    (0..3).all(|k| a.min[k] < b.max[k] - 1e-6 && b.min[k] < a.max[k] - 1e-6)
}

#[test]
fn snap_to_contact_stops_the_brush_flush_with_the_ceiling() {
    let mut state = settle();
    room(&mut state);
    let mut tool = BlockTool::new();
    assert!(state.settings.brush.block_draw.snap_contact, "on by default");
    let view = info_3d(DVec3::new(-200.0, 0.0, 60.0), 0.0, -15.0);
    let c = draw_footprint(&mut tool, &mut state, &view, DVec3::new(-30.0, -30.0, 0.0), DVec3::new(33.0, 15.0, 0.0));
    let far = c - Vec2::new(0.0, 4000.0);
    tool.handle_mouse(&mut state, &view, &Mouse::hover(far));
    let d = tool.draw3d.expect("drawing");
    let Phase3d::Height { height, wanted, limit, .. } = d.phase else { panic!("height phase") };
    assert!(wanted > 128.0, "the mouse asks for more than the room: {wanted}");
    assert_eq!(height, 128.0);
    assert_eq!(limit, Limit::Contact);
    assert!(tool.limit_suffix().is_some_and(|(s, _)| s.contains("snapped")));
    let mut bypass = Mouse::hover(far);
    bypass.modifiers = Modifiers::CTRL;
    tool.handle_mouse(&mut state, &view, &bypass);
    assert!(tool.draw3d.expect("drawing").height() > 128.0, "Ctrl ignores the limits");
    tool.handle_mouse(&mut state, &view, &Mouse::hover(far));
    let before = state.doc.stats().solids;
    let undo = state.doc.undo_labels().len();
    tool.handle_mouse(&mut state, &view, &Mouse::click(far));
    assert_eq!(state.doc.stats().solids, before + 1);
    assert_eq!(state.doc.undo_labels().len(), undo + 1, "one undo step");
    let last = tool.last_bounds().expect("created");
    assert_eq!(last.min.z, 0.0);
    assert_eq!(last.max.z, 128.0, "flush with the ceiling");

    let near = c - Vec2::new(0.0, 60.0);
    draw_footprint(&mut tool, &mut state, &view, DVec3::new(64.0, 64.0, 0.0), DVec3::new(96.0, 96.0, 0.0));
    tool.handle_mouse(&mut state, &view, &Mouse::hover(near));
    let d = tool.draw3d.expect("drawing");
    assert_eq!(d.limit(), Limit::Free);
    assert_eq!(d.height() % 16.0, 0.0, "grid snapping when nothing clamps");
}

#[test]
fn limit_to_face_keeps_the_footprint_on_a_small_and_a_rotated_face() {
    let mut state = settle();
    state.settings.brush.block_draw.limit_to_face = true;
    brush(&mut state, [0.0, 0.0, 0.0], [32.0, 32.0, 64.0]);
    let mut tool = BlockTool::new();
    let view = info_3d_looking_down(DVec3::new(16.0, 16.0, 512.0));
    for far in [DVec3::new(200.0, 150.0, 64.0), DVec3::new(-90.0, 40.0, 64.0), DVec3::new(-20.0, -300.0, 64.0)] {
        draw_footprint(&mut tool, &mut state, &view, DVec3::new(16.0, 16.0, 64.0), far);
        let d = tool.draw3d.expect("a footprint");
        for p in [d.start, d.end] {
            assert!((0.0..=32.0).contains(&p.x) && (0.0..=32.0).contains(&p.y), "{p:?} for {far:?}");
        }
        assert!(d.base_valid());
        tool.cancel(&mut state);
    }

    let mut state = settle();
    state.settings.brush.block_draw.limit_to_face = true;
    let mut maker = BlockTool::new();
    maker.options.kind = Primitive::Cylinder;
    maker.options.sides = 4;
    maker
        .create(&mut state, Aabb { min: DVec3::new(-64.0, -64.0, 0.0), max: DVec3::new(64.0, 64.0, 32.0) })
        .expect("diamond");
    let top = state
        .doc
        .scene()
        .solids()
        .values()
        .next()
        .expect("solid")
        .geom
        .faces
        .iter()
        .find(|f| f.plane.normal.z > 0.9)
        .expect("top")
        .vertices
        .clone();
    let poly = crate::block_limits::project(&top, 2);
    let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 600.0));
    for far in [DVec3::new(60.0, 60.0, 32.0), DVec3::new(-70.0, 20.0, 32.0), DVec3::new(20.0, -90.0, 32.0)] {
        draw_footprint(&mut tool, &mut state, &view, DVec3::new(0.0, 0.0, 32.0), far);
        let d = tool.draw3d.expect("a footprint");
        for (x, y) in [(d.start.x, d.start.y), (d.end.x, d.end.y), (d.start.x, d.end.y), (d.end.x, d.start.y)] {
            let p = glam::DVec2::new(x, y);
            let inside = (0..poly.len()).all(|k| {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                let area: f64 = (0..poly.len()).map(|n| poly[n].perp_dot(poly[(n + 1) % poly.len()])).sum();
                (b - a).perp_dot(p - a) * area.signum() >= -1e-6
            });
            assert!(inside, "{p:?} is outside the face for {far:?}");
        }
        tool.cancel(&mut state);
    }
    let mut out = Overlay::default();
    draw_footprint(&mut tool, &mut state, &view, DVec3::new(0.0, 0.0, 32.0), DVec3::new(20.0, 20.0, 32.0));
    tool.overlay(&state, &view, &mut out);
    assert!(out.lines.len() > 8, "the face boundary is outlined");
}

#[test]
fn no_overlap_never_lets_the_new_brush_intersect_another() {
    let mut state = settle();
    state.settings.brush.block_draw.no_overlap = true;
    state.settings.brush.block_draw.snap_contact = false;
    brush(&mut state, [-256.0, -256.0, -16.0], [256.0, 256.0, 0.0]);
    brush(&mut state, [10.0, 10.0, 40.0], [12.0, 12.0, 400.0]);
    let pole = Aabb { min: DVec3::new(10.0, 10.0, 40.0), max: DVec3::new(12.0, 12.0, 400.0) };
    let mut tool = BlockTool::new();
    let view = info_3d(DVec3::new(-150.0, 30.0, 30.0), 0.0, -10.0);
    let c = draw_footprint(&mut tool, &mut state, &view, DVec3::new(60.0, 60.0, 0.0), DVec3::new(0.0, 0.0, 0.0));
    let far = c - Vec2::new(0.0, 4000.0);
    tool.handle_mouse(&mut state, &view, &Mouse::hover(far));
    let d = tool.draw3d.expect("drawing");
    assert_eq!(d.limit(), Limit::Overlap, "a pole thinner than the ray spacing still stops it");
    assert_eq!(d.height(), 40.0);
    let mut out = Overlay::default();
    tool.overlay(&state, &view, &mut out);
    assert!(out.lines.iter().any(|l| l.color[0] > 0.9 && l.color[1] < 0.5), "red outline at the limit");
    tool.handle_mouse(&mut state, &view, &Mouse::click(far));
    let last = tool.last_bounds().expect("created");
    assert!(!overlaps(&last, &pole), "{last:?}");
    assert_eq!(last.max.z, 40.0);

    brush(&mut state, [100.0, 100.0, -8.0], [110.0, 110.0, 8.0]);
    let before = state.doc.stats().solids;
    let c = draw_footprint(&mut tool, &mut state, &view, DVec3::new(90.0, 90.0, 0.0), DVec3::new(130.0, 130.0, 0.0));
    let up = c - Vec2::new(0.0, 300.0);
    tool.handle_mouse(&mut state, &view, &Mouse::hover(up));
    let d = tool.draw3d.expect("drawing");
    assert!(d.blocked(), "the stub crosses the base");
    assert!(!tool.is_ready());
    tool.handle_mouse(&mut state, &view, &Mouse::click(up));
    assert_eq!(state.doc.stats().solids, before, "no overlapping brush from a click");
    assert!(!tool.confirm(&mut state), "nor from Enter");
    assert!(tool.limit_suffix().is_some_and(|(s, _)| s.contains("overlaps")));
    let mut bypass = Mouse::click(up);
    bypass.modifiers = Modifiers::CTRL;
    tool.handle_mouse(&mut state, &view, &bypass);
    assert_eq!(state.doc.stats().solids, before + 1, "Ctrl ignores the limit");
}

#[test]
fn direction_is_shown_before_dragging_and_can_be_flipped_or_locked() {
    let mut state = settle();
    brush(&mut state, [100.0, -64.0, 0.0], [116.0, 64.0, 128.0]);
    let mut tool = BlockTool::new();
    let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 0.0, 0.0);
    let p = view.world_to_screen(DVec3::new(100.0, 10.0, 70.0)).expect("wall");
    tool.handle_mouse(&mut state, &view, &Mouse::hover(p));
    let (_, axis, sign) = tool.arrow(&state).expect("an arrow before the drag");
    assert_eq!(axis_label(axis, sign), "-X", "out of the wall toward the camera");
    let mut out = Overlay::default();
    tool.overlay(&state, &view, &mut out);
    assert!(!out.triangles_on_top.is_empty(), "the hovered face is highlighted");
    hp_app::testing::run_ui(&mut state, |ui, state| tool.paint(state, &view, ui.painter()));

    let ctx = egui::Context::default();
    let key = |k: egui::Key| egui::RawInput {
        events: vec![egui::Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    ctx.begin_pass(key(super::FLIP_KEY));
    assert!(tool.key_input(&mut state, &ctx));
    let _ = ctx.end_pass();
    let (_, axis, sign) = tool.arrow(&state).expect("arrow");
    assert_eq!(axis_label(axis, sign), "+X", "flipped into the wall");
    tool.set_flip(&state, false);

    let a = p;
    let c = view.world_to_screen(DVec3::new(100.0, -20.0, 40.0)).expect("c");
    tool.handle_mouse(&mut state, &view, &Mouse::drag_start(a, a));
    tool.handle_mouse(&mut state, &view, &Mouse::drag(a, c));
    assert_eq!(tool.draw3d.expect("base").axis, 0);
    ctx.begin_pass(key(egui::Key::Z));
    tool.key_input(&mut state, &ctx);
    let _ = ctx.end_pass();
    assert!(state.runtime.brush.block_axis_keys, "Z is kept from the mouselook while the base is drawn");
    let d = tool.draw3d.expect("base");
    assert_eq!((d.axis, tool.axis_lock), (2, Some(2)), "Z locks the drag to Z");
    tool.handle_mouse(&mut state, &view, &Mouse::drag_end(a, c));
    tool.cancel(&mut state);
    assert!(!state.runtime.brush.block_axis_keys);

    tool.set_axis_lock(None);
    tool.handle_mouse(&mut state, &view, &Mouse::hover(p));
    state.settings.brush.block_draw.direction_preview = false;
    assert!(tool.arrow(&state).is_none(), "the preview can be turned off");
}

#[test]
fn the_limits_are_tool_options_and_settings() {
    let mut state = settle();
    let mut tool = BlockTool::new();
    for (key, value) in
        [("snap_contact", "off"), ("limit_to_face", "on"), ("no_overlap", "on"), ("direction_preview", "off")]
    {
        assert!(tool.set_option(&mut state, key, value), "{key}");
        assert!(tool.option_values().iter().any(|o| o.key == key && o.value == value), "{key}");
    }
    let b = state.settings.brush.block_draw;
    assert!(!b.snap_contact && b.limit_to_face && b.no_overlap && !b.direction_preview);
    assert!(state.settings_dirty);
    assert!(tool.set_option(&mut state, "draw_axis", "y"));
    assert_eq!(tool.axis_lock, Some(1));
    assert!(tool.set_option(&mut state, "flip", "on"));
    assert!(tool.option_values().iter().any(|o| o.key == "draw_axis" && o.value == "y"));
    assert!(tool.option_values().iter().any(|o| o.key == "flip" && o.value == "on"));
    assert!(!tool.set_option(&mut state, "draw_axis", "w"));
    hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
}
