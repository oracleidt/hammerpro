use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use super::locations::{AppDirs, Risk, Spec, TABLE, in_use};

#[derive(Debug, Default)]
pub struct Progress {
    pub rows_done: AtomicUsize,
    pub rows_total: AtomicUsize,
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn counts(&self) -> (usize, usize, u64) {
        (
            self.rows_done.load(Ordering::Relaxed),
            self.rows_total.load(Ordering::Relaxed),
            self.files.load(Ordering::Relaxed),
        )
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: &'static str,
    pub name_key: &'static str,
    pub desc_key: &'static str,
    pub risk: Risk,
    pub uses_compile: bool,
    pub paths: Vec<PathBuf>,
    pub bytes: u64,
    pub files: u64,
    pub items: usize,
    pub in_use: usize,
    pub in_use_bytes: u64,
}

impl Entry {
    pub fn freeable(&self) -> u64 {
        self.bytes.saturating_sub(self.in_use_bytes)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub entries: Vec<Entry>,
    pub total_bytes: u64,
    pub total_files: u64,
    pub millis: u64,
}

impl Report {
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn fraction(&self, bytes: u64) -> f32 {
        if self.total_bytes == 0 { 0.0 } else { (bytes as f64 / self.total_bytes as f64) as f32 }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    format_bytes_in(bytes, crate::i18n::lang())
}

pub fn format_bytes_in(bytes: u64, lang: crate::i18n::Lang) -> String {
    let units: [&str; 5] = match lang {
        crate::i18n::Lang::En => ["B", "KB", "MB", "GB", "TB"],
        crate::i18n::Lang::Fr => ["o", "Ko", "Mo", "Go", "To"],
        crate::i18n::Lang::Ru => ["Б", "КБ", "МБ", "ГБ", "ТБ"],
    };
    if bytes < 1024 {
        return format!("{bytes} {}", units[0]);
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", units[unit])
}

pub(super) fn tree_size(path: &Path, progress: &Progress) -> (u64, u64) {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return (0, 0) };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        progress.files.fetch_add(1, Ordering::Relaxed);
        progress.bytes.fetch_add(meta.len(), Ordering::Relaxed);
        return (meta.len(), 1);
    }
    let (mut bytes, mut files) = (0u64, 0u64);
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if progress.cancelled() {
            break;
        }
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() && !ft.is_symlink() {
                stack.push(entry.path());
            } else {
                let len = entry.metadata().map_or(0, |m| m.len());
                bytes += len;
                files += 1;
                progress.files.fetch_add(1, Ordering::Relaxed);
                progress.bytes.fetch_add(len, Ordering::Relaxed);
            }
        }
    }
    (bytes, files)
}

fn scan_spec(dirs: &AppDirs, spec: &'static Spec, progress: &Progress) -> Entry {
    let mut entry = Entry {
        id: spec.id,
        name_key: spec.name_key,
        desc_key: spec.desc_key,
        risk: spec.risk,
        uses_compile: spec.uses_compile,
        paths: Vec::new(),
        bytes: 0,
        files: 0,
        items: 0,
        in_use: 0,
        in_use_bytes: 0,
    };
    for part in spec.parts {
        let dir = dirs.part_dir(part);
        if !dir.as_os_str().is_empty() {
            entry.paths.push(dir);
        }
        for item in dirs.list_part(part) {
            if progress.cancelled() {
                return entry;
            }
            entry.items += 1;
            let (bytes, files) = tree_size(&item, progress);
            entry.bytes += bytes;
            entry.files += files;
            if in_use(part.root, &item).is_some() {
                entry.in_use += 1;
                entry.in_use_bytes += bytes;
            }
        }
    }
    entry
}

fn finish(mut entries: Vec<Entry>, started: Instant) -> Report {
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.id.cmp(b.id)));
    let total_bytes = entries.iter().map(|e| e.bytes).sum();
    let total_files = entries.iter().map(|e| e.files).sum();
    Report { entries, total_bytes, total_files, millis: started.elapsed().as_millis() as u64 }
}

pub fn scan_report(dirs: &AppDirs, progress: &Progress) -> Report {
    let ids: Vec<&str> = TABLE.iter().map(|s| s.id).collect();
    scan_ids(dirs, &ids, progress)
}

pub fn scan_ids(dirs: &AppDirs, ids: &[&str], progress: &Progress) -> Report {
    let started = Instant::now();
    let specs: Vec<&'static Spec> = TABLE.iter().filter(|s| ids.contains(&s.id)).collect();
    progress.rows_total.store(specs.len(), Ordering::Relaxed);
    progress.rows_done.store(0, Ordering::Relaxed);
    let mut entries = Vec::with_capacity(specs.len());
    for spec in specs {
        entries.push(scan_spec(dirs, spec, progress));
        progress.rows_done.fetch_add(1, Ordering::Relaxed);
    }
    finish(entries, started)
}
