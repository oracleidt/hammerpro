use glam::{DVec3, Vec3A};
use rayon::prelude::*;

use super::bake::{Ctx, SurfHit, hash, sphere_dirs, unit};
use super::data::{CONTENTS_SOLID, MapData};
use super::lights::{Kind, Light};
use super::output::encode;
use super::surface::SurfTri;

const MAX_SAMPLES: usize = 16;
const SCREEN_DIRS: usize = 40;
const TRY_BUDGET: u32 = 8000;
const BOX: [Vec3A; 6] = [Vec3A::X, Vec3A::NEG_X, Vec3A::Y, Vec3A::NEG_Y, Vec3A::Z, Vec3A::NEG_Z];

#[derive(Clone, Debug)]
struct AmbientSample {
    pos: DVec3,
    cube: [Vec3A; 6],
}

pub(crate) struct FaceColors {
    pub luxels: Vec<Vec<Vec<Vec3A>>>,
    pub average: Vec<Vec<Vec3A>>,
}

fn leaf_planes(map: &MapData, leaf: usize, leaf_parent: &[i32], node_parent: &[i32]) -> Vec<(DVec3, f64)> {
    let mut out = Vec::new();
    let mut child = -1 - leaf as i32;
    let mut node = leaf_parent.get(leaf).copied().unwrap_or(-1);
    let mut steps = 0;
    while node >= 0 && steps <= map.nodes.len() {
        steps += 1;
        let Some(n) = map.nodes.get(node as usize) else { break };
        let Some(p) = map.planes.get(n.plane) else { break };
        if n.children[0] == child {
            out.push((p.normal, p.dist));
        } else {
            out.push((-p.normal, -p.dist));
        }
        child = node;
        node = node_parent.get(node as usize).copied().unwrap_or(-1);
    }
    out
}

fn screen(v: f32) -> i32 {
    let i = ((v * 1023.0) as i32).clamp(0, 1023);
    (255.0 * (i as f32 / 1023.0).powf(1.0 / 2.2)) as i32
}

fn color_at(list: &[AmbientSample], pos: DVec3, skip: usize) -> [Vec3A; 6] {
    let mut out = [Vec3A::ZERO; 6];
    let mut total = 0.0f32;
    for (i, s) in list.iter().enumerate() {
        if i == skip {
            continue;
        }
        let f = 1.0 / ((s.pos - pos).length_squared() as f32 + 1.0);
        total += f;
        for (o, c) in out.iter_mut().zip(&s.cube) {
            *o += *c * f;
        }
    }
    if total > 0.0 {
        for c in &mut out {
            *c /= total;
        }
    }
    out
}

fn add_sample(list: &mut Vec<AmbientSample>, s: AmbientSample) {
    list.push(s);
    if list.len() <= MAX_SAMPLES {
        return;
    }
    let (mut nearest, mut nearest_dist, mut nearest_total) = (0usize, f32::MAX, 0.0f32);
    for i in 0..list.len() {
        let (mut closest, mut total) = (f32::MAX, 0.0f32);
        for j in 0..list.len() {
            if i == j {
                continue;
            }
            let mut dist = (list[i].pos - list[j].pos).length() as f32;
            let mut max_dc = 0.0f32;
            for k in 0..6 {
                max_dc = max_dc.max((list[i].cube[k] - list[j].cube[k]).abs().max_element());
                total += max_dc;
            }
            max_dc = if max_dc < 1e-4 { 0.0 } else { max_dc.min(1.0) };
            dist *= 0.1 + max_dc * 0.9;
            closest = closest.min(dist);
        }
        if closest < nearest_dist || (closest == nearest_dist && total < nearest_total) {
            nearest_dist = closest;
            nearest = i;
            nearest_total = total;
        }
    }
    list.swap_remove(nearest);
}

fn compress(list: &mut Vec<AmbientSample>) {
    let mut i = 0;
    while i < list.len() {
        if list.len() > 1 {
            let test = color_at(list, list[i].pos, i);
            let delta = (0..6)
                .flat_map(|k| (0..3).map(move |c| (k, c)))
                .map(|(k, c)| (screen(test[k][c]) - screen(list[i].cube[k][c])).abs())
                .max()
                .unwrap_or(0);
            if delta < 3 {
                list.swap_remove(i);
                continue;
            }
        }
        i += 1;
    }
}

