use glam::DVec3;

use crate::brush::MapBrush;
use crate::materials::contents;
use crate::planes::{MapPlane, PlaneSet};
use crate::tree::{SIDESPACE, Tree};
use crate::winding::Winding;

#[derive(Clone, Debug)]
pub struct Portal {
    pub plane: MapPlane,
    pub on_node: Option<usize>,
    pub nodes: [usize; 2],
    pub winding: Winding,
    pub side: Option<(usize, usize)>,
    pub face: [Option<usize>; 2],
}

#[derive(Clone, Debug, Default)]
pub struct Portals {
    pub list: Vec<Portal>,
}

pub fn visible_contents(c: u32) -> u32 {
    let mut i = 1u32;
    while i <= contents::OPAQUE {
        if c & i != 0 {
            return i;
        }
        i <<= 1;
    }
    0
}

fn fw(w: &Winding) -> Vec<glam::Vec3> {
    w.points.iter().map(|p| p.as_vec3()).collect()
}

fn dw(w: Vec<glam::Vec3>) -> Winding {
    Winding::new(w.into_iter().map(|p| p.as_dvec3()).collect())
}

const NONE: u32 = u32::MAX;

struct Links {
    head: Vec<u32>,
    next: Vec<[u32; 2]>,
    prev: Vec<[u32; 2]>,
}

impl Links {
    fn new(nodes: usize) -> Self {
        Self { head: vec![NONE; nodes], next: Vec::new(), prev: Vec::new() }
    }

    fn slot(list: &[Portal], pid: usize, node: usize) -> usize {
        usize::from(list[pid].nodes[0] != node)
    }

    fn link(&mut self, list: &[Portal], pid: usize) {
        if self.next.len() <= pid {
            self.next.resize(pid + 1, [NONE; 2]);
            self.prev.resize(pid + 1, [NONE; 2]);
        }
        for s in 0..2 {
            let node = list[pid].nodes[s];
            let h = self.head[node];
            self.next[pid][s] = h;
            self.prev[pid][s] = NONE;
            if h != NONE {
                let hs = Self::slot(list, h as usize, node);
                self.prev[h as usize][hs] = pid as u32;
            }
            self.head[node] = pid as u32;
        }
    }

    fn unlink(&mut self, list: &[Portal], pid: usize) {
        for s in 0..2 {
            let node = list[pid].nodes[s];
            let (p, n) = (self.prev[pid][s], self.next[pid][s]);
            if p == NONE {
                self.head[node] = n;
            } else {
                let ps = Self::slot(list, p as usize, node);
                self.next[p as usize][ps] = n;
            }
            if n != NONE {
                let ns = Self::slot(list, n as usize, node);
                self.prev[n as usize][ns] = p;
            }
            self.next[pid][s] = NONE;
            self.prev[pid][s] = NONE;
        }
    }

    fn list_of(&self, list: &[Portal], node: usize) -> Vec<usize> {
        let mut out = Vec::new();
        self.fill(list, node, &mut out);
        out
    }

    fn fill(&self, list: &[Portal], node: usize, out: &mut Vec<usize>) {
        out.clear();
        let mut p = self.head[node];
        while p != NONE {
            out.push(p as usize);
            p = self.next[p as usize][Self::slot(list, p as usize, node)];
        }
    }
}

