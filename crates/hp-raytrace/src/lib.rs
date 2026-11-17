pub mod baker;
pub mod bvh;
pub mod cpu;
pub mod denoise;
pub mod device;
pub mod gpu;
pub mod kernel;
pub mod sampling;
pub mod scene;

pub use baker::{Baker, Control, Output, Phase, Progress, Refinement, Stop, SurfaceOut};
pub use device::{DeviceInfo, DevicePreference, GpuInfo, NO_GPU_MESSAGE, no_gpu_message};
pub use scene::{
    AlphaTexture, Light, LightKind, Luxel, Material, MaterialKind, Point, Scene, SkyCamera, Surface, Triangle,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Quality {
    Draft,
    #[default]
    Normal,
    Final,
    Optimal,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Draft, Quality::Normal, Quality::Final, Quality::Optimal];

    pub fn name(self) -> &'static str {
        match self {
            Quality::Draft => "draft",
            Quality::Normal => "normal",
            Quality::Final => "final",
            Quality::Optimal => "optimal",
        }
    }

    pub fn parse(text: &str) -> Option<Quality> {
        match text.trim().to_ascii_lowercase().as_str() {
            "draft" | "fast" | "0" => Some(Quality::Draft),
            "normal" | "1" => Some(Quality::Normal),
            "final" | "2" => Some(Quality::Final),
            "optimal" | "auto" | "3" => Some(Quality::Optimal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub min_samples: u32,
    pub max_samples: u32,
    pub bounces: u32,
    pub adaptive_threshold: f32,
    pub light_size_scale: f32,
    pub sun_angle: Option<f32>,
    pub sky_samples: u32,
    pub denoise: f32,
    pub supersample: u32,
    pub chunk_luxels: u32,
    pub light_radius: f32,
    pub sun_soft_angle: f32,
    pub luxel_detail: f32,
    pub contact_ao: bool,
    pub cinematic: bool,
}

impl Params {
    pub fn of(q: Quality) -> Params {
        match q {
            Quality::Draft => Params {
                min_samples: 16,
                max_samples: 64,
                bounces: 3,
                adaptive_threshold: 0.08,
                light_size_scale: 1.0,
                sun_angle: None,
                sky_samples: 1,
                denoise: 1.0,
                supersample: 2,
                chunk_luxels: 0,
                light_radius: 2.0,
                sun_soft_angle: 0.0,
                luxel_detail: 1.0,
                contact_ao: false,
                cinematic: false,
            },
            Quality::Normal | Quality::Optimal => Params {
                min_samples: 64,
                max_samples: 256,
                bounces: 6,
                adaptive_threshold: 0.04,
                light_size_scale: 1.0,
                sun_angle: None,
                sky_samples: 1,
                denoise: 1.0,
                supersample: 3,
                chunk_luxels: 0,
                light_radius: if q == Quality::Optimal { 6.0 } else { 2.0 },
                sun_soft_angle: if q == Quality::Optimal { 1.0 } else { 0.0 },
                luxel_detail: 1.0,
                contact_ao: q == Quality::Optimal,
                cinematic: q == Quality::Optimal,
            },
            Quality::Final => Params {
                min_samples: 256,
                max_samples: 1024,
                bounces: 12,
                adaptive_threshold: 0.02,
                light_size_scale: 1.0,
                sun_angle: None,
                sky_samples: 2,
                denoise: 1.0,
                supersample: 4,
                chunk_luxels: 0,
                light_radius: 8.0,
                sun_soft_angle: 1.5,
                luxel_detail: 2.0,
                contact_ao: true,
                cinematic: true,
            },
        }
    }

    pub fn sanitized(mut self) -> Params {
        self.min_samples = self.min_samples.clamp(1, 1 << 16);
        self.max_samples = self.max_samples.clamp(self.min_samples, 1 << 16);
        self.bounces = self.bounces.min(64);
        self.adaptive_threshold =
            if self.adaptive_threshold.is_finite() { self.adaptive_threshold.max(0.0) } else { 0.0 };
        self.light_size_scale =
            if self.light_size_scale.is_finite() { self.light_size_scale.clamp(0.0, 64.0) } else { 1.0 };
        self.sun_angle = self.sun_angle.filter(|a| a.is_finite()).map(|a| a.clamp(0.0, 45.0));
        self.sky_samples = self.sky_samples.clamp(1, 64);
        self.denoise = if self.denoise.is_finite() { self.denoise.clamp(0.0, 4.0) } else { 1.0 };
        self.supersample = self.supersample.clamp(1, 8);
        self.light_radius = if self.light_radius.is_finite() { self.light_radius.clamp(0.0, 256.0) } else { 2.0 };
        self.luxel_detail = if self.luxel_detail.is_finite() { self.luxel_detail.clamp(1.0, 4.0) } else { 1.0 };
        self.sun_soft_angle = if self.sun_soft_angle.is_finite() { self.sun_soft_angle.clamp(0.0, 45.0) } else { 0.0 };
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub params: Params,
    pub seed: u32,
    pub style_bounce: bool,
    pub device: DevicePreference,
    pub threads: Option<usize>,
    pub vram_budget: Option<u64>,
    pub ao_radius: f32,
    pub submit_seconds: f64,
    pub duty: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            params: Params::of(Quality::Normal),
            seed: 0,
            style_bounce: true,
            device: DevicePreference::Auto,
            threads: None,
            vram_budget: None,
            ao_radius: 0.0,
            submit_seconds: 0.08,
            duty: 1.0,
        }
    }
}
