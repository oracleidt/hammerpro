use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use egui::{Align2, CursorIcon, Key, Pos2, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::key_label;
use hp_app::state::AppState;
use hp_app::tool::{ToolMenuEntry, ToolOption};
use hp_app::{Action, Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::{Id, ObjRef};
use hp_render::Overlay;

use super::apply::{self, model_bounds};
use super::build::{Bounds, EntityRole, Plan, model_names};
use super::curve::{Controls, Path};
use super::model::{
    CableStyle, FenceStyle, Interp, MAX_ROLL, MAX_WIDTH, MIN_WIDTH, Mode, ModelAxis, OPTIONS, Profile, Sides,
    SplineMeta, SplinePoint, parse_flag,
};
use super::store;
use crate::block::new_depth_range;
use crate::common::{
    COLOR_GHOST, COLOR_PENDING, COLOR_REMOVED, Mouse, Ov, Snap, active_material, consume_plain_key, depth_axis,
    fmt_len, fmt_num, fresh, paint_label, pick_surface_with, rgba,
};

const HANDLE_PX: f32 = 8.0;
const CURVE_PX: f32 = 8.0;
const REGEN_DELAY: f64 = 0.25;
const MENU_INSERT: u32 = 1;
const MENU_DELETE: u32 = 2;
const MENU_CLOSE: u32 = 3;
const MENU_GENERATE: u32 = 4;
const MENU_BAKE: u32 = 5;
const MENU_NEW: u32 = 6;
const MENU_AUTO_HANDLES: u32 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Point,
    In,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    index: usize,
    part: Part,
    moved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hover {
    add: Option<DVec3>,
    near: Option<(usize, Part)>,
    curve: Option<(usize, DVec3)>,
    kind: ViewKind,
    frame: u64,
}

type Cached = (String, u64, usize, Rc<Result<Plan, String>>);

pub struct SplineTool {
    pub meta: SplineMeta,
    pub editing: Option<Id>,
    applied: Option<SplineMeta>,
    pub selected: Option<usize>,
    drag: Option<Drag>,
    hover: Option<Hover>,
    bounds: HashMap<String, Bounds>,
    pending: Vec<String>,
    cache: RefCell<Option<Cached>>,
    dirty_at: Option<f64>,
    seen_revision: u64,
    pub auto_regenerate: bool,
    pub keep_active: bool,
    initialized: bool,
    wheel: f32,
    menu_curve: Option<(usize, DVec3)>,
}

impl Default for SplineTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SplineTool {
    pub fn new() -> Self {
        Self {
            meta: SplineMeta::default(),
            editing: None,
            applied: None,
            selected: None,
            drag: None,
            hover: None,
            bounds: HashMap::new(),
            pending: Vec::new(),
            cache: RefCell::new(None),
            dirty_at: None,
            seen_revision: 0,
            auto_regenerate: true,
            keep_active: true,
            initialized: false,
            wheel: 0.0,
            menu_curve: None,
        }
    }

    fn ensure_init(&mut self, state: &AppState) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let s = state.settings.edit.default_texture_scale;
        if s.is_finite() && s.abs() > 1e-6 {
            self.meta.tex_scale = s;
        }
        self.meta.material = active_material(state);
    }

    pub fn is_dirty(&self) -> bool {
        match &self.applied {
            Some(a) => *a != self.meta,
            None => !self.meta.points.is_empty(),
        }
    }

    fn touch(&mut self, state: &AppState) {
        self.dirty_at = Some(state.frame.time);
    }

    pub fn load(&mut self, state: &mut AppState, anchor: Id) -> bool {
        let Some(entry) = store::read(&state.doc, anchor) else { return false };
        self.meta = entry.meta.clone();
        self.applied = Some(entry.meta);
        self.editing = Some(anchor);
        self.selected = None;
        self.drag = None;
        self.dirty_at = None;
        self.seen_revision = state.doc.revision();
        true
    }

    pub fn load_selection(&mut self, state: &mut AppState) -> bool {
        match store::selected_anchor(&state.doc) {
            Some(a) if Some(a) != self.editing || !self.is_dirty() => self.load(state, a),
            _ => false,
        }
    }

    pub fn start_new(&mut self) {
        self.meta.points.clear();
        self.editing = None;
        self.applied = None;
        self.selected = None;
        self.drag = None;
        self.dirty_at = None;
    }

    fn outputs(&self, state: &AppState) -> HashSet<ObjRef> {
        let Some(a) = self.editing else { return HashSet::new() };
        let mut out: HashSet<ObjRef> = HashSet::new();
        out.insert(ObjRef::Entity(a));
        if let Some(uid) = store::anchor_uid(&state.doc, a) {
            for o in store::outputs_of(&state.doc, a, &uid) {
                out.insert(o);
                if let ObjRef::Entity(e) = o {
                    out.extend(state.doc.scene().entity_solid_ids(e).into_iter().map(ObjRef::Solid));
                }
            }
        }
        out
    }

    pub fn preview(&self, state: &AppState) -> Rc<Result<Plan, String>> {
        let key = self.meta.encode();
        let rev = state.doc.revision();
        if let Some((k, r, n, plan)) = self.cache.borrow().as_ref()
            && *k == key
            && *r == rev
            && *n == self.bounds.len()
        {
            return plan.clone();
        }
        let exclude = apply::excluded_solids(&state.doc, self.editing);
        let made = apply::plan_in(&state.doc, &self.meta, self.bounds.clone(), &exclude).map_err(|e| e.to_string());
        let rc = Rc::new(made);
        *self.cache.borrow_mut() = Some((key, rev, self.bounds.len(), rc.clone()));
        rc
    }

    pub fn generate(&mut self, state: &mut AppState) -> bool {
        self.ensure_init(state);
        if state.doc.in_transaction() {
            return false;
        }
        match apply::generate(state, self.editing, &self.meta) {
            Ok(done) => {
                let entities = done.applied.outputs.len();
                state.log(
                    Severity::Info,
                    tf!(
                        "spline.generated",
                        self.meta.mode.name(),
                        done.applied.solids,
                        entities,
                        fmt_len(done.plan.length)
                    ),
                );
                if done.removed > 0 {
                    state.log(Severity::Info, tf!("spline.removed", done.removed));
                }
                let was_new = self.editing.is_none();
                self.editing = Some(done.applied.anchor);
                self.applied = Some(self.meta.clone());
                self.dirty_at = None;
                self.seen_revision = state.doc.revision();
                if was_new && !self.keep_active {
                    state.set_tool(ToolId::Select);
                }
                true
            }
            Err(e) => {
                state.notify(Severity::Warning, tf!("spline.failed", e));
                self.dirty_at = None;
                false
            }
        }
    }

    pub fn bake(&mut self, state: &mut AppState) -> bool {
        let Some(a) = self.editing else { return false };
        match store::bake(&mut state.doc, a, true) {
            Ok(objs) => {
                state.log(Severity::Info, tf!("spline.baked", objs.len()));
                self.start_new();
                true
            }
            Err(e) => {
                state.notify(Severity::Warning, tf!("spline.failed", e));
                false
            }
        }
    }

    pub fn insert_point(&mut self, index: usize, pos: DVec3) -> usize {
        let i = index.min(self.meta.points.len());
        let mut p = SplinePoint::new(pos);
        if let (Some(a), Some(b)) = (self.meta.points.get(i.wrapping_sub(1)), self.meta.points.get(i)) {
            p.roll = (a.roll + b.roll) * 0.5;
            p.width = (a.width + b.width) * 0.5;
        } else if let Some(a) = self.meta.points.get(i.wrapping_sub(1)).or_else(|| self.meta.points.get(i)) {
            p.roll = a.roll;
            p.width = a.width;
        }
        self.meta.points.insert(i, p);
        self.selected = Some(i);
        i
    }

    pub fn delete_point(&mut self, index: usize) -> bool {
        if index >= self.meta.points.len() {
            return false;
        }
        self.meta.points.remove(index);
        self.selected = if self.meta.points.is_empty() { None } else { Some(index.min(self.meta.points.len() - 1)) };
        true
    }

    fn add_index(&self) -> usize {
        let n = self.meta.points.len();
        match self.selected {
            Some(0) if n > 1 => 0,
            Some(i) if i + 1 < n => i + 1,
            _ => n,
        }
    }

    fn reference_point(&self) -> Option<DVec3> {
        self.selected.and_then(|i| self.meta.points.get(i)).or_else(|| self.meta.points.last()).map(|p| p.pos)
    }

    fn snap2(&self, state: &AppState, view: &ViewInfo, pos: Pos2, mods: egui::Modifiers) -> DVec2 {
        Snap::new(state, view, mods).v2(view.screen_to_2d(pos))
    }

    fn point_2d(&self, state: &AppState, view: &ViewInfo, pos: Pos2, depth_of: Option<DVec3>) -> Option<DVec3> {
        let kind = view.kind;
        if !kind.is_2d() {
            return None;
        }
        let d = depth_axis(kind);
        let p2 = self.snap2(state, view, pos, egui::Modifiers::NONE);
        let depth = match depth_of.or_else(|| self.reference_point()) {
            Some(r) => r[d],
            None => {
                let snap = Snap::new(state, view, egui::Modifiers::NONE);
                let (lo, _) = new_depth_range(state, state.runtime.brush.last_created, d);
                snap.v(lo, d)
            }
        };
        Some(crate::common::clean_vec(kind.from_2d(p2, depth)))
    }

    fn point_3d(&self, state: &AppState, view: &ViewInfo, pos: Pos2, keep: Option<DVec3>) -> Option<DVec3> {
        let outputs = self.outputs(state);
        let filter = |o: ObjRef| !outputs.contains(&o);
        if let Some(hit) = pick_surface_with(state, view, pos, Some(&filter)) {
            let mut p = hit.point;
            if state.snap_enabled() {
                let s = state.grid_frame(ViewKind::Top).snap_point(p);
                p.x = s.x;
                p.y = s.y;
            }
            return Some(crate::common::clean_vec(p));
        }
        let (o, d) = view.ray(pos);
        let z = keep.map_or(0.0, |k| k.z);
        if d.z.abs() < 1e-9 {
            return None;
        }
        let t = (z - o.z) / d.z;
        (t > 0.0 && t.is_finite() && t < 1e6).then(|| crate::common::clean_vec(o + d * t))
    }

    fn handle_world(&self, index: usize, part: Part) -> Option<DVec3> {
        let p = self.meta.points.get(index)?;
        let c = Controls { points: &self.meta.points, closed: self.meta.is_closed() };
        match part {
            Part::Point => Some(p.pos),
            Part::In => Some(p.pos + c.handle_in(index)),
            Part::Out => Some(p.pos + c.handle_out(index)),
        }
    }

    fn handles_shown(&self) -> Vec<(usize, Part)> {
        let mut out: Vec<(usize, Part)> = (0..self.meta.points.len()).map(|i| (i, Part::Point)).collect();
        if self.meta.interp == Interp::Bezier
            && let Some(i) = self.selected.filter(|i| *i < self.meta.points.len())
        {
            out.push((i, Part::In));
            out.push((i, Part::Out));
        }
        out
    }

    fn hit_handle(&self, view: &ViewInfo, pos: Pos2) -> Option<(usize, Part)> {
        let mut best: Option<((usize, Part), f32)> = None;
        for (i, part) in self.handles_shown().into_iter().rev() {
            let Some(w) = self.handle_world(i, part) else { continue };
            let Some(s) = view.world_to_screen(w) else { continue };
            let d = s.distance(pos);
            if d <= HANDLE_PX && best.is_none_or(|b| d < b.1) {
                best = Some(((i, part), d));
            }
        }
        best.map(|b| b.0)
    }

    fn hit_curve(&self, view: &ViewInfo, pos: Pos2) -> Option<(usize, DVec3)> {
        let path = Path::build(&self.meta, None).ok()?;
        let mut best: Option<(usize, DVec3, f32)> = None;
        for w in path.samples.windows(2) {
            let (Some(a), Some(b)) = (view.world_to_screen(w[0].pos), view.world_to_screen(w[1].pos)) else { continue };
            let ab = b - a;
            let len2 = ab.length_sq();
            let f = if len2 > 1e-6 { ((pos - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let d = (a + ab * f).distance(pos);
            if d <= CURVE_PX && best.is_none_or(|x| d < x.2) {
                best = Some((w[0].span, w[0].pos.lerp(w[1].pos, f64::from(f)), d));
            }
        }
        best.map(|b| (b.0 + 1, crate::common::clean_vec(b.1)))
    }

    fn move_part(&mut self, index: usize, part: Part, to: DVec3) {
        let c = Controls { points: &self.meta.points, closed: self.meta.is_closed() };
        let (hin, hout) = (c.handle_in(index), c.handle_out(index));
        let Some(p) = self.meta.points.get_mut(index) else { return };
        match part {
            Part::Point => p.pos = to,
            Part::Out => {
                p.handle_out = Some(to - p.pos);
                if p.handle_in.is_none() {
                    p.handle_in = Some(hin);
                }
            }
            Part::In => {
                p.handle_in = Some(to - p.pos);
                if p.handle_out.is_none() {
                    p.handle_out = Some(hout);
                }
            }
        }
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        self.ensure_init(state);
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        if m.hovering() {
            let near = self.hit_handle(view, pos);
            let curve = if near.is_none() && m.modifiers.ctrl { self.hit_curve(view, pos) } else { None };
            let add = if near.is_none() {
                if view.is_2d() { self.point_2d(state, view, pos, None) } else { self.point_3d(state, view, pos, None) }
            } else {
                None
            };
            self.hover = Some(Hover { add, near, curve, kind: view.kind, frame: state.frame.index });
            self.menu_curve = self.hit_curve(view, pos);
            return ToolFlow::Ignored;
        }
        if let Some(d) = self.drag {
            if m.dragging || m.drag_stopped {
                let current = self.handle_world(d.index, d.part);
                let to = if view.is_2d() {
                    self.point_2d(state, view, pos, current)
                } else if d.part == Part::Point {
                    self.point_3d(state, view, pos, current)
                } else {
                    None
                };
                if let Some(to) = to
                    && current.is_none_or(|c| c.distance_squared(to) > 1e-9)
                {
                    self.move_part(d.index, d.part, to);
                    if let Some(dr) = &mut self.drag {
                        dr.moved = true;
                    }
                }
                if m.drag_stopped {
                    if self.drag.is_some_and(|d| d.moved) {
                        self.touch(state);
                    }
                    self.drag = None;
                }
                return ToolFlow::Handled;
            }
            self.drag = None;
        }
        if m.drag_started {
            let origin = m.origin().unwrap_or(pos);
            if let Some((i, part)) = self.hit_handle(view, origin) {
                self.selected = Some(i);
                self.drag = Some(Drag { index: i, part, moved: false });
                return ToolFlow::Handled;
            }
        }
        if m.double_clicked {
            if !self.meta.points.is_empty() {
                self.generate(state);
            }
            return ToolFlow::Handled;
        }
        if m.clicked {
            if let Some((i, _)) = self.hit_handle(view, pos) {
                self.selected = Some(i);
                return ToolFlow::Handled;
            }
            if m.modifiers.ctrl {
                if let Some((index, p)) = self.hit_curve(view, pos) {
                    self.insert_point(index, p);
                    self.touch(state);
                }
                return ToolFlow::Handled;
            }
            let p = if view.is_2d() {
                self.point_2d(state, view, pos, None)
            } else {
                self.point_3d(state, view, pos, None)
            };
            if let Some(p) = p {
                if self.meta.points.len() >= super::model::MAX_POINTS {
                    state.notify(Severity::Warning, t!("spline.too_many"));
                    return ToolFlow::Handled;
                }
                if self.meta.points.iter().any(|q| q.pos.distance_squared(p) < 1e-6) {
                    return ToolFlow::Handled;
                }
                let at = self.add_index();
                self.insert_point(at, p);
                self.touch(state);
            }
            return ToolFlow::Handled;
        }
        if m.drag_started || m.dragging {
            return ToolFlow::Handled;
        }
        ToolFlow::Ignored
    }

    fn regenerate_if_due(&mut self, state: &mut AppState) {
        let Some(at) = self.dirty_at else { return };
        if self.drag.is_some() || state.frame.time - at < REGEN_DELAY {
            return;
        }
        if self.editing.is_none() || !self.auto_regenerate || !self.is_dirty() {
            self.dirty_at = None;
            return;
        }
        if self.meta.points.len() < 2 {
            self.dirty_at = None;
            return;
        }
        self.generate(state);
    }

    fn sync_with_document(&mut self, state: &mut AppState) {
        let rev = state.doc.revision();
        if rev == self.seen_revision {
            return;
        }
        self.seen_revision = rev;
        let Some(a) = self.editing else { return };
        if store::anchor_uid(&state.doc, a).is_none() {
            self.start_new();
            return;
        }
        if self.dirty_at.is_none()
            && self.drag.is_none()
            && let Some(entry) = store::read(&state.doc, a)
        {
            if self.applied.as_ref() == Some(&self.meta) {
                self.meta = entry.meta.clone();
            }
            self.applied = Some(entry.meta);
            if self.selected.is_some_and(|i| i >= self.meta.points.len()) {
                self.selected = None;
            }
        }
    }

    fn refresh_bounds(&mut self, state: &mut AppState) {
        let names = model_names(&self.meta);
        if names.iter().all(|n| self.bounds.contains_key(n)) {
            self.pending.clear();
            return;
        }
        let (known, pending) = model_bounds(state, &self.meta);
        self.bounds.extend(known);
        self.pending = pending;
    }

    pub fn set_point_option(&mut self, key: &str, v: &str) -> Option<bool> {
        let num = v.trim().parse::<f64>().ok().filter(|x| x.is_finite());
        if key == "selected_point" {
            let n = num.filter(|n| n.fract() == 0.0)?;
            if n == 0.0 {
                self.selected = None;
                return Some(true);
            }
            let i = n as usize;
            if i >= 1 && i <= self.meta.points.len() {
                self.selected = Some(i - 1);
                return Some(true);
            }
            return Some(false);
        }
        let point_key =
            matches!(key, "point_x" | "point_y" | "point_z" | "point_roll" | "point_width" | "point_auto_handles");
        if !point_key {
            return None;
        }
        let Some(p) = self.selected.and_then(|i| self.meta.points.get_mut(i)) else {
            return Some(v.trim().is_empty());
        };
        Some(match key {
            "point_x" => num.map(|x| p.pos.x = x).is_some(),
            "point_y" => num.map(|x| p.pos.y = x).is_some(),
            "point_z" => num.map(|x| p.pos.z = x).is_some(),
            "point_roll" => num.filter(|x| x.abs() <= MAX_ROLL).map(|x| p.roll = x).is_some(),
            "point_width" => num.filter(|x| (MIN_WIDTH..=MAX_WIDTH).contains(x)).map(|x| p.width = x).is_some(),
            _ => match parse_flag(v) {
                Some(true) => {
                    p.handle_in = None;
                    p.handle_out = None;
                    true
                }
                Some(false) => true,
                None => false,
            },
        })
    }

    fn mode_label(m: Mode) -> &'static str {
        match m {
            Mode::Path => t!("spline.mode_path"),
            Mode::Fence => t!("spline.mode_fence"),
            Mode::Cable => t!("spline.mode_cable"),
            Mode::Sweep => t!("spline.mode_sweep"),
            Mode::Props => t!("spline.mode_props"),
        }
    }

    fn profile_label(p: Profile) -> &'static str {
        match p {
            Profile::Pipe => t!("spline.profile_pipe"),
            Profile::Curb => t!("spline.profile_curb"),
            Profile::Trim => t!("spline.profile_trim"),
            Profile::RailTube => t!("spline.profile_rail_tube"),
            Profile::Custom => t!("spline.profile_custom"),
        }
    }

    fn mode_ui(&mut self, ui: &mut Ui) -> bool {
        let m = &mut self.meta;
        let before = m.clone();
        ui.horizontal_wrapped(|ui| match m.mode {
            Mode::Path => {
                ui.label(t!("spline.path_hint"));
            }
            Mode::Sweep => {
                egui::ComboBox::from_id_salt("spline_profile").selected_text(Self::profile_label(m.profile)).show_ui(
                    ui,
                    |ui| {
                        for p in Profile::ALL {
                            ui.selectable_value(&mut m.profile, *p, Self::profile_label(*p));
                        }
                    },
                );
                match m.profile {
                    Profile::Pipe | Profile::RailTube => {
                        ui.label(t!("spline.sides"));
                        ui.add(egui::DragValue::new(&mut m.sides).range(3..=32));
                        ui.label(t!("spline.radius"));
                        ui.add(egui::DragValue::new(&mut m.radius).speed(0.5).range(0.5..=2048.0));
                        if m.profile == Profile::RailTube {
                            ui.label(t!("spline.height"));
                            ui.add(egui::DragValue::new(&mut m.size_h).speed(0.5).range(0.5..=2048.0));
                        }
                    }
                    Profile::Custom => {
                        ui.label(t!("spline.custom")).on_hover_text(t!("spline.custom_tip"));
                        let mut text = super::model::format_custom(&m.custom);
                        if ui.add(egui::TextEdit::singleline(&mut text).desired_width(220.0)).changed()
                            && let Some(c) = super::model::parse_custom(&text)
                        {
                            m.custom = c;
                        }
                        ui.label(t!("spline.width"));
                        ui.add(egui::DragValue::new(&mut m.size_w).speed(0.5).range(0.5..=2048.0));
                    }
                    _ => {
                        ui.label(t!("spline.width"));
                        ui.add(egui::DragValue::new(&mut m.size_w).speed(0.5).range(0.5..=2048.0));
                        ui.label(t!("spline.height"));
                        ui.add(egui::DragValue::new(&mut m.size_h).speed(0.5).range(0.5..=2048.0));
                    }
                }
                ui.label(t!("spline.segment")).on_hover_text(t!("spline.segment_tip"));
                ui.add(egui::DragValue::new(&mut m.segment).speed(1.0).range(4.0..=2048.0));
                ui.checkbox(&mut m.detail, t!("spline.detail"));
            }
            Mode::Fence => {
                ui.selectable_value(&mut m.fence, FenceStyle::Model, t!("spline.fence_model"));
                ui.selectable_value(&mut m.fence, FenceStyle::Rails, t!("spline.fence_rails"));
                match m.fence {
                    FenceStyle::Model => {
                        model_field(ui, t!("spline.model"), &mut m.model);
                        ui.label(t!("spline.spacing")).on_hover_text(t!("spline.spacing_tip"));
                        ui.add(egui::DragValue::new(&mut m.spacing).speed(1.0).range(0.0..=8192.0));
                        ui.checkbox(&mut m.fit_spacing, t!("spline.fit_spacing"));
                        egui::ComboBox::from_id_salt("spline_axis").selected_text(m.model_axis.name()).show_ui(
                            ui,
                            |ui| {
                                for a in ModelAxis::ALL {
                                    ui.selectable_value(&mut m.model_axis, *a, a.name());
                                }
                            },
                        );
                        model_field(ui, t!("spline.post_model"), &mut m.post_model);
                        ui.checkbox(&mut m.posts_at_points, t!("spline.posts_at_points"));
                        ui.checkbox(&mut m.post_every_segment, t!("spline.post_every_segment"));
                        model_field(ui, t!("spline.start_cap"), &mut m.start_cap);
                        model_field(ui, t!("spline.end_cap"), &mut m.end_cap);
                        ui.checkbox(&mut m.align_slope, t!("spline.align_slope"));
                        class_field(ui, &mut m.prop_class);
                    }
                    FenceStyle::Rails => {
                        ui.label(t!("spline.rail_count"));
                        ui.add(egui::DragValue::new(&mut m.rail_count).range(1..=8));
                        ui.label(t!("spline.post_height"));
                        ui.add(egui::DragValue::new(&mut m.post_height).speed(1.0).range(1.0..=2048.0));
                        ui.label(t!("spline.post_size"));
                        ui.add(egui::DragValue::new(&mut m.post_size).speed(0.5).range(0.5..=2048.0));
                        ui.label(t!("spline.rail_size"));
                        ui.add(egui::DragValue::new(&mut m.rail_size).speed(0.5).range(0.5..=2048.0));
                        ui.label(t!("spline.post_spacing"));
                        ui.add(egui::DragValue::new(&mut m.post_spacing).speed(1.0).range(4.0..=8192.0));
                        ui.checkbox(&mut m.detail, t!("spline.detail"));
                    }
                }
            }
            Mode::Cable => {
                ui.selectable_value(&mut m.cable, CableStyle::Rope, t!("spline.cable_rope"));
                ui.selectable_value(&mut m.cable, CableStyle::Brush, t!("spline.cable_brush"));
                ui.label(t!("spline.slack")).on_hover_text(t!("spline.slack_tip"));
                ui.add(egui::DragValue::new(&mut m.slack).speed(1.0).range(0.0..=4096.0));
                ui.label(t!("spline.rope_every")).on_hover_text(t!("spline.rope_every_tip"));
                ui.add(egui::DragValue::new(&mut m.rope_every).speed(1.0).range(0.0..=8192.0));
                match m.cable {
                    CableStyle::Rope => {
                        ui.label(t!("spline.rope_width"));
                        ui.add(egui::DragValue::new(&mut m.rope_width).speed(0.1).range(0.1..=64.0));
                        ui.label(t!("spline.subdiv"));
                        ui.add(egui::DragValue::new(&mut m.subdiv).range(0..=8));
                        ui.label(t!("spline.rope_material"));
                        ui.add(egui::TextEdit::singleline(&mut m.rope_material).desired_width(140.0));
                    }
                    CableStyle::Brush => {
                        ui.label(t!("spline.sides"));
                        ui.add(egui::DragValue::new(&mut m.sides).range(3..=32));
                        ui.label(t!("spline.radius"));
                        ui.add(egui::DragValue::new(&mut m.radius).speed(0.5).range(0.5..=2048.0));
                        ui.label(t!("spline.segment"));
                        ui.add(egui::DragValue::new(&mut m.segment).speed(1.0).range(4.0..=2048.0));
                        ui.checkbox(&mut m.detail, t!("spline.detail"));
                    }
                }
            }
            Mode::Props => {
                ui.label(t!("spline.models")).on_hover_text(t!("spline.models_tip"));
                let mut text = m.models.join(", ");
                if ui.add(egui::TextEdit::singleline(&mut text).desired_width(260.0)).changed() {
                    let list = super::model::parse_list(&text);
                    if !list.is_empty() {
                        m.models = list;
                    }
                }
                ui.label(t!("spline.spacing")).on_hover_text(t!("spline.spacing_tip"));
                ui.add(egui::DragValue::new(&mut m.spacing).speed(1.0).range(0.0..=8192.0));
                egui::ComboBox::from_id_salt("spline_side").selected_text(side_label(m.sides_mode)).show_ui(ui, |ui| {
                    for s in Sides::ALL {
                        ui.selectable_value(&mut m.sides_mode, *s, side_label(*s));
                    }
                });
                ui.label(t!("spline.jitter"));
                ui.add(egui::DragValue::new(&mut m.jitter).speed(0.5).range(0.0..=4096.0));
                ui.label(t!("spline.yaw_jitter"));
                ui.add(egui::DragValue::new(&mut m.yaw_jitter).speed(1.0).range(0.0..=180.0));
                ui.label(t!("spline.scale"));
                ui.add(egui::DragValue::new(&mut m.scale_min).speed(0.01).range(0.05..=100.0));
                ui.add(egui::DragValue::new(&mut m.scale_max).speed(0.01).range(0.05..=100.0));
                ui.label(t!("spline.seed"));
                ui.add(egui::DragValue::new(&mut m.seed));
                ui.checkbox(&mut m.face_path, t!("spline.face_path"));
                ui.checkbox(&mut m.drop, t!("spline.drop")).on_hover_text(t!("spline.drop_tip"));
                ui.checkbox(&mut m.avoid_overlap, t!("spline.avoid_overlap")).on_hover_text(t!("spline.avoid_tip"));
                class_field(ui, &mut m.prop_class);
            }
        });
        if m.mode != Mode::Path {
            ui.horizontal_wrapped(|ui| {
                ui.label(t!("spline.offset")).on_hover_text(t!("spline.offset_tip"));
                ui.add(egui::DragValue::new(&mut m.offset_x).speed(1.0).range(-4096.0..=4096.0).prefix("x "));
                ui.add(egui::DragValue::new(&mut m.offset_z).speed(1.0).range(-4096.0..=4096.0).prefix("z "));
                if matches!(m.mode, Mode::Sweep | Mode::Cable)
                    || (m.mode == Mode::Fence && m.fence == FenceStyle::Rails)
                {
                    ui.label(t!("create.block.tex_scale"));
                    ui.add(egui::DragValue::new(&mut m.tex_scale).speed(0.01).range(0.01..=64.0).max_decimals(3));
                    ui.label(t!("spline.material"));
                    ui.add(egui::TextEdit::singleline(&mut m.material).desired_width(200.0));
                }
            });
        }
        m.scale_max = m.scale_max.max(m.scale_min);
        *m != before
    }

    fn point_ui(&mut self, ui: &mut Ui) -> bool {
        let Some(i) = self.selected.filter(|i| *i < self.meta.points.len()) else { return false };
        let bezier = self.meta.interp == Interp::Bezier;
        let p = &mut self.meta.points[i];
        let before = *p;
        let mut auto = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(tf!("spline.point_n", i + 1));
            ui.add(egui::DragValue::new(&mut p.pos.x).speed(1.0).prefix("x "));
            ui.add(egui::DragValue::new(&mut p.pos.y).speed(1.0).prefix("y "));
            ui.add(egui::DragValue::new(&mut p.pos.z).speed(1.0).prefix("z "));
            ui.label(t!("spline.roll")).on_hover_text(t!("spline.roll_tip"));
            ui.add(egui::DragValue::new(&mut p.roll).speed(1.0).range(-MAX_ROLL..=MAX_ROLL).suffix("°"));
            ui.label(t!("spline.point_width")).on_hover_text(t!("spline.point_width_tip"));
            ui.add(egui::DragValue::new(&mut p.width).speed(0.01).range(MIN_WIDTH..=MAX_WIDTH));
            if bezier && (p.handle_in.is_some() || p.handle_out.is_some()) {
                auto = ui.button(t!("spline.auto_handles")).clicked();
            }
        });
        if auto {
            p.handle_in = None;
            p.handle_out = None;
        }
        *p != before
    }

    fn draw_plan(&self, ov: &mut Ov<'_>, plan: &Plan, ok: bool) {
        let wire = rgba(if ok { COLOR_GHOST } else { COLOR_REMOVED }, 0.85);
        for s in &plan.solids {
            ov.polygon_outline(&s.rings[0], wire, true);
            ov.polygon_outline(&s.rings[1], wire, true);
            for (a, b) in s.rings[0].iter().zip(&s.rings[1]) {
                ov.xray_line(*a, *b, wire);
            }
        }
        for w in &plan.wires {
            for seg in w.windows(2) {
                ov.xray_line(seg[0], seg[1], wire);
            }
        }
        for e in &plan.entities {
            let color = match e.role {
                EntityRole::Post | EntityRole::Cap => rgba(COLOR_PENDING, 0.9),
                _ => wire,
            };
            match e.bounds {
                Some((lo, hi)) => {
                    let m = hp_geom::angle_matrix(e.angles);
                    let c: [DVec3; 8] = std::array::from_fn(|k| {
                        let l = DVec3::new(
                            if k & 1 == 0 { lo.x } else { hi.x },
                            if k & 2 == 0 { lo.y } else { hi.y },
                            if k & 4 == 0 { lo.z } else { hi.z },
                        );
                        e.origin + m * (l * e.scale)
                    });
                    ov.hull_edges(&c, color, true);
                }
                None => ov.point(e.origin, color, 6.0),
            }
        }
    }
}

