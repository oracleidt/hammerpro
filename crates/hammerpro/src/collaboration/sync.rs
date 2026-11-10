use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use hp_app::{Action, AppState};
use hp_collab::document::{check_units, digest, expand_changes, patch_changes, sanitize_entity, unit_scripts};
use hp_collab::protocol::{EditRefusal, Rejection};
use hp_collab::{Change, Command, Snapshot, Unit};
use hp_editor::{Owner, SharedTouched, SharedUpdate};
use rayon::prelude::*;

use super::Controller;

pub(super) const LIVE_UNITS: usize = 64;
pub(super) const LIVE_INTERVAL: Duration = Duration::from_millis(33);
const PARALLEL: usize = 24;
const BACKGROUND_UNITS: usize = 192;
const BACKGROUND_BYTES: usize = 512 * 1024;

type Applied = (hp_collab::document::ValidatedSnapshot, Vec<Change>, SharedUpdate, Vec<String>);

pub(super) struct Applying {
    pub(super) job: super::install::Job<Applied>,
    pub(super) author: u32,
    pub(super) revision: u64,
    pub(super) serial: u64,
    pub(super) started: Instant,
}

fn split_update(update: &mut SharedUpdate, keys: &BTreeSet<String>) -> Vec<String> {
    let mut removed = Vec::new();
    if update.world.is_some() && keys.contains("world") {
        update.world = None;
        removed.push("world".to_owned());
    }
    if update.metadata.is_some() && keys.contains("metadata") {
        update.metadata = None;
        removed.push("metadata".to_owned());
    }
    update.entities.retain(|(id, _)| {
        let key = format!("entity:{id}");
        let keep = !keys.contains(&key);
        if !keep {
            removed.push(key);
        }
        keep
    });
    update.solids.retain(|(id, _)| {
        let key = format!("solid:{id}");
        let keep = !keys.contains(&key);
        if !keep {
            removed.push(key);
        }
        keep
    });
    removed
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Origin {
    Edit,
    Live,
    Undo,
    Redo,
}

pub(super) struct Pending {
    pub(super) sequence: u64,
    pub(super) changes: Vec<Change>,
    pub(super) undo: Vec<Change>,
    pub(super) origin: Origin,
    pub(super) sent: Instant,
    pub(super) detached: bool,
}

pub(super) struct PersonalEdit {
    pub(super) undo: Vec<Change>,
    pub(super) redo: Vec<Change>,
    pub(super) bytes: usize,
}

pub(super) fn touched_keys<'a>(keys: impl IntoIterator<Item = &'a String>) -> SharedTouched {
    let mut touched = SharedTouched::default();
    for key in keys {
        if let Some(id) = key.strip_prefix("solid:").and_then(|id| id.parse().ok()) {
            touched.solids.push(id);
        } else if let Some(id) = key.strip_prefix("entity:").and_then(|id| id.parse().ok()) {
            touched.entities.push(id);
        } else if key == "world" {
            touched.world = true;
        } else if key == "metadata" {
            touched.metadata = true;
        }
    }
    touched.solids.sort_unstable();
    touched.solids.dedup();
    touched.entities.sort_unstable();
    touched.entities.dedup();
    touched
}

fn merge(touched: &mut SharedTouched, other: SharedTouched) {
    touched.solids.extend(other.solids);
    touched.solids.sort_unstable();
    touched.solids.dedup();
    touched.entities.extend(other.entities);
    touched.entities.sort_unstable();
    touched.entities.dedup();
    touched.world |= other.world;
    touched.metadata |= other.metadata;
}

fn script_entities(state: &AppState) -> SharedTouched {
    let mut touched = SharedTouched::default();
    for (id, node) in state.doc.scene().entities() {
        let entity = &node.entity;
        let inputs = entity.connections.iter().any(|connection| {
            ["command", "runscriptcode", "runscriptfile", "runpassedcode"]
                .iter()
                .any(|name| connection.input.trim().eq_ignore_ascii_case(name))
        });
        if inputs || hp_collab::document::script_class(&entity.classname) {
            touched.entities.push(*id);
        }
    }
    touched
}

