use glam::{DMat3, DVec3};
use hp_editor::{Document, EditError, Id, ObjRef, Owner, SelectOp};
use hp_geom::{GeomError, hull_solid};
use hp_vmf::{EditorInfo, Side, TexAxis};

use super::build::{EntityPlan, EntityRole, Plan, SolidPlan};
use super::model::{SplineMeta, SplinePoint};

pub const ANCHOR_CLASS: &str = "info_null";
pub const SPLINE_KEY: &str = "hammerpro_spline";
pub const DATA_PREFIX: &str = "hammerpro_spline_data";
pub const OWNER_KEY: &str = "hammerpro_spline_of";
pub const ROPE_BASE: &str = "spline_rope";
const CHUNK: usize = 700;
const MAX_SCAN: usize = 64;

pub const UNDO_CREATE: &str = "Create spline";
pub const UNDO_EDIT: &str = "Edit spline";
pub const UNDO_BAKE: &str = "Bake spline";

#[derive(Debug)]
pub enum StoreError {
    Edit(EditError),
    Geom(GeomError),
    NotSpline,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Edit(e) => e.fmt(f),
            StoreError::Geom(e) => e.fmt(f),
            StoreError::NotSpline => write!(f, "this object is not part of a spline"),
        }
    }
}

impl From<EditError> for StoreError {
    fn from(e: EditError) -> Self {
        StoreError::Edit(e)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplineEntry {
    pub anchor: Id,
    pub uid: String,
    pub meta: SplineMeta,
    pub outputs: Vec<ObjRef>,
    pub group: Option<Id>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    pub anchor: Id,
    pub uid: String,
    pub outputs: Vec<ObjRef>,
    pub group: Option<Id>,
    pub props: Vec<Id>,
    pub solids: usize,
    pub detail: Option<Id>,
}

fn editor_value<'a>(e: &'a EditorInfo, key: &str) -> Option<&'a str> {
    e.extra_props.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.trim()).filter(|v| !v.is_empty())
}

fn editor_data(e: &EditorInfo) -> String {
    let mut parts: Vec<(usize, &str)> = e
        .extra_props
        .iter()
        .filter_map(|(k, v)| {
            let head = k.get(..DATA_PREFIX.len())?;
            if !head.eq_ignore_ascii_case(DATA_PREFIX) {
                return None;
            }
            Some((k.get(DATA_PREFIX.len()..)?.parse::<usize>().ok()?, v.as_str()))
        })
        .collect();
    parts.sort_unstable_by_key(|(n, _)| *n);
    parts.into_iter().map(|(_, v)| v).collect()
}

fn is_spline_key(k: &str) -> bool {
    let low = k.to_ascii_lowercase();
    low == SPLINE_KEY || low.starts_with(DATA_PREFIX)
}

fn set_anchor_keys(e: &mut EditorInfo, uid: &str, data: &str) {
    e.extra_props.retain(|(k, _)| !is_spline_key(k));
    e.extra_props.push((SPLINE_KEY.to_owned(), uid.to_owned()));
    for (i, c) in data.as_bytes().chunks(CHUNK).enumerate() {
        e.extra_props.push((format!("{DATA_PREFIX}{i}"), String::from_utf8_lossy(c).into_owned()));
    }
}

fn set_owner(e: &mut EditorInfo, uid: Option<&str>, group: Option<Option<Id>>) {
    e.extra_props.retain(|(k, _)| !k.eq_ignore_ascii_case(OWNER_KEY));
    if let Some(u) = uid {
        e.extra_props.push((OWNER_KEY.to_owned(), u.to_owned()));
    }
    if let Some(g) = group {
        e.group_id = g;
    }
}

pub fn anchor_uid(doc: &Document, id: Id) -> Option<String> {
    let n = doc.scene().entity(id)?;
    editor_value(&n.entity.editor, SPLINE_KEY).map(str::to_owned)
}

pub fn owner_uid(doc: &Document, obj: ObjRef) -> Option<String> {
    let scene = doc.scene();
    let editor = match scene.object_of_solid_ref(obj) {
        ObjRef::Solid(id) => &scene.solid(id)?.solid.editor,
        ObjRef::Entity(id) => &scene.entity(id)?.entity.editor,
    };
    editor_value(editor, OWNER_KEY).map(str::to_owned)
}

trait ObjectOf {
    fn object_of_solid_ref(&self, obj: ObjRef) -> ObjRef;
}

impl ObjectOf for hp_editor::Scene {
    fn object_of_solid_ref(&self, obj: ObjRef) -> ObjRef {
        match obj {
            ObjRef::Solid(id) => self.object_of_solid(id),
            e => e,
        }
    }
}

