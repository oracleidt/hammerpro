use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use crate::assets::{self, AssetReport};
use crate::launch::{self, GmodInstall, LogTail};
use crate::procmem::MemSource;
use crate::query;
use crate::recover::{self, Evaluated, FileAccess};
use crate::report::{MemoryRecovery, SniffReport, coverage_rows, vmf_counts};
use crate::scan::{EntityHit, ScanOptions, find_entity_lumps};
use crate::settings::{GeometrySource, NameContext, SnifferSettings, output_name, unique_path};

pub const MAX_LOG_LINES: usize = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Preflight,
    Querying,
    Launching,
    WaitingGame,
    WaitingMap,
    Capturing,
    Verifying,
    Rebuilding,
    Decompiling,
    Extracting,
    Finishing,
    Done,
    Failed,
    Cancelled,
}

impl Phase {
    pub fn id(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Preflight => "preflight",
            Phase::Querying => "querying",
            Phase::Launching => "launching",
            Phase::WaitingGame => "waiting_game",
            Phase::WaitingMap => "waiting_map",
            Phase::Capturing => "capturing",
            Phase::Verifying => "verifying",
            Phase::Rebuilding => "rebuilding",
            Phase::Decompiling => "decompiling",
            Phase::Extracting => "extracting",
            Phase::Finishing => "finishing",
            Phase::Done => "done",
            Phase::Failed => "failed",
            Phase::Cancelled => "cancelled",
        }
    }

    pub fn is_final(self) -> bool {
        matches!(self, Phase::Done | Phase::Failed | Phase::Cancelled)
    }

    pub fn is_running(self) -> bool {
        !matches!(self, Phase::Idle) && !self.is_final()
    }
}

#[derive(Clone, Debug)]
pub struct Status {
    pub phase: Phase,
    pub detail: String,
    pub progress: f32,
    pub elapsed: Duration,
    pub game_pid: Option<u32>,
    pub map: Option<String>,
    pub error: Option<String>,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            phase: Phase::Idle,
            detail: String::new(),
            progress: 0.0,
            elapsed: Duration::ZERO,
            game_pid: None,
            map: None,
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub seconds: f32,
    pub level: Level,
    pub text: String,
}

pub struct Shared {
    status: Mutex<Status>,
    log: Mutex<Vec<LogLine>>,
    dropped: Mutex<usize>,
    outcome: Mutex<Option<Result<Arc<Outcome>, String>>>,
    started: Mutex<Option<Instant>>,
    repaint: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            status: Mutex::new(Status::default()),
            log: Mutex::new(Vec::new()),
            dropped: Mutex::new(0),
            outcome: Mutex::new(None),
            started: Mutex::new(None),
            repaint: Mutex::new(None),
        }
    }

    fn poke(&self) {
        if let Some(f) = self.repaint.lock().as_ref() {
            f();
        }
    }

    fn phase(&self, phase: Phase, progress: f32, detail: impl Into<String>) {
        let detail = detail.into();
        {
            let mut s = self.status.lock();
            s.phase = phase;
            s.progress = progress.clamp(0.0, 1.0);
            s.detail = detail.clone();
        }
        self.say(Level::Info, format!("[{}] {detail}", phase.id()));
    }

    fn detail(&self, progress: f32, detail: impl Into<String>) {
        let mut s = self.status.lock();
        s.progress = progress.clamp(0.0, 1.0);
        s.detail = detail.into();
        drop(s);
        self.poke();
    }

    fn say(&self, level: Level, text: impl Into<String>) {
        let seconds = self.started.lock().map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
        let mut log = self.log.lock();
        if log.len() >= MAX_LOG_LINES {
            let cut = MAX_LOG_LINES / 4;
            log.drain(..cut);
            *self.dropped.lock() += cut;
        }
        log.push(LogLine { seconds, level, text: text.into() });
        drop(log);
        self.poke();
    }

    fn finish(&self, phase: Phase, error: Option<String>) {
        let mut s = self.status.lock();
        s.phase = phase;
        s.error = error;
        if phase == Phase::Done {
            s.progress = 1.0;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub vmf_path: PathBuf,
    pub bsp_path: Option<PathBuf>,
    pub content_dir: Option<PathBuf>,
    pub report_path: PathBuf,
    pub report: SniffReport,
}

pub struct SnifferHandle {
    shared: Arc<Shared>,
    cancel: Arc<AtomicBool>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl SnifferHandle {
    pub fn start(
        settings: SnifferSettings,
        repaint: Option<Box<dyn Fn() + Send + Sync>>,
    ) -> Result<SnifferHandle, String> {
        if !cfg!(windows) {
            return Err("the BSP Sniffer works on Windows only".to_owned());
        }
        let settings = settings.clamped();
        if !settings.authorized {
            return Err(
                "tick \"I am authorized\" first: the sniffer only works on servers and maps you may test".to_owned()
            );
        }
        settings.parsed_address()?;
        settings.validate()?;
        let shared = Arc::new(Shared::new());
        *shared.repaint.lock() = repaint;
        *shared.started.lock() = Some(Instant::now());
        shared.status.lock().phase = Phase::Preflight;
        let cancel = Arc::new(AtomicBool::new(false));
        let (s2, c2) = (Arc::clone(&shared), Arc::clone(&cancel));
        let thread = std::thread::Builder::new()
            .name("hp-sniffer".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&s2, &c2, &settings)));
                let result =
                    result.unwrap_or_else(|_| Err(Failure::Error("the sniffer stopped unexpectedly".to_owned())));
                match result {
                    Ok(outcome) => {
                        *s2.outcome.lock() = Some(Ok(Arc::new(outcome)));
                        s2.finish(Phase::Done, None);
                        s2.say(Level::Info, "done");
                    }
                    Err(Failure::Cancelled) => {
                        *s2.outcome.lock() = Some(Err("cancelled".to_owned()));
                        s2.finish(Phase::Cancelled, None);
                        s2.say(Level::Warn, "cancelled");
                    }
                    Err(Failure::Error(e)) => {
                        s2.say(Level::Error, e.clone());
                        *s2.outcome.lock() = Some(Err(e.clone()));
                        s2.finish(Phase::Failed, Some(e));
                    }
                }
                s2.poke();
            })
            .map_err(|e| e.to_string())?;
        Ok(SnifferHandle { shared, cancel, thread: Mutex::new(Some(thread)) })
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.shared.say(Level::Warn, "cancel requested");
    }

    pub fn status(&self) -> Status {
        let mut s = self.shared.status.lock().clone();
        if !s.phase.is_final()
            && let Some(t) = *self.shared.started.lock()
        {
            s.elapsed = t.elapsed();
        }
        s
    }

    pub fn log_since(&self, from: usize) -> (Vec<LogLine>, usize) {
        let log = self.shared.log.lock();
        let dropped = *self.shared.dropped.lock();
        let start = from.saturating_sub(dropped).min(log.len());
        (log[start..].to_vec(), dropped + log.len())
    }

    pub fn outcome(&self) -> Option<Result<Arc<Outcome>, String>> {
        self.shared.outcome.lock().clone()
    }

    pub fn is_finished(&self) -> bool {
        self.shared.status.lock().phase.is_final()
    }

    pub fn join(&self) {
        if let Some(t) = self.thread.lock().take() {
            let _ = t.join();
        }
    }
}

impl std::fmt::Debug for SnifferHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnifferHandle").field("phase", &self.shared.status.lock().phase).finish()
    }
}

impl Drop for SnifferHandle {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let Some(thread) = self.thread.lock().take() else { return };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
    }
}

pub enum Failure {
    Cancelled,
    Error(String),
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Error(e)
    }
}

pub type Run<T> = Result<T, Failure>;

fn check(cancel: &AtomicBool) -> Run<()> {
    if cancel.load(Ordering::Relaxed) { Err(Failure::Cancelled) } else { Ok(()) }
}

fn sleep_checked(cancel: &AtomicBool, total: Duration) -> Run<()> {
    let step = Duration::from_millis(200);
    let mut left = total;
    while left > Duration::ZERO {
        check(cancel)?;
        let d = left.min(step);
        std::thread::sleep(d);
        left = left.saturating_sub(d);
    }
    check(cancel)
}

pub fn text_hash(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

pub fn pick_hit<'a>(hits: &'a [EntityHit], baseline: &HashSet<u64>, file_texts: &[String]) -> Option<&'a EntityHit> {
    let parsed: Vec<Vec<hp_bsp::Entity>> = file_texts.iter().map(|t| hp_bsp::parse_entities(t)).collect();
    if !parsed.is_empty() {
        for h in hits {
            let mine = hp_bsp::parse_entities(&h.text);
            if parsed.contains(&mine) {
                return Some(h);
            }
        }
    }
    hits.iter().find(|h| !baseline.contains(&text_hash(&h.text)))
}

struct GameGuard<'a> {
    install: GmodInstall,
    pid: Option<u32>,
    launched: bool,
    kill: bool,
    shared: &'a Shared,
    pre: HashSet<u32>,
}

