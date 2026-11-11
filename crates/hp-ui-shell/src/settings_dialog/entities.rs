use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{BrowserSettings, EntitiesSettings};
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(EntitiesPage::default())]
}

#[derive(Default)]
struct EntitiesPage {
    new_folder: String,
}

impl SettingsPage for EntitiesPage {
    fn id(&self) -> &'static str {
        "entities"
    }

    fn label(&self) -> String {
        t!("shell.entities.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "⚙"
    }

    fn order(&self) -> i32 {
        page_order::EDITING + 40
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let e = &mut state.settings.entities;
        heading(ui, t!("shell.entities.properties"));
        changed |= ui
            .checkbox(&mut e.io_esc_delimiter, t!("shell.entities.esc"))
            .on_hover_text(t!("shell.entities.esc_tip"))
            .changed();
        changed |= ui.checkbox(&mut e.paste_targetname, t!("shell.entities.paste_name")).changed();

        heading(ui, t!("shell.entities.face_edit"));
        super::form_grid(ui, "hp_shell_entities_face_edit").show(ui, |ui| {
            ui.label(t!("shell.entities.decimals"));
            changed |= ui.add(egui::DragValue::new(&mut e.texture_decimals).range(0..=6)).changed();
            ui.end_row();
            ui.label(t!("shell.entities.shift_step"));
            changed |= ui
                .add(egui::DragValue::new(&mut e.texture_shift_step).range(0.01..=64.0).speed(0.05).max_decimals(2))
                .changed();
            ui.end_row();
            ui.label(t!("shell.entities.scale_step"));
            changed |= ui
                .add(egui::DragValue::new(&mut e.texture_scale_step).range(0.0005..=1.0).speed(0.0005).max_decimals(4))
                .changed();
            ui.end_row();
        });

        let b = &mut state.settings.browser;
        heading(ui, t!("shell.entities.textures"));
        changed |= ui
            .checkbox(&mut b.fgd_exclusions, t!("shell.entities.fgd_exclusions"))
            .on_hover_text(t!("shell.entities.fgd_exclusions_tip"))
            .changed();
        changed |= ui.checkbox(&mut b.show_alpha, t!("shell.entities.show_alpha")).changed();
        ui.label(t!("shell.entities.excluded"));
        let mut remove = None;
        for (i, folder) in b.excluded_folders.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.small_button("×").on_hover_text(t!("common.remove")).clicked() {
                    remove = Some(i);
                }
                ui.monospace(folder);
            });
        }
        if let Some(i) = remove {
            b.excluded_folders.remove(i);
            changed = true;
        }
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.new_folder)
                    .hint_text(t!("shell.entities.excluded_hint"))
                    .desired_width(220.0),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let folder = self.new_folder.trim().replace('\\', "/").trim_matches('/').to_owned();
            let valid = !folder.is_empty() && !folder.contains('"');
            if (ui.add_enabled(valid, egui::Button::new(t!("common.add"))).clicked() || enter) && valid {
                if !b.excluded_folders.iter().any(|f| f.eq_ignore_ascii_case(&folder)) {
                    b.excluded_folders.push(folder);
                    changed = true;
                }
                self.new_folder.clear();
            }
        });

        heading(ui, t!("shell.entities.browsers"));
        changed |= ui
            .checkbox(&mut b.gmod_only, t!("shell.entities.gmod_only"))
            .on_hover_text(t!("shell.entities.gmod_only_tip"))
            .changed();
        super::form_grid(ui, "hp_shell_entities_browsers").show(ui, |ui| {
            ui.label(t!("shell.entities.tile"));
            changed |= ui.add(egui::Slider::new(&mut b.tile_size, 48.0..=256.0).step_by(8.0)).changed();
            ui.end_row();
            ui.label(t!("shell.entities.volume"));
            let mut volume = b.volume() * 100.0;
            if ui.add(egui::Slider::new(&mut volume, 0.0..=100.0).suffix("%").max_decimals(0)).changed() {
                b.sound_volume = volume / 100.0;
                changed = true;
            }
            ui.end_row();
        });
        changed |= ui.checkbox(&mut b.sound_autoplay, t!("shell.entities.autoplay")).changed();
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.entities = EntitiesSettings::default();
        let b = &mut state.settings.browser;
        let d = BrowserSettings::default();
        b.excluded_folders = d.excluded_folders;
        b.fgd_exclusions = d.fgd_exclusions;
        b.show_alpha = d.show_alpha;
        b.gmod_only = d.gmod_only;
        b.tile_size = d.tile_size;
        b.sound_volume = d.sound_volume;
        b.sound_autoplay = d.sound_autoplay;
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.entities.page", "Entities and browsers", "Entités et navigateurs"),
    ("shell.entities.properties", "Entity properties", "Propriétés des entités"),
    ("shell.entities.esc", "New outputs use the ESC separator", "Les nouvelles sorties utilisent le séparateur ESC"),
    (
        "shell.entities.esc_tip",
        "Newer Hammer builds and Hammer++ separate output fields with ESC: parameters may then contain commas. Garry's Mod reads both.",
        "Les versions récentes de Hammer et Hammer++ séparent les champs des sorties par ESC : les paramètres peuvent alors contenir des virgules. Garry's Mod lit les deux.",
    ),
    (
        "shell.entities.paste_name",
        "Paste keyvalues also pastes the name (targetname)",
        "Coller les keyvalues colle aussi le nom (targetname)",
    ),
    ("shell.entities.face_edit", "Texture application (Face Edit)", "Application de textures (Face Edit)"),
    ("shell.entities.decimals", "Decimals shown", "Décimales affichées"),
    ("shell.entities.shift_step", "Shift drag step", "Pas du décalage"),
    ("shell.entities.scale_step", "Scale drag step", "Pas de l'échelle"),
    ("shell.entities.textures", "Texture browser", "Navigateur de textures"),
    (
        "shell.entities.fgd_exclusions",
        "Hide the folders the game data excludes",
        "Masquer les dossiers exclus par les données du jeu",
    ),
    (
        "shell.entities.fgd_exclusions_tip",
        "Folders listed by @MaterialExclusion in the FGD files (tool textures always stay)",
        "Dossiers listés par @MaterialExclusion dans les FGD (les textures outils restent toujours)",
    ),
    ("shell.entities.show_alpha", "Show alpha over a checkerboard", "Afficher l'alpha sur un damier"),
    ("shell.entities.excluded", "Hidden folders:", "Dossiers masqués :"),
    ("shell.entities.excluded_hint", "folder or folder/sub", "dossier ou dossier/sous-dossier"),
    ("shell.entities.browsers", "Model, sound and particle browsers", "Navigateurs de modèles, sons et particules"),
    ("shell.entities.gmod_only", "Garry's Mod content only", "Contenu de Garry's Mod uniquement"),
    (
        "shell.entities.gmod_only_tip",
        "List only what every player has (no mounted games, addons or Workshop items)",
        "Ne lister que ce que tous les joueurs possèdent (ni jeux montés, ni addons, ni Workshop)",
    ),
    ("shell.entities.tile", "Grid tile size", "Taille des vignettes"),
    ("shell.entities.volume", "Sound preview volume", "Volume de l'écoute des sons"),
    ("shell.entities.autoplay", "Play sounds when selected", "Lire les sons à la sélection"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_draws_and_resets() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut page = EntitiesPage::default();
        state.settings.browser.favorites.push("a/b".into());
        state.settings.browser.excluded_folders.push("debug".into());
        state.settings.entities.io_esc_delimiter = false;
        hp_app::testing::run_ui(&mut state, |ui, state| page.ui(ui, state));
        page.reset(&mut state);
        assert!(state.settings.entities.io_esc_delimiter);
        assert!(state.settings.browser.excluded_folders.is_empty());
        assert_eq!(state.settings.browser.favorites, ["a/b"], "favorites are kept");
        assert!(hp_app::i18n::lint_table(TR).is_empty());
    }
}
