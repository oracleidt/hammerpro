use std::collections::HashMap;

use egui::{Align2, Color32, FontId, Key, Modifiers, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use hp_app::theme;
use hp_shader::catalog::{CATEGORIES, NODES, NodeDef, PinTy, node_def};
use hp_shader::{Diagnostic, Graph, Link, Node, Probe, Severity};

use super::labels;

pub const NODE_W: f32 = 204.0;
const HEADER: f32 = 26.0;
const ROW: f32 = 22.0;
const THUMB: f32 = 72.0;
const SUMMARY: f32 = 18.0;
const PAD: f32 = 8.0;
const PIN_R: f32 = 5.0;
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 2.5;
const SNAP: f32 = 8.0;

pub struct View<'a> {
    pub thumbs: &'a HashMap<u32, egui::TextureHandle>,
    pub show_thumbs: bool,
    pub diagnostics: &'a [Diagnostic],
    pub probe: Option<&'a Probe>,
}

#[derive(Clone, Debug)]
struct Wire {
    node: u32,
    pin: String,
    output: bool,
    reconnecting: bool,
}

struct Library {
    anchor: Pos2,
    graph_pos: Vec2,
    query: String,
    index: usize,
    connect: Option<Wire>,
    fresh: bool,
}

#[derive(Clone, Default)]
struct Clip {
    nodes: Vec<Node>,
    links: Vec<Link>,
}

pub struct Canvas {
    pub pan: Vec2,
    pub zoom: f32,
    pub selected: Vec<u32>,
    pub preview: Option<(u32, String)>,
    pub want_fit: bool,
    pub moved: bool,
    link_sel: Option<(u32, String)>,
    wire: Option<Wire>,
    marquee: Option<Pos2>,
    library: Option<Library>,
    clip: Clip,
    right_travel: f32,
    rect: Rect,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            pan: vec2(60.0, 60.0),
            zoom: 1.0,
            selected: Vec::new(),
            preview: None,
            want_fit: true,
            moved: false,
            link_sel: None,
            wire: None,
            marquee: None,
            library: None,
            clip: Clip::default(),
            right_travel: 0.0,
            rect: Rect::NOTHING,
        }
    }
}

#[derive(Clone, Copy)]
struct Geo {
    size: Vec2,
    summary_y: Option<f32>,
    thumb_y: Option<f32>,
}

fn summary_text(graph: &Graph, node: &Node, def: &NodeDef) -> Option<String> {
    let opt = |name: &str| match graph.param_value(node.id, name) {
        Some(serde_json::Value::String(s)) => Some(labels::option(&s).to_owned()),
        _ => None,
    };
    match def.id {
        "texture" => {
            let src = match graph.param_value(node.id, "source") {
                Some(serde_json::Value::String(s)) => s,
                _ => String::new(),
            };
            let leaf = src.rsplit(['/', '\\']).next().unwrap_or("").to_owned();
            Some(if leaf.is_empty() { hp_app::t!("shader.no_image").to_owned() } else { leaf })
        }
        "blend" | "grayscale" | "gradient_map" => opt("mode"),
        "noise" | "gradient" => opt("kind"),
        "height_to_normal" => opt("format"),
        "output" => {
            let target = opt("target").unwrap_or_default();
            let suffix = graph.output_suffix(node.id);
            Some(if suffix.is_empty() { target } else { format!("{target}  {suffix}") })
        }
        _ => None,
    }
}

fn geometry(graph: &Graph, node: &Node, def: &NodeDef, thumbs: bool) -> Geo {
    let rows = def.inputs.len().max(def.outputs.len()).max(1) as f32;
    let mut h = HEADER + rows * ROW;
    let summary_y = summary_text(graph, node, def).map(|_| {
        let y = h;
        h += SUMMARY;
        y
    });
    let thumb_y = (thumbs && (!def.outputs.is_empty() || def.is_output())).then(|| {
        let y = h + 2.0;
        h += THUMB + PAD + 2.0;
        y
    });
    Geo { size: vec2(NODE_W, h + 6.0), summary_y, thumb_y }
}

fn category_color(cat: &str) -> Color32 {
    match cat {
        "input" => Color32::from_gray(74),
        "uv" => Color32::from_gray(92),
        "math" => Color32::from_gray(58),
        "color" => Color32::from_gray(110),
        "filter" => Color32::from_gray(128),
        "procedural" => Color32::from_gray(84),
        "channels" => Color32::from_gray(100),
        _ => Color32::from_gray(66),
    }
}

pub fn pin_color(ty: PinTy) -> Color32 {
    match ty {
        PinTy::Color => Color32::from_gray(235),
        PinTy::Float => Color32::from_gray(180),
        PinTy::Uv => Color32::from_gray(135),
    }
}

