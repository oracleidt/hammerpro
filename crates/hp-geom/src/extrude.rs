use glam::{DVec2, DVec3};
use hp_vmf::{Side, TexAxis};

use crate::brush::validate_solid;
use crate::csg::{ClipPiece, carve_traced};
use crate::texture::{effective_scale, world_axes};
use crate::util::clean_vec;
use crate::{GeomError, MAX_COORD, PLANE_EPS, Plane, SolidGeom, build_solid};

pub const MAX_OUTLINE_POINTS: usize = 128;

pub const MIN_EXTRUDE_DEPTH: f64 = 0.125;

const CARVE_MARGIN: f64 = 1.0;

const MIN_OUTLINE_AREA: f64 = 0.01;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExtrudeMode {
    #[default]
    Carve,
    Extrude,
}

impl ExtrudeMode {
    pub fn id(self) -> &'static str {
        match self {
            ExtrudeMode::Carve => "carve",
            ExtrudeMode::Extrude => "extrude",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "carve" => Some(ExtrudeMode::Carve),
            "extrude" => Some(ExtrudeMode::Extrude),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtrudeIssue {
    TooManyPoints,
    TooFewPoints,
    Crossing,
    NotAFace,
    Displacement,
    OutsideFace,
    NoDepth,
    TooThin,
    PastLimits,
    NoCut,
    ThroughBrush,
}

impl ExtrudeIssue {
    pub const ALL: [ExtrudeIssue; 11] = [
        ExtrudeIssue::TooManyPoints,
        ExtrudeIssue::TooFewPoints,
        ExtrudeIssue::Crossing,
        ExtrudeIssue::NotAFace,
        ExtrudeIssue::Displacement,
        ExtrudeIssue::OutsideFace,
        ExtrudeIssue::NoDepth,
        ExtrudeIssue::TooThin,
        ExtrudeIssue::PastLimits,
        ExtrudeIssue::NoCut,
        ExtrudeIssue::ThroughBrush,
    ];

    pub fn message(self) -> &'static str {
        match self {
            ExtrudeIssue::TooManyPoints => "the outline has more than 128 points",
            ExtrudeIssue::TooFewPoints => "the outline needs at least 3 distinct points",
            ExtrudeIssue::Crossing => "the outline crosses itself or has no area",
            ExtrudeIssue::NotAFace => "the side is not a face of the brush",
            ExtrudeIssue::Displacement => "the face is a displacement",
            ExtrudeIssue::OutsideFace => "the outline is outside the face",
            ExtrudeIssue::NoDepth => "the depth must be at least 0.125 unit",
            ExtrudeIssue::TooThin => "a piece of the outline is too thin",
            ExtrudeIssue::PastLimits => "the result goes past the map limits",
            ExtrudeIssue::NoCut => "the outline does not cut into the brush",
            ExtrudeIssue::ThroughBrush => "the carve goes through the whole brush",
        }
    }

    pub fn of(error: &GeomError) -> Option<Self> {
        match error {
            GeomError::InvalidInput(m) => Self::ALL.into_iter().find(|i| i.message() == m),
            _ => None,
        }
    }
}

impl From<ExtrudeIssue> for GeomError {
    fn from(issue: ExtrudeIssue) -> Self {
        GeomError::InvalidInput(issue.message().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceExtrudeParams {
    pub mode: ExtrudeMode,
    pub depth: f64,
    pub through: bool,
    pub material: Option<String>,
}

impl Default for FaceExtrudeParams {
    fn default() -> Self {
        Self { mode: ExtrudeMode::Carve, depth: 16.0, through: false, material: None }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceExtrudeResult {
    pub pieces: Vec<ClipPiece>,
    pub added: Vec<Vec<Side>>,
    pub prism: Vec<Vec<Side>>,
    pub merged: bool,
}

impl FaceExtrudeResult {
    pub fn changes_brush(&self) -> bool {
        self.merged || self.pieces.len() != 1 || self.pieces.iter().any(|p| p.origins.iter().any(Option::is_none))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceFrame {
    pub origin: DVec3,
    pub u: DVec3,
    pub v: DVec3,
    pub normal: DVec3,
}

impl FaceFrame {
    pub fn new(plane: &Plane) -> Self {
        let n = plane.normal;
        let a = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        let u = (a - n * n.dot(a)).try_normalize().unwrap_or(DVec3::X);
        let v = n.cross(u);
        Self { origin: n * plane.dist, u, v, normal: n }
    }

    pub fn to_2d(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.u), d.dot(self.v))
    }

    pub fn to_3d(&self, p: DVec2, height: f64) -> DVec3 {
        self.origin + self.u * p.x + self.v * p.y + self.normal * height
    }

    pub fn plane(&self) -> Plane {
        Plane { normal: self.normal, dist: self.normal.dot(self.origin) }
    }
}

fn cross(o: DVec2, a: DVec2, b: DVec2) -> f64 {
    (a - o).perp_dot(b - o)
}

fn scale_of(points: &[DVec2]) -> f64 {
    points.iter().fold(1.0f64, |m, p| m.max(p.abs().max_element()))
}

pub fn signed_area(points: &[DVec2]) -> f64 {
    let n = points.len();
    (0..n).map(|i| points[i].perp_dot(points[(i + 1) % n])).sum::<f64>() * 0.5
}

fn collinear(a: DVec2, b: DVec2, c: DVec2) -> bool {
    let e1 = b - a;
    let e2 = c - b;
    e1.perp_dot(e2).abs() <= 1e-9 * (e1.length() * e2.length()).max(1e-12)
}

pub(crate) fn clean_outline(points: &[DVec2]) -> Vec<usize> {
    let mut idx: Vec<usize> = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        if !p.is_finite() {
            continue;
        }
        if idx.last().is_some_and(|&j| points[j].distance_squared(*p) < 1e-12) {
            continue;
        }
        idx.push(i);
    }
    while idx.len() > 1 && points[idx[0]].distance_squared(points[idx[idx.len() - 1]]) < 1e-12 {
        idx.pop();
    }
    let mut changed = true;
    while changed && idx.len() >= 3 {
        changed = false;
        let n = idx.len();
        for k in 0..n {
            let a = points[idx[(k + n - 1) % n]];
            let b = points[idx[k]];
            let c = points[idx[(k + 1) % n]];
            if collinear(a, b, c) || a.distance_squared(b) < 1e-12 {
                idx.remove(k);
                changed = true;
                break;
            }
        }
    }
    idx
}

fn segments_touch(a: DVec2, b: DVec2, c: DVec2, d: DVec2, eps: f64) -> bool {
    let d1 = cross(c, d, a);
    let d2 = cross(c, d, b);
    let d3 = cross(a, b, c);
    let d4 = cross(a, b, d);
    if ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps)) && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps)) {
        return true;
    }
    let on = |p: DVec2, q: DVec2, r: DVec2, o: f64| {
        o.abs() <= eps
            && r.x >= p.x.min(q.x) - 1e-9
            && r.x <= p.x.max(q.x) + 1e-9
            && r.y >= p.y.min(q.y) - 1e-9
            && r.y <= p.y.max(q.y) + 1e-9
    };
    on(c, d, a, d1) || on(c, d, b, d2) || on(a, b, c, d3) || on(a, b, d, d4)
}

pub fn outline_is_simple(points: &[DVec2]) -> bool {
    let n = points.len();
    if n < 3 || points.iter().any(|p| !p.is_finite()) {
        return false;
    }
    let s = scale_of(points);
    let eps = 1e-9 * s * s;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        if a.distance_squared(b) < 1e-12 {
            return false;
        }
        for j in (i + 1)..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (c, d) = (points[j], points[(j + 1) % n]);
            if segments_touch(a, b, c, d, eps) {
                return false;
            }
        }
    }
    for i in 0..n {
        let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
        let (e1, e2) = (b - a, c - b);
        if e1.perp_dot(e2).abs() <= 1e-9 * e1.length() * e2.length() && e1.dot(e2) < 0.0 {
            return false;
        }
    }
    signed_area(points).abs() > MIN_OUTLINE_AREA
}

