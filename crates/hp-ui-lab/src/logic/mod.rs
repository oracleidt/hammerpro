pub mod graph;
pub mod layout;

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use hp_app::{Action, AppState, t, tf};
use hp_editor::{Id, ObjRef, SelectOp};

use graph::{EdgeState, Graph, NodeFilter, fingerprint, format_logical_pos};

pub const NODE_W: f32 = 180.0;
pub const NODE_H: f32 = 44.0;
const MIN_ZOOM: f32 = 0.02;
const MAX_ZOOM: f32 = 3.0;
const DETAIL_ZOOM: f32 = 0.35;
const PORT_R: f32 = 5.0;

const GENERIC_IO: &[&str] = &[
    "Kill",
    "KillHierarchy",
    "AddOutput",
    "FireUser1",
    "FireUser2",
    "FireUser3",
    "FireUser4",
    "SetParent",
    "SetParentAttachment",
    "SetParentAttachmentMaintainOffset",
    "ClearParent",
    "Use",
    "OnUser1",
    "OnUser2",
    "OnUser3",
    "OnUser4",
    "OnKilled",
];

type Header<'a> = &'a mut dyn FnMut(&mut egui::Ui, &mut AppState);

pub const UNDO_MOVE: &str = "Move in logical view";
pub const UNDO_ARRANGE: &str = "Arrange logical view";

fn edge_color(state: EdgeState) -> Color32 {
    match state {
        EdgeState::Valid => Color32::from_rgb(110, 200, 120),
        EdgeState::MissingTarget => Color32::from_rgb(240, 90, 80),
        EdgeState::UnknownOutput | EdgeState::UnknownInput => Color32::from_rgb(235, 195, 70),
    }
}

