use std::collections::HashSet;

use hp_geom::{CLIP_EPS, NORMAL_EPS, Plane, texture_axes_valid};
use hp_vmf::Solid;

use crate::document::Document;
use crate::ids::{IdSet, id_set};
use crate::limits::checks::{
    CheckKnowledge, CheckOptions, MaterialFacts, OVERLAY_CLASSES, check_snapshot, target_exists,
};
use crate::limits::snapshot::MapSnapshot;
use crate::scene::Owner;
use crate::selection::{FaceRef, ObjRef};
use crate::visibility::find_visgroup;
use crate::{EditError, EditResult, Id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub objects: Vec<ObjRef>,
    pub faces: Vec<FaceRef>,
    pub fixable: bool,
    pub args: Vec<String>,
}

impl Problem {
    pub fn new(severity: Severity, code: &'static str, message: String, objects: Vec<ObjRef>, fixable: bool) -> Self {
        Self { severity, code, message, objects, faces: Vec::new(), fixable, args: Vec::new() }
    }

    #[must_use]
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn arg(&self, i: usize) -> &str {
        self.args.get(i).map_or("", String::as_str)
    }

    pub fn key(&self) -> String {
        let mut ids: Vec<String> = self
            .objects
            .iter()
            .take(8)
            .map(|o| match o {
                ObjRef::Solid(id) => format!("s{id}"),
                ObjRef::Entity(id) => format!("e{id}"),
            })
            .collect();
        ids.sort();
        if ids.is_empty() { self.code.to_owned() } else { format!("{}:{}", self.code, ids.join(",")) }
    }
}

pub type BrushClassFn<'a> = &'a dyn Fn(&str) -> Option<bool>;

#[derive(Clone, Copy, Default)]
pub struct ProblemContext<'a> {
    pub class_exists: Option<&'a dyn Fn(&str) -> bool>,
    pub is_brush_class: Option<BrushClassFn<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialKind {
    Missing,
    Tool,
    Water,
    Regular,
}

pub type MaterialKindFn<'a> = &'a dyn Fn(&str) -> Option<MaterialKind>;

pub type TargetKeyFn<'a> = &'a dyn Fn(&str, &str) -> Option<bool>;

#[derive(Clone, Copy, Default)]
pub struct ProblemAssets<'a> {
    pub model_exists: Option<&'a dyn Fn(&str) -> bool>,
    pub material_kind: Option<MaterialKindFn<'a>>,
    pub is_target_key: Option<TargetKeyFn<'a>>,
}

struct Closures<'a, 'b> {
    ctx: &'b ProblemContext<'a>,
    assets: &'b ProblemAssets<'a>,
}

impl CheckKnowledge for Closures<'_, '_> {
    fn class_exists(&self, class: &str) -> Option<bool> {
        self.ctx.class_exists.map(|f| f(class))
    }
    fn is_brush_class(&self, class: &str) -> Option<bool> {
        self.ctx.is_brush_class.and_then(|f| f(class))
    }
    fn is_target_key(&self, class: &str, key: &str) -> Option<bool> {
        self.assets.is_target_key.and_then(|f| f(class, key))
    }
    fn material(&self, name: &str) -> Option<MaterialFacts> {
        let kind = self.assets.material_kind.and_then(|f| f(name))?;
        Some(MaterialFacts::from_kind(name, kind))
    }
    fn model_exists(&self, path: &str) -> Option<bool> {
        self.assets.model_exists.map(|f| f(path))
    }
}

pub fn parent_name(value: &str) -> &str {
    value.split_once(',').map_or(value, |(name, _)| name).trim()
}

const DETAIL_CLASS: &str = "func_detail";

impl Document {
    pub fn check_problems(&self, class_exists: Option<&dyn Fn(&str) -> bool>) -> Vec<Problem> {
        self.check_problems_with(&ProblemContext { class_exists, is_brush_class: None })
    }

    pub fn check_problems_with(&self, ctx: &ProblemContext<'_>) -> Vec<Problem> {
        self.check_problems_full(ctx, &ProblemAssets::default())
    }

    pub fn check_problems_full(&self, ctx: &ProblemContext<'_>, assets: &ProblemAssets<'_>) -> Vec<Problem> {
        let snap = MapSnapshot::capture(self);
        check_snapshot(&snap, &Closures { ctx, assets }, &CheckOptions::default())
    }

