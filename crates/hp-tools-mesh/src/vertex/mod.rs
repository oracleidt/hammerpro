pub mod loops;
pub mod merge;
pub(crate) mod model;
pub mod weld;
pub mod xform;

mod loop_tool;

use std::cell::RefCell;

use egui::{Align2, Color32, CursorIcon, FontId, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Stroke, StrokeKind};
use glam::DVec3;
use hp_app::actions::{key_label, shortcut_label};
use hp_app::state::AppState;
use hp_app::theme::{AXIS_X, AXIS_Y, AXIS_Z, VIEW_ACCENT, VIEW_ERROR};
use hp_app::widgets::fmt_vec3;
use hp_app::{Action, Severity, Tool, ToolFlow, ToolId, ToolMenuEntry, ViewInfo, ViewInput, t, tf};
use hp_editor::{Id, ObjRef, PickOptions, SelectOp};
use hp_geom::{Ray, VertexEditOptions};

use crate::common::{
    Gesture, gesture, line_on_top, point, rgba, vertex_error_text, view_axis, warning_badge, world_per_point,
};
use model::{
    DragKind, HandleRef, NUDGE_TAG, NudgeKey, Pickable, Preview, SCALE_RANGE, ScaleState, SolidEdit, SplitError,
    UNDO_EDIT, UNDO_NUDGE, UNDO_SCALE, VertexCache, VertexDrag, apply_edits, axis_dir, drag_point, edge_group,
    edge_visible, edges_in_rect, nudge_delta, pick_handle, selected_moving, vertex_group, vertices_in_rect,
};

const GIZMO_PX: f64 = 70.0;
const GIZMO_HIT_PX: f32 = 6.0;
const GIZMO_DEAD_PX: f32 = 12.0;
const SCALE_CENTER_PX: f32 = 10.0;
const MAX_EDGE_HANDLES: usize = 6000;
const POSITION_TAG: &str = "hp_tools_mesh.vertex_position";
const UNDO_POSITION: &str = "Move vertices";
const UNDO_SNAP: &str = "Snap to grid";
const SCALE_STEP: f64 = 0.1;

const HANDLE: Color32 = Color32::from_rgb(232, 232, 238);
const SELECTED: Color32 = Color32::from_rgb(255, 214, 0);
const PREVIEW: Color32 = Color32::from_rgb(255, 214, 90);
const SPLIT: Color32 = Color32::from_rgb(120, 220, 255);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VertexMode {
    #[default]
    Both,
    Vertices,
    Edges,
}

impl VertexMode {
    pub fn next(self) -> Self {
        match self {
            VertexMode::Both => VertexMode::Vertices,
            VertexMode::Vertices => VertexMode::Edges,
            VertexMode::Edges => VertexMode::Both,
        }
    }

    fn pickable(self) -> Pickable {
        Pickable { vertices: self != VertexMode::Edges, edges: self != VertexMode::Vertices }
    }

