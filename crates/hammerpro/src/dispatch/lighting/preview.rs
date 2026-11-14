use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use glam::DVec3;
use hp_app::runtime::{BakeKind, BakePhase, BakeProgress, BakeSummary, PreviewStats, RelightStats};
use hp_app::{AppState, Severity, tf};
use hp_render::lightmap::{LightmapBatch, LightmapFace, LightmapRect};

use super::{Scope, bake, bsp, cache, child, style_maps};

pub(super) const MIN_UPDATE_DELAY: f64 = 0.0;
pub(super) const GEOMETRY_DELAY: f64 = 0.3;
pub(super) const TEXELS_PER_MB: usize = 1 << 18;
const BYTES_PER_TEXEL: usize = 4;

pub(super) fn luxel_budget(millions: u32, tier: hp_app::runtime::MemoryTier, layers: u32, layer_size: u32) -> u64 {
    let user = u64::from(millions) * 1_000_000;
    match tier.luxel_capacity(layers, layer_size) {
        Some(cap) if user == 0 => cap,
        Some(cap) => user.min(cap),
        None => user,
    }
}

pub(super) fn upload_budget(state: &AppState) -> usize {
    state.settings.lighting.sanitized().upload_mb_per_frame as usize * TEXELS_PER_MB
}

pub(super) struct Running {
    pub id: u64,
    pub kind: BakeKind,
    pub serial: u64,
    pub scope: Scope,
    pub lit: bool,
    pub quick: bool,
    pub relight: bool,
    pub geometry: u64,
    pub baseline: Option<Arc<hp_render::BakedLightBaseline>>,
    pub content: u64,
    pub hdr: bool,
    pub submitted: f64,
    pub background: bool,
}

#[derive(Clone, Copy)]
enum Origin<'a> {
    User,
    Background,
    Drag(u64, &'a [hp_vmf::Entity]),
}

pub(super) struct Upload {
    pub baked: Box<bake::Baked>,
    pub kind: BakeKind,
    pub serial: u64,
    pub scope: Scope,
    pub written: usize,
    pub quick: bool,
    pub geometry: u64,
    pub baseline: Option<Arc<hp_render::BakedLightBaseline>>,
    pub partial: bool,
    pub submitted: f64,
    pub held: bool,
    pub started: bool,
    pub blocks_added: usize,
}

pub(super) struct Preview {
    pub animations: Vec<style_maps::AnimatedFace>,
    pub baseline: Option<Arc<hp_render::BakedLightBaseline>>,
    pub rendered_scene: Option<(u64, bool)>,
    pub worker: bake::Worker,
    pub next_id: u64,
    pub running: Option<Running>,
    pub uploads: VecDeque<Upload>,
    pub refine: Option<(Scope, u64)>,
    pub cancelling: bool,
    pub restart: Option<BakeKind>,
    pub needs_map: bool,
    pub auto_scoped: bool,
    pub cam_last: DVec3,
    pub cam_moved_at: f64,
    pub shown_serial: Option<u64>,
    pub shown_geometry: Option<u64>,
    pub layout: Option<u64>,
    pub shown_faces: u32,
    pub shown_blocks: u32,
    pub last_scope: Option<Scope>,
    pub has_bake: bool,
    pub cache: cache::BakeCache,
    pub lights_revision: u64,
    pub no_lights: bool,
    pub atlas_warned: bool,
    pub triangles: usize,
    pub host_quick: Option<bool>,
    pub models_for: Option<(u64, Scope, bool, u64)>,
    pub indirect_serial: Option<u64>,
    pub indirect_scene: Option<(u64, bool)>,
    pub indirect_failed: Option<u64>,
    pub costs_job: Option<u64>,
    pub visible_first: Option<Scope>,
    pub luxels: Option<(u64, u64, f64)>,
    pub hold: bool,
    pub drag_key: Option<u64>,
    pub drag_seen: Option<(u64, f64)>,
    pub drag_wait: bool,
    pub models_again: bool,
}

pub(super) fn region_of(scope: &Scope) -> Option<[[f64; 3]; 2]> {
    match scope {
        Scope::All => None,
        Scope::Around { center, radius } => {
            let r = radius.max(64.0);
            Some([(*center - DVec3::splat(r)).to_array(), (*center + DVec3::splat(r)).to_array()])
        }
    }
}

fn region_box(scope: &Scope) -> Option<hp_geom::Aabb> {
    region_of(scope).map(|[lo, hi]| hp_geom::Aabb::new(DVec3::from_array(lo), DVec3::from_array(hi)))
}

pub(super) fn points_of(state: &AppState, scope: &Scope) -> Vec<hp_vmf::Entity> {
    let doc = &state.doc;
    let options = doc.light_plus();
    let scene = doc.scene();
    let region = region_box(scope);
    let usable = |id: u32, e: &hp_vmf::Entity| options.uses_entity(id) && !e.hidden && doc.is_entity_visible(id);
    let inside = |e: &hp_vmf::Entity| match &region {
        None => true,
        Some(b) => {
            e.classname.eq_ignore_ascii_case("light_environment") || e.origin().is_none_or(|o| b.contains_point(o))
        }
    };
    let mut lights: Vec<hp_vmf::Entity> = Vec::new();
    let mut targets: HashSet<String> = HashSet::new();
    for (id, node) in scene.entities().iter() {
        let e = &node.entity;
        if !bake::lighting_entity(e) || !usable(*id, e) || !inside(e) {
            continue;
        }
        if let Some(t) = e.get("target").map(|t| t.trim().to_ascii_lowercase()).filter(|t| !t.is_empty()) {
            targets.insert(t);
        }
        let mut copy = e.clone();
        if let Some(b) = &region
            && copy.classname.eq_ignore_ascii_case("light_environment")
            && copy.origin().is_none_or(|o| !b.contains_point(o))
        {
            let c = b.center();
            copy.set("origin", format!("{} {} {}", c.x, c.y, b.max.z - 8.0));
        }
        lights.push(copy);
    }
    if !targets.is_empty() {
        for (id, node) in scene.entities().iter() {
            let e = &node.entity;
            if bake::lighting_entity(e) || !usable(*id, e) || !inside(e) {
                continue;
            }
            if e.get("targetname").is_some_and(|n| targets.contains(&n.trim().to_ascii_lowercase())) {
                lights.push(e.clone());
            }
        }
    }
    lights.sort_unstable_by_key(|e| e.id);
    lights
}

