pub mod model;
mod view;
mod work;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hp_app::runtime::{AssetCollectRequest, AssetCollectSummary};
use hp_app::{AppState, Severity, t, tf};
use hp_fs::collect::{ExistingFiles, UnusedReport};
use hp_fs::deps::{DepRoot, DependencyGraph, DepsCache};

pub use model::{AssetTree, Entry, Filter, Group, Item, Show, Totals, is_base};
pub use work::ResolveInput;
use work::{CollectInput, check_output, map_name};

type Resolved = Result<(DependencyGraph, Vec<String>), String>;

pub const TAB: &str = "map_assets";

const SETTLE: Duration = Duration::from_millis(800);

struct Running<T> {
    rx: Receiver<T>,
    cancel: Arc<AtomicBool>,
}

impl<T: Send + 'static> Running<T> {
    fn spawn(name: &str, f: impl FnOnce(&AtomicBool) -> T + Send + 'static) -> Option<Self> {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let spawned = std::thread::Builder::new().name(format!("hp-map-assets-{name}")).spawn(move || {
            let _ = tx.send(f(&flag));
        });
        match spawned {
            Ok(_) => Some(Running { rx, cancel }),
            Err(e) => {
                log::error!("cannot start the map assets job {name}: {e}");
                None
            }
        }
    }

    fn poll(&self) -> Option<Result<T, ()>> {
        match self.rx.try_recv() {
            Ok(v) => Some(Ok(v)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct BuildKey {
    roots: Vec<DepRoot>,
    instances: Vec<(String, String)>,
    vmf: Option<PathBuf>,
    generation: u64,
}

struct CollectJob {
    job: Running<AssetCollectSummary>,
    done: Arc<AtomicUsize>,
    total: usize,
}

struct Pending {
    input: CollectInput,
    existing: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickFor {
    Output,
    Unused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Group(Group),
    Entry(usize),
    Child(Item, u8),
}

pub struct MapAssetsPanel {
    tree: Option<AssetTree>,
    warnings: Vec<String>,
    error: Option<String>,
    cache: Arc<DepsCache>,
    cache_for: Option<Option<PathBuf>>,
    resolve: Option<Running<Resolved>>,
    resolving_key: Option<BuildKey>,
    built_for: Option<BuildKey>,
    seen_revision: Option<u64>,
    changed_at: Option<Instant>,
    tried_generation: Option<u64>,
    refresh_requested: bool,
    filter: Filter,
    unchecked: HashSet<String>,
    expanded: HashSet<String>,
    collapsed: [bool; 5],
    visible: Vec<usize>,
    rows: Vec<Row>,
    rows_dirty: bool,
    totals: Totals,
    output: String,
    output_loaded: bool,
    base_game: bool,
    replace: bool,
    pick: Option<(PickFor, Receiver<Option<PathBuf>>)>,
    pending: Option<Pending>,
    collect: Option<CollectJob>,
    request: Option<AssetCollectRequest>,
    unused_folder: String,
    unused_job: Option<Running<Result<UnusedReport, String>>>,
    unused: Option<Result<UnusedReport, String>>,
    unused_filter: String,
    bottom_h: f32,
}

impl Default for MapAssetsPanel {
    fn default() -> Self {
        MapAssetsPanel {
            tree: None,
            warnings: Vec::new(),
            error: None,
            cache: Arc::new(DepsCache::new()),
            cache_for: None,
            resolve: None,
            resolving_key: None,
            built_for: None,
            seen_revision: None,
            changed_at: None,
            tried_generation: None,
            refresh_requested: false,
            filter: Filter::default(),
            unchecked: HashSet::new(),
            expanded: HashSet::new(),
            collapsed: [false; 5],
            visible: Vec::new(),
            rows: Vec::new(),
            rows_dirty: true,
            totals: Totals::default(),
            output: String::new(),
            output_loaded: false,
            base_game: false,
            replace: false,
            pick: None,
            pending: None,
            collect: None,
            request: None,
            unused_folder: String::new(),
            unused_job: None,
            unused: None,
            unused_filter: String::new(),
            bottom_h: 200.0,
        }
    }
}

impl MapAssetsPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tree(&self) -> Option<&AssetTree> {
        self.tree.as_ref()
    }

    pub fn is_busy(&self) -> bool {
        self.resolve.is_some() || self.collect.is_some() || self.unused_job.is_some()
    }

    pub fn tick(&mut self, ctx: &egui::Context, state: &mut AppState, visible: bool) {
        self.poll_resolve();
        self.poll_collect(state);
        self.poll_unused();
        self.poll_pick();
        let rt = &mut state.runtime.content.map_assets;
        if let Some(req) = rt.collect_request.take() {
            self.request = Some(req);
        }
        if std::mem::take(&mut rt.cancel_requested) {
            if let Some(c) = &self.collect {
                c.job.cancel.store(true, Ordering::Relaxed);
            }
            self.request = None;
        }
        if visible || self.request.is_some() {
            self.keep_up_to_date(state);
        }
        if self.request.is_some()
            && self.resolve.is_none()
            && self.collect.is_none()
            && self.changed_at.is_none()
            && let Some(req) = self.request.take()
        {
            self.start_request(state, &req);
        }
        if let Some(c) = &self.collect {
            state.runtime.content.map_assets.progress = Some((c.done.load(Ordering::Relaxed), c.total));
        }
        if self.is_busy() || self.pick.is_some() || self.request.is_some() || self.changed_at.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub fn refresh(&mut self) {
        self.refresh_requested = true;
    }

    fn keep_up_to_date(&mut self, state: &AppState) {
        let rev = state.doc.revision();
        if self.seen_revision != Some(rev) {
            if self.seen_revision.is_some() {
                self.changed_at = Some(Instant::now());
            }
            self.seen_revision = Some(rev);
        }
        if self.resolve.is_some() {
            return;
        }
        let generation = state.assets.generation();
        let settled = self.changed_at.is_some_and(|t| t.elapsed() >= SETTLE);
        let due = self.refresh_requested || self.tried_generation != Some(generation) || settled;
        if !due {
            return;
        }
        let forced = std::mem::take(&mut self.refresh_requested);
        self.changed_at = None;
        self.tried_generation = Some(generation);
        let map = state.doc.path().map(Path::to_path_buf);
        if self.cache_for.as_ref() != Some(&map) {
            self.cache = Arc::new(DepsCache::new());
            self.cache_for = Some(map);
        }
        let input = match ResolveInput::of(state, self.cache.clone()) {
            Ok(i) => i,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let key = BuildKey {
            roots: input.roots.clone(),
            instances: input.instances.clone(),
            vmf: input.vmf.clone(),
            generation,
        };
        if !forced && self.built_for.as_ref() == Some(&key) && self.tree.is_some() {
            return;
        }
        self.resolving_key = Some(key);
        self.resolve = Running::spawn("resolve", move |cancel| input.resolve(cancel));
    }

    fn poll_resolve(&mut self) {
        let Some(job) = &self.resolve else { return };
        let Some(result) = job.poll() else { return };
        self.resolve = None;
        match result {
            Ok(Ok((graph, warnings))) => {
                self.tree = Some(AssetTree::build(Arc::new(graph)));
                self.warnings = warnings;
                self.error = None;
                self.built_for = self.resolving_key.take();
                self.rows_dirty = true;
            }
            Ok(Err(e)) => {
                self.error = Some(e);
                self.resolving_key = None;
            }
            Err(()) => {
                self.error = Some(t!("mapassets.err_worker").to_owned());
                self.resolving_key = None;
            }
        }
    }

    fn poll_collect(&mut self, state: &mut AppState) {
        let Some(c) = &self.collect else { return };
        let Some(result) = c.job.poll() else { return };
        self.collect = None;
        let rt = &mut state.runtime.content.map_assets;
        rt.progress = None;
        let summary = result.unwrap_or_else(|()| AssetCollectSummary {
            error: Some(t!("mapassets.err_worker").to_owned()),
            ..AssetCollectSummary::default()
        });
        match &summary.error {
            Some(e) => state.notify(Severity::Error, e.clone()),
            None if summary.cancelled => state.notify(Severity::Warning, t!("mapassets.collect_cancelled").to_owned()),
            None => state.notify(
                if summary.failed.is_empty() { Severity::Info } else { Severity::Warning },
                tf!("mapassets.collect_done", summary.copied + summary.replaced, summary.output.display()),
            ),
        }
        state.runtime.content.map_assets.last_collect = Some(summary);
    }

    fn poll_unused(&mut self) {
        let Some(job) = &self.unused_job else { return };
        let Some(result) = job.poll() else { return };
        self.unused_job = None;
        self.unused = Some(result.unwrap_or_else(|()| Err(t!("mapassets.err_worker").to_owned())));
    }

    fn poll_pick(&mut self) {
        let Some((target, rx)) = &self.pick else { return };
        let picked = match rx.try_recv() {
            Ok(p) => p,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => None,
        };
        let target = *target;
        self.pick = None;
        let Some(path) = picked else { return };
        match target {
            PickFor::Output => self.output = path.display().to_string(),
            PickFor::Unused => {
                self.unused_folder = path.display().to_string();
                self.unused = None;
            }
        }
    }

    fn open_picker(&mut self, target: PickFor, start: &str) {
        if self.pick.is_some() {
            return;
        }
        let title = match target {
            PickFor::Output => t!("mapassets.pick_output"),
            PickFor::Unused => t!("mapassets.pick_unused"),
        };
        let mut dialog = rfd::FileDialog::new().set_title(title);
        let start = Path::new(start.trim());
        if start.is_dir() {
            dialog = dialog.set_directory(start);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        if cfg!(target_os = "macos") {
            let _ = tx.send(dialog.pick_folder());
        } else {
            let spawned = std::thread::Builder::new().name("hp-map-assets-pick".into()).spawn(move || {
                let _ = tx.send(dialog.pick_folder());
            });
            if spawned.is_err() {
                return;
            }
        }
        self.pick = Some((target, rx));
    }

    fn selected(&self) -> Vec<usize> {
        let Some(tree) = &self.tree else { return Vec::new() };
        self.visible.iter().copied().filter(|&i| !self.unchecked.contains(tree.name(tree.entries[i].item))).collect()
    }

    fn plan(
        &self,
        state: &AppState,
        entries: &[usize],
        output: &Path,
        base_game: bool,
        replace: bool,
    ) -> Result<Pending, String> {
        let tree = self.tree.as_ref().ok_or_else(|| t!("mapassets.not_ready").to_owned())?;
        let fs = state.assets.fs().cloned().ok_or_else(|| t!("mapassets.no_content").to_owned())?;
        if let Some(e) = check_output(&fs, output) {
            return Err(e);
        }
        let (nodes, missing) = tree.files_of(entries, base_game);
        if nodes.is_empty() && missing.is_empty() {
            return Err(t!("mapassets.nothing").to_owned());
        }
        let existing = nodes
            .iter()
            .filter(|&&i| {
                let vpath = &tree.graph.nodes[i].vpath;
                vpath.split('/').fold(output.to_path_buf(), |p, c| p.join(c)).is_file()
            })
            .count();
        let input = CollectInput {
            fs,
            graph: tree.graph.clone(),
            nodes,
            missing,
            output: output.to_path_buf(),
            existing: if replace { ExistingFiles::Replace } else { ExistingFiles::Skip },
            map: map_name(state),
        };
        Ok(Pending { input, existing })
    }

    fn start(&mut self, input: CollectInput) {
        let total = input.nodes.len();
        let done = Arc::new(AtomicUsize::new(0));
        let counter = done.clone();
        let job = Running::spawn("collect", move |cancel| {
            let progress = |d: usize, _: usize| counter.store(d, Ordering::Relaxed);
            work::collect(&input, cancel, &progress)
        });
        if let Some(job) = job {
            self.collect = Some(CollectJob { job, done, total });
        }
    }

    fn start_request(&mut self, state: &mut AppState, req: &AssetCollectRequest) {
        let Some(tree) = &self.tree else {
            let error = self.error.clone().unwrap_or_else(|| t!("mapassets.not_ready").to_owned());
            state.runtime.content.map_assets.last_collect =
                Some(AssetCollectSummary { output: req.output.clone(), error: Some(error), ..Default::default() });
            return;
        };
        let mut groups = [req.groups.is_empty(); 5];
        for g in req.groups.iter().filter_map(|g| Group::from_id(g)) {
            groups[g.index()] = true;
        }
        let show = if req.base_game { Show::All } else { Show::Custom };
        let entries = tree.visible(&Filter { show, groups, text: req.filter.clone() });
        match self.plan(state, &entries, &req.output, req.base_game, req.replace) {
            Ok(p) => self.start(p.input),
            Err(e) => {
                state.runtime.content.map_assets.last_collect =
                    Some(AssetCollectSummary { output: req.output.clone(), error: Some(e), ..Default::default() });
            }
        }
    }

    fn collect_clicked(&mut self, state: &mut AppState) {
        let output = PathBuf::from(self.output.trim());
        let entries = self.selected();
        match self.plan(state, &entries, &output, self.base_game, self.replace) {
            Ok(p) if p.existing > 0 && self.replace => self.pending = Some(p),
            Ok(p) => {
                state.settings.content.collect_folder = output.display().to_string();
                state.settings_dirty = true;
                self.start(p.input);
            }
            Err(e) => state.notify(Severity::Warning, e),
        }
    }

    fn check_unused(&mut self) {
        let Some(tree) = &self.tree else { return };
        let graph = tree.graph.clone();
        let folder = PathBuf::from(self.unused_folder.trim());
        self.unused = None;
        self.unused_job = Running::spawn("unused", move |cancel| work::unused(&graph, &folder, cancel));
    }
}

pub fn resolve_tree(input: &ResolveInput) -> Result<AssetTree, String> {
    let (graph, _) = input.resolve(&AtomicBool::new(false))?;
    Ok(AssetTree::build(Arc::new(graph)))
}

pub fn unused_report(tree: &AssetTree, folder: &Path) -> Result<UnusedReport, String> {
    work::unused(&tree.graph, folder, &AtomicBool::new(false))
}

pub fn new_cache() -> Arc<DepsCache> {
    Arc::new(DepsCache::new())
}

pub fn origin_label(node: &hp_fs::deps::DepNode) -> &'static str {
    crate::release::origin_name(node.origin)
}

pub fn output_problem(state: &AppState, output: &Path) -> Option<String> {
    match state.assets.fs() {
        Some(fs) => check_output(fs, output),
        None => Some(t!("mapassets.no_content").to_owned()),
    }
}

#[cfg(test)]
mod tests;