impl GameGuard<'_> {
    fn shutdown(&mut self) {
        if !self.launched || !self.kill {
            return;
        }
        let Some(pid) = self.pid.take() else { return };
        self.shared.say(Level::Info, format!("closing Garry's Mod (process {pid})"));
        let ok = launch::kill_game(pid);
        for extra in launch::game_pids_for(&self.install) {
            if !self.pre.contains(&extra) {
                launch::kill_game(extra);
            }
        }
        if ok {
            self.shared.say(Level::Info, "Garry's Mod closed");
        } else {
            self.shared.say(Level::Warn, "Garry's Mod could not be closed");
        }
    }
}

impl Drop for GameGuard<'_> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn lap(timings: &mut Vec<(String, f64)>, name: &str, t: &mut Instant) {
    timings.push((name.to_owned(), t.elapsed().as_secs_f64()));
    *t = Instant::now();
}

pub struct CaptureInput<'a> {
    pub game_dir: &'a Path,
    pub map_hint: Option<String>,
    pub settings: &'a SnifferSettings,
    pub access: &'a FileAccess,
    pub rescan: Option<Duration>,
    pub rescan_limit: Duration,
}

const RENDER_WAIT: Duration = Duration::from_secs(150);

pub struct Captured {
    pub bsp: Option<hp_bsp::Bsp>,
    pub pak_bsp: Option<hp_bsp::Bsp>,
    pub entity_text: String,
    pub hit_addr: u64,
    pub report: SniffReport,
}

fn memory_report(world: &crate::memmap::MemoryWorld) -> MemoryRecovery {
    let mut m = MemoryRecovery { found: true, ..MemoryRecovery::default() };
    if let Some(r) = &world.render {
        m.render_found = true;
        m.faces = r.faces.len();
        m.texinfo = r.texinfo.len();
        m.vertexes = r.vertexes.len();
        m.faces_address = r.faces_addr;
        m.texinfo_address = r.texinfo_addr;
    }
    let x = &world.extras;
    m.displacements = x.displacements.len();
    m.overlays = x.overlays.len();
    m.props = x.props.len();
    m.prop_models = x.props.iter().map(|p| p.model.to_ascii_lowercase()).collect::<HashSet<_>>().len();
    m.cubemaps = x.cubemaps.len();
    if let Some(ap) = &x.area_portals {
        m.area_portals = ap.count();
        m.area_portal_verts = ap.verts.len();
    }
    m.occluders = x.occlusion.as_ref().map_or(0, |o| o.occluders.len());
    m
}

const EXTRAS_STABLE_SCANS: usize = 2;
const EXTRAS_EMPTY_SCANS: usize = 6;
const EXTRAS_GIVE_UP_SCANS: usize = 8;
const REACQUIRE_MAX: usize = 3;
const STALE_AFTER: Duration = Duration::from_secs(25);

fn extras_signature(x: &crate::engine::MemoryExtras) -> [usize; 5] {
    [
        x.displacements.len(),
        x.props.len(),
        x.overlays.len(),
        x.cubemaps.len(),
        x.area_portals.as_ref().map_or(0, |a| a.count()),
    ]
}

pub fn extras_score(x: &crate::engine::MemoryExtras) -> usize {
    x.displacements.len()
        + x.overlays.iter().filter(|o| !o.faces.is_empty()).count()
        + x.props.len()
        + x.cubemaps.len()
        + x.area_portals.as_ref().map_or(0, |a| a.count())
}

pub fn extras_complete(x: &crate::engine::MemoryExtras, displacements: usize) -> bool {
    x.overlays.len() >= x.overlays_expected
        && x.overlays.iter().all(|o| !o.faces.is_empty())
        && x.props.len() * 100 >= x.props_expected * 99
        && x.displacements.len() >= displacements
}

pub fn extras_looks_stale(best: &crate::engine::MemoryExtras, fresh: &crate::engine::MemoryExtras) -> bool {
    let (b, f) = (extras_score(best), extras_score(fresh));
    b > 0 && f * 4 < b
}

fn settle_extras<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    world: &mut crate::memmap::MemoryWorld,
    map: &str,
    timing: (Duration, Duration),
    shared: Option<&Shared>,
    cancel: &AtomicBool,
) -> Run<()> {
    let (poll, limit) = timing;
    let started = Instant::now();
    let mut reacquired = 0usize;
    let mut last_say = Instant::now();
    let say = |level: Level, text: String| {
        if let Some(sh) = shared {
            sh.say(level, text);
        }
    };
    loop {
        let Some(render) = world.render.clone() else { return Ok(()) };
        let mut faces = render.faces.clone();
        let expected = faces.iter().filter(|f| f.has_disp).count();
        let mut best: Option<crate::engine::MemoryExtras> = None;
        let mut stale_since: Option<Instant> = None;
        let mut reacquire = false;
        loop {
            check(cancel)?;
            let found = world.displacements_now(src, &mut faces);
            if found >= expected {
                break;
            }
            let waiting = *stale_since.get_or_insert_with(Instant::now);
            if found == 0 && expected > 0 && waiting.elapsed() > STALE_AFTER && reacquired < REACQUIRE_MAX {
                reacquire = true;
                break;
            }
            if started.elapsed() > limit {
                say(Level::Warn, format!("only {found} of {expected} displacements were built by the engine in time"));
                break;
            }
            if let Some(sh) = shared {
                let text =
                    format!("waiting for the engine to finish loading the map: displacements {found} of {expected}");
                if last_say.elapsed() >= Duration::from_secs(30) {
                    sh.say(Level::Info, text.clone());
                    last_say = Instant::now();
                }
                sh.detail(0.46, text);
            }
            sleep_checked(cancel, poll)?;
        }
        let mut prev: Option<[usize; 5]> = None;
        let mut stable = 0usize;
        loop {
            if reacquire {
                break;
            }
            check(cancel)?;
            let Some(fresh) = world.extras_now(src, opts, map) else { return Ok(()) };
            let sig = extras_signature(&fresh);
            say(
                Level::Info,
                format!(
                    "scan {:.0} s after the map: displacements {}/{}, overlays {}/{} (with faces {}), static props {}/{}, cubemaps {}",
                    started.elapsed().as_secs_f32(),
                    fresh.displacements.len(),
                    expected,
                    fresh.overlays.len(),
                    fresh.overlays_expected,
                    fresh.overlays.iter().filter(|o| !o.faces.is_empty()).count(),
                    fresh.props.len(),
                    fresh.props_expected,
                    fresh.cubemaps.len()
                ),
            );
            if let Some(b) = &best
                && extras_looks_stale(b, &fresh)
                && reacquired < REACQUIRE_MAX
            {
                break;
            }
            let keep = best.as_ref().is_none_or(|b| extras_score(&fresh) >= extras_score(b));
            if keep {
                best = Some(fresh);
            }
            stable = if prev == Some(sig) { stable + 1 } else { 0 };
            let empty = best.as_ref().is_none_or(|b| b.props.is_empty());
            let needed = if empty { EXTRAS_EMPTY_SCANS } else { EXTRAS_STABLE_SCANS };
            let good = best.as_ref().is_some_and(|b| extras_complete(b, expected));
            if (stable >= needed && good) || stable >= EXTRAS_GIVE_UP_SCANS || started.elapsed() > limit {
                if !good {
                    say(
                        Level::Warn,
                        "the engine had not finished building every structure: some may be incomplete".to_owned(),
                    );
                }
                if let Some(b) = best {
                    world.extras = b;
                }
                return Ok(());
            }
            prev = Some(sig);
            if let Some(sh) = shared {
                sh.detail(
                    0.48,
                    format!(
                        "checking that the map stays identical in memory ({} of {} scans)",
                        stable + 1,
                        EXTRAS_STABLE_SCANS + 1
                    ),
                );
            }
            sleep_checked(cancel, poll)?;
        }
        reacquired += 1;
        say(
            Level::Warn,
            format!("the map moved in memory while it was read: reading it again ({reacquired} of {REACQUIRE_MAX})"),
        );
        match crate::memmap::find_all(src, opts, map) {
            Some(fresh) if fresh.render.is_some() => *world = fresh,
            _ => {
                say(Level::Warn, "the map could not be found again in the game memory".to_owned());
                return Ok(());
            }
        }
        if started.elapsed() > limit {
            return Ok(());
        }
        sleep_checked(cancel, poll)?;
    }
}

