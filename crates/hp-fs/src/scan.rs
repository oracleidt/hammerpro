use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ahash::AHashMap;
use parking_lot::{Mutex, RwLock};
use rayon::prelude::*;

use crate::detect::path_key;
use crate::dirindex::{ASSET_DIRS, MAX_DEPTH, MAX_LINKS};
use crate::drive::{ScanMode, parallel_with, system_probe};
use crate::error::FsError;
use crate::scanfmt::{self, Entry, Node};

const RACY_NS: i64 = 3_000_000_000;

const KEEP_SECS: u64 = 30 * 24 * 3600;

const REFRESH_SECS: u64 = 24 * 3600;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanConfig {
    pub index_file: Option<PathBuf>,
    pub mode: ScanMode,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanStats {
    pub index_file: Option<PathBuf>,
    pub index_state: String,
    pub index_entries: usize,
    pub index_bytes: u64,
    pub dirs_checked: u64,
    pub dirs_reused: u64,
    pub dirs_rescanned: u64,
    pub walks: u64,
    pub last_walk_ms: u64,
    pub mode: ScanMode,
    pub volumes: Vec<(String, Option<bool>)>,
}

fn fingerprint() -> u64 {
    let mut text = String::new();
    for d in ASSET_DIRS {
        text.push_str(d);
        text.push(',');
    }
    text.push_str(&format!("{MAX_DEPTH}/{MAX_LINKS}"));
    scanfmt::fnv1a(text.as_bytes())
}

fn unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn unix_nanos() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).ok().and_then(|d| i64::try_from(d.as_nanos()).ok()).unwrap_or(0)
}

fn mtime_ns(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_nanos()).ok())
        .unwrap_or(0)
}

#[derive(Default)]
struct CacheState {
    loaded: bool,
    state: String,
    entries: AHashMap<String, Entry>,
    dirty: bool,
}

pub(crate) struct IndexCache {
    path: Option<PathBuf>,
    inner: Mutex<CacheState>,
}

