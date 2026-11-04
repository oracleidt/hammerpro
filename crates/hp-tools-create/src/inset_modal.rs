use std::cell::RefCell;

use egui::{Align2, CursorIcon, Event, Key, PointerButton, Pos2, Vec2};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::tool::ToolOption;
use hp_app::{AppState, Severity, Tool, ToolFlow, ToolId, ToolMenuEntry, ViewInfo, ViewInput, t, tf};
use hp_editor::{EditError, FaceRef};
use hp_geom::{InsetIssue, InsetParams, MAX_INSET, MIN_INSET, Plane, inset_ring};
use hp_render::Overlay;

use crate::common::{COLOR_FRONT, COLOR_GHOST, COLOR_PENDING, Ov, fmt_len, paint_label, rgba};

pub const DEPTH_KEY: Key = Key::E;

const MAX_TYPED: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsetTarget {
    Thickness,
    Depth,
}

pub fn thickness_from_distance(base: f64, now: f64, px_per_unit: f64) -> f64 {
    if !(px_per_unit.is_finite() && px_per_unit > 1e-9) {
        return 0.0;
    }
    ((base - now) / px_per_unit).clamp(0.0, MAX_INSET)
}

pub fn depth_from_drag(base: f64, delta: Vec2, normal_px: Vec2, px_per_unit: f64) -> f64 {
    if !(px_per_unit.is_finite() && px_per_unit > 1e-9) {
        return base;
    }
    let len2 = f64::from(normal_px.length_sq());
    let d = if len2.sqrt() < 0.2 * px_per_unit {
        -f64::from(delta.y) / px_per_unit
    } else {
        f64::from(delta.x * normal_px.x + delta.y * normal_px.y) / len2
    };
    (base + d).clamp(-MAX_INSET, MAX_INSET)
}

pub fn snap_value(value: f64, grid: f64, minimum: f64) -> f64 {
    if !(grid.is_finite() && grid > 0.0) {
        return value;
    }
    let s = (value / grid).round() * grid;
    if value.abs() < 1e-12 { 0.0 } else { s.abs().max(minimum).copysign(s) }
}

pub fn typed_push(buffer: &mut String, ch: char) -> bool {
    let ch = if ch == ',' { '.' } else { ch };
    let ok = match ch {
        '0'..='9' => true,
        '.' => !buffer.contains('.'),
        '-' => buffer.is_empty(),
        _ => false,
    };
    if ok && buffer.len() < MAX_TYPED {
        buffer.push(ch);
        true
    } else {
        false
    }
}

pub fn typed_value(buffer: &str) -> Option<f64> {
    buffer.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

pub fn issue_key(issue: InsetIssue) -> &'static str {
    match issue {
        InsetIssue::NotAFace => "inset.err_face",
        InsetIssue::Displacement => "inset.err_disp",
        InsetIssue::BadDistance => "inset.err_distance",
        InsetIssue::TooLarge => "inset.err_large",
        InsetIssue::TooDeep => "inset.err_deep",
        InsetIssue::PastLimits => "inset.err_limits",
    }
}

pub fn error_text(e: &EditError) -> String {
    match e {
        EditError::Geom(g) => match InsetIssue::of(g) {
            Some(issue) => t!(issue_key(issue)).to_owned(),
            None => g.to_string(),
        },
        other => other.to_string(),
    }
}

fn pixels_per_unit(view: &ViewInfo, center: DVec3) -> Option<f64> {
    if view.is_2d() {
        return Some(view.cam2d.zoom).filter(|z| *z > 0.0);
    }
    let c = view.world_to_screen(center)?;
    let r = view.world_to_screen(center + view.cam3d.right() * 16.0)?;
    let u = view.world_to_screen(center + view.cam3d.up() * 16.0)?;
    Some(f64::from((r - c).length() + (u - c).length()) / 32.0).filter(|v| *v > 1e-9)
}

