use std::cell::RefCell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use ahash::AHashMap;
use glam::DVec3;
use rayon::prelude::*;

pub const MAX_CLUSTERS: usize = 65536;
pub const MAX_PORTALS: usize = 1 << 20;
pub const MAX_POINTS: usize = 1024;
const MAX_COORD: f64 = 1.0e7;
const ON_VIS_EPSILON: f64 = 0.1;
const SEPARATOR_MARGIN: f64 = 0.5;
const PRUNE_MARGIN: f64 = 1.0;
const CENTER_SLACK: f64 = 0.01;
const CAP: usize = 32;
const MAX_CHOP: usize = 64;
const MAX_DEPTH: usize = 1024;
const STACK_SIZE: usize = 64 << 20;
const WINDOW: usize = 2;
const MEMO_FLOWS: usize = 8;
const MEMO_MERGES: u32 = 1;
const MEMO_WORDS: usize = 1 << 22;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisPortal {
    pub points: Vec<DVec3>,
    pub clusters: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisOptions {
    pub fast: bool,
    pub threads: Option<usize>,
    pub sort: bool,
    pub trace: Option<(u32, u32)>,
    pub radius_override: Option<f64>,
    pub vvis_flow: bool,
    pub simd: Option<Isa>,
}

impl Default for VisOptions {
    fn default() -> Self {
        VisOptions {
            fast: false,
            threads: None,
            sort: true,
            trace: None,
            radius_override: None,
            vvis_flow: false,
            simd: None,
        }
    }
}

#[path = "vis_simd.rs"]
mod simd;
pub use simd::{Isa, isa};

#[path = "vis_cache.rs"]
pub mod cache;
pub use cache::VisCache;

#[derive(Clone, Copy, Debug)]
struct Plane {
    n: DVec3,
    d: f64,
}

impl Plane {
    #[inline]
    fn dist(&self, p: DVec3) -> f64 {
        self.n.dot(p) - self.d
    }

    #[inline]
    fn flip(self) -> Plane {
        Plane { n: -self.n, d: -self.d }
    }
}

const VVIS_EPSILON: f64 = 0.01;
const VVIS_FIXED_POINTS: usize = 12;

type V3 = [f32; 3];

#[inline]
fn dot3(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn sub3(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn neg3(a: V3) -> V3 {
    [-a[0], -a[1], -a[2]]
}

#[derive(Clone, Copy, Debug, Default)]
struct VPlane {
    n: V3,
    d: f32,
}

impl VPlane {
    #[inline]
    fn dist(&self, p: V3) -> f32 {
        dot3(p, self.n) - self.d
    }

    fn flip(self) -> VPlane {
        VPlane { n: neg3(self.n), d: -self.d }
    }
}

fn vvis_plane(w: &[V3]) -> VPlane {
    let v1 = sub3(w[2], w[1]);
    let v2 = sub3(w[0], w[1]);
    let mut n = [v2[1] * v1[2] - v2[2] * v1[1], v2[2] * v1[0] - v2[0] * v1[2], v2[0] * v1[1] - v2[1] * v1[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let inv = 1.0f32 / (len + f32::EPSILON);
    for x in &mut n {
        *x *= inv;
    }
    VPlane { n, d: dot3(w[0], n) }
}

fn vvis_sphere(w: &[V3]) -> (V3, f32) {
    let mut total = [0.0f32; 3];
    for p in w {
        for k in 0..3 {
            total[k] += p[k];
        }
    }
    for t in &mut total {
        *t /= w.len() as f32;
    }
    let mut best = 0.0f32;
    for p in w {
        let d = sub3(*p, total);
        let r = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if r > best {
            best = r;
        }
    }
    (total, best)
}

#[inline]
fn vvis_side(d: f32) -> u8 {
    if f64::from(d) > VVIS_EPSILON {
        FRONT
    } else if f64::from(d) < -VVIS_EPSILON {
        BACK
    } else {
        ON
    }
}

fn vvis_chop(w: &[V3], split: &VPlane) -> Option<Vec<V3>> {
    let n = w.len();
    let mut dists = Vec::with_capacity(n + 1);
    let mut sides = Vec::with_capacity(n + 1);
    let mut counts = [0usize; 3];
    for p in w {
        let d = split.dist(*p);
        let s = vvis_side(d);
        dists.push(d);
        sides.push(s);
        counts[s as usize] += 1;
    }
    if counts[BACK as usize] == 0 {
        return Some(w.to_vec());
    }
    if counts[FRONT as usize] == 0 {
        return None;
    }
    sides.push(sides[0]);
    dists.push(dists[0]);
    let mut out: Vec<V3> = Vec::with_capacity(VVIS_FIXED_POINTS);
    for i in 0..n {
        let p1 = w[i];
        if out.len() == VVIS_FIXED_POINTS {
            return Some(w.to_vec());
        }
        if sides[i] == ON {
            out.push(p1);
            continue;
        }
        if sides[i] == FRONT {
            out.push(p1);
        }
        if sides[i + 1] == ON || sides[i + 1] == sides[i] {
            continue;
        }
        if out.len() == VVIS_FIXED_POINTS {
            return Some(w.to_vec());
        }
        let p2 = w[(i + 1) % n];
        let dot = dists[i] / (dists[i] - dists[i + 1]);
        let mut mid = [0.0f32; 3];
        for j in 0..3 {
            mid[j] = if split.n[j] == 1.0 {
                split.d
            } else if split.n[j] == -1.0 {
                -split.d
            } else {
                p1[j] + dot * (p2[j] - p1[j])
            };
        }
        out.push(mid);
    }
    Some(out)
}

fn vvis_clip_to_separators(source: &[V3], pass: &[V3], target: Vec<V3>, flipclip: bool) -> Option<Vec<V3>> {
    let mut target = target;
    let ns = source.len();
    for i in 0..ns {
        let l = (i + 1) % ns;
        let v1 = sub3(source[l], source[i]);
        for j in 0..pass.len() {
            let v2 = sub3(pass[j], source[i]);
            let mut n = [v1[1] * v2[2] - v1[2] * v2[1], v1[2] * v2[0] - v1[0] * v2[2], v1[0] * v2[1] - v1[1] * v2[0]];
            let length = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
            if f64::from(length) < VVIS_EPSILON {
                continue;
            }
            let length = (1.0f64 / f64::from(length).sqrt()) as f32;
            for x in &mut n {
                *x *= length;
            }
            let mut plane = VPlane { n, d: dot3(pass[j], n) };
            let mut flip = None;
            for (k, p) in source.iter().enumerate() {
                if k == i || k == l {
                    continue;
                }
                match vvis_side(plane.dist(*p)) {
                    BACK => {
                        flip = Some(false);
                        break;
                    }
                    FRONT => {
                        flip = Some(true);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(flip) = flip else { continue };
            if flip {
                plane = plane.flip();
            }
            let mut front = 0;
            let mut behind = false;
            for (k, p) in pass.iter().enumerate() {
                if k == j {
                    continue;
                }
                match vvis_side(plane.dist(*p)) {
                    BACK => {
                        behind = true;
                        break;
                    }
                    FRONT => front += 1,
                    _ => {}
                }
            }
            if behind || front == 0 {
                continue;
            }
            if flipclip {
                plane = plane.flip();
            }
            target = vvis_chop(&target, &plane)?;
        }
    }
    Some(target)
}

#[derive(Clone, Copy, Debug, Default)]
struct VvisPortal {
    plane: VPlane,
    origin: V3,
    radius: f32,
    first: u32,
    count: u32,
}

#[derive(Clone, Debug)]
struct Portal {
    plane: Plane,
    to: u32,
    origin: DVec3,
    radius: f64,
    first: u32,
    count: u32,
}

enum Chopped {
    Same,
    Gone,
    New,
}

const FRONT: u8 = 0;
const BACK: u8 = 1;
const ON: u8 = 2;

fn chop(input: &[DVec3], plane: &Plane, out: &mut Vec<DVec3>) -> Chopped {
    let n = input.len();
    if n > MAX_CHOP {
        return if input.iter().all(|p| plane.dist(*p) < -ON_VIS_EPSILON) { Chopped::Gone } else { Chopped::Same };
    }
    let (mut front, mut back) = (false, false);
    for p in input {
        let d = plane.dist(*p);
        front |= d > ON_VIS_EPSILON;
        back |= d < -ON_VIS_EPSILON;
    }
    if !back {
        return Chopped::Same;
    }
    if !front {
        return Chopped::Gone;
    }
    let mut dists = [0.0f64; MAX_CHOP + 1];
    let mut sides = [ON; MAX_CHOP + 1];
    for (i, p) in input.iter().enumerate() {
        let d = plane.dist(*p);
        dists[i] = d;
        sides[i] = if d > ON_VIS_EPSILON {
            FRONT
        } else if d < -ON_VIS_EPSILON {
            BACK
        } else {
            ON
        };
    }
    dists[n] = dists[0];
    sides[n] = sides[0];
    out.clear();
    for i in 0..n {
        let p1 = input[i];
        if out.len() == CAP {
            return Chopped::Same;
        }
        if sides[i] == ON {
            out.push(p1);
            continue;
        }
        if sides[i] == FRONT {
            out.push(p1);
        }
        if sides[i + 1] == ON || sides[i + 1] == sides[i] {
            continue;
        }
        if out.len() == CAP {
            return Chopped::Same;
        }
        let p2 = input[(i + 1) % n];
        let t = dists[i] / (dists[i] - dists[i + 1]);
        let mut mid = DVec3::ZERO;
        for j in 0..3 {
            mid[j] = if plane.n[j] == 1.0 {
                plane.d
            } else if plane.n[j] == -1.0 {
                -plane.d
            } else {
                p1[j] + t * (p2[j] - p1[j])
            };
        }
        out.push(mid);
    }
    Chopped::New
}

fn separators(source: &[DVec3], pass: &[DVec3], flip: bool, out: &mut Vec<Plane>) {
    out.clear();
    separators_each(source, pass, flip, |p| {
        out.push(p);
        true
    });
}

#[inline]
fn separators_each(source: &[DVec3], pass: &[DVec3], flip: bool, mut f: impl FnMut(Plane) -> bool) {
    const BATCH: usize = 8;
    let ns = source.len();
    let np = pass.len();

    let mut normals = [(DVec3::ZERO, 0.0f64); BATCH];
    for i in 0..ns {
        let l = if i + 1 == ns { 0 } else { i + 1 };
        let si = source[i];
        let v1 = source[l] - si;
        for j0 in (0..np).step_by(BATCH) {
            let chunk = &pass[j0..np.min(j0 + BATCH)];
            for (c, pj) in chunk.iter().enumerate() {
                let v2 = *pj - si;
                let normal = v1.cross(v2);
                let len2 = normal.length_squared();
                normals[c] = (normal, if len2 < ON_VIS_EPSILON { -1.0 } else { len2.sqrt() });
            }
            for (pj, (normal, len)) in chunk.iter().zip(normals) {
                if len < 0.0 {
                    continue;
                }
                let normal = normal / len;
                let mut plane = Plane { n: normal, d: pj.dot(normal) };
                let mut flip_test = None;
                for (k, sk) in source.iter().enumerate() {
                    if k == i || k == l {
                        continue;
                    }
                    let d = plane.dist(*sk);
                    if d < -ON_VIS_EPSILON {
                        flip_test = Some(false);
                        break;
                    } else if d > ON_VIS_EPSILON {
                        flip_test = Some(true);
                        break;
                    }
                }
                let Some(flip_test) = flip_test else {
                    continue;
                };
                if flip_test {
                    plane = plane.flip();
                }
                let mut front = 0;
                let mut separates = true;
                for pk in pass {
                    let d = plane.dist(*pk);
                    if d < -ON_VIS_EPSILON {
                        separates = false;
                        break;
                    } else if d > ON_VIS_EPSILON {
                        front += 1;
                    }
                }
                if !separates || front == 0 {
                    continue;
                }
                let plane = if flip { plane.flip() } else { plane };
                if !f(Plane { n: plane.n, d: plane.d - SEPARATOR_MARGIN }) {
                    return;
                }
            }
        }
    }
}

fn clip_by(target: &mut Vec<DVec3>, planes: &[Plane], spare: &mut Vec<DVec3>) -> bool {
    for plane in planes {
        match chop(target, plane, spare) {
            Chopped::Same => {}
            Chopped::Gone => return false,
            Chopped::New => std::mem::swap(target, spare),
        }
    }
    true
}

fn clip_by_separators(
    target: &mut Vec<DVec3>,
    source: &[DVec3],
    pass: &[DVec3],
    flip: bool,
    spare: &mut Vec<DVec3>,
) -> bool {
    let mut kept = true;
    separators_each(source, pass, flip, |plane| {
        match chop(target, &plane, spare) {
            Chopped::Same => {}
            Chopped::Gone => kept = false,
            Chopped::New => std::mem::swap(target, spare),
        }
        kept
    });
    kept
}

fn coplanar_hull(points: &[DVec3], n: DVec3) -> Option<Vec<DVec3>> {
    let a = n.abs();
    let up = if a.z >= a.x && a.z >= a.y { DVec3::X } else { DVec3::Z };
    let u = (up - n * up.dot(n)).normalize_or_zero();
    let v = n.cross(u);
    let mut pts: Vec<(f64, f64, DVec3)> = points.iter().map(|p| (p.dot(u), p.dot(v), *p)).collect();
    pts.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
    let cross = |o: &(f64, f64, DVec3), a: &(f64, f64, DVec3), b: &(f64, f64, DVec3)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1.0e-9 && (a.1 - b.1).abs() < 1.0e-9);
    if pts.len() < 3 {
        return None;
    }
    let mut hull: Vec<(f64, f64, DVec3)> = Vec::with_capacity(pts.len() * 2);
    for p in &pts {
        while hull.len() >= 2 && cross(&hull[hull.len() - 2], &hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(*p);
    }
    let lower = hull.len() + 1;
    for p in pts.iter().rev().skip(1) {
        while hull.len() >= lower && cross(&hull[hull.len() - 2], &hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(*p);
    }
    hull.pop();
    if hull.len() < 3 {
        return None;
    }
    Some(hull.into_iter().map(|h| h.2).collect())
}

fn polygon_contains(outer: &[DVec3], inner: &[DVec3]) -> bool {
    let n = outer.len();
    if n < 3 {
        return false;
    }
    let mut normal = DVec3::ZERO;
    for i in 0..n {
        let (c, x) = (outer[i], outer[(i + 1) % n]);
        normal.x += (c.y - x.y) * (c.z + x.z);
        normal.y += (c.z - x.z) * (c.x + x.x);
        normal.z += (c.x - x.x) * (c.y + x.y);
    }
    let len = normal.length();
    if len.is_nan() || len <= 1.0e-9 {
        return false;
    }
    let normal = normal / len;
    for i in 0..n {
        let a = outer[i];
        let e = outer[(i + 1) % n] - a;
        let el = e.length();
        if el < 1.0e-9 {
            continue;
        }
        let inward = normal.cross(e) / el;
        if inner.iter().any(|x| (*x - a).dot(inward) < -1.0e-3) {
            return false;
        }
    }
    true
}

fn bounds(p: &[DVec3]) -> (DVec3, DVec3) {
    p.iter().fold((DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)), |(lo, hi), x| (lo.min(*x), hi.max(*x)))
}

fn box_contains(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    (b.0 - a.0).min_element() >= -1.0e-3 && (a.1 - b.1).min_element() >= -1.0e-3
}

#[derive(Clone, Debug, Default)]
struct Bits {
    start: usize,
    words: Box<[u64]>,
}

impl Bits {
    fn from_dense(d: &[u64]) -> Bits {
        Bits::from_span(0, d)
    }

    fn from_span(start: usize, d: &[u64]) -> Bits {
        let Some(first) = d.iter().position(|w| *w != 0) else { return Bits::default() };
        let last = d.iter().rposition(|w| *w != 0).unwrap_or(first);
        Bits { start: start + first, words: d[first..=last].into() }
    }

    fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    fn view(&self) -> (usize, &[u64]) {
        (self.start, &self.words)
    }
}

#[inline]
fn view_get(v: (usize, &[u64]), i: usize) -> bool {
    let w = i >> 6;
    w >= v.0 && w - v.0 < v.1.len() && v.1[w - v.0] >> (i & 63) & 1 != 0
}

#[inline]
fn dense_get(d: &[u64], i: usize) -> bool {
    d[i >> 6] >> (i & 63) & 1 != 0
}

#[inline]
fn dense_set(d: &mut [u64], i: usize) {
    d[i >> 6] |= 1 << (i & 63);
}

fn dense_or(d: &mut [u64], v: (usize, &[u64])) {
    for (k, w) in v.1.iter().enumerate() {
        if let Some(x) = d.get_mut(v.0 + k) {
            *x |= *w;
        }
    }
}

fn for_each_bit(d: &[u64], mut f: impl FnMut(usize)) {
    for (k, w) in d.iter().enumerate() {
        let mut w = *w;
        while w != 0 {
            let b = w.trailing_zeros() as usize;
            f(k * 64 + b);
            w &= w - 1;
        }
    }
}

struct World {
    portals: Vec<Portal>,
    points: Vec<DVec3>,
    leaf_start: Vec<u32>,
    rev: Vec<u32>,
    spheres: Vec<(DVec3, f64)>,
    words: usize,
    radius2: Option<f64>,
    vv: Vec<VvisPortal>,
    vpoints: Vec<V3>,
    vidx: Vec<u32>,
}

impl World {
    fn new(clusters: usize, input: &[VisPortal], radius2: Option<f64>) -> (World, Vec<[u32; 2]>) {
        let mut tmp: Vec<(u32, Portal, VvisPortal)> = Vec::with_capacity(input.len() * 2);
        let mut points = Vec::new();
        let mut vpoints: Vec<V3> = Vec::new();
        let mut neighbours = Vec::new();
        for vp in input {
            let [a, b] = vp.clusters;
            if a as usize >= clusters || b as usize >= clusters || a == b {
                continue;
            }
            let pts = &vp.points;
            let finite = pts.iter().all(|p| p.is_finite() && p.abs().max_element() <= MAX_COORD);
            if pts.len() < 3 || !finite {
                neighbours.push([a, b]);
                continue;
            }
            let mut n = DVec3::ZERO;
            for i in 0..pts.len() {
                let (c, x) = (pts[i], pts[(i + 1) % pts.len()]);
                n.x += (c.y - x.y) * (c.z + x.z);
                n.y += (c.z - x.z) * (c.x + x.x);
                n.z += (c.x - x.x) * (c.y + x.y);
            }
            let len = n.length();
            if len.is_nan() || len <= 1.0e-6 {
                neighbours.push([a, b]);
                continue;
            }
            let n = n / len;
            let origin = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
            let radius = pts.iter().map(|p| p.distance(origin)).fold(0.0, f64::max);
            let first = points.len() as u32;
            points.extend_from_slice(pts);
            let plane = Plane { n, d: n.dot(origin) };
            let count = pts.len() as u32;
            let w32: Vec<V3> = pts.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
            let vplane = vvis_plane(&w32);
            let mut vportal = |w: &[V3], plane: VPlane| {
                let (origin, radius) = vvis_sphere(w);
                let first = vpoints.len() as u32;
                vpoints.extend_from_slice(w);
                VvisPortal { plane, origin, radius, first, count }
            };
            let forward = vportal(&w32, vplane.flip());
            let reversed: Vec<V3> = w32.iter().rev().copied().collect();
            let backward = vportal(&reversed, vplane);
            tmp.push((a, Portal { plane, to: b, origin, radius, first, count }, forward));
            tmp.push((b, Portal { plane: plane.flip(), to: a, origin, radius, first, count }, backward));
        }
        let mut leaf_start = vec![0u32; clusters + 1];
        for (from, _, _) in &tmp {
            leaf_start[*from as usize + 1] += 1;
        }
        for c in 0..clusters {
            leaf_start[c + 1] += leaf_start[c];
        }
        let mut slots: Vec<Option<Portal>> = vec![None; tmp.len()];
        let mut vv = vec![VvisPortal::default(); tmp.len()];
        let mut vidx = vec![0u32; tmp.len()];
        let mut index = vec![0u32; tmp.len()];
        let mut next = leaf_start.clone();
        for (k, (from, p, v)) in tmp.into_iter().enumerate() {
            let i = &mut next[from as usize];
            slots[*i as usize] = Some(p);
            vv[*i as usize] = v;
            vidx[*i as usize] = k as u32;
            index[k] = *i;
            *i += 1;
        }
        let portals: Vec<Portal> = slots.into_iter().flatten().collect();
        let mut rev = vec![0u32; portals.len()];
        for pair in index.chunks_exact(2) {
            rev[pair[0] as usize] = pair[1];
            rev[pair[1] as usize] = pair[0];
        }
        let words = portals.len().div_ceil(64).max(1);
        let spheres = portals.iter().map(|p| (p.origin, p.radius)).collect();
        (World { portals, points, leaf_start, rev, spheres, words, radius2, vv, vpoints, vidx }, neighbours)
    }

    #[inline]
    fn leaf(&self, c: u32) -> std::ops::Range<usize> {
        let c = c as usize;
        self.leaf_start[c] as usize..self.leaf_start[c + 1] as usize
    }

    #[inline]
    fn winding(&self, i: usize) -> &[DVec3] {
        let p = &self.portals[i];
        &self.points[p.first as usize..(p.first + p.count) as usize]
    }

    #[inline]
    fn vwinding(&self, i: usize) -> &[V3] {
        let p = &self.vv[i];
        &self.vpoints[p.first as usize..(p.first + p.count) as usize]
    }

    fn front(&self, p: usize, q: usize) -> bool {
        let (pp, qp) = (&self.portals[p], &self.portals[q]);
        if pp.plane.dist(qp.origin) < -qp.radius - 1.0 || qp.plane.dist(pp.origin) > pp.radius + 1.0 {
            return false;
        }
        let (a, b) = (&self.vv[p], &self.vv[q]);
        if !self.vwinding(q).iter().any(|x| f64::from(a.plane.dist(*x)) > VVIS_EPSILON) {
            return false;
        }
        if !self.vwinding(p).iter().any(|x| f64::from(b.plane.dist(*x)) < -VVIS_EPSILON) {
            return false;
        }
        if let Some(r2) = self.radius2 {
            let mut min = 1_024_000_000.0f64;
            for x in self.vwinding(q) {
                let s = sub3(*x, a.origin);
                let d2 = f64::from(s[0] * s[0] + s[1] * s[1] + s[2] * s[2]);
                if d2 < min {
                    min = d2;
                }
            }
            if min > r2 {
                return false;
            }
        }
        true
    }
}

struct BaseScratch {
    seen: Vec<u64>,
    flood: Vec<u64>,
    touched: Vec<u32>,
    stack: Vec<u32>,
}

impl BaseScratch {
    fn new(words: usize) -> Self {
        BaseScratch { seen: vec![0; words], flood: vec![0; words], touched: Vec::new(), stack: Vec::new() }
    }
}

fn base_flood(w: &World, pi: usize, s: &mut BaseScratch) -> Bits {
    s.stack.push(w.portals[pi].to);
    dense_set(&mut s.seen, pi);
    s.touched.push(pi as u32);
    while let Some(leaf) = s.stack.pop() {
        for q in w.leaf(leaf) {
            if dense_get(&s.seen, q) {
                continue;
            }
            dense_set(&mut s.seen, q);
            s.touched.push(q as u32);
            if w.front(pi, q) {
                dense_set(&mut s.flood, q);
                s.stack.push(w.portals[q].to);
            }
        }
    }
    let bits = Bits::from_dense(&s.flood);
    for t in s.touched.drain(..) {
        let k = t as usize >> 6;
        s.seen[k] = 0;
        s.flood[k] = 0;
    }
    bits
}

#[derive(Clone, Copy)]
struct Sv<'a> {
    idx: &'a [u32],
    w: &'a [u64],
}

impl Sv<'_> {
    #[inline]
    fn get(&self, i: usize) -> bool {
        match self.idx.binary_search(&((i >> 6) as u32)) {
            Ok(k) => self.w[k] >> (i & 63) & 1 != 0,
            Err(_) => false,
        }
    }

    #[inline]
    fn span_len(&self) -> usize {
        match (self.idx.first(), self.idx.last()) {
            (Some(a), Some(b)) => (*b - *a) as usize + 1,
            _ => 0,
        }
    }

    fn subset_of(&self, b: Sv<'_>) -> bool {
        let mut j = 0;
        for (i, w) in self.idx.iter().zip(self.w) {
            if *w == 0 {
                continue;
            }
            while j < b.idx.len() && b.idx[j] < *i {
                j += 1;
            }
            if j == b.idx.len() || b.idx[j] != *i || w & !b.w[j] != 0 {
                return false;
            }
        }
        true
    }
}

#[derive(Default)]
struct SparseBuf {
    idx: Vec<u32>,
    w: Vec<u64>,
}

impl SparseBuf {
    #[inline]
    fn clear(&mut self) {
        self.idx.clear();
        self.w.clear();
    }

    #[inline]
    fn push(&mut self, i: usize, w: u64) {
        self.idx.push(i as u32);
        self.w.push(w);
    }

    #[inline]
    fn view(&self) -> Sv<'_> {
        Sv { idx: &self.idx, w: &self.w }
    }

    fn and(&mut self, idx: &[u32], w: &[u64], test: (usize, &[u64])) {
        let (start, words) = test;
        let n = idx.partition_point(|i| (*i as usize) - start < words.len());
        self.idx.resize(n, 0);
        self.w.resize(n, 0);
        let mut k = 0;
        for (i, x) in idx[..n].iter().zip(&w[..n]) {
            let v = x & words[*i as usize - start];
            self.idx[k] = *i;
            self.w[k] = v;
            k += usize::from(v != 0);
        }
        self.idx.truncate(k);
        self.w.truncate(k);
    }
}

#[derive(Default)]
struct Frame {
    might: SparseBuf,
    seps: [Vec<Plane>; 2],
    pruned: SparseBuf,
    source: Vec<DVec3>,
    pass: Vec<DVec3>,
    spare: Vec<DVec3>,
    cands: Vec<(u32, u32)>,
}

struct Memo {
    depth: usize,
    pass_box: (DVec3, DVec3),
    source_box: (DVec3, DVec3),
    source: (u32, u32),
    pass: (u32, u32),
    might: (u32, u32),
}

struct PortalMemo {
    portal: u32,
    flows: [u32; MEMO_FLOWS],
    count: usize,
    merges: u32,
    saturated: bool,
}

#[derive(Default)]
struct MemoStore {
    of: Vec<u32>,
    portals: Vec<PortalMemo>,
    memos: Vec<Memo>,
    pts: Vec<DVec3>,
    idx: Vec<u32>,
    words: Vec<u64>,
    used: usize,
}

impl MemoStore {
    fn reset(&mut self, portals: usize) {
        for pm in &self.portals {
            if let Some(x) = self.of.get_mut(pm.portal as usize) {
                *x = u32::MAX;
            }
        }
        if self.of.len() < portals {
            self.of.resize(portals, u32::MAX);
        }
        self.portals.clear();
        self.memos.clear();
        self.pts.clear();
        self.idx.clear();
        self.words.clear();
        self.used = 0;
        if self.pts.capacity() > 1 << 20 || self.words.capacity() > 1 << 22 {
            self.pts = Vec::new();
            self.idx = Vec::new();
            self.words = Vec::new();
            self.memos = Vec::new();
        }
    }

    #[inline]
    fn pts(&self, r: (u32, u32)) -> &[DVec3] {
        &self.pts[r.0 as usize..r.1 as usize]
    }

    #[inline]
    fn might(&self, r: (u32, u32)) -> Sv<'_> {
        let r = r.0 as usize..r.1 as usize;
        Sv { idx: &self.idx[r.clone()], w: &self.words[r] }
    }

    fn add_pts(&mut self, p: &[DVec3]) -> (u32, u32) {
        let a = self.pts.len() as u32;
        self.pts.extend_from_slice(p);
        (a, self.pts.len() as u32)
    }

    fn add_might(&mut self, m: Sv<'_>) -> (u32, u32) {
        let a = self.idx.len() as u32;
        self.idx.extend_from_slice(m.idx);
        self.words.extend_from_slice(m.w);
        (a, self.idx.len() as u32)
    }

    fn dominates(
        &self,
        e: &Memo,
        depth: usize,
        source: &[DVec3],
        pass: &[DVec3],
        boxes: [(DVec3, DVec3); 2],
        might: Sv<'_>,
    ) -> bool {
        e.depth <= depth
            && box_contains(e.pass_box, boxes[1])
            && box_contains(e.source_box, boxes[0])
            && might.subset_of(self.might(e.might))
            && polygon_contains(self.pts(e.pass), pass)
            && polygon_contains(self.pts(e.source), source)
    }
}

#[derive(Default)]
struct FlowScratch {
    vis: Vec<u64>,
    frames: Vec<Frame>,
    root: SparseBuf,
    memo: MemoStore,
}

struct Flow<'a, 's> {
    w: &'a World,
    flood: &'a [Bits],
    done: &'a [OnceLock<Bits>],
    round_of: &'a [u32],
    limit: u32,
    base: usize,
    s: &'s mut FlowScratch,
    isa: simd::Isa,
}

enum Through {
    Skip,
    Same,
    Wider { might: SparseBuf, source: Vec<DVec3>, pass: Vec<DVec3> },
}

#[inline]
#[cfg(test)]
fn hidden(planes: [&[Plane]; 2], sphere: &(DVec3, f64), w: &World, q: usize) -> bool {
    let (o, r) = *sphere;
    let lim = -(r + PRUNE_MARGIN);
    let mut points = false;
    for set in planes {
        for pl in set {
            let d = pl.dist(o);
            if d < lim {
                return true;
            }
            points |= d < r && d < CENTER_SLACK - PRUNE_MARGIN;
        }
    }
    points && hidden_by_points(planes, sphere, w, q)
}

fn hidden_by_points(planes: [&[Plane]; 2], sphere: &(DVec3, f64), w: &World, q: usize) -> bool {
    let (o, r) = *sphere;
    planes.iter().flat_map(|set| set.iter()).any(|pl| {
        let d = pl.dist(o);
        d < r && d < CENTER_SLACK - PRUNE_MARGIN && w.winding(q).iter().all(|x| pl.dist(*x) < -PRUNE_MARGIN)
    })
}

fn prune(
    w: &World,
    planes: [&[Plane]; 2],
    might: Sv<'_>,
    vis: &[u64],
    pruned: &mut SparseBuf,
    cands: &mut Vec<(u32, u32)>,
    isa: simd::Isa,
) -> bool {
    cands.clear();
    for (i, x) in might.idx.iter().zip(might.w) {
        let i = *i as usize;
        let mut bits = x & !vis[i];
        let slot = pruned.idx.len() as u32;
        pruned.push(i, *x);
        while bits != 0 {
            let b = bits.trailing_zeros();
            bits &= bits - 1;
            if i * 64 + (b as usize) < w.spheres.len() {
                cands.push((slot, b));
            }
        }
    }
    let mut left = false;
    for chunk in cands.chunks(simd::LANES) {
        let mut sp = simd::Spheres::default();
        for &(slot, b) in chunk {
            sp.push(w.spheres[pruned.idx[slot as usize] as usize * 64 + b as usize]);
        }
        let (behind, points) = simd::sphere_masks(isa, planes, &sp);
        for (k, &(slot, b)) in chunk.iter().enumerate() {
            let out = if behind >> k & 1 != 0 {
                true
            } else if points >> k & 1 != 0 {
                let qi = pruned.idx[slot as usize] as usize * 64 + b as usize;
                hidden_by_points(planes, &w.spheres[qi], w, qi)
            } else {
                false
            };
            if out {
                pruned.w[slot as usize] &= !(1u64 << b);
            } else {
                left = true;
            }
        }
    }
    let mut k = 0;
    for j in 0..pruned.idx.len() {
        if pruned.w[j] != 0 {
            pruned.idx[k] = pruned.idx[j];
            pruned.w[k] = pruned.w[j];
            k += 1;
        }
    }
    pruned.idx.truncate(k);
    pruned.w.truncate(k);
    left
}

impl<'a> Flow<'a, '_> {
    #[inline]
    fn test(&self, q: usize) -> (usize, &'a [u64]) {
        let (done, flood): (&'a [OnceLock<Bits>], &'a [Bits]) = (self.done, self.flood);
        if self.round_of[q] < self.limit
            && let Some(b) = done[q].get()
        {
            return b.view();
        }
        flood[q].view()
    }

    fn leaf_flow(&mut self, leaf: u32, depth: usize, might: Sv<'_>, source: &[DVec3], pass: Option<&[DVec3]>) {
        if depth >= MAX_DEPTH {
            for (i, x) in might.idx.iter().zip(might.w) {
                if let Some(v) = self.s.vis.get_mut(*i as usize) {
                    *v |= *x;
                }
            }
            return;
        }
        let w = self.w;
        let base = &w.portals[self.base];
        if self.s.frames.len() <= depth {
            self.s.frames.resize_with(depth + 1, Frame::default);
        }
        let mut frame = std::mem::take(&mut self.s.frames[depth]);
        let Frame { might: next, seps, pruned, source: new_source, pass: new_pass, spare, cands } = &mut frame;
        if let Some(pass) = pass {
            let vis = &self.s.vis;
            let fresh = might.idx.iter().zip(might.w).any(|(i, x)| x & !vis[*i as usize] != 0);
            if !fresh {
                self.s.frames[depth] = frame;
                return;
            }
            separators(source, pass, false, &mut seps[0]);
            separators(pass, source, true, &mut seps[1]);
            pruned.clear();
            let left = prune(w, [&seps[0], &seps[1]], might, vis, pruned, cands, self.isa);
            if !left {
                self.s.frames[depth] = frame;
                return;
            }
        }
        let might = if pass.is_some() { pruned.view() } else { might };
        for pi in w.leaf(leaf) {
            if !might.get(pi) {
                continue;
            }
            let test = self.test(pi);
            let from = might.idx.partition_point(|i| (*i as usize) < test.0);
            let (m_idx, m_w) = (&might.idx[from..], &might.w[from..]);
            if dense_get(&self.s.vis, pi) {
                let vis = &self.s.vis;
                let mut more = false;
                for (i, x) in m_idx.iter().zip(m_w) {
                    let i = *i as usize;
                    let Some(t) = test.1.get(i - test.0) else { break };
                    if x & t & !vis[i] != 0 {
                        more = true;
                        break;
                    }
                }
                if !more {
                    continue;
                }
            }
            let p = &w.portals[pi];
            let d = base.plane.dist(p.origin);
            if d < -p.radius {
                continue;
            }
            let pw = w.winding(pi);
            new_pass.clear();
            if d > p.radius {
                new_pass.extend_from_slice(pw);
            } else {
                match chop(pw, &base.plane, spare) {
                    Chopped::Same => new_pass.extend_from_slice(pw),
                    Chopped::Gone => continue,
                    Chopped::New => std::mem::swap(new_pass, spare),
                }
            }
            let d = p.plane.dist(base.origin);
            if d > base.radius {
                continue;
            }
            new_source.clear();
            let mut chopped = false;
            if d < -base.radius {
                new_source.extend_from_slice(source);
            } else {
                match chop(source, &p.plane.flip(), spare) {
                    Chopped::Same => new_source.extend_from_slice(source),
                    Chopped::Gone => continue,
                    Chopped::New => {
                        std::mem::swap(new_source, spare);
                        chopped = true;
                    }
                }
            }
            if let Some(prev_pass) = pass {
                let kept = if chopped {
                    clip_by_separators(new_pass, new_source, prev_pass, false, spare)
                        && clip_by_separators(new_pass, prev_pass, new_source, true, spare)
                } else {
                    clip_by(new_pass, &seps[0], spare) && clip_by(new_pass, &seps[1], spare)
                };
                if !kept
                    || !clip_by_separators(new_source, new_pass, prev_pass, false, spare)
                    || !clip_by_separators(new_source, prev_pass, new_pass, true, spare)
                {
                    continue;
                }
            }
            next.and(m_idx, m_w, test);
            dense_set(&mut self.s.vis, pi);
            self.through(pi, depth + 1, next.view(), new_source, new_pass);
        }
        self.s.frames[depth] = frame;
    }

    fn through(&mut self, pi: usize, depth: usize, might: Sv<'_>, source: &[DVec3], pass: &[DVec3]) {
        let w = self.w;
        let to = w.portals[pi].to;
        let how = self.through_how(pi, depth, might, source, pass);
        match how {
            Through::Skip => {}
            Through::Same => self.leaf_flow(to, depth, might, source, Some(pass)),
            Through::Wider { might, source, pass } => self.leaf_flow(to, depth, might.view(), &source, Some(&pass)),
        }
    }

    fn through_how(&mut self, pi: usize, depth: usize, might: Sv<'_>, source: &[DVec3], pass: &[DVec3]) -> Through {
        let w = self.w;
        let base = &w.portals[self.base];
        let p = &w.portals[pi];
        let boxes = [bounds(source), bounds(pass)];
        let kept = might.span_len();
        let m = &mut self.s.memo;
        let room = m.used + kept + 4 * CAP <= MEMO_WORDS;
        let k = match m.of[pi] {
            u32::MAX => {
                m.of[pi] = m.portals.len() as u32;
                m.portals.push(PortalMemo {
                    portal: pi as u32,
                    flows: [0; MEMO_FLOWS],
                    count: 0,
                    merges: 0,
                    saturated: false,
                });
                m.portals.len() - 1
            }
            k => k as usize,
        };
        let pm = &m.portals[k];
        if pm.saturated {
            return Through::Skip;
        }
        let flows = &pm.flows[..pm.count];
        if let Some(h) =
            flows.iter().position(|e| m.dominates(&m.memos[*e as usize], depth, source, pass, boxes, might))
        {
            m.portals[k].flows.swap(0, h);
            return Through::Skip;
        }
        if pm.count < MEMO_FLOWS && room {
            m.used += kept + 4 * CAP;
            let memo = Memo {
                depth,
                pass_box: boxes[1],
                source_box: boxes[0],
                source: m.add_pts(source),
                pass: m.add_pts(pass),
                might: m.add_might(might),
            };
            m.memos.push(memo);
            let pm = &mut m.portals[k];
            pm.flows[pm.count] = (m.memos.len() - 1) as u32;
            pm.count += 1;
            return Through::Same;
        }
        if pm.merges < MEMO_MERGES && room {
            let flows: Vec<&Memo> = pm.flows[..pm.count].iter().map(|e| &m.memos[*e as usize]).collect();
            let mut sources: Vec<DVec3> = source.to_vec();
            let mut passes: Vec<DVec3> = pass.to_vec();
            let mut d = depth;
            for e in &flows {
                sources.extend_from_slice(m.pts(e.source));
                passes.extend_from_slice(m.pts(e.pass));
                d = d.min(e.depth);
            }
            let hulls = (coplanar_hull(&sources, base.plane.n), coplanar_hull(&passes, p.plane.n));
            let holds = |s_h: &[DVec3], p_h: &[DVec3]| {
                polygon_contains(s_h, source)
                    && polygon_contains(p_h, pass)
                    && flows
                        .iter()
                        .all(|e| polygon_contains(s_h, m.pts(e.source)) && polygon_contains(p_h, m.pts(e.pass)))
            };
            if let (Some(s_h), Some(p_h)) = hulls
                && holds(&s_h, &p_h)
            {
                let mut all: Vec<(u32, u64)> = might.idx.iter().copied().zip(might.w.iter().copied()).collect();
                for e in &flows {
                    let em = m.might(e.might);
                    all.extend(em.idx.iter().copied().zip(em.w.iter().copied()));
                }
                all.sort_unstable_by_key(|x| x.0);
                let mut union = SparseBuf::default();
                for (i, x) in all {
                    match (union.idx.last(), union.w.last_mut()) {
                        (Some(l), Some(u)) if *l == i => *u |= x,
                        _ => union.push(i as usize, x),
                    }
                }
                let merged = Memo {
                    depth: d,
                    pass_box: bounds(&p_h),
                    source_box: bounds(&s_h),
                    source: m.add_pts(&s_h),
                    pass: m.add_pts(&p_h),
                    might: m.add_might(union.view()),
                };
                m.memos.push(merged);
                let pm = &mut m.portals[k];
                pm.merges += 1;
                pm.flows[0] = (m.memos.len() - 1) as u32;
                pm.count = 1;
                return Through::Wider { might: union, source: s_h, pass: p_h };
            }
        }
        let pm = &mut m.portals[k];
        pm.saturated = true;
        pm.count = 0;
        let fb = self.flood[self.base].view();
        let t = self.test(pi);
        let lo = fb.0.max(t.0);
        let hi = (fb.0 + fb.1.len()).min(t.0 + t.1.len());
        let mut all = SparseBuf::default();
        for k in lo..hi.max(lo) {
            let x = fb.1[k - fb.0] & t.1[k - t.0];
            if x != 0 {
                all.push(k, x);
            }
        }
        Through::Wider { might: all, source: w.winding(self.base).to_vec(), pass: w.winding(pi).to_vec() }
    }
}

fn portal_flow(mut f: Flow<'_, '_>) -> Bits {
    let w = f.w;
    let base = f.base;
    f.s.vis.clear();
    f.s.vis.resize(w.words, 0);
    f.s.memo.reset(w.portals.len());
    let flood = f.flood[base].view();
    let mut root = std::mem::take(&mut f.s.root);
    root.clear();
    let back = w.rev[base] as usize;
    for (k, word) in flood.1.iter().enumerate() {
        let mut word = *word;
        let mut bits = word;
        while bits != 0 {
            let b = bits.trailing_zeros();
            bits &= bits - 1;
            let q = (flood.0 + k) * 64 + b as usize;
            let rq = w.rev[q] as usize;
            if w.radius2.is_none()
                && f.round_of[rq] < f.limit
                && let Some(v) = f.done[rq].get()
            {
                if view_get(v.view(), back) {
                    dense_set(&mut f.s.vis, q);
                } else {
                    word &= !(1u64 << b);
                }
            }
        }
        if word != 0 {
            root.push(flood.0 + k, word);
        }
    }
    let source = w.winding(base);
    let to = w.portals[base].to;
    f.leaf_flow(to, 0, root.view(), source, None);
    f.s.root = root;
    Bits::from_dense(&f.s.vis)
}

thread_local!(static SCRATCH: RefCell<FlowScratch> = RefCell::new(FlowScratch::default()));

#[allow(clippy::too_many_arguments)]
fn flow_all(
    pool: &Pool,
    w: &World,
    flood: &[Bits],
    sort: bool,
    bounded: bool,
    isa: simd::Isa,
    progress: &mut dyn FnMut(f32),
    from: f32,
) -> Vec<Bits> {
    let np = w.portals.len();
    let counts: Vec<usize> = flood.iter().map(Bits::count).collect();
    if w.radius2.is_some() {
        return flow_independent(pool, w, flood, bounded, isa, &counts, progress, from);
    }
    let mut order: Vec<usize> = (0..np).collect();
    if sort {
        order.sort_by_key(|&p| (counts[p], p));
    } else {
        order.sort_by_key(|&p| w.vidx[p]);
    }
    let mut rounds = Vec::new();
    let mut i = 0;
    while i < np {
        let size = (i / 8).max(64).min(np - i);
        rounds.push(i..i + size);
        i += size;
    }
    let mut round_of = vec![0u32; np];
    for (r, range) in rounds.iter().enumerate() {
        for &pi in &order[range.clone()] {
            round_of[pi] = r as u32;
        }
    }
    let done: Vec<OnceLock<Bits>> = (0..np).map(|_| OnceLock::new()).collect();
    let remaining: Vec<AtomicUsize> = rounds.iter().map(|r| AtomicUsize::new(r.len())).collect();
    let finished = AtomicU64::new(0);
    let total: f64 = counts.iter().map(|c| *c as f64 + 1.0).sum::<f64>().max(1.0);
    let main = std::thread::current();
    let (done_ref, round_of, remaining_ref, finished_ref, counts, main_ref) =
        (&done, &round_of, &remaining, &finished, &counts, &main);
    pool.scope(|sc| {
        let mut wait = |r: usize| {
            while remaining_ref[r].load(Ordering::Acquire) != 0 {
                let f = finished_ref.load(Ordering::Relaxed) as f64 / total;
                progress((f64::from(from) + f64::from(1.0 - from) * f) as f32);
                std::thread::park_timeout(std::time::Duration::from_millis(50));
            }
        };
        for (r, range) in rounds.iter().enumerate() {
            if r >= WINDOW {
                wait(r - WINDOW);
            }
            let limit = if w.radius2.is_some() { 0 } else { (r + 1).saturating_sub(WINDOW) as u32 };
            for &pi in &order[range.clone()] {
                sc.spawn(move |_| {
                    let bits = if bounded {
                        SCRATCH.with_borrow_mut(|s| {
                            portal_flow(Flow { w, flood, done: done_ref, round_of, limit, base: pi, s, isa })
                        })
                    } else {
                        VFlow::new(w, flood, Done::Rounds { done: done_ref, round_of, limit }, None).run(pi)
                    };
                    let _ = done_ref[pi].set(bits);
                    finished_ref.fetch_add(counts[pi] as u64 + 1, Ordering::Relaxed);
                    if remaining_ref[r].fetch_sub(1, Ordering::AcqRel) == 1 {
                        main_ref.unpark();
                    }
                });
            }
        }
        for r in 0..rounds.len() {
            wait(r);
        }
    });
    done.into_iter().map(|b| b.into_inner().unwrap_or_default()).collect()
}

#[allow(clippy::too_many_arguments)]
fn flow_independent(
    pool: &Pool,
    w: &World,
    flood: &[Bits],
    bounded: bool,
    isa: simd::Isa,
    counts: &[usize],
    progress: &mut dyn FnMut(f32),
    from: f32,
) -> Vec<Bits> {
    let np = w.portals.len();
    let mut order: Vec<usize> = (0..np).collect();
    order.sort_by_key(|&p| (std::cmp::Reverse(counts[p]), p));
    let done: Vec<OnceLock<Bits>> = (0..np).map(|_| OnceLock::new()).collect();
    let round_of = vec![0u32; np];
    let next = AtomicUsize::new(0);
    let finished = AtomicU64::new(0);
    let total: f64 = counts.iter().map(|c| *c as f64 + 1.0).sum::<f64>().max(1.0);
    let (done_ref, round_of, order, next_ref, finished_ref) = (&done, &round_of, &order, &next, &finished);
    let workers = pool.threads();
    let left = AtomicUsize::new(workers);
    let left_ref = &left;
    let main = std::thread::current();
    let main_ref = &main;
    pool.scope(|sc| {
        for _ in 0..workers {
            sc.spawn(move |_| {
                loop {
                    let k = next_ref.fetch_add(1, Ordering::Relaxed);
                    let Some(&pi) = order.get(k) else { break };
                    let bits = if bounded {
                        SCRATCH.with_borrow_mut(|s| {
                            portal_flow(Flow { w, flood, done: done_ref, round_of, limit: 0, base: pi, s, isa })
                        })
                    } else {
                        VFlow::new(w, flood, Done::Rounds { done: done_ref, round_of, limit: 0 }, None).run(pi)
                    };
                    let _ = done_ref[pi].set(bits);
                    finished_ref.fetch_add(counts[pi] as u64 + 1, Ordering::Relaxed);
                }
                if left_ref.fetch_sub(1, Ordering::AcqRel) == 1 {
                    main_ref.unpark();
                }
            });
        }
        while left_ref.load(Ordering::Acquire) != 0 {
            let f = finished_ref.load(Ordering::Relaxed) as f64 / total;
            progress((f64::from(from) + f64::from(1.0 - from) * f) as f32);
            std::thread::park_timeout(std::time::Duration::from_millis(50));
        }
    });
    done.into_iter().map(|b| b.into_inner().unwrap_or_default()).collect()
}

struct Pool(Option<rayon::ThreadPool>);

impl Pool {
    fn new(threads: Option<usize>) -> Pool {
        let builder = |stack: bool| {
            let mut b = rayon::ThreadPoolBuilder::new();
            if stack {
                b = b.stack_size(STACK_SIZE);
            }
            if let Some(t) = threads {
                b = b.num_threads(t.clamp(1, 1024));
            }
            b.build().ok()
        };
        Pool(builder(true).or_else(|| builder(false)))
    }

    fn scope<'s, R>(&self, f: impl FnOnce(&rayon::Scope<'s>) -> R) -> R {
        match &self.0 {
            Some(p) => p.in_place_scope(f),
            None => rayon::in_place_scope(f),
        }
    }

    fn threads(&self) -> usize {
        match &self.0 {
            Some(p) => p.current_num_threads(),
            None => rayon::current_num_threads(),
        }
    }

    fn run<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        match &self.0 {
            Some(p) => p.install(f),
            None => f(),
        }
    }
}

