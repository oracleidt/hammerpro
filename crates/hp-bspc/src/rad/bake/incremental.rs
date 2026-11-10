use std::collections::HashMap;

use glam::Vec3A;
use rayon::prelude::*;

use super::super::surface::LitFace;
use super::super::tiles::Store;
use super::{BOUNCE_TAIL, Ctx, FaceLight, Gather, GridPoint, Q, cosine_dir, grid_cell, hash, lookup, lookup_a, span};

pub(crate) struct GatherCache {
    pub step: usize,
    pub rays: usize,
    grids: Vec<Gather>,
    memory: usize,
    disk: u64,
    pub ratio: f32,
}

impl GatherCache {
    pub(crate) fn bytes(&self) -> usize {
        self.memory + self.grids.iter().map(|g| g.normals.len() * std::mem::size_of::<[Vec3A; 4]>()).sum::<usize>()
    }

    pub(crate) fn spilled(&self) -> u64 {
        self.disk
    }
}

pub(crate) fn bounce_step(ctx: &Ctx<'_>) -> usize {
    let rays = ctx.params.bounce_rays;
    let total: usize = ctx.faces.iter().map(LitFace::luxels).sum();
    let mut step = ctx.params.bounce_step;
    while step < 16 && total / (step * step) * rays * 8 > ctx.bounce_budget {
        step *= 2;
    }
    step
}

pub(crate) fn gather(
    ctx: &Ctx<'_>,
    cancelled: &(dyn Fn() -> bool + Sync),
    done: &(dyn Fn(usize) + Sync),
) -> Result<Option<GatherCache>, String> {
    let rays = ctx.params.bounce_rays;
    let step = bounce_step(ctx);
    let store = Store::new(ctx.bounce_tile_budget);
    let grids: Option<Vec<Gather>> = (0..ctx.faces.len())
        .into_par_iter()
        .map(|i| {
            if cancelled() {
                return Ok(None);
            }
            let g = ctx.gather_grid(i, step, rays, &store)?;
            done(g.luxels);
            Ok(Some(g))
        })
        .collect::<Result<Option<Vec<_>>, String>>()?;
    let (memory, disk) = store.usage();
    Ok(grids.map(|grids| GatherCache { step, rays, grids, memory, disk, ratio: 0.5 }))
}

