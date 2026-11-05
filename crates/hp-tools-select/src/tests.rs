use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Sense, vec2};
use glam::{DVec2, DVec3};
use hp_app::testing::{run_ui, test_state};
use hp_app::view::{Camera2d, Camera3d};
use hp_app::{Action, AppState, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind};
use hp_editor::ObjRef;
use hp_render::Overlay;

use crate::camera::{CamPart, CameraTool, mouselook, move_eye, move_target};
use crate::gizmo::{GizmoLayout, GizmoPart};
use crate::input::PointerEvent;
use crate::math::{Handle, HandleMode, handle_pos};
use crate::select::{NudgeKey, NudgeMod, NudgeView, QueryKind, SelectTool, nudge_delta, take_nudge_keys};

const WALL: ObjRef = ObjRef::Solid(3);
const FLOOR: ObjRef = ObjRef::Solid(2);
const LIGHT: ObjRef = ObjRef::Entity(5);

fn state() -> AppState {
    let s = test_state();
    crate::register_translations();
    s
}

fn view2d(kind: ViewKind) -> ViewInfo {
    ViewInfo {
        index: match kind {
            ViewKind::Top => 1,
            ViewKind::Front => 2,
            ViewKind::Side => 3,
            ViewKind::Perspective => 0,
        },
        kind,
        rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
        cam3d: Camera3d::default(),
        cam2d: Camera2d { center: DVec2::ZERO, zoom: 0.5 },
        pixels_per_point: 1.0,
        grid: 16.0,
    }
}

fn view3d() -> ViewInfo {
    let cam = Camera3d { pos: DVec3::new(0.0, -800.0, 64.0), yaw_deg: 90.0, pitch_deg: 0.0, ..Camera3d::default() };
    ViewInfo { index: 0, kind: ViewKind::Perspective, cam3d: cam, ..view2d(ViewKind::Top) }
}

fn scr(view: &ViewInfo, p: DVec3) -> Pos2 {
    view.world_to_screen(p).expect("point in front of the camera")
}

fn bounds(state: &AppState, obj: ObjRef) -> hp_geom::Aabb {
    state.doc.object_bounds(obj, &|e| state.entity_bounds(e)).expect("object bounds")
}

fn drag(tool: &mut SelectTool, state: &mut AppState, view: &ViewInfo, from: Pos2, to: Pos2, mods: Modifiers) {
    tool.handle_event(state, view, PointerEvent::DragStart { origin: from, pos: from }, mods);
    for i in 1..=4 {
        let t = i as f32 / 4.0;
        tool.handle_event(state, view, PointerEvent::Drag(from + (to - from) * t), mods);
    }
    tool.handle_event(state, view, PointerEvent::DragEnd(to), mods);
}

fn click(tool: &mut SelectTool, state: &mut AppState, view: &ViewInfo, at: Pos2, mods: Modifiers) {
    tool.handle_event(state, view, PointerEvent::Click(at), mods);
}

fn undo_steps(state: &AppState) -> usize {
    state.doc.history_len().0
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn translation_table_is_clean() {
    let problems = hp_app::i18n::lint_table(crate::TR);
    assert!(problems.is_empty(), "{problems:#?}");
    let core: std::collections::HashSet<&str> = hp_app::strings::TR.iter().map(|e| e.0).collect();
    for (k, _, _) in crate::TR {
        assert!(!core.contains(k), "key {k} collides with the core table");
        assert!(k.starts_with("seltool.") || k.starts_with("camtool."), "key {k} lacks the crate prefix");
    }
}

const SOURCES: [(&str, &str); 14] = [
    ("lib.rs", include_str!("lib.rs")),
    ("select.rs", include_str!("select.rs")),
    ("select/draw.rs", include_str!("select/draw.rs")),
    ("select/keys.rs", include_str!("select/keys.rs")),
    ("select/extra.rs", include_str!("select/extra.rs")),
    ("modal.rs", include_str!("modal.rs")),
    ("tests_sel.rs", include_str!("tests_sel.rs")),
    ("input.rs", include_str!("input.rs")),
    ("camera.rs", include_str!("camera.rs")),
    ("math.rs", include_str!("math.rs")),
    ("gizmo.rs", include_str!("gizmo.rs")),
    ("preview.rs", include_str!("preview.rs")),
    ("screen.rs", include_str!("screen.rs")),
    ("tests_qa.rs", include_str!("tests_qa.rs")),
];

#[test]
fn every_used_key_is_translated() {
    let _ = state();
    let ours: std::collections::HashSet<&str> = crate::TR.iter().map(|e| e.0).collect();
    for (name, src) in SOURCES {
        for prefix in ["\"seltool.", "\"camtool."] {
            let mut rest = src;
            while let Some(i) = rest.find(prefix) {
                let tail = &rest[i + 1..];
                let end = tail.find('"').unwrap_or(tail.len());
                let key = &tail[..end];
                assert!(
                    ours.contains(key) || name == "lib.rs" || name.starts_with("tests"),
                    "{name}: missing translation {key}"
                );
                rest = &tail[end..];
            }
        }
    }
    for key in ["tool.camera", "common.reset"] {
        assert!(hp_app::i18n::has_key(key), "core key {key} is missing");
    }
    for mode in HandleMode::ALL {
        assert!(ours.contains(mode.label_key()));
    }
}

#[test]
fn sources_have_no_long_dashes() {
    let tests_src = include_str!("tests.rs");
    for (name, src) in SOURCES.iter().copied().chain([("tests.rs", tests_src)]) {
        assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'), "{name} contains an em or en dash");
    }
}

#[test]
fn tools_are_registered() {
    let tools = crate::tools();
    let ids: Vec<ToolId> = tools.iter().map(|t| t.id()).collect();
    assert_eq!(ids, vec![ToolId::Select, ToolId::Camera]);
    assert!(tools.iter().all(|t| !t.wants_side_panel()));
}

#[test]
fn rubber_band_selects_the_wall_from_the_top_view() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    let from = scr(&v, DVec3::new(-300.0, 300.0, 0.0));
    let to = scr(&v, DVec3::new(300.0, 260.0, 0.0));
    drag(&mut tool, &mut state, &v, from, to, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    assert!(!tool.is_dragging());

    let to = scr(&v, DVec3::new(300.0, 250.0, 0.0));
    drag(&mut tool, &mut state, &v, from, to, Modifiers::ALT);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    drag(&mut tool, &mut state, &v, from, to, Modifiers::NONE);
    let mut sel = state.doc.selected_objects();
    sel.sort();
    assert_eq!(sel, vec![FLOOR, WALL]);

    state.doc.set_selection(&[LIGHT]);
    let far_a = scr(&v, DVec3::new(600.0, 600.0, 0.0));
    let far_b = scr(&v, DVec3::new(700.0, 700.0, 0.0));
    drag(&mut tool, &mut state, &v, far_a, far_b, Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    let to = scr(&v, DVec3::new(300.0, 260.0, 0.0));
    drag(&mut tool, &mut state, &v, from, to, Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT, WALL]);
    assert_eq!(undo_steps(&state), 0);
}

#[test]
fn clicks_select_cycle_and_switch_handles() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    let shared = scr(&v, DVec3::new(100.0, 256.0, 0.0));
    click(&mut tool, &mut state, &v, shared, Modifiers::NONE);
    let first = state.doc.selected_objects();
    assert_eq!(first.len(), 1);
    click(&mut tool, &mut state, &v, shared, Modifiers::NONE);
    let second = state.doc.selected_objects();
    assert_eq!(second.len(), 1);
    assert_ne!(first, second);
    click(&mut tool, &mut state, &v, shared, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), first);

    state.doc.set_selection(&[FLOOR]);
    let inside = scr(&v, DVec3::new(-150.0, -60.0, 0.0));
    assert_eq!(tool.handle_mode(), HandleMode::Scale);
    click(&mut tool, &mut state, &v, inside, Modifiers::NONE);
    assert_eq!(tool.handle_mode(), HandleMode::Rotate);
    click(&mut tool, &mut state, &v, inside, Modifiers::NONE);
    assert_eq!(tool.handle_mode(), HandleMode::Skew);
    click(&mut tool, &mut state, &v, inside, Modifiers::NONE);
    assert_eq!(tool.handle_mode(), HandleMode::Scale);
    assert_eq!(state.doc.selected_objects(), vec![FLOOR]);
    click(&mut tool, &mut state, &v, inside, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::DoubleClick(inside), Modifiers::NONE);
    assert_eq!(tool.handle_mode(), HandleMode::Scale);

    let wall_top = scr(&v, DVec3::new(0.0, 272.0, 0.0));
    click(&mut tool, &mut state, &v, wall_top, Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![FLOOR, WALL]);
    click(&mut tool, &mut state, &v, wall_top + vec2(40.0, 0.0), Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![FLOOR]);

    let empty = scr(&v, DVec3::new(700.0, -700.0, 0.0));
    click(&mut tool, &mut state, &v, empty, Modifiers::NONE);
    assert!(state.doc.selected_objects().is_empty());

    let light = scr(&v, DVec3::new(0.0, 0.0, 96.0));
    tool.handle_event(&mut state, &v, PointerEvent::DoubleClick(light), Modifiers::NONE);
    assert!(state.requested_actions.contains(&Action::Properties));
    assert!(state.doc.is_selected(LIGHT));
}

