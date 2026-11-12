use std::collections::HashSet;
use std::time::Instant;

use glam::DVec3;

use crate::EditResult;
use crate::changes::Changes;
use crate::document::Document;
use crate::problems::Problem;
use crate::selection::ObjRef;

pub const PLACEMENT_CODES: &[&str] = &["floating_object", "sunk_object", "overlapping_objects"];

pub const ARG_WHAT: usize = 0;
pub const ARG_DISTANCE: usize = 1;
pub const ARG_OTHER: usize = 2;
pub const ARG_MOVE: usize = 3;
pub const ARG_CHAIN: usize = 4;
pub const ARG_FINGERPRINT: usize = 5;
pub const ARG_DEPS: usize = 6;

const AMEND_TAG: &str = "\u{1}amend-last-step";
const FINGERPRINT_TOLERANCE: f64 = 0.05;

pub fn is_placement_code(code: &str) -> bool {
    PLACEMENT_CODES.contains(&code)
}

pub fn object_key(o: ObjRef) -> String {
    match o {
        ObjRef::Solid(id) => format!("s{id}"),
        ObjRef::Entity(id) => format!("e{id}"),
    }
}

fn parse_key(s: &str) -> Option<ObjRef> {
    let (kind, id) = s.split_at_checked(1)?;
    let id = id.parse().ok()?;
    match kind {
        "s" => Some(ObjRef::Solid(id)),
        "e" => Some(ObjRef::Entity(id)),
        _ => None,
    }
}

pub fn fmt_move(v: DVec3) -> String {
    hp_vmf::fmt_vec3(v)
}

impl Document {
    pub fn last_step_label(&self) -> Option<&str> {
        self.history.undo.back().filter(|e| !e.selection_only).map(|e| e.label.as_str())
    }

    pub fn amend_last_step<R>(
        &mut self,
        fallback_label: &str,
        f: impl FnOnce(&mut Self) -> EditResult<R>,
    ) -> EditResult<R> {
        if self.history.tx.is_some() {
            return f(self);
        }
        let can = self.history.redo.is_empty() && self.history.undo.back().is_some_and(|t| !t.selection_only);
        if !can {
            return self.tx(fallback_label, f);
        }
        let (old, label) = match self.history.undo.back_mut() {
            Some(top) => {
                top.time = Instant::now();
                (top.merge_tag.replace(AMEND_TAG.to_owned()), top.label.clone())
            }
            None => return self.tx(fallback_label, f),
        };
        self.begin_merge(label, AMEND_TAG);
        let result = f(self);
        match &result {
            Ok(_) => {
                self.commit();
            }
            Err(_) => self.cancel(),
        }
        if let Some(top) = self.history.undo.back_mut()
            && top.merge_tag.as_deref() == Some(AMEND_TAG)
        {
            top.merge_tag = old;
        }
        result
    }

    pub fn restore_changes(&mut self, earlier: Changes) {
        let newer = std::mem::replace(&mut self.changes, earlier);
        for id in newer.solids_changed {
            self.changes.solid_changed(id);
        }
        for id in newer.solids_removed {
            self.changes.solid_removed(id);
        }
        for id in newer.entities_changed {
            self.changes.entity_changed(id);
        }
        for id in newer.entities_removed {
            self.changes.entity_removed(id);
        }
        self.changes.groups_changed |= newer.groups_changed;
        self.changes.visgroups_changed |= newer.visgroups_changed;
        self.changes.world_changed |= newer.world_changed;
        self.changes.extra_blocks_changed |= newer.extra_blocks_changed;
        self.changes.selection_changed |= newer.selection_changed;
        self.changes.visibility_changed |= newer.visibility_changed;
        self.changes.everything |= newer.everything;
    }

    pub fn placement_fingerprint(&self, o: ObjRef) -> Option<DVec3> {
        match o {
            ObjRef::Entity(id) => {
                let n = self.scene.entities.get(&id)?;
                match n.entity.origin().filter(|p| p.is_finite()) {
                    Some(p) => Some(p),
                    None => {
                        let b = self
                            .scene
                            .entity_solid_ids(id)
                            .into_iter()
                            .filter_map(|s| self.scene.solids.get(&s).map(|n| n.bounds))
                            .fold(hp_geom::Aabb::EMPTY, |a, b| a.union(&b));
                        (!b.is_empty()).then_some(b.min)
                    }
                }
            }
            ObjRef::Solid(id) => self.scene.solids.get(&id).map(|n| n.bounds.min).filter(|p| p.is_finite()),
        }
    }

