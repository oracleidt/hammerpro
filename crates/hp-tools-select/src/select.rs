mod draw;
mod element;
mod extra;
mod keys;
mod options;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use egui::{CursorIcon, Modifiers, Painter, Pos2, Rect};
use glam::{DMat4, DVec2, DVec3};
use hp_app::{AppState, Severity, Tool, ToolFlow, ToolId, ToolMenuEntry, ViewInfo, ViewInput, ViewKind, tf};
use hp_editor::{ObjRef, PickOptions, RectMode, SelectOp};
use hp_geom::{Aabb, Ray};
use hp_render::Overlay;

use crate::gizmo::{GizmoLayout, GizmoMode, GizmoPart, WORLD_AXES};
use crate::handles::{HANDLE_PICK, HelperHandle, helper_handles, value_with_point};
use crate::input::{PointerEvent, ctrl};
use crate::math::{
    Handle, HandleMode, angle_2d_deg, closest_param_on_line, fmt_len, hit_handle_sized, normalize_deg, ray_plane,
    rotation_about, scale_about, scale_rect_on, signed_angle_deg, skew_matrix, snap, snap_angle, stretch_box_on,
    view_axis,
};
use crate::modal::{Modal, ModalKind, MoveSnap, lock_scale, modal_angle, modal_factor, modal_move, rotation_axis};
use crate::preview::{MAX_PREVIEW_EDGES, PreviewGeom, push_outline, rgba};
use crate::stretch::{FaceHandle, face_center};

pub use keys::{NudgeKey, NudgeMod, NudgeView, nudge_delta, take_nudge_keys};
pub use options::sync_options;

pub const PICK_POINTS: f64 = 4.0;
const CYCLE_SLOP: f32 = 3.0;
const CYCLE_SECONDS: f64 = 2.0;
const MAX_CYCLE: usize = 64;
const DOUBLE_CLICK_SLOP: f32 = 6.0;
const MAX_DRAG: f64 = hp_geom::MAX_COORD * 2.0;
const PIVOT_PICK: f32 = 8.0;
const SCALE_RANGE: (f64, f64) = (0.01, 100.0);
pub const STRETCH_LABEL: &str = "Stretch";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueryKind {
    #[default]
    Class,
    Material,
}

#[derive(Clone, Debug, PartialEq)]
enum DragOp {
    Band { start: DVec2, current: DVec2 },
    Move { start: DVec2, delta: DVec3 },
    Scale { handle: Handle, start: DVec2, new_bounds: Aabb },
    Rotate { start_angle: f64, angle: f64 },
    Skew { handle: Handle, start: DVec2, amount: f64 },
    GizmoMove { part: GizmoPart, anchor: DVec3, delta: DVec3 },
    GizmoRotate { axis: usize, start_vec: Option<DVec3>, start_screen: f64, angle: f64 },
    GizmoScale { part: GizmoPart, start: f64, scale: DVec3 },
    Paint { remove: bool, done: HashSet<ObjRef> },
    Pivot { pos: DVec3 },
    Modal(Modal),
    Helper { entity: hp_editor::Id, handle: HelperHandle, pos: DVec3 },
    FaceStretch { axis: usize, positive: bool, start: f64, new_bounds: Aabb, dir: egui::Vec2, gap: f32 },
}

#[derive(Clone, Debug)]
struct Drag {
    view: usize,
    kind: ViewKind,
    op: DragOp,
    objs: Vec<ObjRef>,
    bounds: Aabb,
    snap_ref: DVec3,
    pivot: DVec3,
    axes: [DVec3; 3],
    extent: DVec3,
    preview: PreviewGeom,
    mods: Modifiers,
    last_pos: Pos2,
    revision: u64,
    elements: bool,
    ghost: Vec<[DVec3; 2]>,
    ghost_points: Vec<DVec3>,
}

impl Drag {
    fn transforms(&self) -> bool {
        !matches!(self.op, DragOp::Band { .. } | DragOp::Paint { .. } | DragOp::Pivot { .. } | DragOp::Helper { .. })
    }

    fn edits(&self) -> bool {
        self.transforms() || matches!(self.op, DragOp::Helper { .. })
    }

    fn is_modal(&self) -> bool {
        matches!(self.op, DragOp::Modal(_))
    }

    fn is_clone(&self) -> bool {
        match self.op {
            DragOp::Move { .. }
            | DragOp::GizmoMove { .. }
            | DragOp::Scale { .. }
            | DragOp::Rotate { .. }
            | DragOp::Skew { .. }
            | DragOp::GizmoRotate { .. }
            | DragOp::GizmoScale { .. }
            | DragOp::FaceStretch { .. } => self.mods.shift && !self.elements,
            _ => false,
        }
    }

    fn matrix(&self) -> DMat4 {
        match &self.op {
            DragOp::Band { .. } | DragOp::Paint { .. } | DragOp::Pivot { .. } | DragOp::Helper { .. } => {
                DMat4::IDENTITY
            }
            DragOp::Move { delta, .. } | DragOp::GizmoMove { delta, .. } => DMat4::from_translation(*delta),
            DragOp::Scale { new_bounds, .. } | DragOp::FaceStretch { new_bounds, .. } => {
                crate::math::box_to_box(self.bounds.min, self.bounds.max, new_bounds.min, new_bounds.max)
            }
            DragOp::Rotate { angle, .. } => rotation_about(self.pivot, self.kind.toward_viewer(), *angle),
            DragOp::Skew { handle, amount, .. } => self.skew(*handle, *amount).unwrap_or(DMat4::IDENTITY),
            DragOp::GizmoRotate { axis, angle, .. } => rotation_about(self.pivot, self.axes[*axis % 3], *angle),
            DragOp::GizmoScale { scale, .. } => scale_about(self.pivot, self.axes, *scale),
            DragOp::Modal(m) => m.matrix(self.pivot),
        }
    }

    fn is_identity(&self) -> bool {
        match &self.op {
            DragOp::Move { delta, .. } | DragOp::GizmoMove { delta, .. } => delta.length_squared() <= 1e-12,
            DragOp::Scale { new_bounds, .. } | DragOp::FaceStretch { new_bounds, .. } => *new_bounds == self.bounds,
            DragOp::Rotate { angle, .. } | DragOp::GizmoRotate { angle, .. } => angle.abs() <= 1e-9,
            DragOp::Skew { amount, .. } => amount.abs() <= 1e-9,
            DragOp::GizmoScale { scale, .. } => (*scale - DVec3::ONE).abs().max_element() <= 1e-9,
            DragOp::Modal(m) => m.is_identity(),
            DragOp::Band { .. } | DragOp::Paint { .. } | DragOp::Pivot { .. } | DragOp::Helper { .. } => true,
        }
    }

