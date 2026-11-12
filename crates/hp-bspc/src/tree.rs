use glam::DVec3;

use crate::brush::MapBrush;
use crate::materials::{contents, surf};
use crate::planes::{MapPlane, PlaneSet};
use crate::winding::Winding;

pub const SIDESPACE: f64 = 8.0;
const PLANESIDE_EPSILON: f64 = 0.001;

pub const PSIDE_FRONT: u8 = 1;
pub const PSIDE_BACK: u8 = 2;
pub const PSIDE_BOTH: u8 = 3;
pub const PSIDE_FACING: u8 = 4;

#[derive(Clone, Debug)]
pub struct FragSide {
    pub plane: usize,
    pub winding: Option<Winding>,
    pub on_node: bool,
    pub visible: bool,
    pub bevel: bool,
    pub no_split: bool,
    pub surf: u32,
    pub tested: bool,
}

#[derive(Clone, Debug)]
pub struct Frag {
    pub original: usize,
    pub sides: Vec<FragSide>,
    pub mins: DVec3,
    pub maxs: DVec3,
    pub test_side: u8,
    pub side: u8,
}

impl Frag {
    pub fn from_brush(index: usize, b: &MapBrush) -> Frag {
        let sides = b
            .sides
            .iter()
            .map(|s| FragSide {
                plane: s.plane,
                winding: s.winding.clone(),
                on_node: false,
                visible: s.visible || s.surf & surf::HINT != 0,
                bevel: s.bevel,
                no_split: s.no_split,
                surf: s.surf,
                tested: false,
            })
            .collect();
        let mut f = Frag { original: index, sides, mins: b.mins, maxs: b.maxs, test_side: 0, side: 0 };
        f.bound();
        f
    }

    pub fn from_bounds(planes: &mut PlaneSet, mins: DVec3, maxs: DVec3) -> Frag {
        let mut sides = Vec::with_capacity(6);
        for axis in 0..3 {
            for (dir, d) in [(1.0, maxs[axis]), (-1.0, -mins[axis])] {
                let mut n = DVec3::ZERO;
                n[axis] = dir;
                let Some(p) = planes.find(n, d) else { continue };
                sides.push(FragSide {
                    plane: p,
                    winding: None,
                    on_node: false,
                    visible: false,
                    bevel: false,
                    no_split: false,
                    surf: 0,
                    tested: false,
                });
            }
        }
        let mut f = Frag { original: usize::MAX, sides, mins, maxs, test_side: 0, side: 0 };
        f.make_windings(planes);
        f
    }

    pub fn make_windings(&mut self, planes: &PlaneSet) {
        let list: Vec<usize> = self.sides.iter().map(|s| s.plane).collect();
        for s in &mut self.sides {
            let p = planes.get(s.plane);
            let mut w = Some(Winding::base_for_plane(p.normal, p.dist));
            for &o in &list {
                let Some(cur) = w.take() else { break };
                if o == s.plane {
                    w = Some(cur);
                    continue;
                }
                w = cur.clip_back(planes.get(o), 0.0, false);
            }
            s.winding = w;
        }
        self.bound();
    }