    fn label(self) -> &'static str {
        match self {
            VertexMode::Both => t!("vertex.mode.both"),
            VertexMode::Vertices => t!("vertex.mode.vertices"),
            VertexMode::Edges => t!("vertex.mode.edges"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HoverTarget {
    Handle(HandleRef),
    Gizmo(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hover {
    view: usize,
    target: HoverTarget,
    generation: u64,
}

#[derive(Clone, Copy, Debug)]
struct RubberBand {
    view: ViewInfo,
    origin: Pos2,
    current: Pos2,
    additive: bool,
}

pub struct VertexTool {
    pub mode: VertexMode,
    pub selected_only: bool,
    pub merge_tolerance: f64,
    pub weld_distance: f64,
    pub weld_target: weld::WeldTarget,
    weld_memo: Option<((u64, u64), usize)>,
    cache: RefCell<VertexCache>,
    drag: Option<VertexDrag>,
    rubber: Option<RubberBand>,
    scale: Option<ScaleState>,
    xform: Option<xform::XformState>,
    loop_hover: Option<loop_tool::LoopHover>,
    live: bool,
    hover: Option<Hover>,
    last_view: Option<ViewInfo>,
    last_warning: Option<(String, u64)>,
    align_armed: Option<(f64, u64, u64, usize)>,
    merge_prompt: Option<merge::PendingMerge>,
}

impl Default for VertexTool {
    fn default() -> Self {
        Self {
            mode: VertexMode::Both,
            selected_only: false,
            merge_tolerance: 1.0,
            weld_distance: weld::DEFAULT_WELD_DISTANCE,
            weld_target: weld::WeldTarget::default(),
            weld_memo: None,
            cache: RefCell::new(VertexCache::default()),
            drag: None,
            rubber: None,
            scale: None,
            xform: None,
            loop_hover: None,
            live: false,
            hover: None,
            last_view: None,
            last_warning: None,
            align_armed: None,
            merge_prompt: None,
        }
    }
}

impl VertexTool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn selected_vertex_count(&mut self, state: &AppState) -> usize {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        cache.selected_count()
    }

    pub fn selected_vertices(&mut self, state: &AppState) -> Vec<(Id, DVec3)> {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        cache.selected_positions()
    }

    pub fn select_all_vertices(&mut self, state: &AppState) {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        cache.select_all();
    }

    pub fn pending_splits(&mut self, state: &AppState) -> usize {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        cache.splits.len()
    }

    pub fn is_busy(&self) -> bool {
        self.drag.is_some() || self.rubber.is_some() || self.scale.is_some() || self.xform.is_some()
    }

    pub fn is_scaling(&self) -> bool {
        self.scale.is_some()
    }

    pub fn scale_factor(&self) -> Option<f64> {
        self.scale.as_ref().map(|s| s.factor)
    }

    fn warn(&mut self, state: &mut AppState, text: String) {
        let frame = state.frame.index;
        let repeated = self.last_warning.as_ref().is_some_and(|(t, f)| *t == text && frame.saturating_sub(*f) < 120);
        if repeated {
            state.log(Severity::Warning, text.clone());
        } else {
            state.notify(Severity::Warning, text.clone());
        }
        self.last_warning = Some((text, frame));
    }

    fn drop_stale(&mut self, state: &AppState) {
        if self.live && !state.doc.in_transaction() {
            self.live = false;
            self.drag = None;
            let cache = self.cache.get_mut();
            cache.frozen = false;
            cache.invalidate();
            cache.refresh(&state.doc);
        }
        let generation = self.cache.get_mut().generation;
        if self.drag.as_ref().is_some_and(|d| d.generation != generation) {
            self.drag = None;
        }
        if self.scale.as_ref().is_some_and(|s| s.generation != generation) {
            self.scale = None;
        }
        if self.xform.as_ref().is_some_and(|x| x.generation != generation) {
            self.xform = None;
        }
        if self.hover.is_some_and(|h| h.generation != generation) {
            self.hover = None;
        }
    }

    pub(crate) fn handle(&mut self, state: &mut AppState, view: &ViewInfo, g: Gesture, mods: Modifiers) -> ToolFlow {
        self.cache.get_mut().refresh(&state.doc);
        self.drop_stale(state);
        if matches!(g, Gesture::Click(_) | Gesture::DragStart { .. }) {
            self.align_armed = None;
        }
        if !matches!(g, Gesture::Away) {
            self.last_view = Some(*view);
        }
        if self.scale.is_some() {
            return self.scale_gesture(state, view, g, mods);
        }
        if self.xform.is_some() {
            return self.xform_gesture(g);
        }
        let owns_drag = self.drag.as_ref().is_some_and(|d| d.view == view.index);
        let owns_rubber = self.rubber.as_ref().is_some_and(|r| r.view.index == view.index);
        match g {
            Gesture::Hover(p) => {
                if owns_drag {
                    self.finish_drag(state);
                }
                if owns_rubber {
                    self.finish_rubber();
                }
                self.update_hover(view, Some(p));
                self.update_loop_hover(view, Some(p), mods);
                ToolFlow::Ignored
            }
            Gesture::Away => {
                if owns_drag {
                    self.finish_drag(state);
                }
                if owns_rubber {
                    self.finish_rubber();
                }
                self.update_hover(view, None);
                self.update_loop_hover(view, None, mods);
                ToolFlow::Ignored
            }
            Gesture::Click(p) => {
                if owns_drag {
                    self.finish_drag(state);
                }
                self.rubber = None;
                self.click(state, view, p, mods);
                self.update_hover(view, Some(p));
                ToolFlow::Handled
            }
            Gesture::DragStart { origin, pos } => {
                self.drag = None;
                self.rubber = None;
                self.start_drag(state, view, origin, mods);
                self.drag_to(state, view, pos, mods);
                ToolFlow::Handled
            }
            Gesture::Drag(p) => {
                if owns_drag || owns_rubber {
                    self.drag_to(state, view, p, mods);
                    ToolFlow::Handled
                } else {
                    ToolFlow::Ignored
                }
            }
            Gesture::DragEnd(p) => {
                if owns_drag || owns_rubber {
                    self.drag_to(state, view, p, mods);
                    self.finish_drag(state);
                    self.finish_rubber();
                    self.update_hover(view, Some(p));
                    ToolFlow::Handled
                } else {
                    ToolFlow::Ignored
                }
            }
        }
    }

    fn pick_target(&self, cache: &VertexCache, view: &ViewInfo, pos: Pos2) -> Option<HoverTarget> {
        let handle = pick_handle(&cache.solids, view, pos, self.mode.pickable());
        if let Some(h @ HandleRef::Vertex { .. }) = handle {
            return Some(HoverTarget::Handle(h));
        }
        if !view.is_2d()
            && let Some(center) = cache.selection_center()
            && let Some(axis) = pick_gizmo(view, center, pos)
        {
            return Some(HoverTarget::Gizmo(axis));
        }
        handle.map(HoverTarget::Handle)
    }

    fn update_hover(&mut self, view: &ViewInfo, pos: Option<Pos2>) {
        if self.drag.is_some() || self.rubber.is_some() || self.scale.is_some() {
            self.hover = None;
            return;
        }
        let generation = self.cache.get_mut().generation;
        let target = match pos {
            Some(p) => {
                let cache = self.cache.borrow();
                self.pick_target(&cache, view, p)
            }
            None => None,
        };
        match target {
            Some(target) => self.hover = Some(Hover { view: view.index, target, generation }),
            None => {
                if self.hover.is_some_and(|h| h.view == view.index) {
                    self.hover = None;
                }
            }
        }
    }

    fn pick_solid(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<Id> {
        let bounds = |e: &hp_vmf::Entity| state.assets.entity_bounds(e);
        if let Some(axis) = view_axis(view.kind) {
            let p = view.screen_to_2d(pos);
            let tol = 4.0 / view.cam2d.zoom.max(1e-9);
            return state.doc.pick_2d(axis, p, tol, &bounds).into_iter().find_map(|o| match o {
                ObjRef::Solid(id) => Some(id),
                ObjRef::Entity(_) => None,
            });
        }
        let (o, d) = view.ray(pos);
        let opts = PickOptions { include_entities: false, ..PickOptions::default() };
        match state.view_pick(&Ray::new(o, d), &opts)?.obj {
            ObjRef::Solid(id) => Some(id),
            ObjRef::Entity(_) => None,
        }
    }

    fn click(&mut self, state: &mut AppState, view: &ViewInfo, p: Pos2, mods: Modifiers) {
        if mods.alt {
            self.alt_click(state, view, p, mods);
            return;
        }
        let ctrl = mods.command || mods.ctrl;
        let handle = pick_handle(&self.cache.get_mut().solids, view, p, self.mode.pickable());
        let cache = self.cache.get_mut();
        match handle {
            Some(HandleRef::Vertex { solid, index }) => {
                let group = vertex_group(&cache.solids, view, solid, index);
                if ctrl {
                    cache.toggle_group(&group);
                } else {
                    cache.clear_selection();
                    cache.set_selected(&group, true);
                }
                return;
            }
            Some(HandleRef::Edge { solid, edge }) => {
                let group = edge_group(&cache.solids, view, solid, edge);
                if ctrl {
                    cache.toggle_edges(&group);
                } else {
                    cache.clear_selection();
                    cache.set_edges_selected(&group, true);
                }
                return;
            }
            None => {}
        }
        let picked = Self::pick_solid(state, view, p);
        let cache = self.cache.get_mut();
        let edited = picked.is_some_and(|id| cache.solids.iter().any(|s| s.id == id));
        let may_select = cache.solids.is_empty() || !self.selected_only;
        match picked {
            Some(id) if may_select && (ctrl || !edited) => {
                let op = if ctrl { SelectOp::Toggle } else { SelectOp::Replace };
                if !ctrl {
                    cache.clear_selection();
                }
                let mode = state.select_mode();
                state.doc.select_picked(ObjRef::Solid(id), mode, op);
            }
            _ => {
                if !ctrl {
                    cache.clear_selection();
                }
            }
        }
    }

    fn start_drag(&mut self, state: &mut AppState, view: &ViewInfo, origin: Pos2, mods: Modifiers) {
        let ctrl = mods.command || mods.ctrl;
        let target = {
            let cache = self.cache.borrow();
            self.pick_target(&cache, view, origin)
        };
        let mode = self.mode;
        let cache = self.cache.get_mut();
        let free_kind = if view.is_2d() {
            DragKind::View2d(view.kind)
        } else if mods.shift {
            DragKind::Axis(2)
        } else {
            DragKind::Plane { normal: view.cam3d.forward() }
        };
        match target {
            Some(HoverTarget::Gizmo(axis)) => {
                let Some(center) = cache.selection_center() else { return };
                let moving = selected_moving(cache);
                self.drag =
                    VertexDrag::new(&state.doc, cache, view, DragKind::Axis(axis), center, origin, moving, false);
            }
            Some(HoverTarget::Handle(HandleRef::Vertex { solid, index })) => {
                let group = vertex_group(&cache.solids, view, solid, index);
                let all_selected = group.iter().all(|&(s, i)| cache.is_selected(s, i));
                if !all_selected {
                    if !ctrl {
                        cache.clear_selection();
                    }
                    cache.set_selected(&group, true);
                }
                let Some(anchor) = cache.point(solid, index) else { return };
                let moving = selected_moving(cache);
                self.drag = VertexDrag::new(&state.doc, cache, view, free_kind, anchor, origin, moving, false);
            }
            Some(HoverTarget::Handle(HandleRef::Edge { solid, edge })) if mode == VertexMode::Edges => {
                let Some(anchor) = cache.solids.get(solid).and_then(|s| s.midpoint(edge)) else { return };
                let group = edge_group(&cache.solids, view, solid, edge);
                let all_selected = group.iter().all(|&(s, e)| cache.is_edge_selected(s, e));
                if !all_selected {
                    if !ctrl {
                        cache.clear_selection();
                    }
                    cache.set_edges_selected(&group, true);
                }
                let moving = selected_moving(cache);
                self.drag = VertexDrag::new(&state.doc, cache, view, free_kind, anchor, origin, moving, false);
            }
            Some(HoverTarget::Handle(HandleRef::Edge { solid, edge })) => {
                let Some(anchor) = cache.solids.get(solid).and_then(|s| s.midpoint(edge)) else { return };
                let group = edge_group(&cache.solids, view, solid, edge);
                let mut moving: Vec<model::MovingSolid> = Vec::new();
                for (si, ei) in group {
                    let Some(s) = cache.solids.get(si) else { continue };
                    let Some([a, b]) = s.edges.get(ei).copied() else { continue };
                    let slot = match moving.iter().position(|(ci, ..)| *ci == si) {
                        Some(k) => k,
                        None => {
                            moving.push((si, s.points.clone(), Vec::new(), s.topology()));
                            moving.len() - 1
                        }
                    };
                    if let Some((_, base, idx, topology)) = moving.get_mut(slot)
                        && let Some(new_index) = hp_geom::split_edge(base, a, b)
                    {
                        idx.push(new_index);
                        if let Some(t) = topology.as_mut() {
                            t.insert_on_edge(a, b, new_index);
                        }
                    }
                }
                self.drag = VertexDrag::new(&state.doc, cache, view, free_kind, anchor, origin, moving, true);
            }
            None => {
                self.rubber = Some(RubberBand { view: *view, origin, current: origin, additive: ctrl });
            }
        }
        self.hover = None;
    }

    fn drag_to(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        let grid = (state.snap_enabled() && !mods.alt).then(|| state.edit_grid_frame(view.kind));
        if let Some(drag) = self.drag.as_mut()
            && drag.view == view.index
        {
            if drag.update(view, pos, grid, self.merge_tolerance) {
                self.update_live(state);
            }
        } else if let Some(r) = self.rubber.as_mut()
            && r.view.index == view.index
        {
            r.origin = view.follow_2d(&r.view, r.origin);
            r.current = pos;
            r.view = *view;
        }
    }

    fn update_live(&mut self, state: &mut AppState) {
        let Some(drag) = self.drag.as_ref() else { return };
        if !state.settings.mesh.live_preview || drag.solids.len() > model::LIVE_MAX_SOLIDS {
            return;
        }
        if !self.live {
            if !drag.has_motion() || state.doc.in_transaction() {
                return;
            }
            state.doc.begin(UNDO_EDIT);
            self.live = true;
            self.cache.get_mut().frozen = true;
        }
        let opts = VertexEditOptions { uv_lock: state.settings.mesh.uv_lock };
        if !model::sync_live(&mut state.doc, &drag.solids, &drag.previews, opts) {
            state.doc.cancel();
            self.live = false;
            self.cache.get_mut().frozen = false;
        }
    }

    fn end_live(&mut self, state: &mut AppState, commit: bool) -> bool {
        if !std::mem::take(&mut self.live) {
            return false;
        }
        let cache = self.cache.get_mut();
        cache.frozen = false;
        cache.invalidate();
        if !state.doc.in_transaction() {
            cache.refresh(&state.doc);
            return false;
        }
        if commit {
            state.doc.commit();
        } else {
            state.doc.cancel();
        }
        self.cache.get_mut().refresh(&state.doc);
        commit
    }

    fn finish_drag(&mut self, state: &mut AppState) {
        let Some(drag) = self.drag.take() else { return };
        let error = drag.first_error().map(vertex_error_text);
        let valid = drag.has_motion() && error.is_none();
        if self.live {
            let (edits, reselect) = drag.edits();
            let asks = state.settings.vertex.merge_prompt != hp_app::settings::MergePrompt::Off;
            if valid && (!asks || merge::find_stacked(&edits).is_none()) {
                let cache = self.cache.get_mut();
                for e in &edits {
                    cache.follow_splits(e);
                }
                cache.pending_edges = Some(loop_tool::edge_followers(&cache.solids, &edits));
                cache.pending_select = Some(reselect);
                self.end_live(state, true);
                self.report_lost_disps(state, &drag.solids);
                return;
            }
            self.end_live(state, false);
        }
        if !drag.has_motion() {
            return;
        }
        if let Some(err) = error {
            let text = tf!("vertex.cancelled", err);
            self.warn(state, text);
            return;
        }
        let (edits, reselect) = drag.edits();
        self.apply_move(state, &edits, reselect, UNDO_EDIT, None);
    }

    fn report_lost_disps(&mut self, state: &mut AppState, solids: &[model::DragSolid]) {
        let before = model::template_disps(solids);
        let after: usize = solids
            .iter()
            .map(|s| {
                state.doc.scene().solid(s.id).map_or(0, |n| n.solid.sides.iter().filter(|x| x.disp.is_some()).count())
            })
            .sum();
        let lost = before.saturating_sub(after);
        if lost > 0 {
            let text = tf!("vertex.disp_lost", lost);
            self.warn(state, text);
        }
    }

    fn apply(
        &mut self,
        state: &mut AppState,
        edits: &[SolidEdit],
        reselect: model::Reselect,
        label: &str,
        merge_tag: Option<&str>,
    ) -> bool {
        let opts = VertexEditOptions { uv_lock: state.settings.mesh.uv_lock };
        match apply_edits(&mut state.doc, edits, label, merge_tag, opts) {
            Ok(report) => {
                let cache = self.cache.get_mut();
                for e in edits {
                    cache.follow_splits(e);
                }
                cache.pending_edges = Some(loop_tool::edge_followers(&cache.solids, edits));
                cache.pending_select = Some(reselect);
                cache.invalidate();
                cache.refresh(&state.doc);
                if report.lost_disps > 0 {
                    let text = tf!("vertex.disp_lost", report.lost_disps);
                    self.warn(state, text);
                }
                true
            }
            Err((_, e)) => {
                let text = tf!("vertex.cancelled", vertex_error_text(&e));
                self.warn(state, text);
                false
            }
        }
    }

    fn finish_rubber(&mut self) {
        let Some(r) = self.rubber.take() else { return };
        let rect = Rect::from_two_pos(r.origin, r.current);
        let mode = self.mode;
        let cache = self.cache.get_mut();
        if !r.additive {
            cache.clear_selection();
        }
        if rect.width() < 2.0 && rect.height() < 2.0 {
            return;
        }
        if mode == VertexMode::Edges {
            let edges = edges_in_rect(&cache.solids, &r.view, rect);
            cache.set_edges_selected(&edges, true);
        } else {
            let verts = vertices_in_rect(&cache.solids, &r.view, rect);
            cache.set_selected(&verts, true);
        }
    }

    pub(crate) fn move_selection(
        &mut self,
        state: &mut AppState,
        delta: DVec3,
        label: &'static str,
        tag: Option<&str>,
    ) -> bool {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        if cache.selected_count() == 0 || delta == DVec3::ZERO || !delta.is_finite() {
            return false;
        }
        let (edits, reselect) = cache.moved_selection(delta);
        self.apply_move(state, &edits, reselect, label, tag)
    }

    pub(crate) fn snap_selection(&mut self, state: &mut AppState) -> bool {
        let grid = state.edit_grid_frame(state.grid_target_view());
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        let (edits, reselect) = cache.snapped_selection(&grid);
        if edits.is_empty() {
            return false;
        }
        self.apply_move(state, &edits, reselect, UNDO_SNAP, None)
    }

    pub(crate) fn align_selection(&mut self, state: &mut AppState, axis: usize) -> bool {
        if axis > 2 || self.is_busy() {
            return false;
        }
        let Some(view) = self.nudge_view(state).filter(ViewInfo::is_2d) else { return false };
        let grid = state.edit_grid_frame(view.kind);
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        let selected = cache.selected_positions();
        if selected.len() < 2 {
            return false;
        }
        let (min, max) =
            selected.iter().fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(a, b), (_, p)| {
                (a.min(*p), b.max(*p))
            });
        let center = (min + max) * 0.5;
        let coordinate = grid.axis_at(axis, center).snap(center[axis]);
        let (edits, _) = cache.transformed_selection(|mut p| {
            p[axis] = coordinate;
            p
        });
        let reselect = selected
            .into_iter()
            .map(|(id, mut p)| {
                p[axis] = coordinate;
                (id, p)
            })
            .collect();
        !edits.is_empty() && self.apply_move(state, &edits, reselect, "Align vertices", None)
    }

    fn align_keys(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        let Some(view) = self.nudge_view(state).filter(ViewInfo::is_2d) else {
            self.align_armed = None;
            return false;
        };
        if self.is_busy() || ctx.egui_wants_keyboard_input() || self.cache.get_mut().selected_count() < 2 {
            self.align_armed = None;
            return false;
        }
        let now = state.frame.time;
        let identity = (state.sidecar.serial(), state.doc.revision(), view.index);
        if self.align_armed.is_some_and(|(time, serial, revision, view)| {
            (now - time > 1.0 && !ctx.input(|i| i.key_down(Key::S))) || (serial, revision, view) != identity
        }) {
            self.align_armed = None;
        }
        let mut armed_at = self.align_armed.map(|s| s.0);
        let mut armed = armed_at.is_some();
        let mut axis = None;
        let mut handled = false;
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::Key { key, pressed: true, repeat, modifiers, .. } = event else { return true };
                if *modifiers != Modifiers::NONE {
                    armed = false;
                    return true;
                }
                if *key == Key::S && !repeat {
                    armed = true;
                    armed_at = Some(now);
                    handled = true;
                    return false;
                }
                if armed && !repeat {
                    axis = match key {
                        Key::X => Some(0),
                        Key::Y => Some(1),
                        Key::Z => Some(2),
                        _ => None,
                    };
                    armed = false;
                    if axis.is_some() {
                        handled = true;
                        return false;
                    }
                }
                true
            })
        });
        self.align_armed = armed.then_some((armed_at.unwrap_or(now), identity.0, identity.1, identity.2));
        if let Some(axis) = axis {
            self.align_selection(state, axis);
        }
        handled
    }

