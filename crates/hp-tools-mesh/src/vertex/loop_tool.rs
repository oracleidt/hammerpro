use std::collections::HashSet;

use egui::{Color32, Key, Modifiers, Pos2};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::state::AppState;
use hp_app::{Action, Severity, ViewInfo, t, tf};

use super::loops::{
    EdgeCmd, EdgeIssue, EdgeSummary, counts, pick_edge, run_edge_command, run_segments, seed_of, toward_for,
};
use super::model::SolidEdit;
use super::xform::{RotateAxis, SlideIssue, UNDO_ROTATE, UNDO_SLIDE, XformState};
use super::{VertexTool, with_shortcut};
use crate::common::{line_on_top, point, rgba, vertex_error_text};

pub(super) const LOOP_HOVER: Color32 = Color32::from_rgb(90, 220, 255);
const WHEEL_ROTATE_DEG: f64 = 5.0;
const WHEEL_SLIDE: f64 = 0.05;

#[derive(Clone, Debug)]
pub(super) struct LoopHover {
    pub view: usize,
    pub generation: u64,
    pub key: (usize, usize, bool),
    pub segments: Vec<[DVec3; 2]>,
}

pub(super) fn edge_action_cmd(action: Action) -> Option<EdgeCmd> {
    Some(match action {
        Action::EdgeSelectLoop => EdgeCmd::Loop,
        Action::EdgeSelectRing => EdgeCmd::Ring,
        Action::EdgeGrow => EdgeCmd::Grow,
        Action::EdgeShrink => EdgeCmd::Shrink,
        Action::EdgeSelectBoundary => EdgeCmd::Boundary,
        _ => return None,
    })
}

impl VertexTool {
    pub(super) fn update_loop_hover(&mut self, view: &ViewInfo, pos: Option<Pos2>, mods: Modifiers) {
        let busy = self.drag.is_some() || self.rubber.is_some() || self.scale.is_some() || self.xform.is_some();
        let Some(pos) = pos.filter(|_| mods.alt && !busy) else {
            if self.loop_hover.as_ref().is_some_and(|h| h.view == view.index) || !mods.alt {
                self.loop_hover = None;
            }
            return;
        };
        let cache = self.cache.get_mut();
        let Some(pick) = pick_edge(&cache.solids, view, pos) else {
            self.loop_hover = None;
            return;
        };
        let ring = mods.command || mods.ctrl;
        let key = (pick.0, pick.1, ring);
        if self.loop_hover.as_ref().is_some_and(|h| h.key == key && h.generation == cache.generation) {
            return;
        }
        let Some(seed) = seed_of(&cache.solids, pick) else { return };
        let toward = toward_for(view, (seed.1 + seed.2) * 0.5);
        let cmd = if ring { EdgeCmd::Ring } else { EdgeCmd::Loop };
        let segments = run_segments(&cache.solids, cmd, seed, Some(toward));
        self.loop_hover = Some(LoopHover { view: view.index, generation: cache.generation, key, segments });
    }

    pub(super) fn alt_click(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        let ring = mods.command || mods.ctrl;
        let cmd = if ring { EdgeCmd::Ring } else { EdgeCmd::Loop };
        let cache = self.cache.get_mut();
        let Some(pick) = pick_edge(&cache.solids, view, pos) else {
            state.log(Severity::Info, t!("vertex.loop.no_edge").to_owned());
            return;
        };
        let Some(seed) = seed_of(&cache.solids, pick) else { return };
        let toward = toward_for(view, (seed.1 + seed.2) * 0.5);
        let result = run_edge_command(&mut cache.solids, cmd, &[seed], Some(toward), mods.shift);
        self.loop_hover = None;
        if let Ok(sum) = result {
            self.note_edges(state, cmd, sum);
        }
    }

    fn note_edges(&mut self, state: &mut AppState, cmd: EdgeCmd, sum: EdgeSummary) {
        let text = match cmd {
            EdgeCmd::Loop if sum.closed => tf!("vertex.loop.done_closed", sum.edges, sum.vertices),
            EdgeCmd::Loop => tf!("vertex.loop.done_loop", sum.edges, sum.vertices),
            EdgeCmd::Ring => tf!("vertex.loop.done_ring", sum.edges, sum.vertices),
            _ => tf!("vertex.loop.done_set", sum.edges, sum.vertices),
        };
        state.log(Severity::Info, text);
    }

    fn view_toward(&mut self, state: &AppState) -> Option<DVec3> {
        let view = self.nudge_view(state).or(self.last_view)?;
        let center = self.cache.get_mut().selection_center()?;
        Some(toward_for(&view, center))
    }

