use super::*;
use crate::disp::commands;
use crate::vertex::VertexMode;

fn alt() -> Modifiers {
    Modifiers { alt: true, ..Modifiers::default() }
}

fn clear_toasts(state: &mut AppState) {
    while !state.toasts.live().is_empty() {
        state.toasts.dismiss(0);
    }
}

fn bounds(state: &AppState, id: hp_editor::Id) -> hp_geom::Aabb {
    state.doc.scene().solid(id).expect("solid").bounds
}

fn face_count(state: &AppState, id: hp_editor::Id) -> usize {
    state.doc.scene().solid(id).expect("solid").geom.faces.len()
}

fn box_and_tool() -> (AppState, VertexTool, hp_editor::Id) {
    let mut state = state_with(&[]);
    let id = block(&mut state, DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    state.doc.clear_history();
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    (state, tool, id)
}

#[test]
fn s_axis_aligns_the_selected_group_on_the_grid_and_undoes_as_one_edit() {
    for axis in 0..3 {
        for func in [false, true] {
            let mut state = state_with(&[]);
            state.settings.edit.grid_size = 32.0;
            let mut amax = DVec3::splat(64.0);
            amax[axis] = 64.0;
            let mut bmin = DVec3::ZERO;
            bmin[(axis + 1) % 3] = 256.0;
            let mut bmax = bmin + DVec3::splat(64.0);
            bmax[axis] = 96.0;
            let a = block(&mut state, DVec3::ZERO, amax);
            let b = block(&mut state, bmin, bmax);
            if func {
                state.doc.tie_to_entity(&[ObjRef::Solid(a), ObjRef::Solid(b)], "func_brush", &[]).unwrap();
            }
            state.doc.select(&[ObjRef::Solid(a), ObjRef::Solid(b)], SelectOp::Replace);
            let mut tool = VertexTool::new();
            tool.activate(&mut state);
            let points: Vec<_> = [a, b]
                .into_iter()
                .flat_map(|id| {
                    EditSolid::from_doc(&state.doc, id)
                        .unwrap()
                        .points
                        .into_iter()
                        .filter(move |p| (p[axis] - if id == a { 64.0 } else { 96.0 }).abs() < 1e-6)
                })
                .collect();
            tool.select_at(&state, &points);
            let view = view2d(ViewKind::Front, DVec2::ZERO, 1.0);
            tool.handle(&mut state, &view, Gesture::Hover(Pos2::new(5.0, 5.0)), Modifiers::NONE);
            state.ui.hovered_view = Some(view.index);
            state.doc.clear_history();
            let before = vmf(&state);
            assert!(run_keys(
                &mut state,
                &mut tool,
                &[(egui::Key::S, Modifiers::NONE), ([egui::Key::X, egui::Key::Y, egui::Key::Z][axis], Modifiers::NONE)]
            ));
            let selection = tool.selected_vertices(&state);
            assert_eq!(selection.len(), 8);
            assert!(selection.iter().all(|(_, p)| (p[axis] - 96.0).abs() < 1e-6), "{axis}: {selection:?}");
            assert_eq!(state.doc.history_len().0, 1);
            assert!(state.doc.undo());
            assert_eq!(vmf(&state), before);
            assert!(state.doc.redo());
        }
    }
}

#[test]
fn axis_alignment_preserves_a_displacement_quad() {
    let (mut state, _, face) = floor_with_disp(2);
    let other = block(&mut state, DVec3::new(640.0, 0.0, -64.0), DVec3::new(704.0, 64.0, 64.0));
    state.doc.select(&[ObjRef::Solid(FLOOR), ObjRef::Solid(other)], SelectOp::Replace);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let points: Vec<_> = [FLOOR, other]
        .into_iter()
        .flat_map(|id| {
            EditSolid::from_doc(&state.doc, id)
                .unwrap()
                .points
                .into_iter()
                .filter(move |p| (p.z - if id == FLOOR { 0.0 } else { 64.0 }).abs() < 1e-6)
        })
        .collect();
    tool.select_at(&state, &points);
    let view = view2d(ViewKind::Front, DVec2::ZERO, 1.0);
    tool.handle(&mut state, &view, Gesture::Hover(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    assert!(tool.align_selection(&mut state, 2));
    assert_eq!(state.doc.disp_faces(), vec![face]);
    assert!(tool.selected_vertices(&state).iter().all(|(_, p)| (p.z - 32.0).abs() < 1e-6));
}

#[test]
fn axis_shortcuts_ignore_3d_and_modified_keys_and_refuse_flat_solids() {
    let (mut state, mut tool, _) = box_and_tool();
    tool.select_all_vertices(&state);
    let before = vmf(&state);
    let perspective = view3d(DVec3::splat(500.0), 45.0, -20.0);
    tool.handle(&mut state, &perspective, Gesture::Hover(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    assert!(!run_keys(&mut state, &mut tool, &[(egui::Key::S, Modifiers::NONE), (egui::Key::X, Modifiers::NONE)]));
    let top = view2d(ViewKind::Top, DVec2::ZERO, 1.0);
    tool.handle(&mut state, &top, Gesture::Hover(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    state.ui.hovered_view = Some(top.index);
    assert!(!run_keys(&mut state, &mut tool, &[(egui::Key::S, ctrl()), (egui::Key::X, ctrl())]));
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::S, Modifiers::NONE), (egui::Key::X, Modifiers::NONE)]));
    assert_eq!(vmf(&state), before);
    assert!(has_toast(&mut state), "invalid flat brush reports a reason");
}

fn has_edge(state: &AppState, id: hp_editor::Id, a: DVec3, b: DVec3) -> bool {
    let g = &state.doc.scene().solid(id).expect("solid").geom;
    g.edges()
        .iter()
        .any(|[p, q]| (p.distance(a) < 1e-6 && q.distance(b) < 1e-6) || (p.distance(b) < 1e-6 && q.distance(a) < 1e-6))
}

#[test]
fn shift_v_cycles_modes_and_edges_mode_moves_edges() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    tool.reactivate(&mut state);
    assert_eq!(tool.mode, VertexMode::Vertices);
    assert!(has_toast(&mut state), "the new mode is announced");
    tool.reactivate(&mut state);
    assert_eq!(tool.mode, VertexMode::Edges);
    let top = view2d(ViewKind::Top, DVec2::new(0.0, 264.0), 1.0);
    let mid = top.world_to_screen(DVec3::new(0.0, 256.0, 0.0)).expect("screen");
    let to = mid + vec2(0.0, 32.0);
    tool.handle(&mut state, &top, Gesture::DragStart { origin: mid, pos: to }, Modifiers::NONE);
    tool.handle(&mut state, &top, Gesture::DragEnd(to), Modifiers::NONE);
    assert_eq!(bounds(&state, WALL).min.y, 224.0);
    assert_eq!(EditSolid::from_doc(&state.doc, WALL).expect("wall").points.len(), 8);
    assert_eq!(tool.selected_vertex_count(&state), 4, "the two edges stay selected");
    tool.mode = VertexMode::Vertices;
    let corner = top.world_to_screen(DVec3::new(256.0, 224.0, 0.0)).expect("screen");
    tool.handle(&mut state, &top, Gesture::Click(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    let mid = top.world_to_screen(DVec3::new(0.0, 224.0, 0.0)).expect("screen");
    tool.handle(&mut state, &top, Gesture::Click(mid), Modifiers::NONE);
    assert_eq!(tool.selected_vertex_count(&state), 0);
    tool.mode = VertexMode::Edges;
    tool.handle(&mut state, &top, Gesture::Click(corner), Modifiers::NONE);
    assert_eq!(tool.selected_vertex_count(&state), 0);
    tool.reactivate(&mut state);
    assert_eq!(tool.mode, VertexMode::Both);
}

#[test]
fn ctrl_b_snaps_the_selected_vertices() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    assert!(!tool.action(&mut state, Action::SnapToGrid));
    tool.select_at(&state, &[DVec3::new(256.0, 256.0, 128.0), DVec3::new(256.0, 272.0, 128.0)]);
    assert!(tool.move_selection(&mut state, DVec3::new(5.0, 0.0, 3.0), "Move vertices", None));
    assert!(tool.action(&mut state, Action::SnapToGrid));
    let sel = tool.selected_vertices(&state);
    assert_eq!(sel.len(), 2);
    assert!(sel.iter().all(|(_, p)| p.x == 256.0 && p.z == 128.0), "{sel:?}");
    assert_eq!(state.doc.undo_label(), Some("Snap to grid"));
    let steps = undo_steps(&state);
    assert!(tool.action(&mut state, Action::SnapToGrid));
    assert_eq!(undo_steps(&state), steps);
}

#[test]
fn alt_arrows_nudge_by_one_unit_and_page_keys_by_depth() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let top = view2d(ViewKind::Top, DVec2::new(0.0, 264.0), 1.0);
    tool.handle(&mut state, &top, Gesture::Hover(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    state.ui.hovered_view = Some(top.index);
    tool.select_all_vertices(&state);
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::ArrowRight, alt())]));
    assert_eq!(bounds(&state, WALL).min.x, -255.0);
    run_keys(&mut state, &mut tool, &[(egui::Key::ArrowLeft, Modifiers::NONE)]);
    assert_eq!(bounds(&state, WALL).min.x, -271.0);
    run_keys(&mut state, &mut tool, &[(egui::Key::PageUp, Modifiers::NONE)]);
    assert_eq!(bounds(&state, WALL).min.z, 16.0);
    assert!(!run_keys(&mut state, &mut tool, &[(egui::Key::ArrowRight, ctrl())]));
    let v3 = view3d(DVec3::new(0.0, -600.0, 300.0), 90.0, -20.0);
    tool.handle(&mut state, &v3, Gesture::Hover(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    state.ui.hovered_view = Some(v3.index);
    let before = bounds(&state, WALL);
    run_keys(&mut state, &mut tool, &[(egui::Key::ArrowUp, alt())]);
    let after = bounds(&state, WALL);
    assert_eq!(after.min.y - before.min.y, 1.0, "forward is +Y for this camera");
    assert_eq!(undo_steps(&state), 1, "consecutive nudges merge");
}

#[test]
fn split_face_between_vertices_keeps_the_ridge() {
    let (mut state, mut tool, id) = box_and_tool();
    let (a, b) = (DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 128.0, 64.0));
    assert_eq!(tool.select_at(&state, &[a, b]), 2);
    assert!(tool.action(&mut state, Action::VertexSplitFace));
    assert_eq!(tool.pending_splits(&state), 1);
    assert_eq!(tool.selected_vertex_count(&state), 2, "the ends of the new edge");
    assert_eq!(undo_steps(&state), 0, "nothing is written yet");
    let top = view2d(ViewKind::Top, DVec2::new(64.0, 64.0), 1.0);
    let mut out = hp_render::Overlay::default();
    tool.overlay(&state, &top, &mut out);
    let f = |p: DVec3| [p.x as f32, p.y as f32, p.z as f32];
    assert!(out.lines_on_top.iter().any(|l| (l.a == f(a) && l.b == f(b)) || (l.a == f(b) && l.b == f(a))));
    tool.select_at(&state, &[DVec3::new(128.0, 0.0, 64.0)]);
    assert!(tool.move_selection(&mut state, DVec3::new(0.0, 0.0, -32.0), UNDO_EDIT, None));
    assert_eq!(face_count(&state, id), 7);
    assert!(has_edge(&state, id, a, b), "the split edge is a real edge now");
    assert_eq!(tool.pending_splits(&state), 0);
    assert!(state.doc.scene().solid(id).expect("box").geom.valid);
    state.doc.undo();
    let before = vmf(&state);
    tool.select_at(&state, &[a, b]);
    assert!(tool.action(&mut state, Action::VertexSplitFace));
    tool.select_at(&state, &[DVec3::new(128.0, 0.0, 64.0)]);
    assert!(!tool.move_selection(&mut state, DVec3::new(0.0, 0.0, 32.0), UNDO_EDIT, None));
    assert_eq!(vmf(&state), before);
    assert!(has_toast(&mut state));
}

#[test]
fn coplanar_halves_merge_back_and_the_split_waits() {
    let (mut state, mut tool, id) = box_and_tool();
    let (a, b) = (DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 128.0, 64.0));
    tool.select_at(&state, &[a, b]);
    tool.action(&mut state, Action::VertexSplitFace);
    let bottom: Vec<DVec3> =
        [(0.0, 0.0), (128.0, 0.0), (128.0, 128.0), (0.0, 128.0)].iter().map(|(x, y)| DVec3::new(*x, *y, 0.0)).collect();
    tool.select_at(&state, &bottom);
    assert!(tool.move_selection(&mut state, DVec3::new(0.0, 0.0, -16.0), UNDO_EDIT, None));
    assert_eq!(face_count(&state, id), 6);
    assert_eq!(bounds(&state, id).min.z, -16.0);
    assert_eq!(tool.pending_splits(&state), 1);
    assert!(tool.cancel(&mut state));
    assert!(tool.cancel(&mut state));
    assert_eq!(tool.pending_splits(&state), 0);
    assert!(!tool.cancel(&mut state));
}

