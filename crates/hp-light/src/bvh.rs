use glam::{BVec4A, Vec3A, Vec4};

const LEAF_TARGET: usize = 4;
const MAX_LEAF: usize = 8;
const BINS: usize = 16;
const PARALLEL_PRIMS: usize = 16_384;
const MAX_DEPTH: usize = 60;
const STACK: usize = 3 * MAX_DEPTH + 8;
pub const T_MIN: f32 = 1e-3;
pub const ALL_CLASSES: u8 = u8::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub p: [Vec3A; 3],
    pub payload: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blocker {
    tree: u32,
    pack: u32,
    lane: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub t: f32,
    pub u: f32,
    pub v: f32,
    pub payload: u32,
    pub front: bool,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    min: Vec3A,
    max: Vec3A,
    start: u32,
    count: u32,
}

#[derive(Clone, Copy)]
struct BuildPrim {
    min: Vec3A,
    max: Vec3A,
    center: Vec3A,
    index: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C, align(16))]
struct Node4 {
    lo: [Vec4; 3],
    hi: [Vec4; 3],
    child: [u32; 4],
    meta: [u32; 4],
}

const COUNT_MASK: u32 = (1 << 24) - 1;

#[derive(Clone, Copy, Debug)]
#[repr(C, align(16))]
struct Pack {
    v0: [Vec4; 3],
    e1: [Vec4; 3],
    e2: [Vec4; 3],
    payload: [u32; 4],
    class: [u8; 4],
}

#[derive(Debug, Default)]
pub struct Bvh {
    trees: Vec<Tree>,
    count: usize,
    bounds: Option<(Vec3A, Vec3A)>,
}

#[derive(Debug)]
struct Tree {
    nodes: Vec<Node4>,
    packs: Vec<Pack>,
    classes: u8,
}

fn pad_lo(v: f32) -> f32 {
    v - (v.abs() * 1e-5 + 1e-4)
}

fn pad_hi(v: f32) -> f32 {
    v + (v.abs() * 1e-5 + 1e-4)
}

fn surface(min: Vec3A, max: Vec3A) -> f32 {
    let d = (max - min).max(Vec3A::ZERO);
    2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
}

fn bounds(prims: &[BuildPrim]) -> (Vec3A, Vec3A, Vec3A, Vec3A) {
    let mut lo = Vec3A::splat(f32::INFINITY);
    let mut hi = Vec3A::splat(f32::NEG_INFINITY);
    let mut clo = lo;
    let mut chi = hi;
    for p in prims {
        lo = lo.min(p.min);
        hi = hi.max(p.max);
        clo = clo.min(p.center);
        chi = chi.max(p.center);
    }
    (lo, hi, clo, chi)
}

enum Split {
    Leaf,
    Bin { axis: usize, bin: usize, lo: f32, scale: f32 },
    Median { axis: usize },
}

fn find_split(prims: &[BuildPrim], lo: Vec3A, hi: Vec3A, clo: Vec3A, chi: Vec3A) -> Split {
    let n = prims.len();
    if n <= LEAF_TARGET {
        return Split::Leaf;
    }
    let leaf_cost = n as f32 * surface(lo, hi);
    let mut best: Option<(f32, usize, usize)> = None;
    let extent = chi - clo;
    for axis in 0..3 {
        let e = extent[axis];
        if e <= 1e-6 {
            continue;
        }
        let scale = BINS as f32 / e;
        let mut count = [0usize; BINS];
        let mut bmin = [Vec3A::splat(f32::INFINITY); BINS];
        let mut bmax = [Vec3A::splat(f32::NEG_INFINITY); BINS];
        for p in prims {
            let b = (((p.center[axis] - clo[axis]) * scale) as usize).min(BINS - 1);
            count[b] += 1;
            bmin[b] = bmin[b].min(p.min);
            bmax[b] = bmax[b].max(p.max);
        }
        let mut right_cost = [0.0f32; BINS];
        let (mut rmin, mut rmax, mut rc) = (Vec3A::splat(f32::INFINITY), Vec3A::splat(f32::NEG_INFINITY), 0usize);
        for b in (1..BINS).rev() {
            rmin = rmin.min(bmin[b]);
            rmax = rmax.max(bmax[b]);
            rc += count[b];
            right_cost[b] = if rc > 0 { rc as f32 * surface(rmin, rmax) } else { 0.0 };
        }
        let (mut lmin, mut lmax, mut lc) = (Vec3A::splat(f32::INFINITY), Vec3A::splat(f32::NEG_INFINITY), 0usize);
        for b in 0..BINS - 1 {
            lmin = lmin.min(bmin[b]);
            lmax = lmax.max(bmax[b]);
            lc += count[b];
            if lc == 0 || lc == n {
                continue;
            }
            let cost = lc as f32 * surface(lmin, lmax) + right_cost[b + 1];
            if best.is_none_or(|(c, _, _)| cost < c) {
                best = Some((cost, axis, b + 1));
            }
        }
    }
    let longest = {
        let e = hi - lo;
        if e.x >= e.y && e.x >= e.z {
            0
        } else if e.y >= e.z {
            1
        } else {
            2
        }
    };
    match best {
        Some((cost, _, _)) if n <= MAX_LEAF && cost + surface(lo, hi) >= leaf_cost => Split::Leaf,
        Some((_, axis, bin)) => Split::Bin { axis, bin, lo: clo[axis], scale: BINS as f32 / extent[axis].max(1e-6) },
        None if n <= MAX_LEAF => Split::Leaf,
        None => Split::Median { axis: longest },
    }
}

