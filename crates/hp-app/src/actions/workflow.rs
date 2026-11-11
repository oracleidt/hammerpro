use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::runtime::WorkflowRuntime;
use crate::settings::{GridMode, RenderModeSetting, SelectModeSetting};
use crate::state::AppState;
use crate::view::{QuadrantView, ViewKind};

action_list! { workflow_actions {
    CloseMap => "close_map", File, FileClose, sc(CTRL, Key::F4), Ready;
    ToggleIgnoreGroups => "toggle_ignore_groups", Edit, EditSelectGroups, sc(CTRL, Key::W), Ready;
    Zoom2dIn => "zoom_2d_in", View, ViewZoom, sc(NONE, Key::Plus), Ready;
    Zoom2dOut => "zoom_2d_out", View, ViewZoom, sc(NONE, Key::Minus), Ready;
    ResetViewSplits => "reset_view_splits", View, ViewZoom, None, Ready;
    ViewTop => "view_top", View, ViewType, sc(NONE, Key::F2), Ready;
    ViewSide => "view_side", View, ViewType, sc(NONE, Key::F3), Ready;
    ViewFront => "view_front", View, ViewType, sc(NONE, Key::F4), Ready;
    View3dTextured => "view_3d_textured", View, ViewType, sc(NONE, Key::F5), Ready;
    View3dFlat => "view_3d_flat", View, ViewType, sc(SHIFT, Key::F5), Ready;
    CycleView2d => "cycle_view_2d", View, ViewType, sc(NONE, Key::Tab), Ready;
    ToggleViewInfo => "toggle_view_info", View, ViewDisplayInfo, sc(NONE, Key::O), Ready;
    ToggleBoundsOverlay => "toggle_bounds_overlay", View, ViewDisplay, None, Ready;
    ShowHistory => "show_history", View, ViewPanels, None, Ready;
    ShowToolbar => "show_toolbar", View, ViewPanels, None, Ready;
    ShowToolPalette => "show_tool_palette", View, ViewPanels, None, Ready;
    ShowStatusBar => "show_status_bar", View, ViewPanels, None, Ready;
    ToolPaletteTwoColumns => "tool_palette_two_columns", View, ViewPanels, None, Ready;
    ToolbarTwoRows => "toolbar_two_rows", View, ViewPanels, None, Ready;
    ResetToolbar => "reset_toolbar", View, ViewPanels, None, Ready;
    SaveLayout => "save_layout", Window, WindowLayout, None, Ready;
    ResetLayout => "reset_layout", Window, WindowLayout, None, Ready;
    CloseOtherMaps => "close_other_maps", File, FileClose, None, Ready;
    NextMap => "next_map", Window, WindowMaps, sc(CTRL, Key::Tab), Ready;
    PreviousMap => "previous_map", Window, WindowMaps, sc(CTRL_SHIFT, Key::Tab), Ready;
    ViewLogical => "view_logical", View, ViewType, None, Ready;
    GridModeGlobal => "grid_mode_global", Map, MapGrids, None, Ready;
    GridModePerView => "grid_mode_per_view", Map, MapGrids, None, Ready;
    GridModeCustom => "grid_mode_custom", Map, MapGrids, None, Ready;
    GridModeSelection => "grid_mode_selection", Map, MapGrids, None, Ready;
    GridFromSelection => "grid_from_selection", Map, MapGrids, None, Ready;
    GridFromFace => "grid_from_face", Map, MapGrids, None, Ready;
    AttachGrid => "attach_grid", Map, MapGrids, None, Ready;
    DetachGrid => "detach_grid", Map, MapGrids, None, Ready;
    GridManager => "grid_manager", Map, MapGrids, None, Ready;
    RotateGrid45 => "rotate_grid_45", Map, MapGrids, None, Ready;
    AlignGridToSelection => "align_grid_to_selection", Map, MapGrids, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[Action::Zoom2dIn, Action::Zoom2dOut];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::CycleView2d => Some(WorkflowRuntime::target_view_type(state).kind().is_some_and(ViewKind::is_2d)),
        Action::CloseOtherMaps | Action::NextMap | Action::PreviousMap => Some(state.runtime.workflow.tabs.len() > 1),
        Action::GridModeCustom => Some(!state.all_custom_grids().is_empty()),
        Action::GridFromSelection => Some(!state.doc.selection().objects.is_empty()),
        Action::GridFromFace => Some(!state.doc.selection().faces.is_empty()),
        Action::AttachGrid => {
            Some(!state.doc.selection().objects.is_empty() && state.find_grid(&state.settings.grid.custom).is_some())
        }
        Action::DetachGrid => Some(state.doc.selection_grid(4096).is_some()),
        Action::AlignGridToSelection => Some(!state.doc.selection().is_empty()),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    let w = &state.settings.workflow;
    let view = WorkflowRuntime::target_view_type(state);
    let kind = view.kind();
    let mode = state.settings.view.render_mode;
    Some(match action {
        Action::ToggleIgnoreGroups => Check::Toggle(state.settings.edit.select_mode == SelectModeSetting::Solids),
        Action::ViewTop => Check::Radio(kind == Some(ViewKind::Top)),
        Action::ViewSide => Check::Radio(kind == Some(ViewKind::Side)),
        Action::ViewFront => Check::Radio(kind == Some(ViewKind::Front)),
        Action::View3dTextured => Check::Radio(kind == Some(ViewKind::Perspective) && mode != RenderModeSetting::Flat),
        Action::View3dFlat => Check::Radio(kind == Some(ViewKind::Perspective) && mode == RenderModeSetting::Flat),
        Action::ViewLogical => Check::Radio(view == QuadrantView::Logical),
        Action::ToggleViewInfo => Check::Toggle(w.show_view_info),
        Action::ToggleBoundsOverlay => Check::Toggle(w.show_bounds),
        Action::ShowHistory => Check::Toggle(w.show_history),
        Action::ShowToolbar => Check::Toggle(w.show_toolbar),
        Action::ShowToolPalette => Check::Toggle(w.show_tool_palette),
        Action::ShowStatusBar => Check::Toggle(w.show_status_bar),
        Action::ToolPaletteTwoColumns => Check::Toggle(w.tool_palette_two_columns()),
        Action::ToolbarTwoRows => Check::Toggle(w.toolbar_two_rows()),
        Action::GridModeGlobal => Check::Radio(state.settings.grid.mode == GridMode::Global),
        Action::GridModePerView => Check::Radio(state.settings.grid.mode == GridMode::PerView),
        Action::GridModeCustom => Check::Radio(state.settings.grid.mode == GridMode::Custom),
        Action::GridModeSelection => Check::Radio(state.settings.grid.mode == GridMode::Selection),
        Action::GridManager => Check::Toggle(state.ui.show_grid_manager),
        _ => return None,
    })
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::ShowHistory => Some("⟲"),
        Action::ToggleViewInfo => Some("ℹ"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.close_map", "Close", "Fermer"),
    ("action.toggle_ignore_groups", "Ignore groups", "Ignorer les groupes"),
    ("action.zoom_2d_in", "Zoom 2D views in", "Zoom avant des vues 2D"),
    ("action.zoom_2d_out", "Zoom 2D views out", "Zoom arrière des vues 2D"),
    ("action.reset_view_splits", "Reset view splits", "Réinitialiser le partage des vues"),
    ("action.view_top", "Top view", "Vue de dessus"),
    ("action.view_side", "Side view", "Vue de côté"),
    ("action.view_front", "Front view", "Vue de face"),
    ("action.view_3d_textured", "3D textured view", "Vue 3D texturée"),
    ("action.view_3d_flat", "3D flat view", "Vue 3D unie"),
    ("action.cycle_view_2d", "Next 2D view", "Vue 2D suivante"),
    ("action.toggle_view_info", "3D view information", "Informations de la vue 3D"),
    ("action.toggle_bounds_overlay", "Bounding boxes", "Boîtes englobantes"),
    ("action.show_history", "Undo history", "Historique des modifications"),
    ("action.show_toolbar", "Toolbar", "Barre d'outils"),
    ("action.show_tool_palette", "Tool palette", "Palette d'outils"),
    ("action.show_status_bar", "Status bar", "Barre d'état"),
    ("action.tool_palette_two_columns", "Tool palette in two columns", "Palette d'outils sur deux colonnes"),
    ("action.toolbar_two_rows", "Toolbar on two rows", "Barre d'outils sur deux lignes"),
    ("action.reset_toolbar", "Reset the toolbar", "Réinitialiser la barre d'outils"),
    ("action.save_layout", "Save layout...", "Enregistrer la disposition..."),
    ("action.reset_layout", "Reset layout", "Réinitialiser la disposition"),
    ("action.close_other_maps", "Close other maps", "Fermer les autres maps"),
    ("action.next_map", "Next map", "Map suivante"),
    ("action.previous_map", "Previous map", "Map précédente"),
    ("action.view_logical", "2D logical view", "Vue logique 2D"),
    ("action.grid_mode_global", "Global grid", "Grille globale"),
    ("action.grid_mode_per_view", "One grid per view", "Une grille par vue"),
    ("action.grid_mode_custom", "Custom grid", "Grille personnalisée"),
    ("action.grid_mode_selection", "Grid of the selection", "Grille de la sélection"),
    ("action.grid_from_selection", "Grid from selection", "Grille depuis la sélection"),
    ("action.grid_from_face", "Grid from face", "Grille depuis la face"),
    ("action.attach_grid", "Attach the grid to the selection", "Attacher la grille à la sélection"),
    ("action.detach_grid", "Detach the grid of the selection", "Détacher la grille de la sélection"),
    ("action.grid_manager", "Grid manager...", "Gestionnaire de grilles..."),
    ("action.rotate_grid_45", "Rotate grid 45 degrees", "Pivoter la grille de 45 degrés"),
    (
        "action.align_grid_to_selection",
        "Align grid to the selected edge or face",
        "Aligner la grille sur l'arête ou la face sélectionnée",
    ),
    ("shell.undo.attach_grid", "Attach grid", "Attachement de grille"),
    ("shell.undo.detach_grid", "Detach grid", "Détachement de grille"),
    ("shell.undo.edit_grid", "Edit grid", "Modification de grille"),
    ("shell.undo.delete_grid", "Delete grid", "Suppression de grille"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_type_checks_follow_the_target_view() {
        let mut state = crate::testing::test_state();
        state.ui.focused_view = Some(1);
        assert_eq!(WorkflowRuntime::target_view(&state), 1);
        assert_eq!(Action::ViewTop.check(&state), Check::Radio(true));
        assert!(Action::CycleView2d.enabled(&state));
        state.ui.hovered_view = Some(0);
        assert_eq!(Action::View3dTextured.check(&state), Check::Radio(true));
        assert_eq!(Action::View3dFlat.check(&state), Check::Radio(false));
        assert!(!Action::CycleView2d.enabled(&state), "the 3D view has no 2D type to cycle");
        state.ui.hovered_view = Some(9);
        state.ui.focused_view = None;
        assert_eq!(WorkflowRuntime::target_view(&state), 0, "bad indices fall back to the first view");
        state.settings.edit.select_mode = SelectModeSetting::Solids;
        assert_eq!(Action::ToggleIgnoreGroups.check(&state), Check::Toggle(true));
    }

    #[test]
    fn logical_view_type_is_checked_and_not_cycled() {
        let mut state = crate::testing::test_state();
        state.ui.focused_view = Some(2);
        assert_eq!(Action::ViewLogical.check(&state), Check::Radio(false));
        state.settings.ui.quadrants[2] = QuadrantView::LOGICAL_INDEX;
        assert_eq!(WorkflowRuntime::target_view_type(&state), QuadrantView::Logical);
        assert_eq!(Action::ViewLogical.check(&state), Check::Radio(true));
        for other in [Action::ViewTop, Action::ViewSide, Action::ViewFront, Action::View3dTextured, Action::View3dFlat]
        {
            assert_eq!(other.check(&state), Check::Radio(false), "{other:?}");
        }
        assert!(!Action::CycleView2d.enabled(&state), "Tab has no 2D type to cycle in the logical view");
        assert!(Action::ViewLogical.enabled(&state));
        state.ui.focused_view = Some(1);
        state.ui.hovered_logical = Some(2);
        assert_eq!(WorkflowRuntime::target_view(&state), 2);
        state.ui.hovered_view = Some(3);
        assert_eq!(WorkflowRuntime::target_view(&state), 3, "a projected view under the pointer wins");
    }
}
