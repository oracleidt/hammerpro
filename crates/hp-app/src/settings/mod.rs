use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

mod brush;
mod compile;
pub mod compilers;
mod content;
mod discord;
mod entities;
mod grid;
mod health;
mod helpers;
mod instances;
mod lab;
mod launch;
mod lighting;
mod mcp;
mod mesh;
mod placement;
mod render;
mod scan;
mod selection;
mod shell;
mod startup;
mod update;
mod visibility;
mod workflow;

pub use brush::BrushSettings;
pub use compile::{
    BspModeSetting, CompilePresetSetting, CompileProfile, CompileSettings, FlagChoiceSetting, LightingSetting,
    PackPolicy, ProfileStep, RadModeSetting, StaticPropShadows, VisModeSetting, WindowPlacement,
    background_precompile_default,
};
pub use compilers::{CompilerProfile, CompilerStep, CompilerStepKind};
pub use content::{ContentSettings, ReleaseMode};
pub use discord::DiscordSettings;
pub use entities::{BrowserSettings, EntitiesSettings, PathToolSettings};
pub use grid::{
    GRID_COORD_LIMIT, GridMode, GridSettings, GridSpec, MAX_CUSTOM_GRIDS, MAX_MAJOR_EVERY, PerViewGrids, ViewGrid,
};
pub use health::HealthSettings;
pub use helpers::{ConnectionsMode, HelperScope, HelpersSettings};
pub use hp_sniffer::{
    DecompileSettings as SnifferDecompileSettings, GeometrySource as SnifferGeometry,
    InsecureMode as SnifferInsecureMode, SnifferSettings,
};
pub use instances::{InstanceDisplay, InstancesSettings};
pub use lab::{LabSettings, ModelLodSettings, UnpropShape};
pub use launch::{LaunchSettings, LaunchTiming};
pub use lighting::{LightingQuality, LightingSettings};
pub use mcp::McpSettings;
pub use mesh::MeshSettings;
pub use placement::{PlacementSettings, SprinkleMode, SprinkleSettings};
pub use render::{
    Disp2dSetting, Disp2dViewSetting, MAX_VRAM_BUDGET_MB, MIN_VRAM_BUDGET_MB, Models2dSetting, PropShadowSetting,
    RenderSettings, ToneModeSetting, WaterDetailSetting,
};
pub use scan::{ScanModeSetting, ScanSettings};
pub use selection::SelectionSettings;
pub use shell::{
    EditSettings, KeyboardLayout, RenderModeSetting, SelectModeSetting, ThemeChoice, UiSettings, ViewSettings,
};
pub use startup::{CustomDirSetting, GameSettings, MapGame, StartupSettings, profile_of};
pub use update::UpdateSettings;
pub use visibility::VisibilitySettings;
pub use workflow::{
    CtrlWheel, DisplayUnits, FLY_RESPONSE_MAX, GRID_MAX, GRID_MIN, GRID_STEP_MAX, GRID_STEP_MIN, MAX_LAYOUT_NAME,
    MAX_LAYOUTS, NavScheme, SavedLayout, WorkflowSettings, clamp_grid, grid_step,
};

impl Settings {
    pub fn sync_game(&mut self) {
        let g = &self.game;
        self.compile.target = (!g.is_gmod()).then(|| g.active_game().to_owned());
        self.compile.target_dir = g.game_dir_of(g.active_game()).map(Path::to_path_buf);
    }

    pub fn set_map_game(&mut self, id: Option<&str>) -> bool {
        let before = self.game.active_game().to_owned();
        let id = id.map(str::trim).filter(|i| !i.is_empty() && !i.eq_ignore_ascii_case(&self.game.game));
        self.game.map_game = id.map(str::to_owned);
        self.sync_game();
        !before.eq_ignore_ascii_case(self.game.active_game())
    }
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Lang,
    pub language_chosen: bool,
    pub recent_files: Vec<PathBuf>,
    pub view: ViewSettings,
    pub edit: EditSettings,
    pub game: GameSettings,
    pub compile: CompileSettings,
    pub ui: UiSettings,
    pub browser: BrowserSettings,
    pub keybindings: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "is_default")]
    pub selection: SelectionSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub brush: BrushSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub workflow: WorkflowSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub instances: InstancesSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub visibility: VisibilitySettings,
    #[serde(skip_serializing_if = "is_default")]
    pub helpers: HelpersSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub mesh: MeshSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub entities: EntitiesSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub render: RenderSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub lighting: LightingSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub placement: PlacementSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub startup: StartupSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub launch: LaunchSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub content: ContentSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub health: HealthSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub lab: LabSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub mcp: McpSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub grid: GridSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub discord: DiscordSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub scan: ScanSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub update: UpdateSettings,
    #[serde(skip_serializing_if = "is_default")]
    pub sniffer: SnifferSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Lang::En,
            language_chosen: false,
            recent_files: Vec::new(),
            view: ViewSettings::default(),
            edit: EditSettings::default(),
            game: GameSettings::default(),
            compile: CompileSettings::default(),
            ui: UiSettings::default(),
            browser: BrowserSettings::default(),
            keybindings: BTreeMap::new(),
            selection: SelectionSettings::default(),
            brush: BrushSettings::default(),
            workflow: WorkflowSettings::default(),
            instances: InstancesSettings::default(),
            visibility: VisibilitySettings::default(),
            helpers: HelpersSettings::default(),
            mesh: MeshSettings::default(),
            entities: EntitiesSettings::default(),
            render: RenderSettings::default(),
            lighting: LightingSettings::default(),
            placement: PlacementSettings::default(),
            startup: StartupSettings::default(),
            launch: LaunchSettings::default(),
            content: ContentSettings::default(),
            health: HealthSettings::default(),
            lab: LabSettings::default(),
            mcp: McpSettings::default(),
            grid: GridSettings::default(),
            discord: DiscordSettings::default(),
            scan: ScanSettings::default(),
            update: UpdateSettings::default(),
            sniffer: SnifferSettings::default(),
        }
    }
}

