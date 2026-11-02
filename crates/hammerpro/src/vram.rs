use hp_app::runtime::{GpuRelease, GpuReleased, VramBreakdown, VramPressure, VramSource};
use hp_app::{AppState, Severity, tf};
use hp_render::memory::VramProbe;

const CHECK_EVERY: f64 = 0.5;
const HIGH: f64 = 0.85;
const PARKED_SHARE: f64 = 0.75;
const RAISE_BELOW: f64 = 0.5;
const RAISE_AFTER: f64 = 20.0;
const RELEASE_GAP: f64 = 5.0;
const CRITICAL_GAP: f64 = 2.0;
const AUTO_SHARE: f64 = 0.9;
const MIN_TEXTURE_LIMIT: u32 = 256;
const DISCRETE_FALLBACK: u64 = 3 << 30;
const OTHER_FALLBACK: u64 = 3 << 29;
const WARN_GAP: f64 = 60.0;
const QUIET_GAP: f64 = 30.0;

pub(crate) struct VramMonitor {
    probe: VramProbe,
    fallback: u64,
    next_check: f64,
    oom_seen: u64,
    last_release: f64,
    released_at: u64,
    calm_since: Option<f64>,
    warned: VramPressure,
    last_warning: f64,
}

impl Default for VramMonitor {
    fn default() -> Self {
        Self {
            probe: VramProbe::none(),
            fallback: OTHER_FALLBACK,
            next_check: 0.0,
            oom_seen: hp_render::memory::out_of_memory_events(),
            last_release: f64::NEG_INFINITY,
            released_at: 0,
            calm_since: None,
            warned: VramPressure::Normal,
            last_warning: f64::NEG_INFINITY,
        }
    }
}

pub(crate) fn auto_budget(driver_budget: Option<u64>, fallback: u64) -> u64 {
    match driver_budget {
        Some(b) if b > 0 => (b as f64 * AUTO_SHARE) as u64,
        _ => fallback,
    }
}

pub(crate) fn pressure(usage: u64, budget: u64, out_of_memory: bool) -> VramPressure {
    if out_of_memory || usage > budget {
        VramPressure::Critical
    } else if usage as f64 > budget as f64 * HIGH {
        VramPressure::High
    } else {
        VramPressure::Normal
    }
}

pub(crate) fn parked_limit(budget: u64, usage: u64, parked: u64, keep: bool) -> u64 {
    if !keep {
        return 0;
    }
    let live = usage.saturating_sub(parked);
    ((budget as f64 * PARKED_SHARE) as u64).saturating_sub(live)
}

pub(crate) fn lower_limit(current: u32) -> Option<u32> {
    if current <= MIN_TEXTURE_LIMIT {
        return None;
    }
    Some(if current > 4096 { 2048 } else { (current / 2).max(MIN_TEXTURE_LIMIT) })
}

