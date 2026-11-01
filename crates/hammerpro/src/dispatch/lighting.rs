mod bake;
mod bsp;
pub(crate) mod child;
mod grid;

use std::collections::{HashMap, HashSet};

use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::runtime::{BakeKind, BakePhase, BakeProgress, BakeSummary, BudgetEntry, BudgetSummary, PreviewStats};
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
}

struct Upload {
    baked: Box<bake::Baked>,
    kind: BakeKind,
    serial: u64,
    scope: Scope,
    written: usize,
}

struct Preview {
    worker: bake::Worker,
    next_id: u64,
    running: Option<Running>,
    upload: Option<Upload>,
    shown_serial: Option<u64>,
    shown_faces: u32,
    shown_blocks: u32,
    last_scope: Option<Scope>,
    has_bake: bool,
    seen_serial: u64,
    changed_at: f64,
    lights_revision: u64,
    no_lights: bool,
    atlas_warned: bool,
    triangles: usize,
}

fn rad_quality(q: LightingQuality) -> hp_bspc::rad::RadQuality {
    match q {
        LightingQuality::Draft => hp_bspc::rad::RadQuality::Draft,
        LightingQuality::Normal => hp_bspc::rad::RadQuality::Normal,
        LightingQuality::Final => hp_bspc::rad::RadQuality::Final,
    }
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
    fn start(ctx: &egui::Context) -> Option<Self> {
        let ctx = ctx.clone();
        let worker = bake::Worker::start(Box::new(move || ctx.request_repaint())).ok()?;
        Some(Self {
            worker,
            next_id: 1,
            running: None,
            upload: None,
            shown_serial: None,
            shown_faces: 0,
            shown_blocks: 0,
            last_scope: None,
            has_bake: false,
            seen_serial: 0,
            changed_at: 0.0,
            lights_revision: 0,
            no_lights: false,
            atlas_warned: false,
            triangles: 0,
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

    fn bake(&mut self, state: &mut AppState, scope: Scope, kind: BakeKind) {
        let t = std::time::Instant::now();
        let export = hp_editor::compile_export::CompileExport::default();
        let snap = match &scope {
            Scope::All => state.doc.compile_snapshot(&export),
            Scope::Around { center, radius } => {
                let r = DVec3::splat(radius.max(64.0));
                state.doc.compile_snapshot_in(&export, &[hp_geom::Aabb::new(*center - r, *center + r)])
            }
        };
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
        let budget = u64::from(s.max_luxels_millions) * 1_000_000;
        let factor = coarsen_factor(map_luxels(state), budget);
        let (layer_size, max_layers) = state.gfx.as_ref().map_or((LAYER_SIZE, 16), |g| {
            let limits = g.device.limits();
            (LAYER_SIZE.min(limits.max_texture_dimension_2d).max(64), limits.max_texture_array_layers.clamp(1, 64))
        });
        let id = self.next_id;
        self.next_id += 1;
        let serial = state.doc.content_serial();
        self.worker.submit(bake::Job {
            id,
            map: snap.map,
            solids,
            fs: state.assets.fs().cloned(),
            hdr: uses_hdr(state.settings.compile.lighting),
            quality: rad_quality(s.quality),
            bounces: bounces(options.bounced),
            prop_shadows: state.settings.compile.static_prop_shadows,
            prop_lighting: state.settings.compile.static_prop_lighting,
            threads: bake_threads(s.threads),
            layer_size,
            max_layers,
            coarsen: factor,
        });
        log::info!("lighting preview: bake {id} ({kind:?}) started, map copied in {} ms", t.elapsed().as_millis());
        self.running = Some(Running { id, kind, serial, scope: scope.clone() });
        self.last_scope = Some(scope);
        state.runtime.lighting.stale = false;
        state.runtime.lighting.progress =
            Some(BakeProgress { kind, phase: BakePhase::Preparing, fraction: 0.0, elapsed_s: 0.0, faces: 0 });
    }

    fn stop(&mut self, state: &mut AppState) {
        if let Some(r) = self.running.take() {
            self.worker.stop();
            state.runtime.lighting.progress = None;
            state.runtime.lighting.last =
                Some(BakeSummary { kind: r.kind, cancelled: true, density: 1.0, ..Default::default() });
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
                            self.upload =
                                Some(Upload { baked, kind: r.kind, serial: r.serial, scope: r.scope, written: 0 });
                        }
                        bake::Outcome::Cancelled => {
                            state.runtime.lighting.progress = None;
                            state.runtime.lighting.last =
                                Some(BakeSummary { kind: r.kind, cancelled: true, density: 1.0, ..Default::default() });
                        }
                        bake::Outcome::Failed(e) => {
                            log::error!("lighting preview: {e}");
                            state.runtime.lighting.progress = None;
                            state.notify(Severity::Error, tf!("app.lighting.failed", e));
                        }
                    }
                }
            }
        }
    }

    fn upload(&mut self, state: &mut AppState) {
        let Some(mut up) = self.upload.take() else { return };
        let AppState { gfx, assets, runtime, .. } = state;
        let Some(gfx) = gfx.as_mut() else { return };
        let r = &mut gfx.renderer;
        let b = &up.baked;
        if up.written == 0 {
            let old: Vec<u32> = (0..self.shown_faces).collect();
            r.remove_lightmap_faces(&old);
            for id in 0..self.shown_blocks {
                r.remove_lightmap_block(id);
            }
            if b.layers > 0 && !r.ensure_lightmap_layers(b.layer_size, b.layers) && !self.atlas_warned {
                self.atlas_warned = true;
                log::warn!("lighting preview: the GPU cannot hold {} lightmap layers of {}", b.layers, b.layer_size);
            }
            let faces: Vec<(u32, LightmapFace)> = b
                .faces
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let face =
                        LightmapFace { object: f.solid, generation: f.generation, scale: f.scale, flags: f.flags };
                    (i as u32, face)
                })
                .collect();
            r.add_lightmap_faces(&faces);
            let materials: Vec<hp_render::MaterialId> =
                b.materials.iter().map(|m| assets.render_material_id(m)).collect();
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
            self.shown_faces = b.faces.len() as u32;
            self.shown_blocks = b.blocks.len() as u32;
            up.baked.blocks = Vec::new();
        }
        let b = &up.baked;
        let mut batch: Vec<(u32, LightmapRect, &[u32])> = Vec::new();
        let mut texels = 0usize;
        while up.written < b.faces.len() && texels < MAX_TEXELS_PER_FRAME {
            let i = up.written;
            up.written += 1;
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
        drop(batch);
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
        };
        self.has_bake = true;
        self.shown_serial = Some(up.serial);
        if up.kind != BakeKind::Update {
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
        let serial = state.doc.content_serial();
        if serial != self.seen_serial {
            self.seen_serial = serial;
            self.changed_at = now;
        }
        if !self.has_bake && self.running.is_none() {
            return;
        }
        let current =
            self.running.as_ref().map(|r| r.serial).or(self.upload.as_ref().map(|u| u.serial)).or(self.shown_serial);
        if current == Some(serial) {
            return;
        }
        let options = state.doc.light_plus();
        if !options.incremental {
            state.runtime.lighting.stale = self.has_bake && self.running.is_none();
            return;
        }
        let delay = f64::from(options.incremental_delay).max(MIN_UPDATE_DELAY);
        if now - self.changed_at < delay {
            return;
        }
        let scope = self.last_scope.clone().unwrap_or(Scope::All);
        let kind = self.running.as_ref().map_or(BakeKind::Update, |r| r.kind);
        self.bake(state, scope, kind);
    }
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