    pub fn bound(&mut self) {
        let mut lo = DVec3::splat(f64::MAX);
        let mut hi = DVec3::splat(f64::MIN);
        for w in self.sides.iter().filter_map(|s| s.winding.as_ref()) {
            for p in &w.points {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
        self.mins = lo;
        self.maxs = hi;
    }

    pub fn volume(&self) -> f64 {
        let Some(corner) = self.sides.iter().find_map(|s| s.winding.as_ref()).and_then(|w| w.points.first().copied())
        else {
            return 0.0;
        };
        let mut v = 0.0;
        for w in self.sides.iter().filter_map(|s| s.winding.as_ref()) {
            let d = (w.points[0] - corner).dot(w.normal());
            v += d * w.area();
        }
        v / 3.0
    }
}

pub fn box_on_plane_side(mins: DVec3, maxs: DVec3, p: &MapPlane) -> u8 {
    if p.kind < 3 {
        let t = p.kind as usize;
        let mut side = 0;
        if maxs[t] > p.dist + PLANESIDE_EPSILON {
            side |= PSIDE_FRONT;
        }
        if mins[t] < p.dist - PLANESIDE_EPSILON {
            side |= PSIDE_BACK;
        }
        return side;
    }
    let mut c0 = DVec3::ZERO;
    let mut c1 = DVec3::ZERO;
    for i in 0..3 {
        if p.normal[i] < 0.0 {
            c0[i] = mins[i];
            c1[i] = maxs[i];
        } else {
            c1[i] = mins[i];
            c0[i] = maxs[i];
        }
    }
    let d1 = p.distance(c0);
    let d2 = p.distance(c1);
    let mut side = 0;
    if d1 >= PLANESIDE_EPSILON {
        side = PSIDE_FRONT;
    }
    if d2 < PLANESIDE_EPSILON {
        side |= PSIDE_BACK;
    }
    side
}

pub fn split_frag(b: &Frag, plane_idx: usize, planes: &PlaneSet) -> (Option<Frag>, Option<Frag>) {
    let plane = *planes.get(plane_idx);
    let (mut d_front, mut d_back) = (0.0f64, 0.0f64);
    for w in b.sides.iter().filter_map(|s| s.winding.as_ref()) {
        for p in &w.points {
            let d = plane.distance(*p);
            d_front = d_front.max(d);
            d_back = d_back.min(d);
        }
    }
    if d_front < 0.1 {
        return (None, Some(b.clone()));
    }
    if d_back > -0.1 {
        return (Some(b.clone()), None);
    }
    let mut w = Some(Winding::base_for_plane(plane.normal, plane.dist));
    for s in &b.sides {
        let Some(cur) = w.take() else { break };
        w = cur.clip_back(planes.get(s.plane), 0.0, false);
    }
    let mid = match w {
        Some(w) if w.points.len() >= 3 && w.area() > 1e-6 => w,
        _ => {
            let mut best = 0.0;
            let mut side_front = true;
            for w in b.sides.iter().filter_map(|s| s.winding.as_ref()) {
                for p in &w.points {
                    let d = plane.distance(*p);
                    if d.abs() > best {
                        best = d.abs();
                        side_front = d > 0.0;
                    }
                }
            }
            return if side_front { (Some(b.clone()), None) } else { (None, Some(b.clone())) };
        }
    };
    let mut out: [Frag; 2] = [
        Frag { original: b.original, sides: Vec::new(), mins: b.mins, maxs: b.maxs, test_side: 0, side: 0 },
        Frag { original: b.original, sides: Vec::new(), mins: b.mins, maxs: b.maxs, test_side: 0, side: 0 },
    ];
    for s in &b.sides {
        let Some(w) = &s.winding else { continue };
        let (f, k) = w.split(&plane, 0.0);
        for (j, part) in [f, k].into_iter().enumerate() {
            if let Some(part) = part {
                let mut cs = s.clone();
                cs.winding = Some(part);
                cs.tested = false;
                out[j].sides.push(cs);
            }
        }
    }
    let mut keep = [true, true];
    for (i, f) in out.iter_mut().enumerate() {
        f.bound();
        if f.sides.len() < 3
            || f.mins.abs().max_element() > crate::winding::MAX_WORLD
            || f.maxs.abs().max_element() > crate::winding::MAX_WORLD
        {
            keep[i] = false;
        }
    }
    for (i, f) in out.iter_mut().enumerate() {
        if !keep[i] {
            continue;
        }
        f.sides.push(FragSide {
            plane: plane_idx ^ i ^ 1,
            winding: Some(if i == 0 { mid.reversed() } else { mid.clone() }),
            on_node: true,
            visible: false,
            bevel: false,
            no_split: false,
            surf: 0,
            tested: false,
        });
        if f.volume() < 0.001 {
            keep[i] = false;
        }
    }
    let [a, c] = out;
    (keep[0].then_some(a), keep[1].then_some(c))
}

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub plane: Option<usize>,
    pub children: [usize; 2],
    pub parent: Option<usize>,
    pub volume: Option<Frag>,
    pub mins: DVec3,
    pub maxs: DVec3,
    pub contents: u32,
    pub detail_separator: bool,
    pub brushes: Vec<Frag>,
    pub occupied: u32,
    pub occupant: Option<usize>,
    pub portals: Vec<usize>,
    pub faces: Vec<usize>,
    pub detail_faces: Vec<usize>,
    pub extra_brushes: Vec<usize>,
    pub water: Option<u16>,
    pub cluster: i32,
    pub area: i32,
}

impl Node {
    pub fn is_leaf(&self) -> bool {
        self.plane.is_none()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub head: usize,
    pub outside: usize,
    pub mins: DVec3,
    pub maxs: DVec3,
}

fn test_brush_to_plane(b: &Frag, pnum: usize, planes: &PlaneSet) -> (u8, u32, bool, bool) {
    for s in &b.sides {
        if s.plane == pnum {
            return (PSIDE_BACK | PSIDE_FACING, 0, false, false);
        }
        if s.plane == (pnum ^ 1) {
            return (PSIDE_FRONT | PSIDE_FACING, 0, false, false);
        }
    }
    let p = planes.get(pnum);
    let side = box_on_plane_side(b.mins, b.maxs, p);
    if side != PSIDE_BOTH {
        return (side, 0, false, false);
    }
    let (mut splits, mut hint) = (0u32, false);
    let (mut d_front, mut d_back) = (0.0f64, 0.0f64);
    for s in &b.sides {
        if s.on_node || !s.visible {
            continue;
        }
        let Some(w) = &s.winding else { continue };
        let (mut front, mut back) = (false, false);
        for q in &w.points {
            let d = p.distance(*q);
            d_front = d_front.max(d);
            d_back = d_back.min(d);
            if d > 0.1 {
                front = true;
            }
            if d < -0.1 {
                back = true;
            }
        }
        if front && back && s.surf & surf::SKIP == 0 {
            splits += 1;
            if s.surf & surf::HINT != 0 {
                hint = true;
            }
        }
    }
    let epsilon = (d_front > 0.0 && d_front < 1.0) || (d_back < 0.0 && d_back > -1.0);
    (side, splits, hint, epsilon)
}

pub struct Builder<'a> {
    pub planes: &'a mut PlaneSet,
    pub brush_contents: &'a [u32],
    nodes: Vec<Node>,
    blocks: bool,
}

pub const BLOCK_SIZE: f64 = 1024.0;

pub fn block_range(mins: DVec3, maxs: DVec3) -> (i32, i32, i32, i32) {
    block_range_sized(mins, maxs, BLOCK_SIZE)
}

pub fn block_range_sized(mins: DVec3, maxs: DVec3, size: f64) -> (i32, i32, i32, i32) {
    let (lowest, highest) = ((-16384.0 / size).floor() as i32, (16384.0 / size).ceil() as i32 - 1);
    let (mut xl, mut yl, mut xh, mut yh) = (lowest, lowest, highest, highest);
    if f64::from(xh) * size > maxs.x {
        xh = (maxs.x / size).floor() as i32;
    }
    if f64::from(xl + 1) * size < mins.x {
        xl = (mins.x / size).floor() as i32;
    }
    if f64::from(yh) * size > maxs.y {
        yh = (maxs.y / size).floor() as i32;
    }
    if f64::from(yl + 1) * size < mins.y {
        yl = (mins.y / size).floor() as i32;
    }
    (xl.max(lowest), yl.max(lowest), xh.min(highest), yh.min(highest))
}

impl<'a> Builder<'a> {
    pub fn new(planes: &'a mut PlaneSet, brush_contents: &'a [u32]) -> Self {
        Self { planes, brush_contents, nodes: Vec::new(), blocks: false }
    }

