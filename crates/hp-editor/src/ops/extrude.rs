use glam::DVec3;
use hp_geom::{
    ClipPiece, ExtrudeMode, FaceExtrudeParams, NormalExtrudeFace, NormalExtrudeParams, NormalExtrudeResult,
    build_solid, face_extrude, normal_extrude,
};
use hp_vmf::{Side, Solid};

use super::csg::{derived_solid, record_side_ids};
use crate::document::Document;
use crate::ids::{IdMap, id_map, id_set};
use crate::scene::Owner;
use crate::selection::{FaceRef, ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

const UNDO_CARVE_FACE: &str = "Carve face";

const UNDO_EXTRUDE_FACE: &str = "Extrude face";

impl Document {
    pub fn extrude_face(
        &mut self,
        face: FaceRef,
        outline: &[DVec3],
        params: &FaceExtrudeParams,
    ) -> EditResult<Vec<Id>> {
        let n = self.scene.solids.get(&face.solid).ok_or(EditError::NotFound(ObjRef::Solid(face.solid)))?;
        if face.side >= n.solid.sides.len() {
            return Err(EditError::Invalid(format!("solid {} has no side {}", face.solid, face.side)));
        }
        let original = n.solid.sides.clone();
        let owner = n.owner;
        let mut next = self.next_side_id;
        let result = face_extrude(&original, face.side, outline, params, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })?;
        let changes = result.changes_brush();
        if !changes && result.added.is_empty() {
            return Err(EditError::Nothing("nothing to carve or extrude"));
        }
        self.next_side_id = next;
        let mut side_ids: IdMap<Vec<Id>> = id_map(0);
        if changes {
            let refs: Vec<&ClipPiece> = result.pieces.iter().collect();
            record_side_ids(&original, &refs, &mut side_ids);
        }
        let label = match params.mode {
            ExtrudeMode::Carve => UNDO_CARVE_FACE,
            ExtrudeMode::Extrude => UNDO_EXTRUDE_FACE,
        };
        self.tx(label, |doc| {
            let template: Solid = doc
                .scene
                .solids
                .get(&face.solid)
                .map(|n| n.solid.clone())
                .ok_or(EditError::NotFound(ObjRef::Solid(face.solid)))?;
            let mut ids = Vec::new();
            let mut pieces = result.pieces.into_iter();
            if changes {
                match pieces.next() {
                    Some(first) => {
                        let mut s = template.clone();
                        s.sides = first.sides;
                        doc.update_solid(face.solid, s);
                        ids.push(face.solid);
                    }
                    None => {
                        let mut gone = id_set(1);
                        gone.insert(face.solid);
                        let groups = doc.groups_of(&gone, &id_set(0));
                        doc.remove_solids(&[face.solid]);
                        doc.cleanup_empty_groups(groups);
                    }
                }
            } else {
                pieces.next();
            }
            let new: Vec<(Solid, Owner)> = pieces
                .map(|p| p.sides)
                .chain(result.added)
                .map(|sides| {
                    let id = doc.alloc_object_id();
                    (derived_solid(&template, id, sides), owner)
                })
                .collect();
            ids.extend(doc.add_solids(new));
            doc.remap_side_refs(&side_ids);
            doc.deselect_missing();
            let select: Vec<ObjRef> = match owner {
                Owner::World => ids.iter().map(|id| ObjRef::Solid(*id)).collect(),
                Owner::Entity(e) => vec![ObjRef::Entity(e)],
            };
            if !select.is_empty() {
                doc.select(&select, SelectOp::Replace);
            }
            Ok(ids)
        })
    }
}

pub struct ExtrudePreview {
    pub brushes: usize,
    pub outlines: Vec<Vec<DVec3>>,
}

struct Plan {
    result: NormalExtrudeResult,
    solids: Vec<Id>,
    next: u32,
}

