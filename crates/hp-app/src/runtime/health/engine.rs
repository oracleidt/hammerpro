use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use hp_editor::edicts::{EdictReport, estimate_edicts};
use hp_editor::limits::checks::{CheckKnowledge, MaterialFacts, check_snapshot, detail_candidates, leak_problems};
use hp_editor::limits::leak::{LeakInput, LeakPath, LeakReport, apply_cordons, find_leaks, flood_map, leak_input_from};
use hp_editor::limits::snapshot::{MapSnapshot, SnapshotBuilder};
use hp_editor::limits::{LimitProfile, LimitUse, estimate_limits};
use hp_editor::{Document, Problem};
use hp_fs::deps::DepsCache;
use parking_lot::Mutex;

use super::knowledge::{AssetAnswers, ClassFacts, Knowledge, class_facts, material_facts};
use crate::runtime::instances::{InstanceCache, instance_target_names};
use crate::state::{AssetStatus, AssetsApi};

const SLICE: Duration = Duration::from_millis(3);
const MATERIAL_POLL: Duration = Duration::from_millis(1000);

#[derive(Debug)]
pub struct CheckResult {
    pub serial: u64,
    pub problems: Vec<Problem>,
    pub limits: Vec<LimitUse>,
    pub edicts: EdictReport,
    pub ms: u64,
    pub snapshot: Arc<MapSnapshot>,
}

#[derive(Debug)]
pub struct LeakResult {
    pub serial: u64,
    pub report: LeakReport,
    pub problems: Vec<Problem>,
    pub ms: u64,
}

