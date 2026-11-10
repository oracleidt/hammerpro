#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

use egui::{Modifiers, Pos2, Rect, vec2};
use glam::{DVec2, DVec3};
use hp_app::state::AppState;
use hp_app::testing::{run_ctx, run_ui, test_state};
use hp_app::view::{Camera2d, Camera3d};
use hp_app::{Action, Tool, ToolFlow, ToolId, ViewInfo, ViewKind};
use hp_editor::{FaceRef, ObjRef, SelectOp};
use hp_geom::{Falloff, PaintMode, Ray};

use crate::common::Gesture;
use crate::disp::paint::{
    LocalBrush, LocalOp, PaintDir, Sample, build_sample, create_targets, paint_disp_local, pick_disp, surface_ring,
};
use crate::disp::{DisplacementTool, PointerFrame};
use crate::vertex::model::{
    DragKind, EditSolid, HandleRef, Pickable, SolidEdit, UNDO_EDIT, apply_edits, drag_point, edge_group, moved_points,
    pick_handle, preview_edges, snap_delta, unique_edges, vertex_group, vertices_in_rect,
};
use crate::{AlphaEffect, GeoEffect, Interaction, PaintAxis, VertexTool};

mod disp_blend_slope;
mod disp_brushes;
mod disp_erode;
mod disp_feedback;
mod disp_live;
mod disp_options;
mod disp_subdivision;
mod edge_loops;
mod edit_tools;
mod fuzz;
mod roadblend;
mod weld;

const FLOOR: u32 = 2;
const WALL: u32 = 3;

fn rect() -> Rect {
    Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))
}

fn view2d(kind: ViewKind, center: DVec2, zoom: f64) -> ViewInfo {
    let index = match kind {
        ViewKind::Top => 1,
        ViewKind::Front => 2,
        ViewKind::Side => 3,
        ViewKind::Perspective => 0,
    };
    ViewInfo {
        index,
        kind,
        rect: rect(),
        cam3d: Camera3d::default(),
        cam2d: Camera2d { center, zoom },
        pixels_per_point: 1.0,
        grid: 16.0,
    }
}

fn view3d(pos: DVec3, yaw: f64, pitch: f64) -> ViewInfo {
    ViewInfo {
        index: 0,
        kind: ViewKind::Perspective,
        rect: rect(),
        cam3d: Camera3d { pos, yaw_deg: yaw, pitch_deg: pitch, ..Camera3d::default() },
        cam2d: Camera2d::default(),
        pixels_per_point: 1.0,
        grid: 16.0,
    }
}

fn state_with(solids: &[u32]) -> AppState {
    let mut state = test_state();
    crate::register_translations();
    let objs: Vec<ObjRef> = solids.iter().map(|id| ObjRef::Solid(*id)).collect();
    state.doc.select(&objs, SelectOp::Replace);
    state
}

fn vmf(state: &AppState) -> String {
    state.doc.to_vmf_string()
}

fn undo_steps(state: &AppState) -> usize {
    state.doc.history_len().0
}

fn ctrl() -> Modifiers {
    Modifiers { ctrl: true, command: true, ..Modifiers::default() }
}

fn run_keys(state: &mut AppState, tool: &mut dyn Tool, keys: &[(egui::Key, Modifiers)]) -> bool {
    let ctx = egui::Context::default();
    let mods = keys.first().map(|k| k.1).unwrap_or_default();
    let mut events = vec![egui::Event::ModifiersChanged(mods)];
    events.extend(keys.iter().map(|(key, m)| egui::Event::Key {
        key: *key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: *m,
    }));
    let input = egui::RawInput { screen_rect: Some(rect()), events, ..Default::default() };
    let mut handled = false;
    let mut out = ctx.run_ui(input, |ui| {
        handled = tool.key_input(state, ui.ctx());
    });
    out.textures_delta.clear();
    handled
}

const BOTH: Pickable = Pickable { vertices: true, edges: true };
const VERTICES: Pickable = Pickable { vertices: true, edges: false };
const NO_LOCK: hp_geom::VertexEditOptions = hp_geom::VertexEditOptions { uv_lock: false };

fn has_toast(state: &mut AppState) -> bool {
    !state.toasts.live().is_empty()
}

#[test]
fn translation_table_is_clean() {
    let problems = hp_app::i18n::lint_table(crate::TR);
    assert!(problems.is_empty(), "{problems:#?}");
    let core: std::collections::HashSet<&str> = hp_app::strings::TR.iter().map(|e| e.0).collect();
    for (k, _, _) in crate::TR {
        assert!(!core.contains(k), "key {k} collides with hp-app");
        assert!(k.starts_with("vertex.") || k.starts_with("disp.") || k.starts_with("mesh."), "bad prefix {k}");
    }
}

#[test]
fn translation_keys_match_sources() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    fn sources(dir: &std::path::Path, out: &mut String) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).expect("source dir").flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
            if p.is_dir() {
                if name != "tests" {
                    sources(&p, out);
                }
            } else if name.ends_with(".rs") && name != "tr.rs" && name != "tests.rs" {
                out.push_str(&std::fs::read_to_string(&p).expect("source file"));
            }
        }
    }
    let mut text = String::new();
    sources(&dir, &mut text);
    let tables: [hp_app::i18n::TrTable; 7] = [
        crate::TR,
        crate::disp::erode::TR,
        crate::terrain::tr::TR,
        crate::vertex::loops::TR,
        crate::disp::loops::TR,
        crate::disp::blend::TR,
        crate::disp::slope_ui::TR,
    ];
    let ours: std::collections::HashSet<&str> = tables.iter().flat_map(|t| t.iter().map(|e| e.0)).collect();
    let core: std::collections::HashSet<&str> = hp_app::strings::TR.iter().map(|e| e.0).collect();
    for prefix in ["\"vertex.", "\"disp.", "\"mesh."] {
        for (pos, _) in text.match_indices(prefix) {
            let rest = &text[pos + 1..];
            let key = &rest[..rest.find('"').expect("closing quote")];
            if key.ends_with('.') {
                continue;
            }
            assert!(ours.contains(key), "missing translation {key}");
        }
    }
    for mac in ["t!(\"", "tf!(\""] {
        for (pos, _) in text.match_indices(mac) {
            if text[..pos].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = &text[pos + mac.len()..];
            let key = &rest[..rest.find('"').expect("closing quote")];
            assert!(ours.contains(key) || core.contains(key), "unknown key {key}");
        }
    }
    for k in &ours {
        assert!(text.contains(&format!("\"{k}\"")), "unused translation {k}");
    }
}

#[test]
fn french_is_available() {
    crate::register_translations();
    assert_eq!(hp_app::i18n::tr_in("disp.effect.raise", hp_app::i18n::Lang::Fr), "Élever");
    assert_eq!(hp_app::i18n::tr_in("vertex.merge", hp_app::i18n::Lang::En), "Merge distance");
}

#[test]
fn handles_are_unique_with_edge_midpoints() {
    let state = state_with(&[WALL]);
    let s = EditSolid::from_doc(&state.doc, WALL).expect("wall");
    assert_eq!(s.points.len(), 8, "a box has 8 unique vertices");
    assert_eq!(s.edges.len(), 12, "and 12 unique edges");
    let mut mids: Vec<DVec3> = (0..s.edges.len()).filter_map(|e| s.midpoint(e)).collect();
    assert_eq!(mids.len(), 12);
    mids.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)).then(a.z.total_cmp(&b.z)));
    mids.dedup();
    assert_eq!(mids.len(), 12, "edge midpoints are distinct");
    assert!(mids.contains(&DVec3::new(0.0, 256.0, 128.0)));
    let edges = unique_edges(&[vec![0, 1, 2, 3], vec![1, 0, 4, 5]]);
    assert_eq!(edges.len(), 7);
    assert!(edges.iter().all(|[a, b]| a < b));
}

#[test]
fn picking_and_2d_groups() {
    let state = state_with(&[WALL]);
    let solids = vec![EditSolid::from_doc(&state.doc, WALL).expect("wall")];
    let top = view2d(ViewKind::Top, DVec2::new(0.0, 264.0), 1.0);
    let corner = top.world_to_screen(DVec3::new(256.0, 256.0, 0.0)).expect("screen");
    let Some(HandleRef::Vertex { solid, index }) = pick_handle(&solids, &top, corner + vec2(3.0, 2.0), BOTH) else {
        panic!("vertex expected");
    };
    let group = vertex_group(&solids, &top, solid, index);
    assert_eq!(group.len(), 2);
    let zs: Vec<f64> = group.iter().map(|&(s, i)| solids[s].points[i].z).collect();
    assert!(zs.contains(&0.0) && zs.contains(&128.0));
    let v3 = view3d(DVec3::new(0.0, -300.0, 300.0), 90.0, -30.0);
    assert_eq!(vertex_group(&solids, &v3, solid, index).len(), 1);
    let mid = top.world_to_screen(DVec3::new(0.0, 256.0, 0.0)).expect("screen");
    let Some(HandleRef::Edge { solid, edge }) = pick_handle(&solids, &top, mid, BOTH) else {
        panic!("edge expected");
    };
    assert_eq!(edge_group(&solids, &top, solid, edge).len(), 2);
    assert!(pick_handle(&solids, &top, mid, VERTICES).is_none(), "edge handles can be hidden");
    assert!(pick_handle(&solids, &top, Pos2::new(5.0, 5.0), BOTH).is_none());
    let r = Rect::from_center_size(corner, vec2(10.0, 10.0));
    assert_eq!(vertices_in_rect(&solids, &top, r).len(), 2);
}