fn normal_pixels(view: &ViewInfo, center: DVec3, normal: DVec3) -> Vec2 {
    match (view.world_to_screen(center), view.world_to_screen(center + normal * 16.0)) {
        (Some(a), Some(b)) => (b - a) / 16.0,
        _ => Vec2::ZERO,
    }
}

#[derive(Clone, Debug)]
struct FaceData {
    face: FaceRef,
    vertices: Vec<DVec3>,
    plane: Plane,
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    view: usize,
    base: f64,
}

#[derive(Clone, Debug, Default)]
struct Preview {
    key: Option<(u64, u64, u64)>,
    outcome: Option<Result<usize, String>>,
}

pub struct InsetModal {
    faces: Vec<FaceData>,
    center: DVec3,
    normal: DVec3,
    revision: u64,
    anchor: Option<Anchor>,
    pointer: Option<(usize, Pos2)>,
    thickness: f64,
    depth: f64,
    target: InsetTarget,
    typed: String,
    depth_anchor: Option<Pos2>,
    depth_base: f64,
    rebase: bool,
    preview: RefCell<Preview>,
    finished: bool,
}

impl InsetModal {
    pub fn start(state: &AppState, faces: &[FaceRef]) -> Option<Self> {
        let mut data: Vec<FaceData> = Vec::with_capacity(faces.len());
        for f in faces {
            if data.iter().any(|d| d.face == *f) {
                continue;
            }
            let n = state.doc.scene().solid(f.solid)?;
            let g = n.geom.face_for_side(f.side)?;
            data.push(FaceData { face: *f, vertices: g.vertices.clone(), plane: g.plane });
        }
        if data.is_empty() {
            return None;
        }
        let all: Vec<DVec3> = data.iter().flat_map(|d| d.vertices.iter().copied()).collect();
        let center = all.iter().fold(DVec3::ZERO, |s, p| s + *p) / all.len() as f64;
        let normal =
            data.iter().fold(DVec3::ZERO, |s, d| s + d.plane.normal).try_normalize().unwrap_or(data[0].plane.normal);
        Some(Self {
            faces: data,
            center,
            normal,
            revision: state.doc.revision(),
            anchor: None,
            pointer: None,
            thickness: 0.0,
            depth: 0.0,
            target: InsetTarget::Thickness,
            typed: String::new(),
            depth_anchor: None,
            depth_base: 0.0,
            rebase: false,
            preview: RefCell::new(Preview::default()),
            finished: false,
        })
    }

    pub fn is_active(&self) -> bool {
        !self.finished
    }

    pub fn finish(&mut self) {
        self.finished = true;
    }

    pub fn thickness(&self) -> f64 {
        self.thickness
    }

    pub fn depth(&self) -> f64 {
        self.depth
    }

