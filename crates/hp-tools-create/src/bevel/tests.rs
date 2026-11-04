use egui::{Key, Modifiers};
use glam::DVec3;
use hp_app::state::AppState;
use hp_app::{Tool, ToolFlow, ToolId, ViewInfo, ViewKind};
use hp_editor::EdgeRef;
use hp_geom::BevelKind;
use hp_render::Overlay;

use super::{BevelTool, edge_at};
use crate::common::Mouse;
use crate::tests_support::{info_2d, info_3d, settle};

const CTRL: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };

fn wall_view() -> ViewInfo {
    info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0)
}

fn top_edge() -> EdgeRef {
    EdgeRef::new(3, DVec3::new(-256.0, 256.0, 128.0), DVec3::new(256.0, 256.0, 128.0))
}

fn press(tool: &mut BevelTool, state: &mut AppState, key: Key, modifiers: Modifiers) -> (bool, bool) {
    let ctx = egui::Context::default();
    let event = egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
    let input = egui::RawInput { events: vec![event], ..Default::default() };
    let mut handled = false;
    let mut left = false;
    let mut out = ctx.run_ui(input, |ui| {
        handled = tool.key_input(state, ui.ctx());
        left = ui.ctx().input(|i| !i.events.is_empty());
    });
    out.textures_delta.clear();
    (handled, left)
}

fn hover(tool: &mut BevelTool, state: &mut AppState, view: &ViewInfo, p: DVec3) -> egui::Pos2 {
    let s = view.world_to_screen(p).expect("on screen");
    tool.handle_mouse(state, view, &Mouse::hover(s), false);
    s
}

fn volume(state: &AppState) -> f64 {
    state.doc.scene().solids().values().map(|n| n.geom.volume()).sum()
}

#[test]
fn hover_ctrl_b_mouse_wheel_and_enter() {
    let mut state = settle();
    state.settings.edit.grid_size = 4.0;
    let mut tool = BevelTool::new();
    let view = wall_view();
    let at = hover(&mut tool, &mut state, &view, DVec3::new(10.0, 256.0, 129.0));
    let edge = tool.hovered_edge().expect("the top front edge of the wall is under the cursor");
    assert!(edge.same_edge(&top_edge()), "{edge:?}");
    let mut out = Overlay::default();
    tool.overlay(&state, &view, &mut out);
    assert!(!out.lines.is_empty() && !out.points.is_empty(), "the hovered edge is highlighted");
    let (handled, _) = press(&mut tool, &mut state, Key::B, CTRL);
    assert!(handled && tool.is_active(), "Ctrl+B starts the bevel on the hovered edge");
    assert_eq!(tool.limit(), Some(16.0), "the wall is 16 units thick");
    assert_eq!(tool.options.radius, 16.0);
    let before = tool.options.radius;
    tool.handle_mouse(&mut state, &view, &Mouse::hover(at + egui::vec2(0.0, 9.0)), false);
    let snapped = tool.options.radius;
    assert!(snapped < before, "moving the mouse down makes it smaller");
    assert!(snapped >= 4.0 && snapped % 4.0 == 0.0, "on the grid: {snapped}");
    tool.handle_mouse(&mut state, &view, &Mouse::hover(at + egui::vec2(0.0, 300.0)), false);
    assert_eq!(tool.options.radius, 4.0, "the grid step at the smallest");
    let alt = Mouse::hover(at + egui::vec2(0.0, 7.0)).with_modifiers(Modifiers::ALT);
    tool.handle_mouse(&mut state, &view, &alt, false);
    let free = tool.options.radius;
    assert!(free % 4.0 != 0.0 && free < 16.0 && free > 4.0, "Alt gives a free size: {free}");
    tool.handle_mouse(&mut state, &view, &Mouse::hover(at - egui::vec2(0.0, 400.0)), false);
    assert_eq!(tool.options.radius, 16.0, "clamped to what the faces allow");
    assert!(tool.set_radius(8.0));
    assert_eq!(tool.wheel(&mut state, &view, 1.0, Modifiers::NONE), ToolFlow::Handled);
    assert_eq!(tool.options.segments, 5);
    tool.wheel(&mut state, &view, -0.5, Modifiers::NONE);
    tool.wheel(&mut state, &view, -2.5, Modifiers::NONE);
    assert_eq!(tool.options.segments, 2);
    for _ in 0..10 {
        tool.wheel(&mut state, &view, -1.0, Modifiers::NONE);
    }
    assert_eq!(tool.options.segments, 1, "at least one segment");
    tool.options.segments = 4;
    assert_eq!(tool.preview_kinds(&state), vec![BevelKind::Round]);
    assert!(tool.preview_error(&state).is_none());
    assert_eq!(tool.preview_brushes(&state), 1);
    let (handled, _) = press(&mut tool, &mut state, Key::B, Modifiers::NONE);
    assert!(handled && tool.options.inverse, "B toggles the inverse bevel");
    assert_eq!(tool.preview_kinds(&state), vec![BevelKind::Cove]);
    press(&mut tool, &mut state, Key::B, CTRL);
    assert!(!tool.options.inverse, "Ctrl+B again toggles back");
    let (undo, _) = state.doc.history_len();
    let (handled, _) = press(&mut tool, &mut state, Key::Enter, Modifiers::NONE);
    assert!(handled && !tool.is_active(), "Enter applies");
    assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
    let wall = state.doc.scene().solid(3).expect("wall");
    assert!(wall.geom.valid);
    assert_eq!(wall.solid.sides.len(), 10);
    assert_eq!(state.doc.selected_solid_ids(), vec![3]);
    assert!(tool.picked().is_empty());
}

