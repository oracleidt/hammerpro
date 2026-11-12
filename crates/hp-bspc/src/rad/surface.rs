use glam::{DMat3, DVec2, DVec3, Vec3A};
use hp_light::bvh::{Bvh, Triangle};

use super::data::{CONTENTS_SOLID, Face, MASK_OPAQUE, MapData};
use crate::materials::surf;

const MAX_SIDE: i32 = 512;
pub(crate) const SKY_BIT: u32 = 1 << 31;

#[derive(Clone, Debug)]
pub(crate) struct DispSurf {
    pub n: usize,
    pub pos: Vec<DVec3>,
    pub normals: Vec<DVec3>,
}

impl DispSurf {
    fn at(&self, u: usize, v: usize) -> DVec3 {
        self.pos.get(v * self.n + u).copied().unwrap_or(DVec3::ZERO)
    }

    pub fn sample(&self, uv: DVec2, push: f64, face_normal: DVec3) -> (DVec3, DVec3) {
        let n = self.n;
        let fu = uv.x.clamp(0.0, 1.0) * (n as f64 - 1.000001);
        let fv = uv.y.clamp(0.0, 1.0) * (n as f64 - 1.000001);
        let (su, sv) = ((fu as usize).min(n - 2), (fv as usize).min(n - 2));
        let (nu, nv) = (su + 1, sv + 1);
        let (a, b) = (fu - su as f64, fv - sv as f64);
        let odd = (sv * n + su) % 2 == 1;
        let (p, e1, e2) = if odd {
            if a + b >= 1.0 {
                let o = self.at(nu, nv);
                (
                    o + (self.at(su, nv) - o) * (1.0 - a) + (self.at(nu, sv) - o) * (1.0 - b),
                    self.at(su, nv) - o,
                    self.at(nu, sv) - o,
                )
            } else {
                let o = self.at(su, sv);
                (o + (self.at(nu, sv) - o) * a + (self.at(su, nv) - o) * b, self.at(nu, sv) - o, self.at(su, nv) - o)
            }
        } else if a < b {
            let o = self.at(su, nv);
            (
                o + (self.at(nu, nv) - o) * a + (self.at(su, sv) - o) * (1.0 - b),
                self.at(nu, nv) - o,
                self.at(su, sv) - o,
            )
        } else {
            let o = self.at(nu, sv);
            (
                o + (self.at(su, sv) - o) * (1.0 - a) + (self.at(nu, nv) - o) * b,
                self.at(su, sv) - o,
                self.at(nu, nv) - o,
            )
        };
        let mut tri_n = e1.cross(e2).normalize_or_zero();
        if tri_n.dot(face_normal) < 0.0 {
            tri_n = -tri_n;
        }
        let nrm = |u: usize, v: usize| self.normals.get(v * n + u).copied().unwrap_or(face_normal);
        let top = (nrm(su, sv) * (1.0 - a) + nrm(nu, sv) * a).normalize_or_zero();
        let bottom = (nrm(su, nv) * (1.0 - a) + nrm(nu, nv) * a).normalize_or_zero();
        let normal = (top * (1.0 - b) + bottom * b).try_normalize().unwrap_or(face_normal);
        (p + tri_n * push, normal)
    }

    pub fn triangles(&self) -> Vec<([DVec3; 3], [DVec2; 3])> {
        let n = self.n;
        let inv = 1.0 / (n as f64 - 1.0);
        let uv = |u: usize, v: usize| DVec2::new(u as f64 * inv, v as f64 * inv);
        let mut out = Vec::with_capacity((n - 1) * (n - 1) * 2);
        for v in 0..n - 1 {
            for u in 0..n - 1 {
                let corners = if (v * n + u) % 2 == 1 {
                    [[(u, v), (u, v + 1), (u + 1, v)], [(u, v + 1), (u + 1, v + 1), (u + 1, v)]]
                } else {
                    [[(u, v), (u, v + 1), (u + 1, v + 1)], [(u, v), (u + 1, v + 1), (u + 1, v)]]
                };
                for t in corners {
                    out.push((t.map(|(a, b)| self.at(a, b)), t.map(|(a, b)| uv(a, b))));
                }
            }
        }
        out
    }
}