    pub fn target(&self) -> InsetTarget {
        self.target
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn params(&self) -> InsetParams {
        InsetParams { distance: self.thickness, depth: self.depth, material: None }
    }

    pub fn set_values(&mut self, thickness: f64, depth: f64) {
        self.thickness = thickness.clamp(0.0, MAX_INSET);
        self.depth = depth.clamp(-MAX_INSET, MAX_INSET);
    }

    pub fn toggle_target(&mut self) {
        self.typed.clear();
        self.target = match self.target {
            InsetTarget::Thickness => {
                self.depth_anchor = self.pointer.map(|p| p.1);
                self.depth_base = self.depth;
                InsetTarget::Depth
            }
            InsetTarget::Depth => {
                self.rebase = true;
                InsetTarget::Thickness
            }
        };
    }

    pub fn type_char(&mut self, ch: char) -> bool {
        if !typed_push(&mut self.typed, ch) {
            return false;
        }
        self.apply_typed();
        true
    }

    pub fn backspace(&mut self) -> bool {
        if self.typed.pop().is_none() {
            return false;
        }
        if self.typed.is_empty() {
            self.reanchor_target();
        } else {
            self.apply_typed();
        }
        true
    }

    fn reanchor_target(&mut self) {
        match self.target {
            InsetTarget::Thickness => self.rebase = true,
            InsetTarget::Depth => {
                self.depth_anchor = self.pointer.map(|p| p.1);
                self.depth_base = self.depth;
            }
        }
    }

    fn apply_typed(&mut self) {
        let v = typed_value(&self.typed).unwrap_or(0.0);
        match self.target {
            InsetTarget::Thickness => self.thickness = v.clamp(0.0, MAX_INSET),
            InsetTarget::Depth => self.depth = v.clamp(-MAX_INSET, MAX_INSET),
        }
    }

    pub fn on_pointer(&mut self, view: &ViewInfo, pos: Pos2, snap: bool) {
        self.pointer = Some((view.index, pos));
        let Some(ppu) = pixels_per_unit(view, self.center) else { return };
        let Some(c) = view.world_to_screen(self.center) else { return };
        let now = f64::from((pos - c).length());
        let anchor = match self.anchor {
            Some(a) if a.view == view.index => a,
            Some(_) => return,
            None => {
                let a = Anchor { view: view.index, base: now };
                self.anchor = Some(a);
                self.depth_anchor = self.depth_anchor.or(Some(pos));
                a
            }
        };
        if self.rebase {
            self.rebase = false;
            self.anchor = Some(Anchor { view: view.index, base: now + self.thickness * ppu });
            return;
        }
        if !self.typed.is_empty() {
            return;
        }
        match self.target {
            InsetTarget::Thickness => {
                let v = thickness_from_distance(anchor.base, now, ppu);
                self.thickness = if snap && v > 0.0 { snap_value(v, view.grid, view.grid.max(MIN_INSET)) } else { v };
            }
            InsetTarget::Depth => {
                let from = self.depth_anchor.unwrap_or(pos);
                let n = normal_pixels(view, self.center, self.normal);
                let v = depth_from_drag(self.depth_base, pos - from, n, ppu);
                self.depth = if snap { snap_value(v, view.grid, 0.0) } else { v };
            }
        }
    }

    fn stale(&self, state: &AppState) -> bool {
        state.doc.revision() != self.revision
    }

    pub fn confirm(&mut self, state: &mut AppState) -> bool {
        if self.thickness < MIN_INSET {
            state.notify(Severity::Info, t!("inset_modal.too_small"));
            return false;
        }
        let faces: Vec<FaceRef> = self.faces.iter().map(|f| f.face).collect();
        match state.doc.inset_faces(&faces, &self.params()) {
            Ok(ids) => {
                state.log(Severity::Info, tf!("inset.done", ids.len()));
                self.finished = true;
                true
            }
            Err(e) => {
                state.notify(Severity::Warning, tf!("app.op_failed", t!("action.inset_face"), error_text(&e)));
                false
            }
        }
    }

    fn refresh_preview(&self, state: &AppState) {
        let key = (state.doc.revision(), self.thickness.to_bits(), self.depth.to_bits());
        let Ok(mut pv) = self.preview.try_borrow_mut() else { return };
        if pv.key == Some(key) {
            return;
        }
        pv.key = Some(key);
        pv.outcome = (self.thickness >= MIN_INSET).then(|| {
            let faces: Vec<FaceRef> = self.faces.iter().map(|f| f.face).collect();
            match state.doc.inset_preview(&faces, &self.params()) {
                Ok(plan) => Ok(plan.iter().map(|(_, pieces)| pieces.len()).sum()),
                Err(e) => Err(error_text(&e)),
            }
        });
    }

    fn error(&self, state: &AppState) -> Option<String> {
        self.refresh_preview(state);
        self.preview.try_borrow().ok().and_then(|p| p.outcome.clone()).and_then(Result::err)
    }

    fn brushes(&self, state: &AppState) -> Option<usize> {
        self.refresh_preview(state);
        self.preview.try_borrow().ok().and_then(|p| p.outcome.clone()).and_then(Result::ok)
    }

    fn value_text(&self, target: InsetTarget, value: f64) -> String {
        if self.target == target && !self.typed.is_empty() { format!("{}_", self.typed) } else { fmt_len(value) }
    }

    pub fn label(&self, state: &AppState) -> String {
        let a = self.value_text(InsetTarget::Thickness, self.thickness);
        let b = self.value_text(InsetTarget::Depth, self.depth);
        let (a, b) = match self.target {
            InsetTarget::Thickness => {
                (format!("[{}: {a}]", t!("inset_modal.thickness")), format!("{}: {b}", t!("inset_modal.depth")))
            }
            InsetTarget::Depth => {
                (format!("{}: {a}", t!("inset_modal.thickness")), format!("[{}: {b}]", t!("inset_modal.depth")))
            }
        };
        let mut s = format!("{a}   {b}");
        if let Some(e) = self.error(state) {
            s.push_str(&format!("   ({e})"));
        } else if let Some(n) = self.brushes(state) {
            s.push_str(&format!("   {}", tf!("inset_modal.brushes", n)));
        }
        s
    }

    pub fn hint(&self, state: &AppState) -> String {
        if self.anchor.is_none() {
            return t!("inset_modal.hint_wait").to_owned();
        }
        let e = key_label(DEPTH_KEY);
        let enter = key_label(Key::Enter);
        let esc = key_label(Key::Escape);
        format!("{}   {}", self.label(state), tf!("inset_modal.hint", e, enter, esc))
    }

    pub fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if self.thickness < MIN_INSET {
            return;
        }
        let failed = self.error(state).is_some();
        let mut ov = Ov::new(out, view);
        let cut = rgba(COLOR_FRONT, 1.0);
        let inner = rgba(if failed { hp_app::theme::VIEW_ERROR } else { COLOR_PENDING }, 1.0);
        let fill = rgba(COLOR_PENDING, 0.28);
        let ghost = rgba(COLOR_GHOST, 0.8);
        for f in &self.faces {
            let Some(ring) = inset_ring(&f.vertices, &f.plane, self.thickness) else {
                ov.polygon_outline(&f.vertices, rgba(hp_app::theme::VIEW_ERROR, 1.0), true);
                continue;
            };
            let lift = f.plane.normal * self.depth;
            let top: Vec<DVec3> = ring.iter().map(|r| r[1] + lift).collect();
            for r in &ring {
                ov.xray_line(r[0], r[1], cut);
                if self.depth != 0.0 {
                    ov.xray_line(r[1], r[1] + lift, ghost);
                }
            }
            ov.polygon_outline(&top, inner, true);
            ov.polygon_fill(&top, fill);
        }
    }

