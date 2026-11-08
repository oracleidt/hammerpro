mod bake;
#[cfg(test)]
mod bench;
mod bsp;
mod cache;
pub(crate) mod child;
mod grid;
pub(crate) mod memory;
mod prepare;

use std::collections::{HashMap, HashSet};

use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::runtime::{
    BakeKind, BakePhase, BakeProgress, BakeSummary, BudgetEntry, BudgetSummary, IndirectPhase, IndirectStatus,
    PreviewStats,
};
use hp_app::settings::{LightingQuality, LightingSetting, RenderModeSetting};
use hp_app::state::RightTab;
use hp_app::{Action, AppState, Severity, ToolId, t, tf};
use hp_editor::luxels::{BudgetOptions, ColorCorrectionPlus, LightmapKind, MAX_MAP_LIGHTING};
use hp_editor::{FaceRef, SelectOp};
use hp_render::lightmap::{LightmapBatch, LightmapDisplay, LightmapFace, LightmapRect};

use super::{DockTab, Package};
use crate::app::HammerProApp;

pub(crate) struct Lighting;

const TAB: &str = "lighting";
const MIN_UPDATE_DELAY: f64 = 1.0;
const BUDGET_IDLE: f64 = 0.6;
const BUDGET_INTERVAL: f64 = 1.5;
const LAYER_SIZE: u32 = 2048;
const MAX_TEXELS_PER_FRAME: usize = 4 << 20;

#[derive(Default)]
pub(crate) struct State {
    tab: hp_ui_mapping::lighting::LightingTab,
    preview: Option<Preview>,
    previous_mode: Option<RenderModeSetting>,
    failed: bool,
    budget_revision: u64,
    budget_at: f64,
    last_revision: u64,
    last_change: f64,
    cc_applied: Option<ColorCorrectionPlus>,
    cc_job: Option<(ColorCorrectionPlus, crossbeam_channel::Receiver<Option<Vec<u8>>>)>,
    grid: grid::EditorGrid,
    lightmap_changes: HashSet<u32>,
}