fn class_color(class: &str) -> Color32 {
    let c = class.to_ascii_lowercase();
    let starts = |p: &[&str]| p.iter().any(|x| c.starts_with(x));
    if starts(&["logic_", "math_", "filter_", "game_", "point_template", "point_clientcommand", "point_servercommand"])
    {
        Color32::from_rgb(160, 110, 230)
    } else if starts(&["trigger_"]) {
        Color32::from_rgb(235, 140, 50)
    } else if starts(&["func_"]) {
        Color32::from_rgb(80, 145, 235)
    } else if starts(&["env_", "ambient_", "light", "point_spotlight", "sky_"]) {
        Color32::from_rgb(225, 195, 70)
    } else if starts(&["npc_", "prop_", "item_", "weapon_", "monster_"]) {
        Color32::from_rgb(60, 180, 170)
    } else if starts(&["info_", "path_", "point_", "keyframe_", "move_"]) {
        Color32::from_rgb(100, 190, 110)
    } else {
        Color32::from_rgb(150, 150, 160)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Drag {
    #[default]
    None,
    Nodes {
        ids: Vec<Id>,
        delta: Vec2,
    },
    Box {
        start: Pos2,
    },
    Link {
        from: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct NewLink {
    from: Id,
    to: Id,
    output: String,
    input: String,
    param: String,
    delay: f32,
    once: bool,
    name: String,
    needs_name: bool,
    at: Pos2,
}

#[derive(Default)]
pub struct LogicalView {
    graph: Graph,
    auto: Vec<[f32; 2]>,
    checked: Option<(u64, u64, u64, NodeFilter)>,
    graph_fp: Option<u64>,
    layout_fp: Option<u64>,
    map_serial: Option<u64>,
    pan: Vec2,
    zoom: f32,
    frame_all: bool,
    filter: NodeFilter,
    search: String,
    shown: Option<Vec<bool>>,
    drag: Drag,
    new_link: Option<NewLink>,
    hovered_edge: Option<usize>,
    salt: usize,
}

impl LogicalView {
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn node_pos(&self, i: usize) -> [f32; 2] {
        let node = &self.graph.nodes[i];
        let mut p = node.stored.unwrap_or_else(|| self.auto.get(i).copied().unwrap_or_default());
        if let Drag::Nodes { ids, delta } = &self.drag
            && ids.contains(&node.id)
        {
            p[0] += delta.x;
            p[1] += delta.y;
        }
        p
    }

    pub fn refresh(&mut self, state: &AppState) {
        let serial = state.sidecar.serial();
        if self.map_serial != Some(serial) {
            self.map_serial = Some(serial);
            self.graph_fp = None;
            self.layout_fp = None;
            self.frame_all = true;
            self.drag = Drag::None;
            self.new_link = None;
        }
        let generation = state.assets.generation();
        let key = (serial, state.doc.revision(), generation, self.filter);
        if self.checked == Some(key) {
            return;
        }
        self.checked = Some(key);
        let fp = fingerprint(&state.doc, true, generation, self.filter);
        if self.graph_fp == Some(fp) {
            return;
        }
        self.graph_fp = Some(fp);
        let names = state.runtime.helpers.names(&state.doc);
        self.graph = Graph::build(&state.doc, &names, state.assets.fgd(), self.filter);
        drop(names);
        let layout_fp = fingerprint(&state.doc, false, generation, self.filter);
        if self.layout_fp != Some(layout_fp) || self.auto.len() != self.graph.nodes.len() {
            self.layout_fp = Some(layout_fp);
            let fixed: Vec<Option<[f32; 2]>> = self.graph.nodes.iter().map(|n| n.stored).collect();
            self.auto = layout::layout(self.graph.nodes.len(), &self.graph.links(), &fixed);
        }
    }

    fn to_screen(&self, rect: Rect, p: [f32; 2]) -> Pos2 {
        rect.min + self.pan + vec2(p[0], p[1]) * self.zoom
    }

    fn to_graph(&self, rect: Rect, s: Pos2) -> Vec2 {
        (s - rect.min - self.pan) / self.zoom
    }

    fn node_rect(&self, rect: Rect, i: usize) -> Rect {
        Rect::from_min_size(self.to_screen(rect, self.node_pos(i)), vec2(NODE_W, NODE_H) * self.zoom)
    }

    fn frame(&mut self, rect: Rect, which: &[usize]) {
        let all: Vec<usize>;
        let list = if which.is_empty() {
            all = (0..self.graph.nodes.len()).collect();
            &all
        } else {
            which
        };
        if list.is_empty() || rect.width() < 10.0 || rect.height() < 10.0 {
            self.zoom = 1.0;
            self.pan = vec2(20.0, 20.0);
            return;
        }
        let mut lo = vec2(f32::MAX, f32::MAX);
        let mut hi = vec2(f32::MIN, f32::MIN);
        for &i in list {
            let p = self.node_pos(i);
            lo = lo.min(vec2(p[0], p[1]));
            hi = hi.max(vec2(p[0] + NODE_W, p[1] + NODE_H));
        }
        let size = (hi - lo).max(vec2(NODE_W, NODE_H));
        let zoom = ((rect.width() - 40.0) / size.x).min((rect.height() - 40.0) / size.y);
        self.zoom = zoom.clamp(MIN_ZOOM, 1.2);
        let center = (lo + hi) * 0.5;
        self.pan = rect.size() * 0.5 - center * self.zoom;
    }

    fn update_shown(&mut self) {
        let matches = self.matches();
        self.shown = (!self.search.trim().is_empty()).then(|| {
            let mut shown = vec![false; self.graph.nodes.len()];
            for i in matches {
                shown[i] = true;
            }
            shown
        });
    }

    fn is_shown(&self, i: usize) -> bool {
        self.shown.as_ref().is_none_or(|s| s.get(i).copied().unwrap_or(false))
    }

    fn edge_shown(&self, e: &graph::Edge) -> bool {
        self.is_shown(e.from) && e.to.is_none_or(|t| self.is_shown(t))
    }

    fn matches(&self) -> Vec<usize> {
        let q = self.search.trim().to_ascii_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        (0..self.graph.nodes.len())
            .filter(|&i| {
                let n = &self.graph.nodes[i];
                n.name.to_ascii_lowercase().contains(&q) || n.class.to_ascii_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.lab.logic_open {
            return;
        }
        self.refresh(state);
        let mut open = true;
        hp_app::chrome::Window::new(format!("🔀  {}", t!("lab.logic.title")))
            .id(egui::Id::unique("lab_logical_view"))
            .open(&mut open)
            .default_size([900.0, 600.0])
            .min_size([360.0, 240.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| self.ui(ui, state));
        if !open {
            state.runtime.lab.logic_open = false;
            self.drag = Drag::None;
            self.new_link = None;
        }
        self.new_link_window(ctx, state);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.content_ui(ui, state, None);
    }

    pub fn quadrant_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, header: Header<'_>) {
        self.refresh(state);
        self.content_ui(ui, state, Some(header));
        let ctx = ui.ctx().clone();
        self.new_link_window(&ctx, state);
    }

    fn content_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, mut header: Option<Header<'_>>) {
        let mut frame_selection = false;
        let mut frame_search = false;
        ui.horizontal_wrapped(|ui| {
            if let Some(header) = header.as_mut() {
                header(ui, state);
            }
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.search).hint_text(t!("lab.logic.search")).desired_width(180.0),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                frame_selection = true;
            }
            frame_search = r.changed();
            egui::ComboBox::from_id_salt(("lab_logic_filter", self.salt))
                .selected_text(t!(self.filter.label_key()))
                .show_ui(ui, |ui| {
                    for f in NodeFilter::ALL {
                        ui.selectable_value(&mut self.filter, f, t!(f.label_key()));
                    }
                });
            if ui.button(t!("lab.logic.frame_all")).on_hover_text(t!("lab.logic.frame_all_tip")).clicked() {
                self.frame_all = true;
            }
            if ui.button(t!("lab.logic.frame_selection")).clicked() {
                frame_selection = true;
            }
            if ui
                .add_enabled(!self.graph.nodes.is_empty(), egui::Button::new(t!("lab.logic.arrange")))
                .on_hover_text(t!("lab.logic.arrange_tip"))
                .clicked()
            {
                self.arrange(state);
            }
        });
        self.refresh(state);
        self.update_shown();
        let size = ui.available_size() - vec2(0.0, 22.0);
        let (rect, response) = ui.allocate_exact_size(size.max(vec2(100.0, 100.0)), Sense::click_and_drag());
        if self.frame_all {
            self.frame_all = false;
            self.frame(rect, &[]);
        }
        if frame_selection {
            let mut which = self.matches();
            if which.is_empty() {
                which = (0..self.graph.nodes.len())
                    .filter(|&i| state.doc.is_selected(ObjRef::Entity(self.graph.nodes[i].id)))
                    .collect();
            }
            if !which.is_empty() {
                self.frame(rect, &which);
            }
        }
        if frame_search && !frame_selection {
            let which = self.matches();
            if !which.is_empty() {
                self.frame(rect, &which);
            }
        }
        if self.zoom <= 0.0 {
            self.frame(rect, &[]);
        }
        self.input(ui, rect, &response, state);
        let icons = self.icon_textures(ui.ctx(), state);
        self.paint(ui, rect, state, &icons);
        if let Some(e) = self.hovered_edge.and_then(|k| self.graph.edges.get(k)) {
            let text = edge_text(e, &self.graph);
            let problem = e.state.is_problem().then(|| {
                let why = match e.state {
                    EdgeState::MissingTarget => t!("lab.logic.why_missing"),
                    EdgeState::UnknownOutput => t!("lab.logic.why_output"),
                    _ => t!("lab.logic.why_input"),
                };
                (edge_color(e.state), why)
            });
            egui::Tooltip::for_widget(&response).at_pointer().gap(12.0).show(|ui| {
                ui.label(text);
                if let Some((color, why)) = problem {
                    ui.colored_label(color, why);
                }
            });
        }
        ui.horizontal(|ui| {
            let problems = self.graph.problem_count();
            ui.label(
                egui::RichText::new(tf!("lab.logic.summary", self.graph.nodes.len(), self.graph.edges.len()))
                    .small()
                    .weak(),
            );
            if let Some(shown) = &self.shown {
                let found = shown.iter().filter(|&&s| s).count();
                ui.label(
                    egui::RichText::new(tf!("lab.logic.search_results", found)).small().color(hp_app::theme::accent()),
                );
            }
            if problems > 0 {
                ui.label(
                    egui::RichText::new(tf!("lab.logic.problems", problems))
                        .small()
                        .color(edge_color(EdgeState::MissingTarget)),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (state_, key) in [
                    (EdgeState::UnknownOutput, "lab.logic.legend_unknown"),
                    (EdgeState::MissingTarget, "lab.logic.legend_missing"),
                    (EdgeState::Valid, "lab.logic.legend_valid"),
                ] {
                    ui.label(egui::RichText::new(t!(key)).small().weak());
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, edge_color(state_));
                }
            });
        });
    }

    fn node_at(&self, rect: Rect, p: Pos2) -> Option<usize> {
        (0..self.graph.nodes.len()).rev().find(|&i| self.is_shown(i) && self.node_rect(rect, i).contains(p))
    }

    fn out_port(&self, rect: Rect, i: usize) -> Pos2 {
        let r = self.node_rect(rect, i);
        pos2(r.right(), r.center().y)
    }

    fn in_port(&self, rect: Rect, i: usize) -> Pos2 {
        let r = self.node_rect(rect, i);
        pos2(r.left(), r.center().y)
    }

    fn input(&mut self, ui: &egui::Ui, rect: Rect, response: &egui::Response, state: &mut AppState) {
        let pointer = response.hover_pos();
        let (mods, wheel, pan_delta, secondary_down, middle_down) = ui.input(|i| {
            (
                i.modifiers,
                i.smooth_scroll_delta.y,
                i.pointer.delta(),
                i.pointer.secondary_down(),
                i.pointer.middle_down(),
            )
        });
        if let Some(p) = pointer
            && wheel != 0.0
            && response.hovered()
        {
            let before = self.to_graph(rect, p);
            let factor = (wheel * 0.0025).exp();
            self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            self.pan = p - rect.min - before * self.zoom;
        }
        if response.dragged() && (middle_down || secondary_down) {
            self.pan += pan_delta;
        }
        if response.hovered() && ui.input(|i| i.key_pressed(egui::Key::Home)) {
            self.frame(rect, &[]);
        }
        let primary_pressed = response.drag_started_by(egui::PointerButton::Primary);
        let press = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos());
        if primary_pressed && let Some(p) = press {
            let on_port = (0..self.graph.nodes.len()).rev().find(|&i| {
                self.is_shown(i) && self.zoom >= DETAIL_ZOOM && self.out_port(rect, i).distance(p) <= PORT_R + 4.0
            });
            if let Some(from) = on_port {
                self.drag = Drag::Link { from };
            } else if let Some(i) = self.node_at(rect, p) {
                let id = self.graph.nodes[i].id;
                if !state.doc.is_selected(ObjRef::Entity(id)) {
                    self.select(state, &[id], if mods.ctrl || mods.shift { SelectOp::Add } else { SelectOp::Replace });
                }
                let ids: Vec<Id> = self
                    .graph
                    .nodes
                    .iter()
                    .filter(|n| state.doc.is_selected(ObjRef::Entity(n.id)))
                    .map(|n| n.id)
                    .collect();
                self.drag = Drag::Nodes { ids, delta: Vec2::ZERO };
            } else {
                self.drag = Drag::Box { start: p };
            }
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Drag::Nodes { delta, .. } = &mut self.drag
        {
            *delta += pan_delta / self.zoom;
        }
        if response.drag_stopped() {
            let drag = std::mem::take(&mut self.drag);
            match drag {
                Drag::Nodes { ids, delta } if delta.length() > 0.5 => {
                    let moves: Vec<(Id, [f32; 2])> = ids
                        .iter()
                        .filter_map(|id| self.graph.index.get(id).map(|&i| (*id, i)))
                        .map(|(id, i)| {
                            let p = self.node_pos(i);
                            (id, [p[0] + delta.x, p[1] + delta.y])
                        })
                        .collect();
                    write_positions(state, UNDO_MOVE, &moves);
                }
                Drag::Box { start } => {
                    if let Some(end) = response.interact_pointer_pos() {
                        let r = Rect::from_two_pos(start, end);
                        let ids: Vec<Id> = (0..self.graph.nodes.len())
                            .filter(|&i| self.is_shown(i) && r.intersects(self.node_rect(rect, i)))
                            .map(|i| self.graph.nodes[i].id)
                            .collect();
                        let op = if mods.ctrl || mods.shift { SelectOp::Add } else { SelectOp::Replace };
                        self.select(state, &ids, op);
                    }
                }
                Drag::Link { from } => {
                    if let Some(end) = response.interact_pointer_pos()
                        && let Some(to) = self.node_at(rect, end)
                    {
                        self.start_new_link(state, from, to, end);
                    }
                }
                _ => {}
            }
        }
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            match self.node_at(rect, p) {
                Some(i) => {
                    let op = if mods.ctrl {
                        SelectOp::Toggle
                    } else if mods.shift {
                        SelectOp::Add
                    } else {
                        SelectOp::Replace
                    };
                    self.select(state, &[self.graph.nodes[i].id], op);
                }
                None => match self.hovered_edge.and_then(|e| self.graph.edges.get(e)) {
                    Some(e) => {
                        let id = self.graph.nodes[e.from].id;
                        self.select(state, &[id], SelectOp::Replace);
                    }
                    None if !mods.ctrl && !mods.shift => state.doc.select(&[], SelectOp::Replace),
                    None => {}
                },
            }
        }
        if response.double_clicked()
            && let Some(p) = response.interact_pointer_pos()
            && let Some(i) = self.node_at(rect, p)
        {
            self.select(state, &[self.graph.nodes[i].id], SelectOp::Replace);
            state.request(Action::Properties);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.drag = Drag::None;
        }
        let hit = pointer.and_then(|p| self.node_at(rect, p));
        response.context_menu(|ui| {
            let selected: Vec<usize> = (0..self.graph.nodes.len())
                .filter(|&i| state.doc.is_selected(ObjRef::Entity(self.graph.nodes[i].id)))
                .collect();
            if ui.button(t!("lab.logic.properties")).clicked() {
                if let Some(i) = hit {
                    self.select(state, &[self.graph.nodes[i].id], SelectOp::Replace);
                }
                state.request(Action::Properties);
                ui.close();
            }
            if ui.button(t!("lab.logic.center_views")).clicked() {
                if let Some(i) = hit
                    && !selected.contains(&i)
                {
                    self.select(state, &[self.graph.nodes[i].id], SelectOp::Replace);
                }
                state.request(Action::Center2dOnSelection);
                state.request(Action::Center3dOnSelection);
                ui.close();
            }
            let pinned: Vec<Id> = selected
                .iter()
                .filter(|&&i| self.graph.nodes[i].stored.is_some())
                .map(|&i| self.graph.nodes[i].id)
                .collect();
            if ui.add_enabled(!pinned.is_empty(), egui::Button::new(t!("lab.logic.unpin"))).clicked() {
                clear_positions(state, &pinned);
                ui.close();
            }
        });
        self.hovered_edge = None;
        if self.drag == Drag::None
            && let Some(p) = pointer
            && self.node_at(rect, p).is_none()
        {
            self.hovered_edge = self.edge_at(rect, p);
        }
    }

    fn edge_at(&self, rect: Rect, p: Pos2) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (k, e) in self.graph.edges.iter().enumerate() {
            if !self.edge_shown(e) {
                continue;
            }
            let a = self.out_port(rect, e.from);
            let b = match e.to {
                Some(t) => self.in_port(rect, t),
                None => a + vec2(40.0, 0.0),
            };
            let bb = Rect::from_two_pos(a, b).expand(60.0 * self.zoom.max(0.3) + 6.0);
            if !bb.contains(p) {
                continue;
            }
            let pts = curve_points(a, b, self.zoom, e.to == Some(e.from), 12);
            for w in pts.windows(2) {
                let d = segment_distance(p, w[0], w[1]);
                if d < 6.0 && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, k));
                }
            }
        }
        best.map(|(_, k)| k)
    }

    fn select(&mut self, state: &mut AppState, ids: &[Id], op: SelectOp) {
        let objs: Vec<ObjRef> =
            ids.iter().filter(|id| state.doc.is_entity_visible(**id)).map(|id| ObjRef::Entity(*id)).collect();
        if op == SelectOp::Replace || !objs.is_empty() {
            if objs.len() == 1 {
                state.doc.select_picked(objs[0], state.select_mode(), op);
            } else {
                state.doc.select(&objs, op);
            }
        }
    }

    fn icon_textures(&self, ctx: &egui::Context, state: &mut AppState) -> Vec<Option<egui::TextureId>> {
        let mut seen: std::collections::HashMap<&str, Option<egui::TextureId>> = std::collections::HashMap::new();
        self.graph
            .nodes
            .iter()
            .map(|n| {
                let name = n.icon.as_deref()?;
                *seen.entry(name).or_insert_with(|| match state.assets.thumbnail(ctx, name) {
                    hp_app::state::ThumbState::Ready { texture, .. } => Some(texture),
                    _ => None,
                })
            })
            .collect()
    }

    pub fn filter(&self) -> NodeFilter {
        self.filter
    }

    pub fn set_filter(&mut self, filter: NodeFilter) {
        self.filter = filter;
    }

    fn paint(&mut self, ui: &egui::Ui, rect: Rect, state: &AppState, icons: &[Option<egui::TextureId>]) {
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        painter.rect_filled(rect, CornerRadius::same(4), visuals.extreme_bg_color);
        if self.zoom >= 0.25 {
            let step = 100.0 * self.zoom;
            let origin = rect.min + self.pan;
            let color = visuals.widgets.noninteractive.bg_stroke.color.gamma_multiply(0.35);
            let mut x = origin.x.rem_euclid(step) + rect.left();
            while x < rect.right() {
                painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, color));
                x += step;
            }
            let mut y = origin.y.rem_euclid(step) + rect.top();
            while y < rect.bottom() {
                painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, color));
                y += step;
            }
        }
        if self.graph.nodes.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                t!("lab.logic.empty"),
                FontId::proportional(14.0),
                visuals.weak_text_color(),
            );
            return;
        }
        let detail = self.zoom >= DETAIL_ZOOM;
        let view = rect.expand(40.0);
        let selected: Vec<bool> =
            self.graph.nodes.iter().map(|n| state.doc.is_selected(ObjRef::Entity(n.id))).collect();
        for (k, e) in self.graph.edges.iter().enumerate() {
            if !self.edge_shown(e) {
                continue;
            }
            let a = self.out_port(rect, e.from);
            let b = match e.to {
                Some(t) => self.in_port(rect, t),
                None => a + vec2(40.0 * self.zoom.max(0.5), 0.0),
            };
            let bb = Rect::from_two_pos(a, b).expand(80.0 * self.zoom);
            if !view.intersects(bb) {
                continue;
            }
            let hot = selected[e.from] || e.to.is_some_and(|t| selected[t]) || self.hovered_edge == Some(k);
            let color = edge_color(e.state);
            let width = if hot { 2.5 } else { 1.4 };
            let color = if hot { color } else { color.gamma_multiply(0.8) };
            if detail {
                let pts = curve_points(a, b, self.zoom, e.to == Some(e.from), 16);
                painter.add(egui::Shape::line(pts.clone(), Stroke::new(width, color)));
                if e.to.is_some() {
                    arrow_head(&painter, &pts, color, 7.0);
                } else {
                    let s = 4.0;
                    painter.line_segment([b + vec2(-s, -s), b + vec2(s, s)], Stroke::new(2.0, color));
                    painter.line_segment([b + vec2(-s, s), b + vec2(s, -s)], Stroke::new(2.0, color));
                    painter.text(b + vec2(8.0, 0.0), Align2::LEFT_CENTER, &e.target, FontId::proportional(11.0), color);
                }
            } else {
                painter.line_segment([a, b], Stroke::new(1.0, color));
            }
        }
        let text_color = visuals.text_color();
        let weak = visuals.weak_text_color();
        let fill = visuals.widgets.inactive.bg_fill;
        for (i, n) in self.graph.nodes.iter().enumerate() {
            let r = self.node_rect(rect, i);
            if !self.is_shown(i) || !view.intersects(r) {
                continue;
            }
            let accent = class_color(&n.class);
            let hidden = !state.doc.is_entity_visible(n.id);
            let alpha = if hidden { 0.4 } else { 1.0 };
            if !detail {
                painter.rect_filled(r, CornerRadius::same(1), accent.gamma_multiply(alpha));
                if selected[i] {
                    painter.rect_stroke(
                        r.expand(1.0),
                        CornerRadius::same(1),
                        Stroke::new(2.0, hp_app::theme::accent()),
                        egui::StrokeKind::Outside,
                    );
                }
                continue;
            }
            let radius = CornerRadius::same(5);
            painter.rect_filled(r, radius, fill.gamma_multiply(alpha));
            let bar = Rect::from_min_size(r.min, vec2(5.0 * self.zoom.max(0.6), r.height()));
            painter.rect_filled(bar, CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, accent.gamma_multiply(alpha));
            let border = if selected[i] {
                Stroke::new(2.0, hp_app::theme::accent())
            } else {
                Stroke::new(1.0, accent.gamma_multiply(0.5 * alpha))
            };
            painter.rect_stroke(r, radius, border, egui::StrokeKind::Inside);
            let tp = painter.with_clip_rect(r.shrink(2.0).intersect(rect));
            let mut left = r.left() + 10.0 * self.zoom.max(0.6);
            if let Some(texture) = icons.get(i).copied().flatten() {
                let side = (r.height() - 8.0 * self.zoom).max(4.0);
                let icon = Rect::from_min_size(pos2(left, r.center().y - side * 0.5), vec2(side, side));
                tp.image(
                    texture,
                    icon,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE.gamma_multiply(alpha),
                );
                left = icon.right() + 6.0 * self.zoom.max(0.6);
            }
            let (label, label_color) =
                if n.name.is_empty() { (n.class.as_str(), weak) } else { (n.name.as_str(), text_color) };
            if self.zoom < 0.6 {
                let size = (r.height() * 0.62).clamp(6.0, 16.0);
                tp.text(
                    pos2(left, r.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(size),
                    label_color.gamma_multiply(alpha),
                );
            } else {
                let name_size = (13.0 * self.zoom).clamp(8.0, 22.0);
                let class_size = (11.0 * self.zoom).clamp(7.0, 18.0);
                let name = if n.name.is_empty() { t!("lab.logic.unnamed") } else { n.name.as_str() };
                tp.text(
                    pos2(left, r.top() + r.height() * 0.32),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(name_size),
                    if n.name.is_empty() { weak } else { text_color }.gamma_multiply(alpha),
                );
                tp.text(
                    pos2(left, r.top() + r.height() * 0.72),
                    Align2::LEFT_CENTER,
                    &n.class,
                    FontId::proportional(class_size),
                    weak.gamma_multiply(alpha),
                );
            }
            if n.problems > 0 {
                let c = pos2(r.right() - 10.0 * self.zoom, r.top() + 10.0 * self.zoom);
                painter.circle_filled(c, 6.0 * self.zoom.max(0.6), edge_color(EdgeState::MissingTarget));
                painter.text(
                    c,
                    Align2::CENTER_CENTER,
                    "!",
                    FontId::proportional(9.0 * self.zoom.max(0.8)),
                    Color32::WHITE,
                );
            }
            painter.circle(
                self.out_port(rect, i),
                PORT_R,
                if n.outputs > 0 { accent } else { fill },
                Stroke::new(1.0, accent),
            );
            if n.inputs > 0 {
                painter.circle_filled(self.in_port(rect, i), PORT_R - 1.0, accent.gamma_multiply(0.8));
            }
        }
        let pointer = ui.input(|i| i.pointer.hover_pos());
        match (&self.drag, pointer) {
            (Drag::Box { start }, Some(p)) => {
                let r = Rect::from_two_pos(*start, p);
                painter.rect_filled(r, CornerRadius::ZERO, hp_app::theme::accent().gamma_multiply(0.12));
                painter.rect_stroke(
                    r,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, hp_app::theme::accent()),
                    egui::StrokeKind::Inside,
                );
            }
            (Drag::Link { from }, Some(p)) => {
                let a = self.out_port(rect, *from);
                let pts = curve_points(a, p, self.zoom, false, 16);
                painter.add(egui::Shape::line(pts, Stroke::new(2.0, hp_app::theme::accent())));
                if let Some(t) = self.node_at(rect, p) {
                    painter.rect_stroke(
                        self.node_rect(rect, t),
                        CornerRadius::same(5),
                        Stroke::new(2.0, hp_app::theme::accent()),
                        egui::StrokeKind::Outside,
                    );
                }
            }
            _ => {}
        }
    }

    fn start_new_link(&mut self, state: &AppState, from: usize, to: usize, at: Pos2) {
        let (Some(src), Some(dst)) = (self.graph.nodes.get(from), self.graph.nodes.get(to)) else { return };
        let fgd = state.assets.fgd();
        let pick = |list: &[hp_fgd::IoDef]| {
            list.iter()
                .find(|d| !GENERIC_IO.iter().any(|g| d.name.eq_ignore_ascii_case(g)))
                .or(list.first())
                .map(|d| d.name.clone())
        };
        let output = fgd
            .and_then(|f| f.class(&src.class))
            .and_then(|c| pick(&c.outputs))
            .unwrap_or_else(|| "OnTrigger".to_owned());
        let input = fgd.and_then(|f| f.class(&dst.class)).and_then(|c| pick(&c.inputs)).unwrap_or_default();
        let needs_name = dst.name.is_empty();
        let name = if needs_name { format!("{}_{}", dst.class, dst.id) } else { dst.name.clone() };
        self.new_link = Some(NewLink {
            from: src.id,
            to: dst.id,
            output,
            input,
            param: String::new(),
            delay: 0.0,
            once: false,
            name,
            needs_name,
            at,
        });
    }

    fn new_link_window(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(link) = self.new_link.as_mut() else { return };
        let fgd = state.assets.fgd();
        let class_of = |id: Id| state.doc.scene().entity(id).map(|n| n.entity.classname.clone()).unwrap_or_default();
        let (src_class, dst_class) = (class_of(link.from), class_of(link.to));
        let outputs: Vec<String> = fgd
            .and_then(|f| f.class(&src_class))
            .map(|c| c.outputs.iter().map(|o| o.name.clone()).collect())
            .unwrap_or_default();
        let inputs: Vec<String> = fgd
            .and_then(|f| f.class(&dst_class))
            .map(|c| c.inputs.iter().map(|o| o.name.clone()).collect())
            .unwrap_or_default();
        let mut done: Option<bool> = None;
        hp_app::chrome::Window::new(t!("lab.logic.new_output"))
            .id(egui::Id::unique(("lab_logic_new_output", self.salt)))
            .default_pos(link.at)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("lab_logic_new_output_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(t!("lab.logic.output"));
                    combo_edit(ui, "lab_logic_out", &mut link.output, &outputs);
                    ui.end_row();
                    ui.label(t!("lab.logic.target"));
                    if link.needs_name {
                        ui.add(egui::TextEdit::singleline(&mut link.name).desired_width(200.0))
                            .on_hover_text(t!("lab.logic.name_tip"));
                    } else {
                        ui.label(&link.name);
                    }
                    ui.end_row();
                    ui.label(t!("lab.logic.input"));
                    combo_edit(ui, "lab_logic_in", &mut link.input, &inputs);
                    ui.end_row();
                    ui.label(t!("lab.logic.param"));
                    ui.add(egui::TextEdit::singleline(&mut link.param).desired_width(200.0));
                    ui.end_row();
                    ui.label(t!("lab.logic.delay"));
                    ui.add(egui::DragValue::new(&mut link.delay).range(0.0..=3600.0).speed(0.05).suffix(" s"));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut link.once, t!("lab.logic.once"));
                    ui.end_row();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let ok =
                        !link.output.trim().is_empty() && !link.input.trim().is_empty() && !link.name.trim().is_empty();
                    if ui.add_enabled(ok, egui::Button::new(t!("common.add"))).clicked() {
                        done = Some(true);
                    }
                    if ui.button(t!("common.cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        done = Some(false);
                    }
                });
            });
        match done {
            Some(true) => {
                if let Some(link) = self.new_link.take() {
                    add_link(state, &link);
                }
            }
            Some(false) => self.new_link = None,
            None => {}
        }
    }

    fn arrange(&mut self, state: &mut AppState) {
        let n = self.graph.nodes.len();
        let positions = layout::layout(n, &self.graph.links(), &vec![None; n]);
        let moves: Vec<(Id, [f32; 2])> = self.graph.nodes.iter().zip(positions).map(|(node, p)| (node.id, p)).collect();
        write_positions(state, UNDO_ARRANGE, &moves);
        self.frame_all = true;
    }
}

