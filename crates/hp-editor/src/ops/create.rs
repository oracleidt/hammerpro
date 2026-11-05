use std::path::Path;

use glam::{DMat4, DVec3};
use hp_geom::{PrimitiveParams, make_primitive, validate_solid};
use hp_vmf::{EditorInfo, Entity, Group, Map, Side, Solid, VisGroup};

use super::{ENTITY_COLOR, clean_vec, solid_color};
use crate::document::{Document, is_reserved_key};
use crate::ids::{IdMap, IdSet, id_map, id_set};
use crate::scene::Owner;
use crate::selection::{FaceRef, ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

const HINT_MATERIAL: &str = "TOOLS/TOOLSHINT";
const SKIP_MATERIAL: &str = "TOOLS/TOOLSSKIP";

#[derive(Clone, Debug, Default)]
pub(crate) struct Fragment {
    pub solids: Vec<Solid>,
    pub entities: Vec<Entity>,
    pub groups: Vec<Group>,
    pub visgroups: Vec<VisGroup>,
}

impl Fragment {
    pub fn is_empty(&self) -> bool {
        self.solids.is_empty() && self.entities.is_empty()
    }
}

fn clean_keyvalues(keyvalues: &[(String, String)], skip_origin: bool) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::with_capacity(keyvalues.len() + 1);
    for (k, v) in keyvalues {
        if k.trim().is_empty() || is_reserved_key(k) || (skip_origin && k.eq_ignore_ascii_case("origin")) {
            continue;
        }
        match out.iter_mut().find(|(ok, _)| ok.eq_ignore_ascii_case(k)) {
            Some(slot) => slot.1.clone_from(v),
            None => out.push((k.clone(), v.clone())),
        }
    }
    out
}

fn merge_visgroups(tree: &mut Vec<VisGroup>, source: &[VisGroup], alloc: &mut dyn FnMut() -> Id) -> IdMap<Id> {
    fn walk(level: &mut Vec<VisGroup>, source: &[VisGroup], alloc: &mut dyn FnMut() -> Id, map: &mut IdMap<Id>) {
        for src in source {
            let pos = match level.iter().position(|g| g.name.eq_ignore_ascii_case(&src.name)) {
                Some(p) => p,
                None => {
                    level.push(VisGroup {
                        name: src.name.clone(),
                        id: alloc(),
                        color: src.color,
                        children: Vec::new(),
                        extra_props: Vec::new(),
                    });
                    level.len() - 1
                }
            };
            if let Some(target) = level.get_mut(pos) {
                map.insert(src.id, target.id);
                walk(&mut target.children, &src.children, alloc, map);
            }
        }
    }
    let mut map = id_map(8);
    walk(tree, source, alloc, &mut map);
    map
}

impl Document {
    pub fn import_map(&mut self, map: Map) -> EditResult<Vec<ObjRef>> {
        let frag = Fragment {
            solids: map.world.solids,
            entities: map.entities,
            groups: map.world.groups,
            visgroups: map.visgroups,
        };
        if frag.is_empty() {
            return Err(EditError::Nothing("the map has no objects to import"));
        }
        self.tx("Import map", |doc| {
            let objs = doc.instantiate(frag, true)?;
            let visible: Vec<ObjRef> = objs.iter().copied().filter(|o| doc.is_visible(*o)).collect();
            doc.select(&visible, SelectOp::Replace);
            Ok(objs)
        })
    }

    pub fn import_file(&mut self, path: &Path) -> EditResult<Vec<ObjRef>> {
        let map = hp_vmf::load(path)?;
        self.import_map(map)
    }

    fn check_owner(&self, owner: Owner) -> EditResult<()> {
        match owner {
            Owner::World => Ok(()),
            Owner::Entity(e) if self.scene.entities.contains_key(&e) => Ok(()),
            Owner::Entity(e) => Err(EditError::NotFound(ObjRef::Entity(e))),
        }
    }

    fn new_solid(&mut self, sides: Vec<Side>) -> Solid {
        let mut solid = Solid { sides, ..Solid::default() };
        self.assign_fresh_ids(&mut solid);
        let color =
            self.options.new_brush_color.map_or_else(|| solid_color(solid.id), crate::display_color::custom_color);
        solid.editor = EditorInfo { color, ..EditorInfo::default() };
        solid
    }

