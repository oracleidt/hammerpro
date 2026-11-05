use egui::{Align2, CursorIcon, Key, Pos2, Ui};
use glam::DVec3;
use hp_app::actions::{Action, key_label, shortcut_label};
use hp_app::grid::GridFrame;
use hp_app::i18n::TrTable;
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::{Document, Id};
use hp_geom::{Aabb, Plane};
use hp_render::Overlay;

use crate::clip::consume_letter;
use crate::common::{
    COLOR_PENDING, Mouse, Ov, active_material, consume_plain_key, consume_tabs, fmt_num, paint_label,
    plane_box_polygon, rgba, round_to,
};

pub const MAX_SEGMENTS: u32 = 64;

const MIN_PIECE: f64 = 0.25;

const MAX_LABELS: usize = 24;

pub const SLICE_TR: TrTable = &[
    ("slice.left_right", "left to right", "de gauche à droite"),
    ("slice.top_bottom", "top to bottom", "de haut en bas"),
    (
        "slice.hint",
        "Slice {0}: {1} segment(s), position {2}, {3} cut(s) for {4} block(s). Click: slice. Wheel: segments. Right click: center. {5}: switch left to right / top to bottom. {6}: reset.",
        "Tranche {0} : {1} segment(s), position {2}, {3} coupe(s) pour {4} bloc(s). Clic : trancher. Molette : segments. Clic droit : centrer. {5} : alterner de gauche à droite / de haut en bas. {6} : réinitialiser.",
    ),
    (
        "slice.hint_wheel_2d",
        "Slice {0}: {1} segment(s), position {2}, {3} cut(s) for {4} block(s). Click: slice. Shift+wheel: segments. Right click: center. {5}: switch left to right / top to bottom. {6}: reset.",
        "Tranche {0} : {1} segment(s), position {2}, {3} coupe(s) pour {4} bloc(s). Clic : trancher. Maj+molette : segments. Clic droit : centrer. {5} : alterner de gauche à droite / de haut en bas. {6} : réinitialiser.",
    ),
    (
        "slice.hint_none",
        "Select the brushes to slice first. {0}: switch left to right / top to bottom. Wheel: segments.",
        "Sélectionnez d'abord les brushes à trancher. {0} : alterner de gauche à droite / de haut en bas. Molette : segments.",
    ),
    ("slice.center", "{0} (center)", "{0} (centre)"),
    ("slice.no_selection", "Select the brushes to slice first", "Sélectionnez d'abord les brushes à trancher"),
    (
        "slice.nothing",
        "No cut inside the selection: move the cursor away from the edge of the object",
        "Aucune coupe dans la sélection : éloignez le curseur du bord de l'objet",
    ),
    ("slice.done", "Sliced {0} brush(es) into {1} block(s)", "{0} brush(es) tranché(s) en {1} bloc(s)"),
    ("slice.failed", "Slicing failed: {0}", "La tranche a échoué : {0}"),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    LeftRight,
    TopBottom,
}

impl Orientation {
    pub fn id(self) -> &'static str {
        match self {
            Orientation::LeftRight => "left_right",
            Orientation::TopBottom => "top_bottom",
        }
    }

    pub fn parse(text: &str) -> Option<Orientation> {
        match text.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "left_right" | "lr" | "horizontal" => Some(Orientation::LeftRight),
            "top_bottom" | "tb" | "vertical" => Some(Orientation::TopBottom),
            _ => None,
        }
    }

    pub fn toggled(self) -> Orientation {
        match self {
            Orientation::LeftRight => Orientation::TopBottom,
            Orientation::TopBottom => Orientation::LeftRight,
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Orientation::LeftRight => "slice.left_right",
            Orientation::TopBottom => "slice.top_bottom",
        }
    }
}

fn column(frame: &GridFrame, axis: usize) -> DVec3 {
    match axis {
        0 => frame.axes.x_axis,
        1 => frame.axes.y_axis,
        _ => frame.axes.z_axis,
    }
}