#[test]
fn drag_math_in_each_2d_view() {
    for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
        let view = view2d(kind, DVec2::new(10.0, -20.0), 1.0);
        let anchor = DVec3::new(64.0, 64.0, 64.0);
        let k = DragKind::View2d(kind);
        let origin = view.world_to_screen(anchor).expect("screen");
        let start = drag_point(k, &view, anchor, origin).expect("start");
        let now = drag_point(k, &view, anchor, origin + vec2(20.0, -36.0)).expect("now");
        let raw = now - start;
        let (r, u, d) = kind.axes_2d().expect("2d");
        assert!((raw[r] - 20.0).abs() < 1e-9 && (raw[u] - 36.0).abs() < 1e-9 && raw[d] == 0.0, "{kind:?}");
        let snapped = snap_delta(anchor, raw, Some(16.0), k.mask());
        assert_eq!(snapped[r], 16.0, "{kind:?}");
        assert_eq!(snapped[u], 32.0, "{kind:?}");
        assert_eq!(snapped[d], 0.0, "{kind:?}");
        let free = snap_delta(anchor, raw, None, k.mask());
        assert!((free[r] - 20.0).abs() < 1e-9 && (free[u] - 36.0).abs() < 1e-9);
    }
    let off = DVec3::new(3.0, 5.0, 0.0);
    let d = snap_delta(off, DVec3::new(7.0, 9.0, 0.0), Some(16.0), [true, true, true]);
    assert_eq!(d, DVec3::new(0.0, 11.0, 0.0));
    assert_eq!(DragKind::Axis(1).mask(), [false, true, false]);
    let spec = hp_app::settings::GridSpec {
        name: "R".into(),
        step: [12.0, 8.0, 16.0],
        origin: [4.0, 0.0, 0.0],
        angles: [0.0, 30.0, 0.0],
        ..Default::default()
    };
    let frame = hp_app::grid::GridFrame::from_spec(&spec, 16.0);
    let anchor = DVec3::new(10.0, 3.0, 40.0);
    let k = DragKind::View2d(ViewKind::Top);
    let d = crate::vertex::model::snap_delta_on(anchor, DVec3::new(31.0, 17.0, 0.0), Some(&frame), k.mask(), k.plane());
    assert_eq!(d.z, 0.0);
    let l = frame.to_local(DVec3::new(anchor.x + d.x, anchor.y + d.y, 0.0)) / frame.step;
    assert!((l.x - l.x.round()).abs() < 1e-6 && (l.y - l.y.round()).abs() < 1e-6, "{l}");
    let d = crate::vertex::model::snap_delta_on(anchor, DVec3::new(31.0, 17.0, 9.0), Some(&frame), [true; 3], None);
    let l = frame.to_local(anchor + d) / frame.step;
    assert!((l - l.round()).length() < 1e-6, "{l}");
}

#[test]
fn moved_vertices_merge_with_close_vertices() {
    let mut state = state_with(&[WALL]);
    let s = EditSolid::from_doc(&state.doc, WALL).expect("wall");
    let i = s.points.iter().position(|p| *p == DVec3::new(256.0, 256.0, 128.0)).expect("corner");
    let delta = DVec3::new(0.0, 15.5, 0.0);
    let unmerged = moved_points(&s.points, &[i], delta, 0.0);
    assert_eq!(unmerged[i], DVec3::new(256.0, 271.5, 128.0));
    let merged = moved_points(&s.points, &[i], delta, 1.0);
    assert_eq!(merged[i], DVec3::new(256.0, 272.0, 128.0), "snapped onto the neighbour");
    apply_edits(&mut state.doc, &[SolidEdit::points(WALL, merged)], UNDO_EDIT, None, NO_LOCK).expect("valid");
    let after = EditSolid::from_doc(&state.doc, WALL).expect("wall");
    assert_eq!(after.points.len(), 7, "the two vertices became one");
    assert!(state.doc.scene().solid(WALL).expect("wall").geom.valid);
}

#[test]
fn invalid_results_keep_the_original_solid() {
    let mut state = state_with(&[WALL, FLOOR]);
    let before = vmf(&state);
    let s = EditSolid::from_doc(&state.doc, WALL).expect("wall");
    let top: Vec<usize> = (0..s.points.len()).filter(|&i| s.points[i].z == 128.0).collect();
    assert_eq!(top.len(), 4);
    let flat = moved_points(&s.points, &top, DVec3::new(0.0, 0.0, -128.0), 0.0);
    let template = state.doc.scene().solid(WALL).expect("wall").solid.sides.clone();
    assert!(preview_edges(&flat, &template).is_err(), "the preview reports the problem");
    let f = EditSolid::from_doc(&state.doc, FLOOR).expect("floor");
    let up = moved_points(&f.points, &[0], DVec3::new(0.0, 0.0, 8.0), 0.0);
    let err = apply_edits(
        &mut state.doc,
        &[SolidEdit::points(FLOOR, up), SolidEdit::points(WALL, flat)],
        UNDO_EDIT,
        None,
        NO_LOCK,
    );
    assert!(matches!(err, Err((WALL, _))));
    assert_eq!(vmf(&state), before);
    assert!(!state.doc.can_undo());
    assert!(!state.doc.in_transaction());
}

#[test]
fn set_solid_vertices_round_trip_on_wall() {
    let mut state = state_with(&[WALL]);
    let original = vmf(&state);
    let (pts, faces) = state.doc.solid_vertices(WALL).expect("vertices");
    assert_eq!((pts.len(), faces.len()), (8, 6));
    apply_edits(&mut state.doc, &[SolidEdit::points(WALL, pts.clone())], UNDO_EDIT, None, NO_LOCK)
        .expect("same points");
    let (again, _) = state.doc.solid_vertices(WALL).expect("vertices");
    let mut a = pts.clone();
    let mut b = again;
    let key = |p: &DVec3| (p.x as i64, p.y as i64, p.z as i64);
    a.sort_by_key(key);
    b.sort_by_key(key);
    assert_eq!(a, b);
    let i = pts.iter().position(|p| *p == DVec3::new(-256.0, 272.0, 128.0)).expect("corner");
    let moved = moved_points(&pts, &[i], DVec3::new(0.0, 0.0, 64.0), 0.0);
    apply_edits(&mut state.doc, &[SolidEdit::points(WALL, moved)], UNDO_EDIT, None, NO_LOCK).expect("move");
    assert_eq!(state.doc.scene().solid(WALL).expect("wall").bounds.max.z, 192.0);
    while state.doc.can_undo() {
        state.doc.undo();
    }
    assert_eq!(vmf(&state), original);
}

#[test]
fn split_edge_adds_a_vertex() {
    let mut state = state_with(&[WALL]);
    let s = EditSolid::from_doc(&state.doc, WALL).expect("wall");
    let a = s.points.iter().position(|p| *p == DVec3::new(-256.0, 256.0, 128.0)).expect("a");
    let b = s.points.iter().position(|p| *p == DVec3::new(256.0, 256.0, 128.0)).expect("b");
    let mut pts = s.points.clone();
    let m = hp_geom::split_edge(&mut pts, a, b).expect("split");
    assert_eq!(pts[m], DVec3::new(0.0, 256.0, 128.0));
    let moved = moved_points(&pts, &[m], DVec3::new(0.0, 0.0, 32.0), 0.0);
    apply_edits(&mut state.doc, &[SolidEdit::points(WALL, moved)], UNDO_EDIT, None, NO_LOCK).expect("split");
    let after = EditSolid::from_doc(&state.doc, WALL).expect("wall");
    assert_eq!(after.points.len(), 9);
    assert_eq!(state.doc.scene().solid(WALL).expect("wall").bounds.max.z, 160.0);
}

#[test]
fn clicking_a_solid_selects_it() {
    let mut state = test_state();
    crate::register_translations();
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let top = view2d(ViewKind::Top, DVec2::new(0.0, 264.0), 1.0);
    let center = top.world_to_screen(DVec3::new(0.0, 264.0, 0.0)).expect("screen");
    assert_eq!(tool.handle(&mut state, &top, Gesture::Click(center), Modifiers::NONE), ToolFlow::Handled);
    assert_eq!(state.doc.selected_solid_ids(), vec![WALL]);
    let corner = top.world_to_screen(DVec3::new(-256.0, 272.0, 0.0)).expect("screen");
    tool.handle(&mut state, &top, Gesture::Click(corner), Modifiers::NONE);
    assert_eq!(tool.selected_vertex_count(&state), 2);
    let other = top.world_to_screen(DVec3::new(256.0, 272.0, 0.0)).expect("screen");
    tool.handle(&mut state, &top, Gesture::Click(other), ctrl());
    assert_eq!(tool.selected_vertex_count(&state), 4);
    tool.handle(&mut state, &top, Gesture::Click(other), ctrl());
    assert_eq!(tool.selected_vertex_count(&state), 2);
    tool.handle(&mut state, &top, Gesture::Click(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    assert_eq!(tool.selected_vertex_count(&state), 0);
    assert_eq!(state.doc.selected_solid_ids(), vec![WALL]);
    let floor = top.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).expect("screen");
    tool.selected_only = true;
    tool.handle(&mut state, &top, Gesture::Click(floor), Modifiers::NONE);
    assert_eq!(state.doc.selected_solid_ids(), vec![WALL]);
    tool.selected_only = false;
    tool.handle(&mut state, &top, Gesture::Click(floor), Modifiers::NONE);
    assert_eq!(state.doc.selected_solid_ids(), vec![FLOOR]);
}

#[test]
fn rubber_band_then_drag_moves_vertices_in_one_step() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let front = view2d(ViewKind::Front, DVec2::new(264.0, 64.0), 2.0);
    let a = front.world_to_screen(DVec3::new(0.0, 250.0, 136.0)).expect("screen");
    let b = front.world_to_screen(DVec3::new(0.0, 280.0, 120.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: a, pos: a }, Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::Drag(b), Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::DragEnd(b), Modifiers::NONE);
    assert_eq!(tool.selected_vertex_count(&state), 4, "the four top vertices");
    assert_eq!(undo_steps(&state), 0);
    let grab = front.world_to_screen(DVec3::new(0.0, 256.0, 128.0)).expect("screen");
    let to = grab + vec2(0.0, -64.0);
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab + vec2(0.0, -10.0) }, Modifiers::NONE);
    assert!(tool.is_busy());
    assert!(tool.status_hint(&state).is_some());
    tool.handle(&mut state, &front, Gesture::Drag(to), Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::DragEnd(to), Modifiers::NONE);
    assert!(!tool.is_busy());
    assert_eq!(undo_steps(&state), 1);
    assert_eq!(state.doc.undo_label(), Some(UNDO_EDIT));
    assert_eq!(state.doc.scene().solid(WALL).expect("wall").bounds.max.z, 160.0);
    let sel = tool.selected_vertices(&state);
    assert_eq!(sel.len(), 4);
    assert!(sel.iter().all(|(_, p)| p.z == 160.0));
}

