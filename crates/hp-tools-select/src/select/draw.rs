use egui::{Align2, Color32, FontId, Key, Painter, Pos2, Rect, Stroke, StrokeKind};
use glam::DVec3;
use hp_app::settings::SelectElement;
use hp_app::theme::{VIEW_ACCENT, VIEW_SUCCESS};
use hp_app::{AppState, ViewInfo, t, tf};
use hp_geom::Aabb;

use super::{DragOp, QueryKind, SelectTool, screen_rect, signed};
use crate::gizmo::{GizmoLayout, GizmoMode, GizmoPart, axis_color};
use crate::input::ctrl;
use crate::math::{HANDLE_GAP, HandleMode, fmt_deg, fmt_len, handle_pos_sized, units_per_point_at};
use crate::modal::ModalKind;
use crate::preview::box_edges;
use crate::screen::{clipped_segment, paint_dashed_rect, paint_rect_outline, view_clip, visible_part};
use crate::stretch;

pub(super) const PREVIEW_COLOR: Color32 = Color32::from_rgb(255, 214, 90);
const MAX_CENTER_MARKS: usize = 4096;
const PIVOT_ARM: f32 = 9.0;

impl SelectTool {
    pub(super) fn paint_2d(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        let clip = view_clip(view.rect);
        let hsize = Self::handle_size(state);
        if self.centers_only {
            self.paint_centers(state, view, painter);
        }
        if let Some(d) = &self.drag
            && let DragOp::Band { start, current } = d.op
            && d.view == view.index
            && let Some(a) = view.world_to_screen(view.kind.from_2d(start, 0.0))
            && let Some(b) = view.world_to_screen(view.kind.from_2d(current, 0.0))
        {
            let rect = Rect::from_two_pos(a, b);
            let inside = self.box_inside ^ d.mods.alt;
            if let Some(fill) = visible_part(rect, clip) {
                painter.rect_filled(fill, 0.0, VIEW_ACCENT.gamma_multiply(0.08));
            }
            let stroke = Stroke::new(1.0, VIEW_ACCENT);
            if inside || self.centers_only {
                paint_rect_outline(painter, clip, rect, stroke);
            } else {
                paint_dashed_rect(painter, clip, rect, stroke, 5.0, 3.0);
            }
            let size = (current - start).abs();
            let text = format!("{} x {}", fmt_len(size.x), fmt_len(size.y));
            label(painter, rect.right_bottom() + egui::vec2(4.0, 4.0), Align2::LEFT_TOP, &text);
        }
        let Some(bounds) = self.selection_bounds(state) else { return };
        let sel_color = hp_app::theme::rgb(state.settings.view.selection_color);
        match &self.drag {
            Some(d) if d.transforms() => {
                let m = d.matrix();
                let color = if d.is_clone() { VIEW_SUCCESS } else { PREVIEW_COLOR };
                let rect = screen_rect(view, &d.bounds);
                let faint = Stroke::new(1.0, sel_color.gamma_multiply(0.6));
                paint_dashed_rect(painter, clip, rect, faint, 4.0, 4.0);
                let mut moved = Aabb::EMPTY;
                for [a, b] in box_edges(&d.bounds) {
                    let (a, b) = (m.transform_point3(a), m.transform_point3(b));
                    moved.extend(a);
                    moved.extend(b);
                    if let (Some(sa), Some(sb)) = (view.world_to_screen(a), view.world_to_screen(b)) {
                        clipped_segment(painter, clip, sa, sb, Stroke::new(1.5, color));
                    }
                }
                let rect = screen_rect(view, &moved);
                size_labels(painter, view, rect, &moved, hsize);
                let below = rect.center_bottom() + egui::vec2(0.0, HANDLE_GAP + hsize + 2.0);
                match &d.op {
                    DragOp::Rotate { angle, .. } | DragOp::GizmoRotate { angle, .. } => {
                        if let Some(c) = view.world_to_screen(d.pivot)
                            && clip.contains(c)
                        {
                            label(painter, c, Align2::CENTER_CENTER, &format!("{}°", fmt_deg(*angle)));
                        }
                    }
                    DragOp::Move { delta, .. } | DragOp::GizmoMove { delta, .. } => {
                        let d2 = view.kind.to_2d(*delta);
                        let mut text = format!("{} {}", signed(d2.x), signed(d2.y));
                        if d.is_clone() {
                            text = format!("{} {text}", t!("seltool.label.copy"));
                        }
                        label(painter, below, Align2::CENTER_TOP, &text);
                    }
                    DragOp::Skew { amount, .. } => {
                        label(painter, rect.center(), Align2::CENTER_CENTER, &signed(*amount));
                    }
                    DragOp::Modal(_) | DragOp::GizmoScale { .. } => {
                        label(painter, below, Align2::CENTER_TOP, &self.drag_value_text(d));
                    }
                    _ => {}
                }
            }
            _ => {
                let rect = screen_rect(view, &bounds);
                paint_dashed_rect(painter, clip, rect, Stroke::new(1.0, sel_color), 5.0, 3.0);
                let hovered = if self.hover.view == Some(view.index) { self.hover.handle } else { None };
                for &h in self.handle_mode.handles() {
                    let p = handle_pos_sized(rect, h, hsize);
                    if clip.contains(p) {
                        paint_handle(painter, p, self.handle_mode, hovered == Some(h), hsize);
                    }
                }
                size_labels(painter, view, rect, &bounds, hsize);
            }
        }
    }

