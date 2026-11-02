use std::collections::{HashMap, HashSet, VecDeque};

use glam::{DMat3, DVec2, DVec3};
use hp_geom::{ConvexHull, HullFace, Plane, convex_hull, face_axes, validate_solid, world_axes};
use hp_vmf::{Entity, Side, TexAxis};

use crate::{Document, EditError, EditResult, Id, ObjRef, Owner};

pub const NODRAW: &str = "tools/toolsnodraw";

pub const MAX_RENDER_TRIANGLES: usize = 20_000;

pub const MAX_PIECES: usize = 1024;

const DEFAULT_SCALE: f64 = 0.25;

const ON_FACE: f64 = 0.05;
const ON_FACE_DOT: f64 = 0.999;
const NEAR_FACE: f64 = 1.0;
const NEAR_FACE_DOT: f64 = 0.985;

const WELD: f64 = 1.0 / 64.0;
const CONVEX_EPS: f64 = 0.02;
const FLAT_THICKNESS: f64 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub struct ModelTri {
    pub material: String,
    pub pos: [DVec3; 3],
    pub uv: [DVec2; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropTransform {
    pub origin: DVec3,
    pub rotation: DMat3,
    pub scale: f64,
}

impl PropTransform {
    pub fn of(entity: &Entity) -> Self {
        let read = |key: &str| {
            entity.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s > 0.0)
        };
        let scale = read("modelscale").or_else(|| read("uniformscale")).map_or(1.0, |s| s.min(1.0e4));
        let angles = entity.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
        Self {
            origin: entity.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO),
            rotation: hp_geom::angle_matrix(angles),
            scale,
        }
    }

    pub fn apply(&self, p: [f32; 3]) -> DVec3 {
        let p = DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
        self.origin + self.rotation * (p * self.scale)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unpropped {
    pub solids: Vec<Vec<Side>>,
    pub textured_faces: usize,
    pub nodraw_faces: usize,
    pub skipped: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderPieces {
    pub pieces: Vec<Vec<DVec3>>,
    pub approximate: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UnpropError {
    #[error("the model has {0} triangles (at most {MAX_RENDER_TRIANGLES})")]
    TooManyTriangles(usize),
    #[error("the model makes {0} brushes (at most {MAX_PIECES})")]
    TooManyPieces(usize),
    #[error("no piece of the model makes a valid brush")]
    NoBrush,
}

pub fn snap_point(p: DVec3) -> DVec3 {
    let snap = |c: f64| {
        let eighth = (c * 8.0).round() / 8.0;
        let v = if (c - eighth).abs() <= 0.01 { eighth } else { (c * 1024.0).round() / 1024.0 };
        v + 0.0
    };
    DVec3::new(snap(p.x), snap(p.y), snap(p.z))
}

struct PreparedTri<'a> {
    tri: &'a ModelTri,
    plane: Plane,
    area: f64,
    centroid: DVec3,
}

fn prepare(tris: &[ModelTri]) -> Vec<PreparedTri<'_>> {
    tris.iter()
        .filter_map(|t| {
            let [a, b, c] = t.pos;
            let cross = (b - a).cross(c - a);
            let area = cross.length() * 0.5;
            if !area.is_finite() || area < 1e-6 || !t.uv.iter().all(|uv| uv.is_finite()) {
                return None;
            }
            let normal = cross / (area * 2.0);
            Some(PreparedTri {
                tri: t,
                plane: Plane { normal, dist: normal.dot(a) },
                area,
                centroid: (a + b + c) / 3.0,
            })
        })
        .collect()
}

fn plane_basis(normal: DVec3) -> (DVec3, DVec3) {
    let (u, _) = face_axes(normal);
    let e1 = (u - normal * u.dot(normal)).try_normalize().unwrap_or_else(|| normal.any_orthonormal_vector());
    (e1, normal.cross(e1))
}

fn inside_convex(poly: &[DVec2], p: DVec2, margin: f64) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut sign = 0.0f64;
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let edge = b - a;
        let len = edge.length();
        if len < 1e-12 {
            continue;
        }
        let side = edge.perp_dot(p - a) / len;
        if side.abs() <= margin {
            continue;
        }
        if sign == 0.0 {
            sign = side.signum();
        } else if side.signum() != sign {
            return false;
        }
    }
    true
}

struct FaceTexture {
    material: String,
    uaxis: TexAxis,
    vaxis: TexAxis,
}

struct TriMapping {
    gu: DVec2,
    gv: DVec2,
    area: f64,
    tri: usize,
}