    fn can_split(&self) -> bool {
        self.cache.borrow().solids.iter().any(|s| s.split_points().len() == 2)
    }

    pub(crate) fn split_face(&mut self, state: &mut AppState) -> usize {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        let mut done: Vec<(usize, [usize; 2], model::PendingSplit)> = Vec::new();
        let mut error: Option<SplitError> = None;
        let mut wrong_count = false;
        for (ci, s) in cache.solids.iter_mut().enumerate() {
            let pts = s.split_points();
            match pts[..] {
                [a, b] => match s.apply_split(a, b) {
                    Ok(ends) => done.push((ci, ends, (s.id, a, b))),
                    Err(e) => {
                        error.get_or_insert(e);
                    }
                },
                [] => {}
                _ => wrong_count = true,
            }
        }
        if done.is_empty() {
            let text = match error {
                Some(SplitError::AlreadyAnEdge) if !wrong_count => t!("vertex.split_is_edge").to_owned(),
                _ => t!("vertex.split_need").to_owned(),
            };
            self.warn(state, text);
            return 0;
        }
        cache.clear_selection();
        for (ci, [a, b], split) in &done {
            cache.set_selected(&[(*ci, *a), (*ci, *b)], true);
            cache.splits.push(*split);
        }
        cache.generation = cache.generation.wrapping_add(1);
        self.hover = None;
        self.drag = None;
        state.notify(Severity::Info, t!("vertex.split_done"));
        done.len()
    }