pub fn path_is_simple(points: &[DVec2]) -> bool {
    let n = points.len();
    if points.iter().any(|p| !p.is_finite()) {
        return false;
    }
    if n < 3 {
        return n < 2 || points[0].distance_squared(points[1]) > 1e-12;
    }
    let s = scale_of(points);
    let eps = 1e-9 * s * s;
    for i in 0..n - 1 {
        let (a, b) = (points[i], points[i + 1]);
        if a.distance_squared(b) < 1e-12 {
            return false;
        }
        for j in (i + 2)..n - 1 {
            if segments_touch(a, b, points[j], points[j + 1], eps) {
                return false;
            }
        }
        if i > 0 {
            let (e1, e2) = (b - a, a - points[i - 1]);
            if e1.perp_dot(e2).abs() <= 1e-9 * e1.length() * e2.length() && e1.dot(e2) < 0.0 {
                return false;
            }
        }
    }
    true
}

fn in_triangle(p: DVec2, a: DVec2, b: DVec2, c: DVec2, eps: f64) -> bool {
    cross(a, b, p) >= -eps && cross(b, c, p) >= -eps && cross(c, a, p) >= -eps
}

fn min_angle(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    let ang = |o: DVec2, p: DVec2, q: DVec2| (p - o).angle_to(q - o).abs();
    ang(a, b, c).min(ang(b, c, a)).min(ang(c, a, b))
}

fn triangulate(points: &[DVec2]) -> Option<Vec<[usize; 3]>> {
    let s = scale_of(points);
    let eps = 1e-9 * s * s;
    let mut idx: Vec<usize> = (0..points.len()).collect();
    let mut tris = Vec::with_capacity(points.len());
    while idx.len() > 3 {
        let m = idx.len();
        let mut best: Option<(f64, usize)> = None;
        for k in 0..m {
            let (i, j, l) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (points[i], points[j], points[l]);
            if cross(a, b, c) <= eps {
                continue;
            }
            let blocked = idx.iter().any(|&q| {
                q != i && q != j && q != l && {
                    let p = points[q];
                    p.distance_squared(a) > 1e-12
                        && p.distance_squared(b) > 1e-12
                        && p.distance_squared(c) > 1e-12
                        && in_triangle(p, a, b, c, eps)
                }
            });
            if blocked {
                continue;
            }
            let q = min_angle(a, b, c);
            if best.is_none_or(|(bq, _)| q > bq + 1e-12) {
                best = Some((q, k));
            }
        }
        match best {
            Some((_, k)) => {
                let (i, j, l) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
                tris.push([i, j, l]);
                idx.remove(k);
            }
            None => {
                let k = (0..m).find(|&k| {
                    let (i, j, l) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
                    cross(points[i], points[j], points[l]).abs() <= eps
                })?;
                idx.remove(k);
            }
        }
    }
    if let [i, j, l] = idx[..]
        && cross(points[i], points[j], points[l]) > eps
    {
        tris.push([i, j, l]);
    }
    Some(tris)
}

fn piece_convex(points: &[DVec2], piece: &[usize], eps: f64) -> bool {
    let n = piece.len();
    n >= 3
        && (0..n).all(|k| cross(points[piece[(k + n - 1) % n]], points[piece[k]], points[piece[(k + 1) % n]]) >= -eps)
}

fn rotate_to(piece: &[usize], first: usize) -> Option<Vec<usize>> {
    let p = piece.iter().position(|&x| x == first)?;
    Some(piece[p..].iter().chain(&piece[..p]).copied().collect())
}

fn try_merge(points: &[DVec2], a: &[usize], b: &[usize], eps: f64) -> Option<(f64, Vec<usize>)> {
    let n = a.len();
    for k in 0..n {
        let (x, y) = (a[k], a[(k + 1) % n]);
        let m = b.len();
        if !(0..m).any(|j| b[j] == y && b[(j + 1) % m] == x) {
            continue;
        }
        let ra = rotate_to(a, y)?;
        let rb = rotate_to(b, x)?;
        if ra.last() != Some(&x) || rb.last() != Some(&y) {
            continue;
        }
        let mut merged = ra;
        merged.extend(rb.iter().skip(1).take(rb.len().saturating_sub(2)));
        if piece_convex(points, &merged, eps) {
            return Some((points[x].distance(points[y]), merged));
        }
    }
    None
}

fn drop_straight(points: &[DVec2], piece: Vec<usize>) -> Vec<usize> {
    let mut p = piece;
    let mut changed = true;
    while changed && p.len() > 3 {
        changed = false;
        let n = p.len();
        for k in 0..n {
            if collinear(points[p[(k + n - 1) % n]], points[p[k]], points[p[(k + 1) % n]]) {
                p.remove(k);
                changed = true;
                break;
            }
        }
    }
    p
}

