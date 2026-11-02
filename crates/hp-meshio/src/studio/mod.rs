use std::collections::HashMap;

use glam::{DVec2, DVec3};

use crate::Model;
use crate::lod::{self, LodLevel, LodOptions, Wedge};
use crate::smd::Collision;

mod buf;
mod mdl;
mod phy;
mod vtx;

const MAX_MESH_VERTS: usize = 65_535;
const MAX_MODEL_VERTS: usize = 65_536;
const MAX_MODEL_TRIS: usize = 65_536;
const MAX_MATERIALS: usize = 32;
const MAX_BODY_PARTS: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct StudioOptions {
    pub model: String,
    pub cdmaterials: String,
    pub static_prop: bool,
    pub surface_prop: String,
    pub mass: f64,
    pub collision: Collision,
    pub lod: LodOptions,
}

#[derive(Clone, Debug, Default)]
pub struct StudioFiles {
    pub mdl: Vec<u8>,
    pub vvd: Vec<u8>,
    pub vtx: Vec<u8>,
    pub phy: Option<Vec<u8>>,
    pub convex_pieces: usize,
    pub body_parts: usize,
    pub lods: usize,
    pub lod_triangles: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StudioError {
    #[error("the model has no triangle")]
    Empty,
    #[error("the model uses {0} materials (32 at most)")]
    TooManyMaterials(usize),
    #[error("the model is too large")]
    TooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Vertex {
    pub pos: DVec3,
    pub normal: DVec3,
    pub uv: DVec2,
    pub tangent: [f64; 4],
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Mesh {
    pub material: usize,
    pub verts: Vec<Vertex>,
    pub tris: Vec<[u16; 3]>,
    pub lods: Vec<Vec<[u16; 3]>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Part {
    pub meshes: Vec<Mesh>,
}

impl Part {
    fn verts(&self) -> usize {
        self.meshes.iter().map(|m| m.verts.len()).sum()
    }
    fn tris(&self) -> usize {
        self.meshes.iter().map(|m| m.tris.len()).sum()
    }
}

fn key(p: DVec3, n: DVec3, uv: DVec2) -> [i64; 8] {
    let q = |v: f64, s: f64| (v * s).round() as i64;
    [q(p.x, 1e4), q(p.y, 1e4), q(p.z, 1e4), q(n.x, 1e4), q(n.y, 1e4), q(n.z, 1e4), q(uv.x, 1e5), q(uv.y, 1e5)]
}

pub(crate) fn build_parts(model: &Model) -> Result<Vec<Part>, StudioError> {
    let mut by_material: Vec<Vec<usize>> = vec![Vec::new(); model.materials.len().max(1)];
    for (i, t) in model.triangles.iter().enumerate() {
        if !t.pos.iter().all(|p| p.is_finite()) {
            continue;
        }
        let n = (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]);
        if n.length_squared() <= 1e-18 {
            continue;
        }
        let m = t.material.min(by_material.len() - 1);
        by_material[m].push(i);
    }
    let used = by_material.iter().filter(|v| !v.is_empty()).count();
    if used == 0 {
        return Err(StudioError::Empty);
    }
    if used > MAX_MATERIALS {
        return Err(StudioError::TooManyMaterials(used));
    }
    let mut meshes: Vec<Mesh> = Vec::new();
    for (material, tris) in by_material.iter().enumerate() {
        if tris.is_empty() {
            continue;
        }
        let mut mesh = Mesh { material, ..Mesh::default() };
        let mut index: HashMap<[i64; 8], u16> = HashMap::new();
        for &ti in tris {
            if mesh.verts.len() + 3 > MAX_MESH_VERTS || mesh.tris.len() >= MAX_MODEL_TRIS {
                meshes.push(std::mem::replace(&mut mesh, Mesh { material, ..Mesh::default() }));
                index.clear();
            }
            let t = &model.triangles[ti];
            let mut corner = [0u16; 3];
            for (k, slot) in corner.iter_mut().enumerate() {
                let n = if t.normal[k].is_finite() && t.normal[k] != DVec3::ZERO {
                    t.normal[k].normalize()
                } else {
                    (t.pos[1] - t.pos[0]).cross(t.pos[2] - t.pos[0]).normalize_or_zero()
                };
                let uv = if t.uv[k].is_finite() { t.uv[k] } else { DVec2::ZERO };
                let id = *index.entry(key(t.pos[k], n, uv)).or_insert_with(|| {
                    mesh.verts.push(Vertex { pos: t.pos[k], normal: n, uv, tangent: [1.0, 0.0, 0.0, 1.0] });
                    (mesh.verts.len() - 1) as u16
                });
                *slot = id;
            }
            if corner[0] != corner[1] && corner[1] != corner[2] && corner[0] != corner[2] {
                mesh.tris.push(corner);
            }
        }
        if !mesh.tris.is_empty() {
            meshes.push(mesh);
        }
    }
    for m in &mut meshes {
        tangents(m);
    }
    let mut parts: Vec<Part> = vec![Part::default()];
    for mesh in meshes {
        let last = parts.last().expect("a part");
        if !last.meshes.is_empty()
            && (last.verts() + mesh.verts.len() > MAX_MODEL_VERTS || last.tris() + mesh.tris.len() > MAX_MODEL_TRIS)
        {
            parts.push(Part::default());
        }
        parts.last_mut().expect("a part").meshes.push(mesh);
    }
    if parts.len() > MAX_BODY_PARTS {
        return Err(StudioError::TooLarge);
    }
    Ok(parts)
}

fn tangents(mesh: &mut Mesh) {
    let mut t_acc = vec![DVec3::ZERO; mesh.verts.len()];
    let mut b_acc = vec![DVec3::ZERO; mesh.verts.len()];
    for tri in &mesh.tris {
        let [a, b, c] = tri.map(|i| mesh.verts[usize::from(i)]);
        let (e1, e2) = (b.pos - a.pos, c.pos - a.pos);
        let (d1, d2) = (b.uv - a.uv, c.uv - a.uv);
        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        let t = (e1 * d2.y - e2 * d1.y) * r;
        let bt = (e2 * d1.x - e1 * d2.x) * r;
        for &i in tri {
            t_acc[usize::from(i)] += t;
            b_acc[usize::from(i)] += bt;
        }
    }
    for (i, v) in mesh.verts.iter_mut().enumerate() {
        let n = v.normal;
        let mut t = t_acc[i] - n * n.dot(t_acc[i]);
        if t.length_squared() < 1e-20 {
            let other = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
            t = other - n * n.dot(other);
        }
        let t = t.normalize_or_zero();
        let w = if n.cross(t).dot(b_acc[i]) < 0.0 { -1.0 } else { 1.0 };
        v.tangent = [t.x, t.y, t.z, w];
    }
}

pub(crate) fn part_bounds(parts: &[Part]) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for v in parts.iter().flat_map(|p| p.meshes.iter()).flat_map(|m| m.verts.iter()) {
        lo = lo.min(v.pos);
        hi = hi.max(v.pos);
    }
    if lo.x > hi.x { (DVec3::ZERO, DVec3::ZERO) } else { (lo, hi) }
}

fn stem(model: &str) -> String {
    let s = model.trim_matches('/').rsplit(['/', '\\']).next().unwrap_or("model");
    let s = s.strip_suffix(".mdl").unwrap_or(s);
    if s.is_empty() { "model".to_owned() } else { s.to_owned() }
}

pub fn compile(model: &Model, materials: &[String], opts: &StudioOptions) -> Result<StudioFiles, StudioError> {
    let mut parts = build_parts(model)?;
    let mut used: Vec<usize> = parts.iter().flat_map(|p| p.meshes.iter().map(|m| m.material)).collect();
    used.sort_unstable();
    used.dedup();
    for m in parts.iter_mut().flat_map(|p| p.meshes.iter_mut()) {
        m.material = used.binary_search(&m.material).unwrap_or(0);
    }
    let names: Vec<String> =
        used.iter().map(|&i| materials.get(i).cloned().unwrap_or_else(|| format!("material_{i}"))).collect();
    let stem = stem(&opts.model);
    let radius = parts
        .iter()
        .flat_map(|p| p.meshes.iter())
        .flat_map(|m| m.verts.iter())
        .map(|v| v.pos.length())
        .fold(0.0f64, f64::max);
    let total: usize = parts.iter().map(Part::tris).sum();
    let levels = lod::plan(&opts.lod, total, radius);
    for part in &mut parts {
        add_lods(part, &levels);
    }
    let shadow_lod = !opts.static_prop && !levels.is_empty();
    let mut switches: Vec<f32> = std::iter::once(0.0).chain(levels.iter().map(|l| l.switch as f32)).collect();
    if shadow_lod {
        for m in parts.iter_mut().flat_map(|p| p.meshes.iter_mut()) {
            let last = m.lods.last().cloned().unwrap_or_default();
            m.lods.push(last);
        }
        switches.push(-1.0);
    }
    let lod_triangles: Vec<usize> = (0..=levels.len())
        .map(|i| {
            parts
                .iter()
                .flat_map(|p| p.meshes.iter())
                .map(|m| if i == 0 { m.tris.len() } else { m.lods[i - 1].len() })
                .sum()
        })
        .collect();
    let collision = phy::build(model, opts.collision);
    let (vvd_body, layout) = vtx::vvd_vertices(&parts);
    let checksum = buf::checksum(&[&vvd_body, opts.model.as_bytes(), names.concat().as_bytes()]);
    let (mut lo, mut hi) = part_bounds(&parts);
    let mesh_bounds = (lo, hi);
    if let Some(c) = &collision {
        lo = lo.min(c.lo - DVec3::splat(phy::MARGIN));
        hi = hi.max(c.hi + DVec3::splat(phy::MARGIN));
    }
    let info = mdl::Info {
        name: format!("{}.mdl", opts.model.trim_matches('/')),
        stem: stem.clone(),
        cdmaterials: opts.cdmaterials.clone(),
        materials: names,
        static_prop: opts.static_prop,
        surface_prop: clean_key(&opts.surface_prop).unwrap_or_else(|| "default".to_owned()),
        mass: if opts.mass.is_finite() && opts.mass > 0.0 { opts.mass.min(50_000.0) } else { 10.0 },
        hull: (lo, hi),
        mesh_bounds,
        checksum,
        shadow_lod,
    };
    let mdl = mdl::write(&info, &parts, &layout);
    let vvd = vtx::vvd(&vvd_body, &layout, checksum, switches.len());
    let vtx = vtx::vtx(&parts, checksum, &switches);
    let convex_pieces = collision.as_ref().map_or(0, |c| c.pieces);
    let phy = collision.map(|c| phy::file(&c, &info, &format!("{stem}_phys")));
    Ok(StudioFiles {
        mdl,
        vvd,
        vtx,
        phy,
        convex_pieces,
        body_parts: parts.len(),
        lods: levels.len() + 1,
        lod_triangles,
    })
}

fn add_lods(part: &mut Part, levels: &[LodLevel]) {
    if levels.is_empty() {
        return;
    }
    let mut wedges = Vec::new();
    let mut owner: Vec<(usize, u16)> = Vec::new();
    let mut first = Vec::with_capacity(part.meshes.len());
    for (mi, m) in part.meshes.iter().enumerate() {
        first.push(wedges.len() as u32);
        for (vi, v) in m.verts.iter().enumerate() {
            wedges.push(Wedge { pos: v.pos, normal: v.normal, uv: v.uv });
            owner.push((mi, vi as u16));
        }
    }
    let mut tris = Vec::new();
    let mut groups = Vec::new();
    for (mi, m) in part.meshes.iter().enumerate() {
        for t in &m.tris {
            tris.push(t.map(|i| first[mi] + u32::from(i)));
            groups.push(mi as u32);
        }
    }
    let targets: Vec<usize> =
        levels.iter().map(|l| ((tris.len() as f64) * l.fraction).round().max(1.0) as usize).collect();
    let simplified = lod::simplify(&wedges, &tris, &groups, &targets);
    for m in &mut part.meshes {
        m.lods = vec![Vec::new(); levels.len()];
    }
    for (level, out) in simplified.into_iter().enumerate() {
        for (c, g) in out {
            let mesh = g as usize;
            if c.iter().all(|&w| owner[w as usize].0 == mesh) {
                part.meshes[mesh].lods[level].push(c.map(|w| owner[w as usize].1));
            }
        }
    }
}

fn clean_key(s: &str) -> Option<String> {
    let s: String = s.trim().chars().filter(|c| !c.is_control() && !matches!(c, '"' | '{' | '}')).collect();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests;
