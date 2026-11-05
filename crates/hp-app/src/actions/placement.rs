use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { placement_actions {
    ToolSprinkle => "tool_sprinkle", Tools, ToolsSwitch, None, Ready;
    CreatePrefab => "create_prefab", Tools, ToolsPlacement, sc(CTRL, Key::R), Ready;
    InsertOriginalPrefab => "insert_original_prefab", Tools, ToolsPlacement, sc(ALT_SHIFT, Key::C), Ready;
    ShowPrefabs => "show_prefabs", View, ViewPanels, None, Ready;
    ScatterByMaterial => "scatter_by_material", Tools, ToolsPlacement, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::CreatePrefab => Some(!state.doc.selection().objects.is_empty()),
        Action::InsertOriginalPrefab => {
            let rt = &state.runtime.placement;
            Some(rt.selected_prefab.is_some() || rt.armed_prefab.is_some())
        }
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::ShowPrefabs => Some(Check::Toggle(state.settings.placement.show_prefabs)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::CreatePrefab => Some("➕"),
        Action::InsertOriginalPrefab => Some("⌖"),
        Action::ShowPrefabs => Some("⊞"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.tool_sprinkle", "Sprinkle tool", "Outil sprinkle"),
    ("action.create_prefab", "Create prefab...", "Créer un prefab..."),
    ("action.insert_original_prefab", "Insert prefab at original size", "Insérer le prefab à sa taille d'origine"),
    ("action.show_prefabs", "Prefabs", "Prefabs"),
    ("action.scatter_by_material", "Scatter by material...", "Semis par matériau..."),
    ("tool.sprinkle", "Sprinkle", "Sprinkle"),
    ("shell.undo.insert_prefab", "Insert prefab", "Insertion de prefab"),
    ("shell.undo.sprinkle", "Sprinkle", "Sprinkle"),
];
