use glam::{DVec2, DVec3};
use hp_geom::Aabb;
use hp_vmf::Entity;

use super::parse_side_list;
use crate::{Document, FaceRef, Id};

pub const OVERLAY_CLASS: &str = "info_overlay";
pub const TRANSITION_CLASS: &str = "info_overlay_transition";
pub const UV_KEYS: [&str; 4] = ["uv0", "uv1", "uv2", "uv3"];
const MAX_COORD: f64 = 65536.0;
const MIN_FACING: f64 = 0.01;
pub const MAX_FRAGMENTS: usize = 32_768;
const EPS: f64 = 1e-7;

pub fn is_overlay_class(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case(OVERLAY_CLASS)
}

pub fn is_transition_class(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case(TRANSITION_CLASS)
}

pub fn default_basis(normal: DVec3) -> (DVec3, DVec3) {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let major = if n.x.abs() >= n.y.abs() && n.x.abs() >= n.z.abs() { 0 } else { 1 };
    let u0 = if major == 0 { DVec3::Y } else { DVec3::X };
    let u = (u0 - n * u0.dot(n)).try_normalize().unwrap_or(u0);
    let v = n.cross(u).try_normalize().unwrap_or(DVec3::Y);
    (u, v)
}

fn finite_vec(v: DVec3) -> Option<DVec3> {
    (v.is_finite() && v.abs().max_element() <= MAX_COORD).then_some(v)
}

fn key_vec(e: &Entity, key: &str) -> Option<DVec3> {
    e.get(key).and_then(hp_vmf::parse_vec3).and_then(finite_vec)
}

fn key_num(e: &Entity, key: &str, default: f64) -> f64 {
    e.get(key)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map_or(default, |v| v.clamp(-1.0e4, 1.0e4))
}

pub fn format_uv(p: DVec2) -> String {
    hp_vmf::fmt_vec3(DVec3::new(clean(p.x), clean(p.y), 0.0))
}

fn clean(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    let r = x.round();
    if (x - r).abs() < 1e-4 { r + 0.0 } else { (x * 1e4).round() / 1e4 + 0.0 }
}

fn cross2(a: DVec2, b: DVec2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn signed_area2(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| cross2(poly[i], poly[(i + 1) % n])).sum()
}

