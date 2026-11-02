use egui::{RichText, Ui};
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::runtime::BakePhase;
use hp_app::{Action, AppState, t, tf, theme};

pub(super) fn segments() -> Vec<Box<dyn StatusSegment>> {
    vec![Box::new(LightingStatus)]
}

fn decimal(v: f64, digits: usize) -> String {
    let s = format!("{v:.digits$}");
    if hp_app::i18n::lang() == hp_app::i18n::Lang::Fr { s.replace('.', ",") } else { s }
}

fn phase(p: BakePhase) -> String {
    match p {
        BakePhase::Preparing => t!("shell.lighting.preparing").to_owned(),
        BakePhase::Compiling => t!("shell.lighting.compiling").to_owned(),
        BakePhase::Direct => t!("shell.lighting.direct").to_owned(),
        BakePhase::Bounce => t!("shell.lighting.bounce").to_owned(),
        BakePhase::Finishing => t!("shell.lighting.finishing").to_owned(),
    }
}

struct LightingStatus;

impl StatusSegment for LightingStatus {
    fn id(&self) -> &'static str {
        "lighting"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT + 20
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        let rt = &state.runtime.lighting;
        if let Some(p) = rt.progress {
            ui.separator();
            if ui.small_button("⏹").on_hover_text(Action::StopBake.label()).clicked() {
                state.request(Action::StopBake);
            }
            let text = tf!("shell.lighting.progress", phase(p.phase), decimal(f64::from(p.elapsed_s), 0));
            let bar = hp_app::widgets::progress_bar(p.fraction.clamp(0.0, 1.0))
                .text(RichText::new(text).small())
                .desired_width(170.0);
            if ui.add(bar).interact(egui::Sense::click()).on_hover_text(t!("shell.lighting.open")).clicked() {
                state.request(Action::ShowLighting);
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            return;
        }
        if !state.settings.lighting.show_budget {
            return;
        }
        let Some(b) = &rt.budget else { return };
        let fraction = b.fraction();
        let color = if fraction > 1.0 {
            theme::error()
        } else if fraction > 0.75 {
            theme::warning()
        } else {
            ui.visuals().weak_text_color()
        };
        let mb = |v: u64| decimal(v as f64 / (1024.0 * 1024.0), 1);
        let text = tf!("shell.lighting.budget", mb(b.bytes), mb(b.limit));
        let mut tip = tf!("shell.lighting.budget_tip", (fraction * 100.0).round(), b.faces);
        if b.tiny_faces > 0 {
            tip.push('\n');
            tip.push_str(&tf!("shell.lighting.tiny_tip", b.tiny_faces));
        }
        ui.separator();
        let r = ui.add(egui::Button::new(RichText::new(format!("💡 {text}")).small().color(color)).frame(false));
        let r = r.on_hover_text(tip);
        if r.clicked() {
            state.request(Action::ShowLighting);
        }
        r.context_menu(|ui| {
            if ui.button(Action::SelectTinyLightmapFaces.label()).clicked() {
                state.request(Action::SelectTinyLightmapFaces);
                ui.close();
            }
            if ui.button(Action::ShowLighting.label()).clicked() {
                state.request(Action::ShowLighting);
                ui.close();
            }
        });
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.lighting.preparing", "Preparing", "Préparation"),
    ("shell.lighting.compiling", "Compiling", "Compilation"),
    ("shell.lighting.direct", "Direct light", "Lumière directe"),
    ("shell.lighting.bounce", "Bounced light", "Lumière rebondie"),
    ("shell.lighting.finishing", "Finishing", "Finalisation"),
    ("shell.lighting.progress", "Lighting: {0}  {1} s", "Éclairage : {0}  {1} s"),
    ("shell.lighting.open", "Open the Lighting tab", "Ouvrir l'onglet Éclairage"),
    ("shell.lighting.budget", "{0} / {1} MB", "{0} / {1} Mo"),
    (
        "shell.lighting.budget_tip",
        "Lightmaps: {0}% of the 2 GB the game can address, {1} faces (estimate). Click to open the Lighting tab, right click for more.",
        "Lightmaps : {0} % des 2 Go que le jeu peut adresser, {1} faces (estimation). Cliquez pour ouvrir l'onglet Éclairage, clic droit pour plus d'options.",
    ),
    (
        "shell.lighting.tiny_tip",
        "{0} faces have a tiny lightmap scale",
        "{0} faces ont une très petite échelle de lightmap",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::runtime::{BakeKind, BakeProgress, BudgetSummary};

    #[test]
    fn segment_draws_every_state() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut seg = LightingStatus;
        let mut cx = StatusContext::default();
        let mut draw = |state: &mut AppState| {
            hp_app::testing::run_ui(state, |ui, state| {
                ui.horizontal(|ui| seg.ui(ui, state, &mut cx));
            });
        };
        draw(&mut state);
        state.runtime.lighting.budget =
            Some(BudgetSummary { faces: 3, bytes: 20 << 20, limit: 16 << 20, tiny_faces: 2, ..Default::default() });
        draw(&mut state);
        state.settings.lighting.show_budget = false;
        draw(&mut state);
        state.runtime.lighting.progress = Some(BakeProgress {
            kind: BakeKind::Full,
            phase: BakePhase::Bounce,
            fraction: 0.7,
            elapsed_s: 3.0,
            faces: 10,
        });
        draw(&mut state);
        assert!(hp_app::i18n::lint_table(TR).is_empty());
    }
}
