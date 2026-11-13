use ahash::AHashMap as HashMap;

use glam::{DVec2, DVec3, Vec3A};
use rayon::prelude::*;

use super::bake::{Ctx, FaceLight};
use super::surface::LitFace;

const MAX_SAMPLE_RADIUS: f64 = 512.0;
const MAX_PATCH_RADIUS: f64 = 1500.0;
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct Patch {
    pub origin: DVec3,
    pub normal: DVec3,
    uv: [DVec2; 4],
    corners: usize,
    mins: DVec3,
    maxs: DVec3,
    area: f64,
}

impl Patch {
    fn contains(&self, uv: DVec2) -> bool {
        let tri = |a: DVec2, b: DVec2, c: DVec2| {
            let cross = |o: DVec2, p: DVec2, q: DVec2| (p - o).perp_dot(q - o);
            let (d0, d1, d2) = (cross(a, b, uv), cross(b, c, uv), cross(c, a, uv));
            let eps = 1e-9;
            (d0 >= -eps && d1 >= -eps && d2 >= -eps) || (d0 <= eps && d1 <= eps && d2 <= eps)
        };
        match self.corners {
            3 => tri(self.uv[0], self.uv[1], self.uv[2]),
            _ => tri(self.uv[0], self.uv[1], self.uv[2]) || tri(self.uv[0], self.uv[2], self.uv[3]),
        }
    }

    fn center_uv(&self) -> DVec2 {
        self.uv[..self.corners].iter().copied().sum::<DVec2>() / self.corners as f64
    }
}

struct PatchMaker<'a> {
    f: &'a LitFace,
    n: usize,
    min_edge: f64,
    levels: usize,
    out: Vec<Patch>,
}

impl PatchMaker<'_> {
    fn point(&self, i: usize) -> (DVec3, DVec2) {
        let d = self.f.disp.as_ref().expect("a displacement");
        let last = (self.n - 1).max(1) as f64;
        (
            d.pos.get(i).copied().unwrap_or(DVec3::ZERO),
            DVec2::new((i % self.n) as f64 / last, (i / self.n) as f64 / last),
        )
    }

    fn push(&mut self, pts: &[(DVec3, DVec2)]) {
        let (p0, p1) = (pts[0].0, pts[1].0);
        let other = if pts.len() == 4 { pts[3].0 } else { pts[2].0 };
        let mut normal = (other - p0).cross(p1 - p0).normalize_or_zero();
        if normal == DVec3::ZERO {
            normal = self.f.normal;
        } else if normal.dot(self.f.normal) < 0.0 {
            normal = -normal;
        }
        let origin = pts.iter().map(|p| p.0).sum::<DVec3>() / pts.len() as f64;
        let mut uv = [DVec2::ZERO; 4];
        for (k, p) in pts.iter().enumerate() {
            uv[k] = p.1;
        }
        let mins = pts.iter().fold(DVec3::splat(f64::INFINITY), |a, p| a.min(p.0));
        let maxs = pts.iter().fold(DVec3::splat(f64::NEG_INFINITY), |a, p| a.max(p.0));
        let tri = |a: DVec3, b: DVec3, c: DVec3| 0.5 * (b - a).cross(c - a).length();
        let mut area = tri(pts[0].0, pts[1].0, pts[2].0);
        if pts.len() == 4 {
            area += tri(pts[0].0, pts[2].0, pts[3].0);
        }
        self.out.push(Patch { origin, normal, uv, corners: pts.len(), mins, maxs, area });
    }

    fn too_small(&self, p: [DVec3; 3]) -> bool {
        let (e0, e1, e2) = (p[1] - p[0], p[2] - p[1], p[0] - p[2]);
        let longest = e0.length().max(e1.length()).max(e2.length());
        longest < self.min_edge || 0.5 * e1.cross(e0).length() < 0.5 * self.min_edge * self.min_edge
    }

    fn root(&mut self) {
        let n = self.n;
        let q = [0, n * (n - 1), n * n - 1, n - 1].map(|i| self.point(i));
        let (e0, e1, e2, e3) = (q[1].0 - q[0].0, q[2].0 - q[1].0, q[3].0 - q[2].0, q[3].0 - q[0].0);
        let longest = e0.length().max(e1.length()).max(e2.length()).max(e3.length());
        if longest < self.min_edge || e3.cross(e0).length() < self.min_edge * self.min_edge {
            self.push(&q);
            return;
        }
        self.split([n * n - 1, 0, n * (n - 1)], 0);
        self.split([0, n * n - 1, n - 1], 0);
    }

    fn split(&mut self, idx: [usize; 3], level: usize) {
        let pts = idx.map(|i| self.point(i));
        if self.too_small(pts.map(|p| p.0)) {
            self.push(&pts);
            return;
        }
        if level >= self.levels {
            self.halve(pts, 0);
            return;
        }
        let mid = (idx[0] + idx[1]) / 2;
        self.split([idx[2], idx[0], mid], level + 1);
        self.split([idx[1], idx[2], mid], level + 1);
    }

    fn halve(&mut self, pts: [(DVec3, DVec2); 3], depth: usize) {
        if depth >= 24 || self.too_small(pts.map(|p| p.0)) {
            self.push(&pts);
            return;
        }
        let len = |a: usize, b: usize| pts[a].0.distance(pts[b].0);
        let (l0, l1, l2) = (len(0, 1), len(1, 2), len(2, 0));
        let mid = |a: usize, b: usize| ((pts[a].0 + pts[b].0) * 0.5, (pts[a].1 + pts[b].1) * 0.5);
        let [p0, p1, p2] = pts;
        let (a, b) = if l0 >= l1 && l0 >= l2 {
            let m = mid(0, 1);
            ([p0, m, p2], [m, p1, p2])
        } else if l1 >= l2 {
            let m = mid(1, 2);
            ([p0, p1, m], [m, p2, p0])
        } else {
            let m = mid(2, 0);
            ([p0, p1, m], [m, p1, p2])
        };
        self.halve(a, depth + 1);
        self.halve(b, depth + 1);
    }
}

