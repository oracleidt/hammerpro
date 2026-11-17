use std::collections::HashMap;

use hp_vmf::{Connection, Entity};

use crate::copy_names::COMMON_TARGET_KEYS;
use crate::document::Document;
use crate::glob::glob_match;
use crate::problems::TargetKeyFn;
use crate::{EditError, EditResult, Id};

pub const RENAME_LABEL: &str = "Rename entity";

pub const NAME_INPUTS: &[&str] = &[
    "setparent",
    "setdamagefilter",
    "settarget",
    "setmeasuretarget",
    "setmeasurereference",
    "settargetentity",
    "setlightingorigin",
];

#[derive(Clone, Copy, Default)]
pub struct RenameRules<'a> {
    pub is_target_key: Option<TargetKeyFn<'a>>,
    pub input_takes_name: Option<TargetKeyFn<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefsKept {
    SharedName,
    ClassName,
    Special,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenameReport {
    pub renamed: usize,
    pub references: usize,
    pub wildcards: usize,
    pub kept: Option<RefsKept>,
    pub kept_references: usize,
}

#[derive(Default)]
struct EntityFix {
    keys: Vec<(String, String)>,
    conns: Vec<(usize, Option<String>, Option<String>)>,
}

impl EntityFix {
    fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.conns.is_empty()
    }

    fn count(&self) -> usize {
        self.keys.len()
            + self.conns.iter().map(|(_, t, p)| usize::from(t.is_some()) + usize::from(p.is_some())).sum::<usize>()
    }
}

struct Plan {
    fixes: Vec<(Id, EntityFix)>,
    wildcards: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fixup {
    Prefix,
    Postfix,
}

struct Names<'a> {
    old: &'a str,
    new: &'a str,
    fixups: Vec<Fixup>,
}

enum Remap {
    New(String),
    Wildcard,
    Same,
}

impl Names<'_> {
    fn remap(&self, value: &str) -> Remap {
        let v = value.trim();
        if v.eq_ignore_ascii_case(self.old) {
            return Remap::New(self.new.to_owned());
        }
        for f in &self.fixups {
            match f {
                Fixup::Prefix => {
                    let head = format!("{}-", self.old);
                    if v.len() > head.len()
                        && v.is_char_boundary(head.len())
                        && v[..head.len()].eq_ignore_ascii_case(&head)
                    {
                        return Remap::New(format!("{}-{}", self.new, &v[head.len()..]));
                    }
                }
                Fixup::Postfix => {
                    let tail = format!("-{}", self.old);
                    let cut = v.len().saturating_sub(tail.len());
                    if v.len() > tail.len() && v.is_char_boundary(cut) && v[cut..].eq_ignore_ascii_case(&tail) {
                        return Remap::New(format!("{}-{}", &v[..cut], self.new));
                    }
                }
            }
        }
        if v.contains('*') {
            let (o, n, p) = (self.old.to_ascii_lowercase(), self.new.to_ascii_lowercase(), v.to_ascii_lowercase());
            if glob_match(&p, &o) && !glob_match(&p, &n) {
                return Remap::Wildcard;
            }
        }
        Remap::Same
    }
}

fn fixup_of(e: &Entity) -> Option<Fixup> {
    if !e.classname.eq_ignore_ascii_case("func_instance") {
        return None;
    }
    match e.get("fixup_style").map(str::trim) {
        None | Some("" | "0") => Some(Fixup::Prefix),
        Some("1") => Some(Fixup::Postfix),
        _ => None,
    }
}

fn is_instance_replace(e: &Entity, key: &str) -> bool {
    e.classname.eq_ignore_ascii_case("func_instance")
        && key.len() > 7
        && key[..7].eq_ignore_ascii_case("replace")
        && key[7..].bytes().all(|b| b.is_ascii_digit())
}