#[test]
fn split_between_edge_midpoints_then_raise_a_roof() {
    let (mut state, mut tool, id) = box_and_tool();
    let (ma, mb) = (DVec3::new(64.0, 0.0, 64.0), DVec3::new(64.0, 128.0, 64.0));
    assert_eq!(tool.select_edges_at(&state, &[ma, mb]), 2);
    assert!(tool.action(&mut state, Action::VertexSplitFace));
    let sel = tool.selected_vertices(&state);
    assert_eq!(sel.len(), 2);
    assert!(sel.iter().all(|(_, p)| *p == ma || *p == mb), "new vertices at the midpoints: {sel:?}");
    assert!(tool.move_selection(&mut state, DVec3::new(0.0, 0.0, 32.0), UNDO_EDIT, None));
    assert_eq!(face_count(&state, id), 7);
    assert_eq!(bounds(&state, id).max.z, 96.0);
    let g = &state.doc.scene().solid(id).expect("roof").geom;
    assert_eq!(g.faces.iter().filter(|f| f.vertices.len() == 5).count(), 2);
    assert!(g.valid);
    let top_material =
        state.doc.scene().solid(id).expect("roof").solid.sides.iter().filter(|s| s.plane[0].z > 63.0).count();
    assert!(top_material >= 2);
}