#[test]
fn invalid_drag_is_refused_with_a_message() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let before = vmf(&state);
    let front = view2d(ViewKind::Front, DVec2::new(264.0, 64.0), 2.0);
    let a = front.world_to_screen(DVec3::new(0.0, 250.0, 136.0)).expect("screen");
    let b = front.world_to_screen(DVec3::new(0.0, 280.0, 120.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: a, pos: b }, Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::DragEnd(b), Modifiers::NONE);
    let grab = front.world_to_screen(DVec3::new(0.0, 256.0, 128.0)).expect("screen");
    let to = front.world_to_screen(DVec3::new(0.0, 256.0, 0.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: to }, Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::DragEnd(to), Modifiers::NONE);
    assert_eq!(vmf(&state), before, "the original solid is kept");
    assert_eq!(undo_steps(&state), 0);
    assert!(has_toast(&mut state), "the user is told why");
}

#[test]
fn dragging_an_edge_midpoint_splits_overlapping_edges() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let side = view2d(ViewKind::Side, DVec2::new(0.0, 64.0), 1.0);
    let mid = side.world_to_screen(DVec3::new(0.0, 256.0, 128.0)).expect("screen");
    let to = mid + vec2(0.0, -32.0);
    tool.handle(&mut state, &side, Gesture::DragStart { origin: mid, pos: to }, Modifiers::NONE);
    tool.handle(&mut state, &side, Gesture::DragEnd(to), Modifiers::NONE);
    assert_eq!(undo_steps(&state), 1);
    let after = EditSolid::from_doc(&state.doc, WALL).expect("wall");
    assert_eq!(after.points.len(), 10, "both top edges were split");
    assert_eq!(state.doc.scene().solid(WALL).expect("wall").bounds.max.z, 160.0);
    assert_eq!(tool.selected_vertex_count(&state), 2);
}

#[test]
fn alt_disables_snapping() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let top = view2d(ViewKind::Top, DVec2::new(256.0, 264.0), 1.0);
    let corner = top.world_to_screen(DVec3::new(256.0, 256.0, 0.0)).expect("screen");
    let to = corner + vec2(5.0, 0.0);
    let alt = Modifiers { alt: true, ..Modifiers::default() };
    tool.handle(&mut state, &top, Gesture::DragStart { origin: corner, pos: to }, alt);
    tool.handle(&mut state, &top, Gesture::DragEnd(to), alt);
    assert_eq!(state.doc.scene().solid(WALL).expect("wall").bounds.max.x, 261.0);
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.handle(&mut state, &top, Gesture::DragStart { origin: corner, pos: to }, Modifiers::NONE);
    tool.handle(&mut state, &top, Gesture::DragEnd(to), Modifiers::NONE);
    assert_eq!(undo_steps(&state), 0);
}

#[test]
fn gizmo_drag_moves_along_one_axis() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let v = view3d(DVec3::new(0.0, -200.0, 300.0), 80.0, -25.0);
    let corner = DVec3::new(256.0, 256.0, 128.0);
    let cs = v.world_to_screen(corner).expect("visible");
    tool.handle(&mut state, &v, Gesture::Click(cs), Modifiers::NONE);
    assert_eq!(tool.selected_vertex_count(&state), 1);
    let len = crate::common::world_per_point(&v, corner) * 70.0;
    let tip = v.world_to_screen(corner + DVec3::X * len).expect("tip");
    let grab = cs + (tip - cs) * 0.7;
    let to = cs + (tip - cs) * 1.6;
    tool.handle(&mut state, &v, Gesture::Hover(grab), Modifiers::NONE);
    assert_eq!(tool.cursor(&state, &v, grab), Some(egui::CursorIcon::Move));
    tool.handle(&mut state, &v, Gesture::DragStart { origin: grab, pos: to }, Modifiers::NONE);
    tool.handle(&mut state, &v, Gesture::DragEnd(to), Modifiers::NONE);
    let sel = tool.selected_vertices(&state);
    assert_eq!(sel.len(), 1);
    let p = sel[0].1;
    assert!(p.x > 256.0, "moved along +X: {p:?}");
    assert_eq!((p.y, p.z), (256.0, 128.0));
    assert_eq!(p.x % 16.0, 0.0, "snapped to the grid");
}

#[test]
fn keyboard_select_all_nudge_and_escape() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let top = view2d(ViewKind::Top, DVec2::new(0.0, 264.0), 1.0);
    tool.handle(&mut state, &top, Gesture::Hover(Pos2::new(5.0, 5.0)), Modifiers::NONE);
    state.ui.hovered_view = Some(top.index);
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::A, ctrl())]));
    assert_eq!(tool.selected_vertex_count(&state), 8);
    tool.cancel(&mut state);
    assert!(tool.action(&mut state, Action::SelectAll));
    assert_eq!(tool.selected_vertex_count(&state), 8);
    run_keys(&mut state, &mut tool, &[(egui::Key::ArrowRight, Modifiers::NONE)]);
    run_keys(&mut state, &mut tool, &[(egui::Key::ArrowRight, Modifiers::NONE)]);
    let b = state.doc.scene().solid(WALL).expect("wall").bounds;
    assert_eq!((b.min.x, b.max.x), (-224.0, 288.0));
    assert_eq!(undo_steps(&state), 1, "nudges merge");
    assert_eq!(tool.selected_vertex_count(&state), 8);
    let shift = Modifiers { shift: true, ..Modifiers::default() };
    run_keys(&mut state, &mut tool, &[(egui::Key::ArrowUp, shift)]);
    assert_eq!(state.doc.scene().solid(WALL).expect("wall").bounds.min.y, 257.0);
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::Escape, Modifiers::NONE)]));
    assert_eq!(tool.selected_vertex_count(&state), 0);
    assert!(!tool.cancel(&mut state));
}

#[test]
fn opening_another_map_refreshes_the_vertices() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    tool.select_all_vertices(&state);
    assert_eq!(tool.selected_vertex_count(&state), 8);
    let map = hp_vmf::parse(hp_app::testing::TEST_VMF).expect("map");
    state.doc = hp_editor::Document::from_map(map);
    state.doc.select(&[ObjRef::Solid(FLOOR)], SelectOp::Replace);
    assert_eq!(tool.selected_vertex_count(&state), 0);
    tool.select_all_vertices(&state);
    let sel = tool.selected_vertices(&state);
    assert_eq!(sel.len(), 8);
    assert!(sel.iter().all(|(id, _)| *id == FLOOR));
}

#[test]
fn vertex_tool_ui_smoke() {
    let mut state = state_with(&[WALL, FLOOR]);
    let mut tool = VertexTool::new();
    assert_eq!(tool.id(), ToolId::Vertex);
    assert!(!tool.wants_side_panel());
    tool.activate(&mut state);
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    tool.select_all_vertices(&state);
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    let views = [
        view3d(DVec3::new(0.0, -400.0, 300.0), 90.0, -30.0),
        view2d(ViewKind::Top, DVec2::ZERO, 0.5),
        view2d(ViewKind::Front, DVec2::ZERO, 0.5),
        view2d(ViewKind::Side, DVec2::ZERO, 0.5),
    ];
    for v in &views {
        let mut out = hp_render::Overlay::default();
        tool.overlay(&state, v, &mut out);
        assert!(out.points.len() >= 16, "handles for both solids in {:?}", v.kind);
        let _ = tool.cursor(&state, v, v.rect.center());
    }
    run_ctx(&mut state, |ctx, st| {
        let painter = ctx.layer_painter(egui::LayerId::background());
        for v in &views {
            tool.paint(st, v, &painter);
        }
    });
    assert!(tool.status_hint(&state).is_some());
    let top = views[1];
    let corner = top.world_to_screen(DVec3::new(256.0, 272.0, 0.0)).expect("screen");
    tool.handle(
        &mut state,
        &top,
        Gesture::DragStart { origin: corner, pos: corner + vec2(40.0, 0.0) },
        Modifiers::NONE,
    );
    let mut out = hp_render::Overlay::default();
    tool.overlay(&state, &top, &mut out);
    assert!(!out.lines_on_top.is_empty());
    run_ctx(&mut state, |ctx, st| tool.paint(st, &top, &ctx.layer_painter(egui::LayerId::background())));
    assert!(tool.cancel(&mut state), "escape drops the drag");
    tool.deactivate(&mut state);
    state.doc.clear_selection();
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    assert!(tool.status_hint(&state).is_some());
}

