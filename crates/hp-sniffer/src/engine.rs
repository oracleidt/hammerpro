use std::collections::HashMap;

use glam::DVec3;
use hp_bsp::staticprops::{StaticPropFormat, StaticPropLump, StaticPropRecord};
use hp_vmf::DispInfo;
use rayon::prelude::*;

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};
use crate::world::{FACE_RECORD, RenderFace, RenderWorld};

pub const DISP_POWER_AT: usize = 0x20c;
pub const DISP_START_AT: usize = 0x210;
pub const DISP_VERTS_AT: usize = 0x98;
pub const DISP_OBJECT_BYTES: usize = 0x220;
pub const RENDER_VERT_BYTES: usize = 64;

pub const PROP_STRIDE: usize = 256;
pub const PROP_ORIGIN_AT: usize = 0x28;
pub const PROP_ANGLES_AT: usize = 0x34;
pub const PROP_MODEL_AT: usize = 0x40;
pub const PROP_ALPHA_AT: usize = 0x4c;
pub const PROP_SOLID_AT: usize = 0x4d;
pub const PROP_SKIN_AT: usize = 0x4e;
pub const PROP_FADES_AT: usize = 0x4f;
pub const PROP_LEAF_AT: usize = 0x50;
pub const PROP_LEAF_COUNT_AT: usize = 0x52;
pub const PROP_VTABLE_AT: usize = 0x10;
const PROP_MIN_RUN: usize = 3;

pub const OVERLAY_STRIDE: usize = 0xb8;
const OVERLAY_FACES_AT: usize = 0x10;
const OVERLAY_FACE_COUNT_AT: usize = 0x20;
const OVERLAY_UV_AT: usize = 0x30;
const OVERLAY_POINTS_AT: usize = 0x40;
const OVERLAY_ORIGIN_AT: usize = 0x70;
const OVERLAY_BASIS_AT: usize = 0x7c;
pub const OVERLAY_RECORD_BYTES: usize = 352;
const MAX_OVERLAY_FACES: usize = 64;
const OVERLAY_MIN_RUN: usize = 2;

pub const DISP_MIN_TESS: i32 = i32::MIN;
pub const CUBEMAP_DEFAULT_SIZE: i32 = 6;
pub const DEFAULT_PROP_FLAGS: u32 = 0x100;

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn vec3_at(d: &[u8], at: usize) -> [f32; 3] {
    [f32_at(d, at), f32_at(d, at + 4), f32_at(d, at + 8)]
}

fn finite_vec(v: [f32; 3], limit: f32) -> bool {
    v.iter().all(|c| c.is_finite() && c.abs() <= limit)
}

fn user_pointer(p: u64) -> bool {
    (0x10000..0x8000_0000_0000).contains(&p)
}