impl IndexCache {
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        Self { path, inner: Mutex::new(CacheState::default()) }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.path.is_some()
    }

    fn ensure_loaded(&self, st: &mut CacheState) {
        if st.loaded {
            return;
        }
        st.loaded = true;
        let Some(path) = &self.path else {
            st.state = "disabled".to_owned();
            return;
        };
        match fs::read(path) {
            Ok(bytes) => match scanfmt::decode(&bytes, fingerprint()) {
                Ok(entries) => {
                    st.state = format!("loaded {} bytes", bytes.len());
                    st.entries = entries;
                }
                Err(e) => {
                    log::warn!("content index {} ignored, full scan: {e}", path.display());
                    st.state = format!("rejected ({e})");
                    st.dirty = true;
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => st.state = "none yet".to_owned(),
            Err(e) => {
                log::warn!("cannot read content index {}: {e}", path.display());
                st.state = format!("unreadable ({e})");
                st.dirty = true;
            }
        }
    }

    fn take(&self, key: &str) -> Option<Entry> {
        let mut st = self.inner.lock();
        self.ensure_loaded(&mut st);
        st.entries.remove(key)
    }

    fn put(&self, key: String, root: Node, changed: bool, previous_use: Option<u64>) {
        let now = unix_secs();
        let stale = previous_use.is_none_or(|u| now.saturating_sub(u) >= REFRESH_SECS);
        let mut st = self.inner.lock();
        if let Some(path) = key.strip_prefix("custom-assets|") {
            for profile in ["materials", "models"] {
                if st.entries.remove(&format!("{profile}|{path}")).is_some() {
                    st.dirty = true;
                }
            }
        }
        st.entries.insert(key, Entry { last_used: now, root });
        if changed || stale {
            st.dirty = true;
        }
    }

    pub(crate) fn clear(&self) {
        let mut st = self.inner.lock();
        st.loaded = true;
        st.entries.clear();
        st.state = "cleared".to_owned();
        st.dirty = false;
        if let Some(path) = &self.path
            && let Err(e) = fs::remove_file(path)
            && e.kind() != io::ErrorKind::NotFound
        {
            log::warn!("cannot delete content index {}: {e}", path.display());
        }
    }

    pub(crate) fn flush(&self) {
        let Some(path) = &self.path else { return };
        let mut st = self.inner.lock();
        if !st.loaded || !st.dirty {
            return;
        }
        let now = unix_secs();
        st.entries.retain(|_, e| now.saturating_sub(e.last_used) < KEEP_SECS);
        let bytes = scanfmt::encode(&st.entries, fingerprint());
        match write_atomic(path, &bytes) {
            Ok(()) => {
                st.dirty = false;
                st.state = format!("saved {} bytes", bytes.len());
            }
            Err(e) => log::warn!("cannot save content index {}: {e}", path.display()),
        }
    }

    fn summary(&self) -> (String, usize) {
        let st = self.inner.lock();
        (st.state.clone(), st.entries.len())
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let result = fs::write(&tmp, bytes).and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

struct Global {
    config: ScanConfig,
    cache: Arc<IndexCache>,
}

fn global() -> &'static RwLock<Global> {
    static GLOBAL: std::sync::OnceLock<RwLock<Global>> = std::sync::OnceLock::new();
    GLOBAL.get_or_init(|| RwLock::new(Global { config: ScanConfig::default(), cache: Arc::new(IndexCache::new(None)) }))
}

static CHECKED: AtomicU64 = AtomicU64::new(0);
static REUSED: AtomicU64 = AtomicU64::new(0);
static RESCANNED: AtomicU64 = AtomicU64::new(0);
static WALKS: AtomicU64 = AtomicU64::new(0);
static LAST_WALK_MS: AtomicU64 = AtomicU64::new(0);

pub fn configure(config: ScanConfig) {
    let mut g = global().write();
    if g.config.index_file != config.index_file {
        g.cache.flush();
        g.cache = Arc::new(IndexCache::new(config.index_file.clone()));
    }
    g.config = config;
}

pub fn scan_config() -> ScanConfig {
    global().read().config.clone()
}

pub fn clear_index() {
    global().read().cache.clear();
}

pub fn flush_index() {
    let cache = Arc::clone(&global().read().cache);
    cache.flush();
}

pub fn scan_stats() -> ScanStats {
    let g = global().read();
    let (index_state, index_entries) = g.cache.summary();
    let index_bytes = g.config.index_file.as_deref().and_then(|p| fs::metadata(p).ok()).map_or(0, |m| m.len());
    ScanStats {
        index_file: g.config.index_file.clone(),
        index_state,
        index_entries,
        index_bytes,
        dirs_checked: CHECKED.load(Ordering::Relaxed),
        dirs_reused: REUSED.load(Ordering::Relaxed),
        dirs_rescanned: RESCANNED.load(Ordering::Relaxed),
        walks: WALKS.load(Ordering::Relaxed),
        last_walk_ms: LAST_WALK_MS.load(Ordering::Relaxed),
        mode: g.config.mode,
        volumes: system_probe().known_volumes(),
    }
}

pub(crate) fn parallel_for(path: &Path) -> bool {
    let mode = global().read().config.mode;
    parallel_with(system_probe(), mode, path)
}

enum Kind {
    Dir { link: bool },
    File,
}

fn classify(entry: &fs::DirEntry) -> Option<(String, Kind)> {
    let name = entry.file_name().into_string().ok()?;
    let file_type = entry.file_type().ok()?;
    let link = file_type.is_symlink();
    let (is_dir, is_file) = if link {
        let meta = fs::metadata(entry.path()).ok()?;
        (meta.is_dir(), meta.is_file())
    } else {
        (file_type.is_dir(), file_type.is_file())
    };
    if is_dir {
        Some((name, Kind::Dir { link }))
    } else if is_file {
        Some((name, Kind::File))
    } else {
        None
    }
}

struct Ctx<'a> {
    top_dirs: Option<&'a [&'a str]>,
    keep: &'a (dyn Fn(&str) -> bool + Sync),
    parallel: bool,
    track: bool,
    now: i64,
    racy_ns: i64,
    checked: AtomicUsize,
    reused: AtomicUsize,
    rescanned: AtomicUsize,
}

struct Cand {
    name: Box<str>,
    link: bool,
    links: u32,
    old: Option<Node>,
}

