use std::collections::{HashMap, HashSet};

use glam::DVec3;

pub const NET_EPS: f64 = 0.01;
const STRAIGHT_COS: f64 = 0.8;

#[derive(Clone, Copy, Debug)]
pub struct NetSolid<'a> {
    pub points: &'a [DVec3],
    pub faces: &'a [Vec<usize>],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EdgeRun {
    pub edges: Vec<usize>,
    pub closed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlideVertex {
    pub vertex: usize,
    pub up: DVec3,
    pub down: DVec3,
}

#[derive(Clone, Debug, Default)]
pub struct EdgeNet {
    points: Vec<DVec3>,
    edges: Vec<[usize; 2]>,
    faces: Vec<Vec<usize>>,
    lookup: HashMap<(usize, usize), usize>,
    edge_faces: Vec<Vec<usize>>,
    vert_edges: Vec<Vec<usize>>,
}

enum Fan {
    Closed(Vec<usize>),
    Open(Vec<usize>),
    Broken,
}

fn key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

fn cell(p: DVec3, size: f64) -> [i64; 3] {
    let c = (p / size).floor();
    [c.x as i64, c.y as i64, c.z as i64]
}

fn clean_ring(ring: &[usize]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::with_capacity(ring.len());
    for &v in ring {
        if out.last() != Some(&v) {
            out.push(v);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

fn ring_normal(points: &[DVec3], ring: &[usize]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..ring.len() {
        let (Some(&a), Some(&b)) =
            (ring.get(i).and_then(|k| points.get(*k)), ring.get((i + 1) % ring.len()).and_then(|k| points.get(*k)))
        else {
            continue;
        };
        n += a.cross(b);
    }
    n.try_normalize().unwrap_or(DVec3::ZERO)
}

impl EdgeNet {
    pub fn from_faces(points: Vec<DVec3>, faces: Vec<Vec<usize>>) -> Self {
        let all = faces.clone();
        Self::build(points, &all, faces)
    }

    pub fn merged(solids: &[NetSolid<'_>], eps: f64) -> (Self, Vec<Vec<usize>>) {
        let eps = if eps.is_finite() && eps > 0.0 { eps } else { NET_EPS };
        let size = eps * 4.0;
        let mut points: Vec<DVec3> = Vec::new();
        let mut cells: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
        let mut vmap: Vec<Vec<usize>> = Vec::with_capacity(solids.len());
        for s in solids {
            let mut map = Vec::with_capacity(s.points.len());
            for &p in s.points {
                let c = cell(p, size);
                let mut found = None;
                'search: for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            let Some(list) = cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else { continue };
                            if let Some(&k) =
                                list.iter().find(|&&k| points.get(k).is_some_and(|q| q.distance(p) <= eps))
                            {
                                found = Some(k);
                                break 'search;
                            }
                        }
                    }
                }
                let id = found.unwrap_or_else(|| {
                    points.push(p);
                    cells.entry(c).or_default().push(points.len() - 1);
                    points.len() - 1
                });
                map.push(id);
            }
            vmap.push(map);
        }
        let mut all: Vec<Vec<usize>> = Vec::new();
        let mut owner: Vec<usize> = Vec::new();
        for (si, s) in solids.iter().enumerate() {
            let Some(map) = vmap.get(si) else { continue };
            for ring in s.faces {
                let mapped: Vec<usize> = ring.iter().filter_map(|k| map.get(*k).copied()).collect();
                let ring = clean_ring(&mapped);
                let distinct: HashSet<usize> = ring.iter().copied().collect();
                if ring.len() >= 3 && distinct.len() == ring.len() {
                    all.push(ring);
                    owner.push(si);
                }
            }
        }
        let mut groups: HashMap<Vec<usize>, Vec<usize>> = HashMap::new();
        for (fi, ring) in all.iter().enumerate() {
            let mut k = ring.clone();
            k.sort_unstable();
            groups.entry(k).or_default().push(fi);
        }
        let mut cancelled = vec![false; all.len()];
        for list in groups.values() {
            let mut pending: Vec<usize> = list.clone();
            while let Some(first) = pending.pop() {
                let Some(pos) = pending.iter().position(|o| owner.get(*o) != owner.get(first)) else { continue };
                let other = pending.remove(pos);
                if let Some(c) = cancelled.get_mut(first) {
                    *c = true;
                }
                if let Some(c) = cancelled.get_mut(other) {
                    *c = true;
                }
            }
        }
        let surface: Vec<Vec<usize>> =
            all.iter().zip(&cancelled).filter(|(_, c)| !**c).map(|(r, _)| r.clone()).collect();
        (Self::build(points, &all, surface), vmap)
    }

    fn build(points: Vec<DVec3>, all: &[Vec<usize>], surface: Vec<Vec<usize>>) -> Self {
        let mut lookup: HashMap<(usize, usize), usize> = HashMap::new();
        let mut edges: Vec<[usize; 2]> = Vec::new();
        for ring in all.iter().chain(surface.iter()) {
            let n = ring.len();
            for i in 0..n {
                let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
                if a == b {
                    continue;
                }
                let k = key(a, b);
                lookup.entry(k).or_insert_with(|| {
                    edges.push([k.0, k.1]);
                    edges.len() - 1
                });
            }
        }
        let mut edge_faces: Vec<Vec<usize>> = vec![Vec::new(); edges.len()];
        for (fi, ring) in surface.iter().enumerate() {
            let n = ring.len();
            for i in 0..n {
                let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
                if let Some(slot) = lookup.get(&key(a, b)).and_then(|e| edge_faces.get_mut(*e))
                    && !slot.contains(&fi)
                {
                    slot.push(fi);
                }
            }
        }
        let mut vert_edges: Vec<Vec<usize>> = vec![Vec::new(); points.len()];
        for (e, [a, b]) in edges.iter().enumerate() {
            if edge_faces.get(e).is_some_and(|f| !f.is_empty()) {
                for v in [a, b] {
                    if let Some(list) = vert_edges.get_mut(*v) {
                        list.push(e);
                    }
                }
            }
        }
        Self { points, edges, faces: surface, lookup, edge_faces, vert_edges }
    }

    pub fn points(&self) -> &[DVec3] {
        &self.points
    }

    pub fn edges(&self) -> &[[usize; 2]] {
        &self.edges
    }

    pub fn faces(&self) -> &[Vec<usize>] {
        &self.faces
    }

    pub fn edge_between(&self, a: usize, b: usize) -> Option<usize> {
        self.lookup.get(&key(a, b)).copied()
    }

    pub fn edge_faces(&self, e: usize) -> &[usize] {
        self.edge_faces.get(e).map_or(&[], Vec::as_slice)
    }

    pub fn vertex_edges(&self, v: usize) -> &[usize] {
        self.vert_edges.get(v).map_or(&[], Vec::as_slice)
    }

    pub fn other_end(&self, e: usize, v: usize) -> Option<usize> {
        let [a, b] = *self.edges.get(e)?;
        if a == v {
            Some(b)
        } else if b == v {
            Some(a)
        } else {
            None
        }
    }

    pub fn neighbours(&self, v: usize) -> Vec<usize> {
        self.vertex_edges(v).iter().filter_map(|e| self.other_end(*e, v)).collect()
    }

    fn face_rank(&self, f: usize, toward: Option<DVec3>) -> (usize, i64) {
        let Some(ring) = self.faces.get(f) else { return (0, i64::MIN) };
        let facing = toward.map_or(0.0, |t| ring_normal(&self.points, ring).dot(t));
        (ring.len(), (facing * 1000.0).round() as i64)
    }

    fn best_face(&self, candidates: &[usize], toward: Option<DVec3>) -> Option<usize> {
        candidates
            .iter()
            .copied()
            .max_by(|a, b| self.face_rank(*a, toward).cmp(&self.face_rank(*b, toward)).then(b.cmp(a)))
    }

    fn fan(&self, v: usize) -> Fan {
        let ve = self.vertex_edges(v);
        let mut faces: Vec<usize> = Vec::new();
        for e in ve {
            for f in self.edge_faces(*e) {
                if !faces.contains(f) {
                    faces.push(*f);
                }
            }
        }
        let mut links: Vec<(usize, usize)> = Vec::with_capacity(faces.len());
        for f in faces {
            let Some(ring) = self.faces.get(f) else { return Fan::Broken };
            let n = ring.len();
            let mut at = ring.iter().enumerate().filter(|(_, x)| **x == v).map(|(i, _)| i);
            let (Some(i), None) = (at.next(), at.next()) else { return Fan::Broken };
            let (Some(&p), Some(&q)) = (ring.get((i + n - 1) % n), ring.get((i + 1) % n)) else { return Fan::Broken };
            let (Some(e1), Some(e2)) = (self.edge_between(p, v), self.edge_between(v, q)) else { return Fan::Broken };
            links.push((e1, e2));
        }
        let degree = |e: usize| links.iter().filter(|(a, b)| *a == e || *b == e).count();
        if ve.iter().any(|e| degree(*e) > 2 || degree(*e) == 0) {
            return Fan::Broken;
        }
        let ends: Vec<usize> = ve.iter().copied().filter(|e| degree(*e) == 1).collect();
        let open = match ends.len() {
            0 => false,
            2 => true,
            _ => return Fan::Broken,
        };
        let Some(&start) = ends.first().or(ve.first()) else { return Fan::Broken };
        let mut used = vec![false; links.len()];
        let mut order = vec![start];
        let mut cur = start;
        while let Some(li) = links.iter().enumerate().position(|(i, (a, b))| !used[i] && (*a == cur || *b == cur)) {
            used[li] = true;
            let (a, b) = links[li];
            let next = if a == cur { b } else { a };
            if next == start {
                break;
            }
            order.push(next);
            cur = next;
        }
        if order.len() != ve.len() || used.iter().any(|u| !*u) {
            return Fan::Broken;
        }
        if open { Fan::Open(order) } else { Fan::Closed(order) }
    }

    fn straight(&self, e: usize, v: usize) -> Option<usize> {
        let from = self.other_end(e, v)?;
        let dir_in = (*self.points.get(v)? - *self.points.get(from)?).try_normalize()?;
        let mut best: Option<(f64, usize)> = None;
        for &c in self.vertex_edges(v) {
            if c == e {
                continue;
            }
            let to = self.other_end(c, v)?;
            let Some(dir_out) = (*self.points.get(to)? - *self.points.get(v)?).try_normalize() else { continue };
            let d = dir_in.dot(dir_out);
            if d >= STRAIGHT_COS && best.is_none_or(|(bd, _)| d > bd) {
                best = Some((d, c));
            }
        }
        best.map(|(_, c)| c)
    }

    fn face_edge_at(&self, f: usize, v: usize, not: usize) -> Option<usize> {
        let ring = self.faces.get(f)?;
        let n = ring.len();
        let i = ring.iter().position(|x| *x == v)?;
        let (p, q) = (*ring.get((i + n - 1) % n)?, *ring.get((i + 1) % n)?);
        [self.edge_between(p, v)?, self.edge_between(v, q)?].into_iter().find(|e| *e != not)
    }

    fn next_edge(&self, e: usize, v: usize, pref: &mut Option<usize>, toward: Option<DVec3>) -> Option<usize> {
        let ve = self.vertex_edges(v);
        if !ve.contains(&e) {
            return None;
        }
        match ve.len() {
            0 | 1 => None,
            2 => ve.iter().copied().find(|c| *c != e),
            n => match self.fan(v) {
                Fan::Closed(order) if n == 4 => {
                    let at = order.iter().position(|x| *x == e)?;
                    order.get((at + 2) % 4).copied()
                }
                Fan::Closed(_) if n == 3 => {
                    let here: Vec<usize> =
                        self.edge_faces(e).iter().copied().filter(|f| self.face_edge_at(*f, v, e).is_some()).collect();
                    let face = match pref.filter(|p| here.contains(p)) {
                        Some(p) => p,
                        None => self.best_face(&here, toward)?,
                    };
                    *pref = Some(face);
                    self.face_edge_at(face, v, e)
                }
                Fan::Closed(_) => None,
                Fan::Open(order) => {
                    let (first, last) = (*order.first()?, *order.last()?);
                    if e == first {
                        Some(last)
                    } else if e == last {
                        Some(first)
                    } else {
                        None
                    }
                }
                Fan::Broken => self.straight(e, v),
            },
        }
    }

    fn initial_face(&self, e: usize, toward: Option<DVec3>) -> Option<usize> {
        self.best_face(self.edge_faces(e), toward)
    }

    pub fn edge_loop(&self, start: usize, toward: Option<DVec3>) -> EdgeRun {
        let Some(&[a, b]) = self.edges.get(start) else { return EdgeRun::default() };
        if self.edge_faces(start).is_empty() {
            return EdgeRun { edges: vec![start], closed: false };
        }
        let face0 = self.initial_face(start, toward);
        let mut seen: HashSet<usize> = HashSet::from([start]);
        let mut walk = |from: usize| -> (Vec<usize>, bool) {
            let mut out = Vec::new();
            let mut pref = face0;
            let (mut e, mut v) = (start, from);
            loop {
                let Some(n) = self.next_edge(e, v, &mut pref, toward) else { return (out, false) };
                if n == start {
                    return (out, true);
                }
                if !seen.insert(n) {
                    return (out, false);
                }
                out.push(n);
                let Some(w) = self.other_end(n, v) else { return (out, false) };
                e = n;
                v = w;
            }
        };
        let (forward, closed) = walk(b);
        if closed {
            let mut edges = vec![start];
            edges.extend(forward);
            return EdgeRun { edges, closed: true };
        }
        let (mut backward, _) = walk(a);
        backward.reverse();
        backward.push(start);
        backward.extend(forward);
        EdgeRun { edges: backward, closed: false }
    }

    fn ring_step(&self, e: usize, f: usize) -> Option<(usize, Option<usize>)> {
        let ring = self.faces.get(f)?;
        if ring.len() != 4 {
            return None;
        }
        let [a, b] = *self.edges.get(e)?;
        let i = (0..4).find(|&i| {
            let (x, y) = (ring.get(i).copied(), ring.get((i + 1) % 4).copied());
            (x == Some(a) && y == Some(b)) || (x == Some(b) && y == Some(a))
        })?;
        let opposite = self.edge_between(*ring.get((i + 2) % 4)?, *ring.get((i + 3) % 4)?)?;
        let faces = self.edge_faces(opposite);
        let next = if faces.len() == 2 { faces.iter().copied().find(|x| *x != f) } else { None };
        Some((opposite, next))
    }

    pub fn edge_ring(&self, start: usize) -> EdgeRun {
        if self.edges.get(start).is_none() {
            return EdgeRun::default();
        }
        let faces = self.edge_faces(start).to_vec();
        let mut seen: HashSet<usize> = HashSet::from([start]);
        let mut walk = |first: usize| -> (Vec<usize>, bool) {
            let mut out = Vec::new();
            let (mut e, mut f) = (start, first);
            loop {
                let Some((opp, next)) = self.ring_step(e, f) else { return (out, false) };
                if opp == start {
                    return (out, true);
                }
                if !seen.insert(opp) {
                    return (out, false);
                }
                out.push(opp);
                let Some(nf) = next else { return (out, false) };
                e = opp;
                f = nf;
            }
        };
        let Some(&f0) = faces.first() else { return EdgeRun { edges: vec![start], closed: false } };
        let (forward, closed) = walk(f0);
        if closed {
            let mut edges = vec![start];
            edges.extend(forward);
            return EdgeRun { edges, closed: true };
        }
        let mut backward = match faces.get(1) {
            Some(&f1) => walk(f1).0,
            None => Vec::new(),
        };
        backward.reverse();
        backward.push(start);
        backward.extend(forward);
        EdgeRun { edges: backward, closed: false }
    }

    pub fn run_path(&self, edges: &[usize]) -> Vec<usize> {
        let Some(&[a, b]) = edges.first().and_then(|e| self.edges.get(*e)) else { return Vec::new() };
        let Some(&second) = edges.get(1) else { return vec![a, b] };
        let touches = |v: usize| self.edges.get(second).is_some_and(|[c, d]| *c == v || *d == v);
        let (start, mut cur) = if touches(b) { (a, b) } else { (b, a) };
        let mut out = vec![start, cur];
        for &e in &edges[1..] {
            let Some(next) = self.other_end(e, cur) else { break };
            out.push(next);
            cur = next;
        }
        if out.len() > 2 && out.first() == out.last() {
            out.pop();
        }
        out
    }

    pub fn run_vertices(&self, edges: &[usize]) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for e in edges {
            if let Some([a, b]) = self.edges.get(*e) {
                for v in [a, b] {
                    if !out.contains(v) {
                        out.push(*v);
                    }
                }
            }
        }
        out
    }

    pub fn induced_edges(&self, set: &HashSet<usize>) -> Vec<usize> {
        self.edges
            .iter()
            .enumerate()
            .filter(|(e, [a, b])| set.contains(a) && set.contains(b) && !self.edge_faces(*e).is_empty())
            .map(|(e, _)| e)
            .collect()
    }

    pub fn grow(&self, set: &HashSet<usize>) -> HashSet<usize> {
        let mut out = set.clone();
        for &v in set {
            out.extend(self.neighbours(v));
        }
        out
    }

    pub fn shrink(&self, set: &HashSet<usize>) -> HashSet<usize> {
        set.iter().copied().filter(|v| self.neighbours(*v).iter().all(|n| set.contains(n))).collect()
    }

    pub fn boundary(&self, set: &HashSet<usize>) -> (HashSet<usize>, Vec<usize>) {
        let region: Vec<bool> = self.faces.iter().map(|r| r.iter().all(|v| set.contains(v))).collect();
        if region.iter().any(|r| *r) {
            let mut verts = HashSet::new();
            let mut edges = Vec::new();
            for (e, [a, b]) in self.edges.iter().enumerate() {
                let faces = self.edge_faces(e);
                let inside = faces.iter().filter(|f| region.get(**f).copied().unwrap_or(false)).count();
                if inside == 1 && !faces.is_empty() {
                    edges.push(e);
                    verts.insert(*a);
                    verts.insert(*b);
                }
            }
            return (verts, edges);
        }
        let verts: HashSet<usize> = set
            .iter()
            .copied()
            .filter(|v| !self.neighbours(*v).is_empty() && self.neighbours(*v).iter().any(|n| !set.contains(n)))
            .collect();
        let edges = self.induced_edges(&verts);
        (verts, edges)
    }

    fn left_face(&self, u: usize, w: usize) -> Option<usize> {
        let e = self.edge_between(u, w)?;
        self.edge_faces(e).iter().copied().find(|f| {
            self.faces.get(*f).is_some_and(|r| {
                let n = r.len();
                (0..n).any(|i| r.get(i) == Some(&u) && r.get((i + 1) % n) == Some(&w))
            })
        })
    }

    fn neighbour_in(&self, f: usize, v: usize, after: bool) -> Option<usize> {
        let ring = self.faces.get(f)?;
        let n = ring.len();
        let i = ring.iter().position(|x| *x == v)?;
        ring.get(if after { (i + 1) % n } else { (i + n - 1) % n }).copied()
    }

    fn chains(&self, edges: &[usize]) -> Option<Vec<(Vec<usize>, bool)>> {
        let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
        for &e in edges {
            let [a, b] = *self.edges.get(e)?;
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
        if adj.values().any(|l| l.len() > 2) {
            return None;
        }
        let mut seen: HashSet<usize> = HashSet::new();
        let mut out = Vec::new();
        let mut starts: Vec<usize> = adj.iter().filter(|(_, l)| l.len() == 1).map(|(v, _)| *v).collect();
        starts.sort_unstable();
        let mut cyc: Vec<usize> = adj.iter().filter(|(_, l)| l.len() == 2).map(|(v, _)| *v).collect();
        cyc.sort_unstable();
        for s in starts.into_iter().chain(cyc) {
            if seen.contains(&s) {
                continue;
            }
            let mut chain = vec![s];
            seen.insert(s);
            let mut prev = usize::MAX;
            let mut cur = s;
            let mut closed = false;
            loop {
                let next = adj.get(&cur)?.iter().copied().find(|n| *n != prev);
                let Some(next) = next else { break };
                if next == s {
                    closed = chain.len() > 2;
                    break;
                }
                if !seen.insert(next) {
                    break;
                }
                chain.push(next);
                prev = cur;
                cur = next;
            }
            out.push((chain, closed));
        }
        Some(out)
    }

    pub fn slide_plan(&self, edges: &[usize]) -> Option<Vec<SlideVertex>> {
        let chains = self.chains(edges)?;
        let mut plan: Vec<Vec<SlideVertex>> = Vec::new();
        for (chain, closed) in chains {
            let n = chain.len();
            if n < 2 {
                continue;
            }
            let mut list = Vec::with_capacity(n);
            for i in 0..n {
                let v = chain[i];
                let prev = if i > 0 {
                    Some(chain[i - 1])
                } else if closed {
                    chain.last().copied()
                } else {
                    None
                };
                let next = if i + 1 < n {
                    Some(chain[i + 1])
                } else if closed {
                    chain.first().copied()
                } else {
                    None
                };
                let (mut up, mut down): (Option<usize>, Option<usize>) = (None, None);
                match (prev, next) {
                    (Some(u), nx) => {
                        if let Some(f) = self.left_face(u, v)
                            && let Some(x) = self.neighbour_in(f, v, true)
                            && Some(x) != nx
                        {
                            up = Some(x);
                        }
                        if let Some(f) = self.left_face(v, u)
                            && let Some(y) = self.neighbour_in(f, v, false)
                            && Some(y) != nx
                        {
                            down = Some(y);
                        }
                    }
                    (None, Some(w)) => {
                        if let Some(f) = self.left_face(v, w)
                            && let Some(y) = self.neighbour_in(f, v, false)
                        {
                            up = Some(y);
                        }
                        if let Some(f) = self.left_face(w, v)
                            && let Some(z) = self.neighbour_in(f, v, true)
                        {
                            down = Some(z);
                        }
                    }
                    (None, None) => {}
                }
                let p = *self.points.get(v)?;
                let pos = |k: Option<usize>| k.and_then(|k| self.points.get(k)).copied();
                let (u_pos, d_pos) = (pos(up), pos(down));
                let (u_pos, d_pos) = match (u_pos, d_pos) {
                    (Some(u), Some(d)) => (u, d),
                    (Some(u), None) => (u, p * 2.0 - u),
                    (None, Some(d)) => (p * 2.0 - d, d),
                    (None, None) => continue,
                };
                list.push(SlideVertex { vertex: v, up: u_pos, down: d_pos });
            }
            plan.push(list);
        }
        let lean = |list: &[SlideVertex]| list.iter().fold(DVec3::ZERO, |acc, s| acc + (s.up - self.points[s.vertex]));
        let first = plan.iter().map(|l| lean(l)).find(|v| v.length() > 1e-9).unwrap_or(DVec3::ZERO);
        for list in &mut plan {
            if lean(list).dot(first) < 0.0 {
                list.iter_mut().for_each(|s| std::mem::swap(&mut s.up, &mut s.down));
            }
        }
        let total = plan.iter().fold(DVec3::ZERO, |acc, l| acc + lean(l));
        let canonical = [total.z, total.y, total.x].into_iter().find(|c| c.abs() > 1e-9 * total.length().max(1.0));
        if canonical.is_some_and(|c| c < 0.0) {
            for s in plan.iter_mut().flatten() {
                std::mem::swap(&mut s.up, &mut s.down);
            }
        }
        Some(plan.into_iter().flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn box_points(min: DVec3, max: DVec3) -> Vec<DVec3> {
        (0..8)
            .map(|i| {
                DVec3::new(
                    if i & 1 == 0 { min.x } else { max.x },
                    if i & 2 == 0 { min.y } else { max.y },
                    if i & 4 == 0 { min.z } else { max.z },
                )
            })
            .collect()
    }

    pub(super) fn box_faces() -> Vec<Vec<usize>> {
        vec![vec![4, 5, 7, 6], vec![0, 2, 3, 1], vec![0, 1, 5, 4], vec![2, 6, 7, 3], vec![0, 4, 6, 2], vec![1, 3, 7, 5]]
    }

    fn unit_box() -> EdgeNet {
        EdgeNet::from_faces(box_points(DVec3::ZERO, DVec3::splat(10.0)), box_faces())
    }

    #[test]
    fn box_faces_point_outward() {
        let pts = box_points(DVec3::ZERO, DVec3::splat(10.0));
        for ring in box_faces() {
            let n = ring_normal(&pts, &ring);
            let center = ring.iter().map(|i| pts[*i]).sum::<DVec3>() / 4.0;
            assert!(n.dot(center - DVec3::splat(5.0)) > 0.0);
        }
    }

    #[test]
    fn loop_on_a_box_follows_the_preferred_face() {
        let net = unit_box();
        assert_eq!(net.edges().len(), 12);
        let e = net.edge_between(4, 5).unwrap();
        let top = net.edge_loop(e, Some(DVec3::Z));
        assert!(top.closed);
        assert_eq!(top.edges.len(), 4);
        let mut verts = net.run_vertices(&top.edges);
        verts.sort_unstable();
        assert_eq!(verts, vec![4, 5, 6, 7]);
        let front = net.edge_loop(e, Some(-DVec3::Y));
        let mut verts = net.run_vertices(&front.edges);
        verts.sort_unstable();
        assert_eq!(verts, vec![0, 1, 4, 5]);
    }

    #[test]
    fn ring_on_a_box_goes_across_the_quads() {
        let net = unit_box();
        let e = net.edge_between(4, 5).unwrap();
        let ring = net.edge_ring(e);
        assert!(ring.closed);
        assert_eq!(ring.edges.len(), 4);
        for r in &ring.edges {
            let [a, b] = net.edges()[*r];
            assert_eq!(net.points()[a].y, net.points()[b].y);
            assert_eq!(net.points()[a].z, net.points()[b].z);
            assert_ne!(net.points()[a].x, net.points()[b].x);
        }
    }

    #[test]
    fn prism_loop_follows_the_cap_ring() {
        let n = 6;
        let mut pts = Vec::new();
        for z in [0.0, 8.0] {
            for i in 0..n {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                pts.push(DVec3::new(a.cos() * 10.0, a.sin() * 10.0, z));
            }
        }
        let mut faces = vec![(n..2 * n).collect::<Vec<_>>(), (0..n).rev().collect::<Vec<_>>()];
        for i in 0..n {
            let j = (i + 1) % n;
            faces.push(vec![i, j, n + j, n + i]);
        }
        let net = EdgeNet::from_faces(pts, faces);
        let e = net.edge_between(n, n + 1).unwrap();
        let run = net.edge_loop(e, Some(DVec3::Z));
        assert!(run.closed);
        assert_eq!(run.edges.len(), 6);
        let vertical = net.edge_between(0, n).unwrap();
        let run = net.edge_loop(vertical, None);
        assert!(run.closed);
        assert_eq!(run.edges.len(), 4);
    }

    #[test]
    fn merged_boxes_cancel_the_shared_face_and_loop_straight() {
        let a = box_points(DVec3::ZERO, DVec3::new(10.0, 10.0, 10.0));
        let b = box_points(DVec3::new(10.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 10.0));
        let faces = box_faces();
        let solids = [NetSolid { points: &a, faces: &faces }, NetSolid { points: &b, faces: &faces }];
        let (net, vmap) = EdgeNet::merged(&solids, NET_EPS);
        assert_eq!(net.points().len(), 12);
        assert_eq!(vmap[0][1], vmap[1][0]);
        assert_eq!(net.faces().len(), 10);
        let start = net.edge_between(vmap[0][4], vmap[0][5]).unwrap();
        let run = net.edge_loop(start, Some(DVec3::Z));
        assert!(run.closed);
        assert_eq!(run.edges.len(), 6);
    }

    #[test]
    fn loop_through_a_four_valent_vertex_goes_straight() {
        let mut pts = Vec::new();
        for y in 0..3 {
            for x in 0..3 {
                pts.push(DVec3::new(x as f64, y as f64, 0.0));
            }
        }
        let mut faces = Vec::new();
        for y in 0..2 {
            for x in 0..2 {
                let i = y * 3 + x;
                faces.push(vec![i, i + 1, i + 4, i + 3]);
            }
        }
        let net = EdgeNet::from_faces(pts, faces);
        let e = net.edge_between(3, 4).unwrap();
        let run = net.edge_loop(e, None);
        assert_eq!(run.edges.len(), 2);
        assert!(!run.closed);
        let ring = net.edge_ring(e);
        assert_eq!(ring.edges.len(), 3);
        let boundary = net.edge_loop(net.edge_between(0, 1).unwrap(), None);
        assert!(boundary.closed);
        assert_eq!(boundary.edges.len(), 8);
        let across = net.edge_loop(net.edge_between(1, 4).unwrap(), None);
        assert_eq!(across.edges.len(), 2);
    }

    #[test]
    fn grow_shrink_and_boundary() {
        let net = unit_box();
        let one: HashSet<usize> = HashSet::from([0]);
        let grown = net.grow(&one);
        assert_eq!(grown.len(), 4);
        assert_eq!(net.shrink(&grown), one);
        let top: HashSet<usize> = HashSet::from([4, 5, 6, 7]);
        let (verts, edges) = net.boundary(&top);
        assert_eq!(verts, top);
        assert_eq!(edges.len(), 4);
        let all: HashSet<usize> = (0..8).collect();
        assert!(net.boundary(&all).1.is_empty());
    }

    #[test]
    fn slide_plan_moves_a_cap_ring_along_the_side_edges() {
        let net = unit_box();
        let e = net.edge_between(4, 5).unwrap();
        let run = net.edge_loop(e, Some(DVec3::Z));
        let plan = net.slide_plan(&run.edges).unwrap();
        assert_eq!(plan.len(), 4);
        for s in &plan {
            let p = net.points()[s.vertex];
            assert_eq!(p.z, 10.0);
            let a = (s.up - p).z;
            let b = (s.down - p).z;
            assert!(
                (a + 10.0).abs() < 1e-9 && (b - 10.0).abs() < 1e-9
                    || (a - 10.0).abs() < 1e-9 && (b + 10.0).abs() < 1e-9
            );
        }
        let all: Vec<usize> = (0..net.edges().len()).collect();
        assert!(net.slide_plan(&all).is_none());
        let path = net.run_path(&run.edges);
        assert_eq!(path.len(), 4);
        for w in 0..4 {
            assert!(net.edge_between(path[w], path[(w + 1) % 4]).is_some());
        }
    }
}
