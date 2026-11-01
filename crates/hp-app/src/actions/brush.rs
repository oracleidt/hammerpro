use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { brush_actions {
    ToolPolygon => "tool_polygon", Tools, ToolsSwitch, sc(SHIFT, Key::N), Ready;
    ToolCurve => "tool_curve", Tools, ToolsSwitch, sc(SHIFT, Key::U), Ready;
    ToolExtrude => "tool_extrude", Tools, ToolsSwitch, sc(SHIFT, Key::I), Ready;
    MergeBrushes => "merge_brushes", Tools, ToolsBrush, sc(CTRL_SHIFT, Key::M), Ready;
    MakeHintBrush => "make_hint_brush", Tools, Hidden, None, Ready;
    NodrawHiddenFaces => "nodraw_hidden_faces", Tools, ToolsBrush, None, Ready;
    TieEachToEntity => "tie_each_to_entity", Tools, ToolsBrush, sc(CTRL_SHIFT, Key::T), Ready;
    ConvertToDetail => "convert_to_detail", Tools, ToolsBrush, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    let sel = state.doc.selection();
    Some(match action {
        Action::MergeBrushes | Action::TieEachToEntity | Action::ConvertToDetail => !sel.objects.is_empty(),
        Action::MakeHintBrush => !sel.faces.is_empty(),
        _ => return None,
    })
}

pub(super) fn check(_state: &AppState, _action: Action) -> Option<Check> {
    None
}

pub(super) fn icon(_action: Action) -> Option<&'static str> {
    None
}

pub(super) const TR: TrTable = &[
    ("action.tool_polygon", "Polygon tool", "Outil polygone"),
    ("action.merge_brushes", "Merge brushes", "Fusionner les brushes"),
    ("action.make_hint_brush", "Make hint brush", "Créer un brush hint"),
    ("action.nodraw_hidden_faces", "Nodraw on hidden faces...", "Nodraw sur les faces cachées..."),
    ("action.tie_each_to_entity", "Tie each to its own entity", "Lier chacun à sa propre entité"),
    ("action.convert_to_detail", "Convert to func_detail", "Convertir en func_detail"),
    ("tool.polygon", "Polygon", "Polygone"),
    ("action.tool_extrude", "Extrude / carve tool", "Outil extruder / creuser"),
    ("tool.extrude", "Extrude / carve", "Extruder / creuser"),
    ("action.tool_curve", "Curve tool", "Outil courbe"),
    ("tool.curve", "Curve", "Courbe"),
    ("shell.undo.tie_each_to_entity", "Tie each to entity", "Liaison de chaque brush à une entité"),
    ("shell.undo.make_hint_brush", "Make hint brush", "Création de brush hint"),
    ("shell.undo.carve_face", "Carve face", "Creusement de face"),
    ("shell.undo.extrude_face", "Extrude face", "Extrusion de face"),
    ("shell.undo.nodraw_hidden_faces", "Nodraw hidden faces", "Nodraw sur les faces cachées"),
];
