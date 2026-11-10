use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_vmf::{Entity, Map, Solid};
use rayon::prelude::*;

use super::Document;
use crate::scene::Owner;
use crate::{EditError, EditResult};

const SOLID_CHUNK: usize = 1024;
const ENTITY_CHUNK: usize = 2048;

#[derive(Debug)]
pub struct SaveTicket {
    path: PathBuf,
    serial: u64,
    dirty_epoch: u64,
    order: u64,
}

impl SaveTicket {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Default)]
pub struct SnapshotJob {
    revision: u64,
    solids: Vec<(Owner, Solid)>,
    entities: Vec<Entity>,
    restarts: u32,
}

impl SnapshotJob {
    pub fn restarts(&self) -> u32 {
        self.restarts
    }
}

impl Document {
    pub fn can_save_in_background(&self, path: &Path) -> bool {
        self.manifest.as_ref().is_none_or(|link| link.path != path)
    }

    pub fn begin_save(&mut self, path: &Path) -> EditResult<(Map, SaveTicket)> {
        if !self.can_save_in_background(path) {
            return Err(EditError::Invalid("a manifest saves all its submaps at once".to_owned()));
        }
        self.manifest = None;
        self.scene.version_info.map_version = self.scene.version_info.map_version.saturating_add(1);
        let map = self.to_map();
        self.save_order = self.save_order.wrapping_add(1);
        let ticket = SaveTicket {
            path: path.to_path_buf(),
            serial: self.history.state_serial(),
            dirty_epoch: self.dirty_epoch,
            order: self.save_order,
        };
        Ok((map, ticket))
    }

    pub fn finish_save(&mut self, ticket: SaveTicket, saved: bool) {
        if !saved {
            if ticket.order == self.save_order {
                self.set_extra_dirty();
            }
            return;
        }
        if ticket.order < self.saved_order {
            return;
        }
        self.saved_order = ticket.order;
        self.path = Some(ticket.path);
        self.saved_serial = ticket.serial;
        if self.dirty_epoch == ticket.dirty_epoch {
            self.extra_dirty = false;
        }
    }

    pub(crate) fn mark_saved_now(&mut self, path: PathBuf) {
        self.save_order = self.save_order.wrapping_add(1);
        self.saved_order = self.save_order;
        self.path = Some(path);
        self.saved_serial = self.history.state_serial();
        self.extra_dirty = false;
    }

    pub fn start_snapshot(&self) -> SnapshotJob {
        SnapshotJob { revision: self.scene_revision, ..SnapshotJob::default() }
    }

    pub fn step_snapshot(&self, job: &mut SnapshotJob, budget: Duration) -> Option<Map> {
        let started = Instant::now();
        if job.revision != self.scene_revision {
            *job = SnapshotJob {
                revision: self.scene_revision,
                restarts: job.restarts.saturating_add(1),
                ..SnapshotJob::default()
            };
        }
        let scene = &self.scene;
        while job.entities.len() < scene.entities.len() {
            let from = job.entities.len();
            let to = (from + ENTITY_CHUNK).min(scene.entities.len());
            let part: Vec<Entity> = (from..to)
                .into_par_iter()
                .filter_map(|i| scene.entities.get_index(i).map(|(_, n)| n.entity.clone()))
                .collect();
            job.entities.extend(part);
            if started.elapsed() >= budget {
                return None;
            }
        }
        while job.solids.len() < scene.solids.len() {
            let from = job.solids.len();
            let to = (from + SOLID_CHUNK).min(scene.solids.len());
            let part: Vec<(Owner, Solid)> = (from..to)
                .into_par_iter()
                .filter_map(|i| scene.solids.get_index(i).map(|(_, n)| (n.owner, n.solid.clone())))
                .collect();
            job.solids.extend(part);
            if started.elapsed() >= budget {
                return None;
            }
        }
        let solids = std::mem::take(&mut job.solids);
        let entities = std::mem::take(&mut job.entities);
        let mut map = scene.assemble_map(solids, entities);
        crate::curve_meta::prepare_road_contents(&mut map, scene);
        Some(map)
    }

    pub(crate) fn set_extra_dirty(&mut self) {
        self.extra_dirty = true;
        self.dirty_epoch = self.dirty_epoch.wrapping_add(1);
    }
}