#[test]
fn split_needs_two_points_of_one_face() {
    let (mut state, mut tool, _) = box_and_tool();
    assert_eq!(tool.split_face(&mut state), 0);
    assert!(has_toast(&mut state));
    tool.select_at(&state, &[DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 0.0, 64.0)]);
    assert_eq!(tool.split_face(&mut state), 0);
    tool.select_at(&state, &[DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0)]);
    assert_eq!(tool.split_face(&mut state), 0);
    tool.select_at(&state, &[DVec3::ZERO, DVec3::new(128.0, 0.0, 0.0), DVec3::new(0.0, 128.0, 0.0)]);
    assert_eq!(tool.split_face(&mut state), 0);
    assert_eq!(tool.pending_splits(&state), 0);
    let entries = tool.context_entries(&state);
    assert!(!entries.iter().find(|e| e.id == 1).expect("split entry").enabled);
    tool.select_at(&state, &[DVec3::ZERO, DVec3::new(128.0, 128.0, 0.0)]);
    let entries = tool.context_entries(&state);
    assert!(entries.iter().find(|e| e.id == 1).expect("split entry").enabled);
    tool.context_command(&mut state, 1);
    assert_eq!(tool.pending_splits(&state), 1);
}

#[test]
fn scale_vertices_around_their_center() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    assert!(tool.action(&mut state, Action::VertexScale));
    assert!(!tool.is_scaling());
    assert!(has_toast(&mut state));
    tool.select_all_vertices(&state);
    assert!(tool.action(&mut state, Action::VertexScale));
    assert!(tool.is_scaling());
    assert_eq!(tool.scale_center(), Some(DVec3::new(0.0, 264.0, 64.0)));
    let top = view2d(ViewKind::Top, DVec2::new(0.0, 264.0), 1.0);
    assert_eq!(tool.wheel(&mut state, &top, 1.0, Modifiers::NONE), ToolFlow::Ignored);
    assert_eq!(tool.wheel(&mut state, &top, 1.0, alt()), ToolFlow::Handled);
    assert!((tool.scale_factor().expect("factor") - 1.1).abs() < 1e-9);
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    let mut out = hp_render::Overlay::default();
    tool.overlay(&state, &top, &mut out);
    assert!(!out.lines_on_top.is_empty(), "preview");
    run_ctx(&mut state, |ctx, st| tool.paint(st, &top, &ctx.layer_painter(egui::LayerId::background())));
    assert!(tool.status_hint(&state).expect("hint").contains("1.10"));
    tool.set_scale_factor(0.0);
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::Enter, Modifiers::NONE)]));
    assert!(tool.is_scaling());
    assert_eq!(undo_steps(&state), 0);
    tool.set_scale_factor(2.0);
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::Enter, Modifiers::NONE)]));
    assert!(!tool.is_scaling());
    let b = bounds(&state, WALL);
    assert_eq!((b.min.x, b.max.x, b.min.y, b.max.y, b.min.z, b.max.z), (-512.0, 512.0, 248.0, 280.0, -64.0, 192.0));
    assert_eq!(state.doc.undo_label(), Some("Scale vertices"));
    assert_eq!(tool.selected_vertex_count(&state), 8, "the scaled vertices stay selected");
    tool.action(&mut state, Action::VertexScale);
    tool.set_scale_factor(3.0);
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::Escape, Modifiers::NONE)]));
    assert!(!tool.is_scaling());
    assert_eq!(bounds(&state, WALL).max.x, 512.0);
}

