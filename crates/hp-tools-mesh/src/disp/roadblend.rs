use glam::{DVec2, DVec3};
use hp_app::runtime::RoadPreview;
use hp_editor::{Document, EditError, FaceRef};
use hp_geom::roadside::{AlphaMode, BlendShape, RoadField, Spine, mitre_vectors};
use hp_geom::{Aabb, DispMesh};
use hp_vmf::DispInfo;
use hp_vmf::multiblend::MultiBlend;

use super::seams::{UNDO_SEW, plan_sew};
use super::smooth::{SmoothPart, Surface, write_back};

pub const UNDO_BLEND: &str = "Blend terrain to road";
pub const MAX_PREVIEW_TRIANGLES: usize = 60_000;
const BOX_Z: f64 = 100_000.0;
const MIN_WEIGHT: f64 = 1e-4;
const EDGE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const REACH_COLOR: [f32; 4] = [1.0, 0.85, 0.2, 0.9];
const ALPHA_COLOR: [f32; 3] = [0.25, 0.6, 1.0];
const HEIGHT_COLOR: [f32; 3] = [1.0, 0.6, 0.1];
const FILL_ALPHA: f32 = 0.4;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlendReport {
    pub faces: usize,
    pub faces_changed: usize,
    pub vertices_moved: usize,
    pub vertices_blended: usize,
}

fn check(shape: &BlendShape) -> Result<(), EditError> {
    shape.validate().map_err(|e| EditError::Invalid(e.to_string()))?;
    if shape.alpha == AlphaMode::TwoWay && shape.road_channel > 1 {
        return Err(EditError::Invalid("the 2 way blend has two textures: the road channel must be 0 or 1".to_owned()));
    }
    Ok(())
}

pub fn candidate_faces(doc: &Document, field: &RoadField<'_>) -> Vec<FaceRef> {
    let Some((lo, hi)) = field.bounds() else { return Vec::new() };
    let reach = Aabb::new(DVec3::new(lo.x, lo.y, -BOX_Z), DVec3::new(hi.x, hi.y, BOX_Z));
    let mut ids = doc.solids_in_box(&reach);
    ids.sort_unstable();
    let mut out = Vec::new();
    for id in ids {
        if !doc.is_solid_visible(id) {
            continue;
        }
        let Some(node) = doc.scene().solid(id) else { continue };
        for (side, mesh) in node.disp.iter().enumerate() {
            let Some(m) = mesh else { continue };
            let overlaps = m.aabb.max.x >= lo.x && m.aabb.min.x <= hi.x && m.aabb.max.y >= lo.y && m.aabb.min.y <= hi.y;
            if overlaps {
                out.push(FaceRef::new(id, side));
            }
        }
    }
    out
}

fn blend_alpha(mesh: &DispMesh, disp: &mut DispInfo, field: &RoadField<'_>, shape: &BlendShape) -> usize {
    let count = mesh.row_len * mesh.row_len;
    if count == 0 || mesh.positions.len() != count {
        return 0;
    }
    let amounts: Vec<f64> = mesh
        .positions
        .iter()
        .map(|p| field.sample(DVec2::new(p.x, p.y)).map_or(0.0, |s| field.alpha_amount(&s)))
        .collect();
    if amounts.iter().all(|a| *a <= MIN_WEIGHT) {
        return 0;
    }
    let mut changed = 0;
    if shape.alpha == AlphaMode::TwoWay {
        disp.alphas.resize(count, 0.0);
        let target = if shape.road_channel == 1 { 255.0 } else { 0.0 };
        for (a, amount) in disp.alphas.iter_mut().zip(&amounts) {
            if *amount <= MIN_WEIGHT {
                continue;
            }
            let cur = if a.is_finite() { *a } else { 0.0 };
            let new = (cur + (target - cur) * amount).clamp(0.0, 255.0);
            if (new - *a).abs() > 1e-9 {
                *a = new;
                changed += 1;
            }
        }
        return changed;
    }
    let mut blend =
        MultiBlend::of_or_alphas(disp).filter(|b| b.len() == count).unwrap_or_else(|| MultiBlend::new(count));
    for (k, amount) in amounts.iter().enumerate() {
        if *amount > MIN_WEIGHT && blend.paint(k, shape.road_channel, *amount) {
            changed += 1;
        }
    }
    if changed > 0 && !blend.write_to(disp) {
        return 0;
    }
    changed
}

