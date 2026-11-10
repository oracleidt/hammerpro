use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use hp_app::runtime::BakePhase;
use hp_bspc::rad::RadQuality;
use hp_light::atlas::{Atlas, AtlasRect};

use super::bsp::{self, GeoFace, SideTable, SolidInfo};
use super::child;
use super::host::client::{Gone, Link};
use super::host::{self, FaceTexels, LightOptions, LitStats, Mounts, Reply, Request};

const BAKE_STACK: usize = 64 << 20;
const REPLY_WAIT: Duration = Duration::from_millis(50);
const CANCEL_GRACE: Duration = Duration::from_secs(30);
const MODELS_IDLE: Duration = Duration::from_millis(1000);

pub(crate) struct Job {
    pub id: u64,
    pub geometry_key: u64,
    pub map: Option<hp_vmf::Map>,
    pub points: Option<Vec<hp_vmf::Entity>>,
    pub solids: HashMap<u32, SolidInfo>,
    pub fs: Option<hp_fs::GameFs>,
    pub hdr: bool,
    pub quality: RadQuality,
    pub bounces: u32,
    pub prop_shadows: bool,
    pub prop_lighting: bool,
    pub threads: (usize, usize),
    pub layer_size: u32,
    pub max_layers: u32,
    pub coarsen: f64,
    pub lit: bool,
    pub tuning: child::Tuning,
    pub exe: Option<PathBuf>,
    pub focus: Option<[f32; 3]>,
    pub stream: bool,
    pub shown: Option<u64>,
    pub models: bool,
    pub exact: bool,
    pub quiet: bool,
    pub urgent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FaceRecord {
    pub id: u32,
    pub bsp: u32,
    pub solid: u32,
    pub generation: u64,
    pub scale: f32,
    pub flags: u32,
    pub rect: Option<AtlasRect>,
    pub texels: u32,
    pub images: u32,
}

#[derive(Debug, Default)]
pub(crate) struct Baked {
    pub layout: u64,
    pub patch: bool,
    pub faces: Vec<FaceRecord>,
    pub blocks: Vec<bsp::Block>,
    pub materials: Vec<String>,
    pub layer_size: u32,
    pub layers: u32,
    pub texels: Vec<u32>,
    pub luxels: u64,
    pub atlas_full: usize,
    pub triangles: usize,
    pub unbound: usize,
    pub leaked: bool,
    pub seconds: f32,
    pub coarsen: f64,
    pub warnings: Vec<String>,
    pub hdr: bool,
    pub child_peak: Option<super::memory::Usage>,
    pub animations: Vec<super::style_maps::AnimatedFace>,
    pub stats: LitStats,
}

pub(crate) enum Outcome {
    Baked(Box<Baked>),
    Cancelled,
    Failed(String),
    NeedsMap,
}

pub(crate) struct ModelLight {
    pub props: Option<Arc<hp_editor::limits::world_light::PropLighting>>,
    pub indirect: Option<Arc<hp_render::IndirectVolume>>,
    pub seconds: f32,
    pub hdr: bool,
    pub failed: bool,
}

pub(crate) enum Msg {
    Progress { job: u64, phase: BakePhase, fraction: f32, elapsed: f32, faces: usize },
    Partial { job: u64, baked: Box<Baked> },
    Done { job: u64, outcome: Outcome },
    Models { job: u64, models: Box<ModelLight> },
    Costs { job: u64, costs: Vec<host::CostEntry> },
}

struct Cancelled;

struct Shown {
    layout: u64,
    records: Vec<FaceRecord>,
    by_bsp: HashMap<u32, usize>,
    layer_size: u32,
    layers: u32,
}

struct GeometryState {
    key: u64,
    factor: f64,
    sides: Arc<SideTable>,
    solids: HashMap<u32, SolidInfo>,
    leaked: bool,
    warnings: Vec<String>,
    shown: Option<Shown>,
}

type Stash = Vec<(Reply, Vec<u8>)>;

#[derive(Default)]
pub(crate) struct Pipeline {
    link: Option<Link>,
    setup: Option<(Mounts, usize)>,
    geometry: Option<GeometryState>,
    points: HashMap<u32, hp_vmf::Entity>,
    next_layout: u64,
    last: Option<(u64, LightOptions)>,
    models: Option<(u64, bool)>,
    stash: Stash,
}

impl Pipeline {
    fn reset(&mut self) {
        self.link = None;
        self.setup = None;
        self.geometry = None;
        self.points.clear();
        self.last = None;
        self.models = None;
        self.stash.clear();
    }

    fn link(&mut self, exe: Option<&std::path::Path>, threads: usize) -> Result<&mut Link, String> {
        if self.link.as_mut().is_some_and(|l| !l.alive()) {
            log::warn!("lighting preview: the lighting process stopped, starting it again");
            self.reset();
        }
        if self.link.is_none() {
            let link = match exe {
                Some(exe) => Link::spawn(exe, threads).map_err(|e| format!("cannot start the lighting: {e}"))?,
                None => Link::local(),
            };
            self.link = Some(link);
        }
        self.link.as_mut().ok_or_else(|| "no lighting process".to_owned())
    }

