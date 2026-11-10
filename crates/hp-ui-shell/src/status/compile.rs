use egui::{RichText, Ui};
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::runtime::CompileBadge;
use hp_app::{Action, AppState, Severity, t, tf};

use crate::common::severity_color;

pub(super) fn segments() -> Vec<Box<dyn StatusSegment>> {
    vec![Box::new(CompileStatus)]
}

fn secs(s: f32) -> String {
    let text = format!("{s:.0}");
    if hp_app::i18n::lang() == hp_app::i18n::Lang::Fr { text.replace('.', ",") } else { text }
}

struct CompileStatus;

impl StatusSegment for CompileStatus {
    fn id(&self) -> &'static str {
        "compile_progress"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT + 10
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        let c = &state.runtime.compile;
        if let Some(p) = c.progress.clone() {
            ui.separator();
            if ui.small_button("⏹").on_hover_text(t!("shell.status.compile_cancel")).clicked() {
                state.request(Action::CancelCompile);
            }
            let text = tf!("shell.status.compile_step", p.step, secs(p.elapsed_s));
            let bar = hp_app::widgets::progress_bar(p.fraction.clamp(0.0, 1.0))
                .text(RichText::new(text).small())
                .desired_width(220.0);
            if ui.add(bar).interact(egui::Sense::click()).on_hover_text(t!("shell.status.compile_open")).clicked() {
                state.ui.show_compile = true;
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        if let Some(p) = c.background.clone() {
            ui.separator();
            let r = ui.add(
                egui::Label::new(RichText::new(format!("⏳ {}", t!("shell.status.compile_background"))).small())
                    .sense(egui::Sense::click()),
            );
            let r = crate::common::lazy_hover_text(r, || tf!("shell.status.compile_background_tip", secs(p.elapsed_s)));
            if r.clicked() {
                state.request(Action::CancelCompile);
            }
            return;
        }
        let Some(badge) = c.badge.clone() else { return };
        if badge == CompileBadge::Leak && state.runtime.health.leak_path().is_some() {
            return;
        }
        ui.separator();
        let (text, tip, severity) = match badge {
            CompileBadge::Leak => {
                (t!("shell.status.compile_leak"), t!("shell.status.compile_leak_tip"), Severity::Error)
            }
            CompileBadge::Failed => {
                (t!("shell.status.compile_failed"), t!("shell.status.compile_open"), Severity::Error)
            }
        };
        let color = severity_color(ui.visuals(), severity);
        let r = ui
            .add(egui::Button::new(RichText::new(format!("⚠ {text}")).small().color(color)).frame(false))
            .on_hover_text(tip);
        if r.clicked() {
            state.ui.show_compile = true;
        }
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.status.compile_step", "{0}  {1} s", "{0}  {1} s"),
    ("shell.status.compile_cancel", "Cancel the compile", "Annuler la compilation"),
    ("shell.status.compile_open", "Open the compile window", "Ouvrir la fenêtre de compilation"),
    ("shell.status.compile_background", "Background compile", "Compilation en arrière-plan"),
    (
        "shell.status.compile_background_tip",
        "The geometry of the current map is built in the background ({0} s): the next compile reuses it. Click to stop it.",
        "La géométrie de la map actuelle est construite en arrière-plan ({0} s) : la prochaine compilation la réutilisera. Cliquez pour l'arrêter.",
    ),
    ("shell.status.compile_leak", "Leak", "Fuite"),
    (
        "shell.status.compile_leak_tip",
        "The map leaks: the leak path is loaded (Map > Pointfile). Click to open the compile window.",
        "La map fuit : le chemin de fuite est chargé (Carte > Pointfile). Cliquez pour ouvrir la fenêtre de compilation.",
    ),
    ("shell.status.compile_failed", "Compile failed", "Échec de la compilation"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::runtime::CompileProgress;

    #[test]
    fn segment_draws_every_state() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut seg = CompileStatus;
        let mut cx = StatusContext::default();
        let mut draw = |state: &mut AppState| {
            hp_app::testing::run_ui(state, |ui, state| {
                ui.horizontal(|ui| seg.ui(ui, state, &mut cx));
            });
        };
        draw(&mut state);
        state.runtime.compile.progress =
            Some(CompileProgress { step: "Lighting".into(), fraction: 0.4, elapsed_s: 12.0 });
        draw(&mut state);
        state.runtime.compile.progress = None;
        state.runtime.compile.background =
            Some(CompileProgress { step: "Geometry".into(), fraction: 0.1, elapsed_s: 2.0 });
        draw(&mut state);
        state.runtime.compile.background = None;
        state.runtime.compile.badge = Some(CompileBadge::Leak);
        draw(&mut state);
        state.runtime.compile.badge = Some(CompileBadge::Failed);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        draw(&mut state);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(secs(12.4), "12");
    }
}