impl State {
    fn note_lightmap_changes(&mut self, changed: Vec<u32>) {
        self.lightmap_changes.extend(changed.iter().copied());
        self.grid.note_changes(changed);
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Scope {
    All,
    Around { center: DVec3, radius: f64 },
}

struct Running {
    id: u64,
    kind: BakeKind,
    serial: u64,
    scope: Scope,
    lit: bool,
    quick: bool,
}

struct Upload {
    baked: Box<bake::Baked>,
    kind: BakeKind,
    serial: u64,
    scope: Scope,
    written: usize,
    lit: bool,
    quick: bool,
}

struct Preview {
    baseline: Option<std::sync::Arc<hp_render::BakedLightBaseline>>,
    baseline_key: Option<u64>,
    worker: bake::Worker,
    next_id: u64,
    running: Option<Running>,
    upload: Option<Upload>,
    refine: Option<(Scope, u64)>,
    cancelling: bool,
    restart: Option<BakeKind>,
    auto_scoped: bool,
    cam_last: DVec3,
    cam_moved_at: f64,
    shown_serial: Option<u64>,
    shown_faces: u32,
    shown_blocks: u32,
    last_scope: Option<Scope>,
    has_bake: bool,
    cache: cache::BakeCache,
    lights_revision: u64,
    no_lights: bool,
    atlas_warned: bool,
    triangles: usize,
    indirect_serial: Option<u64>,
    indirect_failed: Option<u64>,
}

fn rad_quality(q: LightingQuality) -> hp_bspc::rad::RadQuality {
    match q {
        LightingQuality::Draft => hp_bspc::rad::RadQuality::Draft,
        LightingQuality::Normal => hp_bspc::rad::RadQuality::Normal,
        LightingQuality::Final => hp_bspc::rad::RadQuality::Final,
    }
}

pub(crate) fn primitive_resource_stamp(state: &State) -> u64 {
    state.preview.as_ref().map_or(0, |preview| preview.next_id)
}

pub(crate) fn primitive_resource_probes(
    state: &State,
    probes: &mut hp_app::runtime::primitives::ResourceProbes,
) -> bool {
    state.preview.as_ref().is_none_or(|preview| {
        if let Some(baseline) = &preview.baseline {
            let class = hp_app::runtime::primitives::PrimitiveClass::Lighting;
            probes.hold(baseline, class);
            if let Some(address) = baseline.allocation_address() {
                probes.point(address, class);
            }
        }
        preview.worker.resource_probes(probes)
    })
}

fn uses_hdr(l: LightingSetting) -> bool {
    !matches!(l, LightingSetting::Ldr)
}

fn bounces(bounced: bool) -> u32 {
    if bounced { 100 } else { 0 }
}

fn coarsen_factor(luxels: u64, budget: u64) -> f64 {
    if budget == 0 || luxels <= budget { 1.0 } else { (luxels as f64 / budget as f64).sqrt() }
}

fn initial_scope(
    center: DVec3,
    radius: f64,
    around: bool,
    automatic: bool,
    big: Option<f64>,
) -> (Scope, BakeKind, bool) {
    if around {
        (Scope::Around { center, radius }, BakeKind::AroundCamera, false)
    } else if automatic && let Some(radius) = big {
        (Scope::Around { center, radius }, BakeKind::AroundCamera, true)
    } else {
        (Scope::All, BakeKind::Full, false)
    }
}

fn bake_threads(setting: u32) -> (usize, usize) {
    if setting > 0 {
        return (setting as usize, setting as usize);
    }
    let cores = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    (cores.saturating_sub(2).max(1), cores.saturating_sub(1).max(1))
}

fn display_of(mode: RenderModeSetting) -> LightmapDisplay {
    match mode {
        RenderModeSetting::Baked => LightmapDisplay::Baked,
        RenderModeSetting::LightmapGrid => LightmapDisplay::Grid,
        _ => LightmapDisplay::Off,
    }
}

const NO_LIGHTMAP: u32 = 0x2 | 0x4 | 0x40 | 0x80 | 0x100 | 0x200 | 0x400;

fn budget_kind(state: &mut AppState, raw: &str) -> LightmapKind {
    material_kind(state.assets.as_mut(), raw)
}

fn material_kind(assets: &mut dyn hp_app::state::AssetsApi, raw: &str) -> LightmapKind {
    let name = hp_light::rad::normalize_material(raw);
    if hp_bspc::materials::facts_by_name(&name).surf & NO_LIGHTMAP != 0 {
        return LightmapKind::None;
    }
    let Some(info) = assets.material_info(&name) else { return LightmapKind::Flat };
    let shader = info.shader.to_ascii_lowercase();
    let unlit = shader.contains("unlitgeneric") || shader.contains("unlittwotexture");
    if info.tool.is_some() || info.is_water || unlit { LightmapKind::None } else { LightmapKind::Flat }
}

fn material_kinds(state: &mut AppState) -> HashMap<String, LightmapKind> {
    use rayon::prelude::*;
    let names: Vec<String> = {
        let set: HashSet<&str> = state
            .doc
            .scene()
            .solids()
            .par_values()
            .fold(HashSet::new, |mut set, n| {
                set.extend(n.solid.sides.iter().map(|s| s.material.as_str()));
                set
            })
            .reduce(HashSet::new, |mut a, b| {
                a.extend(b);
                a
            });
        set.into_iter().map(str::to_owned).collect()
    };
    names
        .into_iter()
        .map(|n| {
            let k = budget_kind(state, &n);
            (n, k)
        })
        .collect()
}

fn big_map_radius(state: &mut AppState) -> Option<f64> {
    let s = state.settings.lighting.sanitized();
    if !s.auto_scope {
        return None;
    }
    let limit = u64::from(s.auto_scope_luxels_millions) * 1_000_000;
    (map_luxels(state) > limit).then_some(f64::from(s.auto_scope_radius))
}

fn map_luxels(state: &mut AppState) -> u64 {
    let revision = state.doc.revision();
    if let Some(b) = state.runtime.lighting.budget.as_ref().filter(|b| b.revision == revision) {
        return b.luxels;
    }
    let kinds = material_kinds(state);
    let kind = |m: &str| kinds.get(m).copied().unwrap_or(LightmapKind::Flat);
    let opts = BudgetOptions { styles: 1, top: 0, visible_only: false, tiny_scale: 0.0 };
    state.doc.lightmap_budget(&opts, &kind).luxels
}

impl Preview {
    fn can_revalidate(&self, state: &AppState) -> bool {
        self.has_bake
            && self.shown_serial.is_some()
            && (self.cache.current(state, false, self.shown_serial)
                || (state.doc.light_plus().keep_bake
                    && self.baseline_key.is_some()
                    && self.baseline_key == self.cache.current_geometry_key(state)))
    }

    fn start(ctx: &egui::Context) -> Option<Self> {
        let ctx = ctx.clone();
        let worker = bake::Worker::start(Box::new(move || ctx.request_repaint())).ok()?;
        Some(Self {
            baseline: None,
            baseline_key: None,
            worker,
            next_id: 1,
            running: None,
            upload: None,
            refine: None,
            cancelling: false,
            restart: None,
            auto_scoped: false,
            cam_last: DVec3::ZERO,
            cam_moved_at: 0.0,
            shown_serial: None,
            shown_faces: 0,
            shown_blocks: 0,
            last_scope: None,
            has_bake: false,
            cache: cache::BakeCache::default(),
            lights_revision: 0,
            no_lights: false,
            atlas_warned: false,
            triangles: 0,
            indirect_serial: None,
            indirect_failed: None,
        })
    }

    fn sync_lights(&mut self, state: &AppState) {
        let revision = state.doc.revision();
        if revision == self.lights_revision {
            return;
        }
        self.lights_revision = revision;
        let set = hp_light::lights::collect(state.doc.scene().entities().values().map(|n| &n.entity), 1.0);
        self.no_lights = set.lights.is_empty() && set.sky.sun.is_none() && set.sky.ambient.max_element() <= 0.0;
    }

    fn bake(&mut self, state: &mut AppState, scope: Scope, kind: BakeKind, lit: bool) {
        let serial = self.cache.resolve(state, lit, 0.0, true).expect("forced lighting cache key");
        let quick = !lit && kind != BakeKind::Refine;
        if quick {
            self.refine = None;
        }
        self.cancelling = false;
        self.restart = None;
        self.upload = None;
        let t = std::time::Instant::now();
        let export = hp_editor::compile_export::CompileExport::default();
        let mut map = (kind != BakeKind::Refine).then(|| match &scope {
            Scope::All => state.doc.compile_snapshot(&export).map,
            Scope::Around { center, radius } => {
                let r = DVec3::splat(radius.max(64.0));
                state.doc.compile_snapshot_in(&export, &[hp_geom::Aabb::new(*center - r, *center + r)]).map
            }
        });
        let solids: HashMap<u32, bsp::SolidInfo> = {
            let renderer = state.gfx.as_ref().map(|g| &g.renderer);
            state
                .doc
                .scene()
                .solids()
                .iter()
                .map(|(id, n)| {
                    let info = bsp::SolidInfo {
                        lo: n.bounds.min.as_vec3(),
                        hi: n.bounds.max.as_vec3(),
                        generation: renderer.map_or(0, |r| r.lightmap_generation(*id)),
                    };
                    (*id, info)
                })
                .collect()
        };
        let s = state.settings.lighting.sanitized();
        let options = state.doc.light_plus();
        if let Some(map) = &mut map {
            map.entities.retain(|entity| options.uses_entity(entity.id));
        }
        let budget = u64::from(s.max_luxels_millions) * 1_000_000;
        let factor = coarsen_factor(map_luxels(state), budget);
        let (layer_size, max_layers) = state.gfx.as_ref().map_or((LAYER_SIZE, 16), |g| {
            let limits = g.device.limits();
            (LAYER_SIZE.min(limits.max_texture_dimension_2d).max(64), limits.max_texture_array_layers.clamp(1, 64))
        });
        let id = self.next_id;
        self.next_id += 1;
        self.worker.submit(bake::Job {
            id,
            serial: cache::scope_serial(serial, &scope),
            map,
            keep: quick,
            solids,
            fs: state.assets.fs().cloned(),
            resources_revision: state.assets.lighting_generation(),
            hdr: if lit { state.settings.render.light_preview_hdr } else { uses_hdr(state.settings.compile.lighting) },
            quality: rad_quality(s.quality),
            bounces: if options.bounced { options.bounce_count.min(bounces(true)) } else { bounces(false) },
            prop_shadows: state.settings.compile.static_prop_shadows,
            prop_lighting: state.settings.compile.static_prop_lighting,
            threads: bake_threads(s.threads),
            layer_size,
            max_layers,
            coarsen: factor,
            lit,
            tuning: if quick { child::Tuning::of(&options).quick() } else { child::Tuning::of(&options) },
            exe: bake::child_exe(),
        });
        log::info!(
            "lighting preview: bake {id} ({kind:?}{}) started, map copied in {} ms",
            if lit { ", for the Lit mode" } else { "" },
            t.elapsed().as_millis()
        );
        self.running = Some(Running { id, kind, serial, scope: scope.clone(), lit, quick });
        self.last_scope = Some(scope);
        state.runtime.lighting.stale = false;
        state.runtime.lighting.progress =
            Some(BakeProgress { kind, phase: BakePhase::Preparing, fraction: 0.0, elapsed_s: 0.0, faces: 0 });
    }

    fn stop(&mut self, state: &mut AppState) {
        self.refine = None;
        self.cancelling = false;
        self.restart = None;
        if let Some(r) = self.running.take() {
            self.worker.stop();
            state.runtime.lighting.progress = None;
            if !r.lit {
                state.runtime.lighting.last =
                    Some(BakeSummary { kind: r.kind, cancelled: true, density: 1.0, ..Default::default() });
            }
        }
    }

    fn poll(&mut self, state: &mut AppState) {
        for msg in self.worker.poll() {
            match msg {
                bake::Msg::Progress { job, phase, fraction, elapsed, faces } => {
                    let Some(r) = self.running.as_ref().filter(|r| r.id == job) else { continue };
                    state.runtime.lighting.progress =
                        Some(BakeProgress { kind: r.kind, phase, fraction, elapsed_s: elapsed, faces });
                }
                bake::Msg::Done { job, outcome } => {
                    let Some(r) = self.running.take_if(|r| r.id == job) else { continue };
                    match outcome {
                        bake::Outcome::Baked(baked) => {
                            let props = baked.props.as_deref();
                            log::info!(
                                "lighting bake {:?}: {} faces, {} luxels, {:.2} s ({} faces of no editor solid{}); models lit by {} \
                                 world lights, {} static props ({} with vertex lighting)",
                                r.kind,
                                baked.faces.len(),
                                baked.luxels,
                                baked.seconds,
                                baked.unbound,
                                if baked.leaked { ", the map leaks" } else { "" },
                                props.map_or(0, |p| p.lights.len()),
                                props.map_or(0, |p| p.static_props.len()),
                                props.map_or(0, |p| p.static_props.iter().filter(|s| s.colors.is_some()).count()),
                            );
                            if let Some(peak) = baked.child_peak {
                                log::info!(
                                    "lighting preview: the lighting process peaked at {} MB, the editor uses {} MB",
                                    peak.peak_mb(),
                                    memory::of_self().map_or(0, |u| u.working_set >> 20)
                                );
                            }
                            if !baked.warnings.is_empty() {
                                log::warn!("lighting preview: {}", baked.warnings.join("; "));
                                state.notify(
                                    Severity::Warning,
                                    tf!(
                                        "app.lighting.limits",
                                        baked.warnings.join(
                                            "
"
                                        )
                                    ),
                                );
                            }
                            self.triangles = baked.triangles;
                            self.upload = Some(Upload {
                                baked,
                                kind: r.kind,
                                serial: r.serial,
                                scope: r.scope,
                                written: 0,
                                lit: r.lit,
                                quick: r.quick,
                            });
                        }
                        bake::Outcome::Cancelled => {
                            state.runtime.lighting.progress = None;
                            if std::mem::take(&mut self.cancelling) {
                                self.restart = Some(r.kind);
                            } else if !r.lit && r.kind != BakeKind::Refine {
                                state.runtime.lighting.last = Some(BakeSummary {
                                    kind: r.kind,
                                    cancelled: true,
                                    density: 1.0,
                                    ..Default::default()
                                });
                            }
                        }
                        bake::Outcome::Failed(e) => {
                            log::error!("lighting preview: {e}");
                            state.runtime.lighting.progress = None;
                            if r.lit {
                                self.indirect_failed = Some(r.serial);
                            } else {
                                state.notify(Severity::Error, tf!("app.lighting.failed", e));
                            }
                        }
                    }
                }
            }
        }
    }

    fn upload_indirect(&mut self, state: &mut AppState, up: Upload) {
        let Some(gfx) = state.gfx.as_mut() else { return };
        let b = up.baked;
        gfx.renderer.set_indirect_volume(b.indirect.clone());
        let rt = &mut state.runtime.lighting;
        rt.progress = self.running.as_ref().and(rt.progress);
        self.indirect_serial = b.indirect.as_ref().map(|_| up.serial);
        self.triangles = b.triangles;
        log::info!(
            "lighting preview: {} indirect light cells ({} in the air) in {:.2} s",
            b.indirect.as_ref().map_or(0, |v| v.cells()),
            b.indirect.as_ref().map_or(0, |v| v.valid_cells()),
            b.seconds
        );
        rt.indirect.seconds = b.seconds;
    }

    fn upload(&mut self, state: &mut AppState) {
        let Some(mut up) = self.upload.take() else { return };
        let delay = f64::from(state.doc.light_plus().incremental_delay).max(MIN_UPDATE_DELAY);
        match self.cache.resolve(state, up.lit, delay, false) {
            None => {
                self.upload = Some(up);
                return;
            }
            Some(serial) if serial != up.serial => {
                state.runtime.lighting.progress = None;
                state.runtime.lighting.stale = self.has_bake;
                if !up.lit {
                    self.restart = Some(BakeKind::Update);
                }
                return;
            }
            Some(_) => {}
        }
        if up.lit {
            self.upload_indirect(state, up);
            return;
        }
        let AppState { gfx, assets, runtime, .. } = state;
        let Some(gfx) = gfx.as_mut() else { return };
        let r = &mut gfx.renderer;
        if up.written == 0 {
            self.shown_serial = None;
            let fits = show_geometry(r, assets.as_mut(), &up.baked, (self.shown_faces, self.shown_blocks));
            if !fits && !self.atlas_warned {
                self.atlas_warned = true;
                let b = &up.baked;
                log::warn!("lighting preview: the GPU cannot hold {} lightmap layers of {}", b.layers, b.layer_size);
            }
            self.shown_faces = up.baked.faces.len() as u32;
            self.shown_blocks = up.baked.blocks.len() as u32;
            up.baked.blocks = Vec::new();
        }
        up.written = write_texels(r, &up.baked, up.written, MAX_TEXELS_PER_FRAME);
        let b = &up.baked;
        let rt = &mut runtime.lighting;
        if up.written < b.faces.len() {
            let fraction = 0.95 + 0.05 * up.written as f32 / b.faces.len().max(1) as f32;
            if let Some(p) = rt.progress.as_mut() {
                p.phase = BakePhase::Finishing;
                p.fraction = fraction;
            }
            self.upload = Some(up);
            return;
        }
        let b = up.baked;
        let region = match up.scope {
            Scope::All => None,
            Scope::Around { center, radius } => {
                let r = radius.max(64.0);
                Some([(center - DVec3::splat(r)).to_array(), (center + DVec3::splat(r)).to_array()])
            }
        };
        assets.set_baked_prop_lighting(b.props.clone(), region);
        if b.indirect.is_some() && b.hdr == state.settings.render.light_preview_hdr {
            r.set_indirect_volume(b.indirect.clone());
        }
        rt.progress = self.running.as_ref().and(rt.progress);
        rt.has_bake = true;
        let density = (1.0 / b.coarsen.max(1.0)) as f32;
        rt.last = Some(BakeSummary {
            kind: up.kind,
            cancelled: false,
            seconds: b.seconds,
            faces: b.faces.len(),
            luxels: b.luxels,
            density,
            atlas_full: b.atlas_full,
        });
        rt.stats = PreviewStats {
            faces: b.faces.len(),
            baked_faces: b.faces.iter().filter(|f| f.rect.is_some()).count(),
            luxels: b.luxels,
            triangles: self.triangles,
            layers: b.layers,
            gpu_bytes: r.lightmap_stats().bytes,
            world_lights: b.props.as_ref().map_or(0, |p| p.lights.len()),
            static_props: b.props.as_ref().map_or(0, |p| p.static_props.len()),
            vertex_lit_props: b
                .props
                .as_ref()
                .map_or(0, |p| p.static_props.iter().filter(|s| s.colors.is_some()).count()),
            scope_radius: match &up.scope {
                Scope::All => 0.0,
                Scope::Around { radius, .. } => radius.max(64.0) as f32,
            },
            scope_auto: self.auto_scoped,
        };
        self.has_bake = true;
        self.baseline = Some(r.capture_baked_lights());
        self.baseline_key = Some(self.cache.geometry_key(state));
        self.shown_serial = Some(up.serial);
        self.refine = up.quick.then(|| (up.scope.clone(), up.serial));
        if b.indirect.is_some() && b.hdr == state.settings.render.light_preview_hdr {
            self.indirect_serial = self.cache.resolve(state, true, 0.0, true);
        }
        if !matches!(up.kind, BakeKind::Update | BakeKind::Refine) {
            let text = tf!(
                "app.lighting.baked",
                hp_ui_mapping::lighting::fmt_count(b.faces.len() as u64),
                hp_ui_mapping::lighting::fmt_decimal(f64::from(b.seconds), 1),
                hp_ui_mapping::lighting::fmt_count(b.luxels)
            );
            state.log(Severity::Info, text);
        }
    }

    fn schedule(&mut self, state: &mut AppState, now: f64) {
        if !self.has_bake && self.running.is_none() && self.upload.is_none() && self.restart.is_none() {
            return;
        }
        let options = state.doc.light_plus();
        let geometry_key = self.has_bake.then(|| self.cache.geometry_key(state));
        let delay = f64::from(options.incremental_delay).max(MIN_UPDATE_DELAY);
        let Some(serial) = self.cache.resolve(state, false, delay, false) else {
            return;
        };
        let shown_current = self.shown_serial == Some(serial);
        if shown_current {
            self.restart = None;
            if self.running.as_ref().is_some_and(|r| !r.lit && r.serial != serial) {
                self.worker.stop();
                self.running = None;
                self.cancelling = false;
                self.restart = None;
                state.runtime.lighting.progress = None;
            }
            if self.upload.as_ref().is_some_and(|u| !u.lit && u.serial != serial) {
                self.upload = None;
                state.runtime.lighting.progress = None;
            }
        }
        let current = shown_current
            .then_some(serial)
            .or_else(|| self.running.as_ref().map(|r| r.serial))
            .or_else(|| self.upload.as_ref().map(|u| u.serial))
            .or(self.shown_serial);
        if current == Some(serial) {
            state.runtime.lighting.stale = false;
            if self.running.is_none()
                && self.upload.is_none()
                && let Some((scope, _)) = self.refine.take_if(|(_, s)| *s == serial)
            {
                self.bake(state, scope, BakeKind::Refine, false);
            }
            self.follow_camera(state, now);
            return;
        }
        if !options.incremental {
            state.runtime.lighting.stale = self.has_bake && self.running.is_none();
            return;
        }
        if options.keep_bake
            && self.shown_serial.is_some()
            && self.baseline_key.is_some()
            && self.baseline_key == geometry_key
        {
            if self.running.as_ref().is_some_and(|running| !running.lit && running.serial != serial) {
                self.worker.stop();
                self.running = None;
            }
            self.refine = None;
            self.restart = None;
            if self.upload.as_ref().is_some_and(|upload| !upload.lit && upload.serial != serial) {
                self.upload = None;
            }
            if self.running.is_none() {
                state.runtime.lighting.progress = None;
            }
            state.runtime.lighting.stale = true;
            return;
        }
        if !self.cancelling && self.running.as_ref().is_some_and(|r| !r.lit && r.serial != serial) {
            self.cancelling = true;
            self.worker.stop();
        }
        let scope = self.last_scope.clone().unwrap_or(Scope::All);
        let kind = self.running.as_ref().map(|r| r.kind).or(self.restart).unwrap_or(BakeKind::Update);
        let kind = if kind == BakeKind::Refine { BakeKind::Update } else { kind };
        self.bake(state, scope, kind, false);
    }

    fn follow_camera(&mut self, state: &mut AppState, now: f64) {
        if !state.settings.lighting.auto_scope {
            self.auto_scoped = false;
        }
        let cam = state.cameras.cam3d.pos;
        if cam != self.cam_last {
            self.cam_last = cam;
            self.cam_moved_at = now;
        }
        if !self.auto_scoped
            || !self.has_bake
            || self.running.is_some()
            || self.upload.is_some()
            || self.refine.is_some()
            || now - self.cam_moved_at < 1.0
        {
            return;
        }
        let Some(Scope::Around { center, radius }) = self.last_scope.clone() else { return };
        if (cam - center).length() < radius * 0.5 {
            return;
        }
        self.bake(state, Scope::Around { center: cam, radius }, BakeKind::AroundCamera, false);
    }

    fn note_indirect(&self, state: &mut AppState) {
        let volume = state.gfx.as_ref().and_then(|g| g.renderer.indirect_volume().cloned());
        let current = self.cache.current(state, true, self.indirect_serial);
        let on = state.settings.render.lit_indirect;
        let waiting = self.running.as_ref().is_some_and(|r| r.lit) || self.upload.as_ref().is_some_and(|u| u.lit);
        let phase = if !on {
            IndirectPhase::Off
        } else if self.no_lights {
            IndirectPhase::NoLights
        } else if waiting {
            IndirectPhase::Baking
        } else if self.cache.current(state, true, self.indirect_failed) {
            IndirectPhase::Failed
        } else if volume.is_none() {
            IndirectPhase::Waiting
        } else if current {
            IndirectPhase::Ready
        } else {
            IndirectPhase::Stale
        };
        let seconds = state.runtime.lighting.indirect.seconds;
        state.runtime.lighting.indirect = IndirectStatus {
            phase,
            cells: volume.as_ref().map_or(0, |v| v.cells()),
            valid_cells: volume.as_ref().map_or(0, |v| v.valid_cells()),
            spacing: volume.as_ref().map_or(0.0, |v| v.spacing),
            gpu_bytes: volume.as_ref().map_or(0, |v| v.gpu_bytes()),
            seconds,
        };
    }

    fn schedule_lit(&mut self, state: &mut AppState, shown: bool) {
        if self.running.is_some() || self.upload.is_some() || self.no_lights {
            return;
        }
        let options = state.doc.light_plus();
        if shown && !options.incremental {
            return;
        }
        let delay = if shown { f64::from(options.incremental_delay).max(MIN_UPDATE_DELAY) } else { MIN_UPDATE_DELAY };
        let Some(serial) = self.cache.resolve(state, true, delay, false) else { return };
        if self.indirect_serial == Some(serial) || self.indirect_failed == Some(serial) {
            return;
        }
        self.bake(state, Scope::All, BakeKind::Update, true);
    }
}

fn show_geometry(
    r: &mut hp_render::Renderer,
    assets: &mut dyn hp_app::state::AssetsApi,
    b: &bake::Baked,
    shown: (u32, u32),
) -> bool {
    r.set_lightmap_ldr(!b.hdr);
    let old: Vec<u32> = (0..shown.0).collect();
    r.remove_lightmap_faces(&old);
    for id in 0..shown.1 {
        r.remove_lightmap_block(id);
    }
    let fits = b.layers == 0 || r.ensure_lightmap_layers(b.layer_size, b.layers);
    let faces: Vec<(u32, LightmapFace)> = b
        .faces
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let face = LightmapFace { object: f.solid, generation: f.generation, scale: f.scale, flags: f.flags };
            (i as u32, face)
        })
        .collect();
    r.add_lightmap_faces(&faces);
    let materials: Vec<hp_render::MaterialId> = b.materials.iter().map(|m| assets.render_material_id(m)).collect();
    for (id, block) in b.blocks.iter().enumerate() {
        let batches: Vec<LightmapBatch<'_>> = block
            .batches
            .iter()
            .filter_map(|(m, v, i)| {
                Some(LightmapBatch { material: *materials.get(*m as usize)?, vertices: v, indices: i })
            })
            .collect();
        r.add_lightmap_block(id as u32, &batches);
    }
    fits
}

