mod overlay;

use glam::{DMat3, DMat4, DQuat, DVec3};
use hp_geom::{Aabb, GeomError, Ray, snap_vec3, transform_angles_mat, transform_solid_anchored, translate_solid};
use hp_vmf::{Entity, Solid};
use rayon::prelude::*;

use super::clean_vec;
use crate::clipboard::SideLayout;
use crate::document::Document;
use crate::ids::{IdSet, id_set};
use crate::pick::PickOptions;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

const DROP_INSET: f64 = 0.05;

fn is_model_path(model: &str) -> bool {
    let m = model.trim();
    m.len() > 4 && m.get(m.len() - 4..).is_some_and(|ext| ext.eq_ignore_ascii_case(".mdl"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlignMode {
    Min,
    Max,
    Center,
}

#[derive(Clone, Copy, Debug)]
enum Xform {
    Translate(DVec3),
    Matrix(DMat4, DVec3),
}

fn transform_entity(e: &mut Entity, xf: Xform, is_brush: bool, scale_lock: bool) -> bool {
    let mut changed = false;
    let is_overlay = overlay::is_overlay(e);
    if is_overlay {
        changed |= match xf {
            Xform::Translate(d) => overlay::transform_overlay(e, &DMat4::from_translation(d), true, scale_lock),
            Xform::Matrix(m, _) => overlay::transform_overlay(e, &m, false, scale_lock),
        };
    }
    if let Some(o) = e.origin() {
        let new = match xf {
            Xform::Translate(d) => o + d,
            Xform::Matrix(m, _) => m.transform_point3(o),
        };
        let new = clean_vec(new);
        if new != o && new.is_finite() {
            e.set("origin", hp_vmf::fmt_vec3(new));
            changed = true;
        }
    }
    if let Xform::Matrix(m, _) = xf
        && !is_brush
        && !is_overlay
        && is_conformal(&DMat3::from_mat4(m))
        && let Some(a) = e.angles()
    {
        let new = transform_angles_mat(a, &DMat3::from_mat4(m));
        if new != a && new.is_finite() {
            e.set("angles", hp_vmf::fmt_vec3(new));
            changed = true;
        }
    }
    changed
}

fn is_conformal(l: &DMat3) -> bool {
    let g = l.transpose() * *l;
    let s = g.x_axis.x;
    s.is_finite() && s > 1e-12 && (g * (1.0 / s)).abs_diff_eq(DMat3::IDENTITY, 1e-9)
}

fn nudge_tag(objs: &[ObjRef]) -> String {
    use std::hash::{Hash, Hasher};
    let mut ids = objs.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ids.hash(&mut h);
    format!("nudge:{}:{:016x}", ids.len(), h.finish())
}

fn box_to_box(from: &Aabb, to: &Aabb) -> EditResult<DMat4> {
    let ok = |b: &Aabb| !b.is_empty() && b.min.is_finite() && b.max.is_finite();
    if !ok(from) || !ok(to) {
        return Err(EditError::Invalid("empty or invalid bounds".to_owned()));
    }
    let fs = from.size();
    let ts = to.size();
    let mut scale = DVec3::ONE;
    for k in 0..3 {
        if fs[k] > 1e-9 {
            scale[k] = ts[k] / fs[k];
            if scale[k].abs() < 1e-9 {
                return Err(EditError::Invalid("scaling to a flat box".to_owned()));
            }
        }
    }
    Ok(DMat4::from_translation(to.min) * DMat4::from_scale(scale) * DMat4::from_translation(-from.min))
}

impl Document {
    fn transform_impl(&mut self, label: &str, merge_tag: Option<&str>, objs: &[ObjRef], xf: Xform) -> EditResult<()> {
        match xf {
            Xform::Matrix(m, anchor) => {
                Self::check_affine(&m)?;
                if !anchor.is_finite() {
                    return Err(EditError::Invalid("pivot is not finite".to_owned()));
                }
            }
            Xform::Translate(d) => {
                if !d.is_finite() {
                    return Err(EditError::Invalid("offset is not finite".to_owned()));
                }
                if d == DVec3::ZERO {
                    return Ok(());
                }
            }
        }
        let solid_ids = self.scene.expand_solids(objs);
        let entity_ids = self.scene.expand_entities(objs);
        if solid_ids.is_empty() && entity_ids.is_empty() {
            return Ok(());
        }
        let (lock, scale_lock) = (self.options.texture_lock, self.options.texture_scale_lock);
        let scene = &self.scene;
        let results: Vec<Result<(Id, Solid), GeomError>> = solid_ids
            .par_iter()
            .filter_map(|id| scene.solids.get(id).map(|n| (*id, n)))
            .map(|(id, n)| {
                let mut s = n.solid.clone();
                match xf {
                    Xform::Translate(d) => translate_solid(&mut s, d, lock),
                    Xform::Matrix(m, anchor) => transform_solid_anchored(&mut s, m, lock, scale_lock, anchor)?,
                }
                Ok((id, s))
            })
            .collect();
        let mut updates = Vec::with_capacity(results.len());
        for r in results {
            updates.push(r?);
        }
        let mut ent_updates: Vec<(Id, Entity)> = Vec::new();
        let mut overlays: Vec<Id> = Vec::new();
        for eid in entity_ids {
            let Some(n) = scene.entities.get(&eid) else { continue };
            let mut e = n.entity.clone();
            if transform_entity(&mut e, xf, scene.is_brush_entity(eid), scale_lock) {
                if overlay::is_overlay(&e) {
                    overlays.push(eid);
                }
                ent_updates.push((eid, e));
            }
        }
        let moved: IdSet = if overlays.is_empty() { id_set(0) } else { solid_ids.iter().copied().collect() };
        let run = move |doc: &mut Self| -> EditResult<()> {
            doc.update_solids(updates);
            for (id, e) in ent_updates {
                doc.update_entity(id, |x| *x = e);
            }
            if !overlays.is_empty() {
                doc.fix_overlay_sides(&overlays, &moved);
            }
            Ok(())
        };
        match merge_tag {
            Some(tag) if !self.in_transaction() => {
                self.begin_merge(label, tag);
                let r = run(self);
                self.commit();
                r
            }
            _ => self.tx(label, run),
        }
    }

    pub fn translate(&mut self, objs: &[ObjRef], delta: DVec3) -> EditResult<()> {
        self.transform_impl("Move", None, objs, Xform::Translate(delta))
    }

    pub fn nudge(&mut self, objs: &[ObjRef], delta: DVec3) -> EditResult<()> {
        let tag = nudge_tag(objs);
        self.transform_impl("Nudge", Some(&tag), objs, Xform::Translate(delta))
    }

    pub fn rotate(&mut self, objs: &[ObjRef], pivot: DVec3, rotation: DQuat) -> EditResult<()> {
        let len = rotation.length();
        if !rotation.is_finite() || len < 1e-12 {
            return Err(EditError::Invalid("invalid rotation".to_owned()));
        }
        let q = rotation / len;
        let m = DMat4::from_translation(pivot) * DMat4::from_quat(q) * DMat4::from_translation(-pivot);
        self.transform_impl("Rotate", None, objs, Xform::Matrix(m, pivot))
    }

    pub fn rotate_axis(&mut self, objs: &[ObjRef], pivot: DVec3, axis: DVec3, degrees: f64) -> EditResult<()> {
        let axis = axis.try_normalize().ok_or_else(|| EditError::Invalid("rotation axis is zero".to_owned()))?;
        if !degrees.is_finite() {
            return Err(EditError::Invalid("angle is not finite".to_owned()));
        }
        self.rotate(objs, pivot, DQuat::from_axis_angle(axis, degrees.to_radians()))
    }

    pub fn scale_bounds(&mut self, objs: &[ObjRef], from: &Aabb, to: &Aabb) -> EditResult<()> {
        let m = box_to_box(from, to)?;
        if m.abs_diff_eq(DMat4::IDENTITY, 1e-12) {
            return Ok(());
        }
        self.transform_impl("Scale", None, objs, Xform::Matrix(m, from.center()))
    }

    pub fn flip(&mut self, objs: &[ObjRef], axis: usize, center: DVec3) -> EditResult<()> {
        if axis > 2 {
            return Err(EditError::Invalid(format!("invalid axis {axis}")));
        }
        let mut s = DVec3::ONE;
        s[axis] = -1.0;
        let m = DMat4::from_translation(center) * DMat4::from_scale(s) * DMat4::from_translation(-center);
        self.transform_impl("Flip", None, objs, Xform::Matrix(m, center))
    }

    pub fn transform(&mut self, objs: &[ObjRef], m: DMat4) -> EditResult<()> {
        let solids = self.scene.expand_solids(objs);
        let mut b = Aabb::EMPTY;
        for id in &solids {
            if let Some(sb) = self.scene.solid_bounds(*id) {
                b = b.union(&sb);
            }
        }
        let anchor = if b.is_empty() { DVec3::ZERO } else { b.center() };
        self.transform_impl("Transform", None, objs, Xform::Matrix(m, anchor))
    }

    pub fn duplicate_transformed(
        &mut self,
        objs: &[ObjRef],
        m: DMat4,
        anchor: DVec3,
        keep_visgroups: bool,
    ) -> EditResult<Vec<ObjRef>> {
        self.duplicate_transformed_with(objs, m, anchor, keep_visgroups, None)
    }

    pub fn duplicate_transformed_with(
        &mut self,
        objs: &[ObjRef],
        m: DMat4,
        anchor: DVec3,
        keep_visgroups: bool,
        is_target_key: Option<crate::problems::TargetKeyFn<'_>>,
    ) -> EditResult<Vec<ObjRef>> {
        Self::check_affine(&m)?;
        if !anchor.is_finite() {
            return Err(EditError::Invalid("pivot is not finite".to_owned()));
        }
        let mut frag = self.extract_fragment(objs);
        if frag.is_empty() {
            return Err(EditError::Nothing("nothing to duplicate"));
        }
        self.auto_rename_copy(&mut frag, is_target_key);
        if !keep_visgroups {
            let clear = |e: &mut hp_vmf::EditorInfo| {
                e.visgroup_ids.clear();
                e.visgroup_shown = true;
                e.visgroup_auto_shown = true;
            };
            for s in &mut frag.solids {
                clear(&mut s.editor);
            }
            for e in &mut frag.entities {
                clear(&mut e.editor);
                for s in &mut e.solids {
                    clear(&mut s.editor);
                }
            }
            for g in &mut frag.groups {
                clear(&mut g.editor);
            }
        }
        let layout = SideLayout::of(&frag);
        let translation = m.w_axis.truncate();
        let rigid_move = DMat3::from_mat4(m).abs_diff_eq(DMat3::IDENTITY, 1e-12);
        self.tx("Duplicate", |doc| {
            let new = doc.instantiate(frag, true)?;
            if let Some(layout) = &layout {
                doc.remap_side_lists(layout, &new);
            }
            if rigid_move {
                doc.transform_impl("Duplicate", None, &new, Xform::Translate(translation))?;
            } else {
                doc.transform_impl("Duplicate", None, &new, Xform::Matrix(m, anchor))?;
            }
            doc.select(&new, SelectOp::Replace);
            Ok(new)
        })
    }

    pub fn drop_to_surface(
        &mut self,
        objs: &[ObjRef],
        dir: DVec3,
        align: bool,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
    ) -> EditResult<usize> {
        let Some(dir) = dir.try_normalize().filter(|d| d.is_finite()) else {
            return Err(EditError::Invalid("direction is zero".to_owned()));
        };
        let mut units: Vec<(Option<Id>, Vec<ObjRef>)> = Vec::new();
        for o in objs {
            if !self.scene.contains(*o) {
                continue;
            }
            let root = self.scene.object_group(*o).map(|g| self.scene.group_root(g));
            match root.and_then(|r| units.iter_mut().find(|(g, _)| *g == Some(r))) {
                Some((_, list)) => list.push(*o),
                None => units.push((root, vec![*o])),
            }
        }
        let moving_solids: IdSet = self.scene.expand_solids(objs).into_iter().collect();
        let moving_entities: IdSet = self.scene.expand_entities(objs).into_iter().collect();
        let surface = |o: ObjRef| match o {
            ObjRef::Solid(id) => !moving_solids.contains(&id),
            ObjRef::Entity(id) => {
                !moving_entities.contains(&id)
                    && self.scene.entities.get(&id).and_then(|n| n.entity.get("model")).is_some_and(is_model_path)
            }
        };
        let opts = PickOptions { filter: Some(&surface), ..PickOptions::default() };
        let k = dir.abs().max_position();
        let (a, b) = ((k + 1) % 3, (k + 2) % 3);
        let mut moves: Vec<(Vec<ObjRef>, DVec3, Option<DVec3>)> = Vec::new();
        for (_, unit) in &units {
            let Some(bb) = self.scene.objects_bounds(unit, entity_bounds) else { continue };
            if !bb.min.is_finite() || !bb.max.is_finite() {
                continue;
            }
            let half = bb.size()[k] * 0.5;
            let inset = DVec3::splat(DROP_INSET).min(bb.size() * 0.5);
            let (lo, hi, c) = (bb.min + inset, bb.max - inset, bb.center());
            let mut best: Option<(f64, DVec3)> = None;
            for (u, v) in [(lo[a], lo[b]), (hi[a], lo[b]), (hi[a], hi[b]), (lo[a], hi[b]), (c[a], c[b])] {
                let mut p = DVec3::ZERO;
                p[k] = c[k];
                p[a] = u;
                p[b] = v;
                if let Some(hit) = self.pick_ray(&Ray::new(p, dir), &opts, entity_bounds)
                    && best.is_none_or(|(t, _)| hit.t < t)
                {
                    best = Some((hit.t, hit.normal));
                }
            }
            let Some((t, normal)) = best else { continue };
            let delta = clean_vec(dir * (t - half));
            let normal = align.then_some(normal);
            if delta != DVec3::ZERO || normal.is_some() {
                moves.push((unit.clone(), delta, normal));
            }
        }
        if moves.is_empty() {
            return Ok(0);
        }
        let label = if dir.z > 0.0 { "Move to ceiling" } else { "Move to floor" };
        let count = moves.len();
        self.tx(label, |doc| {
            for (unit, delta, normal) in moves {
                if let Some(n) = normal {
                    doc.align_point_entities(&unit, n);
                }
                if delta != DVec3::ZERO {
                    doc.transform_impl(label, None, &unit, Xform::Translate(delta))?;
                }
            }
            Ok(count)
        })
    }

    fn align_point_entities(&mut self, objs: &[ObjRef], normal: DVec3) {
        let Some(n) = normal.try_normalize() else { return };
        let mut updates: Vec<(Id, DVec3)> = Vec::new();
        for o in objs {
            let ObjRef::Entity(id) = *o else { continue };
            if self.scene.is_brush_entity(id) {
                continue;
            }
            let Some(node) = self.scene.entities.get(&id) else { continue };
            let angles = node.entity.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
            let new = hp_geom::align_angles_to_normal(angles, n);
            if new != angles && new.is_finite() {
                updates.push((id, new));
            }
        }
        for (id, a) in updates {
            self.update_entity(id, |e| e.set("angles", hp_vmf::fmt_vec3(a)));
        }
    }

    pub fn physics_place(
        &mut self,
        objs: &[ObjRef],
        align: bool,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
    ) -> EditResult<usize> {
        let mut units: Vec<Vec<ObjRef>> = Vec::new();
        let mut seen: IdSet = IdSet::default();
        for o in objs {
            if !self.scene.contains(*o) {
                continue;
            }
            let unit = match self.scene.object_group(*o).map(|g| self.scene.group_root(g)) {
                Some(root) => self.scene.group_members(root),
                None => vec![*o],
            };
            if unit.iter().any(|m| !seen.insert(m.id())) {
                continue;
            }
            units.push(unit);
        }
        units.sort_by(|a, b| {
            let z = |u: &[ObjRef]| self.scene.objects_bounds(u, entity_bounds).map_or(f64::MAX, |b| b.min.z);
            z(a).total_cmp(&z(b))
        });
        if units.is_empty() {
            return Ok(0);
        }
        let mut moved = 0;
        self.tx("Physics placement", |doc| {
            for unit in units {
                moved += doc.drop_to_surface(&unit, DVec3::NEG_Z, align, entity_bounds)?;
            }
            Ok(moved)
        })
    }

    pub fn snap_to_grid(
        &mut self,
        objs: &[ObjRef],
        grid: f64,
        individually: bool,
        entity_bounds: &dyn Fn(&Entity) -> Option<hp_geom::Aabb>,
    ) -> EditResult<()> {
        if !grid.is_finite() || grid <= 0.0 {
            return Err(EditError::Invalid(format!("invalid grid size {grid}")));
        }
        let moves = self.unit_moves(objs, individually, entity_bounds, |b| snap_vec3(b.min, grid) - b.min);
        self.apply_moves("Snap to grid", moves)
    }

    pub fn align_objects(
        &mut self,
        objs: &[ObjRef],
        reference: &Aabb,
        axis: usize,
        mode: AlignMode,
        individually: bool,
        entity_bounds: &dyn Fn(&Entity) -> Option<hp_geom::Aabb>,
    ) -> EditResult<()> {
        if axis > 2 {
            return Err(EditError::Invalid(format!("invalid axis {axis}")));
        }
        if reference.is_empty() || !reference.min.is_finite() || !reference.max.is_finite() {
            return Err(EditError::Invalid("invalid reference bounds".to_owned()));
        }
        let r = *reference;
        let moves = self.unit_moves(objs, individually, entity_bounds, |b| {
            let mut d = DVec3::ZERO;
            d[axis] = match mode {
                AlignMode::Min => r.min[axis] - b.min[axis],
                AlignMode::Max => r.max[axis] - b.max[axis],
                AlignMode::Center => r.center()[axis] - b.center()[axis],
            };
            d
        });
        self.apply_moves("Align", moves)
    }

    fn unit_moves(
        &self,
        objs: &[ObjRef],
        individually: bool,
        entity_bounds: &dyn Fn(&Entity) -> Option<hp_geom::Aabb>,
        delta: impl Fn(&Aabb) -> DVec3,
    ) -> Vec<(Vec<ObjRef>, DVec3)> {
        if individually {
            objs.iter()
                .filter_map(|o| self.scene.object_bounds(*o, entity_bounds).map(|b| (vec![*o], delta(&b))))
                .collect()
        } else {
            match self.scene.objects_bounds(objs, entity_bounds) {
                Some(b) => vec![(objs.to_vec(), delta(&b))],
                None => Vec::new(),
            }
        }
    }

    fn apply_moves(&mut self, label: &str, moves: Vec<(Vec<ObjRef>, DVec3)>) -> EditResult<()> {
        let moves: Vec<(Vec<ObjRef>, DVec3)> =
            moves.into_iter().filter(|(_, d)| d.is_finite() && d.length_squared() > 1e-18).collect();
        if moves.is_empty() {
            return Ok(());
        }
        self.tx(label, |doc| {
            for (objs, d) in moves {
                doc.transform_impl(label, None, &objs, Xform::Translate(d))?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_mapping() {
        let from = Aabb::new(DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0));
        let to = Aabb::new(DVec3::new(10.0, 0.0, 0.0), DVec3::new(138.0, 64.0, 32.0));
        let m = box_to_box(&from, &to).expect("matrix");
        assert!((m.transform_point3(from.min) - to.min).length() < 1e-9);
        assert!((m.transform_point3(from.max) - to.max).length() < 1e-9);
        assert!(box_to_box(&from, &Aabb::new(DVec3::ZERO, DVec3::new(0.0, 64.0, 64.0))).is_err());
        assert!(box_to_box(&Aabb::EMPTY, &to).is_err());
        let mut e = Entity { classname: "info_target".into(), ..Entity::default() };
        e.set("origin", "1 2 3");
        e.set("angles", "0 0 0");
        let rot = DMat4::from_quat(DQuat::from_rotation_z(90f64.to_radians()));
        assert!(transform_entity(&mut e, Xform::Matrix(rot, DVec3::ZERO), false, false));
        assert_eq!(e.get("origin"), Some("-2 1 3"));
        assert_eq!(e.get("angles"), Some("0 90 0"));
        let stretch = DMat4::from_scale(DVec3::new(2.0, 1.0, 1.0));
        assert!(transform_entity(&mut e, Xform::Matrix(stretch, DVec3::ZERO), false, false));
        assert_eq!(e.get("origin"), Some("-4 1 3"));
        assert_eq!(e.get("angles"), Some("0 90 0"));
        assert!(is_conformal(&DMat3::from_diagonal(DVec3::new(-2.0, 2.0, 2.0))));
        assert!(!is_conformal(&DMat3::ZERO));
    }

    use crate::Owner;
    use crate::tests::{box_sides, no_bounds};

    fn slab(doc: &mut Document, min: DVec3, max: DVec3) -> Id {
        doc.create_solid(box_sides(min, max, "A"), Owner::World).expect("solid")
    }

    #[test]
    fn drop_to_floor_and_ceiling() {
        let mut doc = Document::new_empty();
        let _floor = slab(&mut doc, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0));
        let _ceiling = slab(&mut doc, DVec3::new(-256.0, -256.0, 300.0), DVec3::new(256.0, 256.0, 316.0));
        let crate_a = slab(&mut doc, DVec3::new(0.0, 0.0, 100.0), DVec3::new(32.0, 32.0, 132.0));
        let crate_b = slab(&mut doc, DVec3::new(64.0, 0.0, 150.0), DVec3::new(96.0, 32.0, 182.0));
        let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        let prop = doc
            .create_point_entity("prop_static", DVec3::new(-100.0, 0.0, 200.0), &[kv("model", "models/x.mdl")])
            .expect("prop");
        let lamp = doc
            .create_point_entity("prop_dynamic", DVec3::new(-100.0, 100.0, 50.0), &[kv("angles", "30 45 0")])
            .expect("lamp");
        let outside = doc.create_point_entity("info_target", DVec3::new(5000.0, 0.0, 64.0), &[]).expect("target");
        doc.group(&[ObjRef::Solid(crate_a), ObjRef::Solid(crate_b)]).expect("group");
        let objs = [ObjRef::Solid(crate_a), ObjRef::Solid(crate_b), ObjRef::Entity(prop), ObjRef::Entity(outside)];
        let steps = doc.history_len().0;
        assert_eq!(doc.drop_to_surface(&objs, DVec3::NEG_Z, false, &no_bounds).expect("drop"), 2);
        assert_eq!(doc.history_len().0, steps + 1, "one undo step");
        assert_eq!(doc.undo_label(), Some("Move to floor"));
        let b = |doc: &Document, id| doc.scene().solid_bounds(id).expect("bounds");
        assert_eq!(b(&doc, crate_a).min.z, 0.0);
        assert_eq!(b(&doc, crate_b).min.z, 50.0, "moved with its group");
        assert_eq!(doc.scene().entity(prop).and_then(|n| n.entity.origin()), Some(DVec3::new(-100.0, 0.0, 8.0)));
        assert_eq!(doc.scene().entity(outside).and_then(|n| n.entity.origin()), Some(DVec3::new(5000.0, 0.0, 64.0)));
        let objs = [ObjRef::Solid(crate_a)];
        assert_eq!(doc.drop_to_surface(&objs, DVec3::Z, false, &no_bounds).expect("up"), 1);
        assert_eq!(doc.undo_label(), Some("Move to ceiling"));
        assert_eq!(b(&doc, crate_a).max.z, 300.0);
        let steps = doc.history_len().0;
        assert_eq!(doc.drop_to_surface(&[ObjRef::Entity(prop)], DVec3::NEG_Z, false, &no_bounds).expect("drop"), 0);
        assert_eq!(doc.history_len().0, steps);
        doc.drop_to_surface(&[ObjRef::Entity(lamp)], DVec3::NEG_Z, true, &no_bounds).expect("align");
        let e = &doc.scene().entity(lamp).expect("lamp").entity;
        assert_eq!(e.get("angles"), Some("0 45 0"));
        assert_eq!(e.origin(), Some(DVec3::new(-100.0, 100.0, 8.0)));
        assert!(doc.drop_to_surface(&objs, DVec3::ZERO, false, &no_bounds).is_err());
        let sunk = slab(&mut doc, DVec3::new(-200.0, -200.0, -4.0), DVec3::new(-168.0, -168.0, 28.0));
        assert_eq!(doc.drop_to_surface(&[ObjRef::Solid(sunk)], DVec3::NEG_Z, false, &no_bounds).expect("drop"), 1);
        assert_eq!(b(&doc, sunk).min.z, 0.0);
    }

    #[test]
    fn physics_placement_stacks_objects() {
        let mut doc = Document::new_empty();
        let _floor = slab(&mut doc, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0));
        let low = slab(&mut doc, DVec3::new(0.0, 0.0, 100.0), DVec3::new(32.0, 32.0, 132.0));
        let mid = slab(&mut doc, DVec3::new(0.0, 0.0, 200.0), DVec3::new(32.0, 32.0, 232.0));
        let high = slab(&mut doc, DVec3::new(0.0, 0.0, 300.0), DVec3::new(32.0, 32.0, 332.0));
        let objs = [ObjRef::Solid(high), ObjRef::Solid(low), ObjRef::Solid(mid)];
        let n = doc.physics_place(&objs, false, &no_bounds).expect("place");
        assert_eq!(n, 3);
        assert_eq!(doc.undo_label(), Some("Physics placement"), "one undo step");
        let z = |doc: &Document, id| doc.scene().solid_bounds(id).expect("bounds").min.z;
        assert_eq!(z(&doc, low), 0.0);
        assert_eq!(z(&doc, mid), 32.0, "rests on the first crate");
        assert_eq!(z(&doc, high), 64.0, "rests on the second");
        assert!(doc.undo());
        assert_eq!(z(&doc, low), 100.0);
        assert_eq!(doc.physics_place(&[], false, &no_bounds).expect("empty"), 0);
    }

    #[test]
    fn transformed_duplicates() {
        let mut doc = Document::new_empty();
        let a = slab(&mut doc, DVec3::ZERO, DVec3::new(64.0, 32.0, 16.0));
        let vis = doc.create_visgroup("Lights", None, &[ObjRef::Solid(a)]).expect("visgroup");
        let pivot = DVec3::new(32.0, 16.0, 8.0);
        let m = DMat4::from_translation(pivot)
            * DMat4::from_quat(DQuat::from_rotation_z(90f64.to_radians()))
            * DMat4::from_translation(-pivot);
        let new = doc.duplicate_transformed(&[ObjRef::Solid(a)], m, DVec3::new(32.0, 16.0, 8.0), true).expect("dup");
        let ObjRef::Solid(c) = new[0] else { panic!("solid") };
        assert_eq!(doc.scene().solids().len(), 2);
        let bb = doc.scene().solid_bounds(c).expect("bounds");
        assert!((bb.size() - DVec3::new(32.0, 64.0, 16.0)).length() < 1e-6, "{bb:?}");
        assert_eq!(doc.selected_objects(), vec![ObjRef::Solid(c)]);
        assert_eq!(doc.scene().solid(c).expect("copy").solid.editor.visgroup_ids, vec![vis]);
        assert!(doc.undo());
        assert_eq!(doc.scene().solids().len(), 1, "one undo step");
        let new = doc
            .duplicate_transformed(&[ObjRef::Solid(a)], DMat4::from_translation(DVec3::X * 64.0), DVec3::ZERO, false)
            .expect("dup");
        let ObjRef::Solid(c) = new[0] else { panic!("solid") };
        assert!(doc.scene().solid(c).expect("copy").solid.editor.visgroup_ids.is_empty());
        assert_eq!(doc.scene().solid_bounds(c).expect("bounds").min, DVec3::new(64.0, 0.0, 0.0));
        assert!(doc.duplicate_transformed(&[ObjRef::Solid(a)], DMat4::ZERO, DVec3::ZERO, true).is_err());
        assert!(doc.duplicate_transformed(&[], DMat4::IDENTITY, DVec3::ZERO, true).is_err());
    }
}
