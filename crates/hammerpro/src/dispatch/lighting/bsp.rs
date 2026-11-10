use std::collections::HashMap;
use std::sync::Arc;

use glam::{DVec3, Vec3};
use hp_render::lightmap::{LightmapVertex, face_flags, pack_blend};

pub(crate) mod lump {
    pub const PLANES: usize = 1;
    pub const TEXDATA: usize = 2;
    pub const VERTEXES: usize = 3;
    pub const TEXINFO: usize = 6;
    pub const FACES: usize = 7;
    pub const FACEIDS: usize = 11;
    pub const EDGES: usize = 12;
    pub const SURFEDGES: usize = 13;
    pub const MODELS: usize = 14;
    pub const DISPINFO: usize = 26;
    pub const DISP_VERTS: usize = 33;
    pub const STRING_DATA: usize = 43;
    pub const STRING_TABLE: usize = 44;
}

const FACE_SIZE: usize = 56;
const TEXINFO_SIZE: usize = 72;
const TEXDATA_SIZE: usize = 32;
const DISPINFO_SIZE: usize = 176;
const DISP_VERT_SIZE: usize = 20;
const MODEL_SIZE: usize = 48;
const PLANE_SIZE: usize = 20;
const MAX_SIDE: u32 = 512;
const MAX_FACE_EDGES: usize = 1024;

const UNLIT: i32 = 0x2 | 0x4 | 0x40 | 0x80 | 0x100 | 0x200 | 0x400;
const BUMPLIGHT: i32 = 0x800;

fn i32_at(d: &[u8], o: usize) -> Option<i32> {
    d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    d.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
}

fn f32_at(d: &[u8], o: usize) -> Option<f32> {
    d.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).filter(|v| v.is_finite())
}

