use hp_app::runtime::PerfSample;
use hp_app::{AppState, Severity, t, tf};

const MEMORY_EVERY: f64 = 0.5;

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
}

impl PerfMonitor {
    pub(crate) fn set_adapter(state: &mut AppState, info: &wgpu::AdapterInfo) {
        let p = &mut state.runtime.render.perf;
        p.adapter = info.name.clone();
        p.integrated = matches!(info.device_type, wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::Cpu);
    }

    pub(crate) fn tick(&mut self, state: &mut AppState, dt: f32, frame_ms: f32, busy: bool, now: f64) {
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
        if let Some(level) = perf.observe(mode, rendered, busy, dt) {
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
            }
            next.gpu_reflection_ms = t.reflection_gpu_ms;
        }
        *s = next;
    }
}

struct Bench {
    frames: u32,
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
                cpu_ms_median: median(&b.cpu_each),
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

fn median(values: &[f32]) -> f32 {
    let mut v = values.to_vec();
    v.sort_by(f32::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}