pub struct LogicalQuadrants {
    views: [LogicalView; 4],
}

impl Default for LogicalQuadrants {
    fn default() -> Self {
        Self { views: std::array::from_fn(|i| LogicalView { salt: i + 1, ..LogicalView::default() }) }
    }
}

impl LogicalQuadrants {
    pub fn view(&self, index: usize) -> Option<&LogicalView> {
        self.views.get(index)
    }
}

impl hp_ui_viewports::logical::LogicalContent for LogicalQuadrants {
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut AppState,
        index: usize,
        header: &mut dyn FnMut(&mut egui::Ui, &mut AppState),
    ) {
        match self.views.get_mut(index) {
            Some(view) => view.quadrant_ui(ui, state, header),
            None => {
                ui.horizontal(|ui| header(ui, state));
            }
        }
    }
}

fn combo_edit(ui: &mut egui::Ui, id: &str, value: &mut String, choices: &[String]) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(170.0));
        ui.add_enabled_ui(!choices.is_empty(), |ui| {
            egui::ComboBox::from_id_salt(id).selected_text("").width(24.0).show_ui(ui, |ui| {
                for c in choices {
                    if ui.selectable_label(c == value, c).clicked() {
                        value.clone_from(c);
                    }
                }
            });
        });
    });
}

fn edge_text(e: &graph::Edge, g: &Graph) -> String {
    let target =
        e.to.and_then(|t| g.nodes.get(t))
            .map_or(e.target.as_str(), |n| if n.name.is_empty() { e.target.as_str() } else { n.name.as_str() });
    let mut s = format!("{} > {}.{}", e.output, target, e.input);
    if !e.param.is_empty() {
        s.push_str(&format!(" ({})", e.param));
    }
    if e.delay > 0.0 {
        s.push_str(&tf!("lab.logic.after", e.delay));
    }
    if e.times == 1 {
        s.push_str(&format!(" {}", t!("lab.logic.once_short")));
    }
    s
}