pub fn convex_decompose(points: &[DVec2]) -> Option<Vec<Vec<usize>>> {
    if !outline_is_simple(points) || signed_area(points) <= 0.0 {
        return None;
    }
    let s = scale_of(points);
    let eps = 1e-9 * s * s;
    let all: Vec<usize> = (0..points.len()).collect();
    if piece_convex(points, &all, eps) {
        return Some(vec![drop_straight(points, all)]);
    }
    let mut pieces: Vec<Vec<usize>> = triangulate(points)?.into_iter().map(|t| t.to_vec()).collect();
    loop {
        let mut best: Option<(f64, usize, usize, Vec<usize>)> = None;
        for i in 0..pieces.len() {
            for j in (i + 1)..pieces.len() {
                if let Some((len, merged)) = try_merge(points, &pieces[i], &pieces[j], eps)
                    && best.as_ref().is_none_or(|b| len > b.0 + 1e-9)
                {
                    best = Some((len, i, j, merged));
                }
            }
        }
        let Some((_, i, j, merged)) = best else { break };
        pieces[i] = merged;
        pieces.remove(j);
    }
    Some(pieces.into_iter().map(|p| drop_straight(points, p)).collect())
}

pub(crate) fn oriented(points: [DVec3; 3], normal: DVec3) -> [DVec3; 3] {
    match Plane::from_points(points[0], points[1], points[2]) {
        Some(p) if p.normal.dot(normal) < 0.0 => [points[2], points[1], points[0]],
        _ => points,
    }
}

fn cap_points(ring: &[DVec3], normal: DVec3) -> Option<[DVec3; 3]> {
    let a = *ring.first()?;
    let mut best: Option<(f64, usize, usize)> = None;
    for i in 1..ring.len() {
        for j in (i + 1)..ring.len() {
            let area = (ring[i] - a).cross(ring[j] - a).length_squared();
            if best.is_none_or(|b| area > b.0) {
                best = Some((area, i, j));
            }
        }
    }
    let (area, i, j) = best?;
    (area > 1e-12).then(|| oriented([a, ring[i], ring[j]], normal))
}

pub(crate) fn new_side(id: u32, plane: [DVec3; 3], template: &Side) -> Side {
    Side {
        id,
        plane: plane.map(clean_vec),
        material: template.material.clone(),
        uaxis: template.uaxis,
        vaxis: template.vaxis,
        rotation: template.rotation,
        lightmap_scale: template.lightmap_scale,
        smoothing_groups: template.smoothing_groups,
        disp: None,
        vertices_plus: None,
        extra_props: Vec::new(),
        extra_blocks: Vec::new(),
    }
}

pub(crate) fn dress(side: &mut Side, face: &Side, face_normal: DVec3, material: &str) {
    let Some(p) = Plane::from_side(side) else { return };
    material.clone_into(&mut side.material);
    side.lightmap_scale = face.lightmap_scale;
    side.smoothing_groups = face.smoothing_groups;
    side.disp = None;
    side.vertices_plus = None;
    if p.normal.dot(face_normal).abs() > 1.0 - 1e-9 {
        side.uaxis = face.uaxis;
        side.vaxis = face.vaxis;
        side.rotation = face.rotation;
    } else {
        let (u, v) = world_axes(p.normal);
        side.uaxis = TexAxis { axis: u, shift: 0.0, scale: effective_scale(face.uaxis.scale) };
        side.vaxis = TexAxis { axis: v, shift: 0.0, scale: effective_scale(face.vaxis.scale) };
        side.rotation = 0.0;
    }
}

pub(crate) fn prism(
    ring: &[DVec3],
    normal: DVec3,
    lo: f64,
    hi: f64,
    template: &Side,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Option<Vec<Side>> {
    let n = ring.len();
    let at = |p: DVec3, h: f64| clean_vec(p + normal * h);
    let mut planes: Vec<([DVec3; 3], Plane)> = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let out = (b - a).cross(normal);
        let pts = oriented([at(a, hi), at(b, hi), at(b, lo)], out);
        let plane = Plane::from_points(pts[0], pts[1], pts[2])?;
        if planes.iter().any(|(_, q)| q.approx_eq(&plane, 1e-9, 1e-6)) {
            continue;
        }
        planes.push((pts, plane));
    }
    let low: Vec<DVec3> = ring.iter().map(|p| at(*p, lo)).collect();
    let high: Vec<DVec3> = ring.iter().map(|p| at(*p, hi)).collect();
    let bottom = cap_points(&low, -normal)?;
    let top = cap_points(&high, normal)?;
    let mut sides: Vec<Side> = planes.into_iter().map(|(pts, _)| new_side(next_side_id(), pts, template)).collect();
    sides.push(new_side(next_side_id(), bottom, template));
    sides.push(new_side(next_side_id(), top, template));
    build_solid(&sides).valid.then_some(sides)
}

fn shares_opposite(a: &[Option<Plane>], b: &[Option<Plane>]) -> bool {
    a.iter().flatten().any(|p| b.iter().flatten().any(|q| p.approx_eq(&q.flip(), 1e-6, PLANE_EPS)))
}

fn merge_pair(a: &ClipPiece, ga: &SolidGeom, b: &ClipPiece, gb: &SolidGeom) -> Option<(ClipPiece, SolidGeom)> {
    if !ga.valid || !gb.valid || ga.aabb.expand(PLANE_EPS).intersection(&gb.aabb.expand(PLANE_EPS)).is_empty() {
        return None;
    }
    let live = |piece: &ClipPiece, g: &SolidGeom| -> Vec<(Side, Option<usize>, Plane)> {
        g.faces
            .iter()
            .filter_map(|f| {
                let s = piece.sides.get(f.side_index)?;
                Some((s.clone(), piece.origins.get(f.side_index).copied().flatten(), f.plane))
            })
            .collect()
    };
    let la = live(a, ga);
    let lb = live(b, gb);
    let pa: Vec<Option<Plane>> = la.iter().map(|x| Some(x.2)).collect();
    let pb: Vec<Option<Plane>> = lb.iter().map(|x| Some(x.2)).collect();
    if !shares_opposite(&pa, &pb) {
        return None;
    }
    let mut out: Vec<(Side, Option<usize>, Plane)> = Vec::with_capacity(la.len() + lb.len());
    for (list, other) in [(&la, &pb), (&lb, &pa)] {
        for (s, o, p) in list {
            if other.iter().flatten().any(|q| p.approx_eq(&q.flip(), 1e-6, PLANE_EPS)) {
                continue;
            }
            match out.iter_mut().find(|x| x.2.approx_eq(p, 1e-6, PLANE_EPS)) {
                Some(slot) => {
                    if slot.1.is_none() && o.is_some() {
                        *slot = (s.clone(), *o, *p);
                    }
                }
                None => out.push((s.clone(), *o, *p)),
            }
        }
    }
    let sides: Vec<Side> = out.iter().map(|x| x.0.clone()).collect();
    let q = build_solid(&sides);
    if !q.valid {
        return None;
    }
    let inside = ga.vertices().chain(gb.vertices()).all(|v| q.faces.iter().all(|f| f.plane.distance(v) <= PLANE_EPS));
    let (va, vb, vq) = (ga.volume(), gb.volume(), q.volume());
    let tolerance = PLANE_EPS * q.surface_area() + 1e-9 * vq;
    if !inside || (vq - va - vb).abs() > tolerance {
        return None;
    }
    let keep: Vec<usize> = q.faces.iter().map(|f| f.side_index).collect();
    let piece = ClipPiece {
        sides: keep.iter().filter_map(|&i| out.get(i).map(|x| x.0.clone())).collect(),
        origins: keep.iter().filter_map(|&i| out.get(i).map(|x| x.1)).collect(),
    };
    let geom = build_solid(&piece.sides);
    geom.valid.then_some((piece, geom))
}

