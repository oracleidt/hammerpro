use egui::{Modifiers, Pos2, Rect, vec2};
use glam::{DVec2, DVec3};
use hp_app::view::{Camera2d, Camera3d};
use hp_app::{Action, AppState, Tool, ViewInfo, ViewKind};
use hp_editor::ObjRef;
use hp_render::Overlay;

use crate::gizmo::{GizmoLayout, GizmoMode};
use crate::input::PointerEvent;
use crate::modal::ModalKind;
use crate::select::{EditMode, SelectTool};

const WALL: ObjRef = ObjRef::Solid(3);
const FLOOR: ObjRef = ObjRef::Solid(2);
const LIGHT: ObjRef = ObjRef::Entity(5);

fn state() -> AppState {
    let s = hp_app::testing::test_state();
    crate::register_translations();
    s
}

fn top() -> ViewInfo {
    ViewInfo {
        index: 1,
        kind: ViewKind::Top,
        rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
        cam3d: Camera3d::default(),
        cam2d: Camera2d { center: DVec2::ZERO, zoom: 0.5 },
        pixels_per_point: 1.0,
        grid: 16.0,
    }
}

fn view3d() -> ViewInfo {
    let cam = Camera3d { pos: DVec3::new(0.0, -800.0, 64.0), yaw_deg: 90.0, pitch_deg: 0.0, ..Camera3d::default() };
    ViewInfo { index: 0, kind: ViewKind::Perspective, cam3d: cam, ..top() }
}

fn scr(view: &ViewInfo, p: DVec3) -> Pos2 {
    view.world_to_screen(p).expect("point in front of the camera")
}

fn min_x(state: &AppState, obj: ObjRef) -> f64 {
    state.doc.object_bounds(obj, &|e| state.entity_bounds(e)).expect("bounds").min.x
}

fn start_move(tool: &mut SelectTool, state: &mut AppState, v: &ViewInfo, dx: f32, mods: Modifiers) -> Pos2 {
    let start = scr(v, DVec3::new(0.0, 264.0, 0.0));
    tool.handle_event(state, v, PointerEvent::DragStart { origin: start, pos: start }, mods);
    tool.handle_event(state, v, PointerEvent::Drag(start + vec2(dx, 0.0)), mods);
    tool.publish_for_tests(state);
    start + vec2(dx, 0.0)
}

#[test]
fn a_move_is_shown_live_and_ends_as_one_undo_step() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = top();
    state.doc.set_selection(&[WALL]);
    let before = min_x(&state, WALL);
    let at = start_move(&mut tool, &mut state, &v, 100.0, Modifiers::NONE);
    let live = min_x(&state, WALL);
    assert!(live > before + 100.0, "the wall itself moved during the drag: {before} -> {live}");
    assert!(state.doc.in_transaction());
    assert!(state.runtime.selection.drag.as_ref().is_some_and(|d| d.applied));
    let mut out = Overlay::default();
    tool.overlay(&state, &view3d(), &mut out);
    assert!(!out.lines.is_empty(), "the original stays as a faint outline");
    assert!(out.lines_on_top.is_empty(), "no wireframe preview on top of the live object");
    tool.handle_event(&mut state, &v, PointerEvent::Drag(at + vec2(64.0, 0.0)), Modifiers::NONE);
    tool.publish_for_tests(&mut state);
    let further = min_x(&state, WALL);
    assert!(further > live, "the live copy follows the mouse");
    tool.handle_event(&mut state, &v, PointerEvent::DragEnd(at + vec2(64.0, 0.0)), Modifiers::NONE);
    tool.publish_for_tests(&mut state);
    assert!(!state.doc.in_transaction());
    assert_eq!(state.doc.history_len().0, 1);
    assert_eq!(state.doc.undo_label(), Some("Move"));
    assert_eq!(min_x(&state, WALL), further);
    assert!(state.doc.undo());
    assert_eq!(min_x(&state, WALL), before);
}

#[test]
fn cancelling_a_live_drag_puts_everything_back() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = top();
    state.doc.set_selection(&[WALL]);
    let before = min_x(&state, WALL);
    start_move(&mut tool, &mut state, &v, 100.0, Modifiers::NONE);
    assert_ne!(min_x(&state, WALL), before);
    assert!(tool.cancel(&mut state));
    assert!(!state.doc.in_transaction());
    assert_eq!(min_x(&state, WALL), before);
    assert_eq!(state.doc.history_len().0, 0);
    start_move(&mut tool, &mut state, &v, 100.0, Modifiers::NONE);
    tool.deactivate(&mut state);
    assert!(!state.doc.in_transaction());
    assert_eq!(min_x(&state, WALL), before);
}