pub(super) fn script_uses(changes: &[Change]) -> Vec<String> {
    changes.iter().filter_map(|change| Some(unit_scripts(&change.key, change.after.as_ref()?))).flatten().collect()
}

pub(super) fn decode(changes: &[&Change], world: u32) -> Result<SharedUpdate> {
    let mut update = SharedUpdate::default();
    let mut solids = Vec::new();
    for change in changes {
        let after = change.after.as_ref();
        if change.key == "world" {
            if let Some(unit) = after {
                update.world = Some(unit.to_world()?);
            }
        } else if change.key == "metadata" {
            if let Some(unit) = after {
                update.metadata = Some(unit.to_metadata()?);
            }
        } else if let Some(id) = change.key.strip_prefix("entity:").and_then(|id| id.parse().ok()) {
            update.entities.push((id, after.map(Unit::to_entity).transpose()?));
        } else if let Some(id) = change.key.strip_prefix("solid:").and_then(|id| id.parse::<u32>().ok()) {
            solids.push((id, after));
        }
    }
    let owner = |unit: &Unit| if unit.owner == world { Owner::World } else { Owner::Entity(unit.owner) };
    let solid = |(id, unit): &(u32, Option<&Unit>)| -> Result<_> {
        Ok((*id, unit.map(|unit| unit.to_solid().map(|solid| (solid, owner(unit)))).transpose()?))
    };
    update.solids = if solids.len() >= PARALLEL {
        solids.par_iter().map(solid).collect::<Result<_>>()?
    } else {
        solids.iter().map(solid).collect::<Result<_>>()?
    };
    Ok(update)
}

impl Controller {
    pub(super) fn has_submitted(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn remember_edit(&mut self, undo: Vec<Change>, redo: Vec<Change>) {
        if undo.is_empty() {
            return;
        }
        let bytes = undo.iter().chain(&redo).map(|c| c.after.as_ref().map_or(0, |unit| unit.vmf.len()) + 96).sum();
        self.undo.push_back(PersonalEdit { undo, redo, bytes });
        self.redo.clear();
        while self.undo.len() > 128 || self.undo.iter().map(|edit| edit.bytes).sum::<usize>() > 64 * 1024 * 1024 {
            self.undo.pop_front();
        }
    }

    pub(super) fn local_units(state: &AppState, touched: &SharedTouched) -> Result<Vec<(String, Option<Unit>)>> {
        Self::export_units(state, touched, &mut Vec::new())
    }

    pub(super) fn export_units(
        state: &AppState,
        touched: &SharedTouched,
        problems: &mut Vec<String>,
    ) -> Result<Vec<(String, Option<Unit>)>> {
        units_of(Self::export_typed(state, touched), problems)
    }

    pub(super) fn export_typed(state: &AppState, touched: &SharedTouched) -> Exported {
        let mut export = state.doc.shared_export(touched);
        let scene = state.doc.scene();
        let scripts = !state.runtime.collaboration.keep_scripts_local;
        let local_only = |owner: u32| {
            scene.entity(owner).is_some_and(|node| sanitize_entity(&mut node.entity.clone(), scripts).is_err())
        };
        for (_, row) in &mut export.solids {
            if row.as_ref().is_some_and(|(_, owner)| local_only(*owner)) {
                *row = None;
            }
        }
        Exported { export, world: touched.world, metadata: touched.metadata, scripts }
    }
}

pub(super) struct Exported {
    export: hp_editor::SharedExport,
    world: bool,
    metadata: bool,
    scripts: bool,
}

impl Exported {
    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .export
            .solids
            .iter()
            .map(|(id, _)| format!("solid:{id}"))
            .chain(self.export.entities.iter().map(|(id, _)| format!("entity:{id}")))
            .collect();
        if self.world {
            keys.push("world".into());
        }
        if self.metadata {
            keys.push("metadata".into());
        }
        keys
    }
}