    fn paint_centers(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        let Some((r, u, _)) = view.kind.axes_2d() else { return };
        let clip = view_clip(view.rect);
        let a = view.screen_to_2d(view.rect.min);
        let b = view.screen_to_2d(view.rect.max);
        let rect2 = [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)];
        let key = (view.index, state.doc.revision(), rect2.map(f64::to_bits));
        let mut cache = self.centers.borrow_mut();
        let slot = match cache.iter().position(|(k, _)| k.0 == view.index) {
            Some(i) => i,
            None => {
                cache.push(((view.index, u64::MAX, [0; 4]), Vec::new()));
                cache.len() - 1
            }
        };
        if cache[slot].0 != key {
            let mut lo = DVec3::splat(f64::NEG_INFINITY);
            let mut hi = DVec3::splat(f64::INFINITY);
            lo[r] = rect2[0];
            hi[r] = rect2[2];
            lo[u] = rect2[1];
            hi[u] = rect2[3];
            let area = Aabb { min: lo, max: hi };
            let doc = &state.doc;
            let mut points = Vec::new();
            for id in doc.solids_in_box(&area) {
                if points.len() >= MAX_CENTER_MARKS {
                    break;
                }
                if doc.is_solid_visible(id)
                    && let Some(b) = doc.scene().solid_bounds(id)
                {
                    points.push(b.center());
                }
            }
            for node in doc.scene().entities().values() {
                if points.len() >= MAX_CENTER_MARKS {
                    break;
                }
                let id = node.entity.id;
                if !doc.scene().is_brush_entity(id)
                    && doc.is_entity_visible(id)
                    && let Some(o) = node.entity.origin().filter(|o| area.contains_point(*o))
                {
                    points.push(o);
                }
            }
            cache[slot] = (key, points);
        }
        let points = &cache[slot].1;
        let stroke = Stroke::new(1.0, Color32::from_gray(200).gamma_multiply(0.7));
        for p in points {
            if let Some(s) = view.world_to_screen(*p).filter(|s| clip.contains(*s)) {
                painter.line_segment([s - egui::vec2(3.0, 3.0), s + egui::vec2(3.0, 3.0)], stroke);
                painter.line_segment([s - egui::vec2(3.0, -3.0), s + egui::vec2(3.0, -3.0)], stroke);
            }
        }
    }

    pub(super) fn paint_3d(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        if self.drag.is_some() {
            if !matches!(self.drag.as_ref().map(|d| &d.op), Some(DragOp::FaceStretch { .. })) {
                self.paint_gizmo(state, view, painter);
            }
            self.paint_stretch_drag(view, painter);
            return;
        }
        let handles = self.stretch_handles(state, view);
        let hot = if self.hover.view == Some(view.index) { self.hover.stretch } else { None };
        stretch::paint_stems(painter, &handles, hot);
        self.paint_gizmo(state, view, painter);
        stretch::paint_heads(painter, &handles, hot);
    }

    fn paint_stretch_drag(&self, view: &ViewInfo, painter: &Painter) {
        let Some(d) = &self.drag else { return };
        let DragOp::FaceStretch { axis, positive, new_bounds, gap, .. } = &d.op else { return };
        let (axis, positive) = (*axis % 3, *positive);
        let face = stretch::face_center(new_bounds, axis, positive);
        let clip = view_clip(view.rect);
        let reach = units_per_point_at(view, face) * f64::from(view.rect.size().max_elem());
        let stroke = Stroke::new(1.0, axis_color(axis).gamma_multiply(0.55));
        if let Some(c) = view.world_to_screen(face) {
            for sign in [-1.0, 1.0] {
                let mut e = crate::math::axis_vec(axis) * reach * sign;
                for _ in 0..12 {
                    if let Some(end) = view.world_to_screen(face + e) {
                        clipped_segment(painter, clip, c, end, stroke);
                        break;
                    }
                    e *= 0.5;
                }
            }
        }
        let size = new_bounds.size();
        let mut text = format!(
            "{} x {} x {}  ({})",
            fmt_len(size.x),
            fmt_len(size.y),
            fmt_len(size.z),
            signed(size[axis] - d.bounds.size()[axis])
        );
        if d.is_clone() {
            text = format!("{} {text}", t!("seltool.label.copy"));
        }
        match stretch::layout(view, new_bounds, &|_| false).iter().find(|h| h.id() == (axis, positive)) {
            Some(h) => {
                let h = h.with_gap(*gap);
                stretch::paint(painter, &h, true);
                let (at, align) = h.label_anchor();
                label(painter, at, align, &text);
            }
            None => label(painter, d.last_pos + egui::vec2(16.0, 16.0), Align2::LEFT_TOP, &text),
        }
    }

    fn paint_gizmo(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        if !self.gizmo_visible {
            return;
        }
        let Some(pivot) = self.gizmo_center(state) else { return };
        let (center, axes, active, mode) = match &self.drag {
            Some(d) if !d.transforms() => return,
            Some(d) => {
                let active = match d.op {
                    DragOp::GizmoMove { part, .. } | DragOp::GizmoScale { part, .. } => Some(part),
                    DragOp::GizmoRotate { axis, .. } => Some(GizmoPart::Ring(axis)),
                    _ => None,
                };
                let mode = match &d.op {
                    DragOp::GizmoRotate { .. } | DragOp::Rotate { .. } => GizmoMode::Rotate,
                    DragOp::GizmoScale { .. } => GizmoMode::Scale,
                    DragOp::Modal(m) => match m.kind {
                        ModalKind::Move => GizmoMode::Move,
                        ModalKind::Rotate => GizmoMode::Rotate,
                        ModalKind::Scale => GizmoMode::Scale,
                    },
                    _ => GizmoMode::Move,
                };
                let center = match d.op {
                    DragOp::GizmoScale { .. } => d.pivot,
                    _ => d.matrix().transform_point3(d.pivot),
                };
                (center, d.axes, active, mode)
            }
            None => {
                let hovered = if self.hover.view == Some(view.index) { self.hover.gizmo } else { None };
                (pivot, self.gizmo_axes(state), hovered, self.effective_gizmo_mode(state))
            }
        };
        let Some(layout) = GizmoLayout::with_axes(view, center, axes) else { return };
        let guide = match (&self.drag, active) {
            (Some(_), Some(GizmoPart::Axis(k))) => Some(k),
            (Some(d), _) => match &d.op {
                DragOp::Modal(m) => match m.lock {
                    crate::modal::Lock::Axis(k) => Some(k),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        if let Some(k) = guide {
            let e = layout.axes[k % 3] * layout.size * 12.0;
            if let (Some(a), Some(b)) = (view.world_to_screen(center - e), view.world_to_screen(center + e)) {
                let stroke = Stroke::new(1.0, axis_color(k).gamma_multiply(0.6));
                clipped_segment(painter, view_clip(view.rect), a, b, stroke);
            }
        }
        layout.paint(view, painter, mode, active);
        if let Some(d) = &self.drag
            && let Some(c) = view.world_to_screen(center)
        {
            let text = self.drag_value_text(d);
            if !text.is_empty() {
                label(painter, c + egui::vec2(14.0, 14.0), Align2::LEFT_TOP, &text);
            }
        }
    }

    fn drag_value_text(&self, d: &super::Drag) -> String {
        let copy = |s: String| if d.is_clone() { format!("{} {s}", t!("seltool.label.copy")) } else { s };
        match &d.op {
            DragOp::GizmoMove { delta, .. } | DragOp::Move { delta, .. } => copy(hp_app::widgets::fmt_vec3(*delta)),
            DragOp::GizmoRotate { angle, .. } | DragOp::Rotate { angle, .. } => copy(format!("{}°", fmt_deg(*angle))),
            DragOp::GizmoScale { scale, .. } => copy(fmt_factors(*scale)),
            DragOp::Modal(m) => {
                let value = match m.kind {
                    ModalKind::Move => hp_app::widgets::fmt_vec3(m.delta),
                    ModalKind::Rotate => format!("{}°", fmt_deg(m.angle)),
                    ModalKind::Scale => fmt_factors(m.scale),
                };
                let lock = m.lock.label();
                if lock.is_empty() { value } else { format!("{lock}: {value}") }
            }
            _ => String::new(),
        }
    }

    pub(super) fn paint_pivot(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        let p = match &self.drag {
            Some(super::Drag { op: DragOp::Pivot { pos }, .. }) => Some(*pos),
            Some(d) if d.transforms() => self.active_pivot(state).map(|p| d.matrix().transform_point3(p)),
            _ => self.active_pivot(state),
        };
        let Some(p) = p else { return };
        let clip = view_clip(view.rect);
        let dark = Stroke::new(3.5, Color32::from_black_alpha(200));
        let white = Stroke::new(1.5, Color32::WHITE);
        if view.is_2d() {
            let Some(s) = view.world_to_screen(p).filter(|s| clip.contains(*s)) else { return };
            let hovered = self.hover.pivot && self.hover.view == Some(view.index);
            let arm = if hovered { PIVOT_ARM + 2.0 } else { PIVOT_ARM };
            for stroke in [dark, white] {
                painter.line_segment([s - egui::vec2(arm, 0.0), s + egui::vec2(arm, 0.0)], stroke);
                painter.line_segment([s - egui::vec2(0.0, arm), s + egui::vec2(0.0, arm)], stroke);
            }
            painter.circle_stroke(s, 3.0, white);
            return;
        }
        let size = units_per_point_at(view, p) * f64::from(PIVOT_ARM) * 1.5;
        if !size.is_finite() || size <= 0.0 {
            return;
        }
        for k in 0..3 {
            let mut e = DVec3::ZERO;
            e[k] = size;
            if let (Some(a), Some(b)) = (view.world_to_screen(p - e), view.world_to_screen(p + e)) {
                clipped_segment(painter, clip, a, b, dark);
                clipped_segment(painter, clip, a, b, white);
            }
        }
    }

    pub(super) fn paint_helpers(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        let Some((id, mut list)) = self.helper_points(state) else { return };
        if let Some(super::Drag { op: DragOp::Helper { handle, pos, .. }, .. }) = &self.drag
            && let Some(h) = list.iter_mut().find(|h| h.key == handle.key && h.index == handle.index)
        {
            h.pos = *pos;
        }
        let clip = view_clip(view.rect);
        let color = Color32::from_rgb(255, 150, 40);
        let line = Stroke::new(1.0, color.gamma_multiply(0.8));
        let origin = state.doc.scene().entity(id).and_then(|n| n.entity.origin());
        for h in &list {
            let Some(s) = view.world_to_screen(h.pos) else { continue };
            match h.kind {
                crate::handles::HelperKind::VecLine => {
                    if let Some(o) = origin.and_then(|o| view.world_to_screen(o)) {
                        clipped_segment(painter, clip, o, s, line);
                    }
                }
                crate::handles::HelperKind::Axis if h.index == 1 => {
                    if let Some(a) = list.iter().find(|a| a.key == h.key && a.index == 0)
                        && let Some(sa) = view.world_to_screen(a.pos)
                    {
                        clipped_segment(painter, clip, sa, s, line);
                    }
                }
                _ => {}
            }
            if clip.contains(s) {
                let hovered =
                    self.hover.helper.as_deref() == Some(h.key.as_str()) && self.hover.view == Some(view.index);
                let r = if hovered { 5.5 } else { 4.0 };
                painter.circle(s, r, color, Stroke::new(1.0, Color32::from_black_alpha(220)));
            }
        }
    }

    pub(super) fn options(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("seltool.element")).on_hover_text(t!("seltool.element_tip"));
            for (element, action) in [
                (SelectElement::Object, hp_app::Action::SelectElementObject),
                (SelectElement::Vertex, hp_app::Action::SelectElementVertex),
                (SelectElement::Edge, hp_app::Action::SelectElementEdge),
                (SelectElement::Face, hp_app::Action::SelectElementFace),
            ] {
                let on = state.settings.selection.element == element;
                if ui
                    .add(egui::Button::selectable(on, t!(element.label_key())))
                    .on_hover_text(t!("seltool.element_tip"))
                    .clicked()
                {
                    state.request(action);
                }
            }
            hp_app::widgets::vsep(ui);
            ui.label(t!("seltool.box")).on_hover_text(t!("seltool.box_tip"));
            for (inside, key) in [(false, "seltool.box.touching"), (true, "seltool.box.inside")] {
                if ui
                    .add(egui::Button::selectable(!self.centers_only && self.box_inside == inside, t!(key)))
                    .on_hover_text(t!("seltool.box_tip"))
                    .clicked()
                {
                    self.box_inside = inside;
                    self.centers_only = false;
                }
            }
            if ui
                .add(egui::Button::selectable(self.centers_only, t!("seltool.box.centers")))
                .on_hover_text(t!("seltool.box.centers_tip"))
                .clicked()
            {
                self.centers_only = !self.centers_only;
            }
            hp_app::widgets::vsep(ui);
            ui.label(t!("seltool.handles")).on_hover_text(t!("seltool.handles_tip"));
            for mode in HandleMode::ALL {
                if ui
                    .add(egui::Button::selectable(self.handle_mode == mode, t!(mode.label_key())))
                    .on_hover_text(t!("seltool.handles_tip"))
                    .clicked()
                {
                    self.handle_mode = mode;
                }
            }
            let mut stretch = state.settings.selection.stretch_handles;
            if ui.checkbox(&mut stretch, t!("seltool.stretch")).on_hover_text(t!("seltool.stretch_tip")).changed() {
                state.settings.selection.stretch_handles = stretch;
                state.settings_dirty = true;
                self.hover.stretch = None;
            }
            hp_app::widgets::vsep(ui);
            ui.label(t!("seltool.gizmo")).on_hover_text(t!("seltool.gizmo_tip"));
            ui.checkbox(&mut self.gizmo_visible, t!("seltool.gizmo.show")).on_hover_text(t!("seltool.gizmo.show_tip"));
            let gizmo_on = self.gizmo_visible;
            {
                for mode in GizmoMode::ALL {
                    if ui
                        .add_enabled(gizmo_on, egui::Button::selectable(self.gizmo_mode == mode, t!(mode.label_key())))
                        .on_hover_text(t!("seltool.gizmo_tip"))
                        .clicked()
                    {
                        self.gizmo_mode = mode;
                    }
                }
                let local = state.settings.selection.gizmo_local;
                let tip = match state.keys.shortcut(hp_app::Action::ToggleGizmoLocal) {
                    Some(sc) => format!("{} ({})", t!("seltool.gizmo.local_tip"), hp_app::actions::shortcut_label(&sc)),
                    None => t!("seltool.gizmo.local_tip").to_owned(),
                };
                if ui
                    .add_enabled(gizmo_on, egui::Button::selectable(local, t!("seltool.gizmo.local")))
                    .on_hover_text(tip)
                    .clicked()
                {
                    state.request(hp_app::Action::ToggleGizmoLocal);
                }
            }
            hp_app::widgets::vsep(ui);
            let pivot = state.pivot.is_some();
            if ui
                .add(egui::Button::selectable(pivot, t!("seltool.pivot")))
                .on_hover_text(t!("seltool.pivot_tip"))
                .clicked()
            {
                state.request(hp_app::Action::TogglePivot);
            }
            hp_app::widgets::vsep(ui);
            for (kind, key) in
                [(QueryKind::Class, "seltool.query.class"), (QueryKind::Material, "seltool.query.material")]
            {
                if ui
                    .add(egui::Button::selectable(self.query_kind == kind, t!(key)))
                    .on_hover_text(t!("seltool.query.tip"))
                    .clicked()
                {
                    self.query_kind = kind;
                    self.query_result = None;
                }
            }
            let hint = match self.query_kind {
                QueryKind::Class => t!("seltool.query.hint_class"),
                QueryKind::Material => t!("seltool.query.hint_material"),
            };
            let resp = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text(hint).desired_width(140.0));
            if resp.changed() {
                self.query_result = None;
            }
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let has_text = !self.query.trim().is_empty();
            let clicked = ui
                .add_enabled(has_text, egui::Button::new(t!("seltool.query.select")))
                .on_hover_text(t!("seltool.query.tip"))
                .clicked();
            if (enter || clicked) && has_text {
                let add = ui.input(|i| ctrl(i.modifiers));
                self.run_query(state, add);
                if enter {
                    resp.request_focus();
                }
            }
            if let Some((n, time)) = self.query_result
                && state.frame.time - time < 5.0
            {
                if n == 0 {
                    ui.colored_label(hp_app::theme::warning(), t!("seltool.query.none"));
                } else {
                    ui.colored_label(hp_app::theme::success(), tf!("seltool.query.found", n));
                }
            }
        });
    }

    pub(super) fn hint(&self, state: &AppState) -> Option<String> {
        if let Some(d) = &self.drag {
            return Some(match &d.op {
                DragOp::Band { .. } => {
                    if self.centers_only {
                        t!("seltool.hint.band_centers").to_owned()
                    } else if self.box_inside ^ d.mods.alt {
                        t!("seltool.hint.band_inside").to_owned()
                    } else {
                        t!("seltool.hint.band_touching").to_owned()
                    }
                }
                DragOp::Paint { remove, .. } => {
                    if *remove {
                        t!("seltool.hint.paint_remove").to_owned()
                    } else {
                        t!("seltool.hint.paint_add").to_owned()
                    }
                }
                DragOp::Pivot { pos } => tf!("seltool.hint.pivot_drag", hp_app::widgets::fmt_vec3(*pos)),
                DragOp::Helper { handle, pos, .. } => {
                    tf!("seltool.hint.helper_drag", handle.key, hp_app::widgets::fmt_vec3(*pos))
                }
                DragOp::Move { delta, .. } | DragOp::GizmoMove { delta, .. } => {
                    let key = if d.is_clone() { "seltool.hint.clone" } else { "seltool.hint.move" };
                    tf!(key, hp_app::widgets::fmt_vec3(*delta))
                }
                DragOp::Scale { new_bounds, .. } | DragOp::FaceStretch { new_bounds, .. } => {
                    let s = new_bounds.size();
                    let size = format!("{} x {} x {}", fmt_len(s.x), fmt_len(s.y), fmt_len(s.z));
                    let key = match (matches!(d.op, DragOp::FaceStretch { .. }), d.is_clone()) {
                        (true, true) => "seltool.hint.stretch_clone",
                        (true, false) => "seltool.hint.stretch_drag",
                        (false, true) => "seltool.hint.scale_clone",
                        (false, false) => "seltool.hint.scale",
                    };
                    tf!(key, size)
                }
                DragOp::GizmoScale { scale, .. } => {
                    let key = if d.is_clone() { "seltool.hint.scale_clone" } else { "seltool.hint.scale" };
                    tf!(key, fmt_factors(*scale))
                }
                DragOp::Rotate { angle, .. } | DragOp::GizmoRotate { angle, .. } => {
                    let key = if d.is_clone() { "seltool.hint.rotate_clone" } else { "seltool.hint.rotate" };
                    tf!(key, fmt_deg(*angle), self.free_rotation_key(state))
                }
                DragOp::Skew { amount, .. } => {
                    let key = if d.is_clone() { "seltool.hint.skew_clone" } else { "seltool.hint.skew" };
                    tf!(key, signed(*amount))
                }
                DragOp::Modal(m) => tf!(m.kind.hint_key(), self.drag_value_text(d)),
            });
        }
        if let Some((text, time)) = &self.wheel_note
            && state.frame.time - time < 2.0
        {
            return Some(text.clone());
        }
        if self.hover.pivot {
            return Some(t!("seltool.hint.pivot").to_owned());
        }
        if let Some(key) = &self.hover.helper {
            return Some(tf!("seltool.hint.helper", key));
        }
        if let Some((axis, positive)) = self.hover.stretch {
            let name = format!("{}{}", if positive { "+" } else { "-" }, ["X", "Y", "Z"][axis % 3]);
            return Some(tf!("seltool.hint.stretch", name));
        }
        if self.hover.gizmo.is_some() {
            return Some(t!("seltool.hint.gizmo").to_owned());
        }
        if let Some(c) = self.live_cycle(state)
            && self.hover.view == Some(c.view)
        {
            return Some(tf!("seltool.hint.cycle", c.index + 1, c.candidates.len()));
        }
        let in_3d = self.hover.view.and_then(|i| self.kind_of(state, i)).is_some_and(|k| !k.is_2d());
        if in_3d {
            return Some(t!("seltool.hint.3d").to_owned());
        }
        if state.doc.selection().objects.is_empty() {
            Some(t!("seltool.hint.idle").to_owned())
        } else {
            Some(tf!("seltool.hint.selected", t!(self.handle_mode.label_key())))
        }
    }

    fn free_rotation_key(&self, state: &AppState) -> &'static str {
        if state.settings.selection.free_rotation_with_alt { t!("seltool.key.alt") } else { t!("seltool.key.ctrl") }
    }
}