fn write_texels(r: &mut hp_render::Renderer, b: &bake::Baked, from: usize, budget: usize) -> usize {
    let mut written = from;
    let mut batch: Vec<(u32, LightmapRect, &[u32])> = Vec::new();
    let mut texels = 0usize;
    while written < b.faces.len() && texels < budget {
        let i = written;
        written += 1;
        let f = &b.faces[i];
        let Some(rect) = f.rect else { continue };
        let n = (rect.w * rect.h) as usize;
        let Some(data) = b.texels.get(f.texels as usize..f.texels as usize + n) else { continue };
        let images = f.images.max(1);
        let rect = LightmapRect {
            layer: rect.layer,
            x: rect.x,
            y: rect.y,
            w: rect.w / images,
            h: rect.h,
            density: 1.0,
            bumped: images == 4,
        };
        batch.push((i as u32, rect, data));
        texels += n;
    }
    r.write_lightmaps(&batch);
    written
}

fn read_lut(fs: Option<&hp_fs::GameFs>, map_dir: Option<&std::path::Path>, name: &str) -> Option<Vec<u8>> {
    let name = name.trim().replace('\\', "/");
    if name.is_empty() || name.contains("..") || name.contains(':') || name.starts_with('/') {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    let size_ok = |b: &Vec<u8>| hp_light::color::is_raw_lut(b);
    if let Some(bytes) = fs.and_then(|f| f.read(&lower)).filter(size_ok) {
        return Some(bytes);
    }
    let path = map_dir?.join(&name);
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() != hp_light::color::RAW_BYTES as u64 {
        return None;
    }
    std::fs::read(&path).ok().filter(size_ok)
}

fn revalidate_cached_faces(app: &mut HammerProApp) {
    let lighting = &mut app.pkg.lighting;
    if lighting.lightmap_changes.is_empty() || !lighting.preview.as_ref().is_some_and(|p| p.can_revalidate(&app.state))
    {
        return;
    }
    let Some(gfx) = app.state.gfx.as_mut() else { return };
    let renderer = &mut gfx.renderer;
    let changed: Vec<_> = lighting.lightmap_changes.drain().map(|id| (id, renderer.lightmap_generation(id))).collect();
    renderer.revalidate_lightmap_objects(&changed);
}

pub(crate) fn scene_synced(app: &mut HammerProApp) {
    if let Some(baseline) = app.pkg.lighting.preview.as_ref().and_then(|preview| preview.baseline.as_ref())
        && let Some(gfx) = app.state.gfx.as_mut()
    {
        gfx.renderer.restore_baked_lights(baseline);
    }
    if let Some(gfx) = app.state.gfx.as_mut() {
        app.pkg.lighting.note_lightmap_changes(gfx.renderer.take_lightmap_changes());
    }
    revalidate_cached_faces(app);
    let bake_shown = app.pkg.lighting.preview.as_ref().is_some_and(|p| p.has_bake || p.upload.is_some());
    let AppState { gfx, doc, assets, .. } = &mut app.state;
    let Some(gfx) = gfx.as_mut() else { return };
    let r = &mut gfx.renderer;
    let editor_grid = &mut app.pkg.lighting.grid;
    if r.lightmap_display() != LightmapDisplay::Grid || bake_shown {
        editor_grid.clear(r);
        return;
    }
    editor_grid.content_changed(r, assets.generation());
    let mut known: HashMap<String, Option<hp_render::MaterialId>> = HashMap::new();
    let mut lit = |name: &str| {
        if let Some(m) = known.get(name) {
            return *m;
        }
        let lit = material_kind(assets.as_mut(), name) != LightmapKind::None;
        let m = lit.then(|| assets.render_material_id(name));
        known.insert(name.to_owned(), m);
        m
    };
    editor_grid.sync(r, doc.scene().solids(), &mut lit);
}

const HANDLED: &[Action] = &[
    Action::RenderBaked,
    Action::RenderLightmapGrid,
    Action::BakeLighting,
    Action::BakeAroundCamera,
    Action::StopBake,
    Action::SelectTinyLightmapFaces,
    Action::ShowLighting,
];

impl Lighting {
    fn set_mode(app: &mut HammerProApp, mode: RenderModeSetting) {
        let current = app.state.settings.view.render_mode;
        if current == mode {
            return;
        }
        if !matches!(current, RenderModeSetting::Baked | RenderModeSetting::LightmapGrid) {
            app.pkg.lighting.previous_mode = Some(current);
        }
        app.state.settings.view.render_mode = mode;
        app.state.settings_dirty = true;
    }

    fn toggle_mode(app: &mut HammerProApp, mode: RenderModeSetting) {
        if app.state.settings.view.render_mode == mode {
            let back = app.pkg.lighting.previous_mode.take().unwrap_or(RenderModeSetting::Textured);
            app.state.settings.view.render_mode = back;
            app.state.settings_dirty = true;
        } else {
            Self::set_mode(app, mode);
        }
    }

    fn preview<'a>(app: &'a mut HammerProApp, ctx: &egui::Context) -> Option<&'a mut Preview> {
        if app.pkg.lighting.preview.is_none() && !app.pkg.lighting.failed {
            match Preview::start(ctx) {
                Some(p) => app.pkg.lighting.preview = Some(p),
                None => {
                    app.pkg.lighting.failed = true;
                    app.state.notify(Severity::Error, t!("app.lighting.start_failed"));
                    return None;
                }
            }
        }
        let p = app.pkg.lighting.preview.as_mut()?;
        p.sync_lights(&app.state);
        app.state.runtime.lighting.no_lights = p.no_lights;
        Some(p)
    }

    fn start_bake(app: &mut HammerProApp, ctx: &egui::Context, around: bool, automatic: bool) {
        Self::set_mode(app, RenderModeSetting::Baked);
        let radius = f64::from(app.state.doc.light_plus().bake_dist);
        let center = app.state.cameras.cam3d.pos;
        if Self::preview(app, ctx).is_none() {
            return;
        }
        let Some(mut p) = app.pkg.lighting.preview.take() else { return };
        let big = if around || !automatic { None } else { big_map_radius(&mut app.state) };
        let (scope, kind, auto_scoped) = initial_scope(center, radius, around, automatic, big);
        p.auto_scoped = auto_scoped;
        if let Some(radius) = big {
            app.state.notify(Severity::Info, tf!("app.lighting.big_map", radius.round()));
        }
        p.bake(&mut app.state, scope, kind, false);
        app.pkg.lighting.preview = Some(p);
    }

    fn select_tiny(app: &mut HammerProApp) {
        let tiny = f64::from(app.state.settings.lighting.sanitized().tiny_scale);
        let kinds = material_kinds(&mut app.state);
        let kind = |m: &str| kinds.get(m).copied().unwrap_or(LightmapKind::Flat);
        let faces: Vec<FaceRef> = app.state.doc.faces_with_lightmap_scale_at_most(tiny, &kind);
        if faces.is_empty() {
            app.state.notify(Severity::Info, tf!("app.lighting.no_tiny", tiny));
            return;
        }
        app.state.doc.select_faces(&faces, SelectOp::Replace);
        app.state.set_tool(ToolId::Texture);
        app.state.notify(Severity::Info, tf!("app.lighting.selected_tiny", faces.len(), tiny));
    }

    fn color_correction(app: &mut HammerProApp, ctx: &egui::Context) {
        if let Some((cc, rx)) = &app.pkg.lighting.cc_job {
            match rx.try_recv() {
                Ok(lut) => {
                    if let Some(gfx) = app.state.gfx.as_mut() {
                        gfx.renderer.set_color_correction(lut.as_deref());
                    }
                    if lut.is_none() && cc.is_active() {
                        app.state.log(Severity::Warning, t!("app.lighting.cc_missing"));
                    }
                    app.pkg.lighting.cc_applied = Some(cc.clone());
                    app.pkg.lighting.cc_job = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => return,
                Err(crossbeam_channel::TryRecvError::Disconnected) => app.pkg.lighting.cc_job = None,
            }
        }
        let wanted = app.state.doc.color_correction_plus();
        if app.pkg.lighting.cc_applied.as_ref() == Some(&wanted) || app.state.gfx.is_none() {
            return;
        }
        if !wanted.is_active() {
            if let Some(gfx) = app.state.gfx.as_mut() {
                gfx.renderer.set_color_correction(None);
            }
            app.pkg.lighting.cc_applied = Some(wanted);
            return;
        }
        let fs = app.state.assets.fs().cloned();
        let map_dir = app.state.doc.path().and_then(|p| p.parent()).map(std::path::Path::to_path_buf);
        let (tx, rx) = crossbeam_channel::bounded(1);
        let slots = wanted.slots.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-light-cc".to_owned()).spawn(move || {
            let files: Vec<(Vec<u8>, f32)> = slots
                .iter()
                .filter(|s| s.weight > 0.0)
                .filter_map(|s| read_lut(fs.as_ref(), map_dir.as_deref(), &s.name).map(|b| (b, s.weight)))
                .collect();
            let refs: Vec<(&[u8], f32)> = files.iter().map(|(b, w)| (b.as_slice(), *w)).collect();
            let _ = tx.send(hp_light::color::blend_luts(&refs));
            ctx.request_repaint();
        });
        if spawned.is_ok() {
            app.pkg.lighting.cc_job = Some((wanted, rx));
        } else {
            app.pkg.lighting.cc_applied = Some(wanted);
        }
    }

    fn budget(app: &mut HammerProApp, now: f64) {
        let state = &mut app.state;
        let wanted = state.runtime.lighting.budget_wanted || state.settings.lighting.show_budget;
        state.runtime.lighting.budget_wanted = false;
        let pkg = &mut app.pkg.lighting;
        let revision = state.doc.revision();
        if revision != pkg.last_revision {
            pkg.last_revision = revision;
            pkg.last_change = now;
        }
        let fresh = state.runtime.lighting.budget.as_ref().is_some_and(|b| b.revision == revision);
        if !wanted || fresh || pkg.budget_revision == revision {
            return;
        }
        let first = state.runtime.lighting.budget.is_none();
        if !first && (now - pkg.last_change < BUDGET_IDLE || now - pkg.budget_at < BUDGET_INTERVAL) {
            return;
        }
        pkg.budget_revision = revision;
        pkg.budget_at = now;
        let s = state.settings.lighting.sanitized();
        let kinds = material_kinds(state);
        let kind = |m: &str| kinds.get(m).copied().unwrap_or(LightmapKind::Flat);
        let opts = BudgetOptions {
            styles: s.budget_styles,
            top: 20,
            visible_only: false,
            tiny_scale: f64::from(s.tiny_scale),
        };
        let b = state.doc.lightmap_budget(&opts, &kind);
        let tiny_faces = b.tiny_faces;
        state.runtime.lighting.budget = Some(BudgetSummary {
            faces: b.faces,
            luxels: b.luxels,
            bytes: b.bytes,
            limit: MAX_MAP_LIGHTING,
            tiny_faces,
            wide_faces: b.wide_faces,
            oversized_displacements: b.oversized_displacements,
            top: b
                .top
                .iter()
                .map(|f| BudgetEntry {
                    solid: f.face.solid,
                    side: f.face.side,
                    bytes: f.bytes,
                    scale: f.scale,
                    dims: f.dims,
                    displacement: f.displacement,
                })
                .collect(),
            revision,
        });
    }
}

