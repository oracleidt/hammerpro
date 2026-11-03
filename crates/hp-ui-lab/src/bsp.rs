use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use hp_editor::Document;

pub struct BspResult {
    pub path: PathBuf,
    pub doc: Document,
    pub report: hp_decompile::Report,
    pub content_dir: Option<PathBuf>,
    pub extracted: hp_decompile::pak::Extracted,
    pub elapsed: Duration,
}

pub fn content_dir(cache: &Path, bsp: &Path) -> PathBuf {
    cache.join("decompiled").join(hp_app::sidecar::map_key(Some(bsp)))
}

pub fn run(path: &Path, cache: Option<&Path>, known_outputs: Option<HashSet<String>>) -> Result<BspResult, String> {
    let start = Instant::now();
    let bsp = hp_bsp::Bsp::open(path).map_err(|e| e.to_string())?;
    let map_name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let options = hp_decompile::Options { map_name, known_outputs, ..hp_decompile::Options::default() };
    let out = hp_decompile::decompile(&bsp, &options).map_err(|e| e.to_string())?;
    let mut doc = Document::from_map(out.map);
    doc.mark_dirty();
    let mut extracted = hp_decompile::pak::Extracted::default();
    let mut dir_out = None;
    if let Some(cache) = cache {
        let dir = content_dir(cache, path);
        if dir.starts_with(cache) && dir.is_dir() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        match hp_decompile::pak::extract_content(&bsp, &dir) {
            Ok(x) if x.files > 0 => {
                extracted = x;
                dir_out = Some(dir);
            }
            Ok(x) => extracted = x,
            Err(e) => log::warn!("pakfile of {} not copied: {e}", path.display()),
        }
    }
    Ok(BspResult {
        path: path.to_path_buf(),
        doc,
        report: out.report,
        content_dir: dir_out,
        extracted,
        elapsed: start.elapsed(),
    })
}

pub struct BspJob {
    pub path: PathBuf,
    pub started: Instant,
    rx: Receiver<Result<BspResult, String>>,
}

impl BspJob {
    pub fn start(
        path: PathBuf,
        cache: Option<PathBuf>,
        known_outputs: Option<HashSet<String>>,
        ctx: Option<egui::Context>,
    ) -> Option<BspJob> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let worker_path = path.clone();
        std::thread::Builder::new()
            .name("hp-decompile".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&worker_path, cache.as_deref(), known_outputs)
                }))
                .unwrap_or_else(|_| Err("the compiled map could not be read".to_owned()));
                let _ = tx.send(result);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            })
            .ok()?;
        Some(BspJob { path, started: Instant::now(), rx })
    }

    pub fn poll(&self) -> Option<Result<BspResult, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(crossbeam_channel::TryRecvError::Empty) => None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => Some(Err("the decompiler stopped".to_owned())),
        }
    }

    pub fn wait(&self, timeout: Duration) -> Option<Result<BspResult, String>> {
        self.rx.recv_timeout(timeout).ok()
    }
}

pub fn known_outputs(fgd: Option<&hp_fgd::ResolvedFgd>) -> Option<HashSet<String>> {
    let fgd = fgd?;
    Some(fgd.all_outputs().into_iter().map(|o| o.name.to_ascii_lowercase()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_damaged_file_is_an_error_not_a_panic() {
        let dir = std::env::temp_dir().join(format!("hp_lab_bsp_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad.bsp");
        std::fs::write(&path, b"VBSP\x14\0\0\0garbage").expect("write");
        let job = BspJob::start(path.clone(), Some(dir.clone()), None, None).expect("job");
        let result = job.wait(Duration::from_secs(30)).expect("answer");
        assert!(result.is_err());
        let missing = run(&dir.join("missing.bsp"), None, None);
        assert!(missing.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn content_folders_are_per_map() {
        let cache = Path::new("cache");
        let a = content_dir(cache, Path::new("C:/maps/gm_a.bsp"));
        let b = content_dir(cache, Path::new("D:/other/gm_a.bsp"));
        assert_ne!(a, b);
        assert!(a.starts_with(cache.join("decompiled")));
    }
}