pub(super) fn units_of(exported: Exported, problems: &mut Vec<String>) -> Result<Vec<(String, Option<Unit>)>> {
    let Exported { mut export, world, metadata, scripts } = exported;
    {
        let solid = |(id, row): &(u32, Option<(hp_vmf::Solid, u32)>)| -> Result<(String, Option<Unit>)> {
            let unit = match row {
                Some((solid, owner)) => Some(Unit::from_solid(solid, *owner)?),
                None => None,
            };
            Ok((format!("solid:{id}"), unit))
        };
        let mut rows: Vec<(String, Option<Unit>)> = if export.solids.len() >= PARALLEL {
            export.solids.par_iter().map(solid).collect::<Result<_>>()?
        } else {
            export.solids.iter().map(solid).collect::<Result<_>>()?
        };
        for (id, row) in &mut export.entities {
            let unit = match row {
                Some((entity, brush)) => match sanitize_entity(entity, scripts) {
                    Ok(found) => {
                        problems.extend(found);
                        Some(Unit::from_entity(entity, *brush)?)
                    }
                    Err(problem) => {
                        problems.push(problem);
                        None
                    }
                },
                None => None,
            };
            rows.push((format!("entity:{id}"), unit));
        }
        if let Some(header) = &mut export.header {
            header.format = hp_vmf::TextFormat::default();
            if let Ok(found) = sanitize_entity(&mut header.world, scripts) {
                problems.extend(found);
            }
            let (world_unit, metadata_unit) = Unit::header_units(header)?;
            if world {
                rows.push(("world".into(), Some(world_unit)));
            }
            if metadata {
                rows.push(("metadata".into(), Some(metadata_unit)));
            }
        }
        Ok(rows)
    }
}

pub(super) struct Capturing {
    job: super::install::Job<(Vec<Change>, Vec<String>)>,
    previous: u64,
    pub(super) keys: BTreeSet<String>,
}

impl Controller {
    fn changes_against(&self, rows: Vec<(String, Option<Unit>)>) -> Vec<Change> {
        let Some(canonical) = &self.canonical else { return Vec::new() };
        rows.into_iter()
            .filter(|(key, _)| !self.held.contains(key))
            .filter_map(|(key, after)| {
                let before = canonical.0.get(&key);
                (before != after.as_ref()).then(|| Change { key, before: before.map(digest), after })
            })
            .collect()
    }

    fn capture_in_background(&mut self, state: &mut AppState, touched: &SharedTouched, previous: u64) -> Result<()> {
        let exported = Self::export_typed(state, touched);
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        let keys: BTreeSet<String> = exported.keys().into_iter().filter(|key| !self.held.contains(key)).collect();
        let base: BTreeMap<String, Option<Unit>> =
            keys.iter().map(|key| (key.clone(), canonical.0.get(key).cloned())).collect();
        let job = super::install::Job::spawn("team-map-capture", move || {
            let mut problems = Vec::new();
            let rows = units_of(exported, &mut problems)?;
            let changes: Vec<Change> = rows
                .into_iter()
                .filter_map(|(key, after)| {
                    let before = base.get(&key)?;
                    (before != &after).then(|| Change { key, before: before.as_ref().map(digest), after })
                })
                .collect();
            if !changes.is_empty() {
                check_units(&changes)?;
            }
            Ok((changes, problems))
        })?;
        self.capturing = Some(Capturing { job, previous, keys });
        Ok(())
    }

