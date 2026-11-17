use glam::{DMat3, DMat4, DVec2, DVec3};
use hp_geom::Aabb;
use hp_vmf::Entity;

use super::{clean_vec, is_conformal};
use crate::EditResult;
use crate::Id;
use crate::clipboard::{format_side_list, parse_side_list};
use crate::document::Document;
use crate::ids::IdSet;
use crate::selection::ObjRef;

const UV_KEYS: [&str; 4] = ["uv0", "uv1", "uv2", "uv3"];
const COPLANAR_DOT: f64 = 0.999;
const COPLANAR_DIST: f64 = 0.5;
const OVERLAP_EPS: f64 = 0.01;
const MAX_FACES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OverlayBasis {
    pub origin: DVec3,
    pub u: DVec3,
    pub v: DVec3,
    pub normal: DVec3,
    pub uv: [DVec3; 4],
}

impl OverlayBasis {
    pub(crate) fn read(e: &Entity) -> Option<Self> {
        let get = |k: &str| e.get(k).and_then(hp_vmf::parse_vec3).filter(|v| v.is_finite());
        let origin = get("BasisOrigin")?;
        let normal = get("BasisNormal")?.try_normalize()?;
        let u = get("BasisU")?.try_normalize()?;
        let v = get("BasisV")?.try_normalize()?;
        let mut uv = [DVec3::ZERO; 4];
        for (slot, key) in uv.iter_mut().zip(UV_KEYS) {
            *slot = get(key).unwrap_or(DVec3::ZERO);
        }
        Some(Self { origin, u, v, normal, uv })
    }

    pub(crate) fn corners(&self) -> [DVec3; 4] {
        self.uv.map(|c| self.origin + self.u * c.x + self.v * c.y)
    }

    fn plane_point(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.u), d.dot(self.v))
    }

    fn write(&self, old: &Self, e: &mut Entity) -> bool {
        let mut changed = false;
        let mut put = |key: &str, new: DVec3, old: DVec3| {
            let new = clean_vec(new);
            if new != old && new.is_finite() {
                e.set(key, hp_vmf::fmt_vec3(new));
                changed = true;
            }
        };
        put("BasisOrigin", self.origin, old.origin);
        put("BasisU", self.u, old.u);
        put("BasisV", self.v, old.v);
        put("BasisNormal", self.normal, old.normal);
        for (i, key) in UV_KEYS.iter().enumerate() {
            put(key, self.uv[i], old.uv[i]);
        }
        changed
    }
}

pub(crate) fn is_overlay(e: &Entity) -> bool {
    e.get("BasisOrigin").is_some() && e.get("BasisNormal").is_some()
}

fn any_perpendicular(n: DVec3) -> DVec3 {
    let a = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    (a - n * n.dot(a)).try_normalize().unwrap_or(DVec3::X)
}

pub(crate) fn transformed_basis(b: &OverlayBasis, m: &DMat4, scale_lock: bool) -> Option<OverlayBasis> {
    let l = DMat3::from_mat4(*m);
    let det = l.determinant();
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let origin = m.transform_point3(b.origin);
    let normal = (l.inverse().transpose() * b.normal).try_normalize().unwrap_or(b.normal);
    let lu = l * b.u;
    let u = (lu - normal * normal.dot(lu)).try_normalize().unwrap_or_else(|| any_perpendicular(normal));
    let lv = l * b.v;
    let v = (lv - normal * normal.dot(lv) - u * u.dot(lv)).try_normalize().unwrap_or_else(|| {
        let c = normal.cross(u);
        if c.dot(lv) < 0.0 { -c } else { c }
    });
    let keep_size = !scale_lock && is_conformal(&l);
    let factor = if keep_size { 1.0 / (l.x_axis.length()).max(1e-12) } else { 1.0 };
    let mut uv = b.uv;
    for (out, c) in uv.iter_mut().zip(b.uv) {
        let w = l * (b.u * c.x + b.v * c.y);
        *out = DVec3::new(w.dot(u) * factor, w.dot(v) * factor, c.z);
    }
    let out = OverlayBasis { origin, u, v, normal, uv };
    let finite = out.origin.is_finite()
        && out.u.is_finite()
        && out.v.is_finite()
        && out.normal.is_finite()
        && out.uv.iter().all(|c| c.is_finite());
    finite.then_some(out)
}

