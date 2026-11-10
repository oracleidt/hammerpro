use egui::{Event, Key, Modifiers, Pos2, Rect, vec2};
use glam::{DVec2, DVec3};
use hp_app::testing::{run_ui, test_state};
use hp_app::view::{Camera2d, Camera3d};
use hp_app::{Action, AppState, Tool, ToolFlow, ViewInfo, ViewKind};
use hp_editor::ObjRef;
use hp_render::Overlay;

use crate::gizmo::{GizmoLayout, GizmoMode, GizmoPart};
use crate::input::PointerEvent;
use crate::math::{Handle, HandleMode, handle_pos};
use crate::modal::{Lock, ModalKind};
use crate::select::SelectTool;

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

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
}

fn key_frame(ctx: &egui::Context, state: &mut AppState, tool: &mut SelectTool, events: Vec<Event>) -> bool {
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

fn wall_rect(v: &ViewInfo, state: &AppState) -> Rect {
    let b = bounds(state, WALL);
    Rect::from_two_pos(scr(v, b.min), scr(v, b.max))
}

#[test]
fn shift_clones_while_rotating_scaling_and_skewing() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    let solids = state.doc.scene().solids().len();
    for (mode, handle) in [
        (HandleMode::Rotate, Handle::new(1, 1)),
        (HandleMode::Scale, Handle::new(1, 0)),
        (HandleMode::Skew, Handle::new(0, 1)),
    ] {
        state.doc.set_selection(&[WALL]);
        tool.set_handle_mode(mode);
        let h = handle_pos(wall_rect(&v, &state), handle);
        let to = match mode {
            HandleMode::Rotate => scr(&v, DVec3::new(-200.0, 600.0, 0.0)),
            _ => h + vec2(40.0, 0.0),
        };
        let before = state.doc.history_len().0;
        drag(&mut tool, &mut state, &v, h, to, Modifiers::SHIFT);
        assert_eq!(state.doc.scene().solids().len(), solids + 1, "{mode:?}: one copy");
        assert_eq!(state.doc.undo_label(), Some("Duplicate"), "{mode:?}");
        assert_eq!(state.doc.history_len().0, before + 1);
        let b = bounds(&state, WALL);
        assert!(approx(b.min.x, -256.0) && approx(b.max.x, 256.0), "{mode:?}: {b:?}");
        let copy = state.doc.selected_objects();
        assert_eq!(copy.len(), 1);
        assert_ne!(copy[0], WALL);
        assert_ne!(bounds(&state, copy[0]), b, "{mode:?}: the copy is transformed");
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().solids().len(), solids);
    }
}

#[test]
fn clones_leave_their_visgroups_when_asked() {
    let mut state = state();
    let vis = state.doc.create_visgroup("Walls", None, &[WALL]).expect("visgroup");
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    let start = scr(&v, DVec3::new(0.0, 264.0, 0.0));
    for keep in [true, false] {
        state.settings.selection.clone_keeps_visgroups = keep;
        state.doc.set_selection(&[WALL]);
        drag(&mut tool, &mut state, &v, start, start + vec2(35.0, 0.0), Modifiers::SHIFT);
        let ObjRef::Solid(copy) = state.doc.selected_objects()[0] else { panic!("solid") };
        let ids = state.doc.scene().solid(copy).expect("copy").solid.editor.visgroup_ids.clone();
        assert_eq!(ids.contains(&vis), keep);
        assert!(state.doc.undo());
    }
}

#[test]
fn clicks_cycle_through_objects_behind_in_3d() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    assert_eq!(tool.cycle_len(), 2);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    let hint = tool.status_hint(&state).expect("hint");
    assert!(hint.contains("1/2"), "{hint}");
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL], "second click: the object behind");
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT], "and around again");
    let ctx = egui::Context::default();
    state.ui.hovered_view = Some(0);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::PageDown, Modifiers::NONE)]));
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::PageUp, Modifiers::NONE)]));
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    assert_eq!(state.doc.history_len().0, 0, "cycling is not an edit");
    state.frame.time += 5.0;
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::PageDown, Modifiers::NONE)]));
    assert_eq!(state.doc.undo_label(), Some("Nudge"));
    tool.middle_click(&mut state, &v, light);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
}

#[test]
fn middle_click_selects_and_cycles_in_2d() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Front);
    let at = scr(&v, DVec3::new(0.0, 0.0, 0.0));
    tool.middle_click(&mut state, &v, at);
    let first = state.doc.selected_objects();
    assert!(!first.is_empty());
    tool.middle_click(&mut state, &v, at);
    assert_ne!(state.doc.selected_objects(), first);
}

