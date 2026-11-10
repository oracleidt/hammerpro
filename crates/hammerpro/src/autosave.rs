use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hp_app::settings::Settings;
use hp_app::{AppState, Severity, t};

pub use hp_app::sidecar::map_key;

pub fn file_name(key: &str, secs: u64) -> String {
    format!("{key}_{secs}.vmf")
}

pub fn parse_file_name(name: &str, key: &str) -> Option<u64> {
    let rest = name.strip_prefix(key)?.strip_prefix('_')?;
    let digits = rest.strip_suffix(".vmf").or_else(|| rest.strip_suffix(".VMF"))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

pub fn list(dir: &Path, key: &str) -> Vec<(u64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| Some((parse_file_name(&e.file_name().to_string_lossy(), key)?, e.path())))
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    files
}

pub fn prune(dir: &Path, key: &str, keep: usize) {
    for (_, path) in list(dir, key).into_iter().skip(keep.max(1)) {
        let _ = std::fs::remove_file(path);
    }
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn discard_marker(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.discarded"))
}

pub fn mark_discarded(dir: &Path, map: &Path) {
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(discard_marker(dir, &map_key(Some(map))), unix_secs(SystemTime::now()).to_string());
    }
}

pub fn recovery_candidate(dir: &Path, map: &Path) -> Option<PathBuf> {
    let key = map_key(Some(map));
    let saved = std::fs::metadata(map).and_then(|m| m.modified()).map(unix_secs).unwrap_or(0);
    let discarded =
        std::fs::read_to_string(discard_marker(dir, &key)).ok().and_then(|t| t.trim().parse::<u64>().ok()).unwrap_or(0);
    let (secs, path) = list(dir, &key).into_iter().next()?;
    (secs > saved.saturating_add(1) && secs > discarded).then_some(path)
}

#[cfg(test)]
pub fn save_now(state: &AppState, dir: &Path) -> std::io::Result<PathBuf> {
    save_doc_now(&state.doc, &state.cameras, &state.settings, dir)
}

pub fn save_doc_now(
    doc: &hp_editor::Document,
    cameras: &hp_app::state::Cameras,
    settings: &Settings,
    dir: &Path,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let key = map_key(doc.path());
    let path = dir.join(file_name(&key, unix_secs(SystemTime::now())));
    let mut map = doc.to_map();
    crate::files::store_view_with(cameras, settings, &mut map);
    hp_vmf::save_with(&map, &path, &hp_vmf::SaveOptions { backup: false, sync: true })
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(path)
}

static NEXT_SAVE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static WRITTEN: std::sync::Mutex<Option<std::collections::HashMap<String, u64>>> = std::sync::Mutex::new(None);

