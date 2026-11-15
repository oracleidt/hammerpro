use glam::DVec3;
use hp_geom::Aabb;
use hp_vmf::{Cordon, CordonStyle, Cordons, EditorInfo, Solid, VisGroup};

use crate::document::Document;
use crate::ids::{IdMap, IdSet, id_map, id_set};
use crate::scene::{Owner, Scene};
use crate::selection::ObjRef;
use crate::{EditError, EditResult, Id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VisgroupState {
    Shown,
    Hidden,
    Partial,
}

#[inline]
fn editor_visible(e: &EditorInfo) -> bool {
    e.visgroup_shown && e.visgroup_auto_shown
}

pub(crate) fn find_visgroup(groups: &[VisGroup], id: Id) -> Option<&VisGroup> {
    let mut stack: Vec<&VisGroup> = groups.iter().collect();
    while let Some(g) = stack.pop() {
        if g.id == id {
            return Some(g);
        }
        stack.extend(g.children.iter());
    }
    None
}

pub(crate) fn find_visgroup_mut(groups: &mut [VisGroup], id: Id) -> Option<&mut VisGroup> {
    for g in groups.iter_mut() {
        if g.id == id {
            return Some(g);
        }
        if let Some(found) = find_visgroup_mut(&mut g.children, id) {
            return Some(found);
        }
    }
    None
}

fn detach_visgroup(groups: &mut Vec<VisGroup>, id: Id) -> Option<VisGroup> {
    if let Some(pos) = groups.iter().position(|g| g.id == id) {
        return Some(groups.remove(pos));
    }
    for g in groups.iter_mut() {
        if let Some(found) = detach_visgroup(&mut g.children, id) {
            return Some(found);
        }
    }
    None
}

fn delete_visgroup_node(groups: &mut Vec<VisGroup>, id: Id) -> bool {
    if let Some(pos) = groups.iter().position(|g| g.id == id) {
        let removed = groups.remove(pos);
        for (k, child) in removed.children.into_iter().enumerate() {
            groups.insert(pos + k, child);
        }
        return true;
    }
    groups.iter_mut().any(|g| delete_visgroup_node(&mut g.children, id))
}

pub(crate) fn subtree_ids(g: &VisGroup) -> Vec<Id> {
    let mut out = Vec::new();
    let mut stack = vec![g];
    while let Some(v) = stack.pop() {
        out.push(v.id);
        stack.extend(v.children.iter());
    }
    out
}

pub(crate) fn effective_hidden(groups: &[VisGroup], hidden: &IdSet) -> IdSet {
    let mut out = id_set(hidden.len());
    let mut stack: Vec<(&VisGroup, bool)> = groups.iter().map(|g| (g, false)).collect();
    while let Some((g, parent_hidden)) = stack.pop() {
        let h = parent_hidden || hidden.contains(&g.id);
        if h {
            out.insert(g.id);
        }
        stack.extend(g.children.iter().map(|c| (c, h)));
    }
    out
}

fn visgroup_color(id: Id) -> [u8; 3] {
    let h = (u64::from(id).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as u32;
    let hue = (h % 360) as f64;
    let (s, v) = (0.6, 0.95);
    let c = v * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |f: f64| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to(r), to(g), to(b)]
}

impl Document {
    pub fn is_visible(&self, obj: ObjRef) -> bool {
        match obj {
            ObjRef::Solid(id) => self.is_solid_visible(id),
            ObjRef::Entity(id) => self.is_entity_visible(id),
        }
    }

    pub fn is_solid_visible(&self, id: Id) -> bool {
        let Some(n) = self.scene.solids.get(&id) else {
            return false;
        };
        if n.solid.hidden || !editor_visible(&n.solid.editor) {
            return false;
        }
        match n.owner {
            Owner::Entity(e) => self.is_entity_visible(e),
            Owner::World => self.groups_visible(n.solid.editor.group_id),
        }
    }

    pub fn is_entity_visible(&self, id: Id) -> bool {
        let Some(n) = self.scene.entities.get(&id) else {
            return false;
        };
        !n.entity.hidden && editor_visible(&n.entity.editor) && self.groups_visible(n.entity.editor.group_id)
    }

    fn groups_visible(&self, mut group: Option<Id>) -> bool {
        for _ in 0..=self.scene.groups.len() {
            let Some(gid) = group else { return true };
            let Some(g) = self.scene.groups.get(&gid) else { return true };
            if !editor_visible(&g.group.editor) {
                return false;
            }
            group = g.group.editor.group_id.filter(|p| *p != gid);
        }
        true
    }

    pub fn set_hidden(&mut self, objs: &[ObjRef], hidden: bool) -> usize {
        let mut solids: Vec<(Id, Solid)> = Vec::new();
        let mut entities: Vec<Id> = Vec::new();
        for o in objs {
            match *o {
                ObjRef::Solid(id) => {
                    if let Some(n) = self.scene.solids.get(&id)
                        && n.solid.hidden != hidden
                    {
                        let mut s = n.solid.clone();
                        s.hidden = hidden;
                        solids.push((id, s));
                    }
                }
                ObjRef::Entity(id) => {
                    if self.scene.entities.get(&id).is_some_and(|n| n.entity.hidden != hidden) {
                        entities.push(id);
                    }
                }
            }
        }
        let count = solids.len() + entities.len();
        if count == 0 {
            return 0;
        }
        let label = if hidden { "Hide objects" } else { "Show objects" };
        let result: EditResult<()> = self.tx(label, |doc| {
            doc.update_solids(solids);
            for id in entities {
                doc.update_entity(id, |e| e.hidden = hidden);
            }
            if hidden {
                doc.deselect(objs);
            }
            doc.changes.visibility_changed = true;
            Ok(())
        });
        if result.is_err() {
            return 0;
        }
        count
    }

    pub fn hide_selection(&mut self) -> usize {
        let objs = self.selected_objects();
        self.set_hidden(&objs, true)
    }

    pub fn hide_unselected(&mut self) -> usize {
        let selected_solids: IdSet = self.selected_solid_ids().into_iter().collect();
        let objs: Vec<ObjRef> = self
            .scene
            .units()
            .filter(|o| self.is_visible(*o))
            .filter(|o| match *o {
                ObjRef::Solid(id) => !selected_solids.contains(&id),
                ObjRef::Entity(id) => {
                    !self.selection.objects.contains(o)
                        && !self.scene.entity_solid_ids(id).iter().any(|s| selected_solids.contains(s))
                }
            })
            .collect();
        self.set_hidden(&objs, true)
    }

    pub fn any_hidden(&self) -> bool {
        self.scene.solids.values().any(|n| n.solid.hidden) || self.scene.entities.values().any(|n| n.entity.hidden)
    }

    pub fn unhide_all(&mut self) -> usize {
        let mut objs: Vec<ObjRef> =
            self.scene.solids.values().filter(|n| n.solid.hidden).map(|n| ObjRef::Solid(n.solid.id)).collect();
        objs.extend(self.scene.entities.values().filter(|n| n.entity.hidden).map(|n| ObjRef::Entity(n.entity.id)));
        self.set_hidden(&objs, false)
    }

    pub fn visgroup(&self, id: Id) -> Option<&VisGroup> {
        find_visgroup(&self.scene.visgroups, id)
    }

    pub fn visgroup_parent(&self, id: Id) -> EditResult<Option<Id>> {
        let mut stack: Vec<(&VisGroup, Option<Id>)> = self.scene.visgroups.iter().map(|g| (g, None)).collect();
        while let Some((g, parent)) = stack.pop() {
            if g.id == id {
                return Ok(parent);
            }
            stack.extend(g.children.iter().map(|c| (c, Some(g.id))));
        }
        Err(EditError::VisgroupNotFound(id))
    }

    pub fn visgroup_members(&self, id: Id) -> Vec<ObjRef> {
        let mut out: Vec<ObjRef> = self
            .scene
            .solids
            .values()
            .filter(|n| n.solid.editor.visgroup_ids.contains(&id))
            .map(|n| ObjRef::Solid(n.solid.id))
            .collect();
        out.extend(
            self.scene
                .entities
                .values()
                .filter(|n| n.entity.editor.visgroup_ids.contains(&id))
                .map(|n| ObjRef::Entity(n.entity.id)),
        );
        out
    }

    pub fn visgroup_state(&self, id: Id) -> VisgroupState {
        let hidden = effective_hidden(&self.scene.visgroups, &self.scene.hidden_visgroups);
        if hidden.contains(&id) {
            return VisgroupState::Hidden;
        }
        match self.visgroup(id) {
            Some(g) if subtree_ids(g).iter().any(|c| hidden.contains(c)) => VisgroupState::Partial,
            _ => VisgroupState::Shown,
        }
    }

    pub fn is_visgroup_visible(&self, id: Id) -> bool {
        self.visgroup_state(id) != VisgroupState::Hidden
    }

    pub fn create_visgroup(&mut self, name: &str, parent: Option<Id>, members: &[ObjRef]) -> EditResult<Id> {
        if let Some(p) = parent
            && self.visgroup(p).is_none()
        {
            return Err(EditError::VisgroupNotFound(p));
        }
        let mut id = self.alloc_visgroup_id();
        while self.visgroup(id).is_some() {
            id = self.alloc_visgroup_id();
        }
        let group = VisGroup { name: name.to_owned(), id, color: visgroup_color(id), ..VisGroup::default() };
        self.tx("Create visgroup", |doc| {
            let (tree, _) = doc.visgroups_mut();
            match parent.and_then(|p| find_visgroup_mut(tree, p)) {
                Some(p) => p.children.push(group),
                None => tree.push(group),
            }
            doc.add_to_visgroup_unchecked(id, members);
            Ok(id)
        })
    }

    pub fn rename_visgroup(&mut self, id: Id, name: &str) -> EditResult<()> {
        let current = self.visgroup(id).ok_or(EditError::VisgroupNotFound(id))?;
        if current.name == name {
            return Ok(());
        }
        self.tx("Rename visgroup", |doc| {
            let (tree, _) = doc.visgroups_mut();
            if let Some(g) = find_visgroup_mut(tree, id) {
                name.clone_into(&mut g.name);
            }
            Ok(())
        })
    }

    pub fn set_visgroup_color(&mut self, id: Id, color: [u8; 3]) -> EditResult<()> {
        let current = self.visgroup(id).ok_or(EditError::VisgroupNotFound(id))?;
        if current.color == color {
            return Ok(());
        }
        self.tx("Change visgroup color", |doc| {
            let (tree, _) = doc.visgroups_mut();
            if let Some(g) = find_visgroup_mut(tree, id) {
                g.color = color;
            }
            Ok(())
        })
    }

    pub fn move_visgroup(&mut self, id: Id, new_parent: Option<Id>) -> EditResult<()> {
        let g = self.visgroup(id).ok_or(EditError::VisgroupNotFound(id))?;
        if let Some(p) = new_parent {
            if self.visgroup(p).is_none() {
                return Err(EditError::VisgroupNotFound(p));
            }
            if subtree_ids(g).contains(&p) {
                return Err(EditError::Invalid("a visgroup cannot be moved into itself".to_owned()));
            }
        }
        if self.visgroup_parent(id)? == new_parent {
            return Ok(());
        }
        self.tx("Move visgroup", |doc| {
            let (tree, _) = doc.visgroups_mut();
            if let Some(node) = detach_visgroup(tree, id) {
                match new_parent.and_then(|p| find_visgroup_mut(tree, p)) {
                    Some(p) => p.children.push(node),
                    None => tree.push(node),
                }
            }
            doc.refresh_visgroup_flags(None);
            Ok(())
        })
    }

    pub fn delete_visgroup(&mut self, id: Id) -> EditResult<()> {
        if self.visgroup(id).is_none() {
            return Err(EditError::VisgroupNotFound(id));
        }
        let members = self.visgroup_members(id);
        self.tx("Delete visgroup", |doc| {
            let (tree, hidden) = doc.visgroups_mut();
            delete_visgroup_node(tree, id);
            hidden.shift_remove(&id);
            doc.remove_from_visgroup_unchecked(id, &members);
            doc.refresh_visgroup_flags(None);
            Ok(())
        })
    }

    pub fn add_to_visgroup(&mut self, id: Id, objs: &[ObjRef]) -> EditResult<usize> {
        if self.visgroup(id).is_none() {
            return Err(EditError::VisgroupNotFound(id));
        }
        self.tx("Add to visgroup", |doc| Ok(doc.add_to_visgroup_unchecked(id, objs)))
    }

    pub fn remove_from_visgroup(&mut self, id: Id, objs: &[ObjRef]) -> EditResult<usize> {
        if self.visgroup(id).is_none() {
            return Err(EditError::VisgroupNotFound(id));
        }
        self.tx("Remove from visgroup", |doc| Ok(doc.remove_from_visgroup_unchecked(id, objs)))
    }

    pub(crate) fn add_to_visgroup_unchecked(&mut self, id: Id, objs: &[ObjRef]) -> usize {
        let hidden = effective_hidden(&self.scene.visgroups, &self.scene.hidden_visgroups);
        let mut count = 0;
        let mut solids = Vec::new();
        for o in objs {
            match *o {
                ObjRef::Solid(sid) => {
                    if let Some(n) = self.scene.solids.get(&sid)
                        && !n.solid.editor.visgroup_ids.contains(&id)
                    {
                        let mut s = n.solid.clone();
                        s.editor.visgroup_ids.push(id);
                        s.editor.visgroup_shown = !s.editor.visgroup_ids.iter().any(|v| hidden.contains(v));
                        solids.push((sid, s));
                        count += 1;
                    }
                }
                ObjRef::Entity(eid) => {
                    if self.scene.entities.get(&eid).is_some_and(|n| !n.entity.editor.visgroup_ids.contains(&id)) {
                        self.update_entity(eid, |e| {
                            e.editor.visgroup_ids.push(id);
                            e.editor.visgroup_shown = !e.editor.visgroup_ids.iter().any(|v| hidden.contains(v));
                        });
                        count += 1;
                    }
                }
            }
        }
        self.update_solids(solids);
        if count > 0 {
            self.changes.visgroups_changed = true;
            self.changes.visibility_changed = true;
        }
        count
    }

    fn remove_from_visgroup_unchecked(&mut self, id: Id, objs: &[ObjRef]) -> usize {
        let hidden = effective_hidden(&self.scene.visgroups, &self.scene.hidden_visgroups);
        let mut count = 0;
        let mut solids = Vec::new();
        for o in objs {
            match *o {
                ObjRef::Solid(sid) => {
                    if let Some(n) = self.scene.solids.get(&sid)
                        && n.solid.editor.visgroup_ids.contains(&id)
                    {
                        let mut s = n.solid.clone();
                        s.editor.visgroup_ids.retain(|v| *v != id);
                        s.editor.visgroup_shown = !s.editor.visgroup_ids.iter().any(|v| hidden.contains(v));
                        solids.push((sid, s));
                        count += 1;
                    }
                }
                ObjRef::Entity(eid) => {
                    if self.scene.entities.get(&eid).is_some_and(|n| n.entity.editor.visgroup_ids.contains(&id)) {
                        self.update_entity(eid, |e| {
                            e.editor.visgroup_ids.retain(|v| *v != id);
                            e.editor.visgroup_shown = !e.editor.visgroup_ids.iter().any(|v| hidden.contains(v));
                        });
                        count += 1;
                    }
                }
            }
        }
        self.update_solids(solids);
        if count > 0 {
            self.changes.visgroups_changed = true;
            self.changes.visibility_changed = true;
        }
        count
    }

    pub(crate) fn replace_visgroups_unchecked(&mut self, objs: &[ObjRef], ids: &[Id]) -> usize {
        let hidden = effective_hidden(&self.scene.visgroups, &self.scene.hidden_visgroups);
        let shown = !ids.iter().any(|v| hidden.contains(v));
        let mut count = 0;
        let mut solids = Vec::new();
        for o in objs {
            match *o {
                ObjRef::Solid(sid) => {
                    if let Some(n) = self.scene.solids.get(&sid)
                        && n.solid.editor.visgroup_ids != ids
                    {
                        let mut s = n.solid.clone();
                        ids.clone_into(&mut s.editor.visgroup_ids);
                        s.editor.visgroup_shown = shown;
                        solids.push((sid, s));
                        count += 1;
                    }
                }
                ObjRef::Entity(eid) => {
                    if self.scene.entities.get(&eid).is_some_and(|n| n.entity.editor.visgroup_ids != ids) {
                        self.update_entity(eid, |e| {
                            ids.clone_into(&mut e.editor.visgroup_ids);
                            e.editor.visgroup_shown = shown;
                        });
                        count += 1;
                    }
                }
            }
        }
        self.update_solids(solids);
        if count > 0 {
            self.changes.visgroups_changed = true;
            self.changes.visibility_changed = true;
        }
        count
    }

    pub fn set_visgroup_visible(&mut self, id: Id, visible: bool) -> EditResult<()> {
        let g = self.visgroup(id).ok_or(EditError::VisgroupNotFound(id))?;
        let ids = subtree_ids(g);
        let hidden = &self.scene.hidden_visgroups;
        let unchanged =
            if visible { ids.iter().all(|i| !hidden.contains(i)) } else { ids.iter().all(|i| hidden.contains(i)) };
        if unchanged {
            return Ok(());
        }
        let label = if visible { "Show visgroup" } else { "Hide visgroup" };
        self.tx(label, |doc| {
            let (_, hidden) = doc.visgroups_mut();
            for i in &ids {
                if visible {
                    hidden.shift_remove(i);
                } else {
                    hidden.insert(*i);
                }
            }
            let affected: IdSet = ids.iter().copied().collect();
            doc.refresh_visgroup_flags(Some(&affected));
            Ok(())
        })
    }

    pub fn toggle_visgroup(&mut self, id: Id) -> EditResult<bool> {
        let visible = !self.is_visgroup_visible(id);
        self.set_visgroup_visible(id, visible)?;
        Ok(visible)
    }

    pub fn show_all_visgroups(&mut self) {
        if self.scene.hidden_visgroups.is_empty() {
            return;
        }
        let _ = self.tx("Show all visgroups", |doc| {
            doc.visgroups_mut().1.clear();
            doc.refresh_visgroup_flags(None);
            Ok(())
        });
    }

    pub fn hide_all_visgroups(&mut self) {
        let mut ids: Vec<Id> = Vec::new();
        for g in self.scene.visgroups.iter() {
            ids.extend(subtree_ids(g));
        }
        if ids.is_empty() || ids.iter().all(|i| self.scene.hidden_visgroups.contains(i)) {
            return;
        }
        let _ = self.tx("Hide all visgroups", |doc| {
            let (_, hidden) = doc.visgroups_mut();
            for i in &ids {
                hidden.insert(*i);
            }
            doc.refresh_visgroup_flags(None);
            Ok(())
        });
    }

    pub(crate) fn refresh_visgroup_flags(&mut self, only: Option<&IdSet>) {
        let hidden = effective_hidden(&self.scene.visgroups, &self.scene.hidden_visgroups);
        let concerned = |e: &EditorInfo| match only {
            Some(set) => e.visgroup_ids.iter().any(|v| set.contains(v)),
            None => true,
        };
        let shown = |e: &EditorInfo| !e.visgroup_ids.iter().any(|v| hidden.contains(v));
        let solids: Vec<(Id, Solid)> = self
            .scene
            .solids
            .values()
            .filter(|n| concerned(&n.solid.editor) && n.solid.editor.visgroup_shown != shown(&n.solid.editor))
            .map(|n| {
                let mut s = n.solid.clone();
                s.editor.visgroup_shown = !s.editor.visgroup_shown;
                (n.solid.id, s)
            })
            .collect();
        let entities: Vec<Id> = self
            .scene
            .entities
            .values()
            .filter(|n| concerned(&n.entity.editor) && n.entity.editor.visgroup_shown != shown(&n.entity.editor))
            .map(|n| n.entity.id)
            .collect();
        let groups: Vec<hp_vmf::Group> = self
            .scene
            .groups
            .values()
            .filter(|n| concerned(&n.group.editor) && n.group.editor.visgroup_shown != shown(&n.group.editor))
            .map(|n| {
                let mut g = n.group.clone();
                g.editor.visgroup_shown = !g.editor.visgroup_shown;
                g
            })
            .collect();
        self.update_solids(solids);
        for id in entities {
            self.update_entity(id, |e| e.editor.visgroup_shown = !e.editor.visgroup_shown);
        }
        for g in groups {
            self.put_group(g);
        }
        self.changes.visibility_changed = true;
    }

    pub fn set_auto_visibility(&mut self, pred: &dyn Fn(&Scene, ObjRef) -> bool, visible: bool) -> usize {
        let scene = &self.scene;
        let solids: Vec<(Id, Solid)> = scene
            .solids
            .values()
            .filter(|n| n.solid.editor.visgroup_auto_shown != visible && pred(scene, ObjRef::Solid(n.solid.id)))
            .map(|n| {
                let mut s = n.solid.clone();
                s.editor.visgroup_auto_shown = visible;
                (n.solid.id, s)
            })
            .collect();
        let entities: Vec<Id> = scene
            .entities
            .values()
            .filter(|n| n.entity.editor.visgroup_auto_shown != visible && pred(scene, ObjRef::Entity(n.entity.id)))
            .map(|n| n.entity.id)
            .collect();
        let count = solids.len() + entities.len();
        if count == 0 {
            return 0;
        }
        let label = if visible { "Show auto visgroup" } else { "Hide auto visgroup" };
        let _ = self.tx(label, |doc| {
            doc.update_solids(solids);
            for id in entities {
                doc.update_entity(id, |e| e.editor.visgroup_auto_shown = visible);
            }
            doc.changes.visibility_changed = true;
            Ok(())
        });
        count
    }

    pub fn visgroup_member_counts(&self) -> IdMap<usize> {
        let mut counts: IdMap<usize> = id_map(8);
        for n in self.scene.solids.values() {
            for v in &n.solid.editor.visgroup_ids {
                *counts.entry(*v).or_insert(0) += 1;
            }
        }
        for n in self.scene.entities.values() {
            for v in &n.entity.editor.visgroup_ids {
                *counts.entry(*v).or_insert(0) += 1;
            }
        }
        counts
    }
}

const CORDON_EPS: f64 = 1e-3;
const MAX_CORDON_BOXES: usize = 256;
const MAX_CORDON_NAME: usize = 128;
const LEGACY_CORDON_NAME: &str = "cordon";

const UNDO_EDIT_CORDONS: &str = "Edit cordons";
const UNDO_TOGGLE_CORDONS: &str = "Toggle cordons";
const UNDO_NEW_CORDON: &str = "New cordon";
const UNDO_DELETE_CORDON: &str = "Delete cordon";
const UNDO_RENAME_CORDON: &str = "Rename cordon";
const UNDO_TOGGLE_CORDON: &str = "Toggle cordon";
const UNDO_CORDON_MODE: &str = "Cordon mode";
const UNDO_RESIZE_CORDON: &str = "Resize cordon";

fn cordon_aabb(mins: DVec3, maxs: DVec3) -> Option<Aabb> {
    if !(mins.is_finite() && maxs.is_finite()) {
        return None;
    }
    let b = Aabb::new(mins, maxs);
    (b.size().min_element() > CORDON_EPS).then_some(b)
}

fn active_boxes(c: &Cordons) -> impl Iterator<Item = Aabb> + '_ {
    boxes_of_mode(c, false)
}

