use glam::DVec3;
use hp_geom::Aabb;
use hp_shape::{Contact, TriMesh};

use super::place_world::World;

pub(super) const STEP: f64 = 0.01;

#[derive(Clone, Debug, Default)]
pub(super) struct Settle {
    pub(super) delta: Option<f64>,
    pub(super) lift: f64,
    pub(super) contact: Option<(usize, Contact)>,
    pub(super) lifted_by: Option<usize>,
    pub(super) ignored: Vec<usize>,
}

pub(super) fn round_back(t: f64) -> f64 {
    let r = (t / STEP).floor() * STEP;
    if r == 0.0 { 0.0 } else { r }
}

fn extent_along(m: &TriMesh, d: DVec3) -> (f64, f64) {
    m.used_vertices().iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        let h = v.dot(d);
        (lo.min(h), hi.max(h))
    })
}

fn swept_box(b: &Aabb, d: DVec3, ahead: f64, back: f64) -> Aabb {
    let mut out = Aabb::new(b.min - d * back, b.max - d * back);
    out = out.union(&Aabb::new(b.min + d * ahead, b.max + d * ahead));
    out.union(b).expand(0.5)
}

pub(super) struct Env<'a> {
    pub(super) meshes: Vec<&'a TriMesh>,
    pub(super) owner: Vec<usize>,
}

pub(super) fn env_near<'a>(world: &'a World, region: &Aabb, skip: &[usize]) -> Env<'a> {
    let mut out = Env { meshes: Vec::new(), owner: Vec::new() };
    for (m, o) in world.near(region) {
        if skip.contains(&o) {
            continue;
        }
        out.meshes.push(m);
        out.owner.push(o);
    }
    out
}

fn ahead_only(env: Env<'_>, d: DVec3, lo: f64) -> Env<'_> {
    let mut out = Env { meshes: Vec::new(), owner: Vec::new() };
    for (m, o) in env.meshes.into_iter().zip(env.owner) {
        let b = m.bounds();
        let far = b.corners().iter().map(|c| c.dot(d)).fold(f64::NEG_INFINITY, f64::max);
        if far > lo + 1e-6 {
            out.meshes.push(m);
            out.owner.push(o);
        }
    }
    out
}

pub(super) fn settle(moving: &TriMesh, world: &World, dir: DVec3, max: f64, skip: &[usize], tol: f64) -> Settle {
    let mut out = Settle::default();
    let Some(d) = dir.try_normalize() else { return out };
    let b = moving.bounds();
    if b.is_empty() {
        return out;
    }
    let (lo, hi) = extent_along(moving, d);
    let extent = (hi - lo).max(0.0);
    let region = swept_box(&b, d, max, extent);
    let env = ahead_only(env_near(world, &region, skip), d, lo);
    if env.meshes.is_empty() {
        return out;
    }
    if let Some((depth, k)) = hp_shape::exit_depth(moving, DVec3::ZERO, &env.meshes, d, extent) {
        out.lift = depth;
        out.lifted_by = Some(env.owner[k]);
    }
    let start = -d * out.lift;
    let lifted = moving.translated(start);
    let mut ignored: Vec<usize> = Vec::new();
    for (m, o) in env.meshes.iter().zip(&env.owner) {
        if ignored.contains(o) {
            continue;
        }
        if let Some(ov) = hp_shape::overlap(&lifted, m, tol.max(1e-3)) {
            let h = ov.point.dot(d) + out.lift;
            if h < lo + extent * 0.75 {
                ignored.push(*o);
            }
        }
    }
    let kept: Vec<(&TriMesh, usize)> =
        env.meshes.iter().zip(&env.owner).filter(|(_, o)| !ignored.contains(o)).map(|(m, o)| (*m, *o)).collect();
    out.ignored = ignored;
    let meshes: Vec<&TriMesh> = kept.iter().map(|(m, _)| *m).collect();
    if let Some(c) = hp_shape::sweep(&lifted, DVec3::ZERO, &meshes, d, 0.0, max + out.lift) {
        out.delta = Some(c.t - out.lift);
        out.contact = Some((kept[c.env].1, c));
    }
    out
}

#[derive(Clone, Debug)]
pub(super) struct OverlapInfo {
    pub(super) owner: usize,
    pub(super) crossings: usize,
    pub(super) contained: bool,
    pub(super) depth: f64,
    pub(super) push: DVec3,
    pub(super) point: DVec3,
}

const AXES: [DVec3; 6] = [DVec3::Z, DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::NEG_Z];

pub(super) fn overlaps(moving: &TriMesh, world: &World, skip: &[usize], tol: f64) -> Vec<OverlapInfo> {
    overlaps_on(moving, world, skip, tol, AXES.len())
}

fn box_push(a: &Aabb, b: &Aabb, d: DVec3) -> f64 {
    let hi = b.corners().iter().map(|c| c.dot(d)).fold(f64::NEG_INFINITY, f64::max);
    let lo = a.corners().iter().map(|c| c.dot(d)).fold(f64::INFINITY, f64::min);
    hi - lo
}

pub(super) fn overlaps_on(moving: &TriMesh, world: &World, skip: &[usize], tol: f64, axes: usize) -> Vec<OverlapInfo> {
    let b = moving.bounds();
    if b.is_empty() {
        return Vec::new();
    }
    let env = env_near(world, &b.expand(tol), skip);
    let mut out: Vec<OverlapInfo> = Vec::new();
    for (m, o) in env.meshes.iter().zip(&env.owner) {
        let Some(ov) = hp_shape::overlap(moving, m, tol) else { continue };
        let mut best = (f64::INFINITY, DVec3::ZERO);
        let mb = m.bounds();
        let mut order: Vec<DVec3> = AXES.to_vec();
        if axes < AXES.len() {
            order.sort_by(|x, y| box_push(&b, &mb, *x).total_cmp(&box_push(&b, &mb, *y)));
            order.truncate(axes.max(1));
        }
        for axis in order {
            let s = hp_shape::separation(moving, m, axis);
            if s < best.0 - 1e-9 {
                best = (s, axis);
            }
        }
        match out.iter_mut().find(|x| x.owner == *o) {
            Some(x) => {
                x.crossings += ov.crossings;
                x.contained |= ov.contained;
                if best.0 > x.depth {
                    x.depth = best.0;
                    x.push = best.1 * best.0;
                }
            }
            None => out.push(OverlapInfo {
                owner: *o,
                crossings: ov.crossings,
                contained: ov.contained,
                depth: best.0,
                push: best.1 * best.0,
                point: ov.point,
            }),
        }
    }
    out.retain(|x| x.depth > tol || x.contained);
    out
}

pub(super) fn clearance(moving: &TriMesh, world: &World, dir: DVec3, max: f64, skip: &[usize]) -> Option<(f64, usize)> {
    let d = dir.try_normalize()?;
    let b = moving.bounds();
    let region = swept_box(&b, d, max, 0.0);
    let env = ahead_only(env_near(world, &region, skip), d, extent_along(moving, d).0);
    let c = hp_shape::sweep(moving, DVec3::ZERO, &env.meshes, d, 0.0, max)?;
    Some((c.t, env.owner[c.env]))
}

pub(super) fn round(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}