#[test]
fn move_drag_snaps_the_box_and_is_one_undo_step() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    let start = scr(&v, DVec3::new(0.0, 264.0, 0.0));
    let end = start + vec2(35.0, 2.5);
    tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: start, pos: start }, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::Drag(end), Modifiers::NONE);
    let m = tool.drag_matrix().expect("drag in progress");
    assert!((m.transform_point3(DVec3::ZERO) - DVec3::new(64.0, 0.0, 0.0)).length() < 1e-9);
    assert_eq!(undo_steps(&state), 0);
    tool.handle_event(&mut state, &v, PointerEvent::DragEnd(end), Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!(approx(b.min.x, -192.0) && approx(b.min.y, 256.0) && approx(b.min.z, 0.0), "{b:?}");
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(state.doc.undo_label(), Some("Move"));

    let start = scr(&v, b.center());
    drag(&mut tool, &mut state, &v, start, start + vec2(3.0, 0.0), Modifiers::ALT);
    assert!(approx(bounds(&state, WALL).min.x, -186.0));
    assert_eq!(undo_steps(&state), 2);
}

#[test]
fn clone_drag_duplicates_in_one_undo_step() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    let solids_before = state.doc.scene().solids().len();
    let start = scr(&v, DVec3::new(0.0, 264.0, 0.0));
    drag(&mut tool, &mut state, &v, start, start + vec2(35.0, 0.0), Modifiers::SHIFT);
    assert_eq!(state.doc.scene().solids().len(), solids_before + 1);
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(state.doc.undo_label(), Some("Duplicate"));
    assert!(approx(bounds(&state, WALL).min.x, -256.0));
    let sel = state.doc.selected_objects();
    assert_eq!(sel.len(), 1);
    assert_ne!(sel[0], WALL);
    assert!(approx(bounds(&state, sel[0]).min.x, -192.0));
    assert!(state.doc.undo());
    assert_eq!(state.doc.scene().solids().len(), solids_before);
}

#[test]
fn escape_cancels_a_drag_without_touching_the_document() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    let start = scr(&v, DVec3::new(0.0, 264.0, 0.0));
    tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: start, pos: start }, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::Drag(start + vec2(100.0, 0.0)), Modifiers::NONE);
    assert!(tool.is_dragging());
    assert!(tool.cancel(&mut state));
    assert!(!tool.is_dragging());
    tool.handle_event(&mut state, &v, PointerEvent::DragEnd(start + vec2(100.0, 0.0)), Modifiers::NONE);
    assert_eq!(undo_steps(&state), 0);
    assert!(approx(bounds(&state, WALL).min.x, -256.0));
    assert!(tool.cancel(&mut state));
    assert!(state.doc.selected_objects().is_empty());
    assert!(!tool.cancel(&mut state));
}

#[test]
fn scale_handle_resizes_the_selection() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    let rect = Rect::from_two_pos(scr(&v, DVec3::new(-256.0, 256.0, 0.0)), scr(&v, DVec3::new(256.0, 272.0, 0.0)));
    let h = handle_pos(rect, Handle::new(1, 0));
    drag(&mut tool, &mut state, &v, h, h + vec2(32.0, 0.0), Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!(approx(b.max.x, 320.0) && approx(b.min.x, -256.0), "{b:?}");
    assert!(approx(b.size().y, 16.0) && approx(b.size().z, 128.0));
    assert_eq!(state.doc.undo_label(), Some("Scale"));
    assert_eq!(undo_steps(&state), 1);
}