fn tri_mapping(t: &PreparedTri<'_>, tri: usize, e1: DVec3, e2: DVec3, size: (u32, u32)) -> Option<TriMapping> {
    let (w, h) = (f64::from(size.0.max(1)), f64::from(size.1.max(1)));
    let q: Vec<DVec2> = t.tri.pos.iter().map(|p| DVec2::new(p.dot(e1), p.dot(e2))).collect();
    let u: Vec<f64> = t.tri.uv.iter().map(|uv| uv.x * w).collect();
    let v: Vec<f64> = t.tri.uv.iter().map(|uv| uv.y * h).collect();
    let (d1, d2) = (q[1] - q[0], q[2] - q[0]);
    let det = d1.perp_dot(d2);
    if det.abs() < 1e-9 {
        return None;
    }
    let solve = |x: &[f64]| {
        let (a, b) = (x[1] - x[0], x[2] - x[0]);
        DVec2::new((a * d2.y - b * d1.y) / det, (d1.x * b - d2.x * a) / det)
    };
    let (gu, gv) = (solve(&u), solve(&v));
    (gu.is_finite() && gv.is_finite()).then_some(TriMapping { gu, gv, area: t.area, tri })
}

fn clean_axis(v: DVec3) -> DVec3 {
    let mut c = DVec3::new(
        if v.x.abs() < 1e-6 { 0.0 } else { v.x },
        if v.y.abs() < 1e-6 { 0.0 } else { v.y },
        if v.z.abs() < 1e-6 { 0.0 } else { v.z },
    );
    c = c.try_normalize().unwrap_or(DVec3::X);
    let a = c.abs();
    let max = a.max_element();
    if max > 1.0 - 1e-6 {
        let pick = |x: f64| if (x.abs() - max).abs() < 1e-12 { x.signum() } else { 0.0 };
        c = DVec3::new(pick(c.x), pick(c.y), pick(c.z));
    }
    let round = |x: f64| {
        let short = (x * 1e4).round() / 1e4;
        if (x - short).abs() < 3e-6 { short } else { (x * 1e6).round() / 1e6 }
    };
    DVec3::new(round(c.x), round(c.y), round(c.z)) + DVec3::ZERO
}

fn clean_scale(s: f64) -> f64 {
    let r = (s * 10_000.0).round() / 10_000.0;
    if r.abs() < 1e-4 { s } else { r }
}

fn clean_shift(shift: f64, size: u32) -> f64 {
    let size = f64::from(size.max(1));
    let mut s = (shift.rem_euclid(size) * 100.0).round() / 100.0;
    if (s - s.round()).abs() < 0.01 {
        s = s.round();
    }
    if s >= size {
        s -= size;
    }
    s + 0.0
}

fn face_matches(plane: &Plane, poly: &[DVec3], tris: &[PreparedTri<'_>], dist: f64, dot: f64) -> Vec<usize> {
    let n = plane.normal;
    let (e1, e2) = plane_basis(n);
    let to2 = |p: DVec3| DVec2::new(p.dot(e1), p.dot(e2));
    let poly2: Vec<DVec2> = poly.iter().map(|&p| to2(p)).collect();
    let center = to2(hp_geom::polygon_center(poly));
    let on_face = |t: &PreparedTri<'_>| {
        t.plane.normal.dot(n) >= dot
            && t.tri.pos.iter().all(|&p| plane.distance(p).abs() <= dist)
            && (inside_convex(&poly2, to2(t.centroid), 0.01) || {
                let corners: Vec<DVec2> = t.tri.pos.iter().map(|&p| to2(p)).collect();
                inside_convex(&corners, center, 0.01)
            })
    };
    tris.iter().enumerate().filter(|(_, t)| on_face(t)).map(|(i, _)| i).collect()
}