    pub fn create_solids(&mut self, solids: Vec<Vec<Side>>, owner: Owner) -> EditResult<Vec<Id>> {
        self.check_owner(owner)?;
        if solids.is_empty() {
            return Err(EditError::Nothing("no solid to create"));
        }
        for sides in &solids {
            validate_solid(sides)?;
        }
        self.tx("Create solid", |doc| {
            let new: Vec<(Solid, Owner)> = solids.into_iter().map(|s| (doc.new_solid(s), owner)).collect();
            let ids = doc.add_solids(new);
            let refs: Vec<ObjRef> = match owner {
                Owner::World => ids.iter().map(|id| ObjRef::Solid(*id)).collect(),
                Owner::Entity(e) => vec![ObjRef::Entity(e)],
            };
            doc.select(&refs, SelectOp::Replace);
            Ok(ids)
        })
    }

    pub fn create_solid(&mut self, sides: Vec<Side>, owner: Owner) -> EditResult<Id> {
        let ids = self.create_solids(vec![sides], owner)?;
        ids.first().copied().ok_or(EditError::Nothing("no solid created"))
    }

    pub fn create_primitive(&mut self, params: &PrimitiveParams, owner: Owner) -> EditResult<Vec<Id>> {
        self.check_owner(owner)?;
        let mut next = self.next_side_id;
        let pieces = make_primitive(params, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })?;
        self.tx("Create primitive", |doc| {
            let ids = doc.create_solids(pieces, owner)?;
            if ids.len() > 1 && owner == Owner::World {
                let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
                doc.group(&objs)?;
            }
            Ok(ids)
        })
    }

    pub fn create_point_entity(
        &mut self,
        classname: &str,
        origin: DVec3,
        keyvalues: &[(String, String)],
    ) -> EditResult<Id> {
        let classname = classname.trim();
        if classname.is_empty() {
            return Err(EditError::Invalid("empty class name".to_owned()));
        }
        if !origin.is_finite() {
            return Err(EditError::Invalid("origin is not finite".to_owned()));
        }
        let mut props = clean_keyvalues(keyvalues, true);
        props.push(("origin".to_owned(), hp_vmf::fmt_vec3(clean_vec(origin))));
        self.tx("Create entity", |doc| {
            let id = doc.alloc_object_id();
            let entity = Entity {
                id,
                classname: classname.to_owned(),
                props,
                editor: EditorInfo { color: ENTITY_COLOR, ..EditorInfo::default() },
                ..Entity::default()
            };
            doc.add_entity(entity);
            doc.select(&[ObjRef::Entity(id)], SelectOp::Replace);
            Ok(id)
        })
    }

    pub fn create_brush_entity(
        &mut self,
        classname: &str,
        keyvalues: &[(String, String)],
        solids: Vec<Vec<Side>>,
    ) -> EditResult<Id> {
        let classname = classname.trim();
        if classname.is_empty() {
            return Err(EditError::Invalid("empty class name".to_owned()));
        }
        if solids.is_empty() {
            return Err(EditError::Nothing("a brush entity needs at least one solid"));
        }
        for sides in &solids {
            validate_solid(sides)?;
        }
        let props = clean_keyvalues(keyvalues, false);
        self.tx("Create entity", |doc| {
            let id = doc.alloc_object_id();
            let entity = Entity {
                id,
                classname: classname.to_owned(),
                props,
                editor: EditorInfo { color: ENTITY_COLOR, ..EditorInfo::default() },
                ..Entity::default()
            };
            doc.add_entity(entity);
            doc.create_solids(solids, Owner::Entity(id))?;
            doc.select(&[ObjRef::Entity(id)], SelectOp::Replace);
            Ok(id)
        })
    }

    pub fn delete(&mut self, objs: &[ObjRef]) -> EditResult<usize> {
        let mut solids = id_set(objs.len());
        let mut entities = id_set(0);
        for o in objs {
            match *o {
                ObjRef::Solid(id) => {
                    if self.scene.solids.contains_key(&id) {
                        solids.insert(id);
                    }
                }
                ObjRef::Entity(id) => {
                    if self.scene.entities.contains_key(&id) {
                        entities.insert(id);
                        solids.extend(self.scene.entity_solid_ids(id));
                    }
                }
            }
        }
        let owners: IdSet = solids
            .iter()
            .filter_map(|id| match self.scene.solids.get(id).map(|n| n.owner) {
                Some(Owner::Entity(e)) => Some(e),
                _ => None,
            })
            .collect();
        for e in owners {
            if !entities.contains(&e) && self.scene.entity_solid_ids(e).iter().all(|s| solids.contains(s)) {
                entities.insert(e);
            }
        }
        if solids.is_empty() && entities.is_empty() {
            return Ok(0);
        }
        let count = solids.len() + entities.len();
        let groups = self.groups_of(&solids, &entities);
        let solids: Vec<Id> = solids.into_iter().collect();
        let entities: Vec<Id> = entities.into_iter().collect();
        self.tx("Delete", |doc| {
            doc.remove_solids(&solids);
            doc.remove_entities(&entities);
            doc.cleanup_empty_groups(groups);
            doc.deselect_missing();
            Ok(count)
        })
    }

    pub fn delete_selection(&mut self) -> EditResult<usize> {
        let objs = self.selected_objects();
        self.delete(&objs)
    }

    pub(crate) fn groups_of(&self, solids: &IdSet, entities: &IdSet) -> IdSet {
        let mut out = id_set(0);
        for id in solids {
            if let Some(g) = self.scene.solids.get(id).and_then(|n| n.solid.editor.group_id) {
                out.insert(g);
            }
        }
        for id in entities {
            if let Some(g) = self.scene.entities.get(id).and_then(|n| n.entity.editor.group_id) {
                out.insert(g);
            }
        }
        out
    }

    pub(crate) fn cleanup_empty_groups(&mut self, mut candidates: IdSet) {
        candidates.retain(|g| self.scene.groups.contains_key(g));
        while !candidates.is_empty() {
            let mut counts: IdMap<usize> = id_map(candidates.len());
            for g in &candidates {
                counts.insert(*g, 0);
            }
            let mut bump = |g: Option<Id>| {
                if let Some(g) = g
                    && let Some(c) = counts.get_mut(&g)
                {
                    *c += 1;
                }
            };
            for n in self.scene.solids.values() {
                if n.owner == Owner::World {
                    bump(n.solid.editor.group_id);
                }
            }
            for n in self.scene.entities.values() {
                bump(n.entity.editor.group_id);
            }
            for n in self.scene.groups.values() {
                if n.group.editor.group_id != Some(n.group.id) {
                    bump(n.group.editor.group_id);
                }
            }
            let empty: Vec<Id> = counts.iter().filter(|(_, c)| **c == 0).map(|(g, _)| *g).collect();
            if empty.is_empty() {
                break;
            }
            let parents: IdSet = empty.iter().filter_map(|g| self.scene.group_parent(*g)).collect();
            self.remove_groups(&empty);
            candidates = parents.into_iter().filter(|p| !empty.contains(p)).collect();
        }
    }

    pub(crate) fn deselect_missing(&mut self) {
        let scene = &self.scene;
        let stale_obj: Vec<ObjRef> = self.selection.objects.iter().copied().filter(|o| !scene.contains(*o)).collect();
        let stale_faces: Vec<_> = self
            .selection
            .faces
            .iter()
            .copied()
            .filter(|f| scene.solids.get(&f.solid).is_none_or(|n| f.side >= n.solid.sides.len()))
            .collect();
        if stale_obj.is_empty() && stale_faces.is_empty() {
            return;
        }
        let sel = self.sel_mut();
        crate::selection::remove_all(&mut sel.objects, &stale_obj);
        crate::selection::remove_all(&mut sel.faces, &stale_faces);
    }

    pub fn duplicate(&mut self, objs: &[ObjRef], offset: DVec3) -> EditResult<Vec<ObjRef>> {
        self.duplicate_with(objs, offset, None)
    }

    pub fn duplicate_with(
        &mut self,
        objs: &[ObjRef],
        offset: DVec3,
        is_target_key: Option<crate::problems::TargetKeyFn<'_>>,
    ) -> EditResult<Vec<ObjRef>> {
        if !offset.is_finite() {
            return Err(EditError::Invalid("offset is not finite".to_owned()));
        }
        let mut frag = self.extract_fragment(objs);
        if frag.is_empty() {
            return Err(EditError::Nothing("nothing to duplicate"));
        }
        self.auto_rename_copy(&mut frag, is_target_key);
        self.tx("Duplicate", |doc| {
            let new = doc.instantiate(frag, true)?;
            if offset != DVec3::ZERO {
                doc.translate(&new, offset)?;
            }
            doc.select(&new, SelectOp::Replace);
            Ok(new)
        })
    }

    pub(crate) fn extract_fragment(&self, objs: &[ObjRef]) -> Fragment {
        let mut frag = Fragment::default();
        let mut unit_set: IdSet = id_set(objs.len());
        let mut ent_set: IdSet = id_set(0);
        let mut seen_solids: IdSet = id_set(objs.len());
        for o in objs {
            match *o {
                ObjRef::Entity(id) => {
                    if self.scene.entities.contains_key(&id) {
                        ent_set.insert(id);
                    }
                }
                ObjRef::Solid(id) => {
                    if self.scene.solids.contains_key(&id) {
                        unit_set.insert(id);
                    }
                }
            }
        }
        let mut included_groups: IdSet = id_set(0);
        let mut checked: IdSet = id_set(0);
        let mut to_check: Vec<Id> = Vec::new();
        for id in &unit_set {
            if let Some(g) = self.scene.solids.get(id).and_then(|n| n.solid.editor.group_id) {
                to_check.push(g);
            }
        }
        for id in &ent_set {
            if let Some(g) = self.scene.entities.get(id).and_then(|n| n.entity.editor.group_id) {
                to_check.push(g);
            }
        }
        let index = if to_check.is_empty() { None } else { Some(self.scene.group_index()) };
        while let Some(g) = to_check.pop() {
            if !checked.insert(g) || !self.scene.groups.contains_key(&g) {
                continue;
            }
            let Some(index) = index.as_ref() else { break };
            let tree = index.subtree(g);
            let members = index.subtree_members(g);
            let all_in = !members.is_empty()
                && members.iter().all(|m| match *m {
                    ObjRef::Solid(id) => unit_set.contains(&id),
                    ObjRef::Entity(id) => ent_set.contains(&id),
                });
            if all_in {
                included_groups.extend(tree.iter().copied());
                if let Some(p) = self.scene.group_parent(g) {
                    to_check.push(p);
                }
            }
        }
        let fix_group = |e: &mut EditorInfo| {
            if e.group_id.is_some_and(|g| !included_groups.contains(&g)) {
                e.group_id = None;
            }
        };
        for n in self.scene.solids.values() {
            if unit_set.contains(&n.solid.id) && seen_solids.insert(n.solid.id) {
                let in_selected_entity = matches!(n.owner, Owner::Entity(e) if ent_set.contains(&e));
                if in_selected_entity {
                    continue;
                }
                let mut s = n.solid.clone();
                fix_group(&mut s.editor);
                frag.solids.push(s);
            }
        }
        for n in self.scene.entities.values() {
            if !ent_set.contains(&n.entity.id) {
                continue;
            }
            let mut e = n.entity.clone();
            fix_group(&mut e.editor);
            e.solids = self
                .scene
                .entity_solid_ids(n.entity.id)
                .into_iter()
                .filter_map(|sid| self.scene.solids.get(&sid).map(|s| s.solid.clone()))
                .collect();
            frag.entities.push(e);
        }
        for n in self.scene.groups.values() {
            if included_groups.contains(&n.group.id) {
                let mut g = n.group.clone();
                fix_group(&mut g.editor);
                frag.groups.push(g);
            }
        }
        frag
    }

    pub(crate) fn instantiate(&mut self, frag: Fragment, keep_hidden: bool) -> EditResult<Vec<ObjRef>> {
        let Fragment { solids, entities, groups, visgroups } = frag;
        let mut group_map: IdMap<Id> = id_map(groups.len());
        for g in &groups {
            if !group_map.contains_key(&g.id) {
                let new = self.alloc_object_id();
                group_map.insert(g.id, new);
            }
        }
        let merged: Option<(Vec<VisGroup>, IdMap<Id>)> = (!visgroups.is_empty()).then(|| {
            let mut tree = self.scene.visgroups.clone();
            let mut next = self.next_visgroup_id.max(crate::scene::max_visgroup_id(&tree).saturating_add(1));
            let map = merge_visgroups(&mut tree, &visgroups, &mut || {
                let id = next;
                next = next.saturating_add(1);
                id
            });
            self.next_visgroup_id = next;
            (tree, map)
        });
        let known_visgroups: IdSet = {
            let mut s = id_set(8);
            let roots = merged.as_ref().map_or(&self.scene.visgroups, |(t, _)| t);
            let mut stack: Vec<&VisGroup> = roots.iter().collect();
            while let Some(v) = stack.pop() {
                s.insert(v.id);
                stack.extend(v.children.iter());
            }
            s
        };
        let hidden_visgroups = crate::visibility::effective_hidden(
            merged.as_ref().map_or(&self.scene.visgroups, |(t, _)| t),
            &self.scene.hidden_visgroups,
        );
        let vis_map = merged.as_ref().map(|(_, m)| m);
        let fix_editor = |e: &mut EditorInfo| {
            e.group_id = e.group_id.and_then(|g| group_map.get(&g).copied());
            if let Some(map) = vis_map {
                let mut ids: Vec<Id> = Vec::with_capacity(e.visgroup_ids.len());
                for v in e.visgroup_ids.iter().filter_map(|v| map.get(v)) {
                    if !ids.contains(v) {
                        ids.push(*v);
                    }
                }
                e.visgroup_ids = ids;
                e.visgroup_shown = !e.visgroup_ids.iter().any(|v| hidden_visgroups.contains(v));
            } else {
                e.visgroup_ids.retain(|v| known_visgroups.contains(v));
            }
            if e.visgroup_ids.is_empty() {
                e.visgroup_shown = true;
            }
        };
        let mut new_groups = Vec::with_capacity(groups.len());
        for mut g in groups {
            let Some(&new) = group_map.get(&g.id) else { continue };
            g.id = new;
            fix_editor(&mut g.editor);
            if g.editor.group_id == Some(new) {
                g.editor.group_id = None;
            }
            new_groups.push(g);
        }
        let mut out = Vec::with_capacity(solids.len() + entities.len());
        let mut new_solids: Vec<(Solid, Owner)> = Vec::new();
        let mut new_entities = Vec::with_capacity(entities.len());
        for mut s in solids {
            self.assign_fresh_ids(&mut s);
            fix_editor(&mut s.editor);
            if !keep_hidden {
                s.hidden = false;
            }
            out.push(ObjRef::Solid(s.id));
            new_solids.push((s, Owner::World));
        }
        for mut e in entities {
            e.id = self.alloc_object_id();
            fix_editor(&mut e.editor);
            if !keep_hidden {
                e.hidden = false;
            }
            for mut s in std::mem::take(&mut e.solids) {
                self.assign_fresh_ids(&mut s);
                fix_editor(&mut s.editor);
                if !keep_hidden {
                    s.hidden = false;
                }
                new_solids.push((s, Owner::Entity(e.id)));
            }
            out.push(ObjRef::Entity(e.id));
            new_entities.push(e);
        }
        let new_tree = merged.map(|(t, _)| t).filter(|t| *t != self.scene.visgroups);
        self.tx("Paste", |doc| {
            if let Some(tree) = new_tree {
                *doc.visgroups_mut().0 = tree;
            }
            for g in new_groups {
                doc.put_group(g);
            }
            for e in new_entities {
                doc.add_entity(e);
            }
            doc.add_solids(new_solids);
            Ok(out)
        })
    }

    pub(crate) fn check_affine(m: &DMat4) -> EditResult<()> {
        let ok = m.is_finite()
            && m.row(3).abs_diff_eq(glam::DVec4::new(0.0, 0.0, 0.0, 1.0), 1e-9)
            && glam::DMat3::from_mat4(*m).determinant().abs() > 1e-12;
        if ok { Ok(()) } else { Err(hp_geom::GeomError::BadTransform.into()) }
    }

    pub fn tie_to_entity(
        &mut self,
        objs: &[ObjRef],
        classname: &str,
        keyvalues: &[(String, String)],
    ) -> EditResult<Id> {
        let classname = classname.trim();
        if classname.is_empty() {
            return Err(EditError::Invalid("empty class name".to_owned()));
        }
        let solids = self.scene.expand_solids(objs);
        if solids.is_empty() {
            return Err(EditError::Nothing("no solid to tie to an entity"));
        }
        let props = clean_keyvalues(keyvalues, false);
        self.tx("Tie to entity", |doc| {
            let id = doc.alloc_object_id();
            doc.add_entity(Entity {
                id,
                classname: classname.to_owned(),
                props,
                editor: EditorInfo { color: ENTITY_COLOR, ..EditorInfo::default() },
                ..Entity::default()
            });
            doc.tie_impl(&solids, id);
            doc.select(&[ObjRef::Entity(id)], SelectOp::Replace);
            Ok(id)
        })
    }

    pub fn tie_each_to_entity(
        &mut self,
        objs: &[ObjRef],
        classname: &str,
        keyvalues: &[(String, String)],
    ) -> EditResult<Vec<Id>> {
        let classname = classname.trim();
        if classname.is_empty() {
            return Err(EditError::Invalid("empty class name".to_owned()));
        }
        let solids: Vec<Id> = self
            .scene
            .expand_solids(objs)
            .into_iter()
            .filter(|id| self.scene.solids.get(id).is_some_and(|n| n.owner == Owner::World))
            .collect();
        if solids.is_empty() {
            return Err(EditError::Nothing("no world solid to tie to an entity"));
        }
        let props = clean_keyvalues(keyvalues, false);
        self.tx("Tie each to entity", |doc| {
            let mut ids = Vec::with_capacity(solids.len());
            for sid in &solids {
                let id = doc.alloc_object_id();
                doc.add_entity(Entity {
                    id,
                    classname: classname.to_owned(),
                    props: props.clone(),
                    editor: EditorInfo { color: ENTITY_COLOR, ..EditorInfo::default() },
                    ..Entity::default()
                });
                doc.tie_impl(&[*sid], id);
                ids.push(id);
            }
            let refs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Entity(*id)).collect();
            doc.select(&refs, SelectOp::Replace);
            Ok(ids)
        })
    }

    pub fn tie_target(&self, objs: &[ObjRef]) -> Option<Id> {
        let mut target = None;
        for o in objs {
            let e = match *o {
                ObjRef::Entity(e) => e,
                ObjRef::Solid(s) => match self.scene.solids.get(&s).map(|n| n.owner) {
                    Some(Owner::Entity(e)) => e,
                    _ => continue,
                },
            };
            if self.scene.entity_solid_count(e) == 0 {
                continue;
            }
            match target {
                None => target = Some(e),
                Some(t) if t == e => {}
                Some(_) => return None,
            }
        }
        let target = target?;
        let others = self
            .scene
            .expand_solids(objs)
            .into_iter()
            .any(|s| self.scene.solids.get(&s).is_some_and(|n| n.owner != Owner::Entity(target)));
        others.then_some(target)
    }

    pub fn tie_to_existing(&mut self, objs: &[ObjRef], entity: Id) -> EditResult<usize> {
        if self.scene.entity_solid_count(entity) == 0 {
            return Err(EditError::Invalid("the target is not a brush entity".to_owned()));
        }
        let solids: Vec<Id> = self
            .scene
            .expand_solids(objs)
            .into_iter()
            .filter(|s| self.scene.solids.get(s).is_some_and(|n| n.owner != Owner::Entity(entity)))
            .collect();
        if solids.is_empty() {
            return Err(EditError::Nothing("no solid to add to the entity"));
        }
        self.tx("Tie to entity", |doc| {
            doc.tie_impl(&solids, entity);
            doc.select(&[ObjRef::Entity(entity)], SelectOp::Replace);
            Ok(solids.len())
        })
    }

    fn tie_impl(&mut self, solids: &[Id], entity: Id) {
        let moving: IdSet = solids.iter().copied().collect();
        let mut emptied = id_set(0);
        for id in solids {
            if let Some(Owner::Entity(e)) = self.scene.solids.get(id).map(|n| n.owner)
                && e != entity
                && self.scene.entity_solid_ids(e).iter().all(|s| moving.contains(s))
            {
                emptied.insert(e);
            }
        }
        let groups = self.groups_of(&moving, &emptied);
        let updates: Vec<(Id, Solid)> = solids
            .iter()
            .filter_map(|sid| {
                let n = self.scene.solids.get(sid)?;
                n.solid.editor.group_id.map(|_| {
                    let mut s = n.solid.clone();
                    s.editor.group_id = None;
                    (*sid, s)
                })
            })
            .collect();
        self.update_solids(updates);
        self.set_solid_owners(solids, Owner::Entity(entity));
        let emptied: Vec<Id> = emptied.into_iter().collect();
        self.remove_entities(&emptied);
        self.cleanup_empty_groups(groups);
    }

    pub fn make_hint_brushes(&mut self, faces: &[FaceRef]) -> EditResult<usize> {
        let mut per_solid: IdMap<Vec<usize>> = id_map(faces.len());
        for f in faces {
            let n = self.scene.solids.get(&f.solid).ok_or(EditError::FaceNotFound(f.solid, f.side))?;
            if f.side >= n.solid.sides.len() {
                return Err(EditError::FaceNotFound(f.solid, f.side));
            }
            per_solid.entry(f.solid).or_default().push(f.side);
        }
        if per_solid.is_empty() {
            return Err(EditError::Nothing("select the faces that become hint faces"));
        }
        let mut updates: Vec<(Id, Solid)> = Vec::with_capacity(per_solid.len());
        for (id, hint) in &per_solid {
            let Some(n) = self.scene.solids.get(id) else { continue };
            let mut s = n.solid.clone();
            for (i, side) in s.sides.iter_mut().enumerate() {
                let material = if hint.contains(&i) { HINT_MATERIAL } else { SKIP_MATERIAL };
                if !side.material.eq_ignore_ascii_case(material) {
                    material.clone_into(&mut side.material);
                }
            }
            if s != n.solid {
                updates.push((*id, s));
            }
        }
        if updates.is_empty() {
            return Ok(0);
        }
        let count = updates.len();
        self.tx("Make hint brush", |doc| {
            doc.update_solids(updates);
            Ok(count)
        })
    }

    pub fn move_to_world(&mut self, objs: &[ObjRef]) -> EditResult<usize> {
        let solids: Vec<Id> = self
            .scene
            .expand_solids(objs)
            .into_iter()
            .filter(|id| matches!(self.scene.solids.get(id).map(|n| n.owner), Some(Owner::Entity(_))))
            .collect();
        if solids.is_empty() {
            return Ok(0);
        }
        let moving: IdSet = solids.iter().copied().collect();
        let mut emptied = id_set(0);
        for id in &solids {
            if let Some(Owner::Entity(e)) = self.scene.solids.get(id).map(|n| n.owner)
                && self.scene.entity_solid_ids(e).iter().all(|s| moving.contains(s))
            {
                emptied.insert(e);
            }
        }
        let groups = self.groups_of(&id_set(0), &emptied);
        self.tx("Move to world", |doc| {
            doc.set_solid_owners(&solids, Owner::World);
            let emptied: Vec<Id> = emptied.into_iter().collect();
            doc.remove_entities(&emptied);
            doc.cleanup_empty_groups(groups);
            let refs: Vec<ObjRef> = solids.iter().map(|id| ObjRef::Solid(*id)).collect();
            doc.select(&refs, SelectOp::Replace);
            Ok(solids.len())
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use glam::DVec3;

    use crate::tests::{check_undo_redo, cube};
    use crate::{Document, FaceRef, ObjRef, Owner};

    fn world_cubes(n: usize) -> (Document, Vec<u32>) {
        let mut doc = Document::new_empty();
        let ids = (0..n)
            .map(|i| doc.create_solid(cube(DVec3::new(i as f64 * 128.0, 0.0, 0.0), 64.0), Owner::World).expect("cube"))
            .collect();
        doc.clear_history();
        doc.clear_selection();
        (doc, ids)
    }

    #[test]
    fn tie_each_solid_to_its_own_entity() {
        let (mut doc, ids) = world_cubes(3);
        let kv = vec![("rendercolor".to_owned(), "255 0 0".to_owned())];
        let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
        let ents = check_undo_redo(&mut doc, |d| d.tie_each_to_entity(&objs, "func_detail", &kv).expect("tie"));
        assert_eq!(ents.len(), 3);
        for (e, s) in ents.iter().zip(&ids) {
            assert_eq!(doc.scene().entity_solid_ids(*e), vec![*s]);
            let ent = &doc.scene().entity(*e).expect("entity").entity;
            assert_eq!(ent.classname, "func_detail");
            assert_eq!(ent.get("rendercolor"), Some("255 0 0"));
        }
        assert_eq!(doc.selected_entity_ids().len(), 3);
        assert_eq!(doc.history_len().0, 1, "one undo step");
        let objs: Vec<ObjRef> = ents.iter().map(|e| ObjRef::Entity(*e)).collect();
        assert!(doc.tie_each_to_entity(&objs, "func_detail", &[]).is_err());
        assert!(doc.tie_each_to_entity(&[], "func_detail", &[]).is_err());
    }

    #[test]
    fn tie_into_an_existing_brush_entity() {
        let (mut doc, ids) = world_cubes(4);
        let door = doc.tie_to_entity(&[ObjRef::Solid(ids[0])], "func_door", &[]).expect("door");
        doc.update_entity(door, |e| e.set("speed", "200"));
        let other = doc.tie_to_entity(&[ObjRef::Solid(ids[1])], "func_wall", &[]).expect("wall");
        doc.clear_history();
        let objs = [ObjRef::Entity(door), ObjRef::Solid(ids[2]), ObjRef::Solid(ids[3])];
        assert_eq!(doc.tie_target(&objs), Some(door));
        assert_eq!(doc.tie_target(&[ObjRef::Entity(door)]), None);
        assert_eq!(doc.tie_target(&[ObjRef::Entity(door), ObjRef::Entity(other)]), None);
        assert_eq!(doc.tie_target(&[ObjRef::Solid(ids[2])]), None);
        assert_eq!(doc.tie_target(&[ObjRef::Solid(ids[0]), ObjRef::Solid(ids[3])]), Some(door));
        let n = check_undo_redo(&mut doc, |d| d.tie_to_existing(&objs, door).expect("tie"));
        assert_eq!(n, 2);
        let mut solids = doc.scene().entity_solid_ids(door);
        solids.sort_unstable();
        assert_eq!(solids, vec![ids[0], ids[2], ids[3]]);
        let ent = &doc.scene().entity(door).expect("door").entity;
        assert_eq!((ent.classname.as_str(), ent.get("speed")), ("func_door", Some("200")), "class and keys kept");
        doc.tie_to_existing(&[ObjRef::Entity(other), ObjRef::Entity(door)], door).expect("tie");
        assert!(doc.scene().entity(other).is_none());
        assert_eq!(doc.scene().entity_solid_count(door), 4);
        assert!(doc.tie_to_existing(&[ObjRef::Entity(door)], door).is_err(), "nothing to add");
    }

    #[test]
    fn hint_brushes() {
        let (mut doc, ids) = world_cubes(2);
        let faces = [FaceRef { solid: ids[0], side: 0 }, FaceRef { solid: ids[1], side: 2 }];
        let n = check_undo_redo(&mut doc, |d| d.make_hint_brushes(&faces).expect("hint"));
        assert_eq!(n, 2);
        for f in faces {
            let sides = &doc.scene().solid(f.solid).expect("solid").solid.sides;
            for (i, s) in sides.iter().enumerate() {
                let expected = if i == f.side { "TOOLS/TOOLSHINT" } else { "TOOLS/TOOLSSKIP" };
                assert_eq!(s.material, expected);
            }
        }
        assert_eq!(doc.make_hint_brushes(&faces).expect("again"), 0, "already hint brushes");
        assert!(doc.make_hint_brushes(&[]).is_err());
        assert!(doc.make_hint_brushes(&[FaceRef { solid: ids[0], side: 99 }]).is_err());
    }
}
