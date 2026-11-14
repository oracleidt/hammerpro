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
    pub tier: MemoryTier,
    pub hardware: GpuHardware,
    pub lightmap_layers: u32,
    pub squeezed_textures: u64,
    pub device_losses: u64,
    pub device_recoveries: u64,
    pub recovering: bool,
    pub last_device_loss: Option<String>,
    pub simulate_loss_request: bool,
}

impl GpuRuntime {
    pub fn fraction(&self) -> f32 {
        if self.budget == 0 { 0.0 } else { (self.usage as f64 / self.budget as f64) as f32 }
    }
}

const MB: u64 = 1 << 20;
const GB: u64 = 1 << 30;
const LOW_DEDICATED: u64 = 3 * GB;
const HIGH_DEDICATED: u64 = 12 * GB;
const LOW_SHARE: f64 = 0.5;
const LOW_CAP: u64 = GB;
const LOW_FLOOR: u64 = 384 * MB;
const NORMAL_SHARE: f64 = 0.9;
const HIGH_SHARE: f64 = 0.95;
const SHARED_CAP: u64 = 2 * GB;
const LOW_LIGHTMAP_SHARE: f64 = 0.12;
const LOW_TEXTURE_LIMIT: u32 = 1024;
const LOW_THUMBNAILS: usize = 600;
const NORMAL_THUMBNAILS: usize = 3000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuHardware {
    pub dedicated: u64,
    pub driver_budget: u64,
    pub integrated: bool,
    pub simulated: bool,
}

impl GpuHardware {
    pub fn with_simulation(mut self, value: Option<&str>) -> Self {
        let Some(v) = value.map(str::trim).filter(|v| !v.is_empty() && *v != "0") else { return self };
        let mb: u64 = v.parse().ok().filter(|m| *m > 1).unwrap_or(128);
        self.dedicated = mb * MB;
        self.integrated = true;
        self.simulated = true;
        self
    }

