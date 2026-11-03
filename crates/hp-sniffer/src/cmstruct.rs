use crate::cm::{AxialBox, BRUSH_RECORD, BrushRec, CmFindings, PLANE_RECORD, Side, parse_brushes, read_boxes, valid_plane};
use crate::hub::{Array, find_pointer_hits};
use crate::memmap::{CModel, CNode, MemoryCm};
use crate::procmem::MemSource;

pub const LEAF_RECORD: usize = 16;
const BACK_REACH: usize = 0x400;
const MAX_COUNT: i64 = 20_000_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CmStruct {
    pub base: u64,
    pub ptr: usize,
    pub brushsides: Array,
    pub boxes: Array,
    pub planes: Array,
    pub nodes: Array,
    pub leafs: Array,
    pub empty_leaf: i32,
    pub solid_leaf: i32,
    pub leafbrushes: Array,
    pub cmodels: Array,
    pub brushes: Array,
    pub displist: Array,
    pub vis: Array,
}

fn align(off: usize, to: usize) -> usize {
    off.div_ceil(to) * to
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn ptr_at(d: &[u8], at: usize, ptr: usize) -> u64 {
    if ptr == 8 {
        u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
    } else {
        u64::from(u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]))
    }
}

pub fn side_record(ptr: usize) -> usize {
    align(ptr + 4, ptr)
}

pub fn node_record(ptr: usize) -> usize {
    ptr + 8
}

pub fn pair_stride(ptr: usize) -> usize {
    align(4, ptr) + ptr
}

pub fn parse_struct<S: MemSource + ?Sized>(src: &S, planes_ptr_at: u64, ptr: usize) -> Option<CmStruct> {
    let stride = pair_stride(ptr);
    let count_to_ptr = align(4, ptr);
    let base = planes_ptr_at.checked_sub((2 * stride + count_to_ptr) as u64)?;
    let total = 2 * stride + 4 * stride + 8 + 6 * stride + 64;
    let raw = src.read_vec(base, total);
    if raw.len() < total {
        return None;
    }
    let pair = |off: usize| -> Array { Array { count: i64::from(i32_at(&raw, off)), addr: ptr_at(&raw, off + count_to_ptr, ptr) } };
    let mut off = 0usize;
    let next = |off: &mut usize| {
        let p = pair(*off);
        *off += stride;
        p
    };
    let brushsides = next(&mut off);
    let boxes = next(&mut off);
    let planes = next(&mut off);
    let nodes = next(&mut off);
    let leafs = next(&mut off);
    let empty_leaf = i32_at(&raw, off);
    let solid_leaf = i32_at(&raw, off + 4);
    off += 8;
    let leafbrushes = next(&mut off);
    let cmodels = next(&mut off);
    let brushes = next(&mut off);
    let displist = next(&mut off);
    let vis = next(&mut off);
    Some(CmStruct {
        base,
        ptr,
        brushsides,
        boxes,
        planes,
        nodes,
        leafs,
        empty_leaf,
        solid_leaf,
        leafbrushes,
        cmodels,
        brushes,
        displist,
        vis,
    })
}

fn sane(cs: &CmStruct) -> bool {
    let counts = [&cs.brushsides, &cs.boxes, &cs.planes, &cs.nodes, &cs.leafs, &cs.leafbrushes, &cs.cmodels, &cs.brushes];
    counts.iter().all(|a| (0..=MAX_COUNT).contains(&a.count))
        && cs.planes.count > 0
        && cs.nodes.count > 0
        && cs.leafs.count > 1
        && cs.cmodels.count > 0
        && cs.brushes.count > 0
        && i64::from(cs.empty_leaf) == cs.leafs.count - 1
}

pub fn find_by_planes<S: MemSource + ?Sized>(src: &S, planes_addr: u64, planes: usize, widths: &[usize]) -> Option<CmStruct> {
    for &ptr in widths {
        for hit in find_pointer_hits(src, planes_addr, ptr) {
            let Some(cs) = parse_struct(src, hit, ptr) else { continue };
            if cs.planes.addr == planes_addr && cs.planes.count == planes as i64 && sane(&cs) {
                return Some(cs);
            }
        }
    }
    None
}

pub fn plane_run<S: MemSource + ?Sized>(src: &S, addr: u64, count: usize) -> bool {
    if count == 0 || count > MAX_COUNT as usize {
        return false;
    }
    let probe = count.min(256);
    let raw = src.read_vec(addr, probe * PLANE_RECORD);
    raw.len() == probe * PLANE_RECORD && raw.chunks_exact(PLANE_RECORD).all(valid_plane)
}

fn read_back<S: MemSource + ?Sized>(src: &S, at: u64, reach: usize) -> Option<(u64, Vec<u8>)> {
    let mut len = reach;
    while len >= 16 {
        let from = at.checked_sub(len as u64)?;
        let v = src.read_vec(from, len);
        if v.len() == len {
            return Some((from, v));
        }
        len -= 16;
    }
    None
}

pub fn find_by_entities<S: MemSource + ?Sized>(
    src: &S,
    entities: &[(u64, usize)],
    widths: &[usize],
) -> Option<CmStruct> {
    for &ptr in widths {
        let count_to_ptr = align(4, ptr);
        for &(addr, len) in entities {
            for hit in find_pointer_hits(src, addr, ptr) {
                let Some((back, window)) = read_back(src, hit, BACK_REACH) else { continue };
                let has_len = (0..window.len().saturating_sub(4)).step_by(4).any(|o| (i32_at(&window, o) as usize).abs_diff(len) <= 1);
                if !has_len {
                    continue;
                }
                let mut at = 0usize;
                while at + count_to_ptr + ptr <= window.len() {
                    let count = i32_at(&window, at);
                    let p = ptr_at(&window, at + count_to_ptr, ptr);
                    if count > 0 && p > 0x10000 && plane_run(src, p, count as usize) {
                        let planes_ptr_at = back + (at + count_to_ptr) as u64;
                        if let Some(cs) = parse_struct(src, planes_ptr_at, ptr)
                            && cs.planes.addr == p
                            && sane(&cs)
                        {
                            return Some(cs);
                        }
                    }
                    at += 4;
                }
            }
        }
    }
    None
}

