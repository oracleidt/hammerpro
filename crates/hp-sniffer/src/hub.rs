use std::collections::HashMap;

use memchr::memmem;

use crate::procmem::{MemSource, Region};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Submodels,
    Planes,
    Leafs,
    LeafWater,
    Vertexes,
    Occluders,
    OccluderPolys,
    OccluderIndices,
    VertNormalIndices,
    VertNormals,
    Nodes,
    LeafMinDist,
    TexInfo,
    TexData,
    DispInfos,
    Surfaces,
    VertIndices,
    MarkSurfaces,
    WorldLights,
    Primitives,
    PrimVerts,
    PrimIndices,
    Areas,
    AreaPortals,
    ClipPortalVerts,
    Cubemaps,
    AmbientIndex,
    DispReferences,
}

pub const LIGHTDATA: usize = usize::MAX;
pub const SHADOWZ: usize = usize::MAX - 1;
pub const SURFACE_ARRAYS: usize = usize::MAX - 2;
pub const CUBEMAP_SPARE: usize = usize::MAX - 3;
pub const AMBIENT_LIGHTING: usize = usize::MAX - 4;
pub const SURFACES1: usize = usize::MAX - 102;
pub const SURFACES2: usize = usize::MAX - 103;
pub const SURFACE_LIGHTING: usize = usize::MAX - 104;
pub const SURFACE_NORMALS: usize = usize::MAX - 105;

#[derive(Clone, Copy, Debug)]
enum Slot {
    Counted(Field),
    CountOnly(Field),
    PtrOnly(Field),
    Named(usize),
    FlagOnly,
}

const SEQUENCE: &[Slot] = &[
    Slot::CountOnly(Field::Submodels),
    Slot::Counted(Field::Planes),
    Slot::Counted(Field::Leafs),
    Slot::Counted(Field::LeafWater),
    Slot::Counted(Field::Vertexes),
    Slot::Counted(Field::Occluders),
    Slot::Counted(Field::OccluderPolys),
    Slot::Counted(Field::OccluderIndices),
    Slot::Counted(Field::VertNormalIndices),
    Slot::Counted(Field::VertNormals),
    Slot::Counted(Field::Nodes),
    Slot::PtrOnly(Field::LeafMinDist),
    Slot::Counted(Field::TexInfo),
    Slot::Counted(Field::TexData),
    Slot::Counted(Field::DispInfos),
    Slot::CountOnly(Field::Surfaces),
    Slot::Named(SURFACE_ARRAYS),
    Slot::FlagOnly,
    Slot::Counted(Field::VertIndices),
    Slot::Counted(Field::MarkSurfaces),
    Slot::Named(LIGHTDATA),
    Slot::Counted(Field::WorldLights),
    Slot::Named(SHADOWZ),
    Slot::Counted(Field::Primitives),
    Slot::Counted(Field::PrimVerts),
    Slot::Counted(Field::PrimIndices),
    Slot::Counted(Field::Areas),
    Slot::Counted(Field::AreaPortals),
    Slot::Counted(Field::ClipPortalVerts),
    Slot::PtrOnly(Field::Cubemaps),
    Slot::CountOnly(Field::Cubemaps),
    Slot::CountOnly(Field::DispReferences),
    Slot::Named(CUBEMAP_SPARE),
    Slot::PtrOnly(Field::AmbientIndex),
    Slot::Named(AMBIENT_LIGHTING),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Array {
    pub count: i64,
    pub addr: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hub {
    pub base: u64,
    pub ptr: usize,
    pub arrays: HashMap<Field, Array>,
    pub surfaces1: u64,
    pub surfaces2: u64,
    pub surfacelighting: u64,
    pub surfacenormals: u64,
    pub lightdata: u64,
    pub ambient_lighting: u64,
    pub submodel_objects: Vec<u64>,
    pub problems: Vec<String>,
}

impl Hub {
    pub fn array(&self, f: Field) -> Array {
        self.arrays.get(&f).copied().unwrap_or_default()
    }

    pub fn count(&self, f: Field) -> usize {
        self.array(f).count.max(0) as usize
    }

    pub fn addr(&self, f: Field) -> u64 {
        self.array(f).addr
    }
}

fn align(off: usize, to: usize) -> usize {
    off.div_ceil(to) * to
}

pub fn layout(ptr: usize) -> (Vec<(Slot2, usize)>, usize) {
    let mut out = Vec::new();
    let mut off = 0usize;
    for slot in SEQUENCE {
        match *slot {
            Slot::Counted(f) => {
                off = align(off, 4);
                out.push((Slot2::Count(f), off));
                off += 4;
                off = align(off, ptr);
                out.push((Slot2::Ptr(f), off));
                off += ptr;
            }
            Slot::CountOnly(f) => {
                off = align(off, 4);
                out.push((Slot2::Count(f), off));
                off += 4;
            }
            Slot::PtrOnly(f) => {
                off = align(off, ptr);
                out.push((Slot2::Ptr(f), off));
                off += ptr;
            }
            Slot::Named(n) => {
                if n == SURFACE_ARRAYS {
                    off = align(off, ptr);
                    for id in [SURFACES1, SURFACES2, SURFACE_LIGHTING, SURFACE_NORMALS] {
                        out.push((Slot2::Named(id), off));
                        off += ptr;
                    }
                } else {
                    off = align(off, ptr);
                    out.push((Slot2::Named(n), off));
                    off += ptr;
                }
            }
            Slot::FlagOnly => {
                off += 1;
            }
        }
    }
    (out, off)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot2 {
    Count(Field),
    Ptr(Field),
    Named(usize),
}

fn read_int(d: &[u8], at: usize) -> i64 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]) as i64
}