#[test]
fn scale_center_is_dragged_in_2d_views() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    tool.select_all_vertices(&state);
    assert!(tool.start_scale(&mut state));
    let top = view2d(ViewKind::Top, DVec2::new(0.0, 264.0), 1.0);
    let c = top.world_to_screen(DVec3::new(0.0, 264.0, 64.0)).expect("screen");
    assert_eq!(tool.cursor(&state, &top, c), Some(egui::CursorIcon::Grab));
    let to = c + vec2(-250.0, 0.0);
    tool.handle(&mut state, &top, Gesture::DragStart { origin: c, pos: to }, Modifiers::NONE);
    tool.handle(&mut state, &top, Gesture::DragEnd(to), Modifiers::NONE);
    assert_eq!(tool.scale_center(), Some(DVec3::new(-256.0, 264.0, 64.0)));
    tool.set_scale_factor(0.5);
    assert!(tool.apply_scale(&mut state));
    let b = bounds(&state, WALL);
    assert_eq!((b.min.x, b.max.x), (-256.0, 0.0));
}

#[test]
fn uv_lock_keeps_textures_on_moved_vertices() {
    for lock in [false, true] {
        let mut state = state_with(&[WALL]);
        state.settings.mesh.uv_lock = lock;
        let mut tool = VertexTool::new();
        tool.activate(&mut state);
        let top_of = |state: &AppState| {
            let node = state.doc.scene().solid(WALL).expect("wall");
            node.solid.sides.iter().find(|s| s.id == 7).cloned().expect("top side")
        };
        let before = top_of(&state);
        let east: Vec<DVec3> = [(256.0, 0.0), (256.0, 128.0)]
            .iter()
            .flat_map(|(x, z)| [DVec3::new(*x, 256.0, *z), DVec3::new(*x, 272.0, *z)])
            .collect();
        tool.select_at(&state, &east);
        assert!(tool.move_selection(&mut state, DVec3::new(256.0, 0.0, 0.0), UNDO_EDIT, None));
        let top = top_of(&state);
        let old_corner = hp_geom::texel_uv(&before, DVec3::new(256.0, 256.0, 128.0));
        let new_corner = hp_geom::texel_uv(&top, DVec3::new(512.0, 256.0, 128.0));
        let fixed = hp_geom::texel_uv(&top, DVec3::new(-256.0, 256.0, 128.0));
        assert!((fixed - hp_geom::texel_uv(&before, DVec3::new(-256.0, 256.0, 128.0))).length() < 1e-6);
        if lock {
            assert!((old_corner - new_corner).length() < 1e-6, "the texture follows the moved edge");
        } else {
            assert_eq!(top.uaxis, before.uaxis, "fixed in world space");
        }
    }
    let mut state = state_with(&[WALL]);
    assert!(!state.settings.mesh.uv_lock);
    assert_eq!(Action::ToggleUvLock.check(&state), hp_app::actions::Check::Toggle(false));
    state.settings.mesh.uv_lock = true;
    assert_eq!(Action::ToggleUvLock.check(&state), hp_app::actions::Check::Toggle(true));
}

#[test]
fn displacements_survive_vertex_moves_that_keep_the_quad() {
    let (mut state, _, face) = floor_with_disp(2);
    let start_before = state.doc.face_side(face).and_then(|s| s.disp.as_ref()).expect("disp").start_position;
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    clear_toasts(&mut state);
    let top: Vec<DVec3> = [(-256.0, -256.0), (256.0, -256.0), (256.0, 256.0), (-256.0, 256.0)]
        .iter()
        .map(|(x, y)| DVec3::new(*x, *y, 0.0))
        .collect();
    tool.select_at(&state, &top);
    assert!(tool.move_selection(&mut state, DVec3::new(0.0, 0.0, 32.0), UNDO_EDIT, None));
    assert_eq!(state.doc.disp_faces(), vec![face], "the displacement survived on the same side");
    let disp = state.doc.face_side(face).and_then(|s| s.disp.as_ref()).expect("disp");
    assert_eq!(disp.start_position, start_before + DVec3::new(0.0, 0.0, 32.0));
    assert!(!has_toast(&mut state), "no displacement lost");
    tool.select_at(&state, &[DVec3::new(256.0, 256.0, 32.0)]);
    assert!(tool.move_selection(&mut state, DVec3::new(0.0, 0.0, 16.0), UNDO_EDIT, None));
    assert!(state.doc.disp_faces().is_empty());
    assert!(has_toast(&mut state), "the user is told");
}