#[test]
fn rotate_handles_snap_to_15_degrees() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    tool.set_handle_mode(HandleMode::Rotate);
    let b = bounds(&state, WALL);
    let rect = Rect::from_two_pos(scr(&v, b.min), scr(&v, b.max));
    let corner = handle_pos(rect, Handle::new(1, 1));
    let c = scr(&v, b.center());
    let r = corner - c;
    let a = 92f32.to_radians();
    let rotated = c + vec2(r.x * a.cos() + r.y * a.sin(), -r.x * a.sin() + r.y * a.cos());
    drag(&mut tool, &mut state, &v, corner, rotated, Modifiers::NONE);
    let nb = bounds(&state, WALL);
    assert!((nb.size().x - 16.0).abs() < 1e-3 && (nb.size().y - 512.0).abs() < 1e-3, "{nb:?}");
    assert!((nb.center() - b.center()).length() < 1e-3);
    assert_eq!(state.doc.undo_label(), Some("Rotate"));
}

#[test]
fn skew_handle_shears_along_the_edge() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    tool.set_handle_mode(HandleMode::Skew);
    let b = bounds(&state, WALL);
    let rect = Rect::from_two_pos(scr(&v, b.min), scr(&v, b.max));
    let top = handle_pos(rect, Handle::new(0, 1));
    drag(&mut tool, &mut state, &v, top, top + vec2(16.0, 0.0), Modifiers::NONE);
    let nb = bounds(&state, WALL);
    assert!(approx(nb.min.x, -256.0) && approx(nb.max.x, 288.0), "{nb:?}");
    assert!(approx(nb.min.y, 256.0) && approx(nb.max.y, 272.0));
    assert_eq!(state.doc.undo_label(), Some("Transform"));
}

#[test]
fn dragging_an_unselected_object_selects_and_moves_it() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Front);
    let edge = scr(&v, DVec3::new(0.0, 264.0, 128.0));
    drag(&mut tool, &mut state, &v, edge, edge + vec2(0.0, -32.0), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    let b = bounds(&state, WALL);
    assert!(approx(b.min.z, 64.0), "{b:?}");
    assert!(approx(b.min.x, -256.0) && approx(b.min.y, 256.0));
}

#[test]
fn single_point_entity_moves_snap_its_origin() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[LIGHT]);
    let start = scr(&v, DVec3::new(0.0, 0.0, 96.0));
    drag(&mut tool, &mut state, &v, start, start + vec2(6.5, 0.0), Modifiers::NONE);
    let origin = state.doc.scene().entity(5).and_then(|n| n.entity.origin()).expect("origin");
    assert_eq!(origin, DVec3::new(16.0, 0.0, 96.0));
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
}

fn key_frame(ctx: &egui::Context, state: &mut AppState, tool: &mut dyn Tool, events: Vec<Event>) -> bool {
    let mut handled = false;
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        handled = tool.key_input(state, ui.ctx());
    });
    out.textures_delta.clear();
    handled
}

#[test]
fn arrow_nudges_merge_into_one_undo_step() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let ctx = egui::Context::default();
    state.doc.set_selection(&[WALL]);
    state.ui.hovered_view = Some(1);
    for _ in 0..3 {
        assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowRight, Modifiers::NONE)]));
    }
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowUp, Modifiers::ALT)]));
    let b = bounds(&state, WALL);
    assert!(approx(b.min.x, -256.0 + 48.0) && approx(b.min.y, 257.0), "{b:?}");
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(state.doc.undo_label(), Some("Nudge"));
    assert!(state.doc.undo());
    assert!(approx(bounds(&state, WALL).min.x, -256.0));

    let solids = state.doc.scene().solids().len();
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowRight, Modifiers::SHIFT)]));
    assert_eq!(state.doc.scene().solids().len(), solids + 1);
    assert_eq!(state.doc.undo_label(), Some("Duplicate"));
    let copy = state.doc.selected_objects();
    assert_ne!(copy, vec![WALL]);
    assert!(approx(bounds(&state, copy[0]).min.x, -256.0 + 16.0));
    assert!(state.doc.undo());
    state.doc.set_selection(&[WALL]);
    state.settings.selection.shift_arrow_clones = false;
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowRight, Modifiers::SHIFT)]));
    assert!(approx(bounds(&state, WALL).min.x, -255.0));
    assert!(state.doc.undo());
    state.settings.selection.shift_arrow_clones = true;

    state.ui.hovered_view = Some(2);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::PageUp, Modifiers::NONE)]));
    assert!(approx(bounds(&state, WALL).min.x, -240.0));

    assert!(!key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowLeft, Modifiers::COMMAND)]));
    state.doc.clear_selection();
    assert!(!key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowLeft, Modifiers::NONE)]));
    state.doc.set_selection(&[WALL]);
    state.settings.edit.arrow_key_nudge = false;
    assert!(!key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowLeft, Modifiers::NONE)]));
}

#[test]
fn escape_key_clears_the_selection() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let ctx = egui::Context::default();
    state.doc.set_selection(&[WALL]);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::Escape, Modifiers::NONE)]));
    assert!(state.doc.selected_objects().is_empty());
    assert!(!key_frame(&ctx, &mut state, &mut tool, vec![key(Key::Escape, Modifiers::NONE)]));
}

#[test]
fn nudge_directions() {
    let top = NudgeView::View2d(ViewKind::Top);
    assert_eq!(nudge_delta(NudgeKey::Right, top, 16.0), DVec3::new(16.0, 0.0, 0.0));
    assert_eq!(nudge_delta(NudgeKey::Down, top, 16.0), DVec3::new(0.0, -16.0, 0.0));
    assert_eq!(nudge_delta(NudgeKey::PageUp, top, 8.0), DVec3::new(0.0, 0.0, 8.0));
    let front = NudgeView::View2d(ViewKind::Front);
    assert_eq!(nudge_delta(NudgeKey::Right, front, 1.0), DVec3::Y);
    assert_eq!(nudge_delta(NudgeKey::Up, front, 1.0), DVec3::Z);
    assert_eq!(nudge_delta(NudgeKey::PageDown, front, 1.0), DVec3::NEG_X);
    let side = NudgeView::View2d(ViewKind::Side);
    assert_eq!(nudge_delta(NudgeKey::PageUp, side, 1.0), DVec3::NEG_Y);
    let cam = Camera3d { yaw_deg: 90.0, pitch_deg: -30.0, ..Camera3d::default() };
    let v3 = NudgeView::View3d { right: cam.right(), forward: cam.forward(), up: cam.up() };
    assert!((nudge_delta(NudgeKey::Right, v3, 1.0) - DVec3::X).length() < 1e-9);
    assert!((nudge_delta(NudgeKey::Up, v3, 1.0) - DVec3::Y).length() < 1e-9);
    assert_eq!(nudge_delta(NudgeKey::PageUp, v3, 2.0), DVec3::new(0.0, 0.0, 2.0));
    let down = Camera3d { yaw_deg: 0.0, pitch_deg: -89.0, ..Camera3d::default() };
    let v3 = NudgeView::View3d { right: down.right(), forward: down.forward(), up: down.up() };
    assert!((nudge_delta(NudgeKey::Up, v3, 1.0) - DVec3::X).length() < 1e-9);

    let mut events = vec![
        key(Key::ArrowLeft, Modifiers::SHIFT),
        key(Key::A, Modifiers::NONE),
        key(Key::ArrowUp, Modifiers::COMMAND),
        key(Key::PageDown, Modifiers::NONE),
    ];
    let keys = take_nudge_keys(&mut events);
    assert_eq!(keys, vec![(NudgeKey::Left, NudgeMod::Shift), (NudgeKey::PageDown, NudgeMod::Plain)]);
    assert_eq!(events.len(), 2);
}

