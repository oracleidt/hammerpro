use std::collections::HashMap;

use glam::DVec3;

use super::smooth::{SmoothPart, Surface, set_position};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubdivisionShape {
    #[default]
    Smooth,
    Sphere,
}

pub(crate) fn connected_parts(parts: &[SmoothPart<'_>]) -> Vec<Vec<usize>> {
    let surface = Surface::build(parts, |mesh, _| mesh.face_normal, |_, _| 1.0, None);
    let mut links = vec![Vec::new(); parts.len()];
    let mut edges: HashMap<[usize; 2], Vec<usize>> = HashMap::new();
    for (pi, part) in parts.iter().enumerate() {
        let n = part.mesh.row_len;
        if n < 2 {
            continue;
        }
        let corners = [0, n - 1, n * n - 1, n * (n - 1)];
        for k in 0..4 {
            let (Some(a), Some(b)) = (surface.node(pi, corners[k]), surface.node(pi, corners[(k + 1) % 4])) else {
                continue;
            };
            let key = [a.min(b), a.max(b)];
            for &other in edges.entry(key).or_default().iter() {
                links[pi].push(other);
                links[other].push(pi);
            }
            edges.entry(key).or_default().push(pi);
        }
    }
    let mut visited = vec![false; parts.len()];
    let mut groups = Vec::new();
    for first in 0..parts.len() {
        if visited[first] {
            continue;
        }
        let mut group = vec![first];
        visited[first] = true;
        let mut cursor = 0;
        while cursor < group.len() {
            for &next in &links[group[cursor]] {
                if !visited[next] {
                    visited[next] = true;
                    group.push(next);
                }
            }
            cursor += 1;
        }
        groups.push(group);
    }
    groups
}

pub(crate) fn sphere_parts(parts: &mut [SmoothPart<'_>], amount: f64) -> Vec<usize> {
    let mut changed = vec![0; parts.len()];
    if !amount.is_finite() || amount <= 0.0 {
        return changed;
    }
    let amount = amount.min(1.0);
    for group in connected_parts(parts) {
        let mut min = DVec3::splat(f64::INFINITY);
        let mut max = DVec3::splat(f64::NEG_INFINITY);
        for &pi in &group {
            for p in &parts[pi].mesh.positions {
                min = min.min(*p);
                max = max.max(*p);
            }
        }
        let center = (min + max) * 0.5;
        let radius = (max - min).min_element() * 0.5;
        if !center.is_finite() || !radius.is_finite() || radius <= 1e-6 {
            continue;
        }
        for &pi in &group {
            let part = &mut parts[pi];
            if part.pinned {
                continue;
            }
            for k in 0..part.mesh.positions.len() {
                let p = part.mesh.positions[k];
                let Some(dir) = (p - center).try_normalize() else { continue };
                if set_position(part, k, p.lerp(center + dir * radius, amount)) {
                    changed[pi] += 1;
                }
            }
            if changed[pi] > 0 {
                part.disp.subdiv = true;
            }
        }
    }
    changed
}