fn exclude_boxes_of(c: &Cordons) -> impl Iterator<Item = Aabb> + '_ {
    boxes_of_mode(c, true)
}

fn boxes_of_mode(c: &Cordons, exclude: bool) -> impl Iterator<Item = Aabb> + '_ {
    let (cordons, extras) = if c.active { (&c.cordons[..], &c.extra_boxes[..]) } else { (&[][..], &[][..]) };
    let firsts = cordons
        .iter()
        .filter(move |cd| cd.active && cd.exclude == exclude)
        .filter_map(|cd| cordon_aabb(cd.mins, cd.maxs));
    let extras = extras
        .iter()
        .filter(move |b| cordons.get(b.cordon).is_some_and(|cd| cd.active && cd.exclude == exclude))
        .filter_map(|b| cordon_aabb(b.mins, b.maxs));
    firsts.chain(extras).take(MAX_CORDON_BOXES)
}

pub(crate) fn box_inside_any(boxes: &[Aabb], b: &Aabb) -> bool {
    boxes.iter().any(|c| c.contains_point(b.min) && c.contains_point(b.max))
}

fn clean_cordon_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control() && *c != '"').take(MAX_CORDON_NAME).collect();
    s.trim().to_owned()
}

pub(crate) fn normalize_cordons(c: &mut Cordons) -> Vec<Option<usize>> {
    let old = std::mem::take(&mut c.cordons);
    let extras = std::mem::take(&mut c.extra_boxes);
    let mut first_extra: Vec<Option<usize>> = vec![None; old.len()];
    for (k, b) in extras.iter().enumerate() {
        if let Some(slot) = first_extra.get_mut(b.cordon)
            && slot.is_none()
            && cordon_aabb(b.mins, b.maxs).is_some()
        {
            *slot = Some(k);
        }
    }
    let mut promoted = vec![false; extras.len()];
    let mut remap: Vec<Option<usize>> = Vec::with_capacity(old.len());
    let mut kept: Vec<Cordon> = Vec::with_capacity(old.len());
    for (i, cd) in old.into_iter().enumerate() {
        let mut first = cordon_aabb(cd.mins, cd.maxs);
        if first.is_none()
            && let Some(k) = first_extra.get(i).copied().flatten()
            && let Some(b) = extras.get(k)
        {
            first = cordon_aabb(b.mins, b.maxs);
            if let Some(p) = promoted.get_mut(k) {
                *p = true;
            }
        }
        let Some(b) = first else {
            remap.push(None);
            continue;
        };
        remap.push(Some(kept.len()));
        let mut name = clean_cordon_name(&cd.name);
        if name.is_empty() {
            LEGACY_CORDON_NAME.clone_into(&mut name);
        }
        kept.push(Cordon { name, active: cd.active, mins: b.min, maxs: b.max, exclude: cd.exclude });
    }
    c.extra_boxes = extras
        .into_iter()
        .enumerate()
        .filter(|(k, _)| !promoted.get(*k).copied().unwrap_or(false))
        .filter_map(|(_, b)| {
            let cordon = remap.get(b.cordon).copied().flatten()?;
            let aabb = cordon_aabb(b.mins, b.maxs)?;
            Some(hp_vmf::CordonBox { cordon, mins: aabb.min, maxs: aabb.max })
        })
        .collect();
    c.cordons = kept;
    let legacy_fits = c.cordons.len() == 1
        && c.extra_boxes.is_empty()
        && c.cordons.iter().all(|cd| cd.active && !cd.exclude && cd.name == LEGACY_CORDON_NAME);
    if !legacy_fits {
        c.style = CordonStyle::Modern;
    }
    remap
}