pub(crate) enum Msg {
    Checks(CheckResult),
    Leak(LeakResult),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Want {
    content: u64,
    visibility: u64,
    knowledge: u64,
    options: u64,
}

struct Job {
    cancel: Arc<AtomicBool>,
    rx: Receiver<Msg>,
    leak_pending: bool,
    answered: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pace {
    pub delay: Duration,
    pub hold: bool,
    pub force: bool,
}

pub(crate) struct Engine {
    builder: SnapshotBuilder,
    building: Option<Want>,
    sent: Option<Want>,
    seen: Option<Want>,
    changed_at: Instant,
    job: Option<Job>,
    classes: Arc<HashMap<String, Option<ClassFacts>>>,
    fgd_generation: Option<(u64, usize)>,
    materials: Arc<HashMap<String, MaterialFacts>>,
    material_generation: u64,
    unknown_materials: Vec<String>,
    last_material_poll: Instant,
    knowledge_version: u64,
    answers: Arc<Mutex<AssetAnswers>>,
    deps_cache: Arc<DepsCache>,
    has_instances: bool,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("building", &self.building)
            .field("sent", &self.sent)
            .field("job", &self.job.is_some())
            .field("materials", &self.materials.len())
            .finish_non_exhaustive()
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            builder: SnapshotBuilder::new(),
            building: None,
            sent: None,
            seen: None,
            changed_at: Instant::now(),
            job: None,
            classes: Arc::default(),
            fgd_generation: None,
            materials: Arc::default(),
            material_generation: u64::MAX,
            unknown_materials: Vec::new(),
            last_material_poll: Instant::now(),
            knowledge_version: 0,
            answers: Arc::default(),
            deps_cache: Arc::new(DepsCache::new()),
            has_instances: false,
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

fn hash_of(v: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

#[derive(Debug, Default)]
pub(crate) struct Tick {
    pub checks: Option<CheckResult>,
    pub leak: Option<LeakResult>,
    pub busy: bool,
}

impl Engine {
    pub(crate) fn leak_pending(&self) -> bool {
        self.job.as_ref().is_some_and(|j| j.leak_pending)
    }

    fn want(&self, doc: &Document, assets: &dyn AssetsApi, profile: LimitProfile, leak: LeakMode) -> Want {
        let hidden: Vec<u32> = doc.scene().hidden_visgroups().iter().copied().collect();
        let instances = if self.has_instances { InstanceCache::shared().generation() } else { 0 };
        Want {
            content: doc.content_serial(),
            visibility: hash_of(&hidden),
            knowledge: hash_of((
                assets.generation(),
                assets.fgd().map_or(0, |f| f.len()),
                self.knowledge_version,
                instances,
            )),
            options: hash_of((profile.id(), leak.on, leak.cordons.then(|| doc.cordon_key()))),
        }
    }

    fn poll_materials(&mut self, assets: &mut dyn AssetsApi) {
        if self.unknown_materials.is_empty() || self.last_material_poll.elapsed() < MATERIAL_POLL {
            return;
        }
        self.last_material_poll = Instant::now();
        let mut learned = false;
        self.unknown_materials.retain(|name| match assets.material_info(name) {
            Some(info) => {
                Arc::make_mut(&mut self.materials).insert(normalize(name), material_facts(&info));
                learned = true;
                false
            }
            None => true,
        });
        if learned {
            self.knowledge_version += 1;
        }
    }

    fn knowledge(&mut self, doc: &Document, assets: &mut dyn AssetsApi, snap: &MapSnapshot) -> Knowledge {
        let status = assets.status();
        self.has_instances = snap.entities.iter().any(|r| hp_editor::instances::is_instance(&r.entity));
        let instance_names =
            if self.has_instances { instance_target_names(doc, assets.fgd()) } else { Arc::new(Vec::new()) };
        let fgd_key = assets.fgd().map(|f| (assets.generation(), f.len()));
        if fgd_key != self.fgd_generation {
            self.classes = Arc::default();
            self.fgd_generation = fgd_key;
        }
        let classes = assets.fgd().filter(|f| !f.is_empty()).map(|fgd| {
            for r in &snap.entities {
                let key = r.entity.classname.trim().to_ascii_lowercase();
                if !self.classes.contains_key(&key) {
                    let facts = class_facts(fgd, &key);
                    Arc::make_mut(&mut self.classes).insert(key, facts);
                }
            }
            Arc::clone(&self.classes)
        });
        let generation = assets.generation();
        if generation != self.material_generation {
            self.materials = Arc::default();
            self.material_generation = generation;
        }
        self.unknown_materials.clear();
        if status == AssetStatus::Ready {
            for m in snap.materials.iter() {
                if self.materials.contains_key(&m.key) {
                    continue;
                }
                match assets.material_info(&m.display) {
                    Some(info) => {
                        Arc::make_mut(&mut self.materials).insert(m.key.clone(), material_facts(&info));
                    }
                    None => self.unknown_materials.push(m.display.clone()),
                }
            }
        }
        let fs = if status == AssetStatus::Ready { assets.fs().cloned() } else { None };
        Knowledge {
            classes,
            materials: Arc::clone(&self.materials),
            fs,
            answers: Arc::clone(&self.answers),
            deps_cache: Arc::clone(&self.deps_cache),
            instance_names,
        }
    }

    pub(crate) fn tick(
        &mut self,
        doc: &Document,
        assets: &mut dyn AssetsApi,
        profile: LimitProfile,
        live: bool,
        leak: LeakMode,
        pace: Pace,
    ) -> Tick {
        let force = pace.force;
        let mut out = Tick::default();
        if let Some(job) = &mut self.job {
            loop {
                match job.rx.try_recv() {
                    Ok(Msg::Checks(r)) => {
                        job.answered = true;
                        out.checks = Some(r);
                    }
                    Ok(Msg::Leak(r)) => {
                        job.leak_pending = false;
                        out.leak = Some(r);
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if !job.answered {
                            self.sent = None;
                        }
                        self.job = None;
                        break;
                    }
                }
            }
        }
        self.poll_materials(assets);
        if !live && !force && self.building.is_none() {
            out.busy = self.job.is_some();
            return out;
        }
        let want = self.want(doc, assets, profile, leak);
        if self.seen != Some(want) {
            if self.seen.is_some()
                && let Some(job) = &self.job
                && (job.leak_pending || !job.answered)
            {
                job.cancel.store(true, Ordering::Relaxed);
                self.sent = None;
            }
            self.seen = Some(want);
            self.changed_at = Instant::now();
        }
        let checked = self.sent == Some(want);
        if checked && !force && self.building.is_none() {
            out.busy = self.job.is_some();
            return out;
        }
        if pace.hold && self.building.is_none() {
            self.changed_at = Instant::now();
        }
        if !force && self.building.is_none() && (pace.hold || self.changed_at.elapsed() < pace.delay) {
            out.busy = true;
            return out;
        }
        self.building = Some(want);
        let Some(snap) = self.builder.step(doc, SLICE) else {
            out.busy = true;
            return out;
        };
        self.building = None;
        let know = self.knowledge(doc, assets, &snap);
        self.start_job(Arc::new(snap), know, profile, leak);
        self.sent = Some(self.want(doc, assets, profile, leak));
        self.seen = self.sent;
        out.busy = true;
        out
    }

    fn start_job(&mut self, snap: Arc<MapSnapshot>, know: Knowledge, profile: LimitProfile, leak: LeakMode) {
        if let Some(old) = self.job.take() {
            old.cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        let flag = Arc::clone(&cancel);
        crate::checks::watch(&cancel);
        let spawned = std::thread::Builder::new().name("hp-health".into()).spawn(move || {
            crate::checks::lowest_priority();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::checks::run(|| run_job(&snap, &know, profile, leak, &flag, &tx));
            }));
            if result.is_err() {
                log::error!("the map health checks failed (panic in a check)");
            }
        });
        match spawned {
            Ok(_) => self.job = Some(Job { cancel, rx, leak_pending: leak.on, answered: false }),
            Err(e) => log::error!("cannot start the map health worker: {e}"),
        }
    }

