use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { mcp_actions {
    ToggleMcpServer => "toggle_mcp_server", Tools, ToolsMcp, None, Ready;
    McpInstall => "mcp_install", Tools, ToolsMcp, None, Ready;
    McpPanel => "mcp_panel", Tools, ToolsMcp, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::McpInstall => Some(!state.runtime.mcp.install_running),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::ToggleMcpServer => Some(Check::Toggle(state.runtime.mcp.running)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::ToggleMcpServer => Some("🔌"),
        Action::McpInstall => Some("⬇"),
        Action::McpPanel => Some("⚙"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.toggle_mcp_server", "MCP server", "Serveur MCP"),
    ("action.mcp_install", "Install MCP in Claude, Codex and Gemini", "Installer MCP dans Claude, Codex et Gemini"),
    ("action.mcp_panel", "MCP...", "MCP..."),
    ("shell.undo.mcp_create_entity", "MCP: create entity", "MCP : création d'entité"),
    ("shell.undo.mcp_set_entity_keyvalues", "MCP: edit keyvalues", "MCP : modification des keyvalues"),
    ("shell.undo.mcp_delete_objects", "MCP: delete objects", "MCP : suppression d'objets"),
    ("shell.undo.mcp_create_brush", "MCP: create brush", "MCP : création de brush"),
    ("shell.undo.mcp_apply_material", "MCP: apply material", "MCP : application de matériau"),
    ("shell.undo.mcp_move_objects", "MCP: move objects", "MCP : déplacement d'objets"),
    ("shell.undo.mcp_rotate_objects", "MCP: rotate objects", "MCP : rotation d'objets"),
    ("shell.undo.mcp_paste_vmf_text", "MCP: paste VMF text", "MCP : collage de texte VMF"),
    ("shell.undo.mcp_run_action", "MCP: editor command", "MCP : commande de l'éditeur"),
];