impl Document {
    pub fn cordon_boxes(&self) -> Vec<Aabb> {
        active_boxes(&self.scene.cordons).collect()
    }

    pub fn exclude_cordon_boxes(&self) -> Vec<Aabb> {
        exclude_boxes_of(&self.scene.cordons).collect()
    }

    pub fn cordon_box_count(&self) -> usize {
        active_boxes(&self.scene.cordons).count() + exclude_boxes_of(&self.scene.cordons).count()
    }

    pub fn cordons_in_use(&self) -> bool {
        active_boxes(&self.scene.cordons).next().is_some() || exclude_boxes_of(&self.scene.cordons).next().is_some()
    }

    pub fn cordon_key(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut any = false;
        let c = &self.scene.cordons;
        for (mode, b) in active_boxes(c).map(|b| (0u64, b)).chain(exclude_boxes_of(c).map(|b| (1u64, b))) {
            any = true;
            h = (h ^ mode).wrapping_mul(0x0100_0000_01b3);
            for v in [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z] {
                h = (h ^ v.to_bits()).wrapping_mul(0x0100_0000_01b3);
            }
        }
        if any { h | 1 } else { 0 }
    }

    pub fn is_box_cordoned_out(&self, b: &Aabb) -> bool {
        if b.is_empty() || !b.min.is_finite() || !b.max.is_finite() {
            return false;
        }
        let c = &self.scene.cordons;
        if exclude_boxes_of(c).any(|x| x.contains_point(b.min) && x.contains_point(b.max)) {
            return true;
        }
        let mut any = false;
        for c in active_boxes(c) {
            if c.intersects(b) {
                return false;
            }
            any = true;
        }
        any
    }