pub(crate) fn merge_pieces(pieces: Vec<ClipPiece>) -> Vec<ClipPiece> {
    let mut list: Vec<(ClipPiece, SolidGeom)> = pieces
        .into_iter()
        .map(|p| {
            let g = build_solid(&p.sides);
            (p, g)
        })
        .collect();
    loop {
        let mut found = None;
        'search: for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                if let Some(m) = merge_pair(&list[i].0, &list[i].1, &list[j].0, &list[j].1) {
                    found = Some((i, j, m));
                    break 'search;
                }
            }
        }
        let Some((i, j, m)) = found else { break };
        list[i] = m;
        list.remove(j);
    }
    list.into_iter().map(|(p, _)| p).collect()
}

pub fn face_thickness(sides: &[Side], side_index: usize) -> Option<f64> {
    let geom = build_solid(sides);
    let face = geom.face_for_side(side_index)?;
    let t = geom.vertices().map(|v| -face.plane.distance(v)).fold(0.0f64, f64::max);
    (geom.valid && t.is_finite()).then_some(t)
}

pub(crate) struct FaceOutline {
    pub frame: FaceFrame,
    pub ring: Vec<DVec3>,
    pub flat: Vec<DVec2>,
    pub pieces: Vec<Vec<usize>>,
}

pub(crate) fn face_outline(plane: &Plane, outline: &[DVec3]) -> Result<FaceOutline, GeomError> {
    if outline.len() > MAX_OUTLINE_POINTS {
        return Err(ExtrudeIssue::TooManyPoints.into());
    }
    if outline.iter().any(|p| !p.is_finite() || p.abs().max_element() > MAX_COORD) {
        return Err(GeomError::NotFinite);
    }
    let frame = FaceFrame::new(plane);
    let projected: Vec<DVec3> = outline.iter().map(|p| clean_vec(plane.project(*p))).collect();
    let flat_all: Vec<DVec2> = projected.iter().map(|p| frame.to_2d(*p)).collect();
    let mut keep = clean_outline(&flat_all);
    if keep.len() < 3 {
        return Err(ExtrudeIssue::TooFewPoints.into());
    }
    let mut flat: Vec<DVec2> = keep.iter().map(|&i| flat_all[i]).collect();
    if !outline_is_simple(&flat) {
        return Err(ExtrudeIssue::Crossing.into());
    }
    if signed_area(&flat) < 0.0 {
        keep.reverse();
        flat.reverse();
    }
    let ring: Vec<DVec3> = keep.iter().map(|&i| projected[i]).collect();
    let pieces = convex_decompose(&flat).ok_or(ExtrudeIssue::Crossing)?;
    Ok(FaceOutline { frame, ring, flat, pieces })
}

fn clip_convex(subject: &[DVec2], clip: &[DVec2]) -> Vec<DVec2> {
    let mut out = subject.to_vec();
    let n = clip.len();
    for i in 0..n {
        let (a, b) = (clip[i], clip[(i + 1) % n]);
        let input = std::mem::take(&mut out);
        let m = input.len();
        for k in 0..m {
            let (p, q) = (input[k], input[(k + 1) % m]);
            let (dp, dq) = (cross(a, b, p), cross(a, b, q));
            if dp >= 0.0 {
                out.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                let t = dp / (dp - dq);
                out.push(p + (q - p) * t);
            }
        }
        if out.len() < 3 {
            return Vec::new();
        }
    }
    out
}

pub(crate) fn outline_overlap(face: &[DVec2], outline: &FaceOutline) -> f64 {
    let mut face = face.to_vec();
    if signed_area(&face) < 0.0 {
        face.reverse();
    }
    outline
        .pieces
        .iter()
        .map(|p| {
            let poly: Vec<DVec2> = p.iter().map(|&i| outline.flat[i]).collect();
            signed_area(&clip_convex(&poly, &face)).abs()
        })
        .sum()
}

