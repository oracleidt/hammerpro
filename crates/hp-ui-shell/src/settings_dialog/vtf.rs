use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::RichText;
use hp_app::actions::Action;
use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::vtf_external as external;
use hp_app::{AppState, t, tf, theme};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(VtfPage { pick: None })]
}

struct VtfPage {
    pick: Option<Receiver<Option<PathBuf>>>,
}

impl VtfPage {
    fn start_browse(&mut self, ctx: &egui::Context) {
        if self.pick.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-vtf-editor-pick".into()).spawn(move || {
            let file = rfd::FileDialog::new().add_filter("Programs", &["exe"]).pick_file();
            let _ = tx.send(file);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.pick = Some(rx),
            Err(e) => log::warn!("cannot start the file dialog thread: {e}"),
        }
    }

    fn poll_browse(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(rx) = &self.pick else { return };
        match rx.try_recv() {
            Ok(file) => {
                self.pick = None;
                if let Some(f) = file {
                    state.settings.vtf.editor_path = f.display().to_string();
                    state.settings_dirty = true;
                }
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            Err(TryRecvError::Disconnected) => self.pick = None,
        }
    }
}

impl SettingsPage for VtfPage {
    fn id(&self) -> &'static str {
        "vtf"
    }

    fn label(&self) -> String {
        t!("shell.vtf.title").to_owned()
    }

    fn icon(&self) -> &'static str {
        "▣"
    }

    fn order(&self) -> i32 {
        page_order::GENERAL + 56
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        self.poll_browse(&ctx, state);
        heading(ui, t!("shell.vtf.heading"));
        ui.label(t!("shell.vtf.intro"));
        ui.add_space(4.0);
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(t!("shell.vtf.path"));
            let reply = ui.add(
                egui::TextEdit::singleline(&mut state.settings.vtf.editor_path)
                    .desired_width(super::field_width(ui, 320.0, 180.0))
                    .hint_text(t!("shell.vtf.path_hint")),
            );
            changed |= reply.changed();
            if ui.button(t!("shell.vtf.browse")).clicked() {
                self.start_browse(&ctx);
            }
            if ui.button(t!("shell.vtf.detect")).on_hover_text(t!("shell.vtf.detect_tip")).clicked() {
                state.settings.vtf.editor_path.clear();
                external::rescan();
                changed = true;
            }
        });
        let found = external::resolve_nonblocking(&state.settings.vtf.editor_path);
        match &found {
            Some(e) => {
                ui.colored_label(theme::success(), tf!("shell.vtf.found", e.label, e.path.display()));
            }
            None => {
                ui.colored_label(theme::warning(), t!("shell.vtf.not_found"));
            }
        }
        if let Some(all) = external::detect_if_ready()
            && !all.is_empty()
        {
            ui.add_space(4.0);
            ui.label(RichText::new(t!("shell.vtf.detected")).strong());
            for e in all {
                ui.horizontal(|ui| {
                    ui.label(format!("{}  ", e.label));
                    ui.weak(e.path.display().to_string());
                    if ui.small_button(t!("shell.vtf.use")).clicked() {
                        state.settings.vtf.editor_path = e.path.display().to_string();
                        changed = true;
                    }
                });
            }
        }
        ui.add_space(6.0);
        changed |= ui
            .checkbox(&mut state.settings.vtf.prefer_integrated, t!("shell.vtf.prefer_integrated"))
            .on_hover_text(t!("shell.vtf.prefer_integrated_tip"))
            .changed();
        if changed {
            state.settings_dirty = true;
        }
        ui.add_space(8.0);
        ui.label(RichText::new(t!("shell.vtf.note")).small().weak());
        ui.add_space(4.0);
        if ui.button(t!("action.vtf_editor")).clicked() {
            state.request(Action::VtfEditor);
        }
    }
}

pub(super) const TR: TrTable = &[
    ("shell.vtf.title", "VTF editor", "Éditeur de VTF"),
    ("shell.vtf.heading", "Texture (VTF) editor", "Éditeur de textures (VTF)"),
    (
        "shell.vtf.intro",
        "The VTF button of the material editor opens a texture in VTFEdit, VTFEdit++, VTFEdit Reloaded or another VTF editor installed on this computer. Without one, the built in VTF editor opens instead.",
        "Le bouton VTF de l'éditeur de matériaux ouvre une texture dans VTFEdit, VTFEdit++, VTFEdit Reloaded ou un autre éditeur VTF installé sur cet ordinateur. Sans éditeur externe, l'éditeur VTF intégré s'ouvre à la place.",
    ),
    ("shell.vtf.path", "External editor", "Éditeur externe"),
    ("shell.vtf.path_hint", "automatic", "automatique"),
    ("shell.vtf.browse", "Browse...", "Parcourir..."),
    ("shell.vtf.detect", "Detect", "Détecter"),
    (
        "shell.vtf.detect_tip",
        "Search the usual install folders, the PATH and the program registered for .vtf files",
        "Cherche dans les dossiers d'installation habituels, le PATH et le programme associé aux fichiers .vtf",
    ),
    ("shell.vtf.found", "Editor used: {0} ({1})", "Éditeur utilisé : {0} ({1})"),
    (
        "shell.vtf.not_found",
        "No external VTF editor found: the built in editor is used.",
        "Aucun éditeur VTF externe trouvé : l'éditeur intégré est utilisé.",
    ),
    ("shell.vtf.detected", "Editors found on this computer", "Éditeurs trouvés sur cet ordinateur"),
    ("shell.vtf.use", "Use", "Utiliser"),
    ("shell.vtf.prefer_integrated", "Always use the built in editor", "Toujours utiliser l'éditeur intégré"),
    (
        "shell.vtf.prefer_integrated_tip",
        "Open textures in the built in editor even when an external one is available",
        "Ouvre les textures dans l'éditeur intégré même si un éditeur externe est disponible",
    ),
    (
        "shell.vtf.note",
        "Textures inside a VPK or a GMA are extracted to a temporary file. Saving in the external editor copies the result into your custom texture folder and reloads it here.",
        "Les textures d'un VPK ou d'un GMA sont extraites dans un fichier temporaire. Enregistrer dans l'éditeur externe copie le résultat dans votre dossier de textures personnalisé et le recharge ici.",
    ),
];