pub(super) fn dragged_lights(state: &AppState) -> Option<(u64, Vec<hp_vmf::Entity>)> {
    let drag = state.runtime.selection.drag.as_ref()?;
    let m = drag.matrix;
    if m == glam::DMat4::IDENTITY || !m.is_finite() {
        return None;
    }
    let moves_only = m.x_axis == glam::DVec4::X && m.y_axis == glam::DVec4::Y && m.z_axis == glam::DVec4::Z;
    let scene = state.doc.scene();
    let mut moved = Vec::with_capacity(drag.objs.len());
    for obj in drag.objs.iter() {
        let hp_editor::ObjRef::Entity(id) = obj else { return None };
        let e = &scene.entity(*id)?.entity;
        let aimed =
            !e.classname.eq_ignore_ascii_case("light") && !e.classname.eq_ignore_ascii_case("light_environment");
        if !bake::lighting_entity(e) || (aimed && !moves_only) {
            return None;
        }
        let p = m.transform_point3(e.origin()?);
        let mut copy = e.clone();
        copy.set("origin", format!("{} {} {}", p.x, p.y, p.z));
        moved.push(copy);
    }
    if moved.is_empty() {
        return None;
    }
    let mut hash = std::hash::DefaultHasher::new();
    std::hash::Hash::hash(&(state.doc.content_serial(), m.to_cols_array().map(f64::to_bits)), &mut hash);
    Some((std::hash::Hasher::finish(&hash), moved))
}

#[cfg(test)]
pub(super) fn show_geometry(
    r: &mut hp_render::Renderer,
    assets: &mut dyn hp_app::state::AssetsApi,
    b: &bake::Baked,
    shown: (u32, u32),
) -> bool {
    let fits = begin_geometry(r, b, shown);
    add_blocks(r, assets, b, 0, usize::MAX);
    fits
}

pub(super) fn block_bytes(block: &bsp::Block) -> usize {
    block.batches.iter().map(|(_, v, i)| v.len() * size_of::<hp_render::lightmap::LightmapVertex>() + i.len() * 4).sum()
}

pub(super) fn add_blocks(
    r: &mut hp_render::Renderer,
    assets: &mut dyn hp_app::state::AssetsApi,
    b: &bake::Baked,
    from: usize,
    byte_budget: usize,
) -> (usize, usize) {
    let materials: Vec<hp_render::MaterialId> = b.materials.iter().map(|m| assets.render_material_id(m)).collect();
    let mut next = from;
    let mut bytes = 0usize;
    while let Some(block) = b.blocks.get(next) {
        if next > from && bytes >= byte_budget {
            break;
        }
        let batches: Vec<LightmapBatch<'_>> = block
            .batches
            .iter()
            .filter_map(|(m, v, i)| {
                Some(LightmapBatch { material: *materials.get(*m as usize)?, vertices: v, indices: i })
            })
            .collect();
        r.add_lightmap_block(next as u32, &batches);
        bytes += block_bytes(block);
        next += 1;
    }
    (next, bytes)
}

fn begin_geometry(r: &mut hp_render::Renderer, b: &bake::Baked, shown: (u32, u32)) -> bool {
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
        .map(|f| {
            let face = LightmapFace { object: f.solid, generation: f.generation, scale: f.scale, flags: f.flags };
            (f.id, face)
        })
        .collect();
    r.add_lightmap_faces(&faces);
    fits
}

#[cfg(test)]
pub(super) fn write_texels(r: &mut hp_render::Renderer, b: &bake::Baked, from: usize, budget: usize) -> usize {
    write_texels_counted(r, b, from, budget).0
}

pub(super) fn write_texels_counted(
    r: &mut hp_render::Renderer,
    b: &bake::Baked,
    from: usize,
    budget: usize,
) -> (usize, usize) {
    let mut written = from;
    let mut batch: Vec<(u32, LightmapRect, &[u32])> = Vec::new();
    let mut texels = 0usize;
    while written < b.faces.len() && texels < budget {
        let f = &b.faces[written];
        written += 1;
        let Some(rect) = f.rect else { continue };
        if f.texels == u32::MAX {
            continue;
        }
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
        batch.push((f.id, rect, data));
        texels += n;
    }
    r.write_lightmaps(&batch);
    (written, texels)
}

impl Preview {
    pub(super) fn start(ctx: &egui::Context) -> Option<Self> {
        let ctx = ctx.clone();
        let worker = bake::Worker::start(Box::new(move || ctx.request_repaint())).ok()?;
        Some(Self {
            animations: Vec::new(),
            baseline: None,
            rendered_scene: None,
            worker,
            next_id: 1,
            running: None,
            uploads: VecDeque::new(),
            refine: None,
            cancelling: false,
            restart: None,
            needs_map: false,
            auto_scoped: false,
            cam_last: DVec3::ZERO,
            cam_moved_at: 0.0,
            shown_serial: None,
            shown_geometry: None,
            layout: None,
            shown_faces: 0,
            shown_blocks: 0,
            last_scope: None,
            has_bake: false,
            cache: cache::BakeCache::default(),
            lights_revision: 0,
            no_lights: false,
            atlas_warned: false,
            triangles: 0,
            host_quick: None,
            models_for: None,
            indirect_serial: None,
            indirect_scene: None,
            indirect_failed: None,
            costs_job: None,
            visible_first: None,
            luxels: None,
            hold: false,
            drag_key: None,
            drag_seen: None,
            drag_wait: false,
            models_again: false,
        })
    }