    pub(crate) fn start_scale(&mut self, state: &mut AppState) -> bool {
        self.drag = None;
        self.rubber = None;
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        match ScaleState::new(&state.doc, cache) {
            Some(s) => {
                self.scale = Some(s);
                self.hover = None;
                true
            }
            None => {
                let text = t!("vertex.scale_need").to_owned();
                self.warn(state, text);
                false
            }
        }
    }

    pub(crate) fn apply_scale(&mut self, state: &mut AppState) -> bool {
        let Some(scale) = self.scale.as_ref() else { return false };
        if !scale.has_motion() {
            self.scale = None;
            return false;
        }
        if let Some(err) = scale.first_error() {
            let text = tf!("vertex.cancelled", vertex_error_text(err));
            self.warn(state, text);
            return false;
        }
        let (edits, reselect) = scale.edits();
        self.scale = None;
        self.apply_move(state, &edits, reselect, UNDO_SCALE, None)
    }

    pub(crate) fn set_scale_factor(&mut self, factor: f64) {
        if let Some(s) = self.scale.as_mut() {
            s.set_factor(factor.clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end()));
        }
    }

    fn scale_gesture(&mut self, state: &mut AppState, view: &ViewInfo, g: Gesture, mods: Modifiers) -> ToolFlow {
        let Some(scale) = self.scale.as_mut() else { return ToolFlow::Ignored };
        let grid = (state.snap_enabled() && !mods.alt).then(|| state.edit_grid_frame(view.kind));
        let move_center = |scale: &mut ScaleState, pos: Pos2| {
            let Some(d) = scale.dragging.filter(|d| d.view == view.index) else { return };
            let Some(p) = drag_point(d.kind, view, d.center, pos) else { return };
            let delta = model::snap_delta_on(d.center, p - d.start, grid.as_ref(), d.kind.mask(), d.kind.plane());
            scale.set_center(d.center + delta);
        };
        match g {
            Gesture::DragStart { origin, pos } => {
                let near = view.world_to_screen(scale.center).is_some_and(|c| c.distance(origin) <= SCALE_CENTER_PX);
                let kind = if view.is_2d() {
                    DragKind::View2d(view.kind)
                } else {
                    DragKind::Plane { normal: view.cam3d.forward() }
                };
                if near && let Some(start) = drag_point(kind, view, scale.center, origin) {
                    scale.dragging = Some(model::CenterDrag { view: view.index, kind, center: scale.center, start });
                    move_center(scale, pos);
                }
                ToolFlow::Handled
            }
            Gesture::Drag(p) | Gesture::DragEnd(p) => {
                move_center(scale, p);
                if matches!(g, Gesture::DragEnd(_)) && scale.dragging.is_some_and(|d| d.view == view.index) {
                    scale.dragging = None;
                }
                ToolFlow::Handled
            }
            Gesture::Click(_) => ToolFlow::Handled,
            Gesture::Hover(_) | Gesture::Away => {
                if scale.dragging.is_some_and(|d| d.view == view.index) {
                    scale.dragging = None;
                }
                ToolFlow::Ignored
            }
        }
    }

    fn nudge_view(&self, state: &AppState) -> Option<ViewInfo> {
        let v = self.last_view?;
        let ui = &state.ui;
        let matches = ui.hovered_view == Some(v.index)
            || (ui.hovered_view.is_none() && (ui.focused_view == Some(v.index) || ui.focused_view.is_none()));
        matches.then_some(v)
    }

    fn has_pending(&self) -> bool {
        let cache = self.cache.borrow();
        self.drag.is_some()
            || self.rubber.is_some()
            || self.scale.is_some()
            || self.xform.is_some()
            || cache.selected_count() > 0
            || !cache.splits.is_empty()
    }

    fn gizmo_hover(&self, view: &ViewInfo) -> Option<usize> {
        match self.hover {
            Some(Hover { view: v, target: HoverTarget::Gizmo(a), .. }) if v == view.index => Some(a),
            _ => None,
        }
    }

    fn current_hover(&self, cache: &VertexCache) -> Option<Hover> {
        self.hover.filter(|h| h.generation == cache.generation)
    }

    fn current_drag(&self, cache: &VertexCache) -> Option<&VertexDrag> {
        self.drag.as_ref().filter(|d| d.generation == cache.generation)
    }

    fn current_scale(&self, cache: &VertexCache) -> Option<&ScaleState> {
        self.scale.as_ref().filter(|s| s.generation == cache.generation)
    }

    fn nudge_keys(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if !state.settings.edit.arrow_key_nudge
            || self.drag.is_some()
            || self.scale.is_some()
            || self.cache.get_mut().selected_count() == 0
        {
            return false;
        }
        let Some(view) = self.nudge_view(state) else { return false };
        let keys: Vec<(NudgeKey, bool)> = ctx.input_mut(|i| {
            let mut out = Vec::new();
            i.events.retain(|e| {
                if let egui::Event::Key { key, pressed: true, modifiers, .. } = e
                    && !modifiers.command
                    && !modifiers.ctrl
                    && !modifiers.mac_cmd
                    && !(modifiers.alt && modifiers.shift)
                    && let Some(k) = NudgeKey::from_key(*key)
                {
                    out.push((k, modifiers.alt || modifiers.shift));
                    return false;
                }
                true
            });
            out
        });
        if keys.is_empty() {
            return false;
        }
        let frame = state.edit_grid_frame(view.kind);
        let steps = DVec3::new(frame.step_along(0), frame.step_along(1), frame.step_along(2)).max(DVec3::splat(1e-3));
        let delta: DVec3 = keys
            .iter()
            .map(|(k, fine)| if *fine { nudge_delta(&view, *k, 1.0) } else { nudge_delta(&view, *k, 1.0) * steps })
            .sum();
        if delta.length_squared() > 1e-12 {
            self.move_selection(state, delta, UNDO_NUDGE, Some(NUDGE_TAG));
        }
        true
    }

    fn draw_previews(out: &mut hp_render::Overlay, solids: &[model::DragSolid], previews: &[Preview]) {
        let accent = rgba(SELECTED);
        let normal = rgba(HANDLE);
        let error = rgba(VIEW_ERROR);
        let preview = rgba(PREVIEW);
        for (ds, pv) in solids.iter().zip(previews) {
            match &pv.result {
                Ok(edges) => {
                    for [a, b] in edges {
                        line_on_top(out, *a, *b, preview);
                    }
                }
                Err(_) => {
                    if let Some(t) = &ds.topology {
                        for [a, b] in model::unique_edges(&t.faces) {
                            if let (Some(pa), Some(pb)) = (pv.points.get(a), pv.points.get(b)) {
                                line_on_top(out, *pa, *pb, error);
                            }
                        }
                    }
                }
            }
            let valid = pv.result.is_ok();
            for (i, p) in pv.points.iter().enumerate() {
                if ds.moving.contains(&i) {
                    point(out, *p, if valid { accent } else { error }, 8.0);
                } else {
                    point(out, *p, normal, 7.0);
                }
            }
        }
    }
}

const SELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::A);

fn action_shortcut(state: &AppState, action: Action) -> Option<String> {
    state.keys.shortcut(action).map(|s| shortcut_label(&s))
}

fn select_all_label(state: &AppState) -> String {
    shortcut_label(&state.keys.shortcut(Action::SelectAll).unwrap_or(SELECT_ALL))
}

fn with_shortcut(state: &AppState, text: &str, action: Action) -> String {
    match action_shortcut(state, action) {
        Some(s) => format!("{text} ({s})"),
        None => text.to_owned(),
    }
}

fn gizmo_arms(view: &ViewInfo, center: DVec3) -> Option<[(usize, DVec3, DVec3); 3]> {
    if view.is_2d() {
        return None;
    }
    view.world_to_screen(center)?;
    let len = world_per_point(view, center) * GIZMO_PX;
    Some([0, 1, 2].map(|a| (a, center, center + axis_dir(a) * len)))
}

fn pick_gizmo(view: &ViewInfo, center: DVec3, pos: Pos2) -> Option<usize> {
    let arms = gizmo_arms(view, center)?;
    let c = view.world_to_screen(center)?;
    if c.distance(pos) < GIZMO_DEAD_PX {
        return None;
    }
    let mut best: Option<(f32, usize)> = None;
    for (axis, _, tip) in arms {
        let Some(t) = view.world_to_screen(tip) else { continue };
        let d = segment_distance(pos, c, t);
        if d <= GIZMO_HIT_PX && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, axis));
        }
    }
    best.map(|(_, a)| a)
}

fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

fn axis_color(axis: usize) -> Color32 {
    match axis {
        0 => AXIS_X,
        1 => AXIS_Y,
        _ => AXIS_Z,
    }
}

const OCCUPIED_CELL_PX: f32 = 3.0;
const OCCUPIED_MAX_VERTICES: usize = 8_000;