    pub fn with_blocks(mut self) -> Self {
        self.blocks = true;
        self
    }

    fn block_split(
        &mut self,
        node: usize,
        (mut xl, mut yl, mut xh, mut yh): (i32, i32, i32, i32),
        range: (i32, i32, i32, i32),
        brushes: Vec<Frag>,
        stack: &mut Vec<(usize, Vec<Frag>)>,
    ) {
        loop {
            if xl == xh && yl == yh {
                let inside = (range.0..=range.2).contains(&xl) && (range.1..=range.3).contains(&yl);
                if brushes.is_empty() && inside {
                    let n = &mut self.nodes[node];
                    n.plane = None;
                    n.contents = contents::SOLID;
                } else {
                    stack.push((node, brushes));
                }
                return;
            }
            let x_axis = xh - xl > yh - yl;
            let mid = if x_axis { xl + (xh - xl) / 2 + 1 } else { yl + (yh - yl) / 2 + 1 };
            let normal = if x_axis { DVec3::X } else { DVec3::Y };
            let Some(found) = self.planes.find(normal, f64::from(mid) * BLOCK_SIZE) else {
                stack.push((node, brushes));
                return;
            };
            let pnum = found & !1;
            let positive = self.planes.get(pnum).normal.dot(normal) > 0.0;
            let high = |xl: i32, yl: i32, xh: i32, yh: i32| if x_axis { (mid, yl, xh, yh) } else { (xl, mid, xh, yh) };
            let low =
                |xl: i32, yl: i32, xh: i32, yh: i32| if x_axis { (xl, yl, mid - 1, yh) } else { (xl, yl, xh, mid - 1) };
            let (front_range, back_range) = if positive {
                (high(xl, yl, xh, yh), low(xl, yl, xh, yh))
            } else {
                (low(xl, yl, xh, yh), high(xl, yl, xh, yh))
            };
            if !self.plane_cuts_volume(node, pnum) {
                let in_front =
                    self.nodes[node].volume.as_ref().is_some_and(|v| split_frag(v, pnum, self.planes).0.is_some());
                (xl, yl, xh, yh) = if in_front { front_range } else { back_range };
                continue;
            }
            self.nodes[node].plane = Some(pnum);
            let (mut front, mut back) = (Vec::new(), Vec::new());
            for b in brushes {
                let (f, k) = split_frag(&b, pnum, self.planes);
                for (part, list) in [(f, &mut front), (k, &mut back)] {
                    if let Some(mut part) = part {
                        for side in &mut part.sides {
                            if side.plane & !1 == pnum {
                                side.on_node = true;
                            }
                        }
                        list.push(part);
                    }
                }
            }
            let volume = self.nodes[node].volume.take();
            let (vf, vb) = match &volume {
                Some(v) => split_frag(v, pnum, self.planes),
                None => (None, None),
            };
            let f = self.nodes.len();
            self.nodes.push(Node { parent: Some(node), volume: vf, cluster: -1, ..Node::default() });
            let k = self.nodes.len();
            self.nodes.push(Node { parent: Some(node), volume: vb, cluster: -1, ..Node::default() });
            self.nodes[node].children = [f, k];
            self.nodes[node].volume = volume;
            self.block_split(k, back_range, range, back, stack);
            self.block_split(f, front_range, range, front, stack);
            return;
        }
    }

