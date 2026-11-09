use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::settings::{ConnectionsMode, HelperScope};
use crate::state::AppState;

action_list! { helpers_actions {
    ToggleOverlays => "toggle_overlays", View, ViewDisplay, None, Ready;
    ToggleConnections => "toggle_connections", View, ViewDisplay, None, Ready;
    ToggleEditorObjects => "toggle_editor_objects", View, ViewDisplay, None, Ready;
    TogglePointEntities => "toggle_point_entities", View, ViewDisplay, None, Ready;
    ToggleEntityNames => "toggle_entity_names", View, ViewDisplay, None, Ready;
    ToggleBrushCenters => "toggle_brush_centers", View, ViewDisplay, None, Ready;
    ToggleHelpersAll => "toggle_helpers_all", View, ViewDisplay, None, Ready;
    ToggleRopes => "toggle_ropes", View, ViewDisplay, None, Ready;
    SimulateRopes => "simulate_ropes", View, ViewDisplay, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::SimulateRopes => Some(state.settings.helpers.show_ropes),
        Action::ToggleHelpersAll => Some(state.settings.view.show_helpers),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    let h = &state.settings.helpers;
    Some(match action {
        Action::ToggleOverlays => Check::Toggle(h.show_overlays),
        Action::ToggleConnections => Check::Toggle(h.connections != ConnectionsMode::Off),
        Action::ToggleEditorObjects => Check::Toggle(h.show_editor_objects),
        Action::TogglePointEntities => Check::Toggle(!state.runtime.helpers.point_entities_hidden(&state.doc)),
        Action::ToggleEntityNames => Check::Toggle(h.show_entity_names),
        Action::ToggleBrushCenters => Check::Toggle(h.show_brush_centers),
        Action::SimulateRopes => Check::Toggle(h.simulate_ropes),
        Action::ToggleHelpersAll => Check::Toggle(h.helper_scope == HelperScope::All),
        Action::ToggleRopes => Check::Toggle(h.show_ropes),
        _ => return None,
    })
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::ToggleEditorObjects => Some("👁"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.toggle_overlays", "Show overlays", "Afficher les overlays"),
    ("action.toggle_connections", "Show connections", "Afficher les connexions"),
    ("action.toggle_editor_objects", "Show editor objects", "Afficher les objets d'éditeur"),
    ("action.toggle_point_entities", "Show point entities", "Afficher les entités ponctuelles"),
    ("action.toggle_entity_names", "Show entity labels in 2D", "Afficher les noms des entités en 2D"),
    ("action.toggle_brush_centers", "Show brush centers in 2D", "Afficher le centre des brushes en 2D"),
    ("action.simulate_ropes", "Simulate ropes", "Simuler les cordes"),
    ("action.toggle_helpers_all", "Helpers of every entity", "Aides de toutes les entités"),
    ("action.toggle_ropes", "Show ropes", "Afficher les cordes"),
    ("shell.undo.edit_overlay", "Edit overlay", "Modifier l'overlay"),
    ("shell.undo.edit_overlay_faces", "Edit overlay faces", "Modifier les faces de l'overlay"),
    ("shell.undo.hide_point_entities", "Hide point entities", "Masquer les entités ponctuelles"),
    ("shell.undo.show_point_entities", "Show point entities", "Afficher les entités ponctuelles"),
];
