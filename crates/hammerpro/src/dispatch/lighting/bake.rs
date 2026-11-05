use std::collections::HashMap;
use std::io::BufRead;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use hp_app::runtime::BakePhase;
use hp_bspc::rad::RadQuality;
use hp_light::atlas::{Atlas, AtlasRect};

use super::bsp::{self, FlatLightmaps, GeoFace, SideTable, SolidInfo};
use super::child;

const BAKE_STACK: usize = 64 << 20;

pub(crate) struct Job {
    pub id: u64,
    pub serial: u64,
    pub map: Option<hp_vmf::Map>,
    pub keep: bool,
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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FaceRecord {
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
    pub props: Option<Arc<hp_editor::limits::world_light::PropLighting>>,
    pub indirect: Option<Arc<hp_render::IndirectVolume>>,
    pub hdr: bool,
    pub child_peak: Option<super::memory::Usage>,
}

pub(crate) enum Outcome {
    Baked(Box<Baked>),
    Cancelled,
    Failed(String),
}

pub(crate) enum Msg {
    Progress { job: u64, phase: BakePhase, fraction: f32, elapsed: f32, faces: usize },
    Done { job: u64, outcome: Outcome },
}

struct Cancelled;

#[derive(Clone)]
struct Compiled {
    serial: u64,
    bsp: Arc<hp_bsp::Bsp>,
    sides: Arc<SideTable>,
    leaked: bool,
    warnings: Vec<String>,
}

type Cache = Arc<std::sync::Mutex<Option<Compiled>>>;

pub(crate) struct Worker {
    tx: Option<Sender<Job>>,
    rx: Receiver<Msg>,
    wanted: Arc<AtomicU64>,
    cache: Cache,
}

impl Worker {
    pub fn start(wake: Box<dyn Fn() + Send + Sync>) -> std::io::Result<Self> {
        let (tx, jobs) = crossbeam_channel::unbounded::<Job>();
        let (out, rx) = crossbeam_channel::unbounded();
        let wanted = Arc::new(AtomicU64::new(0));
        let w = wanted.clone();
        let cache: Cache = Arc::default();
        let kept = cache.clone();
        std::thread::Builder::new().name("hp-light-bake".to_owned()).spawn(move || {
            lower_thread_priority();
            for job in jobs {
                if w.load(Ordering::Acquire) != job.id {
                    continue;
                }
                let id = job.id;
                let ctx = Ctx { id, wanted: &w, out: &out, wake: &*wake, start: Instant::now(), faces: 0, game: None };
                let task = || hp_editor::background::run(|| run(job, &ctx, &kept));
                let outcome = match catch_unwind(AssertUnwindSafe(task)) {
                    Ok(o) => o,
                    Err(_) if ctx.cancelled() => Outcome::Cancelled,
                    Err(_) => Outcome::Failed("the compiler stopped on an error".to_owned()),
                };
                let _ = out.send(Msg::Done { job: id, outcome });
                wake();
            }
        })?;
        Ok(Self { tx: Some(tx), rx, wanted, cache })
    }

    pub fn submit(&self, job: Job) {
        self.wanted.store(job.id, Ordering::Release);
        if let Some(tx) = &self.tx {
            let _ = tx.send(job);
        }
    }

