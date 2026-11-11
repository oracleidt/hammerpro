use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::SelectionSettings;
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(SelectionPage)]
}

struct SelectionPage;

impl SettingsPage for SelectionPage {
    fn id(&self) -> &'static str {
        "selection"
    }

    fn label(&self) -> String {
        t!("shell.selection.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "⬉"
    }

    fn order(&self) -> i32 {
        page_order::EDITING + 15
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let s = &mut state.settings.selection;
        heading(ui, t!("shell.selection.transforms"));
        egui::Grid::new("hp_shell_selection_transforms").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("shell.selection.rotation_step"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.rotation_snap_deg).range(1.0..=90.0).speed(0.5).suffix("°"))
                .on_hover_text(t!("shell.selection.rotation_step_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.selection.free_rotation"));
            ui.horizontal(|ui| {
                changed |=
                    ui.radio_value(&mut s.free_rotation_with_alt, false, t!("shell.selection.key_shift")).changed();
                changed |= ui.radio_value(&mut s.free_rotation_with_alt, true, t!("shell.selection.key_alt")).changed();
            });
            ui.end_row();
            ui.label(t!("shell.selection.handle_size"));
            changed |= ui
                .add(egui::Slider::new(&mut s.handle_size, 4.0..=16.0).step_by(1.0))
                .on_hover_text(t!("shell.selection.handle_size_tip"))
                .changed();
            ui.end_row();
        });
        heading(ui, t!("shell.selection.behavior"));
        changed |= ui
            .checkbox(&mut s.clone_keeps_visgroups, t!("shell.selection.clone_keeps_visgroups"))
            .on_hover_text(t!("shell.selection.clone_keeps_visgroups_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.shift_arrow_clones, t!("shell.selection.shift_arrow_clones"))
            .on_hover_text(t!("shell.selection.shift_arrow_clones_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.box_confirm, t!("shell.selection.box_confirm"))
            .on_hover_text(t!("shell.selection.box_confirm_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.stretch_handles, t!("shell.selection.stretch_handles"))
            .on_hover_text(t!("shell.selection.stretch_handles_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.gizmo_local, t!("shell.selection.gizmo_local"))
            .on_hover_text(t!("shell.selection.gizmo_local_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.click_to_zoom, t!("shell.selection.click_to_zoom"))
            .on_hover_text(t!("shell.selection.click_to_zoom_tip"))
            .changed();
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.selection = SelectionSettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.selection.page", "Selection", "Sélection"),
    ("shell.selection.transforms", "Transforms", "Transformations"),
    ("shell.selection.rotation_step", "Rotation step", "Pas de rotation"),
    (
        "shell.selection.rotation_step_tip",
        "Rotation handles, gizmo rings and the R key snap to this angle",
        "Les poignées de rotation, les anneaux du gizmo et la touche R s'arrêtent sur cet angle",
    ),
    ("shell.selection.free_rotation", "Free rotation key", "Touche de rotation libre"),
    ("shell.selection.key_shift", "Ctrl", "Ctrl"),
    ("shell.selection.key_alt", "Alt", "Alt"),
    ("shell.selection.handle_size", "Handle size", "Taille des poignées"),
    (
        "shell.selection.handle_size_tip",
        "Size of the handles of the selection box in 2D views",
        "Taille des poignées du cadre de sélection dans les vues 2D",
    ),
    ("shell.selection.behavior", "Behavior", "Comportement"),
    (
        "shell.selection.clone_keeps_visgroups",
        "Keep visgroups when cloning",
        "Garder les groupes de visibilité au clonage",
    ),
    (
        "shell.selection.clone_keeps_visgroups_tip",
        "Copies made with Shift or Ctrl while dragging, or with Shift+arrows, stay in the visgroups of their originals",
        "Les copies faites avec Maj ou Ctrl en glissant, ou avec Maj+flèches, restent dans les groupes de visibilité de leurs originaux",
    ),
    ("shell.selection.shift_arrow_clones", "Shift+arrows clone", "Maj+flèches clone"),
    (
        "shell.selection.shift_arrow_clones_tip",
        "Shift+arrow keys copy the selection one grid step away, like Hammer. Alt+arrows then nudge by one unit.",
        "Maj+flèches copie la sélection à un pas de grille, comme Hammer. Alt+flèches décale alors d'une unité.",
    ),
    (
        "shell.selection.box_confirm",
        "Adjustable box selection (Enter to confirm)",
        "Sélection par cadre ajustable (Entrée pour valider)",
    ),
    (
        "shell.selection.box_confirm_tip",
        "Like Hammer++: a box drawn in a 2D view stays after the mouse is released, its handles resize it and dragging inside moves it, Enter selects (Ctrl+Enter adds), Esc cancels. Off (the default): the selection happens on release.",
        "Comme Hammer++ : un cadre tracé dans une vue 2D reste après le relâchement de la souris, ses poignées le redimensionnent et un glisser à l'intérieur le déplace, Entrée sélectionne (Ctrl+Entrée ajoute), Échap annule. Désactivé (par défaut) : la sélection se fait au relâchement.",
    ),
    ("shell.selection.stretch_handles", "3D stretch handles", "Poignées d'étirement 3D"),
    (
        "shell.selection.stretch_handles_tip",
        "The Selection tool shows an arrow on each face of the selected brushes in the 3D view: drag one to stretch the selection along its axis, the opposite face stays (Ctrl: from the center, Alt: no snap, Shift: clone)",
        "L'outil Sélection affiche une flèche sur chaque face des brushes sélectionnés dans la vue 3D : glissez-en une pour étirer la sélection sur son axe, la face opposée reste en place (Ctrl : depuis le centre, Alt : sans magnétisme, Maj : cloner)",
    ),
    ("shell.selection.gizmo_local", "Gizmo in local space", "Gizmo en repère local"),
    (
        "shell.selection.gizmo_local_tip",
        "The 3D gizmo of a single point entity follows its angles",
        "Le gizmo 3D d'une seule entité ponctuelle suit ses angles",
    ),
    ("shell.selection.click_to_zoom", "Camera tool: click to zoom", "Outil caméra : cliquer pour zoomer"),
    (
        "shell.selection.click_to_zoom_tip",
        "With the camera tool, a click in a 2D view zooms in (Ctrl+click zooms out), like Hammer's Magnify tool",
        "Avec l'outil caméra, un clic dans une vue 2D zoome (Ctrl+clic dézoome), comme la loupe de Hammer",
    ),
];
