use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use glam::DVec3;
use hp_geom::{Aabb, DispMesh, SolidGeom};
use hp_vmf::Entity;
use rayon::prelude::*;

use crate::scene::{Owner, Scene, SolidNode};
use crate::selection::ObjRef;
use crate::{Document, EditResult, Id};

const QUANT: f64 = 16.0;
const BOUNDS_EPS: f64 = 0.1;
const INSIDE_EPS: f64 = 0.01;
const CELL: f64 = 256.0;
const MAX_CELLS: i64 = 512;
const DETAIL_CLASS: &str = "func_detail";

pub const CODE_BRUSH: &str = "duplicate_brush";
pub const CODE_CONTAINED: &str = "contained_brush";
pub const CODE_ENTITY: &str = "duplicate_entity";
pub const CODES: [&str; 3] = [CODE_BRUSH, CODE_CONTAINED, CODE_ENTITY];

#[derive(Default)]
struct Mix(u64);

impl Mix {
    fn add(&mut self, w: u64) {
        self.0 = (self.0.rotate_left(5) ^ w).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn int(&mut self, v: i64) {
        self.add(v as u64);
    }
    fn finish(&self) -> u64 {
        self.0 | 1
    }
}

fn quant(v: f64) -> i64 {
    if v.is_finite() { (v * QUANT).round() as i64 } else { i64::MIN }
}

fn quant3(v: DVec3) -> [i64; 3] {
    [quant(v.x), quant(v.y), quant(v.z)]
}

fn points_key(points: impl Iterator<Item = DVec3>) -> u64 {
    let mut pts: Vec<[i64; 3]> = points.map(quant3).collect();
    pts.sort_unstable();
    pts.dedup();
    let mut h = Mix::default();
    h.add(pts.len() as u64);
    for p in &pts {
        h.int(p[0]);
        h.int(p[1]);
        h.int(p[2]);
    }
    h.finish()
}

pub fn shape_key(geom: &SolidGeom, disp: &[Option<Arc<DispMesh>>]) -> u64 {
    if !geom.valid || geom.faces.is_empty() {
        return 0;
    }
    let mut h = Mix::default();
    h.add(points_key(geom.faces.iter().flat_map(|f| f.vertices.iter().copied())));
    let mut meshes: Vec<u64> = disp.iter().flatten().map(|m| points_key(m.positions.iter().copied())).collect();
    meshes.sort_unstable();
    h.add(meshes.len() as u64);
    for m in meshes {
        h.add(m);
    }
    h.finish()
}

pub fn node_shape(n: &SolidNode) -> u64 {
    shape_key(&n.geom, &n.disp)
}

fn same_bounds(a: &Aabb, b: &Aabb) -> bool {
    (a.min - b.min).abs().max_element() <= BOUNDS_EPS && (a.max - b.max).abs().max_element() <= BOUNDS_EPS
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OwnerKey {
    World,
    Detail,
    Entity(Id),
}

impl OwnerKey {
    pub fn of(owner: Owner, is_detail: impl Fn(Id) -> bool) -> OwnerKey {
        match owner {
            Owner::World => OwnerKey::World,
            Owner::Entity(e) => {
                if is_detail(e) {
                    OwnerKey::Detail
                } else {
                    OwnerKey::Entity(e)
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContentClass {
    Solid,
    Tool(u32),
}

pub struct SolidInfo<'a> {
    pub id: Id,
    pub owner: OwnerKey,
    pub shape: u64,
    pub bounds: Aabb,
    pub geom: &'a SolidGeom,
    pub class: Option<ContentClass>,
    pub volume: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Redundant {
    pub brushes: Vec<(Id, Vec<Id>)>,
    pub contained: Vec<(Id, Id)>,
    pub entities: Vec<(Id, Vec<Id>)>,
}

impl Redundant {
    pub fn is_empty(&self) -> bool {
        self.brushes.is_empty() && self.contained.is_empty() && self.entities.is_empty()
    }

    pub fn redundant_brushes(&self) -> Vec<Id> {
        self.brushes.iter().flat_map(|(_, c)| c.iter().copied()).collect()
    }

    pub fn contained_brushes(&self) -> Vec<Id> {
        self.contained.iter().map(|(_, inner)| *inner).collect()
    }

    pub fn redundant_entities(&self) -> Vec<Id> {
        self.entities.iter().flat_map(|(_, c)| c.iter().copied()).collect()
    }

    pub fn objects(&self, code: &str) -> Vec<ObjRef> {
        match code {
            CODE_BRUSH => self.redundant_brushes().into_iter().map(ObjRef::Solid).collect(),
            CODE_CONTAINED => self.contained_brushes().into_iter().map(ObjRef::Solid).collect(),
            CODE_ENTITY => self.redundant_entities().into_iter().map(ObjRef::Entity).collect(),
            _ => Vec::new(),
        }
    }

    pub fn all_objects(&self) -> Vec<ObjRef> {
        CODES.iter().flat_map(|c| self.objects(c)).collect()
    }
}

fn group_by_key<K: Hash + Eq + Copy>(items: impl Iterator<Item = (K, Id)>) -> Vec<(Id, Vec<Id>)> {
    let mut map: HashMap<K, Vec<Id>> = HashMap::new();
    for (k, id) in items {
        map.entry(k).or_default().push(id);
    }
    let mut out: Vec<(Id, Vec<Id>)> = map
        .into_values()
        .filter(|v| v.len() > 1)
        .map(|mut v| {
            v.sort_unstable();
            let keep = v.remove(0);
            (keep, v)
        })
        .collect();
    out.sort_unstable_by_key(|(k, _)| *k);
    out
}

fn exact_duplicates(solids: &[SolidInfo<'_>]) -> Vec<(Id, Vec<Id>)> {
    let mut map: HashMap<(OwnerKey, u64), Vec<usize>> = HashMap::new();
    for (i, s) in solids.iter().enumerate() {
        if s.shape != 0 {
            map.entry((s.owner, s.shape)).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    for list in map.into_values().filter(|l| l.len() > 1) {
        let mut rest: Vec<usize> = list;
        while let Some(first) = rest.first().copied() {
            let (same, other): (Vec<usize>, Vec<usize>) =
                rest.iter().partition(|&&j| same_bounds(&solids[first].bounds, &solids[j].bounds));
            rest = other;
            if same.len() > 1 {
                let mut ids: Vec<Id> = same.iter().map(|&j| solids[j].id).collect();
                ids.sort_unstable();
                let keep = ids.remove(0);
                out.push((keep, ids));
            }
        }
    }
    out.sort_unstable_by_key(|(k, _)| *k);
    out
}

fn cell(v: f64) -> i64 {
    (v / CELL).floor() as i64
}

fn inside(inner: &SolidGeom, outer: &SolidGeom) -> bool {
    inner
        .faces
        .iter()
        .flat_map(|f| f.vertices.iter())
        .all(|v| outer.faces.iter().all(|f| f.plane.distance(*v) <= INSIDE_EPS))
}

fn contained(solids: &[SolidInfo<'_>], skip: &std::collections::HashSet<Id>) -> Vec<(Id, Id)> {
    let usable = |s: &SolidInfo<'_>| s.class.is_some() && s.geom.valid && s.shape != 0 && !s.bounds.is_empty();
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    let mut large: Vec<usize> = Vec::new();
    for (i, s) in solids.iter().enumerate() {
        if !usable(s) || skip.contains(&s.id) {
            continue;
        }
        let (a, b) = (s.bounds.min, s.bounds.max);
        let (x0, y0, z0, x1, y1, z1) = (cell(a.x), cell(a.y), cell(a.z), cell(b.x), cell(b.y), cell(b.z));
        let count = (x1 - x0 + 1).saturating_mul(y1 - y0 + 1).saturating_mul(z1 - z0 + 1);
        if count > MAX_CELLS {
            large.push(i);
            continue;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    grid.entry((x, y, z)).or_default().push(i);
                }
            }
        }
    }
    let mut out: Vec<(Id, Id)> = solids
        .par_iter()
        .enumerate()
        .filter(|(_, s)| usable(s) && !skip.contains(&s.id))
        .filter_map(|(i, s)| {
            let c = s.bounds.center();
            let key = (cell(c.x), cell(c.y), cell(c.z));
            grid.get(&key)
                .into_iter()
                .flatten()
                .chain(large.iter())
                .copied()
                .find(|&j| {
                    let o = &solids[j];
                    j != i
                        && o.owner == s.owner
                        && o.class == s.class
                        && o.shape != s.shape
                        && (o.volume > s.volume || (o.volume == s.volume && o.id < s.id))
                        && o.bounds.expand(INSIDE_EPS).contains_aabb(&s.bounds)
                        && inside(s.geom, o.geom)
                })
                .map(|j| (solids[j].id, s.id))
        })
        .collect();
    out.sort_unstable_by_key(|(_, inner)| *inner);
    out
}

pub fn find_redundant(solids: &[SolidInfo<'_>], entities: &[(Id, u64)]) -> Redundant {
    let brushes = exact_duplicates(solids);
    let skip: std::collections::HashSet<Id> = brushes.iter().flat_map(|(_, c)| c.iter().copied()).collect();
    let contained = contained(solids, &skip);
    let entities = group_by_key(entities.iter().map(|(id, k)| (*k, *id)));
    Redundant { brushes, contained, entities }
}

const SKIPPED_KEYS: &[&str] = &["id", "hammerid", "classname"];
const MODEL_KEYS: &[&str] =
    &["model", "skin", "modelscale", "uniformscale", "body", "setbodygroup", "targetname", "parentname"];

fn text_hash(s: &str, lower: bool) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.trim().bytes() {
        let b = if lower { b.to_ascii_lowercase() } else { b };
        let b = if lower && b == b'\\' { b'/' } else { b };
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn numbers(value: &str) -> impl Iterator<Item = f64> + '_ {
    value.split_ascii_whitespace().filter_map(|t| t.parse::<f64>().ok())
}

fn angle_key(v: f64) -> i64 {
    if v.is_finite() { (v.rem_euclid(360.0) * 100.0).round() as i64 % 36000 } else { i64::MIN }
}

fn value_hash(key: &str, value: &str) -> u64 {
    let mut h = Mix::default();
    h.add(text_hash(key, true));
    if key.eq_ignore_ascii_case("origin") {
        numbers(value).for_each(|v| h.int(quant(v)));
    } else if key.eq_ignore_ascii_case("angles") || key.eq_ignore_ascii_case("angle") {
        numbers(value).for_each(|v| h.int(angle_key(v)));
    } else {
        h.add(text_hash(value, key.eq_ignore_ascii_case("model")));
    }
    h.finish()
}

fn is_detail_or_world(class: &str) -> bool {
    let c = class.trim();
    c.is_empty() || c.eq_ignore_ascii_case(DETAIL_CLASS) || c.eq_ignore_ascii_case("worldspawn")
}

fn has_model(e: &Entity) -> bool {
    e.get("model").is_some_and(|m| !m.trim().is_empty() && !m.trim().starts_with('*'))
}

fn coarse_key(e: &Entity, solid_shapes: &[u64]) -> Option<u64> {
    if is_detail_or_world(&e.classname) || solid_shapes.contains(&0) {
        return None;
    }
    let mut h = Mix::default();
    h.add(text_hash(&e.classname, true));
    if solid_shapes.is_empty() {
        let o = e.origin()?;
        h.int(quant(o.x));
        h.int(quant(o.y));
        h.int(quant(o.z));
    } else {
        let mut acc: u64 = 0;
        for s in solid_shapes {
            acc = acc.wrapping_add(s.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        }
        h.add(acc);
        h.add(solid_shapes.len() as u64);
    }
    Some(h.finish())
}

pub fn entity_key(e: &Entity, solid_shapes: &[u64]) -> Option<u64> {
    let mut h = Mix::default();
    h.add(coarse_key(e, solid_shapes)?);
    let point = solid_shapes.is_empty();
    if point {
        let angles = e.angles().unwrap_or(DVec3::ZERO);
        for v in [angles.x, angles.y, angles.z] {
            h.int(angle_key(v));
        }
    }
    let model = point && has_model(e);
    let mut props: u64 = 0;
    let mut count: u64 = 0;
    for (k, v) in &e.props {
        let k = k.trim();
        if SKIPPED_KEYS.iter().any(|s| k.eq_ignore_ascii_case(s)) {
            continue;
        }
        if model && !MODEL_KEYS.iter().any(|s| k.eq_ignore_ascii_case(s)) {
            continue;
        }
        if point && (k.eq_ignore_ascii_case("origin") || k.eq_ignore_ascii_case("angles")) {
            continue;
        }
        props = props.wrapping_add(value_hash(k, v).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        count += 1;
    }
    h.add(props);
    h.add(count);
    let mut conns: u64 = 0;
    for c in &e.connections {
        let mut m = Mix::default();
        m.add(text_hash(&c.output, true));
        m.add(text_hash(&c.target, true));
        m.add(text_hash(&c.input, true));
        m.add(text_hash(&c.param, false));
        m.add(u64::from(c.delay.to_bits()));
        m.int(i64::from(c.times));
        conns = conns.wrapping_add(m.finish().wrapping_mul(0x9e37_79b9_7f4a_7c15));
    }
    h.add(conns);
    h.add(e.connections.len() as u64);
    Some(h.finish())
}

pub fn entity_keys<'a>(items: impl Iterator<Item = (Id, &'a Entity, &'a [u64])>) -> Vec<(Id, u64)> {
    let items: Vec<(Id, &Entity, &[u64], u64)> =
        items.filter_map(|(id, e, shapes)| Some((id, e, shapes, coarse_key(e, shapes)?))).collect();
    let mut counts: HashMap<u64, u32> = HashMap::with_capacity(items.len());
    for (_, _, _, c) in &items {
        *counts.entry(*c).or_default() += 1;
    }
    items
        .into_iter()
        .filter(|(_, _, _, c)| counts.get(c).is_some_and(|n| *n > 1))
        .filter_map(|(id, e, shapes, _)| Some((id, entity_key(e, shapes)?)))
        .collect()
}

pub fn is_detail_class(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case(DETAIL_CLASS)
}

fn detail(scene: &Scene, e: Id) -> bool {
    scene.entities.get(&e).is_some_and(|n| is_detail_class(&n.entity.classname))
}

fn entity_shapes(scene: &Scene, e: Id) -> Vec<u64> {
    scene.entity_solid_ids(e).into_iter().filter_map(|s| scene.solids.get(&s)).map(node_shape).collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveDuplicates {
    pub brushes: Vec<(Id, Id)>,
    pub entities: Vec<(Id, Id)>,
}

impl LiveDuplicates {
    pub fn is_empty(&self) -> bool {
        self.brushes.is_empty() && self.entities.is_empty()
    }

    pub fn count(&self) -> usize {
        self.brushes.len() + self.entities.len()
    }

    pub fn copies(&self) -> Vec<ObjRef> {
        self.brushes
            .iter()
            .map(|(c, _)| ObjRef::Solid(*c))
            .chain(self.entities.iter().map(|(c, _)| ObjRef::Entity(*c)))
            .collect()
    }
}

impl Document {
    pub fn duplicates_of(&self, created: &[ObjRef]) -> LiveDuplicates {
        let scene = &self.scene;
        let created_solids: std::collections::HashSet<Id> = scene.expand_solids(created).into_iter().collect();
        let created_set: std::collections::HashSet<ObjRef> = created.iter().copied().collect();
        let mut out = LiveDuplicates::default();
        let mut brush_entities: std::collections::HashSet<Id> = std::collections::HashSet::new();
        for id in &created_solids {
            let Some(n) = scene.solids.get(id) else { continue };
            if let Owner::Entity(e) = n.owner
                && created_set.contains(&ObjRef::Entity(e))
                && !detail(scene, e)
            {
                brush_entities.insert(e);
                continue;
            }
            let shape = node_shape(n);
            if shape == 0 {
                continue;
            }
            let owner = OwnerKey::of(n.owner, |e| detail(scene, e));
            let mut found: Option<Id> = None;
            for other in self.solids_in_box(&n.bounds.expand(BOUNDS_EPS)) {
                if other == *id || (created_solids.contains(&other) && other > *id) {
                    continue;
                }
                let Some(o) = scene.solids.get(&other) else { continue };
                if same_bounds(&o.bounds, &n.bounds)
                    && OwnerKey::of(o.owner, |e| detail(scene, e)) == owner
                    && node_shape(o) == shape
                {
                    found = Some(found.map_or(other, |f: Id| f.min(other)));
                }
            }
            if let Some(f) = found {
                out.brushes.push((*id, f));
            }
        }
        let created_entities: Vec<Id> = created
            .iter()
            .filter_map(|o| match o {
                ObjRef::Entity(e) => Some(*e),
                ObjRef::Solid(_) => None,
            })
            .collect();
        let classes: std::collections::HashSet<String> = created_entities
            .iter()
            .filter_map(|e| scene.entities.get(e))
            .map(|n| n.entity.classname.trim().to_ascii_lowercase())
            .collect();
        let mut keys: Option<HashMap<u64, Vec<Id>>> = None;
        for e in created_entities {
            let Some(n) = scene.entities.get(&e) else { continue };
            let is_brush = scene.is_brush_entity(e);
            if is_brush && !brush_entities.contains(&e) {
                continue;
            }
            let Some(key) = entity_key(&n.entity, &entity_shapes(scene, e)) else { continue };
            let map = keys.get_or_insert_with(|| {
                let mut m: HashMap<u64, Vec<Id>> = HashMap::new();
                for (id, node) in &scene.entities {
                    if !classes.contains(&node.entity.classname.trim().to_ascii_lowercase()) {
                        continue;
                    }
                    if let Some(k) = entity_key(&node.entity, &entity_shapes(scene, *id)) {
                        m.entry(k).or_default().push(*id);
                    }
                }
                m
            });
            let other = map
                .get(&key)
                .into_iter()
                .flatten()
                .copied()
                .filter(|o| *o != e && !(created_set.contains(&ObjRef::Entity(*o)) && *o > e))
                .min();
            if let Some(o) = other {
                out.entities.push((e, o));
            }
        }
        out
    }

    pub fn last_step_serial(&self) -> Option<u64> {
        self.history.undo.back().map(|e| e.serial)
    }

    pub fn last_step_created(&self) -> Option<(u64, Vec<ObjRef>)> {
        let e = self.history.undo.back()?;
        if e.label == crate::SHARED_UPDATE_LABEL {
            return Some((e.serial, Vec::new()));
        }
        let mut created: Vec<ObjRef> = e
            .snap
            .solids
            .iter()
            .filter(|(id, before)| before.is_none() && self.scene.solids.contains_key(*id))
            .map(|(id, _)| ObjRef::Solid(*id))
            .collect();
        created.extend(
            e.snap
                .entities
                .iter()
                .filter(|(id, before)| before.is_none() && self.scene.entities.contains_key(*id))
                .map(|(id, _)| ObjRef::Entity(*id)),
        );
        Some((e.serial, created))
    }

    pub fn find_redundant(&self) -> Redundant {
        let snap = crate::limits::snapshot::MapSnapshot::capture(self);
        let guess = |m: u32| crate::limits::checks::MaterialFacts::guess(snap.material_key(m));
        crate::limits::checks::redundant_in(&snap, &mut { guess })
    }

    pub fn delete_redundant(&mut self, objs: &[ObjRef]) -> EditResult<usize> {
        let found = self.find_redundant();
        let still: std::collections::HashSet<ObjRef> = found.all_objects().into_iter().collect();
        let doomed: Vec<ObjRef> = objs.iter().copied().filter(|o| still.contains(o)).collect();
        if doomed.is_empty() {
            return Ok(0);
        }
        self.tx("Fix problems", |doc| doc.delete(&doomed))
    }
}