    pub(crate) fn pid(&self) -> Option<u32> {
        self.link.as_ref().and_then(Link::pid)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HostInfo {
    pub pid: Option<u32>,
    pub working_mb: u64,
    pub peak_mb: u64,
    pub warm: bool,
    pub uptime_s: f32,
}

pub(crate) struct Worker {
    tx: Option<Sender<Command>>,
    rx: Receiver<Msg>,
    wanted: Arc<AtomicU64>,
    info: Arc<std::sync::Mutex<HostInfo>>,
}

enum Command {
    Bake(Box<Job>),
    Costs { id: u64, options: LightOptions },
    Release,
}

impl Worker {
    pub(super) fn resource_probes(&self, probes: &mut hp_app::runtime::primitives::ResourceProbes) -> bool {
        let class = hp_app::runtime::primitives::PrimitiveClass::Lighting;
        probes.hold(&self.info, class);
        true
    }

    pub fn start(wake: Box<dyn Fn() + Send + Sync>) -> std::io::Result<Self> {
        let (tx, jobs) = crossbeam_channel::unbounded::<Command>();
        let (out, rx) = crossbeam_channel::unbounded();
        let wanted = Arc::new(AtomicU64::new(0));
        let w = wanted.clone();
        let info: Arc<std::sync::Mutex<HostInfo>> = Arc::default();
        let shared = info.clone();
        std::thread::Builder::new().name("hp-light-bake".to_owned()).stack_size(BAKE_STACK).spawn(move || {
            super::memory::lower_thread_memory_priority();
            let mut pipe = Pipeline::default();
            let mut due: Option<(Instant, u64)> = None;
            loop {
                let timeout = match (&due, &pipe.models) {
                    (Some((at, _)), _) => at.saturating_duration_since(Instant::now()),
                    (None, Some(_)) => REPLY_WAIT,
                    (None, None) => Duration::from_secs(3600),
                };
                let command = match jobs.recv_timeout(timeout) {
                    Ok(command) => Some(command),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                };
                pump(&mut pipe, &out, &*wake);
                let Some(command) = command else {
                    if let Some((at, id)) = due
                        && Instant::now() >= at
                    {
                        due = None;
                        if w.load(Ordering::Acquire) == id {
                            start_models(&mut pipe, id);
                        }
                    }
                    continue;
                };
                let job = match command {
                    Command::Bake(job) => {
                        due = None;
                        job
                    }
                    Command::Costs { id, options } => {
                        let costs = costs(&mut pipe, id, options);
                        let _ = out.send(Msg::Costs { job: id, costs });
                        wake();
                        continue;
                    }
                    Command::Release => {
                        if let Some(link) = pipe.link.as_mut() {
                            let never = || false;
                            let _ = link.send(Request::Release, &[], &never);
                        }
                        pipe.reset();
                        if let Ok(mut i) = shared.lock() {
                            *i = HostInfo::default();
                        }
                        continue;
                    }
                };
                if w.load(Ordering::Acquire) != job.id {
                    let _ = out.send(Msg::Done { job: job.id, outcome: Outcome::Cancelled });
                    wake();
                    continue;
                }
                let id = job.id;
                let models = job.models;
                let lit_only = job.lit;
                let quiet = job.quiet;
                let ctx = Ctx { id, wanted: &w, out: &out, wake: &*wake, start: Instant::now(), faces: 0, quiet };
                let task = || hp_editor::background::run(|| run(*job, &ctx, &mut pipe));
                let outcome = match catch_unwind(AssertUnwindSafe(task)) {
                    Ok(o) => o,
                    Err(_) if ctx.cancelled() => Outcome::Cancelled,
                    Err(_) => {
                        pipe.reset();
                        Outcome::Failed("the lighting stopped on an error".to_owned())
                    }
                };
                let lit = matches!(outcome, Outcome::Baked(_));
                let _ = out.send(Msg::Done { job: id, outcome });
                wake();
                if lit && models && !ctx.cancelled() {
                    let wait = if lit_only { Duration::ZERO } else { MODELS_IDLE };
                    due = Some((Instant::now() + wait, id));
                }
                if let Ok(mut i) = shared.lock() {
                    let usage = pipe.link.as_ref().and_then(Link::memory);
                    *i = HostInfo {
                        pid: pipe.pid(),
                        working_mb: usage.map_or(0, |u| u.working_set >> 20),
                        peak_mb: usage.map_or(0, |u| u.peak_working_set >> 20),
                        warm: pipe.geometry.is_some(),
                        uptime_s: pipe.link.as_ref().and_then(Link::uptime).map_or(0.0, |d| d.as_secs_f32()),
                    };
                }
            }
        })?;
        Ok(Self { tx: Some(tx), rx, wanted, info })
    }

    pub fn submit(&self, job: Job) {
        self.wanted.store(job.id, Ordering::Release);
        if let Some(tx) = &self.tx {
            let _ = tx.send(Command::Bake(Box::new(job)));
        }
    }

    pub fn costs(&self, id: u64, options: LightOptions) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Command::Costs { id, options });
        }
    }

    pub fn stop(&self) {
        self.wanted.store(0, Ordering::Release);
    }

    pub fn release(&self) {
        self.stop();
        if let Some(tx) = &self.tx {
            let _ = tx.send(Command::Release);
        }
    }