    pub(super) fn can_revalidate(&self, state: &AppState) -> bool {
        self.has_bake
            && self.shown_serial.is_some()
            && (self.cache.current(state, false, self.shown_serial)
                || (self.shown_geometry.is_some()
                    && self.shown_geometry
                        == self
                            .cache
                            .current_geometry_key(state)
                            .map(|g| cache::scope_serial(g, self.last_scope.as_ref().unwrap_or(&Scope::All)))))
    }

    pub(super) fn sync_lights(&mut self, state: &AppState) {
        let revision = state.doc.revision();
        if revision == self.lights_revision {
            return;
        }
        self.lights_revision = revision;
        let set = hp_light::lights::collect(state.doc.scene().entities().values().map(|n| &n.entity), 1.0);
        self.no_lights = set.lights.is_empty() && set.sky.sun.is_none() && set.sky.ambient.max_element() <= 0.0;
    }

    pub(super) fn busy(&self) -> bool {
        self.running.is_some() || self.uploads.iter().any(|u| !u.held)
    }

    pub(super) fn background_only(&self) -> bool {
        self.running.as_ref().is_none_or(|r| r.background) && self.uploads.iter().all(|u| u.held)
    }

    pub(super) fn holds_prepared(&self) -> bool {
        self.uploads.iter().any(|u| u.held)
    }

    pub(super) fn prepared_current(&mut self, state: &AppState) -> bool {
        let Some(serial) = self.cache.resolve(state, false, 0.0, true) else { return false };
        self.uploads.iter().any(|u| u.held && u.serial == serial)
            || self.running.as_ref().is_some_and(|r| r.background && r.serial == serial)
    }

    pub(super) fn prepare(&mut self, state: &mut AppState) {
        self.submit(state, Scope::All, BakeKind::Full, false, false, Origin::Background);
    }

    pub(super) fn foreground(&mut self, state: &mut AppState) {
        if let Some(r) = self.running.as_mut().filter(|r| r.background) {
            r.background = false;
            self.worker.set_priority(state.settings.lighting.priority);
            state.runtime.lighting.progress = Some(BakeProgress {
                kind: r.kind,
                phase: BakePhase::Preparing,
                fraction: 0.0,
                elapsed_s: 0.0,
                faces: 0,
            });
        }
        for up in &mut self.uploads {
            up.held = false;
        }
        state.runtime.lighting.prepared.preparing = false;
    }

    pub(super) fn hide(&mut self, state: &mut AppState) {
        if self.models_for.as_ref().is_some_and(|m| !m.2) {
            self.models_for = None;
            self.models_again = true;
            self.worker.stop_models();
        }
        if self.cancelling || !self.running.as_ref().is_some_and(|r| !r.background && !r.lit) {
            return;
        }
        self.cancelling = true;
        self.worker.stop();
        state.runtime.lighting.progress = None;
        log::info!("lighting preview: hidden while baking, the bake stops and starts again when it is shown");
    }

    pub(super) fn models_halted(&mut self) {
        if self.models_for.as_ref().is_some_and(|m| !m.2) {
            self.models_for = None;
        }
        self.models_again = true;
    }

    pub(super) fn light_models_again(&mut self, state: &mut AppState) {
        if !self.models_again || self.running.is_some() || !self.uploads.is_empty() || !self.has_bake {
            return;
        }
        self.models_again = false;
        if self.layout.is_none() || self.needs_map {
            return;
        }
        let scope = self.last_scope.clone().unwrap_or(Scope::All);
        self.bake(state, scope, BakeKind::Update, false);
    }

    pub(super) fn pause(&mut self, state: &mut AppState) {
        let Some(r) = self.running.as_ref() else { return };
        if !r.lit && !r.background {
            self.cancelling = true;
        }
        self.worker.stop();
        state.runtime.lighting.progress = None;
        log::info!("lighting preview: paused, the running bake stops and starts again later");
    }

    fn geometry_of(&mut self, state: &AppState, scope: &Scope) -> u64 {
        cache::scope_serial(self.cache.geometry_key(state), scope)
    }

    fn light_hdr(state: &AppState, lit: bool) -> bool {
        if lit {
            state.settings.render.preview_hdr(state.settings.compile.lighting)
        } else {
            super::uses_hdr(state.settings.compile.lighting)
        }
    }

    pub(super) fn bake(&mut self, state: &mut AppState, scope: Scope, kind: BakeKind, lit: bool) {
        let geometry = self.geometry_of(state, &scope);
        let relight = !lit
            && !self.needs_map
            && kind != BakeKind::AroundCamera
            && self.layout.is_some()
            && self.shown_geometry == Some(geometry)
            && self.last_scope.as_ref() == Some(&scope);
        let lit_relight = lit && !self.needs_map && self.indirect_scene.is_some_and(|(g, _)| g == geometry);
        self.submit(state, scope, kind, lit, relight || lit_relight, Origin::User);
    }