impl Settings {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("HammerPro")
    }

    pub fn default_path() -> PathBuf {
        Self::config_dir().join("settings.toml")
    }

    pub fn autosave_dir() -> PathBuf {
        dirs::data_local_dir().unwrap_or_else(Self::config_dir).join("HammerPro").join("autosave")
    }

    pub fn crash_dir() -> PathBuf {
        Self::config_dir().join("crash")
    }

    pub fn log_dir() -> PathBuf {
        dirs::data_local_dir().unwrap_or_else(Self::config_dir).join("HammerPro").join("logs")
    }

    pub fn cache_dir() -> PathBuf {
        dirs::cache_dir().unwrap_or_else(Self::config_dir).join("HammerPro")
    }

    pub fn load_or_default(path: &Path) -> Settings {
        let mut s = match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Settings>(&text) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("settings file is invalid, using defaults: {e}");
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        };
        if !s.language_chosen {
            s.language = Lang::from_system();
            s.language_chosen = true;
        }
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    pub fn add_recent(&mut self, path: &Path) {
        self.recent_files.retain(|p| p != path);
        self.recent_files.insert(0, path.to_path_buf());
        self.recent_files.truncate(12);
    }

    pub fn add_recent_material(&mut self, name: &str) {
        let lower = name.to_ascii_lowercase();
        self.browser.recent_materials.retain(|m| m != &lower);
        self.browser.recent_materials.insert(0, lower);
        self.browser.recent_materials.truncate(64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_toml() {
        let mut s = Settings { language: Lang::Fr, ..Settings::default() };
        s.game.custom_texture_dirs.push(CustomDirSetting {
            path: PathBuf::from("C:/textures"),
            prefix: "custom/textures".into(),
            enabled: true,
        });
        s.keybindings.insert("save_map".into(), "Ctrl+S".into());
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn tolerates_partial_files() {
        let s: Settings = toml::from_str("language = \"Fr\"\n[edit]\ngrid_size = 8.0\n").unwrap();
        assert_eq!(s.language, Lang::Fr);
        assert_eq!(s.edit.grid_size, 8.0);
        assert!(s.edit.snap_to_grid);
    }

    #[test]
    fn old_files_load_and_package_tables_stay_out() {
        let old = r#"language = "Fr"
language_chosen = true
recent_files = []

[view]
fov_deg = 90.0

[edit]
grid_size = 32.0

[game]
mount_addons = false

[compile]
threads = 6

[ui]
theme = "Light"

[browser]
favorites = ["dev/dev_measuregeneric01b"]

[keybindings]
undo = "Ctrl+Alt+Z"
"#;
        let s: Settings = toml::from_str(old).unwrap();
        assert_eq!(s.view.fov_deg, 90.0);
        assert_eq!(s.edit.grid_size, 32.0);
        assert!(!s.game.mount_addons);
        assert_eq!(s.compile.threads, Some(6));
        assert_eq!(s.ui.theme, ThemeChoice::Light);
        assert_eq!(s.browser.favorites.len(), 1);
        assert_eq!(s.keybindings.get("undo").map(String::as_str), Some("Ctrl+Alt+Z"));
        assert_eq!(s.render, RenderSettings::default());
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(s.grid.mode, GridMode::Global, "files written before the grid modes use Hammer's grid");
        for table in ["[selection]", "[brush]", "[render]", "[startup]", "[lab]", "[grid]"] {
            assert!(!text.contains(table), "{table} written while empty:\n{text}");
        }
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
        let newer = format!("{text}\n[some_future_package]\nanswer = 42\n");
        assert_eq!(toml::from_str::<Settings>(&newer).unwrap(), s);
    }
}