    fn finish_capture(&mut self, state: &mut AppState) -> Result<bool> {
        let Some(capturing) = &self.capturing else { return Ok(false) };
        let Some(result) = capturing.job.poll() else { return Ok(true) };
        let previous = capturing.previous;
        self.capturing = None;
        let (changes, problems) = match result {
            Ok(found) => found,
            Err(error) => {
                self.last_content_serial = previous;
                self.message(state, format!("{}: {error}", hp_app::t!("team.rejected")));
                return Ok(true);
            }
        };
        self.report_local_only(state, problems);
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        if changes.iter().any(|change| canonical.0.get(&change.key).map(digest) != change.before) {
            self.last_content_serial = previous;
            return Ok(false);
        }
        if changes.is_empty() {
            self.finish_live();
            return Ok(true);
        }
        self.submit(state, changes, Origin::Edit)?;
        Ok(true)
    }

    pub(super) fn capture(&mut self, state: &mut AppState) -> Result<()> {
        if self.finish_capture(state)? {
            return Ok(());
        }
        if self.pending.is_some() || self.canonical.is_none() || self.loading.is_some() {
            return Ok(());
        }
        if !state.doc.in_transaction() && std::mem::take(&mut self.script_rescan) {
            let touched = script_entities(state);
            if !touched.is_empty() {
                let mut problems = Vec::new();
                let rows = Self::export_units(state, &touched, &mut problems)?;
                self.report_local_only(state, problems);
                let changes = self.changes_against(rows);
                if !changes.is_empty() {
                    return self.submit(state, changes, Origin::Edit);
                }
            }
        }
        let started = Instant::now();
        if state.doc.in_transaction() {
            if !state.settings.workflow.team_live_edits
                || self.last_live.is_some_and(|last| last.elapsed() < LIVE_INTERVAL)
            {
                return Ok(());
            }
            let Some(touched) = state.doc.shared_touched_live(self.last_content_serial) else { return Ok(()) };
            if touched.is_empty() || touched.len() > LIVE_UNITS {
                return Ok(());
            }
            let mut problems = Vec::new();
            let rows = Self::export_units(state, &touched, &mut problems)?;
            let changes = self.changes_against(rows);
            self.report_local_only(state, problems);
            if changes.is_empty() || self.canonical.as_ref().is_none_or(|map| map.check(&changes).is_err()) {
                return Ok(());
            }
            self.last_live = Some(Instant::now());
            self.note_capture(started);
            return self.submit(state, changes, Origin::Live);
        }
        let serial = state.doc.content_serial();
        if serial == self.last_content_serial && self.live.is_none() {
            return Ok(());
        }
        let previous = std::mem::replace(&mut self.last_content_serial, serial);
        let changes = match state.doc.shared_touched_since(previous) {
            Some(mut touched) => {
                if let Some(live) = &self.live {
                    merge(&mut touched, touched_keys(live.keys()));
                }
                if touched.is_empty() {
                    Vec::new()
                } else if touched.len() >= BACKGROUND_UNITS && self.canonical.is_some() {
                    return self.capture_in_background(state, &touched, previous);
                } else {
                    let mut problems = Vec::new();
                    let rows = Self::export_units(state, &touched, &mut problems)?;
                    self.report_local_only(state, problems);
                    self.changes_against(rows)
                }
            }
            None => {
                let mut map = state.doc.to_map();
                map.format = hp_vmf::TextFormat::default();
                let current = match Snapshot::from_vmf(&hp_vmf::write(&map)) {
                    Ok(current) => current,
                    Err(error) => {
                        self.message(state, format!("{}: {error:#}", hp_app::t!("team.rejected")));
                        return Ok(());
                    }
                };
                let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
                canonical.diff(&current).into_iter().filter(|change| !self.held.contains(&change.key)).collect()
            }
        };
        self.note_capture(started);
        if changes.is_empty() {
            self.finish_live();
            return Ok(());
        }
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        if let Err(error) = canonical.check(&changes) {
            self.revert(state, &changes, true)?;
            self.finish_live();
            self.message(state, format!("{}: {error}", hp_app::t!("team.rejected")));
            return Ok(());
        }
        self.submit(state, changes, Origin::Edit)
    }