    pub(crate) fn edge_command(
        &mut self,
        state: &mut AppState,
        cmd: EdgeCmd,
        seeds: &[super::loops::Seed],
        toward: Option<DVec3>,
        additive: bool,
    ) -> Result<EdgeSummary, EdgeIssue> {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        let result = run_edge_command(&mut cache.solids, cmd, seeds, toward, additive);
        self.loop_hover = None;
        match result {
            Ok(sum) => {
                self.note_edges(state, cmd, sum);
                Ok(sum)
            }
            Err(e) => {
                let text = if matches!(cmd, EdgeCmd::Loop | EdgeCmd::Ring) {
                    t!("vertex.loop.need_edge")
                } else {
                    t!("vertex.loop.need_vertices")
                };
                self.warn(state, text.to_owned());
                Err(e)
            }
        }
    }

    pub(super) fn edge_action(&mut self, state: &mut AppState, action: Action) -> bool {
        if let Some(cmd) = edge_action_cmd(action) {
            let request = state.runtime.mesh.edge_request.take();
            let exact = request.as_ref().is_some_and(|r| r.exact);
            let (seeds, toward, additive) = match request {
                Some(r) => (r.seeds, r.toward, r.additive),
                None => (Vec::new(), self.view_toward(state), false),
            };
            let cmd = if exact { EdgeCmd::Set } else { cmd };
            let _ = self.edge_command(state, cmd, &seeds, toward, additive);
            return true;
        }
        match action {
            Action::EdgeSlide => {
                self.start_slide(state);
                true
            }
            Action::VertexRotate => {
                self.start_rotate(state);
                true
            }
            _ => false,
        }
    }

    pub(super) fn start_rotate(&mut self, state: &mut AppState) -> bool {
        self.drag = None;
        self.rubber = None;
        self.scale = None;
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        match XformState::rotate(&state.doc, cache) {
            Some(x) => {
                self.xform = Some(x);
                self.hover = None;
                true
            }
            None => {
                self.warn(state, t!("vertex.rotate.need").to_owned());
                false
            }
        }
    }

    pub(super) fn start_slide(&mut self, state: &mut AppState) -> bool {
        self.drag = None;
        self.rubber = None;
        self.scale = None;
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        match XformState::slide(&state.doc, cache) {
            Ok(x) => {
                self.xform = Some(x);
                self.hover = None;
                true
            }
            Err(e) => {
                let text = match e {
                    SlideIssue::Nothing => t!("vertex.slide.need"),
                    SlideIssue::Branching => t!("vertex.slide.branching"),
                };
                self.warn(state, text.to_owned());
                false
            }
        }
    }

    pub(super) fn apply_xform(&mut self, state: &mut AppState) -> bool {
        let Some(x) = self.xform.as_ref() else { return false };
        if !x.has_motion() {
            self.xform = None;
            return false;
        }
        if let Some(err) = x.first_error() {
            let text = tf!("vertex.cancelled", vertex_error_text(err));
            self.warn(state, text);
            return false;
        }
        let label = if x.is_rotate() { UNDO_ROTATE } else { UNDO_SLIDE };
        let (edits, reselect) = x.edits();
        self.xform = None;
        self.apply(state, &edits, reselect, label, None)
    }

    pub(super) fn xform_wheel(&mut self, notches: f32, mods: Modifiers) -> bool {
        let Some(x) = self.xform.as_mut() else { return false };
        if !mods.alt || !notches.is_finite() || notches == 0.0 {
            return false;
        }
        let fine = mods.ctrl || mods.command;
        let step = if x.is_rotate() {
            if fine { WHEEL_ROTATE_DEG / 5.0 } else { WHEEL_ROTATE_DEG }
        } else if fine {
            WHEEL_SLIDE / 5.0
        } else {
            WHEEL_SLIDE
        };
        let v = ((x.value() + f64::from(notches) * step) * 1000.0).round() / 1000.0;
        x.set_value(v);
        true
    }

    pub(super) fn xform_gesture(&mut self, g: crate::common::Gesture) -> hp_app::ToolFlow {
        use crate::common::Gesture;
        match g {
            Gesture::Hover(_) | Gesture::Away => hp_app::ToolFlow::Ignored,
            _ => hp_app::ToolFlow::Handled,
        }
    }