    fn duplicate_planes(&self, solids: Option<&IdSet>) -> Vec<FaceRef> {
        let mut out = Vec::new();
        for n in self.scene.solids.values() {
            if solids.is_some_and(|s| !s.contains(&n.solid.id)) {
                continue;
            }
            let planes: Vec<Option<Plane>> = n.solid.sides.iter().map(Plane::from_side).collect();
            for (i, p) in planes.iter().enumerate() {
                let Some(p) = p else { continue };
                let dup = planes.iter().take(i).flatten().any(|e| p.approx_eq(e, NORMAL_EPS, CLIP_EPS));
                if dup {
                    out.push(FaceRef::new(n.solid.id, i));
                }
            }
        }
        out
    }

    fn all_side_ids(&self) -> HashSet<Id> {
        self.scene.solids.values().flat_map(|n| n.solid.sides.iter().map(|s| s.id)).collect()
    }

    fn bad_texture_axes(&self, solids: Option<&IdSet>) -> Vec<FaceRef> {
        let mut out = Vec::new();
        for n in self.scene.solids.values() {
            if solids.is_some_and(|s| !s.contains(&n.solid.id)) {
                continue;
            }
            for (i, s) in n.solid.sides.iter().enumerate() {
                if let Some(p) = Plane::from_side(s)
                    && !texture_axes_valid(s, p.normal)
                {
                    out.push(FaceRef::new(n.solid.id, i));
                }
            }
        }
        out
    }

    fn target_names(&self) -> HashSet<String> {
        let mut names = HashSet::with_capacity(self.scene.entities.len() * 2);
        for n in self.scene.entities.values() {
            if let Some(t) = n.entity.get("targetname") {
                names.insert(t.to_ascii_lowercase());
            }
            names.insert(n.entity.classname.to_ascii_lowercase());
        }
        names
    }

    fn problem_solids(&self, problem: &Problem, keep: impl Fn(&crate::SolidNode) -> bool) -> Vec<ObjRef> {
        problem
            .objects
            .iter()
            .copied()
            .filter(|o| matches!(o, ObjRef::Solid(id) if self.scene.solids.get(id).is_some_and(&keep)))
            .collect()
    }

