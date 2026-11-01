mod brush;
mod compile;
mod content;
mod discord;
mod entities;
mod health;
pub mod helpers;
pub mod instances;
mod lab;
mod launch;
mod lighting;
mod mcp;
mod mesh;
mod placement;
pub mod render;
mod selection;
mod startup;
mod visibility;
mod workflow;

pub use brush::BrushRuntime;
pub use discord::{DiscordRuntime, DiscordStatus};
pub use compile::{CompileBadge, CompileBrowse, CompileProgress, CompileRuntime};
pub use content::{
    AssetCollectRequest, AssetCollectSummary, CaptureState, CapturedImage, ChecklistRow, ContentRuntime,
    MapAssetsRuntime, ReleaseReport,
};
pub use entities::{AssetKind, AssetPick, EntitiesRuntime, KeyvalueClip};
pub use health::{HealthRuntime, LeakResult};
pub use helpers::HelpersRuntime;
pub use instances::InstancesRuntime;
pub use lab::{DEFAULT_CAMERA_DISTANCE, DEFAULT_PLAYER_MODEL, LabRuntime, LightStyleReport, PlayerState};
pub use launch::{ContentUse, GameState, LaunchRuntime, MapLoadReport, content_use};
pub use lighting::{
    BakeKind, BakePhase, BakeProgress, BakeSummary, BudgetEntry, BudgetSummary, LightingRuntime, PreviewStats,
};
pub use mcp::{McpClientRow, McpRuntime};
pub use mesh::MeshRuntime;
pub use placement::{
    CreatePrefabRequest, FileStamp, PlacementRuntime, PrefabCache, PrefabLoad, PrefabTokens, file_stamp,
};
pub use render::RenderRuntime;
pub use selection::SelectionRuntime;
pub use startup::{OpeningStatus, StartupRuntime};
pub use visibility::VisibilityRuntime;
pub use workflow::{MAX_DROPPED, MapTabInfo, TabRequest, WorkflowMapMemory, WorkflowRuntime};

#[derive(Debug, Default)]
pub struct Runtime {
    pub selection: SelectionRuntime,
    pub brush: BrushRuntime,
    pub workflow: WorkflowRuntime,
    pub instances: InstancesRuntime,
    pub visibility: VisibilityRuntime,
    pub helpers: HelpersRuntime,
    pub mesh: MeshRuntime,
    pub entities: EntitiesRuntime,
    pub render: RenderRuntime,
    pub lighting: LightingRuntime,
    pub placement: PlacementRuntime,
    pub compile: CompileRuntime,
    pub startup: StartupRuntime,
    pub launch: LaunchRuntime,
    pub content: ContentRuntime,
    pub health: HealthRuntime,
    pub lab: LabRuntime,
    pub mcp: McpRuntime,
    pub discord: DiscordRuntime,
}
