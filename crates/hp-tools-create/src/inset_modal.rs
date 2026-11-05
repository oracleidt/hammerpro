use std::cell::RefCell;

use egui::{Align2, CursorIcon, Event, Key, PointerButton, Pos2, Vec2};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::tool::ToolOption;
use hp_app::{AppState, Severity, Tool, ToolFlow, ToolId, ToolMenuEntry, ViewInfo, ViewInput, t, tf};
use hp_editor::{EditError, FaceRef};
use hp_geom::{InsetIssue, InsetParams, MAX_INSET, MIN_EXTRUDE_DEPTH, MIN_INSET, Plane, region_rings};
use hp_render::Overlay;

use crate::common::{COLOR_FRONT, COLOR_GHOST, COLOR_PENDING, DistanceSnap, Ov, fmt_len, paint_label, rgba};

const COLOR_CARVE: egui::Color32 = hp_app::data_colors::CARVE;

pub const DEPTH_KEY: Key = Key::E;

pub const INDIVIDUAL_KEY: Key = Key::I;

const MAX_TYPED: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsetTarget {
    Thickness,
    Depth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalKind {
    Inset,
    Extrude,
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
        InsetIssue::BadRegion => "inset.err_region",
    }
}

pub fn hovered_face(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<FaceRef> {
    if view.is_2d() {
        return None;
    }
    let hit = crate::common::pick_surface(state, view, pos)?;
    Some(FaceRef::new(hit.solid, hit.side_index))
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
    key: Option<(u64, u64, u64, u64)>,
    outcome: Option<Result<usize, String>>,
}

pub struct InsetModal {
    kind: ModalKind,
    individual: bool,
    raw_depth: f64,
    snap: DistanceSnap,
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
        Self::start_kind(state, faces, ModalKind::Inset)
    }

    pub fn start_extrude(state: &AppState, faces: &[FaceRef]) -> Option<Self> {
        Self::start_kind(state, faces, ModalKind::Extrude)
    }

    fn start_kind(state: &AppState, faces: &[FaceRef], kind: ModalKind) -> Option<Self> {
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
            kind,
            individual: false,
            raw_depth: 0.0,
            snap: DistanceSnap::OFF,
            faces: data,
            center,
            normal,
            revision: state.doc.revision(),
            anchor: None,
            pointer: None,
            thickness: 0.0,
            depth: 0.0,
            target: if kind == ModalKind::Extrude { InsetTarget::Depth } else { InsetTarget::Thickness },
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

    pub fn kind(&self) -> ModalKind {
        self.kind
    }

    pub fn is_individual(&self) -> bool {
        self.individual
    }

    pub fn toggle_individual(&mut self) {
        if self.kind == ModalKind::Inset && self.faces.len() > 1 {
            self.individual = !self.individual;
        }
    }

    pub fn is_alternate_grid(&self) -> bool {
        self.snap.is_alternate()
    }

    fn primary(&self) -> &FaceData {
        &self.faces[0]
    }

    fn aligned_depth(&self, raw: f64) -> f64 {
        let f = self.primary();
        self.snap.distance(f.vertices[0], f.plane.normal, raw)
    }

    fn depths(&self) -> Vec<f64> {
        self.faces
            .iter()
            .map(|f| {
                if self.snap.is_on() && self.typed.is_empty() {
                    self.snap.distance(f.vertices[0], f.plane.normal, self.raw_depth)
                } else {
                    self.depth
                }
            })
            .collect()
    }

    fn extrusions(&self) -> Vec<(FaceRef, f64)> {
        self.faces
            .iter()
            .zip(self.depths())
            .filter(|(_, d)| d.abs() >= MIN_EXTRUDE_DEPTH)
            .map(|(f, d)| (f.face, d))
            .collect()
    }

    fn snap_note(&self) -> String {
        match (self.snap.nominal(), self.snap.is_alternate()) {
            (Some(step), true) => tf!("modal.grid_alt", fmt_len(step)),
            (Some(step), false) => tf!("modal.grid", fmt_len(step)),
            (None, _) => t!("modal.grid_free").to_owned(),
        }
    }

    pub fn params(&self) -> InsetParams {
        InsetParams { distance: self.thickness, depth: self.depth, material: None, individual: self.individual }
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
            InsetTarget::Depth => {
                self.depth = v.clamp(-MAX_INSET, MAX_INSET);
                self.raw_depth = self.depth;
            }
        }
    }

    pub(crate) fn on_pointer(&mut self, view: &ViewInfo, pos: Pos2, snap: DistanceSnap) {
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
        self.snap = snap;
        let face_normal = self.primary().plane.normal;
        match self.target {
            InsetTarget::Thickness => {
                let v = thickness_from_distance(anchor.base, now, ppu);
                self.thickness = match snap.step(face_normal) {
                    Some(step) if v > 0.0 => snap.thickness(v, face_normal).max(step.max(MIN_INSET)),
                    _ => v,
                };
            }
            InsetTarget::Depth => {
                let from = self.depth_anchor.unwrap_or(pos);
                let n = normal_pixels(view, self.center, self.normal);
                let v = depth_from_drag(self.depth_base, pos - from, n, ppu);
                self.raw_depth = v;
                self.depth = if snap.is_on() { self.aligned_depth(v) } else { v };
            }
        }
    }

    fn stale(&self, state: &AppState) -> bool {
        state.doc.revision() != self.revision
    }

    pub fn confirm(&mut self, state: &mut AppState) -> bool {
        if self.kind == ModalKind::Extrude {
            return self.confirm_extrude(state);
        }
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

    fn confirm_extrude(&mut self, state: &mut AppState) -> bool {
        let list = self.extrusions();
        if list.is_empty() {
            state.notify(Severity::Info, t!("extrude_modal.too_small"));
            return false;
        }
        match state.doc.extrude_faces(&list, None) {
            Ok(ids) => {
                state.log(Severity::Info, tf!("extrude_modal.done", ids.len()));
                self.finished = true;
                true
            }
            Err(e) => {
                state.notify(Severity::Warning, tf!("app.op_failed", t!("action.extrude_interactive"), error_text(&e)));
                false
            }
        }
    }

    fn preview_key(&self, state: &AppState) -> (u64, u64, u64, u64) {
        let depths = self.depths().iter().fold(0u64, |h, d| h.rotate_left(7) ^ d.to_bits());
        (state.doc.revision(), self.thickness.to_bits(), self.depth.to_bits(), depths ^ u64::from(self.individual))
    }

    fn refresh_preview(&self, state: &AppState) {
        let key = self.preview_key(state);
        let Ok(mut pv) = self.preview.try_borrow_mut() else { return };
        if pv.key == Some(key) {
            return;
        }
        pv.key = Some(key);
        pv.outcome = match self.kind {
            ModalKind::Inset => (self.thickness >= MIN_INSET).then(|| {
                let faces: Vec<FaceRef> = self.faces.iter().map(|f| f.face).collect();
                match state.doc.inset_preview(&faces, &self.params()) {
                    Ok(plan) => Ok(plan.iter().map(|(_, pieces)| pieces.len()).sum()),
                    Err(e) => Err(error_text(&e)),
                }
            }),
            ModalKind::Extrude => {
                let list = self.extrusions();
                (!list.is_empty()).then(|| state.doc.extrude_faces_preview(&list, None).map_err(|e| error_text(&e)))
            }
        };
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
        let mut s = match self.kind {
            ModalKind::Inset => {
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
                if self.faces.len() > 1 {
                    let mode = if self.individual { t!("inset_modal.individual") } else { t!("inset_modal.region") };
                    s.push_str(&format!("   {mode}"));
                }
                s
            }
            ModalKind::Extrude => {
                format!("{}: {}", t!("extrude_modal.distance"), self.value_text(InsetTarget::Depth, self.depth))
            }
        };
        s.push_str(&format!("   {}", self.snap_note()));
        if let Some(e) = self.error(state) {
            s.push_str(&format!("   ({e})"));
        } else if let Some(n) = self.brushes(state) {
            s.push_str(&format!("   {}", tf!("inset_modal.brushes", n)));
        }
        s
    }

    pub fn hint(&self, state: &AppState) -> String {
        let enter = key_label(Key::Enter);
        let esc = key_label(Key::Escape);
        if self.anchor.is_none() {
            let key = if self.kind == ModalKind::Inset { "inset_modal.hint_wait" } else { "extrude_modal.hint_wait" };
            return t!(key).to_owned();
        }
        match self.kind {
            ModalKind::Inset => {
                let e = key_label(DEPTH_KEY);
                let i = key_label(INDIVIDUAL_KEY);
                format!("{}   {}", self.label(state), tf!("inset_modal.hint", e, enter, esc, i))
            }
            ModalKind::Extrude => format!("{}   {}", self.label(state), tf!("extrude_modal.hint", enter, esc)),
        }
    }

    pub fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        match self.kind {
            ModalKind::Inset => self.overlay_inset(state, view, out),
            ModalKind::Extrude => self.overlay_extrude(state, view, out),
        }
    }

    fn overlay_extrude(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let failed = self.error(state).is_some();
        let mut ov = Ov::new(out, view);
        let ghost = rgba(COLOR_GHOST, 0.8);
        for (f, d) in self.faces.iter().zip(self.depths()) {
            if d.abs() < MIN_EXTRUDE_DEPTH {
                continue;
            }
            let carving = d < 0.0;
            let tone = if carving { COLOR_CARVE } else { COLOR_PENDING };
            let edge = rgba(if failed { hp_app::theme::VIEW_ERROR } else { tone }, 1.0);
            let fill = rgba(tone, 0.28);
            let lift = f.plane.normal * d;
            let top: Vec<DVec3> = f.vertices.iter().map(|v| *v + lift).collect();
            for v in &f.vertices {
                ov.xray_line(*v, *v + lift, ghost);
            }
            if carving {
                ov.polygon_outline(&f.vertices, edge, true);
            }
            ov.polygon_outline(&top, edge, true);
            ov.polygon_fill(&top, fill);
        }
    }

    fn overlay_inset(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if self.thickness < MIN_INSET {
            return;
        }
        let failed = self.error(state).is_some();
        let mut ov = Ov::new(out, view);
        let cut = rgba(COLOR_FRONT, 1.0);
        let bad = rgba(hp_app::theme::VIEW_ERROR, 1.0);
        let inner = rgba(if failed { hp_app::theme::VIEW_ERROR } else { COLOR_PENDING }, 1.0);
        let fill = rgba(COLOR_PENDING, 0.28);
        let ghost = rgba(COLOR_GHOST, 0.8);
        let shapes: Vec<(usize, Plane, Vec<DVec3>)> = self
            .faces
            .iter()
            .map(|f| (usize::try_from(f.face.solid).unwrap_or(usize::MAX), f.plane, f.vertices.clone()))
            .collect();
        for (group, ring) in region_rings(&shapes, self.thickness, self.individual) {
            let Some(ring) = ring else {
                for i in group {
                    ov.polygon_outline(&self.faces[i].vertices, bad, true);
                }
                continue;
            };
            let lift = self.faces[group[0]].plane.normal * self.depth;
            let top: Vec<DVec3> = ring.inner.iter().map(|p| *p + lift).collect();
            for (o, i) in ring.outer.iter().zip(&ring.inner) {
                ov.xray_line(*o, *i, cut);
                if self.depth != 0.0 {
                    ov.xray_line(*i, *i + lift, ghost);
                }
            }
            ov.polygon_outline(&ring.outer, cut, true);
            ov.polygon_outline(&top, inner, true);
            if group.len() == 1 {
                ov.polygon_fill(&top, fill);
            }
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
            let snap = match self.kind {
                ModalKind::Inset => DistanceSnap::for_inset(state, input.view, input.modifiers),
                ModalKind::Extrude => DistanceSnap::for_extrude(state, input.view, input.modifiers),
            };
            self.on_pointer(input.view, pos, snap);
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
                k if k == DEPTH_KEY && self.kind == ModalKind::Inset => self.toggle_target(),
                k if k == INDIVIDUAL_KEY => self.toggle_individual(),
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
    ("inset_modal.region", "[region]", "[région]"),
    ("inset_modal.individual", "[individual]", "[individuel]"),
    ("inset_modal.brushes", "{0} brush(es)", "{0} brush(es)"),
    ("modal.grid", "grid {0}", "grille {0}"),
    ("modal.grid_alt", "alt grid {0}", "grille alt. {0}"),
    ("modal.grid_free", "free", "libre"),
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
        "Mouse: thickness (toward the face center = thicker), {0}: depth, {3}: region or individual, Ctrl: force the grid, Alt: free, digits: exact value, {1} or click: apply, {2} or right click: cancel",
        "Souris : épaisseur (vers le centre de la face = plus épais), {0} : profondeur, {3} : région ou individuel, Ctrl : forcer la grille, Alt : libre, chiffres : valeur exacte, {1} ou clic : appliquer, {2} ou clic droit : annuler",
    ),
    ("extrude_modal.distance", "Extrude", "Extrusion"),
    (
        "extrude_modal.too_small",
        "Move the mouse along the face normal or type a distance of at least 0.125 (negative carves) first",
        "Déplacez la souris le long de la normale de la face ou tapez d'abord une distance d'au moins 0,125 (négative : creuser)",
    ),
    (
        "extrude_modal.hint_wait",
        "Extrude: move the mouse over a view to start",
        "Extrusion : placez la souris sur une vue pour commencer",
    ),
    (
        "extrude_modal.hint",
        "Mouse: distance along the face normal (inward = carve into the brush), Alt: alternate grid, Ctrl: force the grid, digits: exact value, {0} or click: apply, {1} or right click: cancel",
        "Souris : distance le long de la normale de la face (vers l'intérieur = creuser le brush), Alt : grille alternative, Ctrl : forcer la grille, chiffres : valeur exacte, {0} ou clic : appliquer, {1} ou clic droit : annuler",
    ),
    ("extrude_modal.done", "Extruded or carved {0} brush(es)", "{0} brush(es) extrudé(s) ou creusé(s)"),
    (
        "extrude_modal.need_face",
        "Select faces or put the mouse over a face in a 3D view to extrude",
        "Sélectionnez des faces ou placez la souris sur une face dans une vue 3D pour extruder",
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

    fn grid16() -> DistanceSnap {
        DistanceSnap::with_frame(hp_app::grid::GridFrame::uniform(16.0), false)
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
        m.on_pointer(&view, c + Vec2::new(150.0, 0.0), DistanceSnap::OFF);
        assert_eq!(m.thickness(), 0.0);
        m.on_pointer(&view, c + Vec2::new(100.0, 0.0), DistanceSnap::OFF);
        assert!(m.thickness() > 0.0);
        m.on_pointer(&view, c + Vec2::new(100.0, 40.0), grid16());
        assert!(m.thickness() > 0.0 && (m.thickness() / 16.0).fract() == 0.0, "ctrl snaps to the grid of 16");
        m.on_pointer(&view, c + Vec2::new(300.0, 0.0), DistanceSnap::OFF);
        assert_eq!(m.thickness(), 0.0);
        assert!(!m.confirm(&mut state), "a zero thickness is refused");
        assert!(m.is_active());
        for ch in "24".chars() {
            m.type_char(ch);
        }
        assert_eq!(m.thickness(), 24.0);
        m.on_pointer(&view, c + Vec2::new(10.0, 0.0), DistanceSnap::OFF);
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
        m.on_pointer(&view, c + Vec2::new(150.0, 0.0), DistanceSnap::OFF);
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

    #[test]
    fn the_extrude_modal_grows_a_new_brush_aligned_to_the_grid() {
        let mut state = crate::tests_support::settle();
        let view = crate::tests_support::info_3d_looking_down(DVec3::new(0.0, 0.0, 600.0));
        let before = state.doc.scene().solids().len();
        let mut m = InsetModal::start_extrude(&state, &[FaceRef::new(3, 5)]).expect("modal");
        assert_eq!(m.kind(), ModalKind::Extrude);
        assert_eq!(m.target(), InsetTarget::Depth);
        let c = view.world_to_screen(m.center).expect("center");
        let n = normal_pixels(&view, m.center, m.normal);
        let out = if n.length() > 0.5 { n.normalized() * 90.0 } else { Vec2::new(0.0, -90.0) };
        m.on_pointer(&view, c, DistanceSnap::OFF);
        m.on_pointer(&view, c - out, DistanceSnap::OFF);
        assert!(m.depth() < 0.0, "pulling inward carves");
        let mut carve = Overlay::default();
        m.overlay(&state, &view, &mut carve);
        assert!(!carve.lines.is_empty() || !carve.lines_on_top.is_empty());
        m.set_values(0.0, 0.0);
        assert!(!m.confirm(&mut state));
        assert!(m.is_active());
        m.on_pointer(&view, c + out, DistanceSnap::OFF);
        let free = m.depth();
        assert!(free > 0.0);
        m.on_pointer(&view, c + out, grid16());
        let base = m.faces[0].vertices[0].z;
        let tip = (base + m.depth()) / 16.0;
        assert!((tip - tip.round()).abs() < 1e-9, "the new face lies on the grid: {tip}");
        let fine = DistanceSnap::with_frame(hp_app::grid::GridFrame::uniform(16.0).alternate(), true);
        m.on_pointer(&view, c + out, fine);
        let tip = (base + m.depth()) / 8.0;
        assert!((tip - tip.round()).abs() < 1e-9 && m.is_alternate_grid());
        assert!(m.label(&state).contains("alt grid 8"));
        assert!(m.hint(&state).contains("Alt"));
        let mut out = Overlay::default();
        m.overlay(&state, &view, &mut out);
        assert!(!out.lines.is_empty() || !out.lines_on_top.is_empty());
        assert!(m.confirm(&mut state));
        assert!(!m.is_active());
        assert_eq!(state.doc.scene().solids().len(), before + 1);
        assert_eq!(state.doc.undo_label(), Some("Extrude face"));
    }

    #[test]
    fn a_negative_extrusion_carves_the_brush_instead_of_adding_one() {
        let mut state = crate::tests_support::settle();
        let before = state.doc.scene().solids().len();
        let volume = state.doc.scene().solid(3).expect("solid").geom.volume();
        let mut m = InsetModal::start_extrude(&state, &[FaceRef::new(3, 5)]).expect("modal");
        for ch in "-2".chars() {
            assert!(m.type_char(ch));
        }
        assert_eq!(m.depth(), -2.0);
        assert!(m.label(&state).contains("-2"));
        assert_eq!(m.brushes(&state), Some(1));
        assert!(m.confirm(&mut state));
        assert_eq!(state.doc.scene().solids().len(), before);
        assert_eq!(state.doc.undo_label(), Some("Carve face"));
        let after = state.doc.scene().solid(3).expect("solid");
        assert!(after.geom.valid && after.geom.volume() < volume);
        assert!(state.doc.undo());
        assert!((state.doc.scene().solid(3).expect("solid").geom.volume() - volume).abs() < 1e-6);
    }

    #[test]
    fn typed_extrusions_ignore_the_grid_and_several_faces_make_several_brushes() {
        let mut state = crate::tests_support::settle();
        let before = state.doc.scene().solids().len();
        let faces = [FaceRef::new(3, 5), FaceRef::new(3, 4)];
        let mut m = InsetModal::start_extrude(&state, &faces).expect("modal");
        assert_eq!(m.face_count(), 2);
        m.snap = grid16();
        for ch in "13".chars() {
            m.type_char(ch);
        }
        assert_eq!(m.depth(), 13.0);
        assert_eq!(m.depths(), vec![13.0, 13.0]);
        assert!(m.confirm(&mut state));
        assert_eq!(state.doc.scene().solids().len(), before + 2);
    }

    #[test]
    fn region_and_individual_insets_are_toggled_for_several_faces_only() {
        let state = crate::tests_support::settle();
        let mut one = InsetModal::start(&state, &[FaceRef::new(3, 5)]).expect("one");
        one.toggle_individual();
        assert!(!one.is_individual());
        let mut two = InsetModal::start(&state, &[FaceRef::new(3, 5), FaceRef::new(3, 4)]).expect("two");
        assert!(!two.is_individual() && !two.params().individual);
        two.toggle_individual();
        assert!(two.is_individual() && two.params().individual);
        two.toggle_individual();
        assert!(!two.is_individual());
    }
}