pub fn face_extrude(
    sides: &[Side],
    side_index: usize,
    outline: &[DVec3],
    params: &FaceExtrudeParams,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<FaceExtrudeResult, GeomError> {
    let geom = validate_solid(sides)?;
    let face = geom.face_for_side(side_index).ok_or(ExtrudeIssue::NotAFace)?;
    let source = sides.get(side_index).ok_or(ExtrudeIssue::NotAFace)?;
    if source.disp.is_some() {
        return Err(ExtrudeIssue::Displacement.into());
    }
    let plane = face.plane;
    let shape = face_outline(&plane, outline)?;
    let face_flat: Vec<DVec2> = face.vertices.iter().map(|v| shape.frame.to_2d(*v)).collect();
    if outline_overlap(&face_flat, &shape) <= MIN_OUTLINE_AREA {
        return Err(ExtrudeIssue::OutsideFace.into());
    }
    let thickness = geom.vertices().map(|v| -plane.distance(v)).fold(0.0f64, f64::max);
    let through = params.through && params.mode == ExtrudeMode::Carve;
    let depth_ok = params.depth.is_finite() && params.depth >= MIN_EXTRUDE_DEPTH;
    if !through && !depth_ok {
        return Err(ExtrudeIssue::NoDepth.into());
    }
    let (lo, hi) = match params.mode {
        ExtrudeMode::Carve if through => (-(thickness + CARVE_MARGIN), CARVE_MARGIN),
        ExtrudeMode::Carve => (-params.depth.min(thickness + CARVE_MARGIN), CARVE_MARGIN),
        ExtrudeMode::Extrude => (0.0, params.depth),
    };
    let material = params
        .material
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map_or_else(|| source.material.clone(), str::to_owned);
    let mut prisms = Vec::with_capacity(shape.pieces.len());
    for piece in &shape.pieces {
        let ring: Vec<DVec3> = piece.iter().map(|&i| shape.ring[i]).collect();
        let mut sides = prism(&ring, plane.normal, lo, hi, source, next_side_id).ok_or(ExtrudeIssue::TooThin)?;
        for s in &mut sides {
            dress(s, source, plane.normal, &material);
        }
        let b = build_solid(&sides).aabb;
        if b.min.min_element() < -MAX_COORD || b.max.max_element() > MAX_COORD {
            return Err(ExtrudeIssue::PastLimits.into());
        }
        prisms.push(sides);
    }
    match params.mode {
        ExtrudeMode::Carve => {
            let mut pieces = vec![ClipPiece::whole(sides)];
            for carver in &prisms {
                let mut next_pieces = Vec::with_capacity(pieces.len() + 4);
                for p in pieces {
                    match carve_traced(&p.sides, carver, next_side_id) {
                        Ok(v) => next_pieces.extend(v.into_iter().map(|q| q.compose(&p.origins))),
                        Err(_) => next_pieces.push(p),
                    }
                }
                pieces = next_pieces;
            }
            if pieces == [ClipPiece::whole(sides)] {
                return Err(ExtrudeIssue::NoCut.into());
            }
            let mut pieces = merge_pieces(pieces);
            for p in &mut pieces {
                for (s, o) in p.sides.iter_mut().zip(&p.origins) {
                    if o.is_none() {
                        dress(s, source, plane.normal, &material);
                    }
                }
            }
            Ok(FaceExtrudeResult { pieces, added: Vec::new(), prism: prisms, merged: false })
        }
        ExtrudeMode::Extrude => {
            let mergeable = sides.iter().all(|s| s.disp.is_none());
            let mut list = vec![ClipPiece::whole(sides)];
            list.extend(prisms.iter().map(|p| ClipPiece { sides: p.clone(), origins: vec![None; p.len()] }));
            let list = if mergeable { merge_pieces(list) } else { list };
            let mut pieces = Vec::new();
            let mut added = Vec::new();
            for p in list {
                if p.origins.iter().any(Option::is_some) {
                    pieces.push(p);
                } else {
                    added.push(p.sides);
                }
            }
            let merged = pieces.first().is_some_and(|p| p.sides.len() != sides.len() || p.sides != sides);
            Ok(FaceExtrudeResult { pieces, added, prism: prisms, merged })
        }
    }
}

pub fn face_prism(
    sides: &[Side],
    side_index: usize,
    depth: f64,
    material: Option<&str>,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<Side>, GeomError> {
    let geom = validate_solid(sides)?;
    let face = geom.face_for_side(side_index).ok_or(ExtrudeIssue::NotAFace)?;
    let source = sides.get(side_index).ok_or(ExtrudeIssue::NotAFace)?;
    if source.disp.is_some() {
        return Err(ExtrudeIssue::Displacement.into());
    }
    if !depth.is_finite() || depth < MIN_EXTRUDE_DEPTH {
        return Err(ExtrudeIssue::NoDepth.into());
    }
    let frame = FaceFrame::new(&face.plane);
    let mut ring: Vec<DVec3> = face.vertices.clone();
    let flat: Vec<DVec2> = ring.iter().map(|v| frame.to_2d(*v)).collect();
    if signed_area(&flat) < 0.0 {
        ring.reverse();
    }
    let material =
        material.map(str::trim).filter(|m| !m.is_empty()).map_or_else(|| source.material.clone(), str::to_owned);
    let mut out = prism(&ring, face.plane.normal, 0.0, depth, source, next_side_id).ok_or(ExtrudeIssue::TooThin)?;
    for s in &mut out {
        dress(s, source, face.plane.normal, &material);
    }
    let b = build_solid(&out).aabb;
    if b.min.min_element() < -MAX_COORD || b.max.max_element() > MAX_COORD {
        return Err(ExtrudeIssue::PastLimits.into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Aabb;
    use crate::test_support::*;

    fn v2(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    fn v3(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x, y, z)
    }

    fn volume(sides: &[Side]) -> f64 {
        let g = build_solid(sides);
        assert!(g.valid, "piece must be valid");
        g.volume()
    }

    fn check_pieces(pieces: &[Vec<Side>]) {
        let mut ids: Vec<u32> = pieces.iter().flatten().map(|s| s.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "side ids are unique");
        for p in pieces {
            let g = validate_solid(p).expect("valid piece");
            assert!(g.volume() > 1.0, "no sliver: {}", g.volume());
            assert_eq!(g.faces.len(), p.len(), "no redundant side");
            let verts: Vec<DVec3> = g.vertices().collect();
            for f in &g.faces {
                for v in &verts {
                    assert!(f.plane.distance(*v) <= 1e-6, "convex");
                }
            }
        }
    }

    fn wall() -> Vec<Side> {
        hammer_box(v3(0.0, 0.0, 0.0), v3(256.0, 16.0, 128.0))
    }

    fn front_side(sides: &[Side]) -> usize {
        sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == DVec3::NEG_Y)).expect("front face")
    }

    fn on_front(points: &[(f64, f64)]) -> Vec<DVec3> {
        points.iter().map(|&(x, z)| v3(x, 0.0, z)).collect()
    }

    fn all_sides(r: &FaceExtrudeResult) -> Vec<Vec<Side>> {
        r.pieces.iter().map(|p| p.sides.clone()).chain(r.added.iter().cloned()).collect()
    }

    #[test]
    fn decomposing_outlines() {
        let square = [v2(0.0, 0.0), v2(64.0, 0.0), v2(64.0, 64.0), v2(0.0, 64.0)];
        assert_eq!(convex_decompose(&square).expect("square").len(), 1);
        let l = [v2(0.0, 0.0), v2(128.0, 0.0), v2(128.0, 64.0), v2(64.0, 64.0), v2(64.0, 128.0), v2(0.0, 128.0)];
        let pieces = convex_decompose(&l).expect("L");
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        let area: f64 = pieces.iter().map(|p| signed_area(&p.iter().map(|&i| l[i]).collect::<Vec<_>>())).sum();
        assert!((area - signed_area(&l)).abs() < 1e-9);
        let u = [
            v2(0.0, 0.0),
            v2(96.0, 0.0),
            v2(96.0, 96.0),
            v2(64.0, 96.0),
            v2(64.0, 32.0),
            v2(32.0, 32.0),
            v2(32.0, 96.0),
            v2(0.0, 96.0),
        ];
        let pieces = convex_decompose(&u).expect("U");
        assert!(pieces.len() <= 4, "{pieces:?}");
        for p in &pieces {
            let poly: Vec<DVec2> = p.iter().map(|&i| u[i]).collect();
            assert!(signed_area(&poly) > 100.0, "no sliver piece");
        }
        let star: Vec<DVec2> = (0..10)
            .map(|k| {
                let r = if k % 2 == 0 { 100.0 } else { 40.0 };
                DVec2::from_angle(f64::from(k) * std::f64::consts::TAU / 10.0) * r
            })
            .collect();
        let pieces = convex_decompose(&star).expect("star");
        let area: f64 = pieces.iter().map(|p| signed_area(&p.iter().map(|&i| star[i]).collect::<Vec<_>>())).sum();
        assert!((area - signed_area(&star)).abs() < 1e-6);
        let bow = [v2(0.0, 0.0), v2(64.0, 64.0), v2(64.0, 0.0), v2(0.0, 64.0)];
        assert!(!outline_is_simple(&bow));
        assert!(convex_decompose(&bow).is_none());
        let mut cw = square;
        cw.reverse();
        assert!(convex_decompose(&cw).is_none(), "the outline must turn counterclockwise");
        assert!(path_is_simple(&[v2(0.0, 0.0), v2(64.0, 0.0), v2(64.0, 64.0)]));
        assert!(!path_is_simple(&[v2(0.0, 0.0), v2(64.0, 0.0), v2(64.0, 64.0), v2(32.0, -10.0)]));
        assert!(!path_is_simple(&[v2(0.0, 0.0), v2(64.0, 0.0), v2(32.0, 0.0)]));
        assert_eq!(
            clean_outline(&[v2(0.0, 0.0), v2(32.0, 0.0), v2(64.0, 0.0), v2(64.0, 64.0), v2(0.0, 0.0)]),
            vec![0, 2, 3]
        );
    }

    #[test]
    fn window_through_a_wall() {
        let sides = wall();
        let front = front_side(&sides);
        let mut next = id_counter(1000);
        let params = FaceExtrudeParams { through: true, ..Default::default() };
        let outline = on_front(&[(64.0, 32.0), (192.0, 32.0), (192.0, 96.0), (64.0, 96.0)]);
        let r = face_extrude(&sides, front, &outline, &params, &mut next).expect("carve");
        assert!(r.added.is_empty() && !r.merged && r.changes_brush());
        let all = all_sides(&r);
        check_pieces(&all);
        assert_eq!(all.len(), 4, "left, right, top and bottom of the window");
        let total: f64 = all.iter().map(|p| volume(p)).sum();
        assert!((total - (256.0 * 16.0 * 128.0 - 128.0 * 16.0 * 64.0)).abs() < 1e-6);
        let original: Vec<&str> = sides.iter().map(|s| s.material.as_str()).collect();
        for p in &r.pieces {
            for (s, o) in p.sides.iter().zip(&p.origins) {
                match o {
                    Some(o) => {
                        let src = &sides[*o];
                        assert_eq!(s.material, src.material, "kept faces keep their texture");
                        assert_eq!((s.uaxis, s.vaxis), (src.uaxis, src.vaxis), "and their alignment");
                    }
                    None => {
                        assert_eq!(s.material, sides[front].material, "inner faces get the face's texture");
                        assert!(original.contains(&s.material.as_str()));
                    }
                }
            }
        }
        let hole = |p: DVec3| all.iter().all(|s| build_solid(s).faces.iter().any(|f| f.plane.distance(p) > 0.01));
        assert!(hole(v3(128.0, 8.0, 64.0)), "the window is open");
        assert!(!hole(v3(16.0, 8.0, 64.0)));
    }

    #[test]
    fn partial_recess_with_the_active_texture() {
        let sides = wall();
        let front = front_side(&sides);
        let mut next = id_counter(1000);
        let params = FaceExtrudeParams { depth: 8.0, material: Some("DEV/RECESS".to_owned()), ..Default::default() };
        let outline = on_front(&[(64.0, 32.0), (192.0, 32.0), (192.0, 96.0), (64.0, 96.0)]);
        let r = face_extrude(&sides, front, &outline, &params, &mut next).expect("carve");
        let all = all_sides(&r);
        check_pieces(&all);
        let total: f64 = all.iter().map(|p| volume(p)).sum();
        assert!((total - (256.0 * 16.0 * 128.0 - 128.0 * 8.0 * 64.0)).abs() < 1e-6);
        let back = r.pieces.iter().flat_map(|p| p.sides.iter().zip(&p.origins)).find(|(s, o)| {
            o.is_none() && Plane::from_side(s).is_some_and(|p| p.normal == DVec3::NEG_Y && (p.dist + 8.0).abs() < 1e-9)
        });
        let (bottom, _) = back.expect("the recess has a back face");
        assert_eq!(bottom.material, "DEV/RECESS");
        assert_eq!((bottom.uaxis, bottom.vaxis), (sides[front].uaxis, sides[front].vaxis), "aligned with the face");
        let walls: Vec<&Side> = r
            .pieces
            .iter()
            .flat_map(|p| p.sides.iter().zip(&p.origins))
            .filter(|(s, o)| o.is_none() && Plane::from_side(s).is_some_and(|p| p.normal.y.abs() < 0.5))
            .map(|(s, _)| s)
            .collect();
        assert!(!walls.is_empty());
        assert!(walls.iter().all(|s| s.material == "DEV/RECESS"));
        let inside = v3(128.0, 4.0, 64.0);
        assert!(all.iter().all(|s| build_solid(s).faces.iter().any(|f| f.plane.distance(inside) > 0.01)));
        let solid = v3(128.0, 12.0, 64.0);
        assert!(all.iter().any(|s| build_solid(s).faces.iter().all(|f| f.plane.distance(solid) < 0.0)));
        let deep = FaceExtrudeParams { depth: 500.0, ..Default::default() };
        let r = face_extrude(&sides, front, &outline, &deep, &mut next).expect("carve");
        let total: f64 = all_sides(&r).iter().map(|p| volume(p)).sum();
        assert!(
            (total - (256.0 * 16.0 * 128.0 - 128.0 * 16.0 * 64.0)).abs() < 1e-6,
            "deeper than the wall cuts through"
        );
    }

    #[test]
    fn concave_outline_through_a_floor() {
        let sides = hammer_box(v3(0.0, 0.0, 0.0), v3(512.0, 512.0, 32.0));
        let top = 0;
        assert_eq!(Plane::from_side(&sides[top]).expect("top").normal, DVec3::Z);
        let mut next = id_counter(1000);
        let l = [(64.0, 64.0), (320.0, 64.0), (320.0, 192.0), (192.0, 192.0), (192.0, 384.0), (64.0, 384.0)];
        let outline: Vec<DVec3> = l.iter().map(|&(x, y)| v3(x, y, 40.0)).collect();
        let params = FaceExtrudeParams { through: true, ..Default::default() };
        let r = face_extrude(&sides, top, &outline, &params, &mut next).expect("carve");
        assert_eq!(r.prism.len(), 2, "an L is two convex pieces");
        let all = all_sides(&r);
        check_pieces(&all);
        let hole = 256.0 * 128.0 + 128.0 * 192.0;
        let total: f64 = all.iter().map(|p| volume(p)).sum();
        assert!((total - (512.0 * 512.0 - hole) * 32.0).abs() < 1e-6, "{total}");
        assert!(all.len() <= 8, "{} pieces", all.len());
        let star: Vec<DVec3> = (0..10)
            .map(|k| {
                let r = if k % 2 == 0 { 160.0 } else { 64.0 };
                let d = DVec2::from_angle(f64::from(k) * std::f64::consts::TAU / 10.0) * r;
                v3((256.0 + d.x).round(), (256.0 + d.y).round(), 32.0)
            })
            .collect();
        let shallow = FaceExtrudeParams { depth: 16.0, ..Default::default() };
        let r = face_extrude(&sides, top, &star, &shallow, &mut next).expect("star");
        let all = all_sides(&r);
        check_pieces(&all);
        let flat: Vec<DVec2> = star.iter().map(|p| p.truncate()).collect();
        let total: f64 = all.iter().map(|p| volume(p)).sum();
        assert!((total - (512.0 * 512.0 * 32.0 - signed_area(&flat).abs() * 16.0)).abs() < 1e-3, "{total}");
    }

    #[test]
    fn extrude_adds_or_extends() {
        let sides = wall();
        let front = front_side(&sides);
        let mut next = id_counter(1000);
        let params = FaceExtrudeParams { mode: ExtrudeMode::Extrude, depth: 32.0, ..Default::default() };
        let outline = on_front(&[(64.0, 32.0), (192.0, 32.0), (192.0, 96.0), (64.0, 96.0)]);
        let r = face_extrude(&sides, front, &outline, &params, &mut next).expect("extrude");
        assert!(!r.merged);
        assert_eq!(r.pieces, vec![ClipPiece::whole(&sides)], "the wall stays");
        assert_eq!(r.added.len(), 1);
        check_pieces(&r.added);
        let b = build_solid(&r.added[0]).aabb;
        assert_eq!(b, Aabb::new(v3(64.0, -32.0, 32.0), v3(192.0, 0.0, 96.0)));
        assert!(r.added[0].iter().all(|s| s.material == sides[front].material));
        let whole = on_front(&[(0.0, 0.0), (256.0, 0.0), (256.0, 128.0), (0.0, 128.0)]);
        let r = face_extrude(&sides, front, &whole, &params, &mut next).expect("extend");
        assert!(r.merged && r.added.is_empty());
        assert_eq!(r.pieces.len(), 1);
        let g = validate_solid(&r.pieces[0].sides).expect("valid");
        assert_eq!(g.faces.len(), 6);
        assert_eq!(g.aabb, Aabb::new(v3(0.0, -32.0, 0.0), v3(256.0, 16.0, 128.0)));
        let kept = r.pieces[0].sides.iter().zip(&r.pieces[0].origins).filter(|(_, o)| o.is_some()).count();
        assert_eq!(kept, 5, "every face but the pushed one is kept");
        let t = on_front(&[
            (0.0, 0.0),
            (256.0, 0.0),
            (256.0, 64.0),
            (160.0, 64.0),
            (160.0, 128.0),
            (96.0, 128.0),
            (96.0, 64.0),
            (0.0, 64.0),
        ]);
        let r = face_extrude(&sides, front, &t, &params, &mut next).expect("T");
        let all = all_sides(&r);
        check_pieces(&all);
        let total: f64 = all.iter().map(|p| volume(p)).sum();
        assert!((total - (256.0 * 16.0 * 128.0 + (256.0 * 64.0 + 64.0 * 64.0) * 32.0)).abs() < 1e-6);
        assert!(!r.merged, "a partial extrusion is not convex with the wall");
        assert!(r.added.len() >= 2);
    }

    #[test]
    fn invalid_requests() {
        let sides = wall();
        let front = front_side(&sides);
        let mut next = id_counter(1000);
        let p = FaceExtrudeParams::default();
        let outside = on_front(&[(300.0, 0.0), (400.0, 0.0), (400.0, 64.0)]);
        assert!(face_extrude(&sides, front, &outside, &p, &mut next).is_err());
        let bow = on_front(&[(0.0, 0.0), (64.0, 64.0), (64.0, 0.0), (0.0, 64.0)]);
        assert!(face_extrude(&sides, front, &bow, &p, &mut next).is_err());
        let line = on_front(&[(0.0, 0.0), (32.0, 0.0), (64.0, 0.0)]);
        assert!(face_extrude(&sides, front, &line, &p, &mut next).is_err());
        let ok = on_front(&[(64.0, 32.0), (192.0, 32.0), (192.0, 96.0)]);
        let zero = FaceExtrudeParams { depth: 0.0, ..Default::default() };
        assert!(face_extrude(&sides, front, &ok, &zero, &mut next).is_err());
        assert!(face_extrude(&sides, 99, &ok, &p, &mut next).is_err());
        let r = face_extrude(&sides, front, &ok, &p, &mut next).expect("triangle recess");
        check_pieces(&all_sides(&r));
        let off_plane: Vec<DVec3> = ok.iter().map(|q| *q + v3(0.0, -50.0, 0.0)).collect();
        let r2 = face_extrude(&sides, front, &off_plane, &p, &mut next).expect("points are projected on the face");
        assert_eq!(r2.pieces.len(), r.pieces.len());
        assert_eq!(face_thickness(&sides, front), Some(16.0));
        assert_eq!(ExtrudeMode::from_id("Extrude"), Some(ExtrudeMode::Extrude));
        assert_eq!(ExtrudeMode::from_id(ExtrudeMode::Carve.id()), Some(ExtrudeMode::Carve));
        assert!(ExtrudeMode::from_id("x").is_none());
        assert!(ExtrudeIssue::TooManyPoints.message().contains(&MAX_OUTLINE_POINTS.to_string()));
        assert!(ExtrudeIssue::NoDepth.message().contains(&MIN_EXTRUDE_DEPTH.to_string()));
        for issue in ExtrudeIssue::ALL {
            assert_eq!(ExtrudeIssue::of(&issue.into()), Some(issue));
        }
        assert_eq!(ExtrudeIssue::of(&GeomError::NoVolume), None);
        let e = face_extrude(&sides, front, &outside, &p, &mut next).expect_err("outside");
        assert_eq!(ExtrudeIssue::of(&e), Some(ExtrudeIssue::OutsideFace));
        let e = face_extrude(&sides, front, &bow, &p, &mut next).expect_err("bow");
        assert_eq!(ExtrudeIssue::of(&e), Some(ExtrudeIssue::Crossing));
    }

    #[test]
    fn slanted_face() {
        let p = crate::PrimitiveParams {
            kind: crate::Primitive::Wedge,
            bounds: Aabb::new(v3(0.0, 0.0, 0.0), v3(256.0, 256.0, 128.0)),
            ..Default::default()
        };
        let mut next = id_counter(1000);
        let wedge = crate::make_primitive(&p, &mut next).expect("wedge").remove(0);
        let g = build_solid(&wedge);
        let slope = g.faces.iter().find(|f| f.plane.normal.z > 0.1 && f.plane.normal.z < 0.99).expect("slope");
        let c = slope.center();
        let frame = FaceFrame::new(&slope.plane);
        let outline: Vec<DVec3> = [v2(-32.0, -32.0), v2(32.0, -32.0), v2(32.0, 32.0), v2(-32.0, 32.0)]
            .iter()
            .map(|q| frame.to_3d(frame.to_2d(c) + *q, 0.0))
            .collect();
        let params = FaceExtrudeParams { depth: 16.0, ..Default::default() };
        let r = face_extrude(&wedge, slope.side_index, &outline, &params, &mut next).expect("slope recess");
        let all = all_sides(&r);
        check_pieces(&all);
        let total: f64 = all.iter().map(|p| volume(p)).sum();
        assert!((total - (g.volume() - 64.0 * 64.0 * 16.0)).abs() < 1e-3, "{total}");
        let up = FaceExtrudeParams { mode: ExtrudeMode::Extrude, depth: 16.0, ..Default::default() };
        let r = face_extrude(&wedge, slope.side_index, &outline, &up, &mut next).expect("slope extrude");
        let all = all_sides(&r);
        check_pieces(&all);
        let total: f64 = all.iter().map(|p| volume(p)).sum();
        assert!((total - (g.volume() + 64.0 * 64.0 * 16.0)).abs() < 1e-3, "{total}");
    }

    fn random_brush(rng: &mut Rng) -> Vec<Side> {
        let mut next = id_counter(1);
        loop {
            let scale = [0.5, 8.0, 64.0, 512.0, 8000.0][(rng.next_u64() % 5) as usize];
            let c = rng.point(256.0);
            let pts: Vec<DVec3> = (0..(4 + rng.next_u64() % 10)).map(|_| c + rng.point(scale)).collect();
            if let Ok(sides) = crate::hull_solid(&pts, "TEST/X", 0.25, 16, &mut next)
                && build_solid(&sides).valid
            {
                return sides;
            }
        }
    }

    #[test]
    fn random_outlines_never_panic_and_give_valid_pieces() {
        let mut rng = Rng(0x5EED_1234_ABCD_0001);
        let mut made = 0;
        for _ in 0..1500 {
            let sides = random_brush(&mut rng);
            let g = build_solid(&sides);
            let face = &g.faces[(rng.next_u64() as usize) % g.faces.len()];
            let center = face.center();
            let span = face.vertices.iter().map(|v| v.distance(center)).fold(0.0f64, f64::max).max(0.01);
            let n = 1 + (rng.next_u64() % 9) as usize;
            let mut outline: Vec<DVec3> = (0..n)
                .map(|_| match rng.next_u64() % 6 {
                    0 => face.vertices[(rng.next_u64() as usize) % face.vertices.len()],
                    1 => center + rng.point(span * 3.0),
                    2 => center,
                    _ => center + rng.point(span * 0.8),
                })
                .collect();
            if rng.next_u64().is_multiple_of(20) {
                outline.push(DVec3::NAN);
            }
            if rng.next_u64().is_multiple_of(10) {
                let p = outline[0];
                outline.push(p);
            }
            let params = FaceExtrudeParams {
                mode: if rng.next_u64().is_multiple_of(2) { ExtrudeMode::Carve } else { ExtrudeMode::Extrude },
                depth: [0.0, 0.1, 0.125, 1.0, 16.0, 500.0, 1e6, f64::NAN][(rng.next_u64() % 8) as usize],
                through: rng.next_u64().is_multiple_of(3),
                material: None,
            };
            let mut next = id_counter(10_000);
            if let Ok(r) = face_extrude(&sides, face.side_index, &outline, &params, &mut next) {
                made += 1;
                for p in r.pieces.iter().map(|p| &p.sides).chain(&r.added) {
                    let pg = validate_solid(p).expect("valid piece");
                    let verts: Vec<DVec3> = pg.vertices().collect();
                    for f in &pg.faces {
                        assert!(verts.iter().all(|v| f.plane.distance(*v) <= 1e-3), "convex piece");
                    }
                }
            }
        }
        assert!(made > 100, "{made}");
    }

    #[test]
    fn a_face_prism_is_a_new_separate_brush_on_the_face() {
        let side_facing = |sides: &[Side], normal: DVec3| {
            sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal.dot(normal) > 0.999)).expect("side")
        };
        let cube = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let top = side_facing(&cube, DVec3::Z);
        let mut n = 5000;
        let mut next = || {
            n += 1;
            n
        };
        let brush = face_prism(&cube, top, 32.0, None, &mut next).expect("prism");
        let g = build_solid(&brush);
        assert!(g.valid);
        assert!((g.volume() - 128.0 * 128.0 * 32.0).abs() < 1e-6);
        assert!((g.aabb.min.z - 64.0).abs() < 1e-6 && (g.aabb.max.z - 96.0).abs() < 1e-6);
        assert!(face_prism(&cube, top, 0.0, None, &mut next).is_err());
        assert!(face_prism(&cube, 99, 8.0, None, &mut next).is_err());
        let down = side_facing(&cube, DVec3::NEG_Z);
        let g = build_solid(&face_prism(&cube, down, 8.0, Some("DEV/X"), &mut next).expect("down"));
        assert!((g.aabb.min.z + 8.0).abs() < 1e-6 && g.aabb.max.z.abs() < 1e-6);
    }
}