pub(crate) fn release_hidden(app: &mut HammerProApp) -> bool {
    if display_of(app.state.settings.view.render_mode) != LightmapDisplay::Off {
        return false;
    }
    let shown = app.state.gfx.as_ref().is_some_and(|g| g.renderer.lightmap_stats().bytes > 0);
    if shown {
        forget(app);
    }
    shown
}

fn forget(app: &mut HammerProApp) {
    if let Some(gfx) = app.state.gfx.as_mut() {
        gfx.renderer.clear_lightmaps();
    }
    app.pkg.lighting.grid.forget();
    app.pkg.lighting.lightmap_changes.clear();
    app.state.assets.set_baked_prop_lighting(None, None);
    if let Some(p) = app.pkg.lighting.preview.as_mut() {
        p.worker.stop();
        p.running = None;
        p.upload = None;
        p.refine = None;
        p.shown_serial = None;
        p.shown_faces = 0;
        p.shown_blocks = 0;
        p.has_bake = false;
        p.baseline = None;
        p.baseline_key = None;
        p.last_scope = None;
        p.lights_revision = 0;
        p.cache = cache::BakeCache::default();
    }
    let rt = &mut app.state.runtime.lighting;
    rt.progress = None;
    rt.last = None;
    rt.has_bake = false;
    rt.stale = false;
    rt.no_lights = false;
    rt.stats = PreviewStats::default();
    rt.budget = None;
    app.pkg.lighting.budget_revision = 0;
    app.pkg.lighting.cc_applied = None;
    app.pkg.lighting.cc_job = None;
}