    fn detail(&self, f: &Frag) -> bool {
        self.brush_contents.get(f.original).is_some_and(|c| c & contents::DETAIL != 0)
    }

    pub fn build(mut self, brushes: Vec<Frag>, mins: DVec3, maxs: DVec3) -> Tree {
        let (mins, maxs) =
            if mins.cmple(maxs).all() { (mins, maxs) } else { (DVec3::splat(-64.0), DVec3::splat(64.0)) };
        let volume = Frag::from_bounds(self.planes, mins - SIDESPACE, maxs + SIDESPACE);
        self.nodes.push(Node { volume: Some(volume), cluster: -1, ..Node::default() });
        let mut stack: Vec<(usize, Vec<Frag>)> = Vec::new();
        if self.blocks {
            let r = block_range(mins, maxs);
            self.block_split(0, (r.0 - 1, r.1 - 1, r.2 + 1, r.3 + 1), r, brushes, &mut stack);
        } else {
            stack.push((0, brushes));
        }
        while let Some((node, list)) = stack.pop() {
            self.build_node(node, list, &mut stack);
        }
        let outside = self.nodes.len();
        self.nodes.push(Node { cluster: -1, ..Node::default() });
        Tree { nodes: self.nodes, head: 0, outside, mins, maxs }
    }

