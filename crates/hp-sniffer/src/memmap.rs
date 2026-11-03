use std::collections::HashMap;

use hp_bsp::{Bsp, lump};

use crate::cm::{
    AxialBox, BOX_RECORD, CmFindings, PLANE_RECORD, attach_surfaces, find_cm_geometry, valid_plane, window_around,
};
use crate::engine::{
    DispLumps, MemoryExtras, cubemap_lump, displacement_lumps, find_extras, overlay_lump, static_prop_lump,
};
use crate::procmem::MemSource;
use crate::scan::ScanOptions;
use crate::world::{RenderWorld, find_render_world};

pub const LEAF_RECORD: usize = 16;
pub const NODE_RECORD: usize = 16;
pub const MODEL_RECORD: usize = 72;
const CHAIN_WINDOW: usize = 64;
const MAX_NODE_REF: i32 = 4_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CLeaf {
    pub contents: i32,
    pub cluster: i16,
    pub area_flags: u16,
    pub first_brush: u16,
    pub num_brushes: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CNode {
    pub plane: u32,
    pub children: [i32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CModel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub origin: [f32; 3],
    pub headnode: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryCm {
    pub cm: CmFindings,
    pub plane_bytes: Vec<u8>,
    pub leafs_addr: u64,
    pub leafs: Vec<CLeaf>,
    pub leafbrushes: Vec<u16>,
    pub models_addr: u64,
    pub models: Vec<CModel>,
    pub nodes_addr: u64,
    pub nodes: Vec<CNode>,
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

pub fn parse_bsp_leafs(data: &[u8]) -> Vec<CLeaf> {
    data.chunks_exact(32)
        .map(|c| CLeaf {
            contents: i32_at(c, 0),
            cluster: i16::from_le_bytes([c[4], c[5]]),
            area_flags: u16_at(c, 6),
            first_brush: u16_at(c, 24),
            num_brushes: u16_at(c, 26),
        })
        .collect()
}

pub fn parse_leafs(data: &[u8]) -> Vec<CLeaf> {
    data.chunks_exact(LEAF_RECORD)
        .map(|c| CLeaf {
            contents: i32_at(c, 0),
            cluster: i16::from_le_bytes([c[4], c[5]]),
            area_flags: u16_at(c, 6),
            first_brush: u16_at(c, 8),
            num_brushes: u16_at(c, 10),
        })
        .collect()
}

fn leaf_plausible(l: &CLeaf) -> bool {
    l.contents >= 0 && l.cluster >= -1 && l.num_brushes <= 8192
}

pub fn leaf_window_ok(window: &[CLeaf]) -> bool {
    if !window.iter().all(leaf_plausible) {
        return false;
    }
    let mut expected = window[0].first_brush;
    let mut with_brushes = 0;
    for l in window {
        if l.first_brush != expected {
            return false;
        }
        expected = l.first_brush.wrapping_add(l.num_brushes);
        if l.num_brushes > 0 {
            with_brushes += 1;
        }
    }
    with_brushes >= 8
}

pub fn leaf_run(leafs: &[CLeaf], at: usize) -> (usize, usize) {
    let mut start = at;
    while start > 0 {
        let prev = &leafs[start - 1];
        let next = &leafs[start];
        if leaf_plausible(prev) && prev.first_brush.wrapping_add(prev.num_brushes) == next.first_brush {
            start -= 1;
        } else {
            break;
        }
    }
    let mut end = at + 1;
    while end < leafs.len() {
        let prev = &leafs[end - 1];
        let next = &leafs[end];
        if leaf_plausible(next) && prev.first_brush.wrapping_add(prev.num_brushes) == next.first_brush {
            end += 1;
        } else {
            break;
        }
    }
    (start, end)
}

pub fn find_leafs<S: MemSource + ?Sized>(src: &S, planes_addr: u64) -> Option<(u64, Vec<CLeaf>, Vec<u16>)> {
    let before = 4u64 << 20;
    let (from, data) = window_around(src, planes_addr.saturating_sub(1), before, 16)?;
    let skew = ((planes_addr - from) % LEAF_RECORD as u64) as usize;
    let base = from + skew as u64;
    let data = &data[skew.min(data.len())..];
    let leafs = parse_leafs(data);
    let mut best: Option<(usize, usize)> = None;
    let mut i = leafs.len().checked_sub(CHAIN_WINDOW)?;
    loop {
        if leaf_window_ok(&leafs[i..i + CHAIN_WINDOW]) {
            let (s, e) = leaf_run(&leafs, i);
            if e - s >= CHAIN_WINDOW {
                best = Some((s, e));
                break;
            }
        }
        if i == 0 {
            break;
        }
        i -= 1;
    }
    let (mut s, e) = best?;
    while s < e && leafs[s].first_brush != 0 {
        s += 1;
    }
    if e - s < CHAIN_WINDOW {
        return None;
    }
    let run = leafs[s..e].to_vec();
    let used = run.last().map(|l| usize::from(l.first_brush) + usize::from(l.num_brushes))?;
    let leafs_addr = base + (s * LEAF_RECORD) as u64;
    let leafs_end = leafs_addr + (run.len() * LEAF_RECORD) as u64;
    let lb_addr = (planes_addr.checked_sub((used * 2) as u64)? / 16) * 16;
    if lb_addr < leafs_end || lb_addr + (used * 2) as u64 > planes_addr {
        return None;
    }
    let raw = src.read_vec(lb_addr, used * 2);
    if raw.len() != used * 2 {
        return None;
    }
    let lb = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    Some((leafs_addr, run, lb))
}

fn node_at(d: &[u8], planes_base: u64, planes: usize) -> Option<CNode> {
    let ptr = u64::from_le_bytes([d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]]);
    if ptr < planes_base {
        return None;
    }
    let off = ptr - planes_base;
    if !off.is_multiple_of(PLANE_RECORD as u64) || off / PLANE_RECORD as u64 >= planes as u64 {
        return None;
    }
    let (c0, c1) = (i32_at(d, 8), i32_at(d, 12));
    if c0.abs() > MAX_NODE_REF || c1.abs() > MAX_NODE_REF {
        return None;
    }
    Some(CNode { plane: (off / PLANE_RECORD as u64) as u32, children: [c0, c1] })
}

pub fn find_nodes<S: MemSource + ?Sized>(
    src: &S,
    from: u64,
    planes_base: u64,
    planes: usize,
    leafs: usize,
) -> Option<(u64, Vec<CNode>)> {
    let (start, data) = window_around(src, from, 0, 8 << 20)?;
    let mut off = 0usize;
    let limit = (1usize << 19).min(data.len());
    while off + NODE_RECORD * CHAIN_WINDOW <= data.len() && off < limit {
        let ok = (0..CHAIN_WINDOW)
            .all(|k| node_at(&data[off + k * NODE_RECORD..off + (k + 1) * NODE_RECORD], planes_base, planes).is_some());
        if ok {
            let mut nodes = Vec::new();
            let mut at = off;
            while at + NODE_RECORD <= data.len() {
                match node_at(&data[at..at + NODE_RECORD], planes_base, planes) {
                    Some(n) => nodes.push(n),
                    None => break,
                }
                at += NODE_RECORD;
            }
            let n = nodes.len() as i32;
            let sane = nodes
                .iter()
                .all(|nd| nd.children.iter().all(|&c| if c >= 0 { c < n } else { ((-1 - c) as usize) < leafs.max(1) }));
            if sane {
                return Some((start + off as u64, nodes));
            }
            return None;
        }
        off += 16;
    }
    None
}

pub fn read_models<S: MemSource + ?Sized>(src: &S, addr: u64, count: usize) -> Vec<CModel> {
    let data = src.read_vec(addr, count * MODEL_RECORD);
    data.chunks_exact(MODEL_RECORD)
        .map(|c| CModel {
            mins: [f32_at(c, 0), f32_at(c, 4), f32_at(c, 8)],
            maxs: [f32_at(c, 12), f32_at(c, 16), f32_at(c, 20)],
            origin: [f32_at(c, 24), f32_at(c, 28), f32_at(c, 32)],
            headnode: i32_at(c, 36),
        })
        .collect()
}

pub fn find_full<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Option<MemoryCm> {
    let mut cm = find_cm_geometry(src, opts)?;
    if cm.planes == 0 || cm.sides.is_empty() && cm.boxes.is_empty() {
        return None;
    }
    let plane_bytes = src.read_vec(cm.planes_addr, cm.planes * PLANE_RECORD);
    if plane_bytes.len() != cm.planes * PLANE_RECORD || !plane_bytes.chunks_exact(PLANE_RECORD).all(valid_plane) {
        return None;
    }
    let (leafs_addr, leafs, leafbrushes) = find_leafs(src, cm.planes_addr)?;
    attach_surfaces(src, &mut cm, Some(leafs_addr), opts);
    let boxes_end = cm.boxes_addr + (cm.boxes.len() * BOX_RECORD) as u64;
    let models_addr = boxes_end.div_ceil(16) * 16;
    let (nodes_addr, nodes) = find_nodes(src, models_addr, cm.planes_addr, cm.planes, leafs.len())?;
    let count = ((nodes_addr - models_addr) / MODEL_RECORD as u64) as usize;
    let models = read_models(src, models_addr, count);
    if models.is_empty() {
        return None;
    }
    Some(MemoryCm { cm, plane_bytes, leafs_addr, leafs, leafbrushes, models_addr, models, nodes_addr, nodes })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryWorld {
    pub cm: MemoryCm,
    pub render: Option<RenderWorld>,
    pub extras: MemoryExtras,
}

impl From<MemoryCm> for MemoryWorld {
    fn from(cm: MemoryCm) -> MemoryWorld {
        MemoryWorld { cm, render: None, extras: MemoryExtras::default() }
    }
}

pub fn find_all<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions, map: &str) -> Option<MemoryWorld> {
    let cm = find_full(src, opts)?;
    let render = find_render_world(src, opts, cm.cm.planes_addr, &cm.plane_bytes);
    let extras = match &render {
        Some(r) => find_extras(src, opts, r, &cm.plane_bytes, map),
        None => MemoryExtras {
            area_portals: crate::engine::find_area_portals(src, opts, &cm.plane_bytes),
            props: crate::engine::find_static_props(src, opts).0,
            cubemaps: crate::engine::find_cubemaps(src, opts, map),
            ..MemoryExtras::default()
        },
    };
    Some(MemoryWorld { cm, render, extras })
}

pub fn texinfo_class(normal: [f32; 3]) -> usize {
    let a = [normal[0].abs(), normal[1].abs(), normal[2].abs()];
    if a[2] >= a[0] && a[2] >= a[1] {
        0
    } else if a[0] >= a[1] {
        1
    } else {
        2
    }
}

fn texinfo_vecs(class: usize) -> [[f32; 4]; 2] {
    match class {
        0 => [[4.0, 0.0, 0.0, 0.0], [0.0, -4.0, 0.0, 0.0]],
        1 => [[0.0, 4.0, 0.0, 0.0], [0.0, 0.0, -4.0, 0.0]],
        _ => [[4.0, 0.0, 0.0, 0.0], [0.0, 0.0, -4.0, 0.0]],
    }
}

struct PlaneTable {
    bytes: Vec<u8>,
    index: HashMap<([u32; 3], u32), u32>,
}

impl PlaneTable {
    fn new(raw: &[u8]) -> PlaneTable {
        let mut bytes = Vec::with_capacity(raw.len());
        let mut index = HashMap::new();
        for (i, rec) in raw.chunks_exact(PLANE_RECORD).enumerate() {
            let key = ([u32_bits(rec, 0), u32_bits(rec, 4), u32_bits(rec, 8)], u32_bits(rec, 12));
            index.entry(key).or_insert(i as u32);
            bytes.extend_from_slice(&rec[..16]);
            bytes.extend_from_slice(&i32::from(rec[16]).to_le_bytes());
        }
        PlaneTable { bytes, index }
    }

    fn len(&self) -> usize {
        self.bytes.len() / 20
    }

    fn normal(&self, i: u32) -> [f32; 3] {
        let at = i as usize * 20;
        [f32_at(&self.bytes, at), f32_at(&self.bytes, at + 4), f32_at(&self.bytes, at + 8)]
    }

    fn find_or_add(&mut self, normal: [f32; 3], dist: f32, kind: i32) -> u32 {
        let key = ([normal[0].to_bits(), normal[1].to_bits(), normal[2].to_bits()], dist.to_bits());
        if let Some(i) = self.index.get(&key) {
            return *i;
        }
        let i = self.len() as u32;
        for v in normal {
            self.bytes.extend_from_slice(&v.to_le_bytes());
        }
        self.bytes.extend_from_slice(&dist.to_le_bytes());
        self.bytes.extend_from_slice(&kind.to_le_bytes());
        self.index.insert(key, i);
        i
    }
}

fn u32_bits(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn box_planes(b: &AxialBox) -> [([f32; 3], f32, i32, usize); 6] {
    [
        ([-1.0, 0.0, 0.0], -b.mins[0], 0, b.surfaces[0] as usize),
        ([1.0, 0.0, 0.0], b.maxs[0], 0, b.surfaces[3] as usize),
        ([0.0, -1.0, 0.0], -b.mins[1], 1, b.surfaces[1] as usize),
        ([0.0, 1.0, 0.0], b.maxs[1], 1, b.surfaces[4] as usize),
        ([0.0, 0.0, -1.0], -b.mins[2], 2, b.surfaces[2] as usize),
        ([0.0, 0.0, 1.0], b.maxs[2], 2, b.surfaces[5] as usize),
    ]
}

pub(crate) fn plane_key(n: [f32; 3], d: f32) -> ([i32; 3], i32) {
    let q = |v: f32| (v * 10_000.0).round() as i32;
    ([q(n[0]), q(n[1]), q(n[2])], (d * 100.0).round() as i32)
}

struct FaceIndex<'a> {
    render: &'a RenderWorld,
    by_plane: HashMap<([i32; 3], i32), Vec<usize>>,
    texdata_of_texinfo: Vec<i32>,
}

impl<'a> FaceIndex<'a> {
    fn new(render: &'a RenderWorld, plane_bytes: &[u8], ranks: Vec<i32>) -> FaceIndex<'a> {
        let mut by_plane: HashMap<([i32; 3], i32), Vec<usize>> = HashMap::new();
        for (i, f) in render.faces.iter().enumerate() {
            if f.disp != 0 {
                continue;
            }
            let at = f.plane as usize * 20;
            let Some(p) = plane_bytes.get(at..at + 16) else { continue };
            let (mut n, mut d) = ([f32_at(p, 0), f32_at(p, 4), f32_at(p, 8)], f32_at(p, 12));
            if f.back {
                n = [-n[0], -n[1], -n[2]];
                d = -d;
            }
            by_plane.entry(plane_key(n, d)).or_default().push(i);
        }
        FaceIndex { render, by_plane, texdata_of_texinfo: ranks }
    }

    fn texinfo_for_side(&self, normal: [f32; 3], dist: f32, texdata: i32, brush: &[([f32; 3], f32)]) -> Option<u16> {
        self.best_texinfo(normal, dist, Some(texdata), brush)
    }

    fn best_texinfo(
        &self,
        normal: [f32; 3],
        dist: f32,
        texdata: Option<i32>,
        brush: &[([f32; 3], f32)],
    ) -> Option<u16> {
        let list = self.by_plane.get(&plane_key(normal, dist))?;
        let mut best: Option<(f64, u16)> = None;
        for &fi in list {
            let f = &self.render.faces[fi];
            if let Some(want) = texdata
                && self.texdata_of_texinfo.get(usize::from(f.texinfo)).copied() != Some(want)
            {
                continue;
            }
            let first = f.first as usize;
            let Some(idx) = self.render.vertindices.get(first..first + usize::from(f.count)) else { continue };
            let poly: Option<Vec<[f64; 3]>> = idx
                .iter()
                .map(|&i| {
                    self.render
                        .vertexes
                        .get(usize::from(i))
                        .map(|v| [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])])
                })
                .collect();
            let Some(poly) = poly else { continue };
            let area = clipped_area(poly, brush, normal, dist);
            if area > 1.0 && best.is_none_or(|(s, _)| area > s) {
                best = Some((area, f.texinfo));
            }
        }
        best.map(|b| b.1)
    }
}

fn clip_polygon(poly: &[[f64; 3]], n: [f64; 3], d: f64) -> Vec<[f64; 3]> {
    let dist = |p: &[f64; 3]| n[0] * p[0] + n[1] * p[1] + n[2] * p[2] - d;
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (a, b) = (&poly[i], &poly[(i + 1) % poly.len()]);
        let (da, db) = (dist(a), dist(b));
        if da <= 0.0 {
            out.push(*a);
        }
        if (da <= 0.0) != (db <= 0.0) {
            let t = da / (da - db);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]);
        }
    }
    out
}