fn curve_points(a: Pos2, b: Pos2, zoom: f32, self_loop: bool, n: usize) -> Vec<Pos2> {
    if self_loop {
        let lift = 40.0 * zoom.max(0.3);
        let c1 = a + vec2(lift, -lift * 1.5);
        let c2 = b + vec2(-lift, -lift * 1.5);
        return bezier(a, c1, c2, b, n);
    }
    let dx = ((b.x - a.x).abs() * 0.5).max(50.0 * zoom.max(0.2));
    bezier(a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b, n)
}

fn bezier(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            let u = 1.0 - t;
            let v = p0.to_vec2() * (u * u * u)
                + p1.to_vec2() * (3.0 * u * u * t)
                + p2.to_vec2() * (3.0 * u * t * t)
                + p3.to_vec2() * (t * t * t);
            v.to_pos2()
        })
        .collect()
}

fn arrow_head(painter: &egui::Painter, pts: &[Pos2], color: Color32, size: f32) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let tip = pts[n - 1];
    let dir = (tip - pts[n - 2]).normalized();
    if !dir.x.is_finite() || !dir.y.is_finite() {
        return;
    }
    let side = vec2(-dir.y, dir.x);
    let base = tip - dir * size;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, base + side * size * 0.5, base - side * size * 0.5],
        color,
        Stroke::NONE,
    ));
}

fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 <= f32::EPSILON { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
    (a + ab * t).distance(p)
}

pub fn write_positions(state: &mut AppState, label: &str, moves: &[(Id, [f32; 2])]) {
    if moves.is_empty() {
        return;
    }
    state.doc.begin(label);
    for (id, p) in moves {
        let text = format_logical_pos(*p);
        let _ = state.doc.edit_entity(*id, |e| e.editor.logical_pos = Some(text));
    }
    state.doc.commit();
}

pub fn clear_positions(state: &mut AppState, ids: &[Id]) {
    if ids.is_empty() {
        return;
    }
    state.doc.begin(UNDO_MOVE);
    for id in ids {
        let _ = state.doc.edit_entity(*id, |e| e.editor.logical_pos = None);
    }
    state.doc.commit();
}

fn add_link(state: &mut AppState, link: &NewLink) {
    let connection = hp_vmf::Connection {
        output: link.output.trim().to_owned(),
        target: link.name.trim().to_owned(),
        input: link.input.trim().to_owned(),
        param: link.param.clone(),
        delay: link.delay.max(0.0),
        times: if link.once { 1 } else { -1 },
        separator: ',',
    };
    state.doc.begin("Add output");
    let named = if link.needs_name {
        state.doc.set_entity_key(&[link.to], "targetname", link.name.trim()).map(|_| ())
    } else {
        Ok(())
    };
    let result = named.and_then(|()| state.doc.add_connection(&[link.from], &connection).map(|_| ()));
    match result {
        Ok(()) => {
            state.doc.commit();
        }
        Err(e) => {
            state.doc.cancel();
            state.notify(hp_app::Severity::Warning, tf!("lab.logic.add_failed", e));
        }
    }
}

#[cfg(test)]
mod tests;
