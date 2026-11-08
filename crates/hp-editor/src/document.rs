use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use hp_kv::Block;
use hp_vmf::{Cameras, Cordons, Entity, Map, SaveOptions, SaveReport, Solid, VisGroup};
use rayon::prelude::*;

use crate::changes::Changes;
use crate::history::{
    Burst, FACE_SELECTION_STEP, History, Misc, SELECTION_STEP, SelSnap, Snapshot, Tx, UndoEntry, UndoLimits,
    apply_states,
};
use crate::ids::{IdSet, id_set};
use crate::pick::PickCache;
use crate::scene::{EntityNode, GroupNode, Owner, Renumbered, Scene, SolidNode};
use crate::selection::{ObjRef, Selection};
use crate::{EditError, EditResult, Id};

#[derive(Clone, Debug, PartialEq)]
pub struct EditOptions {
    pub texture_lock: bool,
    pub texture_scale_lock: bool,
    pub fallback_texture_size: (u32, u32),
    pub new_brush_color: Option<[u8; 3]>,
}

impl Default for EditOptions {
    fn default() -> Self {
        Self { texture_lock: true, texture_scale_lock: false, fallback_texture_size: (512, 512), new_brush_color: None }
    }
}

fn next_revision() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub struct Document {
    pub(crate) scene: Scene,
    pub(crate) selection: Selection,
    pub(crate) history: History,
    pub(crate) changes: Changes,
    pub(crate) options: EditOptions,
    path: Option<PathBuf>,
    revision: u64,
    saved_serial: u64,
    extra_dirty: bool,
    pub(crate) next_object_id: Id,
    pub(crate) next_side_id: Id,
    pub(crate) next_visgroup_id: Id,
    pub(crate) pick: Mutex<PickCache>,
    pub(crate) renumbered: Vec<Renumbered>,
    pub(crate) load_warnings: Vec<String>,
    load_report: Option<hp_vmf::LoadReport>,
    opened_from: Option<PathBuf>,
    manifest: Option<ManifestLink>,
    pub(crate) face_history: crate::selection::FaceHistory,
}

#[derive(Clone, Debug)]
pub struct ManifestLink {
    pub path: PathBuf,
    pub manifest: hp_vmf::Manifest,
    pub layout: Vec<hp_vmf::manifest::SubmapLayout>,
    pub frames: Vec<hp_vmf::Map>,
    pub active: usize,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("path", &self.path)
            .field("solids", &self.scene.solids.len())
            .field("entities", &self.scene.entities.len())
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl Document {
    pub fn new_empty() -> Self {
        Self::from_map(Map::new_empty())
    }

    pub fn from_map(map: Map) -> Self {
        let (scene, info) = Scene::from_map(map);
        let changes = Changes { everything: true, ..Changes::default() };
        let extra_dirty = !info.renumbered.is_empty();
        Self {
            scene,
            selection: Selection::default(),
            history: History::default(),
            changes,
            options: EditOptions::default(),
            path: None,
            revision: next_revision(),
            saved_serial: 0,
            extra_dirty,
            next_object_id: info.next_object_id,
            next_side_id: info.next_side_id,
            next_visgroup_id: info.next_visgroup_id,
            pick: Mutex::new(PickCache::default()),
            renumbered: info.renumbered,
            load_warnings: Vec::new(),
            load_report: None,
            opened_from: None,
            manifest: None,
            face_history: crate::selection::FaceHistory::default(),
        }
    }

    pub fn load_warnings(&self) -> &[String] {
        &self.load_warnings
    }

    pub fn load_report(&self) -> Option<&hp_vmf::LoadReport> {
        self.load_report.as_ref()
    }

    pub fn opened_from(&self) -> Option<&Path> {
        self.opened_from.as_deref()
    }

    pub fn open(path: &Path) -> EditResult<Self> {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmm")) {
            return Self::open_manifest(path);
        }
        let (map, report) = hp_vmf::load_with_report(path)?;
        let mut doc = Self::from_map(map);
        doc.opened_from = Some(path.to_path_buf());
        doc.load_warnings = report.warnings();
        if report.is_damaged() {
            doc.extra_dirty = true;
            doc.load_warnings.push(
                "The map was opened as a new document so that saving does not overwrite the damaged file: use Save As"
                    .to_owned(),
            );
        } else {
            doc.path = Some(path.to_path_buf());
        }
        doc.load_report = Some(report);
        Ok(doc)
    }

    fn open_manifest(path: &Path) -> EditResult<Self> {
        let bytes = std::fs::read(path).map_err(|e| EditError::Invalid(format!("cannot read manifest: {e}")))?;
        let text = String::from_utf8_lossy(&bytes);
        let manifest =
            hp_vmf::Manifest::parse(&text).map_err(|e| EditError::Invalid(format!("not a valid VMF manifest: {e}")))?;
        if manifest.entries.is_empty() {
            return Err(EditError::Invalid("the manifest lists no submaps".to_owned()));
        }
        let mut submaps = Vec::with_capacity(manifest.entries.len());
        let mut warnings = Vec::new();
        for e in &manifest.entries {
            let p = hp_vmf::manifest::submap_path(path, &e.relative_vmf);
            let (map, report) = hp_vmf::load_with_report(&p)
                .map_err(|err| EditError::Invalid(format!("submap {}: {err}", e.relative_vmf)))?;
            for w in report.warnings() {
                warnings.push(format!("{}: {w}", e.relative_vmf));
            }
            submaps.push((e.clone(), map));
        }
        let combined = hp_vmf::manifest::combine(&submaps);
        let active = combined
            .layout
            .iter()
            .position(|l| l.is_primary && !l.is_protected)
            .or_else(|| combined.layout.iter().position(|l| !l.is_protected))
            .unwrap_or(0);
        let end = combined.layout.iter().map(|l| l.id_end).max().unwrap_or(1);
        let mut doc = Self::from_map(combined.map);
        doc.next_object_id = doc.next_object_id.max(end);
        doc.next_side_id = doc.next_side_id.max(end);
        doc.next_visgroup_id = doc.next_visgroup_id.max(end);
        doc.opened_from = Some(path.to_path_buf());
        doc.path = Some(path.to_path_buf());
        doc.load_warnings = warnings;
        doc.manifest = Some(ManifestLink {
            path: path.to_path_buf(),
            manifest,
            layout: combined.layout,
            frames: combined.frames,
            active,
        });
        Ok(doc)
    }