    fn build_node(&mut self, node: usize, mut brushes: Vec<Frag>, stack: &mut Vec<(usize, Vec<Frag>)>) {
        let Some((pnum, detail)) = self.select_split(node, &mut brushes) else {
            self.leaf(node, brushes);
            return;
        };
        let pnum = pnum & !1;
        self.nodes[node].plane = Some(pnum);
        self.nodes[node].detail_separator = detail;
        let (mut front, mut back) = (Vec::new(), Vec::new());
        for b in brushes {
            let s = b.side;
            if s == PSIDE_BOTH {
                let (f, k) = split_frag(&b, pnum, self.planes);
                if let Some(f) = f {
                    front.push(f);
                }
                if let Some(k) = k {
                    back.push(k);
                }
                continue;
            }
            let mut nb = b;
            if s & PSIDE_FACING != 0 {
                for side in &mut nb.sides {
                    if side.plane & !1 == pnum {
                        side.on_node = true;
                    }
                }
            }
            if s & PSIDE_FRONT != 0 {
                front.push(nb);
            } else if s & PSIDE_BACK != 0 {
                back.push(nb);
            }
        }
        let volume = self.nodes[node].volume.take();
        let (vf, vb) = match &volume {
            Some(v) => split_frag(v, pnum, self.planes),
            None => (None, None),
        };
        let f = self.nodes.len();
        self.nodes.push(Node { parent: Some(node), volume: vf, cluster: -1, ..Node::default() });
        let k = self.nodes.len();
        self.nodes.push(Node { parent: Some(node), volume: vb, cluster: -1, ..Node::default() });
        self.nodes[node].children = [f, k];
        self.nodes[node].volume = volume;
        stack.push((k, back));
        stack.push((f, front));
    }

    fn leaf(&mut self, node: usize, brushes: Vec<Frag>) {
        let mut c = 0u32;
        for b in &brushes {
            let bc = self.brush_contents.get(b.original).copied().unwrap_or(0);
            if bc & contents::SOLID != 0 && b.sides.iter().all(|s| s.on_node) {
                c = contents::SOLID | (bc & contents::DETAIL);
                break;
            }
            c |= bc;
        }
        let n = &mut self.nodes[node];
        n.plane = None;
        n.contents = c;
        n.brushes = brushes;
    }

    fn plane_cuts_volume(&self, node: usize, pnum: usize) -> bool {
        let Some(v) = &self.nodes[node].volume else { return false };
        let (f, b) = split_frag(v, pnum, self.planes);
        f.is_some() && b.is_some()
    }