pub fn next_save_seq() -> u64 {
    NEXT_SAVE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub fn write_in_order<T, E>(path: &Path, seq: u64, write: impl FnOnce() -> Result<T, E>) -> Option<Result<T, E>> {
    let mut written = WRITTEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = map_key(Some(path));
    if written.as_ref().and_then(|w| w.get(&key)).is_some_and(|&newer| newer > seq) {
        return None;
    }
    let result = write();
    if result.is_ok() {
        written.get_or_insert_with(Default::default).insert(key, seq);
    }
    Some(result)
}

pub enum SaveOutcome {
    Saved,
    Failed(hp_editor::EditError),
    Superseded,
}

pub struct FinishedSave {
    pub tab: crate::tabs::TabId,
    pub ticket: hp_editor::SaveTicket,
    pub outcome: SaveOutcome,
}

struct RunningSave {
    tab: crate::tabs::TabId,
    ticket: hp_editor::SaveTicket,
    rx: crossbeam_channel::Receiver<SaveOutcome>,
}

#[derive(Default)]
pub struct BackgroundSaves {
    running: Vec<RunningSave>,
}

impl BackgroundSaves {
    pub fn start(
        &mut self,
        tab: crate::tabs::TabId,
        map: hp_vmf::Map,
        ticket: hp_editor::SaveTicket,
        ctx: Option<egui::Context>,
    ) {
        let seq = next_save_seq();
        let path = ticket.path().to_path_buf();
        let (tx, rx) = crossbeam_channel::bounded(1);
        let job = move || {
            let written =
                write_in_order(&path, seq, || hp_vmf::save_with(&map, &path, &hp_vmf::SaveOptions::default()));
            drop(map);
            let outcome = match written {
                None => SaveOutcome::Superseded,
                Some(Ok(_)) => SaveOutcome::Saved,
                Some(Err(e)) => SaveOutcome::Failed(e.into()),
            };
            let _ = tx.send(outcome);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        };
        let job = std::sync::Arc::new(std::sync::Mutex::new(Some(job)));
        let thread_job = job.clone();
        let spawned = std::thread::Builder::new().name("hp-save".to_owned()).spawn(move || {
            if let Some(job) = thread_job.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                job();
            }
        });
        if spawned.is_err()
            && let Some(job) = job.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
        {
            job();
        }
        self.running.push(RunningSave { tab, ticket, rx });
    }

    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    pub fn take_finished(&mut self, wait: bool) -> Vec<FinishedSave> {
        let mut done = Vec::new();
        let mut keep = Vec::new();
        for job in std::mem::take(&mut self.running) {
            let outcome = if wait { job.rx.recv().ok() } else { job.rx.try_recv().ok() };
            match outcome {
                Some(outcome) => done.push(FinishedSave { tab: job.tab, ticket: job.ticket, outcome }),
                None if wait => done.push(FinishedSave {
                    tab: job.tab,
                    ticket: job.ticket,
                    outcome: SaveOutcome::Failed(hp_editor::EditError::Invalid("the save stopped".to_owned())),
                }),
                None => keep.push(job),
            }
        }
        self.running = keep;
        done
    }
}

const SLICE_BUSY: Duration = Duration::from_millis(4);
const SLICE_IDLE: Duration = Duration::from_millis(25);
const IDLE_AFTER: f64 = 1.0;

pub struct Autosaver {
    last: Instant,
    saved: Vec<u64>,
    due: Vec<crate::tabs::TabId>,
    job: Option<(crate::tabs::TabId, hp_editor::SnapshotJob)>,
}

impl Autosaver {
    pub fn new() -> Self {
        Self { last: Instant::now(), saved: Vec::new(), due: Vec::new(), job: None }
    }