#[test]
fn ctrl_drag_paints_a_selection_in_3d() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let wall = scr(&v, DVec3::new(-150.0, 256.0, 30.0));
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    drag(&mut tool, &mut state, &v, wall, light, Modifiers::COMMAND);
    let sel = state.doc.selected_objects();
    assert!(sel.contains(&WALL) && sel.contains(&LIGHT), "{sel:?}");
    assert_eq!(state.doc.history_len().0, 0, "selection changes are not undo steps");
    tool.handle_event(
        &mut state,
        &v,
        PointerEvent::DragStart { origin: wall, pos: wall },
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    let hint = tool.status_hint(&state).expect("hint");
    assert!(!hint.contains("seltool."));
    tool.handle_event(&mut state, &v, PointerEvent::DragEnd(wall), Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
}

#[test]
fn rotations_turn_around_the_movable_pivot() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    state.pivot = Some(DVec3::new(-256.0, 264.0, 64.0));
    tool.set_handle_mode(HandleMode::Rotate);
    let h = handle_pos(wall_rect(&v, &state), Handle::new(1, 1));
    let to = scr(&v, DVec3::new(-256.0, 900.0, 0.0));
    drag(&mut tool, &mut state, &v, h, to, Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!(approx(b.size().y, 512.0), "{b:?}");
    assert!((b.min.y - 256.0).abs() < 16.0 + 1e-6 && b.max.x < -240.0, "turned around the pivot: {b:?}");
    assert_eq!(state.pivot, Some(DVec3::new(-256.0, 264.0, 64.0)), "the pivot stays");
    let start = scr(&v, b.center());
    drag(&mut tool, &mut state, &v, start, start + vec2(32.0, 0.0), Modifiers::NONE);
    let moved = bounds(&state, WALL).min - b.min;
    assert!(moved.x > 0.0);
    assert_eq!(state.pivot, Some(DVec3::new(-256.0, 264.0, 64.0) + moved));
    state.doc.set_selection(&[LIGHT]);
    let ctx = egui::Context::default();
    key_frame(&ctx, &mut state, &mut tool, Vec::new());
    assert_eq!(state.pivot, None);
}

#[test]
fn the_pivot_is_dragged_with_the_grid_in_2d() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    state.pivot = Some(DVec3::new(0.0, 0.0, 50.0));
    let p = scr(&v, DVec3::new(0.0, 0.0, 50.0));
    tool.handle_event(&mut state, &v, PointerEvent::Hover(p), Modifiers::NONE);
    assert_eq!(tool.cursor(&state, &v, p), Some(egui::CursorIcon::Grab));
    let to = scr(&v, DVec3::new(37.0, -21.0, 0.0));
    drag(&mut tool, &mut state, &v, p, to, Modifiers::NONE);
    assert_eq!(state.pivot, Some(DVec3::new(32.0, -16.0, 50.0)));
    assert!(approx(bounds(&state, WALL).min.x, -256.0), "the selection did not move");
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    let p = scr(&v, DVec3::new(32.0, -16.0, 0.0));
    let to = scr(&v, DVec3::new(37.0, -21.0, 0.0));
    drag(&mut tool, &mut state, &v, p, to, Modifiers::ALT);
    let piv = state.pivot.expect("pivot");
    assert!((piv - DVec3::new(37.0, -21.0, 50.0)).length() < 0.01, "{piv:?}");
    let v3 = view3d();
    state.pivot = Some(DVec3::new(-300.0, 256.0, 200.0));
    let p = scr(&v3, DVec3::new(-300.0, 256.0, 200.0));
    let to = scr(&v3, DVec3::new(-200.0, 256.0, 150.0));
    drag(&mut tool, &mut state, &v3, p, to, Modifiers::NONE);
    let piv = state.pivot.expect("pivot");
    assert!((piv - DVec3::new(-208.0, 256.0, 144.0)).length() < 1e-6, "{piv:?}");
    run_ui(&mut state, |ui, state| {
        for view in [view2d(ViewKind::Top), view2d(ViewKind::Front), view3d()] {
            tool.paint(state, &ViewInfo { rect: ui.max_rect(), ..view }, ui.painter());
        }
    });
}

#[test]
fn modal_move_follows_the_pointer_and_locks_axes() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let ctx = egui::Context::default();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    let start = scr(&v, DVec3::new(0.0, 0.0, 0.0));
    tool.begin_modal(&mut state, &v, ModalKind::Move, start);
    assert!(tool.is_modal());
    assert!(state.runtime.selection.modal_transform);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(start + vec2(37.0, -50.0)), Modifiers::NONE);
    let m = tool.modal().expect("modal");
    assert_eq!(m.delta, DVec3::new(80.0, 96.0, 0.0), "snapped to the grid");
    assert!(approx(bounds(&state, WALL).min.x, -256.0));
    let hint = tool.status_hint(&state).expect("hint");
    assert!(!hint.contains("seltool."), "{hint}");
    state.ui.hovered_view = Some(1);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::X, Modifiers::NONE)]));
    let m = tool.modal().expect("modal");
    assert_eq!(m.lock, Lock::Axis(0));
    assert_eq!(m.delta, DVec3::new(80.0, 0.0, 0.0));
    click(&mut tool, &mut state, &v, start + vec2(37.0, -50.0), Modifiers::NONE);
    assert!(!tool.is_modal());
    assert!(!state.runtime.selection.modal_transform);
    assert!(approx(bounds(&state, WALL).min.x, -176.0));
    assert_eq!(state.doc.undo_label(), Some("Move"));
    tool.begin_modal(&mut state, &v, ModalKind::Move, start);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(start + vec2(100.0, 0.0)), Modifiers::NONE);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::Escape, Modifiers::NONE)]));
    assert!(!tool.is_modal());
    assert!(approx(bounds(&state, WALL).min.x, -176.0));
    assert_eq!(state.doc.selected_objects(), vec![WALL], "Escape only cancelled the transform");
}

