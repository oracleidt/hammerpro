use egui::{Align2, CursorIcon, Key, Pos2, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::key_label;
use hp_app::state::AppState;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::{EditError, Owner};
use hp_geom::Aabb;
use hp_render::Overlay;

use super::geom::{
    BuildError, CurveError, CurveLayout, CurveSpec, MAX_CURVE_POINTS, MAX_HEIGHT, MAX_SUBDIVISIONS, MAX_THICKNESS,
    MIN_HEIGHT, MIN_SUBDIVISIONS, MIN_THICKNESS, build_brushes, clamp_height, clamp_subdivisions, clamp_thickness,
    clean_points, layout, quad_corners,
};
use crate::block::{CreateAs, create_as_ui, new_depth_range};
use crate::common::{
    COLOR_PENDING, COLOR_REMOVED, COLOR_SHAPE, Edge, HANDLE_TOLERANCE, MIN_EXTENT, Mouse, Ov, Snap, active_material,
    box_screen_rect, consume_plain_key, depth_axis, fgd_defaults, fmt_len, fmt_size, fresh, material_chip,
    paint_handle, paint_label, rgba,
};
use crate::picker::ClassPicker;

const FREEHAND_SPACING_PX: f32 = 24.0;
const DEFAULT_SUBDIVISIONS: u32 = 16;
const DEFAULT_THICKNESS: f64 = 16.0;

#[derive(Clone, Debug, PartialEq)]
pub struct CurveOptions {
    pub subdivisions: u32,
    pub thickness: f64,
    pub height: Option<f64>,
    pub tex_scale: f64,
    pub create_as: CreateAs,
    pub keep_active: bool,
}

impl Default for CurveOptions {
    fn default() -> Self {
        Self {
            subdivisions: DEFAULT_SUBDIVISIONS,
            thickness: DEFAULT_THICKNESS,
            height: None,
            tex_scale: hp_geom::DEFAULT_TEXTURE_SCALE,
            create_as: CreateAs::World,
            keep_active: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurvePhase {
    Drawing,
    Adjusting,
}

#[derive(Clone, Debug, PartialEq)]
struct Stroke {
    kind: ViewKind,
    points: Vec<DVec2>,
    depth: (f64, f64),
}

impl Stroke {
    fn depth_axis(&self) -> usize {
        depth_axis(self.kind)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hover {
    kind: ViewKind,
    point: DVec2,
    full: bool,
    frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Point { view: usize, index: usize },
    Freehand { view: usize },
    Depth { view: usize, end: Option<Edge>, start: (f64, f64), from: f64 },
    Swallow,
}

pub struct CurveTool {
    pub options: CurveOptions,
    stroke: Option<Stroke>,
    phase: CurvePhase,
    hover: Option<Hover>,
    drag: Option<Drag>,
    depth_hover: Option<(usize, Option<Edge>, u64)>,
    last_bounds: Option<Aabb>,
    initialized: bool,
    picker: ClassPicker,
}

impl Default for CurveTool {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn error_text(e: &BuildError) -> String {
    match e {
        BuildError::Curve(CurveError::TooFewPoints) => t!("curve.err.points").to_owned(),
        BuildError::Curve(CurveError::TooShort { segment }) => tf!("curve.err.short", fmt_len(*segment)),
        BuildError::Curve(CurveError::Folded(list)) => tf!("curve.err.folded", list.len()),
        BuildError::Curve(CurveError::OutOfRange) => t!("curve.err.range").to_owned(),
        BuildError::Height => tf!("curve.err.height", MIN_HEIGHT, MAX_HEIGHT),
        BuildError::Geom(g) => tf!("curve.failed", g),
    }
}

impl CurveTool {
    pub fn new() -> Self {
        Self {
            options: CurveOptions::default(),
            stroke: None,
            phase: CurvePhase::Drawing,
            hover: None,
            drag: None,
            depth_hover: None,
            last_bounds: None,
            initialized: false,
            picker: ClassPicker::default(),
        }
    }

    pub fn points(&self) -> &[DVec2] {
        self.stroke.as_ref().map_or(&[], |s| &s.points)
    }

    pub fn depth(&self) -> Option<(f64, f64)> {
        self.stroke.as_ref().map(|s| s.depth)
    }

    pub fn view_kind(&self) -> Option<ViewKind> {
        self.stroke.as_ref().map(|s| s.kind)
    }

    pub fn phase(&self) -> CurvePhase {
        self.phase
    }

    pub fn last_bounds(&self) -> Option<Aabb> {
        self.last_bounds
    }

    pub fn is_ready(&self) -> bool {
        self.stroke.as_ref().is_some_and(|s| clean_points(&s.points).len() >= 2)
    }

    pub fn layout(&self) -> Option<Result<CurveLayout, CurveError>> {
        let s = self.stroke.as_ref()?;
        Some(layout(&s.points, self.options.subdivisions, self.options.thickness))
    }

    fn valid_layout(&self) -> Result<CurveLayout, BuildError> {
        let l = self.layout().unwrap_or(Err(CurveError::TooFewPoints)).map_err(BuildError::Curve)?;
        l.check().map_err(BuildError::Curve)?;
        Ok(l)
    }

    fn ensure_init(&mut self, state: &AppState) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let s = state.settings.edit.default_texture_scale;
        if s.is_finite() && s.abs() > 1e-6 {
            self.options.tex_scale = s;
        }
    }

    pub fn set_subdivisions(&mut self, n: i64) {
        self.options.subdivisions = clamp_subdivisions(n);
    }

    pub fn set_thickness(&mut self, t: f64) {
        self.options.thickness = clamp_thickness(t);
    }

    pub fn set_height(&mut self, h: f64) {
        let h = clamp_height(h);
        if let Some(s) = self.stroke.as_mut() {
            s.depth.1 = s.depth.0 + h;
        }
    }

    pub fn add_point(&mut self, state: &AppState, kind: ViewKind, p: DVec2) -> bool {
        if !kind.is_2d() || !p.is_finite() {
            return false;
        }
        match &mut self.stroke {
            Some(s) if s.kind == kind => {
                if s.points.len() >= MAX_CURVE_POINTS || s.points.last().is_some_and(|q| q.distance_squared(p) < 1e-12)
                {
                    return false;
                }
                s.points.push(p);
                true
            }
            Some(_) => false,
            None => {
                let last = state.runtime.brush.last_created.or(self.last_bounds);
                let (lo, hi) = new_depth_range(state, last, depth_axis(kind));
                let depth = match self.options.height {
                    Some(h) => (lo, lo + clamp_height(h)),
                    None => (lo, hi),
                };
                self.stroke = Some(Stroke { kind, points: vec![p], depth });
                self.phase = CurvePhase::Drawing;
                true
            }
        }
    }

    pub fn remove_last_point(&mut self) -> bool {
        let Some(s) = self.stroke.as_mut() else { return false };
        s.points.pop();
        if s.points.is_empty() {
            self.stroke = None;
            self.phase = CurvePhase::Drawing;
        }
        self.drag = None;
        true
    }

    pub fn set_depth(&mut self, lo: f64, hi: f64) {
        if let Some(s) = self.stroke.as_mut()
            && lo.is_finite()
            && hi.is_finite()
            && (hi - lo).abs() >= MIN_HEIGHT
        {
            s.depth = (lo.min(hi), lo.max(hi));
        }
    }

    pub fn validate(&mut self) -> bool {
        if !self.is_ready() {
            return false;
        }
        self.phase = CurvePhase::Adjusting;
        self.drag = None;
        true
    }

    pub fn back_to_drawing(&mut self) -> bool {
        let was = self.phase == CurvePhase::Adjusting;
        self.phase = CurvePhase::Drawing;
        was
    }

    pub fn create(&mut self, state: &mut AppState) -> Option<usize> {
        self.ensure_init(state);
        let s = self.stroke.clone()?;
        let material = active_material(state);
        let lightmap = match state.settings.edit.default_lightmap_scale {
            v if v > 0 => v,
            _ => 16,
        };
        let spec = CurveSpec {
            kind: s.kind,
            points: &s.points,
            segments: self.options.subdivisions,
            thickness: self.options.thickness,
            depth: s.depth,
            material: &material,
            tex_scale: self.options.tex_scale,
            lightmap_scale: lightmap,
        };
        let mut next = state.doc.peek_next_side_id();
        let solids = match build_brushes(&spec, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        }) {
            Ok(solids) => solids,
            Err(e) => {
                state.notify(Severity::Warning, error_text(&e));
                return None;
            }
        };
        let bounds = self.preview_bounds().unwrap_or_default();
        let count = solids.len();
        let result: Result<usize, EditError> = match &self.options.create_as {
            CreateAs::World => state.doc.create_solids(solids, Owner::World).map(|ids| ids.len()),
            CreateAs::Entity(class) => {
                let kv = fgd_defaults(state, class);
                state.doc.create_brush_entity(class, &kv, solids).map(|_| count)
            }
        };
        match result {
            Ok(n) => {
                state.log(Severity::Info, tf!("curve.created", n, fmt_size(&bounds)));
                self.last_bounds = Some(bounds);
                state.runtime.brush.last_created = Some(bounds);
                self.stroke = None;
                self.phase = CurvePhase::Drawing;
                self.hover = None;
                self.drag = None;
                if !self.options.keep_active {
                    state.set_tool(ToolId::Select);
                }
                Some(n)
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("curve.failed", e));
                None
            }
        }
    }

    fn preview_bounds(&self) -> Option<Aabb> {
        let s = self.stroke.as_ref()?;
        let pts: Vec<DVec3> = match self.layout() {
            Some(Ok(l)) => l.quads.iter().flat_map(|q| quad_corners(s.kind, q, s.depth)).collect(),
            _ => s.points.iter().flat_map(|p| [s.kind.from_2d(*p, s.depth.0), s.kind.from_2d(*p, s.depth.1)]).collect(),
        };
        (!pts.is_empty()).then(|| Aabb::from_points(pts))
    }

    fn point_at(&self, view: &ViewInfo, pos: Pos2) -> Option<usize> {
        let s = self.stroke.as_ref().filter(|s| s.kind == view.kind)?;
        s.points
            .iter()
            .enumerate()
            .filter_map(|(i, p)| view.world_to_screen(s.kind.from_2d(*p, 0.0)).map(|q| (i, q.distance(pos))))
            .filter(|(_, d)| *d <= HANDLE_TOLERANCE + 2.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn depth_layout(&self, view: &ViewInfo) -> Option<(usize, egui::Rect)> {
        let s = self.stroke.as_ref()?;
        if !view.is_2d() || view.kind == s.kind || s.points.is_empty() {
            return None;
        }
        let (r, u, _) = view.kind.axes_2d()?;
        let d = s.depth_axis();
        let axis = if r == d {
            0
        } else if u == d {
            1
        } else {
            return None;
        };
        let mut b = self.preview_bounds()?;
        for i in 0..3 {
            if b.size()[i] < MIN_EXTENT {
                b.max[i] += MIN_EXTENT * 2.0;
            }
        }
        box_screen_rect(view, &b).map(|rect| (axis, rect))
    }

    fn depth_handle_at(&self, view: &ViewInfo, pos: Pos2) -> Option<Option<Edge>> {
        let (axis, rect) = self.depth_layout(view)?;
        if !rect.expand(HANDLE_TOLERANCE).contains(pos) {
            return None;
        }
        let (p, lo, hi) =
            if axis == 0 { (pos.x, rect.left(), rect.right()) } else { (pos.y, rect.bottom(), rect.top()) };
        let (dl, dh) = ((p - lo).abs(), (p - hi).abs());
        if dl.min(dh) <= HANDLE_TOLERANCE {
            return Some(Some(if dl <= dh { Edge::Min } else { Edge::Max }));
        }
        Some(None)
    }

    fn freehand_spacing(view: &ViewInfo, pos: Pos2, snap: &Snap) -> f64 {
        let px = view.screen_to_2d(pos).distance(view.screen_to_2d(pos + Vec2::new(FREEHAND_SPACING_PX, 0.0)));
        let grid = if snap.is_on() { snap.step() * 2.0 } else { 0.0 };
        px.max(grid).max(1.0)
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        self.ensure_init(state);
        if !view.is_2d() {
            return ToolFlow::Ignored;
        }
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::new(state, view, m.modifiers);
        let drawing_view = self.stroke.as_ref().is_none_or(|s| s.kind == view.kind);
        let adding = self.phase == CurvePhase::Drawing;
        if m.hovering() {
            self.depth_hover = self.depth_handle_at(view, pos).map(|e| (view.index, e, state.frame.index));
            self.hover = (drawing_view && adding).then(|| Hover {
                kind: view.kind,
                point: snap.v2(view.screen_to_2d(pos)),
                full: self.points().len() >= MAX_CURVE_POINTS,
                frame: state.frame.index,
            });
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        if m.drag_started {
            self.drag = None;
            if drawing_view && let Some(index) = self.point_at(view, origin) {
                self.drag = Some(Drag::Point { view: view.index, index });
            } else if !drawing_view && let Some(end) = self.depth_handle_at(view, origin) {
                if let Some(s) = self.stroke.as_ref() {
                    let from = view.kind.from_2d(view.screen_to_2d(origin), 0.0)[s.depth_axis()];
                    self.drag = Some(Drag::Depth { view: view.index, end, start: s.depth, from });
                }
            } else if drawing_view && adding {
                self.add_point(state, view.kind, snap.v2(view.screen_to_2d(origin)));
                self.drag = Some(Drag::Freehand { view: view.index });
            } else {
                self.drag = Some(Drag::Swallow);
            }
        }
        if m.dragging || m.drag_stopped {
            match self.drag {
                Some(Drag::Point { view: vi, index }) if vi == view.index => {
                    let p = snap.v2(view.screen_to_2d(pos));
                    if let Some(s) = self.stroke.as_mut()
                        && let Some(slot) = s.points.get_mut(index)
                        && p.is_finite()
                    {
                        *slot = p;
                    }
                }
                Some(Drag::Freehand { view: vi }) if vi == view.index => {
                    let p = snap.v2(view.screen_to_2d(pos));
                    let spacing = Self::freehand_spacing(view, pos, &snap);
                    let far = self.points().last().is_none_or(|q| q.distance(p) >= spacing);
                    if far || (m.drag_stopped && self.points().last().is_some_and(|q| q.distance_squared(p) > 1e-12)) {
                        self.add_point(state, view.kind, p);
                    }
                }
                Some(Drag::Depth { view: vi, end, start, from }) if vi == view.index => {
                    if let Some(s) = self.stroke.as_mut() {
                        let d = s.depth_axis();
                        let cur = view.kind.from_2d(view.screen_to_2d(pos), 0.0)[d];
                        let delta = cur - from;
                        if delta.is_finite() {
                            let (lo, hi) = start;
                            let next = match end {
                                Some(Edge::Min) => (snap.v(lo + delta, d), hi),
                                Some(Edge::Max) => (lo, snap.v(hi + delta, d)),
                                None => {
                                    let shift = snap.v(lo + delta, d) - lo;
                                    (lo + shift, hi + shift)
                                }
                            };
                            if next.1 - next.0 >= MIN_HEIGHT && next.1 - next.0 <= MAX_HEIGHT {
                                s.depth = next;
                            }
                        }
                    }
                    self.depth_hover = Some((view.index, end, state.frame.index));
                }
                _ => {}
            }
        }
        if m.clicked && !m.drag_started && drawing_view && self.drag.is_none() {
            if m.double_clicked {
                match self.phase {
                    CurvePhase::Drawing => {
                        self.validate();
                    }
                    CurvePhase::Adjusting => {
                        self.create(state);
                    }
                }
            } else if adding {
                self.add_point(state, view.kind, snap.v2(view.screen_to_2d(origin)));
            }
        }
        if m.drag_stopped {
            self.drag = None;
        }
        ToolFlow::Handled
    }

    fn hover_in(&self, state: &AppState, kind: ViewKind) -> Option<Hover> {
        self.hover.filter(|h| h.kind == kind && fresh(state, h.frame) && self.drag.is_none())
    }

    fn sizes_ui(&mut self, ui: &mut Ui) {
        ui.label(t!("curve.subdivisions")).on_hover_text(t!("curve.subdivisions_tip"));
        let mut n = self.options.subdivisions;
        ui.add(egui::DragValue::new(&mut n).speed(0.25).range(MIN_SUBDIVISIONS..=MAX_SUBDIVISIONS))
            .on_hover_text(t!("curve.subdivisions_tip"));
        if ui.small_button("÷2").on_hover_text(t!("curve.halve_tip")).clicked() {
            n /= 2;
        }
        if ui.small_button("×2").on_hover_text(t!("curve.double_tip")).clicked() {
            n = n.saturating_mul(2);
        }
        self.set_subdivisions(i64::from(n));
        ui.label(t!("curve.thickness")).on_hover_text(t!("curve.thickness_tip"));
        let mut th = self.options.thickness;
        ui.add(egui::DragValue::new(&mut th).speed(0.5).range(MIN_THICKNESS..=MAX_THICKNESS).max_decimals(2))
            .on_hover_text(t!("curve.thickness_tip"));
        self.set_thickness(th);
        if let Some((lo, hi)) = self.depth() {
            ui.label(t!("curve.height")).on_hover_text(t!("curve.height_tip"));
            let mut h = hi - lo;
            let resp = ui
                .add(egui::DragValue::new(&mut h).speed(0.5).range(MIN_HEIGHT..=MAX_HEIGHT).max_decimals(2))
                .on_hover_text(t!("curve.height_tip"));
            if resp.changed() {
                self.set_height(h);
            }
        }
    }

    fn summary(&self) -> Option<(String, bool)> {
        let l = self.layout()?;
        Some(match l {
            Ok(l) if l.folded.is_empty() => (tf!("curve.summary", l.quads.len(), fmt_len(l.length)), true),
            Ok(l) => (error_text(&BuildError::Curve(CurveError::Folded(l.folded))), false),
            Err(e) => (error_text(&BuildError::Curve(e)), false),
        })
    }

    fn short_summary(&self) -> Option<(String, bool)> {
        let l = self.layout()?;
        Some(match l {
            Ok(l) if l.folded.is_empty() => (tf!("curve.summary", l.quads.len(), fmt_len(l.length)), true),
            Ok(l) => (tf!("curve.short_folded", l.folded.len()), false),
            Err(CurveError::TooShort { .. }) => (t!("curve.short_short").to_owned(), false),
            Err(e) => (error_text(&BuildError::Curve(e)), false),
        })
    }

    fn adjust_window(&mut self, ui: &mut Ui, state: &mut AppState) {
        if self.phase != CurvePhase::Adjusting || self.stroke.is_none() {
            return;
        }
        let mut open = true;
        let mut create = false;
        let mut back = false;
        hp_app::chrome::Window::new(t!("curve.window_title"))
            .id(egui::Id::unique("hp_curve_adjust"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(Pos2::new(ui.ctx().content_rect().left() + 64.0, ui.ctx().content_rect().bottom() - 280.0))
            .show(ui.ctx(), |ui| {
                egui::Grid::new("hp_curve_adjust_grid").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.label(t!("curve.subdivisions")).on_hover_text(t!("curve.subdivisions_tip"));
                    ui.horizontal(|ui| {
                        let mut n = self.options.subdivisions;
                        ui.add(egui::DragValue::new(&mut n).speed(0.25).range(MIN_SUBDIVISIONS..=MAX_SUBDIVISIONS));
                        if ui.small_button("÷2").on_hover_text(t!("curve.halve_tip")).clicked() {
                            n /= 2;
                        }
                        if ui.small_button("×2").on_hover_text(t!("curve.double_tip")).clicked() {
                            n = n.saturating_mul(2);
                        }
                        self.set_subdivisions(i64::from(n));
                    });
                    ui.end_row();
                    ui.label(t!("curve.thickness")).on_hover_text(t!("curve.thickness_tip"));
                    let mut th = self.options.thickness;
                    ui.add(
                        egui::DragValue::new(&mut th).speed(0.5).range(MIN_THICKNESS..=MAX_THICKNESS).max_decimals(2),
                    );
                    self.set_thickness(th);
                    ui.end_row();
                    if let Some((lo, hi)) = self.depth() {
                        ui.label(t!("curve.height")).on_hover_text(t!("curve.height_tip"));
                        let mut h = hi - lo;
                        let resp = ui.add(
                            egui::DragValue::new(&mut h).speed(0.5).range(MIN_HEIGHT..=MAX_HEIGHT).max_decimals(2),
                        );
                        if resp.changed() {
                            self.set_height(h);
                        }
                        ui.end_row();
                    }
                });
                if let Some((text, ok)) = self.summary() {
                    let color = if ok { ui.visuals().weak_text_color() } else { hp_app::theme::VIEW_ERROR };
                    ui.add(egui::Label::new(egui::RichText::new(text).color(color)).wrap());
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.valid_layout().is_ok(), egui::Button::new(t!("curve.create")))
                        .on_hover_text(tf!("curve.create_tip", key_label(Key::Enter)))
                        .clicked()
                    {
                        create = true;
                    }
                    if ui
                        .button(t!("curve.back"))
                        .on_hover_text(tf!("curve.back_tip", key_label(Key::Escape)))
                        .clicked()
                    {
                        back = true;
                    }
                });
            });
        if create {
            self.create(state);
        } else if back || !open {
            self.back_to_drawing();
        }
    }
}

impl Tool for CurveTool {
    fn id(&self) -> ToolId {
        ToolId::Curve
    }

    fn activate(&mut self, state: &mut AppState) {
        self.ensure_init(state);
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.cancel(state);
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.stroke.is_none() {
            return false;
        }
        if consume_plain_key(ctx, Key::Enter) {
            match self.phase {
                CurvePhase::Drawing => {
                    self.validate();
                }
                CurvePhase::Adjusting => {
                    self.create(state);
                }
            }
            return true;
        }
        if self.phase == CurvePhase::Drawing && consume_plain_key(ctx, Key::Backspace) {
            return self.remove_last_point();
        }
        if consume_plain_key(ctx, Key::Escape) {
            if self.back_to_drawing() {
                return true;
            }
            return self.cancel(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let mut ov = Ov::new(out, view);
        let line = rgba(COLOR_PENDING, 1.0);
        let Some(s) = &self.stroke else {
            if let Some(h) = self.hover_in(state, view.kind) {
                ov.point(view.kind.from_2d(h.point, 0.0), line, 6.0);
            }
            return;
        };
        let bad = rgba(COLOR_REMOVED, 1.0);
        let shape = rgba(COLOR_SHAPE, 0.9);
        let layout = self.layout().and_then(Result::ok);
        let folded = |i: usize| layout.as_ref().is_some_and(|l| l.folded.binary_search(&i).is_ok());
        if view.is_2d() && view.kind == s.kind {
            for w in s.points.windows(2) {
                ov.line(s.kind.from_2d(w[0], 0.0), s.kind.from_2d(w[1], 0.0), rgba(COLOR_PENDING, 0.35));
            }
            if let Some(l) = &layout {
                for w in l.centers.windows(2) {
                    ov.line(s.kind.from_2d(w[0], 0.0), s.kind.from_2d(w[1], 0.0), rgba(COLOR_PENDING, 0.8));
                }
                for (i, q) in l.quads.iter().enumerate() {
                    let pts: Vec<DVec3> = q.iter().map(|p| s.kind.from_2d(*p, 0.0)).collect();
                    ov.polygon_outline(&pts, if folded(i) { bad } else { shape }, false);
                }
            }
            for p in &s.points {
                ov.point(s.kind.from_2d(*p, 0.0), line, 7.0);
            }
            if let Some(h) = self.hover_in(state, view.kind)
                && let Some(last) = s.points.last()
            {
                let color = if h.full { bad } else { rgba(COLOR_PENDING, 0.7) };
                let hp = s.kind.from_2d(h.point, 0.0);
                ov.line(s.kind.from_2d(*last, 0.0), hp, color);
                ov.point(hp, color, if h.full { 9.0 } else { 6.0 });
            }
            return;
        }
        let Some(l) = &layout else {
            for p in &s.points {
                ov.xray_line(s.kind.from_2d(*p, s.depth.0), s.kind.from_2d(*p, s.depth.1), line);
            }
            return;
        };
        let fill = rgba(COLOR_PENDING, 0.10);
        for (i, q) in l.quads.iter().enumerate() {
            let c = quad_corners(s.kind, q, s.depth);
            ov.hull_edges(&c, if folded(i) { bad } else { line }, true);
            if !view.is_2d() {
                ov.hull_faces(&c, if folded(i) { rgba(COLOR_REMOVED, 0.25) } else { fill });
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let Some(s) = &self.stroke else { return };
        if !view.is_2d() {
            if let Some(b) = self.preview_bounds()
                && let Some(p) = view.world_to_screen(b.center())
                && view.rect.contains(p)
                && let Some((text, ok)) = self.short_summary()
            {
                let color = if ok { COLOR_PENDING } else { hp_app::theme::VIEW_ERROR };
                paint_label(&painter, p - Vec2::new(0.0, 8.0), Align2::CENTER_BOTTOM, &text, color);
            }
            return;
        }
        if view.kind == s.kind {
            if let Some(h) = self.hover_in(state, view.kind)
                && let Some(p) = view.world_to_screen(s.kind.from_2d(h.point, 0.0))
            {
                let at = p + Vec2::new(12.0, 12.0);
                if h.full {
                    paint_label(&painter, at, Align2::LEFT_TOP, t!("curve.full"), hp_app::theme::VIEW_ERROR);
                } else if let Some(last) = s.points.last() {
                    paint_label(&painter, at, Align2::LEFT_TOP, &fmt_len(last.distance(h.point)), COLOR_PENDING);
                }
            }
            if let Some((text, ok)) = self.short_summary()
                && let Some(last) = s.points.last()
                && let Some(p) = view.world_to_screen(s.kind.from_2d(*last, 0.0))
            {
                let color = if ok { COLOR_PENDING } else { hp_app::theme::VIEW_ERROR };
                paint_label(&painter, p - Vec2::new(0.0, 12.0), Align2::CENTER_BOTTOM, &text, color);
            }
            return;
        }
        let Some((axis, rect)) = self.depth_layout(view) else { return };
        let size = s.depth.1 - s.depth.0;
        let hovered =
            self.depth_hover.filter(|(vi, _, frame)| *vi == view.index && fresh(state, *frame)).and_then(|(_, e, _)| e);
        if axis == 0 {
            paint_label(
                &painter,
                Pos2::new(rect.center().x, rect.top() - 4.0),
                Align2::CENTER_BOTTOM,
                &fmt_len(size),
                COLOR_PENDING,
            );
            paint_handle(&painter, Pos2::new(rect.left(), rect.center().y), hovered == Some(Edge::Min));
            paint_handle(&painter, Pos2::new(rect.right(), rect.center().y), hovered == Some(Edge::Max));
        } else {
            paint_label(
                &painter,
                Pos2::new(rect.left() - 6.0, rect.center().y),
                Align2::RIGHT_CENTER,
                &fmt_len(size),
                COLOR_PENDING,
            );
            paint_handle(&painter, Pos2::new(rect.center().x, rect.bottom()), hovered == Some(Edge::Min));
            paint_handle(&painter, Pos2::new(rect.center().x, rect.top()), hovered == Some(Edge::Max));
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.ensure_init(state);
        ui.horizontal_wrapped(|ui| {
            self.sizes_ui(ui);
            ui.separator();
            let scale =
                egui::DragValue::new(&mut self.options.tex_scale).speed(0.01).range(0.01..=64.0).max_decimals(3);
            let tip = t!("create.block.tex_scale_tip");
            ui.label(t!("create.block.tex_scale")).on_hover_text(tip);
            ui.add(scale).on_hover_text(tip);
            ui.separator();
            create_as_ui(ui, state, &mut self.picker, &mut self.options.create_as, "curve_create_as");
            ui.separator();
            material_chip(ui, state);
            ui.separator();
            if hp_app::widgets::icon_toggle(ui, "📌", self.options.keep_active, t!("curve.keep_active_tip")).clicked()
            {
                self.options.keep_active = !self.options.keep_active;
            }
            let count = self.points().len();
            if ui
                .add_enabled(count > 0 && self.phase == CurvePhase::Drawing, egui::Button::new(t!("curve.undo_point")))
                .on_hover_text(tf!("curve.undo_point_tip", key_label(Key::Backspace)))
                .clicked()
            {
                self.remove_last_point();
            }
            match self.phase {
                CurvePhase::Drawing => {
                    if ui
                        .add_enabled(self.is_ready(), egui::Button::new(t!("curve.validate")))
                        .on_hover_text(tf!("curve.validate_tip", key_label(Key::Enter)))
                        .clicked()
                    {
                        self.validate();
                    }
                }
                CurvePhase::Adjusting => {
                    if ui
                        .add_enabled(self.valid_layout().is_ok(), egui::Button::new(t!("curve.create")))
                        .on_hover_text(tf!("curve.create_tip", key_label(Key::Enter)))
                        .clicked()
                    {
                        self.create(state);
                    }
                }
            }
            if count > 0 {
                ui.separator();
                ui.label(tf!("curve.count", count));
                if let Some((text, ok)) = self.short_summary() {
                    let color = if ok { ui.visuals().weak_text_color() } else { hp_app::theme::VIEW_ERROR };
                    let resp = ui.label(egui::RichText::new(text).color(color));
                    if !ok && let Some((long, _)) = self.summary() {
                        resp.on_hover_text(long);
                    }
                }
            }
        });
        self.adjust_window(ui, state);
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if !view.is_2d() {
            return None;
        }
        match self.drag {
            Some(Drag::Point { .. }) => return Some(CursorIcon::Grabbing),
            Some(Drag::Depth { .. }) => return Some(CursorIcon::Move),
            _ => {}
        }
        if self.point_at(view, pos).is_some() {
            return Some(CursorIcon::Grab);
        }
        if let Some(end) = self.depth_handle_at(view, pos)
            && let Some((axis, _)) = self.depth_layout(view)
        {
            return Some(match (end, axis) {
                (None, _) => CursorIcon::Move,
                (Some(_), 0) => CursorIcon::ResizeHorizontal,
                (Some(_), _) => CursorIcon::ResizeVertical,
            });
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        let enter = key_label(Key::Enter);
        let back = key_label(Key::Backspace);
        let escape = key_label(Key::Escape);
        Some(match (&self.stroke, self.phase) {
            (None, _) => t!("curve.hint_idle").to_owned(),
            (Some(_), CurvePhase::Adjusting) => tf!("curve.hint_adjust", enter, escape),
            (Some(_), CurvePhase::Drawing) if self.is_ready() => tf!("curve.hint_ready", enter, back, escape),
            (Some(_), CurvePhase::Drawing) => tf!("curve.hint_points", back, escape),
        })
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        let had = self.stroke.is_some() || self.drag.is_some();
        self.stroke = None;
        self.phase = CurvePhase::Drawing;
        self.drag = None;
        self.hover = None;
        self.depth_hover = None;
        had
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        self.ensure_init(state);
        let v = value.trim();
        let flag = match v.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        let num = v.parse::<f64>().ok().filter(|x| x.is_finite());
        match key {
            "subdivisions" => match num.filter(|n| n.fract() == 0.0) {
                Some(n) if (f64::from(MIN_SUBDIVISIONS)..=f64::from(MAX_SUBDIVISIONS)).contains(&n) => {
                    self.set_subdivisions(n as i64);
                    true
                }
                _ => false,
            },
            "thickness" => match num {
                Some(t) if (MIN_THICKNESS..=MAX_THICKNESS).contains(&t) => {
                    self.set_thickness(t);
                    true
                }
                _ => false,
            },
            "height" => {
                if v.eq_ignore_ascii_case("auto") {
                    self.options.height = None;
                    return true;
                }
                match num {
                    Some(h) if (MIN_HEIGHT..=MAX_HEIGHT).contains(&h) => {
                        self.options.height = Some(h);
                        self.set_height(h);
                        true
                    }
                    _ => false,
                }
            }
            "tex_scale" => match num.filter(|x| (0.01..=64.0).contains(x)) {
                Some(x) => {
                    self.options.tex_scale = x;
                    true
                }
                None => false,
            },
            "keep_active" => flag.map(|b| self.options.keep_active = b).is_some(),
            "create_as" => {
                if v.eq_ignore_ascii_case("world") {
                    self.options.create_as = CreateAs::World;
                    true
                } else if !v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    self.options.create_as = CreateAs::Entity(v.to_ascii_lowercase());
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        use hp_app::tool::ToolOption;
        let o = &self.options;
        let num = |v: f64| {
            let s = format!("{v:.4}");
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        };
        vec![
            ToolOption::new(
                "subdivisions",
                o.subdivisions.to_string(),
                "number of brushes along the curve (2 to 4096)",
            ),
            ToolOption::new(
                "thickness",
                num(o.thickness),
                "width of the brushes across the curve in units (1 to 1024)",
            ),
            ToolOption::new(
                "height",
                o.height.map_or_else(|| "auto".to_owned(), num),
                "height of the brushes along the view depth in units (1 to 16384), or auto: from the selection like the block tool",
            ),
            ToolOption::new("tex_scale", num(o.tex_scale), "texture scale of the new faces (0.01 to 64)"),
            ToolOption::new(
                "create_as",
                match &o.create_as {
                    CreateAs::World => "world".to_owned(),
                    CreateAs::Entity(c) => c.clone(),
                },
                "world, or a brush entity class (func_detail...) holding every brush",
            ),
            ToolOption::new(
                "keep_active",
                if o.keep_active { "on" } else { "off" },
                "on: stay in the curve tool after creating",
            ),
        ]
    }
}