fn key_names_entity(rules: &RenameRules<'_>, class: &str, key: &str) -> bool {
    if key.eq_ignore_ascii_case("targetname") || key.eq_ignore_ascii_case("classname") {
        return false;
    }
    rules
        .is_target_key
        .and_then(|f| f(class, key))
        .unwrap_or_else(|| COMMON_TARGET_KEYS.iter().any(|k| k.eq_ignore_ascii_case(key)))
}

fn input_takes_name(rules: &RenameRules<'_>, classes: &[&str], input: &str) -> bool {
    let input = input.trim();
    if input.eq_ignore_ascii_case("addoutput") {
        return false;
    }
    rules.input_takes_name.is_some_and(|f| classes.iter().any(|c| f(c, input) == Some(true)))
        || NAME_INPUTS.iter().any(|n| n.eq_ignore_ascii_case(input))
}

fn fix_key(e: &Entity, key: &str, value: &str, names: &Names<'_>, wildcards: &mut usize) -> Option<String> {
    if is_instance_replace(e, key) {
        let (var, rest) = value.split_once(' ')?;
        return match names.remap(rest) {
            Remap::New(n) => Some(format!("{var} {n}")),
            _ => None,
        };
    }
    let (name, attach) = match value.split_once(',') {
        Some((n, r)) if key.eq_ignore_ascii_case("parentname") => (n, Some(r)),
        _ => (value, None),
    };
    match names.remap(name) {
        Remap::New(n) => Some(match attach {
            Some(r) => format!("{n},{r}"),
            None => n,
        }),
        Remap::Wildcard => {
            *wildcards += 1;
            None
        }
        Remap::Same => None,
    }
}

impl Document {
    fn classes_by_name(&self) -> HashMap<String, Vec<String>> {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for n in self.scene.entities.values() {
            if let Some(name) = n.entity.get("targetname").map(str::trim).filter(|t| !t.is_empty()) {
                map.entry(name.to_ascii_lowercase()).or_default().push(n.entity.classname.to_ascii_lowercase());
            }
        }
        map
    }

    fn plan_references(&self, names: &Names<'_>, rules: &RenameRules<'_>) -> Plan {
        let classes = self.classes_by_name();
        let mut plan = Plan { fixes: Vec::new(), wildcards: 0 };
        let world = &self.scene.world;
        let mut each = |e: &Entity, is_world: bool| {
            let mut fix = EntityFix::default();
            for (i, c) in e.connections.iter().enumerate() {
                let target = match names.remap(&c.target) {
                    Remap::New(n) => Some(n),
                    Remap::Wildcard => {
                        plan.wildcards += 1;
                        None
                    }
                    Remap::Same => None,
                };
                let param = conn_param(c, e, &classes, names, rules);
                if target.is_some() || param.is_some() {
                    fix.conns.push((i, target, param));
                }
            }
            if !is_world {
                for (key, value) in &e.props {
                    let replace = is_instance_replace(e, key);
                    if !replace && !key_names_entity(rules, &e.classname, key) {
                        continue;
                    }
                    if let Some(v) = fix_key(e, key, value, names, &mut plan.wildcards) {
                        fix.keys.push((key.clone(), v));
                    }
                }
            }
            if !fix.is_empty() {
                plan.fixes.push((e.id, fix));
            }
        };
        each(world, true);
        for n in self.scene.entities.values() {
            each(&n.entity, false);
        }
        plan
    }