impl Document {
    fn plan_extrusions(&self, faces: &[(FaceRef, f64)], params: &NormalExtrudeParams) -> EditResult<Plan> {
        if faces.is_empty() {
            return Err(EditError::Nothing("no face to extrude"));
        }
        let mut solids: Vec<Id> = Vec::new();
        let mut list: Vec<NormalExtrudeFace> = Vec::with_capacity(faces.len());
        for (face, distance) in faces {
            let n = self.scene.solids.get(&face.solid).ok_or(EditError::NotFound(ObjRef::Solid(face.solid)))?;
            if face.side >= n.solid.sides.len() {
                return Err(EditError::Invalid(format!("solid {} has no side {}", face.solid, face.side)));
            }
            let brush = match solids.iter().position(|s| *s == face.solid) {
                Some(i) => i,
                None => {
                    solids.push(face.solid);
                    solids.len() - 1
                }
            };
            list.push(NormalExtrudeFace { brush, side: face.side, distance: *distance });
        }
        let brushes: Vec<&[Side]> =
            solids.iter().filter_map(|id| self.scene.solids.get(id).map(|n| n.solid.sides.as_slice())).collect();
        let mut next = self.next_side_id;
        let result = normal_extrude(&brushes, &list, params, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })?;
        if result.brush_count() == 0 {
            return Err(EditError::Nothing("nothing to extrude"));
        }
        Ok(Plan { result, solids, next })
    }

    pub fn extrude_faces_preview(&self, faces: &[(FaceRef, f64)], material: Option<&str>) -> EditResult<usize> {
        let params = NormalExtrudeParams { material: material.map(str::to_owned), ..Default::default() };
        Ok(self.plan_extrusions(faces, &params)?.result.brush_count())
    }

    pub fn extrude_faces_plan(
        &self,
        faces: &[(FaceRef, f64)],
        params: &NormalExtrudeParams,
    ) -> EditResult<ExtrudePreview> {
        let plan = self.plan_extrusions(faces, params)?;
        let shapes = plan.result.changed.iter().map(|(_, p)| &p.sides).chain(plan.result.added.iter().map(|(_, s)| s));
        let mut outlines = Vec::new();
        for sides in shapes {
            outlines.extend(build_solid(sides).faces.into_iter().map(|f| f.vertices));
        }
        Ok(ExtrudePreview { brushes: plan.result.brush_count(), outlines })
    }

    pub fn extrude_faces(&mut self, faces: &[(FaceRef, f64)], material: Option<&str>) -> EditResult<Vec<Id>> {
        let params = NormalExtrudeParams { material: material.map(str::to_owned), ..Default::default() };
        self.extrude_faces_with(faces, &params)
    }

    pub fn extrude_faces_with(
        &mut self,
        faces: &[(FaceRef, f64)],
        params: &NormalExtrudeParams,
    ) -> EditResult<Vec<Id>> {
        let plan = self.plan_extrusions(faces, params)?;
        self.next_side_id = plan.next;
        let carve = faces.first().is_some_and(|(_, d)| *d < 0.0);
        let label = if carve { UNDO_CARVE_FACE } else { UNDO_EXTRUDE_FACE };
        let solids = plan.solids;
        self.tx(label, |doc| {
            let mut sources: Vec<(Solid, Owner)> = Vec::with_capacity(solids.len());
            for id in &solids {
                let n = doc.scene.solids.get(id).ok_or(EditError::NotFound(ObjRef::Solid(*id)))?;
                sources.push((n.solid.clone(), n.owner));
            }
            let mut ids = Vec::with_capacity(plan.result.brush_count());
            let mut select: Vec<ObjRef> = Vec::new();
            let mut side_ids: IdMap<Vec<Id>> = id_map(0);
            let pick = |owner: Owner, added: &[Id], select: &mut Vec<ObjRef>| match owner {
                Owner::World => select.extend(added.iter().map(|i| ObjRef::Solid(*i))),
                Owner::Entity(e) => {
                    if !select.contains(&ObjRef::Entity(e)) {
                        select.push(ObjRef::Entity(e));
                    }
                }
            };
            for (b, piece) in plan.result.changed {
                let (template, owner) = &sources[b];
                let id = solids[b];
                record_side_ids(&template.sides, &[&piece], &mut side_ids);
                let mut solid = template.clone();
                solid.sides = piece.sides;
                doc.update_solid(id, solid);
                pick(*owner, &[id], &mut select);
                ids.push(id);
            }
            for (b, sides) in plan.result.added {
                let (template, owner) = &sources[b];
                let id = doc.alloc_object_id();
                let added = doc.add_solids(vec![(derived_solid(template, id, sides), *owner)]);
                pick(*owner, &added, &mut select);
                ids.extend(added);
            }
            doc.remap_side_refs(&side_ids);
            doc.deselect_missing();
            if !select.is_empty() {
                doc.select(&select, SelectOp::Replace);
            }
            Ok(ids)
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use glam::DVec3;
    use hp_geom::{ExtrudeMode, FaceExtrudeParams, Plane};

    use crate::tests::{box_sides, check_undo_redo, vmf};
    use crate::{Document, FaceRef, ObjRef, Owner};

    fn front(doc: &Document, id: u32) -> FaceRef {
        let n = doc.scene().solid(id).expect("solid");
        let i = n.solid.sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == DVec3::NEG_Y));
        FaceRef::new(id, i.expect("front face"))
    }

    fn window() -> Vec<DVec3> {
        [(64.0, 32.0), (192.0, 32.0), (192.0, 96.0), (64.0, 96.0)].iter().map(|&(x, z)| DVec3::new(x, 0.0, z)).collect()
    }

    #[test]
    fn carving_a_window_is_one_undo_step() {
        let mut doc = Document::new_empty();
        let wall = doc
            .create_solid(box_sides(DVec3::ZERO, DVec3::new(256.0, 16.0, 128.0), "BRICK/WALL"), Owner::World)
            .expect("wall");
        doc.clear_history();
        let face = front(&doc, wall);
        let params = FaceExtrudeParams { through: true, ..Default::default() };
        let ids = check_undo_redo(&mut doc, |d| d.extrude_face(face, &window(), &params).expect("carve"));
        assert_eq!(ids.len(), 4);
        assert_eq!(ids[0], wall, "the first piece keeps the brush id");
        assert_eq!(doc.history_len().0, 1);
        assert_eq!(doc.undo_label(), Some("Carve face"));
        assert_eq!(doc.selected_solid_ids().len(), 4);
        let volume: f64 = ids.iter().map(|id| doc.scene().solid(*id).expect("piece").geom.volume()).sum();
        assert!((volume - (256.0 * 16.0 * 128.0 - 128.0 * 16.0 * 64.0)).abs() < 1e-6);
        for id in &ids {
            let n = doc.scene().solid(*id).expect("piece");
            assert!(n.geom.valid);
            assert!(n.solid.sides.iter().all(|s| s.material == "BRICK/WALL"));
        }
        let before = vmf(&doc);
        assert!(doc.extrude_face(FaceRef::new(wall, 99), &window(), &params).is_err());
        assert!(doc.extrude_face(FaceRef::new(9999, 0), &window(), &params).is_err());
        assert_eq!(vmf(&doc), before, "failed requests change nothing");
    }

    #[test]
    fn extruding_adds_a_brush_in_the_same_entity() {
        let mut doc = Document::new_empty();
        let e = doc
            .create_brush_entity(
                "func_detail",
                &[],
                vec![box_sides(DVec3::ZERO, DVec3::new(256.0, 16.0, 128.0), "BRICK/WALL")],
            )
            .expect("entity");
        let wall = doc.scene().entity_solid_ids(e)[0];
        let face = front(&doc, wall);
        let params = FaceExtrudeParams {
            mode: ExtrudeMode::Extrude,
            depth: 32.0,
            material: Some("DEV/TRIM".to_owned()),
            ..Default::default()
        };
        let ids = doc.extrude_face(face, &window(), &params).expect("extrude");
        assert_eq!(ids.len(), 1);
        assert_ne!(ids[0], wall);
        assert_eq!(doc.undo_label(), Some("Extrude face"));
        assert_eq!(doc.scene().entity_solid_ids(e).len(), 2);
        assert!(doc.selection().objects.contains(&ObjRef::Entity(e)) && doc.selection().objects.len() == 1);
        let n = doc.scene().solid(ids[0]).expect("new");
        assert!(n.solid.sides.iter().all(|s| s.material == "DEV/TRIM"));
        let whole: Vec<DVec3> = [(0.0, 0.0), (256.0, 0.0), (256.0, 128.0), (0.0, 128.0)]
            .iter()
            .map(|&(x, z)| DVec3::new(x, 0.0, z))
            .collect();
        let face = front(&doc, wall);
        let ids = doc.extrude_face(face, &whole, &params).expect("extend");
        assert_eq!(ids, vec![wall], "the brush grows instead");
        assert_eq!(doc.scene().solid(wall).expect("wall").bounds.min.y, -32.0);
    }

    #[test]
    fn extruding_faces_makes_new_brushes_in_one_undo_step() {
        let mut doc = Document::new_empty();
        let a = doc.create_solid(box_sides(DVec3::ZERO, DVec3::new(64.0, 64.0, 32.0), "A"), Owner::World).expect("a");
        let b = doc
            .create_solid(box_sides(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 32.0), "B"), Owner::World)
            .expect("b");
        doc.clear_history();
        let top = |doc: &Document, id: u32| {
            let n = doc.scene().solid(id).expect("solid");
            let i = n.solid.sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == DVec3::Z));
            FaceRef::new(id, i.expect("top"))
        };
        let faces = [(top(&doc, a), 16.0), (top(&doc, b), 24.0)];
        let ids = check_undo_redo(&mut doc, |d| d.extrude_faces(&faces, None).expect("extrude"));
        assert_eq!(ids.len(), 2);
        assert_eq!(doc.history_len().0, 1);
        assert_eq!(doc.undo_label(), Some("Extrude face"));
        assert_eq!(doc.selected_solid_ids().len(), 2);
        let volume: f64 = ids.iter().map(|id| doc.scene().solid(*id).expect("new").geom.volume()).sum();
        assert!((volume - (64.0 * 64.0 * 16.0 + 64.0 * 64.0 * 24.0)).abs() < 1e-6);
        assert!(doc.scene().solid(a).expect("a").geom.volume() > 0.0);
        assert_eq!(doc.extrude_faces_preview(&faces, None).expect("preview"), 2);
        let before = vmf(&doc);
        assert!(doc.extrude_faces(&[], None).is_err());
        assert!(doc.extrude_faces(&[(faces[0].0, 0.0)], None).is_err());
        assert!(doc.extrude_faces(&[(FaceRef::new(a, 99), 8.0)], None).is_err());
        assert_eq!(vmf(&doc), before);
    }

    #[test]
    fn negative_depth_carves_into_the_brush() {
        let mut doc = Document::new_empty();
        let block = doc
            .create_solid(box_sides(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0), "BLOCK"), Owner::World)
            .expect("block");
        doc.clear_history();
        let face_with = |doc: &Document, normal: DVec3| {
            let n = doc.scene().solid(block).expect("solid");
            let i = n.solid.sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == normal));
            FaceRef::new(block, i.expect("face"))
        };
        let top = face_with(&doc, DVec3::Z);
        let ids = check_undo_redo(&mut doc, |d| d.extrude_faces(&[(top, -16.0)], None).expect("carve"));
        assert_eq!(ids, vec![block]);
        assert_eq!(doc.undo_label(), Some("Carve face"));
        let n = doc.scene().solid(block).expect("block");
        assert!(n.geom.valid);
        assert!((n.geom.volume() - 128.0 * 128.0 * 48.0).abs() < 1e-6);
        assert_eq!(n.bounds.max.z, 48.0);
        assert!(doc.selection().objects.contains(&ObjRef::Solid(block)));
        let top = face_with(&doc, DVec3::Z);
        let right = face_with(&doc, DVec3::X);
        let ids = check_undo_redo(&mut doc, |d| d.extrude_faces(&[(top, -8.0), (right, -32.0)], None).expect("two"));
        assert_eq!(ids.len(), 1);
        let n = doc.scene().solid(block).expect("block");
        assert!(n.geom.valid);
        assert_eq!(n.bounds.max.z, 40.0);
        assert_eq!(n.bounds.max.x, 96.0);
        assert_eq!(doc.extrude_faces_preview(&[(top, -8.0)], None).expect("preview"), 1);
        let before = vmf(&doc);
        assert!(doc.extrude_faces(&[(face_with(&doc, DVec3::Z), -500.0)], None).is_err());
        assert!(doc.extrude_faces(&[(face_with(&doc, DVec3::Z), -0.01)], None).is_err());
        assert_eq!(vmf(&doc), before, "a carve through the brush changes nothing");
    }

    #[test]
    fn growing_and_average_extrusions_on_several_brushes() {
        use hp_geom::{ExtrudeDirection, NormalExtrudeParams};
        let mut doc = Document::new_empty();
        let a = doc.create_solid(box_sides(DVec3::ZERO, DVec3::new(64.0, 64.0, 32.0), "A"), Owner::World).expect("a");
        let b = doc
            .create_solid(box_sides(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 32.0), "B"), Owner::World)
            .expect("b");
        doc.clear_history();
        let side = |doc: &Document, id: u32, normal: DVec3| {
            let n = doc.scene().solid(id).expect("solid");
            let i = n.solid.sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == normal));
            FaceRef::new(id, i.expect("face"))
        };
        let top_a = side(&doc, a, DVec3::Z);
        let top_id = doc.scene().solid(a).expect("a").solid.sides[top_a.side].id;
        let faces = [(top_a, 16.0), (side(&doc, b, DVec3::Z), 16.0), (side(&doc, b, DVec3::X), 16.0)];
        let grow = NormalExtrudeParams { grow: true, ..Default::default() };
        let preview = doc.extrude_faces_plan(&faces, &grow).expect("preview");
        assert_eq!(preview.brushes, 2);
        assert!(!preview.outlines.is_empty());
        let before = doc.scene().solids().len();
        let ids = check_undo_redo(&mut doc, |d| d.extrude_faces_with(&faces, &grow).expect("grow"));
        assert_eq!(ids, vec![a, b], "both brushes grow, the wall between the top and the side joins b");
        assert_eq!(doc.history_len().0, 1);
        assert_eq!(doc.scene().solids().len(), before);
        assert_eq!(doc.scene().solid(a).expect("a").bounds.max.z, 48.0);
        let nb = doc.scene().solid(b).expect("b");
        assert!((nb.geom.volume() - (64.0 * 64.0 * 48.0 + 32.0 * 64.0 * 16.0 + 0.5 * 16.0 * 16.0 * 64.0)).abs() < 1e-3);
        assert!(doc.selection().faces.is_empty() || doc.selected_solid_ids().len() == 2);
        let moved = side(&doc, a, DVec3::Z);
        assert_eq!(doc.scene().solid(a).expect("a").solid.sides[moved.side].id, top_id, "the moved face keeps its id");
        let avg = NormalExtrudeParams { direction: ExtrudeDirection::Average, ..Default::default() };
        let pair = [(side(&doc, b, DVec3::Z), 8.0), (side(&doc, b, DVec3::X), 8.0)];
        let ids = doc.extrude_faces_with(&pair, &avg).expect("average");
        assert!(!ids.is_empty() && !ids.contains(&b));
        let mixed = [(side(&doc, a, DVec3::Z), 8.0), (side(&doc, a, DVec3::X), -8.0)];
        let text = vmf(&doc);
        assert!(doc.extrude_faces_with(&mixed, &grow).is_err());
        assert_eq!(vmf(&doc), text);
    }
}
