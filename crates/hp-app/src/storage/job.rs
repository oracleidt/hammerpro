use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use super::delete::{DeleteOutcome, ItemError, delete_rows};
use super::locations::{AppDirs, find_spec};
use super::scan::{Progress, Report, scan_report};

#[derive(Clone, Debug, Default)]
pub struct DeleteSummary {
    pub outcomes: Vec<DeleteOutcome>,
    pub freed: u64,
    pub deleted: usize,
    pub errors: Vec<ItemError>,
    pub failure: Option<String>,
}

impl DeleteSummary {
    pub fn from_outcomes(outcomes: Vec<DeleteOutcome>) -> DeleteSummary {
        let freed = outcomes.iter().map(|o| o.freed).sum();
        let deleted = outcomes.iter().map(|o| o.deleted).sum();
        let errors = outcomes.iter().flat_map(|o| o.errors.iter().cloned()).collect();
        DeleteSummary { outcomes, freed, deleted, errors, failure: None }
    }
}

#[derive(Debug)]
struct Job<T> {
    rx: Receiver<T>,
    progress: Arc<Progress>,
}

#[derive(Debug, Default)]
pub struct StorageRuntime {
    pub dirs: Option<AppDirs>,
    pub report: Option<Report>,
    pub selected: BTreeSet<String>,
    pub confirm: Option<Vec<String>>,
    pub summary: Option<DeleteSummary>,
    pub notice: Option<String>,
    scan: Option<Job<Report>>,
    delete: Option<Job<Result<Vec<DeleteOutcome>, String>>>,
}

impl StorageRuntime {
    pub fn dirs(&self) -> AppDirs {
        self.dirs.clone().unwrap_or_else(AppDirs::current)
    }

    pub fn scanning(&self) -> bool {
        self.scan.is_some()
    }

    pub fn deleting(&self) -> bool {
        self.delete.is_some()
    }

    pub fn busy(&self) -> bool {
        self.scanning() || self.deleting()
    }

    pub fn progress(&self) -> Option<(usize, usize, u64)> {
        let progress = self.delete.as_ref().map(|j| &j.progress).or(self.scan.as_ref().map(|j| &j.progress))?;
        Some(progress.counts())
    }

    pub fn start_scan(&mut self) {
        if self.scan.is_some() {
            return;
        }
        let dirs = self.dirs();
        let progress = Arc::new(Progress::default());
        let (tx, rx) = channel();
        let p = progress.clone();
        let spawned = std::thread::Builder::new().name("storage-scan".into()).spawn(move || {
            let _ = tx.send(scan_report(&dirs, &p));
        });
        if spawned.is_ok() {
            self.scan = Some(Job { rx, progress });
        }
    }

    pub fn start_delete(&mut self, ids: Vec<String>, compile_busy: bool) -> Result<(), String> {
        if self.delete.is_some() {
            return Err("a cleanup is already running".to_owned());
        }
        for id in &ids {
            let spec = find_spec(id).ok_or_else(|| format!("unknown storage item \"{id}\""))?;
            if compile_busy && spec.uses_compile {
                return Err(crate::t!("storage.compile_busy").to_owned());
            }
        }
        let dirs = self.dirs();
        let progress = Arc::new(Progress::default());
        let (tx, rx) = channel();
        let p = progress.clone();
        let list = ids.clone();
        std::thread::Builder::new()
            .name("storage-delete".into())
            .spawn(move || {
                let _ = tx.send(delete_rows(&dirs, &list, &p));
            })
            .map_err(|e| e.to_string())?;
        self.delete = Some(Job { rx, progress });
        for id in &ids {
            self.selected.remove(id);
        }
        self.confirm = None;
        Ok(())
    }

    pub fn cancel(&mut self) {
        for job in self.scan.iter().map(|j| &j.progress).chain(self.delete.iter().map(|j| &j.progress)) {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.scan = None;
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(job) = &self.scan {
            match job.rx.try_recv() {
                Ok(report) => {
                    self.selected.retain(|id| report.entry(id).is_some());
                    self.report = Some(report);
                    self.scan = None;
                    changed = true;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.scan = None;
                    changed = true;
                }
            }
        }
        if let Some(job) = &self.delete {
            let done = match job.rx.try_recv() {
                Ok(Ok(outcomes)) => Some(DeleteSummary::from_outcomes(outcomes)),
                Ok(Err(e)) => Some(DeleteSummary { failure: Some(e), ..DeleteSummary::default() }),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(DeleteSummary { failure: Some("the cleanup stopped".to_owned()), ..DeleteSummary::default() })
                }
            };
            if let Some(summary) = done {
                self.summary = Some(summary);
                self.delete = None;
                self.scan = None;
                self.start_scan();
                changed = true;
            }
        }
        changed
    }
}