fn polygon_area(poly: &[[f64; 3]]) -> f64 {
    let mut sum = [0.0f64; 3];
    for i in 1..poly.len().saturating_sub(1) {
        let (a, b, c) = (poly[0], poly[i], poly[i + 1]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        sum[0] += u[1] * v[2] - u[2] * v[1];
        sum[1] += u[2] * v[0] - u[0] * v[2];
        sum[2] += u[0] * v[1] - u[1] * v[0];
    }
    0.5 * (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt()
}

fn clipped_area(mut poly: Vec<[f64; 3]>, brush: &[([f32; 3], f32)], own_normal: [f32; 3], own_dist: f32) -> f64 {
    for (n, d) in brush {
        let parallel = n[0] * own_normal[0] + n[1] * own_normal[1] + n[2] * own_normal[2] > 0.9999;
        if parallel && (d - own_dist).abs() < 0.05 {
            continue;
        }
        poly = clip_polygon(&poly, [f64::from(n[0]), f64::from(n[1]), f64::from(n[2])], f64::from(*d) + 0.01);
        if poly.len() < 3 {
            return 0.0;
        }
    }
    polygon_area(&poly)
}

fn render_lumps(r: &RenderWorld, disp: &DispLumps) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut faces = Vec::with_capacity(r.faces.len() * 56);
    let mut edges: Vec<u8> = vec![0u8; 4];
    let mut surfedges: Vec<u8> = Vec::new();
    for (i, f) in r.faces.iter().enumerate() {
        let first = f.first as usize;
        let idx = r.vertindices.get(first..first + usize::from(f.count)).unwrap_or(&[]);
        let first_edge = (surfedges.len() / 4) as i32;
        for k in 0..idx.len() {
            let edge = (edges.len() / 4) as i32;
            edges.extend_from_slice(&idx[k].to_le_bytes());
            edges.extend_from_slice(&idx[(k + 1) % idx.len()].to_le_bytes());
            surfedges.extend_from_slice(&edge.to_le_bytes());
        }
        let mut rec = [0u8; 56];
        rec[0..2].copy_from_slice(&(f.plane as u16).to_le_bytes());
        rec[2] = u8::from(f.back);
        rec[4..8].copy_from_slice(&first_edge.to_le_bytes());
        rec[8..10].copy_from_slice(&(idx.len() as i16).to_le_bytes());
        rec[10..12].copy_from_slice(&(f.texinfo as i16).to_le_bytes());
        rec[12..14].copy_from_slice(&disp.face_disp.get(&i).copied().unwrap_or(-1).to_le_bytes());
        rec[14..16].copy_from_slice(&(-1i16).to_le_bytes());
        rec[16..20].copy_from_slice(&[255, 255, 255, 255]);
        rec[20..24].copy_from_slice(&(-1i32).to_le_bytes());
        faces.extend_from_slice(&rec);
    }
    let mut vertexes = Vec::with_capacity(r.vertexes.len() * 12);
    for v in &r.vertexes {
        for c in v {
            vertexes.extend_from_slice(&c.to_le_bytes());
        }
    }
    (faces, edges, surfedges, vertexes)
}

const ID_SLACK: i64 = 64;
const VOTE_TRUST: f64 = 0.6;

const NAME_FLAGS: [(&str, i32); 2] = [("tools/toolsnodraw", 0x80), ("tools/toolsskybox", 0x4)];

pub fn texdata_ranks(
    texinfo: &[crate::world::RenderTexInfo],
    ids: &std::collections::HashMap<u64, u32>,
    names: &[String],
) -> Vec<i32> {
    let mut ptrs: Vec<u64> = texinfo.iter().map(|t| t.material).collect();
    ptrs.sort_unstable();
    ptrs.dedup();
    if !ids.is_empty()
        && !names.is_empty()
        && ptrs.iter().all(|p| ids.contains_key(p))
        && let Some(base) = id_base(texinfo, ids, names, &ptrs)
    {
        return texinfo
            .iter()
            .map(|t| {
                let r = i64::from(ids[&t.material]) - i64::from(base);
                if (0..names.len() as i64).contains(&r) { r as i32 } else { -1 }
            })
            .collect();
    }
    texinfo.iter().map(|t| ptrs.binary_search(&t.material).map_or(-1, |r| r as i32)).collect()
}

fn id_base(
    texinfo: &[crate::world::RenderTexInfo],
    ids: &std::collections::HashMap<u64, u32>,
    names: &[String],
    ptrs: &[u64],
) -> Option<u32> {
    let n = names.len() as i64;
    let used: Vec<i64> = ptrs.iter().map(|p| i64::from(ids[p])).collect();
    used.first()?;
    let mut flags: std::collections::HashMap<i64, i32> = std::collections::HashMap::new();
    for t in texinfo {
        let id = i64::from(ids[&t.material]);
        let e = flags.entry(id).or_insert(!0);
        *e &= t.flags;
    }
    let mut best: Option<(usize, i64, i64)> = None;
    let mut bases: Vec<i64> =
        used.iter().flat_map(|u| (0..=ID_SLACK).map(move |s| u - s)).filter(|b| *b >= 0).collect();
    bases.sort_unstable();
    bases.dedup();
    for base in bases {
        let coverage = used.iter().filter(|u| **u >= base && **u < base + n).count();
        let mut score = 0i64;
        for (idx, name) in names.iter().enumerate() {
            let lower = name.to_ascii_lowercase();
            if let Some((_, bit)) = NAME_FLAGS.iter().find(|(k, _)| *k == lower)
                && let Some(f) = flags.get(&(base + idx as i64))
            {
                score += if f & bit != 0 { 1 } else { -1 };
            }
        }
        if best.is_none_or(|(c, sc, _)| (score, coverage) > (sc, c)) {
            best = Some((coverage, score, base));
        }
    }
    let (coverage, _, base) = best?;
    (coverage * 3 >= used.len() * 2).then_some(base as u32)
}

pub fn material_ranks(world: &MemoryWorld, render: &RenderWorld, names: &[String]) -> Vec<i32> {
    if render.texinfo_ranks.len() == render.texinfo.len() && !render.texinfo.is_empty() {
        return render.texinfo_ranks.clone();
    }
    let voted = vote_material_names(world, render);
    let mut ranks = refine_ranks(&render.texinfo, &render.material_ids, names, &voted);
    if render.texinfo_file_ranks.len() == ranks.len() {
        for (r, f) in ranks.iter_mut().zip(&render.texinfo_file_ranks) {
            if *f >= 0 && (*f as usize) < names.len() {
                *r = *f;
            }
        }
    }
    tool_fixups(&render.texinfo, &mut ranks, names);
    ranks
}

const TOOL_FLAGS: [(&str, i32); 4] =
    [("tools/toolsskip", 0x200), ("tools/toolshint", 0x100), ("tools/toolsskybox", 0x4), ("tools/toolsnodraw", 0x80)];

fn tool_fixups(texinfo: &[crate::world::RenderTexInfo], ranks: &mut [i32], names: &[String]) {
    let mut all: HashMap<u64, i32> = HashMap::new();
    for t in texinfo {
        *all.entry(t.material).or_insert(!0) &= t.flags;
    }
    let mut by_flag: Vec<(i32, i32)> = Vec::new();
    for (bit_name, bit) in TOOL_FLAGS {
        if let Some(i) = names.iter().position(|n| n.eq_ignore_ascii_case(bit_name)) {
            by_flag.push((bit, i as i32));
        }
    }
    let mut ptrs: Vec<u64> = texinfo.iter().map(|t| t.material).collect();
    ptrs.sort_unstable();
    ptrs.dedup();
    for (t, r) in texinfo.iter().zip(ranks.iter_mut()) {
        if *r >= 0 {
            continue;
        }
        let f = all.get(&t.material).copied().unwrap_or(0);
        if let Some((_, idx)) = by_flag.iter().find(|(bit, _)| f & *bit != 0) {
            *r = *idx;
        } else if let Ok(at) = ptrs.binary_search(&t.material)
            && at < names.len()
        {
            *r = at as i32;
        }
    }
}

pub fn resolve_material_ranks(world: &mut MemoryWorld) {
    let names = if world.cm.cm.surface_names.is_empty() {
        vec![String::from("TOOLS/TOOLSNODRAW")]
    } else {
        world.cm.cm.surface_names.clone()
    };
    let ranks = match &world.render {
        Some(r) => material_ranks(world, r, &names),
        None => return,
    };
    if let Some(r) = world.render.as_mut() {
        r.texinfo_ranks = ranks;
    }
}

fn vote_material_names(world: &MemoryWorld, render: &RenderWorld) -> HashMap<u64, usize> {
    let mc = &world.cm;
    let cm = &mc.cm;
    let index = FaceIndex::new(render, &mc.plane_bytes, Vec::new());
    let mut votes: HashMap<u64, HashMap<usize, u32>> = HashMap::new();
    let mut axial_seen = 0usize;
    for b in &cm.brushes {
        let mut list: Vec<([f32; 3], f32, usize)> = Vec::new();
        if b.axial() {
            let Some(bx) = cm.boxes.get(axial_seen) else { break };
            axial_seen += 1;
            for (n, d, _, surface) in box_planes(bx) {
                list.push((n, d, surface));
            }
        } else {
            for k in 0..b.numsides as usize {
                let Some(side) = cm.sides.get(b.index as usize + k) else { break };
                let at = side.plane as usize * 20;
                if at + 16 > mc.plane_bytes.len() {
                    continue;
                }
                let n = [f32_at(&mc.plane_bytes, at), f32_at(&mc.plane_bytes, at + 4), f32_at(&mc.plane_bytes, at + 8)];
                list.push((n, f32_at(&mc.plane_bytes, at + 12), usize::from(side.surface)));
            }
        }
        let shape: Vec<([f32; 3], f32)> = list.iter().map(|e| (e.0, e.1)).collect();
        for (n, d, surface) in &list {
            if let Some(t) = index.best_texinfo(*n, *d, None, &shape)
                && let Some(info) = render.texinfo.get(usize::from(t))
            {
                *votes.entry(info.material).or_default().entry(*surface).or_default() += 1;
            }
        }
    }
    votes
        .into_iter()
        .filter_map(|(ptr, v)| {
            v.into_iter().max_by_key(|(idx, c)| (*c, std::cmp::Reverse(*idx))).map(|(idx, _)| (ptr, idx))
        })
        .collect()
}

fn refine_ranks(
    texinfo: &[crate::world::RenderTexInfo],
    ids: &HashMap<u64, u32>,
    names: &[String],
    voted: &HashMap<u64, usize>,
) -> Vec<i32> {
    let by_id = texdata_ranks(texinfo, ids, names);
    let by_pointer = texdata_ranks(texinfo, &HashMap::new(), names);
    if voted.is_empty() {
        return by_id;
    }
    let agreement = |ranks: &[i32]| -> (usize, usize) {
        let mut of: HashMap<u64, i32> = HashMap::new();
        for (t, r) in texinfo.iter().zip(ranks) {
            of.insert(t.material, *r);
        }
        let comparable = voted.keys().filter(|p| of.get(*p).is_some_and(|r| *r >= 0)).count();
        let agree = voted.iter().filter(|(p, idx)| of.get(*p).is_some_and(|r| *r == **idx as i32)).count();
        (agree, comparable)
    };
    let (ai, ci) = agreement(&by_id);
    let (ap, cp) = agreement(&by_pointer);
    let rate = |a: usize, c: usize| if c == 0 { 0.0 } else { a as f64 / c as f64 };
    if rate(ap, cp) >= VOTE_TRUST && rate(ap, cp) >= rate(ai, ci) {
        return by_pointer;
    }
    if rate(ai, ci) >= VOTE_TRUST {
        return by_id;
    }
    let base = by_id;
    let mut anchors: Vec<(i64, usize)> =
        voted.iter().filter_map(|(p, idx)| ids.get(p).map(|id| (i64::from(*id), *idx))).collect();
    anchors.sort_unstable();
    let mut resolved: HashMap<u64, i32> = HashMap::new();
    for t in texinfo {
        if resolved.contains_key(&t.material) {
            continue;
        }
        let rank = if let Some(idx) = voted.get(&t.material) {
            *idx as i32
        } else if let Some(id) = ids.get(&t.material).map(|i| i64::from(*i)) {
            let at = anchors.partition_point(|a| a.0 <= id);
            let guess = if at > 0 {
                let (i1, n1) = anchors[at - 1];
                let mut g = n1 as i64 + (id - i1);
                if let Some((_, n2)) = anchors.get(at) {
                    g = g.min(*n2 as i64 - 1);
                }
                g
            } else if let Some((i2, n2)) = anchors.first() {
                *n2 as i64 - (i2 - id)
            } else {
                -1
            };
            if (0..names.len() as i64).contains(&guess) { guess as i32 } else { -1 }
        } else {
            -1
        };
        resolved.insert(t.material, rank);
    }
    texinfo
        .iter()
        .zip(&base)
        .map(|(t, b)| match resolved.get(&t.material) {
            Some(r) if *r >= 0 => *r,
            _ => *b,
        })
        .collect()
}

fn with_surface(texinfo: &mut Vec<u8>, cache: &mut HashMap<(u16, usize), i16>, from: u16, surface: usize) -> i16 {
    if let Some(i) = cache.get(&(from, surface)) {
        return *i;
    }
    let at = usize::from(from) * 72;
    let i = (texinfo.len() / 72) as i16;
    let Some(record) = texinfo.get(at..at + 72).map(<[u8]>::to_vec) else { return from as i16 };
    texinfo.extend_from_slice(&record[..68]);
    texinfo.extend_from_slice(&(surface as i32).to_le_bytes());
    cache.insert((from, surface), i);
    i
}

pub fn build_bsp(world: &MemoryWorld, entity_text: &str) -> Result<Bsp, String> {
    let mc = &world.cm;
    let cm = &mc.cm;
    let mut names: Vec<String> =
        if cm.surface_names.is_empty() { vec![String::from("TOOLS/TOOLSNODRAW")] } else { cm.surface_names.clone() };
    let mut planes = PlaneTable::new(&mc.plane_bytes);
    let ranks_all: Vec<i32> = world.render.as_ref().map(|r| material_ranks(world, r, &names)).unwrap_or_default();
    let render_ranks_guessed = world.render.as_ref().is_none_or(|r| r.texinfo_file_ranks.len() != r.texinfo.len());
    let face_index = world.render.as_ref().map(|r| FaceIndex::new(r, &mc.plane_bytes, ranks_all.clone()));
    let mut texinfo: Vec<u8> = Vec::new();
    let mut texinfo_ranks: Vec<i32> = Vec::new();
    if let Some(r) = &world.render {
        let ranks = ranks_all.clone();
        texinfo_ranks = ranks.clone();
        let need = ranks.iter().copied().max().map_or(0, |m| m as usize + 1);
        while names.len() < need {
            names.push(String::from("TOOLS/TOOLSNODRAW"));
        }
        for (t, rank) in r.texinfo.iter().zip(&ranks) {
            for v in &t.vecs {
                texinfo.extend_from_slice(&v.to_le_bytes());
            }
            texinfo.extend_from_slice(&t.flags.to_le_bytes());
            texinfo.extend_from_slice(&(*rank).max(0).to_le_bytes());
        }
    }
    let mut combined: HashMap<(u16, usize), i16> = HashMap::new();
    let mut texinfo_index: HashMap<(usize, usize), i16> = HashMap::new();
    let mut texinfo_for = |texinfo: &mut Vec<u8>, surface: usize, class: usize| -> i16 {
        let surface = surface.min(names.len() - 1);
        if let Some(i) = texinfo_index.get(&(surface, class)) {
            return *i;
        }
        let i = (texinfo.len() / 72) as i16;
        let vecs = texinfo_vecs(class);
        for row in vecs {
            for v in row {
                texinfo.extend_from_slice(&v.to_le_bytes());
            }
        }
        for row in [[1.0f32 / 16.0, 0.0, 0.0, 0.0], [0.0, 1.0 / 16.0, 0.0, 0.0]] {
            for v in row {
                texinfo.extend_from_slice(&v.to_le_bytes());
            }
        }
        texinfo.extend_from_slice(&0i32.to_le_bytes());
        texinfo.extend_from_slice(&(surface as i32).to_le_bytes());
        texinfo_index.insert((surface, class), i);
        i
    };

    let mut brushes: Vec<u8> = Vec::with_capacity(cm.brushes.len() * 12);
    let mut sides: Vec<u8> = Vec::new();
    let mut axial_seen = 0usize;
    for (bi, b) in cm.brushes.iter().enumerate() {
        let first = (sides.len() / 8) as i32;
        let mut list: Vec<(u32, usize, u16, i32, [f32; 3], f32)> = Vec::new();
        if b.axial() {
            let Some(bx) = cm.boxes.get(axial_seen) else { return Err("an axis aligned brush has no box".to_owned()) };
            axial_seen += 1;
            for (n, d, kind, surface) in box_planes(bx) {
                let p = planes.find_or_add(n, d, kind);
                list.push((p, surface, 0, 6, n, d));
            }
        } else {
            for k in 0..b.numsides as usize {
                let s = cm.sides.get(b.index as usize + k).ok_or_else(|| "a brush side is missing".to_owned())?;
                let at = s.plane as usize * 20;
                let (n, d) = (
                    [f32_at(&planes.bytes, at), f32_at(&planes.bytes, at + 4), f32_at(&planes.bytes, at + 8)],
                    f32_at(&planes.bytes, at + 12),
                );
                list.push((s.plane, usize::from(s.surface), s.bevel, b.numsides, n, d));
            }
        }
        let shape: Vec<([f32; 3], f32)> = list.iter().map(|e| (e.4, e.5)).collect();
        for (plane, surface, bevel, _, n, d) in &list {
            let forced = world
                .extras
                .side_texinfo
                .as_ref()
                .and_then(|m| m.get(&(bi, plane_key(*n, *d))))
                .copied()
                .filter(|t| {
                    let rank = texinfo_ranks.get(*t as usize).copied().unwrap_or(-1);
                    let known = rank >= 0 && (rank as usize) < names.len();
                    let same = match (names.get(rank.max(0) as usize), names.get(*surface)) {
                        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                        _ => false,
                    };
                    let from_file = !render_ranks_guessed && known;
                    same || from_file
                        || names.get(*surface).is_some_and(|n| n.to_ascii_lowercase().starts_with("tools/"))
                })
                .map(|t| t as u16);
            let matched = forced
                .or_else(|| face_index.as_ref().and_then(|fi| fi.texinfo_for_side(*n, *d, *surface as i32, &shape)));
            let ti = match matched {
                Some(t) => {
                    let rank = texinfo_ranks.get(usize::from(t)).copied().unwrap_or(-1);
                    let agrees = match (names.get(rank.max(0) as usize), names.get(*surface)) {
                        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                        _ => false,
                    };
                    if agrees { t as i16 } else { with_surface(&mut texinfo, &mut combined, t, *surface) }
                }
                None => texinfo_for(&mut texinfo, *surface, texinfo_class(planes.normal(*plane))),
            };
            sides.extend_from_slice(&(*plane as u16).to_le_bytes());
            sides.extend_from_slice(&ti.to_le_bytes());
            sides.extend_from_slice(&0i16.to_le_bytes());
            sides.extend_from_slice(&(*bevel as i16).to_le_bytes());
        }
        let count = if b.axial() { 6 } else { b.numsides };
        brushes.extend_from_slice(&first.to_le_bytes());
        brushes.extend_from_slice(&count.to_le_bytes());
        brushes.extend_from_slice(&b.contents.to_le_bytes());
    }
    if planes.len() > 65_535 {
        return Err("too many planes for a BSP".to_owned());
    }

    let mut texdata = Vec::new();
    let mut string_data = Vec::new();
    let mut string_table = Vec::new();
    for (i, n) in names.iter().enumerate() {
        string_table.extend_from_slice(&(string_data.len() as u32).to_le_bytes());
        string_data.extend_from_slice(n.as_bytes());
        string_data.push(0);
        for _ in 0..3 {
            texdata.extend_from_slice(&0.5f32.to_le_bytes());
        }
        texdata.extend_from_slice(&(i as i32).to_le_bytes());
        for _ in 0..4 {
            texdata.extend_from_slice(&64i32.to_le_bytes());
        }
    }

    let mut nodes = Vec::with_capacity(mc.nodes.len() * 32);
    for n in &mc.nodes {
        nodes.extend_from_slice(&(n.plane as i32).to_le_bytes());
        nodes.extend_from_slice(&n.children[0].to_le_bytes());
        nodes.extend_from_slice(&n.children[1].to_le_bytes());
        nodes.extend_from_slice(&[0u8; 20]);
    }
    let mut leafs = Vec::with_capacity(mc.leafs.len() * 32);
    for l in &mc.leafs {
        leafs.extend_from_slice(&l.contents.to_le_bytes());
        leafs.extend_from_slice(&l.cluster.to_le_bytes());
        leafs.extend_from_slice(&l.area_flags.to_le_bytes());
        leafs.extend_from_slice(&[0u8; 12]);
        leafs.extend_from_slice(&0u16.to_le_bytes());
        leafs.extend_from_slice(&0u16.to_le_bytes());
        leafs.extend_from_slice(&l.first_brush.to_le_bytes());
        leafs.extend_from_slice(&l.num_brushes.to_le_bytes());
        leafs.extend_from_slice(&(-1i16).to_le_bytes());
        leafs.extend_from_slice(&0i16.to_le_bytes());
    }
    let mut leafbrushes = Vec::with_capacity(mc.leafbrushes.len() * 2);
    for v in &mc.leafbrushes {
        leafbrushes.extend_from_slice(&v.to_le_bytes());
    }
    let mut models = Vec::with_capacity(mc.models.len() * 48);
    for m in &mc.models {
        for v in m.mins.iter().chain(&m.maxs).chain(&m.origin) {
            models.extend_from_slice(&v.to_le_bytes());
        }
        models.extend_from_slice(&m.headnode.to_le_bytes());
        models.extend_from_slice(&0i32.to_le_bytes());
        models.extend_from_slice(&0i32.to_le_bytes());
    }
    let mut ents = entity_text.as_bytes().to_vec();
    ents.push(0);
    let mut extra: Vec<(usize, Vec<u8>)> = Vec::new();
    if let Some(r) = &world.render {
        let disp = displacement_lumps(r, &mc.plane_bytes, &world.extras.displacements);
        let (faces, edges, surfedges, vertexes) = render_lumps(r, &disp);
        extra.push((lump::FACES, faces));
        extra.push((3, vertexes));
        extra.push((12, edges));
        extra.push((13, surfedges));
        if !disp.dispinfo.is_empty() {
            extra.push((26, disp.dispinfo.clone()));
            extra.push((33, disp.dispverts.clone()));
            extra.push((48, disp.disptris.clone()));
        }
        if !world.extras.overlays.is_empty() {
            extra.push((lump::OVERLAYS, overlay_lump(&world.extras.overlays)));
        }
    }
    if let Some(ap) = &world.extras.area_portals {
        extra.push((21, ap.records.clone()));
        extra.push((41, ap.vert_bytes()));
    }
    if let Some(c) = &world.extras.cubemap_file {
        extra.push((42, crate::engine::cubemap_file_lump(c)));
    } else if !world.extras.cubemaps.is_empty() {
        extra.push((42, cubemap_lump(&world.extras.cubemaps)));
    }
    let mut lumps: Vec<(usize, Vec<u8>)> = vec![
        (lump::ENTITIES, ents),
        (1, planes.bytes),
        (lump::TEXDATA, texdata),
        (5, nodes),
        (6, texinfo),
        (lump::LEAFS, leafs),
        (14, models),
        (17, leafbrushes),
        (18, brushes),
        (19, sides),
        (lump::TEXDATA_STRING_DATA, string_data),
        (lump::TEXDATA_STRING_TABLE, string_table),
    ];
    lumps.extend(extra);
    let data = hp_bsp::synthetic_bsp(20, &lumps);
    let bsp = Bsp::from_vec(data).map_err(|e| e.to_string())?;
    let mut edit = bsp.edit();
    if !world.extras.props.is_empty() {
        let (bytes, version) = static_prop_lump(&world.extras.props);
        let mut game = hp_bsp::GameLumps::default();
        game.set(hp_bsp::gamelump::id::STATIC_PROPS, version, bytes).map_err(|e| e.to_string())?;
        edit.set_game_lumps(game);
    }
    if let Some(zip) = &world.extras.pakfile {
        edit.set_pakfile(zip.clone()).map_err(|e| e.to_string())?;
    }
    let leafs_now = edit.source().lump(lump::LEAFS).map_err(|e| e.to_string())?.into_owned();
    edit.replace_lump_with_version(lump::LEAFS, leafs_now, 1).map_err(|e| e.to_string())?;
    let bytes = edit.to_vec().map_err(|e| e.to_string())?;
    Bsp::from_vec(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn cuboid_planes() -> Vec<u8> {
        let mut p = Vec::new();
        let mut add = |n: [f32; 3], d: f32, kind: u8| {
            for v in n {
                p.extend_from_slice(&v.to_le_bytes());
            }
            p.extend_from_slice(&d.to_le_bytes());
            p.extend_from_slice(&[kind, 0, 0, 0]);
        };
        add([1.0, 0.0, 0.0], 16.0, 0);
        add([-1.0, 0.0, 0.0], 16.0, 0);
        add([0.0, 1.0, 0.0], 16.0, 1);
        add([0.0, -1.0, 0.0], 16.0, 1);
        add([0.0, 0.0, 1.0], 16.0, 2);
        add([0.0, 0.0, -1.0], 16.0, 2);
        p
    }

    fn sample() -> MemoryCm {
        let cm = CmFindings {
            brushes: vec![
                crate::cm::BrushRec { contents: 1, numsides: -1, index: 0 },
                crate::cm::BrushRec { contents: 1, numsides: 6, index: 0 },
            ],
            planes: 6,
            sides: (0..6)
                .map(|p| crate::cm::Side { plane: p, surface: if p % 2 == 0 { 0 } else { 1 }, bevel: 0 })
                .collect(),
            boxes: vec![AxialBox { mins: [100.0, 0.0, 0.0], maxs: [132.0, 32.0, 32.0], surfaces: [1, 1, 1, 1, 1, 0] }],
            surface_names: vec!["TOOLS/TOOLSNODRAW".to_owned(), "BRICK/WALL001".to_owned()],
            ..CmFindings::default()
        };
        MemoryCm {
            cm,
            plane_bytes: cuboid_planes(),
            leafs: vec![
                CLeaf { contents: 0x1, cluster: -1, area_flags: 0, first_brush: 0, num_brushes: 0 },
                CLeaf { contents: 0, cluster: 0, area_flags: 1, first_brush: 0, num_brushes: 2 },
            ],
            leafbrushes: vec![0, 1],
            models: vec![CModel { mins: [-64.0; 3], maxs: [64.0; 3], origin: [0.0; 3], headnode: 0 }],
            nodes: vec![CNode { plane: 0, children: [-2, -1] }],
            ..MemoryCm::default()
        }
    }

    #[test]
    fn leaf_chains_are_recognised() {
        let mut leafs = Vec::new();
        let mut first = 0u16;
        for i in 0..100u16 {
            let n = if i % 3 == 0 { 0 } else { (i % 5) + 1 };
            leafs.push(CLeaf {
                contents: 0,
                cluster: (i as i16) - 1,
                area_flags: 0,
                first_brush: first,
                num_brushes: n,
            });
            first += n;
        }
        assert!(leaf_window_ok(&leafs[..64]));
        assert_eq!(leaf_run(&leafs, 50), (0, 100));
        leafs[70].first_brush += 1;
        assert_eq!(leaf_run(&leafs, 10), (0, 70));
        assert_eq!(leaf_run(&leafs, 80).0, 71);
        let empty = vec![CLeaf { contents: 0, cluster: 0, area_flags: 0, first_brush: 0, num_brushes: 0 }; 64];
        assert!(!leaf_window_ok(&empty), "an all empty table is not a leaf table");
        let mut bad = leafs[..64].to_vec();
        bad[3].cluster = -9;
        assert!(!leaf_window_ok(&bad));
    }

    #[test]
    fn texinfo_classes_follow_the_dominant_axis() {
        assert_eq!(texinfo_class([0.0, 0.0, 1.0]), 0);
        assert_eq!(texinfo_class([0.0, 0.0, -1.0]), 0);
        assert_eq!(texinfo_class([1.0, 0.0, 0.0]), 1);
        assert_eq!(texinfo_class([0.0, -1.0, 0.0]), 2);
        assert_eq!(texinfo_class([0.6, 0.8, 0.0]), 2);
    }

    #[test]
    fn a_collision_set_becomes_a_decompilable_bsp() {
        let mc = sample();
        let ents = "{\n\"classname\" \"worldspawn\"\n}\n";
        let bsp = build_bsp(&MemoryWorld::from(mc.clone()), ents).expect("bsp");
        assert_eq!(bsp.lump(18).expect("brushes").len(), 24);
        assert_eq!(bsp.lump(19).expect("sides").len(), 12 * 8);
        let out = hp_decompile::decompile(&bsp, &hp_decompile::Options::default()).expect("decompile");
        assert_eq!(out.report.brushes, 2);
        assert_eq!(out.report.failed_brushes, 0, "{:?}", out.report);
        let solids = &out.map.world.solids;
        assert_eq!(solids.len(), 2);
        let boxed = solids
            .iter()
            .find(|s| s.sides.iter().any(|sd| sd.material.eq_ignore_ascii_case("brick/wall001")))
            .expect("the axis aligned brush");
        let g = hp_geom::validate_solid(&boxed.sides).expect("valid solid");
        assert_eq!(g.aabb.min, glam::DVec3::new(100.0, 0.0, 0.0));
        assert_eq!(g.aabb.max, glam::DVec3::new(132.0, 32.0, 32.0));
        let nodraw = boxed.sides.iter().filter(|s| s.material.eq_ignore_ascii_case("tools/toolsnodraw")).count();
        assert_eq!(nodraw, 1);
        let text = hp_vmf::write(&out.map);
        assert!(hp_vmf::parse(&text).is_ok());
    }

    #[test]
    fn planes_are_added_when_a_box_needs_one() {
        let mut mc = sample();
        mc.cm.boxes[0].maxs = [140.0, 32.0, 32.0];
        let bsp = build_bsp(&MemoryWorld::from(mc.clone()), "{\n\"classname\" \"worldspawn\"\n}\n").expect("bsp");
        assert!(bsp.lump(1).expect("planes").len() > 6 * 20);
        let out = hp_decompile::decompile(&bsp, &hp_decompile::Options::default()).expect("decompile");
        assert_eq!(out.report.failed_brushes, 0);
    }

    pub(crate) fn add_render_memory(mem: &mut crate::procmem::VecMem) {
        let found = find_full(&*mem, &ScanOptions::default()).expect("collision data");
        let planes_addr = found.cm.planes_addr;
        let plane_count = found.cm.planes;
        let mut verts: Vec<[f32; 3]> = vec![[0.0; 3]; 320];
        let mut indices: Vec<u16> = Vec::new();
        let mut faces: Vec<u8> = Vec::new();
        let face_count = 80usize;
        for i in 0..face_count {
            let plane = i % plane_count;
            let rec = &found.plane_bytes[plane * 20..plane * 20 + 20];
            let n = [f32_at(rec, 0), f32_at(rec, 4), f32_at(rec, 8)];
            let d = f32_at(rec, 12);
            let (u, v) = if n[0].abs() > 0.5 {
                ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0])
            } else if n[1].abs() > 0.5 {
                ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
            } else {
                ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
            };
            let base = [n[0] * d, n[1] * d, n[2] * d];
            let first = indices.len() as i32;
            for (k, (a, b)) in [(0.0f32, 0.0f32), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)].into_iter().enumerate() {
                let p = [base[0] + u[0] * a + v[0] * b, base[1] + u[1] * a + v[1] * b, base[2] + u[2] * a + v[2] * b];
                let slot = (i * 4 + k) * 7 % 320;
                indices.push(slot as u16);
                verts[slot] = p;
            }
            let mut f = vec![0u8; 40];
            f[7] = 4;
            f[8..16].copy_from_slice(&(planes_addr + plane as u64 * 20).to_le_bytes());
            f[16..20].copy_from_slice(&first.to_le_bytes());
            f[30..32].copy_from_slice(&(((i as u16 % 70) << 1) | 1).to_le_bytes());
            faces.extend(f);
        }
        let mut block: Vec<u8> = Vec::new();
        for p in &verts {
            for c in p {
                block.extend_from_slice(&c.to_le_bytes());
            }
        }
        block.extend_from_slice(&[0u8; 4]);
        for i in &indices {
            block.extend_from_slice(&i.to_le_bytes());
        }
        while !block.len().is_multiple_of(16) {
            block.push(0);
        }
        let texinfo_start = block.len();
        for t in 0..100usize {
            let mut rec = vec![0u8; 88];
            for k in 0..16 {
                rec[k * 4..k * 4 + 4].copy_from_slice(&((k as f32) * 0.25 + t as f32 * 0.001).to_le_bytes());
            }
            rec[80..88].copy_from_slice(&(0x3e0_0000u64 + (t as u64 % 32) * 0xe0).to_le_bytes());
            block.extend(rec);
        }
        while !block.len().is_multiple_of(16) {
            block.push(0);
        }
        let _ = texinfo_start;
        block.extend(faces);
        mem.add(0x3800_0000, block);
    }

    fn render_for_sample() -> RenderWorld {
        let mut w = crate::engine::tests::quad_world();
        w.faces[0].plane = 4;
        w.faces[0].disp = 0x5000_0000;
        w.vertexes = vec![[0.0, 0.0, 16.0], [0.0, 256.0, 16.0], [256.0, 256.0, 16.0], [256.0, 0.0, 16.0]];
        w.faces.push(crate::world::RenderFace {
            plane: 4,
            back: false,
            first: 0,
            count: 4,
            texinfo: 0,
            disp: 0,
            has_disp: false,
            material_sort: 0,
        });
        let mut vecs = [0f32; 16];
        vecs[0] = 4.0;
        vecs[5] = -4.0;
        vecs[8] = 0.0625;
        vecs[13] = 0.0625;
        w.texinfo = vec![crate::world::RenderTexInfo { vecs, flags: 0, material: 0x3ec_0000 }];
        w
    }

    fn extras_for_sample(render: &RenderWorld) -> MemoryExtras {
        let positions = crate::engine::tests::flat_positions(render, 2, [0.0, 0.0, 16.0])
            .into_iter()
            .enumerate()
            .map(|(i, mut p)| {
                if i == 12 {
                    p[2] += 10.0;
                }
                p
            })
            .collect();
        MemoryExtras {
            displacements: vec![crate::engine::MemDisp {
                face: 0,
                start: [0.0, 0.0, 16.0],
                power: 2,
                positions,
                file: None,
            }],
            overlays: vec![crate::engine::MemOverlay {
                id: 0,
                texinfo: 0,
                faces: vec![1],
                u: [0.0, 1.0],
                v: [0.0, 1.0],
                points: [[-8.0, -8.0, 0.0], [-8.0, 8.0, 0.0], [8.0, 8.0, 0.0], [8.0, -8.0, 0.0]],
                origin: [128.0, 128.0, 16.0],
                basis_u: [1.0, 0.0, 0.0],
                basis_v: [0.0, -1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                render_order: 0,
                from_file: false,
            }],
            overlays_expected: 1,
            props: vec![crate::engine::MemProp {
                origin: [10.0, 20.0, 30.0],
                angles: [0.0, 90.0, 0.0],
                model: "models/props_junk/gnome.mdl".to_owned(),
                alpha: 255,
                solid: 6,
                skin: 2,
                fades: false,
                first_leaf: 0,
                leaf_count: 1,
                file: None,
            }],
            props_expected: 1,
            cubemaps: vec![[64, 64, 32]],
            area_portals: None,
            ..MemoryExtras::default()
        }
    }

    #[test]
    fn the_render_structures_become_lumps_a_decompiler_understands() {
        let render = render_for_sample();
        let extras = extras_for_sample(&render);
        let world = MemoryWorld { cm: sample(), render: Some(render), extras };
        let ents = "{\n\"classname\" \"worldspawn\"\n}\n";
        let bsp = build_bsp(&world, ents).expect("bsp");
        assert_eq!(bsp.lump(7).expect("faces").len(), 2 * 56);
        assert_eq!(bsp.lump(3).expect("vertexes").len(), 4 * 12);
        assert_eq!(bsp.lump(26).expect("dispinfo").len(), 176);
        assert_eq!(bsp.lump(33).expect("dispverts").len(), 25 * 20);
        assert_eq!(bsp.lump(48).expect("disptris").len(), 32 * 2);
        assert_eq!(bsp.lump(45).expect("overlays").len(), 352);
        assert_eq!(bsp.lump(42).expect("cubemaps").len(), 16);
        let faces = bsp.lump(7).expect("faces").into_owned();
        assert_eq!(i16::from_le_bytes([faces[12], faces[13]]), 0, "the first face carries the displacement");
        assert_eq!(i16::from_le_bytes([faces[56 + 12], faces[56 + 13]]), -1);
        let out = hp_decompile::decompile(&bsp, &hp_decompile::Options::default()).expect("decompile");
        assert_eq!(out.report.displacements, 1, "{:?}", out.report);
        assert_eq!(out.report.static_props, 1);
        assert_eq!(out.report.overlays, 1);
        assert_eq!(out.report.cubemaps, 1);
        assert_eq!(out.report.failed_displacements, 0);
        let disp_side = out
            .map
            .world
            .solids
            .iter()
            .flat_map(|s| s.sides.iter())
            .find(|s| s.disp.is_some())
            .expect("displacement side");
        let d = disp_side.disp.as_ref().expect("disp");
        assert_eq!(d.power, 2);
        assert!((d.distances[12] - 10.0).abs() < 1e-3);
        assert!(d.distances.iter().enumerate().all(|(i, v)| i == 12 || v.abs() < 1e-6));
        let prop = out.map.entities.iter().find(|e| e.classname == "prop_static").expect("prop");
        assert_eq!(prop.get("model"), Some("models/props_junk/gnome.mdl"));
        assert_eq!(prop.get("skin"), Some("2"));
        assert_eq!(prop.get("origin"), Some("10 20 30"));
        let overlay = out.map.entities.iter().find(|e| e.classname == "info_overlay").expect("overlay");
        assert_eq!(overlay.get("origin"), Some("128 128 16"));
        assert!(overlay.get("sides").is_some_and(|s| !s.trim().is_empty()), "the overlay is attached to a side");
        let cube = out.map.entities.iter().find(|e| e.classname == "env_cubemap").expect("cubemap");
        assert_eq!(cube.get("origin"), Some("64 64 32"));
        let text = hp_vmf::write(&out.map);
        assert!(hp_vmf::parse(&text).is_ok());
    }

    #[test]
    fn a_displacement_vector_that_is_not_a_unit_vector_keeps_its_real_distance() {
        let render = render_for_sample();
        let extras = extras_for_sample(&render);
        let world = MemoryWorld { cm: sample(), render: Some(render), extras };
        let bsp = build_bsp(
            &world,
            "{
\"classname\" \"worldspawn\"
}
",
        )
        .expect("bsp");
        let mut verts = bsp.lump(33).expect("dispverts").into_owned();
        let at = 12 * 20;
        let (x, y, z, dist) = (0.0f32, 0.0f32, 2.0f32, 3.0f32);
        for (k, v) in [x, y, z, dist, 0.0].iter().enumerate() {
            verts[at + k * 4..at + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        let mut edit = bsp.edit();
        edit.replace_lump(33, verts).expect("replace");
        let patched = Bsp::from_vec(edit.to_vec().expect("bytes")).expect("bsp");
        let out = hp_decompile::decompile(&patched, &hp_decompile::Options::default()).expect("decompile");
        let d = out
            .map
            .world
            .solids
            .iter()
            .flat_map(|s| s.sides.iter())
            .find_map(|s| s.disp.as_ref())
            .expect("displacement");
        assert!((d.normals[12].length() - 1.0).abs() < 1e-9, "the direction is a unit vector");
        assert!((d.distances[12] - 6.0).abs() < 1e-6, "the length of the stored vector scales the distance");
    }

    #[test]
    fn faces_without_a_displacement_stay_plain_and_texture_axes_come_from_the_texinfo() {
        let mut render = render_for_sample();
        render.faces[0].disp = 0;
        render.faces[0].has_disp = false;
        let world = MemoryWorld { cm: sample(), render: Some(render), extras: MemoryExtras::default() };
        let bsp = build_bsp(&world, "{\n\"classname\" \"worldspawn\"\n}\n").expect("bsp");
        assert!(bsp.lump(26).map(|l| l.is_empty()).unwrap_or(true));
        let texinfo = bsp.lump(6).expect("texinfo").into_owned();
        assert_eq!(f32::from_le_bytes([texinfo[0], texinfo[1], texinfo[2], texinfo[3]]), 4.0);
        let out = hp_decompile::decompile(&bsp, &hp_decompile::Options::default()).expect("decompile");
        assert_eq!(out.report.displacements, 0);
    }

    #[test]
    fn the_clipped_area_of_a_face_decides_which_side_gets_its_texture() {
        let square = vec![[0.0, 0.0, 16.0], [0.0, 64.0, 16.0], [64.0, 64.0, 16.0], [64.0, 0.0, 16.0]];
        let brush: Vec<([f32; 3], f32)> = vec![
            ([0.0, 0.0, 1.0], 16.0),
            ([0.0, 0.0, -1.0], 0.0),
            ([1.0, 0.0, 0.0], 32.0),
            ([-1.0, 0.0, 0.0], 0.0),
            ([0.0, 1.0, 0.0], 64.0),
            ([0.0, -1.0, 0.0], 0.0),
        ];
        let area = clipped_area(square.clone(), &brush, [0.0, 0.0, 1.0], 16.0);
        assert!((area - 32.0 * 64.0).abs() < 1.0, "{area}");
        let far = vec![[200.0, 0.0, 16.0], [200.0, 64.0, 16.0], [264.0, 64.0, 16.0], [264.0, 0.0, 16.0]];
        assert_eq!(clipped_area(far, &brush, [0.0, 0.0, 1.0], 16.0), 0.0);
        assert!((polygon_area(&square) - 4096.0).abs() < 1e-6);
        assert!(polygon_area(&square[..2]).abs() < 1e-9);
    }

    fn consistent_file(world: &MemoryWorld) -> Bsp {
        let base = build_bsp(world, "{\n\"classname\" \"worldspawn\"\n}\n").expect("bsp");
        let mut verts = base.lump(33).expect("dispverts").into_owned();
        verts[12 * 20 + 16..12 * 20 + 20].copy_from_slice(&77.0f32.to_le_bytes());
        let mut infos = base.lump(26).expect("dispinfo").into_owned();
        infos[136..140].copy_from_slice(&0xffff_fff0u32.to_le_bytes());
        let mut cubes = Vec::new();
        for v in [64i32, 64, 32, 8, 64, 64, 32, 6] {
            cubes.extend_from_slice(&v.to_le_bytes());
        }
        let mut overlays = base.lump(45).expect("overlays").into_owned();
        overlays[6..8].copy_from_slice(&(1u16 | (2 << 14)).to_le_bytes());
        let mut props = hp_bsp::staticprops::StaticPropLump::default();
        props.names.push("models/props_junk/gnome.mdl".to_owned());
        props.props.push(hp_bsp::staticprops::StaticPropRecord {
            origin: [10.0, 20.0, 30.0],
            angles: [0.0, 90.0, 0.0],
            solid: 0,
            flags: 0x118,
            skin: 2,
            fade_min: 1500.0,
            fade_max: 1600.0,
            ..hp_bsp::staticprops::StaticPropRecord::default()
        });
        let mut game = hp_bsp::GameLumps::default();
        game.set(hp_bsp::gamelump::id::STATIC_PROPS, 10, props.to_bytes(hp_bsp::staticprops::StaticPropFormat::V10))
            .expect("game lump");
        let mut pak = hp_bsp::PakWriter::new();
        pak.add("materials/custom/wall.vmt", b"LightmappedGeneric {}".to_vec()).expect("add");
        let zip = pak.finish(hp_bsp::PakOptions::default()).expect("zip");
        let mut edit = base.edit();
        edit.replace_lump(33, verts).expect("verts");
        edit.replace_lump(26, infos).expect("infos");
        edit.replace_lump(42, cubes).expect("cubes");
        edit.replace_lump(45, overlays).expect("overlays");
        edit.set_game_lumps(game);
        edit.set_pakfile(zip).expect("pak");
        Bsp::from_vec(edit.to_vec().expect("bytes")).expect("file")
    }

    fn memory_world_for_merge() -> MemoryWorld {
        let render = render_for_sample();
        let extras = extras_for_sample(&render);
        MemoryWorld { cm: sample(), render: Some(render), extras }
    }

    #[test]
    fn a_consistent_file_fills_in_what_the_memory_does_not_keep() {
        let mut world = memory_world_for_merge();
        let file = consistent_file(&world);
        let stats = crate::merge::merge(&mut world, &file);
        assert!(stats.file_used);
        assert_eq!((stats.displacements, stats.displacements_from_file), (1, 1));
        assert_eq!((stats.props, stats.props_from_file), (1, 1));
        assert_eq!((stats.cubemaps, stats.cubemaps_from_file), (1, 2));
        assert_eq!((stats.overlays, stats.overlays_from_file), (1, 1));
        assert_eq!(stats.pak_files, 1);
        let out = build_bsp(&world, "{\n\"classname\" \"worldspawn\"\n}\n").expect("bsp");
        let verts = out.lump(33).expect("dispverts").into_owned();
        assert_eq!(
            f32::from_le_bytes([verts[12 * 20 + 16], verts[12 * 20 + 17], verts[12 * 20 + 18], verts[12 * 20 + 19]]),
            77.0
        );
        let infos = out.lump(26).expect("dispinfo").into_owned();
        assert_eq!(u32::from_le_bytes([infos[136], infos[137], infos[138], infos[139]]), 0xffff_fff0);
        let cubes = out.lump(42).expect("cubemaps").into_owned();
        assert_eq!(cubes.len(), 32);
        assert_eq!(i32::from_le_bytes([cubes[12], cubes[13], cubes[14], cubes[15]]), 8);
        let overlays = out.lump(45).expect("overlays").into_owned();
        assert_eq!(u16::from_le_bytes([overlays[6], overlays[7]]) >> 14, 2);
        assert_eq!(out.pakfile().expect("pak").entries().len(), 1);
        let decompiled = hp_decompile::decompile(&out, &hp_decompile::Options::default()).expect("decompile");
        let prop = decompiled.map.entities.iter().find(|e| e.classname == "prop_static").expect("prop");
        assert_eq!(prop.get("fademindist"), Some("1500"));
        assert_eq!(prop.get("fademaxdist"), Some("1600"));
        assert_eq!(prop.get("disableshadows"), Some("1"));
        assert_eq!(prop.get("solid"), Some("0"));
        let disp = decompiled
            .map
            .world
            .solids
            .iter()
            .flat_map(|s| s.sides.iter())
            .find_map(|s| s.disp.as_ref())
            .expect("displacement");
        assert_eq!(disp.alphas[12], 77.0);
        let rows = crate::merge::source_rows(&stats, true, 0, 0);
        let find = |c: &str| rows.iter().find(|r| r.category == c).expect("row").source.clone();
        assert_eq!(find("displacement alphas, tags and allowed vertices"), "map file");
        assert_eq!(find("static props fade distances and flags"), "map file");
        assert_eq!(find("pakfile"), "map file");
        assert_eq!(find("entities"), "memory");
    }

    #[test]
    fn a_file_that_does_not_match_changes_nothing() {
        let mut world = memory_world_for_merge();
        let good = consistent_file(&world);
        let mut infos = good.lump(26).expect("dispinfo").into_owned();
        let mut edit = good.edit();
        infos[0..4].copy_from_slice(&900.0f32.to_le_bytes());
        edit.replace_lump(26, infos).expect("infos");
        let mut props = hp_bsp::staticprops::StaticPropLump::default();
        props.names.push("models/other/thing.mdl".to_owned());
        props.props.push(hp_bsp::staticprops::StaticPropRecord {
            origin: [1.0, 2.0, 3.0],
            fade_min: 9.0,
            ..hp_bsp::staticprops::StaticPropRecord::default()
        });
        let mut game = hp_bsp::GameLumps::default();
        game.set(hp_bsp::gamelump::id::STATIC_PROPS, 10, props.to_bytes(hp_bsp::staticprops::StaticPropFormat::V10))
            .expect("game lump");
        edit.set_game_lumps(game);
        let mut cubes = Vec::new();
        for v in [1i32, 2, 3, 8] {
            cubes.extend_from_slice(&v.to_le_bytes());
        }
        edit.replace_lump(42, cubes).expect("cubes");
        edit.replace_lump(45, vec![0u8; 10]).expect("overlays");
        edit.replace_lump(19, vec![1u8; 17]).expect("sides");
        edit.replace_lump(hp_bsp::lump::PAKFILE, b"not a zip file at all".to_vec()).expect("pak");
        let bad = Bsp::from_vec(edit.to_vec().expect("bytes")).expect("file");
        let before = world.extras.clone();
        let stats = crate::merge::merge(&mut world, &bad);
        assert_eq!(stats.displacements_from_file, 0);
        assert_eq!(stats.props_from_file, 0);
        assert_eq!(stats.cubemaps_from_file, 0);
        assert_eq!(stats.overlays_from_file, 0);
        assert_eq!(stats.pak_files, 0);
        assert_eq!(world.extras, before, "nothing of a file that does not match is used");
        let out = build_bsp(&world, "{\n\"classname\" \"worldspawn\"\n}\n").expect("bsp");
        let reference = build_bsp(&memory_world_for_merge(), "{\n\"classname\" \"worldspawn\"\n}\n").expect("bsp");
        assert_eq!(out.bytes(), reference.bytes(), "the result is the memory only result");
        let rows = crate::merge::source_rows(&stats, true, 0, 0);
        assert!(rows.iter().any(|r| r.category == "pakfile" && r.source == "absent"));
        assert!(rows.iter().any(|r| r.category == "cubemap sizes" && r.source == "absent"));
    }

    #[test]
    fn a_displacement_that_disagrees_with_the_file_is_kept_from_the_memory() {
        let mut world = memory_world_for_merge();
        let file = consistent_file(&world);
        let mut verts = file.lump(33).expect("dispverts").into_owned();
        let mut edit = file.edit();
        verts[12 * 20 + 12..12 * 20 + 16].copy_from_slice(&99.0f32.to_le_bytes());
        edit.replace_lump(33, verts).expect("verts");
        let other = Bsp::from_vec(edit.to_vec().expect("bytes")).expect("file");
        let stats = crate::merge::merge(&mut world, &other);
        assert_eq!(stats.displacements_from_file, 1, "the record matched by position");
        let lumps = crate::engine::displacement_lumps(
            world.render.as_ref().expect("render"),
            &world.cm.plane_bytes,
            &world.extras.displacements,
        );
        assert_eq!(lumps.file_rejected, 1, "but its distances contradict the memory");
        let dist = f32::from_le_bytes([
            lumps.dispverts[12 * 20 + 12],
            lumps.dispverts[12 * 20 + 13],
            lumps.dispverts[12 * 20 + 14],
            lumps.dispverts[12 * 20 + 15],
        ]);
        assert!((dist - 10.0).abs() < 1e-3, "{dist}");
        assert_eq!(
            f32::from_le_bytes([
                lumps.dispverts[12 * 20 + 16],
                lumps.dispverts[12 * 20 + 17],
                lumps.dispverts[12 * 20 + 18],
                lumps.dispverts[12 * 20 + 19]
            ]),
            0.0
        );
    }

    #[test]
    fn without_a_file_the_sources_say_absent() {
        let stats = crate::merge::MergeStats::default();
        let rows = crate::merge::source_rows(&stats, false, 12, 3);
        let find = |c: &str| rows.iter().find(|r| r.category == c).expect("row").clone();
        assert_eq!(find("displacement alphas, tags and allowed vertices").source, "absent");
        assert_eq!(find("static props fade distances and flags").source, "absent");
        assert_eq!(find("overlay render order").source, "absent");
        assert_eq!(find("pakfile").source, "absent");
        assert!(find("pakfile").detail.contains("forbidden"));
        assert_eq!(find("custom and stock content").source, "game content");
        assert_eq!(find("entities").source, "memory");
    }

    #[test]
    fn a_missing_box_is_an_error() {
        let mut mc = sample();
        mc.cm.boxes.clear();
        assert!(build_bsp(&MemoryWorld::from(mc.clone()), "{\n}\n").is_err());
    }

    fn align(data: &mut Vec<u8>, base: u64) {
        while !(base + data.len() as u64).is_multiple_of(16) {
            data.push(0);
        }
    }

    pub(crate) fn world_memory() -> (crate::procmem::VecMem, usize, usize) {
        let base: u64 = 0x3000_0000;
        let mut data: Vec<u8> = vec![0xcd; 32];
        align(&mut data, base);
        let surface_count = 32usize;
        let strings_addr = base + data.len() as u64 + (surface_count * 16) as u64;
        let mut strings: Vec<u8> = Vec::new();
        for i in 0..surface_count {
            let name = match i {
                0 => "TOOLS/TOOLSNODRAW".to_owned(),
                1 => "BRICK/WALL001".to_owned(),
                n => format!("FILLER/MATERIAL{n:02}"),
            };
            data.extend_from_slice(&((strings_addr + strings.len() as u64) as u32).to_le_bytes());
            data.extend_from_slice(&[0u8; 12]);
            strings.extend_from_slice(name.as_bytes());
            strings.push(0);
        }
        data.extend_from_slice(&strings);
        align(&mut data, base);
        let leaf_count = 90usize;
        let mut leafs = Vec::new();
        let mut first = 0u16;
        let mut lb: Vec<u16> = Vec::new();
        for i in 0..leaf_count {
            let n: u16 = if i % 4 == 0 { 0 } else { 2 };
            leafs.extend_from_slice(&1i32.to_le_bytes());
            leafs.extend_from_slice(&(i as i16 - 1).to_le_bytes());
            leafs.extend_from_slice(&0u16.to_le_bytes());
            leafs.extend_from_slice(&first.to_le_bytes());
            leafs.extend_from_slice(&n.to_le_bytes());
            leafs.extend_from_slice(&0u32.to_le_bytes());
            for k in 0..n {
                lb.push((first + k) % 100);
            }
            first += n;
        }
        data.extend_from_slice(&leafs);
        let used = lb.len();
        let planes_n = 120usize;
        let planes_start = {
            let end = base + data.len() as u64 + (used * 2) as u64;
            end.div_ceil(16) * 16
        };
        while base + (data.len() as u64) < planes_start - (used * 2) as u64 {
            data.push(0);
        }
        for v in &lb {
            data.extend_from_slice(&v.to_le_bytes());
        }
        align(&mut data, base);
        let planes_addr = base + data.len() as u64;
        for i in 0..planes_n {
            let d = (i / 6) as f32 * 8.0 + 8.0;
            let (n, kind): ([f32; 3], u8) = match i % 6 {
                0 => ([1.0, 0.0, 0.0], 0),
                1 => ([-1.0, 0.0, 0.0], 0),
                2 => ([0.0, 1.0, 0.0], 1),
                3 => ([0.0, -1.0, 0.0], 1),
                4 => ([0.0, 0.0, 1.0], 2),
                _ => ([0.0, 0.0, -1.0], 2),
            };
            for v in n {
                data.extend_from_slice(&v.to_le_bytes());
            }
            data.extend_from_slice(&d.to_le_bytes());
            data.extend_from_slice(&[kind, 0, 0, 0]);
        }
        let brush_count = 80usize;
        let mut axial = 0i32;
        let mut solid_first = 0i32;
        let mut sides: Vec<(u32, u16)> = Vec::new();
        for i in 0..brush_count {
            if i % 2 == 0 {
                data.extend_from_slice(&1i32.to_le_bytes());
                data.extend_from_slice(&(-1i32).to_le_bytes());
                data.extend_from_slice(&axial.to_le_bytes());
                axial += 1;
            } else {
                data.extend_from_slice(&1i32.to_le_bytes());
                data.extend_from_slice(&6i32.to_le_bytes());
                data.extend_from_slice(&solid_first.to_le_bytes());
                solid_first += 6;
                for k in 0..6u32 {
                    sides.push((((i as u32 / 2) * 6 + k) % planes_n as u32, (k % 2) as u16));
                }
            }
        }
        align(&mut data, base);
        for (plane, surf) in &sides {
            data.extend_from_slice(&(planes_addr + u64::from(*plane) * 20).to_le_bytes());
            data.extend_from_slice(&surf.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
        }
        align(&mut data, base);
        for k in 0..axial {
            let lo = [k as f32 * 64.0, 0.0, 0.0];
            let hi = [k as f32 * 64.0 + 32.0, 32.0, 32.0];
            for v in lo {
                data.extend_from_slice(&v.to_le_bytes());
            }
            data.extend_from_slice(&0f32.to_le_bytes());
            for v in hi {
                data.extend_from_slice(&v.to_le_bytes());
            }
            data.extend_from_slice(&0f32.to_le_bytes());
            for s in 0..6u16 {
                data.extend_from_slice(&(s % 2).to_le_bytes());
            }
            data.extend_from_slice(&[0u8; 4]);
        }
        align(&mut data, base);
        let models = 3usize;
        for m in 0..models {
            for v in [-512.0f32, -512.0, -512.0, 512.0, 512.0, 512.0, 0.0, 0.0, 0.0] {
                data.extend_from_slice(&v.to_le_bytes());
            }
            data.extend_from_slice(&(m as i32).to_le_bytes());
            data.extend_from_slice(&[0u8; 32]);
        }
        align(&mut data, base);
        let nodes_n = 70usize;
        for i in 0..nodes_n {
            data.extend_from_slice(&(planes_addr + (i as u64 % planes_n as u64) * 20).to_le_bytes());
            let c0 = if i + 1 < nodes_n { i as i32 + 1 } else { -2 };
            data.extend_from_slice(&c0.to_le_bytes());
            data.extend_from_slice(&(-1i32).to_le_bytes());
        }
        let mut mem = crate::procmem::VecMem::default();
        mem.add(base, data);
        (mem, brush_count, axial as usize)
    }

    #[test]
    fn the_render_world_is_found_next_to_the_collision_world() {
        let (mut mem, _, _) = world_memory();
        add_render_memory(&mut mem);
        let world = find_all(&mem, &ScanOptions::default(), "gm_x").expect("world");
        let r = world.render.expect("render world");
        assert_eq!(r.faces.len(), 80);
        assert_eq!(r.texinfo.len(), 100);
        assert_eq!(r.vertindices.len(), 320);
        assert_eq!(r.vertexes.len(), 320);
        assert!(r.faces.iter().all(|f| !f.has_disp && f.disp == 0));
        let bsp = build_bsp(
            &MemoryWorld { cm: world.cm, render: Some(r), extras: world.extras },
            "{
}
",
        )
        .expect("bsp");
        assert_eq!(bsp.lump(7).expect("faces").len(), 80 * 56);
        assert_eq!(bsp.lump(6).expect("texinfo").len() % 72, 0);
    }

    #[test]
    fn the_whole_collision_world_is_found_and_rebuilt() {
        let (mem, brushes, axial) = world_memory();
        let mc = find_full(&mem, &ScanOptions::default()).expect("found");
        assert_eq!(mc.cm.brushes.len(), brushes);
        assert_eq!(mc.cm.axial_count(), axial);
        assert_eq!(mc.leafs.len(), 90);
        assert_eq!(mc.leafbrushes.len(), 90 * 2 - 2 * 23);
        assert_eq!(mc.nodes.len(), 70);
        assert_eq!(mc.models.len(), 3);
        assert_eq!(mc.models[2].headnode, 2);
        assert_eq!(mc.cm.surface_names.len(), 32);
        assert_eq!(mc.cm.surface_names[1], "BRICK/WALL001");
        let bsp = build_bsp(
            &MemoryWorld::from(mc.clone()),
            "{
\"classname\" \"worldspawn\"
}
",
        )
        .expect("bsp");
        assert_eq!(bsp.lump(18).expect("brushes").len(), brushes * 12);
        assert_eq!(bsp.lump(19).expect("sides").len(), (axial * 6 + (brushes - axial) * 6) * 8);
    }

    #[test]
    fn material_ranks_follow_the_creation_ids_and_tool_flags() {
        let info = |material: u64, flags: i32| crate::world::RenderTexInfo { vecs: [0.0; 16], flags, material };
        let texinfo = vec![info(0x1000, 0), info(0x2000, 0x80), info(0x3000, 0x4), info(0x4000, 0), info(0x4000, 0)];
        let ids: std::collections::HashMap<u64, u32> =
            [(0x1000, 8u32), (0x2000, 11), (0x3000, 12), (0x4000, 13)].into_iter().collect();
        let names: Vec<String> =
            ["wall", "TOOLS/TOOLSNODRAW", "TOOLS/TOOLSSKYBOX", "other"].iter().map(|n| (*n).to_owned()).collect();
        assert_eq!(texdata_ranks(&texinfo, &ids, &names), vec![-1, 1, 2, 3, 3]);
        let plain = texdata_ranks(&texinfo, &std::collections::HashMap::new(), &names);
        assert_eq!(plain, vec![0, 1, 2, 3, 3], "without ids the order of the pointers is used");
    }
}