pub(crate) fn disp_surface(map: &MapData, face: &Face) -> Option<DispSurf> {
    let d = map.face_disp(face)?;
    if !(2..=4).contains(&d.power) {
        return None;
    }
    let n = (1usize << d.power) + 1;
    let pts = map.face_points(face);
    if pts.len() != 4 {
        return None;
    }
    let first =
        (0..4).min_by(|a, b| pts[*a].distance_squared(d.start).total_cmp(&pts[*b].distance_squared(d.start)))?;
    let p: [DVec3; 4] = std::array::from_fn(|k| pts[(first + k) % 4]);
    let verts = map.disp_verts.get(d.vert_start..d.vert_start + n * n)?;
    let inv = 1.0 / (n - 1) as f64;
    let mut pos = Vec::with_capacity(n * n);
    for row in 0..n {
        let e0 = p[0].lerp(p[1], row as f64 * inv);
        let e1 = p[3].lerp(p[2], row as f64 * inv);
        for col in 0..n {
            let (dir, dist) = verts[row * n + col];
            let q = e0.lerp(e1, col as f64 * inv) + dir * dist;
            pos.push(if q.is_finite() { q } else { e0.lerp(e1, col as f64 * inv) });
        }
    }
    let plane = map.face_plane(face)?;
    let mut s = DispSurf { n, pos, normals: Vec::new() };
    let mut normals = vec![DVec3::ZERO; n * n];
    let p = |u: usize, v: usize| s.at(u, v);
    let tri = |a: DVec3, b: DVec3, c: DVec3| (c - a).cross(b - a).normalize_or_zero();
    for v in 0..n {
        for u in 0..n {
            let (left, right, down, up) = (u > 0, u + 1 < n, v > 0, v + 1 < n);
            let mut acc = DVec3::ZERO;
            let mut count = 0.0;
            if up && right {
                acc += tri(p(u, v), p(u, v + 1), p(u + 1, v)) + tri(p(u + 1, v), p(u, v + 1), p(u + 1, v + 1));
                count += 2.0;
            }
            if left && up {
                acc += tri(p(u - 1, v), p(u - 1, v + 1), p(u, v)) + tri(p(u, v), p(u - 1, v + 1), p(u, v + 1));
                count += 2.0;
            }
            if left && down {
                acc += tri(p(u - 1, v - 1), p(u - 1, v), p(u, v - 1)) + tri(p(u, v - 1), p(u - 1, v), p(u, v));
                count += 2.0;
            }
            if right && down {
                acc += tri(p(u, v - 1), p(u, v), p(u + 1, v - 1)) + tri(p(u + 1, v - 1), p(u, v), p(u + 1, v));
                count += 2.0;
            }
            let nrm = if count > 0.0 { acc / count } else { DVec3::ZERO };
            normals[v * n + u] = if nrm.length_squared() > 1e-12 { nrm } else { plane.normal };
        }
    }
    s.normals = normals;
    Some(s)
}

pub(crate) fn bump_normals(s: DVec3, t: DVec3, flat: DVec3, phong: DVec3) -> [DVec3; 3] {
    const BASIS: [[f64; 3]; 3] = [
        [0.816_496_610_641_479_5, 0.0, 0.577_350_258_827_209_5],
        [-0.408_248_215_913_772_6, 0.707_106_769_084_930_4, 0.577_350_258_827_209_5],
        [-0.408_248_215_913_772_6, -0.707_106_769_084_930_4, 0.577_350_258_827_209_5],
    ];
    let left = flat.dot(s.cross(t)) < 0.0;
    let mut b1 = phong.cross(s).normalize_or_zero();
    let b0 = b1.cross(phong).normalize_or_zero();
    if left {
        b1 = -b1;
    }
    BASIS.map(|l| b0 * l[0] + b1 * l[1] + phong * l[2])
}

#[derive(Clone, Debug)]
pub(crate) struct LitFace {
    pub face: usize,
    pub w: usize,
    pub h: usize,
    pub bump: bool,
    pub normal: DVec3,
    pub tex: [DVec3; 2],
    pub frame: Option<(DVec3, DVec3, DVec3)>,
    pub mins: [i32; 2],
    pub poly2: Vec<DVec2>,
    pub poly: Vec<DVec3>,
    pub disp: Option<DispSurf>,
    pub reflectivity: Vec3A,
    pub minlight: f32,
    pub bounds: (DVec3, DVec3),
    pub lm: [[f64; 4]; 2],
    pub offset: DVec3,
    pub vnormals: Option<Vec<DVec3>>,
}

impl LitFace {
    pub fn luxels(&self) -> usize {
        self.w * self.h
    }

    pub fn plane_point(&self, s: f64, t: f64) -> DVec3 {
        match self.frame {
            Some((o, a, b)) => o + a * (s + f64::from(self.mins[0])) + b * (t + f64::from(self.mins[1])),
            None => self.poly.iter().copied().sum::<DVec3>() / self.poly.len().max(1) as f64,
        }
    }

    pub fn phong(&self, p: DVec3) -> DVec3 {
        let Some(vn) = &self.vnormals else { return self.normal };
        let center = self.poly.iter().copied().sum::<DVec3>() / self.poly.len().max(1) as f64;
        phong_normal(&self.poly, center, self.normal, vn, p)
    }

    pub fn luxel_coords(&self, p: DVec3) -> DVec2 {
        let q = p - self.offset;
        let c = |k: usize| {
            q.dot(DVec3::new(self.lm[k][0], self.lm[k][1], self.lm[k][2])) + self.lm[k][3] - f64::from(self.mins[k])
        };
        DVec2::new(c(0), c(1))
    }
}

pub(crate) fn phong_normal(poly: &[DVec3], center: DVec3, normal: DVec3, vn: &[DVec3], p: DVec3) -> DVec3 {
    let n = poly.len();
    if n < 3 || vn.len() != n {
        return normal;
    }
    let spot = p - center;
    for j in 0..n {
        let (v1, v2) = (poly[j] - center, poly[(j + 1) % n] - center);
        let (aa, bb, ab) = (v1.dot(v1), v2.dot(v2), v1.dot(v2));
        let det = aa * bb - ab * ab;
        if det.abs() < 1e-12 || bb.abs() < 1e-12 {
            continue;
        }
        let a1 = (bb * v1.dot(spot) - ab * spot.dot(v2)) / det;
        let a2 = (spot.dot(v2) - a1 * ab) / bb;
        if a1 >= 0.0 && a2 >= 0.0 {
            let blend = normal * (1.0 - a1 - a2) + vn[j] * a1 + vn[(j + 1) % n] * a2;
            return blend.try_normalize().unwrap_or(normal);
        }
    }
    normal
}