#[test]
fn invert_noise_flags_and_scale_commands() {
    let (mut state, _, face) = floor_with_disp(3);
    state.doc.select_faces(&[face], SelectOp::Replace);
    let steps = undo_steps(&state);
    assert_eq!(commands::invert_alpha(&mut state), 81);
    assert!(alphas(&state, face).iter().all(|a| *a == 255.0));
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(state.doc.undo_label(), Some("Invert displacement alpha"));
    state.settings.mesh.noise_min = 6.0;
    state.settings.mesh.noise_max = 2.0;
    assert_eq!(commands::add_noise(&mut state), 81);
    assert!(distances(&state, face).iter().all(|d| (2.0..=6.0).contains(d)));
    let first = distances(&state, face);
    commands::add_noise(&mut state);
    assert_ne!(distances(&state, face), first);
    let targets = commands::targets(&state.doc);
    assert_eq!(commands::flag_state(&state.doc, &targets, hp_geom::DISP_FLAG_NO_RAY), Some(false));
    assert_eq!(commands::set_flag(&mut state, hp_geom::DISP_FLAG_NO_RAY, true), 1);
    assert_eq!(commands::flag_state(&state.doc, &targets, hp_geom::DISP_FLAG_NO_RAY), Some(true));
    let d0 = distances(&state, face);
    assert_eq!(commands::scale(&mut state, 0.5), 81);
    for (a, b) in d0.iter().zip(distances(&state, face)) {
        assert!((a * 0.5 - b).abs() < 1e-9);
    }
    state.doc.clear_selection();
    state.doc.clear_face_selection();
    let steps = undo_steps(&state);
    assert_eq!(commands::invert_alpha(&mut state), 0);
    assert_eq!(commands::subdivide(&mut state), 0);
    assert_eq!(undo_steps(&state), steps);
    assert!(has_toast(&mut state));
}

#[test]
fn subdivide_smooths_and_keeps_seams_and_borders() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let a = block(&mut state, DVec3::new(-256.0, -128.0, -16.0), DVec3::new(0.0, 128.0, 0.0));
    let b = block(&mut state, DVec3::new(0.0, -128.0, -16.0), DVec3::new(256.0, 128.0, 0.0));
    let faces: Vec<FaceRef> = [a, b].iter().map(|id| side_facing(&state, *id, DVec3::Z)).collect();
    make_disps(&mut state, &mut tool, &faces, 3);
    tool.brush.radius = 40.0;
    tool.brush.strength = 400.0;
    tool.brush.falloff = Falloff::Constant;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::new(0.0, 0.0, 0.0), false);
    tool.end_stroke(&mut state);
    let peak =
        |state: &AppState| faces.iter().flat_map(|f| mesh(state, *f).positions).map(|p| p.z).fold(f64::MIN, f64::max);
    let before = peak(&state);
    assert!(before > 30.0);
    state.doc.select_faces(&faces, SelectOp::Replace);
    let steps = undo_steps(&state);
    assert!(commands::subdivide(&mut state) > 0);
    assert_eq!(undo_steps(&state), steps + 1, "one undo step");
    assert!(peak(&state) < before, "the spike is smoothed");
    assert!(assert_seam_closed(&state, faces[0], faces[1]) > 0);
    for (base, p) in positions_where(&state, faces[0], |p| p.x == -256.0) {
        assert_eq!(base, p);
    }
    for f in &faces {
        assert_tags_current(&state, *f);
        assert!(state.doc.face_side(*f).and_then(|s| s.disp.as_ref()).expect("disp").subdiv);
    }
}

#[test]
fn autosew_closes_cracks_at_the_end_of_a_stroke() {
    for autosew in [false, true] {
        let mut state = state_with(&[]);
        let mut tool = DisplacementTool::new();
        let a = block(&mut state, DVec3::new(-256.0, -128.0, -16.0), DVec3::new(0.0, 128.0, 0.0));
        let b = block(&mut state, DVec3::new(0.0, -128.0, -16.0), DVec3::new(256.0, 128.0, 0.0));
        let fa = side_facing(&state, a, DVec3::Z);
        let fb = side_facing(&state, b, DVec3::Z);
        make_disps(&mut state, &mut tool, &[fa], 2);
        make_disps(&mut state, &mut tool, &[fb], 3);
        tool.brush.autosew = autosew;
        tool.brush.radius = 120.0;
        state.frame.dt = 0.1;
        state.doc.select_faces(&[fa], SelectOp::Replace);
        let steps = undo_steps(&state);
        tool.begin_stroke(&mut state, 0);
        assert!(tool.paint_at(&mut state, DVec3::new(-20.0, 0.0, 0.0), false) > 0);
        tool.end_stroke(&mut state);
        assert_eq!(undo_steps(&state), steps + 1, "one undo step with or without the sew");
        let ma = mesh(&state, fa);
        let mb = mesh(&state, fb);
        let gap = mb
            .base_positions
            .iter()
            .zip(&mb.positions)
            .filter_map(|(base, p)| {
                let k = ma.base_positions.iter().position(|q| q.distance(*base) < 1e-6)?;
                Some(p.distance(ma.positions[k]))
            })
            .fold(0.0f64, f64::max);
        if autosew {
            assert!(gap < 1e-9, "sewn: {gap}");
        } else {
            assert!(gap > 1.0, "the crack is there without AutoSew");
        }
    }
}

