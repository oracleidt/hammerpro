use std::sync::mpsc::{Receiver, TryRecvError};

use anyhow::Result;
use hp_collab::document::ValidatedSnapshot;
use hp_collab::transport::HostOptions;
use hp_collab::{Session, Snapshot, Unit};
use zeroize::Zeroizing;

pub(super) struct Job<T> {
    receiver: Receiver<std::result::Result<T, String>>,
}

impl<T: Send + 'static> Job<T> {
    pub(super) fn spawn(name: &str, work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<Self> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new().name(name.into()).spawn(move || {
            let _ = sender.send(work().map_err(|error| format!("{error:#}")));
        })?;
        Ok(Self { receiver })
    }

    pub(super) fn poll(&self) -> Option<std::result::Result<T, String>> {
        match self.receiver.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("Team Mapping worker stopped".into())),
        }
    }
}

pub(super) struct Installed {
    pub(super) map: ValidatedSnapshot,
    pub(super) document: hp_editor::Document,
}

pub(super) struct Loading {
    pub(super) job: Job<Installed>,
    pub(super) revision: u64,
    pub(super) reset: bool,
}

pub(super) fn install(map: ValidatedSnapshot, revision: u64, reset: bool) -> Result<Loading> {
    let job = Job::spawn("team-map-install", move || {
        let text = map.to_vmf()?;
        let mut parsed = hp_vmf::parse(&text)?;
        drop(text);
        let version = map
            .0
            .get("world")
            .and_then(|unit| hp_kv::parse_bounded(&unit.vmf, 128).ok())
            .and_then(|root| root.children.first().and_then(|world| world.get_parsed::<i32>("mapversion")));
        if let Some(version) = version {
            parsed.version_info.map_version = version;
        }
        let document = hp_editor::Document::from_map(parsed);
        Ok(Installed { map, document })
    })?;
    Ok(Loading { job, revision, reset })
}

pub(super) struct Starting {
    pub(super) job: Job<(Session, Vec<String>)>,
    pub(super) serial: u64,
}

pub(super) struct Building {
    serial: u64,
    solids: Vec<u32>,
    entities: Vec<u32>,
    next: usize,
    sender: Option<std::sync::mpsc::Sender<super::sync::Exported>>,
    job: Option<Job<(Session, Vec<String>)>>,
}

impl Building {
    pub(super) fn new(
        doc: &hp_editor::Document,
        name: String,
        password: Zeroizing<String>,
        options: HostOptions,
    ) -> Result<Self> {
        let (sender, receiver) = std::sync::mpsc::channel::<super::sync::Exported>();
        let job = Job::spawn("team-map-host", move || {
            let mut units = std::collections::BTreeMap::new();
            let mut problems = Vec::new();
            for exported in receiver {
                for (key, unit) in super::sync::units_of(exported, &mut problems)? {
                    if let Some(unit) = unit {
                        units.insert(key, unit);
                    }
                }
            }
            let validated = ValidatedSnapshot::new(Snapshot(units))?;
            Ok((Session::host_validated(validated, name, password, options)?, problems))
        })?;
        Ok(Self {
            serial: doc.content_serial(),
            solids: doc.scene().solids().keys().copied().collect(),
            entities: doc.scene().entities().keys().copied().collect(),
            next: 0,
            sender: Some(sender),
            job: Some(job),
        })
    }

    pub(super) fn step(&mut self, state: &hp_app::AppState, budget: std::time::Duration) -> Result<Option<Starting>> {
        let started = std::time::Instant::now();
        let total = self.solids.len() + self.entities.len();
        let sender = self.sender.as_ref().ok_or_else(|| anyhow::anyhow!("Session already started"))?;
        while self.next < total && started.elapsed() < budget {
            let end = (self.next + 1024).min(total);
            let mut touched = hp_editor::SharedTouched::default();
            for index in self.next..end {
                if index < self.solids.len() {
                    touched.solids.push(self.solids[index]);
                } else {
                    touched.entities.push(self.entities[index - self.solids.len()]);
                }
            }
            self.next = end;
            sender
                .send(super::Controller::export_typed(state, &touched))
                .map_err(|_| anyhow::anyhow!("Team Mapping worker stopped"))?;
        }
        if self.next < total {
            return Ok(None);
        }
        let header = hp_editor::SharedTouched { world: true, metadata: true, ..Default::default() };
        sender
            .send(super::Controller::export_typed(state, &header))
            .map_err(|_| anyhow::anyhow!("Team Mapping worker stopped"))?;
        self.sender = None;
        let job = self.job.take().ok_or_else(|| anyhow::anyhow!("Session already started"))?;
        Ok(Some(Starting { job, serial: self.serial }))
    }
}

pub(super) fn recovery(
    map: Snapshot,
    overlay: Vec<(String, Option<Unit>)>,
    path: std::path::PathBuf,
) -> Result<Job<()>> {
    Job::spawn("team-map-recovery", move || {
        let mut map = map;
        for (key, unit) in overlay {
            match unit {
                Some(unit) => map.0.insert(key, unit),
                None => map.0.remove(&key),
            };
        }
        let world = map.0.get("world").and_then(|unit| unit.to_world().ok()).map_or(0, |world| world.id);
        let owners: std::collections::BTreeSet<u32> =
            map.0.keys().filter_map(|key| key.strip_prefix("entity:")?.parse().ok()).collect();
        for (key, unit) in map.0.iter_mut() {
            if key.starts_with("solid:") && unit.owner != world && !owners.contains(&unit.owner) {
                unit.owner = world;
            }
        }
        let text = map.to_vmf()?;
        std::fs::write(path, text)?;
        Ok(())
    })
}
