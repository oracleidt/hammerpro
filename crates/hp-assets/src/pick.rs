use std::sync::{Arc, Weak};

use ahash::AHashMap;
use glam::{DVec3, Vec3};
use hp_editor::EntityRay;
use hp_geom::{Aabb, Ray};
use parking_lot::Mutex;

use crate::AssetManager;
use crate::material::guess_tool;
use crate::models::{PickGeometry, PreparedModel, SIDE_BACK, SIDE_BOTH, entity_matrix, model_data_key};
use crate::registry::AnaEntry;

const LEAF: usize = 4;
const MAX_TRIANGLES: usize = 1 << 20;
const MAX_CACHED: usize = 256;

#[derive(Clone, Copy, Debug)]
struct Node {
    min: Vec3,
    max: Vec3,
    first: u32,
    count: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Tri {
    pub p: [Vec3; 3],
    pub n: Vec3,
}

#[derive(Debug, Default)]
pub(crate) struct TriBvh {
    nodes: Vec<Node>,
    tris: Vec<Tri>,
}

fn ray_box(o: Vec3, inv: Vec3, min: Vec3, max: Vec3, limit: f32) -> Option<f32> {
    let t0 = (min - o) * inv;
    let t1 = (max - o) * inv;
    let near = t0.min(t1).max_element().max(0.0);
    let far = t0.max(t1).min_element().min(limit);
    (near <= far).then_some(near)
}

fn ray_tri(o: Vec3, d: Vec3, t: &[Vec3; 3]) -> Option<f32> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t[0];
    let u = s.dot(p) * inv;
    if !(-1e-5..=1.0 + 1e-5).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < -1e-5 || u + v > 1.0 + 1e-5 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    (dist >= 0.0 && dist.is_finite()).then_some(dist)
}

