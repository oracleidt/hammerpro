use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use hp_bspc::rad::session::{Control, FaceOut, Phase, Session, Stop};

use super::super::{bsp, child, style_maps};
use super::{
    CostEntry, FaceTexels, Layer, LightOptions, LitStats, Mounts, PHASE_BOUNCE, PHASE_COMPILE, PHASE_DIRECT,
    PHASE_MODELS, PHASE_OUTPUT, PHASE_SCENE, Reply, Request,
};

pub(crate) type Shared = Arc<dyn Fn(Reply, Vec<u8>) + Send + Sync>;
pub(crate) type Out<'a> = &'a (dyn Fn(Reply, Vec<u8>) + Sync);
pub(crate) type Cancelled<'a> = &'a (dyn Fn() -> bool + Sync);

struct Geometry {
    key: u64,
    map: hp_vmf::Map,
    bsp: Vec<u8>,
    entities: Vec<hp_bsp::Entity>,
}

struct Warm {
    key: u64,
    session: Session,
}

#[derive(Default)]
pub(crate) struct Engine {
    mounts: Option<Mounts>,
    fs: Option<hp_fs::GameFs>,
    threads: usize,
    light_files: Option<(Option<String>, Vec<hp_bspc::rad::LightFile>)>,
    geometry: Option<Geometry>,
    warm: Option<Warm>,
    models: Option<Arc<AtomicBool>>,
    carry: Option<hp_bspc::rad::session::Carry>,
}

struct RadModels<'a> {
    fs: &'a hp_fs::GameFs,
}

impl hp_bspc::props::ModelSource for RadModels<'_> {
    fn bounds(&self, _path: &str) -> Option<(glam::DVec3, glam::DVec3)> {
        None
    }

    fn hull_center(&self, path: &str) -> Option<glam::DVec3> {
        let h = hp_mdl::read_header(&self.fs.read(path)?).ok()?;
        let (lo, hi) = (h.hull_min, h.hull_max);
        Some(glam::DVec3::new(f64::from(lo[0] + hi[0]), f64::from(lo[1] + hi[1]), f64::from(lo[2] + hi[2])) * 0.5)
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.fs.read(path)
    }
}

fn phase_code(p: Phase) -> u8 {
    match p {
        Phase::Scene => PHASE_SCENE,
        Phase::Direct => PHASE_DIRECT,
        Phase::Bounce => PHASE_BOUNCE,
        Phase::Output => PHASE_OUTPUT,
        Phase::Models => PHASE_MODELS,
    }
}

const LIGHT_CLASSES: [&str; 4] = ["light", "light_spot", "light_environment", "light_dynamic"];

fn simple_light(e: &hp_vmf::Entity) -> bool {
    e.solids.is_empty() && LIGHT_CLASSES.iter().any(|c| e.classname.eq_ignore_ascii_case(c))
}

fn bsp_entity(e: &hp_vmf::Entity) -> hp_bsp::Entity {
    let mut out = hp_bsp::Entity::default();
    out.set("hammerid", e.id.to_string());
    out.set("classname", e.classname.clone());
    for (k, v) in &e.props {
        if !k.eq_ignore_ascii_case("id") {
            out.set(k, v.clone());
        }
    }
    for c in &e.connections {
        out.pairs.push((c.output.clone(), c.to_value_string()));
    }
    out
}

pub(crate) fn texels_of(f: &FaceOut, on: &[bool; 256], patterns: &HashMap<u8, String>, hdr: bool) -> FaceTexels {
    let lux = (f.width * f.height) as usize;
    let nn = f.normals.max(1) as usize;
    let per = lux * nn;
    let mut sum = vec![[0f32; 3]; per];
    let mut images: Vec<(u8, Vec<[f32; 3]>)> = Vec::new();
    for (k, style) in f.styles.iter().enumerate() {
        if !on[usize::from(*style)] {
            continue;
        }
        let Some(raw) = f.values.get(k * per..(k + 1) * per) else { break };
        let mut image: Vec<[f32; 3]> = raw.iter().map(|v| v.map(|c| c / 255.0)).collect();
        if nn == 4 {
            hp_editor::limits::world_light::normalize_bumped(&mut image, lux);
        }
        for (acc, c) in sum.iter_mut().zip(&image) {
            for ch in 0..3 {
                acc[ch] += c[ch];
            }
        }
        images.push((*style, image));
    }
    let animated = images.iter().any(|(s, _)| patterns.get(s).is_some_and(|p| style_maps::needs_preview(p)));
    let layers = if animated {
        images
            .into_iter()
            .map(|(s, image)| Layer { pattern: patterns.get(&s).cloned().unwrap_or_default(), image })
            .collect()
    } else {
        Vec::new()
    };
    let shown = |c: [f32; 3]| if hdr { c } else { hp_editor::limits::world_light::ldr_lightmap(c) };
    FaceTexels {
        face: f.face,
        width: f.width,
        height: f.height,
        normals: nn as u32,
        texels: sum.into_iter().map(|c| hp_light::pack::pack_rg11b10(shown(c))).collect(),
        layers,
    }
}