fn clip(poly: &[DVec2], axis: usize, limit: f64, keep_above: bool) -> Vec<DVec2> {
    let side = |p: &DVec2| if keep_above { p[axis] - limit } else { limit - p[axis] };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (side(&a), side(&b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            out.push(a + (b - a) * t);
        }
    }
    out
}

fn area_centroid(poly: &[DVec2]) -> (f64, DVec2) {
    let mut area = 0.0;
    let mut c = DVec2::ZERO;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let cr = a.x * b.y - b.x * a.y;
        area += cr;
        c += (a + b) * cr;
    }
    if area.abs() < 1e-12 {
        let n = poly.len().max(1) as f64;
        return (0.0, poly.iter().copied().sum::<DVec2>() / n);
    }
    (area.abs() * 0.5, c / (3.0 * area))
}

fn closest_on_poly(poly: &[DVec2], p: DVec2) -> DVec2 {
    let mut best = poly.first().copied().unwrap_or(p);
    let mut bd = f64::INFINITY;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let ab = b - a;
        let t = if ab.length_squared() > 1e-18 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        let q = a + ab * t;
        let d = q.distance_squared(p);
        if d < bd {
            bd = d;
            best = q;
        }
    }
    best
}

pub(crate) fn inside(poly: &[DVec2], p: DVec2) -> bool {
    let mut sign = 0.0f64;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let c = (b - a).perp_dot(p - a);
        if c.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

impl LitFace {
    pub fn flat_sample(&self, s: usize, t: usize) -> (DVec2, Vec<DVec2>) {
        let c = DVec2::new(s as f64, t as f64);
        if self.poly2.len() < 3 {
            return (c, Vec::new());
        }
        let mut piece = clip(&self.poly2, 0, c.x - 0.5, true);
        piece = clip(&piece, 0, c.x + 0.5, false);
        piece = clip(&piece, 1, c.y - 0.5, true);
        piece = clip(&piece, 1, c.y + 0.5, false);
        if piece.len() >= 3 {
            let (area, centroid) = area_centroid(&piece);
            if area > 1e-4 {
                return (centroid, piece);
            }
        }
        let q = closest_on_poly(&self.poly2, c);
        let (_, center) = area_centroid(&self.poly2);
        let inward = (center - q).normalize_or_zero() * 0.05;
        (q + inward, Vec::new())
    }
}

pub(crate) fn lit_faces(map: &MapData, smooth_angle: f64) -> Vec<Option<LitFace>> {
    let mut face_model = vec![0usize; map.faces.len()];
    for (mi, m) in map.models.iter().enumerate().rev() {
        let end = m.first_face.saturating_add(m.num_faces).min(map.faces.len());
        for slot in face_model.iter_mut().take(end).skip(m.first_face) {
            *slot = mi;
        }
    }
    let mut model_ent: Vec<Option<&hp_bsp::Entity>> = vec![None; map.models.len()];
    for e in &map.entities {
        if let Some(k) = e.get("model").and_then(|m| m.strip_prefix('*')).and_then(|k| k.parse::<usize>().ok())
            && let Some(slot) = model_ent.get_mut(k)
        {
            *slot = Some(e);
        }
    }
    if let Some(slot) = model_ent.first_mut() {
        *slot = map.entities.first();
    }
    let vec3 = |s: Option<&str>| -> DVec3 {
        let v: Vec<f64> = s.unwrap_or("").split_ascii_whitespace().filter_map(|x| x.parse().ok()).collect();
        if v.len() == 3 && v.iter().all(|x| x.is_finite()) { DVec3::new(v[0], v[1], v[2]) } else { DVec3::ZERO }
    };
    let vertex_normals = smoothed_normals(map, smoothing_threshold(smooth_angle));
    use rayon::prelude::*;
    let mut out = (0..map.faces.len())
        .into_par_iter()
        .map(|i| {
            let f = &map.faces[i];
            let ti = usize::try_from(f.texinfo).ok()?;
            let tex = map.texinfo.get(ti)?;
            if tex.flags & (surf::SKY | surf::NOLIGHT) != 0 {
                return None;
            }
            if f.size[0] < 0 || f.size[1] < 0 || f.size[0] >= MAX_SIDE || f.size[1] >= MAX_SIDE {
                return None;
            }
            let plane = map.face_plane(f)?;
            let rel = map.face_points(f);
            if rel.len() < 3 {
                return None;
            }
            let model = face_model[i];
            let ent = model_ent.get(model).copied().flatten();
            let offset = if model > 0 { vec3(ent.and_then(|e| e.get("origin"))) } else { DVec3::ZERO };
            let minlight = ent
                .and_then(|e| e.get("_minlight"))
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .map_or(0.0, |v| (v * 128.0).max(0.0));
            let lm = tex.lm.map(|r| r.map(f64::from));
            let sv = DVec3::new(lm[0][0], lm[0][1], lm[0][2]);
            let tv = DVec3::new(lm[1][0], lm[1][1], lm[1][2]);
            let m = DMat3::from_cols(sv, tv, plane.normal).transpose();
            let frame = (m.determinant().abs() > 1e-12).then(|| {
                let inv = m.inverse();
                let o = inv * DVec3::new(-lm[0][3], -lm[1][3], plane.dist) + offset;
                (o, inv.col(0), inv.col(1))
            });
            let mins = f.mins;
            let poly2: Vec<DVec2> = rel
                .iter()
                .map(|p| {
                    DVec2::new(p.dot(sv) + lm[0][3] - f64::from(mins[0]), p.dot(tv) + lm[1][3] - f64::from(mins[1]))
                })
                .collect();
            let poly: Vec<DVec3> = rel.iter().map(|p| *p + offset).collect();
            let disp = if f.disp >= 0 { disp_surface(map, f) } else { None };
            let pts: &[DVec3] = disp.as_ref().map_or(&poly, |d| &d.pos);
            let lo = pts.iter().fold(DVec3::splat(f64::INFINITY), |a, p| a.min(*p));
            let hi = pts.iter().fold(DVec3::splat(f64::NEG_INFINITY), |a, p| a.max(*p));
            let refl =
                usize::try_from(tex.texdata).ok().and_then(|t| map.reflectivity.get(t)).copied().unwrap_or([0.0; 3]);
            let texv =
                |k: usize| DVec3::new(f64::from(tex.tex[k][0]), f64::from(tex.tex[k][1]), f64::from(tex.tex[k][2]));
            Some(LitFace {
                face: i,
                w: f.size[0] as usize + 1,
                h: f.size[1] as usize + 1,
                bump: tex.flags & surf::BUMPLIGHT != 0,
                normal: plane.normal,
                tex: [texv(0), texv(1)],
                frame,
                mins,
                poly2,
                poly,
                disp,
                reflectivity: Vec3A::from_array(refl),
                minlight,
                bounds: (lo, hi),
                lm,
                offset,
                vnormals: if f.disp >= 0 { None } else { vertex_normals.get(i).cloned().flatten() },
            })
        })
        .collect::<Vec<_>>();
    smooth_disp_normals(&mut out);
    out
}

fn disp_edges(n: usize) -> ([usize; 4], [Vec<usize>; 4]) {
    let at = |u: usize, v: usize| v * n + u;
    let corners = [at(0, 0), at(0, n - 1), at(n - 1, n - 1), at(n - 1, 0)];
    let edges = [
        (0..n).map(|v| at(0, v)).collect(),
        (0..n).map(|u| at(u, n - 1)).collect(),
        (0..n).map(|v| at(n - 1, v)).collect(),
        (0..n).map(|u| at(u, 0)).collect(),
    ];
    (corners, edges)
}

type VertexCells = ahash::AHashMap<(i64, i64, i64), Vec<(usize, usize, bool)>>;

pub(crate) fn smooth_disp_normals(faces: &mut [Option<LitFace>]) {
    let ids: Vec<usize> = (0..faces.len()).filter(|&i| faces[i].as_ref().is_some_and(|f| f.disp.is_some())).collect();
    if ids.len() < 2 {
        return;
    }
    let key = |p: DVec3| (p.x.round() as i64, p.y.round() as i64, p.z.round() as i64);
    let mut cells: VertexCells = ahash::AHashMap::new();
    for &i in &ids {
        let d = faces[i].as_ref().and_then(|f| f.disp.as_ref()).expect("displacement");
        let (corners, edges) = disp_edges(d.n);
        for e in &edges {
            for &k in e {
                cells.entry(key(d.pos[k])).or_default().push((i, k, corners.contains(&k)));
            }
        }
    }
    for list in cells.values_mut() {
        list.sort_unstable();
        list.dedup();
    }
    let pos = |faces: &[Option<LitFace>], i: usize, k: usize| {
        faces[i].as_ref().and_then(|f| f.disp.as_ref()).map_or(DVec3::ZERO, |d| d.pos[k])
    };
    let normal = |faces: &[Option<LitFace>], i: usize, k: usize| {
        faces[i].as_ref().and_then(|f| f.disp.as_ref()).map_or(DVec3::ZERO, |d| d.normals[k])
    };
    let set = |faces: &mut [Option<LitFace>], i: usize, k: usize, n: DVec3| {
        if let Some(d) = faces[i].as_mut().and_then(|f| f.disp.as_mut()) {
            d.normals[k] = n;
        }
    };
    let at = |faces: &[Option<LitFace>], i: usize, p: DVec3, corner: Option<bool>| -> Vec<(usize, usize)> {
        let (x, y, z) = key(p);
        let mut out: Vec<(usize, usize)> = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for &(j, k, c) in cells.get(&(x + dx, y + dy, z + dz)).map_or(&[][..], |v| v.as_slice()) {
                        if j != i
                            && corner.is_none_or(|want| want == c)
                            && pos(faces, j, k).distance(p) <= 0.1
                            && !out.iter().any(|o| o.0 == j)
                        {
                            out.push((j, k));
                        }
                    }
                }
            }
        }
        out
    };
    let norm = |v: DVec3, fallback: DVec3| v.try_normalize().unwrap_or(fallback);
    for &i in &ids {
        let n = faces[i].as_ref().and_then(|f| f.disp.as_ref()).map_or(0, |d| d.n);
        let (_, edges) = disp_edges(n);
        for e in &edges {
            let mid = e[(n - 1) / 2];
            let p = pos(faces, i, mid);
            let others = at(faces, i, p, Some(true));
            if others.len() >= 2 {
                let mut avg = normal(faces, i, mid);
                for &(j, k) in &others[..2] {
                    avg += normal(faces, j, k);
                }
                let avg = norm(avg, normal(faces, i, mid));
                set(faces, i, mid, avg);
                for &(j, k) in &others[..2] {
                    set(faces, j, k, avg);
                }
            }
        }
    }
    for &i in &ids {
        let n = faces[i].as_ref().and_then(|f| f.disp.as_ref()).map_or(0, |d| d.n);
        let (corners, _) = disp_edges(n);
        for c in corners {
            let p = pos(faces, i, c);
            let others = at(faces, i, p, Some(true));
            if others.is_empty() {
                continue;
            }
            let mut avg = normal(faces, i, c);
            for &(j, k) in &others {
                avg += normal(faces, j, k);
            }
            let avg = norm(avg, normal(faces, i, c));
            set(faces, i, c, avg);
            for &(j, k) in &others {
                set(faces, j, k, avg);
            }
        }
    }
    for &i in &ids {
        let n = faces[i].as_ref().and_then(|f| f.disp.as_ref()).map_or(0, |d| d.n);
        let (_, edges) = disp_edges(n);
        for e in &edges {
            let shared_end = |faces: &[Option<LitFace>], k: usize| !at(faces, i, pos(faces, i, k), None).is_empty();
            let mut anchors = vec![(0usize, shared_end(faces, e[0]))];
            for (idx, &k) in e.iter().enumerate().take(n - 1).skip(1) {
                let p = pos(faces, i, k);
                if let Some(&(j, kk)) = at(faces, i, p, Some(false)).first() {
                    let avg = norm(normal(faces, i, k) + normal(faces, j, kk), normal(faces, i, k));
                    set(faces, i, k, avg);
                    set(faces, j, kk, avg);
                    anchors.push((idx, true));
                } else if !at(faces, i, p, Some(true)).is_empty() {
                    anchors.push((idx, true));
                }
            }
            anchors.push((n - 1, shared_end(faces, e[n - 1])));
            for w in anchors.windows(2) {
                let ((a, ra), (b, rb)) = (w[0], w[1]);
                if !(ra && rb) || b - a < 2 || b - a > (n - 1) / 2 {
                    continue;
                }
                for t in a + 1..b {
                    let f = (t - a) as f64 / (b - a) as f64;
                    let blended = normal(faces, i, e[a]).lerp(normal(faces, i, e[b]), f);
                    let k = e[t];
                    set(faces, i, k, norm(blended, normal(faces, i, k)));
                }
            }
        }
    }
}

