use std::sync::{Arc, OnceLock};

use hp_geom::{Aabb, DispMesh, SolidGeom};
use hp_kv::Block;
use hp_vmf::{Cameras, Cordons, EditorInfo, Entity, Group, Map, Solid, TextFormat, VersionInfo, VisGroup};
use rayon::prelude::*;

use crate::Id;
use crate::ids::{IdMap, IdSet, id_map, id_set};
use crate::selection::ObjRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Owner {
    #[default]
    World,
    Entity(Id),
}

#[derive(Clone, Debug)]
pub struct SolidNode {
    pub solid: Solid,
    pub owner: Owner,
    pub geom: Arc<SolidGeom>,
    pub disp: Vec<Option<Arc<DispMesh>>>,
    pub bounds: Aabb,
    pub(crate) sweep: OnceLock<Arc<hp_geom::SweepHull>>,
    pub(crate) order: u64,
}

impl SolidNode {
    pub(crate) fn build(solid: Solid, owner: Owner, order: u64) -> Self {
        let geom = Arc::new(hp_geom::build_solid(&solid.sides));
        let disp = build_disps(&solid, &geom);
        let bounds = node_bounds(&geom, &disp);
        Self { solid, owner, geom, disp, bounds, sweep: OnceLock::new(), order }
    }

    pub(crate) fn with_solid(&self, solid: Solid) -> Self {
        let same_planes = solid.sides.len() == self.solid.sides.len()
            && solid.sides.iter().zip(&self.solid.sides).all(|(a, b)| a.plane == b.plane);
        if !same_planes {
            return Self::build(solid, self.owner, self.order);
        }
        let geom = Arc::clone(&self.geom);
        let any_disp = solid.sides.iter().any(|s| s.disp.is_some());
        let disp = if any_disp {
            solid
                .sides
                .iter()
                .enumerate()
                .map(|(i, side)| {
                    let d = side.disp.as_ref()?;
                    let old_side = self.solid.sides.get(i)?;
                    if old_side.disp.as_ref() == Some(d)
                        && let Some(Some(mesh)) = self.disp.get(i)
                    {
                        return Some(Arc::clone(mesh));
                    }
                    let face = geom.face_for_side(i)?;
                    hp_geom::build_disp(&face.vertices, d).map(Arc::new)
                })
                .collect()
        } else {
            Vec::new()
        };
        let bounds = node_bounds(&geom, &disp);
        Self { solid, owner: self.owner, geom, disp, bounds, sweep: self.sweep.clone(), order: self.order }
    }

    pub fn disp_mesh(&self, side: usize) -> Option<&Arc<DispMesh>> {
        self.disp.get(side).and_then(Option::as_ref)
    }

    pub fn has_disp(&self) -> bool {
        self.solid.sides.iter().any(|s| s.disp.is_some())
    }

    pub fn order(&self) -> u64 {
        self.order
    }
}

fn build_disps(solid: &Solid, geom: &SolidGeom) -> Vec<Option<Arc<DispMesh>>> {
    if !solid.sides.iter().any(|s| s.disp.is_some()) {
        return Vec::new();
    }
    solid
        .sides
        .iter()
        .enumerate()
        .map(|(i, side)| {
            let d = side.disp.as_ref()?;
            let face = geom.face_for_side(i)?;
            hp_geom::build_disp(&face.vertices, d).map(Arc::new)
        })
        .collect()
}

