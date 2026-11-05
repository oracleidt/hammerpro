use egui::{Align2, CursorIcon, Key, Pos2, Ui, Vec2};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::grid::GridFrame;
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_render::Overlay;

use super::apply;
use super::build::{moves_from_points, plan};
use super::model::{
    MAX_COLLAR_EVERY, MAX_RADIUS, MAX_SEGMENTS, MAX_SIDES, MAX_SLICES, MIN_SIDES, PipeError, PipePlan, PipeSpec, Shape,
    format_moves, parse_moves,
};
use crate::block::new_depth_range;
use crate::common::{
    COLOR_PENDING, COLOR_REMOVED, Mouse, Ov, Snap, active_material, consume_plain_key, depth_axis, fmt_len, fmt_num,
    fresh, material_chip, paint_label, pick_surface, rgba,
};

pub const HEADINGS: [&str; 6] = ["+x", "-x", "+y", "-y", "+z", "-z"];

#[derive(Clone, Debug, PartialEq)]
pub struct PipeOptions {
    pub spec: PipeSpec,
    pub path: String,
    pub heading: usize,
    pub depth: Option<f64>,
    pub keep_active: bool,
}

impl Default for PipeOptions {
    fn default() -> Self {
        Self { spec: PipeSpec::default(), path: String::new(), heading: 0, depth: None, keep_active: true }
    }
}

pub fn heading_vector(frame: &GridFrame, index: usize) -> DVec3 {
    let m = frame.axes;
    let axis = [m.x_axis, m.y_axis, m.z_axis][(index / 2).min(2)];
    if index.is_multiple_of(2) { axis } else { -axis }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hover {
    point: DVec3,
    frame: u64,
}

pub struct PipeTool {
    pub options: PipeOptions,
    kind: Option<ViewKind>,
    points: Vec<DVec3>,
    hover: Option<Hover>,
    initialized: bool,
}

impl Default for PipeTool {
    fn default() -> Self {
        Self::new()
    }
}

fn in_plane_axes(frame: &GridFrame, depth: usize) -> Vec<DVec3> {
    let m = frame.axes;
    let mut cols = vec![m.x_axis, m.y_axis, m.z_axis];
    cols.sort_by(|a, b| a[depth].abs().total_cmp(&b[depth].abs()));
    cols.truncate(2);
    cols
}

fn constrain(frame: &GridFrame, kind: ViewKind, last: DVec3, raw: DVec3, snap: bool) -> Option<DVec3> {
    let d = kind.axes_2d()?.2;
    let mut delta = raw - last;
    delta[d] = 0.0;
    let axes = in_plane_axes(frame, d);
    let (axis, along) = axes.iter().map(|a| (*a, a.dot(delta))).max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))?;
    let dir = if along >= 0.0 { axis } else { -axis };
    let mut len = along.abs();
    if snap {
        let m = frame.axes;
        let cols = [m.x_axis, m.y_axis, m.z_axis];
        let j = (0..3).max_by(|a, b| cols[*a].dot(dir).abs().total_cmp(&cols[*b].dot(dir).abs()))?;
        let step = frame.step[j];
        len = (len / step).round() * step;
    }
    (len > 1e-6).then(|| crate::common::clean_vec(last + dir * len))
}

impl PipeTool {
    pub fn new() -> Self {
        Self { options: PipeOptions::default(), kind: None, points: Vec::new(), hover: None, initialized: false }
    }

    pub fn points(&self) -> &[DVec3] {
        &self.points
    }