impl Package for Lighting {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::RenderBaked => {
                let was_baked = app.state.settings.view.render_mode == RenderModeSetting::Baked;
                Self::toggle_mode(app, RenderModeSetting::Baked);
                let has_bake = app
                    .pkg
                    .lighting
                    .preview
                    .as_ref()
                    .is_some_and(|p| p.has_bake || p.running.as_ref().is_some_and(|r| !r.lit));
                if !was_baked && app.state.settings.lighting.auto_bake && !has_bake {
                    Self::start_bake(app, ctx, false, true);
                }
            }
            Action::RenderLightmapGrid => Self::toggle_mode(app, RenderModeSetting::LightmapGrid),
            Action::BakeLighting => Self::start_bake(app, ctx, false, false),
            Action::BakeAroundCamera => Self::start_bake(app, ctx, true, false),
            Action::StopBake => {
                if let Some(mut p) = app.pkg.lighting.preview.take() {
                    p.stop(&mut app.state);
                    app.pkg.lighting.preview = Some(p);
                }
            }
            Action::SelectTinyLightmapFaces => Self::select_tiny(app),
            Action::ShowLighting => app.state.ui.right_tab = RightTab::Package(TAB),
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        let now = app.state.frame.time;
        let mode = app.state.settings.view.render_mode;
        let display = display_of(mode);
        let options = app.state.doc.light_plus();
        let active = display != LightmapDisplay::Off;
        if let Some(gfx) = app.state.gfx.as_mut() {
            gfx.renderer.set_lightmap_display(display, options.brightness_scale);
            gfx.renderer.set_lightmap_tracking(active);
            app.pkg.lighting.note_lightmap_changes(gfx.renderer.take_lightmap_changes());
        }
        let lit_wanted = mode == RenderModeSetting::Lit && app.state.settings.render.lit_indirect;
        if active || lit_wanted {
            Self::preview(app, ctx);
        }
        if let Some(mut p) = app.pkg.lighting.preview.take() {
            if active {
                p.schedule(&mut app.state, now);
            } else if lit_wanted {
                let shown = app.state.gfx.as_ref().is_some_and(|g| g.renderer.indirect_volume().is_some());
                p.schedule_lit(&mut app.state, shown);
            }
            p.poll(&mut app.state);
            if p.upload.is_some()
                && let Some(gfx) = app.state.gfx.as_mut()
            {
                app.pkg.lighting.grid.clear(&mut gfx.renderer);
            }
            p.upload(&mut app.state);
            if p.running.is_some()
                || p.upload.is_some()
                || ((active || lit_wanted)
                    && (p.cache.pending(lit_wanted && !active) || (p.restart.is_some() && options.incremental)))
            {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            p.note_indirect(&mut app.state);
            app.pkg.lighting.preview = Some(p);
        }
        revalidate_cached_faces(app);
        Self::budget(app, now);
        Self::color_correction(app, ctx);
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        forget(app);
        if let Some(gfx) = app.state.gfx.as_mut() {
            gfx.renderer.set_indirect_volume(None);
        }
        if let Some(p) = app.pkg.lighting.preview.as_mut() {
            p.indirect_serial = None;
        }
        app.state.runtime.lighting.indirect = IndirectStatus::default();
    }