fn partition(prims: &mut [BuildPrim], split: &Split) -> usize {
    let n = prims.len();
    let mid = match *split {
        Split::Bin { axis, bin, lo, scale } => {
            let mut i = 0;
            let mut j = n;
            while i < j {
                let b = (((prims[i].center[axis] - lo) * scale) as usize).min(BINS - 1);
                if b < bin {
                    i += 1;
                } else {
                    j -= 1;
                    prims.swap(i, j);
                }
            }
            i
        }
        Split::Median { axis } => {
            let mid = n / 2;
            prims.select_nth_unstable_by(mid, |a, b| a.center[axis].total_cmp(&b.center[axis]));
            mid
        }
        Split::Leaf => n,
    };
    if mid == 0 || mid >= n { n / 2 } else { mid }
}

fn leaf(lo: Vec3A, hi: Vec3A, first: u32, count: usize) -> Node {
    Node { min: lo, max: hi, start: first, count: count as u32 }
}

fn build_rec(prims: &mut [BuildPrim], out: &mut Vec<Node>, first: u32, depth: usize) {
    let (lo, hi, clo, chi) = bounds(prims);
    let at = out.len();
    let n = prims.len();
    let split = if depth >= MAX_DEPTH { Split::Leaf } else { find_split(prims, lo, hi, clo, chi) };
    if matches!(split, Split::Leaf) || n <= 1 {
        out.push(leaf(lo, hi, first, n));
        return;
    }
    out.push(Node { min: lo, max: hi, start: 0, count: 0 });
    let mid = partition(prims, &split);
    let (left, right) = prims.split_at_mut(mid);
    if n >= PARALLEL_PRIMS {
        let (lv, rv) = rayon::join(
            || {
                let mut v = Vec::new();
                build_rec(left, &mut v, 0, depth + 1);
                v
            },
            || {
                let mut v = Vec::new();
                build_rec(right, &mut v, 0, depth + 1);
                v
            },
        );
        let left_base = (at + 1) as u32;
        append(out, lv, left_base, first);
        let right_base = out.len() as u32;
        append(out, rv, right_base, first + mid as u32);
        out[at].start = right_base;
    } else {
        build_rec(left, out, first, depth + 1);
        let right_at = out.len() as u32;
        build_rec(right, out, first + mid as u32, depth + 1);
        out[at].start = right_at;
    }
}

fn append(out: &mut Vec<Node>, sub: Vec<Node>, node_base: u32, tri_base: u32) {
    out.extend(sub.into_iter().map(|mut node| {
        if node.count > 0 {
            node.start += tri_base;
        } else {
            node.start += node_base;
        }
        node
    }));
}

struct Collapse<'a> {
    bin: &'a [Node],
    tris: &'a [(Triangle, u8)],
    nodes: Vec<Node4>,
    packs: Vec<Pack>,
}

impl Collapse<'_> {
    fn leaf(&mut self, b: &Node) -> (u32, u32, u8) {
        let first = self.packs.len() as u32;
        let tris = &self.tris[b.start as usize..(b.start + b.count) as usize];
        let mut classes = 0u8;
        for chunk in tris.chunks(4) {
            let mut p =
                Pack { v0: [Vec4::ZERO; 3], e1: [Vec4::ZERO; 3], e2: [Vec4::ZERO; 3], payload: [0; 4], class: [0; 4] };
            for (lane, (t, class)) in chunk.iter().enumerate() {
                let (v0, e1, e2) = (t.p[0], t.p[1] - t.p[0], t.p[2] - t.p[0]);
                for axis in 0..3 {
                    p.v0[axis][lane] = v0[axis];
                    p.e1[axis][lane] = e1[axis];
                    p.e2[axis][lane] = e2[axis];
                }
                p.payload[lane] = t.payload;
                p.class[lane] = *class;
                classes |= *class;
            }
            self.packs.push(p);
        }
        (first, self.packs.len() as u32 - first, classes)
    }

    fn node(&mut self, at: usize) -> (u32, u8) {
        let mut kids: Vec<usize> = vec![at + 1, self.bin[at].start as usize];
        while kids.len() < 4 {
            let open = kids
                .iter()
                .enumerate()
                .filter(|(_, k)| self.bin[**k].count == 0)
                .max_by(|a, b| {
                    surface(self.bin[*a.1].min, self.bin[*a.1].max)
                        .total_cmp(&surface(self.bin[*b.1].min, self.bin[*b.1].max))
                })
                .map(|(i, _)| i);
            let Some(i) = open else { break };
            let k = kids[i];
            kids[i] = k + 1;
            kids.push(self.bin[k].start as usize);
        }
        let index = self.nodes.len();
        self.nodes.push(Node4 {
            lo: [Vec4::splat(f32::INFINITY); 3],
            hi: [Vec4::splat(f32::NEG_INFINITY); 3],
            child: [0; 4],
            meta: [0; 4],
        });
        let mut classes = 0u8;
        for (lane, &k) in kids.iter().enumerate() {
            let b = self.bin[k];
            let (child, count, class) = if b.count > 0 {
                self.leaf(&b)
            } else {
                let (c, class) = self.node(k);
                (c, 0, class)
            };
            let n = &mut self.nodes[index];
            for axis in 0..3 {
                n.lo[axis][lane] = pad_lo(b.min[axis]);
                n.hi[axis][lane] = pad_hi(b.max[axis]);
            }
            n.child[lane] = child;
            n.meta[lane] = (count & COUNT_MASK) | (u32::from(class) << 24);
            classes |= class;
        }
        (index as u32, classes)
    }
}

