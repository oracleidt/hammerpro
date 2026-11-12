use hp_geom::Aabb;
use hp_vmf::{EditorInfo, Entity};

use super::ENTITY_COLOR;
use crate::document::Document;
use crate::ids::{IdMap, id_map};
use crate::scene::Owner;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

pub const DETAIL_CLASS: &str = "func_detail";
pub const MAKE_DETAIL_LABEL: &str = "Make func_detail";
const TOUCH: f64 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailSplit {
    #[default]
    Selection,
    Cluster,
}

impl DetailSplit {
    pub fn id(self) -> &'static str {
        match self {
            DetailSplit::Selection => "selection",
            DetailSplit::Cluster => "cluster",
        }
    }

    pub fn parse(s: &str) -> Option<DetailSplit> {
        match s.trim().to_ascii_lowercase().as_str() {
            "selection" | "merge" | "one" => Some(DetailSplit::Selection),
            "cluster" | "clusters" | "connected" => Some(DetailSplit::Cluster),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetailPlan {
    pub world: Vec<Id>,
    pub already: Vec<Id>,
    pub other_entity: Vec<Id>,
    pub displacements: Vec<Id>,
    pub targets: Vec<Id>,
}

impl DetailPlan {
    pub fn is_empty(&self) -> bool {
        self.world.is_empty()
    }
}

pub fn is_detail_class(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case(DETAIL_CLASS)
}

impl Document {
    pub fn is_detail_entity(&self, id: Id) -> bool {
        self.scene.entities.get(&id).is_some_and(|n| is_detail_class(&n.entity.classname))
    }

    pub fn detail_plan(&self, objs: &[ObjRef]) -> DetailPlan {
        let mut plan = DetailPlan::default();
        let mut solids = self.scene.expand_solids(objs);
        solids.sort_unstable();
        for o in objs {
            if let ObjRef::Entity(e) = *o
                && self.is_detail_entity(e)
                && self.scene.is_brush_entity(e)
                && !plan.targets.contains(&e)
            {
                plan.targets.push(e);
            }
        }
        for id in solids {
            let Some(n) = self.scene.solids.get(&id) else { continue };
            match n.owner {
                Owner::Entity(e) if self.is_detail_entity(e) => {
                    plan.already.push(id);
                    if !plan.targets.contains(&e) {
                        plan.targets.push(e);
                    }
                }
                Owner::Entity(_) => plan.other_entity.push(id),
                Owner::World if n.has_disp() => plan.displacements.push(id),
                Owner::World => plan.world.push(id),
            }
        }
        plan
    }

    pub fn make_func_detail(
        &mut self,
        world: &[Id],
        targets: &[Id],
        split: DetailSplit,
        keyvalues: &[(String, String)],
    ) -> EditResult<Vec<Id>> {
        let mut ids: Vec<Id> = world
            .iter()
            .copied()
            .filter(|id| self.scene.solids.get(id).is_some_and(|n| n.owner == Owner::World && !n.has_disp()))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Err(EditError::Nothing("no world brush to turn into func_detail"));
        }
        let targets: Vec<Id> =
            targets.iter().copied().filter(|e| self.is_detail_entity(*e) && self.scene.is_brush_entity(*e)).collect();
        let buckets = self.detail_buckets(&ids, split);
        let target_boxes: Vec<(Id, Option<Id>, Vec<Aabb>)> = targets
            .iter()
            .map(|e| {
                let group = self.scene.entities.get(e).and_then(|n| n.entity.editor.group_id);
                let boxes = self
                    .scene
                    .entity_solid_ids(*e)
                    .iter()
                    .filter_map(|s| self.scene.solids.get(s).map(|n| n.geom.aabb.expand(TOUCH)))
                    .collect();
                (*e, group, boxes)
            })
            .collect();
        let mut plan: Vec<(Option<Id>, Option<Id>, Vec<Id>)> = Vec::with_capacity(buckets.len());
        let mut used: Vec<Id> = Vec::new();
        for (group, members) in buckets {
            let boxes: Vec<Aabb> =
                members.iter().filter_map(|s| self.scene.solids.get(s).map(|n| n.geom.aabb)).collect();
            let target = target_boxes
                .iter()
                .filter(|(_, g, _)| *g == group)
                .find(|(_, _, tb)| {
                    split == DetailSplit::Selection || tb.iter().any(|t| boxes.iter().any(|b| t.intersects(b)))
                })
                .map(|(e, _, _)| *e);
            if let Some(t) = target
                && !used.contains(&t)
            {
                used.push(t);
            }
            plan.push((target, group, members));
        }
        let props = super::create::clean_keyvalues(keyvalues, false);
        self.tx(MAKE_DETAIL_LABEL, |doc| {
            let mut out: Vec<Id> = Vec::with_capacity(plan.len());
            for (target, group, members) in plan {
                let id = match target {
                    Some(t) => t,
                    None => {
                        let id = doc.alloc_object_id();
                        doc.add_entity(Entity {
                            id,
                            classname: DETAIL_CLASS.to_owned(),
                            props: props.clone(),
                            editor: EditorInfo { color: ENTITY_COLOR, group_id: group, ..EditorInfo::default() },
                            ..Entity::default()
                        });
                        id
                    }
                };
                doc.tie_impl(&members, id);
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            let refs: Vec<ObjRef> = out.iter().map(|id| ObjRef::Entity(*id)).collect();
            doc.select(&refs, SelectOp::Replace);
            Ok(out)
        })
    }

    fn detail_buckets(&self, ids: &[Id], split: DetailSplit) -> Vec<(Option<Id>, Vec<Id>)> {
        let mut by_group: Vec<(Option<Id>, Vec<Id>)> = Vec::new();
        for id in ids {
            let group = self.scene.solids.get(id).and_then(|n| n.solid.editor.group_id);
            match by_group.iter_mut().find(|(g, _)| *g == group) {
                Some((_, v)) => v.push(*id),
                None => by_group.push((group, vec![*id])),
            }
        }
        if split == DetailSplit::Selection {
            return by_group;
        }
        let mut out = Vec::new();
        for (group, members) in by_group {
            for cluster in self.touching_clusters(&members) {
                out.push((group, cluster));
            }
        }
        out
    }

    pub fn touching_clusters(&self, ids: &[Id]) -> Vec<Vec<Id>> {
        let items: Vec<(Id, Aabb)> =
            ids.iter().filter_map(|id| self.scene.solids.get(id).map(|n| (*id, n.geom.aabb.expand(TOUCH)))).collect();
        let n = items.len();
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|a, b| items[*a].1.min.x.total_cmp(&items[*b].1.min.x));
        for (k, &i) in order.iter().enumerate() {
            let bi = items[i].1;
            for &j in order.iter().skip(k + 1) {
                let bj = items[j].1;
                if bj.min.x > bi.max.x {
                    break;
                }
                if bi.intersects(&bj) {
                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                    if ri != rj {
                        parent[ri.max(rj)] = ri.min(rj);
                    }
                }
            }
        }
        let mut groups: IdMap<Vec<Id>> = id_map(n);
        let mut roots: Vec<Id> = Vec::new();
        for i in 0..n {
            let r = find(&mut parent, i);
            let key = items[r].0;
            if !groups.contains_key(&key) {
                roots.push(key);
            }
            groups.entry(key).or_default().push(items[i].0);
        }
        roots.into_iter().filter_map(|r| groups.swap_remove(&r)).collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use glam::DVec3;

    use super::*;
    use crate::tests::{check_undo_redo, cube};

    fn doc_with(centers: &[DVec3], size: f64) -> (Document, Vec<Id>) {
        let mut doc = Document::new_empty();
        let ids = centers.iter().map(|c| doc.create_solid(cube(*c, size), Owner::World).expect("cube")).collect();
        doc.clear_history();
        doc.clear_selection();
        (doc, ids)
    }

    fn solids(ids: &[Id]) -> Vec<ObjRef> {
        ids.iter().map(|id| ObjRef::Solid(*id)).collect()
    }

    #[test]
    fn one_func_detail_for_the_selection_in_one_undo_step() {
        let (mut doc, ids) = doc_with(&[DVec3::ZERO, DVec3::new(500.0, 0.0, 0.0), DVec3::new(0.0, 900.0, 0.0)], 32.0);
        let plan = doc.detail_plan(&solids(&ids));
        assert_eq!(plan.world, ids);
        let before = doc.undo_label().map(str::to_owned);
        let ents = check_undo_redo(&mut doc, |d| {
            d.make_func_detail(&plan.world, &plan.targets, DetailSplit::Selection, &[]).expect("make")
        });
        assert_eq!(ents.len(), 1);
        assert_eq!(doc.undo_label(), Some(MAKE_DETAIL_LABEL));
        assert_ne!(doc.undo_label().map(str::to_owned), before);
        let e = &doc.scene().entity(ents[0]).expect("entity").entity;
        assert_eq!(e.classname, DETAIL_CLASS);
        let mut owned = doc.scene().entity_solid_ids(ents[0]);
        owned.sort_unstable();
        assert_eq!(owned, ids);
        assert!(doc.selection().objects.iter().eq([ObjRef::Entity(ents[0])].iter()));
    }

    #[test]
    fn clusters_split_touching_brushes_apart_from_far_ones() {
        let (mut doc, ids) = doc_with(
            &[DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 0.0, 0.0), DVec3::new(2000.0, 0.0, 0.0)],
            64.0,
        );
        let ents = doc.make_func_detail(&ids, &[], DetailSplit::Cluster, &[]).expect("make");
        assert_eq!(ents.len(), 2);
        let mut sizes: Vec<usize> = ents.iter().map(|e| doc.scene().entity_solid_ids(*e).len()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![1, 3]);
    }

    #[test]
    fn plan_sorts_world_detail_other_entities_and_merges_into_selected_detail() {
        let (mut doc, ids) = doc_with(&[DVec3::ZERO, DVec3::new(200.0, 0.0, 0.0), DVec3::new(400.0, 0.0, 0.0)], 32.0);
        let detail = doc.tie_to_entity(&[ObjRef::Solid(ids[0])], DETAIL_CLASS, &[]).expect("detail");
        let brush = doc.tie_to_entity(&[ObjRef::Solid(ids[1])], "func_brush", &[]).expect("brush");
        let objs = vec![ObjRef::Entity(detail), ObjRef::Entity(brush), ObjRef::Solid(ids[2])];
        let plan = doc.detail_plan(&objs);
        assert_eq!(plan.world, vec![ids[2]]);
        assert_eq!(plan.already, vec![ids[0]]);
        assert_eq!(plan.other_entity, vec![ids[1]]);
        assert_eq!(plan.targets, vec![detail]);
        let ents = doc.make_func_detail(&plan.world, &plan.targets, DetailSplit::Selection, &[]).expect("make");
        assert_eq!(ents, vec![detail]);
        assert_eq!(doc.scene().entity_solid_ids(detail).len(), 2);
        assert_eq!(doc.scene().solid(ids[1]).expect("brush").owner, Owner::Entity(brush));
    }

    #[test]
    fn groups_visgroups_and_textures_are_kept() {
        let (mut doc, ids) = doc_with(&[DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(900.0, 0.0, 0.0)], 64.0);
        let group = doc.group(&solids(&ids[..2])).expect("group");
        let vg = doc.create_visgroup("Trims", None, &solids(&ids)).expect("visgroup");
        let materials: Vec<String> =
            ids.iter().map(|id| doc.scene().solid(*id).expect("solid").solid.sides[0].material.clone()).collect();
        let ents = doc.make_func_detail(&ids, &[], DetailSplit::Selection, &[]).expect("make");
        assert_eq!(ents.len(), 2);
        let grouped: Vec<Id> = ents
            .iter()
            .copied()
            .filter(|e| doc.scene().entity(*e).and_then(|n| n.entity.editor.group_id) == Some(group))
            .collect();
        assert_eq!(grouped.len(), 1);
        assert_eq!(doc.scene().entity_solid_ids(grouped[0]).len(), 2);
        assert!(doc.scene().groups.contains_key(&group));
        for (id, m) in ids.iter().zip(&materials) {
            let n = doc.scene().solid(*id).expect("solid");
            assert!(n.solid.editor.visgroup_ids.contains(&vg));
            assert_eq!(&n.solid.sides[0].material, m);
        }
    }

    #[test]
    fn nothing_to_convert_is_an_error_and_leaves_no_undo_step() {
        let (mut doc, ids) = doc_with(&[DVec3::ZERO], 32.0);
        let detail = doc.tie_to_entity(&solids(&ids), DETAIL_CLASS, &[]).expect("detail");
        doc.clear_history();
        let plan = doc.detail_plan(&[ObjRef::Entity(detail)]);
        assert!(plan.is_empty());
        assert_eq!(plan.already, ids);
        assert!(doc.make_func_detail(&plan.world, &plan.targets, DetailSplit::Selection, &[]).is_err());
        assert!(doc.undo_label().is_none());
    }

    #[test]
    fn split_ids_parse() {
        for s in [DetailSplit::Selection, DetailSplit::Cluster] {
            assert_eq!(DetailSplit::parse(s.id()), Some(s));
        }
        assert_eq!(DetailSplit::parse("nope"), None);
    }
}