    fn ensure_init(&mut self, state: &AppState) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let s = state.settings.edit.default_texture_scale;
        if s.is_finite() && s.abs() > 1e-6 {
            self.options.spec.tex_scale = s;
        }
    }

    fn frame(&self, state: &AppState) -> GridFrame {
        state.grid_frame(self.kind.unwrap_or(ViewKind::Top))
    }

    pub fn spec_with(&self, state: &AppState, tentative: Option<DVec3>) -> Result<(PipeSpec, GridFrame), PipeError> {
        let frame = self.frame(state);
        let mut spec = self.options.spec.clone();
        spec.snap = state.snap_enabled();
        spec.material = active_material(state);
        let mut pts = self.points.clone();
        pts.extend(tentative);
        let typed = parse_moves(&self.options.path).map_err(PipeError::Invalid)?;
        match pts.len() {
            0 => return Err(PipeError::Invalid("click the start of the pipe".to_owned())),
            1 => {
                if typed.is_empty() {
                    return Err(PipeError::Invalid("click the next point, or type a route".to_owned()));
                }
                spec.start = pts[0];
                spec.direction = heading_vector(&frame, self.options.heading);
                spec.moves = typed;
            }
            _ => {
                let (dir, mut moves) = moves_from_points(&pts, &frame)?;
                moves.extend(typed);
                spec.start = pts[0];
                spec.direction = dir;
                spec.moves = moves;
            }
        }
        Ok((spec, frame))
    }

    pub fn current_plan(&self, state: &AppState, tentative: Option<DVec3>) -> Result<PipePlan, PipeError> {
        let (spec, frame) = self.spec_with(state, tentative)?;
        plan(&spec, &frame)
    }

    pub fn is_ready(&self, state: &AppState) -> bool {
        self.current_plan(state, None).is_ok()
    }

    fn hover_in(&self, state: &AppState) -> Option<Hover> {
        self.hover.filter(|h| fresh(state, h.frame))
    }

    pub fn remove_last_point(&mut self) -> bool {
        let had = self.points.pop().is_some();
        if self.points.is_empty() {
            self.kind = None;
        }
        had
    }

    pub fn create(&mut self, state: &mut AppState) -> bool {
        let made = self.current_plan(state, None);
        let plan = match made {
            Ok(p) => p,
            Err(e) => {
                state.notify(Severity::Warning, e.to_string());
                return false;
            }
        };
        let lightmap = match state.settings.edit.default_lightmap_scale {
            s if s > 0 => s,
            _ => 16,
        };
        match apply::create(&mut state.doc, &plan, lightmap, true) {
            Ok(done) => {
                state.log(Severity::Info, tf!("pipe.created", done.ids.len(), plan.bends, fmt_len(plan.length)));
                state.runtime.brush.last_created = Some(plan.bounds);
                self.points.clear();
                self.kind = None;
                self.hover = None;
                if !self.options.keep_active {
                    state.set_tool(ToolId::Select);
                }
                true
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("pipe.failed", e));
                false
            }
        }
    }

    fn view_point(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<DVec3> {
        let kind = view.kind;
        if !kind.is_2d() || self.kind.is_some_and(|k| k != kind) {
            return None;
        }
        let d = depth_axis(kind);
        let snap = Snap::new(state, view, egui::Modifiers::NONE);
        let p2 = snap.v2(view.screen_to_2d(pos));
        match self.points.last() {
            Some(last) => {
                let raw = kind.from_2d(p2, last[d]);
                constrain(&state.grid_frame(kind), kind, *last, raw, state.snap_enabled())
            }
            None => {
                let depth = self.options.depth.unwrap_or_else(|| {
                    let (lo, hi) = new_depth_range(state, state.runtime.brush.last_created, d);
                    snap.v((lo + hi) / 2.0, d)
                });
                Some(kind.from_2d(p2, depth))
            }
        }
    }

    fn start_in_3d(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<DVec3> {
        let hit = pick_surface(state, view, pos)?;
        let frame = state.grid_frame(ViewKind::Top);
        let p = if state.snap_enabled() { frame.snap_point(hit.point) } else { hit.point };
        Some(crate::common::clean_vec(p))
    }

    fn add_point(&mut self, state: &mut AppState, kind: Option<ViewKind>, p: DVec3) -> bool {
        if self.points.last().is_some_and(|l| l.distance_squared(p) < 1e-9) {
            return false;
        }
        if kind.is_some() && self.kind.is_none() {
            self.kind = kind;
        }
        self.points.push(p);
        if self.points.len() >= 2
            && let Err(e) = self.current_plan(state, None)
        {
            self.points.pop();
            if self.points.is_empty() {
                self.kind = None;
            }
            state.notify(Severity::Warning, e.to_string());
            return false;
        }
        true
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        self.ensure_init(state);
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        if !view.is_2d() {
            if m.clicked && self.points.is_empty() {
                if let Some(p) = self.start_in_3d(state, view, pos) {
                    self.add_point(state, None, p);
                }
                return ToolFlow::Handled;
            }
            return ToolFlow::Ignored;
        }
        if m.hovering() {
            self.hover = self.view_point(state, view, pos).map(|point| Hover { point, frame: state.frame.index });
            return ToolFlow::Ignored;
        }
        if m.double_clicked {
            if !self.points.is_empty() {
                self.create(state);
            }
            return ToolFlow::Handled;
        }
        if m.clicked || m.drag_started {
            let origin = m.origin().unwrap_or(pos);
            if let Some(p) = self.view_point(state, view, origin) {
                self.add_point(state, Some(view.kind), p);
            }
        }
        ToolFlow::Handled
    }
}

impl Tool for PipeTool {
    fn id(&self) -> ToolId {
        ToolId::Pipe
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
        if !self.points.is_empty() && self.is_ready(state) && consume_plain_key(ctx, Key::Enter) {
            self.create(state);
            return true;
        }
        if !self.points.is_empty() && consume_plain_key(ctx, Key::Backspace) {
            return self.remove_last_point();
        }
        if !self.points.is_empty() && consume_plain_key(ctx, Key::Escape) {
            return self.cancel(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let mut ov = Ov::new(out, view);
        let hover = self.hover_in(state).filter(|_| self.kind == Some(view.kind) || self.kind.is_none());
        let tentative = hover.map(|h| h.point);
        let line = rgba(COLOR_PENDING, 1.0);
        if self.points.is_empty() {
            if let Some(h) = hover {
                ov.point(h.point, line, 6.0);
            }
            return;
        }
        let shown = self.current_plan(state, tentative);
        let ok = shown.is_ok();
        let color = if ok { line } else { rgba(COLOR_REMOVED, 1.0) };
        match shown {
            Ok(p) => {
                for w in &p.wire {
                    ov.xray_line(w[0], w[1], rgba(COLOR_PENDING, 0.8));
                }
                for w in p.centerline.windows(2) {
                    ov.line(w[0], w[1], rgba(COLOR_PENDING, 0.5));
                }
            }
            Err(_) => {
                if let (Some(last), Some(h)) = (self.points.last(), tentative) {
                    ov.line(*last, h, color);
                }
            }
        }
        for w in self.points.windows(2) {
            ov.line(w[0], w[1], rgba(COLOR_PENDING, 0.5));
        }
        for p in &self.points {
            ov.point(*p, line, 7.0);
        }
        if let Some(h) = tentative {
            ov.point(h, color, 6.0);
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let Some(h) = self.hover_in(state).filter(|_| self.kind == Some(view.kind)) else { return };
        let Some(s) = view.world_to_screen(h.point) else { return };
        let at = s + Vec2::new(12.0, 12.0);
        match self.current_plan(state, Some(h.point)) {
            Ok(p) => {
                if let Some(last) = self.points.last() {
                    let text = tf!("pipe.label", fmt_len(last.distance(h.point)), p.bends, p.pieces.len());
                    paint_label(&painter, at, Align2::LEFT_TOP, &text, COLOR_PENDING);
                }
            }
            Err(e) => paint_label(&painter, at, Align2::LEFT_TOP, &e.to_string(), hp_app::theme::VIEW_ERROR),
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.ensure_init(state);
        let s = &mut self.options.spec;
        ui.horizontal_wrapped(|ui| {
            let mut shape = s.shape;
            egui::ComboBox::from_id_salt("pipe_shape")
                .selected_text(match shape {
                    Shape::Square => t!("pipe.shape_square"),
                    Shape::Round => t!("pipe.shape_round"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut shape, Shape::Square, t!("pipe.shape_square"));
                    ui.selectable_value(&mut shape, Shape::Round, t!("pipe.shape_round"));
                });
            s.shape = shape;
            if s.shape == Shape::Round {
                ui.label(t!("pipe.sides"));
                ui.add(egui::DragValue::new(&mut s.sides).range(MIN_SIDES..=MAX_SIDES));
            }
            ui.label(t!("pipe.radius")).on_hover_text(t!("pipe.radius_tip"));
            ui.add(egui::DragValue::new(&mut s.radius).speed(1.0).range(1.0..=MAX_RADIUS));
            ui.checkbox(&mut s.hollow, t!("pipe.hollow"));
            if s.hollow {
                ui.label(t!("pipe.wall"));
                ui.add(egui::DragValue::new(&mut s.wall).speed(0.5).range(0.5..=MAX_RADIUS));
            }
            ui.separator();
            ui.label(t!("pipe.segments")).on_hover_text(t!("pipe.segments_tip"));
            ui.add(egui::DragValue::new(&mut s.segments).range(1..=MAX_SEGMENTS));
            ui.label(t!("pipe.bend_radius")).on_hover_text(t!("pipe.bend_radius_tip"));
            ui.add(egui::DragValue::new(&mut s.bend_radius).speed(1.0).range(1.0..=MAX_RADIUS * 4.0));
            ui.label(t!("pipe.bend_slices"));
            ui.add(egui::DragValue::new(&mut s.bend_slices).range(1..=MAX_SLICES));
            ui.separator();
            ui.label(t!("pipe.collar_every")).on_hover_text(t!("pipe.collar_every_tip"));
            ui.add(egui::DragValue::new(&mut s.collar_every).range(0..=MAX_COLLAR_EVERY));
            ui.label(t!("pipe.collar_size"));
            ui.add(egui::DragValue::new(&mut s.collar_size).speed(0.5).range(0.0..=MAX_RADIUS));
            ui.label(t!("pipe.collar_length"));
            ui.add(egui::DragValue::new(&mut s.collar_length).speed(0.5).range(0.5..=MAX_RADIUS));
            ui.checkbox(&mut s.collar_ends, t!("pipe.collar_ends"));
            ui.checkbox(&mut s.collar_bends, t!("pipe.collar_bends"));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("pipe.path")).on_hover_text(t!("pipe.path_tip"));
            ui.add(
                egui::TextEdit::singleline(&mut self.options.path).desired_width(260.0).hint_text(t!("pipe.path_hint")),
            );
            if parse_moves(&self.options.path).is_err() {
                ui.label(egui::RichText::new(t!("pipe.path_bad")).color(hp_app::theme::VIEW_ERROR));
            }
            ui.label(t!("pipe.heading")).on_hover_text(t!("pipe.heading_tip"));
            egui::ComboBox::from_id_salt("pipe_heading").selected_text(HEADINGS[self.options.heading.min(5)]).show_ui(
                ui,
                |ui| {
                    for (i, h) in HEADINGS.iter().enumerate() {
                        ui.selectable_value(&mut self.options.heading, i, *h);
                    }
                },
            );
            ui.separator();
            ui.label(t!("create.block.tex_scale"));
            ui.add(
                egui::DragValue::new(&mut self.options.spec.tex_scale).speed(0.01).range(0.01..=64.0).max_decimals(3),
            );
            material_chip(ui, state);
            ui.separator();
            if hp_app::widgets::icon_toggle(ui, "📌", self.options.keep_active, t!("pipe.keep_active_tip")).clicked()
            {
                self.options.keep_active = !self.options.keep_active;
            }
            let count = self.points.len();
            if ui
                .add_enabled(count > 0, egui::Button::new(t!("pipe.undo_point")))
                .on_hover_text(tf!("pipe.undo_point_tip", key_label(Key::Backspace)))
                .clicked()
            {
                self.remove_last_point();
            }
            let ready = self.is_ready(state);
            if ui
                .add_enabled(ready, egui::Button::new(t!("pipe.create")))
                .on_hover_text(tf!("pipe.create_tip", key_label(Key::Enter)))
                .clicked()
            {
                self.create(state);
            }
            if count > 0 {
                ui.separator();
                ui.label(tf!("pipe.count", count));
                match self.current_plan(state, None) {
                    Ok(p) => {
                        ui.label(tf!("pipe.summary", p.pieces.len(), p.bends, fmt_len(p.length)));
                    }
                    Err(e) => {
                        ui.label(egui::RichText::new(e.to_string()).color(hp_app::theme::VIEW_ERROR));
                    }
                }
            }
        });
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        view.is_2d().then_some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        let back = key_label(Key::Backspace);
        let escape = key_label(Key::Escape);
        Some(if self.points.is_empty() {
            t!("pipe.hint_idle").to_owned()
        } else {
            tf!("pipe.hint_points", key_label(Key::Enter), back, escape)
        })
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        let had = !self.points.is_empty();
        self.points.clear();
        self.kind = None;
        self.hover = None;
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
        let int = |lo: u32, hi: u32| {
            num.filter(|n| n.fract() == 0.0 && *n >= f64::from(lo) && *n <= f64::from(hi)).map(|n| n as u32)
        };
        let size = |hi: f64| num.filter(|n| *n >= 0.0 && *n <= hi);
        let s = &mut self.options.spec;
        match key {
            "shape" => Shape::parse(v).map(|x| s.shape = x).is_some(),
            "sides" => int(MIN_SIDES, MAX_SIDES).map(|x| s.sides = x).is_some(),
            "radius" => num.filter(|n| (1.0..=MAX_RADIUS).contains(n)).map(|x| s.radius = x).is_some(),
            "hollow" => flag.map(|b| s.hollow = b).is_some(),
            "wall" => size(MAX_RADIUS).filter(|n| *n > 0.0).map(|x| s.wall = x).is_some(),
            "segments" => int(1, MAX_SEGMENTS).map(|x| s.segments = x).is_some(),
            "segment_length" => size(super::model::MAX_LENGTH).map(|x| s.segment_length = x).is_some(),
            "bend_radius" => size(MAX_RADIUS * 4.0).filter(|n| *n > 0.0).map(|x| s.bend_radius = x).is_some(),
            "bend_slices" => int(1, MAX_SLICES).map(|x| s.bend_slices = x).is_some(),
            "collar_every" => int(0, MAX_COLLAR_EVERY).map(|x| s.collar_every = x).is_some(),
            "collar_size" => size(MAX_RADIUS).map(|x| s.collar_size = x).is_some(),
            "collar_length" => size(MAX_RADIUS).filter(|n| *n > 0.0).map(|x| s.collar_length = x).is_some(),
            "collar_ends" => flag.map(|b| s.collar_ends = b).is_some(),
            "collar_bends" => flag.map(|b| s.collar_bends = b).is_some(),
            "tex_scale" => num.filter(|x| (0.01..=64.0).contains(x)).map(|x| s.tex_scale = x).is_some(),
            "keep_active" => flag.map(|b| self.options.keep_active = b).is_some(),
            "path" => parse_moves(v).map(|_| self.options.path = v.to_owned()).is_ok(),
            "heading" => HEADINGS.iter().position(|h| *h == v).map(|i| self.options.heading = i).is_some(),
            "depth" => {
                if v.eq_ignore_ascii_case("auto") {
                    self.options.depth = None;
                    return true;
                }
                num.map(|x| self.options.depth = Some(x)).is_some()
            }
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<ToolOption> {
        let s = &self.options.spec;
        let flag = |b: bool| if b { "on" } else { "off" }.to_owned();
        vec![
            ToolOption::new("shape", s.shape.name(), "square or round"),
            ToolOption::new("sides", s.sides.to_string(), "side count of a round profile (5 to 48)"),
            ToolOption::new(
                "radius",
                fmt_num(s.radius),
                "half width of the pipe in units (1 to 2048), snapped to the grid",
            ),
            ToolOption::new("hollow", flag(s.hollow), "on: a hollow tube made of wall brushes, off: solid brushes"),
            ToolOption::new("wall", fmt_num(s.wall), "wall thickness of a hollow pipe in units, snapped to the grid"),
            ToolOption::new("segments", s.segments.to_string(), "brushes along each straight run (1 to 256)"),
            ToolOption::new(
                "segment_length",
                fmt_num(s.segment_length),
                "length of a straight brush in units (0: use segments instead)",
            ),
            ToolOption::new(
                "bend_radius",
                fmt_num(s.bend_radius),
                "radius of the 90 degree bends at the centerline, snapped to the grid",
            ),
            ToolOption::new("bend_slices", s.bend_slices.to_string(), "brush slices per 90 degree bend (1 to 32)"),
            ToolOption::new("collar_every", s.collar_every.to_string(), "a collar every N brushes of a run (0: none)"),
            ToolOption::new(
                "collar_size",
                fmt_num(s.collar_size),
                "how far a collar sticks out of the pipe in units (0: no collars)",
            ),
            ToolOption::new("collar_length", fmt_num(s.collar_length), "length of a collar along the pipe in units"),
            ToolOption::new("collar_ends", flag(s.collar_ends), "a collar at both ends of the pipe"),
            ToolOption::new("collar_bends", flag(s.collar_bends), "a collar on both sides of every bend"),
            ToolOption::new("tex_scale", fmt_num(s.tex_scale), "texture scale of the new faces (0.01 to 64)"),
            ToolOption::new(
                "path",
                format_moves(&parse_moves(&self.options.path).unwrap_or_default()),
                "moves appended after the clicked points: lengths and turns (128 L 64 U 32 R D)",
            ),
            ToolOption::new(
                "heading",
                HEADINGS[self.options.heading.min(5)],
                "first direction of a route typed from one point: +x -x +y -y +z -z",
            ),
            ToolOption::new(
                "depth",
                self.options.depth.map_or_else(|| "auto".to_owned(), fmt_num),
                "plane of the first click in a 2D view in units, or auto",
            ),
            ToolOption::new("keep_active", flag(self.options.keep_active), "keep the pipe tool active after creating"),
        ]
    }
}