#[test]
fn modal_rotate_and_scale_apply_with_enter() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let ctx = egui::Context::default();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    let c = scr(&v, bounds(&state, WALL).center());
    tool.begin_modal(&mut state, &v, ModalKind::Rotate, c + vec2(100.0, 0.0));
    tool.handle_event(&mut state, &v, PointerEvent::Hover(c + vec2(0.0, -100.0)), Modifiers::NONE);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::Enter, Modifiers::NONE)]));
    let b = bounds(&state, WALL);
    assert!(approx(b.size().x, 16.0) && approx(b.size().y, 512.0), "{b:?}");
    assert_eq!(state.doc.undo_label(), Some("Rotate"));
    assert!(state.doc.undo());
    tool.begin_modal(&mut state, &v, ModalKind::Scale, c + vec2(100.0, 0.0));
    state.ui.hovered_view = Some(1);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::Z, Modifiers::SHIFT)]));
    tool.handle_event(&mut state, &v, PointerEvent::Hover(c + vec2(200.0, 0.0)), Modifiers::NONE);
    assert_eq!(tool.modal().expect("modal").scale, DVec3::new(2.0, 2.0, 1.0));
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::Enter, Modifiers::NONE)]));
    let b = bounds(&state, WALL);
    assert!(approx(b.size().x, 1024.0) && approx(b.size().y, 32.0) && approx(b.size().z, 128.0), "{b:?}");
    tool.begin_modal(&mut state, &v, ModalKind::Rotate, c);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::G, Modifiers::NONE)]));
    assert_eq!(tool.modal().expect("modal").kind, ModalKind::Move);
    assert!(tool.cancel(&mut state));
}

#[test]
fn g_key_starts_a_move_only_over_a_view() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let ctx = egui::Context::default();
    let v = view2d(ViewKind::Top);
    state.doc.set_selection(&[WALL]);
    tool.begin_modal(&mut state, &v, ModalKind::Move, Pos2::new(10.0, 10.0));
    assert!(tool.cancel(&mut state));
    let frame = |ctx: &egui::Context, state: &mut AppState, tool: &mut SelectTool, events: Vec<Event>| {
        let mut all = vec![Event::PointerMoved(Pos2::new(400.0, 300.0))];
        all.extend(events);
        key_frame(ctx, state, tool, all)
    };
    state.ui.hovered_view = None;
    assert!(!frame(&ctx, &mut state, &mut tool, vec![key(Key::G, Modifiers::NONE)]));
    assert!(!tool.is_modal());
    state.ui.hovered_view = Some(1);
    assert!(!frame(&ctx, &mut state, &mut tool, vec![key(Key::G, Modifiers::SHIFT)]));
    assert!(!tool.is_modal());
    assert!(frame(&ctx, &mut state, &mut tool, vec![key(Key::G, Modifiers::NONE)]));
    assert!(tool.is_modal());
    assert!(tool.cancel(&mut state));
}

#[test]
fn gizmo_scale_arm_stretches_one_axis() {
    let mut state = state();
    let mut tool = SelectTool::new();
    tool.set_gizmo_mode(GizmoMode::Scale);
    let v = view3d();
    state.doc.set_selection(&[WALL]);
    let c = bounds(&state, WALL).center();
    let g = GizmoLayout::new(&v, c).expect("gizmo");
    let tip = g.axis_segment(0).1;
    let p = scr(&v, tip);
    assert_eq!(g.hit(&v, p, GizmoMode::Scale), Some(GizmoPart::Axis(0)));
    let to = scr(&v, c + (tip - c) * 2.0);
    drag(&mut tool, &mut state, &v, p, to, Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!(approx(b.size().x, 1024.0) && approx(b.size().y, 16.0) && approx(b.size().z, 128.0), "{b:?}");
    assert!((b.center() - c).length() < 1e-6, "scaled about the center");
    assert_eq!(state.doc.undo_label(), Some("Transform"));
    let center = scr(&v, c);
    assert_eq!(
        GizmoLayout::new(&v, c).expect("gizmo").hit(&v, center + vec2(3.0, 3.0), GizmoMode::Scale),
        Some(GizmoPart::Center)
    );
    let solids = state.doc.scene().solids().len();
    drag(&mut tool, &mut state, &v, center + vec2(5.0, 5.0), center + vec2(10.0, 10.0), Modifiers::SHIFT);
    assert_eq!(state.doc.scene().solids().len(), solids + 1);
    assert_eq!(state.doc.undo_label(), Some("Duplicate"));
}

#[test]
fn local_gizmo_follows_the_entity_angles() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let start = ObjRef::Entity(4);
    state.doc.set_selection(&[start]);
    state.settings.selection.gizmo_local = true;
    let origin = DVec3::new(0.0, -128.0, 0.0);
    let axes = [DVec3::Y, DVec3::NEG_X, DVec3::Z];
    let g = GizmoLayout::with_axes(&v, origin, axes).expect("gizmo");
    let grab = origin + axes[0] * g.size * 0.6;
    let p = scr(&v, grab);
    tool.handle_event(&mut state, &v, PointerEvent::Hover(p), Modifiers::NONE);
    assert_eq!(tool.hovered_gizmo(), Some(GizmoPart::Axis(0)));
    let to = scr(&v, grab + DVec3::new(0.0, 100.0, 0.0));
    drag(&mut tool, &mut state, &v, p, to, Modifiers::NONE);
    let o = state.doc.scene().entity(4).and_then(|n| n.entity.origin()).expect("origin");
    assert!(approx(o.x, 0.0) && o.y > -128.0 && approx(o.z, 0.0), "{o:?}");
    assert!(approx(o.y.rem_euclid(16.0), 0.0), "on the grid: {o:?}");
}

