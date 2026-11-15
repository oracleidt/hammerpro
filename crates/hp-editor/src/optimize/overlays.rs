use hp_geom::Aabb;

use super::{Finding, Impact, Topic};
use crate::ids::id_set;
use crate::side_index::format_side_list;
use crate::side_index::overlay::{OverlayShape, fragments, is_overlay_class};
use crate::{Document, EditError, EditResult, FaceRef, Id, IdMap, ObjRef};

pub const MANY_FACES: usize = 16;
pub const MAX_FACES: usize = crate::limits::checks::MAX_OVERLAY_FACES;
pub const UNUSED_FACES: usize = 4;
pub const TRIM_LABEL: &str = "Trim overlay faces";

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayFaces {
    pub id: Id,
    pub material: String,
    pub listed: usize,
    pub touched: Vec<Id>,
    pub untouched: Vec<Id>,
    pub missing: Vec<Id>,
    pub bounds: Aabb,
}

impl OverlayFaces {
    pub fn trimmed(&self) -> Option<Vec<Id>> {
        if self.untouched.is_empty() || self.touched.is_empty() {
            return None;
        }
        let keep: Vec<Id> = self.touched.iter().chain(&self.missing).copied().collect();
        Some(keep)
    }
}

fn survey_ids(doc: &Document, ids: &[Id]) -> Vec<OverlayFaces> {
    let scene = doc.scene();
    let shapes: Vec<(Id, OverlayShape)> = ids
        .iter()
        .filter_map(|id| {
            let e = &scene.entity(*id)?.entity;
            if !is_overlay_class(&e.classname) {
                return None;
            }
            Some((*id, OverlayShape::from_entity(e)?))
        })
        .collect();
    let mut wanted = id_set(0);
    for (_, s) in &shapes {
        wanted.extend(s.sides.iter().copied());
    }
    let faces: IdMap<FaceRef> = doc.find_sides(&wanted);
    shapes
        .into_iter()
        .map(|(id, shape)| {
            let lookup = |s: Id| faces.get(&s).copied();
            let hit: std::collections::HashSet<Id> =
                fragments(doc, &shape, &lookup).into_iter().map(|f| f.side_id).collect();
            let mut out = OverlayFaces {
                id,
                material: shape.material.clone(),
                listed: shape.sides.len(),
                touched: Vec::new(),
                untouched: Vec::new(),
                missing: Vec::new(),
                bounds: shape.bounds(),
            };
            for s in &shape.sides {
                if hit.contains(s) {
                    out.touched.push(*s);
                } else if faces.contains_key(s) {
                    out.untouched.push(*s);
                } else {
                    out.missing.push(*s);
                }
            }
            out
        })
        .collect()
}

pub fn survey(doc: &Document) -> Vec<OverlayFaces> {
    let mut ids: Vec<Id> = doc
        .scene()
        .entities()
        .iter()
        .filter(|(_, n)| is_overlay_class(&n.entity.classname))
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    survey_ids(doc, &ids)
}

pub fn overlay_faces(doc: &Document, id: Id) -> Option<OverlayFaces> {
    survey_ids(doc, &[id]).into_iter().next()
}

pub fn finding(o: &OverlayFaces) -> Option<Finding> {
    let n = o.listed;
    let unused = o.untouched.len();
    let (code, impact, message) = if n > MAX_FACES {
        (
            "overlay_over_limit",
            Impact::High,
            format!(
                "Overlay {} ({}) is applied to {n} brush faces, the engine allows {MAX_FACES}: the compile fails or \
                 drops it. {unused} of them are outside its rectangle: run Trim overlay faces on it, or split it.",
                o.id, o.material
            ),
        )
    } else if n > MANY_FACES {
        (
            "overlay_many_faces",
            Impact::Medium,
            format!(
                "Overlay {} ({}) is applied to {n} brush faces: every face is a separate fragment to clip, store and \
                 draw. {unused} of them are outside its rectangle: run Trim overlay faces on it.",
                o.id, o.material
            ),
        )
    } else if unused >= UNUSED_FACES {
        (
            "overlay_unused_faces",
            Impact::Low,
            format!(
                "Overlay {} ({}) lists {unused} of its {n} brush faces that its rectangle does not touch: they only \
                 count toward the face limit. Run Trim overlay faces on it.",
                o.id, o.material
            ),
        )
    } else {
        return None;
    };
    let score = n as f64 * 10.0 + unused as f64;
    let mut f = Finding::new(Topic::Overlays, code, impact, message)
        .score(score)
        .objects(vec![ObjRef::Entity(o.id)])
        .args(vec![o.id.to_string(), n.to_string(), unused.to_string(), o.material.clone(), MAX_FACES.to_string()]);
    if !o.bounds.is_empty() {
        f = f.boxes(vec![o.bounds]);
    }
    if o.trimmed().is_some() {
        f = f.fix("trim_overlay_faces");
    }
    Some(f)
}