pub(crate) fn bounce_cached(
    ctx: &Ctx<'_>,
    cache: &mut GatherCache,
    lights: &mut [FaceLight],
    bounces: u32,
    cancelled: &(dyn Fn() -> bool + Sync),
    log: &mut dyn FnMut(&str),
) -> Result<u32, String> {
    if bounces == 0 || ctx.faces.is_empty() || cache.grids.len() != ctx.faces.len() {
        return Ok(0);
    }
    let rays = cache.rays;
    let grids = &cache.grids;
    let passes = ctx.passes;
    let mut ratio = cache.ratio;
    let direct: Vec<Vec<&[glam::Vec3]>> = lights
        .iter()
        .zip(ctx.faces)
        .map(|(l, f)| {
            let lux = f.luxels();
            (0..passes)
                .map(|p| {
                    let at = l.index(p, 0, 0, lux);
                    l.values.get(at..at + lux).unwrap_or(&[])
                })
                .collect()
        })
        .collect();
    let normals: Vec<usize> = lights.iter().map(|l| l.normals.max(1)).collect();
    let mut emit: Vec<Vec<Vec<Vec3A>>> = Vec::new();
    let mut total: Vec<Vec<Vec3A>> =
        grids.iter().zip(&normals).map(|(g, nn)| vec![Vec3A::ZERO; passes * nn * g.gw * g.gh]).collect();
    let mut last_added = 0.0f32;
    let mut first_added = 0.0f32;
    let mut done = 0;
    for b in 0..bounces {
        if cancelled() {
            return Err("cancelled".to_owned());
        }
        done = b + 1;
        let recv: Vec<Vec<Vec3A>> = (0..ctx.faces.len())
            .into_par_iter()
            .map(|fi| {
                let g = &grids[fi];
                let nn = normals[fi];
                let npts = g.gw * g.gh;
                let mut pts = vec![Vec3A::ZERO; passes * nn * npts];
                for tile in g.hits.tiles(rays * 128) {
                    let (start, hits) = tile?;
                    for point in 0..hits.len() / rays {
                        let gp = start / rays + point;
                        let seed = hash(fi as u32, gp as u32, 0x5eed);
                        let pn = &g.normals[gp];
                        for k in 0..rays {
                            let (hi, s, t) = hits[point * rays + k];
                            let hi = hi as usize;
                            let Some(hf) = ctx.faces.get(hi) else { continue };
                            let (s, t) = (f32::from(s) / Q, f32::from(t) / Q);
                            let mut wts = [1.0f32; 4];
                            if nn > 1 {
                                let dir = cosine_dir(pn[0], k, rays, seed);
                                let d0 = dir.dot(pn[0]).max(1e-4);
                                for n in 1..nn {
                                    wts[n] = dir.dot(pn[n]).max(0.0) / d0;
                                }
                            }
                            let cell = if b == 0 { None } else { Some(grid_cell(&grids[hi], s, t)) };
                            for p in 0..passes {
                                let v = match cell {
                                    None => lookup(direct[hi][p], hf.w, hf.h, s, t),
                                    Some(c) => c.sample(&emit[hi][p], grids[hi].gw),
                                } * hf.reflectivity;
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
                Ok(pts)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut added = 0.0f32;
        emit = recv
            .iter()
            .zip(&normals)
            .map(|(r, nn)| {
                let npts = r.len() / (passes * nn);
                (0..passes).map(|p| r[p * nn * npts..p * nn * npts + npts].to_vec()).collect()
            })
            .collect();
        for (t, r) in total.iter_mut().zip(&recv) {
            for (a, v) in t.iter_mut().zip(r) {
                *a += *v;
                added = added.max(v.max_element());
            }
        }
        if b == 0 {
            first_added = added;
        } else if b == 1 && first_added > 0.0 {
            ratio = (added / first_added).clamp(0.0, 0.9);
        }
        if added < 0.01 {
            break;
        }
        let left = bounces - 1 - b;
        if added < BOUNCE_TAIL && b > 0 && left > 0 && last_added > 0.0 {
            let r = (added / last_added).clamp(0.0, 0.9);
            let tail = r * (1.0 - r.powi(left.min(1000) as i32)) / (1.0 - r);
            for (t, r) in total.iter_mut().zip(&recv) {
                for (a, v) in t.iter_mut().zip(r) {
                    *a += *v * tail;
                }
            }
            break;
        }
        last_added = added;
    }
    drop(direct);
    cache.ratio = ratio;
    let grids = &cache.grids;
    log(&format!("bounced light: {done} passes over {} cached gather grids", grids.len()));
    lights.par_iter_mut().enumerate().for_each(|(fi, fl)| {
        let f = &ctx.faces[fi];
        let g = &grids[fi];
        let nn = fl.normals.max(1);
        let (lux, npts) = (f.luxels(), g.gw * g.gh);
        if fl.values.len() < fl.index(passes - 1, 0, nn - 1, lux) + lux {
            return;
        }
        for p in 0..passes {
            for n in 0..nn {
                let src = &total[fi][(p * nn + n) * npts..(p * nn + n + 1) * npts];
                let base = fl.index(p, 0, n, lux);
                for t in 0..f.h {
                    let (y0, y1, fy) = span(&g.ys, t);
                    for s in 0..f.w {
                        let (x0, x1, fx) = span(&g.xs, s);
                        let at = |x: usize, y: usize| src[y * g.gw + x];
                        let top = at(x0, y0).lerp(at(x1, y0), fx);
                        let bottom = at(x0, y1).lerp(at(x1, y1), fx);
                        fl.values[base + t * f.w + s] += glam::Vec3::from(top.lerp(bottom, fy));
                    }
                }
            }
        }
    });
    Ok(done)
}

pub(crate) struct StitchCells {
    cells: Vec<Vec<(u32, u32)>>,
    by_face: HashMap<u32, Vec<(u32, u32)>>,
}

impl StitchCells {
    pub(crate) fn of(faces: &[LitFace], lights: &[FaceLight]) -> StitchCells {
        let q = |v: f64, k: f64| (v * k).round() as i64;
        let mut groups: HashMap<Vec<i64>, Vec<usize>> = HashMap::new();
        for (i, f) in faces.iter().enumerate() {
            if f.disp.is_some()
                || f.frame.is_none()
                || f.poly.len() < 3
                || lights.get(i).is_none_or(|l| l.dark.len() != f.luxels())
            {
                continue;
            }
            let d = f.poly[0].dot(f.normal);
            let mut key = vec![q(f.normal.x, 1e4), q(f.normal.y, 1e4), q(f.normal.z, 1e4), q(d, 100.0)];
            key.extend(f.lm.iter().flatten().map(|v| q(*v, 1e5)));
            key.extend([q(f.offset.x, 100.0), q(f.offset.y, 100.0), q(f.offset.z, 100.0)]);
            groups.entry(key).or_default().push(i);
        }
        let point = |p: glam::DVec3| (q(p.x, 8.0), q(p.y, 8.0), q(p.z, 8.0));
        let mut cells: Vec<Vec<(u32, u32)>> = Vec::new();
        for list in groups.values().filter(|l| l.len() > 1) {
            let mut parent: Vec<usize> = (0..list.len()).collect();
            fn root(parent: &mut [usize], mut i: usize) -> usize {
                while parent[i] != i {
                    parent[i] = parent[parent[i]];
                    i = parent[i];
                }
                i
            }
            let mut edges: HashMap<(GridPoint, GridPoint), usize> = HashMap::new();
            for (k, &fi) in list.iter().enumerate() {
                let poly = &faces[fi].poly;
                for e in 0..poly.len() {
                    let (a, b) = (point(poly[e]), point(poly[(e + 1) % poly.len()]));
                    let key = if a < b { (a, b) } else { (b, a) };
                    if let Some(&other) = edges.get(&key) {
                        let (ra, rb) = (root(&mut parent, k), root(&mut parent, other));
                        parent[ra] = rb;
                    } else {
                        edges.insert(key, k);
                    }
                }
            }
            let mut shared: HashMap<(usize, i32, i32), Vec<(u32, u32)>> = HashMap::new();
            for (k, &fi) in list.iter().enumerate() {
                let r = root(&mut parent, k);
                let f = &faces[fi];
                for t in 0..f.h {
                    for s in 0..f.w {
                        shared
                            .entry((r, f.mins[0] + s as i32, f.mins[1] + t as i32))
                            .or_default()
                            .push((fi as u32, (t * f.w + s) as u32));
                    }
                }
            }
            for members in shared.into_values() {
                if members.len() > 1 && !members.iter().any(|&(fi, i)| lights[fi as usize].dark[i as usize]) {
                    cells.push(members);
                }
            }
        }
        let mut by_face: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for (c, members) in cells.iter().enumerate() {
            for &(fi, i) in members {
                by_face.entry(fi).or_default().push((i, c as u32));
            }
        }
        StitchCells { cells, by_face }
    }

    pub(crate) fn partners(&self, face: usize) -> impl Iterator<Item = usize> + '_ {
        self.by_face
            .get(&(face as u32))
            .into_iter()
            .flatten()
            .flat_map(|&(_, c)| self.cells[c as usize].iter().map(|&(fi, _)| fi as usize))
    }

    pub(crate) fn bytes(&self) -> usize {
        self.cells.iter().map(|c| c.capacity() * 8 + 24).sum::<usize>()
            + self.by_face.values().map(|v| v.capacity() * 8 + 40).sum::<usize>()
    }

    pub(crate) fn apply(&self, face: usize, faces: &[LitFace], lights: &[FaceLight], values: &mut [Vec3A]) {
        let Some(list) = self.by_face.get(&(face as u32)) else { return };
        let own = &lights[face];
        let lux = faces[face].luxels();
        let nn = own.normals.max(1);
        let passes = own.values.len() / (own.styles.len().max(1) * nn * lux.max(1));
        for &(luxel, c) in list {
            let members = &self.cells[c as usize];
            if members.iter().any(|&(fi, _)| lights[fi as usize].normals != own.normals) {
                continue;
            }
            for (slot, style) in own.styles.iter().enumerate() {
                let slots: Option<Vec<usize>> =
                    members.iter().map(|&(fi, _)| lights[fi as usize].styles.iter().position(|s| s == style)).collect();
                let Some(slots) = slots else { continue };
                for p in 0..passes {
                    for n in 0..nn {
                        let mut sum = Vec3A::ZERO;
                        for (&(fi, i), &ms) in members.iter().zip(&slots) {
                            let l = &lights[fi as usize];
                            let flux = faces[fi as usize].luxels();
                            sum += l
                                .values
                                .get(l.index(p, ms, n, flux) + i as usize)
                                .map_or(Vec3A::ZERO, |v| Vec3A::from(*v));
                        }
                        let at = own.index(p, slot, n, lux) + luxel as usize;
                        if let Some(v) = values.get_mut(at) {
                            *v = sum / members.len() as f32;
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn bounce_delta(
    ctx: &Ctx<'_>,
    cache: &GatherCache,
    emit: &[(usize, Vec<Vec<Vec3A>>)],
    lights: &mut [FaceLight],
    dirty: &mut [bool],
) -> Option<usize> {
    if cache.grids.len() != ctx.faces.len() {
        return None;
    }
    let rays = cache.rays;
    let passes = ctx.passes;
    let tail = 1.0 + cache.ratio / (1.0 - cache.ratio).max(0.1);
    let mut emit_of: Vec<Option<&Vec<Vec<Vec3A>>>> = vec![None; ctx.faces.len()];
    for (fi, field) in emit {
        if let Some(slot) = emit_of.get_mut(*fi) {
            *slot = Some(field);
        }
    }
    let normals: Vec<usize> = lights.iter().map(|l| l.normals.max(1)).collect();
    let received: Vec<Option<Vec<Vec3A>>> = (0..ctx.faces.len())
        .into_par_iter()
        .map(|fi| {
            let g = &cache.grids[fi];
            let nn = normals[fi];
            let npts = g.gw * g.gh;
            let mut pts: Option<Vec<Vec3A>> = None;
            for tile in g.hits.tiles(rays * 128) {
                let Ok((start, hits)) = tile else { return None };
                for point in 0..hits.len() / rays {
                    let gp = start / rays + point;
                    for k in 0..rays {
                        let (hi, s, t) = hits[point * rays + k];
                        let Some(field) = emit_of.get(hi as usize).copied().flatten() else { continue };
                        let hf = &ctx.faces[hi as usize];
                        let (s, t) = (f32::from(s) / Q, f32::from(t) / Q);
                        let mut wts = [1.0f32; 4];
                        if nn > 1 {
                            let pn = &g.normals[gp];
                            let dir = cosine_dir(pn[0], k, rays, hash(fi as u32, gp as u32, 0x5eed));
                            let d0 = dir.dot(pn[0]).max(1e-4);
                            for n in 1..nn {
                                wts[n] = dir.dot(pn[n]).max(0.0) / d0;
                            }
                        }
                        let out = pts.get_or_insert_with(|| vec![Vec3A::ZERO; passes * nn * npts]);
                        for (p, f) in field.iter().enumerate().take(passes) {
                            let v = lookup_a(f, hf.w, hf.h, s, t) * hf.reflectivity * (tail / rays as f32);
                            for n in 0..nn {
                                out[(p * nn + n) * npts + gp] += v * wts[n];
                            }
                        }
                    }
                }
            }
            pts
        })
        .collect();
    let recv: Vec<(usize, Vec<Vec3A>)> =
        received.into_iter().enumerate().filter_map(|(fi, pts)| pts.map(|p| (fi, p))).collect();
    let count = recv.len();
    for (fi, total) in recv {
        if total.iter().all(|v| v.abs().max_element() < 1e-3) {
            continue;
        }
        if let Some(d) = dirty.get_mut(fi) {
            *d = true;
        }
        let f = &ctx.faces[fi];
        let g = &cache.grids[fi];
        let fl = &mut lights[fi];
        let nn = fl.normals.max(1);
        let (lux, npts) = (f.luxels(), g.gw * g.gh);
        if fl.values.len() < fl.index(passes - 1, 0, nn - 1, lux) + lux {
            continue;
        }
        for p in 0..passes {
            for n in 0..nn {
                let src = &total[(p * nn + n) * npts..(p * nn + n + 1) * npts];
                let base = fl.index(p, 0, n, lux);
                for t in 0..f.h {
                    let (y0, y1, fy) = span(&g.ys, t);
                    for s in 0..f.w {
                        let (x0, x1, fx) = span(&g.xs, s);
                        let at = |x: usize, y: usize| src[y * g.gw + x];
                        let top = at(x0, y0).lerp(at(x1, y0), fx);
                        let bottom = at(x0, y1).lerp(at(x1, y1), fx);
                        let v = &mut fl.values[base + t * f.w + s];
                        *v = (*v + glam::Vec3::from(top.lerp(bottom, fy))).max(glam::Vec3::ZERO);
                    }
                }
            }
        }
    }
    Some(count)
}