fn face_texture(
    n: DVec3,
    matched: &[usize],
    tris: &[PreparedTri<'_>],
    size: &dyn Fn(&str) -> (u32, u32),
) -> Option<FaceTexture> {
    let matched: Vec<&PreparedTri<'_>> = matched.iter().filter_map(|&i| tris.get(i)).collect();
    if matched.is_empty() {
        return None;
    }
    let (e1, e2) = plane_basis(n);
    let mut areas: HashMap<&str, f64> = HashMap::new();
    for t in &matched {
        *areas.entry(t.tri.material.as_str()).or_insert(0.0) += t.area;
    }
    let material = areas
        .iter()
        .max_by(|a, b| a.1.total_cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(m, _)| (*m).to_owned())
        .unwrap_or_default();
    let msize = size(&material);
    let own: Vec<&PreparedTri<'_>> = matched.iter().copied().filter(|t| t.tri.material == material).collect();
    let mut maps: Vec<TriMapping> =
        own.iter().enumerate().filter_map(|(i, t)| tri_mapping(t, i, e1, e2, msize)).collect();
    maps.sort_by(|a, b| b.area.total_cmp(&a.area));
    struct Cluster {
        area: f64,
        gu: DVec2,
        gv: DVec2,
        tris: Vec<usize>,
    }
    let mut clusters: Vec<(usize, Cluster)> = Vec::new();
    for (i, m) in maps.iter().enumerate() {
        let tol = 1e-3 * (m.gu.length() + m.gv.length()) + 1e-9;
        match clusters.iter_mut().find(|(r, _)| (maps[*r].gu - m.gu).length() + (maps[*r].gv - m.gv).length() <= tol) {
            Some((_, c)) => {
                c.area += m.area;
                c.gu += m.gu * m.area;
                c.gv += m.gv * m.area;
                c.tris.push(m.tri);
            }
            None => {
                clusters.push((i, Cluster { area: m.area, gu: m.gu * m.area, gv: m.gv * m.area, tris: vec![m.tri] }))
            }
        }
    }
    let world = || {
        let (u, v) = world_axes(n);
        FaceTexture {
            material: material.clone(),
            uaxis: TexAxis { axis: u, shift: 0.0, scale: DEFAULT_SCALE },
            vaxis: TexAxis { axis: v, shift: 0.0, scale: DEFAULT_SCALE },
        }
    };
    let Some((_, best)) = clusters.iter().max_by(|a, b| a.1.area.total_cmp(&b.1.area)) else {
        return Some(world());
    };
    let (gu, gv) = (best.gu / best.area, best.gv / best.area);
    let (gu3, gv3) = (e1 * gu.x + e2 * gu.y, e1 * gv.x + e2 * gv.y);
    let (lu, lv) = (gu3.length(), gv3.length());
    if lu < 1e-6 || lv < 1e-6 || gu3.cross(gv3).length() < 1e-6 * lu * lv {
        return Some(world());
    }
    let (mut au, mut av) = (gu3 / lu, gv3 / lv);
    if au.dot(av).abs() < 1e-3 {
        if lu >= lv {
            av = (av - au * av.dot(au)).normalize_or(av);
        } else {
            au = (au - av * au.dot(av)).normalize_or(au);
        }
    }
    let (au, av) = (clean_axis(au), clean_axis(av));
    let (su, sv) = (clean_scale(1.0 / lu), clean_scale(1.0 / lv));
    let shift = |axis: DVec3, scale: f64, coord: &dyn Fn(DVec2) -> f64, size: u32| {
        let size = f64::from(size.max(1));
        let (mut first, mut sum, mut count) = (None, 0.0, 0.0);
        for &i in &best.tris {
            let t = own[i];
            for k in 0..3 {
                let r = coord(t.tri.uv[k]) * size - axis.dot(t.tri.pos[k]) / scale;
                let base = *first.get_or_insert(r);
                sum += base + (r - base + size / 2.0).rem_euclid(size) - size / 2.0;
                count += 1.0;
            }
        }
        if count > 0.0 { sum / count } else { 0.0 }
    };
    let (shu, shv) = (shift(au, su, &|uv| uv.x, msize.0), shift(av, sv, &|uv| uv.y, msize.1));
    Some(FaceTexture {
        material,
        uaxis: TexAxis { axis: au, shift: clean_shift(shu, msize.0), scale: su },
        vaxis: TexAxis { axis: av, shift: clean_shift(shv, msize.1), scale: sv },
    })
}

fn face_side(hull: &ConvexHull, face: &HullFace, texture: Option<FaceTexture>, out: &mut Unpropped) -> Side {
    let texture = match texture {
        Some(t) => {
            out.textured_faces += 1;
            t
        }
        None => {
            out.nodraw_faces += 1;
            let (u, v) = world_axes(face.plane.normal);
            FaceTexture {
                material: NODRAW.to_owned(),
                uaxis: TexAxis { axis: u, shift: 0.0, scale: DEFAULT_SCALE },
                vaxis: TexAxis { axis: v, shift: 0.0, scale: DEFAULT_SCALE },
            }
        }
    };
    Side {
        id: 0,
        plane: hull.face_plane_points(face),
        material: texture.material,
        uaxis: texture.uaxis,
        vaxis: texture.vaxis,
        lightmap_scale: 16,
        ..Side::default()
    }
}

pub fn brushes_from_pieces(
    pieces: &[Vec<DVec3>],
    tris: &[ModelTri],
    size: &dyn Fn(&str) -> (u32, u32),
) -> Result<Unpropped, UnpropError> {
    if pieces.len() > MAX_PIECES {
        return Err(UnpropError::TooManyPieces(pieces.len()));
    }
    let prepared = prepare(tris);
    let mut out = Unpropped::default();
    let mut hulls: Vec<ConvexHull> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let points: Vec<DVec3> = piece.iter().map(|&p| snap_point(p)).collect();
        match convex_hull(&points) {
            Ok(hull) => hulls.push(hull),
            Err(_) => out.skipped += 1,
        }
    }
    let mut covered = vec![false; prepared.len()];
    let mut exact: Vec<Vec<Vec<usize>>> = Vec::with_capacity(hulls.len());
    for hull in &hulls {
        let mut faces = Vec::with_capacity(hull.faces.len());
        for f in &hull.faces {
            let m = face_matches(&f.plane, &hull.face_polygon(f), &prepared, ON_FACE, ON_FACE_DOT);
            for &i in &m {
                covered[i] = true;
            }
            faces.push(m);
        }
        exact.push(faces);
    }
    let total: f64 = prepared.iter().map(|t| t.area).sum();
    let on_faces: f64 = prepared.iter().zip(&covered).filter(|(_, c)| **c).map(|(t, _)| t.area).sum();
    let loose = total > 0.0 && on_faces < 0.95 * total;
    for (hull, matches) in hulls.iter().zip(exact) {
        let mut stats = Unpropped::default();
        let mut sides = Vec::with_capacity(hull.faces.len());
        for (face, mut m) in hull.faces.iter().zip(matches) {
            if m.is_empty() && loose {
                m = face_matches(&face.plane, &hull.face_polygon(face), &prepared, NEAR_FACE, NEAR_FACE_DOT);
            }
            let texture = face_texture(face.plane.normal, &m, &prepared, size);
            sides.push(face_side(hull, face, texture, &mut stats));
        }
        if validate_solid(&sides).is_err() {
            out.skipped += 1;
            continue;
        }
        out.textured_faces += stats.textured_faces;
        out.nodraw_faces += stats.nodraw_faces;
        out.solids.push(sides);
    }
    if out.solids.is_empty() {
        return Err(UnpropError::NoBrush);
    }
    Ok(out)
}