fn anchor_frame(doc: &Document, anchor: Id) -> Option<(DVec3, DMat3)> {
    let e = &doc.scene().entity(anchor)?.entity;
    let origin = e.origin().unwrap_or(DVec3::ZERO);
    let angles = e.angles().unwrap_or(DVec3::ZERO);
    Some((origin, hp_geom::angle_matrix(angles)))
}

fn transform_meta(meta: &SplineMeta, f: impl Fn(DVec3) -> DVec3, g: impl Fn(DVec3) -> DVec3) -> SplineMeta {
    let mut m = meta.clone();
    m.points = meta
        .points
        .iter()
        .map(|p| SplinePoint { pos: f(p.pos), handle_in: p.handle_in.map(&g), handle_out: p.handle_out.map(&g), ..*p })
        .collect();
    m
}

fn round_vec(v: DVec3) -> DVec3 {
    v.map(|x| {
        let r = (x * 1.0e4).round() / 1.0e4;
        if r == 0.0 { 0.0 } else { r }
    })
}

pub fn to_world(meta: &SplineMeta, origin: DVec3, rot: DMat3) -> SplineMeta {
    transform_meta(meta, |p| round_vec(origin + rot * p), |h| round_vec(rot * h))
}

pub fn to_local(meta: &SplineMeta, origin: DVec3, rot: DMat3) -> SplineMeta {
    let inv = rot.transpose();
    transform_meta(meta, |p| round_vec(inv * (p - origin)), |h| round_vec(inv * h))
}

pub fn anchors(doc: &Document) -> Vec<Id> {
    let mut out: Vec<Id> = doc
        .scene()
        .entities()
        .values()
        .filter(|n| editor_value(&n.entity.editor, SPLINE_KEY).is_some())
        .map(|n| n.entity.id)
        .collect();
    out.sort_unstable();
    out
}

fn root_of(doc: &Document, obj: ObjRef) -> Option<Id> {
    let scene = doc.scene();
    scene.object_group(scene.object_of_solid_ref(obj)).map(|g| scene.group_root(g))
}

pub fn outputs_of(doc: &Document, anchor: Id, uid: &str) -> Vec<ObjRef> {
    let scene = doc.scene();
    let others: Vec<Id> =
        anchors(doc).into_iter().filter(|a| *a != anchor && anchor_uid(doc, *a).as_deref() == Some(uid)).collect();
    let my_root = root_of(doc, ObjRef::Entity(anchor));
    let other_roots: Vec<Id> = others.iter().filter_map(|a| root_of(doc, ObjRef::Entity(*a))).collect();
    let keep = |obj: ObjRef| {
        let root = root_of(doc, obj);
        match my_root {
            Some(r) if !other_roots.contains(&r) || others.is_empty() => root == Some(r),
            Some(_) => false,
            None => root.is_none_or(|r| !other_roots.contains(&r)),
        }
    };
    let mut out = Vec::new();
    for n in scene.solids().values() {
        if n.owner == Owner::World
            && editor_value(&n.solid.editor, OWNER_KEY) == Some(uid)
            && keep(ObjRef::Solid(n.solid.id))
        {
            out.push(ObjRef::Solid(n.solid.id));
        }
    }
    for n in scene.entities().values() {
        if n.entity.id != anchor
            && editor_value(&n.entity.editor, OWNER_KEY) == Some(uid)
            && keep(ObjRef::Entity(n.entity.id))
        {
            out.push(ObjRef::Entity(n.entity.id));
        }
    }
    out.sort_unstable_by_key(|o| match o {
        ObjRef::Solid(id) => (1, *id),
        ObjRef::Entity(id) => (0, *id),
    });
    out
}

pub fn read(doc: &Document, anchor: Id) -> Option<SplineEntry> {
    let node = doc.scene().entity(anchor)?;
    let uid = editor_value(&node.entity.editor, SPLINE_KEY)?.to_owned();
    let local = SplineMeta::decode(&editor_data(&node.entity.editor))?;
    let (origin, rot) = anchor_frame(doc, anchor)?;
    let meta = to_world(&local, origin, rot);
    let outputs = outputs_of(doc, anchor, &uid);
    let group = doc.scene().object_group(ObjRef::Entity(anchor));
    Some(SplineEntry { anchor, uid, meta, outputs, group })
}

pub fn list(doc: &Document) -> Vec<SplineEntry> {
    anchors(doc).into_iter().filter_map(|a| read(doc, a)).collect()
}