pub(crate) fn transform_overlay(e: &mut Entity, m: &DMat4, translation_only: bool, scale_lock: bool) -> bool {
    let Some(b) = OverlayBasis::read(e) else { return false };
    let new = if translation_only {
        OverlayBasis { origin: b.origin + m.w_axis.truncate(), ..b }
    } else {
        match transformed_basis(&b, m, scale_lock) {
            Some(n) => n,
            None => return false,
        }
    };
    new.write(&b, e)
}

fn convex_overlap(a: &[DVec2], b: &[DVec2]) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let separated_on = |axis: DVec2| {
        let Some(axis) = axis.try_normalize() else { return false };
        let span = |pts: &[DVec2]| {
            pts.iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(axis.dot(*p)), hi.max(axis.dot(*p))))
        };
        let (a0, a1) = span(a);
        let (b0, b1) = span(b);
        !(a1 > b0 + OVERLAP_EPS && b1 > a0 + OVERLAP_EPS)
    };
    for poly in [a, b] {
        let n = poly.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            if separated_on((q - p).perp()) {
                return false;
            }
        }
        if n == 2 && separated_on(poly[1] - poly[0]) {
            return false;
        }
    }
    if a.len() < 2 && b.len() < 2 {
        return a[0].distance(b[0]) <= OVERLAP_EPS;
    }
    true
}

impl Document {
    pub(crate) fn fix_overlay_sides(&mut self, overlays: &[Id], moved: &IdSet) {
        let mut updates: Vec<(Id, String)> = Vec::new();
        for id in overlays {
            let Some(node) = self.scene.entities.get(id) else { continue };
            let Some(b) = OverlayBasis::read(&node.entity) else { continue };
            let listed = parse_side_list(node.entity.get("sides").unwrap_or(""));
            if let Some(faces) = self.overlay_faces(&b, &listed, moved) {
                updates.push((*id, format_side_list(&faces)));
            }
        }
        for (id, sides) in updates {
            self.update_entity(id, |e| e.set("sides", sides));
        }
    }

    pub fn refit_overlay_faces(&mut self, overlays: &[Id]) -> EditResult<()> {
        let overlays: Vec<Id> = overlays
            .iter()
            .copied()
            .filter(|id| self.scene.entities.get(id).is_some_and(|n| is_overlay(&n.entity)))
            .collect();
        if overlays.is_empty() {
            return Ok(());
        }
        self.tx("Edit overlay", |doc| {
            doc.fix_overlay_sides(&overlays, &crate::ids::id_set(0));
            Ok(())
        })
    }

    pub(crate) fn attach_pasted_overlays(&mut self, objs: &[ObjRef]) {
        let overlays: Vec<Id> = objs
            .iter()
            .filter_map(|o| match *o {
                ObjRef::Entity(id) => self.scene.entities.get(&id).filter(|n| is_overlay(&n.entity)).map(|_| id),
                ObjRef::Solid(_) => None,
            })
            .collect();
        if overlays.is_empty() {
            return;
        }
        let pasted: IdSet = self.scene.expand_solids(objs).into_iter().collect();
        self.fix_overlay_sides(&overlays, &pasted);
    }

