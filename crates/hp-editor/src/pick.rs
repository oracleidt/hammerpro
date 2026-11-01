use std::sync::{Arc, PoisonError};

use glam::{DVec2, DVec3};
use hp_geom::{Aabb, Ray, ViewAxis, point_segment_distance_2d, ray_solid, ray_triangle};
use hp_vmf::Entity;
use rayon::prelude::*;

use crate::Id;
use crate::document::Document;
use crate::ids::{IdSet, id_set};
use crate::scene::SolidNode;
use crate::selection::{ObjRef, SelectMode, point_entity_bounds};

#[derive(Clone, Copy)]
pub struct PickOptions<'a> {
    pub include_solids: bool,
    pub include_entities: bool,
    pub ignore_hidden: bool,
    pub max_distance: f64,
    pub filter: Option<&'a dyn Fn(ObjRef) -> bool>,
    pub face_hidden: Option<&'a dyn Fn(&str) -> bool>,
    pub entity_ray: Option<&'a EntityRayFn<'a>>,
}

pub type EntityRayFn<'a> = dyn Fn(&Entity, &Ray) -> EntityRay + 'a;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntityRay {
    Bounds,
    Miss,
    Hit { t: f64, normal: DVec3 },
}

impl Default for PickOptions<'_> {
    fn default() -> Self {
        Self {
            include_solids: true,
            include_entities: true,
            ignore_hidden: true,
            max_distance: f64::INFINITY,
            filter: None,
            face_hidden: None,
            entity_ray: None,
        }
    }
}

impl std::fmt::Debug for PickOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PickOptions")
            .field("include_solids", &self.include_solids)
            .field("include_entities", &self.include_entities)
            .field("ignore_hidden", &self.ignore_hidden)
            .field("max_distance", &self.max_distance)
            .field("filter", &self.filter.is_some())
            .field("face_hidden", &self.face_hidden.is_some())
            .field("entity_ray", &self.entity_ray.is_some())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub obj: ObjRef,
    pub face: Option<usize>,
    pub t: f64,
    pub point: DVec3,
    pub normal: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum RectMode {
    #[default]
    Touching,
    Inside,
    Center,
}

const LEAF_SIZE: usize = 4;

#[derive(Clone, Copy, Debug)]
struct BvhNode {
    bounds: Aabb,
    first: u32,
    count: u32,
}

#[derive(Clone, Copy, Debug)]
struct BvhItem {
    id: Id,
    bounds: Aabb,
    center: DVec3,
}

#[derive(Debug, Default)]
pub(crate) struct Bvh {
    nodes: Vec<BvhNode>,
    items: Vec<BvhItem>,
}

impl Bvh {
    fn build(mut items: Vec<BvhItem>) -> Bvh {
        if items.is_empty() {
            return Bvh::default();
        }
        let mut nodes: Vec<BvhNode> = Vec::with_capacity(2 * items.len() / LEAF_SIZE + 2);
        nodes.push(BvhNode { bounds: Aabb::EMPTY, first: 0, count: 0 });
        let mut stack: Vec<(usize, usize, usize)> = vec![(0, 0, items.len())];
        while let Some((node, start, end)) = stack.pop() {
            let Some(slice) = items.get_mut(start..end) else { continue };
            let mut bounds = Aabb::EMPTY;
            let mut cb = Aabb::EMPTY;
            for it in slice.iter() {
                bounds = bounds.union(&it.bounds);
                cb.extend(it.center);
            }
            let n = end - start;
            if n <= LEAF_SIZE {
                if let Some(nd) = nodes.get_mut(node) {
                    *nd = BvhNode { bounds, first: start as u32, count: n as u32 };
                }
                continue;
            }
            let size = cb.size();
            let axis = if size.x >= size.y && size.x >= size.z {
                0
            } else if size.y >= size.z {
                1
            } else {
                2
            };
            let mid = n / 2;
            slice.select_nth_unstable_by(mid, |a, b| a.center[axis].total_cmp(&b.center[axis]));
            let left = nodes.len();
            nodes.push(BvhNode { bounds: Aabb::EMPTY, first: 0, count: 0 });
            nodes.push(BvhNode { bounds: Aabb::EMPTY, first: 0, count: 0 });
            if let Some(nd) = nodes.get_mut(node) {
                *nd = BvhNode { bounds, first: left as u32, count: 0 };
            }
            stack.push((left + 1, start + mid, end));
            stack.push((left, start, start + mid));
        }
        Bvh { nodes, items }
    }

