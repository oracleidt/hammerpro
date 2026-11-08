use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use hp_app::AppState;
use hp_app::runtime::primitives::{PrimitiveClass, PrimitiveMemory, PrimitiveStats, ResourceProbes};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp(u64, u64, u64);

struct Pending {
    stamp: Stamp,
    stats: PrimitiveStats,
    result: Receiver<(Result<PrimitiveMemory, u32>, u64)>,
}

#[derive(Default)]
pub(crate) struct Monitor {
    observed: Option<(Stamp, f64)>,
    completed: Option<Stamp>,
    pending: Option<Pending>,
}

impl Monitor {
    pub(crate) fn tick(
        &mut self,
        state: &mut AppState,
        lighting: &crate::dispatch::lighting::State,
        ctx: &egui::Context,
    ) {
        if !state.settings.render.primitive_stats {
            self.pending = None;
            return;
        }
        let stamp = Stamp(
            state.doc.revision(),
            state.assets.resource_memory_generation(),
            crate::dispatch::lighting::primitive_resource_stamp(lighting),
        );
        let now = state.frame.time;
        if self.observed.is_none_or(|(seen, _)| seen != stamp) {
            self.observed = Some((stamp, now));
            if state.runtime.primitives.stats.as_ref().is_some_and(|stats| stats.revision != state.doc.revision()) {
                state.runtime.primitives.stats = None;
            } else if let Some(stats) = state.runtime.primitives.stats.as_mut() {
                stats.collecting = true;
            }
        }
        if let Some(pending) = &self.pending {
            let result = match pending.result.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some((Err(1), 0)),
            };
            if let Some((result, measurement_us)) = result {
                let Some(mut pending) = self.pending.take() else { return };
                if pending.stamp == stamp {
                    match result {
                        Ok(memory) => pending.stats.memory = Some(memory),
                        Err(error) => pending.stats.memory_error = Some(error.to_string()),
                    }
                    pending.stats.collecting = false;
                    pending.stats.measurement_us = measurement_us;
                    state.runtime.primitives.stats = Some(pending.stats);
                    self.completed = Some(stamp);
                }
            }
        }
        if self.pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if self.completed == Some(stamp) {
            return;
        }
        if matches!(state.assets.status(), hp_app::state::AssetStatus::Mounting)
            || state.runtime.startup.resources.is_some()
            || state.runtime.lighting.progress.is_some()
            || self.observed.is_some_and(|(_, at)| now - at < 0.6)
        {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        }
        let mut probes = ResourceProbes::default();
        let mut stats = hp_app::runtime::primitive_scene::collect(&state.doc, &mut probes);
        let props = state.assets.primitive_resources(&state.doc, &mut probes);
        stats.props = props.props;
        stats.prop_materials = props.prop_materials;
        stats.models_loading = props.models_loading;
        stats.models_missing = props.models_missing;
        if !crate::dispatch::lighting::primitive_resource_probes(lighting, &mut probes) {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if let Some(gfx) = &state.gfx {
            for address in gfx.renderer.lightmap_cpu_allocation_addresses() {
                probes.point(address, PrimitiveClass::Lighting);
            }
            if let Some(volume) = gfx.renderer.indirect_volume() {
                probes.hold(volume, PrimitiveClass::Lighting);
                if !volume.cpu_words().is_empty() {
                    probes.point(volume.cpu_words().as_ptr() as usize, PrimitiveClass::Lighting);
                }
            }
        }
        stats.collecting = true;
        state.runtime.primitives.stats = Some(stats.clone());
        let (tx, result) = std::sync::mpsc::channel();
        let wake = ctx.clone();
        match std::thread::Builder::new().name("hp-primitive-memory".into()).spawn(move || {
            let start = std::time::Instant::now();
            let memory = crate::primitive_memory::measure(&mut probes);
            let measurement_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
            let _ = tx.send((memory, measurement_us));
            wake.request_repaint();
        }) {
            Ok(_) => self.pending = Some(Pending { stamp, stats, result }),
            Err(error) => {
                stats.collecting = false;
                stats.memory_error = Some(error.to_string());
                state.runtime.primitives.stats = Some(stats);
                self.completed = Some(stamp);
            }
        }
    }
}
