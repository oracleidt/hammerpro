use glam::DVec3;
use hp_vmf::DispInfo;

use crate::texture::world_axes;
use crate::util::polygon_normal;
use crate::{Aabb, GeomError, Ray, ray_triangle};

const MAX_POWER: u32 = 4;

pub const DISP_TAG_WALKABLE: i32 = 1;
pub const DISP_TAG_BUILDABLE: i32 = 1 << 3;
pub const DISP_TAG_FORCE_WALKABLE_BIT: i32 = 1 << 1;
pub const DISP_TAG_FORCE_WALKABLE_VAL: i32 = 1 << 2;
pub const DISP_TAG_FORCE_BUILDABLE_BIT: i32 = 1 << 4;
pub const DISP_TAG_FORCE_BUILDABLE_VAL: i32 = 1 << 5;
pub const DISP_TAG_FORCE_REMOVE_BIT: i32 = 1 << 6;
const WALKABLE_NORMAL_Z: f64 = 0.7;
const BUILDABLE_NORMAL_Z: f64 = 0.8;

#[derive(Clone, Debug, PartialEq)]
pub struct DispMesh {
    pub positions: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub alphas: Vec<f64>,
    pub base_positions: Vec<DVec3>,
    pub indices: Vec<u32>,
    pub row_len: usize,
    pub face_normal: DVec3,
    pub aabb: Aabb,
}

impl DispMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

pub(crate) fn source_corners(face_vertices: &[DVec3], start: DVec3) -> Result<([DVec3; 4], DVec3), GeomError> {
    let [c0, c1, c2, c3] =
        <[DVec3; 4]>::try_from(face_vertices).map_err(|_| GeomError::NotQuad(face_vertices.len()))?;
    if !(c0.is_finite() && c1.is_finite() && c2.is_finite() && c3.is_finite()) {
        return Err(GeomError::NotFinite);
    }
    let normal = polygon_normal(face_vertices).ok_or(GeomError::DegeneratePlane)?;
    let cw = [c0, c3, c2, c1];
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for (i, c) in cw.iter().enumerate() {
        let d = c.distance_squared(start);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    let p = |k: usize| cw.get((best + k) % 4).copied().unwrap_or(DVec3::ZERO);
    Ok(([p(0), p(1), p(2), p(3)], normal))
}

fn check_power(power: u32) -> Result<usize, GeomError> {
    if (1..=MAX_POWER).contains(&power) { Ok((1usize << power) + 1) } else { Err(GeomError::BadPower(power)) }
}

pub(crate) fn flat_grid(p: &[DVec3; 4], n: usize) -> Vec<DVec3> {
    let inv = 1.0 / (n - 1) as f64;
    let mut out = Vec::with_capacity(n * n);
    for i in 0..n {
        let fi = i as f64 * inv;
        let e0 = p[0].lerp(p[1], fi);
        let e1 = p[3].lerp(p[2], fi);
        for j in 0..n {
            out.push(e0.lerp(e1, j as f64 * inv));
        }
    }
    out
}

pub(crate) fn grid_indices(n: usize) -> Vec<u32> {
    let quads = n.saturating_sub(1);
    let mut idx = Vec::with_capacity(quads * quads * 6);
    for i in 0..quads {
        for j in 0..quads {
            let v = (i * n + j) as u32;
            let w = n as u32;
            if v % 2 == 1 {
                idx.extend_from_slice(&[v, v + 1, v + w, v + 1, v + w + 1, v + w]);
            } else {
                idx.extend_from_slice(&[v, v + w + 1, v + w, v, v + 1, v + w + 1]);
            }
        }
    }
    idx
}

fn smooth_normals(positions: &[DVec3], indices: &[u32], fallback: DVec3) -> Vec<DVec3> {
    let mut normals = vec![DVec3::ZERO; positions.len()];
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let (Some(&pa), Some(&pb), Some(&pc)) = (positions.get(a), positions.get(b), positions.get(c)) else {
            continue;
        };
        let n = (pb - pa).cross(pc - pa);
        for k in [a, b, c] {
            if let Some(slot) = normals.get_mut(k) {
                *slot += n;
            }
        }
    }
    for n in &mut normals {
        *n = n.try_normalize().unwrap_or(fallback);
    }
    normals
}

pub fn build_disp(face_vertices: &[DVec3], disp: &DispInfo) -> Option<DispMesh> {
    let n = check_power(disp.power).ok()?;
    let (corners, face_normal) = source_corners(face_vertices, disp.start_position).ok()?;
    let base_positions = flat_grid(&corners, n);
    let elevation = if disp.elevation.is_finite() { face_normal * disp.elevation } else { DVec3::ZERO };
    let mut positions = Vec::with_capacity(base_positions.len());
    let mut alphas = Vec::with_capacity(base_positions.len());
    for (k, &base) in base_positions.iter().enumerate() {
        let normal = disp.normals.get(k).copied().unwrap_or(DVec3::ZERO);
        let dist = disp.distances.get(k).copied().unwrap_or(0.0);
        let offset = disp.offsets.get(k).copied().unwrap_or(DVec3::ZERO);
        let mut p = base + normal * dist + offset + elevation;
        if !p.is_finite() {
            p = base;
        }
        positions.push(p);
        let a = disp.alphas.get(k).copied().unwrap_or(0.0);
        alphas.push(if a.is_finite() { (a / 255.0).clamp(0.0, 1.0) } else { 0.0 });
    }
    let indices = grid_indices(n);
    let normals = smooth_normals(&positions, &indices, face_normal);
    let aabb = Aabb::from_points(positions.iter().copied());
    Some(DispMesh { positions, normals, alphas, base_positions, indices, row_len: n, face_normal, aabb })
}