pub fn blend_terrain(doc: &mut Document, spines: &[Spine], shape: &BlendShape) -> Result<BlendReport, EditError> {
    check(shape)?;
    if spines.is_empty() {
        return Err(EditError::Nothing("no road to follow"));
    }
    let field = RoadField::new(spines, *shape);
    let faces = candidate_faces(doc, &field);
    if faces.is_empty() {
        return Err(EditError::Nothing("no displacement within reach of the road"));
    }
    let mut work: Vec<(FaceRef, DispMesh, DispInfo)> = Vec::with_capacity(faces.len());
    for f in &faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp)) =
            (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
        else {
            continue;
        };
        work.push((*f, (**mesh).clone(), disp.clone()));
    }
    let mut moved = vec![0usize; work.len()];
    {
        let mut parts: Vec<SmoothPart<'_>> =
            work.iter_mut().map(|(_, mesh, disp)| SmoothPart { mesh, disp, pinned: false }).collect();
        let surface = Surface::build(&parts, |m, _| m.face_normal, |_, _| 1.0, Some(DVec3::Z));
        let new_pos: Vec<DVec3> = surface
            .nodes
            .iter()
            .map(|n| match field.sample(DVec2::new(n.pos.x, n.pos.y)) {
                Some(s) => DVec3::new(n.pos.x, n.pos.y, field.new_height(n.pos.z, &s)),
                None => n.pos,
            })
            .collect();
        write_back(&mut parts, &surface, &new_pos, &mut moved);
    }
    let mut blended = vec![0usize; work.len()];
    if shape.alpha != AlphaMode::Off {
        for (i, (_, mesh, disp)) in work.iter_mut().enumerate() {
            blended[i] = blend_alpha(mesh, disp, &field, shape);
        }
    }
    let mut report = BlendReport { faces: work.len(), ..Default::default() };
    let mut updates = Vec::new();
    for (((f, _, mut d), m), b) in work.into_iter().zip(moved).zip(blended) {
        if m == 0 && b == 0 {
            continue;
        }
        if m > 0
            && let Some(poly) = doc.scene().solid(f.solid).and_then(|n| n.geom.face_for_side(f.side))
        {
            hp_geom::update_disp_tags(&mut d, &poly.vertices);
        }
        report.vertices_moved += m;
        report.vertices_blended += b;
        report.faces_changed += 1;
        updates.push((f, d));
    }
    if updates.is_empty() {
        return Ok(report);
    }
    doc.transaction(UNDO_BLEND, |doc| {
        doc.set_disp_data(UNDO_BLEND, updates)?;
        let plan = plan_sew(doc, &faces, &[]);
        doc.set_disp_data(UNDO_SEW, plan.updates)?;
        Ok::<(), EditError>(())
    })?;
    Ok(report)
}

fn blend_color(height: f64, alpha: f64) -> [f32; 4] {
    let t = if height + alpha <= 0.0 { 1.0 } else { (height / (height + alpha)) as f32 };
    let mix = |a: f32, b: f32| a + (b - a) * t;
    let strength = height.max(alpha) as f32;
    [
        mix(ALPHA_COLOR[0], HEIGHT_COLOR[0]),
        mix(ALPHA_COLOR[1], HEIGHT_COLOR[1]),
        mix(ALPHA_COLOR[2], HEIGHT_COLOR[2]),
        FILL_ALPHA * (0.35 + 0.65 * strength),
    ]
}

fn edge_lines(spine: &Spine, shape: &BlendShape, out: &mut Vec<([f32; 3], [f32; 3], [f32; 4])>) {
    let Ok(mv) = mitre_vectors(spine) else { return };
    let reach = spine.half_width + shape.margin + shape.shoulder;
    for (distance, color) in [(spine.half_width, EDGE_COLOR), (reach, REACH_COLOR)] {
        for side in [1.0, -1.0] {
            let pts: Vec<[f32; 3]> = spine
                .stations
                .iter()
                .enumerate()
                .map(|(i, st)| {
                    let p = *st + mv[i] * (side * distance);
                    let z = spine.top[i] - shape.sink + 1.0;
                    [p.x as f32, p.y as f32, z as f32]
                })
                .collect();
            for w in pts.windows(2) {
                out.push((w[0], w[1], color));
            }
        }
    }
}

pub fn preview(doc: &Document, spines: &[Spine], shape: &BlendShape) -> Result<RoadPreview, EditError> {
    check(shape)?;
    let mut out = RoadPreview::default();
    if spines.is_empty() {
        return Ok(out);
    }
    let field = RoadField::new(spines, *shape);
    for spine in spines {
        edge_lines(spine, shape, &mut out.lines);
    }
    for f in candidate_faces(doc, &field) {
        let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side).cloned()) else { continue };
        let n = mesh.row_len;
        if n < 2 || mesh.positions.len() != n * n {
            continue;
        }
        let cells: Vec<Option<(DVec3, f64, f64)>> = mesh
            .positions
            .iter()
            .map(|p| {
                let s = field.sample(DVec2::new(p.x, p.y))?;
                let z = field.new_height(p.z, &s);
                Some((DVec3::new(p.x, p.y, z + 0.5), s.height_weight * shape.strength, field.alpha_amount(&s)))
            })
            .collect();
        let mut touched = false;
        for i in 0..n - 1 {
            for j in 0..n - 1 {
                let idx = [i * n + j, i * n + j + 1, (i + 1) * n + j + 1, (i + 1) * n + j];
                let verts: Vec<(DVec3, f64, f64)> = idx
                    .iter()
                    .map(|k| match cells.get(*k).copied().flatten() {
                        Some(v) => v,
                        None => (mesh.positions[*k], 0.0, 0.0),
                    })
                    .collect();
                let (h, a) = verts.iter().fold((0.0f64, 0.0f64), |acc, v| (acc.0.max(v.1), acc.1.max(v.2)));
                if h <= MIN_WEIGHT && a <= MIN_WEIGHT {
                    continue;
                }
                if out.tris.len() + 2 > MAX_PREVIEW_TRIANGLES {
                    out.truncated = true;
                    continue;
                }
                touched = true;
                let color = blend_color(h, a);
                let p = |v: usize| verts[v].0.as_vec3().to_array();
                out.tris.push(([p(0), p(1), p(2)], color));
                out.tris.push(([p(0), p(2), p(3)], color));
            }
        }
        if touched {
            out.faces += 1;
        }
        out.vertices += cells.iter().flatten().filter(|c| c.1 > MIN_WEIGHT || c.2 > MIN_WEIGHT).count();
    }
    Ok(out)
}