fn occupied_cells(cache: &VertexCache, view: &ViewInfo, clip: Rect) -> Option<std::collections::HashSet<(i32, i32)>> {
    if cache.vertex_count() > OCCUPIED_MAX_VERTICES {
        return None;
    }
    let mut cells = std::collections::HashSet::with_capacity(cache.vertex_count());
    for s in &cache.solids {
        for p in &s.points {
            if let Some(sp) = view.world_to_screen(*p).filter(|sp| clip.contains(*sp)) {
                cells.insert(((sp.x / OCCUPIED_CELL_PX).round() as i32, (sp.y / OCCUPIED_CELL_PX).round() as i32));
            }
        }
    }
    Some(cells)
}

fn cell_taken(cells: &std::collections::HashSet<(i32, i32)>, p: Pos2) -> bool {
    let (cx, cy) = ((p.x / OCCUPIED_CELL_PX).round() as i32, (p.y / OCCUPIED_CELL_PX).round() as i32);
    (-1..=1).any(|dx| (-1..=1).any(|dy| cells.contains(&(cx + dx, cy + dy))))
}

const MENU_SPLIT: u32 = 1;
const MENU_SCALE: u32 = 2;
const MENU_SNAP: u32 = 3;
const MENU_WELD: u32 = 4;
const MENU_WELD_DISTANCE: u32 = 5;
const MENU_EDGE_ACTIONS: [(u32, Action); 7] = [
    (6, Action::EdgeSelectLoop),
    (7, Action::EdgeSelectRing),
    (8, Action::EdgeGrow),
    (9, Action::EdgeShrink),
    (10, Action::EdgeSelectBoundary),
    (11, Action::EdgeSlide),
    (12, Action::VertexRotate),
];

impl Tool for VertexTool {
    fn id(&self) -> ToolId {
        ToolId::Vertex
    }

    fn activate(&mut self, state: &mut AppState) {
        *self.cache.get_mut() = VertexCache::default();
        let revision = state.doc.revision();
        let picked = &state.runtime.selection;
        let edges = picked.fresh_edges(revision);
        let mut corners: Vec<(hp_editor::Id, DVec3)> = picked.fresh_vertices(revision).to_vec();
        corners.extend(edges.iter().flat_map(|e| [(e.solid, e.a), (e.solid, e.b)]));
        let mids: Vec<(hp_editor::Id, DVec3)> = edges.iter().map(|e| (e.solid, (e.a + e.b) * 0.5)).collect();
        let cache = self.cache.get_mut();
        if !corners.is_empty() {
            cache.pending_select = Some(corners);
            cache.pending_edges = Some(mids);
        }
        cache.refresh(&state.doc);
        self.drag = None;
        self.rubber = None;
        self.scale = None;
        self.xform = None;
        self.loop_hover = None;
        self.hover = None;
        self.align_armed = None;
        self.merge_prompt = None;
    }

    fn update(&mut self, state: &mut AppState, ctx: &egui::Context) {
        self.merge_prompt_ui(state, ctx);
    }

    fn reactivate(&mut self, state: &mut AppState) {
        self.mode = self.mode.next();
        self.hover = None;
        state.notify(Severity::Info, tf!("vertex.mode_changed", self.mode.label()));
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.merge_prompt = None;
        self.end_live(state, false);
        self.drag = None;
        self.rubber = None;
        self.scale = None;
        self.xform = None;
        self.loop_hover = None;
        self.hover = None;
        self.cache.get_mut().splits.clear();
        self.cache.get_mut().invalidate();
    }

    fn action(&mut self, state: &mut AppState, action: Action) -> bool {
        if self.merge_prompt.is_some() {
            return true;
        }
        self.cache.get_mut().refresh(&state.doc);
        self.drop_stale(state);
        let ours = matches!(
            action,
            Action::SnapToGrid
                | Action::VertexSplitFace
                | Action::VertexScale
                | Action::VertexWeld
                | Action::VertexWeldDistance
                | Action::VertexCleanup
                | Action::VertexRotate
                | Action::EdgeSelectLoop
                | Action::EdgeSelectRing
                | Action::EdgeGrow
                | Action::EdgeShrink
                | Action::EdgeSelectBoundary
                | Action::EdgeSlide
        );
        if ours && (self.drag.is_some() || self.rubber.is_some() || self.xform.is_some()) {
            return true;
        }
        match action {
            Action::SelectAll if !self.cache.get_mut().solids.is_empty() => {
                self.cache.get_mut().select_all();
                true
            }
            Action::SnapToGrid if self.cache.get_mut().selected_count() > 0 => {
                if !self.snap_selection(state) {
                    state.notify(Severity::Info, t!("vertex.snap_nothing"));
                }
                true
            }
            Action::VertexSplitFace => {
                self.split_face(state);
                true
            }
            Action::VertexWeld => {
                self.weld_selected(state);
                true
            }
            Action::VertexWeldDistance => {
                self.weld_by_distance(state);
                true
            }
            Action::VertexCleanup => {
                self.cleanup_edited(state);
                true
            }
            Action::VertexScale => {
                if self.scale.is_some() {
                    self.apply_scale(state);
                } else {
                    self.start_scale(state);
                }
                true
            }
            Action::VertexRotate
            | Action::EdgeSelectLoop
            | Action::EdgeSelectRing
            | Action::EdgeGrow
            | Action::EdgeShrink
            | Action::EdgeSelectBoundary
            | Action::EdgeSlide => self.edge_action(state, action),
            _ => false,
        }
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if self.merge_prompt.is_some() {
            return ToolFlow::Handled;
        }
        if input.navigating {
            self.hover = None;
            return ToolFlow::Ignored;
        }
        let g = gesture(input);
        self.handle(state, input.view, g, input.modifiers)
    }

    fn wheel(&mut self, _state: &mut AppState, view: &ViewInfo, notches: f32, mods: Modifiers) -> ToolFlow {
        if self.xform_wheel(notches, mods) {
            self.last_view = Some(*view);
            return ToolFlow::Handled;
        }
        let Some(scale) = self.scale.as_ref() else { return ToolFlow::Ignored };
        if !mods.alt || !notches.is_finite() || notches == 0.0 {
            return ToolFlow::Ignored;
        }
        let step = if mods.ctrl || mods.command { SCALE_STEP * 0.1 } else { SCALE_STEP };
        let factor = ((scale.factor + f64::from(notches) * step) * 1000.0).round() / 1000.0;
        self.set_scale_factor(factor);
        self.last_view = Some(*view);
        ToolFlow::Handled
    }

    fn context_entries(&self, state: &AppState) -> Vec<ToolMenuEntry> {
        let cache = self.cache.borrow();
        let selected = cache.selected_count() > 0;
        let label = |a: Action| match action_shortcut(state, a) {
            Some(s) => format!("{}  ({s})", a.label()),
            None => a.label().to_owned(),
        };
        let mut entries = vec![
            ToolMenuEntry { label: label(Action::VertexSplitFace), id: MENU_SPLIT, enabled: self.can_split() },
            ToolMenuEntry { label: label(Action::VertexScale), id: MENU_SCALE, enabled: selected },
            ToolMenuEntry { label: t!("vertex.snap_selected").to_owned(), id: MENU_SNAP, enabled: selected },
            ToolMenuEntry { label: Action::VertexWeld.label().to_owned(), id: MENU_WELD, enabled: selected },
            ToolMenuEntry {
                label: Action::VertexWeldDistance.label().to_owned(),
                id: MENU_WELD_DISTANCE,
                enabled: !cache.solids.is_empty(),
            },
        ];
        entries.extend(MENU_EDGE_ACTIONS.iter().map(|(id, a)| ToolMenuEntry {
            label: a.label().to_owned(),
            id: *id,
            enabled: selected,
        }));
        entries
    }