#[test]
fn clicks_and_hover_in_3d() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let wall = scr(&v, DVec3::new(-150.0, 256.0, 30.0));
    tool.handle_event(&mut state, &v, PointerEvent::Hover(wall), Modifiers::NONE);
    assert_eq!(tool.hovered_objects(), &[WALL]);
    let mut out = Overlay::default();
    tool.overlay(&state, &v, &mut out);
    assert!(out.lines_on_top.is_empty());
    state.settings.view.hover_outline = true;
    let mut out = Overlay::default();
    tool.overlay(&state, &v, &mut out);
    assert_eq!(out.lines_on_top.len(), 12);
    state.settings.view.hover_outline = false;
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    click(&mut tool, &mut state, &v, wall, Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT, WALL]);
    let g =
        GizmoLayout::new(&v, state.doc.selection_center(&|e| state.entity_bounds(e)).expect("center")).expect("gizmo");
    let arrow = scr(&v, g.center + DVec3::X * g.size * 0.7);
    click(&mut tool, &mut state, &v, arrow, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT, WALL]);
    let sky = Pos2::new(5.0, 5.0);
    click(&mut tool, &mut state, &v, sky, Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects().len(), 2);
    click(&mut tool, &mut state, &v, sky, Modifiers::NONE);
    assert!(state.doc.selected_objects().is_empty());
    tool.handle_event(&mut state, &v, PointerEvent::DoubleClick(light), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    assert!(state.requested_actions.contains(&Action::Properties));
    state.requested_actions.clear();
    tool.handle_event(&mut state, &v, PointerEvent::DoubleClick(wall), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    assert!(state.requested_actions.is_empty());
    tool.handle_event(&mut state, &v, PointerEvent::Leave, Modifiers::NONE);
    assert!(tool.hovered_objects().is_empty());
}

fn wall_gizmo(state: &AppState, v: &ViewInfo) -> GizmoLayout {
    GizmoLayout::new(v, bounds(state, WALL).center()).expect("gizmo visible")
}

#[test]
fn gizmo_axis_drag_moves_along_the_axis() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    state.doc.set_selection(&[WALL]);
    let g = wall_gizmo(&state, &v);
    let c = g.center;
    let grab = c + DVec3::X * g.size * 0.6;
    let p = scr(&v, grab);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(p), Modifiers::NONE);
    assert_eq!(tool.hovered_gizmo(), Some(GizmoPart::Axis(0)));
    let to = scr(&v, grab + DVec3::new(100.0, 0.0, 40.0));
    drag(&mut tool, &mut state, &v, p, to, Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!(approx(b.min.x, -160.0) && approx(b.min.y, 256.0) && approx(b.min.z, 0.0), "{b:?}");
    assert_eq!(state.doc.undo_label(), Some("Move"));
    assert_eq!(undo_steps(&state), 1);
}

#[test]
fn gizmo_plane_drag_moves_in_the_plane() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    state.doc.set_selection(&[WALL]);
    let g = wall_gizmo(&state, &v);
    let quad = g.plane_quad(1);
    let mid = (quad[0] + quad[2]) * 0.5;
    let p = scr(&v, mid);
    assert_eq!(g.hit(&v, p, crate::gizmo::GizmoMode::Move), Some(GizmoPart::Plane(1)));
    let to = scr(&v, mid + DVec3::new(64.0, 0.0, 32.0));
    drag(&mut tool, &mut state, &v, p, to, Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!(approx(b.min.x, -192.0) && approx(b.min.y, 256.0) && approx(b.min.z, 32.0), "{b:?}");
}

#[test]
fn gizmo_ring_drag_rotates() {
    let mut state = state();
    let mut tool = SelectTool::new();
    tool.set_gizmo_rotate(true);
    let v = view3d();
    state.doc.set_selection(&[WALL]);
    let g = wall_gizmo(&state, &v);
    let start = g.ring_point(1, 8);
    let end = g.ring_point(1, 64 - 8);
    let p = scr(&v, start);
    assert_eq!(g.hit(&v, p, crate::gizmo::GizmoMode::Rotate), Some(GizmoPart::Ring(1)));
    drag(&mut tool, &mut state, &v, p, scr(&v, end), Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!((b.size().x - 128.0).abs() < 1e-3 && (b.size().z - 512.0).abs() < 1e-3, "{b:?}");
    assert_eq!(state.doc.undo_label(), Some("Rotate"));
}

#[test]
fn gizmo_shift_drag_clones() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    state.doc.set_selection(&[WALL]);
    let g = wall_gizmo(&state, &v);
    let grab = g.center + DVec3::X * g.size * 0.6;
    drag(&mut tool, &mut state, &v, scr(&v, grab), scr(&v, grab + DVec3::X * 64.0), Modifiers::SHIFT);
    assert_eq!(state.doc.scene().solids().len(), 4);
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(state.doc.undo_label(), Some("Duplicate"));
}

#[test]
fn options_bar_leaves_the_select_mode_to_the_toolbar() {
    let _lang = hp_app::testing::lang_lock();
    let mut state = state();
    let mut tool = SelectTool::new();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 200.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| tool.options_ui(ui, &mut state));
    out.textures_delta.clear();
    let labels: Vec<String> = out
        .platform_output
        .accesskit_update
        .map(|u| u.nodes.into_iter().filter_map(|(_, n)| n.label().map(str::to_owned)).collect())
        .unwrap_or_default();
    let texts = |key: &str| hp_app::i18n::Lang::ALL.map(|l| hp_app::i18n::tr_in(key, l));
    assert!(labels.iter().any(|l| texts("seltool.box.touching").contains(&l.as_str())), "{labels:?}");
    for key in
        ["select_mode.label", "action.select_mode_groups", "action.select_mode_objects", "action.select_mode_solids"]
    {
        let found = labels.iter().find(|l| texts(key).contains(&l.as_str()));
        assert!(found.is_none(), "{found:?} is in the options bar: {labels:?}");
    }
}

