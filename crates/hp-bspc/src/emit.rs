use ahash::AHashMap;
use glam::DVec3;

use crate::brush::MapBrush;
use crate::faces::Faces;
use crate::materials::{contents, surf};
use crate::planes::PlaneSet;
use crate::portals::Portals;
use crate::texinfo::TexInfos;
use crate::tree::Tree;

pub const BSP_VERSION: i32 = 20;
pub const LEAF_FLAGS_SKY: u16 = 0x01;
pub const LEAF_FLAGS_SKY2D: u16 = 0x04;
pub const LEAF_FLAGS_RADIAL: u16 = 0x02;

const MAX_DISP_LIGHTMAP_DIM: i32 = 125;

pub mod lump {
    pub const ENTITIES: usize = 0;
    pub const PLANES: usize = 1;
    pub const TEXDATA: usize = 2;
    pub const VERTEXES: usize = 3;
    pub const VISIBILITY: usize = 4;
    pub const NODES: usize = 5;
    pub const TEXINFO: usize = 6;
    pub const FACES: usize = 7;
    pub const LIGHTING: usize = 8;
    pub const OCCLUSION: usize = 9;
    pub const LEAFS: usize = 10;
    pub const FACEIDS: usize = 11;
    pub const EDGES: usize = 12;
    pub const SURFEDGES: usize = 13;
    pub const MODELS: usize = 14;
    pub const LEAFFACES: usize = 16;
    pub const LEAFBRUSHES: usize = 17;
    pub const BRUSHES: usize = 18;
    pub const BRUSHSIDES: usize = 19;
    pub const AREAS: usize = 20;
    pub const AREAPORTALS: usize = 21;
    pub const DISPINFO: usize = 26;
    pub const ORIGINALFACES: usize = 27;
    pub const PRIMITIVES: usize = 37;
    pub const PRIMINDICES: usize = 39;
    pub const PHYSDISP: usize = 28;
    pub const PHYSCOLLIDE: usize = 29;
    pub const VERTNORMALS: usize = 30;
    pub const VERTNORMALINDICES: usize = 31;
    pub const DISP_VERTS: usize = 33;
    pub const DISP_LIGHTMAP_SAMPLE_POSITIONS: usize = 34;
    pub const GAME_LUMP: usize = 35;
    pub const LEAFWATERDATA: usize = 36;
    pub const PAKFILE: usize = 40;
    pub const CLIPPORTALVERTS: usize = 41;
    pub const CUBEMAPS: usize = 42;
    pub const TEXDATA_STRING_DATA: usize = 43;
    pub const TEXDATA_STRING_TABLE: usize = 44;
    pub const OVERLAYS: usize = 45;
    pub const LEAFMINDISTTOWATER: usize = 46;
    pub const FACE_MACRO_TEXTURE_INFO: usize = 47;
    pub const DISP_TRIS: usize = 48;
    pub const LEAF_AMBIENT_INDEX_HDR: usize = 51;
    pub const LEAF_AMBIENT_INDEX: usize = 52;
    pub const LIGHTING_HDR: usize = 53;
    pub const LEAF_AMBIENT_LIGHTING_HDR: usize = 55;
    pub const LEAF_AMBIENT_LIGHTING: usize = 56;
    pub const FACES_HDR: usize = 58;
    pub const MAP_FLAGS: usize = 59;
    pub const OVERLAY_FADES: usize = 60;
    pub const OVERLAY_SYSTEM_LEVELS: usize = 61;
    pub const WORLDLIGHTS: usize = 15;
    pub const WORLDLIGHTS_HDR: usize = 54;
}

#[derive(Default)]
pub struct Buf(pub Vec<u8>);

impl Buf {
    pub fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn vec3(&mut self, v: DVec3) {
        self.f32(v.x as f32);
        self.f32(v.y as f32);
        self.f32(v.z as f32);
    }
}

pub const MAX_MAP_VERTS: usize = hp_bsp::limits::VERTEXES.max_usize();
pub const MAX_MAP_PRIMITIVES: usize = hp_bsp::limits::PRIMITIVES.max_usize();
pub const MAX_MAP_PRIMINDICES: usize = hp_bsp::limits::PRIMINDICES.max_usize();

fn clamp_i16(v: f64) -> i16 {
    v.clamp(-32768.0, 32767.0) as i16
}

pub fn vertex_normals(planes: &[u8], faces: &[u8], surfedges: &[u8], edges: &[u8]) -> (Vec<u8>, Vec<u8>) {
    vertex_normals_impl(planes, faces, surfedges, edges)
}

