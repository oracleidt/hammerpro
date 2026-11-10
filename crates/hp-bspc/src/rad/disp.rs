use ahash::AHashMap as HashMap;

use glam::{DVec2, DVec3, Vec3A};
use rayon::prelude::*;

use super::bake::{Ctx, FaceLight};
use super::surface::LitFace;

const MAX_SAMPLE_RADIUS: f64 = 512.0;
const MAX_PATCH_RADIUS: f64 = 1500.0;
const DISPCHOP: f64 = 8.0;
const MIN_ANGLE: f64 = 0.15;
const MAX_SLOTS: usize = 4;

pub(crate) fn sample_coord(f: &LitFace, s: usize, t: usize) -> DVec2 {
    let c = |i: usize, n: usize| if n > 1 { (i as f64 + 0.5) / n as f64 * (n - 1) as f64 } else { 0.0 };
    DVec2::new(c(s, f.w), c(t, f.h))
}

fn sample_size(f: &LitFace) -> f64 {
    let v = DVec3::new(f.lm[0][0], f.lm[0][1], f.lm[0][2]).length();
    if v > 0.0 { 1.0 / v } else { 16.0 }
}

fn samples_of(f: &LitFace) -> Vec<(DVec3, DVec3)> {
    let mut out = Vec::with_capacity(f.luxels());
    for t in 0..f.h {
        for s in 0..f.w {
            out.push(match &f.disp {
                Some(d) => {
                    let uv = DVec2::new((s as f64 + 0.5) / f.w as f64, (t as f64 + 0.5) / f.h as f64);
                    d.sample(uv, 1.0, f.normal)
                }
                None => {
                    let c = f.flat_sample(s, t).0;
                    let p = f.plane_point(c.x, c.y);
                    (p, f.phong(p))
                }
            });
        }
    }
    out
}

fn luxels_of(f: &LitFace) -> Vec<(DVec3, DVec3)> {
    let Some(d) = &f.disp else { return Vec::new() };
    let step = |i: usize, n: usize| if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 };
    let mut out = Vec::with_capacity(f.luxels());
    for t in 0..f.h {
        for s in 0..f.w {
            out.push(d.sample(DVec2::new(step(s, f.w), step(t, f.h)), 1.0, f.normal));
        }
    }
    out
}

struct Grid<T> {
    size: f64,
    cells: HashMap<(i64, i64, i64), Vec<T>>,
}

impl<T: Copy> Grid<T> {
    fn key(&self, p: DVec3) -> (i64, i64, i64) {
        let k = |v: f64| (v / self.size).floor() as i64;
        (k(p.x), k(p.y), k(p.z))
    }

    fn near<'a>(&'a self, p: DVec3, out: &mut Vec<&'a [T]>) {
        out.clear();
        let (x, y, z) = self.key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = self.cells.get(&(x + dx, y + dy, z + dz)) {
                        out.push(v);
                    }
                }
            }
        }
    }
}

fn neighbors(ctx: &Ctx<'_>, disps: &[usize]) -> Vec<Vec<usize>> {
    let map = ctx.map;
    let mut lit = vec![None; map.faces.len()];
    for (i, f) in ctx.faces.iter().enumerate() {
        if let Some(slot) = lit.get_mut(f.face) {
            *slot = Some(i);
        }
    }
    let mut vert_faces: HashMap<usize, Vec<usize>> = HashMap::new();
    for (fi, f) in map.faces.iter().enumerate() {
        if lit[fi].is_none() {
            continue;
        }
        for v in map.face_vertices(f) {
            vert_faces.entry(v).or_default().push(fi);
        }
    }
    disps
        .iter()
        .map(|&di| {
            let face = &map.faces[ctx.faces[di].face];
            let mut out = vec![di];
            for v in map.face_vertices(face) {
                for &fi in vert_faces.get(&v).map_or(&[][..], |v| v.as_slice()) {
                    if let Some(li) = lit[fi]
                        && !out.contains(&li)
                    {
                        out.push(li);
                    }
                }
            }
            out
        })
        .collect()
}

