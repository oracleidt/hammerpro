use super::disp_live::{max_crack, quad_tiles};
use super::*;
use crate::disp::loops::{LoopAlong, LoopEdit, edit_loop, loop_from_vertex, loop_view, pick_loop};
use crate::vertex::loops::{EdgeCmd, compute_edges, pick_edge};
use crate::vertex::model::EditSolid;
use crate::vertex::xform::{LoopSpec, RotateAxis, transform_loop};

fn box_tool() -> (AppState, VertexTool, hp_editor::Id) {
    let mut state = state_with(&[]);
    let id = block(&mut state, DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    state.doc.clear_history();
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    (state, tool, id)
}

fn top_front(id: hp_editor::Id) -> (hp_editor::Id, DVec3, DVec3) {
    (id, DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 0.0, 64.0))
}

fn frame_of_boxes(state: &mut AppState) -> Vec<hp_editor::Id> {
    let mut ids = Vec::new();
    for gx in 0..3 {
        for gy in 0..3 {
            if gx == 1 && gy == 1 {
                continue;
            }
            let min = DVec3::new(f64::from(gx) * 64.0, f64::from(gy) * 64.0, 0.0);
            ids.push(block(state, min, min + DVec3::new(64.0, 64.0, 64.0)));
        }
    }
    ids
}

fn points_at_height(state: &AppState, id: hp_editor::Id, z: f64) -> usize {
    state.doc.solid_vertices(id).expect("vertices").0.iter().filter(|p| (p.z - z).abs() < 1e-6).count()
}

#[test]
fn the_loop_of_a_box_edge_follows_the_face_that_looks_at_the_viewer() {
    let (mut state, mut tool, id) = box_tool();
    let sum = tool.edge_command(&mut state, EdgeCmd::Loop, &[top_front(id)], Some(DVec3::Z), false).expect("loop");
    assert_eq!((sum.edges, sum.vertices, sum.closed), (4, 4, true));
    assert_eq!(tool.selected_vertex_count(&state), 4);
    assert_eq!(tool.test_edge_counts(), (4, 4));
    assert!(tool.selected_vertices(&state).iter().all(|(_, p)| (p.z - 64.0).abs() < 1e-9));
    let sum = tool.edge_command(&mut state, EdgeCmd::Loop, &[top_front(id)], Some(-DVec3::Y), false).expect("loop");
    assert_eq!(sum.vertices, 4);
    assert!(tool.selected_vertices(&state).iter().all(|(_, p)| p.y.abs() < 1e-9));
    assert_eq!(undo_steps(&state), 0, "selecting edges is not an undo step");
}

#[test]
fn the_ring_of_a_box_edge_crosses_the_four_sided_faces() {
    let (mut state, mut tool, id) = box_tool();
    let sum = tool.edge_command(&mut state, EdgeCmd::Ring, &[top_front(id)], None, false).expect("ring");
    assert_eq!((sum.edges, sum.vertices), (4, 8));
    assert_eq!(tool.selected_vertex_count(&state), 8);
}

#[test]
fn adding_and_the_selection_commands_work_from_the_current_selection() {
    let (mut state, mut tool, id) = box_tool();
    tool.edge_command(&mut state, EdgeCmd::Loop, &[top_front(id)], Some(DVec3::Z), false).expect("loop");
    let bottom = (id, DVec3::ZERO, DVec3::new(128.0, 0.0, 0.0));
    tool.edge_command(&mut state, EdgeCmd::Loop, &[bottom], Some(-DVec3::Z), true).expect("added");
    assert_eq!(tool.selected_vertex_count(&state), 8);
    tool.select_at(&state, &[DVec3::ZERO]);
    assert_eq!(tool.selected_vertex_count(&state), 1);
    assert!(tool.action(&mut state, Action::EdgeGrow));
    assert_eq!(tool.selected_vertex_count(&state), 4);
    assert!(tool.action(&mut state, Action::EdgeShrink));
    assert_eq!(tool.selected_vertex_count(&state), 1);
    let top = [
        DVec3::new(0.0, 0.0, 64.0),
        DVec3::new(128.0, 0.0, 64.0),
        DVec3::new(128.0, 128.0, 64.0),
        DVec3::new(0.0, 128.0, 64.0),
    ];
    tool.select_at(&state, &top);
    assert!(tool.action(&mut state, Action::EdgeSelectBoundary));
    assert_eq!(tool.test_edge_counts(), (4, 4));
    tool.select_at(&state, &[]);
    assert!(tool.action(&mut state, Action::EdgeSelectLoop));
    assert!(has_toast(&mut state), "nothing selected: the tool says what to do");
}

