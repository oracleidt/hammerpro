use std::path::PathBuf;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for CapturedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CapturedImage({}x{})", self.width, self.height)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CaptureState {
    #[default]
    Idle,
    Requested,
    Capturing,
    Ready(CapturedImage),
    Failed,
}

#[derive(Debug, Default)]
pub struct ContentRuntime {
    pub release_open: bool,
    pub release_tab: Option<&'static str>,
    pub inspector_open: bool,
    pub inspector_request: Option<PathBuf>,
    pub capture: CaptureState,
    pub progress: Option<(String, f32)>,
    pub cancel_requested: bool,
    pub graph: Option<std::sync::Arc<hp_fs::deps::DependencyGraph>>,
    pub start_requested: Option<bool>,
    pub start_allow_missing: bool,
    pub awaiting_missing: Option<MissingCounts>,
    pub checklist_requested: bool,
    pub checklist: Option<Vec<ChecklistRow>>,
    pub last_release: Option<ReleaseReport>,
    pub map_assets: MapAssetsRuntime,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MissingCounts {
    pub files: usize,
    pub particle_systems: usize,
    pub left_out: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetCollectRequest {
    pub output: PathBuf,
    pub base_game: bool,
    pub replace: bool,
    pub groups: Vec<String>,
    pub filter: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetCollectSummary {
    pub output: PathBuf,
    pub copied: usize,
    pub replaced: usize,
    pub skipped: usize,
    pub same: usize,
    pub failed: Vec<(String, String)>,
    pub missing: Vec<String>,
    pub bytes: u64,
    pub report: Option<PathBuf>,
    pub cancelled: bool,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct MapAssetsRuntime {
    pub collect_request: Option<AssetCollectRequest>,
    pub cancel_requested: bool,
    pub progress: Option<(usize, usize)>,
    pub last_collect: Option<AssetCollectSummary>,
    pub unchecked_changed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChecklistRow {
    pub label: String,
    pub status: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReleaseReport {
    pub error: Option<String>,
    pub out_root: Option<PathBuf>,
    pub addon_dir: Option<PathBuf>,
    pub gma: Option<PathBuf>,
    pub files: usize,
    pub bytes: u64,
    pub gma_bytes: u64,
    pub duration_ms: u64,
    pub protected_server_bsp: Option<PathBuf>,
    pub log_tail: Vec<String>,
}
