use std::collections::HashSet;

use glam::{DVec2, DVec3};
use hp_editor::{CurveRecord, Document, EditError, Id, ObjRef, Owner, PickOptions, SelectOp};
use hp_geom::{GeomError, Ray, hull_solid};
use hp_vmf::{Side, TexAxis};

use super::build::{
    GroundFn, JunctionProblem, LateralSide, Notch, Road, RoadEnv, RoadError, RoadPiece, build, elevations,
};
use super::model::{Attach, Junction, JunctionEnd, RoadMeta, RoadPoint};
use crate::curve::apply::Target;

const GROUND_REACH: f64 = 1024.0;
const GROUND_TOP: f64 = 16000.0;
const MAX_CHAIN: usize = 6;
const MIN_SPLIT_GAP: f64 = 32.0;
const SNAP_TO_POINT: f64 = 8.0;

#[derive(Debug)]
pub enum ApplyError {
    Build(RoadError),
    Geom(GeomError),
    Edit(EditError),
    Split(String),
    Missing(&'static str),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::Build(e) => e.fmt(f),
            ApplyError::Geom(e) => e.fmt(f),
            ApplyError::Edit(e) => e.fmt(f),
            ApplyError::Split(s) => f.write_str(s),
            ApplyError::Missing(s) => f.write_str(s),
        }
    }
}

impl From<EditError> for ApplyError {
    fn from(e: EditError) -> Self {
        ApplyError::Edit(e)
    }
}