#[test]
fn options_bar_position_edit_and_snap() {
    let mut state = state_with(&[WALL]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    tool.select_all_vertices(&state);
    assert!(tool.move_selection(&mut state, DVec3::new(3.0, 0.0, 0.0), "Move vertices", None));
    assert_eq!(state.doc.scene().solid(WALL).expect("wall").bounds.min.x, -253.0);
    assert!(tool.snap_selection(&mut state));
    assert_eq!(state.doc.scene().solid(WALL).expect("wall").bounds.min.x, -256.0);
    assert!(!tool.snap_selection(&mut state), "already on the grid");
}

fn floor_with_disp(power: u32) -> (AppState, DisplacementTool, FaceRef) {
    let mut state = state_with(&[FLOOR]);
    let mut tool = DisplacementTool::new();
    assert_eq!(tool.create(&mut state, power), 1);
    let faces = state.doc.disp_faces();
    assert_eq!(faces.len(), 1);
    (state, tool, faces[0])
}

fn distances(state: &AppState, f: FaceRef) -> Vec<f64> {
    state.doc.face_side(f).and_then(|s| s.disp.as_ref()).map(|d| d.distances.clone()).unwrap_or_default()
}

fn alphas(state: &AppState, f: FaceRef) -> Vec<f64> {
    state.doc.face_side(f).and_then(|s| s.disp.as_ref()).map(|d| d.alphas.clone()).unwrap_or_default()
}

#[test]
fn samples_follow_the_settings() {
    let mut s = crate::BrushSettings::default();
    let c = DVec3::ZERO;
    assert!(build_sample(&s, Interaction::Select, c, false, 0.016).is_none());
    let Some(Sample::Stock(b)) = build_sample(&s, Interaction::Geometry, c, false, 0.05) else { panic!() };
    assert_eq!(b.mode, PaintMode::Raise);
    assert!((b.amount - 3.2).abs() < 1e-5, "64 units per second for 50 ms");
    let Some(Sample::Stock(b)) = build_sample(&s, Interaction::Geometry, c, true, 5.0) else { panic!() };
    assert_eq!(b.mode, PaintMode::Lower, "shift inverts");
    assert!((b.amount - 6.4).abs() < 1e-6, "frame time is capped");
    s.axis = PaintAxis::Z;
    let Some(Sample::Local(b)) = build_sample(&s, Interaction::Geometry, c, true, 0.05) else { panic!() };
    let LocalOp::Move { dir, amount } = b.op else { panic!("move expected") };
    assert_eq!(dir, PaintDir::Fixed(DVec3::Z));
    assert!((amount + 3.2).abs() < 1e-5);
    s.geo_effect = GeoEffect::Smooth;
    let Some(Sample::Local(b)) = build_sample(&s, Interaction::Geometry, c, false, 1.0) else { panic!() };
    let LocalOp::Smooth { dir, amount } = b.op else { panic!("smooth expected") };
    assert_eq!(dir, PaintDir::Fixed(DVec3::Z), "smoothing follows the paint axis");
    assert!((amount - 0.5 * 0.1 * crate::disp::paint::SMOOTH_RATE).abs() < 1e-6);
    s.axis = PaintAxis::FaceNormal;
    let Some(Sample::Local(b)) = build_sample(&s, Interaction::Geometry, c, false, 1.0) else { panic!() };
    assert!(matches!(b.op, LocalOp::Smooth { dir: PaintDir::FaceNormal, .. }));
    s.axis = PaintAxis::SubdivNormal;
    let Some(Sample::Local(b)) = build_sample(&s, Interaction::Geometry, c, false, 1.0) else { panic!() };
    assert!(matches!(b.op, LocalOp::Smooth { dir: PaintDir::VertexNormal, .. }));
    s.geo_effect = GeoEffect::Raise;
    let Some(Sample::Local(b)) = build_sample(&s, Interaction::Geometry, c, false, 1.0) else { panic!() };
    assert!(matches!(b.op, LocalOp::Move { dir: PaintDir::VertexNormal, .. }));
    s.geo_effect = GeoEffect::Set;
    let Some(Sample::Stock(b)) = build_sample(&s, Interaction::Geometry, c, false, 1.0) else { panic!() };
    assert_eq!(b.mode, PaintMode::Set);
    s.axis = PaintAxis::FaceNormal;
    s.alpha_effect = AlphaEffect::Remove;
    let Some(Sample::Stock(b)) = build_sample(&s, Interaction::Alpha, c, false, 0.1) else { panic!() };
    assert_eq!(b.mode, PaintMode::AlphaPaint);
    assert!(b.amount < 0.0);
    s.alpha_effect = AlphaEffect::Set;
    s.alpha_value = 100.0;
    let Some(Sample::Local(b)) = build_sample(&s, Interaction::Alpha, c, false, 0.1) else { panic!() };
    assert_eq!(b.op, LocalOp::AlphaSet { target: 100.0 });
    s.radius = f64::NAN;
    s.custom_axis = DVec3::ZERO;
    s.sanitize();
    assert!(s.radius.is_finite() && s.custom_axis == DVec3::Z);
}

#[test]
fn local_painter_moves_along_any_axis() {
    let (state, _, face) = floor_with_disp(2);
    let node = state.doc.scene().solid(face.solid).expect("floor");
    let mesh = node.disp_mesh(face.side).expect("mesh").clone();
    let mut disp = (**node.solid.sides[face.side].disp.as_ref().expect("disp")).clone();
    let axis = DVec3::new(1.0, 0.0, 1.0).normalize();
    let brush = LocalBrush {
        center: DVec3::ZERO,
        radius: 1000.0,
        falloff: Falloff::Constant,
        spatial: false,
        op: LocalOp::Move { dir: PaintDir::Fixed(axis), amount: 10.0 },
    };
    assert_eq!(paint_disp_local(&mut disp, &mesh, &brush), 25);
    for (n, d) in disp.normals.iter().zip(&disp.distances) {
        assert!((*n * *d - axis * 10.0).length() < 1e-9);
    }
    let set = LocalBrush { op: LocalOp::SetHeight { axis: DVec3::Z, target: 4.0 }, ..brush };
    paint_disp_local(&mut disp, &mesh, &set);
    for (n, d) in disp.normals.iter().zip(&disp.distances) {
        assert!(((*n * *d).z - 4.0).abs() < 1e-9);
    }
    let alpha =
        LocalBrush { radius: 100.0, falloff: Falloff::Linear, op: LocalOp::AlphaSet { target: 200.0 }, ..brush };
    paint_disp_local(&mut disp, &mesh, &alpha);
    let center = mesh.base_positions.iter().position(|p| p.x == 0.0 && p.y == 0.0).expect("center vertex");
    assert!((disp.alphas[center] - 200.0).abs() < 1e-9);
    assert!(disp.alphas.contains(&0.0), "corners are out of reach");
}

#[test]
fn create_targets_and_non_quads() {
    let mut state = state_with(&[FLOOR, WALL]);
    let t = create_targets(&state.doc);
    assert!(!t.from_faces);
    assert_eq!(t.faces.len(), 2, "the top faces of the floor and the wall");
    let params = hp_geom::PrimitiveParams {
        kind: hp_geom::Primitive::Cylinder,
        bounds: hp_geom::Aabb::new(DVec3::new(-600.0, -64.0, 0.0), DVec3::new(-472.0, 64.0, 64.0)),
        ..Default::default()
    };
    let ids = state.doc.create_primitive(&params, hp_editor::Owner::World).expect("cylinder");
    state.doc.select(&[ObjRef::Solid(ids[0])], SelectOp::Replace);
    let t = create_targets(&state.doc);
    assert_eq!((t.faces.len(), t.skipped), (0, 1));
    let mut tool = DisplacementTool::new();
    assert_eq!(tool.create(&mut state, 3), 0);
    assert!(has_toast(&mut state));
    state.doc.select(&[ObjRef::Solid(WALL)], SelectOp::Replace);
    let wall = state.doc.scene().solid(WALL).expect("wall");
    let side = wall.geom.faces.iter().find(|f| f.normal().y < -0.9).expect("front face").side_index;
    state.doc.select_faces(&[FaceRef::new(WALL, side)], SelectOp::Replace);
    let t = create_targets(&state.doc);
    assert!(t.from_faces);
    assert_eq!(t.faces, vec![FaceRef::new(WALL, side)]);
}

#[test]
fn create_and_destroy_through_the_panel_functions() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    assert_eq!(state.doc.selected_faces(), vec![face], "new displacements are selected");
    let mesh = state.doc.scene().solid(face.solid).and_then(|n| n.disp_mesh(face.side).cloned()).expect("mesh");
    assert_eq!(mesh.row_len, 9);
    assert_eq!(state.doc.undo_label(), Some("Create displacement"));
    assert_eq!(tool.set_power(&mut state, 4), 1);
    assert_eq!(state.doc.face_side(face).and_then(|s| s.disp.as_ref()).map(|d| d.power), Some(4));
    assert_eq!(tool.set_elevation(&mut state, 8.0), 1);
    assert_eq!(tool.set_elevation(&mut state, 12.0), 1);
    assert_eq!(state.doc.face_side(face).and_then(|s| s.disp.as_ref()).map(|d| d.elevation), Some(12.0));
    let steps = undo_steps(&state);
    assert_eq!(tool.flatten(&mut state), 1);
    assert_eq!(undo_steps(&state), steps + 1);
    state.doc.clear_face_selection();
    assert_eq!(tool.select_all(&mut state), 1);
    assert_eq!(tool.destroy(&mut state), 1);
    assert!(state.doc.disp_faces().is_empty());
    assert_eq!(tool.destroy(&mut state), 0, "nothing left");
    state.doc.undo();
    assert_eq!(state.doc.disp_faces().len(), 1);
}

