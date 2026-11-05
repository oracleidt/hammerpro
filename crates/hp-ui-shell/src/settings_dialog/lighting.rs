use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::LightingSettings;
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(LightingPage)]
}

struct LightingPage;

impl SettingsPage for LightingPage {
    fn id(&self) -> &'static str {
        "lighting"
    }

    fn label(&self) -> String {
        t!("shell.lighting.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "💡"
    }

    fn order(&self) -> i32 {
        page_order::VIEWPORT + 20
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let s = &mut state.settings.lighting;
        heading(ui, t!("shell.lighting.baking"));
        changed |= ui
            .checkbox(&mut s.auto_bake, t!("shell.lighting.auto_bake"))
            .on_hover_text(t!("shell.lighting.auto_bake_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.auto_scope, t!("shell.lighting.auto_scope"))
            .on_hover_text(t!("shell.lighting.auto_scope_tip"))
            .changed();
        egui::Grid::new("hp_shell_lighting").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.lighting.threads"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.threads).range(0..=LightingSettings::MAX_THREADS))
                .on_hover_text(t!("shell.lighting.threads_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.max_luxels"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.max_luxels_millions).range(LightingSettings::LUXELS_RANGE))
                .on_hover_text(t!("shell.lighting.max_luxels_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.auto_scope_luxels"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.auto_scope_luxels_millions).range(LightingSettings::LUXELS_RANGE))
                .on_hover_text(t!("shell.lighting.auto_scope_luxels_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.auto_scope_radius"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.auto_scope_radius).range(LightingSettings::RADIUS_RANGE).suffix(" u"))
                .on_hover_text(t!("shell.lighting.auto_scope_radius_tip"))
                .changed();
            ui.end_row();
        });
        heading(ui, t!("shell.lighting.budget_heading"));
        changed |= ui.checkbox(&mut s.show_budget, t!("shell.lighting.show_budget")).changed();
        egui::Grid::new("hp_shell_lighting_budget").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.lighting.tiny_scale"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.tiny_scale).range(1..=64))
                .on_hover_text(t!("shell.lighting.tiny_scale_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.styles"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.budget_styles).range(1..=4))
                .on_hover_text(t!("shell.lighting.styles_tip"))
                .changed();
            ui.end_row();
        });
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.lighting = LightingSettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.lighting.page", "Lighting", "Éclairage"),
    ("shell.lighting.baking", "Baked lighting preview", "Aperçu de l'éclairage précalculé"),
    ("shell.lighting.auto_bake", "Bake when the preview is turned on", "Précalculer à l'activation de l'aperçu"),
    (
        "shell.lighting.auto_bake_tip",
        "Starts a bake of the whole map when the baked lighting preview is turned on and nothing is baked yet",
        "Lance le précalcul de toute la carte quand l'aperçu de l'éclairage précalculé est activé et que rien n'est encore précalculé",
    ),
    (
        "shell.lighting.auto_scope",
        "Bake only around the camera on big maps",
        "Précalculer autour de la caméra sur les grandes cartes",
    ),
    (
        "shell.lighting.auto_scope_tip",
        "A map above the luxel threshold is baked within the radius below around the 3D camera instead of whole, which cuts the memory and the time a lot. The baked area follows the camera once it moves out of the central half and stays still for a second. Off: the whole map is always baked.",
        "Une carte au-dessus du seuil de luxels est précalculée dans le rayon ci-dessous autour de la caméra 3D au lieu de l'être en entier, ce qui réduit beaucoup la mémoire et le temps. La zone précalculée suit la caméra quand elle sort de la moitié centrale et reste immobile une seconde. Désactivé : toute la carte est toujours précalculée.",
    ),
    (
        "shell.lighting.auto_scope_luxels",
        "Big map threshold (millions of luxels)",
        "Seuil de grande carte (millions de luxels)",
    ),
    (
        "shell.lighting.auto_scope_luxels_tip",
        "Maps with more luxels than this are baked around the camera only. Used by the next bake.",
        "Les cartes avec plus de luxels que cela sont précalculées seulement autour de la caméra. Utilisé par le prochain précalcul.",
    ),
    ("shell.lighting.auto_scope_radius", "Big map radius", "Rayon des grandes cartes"),
    (
        "shell.lighting.auto_scope_radius_tip",
        "Radius in units around the 3D camera of the area baked on a big map. Used by the next bake.",
        "Rayon en unités autour de la caméra 3D de la zone précalculée sur une grande carte. Utilisé par le prochain précalcul.",
    ),
    ("shell.lighting.threads", "Baking threads", "Threads de précalcul"),
    (
        "shell.lighting.threads_tip",
        "0 uses every core (every core but two to compile the map); the lighting runs at a lower priority so the editor stays fluid. Used by the next bake.",
        "0 utilise tous les cœurs (tous sauf deux pour compiler la carte) ; l'éclairage tourne avec une priorité plus basse pour que l'éditeur reste fluide. Utilisé par le prochain précalcul.",
    ),
    ("shell.lighting.max_luxels", "Full resolution luxels (millions)", "Luxels en pleine résolution (millions)"),
    (
        "shell.lighting.max_luxels_tip",
        "Maps with more luxels are previewed with coarser lightmaps (larger lightmap scales) to save memory and time. Used by the next bake.",
        "Les cartes avec plus de luxels sont prévisualisées avec des lightmaps plus grossières (échelles de lightmap plus grandes) pour économiser mémoire et temps. Utilisé par le prochain précalcul.",
    ),
    ("shell.lighting.budget_heading", "Lightmap budget", "Budget des lightmaps"),
    (
        "shell.lighting.show_budget",
        "Show the lightmap budget in the status bar",
        "Afficher le budget des lightmaps dans la barre d'état",
    ),
    ("shell.lighting.tiny_scale", "Tiny lightmap scale", "Très petite échelle de lightmap"),
    (
        "shell.lighting.tiny_scale_tip",
        "Faces with this lightmap scale or less are drawn red by the lightmap grid and selected by \"Select faces with a tiny lightmap scale\"",
        "Les faces avec cette échelle de lightmap ou moins sont dessinées en rouge par la grille de lightmap et sélectionnées par « Sélectionner les faces à petite échelle de lightmap »",
    ),
    ("shell.lighting.styles", "Light styles", "Styles de lumière"),
    (
        "shell.lighting.styles_tip",
        "Lightmaps per face: 1 without switchable lights, up to 4",
        "Lightmaps par face : 1 sans lumières commutables, jusqu'à 4",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_edits_settings() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut page = LightingPage;
        hp_app::testing::run_ui(&mut state, |ui, state| page.ui(ui, state));
        state.settings.lighting.tiny_scale = 9;
        page.reset(&mut state);
        assert_eq!(state.settings.lighting, LightingSettings::default());
        assert!(state.settings_dirty);
        assert!(hp_app::i18n::lint_table(TR).is_empty());
    }
}