pub fn capture_from<S: MemSource + ?Sized>(
    src: &S,
    hit: &EntityHit,
    input: &CaptureInput<'_>,
    scratch: &Path,
    shared: Option<&Shared>,
    cancel: &AtomicBool,
) -> Run<Captured> {
    let say = |level: Level, t: String| {
        if let Some(s) = shared {
            s.say(level, t);
        }
    };
    input.settings.validate()?;
    let mut report = SniffReport {
        memory_entities: hit.entities,
        memory_entity_bytes: hit.text.len(),
        memory_entity_address: Some(hit.addr),
        map_file_mode: input.settings.map_file_mode().id().to_owned(),
        ..SniffReport::default()
    };
    let opts = ScanOptions::default();
    let mut client_capture = Some(crate::clientents::ClientCapture::start(src, input.map_hint.as_deref()));
    let map_name = input
        .map_hint
        .clone()
        .or_else(|| {
            hp_bsp::parse_entities(&hit.text)
                .first()
                .and_then(|w| w.get("message").map(str::to_owned))
                .and_then(|m| recover::safe_map_name(&m))
        })
        .unwrap_or_default();
    let candidates = recover::find_candidates(input.game_dir, &map_name, scratch, input.access);
    if input.access.is_allowed() {
        say(Level::Info, format!("{} candidate map files for {map_name} (optional cross check)", candidates.len()));
    } else {
        say(Level::Info, "map files are never read (setting): everything comes from the game memory".to_owned());
    }
    let mut evaluated: Vec<(Evaluated, Option<hp_bsp::Bsp>)> = Vec::new();
    for c in &candidates {
        check(cancel)?;
        let t0 = Instant::now();
        let (ev, bsp) = recover::evaluate(src, c, Some(&hit.text), &opts, input.access);
        say(Level::Info, format!("  checked {} in {:.1} s", c.path.display(), t0.elapsed().as_secs_f32()));
        say(
            Level::Info,
            format!(
                "  {} ({}): entities identical {}, lumps verified in memory {} ({} bytes){}",
                c.path.display(),
                c.origin,
                ev.entities_identical,
                ev.verified_lumps,
                ev.verified_bytes,
                ev.rejected.as_ref().map(|r| format!(", rejected: {r}")).unwrap_or_default()
            ),
        );
        evaluated.push((ev, bsp));
    }
    evaluated.retain(|(_, b)| b.is_some());
    evaluated.sort_by_key(|(e, _)| std::cmp::Reverse(recover::ranking(e)));
    let usable = evaluated
        .into_iter()
        .find(|(e, _)| e.entities_identical || e.verified_lumps > 0)
        .map(|(e, b)| (e, b.expect("kept only parsed files")));
    let mode = input.settings.geometry;
    let mut full = if mode == GeometrySource::File {
        None
    } else {
        let t0 = Instant::now();
        let f = crate::memmap::find_all(src, &opts, &map_name);
        say(Level::Info, format!("world scan took {:.1} s", t0.elapsed().as_secs_f32()));
        if f.is_none() {
            say(Level::Warn, "the collision data was not found in the game memory".to_owned());
        }
        f
    };
    if let (Some(world), Some(poll)) = (full.as_mut(), input.rescan)
        && world.render.is_none()
    {
        let limit = Instant::now() + RENDER_WAIT.min(input.rescan_limit);
        let mut announced = false;
        while world.render.is_none() && Instant::now() < limit {
            check(cancel)?;
            if !announced {
                say(
                    Level::Info,
                    "the collision data is loaded but the render data is not yet: waiting for the engine".to_owned(),
                );
                announced = true;
            }
            sleep_checked(cancel, poll)?;
            if let Some(f) = crate::memmap::find_all(src, &opts, &map_name)
                && f.render.is_some()
            {
                *world = f;
            }
        }
    }
    if let (Some(world), Some(poll)) = (full.as_mut(), input.rescan)
        && world.render.is_some()
    {
        settle_extras(src, &opts, world, &map_name, (poll, input.rescan_limit), shared, cancel)?;
    }
    if let Some(world) = full.as_mut()
        && world.hub.is_some()
    {
        world.refresh_hub(src, true);
    }
    let merged_hit;
    let hit = match client_capture.take() {
        Some(c) => {
            let m = c.finish(src, &hit.text);
            let r = &m.report;
            if let Some(e) = &r.error {
                say(Level::Warn, format!("client entities: {e}"));
            } else {
                say(
                    Level::Info,
                    format!(
                        "client entities: {} networked, {} map created, {} matched to the entity lump, {} added, {} parent names restored",
                        r.networked, r.map_created, r.matched, r.added, r.parents_restored
                    ),
                );
            }
            report.client = m.report;
            merged_hit = EntityHit { text: m.text, ..hit.clone() };
            &merged_hit
        }
        None => hit,
    };
    let mut file_bsp: Option<hp_bsp::Bsp> = None;
    if let Some((ev, bsp)) = usable {
        report.source = format!("{} ({})", ev.candidate.path.display(), ev.candidate.origin);
        report.file_entities = ev.file_entities;
        report.entities_identical = ev.entities_identical;
        report.coverage = coverage_rows(&ev.coverage);
        report.verified_lumps = ev.verified_lumps;
        report.verified_bytes = ev.verified_bytes;
        if !ev.entities_identical {
            report.notes.push(
                "the entity lump of the file differs from the one in the game memory: the memory copy is used"
                    .to_owned(),
            );
        }
        let (bsp, replaced) = recover::assemble(bsp, Some(&hit.text)).map_err(|e| e.to_string())?;
        report.entities_replaced = replaced;
        report.bsp_counts = recover::map_counts(&bsp);
        file_bsp = Some(bsp);
    }
    let mut cm_identical = false;
    if let Some(world) = &full {
        let mc = &world.cm;
        let cmp = match &file_bsp {
            Some(b) => crate::cm::compare_with_bsp(&mc.cm, b),
            None => crate::cm::CmComparison::default(),
        };
        cm_identical = cmp.identical();
        say(
            Level::Info,
            format!(
                "collision data in memory: brushes {}, sides {}, planes {}, leafs {}, nodes {}, models {}{}",
                mc.cm.brushes.len(),
                mc.cm.total_sides(),
                mc.cm.planes,
                mc.leafs.len(),
                mc.nodes.len(),
                mc.models.len(),
                if file_bsp.is_some() {
                    format!(
                        "; compared with the file: {} brushes different, {} sides with another plane, {} brushes with other materials, {} boxes different",
                        cmp.brush_mismatch, cmp.side_plane_mismatch, cmp.material_mismatch_brushes, cmp.box_mismatch
                    )
                } else {
                    String::new()
                }
            ),
        );
        report.cm = crate::report::cm_report(&mc.cm, cmp);
        report.memory = memory_report(world);
        let m = &report.memory;
        say(
            Level::Info,
            format!(
                "render data in memory: faces {}, texinfo {}, vertexes {}, displacements {}, overlays {}, static props {} ({} models), cubemaps {}, area portals {}",
                m.faces,
                m.texinfo,
                m.vertexes,
                m.displacements,
                m.overlays,
                m.props,
                m.prop_models,
                m.cubemaps,
                m.area_portals
            ),
        );
        if !m.render_found {
            say(
                Level::Warn,
                "the render data (faces, texture axes, displacements, overlays) was not found in the game memory"
                    .to_owned(),
            );
        }
    }
    let memory_complete = full.as_ref().is_some_and(|w| w.render.is_some());
    let use_memory = match (mode, &full, &file_bsp) {
        (GeometrySource::File, _, _) => false,
        (_, None, _) => false,
        (GeometrySource::Memory, Some(_), _) => true,
        (GeometrySource::Auto, Some(_), None) => true,
        (GeometrySource::Auto, Some(_), Some(_)) => memory_complete || !cm_identical,
    };
    if use_memory {
        let world = full.as_mut().expect("checked above");
        crate::merge::note_memory(world, &mut report.merge);
        if let Some(f) = &file_bsp {
            report.merge = crate::merge::merge(world, f);
            let m = &report.merge;
            if m.leafs_dropped > 0 {
                say(
                    Level::Info,
                    format!(
                        "{} leaf records in front of the leaf list of the game were not leaves: dropped, the leaf numbering now matches the map file",
                        m.leafs_dropped
                    ),
                );
            }
            say(
                Level::Info,
                format!(
                    "filled from the map file: displacement data {} of {}, static props {} of {}, cubemap sizes {} of {}, overlay orders {} of {}, side texture axes {} of {}, pakfile files {}",
                    m.displacements_from_file,
                    m.displacements,
                    m.props_from_file,
                    m.props,
                    m.cubemaps_from_file,
                    m.cubemaps,
                    m.overlays_from_file,
                    m.overlays,
                    m.sides_from_file,
                    m.sides,
                    m.pak_files
                ),
            );
        }
        crate::memmap::resolve_material_ranks(world);
        report.hub = match &world.hub {
            Some(h) => h.report(),
            None => crate::hubdata::HubReport::default(),
        };
        let mut mem_bsp = crate::memmap::build_bsp(world, &hit.text)?;
        if let Some(f) = &file_bsp {
            let (filled, pass) = crate::passthrough::apply(&mem_bsp, f);
            if let Some(b) = filled {
                mem_bsp = b;
            }
            if !pass.names().is_empty() {
                say(
                    Level::Info,
                    format!(
                        "taken from the map file as they are, because what they index is identical: {} ({} bytes)",
                        pass.names().join(", "),
                        pass.bytes()
                    ),
                );
            }
            report.passthrough = pass;
        }
        report.unprotect.signatures = crate::matguess::collect_signatures(src, world);
        let mem_counts = recover::map_counts(&mem_bsp);
        if let Some(f) = &file_bsp {
            report.cross_check = recover::cross_check(&mem_counts, &recover::map_counts(f));
            let different = report.cross_check.iter().filter(|r| r.memory != r.file).count();
            say(
                Level::Info,
                format!(
                    "cross check with the map file: {} counts compared, {} different",
                    report.cross_check.len(),
                    different
                ),
            );
        }
        report.bsp_counts = mem_counts;
        report.memory_only = file_bsp.is_none();
        report.geometry = "game memory".to_owned();
        report.notes.push(
            "the game memory is the main source: brushes and materials from the collision data, faces and texture axes from the render data, displacements with their vertex alphas and allowed vertices, static props with their fade distances, overlays with their render order, cubemaps with their sizes, occluders and area portals from the engine structures. Taken from a matching and consistent map file when one is allowed, and left out otherwise: 4 of the 8 static prop flag bits the engine does not mirror one for one, the texture axes of hidden sides and the contents of the pakfile beyond the .mdl of a model".to_owned(),
        );
        if file_bsp.is_none() {
            report.source = "game memory (collision data, render data and entities)".to_owned();
        } else {
            report.notes.push("the map file, which matched the memory, cross checked it and filled in field by field what the memory does not keep".to_owned());
        }
        report.map_file_opens = input.access.opens();
        return Ok(Captured {
            bsp: Some(mem_bsp),
            pak_bsp: file_bsp,
            entity_text: hit.text.clone(),
            hit_addr: hit.addr,
            report,
        });
    }
    report.map_file_opens = input.access.opens();
    match file_bsp {
        Some(bsp) => {
            report.geometry = "map file".to_owned();
            Ok(Captured { bsp: Some(bsp), pak_bsp: None, entity_text: hit.text.clone(), hit_addr: hit.addr, report })
        }
        None => {
            report.memory_only = true;
            report.geometry = "entities only".to_owned();
            report.source = "game memory (entities only, no collision data found and no map file)".to_owned();
            report.notes.push(
                "neither the world data of the game memory nor a matching map file was found: only the entities were recovered"
                    .to_owned(),
            );
            Ok(Captured { bsp: None, pak_bsp: None, entity_text: hit.text.clone(), hit_addr: hit.addr, report })
        }
    }
}