    pub fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some((index, pos)) = self.pointer else { return };
        if index != view.index {
            return;
        }
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let color = if self.error(state).is_some() { hp_app::theme::VIEW_ERROR } else { COLOR_PENDING };
        let (right, low) = (pos.x > view.rect.center().x, pos.y < view.rect.top() + 32.0);
        let anchor = match (right, low) {
            (true, false) => Align2::RIGHT_BOTTOM,
            (false, false) => Align2::LEFT_BOTTOM,
            (true, true) => Align2::RIGHT_TOP,
            (false, true) => Align2::LEFT_TOP,
        };
        let (dx, dy) = (if right { -16.0 } else { 16.0 }, if low { 22.0 } else { -14.0 });
        paint_label(&painter, pos + Vec2::new(dx, dy), anchor, &self.label(state), color);
    }

    pub fn handle_pointer(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if self.finished {
            return ToolFlow::Ignored;
        }
        if self.stale(state) {
            self.finished = true;
            return ToolFlow::Ignored;
        }
        if input.navigating {
            return ToolFlow::Handled;
        }
        let r = input.response;
        if let Some(pos) = input.pointer {
            self.on_pointer(input.view, pos, input.modifiers.ctrl || input.modifiers.command);
        }
        let cancel = r.secondary_clicked()
            || (r.contains_pointer() && r.ctx.input(|i| i.pointer.button_pressed(PointerButton::Secondary)));
        if cancel {
            self.finished = true;
        } else if r.clicked_by(PointerButton::Primary) || r.drag_stopped_by(PointerButton::Primary) {
            self.confirm(state);
        }
        ToolFlow::Handled
    }

    pub fn handle_keys(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.finished {
            return false;
        }
        if self.stale(state) {
            self.finished = true;
            return false;
        }
        let mut keys: Vec<Key> = Vec::new();
        let mut text = String::new();
        ctx.input_mut(|i| {
            i.events.retain(|e| match e {
                Event::Key { key, pressed: true, modifiers, .. }
                    if !modifiers.ctrl
                        && !modifiers.command
                        && !modifiers.alt
                        && !hp_app::actions::is_modifier_key(*key) =>
                {
                    keys.push(*key);
                    false
                }
                Event::Text(s) => {
                    text.push_str(s);
                    false
                }
                _ => true,
            });
        });
        for ch in text.chars() {
            self.type_char(ch);
        }
        for key in keys {
            match key {
                Key::Escape => self.finished = true,
                Key::Enter => {
                    self.confirm(state);
                }
                Key::Backspace => {
                    self.backspace();
                }
                k if k == DEPTH_KEY => self.toggle_target(),
                _ => {}
            }
            if self.finished {
                break;
            }
        }
        true
    }
}

