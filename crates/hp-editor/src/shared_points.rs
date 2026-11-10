use hp_kv::Block;
use hp_vmf::{Entity, Map, Solid, VisGroup};
use rayon::prelude::*;

use crate::curve_meta::editor_curve_tag;
use crate::history::Snapshot;
use crate::ids::{IdSet, id_set};
use crate::scene::{EntityNode, GroupNode, Owner, SolidNode};
use crate::{Document, EditError, EditResult, Id};

pub const SHARED_UPDATE_LABEL: &str = "Team Mapping update";
const SHARED_TAG: &str = "team_mapping_remote";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SharedTouched {
    pub solids: Vec<Id>,
    pub entities: Vec<Id>,
    pub world: bool,
    pub metadata: bool,
}

impl SharedTouched {
    pub fn is_empty(&self) -> bool {
        self.solids.is_empty() && self.entities.is_empty() && !self.world && !self.metadata
    }

    pub fn len(&self) -> usize {
        self.solids.len() + self.entities.len() + usize::from(self.world) + usize::from(self.metadata)
    }

    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .solids
            .iter()
            .map(|id| format!("solid:{id}"))
            .chain(self.entities.iter().map(|id| format!("entity:{id}")))
            .collect();
        if self.world {
            keys.push("world".into());
        }
        if self.metadata {
            keys.push("metadata".into());
        }
        keys
    }

    fn collect(&mut self, snapshot: &Snapshot, solids: &mut IdSet, entities: &mut IdSet) {
        solids.extend(snapshot.solids.keys().copied());
        solids.extend(snapshot.solid_patches.keys().copied());
        entities.extend(snapshot.entities.keys().copied());
        entities.extend(snapshot.entity_patches.keys().copied());
        self.world |= !snapshot.groups.is_empty() || snapshot.world.is_some();
        self.metadata |= snapshot.visgroups.is_some() || snapshot.extra_blocks.is_some();
    }
}

#[derive(Clone, Debug, Default)]
pub struct SharedExport {
    pub solids: Vec<(Id, Option<(Solid, Id)>)>,
    pub entities: Vec<(Id, Option<(Entity, bool)>)>,
    pub header: Option<Map>,
}

#[derive(Clone, Debug, Default)]
pub struct SharedUpdate {
    pub world: Option<Entity>,
    pub metadata: Option<(Vec<VisGroup>, Vec<Block>)>,
    pub entities: Vec<(Id, Option<Entity>)>,
    pub solids: Vec<(Id, Option<(Solid, Owner)>)>,
}

impl SharedUpdate {
    pub fn is_empty(&self) -> bool {
        self.world.is_none() && self.metadata.is_none() && self.entities.is_empty() && self.solids.is_empty()
    }
}

impl Document {
    pub fn shared_touched_since(&self, serial: u64) -> Option<SharedTouched> {
        if self.in_transaction() {
            return None;
        }
        self.shared_touched(serial, false)
    }

    pub fn shared_touched_live(&self, serial: u64) -> Option<SharedTouched> {
        self.shared_touched(serial, true)
    }

    fn shared_touched(&self, serial: u64, live: bool) -> Option<SharedTouched> {
        let current = self.content_serial();
        if serial > current || self.history.base_serial > serial {
            return None;
        }
        let mut touched = SharedTouched::default();
        let (mut solids, mut entities) = (id_set(0), id_set(0));
        for edit in self.history.undo.iter().rev().take_while(|edit| edit.serial > serial) {
            if edit.merge_tag.as_deref() != Some(SHARED_TAG) {
                touched.collect(&edit.snap, &mut solids, &mut entities);
            }
        }
        if live && let Some(tx) = &self.history.tx {
            touched.collect(&tx.snap, &mut solids, &mut entities);
        }
        touched.solids = solids.into_iter().collect();
        touched.entities = entities.into_iter().collect();
        Some(touched)
    }

    pub fn shared_transaction_keys(&self) -> Option<SharedTouched> {
        let tx = self.history.tx.as_ref()?;
        let mut touched = SharedTouched::default();
        let (mut solids, mut entities) = (id_set(0), id_set(0));
        touched.collect(&tx.snap, &mut solids, &mut entities);
        touched.solids = solids.into_iter().collect();
        touched.entities = entities.into_iter().collect();
        Some(touched)
    }