#[test]
fn power_change_keeps_the_shape_in_one_undo_step() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    state
        .doc
        .edit_solid(face.solid, |s| {
            if let Some(d) = s.sides.get_mut(face.side).and_then(|s| s.disp.as_mut()) {
                d.distances.iter_mut().for_each(|v| *v = 9.0);
            }
        })
        .expect("sculpt");
    let steps = undo_steps(&state);
    assert_eq!(tool.set_power_with(&mut state, 4, true), 1);
    assert_eq!(undo_steps(&state), steps + 1);
    let d = state.doc.face_side(face).and_then(|s| s.disp.clone()).expect("disp");
    assert_eq!(d.power, 4);
    assert!(d.distances.iter().all(|v| (*v - 9.0).abs() < 1e-9));
    assert_eq!(tool.set_power_with(&mut state, 3, false), 1);
    let d = state.doc.face_side(face).and_then(|s| s.disp.clone()).expect("disp");
    assert!(d.distances.iter().all(|v| *v == 0.0));
}
#[test]
fn brush_stroke_is_one_undo_step() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    state.frame.dt = 1.0 / 30.0;
    let steps = undo_steps(&state);
    tool.begin_stroke(&mut state, 0);
    assert!(tool.is_painting());
    for _ in 0..5 {
        assert!(tool.paint_at(&mut state, DVec3::new(0.0, 0.0, 0.0), false) > 0);
    }
    tool.end_stroke(&mut state);
    assert_eq!(undo_steps(&state), steps + 1, "one step for the whole stroke");
    assert_eq!(state.doc.undo_label(), Some("Paint displacement"));
    let d = distances(&state, face);
    let peak = d.iter().copied().fold(0.0, f64::max);
    assert!((peak - 64.0 / 30.0 * 5.0).abs() < 1e-6, "peak {peak}");
    assert!(d.contains(&0.0), "the corners are outside the brush");
    tool.begin_stroke(&mut state, 0);
    for _ in 0..5 {
        tool.paint_at(&mut state, DVec3::ZERO, true);
    }
    tool.end_stroke(&mut state);
    assert!(distances(&state, face).iter().all(|x| x.abs() < 1e-6));
    state.doc.undo();
    assert!(distances(&state, face).iter().any(|x| *x > 1.0));
    state.doc.undo();
    assert!(distances(&state, face).iter().all(|x| *x == 0.0));
}

#[test]
fn strokes_leave_far_displacements_alone() {
    let (mut state, mut tool, face) = floor_with_disp(2);
    let params = hp_geom::PrimitiveParams {
        bounds: hp_geom::Aabb::new(DVec3::new(-256.0, -256.0, -1032.0), DVec3::new(256.0, 256.0, -1016.0)),
        ..Default::default()
    };
    let ids = state.doc.create_primitive(&params, hp_editor::Owner::World).expect("solid");
    state.doc.select(&[ObjRef::Solid(ids[0])], SelectOp::Replace);
    assert_eq!(tool.create(&mut state, 2), 1);
    let low = state.doc.disp_faces().into_iter().find(|f| f.solid == ids[0]).expect("low disp");
    state.doc.clear_face_selection();
    tool.brush.radius = 300.0;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::ZERO, false) > 0);
    tool.end_stroke(&mut state);
    assert!(distances(&state, face).iter().any(|x| *x > 0.0));
    assert!(distances(&state, low).iter().all(|x| *x == 0.0), "the far displacement is untouched");
    assert_eq!(tool.paint_at(&mut state, DVec3::new(5000.0, 0.0, 0.0), false), 0);
}

#[test]
fn custom_axis_stroke_on_a_wall() {
    let mut state = state_with(&[WALL]);
    let wall = state.doc.scene().solid(WALL).expect("wall");
    let side = wall.geom.faces.iter().find(|f| f.normal().y < -0.9).expect("front face").side_index;
    let face = FaceRef::new(WALL, side);
    state.doc.select_faces(&[face], SelectOp::Replace);
    let mut tool = DisplacementTool::new();
    assert_eq!(tool.create(&mut state, 2), 1);
    tool.brush.axis = PaintAxis::Z;
    tool.brush.radius = 2000.0;
    tool.brush.falloff = Falloff::Constant;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(0.0, 256.0, 64.0), false) > 0);
    tool.end_stroke(&mut state);
    let mesh = state.doc.scene().solid(WALL).and_then(|n| n.disp_mesh(side).cloned()).expect("mesh");
    for (p, b) in mesh.positions.iter().zip(&mesh.base_positions) {
        assert!((p.z - b.z - 6.4).abs() < 1e-6, "moved up along Z");
        assert!((p.y - b.y).abs() < 1e-9, "not along the face normal");
    }
    assert_eq!(undo_steps(&state), 2, "create + stroke");
}

#[test]
fn alpha_painting_changes_alphas() {
    let (mut state, mut tool, face) = floor_with_disp(2);
    tool.interaction = Interaction::Alpha;
    state.frame.dt = 0.5;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::ZERO, false) > 0);
    tool.end_stroke(&mut state);
    let a = alphas(&state, face);
    assert!(a.iter().any(|x| *x > 0.0));
    assert!(a.iter().all(|x| (0.0..=255.0).contains(x)));
    tool.brush.alpha_effect = AlphaEffect::Set;
    tool.brush.alpha_value = 50.0;
    tool.brush.falloff = Falloff::Constant;
    tool.brush.radius = 1000.0;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.end_stroke(&mut state);
    assert!(alphas(&state, face).iter().all(|x| (*x - 50.0).abs() < 1e-9));
    tool.brush.alpha_effect = AlphaEffect::Remove;
    state.doc.select_faces(&[face], SelectOp::Replace);
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.end_stroke(&mut state);
    assert!(alphas(&state, face).iter().all(|x| *x < 50.0));
}

#[test]
fn pointer_stroke_and_click_in_3d() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    state.doc.clear_face_selection();
    let view = view3d(DVec3::new(0.0, 0.0, 600.0), 0.0, -89.0);
    let c = view.rect.center();
    let (o, d) = view.ray(c);
    let hit = pick_disp(&state.doc, &Ray::new(o, d)).expect("hit");
    assert_eq!(hit.face, face);
    let ring = surface_ring(&state.doc, &[hit.face], hit.point, hit.face_normal, 32.0);
    assert_eq!(ring.len(), crate::disp::paint::RING_SEGMENTS + 1);
    assert!(ring.iter().all(|p| (p.z - 0.5).abs() < 1e-6), "on the surface, lifted a little");
    let press =
        PointerFrame { pos: Some(c), pressed: true, down: true, mods: Modifiers::NONE, ..PointerFrame::default() };
    let release =
        PointerFrame { pos: Some(c), pressed: false, down: false, mods: Modifiers::NONE, ..PointerFrame::default() };
    assert_eq!(tool.handle_pointer(&mut state, &view, press), ToolFlow::Handled);
    tool.handle_pointer(&mut state, &view, release);
    assert_eq!(state.doc.selected_faces(), vec![face]);
    assert!(distances(&state, face).iter().all(|x| *x == 0.0), "a click does not paint");
    let ctrl_press = PointerFrame { mods: ctrl(), ..press };
    tool.handle_pointer(&mut state, &view, ctrl_press);
    tool.handle_pointer(&mut state, &view, PointerFrame { mods: ctrl(), ..release });
    assert!(state.doc.selected_faces().is_empty());
    let steps = undo_steps(&state);
    state.frame.dt = 1.0 / 60.0;
    tool.handle_pointer(&mut state, &view, press);
    for i in 1..=4 {
        state.frame.index += 1;
        let at = c + vec2(i as f32 * 3.0, 0.0);
        tool.handle_pointer(
            &mut state,
            &view,
            PointerFrame {
                pos: Some(at),
                pressed: false,
                down: true,
                mods: Modifiers::NONE,
                ..PointerFrame::default()
            },
        );
    }
    assert!(tool.is_painting());
    tool.handle_pointer(&mut state, &view, release);
    assert!(!tool.is_painting());
    assert_eq!(undo_steps(&state), steps + 1);
    assert!(distances(&state, face).iter().any(|x| *x > 0.0));
    let steps = undo_steps(&state);
    tool.handle_pointer(&mut state, &view, press);
    state.frame.time += 0.5;
    state.frame.index += 1;
    tool.handle_pointer(&mut state, &view, PointerFrame { pressed: false, ..press });
    assert!(tool.is_painting());
    tool.handle_pointer(&mut state, &view, release);
    assert_eq!(undo_steps(&state), steps + 1);
}