pub fn face_macro_textures(
    data: &mut AHashMap<usize, (Vec<u8>, u32)>,
    macro_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<u8> {
    let get = |data: &AHashMap<usize, (Vec<u8>, u32)>, i: usize| data.get(&i).map_or(Vec::new(), |(d, _)| d.clone());
    let (faces, texinfo, texdata) = (get(data, lump::FACES), get(data, lump::TEXINFO), get(data, lump::TEXDATA));
    let mut strings = get(data, lump::TEXDATA_STRING_DATA);
    let mut table = get(data, lump::TEXDATA_STRING_TABLE);
    let i32_at = |d: &[u8], o: usize| d.get(o..o + 4).map_or(-1, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let string_at = |strings: &[u8], o: usize| -> String {
        let s = strings.get(o..).unwrap_or(&[]);
        String::from_utf8_lossy(&s[..s.iter().position(|c| *c == 0).unwrap_or(s.len())]).into_owned()
    };
    let mut out = Buf::default();
    let mut added = false;
    for rec in faces.chunks_exact(56) {
        let ti = i16::from_le_bytes([rec[10], rec[11]]);
        let td = if ti >= 0 { i32_at(&texinfo, ti as usize * 72 + 68) } else { -1 };
        let name_id = if td >= 0 { i32_at(&texdata, td as usize * 32 + 12) } else { -1 };
        let name = if name_id >= 0 {
            string_at(&strings, i32_at(&table, name_id as usize * 4).max(0) as usize)
        } else {
            String::new()
        };
        let id = match (td >= 0).then(|| macro_of(&name)).flatten() {
            Some(m) => {
                let count = table.len() / 4;
                let found = (0..count)
                    .find(|&k| string_at(&strings, i32_at(&table, k * 4).max(0) as usize).eq_ignore_ascii_case(&m));
                found.unwrap_or_else(|| {
                    table.extend_from_slice(&(strings.len() as i32).to_le_bytes());
                    strings.extend_from_slice(m.as_bytes());
                    strings.push(0);
                    added = true;
                    count
                }) as u16
            }
            None => 0xFFFF,
        };
        out.u16(id);
    }
    if added {
        data.insert(lump::TEXDATA_STRING_DATA, (strings, 0));
        data.insert(lump::TEXDATA_STRING_TABLE, (table, 0));
    }
    out.0
}

pub fn flat_vertex_normals(planes: &[u8], faces: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut normals = Buf::default();
    let mut indices = Buf::default();
    for (k, rec) in faces.chunks_exact(56).enumerate() {
        let plane = usize::from(u16::from_le_bytes([rec[0], rec[1]]));
        let edges = i16::from_le_bytes([rec[8], rec[9]]).max(0);
        for _ in 0..edges {
            indices.u16(k as u16);
        }
        for c in 0..3 {
            let o = plane * 20 + c * 4;
            normals.0.extend_from_slice(planes.get(o..o + 4).unwrap_or(&[0; 4]));
        }
    }
    (normals.0, indices.0)
}

fn vertex_normals_impl(planes: &[u8], faces: &[u8], surfedges: &[u8], edges: &[u8]) -> (Vec<u8>, Vec<u8>) {
    #[allow(clippy::approx_constant)]
    const SMOOTHING_THRESHOLD: f32 = 0.707_106_7;
    const HARD_EDGE: u32 = 0x8000_0000;
    let rd = |d: &[u8], o: usize, n: usize| d.get(o..o + n).map(<[u8]>::to_vec).unwrap_or_else(|| vec![0; n]);
    let i32_at = |d: &[u8], o: usize| i32::from_le_bytes(rd(d, o, 4).try_into().unwrap_or([0; 4]));
    let u32_at = |d: &[u8], o: usize| u32::from_le_bytes(rd(d, o, 4).try_into().unwrap_or([0; 4]));
    let u16_at = |d: &[u8], o: usize| u16::from_le_bytes(rd(d, o, 2).try_into().unwrap_or([0; 2]));
    let i16_at = |d: &[u8], o: usize| i16::from_le_bytes(rd(d, o, 2).try_into().unwrap_or([0; 2]));
    let f32_at = |d: &[u8], o: usize| f32::from_le_bytes(rd(d, o, 4).try_into().unwrap_or([0; 4]));
    struct F {
        verts: Vec<u32>,
        normal: [f32; 3],
        disp: bool,
        smoothing: u32,
    }
    let list: Vec<F> = (0..faces.len() / 56)
        .map(|i| {
            let o = i * 56;
            let plane = u16_at(faces, o) as usize * 20;
            let (first, count) = (i32_at(faces, o + 4).max(0) as usize, i16_at(faces, o + 8).max(0) as usize);
            let verts = (first..first + count)
                .map(|e| {
                    let se = i32_at(surfedges, e * 4);
                    let edge = se.unsigned_abs() as usize * 4;
                    u32::from(if se < 0 { u16_at(edges, edge + 2) } else { u16_at(edges, edge) })
                })
                .collect();
            F {
                verts,
                normal: [f32_at(planes, plane), f32_at(planes, plane + 4), f32_at(planes, plane + 8)],
                disp: i16_at(faces, o + 12) != -1,
                smoothing: u32_at(faces, o + 52),
            }
        })
        .collect();
    let mut vertex_faces: AHashMap<u32, Vec<usize>> = AHashMap::new();
    for (i, f) in list.iter().enumerate() {
        for v in &f.verts {
            let l = vertex_faces.entry(*v).or_default();
            if !l.contains(&i) {
                l.push(i);
            }
        }
    }
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut normals = Buf::default();
    let mut indices = Buf::default();
    let mut unique: AHashMap<[u32; 3], u16> = AHashMap::new();
    for (i, f) in list.iter().enumerate() {
        for v in &f.verts {
            let mut n = [0.0f32; 3];
            for &k in vertex_faces.get(v).into_iter().flatten() {
                let other = &list[k];
                if k == i || (!f.disp && other.disp) {
                    continue;
                }
                let add = if f.disp {
                    true
                } else if f.smoothing == 0 && other.smoothing == 0 {
                    dot(other.normal, f.normal) >= SMOOTHING_THRESHOLD
                } else {
                    let g = f.smoothing & other.smoothing;
                    g & HARD_EDGE == 0 && g != 0
                };
                if add {
                    for (nc, oc) in n.iter_mut().zip(other.normal) {
                        *nc += oc;
                    }
                }
            }
            for (nc, fc) in n.iter_mut().zip(f.normal) {
                *nc += fc;
            }
            let len = dot(n, n).sqrt();
            if len != 0.0 {
                for c in &mut n {
                    *c /= len;
                }
            }
            let n = n.map(|c| c + 0.0);
            let next = unique.len() as u16;
            let at = *unique.entry(n.map(f32::to_bits)).or_insert_with(|| {
                for c in n {
                    normals.f32(c);
                }
                next
            });
            indices.u16(at);
        }
    }
    (normals.0, indices.0)
}

pub struct ModelOut<'a> {
    pub tree: &'a Tree,
    pub portals: &'a Portals,
    pub faces: &'a mut Faces,
    pub origin: DVec3,
    pub detail_holder: Option<usize>,
    pub disps: &'a [crate::disp::DispFace],
    pub bounds: (DVec3, DVec3),
}

type SideKey = (usize, usize);

struct OrigSide<'w> {
    key: SideKey,
    face_plane: usize,
    winding: &'w crate::winding::Winding,
    plane: usize,
    texinfo: i32,
    detail: bool,
}

#[derive(Default)]
pub struct Lumps {
    pub data: AHashMap<usize, (Vec<u8>, u32)>,
    pub format: hp_bsp::BspFormat,
    nodes: Buf,
    node_count: i32,
    leafs: Buf,
    leaf_count: i32,
    leaf_contents: Vec<u32>,
    faces: Buf,
    face_count: i32,
    faceids: Buf,
    leaffaces: Vec<u16>,
    leafbrushes: Vec<u16>,
    models: Buf,
    vertexes: Vec<DVec3>,
    vert_hash: Vec<u32>,
    vert_chain: Vec<u32>,
    model_vertex: Vec<u32>,
    prim_ids: Vec<Option<u16>>,
    edges: Vec<[u32; 2]>,
    vert_edges: AHashMap<u32, Vec<u32>>,
    edge_faces: Vec<(u32, bool)>,
    surfedges: Vec<i32>,
    orig_faces: Buf,
    orig_count: i32,
    orig_heads: AHashMap<usize, SideKey>,
    orig_sides: AHashMap<SideKey, (Option<SideKey>, i32)>,
    prims: Buf,
    prim_count: i32,
    prim_indices: Vec<u16>,
    pub prims_wanted: (usize, usize),
    pub errors: Vec<String>,
    pub dropped_vertices: usize,
    pub sky: bool,
    pub clusters: i32,
    pub game_lumps: Vec<(u32, u16, Vec<u8>)>,
    pub model_leaves: Vec<(usize, u16)>,
    pub side_faces: AHashMap<u32, Vec<u32>>,
    dispinfo: Vec<u8>,
    disp_verts: Vec<u8>,
    disp_tris: Vec<u8>,
    disp_multiblend: Vec<u8>,
    disp_samples: Vec<u8>,
    disp_count: i32,
}

impl Lumps {
    pub fn new() -> Self {
        let mut l = Lumps::default();
        l.edges.push([0, 0]);
        l.edge_faces.push((0, true));
        l.vert_hash = vec![0; 256 * 256];
        l.vertexes.push(DVec3::ZERO);
        l.vert_chain.push(0);
        l.leaf(contents::SOLID, 0, 0, 0, DVec3::ZERO, DVec3::ZERO, &[], &[], -1);
        l
    }

    pub fn vertex(&mut self, p: DVec3) -> u32 {
        let snap = |v: f64| {
            let r = f64::from((v + 0.5) as i32);
            if (v - r).abs() < 0.01 { r } else { v }
        };
        let p = DVec3::new(snap(p.x), snap(p.y), snap(p.z));
        let cell = |v: f64| ((16384 + (v + 0.5) as i32) >> 7).clamp(0, 255) as usize;
        let h = cell(p.y) * 256 + cell(p.x);
        if self.vert_hash.is_empty() {
            self.vert_hash = vec![0; 256 * 256];
        }
        let mut v = self.vert_hash[h];
        while v != 0 {
            if (self.vertexes[v as usize] - p).abs().max_element() < 0.1 {
                return v;
            }
            v = self.vert_chain[v as usize];
        }
        let i = self.vertexes.len() as u32;
        self.vertexes.push(p);
        self.vert_chain.push(self.vert_hash[h]);
        self.vert_hash[h] = i;
        i
    }

    pub fn begin_vertices(&mut self) {
        self.vert_hash = vec![0; 256 * 256];
    }