fn bezier(a: Pos2, b: Pos2, zoom: f32) -> [Pos2; 4] {
    let d = ((b.x - a.x).abs() * 0.5).max(46.0 * zoom);
    [a, a + vec2(d, 0.0), b - vec2(d, 0.0), b]
}

fn bezier_at(p: &[Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    pos2(a * p[0].x + b * p[1].x + c * p[2].x + d * p[3].x, a * p[0].y + b * p[1].y + c * p[2].y + d * p[3].y)
}

fn wire_distance(p: &[Pos2; 4], q: Pos2) -> f32 {
    let mut best = f32::MAX;
    for i in 0..=24 {
        best = best.min(bezier_at(p, i as f32 / 24.0).distance(q));
    }
    best
}

fn draw_wire(painter: &Painter, a: Pos2, b: Pos2, zoom: f32, color: Color32, width: f32) {
    let p = bezier(a, b, zoom);
    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
        p,
        false,
        Color32::TRANSPARENT,
        Stroke::new(width * zoom.clamp(0.6, 1.4), color),
    ));
}

struct Placed {
    id: u32,
    def: &'static NodeDef,
    rect: Rect,
    geo: Geo,
    visible: bool,
}

impl Placed {
    fn pin(&self, index: usize, output: bool, zoom: f32) -> Pos2 {
        let x = if output { self.rect.right() } else { self.rect.left() };
        pos2(x, self.rect.top() + (HEADER + ROW * (index as f32 + 0.5)) * zoom)
    }
}

enum Edit {
    Delete(Vec<u32>),
    Duplicate,
    Disconnect(u32),
    Preview(u32, String),
    Select(u32, bool),
    OpenLibrary(Pos2, Option<Wire>),
}

impl Canvas {
    pub fn to_screen(&self, rect: Rect, p: [f32; 2]) -> Pos2 {
        rect.min + self.pan + vec2(p[0], p[1]) * self.zoom
    }

    pub fn to_graph(&self, rect: Rect, p: Pos2) -> Vec2 {
        (p - rect.min - self.pan) / self.zoom
    }

    pub fn fit(&mut self, rect: Rect, graph: &Graph, only_selected: bool) {
        let nodes: Vec<&Node> =
            graph.nodes.iter().filter(|n| !only_selected || self.selected.contains(&n.id)).collect();
        if nodes.is_empty() {
            self.zoom = 1.0;
            self.pan = vec2(60.0, 60.0);
            return;
        }
        let (mut lo, mut hi) = (vec2(f32::MAX, f32::MAX), vec2(f32::MIN, f32::MIN));
        for n in nodes {
            let h = node_def(&n.kind).map_or(80.0, |d| geometry(graph, n, d, true).size.y);
            lo = lo.min(vec2(n.pos[0], n.pos[1]));
            hi = hi.max(vec2(n.pos[0] + NODE_W, n.pos[1] + h));
        }
        let size = (hi - lo).max(vec2(40.0, 40.0));
        let avail = (rect.size() - vec2(80.0, 80.0)).max(vec2(80.0, 80.0));
        self.zoom = (avail.x / size.x).min(avail.y / size.y).clamp(MIN_ZOOM, 1.25);
        self.pan = (rect.size() - size * self.zoom) * 0.5 - lo * self.zoom;
    }

    pub fn set_zoom(&mut self, rect: Rect, factor: f32, around: Pos2) {
        let new = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let k = new / self.zoom;
        let local = around - rect.min;
        self.pan = local - (local - self.pan) * k;
        self.zoom = new;
    }