    fn context_command(&mut self, state: &mut AppState, id: u32) {
        match id {
            MENU_SPLIT => {
                self.split_face(state);
            }
            MENU_SCALE => {
                self.start_scale(state);
            }
            MENU_SNAP => {
                self.snap_selection(state);
            }
            MENU_WELD => {
                self.weld_selected(state);
            }
            MENU_WELD_DISTANCE => {
                self.weld_by_distance(state);
            }
            other => {
                if let Some((_, a)) = MENU_EDGE_ACTIONS.iter().find(|(i, _)| *i == other) {
                    self.edge_action(state, *a);
                }
            }
        }
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.merge_prompt.is_some() {
            return false;
        }
        self.cache.get_mut().refresh(&state.doc);
        self.drop_stale(state);
        let mut handled = self.align_keys(state, ctx);
        if self.xform.is_some() {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                self.apply_xform(state);
                handled = true;
            }
        } else if self.scale.is_some() {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                self.apply_scale(state);
                handled = true;
            }
        } else if !self.cache.get_mut().solids.is_empty()
            && ctx.input_mut(|i| !i.modifiers.shift && i.consume_shortcut(&SELECT_ALL))
        {
            self.cache.get_mut().select_all();
            handled = true;
        }
        if self.has_pending() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.cancel(state);
            handled = true;
        }
        handled |= self.nudge_keys(state, ctx);
        handled
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut hp_render::Overlay) {
        if let Some(p) = &self.merge_prompt {
            for [a, b] in &p.edges {
                line_on_top(out, *a, *b, rgba(PREVIEW));
            }
            for spot in &p.spots {
                point(out, *spot, rgba(SELECTED), 11.0);
            }
            return;
        }
        self.cache.borrow_mut().refresh(&state.doc);
        let cache = self.cache.borrow();
        let accent = rgba(SELECTED);
        let normal = rgba(HANDLE);
        let drag = self.current_drag(&cache);
        let scale = self.current_scale(&cache);
        let xform = self.xform.as_ref().filter(|x| x.generation == cache.generation);
        let show_vertices = self.mode != VertexMode::Edges;
        let hidden = |ci: usize| {
            drag.is_some_and(|d| d.dragged_mask.get(ci).copied().unwrap_or(false))
                || scale.is_some_and(|s| s.mask.get(ci).copied().unwrap_or(false))
                || xform.is_some_and(|x| x.mask.get(ci).copied().unwrap_or(false))
        };
        for (ci, s) in cache.solids.iter().enumerate() {
            if hidden(ci) {
                continue;
            }
            for (p, sel) in s.points.iter().zip(&s.selected) {
                if *sel {
                    point(out, *p, accent, if show_vertices { 8.0 } else { 5.0 });
                } else if show_vertices {
                    point(out, *p, normal, 7.0);
                }
            }
            for (e, sel) in s.edges.iter().zip(&s.edge_selected) {
                if *sel && let (Some(a), Some(b)) = (s.points.get(e[0]), s.points.get(e[1])) {
                    line_on_top(out, *a, *b, accent);
                }
            }
        }
        let split_color = rgba(SPLIT);
        for (id, a, b) in &cache.splits {
            if cache.solids.iter().enumerate().any(|(ci, s)| s.id == *id && !hidden(ci)) {
                line_on_top(out, *a, *b, split_color);
            }
        }
        self.draw_loop_hover(out);
        if let Some(d) = drag {
            Self::draw_previews(out, &d.solids, &d.previews);
            if let DragKind::Axis(a) = d.kind {
                let len = world_per_point(view, d.anchor) * 4096.0;
                let dir = axis_dir(a);
                let mut c = rgba(axis_color(a));
                c[3] = 0.6;
                line_on_top(out, d.anchor - dir * len, d.anchor + dir * len, c);
            }
        } else if let Some(s) = scale {
            Self::draw_previews(out, &s.solids, &s.previews);
            point(out, s.center, rgba(PREVIEW), 6.0);
        } else if xform.is_some() {
            self.xform_overlay(out);
        } else if let Some(center) = cache.selection_center()
            && let Some(arms) = gizmo_arms(view, center)
        {
            let hovered = self.gizmo_hover(view);
            let (right, up) = (view.cam3d.right(), view.cam3d.up());
            for (axis, c, tip) in arms {
                let color = if hovered == Some(axis) { rgba(Color32::WHITE) } else { rgba(axis_color(axis)) };
                line_on_top(out, c, tip, color);
                let dir = axis_dir(axis);
                let side = if dir.dot(right).abs() > 0.9 { up } else { right };
                let head = (tip - c) * 0.18;
                line_on_top(out, tip, tip - head + side * head.length() * 0.45, color);
                line_on_top(out, tip, tip - head - side * head.length() * 0.45, color);
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        self.cache.borrow_mut().refresh(&state.doc);
        let cache = self.cache.borrow();
        let clip = view.rect.expand(8.0);
        let drag = self.current_drag(&cache);
        let scale = self.current_scale(&cache);
        if self.mode != VertexMode::Vertices && drag.is_none() && scale.is_none() && self.xform.is_none() {
            let fill = Color32::from_rgb(200, 200, 210);
            let stroke = Stroke::new(1.0, Color32::from_rgb(20, 20, 24));
            let occupied =
                (self.mode == VertexMode::Both && view.is_2d()).then(|| occupied_cells(&cache, view, clip)).flatten();
            let mut drawn = 0usize;
            'solids: for s in &cache.solids {
                for (e, [a, b]) in s.edges.iter().enumerate() {
                    let (Some(&pa), Some(&pb)) = (s.points.get(*a), s.points.get(*b)) else { continue };
                    if !edge_visible(view, pa, pb) {
                        continue;
                    }
                    let Some(m) = view.world_to_screen((pa + pb) * 0.5) else { continue };
                    if occupied.as_ref().is_some_and(|cells| cell_taken(cells, m)) {
                        continue;
                    }
                    if clip.contains(m) {
                        let selected = s.edge_selected.get(e).copied().unwrap_or(false);
                        painter.circle(m, 3.0, if selected { SELECTED } else { fill }, stroke);
                        drawn += 1;
                        if drawn >= MAX_EDGE_HANDLES {
                            break 'solids;
                        }
                    }
                }
            }
        }
        if let Some(h) = self.current_hover(&cache) {
            match h.target {
                HoverTarget::Handle(HandleRef::Vertex { solid, index }) => {
                    if let Some(p) = cache.point(solid, index).and_then(|p| view.world_to_screen(p)) {
                        painter.rect_stroke(
                            Rect::from_center_size(p, egui::vec2(13.0, 13.0)),
                            2.0,
                            Stroke::new(1.5, VIEW_ACCENT),
                            StrokeKind::Middle,
                        );
                    }
                }
                HoverTarget::Handle(HandleRef::Edge { solid, edge }) => {
                    if let Some(p) =
                        cache.solids.get(solid).and_then(|s| s.midpoint(edge)).and_then(|p| view.world_to_screen(p))
                    {
                        painter.circle_stroke(p, 6.5, Stroke::new(1.5, VIEW_ACCENT));
                        if self.mode == VertexMode::Both {
                            painter.line_segment(
                                [p - egui::vec2(3.0, 0.0), p + egui::vec2(3.0, 0.0)],
                                Stroke::new(1.0, VIEW_ACCENT),
                            );
                            painter.line_segment(
                                [p - egui::vec2(0.0, 3.0), p + egui::vec2(0.0, 3.0)],
                                Stroke::new(1.0, VIEW_ACCENT),
                            );
                        }
                    }
                }
                HoverTarget::Gizmo(_) => {}
            }
        }
        if let Some(r) = self.rubber
            && r.view.index == view.index
        {
            let rect = Rect::from_two_pos(r.origin, r.current);
            painter.rect_filled(rect, 0.0, VIEW_ACCENT.gamma_multiply(0.12));
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, VIEW_ACCENT), StrokeKind::Middle);
        }
        let font = FontId::proportional(12.0);
        if let Some(d) = drag {
            if let Some(p) = view.world_to_screen(d.anchor + d.delta) {
                let (text, color) = match d.first_error() {
                    Some(e) => (vertex_error_text(e), VIEW_ERROR),
                    None => (fmt_vec3(d.delta), PREVIEW),
                };
                painter.text(p + egui::vec2(12.0, -10.0), Align2::LEFT_BOTTOM, text, font, color);
            }
        } else if let Some(s) = scale {
            if let Some(c) = view.world_to_screen(s.center) {
                let color = if s.first_error().is_some() { VIEW_ERROR } else { PREVIEW };
                painter.circle_stroke(c, SCALE_CENTER_PX - 2.0, Stroke::new(1.5, color));
                painter.line_segment([c - egui::vec2(5.0, 0.0), c + egui::vec2(5.0, 0.0)], Stroke::new(1.0, color));
                painter.line_segment([c - egui::vec2(0.0, 5.0), c + egui::vec2(0.0, 5.0)], Stroke::new(1.0, color));
                let text = match s.first_error() {
                    Some(e) => vertex_error_text(e),
                    None => format!("x{:.2}", s.factor),
                };
                painter.text(c + egui::vec2(12.0, -10.0), Align2::LEFT_BOTTOM, text, font, color);
            }
        } else if self.xform.is_some() {
            self.xform_paint(view, painter, cache.selection_center(), font.clone());
        } else if let Some(center) = cache.selection_center()
            && let Some(arms) = gizmo_arms(view, center)
        {
            for (axis, _, tip) in arms {
                if let Some(p) = view.world_to_screen(tip) {
                    let label = ["X", "Y", "Z"].get(axis).copied().unwrap_or("Z");
                    painter.text(p + egui::vec2(4.0, -4.0), Align2::LEFT_BOTTOM, label, font.clone(), axis_color(axis));
                }
            }
        }
    }

    fn options_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.cache.get_mut().refresh(&state.doc);
        self.drop_stale(state);
        if self.scale.is_some() {
            self.scale_options(ui, state);
            return;
        }
        if self.xform.is_some() {
            self.xform_options(ui, state);
            return;
        }
        let mode_tip = with_shortcut(state, t!("vertex.mode_tip"), Action::ToolVertex);
        let mut pending: Option<DVec3> = None;
        let mut snap_now = false;
        let mut select_all = false;
        let mut deselect = false;
        let mut split = false;
        let mut scale = false;
        let can_split = self.can_split();
        let mut weld_now = false;
        let mut weld_distance_now = false;
        let mut cleanup_now = false;
        ui.horizontal_wrapped(|ui| {
            for mode in [VertexMode::Both, VertexMode::Vertices, VertexMode::Edges] {
                if ui.add(egui::Button::selectable(self.mode == mode, mode.label())).on_hover_text(&mode_tip).clicked()
                {
                    self.mode = mode;
                    self.hover = None;
                }
            }
            ui.separator();
            let mut uv_lock = state.settings.mesh.uv_lock;
            if ui.checkbox(&mut uv_lock, t!("vertex.uv_lock")).on_hover_text(t!("vertex.uv_lock_tip")).changed() {
                state.settings.mesh.uv_lock = uv_lock;
                state.settings_dirty = true;
            }
            ui.checkbox(&mut self.selected_only, t!("vertex.selected_only"))
                .on_hover_text(t!("vertex.selected_only_tip"));
            ui.label(t!("vertex.merge")).on_hover_text(t!("vertex.merge_tip"));
            ui.add(egui::DragValue::new(&mut self.merge_tolerance).range(0.0..=16.0).speed(0.05).max_decimals(2))
                .on_hover_text(t!("vertex.merge_tip"));
            ui.separator();
            let cache = self.cache.get_mut();
            let selected = cache.selected_count();
            ui.label(tf!("vertex.stats", selected, cache.vertex_count())).on_hover_text(format!(
                "{}\n{}",
                tf!("vertex.stats_tip", cache.solids.len()),
                t!("vertex.align_tip")
            ));
            if cache.truncated {
                warning_badge(ui, &tf!("vertex.truncated", model::MAX_EDIT_SOLIDS));
            }
            if cache.solids.iter().any(|s| s.disp_faces > 0) {
                warning_badge(ui, t!("vertex.has_disp"));
            }
            if !cache.splits.is_empty() {
                ui.colored_label(SPLIT, tf!("vertex.split_pending", cache.splits.len()))
                    .on_hover_text(tf!("vertex.split_pending_tip", key_label(Key::Escape)));
            }
            let all_tip = format!("{} ({})", t!("vertex.select_all_tip"), select_all_label(state));
            if ui
                .add_enabled(!cache.solids.is_empty(), egui::Button::new(t!("vertex.select_all")))
                .on_hover_text(all_tip)
                .clicked()
            {
                select_all = true;
            }
            let split_tip = with_shortcut(state, t!("vertex.split_tip"), Action::VertexSplitFace);
            if ui
                .add_enabled(can_split, egui::Button::new(Action::VertexSplitFace.label()))
                .on_hover_text(&split_tip)
                .on_disabled_hover_text(&split_tip)
                .clicked()
            {
                split = true;
            }
            if selected > 0 {
                if ui
                    .button(t!("vertex.deselect"))
                    .on_hover_text(tf!("vertex.deselect_tip", key_label(Key::Escape)))
                    .clicked()
                {
                    deselect = true;
                }
                ui.separator();
                if let Some(center) = cache.selection_center() {
                    ui.label(t!("vertex.position")).on_hover_text(t!("vertex.position_tip"));
                    let mut edited = center;
                    if hp_app::widgets::vec3_edit(ui, &mut edited, 1.0) {
                        pending = Some(edited - center);
                    }
                }
                let snap_sel_tip = with_shortcut(state, t!("vertex.snap_selected_tip"), Action::SnapToGrid);
                if ui.button(t!("vertex.snap_selected")).on_hover_text(snap_sel_tip).clicked() {
                    snap_now = true;
                }
                let scale_tip = with_shortcut(state, t!("vertex.scale_tip"), Action::VertexScale);
                if ui.button(Action::VertexScale.label()).on_hover_text(scale_tip).clicked() {
                    scale = true;
                }
            }
        });
        self.weld_options(ui, state, &mut [&mut weld_now, &mut weld_distance_now, &mut cleanup_now]);
        let mut loop_clicks: Vec<Action> = Vec::new();
        self.loop_options(ui, state, &mut loop_clicks);
        for a in loop_clicks {
            self.edge_action(state, a);
        }
        if weld_now {
            self.weld_selected(state);
        }
        if weld_distance_now {
            self.weld_by_distance(state);
        }
        if cleanup_now {
            self.cleanup_edited(state);
        }
        if select_all {
            self.cache.get_mut().select_all();
        }
        if deselect {
            self.cache.get_mut().clear_selection();
        }
        if let Some(delta) = pending {
            self.move_selection(state, delta, UNDO_POSITION, Some(POSITION_TAG));
        }
        if snap_now {
            self.snap_selection(state);
        }
        if split {
            self.split_face(state);
        }
        if scale {
            self.start_scale(state);
        }
    }

    fn set_option(&mut self, _state: &mut AppState, key: &str, value: &str) -> bool {
        let number =
            |max: f64, min: f64| value.trim().parse::<f64>().ok().filter(|v| v.is_finite() && (min..=max).contains(v));
        match key {
            "merge_distance" => number(16.0, 0.0).map(|v| self.merge_tolerance = v).is_some(),
            "weld_distance" => number(weld::MAX_WELD_DISTANCE, 0.001).map(|v| self.weld_distance = v).is_some(),
            "weld_target" => weld::WeldTarget::parse(value).map(|v| self.weld_target = v).is_some(),
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        vec![
            hp_app::tool::ToolOption::new(
                "merge_distance",
                format!("{}", self.merge_tolerance),
                "units from 0 to 16: a vertex dropped closer than this to another vertex merges with it",
            ),
            hp_app::tool::ToolOption::new(
                "weld_distance",
                format!("{}", self.weld_distance),
                "units from 0.001 to 64: Weld by distance merges the vertices closer than this",
            ),
            hp_app::tool::ToolOption::new(
                "weld_target",
                self.weld_target.name(),
                "where Weld selected vertices puts them: center, first or last (lowest or highest vertex index)",
            ),
        ]
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if let Some(s) = self.scale.as_ref() {
            if s.dragging.is_some() {
                return Some(CursorIcon::Grabbing);
            }
            let near = view.world_to_screen(s.center).is_some_and(|c| c.distance(pos) <= SCALE_CENTER_PX);
            return near.then_some(CursorIcon::Grab);
        }
        if self.drag.is_some() {
            return Some(CursorIcon::Grabbing);
        }
        if self.rubber.is_some() {
            return Some(CursorIcon::Crosshair);
        }
        match self.hover {
            Some(h) if h.view == view.index => Some(match h.target {
                HoverTarget::Handle(HandleRef::Vertex { .. }) => CursorIcon::Grab,
                HoverTarget::Handle(HandleRef::Edge { .. }) if self.mode == VertexMode::Edges => CursorIcon::Grab,
                HoverTarget::Handle(HandleRef::Edge { .. }) => CursorIcon::Crosshair,
                HoverTarget::Gizmo(_) => CursorIcon::Move,
            }),
            _ => None,
        }
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        if self.align_armed.is_some() {
            return Some(t!("vertex.align_tip").to_owned());
        }
        if let Some(s) = self.scale.as_ref() {
            return Some(match s.first_error() {
                Some(e) => tf!("vertex.hint_scale_invalid", vertex_error_text(e)),
                None => {
                    tf!("vertex.hint_scale", format!("{:.2}", s.factor), key_label(Key::Enter), key_label(Key::Escape))
                }
            });
        }
        if let Some(d) = self.drag.as_ref() {
            return Some(match d.first_error() {
                Some(e) => tf!("vertex.hint_invalid", vertex_error_text(e)),
                None if d.split => tf!("vertex.hint_split", fmt_vec3(d.delta)),
                None => tf!("vertex.hint_drag", d.moving_count(), fmt_vec3(d.delta)),
            });
        }
        if let Some(h) = self.xform_hint() {
            return Some(h);
        }
        if self.rubber.is_some() {
            return Some(t!("vertex.hint_rubber").to_owned());
        }
        self.cache.borrow_mut().refresh(&state.doc);
        if let Some(text) = self.loop_status() {
            return Some(text);
        }
        let cache = self.cache.borrow();
        if cache.solids.is_empty() {
            return Some(t!("vertex.hint_pick").to_owned());
        }
        let key = |a: Action| action_shortcut(state, a).unwrap_or_else(|| a.label().to_owned());
        Some(tf!(
            "vertex.hint_idle",
            select_all_label(state),
            key(Action::VertexSplitFace),
            key(Action::VertexScale),
            key(Action::ToolVertex)
        ))
    }

    fn face_selection(&self, state: &AppState) -> Vec<hp_editor::FaceRef> {
        let Ok(mut cache) = self.cache.try_borrow_mut() else { return Vec::new() };
        cache.refresh(&state.doc);
        let mut out = Vec::new();
        for s in
            cache.solids.iter().filter(|s| !s.split && s.selected_count() > 0 && s.selected_count() < s.points.len())
        {
            for (face, side) in s.faces.iter().zip(&s.face_sides) {
                if face.len() >= 3 && face.iter().all(|i| s.selected.get(*i).copied().unwrap_or(false)) {
                    out.push(hp_editor::FaceRef::new(s.id, *side));
                }
            }
        }
        out
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        if self.merge_prompt.is_some() {
            return self.answer_merge_prompt(state, false, false);
        }
        if self.drag.take().is_some() {
            self.end_live(state, false);
            return true;
        }
        if self.rubber.take().is_some() {
            return true;
        }
        if self.scale.take().is_some() || self.xform.take().is_some() {
            return true;
        }
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        if cache.selected_count() > 0 {
            cache.clear_selection();
            return true;
        }
        if !cache.splits.is_empty() {
            cache.splits.clear();
            cache.invalidate();
            return true;
        }
        false
    }
}

