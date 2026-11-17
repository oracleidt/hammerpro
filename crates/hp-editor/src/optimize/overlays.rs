use glam::DVec2;
use hp_geom::Aabb;

use super::{Finding, Impact, Topic};
use crate::ids::id_set;
use crate::side_index::format_side_list;
use crate::side_index::overlay::{OverlayShape, UV_KEYS, format_uv, fragments, is_overlay_class};
use crate::{Document, EditError, EditResult, FaceRef, Id, IdMap, ObjRef};

pub const MANY_FACES: usize = 16;
pub const MAX_FACES: usize = crate::limits::checks::MAX_OVERLAY_FACES;
pub const UNUSED_FACES: usize = 1;
pub const TRIM_LABEL: &str = "Trim overlay faces";
pub const FIX_LABEL: &str = "Fix overlay faces";
pub const FIX: &str = "fix_overlay_faces";
const MAX_SPLIT_DEPTH: u32 = 12;

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

    pub fn needs_split(&self) -> bool {
        self.touched.len() > MAX_FACES
    }

    pub fn fixable(&self) -> bool {
        self.needs_split() || self.trimmed().is_some()
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
                 drops it. {unused} of them are outside its rectangle: press Fix: it trims the faces and splits it in overlays of 64 faces at most.",
                o.id, o.material
            ),
        )
    } else if n > MANY_FACES {
        (
            "overlay_many_faces",
            Impact::Medium,
            format!(
                "Overlay {} ({}) is applied to {n} brush faces: every face is a separate fragment to clip, store and \
                 draw. {unused} of them are outside its rectangle: press Fix to trim them.",
                o.id, o.material
            ),
        )
    } else if unused >= UNUSED_FACES {
        (
            "overlay_unused_faces",
            Impact::Low,
            format!(
                "Overlay {} ({}) lists {unused} of its {n} brush faces that its rectangle does not touch: they only \
                 count toward the face limit. Press Fix to trim them.",
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
    if o.fixable() {
        f = f.fix(FIX);
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

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayFix {
    pub id: Id,
    pub before: usize,
    pub after: usize,
    pub pieces: Vec<Id>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FixReport {
    pub overlays: Vec<OverlayFix>,
    pub faces_before: usize,
    pub faces_after: usize,
    pub created: usize,
}

fn bilinear(uv: &[DVec2; 4], s: f64, t: f64) -> DVec2 {
    uv[0] * ((1.0 - s) * (1.0 - t)) + uv[3] * (s * (1.0 - t)) + uv[2] * (s * t) + uv[1] * ((1.0 - s) * t)
}

fn piece_shape(shape: &OverlayShape, s: (f64, f64), t: (f64, f64), sides: Vec<Id>) -> OverlayShape {
    let uv = [
        bilinear(&shape.uv, s.0, t.0),
        bilinear(&shape.uv, s.0, t.1),
        bilinear(&shape.uv, s.1, t.1),
        bilinear(&shape.uv, s.1, t.0),
    ];
    let d = shape.end - shape.start;
    OverlayShape {
        uv,
        start: shape.start + d * DVec2::new(s.0, t.0),
        end: shape.start + d * DVec2::new(s.1, t.1),
        sides,
        ..shape.clone()
    }
}

fn touched_sides(doc: &Document, shape: &OverlayShape, faces: &IdMap<FaceRef>) -> Vec<Id> {
    let lookup = |s: Id| faces.get(&s).copied();
    let hit: std::collections::HashSet<Id> = fragments(doc, shape, &lookup).into_iter().map(|f| f.side_id).collect();
    shape.sides.iter().copied().filter(|s| hit.contains(s)).collect()
}

pub fn split_shape(doc: &Document, shape: &OverlayShape) -> Vec<OverlayShape> {
    let mut wanted = id_set(shape.sides.len());
    wanted.extend(shape.sides.iter().copied());
    let faces: IdMap<FaceRef> = doc.find_sides(&wanted);
    let mut out = Vec::new();
    let mut stack = vec![((0.0, 1.0), (0.0, 1.0), shape.sides.clone(), 0u32)];
    while let Some((s, t, sides, depth)) = stack.pop() {
        let piece = piece_shape(shape, s, t, sides);
        let hit = touched_sides(doc, &piece, &faces);
        if hit.is_empty() {
            continue;
        }
        if hit.len() <= MAX_FACES || depth >= MAX_SPLIT_DEPTH {
            out.push(OverlayShape { sides: hit, ..piece });
            continue;
        }
        let q = piece.uv;
        let along_u = (q[3] - q[0]).length() + (q[2] - q[1]).length();
        let along_v = (q[1] - q[0]).length() + (q[2] - q[3]).length();
        if along_u >= along_v {
            let m = (s.0 + s.1) * 0.5;
            stack.push(((m, s.1), t, hit.clone(), depth + 1));
            stack.push(((s.0, m), t, hit, depth + 1));
        } else {
            let m = (t.0 + t.1) * 0.5;
            stack.push((s, (m, t.1), hit.clone(), depth + 1));
            stack.push((s, (t.0, m), hit, depth + 1));
        }
    }
    out
}

fn tidy(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    let r = x.round();
    if (x - r).abs() < 1e-6 { r + 0.0 } else { (x * 1e6).round() / 1e6 + 0.0 }
}

fn fmt_num(x: f64) -> String {
    let mut s = String::new();
    hp_kv::fmt_num(&mut s, tidy(x));
    s
}

fn write_piece(e: &mut hp_vmf::Entity, p: &OverlayShape) {
    let center = (p.uv[0] + p.uv[1] + p.uv[2] + p.uv[3]) * 0.25;
    let o = p.from_basis(center);
    let text = hp_vmf::fmt_vec3(glam::DVec3::new(tidy(o.x), tidy(o.y), tidy(o.z)));
    e.set("origin", text.clone());
    e.set("BasisOrigin", text);
    for (key, c) in UV_KEYS.iter().zip(p.uv) {
        e.set(key, format_uv(c - center));
    }
    e.set("StartU", fmt_num(p.start.x));
    e.set("EndU", fmt_num(p.end.x));
    e.set("StartV", fmt_num(p.start.y));
    e.set("EndV", fmt_num(p.end.y));
    e.set("sides", format_side_list(&p.sides));
}

impl Document {
    pub fn fix_overlay_faces(&mut self, ids: &[Id]) -> EditResult<FixReport> {
        let mut any_overlay = false;
        let mut trims: Vec<(Id, Vec<Id>, usize)> = Vec::new();
        let mut splits: Vec<(Id, usize, Vec<OverlayShape>)> = Vec::new();
        for o in survey_ids(self, ids) {
            any_overlay = true;
            if o.needs_split() {
                let Some(node) = self.scene().entity(o.id) else { continue };
                let Some(shape) = OverlayShape::from_entity(&node.entity) else { continue };
                let shape = OverlayShape { sides: o.touched.clone(), ..shape };
                let pieces = split_shape(self, &shape);
                if !pieces.is_empty() {
                    splits.push((o.id, o.listed, pieces));
                }
            } else if let Some(keep) = o.trimmed() {
                trims.push((o.id, keep, o.listed));
            }
        }
        if !any_overlay {
            return Err(EditError::Invalid("no overlay (info_overlay) given".to_owned()));
        }
        let mut report = FixReport::default();
        if trims.is_empty() && splits.is_empty() {
            return Ok(report);
        }
        self.tx(FIX_LABEL, |doc| {
            for (id, keep, before) in &trims {
                let list = format_side_list(keep);
                doc.update_entity(*id, |e| e.set("sides", list));
                report.overlays.push(OverlayFix { id: *id, before: *before, after: keep.len(), pieces: Vec::new() });
            }
            for (id, before, pieces) in &splits {
                let Some(base) = doc.scene().entity(*id).map(|n| n.entity.clone()) else { continue };
                let mut created = Vec::new();
                for (i, p) in pieces.iter().enumerate() {
                    if i == 0 {
                        doc.update_entity(*id, |e| write_piece(e, p));
                    } else {
                        let mut e = base.clone();
                        e.id = doc.alloc_object_id();
                        write_piece(&mut e, p);
                        created.push(doc.add_entity(e));
                    }
                }
                let after = pieces.iter().map(|p| p.sides.len()).sum();
                report.created += created.len();
                report.overlays.push(OverlayFix { id: *id, before: *before, after, pieces: created });
            }
            Ok(())
        })?;
        report.faces_before = report.overlays.iter().map(|o| o.before).sum();
        report.faces_after = report.overlays.iter().map(|o| o.after).sum();
        Ok(report)
    }

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
        assert_eq!(f[0].fix, Some(FIX));
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

    fn floor_tiles(doc: &mut Document, nx: i32, ny: i32, size: f64) -> Vec<Id> {
        let mut tops = Vec::new();
        for i in 0..nx {
            for j in 0..ny {
                let (x, y) = (f64::from(i) * size, f64::from(j) * size);
                let id = doc
                    .create_solid(
                        box_sides(DVec3::new(x, y, -16.0), DVec3::new(x + size, y + size, 0.0), "DEV/X"),
                        Owner::World,
                    )
                    .expect("tile");
                let node = doc.scene().solid(id).expect("solid");
                let top = (0..node.solid.sides.len())
                    .find(|i| node.geom.face_for_side(*i).is_some_and(|f| f.plane.normal.z > 0.9))
                    .map(|i| node.solid.sides[i].id)
                    .expect("top");
                tops.push(top);
            }
        }
        tops
    }

    #[test]
    fn overlay_over_the_limit_is_split_with_continuous_texture_in_one_step() {
        let mut doc = Document::new_empty();
        let tops = floor_tiles(&mut doc, 12, 8, 32.0);
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let sides = format_side_list(&tops);
        let ov = doc
            .create_point_entity(
                "info_overlay",
                DVec3::new(192.0, 128.0, 0.0),
                &[
                    kv("material", "overlays/road01"),
                    kv("targetname", "road"),
                    kv("sides", &sides),
                    kv("StartU", "0"),
                    kv("EndU", "2"),
                    kv("StartV", "0"),
                    kv("EndV", "1"),
                    kv("BasisOrigin", "192 128 0"),
                    kv("BasisNormal", "0 0 1"),
                    kv("BasisU", "1 0 0"),
                    kv("BasisV", "0 1 0"),
                    kv("uv0", "-180 -120 0"),
                    kv("uv1", "-180 120 0"),
                    kv("uv2", "180 120 0"),
                    kv("uv3", "180 -120 0"),
                ],
            )
            .expect("overlay");
        let o = overlay_faces(&doc, ov).expect("survey");
        assert!(o.touched.len() > MAX_FACES, "{}", o.touched.len());
        assert!(o.needs_split() && o.fixable());
        let f = findings(&doc);
        assert_eq!(f[0].code, "overlay_over_limit");
        assert_eq!(f[0].fix, Some(FIX));
        let count = doc.scene().entities().len();
        let undo = doc.history_len().0;
        let r = doc.fix_overlay_faces(&[ov]).expect("fix");
        assert_eq!(doc.history_len().0, undo + 1, "one undo step");
        assert_eq!(doc.undo_label(), Some(FIX_LABEL));
        assert_eq!(r.overlays.len(), 1);
        assert_eq!(r.faces_before, tops.len());
        assert!(r.created >= 1);
        assert_eq!(doc.scene().entities().len(), count + r.created);
        let mut pieces = vec![ov];
        pieces.extend(&r.overlays[0].pieces);
        let mut covered = std::collections::HashSet::new();
        let mut area = 0.0;
        for id in &pieces {
            let e = &doc.scene().entity(*id).expect("piece").entity;
            assert_eq!(e.get("targetname"), Some("road"));
            assert_eq!(e.get("material"), Some("overlays/road01"));
            let shape = OverlayShape::from_entity(e).expect("shape");
            assert!(shape.sides.len() <= MAX_FACES);
            covered.extend(shape.sides.iter().copied());
            let size = shape.uv[2] - shape.uv[0];
            area += size.x * size.y;
            for c in shape.uv {
                let world = shape.from_basis(c);
                let full = DVec2::new((world.x - 12.0) / 360.0 * 2.0, (world.y - 8.0) / 240.0);
                let tex = shape.texcoord(shape.to_basis(world));
                assert!((tex - full).length() < 1e-6, "texture continues across pieces: {tex} {full}");
            }
        }
        assert!((area - 360.0 * 240.0).abs() < 1e-6, "pieces tile the rectangle: {area}");
        assert_eq!(covered.len(), o.touched.len());
        assert!(findings(&doc).iter().all(|f| f.code != "overlay_over_limit"));
        assert!(doc.undo());
        assert_eq!(doc.scene().entities().len(), count);
        assert_eq!(doc.scene().entity(ov).unwrap().entity.get("sides"), Some(sides.as_str()));
    }

    #[test]
    fn fix_trims_unused_faces_and_reports_before_after() {
        let mut doc = Document::new_empty();
        let tops = floor_tiles(&mut doc, 3, 1, 256.0);
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let ov = doc
            .create_point_entity(
                "info_overlay",
                DVec3::new(128.0, 128.0, 0.0),
                &[
                    kv("material", "overlays/road01"),
                    kv("sides", &format_side_list(&tops)),
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
        let f = findings(&doc);
        assert_eq!((f.len(), f[0].code, f[0].fix), (1, "overlay_unused_faces", Some(FIX)));
        let r = doc.fix_overlay_faces(&[ov]).expect("fix");
        assert_eq!((r.faces_before, r.faces_after, r.created), (3, 1, 0));
        assert!(findings(&doc).is_empty());
        assert!(doc.fix_overlay_faces(&[ov]).expect("again").overlays.is_empty());
        assert!(doc.fix_overlay_faces(&[999_999]).is_err());
    }
}
