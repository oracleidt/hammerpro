use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { placement_actions {
    ToolSprinkle => "tool_sprinkle", Tools, ToolsSwitch, None, Ready;
    CreatePrefab => "create_prefab", Tools, ToolsPlacement, sc(CTRL, Key::R), Ready;
    InsertOriginalPrefab => "insert_original_prefab", Tools, ToolsPlacement, sc(ALT_SHIFT, Key::C), Ready;
    ShowPrefabs => "show_prefabs", View, ViewPanels, None, Ready;
    ScatterByMaterial => "scatter_by_material", Tools, ToolsScatter, None, Ready;
    PlaceOnTop => "place_on_top", Tools, ToolsPlaceObjects, sc(ALT, Key::T), Ready;
    PlaceAgainstWall => "place_against_wall", Tools, ToolsPlaceObjects, sc(ALT, Key::L), Ready;
    PlaceAgainstWallAway => "place_against_wall_away", Tools, ToolsPlaceObjects, sc(ALT_SHIFT, Key::L), Ready;
    PlaceNextTo => "place_next_to", Tools, ToolsPlaceObjects, sc(ALT, Key::X), Ready;
    SettleFloating => "settle_floating", Tools, ToolsPlaceObjects, sc(ALT_SHIFT, Key::N), Ready;
    FitToGround => "fit_to_ground", Tools, ToolsPlaceObjects, sc(ALT, Key::G), Ready;
    ShowShapePreview => "show_shape_preview", Tools, ToolsPlaceObjects, sc(ALT_SHIFT, Key::M), Ready;
    ToggleRealDimensions => "toggle_real_dimensions", View, ViewDisplayInfo, sc(ALT, Key::M), Ready;
    ToolFoliage => "tool_foliage", Tools, ToolsScatter, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::CreatePrefab | Action::PlaceAgainstWall | Action::PlaceAgainstWallAway | Action::FitToGround => {
            Some(!state.doc.selection().objects.is_empty())
        }
        Action::PlaceOnTop | Action::PlaceNextTo => Some(state.doc.selection().objects.len() >= 2),
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
        Action::ToggleRealDimensions => Some(Check::Toggle(state.settings.placement.show_real_dimensions)),
        Action::ShowShapePreview => Some(Check::Toggle(state.runtime.placement.shape_preview_open)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::CreatePrefab => Some("➕"),
        Action::InsertOriginalPrefab => Some("⌖"),
        Action::ShowPrefabs => Some("⊞"),
        Action::PlaceOnTop => Some("⏶"),
        Action::PlaceAgainstWall | Action::PlaceAgainstWallAway => Some("▶"),
        Action::PlaceNextTo => Some("🔀"),
        Action::SettleFloating => Some("⬇"),
        Action::FitToGround => Some("⚖"),
        Action::ShowShapePreview => Some("▣"),
        Action::ToggleRealDimensions => Some("📏"),
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
    ("action.place_on_top", "Place on top of target", "Poser sur la cible"),
    ("action.place_against_wall", "Place against wall", "Plaquer contre le mur"),
    ("action.place_against_wall_away", "Place against wall, facing away", "Plaquer contre le mur, dos au mur"),
    ("action.place_next_to", "Place next to target", "Placer à côté de la cible"),
    ("action.settle_floating", "Settle floating objects", "Reposer les objets flottants"),
    ("action.fit_to_ground", "Fit to ground (supports)", "Ajuster au sol (appuis)"),
    ("action.show_shape_preview", "Shape preview", "Aperçu de la forme"),
    ("action.toggle_real_dimensions", "Real dimensions", "Dimensions réelles"),
    ("shell.undo.place_on_top", "Place on top", "Poser dessus"),
    ("shell.undo.place_against_wall", "Place against wall", "Plaquer contre le mur"),
    ("shell.undo.place_next_to", "Place next to", "Placer à côté"),
    ("shell.undo.settle_floating_objects", "Settle floating objects", "Reposer les objets flottants"),
    ("shell.undo.fit_to_ground", "Fit to ground", "Ajuster au sol"),
    ("placement.real.size", "real size {0} × {1} × {2}", "taille réelle {0} × {1} × {2}"),
    ("placement.real.many", "{0} objects, real size {1} × {2} × {3}", "{0} objets, taille réelle {1} × {2} × {3}"),
    ("placement.real.gap", "floats {0} above the floor", "flotte à {0} du sol"),
    ("placement.real.sunk", "sunk {0} into the floor", "enfoncé de {0} dans le sol"),
    ("placement.real.resting", "rests on the floor", "posé au sol"),
    ("placement.real.no_floor", "nothing below", "rien en dessous"),
    ("placement.real.overlap", "overlaps by {0}", "chevauche de {0}"),
    ("action.tool_foliage", "Foliage paint tool", "Outil de peinture de végétation"),
    ("tool.foliage", "Foliage", "Végétation"),
    ("shell.undo.foliage_paint", "Foliage paint", "Peinture de végétation"),
    ("shell.undo.foliage_erase", "Foliage erase", "Effacement de végétation"),
    ("shell.undo.detail_paint", "Detail paint", "Peinture des détails"),
    ("shell.undo.mcp_paint_foliage", "MCP: paint foliage", "MCP : peinture de végétation"),
    ("shell.undo.mcp_erase_foliage", "MCP: erase foliage", "MCP : effacement de végétation"),
    ("shell.undo.mcp_set_face_detail_type", "MCP: face detail type", "MCP : type de détail des faces"),
];