    pub(crate) fn check_now(
        &mut self,
        doc: &Document,
        assets: &mut dyn AssetsApi,
        profile: LimitProfile,
        leak: LeakMode,
    ) -> (CheckResult, Option<LeakResult>) {
        self.builder.restart();
        self.building = None;
        let snap = Arc::new(MapSnapshot::capture(doc));
        let know = self.knowledge(doc, assets, &snap);
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        run_job(&snap, &know, profile, leak, &cancel, &tx);
        self.sent = Some(self.want(doc, assets, profile, leak));
        self.seen = self.sent;
        let mut checks = None;
        let mut leak_result = None;
        while let Ok(m) = rx.try_recv() {
            match m {
                Msg::Checks(c) => checks = Some(c),
                Msg::Leak(l) => leak_result = Some(l),
            }
        }
        let checks = checks.unwrap_or_else(|| CheckResult {
            serial: snap.content_serial,
            problems: Vec::new(),
            limits: Vec::new(),
            edicts: EdictReport::default(),
            ms: 0,
            snapshot: Arc::clone(&snap),
        });
        (checks, leak_result)
    }
}

fn normalize(name: &str) -> String {
    let mut s = name.trim().replace('\\', "/").to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix("materials/") {
        s = rest.to_owned();
    }
    if let Some(rest) = s.strip_suffix(".vmt") {
        s = rest.to_owned();
    }
    s
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct LeakMode {
    pub on: bool,
    pub cordons: bool,
}

pub(crate) fn confirm_report(report: &mut LeakReport, verdict: crate::extend::FloodVerdict) {
    report.compiler = true;
    match verdict {
        None => {
            report.leak = None;
            report.leaking.clear();
            report.hull.clear();
        }
        Some((entity, points)) => {
            let instant = report.leak.take();
            let entity = entity.or(instant.as_ref().map(|l| l.entity)).unwrap_or(0);
            let points = if points.len() >= 2 { points } else { instant.map(|l| l.points).unwrap_or(points) };
            if entity != 0 && !report.leaking.contains(&entity) {
                report.leaking.insert(0, entity);
            }
            report.leak = Some(LeakPath { entity, points });
        }
    }
}

fn run_job(
    snap: &Arc<MapSnapshot>,
    know: &Knowledge,
    profile: LimitProfile,
    leak: LeakMode,
    cancel: &Arc<AtomicBool>,
    tx: &Sender<Msg>,
) {
    let t0 = Instant::now();
    know.resolve_assets(snap, cancel);
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let problems = check_snapshot(snap, know, &profile.check_options());
    let limits = estimate_limits(snap, know, profile);
    let edicts = estimate_edicts(snap);
    let ms = t0.elapsed().as_millis() as u64;
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let _ = tx.send(Msg::Checks(CheckResult {
        serial: snap.content_serial,
        problems,
        limits,
        edicts,
        ms,
        snapshot: Arc::clone(snap),
    }));
    if !leak.on {
        return;
    }
    let t1 = Instant::now();
    let boxes: &[hp_geom::Aabb] = if leak.cordons { &snap.cordons } else { &[] };
    let mut facts_cache: HashMap<u32, MaterialFacts> = HashMap::new();
    let mut input = leak_input_from(snap, &mut |m| {
        *facts_cache.entry(m).or_insert_with(|| {
            know.material(snap.material_display(m)).unwrap_or_else(|| MaterialFacts::guess(snap.material_key(m)))
        })
    });
    apply_cordons(&mut input, boxes);
    let Some(mut report) = find_leaks(&input, cancel) else { return };
    if report.leaks()
        && let Some(confirm) = crate::extend::leak_confirm()
    {
        let map = flood_map(snap, boxes);
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        match confirm(&map, know.fs.as_ref(), cancel) {
            Ok(Some(verdict)) => confirm_report(&mut report, verdict),
            Ok(None) => return,
            Err(e) => log::warn!("leak check: the compiler flood failed, the instant result is kept: {e}"),
        }
        if cancel.load(Ordering::Relaxed) {
            return;
        }
    }
    let mut candidates =
        if report.compiler && !report.leaks() { Vec::new() } else { detail_candidates(snap, &report, know) };
    if !candidates.is_empty() {
        let set: std::collections::HashSet<u32> = candidates.iter().copied().collect();
        let reduced = LeakInput {
            brushes: input.brushes.iter().filter(|b| !set.contains(&b.id)).cloned().collect(),
            seeds: input.seeds.clone(),
        };
        match find_leaks(&reduced, cancel) {
            Some(r) if !r.leaks() && !r.incomplete => {}
            Some(_) => candidates.clear(),
            None => return,
        }
    }
    let problems = leak_problems(snap, &report, &candidates);
    let ms = t1.elapsed().as_millis() as u64;
    let _ = tx.send(Msg::Leak(LeakResult { serial: snap.content_serial, report, problems, ms }));
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::*;

    fn leaking() -> LeakReport {
        LeakReport {
            leak: Some(LeakPath { entity: 7, points: vec![DVec3::X, DVec3::Y, DVec3::Z] }),
            leaking: vec![7, 8],
            hull: vec![1, 2],
            ..LeakReport::default()
        }
    }

    #[test]
    fn an_edit_cancels_a_stale_running_check() {
        let mut state = crate::testing::test_state();
        let mut engine = Engine::default();
        let (_tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        engine.job = Some(Job { cancel: Arc::clone(&cancel), rx, leak_pending: true, answered: true });
        let mode = LeakMode { on: true, cordons: false };
        let pace = Pace { delay: Duration::from_secs(3600), hold: false, force: false };
        let old = engine.want(&state.doc, state.assets.as_ref(), LimitProfile::Stock, mode);
        engine.seen = Some(old);
        engine.sent = Some(old);
        engine.tick(&state.doc, state.assets.as_mut(), LimitProfile::Stock, true, mode, pace);
        assert!(!cancel.load(Ordering::Relaxed), "nothing changed, the check goes on");
        state.doc.create_point_entity("info_target", DVec3::splat(8.0), &[]).expect("edit");
        engine.tick(&state.doc, state.assets.as_mut(), LimitProfile::Stock, true, mode, pace);
        assert!(cancel.load(Ordering::Relaxed), "the edit stops the outdated compiler flood");
        assert_eq!(engine.sent, None, "an undo back to the old state checks again");
    }

    #[test]
    fn the_compiler_flood_has_the_last_word() {
        let mut sealed = leaking();
        confirm_report(&mut sealed, None);
        assert!(!sealed.leaks() && sealed.compiler, "a false leak of the instant check is dropped");
        assert!(sealed.leaking.is_empty() && sealed.hull.is_empty());

        let mut real = leaking();
        let path = vec![DVec3::splat(-100.0), DVec3::splat(-50.0), DVec3::splat(4.0)];
        confirm_report(&mut real, Some((Some(9), path.clone())));
        let leak = real.leak.as_ref().expect("still a leak");
        assert_eq!((leak.entity, &leak.points), (9, &path), "the line follows the compiler's path");
        assert_eq!(real.leaking[0], 9);
        assert!(real.compiler);

        let mut short = leaking();
        confirm_report(&mut short, Some((None, vec![DVec3::ONE])));
        let leak = short.leak.as_ref().expect("still a leak");
        assert_eq!((leak.entity, leak.points.len()), (7, 3), "a path too short to draw keeps the instant one");
    }
}