const SMOOTHING_GROUP_HARD_EDGE: u32 = 0xff00_0000;
#[allow(clippy::approx_constant)]
pub(crate) const SMOOTHING_THRESHOLD: f64 = 0.707_106_7;

pub(crate) fn smoothing_threshold(degrees: f64) -> f64 {
    if degrees == 45.0 || !degrees.is_finite() { SMOOTHING_THRESHOLD } else { degrees.to_radians().cos() }
}

pub(crate) fn smoothed_normals(map: &MapData, threshold: f64) -> Vec<Option<Vec<DVec3>>> {
    let verts: Vec<Vec<usize>> = map.faces.iter().map(|f| map.face_vertices(f)).collect();
    let normals: Vec<DVec3> = map.faces.iter().map(|f| map.face_plane(f).map_or(DVec3::ZERO, |p| p.normal)).collect();
    let mut users: Vec<Vec<u32>> = vec![Vec::new(); map.verts.len()];
    for (i, vs) in verts.iter().enumerate() {
        for &v in vs {
            if users[v].last() != Some(&(i as u32)) {
                users[v].push(i as u32);
            }
        }
    }
    use rayon::prelude::*;
    (0..map.faces.len())
        .into_par_iter()
        .map(|i| {
            let f = &map.faces[i];
            if f.disp >= 0 || verts[i].is_empty() {
                return None;
            }
            let n0 = normals[i];
            let mut out = Vec::with_capacity(verts[i].len());
            let mut flat = true;
            for &v in &verts[i] {
                let mut sum = DVec3::ZERO;
                let mut seen: Vec<u32> = Vec::new();
                for &k in &users[v] {
                    let k_us = k as usize;
                    if k_us == i || seen.contains(&k) {
                        continue;
                    }
                    seen.push(k);
                    let other = &map.faces[k_us];
                    if other.disp >= 0 {
                        continue;
                    }
                    let nk = normals[k_us];
                    let smooth = if f.smoothing == 0 && other.smoothing == 0 {
                        n0.dot(nk) >= threshold
                    } else {
                        let g = f.smoothing & other.smoothing;
                        g & SMOOTHING_GROUP_HARD_EDGE == 0 && g != 0
                    };
                    if smooth {
                        sum += nk;
                    }
                }
                let n = (sum + n0).try_normalize().unwrap_or(n0);
                if n.dot(n0) < 1.0 - 0.001 {
                    flat = false;
                }
                out.push(n);
            }
            (!flat).then_some(out)
        })
        .collect()
}

