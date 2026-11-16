use glam::Vec3A;
use rayon::prelude::*;

pub const LEAF: u32 = 0x8000_0000;
const BINS: usize = 16;
const MAX_LEAF: usize = 4;
const PARALLEL: usize = 4096;
pub const TIE: f32 = 0.05;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Node {
    pub min: [f32; 3],
    pub a: u32,
    pub max: [f32; 3],
    pub b: u32,
}

impl Node {
    pub fn is_leaf(&self) -> bool {
        self.b & LEAF != 0
    }

    pub fn count(&self) -> u32 {
        self.b & !LEAF
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Tri {
    pub v0: [f32; 3],
    pub id: u32,
    pub e1: [f32; 3],
    pub pad1: u32,
    pub e2: [f32; 3],
    pub pad2: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Bvh {
    pub nodes: Vec<Node>,
    pub tris: Vec<Tri>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub t: f32,
    pub u: f32,
    pub v: f32,
    pub id: u32,
    pub front: bool,
}

#[derive(Clone, Copy)]
struct Prim {
    lo: Vec3A,
    hi: Vec3A,
    c: Vec3A,
    index: u32,
}

enum Build {
    Leaf(Vec3A, Vec3A, Vec<u32>),
    Inner(Vec3A, Vec3A, Box<Build>, Box<Build>),
}

fn area(lo: Vec3A, hi: Vec3A) -> f32 {
    let d = (hi - lo).max(Vec3A::ZERO);
    2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
}

fn bounds(prims: &[Prim]) -> (Vec3A, Vec3A, Vec3A, Vec3A) {
    prims.iter().fold(
        (
            Vec3A::splat(f32::INFINITY),
            Vec3A::splat(f32::NEG_INFINITY),
            Vec3A::splat(f32::INFINITY),
            Vec3A::splat(f32::NEG_INFINITY),
        ),
        |(lo, hi, clo, chi), p| (lo.min(p.lo), hi.max(p.hi), clo.min(p.c), chi.max(p.c)),
    )
}

fn split(prims: &mut [Prim], clo: Vec3A, chi: Vec3A, parent_area: f32) -> Option<usize> {
    let n = prims.len();
    let extent = chi - clo;
    let mut best = (f32::INFINITY, 0usize, 0.0f32);
    for axis in 0..3 {
        if extent[axis] <= 1e-6 {
            continue;
        }
        let mut count = [0u32; BINS];
        let mut blo = [Vec3A::splat(f32::INFINITY); BINS];
        let mut bhi = [Vec3A::splat(f32::NEG_INFINITY); BINS];
        let scale = BINS as f32 / extent[axis];
        for p in prims.iter() {
            let b = (((p.c[axis] - clo[axis]) * scale) as usize).min(BINS - 1);
            count[b] += 1;
            blo[b] = blo[b].min(p.lo);
            bhi[b] = bhi[b].max(p.hi);
        }
        let mut left_area = [0.0f32; BINS];
        let mut left_count = [0u32; BINS];
        let (mut lo, mut hi, mut c) = (Vec3A::splat(f32::INFINITY), Vec3A::splat(f32::NEG_INFINITY), 0u32);
        for i in 0..BINS - 1 {
            c += count[i];
            if count[i] > 0 {
                lo = lo.min(blo[i]);
                hi = hi.max(bhi[i]);
            }
            left_count[i] = c;
            left_area[i] = if c > 0 { area(lo, hi) } else { 0.0 };
        }
        let (mut lo, mut hi, mut c) = (Vec3A::splat(f32::INFINITY), Vec3A::splat(f32::NEG_INFINITY), 0u32);
        for i in (1..BINS).rev() {
            c += count[i];
            if count[i] > 0 {
                lo = lo.min(blo[i]);
                hi = hi.max(bhi[i]);
            }
            let lc = left_count[i - 1];
            if lc == 0 || c == 0 {
                continue;
            }
            let cost = left_area[i - 1] * lc as f32 + area(lo, hi) * c as f32;
            if cost < best.0 {
                best = (cost, axis, clo[axis] + i as f32 / scale);
            }
        }
    }
    let leaf_cost = n as f32 * parent_area;
    let (cost, axis, pos) = best;
    if !cost.is_finite() || (n <= MAX_LEAF && cost * 1.0 + parent_area * 0.5 >= leaf_cost) {
        if n <= MAX_LEAF {
            return None;
        }
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        prims.select_nth_unstable_by(n / 2, |a, b| a.c[axis].total_cmp(&b.c[axis]));
        return Some(n / 2);
    }
    let mut i = 0;
    let mut j = n;
    while i < j {
        if prims[i].c[axis] < pos {
            i += 1;
        } else {
            j -= 1;
            prims.swap(i, j);
        }
    }
    if i == 0 || i == n {
        prims.select_nth_unstable_by(n / 2, |a, b| a.c[axis].total_cmp(&b.c[axis]));
        return Some(n / 2);
    }
    Some(i)
}

fn build(prims: &mut [Prim], depth: u32) -> Build {
    let (lo, hi, clo, chi) = bounds(prims);
    if prims.len() <= 1 || depth > 64 {
        return Build::Leaf(lo, hi, prims.iter().map(|p| p.index).collect());
    }
    match split(prims, clo, chi, area(lo, hi)) {
        None => Build::Leaf(lo, hi, prims.iter().map(|p| p.index).collect()),
        Some(mid) => {
            let (l, r) = prims.split_at_mut(mid);
            let (a, b) = if l.len() + r.len() > PARALLEL {
                rayon::join(|| build(l, depth + 1), || build(r, depth + 1))
            } else {
                (build(l, depth + 1), build(r, depth + 1))
            };
            Build::Inner(lo, hi, Box::new(a), Box::new(b))
        }
    }
}

fn padded(lo: Vec3A, hi: Vec3A) -> ([f32; 3], [f32; 3]) {
    let e = (lo.abs().max(hi.abs()) * 1e-6).max(Vec3A::splat(1e-3));
    ((lo - e).to_array(), (hi + e).to_array())
}

fn flatten(b: &Build, nodes: &mut Vec<Node>, order: &mut Vec<u32>) -> u32 {
    let at = nodes.len() as u32;
    nodes.push(Node::default());
    match b {
        Build::Leaf(lo, hi, list) => {
            let (min, max) = padded(*lo, *hi);
            nodes[at as usize] = Node { min, a: order.len() as u32, max, b: LEAF | list.len() as u32 };
            order.extend_from_slice(list);
        }
        Build::Inner(lo, hi, l, r) => {
            let a = flatten(l, nodes, order);
            let bb = flatten(r, nodes, order);
            let (min, max) = padded(*lo, *hi);
            nodes[at as usize] = Node { min, a, max, b: bb };
        }
    }
    at
}

#[inline]
pub fn safe_inv(d: Vec3A) -> Vec3A {
    let f = |x: f32| {
        let x = if x.abs() < 1e-20 { if x >= 0.0 { 1e-20 } else { -1e-20 } } else { x };
        1.0 / x
    };
    Vec3A::new(f(d.x), f(d.y), f(d.z))
}

impl Bvh {
    pub fn build(tris: &[[[f32; 3]; 3]], ids: &[u32]) -> Bvh {
        if tris.is_empty() {
            return Bvh { nodes: vec![Node { min: [1.0e30; 3], a: 0, max: [-1.0e30; 3], b: LEAF }], tris: Vec::new() };
        }
        let mut prims: Vec<Prim> = tris
            .par_iter()
            .enumerate()
            .map(|(i, t)| {
                let a = Vec3A::from_array(t[0]);
                let b = Vec3A::from_array(t[1]);
                let c = Vec3A::from_array(t[2]);
                let lo = a.min(b).min(c);
                let hi = a.max(b).max(c);
                Prim { lo, hi, c: (lo + hi) * 0.5, index: i as u32 }
            })
            .collect();
        let tree = build(&mut prims, 0);
        let mut nodes = Vec::with_capacity(tris.len() * 2);
        let mut order = Vec::with_capacity(tris.len());
        flatten(&tree, &mut nodes, &mut order);
        let out = order
            .par_iter()
            .map(|&i| {
                let t = &tris[i as usize];
                let v0 = Vec3A::from_array(t[0]);
                Tri {
                    v0: t[0],
                    id: ids.get(i as usize).copied().unwrap_or(i),
                    e1: (Vec3A::from_array(t[1]) - v0).to_array(),
                    pad1: 0,
                    e2: (Vec3A::from_array(t[2]) - v0).to_array(),
                    pad2: 0,
                }
            })
            .collect();
        Bvh { nodes, tris: out }
    }

    pub fn bytes(&self) -> usize {
        self.nodes.len() * 32 + self.tris.len() * 48
    }

    #[inline]
    fn slab(n: &Node, o: Vec3A, inv: Vec3A, tmax: f32) -> f32 {
        let t0 = (Vec3A::from_array(n.min) - o) * inv;
        let t1 = (Vec3A::from_array(n.max) - o) * inv;
        let near = t0.min(t1).max_element().max(0.0);
        let far = t0.max(t1).min_element().min(tmax);
        if near <= far { near } else { f32::INFINITY }
    }

    #[inline]
    pub fn intersect(t: &Tri, o: Vec3A, d: Vec3A) -> Option<(f32, f32, f32, bool)> {
        let e1 = Vec3A::from_array(t.e1);
        let e2 = Vec3A::from_array(t.e2);
        let p = d.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let s = o - Vec3A::from_array(t.v0);
        let u = s.dot(p) * inv;
        if !(-1e-6..=1.000_001).contains(&u) {
            return None;
        }
        let q = s.cross(e1);
        let v = d.dot(q) * inv;
        if v < -1e-6 || u + v > 1.000_001 {
            return None;
        }
        let tt = e2.dot(q) * inv;
        Some((tt, u, v, det < 0.0))
    }

    pub fn closest(
        &self,
        o: Vec3A,
        d: Vec3A,
        tmin: f32,
        tmax: f32,
        accept: &dyn Fn(u32, f32, f32) -> bool,
        priority: &dyn Fn(u32) -> u32,
    ) -> Option<Hit> {
        let inv = safe_inv(d);
        let mut best: Option<Hit> = None;
        let mut best_t = tmax;
        let mut best_p = 0u32;
        let mut stack = [0u32; 96];
        let mut sp = 0usize;
        let root = &self.nodes[0];
        if Self::slab(root, o, inv, tmax).is_infinite() {
            return None;
        }
        stack[sp] = 0;
        sp += 1;
        while sp > 0 {
            sp -= 1;
            let n = &self.nodes[stack[sp] as usize];
            if n.is_leaf() {
                for k in n.a..n.a + n.count() {
                    let t = &self.tris[k as usize];
                    let Some((tt, u, v, front)) = Self::intersect(t, o, d) else { continue };
                    if tt <= tmin || tt > best_t + TIE {
                        continue;
                    }
                    let p = priority(t.id);
                    let better = match best {
                        None => tt <= tmax,
                        Some(_) => {
                            tt < best_t - TIE || (p > best_p && tt <= best_t + TIE) || (p == best_p && tt < best_t)
                        }
                    };
                    if better && accept(t.id, u, v) {
                        best = Some(Hit { t: tt, u, v, id: t.id, front });
                        best_t = tt;
                        best_p = p;
                    }
                }
                continue;
            }
            let limit = best_t + TIE;
            let (l, r) = (&self.nodes[n.a as usize], &self.nodes[n.b as usize]);
            let tl = Self::slab(l, o, inv, limit);
            let tr = Self::slab(r, o, inv, limit);
            let (first, second, tf, ts) = if tl <= tr { (n.a, n.b, tl, tr) } else { (n.b, n.a, tr, tl) };
            if ts.is_finite() && sp < stack.len() {
                stack[sp] = second;
                sp += 1;
            }
            if tf.is_finite() && sp < stack.len() {
                stack[sp] = first;
                sp += 1;
            }
        }
        best
    }

    pub fn any(&self, o: Vec3A, d: Vec3A, tmin: f32, tmax: f32, accept: &dyn Fn(u32, f32, f32, f32) -> bool) -> bool {
        let inv = safe_inv(d);
        let mut stack = [0u32; 96];
        let mut sp = 0usize;
        if Self::slab(&self.nodes[0], o, inv, tmax).is_infinite() {
            return false;
        }
        stack[sp] = 0;
        sp += 1;
        while sp > 0 {
            sp -= 1;
            let n = &self.nodes[stack[sp] as usize];
            if n.is_leaf() {
                for k in n.a..n.a + n.count() {
                    let t = &self.tris[k as usize];
                    let Some((tt, u, v, _)) = Self::intersect(t, o, d) else { continue };
                    if tt > tmin && tt < tmax && accept(t.id, u, v, tt) {
                        return true;
                    }
                }
                continue;
            }
            for c in [n.a, n.b] {
                if Self::slab(&self.nodes[c as usize], o, inv, tmax).is_finite() && sp < stack.len() {
                    stack[sp] = c;
                    sp += 1;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: usize) -> Vec<[[f32; 3]; 3]> {
        let mut out = Vec::new();
        for y in 0..n {
            for x in 0..n {
                let (x0, y0) = (x as f32, y as f32);
                out.push([[x0, y0, 0.0], [x0 + 1.0, y0, 0.0], [x0, y0 + 1.0, 0.0]]);
                out.push([[x0 + 1.0, y0, 0.0], [x0 + 1.0, y0 + 1.0, 0.0], [x0, y0 + 1.0, 0.0]]);
            }
        }
        out
    }

    #[test]
    fn closest_hits_match_brute_force() {
        let mut tris = grid(20);
        for k in 0..200 {
            let z = 1.0 + (k % 7) as f32;
            let x = (k * 37 % 19) as f32;
            let y = (k * 11 % 17) as f32;
            tris.push([[x, y, z], [x + 0.7, y, z + 0.1], [x, y + 0.9, z]]);
        }
        let ids: Vec<u32> = (0..tris.len() as u32).collect();
        let bvh = Bvh::build(&tris, &ids);
        let flat: Vec<Tri> = tris
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let v0 = Vec3A::from_array(t[0]);
                Tri {
                    v0: t[0],
                    id: i as u32,
                    e1: (Vec3A::from_array(t[1]) - v0).to_array(),
                    pad1: 0,
                    e2: (Vec3A::from_array(t[2]) - v0).to_array(),
                    pad2: 0,
                }
            })
            .collect();
        for k in 0..500u32 {
            let o = Vec3A::new((k % 23) as f32 * 0.9 - 1.0, (k % 19) as f32, 12.0);
            let d =
                Vec3A::new(((k * 7) % 11) as f32 * 0.05 - 0.25, ((k * 3) % 13) as f32 * 0.04 - 0.2, -1.0).normalize();
            let got = bvh.closest(o, d, 1e-4, 1e9, &|_, _, _| true, &|_| 0).map(|h| h.t);
            let want = flat
                .iter()
                .filter_map(|t| Bvh::intersect(t, o, d).map(|h| h.0))
                .filter(|t| *t > 1e-4)
                .fold(None, |a: Option<f32>, t| Some(a.map_or(t, |a| a.min(t))));
            match (got, want) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-3, "{a} {b}"),
                (None, None) => {}
                other => panic!("{other:?}"),
            }
            assert_eq!(bvh.any(o, d, 1e-4, 1e9, &|_, _, _, _| true), want.is_some());
        }
    }

    #[test]
    fn ties_prefer_the_higher_priority() {
        let t = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]];
        let bvh = Bvh::build(&[t, t], &[0, 1]);
        let h = bvh.closest(Vec3A::new(1.0, 1.0, 5.0), -Vec3A::Z, 0.0, 100.0, &|_, _, _| true, &|id| id).expect("hit");
        assert_eq!(h.id, 1);
        let h =
            bvh.closest(Vec3A::new(1.0, 1.0, 5.0), -Vec3A::Z, 0.0, 100.0, &|id, _, _| id == 0, &|id| id).expect("hit");
        assert_eq!(h.id, 0);
    }
}