fn side_label(s: Sides) -> &'static str {
    match s {
        Sides::Center => t!("spline.side_center"),
        Sides::Left => t!("spline.side_left"),
        Sides::Right => t!("spline.side_right"),
        Sides::Both => t!("spline.side_both"),
    }
}

fn model_field(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(200.0).hint_text("models/..."));
}

fn class_field(ui: &mut Ui, value: &mut String) {
    egui::ComboBox::from_id_salt("spline_class").selected_text(value.as_str()).show_ui(ui, |ui| {
        for c in ["prop_static", "prop_dynamic", "prop_physics"] {
            ui.selectable_value(value, c.to_owned(), c);
        }
    });
}

impl Tool for SplineTool {
    fn id(&self) -> ToolId {
        ToolId::Spline
    }

    fn activate(&mut self, state: &mut AppState) {
        self.ensure_init(state);
        self.load_selection(state);
        self.seen_revision = state.doc.revision();
    }

    fn reactivate(&mut self, state: &mut AppState) {
        self.load_selection(state);
    }

    fn deactivate(&mut self, state: &mut AppState) {
        if self.editing.is_some() && self.auto_regenerate && self.is_dirty() && self.meta.points.len() >= 2 {
            self.generate(state);
        }
        self.drag = None;
        self.hover = None;
    }

