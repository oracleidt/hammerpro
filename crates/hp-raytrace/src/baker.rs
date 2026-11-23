use crate::device::DeviceInfo;
use crate::scene::Scene;
use crate::{Params, Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Upload,
    Direct,
    Indirect,
    Denoise,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub phase: Phase,
    pub fraction: f32,
    pub seconds_left: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stop::Cancelled => f.write_str("cancelled"),
            Stop::Failed(m) => f.write_str(m),
        }
    }
}

impl From<String> for Stop {
    fn from(m: String) -> Self {
        Stop::Failed(m)
    }
}

pub struct Control<'a> {
    pub cancelled: &'a (dyn Fn() -> bool + Sync),
    pub progress: &'a (dyn Fn(Progress) + Sync),
}

impl Control<'_> {
    pub fn none() -> Control<'static> {
        Control { cancelled: &|| false, progress: &|_| {} }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceOut {
    pub direct: Vec<[f32; 3]>,
    pub indirect: Vec<[f32; 3]>,
    pub ao: Vec<f32>,
    pub samples: f32,
}

impl SurfaceOut {
    pub fn total(&self) -> Vec<[f32; 3]> {
        self.direct.iter().zip(&self.indirect).map(|(a, b)| [a[0] + b[0], a[1] + b[1], a[2] + b[2]]).collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    pub surfaces: Vec<SurfaceOut>,
    pub direct_samples: u64,
    pub indirect_samples: u64,
    pub rays: u64,
    pub seconds: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Refinement {
    pub surfaces: Vec<u32>,
    pub progress: f32,
    pub samples_per_luxel: f32,
    pub finished: bool,
    pub seconds: f64,
}

pub struct Baker {
    pub(crate) scene: std::sync::Arc<Scene>,
    pub(crate) settings: Settings,
    pub(crate) device: DeviceInfo,
    pub(crate) state: crate::cpu::State,
    pub(crate) backend: Backend,
}

pub(crate) enum Backend {
    Cpu,
    Gpu(Box<crate::gpu::Gpu>),
}

impl Baker {
    pub fn new(scene: Scene, settings: Settings, log: &mut dyn FnMut(&str)) -> Result<Baker, String> {
        scene.validate()?;
        let scene = std::sync::Arc::new(scene);
        let settings = Settings { params: settings.params.sanitized(), ..settings };
        let threads = settings.threads.unwrap_or_else(rayon::current_num_threads).max(1);
        let state = crate::cpu::State::new(&scene)?;
        let (backend, device) = crate::gpu::Gpu::open(&scene, &state, &settings, log)
            .map(|(g, info)| (Backend::Gpu(Box::new(g)), info))
            .unwrap_or_else(|why| {
                if let Some(n) = &why.note {
                    log(n);
                }
                let no_gpu = why.no_gpu && settings.device != crate::DevicePreference::Cpu;
                (Backend::Cpu, DeviceInfo::cpu(threads, no_gpu, why.note))
            });
        Ok(Baker { scene, settings, device, state, backend })
    }

    pub fn device(&self) -> &DeviceInfo {
        &self.device
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn params(&self) -> Params {
        self.settings.params
    }

    pub fn set_pacing(&mut self, submit_seconds: f64, duty: f32) {
        self.settings.submit_seconds = submit_seconds;
        self.settings.duty = duty;
        if let Backend::Gpu(g) = &mut self.backend {
            g.set_pacing(submit_seconds, duty);
        }
    }

    pub fn set_params(&mut self, params: Params) {
        self.settings.params = params.sanitized();
    }

    pub fn on_gpu(&self) -> bool {
        matches!(self.backend, Backend::Gpu(_))
    }

    pub fn set_lights(
        &mut self,
        lights: Vec<crate::Light>,
        light_lists: Vec<u32>,
        ranges: Vec<(u32, u32)>,
    ) -> Result<(), String> {
        let mut scene = (*self.scene).clone();
        if ranges.len() != scene.surfaces.len() {
            return Err("one light range per surface expected".to_owned());
        }
        scene.lights = lights;
        scene.light_lists = light_lists;
        for (s, r) in scene.surfaces.iter_mut().zip(ranges) {
            s.lights = r;
        }
        scene.validate()?;
        self.scene = std::sync::Arc::new(scene);
        self.state = crate::cpu::State::new(&self.scene)?;
        if let Backend::Gpu(g) = &mut self.backend {
            g.upload_lights(&self.scene, &self.state)?;
        }
        Ok(())
    }

    pub fn bake(&mut self, ctl: &Control<'_>, log: &mut dyn FnMut(&str)) -> Result<Output, Stop> {
        crate::cpu::bake(self, ctl, log)
    }

    pub fn refine(&mut self, ctl: &Control<'_>, seconds: f32, focus: Option<&[u32]>) -> Result<Refinement, Stop> {
        crate::cpu::refine(self, ctl, seconds, focus)
    }

    pub fn output(&self, denoise: bool) -> Output {
        crate::cpu::output(self, denoise)
    }

    pub fn output_surfaces(&self, surfaces: &[u32], denoise: bool) -> Vec<(u32, SurfaceOut)> {
        crate::cpu::output_some(self, surfaces, denoise)
    }

    pub fn tracer(&self) -> crate::kernel::Ctx<'_> {
        crate::kernel::Ctx { scene: &self.scene, packed: &self.state.packed, tuning: crate::cpu::tuning(self) }
    }

    pub fn finished(&self) -> bool {
        self.state.finished
    }
}
