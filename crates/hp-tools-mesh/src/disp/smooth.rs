use std::collections::HashMap;

use glam::DVec3;
use hp_geom::DispMesh;
use hp_vmf::DispInfo;

use super::paint::{LocalBrush, PaintDir};
use super::seams::{SEAM_TOL, base_corners, border_indices, edge_overlap};

pub(crate) struct SmoothPart<'a> {
    pub mesh: &'a DispMesh,
    pub disp: &'a mut DispInfo,
    pub pinned: bool,
}

#[derive(Clone, Debug, Default)]
struct Node {
    members: Vec<(usize, usize)>,
    pos: DVec3,
    axis: DVec3,
    weight: f64,
}

#[derive(Clone, Copy, Debug)]
struct Junction {
    node: usize,
    a: usize,
    b: usize,
    s: f64,
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut root = i;
    while let Some(&p) = parent.get(root) {
        if p == root {
            break;
        }
        root = p;
    }
    let mut cur = i;
    while let Some(&p) = parent.get(cur) {
        if p == root {
            break;
        }
        if let Some(slot) = parent.get_mut(cur) {
            *slot = root;
        }
        cur = p;
    }
    root
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb
        && let Some(slot) = parent.get_mut(ra.max(rb))
    {
        *slot = ra.min(rb);
    }
}

fn weight(brush: &LocalBrush, mesh: &DispMesh, k: usize) -> f64 {
    let (Some(&pos), Some(&base)) = (mesh.positions.get(k), mesh.base_positions.get(k)) else { return 0.0 };
    let n = mesh.face_normal;
    let dist = if brush.spatial {
        pos.distance(brush.center)
    } else {
        let d = base - brush.center;
        (d - n * d.dot(n)).length()
    };
    brush.falloff.weight(dist / brush.radius)
}

struct Surface {
    nodes: Vec<Node>,
    node_of: Vec<usize>,
    first: Vec<usize>,
    valid: Vec<bool>,
}

impl Surface {
    fn gid(&self, part: usize, k: usize) -> usize {
        self.first.get(part).copied().unwrap_or(0) + k
    }

    fn node(&self, part: usize, k: usize) -> Option<usize> {
        self.node_of.get(self.gid(part, k)).copied().filter(|n| *n != usize::MAX)
    }