    pub fn info(&self) -> HostInfo {
        self.info.lock().map(|i| *i).unwrap_or_default()
    }

    pub fn poll(&self) -> Vec<Msg> {
        self.rx.try_iter().collect()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
        self.tx = None;
    }
}

struct Ctx<'a> {
    id: u64,
    wanted: &'a AtomicU64,
    out: &'a Sender<Msg>,
    wake: &'a (dyn Fn() + Send + Sync),
    start: Instant,
    faces: usize,
    quiet: bool,
}

impl Ctx<'_> {
    fn cancelled(&self) -> bool {
        self.wanted.load(Ordering::Acquire) != self.id
    }

    fn progress(&self, phase: BakePhase, fraction: f32) {
        let _ = self.out.send(Msg::Progress {
            job: self.id,
            phase,
            fraction: fraction.clamp(0.0, 1.0),
            elapsed: self.start.elapsed().as_secs_f32(),
            faces: self.faces,
        });
        if !self.quiet {
            (self.wake)();
        }
    }

    fn send(&self, msg: Msg) {
        let _ = self.out.send(msg);
        (self.wake)();
    }
}

pub(super) fn coarsen(map: &mut hp_vmf::Map, factor: f64) {
    if factor <= 1.0 {
        return;
    }
    let world = map.world.solids.iter_mut();
    let entities = map.entities.iter_mut().flat_map(|e| e.solids.iter_mut());
    for side in world.chain(entities).flat_map(|s| s.sides.iter_mut()) {
        let scale = if side.lightmap_scale > 0 { side.lightmap_scale } else { 16 };
        side.lightmap_scale = (f64::from(scale) * factor).ceil().min(f64::from(i32::MAX / 2)) as i32;
    }
}

pub(super) fn strip_hidden(map: &mut hp_vmf::Map) {
    map.entities.retain(|e| !e.hidden);
    for e in std::iter::once(&mut map.world).chain(map.entities.iter_mut()) {
        e.solids.retain(|s| !s.hidden);
        e.solids.shrink_to_fit();
    }
    map.entities.shrink_to_fit();
    map.visgroups.clear();
    map.extra_blocks.clear();
}

pub(super) struct MapBuild {
    pub bsp: Vec<u8>,
    pub leaked: bool,
    pub warnings: Vec<String>,
}

pub(super) fn compile_map(
    map: &hp_vmf::Map,
    fs: &hp_fs::GameFs,
    threads: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
    progress: &mut (dyn FnMut(f32) + Send),
    stdout: bool,
) -> Result<MapBuild, Outcome> {
    let solid_count = map.world.solids.len() + map.entities.iter().map(|e| e.solids.len()).sum::<usize>();
    let materials = crate::bspc_cli::GameMaterials { fs: fs.clone(), opts: Default::default() };
    let models = crate::bspc_cli::GameModels { fs, allow_dynamic: true };
    let builder =
        || rayon::ThreadPoolBuilder::new().num_threads(threads.max(1)).thread_name(|i| format!("hp-light-{i}"));
    let pool = match builder().stack_size(BAKE_STACK).build().or_else(|_| builder().build()) {
        Ok(p) => p,
        Err(e) => return Err(Outcome::Failed(format!("cannot start the threads: {e}"))),
    };
    let t = Instant::now();
    let tau = 1.0 + solid_count as f32 / 25_000.0;
    let mut last = Instant::now();
    let mut leaked = false;
    let mut warnings: Vec<String> = Vec::new();
    let mut log = |line: &str| {
        if cancelled() {
            resume_unwind(Box::new(Cancelled));
        }
        leaked |= line.contains("leaked");
        if line.contains(hp_bspc::compile::RELAXED_NOTE) {
            warnings.push(
                line.trim_start_matches("Warning: ").replace(&format!(" ({})", hp_bspc::compile::RELAXED_NOTE), ""),
            );
        }
        log::debug!("preview compile: {line}");
        if stdout {
            use std::io::Write;
            let mut output = std::io::stdout().lock();
            let _ = writeln!(output, "{line}");
            let _ = output.flush();
        }
        if last.elapsed() > Duration::from_millis(200) {
            last = Instant::now();
            progress(eased(0.0, 0.9, t.elapsed().as_secs_f32(), tau));
        }
    };
    let opts = hp_bspc::CompileOptions {
        vis: hp_bspc::VisLevel::Fast,
        relaxed_limits: true,
        lean_planes: true,
        preview: true,
        ..Default::default()
    };
    let mut compiled = pool.install(|| hp_bspc::compile(map, &materials, &models, &opts, &mut log));
    let mut retried = None;
    if let Err(hp_bspc::CompileError::Failed(e)) = &compiled
        && e.contains("exceeded")
    {
        log::warn!(
            "lighting preview: {e}; compiled again with faces cut at {} luxels",
            hp_bspc::compile::MAX_LIGHTMAP_DIM_LIMIT
        );
        retried = Some(e.lines().filter(|l| l.contains("exceeded")).collect::<Vec<_>>().join(", "));
        let opts = hp_bspc::CompileOptions { max_lightmap_dim: hp_bspc::compile::MAX_LIGHTMAP_DIM_LIMIT, ..opts };
        compiled = pool.install(|| hp_bspc::compile(map, &materials, &models, &opts, &mut log));
    }
    if let Some(e) = retried {
        warnings.push(format!("{e}: the faces were cut at {} luxels", hp_bspc::compile::MAX_LIGHTMAP_DIM_LIMIT));
    }
    let mut out = match compiled {
        Ok(out) => out,
        Err(hp_bspc::CompileError::Leaked { .. }) => return Err(Outcome::Failed("the map leaked".to_owned())),
        Err(e) => return Err(Outcome::Failed(e.to_string())),
    };
    log::info!("lighting preview: compiled in {:.2} s", t.elapsed().as_secs_f32());
    if cancelled() {
        return Err(Outcome::Cancelled);
    }
    Ok(MapBuild { bsp: std::mem::take(&mut out.bsp), leaked, warnings })
}