#[test]
fn line_of_sight_leaves_hidden_terrain_alone() {
    for los in [false, true] {
        let mut state = state_with(&[]);
        let mut tool = DisplacementTool::new();
        let upper = block(&mut state, DVec3::new(-256.0, -256.0, 48.0), DVec3::new(256.0, 256.0, 64.0));
        let lower = block(&mut state, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0));
        let fu = side_facing(&state, upper, DVec3::Z);
        let fl = side_facing(&state, lower, DVec3::Z);
        make_disps(&mut state, &mut tool, &[fu, fl], 2);
        state.doc.clear_face_selection();
        tool.brush.spatial = true;
        tool.brush.radius = 200.0;
        tool.brush.line_of_sight = los;
        state.frame.dt = 0.1;
        let view = view3d(DVec3::new(0.0, 0.0, 600.0), 0.0, -89.0);
        let c = view.rect.center();
        let press =
            PointerFrame { pos: Some(c), pressed: true, down: true, mods: Modifiers::NONE, ..PointerFrame::default() };
        tool.handle_pointer(&mut state, &view, press);
        state.frame.time += 0.5;
        state.frame.index += 1;
        tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, ..press });
        tool.handle_pointer(&mut state, &view, PointerFrame { down: false, pressed: false, ..press });
        assert!(!untouched(&state, fu));
        assert_eq!(untouched(&state, fl), los, "line of sight {los}");
    }
}

#[test]
fn alt_drag_changes_the_brush_radius() {
    let (mut state, mut tool, face) = floor_with_disp(2);
    let view = view3d(DVec3::new(0.0, 0.0, 600.0), 0.0, -89.0);
    let c = view.rect.center();
    tool.brush.radius = 64.0;
    let press = PointerFrame { pos: Some(c), pressed: true, down: true, mods: alt(), ..PointerFrame::default() };
    assert_eq!(tool.handle_pointer(&mut state, &view, press), ToolFlow::Handled);
    let moved = PointerFrame {
        pos: Some(c + vec2(150.0, 40.0)),
        pressed: false,
        down: true,
        mods: alt(),
        ..PointerFrame::default()
    };
    tool.handle_pointer(&mut state, &view, moved);
    assert_eq!(tool.brush.radius, 128.0);
    assert!(tool.status_hint(&state).expect("hint").contains("128"));
    assert_eq!(tool.cursor(&state, &view, c), Some(egui::CursorIcon::ResizeHorizontal));
    tool.handle_pointer(&mut state, &view, PointerFrame { down: false, ..moved });
    assert!(!tool.is_painting());
    assert!(untouched(&state, face), "resizing does not paint");
}

#[test]
fn surface_normal_axis_raises_along_the_slope() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    tool.brush.falloff = Falloff::Linear;
    tool.brush.radius = 200.0;
    tool.brush.strength = 300.0;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.end_stroke(&mut state);
    let before = mesh(&state, face);
    tool.brush.axis = PaintAxis::SubdivNormal;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(100.0, 0.0, 0.0), false) > 0);
    tool.end_stroke(&mut state);
    let after = mesh(&state, face);
    let sideways = before.positions.iter().zip(&after.positions).any(|(p, q)| (q.x - p.x).abs() > 0.1);
    assert!(sideways);
    assert_tags_current(&state, face);
}

#[test]
fn panels_and_noise_window_render() {
    let (mut state, mut tool, face) = floor_with_disp(2);
    state.doc.select_faces(&[face], SelectOp::Replace);
    commands::set_flag(&mut state, hp_geom::DISP_FLAG_NO_HULL, true);
    for axis in [PaintAxis::SubdivNormal, PaintAxis::FaceNormal] {
        tool.brush.axis = axis;
        run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    }
    state.runtime.mesh.noise_dialog = true;
    run_ctx(&mut state, |ctx, st| crate::disp::terrain_windows::noise_window(ctx, st, Some(&mut tool)));
    assert!(state.runtime.mesh.noise_dialog, "stays open until closed");
    state.runtime.mesh.noise_dialog = false;
    for which in [hp_app::runtime::ErosionDialog::Thermal, hp_app::runtime::ErosionDialog::Hydraulic] {
        state.runtime.mesh.erosion_dialog = Some(which);
        run_ctx(&mut state, |ctx, st| crate::disp::terrain_windows::erosion_window(ctx, st, Some(&mut tool)));
        assert_eq!(state.runtime.mesh.erosion_dialog, Some(which));
    }
    let (mut state, mut tool, _) = box_and_tool();
    tool.select_at(&state, &[DVec3::new(0.0, 0.0, 64.0), DVec3::new(128.0, 128.0, 64.0)]);
    tool.action(&mut state, Action::VertexSplitFace);
    for mode in [VertexMode::Both, VertexMode::Vertices, VertexMode::Edges] {
        tool.mode = mode;
        run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
        assert!(tool.status_hint(&state).is_some());
    }
    tool.select_all_vertices(&state);
    tool.start_scale(&mut state);
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    assert!(tool.is_scaling());
    tool.deactivate(&mut state);
    assert!(!tool.is_scaling());
}