#[test]
fn select_mode_and_2d_clicks_only_select() {
    let (mut state, mut tool, face) = floor_with_disp(2);
    state.doc.clear_face_selection();
    tool.interaction = Interaction::Select;
    let top = view2d(ViewKind::Top, DVec2::ZERO, 0.5);
    let edge = top.world_to_screen(DVec3::new(0.0, -256.0, 0.0)).expect("screen");
    let press =
        PointerFrame { pos: Some(edge), pressed: true, down: true, mods: Modifiers::NONE, ..PointerFrame::default() };
    tool.handle_pointer(&mut state, &top, press);
    state.frame.time += 1.0;
    tool.handle_pointer(&mut state, &top, PointerFrame { pressed: false, ..press });
    assert!(!tool.is_painting(), "2D views never paint");
    tool.handle_pointer(&mut state, &top, PointerFrame { pressed: false, down: false, ..press });
    assert_eq!(state.doc.selected_faces(), vec![face]);
    assert!(run_keys(&mut state, &mut tool, &[(egui::Key::Escape, Modifiers::NONE)]));
    assert!(state.doc.selected_faces().is_empty());
}

#[test]
fn escape_cancels_a_stroke() {
    let (mut state, mut tool, face) = floor_with_disp(2);
    let steps = undo_steps(&state);
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    assert!(distances(&state, face).iter().any(|x| *x > 0.0));
    assert!(tool.cancel(&mut state));
    assert!(!tool.is_painting());
    assert_eq!(undo_steps(&state), steps);
    assert!(distances(&state, face).iter().all(|x| *x == 0.0));
}

#[test]
fn displacement_panel_smoke() {
    let (mut state, mut tool, _) = floor_with_disp(3);
    assert_eq!(tool.id(), ToolId::Displacement);
    assert!(tool.wants_side_panel());
    tool.activate(&mut state);
    for interaction in [Interaction::Select, Interaction::Geometry, Interaction::Alpha] {
        tool.interaction = interaction;
        for geo in [GeoEffect::Raise, GeoEffect::Lower, GeoEffect::Smooth, GeoEffect::Set] {
            tool.brush.geo_effect = geo;
            for alpha in [AlphaEffect::Add, AlphaEffect::Remove, AlphaEffect::Set] {
                tool.brush.alpha_effect = alpha;
                tool.brush.axis = if alpha == AlphaEffect::Set { PaintAxis::Custom } else { PaintAxis::FaceNormal };
                run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
            }
        }
        assert!(tool.status_hint(&state).is_some());
    }
    state.doc.clear_face_selection();
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    state.doc.clear_selection();
    run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
    let mut empty = test_state();
    run_ui(&mut empty, |ui, st| tool.options_ui(ui, st));
    assert!(tool.status_hint(&empty).is_some());
    tool.interaction = Interaction::Geometry;
    state.doc.select_faces(&state.doc.disp_faces(), SelectOp::Replace);
    let v3 = view3d(DVec3::new(0.0, 0.0, 600.0), 0.0, -89.0);
    tool.handle_pointer(&mut state, &v3, PointerFrame { pos: Some(v3.rect.center()), ..PointerFrame::default() });
    assert_eq!(tool.cursor(&state, &v3, v3.rect.center()), Some(egui::CursorIcon::Crosshair));
    let views = [v3, view2d(ViewKind::Top, DVec2::ZERO, 0.5), view2d(ViewKind::Front, DVec2::ZERO, 0.5)];
    for v in &views {
        let mut out = hp_render::Overlay::default();
        tool.overlay(&state, v, &mut out);
        assert!(out.lines_on_top.len() > 32, "selection outline and brush ring");
    }
    run_ctx(&mut state, |ctx, st| {
        let painter = ctx.layer_painter(egui::LayerId::background());
        for v in &views {
            tool.paint(st, v, &painter);
        }
    });
    tool.deactivate(&mut state);
}

#[test]
fn tools_entry_point() {
    let tools = crate::tools();
    let ids: Vec<ToolId> = tools.iter().map(|t| t.id()).collect();
    assert_eq!(ids, vec![ToolId::Vertex, ToolId::Displacement]);
}

#[test]
fn real_maps_if_present() {
    let mut paths = vec![std::path::PathBuf::from(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf")];
    let examples =
        std::path::Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples");
    if let Ok(dir) = std::fs::read_dir(examples) {
        paths.extend(dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "vmf")));
    }
    for path in paths.iter().filter(|p| p.exists()) {
        let Ok(doc) = hp_editor::Document::open(path) else { continue };
        let mut state = test_state();
        crate::register_translations();
        state.doc = doc;
        state.doc.select_all();
        let mut tool = VertexTool::new();
        tool.activate(&mut state);
        tool.select_all_vertices(&state);
        let top = view2d(ViewKind::Top, DVec2::ZERO, 0.05);
        let mut out = hp_render::Overlay::default();
        tool.overlay(&state, &top, &mut out);
        assert_eq!(out.points.len(), tool.selected_vertex_count(&state), "{}", path.display());
        run_ui(&mut state, |ui, st| tool.options_ui(ui, st));
        let mut disp = DisplacementTool::new();
        run_ui(&mut state, |ui, st| disp.options_ui(ui, st));
        if state.doc.disp_faces().is_empty() {
            let targets = create_targets(&state.doc);
            let some: Vec<FaceRef> = targets.faces.iter().copied().take(4).collect();
            if some.is_empty() {
                continue;
            }
            state.doc.select_faces(&some, SelectOp::Replace);
            assert_eq!(disp.create(&mut state, 3), some.len(), "{}", path.display());
            run_ui(&mut state, |ui, st| disp.options_ui(ui, st));
        }
        if let Some(face) = state.doc.disp_faces().first().copied() {
            let center = state
                .doc
                .scene()
                .solid(face.solid)
                .and_then(|n| n.disp_mesh(face.side))
                .map(|m| m.aabb.center())
                .expect("mesh");
            let before = vmf(&state);
            disp.begin_stroke(&mut state, 0);
            disp.paint_at(&mut state, center, false);
            assert!(disp.cancel(&mut state));
            assert_eq!(vmf(&state), before, "{}", path.display());
        }
    }
}

fn block(state: &mut AppState, min: DVec3, max: DVec3) -> hp_editor::Id {
    let params = hp_geom::PrimitiveParams { bounds: hp_geom::Aabb::new(min, max), ..Default::default() };
    state.doc.create_primitive(&params, hp_editor::Owner::World).expect("block")[0]
}

fn side_facing(state: &AppState, id: hp_editor::Id, dir: DVec3) -> FaceRef {
    let node = state.doc.scene().solid(id).expect("solid");
    let f = node.geom.faces.iter().max_by(|a, b| a.normal().dot(dir).total_cmp(&b.normal().dot(dir))).expect("faces");
    FaceRef::new(id, f.side_index)
}

fn make_disps(state: &mut AppState, tool: &mut DisplacementTool, faces: &[FaceRef], power: u32) {
    state.doc.select_faces(faces, SelectOp::Replace);
    assert_eq!(tool.create(state, power), faces.len());
}

fn mesh(state: &AppState, f: FaceRef) -> hp_geom::DispMesh {
    state.doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)).map(|m| (**m).clone()).expect("mesh")
}

fn positions_where(state: &AppState, f: FaceRef, on: impl Fn(DVec3) -> bool) -> Vec<(DVec3, DVec3)> {
    let m = mesh(state, f);
    m.base_positions.iter().zip(&m.positions).filter(|(b, _)| on(**b)).map(|(b, p)| (*b, *p)).collect()
}

fn untouched(state: &AppState, f: FaceRef) -> bool {
    distances(state, f).iter().all(|d| *d == 0.0)
}

#[test]
fn painting_ignores_stacked_terrain() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let low = block(&mut state, DVec3::new(-256.0, -256.0, 0.0), DVec3::new(256.0, 256.0, 64.0));
    let high = block(&mut state, DVec3::new(-256.0, 0.0, 128.0), DVec3::new(256.0, 256.0, 192.0));
    let (lf, hf) = (side_facing(&state, low, DVec3::Z), side_facing(&state, high, DVec3::Z));
    make_disps(&mut state, &mut tool, &[lf, hf], 3);
    state.frame.dt = 0.1;
    let view = view3d(DVec3::new(0.0, -40.0, 600.0), 90.0, -89.9);
    let c = view.rect.center();
    let (o, d) = view.ray(c);
    assert_eq!(pick_disp(&state.doc, &Ray::new(o, d)).expect("hit").face, lf);
    let press =
        PointerFrame { pos: Some(c), pressed: true, down: true, mods: Modifiers::NONE, ..PointerFrame::default() };
    tool.handle_pointer(&mut state, &view, press);
    for i in 1..=5 {
        state.frame.index += 1;
        let at = c + vec2(i as f32 * 2.0, 0.0);
        tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(at), pressed: false, ..press });
    }
    tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(c), pressed: false, down: false, ..press });
    assert!(!untouched(&state, lf), "the floor under the brush is raised");
    assert!(untouched(&state, hf), "the terrain 128 units above is left alone");
    state.doc.clear_face_selection();
    tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(c), ..PointerFrame::default() });
    let mut out = hp_render::Overlay::default();
    tool.overlay(&state, &view, &mut out);
    assert!(out.lines_on_top.iter().all(|l| l.a[2] < 100.0 && l.b[2] < 100.0), "ring on the floor");
    tool.brush.spatial = true;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(0.0, -40.0, 64.0), false) > 0);
    tool.end_stroke(&mut state);
    assert!(untouched(&state, hf));
}