fn is_convex_2d(poly: &[DVec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f64;
    for i in 0..n {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        let z = cross2(b - a, c - b);
        if z.abs() < 1e-9 {
            continue;
        }
        if sign == 0.0 {
            sign = z.signum();
        } else if z.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

fn clip_convex(subject: &[DVec2], clipper: &[DVec2]) -> Vec<DVec2> {
    let mut poly: Vec<DVec2> = subject.to_vec();
    let n = clipper.len();
    for i in 0..n {
        if poly.len() < 3 {
            return Vec::new();
        }
        let (a, b) = (clipper[i], clipper[(i + 1) % n]);
        let edge = b - a;
        let len = edge.length();
        if len < 1e-12 {
            continue;
        }
        let side = |p: DVec2| cross2(edge, p - a) / len;
        let mut out = Vec::with_capacity(poly.len() + 2);
        for j in 0..poly.len() {
            let (p, q) = (poly[j], poly[(j + 1) % poly.len()]);
            let (dp, dq) = (side(p), side(q));
            let (pin, qin) = (dp >= -EPS, dq >= -EPS);
            if pin {
                out.push(p);
            }
            if pin != qin {
                let t = dp / (dp - dq);
                if t.is_finite() {
                    out.push(p + (q - p) * t);
                }
            }
        }
        poly = out;
    }
    poly.dedup_by(|a, b| (*a - *b).length_squared() < 1e-14);
    if poly.len() > 1 && (poly[0] - poly[poly.len() - 1]).length_squared() < 1e-14 {
        poly.pop();
    }
    if poly.len() >= 3 && signed_area2(&poly).abs() > 1e-9 { poly } else { Vec::new() }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayShape {
    pub origin: DVec3,
    pub u: DVec3,
    pub v: DVec3,
    pub normal: DVec3,
    pub uv: [DVec2; 4],
    pub start: DVec2,
    pub end: DVec2,
    pub render_order: u8,
    pub sides: Vec<Id>,
    pub material: String,
}

impl OverlayShape {
    pub fn from_entity(e: &Entity) -> Option<Self> {
        let origin = key_vec(e, "BasisOrigin").or_else(|| e.origin().and_then(finite_vec))?;
        let normal = key_vec(e, "BasisNormal")?.try_normalize()?;
        let u = key_vec(e, "BasisU")
            .map(|u| u - normal * u.dot(normal))
            .and_then(DVec3::try_normalize)
            .unwrap_or_else(|| default_basis(normal).0);
        let nu = normal.cross(u);
        let v = match key_vec(e, "BasisV") {
            Some(v) if v.dot(nu) < 0.0 => -nu,
            _ => nu,
        };
        let mut uv = [DVec2::ZERO; 4];
        for (slot, key) in uv.iter_mut().zip(UV_KEYS) {
            *slot = key_vec(e, key)?.truncate();
        }
        Some(Self {
            origin,
            u,
            v,
            normal,
            uv,
            start: DVec2::new(key_num(e, "StartU", 0.0), key_num(e, "StartV", 0.0)),
            end: DVec2::new(key_num(e, "EndU", 1.0), key_num(e, "EndV", 1.0)),
            render_order: key_num(e, "RenderOrder", 0.0).clamp(0.0, 3.0) as u8,
            sides: parse_side_list(e.get("sides").unwrap_or("")),
            material: e.get("material").unwrap_or("").trim().to_owned(),
        })
    }

    pub fn to_basis(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.u), d.dot(self.v))
    }

    pub fn from_basis(&self, p: DVec2) -> DVec3 {
        self.origin + self.u * p.x + self.v * p.y
    }

    pub fn corners(&self) -> [DVec3; 4] {
        self.uv.map(|c| self.from_basis(c))
    }

    pub fn bounds(&self) -> Aabb {
        Aabb::from_points(self.corners())
    }

    fn quad_ccw(&self) -> [DVec2; 4] {
        let mut q = self.uv;
        if signed_area2(&q) < 0.0 {
            q.reverse();
        }
        q
    }

    fn quad_pieces(&self) -> Vec<Vec<DVec2>> {
        let q = self.uv;
        if is_convex_2d(&q) {
            return vec![self.quad_ccw().to_vec()];
        }
        let reflex = (0..4).find(|&i| {
            let (a, b, c) = (q[(i + 3) % 4], q[i], q[(i + 1) % 4]);
            cross2(b - a, c - b) * signed_area2(&q) < 0.0
        });
        let k = reflex.unwrap_or(0);
        let tris = [[q[k], q[(k + 1) % 4], q[(k + 2) % 4]], [q[k], q[(k + 2) % 4], q[(k + 3) % 4]]];
        tris.iter()
            .map(|t| {
                let mut t = t.to_vec();
                if signed_area2(&t) < 0.0 {
                    t.reverse();
                }
                t
            })
            .filter(|t| signed_area2(t).abs() > 1e-9)
            .collect()
    }

    pub fn contains(&self, p: DVec2) -> bool {
        self.quad_pieces().iter().any(|piece| {
            let n = piece.len();
            (0..n).all(|i| cross2(piece[(i + 1) % n] - piece[i], p - piece[i]) >= -1e-6)
        })
    }

    pub fn quad_param(&self, p: DVec2) -> DVec2 {
        inverse_bilinear(p, self.uv[0], self.uv[3], self.uv[2], self.uv[1])
    }

    pub fn texcoord(&self, p: DVec2) -> DVec2 {
        let st = self.quad_param(p);
        self.start + (self.end - self.start) * st
    }
}

fn inverse_bilinear(p: DVec2, a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> DVec2 {
    let e = b - a;
    let f = d - a;
    let g = a - b + c - d;
    let h = p - a;
    let k2 = cross2(g, f);
    let k1 = cross2(e, f) + cross2(h, g);
    let k0 = cross2(h, e);
    let solve_s = |t: f64| {
        let den = e + g * t;
        let num = h - f * t;
        if den.x.abs() > den.y.abs() {
            if den.x.abs() > 1e-12 { num.x / den.x } else { 0.0 }
        } else if den.y.abs() > 1e-12 {
            num.y / den.y
        } else {
            0.0
        }
    };
    let inside = |s: f64, t: f64| (-0.01..=1.01).contains(&s) && (-0.01..=1.01).contains(&t);
    let result = if k2.abs() < 1e-9 * (k1.abs() + 1.0) {
        let t = if k1.abs() > 1e-12 { -k0 / k1 } else { 0.0 };
        DVec2::new(solve_s(t), t)
    } else {
        let w = (k1 * k1 - 4.0 * k0 * k2).max(0.0).sqrt();
        let t1 = (-k1 - w) / (2.0 * k2);
        let s1 = solve_s(t1);
        if inside(s1, t1) {
            DVec2::new(s1, t1)
        } else {
            let t2 = (-k1 + w) / (2.0 * k2);
            DVec2::new(solve_s(t2), t2)
        }
    };
    if result.is_finite() { result.clamp(DVec2::splat(-0.01), DVec2::splat(1.01)) } else { DVec2::ZERO }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayFragment {
    pub face: FaceRef,
    pub side_id: Id,
    pub points: Vec<DVec3>,
    pub texcoords: Vec<DVec2>,
    pub normal: DVec3,
}

fn clip_receiver(
    shape: &OverlayShape,
    pieces: &[Vec<DVec2>],
    receiver: &[DVec3],
    normal: DVec3,
    mut emit: impl FnMut(Vec<DVec3>, Vec<DVec2>),
) {
    let facing = normal.dot(shape.normal);
    if receiver.len() < 3 || facing < MIN_FACING {
        return;
    }
    let mut poly: Vec<DVec2> = receiver.iter().map(|p| shape.to_basis(*p)).collect();
    if signed_area2(&poly) < 0.0 {
        poly.reverse();
    }
    let plane_d = normal.dot(receiver[0]);
    for piece in pieces {
        let clipped = clip_convex(&poly, piece);
        if clipped.is_empty() {
            continue;
        }
        let points = clipped
            .iter()
            .map(|p| {
                let base = shape.from_basis(*p);
                base + shape.normal * ((plane_d - normal.dot(base)) / facing)
            })
            .collect();
        let texcoords = clipped.iter().map(|p| shape.texcoord(*p)).collect();
        emit(points, texcoords);
    }
}

pub fn fragments(doc: &Document, shape: &OverlayShape, faces: &dyn Fn(Id) -> Option<FaceRef>) -> Vec<OverlayFragment> {
    let pieces = shape.quad_pieces();
    if pieces.is_empty() {
        return Vec::new();
    }
    let quad_box = {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for c in shape.uv {
            lo = lo.min(c);
            hi = hi.max(c);
        }
        (lo, hi)
    };
    let overlaps = |pts: &[DVec3]| {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for p in pts {
            let q = shape.to_basis(*p);
            lo = lo.min(q);
            hi = hi.max(q);
        }
        lo.x <= quad_box.1.x && hi.x >= quad_box.0.x && lo.y <= quad_box.1.y && hi.y >= quad_box.0.y
    };
    let scene = doc.scene();
    let mut out = Vec::new();
    for &side_id in &shape.sides {
        let Some(face) = faces(side_id) else { continue };
        let Some(node) = scene.solid(face.solid) else { continue };
        if node.solid.sides.get(face.side).map(|s| s.id) != Some(side_id) {
            continue;
        }
        let mut push = |points: Vec<DVec3>, texcoords: Vec<DVec2>, normal: DVec3| {
            if out.len() < MAX_FRAGMENTS {
                out.push(OverlayFragment { face, side_id, points, texcoords, normal });
            }
        };
        if let Some(mesh) = node.disp_mesh(face.side) {
            for tri in mesh.indices.chunks_exact(3) {
                let (Some(a), Some(b), Some(c)) = (
                    mesh.positions.get(tri[0] as usize),
                    mesh.positions.get(tri[1] as usize),
                    mesh.positions.get(tri[2] as usize),
                ) else {
                    continue;
                };
                let tri_pts = [*a, *b, *c];
                if !overlaps(&tri_pts) {
                    continue;
                }
                let Some(n) = (*b - *a).cross(*c - *a).try_normalize() else { continue };
                let n = if n.dot(mesh.face_normal) < 0.0 { -n } else { n };
                clip_receiver(shape, &pieces, &tri_pts, n, |p, t| push(p, t, n));
            }
        } else if let Some(f) = node.geom.face_for_side(face.side) {
            if !overlaps(&f.vertices) {
                continue;
            }
            let n = f.plane.normal;
            clip_receiver(shape, &pieces, &f.vertices, n, |p, t| push(p, t, n));
        }
    }
    out
}

pub fn outline(fragments: &[OverlayFragment]) -> Vec<(DVec3, DVec3)> {
    type Key = (i64, i64, i64);
    let key = |p: DVec3| -> Key {
        let q = (p * 64.0).round();
        (q.x as i64, q.y as i64, q.z as i64)
    };
    let mut count: std::collections::HashMap<(Key, Key), (u32, DVec3, DVec3)> = std::collections::HashMap::new();
    for f in fragments {
        let n = f.points.len();
        for i in 0..n {
            let (a, b) = (f.points[i], f.points[(i + 1) % n]);
            let (ka, kb) = (key(a), key(b));
            if ka == kb {
                continue;
            }
            let k = if ka < kb { (ka, kb) } else { (kb, ka) };
            count.entry(k).and_modify(|c| c.0 += 1).or_insert((1, a, b));
        }
    }
    let mut edges: Vec<(DVec3, DVec3)> = count.into_values().filter(|c| c.0 == 1).map(|c| (c.1, c.2)).collect();
    edges.sort_by(|x, y| {
        x.0.to_array()
            .partial_cmp(&y.0.to_array())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(x.1.to_array().partial_cmp(&y.1.to_array()).unwrap_or(std::cmp::Ordering::Equal))
    });
    edges
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransitionShape {
    pub land: Vec<Id>,
    pub water: Vec<Id>,
    pub width_land: f64,
    pub width_water: f64,
    pub length_tex: DVec2,
    pub width_tex: DVec2,
    pub material: String,
}

impl TransitionShape {
    pub fn from_entity(e: &Entity) -> Option<Self> {
        let land = parse_side_list(e.get("sides").unwrap_or(""));
        let water = parse_side_list(e.get("sides2").unwrap_or(""));
        if land.is_empty() || water.is_empty() {
            return None;
        }
        Some(Self {
            land,
            water,
            width_land: key_num(e, "Width1", 25.0).clamp(0.0, 4096.0),
            width_water: key_num(e, "Width2", 25.0).clamp(0.0, 4096.0),
            length_tex: DVec2::new(key_num(e, "LengthTexcoordStart", 0.0), key_num(e, "LengthTexcoordEnd", 1.0)),
            width_tex: DVec2::new(key_num(e, "WidthTexcoordStart", 0.0), key_num(e, "WidthTexcoordEnd", 1.0)),
            material: e.get("material").unwrap_or("").trim().to_owned(),
        })
    }

    pub fn all_sides(&self) -> impl Iterator<Item = Id> + '_ {
        self.land.iter().chain(&self.water).copied()
    }
}

pub fn transition_fragments(
    doc: &Document,
    shape: &TransitionShape,
    faces: &dyn Fn(Id) -> Option<FaceRef>,
) -> Vec<OverlayFragment> {
    let scene = doc.scene();
    let polygon = |side: Id| {
        let face = faces(side)?;
        let node = scene.solid(face.solid)?;
        (node.solid.sides.get(face.side).map(|s| s.id) == Some(side)).then_some(())?;
        let f = node.geom.face_for_side(face.side)?;
        Some((face, f.vertices.clone(), f.plane.normal))
    };
    let lands: Vec<(FaceRef, Vec<DVec3>, DVec3)> = shape.land.iter().filter_map(|s| polygon(*s)).collect();
    let mut out = Vec::new();
    for &water_side in &shape.water {
        let Some((wface, wpoly, wn)) = polygon(water_side) else { continue };
        let wd = wn.dot(wpoly[0]);
        let shore: Vec<(&[DVec3], DVec3)> = lands.iter().map(|(_, poly, ln)| (poly.as_slice(), *ln)).collect();
        let strips = hp_geom::shoreline::shore_strips(&shore, wn, wd);
        let total: f64 = strips.iter().map(hp_geom::shoreline::ShoreStrip::length).sum();
        if total <= 1e-6 {
            continue;
        }
        let (u_axis, v_axis) = default_basis(wn);
        let water_2d: Vec<DVec2> =
            wpoly.iter().map(|p| DVec2::new((*p - wpoly[0]).dot(u_axis), (*p - wpoly[0]).dot(v_axis))).collect();
        let mut water_ccw = water_2d.clone();
        if signed_area2(&water_ccw) < 0.0 {
            water_ccw.reverse();
        }
        let to2 = |p: DVec3| DVec2::new((p - wpoly[0]).dot(u_axis), (p - wpoly[0]).dot(v_axis));
        let to3 = |p: DVec2| wpoly[0] + u_axis * p.x + v_axis * p.y;
        for strip in strips {
            let (a, b, toward_water, along) = (strip.start, strip.end, strip.toward_water, strip.along);
            let len = strip.length();
            let land_off = -toward_water * shape.width_land;
            let water_off = toward_water * shape.width_water;
            let quad3 = [a + land_off, b + land_off, b + water_off, a + water_off];
            let tex = |l: f64, w: f64| {
                DVec2::new(
                    shape.length_tex.x + (shape.length_tex.y - shape.length_tex.x) * (l / total),
                    shape.width_tex.x + (shape.width_tex.y - shape.width_tex.x) * w,
                )
            };
            let quad_tex = [tex(along, 0.0), tex(along + len, 0.0), tex(along + len, 1.0), tex(along, 1.0)];
            let mut quad2: Vec<DVec2> = quad3.iter().map(|p| to2(*p)).collect();
            let mut texs = quad_tex.to_vec();
            if signed_area2(&quad2) < 0.0 {
                quad2.reverse();
                texs.reverse();
            }
            if signed_area2(&quad2).abs() < 1e-9 {
                continue;
            }
            let clipped = clip_convex(&quad2, &water_ccw);
            if clipped.is_empty() {
                continue;
            }
            let st = |p: DVec2| inverse_bilinear(p, quad2[0], quad2[1], quad2[2], quad2[3]);
            let texcoords = clipped
                .iter()
                .map(|p| {
                    let q = st(*p);
                    let top = texs[0].lerp(texs[1], q.x);
                    let bottom = texs[3].lerp(texs[2], q.x);
                    top.lerp(bottom, q.y)
                })
                .collect();
            if out.len() < MAX_FRAGMENTS {
                out.push(OverlayFragment {
                    face: wface,
                    side_id: water_side,
                    points: clipped.into_iter().map(to3).collect(),
                    texcoords,
                    normal: wn,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::id_set;

    fn doc() -> Document {
        let map = hp_vmf::parse(include_str!("test_map.vmf")).expect("test map");
        Document::from_map(map)
    }

    fn overlay(doc: &Document) -> OverlayShape {
        let e = &doc.scene().entity(20).expect("overlay").entity;
        OverlayShape::from_entity(e).expect("shape")
    }

    fn resolver(doc: &Document, shape: &OverlayShape) -> impl Fn(Id) -> Option<FaceRef> {
        let mut ids = id_set(0);
        ids.extend(shape.sides.iter().copied());
        let found = doc.find_sides(&ids);
        move |id| found.get(&id).copied()
    }

    #[test]
    fn reads_keys() {
        let doc = doc();
        let s = overlay(&doc);
        assert_eq!(s.origin, DVec3::new(240.0, 0.0, 0.0));
        assert_eq!((s.u, s.v, s.normal), (DVec3::X, DVec3::Y, DVec3::Z));
        assert_eq!(s.uv[2], DVec2::new(32.0, 16.0));
        assert_eq!((s.start, s.end), (DVec2::ZERO, DVec2::ONE));
        assert_eq!(s.render_order, 1);
        assert_eq!(s.sides, vec![1]);
        assert_eq!(s.material, "overlays/test");
        assert_eq!(s.corners()[0], DVec3::new(208.0, -16.0, 0.0));
        let mut e = doc.scene().entity(20).expect("overlay").entity.clone();
        e.set("uv2", "nan 1 2");
        assert!(OverlayShape::from_entity(&e).is_none());
        e.set("uv2", "1e300 0 0");
        assert!(OverlayShape::from_entity(&e).is_none());
        e.set("uv2", "32 16 0");
        e.set("BasisNormal", "0 0 0");
        assert!(OverlayShape::from_entity(&e).is_none());
        e.set("BasisNormal", "0 0 2");
        e.set("BasisU", "1 0.5 0.3");
        e.set("BasisV", "0 -1 0");
        let s = OverlayShape::from_entity(&e).expect("shape");
        assert!(s.u.dot(s.normal).abs() < 1e-9 && s.v.dot(s.u).abs() < 1e-9);
        assert!((s.u.cross(s.v) + s.normal).length() < 1e-9, "left handed like the file");
    }

    #[test]
    fn clipped_to_the_floor_edge() {
        let doc = doc();
        let s = overlay(&doc);
        let frags = fragments(&doc, &s, &resolver(&doc, &s));
        assert_eq!(frags.len(), 1);
        let f = &frags[0];
        assert_eq!(f.face, FaceRef::new(2, 0));
        assert_eq!(f.side_id, 1);
        let max_x = f.points.iter().map(|p| p.x).fold(f64::MIN, f64::max);
        assert!((max_x - 256.0).abs() < 1e-9, "{max_x}");
        assert!(f.points.iter().all(|p| p.z.abs() < 1e-9), "on the floor plane");
        for (p, t) in f.points.iter().zip(&f.texcoords) {
            let expect_u = (p.x - 208.0) / 64.0;
            let expect_v = (p.y + 16.0) / 32.0;
            assert!((t.x - expect_u).abs() < 1e-9 && (t.y - expect_v).abs() < 1e-9, "{p} {t}");
        }
        let edges = outline(&frags);
        assert_eq!(edges.len(), 4);
    }

    #[test]
    fn mirrored_and_skewed_texture_coordinates() {
        let doc = doc();
        let mut s = overlay(&doc);
        s.origin = DVec3::ZERO;
        s.start = DVec2::new(1.0, 0.0);
        s.end = DVec2::new(0.0, 2.0);
        assert!((s.texcoord(DVec2::new(-32.0, -16.0)) - DVec2::new(1.0, 0.0)).length() < 1e-9);
        assert!((s.texcoord(DVec2::new(32.0, 16.0)) - DVec2::new(0.0, 2.0)).length() < 1e-9);
        assert!((s.texcoord(DVec2::new(0.0, 0.0)) - DVec2::new(0.5, 1.0)).length() < 1e-9);
        s.uv[2] = DVec2::new(64.0, 40.0);
        s.start = DVec2::ZERO;
        s.end = DVec2::ONE;
        for (i, expect) in [(0, (0.0, 0.0)), (1, (0.0, 1.0)), (2, (1.0, 1.0)), (3, (1.0, 0.0))] {
            let t = s.texcoord(s.uv[i]);
            assert!((t - DVec2::new(expect.0, expect.1)).length() < 1e-6, "corner {i}: {t}");
        }
        let mid = s.texcoord(DVec2::new(10.0, 3.0));
        assert!((0.0..=1.0).contains(&mid.x) && (0.0..=1.0).contains(&mid.y));
    }

    #[test]
    fn concave_quads_and_containment() {
        let doc = doc();
        let mut s = overlay(&doc);
        assert!(s.contains(DVec2::ZERO));
        assert!(!s.contains(DVec2::new(40.0, 0.0)));
        s.uv[2] = DVec2::new(-8.0, -4.0);
        s.origin = DVec3::new(0.0, 0.0, 0.0);
        let frags = fragments(&doc, &s, &resolver(&doc, &s));
        assert_eq!(frags.len(), 2);
        let area: f64 = frags
            .iter()
            .map(|f| {
                let p: Vec<DVec2> = f.points.iter().map(|p| p.truncate()).collect();
                signed_area2(&p).abs() * 0.5
            })
            .sum();
        let q = s.uv;
        let expect = signed_area2(&q).abs() * 0.5;
        assert!((area - expect).abs() < 1e-6, "{area} {expect}");
        assert!(s.contains(DVec2::new(-20.0, -10.0)));
        assert!(!s.contains(DVec2::new(20.0, 10.0)));
    }

    #[test]
    fn faces_seen_edge_on_and_unknown_sides_are_skipped() {
        let doc = doc();
        let mut s = overlay(&doc);
        s.sides = vec![3, 424_242];
        assert!(fragments(&doc, &s, &resolver(&doc, &s)).is_empty());
        let stale = |_: Id| Some(FaceRef::new(2, 4));
        s.sides = vec![1];
        assert!(fragments(&doc, &s, &stale).is_empty());
    }

    #[test]
    fn projected_onto_a_wall_and_a_displacement() {
        let mut doc = doc();
        let mut s = overlay(&doc);
        s.origin = DVec3::new(0.0, 200.0, 64.0);
        s.normal = DVec3::NEG_Y;
        s.u = DVec3::X;
        s.v = DVec3::Z;
        s.sides = vec![12];
        let frags = fragments(&doc, &s, &resolver(&doc, &s));
        assert_eq!(frags.len(), 1);
        assert!(frags[0].points.iter().all(|p| (p.y - 256.0).abs() < 1e-9), "lifted onto the wall");
        doc.create_displacements(&[FaceRef::new(2, 0)], 2).expect("disp");
        let s = overlay(&doc);
        let frags = fragments(&doc, &s, &resolver(&doc, &s));
        assert!(frags.len() > 1, "{}", frags.len());
        let area: f64 = frags
            .iter()
            .map(|f| {
                let p: Vec<DVec2> = f.points.iter().map(|p| p.truncate()).collect();
                signed_area2(&p).abs() * 0.5
            })
            .sum();
        assert!((area - 48.0 * 32.0).abs() < 1e-6, "{area}");
        let edges = outline(&frags);
        let len: f64 = edges.iter().map(|(a, b)| (*b - *a).length()).sum();
        assert!((len - (48.0 * 2.0 + 32.0 * 2.0)).abs() < 1e-6, "{len}");
    }

    #[test]
    fn shoreline_strip() {
        let doc = doc();
        let mut e = Entity { classname: TRANSITION_CLASS.to_owned(), ..Default::default() };
        e.set("sides", "12");
        e.set("sides2", "1");
        e.set("Width1", "8");
        e.set("Width2", "16");
        let t = TransitionShape::from_entity(&e).expect("shape");
        assert_eq!(t.all_sides().collect::<Vec<_>>(), vec![12, 1]);
        let mut ids = id_set(0);
        ids.extend(t.all_sides());
        let found = doc.find_sides(&ids);
        let frags = transition_fragments(&doc, &t, &|id| found.get(&id).copied());
        assert_eq!(frags.len(), 1);
        let f = &frags[0];
        let min_y = f.points.iter().map(|p| p.y).fold(f64::MAX, f64::min);
        let max_y = f.points.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        assert!((min_y - 240.0).abs() < 1e-6 && (max_y - 256.0).abs() < 1e-6, "{min_y} {max_y}");
        assert!(f.points.iter().all(|p| p.z.abs() < 1e-9));
        assert!(f.texcoords.iter().all(|t| (0.0..=1.0 + 1e-9).contains(&t.x) && (-1e-9..=1.0 + 1e-9).contains(&t.y)));
        let mut bad = e.clone();
        bad.set("sides2", "");
        assert!(TransitionShape::from_entity(&bad).is_none());
    }

    #[test]
    fn random_keys_are_safe() {
        let mut doc = doc();
        doc.create_displacements(&[FaceRef::new(3, 5)], 3).expect("disp");
        let mut seed: u64 = 0x1234_5678_9abc_def0;
        let mut next = move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            seed >> 33
        };
        let good = ["0", "1", "-1", "0.5", "1e-300", "300", "-256", "16", "-48", "0.001"];
        let bad = ["nan", "inf", "-inf", "1e308", "-1e308", "", "x"];
        let mut pick = || {
            let r = next() as usize;
            if r.is_multiple_of(20) { bad[(r / 20) % bad.len()] } else { good[(r / 20) % good.len()] }
        };
        let found = doc.find_sides(&(1..=12).collect());
        let mut drawn = 0;
        for _ in 0..3000 {
            let mut e = Entity { classname: OVERLAY_CLASS.to_owned(), ..Default::default() };
            let v3 = |pick: &mut dyn FnMut() -> &'static str| format!("{} {} {}", pick(), pick(), pick());
            e.set("BasisOrigin", v3(&mut pick));
            e.set("BasisNormal", v3(&mut pick));
            e.set("BasisU", v3(&mut pick));
            e.set("BasisV", v3(&mut pick));
            for k in UV_KEYS {
                e.set(k, v3(&mut pick));
            }
            for k in ["StartU", "EndU", "StartV", "EndV", "RenderOrder"] {
                e.set(k, pick());
            }
            e.set("sides", "1 7 12 9 x 99999999999");
            let Some(shape) = OverlayShape::from_entity(&e) else { continue };
            let frags = fragments(&doc, &shape, &|id| found.get(&id).copied());
            for f in &frags {
                assert!(f.points.iter().all(|p| p.is_finite()), "{shape:?}");
                assert!(f.texcoords.iter().all(|t| t.is_finite()), "{shape:?}");
            }
            drawn += usize::from(!frags.is_empty());
            let _ = outline(&frags);
            let _ = shape.contains(DVec2::new(1.0, 2.0));
        }
        assert!(drawn > 50, "the test exercises the clipping: {drawn}");
    }

    #[test]
    fn uv_formatting_and_default_basis() {
        assert_eq!(format_uv(DVec2::new(-64.0, 31.999_999_9)), "-64 32 0");
        assert_eq!(format_uv(DVec2::new(0.25, -0.0)), "0.25 0 0");
        assert_eq!(default_basis(DVec3::Z), (DVec3::X, DVec3::Y));
        assert_eq!(default_basis(DVec3::X), (DVec3::Y, DVec3::Z));
        let n = DVec3::new(0.3, -0.4, 0.8).normalize();
        let (u, v) = default_basis(n);
        assert!((u.cross(v) - n).length() < 1e-9);
    }
}
