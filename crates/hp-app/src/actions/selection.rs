use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { selection_actions {
    MoveToFloor => "move_to_floor", Tools, ToolsBrush, sc(ALT, Key::N), Ready;
    MoveToCeiling => "move_to_ceiling", Tools, ToolsBrush, sc(ALT, Key::U), Ready;
    SnapToGridIndividually => "snap_to_grid_individually", Tools, ToolsBrush, sc(CTRL_SHIFT, Key::B), Ready;
    CenterOrigins => "center_origins", Tools, ToolsBrush, None, Ready;
    TogglePivot => "toggle_pivot", Tools, ToolsBrush, None, Ready;
    FlipHorizontal => "flip_horizontal", Tools, ToolsFlip, sc(CTRL, Key::L), Ready;
    FlipVertical => "flip_vertical", Tools, ToolsFlip, None, Ready;
    AlignCenterHorizontal => "align_center_horizontal", Tools, ToolsAlign, None, Ready;
    AlignCenterVertical => "align_center_vertical", Tools, ToolsAlign, None, Ready;
    ToggleGizmoLocal => "toggle_gizmo_local", Edit, Hidden, sc(CTRL, Key::J), Ready;
    MoveToFloorAligned => "move_to_floor_aligned", Tools, ToolsBrush, sc(CTRL_ALT, Key::N), Ready;
    MoveToCeilingAligned => "move_to_ceiling_aligned", Tools, ToolsBrush, sc(CTRL_ALT, Key::U), Ready;
    RandomRotate => "random_rotate", Tools, ToolsBrush, None, Ready;
    RandomRotateDialog => "random_rotate_dialog", Tools, ToolsBrush, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    let objects = !state.doc.selection().objects.is_empty();
    match action {
        Action::MoveToFloor
        | Action::MoveToCeiling
        | Action::MoveToFloorAligned
        | Action::MoveToCeilingAligned
        | Action::SnapToGridIndividually
        | Action::RandomRotate
        | Action::RandomRotateDialog
        | Action::FlipHorizontal
        | Action::FlipVertical => Some(objects),
        Action::AlignCenterHorizontal | Action::AlignCenterVertical => Some(state.doc.selection().objects.len() >= 2),
        Action::CenterOrigins => Some(state.doc.selection().objects.iter().any(|o| match o {
            hp_editor::ObjRef::Entity(id) => state.doc.scene().is_brush_entity(*id),
            hp_editor::ObjRef::Solid(id) => state.doc.scene().object_of_solid(*id).is_entity(),
        })),
        Action::TogglePivot => Some(objects || state.pivot.is_some()),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::TogglePivot => Some(Check::Toggle(state.pivot.is_some())),
        Action::ToggleGizmoLocal => Some(Check::Toggle(state.settings.selection.gizmo_local)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::MoveToFloor | Action::MoveToFloorAligned => "⬇",
        Action::MoveToCeiling | Action::MoveToCeilingAligned => "⬆",
        Action::SnapToGridIndividually => "⊞",
        Action::CenterOrigins => "⌖",
        Action::TogglePivot => "✚",
        Action::FlipHorizontal => "↔",
        Action::FlipVertical => "↕",
        Action::AlignCenterHorizontal | Action::AlignCenterVertical => "⊟",
        Action::ToggleGizmoLocal => "⟲",
        Action::RandomRotate | Action::RandomRotateDialog => "🎲",
        _ => return None,
    })
}

pub(super) const TR: TrTable = &[
    ("action.move_to_floor", "Move to floor", "Poser au sol"),
    ("action.move_to_ceiling", "Move to ceiling", "Coller au plafond"),
    ("action.move_to_floor_aligned", "Move to floor, aligned to the surface", "Poser au sol, aligné sur la surface"),
    (
        "action.move_to_ceiling_aligned",
        "Move to ceiling, aligned to the surface",
        "Coller au plafond, aligné sur la surface",
    ),
    ("action.snap_to_grid_individually", "Snap each object to grid", "Aligner chaque objet sur la grille"),
    ("action.center_origins", "Center origins", "Centrer les origines"),
    ("action.toggle_pivot", "Movable pivot", "Pivot déplaçable"),
    ("action.flip_horizontal", "Flip horizontally", "Retourner horizontalement"),
    ("action.flip_vertical", "Flip vertically", "Retourner verticalement"),
    ("action.align_center_horizontal", "Align centers horizontally", "Centrer horizontalement"),
    ("action.align_center_vertical", "Align centers vertically", "Centrer verticalement"),
    ("action.toggle_gizmo_local", "Gizmo in local space", "Gizmo en repère local"),
    ("action.random_rotate", "Random Z rotation", "Rotation Z aléatoire"),
    ("action.random_rotate_dialog", "Random Z rotation options...", "Options de rotation Z aléatoire..."),
    ("shell.undo.random_rotation", "Random rotation", "Rotation aléatoire"),
    ("shell.undo.move_to_floor", "Move to floor", "Poser au sol"),
    ("shell.undo.move_to_ceiling", "Move to ceiling", "Coller au plafond"),
    ("shell.undo.center_origins", "Center origins", "Centrer les origines"),
    ("shell.undo.add_origins", "Add origins", "Ajouter les origines"),
    ("shell.undo.scale_models", "Scale models", "Mettre les modèles à l'échelle"),
    ("shell.undo.stretch", "Stretch", "Étirement"),
];