#[test]
fn painting_ignores_walls() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let floor = block(&mut state, DVec3::new(-256.0, -256.0, 0.0), DVec3::new(256.0, 256.0, 64.0));
    let wall = block(&mut state, DVec3::new(200.0, -256.0, 64.0), DVec3::new(216.0, 256.0, 256.0));
    let edge_wall = block(&mut state, DVec3::new(-272.0, -256.0, 64.0), DVec3::new(-256.0, 256.0, 256.0));
    let ff = side_facing(&state, floor, DVec3::Z);
    let wf = side_facing(&state, wall, DVec3::NEG_X);
    let ef = side_facing(&state, edge_wall, DVec3::X);
    make_disps(&mut state, &mut tool, &[ff, wf, ef], 3);
    state.frame.dt = 0.1;
    tool.brush.radius = 128.0;
    tool.begin_stroke(&mut state, 0);
    for x in [50.0, -200.0] {
        assert!(tool.paint_at(&mut state, DVec3::new(x, 0.0, 64.0), false) > 0);
    }
    tool.end_stroke(&mut state);
    assert!(!untouched(&state, ff));
    assert!(untouched(&state, wf), "the wall standing on the floor");
    assert!(untouched(&state, ef), "the wall sharing the floor edge");
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(200.0, 0.0, 160.0), false) > 0);
    tool.end_stroke(&mut state);
    assert!(!untouched(&state, wf));
}

#[test]
fn painting_crosses_seams_of_terrain_tiles() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let big = block(&mut state, DVec3::new(-512.0, -256.0, -16.0), DVec3::new(0.0, 256.0, 0.0));
    let a = block(&mut state, DVec3::new(0.0, -256.0, -16.0), DVec3::new(256.0, 0.0, 0.0));
    let b = block(&mut state, DVec3::new(0.0, 0.0, -16.0), DVec3::new(256.0, 256.0, 0.0));
    let faces: Vec<FaceRef> = [big, a, b].iter().map(|id| side_facing(&state, *id, DVec3::Z)).collect();
    make_disps(&mut state, &mut tool, &faces, 3);
    tool.brush.radius = 100.0;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(-10.0, 20.0, 0.0), false) > 0);
    tool.end_stroke(&mut state);
    for f in &faces {
        assert!(!untouched(&state, *f), "{f:?} is under the brush");
    }
    let seam = |p: DVec3| p.x.abs() < 1e-6;
    let left = positions_where(&state, faces[0], seam);
    for f in &faces[1..] {
        for (base, p) in positions_where(&state, *f, seam) {
            if let Some((_, q)) = left.iter().find(|(b, _)| b.distance(base) < 1e-6) {
                assert!(p.distance(*q) < 1e-9, "seam vertex {base:?}: {p:?} vs {q:?}");
            }
        }
    }
}

fn assert_seam_closed(state: &AppState, f: FaceRef, g: FaceRef) -> usize {
    let (mf, mg) = (mesh(state, f), mesh(state, g));
    let mut shared = 0;
    for (b, p) in mf.base_positions.iter().zip(&mf.positions) {
        if let Some(k) = mg.base_positions.iter().position(|q| q.distance(*b) < 1e-6) {
            assert!(p.distance(mg.positions[k]) < 1e-9, "seam vertex {b}: {p} vs {}", mg.positions[k]);
            shared += 1;
        }
    }
    shared
}

fn assert_tags_current(state: &AppState, f: FaceRef) {
    let node = state.doc.scene().solid(f.solid).expect("solid");
    let poly = node.geom.face_for_side(f.side).expect("face").vertices.clone();
    let mut d = (**node.solid.sides[f.side].disp.as_ref().expect("disp")).clone();
    let stored = d.triangle_tags.clone();
    assert!(hp_geom::update_disp_tags(&mut d, &poly));
    assert_eq!(stored, d.triangle_tags, "{f:?}: tags follow the slopes");
}

#[test]
fn smoothing_keeps_seams_closed_and_moves_along_the_axis() {
    for spatial in [false, true] {
        let mut state = state_with(&[]);
        let mut tool = DisplacementTool::new();
        let a = block(&mut state, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(0.0, 256.0, 0.0));
        let b = block(&mut state, DVec3::new(0.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0));
        let faces = [a, b].map(|id| side_facing(&state, id, DVec3::Z));
        make_disps(&mut state, &mut tool, &faces, 3);
        state.doc.clear_face_selection();
        tool.brush.radius = 150.0;
        tool.brush.spatial = spatial;
        state.frame.dt = 0.1;
        tool.begin_stroke(&mut state, 0);
        for x in [-60.0, -40.0, -20.0, 0.0] {
            assert!(tool.paint_at(&mut state, DVec3::new(x, 10.0, 0.0), false) > 0);
        }
        tool.end_stroke(&mut state);
        assert_eq!(assert_seam_closed(&state, faces[0], faces[1]), 9);
        let peak = |state: &AppState| {
            faces.iter().flat_map(|f| mesh(state, *f).positions).map(|p| p.z).fold(f64::MIN, f64::max)
        };
        let raised = peak(&state);
        tool.brush.geo_effect = GeoEffect::Smooth;
        tool.brush.smooth = 1.0;
        let steps = undo_steps(&state);
        tool.begin_stroke(&mut state, 0);
        for _ in 0..8 {
            assert!(tool.paint_at(&mut state, DVec3::new(0.0, 10.0, 0.0), false) > 0);
        }
        tool.end_stroke(&mut state);
        assert_eq!(undo_steps(&state), steps + 1, "one undo step");
        assert!(peak(&state) < raised - 1.0, "spatial {spatial}: the bump is smoothed");
        assert_eq!(assert_seam_closed(&state, faces[0], faces[1]), 9, "spatial {spatial}");
        for f in faces {
            let m = mesh(&state, f);
            for (p, b) in m.positions.iter().zip(&m.base_positions) {
                assert!((p.x - b.x).abs() < 1e-9 && (p.y - b.y).abs() < 1e-9, "{b} moved sideways to {p}");
            }
            assert_tags_current(&state, f);
        }
    }
}

#[test]
fn smoothing_keeps_t_junctions_sewn() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let coarse = block(&mut state, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(0.0, 256.0, 0.0));
    let fine = block(&mut state, DVec3::new(0.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0));
    let faces = [coarse, fine].map(|id| side_facing(&state, id, DVec3::Z));
    make_disps(&mut state, &mut tool, &faces[..1], 2);
    make_disps(&mut state, &mut tool, &faces[1..], 3);
    for (i, f) in faces.iter().enumerate() {
        let n = mesh(&state, *f).row_len;
        state
            .doc
            .edit_solid(f.solid, |s| {
                let d = s.sides[f.side].disp.as_mut().expect("disp");
                for k in 0..n * n {
                    d.normals[k] = DVec3::Z;
                    d.distances[k] = 4.0 + ((k * 7 + i * 3) % 5) as f64 * 6.0;
                }
            })
            .expect("edit");
    }
    state.doc.select_faces(&faces, SelectOp::Replace);
    assert!(tool.sew(&mut state) > 0);
    state.doc.clear_face_selection();
    let on_edge = |state: &AppState| {
        let seam = |b: DVec3| b.x.abs() < 1e-6;
        let mut edge = positions_where(state, faces[0], seam);
        edge.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
        assert_eq!(edge.len(), 5);
        for (base, p) in positions_where(state, faces[1], seam) {
            let w = edge.windows(2).find(|w| w[0].0.y <= base.y + 1e-9 && base.y <= w[1].0.y + 1e-9).expect("window");
            let s = (base.y - w[0].0.y) / (w[1].0.y - w[0].0.y);
            let expected = w[0].1.lerp(w[1].1, s);
            assert!(p.distance(expected) < 1e-6, "{base}: {p} vs {expected}");
        }
    };
    on_edge(&state);
    let before: Vec<hp_geom::DispMesh> = faces.iter().map(|f| mesh(&state, *f)).collect();
    tool.brush.geo_effect = GeoEffect::Smooth;
    tool.brush.smooth = 1.0;
    tool.brush.radius = 200.0;
    tool.brush.axis = PaintAxis::Custom;
    tool.brush.custom_axis = DVec3::new(0.0, 0.0, 2.0);
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    for _ in 0..4 {
        assert!(tool.paint_at(&mut state, DVec3::new(10.0, 30.0, 0.0), false) > 0);
    }
    tool.end_stroke(&mut state);
    on_edge(&state);
    for (f, old) in faces.iter().zip(&before) {
        let m = mesh(&state, *f);
        assert!(m.positions.iter().zip(&old.positions).any(|(p, q)| (p.z - q.z).abs() > 0.5), "{f:?} smoothed");
        for (p, q) in m.positions.iter().zip(&old.positions) {
            assert!((p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() < 1e-9, "only along the custom axis");
        }
    }
}