    pub(super) fn report_local_only(&mut self, state: &mut AppState, problems: Vec<String>) {
        let mut fresh = Vec::new();
        for problem in problems {
            if self.local_only.insert(problem.clone()) {
                fresh.push(problem);
            }
        }
        if let Some(first) = fresh.first() {
            let more = if fresh.len() > 1 { format!(" (+{})", fresh.len() - 1) } else { String::new() };
            state.notify(hp_app::Severity::Warning, format!("Team Mapping: {first}{more}"));
        }
    }

    fn note_capture(&mut self, started: Instant) {
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        self.stats.capture_ms = elapsed;
        self.stats.capture_max_ms = self.stats.capture_max_ms.max(elapsed);
    }

    pub(super) fn submit(&mut self, _state: &AppState, changes: Vec<Change>, origin: Origin) -> Result<()> {
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        let undo = changes
            .iter()
            .map(|change| Change {
                key: change.key.clone(),
                before: change.after.as_ref().map(digest),
                after: canonical.0.get(&change.key).cloned(),
            })
            .collect();
        if origin == Origin::Live || (origin == Origin::Edit && self.live.is_some()) {
            let live = self.live.get_or_insert_with(BTreeMap::new);
            for change in &changes {
                live.entry(change.key.clone()).or_insert_with(|| canonical.0.get(&change.key).cloned());
            }
        }
        let wire = if self.host { changes.clone() } else { patch_changes(&changes, canonical) };
        let bytes: usize = wire
            .iter()
            .map(|change| change.key.len() + change.after.as_ref().map_or(0, |unit| unit.vmf.len()) + 96)
            .sum();
        self.sequence += 1;
        self.session
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Session disconnected"))?
            .send(Command::Edit { sequence: self.sequence, changes: wire })?;
        self.pending =
            Some(Pending { sequence: self.sequence, changes, undo, origin, sent: Instant::now(), detached: false });
        self.stats.edits_sent += 1;
        self.stats.last_edit_bytes = bytes as u64;
        if origin == Origin::Live {
            self.stats.live_edits += 1;
        }
        Ok(())
    }

    pub(super) fn finish_live(&mut self) {
        let Some(originals) = self.live.take() else { return };
        let Some(canonical) = &self.canonical else { return };
        let (mut undo, mut redo) = (Vec::new(), Vec::new());
        for (key, original) in originals {
            let current = canonical.0.get(&key).cloned();
            if current == original {
                continue;
            }
            undo.push(Change { key: key.clone(), before: current.as_ref().map(digest), after: original.clone() });
            redo.push(Change { key, before: original.as_ref().map(digest), after: current });
        }
        self.remember_edit(undo, redo);
    }

    pub(super) fn acknowledge(&mut self, state: &mut AppState, revision: u64, mut changes: Vec<Change>) -> Result<()> {
        let pending = self.pending.take().ok_or_else(|| anyhow::anyhow!("Unexpected session acknowledgement"))?;
        let canonical = self.canonical.as_mut().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        if changes.is_empty() {
            changes = pending.changes;
        } else {
            expand_changes(&mut changes, canonical)?;
            ensure!(changes == pending.changes, "Session acknowledgement mismatch");
        }
        canonical.apply(&changes)?;
        self.revision = revision;
        if pending.detached {
            let mut update = decode(&changes.iter().collect::<Vec<_>>(), state.doc.scene().world().id)?;
            let transaction: BTreeSet<String> =
                state.doc.shared_transaction_keys().map(|keys| keys.keys().into_iter().collect()).unwrap_or_default();
            for key in split_update(&mut update, &transaction) {
                self.held.insert(key);
            }
            state.doc.apply_shared_update(update)?;
            return Ok(());
        }
        let round_trip = pending.sent.elapsed().as_secs_f64() * 1000.0;
        self.stats.round_trip_ms = round_trip;
        self.stats.round_trip_average_ms = if self.stats.round_trip_average_ms == 0.0 {
            round_trip
        } else {
            self.stats.round_trip_average_ms * 0.8 + round_trip * 0.2
        };
        match pending.origin {
            Origin::Edit if self.live.is_some() => self.finish_live(),
            Origin::Edit => self.remember_edit(pending.undo, changes),
            Origin::Live | Origin::Undo | Origin::Redo => {}
        }
        Ok(())
    }