    fn begin_model(&mut self) {
        self.vert_hash = vec![0; 256 * 256];
        self.vert_edges.clear();
    }

    pub fn vertex_point(&self, i: u32) -> DVec3 {
        self.vertexes.get(i as usize).copied().unwrap_or(DVec3::ZERO)
    }

    fn edge(&mut self, a: u32, b: u32, contents: u32) -> i32 {
        if let Some(list) = self.vert_edges.get(&a) {
            for &e in list {
                let [v0, v1] = self.edges[e as usize];
                let (c, taken) = self.edge_faces[e as usize];
                if v1 == a && v0 == b && c == contents && !taken {
                    self.edge_faces[e as usize].1 = true;
                    return -(e as i32);
                }
            }
        }
        let e = self.edges.len() as u32;
        self.edges.push([a, b]);
        self.edge_faces.push((contents, false));
        self.vert_edges.entry(a).or_default().push(e);
        if b != a {
            self.vert_edges.entry(b).or_default().push(e);
        }
        e as i32
    }

    fn find_orig_face(&self, key: SideKey, face_plane: usize) -> Option<i32> {
        let mut cur = self.orig_heads.get(&face_plane).copied();
        while let Some(k) = cur {
            let (next, index) = self.orig_sides[&k];
            if k == key {
                return Some(index);
            }
            cur = next;
        }
        None
    }

    fn orig_face(&mut self, side: &OrigSide<'_>, contents: u32, disp: i16) -> i32 {
        if let Some(i) = self.find_orig_face(side.key, side.face_plane) {
            return i;
        }
        let w = side.winding;
        let index = self.orig_count;
        self.orig_count += 1;
        let next = self.orig_heads.insert(side.face_plane, side.key);
        self.orig_sides.insert(side.key, (next, index));
        let first_edge = self.surfedges.len() as i32;
        let verts: Vec<u32> = w.points.iter().map(|p| self.vertex(*p)).collect();
        for i in 0..verts.len() {
            let e = self.edge(verts[i], verts[(i + 1) % verts.len()], contents);
            self.surfedges.push(e);
        }
        let b = &mut self.orig_faces;
        b.u16(side.plane as u16);
        b.u8((side.plane & 1) as u8);
        b.u8(u8::from(!side.detail));
        b.i32(first_edge);
        b.i16(verts.len() as i16);
        b.i16(side.texinfo as i16);
        b.i16(disp);
        b.0.extend_from_slice(&[0; 2 + 4 + 4 + 4 + 16]);
        b.i32(-1);
        b.0.extend_from_slice(&[0; 8]);
        index
    }

    #[allow(clippy::too_many_arguments)]
    fn leaf(
        &mut self,
        c: u32,
        cluster: i16,
        area: u16,
        flags: u16,
        mins: DVec3,
        maxs: DVec3,
        faces: &[u16],
        brushes: &[u16],
        water: i16,
    ) {
        let b = &mut self.leafs;
        b.i32(c as i32);
        b.i16(cluster);
        b.u16((area & 0x1ff) | (flags << 9));
        for v in [mins, maxs] {
            b.i16(clamp_i16(v.x.trunc()));
            b.i16(clamp_i16(v.y.trunc()));
            b.i16(clamp_i16(v.z.trunc()));
        }
        b.u16(if c & contents::SOLID != 0 { 0 } else { self.leaffaces.len() as u16 });
        b.u16(faces.len() as u16);
        b.u16(self.leafbrushes.len() as u16);
        b.u16(brushes.len() as u16);
        b.i16(water);
        b.i16(0);
        self.leaffaces.extend_from_slice(faces);
        self.leafbrushes.extend_from_slice(brushes);
        self.leaf_contents.push(c);
        self.leaf_count += 1;
    }

