use std::collections::HashMap;

use glam::Vec3A;
use rayon::prelude::*;

use super::super::surface::LitFace;
use super::{
    BOUNCE_TAIL, FaceLight, Gather, MAX_STYLES, Q, blur_grid, blur_radius, blur_weights, cosine_dir, grid_cell, hash,
    patch_light, span, split_hit,
};

pub(crate) const MIN_BOUNCED_STYLE: f32 = 0.5;
const SILENT: f32 = 1e-3;

pub(crate) type StyleField = Vec<Vec<glam::Vec3>>;

pub(crate) fn direct_fields(
    lights: &[FaceLight],
    faces: &[LitFace],
    passes: usize,
) -> HashMap<u8, Vec<(usize, StyleField)>> {
    let mut out: HashMap<u8, Vec<(usize, StyleField)>> = HashMap::new();
    for (fi, (l, f)) in lights.iter().zip(faces).enumerate() {
        let lux = f.luxels();
        for (slot, &style) in l.styles.iter().enumerate() {
            if style == 0 {
                continue;
            }
            let field: StyleField = (0..passes)
                .map(|p| {
                    let at = l.index(p, slot, 0, lux);
                    l.values.get(at..at + lux).map_or_else(|| vec![glam::Vec3::ZERO; lux], <[glam::Vec3]>::to_vec)
                })
                .collect();
            if field.iter().flatten().any(|v| v.abs().max_element() > SILENT) {
                out.entry(style).or_default().push((fi, field));
            }
        }
    }
    out
}

fn seen_by(grids: &[Gather], rays: usize) -> Result<Vec<Vec<u32>>, String> {
    let sees: Vec<Vec<u32>> = grids
        .par_iter()
        .map(|g| {
            let mut list = Vec::new();
            for tile in g.hits.tiles(rays * 128) {
                let (_, hits) = tile?;
                list.extend(hits.iter().map(|h| split_hit(h.0).0).filter(|&h| h != usize::MAX).map(|h| h as u32));
            }
            list.sort_unstable();
            list.dedup();
            Ok(list)
        })
        .collect::<Result<_, String>>()?;
    let mut by: Vec<Vec<u32>> = vec![Vec::new(); grids.len()];
    for (fi, list) in sees.iter().enumerate() {
        for &h in list {
            if let Some(v) = by.get_mut(h as usize) {
                v.push(fi as u32);
            }
        }
    }
    Ok(by)
}

