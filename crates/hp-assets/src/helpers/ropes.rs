use std::path::PathBuf;
use std::time::Instant;

use ahash::{AHashMap, AHashSet};
use glam::DVec3;
use hp_app::runtime::helpers::scene_display;
use hp_app::runtime::instances::{self as rt, InstanceCache, InstanceScene};
use hp_app::runtime::wind;
use hp_app::settings::InstanceDisplay;
use hp_editor::instances::is_instance;
use hp_editor::{Document, Id};
use hp_fgd::ResolvedClass;
use hp_geom::Aabb;
use hp_render::{MeshBatch, ObjectFlags, ObjectMesh, Vertex};
use hp_vmf::Entity;

use crate::entities::{class_of, point_color};
use crate::material::MatKind;
use crate::producer::{DerivedBuild, DerivedObject, DerivedProducer, ProducerChanges, ProducerCx};
use crate::util::material_key;

const GRAVITY: f64 = 1500.0;
const STEP: f64 = 1.0 / 50.0;
const DAMPING: f64 = 0.98;
const PASSES: usize = 3;
const HANG_STEPS: usize = 250;
const MAX_STEPS: usize = 4000;
const SETTLED: f64 = 1e-3;
const SETTLED_STEPS: usize = 25;
pub(crate) const MAX_NODES: usize = 10;
const MAX_SUBDIVS: usize = 8;
const SLACK_FUDGE: i32 = -100;
const BARBED_SUBDIVS: [f64; 3] = [1.5, -0.5, 0.5];
const TUBE_SIDES: usize = 8;
const MAX_FRAME_STEPS: usize = 6;
const DEFAULT_MATERIAL: &str = "cable/cable";
pub(crate) const MISSING_SIZE: f64 = 64.0;
const INSTANCE_KEY: u64 = 1 << 63;
const MAX_INSTANCE_SEGMENTS: usize = 1 << 16;

pub(crate) fn is_rope_class(classname: &str) -> bool {
    let c = classname.trim();
    c.eq_ignore_ascii_case("move_rope") || c.eq_ignore_ascii_case("keyframe_rope")
}

pub(crate) fn is_rope(e: &Entity, class: Option<&ResolvedClass>) -> bool {
    is_rope_class(&e.classname) || class.is_some_and(|c| c.has_helper("keyframe"))
}