fn base_winding(n: DVec3, d: f64) -> Vec<DVec3> {
    let up = if n.z.abs() > 0.9 { DVec3::X } else { DVec3::Z };
    let u = n.cross(up).normalize();
    let v = u.cross(n);
    let c = n * d;
    let r = 131_072.0;
    vec![c - u * r - v * r, c + u * r - v * r, c + u * r + v * r, c - u * r + v * r]
}

fn chop_behind(poly: &[DVec3], n: DVec3, d: f64) -> Vec<DVec3> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (a.dot(n) - d, b.dot(n) - d);
        if da <= 0.0 {
            out.push(a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            out.push(a + (b - a) * (da / (da - db)));
        }
    }
    out
}

fn brushes_under(map: &MapData, head: i32, out: &mut Vec<usize>) {
    let mut seen = vec![false; map.brushes.len()];
    let mut stack = vec![head];
    let mut steps = 0usize;
    while let Some(n) = stack.pop() {
        steps += 1;
        if steps > map.nodes.len() + map.leafs.len() + 1 {
            break;
        }
        if n < 0 {
            let Some(leaf) = map.leafs.get((-1 - n) as usize) else { continue };
            for k in leaf.first_brush..leaf.first_brush + leaf.num_brushes {
                if let Some(&b) = map.leafbrushes.get(k)
                    && let Some(s) = seen.get_mut(usize::from(b))
                    && !*s
                {
                    *s = true;
                    out.push(usize::from(b));
                }
            }
        } else if let Some(node) = map.nodes.get(n as usize) {
            stack.push(node.children[0]);
            stack.push(node.children[1]);
        }
    }
}