fn convert(faces: &[FaceOut], entities: &[hp_bsp::Entity], hdr: bool) -> Vec<FaceTexels> {
    use rayon::prelude::*;
    let on = bsp::styles_on(entities);
    let patterns = style_maps::patterns_of(entities);
    faces.par_iter().map(|f| texels_of(f, &on, &patterns, hdr)).collect()
}

fn say(out: Out<'_>, line: &str) {
    out(Reply::Log { line: line.to_owned() }, Vec::new());
}

fn mb(bytes: usize) -> u64 {
    (bytes >> 20) as u64
}

impl Engine {
    fn fs(&self) -> hp_fs::GameFs {
        self.fs.clone().unwrap_or_else(hp_fs::GameFs::empty)
    }

    fn stop_models(&mut self) {
        if let Some(flag) = self.models.take() {
            flag.store(true, Ordering::Release);
        }
    }

    pub(crate) fn handle(&mut self, req: Request, blob: Vec<u8>, shared: &Shared, cancelled: Cancelled<'_>) {
        if !matches!(req, Request::Costs { .. } | Request::Cancel { .. } | Request::Geometry { .. }) {
            self.stop_models();
        }
        let out: Out<'_> = &**shared;
        let job = match &req {
            Request::Compile { job, .. }
            | Request::Geometry { job }
            | Request::Relight { job, .. }
            | Request::Models { job, .. }
            | Request::Costs { job, .. } => *job,
            _ => 0,
        };
        let task = std::panic::AssertUnwindSafe(|| self.serve(req, blob, shared, cancelled));
        let failure = match std::panic::catch_unwind(task) {
            Ok(Ok(())) => None,
            Ok(Err(Stop::Cancelled)) => Some((true, "cancelled".to_owned())),
            Ok(Err(Stop::Failed(message))) => Some((false, message)),
            Err(_) if cancelled() => Some((true, "cancelled".to_owned())),
            Err(payload) => {
                let text = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_else(|| "the lighting stopped on an error".to_owned());
                self.warm = None;
                Some((false, text))
            }
        };
        if let Some((cancelled, message)) = failure {
            out(Reply::Failed { job, cancelled, message }, Vec::new());
        }
    }