    pub fn open_library_center(&mut self) {
        let rect =
            if self.rect.is_positive() { self.rect } else { Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0)) };
        let anchor = rect.center() - vec2(150.0, 180.0);
        self.open_library(rect, anchor);
    }

    pub fn open_library(&mut self, rect: Rect, anchor: Pos2) {
        let gp = self.to_graph(rect, anchor);
        self.library =
            Some(Library { anchor, graph_pos: gp, query: String::new(), index: 0, connect: None, fresh: true });
    }

    #[cfg(test)]
    pub fn library_open(&self) -> bool {
        self.library.is_some()
    }

    pub fn show(&mut self, ui: &mut Ui, rect: Rect, graph: &mut Graph, view: &View<'_>) {
        self.rect = rect;
        if self.want_fit {
            self.want_fit = false;
            self.fit(rect, graph, false);
        }
        let painter = ui.painter_at(rect);
        let pal = theme::palette();
        painter.rect_filled(rect, 0.0, pal.bg1);
        self.draw_grid(&painter, rect);

        let bg = ui.interact(rect, ui.scope_id().with("hp_shader_canvas_bg"), Sense::click_and_drag());
        let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
        let library_open = self.library.is_some();
        if let Some(h) = hover
            && !library_open
        {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            let pinch = ui.input(|i| i.zoom_delta());
            let factor = if (pinch - 1.0).abs() > 1e-4 { pinch } else { (scroll * 0.0016).exp() };
            if (factor - 1.0).abs() > 1e-4 {
                self.set_zoom(rect, factor, h);
            }
        }
        let panning = bg.dragged_by(egui::PointerButton::Middle) || bg.dragged_by(egui::PointerButton::Secondary);
        if panning {
            let d = bg.drag_delta();
            self.pan += d;
            self.right_travel += d.length();
        }
        if bg.drag_started_by(egui::PointerButton::Secondary) {
            self.right_travel = 0.0;
        }

        let mut placed: Vec<Placed> = Vec::new();
        for n in &graph.nodes {
            let Some(def) = node_def(&n.kind) else { continue };
            let geo = geometry(graph, n, def, view.show_thumbs);
            let min = self.to_screen(rect, n.pos);
            let r = Rect::from_min_size(min, geo.size * self.zoom);
            let visible = r.intersects(rect.expand(40.0));
            placed.push(Placed { id: n.id, def, rect: r, geo, visible });
        }
        let by_id: HashMap<u32, usize> = placed.iter().enumerate().map(|(i, p)| (p.id, i)).collect();

        let mut wire_hit: Option<(u32, String)> = None;
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut best = 7.0f32;
        for l in &graph.links {
            let (Some(&a), Some(&b)) = (by_id.get(&l.from), by_id.get(&l.to)) else { continue };
            let (pa, pb) = (&placed[a], &placed[b]);
            let (Some(oi), Some(ii)) = (
                pa.def.outputs.iter().position(|o| o.name == l.out),
                pb.def.inputs.iter().position(|o| o.name == l.input),
            ) else {
                continue;
            };
            let (p1, p2) = (pa.pin(oi, true, self.zoom), pb.pin(ii, false, self.zoom));
            let ty = pa.def.outputs[oi].ty;
            let selected = self.link_sel.as_ref().is_some_and(|(t, i)| *t == l.to && *i == l.input);
            let on_sel = self.selected.contains(&l.from) || self.selected.contains(&l.to);
            let mut hovered = false;
            if let Some(p) = pointer
                && hover.is_some()
                && !library_open
                && self.wire.is_none()
            {
                let d = wire_distance(&bezier(p1, p2, self.zoom), p);
                if d < best {
                    best = d;
                    wire_hit = Some((l.to, l.input.clone()));
                }
                hovered = d < 7.0;
            }
            let base = pin_color(ty);
            let (color, width) = if selected {
                (theme::accent(), 3.5)
            } else if hovered || on_sel {
                (base, 3.0)
            } else {
                (base.gamma_multiply(0.75), 2.2)
            };
            draw_wire(&painter, p1, p2, self.zoom, color, width);
        }

        let mut edits: Vec<Edit> = Vec::new();
        let order: Vec<usize> = {
            let mut o: Vec<usize> = (0..placed.len()).filter(|i| placed[*i].visible).collect();
            o.sort_by_key(|i| self.selected.contains(&placed[*i].id));
            o
        };
        let mut started: Option<Wire> = None;
        let mut unlink: Option<(u32, String)> = None;
        let mut finished: Option<(Wire, Pos2)> = None;
        let mut move_delta = Vec2::ZERO;
        let mut move_started = false;
        for &i in &order {
            let p = &placed[i];
            let Some(node) = graph.node(p.id) else { continue };
            let is_sel = self.selected.contains(&p.id);
            let resp = ui.interact(p.rect, ui.scope_id().with(("hp_shader_node", p.id)), Sense::click_and_drag());
            self.paint_node(&painter, graph, node, p, is_sel, view);
            if resp.clicked() {
                let m = ui.input(|i| i.modifiers);
                edits.push(Edit::Select(p.id, m.shift || m.command));
            }
            if resp.double_clicked() {
                let out = p.def.outputs.first().map(|o| o.name.to_owned()).unwrap_or_else(|| "color".to_owned());
                edits.push(Edit::Preview(p.id, out));
            }
            if resp.drag_started_by(egui::PointerButton::Primary) {
                move_started = true;
                if !is_sel {
                    self.selected = vec![p.id];
                    self.link_sel = None;
                }
            }
            if resp.dragged_by(egui::PointerButton::Primary) {
                move_delta += resp.drag_delta();
            }
            resp.context_menu(|ui| {
                if !is_sel {
                    edits.push(Edit::Select(p.id, false));
                }
                if ui.button(hp_app::t!("shader.menu.preview")).clicked() {
                    let out = p.def.outputs.first().map(|o| o.name.to_owned()).unwrap_or_else(|| "color".to_owned());
                    edits.push(Edit::Preview(p.id, out));
                    ui.close();
                }
                if ui.button(hp_app::t!("shader.menu.duplicate")).clicked() {
                    edits.push(Edit::Duplicate);
                    ui.close();
                }
                if ui.button(hp_app::t!("shader.menu.disconnect")).clicked() {
                    edits.push(Edit::Disconnect(p.id));
                    ui.close();
                }
                if ui.button(hp_app::t!("shader.menu.delete")).clicked() {
                    edits.push(Edit::Delete(vec![p.id]));
                    ui.close();
                }
            });
            for (pi, pin) in p.def.inputs.iter().enumerate() {
                let c = p.pin(pi, false, self.zoom);
                let r = Rect::from_center_size(c, Vec2::splat(18.0 * self.zoom.max(0.7)));
                let pr = ui.interact(r, ui.scope_id().with(("hp_shader_in", p.id, pin.name)), Sense::click_and_drag());
                self.pin_hover(&painter, &pr, c, pin.name, pin.ty);
                if pr.drag_started() {
                    if let Some(l) = graph.link_to(p.id, pin.name) {
                        started = Some(Wire { node: l.from, pin: l.out.clone(), output: true, reconnecting: true });
                        unlink = Some((p.id, pin.name.to_owned()));
                    } else {
                        started =
                            Some(Wire { node: p.id, pin: pin.name.to_owned(), output: false, reconnecting: false });
                    }
                }
                if pr.drag_stopped() && self.wire.is_some() {
                    finished = self.wire.clone().zip(ui.input(|i| i.pointer.latest_pos()));
                }
            }
            for (pi, pin) in p.def.outputs.iter().enumerate() {
                let c = p.pin(pi, true, self.zoom);
                let r = Rect::from_center_size(c, Vec2::splat(18.0 * self.zoom.max(0.7)));
                let pr = ui.interact(r, ui.scope_id().with(("hp_shader_out", p.id, pin.name)), Sense::click_and_drag());
                self.pin_hover(&painter, &pr, c, pin.name, pin.ty);
                if pr.drag_started() {
                    started = Some(Wire { node: p.id, pin: pin.name.to_owned(), output: true, reconnecting: false });
                }
                if pr.drag_stopped() && self.wire.is_some() {
                    finished = self.wire.clone().zip(ui.input(|i| i.pointer.latest_pos()));
                }
            }
        }
        if let Some((to, input)) = unlink {
            graph.disconnect(to, &input);
        }
        if started.is_some() {
            self.wire = started;
        }
        if move_started {
            self.moved = true;
        }
        if move_delta != Vec2::ZERO {
            let d = move_delta / self.zoom;
            let moving: Vec<u32> = self.selected.clone();
            for n in &mut graph.nodes {
                if moving.contains(&n.id) {
                    n.pos[0] += d.x;
                    n.pos[1] += d.y;
                }
            }
        }
        let dragging_nodes = ui.input(|i| i.pointer.primary_down()) && move_delta != Vec2::ZERO;
        if !dragging_nodes && self.moved && !ui.input(|i| i.pointer.primary_down()) {
            self.moved = false;
            let snap = |v: f32| (v / SNAP).round() * SNAP;
            for n in &mut graph.nodes {
                if self.selected.contains(&n.id) {
                    n.pos = [snap(n.pos[0]), snap(n.pos[1])];
                }
            }
        }

        if let Some(w) = &self.wire {
            if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                let anchor = by_id.get(&w.node).and_then(|&k| {
                    let p = &placed[k];
                    let list = if w.output { p.def.outputs } else { p.def.inputs };
                    let idx = list.iter().position(|x| x.name == w.pin)?;
                    Some((p.pin(idx, w.output, self.zoom), list[idx].ty))
                });
                if let Some((a, ty)) = anchor {
                    let (from, to) = if w.output { (a, pos) } else { (pos, a) };
                    draw_wire(&painter, from, to, self.zoom, pin_color(ty), 3.0);
                }
            }
            ui.ctx().request_repaint();
        }
        if let Some((w, pos)) = finished {
            self.wire = None;
            self.finish_wire(rect, graph, &placed, w, pos, &mut edits);
        }
        if self.wire.is_some() && !ui.input(|i| i.pointer.any_down()) {
            self.wire = None;
        }
        if self.wire.is_some() && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.wire = None;
        }

        if bg.clicked_by(egui::PointerButton::Primary) {
            let m = ui.input(|i| i.modifiers);
            self.link_sel = wire_hit.clone();
            if !(m.shift || m.command) {
                self.selected.clear();
            }
        }
        if bg.double_clicked_by(egui::PointerButton::Primary)
            && wire_hit.is_none()
            && let Some(p) = hover
        {
            edits.push(Edit::OpenLibrary(p, None));
        }
        if bg.secondary_clicked()
            && self.right_travel < 4.0
            && let Some(p) = hover
        {
            edits.push(Edit::OpenLibrary(p, None));
        }
        self.marquee_update(ui, &bg, &placed, rect);

        for e in edits {
            self.apply(rect, graph, e);
        }
        self.keys(ui, rect, graph, hover.is_some());
        self.library_ui(ui, rect, graph);
        if graph.nodes.is_empty() && self.library.is_none() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                hp_app::t!("shader.empty_hint"),
                FontId::proportional(15.0),
                pal.text_weak,
            );
        }
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);
    }

    fn draw_grid(&self, painter: &Painter, rect: Rect) {
        let pal = theme::palette();
        for (step, color) in [(32.0, pal.bg2), (160.0, pal.bg3)] {
            let s = step * self.zoom;
            if s < 7.0 {
                continue;
            }
            let ox = (rect.min.x + self.pan.x).rem_euclid(s);
            let oy = (rect.min.y + self.pan.y).rem_euclid(s);
            let stroke = Stroke::new(1.0, color);
            let mut x = rect.min.x + (ox - rect.min.x).rem_euclid(s);
            while x < rect.max.x {
                painter.vline(x.round() + 0.5, rect.y_range(), stroke);
                x += s;
            }
            let mut y = rect.min.y + (oy - rect.min.y).rem_euclid(s);
            while y < rect.max.y {
                painter.hline(rect.x_range(), y.round() + 0.5, stroke);
                y += s;
            }
        }
    }

    fn pin_hover(&self, painter: &Painter, resp: &egui::Response, c: Pos2, name: &str, ty: PinTy) {
        if resp.hovered() || resp.dragged() {
            painter.circle_stroke(c, (PIN_R + 3.0) * self.zoom.max(0.7), Stroke::new(1.5, theme::accent()));
            let tip = format!("{} ({})", labels::pin(name), hp_app::i18n::tr(&format!("shader.type.{}", ty.name())));
            resp.clone().on_hover_text(tip);
        }
    }

    fn paint_node(&self, painter: &Painter, graph: &Graph, node: &Node, p: &Placed, selected: bool, view: &View<'_>) {
        let z = self.zoom;
        let pal = theme::palette();
        let r = p.rect;
        let round = 6.0 * z;
        painter.rect_filled(r.translate(vec2(0.0, 2.0 * z)).expand(1.0), round, Color32::from_black_alpha(70));
        painter.rect_filled(r, round, pal.bg3);
        let header = Rect::from_min_size(r.min, vec2(r.width(), HEADER * z));
        painter.rect_filled(
            header,
            egui::CornerRadius { nw: round as u8, ne: round as u8, sw: 0, se: 0 },
            category_color(p.def.category),
        );
        let stroke = if selected { Stroke::new(2.0, theme::accent()) } else { Stroke::new(1.0, pal.border_strong) };
        painter.rect_stroke(r, round, stroke, egui::StrokeKind::Inside);
        let title = labels::node_title(p.def.id);
        let probed = view.probe.is_some_and(|pr| pr.node == p.id);
        if z >= 0.4 {
            painter.text(
                header.left_center() + vec2(10.0 * z, 0.0),
                Align2::LEFT_CENTER,
                title,
                FontId::proportional(13.0 * z),
                Color32::WHITE,
            );
        }
        let worst = view.diagnostics.iter().filter(|d| d.node == Some(p.id)).map(|d| d.severity).max();
        let mut badge_x = header.right() - 12.0 * z;
        if let Some(sev) = worst.filter(|s| *s != Severity::Info) {
            let color = if sev == Severity::Error { theme::error() } else { theme::warning() };
            painter.circle_filled(pos2(badge_x, header.center().y), 4.5 * z, color);
            badge_x -= 14.0 * z;
        }
        if probed {
            painter.circle_stroke(
                pos2(badge_x, header.center().y),
                4.5 * z,
                Stroke::new(1.8 * z.max(0.7), Color32::WHITE),
            );
        }
        let font = FontId::proportional(12.0 * z);
        for (i, pin) in p.def.inputs.iter().enumerate() {
            let c = p.pin(i, false, z);
            let linked = graph.link_to(node.id, pin.name).is_some();
            self.paint_pin(painter, c, pin.ty, linked);
            if z >= 0.45 {
                painter.text(
                    c + vec2(10.0 * z, 0.0),
                    Align2::LEFT_CENTER,
                    labels::pin(pin.name),
                    font.clone(),
                    pal.text,
                );
            }
        }
        for (i, pin) in p.def.outputs.iter().enumerate() {
            let c = p.pin(i, true, z);
            let linked = graph.links.iter().any(|l| l.from == node.id && l.out == pin.name);
            self.paint_pin(painter, c, pin.ty, linked);
            if z >= 0.45 {
                painter.text(
                    c - vec2(10.0 * z, 0.0),
                    Align2::RIGHT_CENTER,
                    labels::pin(pin.name),
                    font.clone(),
                    pal.text,
                );
            }
        }
        if let Some(y) = p.geo.summary_y
            && z >= 0.5
            && let Some(text) = summary_text(graph, node, p.def)
        {
            let missing =
                p.def.id == "texture" && view.diagnostics.iter().any(|d| d.node == Some(p.id) && d.code == "no_source");
            let color = if missing { theme::warning() } else { pal.text_weak };
            let clip = painter.with_clip_rect(Rect::from_min_max(
                pos2(r.left() + 8.0 * z, r.top() + y * z),
                pos2(r.right() - 8.0 * z, r.top() + (y + SUMMARY) * z),
            ));
            clip.text(
                pos2(r.left() + 10.0 * z, r.top() + (y + SUMMARY * 0.5) * z),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(11.0 * z),
                color,
            );
        }
        if let Some(y) = p.geo.thumb_y {
            let area = Rect::from_min_size(
                pos2(r.left() + PAD * z, r.top() + y * z),
                vec2(r.width() - 2.0 * PAD * z, THUMB * z),
            );
            painter.rect_filled(area, 3.0 * z, pal.bg0);
            let source =
                if p.def.is_output() { graph.link_to(node.id, "color").map(|l| l.from) } else { Some(node.id) };
            if let Some(tex) = source.and_then(|s| view.thumbs.get(&s)) {
                let ts = tex.size_vec2();
                let fit = (area.width() / ts.x).min(area.height() / ts.y);
                let img = Rect::from_center_size(area.center(), ts * fit);
                painter.image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            painter.rect_stroke(area, 3.0 * z, Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);
        }
    }

    fn paint_pin(&self, painter: &Painter, c: Pos2, ty: PinTy, linked: bool) {
        let r = PIN_R * self.zoom;
        let color = pin_color(ty);
        let pal = theme::palette();
        let stroke = Stroke::new(1.6, color);
        let fill = if linked { color } else { pal.bg3 };
        match ty {
            PinTy::Color => {
                painter.circle_filled(c, r, fill);
                if !linked {
                    painter.circle_stroke(c, r - 0.5, stroke);
                }
            }
            PinTy::Float => {
                let sq = Rect::from_center_size(c, Vec2::splat(r * 1.7));
                painter.rect_filled(sq, 1.0, fill);
                if !linked {
                    painter.rect_stroke(sq, 1.0, stroke, egui::StrokeKind::Inside);
                }
            }
            PinTy::Uv => {
                let k = r * 1.35;
                let pts = vec![c + vec2(0.0, -k), c + vec2(k, 0.0), c + vec2(0.0, k), c + vec2(-k, 0.0)];
                painter.add(egui::Shape::convex_polygon(pts, fill, if linked { Stroke::NONE } else { stroke }));
            }
        }
    }

    fn finish_wire(
        &mut self,
        rect: Rect,
        graph: &mut Graph,
        placed: &[Placed],
        wire: Wire,
        pos: Pos2,
        edits: &mut Vec<Edit>,
    ) {
        let reach = 14.0 * self.zoom.max(0.7);
        let mut target: Option<(u32, String)> = None;
        let mut best = reach;
        for p in placed.iter().filter(|p| p.visible) {
            if p.id == wire.node {
                continue;
            }
            let list = if wire.output { p.def.inputs } else { p.def.outputs };
            for (i, pin) in list.iter().enumerate() {
                let d = p.pin(i, !wire.output, self.zoom).distance(pos);
                if d < best {
                    best = d;
                    target = Some((p.id, pin.name.to_owned()));
                }
            }
        }
        match target {
            Some((id, pin)) => {
                let res = if wire.output {
                    graph.connect(wire.node, &wire.pin, id, &pin)
                } else {
                    graph.connect(id, &pin, wire.node, &wire.pin)
                };
                if res.is_ok() {
                    self.link_sel = None;
                }
            }
            None => {
                if !wire.reconnecting && rect.contains(pos) {
                    edits.push(Edit::OpenLibrary(pos, Some(wire)));
                }
            }
        }
    }

    fn marquee_update(&mut self, ui: &Ui, bg: &egui::Response, placed: &[Placed], _rect: Rect) {
        if bg.drag_started_by(egui::PointerButton::Primary) {
            self.marquee = ui.input(|i| i.pointer.press_origin());
            if !ui.input(|i| i.modifiers.shift || i.modifiers.command) {
                self.selected.clear();
            }
            self.link_sel = None;
        }
        let (Some(start), Some(now)) = (self.marquee, ui.input(|i| i.pointer.latest_pos())) else { return };
        let r = Rect::from_two_pos(start, now);
        if bg.dragged_by(egui::PointerButton::Primary) {
            let acc = theme::accent();
            let painter = ui.painter();
            painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(acc.r(), acc.g(), acc.b(), 36));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, acc), egui::StrokeKind::Inside);
            for p in placed.iter().filter(|p| p.visible) {
                if r.intersects(p.rect) && !self.selected.contains(&p.id) {
                    self.selected.push(p.id);
                }
            }
        }
        if bg.drag_stopped() {
            self.marquee = None;
        }
    }

    fn apply(&mut self, rect: Rect, graph: &mut Graph, edit: Edit) {
        match edit {
            Edit::Select(id, additive) => {
                self.link_sel = None;
                if additive {
                    if let Some(i) = self.selected.iter().position(|s| *s == id) {
                        self.selected.remove(i);
                    } else {
                        self.selected.push(id);
                    }
                } else if !self.selected.contains(&id) || (self.selected.len() > 1 && !self.moved) {
                    self.selected = vec![id];
                }
            }
            Edit::Delete(ids) => {
                for id in ids {
                    graph.remove_node(id);
                    self.selected.retain(|s| *s != id);
                }
            }
            Edit::Duplicate => self.duplicate(graph),
            Edit::Disconnect(id) => {
                graph.links.retain(|l| l.to != id && l.from != id);
            }
            Edit::Preview(id, out) => self.preview = Some((id, out)),
            Edit::OpenLibrary(pos, connect) => {
                let gp = self.to_graph(rect, pos);
                self.library =
                    Some(Library { anchor: pos, graph_pos: gp, query: String::new(), index: 0, connect, fresh: true });
            }
        }
    }

    fn copy(&mut self, graph: &Graph) {
        let nodes: Vec<Node> = graph.nodes.iter().filter(|n| self.selected.contains(&n.id)).cloned().collect();
        let links: Vec<Link> = graph
            .links
            .iter()
            .filter(|l| self.selected.contains(&l.from) && self.selected.contains(&l.to))
            .cloned()
            .collect();
        if !nodes.is_empty() {
            self.clip = Clip { nodes, links };
        }
    }

    fn paste(&mut self, graph: &mut Graph) {
        let clip = self.clip.clone();
        if clip.nodes.is_empty() {
            return;
        }
        let mut map: HashMap<u32, u32> = HashMap::new();
        let mut added = Vec::new();
        for n in &clip.nodes {
            if graph.nodes.len() >= hp_shader::MAX_NODES {
                break;
            }
            let id = graph.next_id();
            let mut copy = n.clone();
            copy.id = id;
            copy.pos = [n.pos[0] + 40.0, n.pos[1] + 40.0];
            graph.nodes.push(copy);
            map.insert(n.id, id);
            added.push(id);
        }
        for l in &clip.links {
            if let (Some(&a), Some(&b)) = (map.get(&l.from), map.get(&l.to)) {
                let _ = graph.connect(a, &l.out, b, &l.input);
            }
        }
        self.clip.nodes.iter_mut().for_each(|n| n.pos = [n.pos[0] + 40.0, n.pos[1] + 40.0]);
        self.selected = added;
    }

    fn duplicate(&mut self, graph: &mut Graph) {
        let keep = self.clip.clone();
        self.copy(graph);
        self.paste(graph);
        self.clip = keep;
    }

    fn keys(&mut self, ui: &Ui, _rect: Rect, graph: &mut Graph, hovered: bool) {
        let ctx = ui.ctx();
        if !hovered || ctx.text_edit_focused() || self.library.is_some() {
            return;
        }
        let consume = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if consume(Modifiers::NONE, Key::Delete) || consume(Modifiers::NONE, Key::Backspace) {
            if let Some((to, input)) = self.link_sel.take() {
                graph.disconnect(to, &input);
            } else {
                let ids = std::mem::take(&mut self.selected);
                for id in ids {
                    graph.remove_node(id);
                }
            }
        }
        if consume(Modifiers::COMMAND, Key::A) {
            self.selected = graph.nodes.iter().map(|n| n.id).collect();
        }
        if consume(Modifiers::COMMAND, Key::D) {
            self.duplicate(graph);
        }
        let (copy, paste) = ctx.input_mut(|i| {
            let before = i.events.len();
            let mut copy = false;
            let mut paste = false;
            i.events.retain(|e| match e {
                egui::Event::Copy => {
                    copy = true;
                    false
                }
                egui::Event::Paste(_) => {
                    paste = true;
                    false
                }
                _ => true,
            });
            let _ = before;
            (copy, paste)
        });
        if copy {
            self.copy(graph);
        }
        if paste {
            self.paste(graph);
        }
        if consume(Modifiers::NONE, Key::F) {
            self.want_fit = true;
        }
        if consume(Modifiers::NONE, Key::Space) {
            let at = ui.input(|i| i.pointer.hover_pos()).unwrap_or(ui.min_rect().center());
            self.open_library(_rect, at);
        }
    }

    fn library_ui(&mut self, ui: &mut Ui, rect: Rect, graph: &mut Graph) {
        let Some(mut lib) = self.library.take() else { return };
        let ctx = ui.ctx().clone();
        let pal = theme::palette();
        let width = 300.0;
        let height = 400.0;
        let mut pos = lib.anchor;
        pos.x = pos.x.min(rect.right() - width - 8.0).max(rect.left() + 4.0);
        pos.y = pos.y.min(rect.bottom() - height - 8.0).max(rect.top() + 4.0);
        let mut chosen: Option<&'static str> = None;
        let mut close = false;
        let compatible = |def: &NodeDef| match &lib.connect {
            Some(w) if w.output => !def.inputs.is_empty(),
            Some(_) => !def.outputs.is_empty(),
            None => true,
        };
        let q = lib.query.trim().to_lowercase();
        let mut list: Vec<&'static NodeDef> = NODES
            .iter()
            .filter(|d| compatible(d))
            .filter(|d| {
                q.is_empty()
                    || q.split_whitespace().all(|w| {
                        labels::node_title(d.id).to_lowercase().contains(w)
                            || d.id.contains(w)
                            || labels::category(d.category).to_lowercase().contains(w)
                    })
            })
            .collect();
        if q.is_empty() {
            list.sort_by_key(|d| CATEGORIES.iter().position(|c| *c == d.category).unwrap_or(99));
        } else {
            list.sort_by_key(|d| {
                let title = labels::node_title(d.id).to_lowercase();
                (!title.starts_with(&q), title)
            });
        }
        if !list.is_empty() {
            lib.index = lib.index.min(list.len() - 1);
        }
        let (down, up, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if down && lib.index + 1 < list.len() {
            lib.index += 1;
        }
        if up && lib.index > 0 {
            lib.index -= 1;
        }
        if enter && let Some(d) = list.get(lib.index) {
            chosen = Some(d.id);
        }
        if esc {
            close = true;
        }
        let area = egui::Area::new(egui::Id::unique("hp_shader_library"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(width);
                    ui.label(egui::RichText::new(hp_app::t!("shader.library.title")).strong());
                    let edit = egui::TextEdit::singleline(&mut lib.query)
                        .hint_text(hp_app::t!("shader.library.search"))
                        .desired_width(f32::INFINITY);
                    let r = ui.add(edit);
                    if lib.fresh {
                        r.request_focus();
                        lib.fresh = false;
                    }
                    if r.changed() {
                        lib.index = 0;
                    }
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical().max_height(height - 70.0).auto_shrink([false, true]).show(ui, |ui| {
                        if list.is_empty() {
                            ui.weak(hp_app::t!("shader.library.none"));
                        }
                        let mut last_cat = "";
                        for (i, d) in list.iter().enumerate() {
                            if q.is_empty() && d.category != last_cat {
                                last_cat = d.category;
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(labels::category(d.category)).small().color(pal.text_weak),
                                );
                            }
                            let sel = i == lib.index;
                            let resp = ui
                                .with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                                    ui.add(
                                        egui::Button::selectable(sel, labels::node_title(d.id))
                                            .frame_when_inactive(false)
                                            .min_size(vec2(0.0, 22.0)),
                                    )
                                })
                                .inner
                                .on_hover_text(labels::node_tip(d.id));
                            if sel && (down || up) {
                                resp.scroll_to_me(None);
                            }
                            if resp.clicked() {
                                chosen = Some(d.id);
                            }
                            if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                                lib.index = i;
                            }
                        }
                    });
                });
            });
        let outside = ctx.input(|i| i.pointer.any_pressed())
            && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if outside && !lib.fresh {
            close = true;
        }
        if let Some(kind) = chosen {
            self.add_from_library(graph, kind, &lib);
            return;
        }
        if !close {
            self.library = Some(lib);
        }
    }

    fn add_from_library(&mut self, graph: &mut Graph, kind: &str, lib: &Library) {
        let pos = [lib.graph_pos.x, lib.graph_pos.y];
        let Ok(id) = graph.add_node(kind, pos) else { return };
        if let (Some(w), Some(def)) = (&lib.connect, node_def(kind)) {
            if w.output {
                let src_ty = graph.def_of(w.node).and_then(|d| d.output(&w.pin)).map(|p| p.ty);
                let pin = def.inputs.iter().find(|p| Some(p.ty) == src_ty).or_else(|| def.inputs.first());
                if let Some(pin) = pin {
                    let _ = graph.connect(w.node, &w.pin, id, pin.name);
                }
            } else {
                let dst_ty = graph.def_of(w.node).and_then(|d| d.input(&w.pin)).map(|p| p.ty);
                let pin = def.outputs.iter().find(|p| Some(p.ty) == dst_ty).or_else(|| def.outputs.first());
                if let Some(pin) = pin {
                    let _ = graph.connect(id, pin.name, w.node, &w.pin);
                }
            }
        }
        self.selected = vec![id];
    }
}
