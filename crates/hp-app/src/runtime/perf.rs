use crate::settings::PerformanceMode;

pub const FLOOR_FPS: f32 = 20.0;
pub const WINDOW_SECS: f64 = 10.0;
pub const WINDOW_SECS_WEAK: f64 = 6.0;
const SLOW_SHARE: f64 = 0.85;
const MIN_FRAMES: u32 = 45;
const MAX_GAP: f32 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PerfLevel {
    #[default]
    Full,
    Low,
    VeryLow,
}

impl PerfLevel {
    pub const fn id(self) -> &'static str {
        match self {
            PerfLevel::Full => "full",
            PerfLevel::Low => "low",
            PerfLevel::VeryLow => "very_low",
        }
    }

    pub fn lower(self) -> Option<PerfLevel> {
        match self {
            PerfLevel::Full => Some(PerfLevel::Low),
            PerfLevel::Low => Some(PerfLevel::VeryLow),
            PerfLevel::VeryLow => None,
        }
    }

    pub fn plan(self) -> PerfPlan {
        match self {
            PerfLevel::Full => PerfPlan::FULL,
            PerfLevel::Low => PerfPlan {
                scale: 0.75,
                msaa: false,
                light_shadows: 4,
                sun_shadow_size: 2048,
                local_shadow_size: 256,
                bloom: false,
                texture_limit: 1024,
                reflections: Reflections::World,
                indirect: false,
                cubemaps: true,
                normal_maps: true,
                fog_volumes: true,
                sun_glow: true,
                sky_occlusion: false,
                model_distance: 8192.0,
            },
            PerfLevel::VeryLow => PerfPlan {
                scale: 0.5,
                msaa: false,
                light_shadows: 0,
                sun_shadow_size: 1024,
                local_shadow_size: 128,
                bloom: false,
                texture_limit: 512,
                reflections: Reflections::Simple,
                indirect: false,
                cubemaps: false,
                normal_maps: false,
                fog_volumes: false,
                sun_glow: false,
                sky_occlusion: false,
                model_distance: 3072.0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reflections {
    All,
    World,
    Simple,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerfPlan {
    pub scale: f32,
    pub msaa: bool,
    pub light_shadows: u32,
    pub sun_shadow_size: u32,
    pub local_shadow_size: u32,
    pub bloom: bool,
    pub texture_limit: u32,
    pub reflections: Reflections,
    pub indirect: bool,
    pub cubemaps: bool,
    pub normal_maps: bool,
    pub fog_volumes: bool,
    pub sun_glow: bool,
    pub sky_occlusion: bool,
    pub model_distance: f32,
}

impl PerfPlan {
    pub const FULL: PerfPlan = PerfPlan {
        scale: 1.0,
        msaa: true,
        light_shadows: u32::MAX,
        sun_shadow_size: u32::MAX,
        local_shadow_size: u32::MAX,
        bloom: true,
        texture_limit: u32::MAX,
        reflections: Reflections::All,
        indirect: true,
        cubemaps: true,
        normal_maps: true,
        fog_volumes: true,
        sun_glow: true,
        sky_occlusion: true,
        model_distance: 0.0,
    };

    pub fn is_full(&self) -> bool {
        *self == Self::FULL
    }

    pub fn model_distance_for(&self, user: f32) -> f32 {
        if self.model_distance <= 0.0 {
            user
        } else if user > 0.0 {
            user.min(self.model_distance)
        } else {
            self.model_distance
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerfSample {
    pub frame_ms: f32,
    pub prepare_ms: f32,
    pub record_ms: f32,
    pub submit_ms: f32,
    pub setup_ms: f32,
    pub clusters_ms: f32,
    pub shadow_prep_ms: f32,
    pub models_ms: f32,
    pub reflection_cpu_ms: f32,
    pub gpu_supported: bool,
    pub gpu_total_ms: f32,
    pub gpu_shadows_ms: f32,
    pub gpu_scene_ms: f32,
    pub gpu_post_ms: f32,
    pub gpu_reflection_ms: f32,
    pub gpu_passes: GpuPasses,
    pub gpu_reflection_passes: GpuPasses,
    pub working_set: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuPasses {
    pub light_cull_ms: f32,
    pub sun_shadows_ms: f32,
    pub local_shadows_ms: f32,
    pub main_ms: f32,
    pub effects_ms: f32,
    pub present_ms: f32,
    pub outline_ms: f32,
}

impl GpuPasses {
    pub fn add(&mut self, o: &GpuPasses) {
        self.light_cull_ms += o.light_cull_ms;
        self.sun_shadows_ms += o.sun_shadows_ms;
        self.local_shadows_ms += o.local_shadows_ms;
        self.main_ms += o.main_ms;
        self.effects_ms += o.effects_ms;
        self.present_ms += o.present_ms;
        self.outline_ms += o.outline_ms;
    }

    pub fn scaled(&self, k: f32) -> GpuPasses {
        GpuPasses {
            light_cull_ms: self.light_cull_ms * k,
            sun_shadows_ms: self.sun_shadows_ms * k,
            local_shadows_ms: self.local_shadows_ms * k,
            main_ms: self.main_ms * k,
            effects_ms: self.effects_ms * k,
            present_ms: self.present_ms * k,
            outline_ms: self.outline_ms * k,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BenchRequest {
    pub frames: u32,
    pub trace: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BenchFrame {
    pub frame_ms: f32,
    pub cpu_ms: f32,
    pub gpu_ms: f32,
    pub views_ms: f32,
    pub view_render_ms: f32,
    pub view_overlay_ms: f32,
    pub app_ms: f32,
    pub gpu_passes: GpuPasses,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BenchResult {
    pub frames: u32,
    pub seconds: f32,
    pub avg_fps: f32,
    pub frame_ms_avg: f32,
    pub frame_ms_p95: f32,
    pub frame_ms_p99: f32,
    pub frame_ms_max: f32,
    pub frame_ms_median: f32,
    pub cpu_ms_median: f32,
    pub cpu_ms_p99: f32,
    pub gpu_ms_p99: f32,
    pub gpu_passes_ms_avg: GpuPasses,
    pub gpu_reflection_passes_ms_avg: GpuPasses,
    pub trace: Vec<BenchFrame>,
    pub gpu_ms_median: f32,
    pub draw_calls_avg: f32,
    pub triangles_avg: f32,
    pub model_instances_avg: f32,
    pub cpu_ms_avg: f32,
    pub cpu_phases_ms_avg: [f32; 8],
    pub gpu_ms_avg: f32,
    pub gpu_shadows_ms_avg: f32,
    pub gpu_scene_ms_avg: f32,
    pub gpu_post_ms_avg: f32,
    pub gpu_reflection_ms_avg: f32,
    pub working_set_start_mb: u64,
    pub working_set_end_mb: u64,
    pub working_set_peak_mb: u64,
    pub views_rendered: u64,
}

pub const FRAME_PHASES: [&str; 9] =
    ["begin", "panels", "tool", "sync", "lighting", "views", "chrome", "dialogs", "end"];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FramePhases {
    pub total_ms: f32,
    pub ms: [f32; 9],
}

impl FramePhases {
    pub fn add(&mut self, other: &FramePhases) {
        self.total_ms += other.total_ms;
        for (a, b) in self.ms.iter_mut().zip(other.ms) {
            *a += b;
        }
    }

    pub fn divided(&self, count: u32) -> FramePhases {
        let n = count.max(1) as f32;
        FramePhases { total_ms: self.total_ms / n, ms: self.ms.map(|m| m / n) }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewCost {
    pub overlay_ms: f32,
    pub render_ms: f32,
    pub paint_ms: f32,
    pub overlay_items: u32,
    pub rendered: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepaintCount {
    pub at: String,
    pub count: u64,
}

#[derive(Clone, Debug, Default)]
pub struct PerfRuntime {
    pub views: [ViewCost; 4],
    pub frame_phases: FramePhases,
    pub worst_frame: FramePhases,
    pub phase_sum: FramePhases,
    pub phase_frames: u32,
    pub repaint_causes: Vec<RepaintCount>,
    pub frames_from_events: u64,
    pub continuous_frames: u64,
    pub bench_request: Option<BenchRequest>,
    pub bench_running: bool,
    pub bench_result: Option<BenchResult>,
    pub sample: PerfSample,
    pub adapter: String,
    pub integrated: bool,
    pub auto_level: PerfLevel,
    pub auto_locked: bool,
    pub auto_fps: f32,
    pub auto_events: u32,
    window: f64,
    slow: f64,
    frames: u32,
    slow_frames: u32,
    pub restore_request: bool,
}

impl PerfRuntime {
    pub fn level(&self, mode: PerformanceMode) -> PerfLevel {
        match mode {
            PerformanceMode::Off => PerfLevel::Full,
            PerformanceMode::Low => PerfLevel::Low,
            PerformanceMode::VeryLow => PerfLevel::VeryLow,
            PerformanceMode::Auto => self.auto_level,
        }
    }

    pub fn plan(&self, mode: PerformanceMode) -> PerfPlan {
        self.level(mode).plan()
    }

    pub fn reduced_automatically(&self, mode: PerformanceMode) -> bool {
        mode == PerformanceMode::Auto && self.auto_level != PerfLevel::Full
    }

    pub fn restore(&mut self) {
        self.auto_level = PerfLevel::Full;
        self.auto_locked = true;
        self.reset_window();
    }

    pub fn unlock(&mut self) {
        self.auto_locked = false;
        self.reset_window();
    }

    fn reset_window(&mut self) {
        self.window = 0.0;
        self.slow = 0.0;
        self.frames = 0;
        self.slow_frames = 0;
    }

    pub fn observe_frame(
        &mut self,
        mode: PerformanceMode,
        rendered: bool,
        continuous: bool,
        busy: bool,
        dt: f32,
    ) -> Option<PerfLevel> {
        self.observe(mode, rendered && continuous, busy, dt)
    }

    pub fn observe(&mut self, mode: PerformanceMode, rendered: bool, busy: bool, dt: f32) -> Option<PerfLevel> {
        if mode != PerformanceMode::Auto || self.auto_locked {
            self.reset_window();
            return None;
        }
        if busy || !rendered || !dt.is_finite() || dt <= 0.0 || dt > MAX_GAP {
            if busy || dt > MAX_GAP {
                self.reset_window();
            }
            return None;
        }
        let limit = 1.0 / FLOOR_FPS;
        self.window += f64::from(dt);
        self.frames += 1;
        if dt > limit {
            self.slow += f64::from(dt);
            self.slow_frames += 1;
        }
        let needed = if self.integrated { WINDOW_SECS_WEAK } else { WINDOW_SECS };
        if self.window < needed {
            return None;
        }
        let sustained = self.frames >= MIN_FRAMES && self.slow >= self.window * SLOW_SHARE;
        let fps = if self.window > 0.0 { self.frames as f32 / self.window as f32 } else { 0.0 };
        self.reset_window();
        if !sustained {
            return None;
        }
        let next = self.auto_level.lower()?;
        self.auto_level = next;
        self.auto_fps = fps;
        self.auto_events += 1;
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(rt: &mut PerfRuntime, mode: PerformanceMode, secs: f64, dt: f32) -> Option<PerfLevel> {
        let mut last = None;
        for _ in 0..(secs / f64::from(dt)) as u32 {
            if let Some(l) = rt.observe(mode, true, false, dt) {
                last = Some(l);
            }
        }
        last
    }

    #[test]
    fn full_speed_never_lowers_anything() {
        let mut rt = PerfRuntime::default();
        assert_eq!(run(&mut rt, PerformanceMode::Auto, 60.0, 0.016), None);
        assert_eq!(rt.auto_level, PerfLevel::Full);
        assert!(rt.plan(PerformanceMode::Auto).is_full());
    }

    #[test]
    fn a_sustained_very_low_rate_steps_down_once_per_window() {
        let mut rt = PerfRuntime::default();
        assert_eq!(run(&mut rt, PerformanceMode::Auto, 11.0, 0.06), Some(PerfLevel::Low));
        assert!(rt.reduced_automatically(PerformanceMode::Auto));
        assert_eq!(run(&mut rt, PerformanceMode::Auto, 11.0, 0.06), Some(PerfLevel::VeryLow));
        assert_eq!(run(&mut rt, PerformanceMode::Auto, 30.0, 0.06), None);
        assert_eq!(rt.auto_level, PerfLevel::VeryLow);
        rt.restore();
        assert_eq!(rt.auto_level, PerfLevel::Full);
        assert_eq!(run(&mut rt, PerformanceMode::Auto, 30.0, 0.06), None, "restoring locks Auto");
        rt.unlock();
        assert_eq!(run(&mut rt, PerformanceMode::Auto, 11.0, 0.06), Some(PerfLevel::Low));
    }

    #[test]
    fn stalls_loading_and_idle_do_not_count() {
        let mut rt = PerfRuntime::default();
        for _ in 0..400 {
            assert_eq!(rt.observe(PerformanceMode::Auto, true, true, 0.1), None);
        }
        for _ in 0..400 {
            assert_eq!(rt.observe(PerformanceMode::Auto, false, false, 0.1), None);
        }
        assert_eq!(run(&mut rt, PerformanceMode::Auto, 30.0, 0.7), None);
        assert_eq!(rt.auto_level, PerfLevel::Full);
    }

    #[test]
    fn frames_spaced_out_on_purpose_never_lower_the_quality() {
        let mut rt = PerfRuntime::default();
        for _ in 0..600 {
            assert_eq!(rt.observe_frame(PerformanceMode::Auto, true, false, false, 0.1), None);
        }
        assert_eq!(rt.auto_level, PerfLevel::Full, "a 10 Hz light animation is not a slow machine");
        let mut lowered = None;
        for _ in 0..200 {
            lowered = lowered.or(rt.observe_frame(PerformanceMode::Auto, true, true, false, 0.1));
        }
        assert_eq!(lowered, Some(PerfLevel::Low), "continuous drawing at 10 FPS still counts");
    }

    #[test]
    fn explicit_modes_are_exact_and_off_stays_full() {
        let rt = PerfRuntime { auto_level: PerfLevel::VeryLow, ..Default::default() };
        assert_eq!(rt.level(PerformanceMode::Off), PerfLevel::Full);
        assert_eq!(rt.level(PerformanceMode::Low), PerfLevel::Low);
        assert_eq!(rt.level(PerformanceMode::VeryLow), PerfLevel::VeryLow);
        assert_eq!(rt.level(PerformanceMode::Auto), PerfLevel::VeryLow);
        let low = PerfLevel::Low.plan();
        assert_eq!(low.model_distance_for(0.0), 8192.0);
        assert_eq!(low.model_distance_for(2000.0), 2000.0);
        assert_eq!(PerfPlan::FULL.model_distance_for(1234.0), 1234.0);
    }
}
