use glam::DVec3;
use hp_geom::{ClipPiece, ExtrudeMode, FaceExtrudeParams, face_extrude};
use hp_vmf::Solid;

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
}