pub struct WithInset<'a> {
    pub modal: &'a mut InsetModal,
    pub inner: &'a mut dyn Tool,
}

impl Tool for WithInset<'_> {
    fn id(&self) -> ToolId {
        self.inner.id()
    }

    fn activate(&mut self, state: &mut AppState) {
        self.inner.activate(state);
    }

    fn context_entries(&self, state: &AppState) -> Vec<ToolMenuEntry> {
        self.inner.context_entries(state)
    }

    fn context_command(&mut self, state: &mut AppState, id: u32) {
        self.inner.context_command(state, id);
    }

    fn wheel(&mut self, state: &mut AppState, view: &ViewInfo, notches: f32, modifiers: egui::Modifiers) -> ToolFlow {
        if self.modal.is_active() {
            return ToolFlow::Ignored;
        }
        self.inner.wheel(state, view, notches, modifiers)
    }

    fn reactivate(&mut self, state: &mut AppState) {
        self.inner.reactivate(state);
    }

    fn action(&mut self, state: &mut AppState, action: hp_app::Action) -> bool {
        self.inner.action(state, action)
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.inner.deactivate(state);
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if self.modal.is_active() {
            return self.modal.handle_pointer(state, input);
        }
        self.inner.view_input(state, input)
    }

    fn click_while_flying(
        &mut self,
        state: &mut AppState,
        view: &ViewInfo,
        pos: Pos2,
        modifiers: egui::Modifiers,
    ) -> ToolFlow {
        if self.modal.is_active() {
            return ToolFlow::Handled;
        }
        self.inner.click_while_flying(state, view, pos, modifiers)
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.modal.is_active() {
            return self.modal.handle_keys(state, ctx);
        }
        self.inner.key_input(state, ctx)
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        self.inner.overlay(state, view, out);
        if self.modal.is_active() {
            self.modal.overlay(state, view, out);
        }
    }

    fn retained_overlay_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        self.inner.retained_overlay_version(state, view)
    }

    fn retained_overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        self.inner.retained_overlay(state, view, out);
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        self.inner.paint(state, view, painter);
        if self.modal.is_active() {
            self.modal.paint(state, view, painter);
        }
    }

    fn options_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.inner.options_ui(ui, state);
    }

    fn takes_right_drag(&self, kind: hp_app::ViewKind, modifiers: egui::Modifiers) -> bool {
        self.modal.is_active() || self.inner.takes_right_drag(kind, modifiers)
    }

    fn wants_side_panel(&self) -> bool {
        self.inner.wants_side_panel()
    }

    fn panel_label_key(&self) -> &'static str {
        self.inner.panel_label_key()
    }

    fn cursor(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if self.modal.is_active() {
            return Some(CursorIcon::Crosshair);
        }
        self.inner.cursor(state, view, pos)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        if self.modal.is_active() {
            return Some(self.modal.hint(state));
        }
        self.inner.status_hint(state)
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        if self.modal.is_active() {
            self.modal.finish();
            return true;
        }
        self.inner.cancel(state)
    }

    fn face_selection(&self, state: &AppState) -> Vec<FaceRef> {
        self.inner.face_selection(state)
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        self.inner.set_option(state, key, value)
    }

    fn option_values(&self) -> Vec<ToolOption> {
        self.inner.option_values()
    }

    fn scripted_stroke(
        &mut self,
        state: &mut AppState,
        stroke: &hp_app::tool::ScriptedStroke,
    ) -> Option<Result<hp_app::tool::StrokeOutcome, String>> {
        self.inner.scripted_stroke(state, stroke)
    }
}

