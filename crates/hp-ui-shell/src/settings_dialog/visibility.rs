use egui::{RichText, Ui};
use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::VisibilitySettings;
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(VisibilityPage)]
}

struct VisibilityPage;

impl SettingsPage for VisibilityPage {
    fn id(&self) -> &'static str {
        "visibility"
    }

    fn label(&self) -> String {
        t!("shell.visibility.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "▣"
    }

    fn order(&self) -> i32 {
        page_order::EDITING + 50
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let current_material = state.current_material.clone();
        let s = &mut state.settings.visibility;
        let mut changed = false;
        heading(ui, t!("shell.visibility.cordons"));
        super::form_grid(ui, "hp_shell_visibility_cordons").show(ui, |ui| {
            ui.label(t!("shell.visibility.cordon_material"));
            ui.horizontal(|ui| {
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut s.cordon_material)
                            .desired_width(super::field_width(ui, 240.0, 200.0))
                            .font(egui::TextStyle::Monospace)
                            .hint_text(VisibilitySettings::default().cordon_material),
                    )
                    .on_hover_text(t!("shell.visibility.cordon_material_tip"))
                    .changed();
                if ui
                    .add_enabled(
                        !current_material.is_empty() && current_material != s.cordon_material,
                        egui::Button::new(t!("shell.visibility.use_current")),
                    )
                    .on_hover_text(&current_material)
                    .clicked()
                {
                    s.cordon_material.clone_from(&current_material);
                    changed = true;
                }
            });
            ui.end_row();
            ui.label(t!("shell.visibility.shade"));
            ui.horizontal(|ui| {
                changed |=
                    ui.checkbox(&mut s.shade_outside_2d, "").on_hover_text(t!("shell.visibility.shade_tip")).changed();
                changed |= ui
                    .add_enabled(
                        s.shade_outside_2d,
                        egui::Slider::new(&mut s.shade_opacity, 0.05..=0.9).show_value(true).fixed_decimals(2),
                    )
                    .on_hover_text(t!("shell.visibility.shade_opacity"))
                    .changed();
            });
            ui.end_row();
            ui.label(t!("shell.visibility.names"));
            changed |=
                ui.checkbox(&mut s.show_cordon_names, "").on_hover_text(t!("shell.visibility.names_tip")).changed();
            ui.end_row();
        });
        ui.label(RichText::new(t!("shell.visibility.cordon_note")).weak().small());
        heading(ui, t!("shell.visibility.move"));
        changed |= ui.checkbox(&mut s.move_remove_others, t!("shell.visibility.move_remove_others")).changed();
        changed |= ui.checkbox(&mut s.move_hide, t!("shell.visibility.move_hide")).changed();
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.visibility = VisibilitySettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.visibility.page", "Cordons and visgroups", "Cordons et visgroups"),
    ("shell.visibility.cordons", "Cordons", "Cordons"),
    ("shell.visibility.cordon_material", "Cordon material", "Matériau du cordon"),
    (
        "shell.visibility.cordon_material_tip",
        "Material of the brushes that seal the cordoned area when only the cordons are compiled",
        "Matériau des brushes qui ferment la zone cordonnée quand seuls les cordons sont compilés",
    ),
    ("shell.visibility.use_current", "Use the current material", "Utiliser le matériau actuel"),
    ("shell.visibility.shade", "Shade outside the cordons in 2D", "Ombrer l'extérieur des cordons en 2D"),
    (
        "shell.visibility.shade_tip",
        "While cordons are used, darken the 2D views outside every active cordon",
        "Quand les cordons sont utilisés, assombrir les vues 2D en dehors de tous les cordons actifs",
    ),
    ("shell.visibility.shade_opacity", "Shade opacity", "Opacité de l'ombre"),
    ("shell.visibility.names", "Cordon names in 2D", "Noms des cordons en 2D"),
    (
        "shell.visibility.names_tip",
        "Show the name of each active cordon at its corner when there are several",
        "Afficher le nom de chaque cordon actif dans son coin quand il y en a plusieurs",
    ),
    (
        "shell.visibility.cordon_note",
        "Objects outside every active cordon are dimmed in the views and cannot be picked while cordons are used.",
        "Les objets en dehors de tous les cordons actifs sont estompés dans les vues et ne peuvent pas être sélectionnés quand les cordons sont utilisés.",
    ),
    ("shell.visibility.move", "Move to visgroup", "Déplacer vers un visgroup"),
    (
        "shell.visibility.move_remove_others",
        "Remove the objects from their other visgroups",
        "Retirer les objets de leurs autres visgroups",
    ),
    ("shell.visibility.move_hide", "Hide the visgroup afterwards", "Masquer le visgroup ensuite"),
];