#[test]
fn alt_wheel_scales_selected_props() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let kv = |k: &str, val: &str| (k.to_owned(), val.to_owned());
    let prop = state
        .doc
        .create_point_entity(
            "prop_dynamic",
            DVec3::new(64.0, 0.0, 0.0),
            &[kv("model", "models/a.mdl"), kv("modelscale", "0")],
        )
        .expect("prop");
    let stat = state.doc.create_point_entity("prop_static", DVec3::new(128.0, 0.0, 0.0), &[]).expect("static");
    let steps = state.doc.history_len().0;
    state.doc.set_selection(&[ObjRef::Entity(prop)]);
    assert_eq!(tool.wheel(&mut state, &v, 1.0, Modifiers::NONE), ToolFlow::Ignored);
    assert_eq!(tool.wheel(&mut state, &v, 1.0, Modifiers::ALT), ToolFlow::Handled);
    assert_eq!(tool.wheel(&mut state, &v, 1.0, Modifiers::ALT), ToolFlow::Handled);
    let scale = state.doc.scene().entity(prop).and_then(|n| n.entity.get("modelscale").map(str::to_owned));
    assert_eq!(scale.as_deref(), Some("1.21"), "0 counts as 1, two notches of 10 %");
    assert_eq!(state.doc.history_len().0, steps + 1, "one merged undo step");
    assert_eq!(state.doc.undo_label(), Some("Scale models"));
    assert!(tool.status_hint(&state).is_some_and(|h| h.contains("1.21")));
    state.doc.set_selection(&[ObjRef::Entity(stat)]);
    assert_eq!(tool.wheel(&mut state, &v, 1.0, Modifiers::ALT), ToolFlow::Handled);
    assert!(state.doc.scene().entity(stat).is_some_and(|n| n.entity.get("uniformscale").is_none()));
    state.doc.set_selection(&[WALL]);
    assert_eq!(tool.wheel(&mut state, &v, 1.0, Modifiers::ALT), ToolFlow::Ignored);
}

#[test]
fn context_menu_offers_floor_ceiling_and_pivot() {
    let _lang = hp_app::testing::lang_lock();
    let mut state = state();
    let mut tool = SelectTool::new();
    assert!(tool.context_entries(&state).is_empty(), "nothing selected");
    state.doc.set_selection(&[LIGHT]);
    let entries = tool.context_entries(&state);
    assert!(entries.len() >= 4, "{entries:?}");
    assert!(entries.iter().all(|e| !e.label.starts_with("action.")));
    let floor = entries.iter().find(|e| e.label == Action::MoveToFloor.label()).expect("floor entry");
    tool.context_command(&mut state, floor.id);
    assert!(state.requested_actions.contains(&Action::MoveToFloor));
    assert!(!entries.iter().any(|e| e.label == Action::CenterOrigins.label()));
    state.doc.set_selection(&[ObjRef::Entity(6)]);
    assert!(tool.context_entries(&state).iter().any(|e| e.label == Action::CenterOrigins.label()));
}

#[test]
fn centers_only_selection() {
    let mut state = state();
    state.settings.selection.box_confirm = false;
    let mut tool = SelectTool::new();
    tool.set_centers_only(true);
    let v = view2d(ViewKind::Top);
    click(&mut tool, &mut state, &v, scr(&v, DVec3::new(-256.0, 0.0, 0.0)), Modifiers::NONE);
    assert!(state.doc.selected_objects().is_empty());
    click(&mut tool, &mut state, &v, scr(&v, DVec3::new(0.0, 0.0, 0.0)), Modifiers::NONE);
    assert!(!state.doc.selected_objects().is_empty());
    state.doc.clear_selection();
    let a = scr(&v, DVec3::new(32.0, 240.0, 0.0));
    let b = scr(&v, DVec3::new(300.0, 290.0, 0.0));
    drag(&mut tool, &mut state, &v, a, b, Modifiers::NONE);
    assert!(state.doc.selected_objects().is_empty(), "{:?}", state.doc.selected_objects());
    let a = scr(&v, DVec3::new(-32.0, 240.0, 0.0));
    drag(&mut tool, &mut state, &v, a, b, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    run_ui(&mut state, |ui, state| tool.paint(state, &ViewInfo { rect: ui.max_rect(), ..v }, ui.painter()));
}

#[test]
fn mixed_selections_snap_by_their_brushes() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.doc.translate(&[LIGHT], DVec3::new(3.0, 5.0, 0.0)).expect("move");
    state.doc.set_selection(&[WALL, LIGHT]);
    let start = scr(&v, DVec3::new(0.0, 264.0, 0.0));
    drag(&mut tool, &mut state, &v, start, start + vec2(35.0, 0.0), Modifiers::NONE);
    let b = bounds(&state, WALL);
    assert!(approx(b.min.x.rem_euclid(16.0), 0.0), "the wall stays on the grid: {b:?}");
}

#[test]
fn shift_dragged_path_nodes_link_themselves() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    let kv = |k: &str, val: &str| (k.to_owned(), val.to_owned());
    let a = state
        .doc
        .create_point_entity(
            "path_track",
            DVec3::new(-128.0, -200.0, 0.0),
            &[kv("targetname", "track01"), kv("target", "track02")],
        )
        .expect("a");
    let _b = state
        .doc
        .create_point_entity("path_track", DVec3::new(128.0, -200.0, 0.0), &[kv("targetname", "track02")])
        .expect("b");
    state.doc.set_selection(&[ObjRef::Entity(a)]);
    let p = scr(&v, DVec3::new(-128.0, -200.0, 0.0));
    drag(&mut tool, &mut state, &v, p, p + vec2(0.0, -40.0), Modifiers::SHIFT);
    let [ObjRef::Entity(copy)] = state.doc.selected_objects()[..] else { panic!("one copy") };
    let get =
        |state: &AppState, id, k: &str| state.doc.scene().entity(id).and_then(|n| n.entity.get(k).map(str::to_owned));
    assert_eq!(get(&state, copy, "targetname").as_deref(), Some("track03"));
    assert_eq!(get(&state, copy, "target").as_deref(), Some("track02"));
    assert_eq!(get(&state, a, "target").as_deref(), Some("track03"));
    assert_eq!(state.doc.undo_label(), Some("Duplicate"));
    assert!(state.doc.undo());
    assert_eq!(get(&state, a, "target").as_deref(), Some("track02"));
}