fn scan_children(ctx: &Ctx, abs: &Path, depth: u32, cands: Vec<Cand>) -> Vec<Node> {
    let run = |c: Cand| {
        let path = abs.join(&*c.name);
        match scan_dir(ctx, &path, &c.name, c.link, depth + 1, c.links, c.old) {
            Ok(node) => node,
            Err(e) => {
                log::debug!("cannot list {}: {e}", path.display());
                Some(Node { name: c.name, link: c.link, ..Node::default() })
            }
        }
    };
    if ctx.parallel && cands.len() > 1 {
        cands.into_par_iter().filter_map(run).collect()
    } else {
        cands.into_iter().filter_map(run).collect()
    }
}

fn scan_dir(
    ctx: &Ctx,
    abs: &Path,
    name: &str,
    link: bool,
    depth: u32,
    links: u32,
    old: Option<Node>,
) -> io::Result<Option<Node>> {
    ctx.checked.fetch_add(1, Ordering::Relaxed);
    let mtime = if ctx.track {
        match fs::metadata(abs) {
            Ok(meta) => mtime_ns(&meta),
            Err(_) if old.is_some() => return Ok(None),
            Err(_) => 0,
        }
    } else {
        0
    };
    let old = match old {
        Some(o) if ctx.track && mtime != 0 && o.mtime == mtime && !o.racy => {
            ctx.reused.fetch_add(1, Ordering::Relaxed);
            let Node { files, dirs, .. } = o;
            let cands = dirs
                .into_iter()
                .map(|c| Cand { links: links + u32::from(c.link), link: c.link, name: c.name.clone(), old: Some(c) })
                .collect();
            let dirs = scan_children(ctx, abs, depth, cands);
            return Ok(Some(Node { name: name.into(), link, racy: false, mtime, files, dirs }));
        }
        other => other,
    };
    ctx.rescanned.fetch_add(1, Ordering::Relaxed);
    let read = fs::read_dir(abs)?;
    let mut old_dirs: AHashMap<Box<str>, Node> =
        old.map(|o| o.dirs.into_iter().map(|n| (n.name.clone(), n)).collect()).unwrap_or_default();
    let at_root = depth == 0;
    let mut files: Vec<Box<str>> = Vec::new();
    let mut cands: Vec<Cand> = Vec::new();
    for entry in read.flatten() {
        let Some((fname, kind)) = classify(&entry) else { continue };
        match kind {
            Kind::Dir { link: is_link } => {
                let wanted = !at_root
                    || ctx.top_dirs.is_none_or(|allowed| allowed.iter().any(|a| a.eq_ignore_ascii_case(&fname)));
                let child_links = links + u32::from(is_link);
                if wanted && depth < MAX_DEPTH && child_links <= MAX_LINKS && !fname.starts_with('.') {
                    cands.push(Cand {
                        old: old_dirs.remove(fname.as_str()),
                        name: fname.into_boxed_str(),
                        link: is_link,
                        links: child_links,
                    });
                }
            }
            Kind::File => {
                if (!at_root || ctx.top_dirs.is_none()) && (ctx.keep)(&fname) {
                    files.push(fname.into_boxed_str());
                }
            }
        }
    }
    let dirs = scan_children(ctx, abs, depth, cands);
    let racy = ctx.track && mtime != 0 && ctx.now.saturating_sub(mtime) < ctx.racy_ns;
    Ok(Some(Node { name: name.into(), link, racy, mtime, files, dirs }))
}

