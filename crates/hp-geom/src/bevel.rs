use std::f64::consts::PI;

use glam::{DQuat, DVec3};
use hp_vmf::Side;

use crate::brush::validate_solid;
use crate::csg::{ClipPiece, clip_solid_traced};
use crate::extrude::{dress, merge_pieces, prism};
use crate::texture::wrap_texture;
use crate::util::clean_decimal;
use crate::{FaceGeom, GeomError, PLANE_EPS, Plane, SolidGeom, build_solid};

pub const MAX_BEVEL_SEGMENTS: u32 = 32;

pub const MIN_BEVEL_RADIUS: f64 = 0.25;

pub const MAX_BEVEL_RADIUS: f64 = 16384.0;

pub const MIN_BEVEL_CHORD: f64 = 0.5;

pub const BEVEL_SMOOTHING_GROUP: u32 = 1;

const VERTEX_EPS: f64 = 0.05;

const FLAT_EPS: f64 = 0.02;

const PROBE: f64 = 0.5;

const NORMAL_MATCH: f64 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BevelIssue {
    NoEdge,
    NotAnEdge,
    Displacement,
    NoRadius,
    Flat,
    NoRoom,
    Failed,
}

impl BevelIssue {
    pub const ALL: [BevelIssue; 7] = [
        BevelIssue::NoEdge,
        BevelIssue::NotAnEdge,
        BevelIssue::Displacement,
        BevelIssue::NoRadius,
        BevelIssue::Flat,
        BevelIssue::NoRoom,
        BevelIssue::Failed,
    ];

    pub fn message(self) -> &'static str {
        match self {
            BevelIssue::NoEdge => "there is no edge to bevel",
            BevelIssue::NotAnEdge => "the points are not an edge of the brush",
            BevelIssue::Displacement => "the brush has a displacement",
            BevelIssue::NoRadius => "the radius must be at least 0.25 unit",
            BevelIssue::Flat => "the two faces of the edge are flat, there is no corner to bevel",
            BevelIssue::NoRoom => "the faces around the edge leave no room for a bevel",
            BevelIssue::Failed => "the bevel does not make valid brushes",
        }
    }

    pub fn of(error: &GeomError) -> Option<Self> {
        match error {
            GeomError::InvalidInput(m) => Self::ALL.into_iter().find(|i| i.message() == m),
            _ => None,
        }
    }
}