fn atoi(s: &str) -> i64 {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut v: i64 = 0;
    for b in digits.bytes().take_while(u8::is_ascii_digit) {
        v = v.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    if neg { -v } else { v }
}

fn atof(s: &str) -> f64 {
    let s = s.trim();
    if let Ok(v) = s.parse::<f64>() {
        return if v.is_finite() { v } else { 0.0 };
    }
    (1..s.len())
        .rev()
        .filter(|i| s.is_char_boundary(*i))
        .find_map(|i| s[..i].parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn wrap_bits(v: i64, bits: u32) -> i32 {
    let shift = 64 - bits;
    ((v << shift) >> shift) as i32
}

pub(crate) fn rope_length(a: DVec3, b: DVec3) -> i32 {
    let d = (a - b).length();
    if !d.is_finite() {
        return 0;
    }
    wrap_bits(d.min(1e15) as i64, 15)
}

fn lower(s: &str) -> Box<str> {
    s.trim().to_ascii_lowercase().into_boxed_str()
}

fn name_matches(query: &str, name: &str) -> bool {
    match query.strip_suffix('*') {
        Some(prefix) => !name.is_empty() && name.starts_with(prefix),
        None => query == name,
    }
}

fn searchable(next: &str) -> bool {
    !next.is_empty() && !next.starts_with('!')
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RopeParams {
    pub slack: i32,
    pub nodes: usize,
    pub subdiv: usize,
    pub barbed: bool,
    pub width: f64,
    pub texture_scale: f64,
    pub material: String,
    pub dangling: bool,
    pub wind: bool,
}

impl RopeParams {
    pub(crate) fn of(e: &Entity) -> Self {
        let nodes = match e.get("Type").map(atoi) {
            None => 5,
            Some(0) => MAX_NODES,
            Some(1) => 4,
            Some(_) => 2,
        };
        let subdiv = e.get("Subdiv").map_or(2, |v| (atoi(v) & 15) as usize).min(MAX_SUBDIVS - 1);
        let legacy = e.get("RopeShader").map(|v| match atoi(v) {
            0 => "cable/cable",
            1 => "cable/rope",
            _ => "cable/chain",
        });
        let material = e
            .get("RopeMaterial")
            .map(material_key)
            .filter(|m| !m.is_empty())
            .or_else(|| legacy.map(str::to_owned))
            .unwrap_or_else(|| DEFAULT_MATERIAL.to_owned());
        let width = e.get("Width").map_or(2.0, atof);
        Self {
            slack: e.get("Slack").map_or(0, |v| wrap_bits(atoi(v), 12)),
            nodes,
            subdiv,
            barbed: e.get("Barbed").is_some_and(|v| atoi(v) == 1),
            width: if width.is_finite() { width.clamp(0.0, 1024.0) } else { 2.0 },
            texture_scale: e.get("TextureScale").map_or(4.0, atof).clamp(0.1, 10.0),
            material,
            dangling: e.get("Dangling").is_some_and(|v| atoi(v) == 1),
            wind: !e.get("NoWind").is_some_and(|v| atoi(v) != 0),
        }
    }

    pub(crate) fn spring(&self, length: i32) -> f64 {
        let n = self.nodes.clamp(2, MAX_NODES) as i64;
        let total = i64::from(length) + i64::from(self.slack) + i64::from(SLACK_FUDGE);
        (total / (n - 1)).max(0) as f64
    }

    fn texels(&self, length: i32) -> f64 {
        4.0 / self.texture_scale * f64::from(length + self.slack + SLACK_FUDGE)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RopeSim {
    pub nodes: Vec<DVec3>,
    prev: Vec<DVec3>,
    spring: f64,
    start: DVec3,
    end: Option<DVec3>,
}

impl RopeSim {
    pub(crate) fn new(start: DVec3, end: Option<DVec3>, p: &RopeParams) -> Self {
        let n = p.nodes.clamp(2, MAX_NODES);
        let to = end.unwrap_or(start);
        let length = end.map_or(0, |b| rope_length(start, b));
        let nodes: Vec<DVec3> = (0..n).map(|i| start.lerp(to, i as f64 / (n - 1) as f64)).collect();
        Self { prev: nodes.clone(), nodes, spring: p.spring(length), start, end: if p.dangling { None } else { end } }
    }

    fn lock(&mut self) {
        self.nodes[0] = self.start;
        if let Some(e) = self.end {
            let last = self.nodes.len() - 1;
            self.nodes[last] = e;
        }
    }

    pub(crate) fn step(&mut self) -> f64 {
        self.step_with_wind(false)
    }

    fn step_with_wind(&mut self, apply_wind: bool) -> f64 {
        for (p, prev) in self.nodes.iter_mut().zip(self.prev.iter_mut()) {
            let force = if apply_wind { wind::rope_velocity_at(*p) * 10.0 } else { DVec3::ZERO };
            let accel = (DVec3::new(0.0, 0.0, -GRAVITY) + force) * (STEP * STEP * 0.5);
            let old = *p;
            *p = old + (old - *prev) * DAMPING + accel;
            *prev = old;
        }
        let spring2 = self.spring * self.spring;
        for _ in 0..PASSES {
            for i in 0..self.nodes.len() - 1 {
                let to = self.nodes[i] - self.nodes[i + 1];
                let d2 = to.length_squared();
                if d2 > spring2 {
                    let fix = to * (1.0 - self.spring / d2.sqrt());
                    self.nodes[i] -= fix * 0.5;
                    self.nodes[i + 1] += fix * 0.5;
                }
            }
            self.lock();
        }
        if !self.nodes.iter().all(|p| p.is_finite()) {
            let to = self.end.unwrap_or(self.start);
            let n = self.nodes.len();
            for (i, p) in self.nodes.iter_mut().enumerate() {
                *p = self.start.lerp(to, i as f64 / (n - 1) as f64);
            }
            self.prev = self.nodes.clone();
            return 0.0;
        }
        self.nodes.iter().zip(&self.prev).map(|(p, q)| (*p - *q).length()).fold(0.0, f64::max)
    }

    pub(crate) fn solve(&mut self) {
        let mut still = 0;
        for k in 0..MAX_STEPS {
            still = if self.step() < SETTLED { still + 1 } else { 0 };
            if still >= SETTLED_STEPS && k >= HANG_STEPS {
                break;
            }
        }
    }
}

pub(crate) struct RopePoints {
    pub points: Vec<DVec3>,
    pub at_node: Vec<f64>,
    pub v: Vec<f64>,
}

pub(crate) fn rope_points(nodes: &[DVec3], p: &RopeParams, length: i32) -> RopePoints {
    let ts: Vec<f64> = if p.barbed {
        BARBED_SUBDIVS.to_vec()
    } else {
        (0..p.subdiv).map(|i| (i + 1) as f64 / (p.subdiv + 1) as f64).collect()
    };
    let n = nodes.len();
    let last = n.saturating_sub(1);
    let mut out = RopePoints { points: Vec::new(), at_node: Vec::new(), v: Vec::new() };
    let mut prev = 0;
    for i in 0..n {
        out.points.push(nodes[i]);
        out.at_node.push(i as f64);
        if i < last {
            let next = i + 1;
            let next2 = (i + 2).min(last);
            let (p1, p2, p3, p4) = (nodes[prev], nodes[i], nodes[next], nodes[next2]);
            let t3 = 0.5 * (-p1 + 3.0 * p2 - 3.0 * p3 + p4);
            let t2 = 0.5 * (2.0 * p1 - 5.0 * p2 + 4.0 * p3 - p4);
            let t1 = 0.5 * (p3 - p1);
            for (k, t) in ts.iter().enumerate() {
                out.points.push(p2 + t1 * *t + t2 * (t * t) + t3 * (t * t * t));
                out.at_node.push(i as f64 + (k + 1) as f64 / (ts.len() + 1) as f64);
            }
            prev = i;
        }
    }
    let total_points = (last * ts.len() + 1) as f64;
    let inc = p.texels(length) / total_points;
    out.v = (0..out.points.len()).map(|k| k as f64 * inc).collect();
    out
}

pub(crate) fn rope_tube(pts: &RopePoints, width: f64, u_span: f64, v_scale: f64) -> (Vec<Vertex>, Vec<u32>) {
    let points = &pts.points;
    let n = points.len();
    let radius = width * 0.5;
    let mut vertices = Vec::with_capacity(n * (TUBE_SIDES + 1));
    let mut indices = Vec::with_capacity(n.saturating_sub(1) * TUBE_SIDES * 6);
    if n < 2 {
        return (vertices, indices);
    }
    let first_dir = (points[1] - points[0]).try_normalize().unwrap_or(DVec3::X);
    let (mut side, _) = first_dir.any_orthonormal_pair();
    for i in 0..n {
        let prev = points[i.saturating_sub(1)];
        let next = points[(i + 1).min(n - 1)];
        let tangent = (next - prev).try_normalize().unwrap_or(first_dir);
        side = (side - tangent * side.dot(tangent)).try_normalize().unwrap_or_else(|| tangent.any_orthonormal_pair().0);
        let up = tangent.cross(side);
        for k in 0..=TUBE_SIDES {
            let a = k as f64 / TUBE_SIDES as f64 * std::f64::consts::TAU;
            let dir = side * a.cos() + up * a.sin();
            let p = points[i] + dir * radius;
            vertices.push(Vertex {
                pos: p.as_vec3().to_array(),
                normal: dir.as_vec3().to_array(),
                uv: [(k as f64 / TUBE_SIDES as f64 * 2.0 * u_span) as f32, (pts.v[i] * v_scale) as f32],
                alpha: 0.0,
            });
        }
    }
    let ring = (TUBE_SIDES + 1) as u32;
    for i in 0..(n as u32 - 1) {
        for k in 0..TUBE_SIDES as u32 {
            let a = i * ring + k;
            let b = a + 1;
            let c = a + ring;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (vertices, indices)
}

pub(crate) fn rope_mesh(
    nodes: &[DVec3],
    p: &RopeParams,
    length: i32,
    tex_width: f64,
    material: u32,
    wire: [u8; 3],
) -> ObjectMesh {
    let pts = rope_points(nodes, p, length);
    let (vertices, indices) = rope_tube(&pts, p.width, tex_width, 1.0);
    let mut bounds = Aabb::empty();
    for v in &vertices {
        bounds.extend(glam::Vec3::from_array(v.pos).as_dvec3());
    }
    for q in &pts.points {
        bounds.extend(*q);
    }
    let mut edges = Vec::with_capacity(pts.points.len().saturating_sub(1) * 2);
    for w in pts.points.windows(2) {
        edges.push(w[0].as_vec3().to_array());
        edges.push(w[1].as_vec3().to_array());
    }
    let (aabb_min, aabb_max) = if bounds.is_empty() {
        ([0.0; 3], [0.0; 3])
    } else {
        (bounds.min.as_vec3().to_array(), bounds.max.as_vec3().to_array())
    };
    ObjectMesh {
        batches: vec![MeshBatch { material, vertices, indices, blend: Vec::new(), cubemaps: Vec::new() }],
        edges,
        aabb_min,
        aabb_max,
        wire_color: wire,
        ..Default::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ListSegment {
    pub start: usize,
    pub end: Option<usize>,
}

pub(crate) fn list_segments(list: &[(&Entity, DVec3)]) -> Vec<ListSegment> {
    let names: Vec<Box<str>> = list.iter().map(|(e, _)| lower(e.get("targetname").unwrap_or(""))).collect();
    let mut first: AHashMap<&str, usize> = AHashMap::new();
    for (i, n) in names.iter().enumerate() {
        if !n.is_empty() {
            first.entry(n).or_insert(i);
        }
    }
    let mut out = Vec::new();
    for (i, (e, _)) in list.iter().enumerate() {
        if !is_rope_class(&e.classname) {
            continue;
        }
        let next = lower(e.get("NextKey").unwrap_or(""));
        let end = if !searchable(&next) {
            None
        } else if next.ends_with('*') {
            names.iter().position(|n| name_matches(&next, n))
        } else {
            first.get(next.as_ref()).copied()
        };
        if end.is_some() || RopeParams::of(e).dangling {
            out.push(ListSegment { start: i, end });
        }
    }
    out
}

pub(crate) fn rest_nodes(a: DVec3, b: Option<DVec3>, p: &RopeParams) -> (Vec<DVec3>, i32) {
    let mut sim = RopeSim::new(a, b, p);
    sim.solve();
    (sim.nodes, b.map_or(0, |b| rope_length(a, b)))
}

struct RopeKey {
    next: Box<str>,
}

struct Segment {
    end: Option<Id>,
    params: RopeParams,
    length: i32,
    color: [u8; 3],
    sim: RopeSim,
    animating: bool,
    pending: bool,
}

struct InstanceRopes {
    keys: Vec<u64>,
}

#[derive(Default)]
pub(crate) struct RopeProducer {
    ropes: AHashMap<Id, RopeKey>,
    names: AHashMap<Id, Box<str>>,
    by_name: AHashMap<Box<str>, Vec<Id>>,
    by_next: AHashMap<Box<str>, Vec<Id>>,
    wildcards: AHashSet<Id>,
    segments: AHashMap<Id, Segment>,
    by_end: AHashMap<Id, Vec<Id>>,
    dirty: AHashSet<Id>,
    instances: AHashMap<Id, InstanceRopes>,
    inst_dirty: AHashSet<Id>,
    inst_state: Option<(u64, InstanceScene, Option<PathBuf>)>,
    shown: Option<(bool, bool)>,
    clear: bool,
    last_step: Option<Instant>,
}

fn push_unique(map: &mut AHashMap<Box<str>, Vec<Id>>, key: &str, id: Id) {
    if key.is_empty() {
        return;
    }
    let list = map.entry(key.into()).or_default();
    if !list.contains(&id) {
        list.push(id);
    }
}

fn remove_from(map: &mut AHashMap<Box<str>, Vec<Id>>, key: &str, id: Id) {
    if let Some(list) = map.get_mut(key) {
        list.retain(|x| *x != id);
        if list.is_empty() {
            map.remove(key);
        }
    }
}

impl RopeProducer {
    fn reset(&mut self) {
        self.ropes.clear();
        self.names.clear();
        self.by_name.clear();
        self.by_next.clear();
        self.wildcards.clear();
        self.segments.clear();
        self.by_end.clear();
        self.dirty.clear();
    }

    fn note_entity(&mut self, doc: &Document, fgd: Option<&hp_fgd::ResolvedFgd>, id: Id) {
        let node = doc.scene().entity(id);
        let new_name = node.map(|n| lower(n.entity.get("targetname").unwrap_or(""))).filter(|n| !n.is_empty());
        let old_name = self.names.get(&id).cloned();
        let new_rope = node
            .filter(|n| is_rope(&n.entity, class_of(fgd, &n.entity)))
            .map(|n| RopeKey { next: lower(n.entity.get("NextKey").unwrap_or("")) });
        let old_rope = self.ropes.remove(&id);
        for name in old_name.iter().chain(&new_name) {
            if let Some(list) = self.by_next.get(name) {
                self.dirty.extend(list.iter().copied());
            }
        }
        if old_name != new_name {
            self.dirty.extend(self.wildcards.iter().copied());
            if let Some(o) = &old_name {
                remove_from(&mut self.by_name, o, id);
            }
            match &new_name {
                Some(n) => {
                    push_unique(&mut self.by_name, n, id);
                    self.names.insert(id, n.clone());
                }
                None => {
                    self.names.remove(&id);
                }
            }
        }
        if let Some(starts) = self.by_end.get(&id) {
            self.dirty.extend(starts.iter().copied());
        }
        if old_rope.is_some() || new_rope.is_some() {
            self.dirty.insert(id);
        }
        if let Some(o) = old_rope {
            remove_from(&mut self.by_next, &o.next, id);
            self.wildcards.remove(&id);
        }
        if let Some(k) = new_rope {
            push_unique(&mut self.by_next, &k.next, id);
            if k.next.ends_with('*') {
                self.wildcards.insert(id);
            }
            self.ropes.insert(id, k);
        }
    }

    fn find_end(&self, doc: &Document, next: &str) -> Option<Id> {
        if !searchable(next) {
            return None;
        }
        let entities = doc.scene().entities();
        let order = |id: &Id| entities.get_index_of(id).unwrap_or(usize::MAX);
        if next.ends_with('*') {
            self.by_name
                .iter()
                .filter(|(n, _)| name_matches(next, n))
                .flat_map(|(_, l)| l.iter())
                .copied()
                .min_by_key(order)
        } else {
            self.by_name.get(next)?.iter().copied().min_by_key(order)
        }
    }

    fn set_end(&mut self, start: Id, old: Option<Id>, new: Option<Id>) {
        if old == new {
            return;
        }
        if let Some(o) = old
            && let Some(list) = self.by_end.get_mut(&o)
        {
            list.retain(|s| *s != start);
            if list.is_empty() {
                self.by_end.remove(&o);
            }
        }
        if let Some(n) = new {
            self.by_end.entry(n).or_default().push(start);
        }
    }

    fn rebuild(&mut self, cx: &mut ProducerCx<'_>, start: Id, animate: bool, out: &mut DerivedBuild) {
        let old_end = self.segments.get(&start).and_then(|s| s.end);
        let scene = cx.doc.scene();
        let fgd = cx.assets.fgd.clone();
        let built = self.ropes.get(&start).and_then(|k| {
            let node = scene.entity(start)?;
            let a = node.entity.origin().filter(|o| o.is_finite())?;
            let params = RopeParams::of(&node.entity);
            let end = self.find_end(cx.doc, &k.next);
            let b = end.and_then(|e| scene.entity(e)?.entity.origin().filter(|o| o.is_finite()));
            if b.is_none() && !params.dangling {
                return None;
            }
            let color = point_color(&node.entity, class_of(fgd.as_deref(), &node.entity));
            Some((end.filter(|_| b.is_some()), a, b, params, color))
        });
        let Some((end, a, b, params, color)) = built else {
            self.set_end(start, old_end, None);
            if self.segments.remove(&start).is_some() {
                out.remove.push(u64::from(start));
            }
            return;
        };
        self.set_end(start, old_end, end);
        let mut sim = RopeSim::new(a, b, &params);
        let length = b.map_or(0, |b| rope_length(a, b));
        if animate {
            if let Some(old) = self.segments.get(&start)
                && old.sim.nodes.len() == sim.nodes.len()
            {
                let n = sim.nodes.len();
                sim.nodes[1..n].copy_from_slice(&old.sim.nodes[1..n]);
                sim.prev[1..n].copy_from_slice(&old.sim.prev[1..n]);
                sim.lock();
            }
        } else {
            sim.solve();
        }
        self.segments.insert(start, Segment { end, params, length, color, sim, animating: animate, pending: false });
        self.upload(cx, start, out);
    }

    fn upload(&mut self, cx: &mut ProducerCx<'_>, start: Id, out: &mut DerivedBuild) {
        let Some(seg) = self.segments.get_mut(&start) else { return };
        let Some(tex_width) = texture_size(cx, &seg.params.material).map(|s| s.0) else {
            seg.pending = true;
            return;
        };
        seg.pending = false;
        let material = cx.assets.asset_material(&seg.params.material, MatKind::World);
        let mesh = rope_mesh(&seg.sim.nodes, &seg.params, seg.length, tex_width, material, seg.color);
        out.upsert.push(DerivedObject { key: u64::from(start), mesh, owner: Some(start), flags: ObjectFlags::ROPE });
    }

    fn build_instance(&mut self, cx: &mut ProducerCx<'_>, id: Id, out: &mut DerivedBuild) {
        let display = rt::scene().display;
        let entity = cx.doc.scene().entity(id).map(|n| &n.entity).filter(|e| is_instance(e));
        let Some(entity) = entity.filter(|_| display != InstanceDisplay::Hidden) else {
            if let Some(old) = self.instances.remove(&id) {
                out.remove.extend(old.keys);
            }
            return;
        };
        let fgd = cx.assets.fgd.clone();
        let exp = rt::expand_one(&InstanceCache::shared(), entity, cx.doc.path(), fgd.as_deref(), true);
        if exp.is_pending() {
            return;
        }
        let kinds = |c: &str, k: &str| fgd.as_deref().and_then(|f| rt::key_fixup(f, c, k));
        let ents: Vec<Entity> = exp.pieces.iter().flat_map(|p| p.point_entities(Some(&kinds), true)).collect();
        let list: Vec<(&Entity, DVec3)> =
            ents.iter().filter_map(|e| Some((e, e.origin().filter(|o| o.is_finite())?))).collect();
        let segs = list_segments(&list);
        let mut sizes = Vec::with_capacity(segs.len());
        for s in &segs {
            let params = RopeParams::of(list[s.start].0);
            let Some(size) = texture_size(cx, &params.material) else {
                self.inst_dirty.insert(id);
                return;
            };
            sizes.push((params, size.0));
        }
        let flags = match display {
            InstanceDisplay::Tinted => ObjectFlags::ROPE | ObjectFlags::INSTANCE,
            _ => ObjectFlags::ROPE,
        };
        let mut keys = Vec::with_capacity(segs.len());
        for (k, (s, (params, tex_width))) in segs.iter().zip(sizes).enumerate().take(MAX_INSTANCE_SEGMENTS) {
            let (e, a) = list[s.start];
            let b = s.end.map(|i| list[i].1);
            let (nodes, length) = rest_nodes(a, b, &params);
            let material = cx.assets.asset_material(&params.material, MatKind::World);
            let color = point_color(e, class_of(fgd.as_deref(), e));
            let mesh = rope_mesh(&nodes, &params, length, tex_width, material, color);
            let key = INSTANCE_KEY | (u64::from(id) << 16) | k as u64;
            keys.push(key);
            out.upsert.push(DerivedObject { key, mesh, owner: Some(id), flags });
        }
        if let Some(old) = self.instances.remove(&id) {
            out.remove.extend(old.keys.into_iter().filter(|k| !keys.contains(k)));
        }
        if !keys.is_empty() {
            self.instances.insert(id, InstanceRopes { keys });
        }
    }
}

fn texture_size(cx: &mut ProducerCx<'_>, material: &str) -> Option<(f64, f64)> {
    match cx.assets.material_info_impl(material) {
        None => None,
        Some(info) if info.missing || info.width == 0 || info.height == 0 => Some((MISSING_SIZE, MISSING_SIZE)),
        Some(info) => Some((f64::from(info.width), f64::from(info.height))),
    }
}

impl DerivedProducer for RopeProducer {
    fn name(&self) -> &'static str {
        "ropes"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        let d = scene_display();
        let replaced = d.game_look && cx.assets.play_world_shown();
        let display = (d.ropes && !replaced, d.simulate_ropes);
        if changes.full {
            self.reset();
            self.instances.clear();
            self.inst_dirty.clear();
        }
        let mut all = changes.full || changes.entities;
        if self.shown.map(|s| s.0) != Some(display.0) {
            if !display.0 {
                self.reset();
                self.instances.clear();
                self.inst_dirty.clear();
                self.shown = Some(display);
                self.clear = true;
                return true;
            }
            all = true;
        }
        self.shown = Some(display);
        if !display.0 {
            return self.clear;
        }
        let fgd = cx.assets.fgd.clone();
        let inst_state = (InstanceCache::shared().generation(), rt::scene(), cx.doc.path().map(PathBuf::from));
        if all || self.inst_state.as_ref() != Some(&inst_state) {
            self.inst_state = Some(inst_state);
            let scene = cx.doc.scene();
            self.inst_dirty.extend(scene.entities().values().filter(|n| is_instance(&n.entity)).map(|n| n.entity.id));
            self.inst_dirty.extend(self.instances.keys().copied());
        }
        if all {
            self.reset();
            for id in cx.doc.scene().entities().keys().copied().collect::<Vec<_>>() {
                self.note_entity(cx.doc, fgd.as_deref(), id);
            }
        } else {
            let ids: Vec<Id> =
                changes.doc.entities_changed.iter().chain(&changes.doc.entities_removed).copied().collect();
            for id in ids {
                self.note_entity(cx.doc, fgd.as_deref(), id);
                if self.instances.contains_key(&id) || cx.doc.scene().entity(id).is_some_and(|n| is_instance(&n.entity))
                {
                    self.inst_dirty.insert(id);
                }
            }
        }
        if display.1 && wind::active() {
            for (id, segment) in &mut self.segments {
                if segment.params.wind
                    && cx.doc.is_entity_visible(*id)
                    && segment.sim.nodes.iter().any(|p| wind::rope_velocity_at(*p).length_squared() > 1e-6)
                {
                    segment.animating = true;
                }
            }
        }
        let tick_due = self.last_step.is_none_or(|time| time.elapsed().as_secs_f64() >= STEP);
        let busy = self.segments.values().any(|s| s.pending || (s.animating && tick_due));
        !self.dirty.is_empty() || busy || self.clear || !self.inst_dirty.is_empty()
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut out = DerivedBuild::default();
        if std::mem::take(&mut self.clear) {
            out.clear = true;
            return out;
        }
        let animate = self.shown.is_some_and(|s| s.1);
        let mut ids: Vec<Id> = self.dirty.drain().collect();
        ids.sort_unstable();
        for id in &ids {
            self.rebuild(cx, *id, animate, &mut out);
        }
        let mut inst: Vec<Id> = self.inst_dirty.drain().collect();
        inst.sort_unstable();
        for id in inst {
            self.build_instance(cx, id, &mut out);
        }
        let now = Instant::now();
        let elapsed = self.last_step.map_or(STEP, |t| now.duration_since(t).as_secs_f64());
        self.last_step = Some(now);
        let steps = ((elapsed / STEP).round() as usize).clamp(1, MAX_FRAME_STEPS);
        let mut moving: Vec<Id> =
            self.segments.iter().filter(|(id, s)| s.animating && !ids.contains(id)).map(|(id, _)| *id).collect();
        moving.sort_unstable();
        for id in &moving {
            if let Some(seg) = self.segments.get_mut(id) {
                if animate {
                    let mut moved: f64 = 0.0;
                    for _ in 0..steps {
                        moved = seg.sim.step_with_wind(seg.params.wind);
                    }
                    if moved < SETTLED {
                        seg.animating = false;
                    }
                } else {
                    seg.sim.solve();
                    seg.animating = false;
                }
            }
            self.upload(cx, *id, &mut out);
        }
        let pending: Vec<Id> =
            self.segments.iter().filter(|(id, s)| s.pending && !moving.contains(id)).map(|(id, _)| *id).collect();
        for id in pending {
            self.upload(cx, id, &mut out);
        }
        if self.segments.values().any(|s| s.animating) {
            cx.assets.shared.request_repaint();
        } else {
            self.last_step = None;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(keys: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: "move_rope".to_owned(), ..Default::default() };
        for (k, v) in keys {
            e.set(k, *v);
        }
        e
    }

    fn rest(a: DVec3, b: Option<DVec3>, keys: &[(&str, &str)]) -> Vec<DVec3> {
        rest_nodes(a, b, &RopeParams::of(&rope(keys))).0
    }

    fn low(nodes: &[DVec3]) -> f64 {
        nodes.iter().map(|n| n.z).fold(f64::MAX, f64::min)
    }

    #[test]
    fn keys_like_the_server() {
        let p = RopeParams::of(&rope(&[
            ("Slack", "25"),
            ("Type", "0"),
            ("Subdiv", "2"),
            ("Width", "2"),
            ("TextureScale", "1"),
            ("RopeMaterial", "cable/cable.vmt"),
            ("Dangling", "0"),
            ("Barbed", "0"),
        ]));
        assert_eq!((p.slack, p.nodes, p.subdiv, p.width, p.texture_scale), (25, MAX_NODES, 2, 2.0, 1.0));
        assert_eq!(p.material, "cable/cable");
        assert!(!p.dangling && !p.barbed);
        let d = RopeParams::of(&rope(&[]));
        assert_eq!((d.slack, d.nodes, d.subdiv, d.width, d.texture_scale), (0, 5, 2, 2.0, 4.0));
        assert_eq!(RopeParams::of(&rope(&[("Type", "1")])).nodes, 4);
        assert_eq!(RopeParams::of(&rope(&[("Type", "2")])).nodes, 2);
        assert_eq!(RopeParams::of(&rope(&[("Slack", "40.9")])).slack, 40);
        assert_eq!(RopeParams::of(&rope(&[("Subdiv", "12")])).subdiv, MAX_SUBDIVS - 1);
        assert_eq!(RopeParams::of(&rope(&[("TextureScale", "50")])).texture_scale, 10.0);
        assert_eq!(RopeParams::of(&rope(&[("RopeShader", "1")])).material, "cable/rope");
        let h = RopeParams::of(&rope(&[("Width", "nan"), ("Slack", "1e99"), ("Subdiv", "-3")]));
        assert!(h.width.is_finite() && h.subdiv < MAX_SUBDIVS);
        assert_eq!(p.spring(300), 25.0);
        assert_eq!(p.spring(10), 0.0);
    }

    #[test]
    fn sag_grows_with_slack() {
        let (a, b) = (DVec3::new(0.0, 0.0, 400.0), DVec3::new(384.0, 0.0, 400.0));
        let mut last = 0.0;
        for slack in [0, 25, 100, 200, 400] {
            let s = slack.to_string();
            let nodes = rest(a, Some(b), &[("Slack", &s), ("Type", "0")]);
            assert_eq!(nodes[0], a);
            assert_eq!(nodes[MAX_NODES - 1], b);
            let sag = 400.0 - low(&nodes);
            assert!(sag > last, "slack {slack}: sag {sag} after {last}");
            last = sag;
            assert!((nodes[3].z - nodes[6].z).abs() < 0.5, "{nodes:?}");
        }
        let deep = rest(a, Some(b), &[("Slack", "200")]);
        let sag = 400.0 - low(&deep);
        assert!(sag > 120.0 && sag < 220.0, "{sag}");
        let taut = rest(a, Some(b), &[("Slack", "0")]);
        let sag = 400.0 - low(&taut);
        assert!(sag > 0.1 && sag < 10.0, "{sag}");
    }

    #[test]
    fn dangling_and_rigid() {
        let (a, b) = (DVec3::new(0.0, 0.0, 400.0), DVec3::new(256.0, 0.0, 400.0));
        let d = rest(a, Some(b), &[("Dangling", "1"), ("Slack", "150"), ("Type", "0")]);
        assert_eq!(d[0], a);
        for n in &d {
            assert!((n.x - a.x).abs() < 1.0 && n.y.abs() < 1e-6, "{n}");
        }
        let len = 400.0 - low(&d);
        assert!(len > 300.0 && len < 340.0, "{len}");
        let alone = rest(a, None, &[("Dangling", "1"), ("Slack", "300")]);
        let len = 400.0 - low(&alone);
        assert!(len > 195.0 && len < 225.0, "{len}");
        let none = rest(a, None, &[("Dangling", "1"), ("Slack", "25")]);
        let reach = none.iter().map(|n| (*n - a).length()).fold(0.0, f64::max);
        assert!(reach < 8.0, "{reach}");
        let r = rest(a, Some(b), &[("Type", "2"), ("Slack", "500")]);
        assert_eq!(r, vec![a, b]);
        let far = rest(DVec3::splat(1e300), Some(DVec3::splat(-1e300)), &[("Type", "1")]);
        assert!(far.iter().all(|n| n.is_finite()));
    }

    #[test]
    fn spline_points_and_texture() {
        let p = RopeParams::of(&rope(&[("Type", "1"), ("Subdiv", "3"), ("TextureScale", "2"), ("Slack", "50")]));
        let nodes =
            [DVec3::ZERO, DVec3::new(100.0, 0.0, -20.0), DVec3::new(200.0, 0.0, -20.0), DVec3::new(300.0, 0.0, 0.0)];
        let pts = rope_points(&nodes, &p, 300);
        assert_eq!(pts.points.len(), 4 + 3 * 3);
        assert_eq!(pts.points[0], nodes[0]);
        assert_eq!(pts.points[4], nodes[1]);
        assert_eq!(*pts.points.last().unwrap(), nodes[3]);
        assert!((pts.at_node[2] - 0.5).abs() < 1e-9);
        let inc = 2.0 * 250.0 / 10.0;
        assert!((pts.v[1] - inc).abs() < 1e-9 && (pts.v[12] - 12.0 * inc).abs() < 1e-9);
        let barbed = RopeParams { barbed: true, ..p.clone() };
        let b = rope_points(&nodes, &barbed, 300);
        assert_eq!(b.points.len(), 4 + 3 * 3);
        assert!(b.points[1].x > 100.0, "1.5 overshoots the next node: {}", b.points[1]);
        let mesh = rope_mesh(&nodes, &p, 300, 32.0, 5, [9, 9, 9]);
        let batch = &mesh.batches[0];
        assert_eq!(batch.material, 5);
        assert_eq!(batch.vertices.len(), pts.points.len() * (TUBE_SIDES + 1));
        assert_eq!(batch.indices.len(), (pts.points.len() - 1) * TUBE_SIDES * 6);
        assert!(batch.indices.iter().all(|i| (*i as usize) < batch.vertices.len()));
        assert_eq!(mesh.edges.len(), (pts.points.len() - 1) * 2);
        for v in &batch.vertices[..TUBE_SIDES] {
            assert!((glam::Vec3::from_array(v.pos).length() - 1.0).abs() < 1e-4, "radius = Width / 2");
        }
        assert!((batch.vertices[TUBE_SIDES / 2].uv[0] - 32.0).abs() < 1e-4);
    }

    #[test]
    fn links_like_the_engine() {
        let ent = |class: &str, name: &str, next: &str| {
            let mut e = Entity { classname: class.to_owned(), ..Default::default() };
            if !name.is_empty() {
                e.set("targetname", name);
            }
            if !next.is_empty() {
                e.set("NextKey", next);
            }
            e
        };
        let mut dangling = ent("move_rope", "d", "missing");
        dangling.set("Dangling", "1");
        let ents = [
            ent("move_rope", "a", "B"),
            ent("keyframe_rope", "b", "c"),
            ent("keyframe_rope", "b", ""),
            ent("info_target", "c", ""),
            ent("move_rope", "e", "missing"),
            ent("move_rope", "f", "pre*"),
            ent("keyframe_rope", "prefix_2", ""),
            dangling,
            ent("move_rope", "g", "!player"),
            ent("keyframe_rope", "h", ""),
        ];
        let list: Vec<(&Entity, DVec3)> = ents.iter().map(|e| (e, DVec3::ZERO)).collect();
        let segs = list_segments(&list);
        assert_eq!(
            segs,
            vec![
                ListSegment { start: 0, end: Some(1) },
                ListSegment { start: 1, end: Some(3) },
                ListSegment { start: 5, end: Some(6) },
                ListSegment { start: 7, end: None },
            ]
        );
    }
}