pub struct Produced {
    pub vmf_path: PathBuf,
    pub bsp_path: Option<PathBuf>,
    pub content_dir: Option<PathBuf>,
    pub report_path: PathBuf,
}

#[allow(clippy::too_many_arguments)]
pub fn produce(
    cap: Captured,
    map_name: &str,
    install: Option<&GmodInstall>,
    settings: &SnifferSettings,
    out_dir: &Path,
    host: &str,
    port: u16,
    shared: Option<&Shared>,
    cancel: &AtomicBool,
    timings: &mut Vec<(String, f64)>,
    memory: Option<&dyn MemSource>,
) -> Run<(SniffReport, Produced)> {
    let say = |level: Level, t: String| {
        if let Some(s) = shared {
            s.say(level, t);
        }
    };
    let phase = |p: Phase, f: f32, t: &str| {
        if let Some(s) = shared {
            s.phase(p, f, t);
        }
    };
    let Captured { bsp, pak_bsp, entity_text, mut report, .. } = cap;
    let mut bsp = bsp;
    let skip_pak: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(b) = bsp.as_ref() {
        let mut un = std::mem::take(&mut report.unprotect);
        let need: Vec<(String, [f32; 3])> = un.signatures.clone();
        let mut guess = std::collections::HashMap::new();
        if !need.is_empty()
            && let Some(inst) = install
            && let Ok(fs) = assets::mount_for(&inst.root)
        {
            let (g, gr) = crate::matguess::resolve(
                &fs,
                &need,
                Some(&crate::settings::default_output_dir().join("texture_reflectivity.cache")),
            );
            say(
                Level::Info,
                format!(
                    "material names guessed from the loaded materials: {} of {} ({} textures indexed, {} ambiguous, {} without a match)",
                    gr.exact,
                    need.len(),
                    gr.indexed_textures,
                    gr.ambiguous,
                    gr.missing
                ),
            );
            un.guess_ambiguous = gr.ambiguous;
            guess = g;
        }
        let (fixed, mut un) = crate::unprotect::restore_materials(b, &guess, un);
        if let Some(nb) = fixed {
            bsp = Some(nb);
        }
        if let (Some(cur), Some(inst)) = (bsp.as_ref(), install)
            && let Ok(fs) = assets::mount_for(&inst.root)
        {
            let r = crate::reflfix::apply(cur, &fs);
            if let Some(nb) = r.bsp {
                say(Level::Info, format!("reflectivity overrides read from the material files of the game content: {}", r.overridden));
                bsp = Some(nb);
            }
        }
        if let Some(b) = bsp.as_ref()
            && let Ok(mut ents) = b.entities()
        {
            crate::unprotect::strip_markers(&mut ents, &mut un);
            if (un.watermark.is_some() || un.lock_hash.is_some())
                && let Ok(copy) = hp_bsp::Bsp::from_vec(b.bytes().to_vec())
            {
                let mut edit = copy.edit();
                if edit.set_entities(&ents).is_ok()
                    && let Some(nb) = edit.to_vec().ok().and_then(|v| hp_bsp::Bsp::from_vec(v).ok())
                {
                    bsp = Some(nb);
                }
            }
        }
        if un.protected() {
            say(
                Level::Info,
                format!(
                    "protected map: {} scrambled material names, {} restored, {} unresolved",
                    un.garbage_names, un.restored, un.unresolved
                ),
            );
        }
        report.unprotect = un;
    }
    std::fs::create_dir_all(out_dir).map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
    let stem =
        output_name(&settings.file_name_template, &NameContext { map: map_name, host, port, unix_seconds: now_secs() });
    let vmf_path = unique_path(out_dir, &stem, "vmf");
    let stem = vmf_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(stem);
    let mut t = Instant::now();

    phase(Phase::Rebuilding, 0.55, "rebuilding the map from what the game loaded");
    let mut bsp_path = None;
    if let Some(b) = &bsp
        && settings.keep_bsp
    {
        let p = out_dir.join(format!("{stem}.bsp"));
        std::fs::write(&p, b.bytes()).map_err(|e| format!("cannot write {}: {e}", p.display()))?;
        say(Level::Info, format!("recovered BSP written: {} ({} bytes)", p.display(), b.len()));
        bsp_path = Some(p);
    }
    lap(timings, "rebuild", &mut t);
    check(cancel)?;

    phase(Phase::Decompiling, 0.65, "decompiling to a VMF");
    let options = hp_decompile::Options {
        map_name: map_name.to_owned(),
        known_outputs: None,
        toggles: settings.decompile.toggles(),
    };
    let decompiled = match &bsp {
        Some(b) => match hp_decompile::decompile(b, &options) {
            Ok(d) => d,
            Err(e) => {
                report
                    .notes
                    .push(format!("the geometry could not be decompiled ({e}): only the entities are in the VMF"));
                hp_decompile::entities_only(&entity_text, &options)
            }
        },
        None => hp_decompile::entities_only(&entity_text, &options),
    };
    report.decompile = decompiled.report.clone();
    let map = decompiled.map;
    let text = hp_vmf::write(&map);
    std::fs::write(&vmf_path, text.as_bytes()).map_err(|e| format!("cannot write {}: {e}", vmf_path.display()))?;
    report.vmf = vmf_counts(&map);
    say(
        Level::Info,
        format!(
            "VMF written: {} (solids {}, sides {}, entities {})",
            vmf_path.display(),
            report.vmf.solids,
            report.vmf.sides,
            report.vmf.entities
        ),
    );
    lap(timings, "decompile", &mut t);
    check(cancel)?;

    let wants_assets = settings.save_models || settings.save_materials || settings.save_sounds || settings.save_other;
    let mut content_dir = None;
    if wants_assets {
        phase(Phase::Extracting, 0.8, "saving the content of the map");
        let dest = assets::content_dir_for(out_dir, &stem);
        let mut ar = AssetReport::default();
        let mut stock_fs: Option<hp_fs::GameFs> = None;
        if let Some(inst) = install {
            match assets::mount_for(&inst.root) {
                Ok(fs) => stock_fs = Some(fs),
                Err(e) => report.notes.push(format!("the game content could not be indexed: {e}")),
            }
        }
        if let Some(b) = pak_bsp.as_ref().or(bsp.as_ref()) {
            let stock = stock_fs.as_ref().map(|fs| move |p: &str| assets::is_stock(fs, p));
            let stock_ref: Option<&dyn Fn(&str) -> bool> = stock.as_ref().map(|f| f as &dyn Fn(&str) -> bool);
            assets::extract_pak(b, &dest, settings, stock_ref, &skip_pak, &mut ar);
        }
        check(cancel)?;
        if let Some(fs) = &stock_fs {
            if dest.is_dir() {
                let _ = fs.add_game_dir(&dest);
            }
            assets::collect_content(fs, &map, map_name, &dest, settings, cancel, &mut ar);
        }
        if let Some(mem) = memory
            && settings.save_models
            && !ar.missing.is_empty()
        {
            let missing = ar.missing.clone();
            assets::recover_models_from_memory(mem, &ScanOptions::default(), &missing, &dest, &mut ar);
            if ar.memory_models > 0 {
                say(
                    Level::Info,
                    format!(
                        "models recovered from the game memory: {} of {} missing ({} bytes of .mdl), vertex files rebuilt {} ({} bytes, {} not whole, {} not found)",
                        ar.memory_models,
                        ar.memory_models_wanted,
                        ar.memory_model_bytes,
                        ar.memory_vvds,
                        ar.memory_vvd_bytes,
                        ar.memory_vvd_partial,
                        ar.memory_vvd_failed
                    ),
                );
                say(
                    Level::Info,
                    format!(
                        "strip files rebuilt {} ({} bytes, {} not whole, {} failed), physics files rebuilt {} ({} bytes)",
                        ar.memory_vtxs, ar.memory_vtx_bytes, ar.memory_vtx_partial, ar.memory_vtx_failed, ar.memory_phys, ar.memory_phy_bytes
                    ),
                );
            }
        }
        if let (Some(mem), Some(fs)) = (memory, &stock_fs)
            && settings.save_materials
        {
            assets::recover_materials_from_memory(mem, &ScanOptions::default(), fs, &dest, &mut ar);
            say(
                Level::Info,
                format!(
                    "materials rebuilt from the key values the game holds: {} not on this computer, of {} loaded ({} bytes)",
                    ar.memory_materials, ar.memory_materials_loaded, ar.memory_material_bytes
                ),
            );
        }
        if let (Some(mem), Some(fs)) = (memory, &stock_fs)
            && settings.save_materials
        {
            assets::list_video_only_textures(mem, &ScanOptions::default(), fs, &dest, &mut ar);
            say(
                Level::Info,
                format!(
                    "textures: {} loaded by the engine, {} not on this computer and held by the video card only ({} bytes), listed in textures_in_video_memory.txt",
                    ar.textures_loaded, ar.textures_video_only, ar.textures_video_only_bytes
                ),
            );
            assets::list_pak_directory(mem, &ScanOptions::default(), &dest, &mut ar);
            if ar.pak_listed > 0 {
                say(
                    Level::Info,
                    format!(
                        "pakfile directory held by the game: {} files, {} identical to what was recovered, {} recovered but different, listed in pakfile_listing.txt",
                        ar.pak_listed, ar.pak_listed_identical, ar.pak_listed_differs
                    ),
                );
            }
        }
        if ar.total_files() > 0 || ar.memory_models > 0 || ar.memory_materials > 0 || ar.textures_video_only > 0 {
            content_dir = Some(dest);
        }
        if report.geometry == "game memory" {
            report.sources =
                crate::merge::source_rows(&report.merge, settings.file_allowed(), ar.content_files, ar.memory_models);
            let hub_rows = report.hub.source_rows();
            report.sources.extend(hub_rows);
            let u = &report.unprotect;
            if u.garbage_names > 0 {
                let detail = format!(
                    "{} scrambled: {} guessed from the loaded materials, {} unresolved",
                    u.garbage_names, u.guessed, u.unresolved
                );
                let source = if u.guessed > 0 {
                    "memory and game content"
                } else {
                    "absent"
                };
                let at = report.sources.iter().position(|r| r.category == "faces and texture axes").unwrap_or(0);
                report.sources.insert(
                    at,
                    crate::merge::SourceRow {
                        category: "material names of a protected map".to_owned(),
                        source: source.to_owned(),
                        detail,
                    },
                );
            }
        }
        report.assets = ar;
        lap(timings, "assets", &mut t);
    }
    check(cancel)?;

    report.vmf_path = vmf_path.clone();
    report.bsp_path = bsp_path.clone();
    report.content_dir = content_dir.clone();
    report.map = map_name.to_owned();
    report.timings = timings.clone();
    let report_path = out_dir.join(format!("{stem}_sniff.txt"));
    report.report_path = Some(report_path.clone());
    let _ = std::fs::write(&report_path, report.render_text());
    Ok((report.clone(), Produced { vmf_path, bsp_path, content_dir, report_path }))
}