fn eased(from: f32, to: f32, t: f32, tau: f32) -> f32 {
    from + (to - from) * (1.0 - (-t / tau.max(0.1)).exp())
}

pub(super) fn mounts_of(fs: &hp_fs::GameFs) -> Mounts {
    Mounts {
        root: fs.root().map(std::path::Path::to_path_buf),
        game_dir: fs.game_dir().map(std::path::Path::to_path_buf),
        mounts: fs.sources().iter().map(hp_fs::SourceInfo::mount).collect(),
        generation: fs.generation(),
    }
}

pub(super) fn options_of(job: &Job) -> LightOptions {
    LightOptions {
        hdr: job.hdr,
        quality: child::quality_arg(job.quality).to_owned(),
        bounces: job.bounces,
        prop_shadows: job.prop_shadows,
        prop_lighting: job.prop_lighting,
        tuning: job.tuning.arg(),
        threads: job.threads.1.max(1),
    }
}

pub(super) fn lighting_entity(e: &hp_vmf::Entity) -> bool {
    e.solids.is_empty() && e.classname.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("light"))
}

pub(super) fn point_entities(map: &hp_vmf::Map) -> Vec<hp_vmf::Entity> {
    let targets: std::collections::HashSet<String> = map
        .entities
        .iter()
        .filter(|e| lighting_entity(e))
        .filter_map(|e| e.get("target").map(|t| t.trim().to_ascii_lowercase()))
        .filter(|t| !t.is_empty())
        .collect();
    map.entities
        .iter()
        .filter(|e| {
            e.solids.is_empty()
                && (lighting_entity(e)
                    || e.get("targetname").is_some_and(|n| targets.contains(&n.trim().to_ascii_lowercase())))
        })
        .cloned()
        .collect()
}

fn entity_text(entities: &[hp_vmf::Entity]) -> Vec<u8> {
    if entities.is_empty() {
        return Vec::new();
    }
    let map = hp_vmf::Map { entities: entities.to_vec(), ..Default::default() };
    hp_vmf::write(&map).into_bytes()
}

enum Wait {
    Reply(Reply, Vec<u8>),
    Cancelled,
    Gone,
}

fn wait(link: &mut Link, stash: &mut Stash, job: u64, ctx: &Ctx<'_>, phases: &mut dyn FnMut(&Reply, &[u8])) -> Wait {
    let mut cancel_sent: Option<Instant> = None;
    loop {
        if cancel_sent.is_none() && ctx.cancelled() {
            if !link.is_process() {
                return Wait::Cancelled;
            }
            link.cancel(job);
            cancel_sent = Some(Instant::now());
        }
        if cancel_sent.is_some_and(|t| t.elapsed() > CANCEL_GRACE) {
            return Wait::Gone;
        }
        match link.recv(REPLY_WAIT) {
            Ok(Some((Reply::Log { line }, _))) => log::info!("lighting process: {line}"),
            Ok(Some((reply, blob))) => {
                if reply.job() != job && !matches!(reply, Reply::Failed { job: 0, .. }) {
                    if stash.len() < 64 {
                        stash.push((reply, blob));
                    }
                    continue;
                }
                match reply {
                    Reply::Progress { .. } | Reply::Partial { .. } => {
                        if cancel_sent.is_none() {
                            phases(&reply, &blob);
                        }
                    }
                    Reply::Failed { cancelled: true, .. } if cancel_sent.is_some() || ctx.cancelled() => {
                        return Wait::Cancelled;
                    }
                    _ if cancel_sent.is_some() => return Wait::Cancelled,
                    _ => return Wait::Reply(reply, blob),
                }
            }
            Ok(None) if !link.is_process() => return Wait::Gone,
            Ok(None) => {}
            Err(Gone::Exited) => return Wait::Gone,
        }
    }
}

fn phase_of(code: u8) -> BakePhase {
    match code {
        host::PHASE_COMPILE => BakePhase::Compiling,
        host::PHASE_SCENE => BakePhase::Preparing,
        host::PHASE_DIRECT => BakePhase::Direct,
        host::PHASE_BOUNCE => BakePhase::Bounce,
        _ => BakePhase::Finishing,
    }
}

