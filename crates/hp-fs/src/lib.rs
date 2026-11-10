mod apps;
mod assets;
mod change_gate;
pub mod collect;
mod custom;
pub mod deps;
mod detect;
mod dirindex;
mod drive;
mod error;
mod export;
mod fs;
pub mod games;
mod keywords;
pub mod lint;
mod material;
mod mount;
mod path;
mod progress;
mod scan;
mod scanfmt;
mod source;
mod surfaces;
mod texture;

#[cfg(test)]
mod test_util;

#[cfg(test)]
mod scan_tests;

#[cfg(test)]
mod dedupe_tests;

pub use apps::{
    MAX_APP_ID, SteamApp, find_steam_app, find_steam_app_in, installed_source_apps, installed_source_apps_in,
    refresh_steam_apps, steam_app_name,
};
pub use assets::AssetEntry;
pub use change_gate::{ChangeGate, GateWait, MAX_WATCHED_ROOTS, WakeEvent};
pub use custom::{
    CustomDir, CustomLayout, CustomMode, IMAGE_EXTENSIONS, custom_layout, game_texture_size, is_auto_prefix,
    is_safe_material_name, safe_material_name, suggest_prefix,
};
pub use detect::{GMOD_APP_ID, detect_gmod_root, detect_steam_libraries, detect_workshop_dir};
pub use drive::{DriveProbe, ScanMode, decide as scan_decision, parallel_with as scan_parallel_with};
pub use error::FsError;
pub use export::{ExportFailure, ExportReport, ResizedImage};
pub use fs::{CustomReport, GameFs};
pub use games::{
    GMOD_ID, GameProfile, KNOWN_GAMES, KnownGame, LightingPasses, detect_games, game_catalogue, known_game,
    refresh_games, resolve_game,
};
pub use keywords::MaterialKeywords;
pub use mount::{DeferredMount, MountOptions};
pub use path::normalize_path;
pub use progress::{MountProgress, MountSnapshot, MountStage};
pub use scan::{ScanConfig, ScanStats, clear_index, configure as configure_scan, flush_index, scan_config, scan_stats};
pub use source::{SourceInfo, SourceKind, SourceMount, SourceMountKind, SourceOrigin};
pub use texture::{CustomImageInfo, LoadedTexture};