fn read_ptr(d: &[u8], at: usize, ptr: usize) -> u64 {
    if ptr == 8 {
        u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
    } else {
        u64::from(u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]))
    }
}

fn mapped<S: MemSource + ?Sized>(src: &S, addr: u64, len: usize) -> bool {
    len == 0 || (addr != 0 && src.read_vec(addr, len.min(16)).len() == len.min(16))
}

pub fn all_regions<S: MemSource + ?Sized>(src: &S) -> Vec<Region> {
    let mut r = src.regions();
    r.sort_by_key(|x| x.base);
    r
}

pub fn find_pointer_hits<S: MemSource + ?Sized>(src: &S, target: u64, ptr: usize) -> Vec<u64> {
    let needle: Vec<u8> = if ptr == 8 { target.to_le_bytes().to_vec() } else { (target as u32).to_le_bytes().to_vec() };
    let finder = memmem::Finder::new(&needle);
    let mut out = Vec::new();
    let chunk = 8usize << 20;
    for r in all_regions(src) {
        if !r.writable {
            continue;
        }
        let mut off = 0u64;
        while off < r.size {
            let n = ((r.size - off) as usize).min(chunk + 16);
            let data = src.read_vec(r.base + off, n);
            for hit in finder.find_iter(&data) {
                if hit < chunk && (r.base + off + hit as u64).is_multiple_of(ptr as u64) {
                    out.push(r.base + off + hit as u64);
                }
            }
            if off + n as u64 >= r.size {
                break;
            }
            off += chunk as u64;
        }
    }
    out
}

pub fn parse_hub<S: MemSource + ?Sized>(src: &S, base: u64, ptr: usize) -> Option<Hub> {
    let (offsets, total) = layout(ptr);
    let raw = src.read_vec(base, total);
    if raw.len() != total {
        return None;
    }
    let mut hub = Hub { base, ptr, ..Hub::default() };
    for (slot, off) in &offsets {
        match slot {
            Slot2::Count(f) => {
                hub.arrays.entry(*f).or_default().count = read_int(&raw, *off);
            }
            Slot2::Ptr(f) => {
                hub.arrays.entry(*f).or_default().addr = read_ptr(&raw, *off, ptr);
            }
            Slot2::Named(n) => {
                let v = read_ptr(&raw, *off, ptr);
                match *n {
                    LIGHTDATA => hub.lightdata = v,
                    AMBIENT_LIGHTING => hub.ambient_lighting = v,
                    SURFACES1 => hub.surfaces1 = v,
                    SURFACES2 => hub.surfaces2 = v,
                    SURFACE_LIGHTING => hub.surfacelighting = v,
                    SURFACE_NORMALS => hub.surfacenormals = v,
                    _ => {}
                }
            }
        }
    }
    if let Some(a) = hub.arrays.get_mut(&Field::Surfaces) {
        a.addr = hub.surfaces1;
    }
    Some(hub)
}

