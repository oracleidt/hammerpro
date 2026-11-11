use glam::{DVec2, DVec3};

use crate::grid::{Grid2, Stamp, bary, seg_cross};
use crate::mesh::TriMesh;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub e1: DVec3,
    pub e2: DVec3,
    pub d: DVec3,
}

impl Frame {
    pub fn new(dir: DVec3) -> Option<Self> {
        let d = dir.try_normalize().filter(|d| d.is_finite())?;
        let helper = if d.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
        let e1 = helper.cross(d).normalize();
        let e2 = d.cross(e1).normalize();
        Some(Self { e1, e2, d })
    }

    pub fn p2(&self, p: DVec3) -> DVec2 {
        DVec2::new(p.dot(self.e1), p.dot(self.e2))
    }

    pub fn h(&self, p: DVec3) -> f64 {
        p.dot(self.d)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactKind {
    VertexFace,
    FaceVertex,
    EdgeEdge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub t: f64,
    pub env: usize,
    pub tri: Option<u32>,
    pub point: DVec3,
    pub normal: DVec3,
    pub kind: ContactKind,
}

struct Projected {
    p: Vec<DVec2>,
    h: Vec<f64>,
}

fn project(frame: &Frame, verts: &[DVec3], offset: DVec3) -> Projected {
    let mut p = Vec::with_capacity(verts.len());
    let mut h = Vec::with_capacity(verts.len());
    for v in verts {
        let w = *v + offset;
        p.push(frame.p2(w));
        h.push(frame.h(w));
    }
    Projected { p, h }
}

fn used_mask(m: &TriMesh) -> Vec<bool> {
    let mut used = vec![false; m.verts.len()];
    for t in &m.tris {
        for &i in t {
            used[i as usize] = true;
        }
    }
    used
}

struct Moving {
    pr: Projected,
    used: Vec<bool>,
    lo: DVec2,
    hi: DVec2,
    hlo: f64,
    hhi: f64,
}

fn moving_info(frame: &Frame, m: &TriMesh, offset: DVec3) -> Option<Moving> {
    let pr = project(frame, &m.verts, offset);
    let used = used_mask(m);
    let (mut lo, mut hi) = (DVec2::INFINITY, DVec2::NEG_INFINITY);
    let (mut hlo, mut hhi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (i, u) in used.iter().enumerate() {
        if *u {
            lo = lo.min(pr.p[i]);
            hi = hi.max(pr.p[i]);
            hlo = hlo.min(pr.h[i]);
            hhi = hhi.max(pr.h[i]);
        }
    }
    lo.is_finite().then_some(Moving { pr, used, lo, hi, hlo, hhi })
}

struct EnvTri {
    env: usize,
    tri: u32,
    p: [DVec2; 3],
    h: [f64; 3],
}

fn env_candidates(
    frame: &Frame,
    env: &[&TriMesh],
    lo: DVec2,
    hi: DVec2,
    hlo: f64,
    hhi: f64,
) -> (Vec<EnvTri>, Vec<Projected>) {
    let mut tris = Vec::new();
    let mut projs = Vec::with_capacity(env.len());
    for (k, m) in env.iter().enumerate() {
        let pr = project(frame, &m.verts, DVec3::ZERO);
        for (ti, t) in m.tris.iter().enumerate() {
            let p = t.map(|i| pr.p[i as usize]);
            let h = t.map(|i| pr.h[i as usize]);
            let tlo = p[0].min(p[1]).min(p[2]);
            let thi = p[0].max(p[1]).max(p[2]);
            if thi.x < lo.x || thi.y < lo.y || tlo.x > hi.x || tlo.y > hi.y {
                continue;
            }
            let (a, b) = (h[0].min(h[1]).min(h[2]), h[0].max(h[1]).max(h[2]));
            if b < hlo || a > hhi {
                continue;
            }
            tris.push(EnvTri { env: k, tri: ti as u32, p, h });
        }
        projs.push(pr);
    }
    (tris, projs)
}

fn oriented_against(n: DVec3, d: DVec3) -> DVec3 {
    if n.dot(d) > 0.0 { -n } else { n }
}

pub fn sweep(moving: &TriMesh, offset: DVec3, env: &[&TriMesh], dir: DVec3, t_min: f64, t_max: f64) -> Option<Contact> {
    let frame = Frame::new(dir)?;
    let mv = moving_info(&frame, moving, offset)?;
    let pad = 1e-6;
    let (lo, hi) = (mv.lo - pad, mv.hi + pad);
    let (cands, projs) = env_candidates(&frame, env, lo, hi, mv.hlo + t_min - pad, mv.hhi + t_max + pad);
    if cands.is_empty() {
        return None;
    }
    let mut best: Option<Contact> = None;
    let mut consider = |c: Contact| {
        if c.t >= t_min && c.t <= t_max && best.is_none_or(|b| c.t < b.t) {
            best = Some(c);
        }
    };
    let mut grid = Grid2::new(lo, hi, cands.len());
    for (i, c) in cands.iter().enumerate() {
        grid.insert(i as u32, c.p[0].min(c.p[1]).min(c.p[2]), c.p[0].max(c.p[1]).max(c.p[2]));
    }
    for (vi, used) in mv.used.iter().enumerate() {
        if !used {
            continue;
        }
        let q = mv.pr.p[vi];
        for &ci in grid.at(q) {
            let c = &cands[ci as usize];
            let Some((u, v, w)) = bary(q, c.p[0], c.p[1], c.p[2]) else { continue };
            let he = u * c.h[0] + v * c.h[1] + w * c.h[2];
            let t = he - mv.pr.h[vi];
            let n = env[c.env].normals[c.tri as usize];
            consider(Contact {
                t,
                env: c.env,
                tri: Some(c.tri),
                point: moving.verts[vi] + offset + frame.d * t,
                normal: oriented_against(n, frame.d),
                kind: ContactKind::VertexFace,
            });
        }
    }
    let mut mgrid = Grid2::new(lo, hi, moving.tris.len());
    for (i, t) in moving.tris.iter().enumerate() {
        let p = t.map(|k| mv.pr.p[k as usize]);
        mgrid.insert(i as u32, p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
    }
    let mut seen_v: std::collections::HashSet<(usize, u32)> = std::collections::HashSet::new();
    let mut env_edges: Vec<(usize, u32, u32)> = Vec::new();
    for c in &cands {
        let t = env[c.env].tris[c.tri as usize];
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            env_edges.push((c.env, a.min(b), a.max(b)));
            if !seen_v.insert((c.env, t[k])) {
                continue;
            }
            let q = c.p[k];
            if q.x < lo.x || q.y < lo.y || q.x > hi.x || q.y > hi.y {
                continue;
            }
            let he = c.h[k];
            for &mi in mgrid.at(q) {
                let mt = moving.tris[mi as usize];
                let p = mt.map(|i| mv.pr.p[i as usize]);
                let Some((u, v, w)) = bary(q, p[0], p[1], p[2]) else { continue };
                let hm = u * mv.pr.h[mt[0] as usize] + v * mv.pr.h[mt[1] as usize] + w * mv.pr.h[mt[2] as usize];
                let tt = he - hm;
                let n = moving.normals[mi as usize];
                consider(Contact {
                    t: tt,
                    env: c.env,
                    tri: None,
                    point: env[c.env].verts[t[k] as usize],
                    normal: oriented_against(-n, frame.d),
                    kind: ContactKind::FaceVertex,
                });
            }
        }
    }
    env_edges.sort_unstable();
    env_edges.dedup();
    let mut egrid = Grid2::new(lo, hi, env_edges.len());
    for (i, &(k, a, b)) in env_edges.iter().enumerate() {
        let (pa, pb) = (projs[k].p[a as usize], projs[k].p[b as usize]);
        if (pb - pa).length_squared() < 1e-12 {
            continue;
        }
        egrid.insert(i as u32, pa.min(pb), pa.max(pb));
    }
    let mut stamp = Stamp::new(env_edges.len());
    for [a, b] in moving.edges() {
        let (pa, pb) = (mv.pr.p[a as usize], mv.pr.p[b as usize]);
        if (pb - pa).length_squared() < 1e-12 {
            continue;
        }
        let (ha, hb) = (mv.pr.h[a as usize], mv.pr.h[b as usize]);
        egrid.visit(pa.min(pb), pa.max(pb), &mut stamp, |ei| {
            let (k, c, d) = env_edges[ei as usize];
            let (pc, pd) = (projs[k].p[c as usize], projs[k].p[d as usize]);
            let Some((s, u)) = seg_cross(pa, pb, pc, pd) else { return };
            let hm = ha + (hb - ha) * s;
            let he = projs[k].h[c as usize] + (projs[k].h[d as usize] - projs[k].h[c as usize]) * u;
            let t = he - hm;
            if t < t_min || t > t_max {
                return;
            }
            let em = moving.verts[b as usize] - moving.verts[a as usize];
            let ee = env[k].verts[d as usize] - env[k].verts[c as usize];
            let n = em.cross(ee).try_normalize().unwrap_or(-frame.d);
            let point = env[k].verts[c as usize].lerp(env[k].verts[d as usize], u);
            consider(Contact {
                t,
                env: k,
                tri: None,
                point,
                normal: oriented_against(n, frame.d),
                kind: ContactKind::EdgeEdge,
            });
        });
    }
    best
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub t: f64,
    pub env: usize,
    pub tri: u32,
    pub point: DVec3,
    pub normal: DVec3,
}

pub fn ray_cast(env: &[&TriMesh], origin: DVec3, dir: DVec3, max: f64) -> Option<RayHit> {
    let d = dir.try_normalize()?;
    let ray = hp_geom::Ray::new(origin, d);
    let mut best: Option<RayHit> = None;
    for (k, m) in env.iter().enumerate() {
        let b = m.bounds();
        if b.is_empty() {
            continue;
        }
        let limit = best.map_or(max, |h| h.t);
        match b.expand(1e-3).ray_intersect(&ray) {
            Some(t) if t <= limit => {}
            _ if b.expand(1e-3).contains_point(origin) => {}
            _ => continue,
        }
        for i in 0..m.tris.len() {
            let [a, bb, c] = m.tri(i);
            let Some(t) = hp_geom::ray_triangle(&ray, a, bb, c, false) else { continue };
            if t >= 0.0 && t <= max && best.is_none_or(|h| t < h.t) {
                best = Some(RayHit { t, env: k, tri: i as u32, point: origin + d * t, normal: m.normals[i] });
            }
        }
    }
    best
}

pub fn exit_depth(
    moving: &TriMesh,
    offset: DVec3,
    env: &[&TriMesh],
    dir: DVec3,
    max_depth: f64,
) -> Option<(f64, usize)> {
    let frame = Frame::new(dir)?;
    let mv = moving_info(&frame, moving, offset)?;
    let center = (mv.hlo + mv.hhi) * 0.5;
    let pad = 1e-6;
    let (lo, hi) = (mv.lo - pad, mv.hi + pad);
    let (cands, _) = env_candidates(&frame, env, lo, hi, mv.hlo - max_depth, mv.hhi + max_depth);
    if cands.is_empty() {
        return None;
    }
    let mut grid = Grid2::new(lo, hi, cands.len());
    for (i, c) in cands.iter().enumerate() {
        grid.insert(i as u32, c.p[0].min(c.p[1]).min(c.p[2]), c.p[0].max(c.p[1]).max(c.p[2]));
    }
    let mut best: Option<(f64, usize)> = None;
    for (vi, used) in mv.used.iter().enumerate() {
        if !used || mv.pr.h[vi] < center {
            continue;
        }
        let q = mv.pr.p[vi];
        let mut first: Option<(f64, usize, u32)> = None;
        for &ci in grid.at(q) {
            let c = &cands[ci as usize];
            let Some((u, v, w)) = bary(q, c.p[0], c.p[1], c.p[2]) else { continue };
            let he = u * c.h[0] + v * c.h[1] + w * c.h[2];
            let s = mv.pr.h[vi] - he;
            if s > -1e-6 && first.is_none_or(|f| s < f.0) {
                first = Some((s, c.env, c.tri));
            }
        }
        let reach = max_depth.min(mv.pr.h[vi] - center);
        if let Some((s, k, tri)) = first
            && s > 1e-6
            && s <= reach
            && env[k].normals[tri as usize].dot(-frame.d) > 1e-6
            && best.is_none_or(|b| s > b.0)
        {
            best = Some((s, k));
        }
    }
    let mut mgrid = Grid2::new(lo, hi, moving.tris.len());
    for (i, t) in moving.tris.iter().enumerate() {
        let p = t.map(|k| mv.pr.p[k as usize]);
        mgrid.insert(i as u32, p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
    }
    let mut seen: std::collections::HashSet<(usize, u32)> = std::collections::HashSet::new();
    for c in &cands {
        let t = env[c.env].tris[c.tri as usize];
        for (k, &vk) in t.iter().enumerate() {
            if !seen.insert((c.env, vk)) || c.h[k] < center || c.h[k] > mv.hhi {
                continue;
            }
            let q = c.p[k];
            let mut first: Option<(f64, u32)> = None;
            for &mi in mgrid.at(q) {
                let mt = moving.tris[mi as usize];
                let p = mt.map(|i| mv.pr.p[i as usize]);
                let Some((u, v, w)) = bary(q, p[0], p[1], p[2]) else { continue };
                let hm = u * mv.pr.h[mt[0] as usize] + v * mv.pr.h[mt[1] as usize] + w * mv.pr.h[mt[2] as usize];
                let s = hm - c.h[k];
                if s > -1e-6 && first.is_none_or(|f| s < f.0) {
                    first = Some((s, mi));
                }
            }
            if let Some((s, mi)) = first
                && s > 1e-6
                && s <= max_depth
                && moving.normals[mi as usize].dot(frame.d) > 1e-6
                && best.is_none_or(|b| s > b.0)
            {
                best = Some((s, c.env));
            }
        }
    }
    best.filter(|b| b.0 > 1e-6)
}
