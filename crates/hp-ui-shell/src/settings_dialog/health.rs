use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::HealthSettings;
use hp_app::{AppState, t};
use hp_editor::limits::LimitProfile;

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(HealthPage)]
}

struct HealthPage;

fn profile_label(p: Option<LimitProfile>) -> &'static str {
    match p {
        None => t!("shell.health.profile_auto"),
        Some(LimitProfile::Stock) => t!("shell.health.profile_stock"),
        Some(LimitProfile::Gmod32) => t!("shell.health.profile_gmod32"),
        Some(LimitProfile::Gmod64) => t!("shell.health.profile_gmod64"),
        Some(LimitProfile::PlusPlus) => t!("shell.health.profile_plusplus"),
    }
}

impl SettingsPage for HealthPage {
    fn id(&self) -> &'static str {
        "health"
    }

    fn label(&self) -> String {
        t!("shell.health.page").to_owned()
    }

    fn icon(&self) -> &'static str {
        "⚠"
    }

    fn order(&self) -> i32 {
        page_order::COMPILE + 10
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let mut changed = false;
        let s = &mut state.settings.health;
        heading(ui, t!("shell.health.checks"));
        changed |= ui
            .checkbox(&mut s.live_checks, t!("shell.health.live_checks"))
            .on_hover_text(t!("shell.health.live_checks_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.live_leak_check, t!("shell.health.live_leak"))
            .on_hover_text(t!("shell.health.live_leak_tip"))
            .changed();
        changed |= ui.checkbox(&mut s.show_leak_path, t!("shell.health.show_leak_path")).changed();
        changed |= ui
            .checkbox(&mut s.visible_only, t!("shell.health.visible_only"))
            .on_hover_text(t!("shell.health.visible_only_tip"))
            .changed();
        changed |= ui
            .checkbox(&mut s.warn_duplicates, t!("shell.health.warn_duplicates"))
            .on_hover_text(t!("shell.health.warn_duplicates_tip"))
            .changed();
        let p = &mut state.settings.placement;
        changed |= ui
            .checkbox(&mut p.problem_checks, t!("shell.placement.problem_checks"))
            .on_hover_text(t!("shell.placement.problem_checks_tip"))
            .changed();
        ui.add_enabled_ui(p.problem_checks, |ui| {
            ui.indent("hp_overlap_check", |ui| {
                changed |= ui
                    .checkbox(&mut p.overlap_check, t!("shell.placement.overlap_check"))
                    .on_hover_text(t!("shell.placement.overlap_check_tip"))
                    .changed();
            });
        });
        let s = &mut state.settings.health;
        heading(ui, t!("shell.health.limits"));
        ui.horizontal(|ui| {
            ui.label(t!("shell.health.profile"));
            let current = s.profile();
            egui::ComboBox::from_id_salt("hp_health_profile").selected_text(profile_label(current)).show_ui(ui, |ui| {
                for choice in std::iter::once(None).chain(LimitProfile::ALL.into_iter().map(Some)) {
                    if ui.selectable_label(current == choice, profile_label(choice)).clicked() {
                        s.set_profile(choice);
                        changed = true;
                    }
                }
            });
        })
        .response
        .on_hover_text(t!("shell.health.profile_tip"));
        heading(ui, t!("shell.health.portals"));
        let mut width = s.portal_width_px();
        ui.horizontal(|ui| {
            ui.label(t!("shell.health.portal_width"));
            if ui.add(egui::Slider::new(&mut width, 1.0..=4.0).step_by(1.0).suffix(" px")).changed() {
                s.portal_width = width;
                changed = true;
            }
        });
        changed |= ui.checkbox(&mut s.leaf_grid, t!("shell.health.leaf_grid")).changed();
        changed |= ui
            .checkbox(&mut s.offer_portals, t!("shell.health.offer_portals"))
            .on_hover_text(t!("shell.health.offer_portals_tip"))
            .changed();
        if changed {
            state.settings_dirty = true;
            state.runtime.health.portal_version = state.runtime.health.portal_version.wrapping_add(1);
        }
    }

    fn reset(&mut self, state: &mut AppState) {
        state.settings.health = HealthSettings::default();
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.health.page", "Map health", "Santé de la map"),
    ("shell.health.checks", "Checks", "Vérifications"),
    ("shell.health.live_checks", "Check the map as I edit", "Vérifier la map pendant l'édition"),
    (
        "shell.health.live_checks_tip",
        "Problems, limits and edicts are checked in the background a moment after each change (the status bar shows the counts)",
        "Problèmes, limites et edicts sont vérifiés en arrière-plan peu après chaque modification (la barre d'état affiche les totaux)",
    ),
    ("shell.health.live_leak", "Look for leaks as I edit", "Chercher les fuites pendant l'édition"),
    (
        "shell.health.live_leak_tip",
        "A flood fill like the compiler's finds leaks without compiling; the compile stays the reference",
        "Un remplissage comme celui du compilateur trouve les fuites sans compiler ; la compilation reste la référence",
    ),
    ("shell.health.show_leak_path", "Draw the path of the leak found", "Tracer le chemin de la fuite trouvée"),
    (
        "shell.health.warn_duplicates",
        "Warn when a new brush or entity is an exact copy of another",
        "Avertir quand un nouveau brush ou une nouvelle entité est une copie exacte d'un autre",
    ),
    (
        "shell.health.warn_duplicates_tip",
        "After creating, pasting or duplicating, a message tells when the result sits exactly on an identical brush or entity (Check for problems can delete the copies)",
        "Après une création, un collage ou une duplication, un message signale quand le résultat est exactement sur un brush ou une entité identique (Vérifier les problèmes peut supprimer les copies)",
    ),
    (
        "shell.health.visible_only",
        "Check for problems lists visible objects only",
        "Vérifier les problèmes ne liste que les objets visibles",
    ),
    (
        "shell.health.visible_only_tip",
        "Problems of hidden objects stay counted in the status bar",
        "Les problèmes des objets masqués restent comptés dans la barre d'état",
    ),
    ("shell.health.limits", "Engine limits", "Limites du moteur"),
    ("shell.health.profile", "Limits of:", "Limites de :"),
    (
        "shell.health.profile_tip",
        "The maxima the limits are measured against (they differ between Source builds)",
        "Les maximums auxquels les limites sont comparées (ils diffèrent selon les versions de Source)",
    ),
    ("shell.health.profile_auto", "Automatic (Garry's Mod 64-bit)", "Automatique (Garry's Mod 64 bits)"),
    ("shell.health.profile_stock", "Source SDK 2013", "Source SDK 2013"),
    ("shell.health.profile_gmod32", "Garry's Mod 32-bit", "Garry's Mod 32 bits"),
    ("shell.health.profile_gmod64", "Garry's Mod 64-bit", "Garry's Mod 64 bits"),
    ("shell.health.profile_plusplus", "VBSP++ (Tools++)", "VBSP++ (Tools++)"),
    ("shell.health.portals", "Portal files", "Fichiers de portails"),
    ("shell.health.portal_width", "Portal line width", "Épaisseur des portails"),
    ("shell.health.leaf_grid", "Show the 1024 unit leaf grid", "Afficher la grille des feuilles de 1024 unités"),
    (
        "shell.health.offer_portals",
        "Offer the new portal file after a compile",
        "Proposer le nouveau fichier de portails après une compilation",
    ),
    (
        "shell.health.offer_portals_tip",
        "Portals already shown are reloaded automatically",
        "Les portails déjà affichés sont rechargés automatiquement",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_draws_and_resets() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut page = HealthPage;
        hp_app::testing::run_ui(&mut state, |ui, state| page.ui(ui, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        hp_app::testing::run_ui(&mut state, |ui, state| page.ui(ui, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        state.settings.health.live_checks = false;
        page.reset(&mut state);
        assert!(state.settings.health.live_checks);
        assert!(state.settings_dirty);
        for p in std::iter::once(None).chain(LimitProfile::ALL.into_iter().map(Some)) {
            assert!(!profile_label(p).starts_with("shell."));
        }
    }
}