fn collect(node: &Node, rel: &str, out: &mut Vec<String>) {
    for f in &node.files {
        out.push(if rel.is_empty() { f.to_string() } else { format!("{rel}/{f}") });
    }
    for d in &node.dirs {
        let child = if rel.is_empty() { d.name.to_string() } else { format!("{rel}/{}", d.name) };
        collect(d, &child, out);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WalkReport {
    pub(crate) checked: usize,
    pub(crate) reused: usize,
    pub(crate) rescanned: usize,
}

pub(crate) fn walk(
    root: &Path,
    top_dirs: Option<&[&str]>,
    keep_file: &(dyn Fn(&str) -> bool + Sync),
    profile: Option<&str>,
) -> Result<Vec<String>, FsError> {
    let (cache, mode) = {
        let g = global().read();
        (Arc::clone(&g.cache), g.config.mode)
    };
    let parallel = parallel_with(system_probe(), mode, root);
    let start = Instant::now();
    let (files, report) = walk_in(&cache, parallel, RACY_NS, root, top_dirs, keep_file, profile)?;
    WALKS.fetch_add(1, Ordering::Relaxed);
    CHECKED.fetch_add(report.checked as u64, Ordering::Relaxed);
    REUSED.fetch_add(report.reused as u64, Ordering::Relaxed);
    RESCANNED.fetch_add(report.rescanned as u64, Ordering::Relaxed);
    LAST_WALK_MS.store(start.elapsed().as_millis() as u64, Ordering::Relaxed);
    Ok(files)
}

pub(crate) fn walk_in(
    cache: &IndexCache,
    parallel: bool,
    racy_ns: i64,
    root: &Path,
    top_dirs: Option<&[&str]>,
    keep_file: &(dyn Fn(&str) -> bool + Sync),
    profile: Option<&str>,
) -> Result<(Vec<String>, WalkReport), FsError> {
    let key = profile.filter(|_| cache.enabled()).map(|p| format!("{p}|{}", path_key(root)));
    let old = key.as_deref().and_then(|k| cache.take(k));
    let previous_use = old.as_ref().map(|e| e.last_used);
    let had_old = old.is_some();
    let ctx = Ctx {
        top_dirs,
        keep: keep_file,
        parallel,
        track: key.is_some(),
        now: unix_nanos(),
        racy_ns,
        checked: AtomicUsize::new(0),
        reused: AtomicUsize::new(0),
        rescanned: AtomicUsize::new(0),
    };
    let node = match scan_dir(&ctx, root, "", false, 0, 0, old.map(|e| e.root)) {
        Ok(Some(node)) => node,
        Ok(None) => return Err(FsError::io(root, io::Error::from(io::ErrorKind::NotFound))),
        Err(e) => return Err(FsError::io(root, e)),
    };
    let mut files = Vec::with_capacity(node.file_count());
    collect(&node, "", &mut files);
    let report = WalkReport {
        checked: ctx.checked.load(Ordering::Relaxed),
        reused: ctx.reused.load(Ordering::Relaxed),
        rescanned: ctx.rescanned.load(Ordering::Relaxed),
    };
    if let Some(key) = key {
        let changed = !had_old || report.rescanned > 0;
        cache.put(key, node, changed, previous_use);
    }
    files.par_sort_unstable();
    Ok((files, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    #[test]
    fn combined_custom_assets_retire_only_their_legacy_indexes_in_memory_and_on_disk() {
        let tmp = TempDir::new("combined-index-migration");
        let path = tmp.path().join("index.bin");
        let cache = IndexCache::new(Some(path.clone()));
        cache.ensure_loaded(&mut cache.inner.lock());
        let keys = ["materials|pack", "models|pack", "game|pack", "materials|other", "models|other"];
        for key in keys {
            cache.put(key.to_owned(), Node::default(), true, None);
        }
        cache.flush();
        assert!(!cache.inner.lock().dirty);
        cache.put("custom-assets|pack".to_owned(), Node::default(), false, Some(unix_secs()));
        {
            let state = cache.inner.lock();
            assert!(state.dirty);
            assert_eq!(state.entries.len(), 4);
            assert!(!state.entries.contains_key("materials|pack"));
            assert!(!state.entries.contains_key("models|pack"));
            for key in ["custom-assets|pack", "game|pack", "materials|other", "models|other"] {
                assert!(state.entries.contains_key(key));
            }
        }
        cache.flush();
        let restored = IndexCache::new(Some(path));
        let mut state = restored.inner.lock();
        restored.ensure_loaded(&mut state);
        assert_eq!(state.entries.len(), 4);
        assert!(!state.entries.contains_key("materials|pack"));
        assert!(!state.entries.contains_key("models|pack"));
        for key in ["custom-assets|pack", "game|pack", "materials|other", "models|other"] {
            assert!(state.entries.contains_key(key));
        }
    }
}
