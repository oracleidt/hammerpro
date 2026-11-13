use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { launch_actions {
    LaunchGame => "launch_game", File, FileGame, None, Ready;
    ReloadInGame => "reload_in_game", File, FileGame, None, Ready;
    ImportFromGame => "import_from_game", File, FileImport, None, Ready;
    LiveLinkStart => "live_link_start", Tools, ToolsLiveLink, None, Ready;
    LiveLinkStop => "live_link_stop", Tools, ToolsLiveLink, None, Ready;
    LiveLinkPanel => "live_link_panel", Tools, ToolsLiveLink, None, Ready;
    LiveLinkFireInput => "live_link_fire_input", Tools, ToolsLiveLink, None, Ready;
    LiveLinkTeleport => "live_link_teleport", Tools, ToolsLiveLink, None, Ready;
    LiveLinkTeleportBack => "live_link_teleport_back", Tools, ToolsLiveLink, None, Ready;
    LiveLinkCameraToGame => "live_link_camera_to_game", Tools, ToolsLiveLink, None, Ready;
    LiveLinkCameraFromGame => "live_link_camera_from_game", Tools, ToolsLiveLink, None, Ready;
    LiveLinkResync => "live_link_resync", Tools, ToolsLiveLink, None, Ready;
    LiveLinkRemoveAddon => "live_link_remove_addon", Tools, ToolsLiveLink, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::LaunchGame => Some(state.doc.path().is_some()),
        Action::ReloadInGame => Some(state.doc.path().is_some() && state.runtime.launch.game.is_running()),
        Action::ImportFromGame => {
            Some(state.runtime.launch.game.is_running() && state.runtime.launch.bridge_installed == Some(true))
        }
        Action::LiveLinkStart => Some(!state.runtime.livelink.is_on()),
        Action::LiveLinkStop | Action::LiveLinkResync => Some(state.runtime.livelink.is_on()),
        Action::LiveLinkFireInput | Action::LiveLinkTeleport | Action::LiveLinkTeleportBack => {
            Some(state.runtime.livelink.is_connected())
        }
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::LiveLinkPanel => Some(Check::Toggle(state.runtime.livelink.panel_open)),
        Action::LiveLinkCameraToGame => Some(Check::Toggle(state.settings.livelink.camera_to_game)),
        Action::LiveLinkCameraFromGame => Some(Check::Toggle(state.settings.livelink.camera_from_game)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::LaunchGame => Some("▶"),
        Action::ReloadInGame => Some("⟳"),
        Action::ImportFromGame => Some("🎮"),
        Action::LiveLinkStart => Some("🔗"),
        Action::LiveLinkStop => Some("⏹"),
        Action::LiveLinkPanel => Some("ℹ"),
        Action::LiveLinkFireInput => Some("⏵"),
        Action::LiveLinkTeleport => Some("📷"),
        Action::LiveLinkTeleportBack => Some("⟲"),
        Action::LiveLinkCameraToGame => Some("🎮"),
        Action::LiveLinkCameraFromGame => Some("👁"),
        Action::LiveLinkResync => Some("⟳"),
        Action::LiveLinkRemoveAddon => Some("🗑"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.launch_game", "Run the map in game", "Lancer la map dans le jeu"),
    ("action.reload_in_game", "Reload the map in game", "Recharger la map dans le jeu"),
    ("action.import_from_game", "Import props from the game", "Importer les props depuis le jeu"),
    ("action.live_link_start", "Start the live link with Garry's Mod", "Démarrer le lien direct avec Garry's Mod"),
    ("action.live_link_stop", "Stop the live link", "Arrêter le lien direct"),
    ("action.live_link_panel", "Live link status...", "État du lien direct..."),
    ("action.live_link_fire_input", "Fire input in game...", "Déclencher une entrée dans le jeu..."),
    ("action.live_link_teleport", "Teleport the player to the 3D camera", "Téléporter le joueur à la caméra 3D"),
    ("action.live_link_teleport_back", "Send the player back", "Ramener le joueur"),
    ("action.live_link_camera_to_game", "Player follows the 3D camera", "Le joueur suit la caméra 3D"),
    ("action.live_link_camera_from_game", "3D camera follows the player", "La caméra 3D suit le joueur"),
    ("action.live_link_resync", "Send all changes to the game again", "Renvoyer toutes les modifications au jeu"),
    (
        "action.live_link_remove_addon",
        "Remove the live link addon from Garry's Mod",
        "Retirer l'addon du lien direct de Garry's Mod",
    ),
];