    fn ray(&self, ray: &Ray, mut limit: f64, mut visit: impl FnMut(Id, f64) -> f64) {
        let Some(root) = self.nodes.first() else { return };
        let rt = RayTester::new(ray);
        let Some(t_root) = rt.test(&root.bounds, limit) else { return };
        let mut stack: Vec<(u32, f64)> = Vec::with_capacity(64);
        stack.push((0, t_root));
        while let Some((ni, t_entry)) = stack.pop() {
            if t_entry > limit {
                continue;
            }
            let Some(node) = self.nodes.get(ni as usize) else { continue };
            if node.count > 0 {
                let start = node.first as usize;
                let end = start + node.count as usize;
                for it in self.items.get(start..end).unwrap_or(&[]) {
                    if let Some(t) = rt.test(&it.bounds, limit) {
                        limit = visit(it.id, t).min(limit);
                    }
                }
                continue;
            }
            let (l, r) = (node.first, node.first + 1);
            let tl = self.nodes.get(l as usize).and_then(|n| rt.test(&n.bounds, limit));
            let tr = self.nodes.get(r as usize).and_then(|n| rt.test(&n.bounds, limit));
            match (tl, tr) {
                (Some(a), Some(b)) => {
                    if a <= b {
                        stack.push((r, b));
                        stack.push((l, a));
                    } else {
                        stack.push((l, a));
                        stack.push((r, b));
                    }
                }
                (Some(a), None) => stack.push((l, a)),
                (None, Some(b)) => stack.push((r, b)),
                (None, None) => {}
            }
        }
    }

    fn query_box(&self, b: &Aabb, mut visit: impl FnMut(Id)) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack: Vec<u32> = Vec::with_capacity(64);
        stack.push(0);
        while let Some(ni) = stack.pop() {
            let Some(node) = self.nodes.get(ni as usize) else { continue };
            if !node.bounds.intersects(b) {
                continue;
            }
            if node.count > 0 {
                let start = node.first as usize;
                let end = start + node.count as usize;
                for it in self.items.get(start..end).unwrap_or(&[]) {
                    if it.bounds.intersects(b) {
                        visit(it.id);
                    }
                }
            } else {
                stack.push(node.first);
                stack.push(node.first + 1);
            }
        }
    }
}

struct RayTester {
    origin: [f64; 3],
    inv: [f64; 3],
    parallel: [bool; 3],
}

impl RayTester {
    fn new(ray: &Ray) -> Self {
        let o = ray.origin.to_array();
        let d = ray.dir.to_array();
        let mut inv = [0.0; 3];
        let mut parallel = [false; 3];
        for k in 0..3 {
            if d[k].abs() < 1e-300 {
                parallel[k] = true;
            } else {
                inv[k] = 1.0 / d[k];
            }
        }
        Self { origin: o, inv, parallel }
    }

    #[inline]
    fn test(&self, b: &Aabb, limit: f64) -> Option<f64> {
        let lo = b.min.to_array();
        let hi = b.max.to_array();
        let mut t0 = 0.0f64;
        let mut t1 = limit;
        for k in 0..3 {
            if self.parallel[k] {
                if self.origin[k] < lo[k] || self.origin[k] > hi[k] {
                    return None;
                }
                continue;
            }
            let mut a = (lo[k] - self.origin[k]) * self.inv[k];
            let mut c = (hi[k] - self.origin[k]) * self.inv[k];
            if a > c {
                std::mem::swap(&mut a, &mut c);
            }
            t0 = t0.max(a);
            t1 = t1.min(c);
            if t0 > t1 {
                return None;
            }
        }
        Some(t0)
    }
}

#[derive(Debug, Default)]
pub(crate) struct PickCache {
    bvh: Option<Arc<Bvh>>,
    dirty: IdSet,
}