fn hull_area_2d(points: &[DVec2]) -> f64 {
    let mut pts: Vec<DVec2> = points.to_vec();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| (*a - *b).length() < 1e-9);
    if pts.len() < 3 {
        return 0.0;
    }
    let mut hull: Vec<DVec2> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &DVec2>> =
            if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 {
                let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
                if (b - a).perp_dot(p - a) <= 0.0 {
                    hull.pop();
                } else {
                    break;
                }
            }
            hull.push(p);
        }
        hull.pop();
    }
    let mut area = 0.0;
    for (i, &a) in hull.iter().enumerate() {
        area += a.perp_dot(hull[(i + 1) % hull.len()]);
    }
    area.abs() * 0.5
}

struct PlaneGroup {
    plane: Plane,
    area: f64,
    points: Vec<DVec3>,
}

pub fn render_pieces(tris: &[ModelTri]) -> Result<RenderPieces, UnpropError> {
    if tris.len() > MAX_RENDER_TRIANGLES {
        return Err(UnpropError::TooManyTriangles(tris.len()));
    }
    let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
    let mut verts: Vec<DVec3> = Vec::new();
    let mut weld = |p: DVec3| {
        let key = [(p.x / WELD).round() as i64, (p.y / WELD).round() as i64, (p.z / WELD).round() as i64];
        *ids.entry(key).or_insert_with(|| {
            verts.push(p);
            (verts.len() - 1) as u32
        })
    };
    struct T {
        v: [u32; 3],
        plane: Plane,
        area: f64,
    }
    let mut mesh: Vec<T> = Vec::new();
    for t in tris {
        let v = [weld(t.pos[0]), weld(t.pos[1]), weld(t.pos[2])];
        if v[0] == v[1] || v[1] == v[2] || v[0] == v[2] {
            continue;
        }
        let [a, b, c] = t.pos;
        let cross = (b - a).cross(c - a);
        let area = cross.length() * 0.5;
        if !area.is_finite() || area < 1e-6 {
            continue;
        }
        let normal = cross / (area * 2.0);
        mesh.push(T { v, plane: Plane { normal, dist: normal.dot(a) }, area });
    }
    let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, t) in mesh.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t.v[k], t.v[(k + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(i);
        }
    }
    let mut order: Vec<usize> = (0..mesh.len()).collect();
    order.sort_by(|&a, &b| mesh[b].area.total_cmp(&mesh[a].area));
    let mut taken = vec![false; mesh.len()];
    let mut out = RenderPieces::default();
    for &seed in &order {
        if taken[seed] {
            continue;
        }
        taken[seed] = true;
        let mut piece = vec![seed];
        let mut corners: Vec<u32> = mesh[seed].v.to_vec();
        let mut corner_set: HashSet<u32> = corners.iter().copied().collect();
        let mut groups = vec![PlaneGroup {
            plane: mesh[seed].plane,
            area: mesh[seed].area,
            points: mesh[seed].v.iter().map(|&i| verts[i as usize]).collect(),
        }];
        let mut rejected: HashSet<usize> = HashSet::new();
        let mut queue: VecDeque<usize> = VecDeque::new();
        let push_neighbors = |t: usize, queue: &mut VecDeque<usize>| {
            for k in 0..3 {
                let (a, b) = (mesh[t].v[k], mesh[t].v[(k + 1) % 3]);
                if let Some(list) = edges.get(&(a.min(b), a.max(b))) {
                    queue.extend(list.iter().copied().filter(|&n| n != t));
                }
            }
        };
        push_neighbors(seed, &mut queue);
        while let Some(c) = queue.pop_front() {
            if taken[c] || rejected.contains(&c) {
                continue;
            }
            let cand = &mesh[c];
            let cand_pts: Vec<DVec3> = cand.v.iter().map(|&i| verts[i as usize]).collect();
            let behind_all = cand_pts.iter().all(|&p| groups.iter().all(|g| g.plane.distance(p) <= CONVEX_EPS));
            let all_behind = corners.iter().all(|&i| cand.plane.distance(verts[i as usize]) <= CONVEX_EPS);
            let group = groups.iter().position(|g| {
                g.plane.normal.dot(cand.plane.normal) > 1.0 - 1e-6
                    && (g.plane.dist - cand.plane.dist).abs() <= CONVEX_EPS
            });
            let flat_convex = match group {
                Some(gi) => {
                    let g = &groups[gi];
                    let (e1, e2) = plane_basis(g.plane.normal);
                    let pts: Vec<DVec2> =
                        g.points.iter().chain(&cand_pts).map(|p| DVec2::new(p.dot(e1), p.dot(e2))).collect();
                    hull_area_2d(&pts) <= (g.area + cand.area) * 1.002 + 1e-3
                }
                None => true,
            };
            if !(behind_all && all_behind && flat_convex) {
                rejected.insert(c);
                continue;
            }
            taken[c] = true;
            piece.push(c);
            for &i in &cand.v {
                if corner_set.insert(i) {
                    corners.push(i);
                }
            }
            match group {
                Some(gi) => {
                    groups[gi].area += cand.area;
                    groups[gi].points.extend(cand_pts);
                }
                None => groups.push(PlaneGroup { plane: cand.plane, area: cand.area, points: cand_pts }),
            }
            push_neighbors(c, &mut queue);
        }
        let mut points: Vec<DVec3> = corners.iter().map(|&i| verts[i as usize]).collect();
        if groups.len() == 1 {
            let n = groups[0].plane.normal;
            let back: Vec<DVec3> = points.iter().map(|&p| p - n * FLAT_THICKNESS).collect();
            points.extend(back);
        } else if let Ok(hull) = convex_hull(&points) {
            let partial = hull.faces.iter().any(|f| {
                let face_area = hp_geom::polygon_area_vector(&hull.face_polygon(f)).length();
                let covered: f64 = groups
                    .iter()
                    .filter(|g| {
                        g.plane.normal.dot(f.plane.normal) > ON_FACE_DOT
                            && (g.plane.dist - f.plane.dist).abs() <= ON_FACE
                    })
                    .map(|g| g.area)
                    .sum();
                face_area > 1e-6 && covered > 0.02 * face_area && covered < 0.98 * face_area
            });
            if partial {
                out.approximate += 1;
            }
        }
        out.pieces.push(points);
        if out.pieces.len() > MAX_PIECES {
            return Err(UnpropError::TooManyPieces(out.pieces.len()));
        }
    }
    Ok(out)
}