pub fn findings(doc: &Document) -> Vec<Finding> {
    survey(doc).iter().filter_map(finding).collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrimReport {
    pub overlays: Vec<(Id, usize, usize)>,
    pub removed: usize,
}

impl Document {
    pub fn trim_overlay_faces(&mut self, ids: &[Id]) -> EditResult<TrimReport> {
        let mut plan: Vec<(Id, Vec<Id>, usize)> = Vec::new();
        let mut any_overlay = false;
        for o in survey_ids(self, ids) {
            any_overlay = true;
            if let Some(keep) = o.trimmed() {
                plan.push((o.id, keep, o.untouched.len()));
            }
        }
        if !any_overlay {
            return Err(EditError::Invalid("no overlay (info_overlay) given".to_owned()));
        }
        let mut report = TrimReport::default();
        if plan.is_empty() {
            return Ok(report);
        }
        self.tx(TRIM_LABEL, |doc| {
            for (id, keep, removed) in &plan {
                let list = format_side_list(keep);
                doc.update_entity(*id, |e| e.set("sides", list));
                report.overlays.push((*id, keep.len(), *removed));
                report.removed += removed;
            }
            Ok(())
        })?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::*;
    use crate::Owner;
    use crate::tests::box_sides;

    #[test]
    fn overlay_on_far_faces_is_reported_and_trimmed_in_one_step() {
        let mut doc = Document::new_empty();
        let mut tops = Vec::new();
        for i in 0..20 {
            let x = f64::from(i) * 256.0;
            let id = doc
                .create_solid(
                    box_sides(DVec3::new(x, 0.0, -16.0), DVec3::new(x + 256.0, 256.0, 0.0), "DEV/X"),
                    Owner::World,
                )
                .expect("floor");
            let node = doc.scene().solid(id).expect("solid");
            let top = (0..node.solid.sides.len())
                .find(|i| node.geom.face_for_side(*i).is_some_and(|f| f.plane.normal.z > 0.9))
                .map(|i| node.solid.sides[i].id)
                .expect("top");
            tops.push(top);
        }
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let sides = format_side_list(&tops);
        let ov = doc
            .create_point_entity(
                "info_overlay",
                DVec3::new(128.0, 128.0, 0.0),
                &[
                    kv("material", "overlays/road01"),
                    kv("sides", &sides),
                    kv("BasisOrigin", "128 128 0"),
                    kv("BasisNormal", "0 0 1"),
                    kv("BasisU", "1 0 0"),
                    kv("BasisV", "0 1 0"),
                    kv("uv0", "-64 -64 0"),
                    kv("uv1", "-64 64 0"),
                    kv("uv2", "64 64 0"),
                    kv("uv3", "64 -64 0"),
                ],
            )
            .expect("overlay");
        let o = overlay_faces(&doc, ov).expect("survey");
        assert_eq!((o.listed, o.touched.len(), o.untouched.len()), (20, 1, 19));
        let f = findings(&doc);
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].code, f[0].impact, f[0].topic), ("overlay_many_faces", Impact::Medium, Topic::Overlays));
        assert_eq!(f[0].objects, vec![ObjRef::Entity(ov)]);
        assert_eq!(f[0].fix, Some("trim_overlay_faces"));
        let before = doc.scene_revision();
        let r = doc.trim_overlay_faces(&[ov]).expect("trim");
        assert_eq!(r.removed, 19);
        assert_ne!(doc.scene_revision(), before);
        assert_eq!(doc.scene().entity(ov).unwrap().entity.get("sides"), Some(tops[0].to_string().as_str()));
        assert!(findings(&doc).is_empty());
        assert_eq!(doc.trim_overlay_faces(&[ov]).expect("again").removed, 0);
        assert!(doc.undo());
        assert_eq!(doc.scene().entity(ov).unwrap().entity.get("sides"), Some(sides.as_str()));
        assert!(doc.trim_overlay_faces(&[999_999]).is_err());
    }
}