#[test]
fn options_paint_and_hints_do_not_panic() {
    let _lang = hp_app::testing::lang_lock();
    let mut state = state();
    let mut tool = SelectTool::new();
    let mut camera = CameraTool::new();
    state.doc.set_selection(&[WALL, LIGHT]);
    run_ui(&mut state, |ui, state| {
        tool.options_ui(ui, state);
        camera.options_ui(ui, state);
    });
    for lang in hp_app::i18n::Lang::ALL {
        hp_app::i18n::set_lang(lang);
        for mode in HandleMode::ALL {
            tool.set_handle_mode(mode);
            run_ui(&mut state, |ui, state| {
                let rect = ui.max_rect();
                for v in [view2d(ViewKind::Top), view2d(ViewKind::Front), view2d(ViewKind::Side), view3d()] {
                    let v = ViewInfo { rect, ..v };
                    tool.paint(state, &v, ui.painter());
                    camera.paint(state, &v, ui.painter());
                    let mut out = Overlay::default();
                    tool.overlay(state, &v, &mut out);
                    camera.overlay(state, &v, &mut out);
                    assert!(tool.cursor(state, &v, rect.center()).is_some());
                    assert!(camera.cursor(state, &v, rect.center()).is_some());
                }
            });
            let hint = tool.status_hint(&state).expect("hint");
            assert!(!hint.contains("seltool."), "{hint}");
        }
    }
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    assert!(camera.status_hint(&state).is_some_and(|h| !h.contains("camtool.")));

    let v = view2d(ViewKind::Top);
    let b = bounds(&state, WALL);
    state.doc.set_selection(&[WALL]);
    let rect = Rect::from_two_pos(scr(&v, b.min), scr(&v, b.max));
    let cases: [(HandleMode, Pos2); 4] = [
        (HandleMode::Scale, scr(&v, b.center())),
        (HandleMode::Scale, handle_pos(rect, Handle::new(1, 1))),
        (HandleMode::Rotate, handle_pos(rect, Handle::new(-1, -1))),
        (HandleMode::Skew, handle_pos(rect, Handle::new(-1, 0))),
    ];
    for (mode, at) in cases {
        tool.set_handle_mode(mode);
        tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: at, pos: at }, Modifiers::SHIFT);
        tool.handle_event(&mut state, &v, PointerEvent::Drag(at + vec2(30.0, -20.0)), Modifiers::SHIFT);
        assert!(tool.is_dragging());
        let hint = tool.status_hint(&state).expect("hint");
        assert!(!hint.contains("seltool."), "{hint}");
        assert!(tool.cursor(&state, &v, at).is_some());
        run_ui(&mut state, |ui, state| {
            for view in [view2d(ViewKind::Top), view2d(ViewKind::Front), view3d()] {
                tool.paint(state, &view, ui.painter());
                let mut out = Overlay::default();
                tool.overlay(state, &view, &mut out);
                assert!(!out.lines_on_top.is_empty());
            }
        });
        assert!(tool.cancel(&mut state));
    }
    state.doc.clear_selection();
    let a = scr(&v, DVec3::new(600.0, 600.0, 0.0));
    tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: a, pos: a }, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::Drag(a + vec2(50.0, 50.0)), Modifiers::ALT);
    assert!(tool.status_hint(&state).is_some());
    run_ui(&mut state, |ui, state| tool.paint(state, &v, ui.painter()));
    tool.deactivate(&mut state);
    assert!(!tool.is_dragging());
}

#[test]
fn query_selects_by_class_and_material() {
    let mut state = state();
    let mut tool = SelectTool::new();
    tool.set_query(QueryKind::Class, "light");
    assert_eq!(tool.run_query(&mut state, false), 1);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    tool.set_query(QueryKind::Class, "player");
    assert_eq!(tool.run_query(&mut state, true), 1);
    assert_eq!(state.doc.selected_objects().len(), 2);
    tool.set_query(QueryKind::Class, "npc_*");
    assert_eq!(tool.run_query(&mut state, false), 0);
    assert_eq!(state.doc.selected_objects().len(), 2);
    tool.set_query(QueryKind::Material, "concrete/*");
    assert_eq!(tool.run_query(&mut state, false), 1);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    tool.set_query(QueryKind::Material, "   ");
    assert_eq!(tool.run_query(&mut state, false), 0);
}

fn cam_drag(tool: &mut CameraTool, state: &mut AppState, view: &ViewInfo, from: Pos2, to: Pos2, mods: Modifiers) {
    tool.handle_event(state, view, PointerEvent::DragStart { origin: from, pos: from }, mods);
    tool.handle_event(state, view, PointerEvent::Drag(to), mods);
    tool.handle_event(state, view, PointerEvent::DragEnd(to), mods);
}