fn fraction_of(code: u8, f: f32) -> f32 {
    match code {
        host::PHASE_COMPILE => 0.02 + 0.13 * f,
        host::PHASE_SCENE => 0.15 + 0.05 * f,
        host::PHASE_DIRECT => 0.2 + 0.6 * f,
        host::PHASE_BOUNCE => 0.8 + 0.1 * f,
        _ => 0.9 + 0.05 * f,
    }
}

fn run(job: Job, ctx: &Ctx<'_>, pipe: &mut Pipeline) -> Outcome {
    let mut stash = std::mem::take(&mut pipe.stash);
    pipe.models = None;
    let outcome = run_inner(job, ctx, pipe, &mut stash);
    pipe.stash.extend(stash);
    outcome
}

fn run_inner(job: Job, ctx: &Ctx<'_>, pipe: &mut Pipeline, stash: &mut Stash) -> Outcome {
    ctx.progress(BakePhase::Preparing, 0.01);
    let fs = job.fs.clone().unwrap_or_else(hp_fs::GameFs::empty);
    let options = options_of(&job);
    let mounts = mounts_of(&fs);
    let threads = job.threads.1.max(1);
    if let Err(e) = pipe.link(job.exe.as_deref(), threads) {
        return Outcome::Failed(e);
    }
    let never_cancel = || false;
    let cancelled = || ctx.cancelled();
    if pipe.setup.as_ref() != Some(&(mounts.clone(), threads)) {
        let Some(link) = pipe.link.as_mut() else { return Outcome::Failed("no lighting process".to_owned()) };
        if link.send(Request::Setup { mounts: mounts.clone(), threads }, &[], &never_cancel).is_err() {
            pipe.reset();
            return Outcome::Failed("the lighting process stopped".to_owned());
        }
        pipe.setup = Some((mounts, threads));
        pipe.geometry = None;
        pipe.points.clear();
    }
    let job_exact = job.exact;
    if let Some(link) = pipe.link.as_mut() {
        link.set_urgent(job.urgent);
    }
    let Job {
        id, map, points, solids, hdr, layer_size, max_layers, coarsen: factor, lit, focus, stream, shown, ..
    } = job;
    let geometry_key = job.geometry_key;
    let mut compiled_bsp: Option<Arc<hp_bsp::Bsp>> = None;
    let same_geometry = pipe.geometry.as_ref().is_some_and(|g| g.key == geometry_key && g.factor == factor);
    let wanted_points = match (&map, points) {
        (Some(map), _) => Some(point_entities(map)),
        (None, Some(points)) => Some(points),
        (None, None) => None,
    };
    if let Some(map) = map.filter(|_| !same_geometry) {
        ctx.progress(BakePhase::Compiling, 0.02);
        let sides = Arc::new(SideTable::from_map(&map));
        let host_points = point_entities(&map);
        let text = hp_vmf::write(&map).into_bytes();
        drop(map);
        pipe.geometry = None;
        let link = match pipe.link.as_mut() {
            Some(l) => l,
            None => return Outcome::Failed("no lighting process".to_owned()),
        };
        let req = Request::Compile { job: id, key: geometry_key, factor, threads: job.threads.0.max(1) };
        if link.send(req, &text, &cancelled).is_err() {
            pipe.reset();
            return Outcome::Failed("the lighting process stopped".to_owned());
        }
        drop(text);
        let mut forward = |reply: &Reply, _: &[u8]| {
            if let Reply::Progress { phase, fraction, .. } = reply {
                ctx.progress(phase_of(*phase), fraction_of(*phase, *fraction));
            }
        };
        match wait(link, stash, id, ctx, &mut forward) {
            Wait::Reply(Reply::Compiled { leaked, warnings, seconds, .. }, blob) => {
                log::info!("lighting preview: geometry prepared by the lighting process in {seconds:.2} s");
                let bsp = match hp_bsp::Bsp::from_vec(blob) {
                    Ok(b) => Arc::new(b),
                    Err(e) => return Outcome::Failed(e.to_string()),
                };
                compiled_bsp = Some(bsp);
                pipe.points = host_points.into_iter().map(|e| (e.id, e)).collect();
                pipe.geometry =
                    Some(GeometryState { key: geometry_key, factor, sides, solids, leaked, warnings, shown: None });
            }
            Wait::Reply(Reply::Failed { message, .. }, _) => return Outcome::Failed(message),
            Wait::Reply(..) => return Outcome::Failed("unexpected lighting reply".to_owned()),
            Wait::Cancelled => return Outcome::Cancelled,
            Wait::Gone => {
                pipe.reset();
                return Outcome::Failed("the lighting process stopped".to_owned());
            }
        }
    }
    let Some(state) = pipe.geometry.as_mut().filter(|g| g.key == geometry_key && g.factor == factor) else {
        return Outcome::NeedsMap;
    };
    let mut removed = Vec::new();
    let mut upserts = Vec::new();
    if let Some(points) = wanted_points {
        let mut seen = std::collections::HashSet::new();
        for e in points {
            seen.insert(e.id);
            if pipe.points.get(&e.id) != Some(&e) {
                pipe.points.insert(e.id, e.clone());
                upserts.push(e);
            }
        }
        pipe.points.retain(|id, _| {
            let keep = seen.contains(id);
            if !keep {
                removed.push(*id);
            }
            keep
        });
    }
    let all = lit || state.shown.as_ref().is_none_or(|s| Some(s.layout) != shown);
    let link = match pipe.link.as_mut() {
        Some(l) => l,
        None => return Outcome::Failed("no lighting process".to_owned()),
    };
    if all && !lit && compiled_bsp.is_none() {
        if link.send(Request::Geometry { job: id }, &[], &cancelled).is_err() {
            pipe.reset();
            return Outcome::Failed("the lighting process stopped".to_owned());
        }
        let mut ignore = |_: &Reply, _: &[u8]| {};
        match wait(link, stash, id, ctx, &mut ignore) {
            Wait::Reply(Reply::Compiled { .. }, blob) => match hp_bsp::Bsp::from_vec(blob) {
                Ok(b) => compiled_bsp = Some(Arc::new(b)),
                Err(e) => return Outcome::Failed(e.to_string()),
            },
            Wait::Reply(Reply::Failed { .. }, _) => {
                pipe.geometry = None;
                return Outcome::NeedsMap;
            }
            Wait::Reply(..) => return Outcome::Failed("unexpected lighting reply".to_owned()),
            Wait::Cancelled => return Outcome::Cancelled,
            Wait::Gone => {
                pipe.reset();
                return Outcome::Failed("the lighting process stopped".to_owned());
            }
        }
    }
    let Some(state) = pipe.geometry.as_mut() else { return Outcome::NeedsMap };
    let Some(link) = pipe.link.as_mut() else { return Outcome::Failed("no lighting process".to_owned()) };
    let mut geometry_task = {
        let need = !lit && all;
        let bsp = compiled_bsp.clone();
        let sides = state.sides.clone();
        let solids = state.solids.clone();
        need.then(|| {
            std::thread::Builder::new().name("hp-light-geometry".to_owned()).spawn(move || {
                super::memory::lower_thread_memory_priority();
                bsp.map(|bsp| bsp::geometry(&bsp, &sides, &solids))
            })
        })
    };
    let req = Request::Relight {
        job: id,
        removed,
        options: options.clone(),
        focus,
        stream: stream && !lit,
        all: all && !lit,
        exact: job_exact,
    };
    if link.send(req, &entity_text(&upserts), &cancelled).is_err() {
        pipe.reset();
        return Outcome::Failed("the lighting process stopped".to_owned());
    }
    drop(upserts);
    let mut early: Option<(Shown, usize)> = None;
    let next_layout = &mut pipe.next_layout;
    let mut forward = |reply: &Reply, blob: &[u8]| match reply {
        Reply::Progress { phase, fraction, .. } => ctx.progress(phase_of(*phase), fraction_of(*phase, *fraction)),
        Reply::Partial { .. } if !all => {
            if let (Ok(faces), Some(shown)) = (host::decode_faces(blob), state.shown.as_ref()) {
                let baked = patch(shown, &faces, hdr);
                ctx.send(Msg::Partial { job: id, baked: Box::new(baked) });
            }
        }
        Reply::Partial { .. } if !lit => {
            if early.is_none()
                && let Some(Ok(handle)) = geometry_task.take()
                && let Ok(Some(mut geo)) = handle.join()
            {
                *next_layout += 1;
                let (mut baked, shown) = assemble(&mut geo, &[], layer_size, max_layers, *next_layout);
                pending_texels(&mut baked, hdr);
                baked.hdr = hdr;
                baked.unbound = geo.unbound;
                baked.coarsen = factor;
                ctx.send(Msg::Partial { job: id, baked: Box::new(baked) });
                early = Some((shown, geo.unbound));
            }
            if let (Ok(faces), Some((shown, _))) = (host::decode_faces(blob), early.as_ref()) {
                ctx.send(Msg::Partial { job: id, baked: Box::new(patch(shown, &faces, hdr)) });
            }
        }
        _ => {}
    };
    let (stats, blob) = match wait(link, stash, id, ctx, &mut forward) {
        Wait::Reply(Reply::Lit { stats, .. }, blob) => (stats, blob),
        Wait::Reply(Reply::Failed { message, .. }, _) => {
            if message == "no geometry" {
                pipe.geometry = None;
                return Outcome::NeedsMap;
            }
            return Outcome::Failed(message);
        }
        Wait::Reply(..) => return Outcome::Failed("unexpected lighting reply".to_owned()),
        Wait::Cancelled => return Outcome::Cancelled,
        Wait::Gone => {
            pipe.reset();
            return Outcome::Failed("the lighting process stopped".to_owned());
        }
    };
    pipe.last = Some((id, options));
    ctx.progress(BakePhase::Finishing, 0.95);
    let faces = match host::decode_faces(&blob) {
        Ok(f) => f,
        Err(e) => return Outcome::Failed(e),
    };
    drop(blob);
    let peak = pipe.link.as_ref().and_then(Link::memory);
    let Some(state) = pipe.geometry.as_mut() else { return Outcome::NeedsMap };
    if lit {
        let mut baked = Baked { hdr, stats, seconds: ctx.start.elapsed().as_secs_f32(), ..Baked::default() };
        baked.triangles = stats.triangles;
        baked.child_peak = peak;
        return Outcome::Baked(Box::new(baked));
    }
    let mut baked = if let Some((shown, unbound)) = early {
        let mut baked = patch(&shown, &faces, hdr);
        baked.unbound = unbound;
        state.shown = Some(shown);
        baked
    } else if all {
        let geometry = match geometry_task {
            Some(Ok(handle)) => handle.join().ok().flatten(),
            _ => None,
        };
        let Some(mut geo) = geometry else {
            return Outcome::Failed("the preview geometry could not be read".to_owned());
        };
        pipe.next_layout += 1;
        let layout_id = pipe.next_layout;
        let (mut baked, shown) = assemble(&mut geo, &faces, layer_size, max_layers, layout_id);
        baked.unbound = geo.unbound;
        state.shown = Some(shown);
        baked
    } else {
        match state.shown.as_ref() {
            Some(shown) => patch(shown, &faces, hdr),
            None => return Outcome::NeedsMap,
        }
    };
    let state = pipe.geometry.as_ref();
    baked.hdr = hdr;
    baked.luxels = stats.luxels;
    baked.triangles = stats.triangles;
    baked.child_peak = peak;
    baked.leaked = state.is_some_and(|s| s.leaked);
    baked.coarsen = factor;
    baked.warnings = state.map(|s| s.warnings.clone()).unwrap_or_default();
    baked.seconds = ctx.start.elapsed().as_secs_f32();
    baked.stats = stats;
    Outcome::Baked(Box::new(baked))
}

