use std::path::PathBuf;

use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{PlacementSettings, Settings};
use hp_app::{AppState, t};

use crate::common::heading;
use crate::jobs::{Job, Poll};

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(PlacementPage::default())]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    Library,
    PrefabDir,
    SprinkleDir,
}

#[derive(Default)]
struct PlacementPage {
    picking: Option<(Pick, Job<Option<PathBuf>>)>,
}

fn pick_folder(ctx: &egui::Context, title: String, start: Option<PathBuf>) -> Job<Option<PathBuf>> {
    let run = move || {
        let mut d = rfd::FileDialog::new().set_title(title);
        if let Some(dir) = start.filter(|d| d.is_dir()) {
            d = d.set_directory(dir);
        }
        d.pick_folder()
    };
    if cfg!(target_os = "macos") { Job::ready(run()) } else { Job::spawn(ctx, "prefab-folder", run) }
}

fn folder_list(ui: &mut egui::Ui, id: &str, dirs: &mut Vec<PathBuf>) -> bool {
    let mut remove = None;
    egui::Grid::new(id).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, d) in dirs.iter().enumerate() {
            ui.label(d.display().to_string());
            if ui.small_button("✖").on_hover_text(t!("shell.placement.remove")).clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if dirs.is_empty() {
        ui.weak(t!("shell.placement.none"));
    }
    match remove {
        Some(i) if i < dirs.len() => {
            dirs.remove(i);
            true
        }
        _ => false,
    }
}

impl PlacementPage {
    fn poll_pick(&mut self, state: &mut AppState) {
        let Some((pick, job)) = &self.picking else { return };
        let path = match job.poll() {
            Poll::Pending => return,
            Poll::Ready(p) => p,
            Poll::Failed => None,
        };
        let pick = *pick;
        self.picking = None;
        let Some(path) = path else { return };
        let s = &mut state.settings.placement;
        match pick {
            Pick::Library => s.prefab_library = Some(path),
            Pick::PrefabDir if !s.prefab_dirs.contains(&path) => s.prefab_dirs.push(path),
            Pick::SprinkleDir if !s.sprinkle.extra_dirs.contains(&path) => s.sprinkle.extra_dirs.push(path),
            _ => return,
        }
        state.settings_dirty = true;
        state.runtime.placement.library_serial = state.runtime.placement.library_serial.wrapping_add(1);
        state.runtime.placement.sprinkle_refresh = true;
    }

    fn browse(&mut self, ui: &egui::Ui, pick: Pick, start: Option<PathBuf>) {
        if self.picking.is_none() {
            self.picking = Some((pick, pick_folder(ui.ctx(), t!("shell.placement.pick_title").to_owned(), start)));
        }
    }
}