fn fraction(v: f64, lo: f64, hi: f64) -> u8 {
    if hi <= lo {
        return 0;
    }
    ((v - lo) / (hi - lo) * 255.0).clamp(0.0, 255.0).round() as u8
}

pub(crate) fn leaf_ambient(ctx: &Ctx<'_>, colors: &FaceColors) -> Vec<(Vec<u8>, Vec<u8>)> {
    let map = ctx.map;
    let passes = ctx.passes;
    let mut leaf_parent = vec![-1i32; map.leafs.len()];
    let mut node_parent = vec![-1i32; map.nodes.len()];
    for (i, n) in map.nodes.iter().enumerate() {
        for c in n.children {
            if c >= 0 {
                if let Some(p) = node_parent.get_mut(c as usize) {
                    *p = i as i32;
                }
            } else if let Some(p) = leaf_parent.get_mut((-1 - c) as usize) {
                *p = i as i32;
            }
        }
    }
    let dirs: Vec<Vec3A> = if ctx.params.leaf_dirs == super::detail::ANORMS.len() {
        super::detail::ANORMS.iter().map(|n| Vec3A::from_array(*n)).collect()
    } else {
        sphere_dirs(ctx.params.leaf_dirs)
    };
    let sky: Vec<Vec3A> = (0..passes)
        .map(|p| {
            ctx.lights
                .list
                .iter()
                .find(|l| l.kind == Kind::SkyAmbient)
                .and_then(|l| l.intensity.get(p))
                .copied()
                .unwrap_or(Vec3A::ZERO)
                / 255.0
        })
        .collect();
    let tan_theta = 7.275f32.to_radians().tan();
    let in_cube: Vec<Vec<&Light>> =
        (0..passes).map(|p| ctx.lights.list.iter().filter(|l| l.in_ambient_cube(p)).collect()).collect();
    let screen = sphere_dirs(SCREEN_DIRS);
    let cubes_at = |pos: DVec3, set: &[Vec3A]| -> Vec<[Vec3A; 6]> {
        let from = pos.as_vec3a();
        let mut rad = vec![Vec3A::ZERO; passes * set.len()];
        for (k, d) in set.iter().enumerate() {
            match ctx.trace_surface(from, *d) {
                SurfHit::Sky => {
                    for p in 0..passes {
                        rad[p * set.len() + k] = sky[p];
                    }
                }
                SurfHit::Face(fi, s, t, dist) => {
                    let Some(f) = ctx.faces.get(fi as usize) else { continue };
                    let scale_avg = ((dist * tan_theta - 20.0) / 20.0).clamp(0.0, 1.0);
                    let (x, y) = ((s.round() as usize).min(f.w - 1), (t.round() as usize).min(f.h - 1));
                    for p in 0..passes {
                        let avg = colors.average[fi as usize][p];
                        let point = colors.luxels[fi as usize][p].get(y * f.w + x).copied().unwrap_or(avg);
                        rad[p * set.len() + k] = (avg * scale_avg + point * (1.0 - scale_avg)) / 255.0 * f.reflectivity;
                    }
                }
                SurfHit::None => {}
            }
        }
        (0..passes)
            .map(|p| {
                let mut cube = [Vec3A::ZERO; 6];
                for (j, b) in BOX.iter().enumerate() {
                    let mut t = 0.0f32;
                    for (k, d) in set.iter().enumerate() {
                        let c = d.dot(*b);
                        if c > 0.0 {
                            t += c;
                            cube[j] += rad[p * set.len() + k] * c;
                        }
                    }
                    if t > 0.0 {
                        cube[j] /= t;
                    }
                }
                add_emit_surface(ctx, from, p, &in_cube[p], &mut cube);
                cube
            })
            .collect()
    };
    let mut lists: Vec<Vec<Vec<AmbientSample>>> = (0..map.leafs.len())
        .into_par_iter()
        .map(|li| {
            let leaf = &map.leafs[li];
            let mut per_pass = vec![Vec::new(); passes];
            if leaf.contents & CONTENTS_SOLID != 0 || leaf_parent[li] < 0 {
                return per_pass;
            }
            let planes = leaf_planes(map, li, &leaf_parent, &node_parent);
            let (lo, hi) = (DVec3::from_array(leaf.mins), DVec3::from_array(leaf.maxs));
            let size = hi - lo;
            let x = ((size.x / 32.0) as usize).max(1);
            let count = (x * x * x).clamp(1, 128).min(ctx.params.leaf_samples.max(1));
            let screening = count > 1 && dirs.len() > 2 * screen.len();
            let mut budget = TRY_BUDGET;
            let mut used_center = false;
            for i in 0..count {
                let mut pos = None;
                let mut covered = false;
                for tries in 0..1000u32 {
                    if budget == 0 {
                        break;
                    }
                    budget -= 1;
                    let r = |c: u32| f64::from(unit(hash(li as u32, i as u32 * 4 + c, tries)));
                    let p = lo + size * DVec3::new(r(0), r(1), r(2));
                    if planes.iter().all(|(n, d)| p.dot(*n) - d >= 0.031_25) {
                        if ctx.in_brush(p) || buried(ctx, p.as_vec3a()) {
                            covered = true;
                            continue;
                        }
                        pos = Some(p);
                        break;
                    }
                }
                let pos = match pos {
                    Some(p) => p,
                    None if !covered
                        && !used_center
                        && !ctx.in_brush((lo + hi) * 0.5)
                        && !buried(ctx, ((lo + hi) * 0.5).as_vec3a()) =>
                    {
                        used_center = true;
                        (lo + hi) * 0.5
                    }
                    None if i == 0 || budget == 0 || used_center => break,
                    None => continue,
                };
                let set = if screening { &screen } else { &dirs };
                for (list, cube) in per_pass.iter_mut().zip(cubes_at(pos, set)) {
                    add_sample(list, AmbientSample { pos, cube });
                }
            }
            if screening {
                let mut done: Vec<([u64; 3], Vec<[Vec3A; 6]>)> = Vec::new();
                for (p, list) in per_pass.iter_mut().enumerate() {
                    for s in list.iter_mut() {
                        let key = s.pos.to_array().map(f64::to_bits);
                        let at = match done.iter().position(|(k, _)| *k == key) {
                            Some(at) => at,
                            None => {
                                done.push((key, cubes_at(s.pos, &dirs)));
                                done.len() - 1
                            }
                        };
                        s.cube = done[at].1[p];
                    }
                }
            }
            for list in &mut per_pass {
                compress(list);
            }
            per_pass
        })
        .inspect(|_| ctx.work.add(1))
        .collect();
    for p in 0..passes {
        let mut cap = MAX_SAMPLES;
        while lists.iter().map(|l| l[p].len()).sum::<usize>() > 65535 && cap > 1 {
            cap -= 1;
            for l in &mut lists {
                l[p].truncate(cap);
            }
        }
    }
    (0..passes)
        .map(|p| {
            let mut index = Vec::with_capacity(map.leafs.len() * 4);
            let mut lighting = Vec::new();
            let mut first = 0usize;
            let has: Vec<bool> = lists.iter().map(|l| !l[p].is_empty()).collect();
            for (li, leaf) in map.leafs.iter().enumerate() {
                let list = &lists[li][p];
                if list.is_empty() || first > 65535 {
                    let reference = nearest_with_light(map, li, &has);
                    index.extend_from_slice(&0u16.to_le_bytes());
                    index.extend_from_slice(&(reference.min(65535) as u16).to_le_bytes());
                    continue;
                }
                index.extend_from_slice(&(list.len() as u16).to_le_bytes());
                index.extend_from_slice(&(first as u16).to_le_bytes());
                for s in list {
                    for c in &s.cube {
                        lighting.extend_from_slice(&encode(*c));
                    }
                    lighting.push(fraction(s.pos.x, leaf.mins[0], leaf.maxs[0]));
                    lighting.push(fraction(s.pos.y, leaf.mins[1], leaf.maxs[1]));
                    lighting.push(fraction(s.pos.z, leaf.mins[2], leaf.maxs[2]));
                    lighting.push(0);
                }
                first += list.len();
            }
            (index, lighting)
        })
        .collect()
}