fn angles_matrix(a: DVec3) -> DMat3 {
    let (sp, cp) = a.x.to_radians().sin_cos();
    let (sy, cy) = a.y.to_radians().sin_cos();
    let (sr, cr) = a.z.to_radians().sin_cos();
    let forward = DVec3::new(cp * cy, cp * sy, -sp);
    let left = DVec3::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp);
    let up = DVec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    DMat3::from_cols(forward, left, up)
}

fn brush_triangles(map: &MapData, b: usize, xf: &dyn Fn(DVec3) -> DVec3, only_nodraw: bool, out: &mut Vec<Triangle>) {
    let Some(brush) = map.brushes.get(b) else { return };
    if brush.contents & MASK_OPAQUE == 0 {
        return;
    }
    let Some(sides) = map.sides.get(brush.first_side..brush.first_side + brush.num_sides) else { return };
    for (i, side) in sides.iter().enumerate() {
        let flags = usize::try_from(side.texinfo).ok().and_then(|t| map.texinfo.get(t)).map_or(0, |t| t.flags);
        let Some(p) = map.planes.get(side.plane) else { continue };
        if flags & surf::SKY != 0 || (only_nodraw && flags & surf::NODRAW == 0) {
            continue;
        }
        let mut w = base_winding(p.normal, p.dist);
        for (j, other) in sides.iter().enumerate() {
            if i == j || other.bevel {
                continue;
            }
            let Some(q) = map.planes.get(other.plane) else { continue };
            w = chop_behind(&w, q.normal, q.dist);
            if w.len() < 3 {
                break;
            }
        }
        if w.len() < 3 {
            continue;
        }
        for k in 2..w.len() {
            let t = [xf(w[0]), xf(w[k - 1]), xf(w[k])].map(|v| v.as_vec3a());
            out.push(Triangle { p: t, payload: 0 });
        }
    }
}

fn world_brush_triangles(map: &MapData, only_nodraw: bool, out: &mut Vec<Triangle>) {
    let mut list = Vec::new();
    if let Some(m) = map.models.first() {
        brushes_under(map, m.head, &mut list);
    }
    let id = |p: DVec3| p;
    for b in list {
        brush_triangles(map, b, &id, only_nodraw, out);
    }
}

fn entity_brush_triangles(map: &MapData, out: &mut Vec<Triangle>) {
    for e in &map.entities {
        let casts = e.get("vrad_brush_cast_shadows").and_then(|v| v.trim().parse::<i32>().ok()).unwrap_or(0) != 0;
        let Some(k) = e.get("model").and_then(|m| m.strip_prefix('*')).and_then(|k| k.parse::<usize>().ok()) else {
            continue;
        };
        let Some(model) = map.models.get(k).filter(|_| casts && k > 0) else { continue };
        let num = |key: &str| -> DVec3 {
            let v: Vec<f64> =
                e.get(key).unwrap_or("").split_ascii_whitespace().filter_map(|x| x.parse().ok()).collect();
            if v.len() == 3 && v.iter().all(|x| x.is_finite()) { DVec3::new(v[0], v[1], v[2]) } else { DVec3::ZERO }
        };
        let (origin, rot) = (num("origin"), angles_matrix(num("angles")));
        let xf = move |p: DVec3| rot * p + origin;
        let mut list = Vec::new();
        brushes_under(map, model.head, &mut list);
        for b in list {
            brush_triangles(map, b, &xf, false, out);
        }
    }
}