pub fn read_cstring<S: MemSource + ?Sized>(src: &S, addr: u64, max: usize) -> Option<String> {
    if !user_pointer(addr) {
        return None;
    }
    let raw = src.read_vec(addr, max);
    let end = raw.iter().position(|&b| b == 0)?;
    if end == 0 || !raw[..end].iter().all(|&b| (0x20..0x7f).contains(&b)) {
        return None;
    }
    Some(String::from_utf8_lossy(&raw[..end]).into_owned())
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileDisp {
    pub min_tess: i32,
    pub allowed: [u32; 10],
    pub verts: Vec<[f32; 5]>,
    pub tags: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemDisp {
    pub face: usize,
    pub start: [f32; 3],
    pub power: u32,
    pub positions: Vec<[f32; 3]>,
    pub file: Option<FileDisp>,
}

pub fn parse_displacement<S: MemSource + ?Sized>(src: &S, face: usize, object: u64) -> Option<MemDisp> {
    let head = src.read_vec(object, DISP_OBJECT_BYTES);
    if head.len() < DISP_OBJECT_BYTES {
        return None;
    }
    let power = i32_at(&head, DISP_POWER_AT);
    if !(2..=4).contains(&power) {
        return None;
    }
    let start = vec3_at(&head, DISP_START_AT);
    if !finite_vec(start, 1.0e6) {
        return None;
    }
    let verts = u64_at(&head, DISP_VERTS_AT);
    if !user_pointer(verts) {
        return None;
    }
    let row = (1usize << power) + 1;
    let raw = src.read_vec(verts, row * row * RENDER_VERT_BYTES);
    if raw.len() != row * row * RENDER_VERT_BYTES {
        return None;
    }
    let positions: Vec<[f32; 3]> = raw.chunks_exact(RENDER_VERT_BYTES).map(|c| vec3_at(c, 0)).collect();
    if !positions.iter().all(|p| finite_vec(*p, 1.0e6)) {
        return None;
    }
    Some(MemDisp { face, start, power: power as u32, positions, file: None })
}

pub fn refresh_displacement_pointers<S: MemSource + ?Sized>(src: &S, faces_addr: u64, faces: &mut [RenderFace]) {
    let raw = src.read_vec(faces_addr, faces.len() * FACE_RECORD);
    for (f, rec) in faces.iter_mut().zip(raw.chunks_exact(FACE_RECORD)) {
        f.disp = u64_at(rec, 32);
        f.has_disp = u16_at(rec, 4) & crate::world::FACE_HAS_DISP != 0;
    }
}

pub fn read_displacements<S: MemSource + ?Sized>(src: &S, faces: &[RenderFace]) -> Vec<MemDisp> {
    let mut out: Vec<MemDisp> = faces
        .par_iter()
        .enumerate()
        .filter(|(_, f)| f.disp != 0)
        .filter_map(|(i, f)| parse_displacement(src, i, f.disp))
        .collect();
    out.sort_by_key(|d| d.face);
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DispLumps {
    pub dispinfo: Vec<u8>,
    pub dispverts: Vec<u8>,
    pub disptris: Vec<u8>,
    pub face_disp: HashMap<usize, i16>,
    pub rejected: usize,
    pub file_rejected: usize,
}

fn dvec(v: [f32; 3]) -> DVec3 {
    DVec3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))
}

pub fn face_polygon(world: &RenderWorld, face: &RenderFace) -> Option<Vec<DVec3>> {
    let first = face.first as usize;
    let idx = world.vertindices.get(first..first + usize::from(face.count))?;
    idx.iter().map(|&i| world.vertexes.get(usize::from(i)).map(|v| dvec(*v))).collect()
}

pub fn plane_normal(plane_bytes: &[u8], face: &RenderFace) -> Option<DVec3> {
    let at = face.plane as usize * 20;
    let p = plane_bytes.get(at..at + 16)?;
    let n = DVec3::new(f64::from(f32_at(p, 0)), f64::from(f32_at(p, 4)), f64::from(f32_at(p, 8)));
    Some(if face.back { -n } else { n })
}

pub fn displacement_lumps(world: &RenderWorld, plane_bytes: &[u8], disps: &[MemDisp]) -> DispLumps {
    let mut out = DispLumps::default();
    let mut vert_start = 0i32;
    let mut tri_start = 0i32;
    for d in disps {
        let Some(face) = world.faces.get(d.face) else {
            out.rejected += 1;
            continue;
        };
        let (Some(polygon), Some(normal)) = (face_polygon(world, face), plane_normal(plane_bytes, face)) else {
            out.rejected += 1;
            continue;
        };
        if polygon.len() != 4 {
            out.rejected += 1;
            continue;
        }
        let ccw: Vec<DVec3> = polygon.iter().rev().copied().collect();
        let mut info = DispInfo::new_flat(d.power, dvec(d.start), normal);
        let Some(flat) = hp_geom::build_disp(&ccw, &info) else {
            out.rejected += 1;
            continue;
        };
        if flat.positions.len() != d.positions.len() {
            out.rejected += 1;
            continue;
        }
        let mut vecs: Vec<DVec3> = Vec::with_capacity(d.positions.len());
        let mut dists: Vec<f64> = Vec::with_capacity(d.positions.len());
        for (pos, base) in d.positions.iter().zip(&flat.positions) {
            let offset = dvec(*pos) - *base;
            let len = offset.length();
            if len < 1.0e-3 {
                vecs.push(DVec3::ZERO);
                dists.push(0.0);
                continue;
            }
            vecs.push(offset / len);
            dists.push(len);
        }
        for k in 0..vecs.len() {
            info.normals[k] = if vecs[k] == DVec3::ZERO { normal } else { vecs[k] };
            info.distances[k] = dists[k];
        }
        let mut tags =
            hp_geom::build_disp(&ccw, &info).map(|mesh| hp_geom::compiled_disp_tags(&info, &mesh)).unwrap_or_default();
        let mut min_tess = DISP_MIN_TESS;
        let mut allowed = [u32::MAX; 10];
        let mut dist_alpha: Vec<f32> = vec![0.0; vecs.len()];
        let mut vec_out: Vec<[f32; 3]> = vecs.iter().map(|v| [v.x as f32, v.y as f32, v.z as f32]).collect();
        let mut dist_out: Vec<f32> = dists.iter().map(|v| *v as f32).collect();
        if let Some(f) = &d.file {
            let consistent = f.verts.len() == vecs.len()
                && f.tags.len() == tags.len()
                && f.verts.iter().zip(vecs.iter().zip(&dists)).all(|(fv, (v, dist))| {
                    let fo = DVec3::new(f64::from(fv[0]), f64::from(fv[1]), f64::from(fv[2])) * f64::from(fv[3]);
                    (fo - *v * *dist).length() < 0.05
                });
            if consistent {
                min_tess = f.min_tess;
                allowed = f.allowed;
                tags = f.tags.clone();
                for (k, fv) in f.verts.iter().enumerate() {
                    vec_out[k] = [fv[0], fv[1], fv[2]];
                    dist_out[k] = fv[3];
                    dist_alpha[k] = fv[4];
                }
            } else {
                out.file_rejected += 1;
            }
        }
        let index = (out.dispinfo.len() / 176) as i16;
        let mut rec = vec![0u8; 176];
        for (k, c) in d.start.iter().enumerate() {
            rec[k * 4..k * 4 + 4].copy_from_slice(&c.to_le_bytes());
        }
        rec[12..16].copy_from_slice(&vert_start.to_le_bytes());
        rec[16..20].copy_from_slice(&tri_start.to_le_bytes());
        rec[20..24].copy_from_slice(&(d.power as i32).to_le_bytes());
        rec[24..28].copy_from_slice(&min_tess.to_le_bytes());
        rec[32..36].copy_from_slice(&1i32.to_le_bytes());
        rec[36..38].copy_from_slice(&(d.face as u16).to_le_bytes());
        for (k, a) in allowed.iter().enumerate() {
            rec[136 + k * 4..140 + k * 4].copy_from_slice(&a.to_le_bytes());
        }
        out.dispinfo.extend_from_slice(&rec);
        for k in 0..vec_out.len() {
            for c in [vec_out[k][0], vec_out[k][1], vec_out[k][2], dist_out[k], dist_alpha[k]] {
                out.dispverts.extend_from_slice(&c.to_le_bytes());
            }
        }
        vert_start += vecs.len() as i32;
        for t in &tags {
            out.disptris.extend_from_slice(&t.to_le_bytes());
        }
        tri_start += tags.len() as i32;
        out.face_disp.insert(d.face, index);
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemOverlay {
    pub id: i32,
    pub texinfo: i32,
    pub faces: Vec<i32>,
    pub u: [f32; 2],
    pub v: [f32; 2],
    pub points: [[f32; 3]; 4],
    pub origin: [f32; 3],
    pub basis_u: [f32; 3],
    pub basis_v: [f32; 3],
    pub normal: [f32; 3],
    pub render_order: u16,
    pub from_file: bool,
}

fn face_of_pointer(ptr: u64, faces_addr: u64, faces: usize) -> Option<i32> {
    let rel = ptr.checked_sub(faces_addr)?;
    let rem = rel % FACE_RECORD as u64;
    if rem != 4 {
        return None;
    }
    let i = rel / FACE_RECORD as u64;
    (i < faces as u64).then_some(i as i32)
}

pub fn parse_overlay<S: MemSource + ?Sized>(
    src: &S,
    rec: &[u8],
    faces_addr: u64,
    faces: usize,
    texinfos: usize,
) -> Option<MemOverlay> {
    if rec.len() < OVERLAY_STRIDE {
        return None;
    }
    let id = i32_at(rec, 0);
    let texinfo = i32_at(rec, 4);
    if !(0..1_000_000).contains(&id) || !(0..texinfos as i32).contains(&texinfo) {
        return None;
    }
    let ptr = u64_at(rec, OVERLAY_FACES_AT);
    let count = i32_at(rec, OVERLAY_FACE_COUNT_AT);
    if !(0..=MAX_OVERLAY_FACES as i32).contains(&count) || (count > 0 && !user_pointer(ptr)) {
        return None;
    }
    let u = [f32_at(rec, OVERLAY_UV_AT), f32_at(rec, OVERLAY_UV_AT + 4)];
    let v = [f32_at(rec, OVERLAY_UV_AT + 8), f32_at(rec, OVERLAY_UV_AT + 12)];
    if !u.iter().chain(&v).all(|c| c.is_finite() && c.abs() < 1.0e6) {
        return None;
    }
    let mut points = [[0f32; 3]; 4];
    for (k, p) in points.iter_mut().enumerate() {
        *p = vec3_at(rec, OVERLAY_POINTS_AT + k * 12);
        if !finite_vec(*p, 1.0e6) {
            return None;
        }
    }
    let origin = vec3_at(rec, OVERLAY_ORIGIN_AT);
    let basis_u = vec3_at(rec, OVERLAY_BASIS_AT);
    let basis_v = vec3_at(rec, OVERLAY_BASIS_AT + 12);
    let normal = vec3_at(rec, OVERLAY_BASIS_AT + 24);
    if !finite_vec(origin, 1.0e6) || !finite_vec(basis_u, 1.5) || !finite_vec(basis_v, 1.5) || !finite_vec(normal, 1.5)
    {
        return None;
    }
    let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if (len(normal) - 1.0).abs() > 0.01 || (len(basis_u) - 1.0).abs() > 0.01 {
        return None;
    }
    let raw = if count == 0 { Vec::new() } else { src.read_vec(ptr, count as usize * 8) };
    if raw.len() != count as usize * 8 {
        return None;
    }
    let faces: Option<Vec<i32>> =
        raw.chunks_exact(8).map(|c| face_of_pointer(u64_at(c, 0), faces_addr, faces)).collect();
    Some(MemOverlay {
        id,
        texinfo,
        faces: faces?,
        u,
        v,
        points,
        origin,
        basis_u,
        basis_v,
        normal,
        render_order: 0,
        from_file: false,
    })
}

pub fn find_overlays<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    faces_addr: u64,
    faces: usize,
    texinfos: usize,
) -> (Vec<MemOverlay>, usize) {
    let opts = ScanOptions { overlap: OVERLAY_STRIDE * 4, writable_only: true, ..*opts };
    let mut starts: Vec<(u64, i32)> = par_chunks(src, &opts, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j + OVERLAY_STRIDE <= data.len() && j < limit {
            let id = i32_at(data, j);
            let texinfo = i32_at(data, j + 4);
            let count = i32_at(data, j + OVERLAY_FACE_COUNT_AT);
            if (0..4096).contains(&id)
                && (0..texinfos as i32).contains(&texinfo)
                && (0..=MAX_OVERLAY_FACES as i32).contains(&count)
                && (count == 0 || user_pointer(u64_at(data, j + OVERLAY_FACES_AT)))
                && finite_vec(vec3_at(data, j + OVERLAY_ORIGIN_AT), 1.0e6)
                && finite_vec(vec3_at(data, j + OVERLAY_BASIS_AT + 24), 1.5)
                && (vec3_at(data, j + OVERLAY_BASIS_AT + 24).iter().map(|c| c * c).sum::<f32>() - 1.0).abs() < 0.01
            {
                out.push((base + j as u64, id));
            }
            j += 8;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    starts.sort_unstable();
    starts.dedup();
    let ids: HashMap<u64, i32> = starts.iter().copied().collect();
    let mut runs: Vec<Vec<(u64, i32)>> = Vec::new();
    for &(addr, id) in &starts {
        let is_head =
            addr.checked_sub(OVERLAY_STRIDE as u64).and_then(|p| ids.get(&p)).is_none_or(|&pid| pid + 1 != id);
        if !is_head {
            continue;
        }
        let mut run = vec![(addr, id)];
        while let Some(&next) = ids.get(&(run.last().expect("not empty").0 + OVERLAY_STRIDE as u64)) {
            let last = run.last().expect("not empty");
            if next != last.1 + 1 {
                break;
            }
            let at = last.0 + OVERLAY_STRIDE as u64;
            run.push((at, next));
        }
        runs.push(run);
    }
    runs.sort_by_key(|r| std::cmp::Reverse(r.len()));
    let wanted = runs.first().map_or(0, Vec::len);
    let usable: Vec<&Vec<(u64, i32)>> = runs.iter().filter(|r| r.len() >= OVERLAY_MIN_RUN).collect();
    let parsed: Vec<Vec<MemOverlay>> = usable
        .iter()
        .map(|run| {
            run.par_iter()
                .filter_map(|&(a, _)| {
                    let rec = src.read_vec(a, OVERLAY_STRIDE);
                    parse_overlay(src, &rec, faces_addr, faces, texinfos)
                })
                .collect()
        })
        .collect();
    let mut by_id: std::collections::BTreeMap<i32, MemOverlay> = std::collections::BTreeMap::new();
    for list in parsed {
        for o in list {
            let better = by_id.get(&o.id).is_none_or(|old| old.faces.is_empty() && !o.faces.is_empty());
            if better {
                by_id.insert(o.id, o);
            }
        }
    }
    let singles: Vec<MemOverlay> = starts
        .par_iter()
        .filter_map(|&(a, _)| {
            let rec = src.read_vec(a, OVERLAY_STRIDE);
            parse_overlay(src, &rec, faces_addr, faces, texinfos).filter(|o| !o.faces.is_empty())
        })
        .collect();
    for o in singles {
        by_id.entry(o.id).or_insert(o);
    }
    if wanted < OVERLAY_MIN_RUN && by_id.is_empty() {
        return (Vec::new(), 0);
    }
    let expected = by_id.keys().next_back().map_or(0, |&m| m as usize + 1);
    (by_id.into_values().collect(), expected)
}

pub fn overlay_lump(overlays: &[MemOverlay]) -> Vec<u8> {
    let mut out = Vec::with_capacity(overlays.len() * OVERLAY_RECORD_BYTES);
    for (k, o) in overlays.iter().enumerate() {
        let mut r = vec![0u8; OVERLAY_RECORD_BYTES];
        r[0..4].copy_from_slice(&(k as i32).to_le_bytes());
        r[4..6].copy_from_slice(&(o.texinfo as i16).to_le_bytes());
        let count = o.faces.len().min(MAX_OVERLAY_FACES) as u16 | (o.render_order.min(3) << 14);
        r[6..8].copy_from_slice(&count.to_le_bytes());
        for (n, f) in o.faces.iter().take(MAX_OVERLAY_FACES).enumerate() {
            r[8 + n * 4..12 + n * 4].copy_from_slice(&f.to_le_bytes());
        }
        let floats: Vec<f32> =
            o.u.iter()
                .chain(&o.v)
                .copied()
                .chain(o.points.iter().enumerate().flat_map(|(n, p)| {
                    let z = if n < 3 { o.basis_u[n] } else { 0.0 };
                    [p[0], p[1], z]
                }))
                .chain(o.origin)
                .chain(o.normal)
                .collect();
        for (n, f) in floats.iter().enumerate() {
            r[264 + n * 4..268 + n * 4].copy_from_slice(&f.to_le_bytes());
        }
        out.extend_from_slice(&r);
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemProp {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub model: String,
    pub alpha: u8,
    pub solid: u8,
    pub skin: u8,
    pub fades: bool,
    pub first_leaf: u16,
    pub leaf_count: u16,
    pub file: Option<StaticPropRecord>,
}

fn prop_plausible(rec: &[u8]) -> bool {
    if rec.len() < PROP_STRIDE {
        return false;
    }
    finite_vec(vec3_at(rec, PROP_ORIGIN_AT), 65_536.0)
        && finite_vec(vec3_at(rec, PROP_ANGLES_AT), 3600.0)
        && user_pointer(u64_at(rec, PROP_MODEL_AT))
        && user_pointer(u64_at(rec, PROP_VTABLE_AT))
        && rec[PROP_SOLID_AT] <= 8
        && rec[PROP_FADES_AT] <= 1
        && u16_at(rec, PROP_LEAF_COUNT_AT) <= 4096
}

fn prop_model_name<S: MemSource + ?Sized>(
    src: &S,
    model_ptr: u64,
    cache: &mut HashMap<u64, Option<String>>,
) -> Option<String> {
    if let Some(v) = cache.get(&model_ptr) {
        return v.clone();
    }
    let name = src
        .read_vec(model_ptr + 8, 8)
        .get(..8)
        .map(|b| u64_at(b, 0))
        .and_then(|p| read_cstring(src, p, 260))
        .filter(|n| n.to_ascii_lowercase().ends_with(".mdl"));
    cache.insert(model_ptr, name.clone());
    name
}

pub fn parse_prop(rec: &[u8], model: String) -> MemProp {
    MemProp {
        origin: vec3_at(rec, PROP_ORIGIN_AT),
        angles: vec3_at(rec, PROP_ANGLES_AT),
        model,
        alpha: rec[PROP_ALPHA_AT],
        solid: rec[PROP_SOLID_AT],
        skin: rec[PROP_SKIN_AT],
        fades: rec[PROP_FADES_AT] != 0,
        first_leaf: u16_at(rec, PROP_LEAF_AT),
        leaf_count: u16_at(rec, PROP_LEAF_COUNT_AT),
        file: None,
    }
}

fn same_vtable(a: &[u8], b: &[u8]) -> bool {
    u64_at(a, PROP_VTABLE_AT) == u64_at(b, PROP_VTABLE_AT)
}

pub fn find_static_props<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> (Vec<MemProp>, usize) {
    let opts = ScanOptions { overlap: PROP_STRIDE * (PROP_MIN_RUN + 1), writable_only: true, ..*opts };
    let mut candidates: Vec<u64> = par_chunks(src, &opts, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let need = PROP_STRIDE * PROP_MIN_RUN;
        let mut j = 0usize;
        while j + need <= data.len() && j < limit {
            let first = &data[j..j + PROP_STRIDE];
            if prop_plausible(first) {
                let mut ok = true;
                for k in 1..PROP_MIN_RUN {
                    let next = &data[j + k * PROP_STRIDE..j + (k + 1) * PROP_STRIDE];
                    if !prop_plausible(next) || !same_vtable(first, next) {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    out.push(base + j as u64);
                    j += need;
                    continue;
                }
            }
            j += 8;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    candidates.sort_unstable();
    let mut runs: Vec<(u64, Vec<MemProp>, usize)> = Vec::new();
    let mut covered: Vec<(u64, u64)> = Vec::new();
    for hit in candidates {
        if covered.iter().any(|&(lo, hi)| hit >= lo && hit < hi) {
            continue;
        }
        let reference = src.read_vec(hit, PROP_STRIDE);
        if reference.len() < PROP_STRIDE {
            continue;
        }
        let mut start = hit;
        while start >= PROP_STRIDE as u64 {
            let rec = src.read_vec(start - PROP_STRIDE as u64, PROP_STRIDE);
            if rec.len() == PROP_STRIDE && prop_plausible(&rec) && same_vtable(&rec, &reference) {
                start -= PROP_STRIDE as u64;
            } else {
                break;
            }
        }
        let mut records: Vec<Vec<u8>> = Vec::new();
        let mut at = start;
        loop {
            let rec = src.read_vec(at, PROP_STRIDE);
            if rec.len() == PROP_STRIDE && prop_plausible(&rec) && same_vtable(&rec, &reference) {
                records.push(rec);
                at += PROP_STRIDE as u64;
            } else {
                break;
            }
        }
        covered.push((start, at));
        let mut cache: HashMap<u64, Option<String>> = HashMap::new();
        let names: Vec<Option<String>> =
            records.iter().map(|r| prop_model_name(src, u64_at(r, PROP_MODEL_AT), &mut cache)).collect();
        let named = names.iter().filter(|n| n.is_some()).count();
        if named * 10 < records.len() * 9 || named < PROP_MIN_RUN {
            continue;
        }
        let vtable = u64_at(&reference, PROP_VTABLE_AT);
        let props: Vec<MemProp> = records.iter().zip(names).filter_map(|(r, n)| n.map(|n| parse_prop(r, n))).collect();
        runs.push((vtable, props, records.len()));
    }
    let Some(lead) = runs.iter().max_by_key(|r| r.1.len()).map(|r| r.0) else { return (Vec::new(), 0) };
    let singles_opts = ScanOptions { overlap: PROP_STRIDE, ..opts };
    let singles: Vec<u64> = par_chunks(src, &singles_opts, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j + PROP_STRIDE <= data.len() && j < limit {
            if u64_at(data, j + PROP_VTABLE_AT) == lead && prop_plausible(&data[j..j + PROP_STRIDE]) {
                out.push(base + j as u64);
            }
            j += 8;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    let mut cache: HashMap<u64, Option<String>> = HashMap::new();
    let mut loose: Vec<MemProp> = Vec::new();
    for at in singles {
        if covered.iter().any(|&(lo, hi)| at >= lo && at < hi) {
            continue;
        }
        let rec = src.read_vec(at, PROP_STRIDE);
        if rec.len() != PROP_STRIDE {
            continue;
        }
        if let Some(name) = prop_model_name(src, u64_at(&rec, PROP_MODEL_AT), &mut cache) {
            loose.push(parse_prop(&rec, name));
        }
    }
    if !loose.is_empty() {
        runs.push((lead, loose.clone(), loose.len()));
    }
    let mut best: Vec<MemProp> = Vec::new();
    let mut best_total = 0usize;
    let mut seen: std::collections::HashSet<(u32, u32, u32, u32, u32, u32, String)> = std::collections::HashSet::new();
    runs.sort_by_key(|r| std::cmp::Reverse(r.1.len()));
    for (vtable, props, total) in runs {
        if vtable != lead {
            continue;
        }
        best_total += total;
        for p in props {
            let key = (
                p.origin[0].to_bits(),
                p.origin[1].to_bits(),
                p.origin[2].to_bits(),
                p.angles[0].to_bits(),
                p.angles[1].to_bits(),
                p.angles[2].to_bits(),
                p.model.to_ascii_lowercase(),
            );
            if seen.insert(key) {
                best.push(p);
            }
        }
    }
    (best, best_total)
}

pub fn static_prop_lump(props: &[MemProp]) -> (Vec<u8>, u16) {
    let mut names: Vec<String> = Vec::new();
    let mut index: HashMap<String, u16> = HashMap::new();
    let mut records = Vec::with_capacity(props.len());
    for p in props {
        let key = p.model.to_ascii_lowercase();
        let id = *index.entry(key).or_insert_with(|| {
            names.push(p.model.clone());
            (names.len() - 1) as u16
        });
        let mut r = StaticPropRecord {
            origin: p.origin,
            angles: p.angles,
            prop_type: id,
            first_leaf: p.first_leaf,
            leaf_count: p.leaf_count,
            solid: p.solid,
            flags: DEFAULT_PROP_FLAGS | u32::from(p.fades),
            skin: i32::from(p.skin),
            lighting_origin: p.origin,
            ..StaticPropRecord::default()
        };
        r.diffuse_modulation = [255, 255, 255, p.alpha];
        if let Some(f) = &p.file {
            r = f.clone();
            r.prop_type = id;
        }
        records.push(r);
    }
    let lump = StaticPropLump { names, leaves: Vec::new(), props: records };
    (lump.to_bytes(StaticPropFormat::V10), StaticPropFormat::V10.version())
}

pub fn find_cubemaps<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions, map: &str) -> Vec<[i32; 3]> {
    if map.is_empty() {
        return Vec::new();
    }
    let needle = format!("maps/{}/c", map.to_ascii_lowercase()).into_bytes();
    let opts = ScanOptions { overlap: needle.len() + 48, ..*opts };
    let finder = memchr::memmem::Finder::new(&needle).into_owned();
    let mut found: Vec<[i32; 3]> = par_chunks(src, &opts, |_, data, own| {
        let mut out = Vec::new();
        for at in finder.find_iter(data) {
            if at >= own {
                break;
            }
            if let Some(c) = parse_cubemap_name(&data[at + needle.len() - 1..]) {
                out.push(c);
            }
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    found.sort_unstable();
    found.dedup();
    found
}

pub fn parse_cubemap_name(text: &[u8]) -> Option<[i32; 3]> {
    if text.first() != Some(&b'c') {
        return None;
    }
    let mut at = 1usize;
    let mut out = [0i32; 3];
    for (k, slot) in out.iter_mut().enumerate() {
        let start = at;
        if text.get(at) == Some(&b'-') {
            at += 1;
        }
        let digits = at;
        while text.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == digits || at - start > 9 {
            return None;
        }
        *slot = std::str::from_utf8(&text[start..at]).ok()?.parse().ok()?;
        if k < 2 {
            if text.get(at) != Some(&b'_') {
                return None;
            }
            at += 1;
        }
    }
    match text.get(at) {
        Some(b'_') => None,
        Some(&c) if c.is_ascii_alphanumeric() && c != b'.' => None,
        _ => Some(out),
    }
}

pub fn cubemap_file_lump(cubemaps: &[[i32; 4]]) -> Vec<u8> {
    cubemaps.iter().flat_map(|c| c.iter().flat_map(|v| v.to_le_bytes())).collect()
}

pub fn cubemap_lump(cubemaps: &[[i32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(cubemaps.len() * 16);
    for c in cubemaps {
        for v in c {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&CUBEMAP_DEFAULT_SIZE.to_le_bytes());
    }
    out
}

pub type SideTexinfo = HashMap<(usize, ([i32; 3], i32)), i16>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryExtras {
    pub area_portals: Option<MemAreaPortals>,
    pub displacements: Vec<MemDisp>,
    pub overlays: Vec<MemOverlay>,
    pub overlays_expected: usize,
    pub props: Vec<MemProp>,
    pub props_expected: usize,
    pub cubemaps: Vec<[i32; 3]>,
    pub cubemap_file: Option<Vec<[i32; 4]>>,
    pub side_texinfo: Option<SideTexinfo>,
    pub pakfile: Option<Vec<u8>>,
}

pub fn find_extras<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    world: &RenderWorld,
    plane_bytes: &[u8],
    map: &str,
) -> MemoryExtras {
    let (overlays, overlays_expected) =
        find_overlays(src, opts, world.faces_addr, world.faces.len(), world.texinfo.len());
    let (props, props_expected) = find_static_props(src, opts);
    let mut faces = world.faces.clone();
    refresh_displacement_pointers(src, world.faces_addr, &mut faces);
    MemoryExtras {
        area_portals: find_area_portals(src, opts, plane_bytes),
        displacements: read_displacements(src, &faces),
        overlays,
        overlays_expected,
        props,
        props_expected,
        cubemaps: find_cubemaps(src, opts, map),
        ..MemoryExtras::default()
    }
}

pub const PORTAL_RECORD: usize = 12;
const PORTAL_MIN_RUN: usize = 6;
const PORTAL_MAX_VERTS: u16 = 16;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemAreaPortals {
    pub records: Vec<u8>,
    pub verts: Vec<[f32; 3]>,
    pub records_addr: u64,
    pub verts_addr: u64,
}

impl MemAreaPortals {
    pub fn count(&self) -> usize {
        self.records.len() / PORTAL_RECORD
    }

    pub fn vert_bytes(&self) -> Vec<u8> {
        self.verts.iter().flat_map(|v| v.iter().flat_map(|c| c.to_le_bytes())).collect()
    }
}

fn portal_ok(rec: &[u8], planes: usize) -> bool {
    let count = u16_at(rec, 6);
    let plane = i32_at(rec, 8);
    (count == 0 || (3..=PORTAL_MAX_VERTS).contains(&count))
        && plane >= 0
        && (plane as usize) < planes
        && u16_at(rec, 2) < 4096
}

fn portal_chain(a: &[u8], b: &[u8]) -> bool {
    u32::from(u16_at(b, 4)) == u32::from(u16_at(a, 4)) + u32::from(u16_at(a, 6))
}

pub fn portal_runs(data: &[u8], planes: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut j = 0usize;
    while j + 2 * PORTAL_RECORD <= data.len() {
        let rec = &data[j..j + PORTAL_RECORD];
        if !portal_ok(rec, planes) {
            j += 4;
            continue;
        }
        let mut n = 1usize;
        while j + (n + 1) * PORTAL_RECORD <= data.len() {
            let a = &data[j + (n - 1) * PORTAL_RECORD..j + n * PORTAL_RECORD];
            let b = &data[j + n * PORTAL_RECORD..j + (n + 1) * PORTAL_RECORD];
            if portal_ok(b, planes) && portal_chain(a, b) {
                n += 1;
            } else {
                break;
            }
        }
        let recs = &data[j..j + n * PORTAL_RECORD];
        let rows: Vec<&[u8]> = recs.chunks_exact(PORTAL_RECORD).collect();
        let first_filled = rows.iter().position(|r| u16_at(r, 6) != 0);
        let mut accepted = false;
        if let Some(lead) = first_filled {
            let from = lead.saturating_sub(1);
            let filled = rows[from..].iter().filter(|r| u16_at(r, 6) != 0).count();
            if filled >= PORTAL_MIN_RUN && filled * 10 >= (n - from) * 7 {
                out.push((j + from * PORTAL_RECORD, n - from));
                accepted = true;
            }
        }
        j += if accepted {
            n * PORTAL_RECORD
        } else if n > 64 {
            (n - 1) * PORTAL_RECORD
        } else {
            4
        };
    }
    out
}

pub fn trim_portal_run(recs: &[u8]) -> &[u8] {
    let rows: Vec<&[u8]> = recs.chunks_exact(PORTAL_RECORD).collect();
    let empty = |r: &[u8]| u16_at(r, 6) == 0;
    let mut start = 0usize;
    while start + 1 < rows.len() && empty(rows[start]) && empty(rows[start + 1]) {
        start += 1;
    }
    let mut end = rows.len();
    while end > start && empty(rows[end - 1]) {
        end -= 1;
    }
    &recs[start * PORTAL_RECORD..end * PORTAL_RECORD]
}

fn on_plane(plane_bytes: &[u8], plane: usize, v: [f32; 3]) -> bool {
    let at = plane * 20;
    let Some(p) = plane_bytes.get(at..at + 16) else { return false };
    let d = f32_at(p, 0) * v[0] + f32_at(p, 4) * v[1] + f32_at(p, 8) * v[2] - f32_at(p, 12);
    d.abs() <= 0.5
}

fn portal_vertex_fit(recs: &[u8], verts: &[[f32; 3]], plane_bytes: &[u8]) -> f64 {
    let mut ok = 0usize;
    let mut total = 0usize;
    for r in recs.chunks_exact(PORTAL_RECORD) {
        let (first, count, plane) = (usize::from(u16_at(r, 4)), usize::from(u16_at(r, 6)), i32_at(r, 8) as usize);
        for k in 0..count {
            total += 1;
            if verts.get(first + k).is_some_and(|v| on_plane(plane_bytes, plane, *v)) {
                ok += 1;
            }
        }
    }
    if total == 0 { 0.0 } else { ok as f64 / total as f64 }
}

pub fn find_area_portals<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    plane_bytes: &[u8],
) -> Option<MemAreaPortals> {
    let planes = plane_bytes.len() / 20;
    let scan = ScanOptions { overlap: PORTAL_RECORD * 1024, writable_only: true, ..*opts };
    let mut runs: Vec<(u64, Vec<u8>)> = par_chunks(src, &scan, |base, data, own| {
        portal_runs(data, planes)
            .into_iter()
            .filter(|&(at, _)| at < own)
            .map(|(at, n)| (base + at as u64, data[at..at + n * PORTAL_RECORD].to_vec()))
            .collect::<Vec<_>>()
    })
    .into_iter()
    .flatten()
    .collect();
    runs.sort_by_key(|(_, r)| std::cmp::Reverse(r.len()));
    let (addr, raw) = runs.into_iter().next()?;
    let trimmed = trim_portal_run(&raw);
    let lead = (trimmed.as_ptr() as usize - raw.as_ptr() as usize) as u64;
    let records = trimmed.to_vec();
    let reference = records.chunks_exact(PORTAL_RECORD).find(|r| u16_at(r, 6) >= 3)?;
    let (first, count, plane) =
        (usize::from(u16_at(reference, 4)), usize::from(u16_at(reference, 6)), i32_at(reference, 8) as usize);
    let needed = records
        .chunks_exact(PORTAL_RECORD)
        .map(|r| usize::from(u16_at(r, 4)) + usize::from(u16_at(r, 6)))
        .max()
        .unwrap_or(0);
    let vscan = ScanOptions { overlap: 12 * (count + 1), writable_only: true, ..*opts };
    let mut hits: Vec<u64> = par_chunks(src, &vscan, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j + 12 * count <= data.len() && j < limit {
            let mut ok = true;
            for k in 0..count {
                let v = vec3_at(data, j + 12 * k);
                if !finite_vec(v, 65_536.0) || v == [0.0; 3] || !on_plane(plane_bytes, plane, v) {
                    ok = false;
                    break;
                }
                if k > 0 && v == vec3_at(data, j + 12 * (k - 1)) {
                    ok = false;
                    break;
                }
            }
            if ok {
                out.push(base + j as u64);
            }
            j += 4;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    hits.sort_unstable();
    let mut best: Option<(f64, u64, Vec<[f32; 3]>)> = None;
    for w in hits {
        let Some(base) = w.checked_sub(12 * first as u64) else { continue };
        let raw = src.read_vec(base, needed * 12);
        if raw.len() != needed * 12 {
            continue;
        }
        let verts: Vec<[f32; 3]> = raw.chunks_exact(12).map(|c| vec3_at(c, 0)).collect();
        let fit = portal_vertex_fit(&records, &verts, plane_bytes);
        if fit >= 0.97 && best.as_ref().is_none_or(|b| fit > b.0) {
            best = Some((fit, base, verts));
        }
    }
    let (_, verts_addr, verts) = best?;
    Some(MemAreaPortals { records, verts, records_addr: addr + lead, verts_addr })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::procmem::VecMem;

    pub(crate) fn put_f32s(d: &mut [u8], at: usize, v: &[f32]) {
        for (i, x) in v.iter().enumerate() {
            d[at + i * 4..at + i * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
    }

    pub(crate) fn plane_bytes_z(dist: f32) -> Vec<u8> {
        let mut p = vec![0u8; 20];
        put_f32s(&mut p, 0, &[0.0, 0.0, 1.0, dist]);
        p
    }

    pub(crate) fn quad_world() -> RenderWorld {
        RenderWorld {
            faces: vec![RenderFace {
                plane: 0,
                back: false,
                first: 0,
                count: 4,
                texinfo: 0,
                disp: 0x5000_0000,
                has_disp: true,
                material_sort: 0,
            }],
            vertindices: vec![0, 1, 2, 3],
            vertexes: vec![[0.0, 0.0, 64.0], [0.0, 256.0, 64.0], [256.0, 256.0, 64.0], [256.0, 0.0, 64.0]],
            ..RenderWorld::default()
        }
    }

    pub(crate) fn flat_positions(world: &RenderWorld, power: u32, start: [f32; 3]) -> Vec<[f32; 3]> {
        let polygon = face_polygon(world, &world.faces[0]).expect("polygon");
        let ccw: Vec<DVec3> = polygon.iter().rev().copied().collect();
        let info = DispInfo::new_flat(power, dvec(start), DVec3::Z);
        let mesh = hp_geom::build_disp(&ccw, &info).expect("mesh");
        mesh.positions.iter().map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect()
    }

    pub(crate) fn disp_object(power: i32, start: [f32; 3], verts_at: u64) -> Vec<u8> {
        let mut o = vec![0u8; DISP_OBJECT_BYTES];
        o[DISP_POWER_AT..DISP_POWER_AT + 4].copy_from_slice(&power.to_le_bytes());
        put_f32s(&mut o, DISP_START_AT, &start);
        o[DISP_VERTS_AT..DISP_VERTS_AT + 8].copy_from_slice(&verts_at.to_le_bytes());
        o
    }

    pub(crate) fn render_verts(positions: &[[f32; 3]]) -> Vec<u8> {
        let mut out = Vec::new();
        for p in positions {
            let mut v = vec![0u8; RENDER_VERT_BYTES];
            put_f32s(&mut v, 0, p);
            out.extend_from_slice(&v);
        }
        out
    }

    #[test]
    fn a_displacement_is_read_and_turned_into_lumps() {
        let world = quad_world();
        let start = [0.0, 0.0, 64.0];
        let mut positions = flat_positions(&world, 2, start);
        assert_eq!(positions.len(), 25);
        positions[6][2] += 5.0;
        positions[12][2] -= 8.0;
        positions[13][0] += 3.0;
        let mut mem = VecMem::default();
        mem.add(0x5000_0000, disp_object(2, start, 0x5100_0000));
        mem.add(0x5100_0000, render_verts(&positions));
        let d = parse_displacement(&mem, 0, 0x5000_0000).expect("displacement");
        assert_eq!((d.power, d.start, d.positions.len()), (2, start, 25));
        assert!(parse_displacement(&mem, 0, 0x6000_0000).is_none(), "an unreadable object is refused");
        let mut bad = VecMem::default();
        bad.add(0x5000_0000, disp_object(7, start, 0x5100_0000));
        assert!(parse_displacement(&bad, 0, 0x5000_0000).is_none(), "a power out of range is refused");
        let lumps = displacement_lumps(&world, &plane_bytes_z(64.0), &[d]);
        assert_eq!(lumps.rejected, 0);
        assert_eq!(lumps.dispinfo.len(), 176);
        assert_eq!(lumps.dispverts.len(), 25 * 20);
        assert_eq!(lumps.disptris.len(), 32 * 2);
        assert_eq!(lumps.face_disp.get(&0), Some(&0));
        assert_eq!(i32_at(&lumps.dispinfo, 20), 2);
        assert_eq!(i32_at(&lumps.dispinfo, 24), DISP_MIN_TESS);
        assert_eq!(u16_at(&lumps.dispinfo, 36), 0);
        assert_eq!(f32_at(&lumps.dispinfo, 8), 64.0);
        let vert = |k: usize| -> [f32; 5] {
            let at = k * 20;
            [
                f32_at(&lumps.dispverts, at),
                f32_at(&lumps.dispverts, at + 4),
                f32_at(&lumps.dispverts, at + 8),
                f32_at(&lumps.dispverts, at + 12),
                f32_at(&lumps.dispverts, at + 16),
            ]
        };
        assert_eq!(vert(0), [0.0; 5], "an untouched vertex has no vector and no distance");
        let v6 = vert(6);
        assert!((v6[2] - 1.0).abs() < 1e-4 && (v6[3] - 5.0).abs() < 1e-3, "{v6:?}");
        let v12 = vert(12);
        assert!((v12[2] + 1.0).abs() < 1e-4 && (v12[3] - 8.0).abs() < 1e-3, "{v12:?}");
        let v13 = vert(13);
        assert!((v13[0] - 1.0).abs() < 1e-4 && (v13[3] - 3.0).abs() < 1e-3, "{v13:?}");
        assert!(lumps.dispverts.chunks_exact(20).all(|c| f32_at(c, 12) >= 0.0), "distances are never negative");
    }

    #[test]
    fn displacements_are_collected_from_the_faces_that_point_to_them() {
        let mut world = quad_world();
        world.faces.push(RenderFace { disp: 0, has_disp: false, ..world.faces[0] });
        let positions = flat_positions(&world, 2, [0.0, 0.0, 64.0]);
        let mut mem = VecMem::default();
        mem.add(0x5000_0000, disp_object(2, [0.0, 0.0, 64.0], 0x5100_0000));
        mem.add(0x5100_0000, render_verts(&positions));
        let found = read_displacements(&mem, &world.faces);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].face, 0);
    }

    pub(crate) fn overlay_record(id: i32, texinfo: i32, faces_ptr: u64, count: i32, origin: [f32; 3]) -> Vec<u8> {
        let mut r = vec![0u8; OVERLAY_STRIDE];
        r[0..4].copy_from_slice(&id.to_le_bytes());
        r[4..8].copy_from_slice(&texinfo.to_le_bytes());
        r[OVERLAY_FACES_AT..OVERLAY_FACES_AT + 8].copy_from_slice(&faces_ptr.to_le_bytes());
        r[OVERLAY_FACE_COUNT_AT..OVERLAY_FACE_COUNT_AT + 4].copy_from_slice(&count.to_le_bytes());
        put_f32s(&mut r, OVERLAY_UV_AT, &[1.0, 0.0, 0.0, 1.0]);
        put_f32s(&mut r, OVERLAY_POINTS_AT, &[-8.0, -8.0, 0.0, -8.0, 8.0, 0.0, 8.0, 8.0, 0.0, 8.0, -8.0, 0.0]);
        put_f32s(&mut r, OVERLAY_ORIGIN_AT, &origin);
        put_f32s(&mut r, OVERLAY_BASIS_AT, &[0.0, 1.0, 0.0, 0.0, 0.0, -1.0, -1.0, 0.0, 0.0]);
        r
    }

    #[test]
    fn overlays_are_found_by_their_ids_and_written_back_as_the_lump() {
        let faces_addr = 0x7000_0000u64;
        let face_ptr = |i: u64| faces_addr + i * FACE_RECORD as u64 + 4;
        let mut table = Vec::new();
        for k in 0..5i32 {
            table.extend(overlay_record(k, 10 + k, 0x8000_0000 + 64 * k as u64, 2, [k as f32 * 64.0, 0.0, 8.0]));
        }
        let mut mem = VecMem::default();
        mem.add(0x4000_0000, table);
        for k in 0..5u64 {
            let mut ptrs = Vec::new();
            ptrs.extend_from_slice(&face_ptr(k).to_le_bytes());
            ptrs.extend_from_slice(&face_ptr(k + 10).to_le_bytes());
            mem.add(0x8000_0000 + 64 * k, ptrs);
        }
        let (found, expected) = find_overlays(&mem, &ScanOptions::default(), faces_addr, 100, 4000);
        assert_eq!((found.len(), expected), (5, 5));
        assert_eq!(found[3].id, 3);
        assert_eq!(found[3].texinfo, 13);
        assert_eq!(found[3].faces, vec![3, 13]);
        assert_eq!(found[3].origin, [192.0, 0.0, 8.0]);
        assert_eq!(found[3].basis_u, [0.0, 1.0, 0.0]);
        let lump = overlay_lump(&found);
        assert_eq!(lump.len(), 5 * OVERLAY_RECORD_BYTES);
        let rec = &lump[3 * OVERLAY_RECORD_BYTES..4 * OVERLAY_RECORD_BYTES];
        assert_eq!(i32_at(rec, 0), 3);
        assert_eq!(rec[4], 13);
        assert_eq!(u16_at(rec, 6), 2);
        assert_eq!(i32_at(rec, 8), 3);
        assert_eq!(i32_at(rec, 12), 13);
        assert_eq!(f32_at(rec, 264), 1.0);
        assert_eq!(f32_at(rec, 280 + 8), 0.0);
        assert_eq!(f32_at(rec, 280 + 12 + 8), 1.0, "the second basis component rides in the z of point 1");
        assert_eq!(f32_at(rec, 280 + 12 + 4), 8.0);
        assert_eq!(f32_at(rec, 328), 192.0);
        assert_eq!(f32_at(rec, 340), -1.0);
    }

    #[test]
    fn the_basis_is_stored_in_the_uv_points() {
        let mut o = MemOverlay {
            id: 0,
            texinfo: 1,
            faces: vec![0],
            u: [0.0, 1.0],
            v: [0.0, 1.0],
            points: [[0.0; 3]; 4],
            origin: [0.0; 3],
            basis_u: [0.25, 0.5, 0.75],
            basis_v: [0.0, 0.0, 1.0],
            normal: [0.0, 0.0, 1.0],
            render_order: 0,
            from_file: false,
        };
        let lump = overlay_lump(std::slice::from_ref(&o));
        assert_eq!(f32_at(&lump, 280 + 8), 0.25);
        assert_eq!(f32_at(&lump, 280 + 20), 0.5);
        assert_eq!(f32_at(&lump, 280 + 32), 0.75);
        assert_eq!(f32_at(&lump, 280 + 44), 0.0);
        o.faces.clear();
        assert_eq!(u16_at(&overlay_lump(&[o]), 6), 0);
    }

    #[test]
    fn an_overlay_table_split_in_two_is_merged_by_id() {
        let faces_addr = 0x7000_0000u64;
        let mut mem = VecMem::default();
        let mut a = Vec::new();
        for k in 0..3i32 {
            a.extend(overlay_record(k, 5, 0x8000_0000, 1, [k as f32, 0.0, 0.0]));
        }
        let mut b = Vec::new();
        for k in 3..6i32 {
            b.extend(overlay_record(k, 5, 0x8000_0000, 1, [k as f32, 0.0, 0.0]));
        }
        mem.add(0x4000_0000, a);
        mem.add(0x4800_0000, b);
        mem.add(0x8000_0000, (faces_addr + 4).to_le_bytes().to_vec());
        let (found, expected) = find_overlays(&mem, &ScanOptions::default(), faces_addr, 10, 100);
        assert_eq!((found.len(), expected), (6, 6));
        assert_eq!(found.iter().map(|o| o.id).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn an_overlay_without_faces_yet_is_still_listed_but_flagged() {
        let faces_addr = 0x7000_0000u64;
        let mut mem = VecMem::default();
        let mut t = Vec::new();
        t.extend(overlay_record(0, 5, 0x8000_0000, 1, [0.0; 3]));
        t.extend(overlay_record(1, 5, 0, 0, [8.0, 0.0, 0.0]));
        mem.add(0x4000_0000, t);
        mem.add(0x8000_0000, (faces_addr + 4).to_le_bytes().to_vec());
        let (found, expected) = find_overlays(&mem, &ScanOptions::default(), faces_addr, 10, 100);
        assert_eq!((found.len(), expected), (2, 2));
        assert!(found[1].faces.is_empty());
    }

    #[test]
    fn interlopers_inside_a_record_do_not_break_the_overlay_run() {
        let faces_addr = 0x7000_0000u64;
        let mut table = Vec::new();
        for k in 0..6i32 {
            table.extend(overlay_record(k, 5, 0x8000_0000, 1, [k as f32, 0.0, 0.0]));
        }
        let at = 2 * OVERLAY_STRIDE + 8;
        table[at..at + 4].copy_from_slice(&77i32.to_le_bytes());
        table[at + 4..at + 8].copy_from_slice(&5i32.to_le_bytes());
        table[at + OVERLAY_FACE_COUNT_AT..at + OVERLAY_FACE_COUNT_AT + 4].copy_from_slice(&1i32.to_le_bytes());
        let mut mem = VecMem::default();
        mem.add(0x4000_0000, table);
        mem.add(0x8000_0000, (faces_addr + 4).to_le_bytes().to_vec());
        let (found, expected) = find_overlays(&mem, &ScanOptions::default(), faces_addr, 10, 100);
        assert_eq!((found.len(), expected), (6, 6));
    }

    fn prop_record(
        vtable: u64,
        origin: [f32; 3],
        angles: [f32; 3],
        model: u64,
        skin: u8,
        solid: u8,
        fades: bool,
    ) -> Vec<u8> {
        let mut r = vec![0u8; PROP_STRIDE];
        r[PROP_VTABLE_AT..PROP_VTABLE_AT + 8].copy_from_slice(&vtable.to_le_bytes());
        put_f32s(&mut r, PROP_ORIGIN_AT, &origin);
        put_f32s(&mut r, PROP_ANGLES_AT, &angles);
        r[PROP_MODEL_AT..PROP_MODEL_AT + 8].copy_from_slice(&model.to_le_bytes());
        r[PROP_ALPHA_AT] = 255;
        r[PROP_SOLID_AT] = solid;
        r[PROP_SKIN_AT] = skin;
        r[PROP_FADES_AT] = u8::from(fades);
        r[PROP_LEAF_COUNT_AT] = 1;
        r
    }

    fn prop_memory(count: usize) -> VecMem {
        let mut mem = VecMem::default();
        let mut array = Vec::new();
        for i in 0..count {
            let model = if i % 2 == 0 { 0x9000_0000u64 } else { 0x9000_0100u64 };
            array.extend(prop_record(
                0x7ffe_b100_0000,
                [i as f32 * 16.0, -32.0, 8.0],
                [0.0, i as f32 * 10.0, 0.0],
                model,
                (i % 3) as u8,
                6,
                i % 2 == 0,
            ));
        }
        mem.add(0x2000_0000, array);
        let mut m0 = vec![0u8; 16];
        m0[8..16].copy_from_slice(&0x9000_1000u64.to_le_bytes());
        let mut m1 = vec![0u8; 16];
        m1[8..16].copy_from_slice(&0x9000_1100u64.to_le_bytes());
        mem.add(0x9000_0000, m0);
        mem.add(0x9000_0100, m1);
        mem.add(0x9000_1000, b"models/props_c17/oildrum001.mdl\0".to_vec());
        mem.add(0x9000_1100, b"models/props_junk/gnome.mdl\0".to_vec());
        mem
    }

    #[test]
    fn static_props_are_found_and_written_as_a_game_lump() {
        let mem = prop_memory(12);
        let (found, total) = find_static_props(&mem, &ScanOptions::default());
        assert_eq!((found.len(), total), (12, 12));
        assert_eq!(found[3].model, "models/props_junk/gnome.mdl");
        assert_eq!(found[3].origin, [48.0, -32.0, 8.0]);
        assert_eq!(found[3].angles, [0.0, 30.0, 0.0]);
        assert_eq!((found[4].skin, found[4].solid, found[4].fades), (1, 6, true));
        assert_eq!(found[5].skin, 2);
        let (bytes, version) = static_prop_lump(&found);
        assert_eq!(version, 10);
        let (lump, format) = StaticPropLump::parse(&bytes, version, None).expect("lump");
        assert_eq!(format, Some(StaticPropFormat::V10));
        assert_eq!(lump.names.len(), 2);
        assert_eq!(lump.props.len(), 12);
        assert_eq!(lump.names[lump.props[2].prop_type as usize], "models/props_c17/oildrum001.mdl");
        assert_eq!(lump.props[7].origin, [112.0, -32.0, 8.0]);
        assert_eq!(lump.props[7].angles[1], 70.0);
        assert_eq!(lump.props[5].skin, 2);
        assert_eq!(lump.props[1].flags & 1, 0);
        assert_eq!(lump.props[2].flags & 1, 1);
        assert_eq!(lump.props[1].flags & 0x100, 0x100);
    }

    #[test]
    fn props_split_over_several_blocks_are_all_collected() {
        let mut mem = prop_memory(12);
        let mut second = Vec::new();
        for i in 0..8 {
            second.extend(prop_record(
                0x7ffe_b100_0000,
                [i as f32 * 16.0, 500.0, 8.0],
                [0.0, 0.0, 0.0],
                0x9000_0000u64,
                0,
                6,
                false,
            ));
        }
        mem.add(0x5000_0000, second.clone());
        mem.add(0x6000_0000, second);
        let (found, total) = find_static_props(&mem, &ScanOptions::default());
        assert_eq!(found.len(), 20, "the duplicate block is dropped");
        assert_eq!(total, 12 + 8 + 8);
    }

    #[test]
    fn props_need_a_resolvable_model_name() {
        let mut mem = prop_memory(10);
        mem.blocks.retain(|(b, _)| *b != 0x9000_1000);
        mem.add(0x9000_1000, b"not a model\0".to_vec());
        let (found, _) = find_static_props(&mem, &ScanOptions::default());
        assert!(found.is_empty(), "half of the props have no valid model");
        let (none, total) = find_static_props(&VecMem::default(), &ScanOptions::default());
        assert!(none.is_empty() && total == 0);
        let mut noise_only = VecMem::default();
        noise_only.add(0x1000_0000, vec![0xabu8; 4096]);
        assert!(find_static_props(&noise_only, &ScanOptions::default()).0.is_empty());
    }

    #[test]
    fn cubemap_names_carry_the_origin() {
        assert_eq!(parse_cubemap_name(b"c-1639_-5767_72.vtf"), Some([-1639, -5767, 72]));
        assert_eq!(parse_cubemap_name(b"c8840_5330_1480\0"), Some([8840, 5330, 1480]));
        assert_eq!(parse_cubemap_name(b"c8840_5330_1480.hdr.vtf"), Some([8840, 5330, 1480]));
        assert_eq!(parse_cubemap_name(b"c1_2_3_extra"), None);
        assert_eq!(parse_cubemap_name(b"c1_2"), None);
        assert_eq!(parse_cubemap_name(b"d1_2_3"), None);
        assert_eq!(parse_cubemap_name(b"c1_2_3abc"), None);
        assert_eq!(parse_cubemap_name(b"c--1_2_3"), None);
        assert_eq!(parse_cubemap_name(b"c12345678901234_2_3"), None);
        let mut mem = VecMem::default();
        let mut text = Vec::new();
        for name in [
            "materials/maps/gm_x/c-1639_-5767_72.vtf",
            "maps/gm_x/c-1639_-5767_72.vtf",
            "maps/gm_x/c10_20_30.hdr.vtf",
            "maps/gm_x/brick/wall_10_20_30",
            "maps/gm_y/c5_6_7.vtf",
            "maps/GM_X/c10_20_30.vtf",
        ] {
            text.extend_from_slice(name.as_bytes());
            text.extend_from_slice(&[0, 0, 7]);
        }
        mem.add(0x3000_0000, text);
        let found = find_cubemaps(&mem, &ScanOptions::default(), "gm_x");
        assert_eq!(found, vec![[-1639, -5767, 72], [10, 20, 30]]);
        assert!(find_cubemaps(&mem, &ScanOptions::default(), "").is_empty());
        let lump = cubemap_lump(&found);
        assert_eq!(lump.len(), 32);
        assert_eq!(i32_at(&lump, 0), -1639);
        assert_eq!(i32_at(&lump, 12), CUBEMAP_DEFAULT_SIZE);
        assert_eq!(i32_at(&lump, 28), CUBEMAP_DEFAULT_SIZE);
    }

    fn portal_planes() -> Vec<u8> {
        let mut p = Vec::new();
        for i in 0..40 {
            let mut rec = vec![0u8; 20];
            put_f32s(&mut rec, 0, &[1.0, 0.0, 0.0, 16.0 * (i as f32 + 1.0)]);
            p.extend(rec);
        }
        p
    }

    fn portal_memory() -> VecMem {
        let mut records = vec![0u8; 12];
        let mut verts: Vec<[f32; 3]> = Vec::new();
        for i in 0..8u16 {
            let plane = i32::from(i) + 3;
            let d = 16.0 * (plane as f32 + 1.0);
            let first = verts.len() as u16;
            for k in 0..4 {
                verts.push([d, (k % 2) as f32 * 32.0 + 1.0, (k / 2) as f32 * 32.0 + 1.0 + f32::from(i)]);
            }
            let mut r = vec![0u8; 12];
            r[0..2].copy_from_slice(&(i + 1).to_le_bytes());
            r[2..4].copy_from_slice(&1u16.to_le_bytes());
            r[4..6].copy_from_slice(&first.to_le_bytes());
            r[6..8].copy_from_slice(&4u16.to_le_bytes());
            r[8..12].copy_from_slice(&plane.to_le_bytes());
            records.extend(r);
        }
        let mut mem = VecMem::default();
        let mut region = vec![0u8; 256];
        region.extend(records);
        region.extend(vec![0u8; 256]);
        mem.add(0x1000_0000, region);
        let mut vb = vec![0u8; 128];
        for v in &verts {
            for c in v {
                vb.extend_from_slice(&c.to_le_bytes());
            }
        }
        vb.extend(vec![0u8; 128]);
        mem.add(0x1800_0000, vb);
        mem
    }

    #[test]
    fn area_portals_and_their_vertexes_are_found_without_a_file() {
        let planes = portal_planes();
        let mem = portal_memory();
        let found = find_area_portals(&mem, &ScanOptions::default(), &planes).expect("portals");
        assert_eq!(found.count(), 9, "the leading empty record is kept, like in the file");
        assert_eq!(found.verts.len(), 32);
        assert_eq!(found.records_addr, 0x1000_0000 + 256);
        assert_eq!(found.verts_addr, 0x1800_0000 + 128);
        assert_eq!(found.vert_bytes().len(), 32 * 12);
        assert_eq!(u16_at(&found.records, 12 + 6), 4);
        assert!(find_area_portals(&VecMem::default(), &ScanOptions::default(), &planes).is_none());
    }

    #[test]
    fn portal_runs_ignore_zero_memory_and_unchained_records() {
        let planes = 40;
        assert!(portal_runs(&vec![0u8; 4096], planes).is_empty());
        let mut chain = Vec::new();
        for i in 0..10u16 {
            let mut r = vec![0u8; 12];
            r[4..6].copy_from_slice(&(i * 4).to_le_bytes());
            r[6..8].copy_from_slice(&4u16.to_le_bytes());
            r[8..12].copy_from_slice(&(i32::from(i) + 1).to_le_bytes());
            chain.extend(r);
        }
        let runs = portal_runs(&chain, planes);
        assert_eq!(runs, vec![(0, 10)]);
        chain[12 * 5 + 4] = 99;
        assert!(portal_runs(&chain, planes).iter().all(|&(_, n)| n < 10));
        let zeros = vec![0u8; 36];
        assert!(trim_portal_run(&zeros).len() <= 12);
    }

    fn mdl_bytes(name: &str, len: usize) -> Vec<u8> {
        let mut m = vec![0u8; len];
        m[0..4].copy_from_slice(b"IDST");
        m[4..8].copy_from_slice(&49i32.to_le_bytes());
        m[12..12 + name.len()].copy_from_slice(name.as_bytes());
        m[76..80].copy_from_slice(&(len as i32).to_le_bytes());
        for (i, b) in m.iter_mut().enumerate().skip(80) {
            *b = (i % 251) as u8;
        }
        m
    }

    #[test]
    fn loaded_models_are_recovered_by_their_name() {
        use crate::assets::{find_loaded_models, mdl_header, normalize_model_name, recover_models_from_memory};
        let gnome = mdl_bytes("props_junk\\gnome.mdl", 2356);
        let other = mdl_bytes("Props_Doors/DoorMain01_Small.mdl", 1000);
        assert_eq!(mdl_header(&gnome[..80]), Some(("props_junk/gnome.mdl".to_owned(), 2356)));
        assert_eq!(normalize_model_name("models\\Props_Junk\\Gnome.mdl"), "props_junk/gnome.mdl");
        assert!(mdl_header(&other[..70]).is_none());
        let mut bad = gnome.clone();
        bad[4..8].copy_from_slice(&7i32.to_le_bytes());
        assert!(mdl_header(&bad[..80]).is_none());
        let mut mem = VecMem::default();
        let mut block = vec![0xeeu8; 64];
        block.extend(&gnome);
        block.extend(vec![0u8; 100]);
        block.extend(&other);
        mem.add(0x5000_0000, block);
        let wanted: std::collections::HashSet<String> = ["props_junk/gnome.mdl".to_owned()].into_iter().collect();
        let found = find_loaded_models(&mem, &ScanOptions::default(), &wanted);
        assert_eq!(found.len(), 1);
        assert_eq!(found["props_junk/gnome.mdl"], gnome);
        let dest = std::env::temp_dir().join(format!("hp_sniffer_models_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        let mut report = crate::assets::AssetReport {
            missing: vec!["models/props_junk/gnome.mdl".to_owned(), "materials/x.vmt".to_owned()],
            ..crate::assets::AssetReport::default()
        };
        let missing = report.missing.clone();
        recover_models_from_memory(&mem, &ScanOptions::default(), &missing, &dest, &mut report);
        assert_eq!((report.memory_models, report.memory_models_wanted), (1, 1));
        assert_eq!(std::fs::read(dest.join("models/props_junk/gnome.mdl")).expect("file"), gnome);
        assert_eq!(report.missing, vec!["materials/x.vmt".to_owned()]);
        let _ = std::fs::remove_dir_all(&dest);
    }
}