pub fn anchor_of(doc: &Document, obj: ObjRef) -> Option<Id> {
    let scene = doc.scene();
    let obj = scene.object_of_solid_ref(obj);
    if let ObjRef::Entity(id) = obj
        && anchor_uid(doc, id).is_some()
    {
        return Some(id);
    }
    let root = root_of(doc, obj);
    if let Some(uid) = owner_uid(doc, obj) {
        let same: Vec<Id> = anchors(doc).into_iter().filter(|a| anchor_uid(doc, *a).as_deref() == Some(&uid)).collect();
        if let Some(a) = same.iter().find(|a| root.is_some() && root_of(doc, ObjRef::Entity(**a)) == root) {
            return Some(*a);
        }
        if let Some(a) = same.iter().find(|a| outputs_of(doc, **a, &uid).contains(&obj)) {
            return Some(*a);
        }
    }
    let r = root?;
    scene.group_members(r).into_iter().find_map(|m| match m {
        ObjRef::Entity(id) if anchor_uid(doc, id).is_some() => Some(id),
        _ => None,
    })
}

pub fn selected_anchor(doc: &Document) -> Option<Id> {
    doc.selection().objects.iter().take(MAX_SCAN).find_map(|o| anchor_of(doc, *o))
}

pub fn next_uid(doc: &Document) -> String {
    let max = anchors(doc).into_iter().filter_map(|a| anchor_uid(doc, a)?.parse::<u64>().ok()).max().unwrap_or(0);
    (max + 1).to_string()
}

fn align_uv(sides: &mut [Side], plan: &SolidPlan) {
    let Some(uv) = &plan.uv else { return };
    for side in sides {
        let n = side.normal();
        if !n.is_finite() || n.length_squared() < 0.5 || n.dot(uv.axis).abs() > 0.7 {
            continue;
        }
        let Some(u) = (uv.axis - n * uv.axis.dot(n)).try_normalize() else { continue };
        let v = u.cross(n).normalize();
        let us = side.uaxis.scale;
        let vs = side.vaxis.scale;
        if us.abs() < 1e-9 || vs.abs() < 1e-9 {
            continue;
        }
        let ushift = (uv.s - uv.start.dot(u)) / us;
        let vshift = -uv.start.dot(v) / vs;
        side.uaxis = TexAxis { axis: round_vec(u), shift: clean_shift(ushift), scale: us };
        side.vaxis = TexAxis { axis: round_vec(v), shift: clean_shift(vshift), scale: vs };
    }
}

fn clean_shift(x: f64) -> f64 {
    let r = (x * 1000.0).round() / 1000.0;
    if r == 0.0 || !r.is_finite() { 0.0 } else { r }
}

pub fn build_solids(
    plan: &Plan,
    tex_scale: f64,
    lightmap: i32,
    first_side_id: u32,
) -> Result<Vec<Vec<Side>>, StoreError> {
    let mut next = first_side_id;
    let mut out = Vec::with_capacity(plan.solids.len());
    for s in &plan.solids {
        let mut sides = hull_solid(&s.points(), &s.material, tex_scale, lightmap, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })
        .map_err(StoreError::Geom)?;
        align_uv(&mut sides, s);
        out.push(sides);
    }
    Ok(out)
}

fn entity_keyvalues(e: &EntityPlan) -> Vec<(String, String)> {
    let mut kv = Vec::new();
    if let Some(m) = &e.model {
        kv.push(("model".to_owned(), m.clone()));
    }
    if e.role != EntityRole::Rope {
        kv.push(("angles".to_owned(), hp_vmf::fmt_vec3(e.angles)));
    }
    if (e.scale - 1.0).abs() > 1e-6 {
        kv.push(("modelscale".to_owned(), super::build::format_num(e.scale)));
    }
    kv.extend(e.keyvalues.iter().cloned());
    kv
}

pub struct WriteSpec<'a> {
    pub anchor: Option<Id>,
    pub meta: &'a SplineMeta,
    pub plan: &'a Plan,
    pub lightmap: i32,
}