fn row_bytes(n: usize) -> usize {
    n.div_ceil(8)
}

fn full_rows(n: usize) -> Vec<Vec<u8>> {
    let mut row = vec![0xffu8; row_bytes(n)];
    if !n.is_multiple_of(8)
        && let Some(last) = row.last_mut()
    {
        *last = (1u8 << (n % 8)) - 1;
    }
    vec![row; n]
}

pub fn compute_pvs(
    cluster_count: usize,
    portals: &[VisPortal],
    opts: &VisOptions,
    progress: &mut dyn FnMut(f32),
) -> Vec<Vec<u8>> {
    compute_pvs_with_radius(cluster_count, portals, opts, None, progress)
}

pub fn compute_pvs_with_radius(
    cluster_count: usize,
    portals: &[VisPortal],
    opts: &VisOptions,
    radius: Option<f64>,
    progress: &mut dyn FnMut(f32),
) -> Vec<Vec<u8>> {
    pvs_rows(cluster_count, portals, opts, radius, None, progress)
}

pub fn compute_fast_pvs_refined(
    cluster_count: usize,
    portals: &[VisPortal],
    opts: &VisOptions,
    radius: Option<f64>,
    refine: &dyn Fn(&[Vec<u8>]) -> Vec<usize>,
    progress: &mut dyn FnMut(f32),
) -> Vec<Vec<u8>> {
    pvs_rows(cluster_count, portals, &VisOptions { fast: true, ..*opts }, radius, Some(refine), progress)
}