#[test]
fn escape_right_click_and_click() {
    let mut state = settle();
    let mut tool = BevelTool::new();
    let view = wall_view();
    let v0 = volume(&state);
    hover(&mut tool, &mut state, &view, DVec3::new(10.0, 256.0, 129.0));
    press(&mut tool, &mut state, Key::B, CTRL);
    assert!(tool.is_active());
    assert!(tool.takes_right_drag(ViewKind::Perspective, Modifiers::NONE));
    let (handled, _) = press(&mut tool, &mut state, Key::Escape, Modifiers::NONE);
    assert!(handled && !tool.is_active(), "Escape cancels");
    assert!(!tool.takes_right_drag(ViewKind::Perspective, Modifiers::NONE));
    let at = hover(&mut tool, &mut state, &view, DVec3::new(10.0, 256.0, 129.0));
    press(&mut tool, &mut state, Key::B, CTRL);
    tool.handle_mouse(&mut state, &view, &Mouse::hover(at), true);
    assert!(!tool.is_active(), "a right click cancels");
    assert_eq!(volume(&state), v0, "nothing changed");
    hover(&mut tool, &mut state, &view, DVec3::new(10.0, 256.0, 129.0));
    press(&mut tool, &mut state, Key::B, CTRL);
    tool.handle_mouse(&mut state, &view, &Mouse::click(at), false);
    assert!(!tool.is_active(), "a click applies");
    assert!(volume(&state) < v0);
    assert!(state.doc.undo());
    assert_eq!(volume(&state), v0);
}

#[test]
fn ctrl_b_away_from_edges_keeps_snap_to_grid() {
    let mut state = settle();
    let mut tool = BevelTool::new();
    let view = wall_view();
    hover(&mut tool, &mut state, &view, DVec3::new(0.0, 256.0, 64.0));
    assert!(tool.hovered_edge().is_none(), "the middle of a face is not an edge");
    let (handled, left) = press(&mut tool, &mut state, Key::B, CTRL);
    assert!(!handled && left, "the key goes on to the snap to grid action");
    assert!(!tool.is_active());
    tool.start(&mut state, Vec::new(), None);
    assert!(!tool.is_active());
}

#[test]
fn picking_several_edges_and_starting_from_the_panel() {
    let mut state = settle();
    let mut tool = BevelTool::new();
    let view = wall_view();
    let a = hover(&mut tool, &mut state, &view, DVec3::new(10.0, 256.0, 129.0));
    tool.handle_mouse(&mut state, &view, &Mouse::click(a), false);
    assert_eq!(tool.picked().len(), 1);
    let b = hover(&mut tool, &mut state, &view, DVec3::new(-256.0, 256.0, 60.0));
    tool.handle_mouse(&mut state, &view, &Mouse::click(b).with_modifiers(CTRL), false);
    assert_eq!(tool.picked().len(), 2, "Ctrl+click adds an edge");
    tool.handle_mouse(&mut state, &view, &Mouse::click(b).with_modifiers(CTRL), false);
    assert_eq!(tool.picked().len(), 1, "Ctrl+click again removes it");
    tool.handle_mouse(&mut state, &view, &Mouse::click(b).with_modifiers(CTRL), false);
    let edges = tool.picked().to_vec();
    assert!(tool.start(&mut state, edges, None));
    assert_eq!(tool.active_edges().len(), 2);
    tool.options.segments = 3;
    assert!(tool.set_radius(8.0));
    let ids = tool.apply(&mut state).expect("two edges");
    assert_eq!(ids, vec![3]);
    assert_eq!(state.doc.scene().solid(3).expect("wall").solid.sides.len(), 12);
    let empty = hover(&mut tool, &mut state, &view, DVec3::new(0.0, 256.0, 64.0));
    tool.handle_mouse(&mut state, &view, &Mouse::click(empty), false);
    assert!(tool.picked().is_empty(), "a click away from the edges clears them");
}