    fn overlay_faces(&self, b: &OverlayBasis, listed: &[Id], moved: &IdSet) -> Option<Vec<Id>> {
        let corners = b.corners();
        let bounds = Aabb::from_points(corners.iter().copied().chain([b.origin]));
        if bounds.is_empty() || !bounds.min.is_finite() || !bounds.max.is_finite() {
            return None;
        }
        let quad = crate::pick::hull_2d(corners.iter().map(|c| b.plane_point(*c)).collect());
        let quad = if quad.is_empty() { vec![DVec2::ZERO] } else { quad };
        let plane_dist = b.normal.dot(b.origin);
        let mut found: Vec<(Id, bool)> = Vec::new();
        'solids: for sid in self.solids_in_box(&bounds.expand(1.0)) {
            let Some(n) = self.scene.solids.get(&sid) else { continue };
            for f in &n.geom.faces {
                let Some(side) = n.solid.sides.get(f.side_index) else { continue };
                let dot = f.plane.normal.dot(b.normal);
                let under = match n.disp.get(f.side_index).and_then(Option::as_ref) {
                    Some(mesh) => dot > 0.5 && mesh.aabb.expand(1.0).intersects(&bounds),
                    None => {
                        dot > COPLANAR_DOT && (f.plane.dist - plane_dist).abs() < COPLANAR_DIST && {
                            let poly: Vec<DVec2> = f.vertices.iter().map(|p| b.plane_point(*p)).collect();
                            convex_overlap(&quad, &poly)
                        }
                    }
                };
                if under {
                    found.push((side.id, moved.contains(&sid)));
                }
                if found.len() >= MAX_FACES {
                    break 'solids;
                }
            }
        }
        let any_moved = found.iter().any(|(_, m)| *m);
        let listed_ok = !listed.is_empty() && listed.iter().all(|s| found.iter().any(|(f, _)| f == s));
        let listed_moved = listed.iter().any(|s| found.iter().any(|(f, m)| f == s && *m));
        if listed_ok && (!any_moved || listed_moved) {
            return None;
        }
        let mut faces: Vec<Id> = found.into_iter().filter(|(_, m)| *m || !any_moved).map(|(f, _)| f).collect();
        faces.sort_unstable();
        faces.dedup();
        (!faces.is_empty() && faces != listed).then_some(faces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DQuat;

    fn overlay() -> Entity {
        let mut e = Entity { id: 50, classname: "info_overlay".into(), ..Entity::default() };
        for (k, v) in [
            ("BasisOrigin", "0 0 0"),
            ("BasisU", "1 0 0"),
            ("BasisV", "0 1 0"),
            ("BasisNormal", "0 0 1"),
            ("uv0", "-32 -16 0"),
            ("uv1", "-32 16 0"),
            ("uv2", "32 16 0"),
            ("uv3", "32 -16 0"),
            ("origin", "0 0 0"),
            ("sides", "1"),
        ] {
            e.set(k, v);
        }
        e
    }

    fn close(a: DVec3, b: DVec3) -> bool {
        (a - b).length() < 1e-6
    }

    #[test]
    fn rotation_turns_the_basis_and_keeps_the_corners() {
        let e = overlay();
        let b = OverlayBasis::read(&e).expect("basis");
        let m = DMat4::from_quat(DQuat::from_rotation_z(90f64.to_radians()));
        let n = transformed_basis(&b, &m, false).expect("basis");
        assert!(close(n.u, DVec3::Y) && close(n.v, DVec3::NEG_X) && close(n.normal, DVec3::Z));
        for (a, c) in b.corners().iter().zip(n.corners()) {
            assert!(close(m.transform_point3(*a), c), "{a:?} {c:?}");
        }
        let m = DMat4::from_quat(DQuat::from_rotation_x(90f64.to_radians()));
        let n = transformed_basis(&b, &m, false).expect("basis");
        assert!(close(n.normal, DVec3::NEG_Y), "{:?}", n.normal);
    }

    #[test]
    fn mirror_and_scale() {
        let b = OverlayBasis::read(&overlay()).expect("basis");
        let m = DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0));
        let n = transformed_basis(&b, &m, false).expect("basis");
        assert!(close(n.normal, DVec3::Z));
        assert!(close(n.u, DVec3::NEG_X));
        let mut want: Vec<[i64; 3]> =
            b.corners().iter().map(|c| m.transform_point3(*c).round().as_i64vec3().to_array()).collect();
        let mut got: Vec<[i64; 3]> = n.corners().iter().map(|c| c.round().as_i64vec3().to_array()).collect();
        want.sort_unstable();
        got.sort_unstable();
        assert_eq!(want, got);
        let m = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
        assert!(close(transformed_basis(&b, &m, false).expect("basis").normal, DVec3::NEG_Z));
        let m = DMat4::from_scale(DVec3::new(2.0, 1.0, 1.0));
        let n = transformed_basis(&b, &m, false).expect("basis");
        assert!(close(n.uv[2], DVec3::new(64.0, 16.0, 0.0)));
        let m = DMat4::from_scale(DVec3::splat(2.0));
        assert!(close(transformed_basis(&b, &m, false).expect("basis").uv[2], DVec3::new(32.0, 16.0, 0.0)));
        assert!(close(transformed_basis(&b, &m, true).expect("basis").uv[2], DVec3::new(64.0, 32.0, 0.0)));
        assert!(transformed_basis(&b, &DMat4::ZERO, false).is_none());
    }

    #[test]
    fn keys_are_written_cleanly() {
        let mut e = overlay();
        let m = DMat4::from_translation(DVec3::new(16.0, 0.0, 0.0));
        assert!(transform_overlay(&mut e, &m, true, false));
        assert_eq!(e.get("BasisOrigin"), Some("16 0 0"));
        assert_eq!(e.get("uv0"), Some("-32 -16 0"), "corners stay");
        let m = DMat4::from_quat(DQuat::from_rotation_z(180f64.to_radians()));
        assert!(transform_overlay(&mut e, &m, false, false));
        assert_eq!(e.get("BasisOrigin"), Some("-16 0 0"));
        assert_eq!(e.get("BasisU"), Some("-1 0 0"));
        assert_eq!(e.get("BasisNormal"), Some("0 0 1"), "unchanged keys keep their text");
        let mut plain = Entity { classname: "info_target".into(), ..Entity::default() };
        assert!(!transform_overlay(&mut plain, &m, false, false));
        assert!(!is_overlay(&plain) && is_overlay(&e));
    }

    #[test]
    fn polygon_overlap() {
        let sq = |x: f64, y: f64, s: f64| {
            vec![DVec2::new(x, y), DVec2::new(x + s, y), DVec2::new(x + s, y + s), DVec2::new(x, y + s)]
        };
        assert!(convex_overlap(&sq(0.0, 0.0, 10.0), &sq(5.0, 5.0, 10.0)));
        assert!(!convex_overlap(&sq(0.0, 0.0, 10.0), &sq(10.0, 0.0, 10.0)), "edges touching only");
        assert!(!convex_overlap(&sq(0.0, 0.0, 10.0), &sq(20.0, 0.0, 10.0)));
        assert!(convex_overlap(&[DVec2::new(5.0, 5.0)], &sq(0.0, 0.0, 10.0)));
        assert!(!convex_overlap(&[DVec2::new(15.0, 5.0)], &sq(0.0, 0.0, 10.0)));
        assert!(!convex_overlap(&[], &sq(0.0, 0.0, 10.0)));
    }

    use crate::selection::ObjRef;
    use crate::{Owner, PasteSpecial};

    fn scene() -> (Document, [Id; 2], [Id; 2], Id) {
        let mut doc = Document::new_empty();
        let mut slabs = [0; 2];
        let mut tops = [0; 2];
        for (i, x) in [0.0, 128.0].into_iter().enumerate() {
            let sides = crate::tests::box_sides(DVec3::new(x, 0.0, -16.0), DVec3::new(x + 128.0, 128.0, 0.0), "A");
            slabs[i] = doc.create_solid(sides, Owner::World).expect("slab");
            tops[i] = doc.scene().solid(slabs[i]).expect("slab").solid.sides[0].id;
        }
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let keys = vec![
            kv("material", "decals/test"),
            kv("BasisOrigin", "64 64 0"),
            kv("BasisU", "1 0 0"),
            kv("BasisV", "0 1 0"),
            kv("BasisNormal", "0 0 1"),
            kv("uv0", "-16 -16 0"),
            kv("uv1", "-16 16 0"),
            kv("uv2", "16 16 0"),
            kv("uv3", "16 -16 0"),
            kv("StartU", "0"),
            kv("EndU", "1"),
            kv("StartV", "0"),
            kv("EndV", "1"),
            kv("sides", &tops[0].to_string()),
        ];
        let ov = doc.create_point_entity("info_overlay", DVec3::new(64.0, 64.0, 0.0), &keys).expect("overlay");
        (doc, slabs, tops, ov)
    }

    fn key(doc: &Document, id: Id, k: &str) -> String {
        doc.scene().entity(id).and_then(|n| n.entity.get(k)).unwrap_or_default().to_owned()
    }

    #[test]
    fn a_moved_overlay_follows_its_new_face() {
        let (mut doc, _, tops, ov) = scene();
        doc.translate(&[ObjRef::Entity(ov)], DVec3::new(16.0, 0.0, 0.0)).expect("move");
        assert_eq!(key(&doc, ov, "BasisOrigin"), "80 64 0");
        assert_eq!(key(&doc, ov, "sides"), tops[0].to_string());
        doc.translate(&[ObjRef::Entity(ov)], DVec3::new(48.0, 0.0, 0.0)).expect("move");
        assert_eq!(key(&doc, ov, "sides"), tops[0].to_string());
        doc.translate(&[ObjRef::Entity(ov)], DVec3::new(64.0, 0.0, 0.0)).expect("move");
        assert_eq!(key(&doc, ov, "BasisOrigin"), "192 64 0");
        assert_eq!(key(&doc, ov, "sides"), tops[1].to_string());
        assert_eq!(key(&doc, ov, "origin"), "192 64 0");
        doc.translate(&[ObjRef::Entity(ov)], DVec3::new(0.0, 0.0, 64.0)).expect("move");
        assert_eq!(key(&doc, ov, "sides"), tops[1].to_string());
        assert!(doc.undo());
        assert_eq!(key(&doc, ov, "BasisOrigin"), "192 64 0");
    }

    #[test]
    fn rotate_and_flip_with_the_brush() {
        let (mut doc, slabs, tops, ov) = scene();
        let objs = [ObjRef::Solid(slabs[0]), ObjRef::Entity(ov)];
        doc.rotate_axis(&objs, DVec3::new(64.0, 64.0, -8.0), DVec3::Z, 90.0).expect("rotate");
        assert_eq!(key(&doc, ov, "BasisU"), "0 1 0");
        assert_eq!(key(&doc, ov, "BasisV"), "-1 0 0");
        assert_eq!(key(&doc, ov, "uv0"), "-16 -16 0", "corners in the rotated frame");
        assert_eq!(key(&doc, ov, "sides"), tops[0].to_string());
        doc.flip(&objs, 2, DVec3::new(64.0, 64.0, 32.0)).expect("flip");
        assert_eq!(key(&doc, ov, "BasisNormal"), "0 0 -1");
        assert_eq!(key(&doc, ov, "BasisOrigin"), "64 64 64");
        assert_eq!(key(&doc, ov, "sides"), tops[0].to_string());
        let m =
            DMat4::from_cols_array(&[1.0, 0.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        let before = OverlayBasis::read(&doc.scene().entity(ov).expect("ov").entity).expect("basis");
        doc.transform(&[ObjRef::Entity(ov)], m).expect("skew");
        let after = OverlayBasis::read(&doc.scene().entity(ov).expect("ov").entity).expect("basis");
        for (a, b) in before.corners().iter().zip(after.corners()) {
            assert!((m.transform_point3(*a) - b).length() < 1e-6, "{a:?} {b:?}");
        }
    }

    #[test]
    fn pasted_and_duplicated_overlays_use_the_copied_faces() {
        let (mut doc, slabs, tops, ov) = scene();
        let text = doc.copy(&[ObjRef::Solid(slabs[0]), ObjRef::Entity(ov)]);
        let objs = doc.paste(&text, DVec3::ZERO).expect("paste");
        let new_ov = objs.iter().find_map(|o| match o {
            ObjRef::Entity(e) => Some(*e),
            ObjRef::Solid(_) => None,
        });
        let new_slab = objs.iter().find_map(|o| match o {
            ObjRef::Solid(s) => Some(*s),
            ObjRef::Entity(_) => None,
        });
        let (new_ov, new_slab) = (new_ov.expect("overlay"), new_slab.expect("slab"));
        let new_top = doc.scene().solid(new_slab).expect("slab").solid.sides[0].id;
        assert_ne!(new_top, tops[0]);
        assert_eq!(key(&doc, new_ov, "sides"), new_top.to_string());
        assert_eq!(key(&doc, ov, "sides"), tops[0].to_string(), "the original keeps its face");
        let text = doc.copy(&[ObjRef::Entity(ov)]);
        let objs = doc.paste(&text, DVec3::new(128.0, 0.0, 0.0)).expect("paste");
        let ObjRef::Entity(alone) = objs[0] else { panic!("entity") };
        assert_eq!(key(&doc, alone, "sides"), tops[1].to_string());
        let text = doc.copy(&[ObjRef::Solid(slabs[0]), ObjRef::Entity(ov)]);
        let opts = PasteSpecial { copies: 2, offset: DVec3::new(0.0, 0.0, 256.0), ..PasteSpecial::default() };
        let objs = doc.paste_special(&text, &opts).expect("special");
        for pair in objs.chunks(2) {
            let (ObjRef::Solid(s), ObjRef::Entity(e)) = (pair[0], pair[1]) else { panic!("{pair:?}") };
            let top = doc.scene().solid(s).expect("slab").solid.sides[0].id;
            assert_eq!(key(&doc, e, "sides"), top.to_string());
        }
        let m = DMat4::from_translation(DVec3::new(0.0, 512.0, 0.0));
        let objs = doc
            .duplicate_transformed(&[ObjRef::Solid(slabs[0]), ObjRef::Entity(ov)], m, DVec3::ZERO, true)
            .expect("dup");
        let (ObjRef::Solid(s), ObjRef::Entity(e)) = (objs[0], objs[1]) else { panic!("{objs:?}") };
        assert_eq!(key(&doc, e, "sides"), doc.scene().solid(s).expect("slab").solid.sides[0].id.to_string());
        assert_eq!(key(&doc, e, "BasisOrigin"), "64 576 0");
        assert_eq!(doc.undo_label(), Some("Duplicate"));
    }

    #[test]
    fn an_overlay_pasted_into_another_map_finds_its_face() {
        let (src, _, tops, ov) = scene();
        let text = src.copy(&[ObjRef::Entity(ov)]);
        let mut doc = Document::new_empty();
        let sides = |min: DVec3, max: DVec3| crate::tests::box_sides(min, max, "A");
        let far =
            doc.create_solid(sides(DVec3::new(1000.0, 1000.0, -16.0), DVec3::new(1128.0, 1128.0, 0.0)), Owner::World);
        let under = doc.create_solid(sides(DVec3::new(0.0, 0.0, -16.0), DVec3::new(128.0, 128.0, 0.0)), Owner::World);
        let (far, under) = (far.expect("far"), under.expect("under"));
        assert_eq!(doc.scene().solid(far).expect("far").solid.sides[0].id, tops[0], "the old id exists elsewhere");
        let objs = doc.paste(&text, DVec3::ZERO).expect("paste");
        let ObjRef::Entity(pasted) = objs[0] else { panic!("entity") };
        let top = doc.scene().solid(under).expect("under").solid.sides[0].id;
        assert_eq!(key(&doc, pasted, "sides"), top.to_string());
    }

    #[test]
    fn duplicated_overlay_moved_by_another_path_prefers_the_moved_slab() {
        let (mut doc, slabs, tops, ov) = scene();
        let objs =
            doc.duplicate(&[ObjRef::Solid(slabs[0]), ObjRef::Entity(ov)], DVec3::new(16.0, 0.0, 0.0)).expect("dup");
        let (ObjRef::Solid(s), ObjRef::Entity(e)) = (objs[0], objs[1]) else { panic!("{objs:?}") };
        let top = doc.scene().solid(s).expect("slab").solid.sides[0].id;
        assert_ne!(top, tops[0]);
        assert_eq!(key(&doc, e, "sides"), top.to_string());
    }
}