pub fn sky_clusters(leafs: &[u8], leaffaces: &[u8], faces: &[u8], texinfo: &[u8], clusters: usize) -> Vec<u8> {
    use crate::materials::surf;
    let mut out = vec![0u8; clusters];
    let u16_at = |d: &[u8], o: usize| d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    for leaf in leafs.chunks_exact(32) {
        let cluster = i16::from_le_bytes([leaf[4], leaf[5]]);
        let Some(slot) = usize::try_from(cluster).ok().and_then(|c| out.get_mut(c)) else { continue };
        let (first, count) = (usize::from(u16_at(leaf, 20)), usize::from(u16_at(leaf, 22)));
        for k in first..first + count {
            let face = usize::from(u16_at(leaffaces, k * 2));
            let Some(ti) = faces.get(face * 56 + 10..face * 56 + 12).map(|b| i16::from_le_bytes([b[0], b[1]])) else {
                continue;
            };
            let Some(flags) = usize::try_from(ti).ok().and_then(|t| texinfo.get(t * 72 + 64..t * 72 + 68)) else {
                continue;
            };
            let flags = u32::from_le_bytes([flags[0], flags[1], flags[2], flags[3]]);
            if flags & surf::SKY != 0 {
                *slot |= if flags & surf::SKY2D != 0 { SKY_2D } else { SKY_3D };
            }
        }
    }
    out
}