    pub fn is_point_cordoned_out(&self, p: DVec3) -> bool {
        if !p.is_finite() {
            return false;
        }
        let c = &self.scene.cordons;
        if exclude_boxes_of(c).any(|x| x.contains_point(p)) {
            return true;
        }
        let mut any = false;
        for c in active_boxes(c) {
            if c.contains_point(p) {
                return false;
            }
            any = true;
        }
        any
    }

    pub fn is_solid_cordoned_out(&self, id: Id) -> bool {
        self.scene.solids.get(&id).is_some_and(|n| self.is_box_cordoned_out(&n.bounds))
    }

    pub fn is_entity_cordoned_out(&self, id: Id) -> bool {
        let Some(n) = self.scene.entities.get(&id) else { return false };
        if !self.cordons_in_use() {
            return false;
        }
        match self.scene.entity_solids.get(&id).filter(|s| !s.is_empty()) {
            Some(solids) => solids.iter().all(|s| self.is_solid_cordoned_out(*s)),
            None => n.entity.origin().is_some_and(|p| self.is_point_cordoned_out(p)),
        }
    }

    pub fn is_cordoned_out(&self, obj: ObjRef) -> bool {
        match obj {
            ObjRef::Solid(id) => self.is_solid_cordoned_out(id),
            ObjRef::Entity(id) => self.is_entity_cordoned_out(id),
        }
    }