fn start_models(pipe: &mut Pipeline, id: u64) {
    let Some((last, options)) = pipe.last.clone() else { return };
    if last != id {
        return;
    }
    let Some(link) = pipe.link.as_mut() else { return };
    link.set_urgent(false);
    let hdr = options.hdr;
    let never = || false;
    if link.send(Request::Models { job: id, options }, &[], &never).is_err() {
        pipe.reset();
        return;
    }
    pipe.models = Some((id, hdr));
}

fn pump(pipe: &mut Pipeline, out: &Sender<Msg>, wake: &(dyn Fn() + Send + Sync)) {
    let mut replies = std::mem::take(&mut pipe.stash);
    if let Some(link) = pipe.link.as_mut() {
        while let Ok(Some(reply)) = link.recv(Duration::ZERO) {
            replies.push(reply);
        }
    }
    for (reply, blob) in replies {
        if let Reply::Log { line } = &reply {
            log::info!("lighting process: {line}");
            continue;
        }
        let Some((id, hdr)) = pipe.models else { continue };
        if reply.job() != id {
            continue;
        }
        match reply {
            Reply::Models { seconds, .. } => {
                pipe.models = None;
                let lit = hp_bsp::Bsp::from_vec(blob).ok();
                let (props, indirect) = lit.as_ref().map(|b| model_lighting(b, hdr)).unwrap_or_default();
                let models = ModelLight { props, indirect, seconds, hdr, failed: false };
                let _ = out.send(Msg::Models { job: id, models: Box::new(models) });
                wake();
            }
            Reply::Partial { .. } => {
                let shown = pipe.geometry.as_ref().and_then(|g| g.shown.as_ref());
                if let (Some(shown), Ok(faces)) = (shown, host::decode_faces(&blob)) {
                    let _ = out.send(Msg::Partial { job: id, baked: Box::new(patch(shown, &faces, hdr)) });
                    wake();
                }
            }
            Reply::Failed { .. } => {
                pipe.models = None;
                let models = ModelLight { props: None, indirect: None, seconds: 0.0, hdr, failed: true };
                let _ = out.send(Msg::Models { job: id, models: Box::new(models) });
                wake();
            }
            _ => {}
        }
    }
}

