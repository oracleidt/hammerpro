use egui::{Pos2, Vec2};
use glam::{DVec2, DVec3};
use hp_app::ViewInfo;
use hp_app::state::AppState;

use super::faces::{FacePick, edge_height};
use crate::common::{HANDLE_TOLERANCE, Snap, pick_surface};

const VERTEX_TOLERANCE_PX: f32 = HANDLE_TOLERANCE + 4.0;

fn tolerance_2d(view: &ViewInfo, pos: Pos2, px: f32) -> f64 {
    view.screen_to_2d(pos).distance(view.screen_to_2d(pos + Vec2::new(px, 0.0)))
}

fn tolerance_3d(view: &ViewInfo, p: DVec3, px: f32) -> Option<f64> {
    let a = view.world_to_screen(p)?;
    let b = view.world_to_screen(p + DVec3::Z)?;
    let per_unit = f64::from(a.distance(b));
    (per_unit > 1e-6).then(|| f64::from(px) / per_unit)
}

fn segment_closest(p: DVec2, a: DVec2, b: DVec2) -> DVec2 {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < 1e-18 {
        return a;
    }
    a + d * ((p - a).dot(d) / len2).clamp(0.0, 1.0)
}

pub(crate) fn face_at(state: &AppState, view: &ViewInfo, pos: Pos2, snap: &Snap) -> Option<FacePick> {
    if view.is_2d() { face_at_2d(state, view, pos, snap) } else { face_at_3d(state, view, pos, snap) }
}

fn face_at_3d(state: &AppState, view: &ViewInfo, pos: Pos2, snap: &Snap) -> Option<FacePick> {
    let hit = pick_surface(state, view, pos)?;
    let node = state.doc.scene().solid(hit.solid)?;
    let face = node.geom.face_for_side(hit.side_index)?;
    let mut pick = FacePick::new(hit.point, face.plane.normal, face.vertices.clone());
    let tol = tolerance_3d(view, hit.point, VERTEX_TOLERANCE_PX).unwrap_or(4.0);
    pick.point.z = edge_height(&pick, hit.point.z, tol).unwrap_or_else(|| snap.v(hit.point.z, 2));
    Some(pick)
}

fn face_at_2d(state: &AppState, view: &ViewInfo, pos: Pos2, snap: &Snap) -> Option<FacePick> {
    let (r, u, d) = view.kind.axes_2d()?;
    let p = view.screen_to_2d(pos);
    let tol = tolerance_2d(view, pos, VERTEX_TOLERANCE_PX);
    let toward = view.kind.toward_viewer();
    let mut best: Option<(f64, f64, FacePick, DVec2)> = None;
    for (id, node) in state.doc.scene().solids() {
        let b = node.bounds;
        if p.x < b.min[r] - tol || p.x > b.max[r] + tol || p.y < b.min[u] - tol || p.y > b.max[u] + tol {
            continue;
        }
        if !state.doc.is_solid_pickable(*id) {
            continue;
        }
        for f in &node.geom.faces {
            let n = f.plane.normal;
            let n2 = DVec2::new(n[r], n[u]);
            if n[d].abs() > 1e-3 || n2.length_squared() < 0.25 || f.vertices.len() < 3 {
                continue;
            }
            let along = n2.perp();
            let pts = f.vertices.iter().map(|v| DVec2::new(v[r], v[u]));
            let lo = pts.clone().min_by(|a, b| a.dot(along).total_cmp(&b.dot(along)))?;
            let hi = pts.max_by(|a, b| a.dot(along).total_cmp(&b.dot(along)))?;
            let c = segment_closest(p, lo, hi);
            let dist = c.distance(p);
            if dist > tol {
                continue;
            }
            let facing = (p - c).dot(n2) >= -1e-9;
            let key = dist + if facing { 0.0 } else { tol * 0.5 };
            let front = f.vertices.iter().map(|v| v.dot(toward)).fold(f64::NEG_INFINITY, f64::max);
            if best.as_ref().is_none_or(|(k, fr, _, _)| key < *k - 1e-9 || ((key - *k).abs() <= 1e-9 && front > *fr)) {
                let depth = f.vertices.iter().map(|v| v[d]).sum::<f64>() / f.vertices.len() as f64;
                let pick = FacePick::new(view.kind.from_2d(c, depth), n, f.vertices.clone());
                best = Some((key, front, pick, c));
            }
        }
    }
    let (_, _, mut pick, c) = best?;
    pick.point.z = if u == 2 {
        let z = c.y;
        edge_height(&pick, z, tol).unwrap_or_else(|| snap.v(z, 2).clamp(pick.bottom(), pick.top()))
    } else {
        pick.top()
    };
    Some(pick)
}

pub(crate) fn vertex_near_2d(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<DVec2> {
    let (r, u, _) = view.kind.axes_2d()?;
    let p = view.screen_to_2d(pos);
    let tol = tolerance_2d(view, pos, VERTEX_TOLERANCE_PX);
    let mut best: Option<(f64, DVec2)> = None;
    for (id, node) in state.doc.scene().solids() {
        let b = node.bounds;
        if p.x < b.min[r] - tol || p.x > b.max[r] + tol || p.y < b.min[u] - tol || p.y > b.max[u] + tol {
            continue;
        }
        if !state.doc.is_solid_pickable(*id) {
            continue;
        }
        for v in node.geom.faces.iter().flat_map(|f| f.vertices.iter()) {
            let q = DVec2::new(v[r], v[u]);
            let dist = q.distance(p);
            if dist <= tol && best.is_none_or(|b| dist < b.0) {
                best = Some((dist, q));
            }
        }
    }
    best.map(|b| b.1)
}

pub(crate) fn snapped_2d(state: &AppState, view: &ViewInfo, pos: Pos2, snap: &Snap) -> DVec2 {
    if snap.is_on()
        && let Some(v) = vertex_near_2d(state, view, pos)
    {
        return v;
    }
    snap.v2(view.screen_to_2d(pos))
}

pub(crate) fn plane_point(view: &ViewInfo, pos: Pos2, axis: usize, at: f64) -> Option<DVec3> {
    let (o, dir) = view.ray(pos);
    if !o.is_finite() || !dir.is_finite() || dir[axis].abs() < 1e-9 {
        return None;
    }
    let t = (at - o[axis]) / dir[axis];
    (t > 0.0 && t.is_finite() && t < 1e7).then(|| o + dir * t)
}

pub(crate) fn surface_point(
    state: &AppState,
    view: &ViewInfo,
    pos: Pos2,
    snap: &Snap,
    fallback: (usize, f64),
) -> Option<DVec3> {
    if let Some(hit) = pick_surface(state, view, pos) {
        let face = state.doc.scene().solid(hit.solid).and_then(|n| n.geom.face_for_side(hit.side_index));
        if snap.is_on()
            && let Some(face) = face
        {
            let near = face
                .vertices
                .iter()
                .filter_map(|v| view.world_to_screen(*v).map(|s| (s.distance(pos), *v)))
                .filter(|(d, _)| *d <= VERTEX_TOLERANCE_PX)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, v)) = near {
                return Some(v);
            }
        }
        return Some(snap.on_plane(hit.point, hit.face_normal));
    }
    let (axis, at) = fallback;
    let p = plane_point(view, pos, axis, at)?;
    let mut out = p;
    for i in 0..3 {
        if i != axis {
            out[i] = snap.v(p[i], i);
        }
    }
    Some(out)
}