fn fmt_factors(s: DVec3) -> String {
    let f = |v: f64| {
        let r = (v * 100.0).round() / 100.0;
        format!("{r}")
    };
    if (s.x - s.y).abs() < 1e-9 && (s.y - s.z).abs() < 1e-9 {
        format!("x{}", f(s.x))
    } else {
        format!("x{} x{} x{}", f(s.x), f(s.y), f(s.z))
    }
}

pub(super) fn label(painter: &Painter, pos: Pos2, anchor: Align2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(11.0), Color32::from_gray(235));
    let rect = anchor.anchor_size(pos, galley.size()).expand2(egui::vec2(3.0, 1.0));
    painter.rect_filled(rect, 3.0, Color32::from_black_alpha(170));
    painter.galley(rect.min + egui::vec2(3.0, 1.0), galley, Color32::from_gray(235));
}

fn size_labels(painter: &Painter, view: &ViewInfo, rect: Rect, b: &Aabb, handle_size: f32) {
    let Some((r, u, _)) = view.kind.axes_2d() else { return };
    if !rect.is_finite() {
        return;
    }
    let Some(vis) = visible_part(rect, view.rect) else { return };
    let size = b.size();
    let off = HANDLE_GAP + handle_size + 3.0;
    let clip = view_clip(view.rect);
    let top = Pos2::new(vis.center().x, rect.top() - off);
    if clip.contains(top) {
        label(painter, top, Align2::CENTER_BOTTOM, &fmt_len(size[r]));
    }
    let left = Pos2::new(rect.left() - off, vis.center().y);
    if clip.contains(left) {
        label(painter, left, Align2::RIGHT_CENTER, &fmt_len(size[u]));
    }
}

fn paint_handle(painter: &Painter, p: Pos2, mode: HandleMode, hovered: bool, size: f32) {
    let fill = if hovered { VIEW_ACCENT } else { Color32::WHITE };
    let stroke = Stroke::new(1.0, Color32::from_black_alpha(220));
    let h = size * 0.5;
    match mode {
        HandleMode::Scale => {
            painter.rect(Rect::from_center_size(p, egui::vec2(size, size)), 1.0, fill, stroke, StrokeKind::Middle);
        }
        HandleMode::Rotate => {
            painter.circle(p, h + 0.5, fill, stroke);
        }
        HandleMode::Skew => {
            let pts = vec![
                p + egui::vec2(0.0, -h - 1.0),
                p + egui::vec2(h + 1.0, 0.0),
                p + egui::vec2(0.0, h + 1.0),
                p + egui::vec2(-h - 1.0, 0.0),
            ];
            painter.add(egui::Shape::convex_polygon(pts, fill, stroke));
        }
    }
}