    pub fn manifest_submaps(&self) -> &[hp_vmf::manifest::SubmapLayout] {
        self.manifest.as_ref().map_or(&[], |m| &m.layout)
    }

    pub fn active_submap(&self) -> Option<usize> {
        self.manifest.as_ref().map(|m| m.active)
    }

    pub fn set_active_submap(&mut self, index: usize) -> bool {
        let Some(link) = self.manifest.as_mut() else { return false };
        match link.layout.get(index) {
            Some(l) if !l.is_protected => {
                link.active = index;
                true
            }
            _ => false,
        }
    }

    fn save_manifest(&mut self, options: &SaveOptions) -> EditResult<SaveReport> {
        let (vmm, layout, frames, manifest_text, active) = {
            let link = self.manifest.as_ref().ok_or(EditError::NoPath)?;
            (link.path.clone(), link.layout.clone(), link.frames.clone(), link.manifest.to_text(), link.active)
        };
        if layout.is_empty() {
            return Err(EditError::Invalid("the manifest lists no submaps".to_owned()));
        }
        let previous = self.scene.version_info.map_version;
        self.scene.version_info.map_version = previous.saturating_add(1);
        let map = self.to_map();
        let outs = hp_vmf::manifest::split(&map, &layout, &frames, active);
        let mut first: Option<SaveReport> = None;
        for (entry, submap) in layout.iter().zip(&outs) {
            if entry.is_protected {
                continue;
            }
            let p = hp_vmf::manifest::submap_path(&vmm, &entry.relative_vmf);
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match hp_vmf::save_with(submap, &p, options) {
                Ok(report) => {
                    if first.is_none() {
                        first = Some(report);
                    }
                }
                Err(e) => {
                    self.scene.version_info.map_version = previous;
                    return Err(e.into());
                }
            }
        }
        if let Err(e) = std::fs::write(&vmm, manifest_text) {
            self.scene.version_info.map_version = previous;
            return Err(EditError::Invalid(format!("cannot write manifest: {e}")));
        }
        self.path = Some(vmm);
        self.saved_serial = self.history.state_serial();
        self.extra_dirty = false;
        Ok(first.unwrap_or_default())
    }

    pub fn manifest(&self) -> Option<&ManifestLink> {
        self.manifest.as_ref()
    }

    pub fn save(&mut self, path: &Path) -> EditResult<()> {
        self.save_with(path, &SaveOptions::default()).map(|_| ())
    }

    pub fn save_with(&mut self, path: &Path, options: &SaveOptions) -> EditResult<SaveReport> {
        if let Some(link) = &self.manifest {
            if path == link.path {
                return self.save_manifest(options);
            }
            self.manifest = None;
        }
        let previous = self.scene.version_info.map_version;
        self.scene.version_info.map_version = previous.saturating_add(1);
        let map = self.to_map();
        match hp_vmf::save_with(&map, path, options) {
            Ok(report) => {
                self.path = Some(path.to_path_buf());
                self.saved_serial = self.history.state_serial();
                self.extra_dirty = false;
                Ok(report)
            }
            Err(e) => {
                self.scene.version_info.map_version = previous;
                Err(e.into())
            }
        }
    }

    pub fn save_current(&mut self) -> EditResult<()> {
        let path = self.path.clone().ok_or(EditError::NoPath)?;
        self.save(&path)
    }

    pub fn to_map(&self) -> Map {
        let mut map = self.scene.to_map();
        crate::curve_meta::prepare_road_contents(&mut map, &self.scene);
        map
    }