fn costs(pipe: &mut Pipeline, id: u64, options: LightOptions) -> Vec<host::CostEntry> {
    let Some(link) = pipe.link.as_mut() else { return Vec::new() };
    let never = || false;
    if link.send(Request::Costs { job: id, options }, &[], &never).is_err() {
        return Vec::new();
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match link.recv(REPLY_WAIT) {
            Ok(Some((Reply::Costs { job, costs }, _))) if job == id => return costs,
            Ok(Some((Reply::Log { line }, _))) => log::info!("lighting process: {line}"),
            Ok(Some((Reply::Failed { job, .. }, _))) if job == id => return Vec::new(),
            Ok(Some(_)) => {}
            Ok(None) if !link.is_process() => return Vec::new(),
            Ok(None) => {}
            Err(_) => return Vec::new(),
        }
    }
    Vec::new()
}

type ModelLighting =
    (Option<Arc<hp_editor::limits::world_light::PropLighting>>, Option<Arc<hp_render::IndirectVolume>>);

fn model_lighting(bsp: &hp_bsp::Bsp, hdr: bool) -> ModelLighting {
    let props = crate::dispatch::health::portals::bake_prop_lighting(bsp, hdr)
        .filter(hp_editor::limits::world_light::PropLighting::has_data)
        .map(Arc::new);
    (props, indirect_volume(bsp))
}

fn indirect_volume(bsp: &hp_bsp::Bsp) -> Option<Arc<hp_render::IndirectVolume>> {
    let lump = bsp.lump(hp_light::probes::LUMP).ok().filter(|l| !l.is_empty())?;
    let v = hp_light::probes::ProbeVolume::decode(&lump)?;
    hp_render::IndirectVolume::new(v.origin, v.spacing, &v.coords, &v.cubes).map(Arc::new)
}