    pub fn stop(&self) {
        self.wanted.store(0, Ordering::Release);
        if let Ok(mut c) = self.cache.lock() {
            *c = None;
        }
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
    game: Option<&'a Path>,
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
        (self.wake)();
    }
}

fn eased(from: f32, to: f32, t: f32, tau: f32) -> f32 {
    from + (to - from) * (1.0 - (-t / tau.max(0.1)).exp())
}

fn seconds_per_luxel(q: RadQuality) -> f32 {
    match q {
        RadQuality::Draft => 1.0 / 70_000.0,
        RadQuality::Normal => 1.0 / 23_000.0,
        RadQuality::Final => 1.0 / 8_000.0,
    }
}

fn coarsen(map: &mut hp_vmf::Map, factor: f64) {
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

fn strip_hidden(map: &mut hp_vmf::Map) {
    map.entities.retain(|e| !e.hidden);
    for e in std::iter::once(&mut map.world).chain(map.entities.iter_mut()) {
        e.solids.retain(|s| !s.hidden);
        e.solids.shrink_to_fit();
    }
    map.entities.shrink_to_fit();
    map.visgroups.clear();
    map.extra_blocks.clear();
}

fn lower_thread_priority() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
    super::memory::lower_thread_memory_priority();
}

fn note_memory(stage: &str) {
    if let Some(u) = super::memory::of_self() {
        log::debug!("lighting preview memory: {stage}: {} MB", u.working_set >> 20);
    }
}

fn run(job: Job, ctx: &Ctx<'_>, cache: &std::sync::Mutex<Option<Compiled>>) -> Outcome {
    note_memory("start");
    let Job {
        serial,
        map,
        keep,
        solids,
        fs,
        hdr,
        quality,
        bounces,
        prop_shadows,
        prop_lighting,
        threads,
        layer_size,
        max_layers,
        coarsen: factor,
        lit: indirect_only,
        tuning,
        exe,
        ..
    } = job;
    ctx.progress(BakePhase::Preparing, 0.01);
    let fs = fs.unwrap_or_else(hp_fs::GameFs::empty);
    let reused = cache.lock().ok().and_then(|mut c| c.take()).filter(|c| c.serial == serial);
    let compiled = match (reused, map) {
        (Some(c), _) => {
            log::info!("lighting preview: the compiled map of the quick pass is used again");
            c
        }
        (None, Some(map)) => match compile(map, factor, &fs, threads.0, ctx, serial) {
            Ok(c) => c,
            Err(o) => return o,
        },
        (None, None) => return Outcome::Cancelled,
    };
    if keep && let Ok(mut c) = cache.lock() {
        *c = Some(compiled.clone());
    }
    let Compiled { bsp, sides, leaked, warnings, .. } = compiled;
    let (faces, luxels) = bsp::lit_faces(&bsp);
    let geometry = {
        let bsp = bsp.clone();
        let task = move || {
            lower_thread_priority();
            if indirect_only { bsp::Geometry::default() } else { bsp::geometry(&bsp, &sides, &solids) }
        };
        std::thread::Builder::new().name("hp-light-geometry".to_owned()).spawn(task)
    };
    let ctx = Ctx { faces, game: fs.game_dir(), ..*ctx };
    ctx.progress(BakePhase::Direct, 0.15);

    let tau = 2.0 + luxels as f32 * seconds_per_luxel(quality);
    let args = LightArgs { hdr, quality, bounces, threads: threads.1, prop_shadows, prop_lighting, tuning };
    let lit = light(bsp, exe.as_deref(), args, &ctx, tau);
    note_memory("lit");
    let mut geo = match geometry.map(std::thread::JoinHandle::join) {
        Ok(Ok(g)) => g,
        _ => return Outcome::Failed("the preview geometry could not be read".to_owned()),
    };
    let (lit, triangles) = match lit {
        Ok(f) => f,
        Err(LightError::Cancelled) => return Outcome::Cancelled,
        Err(LightError::Failed(e)) => return Outcome::Failed(e),
    };
    let luxels: u64 = geo.faces.iter().map(|f| u64::from(f.w) * u64::from(f.h)).sum();
    ctx.progress(BakePhase::Finishing, 0.95);
    let child::LitResult { flat, bsp: lit_bsp, peak } = lit;
    note_memory("result read");
    let (props, indirect) = model_lighting(lit_bsp, hdr);
    note_memory("models");
    let mut baked = if indirect_only { Baked::default() } else { assemble(&mut geo, flat, layer_size, max_layers) };
    note_memory("assembled");
    baked.props = props;
    baked.indirect = indirect;
    baked.hdr = hdr;
    baked.luxels = luxels;
    baked.triangles = triangles;
    baked.child_peak = peak;
    baked.unbound = geo.unbound;
    baked.leaked = leaked;
    baked.coarsen = factor;
    baked.warnings = warnings;
    baked.seconds = ctx.start.elapsed().as_secs_f32();
    Outcome::Baked(Box::new(baked))
}

fn compile(
    mut map: hp_vmf::Map,
    factor: f64,
    fs: &hp_fs::GameFs,
    threads: usize,
    ctx: &Ctx<'_>,
    serial: u64,
) -> Result<Compiled, Outcome> {
    coarsen(&mut map, factor);
    let sides = SideTable::from_map(&map);
    let solid_count = map.world.solids.len() + map.entities.iter().map(|e| e.solids.len()).sum::<usize>();
    strip_hidden(&mut map);
    let materials = crate::bspc_cli::GameMaterials { fs: fs.clone(), opts: Default::default() };
    let models = crate::bspc_cli::GameModels { fs, allow_dynamic: false };
    let builder = || {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads.max(1))
            .thread_name(|i| format!("hp-light-{i}"))
            .start_handler(|_| lower_thread_priority())
    };
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
        if ctx.cancelled() {
            resume_unwind(Box::new(Cancelled));
        }
        leaked |= line.contains("leaked");
        if line.contains(hp_bspc::compile::RELAXED_NOTE) {
            warnings.push(
                line.trim_start_matches("Warning: ").replace(&format!(" ({})", hp_bspc::compile::RELAXED_NOTE), ""),
            );
        }
        log::debug!("preview compile: {line}");
        if last.elapsed() > Duration::from_millis(200) {
            last = Instant::now();
            ctx.progress(BakePhase::Compiling, eased(0.02, 0.12, t.elapsed().as_secs_f32(), tau));
        }
    };
    ctx.progress(BakePhase::Compiling, 0.02);
    let opts = hp_bspc::CompileOptions {
        vis: hp_bspc::VisLevel::Fast,
        relaxed_limits: true,
        lean_planes: true,
        preview: true,
        ..Default::default()
    };
    let mut compiled = pool.install(|| hp_bspc::compile(&map, &materials, &models, &opts, &mut log));
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
        compiled = pool.install(|| hp_bspc::compile(&map, &materials, &models, &opts, &mut log));
    }
    drop(map);
    if let Some(e) = retried {
        warnings.push(format!("{e}: the faces were cut at {} luxels", hp_bspc::compile::MAX_LIGHTMAP_DIM_LIMIT));
    }
    let mut out = match compiled {
        Ok(out) => out,
        Err(hp_bspc::CompileError::Leaked { .. }) => return Err(Outcome::Failed("the map leaked".to_owned())),
        Err(e) => return Err(Outcome::Failed(e.to_string())),
    };
    log::info!("lighting preview: compiled in {:.2} s", t.elapsed().as_secs_f32());
    note_memory("compiled");
    if ctx.cancelled() {
        return Err(Outcome::Cancelled);
    }
    let bytes = std::mem::take(&mut out.bsp);
    drop(out);
    let bsp = match hp_bsp::Bsp::from_vec(bytes) {
        Ok(b) => Arc::new(b),
        Err(e) => return Err(Outcome::Failed(e.to_string())),
    };
    Ok(Compiled { serial, bsp, sides: Arc::new(sides), leaked, warnings })
}