fn peak(v: &[Vec3A]) -> f32 {
    v.iter().map(|x| x.abs().max_element()).fold(0.0, f32::max)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn bounce_styles(
    faces: &[LitFace],
    grids: &[Gather],
    rays: usize,
    passes: usize,
    lights: &mut [FaceLight],
    fields: &HashMap<u8, Vec<(usize, StyleField)>>,
    bounces: u32,
    water: Vec3A,
) -> Result<Vec<usize>, String> {
    if bounces == 0 || fields.is_empty() || grids.len() != faces.len() || lights.len() != faces.len() || rays == 0 {
        return Ok(Vec::new());
    }
    let by = seen_by(grids, rays)?;
    let mut touched = vec![false; faces.len()];
    let mut styles: Vec<u8> = fields.keys().copied().collect();
    styles.sort_unstable();
    for style in styles {
        let total = bounce_one(faces, grids, rays, passes, lights, &by, &fields[&style], (bounces, water))?;
        for (fi, values) in total {
            if add_style(&faces[fi], &grids[fi], &mut lights[fi], style, passes, &values) {
                touched[fi] = true;
            }
        }
    }
    Ok((0..faces.len()).filter(|&i| touched[i]).collect())
}

#[allow(clippy::too_many_arguments)]
fn bounce_one(
    faces: &[LitFace],
    grids: &[Gather],
    rays: usize,
    passes: usize,
    lights: &[FaceLight],
    by: &[Vec<u32>],
    field: &[(usize, StyleField)],
    (bounces, water): (u32, Vec3A),
) -> Result<Vec<(usize, Vec<Vec3A>)>, String> {
    let mut emit: HashMap<usize, Vec<Vec3A>> = HashMap::new();
    for (fi, f) in field {
        let (Some(g), Some(face)) = (grids.get(*fi), faces.get(*fi)) else { continue };
        let npts = g.gw * g.gh;
        let mut out = vec![Vec3A::ZERO; passes * npts];
        for (p, src) in f.iter().enumerate().take(passes) {
            patch_light(src, face.w, face.h, g, &mut out[p * npts..(p + 1) * npts]);
        }
        if peak(&out) > SILENT {
            emit.insert(*fi, out);
        }
    }
    let mut total: HashMap<usize, Vec<Vec3A>> = HashMap::new();
    let mut last_added = 0.0f32;
    for b in 0..bounces {
        if emit.is_empty() {
            break;
        }
        let mut receivers: Vec<u32> = emit.keys().flat_map(|&e| by[e].iter().copied()).collect();
        receivers.sort_unstable();
        receivers.dedup();
        let recv: Vec<(usize, Vec<Vec3A>)> = receivers
            .par_iter()
            .map(|&fi| {
                let fi = fi as usize;
                let g = &grids[fi];
                let nn = lights[fi].normals.max(1);
                let npts = g.gw * g.gh;
                let mut pts = vec![Vec3A::ZERO; passes * nn * npts];
                for tile in g.hits.tiles(rays * 128) {
                    let (start, hits) = tile?;
                    for point in 0..hits.len() / rays {
                        let gp = start / rays + point;
                        for k in 0..rays {
                            let (hi, s, t) = hits[point * rays + k];
                            let (hi, mirrored) = split_hit(hi);
                            let Some(src) = emit.get(&hi) else { continue };
                            let hg = &grids[hi];
                            let hnpts = hg.gw * hg.gh;
                            let cell = grid_cell(hg, f32::from(s) / Q, f32::from(t) / Q);
                            let mut wts = [1.0f32; 4];
                            if nn > 1 {
                                let pn = &g.normals[gp];
                                let dir = cosine_dir(pn[0], k, rays, hash(fi as u32, gp as u32, 0x5eed));
                                let d0 = dir.dot(pn[0]).max(1e-4);
                                for n in 1..nn {
                                    wts[n] = dir.dot(pn[n]).max(0.0) / d0;
                                }
                            }
                            let refl = if mirrored { faces[hi].reflectivity * water } else { faces[hi].reflectivity };
                            for p in 0..passes {
                                let v = cell.sample(&src[p * hnpts..(p + 1) * hnpts], hg.gw) * refl;
                                for n in 0..nn {
                                    pts[(p * nn + n) * npts + gp] += v * wts[n];
                                }
                            }
                        }
                    }
                }
                for v in &mut pts {
                    *v /= rays as f32;
                }
                Ok((fi, pts))
            })
            .collect::<Result<_, String>>()?;
        let mut added = 0.0f32;
        let mut next: HashMap<usize, Vec<Vec3A>> = HashMap::new();
        for (fi, pts) in &recv {
            let nn = lights[*fi].normals.max(1);
            let npts = pts.len() / (passes * nn);
            let e: Vec<Vec3A> =
                (0..passes).flat_map(|p| pts[p * nn * npts..p * nn * npts + npts].iter().copied()).collect();
            let top = peak(pts);
            added = added.max(top);
            if peak(&e) > SILENT {
                next.insert(*fi, e);
            }
        }
        for (fi, pts) in &recv {
            let t = total.entry(*fi).or_insert_with(|| vec![Vec3A::ZERO; pts.len()]);
            for (a, v) in t.iter_mut().zip(pts) {
                *a += *v;
            }
        }
        emit = next;
        if added < 0.01 {
            break;
        }
        let left = bounces - 1 - b;
        if added < BOUNCE_TAIL && b > 0 && left > 0 && last_added > 0.0 {
            let r = (added / last_added).clamp(0.0, 0.9);
            let tail = r * (1.0 - r.powi(left.min(1000) as i32)) / (1.0 - r);
            for (fi, pts) in &recv {
                if let Some(t) = total.get_mut(fi) {
                    for (a, v) in t.iter_mut().zip(pts) {
                        *a += *v * tail;
                    }
                }
            }
            break;
        }
        last_added = added;
    }
    let mut out: Vec<(usize, Vec<Vec3A>)> = total.into_iter().collect();
    out.sort_unstable_by_key(|o| o.0);
    out.par_iter_mut().for_each(|(fi, planes)| {
        let (g, f, fl) = (&grids[*fi], &faces[*fi], &lights[*fi]);
        let radius = blur_radius(rays, g.step);
        let (wx, wy) = (blur_weights(&g.xs, radius), blur_weights(&g.ys, radius));
        let keep: Vec<bool> =
            g.ys.iter()
                .flat_map(|&y| g.xs.iter().map(move |&x| !fl.dark.get(y * f.w + x).copied().unwrap_or(false)))
                .collect();
        let mut tmp = Vec::new();
        for plane in planes.chunks_exact_mut(g.gw * g.gh) {
            blur_grid(plane, &keep, (&wx, &wy), &mut tmp);
        }
    });
    Ok(out)
}

fn add_style(f: &LitFace, g: &Gather, fl: &mut FaceLight, style: u8, passes: usize, planes: &[Vec3A]) -> bool {
    let nn = fl.normals.max(1);
    let (lux, npts) = (f.luxels(), g.gw * g.gh);
    if planes.len() != passes * nn * npts || fl.values.len() != passes * fl.styles.len() * nn * lux {
        return false;
    }
    let mut luxels = vec![glam::Vec3::ZERO; passes * nn * lux];
    let mut top = 0.0f32;
    for (k, src) in planes.chunks_exact(npts).enumerate() {
        for t in 0..f.h {
            let (y0, y1, fy) = span(&g.ys, t);
            for s in 0..f.w {
                let (x0, x1, fx) = span(&g.xs, s);
                let at = |x: usize, y: usize| src[y * g.gw + x];
                let v = at(x0, y0).lerp(at(x1, y0), fx).lerp(at(x0, y1).lerp(at(x1, y1), fx), fy);
                top = top.max(v.max_element());
                luxels[k * lux + t * f.w + s] = v.into();
            }
        }
    }
    let slot = match fl.styles.iter().position(|s| *s == style) {
        Some(slot) => slot,
        None => {
            if top < MIN_BOUNCED_STYLE || fl.styles.len() >= MAX_STYLES {
                return false;
            }
            insert_slot(fl, style, passes, lux);
            fl.styles.len() - 1
        }
    };
    for p in 0..passes {
        for n in 0..nn {
            let base = fl.index(p, slot, n, lux);
            let src = &luxels[(p * nn + n) * lux..(p * nn + n + 1) * lux];
            for (d, v) in fl.values[base..base + lux].iter_mut().zip(src) {
                *d = (*d + *v).max(glam::Vec3::ZERO);
            }
        }
    }
    true
}

pub(crate) fn insert_slot(fl: &mut FaceLight, style: u8, passes: usize, lux: usize) {
    let nn = fl.normals.max(1);
    let old = FaceLight { styles: fl.styles.clone(), normals: nn, values: Vec::new(), dark: Vec::new() };
    let kept = fl.styles.len();
    let mut values = vec![glam::Vec3::ZERO; passes * (kept + 1) * nn * lux];
    fl.styles.push(style);
    for p in 0..passes {
        for slot in 0..kept {
            for n in 0..nn {
                let from = old.index(p, slot, n, lux);
                let to = fl.index(p, slot, n, lux);
                values[to..to + lux].copy_from_slice(&fl.values[from..from + lux]);
            }
        }
    }
    fl.values = values;
}