    fn init(&self, app: &mut HammerProApp) {
        app.viewports.add_layer(Box::new(reach::LightReachLayer));
    }

    fn dock_tabs(&self, _app: &HammerProApp, tabs: &mut Vec<DockTab>) {
        tabs.push(DockTab { id: TAB, label: t!("action.show_lighting").to_owned() });
    }

    fn dock_ui(&self, app: &mut HammerProApp, ui: &mut egui::Ui, id: &'static str) -> bool {
        if id != TAB {
            return false;
        }
        app.pkg.lighting.tab.ui(ui, &mut app.state);
        true
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

mod reach {
    use hp_app::AppState;
    use hp_app::view::ViewInfo;
    use hp_render::{Overlay, OverlayLine};
    use hp_ui_viewports::layers::OverlayLayer;

    const SEGMENTS: usize = 48;
    const MAX_LIGHTS: usize = 64;

    pub(super) struct LightReachLayer;

    impl OverlayLayer for LightReachLayer {
        fn name(&self) -> &'static str {
            "light_reach"
        }

        fn add(&self, state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
            if !state.doc.light_plus().debug {
                return;
            }
            let scene = state.doc.scene();
            let mut drawn = 0;
            for id in state.doc.selected_entity_ids() {
                if drawn >= MAX_LIGHTS {
                    break;
                }
                let Some(n) = scene.entity(id) else { continue };
                let Some(l) = hp_light::lights::parse_light(&n.entity, 1.0) else { continue };
                drawn += 1;
                let c = l.origin;
                let r = l.reach;
                let color = [1.0, 0.85, 0.3, 0.8];
                for axes in [
                    (glam::Vec3A::X, glam::Vec3A::Y),
                    (glam::Vec3A::X, glam::Vec3A::Z),
                    (glam::Vec3A::Y, glam::Vec3A::Z),
                ] {
                    let point = |i: usize| {
                        let a = std::f32::consts::TAU * i as f32 / SEGMENTS as f32;
                        (c + (axes.0 * a.cos() + axes.1 * a.sin()) * r).to_array()
                    };
                    for i in 0..SEGMENTS {
                        out.lines.push(OverlayLine { a: point(i), b: point(i + 1), color });
                    }
                }
            }
        }
    }
}

const TR: TrTable = &[
    (
        "app.lighting.baked",
        "Lighting baked: {0} faces in {1} s ({2} luxels)",
        "Éclairage précalculé : {0} faces en {1} s ({2} luxels)",
    ),
    (
        "app.lighting.cc_missing",
        "Color correction: no lookup file could be read (32 x 32 x 32 .raw files)",
        "Correction des couleurs : aucun fichier de correspondance n'a pu être lu (fichiers .raw de 32 x 32 x 32)",
    ),
    ("app.lighting.failed", "The lighting preview failed: {0}", "L'aperçu de l'éclairage a échoué : {0}"),
    (
        "app.lighting.limits",
        "Lighting preview made past limits of the game (a compile for the game stops on them): {0}",
        "Aperçu de l'éclairage fait au-delà de limites du jeu (une compilation pour le jeu s'y arrête) : {0}",
    ),
    (
        "app.lighting.big_map",
        "Big map: the lighting preview covers {0} units around the camera and follows it (Tools > Options > Lighting)",
        "Grande carte : l'aperçu de l'éclairage couvre {0} unités autour de la caméra et la suit (Outils > Options > Éclairage)",
    ),
    (
        "app.lighting.start_failed",
        "The lighting preview could not start (see the log file)",
        "Impossible de démarrer l'aperçu de l'éclairage (voir le fichier journal)",
    ),
    (
        "app.lighting.no_tiny",
        "No visible face has a lightmap scale of {0} or less",
        "Aucune face visible n'a une échelle de lightmap de {0} ou moins",
    ),
    (
        "app.lighting.selected_tiny",
        "{0} faces with a lightmap scale of {1} or less selected",
        "{0} faces avec une échelle de lightmap de {1} ou moins sélectionnées",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty());
    }

