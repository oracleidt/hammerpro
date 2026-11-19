use hp_editor::rename_refs::RenameReport;
use hp_editor::{Document, EditError, EditResult, Id, ObjRef};
use hp_fgd::ResolvedFgd;
use hp_vmf::Connection;

use crate::values::{parse_flags, with_bit};
use crate::view::Target;

const LABEL_ENTITY: &str = "Edit entity";
const LABEL_WORLD: &str = "Edit map properties";

#[derive(Clone, Debug, PartialEq)]
pub enum ConnField {
    Output(String),
    Target(String),
    Input(String),
    Param(String),
    Delay(f32),
    Once(bool),
    Times(i32),
}

impl ConnField {
    pub fn apply(&self, c: &mut Connection) {
        match self {
            ConnField::Output(v) => v.trim().clone_into(&mut c.output),
            ConnField::Target(v) => v.trim().clone_into(&mut c.target),
            ConnField::Input(v) => v.trim().clone_into(&mut c.input),
            ConnField::Param(v) => v.clone_into(&mut c.param),
            ConnField::Delay(d) => c.delay = if d.is_finite() { d.max(0.0) } else { 0.0 },
            ConnField::Once(once) => c.times = if *once { 1 } else { -1 },
            ConnField::Times(n) => c.times = (*n).clamp(-1, MAX_TIMES),
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            ConnField::Output(v) | ConnField::Target(v) | ConnField::Input(v) | ConnField::Param(v) => Some(v),
            _ => None,
        }
    }
}

pub const MAX_TIMES: i32 = 100_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    SetKey { target: Target, key: String, value: String, merge: bool },
    SetKeys { target: Target, keys: Vec<(String, String)> },
    RemoveKey { target: Target, key: String },
    RenameKey { target: Target, old: String, new: String },
    ResetKey { target: Target, key: String, default: Option<String> },
    Targetname { target: Target, value: String, update_references: bool },
    SetFlag { ids: Vec<Id>, key: String, bit: u32, on: bool },
    ChangeClass { ids: Vec<Id>, class: String, clean: bool },
    CleanKeys { ids: Vec<Id>, reset: bool },
    Comments { ids: Vec<Id>, text: String },
    DisplayColor { objs: Vec<ObjRef>, change: hp_editor::display_color::DisplayColor, merge: bool },
    ConnAdd { ids: Vec<Id>, conn: Connection },
    ConnEdit { rows: Vec<Vec<(Id, usize)>>, field: ConnField, merge: bool },
    ConnDelete { rows: Vec<Vec<(Id, usize)>> },
    ConnPaste { ids: Vec<Id>, conns: Vec<Connection> },
    Tie { objs: Vec<ObjRef>, class: String },
    MoveToWorld { ids: Vec<Id> },
    PasteKeys { ids: Vec<Id>, keys: Vec<(String, String)> },
    SetPerEntity { values: Vec<(Id, Vec<(String, String)>)> },
    Visgroup { ids: Vec<Id>, group: Id, add: bool },
    ReplaceAllUses { key: String, old: String, new: String },
    NameAndSet { named: Id, name: String, target: Target, key: String },
    NameAndConn { named: Id, name: String, rows: Vec<Vec<(Id, usize)>>, field: crate::widgets::ConnText },
}

impl Op {
    pub fn writes_quote(&self) -> bool {
        let q = |s: &str| s.contains('"');
        match self {
            Op::SetKey { key, value, .. } => q(key) || q(value),
            Op::SetKeys { keys, .. } | Op::PasteKeys { keys, .. } => keys.iter().any(|(k, v)| q(k) || q(v)),
            Op::RenameKey { new, .. } => q(new),
            Op::ResetKey { default, .. } => default.as_deref().is_some_and(q),
            Op::Targetname { value, .. } => q(value),
            Op::ChangeClass { class, .. } | Op::Tie { class, .. } => q(class),
            Op::Comments { text, .. } => q(text),
            Op::ConnAdd { conn, .. } => conn_has_quote(conn),
            Op::ConnPaste { conns, .. } => conns.iter().any(conn_has_quote),
            Op::ConnEdit { field, .. } => field.text().is_some_and(q),
            Op::NameAndSet { name, key, .. } => q(name) || q(key),
            Op::NameAndConn { name, .. } => q(name),
            Op::SetPerEntity { values } => values.iter().flat_map(|(_, kv)| kv).any(|(k, v)| q(k) || q(v)),
            Op::ReplaceAllUses { key, new, .. } => q(key) || q(new),
            Op::RemoveKey { .. }
            | Op::SetFlag { .. }
            | Op::CleanKeys { .. }
            | Op::DisplayColor { .. }
            | Op::ConnDelete { .. }
            | Op::MoveToWorld { .. }
            | Op::Visgroup { .. } => false,
        }
    }
}