impl Document {
    pub fn replace_prop_with_brushes(
        &mut self,
        prop: Id,
        solids: Vec<Vec<Side>>,
        detail: bool,
        keep_prop: bool,
    ) -> EditResult<Vec<Id>> {
        if !self.scene.entities.contains_key(&prop) {
            return Err(EditError::NotFound(ObjRef::Entity(prop)));
        }
        if solids.is_empty() {
            return Err(EditError::Nothing("no brush to create"));
        }
        for sides in &solids {
            validate_solid(sides)?;
        }
        self.tx("Convert prop to brushes", |doc| {
            let ids = if detail {
                let entity = doc.create_brush_entity("func_detail", &[], solids)?;
                doc.scene.expand_solids(&[ObjRef::Entity(entity)])
            } else {
                let ids = doc.create_solids(solids, Owner::World)?;
                if ids.len() > 1 {
                    let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
                    doc.group(&objs)?;
                }
                ids
            };
            if !keep_prop {
                doc.delete(&[ObjRef::Entity(prop)])?;
            }
            Ok(ids)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::box_sides;

    fn render_tris(doc: &Document, ids: &[Id], size: (u32, u32)) -> Vec<ModelTri> {
        let mut out = Vec::new();
        for id in ids {
            let node = doc.scene().solid(*id).expect("solid");
            for face in &node.geom.faces {
                let side = &node.solid.sides[face.side_index];
                if side.material.to_ascii_lowercase().starts_with("tools/") {
                    continue;
                }
                let uv = |p: DVec3| {
                    let t = hp_geom::texel_uv(side, p);
                    DVec2::new(t.x / f64::from(size.0), t.y / f64::from(size.1))
                };
                for t in face.triangles() {
                    let pos = [face.vertices[t[0]], face.vertices[t[1]], face.vertices[t[2]]];
                    out.push(ModelTri { material: side.material.clone(), pos, uv: pos.map(uv) });
                }
            }
        }
        out
    }

    fn pieces(doc: &Document, ids: &[Id]) -> Vec<Vec<DVec3>> {
        ids.iter()
            .map(|id| {
                let node = doc.scene().solid(*id).expect("solid");
                node.geom.faces.iter().flat_map(|f| f.vertices.iter().copied()).collect()
            })
            .collect()
    }

    fn side_on<'a>(sides: &'a [Side], plane: &Plane) -> &'a Side {
        sides
            .iter()
            .find(|s| Plane::from_side(s).is_some_and(|p| p.approx_eq(plane, 1e-6, 1e-6)))
            .unwrap_or_else(|| panic!("no side on {plane:?}"))
    }

    fn assert_axis(a: &TexAxis, b: &TexAxis, size: u32) {
        assert!((a.axis - b.axis).length() < 1e-9, "axis {:?} != {:?}", a.axis, b.axis);
        assert!((a.scale - b.scale).abs() < 1e-9, "scale {} != {}", a.scale, b.scale);
        let (sa, sb) = (a.shift.rem_euclid(f64::from(size)), b.shift.rem_euclid(f64::from(size)));
        assert!((sa - sb).abs() < 1e-6, "shift {sa} != {sb}");
    }

    #[test]
    fn hulls_become_the_original_brushes() {
        let mut doc = Document::new_empty();
        let a = doc
            .create_solid(
                box_sides(DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 64.0), "brick/wall"),
                Owner::World,
            )
            .expect("a");
        let b = doc
            .create_solid(
                box_sides(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 32.0, 32.0), "metal/plate"),
                Owner::World,
            )
            .expect("b");
        let ids = [a, b];
        let noisy: Vec<Vec<DVec3>> = pieces(&doc, &ids)
            .into_iter()
            .map(|p| p.into_iter().map(|v| v + DVec3::new(3e-4, -2e-4, 1e-4)).collect())
            .collect();
        let out = brushes_from_pieces(&noisy, &render_tris(&doc, &ids, (256, 256)), &|_| (256, 256)).expect("brushes");
        assert_eq!(out.solids.len(), 2);
        assert_eq!(out.skipped, 0);
        for (sides, id) in out.solids.iter().zip(ids) {
            let original = &doc.scene().solid(id).expect("solid").solid.sides;
            assert_eq!(sides.len(), 6, "a box has six faces");
            for s in original {
                let plane = Plane::from_side(s).expect("plane");
                let back = side_on(sides, &plane);
                assert!(back.plane.iter().all(|p| *p == p.round()), "on the grid: {:?}", back.plane);
            }
            let geom = validate_solid(sides).expect("valid");
            let lo = geom.faces.iter().flat_map(|f| f.vertices.iter()).fold(DVec3::splat(1e9), |m, v| m.min(*v));
            let hi = geom.faces.iter().flat_map(|f| f.vertices.iter()).fold(DVec3::splat(-1e9), |m, v| m.max(*v));
            let node = doc.scene().solid(id).expect("solid");
            assert_eq!((lo, hi), (node.bounds.min, node.bounds.max));
        }
    }

    #[test]
    fn texture_axes_scales_and_shifts_come_back() {
        let mut doc = Document::new_empty();
        let mut sides = box_sides(DVec3::new(-32.0, -16.0, 0.0), DVec3::new(32.0, 48.0, 40.0), "brick/wall");
        sides[0].uaxis = TexAxis { axis: DVec3::new(0.6, 0.8, 0.0), shift: 17.0, scale: 0.5 };
        sides[0].vaxis = TexAxis { axis: DVec3::new(0.8, -0.6, 0.0), shift: -40.0, scale: 0.125 };
        sides[2].uaxis = TexAxis { axis: DVec3::NEG_X, shift: 100.5, scale: 1.0 };
        sides[2].vaxis = TexAxis { axis: DVec3::Z, shift: 3.0, scale: 0.25 };
        sides[4].material = "metal/plate".to_owned();
        sides[4].uaxis.scale = 0.3;
        sides[4].uaxis.shift = 250.0;
        let id = doc.create_solid(sides, Owner::World).expect("box");
        let sizes = |m: &str| if m == "metal/plate" { (128, 64) } else { (512, 256) };
        let mut tris = Vec::new();
        for (m, size) in [("brick/wall", (512, 256)), ("metal/plate", (128, 64))] {
            tris.extend(render_tris(&doc, &[id], size).into_iter().filter(|t| t.material == m));
        }
        let out = brushes_from_pieces(&pieces(&doc, &[id]), &tris, &sizes).expect("brushes");
        assert_eq!(out.textured_faces, 6);
        assert_eq!(out.nodraw_faces, 0);
        for s in &doc.scene().solid(id).expect("solid").solid.sides {
            let back = side_on(&out.solids[0], &Plane::from_side(s).expect("plane"));
            assert_eq!(back.material, s.material);
            let (w, h) = sizes(&s.material);
            assert_axis(&back.uaxis, &s.uaxis, w);
            assert_axis(&back.vaxis, &s.vaxis, h);
        }
    }

    #[test]
    fn faces_without_render_triangles_are_nodraw() {
        let mut doc = Document::new_empty();
        let mut sides = box_sides(DVec3::ZERO, DVec3::splat(64.0), "brick/wall");
        sides[1].material = "tools/toolsnodraw".to_owned();
        let id = doc.create_solid(sides, Owner::World).expect("box");
        let out = brushes_from_pieces(&pieces(&doc, &[id]), &render_tris(&doc, &[id], (256, 256)), &|_| (256, 256))
            .expect("brushes");
        assert_eq!((out.textured_faces, out.nodraw_faces), (5, 1));
        let bottom = side_on(&out.solids[0], &Plane { normal: DVec3::NEG_Z, dist: 0.0 });
        assert_eq!(bottom.material, NODRAW);
        let far = ModelTri {
            material: "other/mat".to_owned(),
            pos: [DVec3::new(200.0, 0.0, 0.0), DVec3::new(200.0, 64.0, 0.0), DVec3::new(264.0, 0.0, 0.0)],
            uv: [DVec2::ZERO; 3],
        };
        let out = brushes_from_pieces(&pieces(&doc, &[id]), &[far], &|_| (256, 256)).expect("brushes");
        assert_eq!(out.nodraw_faces, 6);
    }

    #[test]
    fn approximate_hulls_take_the_triangles_near_them() {
        let mut doc = Document::new_empty();
        let id = doc.create_solid(box_sides(DVec3::ZERO, DVec3::splat(64.0), "wood/crate"), Owner::World).expect("box");
        let hull: Vec<DVec3> = pieces(&doc, &[id])[0]
            .iter()
            .map(|p| (*p - DVec3::splat(32.0)) * (65.0 / 64.0) + DVec3::splat(32.0))
            .collect();
        let out =
            brushes_from_pieces(&[hull], &render_tris(&doc, &[id], (256, 256)), &|_| (256, 256)).expect("brushes");
        assert_eq!((out.textured_faces, out.nodraw_faces), (6, 0));
        assert!(out.solids[0].iter().all(|s| s.material == "wood/crate"));
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn transforms_and_snapping() {
        let mut e = Entity { classname: "prop_static".to_owned(), ..Entity::default() };
        e.set("origin", "100 0 -8");
        e.set("angles", "0 90 0");
        e.set("modelscale", "2");
        let t = PropTransform::of(&e);
        let p = t.apply([10.0, 0.0, 4.0]);
        assert!((p - DVec3::new(100.0, 20.0, 0.0)).length() < 1e-9, "{p:?}");
        assert_eq!(PropTransform::of(&Entity::default()).scale, 1.0);
        assert_eq!(snap_point(DVec3::new(63.996, -0.004, 12.1249)), DVec3::new(64.0, 0.0, 12.125));
        assert_eq!(snap_point(DVec3::new(10.3, 0.0, 0.0)).x, (10.3f64 * 1024.0).round() / 1024.0);
        assert_eq!(clean_shift(-10.0, 256), 246.0);
        assert_eq!(clean_shift(512.004, 256), 0.0);
        assert_eq!(clean_axis(DVec3::new(1e-9, -1.0, 2e-7)), DVec3::NEG_Y);
        assert_eq!(clean_axis(DVec3::new(0.6000014, 0.7999990, 0.0)), DVec3::new(0.6, 0.8, 0.0));
        let diagonal = clean_axis(DVec3::new(1.0, 1.0, 0.0));
        assert_eq!(diagonal, DVec3::new(0.707107, 0.707107, 0.0));
    }

    fn volume(solids: &[Vec<Side>]) -> f64 {
        solids
            .iter()
            .map(|s| {
                let geom = validate_solid(s).expect("valid");
                let points: Vec<DVec3> = geom.faces.iter().flat_map(|f| f.vertices.iter().copied()).collect();
                convex_hull(&points).expect("hull").volume()
            })
            .sum()
    }

    #[test]
    fn render_meshes_split_into_convex_pieces() {
        let mut doc = Document::new_empty();
        let a = doc.create_solid(box_sides(DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0), "brick/wall"), Owner::World);
        let b = doc.create_solid(
            box_sides(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 32.0, 32.0), "brick/wall"),
            Owner::World,
        );
        let c = doc.create_solid(
            box_sides(DVec3::new(300.0, 0.0, 0.0), DVec3::new(332.0, 32.0, 16.0), "brick/wall"),
            Owner::World,
        );
        let ids = [a.expect("a"), b.expect("b"), c.expect("c")];
        let tris = render_tris(&doc, &ids, (256, 256));
        let split = render_pieces(&tris).expect("pieces");
        assert_eq!(split.pieces.len(), 3, "one piece per box");
        assert_eq!(split.approximate, 0);
        let out = brushes_from_pieces(&split.pieces, &tris, &|_| (256, 256)).expect("brushes");
        let expected = 64.0f64.powi(3) + 64.0 * 32.0 * 32.0 + 32.0 * 32.0 * 16.0;
        assert!((volume(&out.solids) - expected).abs() < 1e-6, "{} != {expected}", volume(&out.solids));
        let card = ModelTri {
            material: "sign/a".to_owned(),
            pos: [DVec3::ZERO, DVec3::new(32.0, 0.0, 0.0), DVec3::new(0.0, 32.0, 0.0)],
            uv: [DVec2::ZERO, DVec2::X, DVec2::Y],
        };
        let split = render_pieces(std::slice::from_ref(&card)).expect("pieces");
        let out = brushes_from_pieces(&split.pieces, &[card], &|_| (32, 32)).expect("brushes");
        assert!((volume(&out.solids) - 32.0 * 32.0 * 0.5).abs() < 1e-6);
        assert_eq!(out.textured_faces, 1);
        assert_eq!(
            render_pieces(&vec![tris[0].clone(); MAX_RENDER_TRIANGLES + 1]).unwrap_err(),
            UnpropError::TooManyTriangles(MAX_RENDER_TRIANGLES + 1)
        );
    }

    #[test]
    fn replacing_a_prop_is_one_undo_step() {
        for (detail, keep) in [(true, false), (false, true), (false, false)] {
            let mut doc = Document::new_empty();
            let prop = doc
                .create_point_entity("prop_static", DVec3::ZERO, &[("model".to_owned(), "models/x.mdl".to_owned())])
                .expect("prop");
            let solids = vec![
                box_sides(DVec3::ZERO, DVec3::splat(16.0), "a/b"),
                box_sides(DVec3::new(16.0, 0.0, 0.0), DVec3::new(32.0, 16.0, 16.0), "a/b"),
            ];
            let before = doc.undo_labels().len();
            let ids = doc.replace_prop_with_brushes(prop, solids, detail, keep).expect("replace");
            assert_eq!(doc.undo_labels().len(), before + 1, "one step");
            assert_eq!(doc.undo_label(), Some("Convert prop to brushes"));
            assert_eq!(ids.len(), 2);
            assert_eq!(doc.scene().entity(prop).is_some(), keep);
            let owner_is_detail = doc.scene().entities().values().any(|e| e.entity.classname == "func_detail");
            assert_eq!(owner_is_detail, detail);
            assert!(!doc.selection().objects.is_empty(), "the brushes are selected");
            assert!(doc.undo());
            assert!(doc.scene().entity(prop).is_some(), "undo brings the prop back");
            assert!(ids.iter().all(|id| doc.scene().solid(*id).is_none()));
        }
        let mut doc = Document::new_empty();
        assert!(doc.replace_prop_with_brushes(12345, vec![], false, false).is_err());
    }
}
