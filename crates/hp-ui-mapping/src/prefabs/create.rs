use std::path::PathBuf;

use hp_app::runtime::CreatePrefabRequest;
use hp_app::{AppState, t, tf};

use super::library::{folder_components, prefab_path, subfolders, valid_component};
use crate::dialogs::MapDialog;

const DEFAULT_NAME: &str = "prefab";

#[derive(Default)]
pub(crate) struct CreatePrefabDialog {
    was_open: bool,
    name: String,
    folder: String,
    folders: Vec<String>,
    replace: bool,
    focus: bool,
    exists: Option<(PathBuf, bool)>,
}

impl CreatePrefabDialog {
    fn open(&mut self, state: &AppState) {
        let lib = state.settings.placement.library_dir();
        self.folders = subfolders(&lib);
        self.folder = state
            .runtime
            .placement
            .selected_prefab
            .as_ref()
            .and_then(|p| p.parent()?.strip_prefix(&lib).ok().map(|r| r.to_string_lossy().replace('\\', "/")))
            .filter(|f| folder_components(f).is_some())
            .unwrap_or_default();
        if !valid_component(self.name.trim()) {
            DEFAULT_NAME.clone_into(&mut self.name);
        }
        self.replace = false;
        self.focus = true;
        self.exists = None;
    }

    fn target(&self, state: &AppState) -> Option<PathBuf> {
        prefab_path(&state.settings.placement.library_dir(), &self.folder, &self.name)
    }

    fn target_exists(&mut self, target: &PathBuf) -> bool {
        match &self.exists {
            Some((p, e)) if p == target => *e,
            _ => {
                let e = target.exists();
                self.exists = Some((target.clone(), e));
                e
            }
        }
    }
}

pub(crate) fn window(dialog: &mut CreatePrefabDialog, ctx: &egui::Context, state: &mut AppState) {
    let open = state.runtime.placement.create_open;
    if open && !dialog.was_open {
        dialog.open(state);
    }
    dialog.was_open = open;
    if !open {
        return;
    }
    let count = state.doc.selection().objects.len();
    let mut keep_open = true;
    let mut submit = false;
    let mut cancel = false;
    hp_app::chrome::Window::new(t!("prefabs.create.title"))
        .id(egui::Id::unique("hp_prefabs_create"))
        .open(&mut keep_open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            egui::Grid::new("hp_prefabs_create_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label(t!("prefabs.create.name"));
                let r = ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(260.0));
                if dialog.focus {
                    crate::util::focus_select_all(&r, &dialog.name);
                    dialog.focus = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
                ui.end_row();
                ui.label(t!("prefabs.create.folder"));
                ui.horizontal(|ui| {
                    let root = t!("prefabs.create.root").to_owned();
                    let shown = if dialog.folder.is_empty() { root.clone() } else { dialog.folder.clone() };
                    egui::ComboBox::from_id_salt("hp_prefabs_create_folder").selected_text(shown).width(170.0).show_ui(
                        ui,
                        |ui| {
                            ui.selectable_value(&mut dialog.folder, String::new(), root);
                            for f in &dialog.folders {
                                ui.selectable_value(&mut dialog.folder, f.clone(), f.as_str());
                            }
                        },
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut dialog.folder)
                            .hint_text(t!("prefabs.create.new_folder"))
                            .desired_width(110.0),
                    )
                    .on_hover_text(t!("prefabs.create.folder_tip"));
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            let target = dialog.target(state);
            let mut can_save = count > 0;
            match &target {
                Some(path) => {
                    ui.label(egui::RichText::new(path.display().to_string()).weak().small());
                    if dialog.target_exists(path) {
                        ui.checkbox(&mut dialog.replace, t!("prefabs.create.replace"));
                        can_save &= dialog.replace;
                    }
                }
                None => {
                    ui.colored_label(ui.visuals().warn_fg_color, t!("prefabs.create.invalid"));
                    can_save = false;
                }
            }
            if count == 0 {
                ui.colored_label(ui.visuals().warn_fg_color, t!("prefabs.create.nothing"));
            } else {
                ui.label(tf!(crate::util::plural(count, "prefabs.create.count_one", "prefabs.create.count"), count));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(can_save, egui::Button::new(t!("prefabs.create.save"))).clicked() {
                    submit = true;
                }
                if ui.button(t!("prefabs.create.cancel")).clicked() {
                    cancel = true;
                }
            });
            submit &= can_save;
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                cancel = true;
            }
        });
    if submit && let Some(path) = dialog.target(state) {
        let overwrite = dialog.replace;
        state.runtime.placement.create_request = Some(CreatePrefabRequest { path, overwrite });
        cancel = true;
    }
    if cancel || !keep_open {
        state.runtime.placement.create_open = false;
        dialog.was_open = false;
    }
}

impl MapDialog for CreatePrefabDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        window(self, ctx, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_editor::ObjRef;

    #[test]
    fn requests_a_safe_path() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let dir = crate::test_scratch("prefab_create");
        let mut state = hp_app::testing::test_state();
        state.settings.placement.prefab_library = Some(dir.clone());
        std::fs::create_dir_all(dir.join("doors")).expect("dir");
        let mut d = CreatePrefabDialog::default();
        hp_app::testing::run_ctx(&mut state, |ctx, state| window(&mut d, ctx, state));
        assert!(!d.was_open);
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        state.runtime.placement.create_open = true;
        hp_app::testing::run_ctx(&mut state, |ctx, state| window(&mut d, ctx, state));
        assert_eq!(d.folders, ["doors"]);
        assert_eq!(d.name, "prefab");
        d.folder = "doors".to_owned();
        d.name = "metal door".to_owned();
        assert_eq!(d.target(&state), Some(dir.join("doors").join("metal door.vmf")));
        d.name = "../escape".to_owned();
        assert_eq!(d.target(&state), None);
        d.name = "ok".to_owned();
        d.folder = "a/../../b".to_owned();
        assert_eq!(d.target(&state), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