impl PickCache {
    pub fn note_changed(&mut self, id: Id) {
        if self.bvh.is_some() {
            self.dirty.insert(id);
        }
    }
}

fn ray_node(ray: &Ray, n: &SolidNode, face_hidden: Option<&dyn Fn(&str) -> bool>) -> Option<(f64, Option<usize>, DVec3)> {
    let hidden = |side: usize| {
        face_hidden.is_some_and(|f| n.solid.sides.get(side).is_some_and(|s| f(&s.material)))
    };
    if n.disp.iter().any(Option::is_some) {
        let mut best: Option<(f64, Option<usize>, DVec3)> = None;
        for (side, mesh) in n.disp.iter().enumerate() {
            let Some(mesh) = mesh else { continue };
            if hidden(side) {
                continue;
            }
            if mesh.aabb.expand(1e-6).ray_intersect(ray).is_none() {
                continue;
            }
            for tri in mesh.indices.chunks_exact(3) {
                let (Some(&a), Some(&b), Some(&c)) = (
                    mesh.positions.get(tri[0] as usize),
                    mesh.positions.get(tri[1] as usize),
                    mesh.positions.get(tri[2] as usize),
                ) else {
                    continue;
                };
                if let Some(t) = ray_triangle(ray, a, b, c, false)
                    && best.is_none_or(|bt| t < bt.0)
                {
                    let normal = (b - a).cross(c - a).try_normalize().unwrap_or(mesh.face_normal);
                    best = Some((t, Some(side), normal));
                }
            }
        }
        return best;
    }
    let (t, fi) = ray_solid(ray, &n.geom)?;
    let face = n.geom.faces.get(fi)?;
    if hidden(face.side_index) {
        return None;
    }
    Some((t, Some(face.side_index), face.plane.normal))
}

fn box_normal(b: &Aabb, p: DVec3) -> DVec3 {
    let candidates = [
        ((p.x - b.min.x).abs(), DVec3::NEG_X),
        ((p.x - b.max.x).abs(), DVec3::X),
        ((p.y - b.min.y).abs(), DVec3::NEG_Y),
        ((p.y - b.max.y).abs(), DVec3::Y),
        ((p.z - b.min.z).abs(), DVec3::NEG_Z),
        ((p.z - b.max.z).abs(), DVec3::Z),
    ];
    candidates.iter().min_by(|a, b| a.0.total_cmp(&b.0)).map_or(DVec3::Z, |c| c.1)
}

