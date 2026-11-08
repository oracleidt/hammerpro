use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crossbeam_channel::{Receiver, RecvTimeoutError};
use hp_editor::{Document, EditError};

use crate::tabs::OpenTarget;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenKind {
    Map,
    Recovered { original: Option<PathBuf> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl FileStamp {
    pub fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self { len: meta.len(), modified: meta.modified().ok() })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditMark {
    history: (usize, usize),
    bytes: usize,
    label: Option<String>,
    dirty: bool,
}

impl EditMark {
    pub fn of(doc: &Document) -> Self {
        Self {
            history: doc.history_len(),
            bytes: doc.history_bytes(),
            label: doc.undo_label().map(str::to_owned),
            dirty: doc.is_dirty(),
        }
    }

    pub fn edited_since(&self, doc: &Document) -> bool {
        doc.is_dirty() && Self::of(doc) != *self
    }
}

pub struct Loaded {
    pub path: PathBuf,
    pub kind: OpenKind,
    pub stamp: Option<FileStamp>,
    pub result: Result<Document, EditError>,
    pub timings: hp_vmf::LoadTimings,
    pub worker: Duration,
    pub requested: Instant,
    pub mark: EditMark,
    pub import: Option<hp_vmf::import_map::MapImportReport>,
}

impl std::fmt::Debug for Loaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loaded")
            .field("path", &self.path)
            .field("kind", &self.kind)
            .field("ok", &self.result.is_ok())
            .field("worker", &self.worker)
            .finish_non_exhaustive()
    }
}

struct Job {
    ticket: u64,
    path: PathBuf,
    kind: OpenKind,
    requested: Instant,
    rx: Receiver<(u64, Loaded)>,
}

#[derive(Default)]
pub struct Opener {
    next_ticket: u64,
    job: Option<Job>,
    held: Option<Loaded>,
    preparing: Option<Preparing>,
}

struct Preparing {
    loaded: Loaded,
    remaining: Option<std::collections::VecDeque<hp_app::runtime::PreloadResource>>,
    approved: bool,
}

pub fn is_map_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("map"))
}

const MAX_MAP_FILE: u64 = 1 << 30;

fn import_map_file(path: &Path) -> Result<(Document, hp_vmf::import_map::MapImportReport), EditError> {
    let len = std::fs::metadata(path).map_err(hp_vmf::VmfError::from)?.len();
    if len > MAX_MAP_FILE {
        return Err(EditError::Invalid("the file is too big".to_owned()));
    }
    let bytes = std::fs::read(path).map_err(hp_vmf::VmfError::from)?;
    let (map, report) = hp_vmf::import_map::parse_map_file(&bytes)?;
    let mut doc = Document::from_map(map);
    doc.mark_dirty();
    Ok((doc, report))
}

struct LoadOut {
    result: Result<Document, EditError>,
    timings: hp_vmf::LoadTimings,
    import: Option<hp_vmf::import_map::MapImportReport>,
}

fn open_pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
        rayon::ThreadPoolBuilder::new()
            .num_threads(cores.saturating_sub(1).max(1))
            .thread_name(|i| format!("hp-open-{i}"))
            .build()
            .map_err(|e| log::warn!("cannot make the open thread pool: {e}"))
            .ok()
    })
    .as_ref()
}

fn load(path: &Path) -> LoadOut {
    match open_pool() {
        Some(pool) => pool.install(|| load_here(path)),
        None => load_here(path),
    }
}

fn load_here(path: &Path) -> LoadOut {
    let run = || {
        if is_map_file(path) {
            match import_map_file(path) {
                Ok((doc, report)) => (Ok(doc), Some(report)),
                Err(e) => (Err(e), None),
            }
        } else {
            (Document::open(path), None)
        }
    };
    let (result, import) = std::panic::catch_unwind(run).unwrap_or_else(|_| {
        log::error!("loading {} panicked", path.display());
        (Err(EditError::Invalid("the file could not be read".to_owned())), None)
    });
    let timings = hp_vmf::last_load_timings();
    if let Ok(doc) = &result {
        warm_picking(doc);
    }
    LoadOut { result, timings, import }
}