    pub fn fix_problem(&mut self, problem: &Problem) -> EditResult<bool> {
        if !problem.fixable {
            return Ok(false);
        }
        match problem.code {
            "invalid_solid" => {
                let objs: Vec<ObjRef> = problem
                    .objects
                    .iter()
                    .copied()
                    .filter(
                        |o| matches!(o, ObjRef::Solid(id) if self.scene.solids.get(id).is_some_and(|n| !n.geom.valid)),
                    )
                    .collect();
                Ok(self.tx("Fix problems", |doc| doc.delete(&objs))? > 0)
            }
            "brush_entity_without_solids" => {
                let objs: Vec<ObjRef> = problem
                    .objects
                    .iter()
                    .copied()
                    .filter(|o| matches!(o, ObjRef::Entity(id) if !self.scene.is_brush_entity(*id)))
                    .collect();
                Ok(self.tx("Fix problems", |doc| doc.delete(&objs))? > 0)
            }
            "missing_output_target" => {
                let names = self.target_names();
                let reported: HashSet<String> = problem.args.iter().map(|a| a.trim().to_ascii_lowercase()).collect();
                let keep = |target: &str| {
                    target_exists(&names, target)
                        || (!reported.is_empty() && !reported.contains(&target.trim().to_ascii_lowercase()))
                };
                let world_id = self.scene.world.id;
                let mut changed = false;
                self.tx("Fix problems", |doc| {
                    for o in &problem.objects {
                        let ObjRef::Entity(id) = *o else { continue };
                        if doc.scene.entities.contains_key(&id) {
                            let before = doc.scene.entities.get(&id).map_or(0, |n| n.entity.connections.len());
                            doc.update_entity(id, |e| e.connections.retain(|c| keep(&c.target)));
                            let after = doc.scene.entities.get(&id).map_or(0, |n| n.entity.connections.len());
                            changed |= after != before;
                        } else if id == world_id && doc.scene.world.connections.iter().any(|c| !keep(&c.target)) {
                            doc.world_mut().connections.retain(|c| keep(&c.target));
                            changed = true;
                        }
                    }
                    Ok(())
                })?;
                Ok(changed)
            }
            "missing_target" => {
                let pairs: Vec<(&str, &str)> = problem.args.iter().filter_map(|a| a.split_once('=')).collect();
                let mut changed = false;
                self.tx("Fix problems", |doc| {
                    for o in &problem.objects {
                        let ObjRef::Entity(id) = *o else { continue };
                        let Some(n) = doc.scene.entities.get(&id) else { continue };
                        let stale: Vec<String> = pairs
                            .iter()
                            .filter(|(k, v)| n.entity.get(k) == Some(*v))
                            .map(|(k, _)| (*k).to_owned())
                            .collect();
                        if stale.is_empty() {
                            continue;
                        }
                        doc.update_entity(id, |e| {
                            for k in &stale {
                                e.set(k, "");
                            }
                        });
                        changed = true;
                    }
                    Ok(())
                })?;
                Ok(changed)
            }
            "disp_not_quad" => {
                let faces: Vec<FaceRef> = problem.faces.clone();
                Ok(self.tx("Fix problems", |doc| doc.destroy_displacements(&faces))? > 0)
            }
            "duplicate_side_id" => Ok(self.renumber_duplicate_sides() > 0),
            "duplicate_object_id" => Ok(self.renumber_shared_object_ids() > 0),
            "missing_group" => {
                let objs = problem.objects.clone();
                self.fix_editor_refs(&objs, true)
            }
            "missing_visgroup" => {
                let objs = problem.objects.clone();
                self.fix_editor_refs(&objs, false)
            }
            "duplicate_plane" => {
                let solids: IdSet = problem.faces.iter().map(|f| f.solid).collect();
                let faces = self.duplicate_planes(Some(&solids));
                let mut by_solid: Vec<(Id, Solid)> = Vec::new();
                for f in faces.iter().rev() {
                    let pos = match by_solid.iter().position(|(id, _)| *id == f.solid) {
                        Some(p) => p,
                        None => match self.scene.solids.get(&f.solid) {
                            Some(n) => {
                                by_solid.push((f.solid, n.solid.clone()));
                                by_solid.len() - 1
                            }
                            None => continue,
                        },
                    };
                    if let Some((_, s)) = by_solid.get_mut(pos)
                        && f.side < s.sides.len()
                    {
                        s.sides.remove(f.side);
                    }
                }
                if by_solid.is_empty() {
                    return Ok(false);
                }
                self.tx("Fix problems", |doc| {
                    doc.update_solids(by_solid);
                    doc.deselect_missing();
                    Ok(true)
                })
            }
            "overlay_missing_faces" => {
                let sides = self.all_side_ids();
                let mut changes: Vec<(Id, Vec<(String, String)>)> = Vec::new();
                let mut remove: Vec<ObjRef> = Vec::new();
                for o in &problem.objects {
                    let ObjRef::Entity(id) = *o else { continue };
                    let Some(n) = self.scene.entities.get(&id) else { continue };
                    let e = &n.entity;
                    let overlay = OVERLAY_CLASSES.iter().any(|c| e.classname.eq_ignore_ascii_case(c));
                    let mut kept_any = false;
                    let mut keys = Vec::new();
                    for (k, v) in &e.props {
                        if !(k.eq_ignore_ascii_case("sides") || k.eq_ignore_ascii_case("sides2")) {
                            continue;
                        }
                        let kept: Vec<&str> = v
                            .split_ascii_whitespace()
                            .filter(|t| t.parse::<Id>().is_ok_and(|id| sides.contains(&id)))
                            .collect();
                        kept_any |= !kept.is_empty();
                        let new = kept.join(" ");
                        if new != *v {
                            keys.push((k.clone(), new));
                        }
                    }
                    if overlay && !kept_any {
                        remove.push(ObjRef::Entity(id));
                    } else if !keys.is_empty() {
                        changes.push((id, keys));
                    }
                }
                if changes.is_empty() && remove.is_empty() {
                    return Ok(false);
                }
                self.tx("Fix problems", |doc| {
                    for (id, keys) in changes {
                        doc.update_entity(id, |e| {
                            for (k, v) in keys {
                                e.set(&k, v);
                            }
                        });
                    }
                    doc.delete(&remove)?;
                    Ok(true)
                })
            }
            "invalid_texture_axes" => {
                let solids: IdSet = problem.faces.iter().map(|f| f.solid).collect();
                let faces = self.bad_texture_axes(Some(&solids));
                if faces.is_empty() {
                    return Ok(false);
                }
                let edit = crate::FaceEdit { align: Some(hp_geom::TextureAlign::World), ..crate::FaceEdit::default() };
                Ok(self.tx("Fix problems", |doc| doc.edit_faces(&faces, &edit))? > 0)
            }
            "disp_on_brush_entity" => {
                let objs = self.problem_solids(problem, |n| matches!(n.owner, Owner::Entity(_)) && n.has_disp());
                if objs.is_empty() {
                    return Ok(false);
                }
                Ok(self.tx("Fix problems", |doc| doc.move_to_world(&objs))? > 0)
            }
            "micro_brush" => {
                let objs = self
                    .problem_solids(problem, |n| n.geom.valid && n.geom.volume() < crate::limits::checks::MICRO_VOLUME);
                if objs.is_empty() {
                    return Ok(false);
                }
                Ok(self.tx("Fix problems", |doc| doc.delete(&objs))? > 0)
            }
            "origin_brush_in_world" => {
                let objs = self.problem_solids(problem, |n| {
                    n.owner == Owner::World
                        && n.solid.sides.iter().any(|s| crate::glob::same_material(&s.material, "tools/toolsorigin"))
                });
                if objs.is_empty() {
                    return Ok(false);
                }
                Ok(self.tx("Fix problems", |doc| doc.delete(&objs))? > 0)
            }
            "detail_candidate" => {
                let objs = self.problem_solids(problem, |n| n.owner == Owner::World);
                if objs.is_empty() {
                    return Ok(false);
                }
                self.tx("Convert to func_detail", |doc| doc.tie_to_entity(&objs, DETAIL_CLASS, &[]))?;
                Ok(true)
            }
            "areaportal_door_state" => {
                let Some(ObjRef::Entity(portal)) = problem.objects.first().copied() else { return Ok(false) };
                if !self.scene.entities.contains_key(&portal) {
                    return Ok(false);
                }
                let open = if problem.arg(2) == "1" { "0" } else { "1" };
                self.tx("Fix problems", |doc| {
                    doc.update_entity(portal, |e| e.set("StartOpen", open));
                    Ok(true)
                })
            }
            crate::duplicates::CODE_BRUSH | crate::duplicates::CODE_CONTAINED | crate::duplicates::CODE_ENTITY => {
                let objs = problem.objects.clone();
                Ok(self.delete_redundant(&objs)? > 0)
            }
            other => Err(EditError::Invalid(format!("no fix for problem '{other}'"))),
        }
    }