pub const SKY_3D: u8 = 1;
pub const SKY_2D: u8 = 2;

pub fn compute_pvs_for_sky(
    cluster_count: usize,
    portals: &[VisPortal],
    opts: &VisOptions,
    radius: Option<f64>,
    sky: &[u8],
    progress: &mut dyn FnMut(f32),
) -> Vec<Vec<u8>> {
    if !opts.fast || !sky.iter().any(|k| k & SKY_2D != 0) || !sky.iter().any(|k| k & SKY_3D != 0) {
        return compute_pvs_with_radius(cluster_count, portals, opts, radius, progress);
    }
    let pick = |rows: &[Vec<u8>]| -> Vec<usize> {
        let sees = |row: &[u8], bit: u8| {
            sky.iter()
                .enumerate()
                .any(|(c, k)| k & bit != 0 && row.get(c >> 3).is_some_and(|b| b & (1 << (c & 7)) != 0))
        };
        (0..rows.len())
            .filter(|&c| {
                let own = sky.get(c).copied().unwrap_or(0);
                own & SKY_3D == 0 && (own & SKY_2D != 0 || sees(&rows[c], SKY_2D)) && sees(&rows[c], SKY_3D)
            })
            .collect()
    };
    compute_fast_pvs_refined(cluster_count, portals, opts, radius, &pick, progress)
}

type RowPicker<'a> = dyn Fn(&[Vec<u8>]) -> Vec<usize> + 'a;

fn pvs_rows(
    cluster_count: usize,
    portals: &[VisPortal],
    opts: &VisOptions,
    radius: Option<f64>,
    refine: Option<&RowPicker<'_>>,
    progress: &mut dyn FnMut(f32),
) -> Vec<Vec<u8>> {
    let n = cluster_count;
    if n > MAX_CLUSTERS {
        return Vec::new();
    }
    if portals.len() > MAX_PORTALS {
        progress(1.0);
        return full_rows(n);
    }
    progress(0.0);
    let pool = Pool::new(opts.threads);
    let (w, neighbours) = World::new(n, portals, vis_radius2(opts, radius));
    let np = w.portals.len();
    let words = w.words;

    let base_share = if opts.fast { 0.9 } else { 0.05 };
    let mut flood: Vec<Bits> = Vec::new();
    let flooded = AtomicUsize::new(0);
    let finished = AtomicBool::new(false);
    let main = std::thread::current();
    pool.scope(|sc| {
        let (w, flood, flooded, finished, main) = (&w, &mut flood, &flooded, &finished, &main);
        sc.spawn(move |_| {
            *flood = (0..np)
                .into_par_iter()
                .map_init(
                    || BaseScratch::new(words),
                    |s, pi| {
                        let b = base_flood(w, pi, s);
                        flooded.fetch_add(1, Ordering::Relaxed);
                        b
                    },
                )
                .collect();
            finished.store(true, Ordering::Release);
            main.unpark();
        });
        while !finished.load(Ordering::Acquire) {
            progress(base_share * flooded.load(Ordering::Relaxed) as f32 / np.max(1) as f32);
            std::thread::park_timeout(std::time::Duration::from_millis(50));
        }
    });
    progress(base_share);

    let bounded = !opts.vvis_flow;
    let vis: Vec<Bits> = if opts.fast {
        flood
    } else {
        flow_all(&pool, &w, &flood, opts.sort, bounded, opts.simd.unwrap_or_else(simd::isa), progress, base_share)
    };

    let rb = row_bytes(n);
    let mut rows: Vec<Vec<u8>> = pool.run(|| {
        (0..n)
            .into_par_iter()
            .map_init(
                || vec![0u64; words],
                |acc, c| {
                    acc.fill(0);
                    for pi in w.leaf(c as u32) {
                        dense_or(acc, vis[pi].view());
                        dense_set(acc, pi);
                    }
                    let mut row = vec![0u8; rb];
                    row[c >> 3] |= 1 << (c & 7);
                    for_each_bit(acc, |q| {
                        if let Some(p) = w.portals.get(q) {
                            let t = p.to as usize;
                            row[t >> 3] |= 1 << (t & 7);
                        }
                    });
                    row
                },
            )
            .collect()
    });
    for &[a, b] in &neighbours {
        let (a, b) = (a as usize, b as usize);
        rows[a][b >> 3] |= 1 << (b & 7);
        rows[b][a >> 3] |= 1 << (a & 7);
    }
    let mut rows = pool.run(|| crosscheck(&rows));
    if let Some(pick) = refine
        && opts.fast
    {
        let picked = pick(&rows);
        refine_rows(&pool, &w, &vis, opts, &neighbours, &picked, &mut rows);
    }
    progress(1.0);
    rows
}