    pub(super) fn apply_remote(
        &mut self,
        state: &mut AppState,
        author: u32,
        revision: u64,
        mut changes: Vec<Change>,
    ) -> Result<()> {
        let started = Instant::now();
        ensure!(!changes.is_empty(), "Unexpected session acknowledgement");
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        expand_changes(&mut changes, canonical)?;
        let transaction: BTreeSet<String> =
            state.doc.shared_transaction_keys().map(|keys| keys.keys().into_iter().collect()).unwrap_or_default();
        let touched: Option<BTreeSet<String>> =
            state.doc.shared_touched_live(self.last_content_serial).map(|keys| keys.keys().into_iter().collect());
        let pending: BTreeSet<&str> = self
            .pending
            .iter()
            .flat_map(|pending| pending.changes.iter().map(|change| change.key.as_str()))
            .chain(self.capturing.iter().flat_map(|capturing| capturing.keys.iter().map(String::as_str)))
            .collect();
        let suspects: Vec<&String> = changes
            .iter()
            .map(|change| &change.key)
            .filter(|key| !transaction.contains(*key))
            .filter(|key| {
                touched.as_ref().is_none_or(|touched| touched.contains(*key)) || pending.contains(key.as_str())
            })
            .collect();
        let mut conflicts = Vec::new();
        if !suspects.is_empty() {
            for (key, unit) in Self::local_units(state, &touched_keys(suspects))? {
                if canonical.0.get(&key) != unit.as_ref() {
                    conflicts.push((key, unit));
                }
            }
        }
        self.note_remote_edit(state, author, changes.iter().map(|change| &change.key));
        if !conflicts.is_empty() {
            let text = self.conflict_message(state, conflicts.iter().map(|(key, _)| key));
            for (key, unit) in conflicts {
                self.conflict.insert(key, unit);
            }
            self.message(state, text);
        }
        let world = state.doc.scene().world().id;
        let bytes: usize = changes.iter().map(|change| change.after.as_ref().map_or(64, |unit| unit.vmf.len())).sum();
        if changes.len() >= BACKGROUND_UNITS || bytes >= BACKGROUND_BYTES {
            let mut canonical = self.canonical.take().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
            let job = super::install::Job::spawn("team-map-apply", move || {
                canonical.apply(&changes)?;
                let update = decode(&changes.iter().collect::<Vec<_>>(), world)?;
                let scripts = script_uses(&changes);
                Ok((canonical, changes, update, scripts))
            })?;
            self.applying = Some(Applying { job, author, revision, serial: state.doc.content_serial(), started });
            return Ok(());
        }
        self.canonical.as_mut().unwrap().apply(&changes)?;
        self.revision = revision;
        let scripts = script_uses(&changes);
        self.note_scripts(state, author, scripts);
        let mut visible = Vec::with_capacity(changes.len());
        for change in &changes {
            if transaction.contains(&change.key) {
                self.held.insert(change.key.clone());
            } else {
                visible.push(change);
            }
        }
        let update = decode(&visible, world)?;
        state.doc.apply_shared_update(update)?;
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        self.stats.apply_ms = elapsed;
        self.stats.apply_max_ms = self.stats.apply_max_ms.max(elapsed);
        self.stats.edits_received += 1;
        Ok(())
    }

