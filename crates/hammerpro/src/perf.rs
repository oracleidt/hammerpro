use hp_app::runtime::{FramePhases, PerfSample, RepaintCount};
use hp_app::{AppState, Severity, t, tf};

const MEMORY_EVERY: f64 = 0.5;
const CAUSES_EVERY: f64 = 0.25;
const CAUSES_SHOWN: usize = 24;

pub(crate) const TR: hp_app::i18n::TrTable = &[
    (
        "perf.reduced",
        "Hammer Pro measured only {0} FPS in the 3D view and lowered the quality automatically ({1}). Use Restore full quality in the 3D view or in Settings, Rendering, to go back.",
        "Hammer Pro a mesuré seulement {0} images par seconde dans la vue 3D et a baissé la qualité automatiquement ({1}). Utilisez Rétablir la qualité maximale dans la vue 3D ou dans Réglages, Rendu, pour revenir en arrière.",
    ),
    (
        "perf.restored",
        "Full quality restored, automatic reduction paused",
        "Qualité maximale rétablie, réduction automatique suspendue",
    ),
];

#[derive(Default)]
pub(crate) struct PerfMonitor {
    seen_rendered: u64,
    next_memory: f64,
    last_mode: Option<hp_app::settings::PerformanceMode>,
    causes: Vec<((&'static str, u32), u64)>,
    causes_dirty: bool,
    next_causes: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Begin,
    Panels,
    Tool,
    Sync,
    Lighting,
    Views,
    Chrome,
    Dialogs,
    End,
}

pub(crate) struct PhaseClock {
    started: std::time::Instant,
    last: std::time::Instant,
    ms: [f32; 9],
}

impl Default for PhaseClock {
    fn default() -> Self {
        let now = std::time::Instant::now();
        Self { started: now, last: now, ms: [0.0; 9] }
    }
}

impl PhaseClock {
    pub(crate) fn start(&mut self, now: std::time::Instant) {
        self.started = now;
        self.last = now;
        self.ms = [0.0; 9];
    }

    pub(crate) fn mark(&mut self, phase: Phase) {
        let now = std::time::Instant::now();
        self.ms[phase as usize] += now.duration_since(self.last).as_secs_f32() * 1000.0;
        self.last = now;
    }

    pub(crate) fn publish(&self, state: &mut AppState) {
        let frame = FramePhases { total_ms: self.started.elapsed().as_secs_f32() * 1000.0, ms: self.ms };
        let p = &mut state.runtime.render.perf;
        if frame.total_ms >= p.worst_frame.total_ms {
            p.worst_frame = frame;
        }
        p.phase_sum.add(&frame);
        p.phase_frames = p.phase_frames.saturating_add(1);
        p.frame_phases = frame;
    }
}

fn short_source(file: &str) -> &str {
    if let Some(at) = file.find("crates") {
        return &file[at..];
    }
    let seps: Vec<usize> = file.match_indices(['/', '\\']).map(|(i, _)| i).collect();
    match seps.len() {
        n if n >= 3 => &file[seps[n - 3] + 1..],
        _ => file,
    }
}

impl PerfMonitor {
    pub(crate) fn set_adapter(state: &mut AppState, info: &wgpu::AdapterInfo) {
        let p = &mut state.runtime.render.perf;
        p.adapter = info.name.clone();
        p.integrated = matches!(info.device_type, wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::Cpu);
        p.graphics_api = format!("{:?}", info.backend);
        p.dx12_shader_compiler =
            (info.backend == wgpu::Backend::Dx12).then(hp_render::pipeline_cache::dx12_compiler).flatten();
    }

    pub(crate) fn note_repaints(&mut self, state: &mut AppState, ctx: &egui::Context, now: f64) -> bool {
        let causes = ctx.repaint_causes();
        let continuous = ctx.requested_repaint_last_pass();
        let p = &mut state.runtime.render.perf;
        if causes.is_empty() {
            p.frames_from_events += 1;
        }
        if continuous {
            p.continuous_frames += 1;
        }
        for c in causes {
            let key = (c.file, c.line);
            match self.causes.iter_mut().find(|(k, _)| *k == key) {
                Some((_, n)) => *n += 1,
                None => self.causes.push((key, 1)),
            }
            self.causes_dirty = true;
        }
        if self.causes_dirty && now >= self.next_causes {
            self.next_causes = now + CAUSES_EVERY;
            self.causes_dirty = false;
            self.causes.sort_by_key(|c| std::cmp::Reverse(c.1));
            p.repaint_causes = self
                .causes
                .iter()
                .take(CAUSES_SHOWN)
                .map(|((file, line), count)| RepaintCount {
                    at: format!("{}:{line}", short_source(file)),
                    count: *count,
                })
                .collect();
        }
        continuous
    }

    pub(crate) fn tick(
        &mut self,
        state: &mut AppState,
        dt: f32,
        frame_ms: f32,
        busy: bool,
        continuous: bool,
        now: f64,
    ) {
        let mode = state.settings.render.performance_mode;
        if self.last_mode != Some(mode) {
            if self.last_mode.is_some() {
                state.runtime.render.perf.unlock();
            }
            self.last_mode = Some(mode);
        }
        let rendered = state.runtime.render.views_rendered != self.seen_rendered;
        self.seen_rendered = state.runtime.render.views_rendered;
        let perf = &mut state.runtime.render.perf;
        if std::mem::take(&mut perf.restore_request) {
            perf.restore();
            state.notify(Severity::Info, t!("perf.restored"));
        }
        let perf = &mut state.runtime.render.perf;
        if let Some(level) = perf.observe_frame(mode, rendered, continuous, busy, dt) {
            let fps = perf.auto_fps;
            log::warn!("performance: {:.1} FPS measured, quality lowered to {}", fps, level.id());
            state.notify(Severity::Warning, tf!("perf.reduced", format!("{fps:.0}"), level.id().replace('_', " ")));
        }
        let times = state.gfx.as_ref().map(|g| g.renderer.frame_times());
        let working_set = if now >= self.next_memory {
            self.next_memory = now + MEMORY_EVERY;
            crate::dispatch::lighting::memory::of_self().map(|u| u.working_set)
        } else {
            None
        };
        let s = &mut state.runtime.render.perf.sample;
        let keep_ws = s.working_set;
        let mut next = PerfSample { frame_ms, working_set: working_set.unwrap_or(keep_ws), ..PerfSample::default() };
        if let Some(t) = times {
            next.prepare_ms = t.cpu.prepare_ms;
            next.record_ms = t.cpu.record_ms;
            next.submit_ms = t.cpu.submit_ms;
            next.setup_ms = t.cpu.setup_ms;
            next.clusters_ms = t.cpu.clusters_ms;
            next.shadow_prep_ms = t.cpu.shadow_prep_ms;
            next.models_ms = t.cpu.models_ms;
            next.reflection_cpu_ms = t.reflection_cpu_ms;
            next.gpu_supported = t.gpu_supported;
            if let Some(g) = t.gpu {
                next.gpu_total_ms = g.total_ms;
                next.gpu_shadows_ms = g.shadows_ms;
                next.gpu_scene_ms = g.scene_ms;
                next.gpu_post_ms = g.post_ms;
                next.gpu_passes = passes_of(&g);
            }
            next.gpu_reflection_ms = t.reflection_gpu_ms;
            if let Some(g) = t.reflection_gpu {
                next.gpu_reflection_passes = passes_of(&g);
            }
        }
        *s = next;
    }
}

struct Bench {
    frames: u32,
    trace: Option<Vec<hp_app::runtime::BenchFrame>>,
    done: u32,
    base: hp_app::view::Camera3d,
    frame_ms: Vec<f32>,
    cpu: f32,
    phases: [f32; 8],
    counts: [f64; 3],
    cpu_each: Vec<f32>,
    gpu_each: Vec<f32>,
    gpu: f32,
    shadows: f32,
    scene: f32,
    post: f32,
    reflection: f32,
    passes: hp_app::runtime::GpuPasses,
    reflection_passes: hp_app::runtime::GpuPasses,
    gpu_samples: u32,
    started: std::time::Instant,
    ws_start: u64,
    ws_peak: u64,
    rendered_start: u64,
    last_render: u64,
}

impl Bench {
    fn pose(&self, i: u32) -> (glam::DVec3, f64) {
        let t = f64::from(i) / f64::from(self.frames.max(1));
        let a = t * std::f64::consts::TAU;
        let orbit = 384.0;
        let pos =
            self.base.pos + glam::DVec3::new(a.cos() * orbit, a.sin() * orbit, 0.0) - glam::DVec3::new(orbit, 0.0, 0.0);
        (pos, self.base.yaw_deg + t * 360.0)
    }
}

pub(crate) struct BenchDriver {
    run: Option<Bench>,
}

impl BenchDriver {
    pub(crate) fn new() -> Self {
        Self { run: None }
    }

    pub(crate) fn tick(&mut self, state: &mut AppState, ctx: &egui::Context, dt: f32) {
        if let Some(req) = state.runtime.render.perf.bench_request.take() {
            let ws = crate::dispatch::lighting::memory::of_self().map_or(0, |u| u.working_set);
            self.run = Some(Bench {
                frames: req.frames.max(10),
                trace: req.trace.then(Vec::new),
                done: 0,
                base: state.cameras.cam3d,
                frame_ms: Vec::new(),
                cpu: 0.0,
                phases: [0.0; 8],
                counts: [0.0; 3],
                cpu_each: Vec::new(),
                gpu_each: Vec::new(),
                gpu: 0.0,
                shadows: 0.0,
                scene: 0.0,
                post: 0.0,
                reflection: 0.0,
                passes: hp_app::runtime::GpuPasses::default(),
                reflection_passes: hp_app::runtime::GpuPasses::default(),
                gpu_samples: 0,
                started: std::time::Instant::now(),
                ws_start: ws,
                ws_peak: ws,
                rendered_start: state.runtime.render.views_rendered,
                last_render: state.runtime.render.views_rendered,
            });
            state.runtime.render.perf.bench_running = true;
            state.runtime.render.perf.bench_result = None;
        }
        let Some(b) = self.run.as_mut() else { return };
        if b.done > 0 || b.frame_ms.is_empty() {
            let s = &state.runtime.render.perf.sample;
            if b.done > 0 {
                b.frame_ms.push(dt * 1000.0);
                if let Some(c) = state.gfx.as_ref().and_then(|g| g.renderer.view_counts(0)) {
                    b.counts[0] += f64::from(c.draw_calls);
                    b.counts[1] += c.triangles as f64;
                    b.counts[2] += f64::from(c.model_instances);
                }
                b.cpu += s.prepare_ms + s.record_ms + s.submit_ms;
                b.cpu_each.push(s.prepare_ms + s.record_ms + s.submit_ms);
                if let Some(t) = b.trace.as_mut() {
                    let phases = &state.runtime.render.perf.frame_phases;
                    t.push(hp_app::runtime::BenchFrame {
                        frame_ms: dt * 1000.0,
                        cpu_ms: s.prepare_ms + s.record_ms + s.submit_ms,
                        gpu_ms: if s.gpu_supported { s.gpu_total_ms + s.gpu_reflection_ms } else { 0.0 },
                        views_ms: phases.ms[5],
                        view_render_ms: state.runtime.render.perf.views.iter().map(|v| v.render_ms).sum(),
                        view_overlay_ms: state
                            .runtime
                            .render
                            .perf
                            .views
                            .iter()
                            .map(|v| v.overlay_ms + v.paint_ms)
                            .sum(),
                        app_ms: phases.total_ms,
                        gpu_passes: s.gpu_passes,
                    });
                }
                if s.gpu_supported {
                    b.gpu_each.push(s.gpu_total_ms + s.gpu_reflection_ms);
                }
                let p = [
                    s.setup_ms,
                    s.reflection_cpu_ms,
                    s.clusters_ms,
                    s.shadow_prep_ms,
                    s.models_ms,
                    s.prepare_ms,
                    s.record_ms,
                    s.submit_ms,
                ];
                for (acc, v) in b.phases.iter_mut().zip(p) {
                    *acc += v;
                }
                if s.gpu_supported {
                    b.gpu += s.gpu_total_ms + s.gpu_reflection_ms;
                    b.shadows += s.gpu_shadows_ms;
                    b.scene += s.gpu_scene_ms;
                    b.post += s.gpu_post_ms;
                    b.reflection += s.gpu_reflection_ms;
                    b.passes.add(&s.gpu_passes);
                    b.reflection_passes.add(&s.gpu_reflection_passes);
                    b.gpu_samples += 1;
                }
            }
            b.ws_peak = b.ws_peak.max(s.working_set);
        }
        if b.done >= b.frames {
            let b = self.run.take().expect("running");
            let n = b.frame_ms.len().max(1) as f32;
            let mut sorted = b.frame_ms.clone();
            sorted.sort_by(f32::total_cmp);
            let avg = b.frame_ms.iter().sum::<f32>() / n;
            let g = b.gpu_samples.max(1) as f32;
            let ws_end = crate::dispatch::lighting::memory::of_self().map_or(0, |u| u.working_set);
            state.cameras.cam3d = b.base;
            let p = &mut state.runtime.render.perf;
            p.bench_running = false;
            p.bench_result = Some(hp_app::runtime::BenchResult {
                frames: b.frame_ms.len() as u32,
                seconds: b.started.elapsed().as_secs_f32(),
                avg_fps: if avg > 0.0 { 1000.0 / avg } else { 0.0 },
                frame_ms_avg: avg,
                frame_ms_p95: sorted.get(((sorted.len() as f32) * 0.95) as usize).copied().unwrap_or(0.0),
                frame_ms_max: sorted.last().copied().unwrap_or(0.0),
                frame_ms_median: sorted.get(sorted.len() / 2).copied().unwrap_or(0.0),
                frame_ms_p99: percentile(&b.frame_ms, 0.99),
                cpu_ms_median: median(&b.cpu_each),
                cpu_ms_p99: percentile(&b.cpu_each, 0.99),
                gpu_ms_p99: percentile(&b.gpu_each, 0.99),
                gpu_passes_ms_avg: b.passes.scaled(1.0 / g),
                gpu_reflection_passes_ms_avg: b.reflection_passes.scaled(1.0 / g),
                trace: b.trace.unwrap_or_default(),
                gpu_ms_median: median(&b.gpu_each),
                draw_calls_avg: (b.counts[0] / f64::from(n)) as f32,
                triangles_avg: (b.counts[1] / f64::from(n)) as f32,
                model_instances_avg: (b.counts[2] / f64::from(n)) as f32,
                cpu_ms_avg: b.cpu / n,
                cpu_phases_ms_avg: b.phases.map(|v| v / n),
                gpu_ms_avg: b.gpu / g,
                gpu_shadows_ms_avg: b.shadows / g,
                gpu_scene_ms_avg: b.scene / g,
                gpu_post_ms_avg: b.post / g,
                gpu_reflection_ms_avg: b.reflection / g,
                working_set_start_mb: b.ws_start >> 20,
                working_set_end_mb: ws_end >> 20,
                working_set_peak_mb: b.ws_peak.max(ws_end) >> 20,
                views_rendered: state.runtime.render.views_rendered.saturating_sub(b.rendered_start),
            });
            return;
        }
        let (pos, yaw) = b.pose(b.done);
        let cam = &mut state.cameras.cam3d;
        cam.pos = pos;
        cam.yaw_deg = yaw;
        b.done += 1;
        b.last_render = state.runtime.render.views_rendered;
        ctx.request_repaint();
    }
}

fn passes_of(g: &hp_render::profile::GpuTimes) -> hp_app::runtime::GpuPasses {
    hp_app::runtime::GpuPasses {
        light_cull_ms: g.light_cull_ms,
        sun_shadows_ms: g.sun_shadows_ms,
        local_shadows_ms: g.local_shadows_ms,
        main_ms: g.main_ms,
        effects_ms: g.effects_ms,
        present_ms: g.present_ms,
        outline_ms: g.outline_ms,
    }
}

fn median(values: &[f32]) -> f32 {
    percentile(values, 0.5)
}

fn percentile(values: &[f32], q: f32) -> f32 {
    let mut v = values.to_vec();
    v.sort_by(f32::total_cmp);
    let i = ((v.len() as f32) * q) as usize;
    v.get(i.min(v.len().saturating_sub(1))).copied().unwrap_or(0.0)
}