    pub fn shared_export(&self, touched: &SharedTouched) -> SharedExport {
        let scene = &self.scene;
        let mut ids = id_set(touched.solids.len());
        ids.extend(touched.solids.iter().copied());
        let roads: std::collections::HashSet<&str> = touched
            .solids
            .iter()
            .filter_map(|id| scene.solid(*id))
            .filter_map(|node| editor_curve_tag(&node.solid.editor))
            .collect();
        if !roads.is_empty() {
            for node in scene.solids.values() {
                if editor_curve_tag(&node.solid.editor).is_some_and(|tag| roads.contains(tag)) {
                    ids.insert(node.solid.id);
                }
            }
        }
        let owner_of = |node: &SolidNode| match node.owner {
            Owner::Entity(entity) if scene.entities.contains_key(&entity) => entity,
            _ => scene.world.id,
        };
        let excluded = |node: &SolidNode| {
            let Owner::Entity(entity) = node.owner else { return false };
            scene.entity(entity).is_some_and(|owner| hp_vmf::is_world_particle_volume(&owner.entity.classname))
                && node.solid.sides.first().is_some_and(|side| {
                    side.material.replace('\\', "/").trim().eq_ignore_ascii_case("tools/toolsorigin")
                })
        };
        let mut road_map = Map::new_empty();
        let mut solids = Vec::with_capacity(ids.len());
        for id in ids {
            match scene.solid(id) {
                Some(node) if !excluded(node) => {
                    if editor_curve_tag(&node.solid.editor).is_some() {
                        match node.owner {
                            Owner::Entity(entity) if scene.entities.contains_key(&entity) => {
                                road_map.entities.push(Entity {
                                    id: entity,
                                    solids: vec![node.solid.clone()],
                                    ..Default::default()
                                });
                            }
                            _ => road_map.world.solids.push(node.solid.clone()),
                        }
                    }
                    solids.push((id, Some((node.solid.clone(), owner_of(node)))));
                }
                _ => solids.push((id, None)),
            }
        }
        if !road_map.world.solids.is_empty() || !road_map.entities.is_empty() {
            crate::curve_meta::prepare_road_contents(&mut road_map, scene);
            let prepared: std::collections::HashMap<Id, Solid> = road_map
                .world
                .solids
                .into_iter()
                .chain(road_map.entities.into_iter().flat_map(|entity| entity.solids))
                .map(|solid| (solid.id, solid))
                .collect();
            for (id, row) in &mut solids {
                if let (Some(solid), Some((current, _))) = (prepared.get(id), row.as_mut()) {
                    *current = solid.clone();
                }
            }
        }
        let entities = touched
            .entities
            .iter()
            .map(|id| (*id, scene.entity(*id).map(|node| (node.entity.clone(), scene.is_brush_entity(*id)))))
            .collect();
        let header = (touched.world || touched.metadata).then(|| {
            let mut world = scene.world.clone();
            world.solids.clear();
            world.groups = scene.groups.values().map(|node| node.group.clone()).collect();
            Map {
                version_info: scene.version_info.clone(),
                visgroups: scene.visgroups.clone(),
                world,
                extra_blocks: scene.extra_blocks.clone(),
                format: scene.format,
                ..Map::new_empty()
            }
        });
        SharedExport { solids, entities, header }
    }

    pub fn apply_shared_update(&mut self, update: SharedUpdate) -> EditResult<()> {
        if update.is_empty() {
            return Ok(());
        }
        if let Some(keys) = self.shared_transaction_keys() {
            let world = update.world.is_some() && keys.world;
            let metadata = update.metadata.is_some() && keys.metadata;
            let solids = update.solids.iter().any(|(id, _)| keys.solids.contains(id));
            let entities = update.entities.iter().any(|(id, _)| keys.entities.contains(id));
            if world || metadata || solids || entities {
                return Err(EditError::TransactionOpen);
            }
            self.write_shared(update, false);
            return Ok(());
        }
        self.begin_merge(SHARED_UPDATE_LABEL, SHARED_TAG);
        self.write_shared(update, true);
        self.commit();
        Ok(())
    }