#[test]
fn double_click_inverse_fills_the_floor_corner() {
    let mut state = settle();
    let mut tool = BevelTool::new();
    let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, -10.0);
    let floor = state.doc.scene().solid(2).expect("floor").geom.volume();
    let wall = state.doc.scene().solid(3).expect("wall").geom.volume();
    let s = view.world_to_screen(DVec3::new(30.0, 256.0, 0.0)).expect("corner");
    tool.options.inverse = true;
    tool.handle_mouse(&mut state, &view, &Mouse::double_click(s), false);
    assert!(tool.is_active(), "a double click starts at once");
    assert_eq!(tool.preview_kinds(&state), vec![BevelKind::Fill], "the wall stands on the floor");
    assert!(tool.set_radius(24.0));
    tool.options.segments = 6;
    let ids = tool.apply(&mut state).expect("fill");
    assert!(ids.len() >= 2);
    assert_eq!(state.doc.scene().solid(2).expect("floor").geom.volume(), floor);
    assert_eq!(state.doc.scene().solid(3).expect("wall").geom.volume(), wall);
    let added: f64 =
        ids.iter().filter(|id| **id > 3).map(|id| state.doc.scene().solid(*id).expect("f").geom.volume()).sum();
    let n = 6.0;
    let area = 24.0 * 24.0 - n * 0.5 * 24.0 * 24.0 * (std::f64::consts::PI / (2.0 * n)).sin();
    assert!((added - area * 512.0).abs() < 1e-3, "{added}");
}

#[test]
fn end_on_edges_in_2d_views() {
    let mut state = settle();
    let side = info_2d(ViewKind::Front, 1.0);
    let s = side.world_to_screen(DVec3::new(0.0, 256.0, 128.0)).expect("corner");
    let e = edge_at(&state, &side, s + egui::vec2(3.0, -2.0), false).expect("the edge along X seen end-on");
    assert!(e.same_edge(&top_edge()), "{e:?}");
    let top = info_2d(ViewKind::Top, 1.0);
    let s = top.world_to_screen(DVec3::new(-256.0, 256.0, 0.0)).expect("corner");
    let e = edge_at(&state, &top, s, false).expect("a vertical edge seen from the top");
    assert!((e.b - e.a).normalize().z.abs() > 0.999);
    assert!(e.a.z.max(e.b.z) == 128.0, "the wall is on top of the floor: {e:?}");
    let mut tool = BevelTool::new();
    tool.handle_mouse(
        &mut state,
        &side,
        &Mouse::double_click(side.world_to_screen(DVec3::new(0.0, 256.0, 128.0)).expect("p")),
        false,
    );
    assert!(tool.is_active());
    let from = side.world_to_screen(DVec3::new(0.0, 256.0, 128.0)).expect("p");
    let up = Mouse::hover(from - egui::vec2(0.0, 8.0)).with_modifiers(Modifiers::ALT);
    tool.handle_mouse(&mut state, &side, &up, false);
    assert_eq!(tool.options.radius, 16.0, "8 pixels up at zoom 1 is 8 more units, clamped to the wall");
    let down = Mouse::hover(from + egui::vec2(0.0, 5.0)).with_modifiers(Modifiers::ALT);
    tool.handle_mouse(&mut state, &side, &down, false);
    assert_eq!(tool.options.radius, 11.0);
    let mut out = Overlay::default();
    tool.overlay(&state, &side, &mut out);
    assert!(!out.lines_on_top.is_empty(), "the preview shows in 2D views");
    assert!(tool.apply(&mut state).is_some());
}

#[test]
fn ctrl_b_handoff_from_another_tool() {
    let mut state = settle();
    let view = wall_view();
    let s = view.world_to_screen(DVec3::new(10.0, 256.0, 129.0)).expect("edge");
    state.runtime.brush.bevel_start = Some((view, s));
    let mut tool = BevelTool::new();
    tool.activate(&mut state);
    assert!(tool.is_active(), "the tool starts on the edge the other tool pointed at");
    assert!(state.runtime.brush.bevel_start.is_none());
    tool.deactivate(&mut state);
    assert!(!tool.is_active());
}