#[test]
fn a_live_clone_does_not_burn_ids() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = top();
    state.doc.set_selection(&[WALL]);
    let (obj, side) = (state.doc.peek_next_object_id(), state.doc.peek_next_side_id());
    let at = start_move(&mut tool, &mut state, &v, 100.0, Modifiers::SHIFT);
    assert_eq!(state.doc.scene().solids().len(), 4, "the copy is in the map while dragging");
    for i in 1..=5 {
        tool.handle_event(&mut state, &v, PointerEvent::Drag(at + vec2(32.0 * i as f32, 0.0)), Modifiers::SHIFT);
        tool.publish_for_tests(&mut state);
    }
    assert!(tool.cancel(&mut state));
    assert_eq!(state.doc.scene().solids().len(), 3);
    assert_eq!((state.doc.peek_next_object_id(), state.doc.peek_next_side_id()), (obj, side));
    let at = start_move(&mut tool, &mut state, &v, 100.0, Modifiers::SHIFT);
    tool.handle_event(&mut state, &v, PointerEvent::DragEnd(at), Modifiers::SHIFT);
    assert_eq!(state.doc.scene().solids().len(), 4);
    assert_eq!(state.doc.history_len().0, 1);
    assert_eq!(state.doc.undo_label(), Some("Duplicate"));
}

#[test]
fn the_setting_and_the_budget_keep_the_outline_preview() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = top();
    state.doc.set_selection(&[WALL]);
    let before = min_x(&state, WALL);
    state.settings.selection.live_transform = false;
    start_move(&mut tool, &mut state, &v, 100.0, Modifiers::NONE);
    assert_eq!(min_x(&state, WALL), before);
    assert!(!state.doc.in_transaction());
    let mut out = Overlay::default();
    tool.overlay(&state, &view3d(), &mut out);
    assert!(!out.lines_on_top.is_empty(), "the outline preview");
    assert!(tool.cancel(&mut state));
    state.settings.selection.live_transform = true;
    state.settings.selection.live_transform_limit = 1;
    state.doc.set_selection(&[WALL, FLOOR]);
    start_move(&mut tool, &mut state, &v, 100.0, Modifiers::NONE);
    assert!(!state.doc.in_transaction(), "two objects are over a budget of one");
    assert!(tool.cancel(&mut state));
}

#[test]
fn an_action_during_a_live_drag_falls_back_to_the_outline() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = top();
    state.doc.set_selection(&[WALL]);
    let before = min_x(&state, WALL);
    let at = start_move(&mut tool, &mut state, &v, 100.0, Modifiers::NONE);
    assert!(state.doc.in_transaction());
    assert!(!tool.action(&mut state, Action::ToggleGrid));
    assert!(!state.doc.in_transaction());
    assert_eq!(min_x(&state, WALL), before);
    assert!(tool.is_dragging());
    tool.handle_event(&mut state, &v, PointerEvent::Drag(at + vec2(16.0, 0.0)), Modifiers::NONE);
    tool.publish_for_tests(&mut state);
    assert!(!state.doc.in_transaction(), "the rest of the drag stays an outline");
    tool.handle_event(&mut state, &v, PointerEvent::DragEnd(at + vec2(16.0, 0.0)), Modifiers::NONE);
    assert_eq!(state.doc.history_len().0, 1);
    assert!(min_x(&state, WALL) > before);
}

#[test]
fn a_modal_move_is_live_and_escape_restores() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    state.doc.set_selection(&[WALL]);
    let before = min_x(&state, WALL);
    let c = Pos2::new(400.0, 300.0);
    tool.begin_modal(&mut state, &v, ModalKind::Move, c);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(c + vec2(120.0, 0.0)), Modifiers::NONE);
    tool.publish_for_tests(&mut state);
    assert!(state.doc.in_transaction());
    assert_ne!(min_x(&state, WALL), before);
    assert!(tool.cancel(&mut state));
    assert_eq!(min_x(&state, WALL), before);
    assert!(!state.doc.in_transaction());
}

