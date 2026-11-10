use egui::{Align2, CursorIcon, Key, Pos2, Stroke, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::key_label;
use hp_app::state::AppState;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::{EditError, Owner};
use hp_geom::{Aabb, hull_solid};
use hp_render::Overlay;

use crate::block::{CreateAs, create_as_ui, new_depth_range};
use crate::common::{
    COLOR_PENDING, COLOR_REMOVED, Edge, HANDLE_TOLERANCE, MIN_EXTENT, Mouse, Ov, Snap, active_material,
    box_screen_rect, consume_plain_key, depth_axis, fgd_defaults, fmt_len, fmt_size, fresh, material_chip,
    paint_handle, paint_label, rgba,
};
use crate::picker::ClassPicker;

pub const MAX_POLYGON_POINTS: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub struct PolygonOptions {
    pub tex_scale: f64,
    pub create_as: CreateAs,
    pub keep_active: bool,
}

impl Default for PolygonOptions {
    fn default() -> Self {
        Self { tex_scale: hp_geom::DEFAULT_TEXTURE_SCALE, create_as: CreateAs::World, keep_active: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointCheck {
    Ok,
    Duplicate,
    Concave,
    Full,
    OtherView,
}

pub fn is_convex_outline(points: &[DVec2]) -> bool {
    let n = points.len();
    if n < 3 || points.iter().any(|p| !p.is_finite()) {
        return false;
    }
    let scale = points.iter().fold(1.0f64, |m, p| m.max(p.abs().max_element()));
    let eps = 1e-9 * scale * scale;
    let area2: f64 = (0..n).map(|i| points[i].perp_dot(points[(i + 1) % n])).sum();
    if area2.abs() <= eps {
        return false;
    }
    let sign = area2.signum();
    (0..n).all(|i| {
        let a = points[i];
        let e = points[(i + 1) % n] - a;
        (0..n).filter(|&j| j != i && j != (i + 1) % n).all(|j| e.perp_dot(points[j] - a) * sign > eps)
    })
}

pub fn check_point(points: &[DVec2], p: DVec2) -> PointCheck {
    if !p.is_finite() || points.iter().any(|q| q.distance_squared(p) < 1e-12) {
        return PointCheck::Duplicate;
    }
    if points.len() >= MAX_POLYGON_POINTS {
        return PointCheck::Full;
    }
    if points.len() < 2 {
        return PointCheck::Ok;
    }
    let mut candidate = points.to_vec();
    candidate.push(p);
    if is_convex_outline(&candidate) { PointCheck::Ok } else { PointCheck::Concave }
}

#[derive(Clone, Debug, PartialEq)]
struct Outline {
    kind: ViewKind,
    points: Vec<DVec2>,
    depth: (f64, f64),
}

impl Outline {
    fn prism_points(&self) -> Vec<DVec3> {
        self.points
            .iter()
            .flat_map(|p| [self.kind.from_2d(*p, self.depth.0), self.kind.from_2d(*p, self.depth.1)])
            .collect()
    }

    fn bounds(&self) -> Aabb {
        Aabb::from_points(self.prism_points())
    }

    fn ready(&self) -> bool {
        is_convex_outline(&self.points) && self.depth.1 - self.depth.0 > MIN_EXTENT
    }

    fn depth_axis(&self) -> usize {
        depth_axis(self.kind)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hover {
    kind: ViewKind,
    point: DVec2,
    check: PointCheck,
    frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Point { view: usize, index: usize },
    Depth { view: usize, end: Option<Edge>, start: (f64, f64), from: f64 },
    Swallow,
}

pub struct PolygonTool {
    pub options: PolygonOptions,
    outline: Option<Outline>,
    hover: Option<Hover>,
    drag: Option<Drag>,
    depth_hover: Option<(usize, Option<Edge>, u64)>,
    last_bounds: Option<Aabb>,
    initialized: bool,
    picker: ClassPicker,
}

impl Default for PolygonTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PolygonTool {
    pub fn new() -> Self {
        Self {
            options: PolygonOptions::default(),
            outline: None,
            hover: None,
            drag: None,
            depth_hover: None,
            last_bounds: None,
            initialized: false,
            picker: ClassPicker::default(),
        }
    }

    pub fn points(&self) -> &[DVec2] {
        self.outline.as_ref().map_or(&[], |o| &o.points)
    }

    pub fn depth(&self) -> Option<(f64, f64)> {
        self.outline.as_ref().map(|o| o.depth)
    }

    pub fn last_bounds(&self) -> Option<Aabb> {
        self.last_bounds
    }

    pub fn is_ready(&self) -> bool {
        self.outline.as_ref().is_some_and(Outline::ready)
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

    pub fn add_point(&mut self, state: &AppState, kind: ViewKind, p: DVec2) -> PointCheck {
        if !kind.is_2d() {
            return PointCheck::OtherView;
        }
        match &mut self.outline {
            Some(o) if o.kind == kind => {
                let check = check_point(&o.points, p);
                if check == PointCheck::Ok {
                    o.points.push(p);
                }
                check
            }
            Some(_) => PointCheck::OtherView,
            None => {
                if !p.is_finite() {
                    return PointCheck::Duplicate;
                }
                let last = state.runtime.brush.last_created.or(self.last_bounds);
                let depth = new_depth_range(state, last, depth_axis(kind));
                self.outline = Some(Outline { kind, points: vec![p], depth });
                PointCheck::Ok
            }
        }
    }

    pub fn remove_last_point(&mut self) -> bool {
        let Some(o) = self.outline.as_mut() else { return false };
        o.points.pop();
        if o.points.is_empty() {
            self.outline = None;
        }
        self.drag = None;
        true
    }

    pub fn set_depth(&mut self, lo: f64, hi: f64) {
        if let Some(o) = self.outline.as_mut()
            && lo.is_finite()
            && hi.is_finite()
            && (hi - lo).abs() > MIN_EXTENT
        {
            o.depth = (lo.min(hi), lo.max(hi));
        }
    }

    pub fn create(&mut self, state: &mut AppState) -> Option<usize> {
        let o = self.outline.as_ref()?;
        if !o.ready() {
            state.notify(Severity::Warning, t!("create.poly.not_ready"));
            return None;
        }
        let points = o.prism_points();
        let bounds = o.bounds();
        let material = active_material(state);
        let lightmap = match state.settings.edit.default_lightmap_scale {
            s if s > 0 => s,
            _ => 16,
        };
        let mut next = state.doc.peek_next_side_id();
        let sides = hull_solid(&points, &material, self.options.tex_scale, lightmap, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        });
        let result: Result<usize, EditError> = match (sides, &self.options.create_as) {
            (Err(e), _) => Err(EditError::from(e)),
            (Ok(sides), CreateAs::World) => state.doc.create_solids(vec![sides], Owner::World).map(|ids| ids.len()),
            (Ok(sides), CreateAs::Entity(class)) => {
                let kv = fgd_defaults(state, class);
                state.doc.create_brush_entity(class, &kv, vec![sides]).map(|_| 1)
            }
        };
        match result {
            Ok(n) => {
                state.log(Severity::Info, tf!("create.poly.created", o.points.len(), fmt_size(&bounds)));
                self.last_bounds = Some(bounds);
                state.runtime.brush.last_created = Some(bounds);
                self.outline = None;
                self.hover = None;
                self.drag = None;
                if !self.options.keep_active {
                    state.set_tool(ToolId::Select);
                }
                Some(n)
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("create.poly.failed", e));
                None
            }
        }
    }

    fn point_at(&self, view: &ViewInfo, pos: Pos2) -> Option<usize> {
        let o = self.outline.as_ref().filter(|o| o.kind == view.kind)?;
        o.points
            .iter()
            .enumerate()
            .filter_map(|(i, p)| view.world_to_screen(o.kind.from_2d(*p, 0.0)).map(|s| (i, s.distance(pos))))
            .filter(|(_, d)| *d <= HANDLE_TOLERANCE + 2.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn depth_layout(&self, view: &ViewInfo) -> Option<(usize, egui::Rect)> {
        let o = self.outline.as_ref()?;
        if !view.is_2d() || view.kind == o.kind || o.points.is_empty() {
            return None;
        }
        let (r, u, _) = view.kind.axes_2d()?;
        let d = o.depth_axis();
        let axis = if r == d {
            0
        } else if u == d {
            1
        } else {
            return None;
        };
        let mut b = o.bounds();
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

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        self.ensure_init(state);
        if !view.is_2d() {
            return ToolFlow::Ignored;
        }
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::new(state, view, m.modifiers);
        let drawing_view = self.outline.as_ref().is_none_or(|o| o.kind == view.kind);
        if m.hovering() {
            self.depth_hover = self.depth_handle_at(view, pos).map(|e| (view.index, e, state.frame.index));
            self.hover = drawing_view.then(|| {
                let point = snap.v2(view.screen_to_2d(pos));
                let check = self.outline.as_ref().map_or(PointCheck::Ok, |o| check_point(&o.points, point));
                Hover { kind: view.kind, point, check, frame: state.frame.index }
            });
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        if m.drag_started {
            self.drag = None;
            if drawing_view && let Some(index) = self.point_at(view, origin) {
                self.drag = Some(Drag::Point { view: view.index, index });
            } else if !drawing_view && let Some(end) = self.depth_handle_at(view, origin) {
                let o = self.outline.as_ref().map(|o| (o.depth, o.depth_axis()));
                if let Some((start, d)) = o {
                    let from = view.kind.from_2d(view.screen_to_2d(origin), 0.0)[d];
                    self.drag = Some(Drag::Depth { view: view.index, end, start, from });
                }
            }
        }
        if m.dragging || m.drag_stopped {
            match self.drag {
                Some(Drag::Point { view: vi, index }) if vi == view.index => {
                    let p = snap.v2(view.screen_to_2d(pos));
                    if let Some(o) = self.outline.as_mut() {
                        let mut moved = o.points.clone();
                        if let Some(slot) = moved.get_mut(index) {
                            *slot = p;
                        }
                        let ok = if moved.len() >= 3 {
                            is_convex_outline(&moved)
                        } else {
                            moved.len() < 2 || moved[0].distance_squared(moved[1]) > 1e-12
                        };
                        if ok {
                            o.points = moved;
                        }
                    }
                }
                Some(Drag::Depth { view: vi, end, start, from }) if vi == view.index => {
                    if let Some(o) = self.outline.as_mut() {
                        let d = o.depth_axis();
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
                            if next.1 - next.0 > MIN_EXTENT {
                                o.depth = next;
                            }
                        }
                    }
                    self.depth_hover = Some((view.index, end, state.frame.index));
                }
                _ => {}
            }
        }
        if m.activated() && drawing_view && self.drag.is_none() {
            let p = snap.v2(view.screen_to_2d(origin));
            if m.double_clicked {
                if self.is_ready() {
                    self.create(state);
                }
            } else {
                self.add_point(state, view.kind, p);
            }
            if m.drag_started {
                self.drag = Some(Drag::Swallow);
            }
        }
        if m.drag_stopped {
            self.drag = None;
        }
        ToolFlow::Handled
    }

    fn prism_edges(o: &Outline) -> Vec<[DVec3; 2]> {
        let n = o.points.len();
        let mut out = Vec::with_capacity(n * 3);
        let closed = n >= 3;
        for i in 0..n {
            let a = o.points[i];
            out.push([o.kind.from_2d(a, o.depth.0), o.kind.from_2d(a, o.depth.1)]);
            if i + 1 < n || closed {
                let b = o.points[(i + 1) % n];
                out.push([o.kind.from_2d(a, o.depth.0), o.kind.from_2d(b, o.depth.0)]);
                out.push([o.kind.from_2d(a, o.depth.1), o.kind.from_2d(b, o.depth.1)]);
            }
        }
        out
    }

    fn hover_in(&self, state: &AppState, kind: ViewKind) -> Option<Hover> {
        self.hover.filter(|h| h.kind == kind && fresh(state, h.frame) && self.drag.is_none())
    }
}

impl Tool for PolygonTool {
    fn id(&self) -> ToolId {
        ToolId::Polygon
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
        if self.is_ready() && consume_plain_key(ctx, Key::Enter) {
            self.create(state);
            return true;
        }
        if self.outline.is_some() && consume_plain_key(ctx, Key::Backspace) {
            return self.remove_last_point();
        }
        if self.outline.is_some() && consume_plain_key(ctx, Key::Escape) {
            return self.cancel(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let mut ov = Ov::new(out, view);
        let line = rgba(COLOR_PENDING, 1.0);
        let Some(o) = &self.outline else {
            if let Some(h) = self.hover_in(state, view.kind) {
                ov.point(view.kind.from_2d(h.point, 0.0), line, 6.0);
            }
            return;
        };
        if view.is_2d() && view.kind == o.kind {
            let n = o.points.len();
            for w in o.points.windows(2) {
                ov.line(o.kind.from_2d(w[0], 0.0), o.kind.from_2d(w[1], 0.0), line);
            }
            if n >= 3 {
                ov.line(
                    o.kind.from_2d(o.points[n - 1], 0.0),
                    o.kind.from_2d(o.points[0], 0.0),
                    rgba(COLOR_PENDING, 0.45),
                );
            }
            for p in &o.points {
                ov.point(o.kind.from_2d(*p, 0.0), line, 7.0);
            }
            if let Some(h) = self.hover_in(state, view.kind)
                && let Some(last) = o.points.last()
            {
                let color = match h.check {
                    PointCheck::Concave | PointCheck::Full => rgba(COLOR_REMOVED, 1.0),
                    _ => rgba(COLOR_PENDING, 0.7),
                };
                let hp = o.kind.from_2d(h.point, 0.0);
                ov.line(o.kind.from_2d(*last, 0.0), hp, color);
                if n >= 2 && h.check == PointCheck::Ok {
                    ov.line(hp, o.kind.from_2d(o.points[0], 0.0), rgba(COLOR_PENDING, 0.35));
                }
                ov.point(hp, color, if h.check == PointCheck::Ok { 6.0 } else { 9.0 });
            }
            return;
        }
        let edges = Self::prism_edges(o);
        for e in &edges {
            ov.xray_line(e[0], e[1], line);
        }
        if !view.is_2d() && o.points.len() >= 3 {
            let fill = rgba(COLOR_PENDING, 0.10);
            let bottom: Vec<DVec3> = o.points.iter().map(|p| o.kind.from_2d(*p, o.depth.0)).collect();
            let top: Vec<DVec3> = o.points.iter().map(|p| o.kind.from_2d(*p, o.depth.1)).collect();
            ov.polygon_fill(&bottom, fill);
            ov.polygon_fill(&top, fill);
            let n = o.points.len();
            for i in 0..n {
                let j = (i + 1) % n;
                ov.polygon_fill(&[bottom[i], bottom[j], top[j], top[i]], fill);
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let Some(o) = &self.outline else { return };
        if !view.is_2d() {
            let b = o.bounds();
            if o.points.len() >= 3
                && let Some(s) = view.world_to_screen(b.center())
                && view.rect.contains(s)
            {
                paint_label(&painter, s - Vec2::new(0.0, 8.0), Align2::CENTER_BOTTOM, &fmt_size(&b), COLOR_PENDING);
            }
            return;
        }
        if view.kind == o.kind {
            if let Some(h) = self.hover_in(state, view.kind)
                && let Some(s) = view.world_to_screen(o.kind.from_2d(h.point, 0.0))
            {
                let at = s + Vec2::new(12.0, 12.0);
                match h.check {
                    PointCheck::Concave => {
                        paint_label(
                            &painter,
                            at,
                            Align2::LEFT_TOP,
                            t!("create.poly.concave"),
                            hp_app::theme::VIEW_ERROR,
                        );
                    }
                    PointCheck::Full => {
                        paint_label(&painter, at, Align2::LEFT_TOP, t!("create.poly.full"), hp_app::theme::VIEW_ERROR);
                    }
                    PointCheck::Ok => {
                        if let Some(last) = o.points.last() {
                            paint_label(
                                &painter,
                                at,
                                Align2::LEFT_TOP,
                                &fmt_len(last.distance(h.point)),
                                COLOR_PENDING,
                            );
                        }
                    }
                    PointCheck::Duplicate | PointCheck::OtherView => {}
                }
            }
            return;
        }
        let Some((axis, rect)) = self.depth_layout(view) else { return };
        let size = o.depth.1 - o.depth.0;
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
        if !o.ready() && o.points.len() >= 3 {
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, COLOR_REMOVED), egui::StrokeKind::Outside);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.ensure_init(state);
        ui.horizontal_wrapped(|ui| {
            let scale =
                egui::DragValue::new(&mut self.options.tex_scale).speed(0.01).range(0.01..=64.0).max_decimals(3);
            let tip = t!("create.block.tex_scale_tip");
            ui.label(t!("create.block.tex_scale")).on_hover_text(tip);
            ui.add(scale).on_hover_text(tip);
            ui.separator();
            create_as_ui(ui, state, &mut self.picker, &mut self.options.create_as, "polygon_create_as");
            ui.separator();
            material_chip(ui, state);
            ui.separator();
            if hp_app::widgets::icon_toggle(ui, "📌", self.options.keep_active, t!("create.poly.keep_active_tip"))
                .clicked()
            {
                self.options.keep_active = !self.options.keep_active;
            }
            let count = self.points().len();
            if ui
                .add_enabled(count > 0, egui::Button::new(t!("create.poly.undo_point")))
                .on_hover_text(tf!("create.poly.undo_point_tip", key_label(Key::Backspace)))
                .clicked()
            {
                self.remove_last_point();
            }
            if ui
                .add_enabled(self.is_ready(), egui::Button::new(t!("create.block.create")))
                .on_hover_text(tf!("create.poly.create_tip", key_label(Key::Enter)))
                .clicked()
            {
                self.create(state);
            }
            if count > 0 {
                ui.separator();
                ui.label(tf!("create.poly.count", count));
            }
        });
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

    fn set_option(&mut self, _state: &mut AppState, key: &str, value: &str) -> bool {
        let o = &mut self.options;
        match key {
            "tex_scale" => value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| (0.01..=64.0).contains(v))
                .map(|v| o.tex_scale = v)
                .is_some(),
            "create_as" => crate::common::parse_create_as(value).map(|c| o.create_as = c).is_some(),
            "keep_active" => crate::common::option_flag(value).map(|b| o.keep_active = b).is_some(),
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        use hp_app::tool::ToolOption;
        let o = &self.options;
        vec![
            ToolOption::new("tex_scale", format!("{}", o.tex_scale), "texture scale of the new faces (0.01 to 64)"),
            ToolOption::new(
                "create_as",
                crate::common::create_as_text(&o.create_as),
                "world, or a brush entity class (func_detail, trigger_multiple...)",
            ),
            ToolOption::new(
                "keep_active",
                crate::common::on_off(o.keep_active),
                "on: stay in the polygon tool after creating",
            ),
        ]
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        let back = key_label(Key::Backspace);
        let escape = key_label(Key::Escape);
        Some(match &self.outline {
            None => t!("create.poly.hint_idle").to_owned(),
            Some(_) if self.is_ready() => tf!("create.poly.hint_ready", key_label(Key::Enter), back, escape),
            Some(o) => tf!("create.poly.hint_points", 3usize.saturating_sub(o.points.len()).max(1), back, escape),
        })
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        let had = self.outline.is_some() || self.drag.is_some();
        self.outline = None;
        self.drag = None;
        self.hover = None;
        self.depth_hover = None;
        had
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{info_2d, info_3d, settle};

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    #[test]
    fn convexity_rules() {
        let square = [v(0.0, 0.0), v(64.0, 0.0), v(64.0, 64.0), v(0.0, 64.0)];
        assert!(is_convex_outline(&square));
        let mut cw = square;
        cw.reverse();
        assert!(is_convex_outline(&cw), "either winding");
        assert!(!is_convex_outline(&square[..2]));
        assert!(!is_convex_outline(&[v(0.0, 0.0), v(32.0, 0.0), v(64.0, 0.0)]));
        assert!(!is_convex_outline(&[v(0.0, 0.0), v(32.0, 0.0), v(64.0, 0.0), v(32.0, 32.0)]));
        assert!(!is_convex_outline(&[v(0.0, 0.0), v(64.0, 0.0), v(32.0, 16.0), v(64.0, 64.0), v(0.0, 64.0)]));
        assert!(!is_convex_outline(&[v(0.0, 0.0), v(64.0, 64.0), v(64.0, 0.0), v(0.0, 64.0)]));
        let star: Vec<DVec2> =
            (0..5).map(|k| DVec2::from_angle(f64::from(k * 2) * std::f64::consts::TAU / 5.0) * 100.0).collect();
        assert!(!is_convex_outline(&star), "a pentagram turns the same way at every corner");
        assert!(!is_convex_outline(&[v(0.0, 0.0), v(f64::NAN, 0.0), v(0.0, 1.0)]));

        assert_eq!(check_point(&[], v(1.0, 1.0)), PointCheck::Ok);
        assert_eq!(check_point(&square[..1], square[0]), PointCheck::Duplicate);
        assert_eq!(check_point(&square[..2], v(128.0, 0.0)), PointCheck::Concave, "collinear");
        assert_eq!(check_point(&square[..3], v(0.0, 64.0)), PointCheck::Ok);
        assert_eq!(check_point(&square[..3], v(48.0, 16.0)), PointCheck::Concave, "inside");
        assert_eq!(check_point(&square, v(-32.0, 32.0)), PointCheck::Ok);
        assert_eq!(check_point(&square, v(32.0, 32.0)), PointCheck::Concave);
        assert_eq!(check_point(&square[..1], v(f64::INFINITY, 0.0)), PointCheck::Duplicate);
        let full: Vec<DVec2> = (0..MAX_POLYGON_POINTS)
            .map(|k| DVec2::from_angle(k as f64 * std::f64::consts::TAU / MAX_POLYGON_POINTS as f64) * 1000.0)
            .collect();
        assert!(is_convex_outline(&full));
        assert_eq!(check_point(&full, v(5000.0, 5000.0)), PointCheck::Full);
    }

    fn click(tool: &mut PolygonTool, state: &mut AppState, view: &ViewInfo, p: DVec3) {
        let s = view.world_to_screen(p).expect("screen");
        tool.handle_mouse(state, view, &Mouse::click(s));
    }

    #[test]
    fn draw_and_create_in_the_top_view() {
        let mut state = settle();
        state.doc.clear_selection();
        let mut tool = PolygonTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        for p in [(1.0, -2.0), (63.0, 1.0), (97.0, 47.0), (33.0, 81.0), (-15.0, 41.0)] {
            click(&mut tool, &mut state, &top, DVec3::new(p.0, p.1, 0.0));
        }
        assert_eq!(tool.points(), [v(0.0, 0.0), v(64.0, 0.0), v(96.0, 48.0), v(32.0, 80.0), v(-16.0, 48.0)]);
        click(&mut tool, &mut state, &top, DVec3::new(32.0, 32.0, 0.0));
        assert_eq!(tool.points().len(), 5);
        assert!(tool.is_ready());
        assert_eq!(tool.depth(), Some((0.0, 64.0)), "default depth from 0");
        let before = state.doc.stats().solids;
        let (undo, _) = state.doc.history_len();
        let last = top.world_to_screen(DVec3::new(-16.0, 48.0, 0.0)).expect("last");
        tool.handle_mouse(&mut state, &top, &Mouse::double_click(last));
        assert_eq!(state.doc.stats().solids, before + 1);
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
        assert!(tool.points().is_empty());
        let ids = state.doc.selected_solid_ids();
        assert_eq!(ids.len(), 1);
        let node = state.doc.scene().solid(ids[0]).expect("solid");
        assert!(node.geom.valid);
        assert_eq!(node.geom.faces.len(), 7);
        assert_eq!(node.bounds, Aabb::new(DVec3::new(-16.0, 0.0, 0.0), DVec3::new(96.0, 80.0, 64.0)));
        assert!(node.solid.sides.iter().all(|s| s.material == "dev/dev_measuregeneric01b"));
        assert_eq!(state.runtime.brush.last_created, Some(node.bounds));
        click(&mut tool, &mut state, &top, DVec3::new(200.0, 0.0, 0.0));
        assert_eq!(tool.depth(), Some((0.0, 64.0)));
    }

    #[test]
    fn depth_from_the_selection() {
        let mut state = settle();
        state.runtime.brush.last_created = Some(Aabb::new(DVec3::ZERO, DVec3::splat(64.0)));
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(2)]);
        let mut tool = PolygonTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        click(&mut tool, &mut state, &top, DVec3::new(0.0, 0.0, 0.0));
        assert_eq!(tool.depth(), Some((-16.0, 0.0)));
    }

    #[test]
    fn depth_from_the_front_view_and_editing() {
        let mut state = settle();
        state.doc.clear_selection();
        let mut tool = PolygonTool::new();
        let front = info_2d(ViewKind::Front, 1.0);
        for p in [(0.0, 0.0), (64.0, 0.0), (32.0, 64.0)] {
            click(&mut tool, &mut state, &front, DVec3::new(0.0, p.0, p.1));
        }
        assert_eq!(tool.points().len(), 3);
        assert!(tool.remove_last_point());
        assert_eq!(tool.points().len(), 2);
        click(&mut tool, &mut state, &front, DVec3::new(0.0, 32.0, 64.0));
        let top = info_2d(ViewKind::Top, 1.0);
        let from = top.world_to_screen(DVec3::new(64.0, 32.0, 0.0)).expect("from");
        let to = top.world_to_screen(DVec3::new(130.0, 32.0, 0.0)).expect("to");
        tool.handle_mouse(&mut state, &top, &Mouse::hover(from));
        assert_eq!(tool.cursor(&state, &top, from), Some(CursorIcon::ResizeHorizontal));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(from, from));
        tool.handle_mouse(&mut state, &top, &Mouse::drag(from, to));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(from, to));
        assert_eq!(tool.depth(), Some((0.0, 128.0)));
        assert_eq!(tool.points().len(), 3, "a drag in another view adds no point");
        let p = front.world_to_screen(DVec3::new(0.0, 32.0, 64.0)).expect("p");
        let q = front.world_to_screen(DVec3::new(0.0, 32.0, 96.0)).expect("q");
        tool.handle_mouse(&mut state, &front, &Mouse::drag_start(p, p));
        tool.handle_mouse(&mut state, &front, &Mouse::drag_end(p, q));
        assert_eq!(tool.points()[2], v(32.0, 96.0));
        let r = front.world_to_screen(DVec3::new(0.0, 32.0, 0.0)).expect("r");
        tool.handle_mouse(&mut state, &front, &Mouse::drag_start(q, q));
        tool.handle_mouse(&mut state, &front, &Mouse::drag_end(q, r));
        assert_eq!(tool.points()[2], v(32.0, 96.0));
        let n = tool.create(&mut state).expect("created");
        assert_eq!(n, 1);
        let id = state.doc.selected_solid_ids()[0];
        let b = state.doc.scene().solid(id).expect("solid").bounds;
        assert_eq!(b, Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 96.0)));
    }

    #[test]
    fn create_as_entity_keys_and_cancel() {
        let mut state = crate::tests_support::fgd_state();
        let mut tool = PolygonTool::new();
        tool.options.create_as = CreateAs::Entity("trigger_multiple".to_owned());
        tool.options.keep_active = false;
        let side = info_2d(ViewKind::Side, 1.0);
        assert!(tool.status_hint(&state).is_some());
        for p in [(0.0, 0.0), (64.0, 0.0), (64.0, 64.0)] {
            click(&mut tool, &mut state, &side, DVec3::new(p.0, 0.0, p.1));
        }
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        click(&mut tool, &mut state, &info_2d(ViewKind::Top, 1.0), DVec3::new(500.0, 500.0, 0.0));
        assert_eq!(tool.points().len(), 3);
        let view3d = info_3d(DVec3::new(0.0, -300.0, 64.0), 90.0, 0.0);
        let s = view3d.rect.center();
        assert_eq!(tool.handle_mouse(&mut state, &view3d, &Mouse::click(s)), ToolFlow::Ignored);
        let entities = state.doc.stats().entities;
        tool.create(&mut state).expect("created");
        assert_eq!(state.doc.stats().entities, entities + 1);
        let e = state.doc.selected_entity_ids()[0];
        let ent = &state.doc.scene().entity(e).expect("entity").entity;
        assert_eq!(ent.classname, "trigger_multiple");
        assert_eq!(ent.get("wait"), Some("1"), "FGD defaults");
        assert_eq!(state.requested_tool, Some(ToolId::Select));
        click(&mut tool, &mut state, &side, DVec3::new(0.0, 0.0, 0.0));
        assert!(tool.cancel(&mut state));
        assert!(!tool.cancel(&mut state));
        assert!(tool.create(&mut state).is_none());
    }

    #[test]
    fn overlay_and_paint_in_every_view() {
        let mut state = settle();
        let mut tool = PolygonTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        for p in [(0.0, 0.0), (64.0, 0.0), (32.0, 64.0)] {
            click(&mut tool, &mut state, &top, DVec3::new(p.0, p.1, 0.0));
        }
        let hover = top.world_to_screen(DVec3::new(32.0, 16.0, 0.0)).expect("hover");
        tool.handle_mouse(&mut state, &top, &Mouse::hover(hover));
        let views = [
            top,
            info_2d(ViewKind::Front, 1.0),
            info_2d(ViewKind::Side, 1.0),
            info_3d(DVec3::new(0.0, -300.0, 64.0), 90.0, 0.0),
        ];
        for view in &views {
            let mut out = Overlay::default();
            tool.overlay(&state, view, &mut out);
            assert!(!out.lines_on_top.is_empty() || !out.lines.is_empty(), "{:?}", view.kind);
        }
        let mut out = Overlay::default();
        tool.overlay(&state, &top, &mut out);
        assert!(out.points.iter().any(|p| p.color == rgba(COLOR_REMOVED, 1.0)));
        hp_app::testing::run_ctx(&mut state, |ctx, state| {
            for view in &views {
                tool.paint(state, view, &ctx.layer_painter(egui::LayerId::background()));
            }
        });
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
    }
}
