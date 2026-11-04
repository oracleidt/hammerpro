use super::*;
use crate::vertex::weld::{self, WeldTarget};

fn box_and_tool() -> (AppState, VertexTool, hp_editor::Id) {
    let mut state = state_with(&[]);
    let id = block(&mut state, DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    state.doc.clear_history();
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    (state, tool, id)
}

fn vertex_count(state: &AppState, id: hp_editor::Id) -> usize {
    state.doc.solid_vertices(id).expect("vertices").0.len()
}

fn pulled_close() -> (AppState, VertexTool, hp_editor::Id) {
    let (mut state, mut tool, id) = box_and_tool();
    tool.select_at(&state, &[DVec3::new(128.0, 0.0, 64.0)]);
    assert!(tool.move_selection(&mut state, DVec3::new(-126.0, 0.0, 0.0), "Move vertices", None));
    state.doc.clear_history();
    (state, tool, id)
}

#[test]
fn pure_welds_follow_the_target() {
    let pts = [DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 0.0), DVec3::new(0.0, 9.0, 0.0)];
    let c = weld::weld_group(&pts, &[0, 1], WeldTarget::Center);
    assert_eq!((c[0], c[1], c[2]), (DVec3::new(1.0, 0.0, 0.0), DVec3::new(1.0, 0.0, 0.0), pts[2]));
    assert_eq!(weld::weld_group(&pts, &[1, 0], WeldTarget::First)[1], pts[0]);
    assert_eq!(weld::weld_group(&pts, &[0, 1], WeldTarget::Last)[0], pts[1]);
    assert_eq!(weld::welded_vertices(&pts, &c), 1);
    assert_eq!(weld::weld_group(&pts, &[7], WeldTarget::Center), pts.to_vec());
}

#[test]
fn weld_by_distance_repeats_until_stable() {
    let pts = [DVec3::ZERO, DVec3::new(0.6, 0.0, 0.0), DVec3::new(1.2, 0.0, 0.0), DVec3::new(50.0, 0.0, 0.0)];
    let out = weld::weld_distance(&pts, 1.0);
    assert_eq!(weld::distinct_count(&out), 2);
    assert_eq!(out[3], pts[3]);
    assert_eq!(weld::weld_distance(&pts, 0.5), pts.to_vec());
    assert_eq!(weld::weld_distance(&pts, 0.0), pts.to_vec());
}

#[test]
fn weld_selected_merges_two_close_vertices_in_one_undo_step() {
    let (mut state, mut tool, id) = pulled_close();
    assert_eq!(vertex_count(&state, id), 8);
    tool.select_at(&state, &[DVec3::new(0.0, 0.0, 64.0), DVec3::new(2.0, 0.0, 64.0)]);
    assert_eq!(tool.selected_weld_count(), 1);
    assert!(tool.action(&mut state, Action::VertexWeld));
    assert_eq!(vertex_count(&state, id), 7);
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(tool.selected_vertex_count(&state), 1);
    assert!(state.doc.scene().solid(id).expect("solid").geom.valid);
    state.doc.undo();
    assert_eq!(vertex_count(&state, id), 8);
}

#[test]
fn weld_to_first_keeps_the_first_vertex() {
    let (mut state, mut tool, id) = pulled_close();
    tool.weld_target = WeldTarget::First;
    tool.select_at(&state, &[DVec3::new(0.0, 0.0, 64.0), DVec3::new(2.0, 0.0, 64.0)]);
    assert!(tool.weld_selected(&mut state));
    let pts = state.doc.solid_vertices(id).expect("vertices").0;
    assert!(pts.iter().any(|p| p.distance(DVec3::new(0.0, 0.0, 64.0)) < 1e-6));
    assert!(!pts.iter().any(|p| p.distance(DVec3::new(2.0, 0.0, 64.0)) < 1e-6));
}

#[test]
fn weld_that_would_make_the_brush_concave_is_refused() {
    let (mut state, mut tool, id) = box_and_tool();
    tool.select_at(&state, &[DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 128.0, 64.0)]);
    let before = state.doc.solid_vertices(id).expect("vertices").0;
    assert!(!tool.weld_selected(&mut state));
    assert_eq!(undo_steps(&state), 0);
    assert_eq!(state.doc.solid_vertices(id).expect("vertices").0, before);
    assert!(has_toast(&mut state));
}

#[test]
fn weld_needs_two_selected_vertices() {
    let (mut state, mut tool, id) = box_and_tool();
    assert!(!tool.weld_selected(&mut state));
    tool.select_at(&state, &[DVec3::ZERO]);
    assert!(tool.action(&mut state, Action::VertexWeld));
    assert_eq!(undo_steps(&state), 0);
    assert_eq!(vertex_count(&state, id), 8);
}

#[test]
fn weld_by_distance_welds_close_pairs_and_previews_the_count() {
    let (mut state, mut tool, id) = pulled_close();
    tool.weld_distance = 0.5;
    assert_eq!(tool.distance_weld_count(), 0);
    assert!(!tool.weld_by_distance(&mut state));
    tool.weld_distance = 3.0;
    assert_eq!(tool.distance_weld_count(), 1);
    assert!(tool.action(&mut state, Action::VertexWeldDistance));
    assert_eq!(vertex_count(&state, id), 7);
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(tool.distance_weld_count(), 0);
}

#[test]
fn cleanup_does_nothing_on_a_clean_box_and_the_options_bar_renders() {
    let (mut state, mut tool, _) = pulled_close();
    assert!(tool.action(&mut state, Action::VertexCleanup));
    assert_eq!(undo_steps(&state), 0);
    tool.weld_distance = 3.0;
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    assert!(tool.context_entries(&state).iter().any(|e| e.id == 5 && e.enabled));
}

#[test]
fn weld_options_are_scriptable() {
    let (mut state, mut tool, _) = box_and_tool();
    assert!(tool.set_option(&mut state, "weld_distance", "2.5"));
    assert!(tool.set_option(&mut state, "weld_target", "last"));
    assert!(!tool.set_option(&mut state, "weld_target", "middle"));
    assert!(!tool.set_option(&mut state, "weld_distance", "0"));
    assert!(tool.set_option(&mut state, "merge_distance", "0.5"));
    let keys: Vec<&str> = tool.option_values().iter().map(|o| o.key).collect();
    assert_eq!(keys, ["merge_distance", "weld_distance", "weld_target"]);
    assert_eq!((tool.weld_distance, tool.weld_target, tool.merge_tolerance), (2.5, WeldTarget::Last, 0.5));
}

#[test]
fn document_level_weld_matches_the_tool() {
    let (mut state, _, id) = pulled_close();
    let opts = hp_geom::VertexEditOptions::default();
    let err = weld::weld_vertices_in_solid(&mut state.doc, id, &[0], WeldTarget::Center, opts);
    assert!(err.is_err());
    let report = weld::weld_solids_by_distance(&mut state.doc, &[id], 3.0, opts);
    assert_eq!((report.solids_changed, report.vertices_welded), (1, 1));
    let again = weld::cleanup_solids(&mut state.doc, &[id], opts);
    assert_eq!(again.solids_changed, 0);
}
