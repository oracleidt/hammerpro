use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { health_actions {
    CheckLeaks => "check_leaks", Map, MapInfo, None, Ready;
    SealAreaportal => "seal_areaportal", Map, MapOptimize, None, Ready;
    LoadPortalFile => "load_portal_file", Map, MapPortals, None, Ready;
    UnloadPortalFile => "unload_portal_file", Map, MapPortals, None, Ready;
    TogglePvs => "toggle_pvs", View, ViewDiagnostics, None, Ready;
    ShowPortalViewer => "show_portal_viewer", Map, MapPortals, None, Ready;
    BuildPortals => "build_portals", Map, MapPortals, sc(CTRL_SHIFT, Key::P), Ready;
    ShowDetailSuggestions => "show_detail_suggestions", Map, MapOptimize, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::UnloadPortalFile => Some(state.runtime.health.portals.is_some()),
        Action::BuildPortals => Some(!state.runtime.health.portal_build_running),
        Action::SealAreaportal => Some(!state.doc.selection().objects.is_empty()),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::TogglePvs => Some(Check::Toggle(state.runtime.health.pvs)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::CheckLeaks => Some("💧"),
        Action::LoadPortalFile => Some("🗁"),
        Action::TogglePvs => Some("👁"),
        Action::ShowPortalViewer => Some("▣"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.check_leaks", "Check for leaks", "Vérifier les fuites"),
    ("action.seal_areaportal", "Seal the opening with an areaportal", "Fermer l'ouverture par un areaportal"),
    ("action.load_portal_file", "Load portal file...", "Charger le fichier de portails..."),
    ("action.unload_portal_file", "Unload portal file", "Décharger le fichier de portails"),
    ("action.toggle_pvs", "Show visible clusters", "Afficher les clusters visibles"),
    ("action.show_portal_viewer", "Portals and visibility...", "Portails et visibilité..."),
    ("action.build_portals", "Build portals (geometry only)", "Construire les portails (géométrie seule)"),
    ("action.show_detail_suggestions", "func_detail suggestions...", "Suggestions de func_detail..."),
    ("shell.undo.convert_to_func_detail", "Convert to func_detail", "Convertir en func_detail"),
    ("shell.undo.seal_opening", "Seal opening", "Fermeture de l'ouverture"),
];
