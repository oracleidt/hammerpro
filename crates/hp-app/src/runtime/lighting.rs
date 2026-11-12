#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BakePhase {
    Preparing,
    Compiling,
    Direct,
    Bounce,
    Finishing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BakeKind {
    #[default]
    Full,
    AroundCamera,
    Update,
    Refine,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakeProgress {
    pub kind: BakeKind,
    pub phase: BakePhase,
    pub fraction: f32,
    pub elapsed_s: f32,
    pub faces: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BakeSummary {
    pub kind: BakeKind,
    pub cancelled: bool,
    pub seconds: f32,
    pub faces: usize,
    pub luxels: u64,
    pub density: f32,
    pub atlas_full: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewStats {
    pub faces: usize,
    pub baked_faces: usize,
    pub luxels: u64,
    pub triangles: usize,
    pub layers: u32,
    pub gpu_bytes: u64,
    pub world_lights: usize,
    pub static_props: usize,
    pub vertex_lit_props: usize,
    pub scope_radius: f32,
    pub scope_auto: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BudgetEntry {
    pub solid: u32,
    pub side: usize,
    pub bytes: u64,
    pub scale: f64,
    pub dims: [u32; 2],
    pub displacement: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BudgetSummary {
    pub faces: usize,
    pub luxels: u64,
    pub bytes: u64,
    pub limit: u64,
    pub tiny_faces: usize,
    pub wide_faces: usize,
    pub oversized_displacements: usize,
    pub top: Vec<BudgetEntry>,
    pub revision: u64,
}

impl BudgetSummary {
    pub fn fraction(&self) -> f64 {
        if self.limit == 0 { 0.0 } else { self.bytes as f64 / self.limit as f64 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IndirectPhase {
    #[default]
    Off,
    NoLights,
    Waiting,
    Baking,
    Ready,
    Stale,
    Failed,
}

impl IndirectPhase {
    pub fn name(self) -> &'static str {
        match self {
            IndirectPhase::Off => "off",
            IndirectPhase::NoLights => "no_lights",
            IndirectPhase::Waiting => "waiting",
            IndirectPhase::Baking => "baking",
            IndirectPhase::Ready => "ready",
            IndirectPhase::Stale => "stale",
            IndirectPhase::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IndirectStatus {
    pub phase: IndirectPhase,
    pub cells: u32,
    pub valid_cells: u32,
    pub spacing: f32,
    pub gpu_bytes: u64,
    pub seconds: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RelightStats {
    pub jobs: u64,
    pub full: bool,
    pub lights: usize,
    pub changed_lights: usize,
    pub relit_faces: usize,
    pub updated_faces: usize,
    pub seconds: f32,
    pub direct_seconds: f32,
    pub bounce_seconds: f32,
    pub scene_seconds: f32,
    pub turnaround_seconds: f32,
    pub host_pid: Option<u32>,
    pub host_mb: u64,
    pub session_mb: u64,
    pub warm: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreparedStatus {
    pub preparing: bool,
    pub ready: bool,
    pub jobs: u64,
    pub seconds: f32,
    pub streamed_faces: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightCostEntry {
    pub entity: Option<u32>,
    pub classname: String,
    pub origin: [f32; 3],
    pub faces: usize,
    pub luxels: u64,
}

#[derive(Debug, Default)]
pub struct LightingRuntime {
    pub relight: RelightStats,
    pub prepared: PreparedStatus,
    pub costs: Option<Vec<LightCostEntry>>,
    pub costs_wanted: bool,
    pub progress: Option<BakeProgress>,
    pub last: Option<BakeSummary>,
    pub has_bake: bool,
    pub stale: bool,
    pub stats: PreviewStats,
    pub budget: Option<BudgetSummary>,
    pub budget_wanted: bool,
    pub no_lights: bool,
    pub indirect: IndirectStatus,
    pub paused: bool,
}