fn read_exact<S: MemSource + ?Sized>(src: &S, addr: u64, len: usize) -> Option<Vec<u8>> {
    if len == 0 {
        return Some(Vec::new());
    }
    let v = src.read_vec(addr, len);
    (v.len() == len).then_some(v)
}

pub fn read_sides_n<S: MemSource + ?Sized>(
    src: &S,
    addr: u64,
    count: usize,
    planes_base: u64,
    planes: usize,
    ptr: usize,
) -> Option<Vec<Side>> {
    let rec = side_record(ptr);
    let data = read_exact(src, addr, count * rec)?;
    let mut out = Vec::with_capacity(count);
    for r in data.chunks_exact(rec) {
        let p = ptr_at(r, 0, ptr);
        if p < planes_base {
            return None;
        }
        let off = p - planes_base;
        if !off.is_multiple_of(PLANE_RECORD as u64) || off / PLANE_RECORD as u64 >= planes as u64 {
            return None;
        }
        out.push(Side {
            plane: (off / PLANE_RECORD as u64) as u32,
            surface: u16::from_le_bytes([r[ptr], r[ptr + 1]]),
            bevel: u16::from_le_bytes([r[ptr + 2], r[ptr + 3]]),
        });
    }
    Some(out)
}

pub fn parse_nodes_n(data: &[u8], planes_base: u64, planes: usize, ptr: usize) -> Option<Vec<CNode>> {
    let rec = node_record(ptr);
    let mut out = Vec::with_capacity(data.len() / rec);
    for r in data.chunks_exact(rec) {
        let p = ptr_at(r, 0, ptr);
        if p < planes_base {
            return None;
        }
        let off = p - planes_base;
        if !off.is_multiple_of(PLANE_RECORD as u64) || off / PLANE_RECORD as u64 >= planes as u64 {
            return None;
        }
        out.push(CNode { plane: (off / PLANE_RECORD as u64) as u32, children: [i32_at(r, ptr), i32_at(r, ptr + 4)] });
    }
    Some(out)
}

pub fn parse_models_n(data: &[u8], stride: usize) -> Vec<CModel> {
    let f = |d: &[u8], at: usize| f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);
    data.chunks_exact(stride)
        .map(|c| CModel {
            mins: [f(c, 0), f(c, 4), f(c, 8)],
            maxs: [f(c, 12), f(c, 16), f(c, 20)],
            origin: [f(c, 24), f(c, 28), f(c, 32)],
            headnode: i32_at(c, 36),
        })
        .collect()
}

pub fn read_memory_cm<S: MemSource + ?Sized>(src: &S, cs: &CmStruct, model_stride: usize) -> Option<MemoryCm> {
    let ptr = cs.ptr;
    let planes = cs.planes.count as usize;
    let plane_bytes = read_exact(src, cs.planes.addr, planes * PLANE_RECORD)?;
    if !plane_bytes.chunks_exact(PLANE_RECORD).all(valid_plane) {
        return None;
    }
    let brush_raw = read_exact(src, cs.brushes.addr, cs.brushes.count as usize * BRUSH_RECORD)?;
    let brushes: Vec<BrushRec> = parse_brushes(&brush_raw);
    let sides = read_sides_n(src, cs.brushsides.addr, cs.brushsides.count as usize, cs.planes.addr, planes, ptr)?;
    let boxes: Vec<AxialBox> = read_boxes(src, cs.boxes.addr, cs.boxes.count as usize)?;
    let leaf_raw = read_exact(src, cs.leafs.addr, cs.leafs.count as usize * LEAF_RECORD)?;
    let mut leafs = crate::memmap::parse_leafs(&leaf_raw);
    leafs.truncate(cs.leafs.count as usize - 1);
    let lb_raw = read_exact(src, cs.leafbrushes.addr, cs.leafbrushes.count as usize * 2)?;
    let leafbrushes = lb_raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let node_raw = read_exact(src, cs.nodes.addr, cs.nodes.count as usize * node_record(ptr))?;
    let nodes = parse_nodes_n(&node_raw, cs.planes.addr, planes, ptr)?;
    let model_raw = read_exact(src, cs.cmodels.addr, cs.cmodels.count as usize * model_stride)?;
    let models = parse_models_n(&model_raw, model_stride);
    if models.is_empty() || nodes.is_empty() || leafs.is_empty() {
        return None;
    }
    let cm = CmFindings {
        brush_addr: cs.brushes.addr,
        brushes,
        planes_addr: cs.planes.addr,
        planes,
        sides_addr: cs.brushsides.addr,
        sides,
        boxes_addr: cs.boxes.addr,
        boxes,
        surface_addr: None,
        surface_names: Vec::new(),
    };
    Some(MemoryCm {
        cm,
        plane_bytes,
        leafs_addr: cs.leafs.addr,
        leafs,
        leafbrushes,
        models_addr: cs.cmodels.addr,
        models,
        nodes_addr: cs.nodes.addr,
        nodes,
        vis: (cs.vis.count > 0 && cs.vis.addr != 0).then_some((cs.vis.addr, cs.vis.count as usize)),
        structured: true,
    })
}

#[cfg(test)]
#[path = "cmstruct_tests.rs"]
mod tests;
