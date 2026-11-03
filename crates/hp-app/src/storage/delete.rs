use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::locations::{AppDirs, Spec, find_spec, in_use};
use super::scan::Progress;

const MAX_ERRORS: usize = 25;

#[derive(Clone, Debug)]
pub struct ItemError {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Clone, Debug, Default)]
pub struct DeleteOutcome {
    pub id: String,
    pub deleted: usize,
    pub freed: u64,
    pub skipped: usize,
    pub errors: Vec<ItemError>,
}

fn is_link(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

fn remove_file(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            if let Ok(meta) = std::fs::metadata(path) {
                let mut perm = meta.permissions();
                if perm.readonly() {
                    #[allow(clippy::permissions_set_readonly_false)]
                    perm.set_readonly(false);
                    let _ = std::fs::set_permissions(path, perm);
                    return std::fs::remove_file(path);
                }
            }
            Err(e)
        }
        other => other,
    }
}

fn remove_link(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path))
}

pub fn remove_tree(path: &Path, errors: &mut Vec<ItemError>) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    let mut push = |p: &Path, e: std::io::Error| {
        if errors.len() < MAX_ERRORS {
            errors.push(ItemError { path: p.to_path_buf(), error: e.to_string() });
        }
    };
    if is_link(&meta) {
        return match remove_link(path) {
            Ok(()) => 0,
            Err(e) => {
                push(path, e);
                0
            }
        };
    }
    if !meta.is_dir() {
        return match remove_file(path) {
            Ok(()) => meta.len(),
            Err(e) => {
                push(path, e);
                0
            }
        };
    }
    let mut freed = 0;
    let mut failed = false;
    match std::fs::read_dir(path) {
        Ok(read) => {
            for entry in read.flatten() {
                let before = errors.len();
                freed += remove_tree(&entry.path(), errors);
                failed |= errors.len() > before;
            }
        }
        Err(e) => {
            push(path, e);
            failed = true;
        }
    }
    if !failed
        && let Err(e) = std::fs::remove_dir(path)
        && errors.len() < MAX_ERRORS
    {
        errors.push(ItemError { path: path.to_path_buf(), error: e.to_string() });
    }
    freed
}

fn delete_spec(dirs: &AppDirs, spec: &'static Spec, progress: &Progress) -> DeleteOutcome {
    let mut out = DeleteOutcome { id: spec.id.to_owned(), ..DeleteOutcome::default() };
    for part in spec.parts {
        for item in dirs.list_part(part) {
            if progress.cancelled() {
                return out;
            }
            if let Some(why) = in_use(part.root, &item) {
                out.skipped += 1;
                out.errors.push(ItemError { path: item, error: format!("in use: {why}") });
                continue;
            }
            let guarded = match dirs.guard(part.root, &item) {
                Ok(p) => p,
                Err(e) => {
                    out.skipped += 1;
                    out.errors.push(ItemError { path: item, error: format!("refused: {e}") });
                    continue;
                }
            };
            let mut errors = Vec::new();
            let freed = remove_tree(&guarded, &mut errors);
            out.freed += freed;
            progress.bytes.fetch_add(freed, Ordering::Relaxed);
            if errors.is_empty() {
                out.deleted += 1;
            } else {
                out.skipped += 1;
                for e in errors {
                    if out.errors.len() < MAX_ERRORS {
                        out.errors.push(e);
                    }
                }
            }
        }
    }
    out
}

pub fn delete_rows(dirs: &AppDirs, ids: &[String], progress: &Progress) -> Result<Vec<DeleteOutcome>, String> {
    let mut specs = Vec::new();
    for id in ids {
        let spec = find_spec(id).ok_or_else(|| format!("unknown storage item \"{id}\""))?;
        if !specs.iter().any(|s: &&Spec| s.id == spec.id) {
            specs.push(spec);
        }
    }
    progress.rows_total.store(specs.len(), Ordering::Relaxed);
    progress.rows_done.store(0, Ordering::Relaxed);
    let mut outcomes = Vec::with_capacity(specs.len());
    for spec in specs {
        outcomes.push(delete_spec(dirs, spec, progress));
        progress.rows_done.fetch_add(1, Ordering::Relaxed);
    }
    Ok(outcomes)
}