#[test]
fn camera_markers_drag_in_2d() {
    let mut state = state();
    let mut tool = CameraTool::new();
    let v = view2d(ViewKind::Top);
    let start = state.cameras.cam3d;
    let target = start.pos + start.forward() * tool.look_distance();
    let eye = scr(&v, start.pos);
    assert_eq!(tool.hit_part(&start, &v, eye), Some(CamPart::Eye));
    assert_eq!(tool.hit_part(&start, &v, scr(&v, target)), Some(CamPart::Target));
    assert_eq!(tool.hit_part(&start, &view3d(), eye), None);

    cam_drag(&mut tool, &mut state, &v, eye, eye + vec2(50.0, 0.0), Modifiers::NONE);
    let cam = state.cameras.cam3d;
    assert!((cam.pos - (start.pos + DVec3::new(100.0, 0.0, 0.0))).length() < 1e-9);
    assert!(((target - cam.pos).normalize() - cam.forward()).length() < 1e-6);
    assert!((tool.look_distance() - (target - cam.pos).length()).abs() < 1e-6);

    let before = state.cameras.cam3d;
    let eye = scr(&v, before.pos);
    cam_drag(&mut tool, &mut state, &v, eye, eye + vec2(0.0, -25.0), Modifiers::SHIFT);
    let cam = state.cameras.cam3d;
    assert!(approx(cam.yaw_deg, before.yaw_deg) && approx(cam.pitch_deg, before.pitch_deg));
    assert!((cam.pos - (before.pos + DVec3::new(0.0, 50.0, 0.0))).length() < 1e-9);

    let cam = state.cameras.cam3d;
    let t = cam.pos + cam.forward() * tool.look_distance();
    let ts = scr(&v, t);
    cam_drag(&mut tool, &mut state, &v, ts, ts + vec2(-40.0, 30.0), Modifiers::NONE);
    let new_target = t + DVec3::new(-80.0, -60.0, 0.0);
    let cam = state.cameras.cam3d;
    assert!(((new_target - cam.pos).normalize() - cam.forward()).length() < 1e-6);

    let height = cam.pos.z;
    let a = scr(&v, DVec3::new(500.0, 500.0, 0.0));
    let b = scr(&v, DVec3::new(700.0, 500.0, 0.0));
    cam_drag(&mut tool, &mut state, &v, a, b, Modifiers::NONE);
    let cam = state.cameras.cam3d;
    assert!((cam.pos - DVec3::new(500.0, 500.0, height)).length() < 1e-9);
    assert!(approx(cam.yaw_deg, 0.0) && approx(cam.pitch_deg, 0.0));
    assert!(approx(tool.look_distance(), 200.0));

    let before = state.cameras.cam3d;
    let eye = scr(&v, before.pos);
    tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: eye, pos: eye }, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::Drag(eye + vec2(80.0, 80.0)), Modifiers::NONE);
    assert!(state.cameras.cam3d != before);
    assert!(tool.status_hint(&state).is_some_and(|h| !h.contains("camtool.")));
    assert!(tool.cancel(&mut state));
    assert_eq!(state.cameras.cam3d, before);
    assert!(!tool.cancel(&mut state));
}

#[test]
fn camera_mouselook_in_3d() {
    let mut state = state();
    let mut tool = CameraTool::new();
    let v = view3d();
    let before = state.cameras.cam3d;
    let p = Pos2::new(400.0, 300.0);
    tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: p, pos: p }, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::Drag(p + vec2(10.0, 20.0)), Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::DragEnd(p + vec2(10.0, 20.0)), Modifiers::NONE);
    let cam = state.cameras.cam3d;
    let sens = f64::from(state.settings.view.mouse_sensitivity);
    assert!(approx(cam.yaw_deg, before.yaw_deg - 10.0 * sens));
    assert!(approx(cam.pitch_deg, before.pitch_deg - 20.0 * sens));
    assert_eq!(cam.pos, before.pos);

    let mut c = Camera3d::default();
    mouselook(&mut c, vec2(0.0, -10_000.0), 0.15, false);
    assert!(approx(c.pitch_deg, 89.0));
    mouselook(&mut c, vec2(0.0, -10_000.0), 0.15, true);
    assert!(approx(c.pitch_deg, -89.0));
}

#[test]
fn camera_helpers() {
    let start = Camera3d { pos: DVec3::ZERO, yaw_deg: 0.0, pitch_deg: 0.0, ..Camera3d::default() };
    let mut cam = start;
    let d = move_eye(&mut cam, ViewKind::Top, &start, DVec3::new(100.0, 0.0, 0.0), DVec2::new(0.0, -100.0), false);
    assert!(approx(d, (2.0f64).sqrt() * 100.0));
    assert!(approx(cam.yaw_deg, 45.0));
    let mut cam = start;
    let d = move_target(&mut cam, ViewKind::Front, DVec3::new(100.0, 0.0, 0.0), DVec2::new(0.0, 100.0));
    assert!(approx(d, (2.0f64).sqrt() * 100.0));
    assert!(approx(cam.pitch_deg, 45.0));
}

struct Harness {
    ctx: egui::Context,
    time: f64,
    rect: Rect,
}

impl Harness {
    fn new() -> Self {
        Self { ctx: egui::Context::default(), time: 0.0, rect: Rect::NOTHING }
    }

    fn view(&self, kind: ViewKind) -> ViewInfo {
        let base = if kind.is_2d() { view2d(kind) } else { view3d() };
        ViewInfo { rect: self.rect, ..base }
    }

    fn frame(&mut self, state: &mut AppState, tool: &mut dyn Tool, kind: ViewKind, events: Vec<Event>) -> ToolFlow {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        self.time += 1.0 / 60.0;
        let mut flow = ToolFlow::Ignored;
        let mut rect = self.rect;
        let base = if kind.is_2d() { view2d(kind) } else { view3d() };
        let mut out = self.ctx.run_ui(input, |ui| {
            let (r, resp) = ui.allocate_exact_size(vec2(800.0, 600.0), Sense::click_and_drag());
            rect = r;
            let view = ViewInfo { rect: r, ..base };
            let pointer = resp.hover_pos().or_else(|| {
                (resp.dragged() || resp.drag_stopped()).then(|| ui.input(|i| i.pointer.latest_pos())).flatten()
            });
            let input = ViewInput {
                view: &view,
                response: &resp,
                pointer,
                modifiers: ui.input(|i| i.modifiers),
                navigating: false,
            };
            flow = tool.view_input(state, &input);
            tool.paint(state, &view, &ui.painter_at(r));
            let mut ov = Overlay::default();
            tool.overlay(state, &view, &mut ov);
            if let Some(p) = pointer {
                let _ = tool.cursor(state, &view, p);
            }
            let _ = tool.status_hint(state);
        });
        out.textures_delta.clear();
        self.rect = rect;
        flow
    }

    fn press(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers }
    }

    fn drag(&mut self, state: &mut AppState, tool: &mut dyn Tool, kind: ViewKind, a: Pos2, b: Pos2, mods: Modifiers) {
        self.frame(state, tool, kind, vec![Event::ModifiersChanged(mods), Event::PointerMoved(a)]);
        self.frame(state, tool, kind, vec![Self::press(a, true, mods)]);
        for i in 1..=5 {
            let p = a + (b - a) * (i as f32 / 5.0);
            self.frame(state, tool, kind, vec![Event::PointerMoved(p)]);
        }
        self.frame(state, tool, kind, vec![Self::press(b, false, mods)]);
        self.frame(state, tool, kind, vec![Event::ModifiersChanged(Modifiers::NONE)]);
    }

    fn click(&mut self, state: &mut AppState, tool: &mut dyn Tool, kind: ViewKind, at: Pos2) {
        self.frame(state, tool, kind, vec![Event::PointerMoved(at)]);
        self.frame(state, tool, kind, vec![Self::press(at, true, Modifiers::NONE)]);
        self.frame(state, tool, kind, vec![Self::press(at, false, Modifiers::NONE)]);
        self.time += 1.0;
        self.frame(state, tool, kind, vec![]);
    }
}