    fn build(
        parts: &[SmoothPart<'_>],
        axis_of: impl Fn(&DispMesh, usize) -> DVec3,
        weight_of: impl Fn(&DispMesh, usize) -> f64,
        fixed_axis: Option<DVec3>,
    ) -> Self {
        let valid: Vec<bool> = parts
            .iter()
            .map(|p| {
                let n = p.mesh.row_len;
                n >= 2
                    && p.mesh.positions.len() == n * n
                    && p.mesh.base_positions.len() == n * n
                    && p.mesh.normals.len() == n * n
            })
            .collect();
        let mut first = Vec::with_capacity(parts.len());
        let mut total = 0usize;
        for (p, ok) in parts.iter().zip(&valid) {
            first.push(total);
            if *ok {
                total += p.mesh.row_len * p.mesh.row_len;
            }
        }
        let gid = |pi: usize, k: usize| first.get(pi).copied().unwrap_or(0) + k;

        let mut parent: Vec<usize> = (0..total).collect();
        let cell = |p: DVec3| -> [i64; 3] {
            let c = (p / (SEAM_TOL * 4.0)).floor();
            [c.x as i64, c.y as i64, c.z as i64]
        };
        let mut grid: HashMap<[i64; 3], Vec<(usize, usize)>> = HashMap::new();
        let borders: Vec<Vec<usize>> = parts
            .iter()
            .zip(&valid)
            .map(|(p, ok)| {
                if !*ok {
                    return Vec::new();
                }
                let mut b: Vec<usize> = border_indices(p.mesh.row_len).into_iter().flatten().collect();
                b.sort_unstable();
                b.dedup();
                b
            })
            .collect();
        for (pi, (part, border)) in parts.iter().zip(&borders).enumerate() {
            for &k in border {
                if let Some(b) = part.mesh.base_positions.get(k) {
                    grid.entry(cell(*b)).or_default().push((pi, k));
                }
            }
        }
        for (pi, (part, border)) in parts.iter().zip(&borders).enumerate() {
            for &k in border {
                let (Some(&base), Some(&pos)) = (part.mesh.base_positions.get(k), part.mesh.positions.get(k)) else {
                    continue;
                };
                let c = cell(base);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            let Some(list) = grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else { continue };
                            for &(pj, kj) in list {
                                if pj <= pi {
                                    continue;
                                }
                                let Some(other) = parts.get(pj).map(|p| p.mesh) else { continue };
                                let same_base =
                                    other.base_positions.get(kj).is_some_and(|b| b.distance(base) <= SEAM_TOL);
                                let same_pos = other.positions.get(kj).is_some_and(|q| q.distance(pos) <= SEAM_TOL);
                                if same_base && same_pos {
                                    union(&mut parent, gid(pi, k), gid(pj, kj));
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut node_of = vec![usize::MAX; total];
        let mut nodes: Vec<Node> = Vec::new();
        let mut root_node: HashMap<usize, usize> = HashMap::new();
        for (pi, (part, ok)) in parts.iter().zip(&valid).enumerate() {
            if !*ok {
                continue;
            }
            let n = part.mesh.row_len;
            for k in 0..n * n {
                let g = gid(pi, k);
                let root = find(&mut parent, g);
                let idx = *root_node.entry(root).or_insert_with(|| {
                    nodes.push(Node::default());
                    nodes.len() - 1
                });
                if let Some(slot) = node_of.get_mut(g) {
                    *slot = idx;
                }
                if let Some(node) = nodes.get_mut(idx) {
                    node.members.push((pi, k));
                }
            }
        }
        for node in &mut nodes {
            let mut pos = DVec3::ZERO;
            let mut dirs = DVec3::ZERO;
            let mut w = 0.0;
            for &(pi, k) in &node.members {
                let Some(mesh) = parts.get(pi).map(|p| p.mesh) else { continue };
                pos += mesh.positions.get(k).copied().unwrap_or(DVec3::ZERO);
                dirs += axis_of(mesh, k);
                w += weight_of(mesh, k);
            }
            let count = node.members.len().max(1) as f64;
            node.pos = pos / count;
            node.weight = w / count;
            if node.members.iter().any(|(pi, _)| parts.get(*pi).is_some_and(|p| p.pinned)) {
                node.weight = 0.0;
            }
            node.axis = fixed_axis.or_else(|| dirs.try_normalize()).unwrap_or(DVec3::Z);
        }
        Self { nodes, node_of, first, valid }
    }

    fn neighbours(&self, parts: &[SmoothPart<'_>], ni: usize, diagonal: bool) -> Vec<usize> {
        let Some(node) = self.nodes.get(ni) else { return Vec::new() };
        let mut out: Vec<usize> = Vec::with_capacity(4 * node.members.len());
        let steps: [(isize, isize); 4] =
            if diagonal { [(-1, -1), (-1, 1), (1, -1), (1, 1)] } else { [(-1, 0), (1, 0), (0, -1), (0, 1)] };
        for &(pi, k) in &node.members {
            let Some(n) = parts.get(pi).map(|p| p.mesh.row_len) else { continue };
            let (i, j) = ((k / n) as isize, (k % n) as isize);
            for (di, dj) in steps {
                let (a, b) = (i + di, j + dj);
                if a < 0 || b < 0 || a >= n as isize || b >= n as isize {
                    continue;
                }
                if let Some(m) = self.node(pi, a as usize * n + b as usize)
                    && m != ni
                    && !out.contains(&m)
                {
                    out.push(m);
                }
            }
        }
        out
    }
}

pub(crate) fn smooth_parts(parts: &mut [SmoothPart<'_>], brush: &LocalBrush, dir: PaintDir, amount: f64) -> Vec<usize> {
    let mut changed = vec![0usize; parts.len()];
    let brush_ok = brush.radius.is_finite() && brush.radius > 0.0 && brush.center.is_finite();
    if !brush_ok || !amount.is_finite() || amount <= 0.0 {
        return changed;
    }
    let amount = amount.min(1.0);
    let fixed = match dir {
        PaintDir::Fixed(a) => Some(a.try_normalize().filter(|a| a.is_finite()).unwrap_or(DVec3::Z)),
        PaintDir::FaceNormal | PaintDir::VertexNormal => None,
    };
    let axis_of = |mesh: &DispMesh, k: usize| match dir {
        PaintDir::VertexNormal => mesh.normals.get(k).copied().unwrap_or(mesh.face_normal),
        _ => mesh.face_normal,
    };
    let surface = Surface::build(parts, axis_of, |mesh, k| weight(brush, mesh, k), fixed);

    let junctions = find_junctions(parts, &surface);

    let nodes = &surface.nodes;
    let mut new_pos: Vec<DVec3> = nodes.iter().map(|n| n.pos).collect();
    for (ni, node) in nodes.iter().enumerate() {
        if node.weight <= 0.0 {
            continue;
        }
        let neighbours = surface.neighbours(parts, ni, false);
        if neighbours.is_empty() {
            continue;
        }
        let target =
            neighbours.iter().filter_map(|m| nodes.get(*m)).map(|m| m.pos).sum::<DVec3>() / neighbours.len() as f64;
        let lift = (target - node.pos).dot(node.axis) * amount * node.weight;
        if let Some(slot) = new_pos.get_mut(ni) {
            *slot = node.pos + node.axis * lift;
        }
    }
    for j in &junctions {
        let pinned =
            nodes.get(j.node).is_some_and(|n| n.members.iter().any(|m| parts.get(m.0).is_some_and(|p| p.pinned)));
        if pinned {
            for i in [j.a, j.b] {
                if let (Some(slot), Some(n)) = (new_pos.get_mut(i), nodes.get(i)) {
                    *slot = n.pos;
                }
            }
        }
    }
    follow_junctions(&junctions, nodes, &mut new_pos);
    write_back(parts, &surface, &new_pos, &mut changed);
    changed
}

pub(crate) fn smooth_values(parts: &mut [SmoothPart<'_>], brush: &LocalBrush, amount: f64, blend: bool) -> Vec<usize> {
    use hp_vmf::multiblend::{CHANNELS, MultiBlend};
    let mut changed = vec![0usize; parts.len()];
    let brush_ok = brush.radius.is_finite() && brush.radius > 0.0 && brush.center.is_finite();
    if !brush_ok || !amount.is_finite() || amount <= 0.0 {
        return changed;
    }
    let amount = amount.min(1.0);
    let surface = Surface::build(parts, |m, _| m.face_normal, |mesh, k| weight(brush, mesh, k), Some(DVec3::Z));
    let mut values: Vec<Vec<[f64; CHANNELS]>> = Vec::with_capacity(parts.len());
    let mut blends: Vec<Option<MultiBlend>> = Vec::with_capacity(parts.len());
    for part in parts.iter() {
        let count = part.mesh.row_len * part.mesh.row_len;
        if blend {
            let mut b =
                MultiBlend::of(part.disp).filter(|b| b.len() == count).unwrap_or_else(|| MultiBlend::new(count));
            b.normalize();
            values.push(b.weights.clone());
            blends.push(Some(b));
        } else {
            let v = (0..count)
                .map(|k| {
                    let a = part.disp.alphas.get(k).copied().filter(|a| a.is_finite()).unwrap_or(0.0);
                    [a.clamp(0.0, 255.0), 0.0, 0.0, 0.0]
                })
                .collect();
            values.push(v);
            blends.push(None);
        }
    }
    let node_value = |ni: usize| -> [f64; CHANNELS] {
        let mut sum = [0.0; CHANNELS];
        let Some(node) = surface.nodes.get(ni) else { return sum };
        for &(pi, k) in &node.members {
            if let Some(v) = values.get(pi).and_then(|v| v.get(k)) {
                for (s, x) in sum.iter_mut().zip(v) {
                    *s += x;
                }
            }
        }
        let n = node.members.len().max(1) as f64;
        sum.map(|s| s / n)
    };
    let current: Vec<[f64; CHANNELS]> = (0..surface.nodes.len()).map(node_value).collect();
    for (ni, node) in surface.nodes.iter().enumerate() {
        if node.weight <= 0.0 {
            continue;
        }
        let neighbours = surface.neighbours(parts, ni, false);
        if neighbours.is_empty() {
            continue;
        }
        let Some(&cur) = current.get(ni) else { continue };
        let mut avg = [0.0; CHANNELS];
        for m in &neighbours {
            if let Some(v) = current.get(*m) {
                for (a, x) in avg.iter_mut().zip(v) {
                    *a += x;
                }
            }
        }
        let t = amount * node.weight;
        let new: [f64; CHANNELS] = std::array::from_fn(|c| cur[c] + (avg[c] / neighbours.len() as f64 - cur[c]) * t);
        for &(pi, k) in &node.members {
            let Some(slot) = values.get_mut(pi).and_then(|v| v.get_mut(k)) else { continue };
            let moved = slot.iter().zip(&new).any(|(a, b)| (a - b).abs() > 1e-9);
            if moved {
                *slot = new;
                if let Some(c) = changed.get_mut(pi) {
                    *c += 1;
                }
            }
        }
    }
    for (pi, part) in parts.iter_mut().enumerate() {
        if part.pinned || changed.get(pi).is_none_or(|c| *c == 0) {
            continue;
        }
        let (Some(v), Some(b)) = (values.get(pi), blends.get_mut(pi)) else { continue };
        match b {
            Some(b) => {
                b.weights.clone_from(v);
                b.normalize();
                if !b.write_to(part.disp)
                    && let Some(c) = changed.get_mut(pi)
                {
                    *c = 0;
                }
            }
            None => {
                let count = v.len();
                part.disp.alphas.resize(count, 0.0);
                for (a, x) in part.disp.alphas.iter_mut().zip(v) {
                    *a = x[0].clamp(0.0, 255.0);
                }
            }
        }
    }
    changed
}

pub(crate) fn subdivide_parts(parts: &mut [SmoothPart<'_>]) -> Vec<usize> {
    let mut changed = vec![0usize; parts.len()];
    let surface = Surface::build(parts, |mesh, _| mesh.face_normal, |_, _| 1.0, None);
    let junctions = find_junctions(parts, &surface);
    let nodes = &surface.nodes;
    let mut new_pos: Vec<DVec3> = nodes.iter().map(|n| n.pos).collect();
    for (ni, node) in nodes.iter().enumerate() {
        let edges = surface.neighbours(parts, ni, false);
        let diagonals = surface.neighbours(parts, ni, true);
        if edges.len() != 4 || diagonals.len() != 4 {
            continue;
        }
        let sum = |list: &[usize]| list.iter().filter_map(|m| nodes.get(*m)).map(|m| m.pos).sum::<DVec3>();
        let p = (node.pos * 16.0 + sum(&edges) * 4.0 + sum(&diagonals)) / 36.0;
        if let Some(slot) = new_pos.get_mut(ni)
            && p.is_finite()
        {
            *slot = p;
        }
    }
    follow_junctions(&junctions, nodes, &mut new_pos);
    write_back(parts, &surface, &new_pos, &mut changed);
    for (part, n) in parts.iter_mut().zip(&changed) {
        if *n > 0 {
            part.disp.subdiv = true;
        }
    }
    changed
}

fn follow_junctions(junctions: &[Junction], nodes: &[Node], new_pos: &mut [DVec3]) {
    let moved = |new_pos: &[DVec3], i: usize| match (new_pos.get(i), nodes.get(i)) {
        (Some(p), Some(n)) => *p != n.pos,
        _ => false,
    };
    for j in junctions {
        if !(moved(new_pos, j.node) || moved(new_pos, j.a) || moved(new_pos, j.b)) {
            continue;
        }
        if let (Some(&a), Some(&b)) = (new_pos.get(j.a), new_pos.get(j.b))
            && let Some(slot) = new_pos.get_mut(j.node)
        {
            *slot = a.lerp(b, j.s);
        }
    }
}

fn write_back(parts: &mut [SmoothPart<'_>], surface: &Surface, new_pos: &[DVec3], changed: &mut [usize]) {
    for (ni, node) in surface.nodes.iter().enumerate() {
        let Some(&np) = new_pos.get(ni) else { continue };
        if !np.is_finite() || np.distance(node.pos) < 1e-9 {
            continue;
        }
        for &(pi, k) in &node.members {
            let Some(part) = parts.get_mut(pi).filter(|p| !p.pinned) else { continue };
            if set_position(part, k, np)
                && let Some(c) = changed.get_mut(pi)
            {
                *c += 1;
            }
        }
    }
}

fn find_junctions(parts: &[SmoothPart<'_>], surface: &Surface) -> Vec<Junction> {
    let mut out: Vec<Junction> = Vec::new();
    let valid = &surface.valid;
    let nodes = &surface.nodes;
    let node_pos = |g: usize| surface.node_of.get(g).and_then(|&n| nodes.get(n)).map(|n| n.pos);
    for (a, pa) in parts.iter().enumerate() {
        let Some(ca) = valid.get(a).copied().filter(|v| *v).and_then(|_| base_corners(pa.mesh)) else { continue };
        let ba = border_indices(pa.mesh.row_len);
        for (b, pb) in parts.iter().enumerate() {
            if a == b {
                continue;
            }
            let Some(cb) = valid.get(b).copied().filter(|v| *v).and_then(|_| base_corners(pb.mesh)) else { continue };
            let bb = border_indices(pb.mesh.row_len);
            for (i, edge_a) in ba.iter().enumerate() {
                let (a0, a1) = (ca[i], ca[(i + 1) % 4]);
                let len = a0.distance(a1);
                let spacing_a = len / (pa.mesh.row_len - 1) as f64;
                let u = (a1 - a0) / len.max(1e-12);
                for (j, edge_b) in bb.iter().enumerate() {
                    let (b0, b1) = (cb[j], cb[(j + 1) % 4]);
                    let Some((lo, hi)) = edge_overlap(a0, a1, b0, b1) else { continue };
                    let spacing_b = b0.distance(b1) / (pb.mesh.row_len - 1) as f64;
                    if spacing_a <= spacing_b + 1e-9 {
                        continue;
                    }
                    let coarse: Vec<(f64, usize)> = edge_a
                        .iter()
                        .filter_map(|&k| Some(((*pa.mesh.base_positions.get(k)? - a0).dot(u), surface.gid(a, k))))
                        .collect();
                    for &k in edge_b {
                        let g = surface.gid(b, k);
                        let Some(&node) = surface.node_of.get(g) else { continue };
                        if out.iter().any(|x| x.node == node) {
                            continue;
                        }
                        if nodes.get(node).is_some_and(|n| n.members.iter().any(|m| m.0 == a)) {
                            continue;
                        }
                        let Some(base) = pb.mesh.base_positions.get(k) else { continue };
                        let off = *base - a0;
                        if (off - u * off.dot(u)).length() > SEAM_TOL {
                            continue;
                        }
                        let t = off.dot(u);
                        if t < lo - SEAM_TOL || t > hi + SEAM_TOL {
                            continue;
                        }
                        if coarse.iter().any(|(tc, _)| (tc - t).abs() <= SEAM_TOL) {
                            continue;
                        }
                        let Some(w) = coarse.windows(2).find(|w| w[0].0 < t && t < w[1].0) else { continue };
                        let s = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-12);
                        let (Some(pa0), Some(pa1), Some(p)) = (node_pos(w[0].1), node_pos(w[1].1), node_pos(g)) else {
                            continue;
                        };
                        if p.distance(pa0.lerp(pa1, s)) > SEAM_TOL {
                            continue;
                        }
                        let (Some(&na), Some(&nb)) = (surface.node_of.get(w[0].1), surface.node_of.get(w[1].1)) else {
                            continue;
                        };
                        out.push(Junction { node, a: na, b: nb, s });
                    }
                }
            }
        }
    }
    out
}

fn set_position(part: &mut SmoothPart<'_>, k: usize, p: DVec3) -> bool {
    let mesh = part.mesh;
    let disp = &mut *part.disp;
    let count = mesh.row_len * mesh.row_len;
    let fnorm = mesh.face_normal;
    disp.normals.resize(count, fnorm);
    disp.distances.resize(count, 0.0);
    disp.offsets.resize(count, DVec3::ZERO);
    disp.offset_normals.resize(count, fnorm);
    disp.alphas.resize(count, 0.0);
    let (Some(&old), Some(&base)) = (mesh.positions.get(k), mesh.base_positions.get(k)) else { return false };
    if old.distance(p) < 1e-9 {
        return false;
    }
    let elevation = if disp.elevation.is_finite() { fnorm * disp.elevation } else { DVec3::ZERO };
    let offset = disp.offsets.get(k).copied().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
    let v = p - base - offset - elevation;
    let len = v.length();
    let (Some(normal), Some(dist)) = (disp.normals.get_mut(k), disp.distances.get_mut(k)) else { return false };
    if len > 1e-9 {
        *normal = v / len;
        *dist = len;
    } else {
        *normal = fnorm;
        *dist = 0.0;
    }
    true
}