pub(crate) fn blocker_scene(map: &MapData) -> Bvh {
    let mut tris = Vec::new();
    world_brush_triangles(map, true, &mut tris);
    entity_brush_triangles(map, &mut tris);
    Bvh::from_vec(tris)
}

pub(crate) fn water_scene(map: &MapData) -> (Bvh, Vec3A) {
    let mut tris = Vec::new();
    let (mut sum, mut count) = (Vec3A::ZERO, 0.0f32);
    if let Some(m) = map.models.first() {
        for f in m.first_face..m.first_face.saturating_add(m.num_faces).min(map.faces.len()) {
            let face = &map.faces[f];
            if map.face_flags(face) & surf::WARP == 0 {
                continue;
            }
            let pts = map.face_points(face);
            if pts.len() < 3 {
                continue;
            }
            let refl = usize::try_from(face.texinfo)
                .ok()
                .and_then(|t| map.texinfo.get(t))
                .and_then(|t| usize::try_from(t.texdata).ok())
                .and_then(|t| map.reflectivity.get(t))
                .copied()
                .unwrap_or([0.0; 3]);
            sum += Vec3A::from_array(refl);
            count += 1.0;
            for k in 1..pts.len() - 1 {
                tris.push(Triangle { p: [pts[0], pts[k], pts[k + 1]].map(|v| v.as_vec3a()), payload: f as u32 });
            }
        }
    }
    let refl = if count > 0.0 { (sum / count).clamp(Vec3A::ZERO, Vec3A::splat(0.99)) } else { Vec3A::ZERO };
    (Bvh::from_vec(tris), refl)
}