#[test]
fn alt_click_and_hover_pick_the_edge_under_the_cursor() {
    let (mut state, mut tool, id) = box_tool();
    let view = view2d(ViewKind::Top, DVec2::new(64.0, 64.0), 2.0);
    let mid = view.world_to_screen(DVec3::new(64.0, 0.0, 64.0)).expect("on screen");
    let alt = Modifiers { alt: true, ..Modifiers::default() };
    tool.handle(&mut state, &view, Gesture::Hover(mid), alt);
    assert_eq!(tool.test_loop_hover(), 4);
    tool.handle(&mut state, &view, Gesture::Click(mid), alt);
    assert_eq!(tool.selected_vertex_count(&state), 4);
    assert!(tool.selected_vertices(&state).iter().all(|(s, p)| *s == id && (p.z - 64.0).abs() < 1e-9));
    let status = tool.test_status().expect("status with counts");
    assert!(status.contains('4'), "{status}");
    let ctrl_alt = Modifiers { alt: true, ctrl: true, command: true, ..Modifiers::default() };
    tool.handle(&mut state, &view, Gesture::Click(mid), ctrl_alt);
    assert_eq!(tool.selected_vertex_count(&state), 8, "Ctrl+Alt selects the ring");
    let far = Pos2::new(5.0, 5.0);
    tool.handle(&mut state, &view, Gesture::Click(far), alt);
    assert_eq!(tool.selected_vertex_count(&state), 8, "no edge there: nothing changes");
    let solids = vec![EditSolid::from_doc(&state.doc, id).expect("solid")];
    assert!(pick_edge(&solids, &view, mid).is_some());
    assert!(pick_edge(&solids, &view, far).is_none());
}

#[test]
fn a_ring_of_brushes_gives_one_closed_loop_across_all_of_them() {
    let mut state = state_with(&[]);
    let ids = frame_of_boxes(&mut state);
    let seed = (ids[0], DVec3::new(0.0, 0.0, 64.0), DVec3::new(64.0, 0.0, 64.0));
    let set = compute_edges(&state.doc, &ids, EdgeCmd::Loop, &[seed], Some(DVec3::Z), 1).expect("loop");
    assert!(set.closed);
    assert_eq!(set.edges.len(), 12);
    assert_eq!(set.vertices.len(), 12);
    let solids: std::collections::HashSet<_> = set.edges.iter().map(|e| e.solid).collect();
    assert_eq!(solids.len(), 8);
    let objs: Vec<ObjRef> = ids.iter().map(|i| ObjRef::Solid(*i)).collect();
    state.doc.select(&objs, SelectOp::Replace);
    state.doc.clear_history();
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let sum = tool.edge_command(&mut state, EdgeCmd::Loop, &[seed], Some(DVec3::Z), false).expect("loop");
    assert_eq!((sum.edges, sum.vertices, sum.closed), (12, 12, true));
    assert!(tool.move_selection(&mut state, DVec3::new(0.0, 0.0, 16.0), "Move vertices", None));
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(tool.test_edge_counts().0, 12, "the edge selection follows the move");
    for id in &ids {
        assert!(state.doc.scene().solid(*id).expect("solid").geom.valid);
    }
    state.doc.undo();
    assert_eq!(points_at_height(&state, ids[0], 64.0), 4);
}