    fn skew(&self, handle: Handle, amount: f64) -> Option<DMat4> {
        let (r, u, _) = self.kind.axes_2d()?;
        let min = DVec2::new(self.bounds.min[r], self.bounds.min[u]);
        let max = DVec2::new(self.bounds.max[r], self.bounds.max[u]);
        skew_matrix(r, u, handle, min, max, amount)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClickCycle {
    view: usize,
    pos: Pos2,
    candidates: Vec<Vec<ObjRef>>,
    index: usize,
    time: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PickClick {
    view: usize,
    pos: Pos2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HoverKey {
    view: usize,
    pos: Pos2,
    revision: u64,
    cam_pos: DVec3,
    yaw: f64,
    pitch: f64,
    mode: hp_editor::SelectMode,
}

#[derive(Clone, Debug, Default)]
struct Hover {
    view: Option<usize>,
    key: Option<HoverKey>,
    objects: Vec<ObjRef>,
    gizmo: Option<GizmoPart>,
    stretch: Option<(usize, bool)>,
    handle: Option<Handle>,
    pivot: bool,
    helper: Option<String>,
}

impl Hover {
    fn objects_for(&self, mode: hp_editor::SelectMode) -> &[ObjRef] {
        match self.key {
            Some(k) if k.mode == mode => &self.objects,
            _ => &[],
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SelCache {
    revision: u64,
    frame: u64,
    bounds: Option<Aabb>,
    brushes: bool,
}

pub struct SelectTool {
    handle_mode: HandleMode,
    box_inside: bool,
    centers_only: bool,
    pub(crate) gizmo_visible: bool,
    gizmo_mode: GizmoMode,
    drag: Option<Drag>,
    hover: Hover,
    cycle: Option<ClickCycle>,
    mode_before_click: Option<HandleMode>,
    pick_click: Option<PickClick>,
    query: String,
    query_kind: QueryKind,
    query_result: Option<(usize, f64)>,
    views: Vec<Option<ViewInfo>>,
    sel_cache: Cell<Option<SelCache>>,
    sel_sig: Cell<Option<(u64, u64)>>,
    pivot_sig: Option<u64>,
    wheel_note: Option<(String, f64)>,
    centers: RefCell<Vec<CenterKey>>,
}

type CenterKey = ((usize, u64, [u64; 4]), Vec<DVec3>);

impl Default for SelectTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectTool {
    pub fn new() -> Self {
        Self {
            handle_mode: HandleMode::Scale,
            box_inside: false,
            centers_only: false,
            gizmo_visible: true,
            gizmo_mode: GizmoMode::Move,
            drag: None,
            hover: Hover::default(),
            cycle: None,
            mode_before_click: None,
            pick_click: None,
            query: String::new(),
            query_kind: QueryKind::Class,
            query_result: None,
            views: Vec::new(),
            sel_cache: Cell::new(None),
            sel_sig: Cell::new(None),
            pivot_sig: None,
            wheel_note: None,
            centers: RefCell::new(Vec::new()),
        }
    }

    pub fn handle_mode(&self) -> HandleMode {
        self.handle_mode
    }

    pub fn set_handle_mode(&mut self, mode: HandleMode) {
        self.handle_mode = mode;
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn set_box_inside(&mut self, inside: bool) {
        self.box_inside = inside;
    }

    pub fn set_centers_only(&mut self, on: bool) {
        self.centers_only = on;
    }

    pub fn set_gizmo_rotate(&mut self, rotate: bool) {
        self.gizmo_mode = if rotate { GizmoMode::Rotate } else { GizmoMode::Move };
    }

    pub fn set_gizmo_mode(&mut self, mode: GizmoMode) {
        self.gizmo_mode = mode;
    }

    fn note_view(&mut self, view: &ViewInfo) {
        if view.index < 64 {
            if self.views.len() <= view.index {
                self.views.resize(view.index + 1, None);
            }
            self.views[view.index] = Some(*view);
        }
    }

    fn kind_of(&self, state: &AppState, index: usize) -> Option<ViewKind> {
        self.views
            .get(index)
            .copied()
            .flatten()
            .map(|v| v.kind)
            .or_else(|| state.settings.ui.quadrants.get(index).map(|q| ViewKind::from_index(*q)))
    }

    fn handle_size(state: &AppState) -> f32 {
        state.settings.selection.handle_points()
    }

    fn rotation_snap(state: &AppState, mods: Modifiers) -> (f64, bool) {
        let s = &state.settings.selection;
        let free = if s.free_rotation_with_alt { mods.alt } else { ctrl(mods) };
        (s.rotation_step(), free)
    }

    fn selection_bounds(&self, state: &AppState) -> Option<Aabb> {
        self.selection_info(state).and_then(|c| c.bounds)
    }

    fn selection_info(&self, state: &AppState) -> Option<SelCache> {
        let (revision, frame) = (state.doc.revision(), state.frame.index);
        if let Some(c) = self.sel_cache.get()
            && c.revision == revision
            && c.frame == frame
        {
            return Some(c);
        }
        if let Some(bounds) = element::picked_bounds(state) {
            let info = SelCache { revision, frame, bounds: Some(bounds), brushes: false };
            self.sel_cache.set(Some(info));
            return Some(info);
        }
        if state.doc.selection().objects.is_empty() {
            return None;
        }
        let scene = state.doc.scene();
        let mut b = Aabb::EMPTY;
        let mut brushes = false;
        for o in &state.doc.selection().objects {
            if let Some(ob) = scene.object_bounds(*o, &|e| state.entity_bounds(e)) {
                b = b.union(&ob);
            }
            brushes = brushes
                || match *o {
                    ObjRef::Solid(_) => true,
                    ObjRef::Entity(id) => scene.is_brush_entity(id),
                };
        }
        let bounds = (!b.is_empty()).then_some(b);
        let info = SelCache { revision, frame, bounds, brushes };
        self.sel_cache.set(Some(info));
        Some(info)
    }

    pub(crate) fn stretchable(&self, state: &AppState) -> bool {
        self.selection_info(state).is_some_and(|c| c.brushes && c.bounds.is_some())
    }

    fn selection_sig(&self, state: &AppState) -> u64 {
        use std::hash::{Hash, Hasher};
        let revision = state.doc.revision();
        if let Some((r, sig)) = self.sel_sig.get()
            && r == revision
        {
            return sig;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let objects = &state.doc.selection().objects;
        objects.len().hash(&mut h);
        for o in objects {
            o.hash(&mut h);
        }
        let sig = h.finish();
        self.sel_sig.set(Some((revision, sig)));
        sig
    }

    fn active_pivot(&self, state: &AppState) -> Option<DVec3> {
        let p = state.pivot.filter(|p| p.is_finite())?;
        self.pivot_sig.is_none_or(|s| s == self.selection_sig(state)).then_some(p)
    }

    fn sync_pivot(&mut self, state: &mut AppState) {
        let Some(p) = state.pivot else {
            self.pivot_sig = None;
            return;
        };
        let sig = self.selection_sig(state);
        match self.pivot_sig {
            _ if !p.is_finite() => {
                state.pivot = None;
                self.pivot_sig = None;
            }
            None => self.pivot_sig = Some(sig),
            Some(s) if s != sig => {
                state.pivot = None;
                self.pivot_sig = None;
            }
            Some(_) => {}
        }
    }

    fn snap_on(state: &AppState, mods: Modifiers) -> bool {
        state.snap_enabled() && !mods.alt
    }

    fn report(state: &mut AppState, r: Result<(), hp_editor::EditError>) {
        if let Err(e) = r {
            log::warn!("selection tool: {e}");
            state.notify(Severity::Warning, tf!("seltool.transform_failed", e));
        }
    }

    pub(crate) fn handle_event(
        &mut self,
        state: &mut AppState,
        view: &ViewInfo,
        ev: PointerEvent,
        mods: Modifiers,
    ) -> ToolFlow {
        let modal = self.drag.as_ref().is_some_and(Drag::is_modal);
        match ev {
            PointerEvent::Hover(p) => {
                if modal {
                    self.update_drag(state, view, p, mods);
                } else if self.drag.is_none() {
                    self.update_hover(state, view, p);
                }
                ToolFlow::Ignored
            }
            PointerEvent::Leave => {
                if self.hover.view == Some(view.index) {
                    self.hover = Hover::default();
                }
                ToolFlow::Ignored
            }
            PointerEvent::DragStart { origin, pos } => {
                if modal {
                    self.finish_drag(state);
                    return ToolFlow::Handled;
                }
                if self.drag.is_some() {
                    return ToolFlow::Handled;
                }
                self.hover = Hover::default();
                self.cycle = None;
                self.pick_click = None;
                if view.is_2d() {
                    self.start_drag_2d(state, view, origin, mods);
                } else {
                    self.start_drag_3d(state, view, origin, mods);
                }
                self.drag_to(state, view, pos, mods);
                ToolFlow::Handled
            }
            PointerEvent::Drag(p) => {
                if self.drag.as_ref().is_some_and(|d| d.view == view.index && !d.is_modal()) {
                    self.drag_to(state, view, p, mods);
                    ToolFlow::Handled
                } else {
                    ToolFlow::Ignored
                }
            }
            PointerEvent::DragEnd(p) => {
                if self.drag.as_ref().is_some_and(|d| d.view == view.index && !d.is_modal()) {
                    self.drag_to(state, view, p, mods);
                    self.finish_drag(state);
                    ToolFlow::Handled
                } else {
                    ToolFlow::Ignored
                }
            }
            PointerEvent::Click(p) | PointerEvent::DoubleClick(p) if modal => {
                if self.drag.as_ref().is_some_and(|d| d.view == view.index) {
                    self.update_drag(state, view, p, mods);
                }
                self.finish_drag(state);
                ToolFlow::Handled
            }
            PointerEvent::Click(p) => {
                if self.drag.is_none() {
                    if view.is_2d() {
                        self.click_2d(state, view, p, mods);
                    } else {
                        self.click_3d(state, view, p, mods);
                    }
                }
                ToolFlow::Handled
            }
            PointerEvent::DoubleClick(p) => {
                if self.drag.is_none() {
                    self.double_click(state, view, p);
                }
                ToolFlow::Handled
            }
        }
    }

    fn drag_to(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        if self.drag.as_ref().is_some_and(|d| matches!(d.op, DragOp::Paint { .. })) {
            self.paint_at(state, view, pos, mods);
        } else {
            self.update_drag(state, view, pos, mods);
        }
    }

    fn update_hover(&mut self, state: &AppState, view: &ViewInfo, pos: Pos2) {
        self.hover.view = Some(view.index);
        self.hover.helper = self.helper_hit(state, view, pos).map(|(_, h)| h.key);
        if view.is_2d() {
            self.hover.objects.clear();
            self.hover.key = None;
            self.hover.gizmo = None;
            self.hover.stretch = None;
            self.hover.pivot = self.pivot_hit(state, view, pos);
            let size = Self::handle_size(state);
            self.hover.handle = if self.hover.pivot || self.hover.helper.is_some() {
                None
            } else {
                self.selection_bounds(state)
                    .and_then(|b| hit_handle_sized(screen_rect(view, &b), self.handle_mode, pos, size))
            };
            return;
        }
        self.hover.handle = None;
        self.hover.pivot = self.hover.helper.is_none() && self.pivot_first_3d(state, view, pos);
        let taken = self.hover.helper.is_some() || self.hover.pivot;
        self.hover.stretch = if taken { None } else { self.stretch_hit(state, view, pos) };
        self.hover.gizmo = if taken || self.hover.stretch.is_some() { None } else { self.gizmo_hit(state, view, pos) };
        if self.hover.gizmo.is_some() || self.hover.stretch.is_some() {
            self.hover.objects.clear();
            self.hover.key = None;
            return;
        }
        let cam = view.cam3d;
        let key = HoverKey {
            view: view.index,
            pos,
            revision: state.doc.revision(),
            cam_pos: cam.pos,
            yaw: cam.yaw_deg,
            pitch: cam.pitch_deg,
            mode: state.select_mode(),
        };
        if self.hover.key == Some(key) {
            return;
        }
        self.hover.key = Some(key);
        let (o, d) = view.ray(pos);
        let hit = state.view_pick(&Ray::new(o, d), &PickOptions::default());
        self.hover.objects = hit.map(|h| state.doc.resolve(h.obj, state.select_mode())).unwrap_or_default();
    }

    fn pivot_hit(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> bool {
        self.active_pivot(state).and_then(|p| view.world_to_screen(p)).is_some_and(|s| s.distance(pos) <= PIVOT_PICK)
    }

    fn pivot_first_3d(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> bool {
        !view.is_2d() && self.gizmo_mode != GizmoMode::Scale && self.pivot_hit(state, view, pos)
    }

    fn helper_points(&self, state: &AppState) -> Option<(hp_editor::Id, Vec<HelperHandle>)> {
        let sel = &state.doc.selection().objects;
        let (1, Some(ObjRef::Entity(id))) = (sel.len(), sel.first().copied()) else { return None };
        let scene = state.doc.scene();
        let node = scene.entity(id)?;
        let list = helper_handles(state.assets.fgd(), &node.entity, scene.is_brush_entity(id));
        (!list.is_empty()).then_some((id, list))
    }

    fn helper_hit(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<(hp_editor::Id, HelperHandle)> {
        let (id, list) = self.helper_points(state)?;
        list.into_iter()
            .filter_map(|h| view.world_to_screen(h.pos).map(|s| (s.distance(pos), h)))
            .filter(|(d, _)| *d <= HANDLE_PICK)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, h)| (id, h))
    }

    pub(crate) fn stretch_handles(&self, state: &AppState, view: &ViewInfo) -> Vec<FaceHandle> {
        if view.is_2d() || !state.settings.selection.stretch_handles || !self.stretchable(state) {
            return Vec::new();
        }
        let Some(b) = self.selection_bounds(state) else { return Vec::new() };
        let gizmo = if self.gizmo_visible {
            self.gizmo_center(state).and_then(|c| GizmoLayout::with_axes(view, c, self.gizmo_axes(state)))
        } else {
            None
        };
        let mode = self.effective_gizmo_mode(state);
        let blocked =
            |p: Pos2| gizmo.is_some_and(|g| g.hit(view, p, mode).is_some()) || self.pivot_first_3d(state, view, p);
        crate::stretch::layout(view, &b, &blocked)
    }

    pub(crate) fn stretch_hit(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<(usize, bool)> {
        crate::stretch::hit(&self.stretch_handles(state, view), pos)
    }

    fn on_3d_handle(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> bool {
        self.gizmo_hit(state, view, pos).is_some() || self.stretch_hit(state, view, pos).is_some()
    }

    fn gizmo_hit(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<GizmoPart> {
        if !self.gizmo_visible || view.is_2d() {
            return None;
        }
        let center = self.gizmo_center(state)?;
        GizmoLayout::with_axes(view, center, self.gizmo_axes(state))?.hit(view, pos, self.effective_gizmo_mode(state))
    }

    fn effective_gizmo_mode(&self, state: &AppState) -> GizmoMode {
        if element::picked_points(state).is_empty() { self.gizmo_mode } else { GizmoMode::Move }
    }

    fn gizmo_center(&self, state: &AppState) -> Option<DVec3> {
        let bounds = self.selection_bounds(state)?;
        if let Some(p) = self.active_pivot(state) {
            return Some(p);
        }
        let sel = &state.doc.selection().objects;
        Some(if sel.len() == 1 {
            let objs: Vec<ObjRef> = sel.iter().copied().collect();
            single_point_entity_origin(state, &objs).unwrap_or_else(|| bounds.center())
        } else {
            bounds.center()
        })
    }

    fn pivot_for(&self, state: &AppState, objs: &[ObjRef], bounds: &Aabb) -> DVec3 {
        self.active_pivot(state).or_else(|| single_point_entity_origin(state, objs)).unwrap_or_else(|| bounds.center())
    }

    fn gizmo_axes(&self, state: &AppState) -> [DVec3; 3] {
        if !state.settings.selection.gizmo_local {
            return WORLD_AXES;
        }
        let sel = &state.doc.selection().objects;
        let (1, Some(ObjRef::Entity(id))) = (sel.len(), sel.first().copied()) else { return WORLD_AXES };
        let scene = state.doc.scene();
        if scene.is_brush_entity(id) {
            return WORLD_AXES;
        }
        let Some(angles) = scene.entity(id).and_then(|n| n.entity.angles()).filter(|a| a.is_finite()) else {
            return WORLD_AXES;
        };
        let m = hp_geom::angle_matrix(angles);
        match (m.x_axis.try_normalize(), m.y_axis.try_normalize(), m.z_axis.try_normalize()) {
            (Some(x), Some(y), Some(z)) => [x, y, z],
            _ => WORLD_AXES,
        }
    }

    fn begin_transform(&mut self, state: &AppState, view: &ViewInfo, op: DragOp, pos: Pos2, mods: Modifiers) {
        let Some(bounds) = self.selection_bounds(state) else { return };
        let elements = !element::picked_points(state).is_empty();
        let moving = matches!(op, DragOp::Move { .. } | DragOp::GizmoMove { .. })
            || matches!(&op, DragOp::Modal(m) if m.kind == ModalKind::Move);
        if elements && !moving {
            return;
        }
        let objs = if elements { Vec::new() } else { state.doc.selected_objects() };
        let (ghost, ghost_points) = if elements { element::picked_ghost(state) } else { (Vec::new(), Vec::new()) };
        let preview = PreviewGeom::capture(&state.doc, &objs, &|e| state.entity_bounds(e));
        let snap_ref = snap_reference(state, &objs, &bounds);
        let pivot = self.pivot_for(state, &objs, &bounds);
        let axes = self.gizmo_axes(state);
        let mut extent = DVec3::ZERO;
        for (k, e) in extent.as_mut().iter_mut().enumerate() {
            let (lo, hi) = bounds
                .corners()
                .iter()
                .map(|c| c.dot(axes[k]))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| (lo.min(d), hi.max(d)));
            *e = (hi - lo).max(0.0);
        }
        self.drag = Some(Drag {
            view: view.index,
            kind: view.kind,
            op,
            objs,
            bounds,
            snap_ref,
            pivot,
            axes,
            extent,
            preview,
            mods,
            last_pos: pos,
            revision: state.doc.revision(),
            elements,
            ghost,
            ghost_points,
        });
    }

    fn begin_plain(&mut self, state: &AppState, view: &ViewInfo, op: DragOp, pos: Pos2, mods: Modifiers) {
        self.drag = Some(Drag {
            view: view.index,
            kind: view.kind,
            op,
            objs: Vec::new(),
            bounds: Aabb::EMPTY,
            snap_ref: DVec3::ZERO,
            pivot: DVec3::ZERO,
            axes: WORLD_AXES,
            extent: DVec3::ZERO,
            preview: PreviewGeom::default(),
            mods,
            last_pos: pos,
            revision: state.doc.revision(),
            elements: false,
            ghost: Vec::new(),
            ghost_points: Vec::new(),
        });
    }

    fn start_drag_2d(&mut self, state: &mut AppState, view: &ViewInfo, origin: Pos2, mods: Modifiers) {
        let Some(axis) = view_axis(view.kind) else { return };
        let p2 = view.screen_to_2d(origin);
        let ctrl = ctrl(mods);
        if let Some(p) = self.active_pivot(state)
            && self.pivot_hit(state, view, origin)
        {
            self.begin_plain(state, view, DragOp::Pivot { pos: p }, origin, mods);
            return;
        }
        if let Some((entity, handle)) = self.helper_hit(state, view, origin) {
            let pos = handle.pos;
            self.begin_plain(state, view, DragOp::Helper { entity, handle, pos }, origin, mods);
            return;
        }
        if let Some(b) = self.selection_bounds(state) {
            let rect = screen_rect(view, &b);
            if let Some(h) = hit_handle_sized(rect, self.handle_mode, origin, Self::handle_size(state)) {
                let op = match self.handle_mode {
                    HandleMode::Scale => DragOp::Scale { handle: h, start: p2, new_bounds: b },
                    HandleMode::Rotate => {
                        let pivot = self.gizmo_center(state).unwrap_or_else(|| b.center());
                        let c = view.kind.to_2d(pivot);
                        DragOp::Rotate { start_angle: angle_2d_deg(p2 - c), angle: 0.0 }
                    }
                    HandleMode::Skew => DragOp::Skew { handle: h, start: p2, amount: 0.0 },
                };
                self.begin_transform(state, view, op, origin, mods);
                return;
            }
            if !ctrl && rect.expand(1.0).contains(origin) {
                self.begin_transform(state, view, DragOp::Move { start: p2, delta: DVec3::ZERO }, origin, mods);
                return;
            }
        }
        if !ctrl {
            let picks = self.picks_2d(state, view, origin, axis);
            if let Some(first) = picks.first().copied() {
                state.doc.select_picked(first, state.select_mode(), SelectOp::Replace);
                self.handle_mode = HandleMode::Scale;
                self.begin_transform(state, view, DragOp::Move { start: p2, delta: DVec3::ZERO }, origin, mods);
                return;
            }
        }
        self.begin_plain(state, view, DragOp::Band { start: p2, current: p2 }, origin, mods);
    }

    fn picks_2d(&self, state: &AppState, view: &ViewInfo, pos: Pos2, axis: hp_geom::ViewAxis) -> Vec<ObjRef> {
        let p2 = view.screen_to_2d(pos);
        let tol = PICK_POINTS * view.units_per_point();
        let mut picks = state.doc.pick_2d(axis, p2, tol, &|e| state.entity_bounds(e));
        if self.centers_only {
            let scene = state.doc.scene();
            picks.retain(|o| {
                scene
                    .object_bounds(*o, &|e| state.entity_bounds(e))
                    .is_some_and(|b| axis.to_2d(b.center()).distance(p2) <= tol)
            });
        }
        picks
    }

    fn start_drag_3d(&mut self, state: &mut AppState, view: &ViewInfo, origin: Pos2, mods: Modifiers) {
        if let Some((entity, handle)) = self.helper_hit(state, view, origin) {
            let pos = handle.pos;
            self.begin_plain(state, view, DragOp::Helper { entity, handle, pos }, origin, mods);
            return;
        }
        if let Some(p) = self.active_pivot(state)
            && !ctrl(mods)
            && self.pivot_first_3d(state, view, origin)
        {
            self.begin_plain(state, view, DragOp::Pivot { pos: p }, origin, mods);
            return;
        }
        let handles = self.stretch_handles(state, view);
        if let Some((axis, positive)) = crate::stretch::hit(&handles, origin)
            && let Some(bounds) = self.selection_bounds(state)
        {
            let (o, d) = view.ray(origin);
            let face = face_center(&bounds, axis, positive);
            let Some(start) = closest_param_on_line(o, d, face, WORLD_AXES[axis]) else { return };
            let (dir, gap) = handles
                .iter()
                .find(|h| h.id() == (axis, positive))
                .map_or((egui::Vec2::X, crate::stretch::GAP), |h| (h.dir, h.knob.distance(h.face)));
            let op = DragOp::FaceStretch { axis, positive, start, new_bounds: bounds, dir, gap };
            self.begin_transform(state, view, op, origin, mods);
            return;
        }
        let part = self.gizmo_hit(state, view, origin);
        let (Some(part), Some(c)) = (part, self.gizmo_center(state)) else {
            if state.settings.selection.element != hp_app::settings::SelectElement::Object {
                if let Some(picked) = element::pick(state, view, origin, state.settings.selection.element) {
                    if !element::is_selected(state, picked) {
                        element::click(state, view, origin, mods);
                        self.sel_cache.set(None);
                    }
                    if let Some(bounds) = element::picked_bounds(state) {
                        let center = self.active_pivot(state).unwrap_or_else(|| bounds.center());
                        let (o, d) = view.ray(origin);
                        if let Some(anchor) = ray_plane(o, d, center, view.cam3d.forward()) {
                            self.begin_transform(
                                state,
                                view,
                                DragOp::GizmoMove { part: GizmoPart::Center, anchor, delta: DVec3::ZERO },
                                origin,
                                mods,
                            );
                        }
                    }
                } else {
                    element::click(state, view, origin, mods);
                    self.sel_cache.set(None);
                }
                return;
            }
            if let Some(p) = self.active_pivot(state)
                && !ctrl(mods)
                && self.pivot_hit(state, view, origin)
            {
                self.begin_plain(state, view, DragOp::Pivot { pos: p }, origin, mods);
            } else if ctrl(mods) {
                self.begin_plain(state, view, DragOp::Paint { remove: mods.shift, done: HashSet::new() }, origin, mods);
                self.paint_at(state, view, origin, mods);
            } else {
                self.click_3d(state, view, origin, mods);
            }
            return;
        };
        let axes = self.gizmo_axes(state);
        let (o, d) = view.ray(origin);
        let op = match (self.effective_gizmo_mode(state), part) {
            (GizmoMode::Scale, GizmoPart::Axis(k)) => {
                let Some(t) = closest_param_on_line(o, d, c, axes[k % 3]).filter(|t| t.abs() > 1e-6) else { return };
                DragOp::GizmoScale { part, start: t, scale: DVec3::ONE }
            }
            (GizmoMode::Scale, GizmoPart::Plane(n)) => {
                let Some(hit) = ray_plane(o, d, c, axes[n % 3]) else { return };
                let start = (hit - c).length();
                if start < 1e-6 {
                    return;
                }
                DragOp::GizmoScale { part, start, scale: DVec3::ONE }
            }
            (_, GizmoPart::Center) => {
                let Some(cs) = view.world_to_screen(c) else { return };
                let start = f64::from(cs.distance(origin)).max(4.0);
                DragOp::GizmoScale { part, start, scale: DVec3::ONE }
            }
            (_, GizmoPart::Axis(k)) => {
                let Some(t) = closest_param_on_line(o, d, c, axes[k % 3]) else { return };
                DragOp::GizmoMove { part, anchor: c + axes[k % 3] * t, delta: DVec3::ZERO }
            }
            (_, GizmoPart::Plane(n)) => {
                let Some(hit) = ray_plane(o, d, c, axes[n % 3]) else { return };
                DragOp::GizmoMove { part, anchor: hit, delta: DVec3::ZERO }
            }
            (_, GizmoPart::Ring(k)) => {
                let e = axes[k % 3];
                let start_vec = if d.dot(e).abs() > 0.15 { ray_plane(o, d, c, e).map(|h| h - c) } else { None };
                let start_screen =
                    view.world_to_screen(c).map(|cs| crate::modal::screen_angle(cs, origin)).unwrap_or(0.0);
                DragOp::GizmoRotate { axis: k % 3, start_vec, start_screen, angle: 0.0 }
            }
        };
        self.begin_transform(state, view, op, origin, mods);
    }

    fn start_modal(&mut self, state: &mut AppState, view: &ViewInfo, kind: ModalKind, pos: Pos2) {
        if self.drag.is_some() || state.doc.selection().objects.is_empty() {
            return;
        }
        let axes = self.gizmo_axes(state);
        self.hover = Hover::default();
        self.cycle = None;
        self.begin_transform(state, view, DragOp::Modal(Modal::new(kind, pos, axes)), pos, Modifiers::NONE);
        if self.drag.is_some() {
            state.runtime.selection.modal_transform = true;
        }
    }

    fn paint_at(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        let Some(d) = self.drag.as_mut() else { return };
        if d.view != view.index {
            return;
        }
        d.last_pos = pos;
        d.mods = mods;
        let DragOp::Paint { remove, done } = &mut d.op else { return };
        let (o, dir) = view.ray(pos);
        let Some(hit) = state.view_pick(&Ray::new(o, dir), &PickOptions::default()) else {
            return;
        };
        if !done.insert(hit.obj) {
            return;
        }
        let objs = state.doc.resolve(hit.obj, state.select_mode());
        let op = if *remove { SelectOp::Remove } else { SelectOp::Add };
        state.doc.select(&objs, op);
    }

    fn update_drag(&mut self, state: &AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        let frame = state.edit_grid_frame(view.kind);
        let grid = frame.nominal();
        let snap_on = Self::snap_on(state, mods);
        let (step, free) = Self::rotation_snap(state, mods);
        if self.drag.as_ref().is_some_and(|d| d.edits() && d.revision != state.doc.revision()) {
            self.drag = None;
            return;
        }
        let Some(d) = self.drag.as_mut() else { return };
        if d.view != view.index {
            return;
        }
        d.mods = mods;
        d.last_pos = pos;
        let p2 = view.screen_to_2d(pos);
        let axes2d = d.kind.axes_2d();
        let bounds = d.bounds;
        let snap_ref = d.snap_ref;
        let pivot = d.pivot;
        let kind = d.kind;
        let axes = d.axes;
        let extent = d.extent;
        match &mut d.op {
            DragOp::Band { current, .. } => *current = p2,
            DragOp::Paint { .. } => {}
            DragOp::Helper { handle, pos: p, .. } => {
                let start = handle.pos;
                let next = match axes2d {
                    Some((r, u, _)) => {
                        let mut q = start;
                        let t = if snap_on { frame.snap_2d(kind, p2) } else { p2 };
                        q[r] = t.x;
                        q[u] = t.y;
                        Some(q)
                    }
                    None => {
                        let n = crate::math::axis_vec(view.cam3d.forward().abs().max_position());
                        let (o, dir) = view.ray(pos);
                        ray_plane(o, dir, start, n).map(|hit| {
                            let mut q = hit.clamp(DVec3::splat(-MAX_DRAG), DVec3::splat(MAX_DRAG));
                            for k in 0..3 {
                                if n[k] == 0.0 && snap_on {
                                    q[k] = frame.axis(k).snap(q[k]);
                                }
                            }
                            q
                        })
                    }
                };
                if let Some(q) = next.filter(|q| q.is_finite()) {
                    *p = q;
                }
            }
            DragOp::Pivot { pos: p } => {
                if let Some((_, _, dk)) = axes2d {
                    let depth = p[dk];
                    let target = if snap_on { frame.snap_2d(kind, p2) } else { p2 };
                    let mut next = kind.from_2d(target, 0.0);
                    next[dk] = depth;
                    *p = next;
                } else {
                    let n = crate::math::axis_vec(view.cam3d.forward().abs().max_position());
                    let (o, dir) = view.ray(pos);
                    if let Some(hit) = ray_plane(o, dir, *p, n) {
                        let mut q = hit.clamp(DVec3::splat(-MAX_DRAG), DVec3::splat(MAX_DRAG));
                        for k in 0..3 {
                            if n[k] == 0.0 && snap_on {
                                q[k] = frame.axis(k).snap(q[k]);
                            }
                        }
                        *p = q;
                    }
                }
            }
            DragOp::Move { start, delta } => {
                if let Some((r, u, _)) = axes2d {
                    let reference = DVec2::new(snap_ref[r], snap_ref[u]);
                    let raw = p2 - *start;
                    let d2 = if snap_on { frame.snap_delta_2d(kind, reference, raw) } else { raw };
                    *delta = kind.from_2d(d2, 0.0);
                }
            }
            DragOp::Scale { handle, start, new_bounds } => {
                if let Some((r, u, _)) = axes2d {
                    let min = DVec2::new(bounds.min[r], bounds.min[u]);
                    let max = DVec2::new(bounds.max[r], bounds.max[u]);
                    let axes = [frame.axis(r), frame.axis(u)];
                    let (lo, hi) = scale_rect_on(min, max, *handle, p2 - *start, axes, snap_on, ctrl(mods));
                    let mut nb = bounds;
                    nb.min[r] = lo.x;
                    nb.min[u] = lo.y;
                    nb.max[r] = hi.x;
                    nb.max[u] = hi.y;
                    *new_bounds = nb;
                }
            }
            DragOp::Rotate { start_angle, angle } => {
                let c = kind.to_2d(pivot);
                let v = p2 - c;
                if v.length_squared() > 1e-12 {
                    let raw = normalize_deg(angle_2d_deg(v) - *start_angle);
                    *angle = normalize_deg(snap_angle(raw, step, free));
                }
            }
            DragOp::Skew { handle, start, amount } => {
                let raw = p2 - *start;
                let along = if handle.y != 0 { raw.x } else { raw.y };
                *amount = if snap_on { snap(along, grid) } else { along };
            }
            DragOp::GizmoMove { part, anchor, delta } => {
                let (o, dir) = view.ray(pos);
                let c = pivot;
                let snap_along = |a: DVec3, raw: f64| {
                    let k = a.abs().max_position();
                    if a[k].abs() > 1.0 - 1e-9 {
                        let s = a[k].signum();
                        if snap_on { s * (frame.axis(k).snap(snap_ref[k] + raw * s) - snap_ref[k]) } else { raw }
                    } else if snap_on {
                        snap(raw, grid)
                    } else {
                        raw
                    }
                };
                match *part {
                    GizmoPart::Axis(k) => {
                        let a = axes[k % 3];
                        if let Some(t) = closest_param_on_line(o, dir, c, a) {
                            let raw = (c + a * t - *anchor).dot(a).clamp(-MAX_DRAG, MAX_DRAG);
                            *delta = a * snap_along(a, raw);
                        }
                    }
                    GizmoPart::Plane(n) => {
                        if let Some(hit) = ray_plane(o, dir, c, axes[n % 3]) {
                            let raw = (hit - *anchor).clamp(DVec3::splat(-MAX_DRAG), DVec3::splat(MAX_DRAG));
                            let mut out = DVec3::ZERO;
                            for k in [(n + 1) % 3, (n + 2) % 3] {
                                let a = axes[k];
                                out += a * snap_along(a, raw.dot(a));
                            }
                            *delta = out;
                        }
                    }
                    GizmoPart::Center => {
                        if let Some(start) = view.world_to_screen(*anchor) {
                            *delta = modal_move(
                                view,
                                c,
                                axes,
                                crate::modal::Lock::Free,
                                start,
                                pos,
                                snap_on.then_some(MoveSnap { reference: snap_ref, grid, frame }),
                            );
                        }
                    }
                    GizmoPart::Ring(_) => {}
                }
            }
            DragOp::GizmoRotate { axis, start_vec, start_screen, angle } => {
                let c = pivot;
                let e = axes[*axis % 3];
                let raw = match start_vec {
                    Some(v0) => {
                        let (o, dir) = view.ray(pos);
                        ray_plane(o, dir, c, e).map(|h| signed_angle_deg(*v0, h - c, e))
                    }
                    None => view.world_to_screen(c).map(|cs| {
                        let sign = if e.dot(view.cam3d.pos - c) >= 0.0 { 1.0 } else { -1.0 };
                        (crate::modal::screen_angle(cs, pos) - *start_screen) * sign
                    }),
                };
                if let Some(raw) = raw {
                    *angle = normalize_deg(snap_angle(normalize_deg(raw), step, free));
                }
            }
            DragOp::FaceStretch { axis, positive, start, new_bounds, .. } => {
                let (o, dir) = view.ray(pos);
                let face = face_center(&bounds, *axis, *positive);
                if let Some(t) = closest_param_on_line(o, dir, face, WORLD_AXES[*axis % 3]) {
                    let d = (t - *start).clamp(-MAX_DRAG, MAX_DRAG);
                    let symmetric = ctrl(mods);
                    let g = frame.axis(*axis % 3);
                    let (lo, hi) = stretch_box_on(bounds.min, bounds.max, *axis, *positive, d, g, snap_on, symmetric);
                    *new_bounds = Aabb { min: lo, max: hi };
                }
            }
            DragOp::GizmoScale { part, start, scale } => {
                let (o, dir) = view.ray(pos);
                let c = pivot;
                let factor = match *part {
                    GizmoPart::Axis(k) => closest_param_on_line(o, dir, c, axes[k % 3]).map(|t| {
                        let f = t / *start;
                        let e = extent[k % 3];
                        let g = frame.step_along(k % 3);
                        if snap_on && e > 1e-6 { snap(e * f, g).max(g.min(e)) / e } else { f }
                    }),
                    GizmoPart::Plane(n) => ray_plane(o, dir, c, axes[n % 3]).map(|h| {
                        let f = (h - c).length() / *start;
                        if snap_on { (f * 10.0).round() / 10.0 } else { f }
                    }),
                    GizmoPart::Center => view.world_to_screen(c).map(|cs| {
                        let f = f64::from(cs.distance(pos)) / *start;
                        if snap_on { (f * 10.0).round() / 10.0 } else { f }
                    }),
                    GizmoPart::Ring(_) => None,
                };
                if let Some(f) = factor.filter(|f| f.is_finite()) {
                    let f = f.clamp(SCALE_RANGE.0, SCALE_RANGE.1);
                    *scale = match *part {
                        GizmoPart::Axis(k) => lock_scale(crate::modal::Lock::Axis(k % 3), f),
                        GizmoPart::Plane(n) => lock_scale(crate::modal::Lock::Plane(n % 3), f),
                        _ => DVec3::splat(f),
                    };
                }
            }
            DragOp::Modal(m) => {
                let snapping = Self::snap_on(state, mods).then_some(MoveSnap { reference: snap_ref, grid, frame });
                match m.kind {
                    ModalKind::Move => m.delta = modal_move(view, pivot, m.axes, m.lock, m.start, pos, snapping),
                    ModalKind::Rotate => {
                        m.axis = rotation_axis(view, pivot, m.axes, m.lock);
                        m.angle = modal_angle(view, pivot, m.axis, m.start, pos, step, free);
                    }
                    ModalKind::Scale => {
                        let f = modal_factor(view, pivot, m.start, pos, snapping.is_some());
                        m.scale = lock_scale(m.lock, f);
                    }
                }
            }
        }
    }

    fn publish_drag(&self, state: &mut AppState) {
        let live = self.drag.as_ref().filter(|d| d.transforms() && !d.elements && !d.is_clone());
        let Some(d) = live else {
            state.runtime.selection.drag = None;
            return;
        };
        let matrix = d.matrix();
        let slot = &mut state.runtime.selection.drag;
        match slot {
            Some(t) if t.objs.len() == d.objs.len() && t.objs.first() == d.objs.first() => t.matrix = matrix,
            _ => *slot = Some(hp_app::runtime::DragTransform { objs: d.objs.clone().into(), matrix }),
        }
    }

    fn finish_drag(&mut self, state: &mut AppState) {
        state.runtime.selection.drag = None;
        let Some(d) = self.drag.take() else { return };
        if d.is_modal() {
            state.runtime.selection.modal_transform = false;
        }
        if d.edits() && d.revision != state.doc.revision() {
            return;
        }
        match &d.op {
            DragOp::Band { start, current } => {
                let Some(axis) = view_axis(d.kind) else { return };
                let inside = self.box_inside ^ d.mods.alt;
                let mode = if self.centers_only {
                    RectMode::Center
                } else if inside {
                    RectMode::Inside
                } else {
                    RectMode::Touching
                };
                let found = state.doc.objects_in_rect_2d(axis, *start, *current, mode, &|e| state.entity_bounds(e));
                let objs = if inside && !self.centers_only {
                    units_fully_inside(state, &found, state.select_mode())
                } else {
                    state.doc.resolve_many(&found, state.select_mode())
                };
                if ctrl(d.mods) {
                    state.doc.select(&objs, SelectOp::Add);
                } else {
                    state.doc.select(&objs, SelectOp::Replace);
                    self.handle_mode = HandleMode::Scale;
                }
                return;
            }
            DragOp::Paint { .. } => return,
            DragOp::Helper { entity, handle, pos } => {
                if (*pos - handle.pos).length_squared() > 1e-12
                    && let Some(value) =
                        state.doc.scene().entity(*entity).and_then(|n| value_with_point(&n.entity, handle, *pos))
                {
                    let r = state.doc.set_entity_key(&[*entity], &handle.key, &value).map(|_| ());
                    Self::report(state, r);
                }
                return;
            }
            DragOp::Pivot { pos } => {
                state.pivot = Some(*pos);
                self.pivot_sig = Some(self.selection_sig(state));
                return;
            }
            _ => {}
        }
        if d.is_identity() {
            return;
        }
        let m = d.matrix();
        let pivot = self.active_pivot(state);
        let clone = d.is_clone();
        let r = if clone {
            self.clone_transform(state, &d, m)
        } else {
            match &d.op {
                DragOp::Move { delta, .. } | DragOp::GizmoMove { delta, .. } if d.elements => {
                    element::translate_picked(state, *delta).map(|_| ())
                }
                DragOp::Modal(md) if d.elements && md.kind == ModalKind::Move => {
                    element::translate_picked(state, md.delta).map(|_| ())
                }
                DragOp::Move { delta, .. } | DragOp::GizmoMove { delta, .. } => state.doc.translate(&d.objs, *delta),
                DragOp::Modal(md) if md.kind == ModalKind::Move => state.doc.translate(&d.objs, md.delta),
                DragOp::Scale { new_bounds, .. } => state.doc.scale_bounds(&d.objs, &d.bounds, new_bounds),
                DragOp::FaceStretch { new_bounds, .. } => {
                    state.doc.transaction(STRETCH_LABEL, |doc| doc.scale_bounds(&d.objs, &d.bounds, new_bounds))
                }
                DragOp::Rotate { angle, .. } => state.doc.rotate_axis(&d.objs, d.pivot, d.kind.toward_viewer(), *angle),
                DragOp::GizmoRotate { axis, angle, .. } => {
                    state.doc.rotate_axis(&d.objs, d.pivot, d.axes[*axis % 3], *angle)
                }
                DragOp::Modal(md) if md.kind == ModalKind::Rotate => {
                    state.doc.rotate_axis(&d.objs, d.pivot, md.axis, md.angle)
                }
                _ => state.doc.transform(&d.objs, m),
            }
        };
        let ok = r.is_ok();
        Self::report(state, r);
        if ok && let Some(p) = pivot {
            let moved = m.transform_point3(p);
            state.pivot = moved.is_finite().then_some(moved);
            self.pivot_sig = state.pivot.map(|_| self.selection_sig(state));
        }
    }

    fn clone_transform(&self, state: &mut AppState, d: &Drag, m: DMat4) -> Result<(), hp_editor::EditError> {
        let keep = state.settings.selection.clone_keeps_visgroups;
        let anchor = match d.op {
            DragOp::Rotate { .. } | DragOp::GizmoRotate { .. } => d.pivot,
            _ => d.bounds.center(),
        };
        let link = extra::path_link(state, &d.objs);
        let fgd = state.assets.fgd().filter(|f| !f.is_empty());
        let targets =
            |class: &str, key: &str| fgd.and_then(|f| hp_app::runtime::helpers::fgd_target_key(f, class, key));
        state.doc.transaction("Duplicate", |doc| {
            let new = doc.duplicate_transformed_with(&d.objs, m, anchor, keep, Some(&targets))?;
            if let (Some((src, key)), [ObjRef::Entity(copy)]) = (link, new.as_slice()) {
                doc.link_cloned_path_node(src, *copy, key)?;
            }
            Ok(())
        })
    }

    fn click_2d(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        self.mode_before_click = None;
        let Some(axis) = view_axis(view.kind) else { return };
        let raw = self.picks_2d(state, view, pos, axis);
        let mode = state.select_mode();
        let mut cands: Vec<Vec<ObjRef>> = Vec::new();
        for o in raw {
            let r = state.doc.resolve(o, mode);
            if !r.is_empty() && !cands.contains(&r) {
                cands.push(r);
                if cands.len() >= MAX_CYCLE {
                    break;
                }
            }
        }
        let ctrl = ctrl(mods);
        if cands.is_empty() {
            self.cycle = None;
            if ctrl {
                return;
            }
            let size = Self::handle_size(state);
            let inside = self.selection_bounds(state).is_some_and(|b| screen_rect(view, &b).contains(pos));
            let on_handle = self
                .selection_bounds(state)
                .is_some_and(|b| hit_handle_sized(screen_rect(view, &b), self.handle_mode, pos, size).is_some());
            if inside {
                self.mode_before_click = Some(self.handle_mode);
                self.handle_mode = self.handle_mode.next();
            } else if !on_handle && !self.pivot_hit(state, view, pos) {
                state.doc.clear_selection();
            }
            return;
        }
        let cycling = cycle_continues(self.cycle.as_ref(), state, view, pos, &cands);
        let index = match (&self.cycle, cycling) {
            (Some(c), true) => (c.index + 1) % cands.len(),
            _ => 0,
        };
        let target = cands[index].clone();
        if ctrl {
            toggle_objects(state, &target);
        } else if !cycling && exactly_selected(state, &target) {
            self.mode_before_click = Some(self.handle_mode);
            self.handle_mode = self.handle_mode.next();
        } else {
            state.doc.select(&target, SelectOp::Replace);
            self.handle_mode = HandleMode::Scale;
        }
        self.cycle = Some(ClickCycle { view: view.index, pos, candidates: cands, index, time: state.frame.time });
    }

    fn click_3d(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        let ctrl = ctrl(mods);
        self.pick_click = None;
        let same_spot =
            self.cycle.as_ref().is_some_and(|c| c.view == view.index && (c.pos - pos).length() <= CYCLE_SLOP);
        if !ctrl && !same_spot && self.on_3d_handle(state, view, pos) {
            return;
        }
        self.hover.key = None;
        if element::click(state, view, pos, mods) {
            self.sel_cache.set(None);
            return;
        }
        match click_select_3d(state, view, pos, mods, &mut self.cycle) {
            Pick3d::Nothing => {}
            Pick3d::Toggled => self.pick_click = Some(PickClick { view: view.index, pos }),
            Pick3d::Replaced => {
                self.pick_click = Some(PickClick { view: view.index, pos });
                self.handle_mode = HandleMode::Scale;
            }
        }
    }

    pub(crate) fn middle_click(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2) {
        if self.drag.is_some() {
            return;
        }
        let same_spot =
            self.cycle.as_ref().is_some_and(|c| c.view == view.index && (c.pos - pos).length() <= DOUBLE_CLICK_SLOP);
        if same_spot && self.step_cycle(state, 1, true) {
            return;
        }
        if view.is_2d() {
            self.click_2d(state, view, pos, Modifiers::NONE);
        } else {
            self.click_3d(state, view, pos, Modifiers::NONE);
        }
    }

    fn step_cycle(&mut self, state: &mut AppState, dir: i32, any_age: bool) -> bool {
        let now = state.frame.time;
        let Some(c) = self.cycle.as_mut() else { return false };
        if c.candidates.len() < 2 || (!any_age && now - c.time > CYCLE_SECONDS) {
            return false;
        }
        let n = c.candidates.len();
        c.index = if dir >= 0 { (c.index + 1) % n } else { (c.index + n - 1) % n };
        c.time = now;
        let target = c.candidates[c.index].clone();
        state.doc.select(&target, SelectOp::Replace);
        self.handle_mode = HandleMode::Scale;
        true
    }

    fn live_cycle(&self, state: &AppState) -> Option<&ClickCycle> {
        self.cycle.as_ref().filter(|c| c.candidates.len() > 1 && state.frame.time - c.time <= CYCLE_SECONDS)
    }

    fn double_click(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2) {
        if let Some(m) = self.mode_before_click.take() {
            self.handle_mode = m;
        }
        let mode = state.select_mode();
        let at_cycle = |c: &Option<ClickCycle>| match c {
            Some(c) if c.view == view.index && (c.pos - pos).length() <= CYCLE_SLOP => {
                c.candidates.get(c.index).cloned()
            }
            _ => None,
        };
        let target: Vec<ObjRef> = if view.is_2d() {
            let Some(axis) = view_axis(view.kind) else { return };
            match at_cycle(&self.cycle) {
                Some(t) => t,
                None => {
                    let raw = self.picks_2d(state, view, pos, axis);
                    raw.first().map(|o| state.doc.resolve(*o, mode)).unwrap_or_default()
                }
            }
        } else {
            let picked_first = self
                .pick_click
                .take()
                .is_some_and(|c| c.view == view.index && (c.pos - pos).length() <= DOUBLE_CLICK_SLOP);
            if !picked_first && self.on_3d_handle(state, view, pos) {
                return;
            }
            match at_cycle(&self.cycle) {
                Some(t) => t,
                None => {
                    let (o, d) = view.ray(pos);
                    state
                        .view_pick(&Ray::new(o, d), &PickOptions::default())
                        .map(|h| state.doc.resolve(h.obj, mode))
                        .unwrap_or_default()
                }
            }
        };
        if target.is_empty() {
            return;
        }
        if !exactly_selected(state, &target) {
            state.doc.select(&target, SelectOp::Replace);
        }
        let entity = target.iter().any(|o| match o {
            ObjRef::Entity(_) => true,
            ObjRef::Solid(id) => state.doc.scene().object_of_solid(*id).is_entity(),
        });
        if entity {
            state.request(hp_app::Action::Properties);
        }
    }

    pub fn run_query(&mut self, state: &mut AppState, add: bool) -> usize {
        let pattern = self.query.trim().to_owned();
        if pattern.is_empty() {
            return 0;
        }
        let before = state.doc.selected_objects();
        let op = if add { SelectOp::Add } else { SelectOp::Replace };
        let kind = self.query_kind;
        let run = |p: &str, state: &mut AppState| match kind {
            QueryKind::Class => state.doc.select_by_class(p, op),
            QueryKind::Material => state.doc.select_by_material(p, op),
        };
        let mut n = run(&pattern, state);
        if n == 0 && !pattern.contains(['*', '?']) {
            n = run(&format!("*{pattern}*"), state);
        }
        if n == 0 {
            state.doc.select(&before, SelectOp::Replace);
        } else if !add {
            self.handle_mode = HandleMode::Scale;
        }
        self.query_result = Some((n, state.frame.time));
        n
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pick3d {
    Nothing,
    Replaced,
    Toggled,
}

pub(crate) fn click_select_3d(
    state: &mut AppState,
    view: &ViewInfo,
    pos: Pos2,
    mods: Modifiers,
    cycle: &mut Option<ClickCycle>,
) -> Pick3d {
    let ctrl = ctrl(mods);
    let cands = candidates_3d(state, view, pos);
    if cands.is_empty() {
        *cycle = None;
        if !ctrl {
            state.doc.clear_selection();
        }
        return Pick3d::Nothing;
    }
    if ctrl {
        *cycle = None;
        toggle_objects(state, &cands[0]);
        return Pick3d::Toggled;
    }
    let cycling = cycle_continues(cycle.as_ref(), state, view, pos, &cands);
    let index = match (cycle.as_ref(), cycling) {
        (Some(c), true) => (c.index + 1) % cands.len(),
        _ => 0,
    };
    let target = cands[index].clone();
    *cycle = Some(ClickCycle { view: view.index, pos, candidates: cands, index, time: state.frame.time });
    state.doc.select(&target, SelectOp::Replace);
    Pick3d::Replaced
}

fn candidates_3d(state: &AppState, view: &ViewInfo, pos: Pos2) -> Vec<Vec<ObjRef>> {
    let (o, d) = view.ray(pos);
    let hits = state.view_pick_all(&Ray::new(o, d), &PickOptions::default());
    let mode = state.select_mode();
    let mut cands: Vec<Vec<ObjRef>> = Vec::new();
    for h in hits {
        let r = state.doc.resolve(h.obj, mode);
        if !r.is_empty() && !cands.contains(&r) {
            cands.push(r);
            if cands.len() >= MAX_CYCLE {
                break;
            }
        }
    }
    cands
}

fn signed(v: f64) -> String {
    if v > 0.0 { format!("+{}", fmt_len(v)) } else { fmt_len(v) }
}

fn screen_rect(view: &ViewInfo, b: &Aabb) -> Rect {
    match (view.world_to_screen(b.min), view.world_to_screen(b.max)) {
        (Some(a), Some(c)) if !a.any_nan() && !c.any_nan() => Rect::from_two_pos(a, c),
        _ => Rect::NOTHING,
    }
}

fn single_point_entity_origin(state: &AppState, objs: &[ObjRef]) -> Option<DVec3> {
    let [ObjRef::Entity(id)] = objs else { return None };
    let scene = state.doc.scene();
    if scene.is_brush_entity(*id) {
        return None;
    }
    scene.entity(*id).and_then(|n| n.entity.origin()).filter(|o| o.is_finite())
}

fn snap_reference(state: &AppState, objs: &[ObjRef], bounds: &Aabb) -> DVec3 {
    let scene = state.doc.scene();
    let solids = scene.expand_solids(objs);
    if !solids.is_empty() {
        let mut b = Aabb::EMPTY;
        for id in solids {
            if let Some(sb) = scene.solid_bounds(id) {
                b = b.union(&sb);
            }
        }
        if !b.is_empty() && b.min.is_finite() {
            return b.min;
        }
    }
    single_point_entity_origin(state, objs).unwrap_or(bounds.min)
}

fn cycle_continues(
    cycle: Option<&ClickCycle>,
    state: &AppState,
    view: &ViewInfo,
    pos: Pos2,
    cands: &[Vec<ObjRef>],
) -> bool {
    let Some(c) = cycle else { return false };
    if cands.len() < 2 || c.view != view.index || (c.pos - pos).length() > CYCLE_SLOP || c.candidates != cands {
        return false;
    }
    if !(0.0..=CYCLE_SECONDS).contains(&(state.frame.time - c.time)) {
        return false;
    }
    let sel = &state.doc.selection().objects;
    c.candidates.get(c.index).is_some_and(|t| t.len() == sel.len() && t.iter().all(|o| sel.contains(o)))
}

fn exactly_selected(state: &AppState, target: &[ObjRef]) -> bool {
    let sel = state.doc.selection();
    !target.is_empty() && target.iter().all(|o| sel.contains(*o))
}

fn toggle_objects(state: &mut AppState, target: &[ObjRef]) {
    if target.is_empty() {
        return;
    }
    let scene = state.doc.scene();
    let owners: HashSet<ObjRef> = target
        .iter()
        .filter_map(|t| match *t {
            ObjRef::Solid(id) => Some(scene.object_of_solid(id)).filter(|o| o.is_entity()),
            ObjRef::Entity(_) => None,
        })
        .collect();
    let mut list: Vec<ObjRef> = Vec::with_capacity(state.doc.selection().objects.len() + target.len());
    for o in &state.doc.selection().objects {
        match *o {
            ObjRef::Entity(e) if owners.contains(o) => {
                list.extend(scene.entity_solid_ids(e).into_iter().map(ObjRef::Solid))
            }
            _ => list.push(*o),
        }
    }
    let present: HashSet<ObjRef> = list.iter().copied().collect();
    if target.iter().all(|t| present.contains(t)) {
        let remove: HashSet<ObjRef> = target.iter().copied().collect();
        list.retain(|o| !remove.contains(o));
    } else {
        list.extend(target.iter().copied().filter(|t| !present.contains(t)));
    }
    state.doc.select(&list, SelectOp::Replace);
}

fn units_fully_inside(state: &AppState, found: &[ObjRef], mode: hp_editor::SelectMode) -> Vec<ObjRef> {
    let doc = &state.doc;
    let scene = doc.scene();
    let found_set: HashSet<ObjRef> = found.iter().copied().collect();
    let member_inside = |o: ObjRef| match o {
        ObjRef::Solid(id) => found_set.contains(&o) || !doc.is_solid_visible(id),
        ObjRef::Entity(e) => {
            let solids = scene.entity_solid_ids(e);
            if solids.is_empty() {
                found_set.contains(&o) || !doc.is_entity_visible(e)
            } else {
                solids.iter().all(|s| found_set.contains(&ObjRef::Solid(*s)) || !doc.is_solid_visible(*s))
            }
        }
    };
    let mut covered: HashSet<ObjRef> = HashSet::new();
    let mut seen: HashSet<ObjRef> = HashSet::new();
    let mut out = Vec::new();
    for o in found {
        if covered.contains(o) {
            continue;
        }
        let unit = doc.resolve(*o, mode);
        for u in &unit {
            covered.insert(*u);
            if let ObjRef::Entity(e) = *u {
                covered.extend(scene.entity_solid_ids(e).into_iter().map(ObjRef::Solid));
            }
        }
        if !unit.is_empty() && unit.iter().all(|u| member_inside(*u)) {
            out.extend(unit.into_iter().filter(|u| seen.insert(*u)));
        }
    }
    out
}

impl Tool for SelectTool {
    fn id(&self) -> ToolId {
        ToolId::Select
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.drag = None;
        self.hover = Hover::default();
        self.cycle = None;
        self.pick_click = None;
        state.runtime.selection.modal_transform = false;
        state.runtime.selection.drag = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        let view = input.view;
        self.note_view(view);
        self.sync_pivot(state);
        sync_options(state);
        let mut flow = ToolFlow::Ignored;
        if !input.navigating
            && input.response.middle_clicked()
            && let Some(p) = input.pointer.or_else(|| input.response.hover_pos())
        {
            self.middle_click(state, view, p);
            flow = ToolFlow::Handled;
        }
        for ev in crate::input::events(input) {
            if self.handle_event(state, view, ev, input.modifiers) == ToolFlow::Handled {
                flow = ToolFlow::Handled;
            }
        }
        if self.drag.as_ref().is_some_and(|d| d.view == view.index && !d.is_modal())
            && !crate::input::primary_down(input)
        {
            self.finish_drag(state);
            flow = ToolFlow::Handled;
        }
        self.publish_drag(state);
        flow
    }

    fn click_while_flying(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) -> ToolFlow {
        if view.is_2d() || self.drag.is_some() {
            return ToolFlow::Ignored;
        }
        self.note_view(view);
        self.hover.key = None;
        self.pick_click = None;
        if element::click(state, view, pos, mods) {
            self.sel_cache.set(None);
            return ToolFlow::Handled;
        }
        if click_select_3d(state, view, pos, mods, &mut self.cycle) == Pick3d::Replaced {
            self.handle_mode = HandleMode::Scale;
        }
        ToolFlow::Handled
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        let used = self.keys(state, ctx);
        self.publish_drag(state);
        used
    }

    fn wheel(&mut self, state: &mut AppState, view: &ViewInfo, notches: f32, modifiers: Modifiers) -> ToolFlow {
        self.wheel_scale(state, view, notches, modifiers)
    }

    fn context_entries(&self, state: &AppState) -> Vec<ToolMenuEntry> {
        extra::context_entries(state)
    }

    fn context_command(&mut self, state: &mut AppState, id: u32) {
        extra::context_command(state, id);
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if let Some(d) = &self.drag {
            if d.transforms() {
                let color =
                    if d.is_clone() { rgba(hp_app::theme::VIEW_SUCCESS, 1.0) } else { rgba(draw::PREVIEW_COLOR, 1.0) };
                d.preview.push(out, &d.matrix(), color);
                if d.elements {
                    let m = d.matrix();
                    let f = |p: DVec3| m.transform_point3(p).as_vec3().to_array();
                    for [a, b] in &d.ghost {
                        out.lines_on_top.push(hp_render::OverlayLine { a: f(*a), b: f(*b), color });
                    }
                    for p in &d.ghost_points {
                        out.points.push(hp_render::OverlayPoint { pos: f(*p), color, size_px: 11.0 });
                    }
                }
            }
            if !matches!(d.op, DragOp::Paint { .. }) {
                return;
            }
        }
        let hovered = self.hover.objects_for(state.select_mode());
        if state.settings.view.hover_outline
            && !view.is_2d()
            && self.hover.view == Some(view.index)
            && !hovered.is_empty()
        {
            push_outline(
                out,
                &state.doc,
                hovered,
                &|e| state.entity_bounds(e),
                rgba(egui::Color32::WHITE, 0.5),
                MAX_PREVIEW_EDGES,
            );
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        if view.is_2d() {
            self.paint_2d(state, view, painter);
        } else {
            self.paint_3d(state, view, painter);
        }
        self.paint_pivot(state, view, painter);
        self.paint_helpers(state, view, painter);
    }

    fn options_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.options(ui, state);
        sync_options(state);
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        self.set_named_option(state, key, value)
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        self.named_option_values()
    }

    fn cursor(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if let Some(d) = &self.drag {
            return Some(match &d.op {
                DragOp::Band { .. } | DragOp::Paint { .. } => CursorIcon::Crosshair,
                DragOp::Pivot { .. } | DragOp::Helper { .. } => CursorIcon::Grabbing,
                DragOp::Move { .. } | DragOp::GizmoMove { .. } => {
                    if d.is_clone() {
                        CursorIcon::Copy
                    } else {
                        CursorIcon::Grabbing
                    }
                }
                DragOp::Scale { handle, .. } => handle.cursor(HandleMode::Scale),
                DragOp::Skew { handle, .. } => handle.cursor(HandleMode::Skew),
                DragOp::FaceStretch { dir, .. } => {
                    if d.is_clone() {
                        CursorIcon::Copy
                    } else {
                        crate::stretch::resize_cursor(*dir)
                    }
                }
                DragOp::Rotate { .. } | DragOp::GizmoRotate { .. } | DragOp::GizmoScale { .. } => {
                    if d.is_clone() {
                        CursorIcon::Copy
                    } else {
                        CursorIcon::Grabbing
                    }
                }
                DragOp::Modal(_) => CursorIcon::Move,
            });
        }
        if self.helper_hit(state, view, pos).is_some() {
            return Some(CursorIcon::Grab);
        }
        if !view.is_2d()
            && !self.pivot_first_3d(state, view, pos)
            && let Some(id) = self.stretch_hit(state, view, pos)
            && let Some(h) = self.stretch_handles(state, view).into_iter().find(|h| h.id() == id)
        {
            return Some(h.cursor());
        }
        if view.is_2d() {
            if self.pivot_hit(state, view, pos) {
                return Some(CursorIcon::Grab);
            }
            if let Some(b) = self.selection_bounds(state) {
                let rect = screen_rect(view, &b);
                if let Some(h) = hit_handle_sized(rect, self.handle_mode, pos, Self::handle_size(state)) {
                    return Some(h.cursor(self.handle_mode));
                }
                if rect.contains(pos) {
                    return Some(CursorIcon::Move);
                }
            }
            return Some(CursorIcon::Crosshair);
        }
        if self.gizmo_hit(state, view, pos).is_some() || self.pivot_hit(state, view, pos) {
            return Some(CursorIcon::Grab);
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        self.hint(state)
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        state.runtime.selection.drag = None;
        if let Some(d) = self.drag.take() {
            if d.is_modal() {
                state.runtime.selection.modal_transform = false;
            }
            return true;
        }
        if !state.doc.selection().is_empty() {
            state.doc.clear_selection();
            self.cycle = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
impl SelectTool {
    pub(crate) fn drag_matrix(&self) -> Option<DMat4> {
        self.drag.as_ref().map(Drag::matrix)
    }

    pub(crate) fn hovered_objects(&self) -> &[ObjRef] {
        &self.hover.objects
    }

    pub(crate) fn hovered_gizmo(&self) -> Option<GizmoPart> {
        self.hover.gizmo
    }

    pub(crate) fn set_query(&mut self, kind: QueryKind, text: &str) {
        self.query_kind = kind;
        self.query = text.to_owned();
    }

    pub(crate) fn cycle_len(&self) -> usize {
        self.cycle.as_ref().map_or(0, |c| c.candidates.len())
    }

    pub(crate) fn is_modal(&self) -> bool {
        self.drag.as_ref().is_some_and(Drag::is_modal)
    }

    pub(crate) fn modal(&self) -> Option<Modal> {
        match self.drag.as_ref().map(|d| &d.op) {
            Some(DragOp::Modal(m)) => Some(*m),
            _ => None,
        }
    }

    pub(crate) fn begin_modal(&mut self, state: &mut AppState, view: &ViewInfo, kind: ModalKind, pos: Pos2) {
        self.note_view(view);
        self.start_modal(state, view, kind, pos);
    }
}
