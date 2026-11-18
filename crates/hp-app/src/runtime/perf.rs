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
    Lightweight,
    Low,
    VeryLow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LowEnd {
    Integrated,
    SmallVram,
    Battery,
}

impl LowEnd {
    pub const fn id(self) -> &'static str {
        match self {
            LowEnd::Integrated => "integrated_graphics",
            LowEnd::SmallVram => "small_graphics_memory",
            LowEnd::Battery => "on_battery",
        }
    }

    pub fn detect(integrated: bool, dedicated: u64, on_battery: bool) -> Option<LowEnd> {
        if integrated {
            Some(LowEnd::Integrated)
        } else if dedicated > 0 && dedicated < LOW_END_VRAM {
            Some(LowEnd::SmallVram)
        } else if on_battery {
            Some(LowEnd::Battery)
        } else {
            None
        }
    }
}

pub const LOW_END_VRAM: u64 = 2560 << 20;

impl PerfLevel {
    pub const fn id(self) -> &'static str {
        match self {
            PerfLevel::Full => "full",
            PerfLevel::Lightweight => "lightweight",
            PerfLevel::Low => "low",
            PerfLevel::VeryLow => "very_low",
        }
    }

    pub fn lower(self) -> Option<PerfLevel> {
        match self {
            PerfLevel::Full => Some(PerfLevel::Low),
            PerfLevel::Lightweight | PerfLevel::Low => Some(PerfLevel::VeryLow),
            PerfLevel::VeryLow => None,
        }
    }

    pub fn plan(self) -> PerfPlan {
        match self {
            PerfLevel::Full => PerfPlan::FULL,
            PerfLevel::Lightweight => PerfPlan {
                scale: 1.0,
                msaa: false,
                light_shadows: 0,
                sun_shadow_size: 1024,
                local_shadow_size: 128,
                bloom: false,
                texture_limit: 1024,
                reflections: Reflections::Simple,
                indirect: false,
                cubemaps: false,
                normal_maps: false,
                fog_volumes: false,
                sun_glow: false,
                sky_occlusion: false,
                model_distance: 0.0,
            },
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

    pub fn capped(self, cap: &PerfPlan) -> PerfPlan {
        let reflections = match (self.reflections, cap.reflections) {
            (Reflections::Simple, _) | (_, Reflections::Simple) => Reflections::Simple,
            (Reflections::World, _) | (_, Reflections::World) => Reflections::World,
            _ => Reflections::All,
        };
        let distance = match (self.model_distance > 0.0, cap.model_distance > 0.0) {
            (true, true) => self.model_distance.min(cap.model_distance),
            (true, false) => self.model_distance,
            (false, true) => cap.model_distance,
            (false, false) => 0.0,
        };
        PerfPlan {
            scale: self.scale.min(cap.scale),
            msaa: self.msaa && cap.msaa,
            light_shadows: self.light_shadows.min(cap.light_shadows),
            sun_shadow_size: self.sun_shadow_size.min(cap.sun_shadow_size),
            local_shadow_size: self.local_shadow_size.min(cap.local_shadow_size),
            bloom: self.bloom && cap.bloom,
            texture_limit: self.texture_limit.min(cap.texture_limit),
            reflections,
            indirect: self.indirect && cap.indirect,
            cubemaps: self.cubemaps && cap.cubemaps,
            normal_maps: self.normal_maps && cap.normal_maps,
            fog_volumes: self.fog_volumes && cap.fog_volumes,
            sun_glow: self.sun_glow && cap.sun_glow,
            sky_occlusion: self.sky_occlusion && cap.sky_occlusion,
            model_distance: distance,
        }
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
    pub views_2d_ms: f32,
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
        self.views_2d_ms += o.views_2d_ms;
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
            views_2d_ms: self.views_2d_ms * k,
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

thread_local! {
    static SPANS: std::cell::RefCell<Vec<(&'static str, f32)>> = const { std::cell::RefCell::new(Vec::new()) };
}

const SPAN_MIN_MS: f32 = 0.5;
const MAIN_SPAN_DEPTH: usize = 16;

thread_local! {
    static IS_MAIN_THREAD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

static MAIN_SPAN_NAMES: [std::sync::atomic::AtomicPtr<u8>; MAIN_SPAN_DEPTH] =
    [const { std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()) }; MAIN_SPAN_DEPTH];
static MAIN_SPAN_LENS: [std::sync::atomic::AtomicUsize; MAIN_SPAN_DEPTH] =
    [const { std::sync::atomic::AtomicUsize::new(0) }; MAIN_SPAN_DEPTH];
static MAIN_SPAN_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn mark_main_thread() {
    IS_MAIN_THREAD.with(|m| m.set(true));
}

pub fn main_spans() -> Vec<&'static str> {
    use std::sync::atomic::Ordering;
    let count = MAIN_SPAN_COUNT.load(Ordering::Acquire).min(MAIN_SPAN_DEPTH);
    (0..count)
        .filter_map(|i| {
            let ptr = MAIN_SPAN_NAMES[i].load(Ordering::Acquire);
            let len = MAIN_SPAN_LENS[i].load(Ordering::Acquire);
            if ptr.is_null() {
                return None;
            }
            let bytes: &'static [u8] = unsafe { std::slice::from_raw_parts(ptr, len) };
            std::str::from_utf8(bytes).ok()
        })
        .collect()
}

fn push_main_span(name: &'static str) -> bool {
    use std::sync::atomic::Ordering;
    if !IS_MAIN_THREAD.with(std::cell::Cell::get) {
        return false;
    }
    let depth = MAIN_SPAN_COUNT.load(Ordering::Relaxed);
    if depth < MAIN_SPAN_DEPTH {
        MAIN_SPAN_LENS[depth].store(name.len(), Ordering::Release);
        MAIN_SPAN_NAMES[depth].store(name.as_ptr().cast_mut(), Ordering::Release);
    }
    MAIN_SPAN_COUNT.store(depth + 1, Ordering::Release);
    true
}

fn pop_main_span() {
    use std::sync::atomic::Ordering;
    let depth = MAIN_SPAN_COUNT.load(Ordering::Relaxed);
    if depth > 0 {
        MAIN_SPAN_COUNT.store(depth - 1, Ordering::Release);
    }
}

pub struct Span {
    name: &'static str,
    started: std::time::Instant,
    on_main: bool,
}

impl Span {
    pub fn new(name: &'static str) -> Self {
        let on_main = push_main_span(name);
        Self { name, started: std::time::Instant::now(), on_main }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if self.on_main {
            pop_main_span();
        }
        let ms = self.started.elapsed().as_secs_f32() * 1000.0;
        if ms < SPAN_MIN_MS {
            return;
        }
        SPANS.with(|s| {
            let mut s = s.borrow_mut();
            match s.iter_mut().find(|(n, _)| *n == self.name) {
                Some((_, total)) => *total += ms,
                None => s.push((self.name, ms)),
            }
        });
    }
}

pub fn take_spans() -> Vec<(&'static str, f32)> {
    SPANS.with(|s| std::mem::take(&mut *s.borrow_mut()))
}

pub const LONG_FRAME_MS: f32 = 30.0;
pub const LONG_FRAMES_KEPT: usize = 32;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LongFrame {
    pub frame: u64,
    pub at_s: f64,
    pub phases: FramePhases,
    pub spans: Vec<(&'static str, f32)>,
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

pub const REDRAW_REASONS: [&str; 9] = [
    "first",
    "size_or_camera",
    "view_options",
    "content",
    "lighting",
    "animation",
    "retained_overlay",
    "overlay",
    "textures",
];

#[derive(Clone, Debug, Default)]
pub struct PerfRuntime {
    pub redraw_reasons: [[u64; 9]; 4],
    pub views: [ViewCost; 4],
    pub frame_phases: FramePhases,
    pub worst_frame: FramePhases,
    pub long_frames: std::collections::VecDeque<LongFrame>,
    pub long_frame_count: u64,
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
    pub graphics_api: String,
    pub dx12_shader_compiler: Option<String>,
    pub startup_first_frame_ms: Option<u64>,
    pub startup_views_ready_ms: Option<u64>,
    pub auto_level: PerfLevel,
    pub auto_locked: bool,
    pub auto_fps: f32,
    pub auto_events: u32,
    window: f64,
    slow: f64,
    frames: u32,
    slow_frames: u32,
    pub restore_request: bool,
    pub memory_cap: Option<PerfPlan>,
    pub low_end: Option<LowEnd>,
    pub low_end_at: f64,
}

impl PerfRuntime {
    pub fn level(&self, mode: PerformanceMode) -> PerfLevel {
        match mode {
            PerformanceMode::Off => PerfLevel::Full,
            PerformanceMode::Lightweight => PerfLevel::Lightweight,
            PerformanceMode::Low => PerfLevel::Low,
            PerformanceMode::VeryLow => PerfLevel::VeryLow,
            PerformanceMode::Auto => self.auto_level,
        }
    }

    pub fn plan(&self, mode: PerformanceMode) -> PerfPlan {
        let plan = self.level(mode).plan();
        match &self.memory_cap {
            Some(cap) => plan.capped(cap),
            None => plan,
        }
    }

    pub fn lightweight(&self, mode: PerformanceMode) -> bool {
        self.level(mode) == PerfLevel::Lightweight
    }

    pub fn note_low_end(&mut self, mode: PerformanceMode, found: Option<LowEnd>) -> bool {
        if found == self.low_end {
            return false;
        }
        let was = std::mem::replace(&mut self.low_end, found);
        if mode != PerformanceMode::Auto || self.auto_locked {
            return false;
        }
        match (was, found) {
            (_, Some(_)) if self.auto_level == PerfLevel::Full => {
                self.auto_level = PerfLevel::Lightweight;
                self.auto_fps = 0.0;
                self.reset_window();
                true
            }
            (Some(_), None) if self.auto_level == PerfLevel::Lightweight && self.auto_fps == 0.0 => {
                self.auto_level = PerfLevel::Full;
                self.reset_window();
                true
            }
            _ => false,
        }
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

    #[test]
    fn spans_of_the_main_thread_are_visible_to_the_crash_watchdog() {
        std::thread::spawn(|| {
            let _ignored = Span::new("worker_span");
            assert!(main_spans().is_empty());
            mark_main_thread();
            let outer = Span::new("scene_sync");
            {
                let _inner = Span::new("upload");
                assert_eq!(main_spans(), vec!["scene_sync", "upload"]);
            }
            assert_eq!(main_spans(), vec!["scene_sync"]);
            drop(outer);
            assert!(main_spans().is_empty());
        })
        .join()
        .unwrap();
    }

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
    fn weak_hardware_starts_in_the_lightweight_mode() {
        use super::LowEnd;
        assert_eq!(LowEnd::detect(true, 0, false), Some(LowEnd::Integrated));
        assert_eq!(LowEnd::detect(false, 2048 << 20, false), Some(LowEnd::SmallVram));
        assert_eq!(LowEnd::detect(false, 8192 << 20, true), Some(LowEnd::Battery));
        assert_eq!(LowEnd::detect(false, 8192 << 20, false), None);
        assert_eq!(LowEnd::detect(false, 0, false), None, "unknown memory is not a reason");
        let mut rt = PerfRuntime::default();
        assert!(!rt.note_low_end(PerformanceMode::Off, Some(LowEnd::Integrated)), "Off is never changed");
        assert_eq!(rt.level(PerformanceMode::Off), PerfLevel::Full);
        let mut rt = PerfRuntime::default();
        assert!(rt.note_low_end(PerformanceMode::Auto, Some(LowEnd::Battery)));
        assert_eq!(rt.level(PerformanceMode::Auto), PerfLevel::Lightweight);
        assert!(rt.lightweight(PerformanceMode::Auto) && rt.reduced_automatically(PerformanceMode::Auto));
        assert!(!rt.note_low_end(PerformanceMode::Auto, Some(LowEnd::Battery)), "nothing new");
        assert!(rt.note_low_end(PerformanceMode::Auto, None), "plugged in again");
        assert_eq!(rt.level(PerformanceMode::Auto), PerfLevel::Full);
        rt.note_low_end(PerformanceMode::Auto, Some(LowEnd::Integrated));
        rt.restore();
        assert_eq!(rt.level(PerformanceMode::Auto), PerfLevel::Full, "Restore full quality wins");
        assert_eq!(PerfLevel::Lightweight.lower(), Some(PerfLevel::VeryLow));
        let plan = PerfLevel::Lightweight.plan();
        assert!(plan.scale == 1.0 && !plan.msaa && !plan.bloom && plan.light_shadows == 0);
        assert_eq!(PerformanceMode::Lightweight.id(), "Lightweight");
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