pub const INSET_MODAL_TR: hp_app::i18n::TrTable = &[
    ("inset_modal.thickness", "Inset", "Décalage"),
    ("inset_modal.depth", "Depth", "Profondeur"),
    ("inset_modal.brushes", "{0} brush(es)", "{0} brush(es)"),
    (
        "inset_modal.too_small",
        "Move the mouse toward the face center or type a thickness of at least 0.125 first",
        "Rapprochez la souris du centre de la face ou tapez d'abord une épaisseur d'au moins 0,125",
    ),
    (
        "inset_modal.hint_wait",
        "Inset: move the mouse over a view to start",
        "Décalage : placez la souris sur une vue pour commencer",
    ),
    (
        "inset_modal.hint",
        "Mouse: thickness (toward the face center = thicker), {0}: depth, Ctrl: grid, digits: exact value, {1} or click: apply, {2} or right click: cancel",
        "Souris : épaisseur (vers le centre de la face = plus épais), {0} : profondeur, Ctrl : grille, chiffres : valeur exacte, {1} ou clic : appliquer, {2} ou clic droit : annuler",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_toward_the_center_makes_it_thicker() {
        assert_eq!(thickness_from_distance(200.0, 200.0, 2.0), 0.0);
        assert_eq!(thickness_from_distance(200.0, 160.0, 2.0), 20.0);
        assert_eq!(thickness_from_distance(200.0, 260.0, 2.0), 0.0);
        assert_eq!(thickness_from_distance(200.0, 0.0, 0.0), 0.0);
        assert_eq!(thickness_from_distance(1e12, 0.0, 1.0), MAX_INSET);
    }

    #[test]
    fn depth_follows_the_normal_on_screen() {
        let up = Vec2::new(0.0, -2.0);
        assert_eq!(depth_from_drag(0.0, Vec2::new(0.0, -20.0), up, 2.0), 10.0);
        assert_eq!(depth_from_drag(4.0, Vec2::new(0.0, 20.0), up, 2.0), -6.0);
        assert_eq!(depth_from_drag(0.0, Vec2::new(30.0, 0.0), up, 2.0), 0.0);
        assert_eq!(depth_from_drag(0.0, Vec2::new(0.0, -20.0), Vec2::ZERO, 2.0), 10.0);
    }

    #[test]
    fn grid_snap_keeps_a_minimum() {
        assert_eq!(snap_value(13.0, 8.0, 8.0), 16.0);
        assert_eq!(snap_value(1.0, 8.0, 8.0), 8.0);
        assert_eq!(snap_value(-5.0, 8.0, 0.0), -8.0);
        assert_eq!(snap_value(0.0, 8.0, 0.0), 0.0);
        assert_eq!(snap_value(5.5, 0.0, 8.0), 5.5);
    }

    #[test]
    fn typed_numbers_are_checked() {
        let mut b = String::new();
        for ch in "-12,5".chars() {
            assert!(typed_push(&mut b, ch));
        }
        assert_eq!(b, "-12.5");
        assert!(!typed_push(&mut b, '.'));
        assert!(!typed_push(&mut b, '-'));
        assert!(!typed_push(&mut b, 'x'));
        assert_eq!(typed_value(&b), Some(-12.5));
        assert_eq!(typed_value("-"), None);
        assert_eq!(typed_value(""), None);
    }

    fn modal_on_solid_3() -> (AppState, InsetModal, ViewInfo) {
        let state = crate::tests_support::settle();
        let view = crate::tests_support::info_3d_looking_down(DVec3::new(0.0, 0.0, 600.0));
        let modal = InsetModal::start(&state, &[FaceRef::new(3, 5)]).expect("modal");
        (state, modal, view)
    }

    #[test]
    fn the_mouse_the_keys_and_the_apply() {
        let (mut state, mut m, view) = modal_on_solid_3();
        let c = view.world_to_screen(m.center).expect("center");
        m.on_pointer(&view, c + Vec2::new(150.0, 0.0), false);
        assert_eq!(m.thickness(), 0.0);
        m.on_pointer(&view, c + Vec2::new(100.0, 0.0), false);
        assert!(m.thickness() > 0.0);
        m.on_pointer(&view, c + Vec2::new(100.0, 40.0), true);
        assert!(m.thickness() > 0.0 && (m.thickness() / 16.0).fract() == 0.0, "ctrl snaps to the grid of 16");
        m.on_pointer(&view, c + Vec2::new(300.0, 0.0), false);
        assert_eq!(m.thickness(), 0.0);
        assert!(!m.confirm(&mut state), "a zero thickness is refused");
        assert!(m.is_active());
        for ch in "24".chars() {
            m.type_char(ch);
        }
        assert_eq!(m.thickness(), 24.0);
        m.on_pointer(&view, c + Vec2::new(10.0, 0.0), false);
        assert_eq!(m.thickness(), 24.0, "a typed value is not moved by the mouse");
        m.toggle_target();
        assert_eq!(m.target(), InsetTarget::Depth);
        for ch in "-8".chars() {
            m.type_char(ch);
        }
        assert_eq!(m.depth(), -8.0);
        assert!(m.label(&state).contains('['));
        assert!(m.confirm(&mut state));
        assert!(!m.is_active());
        assert_eq!(state.doc.selected_solid_ids().len(), 5);
        assert_eq!(state.doc.undo_label(), Some("Inset face"));
    }

    #[test]
    fn cancel_and_a_too_large_value_change_nothing() {
        let (mut state, mut m, view) = modal_on_solid_3();
        let before = state.doc.revision();
        let c = view.world_to_screen(m.center).expect("center");
        m.on_pointer(&view, c + Vec2::new(150.0, 0.0), false);
        m.set_values(5000.0, 0.0);
        assert!(m.error(&state).is_some());
        assert!(!m.confirm(&mut state));
        assert!(m.is_active());
        m.finish();
        assert!(!m.is_active());
        assert_eq!(state.doc.revision(), before);
        let mut out = Overlay::default();
        let (_, m2, _) = modal_on_solid_3();
        let mut ok = m2;
        ok.set_values(16.0, 8.0);
        ok.overlay(&state, &view, &mut out);
        assert!(!out.lines.is_empty() || !out.lines_on_top.is_empty());
        assert!(ok.brushes(&state).is_some());
    }
}