#[test]
fn real_egui_input_drives_the_selection_tool() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let mut h = Harness::new();
    h.frame(&mut state, &mut tool, ViewKind::Top, vec![]);
    let v = h.view(ViewKind::Top);
    let a = scr(&v, DVec3::new(-300.0, 300.0, 0.0));
    let b = scr(&v, DVec3::new(300.0, 260.0, 0.0));
    h.drag(&mut state, &mut tool, ViewKind::Top, a, b, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    assert!(!tool.is_dragging());
    let c = scr(&v, DVec3::new(0.0, 264.0, 0.0));
    h.drag(&mut state, &mut tool, ViewKind::Top, c, c + vec2(32.0, 0.0), Modifiers::NONE);
    assert!(approx(bounds(&state, WALL).min.x, -192.0), "{:?}", bounds(&state, WALL));
    assert_eq!(undo_steps(&state), 1);
    h.click(&mut state, &mut tool, ViewKind::Top, scr(&v, DVec3::new(600.0, -600.0, 0.0)));
    assert!(state.doc.selected_objects().is_empty());
    h.click(&mut state, &mut tool, ViewKind::Top, scr(&v, DVec3::new(100.0, -256.0, 0.0)));
    assert_eq!(state.doc.selected_objects(), vec![FLOOR]);

    h.frame(&mut state, &mut tool, ViewKind::Perspective, vec![]);
    let v3 = h.view(ViewKind::Perspective);
    h.click(&mut state, &mut tool, ViewKind::Perspective, scr(&v3, DVec3::new(-100.0, 256.0, 64.0)));
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
}

#[test]
fn real_egui_input_drives_the_camera_tool() {
    let mut state = state();
    let mut tool = CameraTool::new();
    let mut h = Harness::new();
    h.frame(&mut state, &mut tool, ViewKind::Perspective, vec![]);
    let before = state.cameras.cam3d;
    let c = h.rect.center();
    h.drag(&mut state, &mut tool, ViewKind::Perspective, c, c + vec2(40.0, 0.0), Modifiers::NONE);
    assert!(state.cameras.cam3d.yaw_deg < before.yaw_deg);
    assert_eq!(state.cameras.cam3d.pos, before.pos);
    h.frame(&mut state, &mut tool, ViewKind::Top, vec![]);
    let v = h.view(ViewKind::Top);
    let eye = scr(&v, state.cameras.cam3d.pos);
    let z = state.cameras.cam3d.pos.z;
    h.drag(&mut state, &mut tool, ViewKind::Top, eye, eye + vec2(30.0, 30.0), Modifiers::NONE);
    let cam = state.cameras.cam3d;
    assert!(approx(cam.pos.z, z));
    assert!((cam.pos - before.pos).length() > 50.0);
}

fn sample_maps() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let single = std::path::PathBuf::from(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf");
    if single.is_file() {
        out.push(single);
    }
    let dir =
        std::path::Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples");
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut maps: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmf")))
            .collect();
        maps.sort();
        out.extend(maps.into_iter().take(2));
    }
    out
}

#[test]
fn real_maps_select_all_move_and_undo() {
    let maps = sample_maps();
    if maps.is_empty() {
        eprintln!("no sample maps found, skipping");
        return;
    }
    for path in maps {
        let mut state = state();
        let doc = match hp_editor::Document::open(&path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skipping {}: {e}", path.display());
                continue;
            }
        };
        state.doc = doc;
        state.doc.select_all();
        let objs = state.doc.selected_objects();
        if objs.is_empty() {
            continue;
        }
        let before = state.doc.scene().objects_bounds(&objs, &|e| state.entity_bounds(e)).expect("bounds");
        let mut tool = SelectTool::new();
        let mut v = view2d(ViewKind::Top);
        let size = before.size();
        v.cam2d.center = DVec2::new(before.center().x, before.center().y);
        v.cam2d.zoom = (700.0 / size.x.max(size.y).max(16.0)).clamp(Camera2d::MIN_ZOOM, Camera2d::MAX_ZOOM);
        let start = scr(&v, before.center());
        let t0 = std::time::Instant::now();
        tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: start, pos: start }, Modifiers::NONE);
        let dist = (64.0 * v.cam2d.zoom) as f32;
        tool.handle_event(&mut state, &v, PointerEvent::Drag(start + vec2(dist, 0.0)), Modifiers::NONE);
        let mut out = Overlay::default();
        tool.overlay(&state, &v, &mut out);
        assert!(!out.lines_on_top.is_empty());
        assert!(out.lines_on_top.len() <= crate::preview::MAX_PREVIEW_EDGES);
        tool.handle_event(&mut state, &v, PointerEvent::DragEnd(start + vec2(dist, 0.0)), Modifiers::NONE);
        eprintln!("{}: {} objects moved in {:?}", path.display(), objs.len(), t0.elapsed());
        let after = state.doc.scene().objects_bounds(&objs, &|e| state.entity_bounds(e)).expect("bounds");
        assert!((after.min.x - before.min.x).abs() > 1.0, "{}: nothing moved", path.display());
        assert!(approx(after.min.y, before.min.y));
        assert!(state.doc.undo());
        let restored = state.doc.scene().objects_bounds(&objs, &|e| state.entity_bounds(e)).expect("bounds");
        assert!((restored.min - before.min).length() < 1e-3, "{}", path.display());
    }
}

#[test]
fn huge_selection_previews_only_its_box() {
    let mut state = state();
    let mut solids = Vec::new();
    let mut side_id = 1000u32;
    for i in 0..2000 {
        let x = f64::from(i % 50) * 64.0;
        let y = f64::from(i / 50) * 64.0;
        let params = hp_geom::PrimitiveParams {
            bounds: hp_geom::Aabb::new(DVec3::new(x, y, 512.0), DVec3::new(x + 32.0, y + 32.0, 544.0)),
            ..hp_geom::PrimitiveParams::default()
        };
        let pieces = hp_geom::make_primitive(&params, &mut || {
            side_id += 1;
            side_id
        })
        .expect("block");
        solids.extend(pieces);
    }
    state.doc.create_solids(solids, hp_editor::Owner::World).expect("create");
    state.doc.select_all();
    let objs = state.doc.selected_objects();
    let geom = crate::preview::PreviewGeom::capture(&state.doc, &objs, &|e| state.entity_bounds(e));
    assert!(geom.simplified);
    assert_eq!(geom.edges.len(), 12);
    let steps = undo_steps(&state);
    let mut tool = SelectTool::new();
    let mut v = view2d(ViewKind::Top);
    v.cam2d.zoom = 0.1;
    let b = bounds_of_selection(&state);
    let start = scr(&v, b.center());
    drag(&mut tool, &mut state, &v, start, start + vec2(12.8, 0.0), Modifiers::NONE);
    assert_eq!(undo_steps(&state), steps + 1);
    let nb = bounds_of_selection(&state);
    assert!(approx(nb.min.x - b.min.x, 128.0), "{b:?} {nb:?}");
}