    fn select_split(&mut self, node: usize, brushes: &mut [Frag]) -> Option<(usize, bool)> {
        let mut best: Option<(usize, i64, Vec<u8>, bool)> = None;
        let mut chosen_pass = 0;
        for pass in 0..4 {
            for bi in 0..brushes.len() {
                let is_detail = self.detail(&brushes[bi]);
                if (pass & 1 == 1) != is_detail {
                    continue;
                }
                for si in 0..brushes[bi].sides.len() {
                    let side = &brushes[bi].sides[si];
                    if side.bevel || side.winding.is_none() || side.on_node || side.tested || side.no_split {
                        continue;
                    }
                    if side.surf & surf::SKIP != 0 {
                        continue;
                    }
                    if side.visible != (pass < 2) {
                        continue;
                    }
                    let side_hint = side.surf & surf::HINT != 0;
                    let pnum = side.plane & !1;
                    if !self.plane_cuts_volume(node, pnum) {
                        continue;
                    }
                    let (mut front, mut back, mut facing, mut splits, mut eps) = (0i64, 0i64, 0i64, 0i64, 0i64);
                    let mut hint_split = false;
                    let mut sides = Vec::with_capacity(brushes.len());
                    for t in brushes.iter_mut() {
                        let (s, bs, h, e) = test_brush_to_plane(t, pnum, self.planes);
                        splits += i64::from(bs);
                        hint_split |= h;
                        if e {
                            eps += 1;
                        }
                        if s & PSIDE_FACING != 0 {
                            facing += 1;
                            for ts in &mut t.sides {
                                if ts.plane & !1 == pnum {
                                    ts.tested = true;
                                }
                            }
                        }
                        if s & PSIDE_FRONT != 0 {
                            front += 1;
                        }
                        if s & PSIDE_BACK != 0 {
                            back += 1;
                        }
                        sides.push(s);
                    }
                    let mut value = 5 * facing - 5 * splits - (front - back).abs();
                    if self.planes.get(pnum).kind < 3 {
                        value += 5;
                    }
                    value -= eps * 1000;
                    if hint_split && !side_hint {
                        value = -9_999_999;
                    }
                    if self.brush_contents.get(brushes[bi].original).is_some_and(|c| c & contents::MASK_WATER != 0)
                        && brushes[bi].sides[si].surf & (surf::NODRAW | surf::SKIP | surf::HINT) == 0
                    {
                        value = 9_999_999;
                    }
                    if best.as_ref().is_none_or(|(_, v, _, _)| value > *v) {
                        let areaportal = self
                            .brush_contents
                            .get(brushes[bi].original)
                            .is_some_and(|c| c & contents::AREAPORTAL != 0);
                        best = Some((pnum, value, sides, areaportal));
                    }
                }
            }
            if best.is_some() {
                chosen_pass = pass;
                break;
            }
        }
        for b in brushes.iter_mut() {
            for s in &mut b.sides {
                s.tested = false;
            }
        }
        let (pnum, _, sides, areaportal) = best?;
        for (b, s) in brushes.iter_mut().zip(sides) {
            b.side = s;
        }
        Some((pnum, chosen_pass > 0 && !areaportal))
    }
}

impl Tree {
    pub fn ensure_node(&mut self, planes: &mut PlaneSet) -> bool {
        let head = self.head;
        let Some(top) = self.nodes.get(head).filter(|n| n.is_leaf()) else { return false };
        let valid = |lo: DVec3, hi: DVec3| lo.is_finite() && hi.is_finite() && lo.cmple(hi).all();
        let (mins, maxs) = if valid(top.mins, top.maxs) {
            (top.mins, top.maxs)
        } else if valid(self.mins, self.maxs) {
            (self.mins, self.maxs)
        } else {
            (DVec3::ZERO, DVec3::ZERO)
        };
        let leaf = Node {
            contents: top.contents,
            cluster: top.cluster,
            area: top.area,
            mins,
            maxs,
            parent: None,
            ..Node::default()
        };
        let plane = planes.find_f32(glam::Vec3::Z, 0.0);
        let other = self.nodes.len();
        self.nodes.push(leaf);
        let node = self.nodes.len();
        self.nodes.push(Node { plane: Some(plane), children: [head, other], mins, maxs, ..Node::default() });
        for child in [head, other] {
            self.nodes[child].parent = Some(node);
        }
        self.head = node;
        true
    }

