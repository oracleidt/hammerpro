use egui::{RichText, Ui};
use hp_app::actions::key_label;
use hp_app::runtime::helpers::{array_duplicate, array_frame};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::{ArrayAxis, ArrayPlan, ArraySpec, EditError, MAX_ARRAY_AXIS_COUNT, ObjRef};

use crate::util::{enter_pressed, escape_pressed, icon_title};

const AXIS_NAMES: [&str; 3] = ["X", "Y", "Z"];

type PlanCache = (u64, Vec<ObjRef>, [bool; 3], [u32; 3], [u64; 3], Result<ArrayPlan, String>);

pub(crate) struct ArrayDialog {
    pub(crate) enabled: [bool; 3],
    pub(crate) counts: [u32; 3],
    pub(crate) gaps: [f64; 3],
    pub(crate) error: Option<String>,
    preview: Option<PlanCache>,
}

impl Default for ArrayDialog {
    fn default() -> Self {
        Self { enabled: [true, false, false], counts: [3, 3, 3], gaps: [0.0; 3], error: None, preview: None }
    }
}

impl ArrayDialog {
    pub(crate) fn spec(&self, state: &AppState) -> ArraySpec {
        let axis = |k: usize| ArrayAxis {
            count: if self.enabled[k] { self.counts[k].clamp(1, MAX_ARRAY_AXIS_COUNT) } else { 0 },
            gap: self.gaps[k],
        };
        ArraySpec { axes: [axis(0), axis(1), axis(2)], frame: array_frame(state) }
    }

    pub(crate) fn apply(&mut self, state: &mut AppState) -> Result<usize, String> {
        let objs = state.doc.selected_objects();
        if objs.is_empty() {
            return Err(t!("array.err_nothing").to_owned());
        }
        let spec = self.spec(state);
        match array_duplicate(state, &objs, &spec) {
            Ok((_, copies)) => {
                self.error = None;
                Ok(copies.len())
            }
            Err(EditError::Invalid(why)) => Err(tf!("array.err_failed", why)),
            Err(e) => Err(tf!("array.err_failed", e)),
        }
    }

