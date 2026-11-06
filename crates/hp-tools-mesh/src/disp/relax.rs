use std::collections::{HashMap, hash_map::Entry};

use glam::DVec3;
use hp_editor::{Document, EditResult, FaceRef};
use hp_geom::{Aabb, DispMesh};

use super::paint::{BrushSettings, DEFAULT_DT, MAX_DT, SMOOTH_RATE};
use super::sculpt::{commit_updates, set_position};

#[derive(Clone, Debug)]
struct Node {
    bounds: Aabb,
    start: usize,
    len: usize,
    children: Option<(usize, usize)>,
}

#[derive(Clone, Debug)]
struct Reference {
    triangles: Vec<[DVec3; 3]>,
    nodes: Vec<Node>,
    pinned: Vec<bool>,
    orientation: Vec<DVec3>,
}

fn closest_segment(p: DVec3, a: DVec3, b: DVec3) -> DVec3 {
    let edge = b - a;
    a + edge * ((p - a).dot(edge) / edge.length_squared().max(1e-24)).clamp(0.0, 1.0)
}

fn closest_triangle(p: DVec3, [a, b, c]: [DVec3; 3]) -> DVec3 {
    let (ab, ac) = (b - a, c - a);
    let normal = ab.cross(ac);
    let square = normal.length_squared();
    if square > 1e-24 {
        let projected = p - normal * (p - a).dot(normal) / square;
        let side = |u: DVec3, v: DVec3| (v - u).cross(projected - u).dot(normal);
        if side(a, b) >= -1e-12 && side(b, c) >= -1e-12 && side(c, a) >= -1e-12 {
            return projected;
        }
    }
    [closest_segment(p, a, b), closest_segment(p, b, c), closest_segment(p, c, a)]
        .into_iter()
        .min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p)))
        .unwrap_or(a)
}

fn node_distance(node: &Node, p: DVec3) -> f64 {
    p.distance_squared(p.clamp(node.bounds.min, node.bounds.max))
}

impl Reference {
    fn new(mesh: &DispMesh) -> Option<Self> {
        let n = mesh.row_len.checked_mul(mesh.row_len)?;
        if mesh.row_len < 3 || mesh.positions.len() != n || mesh.positions.iter().any(|p| !p.is_finite()) {
            return None;
        }
        let mut pinned: Vec<bool> = (0..n)
            .map(|k| {
                k % mesh.row_len == 0
                    || k % mesh.row_len + 1 == mesh.row_len
                    || k / mesh.row_len == 0
                    || k / mesh.row_len + 1 == mesh.row_len
            })
            .collect();
        let mut triangles = Vec::new();
        let mut orientation = Vec::new();
        let mut normals = vec![DVec3::ZERO; n];
        let mut adjacent = vec![Vec::new(); n];
        let mut edges = HashMap::new();
        for tri in mesh.indices.chunks_exact(3) {
            let ids = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let points = [*mesh.positions.get(ids[0])?, *mesh.positions.get(ids[1])?, *mesh.positions.get(ids[2])?];
            let area = (points[1] - points[0]).cross(points[2] - points[0]);
            let normal = area.try_normalize()?;
            orientation.push(area);
            for id in ids {
                normals[id] += area;
                adjacent[id].push(normal);
            }
            for (a, b) in [(ids[0], ids[1]), (ids[1], ids[2]), (ids[2], ids[0])] {
                *edges.entry((a.min(b), a.max(b))).or_insert(0usize) += 1;
            }
            triangles.push(points);
        }
        if triangles.is_empty() {
            return None;
        }
        for ((a, b), count) in edges {
            if count != 2 {
                pinned[a] = true;
                pinned[b] = true;
            }
        }
        for k in 0..n {
            let normal = normals[k].normalize_or_zero();
            pinned[k] |= adjacent[k].is_empty() || adjacent[k].iter().any(|n| n.dot(normal) < 0.8660254037844386);
        }
        let mut reference = Self { triangles, nodes: Vec::new(), pinned, orientation };
        reference.build(0, reference.triangles.len());
        Some(reference)
    }

