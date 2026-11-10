use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

use hp_collab::Snapshot;
use hp_collab::snapshots::{SNAPSHOT_INTERVAL_SECONDS, SnapshotRow, SnapshotStore};
use hp_collab::transport::SnapshotResult;

use super::tr;

enum Finished {
    Listed(Vec<SnapshotRow>),
    Loaded(Snapshot),
}

#[derive(Default)]
pub struct Tasks {
    store: Option<SnapshotStore>,
    rows: Vec<SnapshotRow>,
    pending: Option<Receiver<Result<Finished, String>>>,
    saving: Option<Receiver<SnapshotResult>>,
    last_save: Option<Instant>,
    restore: Option<SnapshotRow>,
    error: Option<String>,
}

impl Tasks {
    pub fn start(&mut self, persistent: bool) {
        let root = if persistent {
            hp_app::settings::Settings::autosave_dir().with_file_name("team-mapping-snapshots")
        } else {
            std::env::temp_dir().join(format!("HammerPro-team-snapshots-{}", std::process::id()))
        };
        *self = Self { store: Some(SnapshotStore::new(root)), ..Self::default() };
    }

    pub fn store(&self) -> Option<SnapshotStore> {
        self.store.clone()
    }

    pub fn refresh(&mut self) {
        self.last_save = Some(Instant::now());
        if self.pending.is_none()
            && let Some(store) = self.store.clone()
        {
            self.spawn(move || Ok(Finished::Listed(store.list()?)));
        }
    }

    fn busy(&self) -> bool {
        self.pending.is_some() || self.saving.is_some()
    }

    fn spawn(&mut self, operation: impl FnOnce() -> anyhow::Result<Finished> + Send + 'static) {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        match std::thread::Builder::new().name("team-map-snapshot".into()).spawn(move || {
            let _ = tx.send(operation().map_err(|error| error.to_string()));
        }) {
            Ok(_) => self.pending = Some(rx),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub fn begin_save(&mut self) -> Option<(SnapshotStore, SyncSender<SnapshotResult>)> {
        if self.busy() {
            return None;
        }
        let store = self.store.clone()?;
        let (done, receiver) = std::sync::mpsc::sync_channel(1);
        self.saving = Some(receiver);
        self.last_save = Some(Instant::now());
        Some((store, done))
    }

    pub fn tick(&mut self, ready: bool) -> (Option<Snapshot>, bool) {
        let mut restore = None;
        if let Some(receiver) = &self.pending {
            let result = match receiver.try_recv() {
                Ok(value) => Some(value),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("Snapshot worker stopped".into())),
            };
            if let Some(result) = result {
                self.pending = None;
                match result {
                    Ok(Finished::Listed(rows)) => {
                        self.rows = rows;
                        self.error = None;
                    }
                    Ok(Finished::Loaded(map)) => restore = Some(map),
                    Err(error) => self.error = Some(error),
                }
            }
        }
        if let Some(receiver) = &self.saving {
            let result = match receiver.try_recv() {
                Ok(value) => Some(value),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("Snapshot worker stopped".into())),
            };
            if let Some(result) = result {
                self.saving = None;
                match result {
                    Ok(rows) => {
                        self.rows = rows;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        let due = ready
            && self.store.is_some()
            && !self.busy()
            && self.last_save.is_none_or(|last| last.elapsed() >= Duration::from_secs(SNAPSHOT_INTERVAL_SECONDS));
        (restore, due)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, ready: bool) -> bool {
        let mut save = false;
        if self.store.is_none() {
            return false;
        }
        ui.collapsing(tr("Safety snapshots", "Snapshots de sécurité", "Резервные снимки"), |ui| {
            ui.label(tr("Compressed local snapshots every 10 minutes. Only the latest 10 are kept. A safety snapshot is saved before restoring.", "Snapshots locaux compressés toutes les 10 minutes. Les 10 derniers sont conservés. Une copie de sécurité est créée avant restauration.", "Сжатые локальные снимки каждые 10 минут. Хранятся только последние 10. Перед восстановлением сохраняется резервная копия."));
            let bytes: u64 = self.rows.iter().map(|row| row.bytes).sum();
            ui.label(format!("{} / 10 | {:.1} KiB", self.rows.len(), bytes as f64 / 1024.0));
            if self.busy() { ui.spinner(); }
            if ui.add_enabled(!self.busy() && ready, egui::Button::new(tr("Save snapshot now", "Créer un snapshot maintenant", "Создать снимок сейчас"))).clicked() {
                save = true;
            }
            for row in self.rows.clone() {
                let when = chrono::DateTime::from_timestamp(row.created as i64, 0).map_or_else(|| row.created.to_string(), |time| time.format("%Y-%m-%d %H:%M UTC").to_string());
                ui.group(|ui| {
                    ui.label(format!("{} | {}", row.session, when));
                    ui.horizontal(|ui| {
                        ui.label(format!("{} {} | {:.1} KiB", tr("Revision", "Révision", "Версия"), row.revision, row.bytes as f64 / 1024.0));
                        if ui.add_enabled(!self.busy(), egui::Button::new(tr("Restore", "Restaurer", "Восстановить"))).clicked() { self.restore = Some(row.clone()); }
                    });
                });
            }
            if let Some(row) = self.restore.clone() {
                ui.colored_label(hp_app::theme::warning(), tr("Replace the shared map for every participant? Current state will be backed up first.", "Remplacer la map partagée pour tous les participants ? L'état courant sera sauvegardé avant.", "Заменить общую карту для всех участников? Сначала сохранится текущее состояние."));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.busy(), egui::Button::new(tr("Confirm restore", "Confirmer la restauration", "Подтвердить восстановление"))).clicked() {
                        if let Some(store) = self.store.clone() { self.spawn(move || Ok(Finished::Loaded(store.load(&row.filename)?))); }
                        self.restore = None;
                    }
                    if ui.button(tr("Cancel", "Annuler", "Отмена")).clicked() { self.restore = None; }
                });
            }
            if let Some(error) = &self.error { ui.colored_label(hp_app::theme::error(), error); }
        });
        save
    }
}