struct Ray {
    o: [Vec4; 3],
    d: [Vec4; 3],
    inv: [Vec4; 3],
    neg: [bool; 3],
}

impl Ray {
    fn new(o: Vec3A, d: Vec3A) -> Ray {
        let inv = inverse_dir(d);
        Ray {
            o: [Vec4::splat(o.x), Vec4::splat(o.y), Vec4::splat(o.z)],
            d: [Vec4::splat(d.x), Vec4::splat(d.y), Vec4::splat(d.z)],
            inv: [Vec4::splat(inv.x), Vec4::splat(inv.y), Vec4::splat(inv.z)],
            neg: [inv.x < 0.0, inv.y < 0.0, inv.z < 0.0],
        }
    }

    #[inline(always)]
    fn boxes(&self, n: &Node4, tmax: f32, mask: u8) -> (u32, Vec4) {
        let mut near = Vec4::ZERO;
        let mut far = Vec4::splat(tmax);
        for axis in 0..3 {
            let (a, b) = if self.neg[axis] { (n.hi[axis], n.lo[axis]) } else { (n.lo[axis], n.hi[axis]) };
            near = near.max((a - self.o[axis]) * self.inv[axis]);
            far = far.min((b - self.o[axis]) * self.inv[axis]);
        }
        let mut bits = near.cmple(far).bitmask();
        if bits != 0 && mask != ALL_CLASSES {
            for lane in 0..4 {
                if (n.meta[lane] >> 24) as u8 & mask == 0 {
                    bits &= !(1 << lane);
                }
            }
        }
        (bits, near)
    }

    #[inline(always)]
    fn tris(&self, p: &Pack, tmax: f32) -> (u32, Vec4, Vec4, Vec4, Vec4) {
        let [dx, dy, dz] = self.d;
        let [e1x, e1y, e1z] = p.e1;
        let [e2x, e2y, e2z] = p.e2;
        let px = dy * e2z - dz * e2y;
        let py = dz * e2x - dx * e2z;
        let pz = dx * e2y - dy * e2x;
        let det = e1x * px + e1y * py + e1z * pz;
        let inv_det = Vec4::ONE / det;
        let tx = self.o[0] - p.v0[0];
        let ty = self.o[1] - p.v0[1];
        let tz = self.o[2] - p.v0[2];
        let u = (tx * px + ty * py + tz * pz) * inv_det;
        let qx = ty * e1z - tz * e1y;
        let qy = tz * e1x - tx * e1z;
        let qz = tx * e1y - ty * e1x;
        let v = (dx * qx + dy * qy + dz * qz) * inv_det;
        let t = (e2x * qx + e2y * qy + e2z * qz) * inv_det;
        let ok: BVec4A = det.abs().cmpge(Vec4::splat(1e-12))
            & u.cmpge(Vec4::ZERO)
            & u.cmple(Vec4::ONE)
            & v.cmpge(Vec4::ZERO)
            & (u + v).cmple(Vec4::ONE)
            & t.cmpgt(Vec4::splat(T_MIN))
            & t.cmplt(Vec4::splat(tmax));
        (ok.bitmask(), t, u, v, det)
    }
}

#[inline(always)]
fn class_bits(p: &Pack, mask: u8) -> u32 {
    if mask == ALL_CLASSES {
        return 0xf;
    }
    let mut bits = 0;
    for lane in 0..4 {
        if p.class[lane] & mask != 0 {
            bits |= 1 << lane;
        }
    }
    bits
}

fn inverse_dir(d: Vec3A) -> Vec3A {
    let inv = |c: f32| if c.abs() > 1e-20 { 1.0 / c } else { 1e20f32.copysign(c) };
    Vec3A::new(inv(d.x), inv(d.y), inv(d.z))
}

#[inline(always)]
fn far_to_near(bits: u32, near: Vec4) -> ([u8; 4], usize) {
    let t = near.to_array();
    match bits.count_ones() {
        0 => ([0; 4], 0),
        1 => ([bits.trailing_zeros() as u8, 0, 0, 0], 1),
        2 => {
            let a = bits.trailing_zeros() as usize;
            let b = (bits & (bits - 1)).trailing_zeros() as usize;
            if t[a] < t[b] { ([b as u8, a as u8, 0, 0], 2) } else { ([a as u8, b as u8, 0, 0], 2) }
        }
        n => {
            let key = |l: usize| if bits & (1 << l) != 0 { t[l] } else { f32::NEG_INFINITY };
            let mut k = [(key(0), 0u8), (key(1), 1u8), (key(2), 2u8), (key(3), 3u8)];
            let mut cswap = |i: usize, j: usize| {
                let (a, b) = (k[i], k[j]);
                if b.0 > a.0 || (b.0 == a.0 && b.1 < a.1) {
                    k[i] = b;
                    k[j] = a;
                }
            };
            cswap(0, 1);
            cswap(2, 3);
            cswap(0, 2);
            cswap(1, 3);
            cswap(1, 2);
            ([k[0].1, k[1].1, k[2].1, k[3].1], n as usize)
        }
    }
}

impl Bvh {
    pub fn new(tris: &[Triangle]) -> Self {
        Self::with_classes(tris, |_| 1)
    }

    pub fn with_classes(tris: &[Triangle], class: impl Fn(u32) -> u8) -> Self {
        Self::with_groups(tris, class, &[])
    }

    pub fn with_groups(tris: &[Triangle], class: impl Fn(u32) -> u8, groups: &[u8]) -> Self {
        let (sets, lo, hi) = Self::sets_of(tris, class, groups);
        Self::from_sets(sets, lo, hi)
    }