    pub(super) fn finish_applying(&mut self, state: &mut AppState) -> Result<()> {
        let Some(applying) = &self.applying else { return Ok(()) };
        let Some(result) = applying.job.poll() else { return Ok(()) };
        let (author, revision, serial, started) =
            (applying.author, applying.revision, applying.serial, applying.started);
        self.applying = None;
        let (canonical, changes, mut update, scripts) = result.map_err(|error| anyhow::anyhow!(error))?;
        self.canonical = Some(canonical);
        self.revision = revision;
        self.note_scripts(state, author, scripts);
        self.note_remote_edit(state, author, changes.iter().map(|change| &change.key));
        let transaction: BTreeSet<String> =
            state.doc.shared_transaction_keys().map(|keys| keys.keys().into_iter().collect()).unwrap_or_default();
        let touched: BTreeSet<String> =
            state.doc.shared_touched_live(serial).map(|keys| keys.keys().into_iter().collect()).unwrap_or_default();
        let suspects: Vec<&String> = changes
            .iter()
            .map(|change| &change.key)
            .filter(|key| touched.contains(*key) && !transaction.contains(*key))
            .collect();
        if !suspects.is_empty() {
            let canonical = self.canonical.as_ref().unwrap();
            let mut conflicted = Vec::new();
            for (key, unit) in Self::local_units(state, &touched_keys(suspects))? {
                if canonical.0.get(&key) != unit.as_ref() {
                    conflicted.push(key.clone());
                    self.conflict.insert(key, unit);
                }
            }
            if !conflicted.is_empty() {
                let text = self.conflict_message(state, conflicted.iter());
                self.message(state, text);
            }
        }
        for key in split_update(&mut update, &transaction) {
            self.held.insert(key);
        }
        state.doc.apply_shared_update(update)?;
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        self.stats.apply_ms = elapsed;
        self.stats.apply_max_ms = self.stats.apply_max_ms.max(elapsed);
        self.stats.edits_received += 1;
        Ok(())
    }

    pub(super) fn revert(&mut self, state: &mut AppState, changes: &[Change], keep: bool) -> Result<()> {
        let Some(canonical) = &self.canonical else { return Ok(()) };
        let transaction: BTreeSet<String> =
            state.doc.shared_transaction_keys().map(|keys| keys.keys().into_iter().collect()).unwrap_or_default();
        let mut restore = Vec::new();
        for change in changes {
            if canonical.0.get(&change.key) == change.after.as_ref() {
                continue;
            }
            if keep {
                self.conflict.insert(change.key.clone(), change.after.clone());
            }
            if transaction.contains(&change.key) {
                self.held.insert(change.key.clone());
            } else {
                restore.push(Change {
                    key: change.key.clone(),
                    before: None,
                    after: canonical.0.get(&change.key).cloned(),
                });
            }
        }
        let update = decode(&restore.iter().collect::<Vec<_>>(), state.doc.scene().world().id)?;
        state.doc.apply_shared_update(update)?;
        Ok(())
    }

    pub(super) fn refused(
        &mut self,
        state: &mut AppState,
        sequence: u64,
        rejection: &Rejection,
        revision: u64,
    ) -> Result<()> {
        self.revision = revision;
        if self.pending.as_ref().is_none_or(|pending| pending.sequence != sequence) {
            return Ok(());
        }
        let pending = self.pending.take().unwrap();
        if pending.detached {
            let keys: Vec<String> = pending.changes.iter().map(|change| change.key.clone()).collect();
            for change in pending.changes {
                self.conflict.insert(change.key, change.after);
            }
            let text = self.refusal_message(state, &keys, rejection);
            self.message(state, text);
            return Ok(());
        }
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        let (conflicting, rest): (Vec<Change>, Vec<Change>) =
            pending.changes.into_iter().partition(|change| canonical.0.get(&change.key).map(digest) != change.before);
        match pending.origin {
            Origin::Undo => {
                if let Some(edit) = self.redo.pop()
                    && conflicting.is_empty()
                {
                    self.undo.push_back(edit);
                }
            }
            Origin::Redo => {
                if let Some(edit) = self.undo.pop_back()
                    && conflicting.is_empty()
                {
                    self.redo.push(edit);
                }
            }
            Origin::Edit | Origin::Live => {}
        }
        if conflicting.is_empty() || matches!(pending.origin, Origin::Undo | Origin::Redo) {
            let keys: Vec<String> = rest.iter().map(|change| change.key.clone()).collect();
            self.revert(state, &rest, matches!(pending.origin, Origin::Edit | Origin::Live))?;
            let text = self.refusal_message(state, &keys, rejection);
            self.message(state, text);
        } else if !rest.is_empty() {
            self.submit(state, rest, pending.origin)?;
        }
        Ok(())
    }