pub(crate) fn radial(ctx: &Ctx<'_>, light: &mut [FaceLight]) {
    let faces = ctx.faces;
    let disps: Vec<usize> = (0..faces.len()).filter(|&i| faces[i].disp.is_some()).collect();
    if disps.is_empty() {
        return;
    }
    let near = neighbors(ctx, &disps);
    let mut needed = vec![false; faces.len()];
    for list in &near {
        for &i in list {
            needed[i] = true;
        }
    }
    let samples: Vec<Vec<(DVec3, DVec3)>> =
        (0..faces.len()).into_par_iter().map(|i| if needed[i] { samples_of(&faces[i]) } else { Vec::new() }).collect();
    let passes = ctx.passes;
    let blended: Vec<(usize, Vec<glam::Vec3>)> = disps
        .par_iter()
        .zip(&near)
        .map(|(&di, list)| {
            let f = &faces[di];
            let size = sample_size(f);
            let radius = ((size * size * 2.0).sqrt() * 2.2).min(MAX_SAMPLE_RADIUS);
            let r2 = radius * radius;
            let own = &light[di];
            let lux = f.luxels();
            let nn = own.normals;
            let width = passes * nn;
            let mut vals: Vec<Vec3A> = Vec::new();
            let mut grid: Grid<(DVec3, DVec3, bool, [u32; MAX_SLOTS])> = Grid { size: radius, cells: HashMap::new() };
            for &ni in list {
                let nb = &light[ni];
                let nlux = faces[ni].luxels();
                let bumped = nb.normals > 1;
                for (k, &(p, n)) in samples[ni].iter().enumerate() {
                    let mut at = [u32::MAX; MAX_SLOTS];
                    for (slot, &style) in own.styles.iter().enumerate().take(MAX_SLOTS) {
                        let Some(nslot) = nb.styles.iter().position(|s| *s == style) else { continue };
                        at[slot] = vals.len() as u32;
                        for pass in 0..passes {
                            for j in 0..nn {
                                let from = if bumped { j.min(nb.normals - 1) } else { 0 };
                                vals.push(
                                    nb.values
                                        .get(nb.index(pass, nslot, from, nlux) + k)
                                        .map_or(Vec3A::ZERO, |v| Vec3A::from(*v)),
                                );
                            }
                        }
                    }
                    let key = grid.key(p);
                    grid.cells.entry(key).or_default().push((p, n, bumped, at));
                }
            }
            let mut values = vec![glam::Vec3::ZERO; own.values.len()];
            let mut found = Vec::new();
            let mut acc = vec![Vec3A::ZERO; passes * nn];
            for (li, (lp, ln)) in luxels_of(f).into_iter().enumerate() {
                grid.near(lp, &mut found);
                for slot in 0..own.styles.len() {
                    let mut weight = 0.0f64;
                    acc.fill(Vec3A::ZERO);
                    for &(sp, sn, bumped, at) in found.iter().copied().flatten() {
                        let angle = sn.dot(ln);
                        if angle < MIN_ANGLE {
                            continue;
                        }
                        let infl = 1.0 - sp.distance_squared(lp) / r2;
                        if infl <= 0.0 {
                            continue;
                        }
                        let Some(at) = at.get(slot).copied().filter(|a| *a != u32::MAX) else { continue };
                        let mut infl = infl * angle;
                        if nn > 1 && !bumped {
                            infl *= 0.05;
                        }
                        let at = at as usize;
                        for (a, v) in acc.iter_mut().zip(&vals[at..at + width]) {
                            *a += *v * infl as f32;
                        }
                        weight += infl;
                    }
                    if weight > 0.0 {
                        for p in 0..passes {
                            for n in 0..nn {
                                values[own.index(p, slot, n, lux) + li] = (acc[p * nn + n] / weight as f32).into();
                            }
                        }
                    }
                }
            }
            (di, values)
        })
        .collect();
    for (di, values) in blended {
        light[di].values = values;
    }
}

pub(crate) struct Unbounced {
    near: Vec<Vec<usize>>,
    disps: Vec<usize>,
    before: Vec<Vec<glam::Vec3>>,
}

fn style0(fl: &FaceLight, passes: usize, lux: usize) -> Vec<glam::Vec3> {
    let mut out = Vec::with_capacity(passes * fl.normals * lux);
    for p in 0..passes {
        for n in 0..fl.normals {
            let at = fl.index(p, 0, n, lux);
            out.extend_from_slice(fl.values.get(at..at + lux).unwrap_or(&[]));
        }
    }
    out
}

