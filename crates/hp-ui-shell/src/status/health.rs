use egui::{RichText, Ui};
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::{Action, AppState, Severity, t, tf};
use hp_editor::limits::LIMIT_WARNING;

use crate::common::severity_color;

pub(super) fn segments() -> Vec<Box<dyn StatusSegment>> {
    vec![Box::new(ProblemBadge), Box::new(LimitGauge)]
}

struct ProblemBadge;

impl StatusSegment for ProblemBadge {
    fn id(&self) -> &'static str {
        "health_problems"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT + 40
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        let rt = &state.runtime.health;
        if rt.checks.is_none() && !rt.checking {
            return;
        }
        ui.separator();
        if rt.leak_path().is_some() {
            let color = severity_color(ui.visuals(), Severity::Error);
            let r = ui
                .add(
                    egui::Button::new(RichText::new(format!("💧 {}", t!("shell.health.leak"))).small().color(color))
                        .frame(false),
                )
                .on_hover_text(t!("shell.health.leak_tip"));
            if r.clicked() {
                state.request(Action::CheckLeaks);
            }
        }
        let rt = &state.runtime.health;
        let c = rt.counts;
        let mut text = String::new();
        let mut color = severity_color(ui.visuals(), Severity::Success);
        if c.errors > 0 {
            text.push_str(&format!("❌ {}", c.errors));
            color = severity_color(ui.visuals(), Severity::Error);
        }
        if c.warnings > 0 {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&format!("⚠ {}", c.warnings));
            if c.errors == 0 {
                color = severity_color(ui.visuals(), Severity::Warning);
            }
        }
        if text.is_empty() {
            text.push('✔');
        }
        if rt.busy() {
            text.push_str(" ...");
        }
        let checking = rt.busy();
        let r = ui.add(egui::Button::new(RichText::new(text).small().color(color)).frame(false));
        let r = crate::common::lazy_hover_text(r, || {
            format!(
                "{}\n{}",
                tf!("shell.health.counts_tip", c.errors, c.warnings, c.infos),
                if checking { t!("shell.health.checking") } else { t!("shell.health.open_problems") }
            )
        });
        if r.clicked() {
            state.request(Action::CheckProblems);
        }
    }
}

struct LimitGauge;

impl StatusSegment for LimitGauge {
    fn id(&self) -> &'static str {
        "health_limits"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT + 45
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        let rt = &state.runtime.health;
        let Some(worst) = rt.worst_limit() else { return };
        let f = worst.fraction();
        let severity = if f > 1.0 {
            Severity::Error
        } else if f >= LIMIT_WARNING {
            Severity::Warning
        } else {
            Severity::Success
        };
        ui.separator();
        let edicts = rt.checks.as_ref().map(|c| (c.edicts.used, c.edicts.left()));
        let tip = || {
            let mut tip =
                tf!("shell.health.limit_tip", worst.limit.id(), worst.used, worst.max, format!("{:.0}", f * 100.0));
            if let Some((used, left)) = edicts {
                tip.push('\n');
                tip.push_str(&tf!("shell.health.edicts_tip", used, left));
            }
            tip.push('\n');
            tip.push_str(t!("shell.health.open_info"));
            tip
        };
        let bar = hp_app::widgets::progress_bar(f.clamp(0.0, 1.0) as f32)
            .text(RichText::new(format!("{:.0}%", f * 100.0)).small())
            .fill(hp_app::theme::bar_fill(severity))
            .desired_width(56.0);
        let r = crate::common::lazy_hover_text(ui.add(bar).interact(egui::Sense::click()), tip);
        if r.clicked() {
            state.request(Action::MapInfo);
        }
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.health.leak", "Leak", "Fuite"),
    (
        "shell.health.leak_tip",
        "The instant leak check found a way from an entity to the void. Click to go to it and show the path.",
        "La vérification instantanée a trouvé un chemin d'une entité vers le vide. Cliquez pour y aller et afficher le chemin.",
    ),
    (
        "shell.health.counts_tip",
        "Map health: {0} errors, {1} warnings, {2} notes",
        "Santé de la map : {0} erreurs, {1} avertissements, {2} remarques",
    ),
    ("shell.health.checking", "Checking the map...", "Vérification de la map..."),
    ("shell.health.open_problems", "Click to open Check for problems", "Cliquez pour ouvrir Vérifier les problèmes"),
    (
        "shell.health.limit_tip",
        "Closest engine limit: {0}, {1} of {2} ({3}%)",
        "Limite du moteur la plus proche : {0}, {1} sur {2} ({3} %)",
    ),
    ("shell.health.edicts_tip", "Edicts: {0} used, {1} left", "Edicts : {0} utilisés, {1} restants"),
    ("shell.health.open_info", "Click to open Map information", "Cliquez pour ouvrir Informations sur la map"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_draw_every_state() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut cx = StatusContext::default();
        let mut draw = |state: &mut AppState| {
            hp_app::testing::run_ui(state, |ui, state| {
                ui.horizontal(|ui| {
                    for s in &mut segments() {
                        s.ui(ui, state, &mut cx);
                    }
                });
            });
        };
        draw(&mut state);
        state.runtime.health.checking = true;
        draw(&mut state);
        hp_app::runtime::HealthRuntime::check_now(&mut state);
        draw(&mut state);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        draw(&mut state);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(state.runtime.health.worst_limit().is_some());
    }
}
