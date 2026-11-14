mod blender;
mod brush;
pub mod collaboration;
mod compile;
mod content;
mod cubemaps;
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
pub mod soundscape;
pub use soundscape::SoundscapeRuntime;
mod launch;
mod lighting;
mod livelink;
mod mcp;
mod mesh;
mod mesh_disp;
mod optimize;
pub mod perf;
mod placement;
pub mod primitive_scene;
pub mod primitives;
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
pub use cubemaps::CubemapRuntime;
pub use discord::{DiscordRuntime, DiscordStatus};
pub use entities::{AssetKind, AssetPick, EntitiesRuntime, KeyvalueClip};
pub use gpu::{
    GpuHardware, GpuRelease, GpuReleased, GpuRuntime, MemoryTier, TextureStreaming, VramBreakdown, VramPressure,
    VramSource,
};
pub use health::{HealthRuntime, LeakResult};
pub use helpers::HelpersRuntime;
pub use instances::InstancesRuntime;
pub use lab::{
    CombineRequest, DEFAULT_CAMERA_DISTANCE, DEFAULT_PLAYER_MODEL, FlyTextureOp, FlyTextureRuntime, LabRuntime,
    LightStyleReport, ParticlePreviewRuntime, PlayerPlace, PlayerState, PropCreationRequest, TrainReport,
};
pub use launch::{ContentUse, GameState, LaunchRuntime, MapLoadReport, content_use};
pub use lighting::{
    BakeKind, BakePhase, BakeProgress, BakeSummary, BudgetEntry, BudgetSummary, IndirectPhase, IndirectStatus,
    LightCostEntry, LightingRuntime, PreparedStatus, PreviewStats, RelightStats,
};
pub use livelink::{FireForm, LiveLinkRuntime};
pub use mcp::{McpClientRow, McpRuntime};
pub use mesh::{DispLoopRequest, EdgeRequest, ErosionDialog, MeshRuntime, TerrainPreview};
pub use mesh_disp::MeshDispRuntime;
pub use optimize::{ANALYSIS_PORTALS, OptimizeRuntime};
pub use perf::{
    BenchFrame, BenchRequest, BenchResult, FRAME_PHASES, FramePhases, GpuPasses, PerfLevel, PerfPlan, PerfRuntime,
    PerfSample, Reflections, RepaintCount, ViewCost,
};
pub use placement::{
    CreatePrefabRequest, FileStamp, PlacementRuntime, PrefabCache, PrefabLoad, PrefabTokens, RealDims, UnitDims,
    file_stamp, placement_unit_text, real_num,
};
pub use render::RenderRuntime;
pub use roadside::{PreviewLine, PreviewTri, RoadPreview, RoadsideRuntime};
pub use selection::{DragTransform, SelectionRuntime};
pub use shader::{LiveGraph, ShaderRequest, ShaderRuntime};
pub use startup::{OpeningStatus, PreloadMaterial, PreloadResource, PreloadResult, ResourceLoading, StartupRuntime};
pub use update::{UpdateFailure, UpdateInfo, UpdateRuntime, UpdateStatus};
pub use visibility::VisibilityRuntime;
pub use vtf::{VtfRequest, VtfRuntime};
pub use workflow::{MAX_DROPPED, MapTabInfo, TabRequest, WorkflowMapMemory, WorkflowRuntime};

#[derive(Debug, Default)]
pub struct Runtime {
    pub collaboration: collaboration::CollaborationRuntime,
    pub primitives: primitives::PrimitivesRuntime,
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
    pub optimize: OptimizeRuntime,
    pub cubemaps: CubemapRuntime,
    pub lab: LabRuntime,
    pub mcp: McpRuntime,
    pub discord: DiscordRuntime,
    pub update: UpdateRuntime,
    pub blender: BlenderRuntime,
    pub gpu: GpuRuntime,
    pub storage: crate::storage::StorageRuntime,
    pub shader: ShaderRuntime,
    pub vtf: VtfRuntime,
    pub livelink: LiveLinkRuntime,
}