pub fn orientation_axis(frame: &GridFrame, orientation: Orientation, right: DVec3, up: DVec3) -> usize {
    let along = |v: DVec3, skip: Option<usize>| {
        let mut best = None;
        for j in 0..3 {
            if skip == Some(j) {
                continue;
            }
            let d = column(frame, j).dot(v).abs();
            if best.is_none_or(|(_, b)| d > b + 1e-12) {
                best = Some((j, d));
            }
        }
        best.map_or(0, |(j, _)| j)
    };
    let vertical = along(up, None);
    match orientation {
        Orientation::TopBottom => vertical,
        Orientation::LeftRight => along(right, Some(vertical)),
    }
}

pub fn view_basis(kind: ViewKind, camera_right: DVec3) -> (DVec3, DVec3) {
    match kind.axes_2d() {
        Some((r, u, _)) => {
            let (mut right, mut up) = (DVec3::ZERO, DVec3::ZERO);
            right[r] = 1.0;
            up[u] = 1.0;
            (right, up)
        }
        None => {
            let flat = DVec3::new(camera_right.x, camera_right.y, 0.0).try_normalize().unwrap_or(DVec3::X);
            (flat, DVec3::Z)
        }
    }
}

pub fn snap_position(frame: &GridFrame, axis: usize, s: f64) -> f64 {
    let n = column(frame, axis);
    let step = frame.step[axis.min(2)];
    if !(s.is_finite() && step.is_finite() && step > 0.0) {
        return s;
    }
    let base = frame.origin.dot(n);
    crate::common::clean(base + ((s - base) / step).round() * step)
}

fn extent(doc: &Document, frame: &GridFrame, ids: &[Id], normal: DVec3) -> Option<(f64, f64, Aabb)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut bounds = Aabb::EMPTY;
    let rotated = frame.is_rotated();
    for id in ids {
        let Some(node) = doc.scene().solid(*id) else { continue };
        bounds = bounds.union(&node.bounds);
        if rotated {
            for f in &node.geom.faces {
                for v in &f.vertices {
                    let s = v.dot(normal);
                    lo = lo.min(s);
                    hi = hi.max(s);
                }
            }
        } else {
            for c in node.bounds.corners() {
                let s = c.dot(normal);
                lo = lo.min(s);
                hi = hi.max(s);
            }
        }
    }
    (lo.is_finite() && hi.is_finite() && hi > lo && !bounds.is_empty()).then_some((lo, hi, bounds))
}

fn positions(lo: f64, hi: f64, cursor: Option<f64>, segments: u32) -> Vec<f64> {
    let n = segments.clamp(1, MAX_SEGMENTS);
    let mut out: Vec<f64> = match cursor {
        None => (1..=n).map(|k| lo + (hi - lo) * f64::from(k) / f64::from(n + 1)).collect(),
        Some(c) => {
            let c = c.clamp(lo, hi);
            if c - lo <= hi - c {
                let d = c - lo;
                (1..=n).map(|k| lo + d * f64::from(k)).collect()
            } else {
                let d = hi - c;
                (1..=n).map(|k| hi - d * f64::from(k)).collect()
            }
        }
    };
    out.retain(|s| *s > lo + MIN_PIECE && *s < hi - MIN_PIECE);
    out.sort_by(f64::total_cmp);
    for s in &mut out {
        *s = crate::common::clean(*s);
    }
    out
}

#[derive(Clone, Debug)]
pub struct SliceLayout {
    pub axis: usize,
    pub normal: DVec3,
    pub lo: f64,
    pub hi: f64,
    pub positions: Vec<f64>,
    pub bounds: Aabb,
    pub ids: Vec<Id>,
}

impl SliceLayout {
    pub fn new(
        doc: &Document,
        frame: &GridFrame,
        axis: usize,
        ids: Vec<Id>,
        cursor: Option<f64>,
        segments: u32,
    ) -> Option<SliceLayout> {
        let axis = axis.min(2);
        let normal = column(frame, axis);
        let (lo, hi, bounds) = extent(doc, frame, &ids, normal)?;
        let positions = positions(lo, hi, cursor, segments);
        Some(SliceLayout { axis, normal, lo, hi, positions, bounds, ids })
    }

    pub fn planes(&self) -> Vec<Plane> {
        self.positions.iter().filter_map(|s| Plane::new(self.normal, *s)).collect()
    }

