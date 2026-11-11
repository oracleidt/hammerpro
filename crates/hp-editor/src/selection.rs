use glam::DVec3;
use hp_geom::Aabb;
use hp_vmf::Entity;
use indexmap::IndexSet;

use crate::Id;
use crate::document::Document;
use crate::glob::glob_match;
use crate::ids::{IdBuildHasher, IdSet, id_set};
use crate::pick::RectMode;
use crate::scene::{Owner, Scene};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjRef {
    Solid(Id),
    Entity(Id),
}

impl ObjRef {
    pub fn id(self) -> Id {
        match self {
            ObjRef::Solid(id) | ObjRef::Entity(id) => id,
        }
    }

    pub fn is_solid(self) -> bool {
        matches!(self, ObjRef::Solid(_))
    }

    pub fn is_entity(self) -> bool {
        matches!(self, ObjRef::Entity(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceRef {
    pub solid: Id,
    pub side: usize,
}

impl FaceRef {
    pub fn new(solid: Id, side: usize) -> Self {
        Self { solid, side }
    }
}

const FACE_HISTORY_LEN: usize = 64;

#[derive(Clone, Debug, Default)]
pub(crate) struct FaceHistory {
    pub(crate) back: Vec<Vec<FaceRef>>,
    pub(crate) forward: Vec<Vec<FaceRef>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub objects: IndexSet<ObjRef, IdBuildHasher>,
    pub faces: IndexSet<FaceRef, IdBuildHasher>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.faces.is_empty()
    }

    pub fn contains(&self, obj: ObjRef) -> bool {
        self.objects.contains(&obj)
    }

    pub fn contains_face(&self, face: FaceRef) -> bool {
        self.faces.contains(&face)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SelectMode {
    #[default]
    Groups,
    Objects,
    Solids,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SelectOp {
    #[default]
    Replace,
    Add,
    Remove,
    Toggle,
}

pub(crate) fn remove_all<T: std::hash::Hash + Eq + Copy>(set: &mut IndexSet<T, IdBuildHasher>, items: &[T]) {
    match items {
        [] => {}
        [one] => {
            set.shift_remove(one);
        }
        _ => {
            let gone: IndexSet<T, IdBuildHasher> = items.iter().copied().collect();
            set.retain(|x| !gone.contains(x));
        }
    }
}

fn toggle_all<T: std::hash::Hash + Eq + Copy>(set: &mut IndexSet<T, IdBuildHasher>, items: Vec<T>) {
    let mut counts: indexmap::IndexMap<T, u32, IdBuildHasher> =
        indexmap::IndexMap::with_capacity_and_hasher(items.len(), IdBuildHasher::default());
    for x in items {
        *counts.entry(x).or_insert(0) += 1;
    }
    let mut gone: IndexSet<T, IdBuildHasher> = IndexSet::default();
    let mut added = Vec::new();
    for (x, n) in counts {
        if n % 2 == 1 {
            if set.contains(&x) {
                gone.insert(x);
            } else {
                added.push(x);
            }
        }
    }
    if !gone.is_empty() {
        set.retain(|x| !gone.contains(x));
    }
    set.extend(added);
}

pub(crate) const DEFAULT_ENTITY_HALF_SIZE: f64 = 8.0;

pub(crate) fn point_entity_bounds(e: &Entity, entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>) -> Option<Aabb> {
    if let Some(b) = entity_bounds(e)
        && !b.is_empty()
        && b.min.is_finite()
        && b.max.is_finite()
    {
        return Some(b);
    }
    let o = e.origin()?;
    o.is_finite()
        .then(|| Aabb::new(o - DVec3::splat(DEFAULT_ENTITY_HALF_SIZE), o + DVec3::splat(DEFAULT_ENTITY_HALF_SIZE)))
}

impl Scene {
    pub fn resolve(&self, obj: ObjRef, mode: SelectMode) -> Vec<ObjRef> {
        if !self.contains(obj) {
            return Vec::new();
        }
        let base = match (obj, mode) {
            (ObjRef::Solid(id), SelectMode::Solids) => return vec![ObjRef::Solid(id)],
            (ObjRef::Solid(id), _) => self.object_of_solid(id),
            (ObjRef::Entity(id), _) => ObjRef::Entity(id),
        };
        if mode != SelectMode::Groups {
            return vec![base];
        }
        match self.object_group(base) {
            Some(g) => {
                let members = self.group_members(self.group_root(g));
                if members.is_empty() { vec![base] } else { members }
            }
            None => vec![base],
        }
    }

    pub fn units(&self) -> impl Iterator<Item = ObjRef> + '_ {
        self.solids
            .values()
            .filter(|n| n.owner == Owner::World)
            .map(|n| ObjRef::Solid(n.solid.id))
            .chain(self.entities.keys().map(|id| ObjRef::Entity(*id)))
    }

    pub fn expand_solids(&self, objs: &[ObjRef]) -> Vec<Id> {
        let mut out = id_set(objs.len());
        for o in objs {
            match *o {
                ObjRef::Solid(id) => {
                    if self.solids.contains_key(&id) {
                        out.insert(id);
                    }
                }
                ObjRef::Entity(id) => {
                    for sid in self.entity_solid_ids(id) {
                        out.insert(sid);
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    pub fn expand_entities(&self, objs: &[ObjRef]) -> Vec<Id> {
        let mut out = id_set(objs.len());
        for o in objs {
            if let ObjRef::Entity(id) = *o
                && self.entities.contains_key(&id)
            {
                out.insert(id);
            }
        }
        out.into_iter().collect()
    }

    pub fn object_bounds(&self, obj: ObjRef, entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>) -> Option<Aabb> {
        match obj {
            ObjRef::Solid(id) => self.solid_bounds(id),
            ObjRef::Entity(id) => {
                if let Some(b) = self.entity_solid_bounds(id) {
                    return Some(b);
                }
                let e = &self.entities.get(&id)?.entity;
                point_entity_bounds(e, entity_bounds)
            }
        }
    }

    pub fn objects_bounds(&self, objs: &[ObjRef], entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>) -> Option<Aabb> {
        let mut b = Aabb::EMPTY;
        for o in objs {
            if let Some(ob) = self.object_bounds(*o, entity_bounds) {
                b = b.union(&ob);
            }
        }
        (!b.is_empty()).then_some(b)
    }
}

impl Document {
    pub fn resolve(&self, obj: ObjRef, mode: SelectMode) -> Vec<ObjRef> {
        self.scene.resolve(obj, mode)
    }

    pub fn select(&mut self, objs: &[ObjRef], op: SelectOp) {
        let objs: Vec<ObjRef> = objs.iter().copied().filter(|o| self.scene.contains(*o)).collect();
        let current = &self.selection.objects;
        let unchanged = match op {
            SelectOp::Replace => current.len() == objs.len() && objs.iter().all(|o| current.contains(o)),
            SelectOp::Add => objs.iter().all(|o| current.contains(o)),
            SelectOp::Remove => objs.iter().all(|o| !current.contains(o)),
            SelectOp::Toggle => objs.is_empty(),
        };
        if unchanged {
            if matches!(op, SelectOp::Replace | SelectOp::Add) && !objs.is_empty() {
                self.objects_selected_at = crate::document::next_revision();
            }
            return;
        }
        let sel = self.sel_mut();
        match op {
            SelectOp::Replace => {
                sel.objects.clear();
                sel.objects.extend(objs);
            }
            SelectOp::Add => sel.objects.extend(objs),
            SelectOp::Remove => remove_all(&mut sel.objects, &objs),
            SelectOp::Toggle => toggle_all(&mut sel.objects, objs),
        }
        self.objects_selected_at = crate::document::next_revision();
        self.refresh_selection_step();
    }

    pub fn select_picked(&mut self, obj: ObjRef, mode: SelectMode, op: SelectOp) {
        let objs = self.scene.resolve(obj, mode);
        self.select(&objs, op);
    }

    pub fn set_selection(&mut self, objs: &[ObjRef]) {
        self.select(objs, SelectOp::Replace);
    }

    pub fn deselect(&mut self, objs: &[ObjRef]) {
        self.select(objs, SelectOp::Remove);
    }

    pub fn toggle_selection(&mut self, objs: &[ObjRef]) {
        self.select(objs, SelectOp::Toggle);
    }

    pub fn clear_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        if !self.selection.faces.is_empty() {
            self.remember_faces(self.selection.faces.iter().copied().collect());
        }
        let sel = self.sel_mut();
        sel.objects.clear();
        sel.faces.clear();
        self.objects_selected_at = crate::document::next_revision();
        self.faces_selected_at = self.objects_selected_at;
        self.refresh_selection_step();
    }

    fn remember_faces(&mut self, before: Vec<FaceRef>) {
        let h = &mut self.face_history;
        if h.back.last() != Some(&before) {
            h.back.push(before);
            if h.back.len() > FACE_HISTORY_LEN {
                h.back.remove(0);
            }
        }
        h.forward.clear();
    }

    fn existing_faces(&self, faces: Vec<FaceRef>) -> Vec<FaceRef> {
        faces
            .into_iter()
            .filter(|f| self.scene.solids.get(&f.solid).is_some_and(|n| f.side < n.solid.sides.len()))
            .collect()
    }

    fn set_faces_unrecorded(&mut self, faces: Vec<FaceRef>) {
        let sel = self.sel_mut();
        sel.faces.clear();
        sel.faces.extend(faces);
        self.faces_selected_at = crate::document::next_revision();
        self.refresh_selection_step();
    }

    pub fn face_selection_back(&mut self) -> bool {
        while let Some(prev) = self.face_history.back.pop() {
            let prev = self.existing_faces(prev);
            let current = self.selected_faces();
            if prev == current {
                continue;
            }
            self.face_history.forward.push(current);
            self.set_faces_unrecorded(prev);
            return true;
        }
        false
    }

    pub fn face_selection_forward(&mut self) -> bool {
        while let Some(next) = self.face_history.forward.pop() {
            let next = self.existing_faces(next);
            let current = self.selected_faces();
            if next == current {
                continue;
            }
            self.face_history.back.push(current);
            self.set_faces_unrecorded(next);
            return true;
        }
        false
    }

    pub fn can_face_selection_back(&self) -> bool {
        !self.face_history.back.is_empty()
    }

    pub fn can_face_selection_forward(&self) -> bool {
        !self.face_history.forward.is_empty()
    }

    pub fn select_faces(&mut self, faces: &[FaceRef], op: SelectOp) {
        let faces: Vec<FaceRef> = faces
            .iter()
            .copied()
            .filter(|f| self.scene.solids.get(&f.solid).is_some_and(|n| f.side < n.solid.sides.len()))
            .collect();
        if faces.is_empty() && op != SelectOp::Replace {
            return;
        }
        if op == SelectOp::Replace
            && self.selection.faces.len() == faces.len()
            && faces.iter().all(|f| self.selection.faces.contains(f))
        {
            return;
        }
        let before = self.selection.faces.clone();
        let sel = self.sel_mut();
        match op {
            SelectOp::Replace => {
                sel.faces.clear();
                sel.faces.extend(faces);
            }
            SelectOp::Add => sel.faces.extend(faces),
            SelectOp::Remove => remove_all(&mut sel.faces, &faces),
            SelectOp::Toggle => toggle_all(&mut sel.faces, faces),
        }
        if self.selection.faces != before {
            self.remember_faces(before.into_iter().collect());
            self.faces_selected_at = crate::document::next_revision();
        }
        self.refresh_selection_step();
    }

    pub fn clear_face_selection(&mut self) {
        if !self.selection.faces.is_empty() {
            self.remember_faces(self.selection.faces.iter().copied().collect());
            self.sel_mut().faces.clear();
            self.faces_selected_at = crate::document::next_revision();
            self.refresh_selection_step();
        }
    }

    pub fn select_all(&mut self) {
        let all: Vec<ObjRef> = self.scene.units().filter(|o| self.is_visible(*o)).collect();
        self.select(&all, SelectOp::Replace);
    }

    pub fn invert_selection(&mut self) {
        let selected_solids: IdSet = self.selected_solid_ids().into_iter().collect();
        let inverted: Vec<ObjRef> = self
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
        self.select(&inverted, SelectOp::Replace);
    }

    pub fn selected_objects(&self) -> Vec<ObjRef> {
        self.selection.objects.iter().copied().collect()
    }

    pub fn selected_faces(&self) -> Vec<FaceRef> {
        self.selection.faces.iter().copied().collect()
    }

    pub fn selected_solid_ids(&self) -> Vec<Id> {
        let objs: Vec<ObjRef> = self.selection.objects.iter().copied().collect();
        self.scene.expand_solids(&objs)
    }

    pub fn faces_selected_last(&self) -> bool {
        self.faces_selected_at >= self.objects_selected_at
    }

    pub fn disp_faces_of_solids(&self, ids: &[Id]) -> Vec<FaceRef> {
        let mut out = Vec::new();
        for id in ids {
            let Some(n) = self.scene.solids.get(id) else { continue };
            for (i, s) in n.solid.sides.iter().enumerate() {
                if s.disp.is_some() {
                    out.push(FaceRef::new(*id, i));
                }
            }
        }
        out
    }

    pub fn selected_disp_faces_only(&self) -> Vec<FaceRef> {
        self.selection
            .faces
            .iter()
            .copied()
            .filter(|f| {
                self.scene
                    .solids
                    .get(&f.solid)
                    .and_then(|n| n.solid.sides.get(f.side))
                    .is_some_and(|s| s.disp.is_some())
            })
            .collect()
    }

    pub fn selected_object_disp_faces(&self) -> Vec<FaceRef> {
        self.disp_faces_of_solids(&self.selected_solid_ids())
    }

    pub fn object_disp_targets_win(&self) -> bool {
        !self.faces_selected_last() && !self.selected_object_disp_faces().is_empty()
    }

    pub fn selected_disp_targets(&self) -> Vec<FaceRef> {
        if self.object_disp_targets_win() { self.selected_object_disp_faces() } else { self.selected_disp_faces_only() }
    }

    pub fn group_disp_faces(&self, solid: Id) -> Vec<FaceRef> {
        let Some(g) = self.scene.object_group(self.scene.object_of_solid(solid)) else {
            return self.disp_faces_of_solids(&[solid]);
        };
        let members = self.scene.group_members(self.scene.group_root(g));
        let ids: Vec<Id> = self
            .scene
            .expand_solids(&members)
            .into_iter()
            .filter(|id| *id == solid || self.is_solid_visible(*id))
            .collect();
        let out = self.disp_faces_of_solids(&ids);
        if out.is_empty() { self.disp_faces_of_solids(&[solid]) } else { out }
    }

    pub fn selected_entity_ids(&self) -> Vec<Id> {
        let objs: Vec<ObjRef> = self.selection.objects.iter().copied().collect();
        self.scene.expand_entities(&objs)
    }

    pub fn is_solid_selected(&self, id: Id) -> bool {
        if self.selection.objects.contains(&ObjRef::Solid(id)) {
            return true;
        }
        matches!(self.scene.solids.get(&id).map(|n| n.owner), Some(Owner::Entity(e)) if self.selection.objects.contains(&ObjRef::Entity(e)))
    }

    pub fn is_selected(&self, obj: ObjRef) -> bool {
        match obj {
            ObjRef::Solid(id) => self.is_solid_selected(id),
            ObjRef::Entity(_) => self.selection.objects.contains(&obj),
        }
    }

    pub fn select_by_class(&mut self, pattern: &str, op: SelectOp) -> usize {
        let objs: Vec<ObjRef> = self
            .scene
            .entities
            .values()
            .filter(|n| glob_match(pattern, &n.entity.classname))
            .map(|n| ObjRef::Entity(n.entity.id))
            .filter(|o| self.is_visible(*o))
            .collect();
        let n = objs.len();
        self.select(&objs, op);
        n
    }

    pub fn select_by_targetname(&mut self, pattern: &str, op: SelectOp) -> usize {
        let objs: Vec<ObjRef> = self
            .scene
            .entities
            .values()
            .filter(|n| n.entity.get("targetname").is_some_and(|t| glob_match(pattern, t)))
            .map(|n| ObjRef::Entity(n.entity.id))
            .filter(|o| self.is_visible(*o))
            .collect();
        let n = objs.len();
        self.select(&objs, op);
        n
    }

    pub fn select_by_material(&mut self, pattern: &str, op: SelectOp) -> usize {
        let mut objs = IndexSet::<ObjRef, IdBuildHasher>::default();
        for n in self.scene.solids.values() {
            if n.solid.sides.iter().any(|s| crate::glob::material_match(pattern, &s.material)) {
                let o = self.scene.object_of_solid(n.solid.id);
                if self.is_visible(o) && self.is_solid_visible(n.solid.id) {
                    objs.insert(o);
                }
            }
        }
        let objs: Vec<ObjRef> = objs.into_iter().collect();
        let n = objs.len();
        self.select(&objs, op);
        n
    }

    pub fn select_faces_by_material(&mut self, pattern: &str, op: SelectOp) -> usize {
        let mut faces = Vec::new();
        for n in self.scene.solids.values() {
            if !self.is_solid_visible(n.solid.id) {
                continue;
            }
            for (i, s) in n.solid.sides.iter().enumerate() {
                if crate::glob::material_match(pattern, &s.material) {
                    faces.push(FaceRef::new(n.solid.id, i));
                }
            }
        }
        let count = faces.len();
        self.select_faces(&faces, op);
        count
    }

    pub fn select_in_box(
        &mut self,
        bounds: &Aabb,
        mode: RectMode,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
        op: SelectOp,
    ) -> usize {
        let objs: Vec<ObjRef> = self
            .scene
            .units()
            .filter(|o| self.is_visible(*o))
            .filter(|o| {
                self.scene.object_bounds(*o, entity_bounds).is_some_and(|b| match mode {
                    RectMode::Touching => b.intersects(bounds),
                    RectMode::Inside => bounds.contains_aabb(&b),
                    RectMode::Center => bounds.contains_point(b.center()),
                })
            })
            .collect();
        let n = objs.len();
        self.select(&objs, op);
        n
    }

    pub fn object_bounds(&self, obj: ObjRef, entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>) -> Option<Aabb> {
        self.scene.object_bounds(obj, entity_bounds)
    }

    pub fn selection_bounds(&self, entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>) -> Option<Aabb> {
        if !self.selection.objects.is_empty() {
            let objs = self.selected_objects();
            return self.scene.objects_bounds(&objs, entity_bounds);
        }
        let mut b = Aabb::EMPTY;
        for f in &self.selection.faces {
            if let Some(n) = self.scene.solids.get(&f.solid)
                && let Some(face) = n.geom.face_for_side(f.side)
            {
                for v in &face.vertices {
                    b.extend(*v);
                }
            }
        }
        (!b.is_empty()).then_some(b)
    }

    pub fn selection_center(&self, entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>) -> Option<DVec3> {
        self.selection_bounds(entity_bounds).map(|b| b.center())
    }
}