    #[test]
    fn options_map_to_the_compiler() {
        assert_eq!(bounces(true), 100);
        assert_eq!(bounces(false), 0);
        assert!(!uses_hdr(LightingSetting::Ldr));
        assert!(uses_hdr(LightingSetting::Hdr) && uses_hdr(LightingSetting::Both));
        assert_eq!(rad_quality(LightingQuality::Draft), hp_bspc::rad::RadQuality::Draft);
        assert_eq!(rad_quality(LightingQuality::Final), hp_bspc::rad::RadQuality::Final);
        assert_eq!(coarsen_factor(10, 100), 1.0);
        assert_eq!(coarsen_factor(400, 100), 2.0);
        assert_eq!(coarsen_factor(400, 0), 1.0);
        assert_eq!(bake_threads(3), (3, 3));
        let (compile, light) = bake_threads(0);
        assert!(compile >= 1 && light >= compile);
    }

    #[test]
    fn local_light_edits_keep_the_global_bake_and_its_face_bindings() {
        let mut state = hp_app::testing::test_state();
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        let serial = preview.cache.resolve(&state, false, 0.0, true).unwrap();
        preview.has_bake = true;
        preview.shown_serial = Some(serial);
        preview.baseline_key = Some(preview.cache.geometry_key(&state));
        state
            .doc
            .create_point_entity("light", DVec3::new(0.0, 0.0, 128.0), &[("_light".into(), "255 255 255 400".into())])
            .unwrap();
        state.frame.time = 2.0;
        preview.schedule(&mut state, 2.0);
        state.frame.time = 4.0;
        preview.schedule(&mut state, 4.0);
        assert!(preview.has_bake && preview.running.is_none() && preview.upload.is_none());
        assert_eq!(preview.shown_serial, Some(serial));
        assert!(preview.can_revalidate(&state));
        assert!(state.runtime.lighting.stale);
    }