    fn rename_plan(
        &self,
        only: Option<&[Id]>,
        old: &str,
        new: &str,
        rules: Option<&RenameRules<'_>>,
    ) -> EditResult<(Vec<Id>, Plan, RenameReport)> {
        if old.is_empty() || new.is_empty() {
            return Err(EditError::Invalid("empty entity name".to_owned()));
        }
        let all_named = self.entities_named(old);
        let named: Vec<Id> = match only {
            Some(ids) => all_named.iter().copied().filter(|id| ids.contains(id)).collect(),
            None => all_named.clone(),
        };
        let mut report = RenameReport { renamed: named.len(), ..RenameReport::default() };
        let mut plan = Plan { fixes: Vec::new(), wildcards: 0 };
        if let Some(rules) = rules
            && !old.eq_ignore_ascii_case(new)
        {
            let fixups = named
                .iter()
                .filter_map(|id| self.scene.entities.get(id))
                .filter_map(|n| fixup_of(&n.entity))
                .fold(Vec::new(), |mut acc, f| {
                    if !acc.contains(&f) {
                        acc.push(f);
                    }
                    acc
                });
            let names = Names { old, new, fixups };
            plan = self.plan_references(&names, rules);
            let kept = if old.starts_with('!') {
                Some(RefsKept::Special)
            } else if named.len() < all_named.len() {
                Some(RefsKept::SharedName)
            } else if self.scene.entities.values().any(|n| n.entity.classname.eq_ignore_ascii_case(old)) {
                Some(RefsKept::ClassName)
            } else {
                None
            };
            if let Some(why) = kept {
                let n: usize = plan.fixes.iter().map(|(_, f)| f.count()).sum();
                if n > 0 {
                    report.kept = Some(why);
                    report.kept_references = n;
                }
                plan.fixes.clear();
            }
            report.wildcards = plan.wildcards;
        }
        report.references = plan.fixes.iter().map(|(_, f)| f.count()).sum();
        let unchanged =
            named.iter().all(|id| self.scene.entities.get(id).and_then(|n| n.entity.get("targetname")) == Some(new));
        if unchanged {
            report.renamed = 0;
        }
        Ok((named, plan, report))
    }

    pub fn rename_preview(
        &self,
        only: Option<&[Id]>,
        old: &str,
        new: &str,
        rules: Option<&RenameRules<'_>>,
    ) -> EditResult<RenameReport> {
        Ok(self.rename_plan(only, old.trim(), new.trim(), rules)?.2)
    }

    pub fn rename_entity_names(
        &mut self,
        only: Option<&[Id]>,
        old: &str,
        new: &str,
        rules: Option<&RenameRules<'_>>,
    ) -> EditResult<RenameReport> {
        let (old, new) = (old.trim(), new.trim());
        let (named, plan, report) = self.rename_plan(only, old, new, rules)?;
        if report.renamed == 0 && plan.fixes.is_empty() {
            return Ok(report);
        }
        let world_id = self.scene.world.id;
        self.tx(RENAME_LABEL, |doc| {
            for id in &named {
                doc.update_entity(*id, |e| e.set("targetname", new));
            }
            for (id, fix) in plan.fixes {
                let apply = |e: &mut Entity| {
                    for (key, value) in &fix.keys {
                        e.set(key, value.clone());
                    }
                    for (index, target, param) in &fix.conns {
                        if let Some(c) = e.connections.get_mut(*index) {
                            if let Some(t) = target {
                                c.target.clone_from(t);
                            }
                            if let Some(p) = param {
                                c.param.clone_from(p);
                            }
                        }
                    }
                };
                if id == world_id && !doc.scene.entities.contains_key(&id) {
                    apply(doc.world_mut());
                } else {
                    doc.update_entity(id, apply);
                }
            }
            Ok(report)
        })
    }
}

