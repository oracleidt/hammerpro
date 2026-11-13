use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use glam::{DVec2, DVec3};
use hp_geom::{Aabb, Plane, clip_polygon};
use rayon::prelude::*;

use super::checks::MaterialFacts;
use crate::ids::IdBuildHasher;
use crate::scene::Owner;
use crate::{Document, Id, ObjRef};

pub const ROOT_MIN: f64 = -32768.0;
pub const ROOT_SIZE: f64 = 65536.0;
const MAX_LEVEL: u8 = 16;
const DEEPEST: u8 = MAX_LEVEL + 3;
const MIN_CELL: f64 = 4.0;
const LEAF_BRUSHES: usize = 6;
const MAX_PIECES: usize = 192;
pub const MAX_LEAVES: usize = 3_000_000;
const EPS: f64 = 0.001;
pub const THIN: f64 = 0.1;
const PORTAL_EDGE: f64 = THIN;
const AREA_EPS: f64 = 1e-3;
const TOUCH: f64 = 1e-9;
const MAX_REPORTED: usize = 256;

#[derive(Clone, Debug)]
pub struct SealBrush {
    pub id: Id,
    pub planes: Vec<Plane>,
    pub aabb: Aabb,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeakSeed {
    pub entity: Id,
    pub class: String,
    pub origin: DVec3,
}

#[derive(Clone, Debug, Default)]
pub struct LeakInput {
    pub brushes: Vec<SealBrush>,
    pub seeds: Vec<LeakSeed>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LeakPath {
    pub entity: Id,
    pub points: Vec<DVec3>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeakReport {
    pub leak: Option<LeakPath>,
    pub leaking: Vec<Id>,
    pub in_solid: Vec<Id>,
    pub hull: Vec<Id>,
    pub cells: usize,
    pub incomplete: bool,
    pub no_world: bool,
    pub compiler: bool,
}

impl LeakReport {
    pub fn leaks(&self) -> bool {
        self.leak.is_some()
    }
}

#[derive(Clone, Debug)]
struct Brush {
    planes: Vec<Plane>,
    aabb: Aabb,
    axial: bool,
}

impl Brush {
    fn new(b: &SealBrush) -> Brush {
        let box_planes = box_planes(b.aabb.min, b.aabb.max);
        let axial = b.planes.iter().all(|p| {
            let n = p.normal.abs();
            [n.x, n.y, n.z].iter().filter(|&&c| (c - 1.0).abs() < 1e-12).count() == 1
        }) && box_planes.iter().all(|bp| b.planes.iter().any(|p| p.approx_eq(bp, 1e-12, 1e-9)));
        Brush { planes: b.planes.clone(), aabb: b.aabb, axial }
    }
}

fn boxes_overlap(a: &Aabb, b: &Aabb) -> bool {
    a.max.x > b.min.x + EPS
        && a.min.x < b.max.x - EPS
        && a.max.y > b.min.y + EPS
        && a.min.y < b.max.y - EPS
        && a.max.z > b.min.z + EPS
        && a.min.z < b.max.z - EPS
}

fn brush_overlaps(b: &Brush, cell: &Aabb) -> bool {
    if !boxes_overlap(&b.aabb, cell) {
        return false;
    }
    if b.axial {
        return true;
    }
    let corners = cell.corners();
    !b.planes.iter().any(|pl| corners.iter().all(|&c| pl.distance(c) >= -EPS))
}

fn brush_contains(b: &Brush, cell: &Aabb) -> bool {
    if b.axial {
        return b.aabb.min.x <= cell.min.x + EPS
            && b.aabb.min.y <= cell.min.y + EPS
            && b.aabb.min.z <= cell.min.z + EPS
            && b.aabb.max.x >= cell.max.x - EPS
            && b.aabb.max.y >= cell.max.y - EPS
            && b.aabb.max.z >= cell.max.z - EPS;
    }
    let corners = cell.corners();
    b.planes.iter().all(|pl| corners.iter().all(|&c| pl.distance(c) <= EPS))
}

fn box_planes(min: DVec3, max: DVec3) -> [Plane; 6] {
    [
        Plane { normal: DVec3::NEG_X, dist: -min.x },
        Plane { normal: DVec3::X, dist: max.x },
        Plane { normal: DVec3::NEG_Y, dist: -min.y },
        Plane { normal: DVec3::Y, dist: max.y },
        Plane { normal: DVec3::NEG_Z, dist: -min.z },
        Plane { normal: DVec3::Z, dist: max.z },
    ]
}

#[derive(Clone, Debug)]
struct Face {
    plane: Plane,
    pts: Vec<DVec3>,
    side: Option<u8>,
}

#[derive(Clone, Debug)]
struct Piece {
    faces: Vec<Face>,
    aabb: Aabb,
}

fn box_piece(min: DVec3, max: DVec3) -> Piece {
    let c = |x: bool, y: bool, z: bool| {
        DVec3::new(if x { max.x } else { min.x }, if y { max.y } else { min.y }, if z { max.z } else { min.z })
    };
    let quads: [[DVec3; 4]; 6] = [
        [c(false, false, false), c(false, false, true), c(false, true, true), c(false, true, false)],
        [c(true, false, false), c(true, true, false), c(true, true, true), c(true, false, true)],
        [c(false, false, false), c(true, false, false), c(true, false, true), c(false, false, true)],
        [c(false, true, false), c(false, true, true), c(true, true, true), c(true, true, false)],
        [c(false, false, false), c(false, true, false), c(true, true, false), c(true, false, false)],
        [c(false, false, true), c(true, false, true), c(true, true, true), c(false, true, true)],
    ];
    let planes = box_planes(min, max);
    Piece {
        faces: quads
            .iter()
            .zip(planes)
            .enumerate()
            .map(|(i, (q, plane))| Face { plane, pts: q.to_vec(), side: Some(i as u8) })
            .collect(),
        aabb: Aabb::new(min, max),
    }
}

fn cap_square(plane: &Plane, center: DVec3, half: f64) -> Vec<DVec3> {
    let n = plane.normal;
    let a = n.abs();
    let up = if a.z >= a.x && a.z >= a.y { DVec3::X } else { DVec3::Z };
    let u = (up - n * n.dot(up)).try_normalize().unwrap_or(DVec3::X);
    let v = n.cross(u);
    let o = center - n * plane.distance(center);
    let (u, v) = (u * half, v * half);
    vec![o - u - v, o + u - v, o + u + v, o - u + v]
}

impl Piece {
    fn from_faces(faces: Vec<Face>) -> Piece {
        let mut aabb = Aabb::EMPTY;
        for f in &faces {
            for &p in &f.pts {
                aabb.extend(p);
            }
        }
        Piece { faces, aabb }
    }

    fn vertices(&self) -> impl Iterator<Item = DVec3> + '_ {
        self.faces.iter().flat_map(|f| f.pts.iter().copied())
    }

    fn center(&self) -> DVec3 {
        let mut sum = DVec3::ZERO;
        let mut n = 0.0;
        for p in self.vertices() {
            sum += p;
            n += 1.0;
        }
        if n > 0.0 { sum / n } else { DVec3::ZERO }
    }

    fn thickness(&self) -> f64 {
        let mut thinnest = f64::INFINITY;
        for f in &self.faces {
            let depth = self.vertices().map(|p| -f.plane.distance(p)).fold(0.0f64, f64::max);
            thinnest = thinnest.min(depth);
            if thinnest < THIN {
                break;
            }
        }
        thinnest
    }

    fn usable(&self) -> bool {
        self.faces.len() >= 4 && self.thickness() >= THIN
    }

    fn split(self, plane: &Plane, half: f64) -> (Option<Piece>, Option<Piece>) {
        let (mut front_n, mut back_n) = (0usize, 0usize);
        for p in self.vertices() {
            let d = plane.distance(p);
            if d > EPS {
                front_n += 1;
            } else if d < -EPS {
                back_n += 1;
            }
        }
        if back_n == 0 {
            return (Some(self), None);
        }
        if front_n == 0 {
            return (None, Some(self));
        }
        let mut front = Vec::with_capacity(self.faces.len() + 1);
        let mut back = Vec::with_capacity(self.faces.len() + 1);
        for f in &self.faces {
            let (fp, bp) = clip_polygon(&f.pts, plane, EPS);
            if fp.len() >= 3 {
                front.push(Face { plane: f.plane, pts: fp, side: f.side });
            }
            if bp.len() >= 3 {
                back.push(Face { plane: f.plane, pts: bp, side: f.side });
            }
        }
        let mut cap = cap_square(plane, self.center(), half);
        for f in &self.faces {
            if cap.len() < 3 {
                break;
            }
            cap = clip_polygon(&cap, &f.plane, EPS).1;
        }
        if cap.len() >= 3 {
            let flipped = Plane { normal: -plane.normal, dist: -plane.dist };
            let rev: Vec<DVec3> = cap.iter().rev().copied().collect();
            front.push(Face { plane: flipped, pts: rev, side: None });
            back.push(Face { plane: *plane, pts: cap, side: None });
        }
        let make = |faces: Vec<Face>| {
            let p = Piece::from_faces(faces);
            p.usable().then_some(p)
        };
        (make(front), make(back))
    }
}

fn subtract(piece: Piece, brush: &Brush, half: f64, out: &mut Vec<Piece>) {
    if !boxes_overlap(&piece.aabb, &brush.aabb)
        || (!brush.axial && brush.planes.iter().any(|pl| piece.vertices().all(|p| pl.distance(p) >= -EPS)))
    {
        out.push(piece);
        return;
    }
    let mut cur = piece;
    for plane in &brush.planes {
        let (front, back) = cur.split(plane, half);
        if let Some(f) = front {
            out.push(f);
        }
        match back {
            Some(b) => cur = b,
            None => return,
        }
    }
}

fn subtract_box(p: Aabb, b: &Aabb, out: &mut Vec<Aabb>) {
    if !boxes_overlap(&p, b) {
        out.push(p);
        return;
    }
    let thick = |a: &Aabb| {
        let s = a.size();
        s.x >= THIN && s.y >= THIN && s.z >= THIN
    };
    let mut rest = p;
    for axis in 0..3 {
        if b.min[axis] > rest.min[axis] + EPS {
            let mut lo = rest;
            lo.max[axis] = b.min[axis];
            if thick(&lo) {
                out.push(lo);
            }
            rest.min[axis] = b.min[axis];
        }
        if b.max[axis] < rest.max[axis] - EPS {
            let mut hi = rest;
            hi.min[axis] = b.max[axis];
            if thick(&hi) {
                out.push(hi);
            }
            rest.max[axis] = b.max[axis];
        }
    }
}

fn side_axes(side: u8) -> (usize, usize) {
    match side / 2 {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

fn signed_area(poly: &[DVec2]) -> f64 {
    let mut s = 0.0;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        s += a.perp_dot(b);
    }
    s * 0.5
}

fn ccw(mut poly: Vec<DVec2>) -> Vec<DVec2> {
    if signed_area(&poly) < 0.0 {
        poly.reverse();
    }
    poly
}

fn clip_2d(subject: &[DVec2], clip: &[DVec2]) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = subject.to_vec();
    let mut input: Vec<DVec2> = Vec::with_capacity(subject.len() + clip.len());
    let n = clip.len();
    for i in 0..n {
        if out.len() < 3 {
            return Vec::new();
        }
        let (a, b) = (clip[i], clip[(i + 1) % n]);
        let edge = b - a;
        let inside = |p: DVec2| edge.perp_dot(p - a) >= -1e-9;
        std::mem::swap(&mut input, &mut out);
        out.clear();
        let m = input.len();
        for j in 0..m {
            let (p, q) = (input[j], input[(j + 1) % m]);
            let (pi, qi) = (inside(p), inside(q));
            if pi {
                out.push(p);
            }
            if pi != qi {
                let (dp, dq) = (edge.perp_dot(p - a), edge.perp_dot(q - a));
                let t = dp / (dp - dq);
                if t.is_finite() {
                    out.push(p + (q - p) * t);
                }
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
enum SidePoly {
    Rect(DVec2, DVec2),
    Poly(Vec<DVec2>),
}

fn wide_enough(poly: &[DVec2]) -> bool {
    let n = poly.len();
    (0..n).filter(|&i| poly[i].distance(poly[(i + 1) % n]) > PORTAL_EDGE).take(3).count() == 3
}

fn rect_poly(min: DVec2, max: DVec2) -> [DVec2; 4] {
    [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
}

fn poly_overlap(a: &SidePoly, b: &SidePoly) -> Option<(f64, DVec2)> {
    let clipped = |pa: &[DVec2], pb: &[DVec2]| {
        let o = clip_2d(pa, pb);
        if o.len() < 3 || !wide_enough(&o) {
            return None;
        }
        let area = signed_area(&o).abs();
        (area > AREA_EPS).then(|| (area, o.iter().copied().sum::<DVec2>() / o.len() as f64))
    };
    match (a, b) {
        (SidePoly::Rect(a0, a1), SidePoly::Rect(b0, b1)) => {
            let (lo, hi) = (a0.max(*b0), a1.min(*b1));
            let size = hi - lo;
            (size.x > PORTAL_EDGE && size.y > PORTAL_EDGE && size.x * size.y > AREA_EPS)
                .then(|| (size.x * size.y, (lo + hi) * 0.5))
        }
        (SidePoly::Rect(r0, r1), SidePoly::Poly(p)) | (SidePoly::Poly(p), SidePoly::Rect(r0, r1)) => {
            clipped(p, &rect_poly(*r0, *r1))
        }
        (SidePoly::Poly(pa), SidePoly::Poly(pb)) => clipped(pa, pb),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CellKey {
    level: u8,
    x: i32,
    y: i32,
    z: i32,
}

impl CellKey {
    const ROOT: CellKey = CellKey { level: 0, x: 0, y: 0, z: 0 };

    fn size(self) -> f64 {
        ROOT_SIZE / f64::from(1u32 << self.level)
    }

    fn min(self) -> DVec3 {
        let s = self.size();
        DVec3::new(ROOT_MIN + f64::from(self.x) * s, ROOT_MIN + f64::from(self.y) * s, ROOT_MIN + f64::from(self.z) * s)
    }

    fn max(self) -> DVec3 {
        self.min() + DVec3::splat(self.size())
    }

    fn aabb(self) -> Aabb {
        Aabb::new(self.min(), self.max())
    }

    fn child(self, i: usize) -> CellKey {
        CellKey {
            level: self.level + 1,
            x: self.x * 2 + (i & 1) as i32,
            y: self.y * 2 + ((i >> 1) & 1) as i32,
            z: self.z * 2 + ((i >> 2) & 1) as i32,
        }
    }

    fn parent(self) -> CellKey {
        CellKey { level: self.level.saturating_sub(1), x: self.x >> 1, y: self.y >> 1, z: self.z >> 1 }
    }

    fn containing(level: u8, p: DVec3) -> Option<CellKey> {
        let n = f64::from(1u32 << level);
        let s = ROOT_SIZE / n;
        let idx = |v: f64| {
            let i = ((v - ROOT_MIN) / s).floor();
            (i >= 0.0 && i < n).then_some(i as i32)
        };
        Some(CellKey { level, x: idx(p.x)?, y: idx(p.y)?, z: idx(p.z)? })
    }

    fn border_sides(self) -> [bool; 6] {
        let last = (1i64 << self.level) - 1;
        let mut out = [false; 6];
        for (axis, c) in [self.x, self.y, self.z].into_iter().enumerate() {
            out[axis * 2] = c == 0;
            out[axis * 2 + 1] = i64::from(c) == last;
        }
        out
    }
}

#[derive(Clone, Debug)]
enum PieceShape {
    Box(Aabb),
    Planes { planes: Vec<Plane>, center: DVec3, aabb: Aabb },
}

impl PieceShape {
    fn contains(&self, p: DVec3) -> bool {
        match self {
            PieceShape::Box(b) => b.expand(EPS * 10.0).contains_point(p),
            PieceShape::Planes { planes, .. } => planes.iter().all(|pl| pl.distance(p) <= EPS * 10.0),
        }
    }

    fn center(&self) -> DVec3 {
        match self {
            PieceShape::Box(b) => b.center(),
            PieceShape::Planes { center, .. } => *center,
        }
    }

    fn aabb(&self) -> &Aabb {
        match self {
            PieceShape::Box(b) | PieceShape::Planes { aabb: b, .. } => b,
        }
    }
}

#[derive(Clone, Debug)]
struct Component {
    sides: Vec<(u8, SidePoly)>,
    pieces: Vec<PieceShape>,
    links: Vec<(u16, u16, DVec3)>,
}

#[derive(Clone, Debug)]
enum LeafKind {
    Empty,
    Solid,
    Pending,
    Mixed(Vec<Component>),
}

#[derive(Clone, Debug)]
struct Leaf {
    key: CellKey,
    kind: LeafKind,
    brushes: Vec<u32>,
}

impl Leaf {
    fn component_count(&self) -> usize {
        match &self.kind {
            LeafKind::Empty => 1,
            LeafKind::Solid | LeafKind::Pending => 0,
            LeafKind::Mixed(c) => c.len(),
        }
    }

    fn for_side_polys(&self, c: usize, side: u8, mut f: impl FnMut(&SidePoly)) {
        match &self.kind {
            LeafKind::Empty => {
                let (a, b) = side_axes(side);
                let (min, max) = (self.key.min(), self.key.max());
                f(&SidePoly::Rect(DVec2::new(min[a], min[b]), DVec2::new(max[a], max[b])));
            }
            LeafKind::Solid | LeafKind::Pending => {}
            LeafKind::Mixed(comps) => {
                if let Some(k) = comps.get(c) {
                    for (s, p) in &k.sides {
                        if *s == side {
                            f(p);
                        }
                    }
                }
            }
        }
    }

    fn has_side(&self, c: usize, side: u8) -> bool {
        let mut any = false;
        self.for_side_polys(c, side, |_| any = true);
        any
    }

    fn component_at(&self, p: DVec3) -> Option<usize> {
        match &self.kind {
            LeafKind::Empty => Some(0),
            LeafKind::Solid | LeafKind::Pending => None,
            LeafKind::Mixed(comps) => comps.iter().position(|c| c.pieces.iter().any(|piece| piece.contains(p))),
        }
    }
}

fn components_overlap(a: &Leaf, ca: usize, side_a: u8, b: &Leaf, cb: usize, side_b: u8) -> Option<DVec2> {
    let mut best: Option<(f64, DVec2)> = None;
    a.for_side_polys(ca, side_a, |pa| {
        b.for_side_polys(cb, side_b, |pb| {
            if let Some((area, c)) = poly_overlap(pa, pb)
                && best.is_none_or(|(ba, _)| area > ba)
            {
                best = Some((area, c));
            }
        });
    });
    best.map(|(_, c)| c)
}

struct Ctx<'a> {
    brushes: &'a [Brush],
    cancel: &'a AtomicBool,
    leaves: AtomicUsize,
}

impl Ctx<'_> {
    fn stop(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || self.leaves.load(Ordering::Relaxed) > MAX_LEAVES
    }

    fn leaf(&self, key: CellKey, kind: LeafKind, brushes: Vec<u32>) -> Leaf {
        self.leaves.fetch_add(1, Ordering::Relaxed);
        Leaf { key, kind, brushes }
    }

    fn overlapping(&self, cell: &Aabb, candidates: &[u32]) -> Option<Vec<u32>> {
        let mut out = Vec::new();
        for &i in candidates {
            let Some(b) = self.brushes.get(i as usize) else { continue };
            if brush_overlaps(b, cell) {
                if brush_contains(b, cell) {
                    return None;
                }
                out.push(i);
            }
        }
        Some(out)
    }
}

fn build(ctx: &Ctx<'_>, key: CellKey, candidates: &[u32], out: &mut Vec<Leaf>) {
    if ctx.stop() {
        return;
    }
    let Some(inside) = ctx.overlapping(&key.aabb(), candidates) else {
        out.push(ctx.leaf(key, LeafKind::Solid, Vec::new()));
        return;
    };
    if inside.is_empty() {
        out.push(ctx.leaf(key, LeafKind::Empty, Vec::new()));
        return;
    }
    if inside.len() <= LEAF_BRUSHES || key.size() <= MIN_CELL || key.level >= MAX_LEVEL {
        out.push(ctx.leaf(key, LeafKind::Pending, inside));
        return;
    }
    if inside.len() > 64 && key.level < 12 {
        let parts: Vec<Vec<Leaf>> = (0..8)
            .into_par_iter()
            .map(|i| {
                let mut v = Vec::new();
                build(ctx, key.child(i), &inside, &mut v);
                v
            })
            .collect();
        out.extend(parts.into_iter().flatten());
    } else {
        for i in 0..8 {
            build(ctx, key.child(i), &inside, out);
        }
    }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn group_components<P>(
    pieces: Vec<P>,
    links: Vec<(usize, usize, DVec3)>,
    sides_of: impl Fn(&P, &mut Vec<(u8, SidePoly)>),
    shape_of: impl Fn(&P) -> PieceShape,
) -> Vec<Component> {
    let n = pieces.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for &(a, b, _) in &links {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let mut comp_of: Vec<usize> = vec![usize::MAX; n];
    let mut slot: Vec<(usize, usize)> = Vec::with_capacity(n);
    let mut out: Vec<Component> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        let r = find(&mut parent, i);
        if comp_of[r] == usize::MAX {
            comp_of[r] = out.len();
            out.push(Component { sides: Vec::new(), pieces: Vec::new(), links: Vec::new() });
        }
        let c = comp_of[r];
        slot.push((c, out[c].pieces.len()));
        out[c].pieces.push(shape_of(p));
        sides_of(p, &mut out[c].sides);
    }
    for (a, b, pt) in links {
        let ((ca, ia), (cb, ib)) = (slot[a], slot[b]);
        if ca == cb {
            out[ca].links.push((ia as u16, ib as u16, pt));
        }
    }
    out
}

fn box_components(brushes: &[Brush], key: CellKey, list: &[u32]) -> Result<Vec<Component>, ()> {
    let cell = key.aabb();
    let mut boxes = vec![cell];
    for &i in list {
        let Some(b) = brushes.get(i as usize) else { continue };
        let mut next = Vec::with_capacity(boxes.len() + 4);
        for p in boxes {
            subtract_box(p, &b.aabb, &mut next);
        }
        boxes = next;
        if boxes.is_empty() {
            return Ok(Vec::new());
        }
        if boxes.len() > MAX_PIECES {
            return Err(());
        }
    }
    let n = boxes.len();
    let mut links: Vec<(usize, usize, DVec3)> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (&boxes[i], &boxes[j]);
            for k in 0..3 {
                let at = if (a.max[k] - b.min[k]).abs() < TOUCH {
                    a.max[k]
                } else if (b.max[k] - a.min[k]).abs() < TOUCH {
                    a.min[k]
                } else {
                    continue;
                };
                let (u, v) = ((k + 1) % 3, (k + 2) % 3);
                let lo = DVec2::new(a.min[u].max(b.min[u]), a.min[v].max(b.min[v]));
                let hi = DVec2::new(a.max[u].min(b.max[u]), a.max[v].min(b.max[v]));
                let size = hi - lo;
                if size.x > PORTAL_EDGE && size.y > PORTAL_EDGE && size.x * size.y > AREA_EPS {
                    let c = (lo + hi) * 0.5;
                    let mut p = DVec3::ZERO;
                    p[k] = at;
                    p[u] = c.x;
                    p[v] = c.y;
                    links.push((i, j, p));
                }
                break;
            }
        }
    }
    let sides_of = |b: &Aabb, out: &mut Vec<(u8, SidePoly)>| {
        for side in 0..6u8 {
            let k = (side / 2) as usize;
            let on = if side % 2 == 0 {
                (b.min[k] - cell.min[k]).abs() < TOUCH
            } else {
                (b.max[k] - cell.max[k]).abs() < TOUCH
            };
            if on {
                let (u, v) = side_axes(side);
                out.push((side, SidePoly::Rect(DVec2::new(b.min[u], b.min[v]), DVec2::new(b.max[u], b.max[v]))));
            }
        }
    };
    Ok(group_components(boxes, links, sides_of, |b| PieceShape::Box(*b)))
}

fn cell_components(brushes: &[Brush], key: CellKey, list: &[u32]) -> Result<Vec<Component>, ()> {
    if list.iter().all(|&i| brushes.get(i as usize).is_some_and(|b| b.axial)) {
        return box_components(brushes, key, list);
    }
    let (min, max) = (key.min(), key.max());
    let half = key.size() * 2.0;
    let mut pieces = vec![box_piece(min, max)];
    for &i in list {
        let Some(b) = brushes.get(i as usize) else { continue };
        let mut next = Vec::with_capacity(pieces.len() + 4);
        for p in pieces {
            subtract(p, b, half, &mut next);
        }
        pieces = next;
        if pieces.is_empty() {
            return Ok(Vec::new());
        }
        if pieces.len() > MAX_PIECES {
            return Err(());
        }
    }
    let mut links: Vec<(usize, usize, DVec3)> = Vec::new();
    if pieces.len() > 1 {
        let mut by_plane: HashMap<[u64; 4], Vec<(usize, bool, usize)>> = HashMap::new();
        for (pi, p) in pieces.iter().enumerate() {
            for (fi, f) in p.faces.iter().enumerate() {
                if f.side.is_some() {
                    continue;
                }
                let n = f.plane.normal;
                let positive = n.x > 0.0 || (n.x == 0.0 && (n.y > 0.0 || (n.y == 0.0 && n.z > 0.0)));
                let (cn, cd) = if positive { (n, f.plane.dist) } else { (-n, -f.plane.dist) };
                let k = [(cn.x + 0.0).to_bits(), (cn.y + 0.0).to_bits(), (cn.z + 0.0).to_bits(), (cd + 0.0).to_bits()];
                by_plane.entry(k).or_default().push((pi, positive, fi));
            }
        }
        for group in by_plane.values() {
            for (i, &(pa, sa, fa)) in group.iter().enumerate() {
                for &(pb, sb, fb) in &group[i + 1..] {
                    if pa == pb || sa == sb {
                        continue;
                    }
                    if let Some(c) = faces_overlap(&pieces[pa].faces[fa], &pieces[pb].faces[fb]) {
                        links.push((pa, pb, c));
                    }
                }
            }
        }
    }
    let sides_of = |p: &Piece, out: &mut Vec<(u8, SidePoly)>| {
        for f in &p.faces {
            if let Some(s) = f.side {
                let (u, v) = side_axes(s);
                let poly = ccw(f.pts.iter().map(|q| DVec2::new(q[u], q[v])).collect());
                if signed_area(&poly).abs() > AREA_EPS * 0.1 {
                    out.push((s, SidePoly::Poly(poly)));
                }
            }
        }
    };
    let shape_of = |p: &Piece| PieceShape::Planes {
        planes: p.faces.iter().map(|f| f.plane).collect(),
        center: p.center(),
        aabb: p.aabb,
    };
    Ok(group_components(pieces, links, sides_of, shape_of))
}

fn faces_overlap(a: &Face, b: &Face) -> Option<DVec3> {
    let n = a.plane.normal;
    let m = n.abs();
    let (i, j, k) = if m.x >= m.y && m.x >= m.z {
        (1, 2, 0)
    } else if m.y >= m.z {
        (0, 2, 1)
    } else {
        (0, 1, 2)
    };
    let proj = |pts: &[DVec3]| ccw(pts.iter().map(|p| DVec2::new(p[i], p[j])).collect());
    let (pa, pb) = (proj(&a.pts), proj(&b.pts));
    let o = clip_2d(&pa, &pb);
    if o.len() < 3 || signed_area(&o).abs() <= AREA_EPS || !wide_enough(&o) {
        return None;
    }
    let c = o.iter().copied().sum::<DVec2>() / o.len() as f64;
    let mut p = DVec3::ZERO;
    p[i] = c.x;
    p[j] = c.y;
    p[k] = (a.plane.dist - n[i] * c.x - n[j] * c.y) / n[k];
    p.is_finite().then_some(p)
}

fn resolve_leaf(ctx: &Ctx<'_>, leaf: Leaf, out: &mut Vec<Leaf>) {
    if ctx.stop() {
        return;
    }
    if !matches!(leaf.kind, LeafKind::Pending) {
        out.push(leaf);
        return;
    }
    match cell_components(ctx.brushes, leaf.key, &leaf.brushes) {
        Ok(comps) => {
            let kind = if comps.is_empty() { LeafKind::Solid } else { LeafKind::Mixed(comps) };
            out.push(Leaf { key: leaf.key, kind, brushes: leaf.brushes });
        }
        Err(()) if leaf.key.level < DEEPEST => {
            for i in 0..8 {
                let key = leaf.key.child(i);
                let sub = match ctx.overlapping(&key.aabb(), &leaf.brushes) {
                    None => ctx.leaf(key, LeafKind::Solid, Vec::new()),
                    Some(list) if list.is_empty() => ctx.leaf(key, LeafKind::Empty, Vec::new()),
                    Some(list) => ctx.leaf(key, LeafKind::Pending, list),
                };
                resolve_leaf(ctx, sub, out);
            }
        }
        Err(()) => {
            out.push(Leaf { key: leaf.key, kind: LeafKind::Empty, brushes: leaf.brushes });
        }
    }
}

struct Index {
    leaves: Vec<Leaf>,
    by_key: HashMap<CellKey, u32, IdBuildHasher>,
    inner: HashSet<CellKey, IdBuildHasher>,
    first_node: Vec<u32>,
    node_leaf: Vec<u32>,
}

impl Index {
    fn new(leaves: Vec<Leaf>) -> Self {
        let mut by_key: HashMap<CellKey, u32, IdBuildHasher> =
            HashMap::with_capacity_and_hasher(leaves.len(), IdBuildHasher::default());
        let mut inner: HashSet<CellKey, IdBuildHasher> = HashSet::with_hasher(IdBuildHasher::default());
        let mut first_node = Vec::with_capacity(leaves.len());
        let mut node_leaf = Vec::new();
        for (i, l) in leaves.iter().enumerate() {
            by_key.insert(l.key, i as u32);
            let mut k = l.key;
            while k.level > 0 {
                k = k.parent();
                if !inner.insert(k) {
                    break;
                }
            }
            first_node.push(node_leaf.len() as u32);
            for _ in 0..l.component_count() {
                node_leaf.push(i as u32);
            }
        }
        Index { leaves, by_key, inner, first_node, node_leaf }
    }

    fn leaf_at(&self, p: DVec3) -> Option<u32> {
        let mut key = CellKey::containing(DEEPEST, p)?;
        loop {
            if let Some(&i) = self.by_key.get(&key) {
                return Some(i);
            }
            if key.level == 0 {
                return None;
            }
            key = key.parent();
        }
    }

    fn for_neighbours(&self, leaf: &Leaf, axis: usize, mut f: impl FnMut(u32)) {
        let key = leaf.key;
        let size = key.size();
        let min = key.min();
        let mut probe = min + DVec3::splat(size * 0.5);
        probe[axis] = min[axis] + size + size.min(1.0) * 1e-3;
        let Some(across) = CellKey::containing(key.level, probe) else { return };
        if !self.inner.contains(&across) {
            let mut k = across;
            loop {
                if let Some(&i) = self.by_key.get(&k) {
                    f(i);
                    return;
                }
                if k.level == 0 {
                    return;
                }
                k = k.parent();
            }
        }
        let mut stack = vec![across];
        while let Some(k) = stack.pop() {
            if k.level >= DEEPEST {
                continue;
            }
            for c in 0..8 {
                if (c >> axis) & 1 != 0 {
                    continue;
                }
                let child = k.child(c);
                match self.by_key.get(&child) {
                    Some(&i) => f(i),
                    None => {
                        if self.inner.contains(&child) {
                            stack.push(child);
                        }
                    }
                }
            }
        }
    }
}

pub fn find_leaks(input: &LeakInput, cancel: &AtomicBool) -> Option<LeakReport> {
    let usable: Vec<&SealBrush> = input
        .brushes
        .iter()
        .filter(|b| {
            b.planes.len() >= 4
                && !b.aabb.is_empty()
                && b.aabb.min.is_finite()
                && b.aabb.max.is_finite()
                && b.planes.iter().all(|p| p.normal.is_finite() && p.dist.is_finite())
        })
        .collect();
    if usable.is_empty() {
        return Some(LeakReport { no_world: true, ..LeakReport::default() });
    }
    let brushes: Vec<Brush> = usable.par_iter().map(|b| Brush::new(b)).collect();
    let ctx = Ctx { brushes: &brushes, cancel, leaves: AtomicUsize::new(0) };
    let all: Vec<u32> = (0..brushes.len() as u32).collect();
    let mut raw = Vec::new();
    build(&ctx, CellKey::ROOT, &all, &mut raw);
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    if ctx.leaves.load(Ordering::Relaxed) > MAX_LEAVES {
        return Some(LeakReport { incomplete: true, ..LeakReport::default() });
    }
    let leaves: Vec<Leaf> = raw
        .into_par_iter()
        .with_min_len(64)
        .flat_map_iter(|leaf| {
            let mut v = Vec::with_capacity(1);
            resolve_leaf(&ctx, leaf, &mut v);
            v
        })
        .collect();
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    if ctx.leaves.load(Ordering::Relaxed) > MAX_LEAVES {
        return Some(LeakReport { incomplete: true, ..LeakReport::default() });
    }
    let index = Index::new(leaves);
    let report = flood(&index, input, &usable, cancel);
    index.leaves.into_par_iter().with_min_len(1024).for_each(drop);
    report
}

fn flood(index: &Index, input: &LeakInput, usable: &[&SealBrush], cancel: &AtomicBool) -> Option<LeakReport> {
    let node_count = index.node_leaf.len();
    let edges: Vec<(u32, u32, DVec3)> = (0..index.leaves.len())
        .into_par_iter()
        .with_min_len(256)
        .flat_map_iter(|li| {
            let mut out = Vec::new();
            if cancel.load(Ordering::Relaxed) {
                return out;
            }
            let leaf = &index.leaves[li];
            let na = leaf.component_count();
            if na == 0 {
                return out;
            }
            for axis in 0..3 {
                let side_pos = (axis * 2 + 1) as u8;
                let side_neg = (axis * 2) as u8;
                if !(0..na).any(|a| leaf.has_side(a, side_pos)) {
                    continue;
                }
                let plane = leaf.key.max()[axis];
                let (i, j) = side_axes(side_pos);
                index.for_neighbours(leaf, axis, |nb| {
                    let other = &index.leaves[nb as usize];
                    for b in 0..other.component_count() {
                        for a in 0..na {
                            if let Some(c) = components_overlap(leaf, a, side_pos, other, b, side_neg) {
                                let mut p = DVec3::ZERO;
                                p[axis] = plane;
                                p[i] = c.x;
                                p[j] = c.y;
                                out.push((
                                    index.first_node[li] + a as u32,
                                    index.first_node[nb as usize] + b as u32,
                                    p,
                                ));
                            }
                        }
                    }
                });
            }
            out
        })
        .collect();
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    let mut start = vec![0usize; node_count + 1];
    for &(a, b, _) in &edges {
        start[a as usize + 1] += 1;
        start[b as usize + 1] += 1;
    }
    for i in 0..node_count {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut adj: Vec<(u32, u32)> = vec![(0, 0); start[node_count]];
    for (ei, &(a, b, _)) in edges.iter().enumerate() {
        adj[fill[a as usize]] = (b, ei as u32);
        fill[a as usize] += 1;
        adj[fill[b as usize]] = (a, ei as u32);
        fill[b as usize] += 1;
    }
    const NONE: u32 = u32::MAX;
    let mut parent: Vec<(u32, u32)> = vec![(NONE, NONE); node_count];
    let mut depth: Vec<u32> = vec![u32::MAX; node_count];
    let mut queue = VecDeque::new();
    for (li, leaf) in index.leaves.iter().enumerate() {
        let border = leaf.key.border_sides();
        if !border.contains(&true) {
            continue;
        }
        for c in 0..leaf.component_count() {
            let open = (0..6u8).any(|s| border[s as usize] && leaf.has_side(c, s));
            let n = index.first_node[li] as usize + c;
            if open && depth[n] == u32::MAX {
                depth[n] = 0;
                queue.push_back(n as u32);
            }
        }
    }
    while let Some(n) = queue.pop_front() {
        let d = depth[n as usize];
        for &(m, e) in &adj[start[n as usize]..start[n as usize + 1]] {
            if depth[m as usize] == u32::MAX {
                depth[m as usize] = d + 1;
                parent[m as usize] = (n, e);
                queue.push_back(m);
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return None;
    }

    let mut report = LeakReport { cells: index.leaves.len(), ..LeakReport::default() };
    let mut best: Option<(u32, usize, DVec3, u32)> = None;
    for (si, seed) in input.seeds.iter().enumerate() {
        if seed.origin == DVec3::ZERO || !seed.origin.is_finite() {
            continue;
        }
        let base = seed.origin + DVec3::Z;
        let nudges: &[DVec3] =
            if seed.class.eq_ignore_ascii_case("info_player_start") { &NUDGES } else { &[DVec3::ZERO] };
        let mut placed: Option<(DVec3, Option<u32>)> = None;
        for &d in nudges {
            let p = base + d;
            match index.leaf_at(p) {
                None => {
                    placed = Some((p, None));
                    break;
                }
                Some(li) => {
                    if let Some(c) = index.leaves[li as usize].component_at(p) {
                        placed = Some((p, Some(index.first_node[li as usize] + c as u32)));
                        break;
                    }
                }
            }
        }
        let Some((p, node)) = placed else {
            report.in_solid.push(seed.entity);
            continue;
        };
        let reached = match node {
            None => Some(0),
            Some(n) => Some(depth[n as usize]).filter(|d| *d != u32::MAX),
        };
        if let Some(d) = reached {
            if report.leaking.len() < MAX_REPORTED {
                report.leaking.push(seed.entity);
            }
            if best.is_none_or(|(bd, ..)| d < bd) {
                best = Some((d, si, p, node.unwrap_or(NONE)));
            }
        }
    }
    let bounds = usable.iter().fold(Aabb::EMPTY, |a, b| a.union(&b.aabb)).expand(64.0);
    if let Some((_, si, p, node)) = best {
        let mut pts = vec![p];
        if node == NONE {
            pts.push(p + (p - bounds.center()).normalize_or_zero() * 64.0);
        } else {
            let mut cur = node;
            let mut entry = p;
            for _ in 0..node_count {
                let li = index.node_leaf[cur as usize] as usize;
                let leaf = &index.leaves[li];
                let comp = cur as usize - index.first_node[li] as usize;
                let (par, e) = parent[cur as usize];
                if par == NONE {
                    void_route(leaf, comp, entry, &bounds, &mut pts);
                    break;
                }
                let portal = edges[e as usize].2;
                route_in(leaf, comp, entry, portal, &mut pts);
                pts.push(portal);
                if !bounds.contains_point(portal) {
                    break;
                }
                entry = portal;
                cur = par;
            }
        }
        let far = bounds.expand(192.0);
        if pts.len() >= 2 {
            let last = pts[pts.len() - 1];
            let prev = pts[pts.len() - 2];
            if far.contains_point(prev) && !far.contains_point(last) {
                let end = pts.len() - 1;
                pts[end] = exit_point(prev, last, &far);
            }
        }
        pts.dedup();
        pts.reverse();
        report.leak = Some(LeakPath { entity: input.seeds[si].entity, points: pts });
    } else {
        let mut hull: Vec<u32> = Vec::new();
        for (n, &d) in depth.iter().enumerate() {
            if d == u32::MAX {
                continue;
            }
            let li = index.node_leaf[n] as usize;
            let leaf = &index.leaves[li];
            let LeafKind::Mixed(comps) = &leaf.kind else { continue };
            let Some(comp) = comps.get(n - index.first_node[li] as usize) else { continue };
            for &b in &leaf.brushes {
                let Some(brush) = usable.get(b as usize) else { continue };
                let reach = brush.aabb.expand(0.05);
                if comp.pieces.iter().any(|p| reach.intersects(p.aabb())) {
                    hull.push(b);
                }
            }
        }
        hull.sort_unstable();
        hull.dedup();
        report.hull = hull.iter().filter_map(|&i| usable.get(i as usize).map(|b| b.id)).collect();
    }
    Some(report)
}

const NUDGES: [DVec3; 9] = [
    DVec3::new(0.0, 0.0, 0.0),
    DVec3::new(-16.0, -16.0, 0.0),
    DVec3::new(-16.0, 0.0, 0.0),
    DVec3::new(-16.0, 16.0, 0.0),
    DVec3::new(0.0, -16.0, 0.0),
    DVec3::new(0.0, 16.0, 0.0),
    DVec3::new(16.0, -16.0, 0.0),
    DVec3::new(16.0, 0.0, 0.0),
    DVec3::new(16.0, 16.0, 0.0),
];

fn piece_near(c: &Component, p: DVec3) -> usize {
    if let Some(i) = c.pieces.iter().position(|piece| piece.contains(p)) {
        return i;
    }
    let mut best = (f64::INFINITY, 0);
    for (i, piece) in c.pieces.iter().enumerate() {
        let d = piece.center().distance_squared(p);
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

fn walk_pieces(c: &Component, start: usize, done: impl Fn(usize) -> bool) -> Option<(Vec<DVec3>, usize)> {
    let n = c.pieces.len();
    let mut prev: Vec<Option<(usize, DVec3)>> = vec![None; n];
    let mut seen = vec![false; n];
    let mut queue = VecDeque::from([start]);
    seen[start] = true;
    while let Some(i) = queue.pop_front() {
        if done(i) {
            let mut pts = Vec::new();
            let mut k = i;
            while let Some((from, pt)) = prev[k] {
                pts.push(pt);
                k = from;
            }
            pts.reverse();
            return Some((pts, i));
        }
        for &(a, b, pt) in &c.links {
            let (a, b) = (a as usize, b as usize);
            let next = if a == i {
                b
            } else if b == i {
                a
            } else {
                continue;
            };
            if next < n && !seen[next] {
                seen[next] = true;
                prev[next] = Some((i, pt));
                queue.push_back(next);
            }
        }
    }
    None
}

fn route_in(leaf: &Leaf, comp: usize, from: DVec3, to: DVec3, out: &mut Vec<DVec3>) {
    let LeafKind::Mixed(comps) = &leaf.kind else { return };
    let Some(c) = comps.get(comp) else { return };
    if c.pieces.len() < 2 {
        return;
    }
    let (a, b) = (piece_near(c, from), piece_near(c, to));
    if a == b {
        return;
    }
    if let Some((pts, _)) = walk_pieces(c, a, |i| i == b) {
        out.extend(pts);
    }
}

fn void_route(leaf: &Leaf, comp: usize, from: DVec3, bounds: &Aabb, out: &mut Vec<DVec3>) {
    match &leaf.kind {
        LeafKind::Mixed(comps) => {
            let Some(c) = comps.get(comp) else { return };
            let start = piece_near(c, from);
            match walk_pieces(c, start, |i| !bounds.contains_point(c.pieces[i].center())) {
                Some((pts, end)) => {
                    out.extend(pts);
                    out.push(c.pieces[end].center());
                }
                None => {
                    let sum: DVec3 = c.pieces.iter().map(PieceShape::center).sum();
                    out.push(sum / c.pieces.len().max(1) as f64);
                }
            }
        }
        _ => out.push(leaf.key.aabb().center()),
    }
}

fn exit_point(a: DVec3, b: DVec3, bx: &Aabb) -> DVec3 {
    let d = b - a;
    let mut t = 1.0f64;
    for k in 0..3 {
        if d[k] > 1e-12 {
            t = t.min((bx.max[k] - a[k]) / d[k]);
        } else if d[k] < -1e-12 {
            t = t.min((bx.min[k] - a[k]) / d[k]);
        }
    }
    a + d * t.clamp(0.0, 1.0)
}

pub const SEALING_ENTITY_CLASSES: &[&str] = &["func_areaportal", "func_areaportalwindow"];

pub const NOT_FLOODED: &[&str] = &[
    "func_detail",
    "func_instance",
    "func_instance_parms",
    "func_instance_origin",
    "func_instance_io_proxy",
    "info_overlay",
    "info_overlay_transition",
    "info_overlay_accessor",
    "env_cubemap",
];

pub fn guess_material_seals(material: &str) -> bool {
    MaterialFacts::guess(material).seals()
}

pub fn sides_seal(mut facts: impl Iterator<Item = MaterialFacts>) -> bool {
    let Some(first) = facts.next() else { return false };
    first.seals() && facts.all(|f| !f.transparent())
}

pub fn seal_brush(id: Id, geom: &hp_geom::SolidGeom) -> SealBrush {
    SealBrush { id, planes: geom.faces.iter().map(|f| f.plane).collect(), aabb: geom.aabb }
}

pub fn leak_input_from(snap: &super::snapshot::MapSnapshot, facts: &mut dyn FnMut(u32) -> MaterialFacts) -> LeakInput {
    let mut sealing_entities: HashSet<Id> = HashSet::new();
    for r in &snap.entities {
        if SEALING_ENTITY_CLASSES.iter().any(|c| r.entity.classname.eq_ignore_ascii_case(c)) {
            sealing_entities.insert(r.entity.id);
        }
    }
    let mut brushes = Vec::new();
    for s in &snap.solids {
        if !s.geom.valid || !snap.is_visible(ObjRef::Solid(s.id)) {
            continue;
        }
        let sealing = match s.owner {
            Owner::World => !s.has_disp() && sides_seal(s.sides.iter().map(|x| facts(x.material))),
            Owner::Entity(e) => sealing_entities.contains(&e),
        };
        if sealing {
            brushes.push(seal_brush(s.id, &s.geom));
        }
    }
    let mut seeds = Vec::new();
    for r in &snap.entities {
        let e = &r.entity;
        if NOT_FLOODED.iter().any(|c| e.classname.eq_ignore_ascii_case(c)) || !snap.is_visible(ObjRef::Entity(e.id)) {
            continue;
        }
        let Some(origin) = e.origin() else { continue };
        seeds.push(LeakSeed { entity: e.id, class: e.classname.clone(), origin });
    }
    LeakInput { brushes, seeds }
}

pub fn apply_cordons(input: &mut LeakInput, boxes: &[Aabb]) {
    if boxes.is_empty() {
        return;
    }
    input.brushes.retain(|b| boxes.iter().any(|c| c.intersects(&b.aabb)));
    input.seeds.retain(|s| boxes.iter().any(|c| c.contains_point(s.origin)));
    for b in boxes {
        for wall in crate::compile_export::seal_boxes(b, crate::compile_export::CORDON_SEAL_THICKNESS) {
            input.brushes.push(SealBrush { id: 0, planes: box_planes(wall.min, wall.max).to_vec(), aabb: wall });
        }
    }
}

pub fn flood_map(snap: &super::snapshot::MapSnapshot, cordons: &[Aabb]) -> hp_vmf::Map {
    let mut world = (*snap.world).clone();
    world.solids.clear();
    world.groups.clear();
    world.hidden = false;
    let mut entities: Vec<hp_vmf::Entity> = Vec::new();
    let mut slot: HashMap<Id, usize> = HashMap::new();
    let mut point: Vec<bool> = Vec::new();
    for (k, r) in snap.entities.iter().enumerate() {
        let e = &r.entity;
        if !snap.is_visible(ObjRef::Entity(e.id)) {
            continue;
        }
        point.push(snap.entity_solids.get(k).is_none_or(|n| *n == 0));
        let mut copy = e.clone();
        copy.solids.clear();
        copy.groups.clear();
        copy.hidden = false;
        slot.insert(e.id, entities.len());
        entities.push(copy);
    }
    for s in &snap.solids {
        if !s.geom.valid || s.has_disp() || !snap.is_visible(ObjRef::Solid(s.id)) {
            continue;
        }
        if !cordons.is_empty() && !cordons.iter().any(|c| c.intersects(&s.geom.aabb)) {
            continue;
        }
        let sides = s
            .sides
            .iter()
            .map(|x| {
                let mut side = hp_vmf::Side {
                    id: x.id,
                    plane: x.points,
                    material: snap.material_display(x.material).to_owned(),
                    ..hp_vmf::Side::default()
                };
                if x.contents != 0 {
                    side.add_compile_contents(x.contents);
                }
                side
            })
            .collect();
        let solid = hp_vmf::Solid { id: s.id, sides, ..hp_vmf::Solid::default() };
        match s.owner {
            Owner::World => world.solids.push(solid),
            Owner::Entity(e) => {
                if let Some(&i) = slot.get(&e) {
                    entities[i].solids.push(solid);
                }
            }
        }
    }
    if !cordons.is_empty() {
        let mut k = 0;
        entities.retain(|e| {
            let keep = if point[k] {
                e.classname.eq_ignore_ascii_case("light_environment")
                    || e.origin().is_none_or(|o| cordons.iter().any(|c| c.contains_point(o)))
            } else {
                !e.solids.is_empty()
            };
            k += 1;
            keep
        });
        let mut next = snap.solids.iter().map(|s| s.id).max().unwrap_or(0).saturating_add(1);
        let mut side_id = snap.solids.iter().flat_map(|s| s.sides.iter().map(|x| x.id)).max().unwrap_or(0);
        for b in cordons {
            for wall in crate::compile_export::seal_boxes(b, crate::compile_export::CORDON_SEAL_THICKNESS) {
                let mut ids = || {
                    side_id = side_id.saturating_add(1);
                    side_id
                };
                let Ok(sides) = hp_geom::hull_solid(
                    &wall.corners(),
                    crate::compile_export::DEFAULT_SEAL_MATERIAL,
                    0.25,
                    16,
                    &mut ids,
                ) else {
                    continue;
                };
                world.solids.push(hp_vmf::Solid { id: next, sides, ..hp_vmf::Solid::default() });
                next = next.saturating_add(1);
            }
        }
    }
    hp_vmf::Map { world, entities, ..hp_vmf::Map::default() }
}

impl Document {
    pub fn leak_input(&self, facts: &dyn Fn(&str) -> MaterialFacts) -> LeakInput {
        let scene = &self.scene;
        let mut cache: HashMap<&str, MaterialFacts> = HashMap::new();
        let mut brushes = Vec::new();
        for n in scene.solids.values() {
            if !n.geom.valid || !self.is_visible(ObjRef::Solid(n.solid.id)) {
                continue;
            }
            let sealing = match n.owner {
                Owner::World => {
                    !n.has_disp()
                        && sides_seal(
                            n.solid
                                .sides
                                .iter()
                                .map(|s| *cache.entry(s.material.as_str()).or_insert_with(|| facts(&s.material))),
                        )
                }
                Owner::Entity(e) => scene.entities.get(&e).is_some_and(|en| {
                    SEALING_ENTITY_CLASSES.iter().any(|c| en.entity.classname.eq_ignore_ascii_case(c))
                }),
            };
            if sealing {
                brushes.push(seal_brush(n.solid.id, &n.geom));
            }
        }
        let mut seeds = Vec::new();
        for n in scene.entities.values() {
            let e = &n.entity;
            if NOT_FLOODED.iter().any(|c| e.classname.eq_ignore_ascii_case(c)) || !self.is_visible(ObjRef::Entity(e.id))
            {
                continue;
            }
            let Some(origin) = e.origin() else { continue };
            seeds.push(LeakSeed { entity: e.id, class: e.classname.clone(), origin });
        }
        LeakInput { brushes, seeds }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_brush(id: Id, min: DVec3, max: DVec3) -> SealBrush {
        SealBrush { id, planes: box_planes(min, max).to_vec(), aabb: Aabb::new(min, max) }
    }

    fn room(s: f64, skip: &[usize]) -> Vec<SealBrush> {
        let t = 16.0;
        let walls = [
            (DVec3::new(-s - t, -s - t, -s - t), DVec3::new(-s, s + t, s + t)),
            (DVec3::new(s, -s - t, -s - t), DVec3::new(s + t, s + t, s + t)),
            (DVec3::new(-s, -s - t, -s - t), DVec3::new(s, -s, s + t)),
            (DVec3::new(-s, s, -s - t), DVec3::new(s, s + t, s + t)),
            (DVec3::new(-s, -s, -s - t), DVec3::new(s, s, -s)),
            (DVec3::new(-s, -s, s), DVec3::new(s, s, s + t)),
        ];
        walls
            .iter()
            .enumerate()
            .filter(|(i, _)| !skip.contains(i))
            .map(|(i, (a, b))| cube_brush(i as Id + 10, *a, *b))
            .collect()
    }

    fn seed(id: Id, p: DVec3) -> LeakSeed {
        LeakSeed { entity: id, class: "info_target".into(), origin: p }
    }

    fn run(brushes: Vec<SealBrush>, seeds: Vec<LeakSeed>) -> LeakReport {
        let cancel = AtomicBool::new(false);
        find_leaks(&LeakInput { brushes, seeds }, &cancel).expect("not cancelled")
    }

    #[test]
    fn sealed_room_does_not_leak() {
        let r = run(room(256.0, &[]), vec![seed(1, DVec3::new(10.0, 20.0, 30.0))]);
        assert!(!r.leaks(), "{r:?}");
        assert!(r.leaking.is_empty());
        assert!(!r.incomplete && !r.no_world);
        assert_eq!(r.hull.len(), 6);
    }

    #[test]
    fn missing_wall_leaks_with_a_path_to_the_entity() {
        let r = run(room(256.0, &[1]), vec![seed(7, DVec3::new(10.0, 20.0, 30.0))]);
        let leak = r.leak.expect("leak");
        assert_eq!(leak.entity, 7);
        assert_eq!(r.leaking, vec![7]);
        assert_eq!(*leak.points.last().expect("points"), DVec3::new(10.0, 20.0, 31.0));
        assert!(leak.points.len() >= 2);
        let first = leak.points[0];
        assert!(first.x > 256.0, "{:?}", leak.points);
    }

    #[test]
    fn small_crack_leaks_but_a_sliver_does_not() {
        let mut base = room(256.0, &[1]);
        let t = 16.0;
        let s = 256.0;
        let gap = 0.5;
        base.push(cube_brush(100, DVec3::new(s, -s - t, -s - t), DVec3::new(s + t, 0.0, s + t)));
        base.push(cube_brush(101, DVec3::new(s, gap, -s - t), DVec3::new(s + t, s + t, s + t)));
        let r = run(base.clone(), vec![seed(1, DVec3::new(0.0, 100.0, 0.0))]);
        assert!(r.leaks(), "a half unit crack leaks");

        let mut tight = room(256.0, &[1]);
        tight.push(cube_brush(100, DVec3::new(s, -s - t, -s - t), DVec3::new(s + t, 0.0, s + t)));
        tight.push(cube_brush(101, DVec3::new(s, 0.005, -s - t), DVec3::new(s + t, s + t, s + t)));
        let r = run(tight, vec![seed(1, DVec3::new(0.0, 100.0, 0.0))]);
        assert!(!r.leaks(), "a crack under the plane tolerance of VBSP does not leak");

        let mut hairline = room(256.0, &[1]);
        hairline.push(cube_brush(100, DVec3::new(s, -s - t, -s - t), DVec3::new(s + t, 0.0, s + t)));
        hairline.push(cube_brush(101, DVec3::new(s, 0.06, -s - t), DVec3::new(s + t, s + t, s + t)));
        let r = run(hairline, vec![seed(1, DVec3::new(0.0, 100.0, 0.0))]);
        assert!(!r.leaks(), "a crack narrower than the smallest portal VBSP keeps does not leak");
    }

    #[test]
    fn a_diagonal_crack_between_two_wall_pieces_seals_under_the_compiler_tolerance() {
        let s = 256.0;
        let t = 16.0;
        let slab = Aabb::new(DVec3::new(s, -s - t, -s - t), DVec3::new(s + t, s + t, s + t));
        let n = DVec3::new(-1.0, 1.0, 0.0).normalize();
        let d = n.dot(DVec3::new(s, 0.0, 0.0));
        let piece = |id: Id, plane: Plane| {
            let mut planes = box_planes(slab.min, slab.max).to_vec();
            planes.push(plane);
            SealBrush { id, planes, aabb: slab }
        };
        let with_gap = |gap: f64| {
            let mut brushes = room(s, &[1]);
            brushes.push(piece(100, Plane { normal: n, dist: d }));
            brushes.push(piece(101, Plane { normal: -n, dist: -d - gap }));
            run(brushes, vec![seed(1, DVec3::new(0.0, 100.0, 0.0))])
        };
        assert!(with_gap(0.5).leaks(), "a half unit diagonal crack leaks");
        let r = with_gap(0.05);
        assert!(!r.leaks(), "a 0.05 unit diagonal crack is sealed for VBSP: {:?}", r.leak);
    }

    #[test]
    fn cordons_seal_the_box_and_drop_what_is_outside() {
        let mut input = LeakInput {
            brushes: room(256.0, &[1]),
            seeds: vec![seed(1, DVec3::new(0.0, 100.0, 0.0)), seed(2, DVec3::new(5000.0, 0.0, 0.0))],
        };
        let r = run(input.brushes.clone(), input.seeds[..1].to_vec());
        assert!(r.leaks(), "the open room leaks");
        let boxes = [Aabb::new(DVec3::splat(-200.0), DVec3::splat(200.0))];
        apply_cordons(&mut input, &boxes);
        assert_eq!(input.seeds.len(), 1, "the entity outside the cordon is not compiled");
        assert!(input.brushes.iter().filter(|b| b.id == 0).count() == 6, "six sealing walls");
        let r = run(input.brushes, input.seeds);
        assert!(!r.leaks(), "the cordon walls seal the box like the compile does");
        let mut none = LeakInput { brushes: room(256.0, &[]), seeds: vec![seed(1, DVec3::new(0.0, 100.0, 0.0))] };
        apply_cordons(&mut none, &[]);
        assert_eq!(none.brushes.len(), 6, "no cordon, no change");
    }

    #[test]
    fn flood_map_keeps_what_the_compiler_needs() {
        use crate::tests::box_sides;
        let mut doc = Document::new_empty();
        let wall =
            doc.create_solid(box_sides(DVec3::ZERO, DVec3::splat(64.0), "BRICK/WALL"), Owner::World).expect("wall");
        let hidden = doc
            .create_solid(box_sides(DVec3::splat(100.0), DVec3::splat(164.0), "BRICK/WALL"), Owner::World)
            .expect("hidden");
        doc.set_hidden(&[ObjRef::Solid(hidden)], true);
        let detail = doc
            .create_solid(box_sides(DVec3::splat(200.0), DVec3::splat(264.0), "BRICK/WALL"), Owner::World)
            .expect("detail");
        doc.tie_to_entity(&[ObjRef::Solid(detail)], "func_detail", &[]).expect("tie");
        let start = doc.create_point_entity("info_player_start", DVec3::splat(32.0), &[]).expect("start");
        let far = doc.create_point_entity("light", DVec3::splat(3000.0), &[]).expect("far");
        let snap = super::super::snapshot::MapSnapshot::capture(&doc);
        let map = flood_map(&snap, &[]);
        assert_eq!(map.world.solids.iter().map(|s| s.id).collect::<Vec<_>>(), vec![wall]);
        assert_eq!(map.world.solids[0].sides.len(), 6);
        assert_eq!(map.world.solids[0].sides[0].material, "BRICK/WALL");
        let detail_entity = map.entities.iter().find(|e| e.classname == "func_detail").expect("func_detail");
        assert_eq!(detail_entity.solids.iter().map(|s| s.id).collect::<Vec<_>>(), vec![detail]);
        assert!(map.entities.iter().any(|e| e.id == start));
        let boxed = flood_map(&snap, &[Aabb::new(DVec3::splat(-500.0), DVec3::splat(500.0))]);
        assert!(!boxed.entities.iter().any(|e| e.id == far), "outside the cordon");
        assert!(boxed.entities.iter().any(|e| e.id == start));
        assert_eq!(boxed.world.solids.len(), 7, "the wall and six cordon walls");
    }

    #[test]
    fn off_grid_seams_seal() {
        let mut b = room(256.0, &[1]);
        let t = 16.0;
        let s = 256.0;
        let cut = 37.31;
        b.push(cube_brush(100, DVec3::new(s, -s - t, -s - t), DVec3::new(s + t, s + t, cut)));
        let top_min = DVec3::new(s, -s - t, cut);
        let top_max = DVec3::new(s + t, s + t, s + t);
        let slant = Plane::new(DVec3::new(0.0, 0.6, 0.8), 0.6 * 13.7 + 0.8 * 100.0).expect("plane");
        let mut a = cube_brush(101, top_min, top_max);
        a.planes.push(slant);
        let mut c = cube_brush(102, top_min, top_max);
        c.planes.push(Plane { normal: -slant.normal, dist: -slant.dist });
        b.push(a);
        b.push(c);
        let r = run(b, vec![seed(1, DVec3::new(0.0, 0.0, 0.0) + DVec3::new(1.0, 1.0, 1.0))]);
        assert!(!r.leaks(), "{:?}", r.leak);
    }

    #[test]
    fn rotated_room_seals() {
        let mut b = Vec::new();
        let r_in = 300.0;
        let h = 128.0;
        for k in 0..8 {
            let a = std::f64::consts::TAU * f64::from(k) / 8.0;
            let n = DVec3::new(a.cos(), a.sin(), 0.0);
            let side = DVec3::new(-a.sin(), a.cos(), 0.0);
            let half_w = r_in * (std::f64::consts::PI / 8.0).tan() + 40.0;
            let planes = vec![
                Plane { normal: n, dist: r_in + 24.0 },
                Plane { normal: -n, dist: -r_in },
                Plane { normal: side, dist: half_w },
                Plane { normal: -side, dist: half_w },
                Plane { normal: DVec3::Z, dist: h + 16.0 },
                Plane { normal: DVec3::NEG_Z, dist: h + 16.0 },
            ];
            let geom = hp_geom::build_solid_from_planes(&planes);
            assert!(geom.valid);
            b.push(SealBrush { id: 100 + k, planes, aabb: geom.aabb });
        }
        b.push(cube_brush(1, DVec3::new(-400.0, -400.0, h), DVec3::new(400.0, 400.0, h + 16.0)));
        b.push(cube_brush(2, DVec3::new(-400.0, -400.0, -h - 16.0), DVec3::new(400.0, 400.0, -h)));
        let r = run(b.clone(), vec![seed(1, DVec3::new(5.0, 5.0, 0.0))]);
        assert!(!r.leaks(), "{:?}", r.leak);
        b.remove(3);
        let r = run(b, vec![seed(1, DVec3::new(5.0, 5.0, 0.0))]);
        assert!(r.leaks());
    }

    #[test]
    fn entities_in_solid_outside_and_at_the_origin() {
        let r = run(
            room(256.0, &[]),
            vec![
                seed(1, DVec3::new(0.0, 0.0, 0.0)),
                seed(2, DVec3::new(-264.0, 0.0, 0.0)),
                seed(3, DVec3::new(5000.0, 0.0, 0.0)),
                seed(4, DVec3::new(50000.0, 0.0, 0.0)),
                seed(5, DVec3::new(10.0, 10.0, 10.0)),
            ],
        );
        assert_eq!(r.in_solid, vec![2]);
        assert_eq!(r.leaking, vec![3, 4]);
        let leak = r.leak.expect("leak");
        assert!(leak.entity == 3 || leak.entity == 4);
    }

    #[test]
    fn player_start_is_nudged_out_of_a_brush() {
        let mut b = room(256.0, &[]);
        b.push(cube_brush(50, DVec3::new(-8.0, -8.0, -256.0), DVec3::new(8.0, 8.0, 256.0)));
        let start =
            LeakSeed { entity: 9, class: "info_player_start".into(), origin: DVec3::new(0.0, 0.0, 0.0) + DVec3::X };
        let r = run(b.clone(), vec![start.clone()]);
        assert!(r.in_solid.is_empty(), "nudged out of the pillar");
        let other = LeakSeed { class: "info_target".into(), ..start };
        let r = run(b, vec![other]);
        assert_eq!(r.in_solid, vec![9]);
    }

    #[test]
    fn many_brushes_split_cells_and_stay_correct() {
        let mut b = room(256.0, &[4]);
        let mut id = 1000;
        let mut y = -256.0;
        while y < 256.0 {
            let mut x = -256.0;
            while x < 256.0 {
                b.push(cube_brush(id, DVec3::new(x, y, -272.0), DVec3::new(x + 32.0, y + 32.0, -256.0)));
                id += 1;
                x += 32.0;
            }
            y += 32.0;
        }
        let r = run(b.clone(), vec![seed(1, DVec3::new(3.0, 3.0, 0.0))]);
        assert!(!r.leaks(), "{:?}", r.leak);
        b.retain(|br| br.id != 1000 + 70);
        let r = run(b, vec![seed(1, DVec3::new(3.0, 3.0, 0.0))]);
        assert!(r.leaks());
    }

    #[test]
    fn cancelled_and_empty() {
        let cancel = AtomicBool::new(true);
        assert!(find_leaks(&LeakInput { brushes: room(64.0, &[]), seeds: vec![] }, &cancel).is_none());
        let r = run(Vec::new(), vec![seed(1, DVec3::ONE)]);
        assert!(r.no_world && !r.leaks());
    }

    #[test]
    fn material_guesses() {
        assert!(guess_material_seals("TOOLS/TOOLSNODRAW"));
        assert!(guess_material_seals("tools/toolsskybox"));
        assert!(!guess_material_seals("TOOLS/TOOLSCLIP"));
        assert!(!guess_material_seals("tools/toolshint"));
        assert!(!guess_material_seals("liquids/water_river"));
        assert!(guess_material_seals("concrete/concretefloor001a"));
        let f = MaterialFacts::guess;
        assert!(sides_seal([f("brick/wall"), f("tools/toolsnodraw")].into_iter()));
        assert!(!sides_seal([f("brick/wall"), f("liquids/water")].into_iter()));
        assert!(sides_seal([f("tools/toolsnodraw"), f("tools/toolsclip")].into_iter()), "first side wins");
        assert!(!sides_seal([f("tools/toolsclip"), f("tools/toolsnodraw")].into_iter()));
        let glass = MaterialFacts { translucent: true, ..MaterialFacts::default() };
        assert!(!sides_seal([f("brick/wall"), glass].into_iter()));
        assert!(!sides_seal(std::iter::empty()));
    }

    #[test]
    fn snapshot_input_matches_document_input() {
        use crate::tests::box_sides;
        let mut doc = Document::new_empty();
        let a =
            doc.create_solid(box_sides(DVec3::ZERO, DVec3::splat(64.0), "BRICK/WALL"), Owner::World).expect("solid");
        let ap = doc
            .create_solid(box_sides(DVec3::splat(100.0), DVec3::splat(164.0), "TOOLS/TOOLSAREAPORTAL"), Owner::World)
            .expect("portal");
        doc.tie_to_entity(&[ObjRef::Solid(ap)], "func_areaportal", &[]).expect("tie");
        doc.create_point_entity("info_target", DVec3::splat(32.0), &[]).expect("entity");
        let snap = super::super::snapshot::MapSnapshot::capture(&doc);
        let from_snap = leak_input_from(&snap, &mut |m| MaterialFacts::guess(snap.material_key(m)));
        let from_doc = doc.leak_input(&MaterialFacts::guess);
        let ids = |i: &LeakInput| i.brushes.iter().map(|b| b.id).collect::<Vec<_>>();
        assert_eq!(ids(&from_snap), vec![a, ap]);
        assert_eq!(ids(&from_snap), ids(&from_doc));
        assert_eq!(from_snap.seeds, from_doc.seeds);
    }

    #[test]
    fn document_input_skips_detail_and_hidden() {
        use crate::tests::box_sides;
        let mut doc = Document::new_empty();
        let a =
            doc.create_solid(box_sides(DVec3::ZERO, DVec3::splat(64.0), "BRICK/WALL"), Owner::World).expect("solid");
        let clip = doc
            .create_solid(box_sides(DVec3::splat(100.0), DVec3::splat(164.0), "TOOLS/TOOLSCLIP"), Owner::World)
            .expect("clip");
        let det = doc
            .create_solid(box_sides(DVec3::splat(200.0), DVec3::splat(264.0), "BRICK/WALL"), Owner::World)
            .expect("detail");
        doc.tie_to_entity(&[ObjRef::Solid(det)], "func_detail", &[]).expect("tie");
        let hidden = doc
            .create_solid(box_sides(DVec3::splat(300.0), DVec3::splat(364.0), "BRICK/WALL"), Owner::World)
            .expect("hidden");
        doc.set_hidden(&[ObjRef::Solid(hidden)], true);
        doc.create_point_entity("info_target", DVec3::splat(32.0), &[]).expect("entity");
        doc.create_point_entity("env_cubemap", DVec3::splat(40.0), &[]).expect("cubemap");
        let input = doc.leak_input(&MaterialFacts::guess);
        let ids: Vec<Id> = input.brushes.iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![a]);
        assert!(!ids.contains(&clip));
        assert_eq!(input.seeds.len(), 1);
        assert_eq!(input.seeds[0].class, "info_target");
    }

    #[test]
    #[ignore = "benchmark"]
    fn bench_large_map() {
        let mut b = Vec::new();
        let mut id = 0;
        let mut seeds = Vec::new();
        for gy in 0..48 {
            for gx in 0..48 {
                let o = DVec3::new(f64::from(gx) * 320.0 - 7680.0, f64::from(gy) * 320.0 - 7680.0, 0.0);
                for r in room(128.0, &[]) {
                    id += 1;
                    b.push(SealBrush {
                        id,
                        planes: r
                            .planes
                            .iter()
                            .map(|p| Plane { normal: p.normal, dist: p.dist + p.normal.dot(o) })
                            .collect(),
                        aabb: Aabb::new(r.aabb.min + o, r.aabb.max + o),
                    });
                }
                seeds.push(seed(id, o + DVec3::new(3.0, 3.0, 3.0)));
            }
        }
        eprintln!("{} brushes", b.len());
        for _ in 0..2 {
            let t0 = std::time::Instant::now();
            let r = run(b.clone(), seeds.clone());
            eprintln!("{} cells, leaks {}, {:?}", r.cells, r.leaks(), t0.elapsed());
        }
    }
}

#[cfg(test)]
mod real_maps {
    use super::*;

    #[test]
    fn real_maps_when_available() {
        let maps = [
            (
                r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples\ttt_traps.vmf",
                false,
            ),
            (
                r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples\ttt_traitorcheck.vmf",
                false,
            ),
            (
                r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples\ttt_weaponcheck.vmf",
                false,
            ),
            (concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/e2e/assets/leak_room.vmf"), true),
        ];
        for (path, leaks) in maps {
            let Ok(doc) = Document::open(std::path::Path::new(path)) else {
                eprintln!("{path}: not found, skipped");
                continue;
            };
            let input = doc.leak_input(&MaterialFacts::guess);
            let t0 = std::time::Instant::now();
            let r = find_leaks(&input, &AtomicBool::new(false)).expect("not cancelled");
            eprintln!(
                "{path}: {} brushes, {} cells, leak {:?} ({} leaking), in solid {:?}, {:?}",
                input.brushes.len(),
                r.cells,
                r.leak.as_ref().map(|l| (l.entity, l.points.len())),
                r.leaking.len(),
                r.in_solid,
                t0.elapsed()
            );
            assert_eq!(r.leaks(), leaks, "{path}");
        }
    }
}