pub fn update_disp_tags(disp: &mut DispInfo, face_vertices: &[DVec3]) -> bool {
    let Some(mesh) = build_disp(face_vertices, disp) else { return false };
    let count = mesh.triangle_count();
    disp.triangle_tags.resize(count, 0);
    for (tag, t) in disp.triangle_tags.iter_mut().zip(mesh.indices.chunks_exact(3)) {
        let (Some(&a), Some(&b), Some(&c)) =
            (mesh.positions.get(t[0] as usize), mesh.positions.get(t[1] as usize), mesh.positions.get(t[2] as usize))
        else {
            continue;
        };
        let z = (b - a).cross(c - a).try_normalize().map_or(0.0, |n| n.z);
        let mut v = *tag & !(DISP_TAG_WALKABLE | DISP_TAG_BUILDABLE);
        if z >= WALKABLE_NORMAL_Z {
            v |= DISP_TAG_WALKABLE;
        }
        if z >= BUILDABLE_NORMAL_Z {
            v |= DISP_TAG_BUILDABLE;
        }
        *tag = v;
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DispWalk {
    Buildable,
    Walkable,
    Steep,
}

impl DispWalk {
    fn of(walkable: bool, buildable: bool) -> Self {
        match (walkable, buildable) {
            (true, true) => DispWalk::Buildable,
            (true, false) => DispWalk::Walkable,
            (false, _) => DispWalk::Steep,
        }
    }
}

pub fn disp_tag_state(tag: i32) -> (bool, bool) {
    let walkable = if tag & DISP_TAG_FORCE_WALKABLE_BIT != 0 {
        tag & DISP_TAG_FORCE_WALKABLE_VAL != 0
    } else {
        tag & DISP_TAG_WALKABLE != 0
    };
    let buildable = if tag & DISP_TAG_FORCE_BUILDABLE_BIT != 0 {
        tag & DISP_TAG_FORCE_BUILDABLE_VAL != 0
    } else {
        tag & DISP_TAG_BUILDABLE != 0
    };
    (walkable, buildable)
}

pub fn slope_walk(normal_z: f64) -> DispWalk {
    DispWalk::of(normal_z >= WALKABLE_NORMAL_Z, normal_z >= BUILDABLE_NORMAL_Z)
}

pub const DISPTRI_TAG_WALKABLE: u16 = 1 << 1;
pub const DISPTRI_TAG_BUILDABLE: u16 = 1 << 2;
const SLOPE_MARGIN: f64 = 1e-3;

fn triangle_normal_z(mesh: &DispMesh) -> impl Iterator<Item = f64> + '_ {
    mesh.indices.chunks_exact(3).map(|t| {
        let p = |k: u32| mesh.positions.get(k as usize).copied().unwrap_or(DVec3::ZERO);
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        (b - a).cross(c - a).try_normalize().map_or(0.0, |n| n.z)
    })
}

pub fn disp_tri_states(disp: &DispInfo, mesh: &DispMesh) -> Vec<(bool, bool)> {
    let stored = disp.triangle_tags.len() == mesh.triangle_count();
    triangle_normal_z(mesh)
        .enumerate()
        .map(|(i, z)| {
            let (mut walkable, mut buildable) = (z >= WALKABLE_NORMAL_Z, z >= BUILDABLE_NORMAL_Z);
            let tag = if stored { disp.triangle_tags[i] } else { 0 };
            if tag & DISP_TAG_FORCE_WALKABLE_BIT != 0 {
                walkable = tag & DISP_TAG_FORCE_WALKABLE_VAL != 0;
            }
            if tag & DISP_TAG_FORCE_BUILDABLE_BIT != 0 {
                buildable = tag & DISP_TAG_FORCE_BUILDABLE_VAL != 0;
            }
            (walkable, buildable)
        })
        .collect()
}

pub fn disp_walk(disp: &DispInfo, mesh: &DispMesh) -> Vec<DispWalk> {
    disp_tri_states(disp, mesh).into_iter().map(|(w, b)| DispWalk::of(w, b)).collect()
}

pub fn compiled_disp_tags(disp: &DispInfo, mesh: &DispMesh) -> Vec<u16> {
    disp_tri_states(disp, mesh)
        .into_iter()
        .map(|(w, b)| if w { DISPTRI_TAG_WALKABLE } else { 0 } | if b { DISPTRI_TAG_BUILDABLE } else { 0 })
        .collect()
}

pub fn vmf_tags_from_compiled(compiled: &[u16], mesh: &DispMesh) -> Vec<i32> {
    triangle_normal_z(mesh)
        .enumerate()
        .map(|(i, z)| {
            let t = compiled.get(i).copied().unwrap_or(0);
            let (walkable, buildable) = (t & DISPTRI_TAG_WALKABLE != 0, t & DISPTRI_TAG_BUILDABLE != 0);
            let (slope_walkable, slope_buildable) = (z >= WALKABLE_NORMAL_Z, z >= BUILDABLE_NORMAL_Z);
            let mut tag = 0;
            if slope_walkable {
                tag |= DISP_TAG_WALKABLE;
            }
            if slope_buildable {
                tag |= DISP_TAG_BUILDABLE;
            }
            let near = |limit: f64| (z - limit).abs() < SLOPE_MARGIN;
            if walkable != slope_walkable || near(WALKABLE_NORMAL_Z) {
                tag |= DISP_TAG_FORCE_WALKABLE_BIT;
                if walkable {
                    tag |= DISP_TAG_FORCE_WALKABLE_VAL;
                }
            }
            if buildable != slope_buildable || near(BUILDABLE_NORMAL_Z) {
                tag |= DISP_TAG_FORCE_BUILDABLE_BIT;
                if buildable {
                    tag |= DISP_TAG_FORCE_BUILDABLE_VAL;
                }
            }
            tag
        })
        .collect()
}

pub fn create_disp(face_vertices: &[DVec3], power: u32) -> Result<DispInfo, GeomError> {
    let normal = polygon_normal(face_vertices).ok_or(GeomError::DegeneratePlane)?;
    let (u, v) = world_axes(normal);
    create_disp_with_axes(face_vertices, power, u, v)
}

pub fn create_disp_with_axes(
    face_vertices: &[DVec3],
    power: u32,
    uaxis: DVec3,
    vaxis: DVec3,
) -> Result<DispInfo, GeomError> {
    check_power(power)?;
    let (corners, normal) = source_corners(face_vertices, face_vertices.first().copied().unwrap_or(DVec3::ZERO))?;
    let vs: Vec<f64> = corners.iter().map(|c| c.dot(vaxis)).collect();
    let min_v = vs.iter().copied().fold(f64::INFINITY, f64::min);
    let mut start = corners[0];
    let mut best_u = f64::INFINITY;
    for (c, &cv) in corners.iter().zip(&vs) {
        if cv - min_v < 0.1 {
            let cu = c.dot(uaxis);
            if cu < best_u {
                best_u = cu;
                start = *c;
            }
        }
    }
    let mut disp = DispInfo::new_flat(power, start, normal);
    update_disp_tags(&mut disp, face_vertices);
    Ok(disp)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaintMode {
    Raise,
    Lower,
    Smooth,
    Set,
    AlphaPaint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Falloff {
    Constant,
    Linear,
    #[default]
    Smooth,
}

impl Falloff {
    pub fn weight(self, x: f64) -> f64 {
        if !(0.0..=1.0).contains(&x) {
            return 0.0;
        }
        match self {
            Falloff::Constant => 1.0,
            Falloff::Linear => 1.0 - x,
            Falloff::Smooth => {
                let t = 1.0 - x;
                t * t * (3.0 - 2.0 * t)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintBrush {
    pub center: DVec3,
    pub radius: f64,
    pub amount: f64,
    pub mode: PaintMode,
    pub falloff: Falloff,
    pub spatial: bool,
}

pub fn disp_paint(disp: &mut DispInfo, face_vertices: &[DVec3], brush: &PaintBrush) -> Result<usize, GeomError> {
    let changed = paint_vertices(disp, face_vertices, brush)?;
    if changed > 0 && brush.mode != PaintMode::AlphaPaint {
        update_disp_tags(disp, face_vertices);
    }
    Ok(changed)
}

fn paint_vertices(disp: &mut DispInfo, face_vertices: &[DVec3], brush: &PaintBrush) -> Result<usize, GeomError> {
    let n = check_power(disp.power)?;
    let radius_ok = brush.radius.is_finite() && brush.radius > 0.0;
    if !radius_ok || !brush.amount.is_finite() || !brush.center.is_finite() {
        return Err(GeomError::InvalidInput("invalid paint brush".to_owned()));
    }
    let (_, face_normal) = source_corners(face_vertices, disp.start_position)?;
    let count = n * n;
    disp.normals.resize(count, face_normal);
    disp.distances.resize(count, 0.0);
    disp.offsets.resize(count, DVec3::ZERO);
    disp.offset_normals.resize(count, face_normal);
    disp.alphas.resize(count, 0.0);
    let mesh = build_disp(face_vertices, disp).ok_or(GeomError::NotQuad(face_vertices.len()))?;
    let mut changed = 0usize;
    for (k, (&pos, &base)) in mesh.positions.iter().zip(&mesh.base_positions).enumerate() {
        let dist_to_brush = if brush.spatial {
            pos.distance(brush.center)
        } else {
            let d = base - brush.center;
            (d - face_normal * d.dot(face_normal)).length()
        };
        let w = brush.falloff.weight(dist_to_brush / brush.radius);
        if w <= 0.0 {
            continue;
        }
        let (Some(normal), Some(dist)) = (disp.normals.get_mut(k), disp.distances.get_mut(k)) else {
            continue;
        };
        let vec = *normal * *dist;
        let new_vec = match brush.mode {
            PaintMode::Raise => vec + face_normal * (brush.amount * w),
            PaintMode::Lower => vec - face_normal * (brush.amount * w),
            PaintMode::Set => vec + face_normal * ((brush.amount - vec.dot(face_normal)) * w),
            PaintMode::Smooth => {
                let (i, j) = (k / n, k % n);
                let mut sum = DVec3::ZERO;
                let mut cnt = 0.0;
                let neighbours = [
                    (i > 0).then(|| k - n),
                    (i + 1 < n).then(|| k + n),
                    (j > 0).then(|| k - 1),
                    (j + 1 < n).then(|| k + 1),
                ];
                for nb in neighbours.into_iter().flatten() {
                    if let Some(p) = mesh.positions.get(nb) {
                        sum += *p;
                        cnt += 1.0;
                    }
                }
                if cnt == 0.0 {
                    continue;
                }
                let lift = (sum / cnt - pos).dot(face_normal);
                vec + face_normal * (lift * brush.amount.clamp(0.0, 1.0) * w)
            }
            PaintMode::AlphaPaint => {
                if let Some(a) = disp.alphas.get_mut(k) {
                    let new_a = (*a + brush.amount * w).clamp(0.0, 255.0);
                    if new_a != *a {
                        *a = new_a;
                        changed += 1;
                    }
                }
                continue;
            }
        };
        let len = new_vec.length();
        if len > 1e-9 {
            *normal = new_vec / len;
            *dist = len;
        } else {
            *normal = face_normal;
            *dist = 0.0;
        }
        changed += 1;
    }
    Ok(changed)
}

pub const DISP_FLAG_NO_PHYSICS: i32 = 1 << 1;
pub const DISP_FLAG_NO_HULL: i32 = 1 << 2;
pub const DISP_FLAG_NO_RAY: i32 = 1 << 3;

fn fit_vertex_arrays(disp: &mut DispInfo, face_normal: DVec3) -> Option<usize> {
    let n = check_power(disp.power).ok()?;
    let count = n * n;
    disp.normals.resize(count, face_normal);
    disp.distances.resize(count, 0.0);
    disp.offsets.resize(count, DVec3::ZERO);
    disp.offset_normals.resize(count, face_normal);
    disp.alphas.resize(count, 0.0);
    Some(count)
}

pub fn invert_alpha(disp: &mut DispInfo) -> usize {
    let Some(_) = fit_vertex_arrays(disp, DVec3::Z) else { return 0 };
    let mut changed = 0;
    for a in &mut disp.alphas {
        let cur = if a.is_finite() { a.clamp(0.0, 255.0) } else { 0.0 };
        let new_a = 255.0 - cur;
        if new_a != *a {
            *a = new_a;
            changed += 1;
        }
    }
    changed
}

fn mix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn noise_at(p: DVec3, seed: u64) -> f64 {
    let q = |v: f64| (v * 16.0).round() as i64 as u64;
    let h = mix64(seed ^ mix64(q(p.x) ^ mix64(q(p.y) ^ mix64(q(p.z)))));
    (h >> 11) as f64 / (1u64 << 53) as f64
}

pub fn add_noise(
    disp: &mut DispInfo,
    face_vertices: &[DVec3],
    min: f64,
    max: f64,
    seed: u64,
) -> Result<usize, GeomError> {
    if !min.is_finite() || !max.is_finite() {
        return Err(GeomError::InvalidInput("noise range is not finite".to_owned()));
    }
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    add_height_fn(disp, face_vertices, |base| lo + (hi - lo) * noise_at(base, seed))
}

pub fn add_height_fn(
    disp: &mut DispInfo,
    face_vertices: &[DVec3],
    height: impl Fn(DVec3) -> f64,
) -> Result<usize, GeomError> {
    let mesh = build_disp(face_vertices, disp).ok_or(GeomError::NotQuad(face_vertices.len()))?;
    let normal = mesh.face_normal;
    fit_vertex_arrays(disp, normal).ok_or(GeomError::BadPower(disp.power))?;
    let mut changed = 0;
    for (k, base) in mesh.base_positions.iter().enumerate() {
        let h = height(*base);
        if h == 0.0 || !h.is_finite() {
            continue;
        }
        let (Some(nrm), Some(dist)) = (disp.normals.get_mut(k), disp.distances.get_mut(k)) else { continue };
        let vec = *nrm * *dist;
        let vec = if vec.is_finite() { vec } else { DVec3::ZERO };
        set_vector(nrm, dist, vec + normal * h, normal);
        changed += 1;
    }
    if changed > 0 {
        update_disp_tags(disp, face_vertices);
    }
    Ok(changed)
}

pub fn scale_disp(disp: &mut DispInfo, face_vertices: &[DVec3], factor: f64) -> Result<usize, GeomError> {
    if !factor.is_finite() || factor.abs() > 1e6 {
        return Err(GeomError::InvalidInput("scale is not finite".to_owned()));
    }
    let (_, normal) = source_corners(face_vertices, disp.start_position)?;
    fit_vertex_arrays(disp, normal).ok_or(GeomError::BadPower(disp.power))?;
    let mut changed = 0;
    for (nrm, d) in disp.normals.iter_mut().zip(disp.distances.iter_mut()) {
        let vec = *nrm * *d;
        let vec = if vec.is_finite() { vec } else { DVec3::ZERO };
        let (old_n, old_d) = (*nrm, *d);
        set_vector(nrm, d, vec * factor, normal);
        if *nrm != old_n || *d != old_d {
            changed += 1;
        }
    }
    if changed > 0 {
        update_disp_tags(disp, face_vertices);
    }
    Ok(changed)
}

fn set_vector(normal: &mut DVec3, dist: &mut f64, v: DVec3, face_normal: DVec3) {
    let len = v.length();
    if len > 1e-9 && len.is_finite() {
        *normal = v / len;
        *dist = len;
    } else {
        *normal = face_normal;
        *dist = 0.0;
    }
}

pub fn ray_disp(ray: &Ray, mesh: &DispMesh) -> Option<f64> {
    mesh.aabb.expand(1e-6).ray_intersect(ray)?;
    let mut best: Option<f64> = None;
    for t in mesh.indices.chunks_exact(3) {
        let (Some(&a), Some(&b), Some(&c)) =
            (mesh.positions.get(t[0] as usize), mesh.positions.get(t[1] as usize), mesh.positions.get(t[2] as usize))
        else {
            continue;
        };
        if let Some(hit) = ray_triangle(ray, a, b, c, false)
            && best.is_none_or(|x| hit < x)
        {
            best = Some(hit);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_solid;
    use crate::test_support::*;

    fn top_face() -> Vec<DVec3> {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(256.0, 128.0, 64.0));
        build_solid(&sides).face_for_side(0).expect("top").vertices.clone()
    }

    #[test]
    fn sizes_and_orientation() {
        let face = top_face();
        for power in 1..=4u32 {
            let d = create_disp(&face, power).expect("disp");
            let m = build_disp(&face, &d).expect("mesh");
            let n = (1usize << power) + 1;
            assert_eq!(m.row_len, n);
            assert_eq!(m.positions.len(), n * n);
            assert_eq!(m.triangle_count(), 2 * (n - 1) * (n - 1));
            assert!(m.indices.iter().all(|&i| (i as usize) < n * n));
            for t in m.indices.chunks_exact(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.positions[i as usize]);
                assert!((b - a).cross(c - a).normalize().dot(DVec3::Z) > 0.999);
            }
            for nrm in &m.normals {
                assert!((*nrm - DVec3::Z).length() < 1e-12);
            }
        }
        let d = create_disp(&face, 2).expect("disp");
        assert_eq!(d.start_position, DVec3::new(0.0, 128.0, 64.0));
        assert!(build_disp(&face[..3], &d).is_none());
        let mut bad = d.clone();
        bad.power = 9;
        assert!(build_disp(&face, &bad).is_none());
        assert_eq!(create_disp(&face, 0), Err(GeomError::BadPower(0)));
    }

    #[test]
    fn grid_layout_matches_source() {
        let face = top_face();
        let mut d = create_disp(&face, 2).expect("disp");
        let m = build_disp(&face, &d).expect("mesh");
        assert_eq!(m.positions[0], DVec3::new(0.0, 128.0, 64.0));
        assert_eq!(m.positions[4 * 5], DVec3::new(256.0, 128.0, 64.0));
        assert_eq!(m.positions[4], DVec3::new(0.0, 0.0, 64.0));
        assert_eq!(m.positions[24], DVec3::new(256.0, 0.0, 64.0));
        d.distances[6] = 10.0;
        d.offsets[7] = DVec3::new(1.0, 2.0, 3.0);
        d.elevation = 2.0;
        d.alphas[8] = 255.0;
        let m2 = build_disp(&face, &d).expect("mesh");
        assert_eq!(m2.positions[6], m.positions[6] + DVec3::new(0.0, 0.0, 12.0));
        assert_eq!(m2.positions[7], m.positions[7] + DVec3::new(1.0, 2.0, 5.0));
        assert_eq!(m2.alphas[8], 1.0);
        assert_eq!(m2.base_positions, m.positions);
        d.normals.truncate(3);
        d.alphas.clear();
        assert!(build_disp(&face, &d).is_some());
    }

    #[test]
    fn paint_and_pick() {
        let face = top_face();
        let mut d = create_disp(&face, 3).expect("disp");
        let center = DVec3::new(128.0, 64.0, 64.0);
        let brush = PaintBrush {
            center,
            radius: 40.0,
            amount: 16.0,
            mode: PaintMode::Raise,
            falloff: Falloff::Linear,
            spatial: false,
        };
        let changed = disp_paint(&mut d, &face, &brush).expect("paint");
        assert!(changed > 0);
        let m = build_disp(&face, &d).expect("mesh");
        let peak = m.positions.iter().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((peak - 80.0).abs() < 1e-9);
        let r = Ray::new(DVec3::new(128.0, 64.0, 500.0), DVec3::NEG_Z);
        let t = ray_disp(&r, &m).expect("hit");
        assert!((r.at(t).z - 80.0).abs() < 1e-9);
        assert!(ray_disp(&Ray::new(DVec3::new(1000.0, 0.0, 500.0), DVec3::NEG_Z), &m).is_none());
        let lower = PaintBrush { mode: PaintMode::Lower, ..brush };
        disp_paint(&mut d, &face, &lower).expect("paint");
        let m = build_disp(&face, &d).expect("mesh");
        assert!(m.positions.iter().all(|p| (p.z - 64.0).abs() < 1e-9));
        let set = PaintBrush { mode: PaintMode::Set, amount: 8.0, falloff: Falloff::Constant, ..brush };
        disp_paint(&mut d, &face, &set).expect("paint");
        let m = build_disp(&face, &d).expect("mesh");
        let peak = m.positions.iter().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((peak - 72.0).abs() < 1e-9);
        let smooth = PaintBrush { mode: PaintMode::Smooth, amount: 1.0, radius: 200.0, spatial: true, ..brush };
        disp_paint(&mut d, &face, &smooth).expect("paint");
        let m = build_disp(&face, &d).expect("mesh");
        let peak2 = m.positions.iter().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!(peak2 <= 72.0 + 1e-9);
        let alpha = PaintBrush { mode: PaintMode::AlphaPaint, amount: 300.0, falloff: Falloff::Constant, ..brush };
        disp_paint(&mut d, &face, &alpha).expect("paint");
        assert!(d.alphas.contains(&255.0));
        assert!(d.alphas.iter().all(|&a| (0.0..=255.0).contains(&a)));
        let bad = PaintBrush { radius: 0.0, ..brush };
        assert!(disp_paint(&mut d, &face, &bad).is_err());
    }

    #[test]
    fn smooth_moves_along_the_normal_only() {
        let face = top_face();
        let mut d = create_disp(&face, 3).expect("disp");
        let raise = PaintBrush {
            center: DVec3::new(0.0, 64.0, 64.0),
            radius: 80.0,
            amount: 40.0,
            mode: PaintMode::Raise,
            falloff: Falloff::Linear,
            spatial: false,
        };
        disp_paint(&mut d, &face, &raise).expect("raise");
        let before = build_disp(&face, &d).expect("mesh");
        for spatial in [false, true] {
            let mut s = d.clone();
            let smooth = PaintBrush { mode: PaintMode::Smooth, amount: 1.0, spatial, ..raise };
            assert!(disp_paint(&mut s, &face, &smooth).expect("smooth") > 0);
            let after = build_disp(&face, &s).expect("mesh");
            let mut lowered = false;
            for ((p, q), b) in before.positions.iter().zip(&after.positions).zip(&after.base_positions) {
                assert!((q.x - b.x).abs() < 1e-9 && (q.y - b.y).abs() < 1e-9, "{b} slid sideways to {q}");
                lowered |= q.z < p.z - 1.0;
            }
            assert!(lowered, "the bump is smoothed");
            let k = after.base_positions.iter().position(|b| b.x == 0.0 && b.y == 64.0).expect("border vertex");
            assert_eq!((after.positions[k].x, after.positions[k].y), (0.0, 64.0));
        }
    }

    #[test]
    fn triangle_tags_follow_the_slope() {
        let face = top_face();
        let mut d = create_disp(&face, 3).expect("disp");
        assert!(d.triangle_tags.iter().all(|&t| t == DISP_TAG_WALKABLE | DISP_TAG_BUILDABLE));
        d.triangle_tags[5] |= 1 << 1;
        let brush = PaintBrush {
            center: DVec3::new(128.0, 64.0, 64.0),
            radius: 48.0,
            amount: 200.0,
            mode: PaintMode::Raise,
            falloff: Falloff::Linear,
            spatial: false,
        };
        disp_paint(&mut d, &face, &brush).expect("paint");
        let steep = d.triangle_tags.iter().filter(|&&t| t & DISP_TAG_WALKABLE == 0).count();
        assert!(steep > 0, "steep triangles are not walkable");
        assert!(d.triangle_tags.contains(&(DISP_TAG_WALKABLE | DISP_TAG_BUILDABLE)), "flat ones still are");
        assert_eq!(d.triangle_tags[5] & (1 << 1), 1 << 1);
        let m = build_disp(&face, &d).expect("mesh");
        for (tag, t) in d.triangle_tags.iter().zip(m.indices.chunks_exact(3)) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.positions[i as usize]);
            let z = (b - a).cross(c - a).normalize().z;
            assert_eq!(tag & DISP_TAG_WALKABLE != 0, z >= 0.7);
            assert_eq!(tag & DISP_TAG_BUILDABLE != 0, z >= 0.8);
        }
        let sides = hammer_box(DVec3::ZERO, DVec3::new(256.0, 128.0, 64.0));
        let g = build_solid(&sides);
        let wall = g.faces.iter().find(|f| f.plane.normal.z.abs() < 0.5).expect("wall");
        let w = create_disp(&wall.vertices, 2).expect("disp");
        assert!(w.triangle_tags.iter().all(|&t| t == 0));
    }

    #[test]
    fn walkability_follows_source_rules() {
        assert_eq!(slope_walk(1.0), DispWalk::Buildable);
        assert_eq!(slope_walk(0.8), DispWalk::Buildable);
        assert_eq!(slope_walk(0.79), DispWalk::Walkable);
        assert_eq!(slope_walk(0.7), DispWalk::Walkable);
        assert_eq!(slope_walk(0.69), DispWalk::Steep);
        assert_eq!(slope_walk(0.0), DispWalk::Steep);
        assert_eq!(slope_walk(-1.0), DispWalk::Steep);
        assert_eq!(disp_tag_state(DISP_TAG_WALKABLE | DISP_TAG_BUILDABLE), (true, true));
        assert_eq!(disp_tag_state(DISP_TAG_WALKABLE), (true, false));
        assert_eq!(disp_tag_state(0), (false, false));
        let forced_off = DISP_TAG_WALKABLE | DISP_TAG_FORCE_WALKABLE_BIT;
        assert_eq!(disp_tag_state(forced_off), (false, false));
        let forced_on = DISP_TAG_FORCE_WALKABLE_BIT | DISP_TAG_FORCE_WALKABLE_VAL;
        assert_eq!(disp_tag_state(forced_on), (true, false));
        let build_on = DISP_TAG_WALKABLE | DISP_TAG_FORCE_BUILDABLE_BIT | DISP_TAG_FORCE_BUILDABLE_VAL;
        assert_eq!(disp_tag_state(build_on), (true, true));

        let face = top_face();
        let mut d = create_disp(&face, 3).expect("disp");
        let brush = PaintBrush {
            center: DVec3::new(128.0, 64.0, 64.0),
            radius: 48.0,
            amount: 200.0,
            mode: PaintMode::Raise,
            falloff: Falloff::Linear,
            spatial: false,
        };
        disp_paint(&mut d, &face, &brush).expect("paint");
        let m = build_disp(&face, &d).expect("mesh");
        let walk = disp_walk(&d, &m);
        assert_eq!(walk.len(), m.triangle_count());
        assert!(walk.contains(&DispWalk::Steep) && walk.contains(&DispWalk::Buildable));
        let mut untagged = d.clone();
        untagged.triangle_tags.clear();
        assert_eq!(disp_walk(&untagged, &m), walk, "missing tags: the slopes");
        let mut stale = d.clone();
        stale.triangle_tags.iter_mut().for_each(|t| *t = 0);
        assert_eq!(disp_walk(&stale, &m), walk);
        let i = walk.iter().position(|w| *w == DispWalk::Steep).expect("steep triangle");
        d.triangle_tags[i] |= DISP_TAG_FORCE_WALKABLE_BIT | DISP_TAG_FORCE_WALKABLE_VAL;
        assert_eq!(disp_walk(&d, &m)[i], DispWalk::Walkable, "forced walkable, too steep to build");
        let j = walk.iter().position(|w| *w == DispWalk::Buildable).expect("flat triangle");
        d.triangle_tags[j] |= DISP_TAG_FORCE_WALKABLE_BIT;
        assert_eq!(disp_walk(&d, &m)[j], DispWalk::Steep, "forced not walkable");
        d.triangle_tags[j] = DISP_TAG_WALKABLE | DISP_TAG_BUILDABLE | DISP_TAG_FORCE_BUILDABLE_BIT;
        assert_eq!(disp_walk(&d, &m)[j], DispWalk::Walkable, "forced not buildable");
    }

    #[test]
    fn compiled_tags_round_trip() {
        let face = top_face();
        let mut d = create_disp(&face, 3).expect("disp");
        let brush = PaintBrush {
            center: DVec3::new(128.0, 64.0, 64.0),
            radius: 48.0,
            amount: 200.0,
            mode: PaintMode::Raise,
            falloff: Falloff::Linear,
            spatial: false,
        };
        disp_paint(&mut d, &face, &brush).expect("paint");
        let m = build_disp(&face, &d).expect("mesh");
        let compiled = compiled_disp_tags(&d, &m);
        assert!(compiled.iter().all(|t| t & !(DISPTRI_TAG_WALKABLE | DISPTRI_TAG_BUILDABLE) == 0));
        assert!(compiled.contains(&(DISPTRI_TAG_WALKABLE | DISPTRI_TAG_BUILDABLE)) && compiled.contains(&0));
        let back = vmf_tags_from_compiled(&compiled, &m);
        let mut plain = d.clone();
        plain.triangle_tags = back.clone();
        assert_eq!(compiled_disp_tags(&plain, &m), compiled);
        let force = DISP_TAG_FORCE_WALKABLE_BIT | DISP_TAG_FORCE_BUILDABLE_BIT;
        assert!(back.iter().filter(|t| *t & force != 0).count() <= 2, "only slopes at a threshold get pinned");
        let mut odd = compiled.clone();
        let flat = odd.iter().position(|t| *t == DISPTRI_TAG_WALKABLE | DISPTRI_TAG_BUILDABLE).expect("flat");
        let steep = odd.iter().position(|t| *t == 0).expect("steep");
        odd[flat] = 0;
        odd[steep] = DISPTRI_TAG_WALKABLE;
        let back = vmf_tags_from_compiled(&odd, &m);
        assert_eq!(back[flat], DISP_TAG_WALKABLE | DISP_TAG_BUILDABLE | force);
        let walk_on = DISP_TAG_FORCE_WALKABLE_BIT | DISP_TAG_FORCE_WALKABLE_VAL;
        assert_eq!(back[steep] & walk_on, walk_on);
        let mut pinned = d.clone();
        pinned.triangle_tags = back;
        assert_eq!(compiled_disp_tags(&pinned, &m), odd);
    }

    #[test]
    fn invert_alpha_swaps_the_textures() {
        let face = top_face();
        let mut d = create_disp(&face, 2).expect("disp");
        d.alphas[0] = 255.0;
        d.alphas[1] = 100.0;
        d.alphas[2] = f64::NAN;
        d.alphas.truncate(20);
        assert_eq!(invert_alpha(&mut d), 25);
        assert_eq!((d.alphas[0], d.alphas[1], d.alphas[2], d.alphas[24]), (0.0, 155.0, 255.0, 255.0));
        invert_alpha(&mut d);
        assert_eq!((d.alphas[0], d.alphas[1], d.alphas[24]), (255.0, 100.0, 0.0));
        let mut bad = d.clone();
        bad.power = 40;
        bad.alphas.clear();
        assert_eq!(invert_alpha(&mut bad), 0);
        assert!(bad.alphas.is_empty());
    }

    #[test]
    fn noise_is_deterministic_bounded_and_seam_safe() {
        let face = top_face();
        let mut a = create_disp(&face, 3).expect("disp");
        let mut b = a.clone();
        assert_eq!(add_noise(&mut a, &face, -4.0, 8.0, 7).expect("noise"), 81);
        add_noise(&mut b, &face, 8.0, -4.0, 7).expect("noise");
        assert_eq!(a, b, "same seed, same noise (reversed bounds are accepted)");
        let m = build_disp(&face, &a).expect("mesh");
        let heights: Vec<f64> = m.positions.iter().map(|p| p.z - 64.0).collect();
        assert!(heights.iter().all(|h| (-4.0..=8.0).contains(h)));
        let spread =
            heights.iter().copied().fold(f64::MIN, f64::max) - heights.iter().copied().fold(f64::MAX, f64::min);
        assert!(spread > 4.0, "the noise varies");
        for (p, q) in m.positions.iter().zip(&m.base_positions) {
            assert!((p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() < 1e-9);
        }
        let mut c = create_disp(&face, 3).expect("disp");
        add_noise(&mut c, &face, -4.0, 8.0, 8).expect("noise");
        assert_ne!(a, c, "another seed gives other values");
        let sides = hammer_box(DVec3::new(256.0, 0.0, 0.0), DVec3::new(512.0, 128.0, 64.0));
        let nface = build_solid(&sides).face_for_side(0).expect("top").vertices.clone();
        let mut n = create_disp(&nface, 3).expect("disp");
        add_noise(&mut n, &nface, -4.0, 8.0, 7).expect("noise");
        let nm = build_disp(&nface, &n).expect("mesh");
        let mut shared = 0;
        for (p, q) in m.positions.iter().zip(&m.base_positions) {
            if let Some(k) = nm.base_positions.iter().position(|b| b.distance(*q) < 1e-9) {
                assert!((nm.positions[k] - *p).length() < 1e-9);
                shared += 1;
            }
        }
        assert_eq!(shared, 9);
        assert!(add_noise(&mut c, &face, f64::NAN, 1.0, 1).is_err());
        assert!(add_noise(&mut c, &face[..3], 0.0, 1.0, 1).is_err());
    }

    #[test]
    fn scale_multiplies_distances() {
        let face = top_face();
        let mut d = create_disp(&face, 2).expect("disp");
        d.distances[6] = 10.0;
        d.offsets[6] = DVec3::new(0.0, 0.0, 1.0);
        assert_eq!(scale_disp(&mut d, &face, 2.5).expect("scale"), 1);
        assert_eq!(d.distances[6], 25.0);
        assert_eq!(d.offsets[6], DVec3::new(0.0, 0.0, 1.0), "offsets are kept");
        scale_disp(&mut d, &face, -1.0).expect("scale");
        assert_eq!((d.distances[6], d.normals[6]), (25.0, DVec3::NEG_Z));
        assert!(scale_disp(&mut d, &face, f64::INFINITY).is_err());
    }

    #[test]
    fn falloff_profiles() {
        assert_eq!(Falloff::Constant.weight(0.5), 1.0);
        assert_eq!(Falloff::Linear.weight(0.25), 0.75);
        assert_eq!(Falloff::Smooth.weight(0.0), 1.0);
        assert_eq!(Falloff::Smooth.weight(1.0), 0.0);
        assert_eq!(Falloff::Linear.weight(1.5), 0.0);
        assert_eq!(Falloff::Linear.weight(f64::NAN), 0.0);
    }
}