pub(crate) fn hull_2d(mut pts: Vec<DVec2>) -> Vec<DVec2> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| (*a - *b).length_squared() < 1e-12);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<DVec2> = Vec::with_capacity(pts.len() * 2);
    for &p in &pts {
        while hull.len() >= 2 {
            let (Some(&a), Some(&b)) = (hull.get(hull.len() - 2), hull.last()) else { break };
            if cross(a, b, p) <= 0.0 {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(p);
    }
    let lower = hull.len() + 1;
    for &p in pts.iter().rev().skip(1) {
        while hull.len() >= lower {
            let (Some(&a), Some(&b)) = (hull.get(hull.len() - 2), hull.last()) else { break };
            if cross(a, b, p) <= 0.0 {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(p);
    }
    hull.pop();
    hull
}

fn convex_overlaps_rect(hull: &[DVec2], min: DVec2, max: DVec2) -> bool {
    if hull.is_empty() {
        return false;
    }
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    for p in hull {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    if lo.x > max.x || hi.x < min.x || lo.y > max.y || hi.y < min.y {
        return false;
    }
    if hull.len() < 3 {
        if let (Some(&a), Some(&b)) = (hull.first(), hull.last()) {
            let n = (b - a).perp();
            if n.length_squared() > 0.0 {
                return axis_overlaps(n, hull, min, max);
            }
        }
        return true;
    }
    let n = hull.len();
    for i in 0..n {
        let (Some(&a), Some(&b)) = (hull.get(i), hull.get((i + 1) % n)) else { continue };
        if !axis_overlaps((b - a).perp(), hull, min, max) {
            return false;
        }
    }
    true
}

fn axis_overlaps(axis: DVec2, hull: &[DVec2], min: DVec2, max: DVec2) -> bool {
    let (mut a0, mut a1) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in hull {
        let d = axis.dot(*p);
        a0 = a0.min(d);
        a1 = a1.max(d);
    }
    let corners = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
    let (mut b0, mut b1) = (f64::INFINITY, f64::NEG_INFINITY);
    for c in corners {
        let d = axis.dot(c);
        b0 = b0.min(d);
        b1 = b1.max(d);
    }
    a0 <= b1 && b0 <= a1
}

fn column_box(axis: ViewAxis, min: DVec2, max: DVec2) -> Aabb {
    let (h, v) = axis.axes_2d();
    let d = axis.depth_axis();
    let mut lo = [0.0; 3];
    let mut hi = [0.0; 3];
    for k in 0..3 {
        let (a, b) = if k == h {
            (min.x, max.x)
        } else if k == v {
            (min.y, max.y)
        } else {
            debug_assert_eq!(k, d);
            (f64::NEG_INFINITY, f64::INFINITY)
        };
        lo[k] = a;
        hi[k] = b;
    }
    Aabb { min: DVec3::from_array(lo), max: DVec3::from_array(hi) }
}

impl Document {
    fn with_bvh<R>(&self, f: impl FnOnce(&Bvh, &IdSet) -> R) -> R {
        let mut cache = self.pick.lock().unwrap_or_else(PoisonError::into_inner);
        let n = self.scene.solids.len();
        let rebuild = cache.bvh.is_none() || cache.dirty.len() > (n / 16).max(256);
        if rebuild {
            let items: Vec<BvhItem> = self
                .scene
                .solids
                .par_values()
                .filter(|s| !s.bounds.is_empty() && s.bounds.min.is_finite() && s.bounds.max.is_finite())
                .map(|s| BvhItem { id: s.solid.id, bounds: s.bounds, center: s.bounds.center() })
                .collect();
            cache.bvh = Some(Arc::new(Bvh::build(items)));
            cache.dirty = id_set(0);
        }
        let bvh = cache.bvh.clone().unwrap_or_default();
        f(&bvh, &cache.dirty)
    }

    pub fn prepare_picking(&self) {
        self.with_bvh(|_, _| ());
    }

    fn ray_candidates(&self, ray: &Ray, limit: f64, mut visit: impl FnMut(&SolidNode, f64) -> f64) {
        self.with_bvh(|bvh, dirty| {
            let mut limit = limit;
            bvh.ray(ray, limit, |id, t| {
                if dirty.contains(&id) {
                    return f64::INFINITY;
                }
                match self.scene.solids.get(&id) {
                    Some(n) => {
                        let l = visit(n, t);
                        limit = limit.min(l);
                        l
                    }
                    None => f64::INFINITY,
                }
            });
            let rt = RayTester::new(ray);
            for id in dirty {
                if let Some(n) = self.scene.solids.get(id)
                    && !n.bounds.is_empty()
                    && let Some(t) = rt.test(&n.bounds, limit)
                {
                    limit = limit.min(visit(n, t));
                }
            }
        });
    }

    fn box_candidates(&self, b: &Aabb, mut visit: impl FnMut(&SolidNode)) {
        self.with_bvh(|bvh, dirty| {
            bvh.query_box(b, |id| {
                if !dirty.contains(&id)
                    && let Some(n) = self.scene.solids.get(&id)
                {
                    visit(n);
                }
            });
            for id in dirty {
                if let Some(n) = self.scene.solids.get(id)
                    && !n.bounds.is_empty()
                    && n.bounds.intersects(b)
                {
                    visit(n);
                }
            }
        });
    }

    pub fn solids_in_box(&self, b: &Aabb) -> Vec<Id> {
        let mut out = Vec::new();
        self.box_candidates(b, |n| out.push(n.solid.id));
        out
    }

    fn solid_pickable(&self, id: Id, opts: &PickOptions<'_>) -> bool {
        if opts.ignore_hidden && !self.is_solid_pickable(id) {
            return false;
        }
        opts.filter.is_none_or(|f| f(ObjRef::Solid(id)))
    }

    fn ray_hits(
        &self,
        ray: &Ray,
        opts: &PickOptions<'_>,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
        closest_only: bool,
    ) -> Vec<Hit> {
        let mut hits: Vec<Hit> = Vec::new();
        if !ray.origin.is_finite() || !ray.dir.is_finite() || ray.dir.length_squared() < 1e-24 {
            return hits;
        }
        let max_t = if opts.max_distance.is_nan() { f64::INFINITY } else { opts.max_distance.max(0.0) };
        let mut best = max_t;
        let facing = |n: DVec3| if n.dot(ray.dir) > 0.0 { -n } else { n };
        if opts.include_solids {
            self.ray_candidates(ray, max_t, |n, _| {
                let id = n.solid.id;
                if !self.solid_pickable(id, opts) {
                    return if closest_only { best } else { max_t };
                }
                if let Some((t, face, normal)) = ray_node(ray, n, opts.face_hidden)
                    && t <= best
                {
                    let hit = Hit { obj: ObjRef::Solid(id), face, t, point: ray.at(t), normal: facing(normal) };
                    if closest_only {
                        best = t;
                        hits.clear();
                    }
                    hits.push(hit);
                }
                if closest_only { best } else { max_t }
            });
        }
        if opts.include_entities {
            let rt = RayTester::new(ray);
            for node in self.scene.entities.values() {
                let id = node.entity.id;
                if self.scene.is_brush_entity(id) {
                    continue;
                }
                let obj = ObjRef::Entity(id);
                if opts.ignore_hidden && !self.is_entity_pickable(id) {
                    continue;
                }
                if opts.filter.is_some_and(|f| !f(obj)) {
                    continue;
                }
                let Some(b) = point_entity_bounds(&node.entity, entity_bounds) else { continue };
                let limit = if closest_only { best } else { max_t };
                if rt.test(&b, limit).is_none() {
                    continue;
                }
                let found = match opts.entity_ray.map_or(EntityRay::Bounds, |f| f(&node.entity, ray)) {
                    EntityRay::Miss => None,
                    EntityRay::Hit { t, normal } => {
                        (t.is_finite() && t >= 0.0 && t <= limit).then(|| (t, facing(normal)))
                    }
                    EntityRay::Bounds if b.contains_point(ray.origin) => None,
                    EntityRay::Bounds => rt.test(&b, limit).map(|t| (t, box_normal(&b, ray.at(t)))),
                };
                if let Some((t, normal)) = found {
                    let point = ray.at(t);
                    let hit = Hit { obj, face: None, t, point, normal };
                    if closest_only {
                        best = t;
                        hits.clear();
                    }
                    hits.push(hit);
                }
            }
        }
        hits.sort_by(|a, b| a.t.total_cmp(&b.t));
        hits
    }

    pub fn pick_ray(
        &self,
        ray: &Ray,
        opts: &PickOptions<'_>,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
    ) -> Option<Hit> {
        self.ray_hits(ray, opts, entity_bounds, true).into_iter().next()
    }

    pub fn pick_ray_all(
        &self,
        ray: &Ray,
        opts: &PickOptions<'_>,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
    ) -> Vec<Hit> {
        self.ray_hits(ray, opts, entity_bounds, false)
    }

    pub fn pick_2d(
        &self,
        axis: ViewAxis,
        point: DVec2,
        tolerance: f64,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
    ) -> Vec<ObjRef> {
        if !point.is_finite() {
            return Vec::new();
        }
        let tol = if tolerance.is_finite() { tolerance.max(0.0) } else { 0.0 };
        let fwd = axis.forward();
        let mut found: Vec<(f64, f64, ObjRef)> = Vec::new();
        let qb = column_box(axis, point - DVec2::splat(tol), point + DVec2::splat(tol));
        self.box_candidates(&qb, |n| {
            let id = n.solid.id;
            if !self.is_solid_pickable(id) {
                return;
            }
            let mut best = f64::INFINITY;
            for f in &n.geom.faces {
                let k = f.vertices.len();
                for i in 0..k {
                    let (Some(&a), Some(&b)) = (f.vertices.get(i), f.vertices.get((i + 1) % k)) else { continue };
                    best = best.min(point_segment_distance_2d(point, axis.to_2d(a), axis.to_2d(b)));
                }
            }
            for mesh in n.disp.iter().flatten() {
                let r = mesh.row_len;
                for (i, p) in mesh.positions.iter().enumerate() {
                    let p2 = axis.to_2d(*p);
                    if (i + 1) % r != 0
                        && let Some(q) = mesh.positions.get(i + 1)
                    {
                        best = best.min(point_segment_distance_2d(point, p2, axis.to_2d(*q)));
                    }
                    if let Some(q) = mesh.positions.get(i + r) {
                        best = best.min(point_segment_distance_2d(point, p2, axis.to_2d(*q)));
                    }
                }
            }
            let center = n.bounds.center();
            best = best.min(axis.to_2d(center).distance(point));
            if best <= tol {
                found.push((best, -center.dot(fwd), ObjRef::Solid(id)));
            }
        });
        for node in self.scene.entities.values() {
            let id = node.entity.id;
            if self.scene.is_brush_entity(id) || !self.is_entity_pickable(id) {
                continue;
            }
            let Some(b) = point_entity_bounds(&node.entity, entity_bounds) else { continue };
            let (lo, hi) = (axis.to_2d(b.min) - DVec2::splat(tol), axis.to_2d(b.max) + DVec2::splat(tol));
            if point.x < lo.x || point.y < lo.y || point.x > hi.x || point.y > hi.y {
                continue;
            }
            let (bl, bh) = (axis.to_2d(b.min), axis.to_2d(b.max));
            let edge = [(point.x - bl.x).abs(), (point.x - bh.x).abs(), (point.y - bl.y).abs(), (point.y - bh.y).abs()]
                .into_iter()
                .fold(f64::INFINITY, f64::min);
            let center = axis.to_2d(b.center()).distance(point);
            found.push((edge.min(center), -b.center().dot(fwd), ObjRef::Entity(id)));
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.total_cmp(&a.1)));
        found.into_iter().map(|(_, _, o)| o).collect()
    }

    pub fn objects_in_rect_2d(
        &self,
        axis: ViewAxis,
        min: DVec2,
        max: DVec2,
        mode: RectMode,
        entity_bounds: &dyn Fn(&Entity) -> Option<Aabb>,
    ) -> Vec<ObjRef> {
        if !min.is_finite() || !max.is_finite() {
            return Vec::new();
        }
        let (min, max) = (min.min(max), min.max(max));
        let mut solids: Vec<(u64, Id)> = Vec::new();
        let qb = column_box(axis, min, max);
        self.box_candidates(&qb, |n| {
            let id = n.solid.id;
            if !self.is_solid_pickable(id) {
                return;
            }
            let (lo, hi) = (axis.to_2d(n.bounds.min), axis.to_2d(n.bounds.max));
            let ok = match mode {
                RectMode::Center => {
                    let c = axis.to_2d(n.bounds.center());
                    c.x >= min.x && c.y >= min.y && c.x <= max.x && c.y <= max.y
                }
                RectMode::Inside => lo.x >= min.x && lo.y >= min.y && hi.x <= max.x && hi.y <= max.y,
                RectMode::Touching => {
                    let fully_inside = lo.x >= min.x && lo.y >= min.y && hi.x <= max.x && hi.y <= max.y;
                    let centroid_in = || {
                        let count = n.geom.face_vertex_count().max(1) as f64;
                        let c = axis.to_2d(n.geom.vertices().fold(DVec3::ZERO, |a, v| a + v) / count);
                        n.disp.is_empty() && c.x >= min.x && c.y >= min.y && c.x <= max.x && c.y <= max.y
                    };
                    fully_inside || centroid_in() || {
                        let mut pts: Vec<DVec2> = n.geom.vertices().map(|v| axis.to_2d(v)).collect();
                        for mesh in n.disp.iter().flatten() {
                            pts.extend(mesh.positions.iter().map(|p| axis.to_2d(*p)));
                        }
                        convex_overlaps_rect(&hull_2d(pts), min, max)
                    }
                }
            };
            if ok {
                solids.push((n.order, id));
            }
        });
        solids.sort_unstable();
        let mut out: Vec<ObjRef> = solids.into_iter().map(|(_, id)| ObjRef::Solid(id)).collect();
        for node in self.scene.entities.values() {
            let id = node.entity.id;
            if self.scene.is_brush_entity(id) || !self.is_entity_pickable(id) {
                continue;
            }
            let Some(b) = point_entity_bounds(&node.entity, entity_bounds) else { continue };
            let (lo, hi) = (axis.to_2d(b.min), axis.to_2d(b.max));
            let ok = match mode {
                RectMode::Center => {
                    let c = axis.to_2d(b.center());
                    c.x >= min.x && c.y >= min.y && c.x <= max.x && c.y <= max.y
                }
                RectMode::Inside => lo.x >= min.x && lo.y >= min.y && hi.x <= max.x && hi.y <= max.y,
                RectMode::Touching => !(lo.x > max.x || hi.x < min.x || lo.y > max.y || hi.y < min.y),
            };
            if ok {
                out.push(ObjRef::Entity(id));
            }
        }
        out
    }

    pub fn resolve_many(&self, objs: &[ObjRef], mode: SelectMode) -> Vec<ObjRef> {
        let mut out = indexmap::IndexSet::<ObjRef, crate::IdBuildHasher>::default();
        for o in objs {
            out.extend(self.scene.resolve(*o, mode));
        }
        out.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bvh_queries() {
        let mut items = Vec::new();
        for i in 0..1000u32 {
            let x = f64::from(i) * 10.0;
            let b = Aabb::new(DVec3::new(x, 0.0, 0.0), DVec3::new(x + 8.0, 8.0, 8.0));
            items.push(BvhItem { id: i, bounds: b, center: b.center() });
        }
        let bvh = Bvh::build(items);
        let mut hits = Vec::new();
        bvh.ray(&Ray::new(DVec3::new(-100.0, 4.0, 4.0), DVec3::X), f64::INFINITY, |id, t| {
            hits.push((id, t));
            f64::INFINITY
        });
        assert_eq!(hits.len(), 1000);
        let mut first = None;
        bvh.ray(&Ray::new(DVec3::new(-100.0, 4.0, 4.0), DVec3::X), f64::INFINITY, |id, t| {
            if first.is_none_or(|(_, ft)| t < ft) {
                first = Some((id, t));
            }
            t
        });
        assert_eq!(first.map(|f| f.0), Some(0));
        let mut boxed = Vec::new();
        bvh.query_box(&Aabb::new(DVec3::new(15.0, 0.0, 0.0), DVec3::new(35.0, 1.0, 1.0)), |id| boxed.push(id));
        boxed.sort_unstable();
        assert_eq!(boxed, vec![1, 2, 3]);
        let empty = Bvh::build(Vec::new());
        empty.query_box(&Aabb::new(DVec3::ZERO, DVec3::ONE), |_| panic!("no items"));
    }

    #[test]
    fn center_rectangles() {
        let mut doc = Document::new_empty();
        let big = doc
            .create_solid(
                crate::tests::box_sides(DVec3::ZERO, DVec3::new(256.0, 256.0, 64.0), "A"),
                crate::Owner::World,
            )
            .expect("solid");
        let e = doc.create_point_entity("info_target", DVec3::new(500.0, 500.0, 0.0), &[]).expect("entity");
        let none = |_: &Entity| None;
        let r = doc.objects_in_rect_2d(ViewAxis::Top, DVec2::ZERO, DVec2::splat(64.0), RectMode::Center, &none);
        assert!(r.is_empty());
        let r =
            doc.objects_in_rect_2d(ViewAxis::Top, DVec2::splat(100.0), DVec2::splat(600.0), RectMode::Center, &none);
        assert_eq!(r, vec![ObjRef::Solid(big), ObjRef::Entity(e)]);
    }

    #[test]
    fn cordoned_out_objects_are_not_picked() {
        let mut doc = Document::new_empty();
        let inside = doc
            .create_solid(crate::tests::box_sides(DVec3::ZERO, DVec3::splat(64.0), "A"), crate::Owner::World)
            .expect("solid");
        let outside = doc
            .create_solid(
                crate::tests::box_sides(DVec3::new(2000.0, 0.0, 0.0), DVec3::new(2064.0, 64.0, 64.0), "A"),
                crate::Owner::World,
            )
            .expect("solid");
        let far = doc.create_point_entity("info_target", DVec3::new(2032.0, 200.0, 32.0), &[]).expect("entity");
        let near = doc.create_point_entity("info_target", DVec3::new(32.0, 200.0, 32.0), &[]).expect("entity");
        let none = |_: &Entity| None;
        let opts = PickOptions::default();
        let down = |x: f64, y: f64| Ray::new(DVec3::new(x, y, 500.0), -DVec3::Z);
        let hit = |doc: &Document, x: f64, y: f64| doc.pick_ray(&down(x, y), &opts, &none).map(|h| h.obj);
        let all_2d = |doc: &Document| {
            doc.objects_in_rect_2d(ViewAxis::Top, DVec2::splat(-100.0), DVec2::splat(3000.0), RectMode::Touching, &none)
        };
        assert_eq!(hit(&doc, 2032.0, 32.0), Some(ObjRef::Solid(outside)));
        assert_eq!(all_2d(&doc).len(), 4);

        doc.add_cordon("room", Aabb::new(DVec3::splat(-128.0), DVec3::new(128.0, 256.0, 128.0))).expect("cordon");
        doc.set_cordons_enabled(true).expect("toggle");
        assert!(doc.cordons_in_use());
        assert_eq!(hit(&doc, 32.0, 32.0), Some(ObjRef::Solid(inside)));
        assert_eq!(hit(&doc, 2032.0, 32.0), None, "the brush outside the cordon");
        assert_eq!(hit(&doc, 2032.0, 200.0), None, "the entity outside the cordon");
        assert_eq!(hit(&doc, 32.0, 200.0), Some(ObjRef::Entity(near)));
        assert!(doc.pick_2d(ViewAxis::Top, DVec2::new(2032.0, 32.0), 4.0, &none).is_empty());
        assert!(doc.pick_2d(ViewAxis::Top, DVec2::new(2032.0, 200.0), 4.0, &none).is_empty());
        assert_eq!(doc.pick_2d(ViewAxis::Top, DVec2::new(32.0, 200.0), 4.0, &none), vec![ObjRef::Entity(near)]);
        let picked = all_2d(&doc);
        assert_eq!(picked, vec![ObjRef::Solid(inside), ObjRef::Entity(near)]);
        assert!(!picked.contains(&ObjRef::Entity(far)));
        doc.set_cordons_enabled(false).expect("toggle");
        assert_eq!(hit(&doc, 2032.0, 32.0), Some(ObjRef::Solid(outside)));
        assert_eq!(all_2d(&doc).len(), 4);
    }

    #[test]
    fn hull_and_sat() {
        let pts = vec![DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0), DVec2::new(0.0, 10.0), DVec2::new(2.0, 2.0)];
        let h = hull_2d(pts);
        assert_eq!(h.len(), 3);
        assert!(!convex_overlaps_rect(&h, DVec2::new(8.0, 8.0), DVec2::new(9.0, 9.0)));
        assert!(convex_overlaps_rect(&h, DVec2::new(1.0, 1.0), DVec2::new(2.0, 2.0)));
        assert!(convex_overlaps_rect(&h, DVec2::new(-5.0, -5.0), DVec2::new(20.0, 20.0)));
        let b = Aabb::new(DVec3::ZERO, DVec3::ONE);
        assert_eq!(box_normal(&b, DVec3::new(0.5, 0.5, 1.0)), DVec3::Z);
        let cb = column_box(ViewAxis::Front, DVec2::new(1.0, 2.0), DVec2::new(3.0, 4.0));
        assert_eq!(cb.min.y, 1.0);
        assert_eq!(cb.max.z, 4.0);
        assert!(cb.min.x.is_infinite());
    }
}
