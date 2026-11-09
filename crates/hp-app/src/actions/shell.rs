use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::settings::{RenderModeSetting, SelectElement, SelectModeSetting};
use crate::state::AppState;

action_list! { shell_actions {
    NewMap => "new_map", File, FileNew, sc(CTRL, Key::N), Ready;
    OpenMap => "open_map", File, FileNew, sc(CTRL, Key::O), Ready;
    SaveMap => "save_map", File, FileSave, sc(CTRL, Key::S), Ready;
    SaveMapAs => "save_map_as", File, FileSave, sc(CTRL_SHIFT, Key::S), Ready;
    ImportMap => "import_map", File, FileImport, None, Ready;
    ExportSelection => "export_selection", File, FileImport, None, Ready;
    RunMap => "run_map", File, FileRun, sc(CTRL, Key::F9), Ready;
    Exit => "exit", File, FileExit, None, Ready;

    Undo => "undo", Edit, EditHistory, sc(CTRL, Key::Z), Ready;
    Redo => "redo", Edit, EditHistory, sc(CTRL, Key::Y), Ready;
    Cut => "cut", Edit, EditClipboard, sc(CTRL, Key::X), Ready;
    Copy => "copy", Edit, EditClipboard, sc(CTRL, Key::C), Ready;
    Paste => "paste", Edit, EditClipboard, sc(CTRL, Key::V), Ready;
    PasteSpecial => "paste_special", Edit, EditClipboard, sc(CTRL_SHIFT, Key::V), Ready;
    Delete => "delete", Edit, EditClipboard, sc(NONE, Key::Delete), Ready;
    Duplicate => "duplicate", Edit, EditClipboard, sc(CTRL, Key::D), Ready;
    ArrayDuplicate => "array_duplicate", Edit, EditClipboard, sc(CTRL_SHIFT, Key::D), Ready;
    SelectAll => "select_all", Edit, EditSelect, sc(CTRL, Key::A), Ready;
    SelectNone => "select_none", Edit, EditSelect, sc(CTRL_SHIFT, Key::A), Ready;
    InvertSelection => "invert_selection", Edit, EditSelect, sc(CTRL, Key::I), Ready;
    SelectModeGroups => "select_mode_groups", Edit, EditSelectMode, None, Ready;
    SelectModeObjects => "select_mode_objects", Edit, EditSelectMode, None, Ready;
    SelectModeSolids => "select_mode_solids", Edit, EditSelectMode, None, Ready;
    SelectElementObject => "select_element_object", Edit, EditSelectElement, sc(SHIFT, Key::Num4), Ready;
    SelectElementVertex => "select_element_vertex", Edit, EditSelectElement, sc(SHIFT, Key::Num1), Ready;
    SelectElementEdge => "select_element_edge", Edit, EditSelectElement, sc(SHIFT, Key::Num2), Ready;
    SelectElementFace => "select_element_face", Edit, EditSelectElement, sc(SHIFT, Key::Num3), Ready;
    FindEntities => "find_entities", Edit, EditFind, sc(CTRL_SHIFT, Key::F), Ready;
    ReplaceTextures => "replace_textures", Tools, ToolsBrush, sc(CTRL_SHIFT, Key::R), Ready;
    Properties => "properties", Edit, EditProperties, sc(ALT, Key::Enter), Ready;
    CommandPalette => "command_palette", Tools, ToolsApp, sc(CTRL, Key::P), Ready;
    Settings => "settings", Tools, ToolsApp, sc(CTRL, Key::Comma), Ready;

    Group => "group", Tools, ToolsBrush, sc(CTRL, Key::G), Ready;
    Ungroup => "ungroup", Tools, ToolsBrush, sc(CTRL, Key::U), Ready;
    TieToEntity => "tie_to_entity", Tools, ToolsBrush, sc(CTRL, Key::T), Ready;
    MoveToWorld => "move_to_world", Tools, ToolsBrush, sc(CTRL_SHIFT, Key::W), Ready;
    HideSelected => "hide_selected", View, ViewHide, sc(NONE, Key::H), Ready;
    HideUnselected => "hide_unselected", View, ViewHide, sc(CTRL_SHIFT, Key::H), Ready;
    UnhideAll => "unhide_all", View, ViewHide, sc(NONE, Key::U), Ready;
    Carve => "carve", Tools, ToolsBrush, sc(CTRL_SHIFT, Key::C), Ready;
    MakeHollow => "make_hollow", Tools, ToolsBrush, sc(CTRL, Key::H), Ready;
    MergeConvex => "merge_convex", Tools, ToolsBrush, None, Ready;
    SnapToGrid => "snap_to_grid", Tools, ToolsBrush, sc(CTRL, Key::B), Ready;
    TransformDialog => "transform_dialog", Tools, ToolsBrush, sc(CTRL, Key::M), Ready;
    FlipX => "flip_x", Tools, ToolsFlip, None, Ready;
    FlipY => "flip_y", Tools, ToolsFlip, None, Ready;
    FlipZ => "flip_z", Tools, ToolsFlip, None, Ready;
    AlignLeft => "align_left", Tools, ToolsAlign, None, Ready;
    AlignRight => "align_right", Tools, ToolsAlign, None, Ready;
    AlignTop => "align_top", Tools, ToolsAlign, None, Ready;
    AlignBottom => "align_bottom", Tools, ToolsAlign, None, Ready;
    CheckProblems => "check_problems", Map, MapInfo, sc(ALT, Key::P), Ready;
    MapProperties => "map_properties", Map, MapProperties, None, Ready;
    EntityReport => "entity_report", Map, MapInfo, None, Ready;
    MapInfo => "map_info", Map, MapProperties, None, Ready;
    GoToBrush => "go_to_brush", Map, MapInfo, sc(CTRL_SHIFT, Key::G), Ready;
    LoadPointfile => "load_pointfile", Map, MapPointfile, None, Ready;
    UnloadPointfile => "unload_pointfile", Map, MapPointfile, None, Ready;

    GridSmaller => "grid_smaller", Map, MapGrid, sc(NONE, Key::OpenBracket), Ready;
    GridLarger => "grid_larger", Map, MapGrid, sc(NONE, Key::CloseBracket), Ready;
    ToggleGrid => "toggle_grid", Map, MapGrid, sc(SHIFT, Key::G), Ready;
    ToggleSnap => "toggle_snap", Map, MapGrid, sc(SHIFT, Key::W), Ready;
    ToggleTextureLock => "toggle_texture_lock", Tools, ToolsBrush, sc(SHIFT, Key::L), Ready;
    ToggleTextureScaleLock => "toggle_texture_scale_lock", Tools, ToolsBrush, sc(SHIFT, Key::K), Ready;
    Center2dOnSelection => "center_2d", View, ViewNav, sc(CTRL, Key::E), Ready;
    Center3dOnSelection => "center_3d", View, ViewNav, sc(CTRL_SHIFT, Key::E), Ready;
    MaximizeView => "maximize_view", View, ViewNav, sc(SHIFT, Key::Z), Ready;
    ToggleFullscreen => "toggle_fullscreen", View, ViewNav, sc(NONE, Key::F11), Ready;
    RenderTextured => "render_textured", View, ViewRender, sc(ALT, Key::Num1), Ready;
    RenderFlat => "render_flat", View, ViewRender, sc(ALT, Key::Num2), Ready;
    RenderWireframe => "render_wireframe", View, ViewRender, sc(ALT, Key::Num3), Ready;
    RenderTexturedWire => "render_textured_wire", View, ViewRender, sc(ALT, Key::Num4), Ready;
    RenderLit => "render_lit", View, ViewRender, sc(ALT, Key::Num5), Ready;
    ToggleModels => "toggle_models", View, ViewShow, None, Ready;
    ToggleHelpers => "toggle_helpers", View, ViewShow, None, Ready;
    ToggleToolTextures => "toggle_tool_textures", View, ViewShow, None, Ready;
    ShowTextureBrowser => "show_texture_browser", View, ViewPanels, None, Ready;
    ShowModelBrowser => "show_model_browser", View, ViewPanels, None, Ready;
    ShowEntityProperties => "show_entity_properties", View, ViewPanels, None, Ready;
    ShowVisgroups => "show_visgroups", View, ViewPanels, None, Ready;
    ShowOutliner => "show_outliner", View, ViewPanels, None, Ready;
    ShowConsole => "show_console", Window, WindowMessages, sc(NONE, Key::Backtick), Ready;

    ToolSelect => "tool_select", Tools, ToolsSwitch, sc(SHIFT, Key::S), Ready;
    ToolCamera => "tool_camera", Tools, ToolsSwitch, sc(SHIFT, Key::C), Ready;
    ToolEntity => "tool_entity", Tools, ToolsSwitch, sc(SHIFT, Key::E), Ready;
    ToolBlock => "tool_block", Tools, ToolsSwitch, sc(SHIFT, Key::B), Ready;
    ToolTexture => "tool_texture", Tools, ToolsSwitch, sc(SHIFT, Key::A), Ready;
    ApplyCurrentTexture => "apply_current_texture", Tools, ToolsSwitch, sc(SHIFT, Key::T), Ready;
    ToolDecal => "tool_decal", Tools, ToolsSwitch, sc(SHIFT, Key::D), Ready;
    ToolOverlay => "tool_overlay", Tools, ToolsSwitch, sc(SHIFT, Key::O), Ready;
    ToolClip => "tool_clip", Tools, ToolsSwitch, sc(SHIFT, Key::X), Ready;
    ToolVertex => "tool_vertex", Tools, ToolsSwitch, sc(SHIFT, Key::V), Ready;
    ToolDisplacement => "tool_displacement", Tools, ToolsSwitch, sc(SHIFT, Key::P), Ready;

    KeyboardShortcuts => "keyboard_shortcuts", Help, HelpDocs, sc(NONE, Key::F1), Ready;
    OpenDocs => "open_docs", Help, HelpDocs, None, Ready;
    ReportIssue => "report_issue", Help, HelpDocs, None, Ready;
    About => "about", Help, HelpAbout, None, Ready;
    FaceSelectionBack => "face_selection_back", Edit, EditSelect, sc(ALT, Key::Z), Ready;
    FaceSelectionForward => "face_selection_forward", Edit, EditSelect, sc(ALT, Key::Y), Ready;
}}