pub(crate) fn scene_synced(app: &mut HammerProApp) {
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
    editor_grid.note_changes(r.take_lightmap_changes());
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

    fn start_bake(app: &mut HammerProApp, ctx: &egui::Context, around: bool) {
        Self::set_mode(app, RenderModeSetting::Baked);
        let radius = f64::from(app.state.doc.light_plus().bake_dist);
        let center = app.state.cameras.cam3d.pos;
        if Self::preview(app, ctx).is_none() {
            return;
        }
        let Some(mut p) = app.pkg.lighting.preview.take() else { return };
        let (scope, kind) = if around {
            (Scope::Around { center, radius }, BakeKind::AroundCamera)
        } else {
            (Scope::All, BakeKind::Full)
        };
        p.bake(&mut app.state, scope, kind);
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

impl Package for Lighting {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::RenderBaked => {
                let was_baked = app.state.settings.view.render_mode == RenderModeSetting::Baked;
                Self::toggle_mode(app, RenderModeSetting::Baked);
                let has_bake = app.pkg.lighting.preview.as_ref().is_some_and(|p| p.has_bake || p.running.is_some());
                if !was_baked && app.state.settings.lighting.auto_bake && !has_bake {
                    Self::start_bake(app, ctx, false);
                }
            }
            Action::RenderLightmapGrid => Self::toggle_mode(app, RenderModeSetting::LightmapGrid),
            Action::BakeLighting => Self::start_bake(app, ctx, false),
            Action::BakeAroundCamera => Self::start_bake(app, ctx, true),
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
            app.pkg.lighting.grid.note_changes(gfx.renderer.take_lightmap_changes());
        }
        if active {
            Self::preview(app, ctx);
        }
        if let Some(mut p) = app.pkg.lighting.preview.take() {
            if active {
                p.schedule(&mut app.state, now);
            } else {
                p.stop(&mut app.state);
            }
            p.poll(&mut app.state);
            if p.upload.is_some()
                && let Some(gfx) = app.state.gfx.as_mut()
            {
                app.pkg.lighting.grid.clear(&mut gfx.renderer);
            }
            p.upload(&mut app.state);
            if p.running.is_some() || p.upload.is_some() {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            app.pkg.lighting.preview = Some(p);
        }
        Self::budget(app, now);
        Self::color_correction(app, ctx);
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        if let Some(gfx) = app.state.gfx.as_mut() {
            gfx.renderer.clear_lightmaps();
        }
        app.pkg.lighting.grid.forget();
        app.state.assets.set_baked_prop_lighting(None, None);
        if let Some(p) = app.pkg.lighting.preview.as_mut() {
            p.worker.stop();
            p.running = None;
            p.upload = None;
            p.shown_serial = None;
            p.shown_faces = 0;
            p.shown_blocks = 0;
            p.has_bake = false;
            p.last_scope = None;
            p.lights_revision = 0;
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
    fn modes() {
        assert_eq!(display_of(RenderModeSetting::Baked), LightmapDisplay::Baked);
        assert_eq!(display_of(RenderModeSetting::LightmapGrid), LightmapDisplay::Grid);
        assert_eq!(display_of(RenderModeSetting::Lit), LightmapDisplay::Off);
    }
}