#[test]
fn moving_scaling_rotating_and_sliding_a_loop_are_single_undo_steps() {
    let mut state = state_with(&[]);
    let id = block(&mut state, DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
    state.doc.clear_history();
    let seed = top_front(id);
    let opts = hp_geom::VertexEditOptions { uv_lock: false };
    let run = |state: &mut AppState, spec: LoopSpec| {
        transform_loop(&mut state.doc, &[id], &[seed], Some(EdgeCmd::Loop), Some(DVec3::Z), &spec, opts)
    };
    let r = run(&mut state, LoopSpec::Move(DVec3::new(0.0, 0.0, 32.0))).expect("move");
    assert_eq!((r.solids, r.vertices), (1, 4));
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(points_at_height(&state, id, 96.0), 4);
    state.doc.undo();
    let r = run(&mut state, LoopSpec::Scale { factor: 0.5, center: None }).expect("scale");
    assert_eq!(r.vertices, 4);
    let pts = state.doc.solid_vertices(id).expect("vertices").0;
    assert!(pts.iter().any(|p| p.distance(DVec3::new(32.0, 32.0, 64.0)) < 1e-6));
    state.doc.undo();
    let r = run(&mut state, LoopSpec::Rotate { axis: RotateAxis::Normal, degrees: 45.0, center: None }).expect("rot");
    assert_eq!(r.vertices, 4);
    let pts = state.doc.solid_vertices(id).expect("vertices").0;
    let half = 64.0 * 2f64.sqrt();
    assert!(pts.iter().any(|p| p.distance(DVec3::new(64.0, 64.0 - half, 64.0)) < 1e-6), "{pts:?}");
    state.doc.undo();
    let r = run(&mut state, LoopSpec::Slide(0.5)).expect("slide");
    assert_eq!(r.vertices, 4);
    assert_eq!(points_at_height(&state, id, 96.0), 4, "positive slides toward +Z, extending the sides");
    state.doc.undo();
    let steps = undo_steps(&state);
    let refused = run(&mut state, LoopSpec::Slide(-1.0)).expect_err("sliding all the way down flattens the brush");
    assert!(refused.contains("invalid") || refused.contains("flat") || refused.contains("volume"), "{refused}");
    assert_eq!(undo_steps(&state), steps, "a refused edit leaves no undo step");
}

#[test]
fn rotate_and_slide_modals_preview_then_apply() {
    let (mut state, mut tool, id) = box_tool();
    assert!(!tool.test_start_rotate(&mut state), "nothing selected");
    assert!(!tool.test_start_slide(&mut state));
    tool.edge_command(&mut state, EdgeCmd::Loop, &[top_front(id)], Some(DVec3::Z), false).expect("loop");
    assert!(tool.test_start_slide(&mut state));
    assert!(tool.is_busy());
    assert!(tool.test_xform_value(0.5));
    assert!(!tool.test_xform_error());
    assert!(tool.status_hint(&state).expect("hint").contains("0.50"));
    assert!(tool.cancel(&mut state));
    assert!(!tool.is_busy());
    assert_eq!(undo_steps(&state), 0);
    assert!(tool.test_start_slide(&mut state));
    tool.test_xform_value(-1.0);
    assert!(tool.test_xform_error(), "a flat brush is refused in the preview");
    assert!(!tool.test_apply_xform(&mut state));
    assert_eq!(undo_steps(&state), 0);
    tool.test_xform_value(0.5);
    assert!(tool.test_apply_xform(&mut state));
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(tool.selected_vertex_count(&state), 4, "the loop stays selected");
    assert_eq!(tool.test_edge_counts().0, 4);
    assert!(tool.test_start_rotate(&mut state));
    tool.test_xform_axis(RotateAxis::Normal);
    assert!(tool.test_xform_value(30.0));
    assert!(tool.test_apply_xform(&mut state));
    assert_eq!(undo_steps(&state), 2);
    state.doc.undo();
    state.doc.undo();
    assert_eq!(points_at_height(&state, id, 64.0), 4);
}

#[test]
fn bevel_edges_accept_a_whole_loop() {
    let mut state = state_with(&[]);
    let id = block(&mut state, DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
    let seed = hp_editor::EdgeRef::new(id, DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 0.0, 64.0));
    let run = state.doc.edge_run_refs(hp_editor::RunKind::Loop, &seed, &[], Some(DVec3::Z)).expect("loop");
    assert_eq!(run.len(), 4);
    let params = hp_geom::BevelParams { radius: 8.0, segments: 2, inverse: false, material: None };
    state.doc.clear_history();
    let out = state.doc.bevel_edges(&run, &params).expect("bevel the whole loop");
    assert!(!out.ids.is_empty());
    assert_eq!(undo_steps(&state), 1);
    let node = state.doc.scene().solid(id).expect("solid");
    assert!(node.geom.valid);
    assert!(node.solid.sides.len() >= 10, "{} sides", node.solid.sides.len());
}

fn disp_grid() -> (AppState, Vec<FaceRef>) {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let faces = quad_tiles(&mut state, &mut tool, [3, 3, 3, 3]);
    state.doc.clear_history();
    (state, faces)
}

fn untouched_face(state: &AppState, f: FaceRef) -> bool {
    distances(state, f).iter().all(|d| *d == 0.0)
}

#[test]
fn a_displacement_row_runs_across_the_sewn_neighbours() {
    let (state, faces) = disp_grid();
    let sel = loop_from_vertex(&state.doc, faces[0], 4, 0, LoopAlong::Row).expect("seed");
    let view = loop_view(&state.doc, &sel).expect("loop");
    assert_eq!(view.positions.len(), 17, "9 vertices on the first tile and 8 more on its neighbour");
    assert!(!view.closed);
    assert_eq!(view.faces, 4);
    let straight = view.positions.windows(2).all(|w| (w[0].y - w[1].y).abs() < 1e-6 || (w[0].x - w[1].x).abs() < 1e-6);
    assert!(straight);
    let col = loop_from_vertex(&state.doc, faces[0], 0, 4, LoopAlong::Column).expect("seed");
    assert_eq!(loop_view(&state.doc, &col).expect("loop").positions.len(), 17);
    assert!(loop_from_vertex(&state.doc, faces[0], 9, 0, LoopAlong::Row).is_err());
    assert!(loop_from_vertex(&state.doc, FaceRef::new(2, 0), 0, 0, LoopAlong::Row).is_err());
}

#[test]
fn setting_the_height_of_a_loop_keeps_the_tiles_sewn_and_is_one_undo_step() {
    let (mut state, faces) = disp_grid();
    let sel = loop_from_vertex(&state.doc, faces[0], 4, 0, LoopAlong::Row).expect("seed");
    let n = edit_loop(&mut state.doc, &sel, LoopEdit::Height(24.0)).expect("height");
    assert!(n >= 17);
    assert_eq!(undo_steps(&state), 1);
    let after = loop_view(&state.doc, &sel).expect("loop").positions;
    assert_eq!(after.len(), 17);
    assert!(after.iter().all(|p| (p.z - 24.0).abs() < 1e-6), "{after:?}");
    assert!(max_crack(&state, &faces) < 0.3);
    state.doc.undo();
    assert!(faces.iter().all(|f| untouched_face(&state, *f)));
}

#[test]
fn smoothing_a_loop_moves_the_vertices_along_the_loop_only() {
    let (mut state, faces) = disp_grid();
    let row = loop_from_vertex(&state.doc, faces[0], 4, 0, LoopAlong::Row).expect("seed");
    let n = edit_loop(&mut state.doc, &row, LoopEdit::Smooth { strength: 0.5, passes: 2 }).expect("smooth");
    assert_eq!(n, 0, "a flat loop has nothing to smooth");
    assert_eq!(undo_steps(&state), 0);
    edit_loop(&mut state.doc, &row, LoopEdit::Height(32.0)).expect("raise");
    state.doc.clear_history();
    let column = loop_from_vertex(&state.doc, faces[0], 0, 4, LoopAlong::Column).expect("seed");
    let n = edit_loop(&mut state.doc, &column, LoopEdit::Smooth { strength: 1.0, passes: 1 }).expect("smooth");
    assert!(n > 0, "the column crosses the raised row and is smoothed");
    assert_eq!(undo_steps(&state), 1);
    assert!(max_crack(&state, &faces) < 0.3);
}

#[test]
fn picking_a_grid_line_and_the_tool_commands() {
    let (mut state, faces) = disp_grid();
    let view = view3d(DVec3::new(0.0, 0.0, 700.0), 0.0, -89.9);
    let mesh0 = mesh(&state, faces[0]);
    let k = 4 * mesh0.row_len + 3;
    let a = view.world_to_screen(mesh0.positions[k]).expect("on screen");
    let b = view.world_to_screen(mesh0.positions[k + 1]).expect("on screen");
    let at = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
    let sel = pick_loop(&state.doc, &view, at, &faces[..1]).expect("picked");
    assert_eq!(sel.face, faces[0]);
    let mut tool = DisplacementTool::new();
    assert!(tool.set_option(&mut state, "mode", "select"));
    let alt = Modifiers { alt: true, ..Modifiers::default() };
    let press = PointerFrame { pos: Some(at), pressed: true, down: true, mods: alt, ..PointerFrame::default() };
    let release = PointerFrame { pos: Some(at), pressed: false, down: false, mods: alt, ..PointerFrame::default() };
    tool.handle_pointer(&mut state, &view, press);
    tool.handle_pointer(&mut state, &view, release);
    let status = tool.status_hint(&state).expect("status");
    assert!(status.contains("17"), "{status}");
    assert!(tool.set_option(&mut state, "loop_height", "12"));
    assert!(tool.action(&mut state, Action::DispLoopHeight));
    assert_eq!(undo_steps(&state), 1);
    assert!(tool.action(&mut state, Action::DispSmoothLoop));
    assert!(!tool.set_option(&mut state, "loop_passes", "0"));
    assert!(tool.cancel(&mut state));
    assert!(!tool.action(&mut state, Action::EdgeGrow));
}