pub(super) const REPEATS: &[Action] = &[Action::Undo, Action::Redo, Action::GridSmaller, Action::GridLarger];

pub(super) const TR: TrTable = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    let sel = state.doc.selection();
    let has_objects = !sel.objects.is_empty();
    let has_any = !sel.is_empty();
    Some(match action {
        Action::Undo if state.runtime.collaboration.connected && state.runtime.collaboration.active_map => {
            state.runtime.collaboration.can_undo
        }
        Action::Redo if state.runtime.collaboration.connected && state.runtime.collaboration.active_map => {
            state.runtime.collaboration.can_redo
        }
        Action::Undo => state
            .runtime
            .brush
            .road_history
            .filter(|_| state.tool == crate::ToolId::Road)
            .map_or_else(|| state.doc.can_undo(), |h| h.0),
        Action::Redo => state
            .runtime
            .brush
            .road_history
            .filter(|_| state.tool == crate::ToolId::Road)
            .map_or_else(|| state.doc.can_redo(), |h| h.1),
        Action::Cut
        | Action::Copy
        | Action::Delete
        | Action::Duplicate
        | Action::ArrayDuplicate
        | Action::ExportSelection
        | Action::Group
        | Action::TieToEntity
        | Action::MoveToWorld
        | Action::HideSelected
        | Action::Carve
        | Action::MakeHollow
        | Action::MergeConvex
        | Action::SnapToGrid
        | Action::TransformDialog
        | Action::FlipX
        | Action::FlipY
        | Action::FlipZ
        | Action::AlignLeft
        | Action::AlignRight
        | Action::AlignTop
        | Action::AlignBottom
        | Action::Ungroup => has_objects,
        Action::SelectNone
        | Action::HideUnselected
        | Action::Center2dOnSelection
        | Action::Center3dOnSelection
        | Action::ApplyCurrentTexture => has_any,
        Action::UnloadPointfile => state.pointfile.is_some(),
        _ => return None,
    })
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    let s = &state.settings;
    Some(match action {
        Action::ToggleGrid => Check::Toggle(s.edit.show_grid),
        Action::ToggleSnap => Check::Toggle(s.edit.snap_to_grid),
        Action::ToggleTextureLock => Check::Toggle(s.edit.texture_lock),
        Action::ToggleTextureScaleLock => Check::Toggle(s.edit.texture_scale_lock),
        Action::ToggleModels => Check::Toggle(s.view.show_models),
        Action::ToggleHelpers => Check::Toggle(s.view.show_helpers),
        Action::ToggleToolTextures => Check::Toggle(s.view.show_tool_textures),
        Action::MaximizeView => Check::Toggle(state.ui.maximized_view.is_some()),
        Action::ToggleFullscreen => Check::Toggle(state.ui.fullscreen),
        Action::RenderTextured => Check::Radio(s.view.render_mode == RenderModeSetting::Textured),
        Action::RenderFlat => Check::Radio(s.view.render_mode == RenderModeSetting::Flat),
        Action::RenderWireframe => Check::Radio(s.view.render_mode == RenderModeSetting::Wireframe),
        Action::RenderTexturedWire => Check::Radio(s.view.render_mode == RenderModeSetting::TexturedWire),
        Action::RenderLit => Check::Radio(s.view.render_mode == RenderModeSetting::Lit),
        Action::SelectModeGroups => Check::Radio(s.edit.select_mode == SelectModeSetting::Groups),
        Action::SelectModeObjects => Check::Radio(s.edit.select_mode == SelectModeSetting::Objects),
        Action::SelectModeSolids => Check::Radio(s.edit.select_mode == SelectModeSetting::Solids),
        Action::SelectElementObject => Check::Radio(s.selection.element == SelectElement::Object),
        Action::SelectElementVertex => Check::Radio(s.selection.element == SelectElement::Vertex),
        Action::SelectElementEdge => Check::Radio(s.selection.element == SelectElement::Edge),
        Action::SelectElementFace => Check::Radio(s.selection.element == SelectElement::Face),
        Action::ShowConsole => Check::Toggle(s.ui.show_console),
        Action::ShowTextureBrowser => Check::Toggle(s.ui.show_texture_browser),
        Action::ShowModelBrowser => Check::Toggle(s.ui.show_model_browser),
        Action::ShowEntityProperties => Check::Toggle(s.ui.show_entity_properties),
        Action::ShowVisgroups => Check::Toggle(s.ui.show_visgroups),
        Action::ShowOutliner => Check::Toggle(s.ui.show_outliner),
        _ => return None,
    })
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::NewMap => "🗋",
        Action::OpenMap => "🗁",
        Action::SaveMap | Action::SaveMapAs => "💾",
        Action::RunMap => "▶",
        Action::Exit => "🚪",
        Action::Undo => "⟲",
        Action::Redo => "⟳",
        Action::Cut => "✂",
        Action::Copy => "🗐",
        Action::Paste | Action::PasteSpecial => "📋",
        Action::Delete => "🗑",
        Action::FindEntities | Action::ReplaceTextures => "🔍",
        Action::Settings => "⚙",
        Action::CommandPalette => "⌘",
        Action::CheckProblems => "⚠",
        Action::ToggleGrid => "#",
        Action::ToggleSnap => "⊞",
        Action::ToggleTextureLock | Action::ToggleTextureScaleLock => "🔒",
        Action::HideSelected | Action::HideUnselected => "👁",
        Action::UnhideAll => "👁",
        Action::Group | Action::Ungroup => "🗀",
        Action::KeyboardShortcuts => "⌨",
        Action::About | Action::OpenDocs => "ℹ",
        Action::ReportIssue => "🐛",
        Action::LoadPointfile | Action::UnloadPointfile => "💧",
        Action::ShowConsole => "🖵",
        _ => return None,
    })
}