impl TriBvh {
    pub(crate) fn build(mut tris: Vec<Tri>) -> Self {
        tris.retain(|t| {
            let p = &t.p;
            p.iter().all(|v| v.is_finite())
                && t.n.is_finite()
                && (p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-12
        });
        tris.truncate(MAX_TRIANGLES);
        if tris.is_empty() {
            return Self::default();
        }
        let centroid = |t: &Tri| (t.p[0] + t.p[1] + t.p[2]) / 3.0;
        let mut nodes = Vec::with_capacity(2 * tris.len() / LEAF + 2);
        nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
        let mut stack = vec![(0usize, 0usize, tris.len())];
        while let Some((node, start, end)) = stack.pop() {
            let Some(slice) = tris.get_mut(start..end) else { continue };
            let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let (mut cmin, mut cmax) = (min, max);
            for t in slice.iter() {
                for p in &t.p {
                    min = min.min(*p);
                    max = max.max(*p);
                }
                let c = centroid(t);
                cmin = cmin.min(c);
                cmax = cmax.max(c);
            }
            let n = end - start;
            let extent = cmax - cmin;
            if n <= LEAF || extent.max_element() <= 0.0 {
                if let Some(nd) = nodes.get_mut(node) {
                    *nd = Node { min, max, first: start as u32, count: n as u32 };
                }
                continue;
            }
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let mid = n / 2;
            slice.select_nth_unstable_by(mid, |a, b| centroid(a)[axis].total_cmp(&centroid(b)[axis]));
            let left = nodes.len();
            nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
            nodes.push(Node { min: Vec3::ZERO, max: Vec3::ZERO, first: 0, count: 0 });
            if let Some(nd) = nodes.get_mut(node) {
                *nd = Node { min, max, first: left as u32, count: 0 };
            }
            stack.push((left, start, start + mid));
            stack.push((left + 1, start + mid, end));
        }
        Self { nodes, tris }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    pub(crate) fn ray(&self, origin: DVec3, dir: DVec3) -> Option<(f64, DVec3)> {
        if self.nodes.is_empty() || !origin.is_finite() || !dir.is_finite() || dir.length_squared() < 1e-24 {
            return None;
        }
        let (o, d) = (origin.as_vec3(), dir.as_vec3());
        let inv = Vec3::ONE / d;
        let mut best: Option<(f32, usize)> = None;
        let mut stack: Vec<usize> = vec![0];
        while let Some(i) = stack.pop() {
            let Some(n) = self.nodes.get(i) else { continue };
            let limit = best.map_or(f32::INFINITY, |(t, _)| t);
            if ray_box(o, inv, n.min, n.max, limit).is_none() {
                continue;
            }
            if n.count > 0 {
                let start = n.first as usize;
                for k in start..start + n.count as usize {
                    let Some(tri) = self.tris.get(k) else { continue };
                    if (tri.n == Vec3::ZERO || tri.n.dot(d) < 0.0)
                        && let Some(t) = ray_tri(o, d, &tri.p)
                        && best.is_none_or(|(b, _)| t < b)
                    {
                        best = Some((t, k));
                    }
                }
            } else {
                stack.push(n.first as usize);
                stack.push(n.first as usize + 1);
            }
        }
        let (t, k) = best?;
        let tri = self.tris.get(k)?;
        let n = tri.n.as_dvec3().try_normalize().unwrap_or(DVec3::Z);
        Some((f64::from(t), n))
    }
}

type MeshKey = (String, u32);
type CachedMesh = (Weak<PreparedModel>, Arc<TriBvh>);
type CachedCompact = (Weak<PickGeometry>, Arc<TriBvh>);

#[derive(Default)]
pub(crate) struct PickMeshes {
    map: Mutex<AHashMap<MeshKey, CachedMesh>>,
    compact: Mutex<AHashMap<MeshKey, CachedCompact>>,
}

pub(crate) fn compact_triangles(geo: &PickGeometry, body: Option<u32>) -> Vec<Tri> {
    let mut tris = Vec::new();
    for part in geo.parts_for(body) {
        for (t, side) in part.indices.chunks_exact(3).zip(&part.sides) {
            let v = |i: u32| part.positions.get(i as usize).map(|p| Vec3::from_array(*p));
            let (Some(a), Some(b), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) else { continue };
            let face = (b - a).cross(c - a);
            let n = match *side {
                SIDE_BOTH => Vec3::ZERO,
                SIDE_BACK => -face,
                _ => face,
            };
            tris.push(Tri { p: [a, b, c], n });
        }
    }
    tris
}

fn oriented(p: [Vec3; 3], shading: Vec3) -> Tri {
    let face = (p[1] - p[0]).cross(p[2] - p[0]);
    let n = if shading.length_squared() > 1e-12 && shading.dot(face) < 0.0 { -face } else { face };
    Tri { p, n: if shading.length_squared() > 1e-12 { n } else { Vec3::ZERO } }
}

fn model_triangles(data: &PreparedModel, body: Option<u32>) -> Vec<Tri> {
    let parts = match body {
        None => data.parts.iter().collect(),
        Some(b) => data.parts_for_body(b),
    };
    parts_triangles(parts.into_iter().map(|part| (part.vertices.as_slice(), part.indices.as_slice())))
}

pub(crate) fn parts_triangles<'a>(parts: impl Iterator<Item = (&'a [hp_render::Vertex], &'a [u32])>) -> Vec<Tri> {
    let mut tris = Vec::new();
    for (vertices, indices) in parts {
        for t in indices.chunks_exact(3) {
            let v = |i: u32| vertices.get(i as usize);
            if let (Some(a), Some(b), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) {
                let shading = Vec3::from_array(a.normal) + Vec3::from_array(b.normal) + Vec3::from_array(c.normal);
                let pos = [Vec3::from_array(a.pos), Vec3::from_array(b.pos), Vec3::from_array(c.pos)];
                tris.push(oriented(pos, shading));
            }
        }
    }
    tris
}

impl PickMeshes {
    fn get(&self, key: MeshKey, data: &Arc<PreparedModel>, body: Option<u32>) -> Arc<TriBvh> {
        let mut map = self.map.lock();
        if let Some((made_from, bvh)) = map.get(&key)
            && made_from.ptr_eq(&Arc::downgrade(data))
        {
            return Arc::clone(bvh);
        }
        if map.len() >= MAX_CACHED {
            map.retain(|_, (w, _)| w.strong_count() > 0);
            if map.len() >= MAX_CACHED {
                map.clear();
            }
        }
        let bvh = Arc::new(TriBvh::build(model_triangles(data, body)));
        map.insert(key, (Arc::downgrade(data), Arc::clone(&bvh)));
        bvh
    }

