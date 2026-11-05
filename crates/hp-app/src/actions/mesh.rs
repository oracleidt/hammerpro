use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;
use crate::tool::ToolId;

action_list! { mesh_actions {
    VertexSplitFace => "vertex_split_face", Tools, ToolsMesh, sc(CTRL, Key::F), Ready;
    VertexScale => "vertex_scale", Tools, ToolsMesh, sc(ALT, Key::E), Ready;
    VertexWeld => "vertex_weld", Tools, ToolsMesh, None, Ready;
    VertexWeldDistance => "vertex_weld_distance", Tools, ToolsMesh, None, Ready;
    VertexCleanup => "vertex_cleanup", Tools, ToolsMesh, None, Ready;
    EdgeSelectLoop => "edge_select_loop", Edit, EditSelectEdges, None, Ready;
    EdgeSelectRing => "edge_select_ring", Edit, EditSelectEdges, None, Ready;
    EdgeGrow => "edge_grow", Edit, EditSelectEdges, None, Ready;
    EdgeShrink => "edge_shrink", Edit, EditSelectEdges, None, Ready;
    EdgeSelectBoundary => "edge_select_boundary", Edit, EditSelectEdges, None, Ready;
    EdgeSlide => "edge_slide", Tools, ToolsMesh, None, Ready;
    VertexRotate => "vertex_rotate", Tools, ToolsMesh, None, Ready;
    DispSmoothLoop => "disp_smooth_loop", Tools, ToolsMesh, None, Ready;
    DispLoopHeight => "disp_loop_height", Tools, ToolsMesh, None, Ready;
    DispSubdivide => "disp_subdivide", Tools, ToolsMesh, None, Ready;
    DispInvertAlpha => "disp_invert_alpha", Tools, ToolsMesh, None, Ready;
    DispNoise => "disp_noise", Tools, ToolsMesh, None, Ready;
    DispFractalNoise => "disp_fractal_noise", Tools, ToolsMesh, None, Ready;
    DispThermalErosion => "disp_thermal_erosion", Tools, ToolsMesh, None, Ready;
    DispHydraulicErosion => "disp_hydraulic_erosion", Tools, ToolsMesh, None, Ready;
    DispToMesh => "disp_to_mesh", Tools, ToolsMesh, None, Ready;
    ImportHeightmap => "import_heightmap", Tools, ToolsMesh, None, Ready;
    ExportHeightmap => "export_heightmap", Tools, ToolsMesh, None, Ready;
    GenerateTerrain => "generate_terrain", Tools, ToolsMesh, None, Ready;
    MeshToDisp => "mesh_to_disp", Tools, ToolsMesh, None, Ready;
    DispConformMesh => "disp_conform_mesh", Tools, ToolsMesh, None, Ready;
    DispTileSelection => "disp_tile_selection", Tools, ToolsMesh, None, Ready;
    DispBlendHeights => "disp_blend_heights", Tools, ToolsMesh, None, Ready;
    DispLimitSlope => "disp_limit_slope", Tools, ToolsMesh, None, Ready;
    DispUniformSlope => "disp_uniform_slope", Tools, ToolsMesh, None, Ready;
    DispSelectBySlope => "disp_select_by_slope", Tools, ToolsMesh, None, Ready;
    ToggleUvLock => "toggle_uv_lock", Tools, ToolsMesh, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

fn has_selected_disps(state: &AppState) -> bool {
    let doc = &state.doc;
    let sel = doc.selection();
    let face = sel.faces.iter().any(|f| doc.face_side(*f).is_some_and(|s| s.disp.is_some()));
    face || sel.objects.iter().any(|o| match o {
        hp_editor::ObjRef::Solid(id) => doc.scene().solid(*id).is_some_and(|n| n.has_disp()),
        hp_editor::ObjRef::Entity(id) => {
            doc.scene().entity_solid_ids(*id).iter().any(|s| doc.scene().solid(*s).is_some_and(|n| n.has_disp()))
        }
    })
}

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::VertexSplitFace
        | Action::VertexScale
        | Action::VertexWeld
        | Action::VertexWeldDistance
        | Action::VertexCleanup
        | Action::VertexRotate
        | Action::EdgeSelectLoop
        | Action::EdgeSelectRing
        | Action::EdgeGrow
        | Action::EdgeShrink
        | Action::EdgeSelectBoundary
        | Action::EdgeSlide => Some(state.tool == ToolId::Vertex),
        Action::DispSmoothLoop | Action::DispLoopHeight => Some(state.tool == ToolId::Displacement),
        Action::DispSubdivide
        | Action::DispInvertAlpha
        | Action::DispNoise
        | Action::DispFractalNoise
        | Action::DispThermalErosion
        | Action::DispHydraulicErosion
        | Action::DispToMesh
        | Action::ExportHeightmap => Some(has_selected_disps(state)),
        Action::DispConformMesh => Some(has_selected_disps(state)),
        Action::DispTileSelection => {
            Some(!state.doc.selected_objects().is_empty() || !state.doc.selected_faces().is_empty())
        }
        Action::DispBlendHeights | Action::DispLimitSlope | Action::DispUniformSlope | Action::DispSelectBySlope => {
            Some(has_selected_disps(state))
        }
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::ToggleUvLock => Some(Check::Toggle(state.settings.mesh.uv_lock)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::VertexSplitFace => "✂",
        Action::VertexScale => "⛶",
        Action::VertexWeld => "✚",
        Action::VertexWeldDistance => "◎",
        Action::VertexCleanup => "✨",
        Action::EdgeSelectLoop => "○",
        Action::EdgeSelectRing => "⛓",
        Action::EdgeGrow => "⬈",
        Action::EdgeShrink => "⬋",
        Action::EdgeSelectBoundary => "◻",
        Action::EdgeSlide => "↔",
        Action::VertexRotate => "⟳",
        Action::DispSmoothLoop => "≈",
        Action::DispLoopHeight => "↕",
        Action::DispSubdivide => "⊞",
        Action::DispInvertAlpha => "◑",
        Action::DispNoise => "〰",
        Action::DispFractalNoise => "▓",
        Action::DispThermalErosion => "◕",
        Action::DispHydraulicErosion => "☔",
        Action::DispToMesh => "⬣",
        Action::ImportHeightmap => "📥",
        Action::ExportHeightmap => "📤",
        Action::GenerateTerrain => "🌄",
        Action::MeshToDisp => "🔺",
        Action::DispConformMesh => "🔻",
        Action::DispTileSelection => "■",
        Action::DispBlendHeights => "⬌",
        Action::DispLimitSlope => "↗",
        Action::DispUniformSlope => "↘",
        Action::DispSelectBySlope => "⬊",
        Action::ToggleUvLock => "🔒",
        _ => return None,
    })
}