impl VertexTool {
    fn weld_options(&mut self, ui: &mut egui::Ui, state: &AppState, clicks: &mut [&mut bool; 3]) {
        let selected = self.cache.get_mut().selected_count();
        let by_selection = self.selected_weld_count();
        let by_distance = self.distance_weld_count();
        let empty = self.cache.get_mut().solids.is_empty();
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("vertex.weld")).on_hover_text(t!("vertex.weld_tip"));
            egui::ComboBox::from_id_salt("vertex_weld_target")
                .selected_text(self.weld_target.label())
                .show_ui(ui, |ui| {
                    for target in [weld::WeldTarget::Center, weld::WeldTarget::First, weld::WeldTarget::Last] {
                        ui.selectable_value(&mut self.weld_target, target, target.label());
                    }
                })
                .response
                .on_hover_text(t!("vertex.weld_target_tip"));
            let tip = with_shortcut(state, t!("vertex.weld_selected_tip"), Action::VertexWeld);
            if ui
                .add_enabled(selected > 1, egui::Button::new(Action::VertexWeld.label()))
                .on_hover_text(&tip)
                .on_disabled_hover_text(&tip)
                .clicked()
            {
                *clicks[0] = true;
            }
            if by_selection > 0 {
                ui.label(tf!("vertex.weld_preview", by_selection));
            }
            ui.separator();
            ui.label(t!("vertex.weld_distance")).on_hover_text(t!("vertex.weld_distance_tip"));
            ui.add(
                egui::DragValue::new(&mut self.weld_distance)
                    .range(0.001..=weld::MAX_WELD_DISTANCE)
                    .speed(0.05)
                    .max_decimals(3),
            )
            .on_hover_text(t!("vertex.weld_distance_tip"));
            let tip = with_shortcut(state, t!("vertex.weld_by_distance_tip"), Action::VertexWeldDistance);
            if ui
                .add_enabled(!empty, egui::Button::new(Action::VertexWeldDistance.label()))
                .on_hover_text(&tip)
                .on_disabled_hover_text(&tip)
                .clicked()
            {
                *clicks[1] = true;
            }
            if by_distance > 0 {
                ui.label(tf!("vertex.weld_preview", by_distance));
            }
            ui.separator();
            let tip = with_shortcut(state, t!("vertex.cleanup_tip"), Action::VertexCleanup);
            if ui
                .add_enabled(!empty, egui::Button::new(Action::VertexCleanup.label()))
                .on_hover_text(&tip)
                .on_disabled_hover_text(&tip)
                .clicked()
            {
                *clicks[2] = true;
            }
        });
    }

    fn scale_options(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let Some(scale) = self.scale.as_ref() else { return };
        let mut factor = scale.factor;
        let mut center = scale.center;
        let error = scale.first_error().map(vertex_error_text);
        let mut apply = false;
        let mut cancel = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong(Action::VertexScale.label().trim_end_matches('.'));
            ui.separator();
            ui.label(t!("vertex.scale_factor")).on_hover_text(t!("vertex.scale_factor_tip"));
            if ui.small_button("-").on_hover_text(format!("-{SCALE_STEP}")).clicked() {
                factor -= SCALE_STEP;
            }
            ui.add(egui::DragValue::new(&mut factor).range(SCALE_RANGE).speed(0.01).max_decimals(3))
                .on_hover_text(t!("vertex.scale_factor_tip"));
            if ui.small_button("+").on_hover_text(format!("+{SCALE_STEP}")).clicked() {
                factor += SCALE_STEP;
            }
            ui.separator();
            ui.label(t!("vertex.scale_center")).on_hover_text(t!("vertex.scale_center_tip"));
            hp_app::widgets::vec3_edit(ui, &mut center, 1.0);
            ui.separator();
            let apply_tip = format!("{} ({})", t!("vertex.scale_apply_tip"), key_label(Key::Enter));
            if ui.add_enabled(error.is_none(), egui::Button::new(t!("common.apply"))).on_hover_text(apply_tip).clicked()
            {
                apply = true;
            }
            if ui.button(t!("common.cancel")).on_hover_text(key_label(Key::Escape)).clicked() {
                cancel = true;
            }
            if let Some(e) = &error {
                ui.colored_label(hp_app::theme::error(), e);
            }
        });
        if let Some(s) = self.scale.as_mut() {
            s.set_factor(factor.clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end()));
            s.set_center(center);
        }
        if apply {
            self.apply_scale(state);
        }
        if cancel {
            self.scale = None;
        }
    }
}

