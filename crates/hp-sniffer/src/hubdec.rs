use std::collections::HashMap;

use crate::hub::{Field, Hub};
use crate::procmem::MemSource;

pub const PLANE_RECORD: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: &'static str,
    pub ptr: usize,
    pub leaf_stride: usize,
    pub leaf_area: usize,
    pub leaf_center: usize,
    pub leaf_half: usize,
    pub leaf_cluster: usize,
    pub leaf_water: usize,
    pub leaf_first_mark: usize,
    pub leaf_num_mark: usize,
    pub node_stride: usize,
    pub node_area: usize,
    pub node_center: usize,
    pub node_half: usize,
    pub node_plane: usize,
    pub node_child: usize,
    pub node_first_surface: usize,
    pub s1_stride: usize,
    pub s1_prims: usize,
    pub s2_stride: usize,
    pub s2_flags: usize,
    pub s2_count: usize,
    pub s2_plane: usize,
    pub s2_first: usize,
    pub s2_texinfo: usize,
    pub s2_disp: usize,
    pub sl_stride: usize,
    pub sl_styles: usize,
    pub sl_samples: usize,
    pub model_first: usize,
    pub texdata_stride: usize,
    pub texinfo_stride: usize,
    pub texinfo_flags: usize,
    pub texinfo_material: usize,
    pub cubemap_stride: usize,
    pub cubemap_size_at: usize,
    pub prim_stride: usize,
    pub ambient_index_stride: usize,
    pub material_reflectivity_at: Option<i64>,
    pub material_texture_at: Option<i64>,
    pub texture_dims_at: Option<u64>,
    pub disp_contents: Option<usize>,
    pub disp_neighbors: Option<usize>,
    pub disp_sample_start: Option<usize>,
    pub disp_allowed: Option<usize>,
    pub disp_power: usize,
    pub disp_start: usize,
    pub disp_verts: usize,
    pub disp_vert_stride: usize,
    pub disp_object: usize,
    pub cm_model_stride: usize,
    pub vc_count: usize,
    pub vc_desc: usize,
    pub vc_solids: usize,
    pub vc_keys: usize,
    pub pc_body: usize,
    pub pc_drag: usize,
}

pub const X64: Profile = Profile {
    name: "x86-64",
    ptr: 8,
    leaf_stride: 80,
    leaf_area: 16,
    leaf_center: 32,
    leaf_half: 48,
    leaf_cluster: 64,
    leaf_water: 66,
    leaf_first_mark: 68,
    leaf_num_mark: 70,
    node_stride: 96,
    node_area: 16,
    node_center: 32,
    node_half: 48,
    node_plane: 64,
    node_child: 72,
    node_first_surface: 88,
    s1_stride: 16,
    s1_prims: 12,
    s2_stride: 40,
    s2_flags: 0,
    s2_count: 3,
    s2_plane: 4,
    s2_first: 12,
    s2_texinfo: 26,
    s2_disp: 28,
    sl_stride: 40,
    sl_styles: 24,
    sl_samples: 32,
    model_first: 8,
    texdata_stride: 16,
    texinfo_stride: 88,
    texinfo_flags: 72,
    texinfo_material: 80,
    cubemap_stride: 32,
    cubemap_size_at: 24,
    prim_stride: 12,
    ambient_index_stride: 8,
    material_reflectivity_at: Some(-0x18),
    material_texture_at: Some(-32),
    texture_dims_at: Some(40),
    disp_contents: Some(320),
    disp_neighbors: Some(616),
    disp_sample_start: Some(704),
    disp_allowed: Some(408),
    disp_power: 524,
    disp_start: 528,
    disp_verts: 152,
    disp_vert_stride: 64,
    disp_object: 776,
    cm_model_stride: 72,
    vc_count: 40,
    vc_desc: 42,
    vc_solids: 48,
    vc_keys: 56,
    pc_body: 8,
    pc_drag: 16,
};

pub const X86: Profile = Profile {
    name: "x86",
    ptr: 4,
    leaf_stride: 64,
    leaf_area: 12,
    leaf_center: 16,
    leaf_half: 32,
    leaf_cluster: 48,
    leaf_water: 50,
    leaf_first_mark: 52,
    leaf_num_mark: 54,
    node_stride: 64,
    node_area: 12,
    node_center: 16,
    node_half: 32,
    node_plane: 48,
    node_child: 52,
    node_first_surface: 60,
    s1_stride: 16,
    s1_prims: 12,
    s2_stride: 32,
    s2_flags: 0,
    s2_count: 3,
    s2_plane: 4,
    s2_first: 8,
    s2_texinfo: 22,
    s2_disp: 24,
    sl_stride: 32,
    sl_styles: 24,
    sl_samples: 28,
    model_first: 4,
    texdata_stride: 8,
    texinfo_stride: 80,
    texinfo_flags: 72,
    texinfo_material: 76,
    cubemap_stride: 20,
    cubemap_size_at: 16,
    prim_stride: 12,
    ambient_index_stride: 8,
    material_reflectivity_at: Some(-20),
    material_texture_at: Some(-24),
    texture_dims_at: Some(36),
    disp_contents: Some(228),
    disp_neighbors: Some(476),
    disp_sample_start: Some(564),
    disp_allowed: Some(280),
    disp_power: 388,
    disp_start: 392,
    disp_verts: 136,
    disp_vert_stride: 64,
    disp_object: 616,
    cm_model_stride: 56,
    vc_count: 40,
    vc_desc: 42,
    vc_solids: 44,
    vc_keys: 48,
    pc_body: 4,
    pc_drag: 8,
};