pub(crate) fn child_exe() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .filter(|p| !cfg!(test) || p.file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("hammerpro")))
}

const MIN_ATLAS: u32 = 256;

fn atlas_size(sizes: &[(u32, u32)], most: u32) -> u32 {
    let area: u64 = sizes.iter().map(|&(w, h)| u64::from(w) * u64::from(h)).sum();
    let widest = sizes.iter().map(|&(w, h)| w.max(h)).max().unwrap_or(0);
    let side = ((area as f64 * 1.3).sqrt().ceil() as u32).max(widest).max(MIN_ATLAS);
    side.checked_next_power_of_two().unwrap_or(most).min(most.max(1))
}

fn images_of(f: &GeoFace, layer_size: u32) -> u32 {
    if f.bumped && f.w * 4 <= layer_size { 4 } else { 1 }
}

fn assemble(
    geo: &mut bsp::Geometry,
    lightmaps: &[FaceTexels],
    layer_size: u32,
    max_layers: u32,
    layout: u64,
) -> (Baked, Shown) {
    let images: Vec<u32> = geo.faces.iter().map(|f| images_of(f, layer_size)).collect();
    let sizes: Vec<(u32, u32)> = geo.faces.iter().zip(&images).map(|(f, n)| (f.w * n, f.h)).collect();
    let mut atlas = Atlas::new(atlas_size(&sizes, layer_size), max_layers);
    let rects = atlas.alloc_many(&sizes);
    let atlas_full = rects.iter().filter(|r| r.is_none()).count();
    let records: Vec<FaceRecord> = geo
        .faces
        .iter()
        .zip(&rects)
        .zip(&images)
        .enumerate()
        .map(|(i, ((f, rect), &images))| FaceRecord {
            id: i as u32,
            bsp: f.bsp,
            solid: f.solid,
            generation: f.generation,
            scale: f.scale,
            flags: f.flags,
            rect: *rect,
            texels: u32::MAX,
            images,
        })
        .collect();
    let by_bsp: HashMap<u32, usize> = records.iter().enumerate().map(|(i, r)| (r.bsp, i)).collect();
    let shown = Shown { layout, records, by_bsp, layer_size: atlas.size(), layers: atlas.layers() };
    let mut baked = patch(&shown, lightmaps, true);
    let mut faces = shown.records.clone();
    for f in &baked.faces {
        if let Some(slot) = faces.get_mut(f.id as usize) {
            slot.texels = f.texels;
        }
    }
    baked.faces = faces;
    baked.patch = false;
    baked.blocks = bsp::blocks(&mut geo.faces);
    geo.faces = Vec::new();
    baked.materials = std::mem::take(&mut geo.materials);
    baked.layer_size = shown.layer_size;
    baked.layers = shown.layers;
    baked.atlas_full = atlas_full;
    (baked, shown)
}

fn pending_texels(baked: &mut Baked, hdr: bool) {
    let unlit = if hdr { [1.0; 3] } else { hp_editor::limits::world_light::ldr_lightmap([1.0; 3]) };
    let value = hp_light::pack::pack_rg11b10(unlit);
    for f in &mut baked.faces {
        let Some(rect) = f.rect else { continue };
        f.texels = baked.texels.len() as u32;
        baked.texels.extend(std::iter::repeat_n(value, (rect.w * rect.h) as usize));
    }
}

fn patch(shown: &Shown, lightmaps: &[FaceTexels], hdr: bool) -> Baked {
    let mut faces = Vec::with_capacity(lightmaps.len());
    let mut texels = Vec::with_capacity(lightmaps.iter().map(|f| f.texels.len()).sum());
    let mut animations = Vec::new();
    for lm in lightmaps {
        let Some(&index) = shown.by_bsp.get(&lm.face) else { continue };
        let mut record = shown.records[index];
        let n = (lm.width * lm.height) as usize;
        let wanted = n * record.images as usize;
        if record.rect.is_none() || lm.texels.len() < wanted {
            continue;
        }
        record.texels = texels.len() as u32;
        texels.extend_from_slice(&lm.texels[..wanted]);
        if let Some(rect) = record.rect
            && !lm.layers.is_empty()
        {
            let images = record.images.max(1);
            let layers: Vec<(String, Vec<[f32; 3]>)> =
                lm.layers.iter().map(|l| (l.pattern.clone(), l.image.iter().take(wanted).copied().collect())).collect();
            let source = super::style_maps::SourceFace::new(layers);
            animations.push(source.bind(
                record.id,
                hp_render::lightmap::LightmapRect {
                    layer: rect.layer,
                    x: rect.x,
                    y: rect.y,
                    w: rect.w / images,
                    h: rect.h,
                    density: 1.0,
                    bumped: images == 4,
                },
                hdr,
            ));
        }
        faces.push(record);
    }
    Baked {
        layout: shown.layout,
        patch: true,
        faces,
        texels,
        animations,
        layer_size: shown.layer_size,
        layers: shown.layers,
        ..Baked::default()
    }
}

#[cfg(test)]
mod tests;