#[test]
fn clicking_the_selection_in_3d_cycles_the_gizmo() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let wall = scr(&v, DVec3::new(-240.0, 256.0, 30.0));
    tool.handle_event(&mut state, &v, PointerEvent::Click(wall), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    assert_eq!(tool.gizmo_mode(), GizmoMode::Move, "the first click selects");
    for mode in [GizmoMode::Rotate, GizmoMode::Scale, GizmoMode::Move, GizmoMode::Rotate] {
        tool.handle_event(&mut state, &v, PointerEvent::Click(wall), Modifiers::NONE);
        assert_eq!(tool.gizmo_mode(), mode);
        assert_eq!(state.doc.selected_objects(), vec![WALL], "the selection stays");
    }
    assert!(tool.status_hint(&state).is_some_and(|h| h.contains("Rotate")));
    tool.handle_event(&mut state, &v, PointerEvent::Click(wall), Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::DoubleClick(wall), Modifiers::NONE);
    assert_eq!(tool.gizmo_mode(), GizmoMode::Rotate, "a double click keeps the mode it had");
    let c = state.doc.object_bounds(WALL, &|e| state.entity_bounds(e)).expect("bounds").center();
    let g = GizmoLayout::new(&v, c).expect("gizmo");
    tool.handle_event(&mut state, &v, PointerEvent::Click(scr(&v, g.center)), Modifiers::NONE);
    assert_eq!(tool.gizmo_mode(), GizmoMode::Scale, "a plain click on the gizmo cycles too");
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    state.doc.set_selection(&[LIGHT]);
    tool.handle_event(&mut state, &v, PointerEvent::Click(wall), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL], "a click on another object still selects");
    assert_eq!(tool.gizmo_mode(), GizmoMode::Scale);
    state.doc.set_selection(&[WALL, FLOOR]);
    tool.handle_event(&mut state, &v, PointerEvent::Click(wall), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL], "a click on part of a selection picks that part");
    state.doc.clear_selection();
    tool.middle_click(&mut state, &v, light);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    tool.middle_click(&mut state, &v, light);
    assert_eq!(state.doc.selected_objects(), vec![WALL], "the middle click digs behind");
    assert_eq!(state.doc.history_len().0, 0);
    state.settings.selection.click_cycles_gizmo = false;
    state.doc.set_selection(&[WALL]);
    tool.handle_event(&mut state, &v, PointerEvent::Click(wall), Modifiers::NONE);
    assert_eq!(tool.gizmo_mode(), GizmoMode::Scale, "turned off: clicks only select");
}

#[test]
fn right_click_on_the_gizmo_cycles_it() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    state.doc.set_selection(&[WALL]);
    let c = state.doc.object_bounds(WALL, &|e| state.entity_bounds(e)).expect("bounds").center();
    let g = GizmoLayout::new(&v, c).expect("gizmo");
    let on_axis = scr(&v, g.center + DVec3::X * g.size * 0.6);
    assert!(tool.secondary_click(&mut state, &v, on_axis));
    assert_eq!(tool.gizmo_mode(), GizmoMode::Rotate);
    assert!(!tool.secondary_click(&mut state, &v, Pos2::new(3.0, 3.0)), "elsewhere: the context menu");
    assert!(!tool.secondary_click(&mut state, &top(), on_axis));
    state.settings.selection.click_cycles_gizmo = false;
    assert!(!tool.secondary_click(&mut state, &v, on_axis));
}

#[test]
fn cycle_actions_and_edit_modes() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    assert!(tool.action(&mut state, Action::CycleGizmoMode));
    assert_eq!(tool.gizmo_mode(), GizmoMode::Rotate);
    assert_eq!(tool.edit_mode(), EditMode::Gizmo);
    state.doc.set_selection(&[WALL]);
    assert!(!tool.stretch_handles(&state, &v).is_empty() || !state.settings.selection.stretch_handles);
    assert!(tool.action(&mut state, Action::CycleEditMode));
    assert_eq!(tool.edit_mode(), EditMode::None);
    assert!(!tool.gizmo_visible);
    assert!(tool.stretch_handles(&state, &v).is_empty(), "none: no 3D handles at all");
    assert!(tool.action(&mut state, Action::CycleEditMode));
    assert_eq!(tool.edit_mode(), EditMode::Box);
    assert!(tool.action(&mut state, Action::CycleEditMode));
    assert_eq!(tool.edit_mode(), EditMode::Gizmo);
    assert!(tool.set_option(&mut state, "edit_mode", "box"));
    assert_eq!(tool.edit_mode(), EditMode::Box);
    assert!(tool.option_values().iter().any(|o| o.key == "edit_mode" && o.value == "box"));
    assert!(tool.action(&mut state, Action::CycleGizmoMode), "cycling the gizmo shows it");
    assert_eq!(tool.edit_mode(), EditMode::Gizmo);
    assert!(!tool.set_option(&mut state, "edit_mode", "bogus"));
}