#[test]
fn options_overlay_paint_and_errors() {
    let mut state = settle();
    let mut tool = BevelTool::new();
    let view = wall_view();
    hover(&mut tool, &mut state, &view, DVec3::new(10.0, 256.0, 129.0));
    press(&mut tool, &mut state, Key::B, CTRL);
    let views = [view, info_2d(ViewKind::Top, 1.0), info_2d(ViewKind::Front, 1.0), info_2d(ViewKind::Side, 1.0)];
    for v in &views {
        let mut out = Overlay::default();
        tool.overlay(&state, v, &mut out);
        assert!(!out.lines.is_empty() || !out.lines_on_top.is_empty(), "{:?}", v.kind);
        assert!(tool.cursor(&state, v, v.rect.center()).is_some());
    }
    hp_app::testing::run_ctx(&mut state, |ctx, state| {
        for v in &views {
            tool.paint(state, v, &ctx.layer_painter(egui::LayerId::background()));
        }
    });
    hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
    assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
    for o in tool.option_values() {
        assert!(tool.set_option(&mut state, o.key, &o.value), "{} {}", o.key, o.value);
    }
    assert!(tool.set_option(&mut state, "segments", "12"));
    assert!(tool.set_option(&mut state, "inverse", "on"));
    assert!(tool.set_option(&mut state, "new_faces", "current"));
    assert!(tool.set_option(&mut state, "radius", "4"));
    assert!(!tool.set_option(&mut state, "radius", "0"));
    assert!(!tool.set_option(&mut state, "segments", "40"));
    assert!(!tool.set_option(&mut state, "inverse", "maybe"));
    assert!(!tool.set_option(&mut state, "colour", "red"));
    let p = tool.params(&state);
    assert_eq!((p.radius, p.segments, p.inverse), (4.0, 12, true));
    assert_eq!(p.material.as_deref(), Some("dev/dev_measuregeneric01b"));
    assert!(tool.cancel(&mut state));
    hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
    assert!(tool.status_hint(&state).is_some());
    assert!(!tool.cancel(&mut state));
    for issue in hp_geom::BevelIssue::ALL {
        let text = super::tool::issue_text(&issue.into());
        assert!(!text.is_empty() && text != issue.message(), "{issue:?} is translated");
    }
    let font = egui::FontId::proportional(14.0);
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, ToolId::Bevel.icon())), "the tool icon has a glyph");
}

#[test]
fn alt_click_picks_the_whole_edge_loop_and_ctrl_alt_the_ring() {
    let mut state = settle();
    let mut tool = BevelTool::new();
    let view = wall_view();
    let at = hover(&mut tool, &mut state, &view, DVec3::new(10.0, 256.0, 129.0));
    let alt = Modifiers::ALT;
    tool.handle_mouse(&mut state, &view, &Mouse::hover(at).with_modifiers(alt), false);
    let mut out = Overlay::default();
    tool.overlay(&state, &view, &mut out);
    let hovering = out.lines_on_top.len() + out.lines.len();
    tool.handle_mouse(&mut state, &view, &Mouse::click(at).with_modifiers(alt), false);
    let loop_len = tool.picked().len();
    assert!(loop_len >= 3, "a loop of the wall: {loop_len} edges");
    assert!(tool.picked().iter().any(|e| e.same_edge(&top_edge())), "the loop holds the clicked edge");
    assert!(hovering > 0, "the loop is drawn before the click");
    tool.handle_mouse(&mut state, &view, &Mouse::click(at), false);
    assert_eq!(tool.picked().len(), 1, "a plain click picks one edge again");
    let ctrl_alt = Modifiers { alt: true, ..CTRL };
    tool.handle_mouse(&mut state, &view, &Mouse::click(at).with_modifiers(ctrl_alt), false);
    assert!(tool.picked().len() >= 2, "the ring: {} edges", tool.picked().len());
    let shift_alt = Modifiers { alt: true, shift: true, ..Modifiers::default() };
    let before = tool.picked().len();
    tool.handle_mouse(&mut state, &view, &Mouse::click(at).with_modifiers(shift_alt), false);
    assert!(tool.picked().len() >= before, "Shift adds to the picked edges");
    assert!(tool.start_picked_or_hovered(&mut state), "the picked loop can be bevelled");
}
