use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { brush_actions {
    ToolPolygon => "tool_polygon", Tools, ToolsSwitch, sc(SHIFT, Key::N), Ready;
    ToolCurve => "tool_curve", Tools, ToolsSwitch, sc(SHIFT, Key::U), Ready;
    ToolExtrude => "tool_extrude", Tools, ToolsSwitch, sc(SHIFT, Key::I), Ready;
    ToolBevel => "tool_bevel", Tools, ToolsSwitch, sc(SHIFT, Key::J), Ready;
    MergeBrushes => "merge_brushes", Tools, ToolsBrushOps, sc(CTRL_SHIFT, Key::M), Ready;
    IntersectBrushes => "intersect_brushes", Tools, ToolsBrushOps, sc(CTRL_SHIFT, Key::K), Ready;
    UnionBrushes => "union_brushes", Tools, ToolsBrushOps, None, Ready;
    MakeHintBrush => "make_hint_brush", Tools, Hidden, None, Ready;
    NodrawHiddenFaces => "nodraw_hidden_faces", Tools, ToolsBrushMisc, None, Ready;
    TieEachToEntity => "tie_each_to_entity", Tools, ToolsBrushMisc, sc(CTRL_SHIFT, Key::T), Ready;
    ConvertToDetail => "convert_to_detail", Tools, ToolsBrushMisc, sc(CTRL_ALT, Key::D), Ready;
    SelectDetailCandidates => "select_detail_candidates", Tools, ToolsBrushMisc, None, Ready;
    ToggleDetailTint => "toggle_detail_tint", View, ViewDisplay, None, Ready;
    CurveEdit => "curve_edit", Tools, ToolsCurves, None, Ready;
    ToolRoad => "tool_road", Tools, ToolsSwitch, None, Ready;
    ToolPipe => "tool_pipe", Tools, ToolsSwitch, None, Ready;
    ToolSlice => "tool_slice", Tools, ToolsSwitch, sc(SHIFT, Key::Y), Ready;
    RoadEdit => "road_edit", Tools, ToolsRoads, None, Ready;
    InsetFace => "inset_face", Tools, ToolsFaceOps, None, Ready;
    InsetInteractive => "inset_interactive", Tools, ToolsFaceOps, sc(NONE, Key::I), Ready;
    ExtrudeInteractive => "extrude_interactive", Tools, ToolsFaceOps, sc(NONE, Key::E), Ready;
    RoadSidewalk => "road_sidewalk", Tools, ToolsRoads, None, Ready;
    RoadCurb => "road_curb", Tools, ToolsRoads, None, Ready;
    RoadTerrain => "road_terrain", Tools, ToolsRoads, None, Ready;
    RoadsideEdit => "roadside_edit", Tools, ToolsRoads, None, Ready;
    ToolSpline => "tool_spline", Tools, ToolsCurves, None, Ready;
    SplineEdit => "spline_edit", Tools, ToolsCurves, None, Ready;
    SplineBake => "spline_bake", Tools, ToolsCurves, None, Ready;
    ExtrudeAlongNormal => "extrude_along_normal", Tools, ToolsFaceOps, sc(ALT_SHIFT, Key::E), Ready;
    ExtrudeCycleDirection => "extrude_cycle_direction", Tools, ToolsFaceOps, None, Ready;
    ToggleExtrudeGrow => "toggle_extrude_grow", Tools, ToolsFaceOps, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    let sel = state.doc.selection();
    Some(match action {
        Action::MergeBrushes
        | Action::IntersectBrushes
        | Action::UnionBrushes
        | Action::TieEachToEntity
        | Action::ConvertToDetail => !sel.objects.is_empty(),
        Action::MakeHintBrush | Action::InsetFace => !sel.faces.is_empty(),
        Action::RoadsideEdit => state.doc.selected_roadside_piece().is_some(),
        Action::SplineEdit | Action::SplineBake => !sel.objects.is_empty(),
        _ => return None,
    })
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::ToggleDetailTint => Some(Check::Toggle(state.settings.brush.detail.tint)),
        Action::ToggleExtrudeGrow => Some(Check::Toggle(state.settings.brush.extrude_grow)),
        _ => None,
    }
}