pub fn write(doc: &mut Document, spec: &WriteSpec<'_>) -> Result<Applied, StoreError> {
    let meta = spec.meta.clone().clamped();
    let first = meta.points.first().map(|p| p.pos).ok_or(StoreError::NotSpline)?;
    let (anchor, uid) = match spec.anchor {
        Some(a) => {
            let uid = anchor_uid(doc, a).ok_or(StoreError::NotSpline)?;
            let old = outputs_of(doc, a, &uid);
            let shared = anchors(doc).into_iter().any(|o| o != a && anchor_uid(doc, o).as_deref() == Some(&uid));
            if !old.is_empty() {
                doc.delete(&old)?;
            }
            (a, if shared { next_uid(doc) } else { uid })
        }
        None => {
            let uid = next_uid(doc);
            let id =
                doc.create_point_entity(ANCHOR_CLASS, round_vec(first), &[("angles".to_owned(), "0 0 0".to_owned())])?;
            (id, uid)
        }
    };
    let (origin, rot) = anchor_frame(doc, anchor).ok_or(StoreError::NotSpline)?;
    let local = to_local(&meta, origin, rot);
    let data = local.encode();
    doc.edit_entity(anchor, |e| set_anchor_keys(&mut e.editor, &uid, &data))?;
    let mut outputs: Vec<ObjRef> = Vec::new();
    let mut applied = Applied { anchor, uid: uid.clone(), ..Applied::default() };
    if !spec.plan.solids.is_empty() {
        let solids = build_solids(spec.plan, meta.tex_scale, spec.lightmap, doc.peek_next_side_id())?;
        applied.solids = solids.len();
        if meta.detail {
            let e = doc.create_brush_entity("func_detail", &[], solids)?;
            applied.detail = Some(e);
            outputs.push(ObjRef::Entity(e));
        } else {
            let ids = doc.create_solids(solids, Owner::World)?;
            outputs.extend(ids.into_iter().map(ObjRef::Solid));
        }
    }
    let ropes = spec.plan.entities.iter().filter(|e| e.role == EntityRole::Rope).count();
    let names = if ropes > 0 { super::super::path::chain::chain_names(doc, ROPE_BASE, ropes) } else { Vec::new() };
    let mut rope_index = vec![None; spec.plan.entities.len()];
    let mut k = 0;
    for (i, e) in spec.plan.entities.iter().enumerate() {
        if e.role == EntityRole::Rope {
            rope_index[i] = Some(k);
            k += 1;
        }
    }
    for (i, e) in spec.plan.entities.iter().enumerate() {
        let mut kv = entity_keyvalues(e);
        if let Some(r) = rope_index[i] {
            kv.push(("targetname".to_owned(), names[r].clone()));
            if let Some(n) = e.next.and_then(|n| rope_index.get(n).copied().flatten()) {
                kv.push(("NextKey".to_owned(), names[n].clone()));
            }
        }
        let id = doc.create_point_entity(&e.class, round_vec(e.origin), &kv)?;
        if e.role == EntityRole::Prop {
            applied.props.push(id);
        }
        outputs.push(ObjRef::Entity(id));
    }
    let group = doc.scene().object_group(ObjRef::Entity(anchor));
    for o in &outputs {
        match *o {
            ObjRef::Solid(id) => doc.edit_solid(id, |s| set_owner(&mut s.editor, Some(&uid), group.map(Some)))?,
            ObjRef::Entity(id) => doc.edit_entity(id, |e| set_owner(&mut e.editor, Some(&uid), group.map(Some)))?,
        };
    }
    let mut all = vec![ObjRef::Entity(anchor)];
    all.extend(outputs.iter().copied());
    applied.group = match group {
        Some(g) => Some(g),
        None if !outputs.is_empty() => Some(doc.group(&all)?),
        None => None,
    };
    doc.select(&all, SelectOp::Replace);
    applied.outputs = outputs;
    Ok(applied)
}

pub fn retag_outputs(doc: &mut Document, uid: &str, objs: &[ObjRef]) -> Result<(), StoreError> {
    for o in objs {
        match *o {
            ObjRef::Solid(id) => doc.edit_solid(id, |s| set_owner(&mut s.editor, Some(uid), None))?,
            ObjRef::Entity(id) => doc.edit_entity(id, |e| set_owner(&mut e.editor, Some(uid), None))?,
        };
    }
    Ok(())
}

pub fn save_meta(doc: &mut Document, anchor: Id, meta: &SplineMeta) -> Result<(), StoreError> {
    let uid = anchor_uid(doc, anchor).ok_or(StoreError::NotSpline)?;
    let (origin, rot) = anchor_frame(doc, anchor).ok_or(StoreError::NotSpline)?;
    let data = to_local(&meta.clone().clamped(), origin, rot).encode();
    doc.edit_entity(anchor, |e| set_anchor_keys(&mut e.editor, &uid, &data))?;
    Ok(())
}

pub fn bake(doc: &mut Document, anchor: Id, keep_group: bool) -> Result<Vec<ObjRef>, StoreError> {
    let entry = read(doc, anchor).ok_or(StoreError::NotSpline)?;
    let outputs = entry.outputs.clone();
    doc.transaction(UNDO_BAKE, |doc| {
        for o in &outputs {
            match *o {
                ObjRef::Solid(id) => doc.edit_solid(id, |s| set_owner(&mut s.editor, None, None))?,
                ObjRef::Entity(id) => doc.edit_entity(id, |e| set_owner(&mut e.editor, None, None))?,
            };
        }
        doc.delete(&[ObjRef::Entity(anchor)])?;
        let alive: Vec<ObjRef> = outputs.iter().copied().filter(|o| doc.scene().contains(*o)).collect();
        if !keep_group
            && let Some(g) = entry.group
            && doc.scene().group(g).is_some()
            && doc.scene().group_parent(g).is_none()
            && !alive.is_empty()
        {
            doc.ungroup(&alive)?;
        }
        if !alive.is_empty() {
            doc.select(&alive, SelectOp::Replace);
        }
        Ok::<_, StoreError>(alive)
    })
}