    fn write_shared(&mut self, update: SharedUpdate, recorded: bool) {
        let SharedUpdate { world, metadata, entities, solids } = update;
        let visibility = self.changes.visibility_changed;
        self.changes.visibility_changed = false;
        let mut groups_touched = false;
        if let Some(mut header) = world {
            let groups = std::mem::take(&mut header.groups);
            header.solids.clear();
            header.hidden = false;
            if header != self.scene.world {
                if recorded {
                    *self.world_mut() = header;
                } else {
                    self.scene.world = header;
                    self.changes.world_changed = true;
                    self.bump();
                }
            }
            let keep: IdSet = groups.iter().map(|group| group.id).collect();
            let removed: Vec<(Id, Option<GroupNode>)> =
                self.scene.groups.keys().filter(|id| !keep.contains(*id)).map(|id| (*id, None)).collect();
            let mut states = removed;
            for group in groups {
                match self.scene.groups.get(&group.id) {
                    Some(node) if node.group == group => {}
                    Some(node) => states.push((group.id, Some(GroupNode { order: node.order, group }))),
                    None => {
                        let order = self.scene.alloc_order();
                        states.push((group.id, Some(GroupNode { group, order })));
                    }
                }
            }
            if !states.is_empty() {
                groups_touched = true;
                if recorded {
                    self.write_groups(states);
                } else {
                    self.apply_groups_raw(states);
                }
            }
        }
        if let Some((visgroups, extra_blocks)) = metadata {
            if visgroups != self.scene.visgroups {
                if recorded {
                    *self.visgroups_mut().0 = visgroups;
                } else {
                    self.scene.visgroups = visgroups;
                    self.changes.visgroups_changed = true;
                    self.changes.visibility_changed = true;
                    self.bump();
                }
            }
            if extra_blocks != self.scene.extra_blocks {
                if recorded {
                    *self.extra_blocks_mut() = extra_blocks;
                } else {
                    self.scene.extra_blocks = extra_blocks;
                    self.changes.extra_blocks_changed = true;
                    self.bump();
                }
            }
        }
        let entity_states: Vec<(Id, Option<EntityNode>)> = entities
            .into_iter()
            .filter_map(|(id, entity)| match (entity, self.scene.entities.get(&id)) {
                (None, None) => None,
                (None, Some(_)) => Some((id, None)),
                (Some(mut entity), current) => {
                    entity.solids.clear();
                    entity.groups.clear();
                    match current {
                        Some(node) if node.entity == entity => None,
                        Some(node) => Some((id, Some(EntityNode { order: node.order, entity }))),
                        None => {
                            let order = self.scene.alloc_order();
                            Some((id, Some(EntityNode { entity, order })))
                        }
                    }
                }
            })
            .collect();
        if !entity_states.is_empty() {
            if recorded {
                self.write_entities(entity_states);
            } else {
                self.apply_entities_raw(entity_states);
            }
        }
        let scene = &self.scene;
        let start = self.scene.next_order;
        let fresh =
            solids.iter().filter(|(id, solid)| solid.is_some() && !scene.solids.contains_key(id)).count() as u64;
        let mut order = start;
        let mut jobs = Vec::with_capacity(solids.len());
        for (id, solid) in solids {
            let current = scene.solids.get(&id);
            match (solid, current) {
                (None, None) => {}
                (None, Some(_)) => jobs.push((id, None, 0)),
                (Some((solid, owner)), current) => {
                    if current.is_some_and(|node| node.solid == solid && node.owner == owner) {
                        continue;
                    }
                    let slot = current.map_or_else(
                        || {
                            order += 1;
                            order - 1
                        },
                        SolidNode::order,
                    );
                    jobs.push((id, Some((solid, owner)), slot));
                }
            }
        }
        let built: Vec<(Id, Option<SolidNode>)> = jobs
            .into_par_iter()
            .map(|(id, solid, slot)| {
                let node = solid.map(|(solid, owner)| match scene.solids.get(&id) {
                    Some(current) => {
                        let reuse = current.solid.sides.len() == solid.sides.len()
                            && current
                                .solid
                                .sides
                                .iter()
                                .zip(&solid.sides)
                                .all(|(old, new)| old.vertices_plus == new.vertices_plus);
                        let mut node =
                            if reuse { current.with_solid(solid) } else { SolidNode::build(solid, owner, slot) };
                        node.owner = owner;
                        node
                    }
                    None => SolidNode::build(solid, owner, slot),
                });
                (id, node)
            })
            .collect();
        self.scene.next_order = start.saturating_add(fresh);
        if !built.is_empty() {
            if recorded {
                self.write_solids(built);
            } else {
                self.apply_solids_raw(built);
            }
        }
        if self.changes.visibility_changed || groups_touched {
            self.scene.hidden_visgroups = self.scene.derive_hidden_visgroups();
            self.changes.visibility_changed = true;
        }
        self.changes.visibility_changed |= visibility;
        self.prune_selection();
    }
}

impl Document {
    pub fn adopt_shared(&mut self, prepared: Document, id_start: Id, id_end: Id) -> EditResult<()> {
        if self.in_transaction() {
            return Err(EditError::TransactionOpen);
        }
        if !prepared.renumbered.is_empty() {
            return Err(EditError::Invalid("Shared map has duplicate identifiers".into()));
        }
        let mut scene = prepared.scene;
        scene.cameras = self.scene.cameras.clone();
        scene.view_settings = self.scene.view_settings.clone();
        scene.cordons = self.scene.cordons.clone();
        let previous = std::mem::replace(&mut self.scene, scene);
        rayon::spawn(move || drop(previous));
        self.pick = std::sync::Mutex::new(crate::pick::PickCache::default());
        self.changes.everything = true;
        self.clear_history();
        self.prune_selection();
        self.mark_dirty();
        self.bump();
        self.configure_shared_ids(id_start, id_end)
    }
}
