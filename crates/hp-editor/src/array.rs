use glam::{DMat3, DMat4, DVec3};
use hp_geom::Aabb;
use hp_vmf::Entity;

use crate::document::Document;
use crate::problems::TargetKeyFn;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult};

pub const MAX_ARRAY_COPIES: u64 = 4096;
pub const MAX_ARRAY_AXIS_COUNT: u32 = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArrayAxis {
    pub count: u32,
    pub gap: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrayFrame {
    pub origin: DVec3,
    pub axes: DMat3,
    pub step: DVec3,
}

impl Default for ArrayFrame {
    fn default() -> Self {
        Self { origin: DVec3::ZERO, axes: DMat3::IDENTITY, step: DVec3::ZERO }
    }
}

impl ArrayFrame {
    fn local_point(&self, p: DVec3) -> DVec3 {
        self.axes.transpose() * (p - self.origin)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArraySpec {
    pub axes: [ArrayAxis; 3],
    pub frame: ArrayFrame,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayPlan {
    pub steps: DVec3,
    pub extent: DVec3,
    pub deltas: Vec<DVec3>,
}

fn snap_up(value: f64, grid: f64) -> f64 {
    if grid > 0.0 { ((value / grid) - 1e-9).ceil() * grid } else { value }
}

fn snap_round(value: f64, grid: f64) -> f64 {
    if grid > 0.0 { (value / grid).round() * grid } else { value }
}

fn tidy(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if (v - r).abs() < 1e-9 * v.abs().max(1.0) { r + 0.0 } else { v + 0.0 }
}

impl Document {
    fn local_extent(
        &self,
        objs: &[ObjRef],
        frame: &ArrayFrame,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
    ) -> Option<(DVec3, DVec3)> {
        let mut lo = DVec3::splat(f64::INFINITY);
        let mut hi = DVec3::splat(f64::NEG_INFINITY);
        let mut add = |p: DVec3| {
            let l = frame.local_point(p);
            if l.is_finite() {
                lo = lo.min(l);
                hi = hi.max(l);
            }
        };
        for id in self.scene.expand_solids(objs) {
            let Some(n) = self.scene.solids.get(&id) else { continue };
            let geom = hp_geom::build_solid(&n.solid.sides);
            for f in &geom.faces {
                for v in &f.vertices {
                    add(*v);
                }
            }
        }
        for id in self.scene.expand_entities(objs) {
            if self.scene.is_brush_entity(id) {
                continue;
            }
            let Some(n) = self.scene.entities.get(&id) else { continue };
            match entity_bounds(&n.entity).filter(|b| !b.is_empty()) {
                Some(b) => {
                    for i in 0..8 {
                        let corner = DVec3::new(
                            if i & 1 == 0 { b.min.x } else { b.max.x },
                            if i & 2 == 0 { b.min.y } else { b.max.y },
                            if i & 4 == 0 { b.min.z } else { b.max.z },
                        );
                        add(corner);
                    }
                }
                None => {
                    if let Some(o) = n.entity.origin() {
                        add(o);
                    }
                }
            }
        }
        (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
    }

    pub fn array_plan(
        &self,
        objs: &[ObjRef],
        spec: &ArraySpec,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
    ) -> EditResult<ArrayPlan> {
        let a = &spec.axes;
        if a.iter().all(|x| x.count == 0) {
            return Err(EditError::Invalid("give a copy count on at least one axis".to_owned()));
        }
        if a.iter().any(|x| x.count > MAX_ARRAY_AXIS_COUNT || !x.gap.is_finite()) {
            return Err(EditError::Invalid("invalid copy count or spacing".to_owned()));
        }
        let f = &spec.frame;
        if !f.origin.is_finite() || !f.axes.is_finite() || !f.step.is_finite() || f.step.min_element() < 0.0 {
            return Err(EditError::Invalid("invalid grid".to_owned()));
        }
        let total = a.iter().map(|x| u64::from(x.count) + 1).product::<u64>() - 1;
        if total > MAX_ARRAY_COPIES {
            return Err(EditError::Invalid(format!("too many copies ({total}, at most {MAX_ARRAY_COPIES})")));
        }
        let (lo, hi) = self
            .local_extent(objs, f, entity_bounds)
            .ok_or_else(|| EditError::Invalid("the objects have no bounds".to_owned()))?;
        let extent = hi - lo;
        let mut steps = DVec3::ZERO;
        for k in 0..3 {
            if a[k].count == 0 {
                continue;
            }
            let s = snap_up(extent[k] + a[k].gap, f.step[k]);
            if s.abs() < 1e-6 {
                return Err(EditError::Invalid(
                    "the spacing would put every copy in the same place on an axis".to_owned(),
                ));
            }
            steps[k] = tidy(s);
        }
        let base = DVec3::new(
            snap_round(lo.x, f.step.x) - lo.x,
            snap_round(lo.y, f.step.y) - lo.y,
            snap_round(lo.z, f.step.z) - lo.z,
        );
        let mut deltas = Vec::with_capacity(total as usize);
        for k in 0..=a[2].count {
            for j in 0..=a[1].count {
                for i in 0..=a[0].count {
                    if i == 0 && j == 0 && k == 0 {
                        continue;
                    }
                    let local = DVec3::new(
                        f64::from(i) * steps.x + if i > 0 { base.x } else { 0.0 },
                        f64::from(j) * steps.y + if j > 0 { base.y } else { 0.0 },
                        f64::from(k) * steps.z + if k > 0 { base.z } else { 0.0 },
                    );
                    let d = f.axes * local;
                    deltas.push(DVec3::new(tidy(d.x), tidy(d.y), tidy(d.z)));
                }
            }
        }
        Ok(ArrayPlan { steps, extent, deltas })
    }

    pub fn array_copies(
        &mut self,
        objs: &[ObjRef],
        spec: &ArraySpec,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
        is_target_key: Option<TargetKeyFn<'_>>,
    ) -> EditResult<(ArrayPlan, Vec<ObjRef>)> {
        let plan = self.array_plan(objs, spec, entity_bounds)?;
        let deltas = plan.deltas.clone();
        let copies = self.tx("Array", |doc| {
            let mut all = Vec::with_capacity(deltas.len() * objs.len());
            for d in deltas {
                let m = DMat4::from_translation(d);
                all.extend(doc.duplicate_transformed_with(objs, m, DVec3::ZERO, true, is_target_key)?);
            }
            doc.select(&all, SelectOp::Replace);
            Ok(all)
        })?;
        Ok((plan, copies))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_box() -> (Document, ObjRef) {
        let mut doc = Document::new_empty();
        let sides = crate::tests::box_sides(DVec3::ZERO, DVec3::new(100.0, 40.0, 30.0), "dev/dev_measuregeneric01b");
        let id = doc.create_solid(sides, crate::Owner::World).expect("solid");
        (doc, ObjRef::Solid(id))
    }

    fn spec(counts: [u32; 3], gap: f64, step: f64) -> ArraySpec {
        ArraySpec {
            axes: [0, 1, 2].map(|k| ArrayAxis { count: counts[k], gap }),
            frame: ArrayFrame { step: DVec3::splat(step), ..ArrayFrame::default() },
        }
    }

    #[test]
    fn spacing_is_a_gap_between_boxes_on_the_grid() {
        let (doc, obj) = doc_with_box();
        let plan = doc.array_plan(&[obj], &spec([3, 0, 0], 10.0, 16.0), &|_| None).expect("plan");
        assert_eq!(plan.steps.x, 112.0);
        assert_eq!(plan.deltas.len(), 3);
        assert_eq!(plan.deltas[0], DVec3::new(112.0, 0.0, 0.0));
        let exact = doc.array_plan(&[obj], &spec([2, 0, 0], 10.0, 0.0), &|_| None).expect("plan");
        assert_eq!(exact.steps.x, 110.0);
    }

    #[test]
    fn several_axes_make_a_grid_and_one_undo_step() {
        let (mut doc, obj) = doc_with_box();
        let before = doc.history_len().0;
        let (plan, copies) = doc.array_copies(&[obj], &spec([2, 1, 1], 0.0, 8.0), &|_| None, None).expect("array");
        assert_eq!(plan.deltas.len(), 11);
        assert_eq!(copies.len(), 11);
        assert_eq!(doc.scene().solids().len(), 12);
        assert_eq!(doc.history_len().0, before + 1);
        assert!(doc.undo());
        assert_eq!(doc.scene().solids().len(), 1);
    }

    #[test]
    fn rotated_frames_follow_their_axes() {
        let (doc, obj) = doc_with_box();
        let mut s = spec([1, 0, 0], 0.0, 0.0);
        s.frame.axes = DMat3::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let plan = doc.array_plan(&[obj], &s, &|_| None).expect("plan");
        assert!((plan.steps.x - 40.0).abs() < 1e-6, "{plan:?}");
        assert!((plan.deltas[0] - DVec3::new(0.0, 40.0, 0.0)).length() < 1e-6, "{plan:?}");
    }

    #[test]
    fn invalid_specs_are_refused() {
        let (doc, obj) = doc_with_box();
        assert!(doc.array_plan(&[obj], &spec([0, 0, 0], 0.0, 0.0), &|_| None).is_err());
        assert!(doc.array_plan(&[obj], &spec([100, 100, 100], 0.0, 0.0), &|_| None).is_err());
        assert!(doc.array_plan(&[obj], &spec([1, 0, 0], -100.0, 0.0), &|_| None).is_err());
        assert!(doc.array_plan(&[], &spec([1, 0, 0], 0.0, 0.0), &|_| None).is_err());
    }
}
