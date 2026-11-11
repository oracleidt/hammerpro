use egui::Color32;
use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::runtime::helpers::geometry::{LINK_MISSING, LINK_UNKNOWN, LINK_VALID};
use hp_app::settings::{ConnectionsMode, HelperScope, HelpersSettings};
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(HelpersPage)]
}

struct HelpersPage;

fn swatch(ui: &mut egui::Ui, c: [u8; 3], text: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect.shrink(1.0), 2, Color32::from_rgb(c[0], c[1], c[2]));
    ui.label(text);
}

impl SettingsPage for HelpersPage {
    fn id(&self) -> &'static str {
        "helpers"
    }

    fn label(&self) -> String {
        t!("shell.helpers.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "👁"
    }

    fn order(&self) -> i32 {
        page_order::VIEWPORT + 10
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        heading(ui, t!("shell.helpers.helpers"));
        changed |= ui
            .checkbox(&mut state.settings.view.show_helpers, t!("action.toggle_helpers"))
            .on_hover_text(t!("shell.helpers.show_helpers_tip"))
            .changed();
        let s = &mut state.settings.helpers;
        hp_app::widgets::choice_row(ui, |ui| {
            ui.label(t!("shell.helpers.scope"));
            changed |=
                ui.radio_value(&mut s.helper_scope, HelperScope::Selected, t!("shell.helpers.selected")).changed();
            changed |= ui.radio_value(&mut s.helper_scope, HelperScope::All, t!("shell.helpers.every")).changed();
        });
        changed |= ui
            .checkbox(&mut s.show_editor_objects, t!("action.toggle_editor_objects"))
            .on_hover_text(t!("shell.helpers.editor_objects_tip"))
            .changed();
        heading(ui, t!("shell.helpers.connections"));
        hp_app::widgets::choice_row(ui, |ui| {
            changed |= ui.radio_value(&mut s.connections, ConnectionsMode::Off, t!("shell.helpers.hidden")).changed();
            changed |=
                ui.radio_value(&mut s.connections, ConnectionsMode::Selected, t!("shell.helpers.selected")).changed();
            changed |= ui.radio_value(&mut s.connections, ConnectionsMode::All, t!("shell.helpers.every")).changed();
        });
        ui.horizontal_wrapped(|ui| {
            swatch(ui, LINK_VALID, t!("shell.helpers.link_valid"));
            ui.add_space(8.0);
            swatch(ui, LINK_MISSING, t!("shell.helpers.link_missing"));
            ui.add_space(8.0);
            swatch(ui, LINK_UNKNOWN, t!("shell.helpers.link_unknown"));
        });
        heading(ui, t!("shell.helpers.overlays_ropes"));
        changed |= ui.checkbox(&mut s.show_overlays, t!("action.toggle_overlays")).changed();
        changed |= ui.checkbox(&mut s.show_ropes, t!("action.toggle_ropes")).changed();
        ui.add_enabled_ui(s.show_ropes, |ui| {
            changed |= ui
                .checkbox(&mut s.simulate_ropes, t!("action.simulate_ropes"))
                .on_hover_text(t!("shell.helpers.simulate_tip"))
                .changed();
        });
        heading(ui, t!("shell.helpers.names"));
        changed |= ui.checkbox(&mut s.show_brush_centers, t!("action.toggle_brush_centers")).changed();
        changed |= ui
            .checkbox(&mut s.show_entity_names, t!("action.toggle_entity_names"))
            .on_hover_text(t!("shell.helpers.names_tip"))
            .changed();
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.helpers = HelpersSettings::default();
        state.settings.view.show_helpers = true;
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.helpers.page", "Helpers", "Aides"),
    ("shell.helpers.helpers", "Entity helpers", "Aides des entités"),
    (
        "shell.helpers.show_helpers_tip",
        "Radii, light cones, frusta, lines to targets, paths and direction arrows",
        "Rayons, cônes de lumière, frustums, lignes vers les cibles, chemins et flèches de direction",
    ),
    ("shell.helpers.scope", "Draw for:", "Afficher pour :"),
    ("shell.helpers.selected", "Selected entities", "Entités sélectionnées"),
    ("shell.helpers.every", "Every entity", "Toutes les entités"),
    ("shell.helpers.hidden", "Hidden", "Masquées"),
    (
        "shell.helpers.editor_objects_tip",
        "Entity icons and helpers that only exist in the editor. World sprites stay visible.",
        "Icônes d'entités et aides qui n'existent que dans l'éditeur. Les sprites du jeu restent visibles.",
    ),
    ("shell.helpers.connections", "Input/output connections", "Connexions entrées/sorties"),
    ("shell.helpers.link_valid", "Valid", "Valide"),
    ("shell.helpers.link_missing", "Missing target", "Cible introuvable"),
    ("shell.helpers.link_unknown", "Output unknown to the FGD", "Sortie inconnue du FGD"),
    ("shell.helpers.overlays_ropes", "Overlays and ropes", "Overlays et cordes"),
    (
        "shell.helpers.simulate_tip",
        "Ropes swing while they settle after a change instead of showing their final shape at once",
        "Les cordes se balancent jusqu'à leur position de repos après une modification au lieu d'afficher directement leur forme finale",
    ),
    ("shell.helpers.names", "Names", "Noms"),
    (
        "shell.helpers.names_tip",
        "Target names next to point entities in the 2D views, with the class when zoomed in",
        "Noms des entités ponctuelles à côté d'elles dans les vues 2D, avec la classe quand on zoome",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_draws_and_resets() {
        let mut state = hp_app::testing::test_state();
        crate::register_translations();
        let mut page = HelpersPage;
        assert_eq!(page.id(), "helpers");
        assert!(!page.label().is_empty());
        state.settings.helpers.connections = ConnectionsMode::All;
        state.settings.helpers.show_ropes = false;
        hp_app::testing::run_ui(&mut state, |ui, state| page.ui(ui, state));
        page.reset(&mut state);
        assert_eq!(state.settings.helpers, HelpersSettings::default());
        assert!(state.settings_dirty);
        let problems = hp_app::i18n::lint_table(TR);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
