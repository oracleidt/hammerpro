use std::time::{Duration, Instant};

use hp_app::{Severity, t, tf};

use crate::app::HammerProApp;

const LOSS_WINDOW: Duration = Duration::from_secs(120);
const MAX_LOSSES: usize = 3;
const GIVE_UP_AFTER: Duration = Duration::from_secs(20);
const MAX_FAILED_ATTEMPTS: u64 = 40;

#[derive(Default)]
pub(crate) struct GpuRecovery {
    pub render_state: Option<egui_wgpu::RenderState>,
    waiting_since: Option<Instant>,
    losses: Vec<Instant>,
}

impl GpuRecovery {
    pub(crate) fn new(render_state: Option<egui_wgpu::RenderState>) -> Self {
        Self { render_state, ..Self::default() }
    }

    pub(crate) fn waiting(&self) -> bool {
        self.waiting_since.is_some()
    }
}

pub(crate) fn too_many(losses: &mut Vec<Instant>, now: Instant) -> bool {
    losses.retain(|t| now.duration_since(*t) < LOSS_WINDOW);
    losses.push(now);
    losses.len() > MAX_LOSSES
}

pub(crate) fn give_up(waited: Duration, failed_attempts: u64) -> bool {
    waited > GIVE_UP_AFTER || failed_attempts > MAX_FAILED_ATTEMPTS
}

pub(crate) fn install_shadow_filter(ctx: &egui::Context) {
    let ctx = ctx.clone();
    egui_wgpu::recovery::set_shadow_filter(move |id| {
        let manager = ctx.tex_manager();
        let manager = manager.read();
        manager.meta(id).is_none_or(|meta| !meta.name.starts_with("thumb:"))
    });
}

impl HammerProApp {
    pub(crate) fn tick_gpu_recovery(&mut self, frame: &eframe::Frame) {
        if std::mem::take(&mut self.state.runtime.gpu.simulate_loss_request)
            && let Some(gfx) = self.state.gfx.as_ref()
        {
            crate::gpu_guard::simulate_loss(&gfx.device);
        }
        if let Some(reason) = crate::gpu_guard::take_lost() {
            self.begin_gpu_recovery(&reason);
        }
        if let Some(rs) = egui_wgpu::recovery::take_recreated() {
            self.finish_gpu_recovery(rs);
        }
        if let Some(since) = self.gpu.waiting_since
            && give_up(since.elapsed(), egui_wgpu::recovery::failed_attempts())
        {
            self.gpu.waiting_since = None;
            let reason = self.state.runtime.gpu.last_device_loss.clone().unwrap_or_default();
            self.gpu_lost_exit(&reason);
        }
        if self.state.gfx.is_none() && !self.gpu.waiting() {
            let rs = self.gpu.render_state.clone().or_else(|| frame.wgpu_render_state().cloned());
            if let Some(rs) = rs {
                self.install_gfx(&rs);
                self.gpu.render_state = Some(rs);
            }
        }
    }

    pub(crate) fn install_gfx(&mut self, rs: &egui_wgpu::RenderState) {
        crate::gpu_guard::install(&rs.device);
        self.state.gfx = Some(hp_app::state::Gfx::new(rs));
        let rt = &self.state.runtime.gpu;
        if let Some(gfx) = self.state.gfx.as_mut()
            && rt.budget > 0
        {
            gfx.renderer.set_memory_budget(rt.budget, 0);
            gfx.renderer.set_low_memory(rt.tier.low());
            if rt.texture_limit > 0 {
                gfx.renderer.set_texture_limit(rt.texture_limit);
            }
            if rt.lightmap_layers > 0 {
                gfx.renderer.set_lightmap_layer_cap(rt.lightmap_layers);
            }
        }
        let msaa = self.state.settings.view.msaa
            && self.state.runtime.render.perf.plan(self.state.settings.render.performance_mode).msaa;
        crate::app::install_pipeline_cache(rs, self.state.gfx.as_mut(), msaa);
        self.vram = crate::vram::VramMonitor::new(&rs.adapter.get_info());
        crate::perf::PerfMonitor::set_adapter(&mut self.state, &rs.adapter.get_info());
    }

    fn begin_gpu_recovery(&mut self, reason: &str) {
        let rt = &mut self.state.runtime.gpu;
        rt.device_losses += 1;
        rt.last_device_loss = Some(reason.to_owned());
        if too_many(&mut self.gpu.losses, Instant::now()) {
            log::error!("the graphics device was lost {} times in two minutes: Hammer Pro restarts", MAX_LOSSES + 1);
            self.gpu_lost_exit(reason);
            return;
        }
        self.state.log(Severity::Error, tf!("app.gpu_lost", reason));
        self.save_before_gpu_reset();
        self.reset_gpu_state();
        self.gpu.waiting_since = Some(Instant::now());
        self.state.runtime.gpu.recovering = true;
        self.state.notify(Severity::Warning, t!("app.gpu_recovering"));
    }

    fn finish_gpu_recovery(&mut self, rs: egui_wgpu::RenderState) {
        if self.gpu.waiting_since.is_none() {
            log::warn!("the graphics device was created again after a painting failure");
            self.state.runtime.gpu.device_losses += 1;
            self.save_before_gpu_reset();
            self.reset_gpu_state();
        }
        let waited = self.gpu.waiting_since.take().map_or(0, |t| t.elapsed().as_millis());
        self.install_gfx(&rs);
        self.gpu.render_state = Some(rs);
        self.state.doc.mark_everything_changed();
        let rt = &mut self.state.runtime.gpu;
        rt.device_recoveries += 1;
        rt.recovering = false;
        log::info!(
            "the graphics device works again after {waited} ms ({} recoveries this session): the map, textures and lighting load again",
            rt.device_recoveries
        );
        self.state.notify(Severity::Info, t!("app.gpu_recovered"));
    }

    fn save_before_gpu_reset(&mut self) {
        self.emergency_saved = false;
        self.emergency_save();
        self.emergency_saved = false;
    }

    fn reset_gpu_state(&mut self) {
        crate::dispatch::lighting::forget_for_new_device(self);
        self.pkg.render.forget_gpu();
        let old = self.state.gfx.take().map(|g| g.device.clone());
        self.state.assets = self.make_assets();
        if let Some(device) = old {
            for _ in 0..3 {
                let _ = device.poll(wgpu::PollType::Poll);
            }
        }
        self.state.doc.mark_everything_changed();
        self.state.pick = None;
        self.state.hover_world = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_losses_end_in_a_restart() {
        let start = Instant::now();
        let mut losses = Vec::new();
        assert!(!too_many(&mut losses, start));
        assert!(!too_many(&mut losses, start + Duration::from_secs(1)));
        assert!(!too_many(&mut losses, start + Duration::from_secs(2)));
        assert!(too_many(&mut losses, start + Duration::from_secs(3)), "a fourth loss in two minutes");
        let mut spread = Vec::new();
        for i in 0..10 {
            assert!(!too_many(&mut spread, start + LOSS_WINDOW * i), "losses far apart are recovered each time");
        }
    }

    #[test]
    fn waiting_for_a_device_has_an_end() {
        assert!(!give_up(Duration::from_secs(1), 0));
        assert!(give_up(GIVE_UP_AFTER + Duration::from_millis(1), 0));
        assert!(give_up(Duration::ZERO, MAX_FAILED_ATTEMPTS + 1));
    }
}