pub(super) fn icon(_action: Action) -> Option<&'static str> {
    None
}

pub(super) const TR: TrTable = &[
    ("action.tool_polygon", "Polygon tool", "Outil polygone"),
    ("action.merge_brushes", "Merge brushes", "Fusionner les brushes"),
    ("action.intersect_brushes", "Intersect brushes", "Intersection de brushes"),
    ("action.union_brushes", "Union of brushes", "Union de brushes"),
    ("action.make_hint_brush", "Make hint brush", "Créer un brush hint"),
    ("action.nodraw_hidden_faces", "Nodraw on hidden faces...", "Nodraw sur les faces cachées..."),
    ("action.tie_each_to_entity", "Tie each to its own entity", "Lier chacun à sa propre entité"),
    ("action.inset_face", "Inset face...", "Décaler la face..."),
    ("action.inset_interactive", "Interactive inset (I)", "Décalage interactif (I)"),
    ("action.extrude_interactive", "Interactive extrude (E)", "Extrusion interactive (E)"),
    ("action.extrude_along_normal", "Extrude along normal", "Extruder selon la normale"),
    (
        "action.extrude_cycle_direction",
        "Extrude direction: next (each normal, average, axis)",
        "Direction d'extrusion : suivante (chaque normale, moyenne, axe)",
    ),
    ("action.toggle_extrude_grow", "Extrude grows the brush", "L'extrusion agrandit le brush"),
    ("action.convert_to_detail", "Make func_detail", "Transformer en func_detail"),
    (
        "action.select_detail_candidates",
        "Select func_detail candidates...",
        "Sélectionner les candidats func_detail...",
    ),
    ("action.toggle_detail_tint", "Tint func_detail brushes", "Teinter les brushes func_detail"),
    ("action.road_sidewalk", "Sidewalk generator...", "Générateur de trottoirs..."),
    ("action.road_curb", "Curb and gutter generator...", "Générateur de bordures et caniveaux..."),
    ("action.road_terrain", "Blend terrain to road...", "Fondre le terrain sur la route..."),
    ("action.roadside_edit", "Edit road details...", "Modifier les détails de route..."),
    ("shell.undo.create_sidewalk", "Create sidewalk", "Création de trottoirs"),
    ("shell.undo.create_curb", "Create curb and gutter", "Création de bordures et caniveaux"),
    ("shell.undo.edit_roadside", "Edit road details", "Modification des détails de route"),
    ("shell.undo.tag_roadside", "Tag road details", "Marquage des détails de route"),
    ("shell.undo.blend_terrain_to_road", "Blend terrain to road", "Fusion du terrain sur la route"),
    ("tool.polygon", "Polygon", "Polygone"),
    ("action.tool_slice", "Slice tool", "Outil tranche"),
    ("tool.slice", "Slice", "Tranche"),
    ("shell.undo.slice", "Slice", "Tranche"),
    ("action.tool_extrude", "Extrude / carve tool", "Outil extruder / creuser"),
    ("tool.extrude", "Extrude / carve", "Extruder / creuser"),
    ("action.tool_bevel", "Bevel tool", "Outil biseau"),
    ("tool.bevel", "Bevel", "Biseau"),
    ("shell.undo.bevel_edges", "Bevel edges", "Biseautage d'arêtes"),
    ("action.tool_curve", "Curve tool", "Outil courbe"),
    ("action.curve_edit", "Edit curve", "Modifier la courbe"),
    ("shell.undo.edit_curve", "Edit curve", "Modification de courbe"),
    ("shell.undo.create_curve", "Create curve", "Création de courbe"),
    ("shell.undo.tag_curve", "Tag curve", "Marquage de courbe"),
    (
        "curve.edit_need",
        "Select a brush made by the curve tool to edit its curve",
        "Sélectionnez un brush créé par l'outil courbe pour modifier sa courbe",
    ),
    ("tool.curve", "Curve", "Courbe"),
    ("action.tool_road", "Road tool", "Outil route"),
    ("action.road_edit", "Edit road", "Modifier la route"),
    ("tool.road", "Road", "Route"),
    ("action.tool_pipe", "Pipe tool", "Outil tuyau"),
    ("tool.pipe", "Pipe", "Tuyau"),
    ("action.tool_spline", "Spline tool", "Outil spline"),
    ("tool.spline", "Spline", "Spline"),
    ("action.spline_edit", "Edit spline", "Modifier la spline"),
    ("action.spline_bake", "Bake spline to plain objects", "Figer la spline en objets ordinaires"),
    ("shell.undo.create_spline", "Create spline", "Création de spline"),
    ("shell.undo.edit_spline", "Edit spline", "Modification de spline"),
    ("shell.undo.bake_spline", "Bake spline", "Figeage de spline"),
    ("shell.undo.mcp_create_spline", "MCP: create spline", "MCP : création de spline"),
    ("shell.undo.mcp_edit_spline", "MCP: edit spline", "MCP : modification de spline"),
    ("shell.undo.mcp_generate_along_spline", "MCP: generate along spline", "MCP : génération le long de la spline"),
    ("shell.undo.mcp_bake_spline", "MCP: bake spline", "MCP : figeage de spline"),
    ("shell.undo.create_pipe", "Create pipe", "Création de tuyau"),
    ("shell.undo.mcp_create_pipe", "MCP: create pipe", "MCP : création de tuyau"),
    ("shell.undo.create_road", "Create road", "Création de route"),
    ("shell.undo.edit_road", "Edit road", "Modification de route"),
    ("shell.undo.delete_road", "Delete road", "Suppression de route"),
    ("shell.undo.split_road", "Split road", "Découpe de route"),
    ("shell.undo.mcp_create_road", "MCP: create road", "MCP : création de route"),
    ("shell.undo.mcp_edit_road", "MCP: edit road", "MCP : modification de route"),
    ("shell.undo.mcp_delete_road", "MCP: delete road", "MCP : suppression de route"),
    ("shell.undo.mcp_split_road", "MCP: split road", "MCP : découpe de route"),
    ("shell.undo.mcp_add_road_junction", "MCP: road junction", "MCP : jonction de route"),
    ("shell.undo.make_func_detail", "Make func_detail", "Transformation en func_detail"),
    ("shell.undo.mcp_make_func_detail", "MCP: make func_detail", "MCP : transformation en func_detail"),
    ("shell.undo.tie_each_to_entity", "Tie each to entity", "Liaison de chaque brush à une entité"),
    ("shell.undo.make_hint_brush", "Make hint brush", "Création de brush hint"),
    ("shell.undo.carve_face", "Carve face", "Creusement de face"),
    ("shell.undo.inset_face", "Inset face", "Décalage de face"),
    ("shell.undo.resample_displacement_power", "Resample displacement power", "Rééchantillonnage de la puissance"),
    ("shell.undo.intersect", "Intersect", "Intersection"),
    ("shell.undo.union", "Union", "Union"),
    ("shell.undo.thermal_erosion", "Thermal erosion", "Érosion thermique"),
    ("shell.undo.hydraulic_erosion", "Hydraulic erosion", "Érosion hydraulique"),
    (
        "shell.undo.set_height_of_displacement_loop",
        "Set height of displacement loop",
        "Hauteur d'une boucle de displacement",
    ),
    ("shell.undo.extrude_face", "Extrude face", "Extrusion de face"),
    ("shell.undo.nodraw_hidden_faces", "Nodraw hidden faces", "Nodraw sur les faces cachées"),
];
