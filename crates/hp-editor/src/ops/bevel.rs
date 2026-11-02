use glam::DVec3;
use hp_geom::{BevelKind, BevelParams, BevelResult, ClipPiece, bevel_limit, bevel_opening, bevel_solid};
use hp_vmf::{Side, Solid};

use super::csg::{derived_solid, record_side_ids};
use crate::document::Document;
use crate::ids::{IdMap, id_map, id_set};
use crate::scene::Owner;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

const UNDO_BEVEL: &str = "Bevel edges";

const NEIGHBOR_MARGIN: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeRef {
    pub solid: Id,
    pub a: DVec3,
    pub b: DVec3,
}

impl EdgeRef {
    pub fn new(solid: Id, a: DVec3, b: DVec3) -> Self {
        Self { solid, a, b }
    }

    pub fn same_edge(&self, other: &EdgeRef) -> bool {
        let close = |p: DVec3, q: DVec3| p.distance_squared(q) < 1e-4;
        self.solid == other.solid
            && ((close(self.a, other.a) && close(self.b, other.b))
                || (close(self.a, other.b) && close(self.b, other.a)))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BevelOutcome {
    pub ids: Vec<Id>,
    pub radius: f64,
    pub segments: u32,
    pub kinds: Vec<BevelKind>,
}

fn group(edges: &[EdgeRef]) -> Vec<(Id, Vec<[DVec3; 2]>)> {
    let mut out: Vec<(Id, Vec<[DVec3; 2]>)> = Vec::new();
    for e in edges {
        match out.iter_mut().find(|(id, _)| *id == e.solid) {
            Some((_, list)) => list.push([e.a, e.b]),
            None => out.push((e.solid, vec![[e.a, e.b]])),
        }
    }
    out
}

impl Document {
    pub fn bevel_neighbors(&self, solid: Id) -> Vec<Id> {
        let Some(n) = self.scene.solids.get(&solid) else { return Vec::new() };
        let zone = n.bounds.expand(NEIGHBOR_MARGIN);
        let mut ids: Vec<Id> = self
            .scene
            .solids
            .iter()
            .filter(|(id, o)| **id != solid && o.geom.valid && o.bounds.intersects(&zone))
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    fn neighbor_sides(&self, solid: Id) -> Vec<&[Side]> {
        self.bevel_neighbors(solid)
            .into_iter()
            .filter_map(|id| self.scene.solids.get(&id).map(|n| n.solid.sides.as_slice()))
            .collect()
    }

    pub fn bevel_opening(&self, edge: &EdgeRef, inverse: bool) -> Option<DVec3> {
        let n = self.scene.solids.get(&edge.solid)?;
        bevel_opening(&n.solid.sides, [edge.a, edge.b], &self.neighbor_sides(edge.solid), inverse)
    }

    pub fn bevel_limit(&self, edges: &[EdgeRef], inverse: bool) -> EditResult<f64> {
        let mut limit = hp_geom::MAX_BEVEL_RADIUS;
        for (solid, list) in group(edges) {
            let n = self.scene.solids.get(&solid).ok_or(EditError::NotFound(ObjRef::Solid(solid)))?;
            limit = limit.min(bevel_limit(&n.solid.sides, &list, &self.neighbor_sides(solid), inverse)?);
        }
        Ok(limit)
    }

    pub fn bevel_preview(
        &self,
        edges: &[EdgeRef],
        params: &BevelParams,
        next_side_id: &mut dyn FnMut() -> u32,
    ) -> EditResult<Vec<(Id, BevelResult)>> {
        if edges.is_empty() {
            return Err(EditError::Nothing("no edge to bevel"));
        }
        let groups = group(edges);
        let limit = self.bevel_limit(edges, params.inverse)?;
        let radius = if params.radius.is_finite() { params.radius.min(limit) } else { params.radius };
        let params = BevelParams { radius, ..params.clone() };
        let mut out = Vec::with_capacity(groups.len());
        let mut segments = params.segments;
        for (solid, list) in &groups {
            let n = self.scene.solids.get(solid).ok_or(EditError::NotFound(ObjRef::Solid(*solid)))?;
            let p = BevelParams { segments, ..params.clone() };
            let r = bevel_solid(&n.solid.sides, list, &self.neighbor_sides(*solid), &p, next_side_id)?;
            segments = segments.min(r.segments);
            out.push((*solid, r));
        }
        if out.iter().any(|(_, r)| r.segments != segments) {
            out.clear();
            let p = BevelParams { segments, ..params.clone() };
            for (solid, list) in &groups {
                let n = self.scene.solids.get(solid).ok_or(EditError::NotFound(ObjRef::Solid(*solid)))?;
                out.push((*solid, bevel_solid(&n.solid.sides, list, &self.neighbor_sides(*solid), &p, next_side_id)?));
            }
        }
        Ok(out)
    }

    pub fn bevel_edges(&mut self, edges: &[EdgeRef], params: &BevelParams) -> EditResult<BevelOutcome> {
        let mut next = self.next_side_id;
        let results = self.bevel_preview(edges, params, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })?;
        if results.iter().all(|(_, r)| !r.changes_brush() && r.added.is_empty()) {
            return Err(EditError::Nothing("nothing to bevel"));
        }
        self.next_side_id = next;
        let mut side_ids: IdMap<Vec<Id>> = id_map(0);
        for (solid, r) in &results {
            if r.changes_brush()
                && let Some(n) = self.scene.solids.get(solid)
            {
                let refs: Vec<&ClipPiece> = r.pieces.iter().collect();
                record_side_ids(&n.solid.sides, &refs, &mut side_ids);
            }
        }
        let radius = results.iter().map(|(_, r)| r.radius).fold(f64::INFINITY, f64::min);
        let segments = results.iter().map(|(_, r)| r.segments).min().unwrap_or(1);
        let kinds: Vec<BevelKind> = results.iter().flat_map(|(_, r)| r.kinds.iter().copied()).collect();
        self.tx(UNDO_BEVEL, |doc| {
            let mut ids = Vec::new();
            let mut select: Vec<ObjRef> = Vec::new();
            for (solid, r) in results {
                let changes = r.changes_brush();
                let template: Solid = doc
                    .scene
                    .solids
                    .get(&solid)
                    .map(|n| n.solid.clone())
                    .ok_or(EditError::NotFound(ObjRef::Solid(solid)))?;
                let owner = doc.scene.solids.get(&solid).map_or(Owner::World, |n| n.owner);
                let mut pieces = r.pieces.into_iter();
                if changes {
                    match pieces.next() {
                        Some(first) => {
                            let mut s = template.clone();
                            s.sides = first.sides;
                            doc.update_solid(solid, s);
                            ids.push(solid);
                        }
                        None => {
                            let mut gone = id_set(1);
                            gone.insert(solid);
                            let groups = doc.groups_of(&gone, &id_set(0));
                            doc.remove_solids(&[solid]);
                            doc.cleanup_empty_groups(groups);
                        }
                    }
                } else {
                    pieces.next();
                    ids.push(solid);
                }
                let new: Vec<(Solid, Owner)> = pieces
                    .map(|p| p.sides)
                    .chain(r.added)
                    .map(|sides| {
                        let id = doc.alloc_object_id();
                        (derived_solid(&template, id, sides), owner)
                    })
                    .collect();
                let added = doc.add_solids(new);
                match owner {
                    Owner::World => {
                        select.extend(std::iter::once(solid).chain(added.iter().copied()).map(ObjRef::Solid));
                    }
                    Owner::Entity(e) => select.push(ObjRef::Entity(e)),
                }
                ids.extend(added);
            }
            doc.remap_side_refs(&side_ids);
            doc.deselect_missing();
            select.retain(|o| match o {
                ObjRef::Solid(id) => doc.scene.solids.contains_key(id),
                _ => true,
            });
            select.dedup();
            if !select.is_empty() {
                doc.select(&select, SelectOp::Replace);
            }
            ids.dedup();
            Ok(BevelOutcome { ids, radius, segments, kinds })
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use glam::DVec3;
    use hp_geom::{BevelKind, BevelParams};

    use super::EdgeRef;
    use crate::tests::{box_sides, check_undo_redo, vmf};
    use crate::{Document, ObjRef, Owner};

    fn v3(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x, y, z)
    }

    fn params(radius: f64, segments: u32, inverse: bool) -> BevelParams {
        BevelParams { radius, segments, inverse, material: None }
    }

    #[test]
    fn rounding_an_edge_is_one_undo_step() {
        let mut doc = Document::new_empty();
        let block =
            doc.create_solid(box_sides(DVec3::ZERO, v3(128.0, 64.0, 64.0), "BRICK/WALL"), Owner::World).expect("block");
        doc.clear_history();
        let edge = EdgeRef::new(block, v3(0.0, 0.0, 64.0), v3(128.0, 0.0, 64.0));
        let out = check_undo_redo(&mut doc, |d| d.bevel_edges(&[edge], &params(16.0, 4, false)).expect("bevel"));
        assert_eq!(out.ids, vec![block], "the brush keeps its id");
        assert_eq!((out.radius, out.segments), (16.0, 4));
        assert_eq!(out.kinds, vec![BevelKind::Round]);
        assert_eq!(doc.history_len().0, 1);
        assert_eq!(doc.undo_label(), Some("Bevel edges"));
        let n = doc.scene().solid(block).expect("block");
        assert!(n.geom.valid);
        assert_eq!(n.solid.sides.len(), 10);
        assert!(n.solid.sides.iter().all(|s| s.material == "BRICK/WALL"));
        assert_eq!(doc.selected_solid_ids(), vec![block]);
        let before = vmf(&doc);
        let bad = EdgeRef::new(block, v3(0.0, 0.0, 64.0), v3(128.0, 64.0, 0.0));
        assert!(doc.bevel_edges(&[bad], &params(16.0, 4, false)).is_err());
        assert!(doc.bevel_edges(&[], &params(16.0, 4, false)).is_err());
        assert!(doc.bevel_edges(&[EdgeRef::new(9999, DVec3::ZERO, DVec3::X)], &params(16.0, 4, false)).is_err());
        assert_eq!(vmf(&doc), before, "failed requests change nothing");
    }

    #[test]
    fn several_edges_and_a_cove() {
        let mut doc = Document::new_empty();
        let a = doc.create_solid(box_sides(DVec3::ZERO, v3(128.0, 64.0, 64.0), "A/A"), Owner::World).expect("a");
        let b =
            doc.create_solid(box_sides(v3(0.0, 200.0, 0.0), v3(64.0, 264.0, 64.0), "B/B"), Owner::World).expect("b");
        let edges = [
            EdgeRef::new(a, v3(0.0, 0.0, 64.0), v3(128.0, 0.0, 64.0)),
            EdgeRef::new(b, v3(0.0, 200.0, 64.0), v3(64.0, 200.0, 64.0)),
        ];
        assert_eq!(doc.bevel_limit(&edges, true).expect("limit"), 64.0);
        let out = doc.bevel_edges(&edges, &params(8.0, 3, true)).expect("coves");
        assert_eq!(out.kinds, vec![BevelKind::Cove, BevelKind::Cove]);
        assert!(out.ids.len() > 2);
        assert!(out.ids.contains(&a) && out.ids.contains(&b));
        assert_eq!(doc.history_len().0, 3, "two creations and one bevel");
        assert_eq!(doc.selected_solid_ids().len(), out.ids.len());
        for id in &out.ids {
            assert!(doc.scene().solid(*id).expect("piece").geom.valid);
        }
        assert!(doc.undo());
        assert_eq!(doc.scene().solids().len(), 2);
    }

    #[test]
    fn filling_a_corner_adds_brushes_in_the_same_entity() {
        let mut doc = Document::new_empty();
        let e = doc
            .create_brush_entity(
                "func_detail",
                &[],
                vec![
                    box_sides(v3(0.0, 0.0, 0.0), v3(32.0, 16.0, 128.0), "WALL/JAMB"),
                    box_sides(v3(32.0, 0.0, 96.0), v3(160.0, 16.0, 128.0), "WALL/LINTEL"),
                ],
            )
            .expect("entity");
        let ids = doc.scene().entity_solid_ids(e);
        let lintel = ids.iter().copied().find(|id| doc.scene().solid(*id).expect("s").bounds.min.x == 32.0).expect("l");
        assert_eq!(doc.bevel_neighbors(lintel).len(), 1);
        let edge = EdgeRef::new(lintel, v3(32.0, 0.0, 96.0), v3(32.0, 16.0, 96.0));
        let mut p = params(32.0, 6, true);
        p.material = Some("DEV/ARCH".to_owned());
        let out = doc.bevel_edges(&[edge], &p).expect("fill");
        assert_eq!(out.kinds, vec![BevelKind::Fill]);
        assert_eq!(out.ids[0], lintel, "the lintel stays as it was");
        assert!(out.ids.len() >= 2);
        assert_eq!(doc.scene().entity_solid_ids(e).len(), 2 + out.ids.len() - 1);
        assert!(doc.selection().objects.contains(&ObjRef::Entity(e)) && doc.selection().objects.len() == 1);
        let curved = out.ids[1..]
            .iter()
            .flat_map(|id| doc.scene().solid(*id).expect("fill").solid.sides.clone())
            .filter(|s| s.material == "DEV/ARCH")
            .count();
        assert_eq!(curved, 6, "the six curved faces take the material");
    }
}