enum LightError {
    Cancelled,
    Failed(String),
}

struct TempFiles(Vec<PathBuf>);

impl Drop for TempFiles {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn line_phase(line: &str, triangles: &mut usize) -> Option<(BakePhase, f32)> {
    if let Some(i) = line.find(" shadow triangles") {
        let head = &line[..i];
        let n = head.rsplit(|c: char| !c.is_ascii_digit()).next().unwrap_or("");
        *triangles = n.parse().unwrap_or(*triangles);
    }
    if line.starts_with("scene built") {
        Some((BakePhase::Direct, 0.2))
    } else if line.starts_with("direct light in") {
        Some((BakePhase::Bounce, 0.8))
    } else if line.starts_with("bounced light in") || line.starts_with("leaf ambient") {
        Some((BakePhase::Finishing, 0.9))
    } else {
        None
    }
}

type ModelLight = (Option<Arc<hp_editor::limits::world_light::PropLighting>>, Option<Arc<hp_render::IndirectVolume>>);

fn model_lighting(bytes: Vec<u8>, hdr: bool) -> ModelLight {
    let Some(bsp) = hp_bsp::Bsp::from_vec(bytes).map_err(|e| log::warn!("lighting preview: the lit map: {e}")).ok()
    else {
        return (None, None);
    };
    let props = crate::dispatch::health::portals::bake_prop_lighting(&bsp, hdr)
        .filter(hp_editor::limits::world_light::PropLighting::has_data)
        .map(Arc::new);
    (props, indirect_volume(&bsp))
}

fn indirect_volume(bsp: &hp_bsp::Bsp) -> Option<Arc<hp_render::IndirectVolume>> {
    let lump = bsp.lump(hp_light::probes::LUMP).ok().filter(|l| !l.is_empty())?;
    let v = hp_light::probes::ProbeVolume::decode(&lump)?;
    hp_render::IndirectVolume::new(v.origin, v.spacing, &v.coords, &v.cubes).map(Arc::new)
}

#[derive(Clone, Copy, Debug)]
struct LightArgs {
    hdr: bool,
    quality: RadQuality,
    bounces: u32,
    threads: usize,
    prop_shadows: bool,
    prop_lighting: bool,
    tuning: child::Tuning,
}

pub(crate) fn child_exe() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .filter(|p| p.file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("hammerpro")))
}

fn light(
    bsp: Arc<hp_bsp::Bsp>,
    exe: Option<&Path>,
    la: LightArgs,
    ctx: &Ctx<'_>,
    tau: f32,
) -> Result<(child::LitResult, usize), LightError> {
    let LightArgs { hdr, quality, bounces, threads, prop_shadows, prop_lighting, tuning } = la;
    let dir = std::env::temp_dir();
    let stem = format!("hammerpro-light-{}-{}", std::process::id(), ctx.id);
    let (input, output) = (dir.join(format!("{stem}.bsp")), dir.join(format!("{stem}.lit")));
    let temp = TempFiles(vec![input.clone(), output.clone()]);
    if let Some(exe) = exe
        && std::fs::write(&input, bsp.bytes()).is_ok()
    {
        drop(bsp);
        let r = light_child(exe, &input, &output, la, ctx, tau);
        drop(temp);
        return r;
    }
    let mut triangles = 0;
    let mut phase = (BakePhase::Direct, 0.15);
    let mut log = |l: &str| {
        if let Some(p) = line_phase(l, &mut triangles) {
            phase = p;
            ctx.progress(p.0, p.1);
        }
    };
    let lit = child::light(
        bsp.bytes().to_vec(),
        hdr,
        quality,
        bounces,
        Some(threads.max(1)),
        ctx.game,
        prop_shadows,
        prop_lighting,
        tuning,
        &mut log,
    );
    drop(bsp);
    if ctx.cancelled() {
        return Err(LightError::Cancelled);
    }
    lit.map(|f| (f, triangles)).map_err(LightError::Failed)
}