fn refine_rows(
    pool: &Pool,
    w: &World,
    flood: &[Bits],
    opts: &VisOptions,
    neighbours: &[[u32; 2]],
    picked: &[usize],
    rows: &mut [Vec<u8>],
) {
    let n = rows.len();
    let mut picked: Vec<usize> = picked.iter().copied().filter(|&c| c < n).collect();
    picked.sort_unstable();
    picked.dedup();
    let portals: Vec<usize> = picked.iter().flat_map(|&c| w.leaf(c as u32)).collect();
    if portals.is_empty() {
        return;
    }
    let np = w.portals.len();
    let done: Vec<OnceLock<Bits>> = (0..np).map(|_| OnceLock::new()).collect();
    let round_of = vec![0u32; np];
    let bounded = !opts.vvis_flow;
    let (done, round_of) = (&done, &round_of);
    let seen: Vec<Bits> = pool.run(|| {
        portals
            .par_iter()
            .map(|&pi| {
                if bounded {
                    SCRATCH.with_borrow_mut(|s| {
                        portal_flow(Flow {
                            w,
                            flood,
                            done,
                            round_of,
                            limit: 0,
                            base: pi,
                            s,
                            isa: opts.simd.unwrap_or_else(simd::isa),
                        })
                    })
                } else {
                    VFlow::new(w, flood, Done::Rounds { done, round_of, limit: 0 }, None).run(pi)
                }
            })
            .collect()
    });
    let mut acc = vec![0u64; w.words];
    let mut k = 0;
    for &c in &picked {
        acc.fill(0);
        for pi in w.leaf(c as u32) {
            dense_or(&mut acc, seen[k].view());
            dense_set(&mut acc, pi);
            k += 1;
        }
        let mut row = vec![0u8; rows[c].len()];
        row[c >> 3] |= 1 << (c & 7);
        for_each_bit(&acc, |q| {
            if let Some(p) = w.portals.get(q) {
                let t = p.to as usize;
                row[t >> 3] |= 1 << (t & 7);
            }
        });
        for &[a, b] in neighbours {
            let (a, b) = (a as usize, b as usize);
            if a == c {
                row[b >> 3] |= 1 << (b & 7);
            } else if b == c {
                row[a >> 3] |= 1 << (a & 7);
            }
        }
        for (r, old) in row.iter_mut().zip(&rows[c]) {
            *r &= *old;
        }
        rows[c] = row;
    }
}

fn vis_radius2(opts: &VisOptions, fog: Option<f64>) -> Option<f64> {
    if let Some(r) = opts.radius_override {
        return Some(r * r);
    }
    let r = fog? as f32;
    (r > 0.0).then(|| f64::from(r * r))
}

fn crosscheck(rows: &[Vec<u8>]) -> Vec<Vec<u8>> {
    rows.par_iter()
        .enumerate()
        .map(|(a, row)| {
            let mut out = row.clone();
            for (k, byte) in row.iter().enumerate() {
                let mut bits = *byte;
                while bits != 0 {
                    let b = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let c = k * 8 + b;
                    let back = rows.get(c).and_then(|r| r.get(a >> 3)).is_some_and(|x| x & (1 << (a & 7)) != 0);
                    if c != a && !back {
                        out[k] &= !(1u8 << b);
                    }
                }
            }
            out
        })
        .collect()
}

fn row_words(row: &[u8], n: usize) -> Vec<u64> {
    let mut out = vec![0u64; n.div_ceil(64)];
    for (i, b) in row.iter().take(row_bytes(n)).enumerate() {
        out[i >> 3] |= u64::from(*b) << ((i & 7) * 8);
    }
    if !n.is_multiple_of(64)
        && let Some(last) = out.last_mut()
    {
        *last &= (1u64 << (n % 64)) - 1;
    }
    out
}

pub fn compute_pas(pvs: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let n = pvs.len();
    let rb = row_bytes(n);
    let rows: Vec<Vec<u64>> = pvs.par_iter().map(|r| row_words(r, n)).collect();
    let state = ahash::RandomState::with_seeds(1, 2, 3, 4);
    let hashes: Vec<u64> = rows.par_iter().map(|r| state.hash_one(r)).collect();
    let mut by_hash: AHashMap<u64, Vec<usize>> = AHashMap::with_capacity(n);
    let of: Vec<usize> = (0..n)
        .map(|c| {
            let same = by_hash.entry(hashes[c]).or_default();
            match same.iter().find(|f| rows[**f] == rows[c]) {
                Some(f) => *f,
                None => {
                    same.push(c);
                    c
                }
            }
        })
        .collect();
    let unique: Vec<usize> = (0..n).filter(|c| of[*c] == *c).collect();
    let heard: Vec<Vec<u8>> = unique
        .par_iter()
        .map(|&c| {
            let r = &rows[c];
            let mut acc = r.clone();
            for_each_bit(r, |v| {
                for (a, b) in acc.iter_mut().zip(&rows[v]) {
                    *a |= *b;
                }
            });
            (0..rb).map(|i| (acc[i >> 3] >> ((i & 7) * 8)) as u8).collect()
        })
        .collect();
    let mut slot = vec![0usize; n];
    for (k, c) in unique.iter().enumerate() {
        slot[*c] = k;
    }
    (0..n).into_par_iter().map(|c| heard[slot[of[c]]].clone()).collect()
}

pub fn compute_pas_with(pvs: &[Vec<u8>], opts: &VisOptions) -> Vec<Vec<u8>> {
    Pool::new(opts.threads).run(|| compute_pas(pvs))
}

fn compress_row(row: &[u8], out: &mut Vec<u8>) {
    let mut j = 0;
    while j < row.len() {
        out.push(row[j]);
        if row[j] != 0 {
            j += 1;
            continue;
        }
        let mut rep = 1u8;
        j += 1;
        while j < row.len() && row[j] == 0 && rep < 255 {
            rep += 1;
            j += 1;
        }
        out.push(rep);
    }
}

pub fn vis_lump(pvs: &[Vec<u8>], pas: &[Vec<u8>]) -> Vec<u8> {
    let n = pvs.len();
    let rb = row_bytes(n);
    let header = 4 + 8 * n;
    let mut out = vec![0u8; header];
    out[..4].copy_from_slice(&(n as i32).to_le_bytes());
    let mut row = vec![0u8; rb];
    for kind in 0..2 {
        for (c, own) in pvs.iter().enumerate() {
            let src = if kind == 0 { own } else { pas.get(c).unwrap_or(own) };
            row.fill(0);
            let len = src.len().min(rb);
            row[..len].copy_from_slice(&src[..len]);
            if !n.is_multiple_of(8) {
                row[rb - 1] &= (1u8 << (n % 8)) - 1;
            }
            let at = 4 + 8 * c + 4 * kind;
            let ofs = out.len() as u32;
            out[at..at + 4].copy_from_slice(&ofs.to_le_bytes());
            compress_row(&row, &mut out);
        }
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisTrace {
    pub points: Vec<DVec3>,
}

impl VisTrace {
    pub fn lines(&self) -> Vec<[DVec3; 2]> {
        self.points.windows(2).map(|w| [w[0], w[1]]).collect()
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        for p in &self.points {
            s.push_str(&format!("{:.6} {:.6} {:.6}\r\n", p.x, p.y, p.z));
        }
        s
    }
}

pub fn trace_file_name(map: &str) -> String {
    let name_at = map.rfind(['/', '\\']).map_or(0, |i| i + 1);
    let stem = match map[name_at..].rfind('.') {
        Some(dot) => &map[..name_at + dot],
        None => map,
    };
    format!("{stem}.lin")
}

pub fn compute_trace(
    cluster_count: usize,
    portals: &[VisPortal],
    opts: &VisOptions,
    radius: Option<f64>,
) -> Result<Option<VisTrace>, String> {
    let Some((start, stop)) = opts.trace else { return Ok(None) };
    let n = cluster_count;
    if start as usize >= n || stop as usize >= n {
        return Err(format!("Invalid cluster trace: {start} to {stop}, valid range is 0 to {}", n as i64 - 1));
    }
    if n > MAX_CLUSTERS || portals.len() > MAX_PORTALS {
        return Err(format!("Too many clusters ({n}) or portals ({}) to trace", portals.len()));
    }
    let pool = Pool::new(opts.threads);
    let (w, _) = World::new(n, portals, vis_radius2(opts, radius));
    let words = w.words;
    let flood: Vec<Bits> = pool.run(|| {
        (0..w.portals.len())
            .into_par_iter()
            .map_init(|| BaseScratch::new(words), |s, pi| base_flood(&w, pi, s))
            .collect()
    });
    let found = pool.run(|| {
        let mut done: Vec<Option<Bits>> = vec![None; w.portals.len()];
        for pi in w.leaf(start) {
            let mut f = VFlow::new(&w, &flood, Done::List(&done), Some(stop));
            let vis = f.run(pi);
            if f.found.is_some() {
                return f.found;
            }
            done[pi] = Some(vis);
        }
        None
    });
    Ok(found.map(|points| {
        let mut all = vec![cluster_center(&w, start)];
        all.extend(points);
        all.push(cluster_center(&w, stop));
        VisTrace { points: all.iter().map(|p| DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))).collect() }
    }))
}

fn cluster_center(w: &World, c: u32) -> V3 {
    let (mut lo, mut hi) = ([99999.0f32; 3], [-99999.0f32; 3]);
    for pi in w.leaf(c) {
        for p in w.vwinding(pi) {
            for k in 0..3 {
                if p[k] < lo[k] {
                    lo[k] = p[k];
                }
                if p[k] > hi[k] {
                    hi[k] = p[k];
                }
            }
        }
    }
    [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5]
}

fn winding_center(w: &[V3]) -> V3 {
    let mut c = [0.0f32; 3];
    for p in w {
        for k in 0..3 {
            c[k] += p[k];
        }
    }
    let scale = (1.0f64 / w.len() as f64) as f32;
    [c[0] * scale, c[1] * scale, c[2] * scale]
}

#[derive(Clone, Copy)]
enum Done<'a> {
    List(&'a [Option<Bits>]),
    Rounds { done: &'a [OnceLock<Bits>], round_of: &'a [u32], limit: u32 },
}

impl<'a> Done<'a> {
    #[inline]
    fn get(self, q: usize) -> Option<&'a Bits> {
        match self {
            Done::List(l) => l[q].as_ref(),
            Done::Rounds { done, round_of, limit } => {
                if round_of[q] < limit {
                    done[q].get()
                } else {
                    None
                }
            }
        }
    }
}

struct VFlow<'a> {
    w: &'a World,
    flood: &'a [Bits],
    done: Done<'a>,
    base: usize,
    lo: usize,
    vis: Vec<u64>,
    frames: Vec<Vec<u64>>,
    stop: Option<u32>,
    chain: Vec<Vec<V3>>,
    found: Option<Vec<V3>>,
}

impl<'a> VFlow<'a> {
    fn new(w: &'a World, flood: &'a [Bits], done: Done<'a>, stop: Option<u32>) -> Self {
        VFlow {
            w,
            flood,
            done,
            base: 0,
            lo: 0,
            vis: Vec::new(),
            frames: Vec::new(),
            stop,
            chain: Vec::new(),
            found: None,
        }
    }

    fn run(&mut self, pi: usize) -> Bits {
        let (lo, words) = self.flood[pi].view();
        self.base = pi;
        self.lo = lo;
        self.vis.clear();
        self.vis.resize(words.len(), 0);
        let source = self.w.vwinding(pi).to_vec();
        self.leaf_flow(self.w.portals[pi].to, 0, words, &source, None);
        Bits::from_span(lo, &self.vis)
    }

    #[inline]
    fn get(&self, v: &[u64], i: usize) -> bool {
        let k = (i >> 6).wrapping_sub(self.lo);
        k < v.len() && (v[k] >> (i & 63)) & 1 != 0
    }

    fn dump(&mut self) {
        let mut out = Vec::new();
        let head = self.w.vwinding(self.base).to_vec();
        for w in std::iter::once(&head).chain(self.chain.iter()) {
            let mid = winding_center(w);
            out.push(mid);
            for p in w {
                out.push(*p);
                out.push(mid);
            }
            out.extend_from_slice(w);
            out.push(w[0]);
            out.push(mid);
        }
        self.found = Some(out);
    }