    fn build(&mut self, start: usize, len: usize) -> usize {
        let (min, max) = self.triangles[start..start + len]
            .iter()
            .flatten()
            .fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(a, b), p| (a.min(*p), b.max(*p)));
        let index = self.nodes.len();
        self.nodes.push(Node { bounds: Aabb { min, max }, start, len, children: None });
        if len > 8 {
            let extent = max - min;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            self.triangles[start..start + len].sort_unstable_by(|a, b| {
                (a[0][axis] + a[1][axis] + a[2][axis]).total_cmp(&(b[0][axis] + b[1][axis] + b[2][axis]))
            });
            let mid = len / 2;
            let a = self.build(start, mid);
            let b = self.build(start + mid, len - mid);
            self.nodes[index].children = Some((a, b));
        }
        index
    }

    fn project(&self, p: DVec3, stack: &mut Vec<usize>) -> DVec3 {
        let (mut closest, mut distance) = (p, f64::INFINITY);
        stack.clear();
        stack.push(0);
        while let Some(index) = stack.pop() {
            let node = &self.nodes[index];
            if node_distance(node, p) > distance {
                continue;
            }
            if let Some((a, b)) = node.children {
                if node_distance(&self.nodes[a], p) < node_distance(&self.nodes[b], p) {
                    stack.push(b);
                    stack.push(a);
                } else {
                    stack.push(a);
                    stack.push(b);
                }
            } else {
                for triangle in &self.triangles[node.start..node.start + node.len] {
                    let point = closest_triangle(p, *triangle);
                    let d = point.distance_squared(p);
                    if d < distance {
                        closest = point;
                        distance = d;
                    }
                }
            }
        }
        closest
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct RelaxStroke {
    reference: HashMap<FaceRef, Reference>,
}

fn positions(
    mesh: &DispMesh,
    reference: &Reference,
    brush: &BrushSettings,
    center: DVec3,
    amount: f64,
) -> Option<Vec<DVec3>> {
    let old = &mesh.positions;
    let row = mesh.row_len;
    let count = row.checked_mul(row)?;
    if old.len() != count || reference.pinned.len() != count || reference.orientation.len() != mesh.indices.len() / 3 {
        return None;
    }
    let mut normals = vec![DVec3::ZERO; count];
    for tri in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let normal = (*old.get(b)? - *old.get(a)?).cross(*old.get(c)? - old[a]);
        for k in [a, b, c] {
            normals[k] += normal;
        }
    }
    let mut proposed = old.clone();
    let mut stack = Vec::with_capacity(32);
    for k in 0..count {
        if reference.pinned[k] {
            continue;
        }
        let delta = old[k] - center;
        let distance = if brush.spatial {
            delta.length()
        } else {
            (delta - mesh.face_normal * delta.dot(mesh.face_normal)).length()
        };
        let w = brush.falloff.weight(distance / brush.radius);
        if w <= 0.0 {
            continue;
        }
        let neighbours = [k - 1, k + 1, k - row, k + row];
        let average = neighbours.iter().map(|i| old[*i]).sum::<DVec3>() * 0.25;
        let normal = normals[k].try_normalize()?;
        let delta = average - old[k];
        let tangent = delta - normal * delta.dot(normal);
        let cap = neighbours.iter().map(|i| old[*i].distance(old[k])).fold(f64::INFINITY, f64::min) * 0.25;
        let motion = tangent * amount * w;
        let motion = motion * (cap / motion.length().max(cap).max(1e-12));
        proposed[k] = reference.project(old[k] + motion, &mut stack);
    }
    for attempt in 0..10 {
        let fraction = 0.5f64.powi(attempt);
        let next: Vec<_> = old
            .iter()
            .zip(&proposed)
            .map(|(a, b)| if a == b { *a } else { reference.project(a.lerp(*b, fraction), &mut stack) })
            .collect();
        let valid = next.iter().all(|p| p.is_finite())
            && mesh.indices.chunks_exact(3).enumerate().all(|(index, tri)| {
                let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                let before = (old[b] - old[a]).cross(old[c] - old[a]);
                let after = (next[b] - next[a]).cross(next[c] - next[a]);
                let initial = reference.orientation[index];
                after.dot(before) > before.length_squared() * 0.1
                    && after.dot(initial) > initial.length_squared() * 0.01
                    && after.length_squared() > 1e-18
            });
        if valid {
            return Some(next);
        }
    }
    None
}

pub(super) fn sample(
    doc: &mut Document,
    faces: &[FaceRef],
    center: DVec3,
    brush: &BrushSettings,
    stroke: &mut RelaxStroke,
    dt: f64,
) -> EditResult<usize> {
    let dt = if dt.is_finite() && dt > 0.0 { dt.min(f64::from(MAX_DT)) } else { f64::from(DEFAULT_DT) };
    let amount = 1.0 - (-brush.sculpt.relax_strength * SMOOTH_RATE * dt).exp();
    if amount <= 0.0 {
        return Ok(0);
    }
    let mut updates: Vec<(hp_editor::Id, Vec<(usize, hp_vmf::DispInfo)>)> = Vec::new();
    let mut changed = 0;
    for face in faces {
        let Some(node) = doc.scene().solid(face.solid) else { continue };
        let Some(mesh) = node.disp_mesh(face.side) else { continue };
        if let Entry::Vacant(entry) = stroke.reference.entry(*face) {
            let Some(reference) = Reference::new(mesh) else { continue };
            entry.insert(reference);
        }
        let Some(reference) = stroke.reference.get(face) else { continue };
        let Some(points) = positions(mesh, reference, brush, center, amount) else { continue };
        let Some(original) = node.solid.sides.get(face.side).and_then(|s| s.disp.as_deref()) else { continue };
        let mut displacement = original.clone();
        let mut count = 0;
        for (k, p) in points.into_iter().enumerate() {
            if set_position(&mut displacement, mesh, k, p) {
                count += 1;
            }
        }
        if count == 0 {
            continue;
        }
        if let Some(poly) = node.geom.face_for_side(face.side) {
            hp_geom::update_disp_tags(&mut displacement, &poly.vertices);
        }
        changed += count;
        match updates.iter_mut().find(|(id, _)| *id == face.solid) {
            Some((_, faces)) => faces.push((face.side, displacement)),
            None => updates.push((face.solid, vec![(face.side, displacement)])),
        }
    }
    commit_updates(doc, updates, changed)
}