    pub(crate) fn fix_placement(&mut self, p: &Problem, batch: Option<&HashSet<ObjRef>>) -> EditResult<bool> {
        let Some(&target) = p.objects.first() else { return Ok(false) };
        if !self.scene.contains(target) {
            return Ok(false);
        }
        let Some(stored) = hp_vmf::parse_vec3(p.arg(ARG_FINGERPRINT)) else { return Ok(false) };
        let Some(now) = self.placement_fingerprint(target) else { return Ok(false) };
        if (now - stored).abs().max_element() > FINGERPRINT_TOLERANCE {
            return Ok(false);
        }
        let deps: Vec<ObjRef> = p.args.iter().skip(ARG_DEPS).filter_map(|s| parse_key(s)).collect();
        let chain = batch.is_some_and(|b| !deps.is_empty() && deps.iter().all(|d| b.contains(d)));
        let which = if chain { ARG_CHAIN } else { ARG_MOVE };
        let Some(d) = hp_vmf::parse_vec3(p.arg(which)).filter(|d| d.is_finite()) else { return Ok(false) };
        if d.length_squared() < 1e-12 {
            return Ok(false);
        }
        self.tx("Fix problems", |doc| doc.translate(&[target], d))?;
        Ok(true)
    }

    pub(crate) fn placement_batch(problems: &[Problem]) -> HashSet<ObjRef> {
        problems
            .iter()
            .filter(|p| p.fixable && is_placement_code(p.code))
            .filter_map(|p| p.objects.first().copied())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problems::Severity;

    fn prop(doc: &mut Document, z: f64) -> u32 {
        let kv = [("model".to_owned(), "models/a.mdl".to_owned())];
        doc.create_point_entity("prop_static", DVec3::new(0.0, 0.0, z), &kv).expect("prop")
    }

    fn floating(id: u32, z: f64, solo: f64, chain: f64, deps: &[u32]) -> Problem {
        let mut args = vec![
            format!("prop_static #{id}"),
            "1".to_owned(),
            "worldspawn".to_owned(),
            fmt_move(DVec3::new(0.0, 0.0, -solo)),
            fmt_move(DVec3::new(0.0, 0.0, -chain)),
            fmt_move(DVec3::new(0.0, 0.0, z)),
        ];
        args.extend(deps.iter().map(|d| object_key(ObjRef::Entity(*d))));
        Problem::new(Severity::Warning, "floating_object", String::new(), vec![ObjRef::Entity(id)], true)
            .with_args(args)
    }

    #[test]
    fn floating_fix_moves_by_the_stored_delta_once() {
        let mut doc = Document::new_empty();
        let a = prop(&mut doc, 40.0);
        let p = floating(a, 40.0, 24.0, 24.0, &[]);
        assert!(doc.fix_problem(&p).expect("fix"));
        assert_eq!(doc.scene().entity(a).and_then(|n| n.entity.origin()), Some(DVec3::new(0.0, 0.0, 16.0)));
        assert!(!doc.fix_problem(&p).expect("stale"), "a moved object is not moved again");
    }

    #[test]
    fn stacked_objects_use_the_chain_delta_only_with_their_support() {
        let mut doc = Document::new_empty();
        let low = prop(&mut doc, 20.0);
        let high = prop(&mut doc, 60.0);
        let lp = floating(low, 20.0, 20.0, 20.0, &[]);
        let hp = floating(high, 60.0, 8.0, 28.0, &[low]);
        let n = doc.fix_problems(&[lp.clone(), hp.clone()]).expect("fix all");
        assert_eq!(n, 2);
        assert_eq!(doc.scene().entity(high).and_then(|n| n.entity.origin()), Some(DVec3::new(0.0, 0.0, 32.0)));
        assert!(doc.undo());
        assert_eq!(doc.scene().entity(high).and_then(|n| n.entity.origin()), Some(DVec3::new(0.0, 0.0, 60.0)));
        assert_eq!(doc.scene().entity(low).and_then(|n| n.entity.origin()), Some(DVec3::new(0.0, 0.0, 20.0)));
        assert_eq!(doc.fix_problems(&[hp]).expect("alone"), 1);
        assert_eq!(doc.scene().entity(high).and_then(|n| n.entity.origin()), Some(DVec3::new(0.0, 0.0, 52.0)));
    }

    #[test]
    fn amend_merges_into_the_last_step() {
        let mut doc = Document::new_empty();
        let a = prop(&mut doc, 40.0);
        let steps = doc.undo_labels().len();
        doc.amend_last_step("Drop", |doc| doc.translate(&[ObjRef::Entity(a)], DVec3::new(0.0, 0.0, -8.0)))
            .expect("amend");
        assert_eq!(doc.undo_labels().len(), steps);
        assert_eq!(doc.last_step_label(), Some("Create entity"));
        assert!(doc.undo());
        assert!(doc.scene().entity(a).is_none(), "one undo removes the entity and its drop");
    }

    #[test]
    fn restored_changes_keep_both_sets() {
        let mut doc = Document::new_empty();
        let a = prop(&mut doc, 0.0);
        let earlier = doc.take_changes();
        assert!(earlier.entities_changed.contains(&a));
        let b = prop(&mut doc, 8.0);
        let mid = doc.pending_changes().entities_changed.clone();
        assert!(mid.contains(&b) && !mid.contains(&a));
        doc.restore_changes(earlier);
        assert!(doc.pending_changes().entities_changed.contains(&a));
        assert!(doc.pending_changes().entities_changed.contains(&b));
    }
}