#[test]
fn drags_show_their_result_live_and_escape_reverts() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let before = vmf(&state);
    let front = view2d(ViewKind::Front, DVec2::new(264.0, 64.0), 2.0);
    tool.select_at(
        &state,
        &[
            DVec3::new(-256.0, 256.0, 128.0),
            DVec3::new(256.0, 256.0, 128.0),
            DVec3::new(-256.0, 272.0, 128.0),
            DVec3::new(256.0, 272.0, 128.0),
        ],
    );
    let grab = front.world_to_screen(DVec3::new(0.0, 256.0, 128.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab + vec2(0.0, -64.0) }, Modifiers::NONE);
    assert!(state.doc.in_transaction());
    assert_eq!(bounds(&state, WALL).max.z, 160.0);
    tool.handle(&mut state, &front, Gesture::Drag(grab), Modifiers::NONE);
    assert_eq!(bounds(&state, WALL).max.z, 128.0);
    let flat = front.world_to_screen(DVec3::new(0.0, 256.0, 0.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::Drag(flat), Modifiers::NONE);
    assert_eq!(bounds(&state, WALL).max.z, 128.0);
    tool.handle(&mut state, &front, Gesture::Drag(grab + vec2(0.0, -64.0)), Modifiers::NONE);
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::Escape, Modifiers::NONE)]));
    assert!(!state.doc.in_transaction());
    assert_eq!(vmf(&state), before);
    assert_eq!(undo_steps(&state), 0);
    tool.select_all_vertices(&state);
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab + vec2(32.0, 0.0) }, Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::DragEnd(grab + vec2(32.0, 0.0)), Modifiers::NONE);
    assert!(!state.doc.in_transaction());
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(bounds(&state, WALL).min.y, 272.0);
    let after = vmf(&state);
    let grab = front.world_to_screen(DVec3::new(0.0, 272.0, 128.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab + vec2(32.0, 0.0) }, Modifiers::NONE);
    assert!(state.doc.in_transaction());
    tool.deactivate(&mut state);
    assert!(!state.doc.in_transaction());
    assert_eq!(vmf(&state), after);
    state.settings.mesh.live_preview = false;
    tool.activate(&mut state);
    tool.select_all_vertices(&state);
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab + vec2(32.0, 0.0) }, Modifiers::NONE);
    assert!(!state.doc.in_transaction());
    assert_eq!(vmf(&state), after);
    tool.handle(&mut state, &front, Gesture::DragEnd(grab + vec2(32.0, 0.0)), Modifiers::NONE);
    assert_eq!(undo_steps(&state), 2);
}

#[test]
fn a_replaced_map_ends_a_live_drag_safely() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    tool.select_all_vertices(&state);
    let front = view2d(ViewKind::Front, DVec2::new(264.0, 64.0), 2.0);
    let grab = front.world_to_screen(DVec3::new(0.0, 256.0, 128.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab + vec2(32.0, 0.0) }, Modifiers::NONE);
    assert!(state.doc.in_transaction());
    let map = hp_vmf::parse(hp_app::testing::TEST_VMF).expect("map");
    state.doc = hp_editor::Document::from_map(map);
    state.doc.select(&[ObjRef::Solid(WALL)], SelectOp::Replace);
    let fresh = vmf(&state);
    tool.handle(&mut state, &front, Gesture::DragEnd(grab + vec2(64.0, 0.0)), Modifiers::NONE);
    assert!(!state.doc.in_transaction());
    assert_eq!(vmf(&state), fresh, "the new map is untouched");
    assert!(!tool.is_busy());
    assert_eq!(undo_steps(&state), 0);
}

#[test]
fn four_way_blend_painting_writes_multiblend_data() {
    let (mut state, mut tool, face) = floor_with_disp(2);
    tool.interaction = Interaction::Alpha;
    tool.brush.blend_channel = Some(2);
    tool.brush.falloff = Falloff::Constant;
    tool.brush.radius = 2000.0;
    let s = build_sample(&tool.brush, Interaction::Alpha, DVec3::ZERO, false, 0.1).expect("sample");
    let Sample::Local(b) = s else { panic!("local sample") };
    assert!(matches!(b.op, LocalOp::Blend { channel: 2, amount } if amount > 0.0));
    let steps = undo_steps(&state);
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    assert_eq!(tool.paint_at(&mut state, DVec3::ZERO, false), 25);
    tool.end_stroke(&mut state);
    assert_eq!(undo_steps(&state), steps + 1);
    let disp = state.doc.face_side(face).and_then(|s| s.disp.as_deref()).expect("disp").clone();
    let blend = hp_vmf::multiblend::MultiBlend::of(&disp).expect("4-way data created");
    assert!(blend.weights.iter().all(|w| w[2] > 0.0 && (w.iter().sum::<f64>() - 1.0).abs() < 1e-9));
    assert!(alphas(&state, face).iter().all(|a| *a == 0.0), "the 2-way alpha is untouched");
    assert!(state.doc.to_vmf_string().contains("multiblend_color_3"));
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, true);
    tool.end_stroke(&mut state);
    let after =
        hp_vmf::multiblend::MultiBlend::of(state.doc.face_side(face).and_then(|s| s.disp.as_deref()).expect("disp"))
            .expect("data");
    assert!(after.weights[0][2] < blend.weights[0][2]);
    let view = view3d(DVec3::new(0.0, 0.0, 600.0), 0.0, -89.0);
    tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(view.rect.center()), ..PointerFrame::default() });
    let mut out = hp_render::Overlay::default();
    tool.overlay(&state, &view, &mut out);
    assert!(out.points.len() >= 25);
    state.settings.mesh.four_way_blends = false;
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    assert_eq!(tool.brush.blend_channel, None);
}

fn heights(state: &AppState, f: FaceRef) -> Vec<f64> {
    mesh(state, f).positions.iter().map(|p| p.z).collect()
}

