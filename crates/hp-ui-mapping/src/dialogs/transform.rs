use egui::{RichText, Ui};
use glam::{DMat4, DQuat, DVec3, EulerRot};
use hp_app::actions::key_label;
use hp_app::{Action, AppState, t, tf, theme};

use crate::util::{enter_pressed, escape_pressed, icon_title, plural};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransformMode {
    Move,
    Rotate,
    Scale,
    Random,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pivot {
    SelectionCenter,
    Origin,
    Custom,
}

pub(crate) fn rotation_quat(degrees: DVec3) -> DQuat {
    DQuat::from_euler(EulerRot::ZYX, degrees.z.to_radians(), degrees.y.to_radians(), degrees.x.to_radians())
}

pub(crate) fn transform_matrix(mode: TransformMode, v: DVec3, pivot: DVec3) -> Result<DMat4, &'static str> {
    if !v.is_finite() || !pivot.is_finite() {
        return Err("transform.err_invalid");
    }
    let around = |m: DMat4| DMat4::from_translation(pivot) * m * DMat4::from_translation(-pivot);
    match mode {
        TransformMode::Move => Ok(DMat4::from_translation(v)),
        TransformMode::Rotate => Ok(around(DMat4::from_quat(rotation_quat(v)))),
        TransformMode::Random => Ok(DMat4::IDENTITY),
        TransformMode::Scale => {
            if v.x.abs() < 1e-6 || v.y.abs() < 1e-6 || v.z.abs() < 1e-6 {
                return Err("transform.err_zero_scale");
            }
            Ok(around(DMat4::from_scale(v)))
        }
    }
}

pub(crate) struct TransformDialog {
    pub(crate) mode: TransformMode,
    pub(crate) offset: DVec3,
    pub(crate) angles: DVec3,
    pub(crate) scale: DVec3,
    pub(crate) pivot: Pivot,
    pub(crate) error: Option<String>,
    pub(crate) info: Option<String>,
}

impl Default for TransformDialog {
    fn default() -> Self {
        Self {
            mode: TransformMode::Move,
            offset: DVec3::ZERO,
            angles: DVec3::ZERO,
            scale: DVec3::ONE,
            pivot: Pivot::SelectionCenter,
            error: None,
            info: None,
        }
    }
}

impl TransformDialog {
    fn values(&self) -> DVec3 {
        match self.mode {
            TransformMode::Move => self.offset,
            TransformMode::Rotate | TransformMode::Random => self.angles,
            TransformMode::Scale => self.scale,
        }
    }

    fn apply_random(&mut self, state: &mut AppState, objs: &[hp_editor::ObjRef]) -> bool {
        let seed = hp_app::settings::SelectionSettings::random_seed();
        let opts = state.settings.selection.random_rotate(seed);
        match state.doc.random_rotate(objs, &opts) {
            Ok(r) => {
                self.error = None;
                let text = if r.rotated == 0 {
                    tf!("transform.random_none", r.skipped)
                } else {
                    tf!("transform.random_done", r.rotated, r.skipped)
                };
                self.info = Some(text);
                r.rotated > 0
            }
            Err(e) => {
                self.error = Some(tf!("transform.failed", e));
                false
            }
        }
    }

