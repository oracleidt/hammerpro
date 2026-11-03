use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanModeSetting {
    #[default]
    Auto,
    Parallel,
    Sequential,
}

impl ScanModeSetting {
    pub const ALL: [ScanModeSetting; 3] =
        [ScanModeSetting::Auto, ScanModeSetting::Parallel, ScanModeSetting::Sequential];

    pub fn to_fs(self) -> hp_fs::ScanMode {
        match self {
            ScanModeSetting::Auto => hp_fs::ScanMode::Auto,
            ScanModeSetting::Parallel => hp_fs::ScanMode::Parallel,
            ScanModeSetting::Sequential => hp_fs::ScanMode::Sequential,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            ScanModeSetting::Auto => "shell.scan.mode_auto",
            ScanModeSetting::Parallel => "shell.scan.mode_parallel",
            ScanModeSetting::Sequential => "shell.scan.mode_sequential",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanSettings {
    pub use_index: bool,
    pub mode: ScanModeSetting,
}

impl Default for ScanSettings {
    fn default() -> Self {
        Self { use_index: true, mode: ScanModeSetting::Auto }
    }
}

impl ScanSettings {
    pub fn index_file() -> std::path::PathBuf {
        super::Settings::cache_dir().join("content_index.bin")
    }

    pub fn to_config(&self) -> hp_fs::ScanConfig {
        hp_fs::ScanConfig { index_file: self.use_index.then(Self::index_file), mode: self.mode.to_fs() }
    }
}