    pub(super) fn resolve_held(&mut self, state: &mut AppState) -> Result<()> {
        if self.held.is_empty() || state.doc.in_transaction() || self.loading.is_some() || self.canonical.is_none() {
            return Ok(());
        }
        let keys = std::mem::take(&mut self.held);
        let local = Self::local_units(state, &touched_keys(keys.iter()))?;
        let changed: Option<BTreeSet<String>> =
            state.doc.shared_touched_since(self.last_content_serial).map(|keys| keys.keys().into_iter().collect());
        let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
        let mut restore = Vec::new();
        let mut conflicted = Vec::new();
        for (key, unit) in local {
            let target = canonical.0.get(&key).cloned();
            if unit == target {
                continue;
            }
            if changed.as_ref().is_none_or(|changed| changed.contains(&key)) {
                conflicted.push(key.clone());
                self.conflict.insert(key.clone(), unit);
            }
            restore.push(Change { key, before: None, after: target });
        }
        let update = decode(&restore.iter().collect::<Vec<_>>(), state.doc.scene().world().id)?;
        state.doc.apply_shared_update(update)?;
        if !conflicted.is_empty() {
            let text = self.conflict_message(state, conflicted.iter());
            self.message(state, text);
        }
        Ok(())
    }

    fn refusal_message(&self, state: &mut AppState, keys: &[String], rejection: &Rejection) -> String {
        if rejection.code == EditRefusal::Selected
            && let Some(text) = self.lock_message(state, keys)
        {
            return text;
        }
        format!("{}: {}", hp_app::t!("team.rejected"), super::access::rejection_text(rejection))
    }

    pub(crate) fn history(&mut self, state: &mut AppState, action: Action) -> bool {
        if self.session.is_none() || !matches!(action, Action::Undo | Action::Redo) {
            return false;
        }
        let undoing = action == Action::Undo;
        let result = (|| -> Result<()> {
            let busy = self.pending.is_some()
                || self.capturing.is_some()
                || self.loading.is_some()
                || self.applying.is_some()
                || self.canonical.is_none()
                || state.doc.in_transaction();
            ensure!(!busy, "Wait for the current edit to synchronize");
            self.resolve_held(state)?;
            self.capture(state)?;
            ensure!(self.pending.is_none(), "Wait for the current edit to synchronize");
            let edit = if undoing { self.undo.pop_back() } else { self.redo.pop() };
            let Some(edit) = edit else { return Ok(()) };
            let changes = if undoing { edit.undo.clone() } else { edit.redo.clone() };
            let canonical = self.canonical.as_ref().ok_or_else(|| anyhow::anyhow!("Session map unavailable"))?;
            if let Err(error) = canonical.check(&changes) {
                return Err(error.context(hp_app::t!("team.undo_refused")));
            }
            let update = decode(&changes.iter().collect::<Vec<_>>(), state.doc.scene().world().id)?;
            state.doc.apply_shared_update(update)?;
            self.last_content_serial = state.doc.content_serial();
            self.submit(state, changes, if undoing { Origin::Undo } else { Origin::Redo })?;
            if undoing {
                self.redo.push(edit);
            } else {
                self.undo.push_back(edit);
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.message(state, format!("{error:#}"));
        }
        true
    }
}