    pub(crate) fn apply(&mut self, state: &mut AppState) -> bool {
        let objs = state.doc.selected_objects();
        self.info = None;
        if objs.is_empty() {
            self.error = Some(t!("transform.no_selection").to_owned());
            return false;
        }
        if self.mode == TransformMode::Random {
            return self.apply_random(state, &objs);
        }
        let center = |state: &AppState| {
            let assets = &state.assets;
            state.doc.selection_center(&|e| assets.entity_bounds(e)).unwrap_or(DVec3::ZERO)
        };
        let pivot = match self.pivot {
            Pivot::Origin => DVec3::ZERO,
            Pivot::Custom => state.pivot.filter(|p| p.is_finite()).unwrap_or_else(|| center(state)),
            Pivot::SelectionCenter => center(state),
        };
        let v = self.values();
        let m = match transform_matrix(self.mode, v, pivot) {
            Ok(m) => m,
            Err(key) => {
                self.error = Some(t!(key).to_owned());
                return false;
            }
        };
        let result = match self.mode {
            TransformMode::Move => state.doc.translate(&objs, v),
            TransformMode::Rotate => state.doc.rotate(&objs, pivot, rotation_quat(v)),
            TransformMode::Scale | TransformMode::Random => state.doc.transform(&objs, m),
        };
        match result {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(tf!("transform.failed", e));
                false
            }
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.ui.show_transform {
            self.error = None;
            self.info = None;
            state.ui.transform_random = false;
            return;
        }
        if std::mem::take(&mut state.ui.transform_random) {
            self.mode = TransformMode::Random;
            self.error = None;
            self.info = None;
        }
        let mut open = true;
        let response = hp_app::chrome::Window::new(icon_title("⟲", t!("transform.title")))
            .id(egui::Id::unique("hp_transform_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(360.0, 220.0)))
            .open(&mut open)
            .default_width(360.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| self.window_ui(ui, state));
        if let Some(r) = &response {
            if escape_pressed(ctx, &r.response) {
                open = false;
            } else if enter_pressed(ctx, &r.response) {
                self.apply(state);
            }
        }
        if !open {
            state.ui.show_transform = false;
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            for (m, key) in [
                (TransformMode::Move, "transform.move"),
                (TransformMode::Rotate, "transform.rotate"),
                (TransformMode::Scale, "transform.scale"),
                (TransformMode::Random, "transform.random"),
            ] {
                let button = ui.add(egui::Button::selectable(self.mode == m, t!(key)));
                let button =
                    if m == TransformMode::Random { button.on_hover_text(t!("transform.random_tip")) } else { button };
                if button.clicked() {
                    self.mode = m;
                    self.error = None;
                    self.info = None;
                }
            }
        });
        ui.add_space(4.0);
        if self.mode == TransformMode::Random {
            self.random_ui(ui, state);
            return;
        }
        let (label, speed) = match self.mode {
            TransformMode::Move => (t!("transform.offset"), 1.0),
            TransformMode::Rotate | TransformMode::Random => (t!("transform.angles"), 1.0),
            TransformMode::Scale => (t!("transform.factors"), 0.01),
        };
        egui::Grid::new("hp_transform_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(label);
            let v = match self.mode {
                TransformMode::Move => &mut self.offset,
                TransformMode::Rotate | TransformMode::Random => &mut self.angles,
                TransformMode::Scale => &mut self.scale,
            };
            hp_app::widgets::vec3_edit(ui, v, speed);
            ui.end_row();
            if self.mode != TransformMode::Move {
                ui.label(t!("transform.pivot"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.pivot, Pivot::SelectionCenter, t!("transform.pivot_selection"));
                    ui.radio_value(&mut self.pivot, Pivot::Origin, t!("transform.pivot_origin"));
                    let custom = state.pivot.filter(|p| p.is_finite());
                    let tip = match custom {
                        Some(p) => tf!("transform.pivot_custom_tip", hp_app::widgets::fmt_vec3(p)),
                        None => t!("transform.pivot_custom_off").to_owned(),
                    };
                    ui.add_enabled_ui(custom.is_some() || self.pivot == Pivot::Custom, |ui| {
                        ui.radio_value(&mut self.pivot, Pivot::Custom, t!("transform.pivot_custom")).on_hover_text(tip);
                    });
                });
                ui.end_row();
            }
            ui.label(t!("transform.texture_lock"));
            ui.horizontal(|ui| {
                let lock = state.doc.options().texture_lock;
                let text = if lock { t!("common.enabled") } else { t!("common.disabled") };
                ui.label(RichText::new(text).color(if lock {
                    theme::success()
                } else {
                    ui.visuals().weak_text_color()
                }));
                if ui.small_button(t!("transform.toggle")).on_hover_text(t!("action.toggle_texture_lock")).clicked() {
                    state.request(Action::ToggleTextureLock);
                }
            });
            ui.end_row();
        });
        if ui.small_button(t!("common.reset")).clicked() {
            match self.mode {
                TransformMode::Move => self.offset = DVec3::ZERO,
                TransformMode::Rotate | TransformMode::Random => self.angles = DVec3::ZERO,
                TransformMode::Scale => self.scale = DVec3::ONE,
            }
        }
        self.footer(ui, state);
    }

    fn random_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let s = &mut state.settings.selection;
        let mut changed = false;
        egui::Grid::new("hp_transform_random").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(t!("transform.random_range"));
            ui.horizontal(|ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut s.random_yaw_min).range(-360.0..=360.0).speed(1.0).suffix("\u{b0}"))
                    .changed();
                ui.label(t!("transform.random_to"));
                let lo = s.random_yaw_min;
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut s.random_yaw_max)
                            .range(lo..=(lo + 360.0))
                            .speed(1.0)
                            .suffix("\u{b0}"),
                    )
                    .changed();
            });
            ui.end_row();
            ui.label(t!("transform.random_snap"));
            ui.horizontal(|ui| {
                for (step, text) in
                    [(0.0, t!("transform.random_free")), (15.0, "15\u{b0}"), (45.0, "45\u{b0}"), (90.0, "90\u{b0}")]
                {
                    if ui.add(egui::Button::selectable(s.random_yaw_snap == step, text)).clicked() {
                        s.random_yaw_snap = step;
                        changed = true;
                    }
                }
                changed |= ui
                    .add(egui::DragValue::new(&mut s.random_yaw_snap).range(0.0..=180.0).speed(0.5).suffix("\u{b0}"))
                    .on_hover_text(t!("transform.random_snap_tip"))
                    .changed();
            });
            ui.end_row();
            ui.label(t!("transform.random_tilt"));
            ui.horizontal(|ui| {
                changed |= ui.checkbox(&mut s.random_tilt, "").on_hover_text(t!("transform.random_tilt_tip")).changed();
                ui.add_enabled_ui(s.random_tilt, |ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut s.random_tilt_deg)
                                .range(0.0..=hp_editor::MAX_JITTER_DEG)
                                .speed(0.25)
                                .prefix("\u{b1} ")
                                .suffix("\u{b0}"),
                        )
                        .changed();
                });
            });
            ui.end_row();
        });
        if ui.small_button(t!("common.reset")).clicked() {
            let d = hp_app::settings::SelectionSettings::default();
            s.random_yaw_min = d.random_yaw_min;
            s.random_yaw_max = d.random_yaw_max;
            s.random_yaw_snap = d.random_yaw_snap;
            s.random_tilt = d.random_tilt;
            s.random_tilt_deg = d.random_tilt_deg;
            changed = true;
        }
        ui.label(RichText::new(t!("transform.random_note")).small().weak());
        if changed {
            state.settings_dirty = true;
        }
        self.footer(ui, state);
    }

    fn footer(&mut self, ui: &mut Ui, state: &mut AppState) {
        let n = state.doc.selection().objects.len();
        ui.add_space(4.0);
        ui.label(RichText::new(tf!(plural(n, "transform.selection_info_one", "transform.selection_info"), n)).weak());
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(theme::error()));
        }
        if let Some(i) = &self.info {
            ui.label(RichText::new(i).color(theme::success()));
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("common.close")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                    state.ui.show_transform = false;
                }
                if ui
                    .add_enabled(n > 0, egui::Button::new(t!("common.apply")))
                    .on_hover_text(key_label(egui::Key::Enter))
                    .clicked()
                {
                    self.apply(state);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_editor::ObjRef;

    #[test]
    fn matrices() {
        let p = DVec3::new(10.0, 0.0, 0.0);
        let m = transform_matrix(TransformMode::Move, DVec3::new(1.0, 2.0, 3.0), p).expect("move");
        assert_eq!(m.transform_point3(DVec3::ZERO), DVec3::new(1.0, 2.0, 3.0));
        let r = transform_matrix(TransformMode::Rotate, DVec3::new(0.0, 0.0, 90.0), p).expect("rotate");
        assert!((r.transform_point3(DVec3::new(20.0, 0.0, 0.0)) - DVec3::new(10.0, 10.0, 0.0)).length() < 1e-9);
        assert!((r.transform_point3(p) - p).length() < 1e-9, "the pivot stays in place");
        let s = transform_matrix(TransformMode::Scale, DVec3::new(2.0, 1.0, 1.0), p).expect("scale");
        assert!((s.transform_point3(DVec3::new(20.0, 5.0, 0.0)) - DVec3::new(30.0, 5.0, 0.0)).length() < 1e-9);
        assert_eq!(
            transform_matrix(TransformMode::Scale, DVec3::new(0.0, 1.0, 1.0), p),
            Err("transform.err_zero_scale")
        );
        assert_eq!(transform_matrix(TransformMode::Move, DVec3::NAN, p), Err("transform.err_invalid"));
        let q = rotation_quat(DVec3::new(90.0, 0.0, 0.0));
        assert!((q * DVec3::Y - DVec3::Z).length() < 1e-9);
    }

    #[test]
    fn apply_to_selection() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut d = TransformDialog::default();
        assert!(!d.apply(&mut state), "nothing selected");
        assert!(d.error.is_some());

        state.doc.set_selection(&[ObjRef::Entity(5)]);
        d.offset = DVec3::new(16.0, 0.0, 0.0);
        assert!(d.apply(&mut state));
        assert_eq!(state.doc.scene().entity(5).and_then(|n| n.entity.origin()), Some(DVec3::new(16.0, 0.0, 96.0)));
        assert_eq!(state.doc.undo_label(), Some("Move"));

        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let before = state.doc.scene().solid_bounds(3).expect("bounds");
        d.mode = TransformMode::Rotate;
        d.angles = DVec3::new(0.0, 0.0, 90.0);
        assert!(d.apply(&mut state));
        let after = state.doc.scene().solid_bounds(3).expect("bounds");
        assert!((after.size().x - before.size().y).abs() < 1e-6);
        assert!((after.center() - before.center()).length() < 1e-6);

        d.mode = TransformMode::Scale;
        d.pivot = Pivot::Origin;
        d.scale = DVec3::new(1.0, 1.0, 2.0);
        assert!(d.apply(&mut state));
        let scaled = state.doc.scene().solid_bounds(3).expect("bounds");
        assert!((scaled.size().z - after.size().z * 2.0).abs() < 1e-6);
        d.scale = DVec3::new(1.0, 0.0, 1.0);
        assert!(!d.apply(&mut state));

        state.doc.set_selection(&[ObjRef::Entity(5)]);
        state.pivot = Some(DVec3::new(100.0, 0.0, 96.0));
        d.mode = TransformMode::Rotate;
        d.pivot = Pivot::Custom;
        d.angles = DVec3::new(0.0, 0.0, 90.0);
        let before = state.doc.scene().entity(5).and_then(|n| n.entity.origin()).expect("origin");
        assert!(d.apply(&mut state));
        let after = state.doc.scene().entity(5).and_then(|n| n.entity.origin()).expect("origin");
        let p = DVec3::new(100.0, 0.0, 96.0);
        let rotated = p + glam::DQuat::from_rotation_z(90f64.to_radians()) * (before - p);
        assert!((after - rotated).length() < 1e-6, "{after:?}");
        state.pivot = None;
        assert!(d.apply(&mut state));
    }

    #[test]
    fn random_mode_turns_entities_with_angles() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.doc.set_selection(&[ObjRef::Entity(5), ObjRef::Solid(3)]);
        let had = state.doc.scene().entity(5).is_some_and(|n| n.entity.get("angles").is_some());
        let mut d = TransformDialog { mode: TransformMode::Random, ..TransformDialog::default() };
        state.settings.selection.random_yaw_min = 90.0;
        state.settings.selection.random_yaw_max = 90.0;
        let before = state.doc.scene().entity(5).and_then(|n| n.entity.angles());
        let turned = d.apply(&mut state);
        assert_eq!(turned, had);
        assert!(d.info.is_some(), "the dialog says how many turned");
        if had {
            let after = state.doc.scene().entity(5).and_then(|n| n.entity.angles()).expect("angles");
            let before = before.expect("angles");
            assert!(((after.y - before.y).rem_euclid(360.0) - 90.0).abs() < 1e-6, "{before:?} {after:?}");
            assert_eq!(state.doc.undo_label(), Some("Random rotation"));
        }
        state.ui.show_transform = true;
        state.ui.transform_random = true;
        let mut d = TransformDialog::default();
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |root| d.show(&root.ctx().clone(), &mut state));
        assert_eq!(d.mode, TransformMode::Random);
        assert!(!state.ui.transform_random);
    }
}