fn plausible<S: MemSource + ?Sized>(src: &S, hub: &Hub, planes: usize, planes_addr: u64) -> bool {
    let p = hub.array(Field::Planes);
    if p.addr != planes_addr || p.count != planes as i64 {
        return false;
    }
    let sub = hub.array(Field::Submodels).count;
    if !(1..=4096).contains(&sub) {
        return false;
    }
    let mut ok = 0;
    let checks: [(Field, usize); 8] = [
        (Field::Leafs, 16),
        (Field::Vertexes, 12),
        (Field::Nodes, 16),
        (Field::TexInfo, 72),
        (Field::VertIndices, 2),
        (Field::MarkSurfaces, 4),
        (Field::WorldLights, 88),
        (Field::Surfaces, 16),
    ];
    for (f, size) in checks {
        let a = hub.array(f);
        if a.count < 0 || a.count > 50_000_000 {
            return false;
        }
        if a.count > 0 && mapped(src, a.addr, size) {
            ok += 1;
        } else if a.count > 0 {
            return false;
        }
    }
    ok >= 4 && hub.array(Field::Leafs).count > 0 && hub.array(Field::Nodes).count > 0
}

pub fn find_hub<S: MemSource + ?Sized>(src: &S, planes_addr: u64, planes: usize, widths: &[usize]) -> Option<Hub> {
    let mut best: Option<Hub> = None;
    for &ptr in widths {
        let (offsets, _) = layout(ptr);
        let plane_off = offsets
            .iter()
            .find_map(|(s, o)| if *s == Slot2::Ptr(Field::Planes) { Some(*o) } else { None })
            .unwrap_or(8);
        for hit in find_pointer_hits(src, planes_addr, ptr) {
            let Some(base) = hit.checked_sub(plane_off as u64) else { continue };
            let Some(mut hub) = parse_hub(src, base, ptr) else { continue };
            if !plausible(src, &hub, planes, planes_addr) {
                continue;
            }
            hub.submodel_objects = find_submodel_objects(src, &hub);
            if best.as_ref().is_none_or(|b| hub.array(Field::Surfaces).count > b.array(Field::Surfaces).count) {
                best = Some(hub);
            }
        }
        if best.is_some() {
            break;
        }
    }
    best
}

fn find_submodel_objects<S: MemSource + ?Sized>(src: &S, hub: &Hub) -> Vec<u64> {
    let mut hits = find_pointer_hits(src, hub.base, hub.ptr);
    hits.sort_unstable();
    hits.dedup();
    let want = hub.count(Field::Submodels);
    if hits.len() < want {
        return hits;
    }
    let mut best: Vec<u64> = Vec::new();
    for i in 0..hits.len() {
        let mut run = vec![hits[i]];
        if i + 1 < hits.len() {
            let stride = hits[i + 1] - hits[i];
            if (0x40..=0x400).contains(&stride) {
                let mut j = i + 1;
                while j < hits.len() && hits[j] - run[run.len() - 1] == stride {
                    run.push(hits[j]);
                    j += 1;
                }
            }
        }
        if run.len() >= want && best.is_empty() {
            best = run;
        }
    }
    best.truncate(want);
    best
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CmTable {
    pub vis_addr: u64,
    pub vis_len: usize,
}

pub fn find_cm_vis<S: MemSource + ?Sized>(src: &S, planes_addr: u64, planes: usize, ptr: usize) -> Option<CmTable> {
    let plane_slot = align(4, ptr);
    for hit in find_pointer_hits(src, planes_addr, ptr) {
        let Some(base) = hit.checked_sub(plane_slot as u64) else { continue };
        let want = plane_slot;
        let raw = src.read_vec(base, want + 0x100);
        if raw.len() < want + 0x80 {
            continue;
        }
        if read_int(&raw, 0) != planes as i64 {
            continue;
        }
        let mut off = plane_slot + ptr;
        let mut pairs: Vec<(i64, u64)> = Vec::new();
        for kind in 0..7 {
            off = align(off, 4);
            if kind == 2 {
                off += 8;
            }
            if off + 4 > raw.len() {
                break;
            }
            let n = read_int(&raw, off);
            off += 4;
            off = align(off, ptr);
            if off + ptr > raw.len() {
                break;
            }
            pairs.push((n, read_ptr(&raw, off, ptr)));
            off += ptr;
        }
        if pairs.len() < 7 {
            continue;
        }
        let (n, addr) = pairs[6];
        if n > 0 && addr != 0 && mapped(src, addr, 8) {
            return Some(CmTable { vis_addr: addr, vis_len: n as usize });
        }
    }
    None
}