pub(super) const TR: TrTable = &[
    ("action.vertex_split_face", "Split face", "Scinder la face"),
    ("action.vertex_scale", "Scale vertices...", "Mettre les sommets à l'échelle..."),
    ("action.vertex_weld", "Weld selected vertices", "Souder les sommets sélectionnés"),
    ("action.vertex_weld_distance", "Weld vertices by distance", "Souder les sommets par distance"),
    ("action.vertex_cleanup", "Remove degenerate faces and edges", "Supprimer les faces et arêtes dégénérées"),
    ("action.edge_select_loop", "Select edge loop", "Sélectionner la boucle d'arêtes"),
    ("action.edge_select_ring", "Select edge ring", "Sélectionner l'anneau d'arêtes"),
    ("action.edge_grow", "Grow selection", "Agrandir la sélection"),
    ("action.edge_shrink", "Shrink selection", "Réduire la sélection"),
    ("action.edge_select_boundary", "Select boundary", "Sélectionner le contour"),
    ("action.edge_slide", "Slide edge loop...", "Faire glisser la boucle d'arêtes..."),
    ("action.vertex_rotate", "Rotate vertices...", "Faire tourner les sommets..."),
    ("action.disp_smooth_loop", "Smooth displacement loop", "Lisser la boucle du displacement"),
    ("action.disp_loop_height", "Set height of displacement loop", "Fixer la hauteur de la boucle du displacement"),
    ("action.disp_subdivide", "Subdivide displacements", "Subdiviser les displacements"),
    ("action.disp_invert_alpha", "Invert displacement alpha", "Inverser l'alpha des displacements"),
    ("action.disp_noise", "Add noise to displacements...", "Ajouter du bruit aux displacements..."),
    ("action.disp_fractal_noise", "Fractal noise on displacements...", "Bruit fractal sur les displacements..."),
    ("action.disp_thermal_erosion", "Thermal erosion of displacements...", "Érosion thermique des displacements..."),
    (
        "action.disp_hydraulic_erosion",
        "Hydraulic erosion of displacements...",
        "Érosion hydraulique des displacements...",
    ),
    ("action.disp_to_mesh", "Displacement to mesh...", "Displacement en maillage..."),
    ("action.import_heightmap", "Import heightmap...", "Importer une carte de hauteur..."),
    ("action.export_heightmap", "Export heightmap...", "Exporter une carte de hauteur..."),
    ("action.generate_terrain", "Generate terrain...", "Générer un terrain..."),
    ("action.mesh_to_disp", "Mesh to displacement...", "Maillage en displacement..."),
    ("action.disp_conform_mesh", "Conform displacements to mesh...", "Plaquer les displacements sur un maillage..."),
    (
        "action.disp_tile_selection",
        "Tile selection with displacements...",
        "Daller la sélection avec des displacements...",
    ),
    ("action.disp_blend_heights", "Blend displacement heights", "Fusionner les hauteurs des displacements"),
    ("action.disp_limit_slope", "Limit displacement slope", "Limiter la pente des displacements"),
    ("action.disp_uniform_slope", "Make displacement slope uniform", "Uniformiser la pente des displacements"),
    ("action.disp_select_by_slope", "Select displacements by slope", "Sélectionner les displacements par pente"),
    ("shell.undo.blend_displacements", "Blend displacements", "Fusion des displacements"),
    ("shell.undo.limit_displacement_slope", "Limit displacement slope", "Limitation de la pente du displacement"),
    ("shell.undo.displacement_ramp", "Displacement ramp", "Rampe de displacement"),
    ("shell.undo.uniform_displacement_slope", "Uniform displacement slope", "Pente uniforme du displacement"),
    (
        "shell.undo.displacement_slope_along_a_path",
        "Displacement slope along a path",
        "Pente du displacement le long d'un chemin",
    ),
    ("shell.undo.smooth_displacement_slope", "Smooth displacement slope", "Adoucissement de la pente du displacement"),
    ("action.toggle_uv_lock", "UV lock (vertex tool)", "Verrouillage UV (outil sommets)"),
    ("shell.undo.mesh_to_displacement", "Mesh to displacement", "Maillage en displacement"),
    (
        "shell.undo.conform_displacements_to_mesh",
        "Conform displacements to mesh",
        "Plaquage des displacements sur un maillage",
    ),
    (
        "shell.undo.tile_displacements_from_selection",
        "Tile displacements from selection",
        "Dallage de la sélection en displacements",
    ),
    ("shell.undo.mcp_mesh_to_displacement", "MCP: mesh to displacement", "MCP : maillage en displacement"),
    (
        "shell.undo.mcp_conform_displacements_to_mesh",
        "MCP: conform displacements to mesh",
        "MCP : plaquage des displacements sur un maillage",
    ),
    (
        "shell.undo.mcp_tile_displacements_from_selection",
        "MCP: tile displacements from selection",
        "MCP : dallage de la sélection en displacements",
    ),
    ("shell.undo.invert_displacement_alpha", "Invert displacement alpha", "Inversion de l'alpha du displacement"),
    ("shell.undo.displacement_noise", "Displacement noise", "Bruit sur le displacement"),
    ("shell.undo.import_heightmap", "Import heightmap", "Import d'une carte de hauteur"),
    ("shell.undo.generate_terrain", "Generate terrain", "Génération de terrain"),
    ("shell.undo.scale_displacement", "Scale displacement", "Mise à l'échelle du displacement"),
    ("shell.undo.displacement_collision", "Displacement collision", "Collisions du displacement"),
    ("shell.undo.subdivide_displacements", "Subdivide displacements", "Subdivision des displacements"),
    ("shell.undo.rotate_vertices", "Rotate vertices", "Rotation des sommets"),
    ("shell.undo.slide_edge_loop", "Slide edge loop", "Glissement de la boucle d'arêtes"),
    ("shell.undo.move_edge_loop", "Move edge loop", "Déplacement de la boucle d'arêtes"),
    ("shell.undo.scale_edge_loop", "Scale edge loop", "Mise à l'échelle de la boucle d'arêtes"),
    ("shell.undo.rotate_edge_loop", "Rotate edge loop", "Rotation de la boucle d'arêtes"),
    ("shell.undo.smooth_displacement_loop", "Smooth displacement loop", "Lissage de la boucle du displacement"),
    (
        "shell.undo.set_displacement_loop_height",
        "Set height of displacement loop",
        "Hauteur de la boucle du displacement",
    ),
    (
        "shell.undo.mcp_smooth_displacement_loop",
        "MCP: smooth displacement loop",
        "MCP : lissage de la boucle du displacement",
    ),
    (
        "shell.undo.mcp_set_displacement_loop_height",
        "MCP: set height of displacement loop",
        "MCP : hauteur de la boucle du displacement",
    ),
    ("shell.undo.weld_vertices", "Weld vertices", "Soudure des sommets"),
    ("shell.undo.weld_vertices_by_distance", "Weld vertices by distance", "Soudure des sommets par distance"),
    ("shell.undo.clean_up_solids", "Clean up solids", "Nettoyage des solides"),
    ("shell.undo.mcp_move_edge_loop", "MCP: move edge loop", "MCP : déplacement de la boucle d'arêtes"),
    ("shell.undo.mcp_weld_brush_vertices", "MCP: weld vertices", "MCP : soudure des sommets"),
    ("shell.undo.scale_vertices", "Scale vertices", "Mise à l'échelle des sommets"),
    ("shell.undo.sculpt_displacement", "Sculpt displacement", "Sculpture du displacement"),
    ("shell.undo.tag_displacement", "Tag displacement", "Marquage du displacement"),
    ("shell.undo.displacement_shadows", "Displacement shadows", "Ombres du displacement"),
    ("shell.undo.displacement_lightmap_scale", "Displacement lightmap scale", "Échelle de lightmap du displacement"),
];
