use egui::{Align2, Color32, CursorIcon, FontId, Key, Modifiers, Painter, Pos2, Stroke};
use glam::{DVec2, DVec3};
use hp_app::theme::{VIEW_ACCENT, VIEW_ACCENT_BG};
use hp_app::view::Camera3d;
use hp_app::{Action, AppState, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};

use crate::input::PointerEvent;
use crate::math::{fmt_deg, normalize_deg};
use crate::select::{ClickCycle, click_select_3d};

const MARKER_PICK: f32 = 9.0;
const MIN_LOOK_DIST: f64 = 8.0;
const MAX_LOOK_DIST: f64 = 65536.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CamPart {
    Eye,
    Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CamDragKind {
    Eye,
    Target,
    Place,
    Create,
    Look,
}

#[derive(Clone, Copy, Debug)]
struct CamDrag {
    view: usize,
    kind: ViewKind,
    what: CamDragKind,
    start_cam: Camera3d,
    start_target: DVec3,
    start_look: f64,
    start2d: DVec2,
    last_pos: Pos2,
}

pub struct CameraTool {
    drag: Option<CamDrag>,
    look_dist: f64,
    hover: Option<(usize, CamPart)>,
    hover_view: Option<(usize, ViewKind)>,
    cycle: Option<ClickCycle>,
}

impl Default for CameraTool {
    fn default() -> Self {
        Self::new()
    }
}

pub fn move_eye(
    cam: &mut Camera3d,
    kind: ViewKind,
    start: &Camera3d,
    target: DVec3,
    delta: DVec2,
    keep_direction: bool,
) -> f64 {
    let eye = start.pos + kind.from_2d(delta, 0.0);
    cam.pos = eye;
    if keep_direction {
        cam.yaw_deg = start.yaw_deg;
        cam.pitch_deg = start.pitch_deg;
        (target - start.pos).length()
    } else {
        cam.look_at(target);
        (target - eye).length()
    }
}

pub fn move_target(cam: &mut Camera3d, kind: ViewKind, start_target: DVec3, delta: DVec2) -> f64 {
    let target = start_target + kind.from_2d(delta, 0.0);
    cam.look_at(target);
    (target - cam.pos).length()
}

const CLICK_ZOOM: f64 = 2.0;
const MAX_CAMERAS: usize = 256;

pub fn store_live(cams: &hp_vmf::Cameras, live: &Camera3d) -> hp_vmf::Cameras {
    let mut out = cams.clone();
    if !live.pos.is_finite() || !live.forward().is_finite() {
        return out;
    }
    let current = hp_vmf::Camera { position: live.pos, look: live.pos + live.forward() * 256.0 };
    match usize::try_from(out.active).ok().filter(|&i| i < out.cameras.len()) {
        Some(i) => {
            let old = &out.cameras[i];
            let dist = (old.look - old.position).length();
            let dist = if dist.is_finite() && dist >= MIN_LOOK_DIST { dist } else { 256.0 };
            out.cameras[i] = hp_vmf::Camera { position: live.pos, look: live.pos + live.forward() * dist };
        }
        None => {
            out.cameras.push(current);
            out.active = i32::try_from(out.cameras.len() - 1).unwrap_or(0);
        }
    }
    out
}

pub fn view_of(c: &hp_vmf::Camera, template: &Camera3d) -> Option<Camera3d> {
    if !c.position.is_finite() || !c.look.is_finite() || (c.look - c.position).length_squared() < 1e-6 {
        return None;
    }
    let mut cam = Camera3d { pos: c.position, ..*template };
    cam.look_at(c.look);
    Some(cam)
}

pub fn mouselook(cam: &mut Camera3d, delta: egui::Vec2, sensitivity: f64, invert_y: bool) {
    let dy = if invert_y { -f64::from(delta.y) } else { f64::from(delta.y) };
    cam.yaw_deg = normalize_deg(cam.yaw_deg - f64::from(delta.x) * sensitivity);
    cam.pitch_deg = (cam.pitch_deg - dy * sensitivity).clamp(-89.0, 89.0);
}

impl CameraTool {
    pub fn new() -> Self {
        Self { drag: None, look_dist: 256.0, hover: None, hover_view: None, cycle: None }
    }

    pub fn look_distance(&self) -> f64 {
        self.look_dist
    }

    fn target(&self, cam: &Camera3d) -> DVec3 {
        cam.pos + cam.forward() * self.look_dist
    }

    pub fn switch_camera(&mut self, state: &mut AppState, dir: i32) -> bool {
        let mut cams = store_live(state.doc.scene().cameras(), &state.cameras.cam3d);
        let n = cams.cameras.len();
        if n < 2 {
            return false;
        }
        let cur = usize::try_from(cams.active).unwrap_or(0).min(n - 1);
        let next = if dir >= 0 { (cur + 1) % n } else { (cur + n - 1) % n };
        cams.active = i32::try_from(next).unwrap_or(0);
        if let Some(view) = view_of(&cams.cameras[next], &state.cameras.cam3d) {
            self.look_dist =
                (cams.cameras[next].look - cams.cameras[next].position).length().clamp(MIN_LOOK_DIST, MAX_LOOK_DIST);
            state.cameras.cam3d = view;
        }
        state.doc.set_cameras(cams);
        true
    }

    pub fn create_camera(&mut self, state: &mut AppState, eye: DVec3, target: DVec3) -> bool {
        let mut cams = store_live(state.doc.scene().cameras(), &state.cameras.cam3d);
        if cams.cameras.len() >= MAX_CAMERAS || (target - eye).length() < MIN_LOOK_DIST {
            return false;
        }
        let cam = hp_vmf::Camera { position: eye, look: target };
        let Some(view) = view_of(&cam, &state.cameras.cam3d) else { return false };
        cams.cameras.push(cam);
        cams.active = i32::try_from(cams.cameras.len() - 1).unwrap_or(0);
        state.cameras.cam3d = view;
        self.look_dist = (target - eye).length().clamp(MIN_LOOK_DIST, MAX_LOOK_DIST);
        state.doc.set_cameras(cams);
        true
    }

    pub fn delete_camera(&mut self, state: &mut AppState) -> bool {
        let mut cams = state.doc.scene().cameras().clone();
        let n = cams.cameras.len();
        let Some(cur) = usize::try_from(cams.active).ok().filter(|&i| i < n) else { return false };
        cams.cameras.remove(cur);
        if cams.cameras.is_empty() {
            let d = Camera3d::default();
            cams.cameras.push(hp_vmf::Camera { position: d.pos, look: d.pos + d.forward() * 256.0 });
        }
        let next = cur.min(cams.cameras.len() - 1);
        cams.active = i32::try_from(next).unwrap_or(0);
        if let Some(view) = view_of(&cams.cameras[next], &state.cameras.cam3d) {
            state.cameras.cam3d = view;
        }
        state.doc.set_cameras(cams);
        true
    }

    fn camera_count(state: &AppState) -> (usize, usize) {
        let cams = state.doc.scene().cameras();
        let n = cams.cameras.len();
        match usize::try_from(cams.active).ok().filter(|&i| i < n) {
            Some(i) => (i, n),
            None => (n, n + 1),
        }
    }

    pub fn hit_part(&self, cam: &Camera3d, view: &ViewInfo, pos: Pos2) -> Option<CamPart> {
        if !view.is_2d() {
            return None;
        }
        let eye = view.world_to_screen(cam.pos)?;
        let target = view.world_to_screen(self.target(cam))?;
        let de = (eye - pos).length();
        let dt = (target - pos).length();
        match (de <= MARKER_PICK, dt <= MARKER_PICK) {
            (true, true) => Some(if dt < de { CamPart::Target } else { CamPart::Eye }),
            (true, false) => Some(CamPart::Eye),
            (false, true) => Some(CamPart::Target),
            (false, false) => None,
        }
    }

    pub(crate) fn handle_event(
        &mut self,
        state: &mut AppState,
        view: &ViewInfo,
        ev: PointerEvent,
        mods: Modifiers,
    ) -> ToolFlow {
        match ev {
            PointerEvent::Hover(p) => {
                self.hover_view = Some((view.index, view.kind));
                if self.drag.is_none() {
                    self.hover = self.hit_part(&state.cameras.cam3d, view, p).map(|h| (view.index, h));
                }
                ToolFlow::Ignored
            }
            PointerEvent::Leave => {
                if self.hover.is_some_and(|(v, _)| v == view.index) {
                    self.hover = None;
                }
                ToolFlow::Ignored
            }
            PointerEvent::DragStart { origin, pos } => {
                let cam = state.cameras.cam3d;
                let what = if view.is_2d() {
                    match self.hit_part(&cam, view, origin) {
                        Some(CamPart::Eye) => CamDragKind::Eye,
                        Some(CamPart::Target) => CamDragKind::Target,
                        None if mods.shift => CamDragKind::Create,
                        None => CamDragKind::Place,
                    }
                } else {
                    CamDragKind::Look
                };
                self.drag = Some(CamDrag {
                    view: view.index,
                    kind: view.kind,
                    what,
                    start_cam: cam,
                    start_target: self.target(&cam),
                    start_look: self.look_dist,
                    start2d: view.screen_to_2d(origin),
                    last_pos: origin,
                });
                self.update(state, view, pos, mods);
                ToolFlow::Handled
            }
            PointerEvent::Drag(p) => {
                if self.drag.is_some_and(|d| d.view == view.index) {
                    self.update(state, view, p, mods);
                    ToolFlow::Handled
                } else {
                    ToolFlow::Ignored
                }
            }
            PointerEvent::DragEnd(p) => {
                if self.drag.is_some_and(|d| d.view == view.index) {
                    self.update(state, view, p, mods);
                    if let Some(d) = self.drag.take()
                        && d.what == CamDragKind::Create
                        && (state.cameras.cam3d.pos != d.start_cam.pos
                            || state.cameras.cam3d.forward() != d.start_cam.forward())
                    {
                        let new = state.cameras.cam3d;
                        state.cameras.cam3d = d.start_cam;
                        self.look_dist = d.start_look;
                        let target = new.pos + new.forward() * self.look_dist_of(&new, view, p);
                        self.create_camera(state, new.pos, target);
                    }
                    ToolFlow::Handled
                } else {
                    ToolFlow::Ignored
                }
            }
            PointerEvent::Click(p) => {
                if !view.is_2d() {
                    click_select_3d(state, view, p, mods, &mut self.cycle);
                } else if state.settings.selection.click_to_zoom {
                    let factor = if crate::input::ctrl(mods) { 1.0 / CLICK_ZOOM } else { CLICK_ZOOM };
                    let anchor = view.screen_to_2d(p);
                    state.cameras.cam2d_mut(view.kind).zoom_at(factor, anchor);
                }
                ToolFlow::Handled
            }
            PointerEvent::DoubleClick(p) if !view.is_2d() => {
                click_select_3d(state, view, p, mods, &mut self.cycle);
                ToolFlow::Handled
            }
            PointerEvent::DoubleClick(_) => ToolFlow::Handled,
        }
    }

    fn update(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        let Some(d) = self.drag.as_mut() else { return };
        if d.view != view.index {
            return;
        }
        let cam = &mut state.cameras.cam3d;
        let p2 = view.screen_to_2d(pos);
        let delta = p2 - d.start2d;
        match d.what {
            CamDragKind::Eye => {
                self.look_dist = move_eye(cam, d.kind, &d.start_cam, d.start_target, delta, mods.shift)
                    .clamp(MIN_LOOK_DIST, MAX_LOOK_DIST);
            }
            CamDragKind::Target => {
                self.look_dist = move_target(cam, d.kind, d.start_target, delta).clamp(MIN_LOOK_DIST, MAX_LOOK_DIST);
            }
            CamDragKind::Place | CamDragKind::Create => {
                if let Some((_, _, depth_axis)) = d.kind.axes_2d() {
                    let depth = d.start_cam.pos[depth_axis];
                    let eye = d.kind.from_2d(d.start2d, depth);
                    let target = d.kind.from_2d(p2, depth);
                    let dist = (target - eye).length();
                    if dist >= MIN_LOOK_DIST {
                        cam.pos = eye;
                        cam.look_at(target);
                        self.look_dist = dist.min(MAX_LOOK_DIST);
                    }
                }
            }
            CamDragKind::Look => {
                let delta = pos - d.last_pos;
                let sens = f64::from(state.settings.view.mouse_sensitivity);
                mouselook(cam, delta, sens, state.settings.view.invert_mouse_y);
            }
        }
        d.last_pos = pos;
    }

    fn look_dist_of(&self, cam: &Camera3d, view: &ViewInfo, p: Pos2) -> f64 {
        let Some((_, _, depth_axis)) = view.kind.axes_2d() else { return self.look_dist };
        let target = view.kind.from_2d(view.screen_to_2d(p), cam.pos[depth_axis]);
        (target - cam.pos).length().clamp(MIN_LOOK_DIST, MAX_LOOK_DIST)
    }

    fn restore(&mut self, state: &mut AppState) -> bool {
        match self.drag.take() {
            Some(d) => {
                state.cameras.cam3d = d.start_cam;
                self.look_dist = d.start_look;
                true
            }
            None => false,
        }
    }
}

impl Tool for CameraTool {
    fn id(&self) -> ToolId {
        ToolId::Camera
    }

    fn deactivate(&mut self, _state: &mut AppState) {
        self.drag = None;
        self.hover = None;
        self.cycle = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        let mut flow = ToolFlow::Ignored;
        for ev in crate::input::events(input) {
            if self.handle_event(state, input.view, ev, input.modifiers) == ToolFlow::Handled {
                flow = ToolFlow::Handled;
            }
        }
        if self.drag.is_some_and(|d| d.view == input.view.index) && !crate::input::primary_down(input) {
            self.drag = None;
        }
        flow
    }

    fn click_while_flying(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) -> ToolFlow {
        if view.is_2d() || self.drag.is_some() {
            return ToolFlow::Ignored;
        }
        click_select_3d(state, view, pos, mods, &mut self.cycle);
        ToolFlow::Handled
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.drag.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            return self.restore(state);
        }
        if self.drag.is_some() && !ctx.input(|i| i.pointer.primary_down()) {
            self.drag = None;
        }
        if self.drag.is_some() || state.ui.hovered_view.is_none() {
            return false;
        }
        let plain = |k: Key| {
            ctx.input_mut(|i| {
                let before = i.events.len();
                i.events.retain(|e| {
                    !matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. }
                        if *key == k && !modifiers.any())
                });
                before - i.events.len()
            })
        };
        let mut handled = false;
        for _ in 0..plain(Key::PageDown) {
            handled |= self.switch_camera(state, 1);
        }
        for _ in 0..plain(Key::PageUp) {
            handled |= self.switch_camera(state, -1);
        }
        if plain(Key::Delete) > 0 {
            handled |= self.delete_camera(state);
        }
        handled
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &Painter) {
        crate::select::paint_filter_badge(state, view, painter);
        if !view.is_2d() {
            return;
        }
        let clip = crate::screen::view_clip(view.rect);
        let cams = state.doc.scene().cameras();
        for (i, c) in cams.cameras.iter().enumerate() {
            if usize::try_from(cams.active).ok() == Some(i) {
                continue;
            }
            let (Some(e), Some(l)) = (view.world_to_screen(c.position), view.world_to_screen(c.look)) else { continue };
            let faded = VIEW_ACCENT.gamma_multiply(0.45);
            crate::screen::clipped_segment(painter, clip, e, l, Stroke::new(1.0, faded));
            if clip.contains(e) {
                painter.circle(e, 5.0, VIEW_ACCENT_BG.gamma_multiply(0.6), Stroke::new(1.5, faded));
            }
        }
        let cam = &state.cameras.cam3d;
        let target = self.target(cam);
        let (Some(eye_s), Some(target_s)) = (view.world_to_screen(cam.pos), view.world_to_screen(target)) else {
            return;
        };
        let half_v = (cam.fov_y_deg.to_radians() * 0.5).tan();
        let half_h = (half_v * 16.0 / 9.0).atan();
        let fwd = cam.forward();
        let right = cam.right();
        let wedge = self.look_dist.clamp(32.0, 512.0);
        for sign in [-1.0, 1.0] {
            let dir = fwd * half_h.cos() + right * (half_h.sin() * sign);
            if let Some(p) = view.world_to_screen(cam.pos + dir * wedge) {
                crate::screen::clipped_segment(
                    painter,
                    clip,
                    eye_s,
                    p,
                    Stroke::new(1.0, VIEW_ACCENT.gamma_multiply(0.45)),
                );
            }
        }
        crate::screen::clipped_segment(painter, clip, eye_s, target_s, Stroke::new(1.5, VIEW_ACCENT));
        let hovered = |part: CamPart| {
            self.hover == Some((view.index, part))
                || self.drag.is_some_and(|d| {
                    matches!((d.what, part), (CamDragKind::Eye, CamPart::Eye) | (CamDragKind::Target, CamPart::Target))
                })
        };
        let eye_stroke = if hovered(CamPart::Eye) { Color32::WHITE } else { VIEW_ACCENT };
        painter.circle(eye_s, 6.5, VIEW_ACCENT_BG, Stroke::new(2.0, eye_stroke));
        painter.circle_filled(eye_s, 2.0, eye_stroke);
        let target_fill = if hovered(CamPart::Target) { VIEW_ACCENT } else { Color32::WHITE };
        painter.circle(target_s, 4.5, target_fill, Stroke::new(1.5, VIEW_ACCENT));
        let label = |p: Pos2, text: &str| {
            let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(11.0), Color32::from_gray(235));
            let rect =
                Align2::LEFT_BOTTOM.anchor_size(p + egui::vec2(9.0, -9.0), galley.size()).expand2(egui::vec2(3.0, 1.0));
            painter.rect_filled(rect, 3.0, Color32::from_black_alpha(170));
            painter.galley(rect.min + egui::vec2(3.0, 1.0), galley, Color32::from_gray(235));
        };
        if hovered(CamPart::Eye) {
            label(eye_s, t!("tool.camera"));
        }
        if hovered(CamPart::Target) {
            label(target_s, t!("camtool.target"));
        }
    }

    fn options_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            let (index, count) = Self::camera_count(state);
            ui.label(tf!("camtool.camera_n", index + 1, count)).on_hover_text(t!("camtool.cameras_tip"));
            if ui.add_enabled(count > 1, egui::Button::new("◀")).on_hover_text(t!("camtool.previous")).clicked() {
                self.switch_camera(state, -1);
            }
            if ui.add_enabled(count > 1, egui::Button::new("▶")).on_hover_text(t!("camtool.next")).clicked() {
                self.switch_camera(state, 1);
            }
            let saved = !state.doc.scene().cameras().cameras.is_empty();
            if ui.add_enabled(saved, egui::Button::new("🗑")).on_hover_text(t!("camtool.delete")).clicked() {
                self.delete_camera(state);
            }
            ui.separator();
            let cam = &mut state.cameras.cam3d;
            ui.label(t!("camtool.position"));
            hp_app::widgets::vec3_edit(ui, &mut cam.pos, 1.0);
            ui.separator();
            ui.label(t!("camtool.yaw"));
            if ui.add(egui::DragValue::new(&mut cam.yaw_deg).speed(0.5).suffix("°").max_decimals(1)).changed() {
                cam.yaw_deg = normalize_deg(cam.yaw_deg);
            }
            ui.label(t!("camtool.pitch"));
            ui.add(egui::DragValue::new(&mut cam.pitch_deg).speed(0.5).range(-89.0..=89.0).suffix("°").max_decimals(1));
            ui.separator();
            let has_selection = !state.doc.selection().objects.is_empty();
            if ui
                .add_enabled(has_selection, egui::Button::new(t!("camtool.look_at_selection")))
                .on_hover_text(t!("camtool.look_at_selection_tip"))
                .clicked()
                && let Some(c) = state
                    .doc
                    .scene()
                    .objects_bounds(&state.doc.selected_objects(), &|e| state.assets.entity_bounds(e))
                    .map(|b| b.center())
            {
                state.cameras.cam3d.look_at(c);
                self.look_dist = (c - state.cameras.cam3d.pos).length().clamp(MIN_LOOK_DIST, MAX_LOOK_DIST);
            }
            let center_tip = match state.keys.shortcut(Action::Center3dOnSelection) {
                Some(sc) => {
                    format!("{} ({})", Action::Center3dOnSelection.label(), hp_app::actions::shortcut_label(&sc))
                }
                None => Action::Center3dOnSelection.label().to_owned(),
            };
            if ui
                .add_enabled(has_selection, egui::Button::new(Action::Center3dOnSelection.label()))
                .on_hover_text(center_tip)
                .clicked()
            {
                state.request(Action::Center3dOnSelection);
            }
            if ui.button(t!("common.reset")).on_hover_text(t!("camtool.reset_tip")).clicked() {
                let fov = state.cameras.cam3d.fov_y_deg;
                let far = state.cameras.cam3d.far;
                state.cameras.cam3d = Camera3d { fov_y_deg: fov, far, ..Camera3d::default() };
            }
        });
    }

    fn cursor(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if self.drag.is_some() {
            return Some(CursorIcon::Grabbing);
        }
        if !view.is_2d() {
            return Some(CursorIcon::Grab);
        }
        if self.hit_part(&state.cameras.cam3d, view, pos).is_some() {
            Some(CursorIcon::Grab)
        } else {
            Some(CursorIcon::Crosshair)
        }
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        let cam = &state.cameras.cam3d;
        if self.drag.is_some() {
            return Some(tf!(
                "camtool.hint.drag",
                hp_app::widgets::fmt_vec3(cam.pos.round()),
                fmt_deg(cam.yaw_deg),
                fmt_deg(cam.pitch_deg)
            ));
        }
        match self.hover_view {
            Some((_, kind)) if !kind.is_2d() => Some(t!("camtool.hint.3d").to_owned()),
            _ if state.settings.selection.click_to_zoom => Some(t!("camtool.hint.2d_zoom").to_owned()),
            _ => Some(t!("camtool.hint.2d").to_owned()),
        }
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        self.restore(state)
    }
}