    pub(super) fn xform_overlay(&self, out: &mut hp_render::Overlay) {
        let Some(x) = self.xform.as_ref() else { return };
        Self::draw_previews(out, &x.solids, &x.previews);
        if let Some(c) = x.center() {
            point(out, c, rgba(super::PREVIEW), 6.0);
            let len = 32.0;
            let dir = match x.op {
                super::xform::LoopOp::Rotate { axis, .. } => axis,
                _ => DVec3::Z,
            };
            let mut color = rgba(super::PREVIEW);
            color[3] = 0.6;
            line_on_top(out, c - dir * len, c + dir * len, color);
        }
    }

    pub(super) fn xform_paint(
        &self,
        view: &ViewInfo,
        painter: &egui::Painter,
        selection: Option<DVec3>,
        font: egui::FontId,
    ) {
        let Some(x) = self.xform.as_ref() else { return };
        let Some(at) = x.center().or(selection).and_then(|c| view.world_to_screen(c)) else { return };
        let (text, color) = match x.first_error() {
            Some(e) => (vertex_error_text(e), hp_app::theme::VIEW_ERROR),
            None if x.is_rotate() => (format!("{:.1} °", x.value()), super::PREVIEW),
            None => (format!("{:.2}", x.value()), super::PREVIEW),
        };
        painter.text(at + egui::vec2(12.0, -10.0), egui::Align2::LEFT_BOTTOM, text, font, color);
    }

    pub(super) fn xform_hint(&self) -> Option<String> {
        let x = self.xform.as_ref()?;
        if let Some(e) = x.first_error() {
            return Some(tf!("vertex.hint_invalid", vertex_error_text(e)));
        }
        let value = format!("{:.2}", x.value());
        Some(if x.is_rotate() {
            tf!("vertex.hint_rotate", value, key_label(Key::Enter), key_label(Key::Escape))
        } else {
            tf!("vertex.hint_slide", value, key_label(Key::Enter), key_label(Key::Escape))
        })
    }

    pub(super) fn xform_options(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let Some(x) = self.xform.as_ref() else { return };
        let rotate = x.is_rotate();
        let mut value = x.value();
        let mut center = x.center();
        let mut axis = x.axis;
        let error = x.first_error().map(vertex_error_text);
        let range = x.range();
        let mut apply = false;
        let mut cancel = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong(if rotate { t!("vertex.rotate.title") } else { t!("vertex.slide.title") });
            ui.separator();
            if rotate {
                ui.label(t!("vertex.rotate.angle")).on_hover_text(t!("vertex.rotate.angle_tip"));
                ui.add(egui::DragValue::new(&mut value).range(range.clone()).speed(0.5).max_decimals(2).suffix(" °"))
                    .on_hover_text(t!("vertex.rotate.angle_tip"));
                ui.label(t!("vertex.rotate.axis")).on_hover_text(t!("vertex.rotate.axis_tip"));
                for a in RotateAxis::ALL {
                    let label = match a {
                        RotateAxis::X => "X",
                        RotateAxis::Y => "Y",
                        RotateAxis::Z => "Z",
                        RotateAxis::Normal => t!("vertex.rotate.normal"),
                    };
                    if ui
                        .add(egui::Button::selectable(axis == a, label))
                        .on_hover_text(t!("vertex.rotate.axis_tip"))
                        .clicked()
                    {
                        axis = a;
                    }
                }
                if let Some(c) = center.as_mut() {
                    ui.label(t!("vertex.rotate.center")).on_hover_text(t!("vertex.rotate.center_tip"));
                    hp_app::widgets::vec3_edit(ui, c, 1.0);
                }
            } else {
                ui.label(t!("vertex.slide.amount")).on_hover_text(t!("vertex.slide.amount_tip"));
                ui.add(egui::Slider::new(&mut value, range.clone()).max_decimals(3))
                    .on_hover_text(t!("vertex.slide.amount_tip"));
            }
            ui.separator();
            let tip = format!("{} ({})", t!("vertex.scale_apply_tip"), key_label(Key::Enter));
            if ui.add_enabled(error.is_none(), egui::Button::new(t!("common.apply"))).on_hover_text(tip).clicked() {
                apply = true;
            }
            if ui.button(t!("common.cancel")).on_hover_text(key_label(Key::Escape)).clicked() {
                cancel = true;
            }
            if let Some(e) = &error {
                ui.colored_label(hp_app::theme::error(), e);
            }
        });
        if let Some(x) = self.xform.as_mut() {
            if rotate {
                if axis != x.axis {
                    x.set_axis(axis);
                }
                if let Some(c) = center {
                    x.set_center(c);
                }
            }
            x.set_value(value);
        }
        if apply {
            self.apply_xform(state);
        }
        if cancel {
            self.xform = None;
        }
    }

    pub(super) fn loop_options(&mut self, ui: &mut egui::Ui, state: &AppState, clicks: &mut Vec<Action>) {
        let cache = self.cache.get_mut();
        let any_edges = cache.solids.iter().any(|s| s.edge_selected.iter().any(|f| *f));
        let any = cache.selected_count() > 0;
        let empty = cache.solids.is_empty();
        let counted = any_edges.then(|| counts(&cache.solids));
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("vertex.loop.title")).on_hover_text(t!("vertex.loop.hint_pick"));
            let entries: [(Action, bool, &str); 7] = [
                (Action::EdgeSelectLoop, any, "vertex.loop.loop_tip"),
                (Action::EdgeSelectRing, any, "vertex.loop.ring_tip"),
                (Action::EdgeGrow, any, "vertex.loop.grow_tip"),
                (Action::EdgeShrink, any, "vertex.loop.shrink_tip"),
                (Action::EdgeSelectBoundary, any, "vertex.loop.boundary_tip"),
                (Action::EdgeSlide, any_edges, "vertex.loop.slide_tip"),
                (Action::VertexRotate, any, "vertex.loop.rotate_tip"),
            ];
            for (action, enabled, tip) in entries {
                let tip = with_shortcut(state, t!(tip), action);
                if ui
                    .add_enabled(enabled && !empty, egui::Button::new(action.label()))
                    .on_hover_text(&tip)
                    .on_disabled_hover_text(&tip)
                    .clicked()
                {
                    clicks.push(action);
                }
            }
            if let Some((e, v)) = counted {
                ui.label(tf!("vertex.loop.selected_counts", e, v));
            }
        });
    }

    pub(super) fn loop_status(&self) -> Option<String> {
        let cache = self.cache.borrow();
        if !cache.solids.iter().any(|s| s.edge_selected.iter().any(|f| *f)) {
            return None;
        }
        let (e, v) = counts(&cache.solids);
        Some(tf!("vertex.loop.counts", e, v))
    }

    pub(super) fn draw_loop_hover(&self, out: &mut hp_render::Overlay) {
        let Some(h) = self.loop_hover.as_ref() else { return };
        let color = rgba(LOOP_HOVER);
        for [a, b] in &h.segments {
            line_on_top(out, *a, *b, color);
        }
    }
}