    pub fn fix_problems(&mut self, problems: &[Problem]) -> EditResult<usize> {
        self.tx("Fix problems", |doc| {
            let mut n = 0;
            for p in problems {
                if doc.fix_problem(p)? {
                    n += 1;
                }
            }
            Ok(n)
        })
    }

    fn fix_editor_refs(&mut self, objs: &[ObjRef], groups: bool) -> EditResult<bool> {
        let scene = &self.scene;
        let fix = |e: &mut hp_vmf::EditorInfo| {
            if groups {
                if e.group_id.is_some_and(|g| !scene.groups.contains_key(&g)) {
                    e.group_id = None;
                }
            } else {
                e.visgroup_ids.retain(|v| find_visgroup(&scene.visgroups, *v).is_some());
                if e.visgroup_ids.is_empty() {
                    e.visgroup_shown = true;
                }
            }
        };
        let mut solids: Vec<(Id, Solid)> = Vec::new();
        let mut entities: Vec<(Id, hp_vmf::EditorInfo)> = Vec::new();
        for o in objs {
            match *o {
                ObjRef::Solid(id) => {
                    if let Some(n) = scene.solids.get(&id) {
                        let mut s = n.solid.clone();
                        fix(&mut s.editor);
                        if s.editor != n.solid.editor {
                            solids.push((id, s));
                        }
                    }
                }
                ObjRef::Entity(id) => {
                    if let Some(n) = scene.entities.get(&id) {
                        let mut e = n.entity.editor.clone();
                        fix(&mut e);
                        if e != n.entity.editor {
                            entities.push((id, e));
                        }
                    }
                }
            }
        }
        if solids.is_empty() && entities.is_empty() {
            return Ok(false);
        }
        self.tx("Fix problems", |doc| {
            doc.update_solids(solids);
            for (id, ed) in entities {
                doc.update_entity(id, |e| e.editor = ed);
            }
            Ok(true)
        })
    }