    pub fn from_vec(tris: Vec<Triangle>) -> Self {
        Self::from_vec_groups(tris, |_| 1, &[])
    }

    pub fn from_vec_groups(tris: Vec<Triangle>, class: impl Fn(u32) -> u8, groups: &[u8]) -> Self {
        let (sets, lo, hi) = Self::sets_of(&tris, class, groups);
        drop(tris);
        Self::from_sets(sets, lo, hi)
    }

    fn sets_of(
        tris: &[Triangle],
        class: impl Fn(u32) -> u8,
        groups: &[u8],
    ) -> (Vec<Vec<(Triangle, u8)>>, Vec3A, Vec3A) {
        let mut sets: Vec<Vec<(Triangle, u8)>> = vec![Vec::new(); groups.len() + 1];
        let (mut lo, mut hi) = (Vec3A::splat(f32::INFINITY), Vec3A::splat(f32::NEG_INFINITY));
        for t in tris {
            let c = class(t.payload);
            if c == 0 || !t.p.iter().all(|p| p.is_finite()) {
                continue;
            }
            for p in &t.p {
                (lo, hi) = (lo.min(*p), hi.max(*p));
            }
            let g = groups.iter().position(|g| g & c == c).unwrap_or(groups.len());
            sets[g].push((*t, c));
        }
        (sets, lo, hi)
    }