    fn serve(&mut self, req: Request, blob: Vec<u8>, shared: &Shared, cancelled: Cancelled<'_>) -> Result<(), Stop> {
        let out: Out<'_> = &**shared;
        match req {
            Request::Setup { mounts, threads } => {
                self.threads = threads.max(1);
                if self.mounts.as_ref() != Some(&mounts) {
                    let fs =
                        hp_fs::GameFs::from_source_mounts(mounts.root.clone(), mounts.game_dir.clone(), &mounts.mounts)
                            .map_err(|e| e.to_string())?;
                    self.fs = Some(fs);
                    self.mounts = Some(mounts);
                    self.light_files = None;
                    self.warm = None;
                }
                Ok(())
            }
            Request::Compile { job, key, factor, threads } => {
                self.compile(job, key, factor, threads, &blob, out, cancelled)
            }
            Request::Relight { job, removed, options, focus, stream, all, exact } => {
                if exact && let Some(warm) = self.warm.as_mut() {
                    warm.session.forget_lighting();
                }
                self.relight(job, &removed, &blob, &options, focus, stream, all, out, cancelled)
            }
            Request::Models { job, options } => self.models(job, &options, shared, cancelled),
            Request::Geometry { job } => {
                let g = self.geometry.as_ref().ok_or_else(|| Stop::Failed("no geometry".into()))?;
                let reply =
                    Reply::Compiled { job, key: g.key, leaked: false, warnings: Vec::new(), seconds: 0.0, peak_mb: 0 };
                out(reply, g.bsp.clone());
                Ok(())
            }
            Request::Costs { job, options } => {
                let opts = self.rad_options(&options);
                let geometry = self.geometry.as_ref().ok_or_else(|| Stop::Failed("no geometry".into()))?;
                let warm = self.warm.as_ref().ok_or_else(|| Stop::Failed("no lighting session".into()))?;
                let costs = warm
                    .session
                    .light_costs(&geometry.entities, &opts)
                    .into_iter()
                    .map(|c| CostEntry {
                        hammerid: c.hammerid,
                        classname: c.classname,
                        origin: c.origin,
                        faces: c.faces,
                        luxels: c.luxels,
                    })
                    .collect();
                out(Reply::Costs { job, costs }, Vec::new());
                Ok(())
            }
            Request::Cancel { .. } => Ok(()),
            Request::Release => {
                self.warm = None;
                self.geometry = None;
                self.carry = None;
                Ok(())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compile(
        &mut self,
        job: u64,
        key: u64,
        factor: f64,
        threads: usize,
        blob: &[u8],
        out: Out<'_>,
        cancelled: Cancelled<'_>,
    ) -> Result<(), Stop> {
        let started = Instant::now();
        out(Reply::Progress { job, phase: PHASE_COMPILE, fraction: 0.0 }, Vec::new());
        let text = String::from_utf8_lossy(blob);
        let mut map = hp_vmf::parse(&text).map_err(|e| e.to_string())?;
        drop(text);
        self.carry = self.warm.take().and_then(|w| w.session.into_carry());
        self.geometry = None;
        super::super::bake::coarsen(&mut map, factor);
        super::super::bake::strip_hidden(&mut map);
        let fs = self.fs();
        let last = Mutex::new(Instant::now());
        let mut progress = |f: f32| {
            if let Ok(mut l) = last.lock()
                && l.elapsed().as_millis() >= 200
            {
                *l = Instant::now();
                out(Reply::Progress { job, phase: PHASE_COMPILE, fraction: f }, Vec::new());
            }
        };
        let built = super::super::bake::compile_map(&map, &fs, threads.max(1), cancelled, &mut progress, false)
            .map_err(|o| match o {
                super::super::bake::Outcome::Cancelled => Stop::Cancelled,
                super::super::bake::Outcome::Failed(m) => Stop::Failed(m),
                super::super::bake::Outcome::Baked(_) | super::super::bake::Outcome::NeedsMap => {
                    Stop::Failed("unexpected result".into())
                }
            })?;
        map.world.solids = Vec::new();
        for e in map.entities.iter_mut().filter(|e| e.classname.eq_ignore_ascii_case("func_detail")) {
            e.solids = Vec::new();
        }
        let parsed = hp_bsp::Bsp::from_vec(built.bsp).map_err(|e| e.to_string())?;
        let entities = parsed.entities().unwrap_or_default();
        let bsp = parsed.into_vec();
        let peak = super::super::memory::of_self().map_or(0, |u| u.peak_working_set >> 20);
        out(Reply::Log { line: format!("geometry compiled in {:.2} s", started.elapsed().as_secs_f32()) }, Vec::new());
        out(
            Reply::Compiled {
                job,
                key,
                leaked: built.leaked,
                warnings: built.warnings,
                seconds: started.elapsed().as_secs_f32(),
                peak_mb: peak,
            },
            bsp.clone(),
        );
        self.geometry = Some(Geometry { key, map, bsp, entities });
        Ok(())
    }

    fn apply_entities(&mut self, removed: &[u32], blob: &[u8]) -> Result<(), Stop> {
        if removed.is_empty() && blob.is_empty() {
            return Ok(());
        }
        let upserts: Vec<hp_vmf::Entity> = if blob.is_empty() {
            Vec::new()
        } else {
            let text = String::from_utf8_lossy(blob);
            hp_vmf::parse(&text).map_err(|e| e.to_string())?.entities
        };
        let fs = self.fs();
        let geometry = self.geometry.as_mut().ok_or_else(|| Stop::Failed("no geometry".into()))?;
        let mut simple = true;
        for id in removed {
            if let Some(i) = geometry.map.entities.iter().position(|e| e.id == *id) {
                simple &= simple_light(&geometry.map.entities[i]);
                geometry.map.entities.remove(i);
            }
        }
        for e in &upserts {
            simple &= simple_light(e) && !e.hidden;
            match geometry.map.entities.iter_mut().find(|x| x.id == e.id) {
                Some(slot) => {
                    simple &= simple_light(slot);
                    *slot = e.clone();
                }
                None => geometry.map.entities.push(e.clone()),
            }
        }
        if simple {
            let ids: Vec<String> = removed.iter().map(u32::to_string).collect();
            geometry.entities.retain(|e| e.get("hammerid").is_none_or(|h| !ids.iter().any(|i| i == h)));
            for e in &upserts {
                let id = e.id.to_string();
                let converted = bsp_entity(e);
                match geometry.entities.iter_mut().find(|x| x.get("hammerid") == Some(id.as_str())) {
                    Some(slot) => *slot = converted,
                    None => geometry.entities.push(converted),
                }
            }
            return Ok(());
        }
        let materials = crate::bspc_cli::GameMaterials { fs, opts: Default::default() };
        let bytes =
            hp_bspc::compile::entities_only(&geometry.map, &materials, &geometry.bsp).map_err(|e| e.to_string())?;
        let parsed = hp_bsp::Bsp::from_vec(bytes).map_err(|e| e.to_string())?;
        geometry.entities = parsed.entities().unwrap_or_default();
        Ok(())
    }

    fn rad_options(&mut self, o: &LightOptions) -> hp_bspc::rad::RadOptions {
        let tuning = child::Tuning::parse(&o.tuning);
        let mut opts = child::options(
            o.hdr,
            child::parse_quality(&o.quality),
            o.bounces,
            Some(o.threads.max(1)),
            o.prop_shadows,
            o.prop_lighting,
            tuning,
        );
        let world = self.geometry.as_ref().and_then(|g| {
            g.entities
                .first()
                .and_then(|w| w.get("_lightsrad").map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned))
        });
        if self.light_files.as_ref().is_none_or(|(w, _)| *w != world) {
            let game: Option<PathBuf> = self.fs.as_ref().and_then(|fs| fs.game_dir().map(std::path::Path::to_path_buf));
            let files = crate::bspc_cli::light_files(game.as_deref(), None, None, world.as_deref(), None, &mut |l| {
                log::debug!("lighting host: {l}");
            });
            self.light_files = Some((world, files));
        }
        opts.texlights.files = self.light_files.as_ref().map(|(_, f)| f.clone()).unwrap_or_default();
        opts
    }

    #[allow(clippy::too_many_arguments)]
    fn relight(
        &mut self,
        job: u64,
        removed: &[u32],
        blob: &[u8],
        options: &LightOptions,
        focus: Option<[f32; 3]>,
        stream: bool,
        all: bool,
        out: Out<'_>,
        cancelled: Cancelled<'_>,
    ) -> Result<(), Stop> {
        let started = Instant::now();
        self.apply_entities(removed, blob)?;
        let opts = self.rad_options(options);
        let fs = self.fs();
        let models = RadModels { fs: &fs };
        let geometry = self.geometry.as_ref().ok_or_else(|| Stop::Failed("no geometry".into()))?;
        let last = Mutex::new((Instant::now(), None::<Phase>));
        let progress = |phase: Phase, fraction: f32| {
            if let Ok(mut l) = last.lock()
                && (l.1 != Some(phase) || l.0.elapsed().as_millis() >= 150)
            {
                *l = (Instant::now(), Some(phase));
                out(Reply::Progress { job, phase: phase_code(phase), fraction }, Vec::new());
            }
        };
        let hdr = options.hdr;
        let entities_now = geometry.entities.clone();
        let partial = |faces: Vec<FaceOut>| {
            let converted = convert(&faces, &entities_now, hdr);
            out(Reply::Partial { job }, super::encode_faces(&converted));
        };
        let ctl = Control {
            cancelled,
            progress: &progress,
            partial: stream.then_some(&partial as &(dyn Fn(Vec<FaceOut>) + Sync)),
        };
        let mut scene_seconds = 0.0f32;
        let reuse = self.warm.as_ref().is_some_and(|w| w.key == geometry.key && w.session.compatible(&opts));
        if !reuse {
            self.warm = None;
            let t = Instant::now();
            let mut session = Session::new(geometry.bsp.clone(), &opts, &models, &mut |l| say(out, l), &ctl)?;
            session.accept_carry(self.carry.take());
            scene_seconds = t.elapsed().as_secs_f32();
            self.warm = Some(Warm { key: geometry.key, session });
        }
        let warm = self.warm.as_mut().ok_or_else(|| Stop::Failed("no lighting session".into()))?;
        let mut relit = warm.session.relight(&geometry.entities, &opts, focus, &ctl, &mut |l| say(out, l))?;
        if all && !relit.full {
            relit.faces = warm.session.output_all();
        }
        let styled = warm.session.styled_entities().map(<[hp_bsp::Entity]>::to_vec).unwrap_or_default();
        let converted = convert(&relit.faces, &styled, hdr);
        let stats = LitStats {
            full: relit.full,
            lights: relit.lights,
            changed_lights: relit.changed_lights,
            relit_faces: relit.relit_faces,
            faces: warm.session.faces(),
            luxels: warm.session.luxels() as u64,
            triangles: warm.session.shadow_triangles(),
            scene_seconds,
            direct_seconds: relit.direct_seconds as f32,
            bounce_seconds: relit.bounce_seconds as f32,
            seconds: started.elapsed().as_secs_f32(),
            session_mb: mb(warm.session.bytes()),
            process_mb: super::super::memory::of_self().map_or(0, |u| u.working_set >> 20),
        };
        out(Reply::Lit { job, stats }, super::encode_faces(&converted));
        Ok(())
    }

    fn models(
        &mut self,
        job: u64,
        options: &LightOptions,
        shared: &Shared,
        cancelled: Cancelled<'_>,
    ) -> Result<(), Stop> {
        let out: Out<'_> = &**shared;
        let started = Instant::now();
        let opts = self.rad_options(options);
        let fs = self.fs();
        let warm = self.warm.as_mut().ok_or_else(|| Stop::Failed("no lighting session".into()))?;
        let last = Mutex::new(Instant::now());
        let progress = |phase: Phase, fraction: f32| {
            if let Ok(mut l) = last.lock()
                && l.elapsed().as_millis() >= 150
            {
                *l = Instant::now();
                out(Reply::Progress { job, phase: phase_code(phase), fraction }, Vec::new());
            }
        };
        let ctl = Control { cancelled, progress: &progress, partial: None };
        let settled = warm.session.settle(&opts, &ctl, &mut |l| say(out, l))?;
        if !settled.faces.is_empty() {
            let styled = warm.session.styled_entities().map(<[hp_bsp::Entity]>::to_vec).unwrap_or_default();
            let converted = convert(&settled.faces, &styled, options.hdr);
            out(Reply::Partial { job }, super::encode_faces(&converted));
        }
        let snapshot = warm.session.models_job().ok_or_else(|| Stop::Failed("no lighting to write".into()))?;
        let flag = Arc::new(AtomicBool::new(false));
        self.models = Some(flag.clone());
        let shared = shared.clone();
        let workers = (self.threads / 2).max(1);
        let spawned =
            std::thread::Builder::new().name("hp-light-models".to_owned()).stack_size(64 << 20).spawn(move || {
                super::server::idle_priority();
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .thread_name(|i| format!("hp-light-models-{i}"))
                    .stack_size(8 << 20)
                    .start_handler(|_| super::server::idle_priority())
                    .build();
                let models = RadModels { fs: &fs };
                let stopped = || flag.load(Ordering::Acquire);
                let last = Mutex::new(Instant::now());
                let progress = |phase: Phase, fraction: f32| {
                    if let Ok(mut l) = last.lock()
                        && l.elapsed().as_millis() >= 250
                    {
                        *l = Instant::now();
                        shared(Reply::Progress { job, phase: phase_code(phase), fraction }, Vec::new());
                    }
                };
                let ctl = Control { cancelled: &stopped, progress: &progress, partial: None };
                let work = || {
                    let mut lines = Vec::new();
                    let lit = snapshot.lit_bsp(&opts, &models, &ctl, &mut |l| lines.push(l.to_owned()));
                    for line in lines {
                        say(&*shared, &line);
                    }
                    let bsp = hp_bsp::Bsp::from_vec(lit?).map_err(|e| Stop::Failed(e.to_string()))?;
                    child::for_models(bsp).map_err(Stop::Failed)
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &pool {
                    Ok(pool) => pool.install(work),
                    Err(_) => work(),
                }));
                match result {
                    Ok(Ok(bytes)) if !stopped() => {
                        shared(Reply::Models { job, seconds: started.elapsed().as_secs_f32() }, bytes);
                    }
                    Ok(Err(Stop::Failed(message))) => {
                        shared(Reply::Failed { job, cancelled: false, message }, Vec::new())
                    }
                    Err(_) if !stopped() => shared(
                        Reply::Failed {
                            job,
                            cancelled: false,
                            message: "the model lighting stopped on an error".into(),
                        },
                        Vec::new(),
                    ),
                    _ => shared(Reply::Failed { job, cancelled: true, message: "cancelled".into() }, Vec::new()),
                }
            });
        spawned.map(|_| ()).map_err(|e| Stop::Failed(e.to_string()))
    }
}
