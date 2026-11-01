use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomDirSetting {
    pub path: PathBuf,
    pub prefix: String,
    pub enabled: bool,
}

impl Default for CustomDirSetting {
    fn default() -> Self {
        Self { path: PathBuf::new(), prefix: String::new(), enabled: true }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    pub game: String,
    pub game_dirs: BTreeMap<String, PathBuf>,
    #[serde(skip)]
    pub map_game: Option<String>,
    pub gmod_root: Option<PathBuf>,
    pub mount_addons: bool,
    pub mount_workshop: bool,
    pub mount_cfg: bool,
    #[serde(rename = "mount_server_content")]
    pub mount_downloads: bool,
    pub extra_game_dirs: Vec<PathBuf>,
    pub mount_app_ids: Vec<u32>,
    pub custom_texture_dirs: Vec<CustomDirSetting>,
    pub extra_fgds: Vec<PathBuf>,
    pub fgd_override: Option<PathBuf>,
    pub map_dir: Option<PathBuf>,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            game: hp_fs::GMOD_ID.to_owned(),
            game_dirs: BTreeMap::new(),
            map_game: None,
            gmod_root: None,
            mount_addons: true,
            mount_workshop: false,
            mount_cfg: true,
            mount_downloads: true,
            extra_game_dirs: Vec::new(),
            mount_app_ids: Vec::new(),
            custom_texture_dirs: Vec::new(),
            extra_fgds: Vec::new(),
            fgd_override: None,
            map_dir: None,
        }
    }
}

impl GameSettings {
    pub fn active_game(&self) -> &str {
        let id = self.map_game.as_deref().unwrap_or(&self.game).trim();
        if id.is_empty() { hp_fs::GMOD_ID } else { id }
    }

    pub fn is_gmod(&self) -> bool {
        self.active_game().eq_ignore_ascii_case(hp_fs::GMOD_ID)
    }

    pub fn game_dir_of(&self, id: &str) -> Option<&Path> {
        self.game_dirs.get(id).map(PathBuf::as_path).filter(|p| !p.as_os_str().is_empty())
    }

    pub fn active_profile(&self) -> Option<hp_fs::GameProfile> {
        profile_of(self, self.active_game())
    }
}

pub fn profile_of(g: &GameSettings, id: &str) -> Option<hp_fs::GameProfile> {
    if id.eq_ignore_ascii_case(hp_fs::GMOD_ID) {
        let known = hp_fs::known_game(hp_fs::GMOD_ID)?;
        let root = g.gmod_root.clone().filter(|p| !p.as_os_str().is_empty()).or_else(hp_fs::detect_gmod_root);
        let root = root.map(|r| {
            if !r.join("garrysmod").is_dir() && r.file_name().is_some_and(|n| n.eq_ignore_ascii_case("garrysmod")) {
                r.parent().map_or(r.clone(), Path::to_path_buf)
            } else {
                r
            }
        });
        return Some(
            root.and_then(|r| hp_fs::GameProfile::installed_in(known, &r))
                .unwrap_or_else(|| hp_fs::GameProfile::not_installed(known)),
        );
    }
    hp_fs::resolve_game(id, g.game_dir_of(id))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapGame {
    pub id: String,
}

impl crate::sidecar::SidecarSection for MapGame {
    const NAME: &'static str = "game";
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupSettings {}