fn light_child(
    exe: &Path,
    input: &Path,
    output: &Path,
    la: LightArgs,
    ctx: &Ctx<'_>,
    tau: f32,
) -> Result<(child::LitResult, usize), LightError> {
    use std::process::{Command, Stdio};
    let LightArgs { hdr, quality, bounces, threads, prop_shadows, prop_lighting, tuning } = la;
    let mut cmd = Command::new(exe);
    cmd.arg(child::LIGHT_ARG)
        .arg(input)
        .arg(output)
        .arg(if hdr { "1" } else { "0" })
        .arg(child::quality_arg(quality))
        .arg(bounces.to_string())
        .arg(threads.max(1).to_string())
        .arg(ctx.game.map(|g| g.as_os_str().to_owned()).unwrap_or_default())
        .arg(if prop_shadows { "1" } else { "0" })
        .arg(if prop_lighting { "1" } else { "0" })
        .arg(tuning.arg())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{BELOW_NORMAL_PRIORITY_CLASS, CREATE_NO_WINDOW};
        cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    let mut proc = cmd.spawn().map_err(|e| LightError::Failed(format!("cannot start the lighting: {e}")))?;
    let _stdin = proc.stdin.take();
    let (tx, lines) = crossbeam_channel::unbounded::<String>();
    if let Some(out) = proc.stdout.take() {
        let _ = std::thread::Builder::new().name("hp-light-out".to_owned()).spawn(move || {
            for line in std::io::BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    let mut triangles = 0;
    let mut phase = (BakePhase::Direct, 0.15f32);
    let mut phase_start = Instant::now();
    let mut last_lines: Vec<String> = Vec::new();
    let mut last_report = Instant::now();
    let status = loop {
        if ctx.cancelled() {
            let _ = proc.kill();
            let _ = proc.wait();
            return Err(LightError::Cancelled);
        }
        match lines.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                log::debug!("preview lighting: {line}");
                if let Some(p) = line_phase(&line, &mut triangles) {
                    phase = p;
                    phase_start = Instant::now();
                    ctx.progress(p.0, p.1);
                }
                last_lines.push(line);
                if last_lines.len() > 8 {
                    last_lines.remove(0);
                }
                continue;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => match proc.wait() {
                Ok(s) => break s,
                Err(e) => return Err(LightError::Failed(e.to_string())),
            },
        }
        if last_report.elapsed() > Duration::from_millis(250) {
            last_report = Instant::now();
            let t = phase_start.elapsed().as_secs_f32();
            let fraction = match phase.0 {
                BakePhase::Direct => eased(phase.1, 0.8, t, tau),
                BakePhase::Bounce => eased(phase.1, 0.9, t, tau * 0.2),
                _ => eased(phase.1, 0.95, t, 2.0),
            };
            ctx.progress(phase.0, fraction);
        }
        if let Ok(Some(s)) = proc.try_wait() {
            break s;
        }
    };
    if !status.success() {
        let why = last_lines.iter().rev().find(|l| l.starts_with("Error")).cloned();
        return Err(LightError::Failed(why.unwrap_or_else(|| format!("the lighting stopped ({status})"))));
    }
    let mut lit = child::read_result(output).map_err(LightError::Failed)?;
    lit.peak = super::memory::of_child(&proc);
    Ok((lit, triangles))
}

const MIN_ATLAS: u32 = 256;

fn atlas_size(sizes: &[(u32, u32)], most: u32) -> u32 {
    let area: u64 = sizes.iter().map(|&(w, h)| u64::from(w) * u64::from(h)).sum();
    let widest = sizes.iter().map(|&(w, h)| w.max(h)).max().unwrap_or(0);
    let side = ((area as f64 * 1.3).sqrt().ceil() as u32).max(widest).max(MIN_ATLAS);
    side.checked_next_power_of_two().unwrap_or(most).min(most.max(1))
}

fn assemble(geo: &mut bsp::Geometry, flat: FlatLightmaps, layer_size: u32, max_layers: u32) -> Baked {
    let lit = |f: &GeoFace| {
        flat.face(f.bsp as usize).filter(|&(o, n)| (o as usize) + (f.w * f.h * n) as usize <= flat.texels.len())
    };
    geo.faces.retain(|f| lit(f).is_some());
    let images: Vec<(u32, u32)> = geo
        .faces
        .iter()
        .map(|f| {
            let (o, n) = lit(f).unwrap_or((0, 1));
            (o, if n == 4 && f.w * 4 <= layer_size { 4 } else { 1 })
        })
        .collect();
    let sizes: Vec<(u32, u32)> = geo.faces.iter().zip(&images).map(|(f, (_, n))| (f.w * n, f.h)).collect();
    let mut atlas = Atlas::new(atlas_size(&sizes, layer_size), max_layers);
    let rects = atlas.alloc_many(&sizes);
    let atlas_full = rects.iter().filter(|r| r.is_none()).count();
    let faces: Vec<FaceRecord> = geo
        .faces
        .iter()
        .zip(&rects)
        .zip(&images)
        .map(|((f, rect), &(texels, images))| FaceRecord {
            solid: f.solid,
            generation: f.generation,
            scale: f.scale,
            flags: f.flags,
            rect: *rect,
            texels,
            images,
        })
        .collect();
    let texels = flat.texels;
    let blocks = bsp::blocks(&mut geo.faces);
    geo.faces = Vec::new();
    Baked {
        faces,
        blocks,
        materials: std::mem::take(&mut geo.materials),
        layer_size: atlas.size(),
        layers: atlas.layers(),
        texels,
        atlas_full,
        ..Baked::default()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use hp_render::lightmap::face_flags;

    use super::*;

    #[test]
    fn lines_give_phases() {
        let mut tris = 0;
        let l = "29243 faces, 27422 with lightmaps (9536925 luxels), 1002 lights, 211295 shadow triangles, passes: HDR";
        assert_eq!(line_phase(l, &mut tris), None);
        assert_eq!(tris, 211295);
        assert_eq!(line_phase("scene built in 503 ms", &mut tris), Some((BakePhase::Direct, 0.2)));
        assert_eq!(line_phase("direct light in 4 ms", &mut tris).map(|p| p.0), Some(BakePhase::Bounce));
        assert_eq!(line_phase("leaf ambient: 3 samples in 1 ms", &mut tris).map(|p| p.0), Some(BakePhase::Finishing));
        assert!(eased(0.2, 0.8, 1e9, 10.0) <= 0.8 && eased(0.2, 0.8, 0.0, 10.0) == 0.2);
    }

    fn solid(id: u32, side0: u32, lo: [f64; 3], hi: [f64; 3]) -> String {
        let ([x0, y0, z0], [x1, y1, z1]) = (lo, hi);
        let sides = [
            (format!("({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})"), "[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
            (format!("({x0} {y0} {z0}) ({x1} {y0} {z0}) ({x1} {y1} {z0})"), "[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
            (format!("({x0} {y1} {z1}) ({x0} {y0} {z1}) ({x0} {y0} {z0})"), "[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
            (format!("({x1} {y1} {z0}) ({x1} {y0} {z0}) ({x1} {y0} {z1})"), "[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
            (format!("({x1} {y1} {z1}) ({x0} {y1} {z1}) ({x0} {y1} {z0})"), "[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
            (format!("({x1} {y0} {z0}) ({x0} {y0} {z0}) ({x0} {y0} {z1})"), "[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
        ];
        let mut out = format!(
            "solid {{ \"id\" \"{id}\"
"
        );
        for (k, (plane, u, v)) in sides.iter().enumerate() {
            out += &format!(
                "side {{ \"id\" \"{}\" \"plane\" \"{plane}\" \"material\" \"DEV/DEV_MEASUREGENERIC01B\" \"uaxis\" \"{u}\" \"vaxis\" \"{v}\" \"lightmapscale\" \"16\" }}
",
                side0 + k as u32
            );
        }
        out + "}
"
    }

    #[test]
    fn a_room_is_compiled_lit_and_bound_to_its_solids() {
        let walls = [
            ([-16.0, -16.0, -16.0], [272.0, 272.0, 0.0]),
            ([-16.0, -16.0, 256.0], [272.0, 272.0, 272.0]),
            ([-16.0, -16.0, 0.0], [0.0, 272.0, 256.0]),
            ([256.0, -16.0, 0.0], [272.0, 272.0, 256.0]),
            ([0.0, -16.0, 0.0], [256.0, 0.0, 256.0]),
            ([0.0, 256.0, 0.0], [256.0, 272.0, 256.0]),
        ];
        let mut text = String::from(
            "world { \"id\" \"1\" \"classname\" \"worldspawn\"
",
        );
        let mut solids = HashMap::new();
        for (i, (lo, hi)) in walls.iter().enumerate() {
            let id = 2 + i as u32;
            let side0 = if i < 3 { 10 + 6 * i as u32 } else { 65_536 + 10 + 6 * (i as u32 - 3) };
            text += &solid(id, side0, *lo, *hi);
            let info = SolidInfo {
                lo: glam::Vec3::from_array(lo.map(|v| v as f32)),
                hi: glam::Vec3::from_array(hi.map(|v| v as f32)),
                generation: u64::from(id),
            };
            solids.insert(id, info);
        }
        text += "}
";
        text +=
            "entity { \"id\" \"20\" \"classname\" \"light\" \"origin\" \"128 128 128\" \"_light\" \"255 255 255 300\" }
";
        text += "entity { \"id\" \"21\" \"classname\" \"info_player_start\" \"origin\" \"128 128 32\" }
";
        let map = hp_vmf::parse(&text).expect("parse");
        let job = Job {
            id: 7,
            serial: 1,
            map: Some(map),
            keep: false,
            solids: solids.clone(),
            fs: None,
            hdr: true,
            quality: RadQuality::Draft,
            bounces: 2,
            prop_shadows: true,
            prop_lighting: true,
            threads: (2, 2),
            layer_size: 256,
            max_layers: 4,
            coarsen: 1.0,
            lit: false,
            tuning: child::Tuning::default(),
            exe: None,
        };
        let wanted = AtomicU64::new(7);
        let (out, rx) = crossbeam_channel::unbounded();
        let wake = || {};
        let ctx = Ctx { id: 7, wanted: &wanted, out: &out, wake: &wake, start: Instant::now(), faces: 0, game: None };
        let cache = std::sync::Mutex::new(None);
        let solids_again = solids;
        let job = Job { keep: true, ..job };
        let baked = match run(job, &ctx, &cache) {
            Outcome::Baked(b) => b,
            Outcome::Cancelled => panic!("cancelled"),
            Outcome::Failed(e) => panic!("failed: {e}"),
        };
        assert!(cache.lock().unwrap().is_some(), "the compiled map waits for the refined pass");
        let refine = |serial: u64| Job {
            id: 7,
            serial,
            map: None,
            keep: false,
            solids: solids_again.clone(),
            fs: None,
            hdr: true,
            quality: RadQuality::Draft,
            bounces: 2,
            prop_shadows: true,
            prop_lighting: true,
            threads: (2, 2),
            layer_size: 256,
            max_layers: 4,
            coarsen: 1.0,
            lit: false,
            tuning: child::Tuning::default(),
            exe: None,
        };
        assert!(matches!(run(refine(2), &ctx, &cache), Outcome::Cancelled), "another serial cannot use it");
        let job = Job { keep: true, map: Some(hp_vmf::parse(&text).expect("parse")), ..refine(1) };
        assert!(matches!(run(job, &ctx, &cache), Outcome::Baked(_)));
        let Outcome::Baked(again) = run(refine(1), &ctx, &cache) else { panic!("the refined pass") };
        assert_eq!(again.faces.len(), baked.faces.len());
        assert!(cache.lock().unwrap().is_none(), "used once");
        assert_eq!(baked.unbound, 0, "every face found its solid");
        assert!(baked.faces.len() >= 6, "{} faces", baked.faces.len());
        assert!(baked.faces.iter().all(|f| (2..8).contains(&f.solid) && f.generation == u64::from(f.solid)));
        assert!(baked.faces.iter().all(|f| f.rect.is_some()));
        let walls_seen: HashSet<u32> = baked.faces.iter().map(|f| f.solid).collect();
        assert_eq!(walls_seen.len(), 6, "the inner face of every wall");
        let light: Vec<f32> = baked.texels.iter().map(|t| hp_light::pack::unpack_rg11b10(*t)[0]).collect();
        let max = light.iter().copied().fold(0.0f32, f32::max);
        assert!(max > 0.1 && max < 100.0, "{max}");
        assert!(light.iter().all(|v| v.is_finite()));
        let props = baked.props.as_ref().expect("model lighting");
        let lit = props.model_lighting([128.0, 128.0, 64.0]);
        assert!(lit.lights.iter().any(|l| l.origin == [128.0, 128.0, 128.0]), "{:?}", lit.lights);
        assert!(lit.ambient.iter().flatten().any(|v| *v > 0.0), "ambient cube {:?}", lit.ambient);
        let verts: Vec<_> = baked.blocks.iter().flat_map(|b| b.batches.iter().flat_map(|x| x.1.iter())).collect();
        assert!(!verts.is_empty());
        assert!(verts.iter().all(|v| v.pos.iter().all(|c| (-0.5..=256.5).contains(c))));
        assert!(verts.iter().all(|v| (v.face as usize) < baked.faces.len()));
        let phases: Vec<BakePhase> = rx
            .try_iter()
            .filter_map(|m| if let Msg::Progress { phase, .. } = m { Some(phase) } else { None })
            .collect();
        assert!(phases.contains(&BakePhase::Compiling) && phases.contains(&BakePhase::Direct), "{phases:?}");
        let volume = baked.indirect.as_ref().expect("the light probes of the bake");
        assert!(volume.cells() > 20 && volume.spacing >= 32.0, "{} cells of {}", volume.cells(), volume.spacing);

        let wanted = AtomicU64::new(9);
        let ctx = Ctx { id: 9, wanted: &wanted, out: &out, wake: &wake, start: Instant::now(), faces: 0, game: None };
        let map = hp_vmf::parse(&text).expect("parse");
        let lit_job = Job {
            id: 9,
            serial: 1,
            map: Some(map),
            keep: false,
            solids: HashMap::new(),
            fs: None,
            hdr: true,
            quality: RadQuality::Draft,
            bounces: 2,
            prop_shadows: true,
            prop_lighting: true,
            threads: (2, 2),
            layer_size: 256,
            max_layers: 4,
            coarsen: 1.0,
            lit: true,
            tuning: child::Tuning::default(),
            exe: None,
        };
        let Outcome::Baked(lit) = run(lit_job, &ctx, &Default::default()) else { panic!("the Lit mode bake failed") };
        assert!(lit.faces.is_empty() && lit.texels.is_empty() && lit.blocks.is_empty(), "no atlas for the Lit mode");
        let own = lit.indirect.as_ref().expect("the light probes");
        assert_eq!(own.cells(), volume.cells(), "the same probes as the full bake");

        let wanted = AtomicU64::new(0);
        let ctx = Ctx { id: 8, wanted: &wanted, out: &out, wake: &wake, start: Instant::now(), faces: 0, game: None };
        let map = hp_vmf::parse(&text).expect("parse");
        let job = Job {
            id: 8,
            serial: 1,
            map: Some(map),
            keep: false,
            solids: HashMap::new(),
            fs: None,
            hdr: false,
            quality: RadQuality::Draft,
            bounces: 0,
            prop_shadows: true,
            prop_lighting: true,
            threads: (1, 1),
            layer_size: 256,
            max_layers: 1,
            coarsen: 1.0,
            lit: false,
            tuning: child::Tuning::default(),
            exe: None,
        };
        let r = catch_unwind(AssertUnwindSafe(|| run(job, &ctx, &Default::default())));
        assert!(matches!(r, Err(_) | Ok(Outcome::Cancelled)));
    }

    #[test]
    fn baked_light_matches_the_lit_preview() {
        let walls = [
            ([-16.0, -16.0, -16.0], [528.0, 528.0, 0.0]),
            ([-16.0, -16.0, 512.0], [528.0, 528.0, 528.0]),
            ([-16.0, -16.0, 0.0], [0.0, 528.0, 512.0]),
            ([512.0, -16.0, 0.0], [528.0, 528.0, 512.0]),
            ([0.0, -16.0, 0.0], [512.0, 0.0, 512.0]),
            ([0.0, 512.0, 0.0], [512.0, 528.0, 512.0]),
        ];
        let room = |sky: bool| {
            let mut text = String::from(
                "world { \"id\" \"1\" \"classname\" \"worldspawn\"
",
            );
            for (i, (lo, hi)) in walls.iter().enumerate() {
                let s = solid(2 + i as u32, 10 + 6 * i as u32, *lo, *hi);
                text += &if sky && i == 1 { s.replace("DEV/DEV_MEASUREGENERIC01B", "TOOLS/TOOLSSKYBOX") } else { s };
            }
            text + "}
"
        };
        let point = room(false)
            + "entity { \"id\" \"20\" \"classname\" \"light\" \"origin\" \"256 256 100\" \"_light\" \"255 255 255 255\" \"_quadratic_attn\" \"1\" }
";
        let sun = room(true)
            + "entity { \"id\" \"20\" \"classname\" \"light_environment\" \"origin\" \"256 256 100\" \"_light\" \"255 255 255 255\" \"_ambient\" \"0 0 0 0\" \"pitch\" \"-90\" \"angles\" \"-90 0 0\" }
";
        for (text, hdr) in [(&point, true), (&point, false), (&sun, true), (&sun, false)] {
            let job = Job {
                id: 3,
                serial: 1,
                map: Some(hp_vmf::parse(text).expect("parse")),
                keep: false,
                solids: HashMap::new(),
                fs: None,
                hdr,
                quality: RadQuality::Draft,
                bounces: 0,
                prop_shadows: false,
                prop_lighting: false,
                threads: (2, 2),
                layer_size: 512,
                max_layers: 4,
                coarsen: 1.0,
                lit: false,
                tuning: child::Tuning::default(),
                exe: None,
            };
            let wanted = AtomicU64::new(3);
            let (out, _rx) = crossbeam_channel::unbounded();
            let wake = || {};
            let ctx =
                Ctx { id: 3, wanted: &wanted, out: &out, wake: &wake, start: Instant::now(), faces: 0, game: None };
            let baked = match run(job, &ctx, &Default::default()) {
                Outcome::Baked(b) => b,
                Outcome::Cancelled => panic!("cancelled"),
                Outcome::Failed(e) => panic!("failed: {e}"),
            };
            let max = baked.texels.iter().map(|t| hp_light::pack::unpack_rg11b10(*t)[0]).fold(0.0f32, f32::max);
            let what = if std::ptr::eq(text, &sun) { "sun" } else { "point light" };
            assert!((max - 1.0).abs() < 0.12, "{what}, hdr {hdr}: the floor at {max}, the Lit preview at 1");
        }
    }

    #[test]
    fn bumped_faces_bring_their_bumped_lightmaps_and_blends_their_weights() {
        let dir = std::env::temp_dir().join(format!("hp-light-bump-{}", std::process::id()));
        let mats = dir.join("materials/test");
        std::fs::create_dir_all(&mats).expect("dir");
        std::fs::write(
            mats.join("bumpy.vmt"),
            r#"LightmappedGeneric { "$basetexture" "test/bumpy" "$bumpmap" "test/bumpy_normal" }"#,
        )
        .expect("vmt");
        std::fs::write(
            mats.join("blend4.vmt"),
            r#"Lightmapped_4WayBlend { "$basetexture" "test/a" "$basetexture2" "test/b" "$basetexture3" "test/c"
               "$basetexture4" "test/d" "$bumpmap" "test/a_normal" "$bumpmap2" "test/b_normal" }"#,
        )
        .expect("vmt");
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir).expect("game folder");
        let walls = [
            ([-16.0, -16.0, -16.0], [272.0, 272.0, 0.0]),
            ([-16.0, -16.0, 256.0], [272.0, 272.0, 272.0]),
            ([-16.0, -16.0, 0.0], [0.0, 272.0, 256.0]),
            ([256.0, -16.0, 0.0], [272.0, 272.0, 256.0]),
            ([0.0, -16.0, 0.0], [256.0, 0.0, 256.0]),
            ([0.0, 256.0, 0.0], [256.0, 272.0, 256.0]),
            ([64.0, 64.0, 0.0], [192.0, 192.0, 8.0]),
        ];
        let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
        let mut solids = HashMap::new();
        for (i, (lo, hi)) in walls.iter().enumerate() {
            let id = 2 + i as u32;
            let mut s = solid(id, 10 + 6 * i as u32, *lo, *hi);
            if i == 0 {
                s = s.replace("DEV/DEV_MEASUREGENERIC01B", "TEST/BUMPY");
            }
            if i == 6 {
                s = s.replace("DEV/DEV_MEASUREGENERIC01B", "TEST/BLEND4").replacen(
                    "\"lightmapscale\" \"16\" }",
                    "\"lightmapscale\" \"16\" dispinfo { \"power\" \"2\" \"startposition\" \"[64 64 8]\" } }",
                    1,
                );
            }
            text += &s;
            let info = SolidInfo {
                lo: glam::Vec3::from_array(lo.map(|v| v as f32)),
                hi: glam::Vec3::from_array(hi.map(|v| v as f32)),
                generation: u64::from(id),
            };
            solids.insert(id, info);
        }
        text += "}\n";
        text += "entity { \"id\" \"30\" \"classname\" \"light\" \"origin\" \"128 40 128\" \"_light\" \"255 255 255 300\" }\n";
        text += "entity { \"id\" \"31\" \"classname\" \"info_player_start\" \"origin\" \"128 128 32\" }\n";
        let mut map = hp_vmf::parse(&text).expect("parse");
        let disp_solid = map.world.solids.iter_mut().find(|s| s.id == 8).expect("block");
        let disp = disp_solid.sides[0].disp.as_deref_mut().expect("displacement");
        let mut mb = hp_vmf::multiblend::MultiBlend::new(25);
        for (i, w) in mb.weights.iter_mut().enumerate() {
            let t = i as f64 / 24.0;
            *w = [1.0 - t, t, 0.0, 0.0];
        }
        assert!(mb.write_to(disp));
        let editor = hp_editor::Document::from_map(map.clone());
        let positions: Vec<glam::DVec3> =
            editor.scene().solid(8).and_then(|n| n.disp_mesh(0)).expect("editor displacement").positions.clone();
        let job = Job {
            id: 9,
            serial: 1,
            map: Some(map),
            keep: false,
            solids,
            fs: Some(fs),
            hdr: true,
            quality: RadQuality::Draft,
            bounces: 1,
            prop_shadows: false,
            prop_lighting: false,
            threads: (2, 2),
            layer_size: 512,
            max_layers: 4,
            coarsen: 1.0,
            lit: false,
            tuning: child::Tuning::default(),
            exe: None,
        };
        let wanted = AtomicU64::new(9);
        let (out, _rx) = crossbeam_channel::unbounded();
        let wake = || {};
        let ctx = Ctx { id: 9, wanted: &wanted, out: &out, wake: &wake, start: Instant::now(), faces: 0, game: None };
        let baked = match run(job, &ctx, &Default::default()) {
            Outcome::Baked(b) => b,
            Outcome::Cancelled => panic!("cancelled"),
            Outcome::Failed(e) => panic!("failed: {e}"),
        };
        let _ = std::fs::remove_dir_all(&dir);
        let of = |solid: u32| baked.faces.iter().filter(move |f| f.solid == solid);
        for solid in [2, 8] {
            let f = of(solid).find(|f| f.rect.is_some()).expect("lit face");
            assert_eq!(f.images, 4, "solid {solid}");
            let r = f.rect.expect("rect");
            assert_eq!(r.w % 4, 0);
            let n = (r.w * r.h) as usize;
            let texels = &baked.texels[f.texels as usize..f.texels as usize + n];
            let lum = |k: usize| -> f32 {
                let per = n / 4;
                texels[k * per..(k + 1) * per].iter().map(|t| hp_light::pack::unpack_rg11b10(*t)[0]).sum::<f32>()
            };
            assert!((1..4).all(|k| lum(k) > 0.0), "lit along every bump direction");
            let average = (lum(1) + lum(2) + lum(3)) / 3.0;
            assert!(
                (average - lum(0)).abs() <= 0.03 * lum(0),
                "bump lightmaps scaled to the flat one like the compiled view: {average} vs {}",
                lum(0)
            );
        }
        assert!(of(4).all(|f| f.images == 1), "plain walls keep one lightmap");
        let face = baked.faces.iter().position(|f| f.solid == 8 && f.flags & face_flags::DISP != 0).expect("disp face");
        let verts: Vec<_> = baked
            .blocks
            .iter()
            .flat_map(|b| b.batches.iter().flat_map(|x| x.1.iter()))
            .filter(|v| v.face as usize == face)
            .collect();
        assert_eq!(verts.len(), 25);
        for v in verts {
            let p = glam::DVec3::new(f64::from(v.pos[0]), f64::from(v.pos[1]), f64::from(v.pos[2]));
            let i = positions.iter().position(|q| q.distance(p) < 0.1).expect("editor vertex");
            let t = ((i as f64 / 24.0) * 1e6).round() as f32 / 1e6;
            let want = hp_render::lightmap::pack_blend([1.0 - t, t, 0.0, 0.0]);
            assert_eq!(v.blend, want, "vertex {i} at {p}");
        }
    }

    #[test]
    fn the_atlas_is_as_small_as_the_lightmaps() {
        assert_eq!(atlas_size(&[], 2048), MIN_ATLAS);
        assert_eq!(atlas_size(&[(16, 16); 4], 2048), MIN_ATLAS);
        assert_eq!(atlas_size(&[(300, 20)], 2048), 512, "as wide as the widest face");
        assert_eq!(atlas_size(&[(32, 32); 2000], 2048), 2048);
        assert_eq!(atlas_size(&[(32, 32); 100_000], 2048), 2048, "more layers past the largest size");
        let sizes = [(40, 30); 700];
        let mut atlas = Atlas::new(atlas_size(&sizes, 2048), 64);
        assert!(atlas.alloc_many(&sizes).iter().all(Option::is_some));
        assert!(atlas.layers() <= 2, "{} layers of {}", atlas.layers(), atlas.size());
    }

    #[test]
    fn hidden_objects_are_left_out_of_the_compiled_copy() {
        let text = r#"world { "id" "1" "classname" "worldspawn"
  solid { "id" "2" side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "A" } }
  solid { "id" "3" side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "A" } }
}
entity { "id" "5" "classname" "light" "origin" "0 0 0" }
entity { "id" "6" "classname" "light" "origin" "9 9 9" }"#;
        let mut map = hp_vmf::parse(text).expect("parse");
        map.world.solids[1].hidden = true;
        map.entities[1].hidden = true;
        map.visgroups.push(Default::default());
        strip_hidden(&mut map);
        let ids: Vec<u32> = map.world.solids.iter().map(|s| s.id).collect();
        assert_eq!(ids, [2]);
        assert_eq!(map.entities.iter().map(|e| e.id).collect::<Vec<_>>(), [5]);
        assert!(map.visgroups.is_empty());
    }

    #[test]
    fn coarser_lightmaps() {
        let text = r#"world { "id" "1" "classname" "worldspawn"
  solid { "id" "2" side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "A" "lightmapscale" "16" } side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "A" "lightmapscale" "0" } }
}"#;
        let mut map = hp_vmf::parse(text).expect("parse");
        coarsen(&mut map, 1.5);
        let scales: Vec<i32> = map.world.solids[0].sides.iter().map(|s| s.lightmap_scale).collect();
        assert_eq!(scales, [24, 24]);
    }
}
