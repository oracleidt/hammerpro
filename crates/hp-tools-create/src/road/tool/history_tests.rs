#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::tests_support::{info_2d, settle};

#[test]
fn points_undo_and_redo_without_touching_document_history() {
    let mut state = settle();
    state.tool = ToolId::Road;
    let mut tool = RoadTool::new();
    tool.activate(&mut state);
    let revision = state.doc.revision();
    tool.add_point(RoadPoint::new(0.0, 0.0, 0.0));
    tool.add_point(RoadPoint::new(800.0, 0.0, 0.0));
    tool.sync_preview(&mut state);
    assert!(Action::Undo.enabled(&state));
    assert!(tool.action(&mut state, Action::Undo));
    assert_eq!(tool.points.len(), 1);
    assert!(tool.action(&mut state, Action::Undo));
    assert!(tool.points.is_empty());
    assert!(!Action::Undo.enabled(&state));
    assert!(Action::Redo.enabled(&state));
    assert!(tool.action(&mut state, Action::Undo));
    assert_eq!(state.doc.revision(), revision);
    assert!(tool.action(&mut state, Action::Redo));
    assert!(tool.action(&mut state, Action::Redo));
    assert_eq!(tool.points.len(), 2);
    tool.action(&mut state, Action::Undo);
    tool.add_point(RoadPoint::new(900.0, 100.0, 64.0));
    tool.sync_preview(&mut state);
    assert!(!Action::Redo.enabled(&state));
    tool.cancel(&mut state);
    assert!(state.runtime.brush.road_history.is_none());
}

#[test]
fn endpoint_extrusion_and_multiframe_drag_are_one_local_undo_step() {
    for shift in [false, true] {
        let mut state = settle();
        let mut tool = RoadTool::new();
        tool.activate(&mut state);
        tool.add_point(RoadPoint::new(0.0, 0.0, 0.0));
        tool.add_point(RoadPoint::new(800.0, 0.0, 0.0));
        let original = tool.points.clone();
        let history_len = tool.undo.len();
        let view = info_2d(ViewKind::Top, 0.5);
        let from = view.world_to_screen(original[1].xyz()).unwrap();
        let modifiers = if shift { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
        let to = view.world_to_screen(DVec3::new(1056.0, 0.0, 0.0)).unwrap();
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(from, to).with_modifiers(modifiers));
        for x in [1088.0, 1120.0, 1152.0] {
            let at = view.world_to_screen(DVec3::new(x, 0.0, 0.0)).unwrap();
            tool.handle_mouse(
                &mut state,
                &view,
                &Mouse { pos: Some(at), press_origin: Some(from), dragging: true, modifiers, ..Default::default() },
            );
        }
        let to = view.world_to_screen(DVec3::new(1152.0, 0.0, 0.0)).unwrap();
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(from, to).with_modifiers(modifiers));
        let changed = tool.points.clone();
        assert_ne!(original, changed);
        assert_eq!(tool.undo.len(), history_len + 1);
        assert!(tool.action(&mut state, Action::Undo));
        assert_eq!(tool.points, original);
        assert!(tool.action(&mut state, Action::Redo));
        assert_eq!(tool.points, changed);
    }
}

#[test]
fn editing_keeps_point_undo_local_and_commits_one_document_step() {
    let mut state = settle();
    let mut tool = RoadTool::new();
    tool.add_point(RoadPoint::new(0.0, 0.0, 0.0));
    tool.add_point(RoadPoint::new(800.0, 0.0, 0.0));
    let made = tool.create(&mut state).unwrap();
    let original_revision = state.doc.revision();
    let mut editing = RoadTool::new();
    editing.activate(&mut state);
    assert!(editing.action(&mut state, Action::Undo));
    assert_eq!(state.doc.revision(), original_revision);
    editing.set_option(&mut state, "selected_point", "2");
    editing.set_option(&mut state, "point_z", "128");
    editing.action(&mut state, Action::Undo);
    assert_eq!(editing.points[1].z, 0.0);
    editing.action(&mut state, Action::Redo);
    assert_eq!(editing.points[1].z, 128.0);
    let changed = editing.create(&mut state).unwrap();
    assert!(state.runtime.brush.road_history.is_none());
    assert!(!editing.action(&mut state, Action::Undo));
    assert_eq!(read_road(&state.doc, changed.ids[0]).unwrap().1.points[1].z, 128.0);
    assert!(state.doc.undo());
    assert_eq!(read_road(&state.doc, made.ids[0]).unwrap().1.points[1].z, 0.0);
}

#[test]
fn an_empty_undone_trace_accepts_backspace_and_escape_safely() {
    let mut state = settle();
    let mut tool = RoadTool::new();
    tool.add_point(RoadPoint::new(0.0, 0.0, 0.0));
    tool.action(&mut state, Action::Undo);
    let ctx = egui::Context::default();
    for key in [Key::Backspace, Key::Escape] {
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| {
            tool.key_input(&mut state, ui.ctx());
        });
    }
    assert!(tool.points.is_empty());
    assert!(!tool.history_active);
    assert!(state.runtime.brush.road_history.is_none());
}