fn node_bounds(fws: &[FWinding], tree: &mut Tree, n: usize, around: &[usize]) {
    let mut lo = DVec3::splat(99999.0);
    let mut hi = DVec3::splat(-99999.0);
    for &p in around {
        for q in &fws[p] {
            let q = q.as_dvec3();
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    tree.nodes[n].mins = lo;
    tree.nodes[n].maxs = hi;
}

type FWinding = Vec<glam::Vec3>;

impl Portals {
    fn add(&mut self, links: &mut Links, fws: &mut Vec<FWinding>, p: Portal, w: FWinding) -> usize {
        let id = self.list.len();
        self.list.push(p);
        fws.push(w);
        links.link(&self.list, id);
        id
    }

    fn relink(&mut self, links: &mut Links, id: usize, front: usize, back: usize) {
        self.list[id].nodes = [front, back];
        links.link(&self.list, id);
    }

    pub fn make(tree: &mut Tree, planes: &PlaneSet) -> Portals {
        use crate::brushbsp::{base_winding, chop_in_place};
        use glam::Vec3;
        let mut ps = Portals::default();
        let mut fws: Vec<FWinding> = Vec::new();
        let mut links = Links::new(tree.nodes.len());
        let side = SIDESPACE as f32;
        let lo = tree.mins.as_vec3() - Vec3::splat(side);
        let hi = tree.maxs.as_vec3() + Vec3::splat(side);
        let mut bplanes = [(Vec3::ZERO, 0.0f32); 6];
        for i in 0..3 {
            for j in 0..2 {
                let mut n = Vec3::ZERO;
                let d;
                if j == 1 {
                    n[i] = -1.0;
                    d = -hi[i];
                } else {
                    n[i] = 1.0;
                    d = lo[i];
                }
                bplanes[j * 3 + i] = (n, d);
            }
        }
        let head = tree.head;
        let outside = tree.outside;
        let mut ids = [0usize; 6];
        for i in 0..3 {
            for j in 0..2 {
                let k = j * 3 + i;
                let (n, d) = bplanes[k];
                let plane = MapPlane { normal: n.as_dvec3(), dist: f64::from(d), kind: i as i32 };
                ids[k] = ps.add(
                    &mut links,
                    &mut fws,
                    Portal {
                        plane,
                        on_node: None,
                        nodes: [head, outside],
                        winding: Winding::default(),
                        side: None,
                        face: [None; 2],
                    },
                    base_winding(n, d),
                );
            }
        }
        for (i, &id) in ids.iter().enumerate() {
            let mut w = Some(std::mem::take(&mut fws[id]));
            for (j, &(n, d)) in bplanes.iter().enumerate() {
                if j == i {
                    continue;
                }
                let Some(cur) = w.take() else { break };
                w = chop_in_place(cur, n, d, 0.1);
            }
            fws[id] = w.unwrap_or_default();
        }
        let mut stack = vec![head];
        let mut around = Vec::new();
        while let Some(n) = stack.pop() {
            links.fill(&ps.list, n, &mut around);
            node_bounds(&fws, tree, n, &around);
            if tree.nodes[n].is_leaf() {
                continue;
            }
            ps.make_node_portal(tree, planes, &mut links, &mut fws, n, &around);
            ps.split_node_portals(tree, planes, &mut links, &mut fws, n, &around);
            let [f, b] = tree.nodes[n].children;
            stack.push(b);
            stack.push(f);
        }
        for (p, w) in ps.list.iter_mut().zip(fws) {
            p.winding = dw(w);
        }
        for (n, node) in tree.nodes.iter_mut().enumerate() {
            node.portals = links.list_of(&ps.list, n);
        }
        ps
    }

    fn make_node_portal(
        &mut self,
        tree: &Tree,
        planes: &PlaneSet,
        links: &mut Links,
        fws: &mut Vec<FWinding>,
        n: usize,
        around: &[usize],
    ) {
        use crate::brushbsp::{base_winding, chop_in_place, is_tiny};
        let Some(pnum) = tree.nodes[n].plane else { return };
        let (pn, pd) = planes.getf(pnum);
        let mut w = Some(base_winding(pn, pd));
        let mut child = n;
        let mut parent = tree.nodes[n].parent;
        while let Some(par) = parent {
            let Some(cur) = w.take() else { return };
            let (qn, qd) = planes.getf(tree.nodes[par].plane.unwrap_or(0));
            w = if tree.nodes[par].children[0] == child {
                chop_in_place(cur, qn, qd, 0.001)
            } else {
                chop_in_place(cur, -qn, -qd, 0.001)
            };
            child = par;
            parent = tree.nodes[par].parent;
        }
        for &pid in around {
            let Some(cur) = w.take() else { return };
            let p = &self.list[pid];
            let (qn, qd) = (p.plane.normal.as_vec3(), p.plane.dist as f32);
            w = if p.nodes[0] == n { chop_in_place(cur, qn, qd, 0.1) } else { chop_in_place(cur, -qn, -qd, 0.1) };
        }
        let Some(w) = w else { return };
        if is_tiny(&w) {
            return;
        }
        let plane = *planes.get(pnum);
        let [f, b] = tree.nodes[n].children;
        let portal =
            Portal { plane, on_node: Some(n), nodes: [f, b], winding: Winding::default(), side: None, face: [None; 2] };
        self.add(links, fws, portal, w);
    }

    fn split_node_portals(
        &mut self,
        tree: &Tree,
        planes: &PlaneSet,
        links: &mut Links,
        fws: &mut Vec<FWinding>,
        n: usize,
        around: &[usize],
    ) {
        use crate::brushbsp::{clip_epsilon, is_tiny};
        let Some(pnum) = tree.nodes[n].plane else { return };
        let (pn, pd) = planes.getf(pnum);
        let [f, b] = tree.nodes[n].children;
        for &pid in around {
            let side = if self.list[pid].nodes[0] == n { 0 } else { 1 };
            let other = self.list[pid].nodes[1 - side];
            links.unlink(&self.list, pid);
            let (front, back) = clip_epsilon(&fws[pid], pn, pd, 0.001);
            let front = front.filter(|w| !is_tiny(w));
            let back = back.filter(|w| !is_tiny(w));
            match (front, back) {
                (None, None) => {}
                (None, Some(_)) => {
                    if side == 0 {
                        self.relink(links, pid, b, other);
                    } else {
                        self.relink(links, pid, other, b);
                    }
                }
                (Some(_), None) => {
                    if side == 0 {
                        self.relink(links, pid, f, other);
                    } else {
                        self.relink(links, pid, other, f);
                    }
                }
                (Some(fwd), Some(bwd)) => {
                    let copy = self.list[pid].clone();
                    fws[pid] = fwd;
                    let new = self.list.len();
                    self.list.push(copy);
                    fws.push(bwd);
                    if side == 0 {
                        self.relink(links, pid, f, other);
                        self.relink(links, new, b, other);
                    } else {
                        self.relink(links, pid, other, f);
                        self.relink(links, new, other, b);
                    }
                }
            }
        }
    }

    pub fn flood(&self, tree: &mut Tree, planes: &PlaneSet, origins: &[(usize, DVec3)]) -> (Vec<(usize, usize)>, bool) {
        for n in &mut tree.nodes {
            n.occupied = 0;
            n.occupant = None;
        }
        let mut placed = Vec::new();
        for &(ent, o) in origins {
            let leaf = tree.leaf_at(planes, o + DVec3::Z);
            if tree.nodes[leaf].contents & contents::SOLID != 0 {
                continue;
            }
            placed.push((ent, leaf));
            tree.nodes[leaf].occupant = Some(ent);
            self.flood_portals(tree, leaf, 1);
        }
        let leaked = tree.nodes[tree.outside].occupied != 0;
        (placed, leaked)
    }

    fn flood_portals(&self, tree: &mut Tree, start: usize, dist: u32) {
        let mut stack: Vec<(usize, usize)> = Vec::new();
        tree.nodes[start].occupied = dist;
        stack.push((start, 0));
        while let Some(&mut (n, ref mut at)) = stack.last_mut() {
            let list = &tree.nodes[n].portals;
            if *at >= list.len() {
                stack.pop();
                continue;
            }
            let pid = list[*at];
            *at += 1;
            let p = &self.list[pid];
            let other = if p.nodes[0] == n { p.nodes[1] } else { p.nodes[0] };
            if tree.nodes[other].occupied != 0 {
                continue;
            }
            if tree.nodes[n].contents & contents::SOLID != 0 || tree.nodes[other].contents & contents::SOLID != 0 {
                continue;
            }
            let d = tree.nodes[n].occupied + 1;
            tree.nodes[other].occupied = d;
            stack.push((other, 0));
        }
    }

    pub fn leak_path(&self, tree: &Tree) -> (Vec<DVec3>, usize) {
        let mut out = Vec::new();
        let mut n = tree.outside;
        let mut guard = 0;
        while tree.nodes[n].occupied > 1 && guard < 1_000_000 {
            guard += 1;
            let mut next = None;
            for &pid in &tree.nodes[n].portals {
                let p = &self.list[pid];
                let other = if p.nodes[0] == n { p.nodes[1] } else { p.nodes[0] };
                let o = tree.nodes[other].occupied;
                if o != 0 && o < tree.nodes[n].occupied && next.is_none_or(|(_, best, _)| o < best) {
                    next = Some((other, o, crate::brushbsp::center(&fw(&p.winding)).as_dvec3()));
                }
            }
            let Some((other, _, c)) = next else { break };
            out.push(c);
            n = other;
        }
        (out, n)
    }

    pub fn fill_outside(tree: &mut Tree) {
        for n in &mut tree.nodes {
            if n.is_leaf() && n.occupied == 0 && n.contents != contents::SOLID {
                n.contents = contents::SOLID;
            }
        }
    }

    pub fn find_side(&mut self, tree: &Tree, planes: &PlaneSet, brushes: &[MapBrush], pid: usize) {
        let p = &self.list[pid];
        let Some(on) = p.on_node else { return };
        let vis = visible_contents(tree.nodes[p.nodes[0]].contents ^ tree.nodes[p.nodes[1]].contents);
        if vis == 0 {
            return;
        }
        let pnum = tree.nodes[on].plane.unwrap_or(0);
        let points: Vec<glam::Vec3> = p.winding.points.iter().map(|q| q.as_vec3()).collect();
        let mut best: Option<(usize, usize)> = None;
        let mut best_dist = 1_000_000.0f32;
        'search: for j in 0..2 {
            let n = p.nodes[j];
            for frag in &tree.nodes[n].brushes {
                let Some(b) = brushes.get(frag.original) else { continue };
                if b.contents & vis == 0 {
                    continue;
                }
                for (si, s) in b.sides.iter().enumerate() {
                    if s.bevel || s.no_split {
                        continue;
                    }
                    if s.plane & !1 == pnum {
                        best = Some((frag.original, si));
                        break 'search;
                    }
                    let (n2, d2) = planes.getf(s.plane & !1);
                    let mut total = 0.0f32;
                    for q in &points {
                        total += (n2.dot(*q) - d2).abs();
                        if total > best_dist {
                            break;
                        }
                    }
                    if total < best_dist {
                        best_dist = total;
                        best = Some((frag.original, si));
                    }
                }
            }
        }
        self.list[pid].side = best;
    }

    pub fn mark_visible_sides(&mut self, tree: &Tree, planes: &PlaneSet, brushes: &mut [MapBrush]) {
        for b in brushes.iter_mut() {
            for s in &mut b.sides {
                s.visible = false;
            }
        }
        for pid in 0..self.list.len() {
            let [a, b] = self.list[pid].nodes;
            if tree.nodes[a].occupied == 0 && tree.nodes[b].occupied == 0 {
                continue;
            }
            self.find_side(tree, planes, brushes, pid);
            if let Some((bi, si)) = self.list[pid].side
                && let Some(s) = brushes.get_mut(bi).and_then(|b| b.sides.get_mut(si))
            {
                s.visible = true;
            }
        }
    }
}
