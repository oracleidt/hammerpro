use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::MeshSettings;
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(MeshPage)]
}

struct MeshPage;

impl SettingsPage for MeshPage {
    fn id(&self) -> &'static str {
        "mesh"
    }

    fn label(&self) -> String {
        t!("shell.mesh.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "⬟"
    }

    fn order(&self) -> i32 {
        page_order::EDITING + 25
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let s = &mut state.settings.mesh;
        heading(ui, t!("shell.mesh.vertex"));
        changed |= ui
            .checkbox(&mut s.live_preview, t!("shell.mesh.live_preview"))
            .on_hover_text(t!("shell.mesh.live_preview_tip"))
            .changed();
        changed |=
            ui.checkbox(&mut s.uv_lock, t!("shell.mesh.uv_lock")).on_hover_text(t!("shell.mesh.uv_lock_tip")).changed();
        heading(ui, t!("shell.mesh.displacement"));
        changed |= ui
            .checkbox(&mut s.four_way_blends, t!("shell.mesh.four_way"))
            .on_hover_text(t!("shell.mesh.four_way_tip"))
            .changed();
        super::form_grid(ui, "hp_shell_mesh_noise").show(ui, |ui| {
            ui.label(t!("shell.mesh.noise_min"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.noise_min).range(-1024.0..=1024.0).speed(0.25))
                .on_hover_text(t!("shell.mesh.noise_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.mesh.noise_max"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.noise_max).range(-1024.0..=1024.0).speed(0.25))
                .on_hover_text(t!("shell.mesh.noise_tip"))
                .changed();
            ui.end_row();
        });
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.mesh = MeshSettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.mesh.page", "Vertices and displacements", "Sommets et displacements"),
    ("shell.mesh.vertex", "Vertex tool", "Outil sommets"),
    ("shell.mesh.live_preview", "Live preview while dragging", "Aperçu en direct pendant le glissement"),
    (
        "shell.mesh.live_preview_tip",
        "Dragged solids show their new shape with their textures while the mouse button is held (up to 64 solids at once)",
        "Les solides glissés montrent leur nouvelle forme avec leurs textures tant que le bouton est maintenu (jusqu'à 64 solides à la fois)",
    ),
    ("shell.mesh.uv_lock", "UV lock", "Verrouillage UV"),
    (
        "shell.mesh.uv_lock_tip",
        "Textures follow the moved vertices instead of staying fixed in the world (also in the vertex tool options)",
        "Les textures suivent les sommets déplacés au lieu de rester fixes dans le monde (aussi dans les options de l'outil sommets)",
    ),
    ("shell.mesh.displacement", "Displacements", "Displacements"),
    ("shell.mesh.four_way", "Paint 4-way blends", "Peindre les mélanges à 4 textures"),
    (
        "shell.mesh.four_way_tip",
        "Alpha painting can paint each of the 4 textures of a 4-way blend material (Lightmapped_4WayBlend), like Hammer++",
        "La peinture d'alpha peut peindre chacune des 4 textures d'un matériau de mélange à 4 textures (Lightmapped_4WayBlend), comme Hammer++",
    ),
    ("shell.mesh.noise_min", "Minimum noise height", "Hauteur minimale du bruit"),
    ("shell.mesh.noise_max", "Maximum noise height", "Hauteur maximale du bruit"),
    (
        "shell.mesh.noise_tip",
        "Range of the random heights added by Add noise to displacements",
        "Plage des hauteurs aléatoires ajoutées par Ajouter du bruit aux displacements",
    ),
];