pub const PROFILES: [Profile; 2] = [X64, X86];

pub fn profile_for(ptr: usize) -> Option<Profile> {
    PROFILES.iter().copied().find(|p| p.ptr == ptr)
}

static PROFILE_OVERRIDE: std::sync::Mutex<Option<Vec<Profile>>> = std::sync::Mutex::new(None);

pub fn override_profiles(profiles: Option<Vec<Profile>>) {
    if let Ok(mut g) = PROFILE_OVERRIDE.lock() {
        *g = profiles;
    }
}

pub fn profiles_for(ptr: usize) -> Vec<Profile> {
    if let Ok(g) = PROFILE_OVERRIDE.lock()
        && let Some(list) = g.as_ref()
    {
        return list.iter().copied().filter(|p| p.ptr == ptr).collect();
    }
    PROFILES.iter().copied().filter(|p| p.ptr == ptr).collect()
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn i16_at(d: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([d[at], d[at + 1]])
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn ptr_at(d: &[u8], at: usize, ptr: usize) -> u64 {
    if ptr == 8 {
        u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
    } else {
        u64::from(u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]))
    }
}

fn read_exact<S: MemSource + ?Sized>(src: &S, addr: u64, len: usize) -> Option<Vec<u8>> {
    if len == 0 {
        return Some(Vec::new());
    }
    if addr == 0 {
        return None;
    }
    let v = src.read_vec(addr, len);
    (v.len() == len).then_some(v)
}

pub fn flat_array<S: MemSource + ?Sized>(src: &S, hub: &Hub, field: Field, record: usize) -> Option<Vec<u8>> {
    let a = hub.array(field);
    if a.count < 0 {
        return None;
    }
    read_exact(src, a.addr, a.count as usize * record)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Face {
    pub plane: u32,
    pub back: bool,
    pub on_node: bool,
    pub first: i32,
    pub count: u8,
    pub texinfo: u16,
    pub disp_ptr: u64,
    pub flags: u16,
    pub styles: [u8; 4],
    pub light_ptr: u64,
    pub mins: [i16; 2],
    pub extents: [i16; 2],
    pub prims: u16,
    pub first_prim: u16,
}

pub fn read_faces<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<Face>> {
    let n = hub.count(Field::Surfaces);
    let planes = hub.addr(Field::Planes);
    let nplanes = hub.count(Field::Planes) as u64;
    let s1 = read_exact(src, hub.surfaces1, n * p.s1_stride)?;
    let s2 = read_exact(src, hub.surfaces2, n * p.s2_stride)?;
    let sl = read_exact(src, hub.surfacelighting, n * p.sl_stride)?;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let b = &s2[i * p.s2_stride..(i + 1) * p.s2_stride];
        let l = &sl[i * p.sl_stride..(i + 1) * p.sl_stride];
        let m = &s1[i * p.s1_stride..(i + 1) * p.s1_stride];
        let plane_ptr = ptr_at(b, p.s2_plane, p.ptr);
        if plane_ptr < planes || !(plane_ptr - planes).is_multiple_of(PLANE_RECORD as u64) {
            return None;
        }
        let plane = (plane_ptr - planes) / PLANE_RECORD as u64;
        if plane >= nplanes {
            return None;
        }
        let flags = u16_at(b, p.s2_flags);
        out.push(Face {
            plane: plane as u32,
            back: flags & 0x40 != 0,
            on_node: flags & 0x2 != 0,
            first: i32_at(b, p.s2_first),
            count: b[p.s2_count],
            texinfo: u16_at(b, p.s2_texinfo) >> 1,
            disp_ptr: ptr_at(b, p.s2_disp, p.ptr),
            flags,
            styles: [l[p.sl_styles], l[p.sl_styles + 1], l[p.sl_styles + 2], l[p.sl_styles + 3]],
            light_ptr: ptr_at(l, p.sl_samples, p.ptr),
            mins: [i16_at(l, 0), i16_at(l, 2)],
            extents: [i16_at(l, 4), i16_at(l, 6)],
            prims: u16_at(m, p.s1_prims),
            first_prim: u16_at(m, p.s1_prims + 2),
        });
    }
    Some(out)
}

