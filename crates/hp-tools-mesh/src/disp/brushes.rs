use std::collections::HashMap;

use glam::DVec3;
use hp_editor::{Document, EditResult, FaceRef, Id};
use hp_geom::DispMesh;
use hp_vmf::DispInfo;

use super::brush_math::{Plane, fit_plane, flatten_target, grab_target, inflate_target, pinch_target};
use super::paint::{BrushSettings, DEFAULT_DT, MAX_DT, SMOOTH_RATE};
use super::sculpt::{SculptOp, SculptSettings, commit_updates, direction, plane_distance, set_position};

type Grabbed = HashMap<usize, (DVec3, f64)>;

#[derive(Clone, Debug)]
struct Grab {
    origin: DVec3,
    faces: HashMap<FaceRef, Grabbed>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BrushStroke {
    grab: Option<Grab>,
    plane: Option<Plane>,
}

impl BrushStroke {
    pub(crate) fn grabbed_faces(&self) -> Vec<FaceRef> {
        self.grab.as_ref().map(|g| g.faces.keys().copied().collect()).unwrap_or_default()
    }
}

pub(crate) fn handles(op: SculptOp) -> bool {
    matches!(op, SculptOp::Pinch | SculptOp::Grab | SculptOp::Inflate | SculptOp::Flatten)
}

fn weight(brush: &BrushSettings, mesh: &DispMesh, k: usize, center: DVec3) -> f64 {
    plane_distance(mesh, k, center, brush.spatial).map_or(0.0, |d| brush.falloff.weight(d / brush.radius))
}

fn rebuild(
    doc: &Document,
    f: FaceRef,
    mut target: impl FnMut(&DispMesh, usize) -> Option<DVec3>,
) -> Option<(DispInfo, usize)> {
    let node = doc.scene().solid(f.solid)?;
    let mesh = node.disp_mesh(f.side)?;
    let disp = node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref())?;
    let n = mesh.row_len * mesh.row_len;
    if n == 0 || mesh.positions.len() != n || mesh.base_positions.len() != n {
        return None;
    }
    let mut new_disp = disp.clone();
    let mut changed = 0usize;
    for k in 0..n {
        if let Some(p) = target(mesh, k)
            && set_position(&mut new_disp, mesh, k, p)
        {
            changed += 1;
        }
    }
    if changed == 0 {
        return None;
    }
    if let Some(poly) = node.geom.face_for_side(f.side) {
        hp_geom::update_disp_tags(&mut new_disp, &poly.vertices);
    }
    Some((new_disp, changed))
}

fn plane_samples(
    doc: &Document,
    faces: &[FaceRef],
    brush: &BrushSettings,
    center: DVec3,
) -> (Vec<(DVec3, f64)>, DVec3) {
    let mut samples = Vec::new();
    let mut up = None;
    for f in faces {
        let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) else { continue };
        if mesh.positions.len() != mesh.row_len * mesh.row_len {
            continue;
        }
        up.get_or_insert_with(|| direction(brush, mesh));
        for (k, p) in mesh.positions.iter().enumerate() {
            let w = weight(brush, mesh, k, center);
            if w > 0.0 {
                samples.push((*p, w));
            }
        }
    }
    (samples, up.unwrap_or(DVec3::Z))
}

fn capture_grab(doc: &Document, faces: &[FaceRef], brush: &BrushSettings, center: DVec3) -> Option<Grab> {
    let mut out: HashMap<FaceRef, Grabbed> = HashMap::new();
    for f in faces {
        let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) else { continue };
        if mesh.positions.len() != mesh.row_len * mesh.row_len {
            continue;
        }
        let held: Grabbed = mesh
            .positions
            .iter()
            .enumerate()
            .filter_map(|(k, p)| {
                let w = weight(brush, mesh, k, center);
                (w > 0.0).then_some((k, (*p, w)))
            })
            .collect();
        if !held.is_empty() {
            out.insert(*f, held);
        }
    }
    (!out.is_empty()).then_some(Grab { origin: center, faces: out })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sample(
    doc: &mut Document,
    faces: &[FaceRef],
    center: DVec3,
    brush: &BrushSettings,
    sculpt: &SculptSettings,
    stroke: &mut BrushStroke,
    invert: bool,
    dt: f64,
) -> EditResult<usize> {
    if !center.is_finite() || !brush.radius.is_finite() || brush.radius <= 0.0 {
        return Ok(0);
    }
    let sign = if invert { -1.0 } else { 1.0 };
    let dt = if dt.is_finite() && dt > 0.0 { dt.min(f64::from(MAX_DT)) } else { f64::from(DEFAULT_DT) };
    let mut updates: Vec<(Id, Vec<(usize, DispInfo)>)> = Vec::new();
    let mut total = 0usize;
    let mut keep = |f: FaceRef, built: Option<(DispInfo, usize)>| {
        let Some((d, n)) = built else { return };
        total += n;
        match updates.iter_mut().find(|(id, _)| *id == f.solid) {
            Some((_, list)) => list.push((f.side, d)),
            None => updates.push((f.solid, vec![(f.side, d)])),
        }
    };
    match sculpt.op {
        SculptOp::Grab => {
            if stroke.grab.is_none() {
                stroke.grab = capture_grab(doc, faces, brush, center);
            }
            let Some(grab) = stroke.grab.as_ref() else { return Ok(0) };
            let delta = (center - grab.origin) * sign;
            for (f, held) in &grab.faces {
                let built = rebuild(doc, *f, |_, k| held.get(&k).map(|(o, w)| grab_target(*o, delta, *w)));
                keep(*f, built);
            }
        }
        SculptOp::Pinch | SculptOp::Inflate => {
            let step = brush.strength * dt * sign;
            for f in faces {
                let built = rebuild(doc, *f, |mesh, k| {
                    let w = weight(brush, mesh, k, center);
                    if w <= 0.0 {
                        return None;
                    }
                    let pos = *mesh.positions.get(k)?;
                    Some(if sculpt.op == SculptOp::Pinch {
                        pinch_target(pos, center, direction(brush, mesh), step * w)
                    } else {
                        inflate_target(pos, mesh.normals.get(k).copied().unwrap_or(mesh.face_normal), step * w)
                    })
                });
                keep(*f, built);
            }
        }
        SculptOp::Flatten => {
            let plane = match stroke.plane.filter(|_| sculpt.flatten_fixed) {
                Some(p) => p,
                None => {
                    let (samples, up) = plane_samples(doc, faces, brush, center);
                    let Some(p) = fit_plane(&samples, up) else { return Ok(0) };
                    stroke.plane = Some(p);
                    p
                }
            };
            let rate = (sculpt.flatten_strength * dt * SMOOTH_RATE).clamp(0.0, 1.0) * sign;
            for f in faces {
                let built = rebuild(doc, *f, |mesh, k| {
                    let w = weight(brush, mesh, k, center);
                    if w <= 0.0 {
                        return None;
                    }
                    Some(flatten_target(*mesh.positions.get(k)?, &plane, rate * w))
                });
                keep(*f, built);
            }
        }
        SculptOp::Push | SculptOp::Carve | SculptOp::Project | SculptOp::Relax => return Ok(0),
    }
    commit_updates(doc, updates, total)
}