fn warm_picking(doc: &Document) {
    let opts = hp_editor::PickOptions { include_entities: false, max_distance: 1.0, ..Default::default() };
    let ray = hp_geom::Ray::new(glam::DVec3::new(1.0e9, 1.0e9, 1.0e9), glam::DVec3::Z);
    let _ = doc.pick_ray(&ray, &opts, &|_| None);
}

impl Opener {
    pub fn is_loading(&self) -> bool {
        self.job.is_some() || self.preparing.is_some()
    }

    pub fn loading(&self) -> Option<(&Path, Duration)> {
        self.job
            .as_ref()
            .map(|j| (j.path.as_path(), j.requested.elapsed()))
            .or_else(|| self.preparing.as_ref().map(|p| (p.loaded.path.as_path(), p.loaded.requested.elapsed())))
    }

    pub fn start(&mut self, path: &Path, kind: OpenKind, mark: EditMark, ctx: Option<&egui::Context>) -> bool {
        if let Some(job) = &self.job
            && job.path == path
            && job.kind == kind
        {
            return false;
        }
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        let requested = Instant::now();
        let (tx, rx) = crossbeam_channel::bounded(1);
        let owned = path.to_path_buf();
        let worker_kind = kind.clone();
        let fallback_mark = mark.clone();
        let ctx = ctx.cloned();
        let spawned = std::thread::Builder::new().name("hp-open".to_owned()).spawn(move || {
            let stamp = FileStamp::of(&owned);
            let start = Instant::now();
            let out = load(&owned);
            let loaded = Loaded {
                path: owned,
                kind: worker_kind,
                stamp,
                result: out.result,
                timings: out.timings,
                worker: start.elapsed(),
                requested,
                mark,
                import: out.import,
            };
            if tx.send((ticket, loaded)).is_ok()
                && let Some(ctx) = ctx
            {
                ctx.request_repaint();
            }
        });
        match spawned {
            Ok(_) => {
                self.job = Some(Job { ticket, path: path.to_path_buf(), kind, requested, rx });
                true
            }
            Err(e) => {
                log::warn!("cannot start the open thread ({e}): loading on the UI thread");
                let stamp = FileStamp::of(path);
                let start = Instant::now();
                let out = load(path);
                let loaded = Loaded {
                    path: path.to_path_buf(),
                    kind: kind.clone(),
                    stamp,
                    result: out.result,
                    timings: out.timings,
                    worker: start.elapsed(),
                    requested,
                    mark: fallback_mark,
                    import: out.import,
                };
                let (tx, rx) = crossbeam_channel::bounded(1);
                let _ = tx.send((ticket, loaded));
                self.job = Some(Job { ticket, path: path.to_path_buf(), kind, requested, rx });
                true
            }
        }
    }

    pub fn cancel(&mut self) {
        self.job = None;
        if let Some(preparing) = self.preparing.take()
            && let Ok(doc) = preparing.loaded.result
        {
            crate::files::drop_in_background(doc);
        }
    }

