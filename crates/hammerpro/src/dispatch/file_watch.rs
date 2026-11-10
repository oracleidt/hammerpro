use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use hp_app::runtime::{FileStamp, file_stamp};

pub(crate) struct FileWatch {
    paths: Vec<PathBuf>,
    stop: Arc<AtomicBool>,
    changed: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn stamps(paths: &[PathBuf]) -> Vec<FileStamp> {
    paths.iter().map(|p| file_stamp(p)).collect()
}

impl FileWatch {
    pub(crate) fn start(paths: Vec<PathBuf>, every: Duration, wake: impl Fn() + Send + 'static) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let changed = Arc::new(AtomicBool::new(false));
        let watched = paths.clone();
        let (stop2, changed2) = (stop.clone(), changed.clone());
        let thread = std::thread::Builder::new()
            .name("hp-file-watch".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                let mut last = stamps(&watched);
                loop {
                    std::thread::park_timeout(every);
                    if stop2.load(Ordering::Acquire) {
                        return;
                    }
                    let now = stamps(&watched);
                    if now != last {
                        last = now;
                        changed2.store(true, Ordering::Release);
                        wake();
                    }
                }
            })
            .ok()?;
        Some(Self { paths, stop, changed, thread: Some(thread) })
    }

    pub(crate) fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub(crate) fn watches(&self, path: &Path) -> bool {
        self.paths.len() == 1 && self.paths[0] == path
    }

    pub(crate) fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }
}

impl Drop for FileWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    use super::*;

    fn wait_for(flag: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if flag() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn wakes_only_when_a_watched_file_changes_and_stops_at_once() {
        let dir = std::env::temp_dir().join(format!("hp_file_watch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.glb");
        let other = dir.join("missing.nav");
        std::fs::write(&file, b"one").unwrap();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let watch = FileWatch::start(vec![file.clone(), other.clone()], Duration::from_millis(20), move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        assert!(!watch.watches(&file));
        assert_eq!(watch.paths(), [file.clone(), other.clone()]);
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(wakes.load(Ordering::SeqCst), 0, "no wake while nothing changes");
        assert!(!watch.take_changed());
        std::fs::write(&file, b"two bytes more").unwrap();
        assert!(wait_for(|| wakes.load(Ordering::SeqCst) == 1));
        assert!(watch.take_changed());
        assert!(!watch.take_changed());
        std::fs::write(&other, b"created").unwrap();
        assert!(wait_for(|| wakes.load(Ordering::SeqCst) == 2), "a file that appears counts");
        let slow = FileWatch::start(vec![file.clone()], Duration::from_secs(3600), || {}).unwrap();
        assert!(slow.watches(&file));
        let started = Instant::now();
        drop(slow);
        drop(watch);
        assert!(started.elapsed() < Duration::from_secs(2), "dropping does not wait for the next tick");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