fn run(shared: &Shared, cancel: &AtomicBool, settings: &SnifferSettings) -> Run<Outcome> {
    let mut timings: Vec<(String, f64)> = Vec::new();
    let mut t = Instant::now();
    shared.phase(Phase::Preflight, 0.02, "checking the settings and Garry's Mod");
    settings.validate()?;
    let access = FileAccess::new(settings.file_allowed());
    let (host, port) = settings.parsed_address()?;
    let install = launch::detect_install_bits(&settings.gmod_dir, settings.game_bits)?;
    shared.say(
        Level::Info,
        format!(
            "Garry's Mod: {} ({} bit branch, {})",
            install.root.display(),
            if install.is_64bit { 64 } else { 32 },
            install.exe.display()
        ),
    );
    let out_dir = settings.output_root();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
    let scratch = out_dir.join("_work");
    let _ = std::fs::create_dir_all(&scratch);
    check(cancel)?;

    shared.phase(Phase::Querying, 0.05, "asking the server about its map");
    let local = settings.local_map();
    let info = if local.is_some() {
        None
    } else {
        match query::query(&host, port, Duration::from_secs(4)) {
            Ok(i) => {
                shared.say(
                    Level::Info,
                    format!(
                        "server: {} | map {} | players {}/{} | VAC secure {}",
                        i.name, i.map, i.players, i.max_players, i.vac
                    ),
                );
                Some(i)
            }
            Err(e) => {
                shared.say(Level::Warn, format!("the server query failed ({e}); continuing without it"));
                None
            }
        }
    };
    let vac = if local.is_some() { Some(false) } else { info.as_ref().map(|i| i.vac) };
    let expected_map = match &local {
        Some(m) if !m.is_empty() => Some(m.clone()),
        Some(_) => None,
        None => info.as_ref().map(|i| i.map.clone()),
    };
    if let Some(m) = &expected_map {
        shared.status.lock().map = Some(m.clone());
    }
    let insecure = launch::insecure_applies(settings, vac);
    if vac == Some(true) {
        shared.say(
            Level::Warn,
            "this server is VAC secured: the game runs with VAC enabled while Hammer Pro reads its memory (read only)",
        );
    }
    lap(&mut timings, "preflight", &mut t);
    check(cancel)?;

    let pre = launch::pids_set(&install);
    let mut guard = GameGuard {
        install: install.clone(),
        pid: None,
        launched: false,
        kill: settings.kill_game,
        shared,
        pre: pre.clone(),
    };
    let mut tail = LogTail::new(install.console_log());
    let attach = settings.attach_running && !pre.is_empty();
    if attach {
        let pid = *pre.iter().min().expect("not empty");
        shared.say(Level::Info, format!("Garry's Mod is already running (process {pid}): attaching to it"));
        guard.pid = Some(pid);
    } else if !pre.is_empty() {
        return Err(Failure::Error(
            "Garry's Mod is already running: close it or allow attaching to the running game in the settings"
                .to_owned(),
        ));
    } else {
        shared.phase(Phase::Launching, 0.08, "starting Garry's Mod through Steam");
        let args = match &local {
            Some(m) => launch::build_local_args(settings, m),
            None => launch::build_args(settings, &host, port, vac),
        };
        let how = launch::launch_game(&install, &args)?;
        shared.say(Level::Info, how);
        guard.launched = true;
    }

    shared.phase(Phase::WaitingGame, 0.12, "waiting for the game process");
    let proc_deadline = Instant::now() + Duration::from_secs(u64::from(settings.process_timeout_secs));
    if guard.pid.is_none() {
        loop {
            check(cancel)?;
            let fresh: Vec<u32> = launch::game_pids_for(&install).into_iter().filter(|p| !pre.contains(p)).collect();
            if let Some(&pid) = fresh.first() {
                guard.pid = Some(pid);
                shared.say(Level::Info, format!("game process found: {pid}"));
                break;
            }
            if Instant::now() > proc_deadline {
                return Err(Failure::Error(
                    "Garry's Mod did not start in time (is Steam running and logged in?)".to_owned(),
                ));
            }
            sleep_checked(cancel, Duration::from_secs(1))?;
        }
    }
    let pid = guard.pid.expect("set above");
    shared.status.lock().game_pid = Some(pid);
    let game_is_64bit = crate::procmem::process_is_64bit(pid);
    if let Some(is64) = game_is_64bit
        && is64 != install.is_64bit
    {
        shared.say(Level::Warn, "the running game does not match the detected install branch");
    }
    let proc = open_process(pid)?;
    shared.say(Level::Info, format!("reading memory of process {pid} (read only)"));
    lap(&mut timings, "launch", &mut t);

    shared.phase(Phase::WaitingMap, 0.2, "waiting for the connection and the map");
    let load_deadline = Instant::now() + Duration::from_secs(u64::from(settings.load_timeout_secs));
    let poll = Duration::from_secs(u64::from(settings.poll_secs));
    let scan_opts = ScanOptions { writable_only: true, min_region: 1 << 16, ..ScanOptions::default() };
    let mut baseline: HashSet<u64> = HashSet::new();
    let mut stable: Option<(u64, Instant)> = None;
    let mut selected: Option<EntityHit> = None;
    let mut last_note = Instant::now();
    let mut file_cache: Option<(String, Vec<String>, Instant)> = None;
    let mut map_seen: Option<String> = expected_map.clone();
    if attach
        && map_seen.is_none()
        && let Ok(text) = std::fs::read_to_string(install.console_log())
        && let Some(m) = text.lines().rev().find_map(|l| l.trim().strip_prefix("Map: ").map(|m| m.trim().to_owned()))
    {
        shared.say(Level::Info, format!("the running game reports the map {m}"));
        shared.status.lock().map = Some(m.clone());
        map_seen = Some(m);
    }
    let settle = Duration::from_secs(u64::from(settings.settle_secs));
    loop {
        check(cancel)?;
        if !crate::procmem::is_running(pid) {
            return Err(Failure::Error(format!(
                "the game closed before the map was loaded{}",
                tail.events.problems.first().map(|p| format!(" ({p})")).unwrap_or_default()
            )));
        }
        for l in tail.poll() {
            let lower = l.to_ascii_lowercase();
            if lower.starts_with("connecting to")
                || lower.starts_with("connected to")
                || lower.starts_with("map:")
                || lower.starts_with("disconnect")
                || lower.contains("connection failed")
                || lower.contains("kicked")
                || lower.contains("banned")
            {
                shared.say(Level::Info, format!("game: {l}"));
            }
        }
        if let Some(m) = &tail.events.map {
            map_seen = Some(m.clone());
            shared.status.lock().map = Some(m.clone());
        }
        if Instant::now() > load_deadline {
            let why = tail.events.problems.first().cloned().unwrap_or_else(|| {
                if tail.events.connected {
                    "connected but the map never finished loading".to_owned()
                } else {
                    "the game never connected".to_owned()
                }
            });
            return Err(Failure::Error(format!("timeout waiting for the map: {why}")));
        }
        if let Some(p) = tail.events.problems.first()
            && !tail.events.connected
            && tail.events.connecting
            && local.is_none()
        {
            return Err(Failure::Error(format!("the server refused the connection: {p}")));
        }
        let scan_started = Instant::now();
        let hits = find_entity_lumps(&proc, &scan_opts);
        let scan_took = scan_started.elapsed();
        let loaded = match &local {
            Some(m) if !m.is_empty() => tail.events.map.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(m)),
            Some(_) => attach,
            None => tail.events.connected,
        };
        if !loaded && !attach && local.is_none() {
            for h in &hits {
                baseline.insert(text_hash(&h.text));
            }
        }
        if loaded || attach {
            if let Some(m) = &map_seen {
                let stale = match &file_cache {
                    Some((key, texts, at)) => key != m || (texts.is_empty() && at.elapsed() > Duration::from_secs(20)),
                    None => true,
                };
                if stale {
                    let mut texts: Vec<String> = Vec::new();
                    for c in recover::find_candidates(&install.game_dir, m, &scratch, &access) {
                        if access.is_allowed()
                            && let Ok(b) = hp_bsp::Bsp::open(&c.path)
                            && let Ok(text) = b.entity_text()
                        {
                            texts.push(text);
                        }
                    }
                    file_cache = Some((m.clone(), texts, Instant::now()));
                }
            }
            let file_texts: &[String] = file_cache.as_ref().map(|c| c.1.as_slice()).unwrap_or(&[]);
            if let Some(h) = pick_hit(&hits, &baseline, file_texts) {
                let hash = text_hash(&h.text);
                match stable {
                    Some((sh, since)) if sh == hash => {
                        let waited = since.elapsed();
                        shared.detail(
                            0.5,
                            format!(
                                "map found in memory, settling ({:.0} s of {:.0} s)",
                                waited.as_secs_f32(),
                                settle.as_secs_f32()
                            ),
                        );
                        if waited >= settle {
                            selected = Some(h.clone());
                            break;
                        }
                    }
                    _ => {
                        shared.say(
                            Level::Info,
                            format!("entity lump in memory: {} entities at 0x{:x}", h.entities, h.addr),
                        );
                        stable = Some((hash, Instant::now()));
                        if settle.is_zero() {
                            selected = Some(h.clone());
                            break;
                        }
                    }
                }
            } else {
                stable = None;
            }
        }
        if selected.is_none() && last_note.elapsed() > Duration::from_secs(20) {
            last_note = Instant::now();
            let left = load_deadline.saturating_duration_since(Instant::now()).as_secs();
            shared.detail(
                0.3,
                format!(
                    "waiting: connected {}, map {}, entity lumps seen {} ({} s left)",
                    tail.events.connected,
                    map_seen.as_deref().unwrap_or("not known yet"),
                    hits.len(),
                    left
                ),
            );
        }
        sleep_checked(cancel, poll.max(scan_took * 2))?;
    }
    let hit = selected.expect("loop breaks only with a hit");
    lap(&mut timings, "wait for the map", &mut t);

    shared.phase(Phase::Capturing, 0.4, "reading the loaded map from memory");
    let input = CaptureInput {
        game_dir: &install.game_dir,
        map_hint: map_seen.clone(),
        settings,
        access: &access,
        rescan: Some(poll),
        rescan_limit: Duration::from_secs(u64::from(settings.load_timeout_secs)),
    };
    shared.phase(Phase::Verifying, 0.45, "comparing the game memory with the map files");
    let mut cap = capture_from(&proc, &hit, &input, &scratch, Some(shared), cancel)?;
    cap.report.server_name = info.as_ref().map(|i| i.name.clone()).unwrap_or_default();
    cap.report.address = match &local {
        Some(m) => format!("local game, map {m}"),
        None => format!("{host}:{port}"),
    };
    cap.report.game_pid = Some(pid);
    cap.report.game_is_64bit = game_is_64bit;
    cap.report.vac_secure = vac;
    cap.report.insecure_flag = insecure;
    cap.report.launched_by_us = guard.launched;
    lap(&mut timings, "capture and verify", &mut t);
    let map_name = map_seen.clone().unwrap_or_else(|| "sniffed".to_owned());
    let (mut report, produced) = produce(
        cap,
        &map_name,
        Some(&install),
        settings,
        &out_dir,
        &host,
        port,
        Some(shared),
        cancel,
        &mut timings,
        Some(&proc),
    )?;
    if settings.read_ldr_lighting && guard.launched {
        shared.phase(Phase::Finishing, 0.93, "reading the second lighting set from the game");
        let connect = local.is_none().then(|| format!("{host}:{port}"));
        crate::ldrpass::after_produce(
            &proc,
            &install,
            &mut tail,
            settings,
            local.as_deref(),
            connect.as_deref(),
            &mut report,
            produced.bsp_path.as_deref(),
            &produced.report_path,
            cancel,
            &|t| shared.say(Level::Info, t),
        );
    }
    drop(proc);

    shared.phase(Phase::Finishing, 0.97, "closing the session");
    if settings.auto_disconnect && !(guard.launched && settings.kill_game) {
        shared.say(Level::Info, "auto disconnect: asking the game to disconnect");
        if launch::disconnect_game(&install, &mut tail, Duration::from_secs(90)) {
            shared.say(Level::Info, "the game disconnected from the server");
        } else {
            shared.say(Level::Warn, "the game did not confirm the disconnect (close it from the game if needed)");
        }
    }
    guard.shutdown();
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(Outcome {
        vmf_path: produced.vmf_path,
        bsp_path: produced.bsp_path,
        content_dir: produced.content_dir,
        report_path: produced.report_path,
        report,
    })
}