    pub fn tick(&mut self, state: &mut AppState, tabs: &crate::tabs::Tabs, ctx: &egui::Context) {
        let minutes = state.settings.edit.autosave_minutes;
        if minutes == 0 {
            self.due.clear();
            self.job = None;
            return;
        }
        let every = Duration::from_secs(u64::from(minutes) * 60);
        if self.due.is_empty() && self.job.is_none() {
            let elapsed = self.last.elapsed();
            if elapsed < every {
                ctx.request_repaint_after(every - elapsed);
                return;
            }
            self.last = Instant::now();
            ctx.request_repaint_after(every);
            let mut open = Vec::new();
            for id in tabs.ids() {
                let Some(doc) = tabs.doc(state, id) else { continue };
                open.push(doc.scene_revision());
                if doc.is_dirty() && !self.saved.contains(&doc.scene_revision()) {
                    self.due.push(id);
                }
            }
            self.saved.retain(|r| open.contains(r));
            self.due.reverse();
        }
        if self.job.is_none() {
            let Some(id) = self.due.pop() else { return };
            let Some(doc) = tabs.doc(state, id) else { return };
            if !doc.is_dirty() {
                return;
            }
            self.job = Some((id, doc.start_snapshot()));
        }
        let Some((id, job)) = self.job.as_mut() else { return };
        let Some((doc, cameras)) = tabs.map_view(state, *id) else {
            self.job = None;
            return;
        };
        let idle = state.frame.time - state.runtime.render.last_input >= IDLE_AFTER;
        let Some(mut map) = doc.step_snapshot(job, if idle { SLICE_IDLE } else { SLICE_BUSY }) else {
            ctx.request_repaint();
            return;
        };
        self.job = None;
        self.saved.push(doc.scene_revision());
        crate::files::store_view_with(cameras, &state.settings, &mut map);
        let dir = Settings::autosave_dir();
        let key = map_key(doc.path());
        let keep = state.settings.edit.autosave_keep.max(1) as usize;
        let secs = unix_secs(SystemTime::now());
        std::thread::spawn(move || {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::warn!("autosave: cannot create {}: {e}", dir.display());
                return;
            }
            let path = dir.join(file_name(&key, secs));
            match hp_vmf::save_with(&map, &path, &hp_vmf::SaveOptions { backup: false, sync: false }) {
                Ok(_) => log::info!("autosaved to {}", path.display()),
                Err(e) => log::warn!("autosave failed: {e}"),
            }
            prune(&dir, &key, keep);
        });
        state.log(Severity::Info, t!("status.autosaved"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hammerpro_autosave_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn older_saves_never_overwrite_newer_ones() {
        let path = Path::new("C:/maps/write_order_check.vmf");
        let older = next_save_seq();
        let newer = next_save_seq();
        assert_eq!(write_in_order(path, newer, || Ok::<u8, ()>(1)), Some(Ok(1)));
        assert_eq!(write_in_order(path, older, || Ok::<u8, ()>(2)), None, "an older snapshot is dropped");
        let failing = next_save_seq();
        assert_eq!(write_in_order(path, failing, || Err::<u8, ()>(())), Some(Err(())));
        assert_eq!(write_in_order(path, failing - 1, || Ok::<u8, ()>(3)), Some(Ok(3)), "a failed write does not count");
        let other = Path::new("C:/maps/write_order_other.vmf");
        assert_eq!(write_in_order(other, older, || Ok::<u8, ()>(4)), Some(Ok(4)), "files are ordered one by one");
    }

    #[test]
    fn background_saves_write_and_report() {
        let dir = scratch("background");
        let path = dir.join("room.vmf");
        let mut doc = hp_editor::Document::from_map(hp_vmf::parse(hp_app::testing::TEST_VMF).unwrap());
        doc.mark_dirty();
        let (map, ticket) = doc.begin_save(&path).unwrap();
        let mut saves = BackgroundSaves::default();
        saves.start(7, map, ticket, None);
        assert!(!saves.is_empty());
        let done = saves.take_finished(true);
        assert!(saves.is_empty());
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].tab, 7);
        assert!(matches!(done[0].outcome, SaveOutcome::Saved));
        let FinishedSave { ticket, .. } = done.into_iter().next().unwrap();
        doc.finish_save(ticket, true);
        assert!(!doc.is_dirty());
        assert_eq!(doc.path(), Some(path.as_path()));
        assert!(hp_vmf::load(&path).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_save_as_during_a_background_save_keeps_the_new_file() {
        hp_app::register_translations();
        hp_app::i18n::register(crate::app::TR);
        let dir = scratch("save_as_race");
        let (original, copy) = (dir.join("original.vmf"), dir.join("copy.vmf"));
        let mut state = hp_app::testing::test_state();
        assert!(crate::files::save_map_to(&mut state, &original));
        state.doc.mark_dirty();
        let (map, ticket) = state.doc.begin_save(&original).unwrap();
        let mut saves = BackgroundSaves::default();
        saves.start(1, map, ticket, None);
        assert!(crate::files::save_map_to(&mut state, &copy));
        for FinishedSave { ticket, outcome, .. } in saves.take_finished(true) {
            match outcome {
                SaveOutcome::Saved => state.doc.finish_save(ticket, true),
                SaveOutcome::Failed(_) => state.doc.finish_save(ticket, false),
                SaveOutcome::Superseded => {}
            }
        }
        assert_eq!(state.doc.path(), Some(copy.as_path()), "the map keeps the Save As file");
        assert!(!state.doc.is_dirty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keys_identify_the_full_path() {
        let a = map_key(Some(Path::new("C:/maps/ttt.vmf")));
        let b = map_key(Some(Path::new("C:/maps/ttt_traps.vmf")));
        let c = map_key(Some(Path::new("D:/other/ttt.vmf")));
        assert!(a.starts_with("ttt_") && b.starts_with("ttt_traps_"));
        assert_ne!(a, c, "same file name in another folder");
        assert_eq!(a, map_key(Some(Path::new("c:\\MAPS\\ttt.vmf"))), "case and slashes do not matter");
        assert_eq!(map_key(None), "untitled");
    }

    #[test]
    fn names_parse_exactly() {
        let key = "ttt_0badf00d";
        assert_eq!(parse_file_name(&file_name(key, 1700000000), key), Some(1700000000));
        assert_eq!(parse_file_name("ttt_0badf00d_traps_1.vmf", key), None);
        assert_eq!(parse_file_name("ttt_0badf00d_.vmf", key), None);
        assert_eq!(parse_file_name("ttt_0badf00d_12.vmx", key), None);
    }

    #[test]
    fn rotation_only_touches_one_map() {
        let dir = scratch("rotation");
        let ttt = map_key(Some(Path::new("C:/maps/ttt.vmf")));
        let traps = map_key(Some(Path::new("C:/maps/ttt_traps.vmf")));
        let twin = map_key(Some(Path::new("D:/elsewhere/ttt.vmf")));
        for secs in 1..=5 {
            for key in [&ttt, &traps, &twin] {
                std::fs::write(dir.join(file_name(key, secs)), "x").unwrap();
            }
        }
        std::fs::write(dir.join("ttt_123.vmf"), "an autosave of an older version").unwrap();
        prune(&dir, &ttt, 2);
        assert_eq!(list(&dir, &ttt).iter().map(|f| f.0).collect::<Vec<_>>(), vec![5, 4]);
        assert_eq!(list(&dir, &traps).len(), 5);
        assert_eq!(list(&dir, &twin).len(), 5);
        assert!(dir.join("ttt_123.vmf").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emergency_save_is_a_recoverable_autosave() {
        let dir = scratch("emergency");
        let mut state = hp_app::testing::test_state();
        let map = dir.join("room.vmf");
        std::fs::write(&map, hp_app::testing::TEST_VMF).unwrap();
        let old = std::fs::File::options().write(true).open(&map).unwrap();
        old.set_modified(SystemTime::now() - Duration::from_secs(3600)).unwrap();
        drop(old);
        state.doc.set_path(Some(map.clone()));
        state.doc.select_all();
        state.doc.delete_selection().unwrap();
        let saved = save_now(&state, &dir).unwrap();
        assert_eq!(recovery_candidate(&dir, &map), Some(saved.clone()));
        let back = hp_vmf::parse(&std::fs::read_to_string(&saved).unwrap()).unwrap();
        assert!(back.world.solids.is_empty() && back.entities.is_empty(), "the unsaved edit is in the save");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_needs_a_newer_autosave() {
        let dir = scratch("recovery");
        let map = dir.join("room.vmf");
        std::fs::write(&map, "versioninfo {}").unwrap();
        assert_eq!(recovery_candidate(&dir, &map), None);
        let key = map_key(Some(&map));
        let saved = unix_secs(std::fs::metadata(&map).unwrap().modified().unwrap());
        std::fs::write(dir.join(file_name(&key, saved.saturating_sub(60))), "old").unwrap();
        assert_eq!(recovery_candidate(&dir, &map), None, "older than the map");
        let newer = dir.join(file_name(&key, saved + 120));
        std::fs::write(&newer, "new").unwrap();
        assert_eq!(recovery_candidate(&dir, &map), Some(newer.clone()));
        std::fs::write(discard_marker(&dir, &key), (saved + 120).to_string()).unwrap();
        assert_eq!(recovery_candidate(&dir, &map), None);
        assert!(newer.exists(), "the file stays");
        let later = dir.join(file_name(&key, saved + 600));
        std::fs::write(&later, "after").unwrap();
        assert_eq!(recovery_candidate(&dir, &map), Some(later));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