    pub fn blocks(&self) -> usize {
        self.positions.len() + 1
    }
}

pub struct SliceTool {
    pub orientation: Orientation,
    pub segments: u32,
    cursor: Option<f64>,
    axis: usize,
    kind: ViewKind,
    right: DVec3,
    last_pointer: Option<Pos2>,
    wheel: f32,
    last_focus: Option<egui::Id>,
}

impl Default for SliceTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SliceTool {
    pub fn new() -> Self {
        Self {
            orientation: Orientation::LeftRight,
            segments: 1,
            cursor: None,
            axis: 0,
            kind: ViewKind::Perspective,
            right: DVec3::X,
            last_pointer: None,
            wheel: 0.0,
            last_focus: None,
        }
    }

    pub fn cursor(&self) -> Option<f64> {
        self.cursor
    }

    pub fn axis(&self) -> usize {
        self.axis
    }

    pub fn set_cursor(&mut self, s: Option<f64>) {
        self.cursor = s.filter(|v| v.is_finite());
    }

    fn frame(&self, state: &AppState) -> GridFrame {
        state.edit_grid_frame(self.kind)
    }

    fn resolve_axis(&mut self, state: &AppState) {
        let frame = self.frame(state);
        let (right, up) = view_basis(self.kind, self.right);
        let axis = orientation_axis(&frame, self.orientation, right, up);
        if axis != self.axis {
            self.axis = axis;
            self.cursor = None;
        }
    }

    fn set_orientation(&mut self, state: &AppState, orientation: Orientation) {
        if orientation != self.orientation {
            self.orientation = orientation;
            self.cursor = None;
        }
        self.resolve_axis(state);
    }

    pub fn toggle_orientation(&mut self, state: &AppState) {
        self.set_orientation(state, self.orientation.toggled());
    }

    pub fn set_segments(&mut self, n: u32) {
        self.segments = n.clamp(1, MAX_SEGMENTS);
    }

    pub fn layout(&self, state: &AppState) -> Option<SliceLayout> {
        let ids = state.doc.selected_solid_ids();
        if ids.is_empty() {
            return None;
        }
        let frame = self.frame(state);
        SliceLayout::new(&state.doc, &frame, self.axis, ids, self.cursor, self.segments)
    }

    fn adopt_view(&mut self, state: &AppState, view: &ViewInfo) {
        self.kind = view.kind;
        self.right = view_basis(view.kind, view.cam3d.right()).0;
        self.resolve_axis(state);
    }

    fn cursor_from_mouse(
        &self,
        state: &AppState,
        view: &ViewInfo,
        pos: Pos2,
        alt: bool,
        base: &SliceLayout,
    ) -> Option<f64> {
        let frame = self.frame(state);
        let n = base.normal;
        let raw = if let Some((_, _, d)) = view.kind.axes_2d() {
            let world = view.kind.from_2d(view.screen_to_2d(pos), base.bounds.center()[d]);
            world.dot(n)
        } else {
            let (origin, dir) = view.ray(pos);
            let direct = base
                .bounds
                .ray_intersect(&hp_geom::Ray::new(origin, dir))
                .filter(|t| t.is_finite() && *t >= 0.0)
                .map(|t| (origin + dir * t).dot(n));
            if let Some(s) = direct {
                return self.finish_cursor(state, &frame, base, s, alt);
            }
            let forward = view.cam3d.forward();
            let mut best: Option<(usize, f64)> = None;
            for k in 0..3 {
                if k == base.axis {
                    continue;
                }
                let d = column(&frame, k).dot(forward).abs();
                if best.is_none_or(|(_, b)| d > b) {
                    best = Some((k, d));
                }
            }
            let (k, _) = best?;
            let m = column(&frame, k);
            let corners = base.bounds.corners();
            let (mut mlo, mut mhi) = (f64::INFINITY, f64::NEG_INFINITY);
            for c in corners {
                mlo = mlo.min(c.dot(m));
                mhi = mhi.max(c.dot(m));
            }
            let eye = view.cam3d.pos.dot(m);
            let plane_at = if eye >= mhi {
                mhi
            } else if eye <= mlo {
                mlo
            } else {
                (mlo + mhi) * 0.5
            };
            let denom = dir.dot(m);
            let t = (plane_at - origin.dot(m)) / denom;
            if denom.abs() < 1e-9 || !t.is_finite() || t <= 0.0 {
                return None;
            }
            (origin + dir * t).dot(n)
        };
        self.finish_cursor(state, &frame, base, raw, alt)
    }

