use glam::DVec3;
use hp_bsp::{Bsp, lump};

use crate::DecompileError;

pub(crate) mod idx {
    pub const PLANES: usize = 1;
    pub const VERTEXES: usize = 3;
    pub const NODES: usize = 5;
    pub const TEXINFO: usize = 6;
    pub const OCCLUSION: usize = 9;
    pub const EDGES: usize = 12;
    pub const SURFEDGES: usize = 13;
    pub const LEAFBRUSHES: usize = 17;
    pub const BRUSHES: usize = 18;
    pub const BRUSHSIDES: usize = 19;
    pub const AREAPORTALS: usize = 21;
    pub const DISPINFO: usize = 26;
    pub const DISP_VERTS: usize = 33;
    pub const CLIPPORTALVERTS: usize = 41;
    pub const DISP_TRIS: usize = 48;
    pub const DISP_MULTIBLEND: usize = 63;
}

pub(crate) const DISP_HAS_MULTIBLEND: i32 = 0x4000_0000;

pub(crate) mod contents {
    pub const AREAPORTAL: i32 = 0x8000;
    pub const DETAIL: i32 = 0x0800_0000;
}

pub(crate) mod surf {
    pub const SKY: i32 = 0x4;
    pub const HINT: i32 = 0x100;
    pub const SKIP: i32 = 0x200;
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn i16_at(b: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([b[at], b[at + 1]])
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn vec_at(b: &[u8], at: usize) -> DVec3 {
    DVec3::new(f64::from(f32_at(b, at)), f64::from(f32_at(b, at + 4)), f64::from(f32_at(b, at + 8)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Plane {
    pub normal: DVec3,
    pub dist: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TexInfo {
    pub vecs: [[f64; 4]; 2],
    pub lightmap_scale: f64,
    pub flags: i32,
    pub texdata: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Face {
    pub planenum: u16,
    pub side: u8,
    pub firstedge: i32,
    pub numedges: i16,
    pub texinfo: i16,
    pub dispinfo: i16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Node {
    pub children: [i32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Leaf {
    pub first_brush: u16,
    pub num_brushes: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Model {
    pub origin: DVec3,
    pub headnode: i32,
    pub firstface: i32,
    pub numfaces: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Brush {
    pub firstside: i32,
    pub numsides: i32,
    pub contents: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BrushSide {
    pub planenum: u16,
    pub texinfo: i16,
    pub bevel: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DispInfo {
    pub start: DVec3,
    pub vert_start: i32,
    pub tri_start: i32,
    pub power: i32,
    pub min_tess: i32,
    pub map_face: u16,
    pub allowed_verts: [u32; 10],
    pub multiblend_start: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DispVert {
    pub vector: DVec3,
    pub dist: f64,
    pub alpha: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Overlay {
    pub id: i32,
    pub texinfo: i16,
    pub render_order: u16,
    pub faces: Vec<i32>,
    pub u: [f64; 2],
    pub v: [f64; 2],
    pub uv_points: [DVec3; 4],
    pub origin: DVec3,
    pub normal: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AreaPortal {
    pub key: u16,
    pub first_vert: u16,
    pub num_verts: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Occluder {
    pub mins: DVec3,
    pub maxs: DVec3,
    pub polys: Vec<(i32, Vec<i32>)>,
}

#[derive(Default)]
pub(crate) struct Lumps {
    pub planes: Vec<Plane>,
    pub texinfo: Vec<TexInfo>,
    pub texdata_names: Vec<String>,
    pub vertexes: Vec<DVec3>,
    pub edges: Vec<[u16; 2]>,
    pub surfedges: Vec<i32>,
    pub faces: Vec<Face>,
    pub nodes: Vec<Node>,
    pub leafs: Vec<Leaf>,
    pub leafbrushes: Vec<u16>,
    pub models: Vec<Model>,
    pub brushes: Vec<Brush>,
    pub brushsides: Vec<BrushSide>,
    pub dispinfo: Vec<DispInfo>,
    pub dispverts: Vec<DispVert>,
    pub disptris: Vec<u16>,
    pub dispmultiblend: Vec<[f32; 20]>,
    pub overlays: Vec<Overlay>,
    pub water_overlays: Vec<Overlay>,
    pub areaportals: Vec<AreaPortal>,
    pub clipportalverts: Vec<DVec3>,
    pub occluders: Vec<Occluder>,
}

fn records<T>(data: &[u8], size: usize, f: impl Fn(&[u8]) -> T) -> Vec<T> {
    data.chunks_exact(size).map(f).collect()
}

fn lump_bytes<'a>(
    bsp: &'a Bsp,
    index: usize,
    required: bool,
    warnings: &mut Vec<String>,
) -> Result<std::borrow::Cow<'a, [u8]>, DecompileError> {
    match bsp.lump(index) {
        Ok(d) => Ok(d),
        Err(e) if required => Err(DecompileError::Bsp(e)),
        Err(e) => {
            warnings.push(format!("lump {} ({}) could not be read: {e}", index, hp_bsp::lump_name(index)));
            Ok(std::borrow::Cow::Borrowed(&[]))
        }
    }
}

impl Lumps {
    pub(crate) fn read(bsp: &Bsp, warnings: &mut Vec<String>) -> Result<Lumps, DecompileError> {
        let mut l = Lumps::default();
        let planes = lump_bytes(bsp, idx::PLANES, true, warnings)?;
        l.planes = records(&planes, 20, |b| Plane { normal: vec_at(b, 0), dist: f64::from(f32_at(b, 12)) });
        let texinfo = lump_bytes(bsp, idx::TEXINFO, false, warnings)?;
        l.texinfo = records(&texinfo, 72, |b| {
            let mut vecs = [[0.0; 4]; 2];
            for (r, row) in vecs.iter_mut().enumerate() {
                for (c, v) in row.iter_mut().enumerate() {
                    *v = f64::from(f32_at(b, (r * 4 + c) * 4));
                }
            }
            let lm = DVec3::new(f64::from(f32_at(b, 32)), f64::from(f32_at(b, 36)), f64::from(f32_at(b, 40)));
            let len = lm.length();
            let lightmap_scale = if len.is_finite() && len > 1e-6 { 1.0 / len } else { 0.0 };
            TexInfo { vecs, lightmap_scale, flags: i32_at(b, 64), texdata: i32_at(b, 68) }
        });
        let names = bsp.texture_names().unwrap_or_else(|e| {
            warnings.push(format!("texture names could not be read: {e}"));
            Vec::new()
        });
        let texdata = lump_bytes(bsp, lump::TEXDATA, false, warnings)?;
        l.texdata_names = records(&texdata, 32, |b| {
            let id = i32_at(b, 12);
            usize::try_from(id).ok().and_then(|i| names.get(i)).cloned().unwrap_or_default()
        });
        let vertexes = lump_bytes(bsp, idx::VERTEXES, false, warnings)?;
        l.vertexes = records(&vertexes, 12, |b| vec_at(b, 0));
        let edges = lump_bytes(bsp, idx::EDGES, false, warnings)?;
        l.edges = records(&edges, 4, |b| [u16_at(b, 0), u16_at(b, 2)]);
        let surfedges = lump_bytes(bsp, idx::SURFEDGES, false, warnings)?;
        l.surfedges = records(&surfedges, 4, |b| i32_at(b, 0));
        let faces = lump_bytes(bsp, lump::FACES, false, warnings)?;
        l.faces = records(&faces, 56, |b| Face {
            planenum: u16_at(b, 0),
            side: b[2],
            firstedge: i32_at(b, 4),
            numedges: i16_at(b, 8),
            texinfo: i16_at(b, 10),
            dispinfo: i16_at(b, 12),
        });
        let nodes = lump_bytes(bsp, idx::NODES, false, warnings)?;
        l.nodes = records(&nodes, 32, |b| Node { children: [i32_at(b, 4), i32_at(b, 8)] });
        let leafs = lump_bytes(bsp, lump::LEAFS, false, warnings)?;
        let leaf_version = bsp.lump_info(lump::LEAFS).map(|i| i.version).unwrap_or(1);
        let leaf_size = if leaf_version == 0 && leafs.len() % 56 == 0 { 56 } else { 32 };
        l.leafs = records(&leafs, leaf_size, |b| Leaf { first_brush: u16_at(b, 24), num_brushes: u16_at(b, 26) });
        let leafbrushes = lump_bytes(bsp, idx::LEAFBRUSHES, false, warnings)?;
        l.leafbrushes = records(&leafbrushes, 2, |b| u16_at(b, 0));
        let models = lump_bytes(bsp, lump::MODELS, false, warnings)?;
        l.models = records(&models, 48, |b| Model {
            origin: vec_at(b, 24),
            headnode: i32_at(b, 36),
            firstface: i32_at(b, 40),
            numfaces: i32_at(b, 44),
        });
        let brushes = lump_bytes(bsp, idx::BRUSHES, true, warnings)?;
        l.brushes = records(&brushes, 12, |b| Brush {
            firstside: i32_at(b, 0),
            numsides: i32_at(b, 4),
            contents: i32_at(b, 8),
        });
        let sides = lump_bytes(bsp, idx::BRUSHSIDES, true, warnings)?;
        let side_total = i32::try_from(sides.len() / 8).unwrap_or(i32::MAX);
        let firsts: Vec<i32> = l.brushes.iter().map(|b| b.firstside).collect();
        for (k, b) in l.brushes.iter_mut().enumerate() {
            if b.numsides == 0 && b.firstside >= 0 {
                let end = firsts.get(k + 1).copied().unwrap_or(side_total);
                let n = end - b.firstside;
                if (4..=64).contains(&n) {
                    b.numsides = n;
                    warnings.push(format!("brush {k} stored no side count: {n} sides taken from the next brush"));
                }
            }
        }
        l.brushsides =
            records(&sides, 8, |b| BrushSide { planenum: u16_at(b, 0), texinfo: i16_at(b, 2), bevel: b[6] != 0 });
        let dispinfo = lump_bytes(bsp, idx::DISPINFO, false, warnings)?;
        l.dispinfo = records(&dispinfo, 176, |b| {
            let mut allowed_verts = [0u32; 10];
            for (k, v) in allowed_verts.iter_mut().enumerate() {
                *v = u32_at(b, 136 + k * 4);
            }
            DispInfo {
                start: vec_at(b, 0),
                vert_start: i32_at(b, 12),
                tri_start: i32_at(b, 16),
                power: i32_at(b, 20),
                min_tess: i32_at(b, 24),
                map_face: u16_at(b, 36),
                allowed_verts,
                multiblend_start: None,
            }
        });
        let mut next = 0usize;
        for d in &mut l.dispinfo {
            if d.min_tess < 0 && d.min_tess & DISP_HAS_MULTIBLEND != 0 && (2..=4).contains(&d.power) {
                let row = (1usize << d.power) + 1;
                d.multiblend_start = Some(next);
                next += row * row;
            }
        }
        if next > 0 {
            let blend = lump_bytes(bsp, idx::DISP_MULTIBLEND, false, warnings)?;
            l.dispmultiblend = records(&blend, 80, |b| std::array::from_fn(|k| f32_at(b, k * 4)));
            if l.dispmultiblend.len() < next {
                warnings.push(format!(
                    "the displacement multiblend lump holds {} vertices, {} are needed: 4-way blends were dropped",
                    l.dispmultiblend.len(),
                    next
                ));
                l.dispmultiblend.clear();
            }
        }
        let dispverts = lump_bytes(bsp, idx::DISP_VERTS, false, warnings)?;
        l.dispverts = records(&dispverts, 20, |b| DispVert {
            vector: vec_at(b, 0),
            dist: f64::from(f32_at(b, 12)),
            alpha: f64::from(f32_at(b, 16)),
        });
        let disptris = lump_bytes(bsp, idx::DISP_TRIS, false, warnings)?;
        l.disptris = records(&disptris, 2, |b| u16_at(b, 0));
        let overlays = lump_bytes(bsp, lump::OVERLAYS, false, warnings)?;
        l.overlays = records(&overlays, 352, |b| {
            let count_order = u16_at(b, 6);
            let count = usize::from(count_order & 0x3FFF).min(64);
            let faces = (0..count).map(|k| i32_at(b, 8 + k * 4)).collect();
            let mut uv_points = [DVec3::ZERO; 4];
            for (k, p) in uv_points.iter_mut().enumerate() {
                *p = vec_at(b, 280 + k * 12);
            }
            Overlay {
                id: i32_at(b, 0),
                texinfo: i16_at(b, 4),
                render_order: count_order >> 14,
                faces,
                u: [f64::from(f32_at(b, 264)), f64::from(f32_at(b, 268))],
                v: [f64::from(f32_at(b, 272)), f64::from(f32_at(b, 276))],
                uv_points,
                origin: vec_at(b, 328),
                normal: vec_at(b, 340),
            }
        });
        let water_overlays = lump_bytes(bsp, lump::WATEROVERLAYS, false, warnings)?;
        l.water_overlays = records(&water_overlays, 1120, |b| {
            let count_order = u16_at(b, 6);
            let count = usize::from(count_order & 0x3FFF).min(256);
            let faces = (0..count).map(|k| i32_at(b, 8 + k * 4)).collect();
            let mut uv_points = [DVec3::ZERO; 4];
            for (k, p) in uv_points.iter_mut().enumerate() {
                *p = vec_at(b, 1048 + k * 12);
            }
            Overlay {
                id: i32_at(b, 0),
                texinfo: i16_at(b, 4),
                render_order: count_order >> 14,
                faces,
                u: [f64::from(f32_at(b, 1032)), f64::from(f32_at(b, 1036))],
                v: [f64::from(f32_at(b, 1040)), f64::from(f32_at(b, 1044))],
                uv_points,
                origin: vec_at(b, 1096),
                normal: vec_at(b, 1108),
            }
        });
        let portals = lump_bytes(bsp, idx::AREAPORTALS, false, warnings)?;
        l.areaportals = records(&portals, 12, |b| AreaPortal {
            key: u16_at(b, 0),
            first_vert: u16_at(b, 4),
            num_verts: u16_at(b, 6),
        });
        let clip = lump_bytes(bsp, idx::CLIPPORTALVERTS, false, warnings)?;
        l.clipportalverts = records(&clip, 12, |b| vec_at(b, 0));
        let occlusion = lump_bytes(bsp, idx::OCCLUSION, false, warnings)?;
        let occlusion_version = bsp.lump_info(idx::OCCLUSION).map(|i| i.version).unwrap_or(2);
        l.occluders = read_occluders(&occlusion, occlusion_version).unwrap_or_else(|| {
            if !occlusion.is_empty() {
                warnings.push("the occluder lump could not be read".to_owned());
            }
            Vec::new()
        });
        Ok(l)
    }

    pub(crate) fn texinfo_name(&self, texinfo: i32) -> &str {
        usize::try_from(texinfo)
            .ok()
            .and_then(|i| self.texinfo.get(i))
            .and_then(|t| usize::try_from(t.texdata).ok())
            .and_then(|d| self.texdata_names.get(d))
            .map_or("", String::as_str)
    }

    pub(crate) fn plane(&self, index: i64) -> Option<Plane> {
        usize::try_from(index).ok().and_then(|i| self.planes.get(i)).copied()
    }

    pub(crate) fn face_vertices(&self, face: &Face) -> Vec<DVec3> {
        let n = usize::try_from(face.numedges).unwrap_or(0);
        let Ok(first) = usize::try_from(face.firstedge) else { return Vec::new() };
        let mut out = Vec::with_capacity(n.min(64));
        for k in 0..n {
            let Some(&se) = first.checked_add(k).and_then(|i| self.surfedges.get(i)) else { return Vec::new() };
            let edge = self.edges.get(se.unsigned_abs() as usize);
            let Some(edge) = edge else { return Vec::new() };
            let v = if se >= 0 { edge[0] } else { edge[1] };
            let Some(&p) = self.vertexes.get(usize::from(v)) else { return Vec::new() };
            out.push(p);
        }
        out
    }
}

fn read_occluders(data: &[u8], version: u32) -> Option<Vec<Occluder>> {
    if data.len() < 4 {
        return Some(Vec::new());
    }
    let mut at = 0usize;
    let mut next = |n: usize| -> Option<&[u8]> {
        let s = data.get(at..at.checked_add(n)?)?;
        at += n;
        Some(s)
    };
    let count = usize::try_from(i32_at(next(4)?, 0)).ok()?;
    let size = if version >= 1 { 40 } else { 36 };
    if count.checked_mul(size)? > data.len() {
        return None;
    }
    let mut occ = Vec::with_capacity(count);
    for _ in 0..count {
        let b = next(size)?;
        occ.push((i32_at(b, 4), i32_at(b, 8), vec_at(b, 12), vec_at(b, 24)));
    }
    let poly_count = usize::try_from(i32_at(next(4)?, 0)).ok()?;
    if poly_count.checked_mul(12)? > data.len() {
        return None;
    }
    let mut polys = Vec::with_capacity(poly_count);
    for _ in 0..poly_count {
        let b = next(12)?;
        polys.push((i32_at(b, 0), i32_at(b, 4), i32_at(b, 8)));
    }
    let index_count = usize::try_from(i32_at(next(4)?, 0)).ok()?;
    if index_count.checked_mul(4)? > data.len() {
        return None;
    }
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        indices.push(i32_at(next(4)?, 0));
    }
    let mut out = Vec::with_capacity(occ.len());
    for (first, n, mins, maxs) in occ {
        let mut list = Vec::new();
        let (Ok(first), Ok(n)) = (usize::try_from(first), usize::try_from(n)) else { continue };
        for p in polys.iter().skip(first).take(n) {
            let (Ok(fv), Ok(nv)) = (usize::try_from(p.0), usize::try_from(p.1)) else { continue };
            list.push((p.2, indices.iter().skip(fv).take(nv.min(64)).copied().collect()));
        }
        out.push(Occluder { mins, maxs, polys: list });
    }
    Some(out)
}
