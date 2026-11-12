use hp_geom::{ClipPiece, Plane, intersect_traced, keep_cut_displacements, union_pieces, validate_solid};
use hp_vmf::Side;

use super::csg::{derived_solid, record_side_ids};
use crate::document::Document;
use crate::ids::{IdMap, IdSet, id_map, id_set};
use crate::scene::Owner;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

pub(super) fn plane_side_map(sources: &[&[Side]], merged: &[Side]) -> IdMap<Vec<Id>> {
    let planes: Vec<(Id, Option<Plane>)> = merged.iter().map(|m| (m.id, Plane::from_side(m))).collect();
    let mut map: IdMap<Vec<Id>> = id_map(0);
    for src in sources.iter().flat_map(|s| s.iter()) {
        let sp = Plane::from_side(src);
        let new: Vec<Id> = planes
            .iter()
            .filter(|(_, p)| matches!((p, &sp), (Some(a), Some(b)) if a.approx_eq(b, 1e-9, 1e-6)))
            .map(|(id, _)| *id)
            .collect();
        if new != [src.id] {
            map.insert(src.id, new);
        }
    }
    map
}

impl Document {
    pub const BOOLEAN_NO_OVERLAP: &'static str = "the brushes do not overlap";
    pub const UNION_UNCHANGED: &'static str = "the brushes cannot be joined any further";

    fn boolean_inputs(&self, solids: &[Id]) -> EditResult<(Vec<Id>, Vec<Vec<Side>>)> {
        let mut unique: IdSet = id_set(solids.len());
        for id in solids {
            if self.scene.solids.contains_key(id) {
                unique.insert(*id);
            }
        }
        if unique.len() < 2 {
            return Err(EditError::Nothing("select at least two solids"));
        }
        let mut owners = unique.iter().filter_map(|id| self.scene.solids.get(id).map(|n| n.owner));
        let first_owner = owners.next();
        if owners.any(|o| Some(o) != first_owner) {
            return Err(EditError::Invalid(Self::MERGE_MIXED_OWNERS.to_owned()));
        }
        let ids: Vec<Id> = unique.into_iter().collect();
        let lists =
            ids.iter().filter_map(|id| self.scene.solids.get(id).map(|n| n.solid.sides.clone())).collect::<Vec<_>>();
        for l in &lists {
            validate_solid(l)?;
        }
        Ok((ids, lists))
    }

    pub fn intersect_solids(&mut self, solids: &[Id]) -> EditResult<Id> {
        let (ids, lists) = self.boolean_inputs(solids)?;
        let mut next = self.next_side_id;
        let mut gen_side = || {
            let id = next;
            next = next.saturating_add(1);
            id
        };
        let mut piece = ClipPiece::whole(&lists[0]);
        for other in &lists[1..] {
            let Some(p) = intersect_traced(&piece.sides, other, &mut gen_side)? else {
                return Err(EditError::Invalid(Self::BOOLEAN_NO_OVERLAP.to_owned()));
            };
            piece = p.compose(&piece.origins);
        }
        validate_solid(&piece.sides)?;
        let mut pieces = if self.options.keep_displacements_on_cut {
            keep_cut_displacements(&lists[0], vec![piece], &mut gen_side)
        } else {
            vec![piece]
        };
        self.next_side_id = next;
        let first = ids[0];
        let mut side_ids: IdMap<Vec<Id>> = id_map(0);
        let refs: Vec<&ClipPiece> = pieces.iter().collect();
        record_side_ids(&lists[0], &refs, &mut side_ids);
        let others: Vec<ObjRef> = ids.iter().skip(1).map(|id| ObjRef::Solid(*id)).collect();
        let node = self.scene.solids.get(&first).ok_or(EditError::NotFound(ObjRef::Solid(first)))?;
        let (mut solid, owner) = (node.solid.clone(), node.owner);
        let template = solid.clone();
        let extra: Vec<Vec<Side>> = pieces.drain(1..).map(|p| p.sides).collect();
        solid.sides = pieces.remove(0).sides;
        self.tx("Intersect", |doc| {
            doc.update_solid(first, solid);
            let mut created = Vec::with_capacity(extra.len());
            for sides in extra {
                let id = doc.alloc_object_id();
                created.push((derived_solid(&template, id, sides), owner));
            }
            doc.add_solids(created);
            doc.remap_side_refs(&side_ids);
            doc.delete(&others)?;
            Ok(first)
        })
    }

    pub fn union_solids(&mut self, solids: &[Id]) -> EditResult<Vec<Id>> {
        let (ids, lists) = self.boolean_inputs(solids)?;
        let mut next = self.next_side_id;
        let mut gen_side = || {
            let id = next;
            next = next.saturating_add(1);
            id
        };
        let parts = union_pieces(&lists, &mut gen_side)?;
        let unchanged =
            parts.len() == lists.len() && parts.iter().enumerate().all(|(i, p)| p.origin == i && p.sides == lists[i]);
        if unchanged {
            return Err(EditError::Invalid(Self::UNION_UNCHANGED.to_owned()));
        }
        self.next_side_id = next;
        let mut side_ids: IdMap<Vec<Id>> = id_map(0);
        for src in lists.iter().flatten() {
            side_ids.insert(src.id, Vec::new());
        }
        for part in &parts {
            for (side, srcs) in part.sides.iter().zip(&part.sources) {
                for src in srcs {
                    if let Some(list) = side_ids.get_mut(src) {
                        list.push(side.id);
                    }
                }
            }
        }
        side_ids.retain(|src, new| new.as_slice() != [*src]);
        let owner = self.scene.solids.get(&ids[0]).map_or(Owner::World, |n| n.owner);
        let templates: Vec<_> =
            ids.iter().filter_map(|id| self.scene.solids.get(id).map(|n| n.solid.clone())).collect();
        self.tx("Union", |doc| {
            let mut claimed = vec![false; ids.len()];
            let mut result = Vec::with_capacity(parts.len());
            let mut created = Vec::new();
            for part in parts {
                let (k, sides) = (part.origin, part.sides);
                let template = &templates[k];
                if claimed[k] {
                    let id = doc.alloc_object_id();
                    created.push((derived_solid(template, id, sides), owner));
                } else {
                    claimed[k] = true;
                    doc.update_solid(ids[k], derived_solid(template, ids[k], sides));
                    result.push(ids[k]);
                }
            }
            let gone: Vec<ObjRef> =
                ids.iter().zip(&claimed).filter(|(_, c)| !**c).map(|(id, _)| ObjRef::Solid(*id)).collect();
            let selected = ids.iter().any(|id| doc.selection.objects.contains(&ObjRef::Solid(*id)));
            let added = doc.add_solids(created);
            if selected {
                let objs: Vec<ObjRef> = added.iter().map(|id| ObjRef::Solid(*id)).collect();
                doc.select(&objs, SelectOp::Add);
            }
            result.extend(added);
            doc.remap_side_refs(&side_ids);
            doc.delete(&gone)?;
            Ok(result)
        })
    }
}