    fn get_compact(&self, key: MeshKey, geo: &Arc<PickGeometry>, body: Option<u32>) -> Arc<TriBvh> {
        let mut map = self.compact.lock();
        if let Some((made_from, bvh)) = map.get(&key)
            && made_from.ptr_eq(&Arc::downgrade(geo))
        {
            return Arc::clone(bvh);
        }
        if map.len() >= MAX_CACHED {
            map.retain(|_, (w, _)| w.strong_count() > 0);
            if map.len() >= MAX_CACHED {
                map.clear();
            }
        }
        let bvh = Arc::new(TriBvh::build(compact_triangles(geo, body)));
        map.insert(key, (Arc::downgrade(geo), Arc::clone(&bvh)));
        bvh
    }
}

fn box_ray(b: &Aabb, ray: &Ray) -> EntityRay {
    if b.is_empty() || !b.min.is_finite() || !b.max.is_finite() || b.contains_point(ray.origin) {
        return EntityRay::Miss;
    }
    match b.ray_intersect(ray) {
        Some(t) => {
            let p = ray.at(t);
            let candidates = [
                ((p.x - b.min.x).abs(), DVec3::NEG_X),
                ((p.x - b.max.x).abs(), DVec3::X),
                ((p.y - b.min.y).abs(), DVec3::NEG_Y),
                ((p.y - b.max.y).abs(), DVec3::Y),
                ((p.z - b.min.z).abs(), DVec3::NEG_Z),
                ((p.z - b.max.z).abs(), DVec3::Z),
            ];
            let normal = candidates.iter().min_by(|a, b| a.0.total_cmp(&b.0)).map_or(DVec3::Z, |c| c.1);
            EntityRay::Hit { t, normal }
        }
        None => EntityRay::Miss,
    }
}

impl AssetManager {
    pub(crate) fn entity_ray_impl(&self, e: &hp_vmf::Entity, ray: &Ray) -> EntityRay {
        if let Some(hit) = self.ragdoll_ray(e.id, ray) {
            return hit;
        }
        let Some(s) = self.scene.ents.get(&e.id) else { return EntityRay::Bounds };
        let d = &s.desc;
        let Some((path, _)) = d.model.as_ref().filter(|_| s.model.is_some()) else {
            let b = Aabb::new(d.origin + d.bounds.0, d.origin + d.bounds.1);
            return box_ray(&b, ray);
        };
        let plain = d.body == 0 && d.anim.is_none();
        let key = if plain { path.clone() } else { model_data_key(path, d.anim.as_deref()) };
        let bvh = match (self.models.map.get(&key), self.models.pick.get(&key)) {
            (Some(crate::models::ModelEntry::Ready(data)), _) => {
                let body = (!plain).then(|| data.body_key(d.body));
                self.pick_meshes.get((key, body.unwrap_or(0)), data, body)
            }
            (_, Some(geo)) => {
                let body = (!plain).then(|| crate::models::body_key(&geo.body_parts, d.body));
                self.pick_meshes.get_compact((key, body.unwrap_or(0)), geo, body)
            }
            _ => return EntityRay::Bounds,
        };
        if bvh.is_empty() {
            return EntityRay::Bounds;
        }
        let m = entity_matrix(d.origin, d.angles, d.scale);
        let inv = m.inverse();
        if !inv.is_finite() {
            return EntityRay::Bounds;
        }
        let (lo, ld) = (inv.transform_point3(ray.origin), inv.transform_vector3(ray.dir));
        match bvh.ray(lo, ld) {
            Some((t, n)) => {
                let normal = inv.transpose().transform_vector3(n).try_normalize().unwrap_or(DVec3::Z);
                EntityRay::Hit { t, normal }
            }
            None => EntityRay::Miss,
        }
    }