pub(crate) fn before_bounce(ctx: &Ctx<'_>, light: &[FaceLight]) -> Option<Unbounced> {
    let faces = ctx.faces;
    let disps: Vec<usize> = (0..faces.len()).filter(|&i| faces[i].disp.is_some()).collect();
    if disps.is_empty() {
        return None;
    }
    let near = neighbors(ctx, &disps);
    let mut needed = vec![false; faces.len()];
    for list in &near {
        for &i in list {
            needed[i] = true;
        }
    }
    let before = (0..faces.len())
        .into_par_iter()
        .map(|i| if needed[i] { style0(&light[i], ctx.passes, faces[i].luxels()) } else { Vec::new() })
        .collect();
    Some(Unbounced { near, disps, before })
}

pub(crate) fn after_bounce(ctx: &Ctx<'_>, light: &mut [FaceLight], saved: Unbounced) {
    let faces = ctx.faces;
    let passes = ctx.passes;
    let Unbounced { near, disps, before } = saved;
    let normals: Vec<usize> = light.iter().map(|l| l.normals).collect();
    let view: &[FaceLight] = light;
    let bounced = |ni: usize, p: usize, from: usize, k: usize| -> Vec3A {
        let (nlux, nbn) = (faces[ni].luxels(), normals[ni]);
        let l = &view[ni];
        match (l.values.get(l.index(p, 0, from, nlux) + k), before[ni].get((p * nbn + from) * nlux + k)) {
            (Some(now), Some(then)) => Vec3A::from(*now) - Vec3A::from(*then),
            _ => Vec3A::ZERO,
        }
    };
    let blended: Vec<(usize, Vec<Vec3A>)> = disps
        .par_iter()
        .zip(&near)
        .map(|(&di, list)| {
            let f = &faces[di];
            let radius = (sample_size(f) * DISPCHOP * 2.2).min(MAX_PATCH_RADIUS);
            let r2 = radius * radius;
            let nn = normals[di];
            let mut vals: Vec<Vec3A> = Vec::new();
            let mut grid: Grid<(DVec3, DVec3, bool, u32)> = Grid { size: radius, cells: HashMap::new() };
            let stride = ((radius / sample_size(f)) / 4.0).floor().clamp(1.0, 4.0) as usize;
            let on = |x: usize, n: usize| x.is_multiple_of(stride) || x + 1 == n;
            for &ni in list {
                if before[ni].is_empty() {
                    continue;
                }
                let nf = &faces[ni];
                let (nw, nh) = (nf.w.max(1), nf.h);
                let disp_points = nf.disp.is_some().then(|| luxels_of(nf));
                for t in (0..nh).filter(|&t| on(t, nh)) {
                    for s in (0..nw).filter(|&s| on(s, nw)) {
                        let k = t * nw + s;
                        let (p, n) = match &disp_points {
                            Some(points) => match points.get(k) {
                                Some(pn) => *pn,
                                None => continue,
                            },
                            None => {
                                let c = nf.flat_sample(s, t).0;
                                let p = nf.plane_point(c.x, c.y);
                                (p, nf.phong(p))
                            }
                        };
                        let nbn = normals[ni];
                        let at = vals.len() as u32;
                        for pass in 0..passes {
                            for j in 0..nn {
                                vals.push(bounced(ni, pass, if nbn > 1 { j.min(nbn - 1) } else { 0 }, k));
                            }
                        }
                        let key = grid.key(p);
                        grid.cells.entry(key).or_default().push((p, n, nbn == 1, at));
                    }
                }
            }
            let lux = f.luxels();
            let width = passes * nn;
            let mut out = vec![Vec3A::ZERO; width * lux];
            let mut found = Vec::new();
            let mut acc = vec![Vec3A::ZERO; width];
            for (li, (lp, ln)) in luxels_of(f).iter().enumerate() {
                grid.near(*lp, &mut found);
                let mut weight = 0.0f64;
                acc.fill(Vec3A::ZERO);
                for &(pp, pn, flat, at) in found.iter().copied().flatten() {
                    let infl = 1.0 - pp.distance_squared(*lp) / r2;
                    if infl <= 0.0 {
                        continue;
                    }
                    let mut infl = infl * pn.dot(*ln).max(0.0);
                    if infl <= 0.0 {
                        continue;
                    }
                    if nn > 1 && flat {
                        infl *= 0.05;
                    }
                    let at = at as usize;
                    for (a, v) in acc.iter_mut().zip(&vals[at..at + width]) {
                        *a += *v * infl as f32;
                    }
                    weight += infl;
                }
                if weight > 0.0 {
                    for (j, a) in acc.iter().enumerate() {
                        out[j * lux + li] = *a / weight as f32;
                    }
                }
            }
            (di, out)
        })
        .collect();
    for (di, blend) in blended {
        let lux = faces[di].luxels();
        let fl = &mut light[di];
        let nn = fl.normals;
        for p in 0..passes {
            for n in 0..nn {
                let at = fl.index(p, 0, n, lux);
                let base = &before[di][(p * nn + n) * lux..(p * nn + n + 1) * lux];
                for k in 0..lux {
                    fl.values[at + k] = (Vec3A::from(base[k]) + blend[(p * nn + n) * lux + k]).into();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3A;

    use super::super::data::i32_at;
    use super::super::output::decode;
    use super::super::tests::pct;
    use super::super::{RadOptions, light_bsp};
    use crate::emit::lump;

    fn ratios(ours: &[u8], valve: &[u8], hdr: bool, disp: bool) -> Vec<f32> {
        let (fl, ll) = if hdr { (lump::FACES_HDR, lump::LIGHTING_HDR) } else { (lump::FACES, lump::LIGHTING) };
        let a = hp_bsp::Bsp::from_vec(ours.to_vec()).unwrap();
        let b = hp_bsp::Bsp::from_vec(valve.to_vec()).unwrap();
        let (fa, la) = (a.lump(fl).unwrap().into_owned(), a.lump(ll).unwrap().into_owned());
        let (fb, lb) = (b.lump(fl).unwrap().into_owned(), b.lump(ll).unwrap().into_owned());
        let mut out = Vec::new();
        for i in 0..fa.len().min(fb.len()) / 56 {
            let o = i * 56;
            let is_disp = i16::from_le_bytes([fa[o + 12], fa[o + 13]]) >= 0;
            let (oa, ob) = (i32_at(&fa, o + 20), i32_at(&fb, o + 20));
            if is_disp != disp || oa < 0 || ob < 0 || fa[o + 16] != 0 {
                continue;
            }
            let n = ((i32_at(&fa, o + 36) + 1) * (i32_at(&fa, o + 40) + 1)) as usize;
            let get = |l: &[u8], ofs: i32, j: usize| {
                let at = ofs as usize + j * 4;
                l.get(at..at + 4).map_or(Vec3A::ZERO, |c| decode([c[0], c[1], c[2], c[3]]))
            };
            for j in 0..n {
                let (x, y) = (get(&la, oa, j), get(&lb, ob, j));
                if y.max_element() > 2.0 {
                    out.push((x.x + x.y + x.z) / (y.x + y.y + y.z));
                }
            }
        }
        out.sort_by(f32::total_cmp);
        out
    }

    #[test]
    fn golden_displacements() {
        let path = "C:/Users/oracle/Desktop/gmodmapping/test_maps/.valve_ref/disp/disp_golden.bsp";
        let Ok(valve) = std::fs::read(path) else {
            eprintln!("{path} is not here: skipped");
            return;
        };
        let ours = light_bsp(&valve, &RadOptions::default(), &mut |_| {}).expect("light");
        for hdr in [false, true] {
            for disp in [true, false] {
                let r = ratios(&ours, &valve, hdr, disp);
                let (p10, median, p90) = (pct(&r, 0.1), pct(&r, 0.5), pct(&r, 0.9));
                eprintln!(
                    "{} {}: {} luxels, ours/VRAD p10 {p10:.3} median {median:.3} p90 {p90:.3}",
                    if hdr { "HDR" } else { "LDR" },
                    if disp { "displacements" } else { "faces" },
                    r.len()
                );
                assert!(r.len() > 1000);
                assert!((median - 1.0).abs() < 0.04 && p10 > 0.95 && p90 < 1.12, "p10 {p10} median {median} p90 {p90}");
            }
        }
    }
}