pub(crate) fn raise_limit(current: u32, max: u32) -> Option<u32> {
    (current < max).then(|| if current >= 4096 { max } else { (current * 2).min(max) })
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Decision {
    pub request: Option<bool>,
    pub level: VramPressure,
    pub since: f64,
    pub worth: bool,
    pub over: bool,
    pub texture_limit: u32,
}

pub(crate) fn decide(d: Decision) -> Option<GpuRelease> {
    let worth = d.worth || d.since >= QUIET_GAP;
    match (d.request, d.level) {
        (Some(true), _) => Some(GpuRelease::Textures(lower_limit(d.texture_limit).unwrap_or(d.texture_limit))),
        (Some(false), _) => Some(GpuRelease::Caches),
        (None, VramPressure::Critical) if d.since >= CRITICAL_GAP => {
            match lower_limit(d.texture_limit).filter(|_| d.over) {
                Some(l) => Some(GpuRelease::Textures(l)),
                None => worth.then_some(GpuRelease::Caches),
            }
        }
        (None, VramPressure::High) if d.since >= RELEASE_GAP && worth => Some(GpuRelease::BackgroundMaps),
        _ => None,
    }
}

fn mb(bytes: u64) -> u64 {
    bytes >> 20
}

impl VramMonitor {
    pub(crate) fn new(info: &wgpu::AdapterInfo) -> Self {
        let probe = VramProbe::new(info);
        let fallback =
            if info.device_type == wgpu::DeviceType::DiscreteGpu { DISCRETE_FALLBACK } else { OTHER_FALLBACK };
        match probe.query() {
            Some(v) => log::info!(
                "graphics memory: {} MB dedicated, the system gives Hammer Pro a budget of {} MB",
                mb(v.dedicated),
                mb(v.budget)
            ),
            None => log::info!("graphics memory: the driver does not report a budget, {} MB is assumed", mb(fallback)),
        }
        Self { probe, fallback, ..Self::default() }
    }

    pub(crate) fn tick(&mut self, state: &mut AppState, now: f64) {
        let oom = hp_render::memory::out_of_memory_events();
        let new_oom = oom != self.oom_seen;
        let request = state.runtime.gpu.release_request.take();
        if now < self.next_check && !new_oom && request.is_none() {
            return;
        }
        self.next_check = now + CHECK_EVERY;
        self.oom_seen = oom;
        let Some(gfx) = state.gfx.as_mut() else { return };
        let report = gfx.renderer.memory();
        let upload_failed = gfx.renderer.take_upload_failed();
        let parked = state.assets.parked_gpu_bytes();
        let estimate = VramBreakdown {
            geometry: report.geometry,
            textures: report.textures,
            models: report.models,
            lightmaps: report.lightmaps,
            compiled: report.compiled,
            other: report.other,
            background_maps: parked,
        };
        let driver = self.probe.query();
        let (usage, source) = match driver {
            Some(v) => (v.usage, VramSource::Driver),
            None => (estimate.total(), VramSource::Estimate),
        };
        let fixed = state.settings.render.vram_budget();
        let budget = fixed.unwrap_or_else(|| auto_budget(driver.map(|v| v.budget), self.fallback));
        let keep = state.settings.render.keep_background_maps_on_gpu;
        let level = pressure(usage, budget, new_oom || upload_failed);
        let texture_limit = gfx.renderer.texture_limit();
        let max_texture = gfx.renderer.max_texture_size();
        state.assets.set_parked_gpu_limit(parked_limit(budget, usage, parked, keep && level == VramPressure::Normal));
        let mut released = GpuReleased::default();
        let worth = new_oom || usage > self.released_at.saturating_add(budget / 20) || parked > 0;
        let what = decide(Decision {
            request,
            level,
            since: now - self.last_release,
            worth,
            over: usage > budget || new_oom,
            texture_limit,
        });
        if let Some(w) = what
            && let Some(gfx) = state.gfx.as_mut()
        {
            released = state.assets.release_gpu_memory(gfx, w);
            self.last_release = now;
            self.released_at = usage;
            self.calm_since = None;
        }
        if level == VramPressure::Normal && usage as f64 <= budget as f64 * RAISE_BELOW && texture_limit < max_texture {
            let since = *self.calm_since.get_or_insert(now);
            let grown = usage + estimate.textures.saturating_mul(3);
            if now - since >= RAISE_AFTER
                && (grown as f64) < budget as f64 * PARKED_SHARE
                && let (Some(up), Some(gfx)) = (raise_limit(texture_limit, max_texture), state.gfx.as_mut())
            {
                let reloading = state.assets.apply_texture_limit(gfx, up);
                log::info!("graphics memory is free again: textures limited to {up} pixels, {reloading} loaded again");
                self.calm_since = None;
            }
        } else if level != VramPressure::Normal {
            self.calm_since = None;
        }
        if (upload_failed || new_oom) && !released.is_empty() {
            state.doc.mark_everything_changed();
        }
        self.warn(state, level, now, usage, budget, &released);
        let texture_limit = state.gfx.as_ref().map_or(texture_limit, |g| g.renderer.texture_limit());
        let rt = &mut state.runtime.gpu;
        rt.budget = budget;
        rt.budget_is_auto = fixed.is_none();
        rt.usage = usage;
        rt.peak_usage = rt.peak_usage.max(usage);
        rt.source = source;
        rt.driver_budget = driver.map_or(0, |v| v.budget);
        rt.dedicated = driver.map_or(0, |v| v.dedicated);
        rt.estimate = estimate;
        rt.background_maps = state.assets.parked_scenes();
        rt.texture_limit = texture_limit;
        rt.max_texture = max_texture;
        rt.pressure = level;
        rt.out_of_memory_events = oom;
        if !released.is_empty() {
            rt.releases += 1;
            rt.released.add(released);
            rt.last_release = Some(format!(
                "{} background maps, {} textures, {} models, {} thumbnails freed, {} textures loaded again",
                released.background_maps, released.textures, released.models, released.thumbnails, released.reloading
            ));
            log::info!(
                "graphics memory {} of {} MB ({}): {}",
                mb(usage),
                mb(budget),
                source.id(),
                rt.last_release.as_deref().unwrap_or("")
            );
        }
    }

    fn warn(
        &mut self,
        state: &mut AppState,
        level: VramPressure,
        now: f64,
        usage: u64,
        budget: u64,
        released: &GpuReleased,
    ) {
        if level == VramPressure::Normal {
            state.runtime.gpu.warning = None;
            self.warned = VramPressure::Normal;
            return;
        }
        if level <= self.warned || now - self.last_warning < WARN_GAP {
            return;
        }
        self.warned = level;
        self.last_warning = now;
        let text = if level == VramPressure::High {
            tf!("vram.high", mb(usage), mb(budget))
        } else if released.reloading > 0 {
            tf!("vram.critical_textures", mb(usage), mb(budget))
        } else {
            tf!("vram.critical", mb(usage), mb(budget))
        };
        state.runtime.gpu.warning = Some(text.clone());
        let severity = if level == VramPressure::Critical { Severity::Error } else { Severity::Warning };
        state.notify(severity, text);
    }
}

pub const TR: hp_app::i18n::TrTable = &[
    (
        "vram.high",
        "Graphics memory is almost full ({0} of {1} MB): Hammer Pro frees what is not shown (maps of other tabs, unused textures and models), it loads again when needed.",
        "La mémoire graphique est presque pleine ({0} sur {1} Mo) : Hammer Pro libère ce qui n'est pas affiché (maps des autres onglets, textures et modèles inutilisés), cela se recharge au besoin.",
    ),
    (
        "vram.critical",
        "The graphics card is out of memory ({0} of {1} MB): Hammer Pro freed what it could. Close some tabs or lower the graphics memory budget in Settings > Render.",
        "La carte graphique n'a plus de mémoire ({0} sur {1} Mo) : Hammer Pro a libéré ce qu'il pouvait. Fermez des onglets ou baissez le budget de mémoire graphique dans Paramètres > Rendu.",
    ),
    (
        "vram.critical_textures",
        "The graphics card is out of memory ({0} of {1} MB): textures are shown at a lower resolution until memory is free again.",
        "La carte graphique n'a plus de mémoire ({0} sur {1} Mo) : les textures sont affichées en plus basse résolution jusqu'à ce que la mémoire se libère.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_comes_from_the_driver_when_it_reports_one() {
        assert_eq!(auto_budget(Some(8 << 30), DISCRETE_FALLBACK), (8u64 << 30) * 9 / 10);
        assert_eq!(auto_budget(None, DISCRETE_FALLBACK), DISCRETE_FALLBACK);
        assert_eq!(auto_budget(Some(0), OTHER_FALLBACK), OTHER_FALLBACK);
    }

    #[test]
    fn pressure_levels() {
        let b = 1000;
        assert_eq!(pressure(500, b, false), VramPressure::Normal);
        assert_eq!(pressure(900, b, false), VramPressure::High);
        assert_eq!(pressure(1001, b, false), VramPressure::Critical);
        assert_eq!(pressure(10, b, true), VramPressure::Critical, "an out of memory error is critical");
    }

    #[test]
    fn background_maps_only_use_free_room() {
        assert_eq!(parked_limit(1000, 400, 100, true), 450);
        assert_eq!(parked_limit(1000, 900, 0, true), 0);
        assert_eq!(parked_limit(1000, 100, 0, false), 0);
    }

    #[test]
    fn texture_limits_step_down_and_up() {
        assert_eq!(lower_limit(16384), Some(2048));
        assert_eq!(lower_limit(2048), Some(1024));
        assert_eq!(lower_limit(512), Some(256));
        assert_eq!(lower_limit(256), None);
        assert_eq!(lower_limit(300), Some(256));
        assert_eq!(raise_limit(256, 16384), Some(512));
        assert_eq!(raise_limit(2048, 16384), Some(4096));
        assert_eq!(raise_limit(4096, 16384), Some(16384));
        assert_eq!(raise_limit(16384, 16384), None);
    }

    #[test]
    fn releases_step_down_then_back_off() {
        let d = Decision {
            request: None,
            level: VramPressure::Critical,
            since: 3.0,
            worth: false,
            over: true,
            texture_limit: 1024,
        };
        assert_eq!(decide(d), Some(GpuRelease::Textures(512)));
        assert_eq!(decide(Decision { since: 1.0, ..d }), None, "not twice in a row");
        let floor = Decision { texture_limit: MIN_TEXTURE_LIMIT, ..d };
        assert_eq!(decide(floor), None, "nothing more to lower and nothing new to free");
        assert_eq!(decide(Decision { worth: true, ..floor }), Some(GpuRelease::Caches));
        assert_eq!(decide(Decision { since: QUIET_GAP, ..floor }), Some(GpuRelease::Caches));
        let high = Decision { level: VramPressure::High, since: RELEASE_GAP, worth: true, ..d };
        assert_eq!(decide(high), Some(GpuRelease::BackgroundMaps));
        assert_eq!(decide(Decision { worth: false, ..high }), None);
        assert_eq!(decide(Decision { level: VramPressure::Normal, ..d }), None);
        assert_eq!(
            decide(Decision { request: Some(false), level: VramPressure::Normal, ..d }),
            Some(GpuRelease::Caches)
        );
        assert_eq!(decide(Decision { request: Some(true), since: 0.0, ..floor }), Some(GpuRelease::Textures(256)));
    }

    #[test]
    fn texts_have_no_long_dashes() {
        for (_, en, fr) in TR {
            assert!(!en.contains('\u{2014}') && !fr.contains('\u{2014}'));
        }
    }
}
