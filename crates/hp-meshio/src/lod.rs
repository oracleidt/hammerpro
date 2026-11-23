use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use glam::{DVec2, DVec3};

use crate::Triangle;

type CollapsePlan = (Vec<(u32, Corner)>, Vec<Wedge>);
type SimplifiedLods = (Vec<Wedge>, Vec<Vec<([u32; 3], u32)>>);

pub const MIN_LEVELS: usize = 3;
pub const MAX_LEVELS: usize = 6;
pub const DEFAULT_LEVELS: usize = 4;
pub const DEFAULT_REDUCTION: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodOptions {
    pub enabled: bool,
    pub levels: usize,
    pub reduction: f64,
    pub distances: [Option<f64>; MAX_LEVELS],
}

impl Default for LodOptions {
    fn default() -> Self {
        Self { enabled: true, levels: DEFAULT_LEVELS, reduction: DEFAULT_REDUCTION, distances: [None; MAX_LEVELS] }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodLevel {
    pub fraction: f64,
    pub switch: f64,
}

const SMALL_MODEL: usize = 300;
const SMALL_REDUCTION: f64 = 0.75;

pub fn plan(opts: &LodOptions, triangles: usize, radius: f64) -> Vec<LodLevel> {
    if !opts.enabled || triangles == 0 {
        return Vec::new();
    }
    let mut levels = opts.levels.clamp(MIN_LEVELS, MAX_LEVELS);
    if triangles > 20_000 {
        levels += 1;
    }
    if triangles > 80_000 {
        levels += 1;
    }
    let levels = levels.min(MAX_LEVELS);
    let mut reduction = if opts.reduction.is_finite() { opts.reduction.clamp(0.2, 0.9) } else { DEFAULT_REDUCTION };
    if triangles < SMALL_MODEL {
        reduction = reduction.max(SMALL_REDUCTION);
    }
    let radius = if radius.is_finite() { radius.max(8.0) } else { 64.0 };
    let mut previous = 0.0;
    (1..=levels)
        .map(|i| {
            let automatic = (radius * 6.0 / 4.8 * f64::from(1u32 << (i - 1))).round().max(4.0);
            let distance = opts.distances[i - 1].filter(|v| v.is_finite() && *v > 0.0).unwrap_or(automatic);
            let switch = distance.min(1_000_000.0).max(previous + 0.01);
            previous = switch;
            LodLevel { fraction: reduction.powi(i as i32), switch }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wedge {
    pub pos: DVec3,
    pub normal: DVec3,
    pub uv: DVec2,
}

#[derive(Clone, Copy, Debug, Default)]
struct Quadric([f64; 10]);

impl Quadric {
    fn plane(n: DVec3, d: f64, w: f64) -> Self {
        let (a, b, c) = (n.x, n.y, n.z);
        Quadric([a * a, a * b, a * c, a * d, b * b, b * c, b * d, c * c, c * d, d * d].map(|v| v * w))
    }
    fn add(&mut self, o: &Quadric) {
        for (a, b) in self.0.iter_mut().zip(o.0) {
            *a += b;
        }
    }
    fn error(&self, p: DVec3) -> f64 {
        let q = &self.0;
        let (x, y, z) = (p.x, p.y, p.z);
        q[0] * x * x
            + 2.0 * q[1] * x * y
            + 2.0 * q[2] * x * z
            + 2.0 * q[3] * x
            + q[4] * y * y
            + 2.0 * q[5] * y * z
            + 2.0 * q[6] * y
            + q[7] * z * z
            + 2.0 * q[8] * z
            + q[9]
    }
}

const SEAM_WEIGHT: f64 = 8.0;
const SOFT_SEAM_WEIGHT: f64 = 1.0;
const MAX_FOLD_COS: f64 = 0.5;
const MAX_DEVIATION: f64 = 0.05;

struct Entry {
    cost: f64,
    point: u32,
    stamp: u32,
}

impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Entry {
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost.total_cmp(&self.cost).then_with(|| o.point.cmp(&self.point))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Smooth,
    Soft,
    Hard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Corner {
    Old(u32),
    New(usize),
}

struct Collapse {
    target: u32,
    cost: f64,
    remap: Vec<(u32, Corner)>,
    made: Vec<Wedge>,
}

struct Simplifier {
    wedges: Vec<Wedge>,
    synth: bool,
    point_of: Vec<u32>,
    points: Vec<DVec3>,
    tris: Vec<[u32; 3]>,
    group: Vec<u32>,
    alive: Vec<bool>,
    alive_count: usize,
    around: Vec<Vec<u32>>,
    quadric: Vec<Quadric>,
    shape: Vec<Quadric>,
    area: Vec<f64>,
    limit: f64,
    removed: Vec<bool>,
    stamp: Vec<u32>,
}

impl Simplifier {
    fn new(wedges: &[Wedge], tris: &[[u32; 3]], groups: &[u32], synth: bool) -> Self {
        let mut index: HashMap<[i64; 3], u32> = HashMap::new();
        let mut points = Vec::new();
        let q = |v: f64| (v * 1e4).round() as i64;
        let point_of: Vec<u32> = wedges
            .iter()
            .map(|w| {
                *index.entry([q(w.pos.x), q(w.pos.y), q(w.pos.z)]).or_insert_with(|| {
                    points.push(w.pos);
                    (points.len() - 1) as u32
                })
            })
            .collect();
        let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        for p in &points {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let diagonal = if points.is_empty() { 0.0 } else { (hi - lo).length() };
        let limit = if synth { (diagonal * MAX_DEVIATION).powi(2) } else { f64::INFINITY };
        let mut s = Simplifier {
            wedges: wedges.to_vec(),
            synth,
            point_of,
            around: vec![Vec::new(); points.len()],
            quadric: vec![Quadric::default(); points.len()],
            shape: vec![Quadric::default(); points.len()],
            area: vec![0.0; points.len()],
            limit,
            removed: vec![false; points.len()],
            stamp: vec![0; points.len()],
            points,
            tris: Vec::with_capacity(tris.len()),
            group: Vec::with_capacity(tris.len()),
            alive: Vec::with_capacity(tris.len()),
            alive_count: 0,
        };
        for (i, t) in tris.iter().enumerate() {
            if t.iter().any(|&w| w as usize >= wedges.len()) {
                continue;
            }
            let p = t.map(|w| s.point_of[w as usize]);
            if p[0] == p[1] || p[1] == p[2] || p[0] == p[2] {
                continue;
            }
            let id = s.tris.len() as u32;
            s.tris.push(*t);
            s.group.push(groups.get(i).copied().unwrap_or(0));
            s.alive.push(true);
            s.alive_count += 1;
            for k in p {
                s.around[k as usize].push(id);
            }
        }
        s.init_quadrics();
        s
    }

    fn tri_points(&self, t: u32) -> [u32; 3] {
        self.tris[t as usize].map(|w| self.point_of[w as usize])
    }

    fn tri_normal(&self, t: u32) -> DVec3 {
        let p = self.tri_points(t).map(|i| self.points[i as usize]);
        (p[1] - p[0]).cross(p[2] - p[0])
    }

    fn wedge_at(&self, t: u32, p: u32) -> Option<u32> {
        self.tris[t as usize].iter().copied().find(|&w| self.point_of[w as usize] == p)
    }

    fn alive_around(&self, p: u32) -> impl Iterator<Item = u32> + '_ {
        self.around[p as usize].iter().copied().filter(|&t| self.alive[t as usize])
    }

    fn edge_tris(&self, a: u32, b: u32) -> Vec<u32> {
        self.alive_around(a).filter(|&t| self.tri_points(t).contains(&b)).collect()
    }

    fn neighbours(&self, p: u32) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for t in self.alive_around(p) {
            for q in self.tri_points(t) {
                if q != p && !out.contains(&q) {
                    out.push(q);
                }
            }
        }
        out
    }

    fn edge(&self, a: u32, b: u32) -> Option<Edge> {
        let tris = self.edge_tris(a, b);
        match tris.len() {
            1 => Some(Edge::Hard),
            2 => {
                let (t1, t2) = (tris[0], tris[1]);
                if self.group[t1 as usize] != self.group[t2 as usize] {
                    Some(Edge::Hard)
                } else if self.wedge_at(t1, a) != self.wedge_at(t2, a) || self.wedge_at(t1, b) != self.wedge_at(t2, b) {
                    Some(if self.synth { Edge::Soft } else { Edge::Hard })
                } else {
                    Some(Edge::Smooth)
                }
            }
            _ => None,
        }
    }

    fn init_quadrics(&mut self) {
        for t in 0..self.tris.len() as u32 {
            let p = self.tri_points(t);
            let n = self.tri_normal(t);
            let area2 = n.length();
            if area2 <= 1e-12 {
                continue;
            }
            let n = n / area2;
            let q = Quadric::plane(n, -n.dot(self.points[p[0] as usize]), area2 * 0.5);
            for k in p {
                self.quadric[k as usize].add(&q);
                self.shape[k as usize].add(&q);
                self.area[k as usize] += area2 * 0.5;
            }
        }
        let mut seen = HashSet::new();
        for t in 0..self.tris.len() as u32 {
            let p = self.tri_points(t);
            for k in 0..3 {
                let (a, b) = (p[k], p[(k + 1) % 3]);
                if !seen.insert((a.min(b), a.max(b), t)) {
                    continue;
                }
                let weight = match self.edge(a, b) {
                    Some(Edge::Smooth) => continue,
                    Some(Edge::Soft) => SOFT_SEAM_WEIGHT,
                    _ => SEAM_WEIGHT,
                };
                let (pa, pb) = (self.points[a as usize], self.points[b as usize]);
                let e = pb - pa;
                let n = e.cross(self.tri_normal(t)).normalize_or_zero();
                if n == DVec3::ZERO {
                    continue;
                }
                let q = Quadric::plane(n, -n.dot(pa), e.length_squared() * weight);
                self.quadric[a as usize].add(&q);
                self.quadric[b as usize].add(&q);
            }
        }
    }

    fn deviation(&self, p: u32, q: u32) -> f64 {
        let mut shape = self.shape[p as usize];
        shape.add(&self.shape[q as usize]);
        let area = self.area[p as usize] + self.area[q as usize];
        if area <= 1e-12 { 0.0 } else { shape.error(self.points[q as usize]).max(0.0) / area }
    }

    fn best(&self, p: u32) -> Option<Collapse> {
        if self.removed[p as usize] {
            return None;
        }
        let ring = self.neighbours(p);
        if ring.is_empty() {
            return None;
        }
        let mut hard = Vec::new();
        for &q in &ring {
            match self.edge(p, q) {
                None => return None,
                Some(Edge::Hard) => hard.push(q),
                Some(_) => {}
            }
        }
        let candidates: &[u32] = match hard.len() {
            0 => &ring,
            2 => &hard,
            _ => return None,
        };
        let mut best: Option<Collapse> = None;
        for &q in candidates {
            let mut qq = self.quadric[p as usize];
            qq.add(&self.quadric[q as usize]);
            let cost = qq.error(self.points[q as usize]).max(0.0);
            if best.as_ref().is_some_and(|b| b.cost <= cost) {
                continue;
            }
            if self.deviation(p, q) > self.limit {
                continue;
            }
            if let Some((remap, made)) = self.check(p, q, &ring) {
                best = Some(Collapse { target: q, cost, remap, made });
            }
        }
        best
    }

    fn extrapolated(&self, t: u32, p: u32, at: DVec3) -> Option<Wedge> {
        let corners = self.tris[t as usize].map(|i| self.wedges[i as usize]);
        let own = corners[self.tris[t as usize].iter().position(|&w| self.point_of[w as usize] == p)?];
        let (a, e1, e2) = (corners[0].pos, corners[1].pos - corners[0].pos, corners[2].pos - corners[0].pos);
        let n = e1.cross(e2);
        let nn = n.length_squared();
        if nn <= 1e-18 {
            return None;
        }
        let d = at - a;
        let u = d.cross(e2).dot(n) / nn;
        let v = e1.cross(d).dot(n) / nn;
        let uv = corners[0].uv + (corners[1].uv - corners[0].uv) * u + (corners[2].uv - corners[0].uv) * v;
        (uv.is_finite()).then_some(Wedge { pos: at, normal: own.normal, uv })
    }

    fn check(&self, p: u32, q: u32, ring_p: &[u32]) -> Option<CollapsePlan> {
        let shared = self.edge_tris(p, q);
        if shared.is_empty() {
            return None;
        }
        let ring_q = self.neighbours(q);
        let common = ring_p.iter().filter(|r| ring_q.contains(r)).count();
        if common != shared.len() {
            return None;
        }
        let fan: Vec<u32> = self.alive_around(p).collect();
        let pos_q = self.points[q as usize];
        let mut side: HashMap<u32, Corner> = HashMap::new();
        let mut made: Vec<Wedge> = Vec::new();
        let mut seeds: Vec<(u32, Corner)> = Vec::new();
        for &t in &shared {
            seeds.push((t, Corner::Old(self.wedge_at(t, q)?)));
        }
        loop {
            for (t, corner) in seeds.drain(..) {
                if side.contains_key(&t) {
                    continue;
                }
                side.insert(t, corner);
                let mut stack = vec![t];
                while let Some(t) = stack.pop() {
                    for x in self.tri_points(t) {
                        if x == p || x == q || self.edge(p, x) != Some(Edge::Smooth) {
                            continue;
                        }
                        for &u in &fan {
                            if !side.contains_key(&u) && self.tri_points(u).contains(&x) {
                                side.insert(u, corner);
                                stack.push(u);
                            }
                        }
                    }
                }
            }
            let Some(&rest) = fan.iter().find(|t| !side.contains_key(t)) else { break };
            if !self.synth {
                return None;
            }
            made.push(self.extrapolated(rest, p, pos_q)?);
            seeds.push((rest, Corner::New(made.len() - 1)));
        }
        let wedge = |c: Corner| match c {
            Corner::Old(i) => self.wedges[i as usize],
            Corner::New(i) => made[i],
        };
        let mut remap = Vec::new();
        for &t in &fan {
            if shared.contains(&t) {
                continue;
            }
            let &w = side.get(&t)?;
            let old = self.tri_normal(t);
            let pts = self.tri_points(t);
            let moved = pts.map(|i| if i == p { pos_q } else { self.points[i as usize] });
            let new = (moved[1] - moved[0]).cross(moved[2] - moved[0]);
            let (lo, ln) = (old.length(), new.length());
            if ln <= 1e-9 * lo.max(1e-9) || ln < 1e-12 {
                return None;
            }
            if lo > 1e-12 && old.dot(new) < MAX_FOLD_COS * lo * ln {
                return None;
            }
            let corners = self.tris[t as usize];
            let old_uv = corners.map(|i| self.wedges[i as usize].uv);
            let new_uv =
                corners.map(|c| if self.point_of[c as usize] == p { wedge(w).uv } else { self.wedges[c as usize].uv });
            let area = |u: [DVec2; 3]| (u[1] - u[0]).perp_dot(u[2] - u[0]);
            let (a0, a1) = (area(old_uv), area(new_uv));
            if a0.abs() > 1e-12 && a0 * a1 <= 0.0 {
                return None;
            }
            remap.push((t, w));
        }
        Some((remap, made))
    }

    fn apply(&mut self, p: u32, c: Collapse) {
        let q = c.target;
        for t in self.edge_tris(p, q) {
            self.alive[t as usize] = false;
            self.alive_count -= 1;
        }
        let base = self.wedges.len() as u32;
        for w in c.made {
            self.wedges.push(w);
            self.point_of.push(q);
        }
        for (t, w) in c.remap {
            let w = match w {
                Corner::Old(i) => i,
                Corner::New(i) => base + i as u32,
            };
            for corner in self.tris[t as usize].iter_mut() {
                if self.point_of[*corner as usize] == p {
                    *corner = w;
                }
            }
            self.around[q as usize].push(t);
        }
        self.removed[p as usize] = true;
        let qp = self.quadric[p as usize];
        self.quadric[q as usize].add(&qp);
        let sp = self.shape[p as usize];
        self.shape[q as usize].add(&sp);
        self.area[q as usize] += self.area[p as usize];
        self.around[p as usize].clear();
        let alive = &self.alive;
        self.around[q as usize].retain(|&t| alive[t as usize]);
    }

    fn snapshot(&self) -> Vec<([u32; 3], u32)> {
        (0..self.tris.len()).filter(|&t| self.alive[t]).map(|t| (self.tris[t], self.group[t])).collect()
    }

    fn run(mut self, targets: &[usize]) -> SimplifiedLods {
        let mut heap = BinaryHeap::new();
        for p in 0..self.points.len() as u32 {
            if let Some(c) = self.best(p) {
                heap.push(Entry { cost: c.cost, point: p, stamp: 0 });
            }
        }
        let mut out = Vec::with_capacity(targets.len());
        for &target in targets {
            while self.alive_count > target {
                let Some(e) = heap.pop() else { break };
                if self.removed[e.point as usize] || self.stamp[e.point as usize] != e.stamp {
                    continue;
                }
                let Some(c) = self.best(e.point) else { continue };
                if c.cost > e.cost + 1e-9 * (1.0 + e.cost.abs()) {
                    heap.push(Entry { cost: c.cost, point: e.point, stamp: e.stamp });
                    continue;
                }
                let q = c.target;
                self.apply(e.point, c);
                let mut touched = self.neighbours(q);
                touched.push(q);
                for r in touched {
                    self.stamp[r as usize] = self.stamp[r as usize].wrapping_add(1);
                    if let Some(c) = self.best(r) {
                        heap.push(Entry { cost: c.cost, point: r, stamp: self.stamp[r as usize] });
                    }
                }
            }
            out.push(self.snapshot());
        }
        (self.wedges, out)
    }
}

pub fn simplify(wedges: &[Wedge], tris: &[[u32; 3]], groups: &[u32], targets: &[usize]) -> Vec<Vec<([u32; 3], u32)>> {
    Simplifier::new(wedges, tris, groups, false).run(targets).1
}

pub fn simplify_with_new_corners(
    wedges: &[Wedge],
    tris: &[[u32; 3]],
    groups: &[u32],
    targets: &[usize],
) -> SimplifiedLods {
    Simplifier::new(wedges, tris, groups, true).run(targets)
}

fn wedge_key(p: DVec3, n: DVec3, uv: DVec2) -> [i64; 8] {
    let q = |v: f64, s: f64| (v * s).round() as i64;
    [q(p.x, 1e4), q(p.y, 1e4), q(p.z, 1e4), q(n.x, 1e3), q(n.y, 1e3), q(n.z, 1e3), q(uv.x, 1e5), q(uv.y, 1e5)]
}

pub fn simplify_triangles(triangles: &[Triangle], levels: &[LodLevel]) -> Vec<Vec<Triangle>> {
    let mut index: HashMap<[i64; 8], u32> = HashMap::new();
    let mut wedges: Vec<Wedge> = Vec::new();
    let mut tris = Vec::with_capacity(triangles.len());
    let mut groups = Vec::with_capacity(triangles.len());
    for t in triangles {
        let corner = |k: usize, wedges: &mut Vec<Wedge>, index: &mut HashMap<[i64; 8], u32>| {
            *index.entry(wedge_key(t.pos[k], t.normal[k], t.uv[k])).or_insert_with(|| {
                wedges.push(Wedge { pos: t.pos[k], normal: t.normal[k], uv: t.uv[k] });
                (wedges.len() - 1) as u32
            })
        };
        let c = [
            corner(0, &mut wedges, &mut index),
            corner(1, &mut wedges, &mut index),
            corner(2, &mut wedges, &mut index),
        ];
        tris.push(c);
        groups.push(t.material as u32);
    }
    let targets: Vec<usize> =
        levels.iter().map(|l| ((triangles.len() as f64) * l.fraction).round().max(1.0) as usize).collect();
    let (wedges, levels) = simplify_with_new_corners(&wedges, &tris, &groups, &targets);
    levels
        .into_iter()
        .map(|level| {
            level
                .into_iter()
                .map(|(c, g)| {
                    let w = c.map(|i| wedges[i as usize]);
                    Triangle {
                        pos: w.map(|x| x.pos),
                        normal: w.map(|x| x.normal),
                        uv: w.map(|x| x.uv),
                        material: g as usize,
                    }
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests;