    fn finish_cursor(
        &self,
        state: &AppState,
        frame: &GridFrame,
        base: &SliceLayout,
        raw: f64,
        alt: bool,
    ) -> Option<f64> {
        if !raw.is_finite() {
            return None;
        }
        let s = if state.snap_enabled() && !alt { snap_position(frame, base.axis, raw) } else { raw };
        Some(s.clamp(base.lo, base.hi))
    }

    pub fn apply(&mut self, state: &mut AppState) -> bool {
        let Some(layout) = self.layout(state) else {
            state.notify(Severity::Warning, t!("slice.no_selection"));
            return false;
        };
        let planes = layout.planes();
        if planes.is_empty() {
            state.notify(Severity::Info, t!("slice.nothing"));
            return false;
        }
        let material = state.settings.brush.clip_current_material.then(|| active_material(state));
        match state.doc.slice_solids(&layout.ids, &planes, material.as_deref()) {
            Ok(pieces) if pieces.is_empty() => {
                state.notify(Severity::Info, t!("slice.nothing"));
                false
            }
            Ok(pieces) => {
                state.log(Severity::Info, tf!("slice.done", layout.ids.len(), pieces.len()));
                true
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("slice.failed", e));
                false
            }
        }
    }

    fn keys_text(state: &AppState) -> String {
        let tab = key_label(Key::Tab);
        let o = key_label(Key::O);
        match state.keys.shortcut(Action::ToolSlice) {
            Some(sc) => format!("{tab} / {o} / {}", shortcut_label(&sc)),
            None => format!("{tab} / {o}"),
        }
    }

    fn position_text(&self, layout: &SliceLayout) -> String {
        let anchor = self.cursor.map_or_else(|| (layout.lo + layout.hi) * 0.5, |c| c.clamp(layout.lo, layout.hi));
        let text = fmt_num(round_to(anchor, 0.01));
        if self.cursor.is_none() { tf!("slice.center", text) } else { text }
    }

    fn tab_input(&mut self, state: &AppState, ctx: &egui::Context) -> bool {
        let focused = ctx.memory(|m| m.focused());
        let had_focus = std::mem::replace(&mut self.last_focus, focused);
        if had_focus.is_some() {
            return false;
        }
        let presses = consume_tabs(ctx);
        if presses.is_empty() {
            return false;
        }
        if let Some(id) = focused {
            ctx.memory_mut(|m| m.surrender_focus(id));
            self.last_focus = None;
            ctx.request_repaint();
        }
        for _ in presses {
            self.toggle_orientation(state);
        }
        true
    }
}

impl Tool for SliceTool {
    fn id(&self) -> ToolId {
        ToolId::Slice
    }

    fn activate(&mut self, state: &mut AppState) {
        self.last_focus = None;
        self.last_pointer = None;
        self.cursor = None;
        self.kind = ViewKind::Perspective;
        self.right = view_basis(ViewKind::Perspective, state.cameras.cam3d.right()).0;
        self.axis = usize::MAX;
        self.resolve_axis(state);
    }

    fn reactivate(&mut self, state: &mut AppState) {
        self.toggle_orientation(state);
    }

    fn deactivate(&mut self, _state: &mut AppState) {
        self.last_pointer = None;
        self.last_focus = None;
        self.cursor = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        self.adopt_view(state, input.view);
        if input.response.secondary_clicked() {
            self.cursor = None;
            self.last_pointer = Some(pos);
            return ToolFlow::Handled;
        }
        if self.last_pointer != Some(pos) {
            self.last_pointer = Some(pos);
            if let Some(base) = self.layout(state)
                && let Some(s) = self.cursor_from_mouse(state, input.view, pos, m.modifiers.alt, &base)
            {
                self.cursor = Some(s);
            }
        }
        if m.clicked {
            self.apply(state);
            return ToolFlow::Handled;
        }
        ToolFlow::Ignored
    }