#[test]
fn shift_arrow_clones_of_path_nodes_link_themselves_too() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let ctx = egui::Context::default();
    let kv = |k: &str, val: &str| (k.to_owned(), val.to_owned());
    let a = state
        .doc
        .create_point_entity(
            "path_track",
            DVec3::new(-128.0, -200.0, 0.0),
            &[kv("targetname", "p1"), kv("target", "p2")],
        )
        .expect("a");
    state.doc.create_point_entity("path_track", DVec3::new(128.0, -200.0, 0.0), &[kv("targetname", "p2")]).expect("b");
    state.doc.set_selection(&[ObjRef::Entity(a)]);
    state.ui.hovered_view = Some(1);
    state.settings.selection.shift_arrow_clones = true;
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowRight, Modifiers::SHIFT)]));
    let [ObjRef::Entity(copy)] = state.doc.selected_objects()[..] else { panic!("one copy") };
    assert_ne!(copy, a);
    let get =
        |state: &AppState, id, k: &str| state.doc.scene().entity(id).and_then(|n| n.entity.get(k).map(str::to_owned));
    assert_eq!(get(&state, copy, "targetname").as_deref(), Some("p3"));
    assert_eq!(get(&state, copy, "target").as_deref(), Some("p2"));
    assert_eq!(get(&state, a, "target").as_deref(), Some("p3"));
    assert_eq!(state.doc.undo_label(), Some("Duplicate"));
    assert!(state.doc.undo());
    assert_eq!(get(&state, a, "target").as_deref(), Some("p2"));
    assert!(state.doc.scene().entity(copy).is_none(), "one undo step");
}

#[test]
fn arrows_only_nudge_in_the_view_under_the_pointer() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let ctx = egui::Context::default();
    state.doc.set_selection(&[WALL]);
    state.ui.hovered_view = None;
    state.ui.focused_view = Some(1);
    assert!(!key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowRight, Modifiers::NONE)]));
    assert!(approx(bounds(&state, WALL).min.x, -256.0));
    state.ui.hovered_view = Some(1);
    assert!(key_frame(&ctx, &mut state, &mut tool, vec![key(Key::ArrowRight, Modifiers::NONE)]));
    assert!(approx(bounds(&state, WALL).min.x, -240.0));
}

#[test]
fn rotation_step_and_free_rotation_key_come_from_the_settings() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    state.settings.selection.rotation_snap_deg = 45.0;
    state.doc.set_selection(&[FLOOR]);
    tool.set_handle_mode(HandleMode::Rotate);
    let b = bounds(&state, FLOOR);
    let rect = Rect::from_two_pos(scr(&v, b.min), scr(&v, b.max));
    let h = handle_pos(rect, Handle::new(1, 1));
    let c = scr(&v, b.center());
    let dir = (h - c).normalized();
    let rot = |d: egui::Vec2, deg: f32| {
        let (s, co) = (-deg.to_radians()).sin_cos();
        egui::vec2(d.x * co - d.y * s, d.x * s + d.y * co)
    };
    let to = c + rot(dir, 30.0) * (h - c).length();
    tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: h, pos: h }, Modifiers::NONE);
    tool.handle_event(&mut state, &v, PointerEvent::Drag(to), Modifiers::NONE);
    let m = tool.drag_matrix().expect("drag");
    let turned = m.transform_vector3(DVec3::X);
    assert!((turned.y.atan2(turned.x).to_degrees() - 45.0).abs() < 1e-6, "{turned:?}");
    state.settings.selection.free_rotation_with_alt = true;
    tool.handle_event(&mut state, &v, PointerEvent::Drag(to), Modifiers::ALT);
    let m = tool.drag_matrix().expect("drag");
    let turned = m.transform_vector3(DVec3::X);
    assert!((turned.y.atan2(turned.x).to_degrees() - 30.0).abs() < 0.5, "{turned:?}");
    assert!(tool.cancel(&mut state));
    state.settings.selection.handle_size = 14.0;
    run_ui(&mut state, |ui, state| tool.paint(state, &ViewInfo { rect: ui.max_rect(), ..v }, ui.painter()));
    let mut out = Overlay::default();
    tool.overlay(&state, &v, &mut out);
}

const FGD: &str = r#"
@PointClass axis(hingeaxis) = phys_hinge : "Hinge"
[
    hingeaxis(axis) : "Hinge axis" : ""
]
@PointClass = prop_static : "Static prop"
[
    model(studio) : "Model"
    uniformscale(float) : "Scale" : "1"
]
"#;

struct FgdAssets(hp_app::testing::MockAssets, hp_fgd::ResolvedFgd);