#[test]
fn sew_closes_cracks_between_equal_powers() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let a = block(&mut state, DVec3::new(-256.0, -128.0, -16.0), DVec3::new(0.0, 128.0, 0.0));
    let b = block(&mut state, DVec3::new(0.0, -128.0, -16.0), DVec3::new(256.0, 128.0, 0.0));
    let (fa, fb) = (side_facing(&state, a, DVec3::Z), side_facing(&state, b, DVec3::Z));
    make_disps(&mut state, &mut tool, &[fa, fb], 3);
    state.doc.select_faces(&[fa], SelectOp::Replace);
    tool.brush.radius = 96.0;
    state.frame.dt = 0.2;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(-8.0, 0.0, 0.0), false) > 0);
    tool.end_stroke(&mut state);
    assert!(untouched(&state, fb));
    let seam = |p: DVec3| p.x.abs() < 1e-6;
    let before: Vec<(DVec3, DVec3)> = positions_where(&state, fa, seam);
    assert!(before.iter().any(|(b, p)| p.z - b.z > 1.0), "the left seam is raised");
    state.doc.select_faces(&[fa, fb], SelectOp::Replace);
    let steps = undo_steps(&state);
    assert!(tool.sew(&mut state) > 0);
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(state.doc.undo_label(), Some("Sew displacements"));
    let right = positions_where(&state, fb, seam);
    for (base, p) in positions_where(&state, fa, seam) {
        let (_, q) = right.iter().find(|(b, _)| b.distance(base) < 1e-6).expect("partner");
        assert!(p.distance(*q) < 1e-9, "{base:?}");
        let (_, old) = before.iter().find(|(b, _)| b.distance(base) < 1e-6).expect("old");
        assert!((p.z - (old.z + base.z) / 2.0).abs() < 1e-9, "average of both sides");
    }
    assert_eq!(tool.sew(&mut state), 0);
    assert_eq!(undo_steps(&state), steps + 1);
    state.doc.select_faces(&[fa], SelectOp::Replace);
    assert_eq!(tool.sew(&mut state), 0);
    assert!(has_toast(&mut state));
    state.doc.undo();
    assert!(untouched(&state, fb));
}

#[test]
fn sew_puts_finer_edges_on_coarser_ones_and_joins_corners() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let ids = [
        block(&mut state, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(0.0, 0.0, 0.0)),
        block(&mut state, DVec3::new(0.0, -256.0, -16.0), DVec3::new(256.0, 0.0, 0.0)),
        block(&mut state, DVec3::new(-256.0, 0.0, -16.0), DVec3::new(0.0, 256.0, 0.0)),
        block(&mut state, DVec3::new(0.0, 0.0, -16.0), DVec3::new(256.0, 256.0, 0.0)),
    ];
    let faces: Vec<FaceRef> = ids.iter().map(|id| side_facing(&state, *id, DVec3::Z)).collect();
    for (f, power) in faces.iter().zip([2, 3, 3, 4]) {
        make_disps(&mut state, &mut tool, &[*f], power);
    }
    for (i, f) in faces.iter().enumerate() {
        let n = mesh(&state, *f).row_len;
        state
            .doc
            .edit_solid(f.solid, |s| {
                let d = s.sides[f.side].disp.as_mut().expect("disp");
                for k in 0..n * n {
                    d.normals[k] = DVec3::Z;
                    d.distances[k] = (i + 1) as f64 * 3.0 + ((k * 7 + i * 3) % 5) as f64;
                }
            })
            .expect("edit");
    }
    state.doc.select_faces(&faces, SelectOp::Replace);
    assert!(tool.sew(&mut state) > 0);
    let corner: Vec<DVec3> =
        faces.iter().flat_map(|f| positions_where(&state, *f, |b| b.length() < 1e-6)).map(|(_, p)| p).collect();
    assert_eq!(corner.len(), 4);
    assert!(corner.iter().all(|p| p.distance(corner[0]) < 1e-9), "{corner:?}");
    let pairs = [(0, 1, 0usize), (0, 2, 1), (1, 3, 1), (2, 3, 0)];
    for (coarse, fine, axis) in pairs {
        let on_seam = |b: DVec3| b[axis].abs() < 1e-6;
        let mut edge = positions_where(&state, faces[coarse], on_seam);
        let along = if axis == 0 { 1 } else { 0 };
        edge.sort_by(|a, b| a.0[along].total_cmp(&b.0[along]));
        for (base, p) in positions_where(&state, faces[fine], on_seam) {
            let t = base[along];
            let Some(w) = edge.windows(2).find(|w| w[0].0[along] <= t + 1e-9 && t <= w[1].0[along] + 1e-9) else {
                continue;
            };
            let s = (t - w[0].0[along]) / (w[1].0[along] - w[0].0[along]);
            let expected = w[0].1.lerp(w[1].1, s);
            assert!(p.distance(expected) < 1e-6, "tiles {coarse}/{fine} at {base:?}: {p:?} vs {expected:?}");
        }
    }
}

#[test]
fn concave_vertex_drag_is_refused() {
    let mut state = state_with(&[]);
    let id = block(&mut state, DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 256.0, 256.0));
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let before = vmf(&state);
    let steps = undo_steps(&state);
    let front = view2d(ViewKind::Front, DVec2::new(128.0, 128.0), 1.0);
    let grab = front.world_to_screen(DVec3::new(0.0, 256.0, 256.0)).expect("screen");
    let to = front.world_to_screen(DVec3::new(0.0, 32.0, 32.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab + vec2(4.0, 4.0) }, Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::Drag(to), Modifiers::NONE);
    let hint = tool.status_hint(&state).expect("hint");
    assert!(hint.contains("concave"), "the preview explains the problem: {hint}");
    tool.handle(&mut state, &front, Gesture::DragEnd(to), Modifiers::NONE);
    assert_eq!(vmf(&state), before, "the corner is kept");
    assert_eq!(undo_steps(&state), steps);
    assert!(has_toast(&mut state), "the user is told why");
    let out = front.world_to_screen(DVec3::new(0.0, 288.0, 288.0)).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab + vec2(4.0, -4.0) }, Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::DragEnd(out), Modifiers::NONE);
    assert_eq!(undo_steps(&state), steps + 1);
    let b = state.doc.scene().solid(id).expect("solid").bounds;
    assert_eq!((b.max.y, b.max.z), (288.0, 288.0));
}

#[test]
fn concave_edits_are_refused_by_apply_and_preview() {
    let mut state = state_with(&[WALL]);
    let before = vmf(&state);
    let s = EditSolid::from_doc(&state.doc, WALL).expect("wall");
    let template = state.doc.scene().solid(WALL).expect("wall").solid.sides.clone();
    let i = s.points.iter().position(|p| *p == DVec3::new(256.0, 256.0, 128.0)).expect("corner");
    let inside = moved_points(&s.points, &[i], DVec3::new(-32.0, 8.0, -64.0), 0.0);
    match preview_edges(&inside, &template) {
        Err(crate::vertex::model::VertexError::Concave(1)) => {}
        other => panic!("expected a concave error, got {other:?}"),
    }
    let on_face = moved_points(&s.points, &[i], DVec3::new(-256.0, 8.0, 0.0), 0.0);
    assert!(preview_edges(&on_face, &template).is_err());
    let err = apply_edits(&mut state.doc, &[SolidEdit::points(WALL, inside)], UNDO_EDIT, None, NO_LOCK);
    assert!(matches!(err, Err((WALL, crate::vertex::model::VertexError::Concave(1)))));
    assert_eq!(vmf(&state), before);
    assert!(!state.doc.can_undo());
    let outward = moved_points(&s.points, &[i], DVec3::new(16.0, 0.0, 16.0), 0.0);
    assert!(preview_edges(&outward, &template).is_ok());
    let mut checked = 0;
    for id in state.doc.scene().solids().keys().copied().collect::<Vec<_>>() {
        let s = EditSolid::from_doc(&state.doc, id).expect("solid");
        let template = state.doc.scene().solid(id).expect("solid").solid.sides.clone();
        assert!(preview_edges(&s.points, &template).is_ok(), "solid {id}");
        checked += 1;
    }
    assert!(checked >= 2);
}

#[test]
fn real_map_solids_accept_their_own_vertices() {
    let mut paths = vec![std::path::PathBuf::from(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf")];
    let examples =
        std::path::Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples");
    if let Ok(dir) = std::fs::read_dir(examples) {
        paths.extend(dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "vmf")));
    }
    for path in paths.iter().filter(|p| p.exists()) {
        let Ok(doc) = hp_editor::Document::open(path) else { continue };
        let mut refused = Vec::new();
        for (id, node) in doc.scene().solids() {
            if !node.geom.valid {
                continue;
            }
            let Some(s) = EditSolid::from_doc(&doc, *id) else { continue };
            if let Err(e) = preview_edges(&s.points, &node.solid.sides) {
                refused.push((*id, format!("{e:?}")));
            }
        }
        assert!(refused.is_empty(), "{}: {refused:?}", path.display());
    }
}

#[test]
fn hints_name_keys_with_localized_labels() {
    use egui::{Key, KeyboardShortcut};
    use hp_app::actions::{key_label, shortcut_label};
    let mut state = state_with(&[FLOOR]);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    let ctrl_a = KeyboardShortcut::new(Modifiers::COMMAND, Key::A);
    assert_eq!(state.keys.shortcut(Action::SelectAll), Some(ctrl_a));
    let hint = tool.status_hint(&state).expect("hint");
    assert!(hint.contains(&shortcut_label(&ctrl_a)), "{hint}");
    let rebound = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::E);
    state.keys.set(Action::SelectAll, Some(rebound));
    let hint = tool.status_hint(&state).expect("hint");
    assert!(hint.contains(&shortcut_label(&rebound)), "{hint}");
    state.keys.set(Action::SelectAll, None);
    let hint = tool.status_hint(&state).expect("hint");
    assert!(hint.contains(&shortcut_label(&ctrl_a)), "{hint}");
    tool.deactivate(&mut state);

    let (mut state, mut tool, _) = floor_with_disp(3);
    tool.begin_stroke(&mut state, 0);
    let hint = tool.status_hint(&state).expect("hint");
    assert!(hint.contains(&key_label(Key::Escape)), "{hint}");
    tool.end_stroke(&mut state);
    for (key, en, fr) in crate::tr::TR.iter().copied() {
        for text in [en, fr] {
            for name in ["Escape", "Échap", "Ctrl+A"] {
                assert!(!text.contains(name), "{key}: name keys with key_label / shortcut_label: {text}");
            }
        }
    }
}