#[cfg(test)]
impl VertexTool {
    pub(crate) fn select_at(&mut self, state: &AppState, positions: &[DVec3]) -> usize {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        cache.clear_selection();
        let mut picked = Vec::new();
        for (si, s) in cache.solids.iter().enumerate() {
            for (i, p) in s.points.iter().enumerate() {
                if positions.iter().any(|q| q.distance(*p) < 1e-6) {
                    picked.push((si, i));
                }
            }
        }
        cache.set_selected(&picked, true);
        picked.len()
    }

    pub(crate) fn select_edges_at(&mut self, state: &AppState, mids: &[DVec3]) -> usize {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        cache.clear_selection();
        let mut picked = Vec::new();
        for (si, s) in cache.solids.iter().enumerate() {
            for e in 0..s.edges.len() {
                if s.midpoint(e).is_some_and(|m| mids.iter().any(|q| q.distance(m) < 1e-6)) {
                    picked.push((si, e));
                }
            }
        }
        cache.set_edges_selected(&picked, true);
        picked.len()
    }

    pub(crate) fn scale_center(&self) -> Option<DVec3> {
        self.scale.as_ref().map(|s| s.center)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle_like_hammer() {
        let mut m = VertexMode::Both;
        let seen: Vec<VertexMode> = (0..4)
            .map(|_| {
                m = m.next();
                m
            })
            .collect();
        assert_eq!(seen, [VertexMode::Vertices, VertexMode::Edges, VertexMode::Both, VertexMode::Vertices]);
        assert_eq!(VertexMode::Edges.pickable(), Pickable { vertices: false, edges: true });
        assert_eq!(VertexMode::Vertices.pickable(), Pickable { vertices: true, edges: false });
        assert_eq!(VertexTool::new().mode, VertexMode::Both);
    }

    #[test]
    fn faces_whose_corners_are_all_selected_are_the_face_selection() {
        let mut state = hp_app::testing::test_state();
        state.doc.select(&[ObjRef::Solid(3)], SelectOp::Replace);
        let mut tool = VertexTool::new();
        tool.activate(&mut state);
        assert!(tool.face_selection(&state).is_empty());
        let top = {
            let cache = tool.cache.get_mut();
            let s = &mut cache.solids[0];
            let max = s.points.iter().map(|p| p.z).fold(f64::MIN, f64::max);
            for (i, p) in s.points.iter().enumerate() {
                s.selected[i] = (p.z - max).abs() < 1e-6;
            }
            s.face_sides[s.faces.iter().position(|f| f.iter().all(|i| s.selected[*i])).expect("top face")]
        };
        assert_eq!(tool.face_selection(&state), vec![hp_editor::FaceRef::new(3, top)]);
        tool.select_all_vertices(&state);
        assert!(tool.face_selection(&state).is_empty(), "a whole selected brush is not a face selection");
    }
}
