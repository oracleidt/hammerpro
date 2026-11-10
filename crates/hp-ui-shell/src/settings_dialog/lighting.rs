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
        changed |= ui
            .checkbox(&mut s.visible_first, t!("shell.lighting.visible_first"))
            .on_hover_text(t!("shell.lighting.visible_first_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.prepare, t!("shell.lighting.prepare"))
            .on_hover_text(t!("shell.lighting.prepare_tip"))
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
        "Starts a bake when the baked lighting preview is turned on and nothing is baked yet. Uses the whole map by default; the optional regional mode applies to this automatic start.",
        "Lance un précalcul quand l'aperçu précalculé est activé et que rien n'est encore calculé. Toute la carte est utilisée par défaut ; le mode régional optionnel concerne ce démarrage automatique.",
    ),
    (
        "shell.lighting.auto_scope",
        "Bake only around the camera on big maps",
        "Précalculer autour de la caméra sur les grandes cartes",
    ),
    (
        "shell.lighting.auto_scope_tip",
        "Optional regional mode for automatic preview activation on maps above the luxel threshold. The region follows the camera after it leaves the central half and stays still for a second. Off by default. Bake lighting always calculates the whole map and never follows the camera.",
        "Mode régional optionnel à l'activation automatique de l'aperçu sur les cartes au-dessus du seuil de luxels. La zone suit la caméra quand elle quitte la moitié centrale et reste immobile une seconde. Désactivé par défaut. Précalculer l'éclairage calcule toujours toute la carte sans suivre la caméra.",
    ),
    (
        "shell.lighting.auto_scope_luxels",
        "Big map threshold (millions of luxels)",
        "Seuil de grande carte (millions de luxels)",
    ),
    (
        "shell.lighting.auto_scope_luxels_tip",
        "Threshold for the optional regional bake on automatic preview activation. Bake lighting remains a whole-map calculation.",
        "Seuil du précalcul régional optionnel à l'activation automatique de l'aperçu. Précalculer l'éclairage conserve un calcul de toute la carte.",
    ),
    ("shell.lighting.auto_scope_radius", "Big map radius", "Rayon des grandes cartes"),
    ("shell.lighting.visible_first", "Bake the visible area first", "Précalculer d'abord la zone visible"),
    (
        "shell.lighting.prepare",
        "Prepare the baked preview in the background",
        "Préparer l'aperçu précalculé en arrière-plan",
    ),
    (
        "shell.lighting.prepare_tip",
        "Once the map is open and edits pause, the lighting is compiled and lit at low priority so the baked preview shows at once when you turn it on. Turn it off on a slow computer to keep its processor and memory free.",
        "Une fois la carte ouverte et les modifications en pause, l'éclairage est compilé et calculé en priorité basse pour que l'aperçu précalculé s'affiche aussitôt quand vous l'activez. Désactivez-le sur un ordinateur lent pour garder son processeur et sa mémoire libres.",
    ),
    (
        "shell.lighting.visible_first_tip",
        "On big maps the area around the camera is baked first so lighting shows in seconds, then the whole map is baked in the background and replaces it.",
        "Sur les grandes cartes, la zone autour de la caméra est précalculée d'abord pour que l'éclairage apparaisse en quelques secondes, puis toute la carte est précalculée en arrière-plan et la remplace.",
    ),
    (
        "shell.lighting.auto_scope_radius_tip",
        "Radius of the optional automatic regional preview. Bake around the camera uses its separate radius in the Lighting tab. Used by the next automatic regional bake.",
        "Rayon de l'aperçu régional automatique optionnel. Précalculer autour de la caméra utilise son rayon distinct dans l'onglet Éclairage. Utilisé par le prochain précalcul régional automatique.",
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