    fn leaf_flow(&mut self, leaf: u32, depth: usize, might: &[u64], source: &[V3], pass: Option<&[V3]>) {
        if self.stop == Some(leaf) {
            if self.found.is_none() {
                self.dump();
            }
            return;
        }
        let w = self.w;
        let flood = self.flood;
        let base = w.vv[self.base];
        if self.frames.len() <= depth {
            self.frames.resize_with(depth + 1, Vec::new);
        }
        let mut next = std::mem::take(&mut self.frames[depth]);
        next.clear();
        next.resize(might.len(), 0);
        for pi in w.leaf(leaf) {
            if !self.get(might, pi) {
                continue;
            }
            let (t0, t) = self.done.get(pi).unwrap_or(&flood[pi]).view();
            let mut more = false;
            for (k, x) in next.iter_mut().enumerate() {
                let g = (self.lo + k).wrapping_sub(t0);
                *x = might[k] & t.get(g).copied().unwrap_or(0);
                more |= *x & !self.vis[k] != 0;
            }
            if !more && self.get(&self.vis, pi) {
                continue;
            }
            let p = w.vv[pi];
            let d = base.plane.dist(p.origin);
            let new_pass = if d < -p.radius {
                continue;
            } else if d > p.radius {
                w.vwinding(pi).to_vec()
            } else {
                let Some(x) = vvis_chop(w.vwinding(pi), &base.plane) else { continue };
                x
            };
            let d = p.plane.dist(base.origin);
            let new_source = if d > base.radius {
                continue;
            } else if d < -base.radius {
                source.to_vec()
            } else {
                let Some(x) = vvis_chop(source, &p.plane.flip()) else { continue };
                x
            };
            let new_pass = match pass {
                None => new_pass,
                Some(prev) => {
                    let Some(x) = vvis_clip_to_separators(&new_source, prev, new_pass, false) else { continue };
                    let Some(x) = vvis_clip_to_separators(prev, &new_source, x, true) else { continue };
                    x
                }
            };
            let k = (pi >> 6) - self.lo;
            self.vis[k] |= 1 << (pi & 63);
            if self.stop.is_some() {
                self.chain.push(new_pass.clone());
            }
            self.leaf_flow(w.portals[pi].to, depth + 1, &next, &new_source, Some(&new_pass));
            if self.stop.is_some() {
                self.chain.pop();
                if self.found.is_some() {
                    break;
                }
            }
        }
        self.frames[depth] = next;
    }
}

pub fn parse_prt(text: &str) -> Option<(usize, Vec<VisPortal>)> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if !lines.next()?.eq_ignore_ascii_case("PRT1") {
        return None;
    }
    let mut number = || -> Option<usize> { lines.next()?.split_ascii_whitespace().next()?.parse().ok() };
    let clusters = number()?;
    let declared = number()?;
    if clusters > MAX_CLUSTERS || declared > MAX_PORTALS {
        return None;
    }
    let mut portals = Vec::with_capacity(declared.min(text.len() / 16 + 1));
    for _ in 0..declared {
        let line = lines.next()?;
        let paren = line.find('(')?;
        let mut head = line[..paren].split_ascii_whitespace();
        let count: usize = head.next()?.parse().ok()?;
        let a: u32 = head.next()?.parse().ok()?;
        let b: u32 = head.next()?.parse().ok()?;
        if !(3..=MAX_POINTS).contains(&count) || a as usize >= clusters || b as usize >= clusters {
            return None;
        }
        let mut points = Vec::with_capacity(count);
        for group in line[paren..].split('(').skip(1) {
            let group = group.trim().trim_end_matches(')');
            let mut v = [0.0f64; 3];
            let mut it = group.split_ascii_whitespace();
            for slot in &mut v {
                let x: f64 = it.next()?.parse().ok()?;
                if !x.is_finite() || x.abs() > MAX_COORD {
                    return None;
                }
                *slot = x;
            }
            if points.len() == count {
                return None;
            }
            points.push(DVec3::from_array(v));
        }
        if points.len() != count {
            return None;
        }
        portals.push(VisPortal { points, clusters: [a, b] });
    }
    Some((clusters, portals))
}

#[cfg(test)]
#[allow(clippy::needless_range_loop)]
mod tests {
    use super::*;

    fn decompress(lump: &[u8], c: usize, kind: usize) -> Option<Vec<bool>> {
        let i32_at = |o: usize| lump.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let n = usize::try_from(i32_at(0)?).ok()?;
        if c >= n {
            return None;
        }
        let mut pos = usize::try_from(i32_at(4 + c * 8 + kind * 4)?).ok()?;
        let row = n.div_ceil(8);
        let mut bits: Vec<u8> = Vec::with_capacity(row);
        while bits.len() < row {
            let b = *lump.get(pos)?;
            pos += 1;
            if b != 0 {
                bits.push(b);
            } else {
                let run = usize::from(*lump.get(pos)?);
                pos += 1;
                if run == 0 {
                    return None;
                }
                let run = run.min(row - bits.len());
                bits.extend(std::iter::repeat_n(0u8, run));
            }
        }
        Some((0..n).map(|i| bits[i / 8] & (1 << (i % 8)) != 0).collect())
    }

    fn sees(rows: &[Vec<u8>], a: usize, b: usize) -> bool {
        rows[a][b >> 3] & (1 << (b & 7)) != 0
    }

    fn portal(pts: [DVec3; 4], a: u32, b: u32, ca: DVec3) -> VisPortal {
        let mut points = pts.to_vec();
        let normal = (points[2] - points[0]).cross(points[1] - points[0]);
        if normal.dot(ca - points[0]) < 0.0 {
            points.reverse();
        }
        VisPortal { points, clusters: [a, b] }
    }

    fn wall(axis: usize, at: f64, lo: DVec3, hi: DVec3, a: u32, b: u32, ca: DVec3) -> VisPortal {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let corner = |su: f64, sv: f64| {
            let mut p = DVec3::ZERO;
            p[axis] = at;
            p[u] = su;
            p[v] = sv;
            p
        };
        portal([corner(lo[u], lo[v]), corner(hi[u], lo[v]), corner(hi[u], hi[v]), corner(lo[u], hi[v])], a, b, ca)
    }

    fn run(n: usize, portals: &[VisPortal], fast: bool) -> Vec<Vec<u8>> {
        compute_pvs(n, portals, &VisOptions { fast, threads: Some(2), ..Default::default() }, &mut |_| {})
    }

    fn corridor() -> Vec<VisPortal> {
        let cell = |x: f64, y: f64| DVec3::new(x + 32.0, y + 32.0, 32.0);
        let c = [
            cell(0.0, 0.0),
            cell(64.0, 0.0),
            cell(128.0, 0.0),
            cell(128.0, 64.0),
            cell(128.0, 128.0),
            cell(64.0, 128.0),
            cell(0.0, 128.0),
        ];
        let lo = |y: f64| DVec3::new(0.0, y, 0.0);
        let hi = |y: f64| DVec3::new(0.0, y + 64.0, 64.0);
        vec![
            wall(0, 64.0, lo(0.0), hi(0.0), 0, 1, c[0]),
            wall(0, 128.0, lo(0.0), hi(0.0), 1, 2, c[1]),
            wall(1, 64.0, DVec3::new(128.0, 0.0, 0.0), DVec3::new(192.0, 0.0, 64.0), 2, 3, c[2]),
            wall(1, 128.0, DVec3::new(128.0, 0.0, 0.0), DVec3::new(192.0, 0.0, 64.0), 3, 4, c[3]),
            wall(0, 128.0, lo(128.0), hi(128.0), 4, 5, c[4]),
            wall(0, 64.0, lo(128.0), hi(128.0), 5, 6, c[5]),
        ]
    }

    #[test]
    fn corridor_with_corners() {
        let p = corridor();
        for fast in [false, true] {
            let rows = run(7, &p, fast);
            assert_eq!(rows.len(), 7);
            for c in 0..7 {
                assert!(sees(&rows, c, c), "{c} sees itself");
            }
            for q in &p {
                let [a, b] = q.clusters.map(|x| x as usize);
                assert!(sees(&rows, a, b) && sees(&rows, b, a), "neighbours {a} {b}");
            }
            assert!(sees(&rows, 0, 2) && sees(&rows, 0, 3));
            if !fast {
                for (a, b) in [(0, 5), (0, 6), (6, 0), (6, 1), (1, 6), (5, 0)] {
                    assert!(!sees(&rows, a, b), "{a} must not see {b}");
                }
            }
        }
        let (full, fast) = (run(7, &p, false), run(7, &p, true));
        for c in 0..7 {
            for d in 0..7 {
                assert!(!sees(&full, c, d) || sees(&fast, c, d));
            }
        }
    }

