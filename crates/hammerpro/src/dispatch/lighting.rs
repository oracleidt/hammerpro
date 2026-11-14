mod bake;
#[cfg(test)]
mod bench;
mod bsp;
mod cache;
pub(crate) mod child;
mod grid;
pub(crate) mod host;
pub(crate) mod memory;
mod preview;
mod style_maps;

use std::collections::{HashMap, HashSet};

use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::runtime::{BakeKind, BudgetEntry, BudgetSummary, IndirectStatus, PreviewStats};
use hp_app::settings::{LightingQuality, LightingSetting, RenderModeSetting};
use hp_app::state::RightTab;
use hp_app::{Action, AppState, Severity, ToolId, t, tf};
use hp_editor::luxels::{BudgetOptions, ColorCorrectionPlus, LightmapKind, MAX_MAP_LIGHTING};
use hp_editor::{FaceRef, SelectOp};
use hp_render::lightmap::LightmapDisplay;
use preview::Preview;

use super::{DockTab, Package};
use crate::app::HammerProApp;

pub(crate) struct Lighting;

const TAB: &str = "lighting";
const BUDGET_IDLE: f64 = 0.6;
const BUDGET_INTERVAL: f64 = 1.5;
const LAYER_SIZE: u32 = 2048;
const HOST_IDLE: f64 = 120.0;
const PREPARED_IDLE: f64 = 300.0;
const PREPARE_AFTER_LOAD: f64 = 2.0;
const PREPARE_IDLE: f64 = 15.0;
const PREPARE_INTERVAL: f64 = 60.0;
const VISIBLE_FIRST_SOLIDS: usize = 6000;
const VISIBLE_FIRST_RADIUS: f64 = 2048.0;
const VISIBLE_FIRST_REACH: f64 = 32768.0;
const VISIBLE_FIRST_SHARE: f64 = 0.6;
const SLOW_UPLOAD: std::time::Duration = std::time::Duration::from_millis(12);

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
    inactive_since: Option<f64>,
    prepare_revision: u64,
    prepare_changed: f64,
    prepare_started: Option<f64>,
    rebake_after_reset: bool,
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
        let class = hp_app::runtime::primitives::PrimitiveClass::Lighting;
        probes.vec(&preview.animations, class);
        for face in &preview.animations {
            face.resource_probes(probes);
        }
        if let Some(baseline) = &preview.baseline {
            probes.hold(baseline, class);
            for address in baseline.allocation_addresses() {
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

fn threads_for(setting: u32, cores: usize) -> usize {
    let cap = cores.saturating_sub(2).max(1);
    if setting > 0 {
        return (setting as usize).min(cap);
    }
    cores.saturating_sub((cores / 4).max(2)).max(1)
}

fn bake_threads(setting: u32) -> (usize, usize) {
    let cores = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let n = threads_for(setting, cores);
    (n, n)
}

pub(crate) fn preview_threads(state: &AppState) -> usize {
    bake_threads(state.settings.lighting.sanitized().threads).1
}

fn paused(state: &AppState) -> bool {
    let s = &state.settings.lighting;
    let r = &state.runtime.render;
    let window = hp_app::native_window::main_window_state().unwrap_or((r.minimized, r.focused));
    bake::away((s.pause_minimized, s.pause_unfocused), Some(window))
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
    if let Some(preview) = app.pkg.lighting.preview.as_mut() {
        let content = app.state.doc.content_serial();
        let hdr = hp_app::runtime::render::render_display().light_hdr;
        preview.rendered_scene = Some((content, hdr));
        if let Some(running) = preview.running.as_mut()
            && !running.lit
            && running.baseline.is_none()
            && running.content == content
            && running.hdr == hdr
            && let Some(gfx) = app.state.gfx.as_ref()
        {
            running.baseline = Some(gfx.renderer.snapshot_baked_lights());
        }
    }
    if let Some(baseline) = app.pkg.lighting.preview.as_ref().and_then(|preview| preview.baseline.as_ref())
        && let Some(gfx) = app.state.gfx.as_mut()
    {
        gfx.renderer.restore_baked_lights(baseline);
    }
    if let Some(gfx) = app.state.gfx.as_mut() {
        app.pkg.lighting.note_lightmap_changes(gfx.renderer.take_lightmap_changes());
    }
    revalidate_cached_faces(app);
    let bake_shown = app.pkg.lighting.preview.as_ref().is_some_and(|p| p.has_bake || !p.uploads.is_empty());
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
    Action::SoloSelectedLights,
    Action::MuteSelectedLights,
    Action::UnmuteAllLights,
    Action::MeasureLightCosts,
];

fn is_light(e: &hp_vmf::Entity) -> bool {
    e.classname.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("light"))
}

fn light_ids(state: &AppState) -> Vec<u32> {
    let mut ids: Vec<u32> =
        state.doc.scene().entities().iter().filter(|(_, n)| is_light(&n.entity)).map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids
}

fn selected_lights(state: &AppState) -> Vec<u32> {
    let scene = state.doc.scene();
    let mut ids: Vec<u32> = state
        .doc
        .selected_entity_ids()
        .into_iter()
        .filter(|id| scene.entity(*id).is_some_and(|n| is_light(&n.entity)))
        .collect();
    ids.sort_unstable();
    ids
}

fn set_excluded(state: &mut AppState, excluded: Vec<u32>) -> bool {
    let mut options = state.doc.light_plus();
    if options.excluded_entities == excluded {
        return false;
    }
    options.excluded_entities = excluded;
    state.doc.set_light_plus(&options).is_ok()
}

pub(crate) fn solo_lights(state: &mut AppState, ids: &[u32]) -> Result<usize, String> {
    if ids.is_empty() {
        return Err(t!("app.lighting.no_light_selected").to_owned());
    }
    let excluded: Vec<u32> = light_ids(state).into_iter().filter(|id| !ids.contains(id)).collect();
    let muted = excluded.len();
    set_excluded(state, excluded);
    Ok(muted)
}

pub(crate) fn mute_lights(state: &mut AppState, ids: &[u32], mute: bool) -> Result<usize, String> {
    if ids.is_empty() {
        return Err(t!("app.lighting.no_light_selected").to_owned());
    }
    let mut excluded = state.doc.light_plus().excluded_entities;
    if mute {
        excluded.extend_from_slice(ids);
    } else {
        excluded.retain(|id| !ids.contains(id));
    }
    excluded.sort_unstable();
    excluded.dedup();
    let count = excluded.len();
    set_excluded(state, excluded);
    Ok(count)
}

pub(crate) fn unmute_all(state: &mut AppState) -> usize {
    let before = state.doc.light_plus().excluded_entities.len();
    set_excluded(state, Vec::new());
    before
}

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
                Some(p) => {
                    app.pkg.lighting.preview = Some(p);
                    if let Some(gfx) = app.state.gfx.as_ref() {
                        gfx.renderer.prewarm_lightmap_pipelines();
                    }
                }
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
        p.visible_first = None;
        if let Some(radius) = big {
            app.state.notify(Severity::Info, tf!("app.lighting.big_map", radius.round()));
        }
        let first = (!around && scope == Scope::All && visible_first_wanted(&app.state))
            .then(|| visible_region(&app.state, radius))
            .flatten();
        match first {
            Some(region) => {
                log::info!("lighting preview: big map, the area around the camera is baked first");
                p.visible_first = Some(Scope::All);
                p.bake(&mut app.state, region, BakeKind::AroundCamera, false);
            }
            None => p.bake(&mut app.state, scope, kind, false),
        }
        app.pkg.lighting.preview = Some(p);
    }

    fn with_preview(app: &mut HammerProApp, ctx: &egui::Context, f: impl FnOnce(&mut Preview, &mut AppState)) {
        if Self::preview(app, ctx).is_none() {
            return;
        }
        if let Some(mut p) = app.pkg.lighting.preview.take() {
            f(&mut p, &mut app.state);
            app.pkg.lighting.preview = Some(p);
        }
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

    fn prepare_when_idle(app: &mut HammerProApp, ctx: &egui::Context, now: f64) {
        let revision = app.state.doc.revision();
        let pkg = &mut app.pkg.lighting;
        if pkg.prepare_revision != revision {
            pkg.prepare_revision = revision;
            pkg.prepare_changed = now;
        }
        let s = app.state.settings.lighting.sanitized();
        if !s.prepare || !s.auto_bake || pkg.failed || app.state.gfx.is_none() {
            return;
        }
        if app.state.runtime.lighting.paused || !app.state.runtime.render.focused || app.state.runtime.render.minimized
        {
            return;
        }
        if app.state.runtime.startup.resources.is_some() || app.state.doc.scene().solids().is_empty() {
            return;
        }
        let quiet = if pkg.prepare_started.is_none() { PREPARE_AFTER_LOAD } else { PREPARE_IDLE };
        if now - pkg.prepare_changed < quiet || pkg.prepare_started.is_some_and(|t| now - t < PREPARE_INTERVAL) {
            return;
        }
        if Self::preview(app, ctx).is_none() {
            return;
        }
        let Some(p) = app.pkg.lighting.preview.as_mut() else { return };
        if p.has_bake || p.running.is_some() || p.busy() || p.no_lights || p.prepared_current(&app.state) {
            return;
        }
        p.prepare(&mut app.state);
        app.pkg.lighting.prepare_started = Some(now);
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

fn visible_first_wanted(state: &AppState) -> bool {
    state.settings.lighting.visible_first && state.doc.scene().solids().len() >= VISIBLE_FIRST_SOLIDS
}

fn visible_region(state: &AppState, radius: f64) -> Option<Scope> {
    let cam = &state.cameras.cam3d;
    let opts =
        hp_editor::PickOptions { include_entities: false, max_distance: VISIBLE_FIRST_REACH, ..Default::default() };
    let ray = hp_geom::Ray::new(cam.pos, cam.forward());
    let seen = state.doc.pick_ray(&ray, &opts, &|_| None).map_or(0.0, |hit| hit.t);
    let radius = VISIBLE_FIRST_RADIUS.max(radius).max(seen * 1.25);
    let region = hp_geom::Aabb::new(cam.pos - DVec3::splat(radius), cam.pos + DVec3::splat(radius));
    let inside = state.doc.solids_in_box(&region).len();
    let total = state.doc.scene().solids().len();
    let useful = inside > 0 && (inside as f64) < total as f64 * VISIBLE_FIRST_SHARE;
    useful.then_some(Scope::Around { center: cam.pos, radius })
}

pub(crate) fn forget_for_new_device(app: &mut HammerProApp) {
    let had = app
        .pkg
        .lighting
        .preview
        .as_ref()
        .is_some_and(|p| p.has_bake || !p.uploads.is_empty() || p.running.as_ref().is_some_and(|r| !r.lit));
    app.pkg.lighting.rebake_after_reset |= had;
    forget(app);
    app.pkg.lighting.preview = None;
    app.pkg.lighting.cc_applied = None;
    app.pkg.lighting.cc_job = None;
}

fn forget(app: &mut HammerProApp) {
    if let Some(gfx) = app.state.gfx.as_mut() {
        gfx.renderer.clear_lightmaps();
    }
    app.pkg.lighting.grid.forget();
    app.pkg.lighting.lightmap_changes.clear();
    app.state.assets.set_baked_prop_lighting(None, None);
    if let Some(p) = app.pkg.lighting.preview.as_mut() {
        p.worker.release();
        p.running = None;
        p.uploads.clear();
        p.refine = None;
        p.visible_first = None;
        p.shown_serial = None;
        p.shown_faces = 0;
        p.shown_blocks = 0;
        p.has_bake = false;
        p.baseline = None;
        p.shown_geometry = None;
        p.layout = None;
        p.host_quick = None;
        p.models_for = None;
        p.last_scope = None;
        p.lights_revision = 0;
        p.cache = cache::BakeCache::default();
    }
    app.state.runtime.lighting.relight = hp_app::runtime::RelightStats::default();
    app.state.runtime.lighting.prepared = hp_app::runtime::PreparedStatus::default();
    app.state.runtime.lighting.costs = None;
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
                let has_bake =
                    app.pkg.lighting.preview.as_ref().is_some_and(|p| {
                        p.has_bake || !p.uploads.is_empty() || p.running.as_ref().is_some_and(|r| !r.lit)
                    });
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
            Action::SoloSelectedLights => {
                let ids = selected_lights(&app.state);
                match solo_lights(&mut app.state, &ids) {
                    Ok(muted) => app.state.notify(Severity::Info, tf!("app.lighting.solo", ids.len(), muted)),
                    Err(e) => app.state.notify(Severity::Warning, e),
                }
            }
            Action::MuteSelectedLights => {
                let ids = selected_lights(&app.state);
                match mute_lights(&mut app.state, &ids, true) {
                    Ok(muted) => app.state.notify(Severity::Info, tf!("app.lighting.muted", muted)),
                    Err(e) => app.state.notify(Severity::Warning, e),
                }
            }
            Action::UnmuteAllLights => {
                let count = unmute_all(&mut app.state);
                app.state.notify(Severity::Info, tf!("app.lighting.unmuted", count));
            }
            Action::MeasureLightCosts => {
                app.state.runtime.lighting.costs_wanted = true;
                Self::with_preview(app, ctx, |p, state| p.request_costs(state));
            }
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        if app.pkg.lighting.rebake_after_reset && app.state.gfx.is_some() {
            app.pkg.lighting.rebake_after_reset = false;
            if display_of(app.state.settings.view.render_mode) == LightmapDisplay::Baked {
                log::info!("lighting preview: baked again for the new graphics device");
                Self::start_bake(app, ctx, false, true);
            }
        }
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
        let paused = paused(&app.state);
        let rules = &app.state.settings.lighting;
        bake::set_pause_rules(rules.pause_minimized, rules.pause_unfocused);
        if bake::take_stopped_away()
            && let Some(p) = app.pkg.lighting.preview.as_mut()
            && p.running.as_ref().is_some_and(|r| !r.lit && !r.background)
        {
            p.cancelling = true;
        }
        if bake::take_models_halted()
            && let Some(p) = app.pkg.lighting.preview.as_mut()
        {
            p.models_halted();
        }
        if paused != app.state.runtime.lighting.paused {
            app.state.runtime.lighting.paused = paused;
            if paused && let Some(mut p) = app.pkg.lighting.preview.take() {
                p.pause(&mut app.state);
                app.pkg.lighting.preview = Some(p);
            }
        }
        if !active && let Some(mut p) = app.pkg.lighting.preview.take() {
            p.hide(&mut app.state);
            app.pkg.lighting.preview = Some(p);
        }
        if active || lit_wanted {
            Self::preview(app, ctx);
            if active && let Some(p) = app.pkg.lighting.preview.as_mut() {
                p.hold = false;
                p.foreground(&mut app.state);
            }
            app.pkg.lighting.inactive_since = None;
        } else {
            Self::prepare_when_idle(app, ctx, now);
            let pkg = &mut app.pkg.lighting;
            if let Some(p) = pkg.preview.as_mut() {
                p.hold = true;
                if p.running.is_some() || p.busy() || p.worker.info().pid.is_none() {
                    pkg.inactive_since = None;
                } else {
                    let since = *pkg.inactive_since.get_or_insert(now);
                    let limit = if p.holds_prepared() { PREPARED_IDLE } else { HOST_IDLE };
                    if now - since > limit {
                        log::info!("lighting preview: unused for {limit} s, the lighting process is closed");
                        p.worker.release();
                        p.needs_map = true;
                        pkg.inactive_since = None;
                    }
                }
            }
        }
        if app.state.runtime.lighting.costs_wanted
            && app.state.runtime.lighting.costs.is_none()
            && let Some(p) = app.pkg.lighting.preview.as_mut()
            && p.costs_job.is_none()
            && p.has_bake
            && !p.busy()
        {
            p.request_costs(&app.state);
        }
        if let Some(mut p) = app.pkg.lighting.preview.take() {
            if active && !paused {
                p.schedule(&mut app.state, now);
                p.light_models_again(&mut app.state);
            } else if lit_wanted && !paused {
                let shown = app.state.gfx.as_ref().is_some_and(|g| g.renderer.indirect_volume().is_some());
                p.schedule_lit(&mut app.state, shown);
            }
            let polled = std::time::Instant::now();
            p.poll(&mut app.state);
            if polled.elapsed() > SLOW_UPLOAD {
                log::info!(
                    "lighting preview: {} ms to take the lighting results in one frame",
                    polled.elapsed().as_millis()
                );
            }
            if !p.uploads.is_empty()
                && let Some(gfx) = app.state.gfx.as_mut()
            {
                app.pkg.lighting.grid.clear(&mut gfx.renderer);
            }
            let sent = std::time::Instant::now();
            p.upload(&mut app.state);
            if sent.elapsed() > SLOW_UPLOAD {
                log::info!("lighting preview: {} ms to send lightmaps in one frame", sent.elapsed().as_millis());
            }
            if active && p.uploads.is_empty() && !p.animations.is_empty() {
                let changed: Vec<_> =
                    p.animations.iter_mut().filter_map(|face| face.sample(now).then_some(face)).collect();
                if let Some(gfx) = app.state.gfx.as_mut() {
                    let writes: Vec<_> = changed.iter().map(|face| face.write()).collect();
                    if !writes.is_empty() {
                        gfx.renderer.write_animated_lightmaps(&writes);
                    }
                }
                if p.animations.iter().any(style_maps::AnimatedFace::animated)
                    && hp_app::runtime::render::animations_running()
                {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
            if active && p.drag_wait && !paused {
                let pause = u64::from(app.state.settings.lighting.sanitized().drag_pause_ms);
                ctx.request_repaint_after(std::time::Duration::from_millis(pause.max(16)));
            }
            let waiting = (p.busy() && !p.background_only())
                || ((active || lit_wanted)
                    && (p.cache.pending(lit_wanted && !active) || (p.restart.is_some() && options.incremental)));
            if waiting && !paused {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            let prepared = p.holds_prepared() && p.prepared_current(&app.state);
            app.state.runtime.lighting.prepared.ready = prepared;
            p.note_indirect(&mut app.state);
            app.pkg.lighting.preview = Some(p);
        }
        revalidate_cached_faces(app);
        Self::budget(app, now);
        Self::color_correction(app, ctx);
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        forget(app);
        let pkg = &mut app.pkg.lighting;
        pkg.prepare_started = None;
        pkg.prepare_revision = app.state.doc.revision();
        pkg.prepare_changed = app.state.frame.time;
        if let Some(gfx) = app.state.gfx.as_mut() {
            gfx.renderer.set_indirect_volume(None);
        }
        if let Some(p) = app.pkg.lighting.preview.as_mut() {
            p.indirect_serial = None;
            p.indirect_scene = None;
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
    (
        "app.lighting.no_light_selected",
        "Select one or more light entities first",
        "Sélectionnez d'abord une ou plusieurs entités de lumière",
    ),
    (
        "app.lighting.solo",
        "Lighting preview: {0} lights kept, {1} other lights muted (Unmute all lights brings them back)",
        "Aperçu de l'éclairage : {0} lumières gardées, {1} autres lumières coupées (Réactiver toutes les lumières les rétablit)",
    ),
    ("app.lighting.muted", "Lighting preview: {0} lights muted", "Aperçu de l'éclairage : {0} lumières coupées"),
    (
        "app.lighting.unmuted",
        "Lighting preview: {0} muted lights back on",
        "Aperçu de l'éclairage : {0} lumières coupées rallumées",
    ),
];

#[cfg(test)]
mod tests {
    use super::preview::{Running, Upload, dragged_lights, points_of};
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
        let (compile, light) = bake_threads(0);
        assert!(compile >= 1 && light >= compile);
    }

    #[test]
    fn baking_leaves_cores_free() {
        assert_eq!(threads_for(0, 12), 9);
        assert_eq!(threads_for(0, 16), 12);
        assert_eq!(threads_for(0, 8), 6);
        assert_eq!(threads_for(0, 4), 2);
        assert_eq!((threads_for(0, 2), threads_for(0, 1)), (1, 1));
        assert_eq!(threads_for(3, 12), 3);
        assert_eq!(threads_for(64, 12), 10, "never more than every core but two");
        assert_eq!(threads_for(5, 4), 2);
        assert_eq!(threads_for(8, 2), 1);
    }

    #[test]
    fn the_preview_pauses_when_the_window_is_away() {
        let mut state = hp_app::testing::test_state();
        state.runtime.render.focused = true;
        assert!(!paused(&state));
        state.runtime.render.minimized = true;
        assert!(paused(&state), "minimized windows pause by default");
        state.settings.lighting.pause_minimized = false;
        assert!(!paused(&state));
        state.runtime.render.minimized = false;
        state.runtime.render.focused = false;
        assert!(!paused(&state), "another active window does not pause by default");
        state.settings.lighting.pause_unfocused = true;
        assert!(paused(&state));
    }

    #[test]
    fn bakes_run_below_normal_and_the_preparation_at_idle() {
        use hp_app::settings::BakePriority;
        let mut state = hp_app::testing::test_state();
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        preview.prepare(&mut state);
        assert_eq!(preview.worker.priority(), BakePriority::Idle);
        preview.running = None;
        preview.bake(&mut state, Scope::All, BakeKind::Full, false);
        assert_eq!(preview.worker.priority(), BakePriority::BelowNormal);
        preview.running = None;
        state.settings.lighting.priority = BakePriority::Normal;
        preview.bake(&mut state, Scope::All, BakeKind::Full, false);
        assert_eq!(preview.worker.priority(), BakePriority::Normal);
    }

    #[test]
    fn hiding_the_preview_stops_the_bake_and_bakes_again_when_shown() {
        use hp_app::settings::BakePriority;
        let mut state = hp_app::testing::test_state();
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        preview.bake(&mut state, Scope::All, BakeKind::Full, false);
        preview.hide(&mut state);
        assert!(preview.cancelling && state.runtime.lighting.progress.is_none());
        let r = preview.running.take().unwrap();
        preview.done(&mut state, r, bake::Outcome::Cancelled);
        assert_eq!(preview.restart, Some(BakeKind::Full), "the bake starts again once the preview is shown");
        preview.hide(&mut state);
        assert!(!preview.cancelling, "nothing left to stop");
        preview.prepare(&mut state);
        preview.hide(&mut state);
        assert!(!preview.cancelling, "the background preparation is not the shown bake");
        assert_eq!(preview.worker.priority(), BakePriority::Idle);
        preview.foreground(&mut state);
        assert!(!running(&preview).background);
        assert_eq!(preview.worker.priority(), BakePriority::BelowNormal, "shown: the preparation is waited for");
    }

    #[test]
    fn the_props_are_lit_again_when_the_hidden_preview_comes_back() {
        let mut state = hp_app::testing::test_state();
        let mut preview = shown(&state);
        preview.models_for = Some((7, Scope::All, false, preview.shown_serial.unwrap()));
        preview.hide(&mut state);
        assert!(preview.models_for.is_none() && preview.models_again, "hidden: the lighting of the props stops");
        preview.light_models_again(&mut state);
        let job = running(&preview);
        assert!(job.relight && job.kind == BakeKind::Update, "shown again: a relight brings the props back");
        assert!(!preview.models_again);
        preview.models_halted();
        assert!(preview.models_again);
    }

    #[test]
    fn pausing_stops_the_bake_and_takes_it_up_again() {
        let mut state = hp_app::testing::test_state();
        let mut preview = shown(&state);
        state.doc.create_point_entity("light", DVec3::new(0.0, 0.0, 128.0), &[]).unwrap();
        preview.schedule(&mut state, 1.0);
        assert!(running(&preview).relight);
        preview.pause(&mut state);
        assert!(preview.cancelling);
        let r = preview.running.take().unwrap();
        preview.done(&mut state, r, bake::Outcome::Cancelled);
        assert_eq!(preview.restart, Some(BakeKind::Update));
        preview.schedule(&mut state, 2.0);
        assert!(running(&preview).relight, "the relight starts again once the window is back");
    }

    #[test]
    fn dragged_lights_wait_for_the_mouse_to_rest() {
        let mut state = hp_app::testing::test_state();
        let light = state
            .doc
            .create_point_entity("light", DVec3::new(0.0, 0.0, 128.0), &[("_light".into(), "255 255 255 400".into())])
            .unwrap();
        let mut preview = shown(&state);
        let at = |x: f64| hp_app::runtime::DragTransform {
            objs: vec![hp_editor::ObjRef::Entity(light)].into(),
            matrix: glam::DMat4::from_translation(DVec3::X * x),
        };
        state.runtime.selection.drag = Some(at(16.0));
        preview.schedule(&mut state, 1.0);
        assert!(preview.running.is_none() && preview.drag_wait, "moving: no relight yet");
        state.runtime.selection.drag = Some(at(32.0));
        preview.schedule(&mut state, 1.1);
        preview.schedule(&mut state, 1.2);
        assert!(preview.running.is_none(), "the mouse rested only 0.1 s");
        preview.schedule(&mut state, 1.23);
        assert!(running(&preview).relight, "the mouse rested 0.13 s");
        assert!(!preview.drag_wait);
        state.runtime.selection.drag = None;
        preview.schedule(&mut state, 1.3);
        assert!(preview.drag_seen.is_none() && !preview.drag_wait);
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

    fn shown(state: &AppState) -> Preview {
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        let serial = preview.cache.resolve(state, false, 0.0, true).unwrap();
        preview.has_bake = true;
        preview.shown_serial = Some(serial);
        preview.layout = Some(3);
        preview.last_scope = Some(Scope::All);
        preview.shown_geometry = Some(cache::scope_serial(preview.cache.geometry_key(state), &Scope::All));
        preview.host_quick = Some(false);
        preview
    }

    fn running(preview: &Preview) -> &Running {
        preview.running.as_ref().expect("a job")
    }

    #[test]
    fn light_edits_relight_at_once_without_copying_the_map() {
        let mut state = hp_app::testing::test_state();
        let mut preview = shown(&state);
        assert!(preview.can_revalidate(&state));
        let light = state
            .doc
            .create_point_entity("light", DVec3::new(0.0, 0.0, 128.0), &[("_light".into(), "255 255 255 400".into())])
            .unwrap();
        state.frame.time = 2.0;
        preview.schedule(&mut state, 2.0);
        let job = running(&preview);
        assert!(job.relight && !job.lit && !job.quick, "lights only, at the quality already shown");
        assert_eq!(job.kind, BakeKind::Update);
        let first = job.id;
        assert!(preview.can_revalidate(&state), "the shown faces stay bound to the same geometry");
        state.doc.set_entity_key(&[light], "_light", "255 0 0 900").unwrap();
        preview.schedule(&mut state, 2.05);
        assert_eq!(running(&preview).id, first, "a running relight is not restarted by the next drag step");
        assert!(state.runtime.lighting.stale);
    }

    #[test]
    fn geometry_edits_wait_for_the_debounce_and_send_the_map() {
        let mut state = hp_app::testing::test_state();
        let mut preview = shown(&state);
        let solid = *state.doc.scene().solids().keys().next().unwrap();
        state.doc.translate(&[hp_editor::ObjRef::Solid(solid)], DVec3::X * 32.0).unwrap();
        state.frame.time = 5.0;
        preview.schedule(&mut state, 5.0);
        assert!(preview.running.is_none(), "brush edits settle first");
        let now = 5.0 + preview::GEOMETRY_DELAY + 0.05;
        state.frame.time = now;
        preview.schedule(&mut state, now);
        let job = running(&preview);
        assert!(!job.relight && job.quick, "the new geometry is compiled and lit quickly first");
        assert!(!preview.can_revalidate(&state));
    }

    #[test]
    fn undo_back_to_the_shown_lighting_cancels_the_outdated_relight() {
        let mut state = hp_app::testing::test_state();
        let mut preview = shown(&state);
        let serial = preview.shown_serial;
        state.doc.create_point_entity("light", DVec3::ZERO, &[]).unwrap();
        preview.schedule(&mut state, 1.0);
        assert!(running(&preview).relight);
        assert!(state.doc.undo());
        preview.schedule(&mut state, 1.1);
        assert!(preview.running.is_none());
        assert_eq!(preview.shown_serial, serial);
        assert!(!state.runtime.lighting.stale);
    }

    #[test]
    fn brightness_changes_leave_the_baked_result_ready() {
        let mut state = hp_app::testing::test_state();
        let mut preview = shown(&state);
        let serial = preview.shown_serial;
        let mut options = state.doc.light_plus();
        options.brightness_scale = 2.0;
        state.doc.set_light_plus(&options).unwrap();
        preview.schedule(&mut state, 10.0);
        preview.schedule(&mut state, 12.0);
        assert!(!state.runtime.lighting.stale);
        assert_eq!(preview.shown_serial, serial);
        assert!(preview.running.is_none());
    }

    fn upload(preview: &Preview, layout: u64, patch: bool, geometry: u64) -> Upload {
        Upload {
            baked: Box::new(bake::Baked { layout, patch, ..Default::default() }),
            kind: BakeKind::Update,
            serial: preview.shown_serial.unwrap_or(0),
            scope: Scope::All,
            written: 0,
            quick: false,
            geometry,
            baseline: None,
            partial: false,
            submitted: 0.0,
            held: false,
            started: false,
            blocks_added: 0,
        }
    }

    #[test]
    fn results_for_the_shown_geometry_are_kept_while_lights_keep_moving() {
        let mut state = hp_app::testing::test_state();
        let mut preview = shown(&state);
        let geometry = preview.shown_geometry.unwrap();
        state.doc.create_point_entity("light", DVec3::ZERO, &[]).unwrap();
        preview.uploads.push_back(upload(&preview, 3, true, geometry));
        preview.upload(&mut state);
        assert!(preview.uploads.is_empty());
        assert!(preview.restart.is_none(), "a lagging relight still improves the picture");

        preview.uploads.push_back(upload(&preview, 9, true, geometry));
        preview.upload(&mut state);
        assert!(preview.uploads.is_empty());
        assert_eq!(preview.layout, None, "a patch for another atlas means the atlas was lost");
        assert_eq!(preview.restart, Some(BakeKind::Update));

        let mut preview = shown(&state);
        let solid = *state.doc.scene().solids().keys().next().unwrap();
        state.doc.translate(&[hp_editor::ObjRef::Solid(solid)], DVec3::Y * 16.0).unwrap();
        preview.uploads.push_back(upload(&preview, 3, false, geometry));
        preview.upload(&mut state);
        assert!(preview.uploads.is_empty());
        assert_eq!(preview.restart, Some(BakeKind::Update), "results for moved brushes are dropped");
        assert!(state.runtime.lighting.stale);
    }

    #[test]
    fn relights_send_lights_and_their_targets_only() {
        let mut state = hp_app::testing::test_state();
        let spot = state
            .doc
            .create_point_entity("light_spot", DVec3::new(0.0, 0.0, 64.0), &[("target".into(), "aim".into())])
            .unwrap();
        let aim = state
            .doc
            .create_point_entity("info_target", DVec3::new(64.0, 0.0, 0.0), &[("targetname".into(), "Aim".into())])
            .unwrap();
        let muted = state.doc.create_point_entity("light", DVec3::new(9000.0, 0.0, 0.0), &[]).unwrap();
        state.doc.create_point_entity("info_player_start", DVec3::ZERO, &[]).unwrap();
        let ids = |s: &AppState, scope: &Scope| points_of(s, scope).iter().map(|e| e.id).collect::<Vec<_>>();
        let all = ids(&state, &Scope::All);
        assert!(all.contains(&spot) && all.contains(&aim) && all.contains(&muted));
        assert!(mute_lights(&mut state, &[muted], true).is_ok());
        assert!(!ids(&state, &Scope::All).contains(&muted), "muted lights leave the preview");
        assert_eq!(unmute_all(&mut state), 1);
        let near = Scope::Around { center: DVec3::ZERO, radius: 512.0 };
        assert!(!ids(&state, &near).contains(&muted), "a region keeps its own lights");
        assert_eq!(solo_lights(&mut state, &[spot]).ok(), Some(light_ids(&state).len() - 1));
        assert_eq!(ids(&state, &Scope::All).iter().filter(|id| light_ids(&state).contains(id)).count(), 1);
        assert!(solo_lights(&mut state, &[]).is_err());
        assert!(state.doc.undo(), "solo is one undo step");
        assert!(state.doc.light_plus().excluded_entities.is_empty());
    }

    #[test]
    fn big_maps_bake_the_visible_area_first_then_the_whole_map() {
        let mut state = hp_app::testing::test_state();
        assert!(!visible_first_wanted(&state), "small maps bake whole at once");
        state.settings.lighting.visible_first = false;
        assert!(!visible_first_wanted(&state));
        let mut preview = shown(&state);
        preview.visible_first = Some(Scope::All);
        preview.last_scope = Some(Scope::Around { center: DVec3::ZERO, radius: 2048.0 });
        preview.shown_geometry =
            Some(cache::scope_serial(preview.cache.geometry_key(&state), preview.last_scope.as_ref().unwrap()));
        preview.refine = preview.last_scope.clone().map(|s| (s, 0));
        preview.schedule(&mut state, 1.0);
        let job = running(&preview);
        assert_eq!((job.kind, job.scope.clone()), (BakeKind::Full, Scope::All));
        assert!(!job.relight, "the whole map needs its own geometry");
        assert!(preview.refine.is_none(), "no refined pass for the temporary region");
    }

    #[test]
    fn background_preparation_waits_unseen_until_the_preview_is_turned_on() {
        let mut state = hp_app::testing::test_state();
        let mut preview = Preview::start(&egui::Context::default()).unwrap();
        preview.prepare(&mut state);
        let job = running(&preview);
        assert!(job.background && !job.relight && job.kind == BakeKind::Full);
        assert!(state.runtime.lighting.progress.is_none(), "no progress bar for the preparation");
        assert!(state.runtime.lighting.prepared.preparing);
        assert!(preview.background_only() && preview.busy());
        assert!(preview.prepared_current(&state), "a running preparation counts as current");
        let r = preview.running.take().unwrap();
        let mut up = upload(&preview, 1, false, r.geometry);
        up.serial = r.serial;
        up.held = true;
        preview.uploads.push_back(up);
        preview.hold = true;
        preview.upload(&mut state);
        assert_eq!(preview.uploads.len(), 1, "the prepared result waits while the preview is off");
        assert!(preview.holds_prepared() && !preview.busy() && preview.prepared_current(&state));
        let light = state.doc.create_point_entity("light", DVec3::new(0.0, 64.0, 64.0), &[]).unwrap();
        assert!(!preview.prepared_current(&state), "an edit makes the prepared result old");
        assert!(state.doc.undo() && state.doc.scene().entity(light).is_none());
        assert!(preview.prepared_current(&state));
        preview.foreground(&mut state);
        preview.hold = false;
        assert!(!state.runtime.lighting.prepared.preparing);
        preview.upload(&mut state);
        assert!(preview.uploads.is_empty(), "turning the preview on shows it");
    }

    #[test]
    fn a_light_edit_stops_the_refined_pass_and_refines_again_after_it() {
        let mut state = hp_app::testing::test_state();
        let mut preview = shown(&state);
        preview.refine = Some((Scope::All, preview.shown_serial.unwrap()));
        preview.schedule(&mut state, 1.0);
        assert_eq!(running(&preview).kind, BakeKind::Refine);
        state.doc.create_point_entity("light", DVec3::new(0.0, 0.0, 128.0), &[]).unwrap();
        preview.schedule(&mut state, 1.1);
        assert!(preview.cancelling, "the refined pass gives way to the edit");
        let r = preview.running.take().unwrap();
        preview.done(&mut state, r, bake::Outcome::Cancelled);
        assert_eq!(preview.restart, Some(BakeKind::Update));
        assert!(preview.refine.is_some(), "the refined pass runs again later");
        preview.schedule(&mut state, 1.2);
        let job = running(&preview);
        assert!(job.relight && job.kind == BakeKind::Update);
    }

    #[test]
    fn dragged_lights_relight_live_without_editing_the_map() {
        let mut state = hp_app::testing::test_state();
        state.settings.lighting.drag_pause_ms = 0;
        let light = state
            .doc
            .create_point_entity("light", DVec3::new(0.0, 0.0, 128.0), &[("_light".into(), "255 255 255 400".into())])
            .unwrap();
        let mut preview = shown(&state);
        let drag = |objs: Vec<hp_editor::ObjRef>, offset: DVec3| hp_app::runtime::DragTransform {
            objs: objs.into(),
            matrix: glam::DMat4::from_translation(offset),
        };
        state.runtime.selection.drag = Some(drag(vec![hp_editor::ObjRef::Entity(light)], DVec3::X * 64.0));
        let (key, moved) = dragged_lights(&state).expect("a dragged light");
        assert_eq!(moved[0].origin(), Some(DVec3::new(64.0, 0.0, 128.0)));
        let revision = state.doc.revision();
        preview.schedule(&mut state, 1.0);
        let job = running(&preview);
        assert!(job.relight && job.kind == BakeKind::Update && job.serial == key);
        assert_eq!(state.doc.revision(), revision, "the map is not edited while dragging");
        assert_eq!(preview.drag_key, Some(key));
        let solid = *state.doc.scene().solids().keys().next().unwrap();
        state.runtime.selection.drag = Some(drag(vec![hp_editor::ObjRef::Solid(solid)], DVec3::X * 64.0));
        assert!(dragged_lights(&state).is_none(), "brushes wait for the end of the drag");
        state.runtime.selection.drag = Some(drag(vec![hp_editor::ObjRef::Entity(light)], DVec3::ZERO));
        assert!(dragged_lights(&state).is_none(), "nothing moved yet");
        state.runtime.selection.drag = None;
        preview.schedule(&mut state, 1.1);
        assert_eq!(preview.drag_key, None);
    }

    #[test]
    fn the_visible_area_is_what_the_camera_looks_at() {
        let mut state = hp_app::testing::test_state();
        let solid = *state.doc.scene().solids().keys().next().unwrap();
        for i in 1..=20 {
            state
                .doc
                .duplicate(&[hp_editor::ObjRef::Solid(solid)], DVec3::new(30000.0 + 512.0 * f64::from(i), 0.0, 0.0))
                .unwrap();
        }
        let center = state.doc.scene().solid(solid).unwrap().bounds.center();
        state.cameras.cam3d.pos = center + DVec3::new(-256.0, 0.0, 64.0);
        state.cameras.cam3d.look_at(center);
        let Some(Scope::Around { center: c, radius }) = visible_region(&state, 512.0) else {
            panic!("a region around the camera")
        };
        assert_eq!(c, state.cameras.cam3d.pos);
        assert!(radius >= VISIBLE_FIRST_RADIUS);
        state.cameras.cam3d.pos = DVec3::new(0.0, 0.0, 20000.0);
        state.cameras.cam3d.look_at(DVec3::new(0.0, 20000.0, 20000.0));
        assert_eq!(visible_region(&state, 512.0), None, "nothing to see: the whole map is baked at once");
        state.cameras.cam3d.pos = DVec3::new(36000.0, 0.0, 2000.0);
        state.cameras.cam3d.look_at(DVec3::new(36000.0, 0.0, 0.0));
        let Some(Scope::Around { radius, .. }) = visible_region(&state, 512.0) else { panic!("far below the camera") };
        assert!(radius > VISIBLE_FIRST_RADIUS + 256.0, "the region reaches what the camera looks at: {radius}");
        state.cameras.cam3d.pos = DVec3::new(36000.0, 0.0, 30000.0);
        assert_eq!(visible_region(&state, 512.0), None, "a region holding the whole map is no shortcut");
    }
}
