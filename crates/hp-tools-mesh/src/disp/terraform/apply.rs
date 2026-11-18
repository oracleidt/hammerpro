use std::collections::HashMap;

use glam::DVec3;
use hp_editor::{Document, EditResult, FaceRef, Id};
use hp_geom::DispMesh;
use hp_vmf::DispInfo;

use super::grid::{self, DropletParams, Grid};
use super::stamps::{StampShape, local_uv, shape_value};
use super::{ErodeKind, NoiseStyle, PlateauLevel, StampBlend, TerraOp, TerraformSettings, mask};
use crate::disp::paint::{BrushSettings, DEFAULT_DT, MAX_DT, SMOOTH_RATE};
use crate::disp::sculpt::{commit_updates, direction, plane_distance, set_position};

const MAX_DROPLETS_PER_SAMPLE: usize = 4000;
const MAX_PASSES_PER_SAMPLE: usize = 16;

#[derive(Clone, Debug, Default)]
pub struct TerraStroke {
    start: HashMap<FaceRef, Vec<DVec3>>,
    pub origin: Option<DVec3>,
    level: Option<f64>,
    pub stamped: bool,
    rng: u64,
    carry: f64,
}

type Plan = Vec<(FaceRef, Vec<(usize, DVec3)>)>;

fn mesh_of(doc: &Document, f: FaceRef) -> Option<&DispMesh> {
    let mesh = doc.scene().solid(f.solid)?.disp_mesh(f.side)?;
    (mesh.positions.len() == mesh.row_len * mesh.row_len && mesh.base_positions.len() == mesh.positions.len())
        .then_some(&**mesh)
}