    fn wheel(&mut self, _state: &mut AppState, view: &ViewInfo, notches: f32, mods: egui::Modifiers) -> ToolFlow {
        if !notches.is_finite() || mods.ctrl || mods.command || mods.alt {
            return ToolFlow::Ignored;
        }
        if view.is_2d() && !mods.shift {
            return ToolFlow::Ignored;
        }
        self.wheel += notches;
        while self.wheel >= 1.0 {
            self.wheel -= 1.0;
            self.set_segments(self.segments.saturating_add(1));
        }
        while self.wheel <= -1.0 {
            self.wheel += 1.0;
            self.set_segments(self.segments.saturating_sub(1));
        }
        ToolFlow::Handled
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.tab_input(state, ctx) {
            return true;
        }
        if consume_letter(ctx, Key::O) {
            self.toggle_orientation(state);
            return true;
        }
        if consume_plain_key(ctx, Key::Enter) {
            self.apply(state);
            return true;
        }
        false
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        self.cursor.take().is_some()
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let Some(layout) = self.layout(state) else { return };
        let mut ov = Ov::new(out, view);
        let region = layout.bounds.expand(4.0);
        let line = rgba(COLOR_PENDING, 0.95);
        let planes = layout.planes();
        for plane in &planes {
            let poly = plane_box_polygon(plane, &region);
            ov.polygon_fill(&poly, rgba(COLOR_PENDING, 0.12));
            ov.polygon_outline(&poly, line, true);
        }
        ov.box_edges(&layout.bounds, rgba(COLOR_PENDING, 0.3), false);
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(layout) = self.layout(state) else { return };
        if layout.positions.len() > MAX_LABELS {
            return;
        }
        let center = layout.bounds.center();
        for s in &layout.positions {
            let at = center + layout.normal * (s - center.dot(layout.normal));
            if let Some(p) = view.world_to_screen(at)
                && view.rect.contains(p)
            {
                paint_label(painter, p, Align2::CENTER_CENTER, &fmt_num(round_to(*s, 0.01)), COLOR_PENDING);
            }
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let hint = self.status_hint(state).unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            ui.label(hint);
        });
    }

    fn cursor(&self, _state: &AppState, _view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        let keys = Self::keys_text(state);
        let Some(layout) = self.layout(state) else {
            return Some(tf!("slice.hint_none", keys));
        };
        let key = if self.kind.is_2d() { "slice.hint_wheel_2d" } else { "slice.hint" };
        let cuts = layout.positions.len();
        Some(tf!(
            key,
            t!(self.orientation.label_key()),
            self.segments,
            self.position_text(&layout),
            cuts,
            layout.blocks(),
            keys,
            key_label(Key::Escape)
        ))
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        match key {
            "orientation" => Orientation::parse(value).map(|o| self.set_orientation(state, o)).is_some(),
            "segments" => value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && v.fract() == 0.0 && *v >= 1.0 && *v <= f64::from(MAX_SEGMENTS))
                .map(|v| self.set_segments(v as u32))
                .is_some(),
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<ToolOption> {
        vec![
            ToolOption::new(
                "orientation",
                self.orientation.id(),
                "left_right: cut planes that split the object into left and right blocks (across the horizontal grid \
                 axis nearest the screen right); top_bottom: planes across the vertical grid axis (Tab or O \
                 toggles)",
            ),
            ToolOption::new(
                "segments",
                self.segments.to_string(),
                "number of cuts, 1 to 64 (wheel); N cuts give N+1 blocks",
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_cuts_are_even() {
        let p = positions(0.0, 120.0, None, 2);
        assert_eq!(p, vec![40.0, 80.0]);
        assert_eq!(positions(0.0, 100.0, None, 1), vec![50.0]);
    }

    #[test]
    fn cursor_sets_the_pitch_from_the_nearest_side() {
        assert_eq!(positions(0.0, 100.0, Some(16.0), 3), vec![16.0, 32.0, 48.0]);
        assert_eq!(positions(0.0, 100.0, Some(84.0), 2), vec![68.0, 84.0]);
        assert_eq!(positions(0.0, 100.0, Some(30.0), 5), vec![30.0, 60.0, 90.0]);
    }

    #[test]
    fn cuts_never_leave_the_object() {
        assert!(positions(0.0, 100.0, Some(0.0), 3).is_empty());
        assert!(positions(0.0, 100.0, Some(100.0), 3).is_empty());
        assert!(positions(0.0, 100.0, Some(-50.0), 3).is_empty());
        for s in positions(0.0, 100.0, Some(40.0), 64) {
            assert!(s > 0.0 && s < 100.0);
        }
        assert_eq!(positions(0.0, 100.0, Some(900.0), 1), Vec::<f64>::new());
    }

    #[test]
    fn segments_are_clamped() {
        assert_eq!(positions(0.0, 100.0, None, 0).len(), 1);
        assert_eq!(positions(0.0, 10_000.0, None, 500).len(), MAX_SEGMENTS as usize);
    }

    #[test]
    fn orientation_follows_the_view() {
        let plain = GridFrame::uniform(16.0);
        assert_eq!(orientation_axis(&plain, Orientation::TopBottom, DVec3::X, DVec3::Z), 2);
        assert_eq!(orientation_axis(&plain, Orientation::LeftRight, DVec3::X, DVec3::Z), 0);
        assert_eq!(orientation_axis(&plain, Orientation::LeftRight, DVec3::Y, DVec3::Z), 1);
        assert_eq!(orientation_axis(&plain, Orientation::TopBottom, DVec3::X, DVec3::Y), 1);
        assert_eq!(orientation_axis(&plain, Orientation::LeftRight, DVec3::NEG_X, DVec3::Y), 0);
    }

    #[test]
    fn rotated_grid_uses_its_own_axes() {
        let mut frame = GridFrame::uniform(16.0);
        frame.axes = hp_geom::angle_matrix(DVec3::new(0.0, 90.0, 0.0));
        let axis = orientation_axis(&frame, Orientation::TopBottom, DVec3::X, DVec3::Z);
        assert!(column(&frame, axis).dot(DVec3::Z).abs() > 0.99);
    }

    #[test]
    fn snap_follows_the_grid_origin() {
        let mut frame = GridFrame::uniform(16.0);
        assert_eq!(snap_position(&frame, 0, 21.0), 16.0);
        frame.origin = DVec3::new(4.0, 0.0, 0.0);
        assert_eq!(snap_position(&frame, 0, 21.0), 20.0);
    }

    #[test]
    fn orientation_parses() {
        assert_eq!(Orientation::parse("Left-Right"), Some(Orientation::LeftRight));
        assert_eq!(Orientation::parse("top bottom"), Some(Orientation::TopBottom));
        assert_eq!(Orientation::parse("diagonal"), None);
        assert_eq!(Orientation::LeftRight.toggled().toggled(), Orientation::LeftRight);
    }

    fn selected_state() -> AppState {
        let mut state = crate::tests_support::settle();
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(2), hp_editor::ObjRef::Solid(3)]);
        state
    }

    #[test]
    fn apply_makes_valid_blocks_in_one_undo_step() {
        let mut state = selected_state();
        let before = state.doc.stats().solids;
        let mut tool = SliceTool::new();
        tool.activate(&mut state);
        tool.set_segments(3);
        let layout = tool.layout(&state).expect("layout");
        assert_eq!(layout.positions.len(), 3);
        assert!(layout.positions.iter().all(|s| *s > layout.lo && *s < layout.hi));
        assert!(tool.apply(&mut state));
        let after = state.doc.stats().solids;
        assert!(after > before, "{before} {after}");
        for id in state.doc.selected_solid_ids() {
            assert!(state.doc.scene().solid(id).expect("solid").geom.valid);
        }
        assert!(state.doc.undo());
        assert_eq!(state.doc.stats().solids, before, "one undo step restores everything");
    }

    #[test]
    fn nothing_to_cut_without_selection_or_room() {
        let mut state = crate::tests_support::settle();
        state.doc.set_selection(&[]);
        let mut tool = SliceTool::new();
        tool.activate(&mut state);
        assert!(tool.layout(&state).is_none());
        assert!(!tool.apply(&mut state));
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        let mut state = selected_state();
        let before = state.doc.stats().solids;
        tool.set_cursor(Some(-1.0e6));
        assert!(!tool.apply(&mut state));
        assert_eq!(state.doc.stats().solids, before);
    }

    #[test]
    fn wheel_changes_segments_and_never_goes_below_one() {
        use crate::tests_support::{info_2d, info_3d};
        let mut state = selected_state();
        let mut tool = SliceTool::new();
        let v3 = info_3d(DVec3::new(-300.0, -300.0, 200.0), 45.0, -20.0);
        let v2 = info_2d(ViewKind::Top, 0.5);
        assert_eq!(tool.wheel(&mut state, &v3, 1.0, egui::Modifiers::NONE), ToolFlow::Handled);
        assert_eq!(tool.segments, 2);
        tool.wheel(&mut state, &v3, -10.0, egui::Modifiers::NONE);
        assert_eq!(tool.segments, 1);
        tool.wheel(&mut state, &v3, 200.0, egui::Modifiers::NONE);
        assert_eq!(tool.segments, MAX_SEGMENTS);
        assert_eq!(tool.wheel(&mut state, &v2, 1.0, egui::Modifiers::NONE), ToolFlow::Ignored);
        assert_eq!(tool.wheel(&mut state, &v2, -200.0, egui::Modifiers::SHIFT), ToolFlow::Handled);
        assert_eq!(tool.segments, 1);
        assert_eq!(tool.wheel(&mut state, &v3, 1.0, egui::Modifiers::CTRL), ToolFlow::Ignored);
    }

    #[test]
    fn reset_and_orientation_options() {
        let mut state = selected_state();
        let mut tool = SliceTool::new();
        tool.activate(&mut state);
        tool.set_cursor(Some(5.0));
        assert!(tool.cancel(&mut state));
        assert!(!tool.cancel(&mut state));
        assert!(tool.set_option(&mut state, "orientation", "top_bottom"));
        assert_eq!(tool.axis(), 2);
        assert!(tool.set_option(&mut state, "segments", "7"));
        assert_eq!(tool.segments, 7);
        assert!(!tool.set_option(&mut state, "segments", "0"));
        assert!(!tool.set_option(&mut state, "segments", "65"));
        assert!(!tool.set_option(&mut state, "orientation", "diagonal"));
        tool.reactivate(&mut state);
        assert_eq!(tool.orientation, Orientation::LeftRight);
        assert_eq!(tool.option_values().len(), 2);
    }

    #[test]
    fn mouse_position_is_grid_snapped_and_clamped() {
        use crate::tests_support::info_2d;
        let mut state = selected_state();
        let mut tool = SliceTool::new();
        tool.activate(&mut state);
        let view = info_2d(ViewKind::Top, 0.5);
        tool.adopt_view(&state, &view);
        let base = tool.layout(&state).expect("layout");
        let mid = view.rect.center();
        for dx in [-2000.0, -37.0, 0.0, 41.0, 2000.0] {
            let p = Pos2::new(mid.x + dx, mid.y);
            let s = tool.cursor_from_mouse(&state, &view, p, false, &base).expect("cursor");
            assert!(s >= base.lo && s <= base.hi);
            let on_grid = (s / state.settings.edit.grid_size).fract().abs();
            let edge = (s - base.lo).abs() < 1e-9 || (s - base.hi).abs() < 1e-9;
            assert!(edge || on_grid < 1e-9 || (1.0 - on_grid) < 1e-9, "{s}");
        }
    }

    #[test]
    fn translations() {
        let problems = hp_app::i18n::lint_table(SLICE_TR);
        assert!(problems.is_empty(), "{problems:#?}");
        for (k, en, fr) in SLICE_TR {
            for i in 0..7 {
                let p = format!("{{{i}}}");
                assert_eq!(en.contains(&p), fr.contains(&p), "{k}: {p}");
            }
        }
    }
}