pub(crate) fn chop_or_default(chop: f32) -> f64 {
    if chop.is_finite() && chop >= 1.0 { f64::from(chop) } else { 8.0 }
}

pub(crate) fn patches(f: &LitFace, chop: f64) -> Vec<Patch> {
    let Some(d) = &f.disp else { return Vec::new() };
    let n = d.n;
    if n < 2 || d.pos.len() < n * n {
        return Vec::new();
    }
    let size = sample_size(f);
    let size = if size.is_finite() && size > 0.0 { size } else { 16.0 };
    let power = (n - 1).trailing_zeros() as usize;
    let mut maker = PatchMaker { f, n, min_edge: size * chop, levels: 2 * power, out: Vec::new() };
    maker.root();
    maker.out
}

pub(crate) fn patch_index(list: &[Patch], uv: DVec2) -> Option<usize> {
    list.iter().position(|p| p.contains(uv)).or_else(|| {
        (0..list.len()).min_by(|&a, &b| {
            list[a].center_uv().distance_squared(uv).total_cmp(&list[b].center_uv().distance_squared(uv))
        })
    })
}

pub(crate) fn patch_at(list: &[Patch], uv: DVec2) -> Option<&Patch> {
    patch_index(list, uv).and_then(|i| list.get(i))
}

pub(crate) fn luxel_patches(f: &LitFace, list: &[Patch]) -> (Vec<u32>, Vec<f32>) {
    let (w, h) = (f.w.max(1), f.h.max(1));
    let step = |i: usize, n: usize| if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
    let edge = |i: usize, n: usize| if n > 1 && (i == 0 || i + 1 == n) { 0.5 } else { 1.0 };
    let mut of_luxel = Vec::with_capacity(w * h);
    let mut weight = vec![0.0f64; list.len()];
    for t in 0..h {
        for s in 0..w {
            let k = patch_index(list, DVec2::new(step(s, w), step(t, h))).unwrap_or(0);
            of_luxel.push(k as u32);
            if let Some(wt) = weight.get_mut(k) {
                *wt += edge(s, w) * edge(t, h);
            }
        }
    }
    let total_area: f64 = list.iter().map(|p| p.area).sum();
    let total_weight: f64 = weight.iter().sum();
    let scale = list
        .iter()
        .zip(&weight)
        .map(|(p, &wt)| {
            if wt > 0.0 && total_area > 0.0 && total_weight > 0.0 {
                ((p.area / total_area) / (wt / total_weight)) as f32
            } else {
                1.0
            }
        })
        .collect();
    (of_luxel, scale)
}

pub(crate) fn patch_members(f: &LitFace, list: &[Patch]) -> Vec<Vec<u32>> {
    let points = luxels_of(f);
    let reach = sample_size(f) * 0.5;
    let reach = if reach.is_finite() { reach } else { 8.0 };
    list.iter()
        .map(|p| {
            let (lo, hi) = (p.mins - DVec3::splat(reach), p.maxs + DVec3::splat(reach));
            points
                .iter()
                .enumerate()
                .filter(|(_, (q, _))| q.cmpge(lo).all() && q.cmple(hi).all())
                .map(|(k, _)| k as u32)
                .collect()
        })
        .collect()
}

