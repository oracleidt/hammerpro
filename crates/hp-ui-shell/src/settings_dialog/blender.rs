use egui::RichText;
use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::BlenderFormat;
use hp_app::{AppState, t, tf, theme};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(BlenderPage)]
}

struct BlenderPage;

fn format_label(f: BlenderFormat) -> &'static str {
    match f {
        BlenderFormat::Gltf => "glTF (GLB)",
        BlenderFormat::Obj => "OBJ",
        BlenderFormat::Fbx => "FBX",
    }
}

impl SettingsPage for BlenderPage {
    fn id(&self) -> &'static str {
        "blender"
    }

    fn label(&self) -> String {
        t!("shell.blender.title").to_owned()
    }

    fn icon(&self) -> &'static str {
        "🔗"
    }

    fn order(&self) -> i32 {
        page_order::GENERAL + 55
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        heading(ui, t!("shell.blender.heading"));
        ui.label(t!("shell.blender.intro"));
        ui.add_space(4.0);
        if !state.runtime.blender.exe_checked {
            state.runtime.blender.detect_requested = true;
        }
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(t!("shell.blender.path"));
            let reply = ui.add(
                egui::TextEdit::singleline(&mut state.settings.blender.path)
                    .desired_width(320.0)
                    .hint_text(t!("shell.blender.path_hint")),
            );
            if reply.changed() {
                changed = true;
                state.runtime.blender.detect_requested = true;
            }
            if ui.button(t!("shell.blender.browse")).clicked() {
                state.runtime.blender.browse_requested = true;
            }
            if ui.button(t!("shell.blender.detect")).clicked() {
                state.settings.blender.path.clear();
                state.runtime.blender.detect_requested = true;
                changed = true;
            }
        });
        match &state.runtime.blender.exe {
            Some(exe) => {
                ui.colored_label(theme::success(), tf!("shell.blender.found", exe.display()));
            }
            None => {
                ui.colored_label(theme::warning(), t!("shell.blender.not_found"));
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(t!("shell.blender.format"));
            for f in BlenderFormat::ALL {
                changed |= ui.radio_value(&mut state.settings.blender.format, f, format_label(f)).changed();
            }
        });
        changed |= ui
            .checkbox(&mut state.settings.blender.live_link, t!("shell.blender.live_link"))
            .on_hover_text(t!("shell.blender.live_link_tip"))
            .changed();
        if changed {
            state.settings_dirty = true;
        }

        heading(ui, t!("shell.blender.actions"));
        let selection = !state.doc.selection().is_empty();
        let linked = state.runtime.blender.link.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(selection, egui::Button::new(t!("action.send_to_blender"))).clicked() {
                state.runtime.blender.send_requested = true;
            }
            let busy = state.runtime.blender.pull_in_progress;
            if ui.add_enabled(linked && !busy, egui::Button::new(t!("action.pull_from_blender"))).clicked() {
                state.runtime.blender.pull_requested = true;
            }
            if ui.add_enabled(linked, egui::Button::new(t!("action.stop_blender_link"))).clicked() {
                state.runtime.blender.stop_requested = true;
            }
            if ui.button(t!("action.install_blender_addon")).clicked() {
                state.runtime.blender.install_requested = true;
            }
        });
        if !selection {
            ui.label(RichText::new(t!("shell.blender.select_hint")).small().weak());
        }
        if let Some(link) = &state.runtime.blender.link {
            ui.add_space(4.0);
            ui.label(tf!("shell.blender.linked", link.file.display(), link.pulls));
        }
        if state.runtime.blender.pull_in_progress {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t!("shell.blender.pulling"));
            });
        }
        if let Some(note) = &state.runtime.blender.note {
            let color = if note.ok { theme::success() } else { theme::warning() };
            ui.colored_label(color, &note.text);
        }
    }
}

pub(super) const TR: TrTable = &[
    ("shell.blender.title", "Blender", "Blender"),
    ("shell.blender.heading", "Blender link", "Lien avec Blender"),
    (
        "shell.blender.intro",
        "Send displacements (or brushes, or the model of a prop) to Blender, sculpt and edit them there, and Hammer Pro picks the result up by itself: displacements are reshaped in place, brushes are replaced, props are recompiled.",
        "Envoyez des displacements (ou des brushes, ou le modèle d'un prop) vers Blender, sculptez et modifiez-les là-bas, et Hammer Pro récupère le résultat tout seul : les displacements sont remodelés sur place, les brushes remplacés, les props recompilés.",
    ),
    ("shell.blender.path", "Blender program", "Programme Blender"),
    ("shell.blender.path_hint", "automatic", "automatique"),
    ("shell.blender.browse", "Browse...", "Parcourir..."),
    ("shell.blender.detect", "Detect", "Détecter"),
    ("shell.blender.found", "Blender found: {0}", "Blender trouvé : {0}"),
    (
        "shell.blender.not_found",
        "Blender was not found. Browse to blender.exe.",
        "Blender est introuvable. Choisissez blender.exe.",
    ),
    ("shell.blender.format", "File format", "Format de fichier"),
    ("shell.blender.live_link", "Live link", "Lien direct"),
    (
        "shell.blender.live_link_tip",
        "Hammer Pro watches the exported file and updates the map each time Blender saves or presses Update in Hammer Pro.",
        "Hammer Pro surveille le fichier exporté et met la map à jour à chaque enregistrement dans Blender ou clic sur Update in Hammer Pro.",
    ),
    ("shell.blender.actions", "Actions", "Actions"),
    (
        "shell.blender.select_hint",
        "Select displacements, brushes or a prop to send.",
        "Sélectionnez des displacements, des brushes ou un prop à envoyer.",
    ),
    ("shell.blender.linked", "Linked file: {0} ({1} updates)", "Fichier lié : {0} ({1} mises à jour)"),
    ("shell.blender.pulling", "Reading the file from Blender...", "Lecture du fichier de Blender..."),
];