#[test]
fn sculpt_push_reaches_a_layer_and_does_not_pile_up() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    tool.interaction = Interaction::Sculpt;
    tool.brush.falloff = Falloff::Constant;
    tool.brush.radius = 100.0;
    tool.brush.sculpt.distance = 24.0;
    let steps = undo_steps(&state);
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::ZERO, false) > 0);
    assert_eq!(tool.paint_at(&mut state, DVec3::ZERO, false), 0);
    tool.end_stroke(&mut state);
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(state.doc.undo_label(), Some("Sculpt displacement"));
    let h = heights(&state, face);
    assert_eq!(h.iter().copied().fold(f64::MIN, f64::max), 24.0);
    assert!(h.contains(&0.0), "outside the brush");
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.end_stroke(&mut state);
    assert_eq!(heights(&state, face).iter().copied().fold(f64::MIN, f64::max), 48.0);
    tool.brush.sculpt.push_mode = crate::disp::sculpt::PushMode::Adaptive;
    tool.brush.sculpt.percent = 10.0;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::new(200.0, 200.0, 0.0), true);
    tool.end_stroke(&mut state);
    assert_eq!(heights(&state, face).iter().copied().fold(f64::MAX, f64::min), -10.0);
    assert_tags_current(&state, face);
}

#[test]
fn sculpt_carve_digs_along_the_stroke_path() {
    let (mut state, mut tool, face) = floor_with_disp(4);
    tool.interaction = Interaction::Sculpt;
    tool.brush.sculpt.op = crate::disp::sculpt::SculptOp::Carve;
    tool.brush.sculpt.carve_depth = 20.0;
    tool.brush.sculpt.profile = crate::disp::sculpt::PROFILE_FLAT;
    tool.brush.radius = 40.0;
    tool.begin_stroke(&mut state, 0);
    for i in 0..=10 {
        tool.paint_at(&mut state, DVec3::new(-200.0 + 40.0 * i as f64, 0.0, 0.0), false);
    }
    tool.end_stroke(&mut state);
    let m = mesh(&state, face);
    for (b, p) in m.base_positions.iter().zip(&m.positions) {
        if b.y.abs() < 1.0 && b.x.abs() <= 200.0 {
            assert!((p.z + 20.0).abs() < 1e-9, "on the path: {b} -> {p}");
        }
        if b.y.abs() > 64.0 {
            assert_eq!(p.z, 0.0, "away from the path: {b}");
        }
    }
}

#[test]
fn sculpt_project_moves_the_surface_onto_selected_brushes() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    let slab = block(&mut state, DVec3::new(-64.0, -64.0, 40.0), DVec3::new(64.0, 64.0, 48.0));
    state.doc.select(&[ObjRef::Solid(FLOOR), ObjRef::Solid(slab)], SelectOp::Replace);
    tool.interaction = Interaction::Sculpt;
    tool.brush.sculpt.op = crate::disp::sculpt::SculptOp::Project;
    tool.brush.falloff = Falloff::Constant;
    tool.brush.radius = 300.0;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::ZERO, false) > 0);
    tool.end_stroke(&mut state);
    let m = mesh(&state, face);
    for (b, p) in m.base_positions.iter().zip(&m.positions) {
        if b.x.abs() <= 64.0 && b.y.abs() <= 64.0 {
            assert!((p.z - 40.0).abs() < 1e-9, "under the slab: {b} -> {p}");
        } else {
            assert_eq!(p.z, 0.0, "nothing above: {b}");
        }
    }
    state.doc.select(&[ObjRef::Solid(FLOOR)], SelectOp::Replace);
    tool.begin_stroke(&mut state, 0);
    assert_eq!(tool.paint_at(&mut state, DVec3::new(100.0, 100.0, 0.0), false), 0);
    tool.end_stroke(&mut state);
}

#[test]
fn sculpt_shift_smooths_and_panel_renders() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    tool.interaction = Interaction::Sculpt;
    tool.brush.falloff = Falloff::Constant;
    tool.brush.radius = 40.0;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.end_stroke(&mut state);
    let before = heights(&state, face).iter().copied().fold(f64::MIN, f64::max);
    tool.brush.radius = 200.0;
    tool.brush.smooth = 1.0;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    let shift = Modifiers { shift: true, ..Modifiers::default() };
    assert!(tool.paint_at_mods(&mut state, DVec3::ZERO, shift) > 0);
    tool.end_stroke(&mut state);
    assert!(heights(&state, face).iter().copied().fold(f64::MIN, f64::max) < before);
    for op in [
        crate::disp::sculpt::SculptOp::Push,
        crate::disp::sculpt::SculptOp::Carve,
        crate::disp::sculpt::SculptOp::Project,
    ] {
        tool.brush.sculpt.op = op;
        run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
        assert!(tool.status_hint(&state).is_some());
    }
    tool.brush.axis = PaintAxis::SubdivNormal;
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    assert_eq!(tool.brush.axis, PaintAxis::FaceNormal);
}

#[test]
fn a_rubber_band_keeps_its_corner_while_the_2d_view_scrolls() {
    let (mut state, mut tool, _) = box_and_tool();
    tool.mode = VertexMode::Vertices;
    let top = view2d(ViewKind::Top, DVec2::new(64.0, 64.0), 1.0);
    let origin = top.world_to_screen(DVec3::new(-32.0, 160.0, 0.0)).expect("screen");
    let cursor = top.world_to_screen(DVec3::new(40.0, -32.0, 0.0)).expect("screen");
    tool.handle(&mut state, &top, Gesture::DragStart { origin, pos: origin + vec2(8.0, 8.0) }, Modifiers::NONE);
    tool.handle(&mut state, &top, Gesture::Drag(cursor), Modifiers::NONE);
    let mut scrolled = top;
    scrolled.cam2d.center.x += 100.0;
    tool.handle(&mut state, &scrolled, Gesture::Drag(cursor), Modifiers::NONE);
    tool.handle(&mut state, &scrolled, Gesture::DragEnd(cursor), Modifiers::NONE);
    assert_eq!(tool.selected_vertex_count(&state), 8, "the band covers the whole box from its pressed corner");
}