fn conn_param(
    c: &Connection,
    owner: &Entity,
    classes: &HashMap<String, Vec<String>>,
    names: &Names<'_>,
    rules: &RenameRules<'_>,
) -> Option<String> {
    let Remap::New(n) = names.remap(&c.param) else { return None };
    if !c.param.trim().eq_ignore_ascii_case(names.old) {
        return None;
    }
    let target = c.target.trim().to_ascii_lowercase();
    let owner_class = owner.classname.to_ascii_lowercase();
    let receivers: Vec<&str> = if target == "!self" || target == "!caller" {
        vec![owner_class.as_str()]
    } else {
        classes.get(&target).map(|v| v.iter().map(String::as_str).collect()).unwrap_or_default()
    };
    input_takes_name(rules, &receivers, &c.input).then_some(n)
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use hp_vmf::Connection;

    use super::*;

    fn ent(doc: &mut Document, class: &str, kv: &[(&str, &str)]) -> Id {
        let kv: Vec<(String, String)> = kv.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        doc.create_point_entity(class, DVec3::ZERO, &kv).expect("entity")
    }

    fn get(doc: &Document, id: Id, key: &str) -> Option<String> {
        doc.scene().entity(id).and_then(|n| n.entity.get(key).map(str::to_owned))
    }

    fn conn(doc: &Document, id: Id, i: usize) -> Connection {
        doc.scene().entity(id).expect("entity").entity.connections[i].clone()
    }

    #[test]
    fn rename_updates_outputs_keys_and_params_in_one_step() {
        let mut doc = Document::new_empty();
        let door = ent(&mut doc, "func_door", &[("targetname", "doora")]);
        let trig = ent(&mut doc, "trigger_multiple", &[("filtername", "doora"), ("parentname", "doora,hand")]);
        let other = ent(&mut doc, "prop_dynamic", &[("targetname", "box")]);
        let outs = [
            "doora,Open,,0,-1",
            "door*,Close,,0,-1",
            "box,SetParent,doora,0,-1",
            "box,AddOutput,doora,0,-1",
            "box,SetText,doora,0,-1",
            "doora,Lock,,0,-1",
        ];
        for o in outs {
            doc.add_connection(&[trig], &Connection::parse("OnStartTouch", o)).expect("output");
        }
        doc.clear_history();
        let steps = doc.history_len().0;
        let rules = RenameRules::default();
        let r = doc.rename_entity_names(Some(&[door]), "doora", "gate", Some(&rules)).expect("rename");
        assert_eq!(doc.history_len().0, steps + 1, "one undo step");
        assert_eq!(r.renamed, 1);
        assert_eq!(r.references, 5);
        assert_eq!(r.wildcards, 1);
        assert_eq!(get(&doc, door, "targetname").as_deref(), Some("gate"));
        assert_eq!(get(&doc, trig, "filtername").as_deref(), Some("gate"));
        assert_eq!(get(&doc, trig, "parentname").as_deref(), Some("gate,hand"));
        assert_eq!(conn(&doc, trig, 0).target, "gate");
        assert_eq!(conn(&doc, trig, 1).target, "door*");
        assert_eq!(conn(&doc, trig, 2).param, "gate");
        assert_eq!(conn(&doc, trig, 3).param, "doora", "AddOutput is never touched");
        assert_eq!(conn(&doc, trig, 4).param, "doora", "text parameters are not names");
        assert_eq!(conn(&doc, trig, 5).target, "gate");
        assert_eq!(get(&doc, other, "targetname").as_deref(), Some("box"));
        assert!(doc.undo());
        assert_eq!(get(&doc, door, "targetname").as_deref(), Some("doora"));
        assert_eq!(conn(&doc, trig, 0).target, "doora");
        assert_eq!(get(&doc, trig, "parentname").as_deref(), Some("doora,hand"));
    }

    #[test]
    fn shared_names_keep_references() {
        let mut doc = Document::new_empty();
        let a = ent(&mut doc, "func_door", &[("targetname", "doora")]);
        let b = ent(&mut doc, "func_door", &[("targetname", "doora")]);
        let t = ent(&mut doc, "logic_relay", &[]);
        doc.add_connection(&[t], &Connection::parse("OnTrigger", "doora,Open,,0,-1")).expect("output");
        let rules = RenameRules::default();
        let r = doc.rename_entity_names(Some(&[a]), "doora", "doorb", Some(&rules)).expect("rename");
        assert_eq!((r.renamed, r.references, r.kept, r.kept_references), (1, 0, Some(RefsKept::SharedName), 1));
        assert_eq!(conn(&doc, t, 0).target, "doora");
        assert_eq!(get(&doc, b, "targetname").as_deref(), Some("doora"));
        let r = doc.rename_entity_names(Some(&[b]), "doora", "doorb", Some(&rules)).expect("rename");
        assert_eq!((r.renamed, r.references, r.kept), (1, 1, None));
        assert_eq!(conn(&doc, t, 0).target, "doorb");
    }

    #[test]
    fn no_rules_renames_only_the_entity() {
        let mut doc = Document::new_empty();
        let a = ent(&mut doc, "func_door", &[("targetname", "doora")]);
        let t = ent(&mut doc, "logic_relay", &[("target", "doora")]);
        let r = doc.rename_entity_names(Some(&[a]), "doora", "doorb", None).expect("rename");
        assert_eq!((r.renamed, r.references), (1, 0));
        assert_eq!(get(&doc, t, "target").as_deref(), Some("doora"));
    }

    #[test]
    fn instance_fixup_names_follow_the_instance() {
        let mut doc = Document::new_empty();
        let inst = ent(&mut doc, "func_instance", &[("targetname", "room1"), ("file", "a.vmf")]);
        let post = ent(&mut doc, "func_instance", &[("targetname", "hall"), ("fixup_style", "1")]);
        let none = ent(&mut doc, "func_instance", &[("targetname", "raw"), ("fixup_style", "2")]);
        let user = ent(&mut doc, "func_instance", &[("replace01", "$door room1"), ("replace02", "$x room1-relay")]);
        let t = ent(&mut doc, "logic_relay", &[]);
        for o in
            ["room1-relay,Trigger,,0,-1", "relay-hall,Trigger,,0,-1", "raw-relay,Trigger,,0,-1", "room1,Kill,,0,-1"]
        {
            doc.add_connection(&[t], &Connection::parse("OnTrigger", o)).expect("output");
        }
        let rules = RenameRules::default();
        doc.rename_entity_names(Some(&[inst]), "room1", "room2", Some(&rules)).expect("rename");
        assert_eq!(conn(&doc, t, 0).target, "room2-relay");
        assert_eq!(conn(&doc, t, 3).target, "room2");
        assert_eq!(get(&doc, user, "replace01").as_deref(), Some("$door room2"));
        assert_eq!(get(&doc, user, "replace02").as_deref(), Some("$x room2-relay"));
        doc.rename_entity_names(Some(&[post]), "hall", "lobby", Some(&rules)).expect("rename");
        assert_eq!(conn(&doc, t, 1).target, "relay-lobby");
        doc.rename_entity_names(Some(&[none]), "raw", "cooked", Some(&rules)).expect("rename");
        assert_eq!(conn(&doc, t, 2).target, "raw-relay");
    }

    #[test]
    fn fgd_rules_and_special_names() {
        let mut doc = Document::new_empty();
        let a = ent(&mut doc, "info_target", &[("targetname", "spot")]);
        let t = ent(&mut doc, "point_spotlight", &[("aimat", "spot"), ("target", "spot")]);
        doc.add_connection(&[t], &Connection::parse("OnUser1", "!self,PointAt,spot,0,-1")).expect("output");
        let keys = |class: &str, key: &str| (class == "point_spotlight").then(|| key.eq_ignore_ascii_case("aimat"));
        let inputs = |class: &str, input: &str| (class == "point_spotlight").then(|| input == "PointAt");
        let rules = RenameRules { is_target_key: Some(&keys), input_takes_name: Some(&inputs) };
        let r = doc.rename_entity_names(Some(&[a]), "spot", "mark", Some(&rules)).expect("rename");
        assert_eq!(r.references, 2);
        assert_eq!(get(&doc, t, "aimat").as_deref(), Some("mark"));
        assert_eq!(get(&doc, t, "target").as_deref(), Some("spot"), "the FGD says target is not a name here");
        assert_eq!(conn(&doc, t, 0).param, "mark");
        let s = ent(&mut doc, "info_target", &[("targetname", "!player")]);
        let r = doc.rename_entity_names(Some(&[s]), "!player", "p", Some(&rules)).expect("rename");
        assert_eq!(r.references, 0);
    }
}
