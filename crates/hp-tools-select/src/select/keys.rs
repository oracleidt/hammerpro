use egui::{Key, Modifiers};
use glam::{DMat4, DVec3};
use hp_app::view::Camera3d;
use hp_app::{AppState, ViewKind};

use super::{Drag, DragOp, SelectTool};
use crate::math::{axis_vec, dominant_horizontal_axis};
use crate::modal::ModalKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NudgeKey {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
}

impl NudgeKey {
    fn from_key(k: Key) -> Option<Self> {
        Some(match k {
            Key::ArrowLeft => NudgeKey::Left,
            Key::ArrowRight => NudgeKey::Right,
            Key::ArrowUp => NudgeKey::Up,
            Key::ArrowDown => NudgeKey::Down,
            Key::PageUp => NudgeKey::PageUp,
            Key::PageDown => NudgeKey::PageDown,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NudgeMod {
    Plain,
    Shift,
    Alt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NudgeView {
    View2d(ViewKind),
    View3d { right: DVec3, forward: DVec3, up: DVec3 },
}

pub fn nudge_delta(key: NudgeKey, view: NudgeView, step: f64) -> DVec3 {
    let (right, up, depth) = match view {
        NudgeView::View2d(kind) => match kind.axes_2d() {
            Some((r, u, _)) => (axis_vec(r), axis_vec(u), kind.toward_viewer()),
            None => (DVec3::X, DVec3::Y, DVec3::Z),
        },
        NudgeView::View3d { right, forward, up } => {
            let fwd = DVec3::new(forward.x, forward.y, 0.0);
            let ahead = if fwd.length() > 0.2 { fwd } else { DVec3::new(up.x, up.y, 0.0) };
            let r = dominant_horizontal_axis(right);
            let mut a = dominant_horizontal_axis(ahead);
            if a.dot(r).abs() > 0.5 {
                a = DVec3::new(-r.y, r.x, 0.0);
            }
            (r, a, DVec3::Z)
        }
    };
    let dir = match key {
        NudgeKey::Left => -right,
        NudgeKey::Right => right,
        NudgeKey::Up => up,
        NudgeKey::Down => -up,
        NudgeKey::PageUp => depth,
        NudgeKey::PageDown => -depth,
    };
    dir * step
}

pub fn take_nudge_keys(events: &mut Vec<egui::Event>) -> Vec<(NudgeKey, NudgeMod)> {
    let mut out = Vec::new();
    events.retain(|e| {
        if let egui::Event::Key { key, pressed: true, modifiers, .. } = e
            && !modifiers.command
            && !modifiers.ctrl
            && !modifiers.mac_cmd
            && !(modifiers.alt && modifiers.shift)
            && let Some(k) = NudgeKey::from_key(*key)
        {
            let m = if modifiers.shift {
                NudgeMod::Shift
            } else if modifiers.alt {
                NudgeMod::Alt
            } else {
                NudgeMod::Plain
            };
            out.push((k, m));
            return false;
        }
        true
    });
    out
}

fn take_plain_key(ctx: &egui::Context, key: Key, shift: bool) -> bool {
    ctx.input_mut(|i| {
        let before = i.events.len();
        i.events.retain(|e| {
            !matches!(e, egui::Event::Key { key: k, pressed: true, modifiers: m, .. }
                if *k == key && m.shift == shift && !m.alt && !m.ctrl && !m.command && !m.mac_cmd)
        });
        i.events.len() != before
    })
}

const MODAL_KEYS: [(Key, ModalKind); 3] =
    [(Key::G, ModalKind::Move), (Key::R, ModalKind::Rotate), (Key::E, ModalKind::Scale)];

impl SelectTool {
    pub(super) fn keys(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        self.sync_pivot(state);
        let mut handled = false;
        if self.drag.as_ref().is_some_and(|d| !d.is_modal()) && !ctx.input(|i| i.pointer.primary_down()) {
            self.finish_drag(state);
            handled = true;
        }
        if self.drag.as_ref().is_some_and(Drag::is_modal) {
            return self.modal_keys(state, ctx) || handled;
        }
        let wants_escape = self.drag.is_some() || !state.doc.selection().is_empty();
        if wants_escape && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            handled |= hp_app::Tool::cancel(self, state);
        }
        if self.drag.is_none() {
            handled |= self.start_modal_keys(state, ctx);
        }
        if self.drag.is_none() && self.live_cycle(state).is_some() {
            for (key, dir) in [(Key::PageUp, -1), (Key::PageDown, 1)] {
                while take_plain_key(ctx, key, false) {
                    handled |= self.step_cycle(state, dir, false);
                }
            }
        }
        if self.drag.is_none()
            && state.settings.edit.arrow_key_nudge
            && !state.doc.selection().objects.is_empty()
            && let Some(view) = self.nudge_view(state)
        {
            let keys = ctx.input_mut(|i| take_nudge_keys(&mut i.events));
            if !keys.is_empty() {
                self.nudge(state, view, &keys);
                handled = true;
            }
        }
        handled
    }

    fn start_modal_keys(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if state.doc.selection().objects.is_empty() {
            return false;
        }
        let Some(index) = state.ui.hovered_view else { return false };
        let Some(view) = self.views.get(index).copied().flatten() else { return false };
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()).filter(|p| view.rect.contains(*p)) else {
            return false;
        };
        for (key, kind) in MODAL_KEYS {
            if key == Key::E && !state.doc.selection().faces.is_empty() {
                continue;
            }
            if take_plain_key(ctx, key, false) {
                self.start_modal(state, &view, kind, pos);
                return true;
            }
        }
        false
    }

    fn modal_keys(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.drag = None;
            state.runtime.selection.modal_transform = false;
            return true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            self.finish_drag(state);
            return true;
        }
        let mut changed = false;
        for (key, k) in [(Key::X, 0), (Key::Y, 1), (Key::Z, 2)] {
            for plane in [false, true] {
                while take_plain_key(ctx, key, plane) {
                    if let Some(Drag { op: DragOp::Modal(m), .. }) = self.drag.as_mut() {
                        m.lock = m.lock.toggled(k, plane);
                        changed = true;
                    }
                }
            }
        }
        for (key, kind) in MODAL_KEYS {
            while take_plain_key(ctx, key, false) {
                if let Some(Drag { op: DragOp::Modal(m), last_pos, .. }) = self.drag.as_mut()
                    && m.kind != kind
                {
                    *m = crate::modal::Modal { lock: m.lock, ..crate::modal::Modal::new(kind, *last_pos, m.axes) };
                }
                changed = true;
            }
        }
        if changed
            && let Some(d) = &self.drag
            && let Some(view) = self.views.get(d.view).copied().flatten()
        {
            let (pos, mods) = (d.last_pos, d.mods);
            self.update_drag(state, &view, pos, mods);
        }
        changed
    }

    fn nudge_view(&self, state: &AppState) -> Option<NudgeView> {
        let index = state.ui.hovered_view?;
        let kind = self.kind_of(state, index)?;
        Some(if kind.is_2d() {
            NudgeView::View2d(kind)
        } else {
            let cam: &Camera3d = &state.cameras.cam3d;
            NudgeView::View3d { right: cam.right(), forward: cam.forward(), up: cam.up() }
        })
    }

    #[cfg(test)]
    pub(crate) fn nudge_for_tests(
        &mut self,
        state: &mut AppState,
        view: NudgeView,
        keys: &[(super::NudgeKey, NudgeMod)],
    ) {
        self.nudge(state, view, keys);
    }

    fn nudge(&mut self, state: &mut AppState, view: NudgeView, keys: &[(super::NudgeKey, NudgeMod)]) {
        let kind = match view {
            NudgeView::View2d(kind) => kind,
            NudgeView::View3d { .. } => hp_app::ViewKind::Perspective,
        };
        let frame = state.edit_grid_frame(kind);
        let steps = DVec3::new(frame.step_along(0), frame.step_along(1), frame.step_along(2));
        let grid_step = |k: super::NudgeKey| nudge_delta(k, view, 1.0) * steps;
        let clones = state.settings.selection.shift_arrow_clones;
        let (mut delta, mut clone_delta) = (DVec3::ZERO, DVec3::ZERO);
        for (k, m) in keys {
            match m {
                NudgeMod::Plain => delta += grid_step(*k),
                NudgeMod::Alt => delta += nudge_delta(*k, view, 1.0),
                NudgeMod::Shift if clones => clone_delta += grid_step(*k),
                NudgeMod::Shift => delta += nudge_delta(*k, view, 1.0),
            }
        }
        let pivot = self.active_pivot(state);
        let mut moved = DVec3::ZERO;
        if clone_delta.length_squared() > 1e-12 {
            let objs = state.doc.selected_objects();
            let r = hp_app::runtime::helpers::duplicate_linking_paths(
                state,
                &objs,
                DMat4::from_translation(clone_delta),
                DVec3::ZERO,
            );
            if r.is_ok() {
                moved += clone_delta;
            }
            Self::report(state, r.map(|_| ()));
        }
        if delta.length_squared() > 1e-12 && !super::element::picked_points(state).is_empty() {
            let r = super::element::translate_picked(state, delta).map(|_| ());
            if r.is_ok() {
                moved += delta;
            }
            Self::report(state, r);
        } else if delta.length_squared() > 1e-12 {
            let objs = state.doc.selected_objects();
            let r = state.doc.nudge(&objs, delta);
            if r.is_ok() {
                moved += delta;
            }
            Self::report(state, r);
        }
        if let Some(p) = pivot
            && moved != DVec3::ZERO
        {
            state.pivot = Some(p + moved);
            self.pivot_sig = Some(self.selection_sig(state));
        }
    }
}