    pub fn is_pickable(&self, obj: ObjRef) -> bool {
        self.is_visible(obj) && !self.is_cordoned_out(obj)
    }

    pub fn is_solid_pickable(&self, id: Id) -> bool {
        self.is_solid_visible(id) && !self.is_solid_cordoned_out(id)
    }

    pub fn is_entity_pickable(&self, id: Id) -> bool {
        self.is_entity_visible(id) && !self.is_entity_cordoned_out(id)
    }

    pub fn refresh_visibility(&mut self) {
        self.changes.visibility_changed = true;
    }

    pub fn unique_cordon_name(&self, base: &str) -> String {
        let mut base = clean_cordon_name(base);
        if base.is_empty() {
            LEGACY_CORDON_NAME.clone_into(&mut base);
        }
        let used = |n: &str| self.scene.cordons.cordons.iter().any(|c| c.name.eq_ignore_ascii_case(n));
        if !used(&base) {
            return base;
        }
        (2..=self.scene.cordons.cordons.len() + 2).map(|k| format!("{base} {k}")).find(|n| !used(n)).unwrap_or(base)
    }

    fn check_cordon(&self, index: usize) -> EditResult<()> {
        if index < self.scene.cordons.cordons.len() {
            Ok(())
        } else {
            Err(EditError::Invalid(format!("cordon {index} does not exist")))
        }
    }