fn weight(brush: &BrushSettings, mesh: &DispMesh, k: usize, center: DVec3) -> f64 {
    plane_distance(mesh, k, center, brush.spatial).map_or(0.0, |d| brush.falloff.weight(d / brush.radius))
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn terrace(h: f64, step: f64, sharpness: f64, offset: f64) -> f64 {
    if !h.is_finite() || !step.is_finite() || step <= 0.0 {
        return h;
    }
    let x = (h - offset) / step;
    let f = x.floor();
    let t = x - f;
    let riser = (1.0 - sharpness.clamp(0.0, 1.0)).max(0.02);
    let s = smoothstep(1.0 - riser, 1.0, t);
    (f + s) * step + offset
}

pub fn noise_value(style: NoiseStyle, p: DVec3, scale: f64, octaves: u32, roughness: f64, seed: u64) -> f64 {
    let scale = if scale.is_finite() && scale > 1e-6 { scale } else { 1.0 };
    let (mut sum, mut norm, mut amp, mut freq) = (0.0, 0.0, 1.0, 1.0);
    for octave in 0..octaves.clamp(1, 8) {
        let n =
            hp_geom::perlin(p * (freq / scale), seed.wrapping_mul(0x9E37_79B9).wrapping_add(u64::from(octave) * 7919));
        sum += amp
            * match style {
                NoiseStyle::Fbm => n,
                NoiseStyle::Ridged => {
                    let r = 1.0 - n.abs();
                    r * r
                }
                NoiseStyle::Billow => n.abs(),
            };
        norm += amp;
        amp *= roughness;
        freq *= 2.0;
    }
    if norm <= 0.0 {
        return 0.0;
    }
    let v = sum / norm;
    match style {
        NoiseStyle::Fbm => (v * 1.6).clamp(-1.0, 1.0),
        NoiseStyle::Ridged => v.clamp(0.0, 1.0),
        NoiseStyle::Billow => (v * 1.8).clamp(0.0, 1.0),
    }
}

fn commit(doc: &mut Document, plan: Plan) -> EditResult<usize> {
    let mut updates: Vec<(Id, Vec<(usize, DispInfo)>)> = Vec::new();
    let mut total = 0usize;
    for (f, list) in plan {
        if list.is_empty() {
            continue;
        }
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let Some(mesh) = mesh_of(doc, f) else { continue };
        let Some(disp) = node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()) else { continue };
        let mut new_disp = disp.clone();
        let mut changed = 0usize;
        for (k, p) in list {
            if set_position(&mut new_disp, mesh, k, p) {
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
            Some((_, l)) => l.push((f.side, new_disp)),
            None => updates.push((f.solid, vec![(f.side, new_disp)])),
        }
    }
    commit_updates(doc, updates, total)
}

fn center_face(doc: &Document, faces: &[FaceRef], center: DVec3) -> Option<FaceRef> {
    crate::disp::seams::nearest_disp_face(doc, center).filter(|f| faces.contains(f)).or_else(|| faces.first().copied())
}

pub fn up_axis(doc: &Document, faces: &[FaceRef], center: DVec3, brush: &BrushSettings) -> Option<DVec3> {
    let f = center_face(doc, faces, center)?;
    let mesh = mesh_of(doc, f)?;
    direction(brush, mesh).try_normalize()
}

fn read_faces(doc: &Document, faces: &[FaceRef], center: DVec3, radius: f64, spatial: bool) -> Vec<FaceRef> {
    let mut out: Vec<FaceRef> = match crate::disp::seams::nearest_disp_face(doc, center) {
        Some(start) => crate::disp::seams::brush_faces(doc, start, center, radius, spatial, &[]),
        None => Vec::new(),
    };
    for f in faces {
        if !out.contains(f) {
            out.push(*f);
        }
    }
    out
}

pub fn surface_height(doc: &Document, faces: &[FaceRef], center: DVec3, up: DVec3) -> Option<f64> {
    let mut best: Option<(f64, f64)> = None;
    for f in faces {
        let Some(mesh) = mesh_of(doc, *f) else { continue };
        for p in &mesh.positions {
            let d = *p - center;
            let flat = (d - up * d.dot(up)).length_squared();
            if best.is_none_or(|(b, _)| flat < b) {
                best = Some((flat, up.dot(*p)));
            }
        }
    }
    best.map(|(_, h)| h)
}

fn rim_height(doc: &Document, faces: &[FaceRef], center: DVec3, up: DVec3, size: f64, square: bool) -> Option<f64> {
    let (t1, t2) = grid::basis(up);
    let (mut sum, mut count) = (0.0, 0usize);
    for f in faces {
        let Some(mesh) = mesh_of(doc, *f) else { continue };
        for p in &mesh.positions {
            let d = *p - center;
            let (u, v) = (d.dot(t1) / size, d.dot(t2) / size);
            let r = if square { u.abs().max(v.abs()) } else { (u * u + v * v).sqrt() };
            if (0.85..=1.05).contains(&r) {
                sum += up.dot(*p);
                count += 1;
            }
        }
    }
    (count > 0).then(|| sum / count as f64)
}

fn rotate(du: f64, dv: f64, degrees: f64) -> (f64, f64) {
    let (s, c) = degrees.to_radians().sin_cos();
    (du * c - dv * s, du * s + dv * c)
}

pub fn clone_point(source: DVec3, origin: DVec3, center: DVec3, up: DVec3, degrees: f64) -> DVec3 {
    let (t1, t2) = grid::basis(up);
    let d = center - origin;
    let (su, sv) = rotate(d.dot(t1), d.dot(t2), degrees);
    source + t1 * su + t2 * sv
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sample(
    doc: &mut Document,
    faces: &[FaceRef],
    center: DVec3,
    brush: &BrushSettings,
    tf: &TerraformSettings,
    stroke: &mut TerraStroke,
    invert: bool,
    dt: f64,
) -> EditResult<usize> {
    if !center.is_finite() || !brush.radius.is_finite() || brush.radius <= 0.0 || faces.is_empty() {
        return Ok(0);
    }
    let dt = if dt.is_finite() && dt > 0.0 { dt.min(f64::from(MAX_DT)) } else { f64::from(DEFAULT_DT) };
    let Some(up) = up_axis(doc, faces, center, brush) else { return Ok(0) };
    if stroke.origin.is_none() {
        stroke.origin = Some(center);
        stroke.rng = center.x.to_bits() ^ center.y.to_bits().rotate_left(21) ^ center.z.to_bits().rotate_left(42) | 1;
    }
    let rate = (tf.strength * dt * SMOOTH_RATE).clamp(0.0, 1.0);
    match tf.op {
        TerraOp::Mask => paint_mask(doc, faces, center, brush, rate, invert),
        TerraOp::Erode | TerraOp::Cliff => {
            let plan = grid_op(doc, faces, center, up, brush, tf, stroke, invert, dt);
            commit(doc, plan)
        }
        TerraOp::Stamp => {
            if stroke.stamped {
                return Ok(0);
            }
            let plan = stamp_plan(doc, faces, center, up, brush.radius, tf, invert);
            stroke.stamped = true;
            commit(doc, plan)
        }
        TerraOp::Clone => {
            let (Some(source), Some(origin)) = (tf.clone_source, stroke.origin) else { return Ok(0) };
            let plan = clone_plan(doc, faces, center, up, brush, tf, source, origin, rate);
            commit(doc, plan)
        }
        TerraOp::Terrace | TerraOp::Noise | TerraOp::Plateau => {
            if tf.op == TerraOp::Plateau && stroke.level.is_none() {
                stroke.level = Some(match tf.plateau_level {
                    PlateauLevel::Stroke => surface_height(doc, faces, center, up).unwrap_or_else(|| up.dot(center)),
                    PlateauLevel::Fixed => tf.plateau_height,
                });
            }
            let plan = layer_plan(doc, faces, center, up, brush, tf, stroke, invert, rate);
            commit(doc, plan)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn layer_plan(
    doc: &Document,
    faces: &[FaceRef],
    center: DVec3,
    up: DVec3,
    brush: &BrushSettings,
    tf: &TerraformSettings,
    stroke: &mut TerraStroke,
    invert: bool,
    rate: f64,
) -> Plan {
    let sign = if invert { -1.0 } else { 1.0 };
    let level = stroke.level.unwrap_or_else(|| up.dot(center));
    let mut plan = Plan::new();
    for f in faces {
        let Some(mesh) = mesh_of(doc, *f) else { continue };
        let start = stroke.start.entry(*f).or_insert_with(|| mesh.positions.clone());
        let mut list = Vec::new();
        for (k, p) in mesh.positions.iter().enumerate() {
            let w = weight(brush, mesh, k, center);
            if w <= 0.0 {
                continue;
            }
            let p0 = start.get(k).copied().unwrap_or(*p);
            let (h, h0) = (up.dot(*p), up.dot(p0));
            let goal = match tf.op {
                TerraOp::Terrace => terrace(h0, tf.terrace_step, tf.terrace_sharpness, tf.terrace_offset),
                TerraOp::Noise => {
                    let n = noise_value(
                        tf.noise_style,
                        p0,
                        tf.noise_scale,
                        tf.noise_octaves,
                        tf.noise_roughness,
                        tf.noise_seed,
                    );
                    h0 + tf.noise_amplitude * n * sign
                }
                _ => {
                    if (!invert && h0 > level) || (invert && h0 < level) {
                        level
                    } else {
                        h0
                    }
                }
            };
            let target = h0 + (goal - h0) * w;
            let dh = (target - h) * rate;
            if dh.abs() > 1e-6 {
                list.push((k, *p + up * dh));
            }
        }
        plan.push((*f, list));
    }
    plan
}

pub fn stamp_height(shape: StampShape, tf: &TerraformSettings, du: f64, dv: f64, size: f64) -> (f64, f64) {
    let (u, v) = local_uv(du, dv, size, tf.stamp_rotation);
    let s = shape_value(shape, tf.stamp_image.as_deref(), u, v);
    let inside = if shape == StampShape::Image {
        smoothstep(1.0, 0.94, u.abs().max(v.abs()))
    } else {
        smoothstep(1.0, 0.9, (u * u + v * v).sqrt())
    };
    (s, inside)
}

fn stamp_plan(
    doc: &Document,
    faces: &[FaceRef],
    center: DVec3,
    up: DVec3,
    size: f64,
    tf: &TerraformSettings,
    invert: bool,
) -> Plan {
    let (t1, t2) = grid::basis(up);
    let height = if invert { -tf.stamp_height } else { tf.stamp_height };
    let base = rim_height(doc, faces, center, up, size, tf.stamp_shape == StampShape::Image)
        .or_else(|| surface_height(doc, faces, center, up))
        .unwrap_or_else(|| up.dot(center));
    let mut plan = Plan::new();
    for f in faces {
        let Some(mesh) = mesh_of(doc, *f) else { continue };
        let mut list = Vec::new();
        for (k, p) in mesh.positions.iter().enumerate() {
            let d = *p - center;
            let (s, inside) = stamp_height(tf.stamp_shape, tf, d.dot(t1), d.dot(t2), size);
            if inside <= 0.0 {
                continue;
            }
            let h = up.dot(*p);
            let shaped = base + height * s;
            let dh = match tf.stamp_blend {
                StampBlend::Add => height * s,
                StampBlend::Max => (shaped.max(h) - h) * inside,
                StampBlend::Min => (shaped.min(h) - h) * inside,
                StampBlend::Replace => (shaped - h) * inside,
            };
            if dh.abs() > 1e-6 {
                list.push((k, *p + up * dh));
            }
        }
        plan.push((*f, list));
    }
    plan
}

#[allow(clippy::too_many_arguments)]
fn clone_plan(
    doc: &Document,
    faces: &[FaceRef],
    center: DVec3,
    up: DVec3,
    brush: &BrushSettings,
    tf: &TerraformSettings,
    source: DVec3,
    origin: DVec3,
    rate: f64,
) -> Plan {
    let src = clone_point(source, origin, center, up, tf.clone_rotation);
    let half = brush.radius * 1.2;
    let near = read_faces(doc, &[], src, half, true);
    let Some(grid) = Grid::build_with(doc, &near, src, up, half, 0.5) else { return Plan::new() };
    let (t1, t2) = grid::basis(up);
    let mut plan = Plan::new();
    for f in faces {
        let Some(mesh) = mesh_of(doc, *f) else { continue };
        let mut list = Vec::new();
        for (k, p) in mesh.positions.iter().enumerate() {
            let w = weight(brush, mesh, k, center);
            if w <= 0.0 {
                continue;
            }
            let d = *p - center;
            let (su, sv) = rotate(d.dot(t1), d.dot(t2), tf.clone_rotation);
            let Some(hs) = grid.height_at(src + t1 * su + t2 * sv) else { continue };
            let h = up.dot(*p);
            let dh = (hs + tf.clone_height - h) * (rate * 2.0).min(1.0) * w;
            if dh.abs() > 1e-6 {
                list.push((k, *p + up * dh));
            }
        }
        plan.push((*f, list));
    }
    plan
}

#[allow(clippy::too_many_arguments)]
fn grid_op(
    doc: &Document,
    faces: &[FaceRef],
    center: DVec3,
    up: DVec3,
    brush: &BrushSettings,
    tf: &TerraformSettings,
    stroke: &mut TerraStroke,
    invert: bool,
    dt: f64,
) -> Plan {
    let half = brush.radius * 1.15;
    let near = read_faces(doc, faces, center, half, brush.spatial);
    let Some(mut g) = Grid::build(doc, &near, center, up, half) else { return Plan::new() };
    let old = g.h.clone();
    let n = g.n;
    match tf.op {
        TerraOp::Erode => match tf.erode_kind {
            ErodeKind::Hydraulic => {
                stroke.carry += tf.erode_rate * dt;
                let count = (stroke.carry.floor() as usize).min(MAX_DROPLETS_PER_SAMPLE);
                stroke.carry -= count as f64;
                stroke.carry = stroke.carry.min(MAX_DROPLETS_PER_SAMPLE as f64);
                if count == 0 {
                    return Plan::new();
                }
                let scale = g.cell;
                let mut hc: Vec<f64> = g.h.iter().map(|v| v / scale).collect();
                let p = DropletParams {
                    inertia: tf.erode_inertia,
                    capacity: tf.erode_capacity,
                    deposition: tf.erode_deposition,
                    erosion: tf.erode_strength * 0.6,
                    evaporation: tf.erode_evaporation,
                    radius: brush.radius / g.cell,
                };
                grid::hydraulic(&mut hc, n, count, &p, &mut stroke.rng);
                g.h = hc.iter().map(|v| v * scale).collect();
            }
            ErodeKind::Thermal => {
                stroke.carry += tf.erode_rate / 50.0 * dt;
                let passes = (stroke.carry.floor() as usize).min(MAX_PASSES_PER_SAMPLE);
                stroke.carry -= passes as f64;
                stroke.carry = stroke.carry.min(MAX_PASSES_PER_SAMPLE as f64);
                if passes == 0 {
                    return Plan::new();
                }
                grid::thermal(&mut g.h, n, g.cell, tf.erode_talus, tf.erode_strength, passes);
            }
        },
        _ => return cliff_plan(doc, faces, center, up, brush, tf, &g, invert, dt),
    }
    let mut delta: Vec<f64> = g.h.iter().zip(&old).map(|(a, b)| a - b).collect();
    if tf.op == TerraOp::Erode && tf.erode_kind == ErodeKind::Hydraulic {
        let limit = g.cell * grid::MAX_STEP_CELLS;
        delta = grid::blur(&delta, n, 1).into_iter().map(|d| d.clamp(-limit, limit)).collect();
    }
    let (floor, ceiling) = old.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
    let mut plan = Plan::new();
    for f in faces {
        let Some(mesh) = mesh_of(doc, *f) else { continue };
        let mut list = Vec::new();
        for (k, p) in mesh.positions.iter().enumerate() {
            let w = weight(brush, mesh, k, center);
            if w <= 0.0 {
                continue;
            }
            let (x, y) = g.xy(*p);
            let Some(dd) = g.sample(&delta, x, y) else { continue };
            let h = up.dot(*p);
            let dh = (h + dd * w).clamp(floor.min(h), ceiling.max(h)) - h;
            if dh.abs() > 1e-6 {
                list.push((k, *p + up * dh));
            }
        }
        plan.push((*f, list));
    }
    plan
}

#[allow(clippy::too_many_arguments)]
fn cliff_plan(
    doc: &Document,
    faces: &[FaceRef],
    center: DVec3,
    up: DVec3,
    brush: &BrushSettings,
    tf: &TerraformSettings,
    g: &Grid,
    invert: bool,
    dt: f64,
) -> Plan {
    let n = g.n;
    let radius = (n / 10).max(2);
    let fine = grid::blur(&g.h, n, 1);
    let blurred = grid::blur(&g.h, n, radius);
    let (lo, hi) = grid::local_range(&g.h, n, radius);
    let k = (tf.cliff_strength * dt * 6.0).min(1.0);
    let mut plan = Plan::new();
    for f in faces {
        let Some(mesh) = mesh_of(doc, *f) else { continue };
        let mut list = Vec::new();
        for (i, p) in mesh.positions.iter().enumerate() {
            let w = weight(brush, mesh, i, center);
            if w <= 0.0 {
                continue;
            }
            let (x, y) = g.xy(*p);
            let (Some(b), Some(s), Some(l), Some(u)) =
                (g.sample(&blurred, x, y), g.sample(&fine, x, y), g.sample(&lo, x, y), g.sample(&hi, x, y))
            else {
                continue;
            };
            let h = up.dot(*p);
            let target = if invert { h + (b - h) * k } else { (h + (s - b) * k).clamp(l.min(h), u.max(h)) };
            let dh = (target - h) * w;
            if dh.abs() > 1e-6 {
                list.push((i, *p + up * dh));
            }
        }
        plan.push((*f, list));
    }
    plan
}

fn paint_mask(
    doc: &mut Document,
    faces: &[FaceRef],
    center: DVec3,
    brush: &BrushSettings,
    rate: f64,
    erase: bool,
) -> EditResult<usize> {
    let step = (rate * 1.5).clamp(0.0, 1.0);
    let mut updates = Vec::new();
    let mut total = 0usize;
    for f in faces {
        let Some(mesh) = mesh_of(doc, *f) else { continue };
        let Some(disp) = doc.face_side(*f).and_then(|s| s.disp.as_deref()) else { continue };
        let n = disp.row_len() * disp.row_len();
        if n != mesh.positions.len() {
            continue;
        }
        let mut values = mask::mask_of(disp).unwrap_or_else(|| vec![0.0; n]);
        values.resize(n, 0.0);
        let mut changed = 0usize;
        for (k, v) in values.iter_mut().enumerate() {
            let w = weight(brush, mesh, k, center);
            if w <= 0.0 {
                continue;
            }
            let new = if erase { (*v - step * w).max(0.0) } else { (*v + step * w).min(1.0) };
            if (new - *v).abs() > 1e-6 {
                *v = new;
                changed += 1;
            }
        }
        if changed == 0 {
            continue;
        }
        let mut d = disp.clone();
        if mask::set_mask(&mut d, &values) {
            total += changed;
            updates.push((*f, d));
        }
    }
    if updates.is_empty() {
        return Ok(0);
    }
    doc.set_disp_data(mask::UNDO_MASK, updates)?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrace_steps_flat_and_keeps_levels() {
        assert!((terrace(10.0, 64.0, 1.0, 0.0) - 0.0).abs() < 1e-9);
        assert!((terrace(70.0, 64.0, 1.0, 0.0) - 64.0).abs() < 1e-9);
        assert!((terrace(64.0, 64.0, 0.75, 0.0) - 64.0).abs() < 1e-9);
        assert!(terrace(60.0, 64.0, 0.75, 0.0) > 0.0);
    }

    #[test]
    fn noise_styles_stay_in_range() {
        for style in [NoiseStyle::Fbm, NoiseStyle::Ridged, NoiseStyle::Billow] {
            for i in 0..50 {
                let p = DVec3::new(i as f64 * 37.0, i as f64 * -11.0, 3.0);
                let v = noise_value(style, p, 256.0, 5, 0.5, 3);
                assert!((-1.0..=1.0).contains(&v));
            }
        }
    }

    #[test]
    fn clone_point_rotates_around_the_source() {
        let p = clone_point(DVec3::new(100.0, 0.0, 0.0), DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::Z, 0.0);
        assert!((p - DVec3::new(110.0, 0.0, 0.0)).length() < 1e-9);
        let q = clone_point(DVec3::ZERO, DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::Z, 180.0);
        assert!((q.x + 10.0).abs() < 1e-9);
    }
}
