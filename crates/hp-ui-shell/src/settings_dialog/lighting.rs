use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::{BakePriority, LightingSettings};
use hp_app::{AppState, t};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(LightingPage)]
}

struct LightingPage;

impl SettingsPage for LightingPage {
    fn id(&self) -> &'static str {
        "lighting"
    }

    fn label(&self) -> String {
        t!("shell.lighting.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "💡"
    }

    fn order(&self) -> i32 {
        page_order::VIEWPORT + 20
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let s = &mut state.settings.lighting;
        heading(ui, t!("shell.lighting.baking"));
        changed |= ui
            .checkbox(&mut s.auto_bake, t!("shell.lighting.auto_bake"))
            .on_hover_text(t!("shell.lighting.auto_bake_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.auto_scope, t!("shell.lighting.auto_scope"))
            .on_hover_text(t!("shell.lighting.auto_scope_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.visible_first, t!("shell.lighting.visible_first"))
            .on_hover_text(t!("shell.lighting.visible_first_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.prepare, t!("shell.lighting.prepare"))
            .on_hover_text(t!("shell.lighting.prepare_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.pause_minimized, t!("shell.lighting.pause_minimized"))
            .on_hover_text(t!("shell.lighting.pause_minimized_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.pause_unfocused, t!("shell.lighting.pause_unfocused"))
            .on_hover_text(t!("shell.lighting.pause_unfocused_tip"))
            .changed();
        super::form_grid(ui, "hp_shell_lighting").show(ui, |ui| {
            ui.label(t!("shell.lighting.threads"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.threads).range(0..=LightingSettings::MAX_THREADS))
                .on_hover_text(t!("shell.lighting.threads_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.priority"));
            let label = |p: BakePriority| match p {
                BakePriority::Idle => t!("shell.lighting.priority_idle"),
                BakePriority::BelowNormal => t!("shell.lighting.priority_below_normal"),
                BakePriority::Normal => t!("shell.lighting.priority_normal"),
            };
            egui::ComboBox::from_id_salt("hp_shell_lighting_priority").selected_text(label(s.priority)).show_ui(
                ui,
                |ui| {
                    for p in [BakePriority::Idle, BakePriority::BelowNormal, BakePriority::Normal] {
                        changed |= ui.selectable_value(&mut s.priority, p, label(p)).changed();
                    }
                },
            );
            ui.end_row();
            ui.label(t!("shell.lighting.drag_pause"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.drag_pause_ms).range(LightingSettings::DRAG_PAUSE_RANGE).suffix(" ms"))
                .on_hover_text(t!("shell.lighting.drag_pause_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.upload"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut s.upload_mb_per_frame)
                        .range(LightingSettings::UPLOAD_RANGE)
                        .suffix(" MB"),
                )
                .on_hover_text(t!("shell.lighting.upload_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.max_luxels"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.max_luxels_millions).range(LightingSettings::LUXELS_RANGE))
                .on_hover_text(t!("shell.lighting.max_luxels_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.auto_scope_luxels"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.auto_scope_luxels_millions).range(LightingSettings::LUXELS_RANGE))
                .on_hover_text(t!("shell.lighting.auto_scope_luxels_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.auto_scope_radius"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.auto_scope_radius).range(LightingSettings::RADIUS_RANGE).suffix(" u"))
                .on_hover_text(t!("shell.lighting.auto_scope_radius_tip"))
                .changed();
            ui.end_row();
        });
        heading(ui, t!("shell.lighting.budget_heading"));
        changed |= ui.checkbox(&mut s.show_budget, t!("shell.lighting.show_budget")).changed();
        super::form_grid(ui, "hp_shell_lighting_budget").show(ui, |ui| {
            ui.label(t!("shell.lighting.tiny_scale"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.tiny_scale).range(1..=64))
                .on_hover_text(t!("shell.lighting.tiny_scale_tip"))
                .changed();
            ui.end_row();
            ui.label(t!("shell.lighting.styles"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.budget_styles).range(1..=4))
                .on_hover_text(t!("shell.lighting.styles_tip"))
                .changed();
            ui.end_row();
        });
        if changed {
            state.settings_dirty = true;
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.lighting = LightingSettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.lighting.page", "Lighting", "Éclairage"),
    ("shell.lighting.baking", "Baked lighting preview", "Aperçu de l'éclairage précalculé"),
    ("shell.lighting.auto_bake", "Bake when the preview is turned on", "Précalculer à l'activation de l'aperçu"),
    (
        "shell.lighting.auto_bake_tip",
        "Starts a bake when the baked lighting preview is turned on and nothing is baked yet. Uses the whole map by default; the optional regional mode applies to this automatic start.",
        "Lance un précalcul quand l'aperçu précalculé est activé et que rien n'est encore calculé. Toute la carte est utilisée par défaut ; le mode régional optionnel concerne ce démarrage automatique.",
    ),
    (
        "shell.lighting.auto_scope",
        "Bake only around the camera on big maps",
        "Précalculer autour de la caméra sur les grandes cartes",
    ),
    (
        "shell.lighting.auto_scope_tip",
        "Optional regional mode for automatic preview activation on maps above the luxel threshold. The region follows the camera after it leaves the central half and stays still for a second. Off by default. Bake lighting always calculates the whole map and never follows the camera.",
        "Mode régional optionnel à l'activation automatique de l'aperçu sur les cartes au-dessus du seuil de luxels. La zone suit la caméra quand elle quitte la moitié centrale et reste immobile une seconde. Désactivé par défaut. Précalculer l'éclairage calcule toujours toute la carte sans suivre la caméra.",
    ),
    (
        "shell.lighting.auto_scope_luxels",
        "Big map threshold (millions of luxels)",
        "Seuil de grande carte (millions de luxels)",
    ),
    (
        "shell.lighting.auto_scope_luxels_tip",
        "Threshold for the optional regional bake on automatic preview activation. Bake lighting remains a whole-map calculation.",
        "Seuil du précalcul régional optionnel à l'activation automatique de l'aperçu. Précalculer l'éclairage conserve un calcul de toute la carte.",
    ),
    ("shell.lighting.auto_scope_radius", "Big map radius", "Rayon des grandes cartes"),
    ("shell.lighting.visible_first", "Bake the visible area first", "Précalculer d'abord la zone visible"),
    (
        "shell.lighting.prepare",
        "Prepare the baked preview in the background",
        "Préparer l'aperçu précalculé en arrière-plan",
    ),
    (
        "shell.lighting.prepare_tip",
        "Once the map is open and edits pause, the lighting is compiled and lit at low priority so the baked preview shows at once when you turn it on. Turn it off on a slow computer to keep its processor and memory free.",
        "Une fois la carte ouverte et les modifications en pause, l'éclairage est compilé et calculé en priorité basse pour que l'aperçu précalculé s'affiche aussitôt quand vous l'activez. Désactivez-le sur un ordinateur lent pour garder son processeur et sa mémoire libres.",
    ),
    (
        "shell.lighting.visible_first_tip",
        "On big maps the area around the camera is baked first so lighting shows in seconds, then the whole map is baked in the background and replaces it.",
        "Sur les grandes cartes, la zone autour de la caméra est précalculée d'abord pour que l'éclairage apparaisse en quelques secondes, puis toute la carte est précalculée en arrière-plan et la remplace.",
    ),
    (
        "shell.lighting.auto_scope_radius_tip",
        "Radius of the optional automatic regional preview. Bake around the camera uses its separate radius in the Lighting tab. Used by the next automatic regional bake.",
        "Rayon de l'aperçu régional automatique optionnel. Précalculer autour de la caméra utilise son rayon distinct dans l'onglet Éclairage. Utilisé par le prochain précalcul régional automatique.",
    ),
    ("shell.lighting.threads", "Baking threads", "Threads de précalcul"),
    (
        "shell.lighting.threads_tip",
        "0 picks a sensible number: every core but a quarter of them, and at least two cores are always left free for the editor and the other programs. A higher number is capped to every core but two. Used by the next bake.",
        "0 choisit un nombre raisonnable : tous les cœurs sauf un quart, et au moins deux cœurs restent toujours libres pour l'éditeur et les autres programmes. Un nombre plus grand est limité à tous les cœurs sauf deux. Utilisé par le prochain précalcul.",
    ),
    ("shell.lighting.priority", "Baking priority", "Priorité du précalcul"),
    (
        "shell.lighting.priority_idle",
        "Lowest (only when the computer is idle)",
        "La plus basse (seulement quand l'ordinateur est inactif)",
    ),
    ("shell.lighting.priority_below_normal", "Below normal", "Inférieure à la normale"),
    (
        "shell.lighting.priority_normal",
        "Normal (faster, can slow down other programs)",
        "Normale (plus rapide, peut ralentir les autres programmes)",
    ),
    (
        "shell.lighting.pause_minimized",
        "Pause the lighting preview while the window is minimized",
        "Mettre l'aperçu de l'éclairage en pause quand la fenêtre est réduite",
    ),
    (
        "shell.lighting.pause_minimized_tip",
        "Stops the bakes and relights while Hammer Pro is minimized and takes them up again when the window comes back.",
        "Arrête les précalculs et les mises à jour pendant que Hammer Pro est réduit et les reprend au retour de la fenêtre.",
    ),
    (
        "shell.lighting.pause_unfocused",
        "Pause the lighting preview while another window is active",
        "Mettre l'aperçu de l'éclairage en pause quand une autre fenêtre est active",
    ),
    (
        "shell.lighting.pause_unfocused_tip",
        "Stops the bakes and relights while you work in another program (the game, for example) and takes them up again when you come back to Hammer Pro.",
        "Arrête les précalculs et les mises à jour pendant que vous travaillez dans un autre programme (le jeu, par exemple) et les reprend quand vous revenez dans Hammer Pro.",
    ),
    ("shell.lighting.drag_pause", "Relight a dragged light after", "Recalculer une lumière déplacée après"),
    (
        "shell.lighting.drag_pause_tip",
        "While you drag a light, its baked lighting is computed again once the mouse rests this long, and when you release it; the real time lights show the change in between. 0 relights continuously while dragging.",
        "Pendant que vous déplacez une lumière, son éclairage précalculé est recalculé dès que la souris reste immobile ce temps, et quand vous la relâchez ; les lumières en temps réel montrent le changement entre-temps. 0 recalcule en continu pendant le déplacement.",
    ),
    ("shell.lighting.upload", "Lightmap upload per frame", "Envoi de lightmaps par image"),
    (
        "shell.lighting.upload_tip",
        "How much lightmap data goes to the graphics card in one frame. Lower keeps the editor smoother while a bake is shown, higher shows a big bake a little sooner.",
        "Quantité de données de lightmap envoyée à la carte graphique en une image. Plus bas garde l'éditeur plus fluide pendant l'affichage d'un précalcul, plus haut affiche un grand précalcul un peu plus tôt.",
    ),
    ("shell.lighting.max_luxels", "Full resolution luxels (millions)", "Luxels en pleine résolution (millions)"),
    (
        "shell.lighting.max_luxels_tip",
        "Maps with more luxels are previewed with coarser lightmaps (larger lightmap scales) to save memory and time. Used by the next bake.",
        "Les cartes avec plus de luxels sont prévisualisées avec des lightmaps plus grossières (échelles de lightmap plus grandes) pour économiser mémoire et temps. Utilisé par le prochain précalcul.",
    ),
    ("shell.lighting.budget_heading", "Lightmap budget", "Budget des lightmaps"),
    (
        "shell.lighting.show_budget",
        "Show the lightmap budget in the status bar",
        "Afficher le budget des lightmaps dans la barre d'état",
    ),
    ("shell.lighting.tiny_scale", "Tiny lightmap scale", "Très petite échelle de lightmap"),
    (
        "shell.lighting.tiny_scale_tip",
        "Faces with this lightmap scale or less are drawn red by the lightmap grid and selected by \"Select faces with a tiny lightmap scale\"",
        "Les faces avec cette échelle de lightmap ou moins sont dessinées en rouge par la grille de lightmap et sélectionnées par « Sélectionner les faces à petite échelle de lightmap »",
    ),
    ("shell.lighting.styles", "Light styles", "Styles de lumière"),
    (
        "shell.lighting.styles_tip",
        "Lightmaps per face: 1 without switchable lights, up to 4",
        "Lightmaps par face : 1 sans lumières commutables, jusqu'à 4",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_edits_settings() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut page = LightingPage;
        hp_app::testing::run_ui(&mut state, |ui, state| page.ui(ui, state));
        state.settings.lighting.tiny_scale = 9;
        page.reset(&mut state);
        assert_eq!(state.settings.lighting, LightingSettings::default());
        assert!(state.settings_dirty);
        assert!(hp_app::i18n::lint_table(TR).is_empty());
    }
}