    fn update(&mut self, state: &mut AppState, _ctx: &egui::Context) {
        self.ensure_init(state);
        self.sync_with_document(state);
        self.refresh_bounds(state);
        self.regenerate_if_due(state);
    }

    fn action(&mut self, state: &mut AppState, action: Action) -> bool {
        match action {
            Action::SplineEdit => {
                self.load_selection(state);
                true
            }
            Action::SplineBake => {
                if self.editing.is_none() {
                    self.load_selection(state);
                }
                self.bake(state)
            }
            _ => false,
        }
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn wheel(&mut self, state: &mut AppState, _view: &ViewInfo, notches: f32, mods: egui::Modifiers) -> ToolFlow {
        let Some(i) = self.selected.filter(|i| *i < self.meta.points.len()) else { return ToolFlow::Ignored };
        if !(mods.shift || mods.alt) || mods.ctrl || mods.command || !notches.is_finite() {
            self.wheel = 0.0;
            return ToolFlow::Ignored;
        }
        self.wheel += notches;
        let steps = self.wheel.trunc();
        self.wheel -= steps;
        if steps != 0.0 {
            let p = &mut self.meta.points[i];
            if mods.alt {
                p.width =
                    ((p.width + f64::from(steps) * 0.1) * 100.0).round().clamp(MIN_WIDTH * 100.0, MAX_WIDTH * 100.0)
                        / 100.0;
            } else {
                p.roll = (p.roll + f64::from(steps) * 5.0).clamp(-MAX_ROLL, MAX_ROLL);
            }
            self.touch(state);
        }
        ToolFlow::Handled
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.meta.points.len() >= 2 && consume_plain_key(ctx, Key::Enter) {
            self.generate(state);
            return true;
        }
        if let Some(i) = self.selected
            && consume_plain_key(ctx, Key::Delete)
        {
            self.delete_point(i);
            self.touch(state);
            return true;
        }
        if self.editing.is_none() && !self.meta.points.is_empty() && consume_plain_key(ctx, Key::Backspace) {
            let last = self.meta.points.len() - 1;
            self.delete_point(last);
            return true;
        }
        if consume_plain_key(ctx, Key::Escape) {
            return self.cancel(state);
        }
        false
    }

    fn context_entries(&self, _state: &AppState) -> Vec<ToolMenuEntry> {
        let n = self.meta.points.len();
        let mut out = vec![
            ToolMenuEntry {
                label: t!("spline.menu_insert").to_owned(),
                id: MENU_INSERT,
                enabled: self.menu_curve.is_some(),
            },
            ToolMenuEntry {
                label: t!("spline.menu_delete").to_owned(),
                id: MENU_DELETE,
                enabled: self.selected.is_some(),
            },
            ToolMenuEntry {
                label: if self.meta.closed { t!("spline.menu_open") } else { t!("spline.menu_close") }.to_owned(),
                id: MENU_CLOSE,
                enabled: n >= 3,
            },
        ];
        if self.meta.interp == Interp::Bezier {
            out.push(ToolMenuEntry {
                label: t!("spline.auto_handles").to_owned(),
                id: MENU_AUTO_HANDLES,
                enabled: self.selected.is_some(),
            });
        }
        out.push(ToolMenuEntry { label: t!("spline.generate").to_owned(), id: MENU_GENERATE, enabled: n >= 2 });
        out.push(ToolMenuEntry { label: t!("spline.bake").to_owned(), id: MENU_BAKE, enabled: self.editing.is_some() });
        out.push(ToolMenuEntry { label: t!("spline.new").to_owned(), id: MENU_NEW, enabled: n > 0 });
        out
    }

    fn context_command(&mut self, state: &mut AppState, id: u32) {
        match id {
            MENU_INSERT => {
                if let Some((i, p)) = self.menu_curve {
                    self.insert_point(i, p);
                    self.touch(state);
                }
            }
            MENU_DELETE => {
                if let Some(i) = self.selected {
                    self.delete_point(i);
                    self.touch(state);
                }
            }
            MENU_CLOSE => {
                self.meta.closed = !self.meta.closed;
                self.touch(state);
            }
            MENU_AUTO_HANDLES => {
                if let Some(p) = self.selected.and_then(|i| self.meta.points.get_mut(i)) {
                    p.handle_in = None;
                    p.handle_out = None;
                    self.touch(state);
                }
            }
            MENU_GENERATE => {
                self.generate(state);
            }
            MENU_BAKE => {
                self.bake(state);
            }
            MENU_NEW => self.start_new(),
            _ => {}
        }
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let mut ov = Ov::new(out, view);
        let hover = self.hover.filter(|h| fresh(state, h.frame) && h.kind == view.kind);
        let line = rgba(COLOR_PENDING, 1.0);
        let show_plan = self.is_dirty() || self.editing.is_none();
        if self.meta.points.len() >= 2 {
            let shown = self.preview(state);
            match shown.as_ref() {
                Ok(plan) => {
                    for w in plan.curve.windows(2) {
                        ov.xray_line(w[0], w[1], line);
                    }
                    if show_plan {
                        self.draw_plan(&mut ov, plan, true);
                    }
                }
                Err(_) => {
                    for w in self.meta.points.windows(2) {
                        ov.line(w[0].pos, w[1].pos, rgba(COLOR_REMOVED, 1.0));
                    }
                }
            }
        }
        for (i, p) in self.meta.points.iter().enumerate() {
            let sel = self.selected == Some(i);
            let near = hover.and_then(|h| h.near).is_some_and(|(k, part)| k == i && part == Part::Point);
            let color = if sel || near { rgba(hp_app::theme::VIEW_ACCENT, 1.0) } else { line };
            ov.point(p.pos, color, if sel { 10.0 } else { 8.0 });
        }
        if self.meta.interp == Interp::Bezier
            && let Some(i) = self.selected.filter(|i| *i < self.meta.points.len())
        {
            let c = rgba(COLOR_GHOST, 1.0);
            for part in [Part::In, Part::Out] {
                if let Some(h) = self.handle_world(i, part) {
                    ov.line(self.meta.points[i].pos, h, c);
                    ov.point(h, c, 7.0);
                }
            }
        }
        if let Some(h) = hover {
            if let Some((_, p)) = h.curve {
                ov.point(p, rgba(COLOR_GHOST, 1.0), 8.0);
            } else if let Some(p) = h.add.filter(|_| self.drag.is_none()) {
                ov.point(p, rgba(COLOR_PENDING, 0.6), 6.0);
                let at = self.add_index();
                let n = self.meta.points.len();
                let link = |k: usize| self.meta.points.get(k).map(|q| q.pos);
                if n > 0 {
                    let from = if at == 0 { link(0) } else { link(at - 1) };
                    if let Some(f) = from {
                        ov.line(f, p, rgba(COLOR_PENDING, 0.35));
                    }
                }
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let Some(h) = self.hover.filter(|h| fresh(state, h.frame) && h.kind == view.kind) else { return };
        let Some(p) = h.add.or(h.curve.map(|c| c.1)) else { return };
        let Some(s) = view.world_to_screen(p) else { return };
        let at = s + Vec2::new(12.0, 12.0);
        if self.meta.points.len() >= 2 {
            match self.preview(state).as_ref() {
                Ok(plan) => {
                    let text = tf!(
                        "spline.label",
                        self.meta.points.len(),
                        fmt_len(plan.length),
                        plan.solids.len(),
                        plan.entities.len()
                    );
                    paint_label(&painter, at, Align2::LEFT_TOP, &text, COLOR_PENDING);
                }
                Err(e) => paint_label(&painter, at, Align2::LEFT_TOP, e, hp_app::theme::VIEW_ERROR),
            }
        } else if let Some(r) = self.reference_point() {
            paint_label(&painter, at, Align2::LEFT_TOP, &fmt_len(r.distance(p)), COLOR_PENDING);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.ensure_init(state);
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            let mut mode = self.meta.mode;
            egui::ComboBox::from_id_salt("spline_mode").selected_text(Self::mode_label(mode)).show_ui(ui, |ui| {
                for m in Mode::ALL {
                    ui.selectable_value(&mut mode, *m, Self::mode_label(*m));
                }
            });
            if mode != self.meta.mode {
                self.meta.mode = mode;
                changed = true;
            }
            let mut interp = self.meta.interp;
            ui.selectable_value(&mut interp, Interp::CatmullRom, t!("spline.catmull_rom"))
                .on_hover_text(t!("spline.catmull_rom_tip"));
            ui.selectable_value(&mut interp, Interp::Bezier, t!("spline.bezier"))
                .on_hover_text(t!("spline.bezier_tip"));
            if interp != self.meta.interp {
                self.meta.interp = interp;
                changed = true;
            }
            changed |= ui.checkbox(&mut self.meta.closed, t!("spline.closed")).changed();
            changed |= ui
                .checkbox(&mut self.meta.follow_terrain, t!("spline.follow_terrain"))
                .on_hover_text(t!("spline.follow_terrain_tip"))
                .changed();
            if self.meta.follow_terrain {
                changed |= ui
                    .add(egui::DragValue::new(&mut self.meta.terrain_offset).speed(0.5).range(-4096.0..=4096.0))
                    .on_hover_text(t!("spline.terrain_offset_tip"))
                    .changed();
            }
            ui.separator();
            if hp_app::widgets::icon_toggle(ui, "🔄", self.auto_regenerate, t!("spline.auto_tip")).clicked() {
                self.auto_regenerate = !self.auto_regenerate;
            }
            if hp_app::widgets::icon_toggle(ui, "📌", self.keep_active, t!("spline.keep_active_tip")).clicked() {
                self.keep_active = !self.keep_active;
            }
            let n = self.meta.points.len();
            let label = if self.editing.is_some() { t!("spline.apply") } else { t!("spline.generate") };
            if ui
                .add_enabled(n >= 2, egui::Button::new(label))
                .on_hover_text(tf!("spline.generate_tip", key_label(Key::Enter)))
                .clicked()
            {
                self.generate(state);
            }
            if ui
                .add_enabled(self.editing.is_some(), egui::Button::new(t!("spline.bake")))
                .on_hover_text(t!("spline.bake_tip"))
                .clicked()
            {
                self.bake(state);
            }
            if ui.add_enabled(n > 0, egui::Button::new(t!("spline.new"))).on_hover_text(t!("spline.new_tip")).clicked()
            {
                self.start_new();
            }
            if ui
                .add_enabled(self.selected.is_some(), egui::Button::new(t!("spline.menu_delete")))
                .on_hover_text(tf!("spline.delete_tip", key_label(Key::Delete)))
                .clicked()
                && let Some(i) = self.selected
            {
                self.delete_point(i);
                changed = true;
            }
        });
        changed |= self.point_ui(ui);
        changed |= self.mode_ui(ui);
        ui.horizontal_wrapped(|ui| {
            let n = self.meta.points.len();
            if self.editing.is_some() {
                ui.label(t!("spline.editing"));
            }
            ui.label(tf!("spline.count", n));
            if n >= 2 {
                match self.preview(state).as_ref() {
                    Ok(p) => {
                        ui.label(tf!("spline.summary", fmt_len(p.length), p.solids.len(), p.entities.len()));
                        if p.skipped > 0 {
                            ui.label(tf!("spline.skipped", p.skipped));
                        }
                        for note in &p.notes {
                            ui.label(egui::RichText::new(note).weak());
                        }
                    }
                    Err(e) => {
                        ui.label(egui::RichText::new(e).color(hp_app::theme::VIEW_ERROR));
                    }
                }
            }
            if !self.pending.is_empty() {
                ui.label(egui::RichText::new(t!("spline.loading_models")).weak());
            }
        });
        if changed {
            self.touch(state);
        }
    }

    fn cursor(&self, _state: &AppState, _view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        Some(if self.drag.is_some() { CursorIcon::Grabbing } else { CursorIcon::Crosshair })
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        Some(if self.meta.points.is_empty() {
            t!("spline.hint_idle").to_owned()
        } else {
            tf!("spline.hint_points", key_label(Key::Enter), key_label(Key::Delete), key_label(Key::Escape))
        })
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        if self.drag.take().is_some() {
            return true;
        }
        if self.selected.take().is_some() {
            return true;
        }
        if self.editing.is_none() && !self.meta.points.is_empty() {
            self.meta.points.clear();
            return true;
        }
        if self.editing.is_some() {
            if let Some(a) = self.applied.clone() {
                self.meta = a;
            }
            self.editing = None;
            self.applied = None;
            self.meta.points.clear();
            return true;
        }
        false
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        self.ensure_init(state);
        if let Some(done) = self.set_point_option(key, value) {
            if done {
                self.touch(state);
            }
            return done;
        }
        match key {
            "auto_regenerate" => return parse_flag(value).map(|b| self.auto_regenerate = b).is_some(),
            "keep_active" => return parse_flag(value).map(|b| self.keep_active = b).is_some(),
            "points" => return value.trim() == self.meta.points.len().to_string(),
            "editing" => {
                return value.trim() == self.editing.map_or_else(|| "none".to_owned(), |a| a.to_string());
            }
            _ => {}
        }
        let ok = self.meta.set_option(key, value).is_ok();
        if ok {
            self.touch(state);
        }
        ok
    }

    fn option_order(&self, key: &str) -> i32 {
        match key {
            "selected_point" => -1,
            _ => 0,
        }
    }

    fn option_values(&self) -> Vec<ToolOption> {
        let mut out: Vec<ToolOption> = OPTIONS
            .iter()
            .filter_map(|o| self.meta.get_option(o.key).map(|v| ToolOption::new(o.key, v, o.takes)))
            .collect();
        let flag = |b: bool| if b { "on" } else { "off" }.to_owned();
        out.push(ToolOption::new(
            "auto_regenerate",
            flag(self.auto_regenerate),
            "on: an edited spline regenerates its objects after each change",
        ));
        out.push(ToolOption::new("keep_active", flag(self.keep_active), "keep the spline tool active after creating"));
        out.push(ToolOption::new(
            "selected_point",
            self.selected.map_or(0, |i| i + 1).to_string(),
            "the selected point, numbered from 1 (0: none)",
        ));
        let p = self.selected.and_then(|i| self.meta.points.get(i));
        let val = |f: &dyn Fn(&SplinePoint) -> String| p.map_or_else(String::new, f);
        out.push(ToolOption::new("point_x", val(&|p| fmt_num(p.pos.x)), "x of the selected point"));
        out.push(ToolOption::new("point_y", val(&|p| fmt_num(p.pos.y)), "y of the selected point"));
        out.push(ToolOption::new("point_z", val(&|p| fmt_num(p.pos.z)), "z of the selected point"));
        out.push(ToolOption::new("point_roll", val(&|p| fmt_num(p.roll)), "roll of the selected point in degrees"));
        out.push(ToolOption::new("point_width", val(&|p| fmt_num(p.width)), "width scale of the selected point"));
        out.push(ToolOption::new(
            "point_auto_handles",
            val(&|p| flag(p.handle_in.is_none() && p.handle_out.is_none())),
            "on: the bezier handles of the selected point are automatic (empty while no point is selected)",
        ));
        out.push(ToolOption::new("points", self.meta.points.len().to_string(), "point count (read only)"));
        out.push(ToolOption::new(
            "editing",
            self.editing.map_or_else(|| "none".to_owned(), |a| a.to_string()),
            "id of the spline anchor being edited (read only)",
        ));
        out
    }
}