impl From<RoadError> for ApplyError {
    fn from(e: RoadError) -> Self {
        ApplyError::Build(e)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadCreated {
    pub uid: u32,
    pub ids: Vec<Id>,
    pub entity: Option<Id>,
    pub length: f64,
    pub refreshed: Vec<u32>,
    pub stale: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct RoadEntry {
    pub record: CurveRecord,
    pub meta: RoadMeta,
}

pub fn read_road(doc: &Document, piece: Id) -> Option<(CurveRecord, RoadMeta)> {
    let record = doc.curve_record(piece)?;
    let meta = RoadMeta::decode(&record.data)?;
    Some((record, meta))
}

pub fn road_piece_of(doc: &Document, id: Id) -> Option<Id> {
    let candidates = if doc.scene().solid(id).is_some() { vec![id] } else { doc.scene().entity_solid_ids(id) };
    candidates.into_iter().find(|p| doc.curve_tag(*p).is_some() && read_road(doc, *p).is_some())
}

pub fn selected_road_piece(doc: &Document) -> Option<Id> {
    let sel = doc.selection();
    for obj in sel.objects.iter().take(64) {
        let id = match *obj {
            ObjRef::Solid(id) | ObjRef::Entity(id) => id,
        };
        if let Some(p) = road_piece_of(doc, id) {
            return Some(p);
        }
    }
    None
}

pub fn list_roads(doc: &Document) -> Vec<RoadEntry> {
    let mut seen: HashSet<Id> = HashSet::new();
    let mut out = Vec::new();
    let mut ids: Vec<Id> = doc
        .scene()
        .solids()
        .iter()
        .filter(|(_, n)| hp_editor::editor_curve_tag(&n.solid.editor).is_some())
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    for id in ids {
        if seen.contains(&id) {
            continue;
        }
        let Some(record) = doc.curve_record(id) else { continue };
        seen.extend(record.ids.iter().copied());
        if let Some(meta) = RoadMeta::decode(&record.data) {
            out.push(RoadEntry { record, meta });
        }
    }
    out.sort_by_key(|e| (e.meta.uid, e.record.ids.first().copied()));
    out
}

pub fn find_road(doc: &Document, uid: u32) -> Option<RoadEntry> {
    list_roads(doc).into_iter().find(|e| e.meta.uid == uid)
}

pub fn next_uid(doc: &Document) -> u32 {
    list_roads(doc).iter().map(|e| e.meta.uid).max().map_or(1, |m| m.saturating_add(1))
}

fn cast_down(doc: &Document, exclude: &HashSet<Id>, p: DVec2, from_z: f64) -> Option<f64> {
    let ray = Ray::new(DVec3::new(p.x, p.y, from_z), DVec3::NEG_Z);
    let filter = |o: ObjRef| matches!(o, ObjRef::Solid(id) if !exclude.contains(&id));
    let opts = PickOptions { include_entities: false, filter: Some(&filter), ..PickOptions::default() };
    let hit = doc.pick_ray(&ray, &opts, &|_| None)?;
    hit.point.z.is_finite().then_some(hit.point.z)
}

pub fn ground_height(doc: &Document, exclude: &HashSet<Id>, p: DVec2, reference_z: f64) -> Option<f64> {
    cast_down(doc, exclude, p, reference_z + GROUND_REACH).or_else(|| cast_down(doc, exclude, p, GROUND_TOP))
}

pub fn road_piece_ids(doc: &Document) -> HashSet<Id> {
    list_roads(doc).into_iter().flat_map(|e| e.record.ids).collect()
}

pub fn build_in_doc(doc: &Document, meta: &RoadMeta, own: &[Id]) -> Result<Road, RoadError> {
    build_chain(doc, meta, own, 0, true)
}

fn lane_dependents(doc: &Document, uid: u32) -> Vec<RoadEntry> {
    list_roads(doc)
        .into_iter()
        .filter(|e| e.meta.uid != uid && e.meta.junction.is_some_and(|j| j.target == uid && j.attach == Attach::Lane))
        .collect()
}

fn notches_for(doc: &Document, a: &Road) -> Vec<Notch> {
    let mut out = Vec::new();
    for entry in lane_dependents(doc, a.meta.uid) {
        let Some(j) = entry.meta.junction else { continue };
        let Ok(b) = build_chain(doc, &entry.meta, &entry.record.ids, 1, false) else { continue };
        let Some(end) = (match j.end {
            JunctionEnd::Start => b.samples.first(),
            JunctionEnd::End => b.samples.last(),
        }) else {
            continue;
        };
        let mut best: Option<(f64, LateralSide, [f64; 2])> = None;
        for side in [LateralSide::Right, LateralSide::Left] {
            let edge = a.edge(side, false);
            let mut dist = 0.0;
            let mut arcs = [0.0; 2];
            for (k, c) in [end.left_edge, end.right_edge].iter().enumerate() {
                let mut near = (f64::INFINITY, 0.0);
                for (i, w) in edge.windows(2).enumerate() {
                    let (p, q) = (w[0].truncate(), w[1].truncate());
                    let d = q - p;
                    let len2 = d.length_squared();
                    let f = if len2 > 1e-18 { ((c.truncate() - p).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
                    let dd = (p + d * f).distance(c.truncate());
                    if dd < near.0 {
                        near = (dd, a.samples[i].s + (a.samples[i + 1].s - a.samples[i].s) * f);
                    }
                }
                dist += near.0;
                arcs[k] = near.1;
            }
            if best.as_ref().is_none_or(|b| dist < b.0) {
                best = Some((dist, side, arcs));
            }
        }
        if let Some((_, side, arcs)) = best {
            out.push(Notch { side, from: arcs[0].min(arcs[1]), to: arcs[0].max(arcs[1]) });
        }
    }
    out
}

fn build_chain(doc: &Document, meta: &RoadMeta, own: &[Id], depth: usize, notch: bool) -> Result<Road, RoadError> {
    let mut exclude: HashSet<Id> = own.iter().copied().collect();
    if meta.follow_terrain {
        exclude.extend(road_piece_ids(doc));
    }
    let ground = |p: DVec2, z: f64| ground_height(doc, &exclude, p, z);
    let ground_ref: &GroundFn<'_> = &ground;
    let target = match meta.junction {
        Some(j) => {
            if depth >= MAX_CHAIN {
                return Err(RoadError::Junction(JunctionProblem::MissingTarget));
            }
            let entry = find_road(doc, j.target).ok_or(RoadError::Junction(JunctionProblem::MissingTarget))?;
            Some(build_chain(doc, &entry.meta, &entry.record.ids, depth + 1, false)?)
        }
        None => None,
    };
    let env = RoadEnv { ground: meta.follow_terrain.then_some(ground_ref), target: target.as_ref(), notches: &[] };
    let road = build(meta, &env)?;
    if notch {
        let notches = notches_for(doc, &road);
        if !notches.is_empty() {
            return build(meta, &RoadEnv { notches: &notches, ..env });
        }
    }
    Ok(road)
}

pub fn piece_solid(
    piece: &RoadPiece,
    tex_scale: f64,
    lightmap: i32,
    next: &mut dyn FnMut() -> u32,
) -> Result<Vec<Side>, GeomError> {
    let mut sides = hull_solid(&piece.corners, &piece.material, tex_scale, lightmap, next)?;
    for s in &mut sides {
        if s.normal().z > 0.5 {
            s.uaxis = TexAxis { axis: piece.uv.u_axis, shift: piece.uv.u_shift, scale: tex_scale };
            s.vaxis = TexAxis { axis: piece.uv.v_axis, shift: piece.uv.v_shift, scale: tex_scale };
        }
    }
    Ok(sides)
}

pub fn road_solids(road: &Road, lightmap: i32, first_side_id: u32) -> Result<Vec<Vec<Side>>, ApplyError> {
    let mut id = first_side_id;
    let mut next = || {
        let v = id;
        id = id.saturating_add(1);
        v
    };
    road.pieces
        .iter()
        .map(|p| piece_solid(p, road.meta.tex_scale, lightmap, &mut next).map_err(ApplyError::Geom))
        .collect()
}

fn place(doc: &mut Document, solids: Vec<Vec<Side>>, target: Target<'_>) -> Result<(Vec<Id>, Option<Id>), EditError> {
    match target {
        Target::World => Ok((doc.create_solids(solids, Owner::World)?, None)),
        Target::NewEntity { class, keyvalues } => {
            let e = doc.create_brush_entity(class, keyvalues, solids)?;
            Ok((doc.scene().entity_solid_ids(e), Some(e)))
        }
        Target::Entity(e) => Ok((doc.create_solids(solids, Owner::Entity(e))?, Some(e))),
    }
}

fn select_road(doc: &mut Document, ids: &[Id], entity: Option<Id>) {
    let objs: Vec<ObjRef> = match entity {
        Some(e) => vec![ObjRef::Entity(e)],
        None => ids.iter().map(|i| ObjRef::Solid(*i)).collect(),
    };
    doc.select(&objs, SelectOp::Replace);
}

pub fn create_road(
    doc: &mut Document,
    meta: &RoadMeta,
    lightmap: i32,
    target: Target<'_>,
) -> Result<RoadCreated, ApplyError> {
    doc.transaction("Create road", |doc| {
        let mut meta = meta.clone().clamped();
        meta.uid = next_uid(doc);
        let road = build_in_doc(doc, &meta, &[])?;
        let solids = road_solids(&road, lightmap, doc.peek_next_side_id())?;
        let (ids, entity) = place(doc, solids, target)?;
        doc.tag_curve_pieces(&ids, &meta.encode())?;
        select_road(doc, &ids, entity);
        Ok::<_, ApplyError>(RoadCreated {
            uid: meta.uid,
            ids,
            entity,
            length: road.length,
            refreshed: Vec::new(),
            stale: Vec::new(),
        })
    })
}

fn owner_target(owner: Owner) -> Target<'static> {
    match owner {
        Owner::World => Target::World,
        Owner::Entity(e) => Target::Entity(e),
    }
}

struct Prepared {
    meta: RoadMeta,
    length: f64,
    solids: Vec<Vec<Side>>,
}

fn prepare(doc: &Document, old_ids: &[Id], meta: &RoadMeta, lightmap: i32) -> Result<Prepared, ApplyError> {
    let meta = meta.clone().clamped();
    let road = build_in_doc(doc, &meta, old_ids)?;
    let solids = road_solids(&road, lightmap, doc.peek_next_side_id())?;
    Ok(Prepared { meta, length: road.length, solids })
}

fn commit(doc: &mut Document, old: &CurveRecord, prepared: Prepared) -> Result<RoadCreated, ApplyError> {
    let Prepared { meta, length, solids } = prepared;
    let (ids, entity) = place(doc, solids, owner_target(old.owner))?;
    doc.tag_curve_pieces(&ids, &meta.encode())?;
    let gone: Vec<ObjRef> = old.ids.iter().map(|id| ObjRef::Solid(*id)).collect();
    doc.delete(&gone)?;
    Ok(RoadCreated { uid: meta.uid, ids, entity, length, refreshed: Vec::new(), stale: Vec::new() })
}

fn rebuild(
    doc: &mut Document,
    old: &CurveRecord,
    meta: &RoadMeta,
    lightmap: i32,
    visited: &mut Vec<u32>,
    report: &mut (Vec<u32>, Vec<u32>),
) -> Result<RoadCreated, ApplyError> {
    let prepared = prepare(doc, &old.ids, meta, lightmap)?;
    let uid = prepared.meta.uid;
    let new_target = prepared.meta.junction.filter(|j| j.attach == Attach::Lane).map(|j| j.target);
    let made = commit(doc, old, prepared)?;
    visited.push(uid);
    let old_target =
        RoadMeta::decode(&old.data).and_then(|m| m.junction).filter(|j| j.attach == Attach::Lane).map(|j| j.target);
    for t in [new_target, old_target].into_iter().flatten().filter(|t| *t != uid) {
        if visited.contains(&t) {
            continue;
        }
        if let Some(e) = find_road(doc, t) {
            rebuild(doc, &e.record, &e.meta, lightmap, visited, report)?;
            report.0.push(t);
        }
    }
    refresh_dependents(doc, uid, lightmap, visited, report)?;
    Ok(made)
}

fn refresh_dependents(
    doc: &mut Document,
    uid: u32,
    lightmap: i32,
    visited: &mut Vec<u32>,
    report: &mut (Vec<u32>, Vec<u32>),
) -> Result<(), ApplyError> {
    let dependents: Vec<RoadEntry> = list_roads(doc)
        .into_iter()
        .filter(|e| {
            e.meta.junction.is_some_and(|j| j.target == uid) && e.meta.uid != uid && !visited.contains(&e.meta.uid)
        })
        .collect();
    for entry in dependents {
        match prepare(doc, &entry.record.ids, &entry.meta, lightmap) {
            Ok(prepared) => {
                commit(doc, &entry.record, prepared)?;
                visited.push(entry.meta.uid);
                report.0.push(entry.meta.uid);
                refresh_dependents(doc, entry.meta.uid, lightmap, visited, report)?;
            }
            Err(_) => report.1.push(entry.meta.uid),
        }
    }
    Ok(())
}

pub fn replace_road(
    doc: &mut Document,
    old: &CurveRecord,
    meta: &RoadMeta,
    lightmap: i32,
) -> Result<RoadCreated, ApplyError> {
    doc.transaction("Edit road", |doc| {
        let mut visited = Vec::new();
        let mut report = (Vec::new(), Vec::new());
        let mut made = rebuild(doc, old, meta, lightmap, &mut visited, &mut report)?;
        select_road(doc, &made.ids, made.entity);
        made.refreshed = report.0;
        made.stale = report.1;
        Ok::<_, ApplyError>(made)
    })
}

pub fn delete_road(doc: &mut Document, record: &CurveRecord, lightmap: i32) -> Result<usize, ApplyError> {
    doc.transaction("Delete road", |doc| {
        let mut gone: Vec<ObjRef> = Vec::new();
        match record.owner {
            Owner::Entity(e) if doc.scene().entity_solid_ids(e).iter().all(|id| record.ids.contains(id)) => {
                gone.push(ObjRef::Entity(e));
            }
            _ => gone.extend(record.ids.iter().map(|id| ObjRef::Solid(*id))),
        }
        doc.delete(&gone)?;
        let target = RoadMeta::decode(&record.data).and_then(|m| m.junction).filter(|j| j.attach == Attach::Lane);
        if let Some(j) = target
            && let Some(e) = find_road(doc, j.target)
        {
            rebuild(doc, &e.record, &e.meta, lightmap, &mut Vec::new(), &mut (Vec::new(), Vec::new()))?;
        }
        Ok::<_, ApplyError>(record.ids.len())
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplitAt {
    Point(usize),
    Arc(f64),
    Near(DVec2),
}

fn arc3d(dense: &super::spline::Dense, elevation: &super::spline::Elevation, s: f64) -> f64 {
    let mut total = 0.0;
    for (i, w) in dense.points.windows(2).enumerate() {
        let (a, b) = (dense.cumulative[i], dense.cumulative[i + 1]);
        if a >= s {
            break;
        }
        let end = b.min(s);
        let dz = elevation.at(end) - elevation.at(a);
        let run = if b > a { w[0].distance(w[1]) * (end - a) / (b - a) } else { 0.0 };
        total += (run * run + dz * dz).sqrt();
    }
    total
}

pub fn split_meta(meta: &RoadMeta, at: SplitAt, new_uid: u32) -> Result<(RoadMeta, RoadMeta), ApplyError> {
    let meta = meta.clone().clamped();
    let bad = |s: &str| ApplyError::Split(s.to_owned());
    let e = elevations(&meta)?;
    let len = e.dense.length();
    let n = meta.points.len();
    let mut pts = meta.points.clone();
    let mut s = match at {
        SplitAt::Point(k) => {
            if k == 0 || k + 1 >= n {
                return Err(bad("the road can only be split at an inner point"));
            }
            e.dense.control_arc(k)
        }
        SplitAt::Arc(a) => a,
        SplitAt::Near(p) => e.dense.nearest(p).map(|n| n.0).ok_or_else(|| bad("the road has no points"))?,
    };
    if !s.is_finite() || s < MIN_SPLIT_GAP || s > len - MIN_SPLIT_GAP {
        return Err(bad("the split point must be at least 32 units from both ends of the road"));
    }
    let mut index = None;
    for k in 1..n - 1 {
        if matches!(at, SplitAt::Point(p) if p == k) || (e.dense.control_arc(k) - s).abs() <= SNAP_TO_POINT {
            index = Some(k);
            s = e.dense.control_arc(k);
            break;
        }
    }
    let k = match index {
        Some(k) => k,
        None => {
            let span = e.dense.span_of_arc(s);
            let i0 = e.dense.control[span];
            let i1 = e.dense.control[span + 1];
            let mut at_pos = e.dense.points[i0];
            for i in i0..i1 {
                let (ca, cb) = (e.dense.cumulative[i], e.dense.cumulative[i + 1]);
                if s >= ca && s <= cb {
                    let t = if cb > ca { (s - ca) / (cb - ca) } else { 0.0 };
                    at_pos = e.dense.points[i].lerp(e.dense.points[i + 1], t);
                    break;
                }
            }
            pts.insert(span + 1, RoadPoint::new(at_pos.x, at_pos.y, e.elevation.at(s)));
            span + 1
        }
    };
    let mut full = meta.clone();
    full.points = pts;
    let e2 = elevations(&full)?;
    let s_k = e2.dense.control_arc(k);
    let tangent = e2.dense.tangent_at_control(k).ok_or_else(|| bad("the split point has no direction"))?;
    let slope = e2.elevation.slope_at(s_k);
    let fraction = (s_k / e2.dense.length()).clamp(0.0, 1.0);
    let roll = meta.bank.roll_start + (meta.bank.roll_end - meta.bank.roll_start) * fraction;
    let mut first = full.clone();
    first.points.truncate(k + 1);
    first.end_dir = Some(tangent);
    first.end_slope = Some(slope);
    first.bank.roll_end = roll;
    let mut second = full.clone();
    second.points.drain(..k);
    second.start_dir = Some(tangent);
    second.start_slope = Some(slope);
    second.bank.roll_start = roll;
    second.uid = new_uid;
    second.u0 = meta.u0 + arc3d(&e2.dense, &e2.elevation, s_k);
    if let Some(j) = meta.junction {
        match j.end {
            JunctionEnd::Start => second.junction = None,
            JunctionEnd::End => first.junction = None,
        }
    }
    Ok((first.clamped(), second.clamped()))
}

pub fn split_road(
    doc: &mut Document,
    record: &CurveRecord,
    meta: &RoadMeta,
    at: SplitAt,
    lightmap: i32,
) -> Result<(RoadCreated, RoadCreated), ApplyError> {
    doc.transaction("Split road", |doc| {
        let new_uid = next_uid(doc);
        let (first, mut second) = split_meta(meta, at, new_uid)?;
        let mut report = (Vec::new(), Vec::new());
        let mut a = rebuild_split(doc, record, &first, &second, lightmap, &mut report)?;
        second.u0 = first.u0 + a.length;
        let road_b = build_in_doc(doc, &second, &[])?;
        let solids = road_solids(&road_b, lightmap, doc.peek_next_side_id())?;
        let (ids, entity) = place(doc, solids, owner_target(record.owner))?;
        doc.tag_curve_pieces(&ids, &second.encode())?;
        let mut b = RoadCreated {
            uid: second.uid,
            ids,
            entity,
            length: road_b.length,
            refreshed: Vec::new(),
            stale: Vec::new(),
        };
        for u in [first.uid, second.uid] {
            if !lane_dependents(doc, u).is_empty()
                && let Some(e) = find_road(doc, u)
            {
                rebuild(doc, &e.record, &e.meta, lightmap, &mut Vec::new(), &mut (Vec::new(), Vec::new()))?;
            }
        }
        if let Some(e) = find_road(doc, first.uid) {
            a.ids = e.record.ids;
        }
        if let Some(e) = find_road(doc, second.uid) {
            b.ids = e.record.ids;
        }
        select_road(doc, &a.ids, a.entity);
        a.refreshed = report.0;
        a.stale = report.1;
        Ok::<_, ApplyError>((a, b))
    })
}

fn rebuild_split(
    doc: &mut Document,
    old: &CurveRecord,
    first: &RoadMeta,
    second: &RoadMeta,
    lightmap: i32,
    report: &mut (Vec<u32>, Vec<u32>),
) -> Result<RoadCreated, ApplyError> {
    let road = build_in_doc(doc, first, &old.ids)?;
    let solids = road_solids(&road, lightmap, doc.peek_next_side_id())?;
    let (ids, entity) = place(doc, solids, owner_target(old.owner))?;
    doc.tag_curve_pieces(&ids, &first.encode())?;
    let gone: Vec<ObjRef> = old.ids.iter().map(|id| ObjRef::Solid(*id)).collect();
    doc.delete(&gone)?;
    retarget_dependents(doc, first, second, report);
    Ok(RoadCreated { uid: first.uid, ids, entity, length: road.length, refreshed: Vec::new(), stale: Vec::new() })
}

fn retarget_dependents(doc: &mut Document, first: &RoadMeta, second: &RoadMeta, report: &mut (Vec<u32>, Vec<u32>)) {
    let dense_of = |m: &RoadMeta| elevations(m).ok().map(|e| e.dense);
    let (Some(d1), Some(d2)) = (dense_of(first), dense_of(second)) else { return };
    let dependents: Vec<RoadEntry> = list_roads(doc)
        .into_iter()
        .filter(|e| e.meta.junction.is_some_and(|j| j.target == first.uid) && e.meta.uid != first.uid)
        .collect();
    for entry in dependents {
        let Some(j) = entry.meta.junction else { continue };
        let probe = match j.end {
            JunctionEnd::Start => entry.meta.points.first(),
            JunctionEnd::End => entry.meta.points.last(),
        };
        let Some(p) = probe.map(|p| p.xy()) else { continue };
        let near = |d: &super::spline::Dense| d.nearest(p).map_or(f64::INFINITY, |n| n.2);
        let mut meta = entry.meta.clone();
        if near(&d2) < near(&d1) {
            meta.junction = Some(Junction { target: second.uid, ..j });
        }
        if meta.junction == entry.meta.junction {
            continue;
        }
        let attempt = rewrite_tag_only(doc, &entry.record, &meta);
        match attempt {
            Ok(()) => report.0.push(meta.uid),
            Err(_) => report.1.push(meta.uid),
        }
    }
}

fn rewrite_tag_only(doc: &mut Document, record: &CurveRecord, meta: &RoadMeta) -> Result<(), ApplyError> {
    doc.tag_curve_pieces(&record.ids, &meta.encode())?;
    Ok(())
}

pub fn set_junction(
    doc: &mut Document,
    record: &CurveRecord,
    meta: &RoadMeta,
    junction: Option<Junction>,
    lightmap: i32,
) -> Result<RoadCreated, ApplyError> {
    if let Some(j) = junction {
        let target =
            find_road(doc, j.target).ok_or(ApplyError::Build(RoadError::Junction(JunctionProblem::MissingTarget)))?;
        if target.meta.uid == meta.uid {
            return Err(ApplyError::Build(RoadError::Junction(JunctionProblem::SelfTarget)));
        }
        if target.meta.junction.is_some_and(|t| t.target == meta.uid) {
            return Err(ApplyError::Split("the other road is already joined to this one".to_owned()));
        }
    }
    let mut next = meta.clone();
    next.junction = junction;
    replace_road(doc, record, &next, lightmap)
}
