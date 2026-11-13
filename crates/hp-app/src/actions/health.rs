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
    ShowOptimize => "show_optimize", Map, MapInfo, None, Ready;
    AnalyzeOptimization => "analyze_optimization", Map, MapOptimize, None, Ready;
    ToggleVisleaves => "toggle_visleaves", Map, MapOptimize, None, Ready;
    ShowCubemaps => "show_cubemaps", Tools, ToolsPlacement, None, Ready;
    ToggleCubemapAssignment => "toggle_cubemap_assignment", View, ViewDiagnostics, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::UnloadPortalFile => Some(state.runtime.health.portals.is_some()),
        Action::BuildPortals => Some(!state.runtime.health.portal_build_running),
        Action::AnalyzeOptimization => Some(!state.runtime.optimize.running),
        Action::ToggleVisleaves => Some(state.runtime.optimize.portals.is_some() || state.runtime.optimize.view),
        Action::SealAreaportal => Some(!state.doc.selection().objects.is_empty()),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::TogglePvs => Some(Check::Toggle(state.runtime.health.pvs)),
        Action::ToggleVisleaves => Some(Check::Toggle(state.runtime.optimize.view)),
        Action::ToggleCubemapAssignment => Some(Check::Toggle(state.runtime.cubemaps.view)),
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
    ("action.show_optimize", "Optimize the map...", "Optimiser la map..."),
    ("action.analyze_optimization", "Analyze the optimization", "Analyser l'optimisation"),
    ("action.toggle_visleaves", "Show visleaves and portals", "Afficher les visleaves et les portails"),
    ("shell.undo.convert_to_func_detail", "Convert to func_detail", "Convertir en func_detail"),
    ("shell.undo.seal_opening", "Seal opening", "Fermeture de l'ouverture"),
    ("shell.undo.place_areaportal", "Place areaportal", "Placement d'areaportal"),
    ("shell.undo.fit_areaportal", "Fit areaportal", "Ajustement d'areaportal"),
    ("shell.undo.link_areaportal", "Link areaportal", "Liaison d'areaportal"),
    ("shell.undo.fix_areaportal", "Fix areaportal", "Correction d'areaportal"),
    ("shell.undo.add_hint_brushes", "Add hint brushes", "Ajout de brushes hint"),
    ("shell.undo.fix_lights", "Fix lights", "Correction des lumières"),
    ("action.show_cubemaps", "Cubemaps...", "Cubemaps..."),
    ("action.toggle_cubemap_assignment", "Show cubemap assignment", "Afficher l'attribution des cubemaps"),
    ("shell.undo.place_cubemaps", "Place cubemaps", "Placement de cubemaps"),
];