fn node_bounds(geom: &SolidGeom, disp: &[Option<Arc<DispMesh>>]) -> Aabb {
    let mut b = geom.aabb;
    for m in disp.iter().flatten() {
        if !m.aabb.is_empty() {
            b = b.union(&m.aabb);
        }
    }
    b
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityNode {
    pub entity: Entity,
    pub(crate) order: u64,
}

impl EntityNode {
    pub fn order(&self) -> u64 {
        self.order
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupNode {
    pub group: Group,
    pub(crate) order: u64,
}

impl GroupNode {
    pub fn order(&self) -> u64 {
        self.order
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Renumbered {
    Solid { old: Id, new: Id },
    Entity { old: Id, new: Id },
    Group { old: Id, new: Id },
}

pub(crate) struct LoadInfo {
    pub renumbered: Vec<Renumbered>,
    pub next_object_id: Id,
    pub next_side_id: Id,
    pub next_visgroup_id: Id,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub(crate) solids: IdMap<SolidNode>,
    pub(crate) entities: IdMap<EntityNode>,
    pub(crate) groups: IdMap<GroupNode>,
    pub(crate) world: Entity,
    pub(crate) visgroups: Vec<VisGroup>,
    pub(crate) hidden_visgroups: IdSet,
    pub(crate) cameras: Cameras,
    pub(crate) cordons: Cordons,
    pub(crate) view_settings: Block,
    pub(crate) version_info: VersionInfo,
    pub(crate) extra_blocks: Vec<Block>,
    pub(crate) format: TextFormat,
    pub(crate) entity_solids: IdMap<IdSet>,
    pub(crate) next_order: u64,
}

impl Default for Scene {
    fn default() -> Self {
        Self::from_map(Map::new_empty()).0
    }
}

impl Scene {
    pub(crate) fn from_map(map: Map) -> (Scene, LoadInfo) {
        let Map { version_info, visgroups, view_settings, mut world, entities, cameras, cordons, extra_blocks, format } =
            map;
        let mut max_id = world.id;
        for e in &entities {
            max_id = max_id.max(e.id);
            for s in &e.solids {
                max_id = max_id.max(s.id);
            }
        }
        for s in &world.solids {
            max_id = max_id.max(s.id);
        }
        for g in &world.groups {
            max_id = max_id.max(g.id);
        }
        let mut next_id = max_id.saturating_add(1);
        let mut renumbered = Vec::new();
        let mut fresh = || {
            let id = next_id;
            next_id = next_id.saturating_add(1);
            id
        };

        let world_solids = std::mem::take(&mut world.solids);
        let raw_groups = std::mem::take(&mut world.groups);
        world.hidden = false;

        let mut seen_entities = id_set(entities.len());
        let mut raw_solids: Vec<(Owner, Solid)> = Vec::with_capacity(world_solids.len());
        raw_solids.extend(world_solids.into_iter().map(|s| (Owner::World, s)));
        let mut entity_list = Vec::with_capacity(entities.len());
        for mut e in entities {
            if e.id == 0 || !seen_entities.insert(e.id) {
                let new = fresh();
                renumbered.push(Renumbered::Entity { old: e.id, new });
                e.id = new;
                seen_entities.insert(new);
            }
            let solids = std::mem::take(&mut e.solids);
            raw_solids.extend(solids.into_iter().map(|s| (Owner::Entity(e.id), s)));
            entity_list.push(e);
        }

        let mut seen_solids = id_set(raw_solids.len());
        for (_, s) in raw_solids.iter_mut() {
            if s.id == 0 || !seen_solids.insert(s.id) {
                let new = fresh();
                renumbered.push(Renumbered::Solid { old: s.id, new });
                s.id = new;
                seen_solids.insert(new);
            }
        }
        let mut groups = id_map(raw_groups.len());
        for (i, mut g) in raw_groups.into_iter().enumerate() {
            if g.id == 0 || groups.contains_key(&g.id) {
                let new = fresh();
                renumbered.push(Renumbered::Group { old: g.id, new });
                g.id = new;
            }
            groups.insert(g.id, GroupNode { group: g, order: i as u64 });
        }

        let max_side = raw_solids.iter().flat_map(|(_, s)| s.sides.iter()).map(|s| s.id).max().unwrap_or(0);

        let nodes: Vec<SolidNode> = raw_solids
            .into_par_iter()
            .enumerate()
            .map(|(i, (owner, solid))| SolidNode::build(solid, owner, i as u64))
            .collect();
        let mut solids = id_map(nodes.len());
        let mut entity_solids: IdMap<IdSet> = id_map(entity_list.len() / 4 + 1);
        for node in nodes {
            if let Owner::Entity(e) = node.owner {
                entity_solids.entry(e).or_insert_with(|| id_set(4)).insert(node.solid.id);
            }
            solids.insert(node.solid.id, node);
        }
        let mut ents = id_map(entity_list.len());
        for (i, e) in entity_list.into_iter().enumerate() {
            ents.insert(e.id, EntityNode { entity: e, order: i as u64 });
        }

        let next_order = solids.len().max(ents.len()).max(groups.len()) as u64;
        let mut scene = Scene {
            solids,
            entities: ents,
            groups,
            world,
            visgroups,
            hidden_visgroups: id_set(0),
            cameras,
            cordons,
            view_settings,
            version_info,
            extra_blocks,
            format,
            entity_solids,
            next_order,
        };
        scene.hidden_visgroups = scene.derive_hidden_visgroups();
        let max_vis = max_visgroup_id(&scene.visgroups);
        let info = LoadInfo {
            renumbered,
            next_object_id: next_id,
            next_side_id: max_side.saturating_add(1),
            next_visgroup_id: max_vis.saturating_add(1),
        };
        (scene, info)
    }

    pub fn to_map(&self) -> Map {
        let mut world = self.world.clone();
        world.groups = self.groups.values().map(|g| g.group.clone()).collect();
        let mut entities: Vec<Entity> = self.entities.values().map(|n| n.entity.clone()).collect();
        let positions: IdMap<usize> = {
            let mut m = id_map(self.entities.len());
            for (i, id) in self.entities.keys().enumerate() {
                m.insert(*id, i);
            }
            m
        };
        let solids: Vec<(Owner, Solid)> = self.solids.par_values().map(|n| (n.owner, n.solid.clone())).collect();
        for (owner, solid) in solids {
            let slot = match owner {
                Owner::World => None,
                Owner::Entity(e) => positions.get(&e).and_then(|&i| entities.get_mut(i)),
            };
            match slot {
                Some(ent) => ent.solids.push(solid),
                None => world.solids.push(solid),
            }
        }
        Map {
            version_info: self.version_info.clone(),
            visgroups: self.visgroups.clone(),
            view_settings: self.view_settings.clone(),
            world,
            entities,
            cameras: self.cameras.clone(),
            cordons: self.cordons.clone(),
            extra_blocks: self.extra_blocks.clone(),
            format: self.format,
        }
    }

    pub fn solids(&self) -> &IdMap<SolidNode> {
        &self.solids
    }

    pub fn solid(&self, id: Id) -> Option<&SolidNode> {
        self.solids.get(&id)
    }

    pub fn entities(&self) -> &IdMap<EntityNode> {
        &self.entities
    }

    pub fn entity(&self, id: Id) -> Option<&EntityNode> {
        self.entities.get(&id)
    }

    pub fn groups(&self) -> &IdMap<GroupNode> {
        &self.groups
    }

    pub fn group(&self, id: Id) -> Option<&GroupNode> {
        self.groups.get(&id)
    }

    pub fn world(&self) -> &Entity {
        &self.world
    }

    pub fn visgroups(&self) -> &[VisGroup] {
        &self.visgroups
    }

    pub fn hidden_visgroups(&self) -> &IdSet {
        &self.hidden_visgroups
    }

    pub fn cameras(&self) -> &Cameras {
        &self.cameras
    }

    pub fn cordons(&self) -> &Cordons {
        &self.cordons
    }

    pub fn view_settings(&self) -> &Block {
        &self.view_settings
    }

    pub fn version_info(&self) -> &VersionInfo {
        &self.version_info
    }

    pub fn extra_blocks(&self) -> &[Block] {
        &self.extra_blocks
    }

    pub fn text_format(&self) -> &TextFormat {
        &self.format
    }

    pub fn entity_solid_ids(&self, entity: Id) -> Vec<Id> {
        let Some(set) = self.entity_solids.get(&entity) else {
            return Vec::new();
        };
        let mut ids: Vec<(u64, Id)> = set.iter().filter_map(|id| self.solids.get(id).map(|n| (n.order, *id))).collect();
        ids.sort_unstable();
        ids.into_iter().map(|(_, id)| id).collect()
    }

    pub fn entity_solid_count(&self, entity: Id) -> usize {
        self.entity_solids.get(&entity).map_or(0, IdSet::len)
    }

    pub fn is_brush_entity(&self, entity: Id) -> bool {
        self.entity_solid_count(entity) > 0
    }

    pub fn contains(&self, obj: ObjRef) -> bool {
        match obj {
            ObjRef::Solid(id) => self.solids.contains_key(&id),
            ObjRef::Entity(id) => self.entities.contains_key(&id),
        }
    }

    pub fn editor_info(&self, obj: ObjRef) -> Option<&EditorInfo> {
        match obj {
            ObjRef::Solid(id) => self.solids.get(&id).map(|n| &n.solid.editor),
            ObjRef::Entity(id) => self.entities.get(&id).map(|n| &n.entity.editor),
        }
    }

    pub fn object_of_solid(&self, id: Id) -> ObjRef {
        match self.solids.get(&id).map(|n| n.owner) {
            Some(Owner::Entity(e)) if self.entities.contains_key(&e) => ObjRef::Entity(e),
            _ => ObjRef::Solid(id),
        }
    }

    pub fn group_parent(&self, group: Id) -> Option<Id> {
        let parent = self.groups.get(&group)?.group.editor.group_id?;
        (parent != group && self.groups.contains_key(&parent)).then_some(parent)
    }

    pub fn group_root(&self, group: Id) -> Id {
        let mut cur = group;
        for _ in 0..=self.groups.len() {
            match self.group_parent(cur) {
                Some(p) => cur = p,
                None => break,
            }
        }
        cur
    }

    pub fn object_group(&self, obj: ObjRef) -> Option<Id> {
        let gid = self.editor_info(obj)?.group_id?;
        self.groups.contains_key(&gid).then_some(gid)
    }

    pub fn group_subtree(&self, group: Id) -> IdSet {
        let mut out = id_set(4);
        if !self.groups.contains_key(&group) {
            return out;
        }
        out.insert(group);
        for gid in self.groups.keys() {
            if *gid == group {
                continue;
            }
            let mut cur = *gid;
            for _ in 0..=self.groups.len() {
                match self.group_parent(cur) {
                    Some(p) if p == group => {
                        out.insert(*gid);
                        break;
                    }
                    Some(p) => cur = p,
                    None => break,
                }
            }
        }
        out
    }

    pub fn members_of_groups(&self, groups: &IdSet) -> Vec<ObjRef> {
        if groups.is_empty() {
            return Vec::new();
        }
        let in_set = |e: &EditorInfo| e.group_id.is_some_and(|g| groups.contains(&g));
        let mut out: Vec<ObjRef> = self
            .solids
            .values()
            .filter(|n| n.owner == Owner::World && in_set(&n.solid.editor))
            .map(|n| ObjRef::Solid(n.solid.id))
            .collect();
        out.extend(self.entities.values().filter(|n| in_set(&n.entity.editor)).map(|n| ObjRef::Entity(n.entity.id)));
        out
    }

    pub fn group_members(&self, group: Id) -> Vec<ObjRef> {
        self.members_of_groups(&self.group_subtree(group))
    }

    pub(crate) fn group_index(&self) -> GroupIndex {
        let mut members: IdMap<Vec<ObjRef>> = id_map(self.groups.len());
        let mut children: IdMap<Vec<Id>> = id_map(self.groups.len());
        for n in self.solids.values() {
            if n.owner == Owner::World
                && let Some(g) = n.solid.editor.group_id
            {
                members.entry(g).or_default().push(ObjRef::Solid(n.solid.id));
            }
        }
        for n in self.entities.values() {
            if let Some(g) = n.entity.editor.group_id {
                members.entry(g).or_default().push(ObjRef::Entity(n.entity.id));
            }
        }
        for gid in self.groups.keys() {
            if let Some(p) = self.group_parent(*gid) {
                children.entry(p).or_default().push(*gid);
            }
        }
        GroupIndex { members, children }
    }

    pub fn solid_bounds(&self, id: Id) -> Option<Aabb> {
        let n = self.solids.get(&id)?;
        (!n.bounds.is_empty()).then_some(n.bounds)
    }

    pub fn entity_solid_bounds(&self, entity: Id) -> Option<Aabb> {
        let set = self.entity_solids.get(&entity)?;
        let mut b = Aabb::EMPTY;
        for id in set {
            if let Some(n) = self.solids.get(id)
                && !n.bounds.is_empty()
            {
                b = b.union(&n.bounds);
            }
        }
        (!b.is_empty()).then_some(b)
    }

    pub(crate) fn alloc_order(&mut self) -> u64 {
        let o = self.next_order;
        self.next_order = self.next_order.saturating_add(1);
        o
    }

    pub(crate) fn derive_hidden_visgroups(&self) -> IdSet {
        let mut counts: IdMap<(usize, usize)> = id_map(8);
        let mut add = |e: &EditorInfo| {
            for v in &e.visgroup_ids {
                let c = counts.entry(*v).or_insert((0, 0));
                c.0 += 1;
                if e.visgroup_shown {
                    c.1 += 1;
                }
            }
        };
        for n in self.solids.values() {
            add(&n.solid.editor);
        }
        for n in self.entities.values() {
            add(&n.entity.editor);
        }
        for n in self.groups.values() {
            add(&n.group.editor);
        }
        let mut out = id_set(0);
        for (id, (members, shown)) in counts {
            if members > 0 && shown == 0 {
                out.insert(id);
            }
        }
        out
    }
}

pub(crate) struct GroupIndex {
    members: IdMap<Vec<ObjRef>>,
    children: IdMap<Vec<Id>>,
}

impl GroupIndex {
    pub fn subtree(&self, group: Id) -> IdSet {
        let mut out = id_set(4);
        let mut stack = vec![group];
        while let Some(g) = stack.pop() {
            if out.insert(g)
                && let Some(c) = self.children.get(&g)
            {
                stack.extend(c.iter().copied());
            }
        }
        out
    }

    pub fn subtree_members(&self, group: Id) -> Vec<ObjRef> {
        let mut out = Vec::new();
        for g in self.subtree(group) {
            if let Some(m) = self.members.get(&g) {
                out.extend(m.iter().copied());
            }
        }
        out
    }
}

pub(crate) fn max_visgroup_id(groups: &[VisGroup]) -> Id {
    let mut best = 0;
    let mut stack: Vec<&VisGroup> = groups.iter().collect();
    while let Some(g) = stack.pop() {
        best = best.max(g.id);
        stack.extend(g.children.iter());
    }
    best
}