    #[test]
    fn modes() {
        assert_eq!(display_of(RenderModeSetting::Baked), LightmapDisplay::Baked);
        assert_eq!(display_of(RenderModeSetting::LightmapGrid), LightmapDisplay::Grid);
        assert_eq!(display_of(RenderModeSetting::Lit), LightmapDisplay::Off);
    }

    #[test]
    fn explicit_bake_is_global_and_automatic_regions_remain_opt_in() {
        let center = DVec3::splat(1000.0);
        let (scope, kind, follows) = initial_scope(center, 1024.0, false, false, Some(3072.0));
        assert_eq!((scope, kind, follows), (Scope::All, BakeKind::Full, false));
        let (scope, kind, follows) = initial_scope(center, 1024.0, false, true, Some(3072.0));
        assert_eq!(scope, Scope::Around { center, radius: 3072.0 });
        assert_eq!((kind, follows), (BakeKind::AroundCamera, true));
        let (scope, kind, follows) = initial_scope(center, 1024.0, true, false, Some(3072.0));
        assert_eq!(scope, Scope::Around { center, radius: 1024.0 });
        assert_eq!((kind, follows), (BakeKind::AroundCamera, false));
    }

    #[test]
    fn outdated_worker_results_wait_for_validation_and_are_discarded() {
        let mut state = hp_app::testing::test_state();
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        let serial = preview.cache.resolve(&state, false, 0.0, true).unwrap();
        preview.has_bake = true;
        preview.upload = Some(Upload {
            baked: Box::new(bake::Baked::default()),
            kind: BakeKind::Full,
            serial,
            scope: Scope::All,
            written: 0,
            lit: false,
            quick: false,
        });
        state.runtime.lighting.progress = Some(BakeProgress {
            kind: BakeKind::Full,
            phase: BakePhase::Finishing,
            fraction: 0.95,
            elapsed_s: 1.0,
            faces: 0,
        });
        state.doc.create_point_entity("light", DVec3::ZERO, &[]).unwrap();
        state.frame.time = 10.0;
        preview.upload(&mut state);
        assert!(preview.upload.is_some(), "validation waits until the edit settles");
        state.frame.time = 12.0;
        preview.upload(&mut state);
        assert!(preview.upload.is_none());
        assert!(state.runtime.lighting.progress.is_none());
        assert!(state.runtime.lighting.stale);
        assert_eq!(preview.restart, Some(BakeKind::Update));
        assert!(!preview.has_bake || preview.shown_serial != Some(serial));
    }

    #[test]
    fn brightness_changes_leave_the_baked_result_ready() {
        let mut state = hp_app::testing::test_state();
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        let serial = preview.cache.resolve(&state, false, 0.0, true).unwrap();
        preview.has_bake = true;
        preview.shown_serial = Some(serial);
        let mut options = state.doc.light_plus();
        options.brightness_scale = 2.0;
        state.doc.set_light_plus(&options).unwrap();
        state.frame.time = 10.0;
        preview.schedule(&mut state, 10.0);
        assert!(!state.runtime.lighting.stale);
        assert!(preview.running.is_none());
        state.frame.time = 12.0;
        preview.schedule(&mut state, 12.0);
        assert!(!state.runtime.lighting.stale);
        assert_eq!(preview.shown_serial, Some(serial));
        assert!(preview.running.is_none());
    }

    #[test]
    fn invalidated_faces_wait_for_the_same_physical_bake() {
        let mut state = hp_app::testing::test_state();
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        let serial = preview.cache.resolve(&state, false, 0.0, true).unwrap();
        preview.has_bake = true;
        preview.shown_serial = Some(serial);
        let mut lighting = State::default();
        lighting.note_lightmap_changes(vec![2, 2, 3]);
        lighting.note_lightmap_changes(vec![2, 3]);
        assert_eq!(lighting.lightmap_changes.len(), 2);
        assert!(preview.can_revalidate(&state));
        state.doc.create_point_entity("light", DVec3::ZERO, &[]).unwrap();
        assert!(!preview.can_revalidate(&state));
        let edited = preview.cache.resolve(&state, false, 0.0, true).unwrap();
        assert_ne!(serial, edited);
        assert!(!preview.can_revalidate(&state));
        assert_eq!(lighting.lightmap_changes.len(), 2);
        assert!(state.doc.undo());
        assert_eq!(preview.cache.resolve(&state, false, 0.0, true), Some(serial));
        assert!(preview.can_revalidate(&state));
        preview.shown_serial = None;
        assert!(!preview.can_revalidate(&state), "a partially replaced atlas cannot be restored");
    }

    #[test]
    fn undo_keeps_the_original_atlas_and_cancels_the_outdated_job() {
        let mut state = hp_app::testing::test_state();
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        let serial = preview.cache.resolve(&state, false, 0.0, true).unwrap();
        preview.has_bake = true;
        preview.shown_serial = Some(serial);
        state.doc.create_point_entity("light", DVec3::ZERO, &[]).unwrap();
        let edited = preview.cache.resolve(&state, false, 0.0, true).unwrap();
        preview.running =
            Some(Running { id: 1, kind: BakeKind::Update, serial: edited, scope: Scope::All, lit: false, quick: true });
        assert!(state.doc.undo());
        preview.cache.resolve(&state, false, 0.0, true).unwrap();
        preview.schedule(&mut state, 0.0);
        assert!(preview.running.is_none());
        assert!(preview.upload.is_none());
        assert_eq!(preview.shown_serial, Some(serial));
        assert!(preview.can_revalidate(&state));
    }
}