    pub fn poll(&mut self) -> Option<Loaded> {
        let job = self.job.as_ref()?;
        match job.rx.try_recv() {
            Ok((ticket, loaded)) if ticket == job.ticket => {
                self.job = None;
                Some(loaded)
            }
            Ok(_) => None,
            Err(crossbeam_channel::TryRecvError::Empty) => None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.job = None;
                None
            }
        }
    }

    pub fn wait(&mut self, timeout: Duration) -> Option<Loaded> {
        let job = self.job.as_ref()?;
        match job.rx.recv_timeout(timeout) {
            Ok((ticket, loaded)) if ticket == job.ticket => {
                self.job = None;
                Some(loaded)
            }
            Ok(_) | Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                self.job = None;
                None
            }
        }
    }

    pub fn hold(&mut self, loaded: Loaded) {
        self.held = Some(loaded);
    }

    pub fn take_held(&mut self, path: &Path, kind: &OpenKind) -> Option<Loaded> {
        let held = self.held.take()?;
        let current = held.stamp.is_some() && FileStamp::of(path) == held.stamp;
        (held.path == path && held.kind == *kind && current).then_some(held)
    }

    pub fn drop_held(&mut self) {
        self.held = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hammerpro_opening_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn mark() -> EditMark {
        EditMark::of(&Document::new_empty())
    }

    #[test]
    fn documents_can_travel_between_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<Document>();
        assert_send::<Loaded>();
    }

    #[test]
    fn cancelling_resource_preload_does_not_replace_the_current_document() {
        let dir = scratch("preload_cancel");
        let path = dir.join("next.vmf");
        std::fs::write(&path, hp_app::testing::TEST_VMF).unwrap();
        let current = Document::new_empty();
        let revision = current.revision();
        let mut opener = Opener::default();
        opener.start(&path, OpenKind::Map, EditMark::of(&current), None);
        let loaded = opener.wait(Duration::from_secs(30)).unwrap();
        opener.preparing = Some(Preparing { loaded, remaining: Some(Default::default()), approved: false });
        assert!(opener.is_loading());
        assert_eq!(opener.loading().map(|(path, _)| path.to_owned()), Some(path));
        opener.cancel();
        assert!(!opener.is_loading());
        assert!(opener.loading().is_none());
        assert_eq!(current.revision(), revision);
        assert!(current.scene().solids().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_map_loads_on_a_worker_and_comes_back() {
        let dir = scratch("load");
        let path = dir.join("room.vmf");
        std::fs::write(&path, hp_app::testing::TEST_VMF).expect("write");
        let mut opener = Opener::default();
        assert!(opener.start(&path, OpenKind::Map, mark(), None));
        assert!(opener.is_loading());
        assert_eq!(opener.loading().map(|(p, _)| p.to_path_buf()), Some(path.clone()));
        assert!(!opener.start(&path, OpenKind::Map, mark(), None));
        let loaded = opener.wait(Duration::from_secs(30)).expect("loaded");
        assert!(!opener.is_loading());
        let doc = loaded.result.as_ref().expect("parsed");
        assert!(!doc.scene().solids().is_empty());
        assert_eq!(doc.path(), Some(path.as_path()));
        assert_eq!(loaded.timings.bytes, hp_app::testing::TEST_VMF.len() as u64);
        assert!(loaded.worker >= loaded.timings.read);
        assert!(loaded.stamp.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_request_replaces_the_running_one() {
        let dir = scratch("replace");
        let (a, b) = (dir.join("a.vmf"), dir.join("b.vmf"));
        std::fs::write(&a, hp_app::testing::TEST_VMF).expect("write");
        std::fs::write(&b, hp_app::testing::TEST_VMF).expect("write");
        let mut opener = Opener::default();
        assert!(opener.start(&a, OpenKind::Map, mark(), None));
        assert!(opener.start(&b, OpenKind::Map, mark(), None));
        let loaded = opener.wait(Duration::from_secs(30)).expect("loaded");
        assert_eq!(loaded.path, b, "only the last request is installed");
        assert!(opener.poll().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_come_back_too() {
        let dir = scratch("errors");
        let mut opener = Opener::default();
        assert!(opener.start(&dir.join("missing.vmf"), OpenKind::Map, mark(), None));
        let loaded = opener.wait(Duration::from_secs(30)).expect("answered");
        assert!(loaded.result.is_err());
        let bin = dir.join("bin.vmf");
        std::fs::write(&bin, [0u8, 1, 2, 3, 0, 0, 0, 255].repeat(64)).expect("write");
        assert!(opener.start(&bin, OpenKind::Map, mark(), None));
        assert!(opener.wait(Duration::from_secs(30)).expect("answered").result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_held_map_is_reused_only_while_the_file_is_unchanged() {
        let dir = scratch("held");
        let path = dir.join("room.vmf");
        std::fs::write(&path, hp_app::testing::TEST_VMF).expect("write");
        let mut opener = Opener::default();
        opener.start(&path, OpenKind::Map, mark(), None);
        let loaded = opener.wait(Duration::from_secs(30)).expect("loaded");
        opener.hold(loaded);
        assert!(opener.take_held(&dir.join("other.vmf"), &OpenKind::Map).is_none(), "another file");
        assert!(opener.take_held(&path, &OpenKind::Map).is_none(), "taking a wrong one drops the held map");
        opener.start(&path, OpenKind::Map, mark(), None);
        let loaded = opener.wait(Duration::from_secs(30)).expect("loaded");
        opener.hold(loaded);
        assert!(opener.take_held(&path, &OpenKind::Map).is_some());
        opener.start(&path, OpenKind::Map, mark(), None);
        let loaded = opener.wait(Duration::from_secs(30)).expect("loaded");
        opener.hold(loaded);
        std::fs::write(&path, format!("{}\n", hp_app::testing::TEST_VMF)).expect("write");
        assert!(opener.take_held(&path, &OpenKind::Map).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quake_maps_open_as_new_documents() {
        let dir = scratch("quake");
        let path = dir.join("old.map");
        let text = "{\n\"classname\" \"worldspawn\"\n{\n\
( 0 0 0 ) ( 0 1 0 ) ( 1 0 0 ) a 0 0 0 1 1\n\
( 0 0 64 ) ( 1 0 64 ) ( 0 1 64 ) a 0 0 0 1 1\n\
( 0 0 0 ) ( 0 0 1 ) ( 0 1 0 ) a 0 0 0 1 1\n\
( 64 0 0 ) ( 64 1 0 ) ( 64 0 1 ) a 0 0 0 1 1\n\
( 0 0 0 ) ( 1 0 0 ) ( 0 0 1 ) a 0 0 0 1 1\n\
( 0 64 0 ) ( 0 64 1 ) ( 1 64 0 ) a 0 0 0 1 1\n}\n}\n";
        std::fs::write(&path, text).expect("write");
        let mut opener = Opener::default();
        opener.start(&path, OpenKind::Map, mark(), None);
        let loaded = opener.wait(Duration::from_secs(30)).expect("loaded");
        let report = loaded.import.clone().expect("imported");
        assert_eq!(report.brushes, 1);
        let doc = loaded.result.expect("document");
        assert_eq!(doc.path(), None, "saving asks for a VMF");
        assert!(doc.is_dirty());
        assert_eq!(doc.scene().solids().len(), 1);
        assert!(is_map_file(Path::new("A.MAP")) && !is_map_file(Path::new("a.vmf")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edit_marks_ignore_the_selection() {
        let mut doc = Document::from_map(hp_vmf::parse(hp_app::testing::TEST_VMF).expect("parses"));
        let mark = EditMark::of(&doc);
        let id = *doc.scene().solids().keys().next().expect("a solid");
        doc.select(&[hp_editor::ObjRef::Solid(id)], hp_editor::SelectOp::Replace);
        assert!(!mark.edited_since(&doc), "selecting is not editing");
        doc.delete_selection().expect("delete");
        assert!(mark.edited_since(&doc));
    }
}

#[derive(Debug)]
pub struct Bench {
    map: PathBuf,
    open: Option<crate::timing::OpenReport>,
    drawn: Option<Instant>,
    done: bool,
}

const BENCH_SETTLE_TIMEOUT: Duration = Duration::from_secs(120);

impl Bench {
    pub fn new(map: PathBuf) -> Self {
        Self { map, open: None, drawn: None, done: false }
    }

    pub fn map(&self) -> &Path {
        &self.map
    }
}

const MAX_OPEN_QUEUE: usize = 32;

impl crate::app::HammerProApp {
    pub(crate) fn start_open(&mut self, ctx: &egui::Context, path: &Path, kind: OpenKind, target: OpenTarget) {
        if let Some(held) = self.opener.take_held(path, &kind) {
            self.opener.preparing = Some(Preparing { loaded: held, remaining: None, approved: true });
            self.open_target = Some(target);
            self.update_open_status();
            return;
        }
        if self.state.runtime.startup.resources.is_some() && !self.opener.is_loading() {
            if self.open_queue.len() < MAX_OPEN_QUEUE {
                self.open_queue.push_back((path.to_path_buf(), kind, target));
            }
            return;
        }
        if let Some((loading, _)) = self.opener.loading()
            && loading != path
        {
            if self.open_queue.len() < MAX_OPEN_QUEUE {
                self.open_queue.push_back((path.to_path_buf(), kind, target));
            }
            return;
        }
        let mark = EditMark::of(&self.state.doc);
        if self.opener.start(path, kind, mark, Some(ctx)) {
            self.open_longest_frame = Duration::ZERO;
        }
        self.open_target = Some(target);
        self.update_open_status();
    }

    pub(crate) fn poll_open(&mut self, ctx: &egui::Context) {
        if self
            .state
            .runtime
            .startup
            .resources
            .as_ref()
            .is_some_and(|p| p.installing && self.state.frame.index > p.installed_frame.saturating_add(1))
            && self.state.assets.scene_prepared()
        {
            self.state.runtime.startup.resources = None;
        }
        if std::mem::take(&mut self.state.runtime.startup.cancel_open) {
            if !self.state.runtime.startup.resources.as_ref().is_some_and(|p| p.installing) {
                self.opener.cancel();
                self.open_queue.clear();
                self.open_target = None;
                self.state.runtime.startup.resources = None;
            }
        }
        if let Some(loaded) = self.opener.poll() {
            if loaded.result.is_err() {
                self.finish_open(loaded);
            } else {
                self.opener.preparing = Some(Preparing { loaded, remaining: None, approved: false });
            }
        }
        self.poll_resource_preload(ctx);
        if !self.opener.is_loading()
            && self.state.runtime.startup.resources.is_none()
            && self.pending.is_none()
            && let Some((path, kind, target)) = self.open_queue.pop_front()
        {
            match self.tabs.find_map(&self.state, &path) {
                Some(id) if kind == OpenKind::Map => self.switch_to_tab(id),
                _ => self.start_open(ctx, &path, kind, target),
            }
        }
        self.update_open_status();
    }

    fn poll_resource_preload(&mut self, ctx: &egui::Context) {
        use hp_app::runtime::{PreloadResult, ResourceLoading};
        let Some(preparing) = self.opener.preparing.as_mut() else { return };
        if !self.state.assets.preload_manifest_ready() {
            ctx.request_repaint_after(Duration::from_millis(25));
            return;
        }
        let remaining = preparing.remaining.get_or_insert_with(|| {
            let doc = preparing.loaded.result.as_ref().expect("a parsed document is prepared");
            let resources = self.state.assets.preload_resources(doc);
            self.state.runtime.startup.resources =
                Some(ResourceLoading { total: resources.len().saturating_add(1), ..Default::default() });
            resources.into()
        });
        let progress = self.state.runtime.startup.resources.as_mut().expect("resource progress");
        let started = Instant::now();
        let attempts = remaining.len();
        if let Some(gfx) = self.state.gfx.as_mut() {
            for _ in 0..attempts {
                let Some(resource) = remaining.pop_front() else { break };
                let result = self.state.assets.preload_resource(&resource, gfx);
                progress.current = resource.name().to_owned();
                match result {
                    PreloadResult::Pending => remaining.push_back(resource),
                    PreloadResult::Ready | PreloadResult::Missing => {
                        progress.done += 1;
                        if result == PreloadResult::Missing {
                            self.state
                                .console
                                .push(hp_app::Severity::Warning, hp_app::tf!("shell.loading.missing", resource.name()));
                        }
                        if progress.log.len() == 6 {
                            progress.log.remove(0);
                        }
                        progress.log.push((resource.name().to_owned(), result == PreloadResult::Missing));
                    }
                }
                if started.elapsed() >= Duration::from_millis(4) {
                    break;
                }
            }
        }
        if remaining.is_empty() {
            progress.installing = true;
            progress.installed_frame = self.state.frame.index;
            let preparing = self.opener.preparing.take().expect("prepared document");
            if preparing.approved {
                let target = self.open_target.take().unwrap_or(OpenTarget::NewTab);
                self.enter_open_target(target);
                self.install_open(preparing.loaded);
            } else {
                self.finish_open(preparing.loaded);
            }
            if self.pending.is_some() {
                self.state.runtime.startup.resources = None;
            }
        } else {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn update_open_status(&mut self) {
        self.state.runtime.startup.opening = self.opener.loading().map(|(path, elapsed)| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            hp_app::runtime::OpeningStatus { name, elapsed }
        });
    }

    pub(crate) fn finish_open(&mut self, loaded: Loaded) {
        let target = self.open_target.take().unwrap_or(OpenTarget::NewTab);
        if loaded.result.is_err() {
            self.install_open(loaded);
            return;
        }
        self.enter_open_target(target);
        let replaces = matches!(target, OpenTarget::Replace(id) if id == self.tabs.active_id());
        if replaces && loaded.mark.edited_since(&self.state.doc) {
            let op = match &loaded.kind {
                OpenKind::Map => crate::files::PendingOp::Open(loaded.path.clone()),
                OpenKind::Recovered { original } => {
                    crate::files::PendingOp::Recover { autosave: loaded.path.clone(), original: original.clone() }
                }
            };
            self.opener.hold(loaded);
            self.pending = Some(op);
            return;
        }
        self.install_open(loaded);
    }

    fn install_open(&mut self, loaded: Loaded) {
        let name = loaded.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut report = crate::timing::OpenReport::new(name, loaded.requested, &loaded.timings, loaded.worker);
        report.longest_frame = self.open_longest_frame;
        let Loaded { path, kind, result, requested, import, .. } = loaded;
        let installed =
            crate::files::install_loaded(&mut self.state, &path, &kind, result, requested, self.central_size);
        self.tabs.touch();
        self.update_open_status();
        if installed && let Some(report) = import {
            crate::files::announce_import(&mut self.state, &report);
        }
        if !installed {
            if let Some(bench) = &mut self.bench {
                println!("bench-open failed: {}", path.display());
                bench.done = true;
            }
            return;
        }
        report.timeline.mark("install");
        report.sync_count = self.sync_count;
        self.open_report = Some(report);
        if kind == OpenKind::Map && !is_map_file(&path) {
            self.offer_recovery(&path);
        }
    }

    pub(crate) fn after_frame(&mut self, ctx: &egui::Context, frame_time: Duration) {
        if self.opener.is_loading() || self.open_report.is_some() {
            self.open_longest_frame = self.open_longest_frame.max(frame_time);
        }
        if let Some(mut report) = self.open_report.take_if(|r| self.sync_count > r.sync_count) {
            report.longest_frame = self.open_longest_frame;
            report.sync = self.last_sync;
            report.timeline.mark("first_draw");
            report.log();
            if let Some(bench) = &mut self.bench {
                bench.open = Some(report);
                bench.drawn = Some(Instant::now());
            }
        }

        let status = self.state.assets.status();
        let content_known = !matches!(status, hp_app::state::AssetStatus::Mounting);
        let since_start = crate::timing::process_start().elapsed();
        let fgd_known = self.state.assets.fgd().is_some()
            || (content_known && status != hp_app::state::AssetStatus::Ready)
            || since_start > Duration::from_secs(20);
        if let Some(first) = self.startup.after_frame(content_known, fgd_known) {
            self.state.log(hp_app::Severity::Info, hp_app::tf!("app.started", crate::timing::ms(first)));
        }
        self.bench_tick(ctx);
    }

    fn bench_tick(&mut self, ctx: &egui::Context) {
        let Some(bench) = &mut self.bench else { return };
        if bench.done {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        ctx.request_repaint();
        let Some(drawn) = bench.drawn else { return };
        let mounting = matches!(self.state.assets.status(), hp_app::state::AssetStatus::Mounting);
        let busy = mounting || self.state.assets.pending_work() > 0;
        let timed_out = drawn.elapsed() > BENCH_SETTLE_TIMEOUT;
        if busy && !timed_out {
            return;
        }
        let Some(open) = &bench.open else { return };
        let stats = self.state.doc.stats();
        let settled = if timed_out { "timeout".to_owned() } else { crate::timing::ms(drawn.elapsed()).to_string() };
        println!(
            "bench-open map={} bytes={} solids={} faces={} entities={} {} {} sync_ms={} longest_frame_ms={} settled_after_draw_ms={}",
            open.name,
            open.bytes,
            stats.solids,
            stats.faces,
            stats.entities,
            self.startup.timeline().key_values("startup_"),
            open.timeline.key_values("open_"),
            crate::timing::ms(open.sync),
            crate::timing::ms(open.longest_frame),
            settled,
        );
        bench.done = true;
    }
}