fn conn_has_quote(c: &Connection) -> bool {
    [&c.output, &c.target, &c.input, &c.param].iter().any(|s| s.contains('"'))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applied {
    Nothing,
    Changed,
    Created(Id),
    Renamed(RenameReport),
}

fn label(target: &Target) -> &'static str {
    if target.is_world() { LABEL_WORLD } else { LABEL_ENTITY }
}

fn in_tx<R>(
    doc: &mut Document,
    label: &str,
    merge: Option<String>,
    f: impl FnOnce(&mut Document) -> EditResult<R>,
) -> EditResult<R> {
    match merge {
        Some(tag) => doc.begin_merge(label, tag),
        None => doc.begin(label),
    }
    match f(doc) {
        Ok(r) => {
            doc.commit();
            Ok(r)
        }
        Err(e) => {
            doc.cancel();
            Err(e)
        }
    }
}

fn set_key_raw(doc: &mut Document, target: &Target, key: &str, value: &str) -> EditResult<bool> {
    match target {
        Target::World => {
            let before = doc.scene().world().get(key) == Some(value);
            doc.set_world_key(key, value)?;
            Ok(!before)
        }
        Target::Entities(ids) => Ok(doc.set_entity_key(ids, key, value)? > 0),
    }
}

fn remove_key_raw(doc: &mut Document, target: &Target, key: &str) -> EditResult<bool> {
    match target {
        Target::World => Ok(doc.remove_world_key(key)?.is_some()),
        Target::Entities(ids) => Ok(doc.remove_entity_key(ids, key)? > 0),
    }
}

fn changed(b: bool) -> Applied {
    if b { Applied::Changed } else { Applied::Nothing }
}