pub fn polygon_area(verts: &[[f32; 3]]) -> f32 {
    let mut sum = [0.0f64; 3];
    for i in 1..verts.len().saturating_sub(1) {
        let (a, b, c) = (verts[0], verts[i], verts[i + 1]);
        let u = [f64::from(b[0] - a[0]), f64::from(b[1] - a[1]), f64::from(b[2] - a[2])];
        let v = [f64::from(c[0] - a[0]), f64::from(c[1] - a[1]), f64::from(c[2] - a[2])];
        sum[0] += u[1] * v[2] - u[2] * v[1];
        sum[1] += u[2] * v[0] - u[0] * v[2];
        sum[2] += u[0] * v[1] - u[1] * v[0];
    }
    (0.5 * (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt()) as f32
}

pub fn face_lump(
    faces: &[Face],
    lightdata: u64,
    vertindices: &[u16],
    vertexes: &[[f32; 3]],
    face_disp: &HashMap<usize, i16>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(faces.len() * 56);
    for (i, f) in faces.iter().enumerate() {
        let mut rec = [0u8; 56];
        rec[0..2].copy_from_slice(&(f.plane as u16).to_le_bytes());
        rec[2] = u8::from(f.back);
        rec[3] = u8::from(f.on_node);
        rec[4..8].copy_from_slice(&f.first.to_le_bytes());
        rec[8..10].copy_from_slice(&i16::from(f.count).to_le_bytes());
        rec[10..12].copy_from_slice(&(f.texinfo as i16).to_le_bytes());
        rec[12..14].copy_from_slice(&face_disp.get(&i).copied().unwrap_or(-1).to_le_bytes());
        rec[14..16].copy_from_slice(&(-1i16).to_le_bytes());
        rec[16..20].copy_from_slice(&f.styles);
        let lightofs: i32 = if f.light_ptr == 0 { -1 } else { f.light_ptr.wrapping_sub(lightdata) as i64 as i32 };
        rec[20..24].copy_from_slice(&lightofs.to_le_bytes());
        let first = f.first.max(0) as usize;
        let poly: Vec<[f32; 3]> = vertindices
            .get(first..first + usize::from(f.count))
            .map(|idx| idx.iter().filter_map(|&v| vertexes.get(usize::from(v)).copied()).collect())
            .unwrap_or_default();
        let area = if f.light_ptr == 0 && f.styles[0] == 255 { 1_048_576.0f32 } else { polygon_area(&poly) };
        rec[24..28].copy_from_slice(&area.to_le_bytes());
        rec[28..32].copy_from_slice(&i32::from(f.mins[0]).to_le_bytes());
        rec[32..36].copy_from_slice(&i32::from(f.mins[1]).to_le_bytes());
        rec[36..40].copy_from_slice(&i32::from(f.extents[0]).to_le_bytes());
        rec[40..44].copy_from_slice(&i32::from(f.extents[1]).to_le_bytes());
        rec[44..48].copy_from_slice(&(-1i32).to_le_bytes());
        rec[48..50].copy_from_slice(&f.prims.to_le_bytes());
        rec[50..52].copy_from_slice(&f.first_prim.to_le_bytes());
        out.extend_from_slice(&rec);
    }
    out
}

pub fn luxels(f: &Face) -> usize {
    (f.extents[0].max(0) as usize + 1) * (f.extents[1].max(0) as usize + 1)
}

pub fn light_bytes(f: &Face) -> usize {
    let styles = f.styles.iter().filter(|s| **s != 255).count();
    let luxels = (f.extents[0].max(0) as usize + 1) * (f.extents[1].max(0) as usize + 1);
    let bump = if f.flags & 0x8 != 0 { 4 } else { 1 };
    luxels * styles * bump * 4
}

pub fn lighting_len(faces: &[Face], lightdata: u64) -> usize {
    faces
        .iter()
        .filter(|f| f.light_ptr != 0)
        .map(|f| (f.light_ptr - lightdata) as usize + light_bytes(f))
        .max()
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemLeaf {
    pub contents: i32,
    pub area: i16,
    pub flags: i16,
    pub center: [f32; 3],
    pub half: [f32; 3],
    pub cluster: i16,
    pub water: i16,
    pub first_mark: u16,
    pub num_mark: u16,
}

pub fn read_leafs<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<MemLeaf>> {
    let raw = flat_array(src, hub, Field::Leafs, p.leaf_stride)?;
    Some(
        raw.chunks_exact(p.leaf_stride)
            .map(|r| MemLeaf {
                contents: i32_at(r, 0),
                area: i16_at(r, p.leaf_area),
                flags: i16_at(r, p.leaf_area + 2),
                center: [f32_at(r, p.leaf_center), f32_at(r, p.leaf_center + 4), f32_at(r, p.leaf_center + 8)],
                half: [f32_at(r, p.leaf_half), f32_at(r, p.leaf_half + 4), f32_at(r, p.leaf_half + 8)],
                cluster: i16_at(r, p.leaf_cluster),
                water: i16_at(r, p.leaf_water),
                first_mark: u16_at(r, p.leaf_first_mark),
                num_mark: u16_at(r, p.leaf_num_mark),
            })
            .collect(),
    )
}

fn short(v: f32) -> i16 {
    v.round().clamp(-32768.0, 32767.0) as i16
}

pub fn bounds_shorts(center: [f32; 3], half: [f32; 3]) -> ([i16; 3], [i16; 3]) {
    (
        [short(center[0] - half[0]), short(center[1] - half[1]), short(center[2] - half[2])],
        [short(center[0] + half[0]), short(center[1] + half[1]), short(center[2] + half[2])],
    )
}

pub fn leaf_lump(leafs: &[MemLeaf], brushes: &[(u16, u16)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(leafs.len() * 32);
    for (i, l) in leafs.iter().enumerate() {
        let (mn, mx) = bounds_shorts(l.center, l.half);
        out.extend_from_slice(&l.contents.to_le_bytes());
        out.extend_from_slice(&l.cluster.to_le_bytes());
        let af = (l.area as u16 & 0x1ff) | ((l.flags as u16) << 9);
        out.extend_from_slice(&af.to_le_bytes());
        for v in mn.iter().chain(&mx) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let first = if l.num_mark == 0 && l.contents == 1 { 0 } else { l.first_mark };
        out.extend_from_slice(&first.to_le_bytes());
        out.extend_from_slice(&l.num_mark.to_le_bytes());
        let (fb, nb) = brushes.get(i).copied().unwrap_or((0, 0));
        out.extend_from_slice(&fb.to_le_bytes());
        out.extend_from_slice(&nb.to_le_bytes());
        out.extend_from_slice(&l.water.to_le_bytes());
        out.extend_from_slice(&0i16.to_le_bytes());
    }
    out
}

pub fn node_lump<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<u8>> {
    let raw = flat_array(src, hub, Field::Nodes, p.node_stride)?;
    let nodes = hub.addr(Field::Nodes);
    let leafs = hub.addr(Field::Leafs);
    let n_nodes = hub.count(Field::Nodes) as u64;
    let n_leafs = hub.count(Field::Leafs) as u64;
    let planes = hub.addr(Field::Planes);
    let mut out = Vec::with_capacity(raw.len() / p.node_stride * 32);
    for r in raw.chunks_exact(p.node_stride) {
        let plane_ptr = ptr_at(r, p.node_plane, p.ptr);
        if plane_ptr < planes {
            return None;
        }
        let plane = (plane_ptr - planes) / PLANE_RECORD as u64;
        out.extend_from_slice(&(plane as i32).to_le_bytes());
        for k in 0..2 {
            let c = ptr_at(r, p.node_child + k * p.ptr, p.ptr);
            let idx: i32 = if c >= nodes && (c - nodes) / (p.node_stride as u64) < n_nodes {
                ((c - nodes) / p.node_stride as u64) as i32
            } else if c >= leafs && (c - leafs) / (leaf_stride_of(p)) < n_leafs {
                -1 - ((c - leafs) / leaf_stride_of(p)) as i32
            } else {
                return None;
            };
            out.extend_from_slice(&idx.to_le_bytes());
        }
        let center = [f32_at(r, p.node_center), f32_at(r, p.node_center + 4), f32_at(r, p.node_center + 8)];
        let half = [f32_at(r, p.node_half), f32_at(r, p.node_half + 4), f32_at(r, p.node_half + 8)];
        let (mn, mx) = bounds_shorts(center, half);
        for v in mn.iter().chain(&mx) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&u16_at(r, p.node_first_surface).to_le_bytes());
        out.extend_from_slice(&u16_at(r, p.node_first_surface + 2).to_le_bytes());
        out.extend_from_slice(&i16_at(r, p.node_area).to_le_bytes());
        out.extend_from_slice(&0i16.to_le_bytes());
    }
    Some(out)
}

fn leaf_stride_of(p: &Profile) -> u64 {
    p.leaf_stride as u64
}

pub fn leaf_faces_lump<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<u8>> {
    let a = hub.array(Field::MarkSurfaces);
    let raw = read_exact(src, a.addr, a.count.max(0) as usize * p.ptr)?;
    let base = hub.surfaces2;
    let stride = p.s2_stride as u64;
    let n = hub.count(Field::Surfaces) as u64;
    let mut out = Vec::with_capacity(raw.len() / p.ptr * 2);
    for c in raw.chunks_exact(p.ptr) {
        let v = ptr_at(c, 0, p.ptr);
        if v < base {
            return None;
        }
        let idx = (v - base) / stride;
        if idx >= n {
            return None;
        }
        out.extend_from_slice(&(idx as u16).to_le_bytes());
    }
    Some(out)
}

pub fn leafwater_lump<S: MemSource + ?Sized>(src: &S, hub: &Hub) -> Option<Vec<u8>> {
    let mut raw = flat_array(src, hub, Field::LeafWater, 12)?;
    for r in raw.chunks_exact_mut(12) {
        r[10] = 0;
        r[11] = 0;
    }
    Some(raw)
}

pub fn primitive_lump<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<u8>> {
    let raw = flat_array(src, hub, Field::Primitives, p.prim_stride)?;
    let mut out = Vec::with_capacity(raw.len() / p.prim_stride * 10);
    for r in raw.chunks_exact(p.prim_stride) {
        out.push(r[0]);
        out.push(0);
        out.extend_from_slice(&r[4..12]);
    }
    Some(out)
}

pub fn ambient_index_lump<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<u8>> {
    let a = hub.array(Field::Leafs);
    let raw = read_exact(src, hub.addr(Field::AmbientIndex), a.count.max(0) as usize * p.ambient_index_stride)?;
    let mut out = Vec::with_capacity(raw.len() / p.ambient_index_stride * 4);
    for r in raw.chunks_exact(p.ambient_index_stride) {
        out.extend_from_slice(&(i32_at(r, 0) as u16).to_le_bytes());
        out.extend_from_slice(&(i32_at(r, 4) as u16).to_le_bytes());
    }
    Some(out)
}

pub fn ambient_samples(index: &[u8]) -> usize {
    index.chunks_exact(4).map(|c| usize::from(u16_at(c, 0))).sum()
}

const RADIUS_AT: usize = 60;
const ATTN_AT: usize = 64;
const RADIUS_THRESHOLD_HDR: f32 = 0.015;
const RADIUS_THRESHOLD_LDR: f32 = 0.03;
const DEFAULT_RADIUS: f32 = 2000.0;

pub fn computed_radius(rec: &[u8], hdr: bool) -> Option<f32> {
    let constant = f64::from(f32_at(rec, ATTN_AT));
    let linear = f64::from(f32_at(rec, ATTN_AT + 4));
    let quad = f64::from(f32_at(rec, ATTN_AT + 8));
    let i = [f64::from(f32_at(rec, 12)), f64::from(f32_at(rec, 16)), f64::from(f32_at(rec, 20))];
    let target = (i[0] * i[0] + i[1] * i[1] + i[2] * i[2]).sqrt() / f64::from(if hdr { RADIUS_THRESHOLD_HDR } else { RADIUS_THRESHOLD_LDR });
    if quad > 0.0 {
        let disc = linear * linear - 4.0 * quad * (constant - target);
        return (disc >= 0.0).then(|| ((-linear + disc.sqrt()) / (2.0 * quad)) as f32);
    }
    if linear > 0.0 {
        let d = (target - constant) / linear;
        return (d > 0.0).then_some(d as f32);
    }
    Some(DEFAULT_RADIUS)
}

pub fn worldlight_lump<S: MemSource + ?Sized>(src: &S, hub: &Hub, hdr: bool) -> Option<Vec<u8>> {
    let mut raw = flat_array(src, hub, Field::WorldLights, 88)?;
    for r in raw.chunks_exact_mut(88) {
        let have = f32_at(r, RADIUS_AT);
        if let Some(c) = computed_radius(r, hdr)
            && have != 0.0
            && (have - c).abs() <= c.abs() * 1.0e-3
        {
            r[RADIUS_AT..RADIUS_AT + 4].copy_from_slice(&0f32.to_le_bytes());
        }
    }
    Some(raw)
}

fn read_name<S: MemSource + ?Sized>(src: &S, addr: u64) -> Option<String> {
    if addr == 0 {
        return None;
    }
    let raw = src.read_vec(addr, 256);
    let end = raw.iter().position(|b| *b == 0)?;
    let text = std::str::from_utf8(&raw[..end]).ok()?;
    (end > 0 && text.chars().all(|c| !c.is_control())).then(|| text.to_owned())
}

pub fn texdata_names<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<(u64, String)>> {
    let raw = flat_array(src, hub, Field::TexData, p.texdata_stride)?;
    let mut out = Vec::new();
    for r in raw.chunks_exact(p.texdata_stride) {
        let at = ptr_at(r, 0, p.ptr);
        let text = read_name(src, at)?;
        out.push((at, text));
    }
    Some(out)
}

pub fn string_lumps<S: MemSource + ?Sized>(src: &S, names: &[(u64, String)]) -> Option<(Vec<u8>, Vec<u8>)> {
    let base = names.iter().map(|n| n.0).min()?;
    let mut table = Vec::with_capacity(names.len() * 4);
    let mut end = 0u64;
    for (addr, name) in names {
        table.extend_from_slice(&((*addr - base) as u32).to_le_bytes());
        end = end.max(*addr - base + name.len() as u64 + 1);
    }
    let data = read_exact(src, base, end as usize)?;
    Some((data, table))
}

const COLLIDE_HEADER: usize = 28;
const SURFACE_ROOT_AT: usize = 32;
const SURFACE_MARK_AT: usize = 36;
const TREE_NODE: usize = 28;
const TREE_MARK_AT: usize = 27;

pub fn clear_runtime_marks(body: &mut [u8]) {
    if body.len() < 48 {
        return;
    }
    body[SURFACE_MARK_AT..SURFACE_MARK_AT + 4].fill(0);
    let root = u32::from_le_bytes([
        body[SURFACE_ROOT_AT],
        body[SURFACE_ROOT_AT + 1],
        body[SURFACE_ROOT_AT + 2],
        body[SURFACE_ROOT_AT + 3],
    ]) as usize;
    if root + TREE_NODE <= body.len() {
        body[root + TREE_MARK_AT] = 0;
    }
}
const COLLIDE_ID: [u8; 4] = *b"VPHY";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhysModel {
    pub index: i32,
    pub solids: Vec<Vec<u8>>,
    pub keys: Vec<u8>,
}

pub fn collide_solid<S: MemSource + ?Sized>(src: &S, object: u64, p: &Profile) -> Option<Vec<u8>> {
    let o = read_exact(src, object, p.pc_drag + 12)?;
    let body = ptr_at(&o, p.pc_body, p.ptr);
    let drag = &o[p.pc_drag..p.pc_drag + 12];
    let head = read_exact(src, body, 32)?;
    let size = (u32::from_le_bytes([head[28], head[29], head[30], head[31]]) >> 8) as usize;
    if !(64..=(64 << 20)).contains(&size) {
        return None;
    }
    let mut data = read_exact(src, body, size)?;
    clear_runtime_marks(&mut data);
    let mut out = Vec::with_capacity(COLLIDE_HEADER + size);
    out.extend_from_slice(&COLLIDE_ID);
    out.extend_from_slice(&0x100u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(drag);
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&data);
    Some(out)
}

pub fn read_phys_models<S: MemSource + ?Sized>(src: &S, models_addr: u64, count: usize, p: &Profile) -> Vec<(PhysModel, u16)> {
    let raw = src.read_vec(models_addr, count * p.cm_model_stride);
    let mut out = Vec::new();
    for (i, r) in raw.chunks_exact(p.cm_model_stride).enumerate() {
        let n = usize::from(u16_at(r, p.vc_count) & 0x7fff);
        let desc = u16_at(r, p.vc_desc);
        if n == 0 {
            continue;
        }
        let table = ptr_at(r, p.vc_solids, p.ptr);
        let Some(tbl) = read_exact(src, table, n * p.ptr) else { continue };
        let mut solids = Vec::with_capacity(n);
        for c in tbl.chunks_exact(p.ptr) {
            match collide_solid(src, ptr_at(c, 0, p.ptr), p) {
                Some(s) => solids.push(s),
                None => break,
            }
        }
        if solids.len() != n {
            continue;
        }
        let keys_at = ptr_at(r, p.vc_keys, p.ptr);
        let keys = read_exact(src, keys_at, usize::from(desc)).unwrap_or_default();
        out.push((PhysModel { index: i as i32, solids, keys }, desc));
    }
    out
}

fn c_string_at(d: &[u8], at: usize) -> Option<String> {
    let rest = d.get(at..)?;
    let end = rest.iter().position(|b| *b == 0)?;
    String::from_utf8(rest[..end].to_vec()).ok().filter(|s| !s.is_empty())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyKind {
    Int,
    Float,
    Text,
    Vec3,
    Vec4,
    Pair,
}

const BLOCK_CUSTOM: u16 = 0xffff;

fn block_name(block: u16) -> Option<&'static str> {
    Some(match block {
        0 => "solid",
        1 => "fluid",
        2 => "ragdollconstraint",
        3 => "animatedfriction",
        4 => "collisionrules",
        5 => "staticsolid",
        _ => return None,
    })
}

const SOLID_KEYS: [(&str, KeyKind); 14] = [
    ("index", KeyKind::Int),
    ("contents", KeyKind::Int),
    ("name", KeyKind::Text),
    ("parent", KeyKind::Text),
    ("mass", KeyKind::Float),
    ("surfaceprop", KeyKind::Text),
    ("massCenterOverride", KeyKind::Vec3),
    ("damping", KeyKind::Float),
    ("rotdamping", KeyKind::Float),
    ("drag", KeyKind::Float),
    ("rollingdrag", KeyKind::Float),
    ("inertia", KeyKind::Float),
    ("rotInertiaLimit", KeyKind::Float),
    ("volume", KeyKind::Float),
];

const FLUID_KEYS: [(&str, KeyKind); 6] = [
    ("index", KeyKind::Int),
    ("surfaceprop", KeyKind::Text),
    ("damping", KeyKind::Float),
    ("surfaceplane", KeyKind::Vec4),
    ("currentvelocity", KeyKind::Vec3),
    ("contents", KeyKind::Int),
];

const RAGDOLL_KEYS: [(&str, KeyKind); 11] = [
    ("parent", KeyKind::Int),
    ("child", KeyKind::Int),
    ("xmin", KeyKind::Float),
    ("xmax", KeyKind::Float),
    ("xfriction", KeyKind::Float),
    ("ymin", KeyKind::Float),
    ("ymax", KeyKind::Float),
    ("yfriction", KeyKind::Float),
    ("zmin", KeyKind::Float),
    ("zmax", KeyKind::Float),
    ("zfriction", KeyKind::Float),
];

const ANIMATED_KEYS: [(&str, KeyKind); 5] = [
    ("animfrictionmin", KeyKind::Float),
    ("animfrictionmax", KeyKind::Float),
    ("animfrictiontimein", KeyKind::Float),
    ("animfrictiontimeout", KeyKind::Float),
    ("animfrictiontimehold", KeyKind::Float),
];

const RULES_KEYS: [(&str, KeyKind); 2] = [("selfcollisions", KeyKind::Int), ("collisionpair", KeyKind::Pair)];

fn key_info(block: u16, key: u16) -> Option<(&'static str, KeyKind)> {
    let table: &[(&str, KeyKind)] = match block {
        0 | 5 => &SOLID_KEYS,
        1 => &FLUID_KEYS,
        2 => &RAGDOLL_KEYS,
        3 => &ANIMATED_KEYS,
        4 => &RULES_KEYS,
        _ => return None,
    };
    table.get(usize::from(key)).copied()
}

pub fn fixed6(v: f64) -> String {
    if !v.is_finite() {
        return format!("{v:.6}");
    }
    let long = format!("{:.40}", v.abs());
    let (int_part, frac) = long.split_once('.').unwrap_or((long.as_str(), ""));
    let mut digits: Vec<u8> = int_part.bytes().chain(frac.bytes().take(6)).collect();
    let round_up = frac.as_bytes().get(6).is_some_and(|d| *d >= b'5');
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let split = digits.len() - 6;
    let text = format!("{}.{}", String::from_utf8_lossy(&digits[..split]), String::from_utf8_lossy(&digits[split..]));
    if v.is_sign_negative() { format!("-{text}") } else { text }
}

pub fn decode_packed_keys(d: &[u8]) -> Option<String> {
    if d.len() < 12 || u16_at(d, 0) != 0x100 {
        return None;
    }
    let (items, strings, start, values_at) = (usize::from(u16_at(d, 4)), usize::from(u16_at(d, 6)), 12usize, usize::from(u16_at(d, 10)));
    let table = start + items * 8;
    let text_at = table + strings * 2;
    if text_at > values_at || values_at > d.len() {
        return None;
    }
    let offsets: Vec<usize> = (0..strings).map(|i| usize::from(u16_at(d, table + i * 2))).collect();
    let string = |i: usize| -> Option<String> {
        let at = text_at + *offsets.get(i)?;
        c_string_at(&d[..values_at], at).or_else(|| d.get(at).filter(|b| **b == 0).map(|_| String::new()))
    };
    let values = &d[values_at..];
    let mut out = String::new();
    let mut block: Option<u16> = None;
    let mut inline = false;
    for i in 0..items {
        let at = start + i * 8;
        let (kind, section, a, b) =
            (u16_at(d, at), u16_at(d, at + 2), usize::from(u16_at(d, at + 4)), usize::from(u16_at(d, at + 6)));
        match kind {
            1 => {
                let name = if section == BLOCK_CUSTOM { string(a)? } else { block_name(section)?.to_owned() };
                inline = section == BLOCK_CUSTOM && name.eq_ignore_ascii_case("break");
                if inline {
                    out.push_str(&format!("{name} {{"));
                } else {
                    out.push_str(&format!("{name} {{
"));
                }
                block = Some(section);
            }
            2 => {
                out.push_str(if inline { " }
" } else { "}
" });
                inline = false;
                block = None;
            }
            0 => {
                let item_section = section;
                let section = block?;
                if item_section == BLOCK_CUSTOM && section != BLOCK_CUSTOM {
                    out.push_str(&format!("\"{}\" \"{}\"\n", string(a)?, string(b)?));
                    continue;
                }
                if section == BLOCK_CUSTOM {
                    if inline {
                        out.push_str(&format!("\"{}\" \"{}\" ", string(a)?, string(b)?));
                    } else {
                        out.push_str(&format!("\"{}\" \"{}\"
", string(a)?, string(b)?));
                    }
                    continue;
                }
                let (name, kind) = key_info(section, a as u16)?;
                let f = |o: usize| values.get(o..o + 4).map(|v| f64::from(f32_at(v, 0)));
                let text = match kind {
                    KeyKind::Int => format!("{}", i32_at(values.get(b..b + 4)?, 0)),
                    KeyKind::Float => fixed6(f(b)?),
                    KeyKind::Text => string(b)?,
                    KeyKind::Vec3 => format!("{} {} {} ", fixed6(f(b)?), fixed6(f(b + 4)?), fixed6(f(b + 8)?)),
                    KeyKind::Vec4 => format!("{} {} {} {} ", fixed6(f(b)?), fixed6(f(b + 4)?), fixed6(f(b + 8)?), fixed6(f(b + 12)?)),
                    KeyKind::Pair => {
                        let word = i32_at(values.get(b..b + 4)?, 0);
                        format!("{},{}", word & 0xffff, (word >> 16) & 0xffff)
                    }
                };
                out.push_str(&format!("\"{name}\" \"{text}\"
"));
            }
            _ => return None,
        }
    }
    (block.is_none() && !out.is_empty()).then_some(out)
}

pub fn key_text(model: &PhysModel) -> Vec<u8> {
    if let Some(mut t) = decode_packed_keys(&model.keys) {
        t.push(' ');
        return t.into_bytes();
    }
    let mut t = String::new();
    if model.index == 0 {
        for i in 0..model.solids.len() {
            t.push_str(&format!("staticsolid {{
\"index\" \"{i}\"
}}
"));
        }
    } else {
        t.push_str("solid {
\"index\" \"0\"
}
");
    }
    let mut out = t.into_bytes();
    out.push(0);
    out
}

pub fn undecoded_keys(models: &[(PhysModel, u16)]) -> usize {
    models.iter().filter(|(m, _)| decode_packed_keys(&m.keys).is_none()).count()
}

pub fn physcollide_lump(models: &[(PhysModel, u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (m, _) in models {
        let keys = key_text(m);
        let data: usize = m.solids.iter().map(|s| 4 + s.len()).sum();
        out.extend_from_slice(&m.index.to_le_bytes());
        out.extend_from_slice(&(data as i32).to_le_bytes());
        out.extend_from_slice(&(keys.len() as i32).to_le_bytes());
        out.extend_from_slice(&(m.solids.len() as i32).to_le_bytes());
        for s in &m.solids {
            out.extend_from_slice(&(s.len() as i32).to_le_bytes());
            out.extend_from_slice(s);
        }
        out.extend_from_slice(&keys);
    }
    for v in [-1i32, -1, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispExtra {
    pub contents: i32,
    pub sample_start: i32,
    pub neighbors: Vec<u8>,
    pub allowed: Vec<u8>,
}

pub fn read_disp_extra<S: MemSource + ?Sized>(src: &S, object: u64, p: &Profile) -> Option<DispExtra> {
    let (c, n, l, a) = (p.disp_contents?, p.disp_neighbors?, p.disp_sample_start?, p.disp_allowed?);
    let end = c.max(l).max(a + 40).max(n + 88) + 4;
    let o = read_exact(src, object, end)?;
    Some(DispExtra {
        contents: i32_at(&o, c),
        sample_start: i32_at(&o, l),
        neighbors: o[n..n + 88].to_vec(),
        allowed: o[a..a + 40].to_vec(),
    })
}

const ALT_START_AT: [u64; 2] = [20, 16];
const ALT_STRIDES: std::ops::RangeInclusive<usize> = 64..=1024;
const ALT_SAMPLE: usize = 48;
const ALT_MAX_KINDS: usize = 16;

fn start_bytes(v: &[f32; 3]) -> [u8; 12] {
    let mut b = [0u8; 12];
    for (k, f) in v.iter().enumerate() {
        b[k * 4..k * 4 + 4].copy_from_slice(&f.to_le_bytes());
    }
    b
}

pub fn alt_contents<S: MemSource + ?Sized>(src: &S, starts: &[[f32; 3]]) -> Option<Vec<i32>> {
    if starts.len() < 2 {
        return None;
    }
    let needle = start_bytes(&starts[0]);
    let finder = memchr::memmem::Finder::new(&needle);
    let sample = starts.len().min(ALT_SAMPLE);
    let chunk = 8usize << 20;
    for r in crate::hub::all_regions(src) {
        if !r.writable {
            continue;
        }
        let mut off = 0u64;
        while off < r.size {
            let n = ((r.size - off) as usize).min(chunk + 16);
            let data = src.read_vec(r.base + off, n);
            for hit in finder.find_iter(&data) {
                if hit >= chunk {
                    continue;
                }
                let first = r.base + off + hit as u64;
                for start_at in ALT_START_AT {
                    if first < start_at {
                        continue;
                    }
                    let base = first - start_at;
                    for stride in ALT_STRIDES.step_by(4) {
                        let probe = base + stride as u64 + start_at;
                        if src.read_vec(probe, 12) != start_bytes(&starts[1]) {
                            continue;
                        }
                        let good = (0..sample)
                            .filter(|k| src.read_vec(base + *k as u64 * stride as u64 + start_at, 12) == start_bytes(&starts[*k]))
                            .count();
                        if good * 10 < sample * 9 {
                            continue;
                        }
                        let out: Vec<i32> = (0..starts.len())
                            .map(|k| {
                                let raw = src.read_vec(base + k as u64 * stride as u64, 4);
                                if raw.len() == 4 { i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) } else { 0 }
                            })
                            .collect();
                        let mut kinds = out.clone();
                        kinds.sort_unstable();
                        kinds.dedup();
                        if kinds.len() > ALT_MAX_KINDS || out.contains(&0) {
                            continue;
                        }
                        return Some(out);
                    }
                }
            }
            if off + n as u64 >= r.size {
                break;
            }
            off += chunk as u64;
        }
    }
    None
}

pub fn patch_dispinfo(lump: &mut [u8], extras: &HashMap<usize, DispExtra>) -> usize {
    let mut done = 0;
    for rec in lump.chunks_exact_mut(176) {
        let face = usize::from(u16_at(rec, 36));
        let Some(x) = extras.get(&face) else { continue };
        rec[32..36].copy_from_slice(&x.contents.to_le_bytes());
        rec[44..48].copy_from_slice(&x.sample_start.to_le_bytes());
        rec[48..136].copy_from_slice(&x.neighbors);
        rec[136..176].copy_from_slice(&x.allowed);
        done += 1;
    }
    done
}

pub fn find_sample_positions<S: MemSource + ?Sized>(
    src: &S,
    last_start: usize,
    last_luxels: usize,
    ptr: usize,
) -> Option<(u64, usize)> {
    let lo = last_start + last_luxels * 4;
    let hi = last_start + last_luxels * 6 + 64;
    for r in crate::hub::all_regions(src) {
        if !r.writable {
            continue;
        }
        let chunk = 8usize << 20;
        let mut off = 0u64;
        while off < r.size {
            let n = ((r.size - off) as usize).min(chunk + 32);
            let data = src.read_vec(r.base + off, n);
            let step = ptr;
            let mut at = 0usize;
            while at + 3 * 8 <= data.len() && at < chunk {
                let a = ptr_at(&data, at, ptr);
                let (n1, n2) = (ptr_at(&data, at + ptr, ptr), ptr_at(&data, at + 2 * ptr, ptr));
                if a > 0x10000 && n1 == n2 && (lo as u64..=hi as u64).contains(&n1) {
                    let probe = src.read_vec(a, 16);
                    let tail = src.read_vec(a + n1 - 16, 16);
                    if probe.len() == 16 && tail.len() == 16 {
                        return Some((a, n1 as usize));
                    }
                }
                at += step;
            }
            if off + n as u64 >= r.size {
                break;
            }
            off += chunk as u64;
        }
    }
    None
}

pub fn cubemap_lump<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<u8>> {
    let raw = flat_array(src, hub, Field::Cubemaps, p.cubemap_stride)?;
    let mut out = Vec::with_capacity(raw.len() / p.cubemap_stride * 16);
    for r in raw.chunks_exact(p.cubemap_stride) {
        for k in 0..3 {
            out.extend_from_slice(&(f32_at(r, k * 4).round() as i32).to_le_bytes());
        }
        out.extend_from_slice(&i32_at(r, p.cubemap_size_at).to_le_bytes());
    }
    Some(out)
}

#[cfg(test)]
#[path = "hubdec_tests.rs"]
mod tests;