pub(super) fn edge_followers(solids: &[super::model::EditSolid], edits: &[SolidEdit]) -> Vec<(hp_editor::Id, DVec3)> {
    let mut out = Vec::new();
    let mut seen: HashSet<hp_editor::Id> = HashSet::new();
    for e in edits {
        if !seen.insert(e.id) {
            continue;
        }
        let Some(s) = solids.iter().find(|s| s.id == e.id) else { continue };
        for (ei, sel) in s.edge_selected.iter().enumerate() {
            if !*sel {
                continue;
            }
            let Some([a, b]) = s.edges.get(ei).copied() else { continue };
            if let (Some(pa), Some(pb)) = (e.points.get(a), e.points.get(b)) {
                out.push((e.id, (*pa + *pb) * 0.5));
            }
        }
    }
    out
}

#[cfg(test)]
impl VertexTool {
    pub(crate) fn test_start_rotate(&mut self, state: &mut AppState) -> bool {
        self.start_rotate(state)
    }

    pub(crate) fn test_start_slide(&mut self, state: &mut AppState) -> bool {
        self.start_slide(state)
    }

    pub(crate) fn test_xform_value(&mut self, v: f64) -> bool {
        self.xform.as_mut().is_some_and(|x| x.set_value(v))
    }

    pub(crate) fn test_xform_axis(&mut self, axis: RotateAxis) {
        if let Some(x) = self.xform.as_mut() {
            x.set_axis(axis);
        }
    }

    pub(crate) fn test_xform_error(&self) -> bool {
        self.xform.as_ref().is_some_and(|x| x.first_error().is_some())
    }

    pub(crate) fn test_apply_xform(&mut self, state: &mut AppState) -> bool {
        self.apply_xform(state)
    }

    pub(crate) fn test_loop_hover(&self) -> usize {
        self.loop_hover.as_ref().map_or(0, |h| h.segments.len())
    }

    pub(crate) fn test_edge_counts(&mut self) -> (usize, usize) {
        counts(&self.cache.get_mut().solids)
    }

    pub(crate) fn test_status(&self) -> Option<String> {
        self.loop_status()
    }
}