    fn commit_cordons(&mut self, label: &str, mut c: Cordons) -> EditResult<Vec<Option<usize>>> {
        let remap = normalize_cordons(&mut c);
        if self.scene.cordons == c {
            return Ok(remap);
        }
        self.tx(label, |doc| {
            *doc.misc_mut().1 = c;
            doc.changes.visibility_changed = true;
            Ok(remap)
        })
    }

    pub fn edit_cordons(&mut self, cordons: Cordons) -> EditResult<()> {
        self.commit_cordons(UNDO_EDIT_CORDONS, cordons).map(|_| ())
    }

    pub fn set_cordons_enabled(&mut self, on: bool) -> EditResult<bool> {
        if self.scene.cordons.active == on {
            return Ok(false);
        }
        let mut c = self.scene.cordons.clone();
        c.active = on;
        self.commit_cordons(UNDO_TOGGLE_CORDONS, c).map(|_| true)
    }

    pub fn add_cordon(&mut self, name: &str, b: Aabb) -> EditResult<usize> {
        let Some(b) = cordon_aabb(b.min, b.max) else {
            return Err(EditError::Invalid("a cordon box needs a volume".to_owned()));
        };
        let mut c = self.scene.cordons.clone();
        c.cordons.push(Cordon { name: name.to_owned(), active: true, mins: b.min, maxs: b.max, exclude: false });
        let remap = self.commit_cordons(UNDO_NEW_CORDON, c)?;
        remap.last().copied().flatten().ok_or_else(|| EditError::Invalid("cordon dropped".to_owned()))
    }

    pub fn remove_cordon(&mut self, index: usize) -> EditResult<()> {
        self.check_cordon(index)?;
        let mut c = self.scene.cordons.clone();
        c.cordons.remove(index);
        c.extra_boxes.retain(|b| b.cordon != index);
        for b in &mut c.extra_boxes {
            if b.cordon > index {
                b.cordon -= 1;
            }
        }
        self.commit_cordons(UNDO_DELETE_CORDON, c).map(|_| ())
    }

    pub fn rename_cordon(&mut self, index: usize, name: &str) -> EditResult<()> {
        self.check_cordon(index)?;
        let name = clean_cordon_name(name);
        if name.is_empty() {
            return Err(EditError::Invalid("a cordon needs a name".to_owned()));
        }
        let mut c = self.scene.cordons.clone();
        if let Some(cd) = c.cordons.get_mut(index) {
            cd.name = name;
        }
        self.commit_cordons(UNDO_RENAME_CORDON, c).map(|_| ())
    }

    pub fn set_cordon_active(&mut self, index: usize, active: bool) -> EditResult<()> {
        self.check_cordon(index)?;
        let mut c = self.scene.cordons.clone();
        if let Some(cd) = c.cordons.get_mut(index) {
            cd.active = active;
        }
        self.commit_cordons(UNDO_TOGGLE_CORDON, c).map(|_| ())
    }

    pub fn set_cordon_exclude(&mut self, index: usize, exclude: bool) -> EditResult<()> {
        self.check_cordon(index)?;
        let mut c = self.scene.cordons.clone();
        if let Some(cd) = c.cordons.get_mut(index) {
            cd.exclude = exclude;
        }
        self.commit_cordons(UNDO_CORDON_MODE, c).map(|_| ())
    }