    pub fn prune(&mut self) {
        let mut order = Vec::new();
        let mut stack = vec![self.head];
        while let Some(n) = stack.pop() {
            order.push(n);
            if !self.nodes[n].is_leaf() {
                stack.extend(self.nodes[n].children);
            }
        }
        for &n in order.iter().rev() {
            if self.nodes[n].is_leaf() {
                continue;
            }
            let [a, b] = self.nodes[n].children;
            let solid = |x: usize| self.nodes[x].is_leaf() && self.nodes[x].contents & contents::SOLID != 0;
            if solid(a) && solid(b) && self.nodes[n].faces.is_empty() {
                let mut list: Vec<Frag> = std::mem::take(&mut self.nodes[a].brushes);
                list.reverse();
                list.extend(std::mem::take(&mut self.nodes[b].brushes));
                let node = &mut self.nodes[n];
                node.plane = None;
                node.contents = contents::SOLID;
                node.detail_separator = false;
                node.brushes = list;
                node.children = [0, 0];
            }
        }
    }

    pub fn leaf_at(&self, planes: &PlaneSet, p: DVec3) -> usize {
        let mut n = self.head;
        while let Some(pl) = self.nodes[n].plane {
            n = if planes.get(pl).distance(p) >= 0.0 { self.nodes[n].children[0] } else { self.nodes[n].children[1] };
        }
        n
    }

    pub fn number_clusters(&mut self) -> i32 {
        self.number_clusters_with(&[])
    }

