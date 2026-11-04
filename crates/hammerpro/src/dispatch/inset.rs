use hp_app::{Action, Severity, t, tf};
use hp_editor::EditError;
use hp_geom::{InsetIssue, InsetParams, MAX_INSET, MIN_INSET};

use super::edit_error_text;
use crate::app::HammerProApp;

pub(crate) struct InsetPrompt {
    pub open: bool,
    pub distance: f64,
    pub depth: f64,
    pub snap: bool,
}

impl Default for InsetPrompt {
    fn default() -> Self {
        Self { open: false, distance: 16.0, depth: 0.0, snap: false }
    }
}

pub(crate) fn snapped(grid: f64, distance: f64, depth: f64) -> (f64, f64) {
    if grid <= 0.0 {
        return (distance, depth);
    }
    (((distance / grid).round() * grid).max(grid), (depth / grid).round() * grid)
}

fn issue_key(issue: InsetIssue) -> &'static str {
    match issue {
        InsetIssue::NotAFace => "inset.err_face",
        InsetIssue::Displacement => "inset.err_disp",
        InsetIssue::BadDistance => "inset.err_distance",
        InsetIssue::TooLarge => "inset.err_large",
        InsetIssue::TooDeep => "inset.err_deep",
        InsetIssue::PastLimits => "inset.err_limits",
    }
}

fn error_text(e: &EditError) -> String {
    match e {
        EditError::Geom(g) => match InsetIssue::of(g) {
            Some(issue) => hp_app::i18n::tr(issue_key(issue)).to_owned(),
            None => g.to_string(),
        },
        other => edit_error_text(other),
    }
}

impl HammerProApp {
    pub(crate) fn start_inset(&mut self) {
        if self.state.doc.selected_faces().is_empty() {
            self.state.notify(Severity::Info, t!("inset.need_face"));
            return;
        }
        let p = &mut self.pkg.brush.inset;
        if !p.open {
            p.snap = self.state.snap_enabled();
        }
        p.open = true;
    }

    pub(crate) fn apply_inset(&mut self, distance: f64, depth: f64, snap: bool) {
        let faces = self.state.doc.selected_faces();
        if faces.is_empty() {
            self.state.notify(Severity::Info, t!("inset.need_face"));
            return;
        }
        let (distance, depth) = if snap { snapped(self.state.grid_size(), distance, depth) } else { (distance, depth) };
        let params = InsetParams { distance, depth, material: None };
        match self.state.doc.inset_faces(&faces, &params) {
            Ok(ids) => self.state.log(Severity::Info, tf!("inset.done", ids.len())),
            Err(e) => {
                self.state.notify(Severity::Warning, tf!("app.op_failed", Action::InsetFace.label(), error_text(&e)));
            }
        }
    }

    pub(crate) fn inset_dialog(&mut self, ctx: &egui::Context) {
        if !self.pkg.brush.inset.open {
            return;
        }
        let count = self.state.doc.selected_faces().len();
        let p = &mut self.pkg.brush.inset;
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        hp_app::chrome::Window::new(t!("action.inset_face"))
            .id(egui::Id::unique("hp_brush_inset"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.label(tf!("inset.faces", count));
                ui.horizontal(|ui| {
                    ui.label(t!("inset.distance")).on_hover_text(t!("inset.distance_tip"));
                    ui.add(
                        egui::DragValue::new(&mut p.distance).speed(1.0).range(MIN_INSET..=MAX_INSET).max_decimals(3),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(t!("inset.depth")).on_hover_text(t!("inset.depth_tip"));
                    ui.add(egui::DragValue::new(&mut p.depth).speed(1.0).range(-MAX_INSET..=MAX_INSET).max_decimals(3));
                });
                ui.checkbox(&mut p.snap, t!("inset.snap"));
                ui.label(egui::RichText::new(t!("inset.hint")).weak());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    apply = ui.button(t!("common.ok")).clicked();
                    cancel = ui.button(t!("common.cancel")).clicked();
                });
            });
        if apply {
            let (d, h, s) = (p.distance, p.depth, p.snap);
            p.open = false;
            self.apply_inset(d, h, s);
        } else if cancel || !open {
            p.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_rounds_to_the_grid_and_keeps_a_positive_distance() {
        assert_eq!(snapped(8.0, 13.0, -3.0), (16.0, 0.0));
        assert_eq!(snapped(8.0, 1.0, 12.0), (8.0, 16.0));
        assert_eq!(snapped(0.0, 5.0, 2.0), (5.0, 2.0));
    }
}