    fn plan(&mut self, state: &AppState) -> Result<ArrayPlan, String> {
        let objs = state.doc.selected_objects();
        let spec = self.spec(state);
        let gaps = [0, 1, 2].map(|k| spec.axes[k].gap.to_bits());
        let frame_key = [
            spec.frame.step.to_array(),
            spec.frame.origin.to_array(),
            spec.frame.axes.x_axis.to_array(),
            spec.frame.axes.y_axis.to_array(),
            spec.frame.axes.z_axis.to_array(),
        ]
        .iter()
        .flatten()
        .fold(0u64, |acc, v| acc.rotate_left(7) ^ v.to_bits());
        let key = state.doc.revision() ^ frame_key.rotate_left(17);
        if let Some((rev, o, en, cn, g, result)) = &self.preview
            && *rev == key
            && *o == objs
            && *en == self.enabled
            && *cn == self.counts
            && *g == gaps
        {
            return result.clone();
        }
        let assets = &state.assets;
        let result = state.doc.array_plan(&objs, &spec, &|e| assets.entity_bounds(e)).map_err(|e| match e {
            EditError::Invalid(why) => why,
            other => other.to_string(),
        });
        self.preview = Some((key, objs, self.enabled, self.counts, gaps, result.clone()));
        result
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.ui.show_array_duplicate {
            self.error = None;
            return;
        }
        let mut open = true;
        let mut apply = false;
        let response = hp_app::chrome::Window::new(icon_title("⊞", t!("array.title")))
            .id(egui::Id::unique("hp_array_duplicate_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(420.0, 300.0)))
            .open(&mut open)
            .default_width(420.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| apply = self.window_ui(ui, state));
        if let Some(r) = &response {
            if escape_pressed(ctx, &r.response) {
                open = false;
            } else if enter_pressed(ctx, &r.response) {
                apply = true;
            }
        }
        if apply {
            match self.apply(state) {
                Ok(n) => {
                    state.log(Severity::Info, tf!("array.done", n));
                    open = false;
                }
                Err(e) => self.error = Some(e),
            }
        }
        if !open {
            state.ui.show_array_duplicate = false;
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) -> bool {
        ui.label(RichText::new(t!("array.intro")).weak());
        ui.add_space(4.0);
        egui::Grid::new("hp_array_grid").num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new(t!("array.copies")).strong());
            ui.label(RichText::new(t!("array.gap")).strong());
            ui.label(RichText::new(t!("array.step")).strong());
            ui.end_row();
            let plan = self.plan(state);
            for (k, name) in AXIS_NAMES.iter().enumerate() {
                ui.checkbox(&mut self.enabled[k], *name).on_hover_text(t!("array.axis_tip"));
                ui.add_enabled(
                    self.enabled[k],
                    egui::DragValue::new(&mut self.counts[k]).range(1..=MAX_ARRAY_AXIS_COUNT).speed(0.1),
                );
                ui.add_enabled(self.enabled[k], egui::DragValue::new(&mut self.gaps[k]).speed(1.0).max_decimals(3));
                let step = match &plan {
                    Ok(p) if self.enabled[k] => format!("{}", (p.steps[k] * 1000.0).round() / 1000.0),
                    _ => String::new(),
                };
                ui.label(RichText::new(step).weak());
                ui.end_row();
            }
        });
        ui.add_space(2.0);
        let copies: u64 =
            (0..3).map(|k| if self.enabled[k] { u64::from(self.counts[k]) + 1 } else { 1 }).product::<u64>() - 1;
        let any = self.enabled.iter().any(|e| *e);
        if any {
            ui.label(RichText::new(tf!("array.total", copies)).weak());
        }
        let snap_on = state.snap_enabled();
        let note = if snap_on { t!("array.grid_on") } else { t!("array.grid_off") };
        ui.label(RichText::new(note).weak());
        if copies > hp_editor::MAX_ARRAY_COPIES {
            ui.label(RichText::new(tf!("array.too_many", hp_editor::MAX_ARRAY_COPIES)).color(theme::warning()));
        } else if let Err(why) = self.plan(state)
            && any
            && !state.doc.selected_objects().is_empty()
        {
            ui.label(RichText::new(why).color(theme::warning()));
        }
        if state.doc.selected_objects().is_empty() {
            ui.label(RichText::new(t!("array.err_nothing")).color(theme::warning()));
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(theme::error()));
        }
        ui.separator();
        let mut apply = false;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("common.cancel")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                    state.ui.show_array_duplicate = false;
                }
                let ready = any && !state.doc.selected_objects().is_empty();
                let button = ui.add_enabled(ready, egui::Button::new(t!("array.apply")));
                if button.on_hover_text(key_label(egui::Key::Enter)).clicked() {
                    apply = true;
                }
            });
        });
        apply
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_of_the_selection_in_one_undo_step() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.doc.select(&[ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
        let before = state.doc.scene().solids().len();
        let mut d = ArrayDialog::default();
        d.counts[0] = 2;
        d.gaps[0] = 10.0;
        let n = d.apply(&mut state).expect("array");
        assert_eq!(n, 2);
        assert_eq!(state.doc.scene().solids().len(), before + 2);
        let plan = d.plan(&state).expect("plan");
        assert!(plan.steps.x >= 522.0, "{plan:?}");
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().solids().len(), before);
        d.enabled = [false; 3];
        assert!(d.apply(&mut state).is_err());
        state.doc.clear_selection();
        d.enabled[1] = true;
        assert!(d.apply(&mut state).is_err());
    }

    #[test]
    fn dialog_draws() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.doc.select(&[ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
        state.ui.show_array_duplicate = true;
        let mut d = ArrayDialog::default();
        hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        assert!(state.ui.show_array_duplicate);
    }
}