    pub fn set_cordon_box(&mut self, index: usize, b: Aabb) -> EditResult<Option<usize>> {
        self.check_cordon(index)?;
        let mut c = self.scene.cordons.clone();
        if let Some(cd) = c.cordons.get_mut(index) {
            cd.mins = b.min;
            cd.maxs = b.max;
        }
        let remap = self.commit_cordons(UNDO_RESIZE_CORDON, c)?;
        Ok(remap.get(index).copied().flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_helpers() {
        let mut tree = vec![VisGroup {
            name: "a".into(),
            id: 1,
            children: vec![
                VisGroup { name: "b".into(), id: 2, ..VisGroup::default() },
                VisGroup {
                    name: "c".into(),
                    id: 3,
                    children: vec![VisGroup { name: "d".into(), id: 4, ..VisGroup::default() }],
                    ..VisGroup::default()
                },
            ],
            ..VisGroup::default()
        }];
        assert!(find_visgroup(&tree, 4).is_some());
        let mut hidden = id_set(0);
        hidden.insert(3);
        let eff = effective_hidden(&tree, &hidden);
        assert!(eff.contains(&3) && eff.contains(&4) && !eff.contains(&1));
        assert!(delete_visgroup_node(&mut tree, 3));
        assert!(find_visgroup(&tree, 3).is_none());
        assert_eq!(find_visgroup(&tree, 1).map(|g| g.children.len()), Some(2));
        let d = detach_visgroup(&mut tree, 4);
        assert_eq!(d.map(|g| g.id), Some(4));
        let c = visgroup_color(7);
        assert!(c.iter().any(|v| *v > 100));
    }

    use crate::Owner;
    use crate::tests::{check_undo_redo, cube, vmf};
    use hp_vmf::CordonBox;

    fn aabb(min: [f64; 3], max: [f64; 3]) -> Aabb {
        Aabb::new(DVec3::from_array(min), DVec3::from_array(max))
    }

    fn cordon_doc() -> (Document, Id, Id, Id, Id) {
        let mut doc = Document::new_empty();
        let inside = doc.create_solid(cube(DVec3::ZERO, 16.0), Owner::World).expect("inside");
        let outside = doc.create_solid(cube(DVec3::new(512.0, 0.0, 0.0), 16.0), Owner::World).expect("outside");
        let point = doc.create_point_entity("info_target", DVec3::new(1000.0, 0.0, 0.0), &[]).expect("point");
        let brush = doc
            .create_brush_entity(
                "func_wall",
                &[],
                vec![cube(DVec3::new(90.0, 0.0, 0.0), 16.0), cube(DVec3::new(600.0, 0.0, 0.0), 16.0)],
            )
            .expect("brush");
        doc.clear_selection();
        doc.clear_history();
        (doc, inside, outside, point, brush)
    }

    #[test]
    fn exclude_cordons_cut_what_lies_inside() {
        let (mut doc, inside, outside, point, brush) = cordon_doc();
        doc.set_cordon_box(0, aabb([-64.0, -64.0, -64.0], [64.0, 64.0, 64.0])).expect("box");
        doc.set_cordon_exclude(0, true).expect("exclude");
        doc.set_cordons_enabled(true).expect("on");
        assert!(doc.cordons_in_use() && doc.cordon_boxes().is_empty());
        assert_eq!(doc.exclude_cordon_boxes().len(), 1);
        assert!(doc.is_cordoned_out(ObjRef::Solid(inside)));
        assert!(!doc.is_cordoned_out(ObjRef::Solid(outside)));
        assert!(!doc.is_cordoned_out(ObjRef::Entity(point)));
        assert!(!doc.is_cordoned_out(ObjRef::Entity(brush)), "one of its brushes is outside");
        assert!(doc.is_point_cordoned_out(DVec3::ZERO));
        doc.add_cordon("keep", aabb([400.0, -64.0, -64.0], [700.0, 64.0, 64.0])).expect("include");
        assert!(doc.is_cordoned_out(ObjRef::Solid(inside)));
        assert!(!doc.is_cordoned_out(ObjRef::Solid(outside)));
        assert!(doc.is_cordoned_out(ObjRef::Entity(point)), "outside the include cordon");
        let key = doc.cordon_key();
        doc.set_cordon_exclude(0, false).expect("include");
        assert_ne!(doc.cordon_key(), key);
        assert!(!doc.is_cordoned_out(ObjRef::Solid(inside)));
        assert!(doc.undo());
        assert!(doc.scene().cordons().cordons[0].exclude);
    }

    #[test]
    fn new_maps_do_not_use_cordons() {
        let (doc, inside, outside, point, _) = cordon_doc();
        assert_eq!(doc.scene().cordons().cordons.len(), 1);
        assert!(!doc.cordons_in_use());
        assert_eq!(doc.cordon_key(), 0);
        assert!(doc.cordon_boxes().is_empty());
        for o in [ObjRef::Solid(inside), ObjRef::Solid(outside), ObjRef::Entity(point)] {
            assert!(!doc.is_cordoned_out(o));
            assert!(doc.is_pickable(o));
        }
    }

    #[test]
    fn objects_outside_every_box_are_cordoned_out() {
        let (mut doc, inside, outside, point, brush) = cordon_doc();
        doc.set_cordon_box(0, aabb([-64.0, -64.0, -64.0], [128.0, 64.0, 64.0])).expect("box");
        assert!(!doc.cordons_in_use(), "drawing a box does not switch cordons on");
        assert!(check_undo_redo(&mut doc, |d| d.set_cordons_enabled(true)).expect("on"));
        assert!(doc.cordons_in_use());
        assert_ne!(doc.cordon_key(), 0);
        assert!(!doc.is_cordoned_out(ObjRef::Solid(inside)));
        assert!(doc.is_cordoned_out(ObjRef::Solid(outside)));
        assert!(doc.is_cordoned_out(ObjRef::Entity(point)));
        assert!(!doc.is_pickable(ObjRef::Entity(point)) && !doc.is_entity_pickable(point));
        assert!(!doc.is_solid_pickable(outside) && doc.is_solid_pickable(inside));
        assert!(!doc.is_cordoned_out(ObjRef::Entity(brush)));
        let solids = doc.scene().entity_solid_ids(brush);
        assert!(!doc.is_solid_cordoned_out(solids[0]) && doc.is_solid_cordoned_out(solids[1]));
        assert!(!doc.is_box_cordoned_out(&aabb([128.0, 0.0, 0.0], [160.0, 16.0, 16.0])));
        assert!(doc.is_box_cordoned_out(&aabb([128.5, 0.0, 0.0], [160.0, 16.0, 16.0])));
        assert!(!doc.is_point_cordoned_out(DVec3::new(128.0, 64.0, -64.0)));
        assert!(doc.is_point_cordoned_out(DVec3::new(129.0, 0.0, 0.0)));
        assert!(!doc.is_point_cordoned_out(DVec3::NAN));
        assert!(!doc.is_box_cordoned_out(&Aabb::EMPTY));
        let i = doc.add_cordon("east", aabb([500.0, -8.0, -8.0], [540.0, 24.0, 24.0])).expect("add");
        assert_eq!(i, 1);
        assert!(!doc.is_cordoned_out(ObjRef::Solid(outside)));
        let key = doc.cordon_key();
        doc.set_cordon_active(1, false).expect("inactive");
        assert!(doc.is_cordoned_out(ObjRef::Solid(outside)));
        assert_ne!(doc.cordon_key(), key);
        doc.set_cordons_enabled(false).expect("off");
        assert!(!doc.is_cordoned_out(ObjRef::Solid(outside)) && !doc.is_cordoned_out(ObjRef::Entity(point)));
    }

    #[test]
    fn cordon_edits_are_undoable_and_flag_visibility() {
        let (mut doc, ..) = cordon_doc();
        let _ = doc.take_changes();
        let i = check_undo_redo(&mut doc, |d| d.add_cordon("Room \"A\"\n", aabb([0.0; 3], [64.0; 3]))).expect("add");
        assert_eq!(doc.scene().cordons().cordons[i].name, "Room A", "quotes and control characters are removed");
        assert!(doc.take_changes().visibility_changed);
        assert_eq!(doc.undo_label(), Some("New cordon"));
        check_undo_redo(&mut doc, |d| d.rename_cordon(i, "Hall")).expect("rename");
        assert!(doc.rename_cordon(i, "  ").is_err());
        assert!(doc.rename_cordon(9, "x").is_err());
        check_undo_redo(&mut doc, |d| d.set_cordon_active(i, false)).expect("active");
        let moved = check_undo_redo(&mut doc, |d| d.set_cordon_box(i, aabb([8.0; 3], [72.0; 3]))).expect("box");
        assert_eq!(moved, Some(i));
        assert_eq!(doc.scene().cordons().cordons[i].mins, DVec3::splat(8.0));
        check_undo_redo(&mut doc, |d| d.remove_cordon(0)).expect("remove");
        assert_eq!(doc.scene().cordons().cordons.len(), 1);
        assert_eq!(doc.scene().cordons().cordons[0].name, "Hall");
        assert!(doc.add_cordon("flat", aabb([0.0; 3], [64.0, 64.0, 0.0])).is_err(), "a cordon needs a volume");
        assert!(doc.remove_cordon(5).is_err());
        assert_eq!(doc.unique_cordon_name("hall"), "hall 2");
        assert_eq!(doc.unique_cordon_name("Other"), "Other");
        assert_eq!(doc.unique_cordon_name("\""), "cordon");
    }

    #[test]
    fn zero_size_cordons_are_dropped_and_the_modern_block_is_used() {
        let (mut doc, ..) = cordon_doc();
        assert_eq!(doc.scene().cordons().style, CordonStyle::Legacy);
        doc.rename_cordon(0, "Main").expect("rename");
        assert_eq!(doc.scene().cordons().style, CordonStyle::Modern);
        assert!(vmf(&doc).contains("cordons"));
        doc.add_cordon("second", aabb([0.0; 3], [32.0; 3])).expect("add");
        assert_eq!(doc.set_cordon_box(0, aabb([0.0; 3], [32.0, 32.0, 0.0])).expect("flat"), None);
        assert_eq!(doc.scene().cordons().cordons.len(), 1);
        assert_eq!(doc.scene().cordons().cordons[0].name, "second");
        doc.remove_cordon(0).expect("remove");
        let back = hp_vmf::parse(&vmf(&doc)).expect("parse");
        assert!(back.cordons.cordons.is_empty());
    }

    #[test]
    fn normalizing_keeps_extra_boxes_consistent() {
        let mut c = Cordons {
            active: true,
            cordons: vec![
                Cordon { name: "a".into(), active: true, mins: DVec3::ZERO, maxs: DVec3::ZERO, exclude: false },
                Cordon {
                    name: String::new(),
                    active: true,
                    mins: DVec3::splat(4.0),
                    maxs: DVec3::splat(-4.0),
                    exclude: false,
                },
                Cordon {
                    name: "c".into(),
                    active: false,
                    mins: DVec3::splat(f64::NAN),
                    maxs: DVec3::ONE,
                    exclude: false,
                },
            ],
            style: CordonStyle::Legacy,
            extra_boxes: vec![
                CordonBox { cordon: 0, mins: DVec3::splat(10.0), maxs: DVec3::splat(20.0) },
                CordonBox { cordon: 0, mins: DVec3::splat(30.0), maxs: DVec3::splat(40.0) },
                CordonBox { cordon: 1, mins: DVec3::ZERO, maxs: DVec3::new(1.0, 1.0, 0.0) },
                CordonBox { cordon: 7, mins: DVec3::ZERO, maxs: DVec3::ONE },
            ],
        };
        let remap = normalize_cordons(&mut c);
        assert_eq!(remap, vec![Some(0), Some(1), None]);
        assert_eq!((c.cordons[0].mins, c.cordons[0].maxs), (DVec3::splat(10.0), DVec3::splat(20.0)));
        assert_eq!((c.cordons[1].name.as_str(), c.cordons[1].mins), ("cordon", DVec3::splat(-4.0)));
        assert_eq!(c.extra_boxes, vec![CordonBox { cordon: 0, mins: DVec3::splat(30.0), maxs: DVec3::splat(40.0) }]);
        assert_eq!(c.style, CordonStyle::Modern);
        let mut doc = Document::new_empty();
        doc.edit_cordons(c).expect("edit");
        assert_eq!(doc.cordon_boxes().len(), 3);
        assert_eq!(doc.cordon_box_count(), 3);
        assert!(!doc.is_point_cordoned_out(DVec3::splat(35.0)));
        assert!(doc.is_point_cordoned_out(DVec3::splat(25.0)));
    }

    #[test]
    fn hostile_cordon_counts_stay_bounded() {
        let mut doc = Document::new_empty();
        let many = Cordons {
            active: true,
            cordons: (0..5000)
                .map(|i| Cordon {
                    name: format!("c{i}"),
                    active: true,
                    mins: DVec3::splat(f64::from(i) * 10.0),
                    maxs: DVec3::splat(f64::from(i) * 10.0 + 1.0),
                    exclude: false,
                })
                .collect(),
            ..Cordons::default()
        };
        doc.edit_cordons(many).expect("edit");
        assert_eq!(doc.scene().cordons().cordons.len(), 5000, "edits never drop user data");
        assert_eq!(doc.cordon_boxes().len(), MAX_CORDON_BOXES);
        assert!(doc.is_point_cordoned_out(DVec3::splat(-50.0)));
    }
}