fn vec_at(d: &[u8], o: usize) -> Option<DVec3> {
    Some(DVec3::new(f64::from(f32_at(d, o)?), f64::from(f32_at(d, o + 4)?), f64::from(f32_at(d, o + 8)?)))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SideRef {
    pub solid: u32,
    pub normal: DVec3,
    pub dist: f64,
    pub blend: Option<Arc<[u32]>>,
}

fn disp_blend(disp: &hp_vmf::DispInfo) -> Option<Arc<[u32]>> {
    let mb = hp_vmf::multiblend::MultiBlend::of(disp)?;
    let packed: Vec<u32> = mb.weights.iter().map(|w| pack_blend(w.map(|v| v as f32))).collect();
    Some(packed.into())
}

#[derive(Debug, Default)]
pub(crate) struct SideTable {
    sides: HashMap<u16, Vec<SideRef>>,
}

impl SideTable {
    pub fn from_map(map: &hp_vmf::Map) -> Self {
        let mut sides: HashMap<u16, Vec<SideRef>> = HashMap::new();
        let world = map.world.solids.iter();
        let entities = map.entities.iter().filter(|e| !e.hidden).flat_map(|e| e.solids.iter());
        for solid in world.chain(entities).filter(|s| !s.hidden) {
            for side in &solid.sides {
                let [a, b, c] = side.plane;
                let Some(normal) = (a - b).cross(c - b).try_normalize() else { continue };
                let dist = normal.dot(a);
                if !dist.is_finite() {
                    continue;
                }
                let blend = side.disp.as_deref().and_then(disp_blend);
                sides.entry(side.id as u16).or_default().push(SideRef { solid: solid.id, normal, dist, blend });
            }
        }
        Self { sides }
    }

    fn get(&self, low: u16) -> &[SideRef] {
        self.sides.get(&low).map_or(&[], Vec::as_slice)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SolidInfo {
    pub lo: Vec3,
    pub hi: Vec3,
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Vert {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub lm: [f32; 2],
    pub alpha: f32,
    pub blend: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GeoFace {
    pub bsp: u32,
    pub solid: u32,
    pub generation: u64,
    pub material: u32,
    pub w: u32,
    pub h: u32,
    pub scale: f32,
    pub flags: u32,
    pub bumped: bool,
    pub vertices: Vec<Vert>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Default)]
pub(crate) struct Geometry {
    pub faces: Vec<GeoFace>,
    pub materials: Vec<String>,
    pub unbound: usize,
}

struct Lumps<'a> {
    planes: &'a [u8],
    vertexes: &'a [u8],
    edges: &'a [u8],
    surfedges: &'a [u8],
    faces: &'a [u8],
    texinfo: &'a [u8],
    texdata: &'a [u8],
    string_table: &'a [u8],
    string_data: &'a [u8],
    models: &'a [u8],
    dispinfo: &'a [u8],
    disp_verts: &'a [u8],
    faceids: &'a [u8],
}

#[derive(Clone, Copy)]
struct Proj {
    tex: [[f64; 4]; 2],
    lm: [[f64; 4]; 2],
}

impl Proj {
    fn read(texinfo: &[u8], i: usize) -> Option<(Self, i32, i32)> {
        let o = i * TEXINFO_SIZE;
        let mut p = Proj { tex: [[0.0; 4]; 2], lm: [[0.0; 4]; 2] };
        for r in 0..2 {
            for k in 0..4 {
                p.tex[r][k] = f64::from(f32_at(texinfo, o + r * 16 + k * 4).unwrap_or(0.0));
                p.lm[r][k] = f64::from(f32_at(texinfo, o + 32 + r * 16 + k * 4).unwrap_or(0.0));
            }
        }
        Some((p, i32_at(texinfo, o + 64)?, i32_at(texinfo, o + 68)?))
    }

    fn apply(v: &[f64; 4], p: DVec3) -> f64 {
        v[0] * p.x + v[1] * p.y + v[2] * p.z + v[3]
    }

    fn uv(&self, p: DVec3) -> [f32; 2] {
        [Self::apply(&self.tex[0], p) as f32, Self::apply(&self.tex[1], p) as f32]
    }

    fn luxel(&self, p: DVec3, mins: [i32; 2]) -> [f32; 2] {
        [
            (Self::apply(&self.lm[0], p) - f64::from(mins[0])) as f32,
            (Self::apply(&self.lm[1], p) - f64::from(mins[1])) as f32,
        ]
    }

    fn scale(&self) -> f32 {
        let len = DVec3::new(self.lm[0][0], self.lm[0][1], self.lm[0][2]).length();
        if len > 1e-6 { (1.0 / len) as f32 } else { 16.0 }
    }
}

fn texdata_name(l: &Lumps<'_>, i: usize) -> Option<String> {
    let id = usize::try_from(i32_at(l.texdata, i * TEXDATA_SIZE + 12)?).ok()?;
    let ofs = usize::try_from(i32_at(l.string_table, id * 4)?).ok()?;
    let rest = l.string_data.get(ofs..)?;
    let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len()).min(256);
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

fn model_origins(entities: &[hp_bsp::Entity]) -> HashMap<usize, DVec3> {
    let mut out = HashMap::new();
    for e in entities {
        let Some(n) = e.get("model").and_then(|m| m.trim().strip_prefix('*')).and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        if let Some(o) = e.origin().map(DVec3::from_array).filter(|o| o.is_finite()) {
            out.insert(n, o);
        }
    }
    out
}

fn polygon(l: &Lumps<'_>, f: usize) -> Option<Vec<DVec3>> {
    let first_edge = usize::try_from(i32_at(l.faces, f + 4)?).ok()?;
    let num_edges = usize::try_from(i16_at(l.faces, f + 8)?).ok()?;
    if !(3..=MAX_FACE_EDGES).contains(&num_edges) {
        return None;
    }
    let mut poly = Vec::with_capacity(num_edges);
    for e in 0..num_edges {
        let se = i32_at(l.surfedges, (first_edge + e) * 4)?;
        let (edge, back) = (se.unsigned_abs() as usize, se < 0);
        let v = u16_at(l.edges, edge * 4 + if back { 2 } else { 0 })?;
        poly.push(vec_at(l.vertexes, usize::from(v) * 12)?);
    }
    Some(poly)
}

type Bound<'s> = (u32, SolidInfo, Option<&'s Arc<[u32]>>);

fn bind<'s>(
    sides: &'s SideTable,
    solids: &HashMap<u32, SolidInfo>,
    low: u16,
    normal: DVec3,
    poly: &[DVec3],
) -> Option<Bound<'s>> {
    let n = poly.len().max(1) as f64;
    let center = poly.iter().copied().sum::<DVec3>() / n;
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for p in poly {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let mut best: Option<(u8, Bound<'s>)> = None;
    for s in sides.get(low) {
        let facing = s.normal.dot(normal);
        if facing.abs() < 0.99 || (s.normal.dot(center) - s.dist).abs() > 1.0 {
            continue;
        }
        let Some(info) = solids.get(&s.solid) else { continue };
        let touches = (0..3).all(|k| f64::from(info.lo[k]) - 1.0 <= hi[k] && f64::from(info.hi[k]) + 1.0 >= lo[k]);
        let rank = match (facing > 0.0, touches) {
            (true, true) => return Some((s.solid, *info, s.blend.as_ref())),
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        };
        if best.as_ref().is_none_or(|b| rank < b.0) {
            best = Some((rank, (s.solid, *info, s.blend.as_ref())));
        }
    }
    best.map(|b| b.1)
}

fn grid_indices(n: u32) -> Vec<u32> {
    let mut idx = Vec::with_capacity(((n - 1) * (n - 1) * 6) as usize);
    for i in 0..n - 1 {
        for j in 0..n - 1 {
            let v = i * n + j;
            if v % 2 == 1 {
                idx.extend_from_slice(&[v, v + 1, v + n, v + 1, v + n + 1, v + n]);
            } else {
                idx.extend_from_slice(&[v, v + n + 1, v + n, v, v + 1, v + n + 1]);
            }
        }
    }
    idx
}

fn orient(indices: &mut [u32], flat: &[DVec3], normal: DVec3) {
    let mut sum = 0.0;
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (flat[t[0] as usize], flat[t[1] as usize], flat[t[2] as usize]);
        sum += (b - a).cross(c - a).dot(normal);
    }
    if sum < 0.0 {
        for t in indices.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
    }
}

pub(crate) fn geometry(bsp: &hp_bsp::Bsp, sides: &SideTable, solids: &HashMap<u32, SolidInfo>) -> Geometry {
    let get = |i: usize| bsp.lump(i).unwrap_or(std::borrow::Cow::Borrowed(&[] as &[u8]));
    let (planes, vertexes, edges, surfedges, faces, texinfo, texdata, string_table, string_data, models, dispinfo) = (
        get(lump::PLANES),
        get(lump::VERTEXES),
        get(lump::EDGES),
        get(lump::SURFEDGES),
        get(lump::FACES),
        get(lump::TEXINFO),
        get(lump::TEXDATA),
        get(lump::STRING_TABLE),
        get(lump::STRING_DATA),
        get(lump::MODELS),
        get(lump::DISPINFO),
    );
    let (disp_verts, faceids) = (get(lump::DISP_VERTS), get(lump::FACEIDS));
    let l = Lumps {
        planes: &planes,
        vertexes: &vertexes,
        edges: &edges,
        surfedges: &surfedges,
        faces: &faces,
        texinfo: &texinfo,
        texdata: &texdata,
        string_table: &string_table,
        string_data: &string_data,
        models: &models,
        dispinfo: &dispinfo,
        disp_verts: &disp_verts,
        faceids: &faceids,
    };
    let entities = bsp.entities().unwrap_or_default();
    let origins = model_origins(&entities);
    let total = l.faces.len() / FACE_SIZE;
    let mut out = Geometry::default();
    let mut material_index: HashMap<i32, u32> = HashMap::new();
    for m in 0..l.models.len() / MODEL_SIZE {
        let o = m * MODEL_SIZE;
        let first = i32_at(l.models, o + 40).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
        let count = i32_at(l.models, o + 44).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
        let offset = if m == 0 { DVec3::ZERO } else { origins.get(&m).copied().unwrap_or(DVec3::ZERO) };
        for face in first.min(total)..first.saturating_add(count).min(total) {
            let Some(f) = read_face(&l, face, offset, sides, solids, &mut out, &mut material_index) else { continue };
            out.faces.push(f);
        }
    }
    out
}

fn read_face(
    l: &Lumps<'_>,
    face: usize,
    offset: DVec3,
    sides: &SideTable,
    solids: &HashMap<u32, SolidInfo>,
    out: &mut Geometry,
    material_index: &mut HashMap<i32, u32>,
) -> Option<GeoFace> {
    let f = face * FACE_SIZE;
    let ti = usize::try_from(i16_at(l.faces, f + 10)?).ok()?;
    let (proj, flags, texdata) = Proj::read(l.texinfo, ti)?;
    if flags & UNLIT != 0 {
        return None;
    }
    let mins = [i32_at(l.faces, f + 28)?, i32_at(l.faces, f + 32)?];
    let size = [i32_at(l.faces, f + 36)?, i32_at(l.faces, f + 40)?];
    let (w, h) = (u32::try_from(size[0]).ok()? + 1, u32::try_from(size[1]).ok()? + 1);
    if w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let poly = polygon(l, f)?;
    let plane = usize::from(u16_at(l.faces, f)?);
    let normal = vec_at(l.planes, plane * PLANE_SIZE)?;
    let world: Vec<DVec3> = poly.iter().map(|p| *p + offset).collect();
    let low = u16_at(l.faceids, face * 2).unwrap_or(0);
    let Some((solid, info, blend)) = bind(sides, solids, low, normal, &world) else {
        out.unbound += 1;
        return None;
    };
    let material = match material_index.get(&texdata) {
        Some(m) => *m,
        None => {
            let name = usize::try_from(texdata)
                .ok()
                .and_then(|i| texdata_name(l, i))
                .map(|n| hp_editor::limits::world::unpatched_material(&n))
                .unwrap_or_else(|| "tools/toolsnodraw".to_owned());
            let m = out.materials.len() as u32;
            out.materials.push(name);
            material_index.insert(texdata, m);
            m
        }
    };
    let disp = i16_at(l.faces, f + 12).unwrap_or(-1);
    let (vertices, indices, render_flags) = if disp >= 0 {
        disp_mesh(l, usize::try_from(disp).ok()?, &poly, offset, &proj, [w, h], normal, blend.map(|b| &b[..]))?
    } else {
        let vertices: Vec<Vert> = poly
            .iter()
            .map(|p| Vert {
                pos: (*p + offset).as_vec3().to_array(),
                uv: proj.uv(*p),
                lm: proj.luxel(*p, mins),
                alpha: 0.0,
                blend: 0,
            })
            .collect();
        let mut indices: Vec<u32> = (1..poly.len() as u32 - 1).flat_map(|i| [0, i, i + 1]).collect();
        orient(&mut indices, &poly, normal);
        (vertices, indices, 0)
    };
    Some(GeoFace {
        bsp: face as u32,
        solid,
        generation: info.generation,
        material,
        w,
        h,
        scale: proj.scale(),
        flags: render_flags,
        bumped: flags & BUMPLIGHT != 0,
        vertices,
        indices,
    })
}

#[allow(clippy::too_many_arguments)]
fn disp_mesh(
    l: &Lumps<'_>,
    index: usize,
    poly: &[DVec3],
    offset: DVec3,
    proj: &Proj,
    dims: [u32; 2],
    normal: DVec3,
    blend: Option<&[u32]>,
) -> Option<(Vec<Vert>, Vec<u32>, u32)> {
    if poly.len() != 4 {
        return None;
    }
    let d = index * DISPINFO_SIZE;
    let start = vec_at(l.dispinfo, d)?;
    let first_vert = usize::try_from(i32_at(l.dispinfo, d + 12)?).ok()?;
    let power = i32_at(l.dispinfo, d + 20)?;
    if !(2..=4).contains(&power) {
        return None;
    }
    let n = (1usize << power) + 1;
    let first = (0..4).min_by(|a, b| poly[*a].distance_squared(start).total_cmp(&poly[*b].distance_squared(start)))?;
    let c: [DVec3; 4] = std::array::from_fn(|k| poly[(first + k) % 4]);
    let inv = 1.0 / (n - 1) as f64;
    let (sw, sh) = (f64::from(dims[0].max(1) - 1), f64::from(dims[1].max(1) - 1));
    let blend = blend.filter(|b| b.len() == n * n);
    let mut vertices = Vec::with_capacity(n * n);
    let mut flat = Vec::with_capacity(n * n);
    for row in 0..n {
        let e0 = c[0].lerp(c[1], row as f64 * inv);
        let e1 = c[3].lerp(c[2], row as f64 * inv);
        for col in 0..n {
            let base = e0.lerp(e1, col as f64 * inv);
            let v = (first_vert + row * n + col) * DISP_VERT_SIZE;
            let dir = vec_at(l.disp_verts, v).unwrap_or(DVec3::ZERO);
            let dist = f64::from(f32_at(l.disp_verts, v + 12).unwrap_or(0.0));
            let alpha = f32_at(l.disp_verts, v + 16).unwrap_or(0.0);
            let mut p = base + dir * dist;
            if !p.is_finite() {
                p = base;
            }
            flat.push(base);
            vertices.push(Vert {
                pos: (p + offset).as_vec3().to_array(),
                uv: proj.uv(base),
                lm: [(col as f64 * inv * sw) as f32, (row as f64 * inv * sh) as f32],
                alpha: (alpha / 255.0).clamp(0.0, 1.0),
                blend: blend.and_then(|b| b.get(row * n + col)).copied().unwrap_or(0),
            });
        }
    }
    let mut indices = grid_indices(n as u32);
    orient(&mut indices, &flat, normal);
    let flags = face_flags::DISP | (((power as u32) << face_flags::POWER_SHIFT) & face_flags::POWER_MASK);
    Some((vertices, indices, flags))
}

pub(super) fn styles_on(entities: &[hp_bsp::Entity]) -> [bool; 256] {
    let mut on = [true; 256];
    let mut seen_lit = [false; 256];
    let mut seen_dark = [false; 256];
    for e in entities {
        if !e.classname().to_ascii_lowercase().starts_with("light") {
            continue;
        }
        let Some(style) = e.get("style").and_then(|s| s.trim().parse::<i64>().ok()).filter(|s| (1..255).contains(s))
        else {
            continue;
        };
        let named = e.get("targetname").is_some_and(|t| !t.trim().is_empty());
        let flags = e.get("spawnflags").and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
        if named && flags & 1 != 0 {
            seen_dark[style as usize] = true;
        } else {
            seen_lit[style as usize] = true;
        }
    }
    for ((o, lit), dark) in on.iter_mut().zip(seen_lit).zip(seen_dark) {
        *o = lit || !dark;
    }
    on[255] = false;
    on
}

#[derive(Debug, Default)]
pub(crate) struct Block {
    pub batches: Vec<(u32, Vec<LightmapVertex>, Vec<u32>)>,
}

const BLOCK_FACES: usize = 2048;

pub(crate) fn blocks(faces: &mut [GeoFace]) -> Vec<Block> {
    let centers: Vec<Vec3> = faces
        .iter()
        .map(|f| {
            let n = f.vertices.len().max(1) as f32;
            f.vertices.iter().map(|v| Vec3::from(v.pos)).sum::<Vec3>() / n
        })
        .collect();
    let mut order: Vec<u32> = (0..faces.len() as u32).collect();
    let mut groups = Vec::new();
    split(&mut order, &centers, &mut groups);
    groups
        .into_iter()
        .map(|group| {
            let mut by_material: HashMap<u32, (Vec<LightmapVertex>, Vec<u32>)> = HashMap::new();
            for i in group {
                let f = &mut faces[i as usize];
                let (verts, idx) = by_material.entry(f.material).or_default();
                let base = verts.len() as u32;
                verts.extend(f.vertices.iter().map(|v| LightmapVertex {
                    pos: v.pos,
                    uv: v.uv,
                    lm: v.lm,
                    alpha: v.alpha,
                    face: i,
                    blend: v.blend,
                }));
                idx.extend(f.indices.iter().map(|k| base + k));
                f.vertices = Vec::new();
                f.indices = Vec::new();
            }
            let mut batches: Vec<(u32, Vec<LightmapVertex>, Vec<u32>)> =
                by_material.into_iter().map(|(m, (v, i))| (m, v, i)).collect();
            batches.sort_by_key(|b| b.0);
            Block { batches }
        })
        .collect()
}

fn split(order: &mut [u32], centers: &[Vec3], out: &mut Vec<Vec<u32>>) {
    if order.len() <= BLOCK_FACES {
        if !order.is_empty() {
            out.push(order.to_vec());
        }
        return;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for &i in order.iter() {
        lo = lo.min(centers[i as usize]);
        hi = hi.max(centers[i as usize]);
    }
    let ext = hi - lo;
    let axis = if ext.x >= ext.y && ext.x >= ext.z {
        0
    } else if ext.y >= ext.z {
        1
    } else {
        2
    };
    let mid = order.len() / 2;
    order.select_nth_unstable_by(mid, |a, b| centers[*a as usize][axis].total_cmp(&centers[*b as usize][axis]));
    let (a, b) = order.split_at_mut(mid);
    split(a, centers, out);
    split(b, centers, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_matches_the_editor() {
        let info = hp_geom::create_disp(
            &[
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(64.0, 0.0, 0.0),
                DVec3::new(64.0, 64.0, 0.0),
                DVec3::new(0.0, 64.0, 0.0),
            ],
            2,
        )
        .expect("disp");
        let quad = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(64.0, 0.0, 0.0),
            DVec3::new(64.0, 64.0, 0.0),
            DVec3::new(0.0, 64.0, 0.0),
        ];
        let mesh = hp_geom::build_disp(&quad, &info).expect("mesh");
        let mut tris: Vec<[u32; 3]> = mesh.indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let mut ours: Vec<[u32; 3]> = grid_indices(5).chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let canon = |t: &mut [u32; 3]| {
            let k = (0..3).min_by_key(|k| t[*k]).unwrap_or(0);
            t.rotate_left(k);
        };
        tris.iter_mut().for_each(canon);
        ours.iter_mut().for_each(canon);
        tris.sort_unstable();
        ours.sort_unstable();
        assert_eq!(tris, ours);
    }

    #[test]
    fn orientation_follows_the_normal() {
        let flat = [DVec3::ZERO, DVec3::X, DVec3::Y];
        let mut idx = vec![0, 1, 2];
        orient(&mut idx, &flat, DVec3::Z);
        assert_eq!(idx, [0, 1, 2]);
        orient(&mut idx, &flat, -DVec3::Z);
        assert_eq!(idx, [0, 2, 1]);
    }

    #[test]
    fn dark_styles() {
        let mk = |kv: &[(&str, &str)]| {
            let mut e = hp_bsp::Entity::default();
            for (k, v) in kv {
                e.set(k, *v);
            }
            e
        };
        let ents = [
            mk(&[("classname", "light"), ("style", "32"), ("targetname", "a"), ("spawnflags", "1")]),
            mk(&[("classname", "light"), ("style", "33"), ("targetname", "b")]),
            mk(&[("classname", "light_spot"), ("style", "10")]),
        ];
        let on = styles_on(&ents);
        assert!(on[0] && !on[32] && on[33] && on[10] && !on[255]);
    }

    #[test]
    fn blocks_cover_every_face() {
        let face = |i: u32, x: f32| GeoFace {
            bsp: i,
            solid: 1,
            generation: 0,
            material: i % 3,
            w: 2,
            h: 2,
            scale: 16.0,
            flags: 0,
            bumped: false,
            vertices: vec![
                Vert { pos: [x, 0.0, 0.0], ..Default::default() },
                Vert { pos: [x + 1.0, 0.0, 0.0], ..Default::default() },
                Vert { pos: [x, 1.0, 0.0], ..Default::default() },
            ],
            indices: vec![0, 1, 2],
        };
        let mut faces: Vec<GeoFace> = (0..5000).map(|i| face(i, i as f32 * 4.0)).collect();
        let blocks = blocks(&mut faces);
        assert!(blocks.len() >= 3);
        let mut seen = vec![0u32; 5000];
        for b in &blocks {
            for (m, v, idx) in &b.batches {
                assert_eq!(v.len(), idx.len());
                for vert in v {
                    assert_eq!(vert.face % 3, *m);
                    seen[vert.face as usize] += 1;
                }
            }
        }
        assert!(seen.iter().all(|n| *n == 3));
        assert!(faces.iter().all(|f| f.vertices.is_empty()));
    }
}