    fn schedule_drag(&mut self, state: &mut AppState, key: u64, moved: &[hp_vmf::Entity], now: f64) {
        if self.drag_seen.is_none_or(|(k, _)| k != key) {
            self.drag_seen = Some((key, now));
        }
        let pause = f64::from(state.settings.lighting.sanitized().drag_pause_ms) / 1000.0;
        let settled = self.drag_seen.is_some_and(|(_, t)| now - t >= pause);
        self.drag_wait = !settled && self.drag_key != Some(key);
        if let Some(r) = self.running.as_ref() {
            if r.kind == BakeKind::Refine && !r.lit && !self.cancelling {
                self.cancelling = true;
                self.worker.stop();
            }
            return;
        }
        if self.drag_key == Some(key) || !self.uploads.is_empty() || !settled {
            return;
        }
        if self.needs_map || self.layout.is_none() || !self.has_bake {
            return;
        }
        let scope = self.last_scope.clone().unwrap_or(Scope::All);
        let geometry = self.geometry_of(state, &scope);
        if self.shown_geometry != Some(geometry) {
            return;
        }
        self.drag_key = Some(key);
        self.submit(state, scope, BakeKind::Update, false, true, Origin::Drag(key, moved));
    }

    fn submit(
        &mut self,
        state: &mut AppState,
        scope: Scope,
        kind: BakeKind,
        lit: bool,
        relight: bool,
        origin: Origin<'_>,
    ) {
        let background = matches!(origin, Origin::Background);
        let priority = if background { hp_app::settings::BakePriority::Idle } else { state.settings.lighting.priority };
        let live = match origin {
            Origin::Drag(key, moved) => Some((key, moved)),
            _ => None,
        };
        let resolved = self.cache.resolve(state, lit, 0.0, true).expect("forced lighting cache key");
        let serial = live.map_or(resolved, |(key, _)| key);
        let geometry = self.geometry_of(state, &scope);
        let hdr = Self::light_hdr(state, lit);
        let content = state.doc.content_serial();
        let baseline = (!lit && self.rendered_scene == Some((content, hdr)))
            .then(|| state.gfx.as_ref().map(|gfx| gfx.renderer.snapshot_baked_lights()))
            .flatten();
        let first = !relight && kind != BakeKind::Refine;
        let quick = !lit && if relight && kind != BakeKind::Refine { self.host_quick.unwrap_or(true) } else { first };
        if first && !lit {
            self.refine = None;
        }
        if !background && !lit {
            self.uploads.retain(|u| !u.held);
        }
        self.cancelling = false;
        self.restart = None;
        self.needs_map = false;
        self.models_for = None;
        let t = std::time::Instant::now();
        let s = state.settings.lighting.sanitized();
        let options = state.doc.light_plus();
        let (layer_size, max_layers) = state.gfx.as_ref().map_or((super::LAYER_SIZE, 16), |g| {
            let limits = g.device.limits();
            let device = limits.max_texture_array_layers.clamp(1, 64);
            let cap = state.runtime.gpu.lightmap_layers;
            (
                super::LAYER_SIZE.min(limits.max_texture_dimension_2d).max(64),
                if cap > 0 { device.min(cap) } else { device },
            )
        });
        let budget = luxel_budget(s.max_luxels_millions, state.runtime.gpu.tier, max_layers, layer_size);
        let factor = match self.luxels {
            Some((g, luxels, _)) if g == geometry => super::coarsen_factor(luxels, budget),
            _ => {
                let luxels = super::map_luxels(state);
                let factor = super::coarsen_factor(luxels, budget);
                self.luxels = Some((geometry, luxels, factor));
                factor
            }
        };
        let export = hp_editor::compile_export::CompileExport::default();
        let map = (!relight).then(|| {
            let mut map = match region_box(&scope) {
                None => state.doc.compile_snapshot(&export).map,
                Some(b) => state.doc.compile_snapshot_in(&export, &[b]).map,
            };
            map.entities.retain(|entity| options.uses_entity(entity.id));
            map
        });
        let points = relight.then(|| {
            let mut points = points_of(state, &scope);
            if let Some((_, moved)) = live {
                for point in &mut points {
                    if let Some(m) = moved.iter().find(|m| m.id == point.id) {
                        *point = m.clone();
                    }
                }
            }
            points
        });
        let solids: HashMap<u32, bsp::SolidInfo> = if relight {
            HashMap::new()
        } else {
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
        let id = self.next_id;
        self.next_id += 1;
        let tuning = if quick { child::Tuning::of(&options).quick() } else { child::Tuning::of(&options) };
        let cam = state.cameras.cam3d.pos;
        let stream = !relight && !lit && !background && !self.has_bake;
        self.worker.submit(bake::Job {
            id,
            geometry_key: geometry,
            map,
            points,
            solids,
            fs: state.assets.fs().cloned(),
            hdr,
            quality: super::rad_quality(s.quality),
            bounces: if options.bounced {
                options.bounce_count.min(super::bounces(true))
            } else {
                super::bounces(false)
            },
            prop_shadows: state.settings.compile.static_prop_shadows,
            prop_lighting: state.settings.compile.static_prop_lighting,
            threads: super::bake_threads(s.threads),
            layer_size,
            max_layers,
            coarsen: factor,
            lit,
            tuning,
            exe: bake::child_exe(),
            focus: Some(cam.as_vec3().to_array()),
            stream,
            shown: if lit { None } else { self.layout },
            models: !background,
            exact: relight && (!options.keep_bake || kind == BakeKind::Full),
            quiet: background,
            priority,
        });
        log::info!(
            "lighting preview: bake {id} ({kind:?}{}{}{}) started, prepared in {} ms",
            if relight { ", lights only" } else { "" },
            if lit { ", for the Lit mode" } else { "" },
            if background { ", in the background" } else { "" },
            t.elapsed().as_millis()
        );
        self.running = Some(Running {
            id,
            kind,
            serial,
            scope: scope.clone(),
            lit,
            quick,
            relight,
            geometry,
            baseline,
            content,
            hdr,
            submitted: state.frame.time,
            background,
        });
        if !lit {
            self.last_scope = Some(scope);
        }
        state.runtime.lighting.prepared.streamed_faces = 0;
        if background {
            state.runtime.lighting.prepared.preparing = true;
            return;
        }
        state.runtime.lighting.stale = false;
        let phase = if relight { BakePhase::Direct } else { BakePhase::Preparing };
        state.runtime.lighting.progress = Some(BakeProgress { kind, phase, fraction: 0.0, elapsed_s: 0.0, faces: 0 });
    }

    pub(super) fn stop(&mut self, state: &mut AppState) {
        self.refine = None;
        self.cancelling = false;
        self.restart = None;
        self.visible_first = None;
        if let Some(r) = self.running.take() {
            self.worker.stop();
            state.runtime.lighting.progress = None;
            if !r.lit {
                state.runtime.lighting.last =
                    Some(BakeSummary { kind: r.kind, cancelled: true, density: 1.0, ..Default::default() });
            }
        }
    }

    pub(super) fn request_costs(&mut self, state: &AppState) {
        let id = self.next_id;
        self.next_id += 1;
        self.costs_job = Some(id);
        let options = state.doc.light_plus();
        let quick = self.host_quick.unwrap_or(true);
        let tuning = if quick { child::Tuning::of(&options).quick() } else { child::Tuning::of(&options) };
        let s = state.settings.lighting.sanitized();
        self.worker.costs(
            id,
            super::host::LightOptions {
                hdr: Self::light_hdr(state, false),
                quality: child::quality_arg(super::rad_quality(s.quality)).to_owned(),
                bounces: if options.bounced { options.bounce_count.min(100) } else { 0 },
                prop_shadows: state.settings.compile.static_prop_shadows,
                prop_lighting: state.settings.compile.static_prop_lighting,
                tuning: tuning.arg(),
                threads: super::bake_threads(s.threads).1,
            },
        );
    }

    pub(super) fn poll(&mut self, state: &mut AppState) {
        for msg in self.worker.poll() {
            match msg {
                bake::Msg::Progress { job, phase, fraction, elapsed, faces } => {
                    let Some(r) = self.running.as_ref().filter(|r| r.id == job && !r.background) else { continue };
                    state.runtime.lighting.progress =
                        Some(BakeProgress { kind: r.kind, phase, fraction, elapsed_s: elapsed, faces });
                }
                bake::Msg::Partial { job, baked } => {
                    let Some(r) = self.running.as_ref().filter(|r| r.id == job) else {
                        if baked.patch
                            && Some(baked.layout) == self.layout
                            && self.models_for.as_ref().is_some_and(|m| m.0 == job)
                        {
                            self.uploads.push_back(Upload {
                                baked,
                                kind: BakeKind::Update,
                                serial: 0,
                                scope: self.last_scope.clone().unwrap_or(Scope::All),
                                written: 0,
                                started: false,
                                blocks_added: 0,
                                quick: false,
                                geometry: self.shown_geometry.unwrap_or(0),
                                baseline: None,
                                partial: true,
                                submitted: state.frame.time,
                                held: false,
                            });
                        }
                        continue;
                    };
                    if r.lit {
                        continue;
                    }
                    self.uploads.push_back(Upload {
                        baked,
                        kind: r.kind,
                        serial: r.serial,
                        scope: r.scope.clone(),
                        written: 0,
                        started: false,
                        blocks_added: 0,
                        quick: r.quick,
                        geometry: r.geometry,
                        baseline: r.baseline.clone(),
                        partial: true,
                        submitted: r.submitted,
                        held: r.background,
                    });
                }
                bake::Msg::Done { job, outcome } => {
                    let Some(r) = self.running.take_if(|r| r.id == job) else { continue };
                    self.done(state, r, outcome);
                }
                bake::Msg::Models { job, models } => self.models(state, job, *models),
                bake::Msg::Costs { job, costs } => {
                    if self.costs_job == Some(job) {
                        self.costs_job = None;
                        state.runtime.lighting.costs_wanted = false;
                        state.runtime.lighting.costs = Some(
                            costs
                                .into_iter()
                                .map(|c| hp_app::runtime::LightCostEntry {
                                    entity: c.hammerid,
                                    classname: c.classname,
                                    origin: c.origin,
                                    faces: c.faces,
                                    luxels: c.luxels,
                                })
                                .collect(),
                        );
                    }
                }
            }
        }
    }

    pub(super) fn done(&mut self, state: &mut AppState, r: Running, outcome: bake::Outcome) {
        match outcome {
            bake::Outcome::Baked(baked) => {
                self.host_quick = Some(r.quick);
                let stats = baked.stats;
                let rt = &mut state.runtime.lighting;
                rt.relight = RelightStats {
                    jobs: rt.relight.jobs + 1,
                    full: stats.full,
                    lights: stats.lights,
                    changed_lights: stats.changed_lights,
                    relit_faces: stats.relit_faces,
                    updated_faces: if baked.patch { baked.faces.len() } else { stats.faces },
                    seconds: stats.seconds,
                    direct_seconds: stats.direct_seconds,
                    bounce_seconds: stats.bounce_seconds,
                    scene_seconds: stats.scene_seconds,
                    turnaround_seconds: (state.frame.time - r.submitted).max(0.0) as f32,
                    host_pid: self.worker.info().pid,
                    host_mb: baked.child_peak.map_or(0, |u| u.working_set >> 20),
                    session_mb: stats.session_mb,
                    warm: true,
                };
                if let Some(peak) = baked.child_peak {
                    log::info!(
                        "lighting preview: the lighting process uses {} MB (peak {} MB), the editor {} MB",
                        peak.working_set >> 20,
                        peak.peak_mb(),
                        super::memory::of_self().map_or(0, |u| u.working_set >> 20)
                    );
                }
                log::info!(
                    "lighting bake {:?}: {} faces sent of {}, {} luxels, {:.3} s ({} of {} lights changed, {} faces relit, scene {:.2} s, direct {:.3} s, bounce {:.3} s){}",
                    r.kind,
                    baked.faces.len(),
                    stats.faces,
                    baked.luxels,
                    stats.seconds,
                    stats.changed_lights,
                    stats.lights,
                    stats.relit_faces,
                    stats.scene_seconds,
                    stats.direct_seconds,
                    stats.bounce_seconds,
                    if baked.leaked { ", the map leaks" } else { "" },
                );
                if !baked.warnings.is_empty() && !baked.patch {
                    log::warn!("lighting preview: {}", baked.warnings.join("; "));
                    if !r.background {
                        state.notify(Severity::Warning, tf!("app.lighting.limits", baked.warnings.join("\n")));
                    }
                }
                self.triangles = baked.triangles;
                if r.lit {
                    self.models_for = Some((r.id, r.scope.clone(), true, r.serial));
                    state.runtime.lighting.progress = Some(BakeProgress {
                        kind: r.kind,
                        phase: BakePhase::Finishing,
                        fraction: 0.97,
                        elapsed_s: 0.0,
                        faces: 0,
                    });
                    self.indirect_scene_pending(r.geometry, r.hdr);
                    return;
                }
                if r.background {
                    self.uploads.retain(|u| !u.held);
                    let prepared = &mut state.runtime.lighting.prepared;
                    prepared.preparing = false;
                    prepared.jobs += 1;
                    prepared.seconds = baked.seconds;
                    log::info!(
                        "lighting preview: prepared in the background in {:.2} s, shown when the baked preview is turned on",
                        baked.seconds
                    );
                }
                if !r.background {
                    self.models_for = Some((r.id, r.scope.clone(), false, r.serial));
                }
                self.uploads.push_back(Upload {
                    baked,
                    kind: r.kind,
                    serial: r.serial,
                    scope: r.scope,
                    written: 0,
                    started: false,
                    blocks_added: 0,
                    quick: r.quick,
                    geometry: r.geometry,
                    baseline: r.baseline,
                    partial: false,
                    submitted: r.submitted,
                    held: r.background,
                });
            }
            _ if r.background => {
                state.runtime.lighting.prepared.preparing = false;
                match outcome {
                    bake::Outcome::Failed(e) => {
                        log::warn!("lighting preview: the background preparation failed: {e}");
                        self.needs_map = true;
                    }
                    bake::Outcome::NeedsMap => self.needs_map = true,
                    _ => {}
                }
            }
            bake::Outcome::NeedsMap => {
                self.needs_map = true;
                self.layout = None;
                self.shown_geometry = None;
                state.runtime.lighting.progress = None;
                self.restart = Some(if r.kind == BakeKind::Refine { BakeKind::Update } else { r.kind });
            }
            bake::Outcome::Cancelled => {
                state.runtime.lighting.progress = None;
                if r.kind == BakeKind::Refine && std::mem::take(&mut self.cancelling) {
                    self.refine = Some((r.scope, r.serial));
                    self.restart = Some(BakeKind::Update);
                } else if std::mem::take(&mut self.cancelling) {
                    self.restart = Some(r.kind);
                } else if !r.lit && r.kind != BakeKind::Refine {
                    state.runtime.lighting.last =
                        Some(BakeSummary { kind: r.kind, cancelled: true, density: 1.0, ..Default::default() });
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

    fn indirect_scene_pending(&mut self, geometry: u64, hdr: bool) {
        self.indirect_scene = Some((geometry, hdr));
    }

    fn models(&mut self, state: &mut AppState, job: u64, m: bake::ModelLight) {
        let Some((id, scope, lit, serial)) = self.models_for.clone() else { return };
        if id != job {
            return;
        }
        self.models_for = None;
        if m.failed {
            if lit && self.running.is_none() {
                state.runtime.lighting.progress = None;
            }
            return;
        }
        let rt = &mut state.runtime.lighting;
        let preview_hdr = state.settings.render.preview_hdr(state.settings.compile.lighting);
        if lit {
            if self.running.is_none() {
                rt.progress = None;
            }
            if let Some(gfx) = state.gfx.as_mut() {
                gfx.renderer.set_indirect_volume(m.indirect.clone());
            }
            self.indirect_serial = m.indirect.as_ref().map(|_| serial);
            log::info!(
                "lighting preview: {} indirect light cells ({} in the air) in {:.2} s",
                m.indirect.as_ref().map_or(0, |v| v.cells()),
                m.indirect.as_ref().map_or(0, |v| v.valid_cells()),
                m.seconds
            );
            state.runtime.lighting.indirect.seconds = m.seconds;
            return;
        }
        state.assets.set_baked_prop_lighting(m.props.clone(), region_of(&scope));
        if m.indirect.is_some()
            && m.hdr == preview_hdr
            && let Some(gfx) = state.gfx.as_mut()
        {
            gfx.renderer.set_indirect_volume(m.indirect.clone());
            self.indirect_serial = self
                .cache
                .current(state, false, Some(serial))
                .then(|| self.cache.resolve(state, true, 0.0, true))
                .flatten();
            self.indirect_scene = self.shown_geometry.map(|g| (g, m.hdr));
        }
        let stats = &mut state.runtime.lighting.stats;
        stats.world_lights = m.props.as_ref().map_or(0, |p| p.lights.len());
        stats.static_props = m.props.as_ref().map_or(0, |p| p.static_props.len());
        stats.vertex_lit_props =
            m.props.as_ref().map_or(0, |p| p.static_props.iter().filter(|s| s.colors.is_some()).count());
        log::info!(
            "lighting preview: models lit by {} world lights, {} static props ({} with vertex lighting) in {:.2} s",
            stats.world_lights,
            stats.static_props,
            stats.vertex_lit_props,
            m.seconds
        );
    }

    pub(super) fn upload(&mut self, state: &mut AppState) {
        let mut budget = upload_budget(state);
        while budget > 0 {
            if self.hold && self.uploads.front().is_some_and(|u| u.held) {
                return;
            }
            let Some(mut up) = self.uploads.pop_front() else { return };
            let current = self.geometry_of(state, &up.scope);
            if up.geometry != current || (up.baked.patch && Some(up.baked.layout) != self.layout) {
                if !up.partial {
                    state.runtime.lighting.progress = self.running.as_ref().and(state.runtime.lighting.progress);
                    state.runtime.lighting.stale = self.has_bake;
                    if up.baked.patch && Some(up.baked.layout) != self.layout {
                        self.layout = None;
                    }
                    self.restart = Some(if self.has_bake { BakeKind::Update } else { BakeKind::Full });
                }
                continue;
            }
            if !self.apply(state, &mut up, &mut budget) {
                self.uploads.push_front(up);
                return;
            }
        }
    }

    fn apply(&mut self, state: &mut AppState, up: &mut Upload, budget: &mut usize) -> bool {
        let AppState { gfx, assets, runtime, frame, .. } = state;
        let Some(gfx) = gfx.as_mut() else { return true };
        let r = &mut gfx.renderer;
        if !up.started {
            up.started = true;
            if up.baked.patch {
                let ids: HashSet<u32> = up.baked.faces.iter().map(|f| f.id).collect();
                self.animations.retain(|a| !ids.contains(&a.face_id()));
            } else {
                self.animations.clear();
                if let Some(baseline) = up.baseline.as_ref() {
                    r.restore_baked_lights(baseline);
                    self.baseline = Some(baseline.clone());
                }
                self.shown_serial = None;
                let fits = begin_geometry(r, &up.baked, (self.shown_faces, self.shown_blocks));
                if !fits && !self.atlas_warned {
                    self.atlas_warned = true;
                    let b = &up.baked;
                    log::warn!(
                        "lighting preview: the GPU cannot hold {} lightmap layers of {}",
                        b.layers,
                        b.layer_size
                    );
                }
                self.shown_faces = up.baked.faces.len() as u32;
                self.shown_blocks = up.baked.blocks.len() as u32;
                self.layout = Some(up.baked.layout);
                self.shown_geometry = Some(up.geometry);
            }
        }
        if !up.baked.blocks.is_empty() {
            let (next, bytes) =
                add_blocks(r, assets.as_mut(), &up.baked, up.blocks_added, budget.saturating_mul(BYTES_PER_TEXEL));
            up.blocks_added = next;
            *budget = budget.saturating_sub(bytes / BYTES_PER_TEXEL);
            if next < up.baked.blocks.len() {
                return false;
            }
            up.baked.blocks = Vec::new();
            if *budget == 0 {
                return false;
            }
        }
        let (written, texels) = write_texels_counted(r, &up.baked, up.written, *budget);
        up.written = written;
        *budget = budget.saturating_sub(texels);
        let b = &up.baked;
        let rt = &mut runtime.lighting;
        if up.written < b.faces.len() {
            if !up.partial {
                let fraction = 0.95 + 0.05 * up.written as f32 / b.faces.len().max(1) as f32;
                if let Some(p) = rt.progress.as_mut() {
                    p.phase = BakePhase::Finishing;
                    p.fraction = fraction;
                }
            }
            return false;
        }
        let mut animations = std::mem::take(&mut up.baked.animations);
        self.animations.append(&mut animations);
        if up.partial {
            if up.baked.patch {
                rt.prepared.streamed_faces += up.baked.faces.len();
            }
            return true;
        }
        let b = &up.baked;
        rt.progress = self.running.as_ref().and(rt.progress);
        rt.has_bake = true;
        let density = (1.0 / b.coarsen.max(1.0)) as f32;
        let total_faces = if b.patch { self.shown_faces as usize } else { b.faces.len() };
        rt.last = Some(BakeSummary {
            kind: up.kind,
            cancelled: false,
            seconds: if b.patch { b.stats.seconds } else { b.seconds },
            faces: total_faces,
            luxels: b.luxels,
            density,
            atlas_full: b.atlas_full,
        });
        rt.relight.turnaround_seconds = (frame.time - up.submitted).max(0.0) as f32;
        let previous = rt.stats;
        rt.stats = PreviewStats {
            faces: total_faces,
            baked_faces: if b.patch {
                previous.baked_faces
            } else {
                b.faces.iter().filter(|f| f.rect.is_some()).count()
            },
            luxels: b.luxels,
            triangles: self.triangles,
            layers: if b.patch { previous.layers } else { b.layers },
            gpu_bytes: r.lightmap_stats().bytes,
            world_lights: previous.world_lights,
            static_props: previous.static_props,
            vertex_lit_props: previous.vertex_lit_props,
            scope_radius: match &up.scope {
                Scope::All => 0.0,
                Scope::Around { radius, .. } => radius.max(64.0) as f32,
            },
            scope_auto: self.auto_scoped,
        };
        self.has_bake = true;
        let baseline = up.baseline.clone().unwrap_or_else(|| r.capture_baked_lights());
        r.restore_baked_lights(&baseline);
        self.baseline = Some(baseline);
        self.shown_serial = Some(up.serial);
        if up.quick {
            self.refine = Some((up.scope.clone(), up.serial));
        } else if self.refine.as_ref().is_some_and(|(s, _)| *s == up.scope) && up.kind == BakeKind::Refine {
            self.refine = None;
        }
        if !matches!(up.kind, BakeKind::Update | BakeKind::Refine) {
            let text = tf!(
                "app.lighting.baked",
                hp_ui_mapping::lighting::fmt_count(total_faces as u64),
                hp_ui_mapping::lighting::fmt_decimal(f64::from(b.seconds), 1),
                hp_ui_mapping::lighting::fmt_count(b.luxels)
            );
            state.log(Severity::Info, text);
        }
        true
    }

    pub(super) fn schedule(&mut self, state: &mut AppState, now: f64) {
        if !self.has_bake && self.running.is_none() && self.uploads.is_empty() && self.restart.is_none() {
            return;
        }
        if let Some((key, moved)) = dragged_lights(state) {
            self.schedule_drag(state, key, &moved, now);
            return;
        }
        self.drag_key = None;
        self.drag_seen = None;
        self.drag_wait = false;
        let options = state.doc.light_plus();
        let scope = self.last_scope.clone().unwrap_or(Scope::All);
        let geometry = self.geometry_of(state, &scope);
        let light_only = !self.needs_map && self.layout.is_some() && self.shown_geometry == Some(geometry);
        let user = f64::from(options.incremental_delay);
        let delay = if light_only { user.max(MIN_UPDATE_DELAY) } else { user.max(GEOMETRY_DELAY) };
        let Some(serial) = self.cache.resolve(state, false, delay, false) else { return };
        if self.shown_serial == Some(serial) {
            self.restart = None;
            state.runtime.lighting.stale = false;
            if self.running.as_ref().is_some_and(|r| !r.lit && r.serial != serial && r.relight) {
                self.worker.stop();
                self.running = None;
                self.cancelling = false;
                state.runtime.lighting.progress = None;
            }
            if self.running.is_none()
                && self.uploads.is_empty()
                && let Some(full) = self.visible_first.take()
            {
                self.auto_scoped = false;
                self.refine = None;
                self.bake(state, full, BakeKind::Full, false);
                return;
            }
            if self.running.is_none()
                && self.uploads.is_empty()
                && let Some((scope, _)) = self.refine.take_if(|(s, _)| *s == scope)
            {
                self.bake(state, scope, BakeKind::Refine, false);
                return;
            }
            self.follow_camera(state, now);
            return;
        }
        if let Some(r) = self.running.as_ref().filter(|r| !r.lit) {
            if r.serial == serial {
                return;
            }
            let wait = if r.relight { light_only && r.kind != BakeKind::Refine } else { r.geometry == geometry };
            if wait || self.cancelling {
                state.runtime.lighting.stale = self.has_bake;
                return;
            }
            self.cancelling = true;
            self.worker.stop();
            state.runtime.lighting.stale = self.has_bake;
            return;
        }
        if !self.uploads.is_empty() {
            return;
        }
        if !options.incremental && self.restart.is_none() {
            state.runtime.lighting.stale = self.has_bake;
            return;
        }
        state.runtime.lighting.stale = self.has_bake;
        let kind = self.restart.take().unwrap_or(BakeKind::Update);
        let kind = if kind == BakeKind::Refine { BakeKind::Update } else { kind };
        self.bake(state, scope, kind, false);
    }

    pub(super) fn follow_camera(&mut self, state: &mut AppState, now: f64) {
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
            || !self.uploads.is_empty()
            || self.refine.is_some()
            || self.visible_first.is_some()
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

    pub(super) fn note_indirect(&self, state: &mut AppState) {
        use hp_app::runtime::{IndirectPhase, IndirectStatus};
        let volume = state.gfx.as_ref().and_then(|g| g.renderer.indirect_volume().cloned());
        let current = self.cache.current(state, true, self.indirect_serial);
        let on = state.settings.render.lit_indirect;
        let waiting = self.running.as_ref().is_some_and(|r| r.lit) || self.models_for.as_ref().is_some_and(|m| m.2);
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

    pub(super) fn schedule_lit(&mut self, state: &mut AppState, shown: bool) {
        if self.running.is_some() || self.busy() || self.no_lights {
            return;
        }
        if self.models_for.as_ref().is_some_and(|m| m.2) {
            return;
        }
        let options = state.doc.light_plus();
        if shown && !options.incremental {
            return;
        }
        let delay = if shown { f64::from(options.incremental_delay).max(GEOMETRY_DELAY) } else { MIN_UPDATE_DELAY };
        let Some(serial) = self.cache.resolve(state, true, delay, false) else { return };
        if self.indirect_serial == Some(serial) || self.indirect_failed == Some(serial) {
            return;
        }
        self.bake(state, Scope::All, BakeKind::Update, true);
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use hp_app::runtime::MemoryTier;

    #[test]
    fn low_memory_caps_the_luxels_to_the_lightmap_layers_it_may_use() {
        assert_eq!(luxel_budget(16, MemoryTier::Normal, 64, 2048), 16_000_000);
        let low = luxel_budget(16, MemoryTier::Low, 7, 2048);
        assert!(low < 16_000_000 && low > 757_601, "{low}");
        assert_eq!(luxel_budget(16, MemoryTier::Low, 1, 2048), 629_145);
        assert_eq!(luxel_budget(0, MemoryTier::Low, 1, 2048), 629_145);
        assert_eq!(luxel_budget(1, MemoryTier::Low, 64, 2048), 1_000_000, "a smaller user budget wins");
    }

    #[test]
    fn block_sizes_count_vertices_and_indices() {
        let block =
            bsp::Block { batches: vec![(0, vec![hp_render::lightmap::LightmapVertex::default(); 10], vec![0; 30])] };
        assert_eq!(block_bytes(&block), 10 * size_of::<hp_render::lightmap::LightmapVertex>() + 120);
        assert_eq!(block_bytes(&bsp::Block::default()), 0);
    }
}