    pub fn to_vmf_string(&self) -> String {
        hp_vmf::write(&self.to_map())
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    pub fn is_dirty(&self) -> bool {
        self.extra_dirty || self.history.state_serial() != self.saved_serial
    }

    pub fn mark_dirty(&mut self) {
        self.extra_dirty = true;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn options(&self) -> &EditOptions {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut EditOptions {
        &mut self.options
    }

    pub fn take_changes(&mut self) -> Changes {
        std::mem::take(&mut self.changes)
    }

    pub fn pending_changes(&self) -> &Changes {
        &self.changes
    }

    pub fn mark_everything_changed(&mut self) {
        self.changes.everything = true;
    }

    pub fn peek_next_object_id(&self) -> Id {
        self.next_object_id
    }

    pub fn peek_next_side_id(&self) -> Id {
        self.next_side_id
    }

    pub(crate) fn alloc_object_id(&mut self) -> Id {
        let id = self.next_object_id;
        self.next_object_id = self.next_object_id.saturating_add(1);
        id
    }

    pub(crate) fn alloc_side_id(&mut self) -> Id {
        let id = self.next_side_id;
        self.next_side_id = self.next_side_id.saturating_add(1);
        id
    }

    pub(crate) fn alloc_visgroup_id(&mut self) -> Id {
        let id = self.next_visgroup_id;
        self.next_visgroup_id = self.next_visgroup_id.saturating_add(1);
        id
    }

    pub(crate) fn assign_fresh_ids(&mut self, solid: &mut Solid) {
        solid.id = self.alloc_object_id();
        for side in &mut solid.sides {
            side.id = self.alloc_side_id();
        }
    }

    fn bump(&mut self) {
        self.revision = next_revision();
    }

    fn pick_cache(&mut self) -> &mut PickCache {
        self.pick.get_mut().unwrap_or_else(PoisonError::into_inner)
    }

    fn apply_solids_raw(&mut self, states: Vec<(Id, Option<SolidNode>)>) -> Vec<(Id, Option<SolidNode>)> {
        if states.is_empty() {
            return Vec::new();
        }
        let old = apply_states(&mut self.scene.solids, states);
        let mut visibility = false;
        for (id, prev) in &old {
            let prev_owner = prev.as_ref().map(|n| n.owner);
            let cur = self.scene.solids.get(id);
            let cur_owner = cur.map(|n| n.owner);
            if let (Some(p), Some(c)) = (prev.as_ref(), cur)
                && (p.solid.hidden != c.solid.hidden
                    || p.solid.editor.visgroup_shown != c.solid.editor.visgroup_shown
                    || p.solid.editor.visgroup_auto_shown != c.solid.editor.visgroup_auto_shown
                    || p.solid.editor.group_id != c.solid.editor.group_id)
            {
                visibility = true;
            }
            if prev_owner != cur_owner {
                if let Some(Owner::Entity(e)) = prev_owner
                    && let Some(set) = self.scene.entity_solids.get_mut(&e)
                {
                    set.swap_remove(id);
                    if set.is_empty() {
                        self.scene.entity_solids.swap_remove(&e);
                    }
                }
                if let Some(Owner::Entity(e)) = cur_owner {
                    self.scene.entity_solids.entry(e).or_insert_with(|| id_set(4)).insert(*id);
                }
            }
            if cur.is_some() {
                self.changes.solid_changed(*id);
            } else {
                self.changes.solid_removed(*id);
            }
        }
        let cache = self.pick_cache();
        for (id, _) in &old {
            cache.note_changed(*id);
        }
        if visibility {
            self.changes.visibility_changed = true;
        }
        self.bump();
        old
    }

    fn apply_entities_raw(&mut self, states: Vec<(Id, Option<EntityNode>)>) -> Vec<(Id, Option<EntityNode>)> {
        if states.is_empty() {
            return Vec::new();
        }
        let old = apply_states(&mut self.scene.entities, states);
        for (id, prev) in &old {
            let cur = self.scene.entities.get(id);
            match cur {
                Some(c) => {
                    self.changes.entity_changed(*id);
                    let vis_changed = prev.as_ref().is_some_and(|p| {
                        p.entity.hidden != c.entity.hidden
                            || p.entity.editor.visgroup_shown != c.entity.editor.visgroup_shown
                            || p.entity.editor.visgroup_auto_shown != c.entity.editor.visgroup_auto_shown
                            || p.entity.editor.group_id != c.entity.editor.group_id
                    });
                    if vis_changed {
                        self.changes.visibility_changed = true;
                    }
                }
                None => self.changes.entity_removed(*id),
            }
            if let Some(set) = self.scene.entity_solids.get(id) {
                for sid in set {
                    if self.scene.solids.contains_key(sid) {
                        self.changes.solid_changed(*sid);
                    }
                }
            }
        }
        self.bump();
        old
    }

    fn apply_groups_raw(&mut self, states: Vec<(Id, Option<GroupNode>)>) -> Vec<(Id, Option<GroupNode>)> {
        if states.is_empty() {
            return Vec::new();
        }
        let old = apply_states(&mut self.scene.groups, states);
        self.changes.groups_changed = true;
        self.changes.visibility_changed = true;
        self.bump();
        old
    }

    pub(crate) fn write_solids(&mut self, states: Vec<(Id, Option<SolidNode>)>) {
        let old = self.apply_solids_raw(states);
        if let Some(tx) = &mut self.history.tx {
            for (id, prev) in old {
                tx.snap.solids.entry(id).or_insert(prev);
            }
        }
    }

    pub(crate) fn write_entities(&mut self, states: Vec<(Id, Option<EntityNode>)>) {
        let old = self.apply_entities_raw(states);
        if let Some(tx) = &mut self.history.tx {
            for (id, prev) in old {
                tx.snap.entities.entry(id).or_insert(prev);
            }
        }
    }

    pub(crate) fn write_groups(&mut self, states: Vec<(Id, Option<GroupNode>)>) {
        let old = self.apply_groups_raw(states);
        if let Some(tx) = &mut self.history.tx {
            for (id, prev) in old {
                tx.snap.groups.entry(id).or_insert(prev);
            }
        }
    }

    pub(crate) fn world_mut(&mut self) -> &mut Entity {
        if let Some(tx) = &mut self.history.tx
            && tx.snap.world.is_none()
        {
            tx.snap.world = Some(self.scene.world.clone());
        }
        self.changes.world_changed = true;
        self.bump();
        &mut self.scene.world
    }

    pub(crate) fn visgroups_mut(&mut self) -> (&mut Vec<VisGroup>, &mut IdSet) {
        if let Some(tx) = &mut self.history.tx
            && tx.snap.visgroups.is_none()
        {
            tx.snap.visgroups = Some((self.scene.visgroups.clone(), self.scene.hidden_visgroups.clone()));
        }
        self.changes.visgroups_changed = true;
        self.changes.visibility_changed = true;
        self.bump();
        (&mut self.scene.visgroups, &mut self.scene.hidden_visgroups)
    }

    pub(crate) fn misc_mut(&mut self) -> (&mut Cameras, &mut Cordons) {
        if let Some(tx) = &mut self.history.tx
            && tx.snap.misc.is_none()
        {
            tx.snap.misc = Some(Misc { cameras: self.scene.cameras.clone(), cordons: self.scene.cordons.clone() });
        }
        self.changes.world_changed = true;
        self.bump();
        (&mut self.scene.cameras, &mut self.scene.cordons)
    }

    pub(crate) fn extra_blocks_mut(&mut self) -> &mut Vec<Block> {
        if let Some(tx) = &mut self.history.tx
            && tx.snap.extra_blocks.is_none()
        {
            tx.snap.extra_blocks = Some(self.scene.extra_blocks.clone());
        }
        self.changes.extra_blocks_changed = true;
        self.bump();
        &mut self.scene.extra_blocks
    }

    pub(crate) fn sel_mut(&mut self) -> &mut Selection {
        match &mut self.history.tx {
            Some(tx) => {
                if tx.snap.selection.is_none() {
                    tx.snap.selection = Some(SelSnap::of(&self.selection));
                }
            }
            None => {
                if self.history.record_selection {
                    self.note_selection_change(None);
                }
            }
        }
        self.changes.selection_changed = true;
        self.bump();
        &mut self.selection
    }

    pub(crate) fn update_solids(&mut self, updates: Vec<(Id, Solid)>) {
        let scene = &self.scene;
        let nodes: Vec<(Id, Option<SolidNode>)> = updates
            .into_par_iter()
            .filter_map(|(id, solid)| scene.solids.get(&id).map(|n| (id, Some(n.with_solid(solid)))))
            .collect();
        self.write_solids(nodes);
    }

    pub(crate) fn update_solid(&mut self, id: Id, solid: Solid) {
        self.update_solids(vec![(id, solid)]);
    }

    pub(crate) fn set_solid_owners(&mut self, ids: &[Id], owner: Owner) {
        let states: Vec<(Id, Option<SolidNode>)> = ids
            .iter()
            .filter_map(|id| {
                let n = self.scene.solids.get(id)?;
                (n.owner != owner).then(|| {
                    let mut n = n.clone();
                    n.owner = owner;
                    (*id, Some(n))
                })
            })
            .collect();
        self.write_solids(states);
    }

    pub(crate) fn add_solids(&mut self, solids: Vec<(Solid, Owner)>) -> Vec<Id> {
        let start = self.scene.next_order;
        self.scene.next_order = start.saturating_add(solids.len() as u64);
        let nodes: Vec<SolidNode> = solids
            .into_par_iter()
            .enumerate()
            .map(|(i, (solid, owner))| SolidNode::build(solid, owner, start + i as u64))
            .collect();
        let ids: Vec<Id> = nodes.iter().map(|n| n.solid.id).collect();
        self.write_solids(nodes.into_iter().map(|n| (n.solid.id, Some(n))).collect());
        ids
    }

    pub(crate) fn add_entity(&mut self, mut entity: Entity) -> Id {
        entity.solids.clear();
        let id = entity.id;
        let order = self.scene.alloc_order();
        self.write_entities(vec![(id, Some(EntityNode { entity, order }))]);
        id
    }

    pub(crate) fn update_entity(&mut self, id: Id, f: impl FnOnce(&mut Entity)) -> bool {
        let Some(node) = self.scene.entities.get(&id) else {
            return false;
        };
        let mut node = node.clone();
        f(&mut node.entity);
        node.entity.solids.clear();
        if Some(&node) == self.scene.entities.get(&id) {
            return true;
        }
        self.write_entities(vec![(id, Some(node))]);
        true
    }

    pub(crate) fn remove_solids(&mut self, ids: &[Id]) {
        let states: Vec<(Id, Option<SolidNode>)> =
            ids.iter().filter(|id| self.scene.solids.contains_key(*id)).map(|id| (*id, None)).collect();
        self.write_solids(states);
    }

    pub(crate) fn remove_entities(&mut self, ids: &[Id]) {
        let states: Vec<(Id, Option<EntityNode>)> =
            ids.iter().filter(|id| self.scene.entities.contains_key(*id)).map(|id| (*id, None)).collect();
        self.write_entities(states);
    }

    pub(crate) fn put_group(&mut self, group: hp_vmf::Group) {
        let order = match self.scene.groups.get(&group.id) {
            Some(n) => n.order,
            None => self.scene.alloc_order(),
        };
        let id = group.id;
        if self.scene.groups.get(&id).is_some_and(|n| n.group == group) {
            return;
        }
        self.write_groups(vec![(id, Some(GroupNode { group, order }))]);
    }

    pub(crate) fn remove_groups(&mut self, ids: &[Id]) {
        let states: Vec<(Id, Option<GroupNode>)> =
            ids.iter().filter(|id| self.scene.groups.contains_key(*id)).map(|id| (*id, None)).collect();
        self.write_groups(states);
    }

    pub fn begin(&mut self, label: impl Into<String>) {
        self.begin_impl(label.into(), None);
    }

    pub fn begin_merge(&mut self, label: impl Into<String>, merge_tag: impl Into<String>) {
        self.begin_impl(label.into(), Some(merge_tag.into()));
    }

    fn begin_impl(&mut self, label: String, merge_tag: Option<String>) {
        match &mut self.history.tx {
            Some(tx) => tx.depth = tx.depth.saturating_add(1),
            None => self.history.tx = Some(Tx { label, merge_tag, depth: 1, snap: Snapshot::new() }),
        }
    }

    pub fn in_transaction(&self) -> bool {
        self.history.tx.is_some()
    }

    pub fn commit(&mut self) -> bool {
        match &mut self.history.tx {
            None => false,
            Some(tx) if tx.depth > 1 => {
                tx.depth -= 1;
                false
            }
            Some(_) => match self.history.tx.take() {
                Some(tx) => self.finalize(tx),
                None => false,
            },
        }
    }

    pub fn cancel(&mut self) {
        if let Some(mut tx) = self.history.tx.take() {
            self.swap_in(&mut tx.snap);
            self.prune_selection();
        }
    }

    pub fn transaction<R, E>(
        &mut self,
        label: impl Into<String>,
        f: impl FnOnce(&mut Self) -> Result<R, E>,
    ) -> Result<R, E> {
        self.begin(label);
        match f(self) {
            Ok(r) => {
                self.commit();
                Ok(r)
            }
            Err(e) => {
                self.cancel();
                Err(e)
            }
        }
    }

    pub(crate) fn tx<R>(&mut self, label: &str, f: impl FnOnce(&mut Self) -> EditResult<R>) -> EditResult<R> {
        if self.history.tx.is_some() {
            return f(self);
        }
        self.begin(label);
        match f(self) {
            Ok(r) => {
                self.commit();
                Ok(r)
            }
            Err(e) => {
                self.cancel();
                Err(e)
            }
        }
    }

    fn finalize(&mut self, tx: Tx) -> bool {
        let Tx { label, merge_tag, mut snap, .. } = tx;
        self.prune_noops(&mut snap);
        if snap.has_no_document_change() {
            if self.history.record_selection
                && let Some(before) = snap.selection
            {
                self.note_selection_change(Some(before));
                self.refresh_selection_step();
            }
            return false;
        }
        self.seal_selection_step();
        self.history.burst = Burst::Edit;
        let now = Instant::now();
        let window = self.history.limits.merge_window;
        let can_merge = merge_tag.is_some()
            && self.history.redo.is_empty()
            && self.history.undo.back().is_some_and(|top| {
                top.merge_tag == merge_tag && window.is_none_or(|w| now.duration_since(top.time) <= w)
            });
        if can_merge && let Some(mut top) = self.history.undo.pop_back() {
            self.history.bytes = self.history.bytes.saturating_sub(top.bytes);
            top.snap.absorb(snap);
            self.prune_noops(&mut top.snap);
            if top.snap.has_no_document_change() {
                return true;
            }
            top.bytes = top.snap.bytes();
            top.serial = self.history.alloc_serial();
            top.time = now;
            self.history.bytes += top.bytes;
            self.history.undo.push_back(top);
            self.history.enforce_limits();
            return true;
        }
        let bytes = snap.bytes();
        let serial = self.history.alloc_serial();
        self.history.clear_redo();
        self.history.undo.push_back(UndoEntry {
            label,
            merge_tag,
            snap,
            bytes,
            serial,
            time: now,
            selection_only: false,
        });
        self.history.bytes += bytes;
        self.history.enforce_limits();
        true
    }

    fn prune_noops(&self, snap: &mut Snapshot) {
        let scene = &self.scene;
        snap.solids.retain(|id, before| match (before.as_ref(), scene.solids.get(id)) {
            (None, None) => false,
            (Some(a), Some(b)) => !(a.order == b.order && a.owner == b.owner && a.solid == b.solid),
            _ => true,
        });
        snap.entities.retain(|id, before| match (before.as_ref(), scene.entities.get(id)) {
            (None, None) => false,
            (Some(a), Some(b)) => a != b,
            _ => true,
        });
        snap.groups.retain(|id, before| match (before.as_ref(), scene.groups.get(id)) {
            (None, None) => false,
            (Some(a), Some(b)) => a != b,
            _ => true,
        });
        if snap.world.as_ref() == Some(&scene.world) {
            snap.world = None;
        }
        if snap.visgroups.as_ref().is_some_and(|(v, h)| *v == scene.visgroups && *h == scene.hidden_visgroups) {
            snap.visgroups = None;
        }
        if snap.selection.as_ref().is_some_and(|s| s.same_as(&self.selection)) {
            snap.selection = None;
        }
        if snap.misc.as_ref().is_some_and(|m| m.cameras == scene.cameras && m.cordons == scene.cordons) {
            snap.misc = None;
        }
        if snap.extra_blocks.as_ref() == Some(&scene.extra_blocks) {
            snap.extra_blocks = None;
        }
    }

    fn swap_in(&mut self, snap: &mut Snapshot) {
        let solids: Vec<(Id, Option<SolidNode>)> = snap.solids.drain(..).collect();
        let old = self.apply_solids_raw(solids);
        snap.solids.extend(old);
        let entities: Vec<(Id, Option<EntityNode>)> = snap.entities.drain(..).collect();
        let old = self.apply_entities_raw(entities);
        snap.entities.extend(old);
        let groups: Vec<(Id, Option<GroupNode>)> = snap.groups.drain(..).collect();
        let old = self.apply_groups_raw(groups);
        snap.groups.extend(old);
        if let Some(w) = &mut snap.world {
            std::mem::swap(w, &mut self.scene.world);
            self.changes.world_changed = true;
        }
        if let Some((v, h)) = &mut snap.visgroups {
            std::mem::swap(v, &mut self.scene.visgroups);
            std::mem::swap(h, &mut self.scene.hidden_visgroups);
            self.changes.visgroups_changed = true;
            self.changes.visibility_changed = true;
        }
        if let Some(s) = &mut snap.selection {
            let current = SelSnap::of(&self.selection);
            self.selection = std::mem::replace(s, current).into_selection();
            self.changes.selection_changed = true;
        }
        if let Some(m) = &mut snap.misc {
            std::mem::swap(&mut m.cameras, &mut self.scene.cameras);
            std::mem::swap(&mut m.cordons, &mut self.scene.cordons);
            self.changes.world_changed = true;
        }
        if let Some(x) = &mut snap.extra_blocks {
            std::mem::swap(x, &mut self.scene.extra_blocks);
            self.changes.extra_blocks_changed = true;
        }
        self.bump();
    }

    pub(crate) fn prune_selection(&mut self) {
        let scene = &self.scene;
        let stale_obj = self.selection.objects.iter().any(|o| !scene.contains(*o));
        let stale_face = self
            .selection
            .faces
            .iter()
            .any(|f| scene.solids.get(&f.solid).is_none_or(|n| f.side >= n.solid.sides.len()));
        if !stale_obj && !stale_face {
            return;
        }
        let objects: Vec<ObjRef> = self.selection.objects.iter().copied().filter(|o| scene.contains(*o)).collect();
        let faces: Vec<_> = self
            .selection
            .faces
            .iter()
            .copied()
            .filter(|f| scene.solids.get(&f.solid).is_some_and(|n| f.side < n.solid.sides.len()))
            .collect();
        self.selection.objects = objects.into_iter().collect();
        self.selection.faces = faces.into_iter().collect();
        self.changes.selection_changed = true;
        self.bump();
    }

    pub fn undo(&mut self) -> bool {
        self.step(true)
    }

    pub fn redo(&mut self) -> bool {
        self.step(false)
    }

    fn step(&mut self, undo: bool) -> bool {
        if self.history.tx.is_some() {
            return false;
        }
        self.end_selection_burst();
        let mut moved = false;
        loop {
            let popped = if undo { self.history.undo.pop_back() } else { self.history.redo.pop() };
            let Some(mut e) = popped else { break };
            moved = true;
            self.swap_in(&mut e.snap);
            self.history.bytes = self.history.bytes.saturating_sub(e.bytes);
            e.bytes = e.snap.bytes();
            self.history.bytes += e.bytes;
            self.prune_selection();
            let unchanged = e.snap.selection.as_ref().is_none_or(|s| s.same_as(&self.selection));
            if !unchanged {
                self.history.selection_restored = true;
            }
            let skip = e.selection_only && unchanged;
            if undo {
                self.history.redo.push(e);
            } else {
                self.history.undo.push_back(e);
            }
            let more = if undo { !self.history.undo.is_empty() } else { !self.history.redo.is_empty() };
            if !(skip && more) {
                break;
            }
        }
        if moved {
            self.changes.visibility_changed = true;
        }
        moved
    }

    pub fn set_record_selection(&mut self, on: bool) {
        if self.history.record_selection != on {
            self.end_selection_burst();
            self.history.record_selection = on;
        }
    }

    pub fn records_selection(&self) -> bool {
        self.history.record_selection
    }

    pub fn end_selection_burst(&mut self) {
        self.seal_selection_step();
        self.history.burst = Burst::None;
    }

    pub fn take_selection_restored(&mut self) -> bool {
        std::mem::take(&mut self.history.selection_restored)
    }

    pub fn undo_is_selection_step(&self) -> bool {
        self.history.undo.back().is_some_and(|e| e.selection_only)
    }

    fn note_selection_change(&mut self, before: Option<SelSnap>) {
        let h = &mut self.history;
        let top_selection_only = h.undo.back().map(|e| e.selection_only);
        match (h.burst, top_selection_only) {
            (Burst::Selection, Some(true)) if h.redo.is_empty() => return,
            (Burst::Edit, Some(false)) if h.redo.is_empty() => {
                if let Some(top) = h.undo.back_mut()
                    && top.snap.selection.is_none()
                {
                    top.snap.selection = Some(before.unwrap_or_else(|| SelSnap::of(&self.selection)));
                    h.bytes = h.bytes.saturating_sub(top.bytes);
                    top.bytes = top.snap.bytes();
                    h.bytes += top.bytes;
                    h.enforce_limits();
                }
                return;
            }
            _ => {}
        }
        let before = before.unwrap_or_else(|| SelSnap::of(&self.selection));
        let h = &mut self.history;
        let mut snap = Snapshot::new();
        snap.selection = Some(before);
        let bytes = snap.bytes();
        let serial = h.state_serial();
        h.clear_redo();
        h.undo.push_back(UndoEntry {
            label: SELECTION_STEP.to_owned(),
            merge_tag: None,
            snap,
            bytes,
            serial,
            time: Instant::now(),
            selection_only: true,
        });
        h.bytes += bytes;
        h.burst = Burst::Selection;
        h.enforce_limits();
    }

    pub(crate) fn refresh_selection_step(&mut self) {
        if self.history.burst != Burst::Selection || self.history.tx.is_some() {
            return;
        }
        let Some(top) = self.history.undo.back_mut().filter(|e| e.selection_only) else { return };
        let Some(before) = &top.snap.selection else { return };
        let objects = before.same_objects(&self.selection);
        let faces = before.same_faces(&self.selection);
        if objects && faces {
            if let Some(e) = self.history.undo.pop_back() {
                self.history.bytes = self.history.bytes.saturating_sub(e.bytes);
            }
            self.history.burst = Burst::None;
            return;
        }
        let label = if objects { FACE_SELECTION_STEP } else { SELECTION_STEP };
        if top.label != label {
            label.clone_into(&mut top.label);
        }
    }

    fn seal_selection_step(&mut self) {
        self.refresh_selection_step();
        if self.history.burst == Burst::Selection {
            self.history.burst = Burst::None;
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.tx.is_none() && !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        self.history.tx.is_none() && !self.history.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.history.undo.back().map(|e| e.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.history.redo.last().map(|e| e.label.as_str())
    }

    pub fn undo_labels(&self) -> Vec<&str> {
        self.history.undo.iter().map(|e| e.label.as_str()).collect()
    }

    pub fn history_len(&self) -> (usize, usize) {
        (self.history.undo.len(), self.history.redo.len())
    }

    pub fn history_bytes(&self) -> usize {
        self.history.bytes
    }

    pub fn undo_limits(&self) -> UndoLimits {
        self.history.limits
    }

    pub fn set_undo_limits(&mut self, limits: UndoLimits) {
        self.history.limits = limits;
        self.history.enforce_limits();
    }

    pub fn clear_history(&mut self) {
        let dirty = self.is_dirty();
        let undo = std::mem::take(&mut self.history.undo);
        let redo = std::mem::take(&mut self.history.redo);
        if self.history.bytes > 256 * 1024 {
            rayon::spawn(move || drop((undo, redo)));
        }
        self.history.bytes = 0;
        self.history.burst = Burst::None;
        self.history.base_serial = self.history.alloc_serial();
        self.saved_serial = if dirty { 0 } else { self.history.base_serial };
        if dirty {
            self.extra_dirty = true;
        }
    }

    pub fn set_world_key(&mut self, key: &str, value: &str) -> EditResult<()> {
        if key.trim().is_empty() || is_reserved_key(key) || key.eq_ignore_ascii_case("mapversion") {
            return Err(EditError::Invalid(format!("key '{key}' cannot be set")));
        }
        if self.scene.world.get(key) == Some(value) {
            return Ok(());
        }
        self.tx("Edit map properties", |doc| {
            doc.world_mut().set(key, value);
            Ok(())
        })
    }

    pub fn remove_world_key(&mut self, key: &str) -> EditResult<Option<String>> {
        if self.scene.world.get(key).is_none() {
            return Ok(None);
        }
        self.tx("Edit map properties", |doc| Ok(doc.world_mut().remove(key)))
    }

    pub fn set_cordons(&mut self, cordons: Cordons) -> EditResult<()> {
        if self.scene.cordons == cordons {
            return Ok(());
        }
        self.tx("Edit cordons", |doc| {
            *doc.misc_mut().1 = cordons;
            Ok(())
        })
    }

    pub fn set_cameras(&mut self, cameras: Cameras) {
        self.scene.cameras = cameras;
        self.changes.world_changed = true;
        self.bump();
    }

    pub fn set_view_setting(&mut self, key: &str, value: &str) {
        self.scene.view_settings.set(key, value);
        self.bump();
    }
}

pub(crate) fn is_reserved_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("id") || key.eq_ignore_ascii_case("classname")
}

#[cfg(test)]
mod manifest_tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-editor/scratch").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn opens_and_saves_a_vmm_manifest() {
        let dir = scratch("manifest");
        let vmm = dir.join("city.vmm");
        let submaps = dir.join("city");
        fs::create_dir_all(&submaps).expect("submap dir");
        let primary = submaps.join("main.vmf");
        hp_vmf::save_with(&Map::new_empty(), &primary, &SaveOptions::default()).expect("write submap");
        fs::write(&vmm, hp_vmf::Manifest::single("main.vmf").to_text()).expect("write manifest");

        let mut doc = Document::open(&vmm).expect("open manifest");
        assert!(doc.manifest().is_some(), "the manifest link is remembered");
        assert_eq!(doc.path(), Some(vmm.as_path()), "the manifest is the save target");
        assert_eq!(doc.manifest().unwrap().layout.len(), 1, "one submap merged in");

        fs::remove_file(&vmm).expect("remove manifest");
        fs::remove_file(&primary).expect("remove submap");
        doc.save_current().expect("save");
        assert!(vmm.exists(), "the manifest was rewritten");
        assert!(primary.exists(), "the submap was rewritten");
        let again = hp_vmf::Manifest::parse(&fs::read_to_string(&vmm).unwrap()).expect("reparse");
        assert_eq!(again.primary().unwrap().relative_vmf, "main.vmf");

        let other = dir.join("plain.vmf");
        doc.save(&other).expect("save as");
        assert!(doc.manifest().is_none(), "Save As detaches the manifest");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn edits_route_back_to_each_submap() {
        use hp_vmf::{Entity, Map};
        let dir = scratch("manifest_multi");
        let vmm = dir.join("city.vmm");
        let submaps = dir.join("city");
        fs::create_dir_all(&submaps).expect("submap dir");

        let point = |id, class: &str| {
            let mut m = Map::new_empty();
            m.entities.push(Entity { id, classname: class.to_owned(), ..Default::default() });
            m
        };
        hp_vmf::save_with(&point(1, "info_player_start"), &submaps.join("main.vmf"), &SaveOptions::default()).unwrap();
        hp_vmf::save_with(&point(1, "light"), &submaps.join("east.vmf"), &SaveOptions::default()).unwrap();
        let vmm_text = "\"Maps\"\n{\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"1\"\n\t\t\"InternalID\" \"1\"\n\t\t\"RelativeVMF\" \"main.vmf\"\n\t}\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"0\"\n\t\t\"InternalID\" \"2\"\n\t\t\"RelativeVMF\" \"east.vmf\"\n\t}\n}\n";
        fs::write(&vmm, vmm_text).unwrap();

        let mut doc = Document::open(&vmm).expect("open");
        assert_eq!(doc.scene().entities().len(), 2, "both submaps merged in");

        doc.save_current().expect("save");
        let main = hp_vmf::load(&submaps.join("main.vmf")).expect("main");
        let east = hp_vmf::load(&submaps.join("east.vmf")).expect("east");
        assert_eq!(main.entities.len(), 1);
        assert_eq!(main.entities[0].classname, "info_player_start");
        assert_eq!(east.entities.len(), 1);
        assert_eq!(east.entities[0].classname, "light");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn active_submap_and_protection() {
        use hp_vmf::{Entity, Map};
        let dir = scratch("manifest_active");
        let vmm = dir.join("city.vmm");
        let submaps = dir.join("city");
        fs::create_dir_all(&submaps).expect("submap dir");
        let empty = |class: &str| {
            let mut m = Map::new_empty();
            m.entities.push(Entity { id: 1, classname: class.to_owned(), ..Default::default() });
            m
        };
        hp_vmf::save_with(&empty("info_player_start"), &submaps.join("main.vmf"), &SaveOptions::default()).unwrap();
        hp_vmf::save_with(&empty("light"), &submaps.join("east.vmf"), &SaveOptions::default()).unwrap();
        let vmm_text = "\"Maps\"\n{\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"1\"\n\t\t\"InternalID\" \"1\"\n\t\t\"RelativeVMF\" \"main.vmf\"\n\t}\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"0\"\n\t\t\"IsProtected\" \"1\"\n\t\t\"InternalID\" \"2\"\n\t\t\"RelativeVMF\" \"east.vmf\"\n\t}\n}\n";
        fs::write(&vmm, vmm_text).unwrap();

        let mut doc = Document::open(&vmm).expect("open");
        assert_eq!(doc.manifest_submaps().len(), 2);
        assert!(doc.manifest_submaps()[1].is_protected);
        assert_eq!(doc.active_submap(), Some(0));
        assert!(!doc.set_active_submap(1), "a protected submap cannot be active");
        assert_eq!(doc.active_submap(), Some(0));

        let east_path = submaps.join("east.vmf");
        let before = fs::read_to_string(&east_path).unwrap();
        doc.save_current().expect("save");
        assert_eq!(fs::read_to_string(&east_path).unwrap(), before, "the protected submap was left untouched");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_brushes_go_to_the_active_submap() {
        use crate::Owner;
        use crate::tests::box_sides;
        use glam::DVec3;
        use hp_vmf::{Map, Solid};
        let dir = scratch("manifest_new_brush");
        let vmm = dir.join("city.vmm");
        let submaps = dir.join("city");
        fs::create_dir_all(&submaps).expect("submap dir");
        let brush_map = |x: f64| {
            let mut m = Map::new_empty();
            let mut sides =
                box_sides(DVec3::new(x, 0.0, 0.0), DVec3::new(x + 64.0, 64.0, 64.0), "dev/dev_measurewall01a");
            for (k, s) in sides.iter_mut().enumerate() {
                s.id = 21 + k as u32;
            }
            m.world.solids.push(Solid { id: 2, sides, ..Default::default() });
            m
        };
        for (name, x) in [("main.vmf", 0.0), ("east.vmf", 256.0), ("west.vmf", -256.0)] {
            hp_vmf::save_with(&brush_map(x), &submaps.join(name), &SaveOptions::default()).unwrap();
        }
        let vmm_text = "\"Maps\"\n{\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"1\"\n\t\t\"InternalID\" \"1\"\n\t\t\"RelativeVMF\" \"main.vmf\"\n\t}\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"0\"\n\t\t\"InternalID\" \"2\"\n\t\t\"RelativeVMF\" \"east.vmf\"\n\t}\n\t\"VMFEntry\"\n\t{\n\t\t\"IsPrimary\" \"0\"\n\t\t\"IsProtected\" \"1\"\n\t\t\"InternalID\" \"3\"\n\t\t\"RelativeVMF\" \"west.vmf\"\n\t}\n}\n";
        fs::write(&vmm, vmm_text).unwrap();
        let west_before = fs::read_to_string(submaps.join("west.vmf")).unwrap();

        let mut doc = Document::open(&vmm).expect("open");
        assert_eq!(doc.scene().solids.len(), 3, "three brushes merged");
        assert!(doc.set_active_submap(1), "east can be active");
        doc.create_solid(
            box_sides(DVec3::new(0.0, 256.0, 0.0), DVec3::new(64.0, 320.0, 64.0), "dev/dev_measuregeneric01b"),
            Owner::World,
        )
        .expect("new brush");
        doc.save_current().expect("save");

        let count = |name: &str| hp_vmf::load(&submaps.join(name)).expect(name).world.solids.len();
        assert_eq!(count("main.vmf"), 1, "main keeps its own brush only");
        assert_eq!(count("east.vmf"), 2, "the new brush went to the active submap");
        assert_eq!(fs::read_to_string(submaps.join("west.vmf")).unwrap(), west_before, "protected file untouched");

        let _ = fs::remove_dir_all(&dir);
    }
}