    pub(crate) fn material_tool_impl(&self, name: &str) -> Option<hp_vmt::ToolKind> {
        let key: std::borrow::Cow<'_, str> = if crate::util::is_normalized_material(name) {
            std::borrow::Cow::Borrowed(name)
        } else {
            std::borrow::Cow::Owned(crate::util::material_key(name))
        };
        match self.mats.analyses.get(key.as_ref()).and_then(|e: &AnaEntry| e.value.as_ref()) {
            Some(Some(a)) => a.tool,
            _ => guess_tool(&key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(half: f32) -> Vec<Tri> {
        let c = |x: f32, y: f32, z: f32| Vec3::new(x, y, z) * half;
        let corners = [
            c(-1.0, -1.0, -1.0),
            c(1.0, -1.0, -1.0),
            c(-1.0, 1.0, -1.0),
            c(1.0, 1.0, -1.0),
            c(-1.0, -1.0, 1.0),
            c(1.0, -1.0, 1.0),
            c(-1.0, 1.0, 1.0),
            c(1.0, 1.0, 1.0),
        ];
        let faces = [[0, 2, 6, 4], [1, 5, 7, 3], [0, 4, 5, 1], [2, 3, 7, 6], [0, 1, 3, 2], [4, 6, 7, 5]];
        let mut tris = Vec::new();
        for f in faces {
            for p in [[corners[f[0]], corners[f[1]], corners[f[2]]], [corners[f[0]], corners[f[2]], corners[f[3]]]] {
                tris.push(oriented(p, p[0] + p[1] + p[2]));
            }
        }
        tris
    }

    #[test]
    fn triangle_rays_find_the_nearest_surface() {
        let b = TriBvh::build(cube(8.0));
        let (t, n) = b.ray(DVec3::new(1.0, 2.0, 100.0), DVec3::NEG_Z).expect("hit");
        assert!((t - 92.0).abs() < 1e-4, "{t}");
        assert!(n.z.abs() > 0.99);
        assert!(b.ray(DVec3::new(20.0, 0.0, 100.0), DVec3::NEG_Z).is_none());
        assert!(b.ray(DVec3::ZERO, DVec3::X).is_none(), "back faces are culled like in the view");
        let flat = Tri { p: [Vec3::ZERO, Vec3::X, Vec3::Y], n: Vec3::ZERO };
        let open = TriBvh::build(vec![flat]);
        assert!(open.ray(DVec3::new(0.2, 0.2, -5.0), DVec3::Z).is_some(), "no shading normal: both sides");
        assert!(b.ray(DVec3::NAN, DVec3::Z).is_none());
        assert!(TriBvh::build(vec![Tri { p: [Vec3::ZERO; 3], n: Vec3::Z }]).is_empty());
    }

    #[test]
    fn compact_pick_geometry_hits_exactly_like_the_full_model() {
        use crate::models::{PreparedModel, PreparedPart};
        let v =
            |x: f32, y: f32, z: f32, n: [f32; 3]| hp_render::Vertex { pos: [x, y, z], normal: n, ..Default::default() };
        let up = [0.0, 0.0, 1.0];
        let quad = |z: f32, choice: usize, normal: [f32; 3]| PreparedPart {
            skin_ref: 0,
            body_part: 0,
            choice,
            vertices: vec![
                v(-8.0, -8.0, z, normal),
                v(8.0, -8.0, z, normal),
                v(8.0, 8.0, z, normal),
                v(-8.0, 8.0, z, normal),
            ],
            indices: vec![0, 1, 2, 0, 2, 3, 0, 2, 1],
            tangents: Vec::new(),
            color_index: Vec::new(),
        };
        let model = PreparedModel {
            parts: vec![quad(0.0, 0, up)],
            variant_parts: vec![quad(16.0, 1, [0.0; 3])],
            body_parts: vec![(1, 2)],
            ..Default::default()
        };
        let geo = PickGeometry::of(&model);
        assert!(geo.bytes() < model.geometry_bytes() / 2, "{} vs {}", geo.bytes(), model.geometry_bytes());
        for body in [None, Some(0), Some(1)] {
            let full = TriBvh::build(model_triangles(&model, body));
            let compact = TriBvh::build(compact_triangles(&geo, body));
            for (o, d) in [
                (DVec3::new(1.0, 2.0, 50.0), DVec3::NEG_Z),
                (DVec3::new(1.0, 2.0, -50.0), DVec3::Z),
                (DVec3::new(3.0, -1.0, 8.0), DVec3::Z),
                (DVec3::new(30.0, 0.0, 50.0), DVec3::NEG_Z),
            ] {
                assert_eq!(full.ray(o, d), compact.ray(o, d), "body {body:?} from {o:?}");
            }
        }
    }

    #[test]
    fn helper_boxes_are_hit_from_outside_only() {
        let b = Aabb::new(DVec3::splat(-8.0), DVec3::splat(8.0));
        let hit = box_ray(&b, &Ray::new(DVec3::new(-100.0, 0.0, 0.0), DVec3::X));
        assert!(matches!(hit, EntityRay::Hit { t, normal } if (t - 92.0).abs() < 1e-9 && normal == DVec3::NEG_X));
        assert_eq!(box_ray(&b, &Ray::new(DVec3::ZERO, DVec3::X)), EntityRay::Miss);
        assert_eq!(box_ray(&b, &Ray::new(DVec3::new(-100.0, 50.0, 0.0), DVec3::X)), EntityRay::Miss);
    }
}