#[cfg(windows)]
fn open_process(pid: u32) -> Run<crate::procmem::GameProcess> {
    crate::procmem::GameProcess::open(pid).map_err(|e| Failure::Error(e.to_string()))
}

#[cfg(not(windows))]
fn open_process(_pid: u32) -> Run<crate::procmem::VecMem> {
    Err(Failure::Error("the BSP Sniffer works on Windows only".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    fn entity_text(n: usize, tag: &str) -> String {
        let mut t = format!("{{\n\"classname\" \"worldspawn\"\n\"skyname\" \"{tag}\"\n}}\n");
        for i in 0..n {
            t.push_str(&format!(
                "{{\n\"classname\" \"info_target\"\n\"targetname\" \"t{i}\"\n\"origin\" \"{i} 0 0\"\n}}\n"
            ));
        }
        t
    }

    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(11);
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as u8 | 0x80
            })
            .collect()
    }

    fn hit(n: usize, tag: &str, addr: u64) -> EntityHit {
        EntityHit { addr, text: entity_text(n, tag), entities: n + 1 }
    }

    #[test]
    fn the_map_of_the_server_is_not_the_menu_background() {
        let menu = hit(20, "menu", 0x1000);
        let live = hit(60, "sky_day01_01", 0x9000);
        let hits = vec![live.clone(), menu.clone()];
        let baseline: HashSet<u64> = [text_hash(&menu.text)].into_iter().collect();
        assert_eq!(pick_hit(&hits, &baseline, &[]).map(|h| h.addr), Some(0x9000));
        let only_menu = vec![menu.clone()];
        assert!(pick_hit(&only_menu, &baseline, &[]).is_none());
        let files = vec![menu.text.clone()];
        assert_eq!(
            pick_hit(&hits, &HashSet::new(), &files).map(|h| h.addr),
            Some(0x1000),
            "a file match beats the baseline"
        );
        assert_eq!(pick_hit(&[], &baseline, &files).map(|h| h.addr), None);
    }

    #[test]
    fn the_phases_have_stable_names() {
        let all = [
            Phase::Idle,
            Phase::Preflight,
            Phase::Querying,
            Phase::Launching,
            Phase::WaitingGame,
            Phase::WaitingMap,
            Phase::Capturing,
            Phase::Verifying,
            Phase::Rebuilding,
            Phase::Decompiling,
            Phase::Extracting,
            Phase::Finishing,
            Phase::Done,
            Phase::Failed,
            Phase::Cancelled,
        ];
        let ids: HashSet<&str> = all.iter().map(|p| p.id()).collect();
        assert_eq!(ids.len(), all.len());
        assert!(Phase::Done.is_final() && Phase::Failed.is_final() && Phase::Cancelled.is_final());
        assert!(Phase::WaitingMap.is_running() && !Phase::Idle.is_running() && !Phase::Done.is_running());
    }

    #[test]
    fn starting_needs_consent_and_a_valid_address() {
        let mut s = SnifferSettings::default();
        assert!(SnifferHandle::start(s.clone(), None).is_err(), "not authorized");
        s.authorized = true;
        s.address = "not an address".to_owned();
        assert!(SnifferHandle::start(s, None).is_err());
    }

    #[test]
    fn a_capture_without_a_map_file_still_gives_the_entities() {
        let game = std::env::temp_dir().join(format!("hp_sniffer_session_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(&game).expect("dir");
        let h = hit(15, "sky_x", 0x2000);
        let mut m = VecMem::default();
        let mut block = noise(64, 1);
        block.push(0);
        block.extend_from_slice(h.text.as_bytes());
        block.push(0);
        m.add(0x1000, block);
        let settings = SnifferSettings::default();
        let access = FileAccess::allowed();
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_nothing".to_owned()),
            settings: &settings,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cancel = AtomicBool::new(false);
        let cap = capture_from(&m, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert!(cap.bsp.is_none());
        assert!(cap.report.memory_only);
        let mut timings = Vec::new();
        let out = game.join("out");
        let s2 = SnifferSettings {
            save_models: false,
            save_materials: false,
            save_sounds: false,
            save_other: false,
            ..settings
        };
        let (report, produced) =
            produce(cap, "gm_nothing", None, &s2, &out, "1.2.3.4", 27015, None, &cancel, &mut timings, None)
                .ok()
                .expect("produce");
        assert!(produced.vmf_path.is_file());
        assert!(produced.report_path.is_file());
        assert!(produced.bsp_path.is_none());
        let vmf = hp_vmf::parse(&std::fs::read_to_string(&produced.vmf_path).expect("read")).expect("parse");
        assert_eq!(vmf.entities.len(), 15);
        assert_eq!(report.vmf.entities, 15);
        assert!(vmf.world.get("skyname").is_some());
        let second = produce(
            Captured { bsp: None, pak_bsp: None, entity_text: h.text.clone(), hit_addr: 0, report: report.clone() },
            "gm_nothing",
            None,
            &s2,
            &out,
            "1.2.3.4",
            27015,
            None,
            &cancel,
            &mut timings,
            None,
        )
        .ok()
        .expect("second");
        assert_ne!(second.1.vmf_path, produced.vmf_path, "files are never overwritten");
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_capture_with_a_matching_file_gives_a_bsp_and_a_vmf() {
        let game = std::env::temp_dir().join(format!("hp_sniffer_session_full_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(game.join("maps")).expect("dir");
        let verts = noise(1200, 9);
        let h = hit(8, "sky_y", 0x3000);
        let mut ents = h.text.as_bytes().to_vec();
        ents.push(0);
        let data = hp_bsp::synthetic_bsp(20, &[(hp_bsp::lump::ENTITIES, ents), (3, verts.clone())]);
        std::fs::write(game.join("maps").join("gm_x.bsp"), data).expect("write");
        let mut m = VecMem::default();
        let mut block = noise(64, 2);
        block.push(0);
        block.extend_from_slice(h.text.as_bytes());
        block.push(0);
        while !block.len().is_multiple_of(4) {
            block.push(0);
        }
        m.add(0x10_0000, block);
        let mut b2 = noise(800, 3);
        b2.extend_from_slice(&verts);
        m.add(0x40_0000, b2);
        let settings = SnifferSettings::default();
        let access = FileAccess::allowed();
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_x".to_owned()),
            settings: &settings,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cancel = AtomicBool::new(false);
        let cap = capture_from(&m, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert!(cap.bsp.is_some());
        assert!(cap.report.entities_identical);
        assert_eq!(cap.report.verified_lumps, 1);
        let mut timings = Vec::new();
        let s2 = SnifferSettings {
            save_models: false,
            save_materials: false,
            save_sounds: false,
            save_other: false,
            ..settings
        };
        let (report, produced) =
            produce(cap, "gm_x", None, &s2, &game.join("out"), "1.2.3.4", 27015, None, &cancel, &mut timings, None)
                .ok()
                .expect("produce");
        assert!(produced.bsp_path.as_ref().is_some_and(|p| p.is_file()));
        assert!(
            report.notes.iter().any(|n| n.contains("could not be decompiled")),
            "no brushes in the synthetic map: {:?}",
            report.notes
        );
        assert_eq!(report.vmf.entities, 8);
        let _ = std::fs::remove_dir_all(&game);
    }

    fn world_with_entities(text: &str) -> (VecMem, usize) {
        let (mut mem, brushes, _) = crate::memmap::tests::world_memory();
        let mut block = noise(64, 7);
        block.push(0);
        block.extend_from_slice(text.as_bytes());
        block.push(0);
        mem.add(0x6000_0000, block);
        (mem, brushes)
    }

    #[test]
    fn without_a_file_the_brushes_come_from_the_game_memory() {
        let game = std::env::temp_dir().join(format!("hp_sniffer_session_mem_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(&game).expect("dir");
        let h = hit(6, "sky_mem", 0x6000_0040);
        let (mem, brushes) = world_with_entities(&h.text);
        let settings = SnifferSettings::default();
        let access = FileAccess::allowed();
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_none".to_owned()),
            settings: &settings,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cancel = AtomicBool::new(false);
        let cap = capture_from(&mem, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert!(cap.bsp.is_some());
        assert_eq!(cap.report.geometry, "game memory");
        assert!(cap.report.memory_only);
        assert!(cap.report.cm.found);
        assert_eq!(cap.report.cm.brushes, brushes);
        assert_eq!(cap.report.bsp_counts.brushes, brushes);
        assert!(cap.pak_bsp.is_none());
        let forced_file = SnifferSettings { geometry: GeometrySource::File, ..SnifferSettings::default() };
        let access = FileAccess::allowed();
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_none".to_owned()),
            settings: &forced_file,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cap = capture_from(&mem, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert!(cap.bsp.is_none(), "the file only mode never rebuilds from memory");
        assert_eq!(cap.report.geometry, "entities only");
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_file_that_differs_from_the_memory_is_not_trusted() {
        let game = std::env::temp_dir().join(format!("hp_sniffer_session_diff_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(game.join("maps")).expect("dir");
        let h = hit(6, "sky_diff", 0x6000_0040);
        let (mem, brushes) = world_with_entities(&h.text);
        let mut ents = h.text.as_bytes().to_vec();
        ents.push(0);
        let mut pak = hp_bsp::PakWriter::new();
        pak.add("materials/custom/wall.vmt", b"LightmappedGeneric {}".to_vec()).expect("add");
        let zip = pak.finish(hp_bsp::PakOptions::default()).expect("zip");
        let verts = noise(1200, 12);
        let mut vert_block = noise(800, 13);
        vert_block.extend_from_slice(&verts);
        let mut mem = mem;
        mem.add(0x7000_0000, vert_block);
        let data = hp_bsp::synthetic_bsp(
            20,
            &[(hp_bsp::lump::ENTITIES, ents), (3, verts), (18, vec![0u8; 12 * 3]), (hp_bsp::lump::PAKFILE, zip)],
        );
        std::fs::write(game.join("maps").join("gm_diff.bsp"), data).expect("write");
        let settings = SnifferSettings::default();
        let access = FileAccess::allowed();
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_diff".to_owned()),
            settings: &settings,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cancel = AtomicBool::new(false);
        let cap = capture_from(&mem, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert_eq!(cap.report.geometry, "game memory", "{:?}", cap.report.notes);
        assert!(!cap.report.memory_only, "a file was found");
        assert!(cap.report.entities_identical);
        assert!(!cap.report.cm.identical);
        assert_eq!(cap.report.cm.brushes, brushes);
        assert!(
            cap.pak_bsp.as_ref().is_some_and(|b| b.pakfile().is_ok_and(|p| p.len() == 1)),
            "the pakfile still comes from the file"
        );
        let forced = SnifferSettings { geometry: GeometrySource::File, ..SnifferSettings::default() };
        let access = FileAccess::allowed();
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_diff".to_owned()),
            settings: &forced,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cap = capture_from(&mem, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert_eq!(cap.report.geometry, "map file");
        let _ = std::fs::remove_dir_all(&game);
    }

    fn forbidden_setup(name: &str) -> (PathBuf, EntityHit, VecMem, usize, Vec<u8>) {
        let game = std::env::temp_dir().join(format!("hp_sniffer_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(game.join("maps")).expect("dir");
        std::fs::create_dir_all(game.join("cache")).expect("cache");
        let h = hit(6, "sky_never", 0x6000_0040);
        let (mem, brushes) = world_with_entities(&h.text);
        let mut other = hit(9, "sky_from_the_file", 0).text.into_bytes();
        other.push(0);
        let mut pak = hp_bsp::PakWriter::new();
        pak.add("materials/custom/wall.vmt", b"LightmappedGeneric {}".to_vec()).expect("add");
        let zip = pak.finish(hp_bsp::PakOptions::default()).expect("zip");
        let data = hp_bsp::synthetic_bsp(
            20,
            &[(hp_bsp::lump::ENTITIES, other), (18, vec![0u8; 12 * 3]), (hp_bsp::lump::PAKFILE, zip)],
        );
        std::fs::write(game.join("maps").join("gm_never.bsp"), &data).expect("write");
        std::fs::write(game.join("cache").join("map_pack.bsp"), &data).expect("write");
        (game, h, mem, brushes, data)
    }

    #[test]
    fn a_forbidden_map_file_is_never_opened_and_everything_still_comes_from_the_memory() {
        let (game, h, mem, brushes, _) = forbidden_setup("never");
        let settings = SnifferSettings { map_file: crate::settings::MapFileMode::Never, ..SnifferSettings::default() };
        assert!(!settings.file_allowed());
        let access = FileAccess::new(settings.file_allowed());
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_never".to_owned()),
            settings: &settings,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cancel = AtomicBool::new(false);
        let cap = capture_from(&mem, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert_eq!(access.opens(), 0, "no map file was opened");
        assert_eq!(cap.report.map_file_opens, 0);
        assert_eq!(cap.report.map_file_mode, "never");
        assert!(cap.pak_bsp.is_none(), "no file means no pakfile either");
        assert!(cap.report.memory_only);
        assert_eq!(cap.report.geometry, "game memory");
        assert_eq!(cap.report.cm.brushes, brushes);
        assert!(cap.report.cross_check.is_empty());
        assert_eq!(cap.report.verified_lumps, 0);
        assert!(cap.report.coverage.is_empty());
        assert_eq!(cap.report.file_entities, 0);
        let bsp = cap.bsp.expect("bsp");
        assert_eq!(bsp.entities().expect("entities").len(), 7, "the entities are the ones of the memory");
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn the_same_setup_reads_and_cross_checks_the_files_when_they_are_allowed() {
        let (game, h, mem, brushes, _) = forbidden_setup("allowed");
        let settings = SnifferSettings::default();
        let access = FileAccess::allowed();
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_never".to_owned()),
            settings: &settings,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cancel = AtomicBool::new(false);
        let cap = capture_from(&mem, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert!(access.opens() >= 2, "both files were read: {}", access.opens());
        assert_eq!(cap.report.cm.brushes, brushes);
        let _ = std::fs::remove_dir_all(&game);
    }

    fn extras_with(
        displacements: usize,
        overlays: usize,
        with_faces: bool,
        props: usize,
    ) -> crate::engine::MemoryExtras {
        let disp: Vec<crate::engine::MemDisp> = (0..displacements)
            .map(|i| crate::engine::MemDisp { face: i, start: [0.0; 3], power: 2, positions: Vec::new(), file: None, ..Default::default() })
            .collect();
        let overlays: Vec<crate::engine::MemOverlay> = (0..overlays)
            .map(|i| crate::engine::MemOverlay {
                id: i as i32,
                texinfo: 0,
                faces: if with_faces { vec![0] } else { Vec::new() },
                u: [0.0; 2],
                v: [0.0; 2],
                points: [[0.0; 3]; 4],
                origin: [0.0; 3],
                basis_u: [1.0, 0.0, 0.0],
                basis_v: [0.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                render_order: 0,
                fade: crate::engine::OVERLAY_FADE_DEFAULT,
                from_file: false,
            })
            .collect();
        let props: Vec<crate::engine::MemProp> = (0..props)
            .map(|i| crate::engine::MemProp {
                origin: [i as f32, 0.0, 0.0],
                angles: [0.0; 3],
                model: "models/a.mdl".to_owned(),
                alpha: 255,
                solid: 6,
                skin: 0,
                fades: false,
                first_leaf: 0,
                leaf_count: 1,
                file: None,
                ..Default::default()
            })
            .collect();
        crate::engine::MemoryExtras {
            displacements: disp,
            overlays_expected: overlays.len(),
            overlays,
            props_expected: props.len(),
            props,
            cubemaps: vec![[0, 0, 0]],
            ..crate::engine::MemoryExtras::default()
        }
    }

    #[test]
    fn a_collapsed_scan_is_seen_as_the_map_moving_and_never_replaces_a_good_one() {
        let good = extras_with(110, 11, true, 182);
        let empty = crate::engine::MemoryExtras::default();
        let half = extras_with(110, 11, true, 90);
        assert!(extras_score(&good) > extras_score(&half));
        assert!(extras_looks_stale(&good, &empty), "everything at zero after a good scan is a moved map");
        assert!(!extras_looks_stale(&good, &half), "a partial scan is not a moved map");
        assert!(!extras_looks_stale(&empty, &empty), "nothing found yet is not a moved map");
        assert!(!extras_looks_stale(&good, &good));
        assert!(extras_complete(&good, 110));
        assert!(!extras_complete(&good, 111), "a missing displacement is not complete");
        assert!(!extras_complete(&extras_with(110, 11, false, 182), 110), "an overlay without faces is not complete");
        assert!(!extras_complete(&empty, 110));
        assert!(extras_complete(&empty, 0), "a map with none of them is complete at once");
    }

    #[test]
    fn forbidding_the_file_is_enforced_below_the_session_too() {
        let (game, _, mem, _, _) = forbidden_setup("guard");
        let access = FileAccess::forbidden();
        let found = recover::find_candidates(&game, "gm_never", &game.join("scratch"), &access);
        assert!(found.is_empty(), "no candidate is even listed");
        assert_eq!(access.opens(), 0);
        let allowed = FileAccess::allowed();
        let listed = recover::find_candidates(&game, "gm_never", &game.join("scratch"), &allowed);
        assert!(listed.len() >= 2, "the files exist and are found when allowed");
        let (ev, bsp) = recover::evaluate(&mem, &listed[0], None, &ScanOptions::default(), &access);
        assert!(bsp.is_none() && ev.rejected.is_some(), "evaluate refuses to open a file");
        assert_eq!(access.opens(), 0);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn auto_prefers_the_memory_even_when_a_matching_file_exists() {
        let game = std::env::temp_dir().join(format!("hp_sniffer_prefer_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(game.join("maps")).expect("dir");
        let h = hit(6, "sky_pref", 0x6000_0040);
        let (mut mem, brushes) = world_with_entities(&h.text);
        crate::memmap::tests::add_render_memory(&mut mem);
        let world = crate::memmap::find_all(&mem, &ScanOptions::default(), "gm_pref").expect("world");
        assert!(world.render.is_some());
        let built = crate::memmap::build_bsp(&world, &h.text).expect("bsp");
        std::fs::write(game.join("maps").join("gm_pref.bsp"), built.bytes()).expect("write");
        let settings = SnifferSettings::default();
        let access = FileAccess::allowed();
        let input = CaptureInput {
            game_dir: &game,
            map_hint: Some("gm_pref".to_owned()),
            settings: &settings,
            access: &access,
            rescan: None,
            rescan_limit: Duration::ZERO,
        };
        let cancel = AtomicBool::new(false);
        let cap = capture_from(&mem, &h, &input, &game, None, &cancel).ok().expect("capture");
        assert_eq!(cap.report.geometry, "game memory");
        assert_eq!(cap.report.cm.brushes, brushes);
        let rows: std::collections::HashMap<&str, (usize, usize)> =
            cap.report.cross_check.iter().map(|r| (r.name.as_str(), (r.memory, r.file))).collect();
        assert!(cap.report.merge.file_used, "the consistent file fills in the memory");
        assert_eq!(rows["brushes"], (brushes, brushes));
        assert_eq!(rows["brush sides"].0, rows["brush sides"].1);
        let _ = std::fs::remove_dir_all(&game);
    }
}