    pub fn number_clusters_with(&mut self, forced: &[Option<u32>]) -> i32 {
        let mut count = 0;
        let mut keyed: ahash::AHashMap<u32, i32> = ahash::AHashMap::new();
        let mut stack = vec![self.head];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            if !node.is_leaf() && !node.detail_separator {
                let [c0, c1] = node.children;
                stack.push(c1);
                stack.push(c0);
                continue;
            }
            let mut leaves = Vec::new();
            let mut sub = vec![n];
            while let Some(m) = sub.pop() {
                if self.nodes[m].is_leaf() {
                    leaves.push(m);
                } else {
                    sub.extend(self.nodes[m].children);
                }
            }
            let mut own: Option<i32> = None;
            for l in &leaves {
                if self.nodes[*l].contents & contents::SOLID != 0 {
                    self.nodes[*l].cluster = -1;
                    continue;
                }
                let c = match forced.get(*l).copied().flatten() {
                    Some(k) => *keyed.entry(k).or_insert_with(|| {
                        count += 1;
                        count - 1
                    }),
                    None => *own.get_or_insert_with(|| {
                        count += 1;
                        count - 1
                    }),
                };
                self.nodes[*l].cluster = c;
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::BrushSide;

    fn cube(planes: &mut PlaneSet, lo: DVec3, hi: DVec3, c: u32) -> MapBrush {
        let mut sides = Vec::new();
        for axis in 0..3 {
            for (dir, d) in [(1.0, hi[axis]), (-1.0, -lo[axis])] {
                let mut n = DVec3::ZERO;
                n[axis] = dir;
                sides.push(BrushSide {
                    plane: planes.find(n, d).unwrap(),
                    texinfo: 0,
                    surf: 0,
                    winding: None,
                    bevel: false,
                    visible: true,
                    no_split: false,
                    side_id: 0,
                    disp: None,
                    contents: c,
                    fwinding: None,
                    smoothing: 0,
                    overlays: Vec::new(),
                });
            }
        }
        let mut b = MapBrush { id: 1, entity: 0, contents: c, sides, mins: lo, maxs: hi, surfaceprop: String::new() };
        assert!(b.make_windings(planes));
        b
    }

    #[test]
    fn split_keeps_volume() {
        let mut planes = PlaneSet::new();
        let b = cube(&mut planes, DVec3::ZERO, DVec3::splat(64.0), contents::SOLID);
        let f = Frag::from_brush(0, &b);
        let p = planes.find(DVec3::X, 16.0).unwrap();
        let (a, c) = split_frag(&f, p, &planes);
        let (a, c) = (a.unwrap(), c.unwrap());
        assert!((a.volume() - 48.0 * 64.0 * 64.0).abs() < 1.0, "{}", a.volume());
        assert!((c.volume() - 16.0 * 64.0 * 64.0).abs() < 1.0, "{}", c.volume());
        assert!(a.mins.x >= 15.9 && c.maxs.x <= 16.1);
    }

    #[test]
    fn a_hollow_box_has_an_empty_inside_and_solid_walls() {
        let mut planes = PlaneSet::new();
        let (lo, hi) = (DVec3::splat(-128.0), DVec3::splat(128.0));
        let mut brushes = Vec::new();
        for axis in 0..3 {
            for top in [false, true] {
                let (mut a, mut b) = (lo, hi);
                if top {
                    a[axis] = hi[axis] - 16.0;
                } else {
                    b[axis] = lo[axis] + 16.0;
                }
                brushes.push(cube(&mut planes, a, b, contents::SOLID));
            }
        }
        let contents: Vec<u32> = brushes.iter().map(|b| b.contents).collect();
        let frags: Vec<Frag> = brushes.iter().enumerate().map(|(i, b)| Frag::from_brush(i, b)).collect();
        let tree = Builder::new(&mut planes, &contents).build(frags, lo, hi);
        let inside = tree.leaf_at(&planes, DVec3::ZERO);
        assert_eq!(tree.nodes[inside].contents & contents::SOLID, 0);
        let wall = tree.leaf_at(&planes, DVec3::new(0.0, 0.0, 120.0));
        assert_ne!(tree.nodes[wall].contents & contents::SOLID, 0);
    }

    #[test]
    fn blocks_like_vbsp() {
        assert_eq!(block_range(DVec3::new(-3000.0, 100.0, 0.0), DVec3::new(2047.0, 900.0, 0.0)), (-3, 0, 1, 0));
        assert_eq!(block_range(DVec3::splat(-40000.0), DVec3::splat(40000.0)), (-16, -16, 15, 15));
        let mut planes = PlaneSet::new();
        let floor = cube(&mut planes, DVec3::new(-1500.0, 0.0, -16.0), DVec3::new(2500.0, 512.0, 0.0), contents::SOLID);
        let contents = vec![floor.contents];
        let frags = vec![Frag::from_brush(0, &floor)];
        let (lo, hi) = (floor.mins, floor.maxs);
        let tree = Builder::new(&mut planes, &contents).with_blocks().build(frags, lo, hi);
        for x in [-1024.0, 0.0, 1024.0, 2048.0] {
            let p = planes.find(DVec3::X, x).unwrap() & !1;
            assert!(tree.nodes.iter().any(|n| n.plane == Some(p)), "no node on x = {x}");
        }
        let above = tree.leaf_at(&planes, DVec3::new(100.0, 100.0, 4.0));
        assert_eq!(tree.nodes[above].contents & contents::SOLID, 0);
        let inside = tree.leaf_at(&planes, DVec3::new(1500.0, 100.0, -8.0));
        assert_ne!(tree.nodes[inside].contents & contents::SOLID, 0);
        let mut planes2 = PlaneSet::new();
        let floor2 =
            cube(&mut planes2, DVec3::new(-1500.0, 0.0, -16.0), DVec3::new(2500.0, 512.0, 0.0), contents::SOLID);
        let tree2 = Builder::new(&mut planes2, &contents).build(vec![Frag::from_brush(0, &floor2)], lo, hi);
        let x0 = planes2.find(DVec3::X, 0.0).map(|p| p & !1);
        assert!(!tree2.nodes.iter().any(|n| n.plane.is_some() && n.plane == x0));
    }
}