    fn from_sets(sets: Vec<Vec<(Triangle, u8)>>, lo: Vec3A, hi: Vec3A) -> Self {
        let count = sets.iter().map(Vec::len).sum();
        let trees: Vec<Tree> = sets.into_iter().filter(|s| !s.is_empty()).map(Tree::build).collect();
        Self { trees, count, bounds: (count > 0).then_some((lo, hi)) }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn bounds(&self) -> Option<(Vec3A, Vec3A)> {
        self.bounds
    }

    pub fn bytes(&self) -> usize {
        self.trees.iter().map(|t| t.nodes.len() * size_of::<Node4>() + t.packs.len() * size_of::<Pack>()).sum()
    }

    pub fn occluded(&self, origin: Vec3A, dir: Vec3A, tmax: f32, filter: impl Fn(u32) -> bool) -> bool {
        self.occluded_in(ALL_CLASSES, origin, dir, tmax, filter)
    }

    pub fn occluded_in(&self, mask: u8, origin: Vec3A, dir: Vec3A, tmax: f32, filter: impl Fn(u32) -> bool) -> bool {
        let ray = Ray::new(origin, dir);
        self.trees.iter().any(|t| t.classes & mask != 0 && t.occluded(t.mask(mask), &ray, tmax, &filter).is_some())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn occluded_hint(
        &self,
        mask: u8,
        origin: Vec3A,
        dir: Vec3A,
        tmax: f32,
        filter: impl Fn(u32) -> bool,
        hint: &mut Option<Blocker>,
    ) -> bool {
        let ray = Ray::new(origin, dir);
        if let Some(b) = *hint
            && let Some(t) = self.trees.get(b.tree as usize)
            && t.classes & mask != 0
            && t.blocks(b.pack, b.lane, t.mask(mask), &ray, tmax, &filter)
        {
            return true;
        }
        for (i, t) in self.trees.iter().enumerate() {
            if t.classes & mask != 0
                && let Some((pack, lane)) = t.occluded(t.mask(mask), &ray, tmax, &filter)
            {
                *hint = Some(Blocker { tree: i as u32, pack, lane });
                return true;
            }
        }
        false
    }

    pub fn hint_distance(
        &self,
        hint: Option<Blocker>,
        mask: u8,
        origin: Vec3A,
        dir: Vec3A,
        tmax: f32,
        filter: impl Fn(u32) -> bool,
    ) -> Option<f32> {
        let b = hint?;
        let t = self.trees.get(b.tree as usize)?;
        if t.classes & mask == 0 {
            return None;
        }
        let p = t.packs.get(b.pack as usize)?;
        let lane = (b.lane & 3) as usize;
        let (hits, dist, ..) = Ray::new(origin, dir).tris(p, tmax);
        (hits & class_bits(p, t.mask(mask)) & (1 << lane) != 0 && filter(p.payload[lane]))
            .then(|| dist.to_array()[lane])
    }

    pub fn each_hit(&self, origin: Vec3A, dir: Vec3A, tmax: f32, visit: impl FnMut(&Hit) -> bool) -> bool {
        self.each_hit_in(ALL_CLASSES, origin, dir, tmax, visit)
    }

    pub fn each_hit_in(
        &self,
        mask: u8,
        origin: Vec3A,
        dir: Vec3A,
        tmax: f32,
        mut visit: impl FnMut(&Hit) -> bool,
    ) -> bool {
        let ray = Ray::new(origin, dir);
        self.trees.iter().all(|t| t.classes & mask == 0 || t.each_hit(t.mask(mask), &ray, tmax, &mut visit))
    }

    pub fn closest(&self, origin: Vec3A, dir: Vec3A, tmax: f32, filter: impl Fn(u32) -> bool) -> Option<Hit> {
        self.closest_in(ALL_CLASSES, origin, dir, tmax, filter)
    }

    pub fn closest_in(
        &self,
        mask: u8,
        origin: Vec3A,
        dir: Vec3A,
        tmax: f32,
        filter: impl Fn(u32) -> bool,
    ) -> Option<Hit> {
        let ray = Ray::new(origin, dir);
        let mut tbest = tmax;
        let mut best = None;
        for t in self.trees.iter().filter(|t| t.classes & mask != 0) {
            if let Some(h) = t.closest(t.mask(mask), &ray, &mut tbest, &filter) {
                best = Some(h);
            }
        }
        best
    }
}

impl Tree {
    fn mask(&self, mask: u8) -> u8 {
        if mask & self.classes == self.classes { ALL_CLASSES } else { mask }
    }

    fn build(tris: Vec<(Triangle, u8)>) -> Tree {
        let mut build: Vec<BuildPrim> = tris
            .iter()
            .enumerate()
            .map(|(i, (t, _))| {
                let min = t.p[0].min(t.p[1]).min(t.p[2]);
                let max = t.p[0].max(t.p[1]).max(t.p[2]);
                BuildPrim { min, max, center: (min + max) * 0.5, index: i as u32 }
            })
            .collect();
        let mut bin = Vec::with_capacity(build.len() / 2 + 1);
        build_rec(&mut build, &mut bin, 0, 0);
        let order: Vec<u32> = build.iter().map(|b| b.index).collect();
        drop(build);
        let ordered: Vec<(Triangle, u8)> = order.iter().map(|&i| tris[i as usize]).collect();
        drop(order);
        drop(tris);
        let root = bin[0];
        let mut c = Collapse {
            bin: &bin,
            tris: &ordered,
            nodes: Vec::with_capacity(bin.len() / 3 + 1),
            packs: Vec::with_capacity(ordered.len() / 3 + 1),
        };
        if root.count > 0 {
            let (first, count, class) = c.leaf(&root);
            let mut n = Node4 {
                lo: [Vec4::splat(f32::INFINITY); 3],
                hi: [Vec4::splat(f32::NEG_INFINITY); 3],
                child: [0; 4],
                meta: [0; 4],
            };
            for axis in 0..3 {
                n.lo[axis][0] = pad_lo(root.min[axis]);
                n.hi[axis][0] = pad_hi(root.max[axis]);
            }
            n.child[0] = first;
            n.meta[0] = count | (u32::from(class) << 24);
            c.nodes.push(n);
        } else {
            c.node(0);
        }
        let classes = ordered.iter().fold(0, |a, t| a | t.1);
        Tree { nodes: c.nodes, packs: c.packs, classes }
    }

    fn occluded(&self, mask: u8, ray: &Ray, tmax: f32, filter: &impl Fn(u32) -> bool) -> Option<(u32, u32)> {
        let mut stack = [0u32; STACK];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let node = &self.nodes[stack[sp] as usize];
            let (mut bits, _) = ray.boxes(node, tmax, mask);
            while bits != 0 {
                let lane = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let count = node.meta[lane] & COUNT_MASK;
                if count == 0 {
                    if sp < STACK {
                        stack[sp] = node.child[lane];
                        sp += 1;
                    }
                    continue;
                }
                let first = node.child[lane] as usize;
                for (i, p) in self.packs[first..first + count as usize].iter().enumerate() {
                    let (mut hits, ..) = ray.tris(p, tmax);
                    hits &= class_bits(p, mask);
                    while hits != 0 {
                        let k = hits.trailing_zeros() as usize;
                        hits &= hits - 1;
                        if filter(p.payload[k]) {
                            return Some(((first + i) as u32, k as u32));
                        }
                    }
                }
            }
        }
        None
    }

    fn blocks(&self, pack: u32, lane: u32, mask: u8, ray: &Ray, tmax: f32, filter: &impl Fn(u32) -> bool) -> bool {
        let Some(p) = self.packs.get(pack as usize) else { return false };
        let bit = 1u32 << (lane & 3);
        let (hits, ..) = ray.tris(p, tmax);
        hits & class_bits(p, mask) & bit != 0 && filter(p.payload[(lane & 3) as usize])
    }

    fn each_hit(&self, mask: u8, ray: &Ray, tmax: f32, visit: &mut impl FnMut(&Hit) -> bool) -> bool {
        let mut stack = [0u32; STACK];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let node = &self.nodes[stack[sp] as usize];
            let (mut bits, _) = ray.boxes(node, tmax, mask);
            while bits != 0 {
                let lane = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let count = node.meta[lane] & COUNT_MASK;
                if count == 0 {
                    if sp < STACK {
                        stack[sp] = node.child[lane];
                        sp += 1;
                    }
                    continue;
                }
                let first = node.child[lane] as usize;
                for p in &self.packs[first..first + count as usize] {
                    let (mut hits, t, u, v, det) = ray.tris(p, tmax);
                    hits &= class_bits(p, mask);
                    if hits == 0 {
                        continue;
                    }
                    let (t, u, v, det) = (t.to_array(), u.to_array(), v.to_array(), det.to_array());
                    while hits != 0 {
                        let k = hits.trailing_zeros() as usize;
                        hits &= hits - 1;
                        if !visit(&Hit { t: t[k], u: u[k], v: v[k], payload: p.payload[k], front: det[k] > 0.0 }) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    fn closest(&self, mask: u8, ray: &Ray, tbest: &mut f32, filter: &impl Fn(u32) -> bool) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        let mut stack = [(0u32, 0.0f32); STACK];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let (index, entry) = stack[sp];
            if entry > *tbest {
                continue;
            }
            let node = &self.nodes[index as usize];
            let (bits, near) = ray.boxes(node, *tbest, mask);
            if bits == 0 {
                continue;
            }
            let (order, n) = far_to_near(bits, near);
            let near = near.to_array();
            for &lane in order[..n].iter().rev() {
                let lane = lane as usize;
                if near[lane] > *tbest {
                    continue;
                }
                let count = node.meta[lane] & COUNT_MASK;
                if count > 0 {
                    let first = node.child[lane] as usize;
                    for p in &self.packs[first..first + count as usize] {
                        let (mut hits, t, u, v, det) = ray.tris(p, *tbest);
                        hits &= class_bits(p, mask);
                        if hits == 0 {
                            continue;
                        }
                        let (t, u, v, det) = (t.to_array(), u.to_array(), v.to_array(), det.to_array());
                        while hits != 0 {
                            let k = hits.trailing_zeros() as usize;
                            hits &= hits - 1;
                            if t[k] < *tbest && filter(p.payload[k]) {
                                *tbest = t[k];
                                best =
                                    Some(Hit { t: t[k], u: u[k], v: v[k], payload: p.payload[k], front: det[k] > 0.0 });
                            }
                        }
                    }
                }
            }
            for &lane in &order[..n] {
                let lane = lane as usize;
                if node.meta[lane] & COUNT_MASK == 0 && near[lane] <= *tbest && sp < STACK {
                    stack[sp] = (node.child[lane], near[lane]);
                    sp += 1;
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(min: Vec3A, max: Vec3A, z: f32, payload: u32) -> [Triangle; 2] {
        let a = Vec3A::new(min.x, min.y, z);
        let b = Vec3A::new(max.x, min.y, z);
        let c = Vec3A::new(max.x, max.y, z);
        let d = Vec3A::new(min.x, max.y, z);
        [Triangle { p: [a, b, c], payload }, Triangle { p: [a, c, d], payload }]
    }

    fn hit_tri(t: &Triangle, o: Vec3A, d: Vec3A, tmax: f32) -> Option<(f32, f32, f32, bool)> {
        let (v0, e1, e2) = (t.p[0], t.p[1] - t.p[0], t.p[2] - t.p[0]);
        let pvec = d.cross(e2);
        let det = e1.dot(pvec);
        if det.abs() < 1e-12 {
            return None;
        }
        let inv_det = 1.0 / det;
        let tvec = o - v0;
        let u = tvec.dot(pvec) * inv_det;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let qvec = tvec.cross(e1);
        let v = d.dot(qvec) * inv_det;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = e2.dot(qvec) * inv_det;
        (t > T_MIN && t < tmax).then_some((t, u, v, det > 0.0))
    }

    fn brute(
        tris: &[Triangle],
        o: Vec3A,
        d: Vec3A,
        tmax: f32,
        keep: impl Fn(u32) -> bool,
    ) -> Option<(f32, u32, f32, f32, bool)> {
        let mut best: Option<(f32, u32, f32, f32, bool)> = None;
        for t in tris.iter().filter(|t| keep(t.payload)) {
            if let Some((h, u, v, f)) = hit_tri(t, o, d, best.map_or(tmax, |b| b.0)) {
                best = Some((h, t.payload, u, v, f));
            }
        }
        best
    }

    fn insertion_order(bits: u32, near: Vec4) -> Vec<u8> {
        let t = near.to_array();
        let mut order: Vec<u8> = Vec::new();
        for lane in (0..4u8).filter(|l| bits & (1 << l) != 0) {
            let at = order.iter().position(|&o| t[o as usize] < t[lane as usize]).unwrap_or(order.len());
            order.insert(at, lane);
        }
        order
    }

    #[test]
    fn child_order_is_far_to_near_with_stable_ties() {
        let values = [0.0f32, 1.0, 1.0, 2.5, 7.0, f32::INFINITY];
        let mut checked = 0;
        for bits in 0u32..16 {
            for a in values {
                for b in values {
                    for c in values {
                        for d in values {
                            let near = Vec4::new(a, b, c, d);
                            let (order, n) = far_to_near(bits, near);
                            assert_eq!(order[..n].to_vec(), insertion_order(bits, near), "{bits:04b} {near}");
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 16 * 6usize.pow(4));
    }

    #[test]
    fn empty_and_simple() {
        let b = Bvh::new(&[]);
        assert!(b.is_empty());
        assert!(!b.occluded(Vec3A::ZERO, Vec3A::Z, 100.0, |_| true));
        assert!(b.closest(Vec3A::ZERO, Vec3A::Z, 100.0, |_| true).is_none());
        assert!(b.bounds().is_none());

        let tris = quad(Vec3A::splat(-10.0), Vec3A::splat(10.0), 5.0, 7);
        let b = Bvh::new(&tris);
        assert!(b.occluded(Vec3A::ZERO, Vec3A::Z, 100.0, |_| true));
        assert!(!b.occluded(Vec3A::ZERO, Vec3A::Z, 4.0, |_| true), "beyond tmax");
        assert!(!b.occluded(Vec3A::ZERO, Vec3A::Z, 100.0, |p| p != 7), "filtered");
        assert!(b.occluded(Vec3A::new(0.0, 0.0, 10.0), -Vec3A::Z, 100.0, |_| true));
        let h = b.closest(Vec3A::ZERO, Vec3A::Z, 100.0, |_| true).unwrap();
        assert!((h.t - 5.0).abs() < 1e-5);
        assert_eq!(h.payload, 7);
        assert!(!b.occluded(Vec3A::new(20.0, 0.0, 0.0), Vec3A::Z, 100.0, |_| true));
        let (lo, hi) = b.bounds().unwrap();
        assert_eq!((lo, hi), (Vec3A::new(-10.0, -10.0, 5.0), Vec3A::new(10.0, 10.0, 5.0)));
        assert!(b.bytes() > 0);
    }

    fn soup(n: u32, seed: &mut u32) -> Vec<Triangle> {
        let mut rnd = || {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 17;
            *seed ^= *seed << 5;
            (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        (0..n)
            .map(|i| {
                let c = Vec3A::new(rnd(), rnd(), rnd()) * 500.0;
                let p = [c + Vec3A::new(rnd(), rnd(), rnd()) * 40.0, c + Vec3A::new(rnd(), rnd(), rnd()) * 40.0, c];
                Triangle { p, payload: i }
            })
            .collect()
    }

    #[test]
    fn matches_brute_force() {
        let mut seed = 12345u32;
        let tris = soup(3000, &mut seed);
        let b = Bvh::new(&tris);
        assert_eq!(b.len(), tris.len());
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        for i in 0..1200 {
            let o = Vec3A::new(rnd(), rnd(), rnd()) * 600.0;
            let d = match i % 4 {
                0 => [Vec3A::X, -Vec3A::Y, Vec3A::Z][i / 4 % 3],
                _ => Vec3A::new(rnd(), rnd(), rnd()).normalize_or(Vec3A::X),
            };
            let want = brute(&tris, o, d, 2000.0, |_| true);
            let got = b.closest(o, d, 2000.0, |_| true);
            assert_eq!(want.map(|w| w.1), got.map(|g| g.payload));
            if let (Some(w), Some(g)) = (want, got) {
                assert_eq!((w.0, w.2, w.3, w.4), (g.t, g.u, g.v, g.front));
            }
            assert_eq!(want.is_some(), b.occluded(o, d, 2000.0, |_| true));
            let keep = |p: u32| !p.is_multiple_of(3);
            let want = brute(&tris, o, d, 2000.0, keep);
            assert_eq!(want.map(|w| w.1), b.closest(o, d, 2000.0, keep).map(|g| g.payload));
            assert_eq!(want.is_some(), b.occluded(o, d, 2000.0, keep));
            let mut all: Vec<u32> =
                tris.iter().filter(|t| hit_tri(t, o, d, 2000.0).is_some()).map(|t| t.payload).collect();
            let mut seen = Vec::new();
            assert!(b.each_hit(o, d, 2000.0, |h| {
                seen.push(h.payload);
                true
            }));
            all.sort_unstable();
            seen.sort_unstable();
            assert_eq!(all, seen);
        }
    }

    #[test]
    fn classes() {
        let mut seed = 777u32;
        let tris = soup(2000, &mut seed);
        let class = |p: u32| if p.is_multiple_of(7) { 0 } else { 1u8 << (p % 3) };
        let one = Bvh::with_classes(&tris, class);
        let grouped = Bvh::with_groups(&tris, class, &[2, 1 | 8]);
        assert_eq!(grouped.trees.len(), 3);
        for b in [&one, &grouped] {
            assert_eq!(b.len(), tris.iter().filter(|t| class(t.payload) != 0).count());
        }
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        for _ in 0..600 {
            let o = Vec3A::new(rnd(), rnd(), rnd()) * 600.0;
            let d = Vec3A::new(rnd(), rnd(), rnd()).normalize_or(Vec3A::X);
            for (b, mask) in [1u8, 2, 4, 3, 5, 6, ALL_CLASSES].into_iter().flat_map(|m| [(&one, m), (&grouped, m)]) {
                let keep = |p: u32| class(p) & mask != 0;
                let want = brute(&tris, o, d, 2000.0, keep);
                assert_eq!(want.map(|w| w.1), b.closest_in(mask, o, d, 2000.0, |_| true).map(|g| g.payload));
                assert_eq!(want.is_some(), b.occluded_in(mask, o, d, 2000.0, |_| true));
                let mut n = 0;
                b.each_hit_in(mask, o, d, 2000.0, |h| {
                    assert!(keep(h.payload));
                    n += 1;
                    true
                });
                assert_eq!(n, tris.iter().filter(|t| keep(t.payload) && hit_tri(t, o, d, 2000.0).is_some()).count());
            }
        }
    }

    #[test]
    fn hints() {
        let mut seed = 99u32;
        let tris = soup(3000, &mut seed);
        let class = |p: u32| 1u8 << (p % 3);
        let b = Bvh::with_groups(&tris, class, &[2]);
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let mut hint = None;
        let mut used = 0;
        for _ in 0..2000 {
            let o = Vec3A::new(rnd(), rnd(), rnd()) * 600.0;
            let d = Vec3A::new(rnd(), rnd(), rnd()).normalize_or(Vec3A::X);
            for mask in [1u8, 2, 5, ALL_CLASSES] {
                let keep = |p: u32| !p.is_multiple_of(5);
                let want = b.occluded_in(mask, o, d, 900.0, keep);
                let before = hint;
                if let Some(t) = b.hint_distance(before, mask, o, d, 900.0, keep) {
                    assert!(t > T_MIN && t < 900.0);
                    assert!(want, "a hint on the ray blocks it");
                    let blocker = tris
                        .iter()
                        .filter(|x| keep(x.payload) && class(x.payload) & mask != 0)
                        .filter_map(|x| hit_tri(x, o, d, 900.0))
                        .any(|h| h.0 == t);
                    assert!(blocker, "{t}");
                }
                assert_eq!(want, b.occluded_hint(mask, o, d, 900.0, keep, &mut hint));
                used += usize::from(want && before == hint);
            }
        }
        assert!(used > 0);
    }

    #[test]
    fn edges_are_never_culled() {
        let mut tris = Vec::new();
        for x in 0..12 {
            for y in 0..12 {
                let min = Vec3A::new(x as f32 * 64.0 - 300.0, y as f32 * 64.0 + 1000.0, 0.0);
                tris.extend(quad(min, min + Vec3A::new(64.0, 64.0, 0.0), 517.25 + (x % 3) as f32, x * 100 + y));
            }
        }
        let b = Bvh::new(&tris);
        let mut seed = 4242u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32
        };
        let mut checked = 0;
        for _ in 0..4000 {
            let (gx, gy) = ((rnd() * 12.0).floor(), (rnd() * 12.0).floor());
            let along = if rnd() < 0.5 { 0.0 } else { (rnd() * 64.0).floor() };
            let target = Vec3A::new(gx * 64.0 - 300.0 + along, gy * 64.0 + 1000.0, 517.25 + (gx as u32 % 3) as f32);
            let o =
                target + Vec3A::new(rnd() - 0.5, rnd() - 0.5, rnd() * 0.5 + 0.1).normalize() * (rnd() * 3000.0 + 10.0);
            let d = (target - o).normalize();
            let want = tris.iter().any(|t| hit_tri(t, o, d, 1e5).is_some());
            assert_eq!(want, b.occluded(o, d, 1e5, |_| true), "{o} {d}");
            assert_eq!(want, b.closest(o, d, 1e5, |_| true).is_some());
            checked += usize::from(want);
        }
        assert!(checked > 1000);
    }

    #[test]
    fn each_hit_stops() {
        let tris: Vec<Triangle> =
            (0..10).flat_map(|i| quad(Vec3A::splat(-10.0), Vec3A::splat(10.0), i as f32 + 1.0, i)).collect();
        let b = Bvh::new(&tris);
        let mut n = 0;
        assert!(!b.each_hit(Vec3A::new(0.1, 0.2, 0.0), Vec3A::Z, 100.0, |_| {
            n += 1;
            n < 3
        }));
        assert_eq!(n, 3);
        let h = b.closest(Vec3A::new(0.1, 0.2, 0.0), Vec3A::Z, 100.0, |p| p >= 4).unwrap();
        assert_eq!(h.payload, 4);
        assert!(!h.front);
        let h = b.closest(Vec3A::new(0.1, 0.2, 20.0), -Vec3A::Z, 100.0, |_| true).unwrap();
        assert_eq!(h.payload, 9);
        assert!(h.front);
    }

    #[test]
    fn large_and_degenerate_inputs() {
        let t = Triangle { p: [Vec3A::ZERO, Vec3A::X, Vec3A::Y], payload: 1 };
        let tris = vec![t; PARALLEL_PRIMS * 2 + 3];
        let b = Bvh::new(&tris);
        assert_eq!(b.len(), tris.len());
        assert!(b.occluded(Vec3A::new(0.2, 0.2, 1.0), -Vec3A::Z, 10.0, |_| true));
        let mut n = 0;
        b.each_hit(Vec3A::new(0.2, 0.2, 1.0), -Vec3A::Z, 10.0, |_| {
            n += 1;
            true
        });
        assert_eq!(n, tris.len());
        let bad = [
            Triangle { p: [Vec3A::splat(f32::NAN), Vec3A::X, Vec3A::Y], payload: 0 },
            Triangle { p: [Vec3A::ZERO, Vec3A::ZERO, Vec3A::ZERO], payload: 0 },
        ];
        let b = Bvh::new(&bad);
        assert_eq!(b.len(), 1);
        assert!(!b.occluded(Vec3A::new(0.0, 0.0, 1.0), -Vec3A::Z, 10.0, |_| true));
        let mut grid = Vec::new();
        for x in 0..120 {
            for y in 0..120 {
                let min = Vec3A::new(x as f32 * 10.0, y as f32 * 10.0, 0.0);
                grid.extend(quad(min, min + Vec3A::splat(9.0), (x + y) as f32, x * 1000 + y));
            }
        }
        let b = Bvh::new(&grid);
        let h = b.closest(Vec3A::new(55.0, 75.0, 1000.0), -Vec3A::Z, 2000.0, |_| true).unwrap();
        assert_eq!(h.payload, 5 * 1000 + 7);
        assert!((h.t - (1000.0 - 12.0)).abs() < 1e-3);
        assert!(!b.occluded(Vec3A::new(9.5, 9.5, 1000.0), -Vec3A::Z, 2000.0, |_| true));
        assert!(b.occluded(Vec3A::new(8.5, 8.5, 1000.0), -Vec3A::Z, 2000.0, |_| true));
    }

    #[test]
    fn an_owned_build_gives_the_same_tree() {
        let mut grid = Vec::new();
        for x in 0..40 {
            for y in 0..40 {
                let min = Vec3A::new(x as f32 * 10.0, y as f32 * 10.0, 0.0);
                grid.extend(quad(min, min + Vec3A::splat(9.0), (x + y) as f32, x * 1000 + y));
            }
        }
        let class = |p: u32| if p.is_multiple_of(2) { 1 } else { 2 };
        let borrowed = Bvh::with_groups(&grid, class, &[1]);
        let owned = Bvh::from_vec_groups(grid.clone(), class, &[1]);
        assert_eq!(borrowed.len(), owned.len());
        assert_eq!(borrowed.bytes(), owned.bytes());
        for k in 0..50 {
            let o = Vec3A::new(5.0 + k as f32 * 7.0, 5.0 + k as f32 * 3.0, 500.0);
            let a = borrowed.closest(o, -Vec3A::Z, 1000.0, |_| true).map(|h| (h.payload, h.t));
            let b = owned.closest(o, -Vec3A::Z, 1000.0, |_| true).map(|h| (h.payload, h.t));
            assert_eq!(a, b);
        }
        assert_eq!(Bvh::from_vec(grid.clone()).len(), Bvh::new(&grid).len());
    }
}