pub(crate) fn shadow_scene(
    map: &MapData,
    faces: &[Option<LitFace>],
    extra: Vec<Triangle>,
    disp_shadows: bool,
) -> (Bvh, bool) {
    let mut tris = extra;
    world_brush_triangles(map, false, &mut tris);
    entity_brush_triangles(map, &mut tris);
    let mut has_sky = false;
    if let Some(m) = map.models.first() {
        for f in m.first_face..m.first_face.saturating_add(m.num_faces).min(map.faces.len()) {
            let face = &map.faces[f];
            if map.face_flags(face) & surf::SKY == 0 {
                continue;
            }
            let pts = map.face_points(face);
            for k in 2..pts.len() {
                has_sky = true;
                tris.push(Triangle { p: [pts[0], pts[k - 1], pts[k]].map(|v| v.as_vec3a()), payload: SKY_BIT });
            }
        }
    }
    for lf in faces.iter().flatten() {
        let casts = disp_shadows && !map.faces.get(lf.face).and_then(|f| map.face_disp(f)).is_some_and(|d| d.no_shadow);
        if !casts {
            continue;
        }
        if let Some(d) = &lf.disp {
            let tri = d.triangles();
            let facing: f64 = tri.iter().map(|(t, _)| (t[1] - t[0]).cross(t[2] - t[0]).dot(lf.normal)).sum();
            for (t, _) in tri {
                let t = if facing > 0.0 { [t[0], t[2], t[1]] } else { t };
                tris.push(Triangle { p: t.map(|v| v.as_vec3a()), payload: 0 });
            }
        }
    }
    (Bvh::from_vec_groups(tris, super::props::shadow_class, super::props::SHADOW_GROUPS), has_sky)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SurfTri {
    Sky,
    Sky2D,
    Dark,
    Flat(u32),
    Disp(u32, [[f32; 2]; 3]),
}

pub(crate) const OPAQUE_SURFACES: u8 = 1;
pub(crate) const CLEAR_SURFACES: u8 = 2;

pub(crate) fn see_through_faces(map: &MapData) -> Vec<bool> {
    let mut out = vec![false; map.faces.len()];
    let Some(m) = map.models.first() else { return out };
    let mut brushes = Vec::new();
    brushes_under(map, m.head, &mut brushes);
    brushes.retain(|&b| map.brushes.get(b).is_some_and(|b| b.contents != 0 && b.contents & MASK_OPAQUE == 0));
    if brushes.is_empty() {
        return out;
    }
    let mut by_plane: ahash::AHashMap<usize, Vec<usize>> = ahash::AHashMap::new();
    for f in m.first_face..m.first_face.saturating_add(m.num_faces).min(map.faces.len()) {
        by_plane.entry(map.faces[f].plane & !1).or_default().push(f);
    }
    let warp = |texinfo: i32| {
        usize::try_from(texinfo).ok().and_then(|t| map.texinfo.get(t)).is_some_and(|t| t.flags & surf::WARP != 0)
    };
    for b in brushes {
        let brush = &map.brushes[b];
        let Some(sides) = map.sides.get(brush.first_side..brush.first_side + brush.num_sides) else { continue };
        let planes: Vec<_> = sides.iter().filter(|s| !s.bevel).filter_map(|s| map.planes.get(s.plane)).collect();
        for side in sides.iter().filter(|s| !s.bevel && s.texinfo >= 0) {
            for &f in by_plane.get(&(side.plane & !1)).into_iter().flatten() {
                let face = &map.faces[f];
                if out[f] || !(face.texinfo == side.texinfo || (warp(face.texinfo) && warp(side.texinfo))) {
                    continue;
                }
                let pts = map.face_points(face);
                if pts.is_empty() {
                    continue;
                }
                let c = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
                out[f] = planes.iter().all(|p| c.dot(p.normal) - p.dist <= 0.1);
            }
        }
    }
    out
}

pub(crate) fn surface_scene(
    map: &MapData,
    faces: &[Option<LitFace>],
    lit_index: &[Option<u32>],
) -> (Bvh, Vec<SurfTri>) {
    let mut tris = Vec::new();
    let mut info = Vec::new();
    let mut classes: Vec<u8> = Vec::new();
    let Some(m) = map.models.first() else { return (Bvh::new(&[]), info) };
    let clear = see_through_faces(map);
    for f in m.first_face..m.first_face.saturating_add(m.num_faces).min(map.faces.len()) {
        let face = &map.faces[f];
        let flags = map.face_flags(face);
        if flags & surf::NODRAW != 0 {
            continue;
        }
        let li = lit_index.get(f).copied().flatten();
        let class = match (clear.get(f).copied().unwrap_or(false), li) {
            (true, None) => continue,
            (true, Some(_)) => CLEAR_SURFACES,
            (false, _) => OPAQUE_SURFACES,
        };
        if let Some(lf) = faces.get(f).and_then(Option::as_ref)
            && li.is_some()
            && let Some(d) = &lf.disp
        {
            let up = lf.normal.as_vec3a();
            for (mut t, mut uv) in d.triangles() {
                if (t[1] - t[0]).cross(t[2] - t[0]).as_vec3a().dot(up) < 0.0 {
                    t.swap(1, 2);
                    uv.swap(1, 2);
                }
                tris.push(Triangle { p: t.map(|v| v.as_vec3a()), payload: info.len() as u32 });
                info.push(SurfTri::Disp(li.unwrap_or(0), uv.map(|c| [c.x as f32, c.y as f32])));
                classes.push(OPAQUE_SURFACES);
            }
            continue;
        }
        let kind = if flags & surf::SKY != 0 {
            if flags & surf::SKY2D != 0 { SurfTri::Sky2D } else { SurfTri::Sky }
        } else {
            match li {
                Some(i) => SurfTri::Flat(i),
                None => SurfTri::Dark,
            }
        };
        let pts = map.face_points(face);
        for k in 2..pts.len() {
            tris.push(Triangle { p: [pts[0], pts[k - 1], pts[k]].map(|v| v.as_vec3a()), payload: info.len() as u32 });
            info.push(kind);
            classes.push(class);
        }
    }
    let class_of = |p: u32| classes.get(p as usize).copied().unwrap_or(OPAQUE_SURFACES);
    (Bvh::from_vec_groups(tris, class_of, &[CLEAR_SURFACES]), info)
}

pub(crate) fn sky_clusters(map: &MapData) -> Vec<i32> {
    let mut out = Vec::new();
    for leaf in &map.leafs {
        if leaf.contents & CONTENTS_SOLID != 0 || leaf.cluster < 0 {
            continue;
        }
        let sky = (leaf.first_face..leaf.first_face + leaf.num_faces).any(|k| {
            map.leaffaces
                .get(k)
                .and_then(|f| map.faces.get(usize::from(*f)))
                .is_some_and(|f| map.face_flags(f) & surf::SKY != 0)
        });
        if sky && !out.contains(&leaf.cluster) {
            out.push(leaf.cluster);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_basis_is_vrads() {
        let b = bump_normals(DVec3::X, -DVec3::Y, DVec3::Z, DVec3::Z);
        for v in b {
            assert!((v.length() - 1.0).abs() < 1e-6);
            assert!((v.z - 0.577_350_258_827_209_5).abs() < 1e-6);
        }
        assert!(b[0].x > 0.8);
        let sum: DVec3 = b.iter().copied().sum();
        assert!(sum.x.abs() < 1e-6 && sum.y.abs() < 1e-6);
    }

    #[test]
    fn clipping() {
        let sq = vec![DVec2::new(0.0, 0.0), DVec2::new(2.0, 0.0), DVec2::new(2.0, 2.0), DVec2::new(0.0, 2.0)];
        let c = clip(&sq, 0, 1.0, true);
        let (a, cen) = area_centroid(&c);
        assert!((a - 2.0).abs() < 1e-9 && (cen.x - 1.5).abs() < 1e-9);
        assert!(inside(&sq, DVec2::new(1.0, 1.0)));
        assert!(!inside(&sq, DVec2::new(3.0, 1.0)));
        let w = chop_behind(&base_winding(DVec3::Z, 0.0), DVec3::X, 10.0);
        assert!(w.iter().all(|p| p.x <= 10.0 + 1e-6));
    }

    #[test]
    fn disp_sampling_matches_grid() {
        let n = 5;
        let pos = (0..n * n).map(|k| DVec3::new((k % n) as f64 * 10.0, (k / n) as f64 * 10.0, 0.0)).collect();
        let d = DispSurf { n, pos, normals: vec![DVec3::Z; n * n] };
        let (p, nrm) = d.sample(DVec2::new(0.5, 0.25), 1.0, DVec3::Z);
        assert!((p - DVec3::new(20.0, 10.0, 1.0)).length() < 1e-3, "{p}");
        assert!((nrm - DVec3::Z).length() < 1e-9);
        assert_eq!(d.triangles().len(), 2 * 16);
    }
}