    fn face(
        &mut self,
        f: &crate::faces::Face,
        on_node: bool,
        disp: i16,
        tex: &TexInfos,
        brushes: &[MapBrush],
        disp_side: Option<(OrigSide<'_>, u32)>,
    ) -> Option<usize> {
        let from_tree = disp < 0;
        if (from_tree && f.verts.len() < 3) || (!from_tree && f.winding.points.len() < 3) {
            return None;
        }
        if disp < 0 && tex.infos.get(f.texinfo as usize).is_some_and(|t| t.flags & crate::materials::surf::NODRAW != 0)
        {
            return None;
        }
        let (bi, si) = f.source;
        let brush = brushes.get(bi);
        let contents = match &disp_side {
            Some((_, c)) => *c,
            None => f.contents,
        };
        let orig = match &disp_side {
            Some((side, _)) => self.orig_face(side, contents, disp),
            None if f.piece => self.find_orig_face((bi, si), f.plane).unwrap_or(-1),
            None => match brush.and_then(|b| b.sides.get(si)) {
                Some(s) if s.winding.is_some() => {
                    let fw = s
                        .fwinding
                        .as_ref()
                        .map(|w| crate::winding::Winding::new(w.iter().map(|p| p.as_dvec3()).collect()));
                    let side = OrigSide {
                        key: (bi, si),
                        face_plane: f.plane,
                        winding: fw.as_ref().or(s.winding.as_ref()).unwrap_or(&f.winding),
                        plane: s.plane,
                        texinfo: s.texinfo,
                        detail: s.contents & crate::materials::contents::DETAIL != 0,
                    };
                    self.orig_face(&side, contents, disp)
                }
                _ => -1,
            },
        };
        let first_edge = self.surfedges.len() as i32;
        let verts: Vec<u32> = if from_tree {
            f.verts.iter().map(|&v| self.model_vertex[v as usize]).collect()
        } else {
            f.winding.points.iter().map(|p| self.vertex(*p)).collect()
        };
        let n = verts.len();
        for i in 0..n {
            let (a, b) = (verts[i], verts[(i + 1) % n]);
            if a == b && !from_tree {
                continue;
            }
            let e = self.edge(a, b, contents);
            self.surfedges.push(e);
        }
        let num_edges = self.surfedges.len() as i32 - first_edge;
        if num_edges < 3 {
            self.surfedges.truncate(first_edge as usize);
            return None;
        }
        let (num_prims, first_prim) = match f.prim.and_then(|k| self.prim_ids.get(k).copied().flatten()) {
            Some(id) => (1u16, id),
            None => (0, 0),
        };
        let info = tex.infos.get(f.texinfo as usize);
        let flags = info.map_or(0, |t| t.flags);
        if flags & surf::SKY != 0 {
            self.sky = true;
        }
        let (mut mins, mut size) = ([0i32; 2], [0i32; 2]);
        if let Some(t) = info
            && flags & (surf::NOLIGHT | surf::SKY) == 0
        {
            (mins, size) =
                crate::faces::lightmap_extents(verts.iter().map(|&v| self.vertexes[v as usize].as_vec3()), &t.lightmap);
            for (axis, &extent) in size.iter().enumerate() {
                let max_extent = hp_bsp::limits::LIGHTMAP_EXTENT.max_in(self.format.limits) as i32;
                if disp < 0 && extent > max_extent {
                    let (bi, _) = f.source;
                    let id = brushes.get(bi).map_or(0, |b| b.id);
                    let mut center = glam::Vec3::ZERO;
                    for &v in &verts {
                        center += self.vertexes[v as usize].as_vec3();
                    }
                    center /= verts.len().max(1) as f32;
                    let name =
                        usize::try_from(t.texdata).ok().and_then(|d| tex.data.get(d)).map_or("", |d| d.name.as_str());
                    self.errors.push(format!(
                        "Bad surface extents - surface is too big to have a lightmap
	material {name} around point ({:.1} {:.1} {:.1})
	(dimension: {axis}, {}>{}) (brush {id})",
                        center.x, center.y, center.z, extent, max_extent
                    ));
                }
            }
        }
        let index = self.face_count as usize;
        let b = &mut self.faces;
        b.u16(f.plane as u16);
        b.u8((f.plane & 1) as u8);
        b.u8(u8::from(on_node));
        b.i32(first_edge);
        b.i16(num_edges as i16);
        b.i16(f.texinfo as i16);
        b.i16(disp);
        b.i16(-1);
        b.u8(0);
        b.u8(0);
        b.u8(0);
        b.u8(0);
        b.i32(0);
        b.f32(if f.piece {
            0.0
        } else {
            crate::brushbsp::area(&f.winding.points.iter().map(|p| p.as_vec3()).collect())
        });
        b.i32(mins[0]);
        b.i32(mins[1]);
        b.i32(size[0]);
        b.i32(size[1]);
        b.i32(orig);
        b.u16(num_prims);
        b.u16(first_prim);
        b.u32(f.smoothing);
        let side_id = brushes.get(f.source.0).and_then(|b| b.sides.get(f.source.1)).map_or(0, |s| s.side_id);
        self.faceids.u16(side_id as u16);
        for (bi, si) in f.also.iter().copied().chain(std::iter::once(f.source)) {
            let sid = brushes.get(bi).and_then(|b| b.sides.get(si)).map_or(0, |s| s.side_id);
            if sid != 0 {
                let list = self.side_faces.entry(sid).or_default();
                if !list.contains(&(index as u32)) {
                    list.push(index as u32);
                }
            }
        }
        self.face_count += 1;
        Some(index)
    }

    fn disp_lightmap_extents(
        &mut self,
        fi: usize,
        corners: &[DVec3],
        start: DVec3,
        tex: &TexInfos,
        texinfo: i32,
        luxels: Option<[i32; 2]>,
    ) {
        let Some(info) = tex.infos.get(texinfo as usize) else { return };
        let Some(rec) = self.faces.0.get(fi * 56..fi * 56 + 56) else { return };
        let first = i32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]).max(0) as usize;
        let count = i16::from_le_bytes([rec[8], rec[9]]).max(0) as usize;
        let mut mins = [0i32; 2];
        if info.flags & (surf::NOLIGHT | surf::SKY) == 0 {
            for (axis, m) in mins.iter_mut().enumerate() {
                let v = DVec3::new(
                    f64::from(info.lightmap[axis][0]),
                    f64::from(info.lightmap[axis][1]),
                    f64::from(info.lightmap[axis][2]),
                );
                let o = f64::from(info.lightmap[axis][3]);
                let lo = self
                    .surfedges
                    .get(first..first + count)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|&e| {
                        let edge = self.edges.get(e.unsigned_abs() as usize)?;
                        self.vertexes.get(edge[usize::from(e < 0)] as usize)
                    })
                    .map(|p| p.dot(v) + o)
                    .fold(f64::INFINITY, f64::min);
                *m = if lo.is_finite() { lo.floor() as i32 } else { 0 };
            }
        }
        let fallback = || match <[DVec3; 4]>::try_from(corners) {
            Ok(c) => {
                let k = (0..4)
                    .min_by(|a, b| c[*a].distance_squared(start).total_cmp(&c[*b].distance_squared(start)))
                    .unwrap_or(0);
                let p = |i: usize| c[(k + i) % 4];
                let u = (p(3) - p(0)).length().max((p(2) - p(1)).length());
                let v = (p(1) - p(0)).length().max((p(2) - p(3)).length());
                let axis = DVec3::new(
                    f64::from(info.lightmap[0][0]),
                    f64::from(info.lightmap[0][1]),
                    f64::from(info.lightmap[0][2]),
                );
                let units = if axis.length() > 0.0 { (1.0 / axis.length()) as i32 } else { 0 };
                if units <= 0 {
                    [0, 0]
                } else {
                    let luxels = |len: f64| ((len / f64::from(units)) as i32 + 1).min(MAX_DISP_LIGHTMAP_DIM);
                    [luxels(u), luxels(v)]
                }
            }
            Err(_) => [0, 0],
        };
        let size = luxels.unwrap_or_else(fallback);
        let rec = &mut self.faces.0[fi * 56..fi * 56 + 56];
        rec[28..32].copy_from_slice(&mins[0].to_le_bytes());
        rec[32..36].copy_from_slice(&mins[1].to_le_bytes());
        rec[36..40].copy_from_slice(&size[0].to_le_bytes());
        rec[40..44].copy_from_slice(&size[1].to_le_bytes());
    }

    fn register_vertices(&mut self, faces: &Faces) {
        self.model_vertex = Vec::with_capacity(faces.vertices.len());
        for &p in &faces.vertices {
            let v = self.vertex(p);
            self.model_vertex.push(v);
        }
        self.prim_ids.clear();
        for list in &faces.prim_list {
            self.prims_wanted.0 += 1;
            self.prims_wanted.1 += list.len();
            let start = self.prim_indices.len();
            let fits = (self.prim_count as usize) < MAX_MAP_PRIMITIVES
                && start + list.len() <= MAX_MAP_PRIMINDICES
                && start <= usize::from(u16::MAX)
                && !list.is_empty();
            if !fits {
                self.prim_ids.push(None);
                continue;
            }
            self.prim_ids.push(Some(self.prim_count as u16));
            self.prim_count += 1;
            let b = &mut self.prims;
            b.u8(0);
            b.u8(0);
            b.u16(start as u16);
            b.u16(list.len() as u16);
            b.u16(0);
            b.u16(0);
            self.prim_indices.extend_from_slice(list);
        }
    }

    pub fn prims_over_limit(&self) -> bool {
        self.prims_wanted.0 > self.prim_count as usize || self.prims_wanted.1 > self.prim_indices.len()
    }

    pub fn model(&mut self, m: ModelOut<'_>, _planes: &PlaneSet, tex: &TexInfos, brushes: &[MapBrush]) {
        let tree = m.tree;
        let first_face = self.face_count;
        let head_index = self.node_count;
        self.model_leaves.clear();
        self.begin_model();
        self.register_vertices(m.faces);
        if let Some(h) = m.detail_holder {
            for &fid in &tree.nodes[h].faces {
                if m.faces.list[fid].live() && m.faces.list[fid].output.is_none() {
                    let out = self.face(&m.faces.list[fid], false, -1, tex, brushes, None);
                    m.faces.list[fid].output = out;
                }
            }
        }
        if tree.nodes[tree.head].is_leaf() {
            self.errors.push("a brush model has no node".to_owned());
        }
        enum Item {
            Node(usize, Option<(usize, usize)>),
        }
        let mut stack = vec![Item::Node(tree.head, None)];
        let mut patch: Vec<(usize, usize, i32)> = Vec::new();
        while let Some(Item::Node(n, parent)) = stack.pop() {
            let node = &tree.nodes[n];
            if node.is_leaf() {
                let leaf_index = self.leaf_count;
                self.model_leaves.push((n, leaf_index as u16));
                self.emit_leaf(tree, m.portals, m.faces, n, brushes);
                if let Some((pn, slot)) = parent {
                    patch.push((pn, slot, -(leaf_index + 1)));
                }
                continue;
            }
            let my = self.node_count as usize;
            self.node_count += 1;
            if let Some((pn, slot)) = parent {
                patch.push((pn, slot, my as i32));
            }
            let first = self.face_count;
            for &fid in &node.faces {
                if !m.faces.list[fid].live() {
                    continue;
                }
                if m.faces.list[fid].output.is_some() {
                    continue;
                }
                let out = self.face(&m.faces.list[fid], true, -1, tex, brushes, None);
                m.faces.list[fid].output = out;
            }
            let (lo, hi) = (node.mins, node.maxs);
            let b = &mut self.nodes;
            let at = b.0.len();
            b.i32(node.plane.unwrap_or(0) as i32);
            b.i32(0);
            b.i32(0);
            for v in [lo.trunc(), hi.trunc()] {
                b.i16(clamp_i16(v.x));
                b.i16(clamp_i16(v.y));
                b.i16(clamp_i16(v.z));
            }
            b.u16(first as u16);
            b.u16((self.face_count - first) as u16);
            b.i16(0);
            b.i16(0);
            debug_assert_eq!(at, my * 32);
            let [c0, c1] = node.children;
            stack.push(Item::Node(c1, Some((my, 1))));
            stack.push(Item::Node(c0, Some((my, 0))));
        }
        for (node, slot, value) in patch {
            let off = node * 32 + 4 + slot * 4;
            self.nodes.0[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
        let (cores, built) = {
            use rayon::prelude::*;
            let cores: Vec<Option<crate::disp::vbsp::Core>> = m.disps.par_iter().map(crate::disp::vbsp::core).collect();
            let built: Vec<_> = m
                .disps
                .par_iter()
                .map(|d| {
                    let data = crate::disp::disp_data(d, DVec3::ZERO);
                    let lightmap = data
                        .as_ref()
                        .and_then(|_| tex.infos.get(d.texinfo as usize))
                        .and_then(|t| crate::disp::lightmap(d, &t.lightmap));
                    (data, lightmap)
                })
                .collect();
            (cores, built)
        };
        let neighbors = crate::disp::vbsp::neighbors(&cores);
        drop(cores);
        for ((di, d), (data, lightmap)) in m.disps.iter().enumerate().zip(built) {
            let Some((verts, tags)) = data else {
                self.errors.push(format!("Brush {}: displacement side {} cannot be built", d.ids.0, d.ids.1));
                continue;
            };
            let index = self.disp_count;
            let face = crate::faces::Face {
                plane: d.plane,
                texinfo: d.texinfo,
                winding: d.winding.clone(),
                contents: 0,
                on_node: 0,
                merged: None,
                split: None,
                output: None,
                source: (usize::MAX, 0),
                also: Vec::new(),
                verts: Vec::new(),
                piece: false,
                prim: None,
                smoothing: 0,
            };
            let side = OrigSide {
                key: (usize::MAX, d.ids.1 as usize),
                face_plane: d.plane,
                winding: &d.winding,
                plane: d.plane,
                texinfo: d.texinfo,
                detail: d.contents & crate::materials::contents::DETAIL != 0,
            };
            let Some(fi) = self.face(&face, false, index as i16, tex, brushes, Some((side, d.contents))) else {
                continue;
            };
            let samples_start = self.disp_samples.len() as i32;
            if let Some((_, samples)) = &lightmap {
                self.disp_samples.extend_from_slice(samples);
            }
            self.disp_lightmap_extents(
                fi,
                &d.winding.points,
                d.info.start_position,
                tex,
                face.texinfo,
                lightmap.map(|l| l.0),
            );
            self.side_faces.entry(d.ids.1).or_default().push(fi as u32);
            if let Some(id) = self.faceids.0.get_mut(fi * 2..fi * 2 + 2) {
                id.copy_from_slice(&(d.ids.1 as u16).to_le_bytes());
            }
            let multiblend = self.format.disp_multiblend_lump.and_then(|_| crate::disp::multiblend_data(d));
            let vert_start = (self.disp_verts.len() / 20) as i32;
            let tri_start = (self.disp_tris.len() / 2) as i32;
            let rec = crate::disp::dispinfo_record(
                d,
                DVec3::ZERO,
                vert_start,
                tri_start,
                fi as u16,
                samples_start,
                &neighbors[di],
                multiblend.is_some(),
            );
            self.dispinfo.extend_from_slice(&rec);
            self.disp_verts.extend_from_slice(&verts);
            if let Some(blend) = multiblend {
                self.disp_multiblend.extend_from_slice(&blend);
            }
            for t in tags {
                self.disp_tris.extend_from_slice(&t.to_le_bytes());
            }
            self.disp_count += 1;
        }
        let (lo, hi) = m.bounds;
        let b = &mut self.models;
        b.vec3(lo);
        b.vec3(hi);
        b.vec3(DVec3::ZERO);
        b.i32(head_index);
        b.i32(first_face);
        b.i32(self.face_count - first_face);
    }

    fn emit_leaf(&mut self, tree: &Tree, portals: &Portals, faces: &Faces, n: usize, brushes: &[MapBrush]) {
        let node = &tree.nodes[n];
        let mut leaf_faces: Vec<u16> = Vec::new();
        if node.contents & contents::SOLID == 0 {
            let mut finals = Vec::new();
            for &pid in &node.portals {
                let p = &portals.list[pid];
                let s = usize::from(p.nodes[1] == n);
                if let Some(f) = p.face[s] {
                    faces.final_faces(f, &mut finals);
                }
            }
            for &f in &node.detail_faces {
                faces.final_faces(f, &mut finals);
            }
            for f in finals {
                if let Some(o) = faces.list[f].output
                    && !leaf_faces.contains(&(o as u16))
                {
                    leaf_faces.push(o as u16);
                }
            }
        }
        let mut leaf_brushes: Vec<u16> = Vec::new();
        let originals = node.brushes.iter().map(|b| b.original).chain(node.extra_brushes.iter().copied());
        for b in originals {
            if b < brushes.len() && !leaf_brushes.contains(&(b as u16)) {
                leaf_brushes.push(b as u16);
            }
        }
        let (lo, hi) = (node.mins, node.maxs);
        let area = node.area.clamp(0, 0x1ff) as u16;
        let flags = LEAF_FLAGS_SKY;
        let water = node.water.map_or(-1, |w| w as i16);
        self.leaf(node.contents, node.cluster as i16, area, flags, lo, hi, &leaf_faces, &leaf_brushes, water);
    }

    pub fn brushes(&mut self, brushes: &[MapBrush], planes: &mut PlaneSet) {
        let mut b = Buf::default();
        let mut s = Buf::default();
        let mut side_count = 0i32;
        for br in brushes {
            let first = side_count;
            let mut count = br.sides.len() as i32;
            for side in &br.sides {
                s.u16(side.plane as u16);
                s.i16(side.texinfo as i16);
                s.i16(0);
                s.u8(u8::from(side.bevel));
                s.u8(0);
                side_count += 1;
            }
            let (mins, maxs) = crate::brushbsp::map_brush_bounds(br);
            let last_tex = br.sides.last().map_or(0, |x| x.texinfo);
            for x in 0..3 {
                for dir in [-1.0f32, 1.0] {
                    let mut n = glam::Vec3::ZERO;
                    n[x] = dir;
                    let dist = if dir < 0.0 { -mins[x] } else { maxs[x] };
                    let p = planes.find_f32(n, dist);
                    if br.sides.iter().any(|side| side.plane == p) {
                        continue;
                    }
                    s.u16(p as u16);
                    s.i16(last_tex as i16);
                    s.i16(0);
                    s.u8(0);
                    s.u8(0);
                    side_count += 1;
                    count += 1;
                }
            }
            b.i32(first);
            b.i32(count);
            b.i32(br.contents as i32);
        }
        self.data.insert(lump::BRUSHES, (b.0, 0));
        self.data.insert(lump::BRUSHSIDES, (s.0, 0));
    }

    pub fn finish(&mut self, planes: &PlaneSet, tex: &TexInfos) {
        let mut b = Buf::default();
        for p in planes.all() {
            b.vec3(p.normal);
            b.f32(p.dist as f32);
            b.i32(p.kind);
        }
        self.data.insert(lump::PLANES, (b.0, 0));
        let n = tex.infos.len();
        let i16_at = |d: &[u8], o: usize| i16::from_le_bytes([d[o], d[o + 1]]);
        let mut refs = vec![0u32; n];
        for rec in self.faces.0.chunks_exact(56) {
            let t = i16_at(rec, 10);
            if t >= 0 && (t as usize) < n {
                refs[t as usize] += 1;
            }
        }
        let surfaceprop = |t: usize| {
            usize::try_from(tex.infos[t].texdata)
                .ok()
                .and_then(|d| tex.data.get(d))
                .map(|d| d.has_surfaceprop.then_some(d.surfaceprop.as_str()))
        };
        if let Some((sides, _)) = self.data.get_mut(&lump::BRUSHSIDES) {
            for rec in sides.chunks_exact_mut(8) {
                let mut t = i16_at(rec, 2);
                if t < 0 || t as usize >= n || refs[t as usize] > 0 {
                    continue;
                }
                let (flags, prop) = (tex.infos[t as usize].flags, surfaceprop(t as usize));
                if let Some(j) = (0..n).find(|&j| refs[j] > 0 && tex.infos[j].flags == flags && surfaceprop(j) == prop)
                {
                    t = j as i16;
                    rec[2..4].copy_from_slice(&t.to_le_bytes());
                }
                if refs[t as usize] == 0 {
                    refs[t as usize] += 1;
                }
            }
        }
        for (l, size, at) in [(lump::OVERLAYS, 352, 4), (lump::LEAFWATERDATA, 12, 8)] {
            for rec in self.data.get(&l).map_or(&[][..], |(d, _)| d.as_slice()).chunks_exact(size) {
                let t = i16_at(rec, at);
                if t >= 0 && (t as usize) < n {
                    refs[t as usize] += 1;
                }
            }
        }
        let mut data_used = vec![false; tex.data.len()];
        for (i, info) in tex.infos.iter().enumerate() {
            if refs[i] > 0
                && let Some(u) = usize::try_from(info.texdata).ok().and_then(|d| data_used.get_mut(d))
            {
                *u = true;
            }
        }
        let mut remap = vec![-1i16; n];
        let mut kept_infos: Vec<usize> = Vec::new();
        let (mut first_sky, mut first_sky2d) = (None, None);
        for i in 0..n {
            if refs[i] == 0 {
                continue;
            }
            let flags = tex.infos[i].flags;
            let slot = if flags & surf::SKY2D != 0 {
                &mut first_sky2d
            } else if flags & surf::SKY != 0 {
                &mut first_sky
            } else {
                kept_infos.push(i);
                remap[i] = (kept_infos.len() - 1) as i16;
                continue;
            };
            let out = *slot.get_or_insert_with(|| {
                kept_infos.push(i);
                (kept_infos.len() - 1) as i16
            });
            remap[i] = out;
        }
        let mut data_remap = vec![-1i32; tex.data.len()];
        let (mut d, mut table, mut strings) = (Buf::default(), Buf::default(), Vec::new());
        let mut names: Vec<(String, usize)> = Vec::new();
        let mut data_count = 0;
        for (i, t) in tex.data.iter().enumerate() {
            if !data_used[i] {
                continue;
            }
            data_remap[i] = data_count;
            data_count += 1;
            let key = t.name.to_ascii_lowercase();
            let string = match names.iter().position(|(k, _)| *k == key) {
                Some(s) => s,
                None => {
                    table.i32(strings.len() as i32);
                    strings.extend_from_slice(t.name.as_bytes());
                    strings.push(0);
                    names.push((key, names.len()));
                    names.len() - 1
                }
            };
            d.f32(t.reflectivity[0]);
            d.f32(t.reflectivity[1]);
            d.f32(t.reflectivity[2]);
            d.i32(string as i32);
            d.i32(t.width);
            d.i32(t.height);
            d.i32(t.width);
            d.i32(t.height);
        }
        let kept = kept_infos.len();
        let mut b = Buf::default();
        for &i in &kept_infos {
            let t = &tex.infos[i];
            for v in t.texture.iter().chain(t.lightmap.iter()) {
                for c in v {
                    b.f32(*c);
                }
            }
            b.i32(t.flags as i32);
            b.i32(usize::try_from(t.texdata).ok().and_then(|x| data_remap.get(x).copied()).unwrap_or(-1));
        }
        self.data.insert(lump::TEXINFO, (b.0, 0));
        for (which, size, at) in [
            (None, 56, 10),
            (Some(lump::BRUSHSIDES), 8, 2),
            (Some(lump::OVERLAYS), 352, 4),
            (Some(lump::LEAFWATERDATA), 12, 8),
        ] {
            let data = match which {
                None => Some(&mut self.faces.0),
                Some(l) => self.data.get_mut(&l).map(|(d, _)| d),
            };
            let Some(data) = data else { continue };
            for rec in data.chunks_exact_mut(size) {
                let t = i16_at(rec, at);
                if t >= 0 && (t as usize) < n {
                    rec[at..at + 2].copy_from_slice(&remap[t as usize].to_le_bytes());
                }
            }
        }
        self.data.insert(lump::TEXDATA, (d.0, 0));
        self.data.insert(lump::TEXDATA_STRING_TABLE, (table.0, 0));
        self.data.insert(lump::TEXDATA_STRING_DATA, (strings, 0));
        if self.vertexes.len() > MAX_MAP_VERTS {
            self.dropped_vertices = self.compact_vertices();
        }
        let mut b = Buf::default();
        for v in &self.vertexes {
            b.vec3(*v);
        }
        self.data.insert(lump::VERTEXES, (b.0, 0));
        let mut b = Buf::default();
        for e in &self.edges {
            b.u16(e[0] as u16);
            b.u16(e[1] as u16);
        }
        self.data.insert(lump::EDGES, (b.0, 0));
        let mut b = Buf::default();
        for e in &self.surfedges {
            b.i32(*e);
        }
        self.data.insert(lump::SURFEDGES, (b.0, 0));
        let faces = std::mem::take(&mut self.faces.0);
        self.data.insert(lump::ORIGINALFACES, (std::mem::take(&mut self.orig_faces.0), 0));
        self.data.insert(lump::PRIMITIVES, (std::mem::take(&mut self.prims.0), 0));
        self.data.insert(lump::PRIMINDICES, (self.prim_indices.iter().flat_map(|i| i.to_le_bytes()).collect(), 0));
        self.data.insert(lump::FACES, (faces, 1));
        self.data.insert(lump::FACEIDS, (std::mem::take(&mut self.faceids.0), 0));
        self.data.insert(lump::NODES, (std::mem::take(&mut self.nodes.0), 0));
        self.data.insert(lump::LEAFS, (std::mem::take(&mut self.leafs.0), 1));
        let mut b = Buf::default();
        for f in &self.leaffaces {
            b.u16(*f);
        }
        self.data.insert(lump::LEAFFACES, (b.0, 0));
        let mut b = Buf::default();
        for f in &self.leafbrushes {
            b.u16(*f);
        }
        self.data.insert(lump::LEAFBRUSHES, (b.0, 0));
        self.data.insert(lump::MODELS, (std::mem::take(&mut self.models.0), 0));
        if !self.data.contains_key(&lump::AREAS) {
            let mut b = Buf::default();
            b.i32(0);
            b.i32(0);
            b.i32(0);
            b.i32(1);
            self.data.insert(lump::AREAS, (b.0, 0));
            self.data.insert(lump::AREAPORTALS, (vec![0u8; 12], 0));
        }
        if !self.data.contains_key(&lump::OCCLUSION) {
            let mut b = Buf::default();
            b.i32(0);
            b.i32(0);
            b.i32(0);
            self.data.insert(lump::OCCLUSION, (b.0, 2));
        }
        let mut b = Buf::default();
        for _ in 0..self.leaf_count {
            b.u16(0);
        }
        self.data.insert(lump::LEAFMINDISTTOWATER, (b.0, 0));
        self.data.insert(lump::LIGHTING, (Vec::new(), 1));
        self.data.insert(lump::DISPINFO, (std::mem::take(&mut self.dispinfo), 0));
        self.data.insert(lump::DISP_VERTS, (std::mem::take(&mut self.disp_verts), 0));
        self.data.insert(lump::DISP_TRIS, (std::mem::take(&mut self.disp_tris), 0));
        if let Some(at) = self.format.disp_multiblend_lump
            && !self.disp_multiblend.is_empty()
        {
            self.data.insert(at, (std::mem::take(&mut self.disp_multiblend), 0));
        }
        self.data.insert(lump::DISP_LIGHTMAP_SAMPLE_POSITIONS, (std::mem::take(&mut self.disp_samples), 0));
        use hp_bsp::limits as lim;
        let records = |l: usize, size: usize| self.data.get(&l).map_or(0, |(d, _)| d.len() / size);
        let limits = [
            (lim::PLANES, planes.len()),
            (lim::VERTEXES, self.vertexes.len()),
            (lim::EDGES, self.edges.len()),
            (lim::SURFEDGES, self.surfedges.len()),
            (lim::FACES, self.face_count as usize),
            (lim::LEAFFACES, self.leaffaces.len()),
            (lim::LEAFBRUSHES, self.leafbrushes.len()),
            (lim::NODES, self.node_count as usize),
            (lim::LEAFS, self.leaf_count as usize),
            (lim::BRUSHES, records(lump::BRUSHES, 12)),
            (lim::BRUSHSIDES, records(lump::BRUSHSIDES, 8)),
            (lim::DISPINFO, self.disp_count as usize),
            (lim::TEXINFO, kept),
            (lim::TEXDATA, data_count as usize),
        ];
        for (limit, used) in limits {
            let (name, max) = (limit.valve, limit.max_usize_in(self.format.limits));
            if used > max {
                self.errors.push(format!("Error! {name} exceeded: {used} of {max}"));
            }
        }
    }

    fn compact_vertices(&mut self) -> usize {
        let n = self.vertexes.len();
        let mut used = vec![false; n];
        used[0] = true;
        for e in &self.edges {
            for v in e {
                if let Some(u) = used.get_mut(*v as usize) {
                    *u = true;
                }
            }
        }
        let occ_indices = self.data.get(&lump::OCCLUSION).and_then(|(d, _)| {
            let at = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]).max(0) as usize);
            let polys = 4 + at(0)? * 40;
            let indices = polys + 4 + at(polys)? * 12;
            let count = at(indices)?;
            (d.len() >= indices + 4 + count * 4).then_some((indices + 4, count))
        });
        if let (Some((start, count)), Some((d, _))) = (occ_indices, self.data.get(&lump::OCCLUSION)) {
            for k in 0..count {
                let o = start + k * 4;
                let v = i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
                if let Some(u) = usize::try_from(v).ok().and_then(|v| used.get_mut(v)) {
                    *u = true;
                }
            }
        }
        let mut remap = vec![0u32; n];
        let mut kept = Vec::with_capacity(n);
        for (i, p) in self.vertexes.iter().enumerate() {
            if used[i] {
                remap[i] = kept.len() as u32;
                kept.push(*p);
            }
        }
        let dropped = n - kept.len();
        if dropped == 0 {
            return 0;
        }
        self.vertexes = kept;
        for e in &mut self.edges {
            for v in e.iter_mut() {
                *v = remap.get(*v as usize).copied().unwrap_or(0);
            }
        }
        if let (Some((start, count)), Some((d, _))) = (occ_indices, self.data.get_mut(&lump::OCCLUSION)) {
            for k in 0..count {
                let o = start + k * 4;
                let v = i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
                let w = usize::try_from(v).ok().and_then(|v| remap.get(v)).map_or(v, |&r| r as i32);
                d[o..o + 4].copy_from_slice(&w.to_le_bytes());
            }
        }
        self.vert_hash = vec![0; 256 * 256];
        self.vert_chain = vec![0; self.vertexes.len()];
        self.vert_edges.clear();
        dropped
    }

    pub fn leaf_count(&self) -> usize {
        self.leaf_count as usize
    }

    pub fn file(&self, revision: i32) -> Vec<u8> {
        const HEADER: usize = 8 + 64 * 16 + 4;
        let mut header = Vec::with_capacity(HEADER);
        header.extend_from_slice(b"VBSP");
        header.extend_from_slice(&self.format.version.to_le_bytes());
        let mut body: Vec<u8> = Vec::new();
        let mut infos = [(0u32, 0u32, 0u32); 64];
        const ORDER: [usize; 55] = [
            1, 10, 56, 52, 51, 55, 3, 5, 6, 2, 26, 33, 48, 34, 63, 47, 37, 38, 39, 7, 58, 11, 27, 18, 19, 16, 17, 13,
            12, 14, 20, 21, 8, 53, 4, 0, 15, 54, 36, 9, 59, 41, 42, 43, 44, 45, 50, 60, 29, 28, 30, 31, 46, 35, 40,
        ];
        let mut order: Vec<usize> = ORDER
            .iter()
            .copied()
            .filter(|&i| i != lump::FACES_HDR || self.data.get(&i).is_some_and(|(d, _)| !d.is_empty()))
            .filter(|&i| !matches!(i, lump::PHYSCOLLIDE | lump::PHYSDISP) || self.data.contains_key(&i))
            .collect();
        if self.data.contains_key(&lump::OVERLAY_SYSTEM_LEVELS)
            && let Some(at) = order.iter().position(|&i| i == lump::OVERLAY_FADES)
        {
            order.insert(at + 1, lump::OVERLAY_SYSTEM_LEVELS);
        }
        let mut rest: Vec<usize> = self.data.keys().copied().filter(|i| !order.contains(i)).collect();
        rest.sort_unstable();
        order.extend(rest);
        let empty = (Vec::new(), 0u32);
        for i in order {
            let at = HEADER + body.len();
            let built;
            let (data, version) = if i == lump::GAME_LUMP {
                built = crate::props::game_lump(&self.game_lumps, at);
                (&built, &0u32)
            } else {
                let (d, v) = self.data.get(&i).unwrap_or(&empty);
                (d, v)
            };
            infos[i] = (at as u32, data.len() as u32, *version);
            body.extend_from_slice(data);
            while !(HEADER + body.len()).is_multiple_of(4) {
                body.push(0);
            }
        }
        for (off, len, ver) in infos {
            let fields = match self.format.layout {
                hp_bsp::LumpLayout::Standard => [off, len, ver],
                hp_bsp::LumpLayout::VersionFirst => [ver, off, len],
            };
            for f in fields {
                header.extend_from_slice(&f.to_le_bytes());
            }
            header.extend_from_slice(&[0u8; 4]);
        }
        header.extend_from_slice(&revision.to_le_bytes());
        header.extend_from_slice(&body);
        header
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestFace<'a> = (&'a [[f32; 3]], [f32; 3], u32);

    fn lumps(faces: &[TestFace]) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
        let (mut planes, mut fl, mut se, mut ed, mut verts) =
            (Buf::default(), Buf::default(), Buf::default(), Buf::default(), Vec::<[f32; 3]>::new());
        ed.u16(0);
        ed.u16(0);
        let mut edge_count = 1i32;
        for (i, (corners, normal, smoothing)) in faces.iter().enumerate() {
            planes.f32(normal[0]);
            planes.f32(normal[1]);
            planes.f32(normal[2]);
            planes.f32(0.0);
            planes.i32(0);
            let ids: Vec<u16> = corners
                .iter()
                .map(|c| match verts.iter().position(|v| v == c) {
                    Some(k) => k as u16,
                    None => {
                        verts.push(*c);
                        (verts.len() - 1) as u16
                    }
                })
                .collect();
            let first = se.0.len() / 4;
            for k in 0..ids.len() {
                ed.u16(ids[k]);
                ed.u16(ids[(k + 1) % ids.len()]);
                se.i32(edge_count);
                edge_count += 1;
            }
            fl.u16(i as u16);
            fl.u8(0);
            fl.u8(0);
            fl.i32(first as i32);
            fl.i16(ids.len() as i16);
            fl.i16(0);
            fl.i16(-1);
            fl.0.resize(fl.0.len() + 52 - 14, 0);
            fl.u32(*smoothing);
        }
        (planes.0, fl.0, se.0, ed.0)
    }

    fn normal_of(n: &[u8], ix: &[u8], corner: usize) -> [f32; 3] {
        let i = u16::from_le_bytes([ix[corner * 2], ix[corner * 2 + 1]]) as usize * 12;
        let f = |o: usize| f32::from_le_bytes([n[i + o], n[i + o + 1], n[i + o + 2], n[i + o + 3]]);
        [f(0), f(4), f(8)]
    }

    #[test]
    fn vertex_normals_like_vrad() {
        let floor: &[[f32; 3]] = &[[0.0, 0.0, 0.0], [0.0, 64.0, 0.0], [64.0, 64.0, 0.0], [64.0, 0.0, 0.0]];
        let wall: &[[f32; 3]] = &[[64.0, 0.0, 0.0], [64.0, 64.0, 0.0], [64.0, 64.0, 64.0], [64.0, 0.0, 64.0]];
        let ramp: &[[f32; 3]] = &[[64.0, 0.0, 0.0], [64.0, 64.0, 0.0], [128.0, 64.0, 32.0], [128.0, 0.0, 32.0]];
        let ramp_n = [-(1.0f32 / 5.0f32.sqrt()), 0.0, 2.0 / 5.0f32.sqrt()];
        let (p, f, se, ed) = lumps(&[(floor, [0.0, 0.0, 1.0], 0), (wall, [-1.0, 0.0, 0.0], 0)]);
        let (n, ix) = vertex_normals(&p, &f, &se, &ed);
        assert_eq!(ix.len(), 16);
        assert_eq!(n.len(), 24, "two unique normals");
        assert_eq!(normal_of(&n, &ix, 3), [0.0, 0.0, 1.0]);
        assert_eq!(normal_of(&n, &ix, 4), [-1.0, 0.0, 0.0]);
        let (p, f, se, ed) = lumps(&[(floor, [0.0, 0.0, 1.0], 0), (ramp, ramp_n, 0)]);
        let (n, ix) = vertex_normals(&p, &f, &se, &ed);
        let shared = normal_of(&n, &ix, 3);
        assert!(shared[0] < -0.2 && shared[2] > 0.9, "{shared:?}");
        assert_eq!(normal_of(&n, &ix, 0), [0.0, 0.0, 1.0]);
        let (p, f, se, ed) = lumps(&[(floor, [0.0, 0.0, 1.0], 1), (ramp, ramp_n, 2)]);
        let (n, ix) = vertex_normals(&p, &f, &se, &ed);
        assert_eq!(normal_of(&n, &ix, 3), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn unused_vertices_go_over_the_limit() {
        let mut l = Lumps::new();
        let p = |i: usize| DVec3::new((i % 200) as f64 * 3.0, (i / 200) as f64 * 3.0, 0.0);
        let ids: Vec<u32> = (0..MAX_MAP_VERTS + 100).map(|i| l.vertex(p(i))).collect();
        assert_eq!(l.vertexes.len(), MAX_MAP_VERTS + 101);
        let used: Vec<u32> = ids.iter().copied().step_by(10).collect();
        for t in used.chunks_exact(3) {
            for k in 0..3 {
                let e = l.edge(t[k], t[(k + 1) % 3], 0);
                l.surfedges.push(e);
            }
        }
        let occ = [ids[1], ids[2], ids[MAX_MAP_VERTS + 99]];
        let mut b = Buf::default();
        b.i32(1);
        b.0.extend_from_slice(&[0; 40]);
        b.i32(1);
        b.i32(0);
        b.i32(3);
        b.i32(0);
        b.i32(3);
        for v in occ {
            b.i32(v as i32);
        }
        l.data.insert(lump::OCCLUSION, (b.0, 2));
        let points: Vec<[DVec3; 2]> =
            l.edges.iter().map(|e| [l.vertexes[e[0] as usize], l.vertexes[e[1] as usize]]).collect();
        let dropped = l.compact_vertices();
        let kept = used.len() / 3 * 3 + occ.len() + 1;
        assert_eq!(l.vertexes.len(), kept);
        assert_eq!(dropped, MAX_MAP_VERTS + 101 - kept);
        assert!(l.vertexes.len() <= MAX_MAP_VERTS);
        for (e, want) in l.edges.iter().zip(&points) {
            assert_eq!([l.vertexes[e[0] as usize], l.vertexes[e[1] as usize]], *want);
        }
        let d = &l.data[&lump::OCCLUSION].0;
        let at = |o: usize| i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize;
        let start = 4 + 40 + 4 + 12 + 4;
        for (k, v) in occ.iter().enumerate() {
            assert_eq!(l.vertexes[at(start + k * 4)], p(*v as usize - 1));
        }
        assert_eq!(l.vertexes[1], p(0));
        assert_eq!(l.vertexes[2], p(1));
    }

    #[test]
    fn primitives_stay_within_the_engine_limits() {
        let mut faces = crate::faces::Faces::default();
        faces.prim_list = (0..3000).map(|k| (0..30).map(|i| ((i + k) % 30) as u16).collect()).collect();
        faces.prim_list.push(vec![0, 1, 2]);
        let mut l = Lumps::new();
        l.register_vertices(&faces);
        assert!(l.prims_over_limit());
        assert_eq!(l.prims_wanted, (3001, 90_003));
        let kept = MAX_MAP_PRIMINDICES / 30;
        assert_eq!(l.prim_count as usize, kept + 1);
        assert_eq!(l.prims.0.len(), (kept + 1) * 10);
        assert!(l.prim_indices.len() <= MAX_MAP_PRIMINDICES);
        let lists: Vec<&Vec<u16>> = faces.prim_list[..kept].iter().chain(faces.prim_list.last()).collect();
        for (rec, list) in l.prims.0.chunks_exact(10).zip(&lists) {
            let first = usize::from(u16::from_le_bytes([rec[2], rec[3]]));
            let count = usize::from(u16::from_le_bytes([rec[4], rec[5]]));
            assert_eq!(&l.prim_indices[first..first + count], list.as_slice());
        }
        assert!(l.prim_ids[..kept].iter().enumerate().all(|(k, id)| *id == Some(k as u16)));
        assert!(l.prim_ids[kept..3000].iter().all(Option::is_none));
        assert_eq!(l.prim_ids[3000], Some(kept as u16));
        faces.prim_list.truncate(100);
        let mut l = Lumps::new();
        l.register_vertices(&faces);
        assert!(!l.prims_over_limit());
        assert_eq!(l.prim_count, 100);
        assert!(l.prim_ids.iter().enumerate().all(|(k, id)| *id == Some(k as u16)));
    }
}
