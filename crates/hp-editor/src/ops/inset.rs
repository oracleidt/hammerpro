use hp_geom::{ClipPiece, InsetParams, face_inset};
use hp_vmf::{Side, Solid};

use super::csg::{derived_solid, record_side_ids};
use crate::document::Document;
use crate::ids::{IdMap, id_map, id_set};
use crate::scene::Owner;
use crate::selection::{FaceRef, ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

const UNDO_INSET_FACE: &str = "Inset face";

struct Job {
    solid: Id,
    owner: Owner,
    original: Vec<Side>,
    pieces: Vec<ClipPiece>,
}

impl Document {
    pub fn inset_faces(&mut self, faces: &[FaceRef], params: &InsetParams) -> EditResult<Vec<Id>> {
        if faces.is_empty() {
            return Err(EditError::Nothing("no face to inset"));
        }
        let mut seen = id_set(faces.len());
        for f in faces {
            if !seen.insert(f.solid) {
                return Err(EditError::Invalid(format!("brush {} has more than one face to inset", f.solid)));
            }
        }
        let mut next = self.next_side_id;
        let mut jobs = Vec::with_capacity(faces.len());
        for f in faces {
            let n = self.scene.solids.get(&f.solid).ok_or(EditError::NotFound(ObjRef::Solid(f.solid)))?;
            if f.side >= n.solid.sides.len() {
                return Err(EditError::Invalid(format!("solid {} has no side {}", f.solid, f.side)));
            }
            let original = n.solid.sides.clone();
            let pieces = face_inset(&original, f.side, params, &mut || {
                let id = next;
                next = next.saturating_add(1);
                id
            })?;
            jobs.push(Job { solid: f.solid, owner: n.owner, original, pieces });
        }
        self.next_side_id = next;
        self.tx(UNDO_INSET_FACE, |doc| {
            let mut ids = Vec::new();
            let mut side_ids: IdMap<Vec<Id>> = id_map(0);
            let mut entities = Vec::new();
            for job in jobs {
                let template: Solid = doc
                    .scene
                    .solids
                    .get(&job.solid)
                    .map(|n| n.solid.clone())
                    .ok_or(EditError::NotFound(ObjRef::Solid(job.solid)))?;
                let refs: Vec<&ClipPiece> = job.pieces.iter().collect();
                record_side_ids(&job.original, &refs, &mut side_ids);
                let mut pieces = job.pieces.into_iter();
                if let Some(first) = pieces.next() {
                    let mut s = template.clone();
                    s.sides = first.sides;
                    doc.update_solid(job.solid, s);
                    ids.push(job.solid);
                }
                let new: Vec<(Solid, Owner)> = pieces
                    .map(|p| {
                        let id = doc.alloc_object_id();
                        (derived_solid(&template, id, p.sides), job.owner)
                    })
                    .collect();
                ids.extend(doc.add_solids(new));
                if let Owner::Entity(e) = job.owner {
                    entities.push(ObjRef::Entity(e));
                }
            }
            doc.remap_side_refs(&side_ids);
            doc.deselect_missing();
            let select: Vec<ObjRef> = if entities.is_empty() {
                ids.iter().map(|id| ObjRef::Solid(*id)).collect()
            } else {
                entities.dedup();
                entities
            };
            doc.select(&select, SelectOp::Replace);
            Ok(ids)
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use glam::DVec3;
    use hp_geom::{InsetParams, Plane};

    use crate::tests::{box_sides, check_undo_redo, vmf};
    use crate::{Document, FaceRef, Owner};

    fn top(doc: &Document, id: u32) -> FaceRef {
        let n = doc.scene().solid(id).expect("solid");
        let i = n.solid.sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == DVec3::Z));
        FaceRef::new(id, i.expect("top face"))
    }

    #[test]
    fn inset_a_box_face_is_one_undo_step() {
        let mut doc = Document::new_empty();
        let b = doc
            .create_solid(box_sides(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0), "BRICK/WALL"), Owner::World)
            .expect("box");
        doc.clear_history();
        let face = top(&doc, b);
        let params = InsetParams { distance: 16.0, ..Default::default() };
        let ids = check_undo_redo(&mut doc, |d| d.inset_faces(&[face], &params).expect("inset"));
        assert_eq!(ids.len(), 5);
        assert_eq!(ids[0], b);
        assert_eq!(doc.history_len().0, 1);
        assert_eq!(doc.undo_label(), Some("Inset face"));
        assert_eq!(doc.selected_solid_ids().len(), 5);
        let volume: f64 = ids.iter().map(|id| doc.scene().solid(*id).expect("piece").geom.volume()).sum();
        assert!((volume - 128.0 * 128.0 * 64.0).abs() < 1e-6);
        assert!(ids.iter().all(|id| doc.scene().solid(*id).expect("piece").geom.valid));
    }

    #[test]
    fn failures_change_nothing() {
        let mut doc = Document::new_empty();
        let b = doc
            .create_solid(box_sides(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0), "BRICK/WALL"), Owner::World)
            .expect("box");
        let face = top(&doc, b);
        let before = vmf(&doc);
        let params = InsetParams { distance: 16.0, ..Default::default() };
        assert!(doc.inset_faces(&[], &params).is_err());
        assert!(doc.inset_faces(&[FaceRef::new(b, 99)], &params).is_err());
        assert!(doc.inset_faces(&[FaceRef::new(9999, 0)], &params).is_err());
        assert!(doc.inset_faces(&[face, FaceRef::new(b, 0)], &params).is_err());
        assert!(doc.inset_faces(&[face], &InsetParams { distance: 64.0, ..Default::default() }).is_err());
        assert_eq!(vmf(&doc), before);
    }

    #[test]
    fn raised_inset_on_two_brushes() {
        let mut doc = Document::new_empty();
        let a = doc.create_solid(box_sides(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0), "A"), Owner::World).expect("a");
        let b = doc
            .create_solid(box_sides(DVec3::new(256.0, 0.0, 0.0), DVec3::new(384.0, 128.0, 64.0), "B"), Owner::World)
            .expect("b");
        let faces = [top(&doc, a), top(&doc, b)];
        let params = InsetParams { distance: 16.0, depth: 8.0, material: None };
        let ids = doc.inset_faces(&faces, &params).expect("inset");
        assert_eq!(ids.len(), 10);
        let volume: f64 = ids.iter().map(|id| doc.scene().solid(*id).expect("piece").geom.volume()).sum();
        assert!((volume - (2.0 * 128.0 * 128.0 * 64.0 + 2.0 * 96.0 * 96.0 * 8.0)).abs() < 1e-6);
    }
}
