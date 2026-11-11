use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{InstanceDisplay, InstancesSettings};
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(InstancesPage)]
}

struct InstancesPage;

impl SettingsPage for InstancesPage {
    fn id(&self) -> &'static str {
        "instances"
    }

    fn label(&self) -> String {
        t!("shell.instances.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "🗐"
    }

    fn order(&self) -> i32 {
        page_order::EDITING + 40
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let s = &mut state.settings.instances;
        heading(ui, t!("shell.instances.display"));
        hp_app::widgets::choice_row(ui, |ui| {
            for (value, key) in [
                (InstanceDisplay::Hidden, "shell.instances.hidden"),
                (InstanceDisplay::Tinted, "shell.instances.tinted"),
                (InstanceDisplay::Normal, "shell.instances.normal"),
            ] {
                changed |= ui.radio_value(&mut s.display, value, t!(key)).changed();
            }
        });
        changed |= ui
            .checkbox(&mut s.outline_selected, t!("shell.instances.outline"))
            .on_hover_text(t!("shell.instances.outline_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.show_context, t!("shell.instances.context"))
            .on_hover_text(t!("shell.instances.context_tip"))
            .changed();
        heading(ui, t!("shell.instances.create"));
        super::form_grid(ui, "hp_shell_instances_create").show(ui, |ui| {
            ui.label(t!("shell.instances.folder"));
            let r = ui
                .add(egui::TextEdit::singleline(&mut s.folder).desired_width(260.0))
                .on_hover_text(t!("shell.instances.folder_tip"));
            if r.changed() {
                s.folder.retain(|c| c != '"' && !c.is_control());
                changed = true;
            }
            ui.end_row();
        });
        heading(ui, t!("shell.instances.collapse"));
        changed |= ui
            .checkbox(&mut s.collapse_skips_hidden, t!("shell.instances.skip_hidden"))
            .on_hover_text(t!("shell.instances.skip_hidden_tip"))
            .changed();
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.instances = InstancesSettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.instances.page", "Instances", "Instances"),
    ("shell.instances.display", "Contents of func_instance", "Contenu des func_instance"),
    ("shell.instances.hidden", "Hidden", "Masqué"),
    ("shell.instances.tinted", "Tinted", "Teinté"),
    ("shell.instances.normal", "Normal", "Normal"),
    ("shell.instances.outline", "Outline the selected instances", "Encadrer les instances sélectionnées"),
    (
        "shell.instances.outline_tip",
        "Draw the bounds of the contents of the selected instances and the name of their file",
        "Dessiner les limites du contenu des instances sélectionnées et le nom de leur fichier",
    ),
    (
        "shell.instances.context",
        "Show the map around an edited instance",
        "Afficher la map autour d'une instance modifiée",
    ),
    (
        "shell.instances.context_tip",
        "An instance opened with \"Edit instance\" is drawn inside the map holding it, tinted and read only",
        "Une instance ouverte avec « Modifier l'instance » est dessinée dans la map qui la contient, teintée et en lecture seule",
    ),
    ("shell.instances.create", "New instances", "Nouvelles instances"),
    ("shell.instances.folder", "Default folder", "Dossier par défaut"),
    (
        "shell.instances.folder_tip",
        "Where \"Create instance from selection\" saves new instances: relative to the map folder, or a full path",
        "Où « Créer une instance depuis la sélection » enregistre les nouvelles instances : relatif au dossier de la map, ou un chemin complet",
    ),
    ("shell.instances.collapse", "Collapse", "Fusion"),
    (
        "shell.instances.skip_hidden",
        "Leave out hidden objects of the instance files",
        "Ignorer les objets masqués des fichiers d'instance",
    ),
    (
        "shell.instances.skip_hidden_tip",
        "Like Hammer++: objects hidden in an instance file are not added to the map when it is collapsed",
        "Comme Hammer++ : les objets masqués dans un fichier d'instance ne sont pas ajoutés à la map quand il est fusionné",
    ),
];