    #[test]
    fn wall_between_two_rooms() {
        let ca = DVec3::new(128.0, 128.0, 64.0);
        let ch = DVec3::new(132.0, 288.0, 64.0);
        let ch2 = DVec3::new(396.0, 288.0, 64.0);
        let p = vec![
            wall(1, 256.0, DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 0.0, 128.0), 0, 2, ca),
            wall(0, 264.0, DVec3::new(0.0, 256.0, 0.0), DVec3::new(0.0, 320.0, 128.0), 2, 3, ch),
            wall(1, 256.0, DVec3::new(464.0, 0.0, 0.0), DVec3::new(528.0, 0.0, 128.0), 3, 1, ch2),
        ];
        let rows = run(4, &p, false);
        assert!(!sees(&rows, 0, 1) && !sees(&rows, 1, 0), "the wall hides the rooms");
        for (a, b) in [(0, 2), (0, 3), (1, 3), (1, 2), (2, 3)] {
            assert!(sees(&rows, a, b) && sees(&rows, b, a), "{a} and {b}");
        }
        let mut p2 = p.clone();
        p2.push(wall(0, 256.0, DVec3::new(0.0, 96.0, 0.0), DVec3::new(0.0, 160.0, 96.0), 0, 1, ca));
        let rows = run(4, &p2, false);
        assert!(sees(&rows, 0, 1) && sees(&rows, 1, 0));
        let pas = compute_pas(&run(4, &p, false));
        for a in 0..4 {
            for b in 0..4 {
                assert!(sees(&pas, a, b));
            }
        }
    }

    #[test]
    fn lump_round_trip() {
        let pvs = run(7, &corridor(), false);
        let pas = compute_pas(&pvs);
        let lump = vis_lump(&pvs, &pas);
        for c in 0..7 {
            for (kind, rows) in [(0, &pvs), (1, &pas)] {
                let row = decompress(&lump, c, kind).unwrap();
                for (d, v) in row.iter().enumerate() {
                    assert_eq!(*v, sees(rows, c, d));
                }
            }
        }
        let mut big = vec![vec![0u8; 125]; 1000];
        for (c, r) in big.iter_mut().enumerate() {
            r[c >> 3] |= 1 << (c & 7);
            r[999 >> 3] |= 1 << (999 & 7);
        }
        let lump = vis_lump(&big, &big);
        let i32_at = |o: usize| i32::from_le_bytes(lump[o..o + 4].try_into().unwrap());
        assert_eq!(i32_at(0), 1000);
        assert_eq!(i32_at(4), 4 + 8 * 1000, "the first PVS row after the offsets");
        for c in 0..999 {
            let o = 4 + 8 * c;
            assert!(i32_at(o) < i32_at(o + 8) && i32_at(o + 4) < i32_at(o + 12), "rows in order");
        }
        assert!(i32_at(4 + 8 * 999) < i32_at(8), "the PVS rows, then the PAS rows");
        for c in [0, 5, 500, 998, 999] {
            let row = decompress(&lump, c, 1).unwrap();
            assert_eq!(row.iter().filter(|v| **v).count(), if c == 999 { 1 } else { 2 });
            assert!(row[c] && row[999]);
        }
        assert_eq!(vis_lump(&[], &[]), 0i32.to_le_bytes().to_vec());
    }

    #[test]
    fn parses_portal_files() {
        let prt =
            "PRT1\n3\n2\n4 0 1 (0 0 0 ) (0 64 0 ) (0 64 64 ) (0 0 64 ) \n3 1 2 (64 0 0) (64 64 0) (64 0 64)\n0 -1\n";
        let (n, p) = parse_prt(prt).unwrap();
        assert_eq!(n, 3);
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].clusters, [1, 2]);
        assert_eq!(p[0].points[2], DVec3::new(0.0, 64.0, 64.0));
        for bad in [
            "",
            "PRT2\n1\n0\n",
            "PRT1\n3\n3\n4 0 1 (0 0 0 ) (0 64 0 ) (0 64 64 ) (0 0 64 )\n",
            "PRT1\n3\n1\n4 0 3 (0 0 0 ) (0 64 0 ) (0 64 64 ) (0 0 64 )\n",
            "PRT1\n3\n1\n4 0 1 (0 0 0 ) (0 64 0 ) (0 64 64 )\n",
            "PRT1\n3\n1\n2 0 1 (0 0 0 ) (0 64 0 )\n",
            "PRT1\n3\n1\n3 0 1 (0 0 0 ) (0 64 nan ) (0 64 64 )\n",
            "PRT1\n3\n1\n3 0 1 (0 0 0 ) (0 64 1e300 ) (0 64 64 )\n",
            "PRT1\n99999999\n1\n",
            "PRT1\n3\n99999999999\n",
            "PRT1\n3\n1\n-4 0 1 (0 0 0 )\n",
        ] {
            assert!(parse_prt(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn hostile_input_never_panics() {
        let nan = DVec3::splat(f64::NAN);
        let p = vec![
            VisPortal { points: vec![DVec3::ZERO; 3], clusters: [0, 1] },
            VisPortal { points: vec![nan, DVec3::X, DVec3::Y], clusters: [1, 2] },
            VisPortal { points: vec![DVec3::X, DVec3::Y], clusters: [2, 3] },
            VisPortal { points: vec![DVec3::X, DVec3::Y, DVec3::Z], clusters: [3, 99] },
            VisPortal { points: vec![DVec3::X, DVec3::Y, DVec3::Z], clusters: [3, 3] },
            VisPortal {
                points: (0..200).map(|i| DVec3::new(0.0, (i as f64).cos(), (i as f64).sin()) * 64.0).collect(),
                clusters: [3, 4],
            },
            VisPortal { points: vec![DVec3::splat(1e300), DVec3::X, DVec3::Y], clusters: [4, 0] },
        ];
        let rows = run(5, &p, false);
        assert_eq!(rows.len(), 5);
        assert!(sees(&rows, 0, 1) && sees(&rows, 2, 3) && sees(&rows, 3, 4) && sees(&rows, 0, 4));
        assert!(run(0, &p, false).is_empty());
        assert!(run(MAX_CLUSTERS + 1, &[], false).is_empty());
        assert_eq!(compute_pas(&[vec![], vec![0xff; 9]]).len(), 2);
        assert!(!vis_lump(&[vec![1]], &[]).is_empty());
    }

    fn maze(nx: usize, ny: usize, nz: usize, open_pct: u64, seed: u64) -> (usize, Vec<VisPortal>, Vec<[bool; 3]>) {
        let mut rng = seed;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng % 100
        };
        let id = |x: usize, y: usize, z: usize| (x + nx * (y + ny * z)) as u32;
        let size = 64.0;
        let mut open = vec![[false; 3]; nx * ny * nz];
        let mut portals = Vec::new();
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let c = id(x, y, z);
                    let lo = DVec3::new(x as f64, y as f64, z as f64) * size;
                    let center = lo + DVec3::splat(size / 2.0);
                    for axis in 0..3 {
                        let (mut nx_, mut ny_, mut nz_) = (x, y, z);
                        match axis {
                            0 => nx_ += 1,
                            1 => ny_ += 1,
                            _ => nz_ += 1,
                        }
                        if nx_ >= nx || ny_ >= ny || nz_ >= nz || next() >= open_pct {
                            continue;
                        }
                        open[c as usize][axis] = true;
                        let mut a = lo;
                        a[axis] += size;
                        let mut b = a + DVec3::splat(size);
                        b[axis] = a[axis];
                        portals.push(wall(axis, a[axis], a, b, c, id(nx_, ny_, nz_), center));
                    }
                }
            }
        }
        (nx * ny * nz, portals, open)
    }

    fn maze_rays(nx: usize, ny: usize, nz: usize, open: &[[bool; 3]], samples: usize, seed: u64) -> Vec<Vec<bool>> {
        let n = nx * ny * nz;
        let mut rng = seed;
        let mut rnd = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 11) as f64 / (1u64 << 53) as f64
        };
        let size = 64.0;
        let mut seen = vec![vec![false; n]; n];
        let cell_of =
            |p: DVec3| -> [usize; 3] { [(p.x / size) as usize, (p.y / size) as usize, (p.z / size) as usize] };
        let id = |c: [usize; 3]| c[0] + nx * (c[1] + ny * c[2]);
        for a in 0..n {
            seen[a][a] = true;
            let ca = [a % nx, (a / nx) % ny, a / (nx * ny)];
            for _ in 0..samples {
                let pa =
                    (DVec3::new(ca[0] as f64, ca[1] as f64, ca[2] as f64) + DVec3::new(rnd(), rnd(), rnd())) * size;
                let pb = DVec3::new(rnd() * nx as f64, rnd() * ny as f64, rnd() * nz as f64) * size;
                let steps = ((pb - pa).length() / 0.25) as usize + 1;
                let mut cur = cell_of(pa);
                let mut ok = true;
                let mut path = vec![id(cur)];
                for k in 1..=steps {
                    let p = pa + (pb - pa) * (k as f64 / steps as f64);
                    let c = cell_of(p);
                    if c == cur {
                        continue;
                    }
                    let diff: Vec<usize> = (0..3).filter(|&i| c[i] != cur[i]).collect();
                    if diff.len() != 1 {
                        ok = false;
                        break;
                    }
                    let axis = diff[0];
                    let lower = if c[axis] > cur[axis] { cur } else { c };
                    if !open[id(lower)][axis] {
                        ok = false;
                        break;
                    }
                    cur = c;
                    path.push(id(cur));
                }
                if ok {
                    let b = id(cur);
                    seen[a][b] = true;
                    seen[b][a] = true;
                }
            }
        }
        seen
    }

    #[test]
    fn conservative_in_a_maze() {
        for (seed, pct) in [(7u64, 55u64), (13, 75)] {
            let (nx, ny, nz) = (8, 8, 2);
            let (n, portals, open) = maze(nx, ny, nz, pct, seed);
            let full = run(n, &portals, false);
            let fast = run(n, &portals, true);
            let truth = maze_rays(nx, ny, nz, &open, 400, seed);
            let (mut t, mut o, mut f) = (0, 0, 0);
            for a in 0..n {
                for b in 0..n {
                    t += truth[a][b] as usize;
                    o += sees(&full, a, b) as usize;
                    f += sees(&fast, a, b) as usize;
                    assert!(!truth[a][b] || sees(&full, a, b), "maze {seed}: {a} sees {b}");
                    assert!(!sees(&full, a, b) || sees(&fast, a, b));
                }
            }
            assert!(o < f, "the flow culls something");
            eprintln!("maze {seed}: rays {t} pairs, flow {o}, fast {f}");
        }
    }

    #[test]
    fn refined_fast_rows_are_the_full_rows() {
        for (seed, pct) in [(7u64, 55u64), (13, 75)] {
            let (nx, ny, nz) = (8, 8, 2);
            let (n, portals, open) = maze(nx, ny, nz, pct, seed);
            let full = run(n, &portals, false);
            let fast = run(n, &portals, true);
            let truth = maze_rays(nx, ny, nz, &open, 400, seed);
            let picked: Vec<usize> = (0..n).filter(|c| c % 3 == 0).collect();
            let pick = |_: &[Vec<u8>]| picked.clone();
            let opts = VisOptions { threads: Some(2), ..Default::default() };
            let refined = compute_fast_pvs_refined(n, &portals, &opts, None, &pick, &mut |_| {});
            for a in 0..n {
                if a % 3 != 0 {
                    assert_eq!(refined[a], fast[a], "maze {seed}: row {a} not picked");
                    continue;
                }
                for b in 0..n {
                    assert!(!truth[a][b] || sees(&refined, a, b), "maze {seed}: {a} sees {b}");
                    assert!(!sees(&refined, a, b) || sees(&fast, a, b));
                    assert!(!sees(&full, a, b) || sees(&refined, a, b), "maze {seed}: {a} -> {b} hidden");
                }
            }
            assert!((0..n).any(|a| a % 3 == 0 && refined[a] != fast[a]), "maze {seed}: nothing refined");
        }
    }

    #[test]
    fn sky_clusters_from_lumps() {
        use crate::materials::surf;
        let mut texinfo = vec![0u8; 72 * 3];
        for (t, f) in [(0usize, surf::SKY), (1, surf::SKY | surf::SKY2D), (2, 0)] {
            texinfo[t * 72 + 64..t * 72 + 68].copy_from_slice(&f.to_le_bytes());
        }
        let mut faces = vec![0u8; 56 * 3];
        for (f, t) in [(0usize, 0i16), (1, 1), (2, 2)] {
            faces[f * 56 + 10..f * 56 + 12].copy_from_slice(&t.to_le_bytes());
        }
        let leaffaces: Vec<u8> = [0u16, 2, 1].iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut leafs = vec![0u8; 32 * 3];
        for (l, cluster, first, count) in [(0usize, 0i16, 0u16, 1u16), (1, 1, 1, 2), (2, -1, 0, 3)] {
            leafs[l * 32 + 4..l * 32 + 6].copy_from_slice(&cluster.to_le_bytes());
            leafs[l * 32 + 20..l * 32 + 22].copy_from_slice(&first.to_le_bytes());
            leafs[l * 32 + 22..l * 32 + 24].copy_from_slice(&count.to_le_bytes());
        }
        assert_eq!(sky_clusters(&leafs, &leaffaces, &faces, &texinfo, 3), vec![SKY_3D, SKY_2D, 0]);
    }

    #[test]
    fn same_with_any_instruction_set() {
        let (n, portals, _) = maze(10, 10, 2, 70, 3);
        let mut isas = vec![Isa::Scalar, Isa::Sse];
        if matches!(isa(), Isa::Avx2 | Isa::Avx512) {
            isas.push(Isa::Avx2);
        }
        if isa() == Isa::Avx512 {
            isas.push(Isa::Avx512);
        }
        for radius in [None, Some(200.0)] {
            let one = VisOptions { threads: Some(1), simd: Some(Isa::Scalar), ..Default::default() };
            let all = |_: &[Vec<u8>]| (0..n).collect::<Vec<_>>();
            let reference = compute_pvs_with_radius(n, &portals, &one, radius, &mut |_| {});
            let reference_refined = compute_fast_pvs_refined(n, &portals, &one, radius, &all, &mut |_| {});
            for &simd in &isas {
                for threads in [Some(1), Some(3)] {
                    let o = VisOptions { threads, simd: Some(simd), ..Default::default() };
                    let rows = compute_pvs_with_radius(n, &portals, &o, radius, &mut |_| {});
                    assert!(rows == reference, "{simd:?}, {threads:?} threads, radius {radius:?}");
                    let refined = compute_fast_pvs_refined(n, &portals, &o, radius, &all, &mut |_| {});
                    assert!(refined == reference_refined, "refined: {simd:?}, {threads:?} threads, radius {radius:?}");
                }
            }
        }
    }

    #[test]
    fn sphere_lanes_match_hidden() {
        let (n, portals, _) = maze(6, 6, 1, 80, 9);
        let (w, _) = World::new(n, &portals, None);
        let mut rnd = xorshift(5);
        let mut f = move || (rnd() % 2001) as f64 / 1000.0 - 1.0;
        for _ in 0..200 {
            let planes: Vec<Plane> = (0..6)
                .map(|_| {
                    let nrm = DVec3::new(f(), f(), f()).normalize_or(DVec3::Z);
                    Plane { n: nrm, d: f() * 300.0 }
                })
                .collect();
            let (a, b) = planes.split_at(3);
            for start in (0..w.portals.len()).step_by(simd::LANES) {
                let mut sp = simd::Spheres::default();
                let qs: Vec<usize> = (start..(start + simd::LANES).min(w.portals.len())).collect();
                for &q in &qs {
                    sp.push(w.spheres[q]);
                }
                for isa in [Isa::Scalar, Isa::Sse, isa()] {
                    let (behind, points) = simd::sphere_masks(isa, [a, b], &sp);
                    for (k, &q) in qs.iter().enumerate() {
                        let lanes = behind >> k & 1 != 0
                            || (points >> k & 1 != 0 && hidden_by_points([a, b], &w.spheres[q], &w, q));
                        assert_eq!(lanes, hidden([a, b], &w.spheres[q], &w, q), "{isa:?} portal {q}");
                    }
                }
            }
        }
    }

    #[test]
    fn same_with_any_threads() {
        let (n, portals, _) = maze(10, 10, 2, 70, 3);
        for fast in [false, true] {
            let rows: Vec<_> = [Some(1), Some(3), None]
                .into_iter()
                .map(|threads| {
                    compute_pvs(n, &portals, &VisOptions { fast, threads, ..Default::default() }, &mut |_| {})
                })
                .collect();
            assert!(rows[0] == rows[1] && rows[1] == rows[2], "fast {fast}");
        }
    }

    fn xorshift(seed: u64) -> impl FnMut() -> u64 {
        let mut s = seed.max(1);
        move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        }
    }

    #[test]
    fn sparse_sets_match_dense_ones() {
        let mut rnd = xorshift(11);
        let words = 40;
        let set = |rnd: &mut dyn FnMut() -> u64| {
            let mut dense = vec![0u64; words];
            let mut sparse = SparseBuf::default();
            for (i, w) in dense.iter_mut().enumerate() {
                if rnd().is_multiple_of(3) {
                    *w = rnd() & rnd();
                }
                if *w != 0 {
                    sparse.push(i, *w);
                }
            }
            (dense, sparse)
        };
        for _ in 0..200 {
            let (a, sa) = set(&mut rnd);
            let (b, sb) = set(&mut rnd);
            let va = sa.view();
            for i in 0..words * 64 {
                assert_eq!(va.get(i), dense_get(&a, i));
            }
            let nz: Vec<usize> = (0..words).filter(|i| a[*i] != 0).collect();
            assert_eq!(va.span_len(), nz.last().map_or(0, |l| l + 1 - nz[0]));
            assert_eq!(va.subset_of(sb.view()), a.iter().zip(&b).all(|(x, y)| x & !y == 0));
            let mut union = b.clone();
            dense_or(&mut union, (0, &a));
            let mut su = SparseBuf::default();
            for (i, w) in union.iter().enumerate().filter(|(_, w)| **w != 0) {
                su.push(i, *w);
            }
            assert!(va.subset_of(su.view()) && sb.view().subset_of(su.view()));
            let start = (rnd() % 10) as usize;
            let end = start + (rnd() % 30) as usize;
            let from = sa.idx.partition_point(|i| (*i as usize) < start);
            let mut and = SparseBuf::default();
            and.and(&sa.idx[from..], &sa.w[from..], (start, &b[start..end.min(words)]));
            let want: Vec<(u32, u64)> =
                (start..end.min(words)).map(|i| (i as u32, a[i] & b[i])).filter(|(_, w)| *w != 0).collect();
            assert_eq!(and.idx.iter().copied().zip(and.w.iter().copied()).collect::<Vec<_>>(), want);
        }
    }

    #[test]
    fn lazy_clips_match_whole_ones() {
        let mut rnd = xorshift(5);
        let mut f = move || (rnd() >> 11) as f64 / (1u64 << 53) as f64;
        let quad = |f: &mut dyn FnMut() -> f64, x: f64| -> Vec<DVec3> {
            let c = DVec3::new(x, f() * 64.0, f() * 64.0);
            let (u, v) = (
                DVec3::new(f() * 8.0, 16.0 + f() * 32.0, f() * 8.0),
                DVec3::new(f() * 8.0, f() * 8.0, 16.0 + f() * 32.0),
            );
            vec![c - u - v, c + u - v, c + u + v, c - u + v]
        };
        let (mut kept, mut gone) = (0, 0);
        for _ in 0..2000 {
            let a = quad(&mut f, 0.0);
            let xb = 64.0 + f() * 64.0;
            let b = quad(&mut f, xb);
            let xt = 200.0 + f() * 64.0;
            let t = quad(&mut f, xt);
            for flip in [false, true] {
                let mut planes = Vec::new();
                separators(&a, &b, flip, &mut planes);
                let (mut whole, mut spare) = (t.clone(), Vec::new());
                let ok_whole = clip_by(&mut whole, &planes, &mut spare);
                let mut lazy = t.clone();
                let ok_lazy = clip_by_separators(&mut lazy, &a, &b, flip, &mut spare);
                assert_eq!(ok_whole, ok_lazy);
                if ok_whole {
                    assert!(whole.len() <= CAP.max(t.len()));
                    assert!(
                        whole
                            .iter()
                            .zip(&lazy)
                            .all(|(x, y)| x.to_array().map(f64::to_bits) == y.to_array().map(f64::to_bits))
                    );
                    assert_eq!(whole.len(), lazy.len());
                    kept += 1;
                } else {
                    gone += 1;
                }
            }
        }
        assert!(kept > 0 && gone > 0, "{kept} kept, {gone} gone");
    }

    struct RayBsp {
        planes: Vec<(DVec3, f64)>,
        nodes: Vec<(usize, [i32; 2])>,
        leafs: Vec<(i16, DVec3, DVec3)>,
        by_cluster: Vec<Vec<usize>>,
    }

    impl RayBsp {
        fn new(bsp: &hp_bsp::Bsp, clusters: usize) -> Option<RayBsp> {
            let planes_l = bsp.lump(1).ok()?;
            let nodes_l = bsp.lump(5).ok()?;
            let leafs_l = bsp.lump(10).ok()?;
            if leafs_l.len() % 32 != 0 {
                return None;
            }
            let f = |d: &[u8], o: usize| f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as f64;
            let i = |d: &[u8], o: usize| i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
            let h = |d: &[u8], o: usize| i16::from_le_bytes([d[o], d[o + 1]]);
            let planes = (0..planes_l.len() / 20)
                .map(|k| {
                    (
                        DVec3::new(f(&planes_l, k * 20), f(&planes_l, k * 20 + 4), f(&planes_l, k * 20 + 8)),
                        f(&planes_l, k * 20 + 12),
                    )
                })
                .collect();
            let nodes = (0..nodes_l.len() / 32)
                .map(|k| (i(&nodes_l, k * 32) as usize, [i(&nodes_l, k * 32 + 4), i(&nodes_l, k * 32 + 8)]))
                .collect();
            let leafs: Vec<(i16, DVec3, DVec3)> = (0..leafs_l.len() / 32)
                .map(|k| {
                    let o = k * 32;
                    let v = |x: usize| h(&leafs_l, o + x) as f64;
                    (h(&leafs_l, o + 4), DVec3::new(v(8), v(10), v(12)), DVec3::new(v(14), v(16), v(18)))
                })
                .collect();
            let mut by_cluster = vec![Vec::new(); clusters];
            for (k, l) in leafs.iter().enumerate() {
                if l.0 >= 0 && (l.0 as usize) < clusters {
                    by_cluster[l.0 as usize].push(k);
                }
            }
            Some(RayBsp { planes, nodes, leafs, by_cluster })
        }

        fn leaf_at(&self, p: DVec3) -> usize {
            let mut n: i32 = 0;
            while n >= 0 {
                let (pl, ch) = self.nodes[n as usize];
                let (normal, d) = self.planes[pl];
                n = if normal.dot(p) - d >= 0.0 { ch[0] } else { ch[1] };
            }
            (-1 - n) as usize
        }

        fn clear(&self, node: i32, p1: DVec3, p2: DVec3) -> bool {
            if node < 0 {
                return self.leafs[(-1 - node) as usize].0 != -1;
            }
            let (pl, ch) = self.nodes[node as usize];
            let (n, d) = self.planes[pl];
            let (d1, d2) = (n.dot(p1) - d, n.dot(p2) - d);
            if d1 >= 0.0 && d2 >= 0.0 {
                return self.clear(ch[0], p1, p2);
            }
            if d1 < 0.0 && d2 < 0.0 {
                return self.clear(ch[1], p1, p2);
            }
            let mid = p1 + (p2 - p1) * (d1 / (d1 - d2));
            let side = usize::from(d1 < 0.0);
            self.clear(ch[side], p1, mid) && self.clear(ch[1 - side], mid, p2)
        }

        fn point_in(&self, c: usize, rnd: &mut impl FnMut() -> f64) -> Option<DVec3> {
            let ls = self.by_cluster.get(c)?;
            if ls.is_empty() {
                return None;
            }
            for _ in 0..200 {
                let (_, lo, hi) = self.leafs[ls[(rnd() * ls.len() as f64) as usize % ls.len()]];
                let p = lo + (hi - lo) * DVec3::new(rnd(), rnd(), rnd());
                if self.leafs[self.leaf_at(p)].0 as usize == c {
                    return Some(p);
                }
            }
            None
        }

        fn clear_rays(&self, a: usize, b: usize, tries: usize, max_len: f64) -> usize {
            let mut state = 0x9e37_79b9_7f4a_7c15u64 ^ (a as u64) << 20 ^ b as u64;
            let mut rnd = move || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 11) as f64 / (1u64 << 53) as f64
            };
            (0..tries)
                .filter(|_| match (self.point_in(a, &mut rnd), self.point_in(b, &mut rnd)) {
                    (Some(pa), Some(pb)) => pa.distance(pb) < max_len && self.clear(0, pa, pb),
                    _ => false,
                })
                .count()
        }
    }

    struct Compare {
        clusters: usize,
        valve_pairs: usize,
        our_pairs: usize,
        missing: Vec<(usize, usize)>,
        not_self_or_neighbour: usize,
        valve_symmetric_lost: usize,
        pas_missing: usize,
        radius: Option<f64>,
        elapsed: std::time::Duration,
    }

    fn compare(prt: &str, bsp: &hp_bsp::Bsp, fast: bool, radius: Option<f64>) -> Option<Compare> {
        let text = std::fs::read_to_string(prt).ok()?;
        let lump = bsp.lump(4).ok()?.into_owned();
        let (n, portals) = parse_prt(&text).expect("valid portal file");
        let valve: Vec<Vec<bool>> = (0..n).map(|c| decompress(&lump, c, 0).expect("valve row")).collect();
        let t = std::time::Instant::now();
        let ours =
            compute_pvs_with_radius(n, &portals, &VisOptions { fast, ..Default::default() }, radius, &mut |_| {});
        let elapsed = t.elapsed();
        let t = std::time::Instant::now();
        let pas = compute_pas(&ours);
        let lump_ours = vis_lump(&ours, &pas);
        let pas_time = t.elapsed();
        let mut r = Compare {
            clusters: n,
            valve_pairs: 0,
            our_pairs: 0,
            missing: Vec::new(),
            not_self_or_neighbour: 0,
            valve_symmetric_lost: 0,
            pas_missing: 0,
            radius,
            elapsed,
        };
        for a in 0..n {
            let valve_pas = decompress(&lump, a, 1).expect("valve pas row");
            for b in 0..n {
                let (v, o) = (valve[a][b], sees(&ours, a, b));
                r.valve_pairs += v as usize;
                r.our_pairs += o as usize;
                if v && !o {
                    r.missing.push((a, b));
                }
                if v && valve[b][a] && !(o && sees(&ours, b, a)) {
                    r.valve_symmetric_lost += 1;
                }
                if valve_pas[b] && !sees(&pas, a, b) {
                    r.pas_missing += 1;
                }
            }
            if !sees(&ours, a, a) {
                r.not_self_or_neighbour += 1;
            }
        }
        for p in &portals {
            let [a, b] = p.clusters.map(|x| x as usize);
            if a != b && !(sees(&ours, a, b) && sees(&ours, b, a)) {
                r.not_self_or_neighbour += 1;
            }
        }
        eprintln!(
            "{prt} fast={fast} radius={radius:?}: {n} clusters, {} portals, {elapsed:.2?} (PAS and lump {pas_time:.2?}, {} bytes); \
             valve {:.1} visible per cluster, ours {:.1} (+{:.1}), {} missing pairs, {} missing PAS pairs",
            portals.len(),
            lump_ours.len(),
            r.valve_pairs as f64 / n as f64,
            r.our_pairs as f64 / n as f64,
            (r.our_pairs as f64 - r.valve_pairs as f64) / n as f64,
            r.missing.len(),
            r.pas_missing
        );
        Some(r)
    }

    fn check(r: &Compare, rays: Option<&RayBsp>) {
        assert!(r.clusters > 0);
        assert_eq!(r.not_self_or_neighbour, 0, "self and neighbours visible");
        assert!(r.our_pairs >= r.valve_pairs);
        match rays {
            None => {
                assert!(r.missing.is_empty(), "a superset of Valve's PVS: {:?}", &r.missing[..r.missing.len().min(10)]);
                assert_eq!(r.valve_symmetric_lost, 0, "symmetric where Valve's is");
                assert_eq!(r.pas_missing, 0, "a superset of Valve's PAS");
            }
            Some(rays) => {
                for &(a, b) in &r.missing {
                    let clear = rays.clear_rays(a, b, 20000, r.radius.map_or(f64::INFINITY, |x| x * 0.9));
                    assert_eq!(clear, 0, "cluster {a} sees cluster {b} ({clear} clear rays of 20000)");
                }
                assert!(r.valve_symmetric_lost <= 2 * r.missing.len());
                assert!(r.missing.len() * 10000 < r.valve_pairs, "close to Valve's PVS");
            }
        }
    }

    const SMALL_PRT: &str = "C:/Users/oracle/Desktop/gmodmapping/test_maps/.valve_ref/hammerpro_test_areaportal.prt";
    const SMALL_BSP: &str = "C:/Users/oracle/Desktop/gmodmapping/test_maps/.valve_ref/hammerpro_test_areaportal.bsp";
    const BIG_PRT: &str = "D:/GMC/GMC One Piece V1/map/gmc_op_v2.prt";
    const BIG_BSP: &str = "D:/GMC/GMC One Piece V1/map/gmc_op_v2.bsp";
    const BIG_FARZ: f64 = 12000.0;

    #[test]
    fn threads_are_limited() {
        for t in [1, 2, 3] {
            assert_eq!(Pool::new(Some(t)).run(rayon::current_num_threads), t);
        }
        let opts = VisOptions { threads: Some(1), ..Default::default() };
        let rows = compute_pvs(7, &corridor(), &opts, &mut |_| {});
        assert_eq!(compute_pas_with(&rows, &opts), compute_pas(&rows));
    }

    #[test]
    fn vis_radius_like_vvis() {
        let o = VisOptions::default();
        assert_eq!(vis_radius2(&o, None), None);
        assert_eq!(vis_radius2(&o, Some(0.0)), None);
        assert_eq!(vis_radius2(&o, Some(-5.0)), None);
        assert_eq!(vis_radius2(&o, Some(512.0)), Some(512.0 * 512.0));
        let o = VisOptions { radius_override: Some(-300.0), ..Default::default() };
        assert_eq!(vis_radius2(&o, Some(512.0)), Some(90000.0));
    }

    #[test]
    fn trace_names_and_errors() {
        assert_eq!(trace_file_name(r"C:\maps\a.b\x.bsp"), r"C:\maps\a.b\x.lin");
        assert_eq!(trace_file_name("maps/x"), "maps/x.lin");
        let opts = VisOptions { trace: Some((0, 9)), ..Default::default() };
        let err = compute_trace(7, &corridor(), &opts, None).unwrap_err();
        assert_eq!(err, "Invalid cluster trace: 0 to 9, valid range is 0 to 6");
        assert_eq!(compute_trace(7, &corridor(), &VisOptions::default(), None), Ok(None));
        let opts = VisOptions { trace: Some((0, 2)), ..Default::default() };
        let t = compute_trace(7, &corridor(), &opts, None).unwrap().expect("a trace");
        assert_eq!(t.lines().len() + 1, t.points.len());
        assert!(t.text().ends_with(
            "
"
        ));
    }

    #[test]
    fn matches_valve_on_a_small_map() {
        let Ok(bsp) = hp_bsp::Bsp::open(SMALL_BSP) else { return };
        for fast in [false, true] {
            if let Some(r) = compare(SMALL_PRT, &bsp, fast, None) {
                check(&r, None);
            }
        }
    }

    #[test]
    #[ignore = "a big real map (minutes): run with --ignored"]
    fn matches_valve_on_a_big_map() {
        let Ok(bsp) = hp_bsp::Bsp::open(BIG_BSP) else { return };
        let rays = RayBsp::new(&bsp, MAX_CLUSTERS).expect("bsp tree");
        for (fast, radius) in [(true, Some(BIG_FARZ)), (false, Some(BIG_FARZ)), (true, None), (false, None)] {
            if let Some(r) = compare(BIG_PRT, &bsp, fast, radius) {
                check(&r, Some(&rays));
                assert!(r.elapsed.as_secs() < 600, "the visibility took {:?}", r.elapsed);
            }
        }
    }
}