fn bounds_of_selection(state: &AppState) -> hp_geom::Aabb {
    state.doc.selection_bounds(&|e| state.entity_bounds(e)).expect("selection bounds")
}

#[test]
fn a_document_change_during_a_drag_drops_it() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    let start = scr(&v, DVec3::new(0.0, 264.0, 0.0));
    tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: start, pos: start }, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::Drag(start + vec2(64.0, 0.0)), Modifiers::NONE);
    state.doc.translate(&[FLOOR], DVec3::new(0.0, 0.0, -16.0)).expect("move floor");
    tool.handle_event(&mut state, &v, PointerEvent::DragEnd(start + vec2(64.0, 0.0)), Modifiers::NONE);
    assert!(!tool.is_dragging());
    assert!(approx(bounds(&state, WALL).min.x, -256.0));
    assert_eq!(undo_steps(&state), 1);
}

#[test]
fn moves_snap_to_the_grid_of_the_object_and_of_the_view() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let spec = hp_app::settings::GridSpec {
        name: "Wall".into(),
        step: [24.0, 12.0, 16.0],
        origin: [5.0, 0.0, 0.0],
        angles: [0.0, 30.0, 0.0],
        ..Default::default()
    };
    state.put_custom_grid("", spec.clone(), true, false).expect("grid");
    state.doc.set_object_grid(&[WALL], &[], Some("Wall")).expect("attach");
    let frame = hp_app::grid::GridFrame::from_spec(&spec, 16.0);
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    let start = scr(&v, bounds(&state, WALL).center());
    drag(&mut tool, &mut state, &v, start, start + vec2(37.0, -21.0), Modifiers::NONE);
    let b = bounds(&state, WALL);
    let l = frame.to_local(DVec3::new(b.min.x, b.min.y, 0.0)) / frame.step;
    assert!((l.x - l.x.round()).abs() < 1e-6 && (l.y - l.y.round()).abs() < 1e-6, "min corner on a node: {l}");
    state.settings.grid.object_grids = false;
    let start = scr(&v, bounds(&state, WALL).center());
    drag(&mut tool, &mut state, &v, start, start + vec2(30.0, 0.0), Modifiers::NONE);
    assert!(approx(bounds(&state, WALL).min.x % 16.0, 0.0), "{:?}", bounds(&state, WALL));
    state.set_grid_mode(hp_app::settings::GridMode::PerView);
    state.settings.grid.per_view.front.size = 10.0;
    let f = view2d(ViewKind::Front);
    let start = scr(&f, bounds(&state, WALL).center());
    drag(&mut tool, &mut state, &f, start, start + vec2(12.0, 0.0), Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!(approx(b.min.y % 10.0, 0.0), "front moves land on the 10 grid: {b:?}");
}

#[test]
fn nudges_follow_the_steps_of_the_grid() {
    let mut state = state();
    state.settings.grid.mode = hp_app::settings::GridMode::PerView;
    state.settings.grid.per_view.top.size = 12.0;
    let mut tool = SelectTool::new();
    state.doc.set_selection(&[WALL]);
    let before = bounds(&state, WALL).min;
    tool.nudge_for_tests(&mut state, NudgeView::View2d(ViewKind::Top), &[(NudgeKey::Right, NudgeMod::Plain)]);
    let after = bounds(&state, WALL).min;
    assert!(approx(after.x - before.x, 12.0), "{before} -> {after}");
}

fn top_face(state: &AppState, id: u32) -> hp_editor::FaceRef {
    let n = state.doc.scene().solid(id).expect("solid");
    let i = n.geom.faces.iter().position(|f| f.plane.normal.z > 0.99).expect("top face");
    let side = n.geom.faces[i].side_index;
    hp_editor::FaceRef::new(id, side)
}

#[test]
fn arrows_move_the_picked_face_instead_of_the_object() {
    let mut state = state();
    state.settings.selection.element = hp_app::settings::SelectElement::Face;
    let mut tool = SelectTool::new();
    let face = top_face(&state, 3);
    state.doc.clear_selection();
    state.doc.select_faces(&[face], hp_editor::SelectOp::Replace);
    let before = bounds(&state, WALL);
    tool.nudge_for_tests(&mut state, NudgeView::View2d(ViewKind::Front), &[(NudgeKey::Up, NudgeMod::Plain)]);
    let after = bounds(&state, WALL);
    assert!(approx(after.max.z - before.max.z, 16.0), "{before:?} -> {after:?}");
    assert!(approx(after.min.z, before.min.z), "only the top face moved");
    assert_eq!(state.doc.selected_faces().len(), 1, "the face stays selected");
    assert!(state.doc.undo());
    assert!(approx(bounds(&state, WALL).max.z, before.max.z));
}

#[test]
fn arrows_move_picked_vertices_and_edges_and_keep_them_picked() {
    let mut state = state();
    state.settings.selection.element = hp_app::settings::SelectElement::Vertex;
    let mut tool = SelectTool::new();
    let b = bounds(&state, WALL);
    let top: Vec<DVec3> = [
        DVec3::new(b.min.x, b.min.y, b.max.z),
        DVec3::new(b.max.x, b.min.y, b.max.z),
        DVec3::new(b.max.x, b.max.y, b.max.z),
        DVec3::new(b.min.x, b.max.y, b.max.z),
    ]
    .to_vec();
    state.doc.set_selection(&[WALL]);
    let rev = state.doc.revision();
    state.runtime.selection.element_revision = rev;
    state.runtime.selection.edges = vec![hp_editor::EdgeRef::new(3, top[0], top[1])];
    tool.nudge_for_tests(&mut state, NudgeView::View2d(ViewKind::Front), &[(NudgeKey::Up, NudgeMod::Plain)]);
    let after = bounds(&state, WALL);
    assert!(approx(after.max.z, b.max.z + 16.0), "{b:?} -> {after:?}");
    let fresh = state.runtime.selection.fresh_edges(state.doc.revision()).to_vec();
    assert_eq!(fresh.len(), 1, "the picked edge follows the move");
    assert!(approx(fresh[0].a.z, top[0].z + 16.0));
}
