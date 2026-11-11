use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { launch_actions {
    LaunchGame => "launch_game", File, FileGame, None, Ready;
    ReloadInGame => "reload_in_game", File, FileGame, None, Ready;
    ImportFromGame => "import_from_game", File, FileImport, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::LaunchGame => Some(state.doc.path().is_some()),
        Action::ReloadInGame => Some(state.doc.path().is_some() && state.runtime.launch.game.is_running()),
        Action::ImportFromGame => {
            Some(state.runtime.launch.game.is_running() && state.runtime.launch.bridge_installed == Some(true))
        }
        _ => None,
    }
}

pub(super) fn check(_state: &AppState, _action: Action) -> Option<Check> {
    None
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::LaunchGame => Some("▶"),
        Action::ReloadInGame => Some("⟳"),
        Action::ImportFromGame => Some("🎮"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.launch_game", "Run the map in game", "Lancer la map dans le jeu"),
    ("action.reload_in_game", "Reload the map in game", "Recharger la map dans le jeu"),
    ("action.import_from_game", "Import props from the game", "Importer les props depuis le jeu"),
];