pub fn apply(doc: &mut Document, fgd: Option<&ResolvedFgd>, op: Op) -> EditResult<Applied> {
    if op.writes_quote() {
        return Err(EditError::Invalid("a key value cannot contain a double quote".to_owned()));
    }
    match op {
        Op::SetKey { target, key, value, merge } => {
            let tag = merge.then(|| target.merge_tag(&key));
            in_tx(doc, label(&target), tag, |doc| set_key_raw(doc, &target, &key, &value)).map(changed)
        }
        Op::SetKeys { target, keys } => in_tx(doc, label(&target), None, |doc| {
            let mut any = false;
            for (key, value) in &keys {
                any |= set_key_raw(doc, &target, key, value)?;
            }
            Ok(any)
        })
        .map(changed),
        Op::RemoveKey { target, key } => remove_key_raw(doc, &target, &key).map(changed),
        Op::RenameKey { target, old, new } => rename_key(doc, &target, &old, &new).map(changed),
        Op::ResetKey { target, key, default } => match default.filter(|d| !d.is_empty()) {
            Some(d) => set_key_raw(doc, &target, &key, &d).map(changed),
            None => remove_key_raw(doc, &target, &key).map(changed),
        },
        Op::Targetname { target, value, update_references } => {
            set_targetname(doc, fgd, &target, &value, update_references)
        }
        Op::SetFlag { ids, key, bit, on } => set_flag(doc, &ids, &key, bit, on).map(changed),
        Op::ChangeClass { ids, class, clean } => change_class(doc, fgd, &ids, &class, clean).map(changed),
        Op::CleanKeys { ids, reset } => apply_clean_keys(doc, fgd, &ids, reset).map(|r| changed(r.any())),
        Op::Comments { ids, text } => in_tx(doc, LABEL_ENTITY, None, |doc| {
            let mut any = false;
            let value = (!text.trim().is_empty()).then(|| text.clone());
            for id in &ids {
                any |= doc.edit_entity(*id, |e| e.editor.comments.clone_from(&value))?;
            }
            Ok(any)
        })
        .map(changed),
        Op::DisplayColor { objs, change, merge } => doc.set_display_color(&objs, change, merge).map(|n| changed(n > 0)),
        Op::ConnAdd { ids, conn } => Ok(changed(doc.add_connection(&ids, &conn)? > 0)),
        Op::ConnEdit { rows, field, merge } => {
            let tag = merge.then(|| format!("hp-ent:conn:{rows:?}"));
            in_tx(doc, "Edit output", tag, |doc| {
                let mut any = false;
                for (id, index) in rows.iter().flatten() {
                    let Some(current) = doc.scene().entity(*id).and_then(|n| n.entity.connections.get(*index)) else {
                        continue;
                    };
                    let mut c = current.clone();
                    field.apply(&mut c);
                    if c != *current {
                        doc.set_connection(*id, *index, c)?;
                        any = true;
                    }
                }
                Ok(any)
            })
            .map(changed)
        }
        Op::ConnDelete { rows } => delete_connections(doc, &rows).map(changed),
        Op::ConnPaste { ids, conns } => in_tx(doc, "Paste outputs", None, |doc| {
            let mut n = 0;
            for c in &conns {
                n += doc.add_connection(&ids, c)?;
            }
            Ok(n > 0)
        })
        .map(changed),
        Op::Tie { objs, class } => {
            let defaults = fgd.and_then(|f| f.class(&class)).map(|c| c.default_keyvalues()).unwrap_or_default();
            doc.tie_to_entity(&objs, &class, &defaults).map(Applied::Created)
        }
        Op::MoveToWorld { ids } => {
            let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Entity(*id)).collect();
            Ok(changed(doc.move_to_world(&objs)? > 0))
        }
        Op::PasteKeys { ids, keys } => in_tx(doc, LABEL_PASTE_KEYS, None, |doc| {
            let mut any = false;
            for (key, value) in &keys {
                if key.trim().is_empty() || is_protected_key(key) {
                    continue;
                }
                any |= doc.set_entity_key(&ids, key, value)? > 0;
            }
            Ok(any)
        })
        .map(changed),
        Op::Visgroup { ids, group, add } => {
            let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Entity(*id)).collect();
            let n = if add { doc.add_to_visgroup(group, &objs)? } else { doc.remove_from_visgroup(group, &objs)? };
            Ok(changed(n > 0))
        }
        Op::ReplaceAllUses { key, old, new } => {
            let ids = entities_using(doc, &key, &old);
            if ids.is_empty() || new.trim().is_empty() {
                return Ok(Applied::Nothing);
            }
            in_tx(doc, LABEL_REPLACE_USES, None, |doc| Ok(doc.set_entity_key(&ids, &key, new.trim())? > 0)).map(changed)
        }
        Op::SetPerEntity { values } => in_tx(doc, LABEL_ENTITY, None, |doc| {
            let mut any = false;
            for (id, keys) in &values {
                for (key, value) in keys {
                    any |= doc.set_entity_key(&[*id], key, value)? > 0;
                }
            }
            Ok(any)
        })
        .map(changed),
        Op::NameAndSet { named, name, target, key } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err(EditError::Invalid("empty entity name".to_owned()));
            }
            in_tx(doc, label(&target), None, |doc| {
                let mut any = doc.set_entity_key(&[named], "targetname", &name)? > 0;
                any |= set_key_raw(doc, &target, &key, &name)?;
                Ok(any)
            })
            .map(changed)
        }
        Op::NameAndConn { named, name, rows, field } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err(EditError::Invalid("empty entity name".to_owned()));
            }
            let field = crate::pick::conn_field(field, &name);
            in_tx(doc, "Edit output", None, |doc| {
                let mut any = doc.set_entity_key(&[named], "targetname", &name)? > 0;
                for (id, index) in rows.iter().flatten() {
                    let Some(current) = doc.scene().entity(*id).and_then(|n| n.entity.connections.get(*index)) else {
                        continue;
                    };
                    let mut c = current.clone();
                    field.apply(&mut c);
                    if c != *current {
                        doc.set_connection(*id, *index, c)?;
                        any = true;
                    }
                }
                Ok(any)
            })
            .map(changed)
        }
    }
}

const LABEL_PASTE_KEYS: &str = "Paste keyvalues";
const LABEL_REPLACE_USES: &str = "Replace all uses";

fn asset_key(value: &str) -> String {
    value.trim().replace('\\', "/").to_ascii_lowercase()
}

pub fn entities_using(doc: &Document, key: &str, value: &str) -> Vec<Id> {
    let wanted = asset_key(value);
    if wanted.is_empty() {
        return Vec::new();
    }
    doc.scene()
        .entities()
        .values()
        .filter(|n| n.entity.get(key).is_some_and(|v| asset_key(v) == wanted))
        .map(|n| n.entity.id)
        .collect()
}

pub(crate) fn is_protected_key(key: &str) -> bool {
    ["id", "classname", "origin", "hammerid"].iter().any(|k| key.trim().eq_ignore_ascii_case(k))
}

fn rename_key(doc: &mut Document, target: &Target, old: &str, new: &str) -> EditResult<bool> {
    let new = new.trim();
    if new.is_empty() {
        return Err(EditError::Invalid("empty key name".to_owned()));
    }
    if old == new {
        return Ok(false);
    }
    match target {
        Target::Entities(ids) => Ok(doc.rename_entity_key(ids, old, new)? > 0),
        Target::World => {
            let Some(value) = doc.scene().world().get(old).map(str::to_owned) else {
                return Ok(false);
            };
            in_tx(doc, LABEL_WORLD, None, |doc| {
                doc.remove_world_key(old)?;
                doc.set_world_key(new, &value)?;
                Ok(true)
            })
        }
    }
}