fn buried(ctx: &Ctx<'_>, p: Vec3A) -> bool {
    let k = 0.577_350_3;
    let dirs = [
        Vec3A::X,
        Vec3A::NEG_X,
        Vec3A::Y,
        Vec3A::NEG_Y,
        Vec3A::Z,
        Vec3A::NEG_Z,
        Vec3A::new(k, k, k),
        Vec3A::new(-k, k, k),
        Vec3A::new(k, -k, k),
        Vec3A::new(-k, -k, k),
        Vec3A::new(k, k, -k),
        Vec3A::new(-k, k, -k),
        Vec3A::new(k, -k, -k),
        Vec3A::new(-k, -k, -k),
    ];
    let back = dirs
        .iter()
        .filter(|d| {
            let Some(h) = ctx.surf.closest(p, **d, super::bake::MAX_TRACE, |_| true) else { return false };
            match ctx.surf_info.get(h.payload as usize) {
                Some(SurfTri::Flat(i) | SurfTri::Disp(i, _)) => {
                    ctx.faces.get(*i as usize).is_some_and(|f| d.dot(f.normal.as_vec3a()) > 0.0)
                }
                _ => false,
            }
        })
        .count();
    back * 2 > dirs.len()
}

fn add_emit_surface(ctx: &Ctx<'_>, from: Vec3A, pass: usize, lights: &[&Light], cube: &mut [Vec3A; 6]) {
    for l in lights {
        let delta = l.origin - from;
        let len = delta.length();
        if len.is_nan() || len <= 1e-4 {
            continue;
        }
        let dir = delta / len;
        let dot2 = -dir.dot(l.normal);
        if dot2 <= 0.01 {
            continue;
        }
        let ratio = dot2 / (len * len).max(1.0);
        let intensity = l.intensity.get(pass).copied().unwrap_or(Vec3A::ZERO) / 255.0;
        if intensity.max_element() * ratio < 1.0e-6 {
            continue;
        }
        if ctx.shadow.occluded(from, dir, len - 0.01, |_| true) {
            continue;
        }
        for (c, b) in cube.iter_mut().zip(BOX) {
            let t = b.dot(dir);
            if t > 0.0 {
                *c += intensity * (t * ratio);
            }
        }
    }
}

