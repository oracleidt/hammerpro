use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { startup_actions {
    ExportCopy => "export_copy", File, FileImport, None, Ready;
    ExportAgain => "export_again", File, FileImport, sc(ALT, Key::B), Ready;
    ExportObj => "export_obj", File, FileImport, None, Ready;
    ReloadContent => "reload_content", Tools, ToolsContent, None, Ready;
    RebuildContentIndex => "rebuild_content_index", Tools, ToolsContent, None, Ready;
    CheckForUpdates => "check_for_updates", Help, HelpDocs, None, Ready;
    InstallUpdate => "install_update", Help, Hidden, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::CheckForUpdates => Some(!state.runtime.update.status.is_busy()),
        Action::InstallUpdate => Some(state.runtime.update.can_install()),
        _ => None,
    }
}

pub(super) fn check(_state: &AppState, _action: Action) -> Option<Check> {
    None
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::ReloadContent | Action::RebuildContentIndex => Some("⟳"),
        Action::CheckForUpdates | Action::InstallUpdate => Some("⟳"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.export_copy", "Export a copy...", "Exporter une copie..."),
    ("action.export_again", "Export again", "Exporter à nouveau"),
    ("action.export_obj", "Export OBJ...", "Exporter en OBJ..."),
    ("action.reload_content", "Reload content from disk", "Recharger le contenu depuis le disque"),
    ("action.check_for_updates", "Check for updates...", "Rechercher des mises à jour..."),
    ("action.install_update", "Install the available update", "Installer la mise à jour disponible"),
    ("action.rebuild_content_index", "Rebuild the content index", "Reconstruire l'index du contenu"),
];