fn set_targetname(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    target: &Target,
    value: &str,
    update_references: bool,
) -> EditResult<Applied> {
    let ids = match target {
        Target::World => return set_key_raw(doc, target, "targetname", value).map(changed),
        Target::Entities(ids) => ids,
    };
    if !update_references || value.trim().is_empty() {
        return Ok(changed(doc.set_entity_key(ids, "targetname", value.trim())? > 0));
    }
    let report = crate::rename::rename_ids(doc, fgd, ids, value, true, true)?;
    let touched = report.renamed > 0 || report.references > 0;
    Ok(if touched || report.kept.is_some() { Applied::Renamed(report) } else { Applied::Nothing })
}

fn set_flag(doc: &mut Document, ids: &[Id], key: &str, bit: u32, on: bool) -> EditResult<bool> {
    let updates: Vec<(Id, u32)> = ids
        .iter()
        .filter_map(|id| {
            let e = &doc.scene().entity(*id)?.entity;
            let current = e.get(key).map_or(0, parse_flags);
            let new = with_bit(current, bit, on);
            (new != current || e.get(key).is_none()).then_some((*id, new))
        })
        .collect();
    if updates.is_empty() {
        return Ok(false);
    }
    in_tx(doc, "Edit flags", None, |doc| {
        for (id, value) in &updates {
            doc.set_entity_key(&[*id], key, &value.to_string())?;
        }
        Ok(true)
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyCleanup {
    pub removed: Vec<(Id, Vec<String>)>,
    pub reset: usize,
}

impl KeyCleanup {
    pub fn any(&self) -> bool {
        self.reset > 0 || self.removed.iter().any(|(_, k)| !k.is_empty())
    }

    pub fn removed_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for key in self.removed.iter().flat_map(|(_, k)| k) {
            if !out.iter().any(|k| k.eq_ignore_ascii_case(key)) {
                out.push(key.clone());
            }
        }
        out
    }
}

type KeyPlan = (Id, Vec<String>, Vec<(String, String)>);

pub fn clean_keys(doc: &mut Document, fgd: Option<&ResolvedFgd>, ids: &[Id], reset: bool) -> EditResult<KeyCleanup> {
    use hp_app::runtime::helpers::class_keys;
    let mut report = KeyCleanup::default();
    let Some(fgd) = fgd else { return Ok(report) };
    let mut plans: Vec<KeyPlan> = Vec::new();
    for id in ids {
        let Some(node) = doc.scene().entity(*id) else { continue };
        let Some(class) = fgd.class(node.entity.classname.trim()) else { continue };
        let removed = class_keys::unused_keys(&node.entity, class);
        let resets = if reset { class_keys::default_resets(&node.entity, class) } else { Vec::new() };
        if !removed.is_empty() || !resets.is_empty() {
            plans.push((*id, removed, resets));
        }
    }
    for (id, removed, resets) in plans {
        for key in &removed {
            doc.remove_entity_key(&[id], key)?;
        }
        for (key, value) in &resets {
            doc.set_entity_key(&[id], key, value)?;
        }
        report.reset += resets.len();
        report.removed.push((id, removed));
    }
    Ok(report)
}

pub fn apply_clean_keys(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    ids: &[Id],
    reset: bool,
) -> EditResult<KeyCleanup> {
    let label = if reset { "Reset to class defaults" } else { "Clean up keyvalues" };
    in_tx(doc, label, None, |doc| clean_keys(doc, fgd, ids, reset))
}

pub fn change_class_report(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    ids: &[Id],
    class: &str,
    clean: bool,
) -> EditResult<(bool, KeyCleanup)> {
    let mut report = KeyCleanup::default();
    let any = in_tx(doc, "Change class", None, |doc| {
        let any = change_class_raw(doc, fgd, ids, class)?;
        if clean && any {
            report = clean_keys(doc, fgd, ids, false)?;
        }
        Ok(any || report.any())
    })?;
    Ok((any, report))
}

fn change_class(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    ids: &[Id],
    class: &str,
    clean: bool,
) -> EditResult<bool> {
    change_class_report(doc, fgd, ids, class, clean).map(|(any, _)| any)
}

fn change_class_raw(doc: &mut Document, fgd: Option<&ResolvedFgd>, ids: &[Id], class: &str) -> EditResult<bool> {
    let class = class.trim();
    if class.is_empty() {
        return Err(EditError::Invalid("empty class name".to_owned()));
    }
    let defaults = fgd.and_then(|f| f.class(class)).map(|c| c.default_keyvalues()).unwrap_or_default();
    {
        let mut any = doc.set_entity_class(ids, class)? > 0;
        for (key, value) in &defaults {
            let missing: Vec<Id> = ids
                .iter()
                .copied()
                .filter(|id| doc.scene().entity(*id).is_some_and(|n| n.entity.get(key).is_none()))
                .collect();
            if !missing.is_empty() {
                any |= doc.set_entity_key(&missing, key, value)? > 0;
            }
        }
        if hp_app::runtime::helpers::brush_entity_needs_origin(fgd, class) {
            any |= doc.add_missing_origins(ids)? > 0;
        }
        Ok(any)
    }
}

fn delete_connections(doc: &mut Document, rows: &[Vec<(Id, usize)>]) -> EditResult<bool> {
    let mut per_entity: Vec<(Id, Vec<usize>)> = Vec::new();
    for (id, index) in rows.iter().flatten() {
        match per_entity.iter_mut().find(|(e, _)| e == id) {
            Some((_, list)) => list.push(*index),
            None => per_entity.push((*id, vec![*index])),
        }
    }
    if per_entity.is_empty() {
        return Ok(false);
    }
    in_tx(doc, "Delete output", None, |doc| {
        let mut any = false;
        for (id, indices) in &per_entity {
            let Some(n) = doc.scene().entity(*id) else { continue };
            let kept: Vec<Connection> = n
                .entity
                .connections
                .iter()
                .enumerate()
                .filter(|(i, _)| !indices.contains(i))
                .map(|(_, c)| c.clone())
                .collect();
            if kept.len() != n.entity.connections.len() {
                doc.set_connections(*id, kept)?;
                any = true;
            }
        }
        Ok(any)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fgd::test_fgd;
    use hp_editor::display_color::DisplayColor;

    fn state() -> hp_app::AppState {
        hp_app::testing::test_state()
    }

    #[test]
    fn changing_to_particle_volume_does_not_rebase_the_brush_or_keep_a_previous_pivot() {
        let mut state = state();
        let fgd = hp_fgd::Fgd::parse(
            "@BaseClass = Origin [ origin(origin) : \"Origin\" ]\n\
             @SolidClass base(Origin) = env_embers : \"Embers\" []\n\
             @SolidClass = func_smokevolume : \"Smoke\" []",
            None,
        )
        .resolve();
        let bounds = state.doc.scene().entity_solid_bounds(6).unwrap();
        for class in ["env_embers", "func_smokevolume"] {
            state.doc.set_entity_key(&[6], "origin", "880 560 77.5").unwrap();
            apply(&mut state.doc, Some(&fgd), Op::ChangeClass { ids: vec![6], class: class.into(), clean: false })
                .unwrap();
            assert!(state.doc.scene().entity(6).unwrap().entity.origin().is_none(), "{class}");
            assert_eq!(state.doc.scene().entity_solid_bounds(6), Some(bounds));
            assert!(state.doc.undo());
            assert_eq!(state.doc.scene().entity(6).unwrap().entity.get("origin"), Some("880 560 77.5"));
        }
    }

    #[test]
    fn light_color_change_is_one_undo_step() {
        let mut s = state();
        let doc = &mut s.doc;
        let target = Target::Entities(vec![5]);
        let op = |v: &str| Op::SetKey { target: target.clone(), key: "_light".into(), value: v.into(), merge: true };
        apply(doc, None, op("255 0 0 300")).unwrap();
        apply(doc, None, op("250 0 0 300")).unwrap();
        apply(doc, None, op("240 0 0 300")).unwrap();
        assert_eq!(doc.history_len().0, 1, "rapid edits of one key merge");
        assert_eq!(doc.undo_label(), Some("Edit entity"));
        assert_eq!(doc.scene().entity(5).unwrap().entity.get("_light"), Some("240 0 0 300"));
        assert!(doc.undo());
        assert_eq!(doc.scene().entity(5).unwrap().entity.get("_light"), Some("255 240 200 300"));
        let doc = &mut s.doc;
        apply(doc, None, Op::SetKey { target: target.clone(), key: "style".into(), value: "1".into(), merge: false })
            .unwrap();
        apply(doc, None, Op::SetKey { target, key: "style".into(), value: "2".into(), merge: false }).unwrap();
        assert_eq!(doc.history_len().0, 2);
    }

    #[test]
    fn same_value_is_not_an_edit() {
        let mut s = state();
        let r = apply(
            &mut s.doc,
            None,
            Op::SetKey {
                target: Target::Entities(vec![5]),
                key: "targetname".into(),
                value: "main_light".into(),
                merge: false,
            },
        )
        .unwrap();
        assert_eq!(r, Applied::Nothing);
        assert_eq!(s.doc.history_len().0, 0);
    }

    #[test]
    fn world_keys_edit_rename_reset() {
        let mut s = state();
        let doc = &mut s.doc;
        apply(
            doc,
            None,
            Op::SetKey { target: Target::World, key: "skyname".into(), value: "sky_night".into(), merge: false },
        )
        .unwrap();
        assert_eq!(doc.scene().world().get("skyname"), Some("sky_night"));
        assert_eq!(doc.undo_label(), Some("Edit map properties"));
        apply(doc, None, Op::RenameKey { target: Target::World, old: "skyname".into(), new: "sky2".into() }).unwrap();
        assert_eq!(doc.scene().world().get("sky2"), Some("sky_night"));
        assert!(doc.scene().world().get("skyname").is_none());
        assert_eq!(doc.history_len().0, 2, "rename is one step");
        apply(doc, None, Op::ResetKey { target: Target::World, key: "sky2".into(), default: None }).unwrap();
        assert!(doc.scene().world().get("sky2").is_none());
        apply(
            doc,
            None,
            Op::ResetKey { target: Target::World, key: "skyname".into(), default: Some("sky_day01_01".into()) },
        )
        .unwrap();
        assert_eq!(doc.scene().world().get("skyname"), Some("sky_day01_01"));
    }

    #[test]
    fn trigger_output_edit_and_delete() {
        let mut s = state();
        let doc = &mut s.doc;
        let rows = vec![vec![(6, 0)]];
        apply(doc, None, Op::ConnEdit { rows: rows.clone(), field: ConnField::Input("TurnOn".into()), merge: false })
            .unwrap();
        let c = &doc.scene().entity(6).unwrap().entity.connections[0];
        assert_eq!(c.input, "TurnOn");
        assert_eq!(doc.undo_label(), Some("Edit output"));
        apply(doc, None, Op::ConnEdit { rows: rows.clone(), field: ConnField::Once(true), merge: false }).unwrap();
        assert_eq!(doc.scene().entity(6).unwrap().entity.connections[0].times, 1);
        apply(doc, None, Op::ConnEdit { rows: rows.clone(), field: ConnField::Delay(-3.0), merge: false }).unwrap();
        assert_eq!(doc.scene().entity(6).unwrap().entity.connections[0].delay, 0.0);
        apply(
            doc,
            None,
            Op::ConnAdd { ids: vec![6], conn: Connection::parse("OnStartTouch", "main_light,Toggle,,1,-1") },
        )
        .unwrap();
        assert_eq!(doc.scene().entity(6).unwrap().entity.connections.len(), 2);
        apply(doc, None, Op::ConnDelete { rows }).unwrap();
        let conns = &doc.scene().entity(6).unwrap().entity.connections;
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].output, "OnStartTouch");
        assert!(doc.undo());
        assert_eq!(doc.scene().entity(6).unwrap().entity.connections.len(), 2);
    }

    #[test]
    fn class_change_adds_defaults_in_one_step() {
        let mut s = state();
        let fgd = test_fgd();
        let doc = &mut s.doc;
        apply(doc, Some(&fgd), Op::ChangeClass { ids: vec![4], class: "light".into(), clean: false }).unwrap();
        let e = &doc.scene().entity(4).unwrap().entity;
        assert_eq!(e.classname, "light");
        assert_eq!(e.get("_light"), Some("255 255 255 200"));
        assert_eq!(e.get("angles"), Some("0 90 0"), "existing keys are kept");
        assert_eq!(doc.undo_label(), Some("Change class"));
        assert_eq!(doc.history_len().0, 1);
        assert!(doc.undo());
        let e = &doc.scene().entity(4).unwrap().entity;
        assert_eq!(e.classname, "info_player_start");
        assert!(e.get("_light").is_none());
        assert!(apply(doc, Some(&fgd), Op::ChangeClass { ids: vec![4], class: "  ".into(), clean: false }).is_err());
    }

    #[test]
    fn class_change_with_cleanup_drops_the_old_class_keys_in_the_same_step() {
        let mut s = state();
        let fgd = test_fgd();
        let doc = &mut s.doc;
        doc.set_entity_key(&[4], "model", "models/props_c17/oildrum001.mdl").unwrap();
        doc.set_entity_key(&[4], "targetname", "lamp").unwrap();
        doc.set_entity_key(&[4], "skin", "2").unwrap();
        let before = doc.history_len().0;
        let (any, report) = change_class_report(doc, Some(&fgd), &[4], "light", true).unwrap();
        assert!(any);
        let e = &doc.scene().entity(4).unwrap().entity;
        assert_eq!(e.classname, "light");
        assert!(e.get("model").is_none() && e.get("skin").is_none());
        assert_eq!(e.get("targetname"), Some("lamp"));
        assert_eq!(e.get("angles"), Some("0 90 0"));
        assert_eq!(e.get("_light"), Some("255 255 255 200"));
        let names = report.removed_names();
        assert!(names.iter().any(|k| k == "model") && names.iter().any(|k| k == "skin"), "{names:?}");
        assert_eq!(doc.history_len().0, before + 1);
        assert!(doc.undo());
        let e = &doc.scene().entity(4).unwrap().entity;
        assert_eq!(e.classname, "info_player_start");
        assert_eq!(e.get("model"), Some("models/props_c17/oildrum001.mdl"));
    }

    #[test]
    fn clean_keys_and_reset_to_class_defaults() {
        let mut s = state();
        let fgd = test_fgd();
        let doc = &mut s.doc;
        apply(doc, Some(&fgd), Op::ChangeClass { ids: vec![4], class: "light".into(), clean: false }).unwrap();
        doc.set_entity_key(&[4], "model", "models/x.mdl").unwrap();
        doc.set_entity_key(&[4], "_light", "1 2 3 4").unwrap();
        apply(doc, Some(&fgd), Op::CleanKeys { ids: vec![4], reset: false }).unwrap();
        let e = &doc.scene().entity(4).unwrap().entity;
        assert!(e.get("model").is_none());
        assert_eq!(e.get("_light"), Some("1 2 3 4"));
        apply(doc, Some(&fgd), Op::CleanKeys { ids: vec![4], reset: true }).unwrap();
        assert_eq!(doc.scene().entity(4).unwrap().entity.get("_light"), Some("255 255 255 200"));
        assert_eq!(doc.undo_label(), Some("Reset to class defaults"));
    }

    #[test]
    fn a_brush_entity_that_becomes_a_train_gets_an_origin_at_its_center() {
        let mut s = state();
        let fgd = hp_fgd::Fgd::parse(
            "@BaseClass = Origin [ origin(origin) : \"Origin\" ]\n\
             @SolidClass base(Origin) = func_tracktrain : \"Train\" [ target(target_destination) : \"First stop\" ]",
            None,
        )
        .resolve();
        let doc = &mut s.doc;
        let center = doc.scene().entity_solid_bounds(6).expect("trigger bounds").center();
        assert!(doc.scene().entity(6).unwrap().entity.get("origin").is_none());
        apply(doc, Some(&fgd), Op::ChangeClass { ids: vec![6], class: "func_tracktrain".into(), clean: false })
            .unwrap();
        let e = &doc.scene().entity(6).unwrap().entity;
        assert_eq!(e.classname, "func_tracktrain");
        assert_eq!(e.origin(), Some(center));
        assert_eq!(doc.undo_label(), Some("Change class"));
        assert_eq!(doc.history_len().0, 1, "one undo step with the origin");
        assert!(doc.undo());
        assert!(doc.scene().entity(6).unwrap().entity.get("origin").is_none());
        apply(doc, Some(&fgd), Op::ChangeClass { ids: vec![4], class: "func_tracktrain".into(), clean: false })
            .unwrap();
        assert_eq!(doc.scene().entity(4).unwrap().entity.get("origin"), Some("0 -128 0"), "point entities keep theirs");
    }

    #[test]
    fn flags_toggle_keeps_other_bits() {
        let mut s = state();
        let doc = &mut s.doc;
        doc.set_entity_key(&[5], "spawnflags", "4").unwrap();
        apply(doc, None, Op::SetFlag { ids: vec![5, 6], key: "spawnflags".into(), bit: 2, on: true }).unwrap();
        assert_eq!(doc.scene().entity(5).unwrap().entity.get("spawnflags"), Some("6"));
        assert_eq!(doc.scene().entity(6).unwrap().entity.get("spawnflags"), Some("3"));
        assert_eq!(doc.undo_label(), Some("Edit flags"));
        apply(doc, None, Op::SetFlag { ids: vec![5, 6], key: "spawnflags".into(), bit: 2, on: false }).unwrap();
        assert_eq!(doc.scene().entity(5).unwrap().entity.get("spawnflags"), Some("4"));
        assert_eq!(doc.scene().entity(6).unwrap().entity.get("spawnflags"), Some("1"));
    }

    #[test]
    fn rename_with_references() {
        let mut s = state();
        let doc = &mut s.doc;
        let target = Target::Entities(vec![5]);
        apply(doc, None, Op::Targetname { target: target.clone(), value: "lamp".into(), update_references: true })
            .unwrap();
        assert_eq!(doc.scene().entity(5).unwrap().entity.get("targetname"), Some("lamp"));
        assert_eq!(doc.scene().entity(6).unwrap().entity.connections[0].target, "lamp");
        apply(doc, None, Op::Targetname { target, value: "bulb".into(), update_references: false }).unwrap();
        assert_eq!(doc.scene().entity(6).unwrap().entity.connections[0].target, "lamp");
    }

    #[test]
    fn multi_edit_renames_each_old_name_in_one_step() {
        let mut s = state();
        let doc = &mut s.doc;
        doc.set_entity_key(&[4], "targetname", "doora").unwrap();
        doc.set_entity_key(&[5], "targetname", "doorb").unwrap();
        let out = |t: &str| Connection::parse("OnTrigger", &format!("{t},Open,,0,-1"));
        doc.set_connections(6, vec![out("doora"), out("doorb"), out("door*")]).unwrap();
        let steps = doc.history_len().0;
        let target = Target::Entities(vec![4, 5]);
        let r = apply(doc, None, Op::Targetname { target, value: "gate".into(), update_references: true }).unwrap();
        let Applied::Renamed(report) = r else { panic!("expected a rename report") };
        assert_eq!((report.renamed, report.references, report.wildcards), (2, 2, 1));
        assert_eq!(doc.history_len().0, steps + 1);
        let conns = &doc.scene().entity(6).unwrap().entity.connections;
        assert_eq!(
            [conns[0].target.as_str(), conns[1].target.as_str(), conns[2].target.as_str()],
            ["gate", "gate", "door*"]
        );
        let text = crate::rename::report_text(&report).unwrap().1;
        assert!(text.contains('2') && text.contains('1'), "{text}");
        assert!(doc.undo());
        assert_eq!(doc.scene().entity(6).unwrap().entity.connections[0].target, "doora");
    }

    #[test]
    fn comments_color_and_tie() {
        let mut s = state();
        let fgd = test_fgd();
        let doc = &mut s.doc;
        apply(doc, None, Op::Comments { ids: vec![5], text: "hello".into() }).unwrap();
        assert_eq!(doc.scene().entity(5).unwrap().entity.editor.comments.as_deref(), Some("hello"));
        apply(doc, None, Op::Comments { ids: vec![5], text: " ".into() }).unwrap();
        assert_eq!(doc.scene().entity(5).unwrap().entity.editor.comments, None);
        let objs = vec![ObjRef::Entity(5)];
        let set = |rgb| Op::DisplayColor { objs: objs.clone(), change: DisplayColor::Set(rgb), merge: true };
        let steps = doc.history_len().0;
        apply(doc, None, set([1, 2, 3])).unwrap();
        apply(doc, None, set([4, 5, 6])).unwrap();
        assert_eq!(doc.scene().entity(5).unwrap().entity.editor.color, [4, 5, 6]);
        assert_eq!(doc.history_len().0, steps + 1, "a live color drag is one undo step");
        let before = doc.history_len().0;
        let r = apply(doc, Some(&fgd), Op::Tie { objs: vec![ObjRef::Solid(2)], class: "func_brush".into() }).unwrap();
        let Applied::Created(id) = r else { panic!("expected a new entity") };
        let e = &doc.scene().entity(id).unwrap().entity;
        assert_eq!(e.classname, "func_brush");
        assert_eq!(e.get("solidity"), Some("0"), "FGD defaults are written");
        assert_eq!(doc.history_len().0, before + 1);
    }

    #[test]
    fn move_brush_entity_to_world() {
        let mut s = state();
        let doc = &mut s.doc;
        apply(doc, None, Op::MoveToWorld { ids: vec![6] }).unwrap();
        assert!(doc.scene().entity(6).is_none(), "the emptied brush entity is deleted");
        assert_eq!(doc.scene().solid(7).map(|n| n.owner), Some(hp_editor::Owner::World));
        assert_eq!(doc.undo_label(), Some("Move to world"));
        assert_eq!(apply(doc, None, Op::MoveToWorld { ids: vec![5] }).unwrap(), Applied::Nothing);
    }

    #[test]
    fn paste_outputs_is_one_step() {
        let mut s = state();
        let doc = &mut s.doc;
        let conns = vec![
            Connection::parse("OnUser1", "main_light,TurnOn,,0,-1"),
            Connection::parse("OnUser2", "main_light,TurnOff,,0,-1"),
        ];
        apply(doc, None, Op::ConnPaste { ids: vec![4, 5], conns }).unwrap();
        assert_eq!(doc.scene().entity(4).unwrap().entity.connections.len(), 2);
        assert_eq!(doc.scene().entity(5).unwrap().entity.connections.len(), 2);
        assert_eq!(doc.history_len().0, 1);
    }
}