    pub fn renumber_duplicate_sides(&mut self) -> usize {
        let mut seen: HashSet<Id> = HashSet::new();
        let mut updates: Vec<(Id, Solid)> = Vec::new();
        let mut count = 0;
        let mut next = self.next_side_id;
        for n in self.scene.solids.values() {
            let mut solid: Option<Solid> = None;
            for (i, s) in n.solid.sides.iter().enumerate() {
                if seen.insert(s.id) {
                    continue;
                }
                let sol = solid.get_or_insert_with(|| n.solid.clone());
                if let Some(side) = sol.sides.get_mut(i) {
                    side.id = next;
                    seen.insert(next);
                    next = next.saturating_add(1);
                    count += 1;
                }
            }
            if let Some(s) = solid {
                updates.push((n.solid.id, s));
            }
        }
        self.next_side_id = next;
        if updates.is_empty() {
            return 0;
        }
        let _ = self.tx("Fix problems", |doc| {
            doc.update_solids(updates);
            Ok(())
        });
        count
    }

    pub fn renumber_shared_object_ids(&mut self) -> usize {
        let mut used: IdSet = id_set(self.scene.solids.len() + 1);
        used.insert(self.scene.world.id);
        used.extend(self.scene.solids.keys().copied());
        let ent_dups: Vec<Id> = self.scene.entities.keys().copied().filter(|id| used.contains(id)).collect();
        let used_with_entities: IdSet = used.iter().copied().chain(self.scene.entities.keys().copied()).collect();
        let group_dups: Vec<Id> =
            self.scene.groups.keys().copied().filter(|id| used_with_entities.contains(id)).collect();
        if ent_dups.is_empty() && group_dups.is_empty() {
            return 0;
        }
        let count = ent_dups.len() + group_dups.len();
        let _ = self.tx("Fix problems", |doc| {
            for old in ent_dups {
                let Some(node) = doc.scene.entities.get(&old).cloned() else { continue };
                let new = doc.alloc_object_id();
                let solids = doc.scene.entity_solid_ids(old);
                let mut entity = node.entity.clone();
                entity.id = new;
                doc.remove_entities(&[old]);
                doc.add_entity(entity);
                doc.set_solid_owners(&solids, Owner::Entity(new));
                if doc.selection.objects.contains(&ObjRef::Entity(old)) {
                    doc.select(&[ObjRef::Entity(new)], crate::SelectOp::Add);
                }
            }
            for old in group_dups {
                let Some(node) = doc.scene.groups.get(&old).cloned() else { continue };
                let new = doc.alloc_object_id();
                let mut g = node.group.clone();
                g.id = new;
                let members = doc.scene.members_of_groups(&std::iter::once(old).collect());
                let children: Vec<hp_vmf::Group> = doc
                    .scene
                    .groups
                    .values()
                    .filter(|n| n.group.editor.group_id == Some(old) && n.group.id != old)
                    .map(|n| {
                        let mut c = n.group.clone();
                        c.editor.group_id = Some(new);
                        c
                    })
                    .collect();
                doc.remove_groups(&[old]);
                doc.put_group(g);
                for c in children {
                    doc.put_group(c);
                }
                for m in members {
                    match m {
                        ObjRef::Solid(id) => {
                            if let Some(n) = doc.scene.solids.get(&id) {
                                let mut s = n.solid.clone();
                                s.editor.group_id = Some(new);
                                doc.update_solid(id, s);
                            }
                        }
                        ObjRef::Entity(id) => {
                            doc.update_entity(id, |e| e.editor.group_id = Some(new));
                        }
                    }
                }
            }
            doc.deselect_missing();
            Ok(())
        });
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::checks::guess_brush_class;

    #[test]
    fn helpers() {
        assert!(guess_brush_class("func_door"));
        assert!(guess_brush_class("TRIGGER_MULTIPLE"));
        assert!(!guess_brush_class("func_instance"));
        assert!(!guess_brush_class("info_target"));
        let names: HashSet<String> = ["door1".to_owned(), "logic_relay".to_owned()].into_iter().collect();
        assert!(target_exists(&names, "Door1"));
        assert!(target_exists(&names, "door*"));
        assert!(target_exists(&names, "!activator"));
        assert!(target_exists(&names, "logic_relay"));
        assert!(!target_exists(&names, "door2"));
    }

    #[test]
    fn problem_keys() {
        let p = Problem::new(
            Severity::Warning,
            "micro_brush",
            String::new(),
            vec![ObjRef::Solid(9), ObjRef::Entity(3)],
            true,
        );
        assert_eq!(p.key(), "micro_brush:e3,s9");
        let q = Problem::new(Severity::Warning, "no_player_spawn", String::new(), Vec::new(), false);
        assert_eq!(q.key(), "no_player_spawn");
    }
}
