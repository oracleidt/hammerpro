mod blender;
mod brush;
mod compile;
mod content;
mod discord;
mod entities;
pub mod fade;
pub mod fog;
mod gpu;
mod health;
pub mod helpers;
pub mod instances;
mod lab;
mod soundcolor;
pub use soundcolor::SoundColorRuntime;
mod launch;
mod lighting;
mod mcp;
mod mesh;
mod mesh_disp;
pub mod perf;
mod placement;
pub mod render;
mod roadside;
mod selection;
mod shader;
pub mod shadow;
mod startup;
pub mod update;
mod visibility;
mod vtf;
pub mod wind;
mod workflow;

pub use blender::{BlenderLink, BlenderNote, BlenderRuntime, BlenderTarget, DispSent};
pub use brush::BrushRuntime;
pub use compile::{CompileBadge, CompileBrowse, CompileProgress, CompileRuntime};
pub use content::{
    AssetCollectRequest, AssetCollectSummary, CaptureState, CapturedImage, ChecklistRow, ContentRuntime,
    MapAssetsRuntime, ReleaseReport,
};
pub use discord::{DiscordRuntime, DiscordStatus};
pub use entities::{AssetKind, AssetPick, EntitiesRuntime, KeyvalueClip};
pub use gpu::{GpuRelease, GpuReleased, GpuRuntime, VramBreakdown, VramPressure, VramSource};
pub use health::{HealthRuntime, LeakResult};
pub use helpers::HelpersRuntime;
pub use instances::InstancesRuntime;
pub use lab::{
    DEFAULT_CAMERA_DISTANCE, DEFAULT_PLAYER_MODEL, FlyTextureOp, FlyTextureRuntime, LabRuntime, LightStyleReport,
    ParticlePreviewRuntime, PlayerPlace, PlayerState, TrainReport,
};
pub use launch::{ContentUse, GameState, LaunchRuntime, MapLoadReport, content_use};
pub use lighting::{
    BakeKind, BakePhase, BakeProgress, BakeSummary, BudgetEntry, BudgetSummary, IndirectPhase, IndirectStatus,
    LightingRuntime, PreviewStats,
};
pub use mcp::{McpClientRow, McpRuntime};
pub use mesh::{DispLoopRequest, EdgeRequest, ErosionDialog, MeshRuntime, TerrainPreview};
pub use mesh_disp::MeshDispRuntime;
pub use perf::{BenchRequest, BenchResult, PerfLevel, PerfPlan, PerfRuntime, PerfSample, Reflections};
pub use placement::{
    CreatePrefabRequest, FileStamp, PlacementRuntime, PrefabCache, PrefabLoad, PrefabTokens, file_stamp,
};
pub use render::RenderRuntime;
pub use roadside::{PreviewLine, PreviewTri, RoadPreview, RoadsideRuntime};
pub use selection::SelectionRuntime;
pub use shader::{LiveGraph, ShaderRequest, ShaderRuntime};
pub use startup::{OpeningStatus, StartupRuntime};
pub use update::{UpdateFailure, UpdateInfo, UpdateRuntime, UpdateStatus};
pub use visibility::VisibilityRuntime;
pub use vtf::{VtfRequest, VtfRuntime};
pub use workflow::{MAX_DROPPED, MapTabInfo, TabRequest, WorkflowMapMemory, WorkflowRuntime};

#[derive(Debug, Default)]
pub struct Runtime {
    pub fade: fade::FadePreview,
    pub selection: SelectionRuntime,
    pub brush: BrushRuntime,
    pub workflow: WorkflowRuntime,
    pub instances: InstancesRuntime,
    pub visibility: VisibilityRuntime,
    pub helpers: HelpersRuntime,
    pub mesh: MeshRuntime,
    pub mesh_disp: MeshDispRuntime,
    pub entities: EntitiesRuntime,
    pub render: RenderRuntime,
    pub lighting: LightingRuntime,
    pub placement: PlacementRuntime,
    pub roadside: RoadsideRuntime,
    pub compile: CompileRuntime,
    pub startup: StartupRuntime,
    pub launch: LaunchRuntime,
    pub content: ContentRuntime,
    pub health: HealthRuntime,
    pub lab: LabRuntime,
    pub mcp: McpRuntime,
    pub discord: DiscordRuntime,
    pub update: UpdateRuntime,
    pub blender: BlenderRuntime,
    pub gpu: GpuRuntime,
    pub storage: crate::storage::StorageRuntime,
    pub shader: ShaderRuntime,
    pub vtf: VtfRuntime,
}