fn nearest_with_light(map: &MapData, leaf: usize, has: &[bool]) -> usize {
    let Some(l) = map.leafs.get(leaf) else { return leaf };
    let (lo, hi) = (DVec3::from_array(l.mins), DVec3::from_array(l.maxs));
    let size = hi - lo;
    let (qlo, qhi) = (lo - size, hi + size);
    let head = map.models.first().map_or(-1, |m| m.head);
    let mut best = (f64::MAX, leaf);
    let mut stack = vec![head];
    let mut steps = 0usize;
    while let Some(n) = stack.pop() {
        steps += 1;
        if steps > 4 * (map.nodes.len() + map.leafs.len()) + 4 {
            break;
        }
        if n < 0 {
            let li = (-1 - n) as usize;
            if !has.get(li).copied().unwrap_or(false) {
                continue;
            }
            let Some(o) = map.leafs.get(li) else { continue };
            let (olo, ohi) = (DVec3::from_array(o.mins), DVec3::from_array(o.maxs));
            let gap = (lo.max(olo) - hi.min(ohi)).max(DVec3::ZERO);
            let d = gap.length();
            if d < best.0 {
                best = (d, li);
            }
            continue;
        }
        let Some(node) = map.nodes.get(n as usize) else { continue };
        let Some(p) = map.planes.get(node.plane) else { continue };
        let c = (qlo + qhi) * 0.5;
        let e = (qhi - qlo) * 0.5;
        let r = e.dot(p.normal.abs());
        let d = c.dot(p.normal) - p.dist;
        if d > -r {
            stack.push(node.children[0]);
        }
        if d < r {
            stack.push(node.children[1]);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_lists() {
        let mut list = Vec::new();
        for i in 0..40 {
            add_sample(
                &mut list,
                AmbientSample { pos: DVec3::new(i as f64 * 10.0, 0.0, 0.0), cube: [Vec3A::splat(0.2); 6] },
            );
        }
        assert_eq!(list.len(), MAX_SAMPLES);
        compress(&mut list);
        assert_eq!(list.len(), 1);
        assert_eq!(fraction(5.0, 0.0, 10.0), 128);
        assert_eq!(fraction(5.0, 10.0, 10.0), 0);
        assert_eq!(screen(1.0), 255);
        assert_eq!(screen(0.0), 0);
    }
}