impl hp_app::AssetsApi for FgdAssets {
    fn status(&self) -> hp_app::state::AssetStatus {
        self.0.status()
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        None
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        Some(&self.1)
    }
    fn generation(&self) -> u64 {
        self.0.generation()
    }
    fn remount(&mut self, settings: &hp_app::settings::Settings) {
        self.0.remount(settings);
    }
    fn reload_fgd(&mut self, settings: &hp_app::settings::Settings) {
        self.0.reload_fgd(settings);
    }
    fn materials(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.0.materials()
    }
    fn models(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.0.models()
    }
    fn sounds(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.0.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.0.source_label(source)
    }
    fn material_info(&mut self, name: &str) -> Option<std::sync::Arc<hp_app::state::MaterialInfo>> {
        self.0.material_info(name)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> hp_app::state::ThumbState {
        self.0.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> hp_app::state::ModelState {
        self.0.model_info(path)
    }
    fn model_preview(
        &mut self,
        gfx: &mut hp_app::Gfx,
        path: &str,
        camera: &hp_app::state::PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.0.model_preview(gfx, path, camera, size_px)
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        self.0.entity_bounds(entity)
    }
    fn invalidate_material(&mut self, name: &str) {
        self.0.invalidate_material(name);
    }
    fn import_images(
        &mut self,
        files: &[std::path::PathBuf],
        opts: &hp_app::state::ImportOptions,
    ) -> Vec<Result<String, String>> {
        self.0.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut hp_app::Gfx>) {
        self.0.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut hp_app::Gfx, opts: &hp_app::state::SceneOptions) {
        self.0.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.0.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        self.0.status_line()
    }
}

fn state_with_fgd() -> AppState {
    let mut s = state();
    let fgd = hp_fgd::Fgd::parse(FGD, None).resolve();
    s.assets = Box::new(FgdAssets(hp_app::testing::MockAssets::default(), fgd));
    s
}

#[test]
fn helper_points_are_dragged_like_handles() {
    let mut state = state_with_fgd();
    let mut tool = SelectTool::new();
    let v = view2d(ViewKind::Top);
    let kv = |k: &str, val: &str| (k.to_owned(), val.to_owned());
    let hinge = state
        .doc
        .create_point_entity(
            "phys_hinge",
            DVec3::new(-100.0, -100.0, 0.0),
            &[kv("hingeaxis", "-100 -100 0, -100 -100 64")],
        )
        .expect("hinge");
    state.doc.set_selection(&[ObjRef::Entity(hinge)]);
    let steps = state.doc.history_len().0;
    let p = scr(&v, DVec3::new(-100.0, -100.0, 0.0));
    tool.handle_event(&mut state, &v, PointerEvent::Hover(p), Modifiers::NONE);
    assert!(tool.status_hint(&state).is_some_and(|h| h.contains("hingeaxis")));
    assert_eq!(tool.cursor(&state, &v, p), Some(egui::CursorIcon::Grab));
    let to = scr(&v, DVec3::new(-37.0, -100.0, 0.0));
    drag(&mut tool, &mut state, &v, p, to, Modifiers::NONE);
    let value = state.doc.scene().entity(hinge).and_then(|n| n.entity.get("hingeaxis").map(str::to_owned));
    let points = crate::handles::parse_axis(value.as_deref().unwrap_or("")).expect("axis");
    assert!(points.contains(&DVec3::new(-100.0, -100.0, 0.0)) || points.contains(&DVec3::new(-100.0, -100.0, 64.0)));
    assert!(points.iter().any(|p| approx(p.x, -32.0)), "{points:?}");
    assert_eq!(state.doc.history_len().0, steps + 1);
    assert_eq!(state.doc.scene().entity(hinge).and_then(|n| n.entity.origin()), Some(DVec3::new(-100.0, -100.0, 0.0)));
    let v3 = view3d();
    run_ui(&mut state, |ui, state| {
        for view in [v, v3] {
            tool.paint(state, &ViewInfo { rect: ui.max_rect(), ..view }, ui.painter());
        }
    });
}

#[test]
fn static_props_scale_through_uniformscale_with_the_fgd() {
    let mut state = state_with_fgd();
    let mut tool = SelectTool::new();
    let stat = state.doc.create_point_entity("prop_static", DVec3::new(128.0, 0.0, 0.0), &[]).expect("static");
    state.doc.set_selection(&[ObjRef::Entity(stat)]);
    assert_eq!(tool.wheel(&mut state, &view3d(), -1.0, Modifiers::ALT), ToolFlow::Handled);
    let v = state.doc.scene().entity(stat).and_then(|n| n.entity.get("uniformscale").map(str::to_owned));
    assert_eq!(v.as_deref(), Some("0.909"));
}

#[test]
fn camera_tool_manages_the_map_cameras() {
    use crate::camera::{CameraTool, store_live, view_of};
    let mut state = state();
    let mut cam_tool = CameraTool::new();
    let ctx = egui::Context::default();
    let v = view2d(ViewKind::Top);
    state.cameras.cam3d = Camera3d { pos: DVec3::new(0.0, -500.0, 64.0), yaw_deg: 90.0, ..Camera3d::default() };
    assert!(state.doc.scene().cameras().cameras.is_empty());
    let from = scr(&v, DVec3::new(300.0, 300.0, 0.0));
    let to = scr(&v, DVec3::new(300.0, 500.0, 0.0));
    cam_tool.handle_event(&mut state, &v, PointerEvent::DragStart { origin: from, pos: from }, Modifiers::SHIFT);
    cam_tool.handle_event(&mut state, &v, PointerEvent::Drag(to), Modifiers::SHIFT);
    cam_tool.handle_event(&mut state, &v, PointerEvent::DragEnd(to), Modifiers::SHIFT);
    let cams = state.doc.scene().cameras().clone();
    assert_eq!(cams.cameras.len(), 2, "{cams:?}");
    assert_eq!(cams.active, 1);
    assert!((state.cameras.cam3d.pos.truncate() - glam::DVec2::new(300.0, 300.0)).length() < 1e-6);
    assert!((cams.cameras[0].position - DVec3::new(0.0, -500.0, 64.0)).length() < 1e-6, "the old view is kept");
    state.ui.hovered_view = Some(1);
    let frame = |ctx: &egui::Context, state: &mut AppState, t: &mut CameraTool, k: Key| {
        let input = egui::RawInput { events: vec![key(k, Modifiers::NONE)], ..Default::default() };
        let mut handled = false;
        let mut out = ctx.run_ui(input, |ui| handled = t.key_input(state, ui.ctx()));
        out.textures_delta.clear();
        handled
    };
    assert!(frame(&ctx, &mut state, &mut cam_tool, Key::PageDown));
    assert_eq!(state.doc.scene().cameras().active, 0);
    assert!((state.cameras.cam3d.pos - DVec3::new(0.0, -500.0, 64.0)).length() < 1e-6);
    assert!(frame(&ctx, &mut state, &mut cam_tool, Key::Delete));
    assert_eq!(state.doc.scene().cameras().cameras.len(), 1);
    assert!((state.cameras.cam3d.pos.truncate() - glam::DVec2::new(300.0, 300.0)).length() < 1e-6);
    assert!(frame(&ctx, &mut state, &mut cam_tool, Key::Delete));
    assert_eq!(state.doc.scene().cameras().cameras.len(), 1, "a default camera replaces the last one");
    assert!(!state.doc.is_dirty(), "cameras do not dirty the map, like Hammer");
    let live = Camera3d { pos: DVec3::new(1.0, 2.0, 3.0), ..Camera3d::default() };
    let stored = store_live(&hp_vmf::Cameras::default(), &live);
    assert_eq!((stored.active, stored.cameras.len()), (0, 1));
    assert!(view_of(&hp_vmf::Camera { position: DVec3::ZERO, look: DVec3::ZERO }, &live).is_none());
    run_ui(&mut state, |ui, state| {
        cam_tool.options_ui(ui, state);
        cam_tool.paint(state, &ViewInfo { rect: ui.max_rect(), ..v }, ui.painter());
    });
}

#[test]
fn camera_tool_click_to_zoom() {
    use crate::camera::CameraTool;
    let mut state = state();
    let mut cam_tool = CameraTool::new();
    let v = view2d(ViewKind::Top);
    let zoom = state.cameras.cam2d(ViewKind::Top).zoom;
    cam_tool.handle_event(&mut state, &v, PointerEvent::Click(Pos2::new(100.0, 100.0)), Modifiers::NONE);
    assert_eq!(state.cameras.cam2d(ViewKind::Top).zoom, zoom);
    state.settings.selection.click_to_zoom = true;
    cam_tool.handle_event(&mut state, &v, PointerEvent::Click(Pos2::new(100.0, 100.0)), Modifiers::NONE);
    let zin = state.cameras.cam2d(ViewKind::Top).zoom;
    assert!(zin > zoom);
    cam_tool.handle_event(&mut state, &v, PointerEvent::Click(Pos2::new(100.0, 100.0)), Modifiers::COMMAND);
    assert!(state.cameras.cam2d(ViewKind::Top).zoom < zin);
    assert!(cam_tool.status_hint(&state).is_some_and(|h| !h.contains("camtool.")));
}

#[test]
fn camera_tool_clicks_select_in_3d() {
    use crate::camera::CameraTool;
    let mut state = state();
    let mut cam = CameraTool::new();
    let v = view3d();
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    let wall = scr(&v, DVec3::new(-150.0, 256.0, 30.0));
    let ev = |cam: &mut CameraTool, state: &mut AppState, e: PointerEvent, m: Modifiers| {
        assert_eq!(cam.handle_event(state, &v, e, m), ToolFlow::Handled);
    };
    ev(&mut cam, &mut state, PointerEvent::Click(light), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    ev(&mut cam, &mut state, PointerEvent::Click(light), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL], "clicking again: the object behind");
    ev(&mut cam, &mut state, PointerEvent::DoubleClick(light), Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    ev(&mut cam, &mut state, PointerEvent::Click(wall), Modifiers::CTRL);
    let sel = state.doc.selected_objects();
    assert!(sel.contains(&WALL) && sel.contains(&LIGHT), "{sel:?}");
    ev(&mut cam, &mut state, PointerEvent::Click(light), Modifiers::COMMAND);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    let sky = Pos2::new(5.0, 5.0);
    ev(&mut cam, &mut state, PointerEvent::Click(sky), Modifiers::CTRL);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    ev(&mut cam, &mut state, PointerEvent::Click(sky), Modifiers::NONE);
    assert!(state.doc.selected_objects().is_empty());
    let yaw = state.cameras.cam3d.yaw_deg;
    ev(&mut cam, &mut state, PointerEvent::DragStart { origin: light, pos: light }, Modifiers::NONE);
    ev(&mut cam, &mut state, PointerEvent::Drag(light + vec2(40.0, 0.0)), Modifiers::NONE);
    ev(&mut cam, &mut state, PointerEvent::DragEnd(light + vec2(60.0, 0.0)), Modifiers::NONE);
    assert_ne!(state.cameras.cam3d.yaw_deg, yaw, "the drag looked around");
    assert!(state.doc.selected_objects().is_empty(), "a camera drag does not select");
    assert_eq!(state.doc.history_len().0, 0, "selecting is not an edit");
    let top = view2d(ViewKind::Top);
    cam.handle_event(&mut state, &top, PointerEvent::Click(Pos2::new(400.0, 300.0)), Modifiers::NONE);
    assert!(state.doc.selected_objects().is_empty());
}

#[test]
fn clicks_while_flying_select_at_the_crosshair() {
    use crate::camera::CameraTool;
    let mut state = state();
    let v = view3d();
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    let wall = scr(&v, DVec3::new(-150.0, 256.0, 30.0));
    let mut sel = SelectTool::new();
    assert_eq!(sel.click_while_flying(&mut state, &v, wall, Modifiers::NONE), ToolFlow::Handled);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    assert_eq!(sel.click_while_flying(&mut state, &v, light, Modifiers::CTRL), ToolFlow::Handled);
    let s = state.doc.selected_objects();
    assert!(s.contains(&WALL) && s.contains(&LIGHT), "{s:?}");
    state.doc.select(&[LIGHT], hp_editor::SelectOp::Replace);
    let mut blocked = SelectTool::new();
    click(&mut blocked, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(blocked.cycle_len(), 0, "the gizmo takes an ordinary click");
    let mut flying = SelectTool::new();
    flying.click_while_flying(&mut state, &v, light, Modifiers::NONE);
    assert_eq!(flying.cycle_len(), 2);
    flying.click_while_flying(&mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
    let mut cam = CameraTool::new();
    state.doc.clear_selection();
    assert_eq!(cam.click_while_flying(&mut state, &v, light, Modifiers::NONE), ToolFlow::Handled);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    assert_eq!(cam.click_while_flying(&mut state, &v, wall, Modifiers::CTRL), ToolFlow::Handled);
    let s = state.doc.selected_objects();
    assert!(s.contains(&WALL) && s.contains(&LIGHT), "{s:?}");
    assert_eq!(cam.click_while_flying(&mut state, &v, wall, Modifiers::CTRL), ToolFlow::Handled);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    cam.click_while_flying(&mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    cam.click_while_flying(&mut state, &v, light, Modifiers::CTRL);
    assert!(state.doc.selected_objects().is_empty(), "Ctrl + click removes the clicked object");
    cam.click_while_flying(&mut state, &v, light, Modifiers::CTRL);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT], "and adds it back");
    cam.click_while_flying(&mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    cam.click_while_flying(&mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL], "plain clicks still cycle");
    let top = view2d(ViewKind::Top);
    assert_eq!(cam.click_while_flying(&mut state, &top, light, Modifiers::NONE), ToolFlow::Ignored);
    assert_eq!(sel.click_while_flying(&mut state, &top, light, Modifiers::NONE), ToolFlow::Ignored);
}

fn block(state: &mut AppState, min: DVec3, max: DVec3, material: &str) -> ObjRef {
    let params = hp_geom::PrimitiveParams {
        bounds: hp_geom::Aabb::new(min, max),
        material: material.to_owned(),
        ..hp_geom::PrimitiveParams::default()
    };
    let mut side_id = 9000u32;
    let pieces = hp_geom::make_primitive(&params, &mut || {
        side_id += 1;
        side_id
    })
    .expect("block");
    let ids = state.doc.create_solids(pieces, hp_editor::Owner::World).expect("create");
    ObjRef::Solid(ids[0])
}

#[test]
fn a_later_click_on_the_same_spot_keeps_the_front_object() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);
    state.doc.clear_selection();
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT], "the selection changed: no cycling to the wall behind");
    state.frame.time += 3.0;
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT], "the cycle ended: still the front object");
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL], "a quick second click still digs behind");
    state.doc.select(&[FLOOR], hp_editor::SelectOp::Replace);
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT]);

    let top = view2d(ViewKind::Top);
    let at = scr(&top, DVec3::new(0.0, -8.0, 96.0));
    click(&mut tool, &mut state, &top, at, Modifiers::NONE);
    let first = state.doc.selected_objects();
    state.doc.clear_selection();
    click(&mut tool, &mut state, &top, at, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), first, "same rule in the 2D views");
}

