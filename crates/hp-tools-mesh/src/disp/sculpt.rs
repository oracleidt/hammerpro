use std::collections::HashMap;

use glam::DVec3;
use hp_editor::{Document, EditResult, FaceRef, Id};
use hp_geom::{DispMesh, Ray};
use hp_vmf::DispInfo;

use super::paint::{BrushSettings, PaintAxis, UNDO_SCULPT};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SculptOp {
    #[default]
    Push,
    Carve,
    Project,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PushMode {
    #[default]
    Absolute,
    Adaptive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoundsLimit {
    #[default]
    Attenuated,
    Additive,
}

pub const ADDITIVE_SECONDS: f64 = 0.25;

pub const PROFILE_V: [f64; 5] = [1.0, 0.75, 0.5, 0.25, 0.0];
pub const PROFILE_U: [f64; 5] = [1.0, 0.97, 0.87, 0.6, 0.0];
pub const PROFILE_FLAT: [f64; 5] = [1.0, 1.0, 1.0, 0.85, 0.0];

#[derive(Clone, Debug, PartialEq)]
pub struct SculptSettings {
    pub op: SculptOp,
    pub push_mode: PushMode,
    pub distance: f64,
    pub percent: f64,
    pub carve_depth: f64,
    pub profile: [f64; 5],
    pub project_range: f64,
    pub bounds: BoundsLimit,
    pub falloff_position: f64,
    pub falloff_final: f64,
}

impl Default for SculptSettings {
    fn default() -> Self {
        Self {
            op: SculptOp::Push,
            push_mode: PushMode::Absolute,
            distance: 32.0,
            percent: 25.0,
            carve_depth: 32.0,
            profile: PROFILE_U,
            project_range: 512.0,
            bounds: BoundsLimit::Attenuated,
            falloff_position: 0.0,
            falloff_final: 0.0,
        }
    }
}

impl SculptSettings {
    pub fn sanitize(&mut self) {
        let fix = |v: f64, lo: f64, hi: f64, default: f64| if v.is_finite() { v.clamp(lo, hi) } else { default };
        self.distance = fix(self.distance, 0.0, 4096.0, 32.0);
        self.percent = fix(self.percent, 0.0, 400.0, 25.0);
        self.carve_depth = fix(self.carve_depth, 0.0, 4096.0, 32.0);
        self.project_range = fix(self.project_range, 1.0, 16384.0, 512.0);
        self.falloff_position = fix(self.falloff_position, 0.0, 100.0, 0.0);
        self.falloff_final = fix(self.falloff_final, 0.0, 100.0, 0.0);
        for v in &mut self.profile {
            *v = fix(*v, 0.0, 1.0, 0.0);
        }
    }

    pub fn push_offset(&self, radius: f64) -> f64 {
        match self.push_mode {
            PushMode::Absolute => self.distance,
            PushMode::Adaptive => radius * self.percent / 100.0,
        }
    }

    pub fn push_weight(&self, falloff: hp_geom::Falloff, x: f64) -> f64 {
        if !x.is_finite() || x > 1.0 {
            return 0.0;
        }
        let pos = (self.falloff_position / 100.0).clamp(0.0, 1.0);
        let w = if x <= pos || pos >= 1.0 { 1.0 } else { falloff.weight((x - pos) / (1.0 - pos)) };
        let floor = (self.falloff_final / 100.0).clamp(0.0, 1.0);
        floor + (1.0 - floor) * w
    }

    pub fn profile_at(&self, x: f64) -> f64 {
        if !(0.0..=1.0).contains(&x) {
            return 0.0;
        }
        let f = x * 4.0;
        let i = (f.floor() as usize).min(3);
        let t = f - i as f64;
        let (Some(a), Some(b)) = (self.profile.get(i), self.profile.get(i + 1)) else { return 0.0 };
        a + (b - a) * t
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SculptStroke {
    start: HashMap<FaceRef, Vec<f64>>,
    path: Vec<DVec3>,
}

const MAX_PATH: usize = 4096;

fn direction(brush: &BrushSettings, mesh: &DispMesh) -> DVec3 {
    match brush.axis {
        PaintAxis::X | PaintAxis::Y | PaintAxis::Z | PaintAxis::Custom => brush.axis_vector().unwrap_or(DVec3::Z),
        PaintAxis::FaceNormal
        | PaintAxis::SubdivNormal
        | PaintAxis::View
        | PaintAxis::ViewFlat
        | PaintAxis::BrushCenter => mesh.face_normal,
    }
}

fn plane_distance(mesh: &DispMesh, k: usize, p: DVec3, spatial: bool) -> Option<f64> {
    let (pos, base) = (*mesh.positions.get(k)?, *mesh.base_positions.get(k)?);
    Some(if spatial {
        pos.distance(p)
    } else {
        let d = base - p;
        let n = mesh.face_normal;
        (d - n * d.dot(n)).length()
    })
}

fn segment_distance(p: DVec3, a: DVec3, b: DVec3, normal: Option<DVec3>) -> f64 {
    let flat = |v: DVec3| match normal {
        Some(n) => v - n * v.dot(n),
        None => v,
    };
    let (p, a, b) = (flat(p), flat(a), flat(b));
    hp_geom::point_segment_distance(p, a, b)
}

fn set_position(disp: &mut DispInfo, mesh: &DispMesh, k: usize, p: DVec3) -> bool {
    let count = mesh.row_len * mesh.row_len;
    let fnorm = mesh.face_normal;
    disp.normals.resize(count, fnorm);
    disp.distances.resize(count, 0.0);
    disp.offsets.resize(count, DVec3::ZERO);
    disp.offset_normals.resize(count, fnorm);
    disp.alphas.resize(count, 0.0);
    let (Some(&old), Some(&base)) = (mesh.positions.get(k), mesh.base_positions.get(k)) else { return false };
    if !p.is_finite() || old.distance(p) < 1e-9 {
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

pub(crate) fn project_targets(doc: &Document) -> Vec<Id> {
    doc.selected_solid_ids().into_iter().filter(|id| doc.scene().solid(*id).is_some_and(|n| !n.has_disp())).collect()
}

fn project_point(doc: &Document, targets: &[Id], p: DVec3, dir: DVec3, range: f64) -> Option<DVec3> {
    let mut best: Option<(f64, DVec3)> = None;
    for id in targets {
        let Some(node) = doc.scene().solid(*id) else { continue };
        for d in [dir, -dir] {
            let origin = p - d * range;
            let Some((t, _)) = hp_geom::ray_solid(&Ray::new(origin, d), &node.geom) else { continue };
            let hit = origin + d * t;
            let dist = hit.distance(p);
            if dist <= range && best.is_none_or(|(bd, _)| dist < bd) {
                best = Some((dist, hit));
            }
        }
    }
    best.map(|(_, h)| h)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sculpt_sample(
    doc: &mut Document,
    faces: &[FaceRef],
    center: DVec3,
    brush: &BrushSettings,
    sculpt: &SculptSettings,
    stroke: &mut SculptStroke,
    invert: bool,
    dt: f64,
) -> EditResult<usize> {
    let radius = brush.radius;
    if !center.is_finite() || !radius.is_finite() || radius <= 0.0 {
        return Ok(0);
    }
    if sculpt.op == SculptOp::Carve && stroke.path.last().is_none_or(|q| q.distance(center) > radius * 0.05) {
        stroke.path.push(center);
        if stroke.path.len() > MAX_PATH {
            stroke.path.drain(..stroke.path.len() - MAX_PATH);
        }
    }
    let targets = if sculpt.op == SculptOp::Project { project_targets(doc) } else { Vec::new() };
    if sculpt.op == SculptOp::Project && targets.is_empty() {
        return Ok(0);
    }
    let near: Vec<(DVec3, DVec3)> = if sculpt.op == SculptOp::Carve {
        let pts: Vec<DVec3> = match stroke.path[..] {
            [only] => vec![only, only],
            _ => stroke.path.clone(),
        };
        pts.windows(2)
            .filter_map(|w| match w {
                [a, b] => Some((*a, *b)),
                _ => None,
            })
            .filter(|(a, b)| hp_geom::point_segment_distance(center, *a, *b) <= radius * 2.0)
            .collect()
    } else {
        Vec::new()
    };
    let sign = if invert { -1.0 } else { 1.0 };
    let additive = sculpt.bounds == BoundsLimit::Additive;
    let step = if dt.is_finite() && dt > 0.0 { (dt / ADDITIVE_SECONDS).min(1.0) } else { 1.0 / 15.0 };
    let mut updates: Vec<(Id, Vec<(usize, DispInfo)>)> = Vec::new();
    let mut total = 0usize;
    for f in faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp)) =
            (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
        else {
            continue;
        };
        let n = mesh.row_len * mesh.row_len;
        if n == 0 || mesh.positions.len() != n || mesh.base_positions.len() != n {
            continue;
        }
        let dir = direction(brush, mesh);
        let start = stroke.start.entry(*f).or_insert_with(|| mesh.positions.iter().map(|p| p.dot(dir)).collect());
        let mut new_disp = disp.clone();
        let mut changed = 0usize;
        for k in 0..n {
            let Some(&pos) = mesh.positions.get(k) else { continue };
            let h = pos.dot(dir);
            let h0 = start.get(k).copied().unwrap_or(h);
            let target = match sculpt.op {
                SculptOp::Push => {
                    let Some(d) = plane_distance(mesh, k, center, brush.spatial) else { continue };
                    let w = sculpt.push_weight(brush.falloff, d / radius);
                    if w <= 0.0 {
                        continue;
                    }
                    if additive {
                        h + sign * sculpt.push_offset(radius) * w * step
                    } else {
                        let t = h0 + sign * sculpt.push_offset(radius) * w;
                        if invert { h.min(t) } else { h.max(t) }
                    }
                }
                SculptOp::Carve => {
                    let normal = (!brush.spatial).then_some(mesh.face_normal);
                    let probe = if brush.spatial { pos } else { mesh.base_positions.get(k).copied().unwrap_or(pos) };
                    let d =
                        near.iter().map(|(a, b)| segment_distance(probe, *a, *b, normal)).fold(f64::INFINITY, f64::min);
                    let depth = sculpt.carve_depth * sculpt.profile_at(d / radius);
                    if depth <= 0.0 {
                        continue;
                    }
                    if additive {
                        h - sign * depth * step
                    } else {
                        let t = h0 - sign * depth;
                        if invert { h.max(t) } else { h.min(t) }
                    }
                }
                SculptOp::Project => {
                    let Some(d) = plane_distance(mesh, k, center, brush.spatial) else { continue };
                    let w = brush.falloff.weight(d / radius);
                    if w <= 0.0 {
                        continue;
                    }
                    let Some(hit) = project_point(doc, &targets, pos, dir, sculpt.project_range) else { continue };
                    h + (hit.dot(dir) - h) * w
                }
            };
            if (target - h).abs() < 1e-9 || !target.is_finite() {
                continue;
            }
            if set_position(&mut new_disp, mesh, k, pos + dir * (target - h)) {
                changed += 1;
            }
        }
        if changed == 0 {
            continue;
        }
        if let Some(poly) = node.geom.face_for_side(f.side) {
            hp_geom::update_disp_tags(&mut new_disp, &poly.vertices);
        }
        total += changed;
        match updates.iter_mut().find(|(id, _)| *id == f.solid) {
            Some((_, list)) => list.push((f.side, new_disp)),
            None => updates.push((f.solid, vec![(f.side, new_disp)])),
        }
    }
    if updates.is_empty() {
        return Ok(0);
    }
    doc.transaction(UNDO_SCULPT, |doc| {
        for (id, list) in updates {
            doc.edit_solid(id, |s| {
                for (side, d) in list {
                    if let Some(sd) = s.sides.get_mut(side) {
                        sd.disp = Some(Box::new(d));
                    }
                }
            })?;
        }
        Ok(total)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    #[test]
    fn profile_interpolates_and_clamps() {
        let mut s = SculptSettings { profile: PROFILE_V, ..SculptSettings::default() };
        assert_eq!(s.profile_at(0.0), 1.0);
        assert!((s.profile_at(0.125) - 0.875).abs() < 1e-12);
        assert_eq!(s.profile_at(1.0), 0.0);
        assert_eq!(s.profile_at(1.5), 0.0);
        assert_eq!(s.profile_at(f64::NAN), 0.0);
        s.profile = [f64::NAN, 2.0, -1.0, 0.5, 0.0];
        s.distance = f64::INFINITY;
        s.sanitize();
        assert_eq!(s.profile, [0.0, 1.0, 0.0, 0.5, 0.0]);
        assert_eq!(s.distance, 32.0);
        assert_eq!(SculptSettings { push_mode: PushMode::Adaptive, percent: 50.0, ..s }.push_offset(64.0), 32.0);
    }

    #[test]
    fn push_falloff_position_and_final() {
        use hp_geom::Falloff;
        let mut s = SculptSettings::default();
        assert_eq!(s.push_weight(Falloff::Linear, 0.5), 0.5, "plain falloff by default");
        assert_eq!(s.push_weight(Falloff::Linear, 1.5), 0.0, "outside the brush");
        s.falloff_position = 50.0;
        assert_eq!(s.push_weight(Falloff::Linear, 0.4), 1.0, "full offset inside the position");
        assert!((s.push_weight(Falloff::Linear, 0.75) - 0.5).abs() < 1e-12);
        s.falloff_final = 90.0;
        assert!((s.push_weight(Falloff::Linear, 1.0) - 0.9).abs() < 1e-12, "never below the final share");
        s.falloff_position = f64::NAN;
        s.sanitize();
        assert_eq!(s.falloff_position, 0.0);
    }
}
