use glam::{DVec2, DVec3};
use hp_vmf::{DispInfo, Side, TexAxis};

use crate::brush::validate_solid;
use crate::csg::ClipPiece;
use crate::extrude::{
    ExtrudeIssue, FaceFrame, MIN_EXTRUDE_DEPTH, cap_points, merge_pieces, new_side, oriented, signed_area,
};
use crate::hull::convex_hull;
use crate::texture::{effective_scale, wrap_texture};
use crate::util::clean_vec;
use crate::{GeomError, MAX_COORD, PLANE_EPS, Plane, SolidGeom, build_solid};

pub const MIN_DIRECTION_DOT: f64 = 0.05;

const EDGE_EPS: f64 = 0.01;

const MIN_WEDGE_VOLUME: f64 = 1e-3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExtrudeAxis {
    #[default]
    Auto,
    X,
    Y,
    Z,
}

impl ExtrudeAxis {
    pub const ALL: [ExtrudeAxis; 4] = [ExtrudeAxis::Auto, ExtrudeAxis::X, ExtrudeAxis::Y, ExtrudeAxis::Z];

    pub fn id(self) -> &'static str {
        match self {
            ExtrudeAxis::Auto => "auto",
            ExtrudeAxis::X => "x",
            ExtrudeAxis::Y => "y",
            ExtrudeAxis::Z => "z",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|a| a.id() == s)
    }

    fn index(self, average: DVec3) -> usize {
        match self {
            ExtrudeAxis::X => 0,
            ExtrudeAxis::Y => 1,
            ExtrudeAxis::Z => 2,
            ExtrudeAxis::Auto => {
                let a = average.abs();
                if a.z >= a.x && a.z >= a.y {
                    2
                } else if a.x >= a.y {
                    0
                } else {
                    1
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExtrudeDirection {
    #[default]
    EachNormal,
    Average,
    Axis(ExtrudeAxis),
}

impl ExtrudeDirection {
    pub fn id(self) -> &'static str {
        match self {
            ExtrudeDirection::EachNormal => "normal",
            ExtrudeDirection::Average => "average",
            ExtrudeDirection::Axis(_) => "axis",
        }
    }

    pub fn from_ids(mode: &str, axis: ExtrudeAxis) -> Option<Self> {
        match mode.trim().to_ascii_lowercase().as_str() {
            "normal" | "each" => Some(ExtrudeDirection::EachNormal),
            "average" => Some(ExtrudeDirection::Average),
            "axis" => Some(ExtrudeDirection::Axis(axis)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NormalExtrudeParams {
    pub direction: ExtrudeDirection,
    pub grow: bool,
    pub material: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalExtrudeFace {
    pub brush: usize,
    pub side: usize,
    pub distance: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NormalExtrudeResult {
    pub changed: Vec<(usize, ClipPiece)>,
    pub added: Vec<(usize, Vec<Side>)>,
}

impl NormalExtrudeResult {
    pub fn brush_count(&self) -> usize {
        self.changed.len() + self.added.len()
    }
}

pub fn extrude_directions(normals: &[DVec3], direction: ExtrudeDirection) -> Result<Vec<DVec3>, GeomError> {
    let first = *normals.first().ok_or(ExtrudeIssue::NotAFace)?;
    let average = normals.iter().fold(DVec3::ZERO, |s, n| s + *n).try_normalize();
    match direction {
        ExtrudeDirection::EachNormal => Ok(normals.to_vec()),
        ExtrudeDirection::Average => {
            let v = average.ok_or(ExtrudeIssue::AwayFromDirection)?;
            if normals.iter().any(|n| n.dot(v) < MIN_DIRECTION_DOT) {
                return Err(ExtrudeIssue::AwayFromDirection.into());
            }
            Ok(vec![v; normals.len()])
        }
        ExtrudeDirection::Axis(axis) => {
            let k = axis.index(average.unwrap_or(first));
            normals
                .iter()
                .map(|n| {
                    let c = n[k];
                    if c.abs() < MIN_DIRECTION_DOT {
                        return Err(ExtrudeIssue::AwayFromDirection.into());
                    }
                    let mut v = DVec3::ZERO;
                    v[k] = c.signum();
                    Ok(v)
                })
                .collect()
        }
    }
}

struct Face {
    brush: usize,
    side: usize,
    ring: Vec<DVec3>,
    normal: DVec3,
    offset: DVec3,
}

impl Face {
    fn center(&self) -> DVec3 {
        self.ring.iter().fold(DVec3::ZERO, |s, p| s + *p) / self.ring.len().max(1) as f64
    }
}

fn loose(sides: Vec<Side>) -> ClipPiece {
    let origins = vec![None; sides.len()];
    ClipPiece { sides, origins }
}

fn same_polygon(a: &[DVec3], b: &[DVec3]) -> bool {
    a.len() == b.len() && a.iter().all(|p| b.iter().any(|q| p.distance(*q) <= EDGE_EPS))
}

fn brush_order(faces: &[Face]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for f in faces {
        if !out.contains(&f.brush) {
            out.push(f.brush);
        }
    }
    out
}

fn copy_face_texture(side: &mut Side, source: &Side, offset: DVec3, material: &str) {
    let moved = |a: &TexAxis| TexAxis { shift: a.shift - offset.dot(a.axis) / effective_scale(a.scale), ..*a };
    material.clone_into(&mut side.material);
    side.uaxis = moved(&source.uaxis);
    side.vaxis = moved(&source.vaxis);
    side.rotation = source.rotation;
    side.lightmap_scale = source.lightmap_scale;
    side.smoothing_groups = source.smoothing_groups;
    side.disp = None;
    side.vertices_plus = None;
}

fn dress_wall(side: &mut Side, source: &Side, normal: DVec3, edge_point: DVec3, material: &str) {
    let Some(p) = Plane::from_side(side) else { return };
    if p.normal.dot(normal).abs() > 1.0 - 1e-9 {
        copy_face_texture(side, source, DVec3::ZERO, material);
        return;
    }
    wrap_texture(source, normal, side, p.normal, edge_point);
    material.clone_into(&mut side.material);
    side.smoothing_groups = source.smoothing_groups;
    side.disp = None;
    side.vertices_plus = None;
}

fn oblique_prism(
    face: &Face,
    source: &Side,
    wall_material: &str,
    top_material: &str,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Option<Vec<Side>> {
    let ring: Vec<DVec3> = face.ring.iter().map(|p| clean_vec(*p)).collect();
    let n = ring.len();
    let offset = face.offset;
    let center = face.center();
    let mut walls: Vec<(Side, Plane)> = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let mut out = (b - a).cross(offset);
        if out.dot(center - a) > 0.0 {
            out = -out;
        }
        let pts = oriented([clean_vec(a + offset), clean_vec(b + offset), b], out);
        let plane = Plane::from_points(pts[0], pts[1], pts[2])?;
        if walls.iter().any(|(_, q)| q.approx_eq(&plane, 1e-9, 1e-6)) {
            continue;
        }
        let mut side = new_side(next_side_id(), pts, source);
        dress_wall(&mut side, source, face.normal, b, wall_material);
        walls.push((side, plane));
    }
    let high: Vec<DVec3> = ring.iter().map(|p| clean_vec(*p + offset)).collect();
    let mut bottom = new_side(next_side_id(), cap_points(&ring, -face.normal)?, source);
    copy_face_texture(&mut bottom, source, DVec3::ZERO, wall_material);
    let mut top = new_side(next_side_id(), cap_points(&high, face.normal)?, source);
    copy_face_texture(&mut top, source, offset, top_material);
    let mut sides: Vec<Side> = walls.into_iter().map(|(s, _)| s).collect();
    sides.push(bottom);
    sides.push(top);
    build_solid(&sides).valid.then_some(sides)
}

fn shared_segment(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> Option<(DVec3, DVec3)> {
    let ab = b - a;
    let len = ab.length();
    if len < EDGE_EPS {
        return None;
    }
    let u = ab / len;
    let off_line = |p: DVec3| {
        let r = p - a;
        (r - u * r.dot(u)).length() > EDGE_EPS
    };
    if off_line(c) || off_line(d) {
        return None;
    }
    let (tc, td) = ((c - a).dot(u), (d - a).dot(u));
    let lo = tc.min(td).max(0.0);
    let hi = tc.max(td).min(len);
    (hi - lo >= MIN_EXTRUDE_DEPTH).then(|| (a + u * lo, a + u * hi))
}

fn opens_gap(fi: &Face, fj: &Face, p: DVec3, q: DVec3) -> bool {
    let e = q - p;
    let wall = |f: &Face| -> Option<DVec3> {
        let m = e.cross(f.offset).try_normalize()?;
        Some(if m.dot(f.center() - p) > 0.0 { -m } else { m })
    };
    let (Some(mi), Some(mj)) = (wall(fi), wall(fj)) else { return false };
    fj.offset.dot(mi) > 1e-6 * fj.offset.length() && fi.offset.dot(mj) > 1e-6 * fi.offset.length()
}

fn wedge_sides(
    points: &[DVec3],
    source: &Side,
    normal: DVec3,
    edge_point: DVec3,
    material: &str,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Option<Vec<Side>> {
    let points: Vec<DVec3> = points.iter().map(|p| clean_vec(*p)).collect();
    let hull = convex_hull(&points).ok()?;
    if hull.volume() < MIN_WEDGE_VOLUME {
        return None;
    }
    let mut sides = Vec::with_capacity(hull.faces.len());
    for f in &hull.faces {
        let mut side = new_side(next_side_id(), hull.face_plane_points(f), source);
        dress_wall(&mut side, source, normal, edge_point, material);
        sides.push(side);
    }
    build_solid(&sides).valid.then_some(sides)
}

fn ring_edges(ring: &[DVec3]) -> impl Iterator<Item = (DVec3, DVec3)> + '_ {
    (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()]))
}

fn wedges(
    faces: &[Face],
    brushes: &[&[Side]],
    material: Option<&str>,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Vec<(usize, usize, Vec<Side>)> {
    let mut out = Vec::new();
    for i in 0..faces.len() {
        for j in (i + 1)..faces.len() {
            let (fi, fj) = (&faces[i], &faces[j]);
            if (fi.offset - fj.offset).length() < 1e-9 {
                continue;
            }
            let source = &brushes[fi.brush][fi.side];
            let material = material.unwrap_or(&source.material);
            for (a, b) in ring_edges(&fi.ring) {
                for (c, d) in ring_edges(&fj.ring) {
                    let Some((p, q)) = shared_segment(a, b, c, d) else { continue };
                    if !opens_gap(fi, fj, p, q) {
                        continue;
                    }
                    let pts = [p, q, p + fi.offset, q + fi.offset, p + fj.offset, q + fj.offset];
                    if let Some(sides) = wedge_sides(&pts, source, fi.normal, p, material, next_side_id) {
                        out.push((i, j, sides));
                    }
                }
            }
        }
    }
    out
}

fn take_disps(piece: &mut ClipPiece) -> Vec<(u32, Box<DispInfo>)> {
    piece.sides.iter_mut().filter_map(|s| s.disp.take().map(|d| (s.id, d))).collect()
}

fn grow(original: &[Side], geom: &SolidGeom, faces: &[&Face], items: Vec<Vec<Side>>) -> Option<GrowOutcome> {
    let mut whole = ClipPiece::whole(original);
    take_disps(&mut whole);
    let mut list = vec![whole];
    list.extend(items.into_iter().map(loose));
    let mut changed: Option<ClipPiece> = None;
    let mut extra = Vec::new();
    for p in merge_pieces(list) {
        if p.origins.iter().any(Option::is_some) {
            if changed.is_some() {
                return None;
            }
            changed = Some(p);
        } else {
            extra.push(p.sides);
        }
    }
    let mut piece = changed?;
    let g = build_solid(&piece.sides);
    if !g.valid {
        return None;
    }
    let mut tops: Vec<usize> = Vec::new();
    for f in faces {
        let top = Plane { normal: f.normal, dist: f.normal.dot(f.ring[0] + f.offset) };
        let found = (0..piece.sides.len()).find(|&k| {
            piece.origins[k].is_none()
                && Plane::from_side(&piece.sides[k]).is_some_and(|p| p.approx_eq(&top, 1e-6, PLANE_EPS))
        });
        let Some(k) = found else { continue };
        let source = original.get(f.side)?;
        piece.sides[k].id = source.id;
        piece.origins[k] = Some(f.side);
        tops.push(k);
        if let Some(disp) = &source.disp {
            let moved: Vec<DVec3> = f.ring.iter().map(|v| *v + f.offset).collect();
            if !same_polygon(&moved, &g.face_for_side(k)?.vertices) {
                return None;
            }
            let mut d = disp.clone();
            d.start_position = clean_vec(d.start_position + f.offset);
            piece.sides[k].disp = Some(d);
        }
    }
    for k in 0..piece.sides.len() {
        if tops.contains(&k) {
            continue;
        }
        let Some(j) = piece.origins[k] else { continue };
        let Some(disp) = original.get(j).and_then(|s| s.disp.as_ref()) else { continue };
        if !same_polygon(&geom.face_for_side(j)?.vertices, &g.face_for_side(k)?.vertices) {
            return None;
        }
        piece.sides[k].disp = Some(disp.clone());
    }
    let unchanged = tops.is_empty()
        && piece.sides.len() == original.len()
        && piece.origins.iter().enumerate().all(|(i, o)| *o == Some(i));
    Some(GrowOutcome { changed: (!unchanged).then_some(piece), extra })
}

struct GrowOutcome {
    changed: Option<ClipPiece>,
    extra: Vec<Vec<Side>>,
}

fn absorb(piece: &ClipPiece, wedge: &[Side]) -> Option<ClipPiece> {
    let before = build_solid(&piece.sides);
    let mut bare = piece.clone();
    let disps = take_disps(&mut bare);
    let mut merged = merge_pieces(vec![bare, loose(wedge.to_vec())]);
    if merged.len() != 1 {
        return None;
    }
    let mut m = merged.pop()?;
    let after = build_solid(&m.sides);
    for (id, disp) in disps {
        let old = piece.sides.iter().position(|s| s.id == id)?;
        let new = m.sides.iter().position(|s| s.id == id)?;
        if !same_polygon(&before.face_for_side(old)?.vertices, &after.face_for_side(new)?.vertices) {
            return None;
        }
        m.sides[new].disp = Some(disp);
    }
    Some(m)
}

fn check_limits(sides: &[Side]) -> Result<(), GeomError> {
    let b = build_solid(sides).aabb;
    if b.min.min_element() < -MAX_COORD || b.max.max_element() > MAX_COORD {
        return Err(ExtrudeIssue::PastLimits.into());
    }
    Ok(())
}

fn extrude_out(
    brushes: &[&[Side]],
    geoms: &[Option<SolidGeom>],
    faces: &[Face],
    params: &NormalExtrudeParams,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<NormalExtrudeResult, GeomError> {
    let material = params.material.as_deref().map(str::trim).filter(|m| !m.is_empty());
    let mut prisms: Vec<Vec<Side>> = Vec::with_capacity(faces.len());
    for f in faces {
        let source = &brushes[f.brush][f.side];
        let wall = material.unwrap_or(&source.material).to_owned();
        let top = if params.grow { source.material.clone() } else { wall.clone() };
        prisms.push(oblique_prism(f, source, &wall, &top, next_side_id).ok_or(ExtrudeIssue::TooThin)?);
    }
    let walls = wedges(faces, brushes, material, next_side_id);
    let mut out = NormalExtrudeResult::default();
    let mut grown: Vec<(usize, ClipPiece)> = Vec::new();
    for b in brush_order(faces) {
        let mut items: Vec<Vec<Side>> =
            faces.iter().zip(&prisms).filter(|(f, _)| f.brush == b).map(|(_, p)| p.clone()).collect();
        items.extend(
            walls.iter().filter(|(i, j, _)| faces[*i].brush == b && faces[*j].brush == b).map(|(_, _, w)| w.clone()),
        );
        if params.grow
            && let Some(geom) = geoms.get(b).and_then(Option::as_ref)
        {
            let mine: Vec<&Face> = faces.iter().filter(|f| f.brush == b).collect();
            if let Some(g) = grow(brushes[b], geom, &mine, items.clone()) {
                if let Some(piece) = g.changed {
                    grown.push((b, piece));
                }
                out.added.extend(g.extra.into_iter().map(|s| (b, s)));
                continue;
            }
        }
        out.added.extend(merge_pieces(items.into_iter().map(loose).collect()).into_iter().map(|p| (b, p.sides)));
    }
    for (i, j, wedge) in walls {
        let (bi, bj) = (faces[i].brush, faces[j].brush);
        if bi == bj {
            continue;
        }
        let mut placed = false;
        if params.grow {
            for b in [bi, bj] {
                if let Some(slot) = grown.iter_mut().find(|(g, _)| *g == b)
                    && let Some(m) = absorb(&slot.1, &wedge)
                {
                    slot.1 = m;
                    placed = true;
                    break;
                }
            }
        }
        if !placed {
            out.added.push((bi, wedge));
        }
    }
    out.changed = grown;
    for (_, p) in &out.changed {
        check_limits(&p.sides)?;
    }
    for (_, s) in &out.added {
        check_limits(s)?;
    }
    Ok(out)
}

fn carve_in(
    brushes: &[&[Side]],
    geoms: &[Option<SolidGeom>],
    faces: &[Face],
) -> Result<NormalExtrudeResult, GeomError> {
    let mut out = NormalExtrudeResult::default();
    for b in brush_order(faces) {
        let original = brushes[b];
        let geom = geoms.get(b).and_then(Option::as_ref).ok_or(ExtrudeIssue::NotAFace)?;
        let mut sides: Vec<Side> = original.to_vec();
        for s in &mut sides {
            s.disp = None;
        }
        let moved: Vec<(usize, DVec3)> = faces.iter().filter(|f| f.brush == b).map(|f| (f.side, -f.offset)).collect();
        for (i, w) in &moved {
            let s = &mut sides[*i];
            s.plane = s.plane.map(|p| clean_vec(p + *w));
        }
        let g = build_solid(&sides);
        if !g.valid || g.volume() < MIN_WEDGE_VOLUME {
            return Err(ExtrudeIssue::ThroughBrush.into());
        }
        let keep: Vec<usize> = (0..sides.len()).filter(|i| g.face_for_side(*i).is_some()).collect();
        for (j, src) in original.iter().enumerate() {
            let Some(disp) = &src.disp else { continue };
            if !keep.contains(&j) {
                return Err(ExtrudeIssue::DisplacementShape.into());
            }
            let now = g.face_for_side(j).ok_or(ExtrudeIssue::DisplacementShape)?;
            let mut d = disp.clone();
            match moved.iter().find(|(i, _)| *i == j) {
                Some((_, w)) => {
                    if now.vertices.len() != 4 {
                        return Err(ExtrudeIssue::DisplacementShape.into());
                    }
                    let target = d.start_position + *w;
                    let corner = now
                        .vertices
                        .iter()
                        .copied()
                        .min_by(|a, b| a.distance_squared(target).total_cmp(&b.distance_squared(target)))
                        .ok_or(ExtrudeIssue::DisplacementShape)?;
                    d.start_position = clean_vec(corner);
                }
                None => {
                    let old = geom.face_for_side(j).ok_or(ExtrudeIssue::DisplacementShape)?;
                    if !same_polygon(&old.vertices, &now.vertices) {
                        return Err(ExtrudeIssue::DisplacementShape.into());
                    }
                }
            }
            sides[j].disp = Some(d);
        }
        let piece = ClipPiece {
            sides: keep.iter().map(|&i| sides[i].clone()).collect(),
            origins: keep.iter().map(|&i| Some(i)).collect(),
        };
        out.changed.push((b, piece));
    }
    Ok(out)
}

pub fn normal_extrude(
    brushes: &[&[Side]],
    faces: &[NormalExtrudeFace],
    params: &NormalExtrudeParams,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<NormalExtrudeResult, GeomError> {
    let mut list: Vec<NormalExtrudeFace> = Vec::with_capacity(faces.len());
    for f in faces {
        if !list.iter().any(|g| g.brush == f.brush && g.side == f.side) {
            list.push(*f);
        }
    }
    let first = list.first().ok_or(ExtrudeIssue::NotAFace)?;
    if list.iter().any(|f| !f.distance.is_finite() || f.distance.abs() < MIN_EXTRUDE_DEPTH) {
        return Err(ExtrudeIssue::NoDepth.into());
    }
    let carve = first.distance < 0.0;
    if list.iter().any(|f| (f.distance < 0.0) != carve) {
        return Err(ExtrudeIssue::MixedDistances.into());
    }
    let mut geoms: Vec<Option<SolidGeom>> = (0..brushes.len()).map(|_| None).collect();
    let mut data: Vec<Face> = Vec::with_capacity(list.len());
    let mut lengths: Vec<f64> = Vec::with_capacity(list.len());
    for f in &list {
        let sides = brushes.get(f.brush).ok_or(ExtrudeIssue::NotAFace)?;
        if geoms[f.brush].is_none() {
            geoms[f.brush] = Some(validate_solid(sides)?);
        }
        let face = geoms[f.brush].as_ref().and_then(|g| g.face_for_side(f.side)).ok_or(ExtrudeIssue::NotAFace)?;
        let frame = FaceFrame::new(&face.plane);
        let mut ring = face.vertices.clone();
        let flat: Vec<DVec2> = ring.iter().map(|v| frame.to_2d(*v)).collect();
        if signed_area(&flat) < 0.0 {
            ring.reverse();
        }
        data.push(Face { brush: f.brush, side: f.side, ring, normal: face.plane.normal, offset: DVec3::ZERO });
        lengths.push(f.distance.abs());
    }
    let normals: Vec<DVec3> = data.iter().map(|f| f.normal).collect();
    let dirs = extrude_directions(&normals, params.direction)?;
    for ((f, d), len) in data.iter_mut().zip(dirs).zip(lengths) {
        f.offset = d * len;
    }
    if carve { carve_in(brushes, &geoms, &data) } else { extrude_out(brushes, &geoms, &data, params, next_side_id) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hull_solid;
    use crate::test_support::hammer_box;

    fn v3(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x, y, z)
    }

    fn ids(start: u32) -> impl FnMut() -> u32 {
        let mut next = start;
        move || {
            next += 1;
            next
        }
    }

    fn side_with(sides: &[Side], normal: DVec3) -> usize {
        sides
            .iter()
            .position(|s| Plane::from_side(s).is_some_and(|p| p.normal.distance(normal) < 1e-6))
            .expect("side with that normal")
    }

    fn slanted() -> Vec<Side> {
        let pts = [
            v3(0.0, 0.0, 0.0),
            v3(64.0, 0.0, 0.0),
            v3(64.0, 64.0, 0.0),
            v3(0.0, 64.0, 0.0),
            v3(0.0, 0.0, 32.0),
            v3(64.0, 0.0, 32.0),
            v3(64.0, 64.0, 64.0),
            v3(0.0, 64.0, 64.0),
        ];
        hull_solid(&pts, "TEST/SLOPE", 0.25, 16, &mut ids(0)).expect("slanted brush")
    }

    fn vertices(sides: &[Side]) -> Vec<DVec3> {
        let g = build_solid(sides);
        assert!(g.valid, "valid result");
        let mut out: Vec<DVec3> = Vec::new();
        for v in g.vertices() {
            if !out.iter().any(|p| p.distance(v) < 1e-3) {
                out.push(v);
            }
        }
        out
    }

    fn has_all(found: &[DVec3], expected: &[DVec3]) {
        assert_eq!(found.len(), expected.len(), "{found:?}");
        for e in expected {
            assert!(found.iter().any(|v| v.distance(*e) < 1e-3), "missing {e} in {found:?}");
        }
    }

    fn face(brush: usize, side: usize, distance: f64) -> NormalExtrudeFace {
        NormalExtrudeFace { brush, side, distance }
    }

    #[test]
    fn a_slanted_face_moves_along_its_own_normal() {
        let brush = slanted();
        let n = v3(0.0, -1.0, 2.0).normalize();
        let top = side_with(&brush, n);
        let r = normal_extrude(&[&brush], &[face(0, top, 16.0)], &NormalExtrudeParams::default(), &mut ids(100))
            .expect("extrude");
        assert!(r.changed.is_empty());
        assert_eq!(r.added.len(), 1);
        let base = [v3(0.0, 0.0, 32.0), v3(64.0, 0.0, 32.0), v3(64.0, 64.0, 64.0), v3(0.0, 64.0, 64.0)];
        let mut expected: Vec<DVec3> = base.to_vec();
        expected.extend(base.iter().map(|p| *p + n * 16.0));
        has_all(&vertices(&r.added[0].1), &expected);
        let s = &r.added[0].1;
        assert!(s.iter().all(|s| s.material == "TEST/SLOPE"));
        let mut checked = 0;
        for wall in s {
            let p = Plane::from_side(wall).expect("plane");
            if p.normal.dot(n).abs() > 0.5 {
                continue;
            }
            for v in base.iter().filter(|v| p.distance(**v).abs() < 1e-6) {
                let (a, b) = (crate::texel_uv(&brush[top], *v), crate::texel_uv(wall, *v));
                assert!(a.distance(b) < 1e-6, "the texture continues over the edge: {a} {b}");
                checked += 1;
            }
        }
        assert_eq!(checked, 8);
        let g = build_solid(s);
        assert!((g.volume() - 64.0 * (64.0f64 * 64.0 + 32.0 * 32.0).sqrt() * 16.0).abs() < 1e-3);
    }

    #[test]
    fn growing_keeps_the_brush_convex_or_makes_a_new_brush() {
        let brush = hammer_box(v3(0.0, 0.0, 0.0), v3(64.0, 64.0, 32.0));
        let top = side_with(&brush, DVec3::Z);
        let grow = NormalExtrudeParams { grow: true, ..Default::default() };
        let r = normal_extrude(&[&brush], &[face(0, top, 16.0)], &grow, &mut ids(100)).expect("grow");
        assert!(r.added.is_empty());
        assert_eq!(r.changed.len(), 1);
        let piece = &r.changed[0].1;
        assert_eq!(piece.sides.len(), 6);
        assert!(piece.sides.iter().any(|s| s.id == brush[top].id), "the moved face keeps its id");
        assert_eq!(build_solid(&piece.sides).aabb.max.z, 48.0);
        let slope = slanted();
        let n = v3(0.0, -1.0, 2.0).normalize();
        let s = side_with(&slope, n);
        let r = normal_extrude(&[&slope], &[face(0, s, 16.0)], &grow, &mut ids(100)).expect("slope");
        assert!(r.changed.is_empty(), "the bump would not be convex");
        assert_eq!(r.added.len(), 1);
        let up = NormalExtrudeParams {
            grow: true,
            direction: ExtrudeDirection::Axis(ExtrudeAxis::Auto),
            ..Default::default()
        };
        let r = normal_extrude(&[&slope], &[face(0, s, 16.0)], &up, &mut ids(100)).expect("axis");
        assert!(r.added.is_empty());
        assert_eq!(r.changed.len(), 1);
        let expected = [
            v3(0.0, 0.0, 0.0),
            v3(64.0, 0.0, 0.0),
            v3(64.0, 64.0, 0.0),
            v3(0.0, 64.0, 0.0),
            v3(0.0, 0.0, 48.0),
            v3(64.0, 0.0, 48.0),
            v3(64.0, 64.0, 80.0),
            v3(0.0, 64.0, 80.0),
        ];
        has_all(&vertices(&r.changed[0].1.sides), &expected);
    }

    #[test]
    fn several_faces_get_walls_between_them() {
        let brush = hammer_box(v3(0.0, 0.0, 0.0), v3(64.0, 64.0, 32.0));
        let top = side_with(&brush, DVec3::Z);
        let right = side_with(&brush, DVec3::X);
        let faces = [face(0, top, 16.0), face(0, right, 16.0)];
        let r = normal_extrude(&[&brush], &faces, &NormalExtrudeParams::default(), &mut ids(100)).expect("new");
        assert!(r.changed.is_empty());
        let volume: f64 = r.added.iter().map(|(_, s)| build_solid(s).volume()).sum();
        let shell = 64.0 * 64.0 * 16.0 + 32.0 * 64.0 * 16.0 + 0.5 * 16.0 * 16.0 * 64.0;
        assert!((volume - shell).abs() < 1e-3, "{volume} {shell}");
        let grow = NormalExtrudeParams { grow: true, ..Default::default() };
        let r = normal_extrude(&[&brush], &faces, &grow, &mut ids(100)).expect("grow");
        assert!(r.added.is_empty());
        let g = build_solid(&r.changed[0].1.sides);
        assert!((g.volume() - (64.0 * 64.0 * 32.0 + shell)).abs() < 1e-3);
        let ramp_points = [
            v3(64.0, 0.0, 0.0),
            v3(64.0, 64.0, 0.0),
            v3(64.0, 0.0, 32.0),
            v3(64.0, 64.0, 32.0),
            v3(96.0, 0.0, 0.0),
            v3(96.0, 64.0, 0.0),
        ];
        let ramp = hull_solid(&ramp_points, "TEST/RAMP", 0.25, 16, &mut ids(50)).expect("ramp");
        let slope = side_with(&ramp, v3(1.0, 0.0, 1.0).normalize());
        let pair = [face(0, top, 16.0), face(1, slope, 16.0)];
        let r = normal_extrude(&[&brush, &ramp], &pair, &grow, &mut ids(100)).expect("two brushes");
        assert_eq!(r.changed.len(), 2);
        assert!(r.added.is_empty(), "the wall between the brushes joins the ramp");
        let volume: f64 = r.changed.iter().map(|(_, p)| build_solid(&p.sides).volume()).sum();
        let wedge = 0.5 * 16.0 * 16.0 * std::f64::consts::FRAC_1_SQRT_2 * 64.0;
        let total = 64.0 * 64.0 * 48.0 + 512.0 * 64.0 + 32.0 * 2.0f64.sqrt() * 64.0 * 16.0 + wedge;
        assert!((volume - total).abs() < 1e-2, "{volume} {total}");
        let r = normal_extrude(&[&brush, &ramp], &pair, &NormalExtrudeParams::default(), &mut ids(100)).expect("new");
        assert!(r.changed.is_empty());
        let volume: f64 = r.added.iter().map(|(_, s)| build_solid(s).volume()).sum();
        assert!((volume - (total - 64.0 * 64.0 * 32.0 - 512.0 * 64.0)).abs() < 1e-2, "{volume}");
        let avg = NormalExtrudeParams { direction: ExtrudeDirection::Average, ..Default::default() };
        let r = normal_extrude(&[&brush], &faces, &avg, &mut ids(100)).expect("average");
        let volume: f64 = r.added.iter().map(|(_, s)| build_solid(s).volume()).sum();
        let d = 16.0 / 2.0f64.sqrt();
        assert!((volume - (64.0 * 64.0 * d + 32.0 * 64.0 * d)).abs() < 1e-3, "{volume}");
    }

    #[test]
    fn negative_distances_carve_and_bad_requests_fail() {
        let brush = hammer_box(v3(0.0, 0.0, 0.0), v3(64.0, 64.0, 32.0));
        let top = side_with(&brush, DVec3::Z);
        let right = side_with(&brush, DVec3::X);
        let p = NormalExtrudeParams::default();
        let r =
            normal_extrude(&[&brush], &[face(0, top, -8.0), face(0, right, -16.0)], &p, &mut ids(100)).expect("carve");
        assert_eq!(r.changed.len(), 1);
        let g = build_solid(&r.changed[0].1.sides);
        assert_eq!((g.aabb.max.z, g.aabb.max.x), (24.0, 48.0));
        let slope = slanted();
        let n = v3(0.0, -1.0, 2.0).normalize();
        let s = side_with(&slope, n);
        let r = normal_extrude(&[&slope], &[face(0, s, -4.0)], &p, &mut ids(100)).expect("slope carve");
        let piece = &r.changed[0].1;
        let plane = Plane::from_side(&piece.sides[piece.origins.iter().position(|o| *o == Some(s)).expect("kept")])
            .expect("plane");
        let old = Plane::from_side(&slope[s]).expect("old");
        assert!((old.dist - plane.dist - 4.0).abs() < 1e-6);
        assert!(normal_extrude(&[&brush], &[face(0, top, -64.0)], &p, &mut ids(100)).is_err());
        assert!(normal_extrude(&[&brush], &[face(0, top, 8.0), face(0, right, -8.0)], &p, &mut ids(100)).is_err());
        assert!(normal_extrude(&[&brush], &[face(0, top, 0.01)], &p, &mut ids(100)).is_err());
        assert!(normal_extrude(&[&brush], &[face(0, 99, 8.0)], &p, &mut ids(100)).is_err());
        assert!(normal_extrude(&[&brush], &[], &p, &mut ids(100)).is_err());
        let bottom = side_with(&brush, DVec3::NEG_Z);
        let avg = NormalExtrudeParams { direction: ExtrudeDirection::Average, ..Default::default() };
        let e = normal_extrude(&[&brush], &[face(0, top, 8.0), face(0, bottom, 8.0)], &avg, &mut ids(100));
        assert_eq!(e.err().and_then(|e| ExtrudeIssue::of(&e)), Some(ExtrudeIssue::AwayFromDirection));
        let axis = NormalExtrudeParams { direction: ExtrudeDirection::Axis(ExtrudeAxis::Z), ..Default::default() };
        let r = normal_extrude(&[&brush], &[face(0, top, 8.0), face(0, bottom, 8.0)], &axis, &mut ids(100))
            .expect("both ways along z");
        assert_eq!(r.added.len(), 2);
        assert!(normal_extrude(&[&brush], &[face(0, right, 8.0)], &axis, &mut ids(100)).is_err());
    }

    #[test]
    fn displacements_move_with_the_face_when_the_brush_grows() {
        let mut brush = hammer_box(v3(0.0, 0.0, 0.0), v3(64.0, 64.0, 16.0));
        let top = side_with(&brush, DVec3::Z);
        let verts = build_solid(&brush).face_for_side(top).expect("top").vertices.clone();
        let mut disp = crate::create_disp(&verts, 2).expect("disp");
        disp.start_position = v3(0.0, 0.0, 16.0);
        brush[top].disp = Some(Box::new(disp));
        let grow = NormalExtrudeParams { grow: true, ..Default::default() };
        let r = normal_extrude(&[&brush], &[face(0, top, 16.0)], &grow, &mut ids(100)).expect("grow");
        let piece = &r.changed[0].1;
        let moved = piece.sides.iter().find(|s| s.id == brush[top].id).expect("top");
        assert_eq!(moved.disp.as_ref().map(|d| d.start_position), Some(v3(0.0, 0.0, 32.0)));
        assert_eq!(piece.sides.iter().filter(|s| s.disp.is_some()).count(), 1);
        let r = normal_extrude(&[&brush], &[face(0, top, -8.0)], &grow, &mut ids(100)).expect("carve");
        let piece = &r.changed[0].1;
        let moved = piece.sides.iter().find(|s| s.id == brush[top].id).expect("top");
        assert_eq!(moved.disp.as_ref().map(|d| d.start_position), Some(v3(0.0, 0.0, 8.0)));
        let r = normal_extrude(&[&brush], &[face(0, top, 16.0)], &NormalExtrudeParams::default(), &mut ids(100))
            .expect("new");
        assert!(r.added[0].1.iter().all(|s| s.disp.is_none()));
        let right = side_with(&brush, DVec3::X);
        let r = normal_extrude(&[&brush], &[face(0, right, 16.0)], &grow, &mut ids(100)).expect("side");
        assert!(r.changed.is_empty(), "growing would stretch the displacement");
        assert_eq!(r.added.len(), 1);
        assert!(normal_extrude(&[&brush], &[face(0, right, -16.0)], &grow, &mut ids(100)).is_err());
    }

    #[test]
    fn directions_and_ids() {
        let n = [DVec3::Z, DVec3::X];
        assert_eq!(extrude_directions(&n, ExtrudeDirection::EachNormal).expect("each"), n.to_vec());
        let a = extrude_directions(&n, ExtrudeDirection::Average).expect("average");
        assert!(a.iter().all(|v| v.distance(v3(1.0, 0.0, 1.0).normalize()) < 1e-9));
        let z =
            extrude_directions(&[DVec3::NEG_Z, v3(0.0, 0.3, 0.9).normalize()], ExtrudeDirection::Axis(ExtrudeAxis::Z))
                .expect("z");
        assert_eq!(z, vec![DVec3::NEG_Z, DVec3::Z]);
        let auto = extrude_directions(&[v3(0.9, 0.0, 0.3).normalize()], ExtrudeDirection::Axis(ExtrudeAxis::Auto))
            .expect("auto");
        assert_eq!(auto, vec![DVec3::X]);
        for a in ExtrudeAxis::ALL {
            assert_eq!(ExtrudeAxis::from_id(a.id()), Some(a));
        }
        for d in [ExtrudeDirection::EachNormal, ExtrudeDirection::Average, ExtrudeDirection::Axis(ExtrudeAxis::Y)] {
            assert_eq!(ExtrudeDirection::from_ids(d.id(), ExtrudeAxis::Y), Some(d));
        }
        assert_eq!(ExtrudeDirection::from_ids("sideways", ExtrudeAxis::Auto), None);
    }
}
