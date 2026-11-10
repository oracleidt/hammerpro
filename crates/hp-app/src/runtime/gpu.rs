#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VramSource {
    #[default]
    Estimate,
    Driver,
}

impl VramSource {
    pub fn id(self) -> &'static str {
        match self {
            VramSource::Estimate => "estimate",
            VramSource::Driver => "driver",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum VramPressure {
    #[default]
    Normal,
    High,
    Critical,
}

impl VramPressure {
    pub fn id(self) -> &'static str {
        match self {
            VramPressure::Normal => "normal",
            VramPressure::High => "high",
            VramPressure::Critical => "critical",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureStreaming {
    pub enabled: bool,
    pub active: bool,
    pub textures: usize,
    pub reduced: usize,
    pub pending: usize,
    pub bytes: u64,
    pub full_bytes: u64,
    pub budget: u64,
    pub aggression: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuRelease {
    BackgroundMaps,
    Caches,
    Textures(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuReleased {
    pub background_maps: usize,
    pub textures: usize,
    pub models: usize,
    pub thumbnails: usize,
    pub reloading: usize,
}

impl GpuReleased {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn add(&mut self, o: GpuReleased) {
        self.background_maps += o.background_maps;
        self.textures += o.textures;
        self.models += o.models;
        self.thumbnails += o.thumbnails;
        self.reloading += o.reloading;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VramBreakdown {
    pub geometry: u64,
    pub textures: u64,
    pub models: u64,
    pub lightmaps: u64,
    pub compiled: u64,
    pub other: u64,
    pub background_maps: u64,
}

impl VramBreakdown {
    pub fn total(&self) -> u64 {
        self.geometry + self.textures + self.models + self.lightmaps + self.compiled + self.other + self.background_maps
    }
}

#[derive(Clone, Debug, Default)]
pub struct GpuRuntime {
    pub budget: u64,
    pub budget_is_auto: bool,
    pub usage: u64,
    pub source: VramSource,
    pub driver_budget: u64,
    pub dedicated: u64,
    pub estimate: VramBreakdown,
    pub background_maps: usize,
    pub texture_limit: u32,
    pub max_texture: u32,
    pub pressure: VramPressure,
    pub out_of_memory_events: u64,
    pub releases: u64,
    pub released: GpuReleased,
    pub last_release: Option<String>,
    pub warning: Option<String>,
    pub release_request: Option<bool>,
    pub peak_usage: u64,
}

impl GpuRuntime {
    pub fn fraction(&self) -> f32 {
        if self.budget == 0 { 0.0 } else { (self.usage as f64 / self.budget as f64) as f32 }
    }
}
