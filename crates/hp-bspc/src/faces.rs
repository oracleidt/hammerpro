use glam::{DVec3, Vec3};

use crate::brush::MapBrush;
use crate::materials::{contents, surf};
use crate::planes::PlaneSet;
use crate::portals::{Portals, visible_contents};
use crate::texinfo::TexInfos;
use crate::tree::Tree;
use crate::winding::Winding;

pub const MAX_LIGHTMAP_DIM: f64 = 32.0;
pub const MAX_LIGHTMAP_EXTENT: i32 = hp_bsp::limits::LIGHTMAP_EXTENT.max() as i32;
pub const LUXEL_SLACK: f64 = 0.02;

#[derive(Clone, Debug)]
pub struct Face {
    pub plane: usize,
    pub texinfo: i32,
    pub winding: Winding,
    pub contents: u32,
    pub on_node: usize,
    pub merged: Option<usize>,
    pub split: Option<[usize; 2]>,
    pub output: Option<usize>,
    pub source: (usize, usize),
    pub also: Vec<(usize, usize)>,
    pub verts: Vec<u32>,
    pub piece: bool,
    pub prim: Option<usize>,
    pub smoothing: u32,
}

impl Face {
    pub fn live(&self) -> bool {
        self.merged.is_none() && self.split.is_none()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Faces {
    pub list: Vec<Face>,
    pub vertices: Vec<DVec3>,
    pub prim_list: Vec<Vec<u16>>,
    vhash: Vec<u32>,
    vchain: Vec<u32>,
    pub options: FaceOptions,
    pub tight: ahash::AHashMap<(usize, usize, usize), u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FaceOptions {
    pub no_merge: bool,
    pub no_subdiv: bool,
    pub no_tjunc: bool,
    pub max_lightmap_dim: Option<f32>,
    pub max_extent: Option<i32>,
}

impl Faces {
    pub fn make(
        tree: &mut Tree,
        portals: &mut Portals,
        planes: &PlaneSet,
        brushes: &[MapBrush],
        tex: &TexInfos,
        world: bool,
        keep_nodraw: bool,
    ) -> Faces {
        Faces::make_with(tree, portals, planes, brushes, tex, world, keep_nodraw, FaceOptions::default())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn make_with(
        tree: &mut Tree,
        portals: &mut Portals,
        planes: &PlaneSet,
        brushes: &[MapBrush],
        tex: &TexInfos,
        world: bool,
        keep_nodraw: bool,
        options: FaceOptions,
    ) -> Faces {
        let mut faces = Faces { options, ..Faces::default() };
        for leaf in 0..tree.nodes.len() {
            let node = &tree.nodes[leaf];
            if !node.is_leaf() || leaf == tree.outside || node.contents & contents::SOLID != 0 {
                continue;
            }
            for pid in node.portals.clone() {
                let s = usize::from(portals.list[pid].nodes[1] == leaf);
                if portals.list[pid].side.is_none() {
                    portals.find_side(tree, planes, brushes, pid);
                }
                let p = &portals.list[pid];
                let Some(on) = p.on_node else { continue };
                let Some((bi, si)) = p.side else { continue };
                let Some(side) = brushes.get(bi).and_then(|b| b.sides.get(si)) else { continue };
                if side.texinfo < 0 {
                    continue;
                }
                let flags = tex.infos.get(side.texinfo as usize).map_or(0, |t| t.flags);
                if !keep_nodraw && flags & (surf::NODRAW | surf::SKIP | surf::HINT) != 0 && flags & surf::SKY == 0 {
                    continue;
                }
                let here = tree.nodes[leaf].contents;
                let there = tree.nodes[p.nodes[1 - s]].contents;
                let delta = visible_contents(here ^ there);
                if (here & contents::WINDOW != 0 && delta == contents::WINDOW)
                    || (here & contents::GRATE != 0 && delta == contents::GRATE)
                {
                    continue;
                }
                let mut texinfo = side.texinfo;
                if here & contents::WATER != 0 && visible_contents(here ^ there) == contents::WATER {
                    match tex.bottom.get(&texinfo) {
                        Some(&b) => texinfo = b,
                        None => continue,
                    }
                }
                let plane = if world || here & contents::MASK_WATER != 0 { (side.plane & !1) | s } else { side.plane };
                let w = if s == 1 { p.winding.reversed() } else { p.winding.clone() };
                let id = faces.list.len();
                faces.list.push(Face {
                    plane,
                    texinfo,
                    winding: w,
                    contents: here,
                    on_node: on,
                    merged: None,
                    split: None,
                    output: None,
                    source: (bi, si),
                    also: Vec::new(),
                    verts: Vec::new(),
                    piece: false,
                    prim: None,
                    smoothing: side.smoothing,
                });
                portals.list[pid].face[s] = Some(id);
                tree.nodes[on].faces.insert(0, id);
            }
        }
        faces
    }

    pub fn merge(&mut self, tree: &mut Tree, planes: &PlaneSet, brushes: &[MapBrush]) {
        for n in 0..tree.nodes.len() {
            let mut list = std::mem::take(&mut tree.nodes[n].faces);
            self.merge_list(&mut list, planes, brushes);
            tree.nodes[n].faces = list;
        }
    }

    pub fn merge_list(&mut self, list: &mut Vec<usize>, planes: &PlaneSet, brushes: &[MapBrush]) {
        if self.options.no_merge {
            return;
        }
        let side = |s: (usize, usize)| brushes.get(s.0).and_then(|b| b.sides.get(s.1));
        let same_sides = |a: (usize, usize), b: (usize, usize)| match (side(a), side(b)) {
            (Some(x), Some(y)) => {
                x.smoothing == y.smoothing
                    && x.overlays.len() == y.overlays.len()
                    && x.overlays.iter().all(|o| y.overlays.contains(o))
            }
            _ => true,
        };
        let mut i = 0;
        while i < list.len() {
            let f1 = list[i];
            if !self.list[f1].live() {
                i += 1;
                continue;
            }
            for j in 0..i {
                let f2 = list[j];
                if !self.list[f2].live() {
                    continue;
                }
                let (a, b) = (&self.list[f1], &self.list[f2]);
                if a.texinfo != b.texinfo
                    || a.plane != b.plane
                    || a.contents != b.contents
                    || !same_sides(a.source, b.source)
                {
                    continue;
                }
                let normal = planes.get(a.plane).normal;
                if let Some(w) = try_merge(&a.winding, &b.winding, normal) {
                    let mut nf = a.clone();
                    nf.winding = w;
                    for s in std::iter::once(b.source).chain(b.also.iter().copied()) {
                        if s != nf.source && !nf.also.contains(&s) {
                            nf.also.push(s);
                        }
                    }
                    let id = self.list.len();
                    self.list.push(nf);
                    self.list[f1].merged = Some(id);
                    self.list[f2].merged = Some(id);
                    list.push(id);
                    break;
                }
            }
            i += 1;
        }
    }

    pub fn subdivide(&mut self, tree: &mut Tree, planes: &PlaneSet, tex: &TexInfos) {
        let _ = planes;
        for n in 0..tree.nodes.len() {
            self.subdivide_node(tree, n, tex);
        }
    }

    pub fn subdivide_node(&mut self, tree: &mut Tree, n: usize, tex: &TexInfos) {
        if self.options.no_subdiv {
            return;
        }
        let list = tree.nodes[n].faces.clone();
        for f in list {
            self.subdivide_face(tree, n, f, tex);
        }
    }

    fn subdivide_face(&mut self, tree: &mut Tree, n: usize, f: usize, tex: &TexInfos) {
        use glam::Vec3;
        if !self.list[f].live() {
            return;
        }
        let Some(info) = tex.infos.get(self.list[f].texinfo as usize) else { return };
        if info.flags & surf::NOLIGHT != 0 {
            return;
        }
        let face = &self.list[f];
        let margin = self.tight.get(&(face.source.0, face.source.1, face.plane)).map_or(0.0, |&m| m as f32);
        let max_dim = (self.options.max_lightmap_dim.unwrap_or(MAX_LIGHTMAP_DIM as f32) - margin).max(2.0);
        for axis in 0..2 {
            let l = info.lightmap[axis];
            let temp = Vec3::new(l[0], l[1], l[2]);
            let w: Vec<Vec3> = self.list[f].winding.points.iter().map(|p| p.as_vec3()).collect();
            let (mut mins, mut maxs) = (999_999.0f32, -999_999.0f32);
            for p in &w {
                let v = p.dot(temp);
                if v < mins {
                    mins = v;
                }
                if v > maxs {
                    maxs = v;
                }
            }
            if maxs - mins <= max_dim {
                continue;
            }
            let (normal, luxels) = crate::planes::normalize_f32(temp);
            let dist = (mins + max_dim - 1.0) / luxels;
            let (front, back) = crate::brushbsp::clip_epsilon(&w, normal, dist, 0.1);
            let (Some(front), Some(back)) = (front, back) else { return };
            let to_w = |v: Vec<Vec3>| Winding::new(v.into_iter().map(|p| p.as_dvec3()).collect());
            let mut a = self.list[f].clone();
            a.winding = to_w(front);
            let mut b = self.list[f].clone();
            b.winding = to_w(back);
            let ia = self.list.len();
            self.list.push(a);
            let ib = self.list.len();
            self.list.push(b);
            self.list[f].split = Some([ia, ib]);
            tree.nodes[n].faces.insert(0, ia);
            tree.nodes[n].faces.insert(0, ib);
            self.subdivide_face(tree, n, ia, tex);
            self.subdivide_face(tree, n, ib, tex);
            return;
        }
    }

    pub fn oversized(&self, tex: &TexInfos) -> Vec<usize> {
        if self.options.no_subdiv {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (i, f) in self.list.iter().enumerate() {
            if !f.live() || f.verts.len() < 3 {
                continue;
            }
            let Some(info) = tex.infos.get(f.texinfo as usize) else { continue };
            if info.flags & (surf::NOLIGHT | surf::SKY | surf::NODRAW) != 0 {
                continue;
            }
            let points = f.verts.iter().filter_map(|&v| self.vertices.get(v as usize)).map(|p| p.as_vec3());
            let (_, size) = lightmap_extents(points, &info.lightmap);
            let max_extent = self.options.max_extent.unwrap_or(MAX_LIGHTMAP_EXTENT);
            if size.iter().any(|&s| s > max_extent) {
                out.push(i);
            }
        }
        out
    }

    pub fn tighten(&self, bad: &[usize], tight: &mut ahash::AHashMap<(usize, usize, usize), u32>) -> bool {
        let cap = (self.options.max_lightmap_dim.unwrap_or(MAX_LIGHTMAP_DIM as f32) / 2.0).max(1.0) as u32;
        let mut changed = false;
        for &i in bad {
            let f = &self.list[i];
            let m = tight.entry((f.source.0, f.source.1, f.plane)).or_insert(0);
            let next = if *m == 0 { 1 } else { (*m * 2).min(cap) };
            if next > *m {
                *m = next;
                changed = true;
            }
        }
        changed
    }

    pub fn final_faces(&self, mut f: usize, out: &mut Vec<usize>) {
        let mut guard = 0;
        while let Some(m) = self.list[f].merged {
            f = m;
            guard += 1;
            if guard > 100_000 {
                return;
            }
        }
        match self.list[f].split {
            Some([a, b]) => {
                self.final_faces(a, out);
                self.final_faces(b, out);
            }
            None => {
                if !out.contains(&f) {
                    out.push(f);
                }
            }
        }
    }

    pub fn fix_tjuncs(&mut self, tree: &mut Tree, world: bool) {
        self.vertices.clear();
        self.vhash = vec![u32::MAX; HASH_SIZE * HASH_SIZE];
        self.vchain.clear();
        for n in 0..tree.nodes.len() {
            let list = tree.nodes[n].faces.clone();
            for f in list {
                if !self.list[f].live() {
                    continue;
                }
                let pts: Vec<Vec3> = self.list[f].winding.points.iter().map(|p| p.as_vec3()).collect();
                let superverts: Vec<u32> = pts.iter().map(|p| self.vertex_num(*p)).collect();
                self.face_from_superverts(tree, n, f, &superverts, 0);
            }
        }
        if self.options.no_tjunc {
            return;
        }
        let work: Vec<(usize, usize)> =
            (0..tree.nodes.len()).flat_map(|n| tree.nodes[n].faces.iter().map(move |&f| (n, f))).collect();
        let edges: Vec<Option<EdgePoints>> = {
            use rayon::prelude::*;
            let me = &*self;
            work.par_iter().map(|&(_, f)| me.edge_points(f)).collect()
        };
        for (&(n, f), e) in work.iter().zip(edges) {
            if let Some(e) = e {
                self.fix_face_edges(tree, n, f, world, e);
            }
        }
    }

    fn vertex_num(&mut self, p: Vec3) -> u32 {
        let mut v = Vec3::ZERO;
        for i in 0..3 {
            let r = (f64::from(p[i]) + 0.5) as i32;
            v[i] = if f64::from((p[i] - r as f32).abs()) < 0.01 { (p[i] + 0.5) as i32 as f32 } else { p[i] };
        }
        let h = hash_cell(v.y) * HASH_SIZE + hash_cell(v.x);
        let mut i = self.vhash[h];
        while i != u32::MAX {
            let q = self.vertices[i as usize].as_vec3();
            if (0..3).all(|k| f64::from((q[k] - v[k]).abs()) < 0.1) {
                return i;
            }
            i = self.vchain[i as usize];
        }
        let n = self.vertices.len() as u32;
        self.vertices.push(v.as_dvec3());
        self.vchain.push(self.vhash[h]);
        self.vhash[h] = n;
        n
    }

    fn face_from_superverts(&mut self, tree: &mut Tree, n: usize, f: usize, superverts: &[u32], base: usize) {
        if superverts.len() > MAXEDGES
            && let Some(pieces) = self.chord_pieces(superverts)
        {
            let mut cur = f;
            let last = pieces.len() - 1;
            for (k, verts) in pieces.into_iter().enumerate() {
                if k == last {
                    self.list[cur].verts = verts;
                    break;
                }
                let mut piece = self.list[cur].clone();
                piece.merged = None;
                piece.split = None;
                piece.piece = true;
                piece.verts = verts;
                piece.prim = None;
                let a = self.list.len();
                self.list.push(piece);
                tree.nodes[n].faces.insert(0, a);
                let mut rest = self.list[cur].clone();
                rest.merged = None;
                rest.split = None;
                rest.piece = true;
                rest.prim = None;
                let b = self.list.len();
                self.list.push(rest);
                tree.nodes[n].faces.insert(0, b);
                self.list[cur].split = Some([a, b]);
                cur = b;
            }
            return;
        }
        self.fan_pieces(tree, n, f, superverts, base);
    }

    fn fan_pieces(&mut self, tree: &mut Tree, n: usize, f: usize, superverts: &[u32], mut base: usize) {
        let num = superverts.len();
        let mut remaining = num;
        let mut cur = f;
        while remaining > MAXEDGES {
            let mut piece = self.list[cur].clone();
            piece.merged = None;
            piece.split = None;
            piece.piece = true;
            piece.verts = (0..MAXEDGES).map(|i| superverts[(i + base) % num]).collect();
            piece.prim = None;
            let a = self.list.len();
            self.list.push(piece);
            tree.nodes[n].faces.insert(0, a);
            let mut rest = self.list[cur].clone();
            rest.merged = None;
            rest.split = None;
            rest.piece = true;
            rest.prim = None;
            let b = self.list.len();
            self.list.push(rest);
            tree.nodes[n].faces.insert(0, b);
            self.list[cur].split = Some([a, b]);
            cur = b;
            remaining -= MAXEDGES - 2;
            base = (base + MAXEDGES - 1) % num;
        }
        self.list[cur].verts = (0..remaining).map(|i| superverts[(i + base) % num]).collect();
    }

    fn colinear(&self, a: u32, b: u32, c: u32) -> bool {
        let at = |v: u32| self.vertices.get(v as usize).copied().unwrap_or(DVec3::ZERO);
        let (pa, pb, pc) = (at(a), at(b), at(c));
        let ac = pc - pa;
        let len = ac.length();
        len < 1e-6 || (pb - pa).cross(ac).length() / len < COLINEAR_EPSILON
    }

    fn chord_pieces(&self, sv: &[u32]) -> Option<Vec<Vec<u32>>> {
        let n = sv.len();
        let corners: Vec<usize> =
            (0..n).filter(|&k| !self.colinear(sv[(k + n - 1) % n], sv[k], sv[(k + 1) % n])).collect();
        if corners.len() < 3 {
            return None;
        }
        let m = corners.len();
        let mut order: Vec<usize> = (0..m).collect();
        let runs = |ci: usize| {
            let c = corners[ci];
            ((corners[(ci + 1) % m] + n - c) % n, (corners[(ci + m - 1) % m] + n - c) % n)
        };
        order.sort_by_key(|&ci| {
            let (a, b) = runs(ci);
            a.max(n - b)
        });
        for ci in order {
            let c = corners[ci];
            let (a, b) = runs(ci);
            if a + 2 > MAXEDGES || n - b + 2 > MAXEDGES || b < a + 2 {
                continue;
            }
            let rel = |i: usize| sv[(c + i) % n];
            let mut pieces: Vec<Vec<u32>> = Vec::new();
            let mut s = 1;
            let ok = loop {
                if n - s < MAXEDGES {
                    pieces.push(std::iter::once(rel(0)).chain((s..n).map(rel)).collect());
                    break true;
                }
                let e = (s + MAXEDGES - 2).min(b - 1);
                if e <= s || e < a + 1 {
                    break false;
                }
                pieces.push(std::iter::once(rel(0)).chain((s..=e).map(rel)).collect());
                s = e;
            };
            if ok {
                return Some(pieces);
            }
        }
        None
    }

    fn piece_prims(&mut self, f: usize, world: bool) {
        let mut pieces = Vec::new();
        self.final_faces(f, &mut pieces);
        for p in pieces {
            let verts = self.list[p].verts.clone();
            let n = verts.len();
            if n <= 3 {
                continue;
            }
            let flat = |k: usize| self.colinear(verts[(k + n - 1) % n], verts[k], verts[(k + 1) % n]);
            if !(0..n).any(flat) {
                continue;
            }
            let clean = (0..n).find(|&k| !flat(k) && !flat((k + 1) % n) && !flat((k + n - 1) % n));
            if let Some(k) = clean {
                self.list[p].verts.rotate_left(k);
                continue;
            }
            if !world {
                continue;
            }
            let corners: Vec<usize> = (0..n).filter(|&k| !flat(k)).collect();
            if corners.len() < 3 {
                continue;
            }
            let mut table = vec![[-1i32; 2]; n];
            for (r, &c) in corners.iter().enumerate() {
                let end = corners[(r + 1) % corners.len()];
                let mut k = c;
                loop {
                    let t = &mut table[k];
                    if t[0] == -1 {
                        t[0] = r as i32;
                    } else {
                        t[1] = r as i32;
                    }
                    if k == end {
                        break;
                    }
                    k = (k + 1) % n;
                }
            }
            let indices: Vec<usize> = (0..n).collect();
            let mut tris = Vec::with_capacity((n - 2) * 3);
            triangulate(&mut tris, &indices, &table);
            let tris: Vec<u16> = tris.into_iter().map(|i| i as u16).collect();
            self.list[p].prim = Some(self.prim_list.len());
            self.prim_list.push(tris);
        }
    }

    fn edge_points(&self, f: usize) -> Option<EdgePoints> {
        if !self.list[f].live() {
            return None;
        }
        let verts = &self.list[f].verts;
        let np = verts.len();
        let mut superverts: Vec<u32> = Vec::new();
        let mut start = vec![0usize; np];
        let mut count = vec![0usize; np];
        for i in 0..np {
            let p1 = verts[i];
            let p2 = verts[(i + 1) % np];
            let edge_start = self.vertices[p1 as usize].as_vec3();
            let e2 = self.vertices[p2 as usize].as_vec3();
            let edge_verts = self.edge_verts(edge_start, e2);
            let (dir, len) = crate::planes::normalize_f32(e2 - edge_start);
            start[i] = superverts.len();
            self.test_edge(&edge_verts, edge_start, dir, 0.0, len, p1, p2, 0, &mut superverts);
            count[i] = superverts.len() - start[i];
        }
        Some(EdgePoints { superverts, start, count })
    }

    fn fix_face_edges(&mut self, tree: &mut Tree, n: usize, f: usize, world: bool, e: EdgePoints) {
        let EdgePoints { superverts, start, count } = e;
        let np = start.len();
        if superverts.len() < 3 {
            self.list[f].verts.clear();
            return;
        }
        let clean = (0..np).find(|&i| count[i] == 1 && count[(i + np - 1) % np] == 1);
        let base = clean.map_or(0, |i| start[i]);
        self.face_from_superverts(tree, n, f, &superverts, base);
        self.list[f].prim = None;
        if self.list[f].split.is_some() {
            self.piece_prims(f, world);
            return;
        }
        if clean.is_none() && world {
            let total = superverts.len();
            let mut table = vec![[-1i32; 2]; total];
            for i in 0..np {
                if count[i] == 0 {
                    continue;
                }
                for j in 0..=count[i] {
                    let t = &mut table[(start[i] + j) % total];
                    if t[0] == -1 {
                        t[0] = i as i32;
                    } else {
                        t[1] = i as i32;
                    }
                }
            }
            let indices: Vec<usize> = (0..total).collect();
            let mut tris = Vec::new();
            triangulate(&mut tris, &indices, &table);
            self.list[f].prim = Some(self.prim_list.len());
            self.prim_list.push(tris.into_iter().map(|i| i as u16).collect());
        }
        if self.list[f].prim.is_none() {
            self.piece_prims(f, world);
        }
    }

    fn edge_verts(&self, v1: Vec3, v2: Vec3) -> Vec<u32> {
        let (mut x1, mut y1, mut x2, mut y2) = (hash_cell(v1.x), hash_cell(v1.y), hash_cell(v2.x), hash_cell(v2.y));
        if x1 > x2 {
            std::mem::swap(&mut x1, &mut x2);
        }
        if y1 > y2 {
            std::mem::swap(&mut y1, &mut y2);
        }
        let mut out = Vec::new();
        for x in x1..=x2 {
            for y in y1..=y2 {
                let mut i = self.vhash[y * HASH_SIZE + x];
                while i != u32::MAX {
                    out.push(i);
                    i = self.vchain[i as usize];
                }
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn test_edge(
        &self,
        edge_verts: &[u32],
        edge_start: Vec3,
        dir: Vec3,
        s: f32,
        e: f32,
        p1: u32,
        p2: u32,
        startvert: usize,
        out: &mut Vec<u32>,
    ) {
        if p1 == p2 {
            return;
        }
        for k in startvert..edge_verts.len() {
            let j = edge_verts[k];
            if j == p1 || j == p2 {
                continue;
            }
            let p = self.vertices[j as usize].as_vec3();
            let delta = p - edge_start;
            let dist = delta.dot(dir);
            if dist <= s || dist >= e {
                continue;
            }
            let exact =
                Vec3::new(edge_start.x + dist * dir.x, edge_start.y + dist * dir.y, edge_start.z + dist * dir.z);
            let off = p - exact;
            let error = (off.x * off.x + off.y * off.y + off.z * off.z).sqrt();
            if f64::from(error) > 0.25 {
                continue;
            }
            self.test_edge(edge_verts, edge_start, dir, s, dist, p1, j, k + 1, out);
            self.test_edge(edge_verts, edge_start, dir, dist, e, j, p2, k + 1, out);
            return;
        }
        out.push(p1);
    }
}

struct EdgePoints {
    superverts: Vec<u32>,
    start: Vec<usize>,
    count: Vec<usize>,
}

pub fn lightmap_extents(points: impl IntoIterator<Item = Vec3>, lightmap: &[[f32; 4]; 2]) -> ([i32; 2], [i32; 2]) {
    let mut lo = [1e24f32; 2];
    let mut hi = [-1e24f32; 2];
    for p in points {
        for axis in 0..2 {
            let l = lightmap[axis];
            let val = p.x * l[0] + p.y * l[1] + p.z * l[2] + l[3];
            if val < lo[axis] {
                lo[axis] = val;
            }
            if val > hi[axis] {
                hi[axis] = val;
            }
        }
    }
    let (mut mins, mut size) = ([0i32; 2], [0i32; 2]);
    for axis in 0..2 {
        let (a, b) = (lo[axis].floor(), hi[axis].ceil());
        mins[axis] = a as i32;
        size[axis] = (b - a) as i32;
    }
    (mins, size)
}

const MAXEDGES: usize = 32;
const COLINEAR_EPSILON: f64 = 0.25;
const HASH_SIZE: usize = 256;

fn hash_cell(v: f32) -> usize {
    (((16384 + (v + 0.5) as i32) >> 7).clamp(0, HASH_SIZE as i32 - 1)) as usize
}

fn triangulate(out: &mut Vec<usize>, indices: &[usize], table: &[[i32; 2]]) {
    let has = |t: &[i32; 2], e: i32| e >= 0 && (t[0] == e || t[1] == e);
    let diagonal = |a: &[i32; 2], b: &[i32; 2]| !(has(b, a[0]) || has(b, a[1]));
    let count = indices.len();
    if count <= 3 {
        out.extend_from_slice(indices);
        return;
    }
    for i in 0..count {
        for j in 2..count - 1 {
            let index = indices[i];
            let next = (i + j) % count;
            let next_index = indices[next];
            if diagonal(&table[index], &table[next_index]) {
                let mut in1 = Vec::new();
                let mut k = i;
                while k != next {
                    in1.push(indices[k]);
                    k = (k + 1) % count;
                }
                in1.push(next_index);
                let mut in2 = vec![index];
                let mut l = next;
                while l != i {
                    in2.push(indices[l]);
                    l = (l + 1) % count;
                }
                triangulate(out, &in1, table);
                triangulate(out, &in2, table);
                return;
            }
        }
    }
    for k in 1..count - 1 {
        out.extend_from_slice(&[indices[0], indices[k], indices[k + 1]]);
    }
}

pub fn try_merge(a: &Winding, b: &Winding, normal: DVec3) -> Option<Winding> {
    use glam::Vec3;
    const EQUAL_EPSILON: f64 = 0.001;
    const CONTINUOUS_EPSILON: f64 = 0.001;
    let f1: Vec<Vec3> = a.points.iter().map(|p| p.as_vec3()).collect();
    let f2: Vec<Vec3> = b.points.iter().map(|p| p.as_vec3()).collect();
    let pn = normal.as_vec3();
    let (n1, n2) = (f1.len(), f2.len());
    let mut found = None;
    'outer: for i in 0..n1 {
        let p1 = f1[i];
        let p2 = f1[(i + 1) % n1];
        for j in 0..n2 {
            let p3 = f2[j];
            let p4 = f2[(j + 1) % n2];
            let same = (0..3).all(|k| {
                f64::from((p1[k] - p4[k]).abs()) <= EQUAL_EPSILON && f64::from((p2[k] - p3[k]).abs()) <= EQUAL_EPSILON
            });
            if same {
                found = Some((i, j));
                break 'outer;
            }
        }
    }
    let (i, j) = found?;
    let p1 = f1[i];
    let p2 = f1[(i + 1) % n1];
    let back = f1[(i + n1 - 1) % n1];
    let (nrm, _) = crate::planes::normalize_f32(pn.cross(p1 - back));
    let dot = (f2[(j + 2) % n2] - p1).dot(nrm);
    if f64::from(dot) > CONTINUOUS_EPSILON {
        return None;
    }
    let keep1 = f64::from(dot) < -CONTINUOUS_EPSILON;
    let back = f1[(i + 2) % n1];
    let (nrm, _) = crate::planes::normalize_f32(pn.cross(back - p2));
    let dot = (f2[(j + n2 - 1) % n2] - p2).dot(nrm);
    if f64::from(dot) > CONTINUOUS_EPSILON {
        return None;
    }
    let keep2 = f64::from(dot) < -CONTINUOUS_EPSILON;
    let mut out = Vec::with_capacity(n1 + n2);
    let mut k = (i + 1) % n1;
    while k != i {
        if k != (i + 1) % n1 || keep2 {
            out.push(f1[k].as_dvec3());
        }
        k = (k + 1) % n1;
    }
    let mut l = (j + 1) % n2;
    while l != j {
        if l != (j + 1) % n2 || keep1 {
            out.push(f2[l].as_dvec3());
        }
        l = (l + 1) % n2;
    }
    Some(Winding::new(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_squares_merge_into_a_rectangle() {
        let a = Winding::new(vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 64.0, 0.0),
            DVec3::new(64.0, 64.0, 0.0),
            DVec3::new(64.0, 0.0, 0.0),
        ]);
        let b = Winding::new(vec![
            DVec3::new(64.0, 0.0, 0.0),
            DVec3::new(64.0, 64.0, 0.0),
            DVec3::new(128.0, 64.0, 0.0),
            DVec3::new(128.0, 0.0, 0.0),
        ]);
        let m = try_merge(&a, &b, a.normal()).expect("merge");
        assert_eq!(m.points.len(), 4);
        assert!((m.area() - 128.0 * 64.0).abs() < 1e-6);
        let c = Winding::new(vec![
            DVec3::new(64.0, 64.0, 0.0),
            DVec3::new(64.0, 128.0, 0.0),
            DVec3::new(128.0, 128.0, 0.0),
            DVec3::new(128.0, 64.0, 0.0),
        ]);
        assert!(try_merge(&a, &c, a.normal()).is_none());
    }
    fn ring(bottom: usize, top: usize) -> (Faces, Vec<u32>) {
        let mut faces = Faces::default();
        let mut pts = vec![DVec3::new(0.0, 0.0, 0.0)];
        for k in 1..=bottom {
            pts.push(DVec3::new(512.0 * k as f64 / (bottom + 1) as f64, 0.0, 0.0));
        }
        pts.push(DVec3::new(512.0, 0.0, 0.0));
        pts.push(DVec3::new(512.0, 64.0, 0.0));
        for k in 1..=top {
            pts.push(DVec3::new(512.0 - 512.0 * k as f64 / (top + 1) as f64, 64.0, 0.0));
        }
        pts.push(DVec3::new(0.0, 64.0, 0.0));
        faces.vertices = pts;
        let ids = (0..faces.vertices.len() as u32).collect();
        (faces, ids)
    }

    fn area(faces: &Faces, verts: &[u32]) -> f64 {
        let p: Vec<DVec3> = verts.iter().map(|&v| faces.vertices[v as usize]).collect();
        (1..p.len() - 1).map(|k| (p[k] - p[0]).cross(p[k + 1] - p[0]).length() * 0.5).sum()
    }

    #[test]
    fn long_faces_split_along_chords_that_cross_the_face() {
        let (faces, sv) = ring(40, 12);
        let pieces = faces.chord_pieces(&sv).expect("a split along chords");
        assert!(pieces.len() >= 2);
        let total: f64 = pieces.iter().map(|p| area(&faces, p)).sum();
        assert!((total - 512.0 * 64.0).abs() < 1e-6, "the pieces cover the face once: {total}");
        for p in &pieces {
            assert!(p.len() <= MAXEDGES && p.len() >= 3, "{}", p.len());
            assert!(area(&faces, p) > 1.0, "no piece is a sliver: {p:?}");
        }
        for p in &pieces {
            let (a, b) = (faces.vertices[p[0] as usize], faces.vertices[*p.last().unwrap() as usize]);
            for (k, v) in faces.vertices.iter().enumerate() {
                if p.contains(&(k as u32)) {
                    continue;
                }
                let t = (*v - a).dot(b - a) / (b - a).length_squared();
                let off = (*v - (a + (b - a) * t)).length();
                assert!(!(0.0 < t && t < 1.0 && off < 0.25), "vertex {k} lies on the chord of a piece: a T-junction");
            }
        }
        let mut seen: Vec<u32> = pieces.iter().flatten().copied().collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), sv.len(), "every vertex stays on the face edges");
    }

    #[test]
    fn chords_start_from_a_corner_away_from_long_runs() {
        let (faces, sv) = ring(70, 70);
        assert!(
            faces.chord_pieces(&sv).is_none(),
            "runs longer than a piece next to every corner cannot be cut by chords"
        );
        let (faces, sv) = ring(70, 0);
        let pieces = faces.chord_pieces(&sv).expect("chords from the far corners");
        assert!(pieces.iter().all(|p| area(&faces, p) > 1.0));
        let (faces, sv) = ring(20, 20);
        let pieces = faces.chord_pieces(&sv).expect("split");
        assert!(pieces.iter().all(|p| area(&faces, p) > 1.0));
    }
    fn face_of(verts: Vec<u32>) -> Face {
        Face {
            plane: 0,
            texinfo: 0,
            winding: Winding::new(Vec::new()),
            contents: 0,
            on_node: 0,
            merged: None,
            split: None,
            output: None,
            source: (0, 0),
            also: Vec::new(),
            verts,
            piece: false,
            prim: None,
            smoothing: 0,
        }
    }

    #[test]
    fn faces_with_corners_in_a_row_get_triangles_without_slivers() {
        let mut faces = Faces::default();
        let mut pts = Vec::new();
        for k in 0..24 {
            pts.push(DVec3::new(0.0, 0.0, 1000.0 + 40.0 * k as f64));
        }
        for k in 0..8 {
            pts.push(DVec3::new(64.0, 0.0, 1920.0 - 40.0 * k as f64));
        }
        faces.vertices = pts;
        faces.list.push(face_of((0..32).collect()));
        faces.piece_prims(0, true);
        let prim = faces.list[0].prim.expect("a triangle list");
        let tris = &faces.prim_list[prim];
        assert_eq!(tris.len(), 30 * 3);
        let mut total = 0.0;
        for t in tris.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| faces.vertices[faces.list[0].verts[i as usize] as usize]);
            let area = (b - a).cross(c - a).length() * 0.5;
            assert!(area > 1.0, "no sliver triangle: {t:?}");
            total += area;
        }
        let expected = area(&faces, &faces.list[0].verts.clone());
        assert!((total - expected).abs() < 1e-6, "{total} vs {expected}");
        faces.list.push(face_of(vec![0, 23, 24, 31]));
        faces.piece_prims(1, true);
        assert!(faces.list[1].prim.is_none(), "a plain quad keeps its fan");
        let mut entity = Faces { vertices: faces.vertices.clone(), ..Faces::default() };
        entity.list.push(face_of((0..32).collect()));
        entity.piece_prims(0, false);
        assert!(entity.list[0].prim.is_none() && entity.prim_list.is_empty(), "brush entities get no triangle list");
    }
}
