use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{MergePrompt, MeshSettings, VertexSettings};
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
        let prompt = &mut state.settings.vertex.merge_prompt;
        ui.horizontal(|ui| {
            ui.label(t!("shell.mesh.merge_prompt")).on_hover_text(t!("shell.mesh.merge_prompt_tip"));
            egui::ComboBox::from_id_salt("hp_shell_mesh_merge_prompt")
                .selected_text(merge_prompt_label(*prompt))
                .show_ui(ui, |ui| {
                    for m in MergePrompt::ALL {
                        changed |= ui.selectable_value(prompt, m, merge_prompt_label(m)).changed();
                    }
                })
                .response
                .on_hover_text(t!("shell.mesh.merge_prompt_tip"));
        });
        let s = &mut state.settings.mesh;
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
        state.settings.vertex = VertexSettings::default();
        state.settings_dirty = true;
    }
}

fn merge_prompt_label(m: MergePrompt) -> &'static str {
    match m {
        MergePrompt::Off => t!("shell.mesh.merge_prompt.off"),
        MergePrompt::Ask => t!("shell.mesh.merge_prompt.ask"),
        MergePrompt::Always => t!("shell.mesh.merge_prompt.always"),
        MergePrompt::Never => t!("shell.mesh.merge_prompt.never"),
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
    ("shell.mesh.merge_prompt", "Stacked vertices", "Sommets superposés"),
    (
        "shell.mesh.merge_prompt_tip",
        "What happens when a vertex move (drag, arrow keys, snap, align, scale) ends with vertices of a solid on top of each other: off (they merge silently, as in earlier versions), ask like Hammer, always merge them, or never merge (the move is undone)",
        "Ce qui se passe quand un déplacement de sommets (glisser, flèches, aimanter, aligner, mise à l'échelle) se termine avec des sommets d'un solide les uns sur les autres : désactivé (ils fusionnent sans question, comme dans les versions précédentes), demander comme Hammer, toujours les fusionner, ou ne jamais fusionner (le déplacement est annulé)",
    ),
    (
        "shell.mesh.merge_prompt.off",
        "Off: stacked vertices merge silently, as before",
        "Désactivé : les sommets superposés fusionnent sans question, comme avant",
    ),
    (
        "shell.mesh.merge_prompt.ask",
        "Ask before merging stacked vertices",
        "Demander avant de fusionner les sommets superposés",
    ),
    ("shell.mesh.merge_prompt.always", "Always merge stacked vertices", "Toujours fusionner les sommets superposés"),
    (
        "shell.mesh.merge_prompt.never",
        "Never merge stacked vertices (undo the move)",
        "Ne jamais fusionner les sommets superposés (annuler le déplacement)",
    ),
];