pub(crate) fn root_plane(f: &LitFace) -> Option<(DVec3, f64)> {
    let d = f.disp.as_ref()?;
    let n = d.n;
    let at = |i: usize| d.pos.get(i).copied();
    let (p0, p1, p3) = (at(0)?, at(n * (n - 1))?, at(n - 1)?);
    let mut normal = (p3 - p0).cross(p1 - p0).try_normalize()?;
    if normal.dot(f.normal) < 0.0 {
        normal = -normal;
    }
    Some((normal, normal.dot(p0)))
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
            let radius = (sample_size(f) * ctx.dispchop * 2.2).min(MAX_PATCH_RADIUS);
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
                let patch_list = patches(nf, ctx.dispchop);
                for t in (0..nh).filter(|&t| on(t, nh)) {
                    for s in (0..nw).filter(|&s| on(s, nw)) {
                        let k = t * nw + s;
                        let (p, n) = match &disp_points {
                            Some(points) => match points.get(k) {
                                Some(&(p, n)) => {
                                    let step = |i: usize, n: usize| if n > 1 { i as f64 / (n - 1) as f64 } else { 0.0 };
                                    let uv = DVec2::new(step(s, nw), step(t, nh));
                                    (p, patch_at(&patch_list, uv).map_or(n, |patch| patch.normal))
                                }
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

    fn disp_face(power: u32, size: f64, height: impl Fn(f64, f64) -> f64) -> super::LitFace {
        use glam::{DVec2, DVec3};
        let n = (1usize << power) + 1;
        let mut pos = Vec::new();
        for row in 0..n {
            for col in 0..n {
                let (u, v) = (col as f64 / (n - 1) as f64, row as f64 / (n - 1) as f64);
                pos.push(DVec3::new(u * size, v * size, height(u, v)));
            }
        }
        let poly =
            vec![DVec3::ZERO, DVec3::new(0.0, size, 0.0), DVec3::new(size, size, 0.0), DVec3::new(size, 0.0, 0.0)];
        let w = (size / 16.0) as usize + 1;
        super::LitFace {
            face: 0,
            w,
            h: w,
            bump: false,
            normal: DVec3::Z,
            tex: [DVec3::X, DVec3::Y],
            frame: Some((DVec3::ZERO, DVec3::X * 16.0, DVec3::Y * 16.0)),
            mins: [0, 0],
            poly2: poly.iter().map(|p| DVec2::new(p.x / 16.0, p.y / 16.0)).collect(),
            bounds: (DVec3::ZERO, DVec3::new(size, size, 0.0)),
            poly,
            disp: Some(super::super::surface::DispSurf { n, pos, normals: vec![DVec3::Z; n * n] }),
            reflectivity: Vec3A::splat(0.5),
            minlight: 0.0,
            lm: [[1.0 / 16.0, 0.0, 0.0, 0.0], [0.0, 1.0 / 16.0, 0.0, 0.0]],
            offset: DVec3::ZERO,
            vnormals: None,
        }
    }

    fn uv_area(p: &super::Patch) -> f64 {
        let tri = |a: glam::DVec2, b: glam::DVec2, c: glam::DVec2| 0.5 * (b - a).perp_dot(c - a).abs();
        let mut a = tri(p.uv[0], p.uv[1], p.uv[2]);
        if p.corners == 4 {
            a += tri(p.uv[0], p.uv[2], p.uv[3]);
        }
        a
    }

    #[test]
    fn displacement_patches_follow_vrads_dispchop_split() {
        let f = disp_face(3, 512.0, |_, _| 0.0);
        let list = super::patches(&f, 8.0);
        assert!(
            list.len() >= 16,
            "a 512 unit displacement at 16 units a luxel splits into patches of about 128 units: {}",
            list.len()
        );
        let total: f64 = list.iter().map(uv_area).sum();
        assert!((total - 1.0).abs() < 1e-9, "the patches cover the displacement once: {total}");
        for p in &list {
            assert!((p.normal - glam::DVec3::Z).length() < 1e-9 && p.origin.z.abs() < 1e-9);
            let pts: Vec<glam::DVec3> =
                p.uv[..p.corners].iter().map(|uv| glam::DVec3::new(uv.x * 512.0, uv.y * 512.0, 0.0)).collect();
            let longest = (0..p.corners).map(|k| pts[k].distance(pts[(k + 1) % p.corners])).fold(0.0, f64::max);
            assert!(longest < 260.0, "no patch is much bigger than the chop size: {longest}");
        }
        for k in 0..50 {
            let uv = glam::DVec2::new((k as f64 * 0.37).fract(), (k as f64 * 0.61).fract());
            assert!(super::patch_at(&list, uv).is_some_and(|p| p.contains(uv)));
        }
        let small = disp_face(2, 64.0, |_, _| 0.0);
        assert_eq!(super::patches(&small, 8.0).len(), 1, "a displacement smaller than the chop size is one patch");
        let (normal, dist) = super::root_plane(&disp_face(2, 256.0, |_, _| 12.0)).expect("a plane");
        assert!((normal - glam::DVec3::Z).length() < 1e-9 && (dist - 12.0).abs() < 1e-9);
    }

    #[test]
    fn patches_of_a_dome_sit_under_its_surface_like_vrads() {
        let dome = |u: f64, v: f64| 200.0 * (std::f64::consts::PI * u).sin() * (std::f64::consts::PI * v).sin();
        let f = disp_face(3, 512.0, dome);
        let list = super::patches(&f, 8.0);
        let under = list.iter().filter(|p| {
            let c = p.center_uv();
            p.origin.z + 1.0 < dome(c.x, c.y) - 1.0
        });
        assert!(under.count() * 2 >= list.len(), "the flat patches of a round surface are inside it");
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