#[test]
fn hidden_tool_brushes_do_not_catch_clicks() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let clip =
        block(&mut state, DVec3::new(-64.0, -400.0, 0.0), DVec3::new(64.0, -384.0, 200.0), "tools/toolsplayerclip");
    state.doc.clear_selection();
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![clip], "a shown clip brush is in front");
    state.frame.time += 3.0;
    state.settings.render.hide_clips = true;
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT], "hidden clips are not picked in 3D");
    state.settings.render.hide_clips = false;
    state.settings.view.show_tool_textures = false;
    state.frame.time += 3.0;
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT], "tool textures off: same");
    state.settings.view.show_tool_textures = true;
    state.frame.time += 3.0;
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![clip]);
    let top = view2d(ViewKind::Top);
    state.settings.render.hide_clips = true;
    state.doc.clear_selection();
    let edge = scr(&top, DVec3::new(-64.0, -392.0, 0.0));
    click(&mut tool, &mut state, &top, edge, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![clip], "the 2D views still draw and pick tool brushes");
}

#[test]
fn hidden_displacements_and_disp_only_follow_the_view() {
    let mut state = state();
    let mut tool = SelectTool::new();
    let v = view3d();
    let light = scr(&v, DVec3::new(0.0, -8.0, 96.0));
    state.settings.render.displacements_only = true;
    click(&mut tool, &mut state, &v, light, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![LIGHT], "entities stay pickable");
    let wall = scr(&v, DVec3::new(-150.0, 256.0, 30.0));
    state.frame.time += 3.0;
    click(&mut tool, &mut state, &v, wall, Modifiers::NONE);
    assert!(!state.doc.selected_objects().contains(&WALL), "brushes are hidden in the 3D view");
    state.settings.render.displacements_only = false;
    state.frame.time += 3.0;
    click(&mut tool, &mut state, &v, wall, Modifiers::NONE);
    assert_eq!(state.doc.selected_objects(), vec![WALL]);
}