    pub fn with_forced_budget(mut self, budget: Option<u64>) -> Self {
        if let Some(b) = budget.filter(|b| *b > 0 && *b < LOW_DEDICATED) {
            self.driver_budget = b;
            self.simulated = true;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryTier {
    Low,
    #[default]
    Normal,
    High,
}

impl MemoryTier {
    pub const fn id(self) -> &'static str {
        match self {
            MemoryTier::Low => "low",
            MemoryTier::Normal => "normal",
            MemoryTier::High => "high",
        }
    }

    pub fn resolve(setting: crate::settings::GraphicsMemory, hw: GpuHardware) -> MemoryTier {
        use crate::settings::GraphicsMemory;
        match setting {
            GraphicsMemory::Low => MemoryTier::Low,
            GraphicsMemory::Normal => MemoryTier::Normal,
            GraphicsMemory::High => MemoryTier::High,
            GraphicsMemory::Auto => {
                let small_card = hw.dedicated > 0 && hw.dedicated < LOW_DEDICATED;
                let small_budget = hw.driver_budget > 0 && hw.driver_budget < LOW_DEDICATED;
                if hw.integrated || small_card || small_budget {
                    MemoryTier::Low
                } else if hw.dedicated >= HIGH_DEDICATED {
                    MemoryTier::High
                } else {
                    MemoryTier::Normal
                }
            }
        }
    }

    pub fn budget(self, driver_budget: Option<u64>, fallback: u64, shared: bool) -> u64 {
        let driver = driver_budget.filter(|b| *b > 0);
        match self {
            MemoryTier::Low => {
                let base = driver.unwrap_or(fallback);
                ((base as f64 * LOW_SHARE) as u64).clamp(LOW_FLOOR.min(base), LOW_CAP)
            }
            MemoryTier::Normal => {
                let b = driver.map_or(fallback, |b| (b as f64 * NORMAL_SHARE) as u64);
                if shared { b.min(SHARED_CAP) } else { b }
            }
            MemoryTier::High => driver.map_or(fallback, |b| (b as f64 * HIGH_SHARE) as u64),
        }
    }

    pub fn plan_cap(self) -> Option<super::perf::PerfPlan> {
        use super::perf::{PerfPlan, Reflections};
        (self == MemoryTier::Low).then_some(PerfPlan {
            scale: 1.0,
            msaa: false,
            light_shadows: 4,
            sun_shadow_size: 1024,
            local_shadow_size: 256,
            bloom: true,
            texture_limit: LOW_TEXTURE_LIMIT,
            reflections: Reflections::World,
            indirect: true,
            cubemaps: true,
            normal_maps: true,
            fog_volumes: true,
            sun_glow: true,
            sky_occlusion: false,
            model_distance: 0.0,
        })
    }

    pub fn lightmap_layers(self, budget: u64, layer_size: u32, device_max: u32) -> u32 {
        let device_max = device_max.max(1);
        if self != MemoryTier::Low {
            return device_max;
        }
        let layer = u64::from(layer_size.max(1)).pow(2) * 4;
        let fit = ((budget as f64 * LOW_LIGHTMAP_SHARE) as u64 / layer).max(1);
        fit.min(u64::from(device_max)) as u32
    }

    pub fn luxel_capacity(self, layers: u32, layer_size: u32) -> Option<u64> {
        (self == MemoryTier::Low).then(|| u64::from(layers) * u64::from(layer_size).pow(2) * 3 / 20)
    }

    pub fn thumbnails(self) -> usize {
        if self == MemoryTier::Low { LOW_THUMBNAILS } else { NORMAL_THUMBNAILS }
    }

    pub fn low(self) -> bool {
        self == MemoryTier::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::GraphicsMemory;

    fn hw(dedicated_mb: u64, budget_mb: u64, integrated: bool) -> GpuHardware {
        GpuHardware { dedicated: dedicated_mb * MB, driver_budget: budget_mb * MB, integrated, simulated: false }
    }

    #[test]
    fn auto_picks_low_for_integrated_and_small_cards() {
        let intel = hw(128, 3553, true);
        assert_eq!(MemoryTier::resolve(GraphicsMemory::Auto, intel), MemoryTier::Low);
        assert_eq!(MemoryTier::resolve(GraphicsMemory::Auto, hw(2048, 1900, false)), MemoryTier::Low);
        assert_eq!(MemoryTier::resolve(GraphicsMemory::Auto, hw(8192, 7000, false)), MemoryTier::Normal);
        assert_eq!(MemoryTier::resolve(GraphicsMemory::Auto, hw(16384, 15000, false)), MemoryTier::High);
        assert_eq!(MemoryTier::resolve(GraphicsMemory::Auto, hw(0, 0, false)), MemoryTier::Normal);
        assert_eq!(MemoryTier::resolve(GraphicsMemory::Normal, intel), MemoryTier::Normal, "a choice wins");
        assert_eq!(MemoryTier::resolve(GraphicsMemory::Low, hw(24576, 23000, false)), MemoryTier::Low);
    }

    #[test]
    fn simulation_makes_a_small_integrated_card() {
        let big = hw(16384, 15000, false);
        assert_eq!(big.with_simulation(None), big);
        assert_eq!(big.with_simulation(Some("0")), big);
        let sim = big.with_simulation(Some("1"));
        assert!(sim.integrated && sim.simulated && sim.dedicated == 128 * MB);
        assert_eq!(big.with_simulation(Some("512")).dedicated, 512 * MB);
        assert_eq!(MemoryTier::resolve(GraphicsMemory::Auto, sim), MemoryTier::Low);
        let forced = big.with_forced_budget(Some(512 * MB));
        assert!(forced.simulated && forced.driver_budget == 512 * MB);
        assert_eq!(
            MemoryTier::resolve(GraphicsMemory::Auto, forced),
            MemoryTier::Low,
            "a small forced budget tests Low"
        );
        assert_eq!(big.with_forced_budget(Some(4 * GB)), big, "a big forced budget keeps the card's mode");
        assert_eq!(big.with_forced_budget(None), big);
    }

    #[test]
    fn the_crashing_laptop_gets_a_budget_well_below_its_limit() {
        let low = MemoryTier::Low.budget(Some(3553 * MB), 1536 * MB, true);
        assert_eq!(low, GB, "half of 3553 MB, at most 1 GB");
        assert_eq!(MemoryTier::Low.budget(Some(600 * MB), 1536 * MB, true), 384 * MB);
        assert_eq!(MemoryTier::Low.budget(Some(300 * MB), 1536 * MB, true), 300 * MB, "never above the driver budget");
        assert_eq!(MemoryTier::Low.budget(None, 1536 * MB, true), 768 * MB);
        assert_eq!(MemoryTier::Normal.budget(Some(16 * GB), 0, true), SHARED_CAP);
        assert_eq!(MemoryTier::Normal.budget(Some(8 * GB), 0, false), ((8 * GB) as f64 * 0.9) as u64);
        assert_eq!(MemoryTier::High.budget(Some(16 * GB), 0, true), ((16 * GB) as f64 * 0.95) as u64);
    }

    #[test]
    fn low_caps_quality_and_lightmaps() {
        let cap = MemoryTier::Low.plan_cap().expect("a cap");
        assert!(!cap.msaa);
        assert_eq!(cap.texture_limit, LOW_TEXTURE_LIMIT);
        assert!(MemoryTier::Normal.plan_cap().is_none() && MemoryTier::High.plan_cap().is_none());
        assert_eq!(MemoryTier::Low.lightmap_layers(GB, 2048, 256), 7);
        assert_eq!(MemoryTier::Normal.lightmap_layers(GB, 2048, 64), 64);
        assert_eq!(MemoryTier::Low.lightmap_layers(64 * MB, 2048, 256), 1);
        assert!(MemoryTier::Low.luxel_capacity(7, 2048).is_some_and(|c| c > 757_601));
        assert_eq!(MemoryTier::Normal.luxel_capacity(7, 2048), None);
        assert!(MemoryTier::Low.thumbnails() < MemoryTier::Normal.thumbnails());
        let full = super::super::perf::PerfPlan::FULL.capped(&cap);
        assert_eq!(full, cap, "the full plan takes the cap");
        let very = super::super::perf::PerfLevel::VeryLow.plan();
        assert_eq!(very.capped(&cap).texture_limit, 512, "a stricter plan stays stricter");
    }
}
