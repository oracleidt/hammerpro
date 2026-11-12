use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{BrushSettings, DetailSplitSetting};
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(BrushPage)]
}

struct BrushPage;

impl SettingsPage for BrushPage {
    fn id(&self) -> &'static str {
        "brush"
    }

    fn label(&self) -> String {
        t!("shell.brush.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "⬛"
    }

    fn order(&self) -> i32 {
        page_order::EDITING + 20
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let s = &mut state.settings.brush;
        heading(ui, t!("shell.brush.primitives"));
        changed |= ui
            .checkbox(&mut s.reorient_primitives, t!("shell.brush.reorient"))
            .on_hover_text(t!("shell.brush.reorient_tip"))
            .changed();
        heading(ui, t!("shell.brush.clipping"));
        changed |= ui
            .checkbox(&mut s.clip_measurements, t!("shell.brush.clip_measurements"))
            .on_hover_text(t!("shell.brush.clip_measurements_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.clip_current_material, t!("shell.brush.clip_current_material"))
            .on_hover_text(t!("shell.brush.clip_current_material_tip"))
            .changed();
        heading(ui, t!("shell.brush.func_detail"));
        ui.horizontal(|ui| {
            ui.label(t!("shell.brush.detail_split"));
            for (mode, key) in [
                (DetailSplitSetting::Selection, "shell.brush.detail_split_selection"),
                (DetailSplitSetting::Cluster, "shell.brush.detail_split_cluster"),
            ] {
                if ui
                    .selectable_label(s.detail.split == mode, t!(key))
                    .on_hover_text(t!("shell.brush.detail_split_tip"))
                    .clicked()
                    && s.detail.split != mode
                {
                    s.detail.split = mode;
                    changed = true;
                }
            }
        });
        changed |= ui
            .checkbox(&mut s.detail.check_sealing, t!("shell.brush.detail_check_sealing"))
            .on_hover_text(t!("shell.brush.detail_check_sealing_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.detail.tint, t!("shell.brush.detail_tint"))
            .on_hover_text(t!("shell.brush.detail_tint_tip"))
            .changed();
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.brush = BrushSettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.brush.page", "Brushes", "Brushes"),
    ("shell.brush.primitives", "Primitives", "Primitives"),
    ("shell.brush.reorient", "Orient primitives to the drawing view", "Orienter les primitives selon la vue de tracé"),
    (
        "shell.brush.reorient_tip",
        "A cylinder, arch or spike drawn in the Front or Side view (or on a wall in 3D) stands along the view depth axis, like Hammer's \"Reorient primitives on creation in the active 2D view\"",
        "Un cylindre, une arche ou une pointe tracés dans la vue de face ou de côté (ou sur un mur en 3D) s'alignent sur l'axe de profondeur de la vue, comme l'option de Hammer « Réorienter les primitives à la création dans la vue 2D active »",
    ),
    ("shell.brush.clipping", "Clipping tool", "Outil de découpe"),
    (
        "shell.brush.clip_measurements",
        "Show the length and angle of the clip line",
        "Afficher la longueur et l'angle de la ligne de découpe",
    ),
    (
        "shell.brush.clip_measurements_tip",
        "O toggles them while the clipping tool is active",
        "O les affiche ou les masque quand l'outil de découpe est actif",
    ),
    (
        "shell.brush.clip_current_material",
        "Current material on the cut faces",
        "Matériau actuel sur les faces de coupe",
    ),
    (
        "shell.brush.clip_current_material_tip",
        "Otherwise the new faces copy the most similar face of the brush",
        "Sinon, les nouvelles faces copient la face la plus proche du brush",
    ),
    ("shell.brush.func_detail", "func_detail", "func_detail"),
    ("shell.brush.detail_split", "Make func_detail creates", "Transformer en func_detail crée"),
    ("shell.brush.detail_split_selection", "One per selection", "Un par sélection"),
    ("shell.brush.detail_split_cluster", "One per cluster", "Un par groupe"),
    (
        "shell.brush.detail_split_tip",
        "One func_detail for the whole selection, or one for each group of touching brushes (brushes of different groups always get their own func_detail)",
        "Un func_detail pour toute la sélection, ou un par ensemble de brushes qui se touchent (les brushes de groupes différents ont toujours leur propre func_detail)",
    ),
    (
        "shell.brush.detail_check_sealing",
        "Keep the brushes that seal the map in the world",
        "Garder dans le monde les brushes qui scellent la map",
    ),
    (
        "shell.brush.detail_check_sealing_tip",
        "Make func_detail runs the leak check first and asks before converting brushes of the outer shell, because a func_detail does not seal",
        "Transformer en func_detail lance d'abord la vérification des fuites et demande avant de convertir des brushes de la coque extérieure, car un func_detail ne scelle pas",
    ),
    (
        "shell.brush.detail_tint",
        "Tint func_detail brushes in the views",
        "Teinter les brushes func_detail dans les vues",
    ),
    (
        "shell.brush.detail_tint_tip",
        "func_detail brushes get a blue tint in 3D and a blue outline in 2D, to see at a glance what is detail (View > Tint func_detail brushes)",
        "Les brushes func_detail prennent une teinte bleue en 3D et un contour bleu en 2D, pour voir d'un coup d'oeil ce qui est du détail (Affichage > Teinter les brushes func_detail)",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_edits_and_resets_the_brush_settings() {
        let mut state = hp_app::testing::test_state();
        let mut page = BrushPage;
        assert_eq!(page.id(), "brush");
        assert!(!page.label().is_empty());
        hp_app::testing::run_ui(&mut state, |ui, state| page.ui(ui, state));
        state.settings.brush.clip_measurements = false;
        page.reset(&mut state);
        assert_eq!(state.settings.brush, BrushSettings::default());
        assert!(state.settings_dirty);
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
    }
}