impl SettingsPage for PlacementPage {
    fn id(&self) -> &'static str {
        "placement"
    }

    fn label(&self) -> String {
        t!("shell.placement.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "⊞"
    }

    fn order(&self) -> i32 {
        page_order::EDITING + 60
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.poll_pick(state);
        let mut changed = false;
        let mut rescan = false;
        heading(ui, t!("shell.placement.prefabs"));
        let library = state.settings.placement.library_dir();
        ui.horizontal(|ui| {
            ui.label(t!("shell.placement.library"));
            ui.label(egui::RichText::new(library.display().to_string()).monospace())
                .on_hover_text(t!("shell.placement.library_tip"));
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(self.picking.is_none(), egui::Button::new(t!("shell.placement.browse"))).clicked() {
                self.browse(ui, Pick::Library, Some(library.clone()));
            }
            if state.settings.placement.prefab_library.is_some() && ui.button(t!("shell.placement.default")).clicked() {
                state.settings.placement.prefab_library = None;
                changed = true;
                rescan = true;
            }
        });
        ui.add_space(4.0);
        ui.label(t!("shell.placement.extra_dirs"));
        if folder_list(ui, "hp_shell_placement_dirs", &mut state.settings.placement.prefab_dirs) {
            changed = true;
            rescan = true;
        }
        if ui.add_enabled(self.picking.is_none(), egui::Button::new(t!("shell.placement.add"))).clicked() {
            self.browse(ui, Pick::PrefabDir, None);
        }
        let s = &mut state.settings.placement;
        if ui
            .checkbox(&mut s.game_prefabs, t!("shell.placement.game_prefabs"))
            .on_hover_text(t!("shell.placement.game_prefabs_tip"))
            .changed()
        {
            changed = true;
            rescan = true;
        }
        changed |= ui
            .checkbox(&mut s.group_prefabs, t!("shell.placement.group"))
            .on_hover_text(t!("shell.placement.group_tip"))
            .changed();
        changed |= ui.checkbox(&mut s.show_prefabs, t!("shell.placement.show_tab")).changed();

        heading(ui, t!("shell.placement.sprinkle"));
        ui.label(t!("shell.placement.sprinkle_dirs"));
        ui.weak(Settings::config_dir().join("sprinkle").display().to_string());
        if folder_list(ui, "hp_shell_placement_sprinkle", &mut state.settings.placement.sprinkle.extra_dirs) {
            changed = true;
            state.runtime.placement.sprinkle_refresh = true;
        }
        if ui.add_enabled(self.picking.is_none(), egui::Button::new(t!("shell.placement.add"))).clicked() {
            self.browse(ui, Pick::SprinkleDir, None);
        }
        if changed {
            state.settings_dirty = true;
        }
        if rescan {
            state.runtime.placement.library_serial = state.runtime.placement.library_serial.wrapping_add(1);
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        let sprinkle = state.settings.placement.sprinkle.clone();
        state.settings.placement = PlacementSettings { sprinkle, ..PlacementSettings::default() };
        state.settings.placement.sprinkle.extra_dirs.clear();
        state.settings_dirty = true;
        state.runtime.placement.library_serial = state.runtime.placement.library_serial.wrapping_add(1);
        state.runtime.placement.sprinkle_refresh = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.placement.page", "Prefabs", "Prefabs"),
    ("shell.placement.prefabs", "Prefab library", "Bibliothèque de prefabs"),
    ("shell.placement.library", "Library folder:", "Dossier de la bibliothèque :"),
    (
        "shell.placement.library_tip",
        "New prefabs are saved here (Tools > Placement and props > Create prefab)",
        "Les nouveaux prefabs sont enregistrés ici (Outils > Placement et props > Créer un prefab)",
    ),
    ("shell.placement.browse", "Browse...", "Parcourir..."),
    ("shell.placement.default", "Use the default folder", "Utiliser le dossier par défaut"),
    ("shell.placement.extra_dirs", "More prefab folders (read only):", "Autres dossiers de prefabs (lecture seule) :"),
    ("shell.placement.add", "Add a folder...", "Ajouter un dossier..."),
    ("shell.placement.remove", "Remove this folder from the list", "Retirer ce dossier de la liste"),
    ("shell.placement.none", "No folder", "Aucun dossier"),
    ("shell.placement.pick_title", "Choose a folder", "Choisir un dossier"),
    ("shell.placement.game_prefabs", "List the prefabs of Garry's Mod", "Lister les prefabs de Garry's Mod"),
    (
        "shell.placement.game_prefabs_tip",
        "The Hammer prefab folders of the game install (bin/prefabs), read only",
        "Les dossiers de prefabs de Hammer dans l'installation du jeu (bin/prefabs), en lecture seule",
    ),
    ("shell.placement.group", "Group inserted prefabs", "Grouper les prefabs insérés"),
    (
        "shell.placement.group_tip",
        "The objects of each inserted prefab form a group",
        "Les objets de chaque prefab inséré forment un groupe",
    ),
    ("shell.placement.show_tab", "Show the Prefabs tab", "Afficher l'onglet Prefabs"),
    ("shell.placement.sprinkle", "Sprinkle", "Sprinkle"),
    (
        "shell.placement.sprinkle_dirs",
        "Definitions are read from the game (scripts/hammer/sprinkle), the Hammer++ folder, this folder and the folders below:",
        "Les définitions sont lues dans le jeu (scripts/hammer/sprinkle), le dossier de Hammer++, ce dossier et les dossiers ci-dessous :",
    ),
];