impl From<BevelIssue> for GeomError {
    fn from(issue: BevelIssue) -> Self {
        GeomError::InvalidInput(issue.message().to_owned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BevelKind {
    Round,
    Cove,
    Fill,
}

impl BevelKind {
    pub fn id(self) -> &'static str {
        match self {
            BevelKind::Round => "round",
            BevelKind::Cove => "cove",
            BevelKind::Fill => "fill",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BevelParams {
    pub radius: f64,
    pub segments: u32,
    pub inverse: bool,
    pub material: Option<String>,
}

impl Default for BevelParams {
    fn default() -> Self {
        Self { radius: 16.0, segments: 4, inverse: false, material: None }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BevelResult {
    pub pieces: Vec<ClipPiece>,
    pub added: Vec<Vec<Side>>,
    pub radius: f64,
    pub segments: u32,
    pub kinds: Vec<BevelKind>,
}

impl BevelResult {
    pub fn changes_brush(&self) -> bool {
        self.pieces.len() != 1 || self.pieces.iter().any(|p| p.origins.iter().any(Option::is_none))
    }
}

#[derive(Clone, Copy, Debug)]
struct Corner {
    a: DVec3,
    d: DVec3,
    len: f64,
    sides: [usize; 2],
    faces: [usize; 2],
    n: [DVec3; 2],
    t: [DVec3; 2],
    reach: [f64; 2],
}

#[derive(Clone, Copy, Debug)]
struct Fill {
    covered: usize,
    neighbor: usize,
    reach: f64,
}

#[derive(Clone, Copy, Debug)]
struct Plan {
    corner: Corner,
    kind: BevelKind,
    fill: Option<Fill>,
    u: DVec3,
    v: DVec3,
    limit: f64,
}

impl Plan {
    fn angle(&self) -> f64 {
        self.u.dot(self.v).clamp(-1.0, 1.0).acos()
    }
}

fn vertex_index(f: &FaceGeom, p: DVec3) -> Option<usize> {
    f.vertices.iter().position(|v| v.distance_squared(p) <= VERTEX_EPS * VERTEX_EPS)
}

fn edge_faces(geom: &SolidGeom, a: DVec3, b: DVec3) -> Vec<usize> {
    geom.faces
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let n = f.vertices.len();
            match (vertex_index(f, a), vertex_index(f, b)) {
                (Some(i), Some(j)) => i != j && ((i + 1) % n == j || (j + 1) % n == i),
                _ => false,
            }
        })
        .map(|(i, _)| i)
        .collect()
}

fn reach(f: &FaceGeom, a: DVec3, t: DVec3) -> f64 {
    f.vertices.iter().map(|v| (*v - a).dot(t)).fold(0.0f64, f64::max)
}

fn corner(geom: &SolidGeom, a: DVec3, b: DVec3) -> Result<Corner, GeomError> {
    let faces = edge_faces(geom, a, b);
    let [i, j] = faces[..] else { return Err(BevelIssue::NotAnEdge.into()) };
    let (f1, f2) = (&geom.faces[i], &geom.faces[j]);
    let a = vertex_index(f1, a).map_or(a, |k| f1.vertices[k]);
    let b = vertex_index(f1, b).map_or(b, |k| f1.vertices[k]);
    let len = a.distance(b);
    if !len.is_finite() || len < 0.01 {
        return Err(BevelIssue::NotAnEdge.into());
    }
    let d = (b - a) / len;
    let t_of = |f: &FaceGeom| -> Option<DVec3> {
        let t = f.plane.normal.cross(d).try_normalize()?;
        Some(if (f.center() - a).dot(t) < 0.0 { -t } else { t })
    };
    let (Some(t1), Some(t2)) = (t_of(f1), t_of(f2)) else { return Err(BevelIssue::NotAnEdge.into()) };
    let theta = t1.dot(t2).clamp(-1.0, 1.0).acos();
    if !(FLAT_EPS..=PI - FLAT_EPS).contains(&theta) {
        return Err(BevelIssue::Flat.into());
    }
    Ok(Corner {
        a,
        d,
        len,
        sides: [f1.side_index, f2.side_index],
        faces: [i, j],
        n: [f1.plane.normal, f2.plane.normal],
        t: [t1, t2],
        reach: [reach(f1, a, t1), reach(f2, a, t2)],
    })
}

fn find_fill(geom: &SolidGeom, c: &Corner, neighbors: &[SolidGeom]) -> Option<Fill> {
    let mid = c.a + c.d * (c.len * 0.5);
    for covered in 0..2 {
        let plane = geom.faces[c.faces[covered]].plane.flip();
        let probe = mid + c.n[covered] * PROBE - c.t[covered] * PROBE;
        for (k, g) in neighbors.iter().enumerate() {
            if !g.valid {
                continue;
            }
            let Some(face) = g.faces.iter().find(|f| f.plane.approx_eq(&plane, NORMAL_MATCH, PLANE_EPS)) else {
                continue;
            };
            if g.faces.iter().all(|f| f.plane.distance(probe) < -PLANE_EPS) {
                return Some(Fill { covered, neighbor: k, reach: reach(face, c.a, -c.t[covered]) });
            }
        }
    }
    None
}

fn on_plane(plane: &Plane, p: DVec3) -> bool {
    plane.distance(p).abs() <= PLANE_EPS
}

fn plans(
    sides: &[Side],
    edges: &[[DVec3; 2]],
    neighbors: &[SolidGeom],
    inverse: bool,
) -> Result<(SolidGeom, Vec<Plan>), GeomError> {
    if edges.is_empty() {
        return Err(BevelIssue::NoEdge.into());
    }
    let geom = validate_solid(sides)?;
    if sides.iter().any(|s| s.disp.is_some()) {
        return Err(BevelIssue::Displacement.into());
    }
    let mut corners: Vec<Corner> = Vec::with_capacity(edges.len());
    for [a, b] in edges {
        let c = corner(&geom, *a, *b)?;
        let same = |o: &Corner| {
            let (p, q) = (o.a, o.a + o.d * o.len);
            let (r, s) = (c.a, c.a + c.d * c.len);
            (p.distance(r) < VERTEX_EPS && q.distance(s) < VERTEX_EPS)
                || (p.distance(s) < VERTEX_EPS && q.distance(r) < VERTEX_EPS)
        };
        if !corners.iter().any(same) {
            corners.push(c);
        }
    }
    let mut out = Vec::with_capacity(corners.len());
    for (i, c) in corners.iter().enumerate() {
        let mut reach = c.reach;
        for (k, slot) in reach.iter_mut().enumerate() {
            let plane = geom.faces[c.faces[k]].plane;
            for (j, o) in corners.iter().enumerate() {
                if i == j || c.d.dot(o.d).abs() < 1.0 - 1e-6 {
                    continue;
                }
                if !on_plane(&plane, o.a) || !on_plane(&plane, o.a + o.d * o.len) {
                    continue;
                }
                let gap = (o.a - c.a).dot(c.t[k]);
                if gap > PLANE_EPS {
                    *slot = slot.min(gap * 0.5);
                }
            }
        }
        let fill = if inverse { find_fill(&geom, c, neighbors) } else { None };
        let plan = match fill {
            Some(f) => {
                let open = 1 - f.covered;
                let u = c.t[open];
                let v = -c.t[f.covered];
                let half = (u.dot(v).clamp(-1.0, 1.0).acos() * 0.5).tan();
                let limit = clean_decimal(half * reach[open].min(f.reach));
                Plan { corner: *c, kind: BevelKind::Fill, fill: Some(f), u, v, limit }
            }
            None => {
                let half = (c.t[0].dot(c.t[1]).clamp(-1.0, 1.0).acos() * 0.5).tan();
                let kind = if inverse { BevelKind::Cove } else { BevelKind::Round };
                let limit = clean_decimal(half * reach[0].min(reach[1]));
                Plan { corner: *c, kind, fill: None, u: c.t[0], v: c.t[1], limit }
            }
        };
        out.push(plan);
    }
    Ok((geom, out))
}

pub fn brush_edges(sides: &[Side]) -> Vec<[DVec3; 2]> {
    let g = build_solid(sides);
    if g.valid { g.edges() } else { Vec::new() }
}

pub fn bevel_limit(
    sides: &[Side],
    edges: &[[DVec3; 2]],
    neighbors: &[&[Side]],
    inverse: bool,
) -> Result<f64, GeomError> {
    let nb: Vec<SolidGeom> = if inverse { neighbors.iter().map(|s| build_solid(s)).collect() } else { Vec::new() };
    let (_, list) = plans(sides, edges, &nb, inverse)?;
    Ok(list.iter().map(|p| p.limit).fold(MAX_BEVEL_RADIUS, f64::min))
}

pub fn bevel_kinds(sides: &[Side], edges: &[[DVec3; 2]], neighbors: &[&[Side]], inverse: bool) -> Vec<BevelKind> {
    let nb: Vec<SolidGeom> = if inverse { neighbors.iter().map(|s| build_solid(s)).collect() } else { Vec::new() };
    plans(sides, edges, &nb, inverse).map(|(_, l)| l.iter().map(|p| p.kind).collect()).unwrap_or_default()
}

pub fn bevel_opening(sides: &[Side], edge: [DVec3; 2], neighbors: &[&[Side]], inverse: bool) -> Option<DVec3> {
    let nb: Vec<SolidGeom> = if inverse { neighbors.iter().map(|s| build_solid(s)).collect() } else { Vec::new() };
    let (_, list) = plans(sides, &[edge], &nb, inverse).ok()?;
    let p = list.first()?;
    match p.kind {
        BevelKind::Fill => (p.u + p.v).try_normalize(),
        BevelKind::Round | BevelKind::Cove => (-(p.u + p.v)).try_normalize(),
    }
}

fn max_segments(radius: f64, sweep: f64) -> u32 {
    let ratio = (MIN_BEVEL_CHORD / (2.0 * radius)).min(1.0);
    let step = 2.0 * ratio.asin();
    if step <= 0.0 || !step.is_finite() {
        return MAX_BEVEL_SEGMENTS;
    }
    let n = (sweep / step).floor();
    if n.is_finite() { (n as u32).clamp(1, MAX_BEVEL_SEGMENTS) } else { 1 }
}

pub fn bevel_segments(radius: f64, segments: u32, corner_angle: f64) -> u32 {
    segments.clamp(1, MAX_BEVEL_SEGMENTS).min(max_segments(radius, PI - corner_angle))
}

struct Arc {
    center: DVec3,
    points: Vec<DVec3>,
}

fn fillet(apex: DVec3, u: DVec3, v: DVec3, d: DVec3, r: f64, n: u32) -> Arc {
    let phi = u.dot(v).clamp(-1.0, 1.0).acos();
    let s = r / (phi * 0.5).tan();
    let bis = (u + v).normalize_or_zero();
    let center = apex + bis * (r / (phi * 0.5).sin());
    let (p0, p1) = (apex + u * s, apex + v * s);
    let x = p0 - center;
    let y = p1 - center;
    let total = d.dot(x.cross(y)).atan2(x.dot(y));
    let mut points = Vec::with_capacity(n as usize + 1);
    points.push(p0);
    for k in 1..n {
        let q = DQuat::from_axis_angle(d, total * f64::from(k) / f64::from(n));
        points.push(center + q * x);
    }
    points.push(p1);
    Arc { center, points }
}

fn reflect(p: DVec3, origin: DVec3, dir: DVec3) -> DVec3 {
    let foot = origin + dir * (p - origin).dot(dir);
    foot * 2.0 - p
}

fn cove(arc: &Arc) -> Arc {
    let (Some(&first), Some(&last)) = (arc.points.first(), arc.points.last()) else {
        return Arc { center: arc.center, points: Vec::new() };
    };
    let dir = (last - first).normalize_or_zero();
    let n = arc.points.len();
    let points: Vec<DVec3> = arc
        .points
        .iter()
        .enumerate()
        .map(|(k, p)| match k {
            0 => first,
            _ if k + 1 == n => last,
            _ => reflect(*p, first, dir),
        })
        .collect();
    Arc { center: reflect(arc.center, first, dir), points }
}

fn side_plane(p: DVec3, dir: DVec3, d: DVec3, keep: DVec3) -> Option<Plane> {
    let n = d.cross(dir).try_normalize()?;
    let n = if n.dot(keep - p) > 0.0 { -n } else { n };
    Plane::from_normal_point(n, p)
}

fn chord_planes(points: &[DVec3], d: DVec3, keep: &dyn Fn(DVec3) -> DVec3) -> Option<Vec<Plane>> {
    points
        .windows(2)
        .map(|w| {
            let mid = (w[0] + w[1]) * 0.5;
            side_plane(w[0], w[1] - w[0], d, keep(mid))
        })
        .collect()
}

fn oblique(p: DVec3, apex: DVec3, u: DVec3, v: DVec3) -> (f64, f64) {
    let g = u.dot(v);
    let den = (1.0 - g * g).max(1e-12);
    let (x, y) = ((p - apex).dot(u), (p - apex).dot(v));
    ((x - g * y) / den, (y - g * x) / den)
}

fn cove_cells(arc: &Arc, u: DVec3, v: DVec3, d: DVec3) -> Option<Vec<Vec<Plane>>> {
    let pts = &arc.points;
    let n = pts.len().checked_sub(1)?;
    let m = n / 2;
    let (t1, t2) = (pts[0], pts[n]);
    let away = |mid: DVec3| mid + (mid - arc.center);
    let chords = chord_planes(pts, d, &away)?;
    let inside_u = side_plane(t1, v, d, t1 - u)?;
    let inside_v = side_plane(t2, u, d, t2 - v)?;
    let mut cells = Vec::with_capacity(n + 3);
    for k in 0..n {
        let mut planes = Vec::with_capacity(5);
        if k < m {
            if k > 0 {
                planes.push(side_plane(pts[k], u, d, pts[k] + v)?);
            }
            planes.push(side_plane(pts[k + 1], u, d, pts[k + 1] - v)?);
            planes.push(inside_u);
        } else {
            if k + 1 < n {
                planes.push(side_plane(pts[k + 1], v, d, pts[k + 1] + u)?);
            }
            planes.push(side_plane(pts[k], v, d, pts[k] - u)?);
            if m > 0 {
                planes.push(side_plane(pts[m], u, d, pts[m] + v)?);
            }
            planes.push(inside_v);
        }
        planes.push(chords[k]);
        cells.push(planes);
    }
    if m > 0 {
        cells.push(vec![
            side_plane(pts[m], v, d, pts[m] + u)?,
            side_plane(pts[m], u, d, pts[m] + v)?,
            inside_u,
            inside_v,
        ]);
    }
    cells.push(vec![side_plane(t1, v, d, t1 + u)?, inside_v]);
    cells.push(vec![side_plane(t2, u, d, t2 + v)?]);
    Some(cells)
}

fn fill_rings(arc: &Arc, apex: DVec3, u: DVec3, v: DVec3) -> Vec<Vec<DVec3>> {
    let pts = &arc.points;
    let Some(n) = pts.len().checked_sub(1) else { return Vec::new() };
    let m = n / 2;
    let ab: Vec<(f64, f64)> = pts.iter().map(|p| oblique(*p, apex, u, v)).collect();
    let at = |a: f64, b: f64| apex + u * a + v * b;
    let mut rings = Vec::with_capacity(n);
    for k in 0..n {
        let ring = if k < m {
            vec![at(0.0, ab[k].1), pts[k], pts[k + 1], at(0.0, ab[k + 1].1)]
        } else {
            vec![at(ab[k + 1].0, ab[m].1), at(ab[k].0, ab[m].1), pts[k], pts[k + 1]]
        };
        let mut clean: Vec<DVec3> = Vec::with_capacity(4);
        for p in ring {
            if clean.last().is_none_or(|q: &DVec3| q.distance_squared(p) > 1e-8) {
                clean.push(p);
            }
        }
        while clean.len() > 1 && clean[0].distance_squared(clean[clean.len() - 1]) <= 1e-8 {
            clean.pop();
        }
        if clean.len() >= 3 {
            rings.push(clean);
        }
    }
    rings
}

fn ccw(ring: &mut [DVec3], normal: DVec3) {
    let c = ring.iter().copied().sum::<DVec3>() / ring.len().max(1) as f64;
    let mut area = DVec3::ZERO;
    for i in 0..ring.len() {
        area += (ring[i] - c).cross(ring[(i + 1) % ring.len()] - c);
    }
    if area.dot(normal) < 0.0 {
        ring.reverse();
    }
}

fn copy_dressing(side: &mut Side, from: &Side) {
    side.material.clone_from(&from.material);
    side.uaxis = from.uaxis;
    side.vaxis = from.vaxis;
    side.rotation = from.rotation;
    side.lightmap_scale = from.lightmap_scale;
    side.smoothing_groups = from.smoothing_groups;
    side.disp = None;
    side.vertices_plus = None;
}

fn chord_dressing(
    start: (&Side, DVec3),
    points: &[DVec3],
    planes: &[Plane],
    material: Option<&str>,
) -> Vec<(Plane, Side)> {
    let n = planes.len();
    let mut out: Vec<Side> = Vec::with_capacity(n);
    let mut prev = (start.0.clone(), start.1);
    for (k, plane) in planes.iter().enumerate() {
        let mut s = start.0.clone();
        wrap_texture(&prev.0, prev.1, &mut s, plane.normal, points[k]);
        prev = (s.clone(), plane.normal);
        out.push(s);
    }
    for s in &mut out {
        s.disp = None;
        s.vertices_plus = None;
        if n > 1 {
            s.smoothing_groups |= BEVEL_SMOOTHING_GROUP;
        }
        if let Some(m) = material {
            m.clone_into(&mut s.material);
        }
    }
    planes.iter().copied().zip(out).collect()
}

fn dress_new(side: &mut Side, templates: &[(Plane, Side)], references: &[(Plane, Side)], fallback: &[Side]) {
    let Some(p) = Plane::from_side(side) else { return };
    let matching = |list: &[(Plane, Side)]| {
        list.iter().find(|(q, _)| q.approx_eq(&p, NORMAL_MATCH, PLANE_EPS)).map(|(_, s)| s.clone())
    };
    if let Some(t) = matching(templates).or_else(|| matching(references)) {
        copy_dressing(side, &t);
        return;
    }
    let best = fallback
        .iter()
        .filter_map(|s| Plane::from_side(s).map(|q| (q.normal.dot(p.normal), q.normal, s)))
        .max_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, n, s)) = best {
        let material = s.material.clone();
        dress(side, s, n, &material);
    }
}

fn clip_keep(
    piece: &ClipPiece,
    planes: &[Plane],
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Option<ClipPiece>, GeomError> {
    let mut cur = piece.clone();
    for plane in planes {
        let (_, back) = clip_solid_traced(&cur.sides, plane, next_side_id).map_err(|_| BevelIssue::Failed)?;
        match back {
            Some(b) => cur = b.compose(&cur.origins),
            None => return Ok(None),
        }
    }
    Ok(Some(cur))
}

fn solid_ok(sides: &[Side]) -> bool {
    validate_solid(sides).is_ok_and(|g| g.volume() > 1e-3)
}

pub fn bevel_solid(
    sides: &[Side],
    edges: &[[DVec3; 2]],
    neighbors: &[&[Side]],
    params: &BevelParams,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<BevelResult, GeomError> {
    if !params.radius.is_finite() || params.radius < MIN_BEVEL_RADIUS {
        return Err(BevelIssue::NoRadius.into());
    }
    let nb_geom: Vec<SolidGeom> =
        if params.inverse { neighbors.iter().map(|s| build_solid(s)).collect() } else { Vec::new() };
    let (geom, list) = plans(sides, edges, &nb_geom, params.inverse)?;
    let limit = list.iter().map(|p| p.limit).fold(MAX_BEVEL_RADIUS, f64::min);
    if limit < MIN_BEVEL_RADIUS {
        return Err(BevelIssue::NoRoom.into());
    }
    let radius = params.radius.min(limit).min(MAX_BEVEL_RADIUS);
    let segments = list.iter().fold(params.segments.clamp(1, MAX_BEVEL_SEGMENTS), |n, p| {
        n.min(bevel_segments(radius, MAX_BEVEL_SEGMENTS, p.angle()))
    });
    let material = params.material.as_deref().map(str::trim).filter(|m| !m.is_empty());
    let mut templates: Vec<(Plane, Side)> = Vec::new();
    let mut references: Vec<(Plane, Side)> = Vec::new();
    let mut pieces = vec![ClipPiece::whole(sides)];
    let mut added: Vec<Vec<Side>> = Vec::new();
    for plan in &list {
        let c = &plan.corner;
        let arc = fillet(c.a, plan.u, plan.v, c.d, radius, segments);
        match plan.kind {
            BevelKind::Round => {
                let center = arc.center;
                let planes = chord_planes(&arc.points, c.d, &|_| center).ok_or(BevelIssue::Failed)?;
                templates.extend(chord_dressing((&sides[c.sides[0]], c.n[0]), &arc.points, &planes, material));
                let mut next = Vec::with_capacity(pieces.len());
                for p in &pieces {
                    if let Some(q) = clip_keep(p, &planes, next_side_id)? {
                        next.push(q);
                    }
                }
                pieces = next;
            }
            BevelKind::Cove => {
                let arc = cove(&arc);
                let center = arc.center;
                let away = |mid: DVec3| mid + (mid - center);
                let planes = chord_planes(&arc.points, c.d, &away).ok_or(BevelIssue::Failed)?;
                templates.extend(chord_dressing((&sides[c.sides[0]], c.n[0]), &arc.points, &planes, material));
                let cells = cove_cells(&arc, plan.u, plan.v, c.d).ok_or(BevelIssue::Failed)?;
                let mut next = Vec::with_capacity(pieces.len() * cells.len());
                for p in &pieces {
                    for cell in &cells {
                        if let Some(q) = clip_keep(p, cell, next_side_id)? {
                            next.push(q);
                        }
                    }
                }
                pieces = next;
            }
            BevelKind::Fill => {
                let Some(f) = plan.fill else { return Err(BevelIssue::Failed.into()) };
                let Some(other) = neighbors.get(f.neighbor) else { return Err(BevelIssue::Failed.into()) };
                let Some(og) = nb_geom.get(f.neighbor) else { return Err(BevelIssue::Failed.into()) };
                let open = 1 - f.covered;
                let apex = c.a;
                let planes = chord_planes(&arc.points, c.d, &|_| apex).ok_or(BevelIssue::Failed)?;
                templates.extend(chord_dressing((&sides[c.sides[open]], c.n[open]), &arc.points, &planes, material));
                for g in &geom.faces {
                    if let Some(s) = sides.get(g.side_index) {
                        references.push((g.plane, s.clone()));
                    }
                }
                for g in &og.faces {
                    if let Some(s) = other.get(g.side_index) {
                        references.push((g.plane, s.clone()));
                    }
                }
                let end = c.a + c.d * c.len;
                let mut ends: Vec<Plane> = geom
                    .faces
                    .iter()
                    .enumerate()
                    .filter(|(i, g)| {
                        !c.faces.contains(i)
                            && g.plane.normal.dot(c.d).abs() > 1e-3
                            && (on_plane(&g.plane, c.a) || on_plane(&g.plane, end))
                    })
                    .map(|(_, g)| g.plane)
                    .collect();
                ends.extend(og.faces.iter().filter(|g| g.plane.normal.dot(c.d).abs() > 0.5).map(|g| g.plane));
                let margin = radius * 4.0 + 64.0;
                let template = &sides[c.sides[open]];
                let mut made = Vec::new();
                for mut ring in fill_rings(&arc, apex, plan.u, plan.v) {
                    ccw(&mut ring, c.d);
                    let Some(prism_sides) = prism(&ring, c.d, -margin, c.len + margin, template, next_side_id) else {
                        continue;
                    };
                    let whole = ClipPiece { origins: vec![None; prism_sides.len()], sides: prism_sides };
                    if let Some(q) = clip_keep(&whole, &ends, next_side_id)? {
                        made.push(ClipPiece { origins: vec![None; q.sides.len()], sides: q.sides });
                    }
                }
                if made.is_empty() {
                    return Err(BevelIssue::Failed.into());
                }
                added.extend(merge_pieces(made).into_iter().map(|p| p.sides));
            }
        }
    }
    if pieces.is_empty() {
        return Err(BevelIssue::Failed.into());
    }
    let mut pieces = if pieces.len() > 1 { merge_pieces(pieces) } else { pieces };
    for p in &mut pieces {
        for (s, o) in p.sides.iter_mut().zip(&p.origins) {
            if o.is_none() {
                dress_new(s, &templates, &[], sides);
            }
        }
    }
    for a in &mut added {
        for s in a.iter_mut() {
            dress_new(s, &templates, &references, sides);
        }
    }
    let mut seen = std::collections::HashSet::new();
    for s in pieces.iter_mut().flat_map(|p| p.sides.iter_mut()).chain(added.iter_mut().flatten()) {
        if !seen.insert(s.id) {
            s.id = next_side_id();
            seen.insert(s.id);
        }
    }
    if !pieces.iter().all(|p| solid_ok(&p.sides)) || !added.iter().all(|a| solid_ok(a)) {
        return Err(BevelIssue::Failed.into());
    }
    Ok(BevelResult { pieces, added, radius, segments, kinds: list.iter().map(|p| p.kind).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn v3(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x, y, z)
    }

    fn check(pieces: &[Vec<Side>]) -> f64 {
        let mut ids: Vec<u32> = pieces.iter().flatten().map(|s| s.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "side ids are unique");
        let mut total = 0.0;
        for p in pieces {
            let g = validate_solid(p).expect("valid piece");
            assert!(g.volume() > 1.0, "no sliver: {}", g.volume());
            assert_eq!(g.faces.len(), p.len(), "no redundant side");
            let verts: Vec<DVec3> = g.vertices().collect();
            for f in &g.faces {
                assert!(f.area() > 0.01, "no sliver face");
                for v in &verts {
                    assert!(f.plane.distance(*v) <= 1e-6, "convex");
                }
            }
            total += g.volume();
        }
        total
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 + 1e-9 * b.abs()
    }

    fn all(r: &BevelResult) -> Vec<Vec<Side>> {
        r.pieces.iter().map(|p| p.sides.clone()).chain(r.added.iter().cloned()).collect()
    }

    fn block() -> Vec<Side> {
        hammer_box(v3(0.0, 0.0, 0.0), v3(128.0, 64.0, 64.0))
    }

    fn top_front() -> [DVec3; 2] {
        [v3(0.0, 0.0, 64.0), v3(128.0, 0.0, 64.0)]
    }

    fn round_area(r: f64, n: u32) -> f64 {
        let n = f64::from(n);
        r * r - n * 0.5 * r * r * (PI / (2.0 * n)).sin()
    }

    fn cove_area(r: f64, n: u32) -> f64 {
        let n = f64::from(n);
        n * 0.5 * r * r * (PI / (2.0 * n)).sin()
    }

    fn params(radius: f64, segments: u32, inverse: bool) -> BevelParams {
        BevelParams { radius, segments, inverse, material: None }
    }

    #[test]
    fn rounding_a_box_edge() {
        let sides = block();
        let mut next = id_counter(1000);
        let r = bevel_solid(&sides, &[top_front()], &[], &params(16.0, 4, false), &mut next).expect("bevel");
        assert_eq!(r.kinds, vec![BevelKind::Round]);
        assert_eq!((r.radius, r.segments), (16.0, 4));
        assert!(r.changes_brush() && r.added.is_empty());
        assert_eq!(r.pieces.len(), 1, "the outward bevel keeps one convex brush");
        let volume = check(&all(&r));
        let expected = 128.0 * 64.0 * 64.0 - round_area(16.0, 4) * 128.0;
        assert!(close(volume, expected), "{volume} {expected}");
        let p = &r.pieces[0];
        assert_eq!(p.sides.len(), 10, "six faces and four segments");
        let mut new_faces = 0;
        for (s, o) in p.sides.iter().zip(&p.origins) {
            match o {
                Some(o) => {
                    assert_eq!(s.material, sides[*o].material, "kept faces keep their texture");
                    assert_eq!((s.uaxis, s.vaxis), (sides[*o].uaxis, sides[*o].vaxis));
                }
                None => {
                    new_faces += 1;
                    assert_eq!(s.smoothing_groups & BEVEL_SMOOTHING_GROUP, BEVEL_SMOOTHING_GROUP);
                    assert!(s.uaxis.axis.is_normalized() && s.vaxis.axis.is_normalized());
                }
            }
        }
        assert_eq!(new_faces, 4);
        let g0 = build_solid(&p.sides);
        let mut joins = 0;
        for f in &g0.faces {
            for h in &g0.faces {
                let (fo, ho) = (p.origins[f.side_index], p.origins[h.side_index]);
                let curve = |o: Option<usize>| o.is_none_or(|i| i == 0);
                if f.side_index >= h.side_index || (fo.is_some() && ho.is_some()) || !curve(fo) || !curve(ho) {
                    continue;
                }
                let shared: Vec<DVec3> =
                    f.vertices.iter().copied().filter(|v| h.vertices.iter().any(|w| w.distance(*v) < 1e-6)).collect();
                if shared.len() < 2 {
                    continue;
                }
                joins += 1;
                for v in shared {
                    let (a, b) =
                        (crate::texel_uv(&p.sides[f.side_index], v), crate::texel_uv(&p.sides[h.side_index], v));
                    assert!(a.distance(b) < 1e-6, "the texture runs on without a seam: {a} {b}");
                }
            }
        }
        assert_eq!(joins, 4, "the top face and the four rounded faces join without a seam");
        assert!(p.sides.iter().zip(&p.origins).all(|(s, o)| o.is_some() || s.material == sides[0].material));
        let g = build_solid(&p.sides);
        assert!(g.vertices().all(|v| v.z <= 64.0 + 1e-9 && v.y >= -1e-9));
        assert!(!g.vertices().any(|v| v.distance(v3(0.0, 0.0, 64.0)) < 1.0), "the corner is gone");
    }

    #[test]
    fn chamfer_and_segment_counts() {
        let sides = block();
        let mut next = id_counter(1000);
        let r = bevel_solid(&sides, &[top_front()], &[], &params(16.0, 1, false), &mut next).expect("chamfer");
        assert_eq!(r.pieces[0].sides.len(), 7);
        let volume = check(&all(&r));
        assert!(close(volume, 128.0 * 64.0 * 64.0 - 128.0 * 128.0));
        assert!(r.pieces[0].sides.iter().all(|s| s.smoothing_groups == 0), "a flat chamfer is not smoothed");
        for n in [2, 3, 8, 16] {
            let r = bevel_solid(&sides, &[top_front()], &[], &params(32.0, n, false), &mut next).expect("round");
            assert_eq!(r.segments, n);
            assert_eq!(r.pieces[0].sides.len(), 6 + n as usize);
            let volume = check(&all(&r));
            let expected = 128.0 * 64.0 * 64.0 - round_area(32.0, n) * 128.0;
            assert!(close(volume, expected), "{n}: {volume} {expected}");
        }
        let r = bevel_solid(&sides, &[top_front()], &[], &params(1.0, 32, false), &mut next).expect("tiny");
        assert_eq!(r.segments, 3, "segments shorter than half a unit are merged");
        check(&all(&r));
        assert_eq!(bevel_segments(1.0, 32, PI / 2.0), 3);
        assert_eq!(bevel_segments(64.0, 0, PI / 2.0), 1);
        assert_eq!(bevel_segments(64.0, 99, PI / 2.0), MAX_BEVEL_SEGMENTS);
    }

    #[test]
    fn radius_is_clamped_to_the_faces() {
        let sides = block();
        let mut next = id_counter(1000);
        assert_eq!(bevel_limit(&sides, &[top_front()], &[], false).expect("limit"), 64.0);
        let r = bevel_solid(&sides, &[top_front()], &[], &params(1000.0, 8, false), &mut next).expect("big");
        assert_eq!(r.radius, 64.0);
        let volume = check(&all(&r));
        assert!(close(volume, 128.0 * 64.0 * 64.0 - round_area(64.0, 8) * 128.0));
        let back = [v3(0.0, 64.0, 64.0), v3(128.0, 64.0, 64.0)];
        assert_eq!(
            bevel_limit(&sides, &[top_front(), back], &[], false).expect("two"),
            32.0,
            "two edges share the top"
        );
        let r = bevel_solid(&sides, &[top_front(), back], &[], &params(100.0, 6, false), &mut next).expect("two");
        assert_eq!(r.radius, 32.0);
        assert_eq!(r.pieces.len(), 1);
        let volume = check(&all(&r));
        assert!(close(volume, 128.0 * 64.0 * 64.0 - 2.0 * round_area(32.0, 6) * 128.0));
        let ring = [
            top_front(),
            back,
            [v3(0.0, 0.0, 64.0), v3(0.0, 64.0, 64.0)],
            [v3(128.0, 64.0, 64.0), v3(128.0, 0.0, 64.0)],
        ];
        let r = bevel_solid(&sides, &ring, &[], &params(8.0, 4, false), &mut next).expect("four edges");
        assert_eq!(r.pieces.len(), 1);
        check(&all(&r));
    }

    #[test]
    fn inverse_bevel_carves_a_cove() {
        let sides = block();
        let mut next = id_counter(1000);
        for n in [1, 2, 4, 7, 12] {
            let r = bevel_solid(&sides, &[top_front()], &[], &params(24.0, n, true), &mut next).expect("cove");
            assert_eq!(r.kinds, vec![BevelKind::Cove]);
            assert!(r.added.is_empty());
            let pieces = all(&r);
            let volume = check(&pieces);
            let removed = if n == 1 { 24.0 * 24.0 * 0.5 } else { cove_area(24.0, n) };
            let expected = 128.0 * 64.0 * 64.0 - removed * 128.0;
            assert!(close(volume, expected), "{n}: {volume} {expected}");
            assert!(pieces.len() <= n as usize + 3, "{n}: {} pieces", pieces.len());
            for p in &pieces {
                let size = build_solid(p).aabb.size();
                let (thin, wide) = (size.y.min(size.z), size.y.max(size.z));
                assert!(thin >= 4.0 || wide <= 24.0 + 1e-6, "{n}: no long thin piece: {size:?}");
            }
            let hole = v3(64.0, 2.0, 62.0);
            assert!(pieces.iter().all(|p| build_solid(p).faces.iter().any(|f| f.plane.distance(hole) > 0.01)));
            let kept = v3(64.0, 32.0, 32.0);
            assert!(pieces.iter().any(|p| build_solid(p).faces.iter().all(|f| f.plane.distance(kept) < 0.0)));
            for p in &r.pieces {
                for (s, o) in p.sides.iter().zip(&p.origins) {
                    if let Some(o) = o {
                        assert_eq!(s.material, sides[*o].material);
                        assert_eq!((s.uaxis, s.vaxis), (sides[*o].uaxis, sides[*o].vaxis));
                    }
                }
            }
        }
        let active = BevelParams { material: Some("DEV/COVE".to_owned()), ..params(16.0, 4, true) };
        let r = bevel_solid(&sides, &[top_front()], &[], &active, &mut next).expect("cove");
        let cove_faces = r
            .pieces
            .iter()
            .flat_map(|p| p.sides.iter().zip(&p.origins))
            .filter(|(s, o)| o.is_none() && s.material == "DEV/COVE")
            .count();
        assert_eq!(cove_faces, 4, "the curved faces take the active material");
    }

    fn jamb_and_lintel() -> (Vec<Side>, Vec<Side>) {
        let jamb = hammer_box(v3(0.0, 0.0, 0.0), v3(32.0, 16.0, 128.0));
        let lintel = hammer_box(v3(32.0, 0.0, 96.0), v3(160.0, 16.0, 128.0));
        (jamb, lintel)
    }

    #[test]
    fn inverse_bevel_fills_an_inner_corner() {
        let (jamb, lintel) = jamb_and_lintel();
        let mut next = id_counter(1000);
        let edge = [v3(32.0, 0.0, 96.0), v3(32.0, 16.0, 96.0)];
        let neighbors: Vec<&[Side]> = vec![&jamb];
        assert_eq!(bevel_kinds(&lintel, &[edge], &neighbors, true), vec![BevelKind::Fill]);
        assert_eq!(bevel_kinds(&lintel, &[edge], &[], true), vec![BevelKind::Cove]);
        assert_eq!(bevel_kinds(&lintel, &[edge], &neighbors, false), vec![BevelKind::Round]);
        let r = bevel_solid(&lintel, &[edge], &neighbors, &params(32.0, 4, true), &mut next).expect("fill");
        assert!(!r.changes_brush(), "the lintel is not touched");
        assert!(!r.added.is_empty());
        let volume = check(&r.added);
        let expected = round_area(32.0, 4) * 16.0;
        assert!(close(volume, expected), "{volume} {expected}");
        for a in &r.added {
            let g = build_solid(a);
            assert!(g.aabb.min.x >= 32.0 - 1e-9 && g.aabb.max.z <= 96.0 + 1e-9, "in the opening");
            assert!(g.aabb.min.y >= -1e-9 && g.aabb.max.y <= 16.0 + 1e-9, "within the wall");
            for f in &g.faces {
                let s = &a[f.side_index];
                if f.plane.normal.y < -0.999 {
                    assert_eq!(s.material, lintel[3].material, "the wall face texture goes on");
                    assert_eq!((s.uaxis, s.vaxis), (lintel[3].uaxis, lintel[3].vaxis));
                }
            }
        }
        let chords = r
            .added
            .iter()
            .flatten()
            .filter(|s| Plane::from_side(s).is_some_and(|p| p.normal.x.abs() > 0.01 && p.normal.z.abs() > 0.01))
            .collect::<Vec<_>>();
        assert_eq!(chords.len(), 4);
        assert!(chords.iter().all(|s| s.material == lintel[1].material), "the round takes the bevelled face texture");
        let reversed = [v3(32.0, 16.0, 96.0), v3(32.0, 0.0, 96.0)];
        let r2 = bevel_solid(&lintel, &[reversed], &neighbors, &params(32.0, 4, true), &mut next).expect("reversed");
        assert!((check(&r2.added) - expected).abs() < 1e-6);
        let side_edge = [v3(32.0, 0.0, 96.0), v3(32.0, 0.0, 128.0)];
        let r3 = bevel_solid(&lintel, &[side_edge], &neighbors, &params(8.0, 2, true), &mut next).expect("side");
        assert_eq!(r3.kinds, vec![BevelKind::Cove]);
        let jamb_edge = [v3(32.0, 0.0, 128.0), v3(32.0, 16.0, 128.0)];
        let lintel_nb: Vec<&[Side]> = vec![&lintel];
        let r4 = bevel_solid(&jamb, &[jamb_edge], &lintel_nb, &params(16.0, 4, true), &mut next).expect("jamb");
        assert_eq!(r4.kinds, vec![BevelKind::Cove], "the lintel does not reach past the jamb top");
    }

    #[test]
    fn fill_from_the_other_brush() {
        let jamb = hammer_box(v3(0.0, 0.0, 0.0), v3(32.0, 16.0, 96.0));
        let lintel = hammer_box(v3(0.0, 0.0, 96.0), v3(160.0, 16.0, 128.0));
        let edge = [v3(32.0, 0.0, 96.0), v3(32.0, 16.0, 96.0)];
        let neighbors: Vec<&[Side]> = vec![&lintel];
        let mut next = id_counter(1000);
        let r = bevel_solid(&jamb, &[edge], &neighbors, &params(16.0, 6, true), &mut next).expect("fill");
        assert_eq!(r.kinds, vec![BevelKind::Fill]);
        let volume = check(&r.added);
        assert!(close(volume, round_area(16.0, 6) * 16.0), "{volume}");
        let shoulder = v3(33.0, 8.0, 95.0);
        assert!(r.added.iter().any(|a| build_solid(a).faces.iter().all(|f| f.plane.distance(shoulder) < 0.0)));
    }

    #[test]
    fn arch_from_a_doorway() {
        let left = hammer_box(v3(0.0, 0.0, 0.0), v3(64.0, 16.0, 192.0));
        let right = hammer_box(v3(192.0, 0.0, 0.0), v3(256.0, 16.0, 192.0));
        let top = hammer_box(v3(64.0, 0.0, 128.0), v3(192.0, 16.0, 192.0));
        let edges = [[v3(64.0, 0.0, 128.0), v3(64.0, 16.0, 128.0)], [v3(192.0, 16.0, 128.0), v3(192.0, 0.0, 128.0)]];
        let neighbors: Vec<&[Side]> = vec![&left, &right];
        assert_eq!(bevel_limit(&top, &edges, &neighbors, true).expect("limit"), 64.0);
        let mut next = id_counter(1000);
        let r = bevel_solid(&top, &edges, &neighbors, &params(500.0, 8, true), &mut next).expect("arch");
        assert_eq!(r.kinds, vec![BevelKind::Fill, BevelKind::Fill]);
        assert_eq!(r.radius, 64.0, "half the opening");
        let volume = check(&r.added);
        assert!(close(volume, 2.0 * round_area(64.0, 8) * 16.0));
        let apex = v3(128.0, 8.0, 127.0);
        let open = r.added.iter().all(|a| build_solid(a).faces.iter().any(|f| f.plane.distance(apex) > 0.01));
        assert!(open, "the arch top stays open");
        let shoulder = v3(66.0, 8.0, 126.0);
        let filled = r.added.iter().any(|a| build_solid(a).faces.iter().all(|f| f.plane.distance(shoulder) < 0.0));
        assert!(filled, "the corners are filled");
    }

    #[test]
    fn slanted_corners() {
        let p = crate::PrimitiveParams {
            kind: crate::Primitive::Wedge,
            bounds: crate::Aabb::new(v3(0.0, 0.0, 0.0), v3(256.0, 128.0, 128.0)),
            ..Default::default()
        };
        let mut next = id_counter(1000);
        let wedge = crate::make_primitive(&p, &mut next).expect("wedge").remove(0);
        let edges = brush_edges(&wedge);
        assert_eq!(edges.len(), 9);
        let before = build_solid(&wedge).volume();
        let mut done = 0;
        for e in &edges {
            for inverse in [false, true] {
                let r = bevel_solid(&wedge, &[*e], &[], &params(12.0, 5, inverse), &mut next).expect("bevel");
                let after = check(&all(&r));
                assert!(after < before - 1.0 && after > before * 0.5, "{e:?} {inverse}: {after} {before}");
                done += 1;
            }
        }
        assert_eq!(done, 18, "every edge of a wedge can be bevelled both ways");
    }

    #[test]
    fn rotated_and_random_brushes() {
        let mut rng = Rng(0x5eed_beef_1234_5678);
        let mut next = id_counter(1000);
        let mut made = 0;
        for i in 0..60u32 {
            let size = v3(rng.range(24.0, 256.0), rng.range(24.0, 256.0), rng.range(24.0, 256.0));
            let mut sides = hammer_box(DVec3::ZERO, size);
            let rot =
                DQuat::from_euler(glam::EulerRot::XYZ, rng.range(-PI, PI), rng.range(-PI, PI), rng.range(-PI, PI));
            let m = glam::DMat4::from_rotation_translation(rot, rng.point(1000.0));
            crate::transform_sides(&mut sides, m, true, false, None).expect("transform");
            let before = build_solid(&sides).volume();
            let edges = brush_edges(&sides);
            let picked: Vec<[DVec3; 2]> =
                (0..1 + i as usize % 3).map(|k| edges[(i as usize * 5 + k * 7) % edges.len()]).collect();
            for inverse in [false, true] {
                let p = params(rng.range(1.0, 80.0), 1 + i % 12, inverse);
                match bevel_solid(&sides, &picked, &[], &p, &mut next) {
                    Ok(r) => {
                        let pieces = all(&r);
                        for piece in &pieces {
                            let g = validate_solid(piece).expect("valid piece");
                            let verts: Vec<DVec3> = g.vertices().collect();
                            assert!(g.faces.iter().all(|f| verts.iter().all(|v| f.plane.distance(*v) <= 1e-6)));
                        }
                        let after: f64 = pieces.iter().map(|q| build_solid(q).volume()).sum();
                        assert!(after < before && after > before * 0.4, "{i} {inverse}: {after} {before}");
                        made += 1;
                    }
                    Err(e) => assert_ne!(BevelIssue::of(&e), Some(BevelIssue::Failed), "{i} {inverse}: {e}"),
                }
            }
        }
        assert!(made > 90, "{made}");
    }

    #[test]
    fn invalid_requests() {
        let sides = block();
        let mut next = id_counter(1000);
        let p = params(16.0, 4, false);
        let diagonal = [v3(0.0, 0.0, 64.0), v3(128.0, 64.0, 64.0)];
        let e = bevel_solid(&sides, &[diagonal], &[], &p, &mut next).expect_err("diagonal");
        assert_eq!(BevelIssue::of(&e), Some(BevelIssue::NotAnEdge));
        let e = bevel_solid(&sides, &[], &[], &p, &mut next).expect_err("nothing");
        assert_eq!(BevelIssue::of(&e), Some(BevelIssue::NoEdge));
        let e = bevel_solid(&sides, &[top_front()], &[], &params(0.0, 4, false), &mut next).expect_err("zero");
        assert_eq!(BevelIssue::of(&e), Some(BevelIssue::NoRadius));
        let e = bevel_solid(&sides, &[top_front()], &[], &params(f64::NAN, 4, false), &mut next).expect_err("nan");
        assert_eq!(BevelIssue::of(&e), Some(BevelIssue::NoRadius));
        let mut disp = sides.clone();
        let face = build_solid(&sides).face_for_side(2).expect("face").vertices.clone();
        disp[2].disp = Some(Box::new(crate::create_disp(&face, 2).expect("disp")));
        let e = bevel_solid(&disp, &[top_front()], &[], &p, &mut next).expect_err("disp");
        assert_eq!(BevelIssue::of(&e), Some(BevelIssue::Displacement));
        let near = [v3(0.01, 0.02, 63.99), v3(128.0, 0.0, 64.0)];
        assert!(bevel_solid(&sides, &[near], &[], &p, &mut next).is_ok(), "endpoints are matched with a tolerance");
        let thin = hammer_box(v3(0.0, 0.0, 0.0), v3(128.0, 0.2, 64.0));
        let edge = [v3(0.0, 0.0, 64.0), v3(128.0, 0.0, 64.0)];
        let e = bevel_solid(&thin, &[edge], &[], &p, &mut next).expect_err("thin");
        assert_eq!(BevelIssue::of(&e), Some(BevelIssue::NoRoom));
        for issue in BevelIssue::ALL {
            assert_eq!(BevelIssue::of(&issue.into()), Some(issue));
        }
        assert_eq!(BevelIssue::of(&GeomError::NoVolume), None);
        assert_eq!(BevelKind::Fill.id(), "fill");
        assert_eq!(brush_edges(&sides).len(), 12);
        assert!(brush_edges(&sides[..3]).is_empty());
    }
}
