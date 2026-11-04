use glam::{DVec2, DVec3};
use hp_vmf::Side;

use crate::brush::validate_solid;
use crate::csg::ClipPiece;
use crate::extrude::{FaceFrame, MIN_EXTRUDE_DEPTH, dress, new_side, oriented, signed_area};
use crate::util::clean_vec;
use crate::{GeomError, MAX_COORD, Plane, SolidGeom, build_solid};

pub const MIN_INSET: f64 = 0.125;

pub const MAX_INSET: f64 = 16384.0;

const MARGIN: f64 = 1.0;

const MIN_AREA: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InsetIssue {
    NotAFace,
    Displacement,
    BadDistance,
    TooLarge,
    TooDeep,
    PastLimits,
}

impl InsetIssue {
    pub const ALL: [InsetIssue; 6] = [
        InsetIssue::NotAFace,
        InsetIssue::Displacement,
        InsetIssue::BadDistance,
        InsetIssue::TooLarge,
        InsetIssue::TooDeep,
        InsetIssue::PastLimits,
    ];

    pub fn message(self) -> &'static str {
        match self {
            InsetIssue::NotAFace => "the side is not a face of the brush",
            InsetIssue::Displacement => "the face is a displacement",
            InsetIssue::BadDistance => "the inset distance must be from 0.125 to 16384 units",
            InsetIssue::TooLarge => "the inset distance is larger than the face",
            InsetIssue::TooDeep => "the sink depth is not smaller than the brush thickness",
            InsetIssue::PastLimits => "the result goes past the map limits",
        }
    }

    pub fn of(error: &GeomError) -> Option<Self> {
        match error {
            GeomError::InvalidInput(m) => Self::ALL.into_iter().find(|i| i.message() == m),
            _ => None,
        }
    }
}

impl From<InsetIssue> for GeomError {
    fn from(issue: InsetIssue) -> Self {
        GeomError::InvalidInput(issue.message().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InsetParams {
    pub distance: f64,
    pub depth: f64,
    pub material: Option<String>,
}

impl Default for InsetParams {
    fn default() -> Self {
        Self { distance: 16.0, depth: 0.0, material: None }
    }
}

fn cross(o: DVec2, a: DVec2, b: DVec2) -> f64 {
    (a - o).perp_dot(b - o)
}

pub fn inset_polygon(poly: &[DVec2], distance: f64) -> Vec<DVec2> {
    let mut edges = poly.to_vec();
    if signed_area(&edges) < 0.0 {
        edges.reverse();
    }
    let n = edges.len();
    let mut out = edges.clone();
    for i in 0..n {
        let (a, b) = (edges[i], edges[(i + 1) % n]);
        let len = (b - a).length();
        if len < 1e-9 {
            continue;
        }
        let input = std::mem::take(&mut out);
        let m = input.len();
        for k in 0..m {
            let (p, q) = (input[k], input[(k + 1) % m]);
            let (dp, dq) = (cross(a, b, p) / len - distance, cross(a, b, q) / len - distance);
            if dp >= 0.0 {
                out.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                out.push(p + (q - p) * (dp / (dp - dq)));
            }
        }
        if out.len() < 3 {
            return Vec::new();
        }
    }
    let mut clean: Vec<DVec2> = Vec::with_capacity(out.len());
    for p in out {
        if clean.last().is_none_or(|l| l.distance(p) > 1e-6) {
            clean.push(p);
        }
    }
    while clean.len() > 1 && clean[0].distance(clean[clean.len() - 1]) <= 1e-6 {
        clean.pop();
    }
    let m = clean.len();
    let keep: Vec<DVec2> = (0..m)
        .filter(|&i| {
            let (a, b, c) = (clean[(i + m - 1) % m], clean[i], clean[(i + 1) % m]);
            cross(a, b, c).abs() > 1e-9 * (b - a).length().max(1.0) * (c - b).length().max(1.0)
        })
        .map(|i| clean[i])
        .collect();
    if keep.len() < 3 || signed_area(&keep) < MIN_AREA { Vec::new() } else { keep }
}

pub fn clean_polygon(poly: &[DVec2]) -> Vec<DVec2> {
    let mut pts = poly.to_vec();
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    let mut clean: Vec<DVec2> = Vec::with_capacity(pts.len());
    for p in pts {
        if clean.last().is_none_or(|l| l.distance(p) > 1e-6) {
            clean.push(p);
        }
    }
    while clean.len() > 1 && clean[0].distance(clean[clean.len() - 1]) <= 1e-6 {
        clean.pop();
    }
    let m = clean.len();
    (0..m)
        .filter(|&i| {
            let (a, b, c) = (clean[(i + m - 1) % m], clean[i], clean[(i + 1) % m]);
            cross(a, b, c).abs() > 1e-9 * (b - a).length().max(1.0) * (c - b).length().max(1.0)
        })
        .map(|i| clean[i])
        .collect()
}

pub fn inset_corners(poly: &[DVec2], distance: f64) -> Option<(Vec<DVec2>, Vec<DVec2>)> {
    let outer = clean_polygon(poly);
    let n = outer.len();
    if n < 3 || !distance.is_finite() || distance <= 0.0 {
        return None;
    }
    let inward: Vec<DVec2> =
        (0..n).map(|i| (outer[(i + 1) % n] - outer[i]).try_normalize().map(DVec2::perp)).collect::<Option<_>>()?;
    let mut inner = Vec::with_capacity(n);
    for i in 0..n {
        let (m0, m1) = (inward[(i + n - 1) % n], inward[i]);
        let det = m0.perp_dot(m1);
        if det.abs() < 1e-9 {
            return None;
        }
        let w = DVec2::new(distance * (m1.y - m0.y) / det, distance * (m0.x - m1.x) / det);
        inner.push(outer[i] + w);
    }
    let slack = 1e-6 * distance.max(1.0);
    let fits = inner.iter().all(|p| (0..n).all(|k| inward[k].dot(*p - outer[k]) >= distance - slack));
    if !fits || signed_area(&inner) < MIN_AREA || inset_polygon(&outer, distance).len() != n {
        return None;
    }
    Some((outer, inner))
}

pub fn inset_ring(vertices: &[DVec3], plane: &Plane, distance: f64) -> Option<Vec<[DVec3; 2]>> {
    let frame = FaceFrame::new(plane);
    let flat: Vec<DVec2> = vertices.iter().map(|v| frame.to_2d(*v)).collect();
    let (outer, inner) = inset_corners(&flat, distance)?;
    Some(outer.iter().zip(&inner).map(|(o, i)| [frame.to_3d(*o, 0.0), frame.to_3d(*i, 0.0)]).collect())
}

const MIN_VOLUME: f64 = 1e-3;

struct Cut<'a> {
    index: usize,
    source: &'a Side,
    frame: FaceFrame,
    outer: Vec<DVec2>,
    inner: Vec<DVec2>,
    lo: f64,
    hi: f64,
    depth: f64,
    material: String,
}

impl Cut<'_> {
    fn at(&self, p: DVec2, h: f64) -> DVec3 {
        clean_vec(self.frame.to_3d(p, h))
    }

    fn wall(&self, a: DVec2, b: DVec2, inside: DVec2, id: u32) -> Result<Side, GeomError> {
        let mut away = (b - a).perp();
        if away.dot(inside - a) > 0.0 {
            away = -away;
        }
        let out = self.frame.u * away.x + self.frame.v * away.y;
        let pts = oriented([self.at(a, self.hi), self.at(b, self.hi), self.at(b, self.lo)], out);
        let mut side = new_side(id, pts, self.source);
        if Plane::from_side(&side).is_none() {
            return Err(InsetIssue::TooLarge.into());
        }
        dress(&mut side, self.source, self.frame.normal, &self.material);
        Ok(side)
    }

    fn regions(&self, next: &mut dyn FnMut() -> u32) -> Result<Vec<Vec<Side>>, GeomError> {
        let n = self.outer.len();
        let mut out = Vec::with_capacity(n + 1);
        for j in 0..n {
            let k = (j + 1) % n;
            let (v0, v1, i0, i1) = (self.outer[j], self.outer[k], self.inner[j], self.inner[k]);
            let center = (v0 + v1 + i0 + i1) * 0.25;
            out.push(vec![
                self.wall(i0, i1, center, next())?,
                self.wall(v0, i0, center, next())?,
                self.wall(v1, i1, center, next())?,
            ]);
        }
        let center = self.inner.iter().fold(DVec2::ZERO, |s, p| s + *p) / n as f64;
        let mut block = Vec::with_capacity(n);
        for j in 0..n {
            block.push(self.wall(self.inner[j], self.inner[(j + 1) % n], center, next())?);
        }
        out.push(block);
        Ok(out)
    }

    fn sink_through(&self, piece: &mut ClipPiece, next: &mut dyn FnMut() -> u32) {
        let shift = self.frame.normal * self.depth;
        let pts = [self.outer[0], self.outer[1], self.outer[2]].map(|p| self.at(p, 0.0) + shift);
        let mut floor = new_side(next(), oriented(pts, self.frame.normal), self.source);
        dress(&mut floor, self.source, self.frame.normal, &self.material);
        piece.sides.push(floor);
        piece.origins.push(None);
    }

    fn reshape(&self, cut: &mut ClipPiece, next: &mut dyn FnMut() -> u32) {
        match cut.origins.iter().position(|o| *o == Some(self.index)) {
            Some(t) => {
                let shift = self.frame.normal * self.depth;
                for p in &mut cut.sides[t].plane {
                    *p = clean_vec(*p + shift);
                }
            }
            None if self.depth < 0.0 => self.sink_through(cut, next),
            None => {}
        }
    }

    fn apply(&self, pieces: Vec<ClipPiece>, next: &mut dyn FnMut() -> u32) -> Result<Vec<ClipPiece>, GeomError> {
        let n = self.outer.len();
        let mut out = Vec::with_capacity(pieces.len() * (n + 1));
        for piece in pieces {
            for (r, region) in self.regions(next)?.into_iter().enumerate() {
                let Some(mut cut) = cut_piece(&piece, region) else { continue };
                if r == n && self.depth != 0.0 {
                    self.reshape(&mut cut, next);
                    let g = build_solid(&cut.sides);
                    if !g.valid || g.volume() < MIN_VOLUME {
                        if self.depth > 0.0 {
                            return Err(InsetIssue::TooLarge.into());
                        }
                        continue;
                    }
                    cut = prune(&cut, &g);
                }
                out.push(cut);
            }
        }
        Ok(out)
    }
}

fn prune(piece: &ClipPiece, g: &SolidGeom) -> ClipPiece {
    let keep: Vec<usize> = g.faces.iter().map(|f| f.side_index).collect();
    ClipPiece {
        sides: keep.iter().filter_map(|&i| piece.sides.get(i).cloned()).collect(),
        origins: keep.iter().map(|&i| piece.origins.get(i).copied().flatten()).collect(),
    }
}

fn cut_piece(piece: &ClipPiece, region: Vec<Side>) -> Option<ClipPiece> {
    let mut joined = ClipPiece { sides: piece.sides.clone(), origins: piece.origins.clone() };
    for s in region {
        joined.sides.push(s);
        joined.origins.push(None);
    }
    let g = build_solid(&joined.sides);
    (g.valid && g.volume() >= MIN_VOLUME).then(|| prune(&joined, &g))
}

pub fn faces_inset(
    sides: &[Side],
    side_indices: &[usize],
    params: &InsetParams,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<ClipPiece>, GeomError> {
    if !params.distance.is_finite() || !(MIN_INSET..=MAX_INSET).contains(&params.distance) {
        return Err(InsetIssue::BadDistance.into());
    }
    if !params.depth.is_finite() || params.depth.abs() > MAX_INSET {
        return Err(InsetIssue::BadDistance.into());
    }
    let geom = validate_solid(sides)?;
    let material_override = params.material.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(str::to_owned);
    let mut pieces = vec![ClipPiece::whole(sides)];
    let mut done: Vec<usize> = Vec::with_capacity(side_indices.len());
    for &index in side_indices {
        if done.contains(&index) {
            continue;
        }
        done.push(index);
        let face = geom.face_for_side(index).ok_or(InsetIssue::NotAFace)?;
        let source = sides.get(index).ok_or(InsetIssue::NotAFace)?;
        if source.disp.is_some() {
            return Err(InsetIssue::Displacement.into());
        }
        let frame = FaceFrame::new(&face.plane);
        let flat: Vec<DVec2> = face.vertices.iter().map(|v| frame.to_2d(*v)).collect();
        let (outer, inner) = inset_corners(&flat, params.distance).ok_or(InsetIssue::TooLarge)?;
        let thickness = geom.vertices().map(|v| -face.plane.distance(v)).fold(0.0f64, f64::max);
        if params.depth < 0.0 && -params.depth >= thickness - MIN_EXTRUDE_DEPTH {
            return Err(InsetIssue::TooDeep.into());
        }
        let cut = Cut {
            index,
            source,
            frame,
            outer,
            inner,
            lo: -(thickness + MARGIN),
            hi: MARGIN + params.depth.max(0.0),
            depth: params.depth,
            material: material_override.clone().unwrap_or_else(|| source.material.clone()),
        };
        pieces = cut.apply(pieces, next_side_id)?;
        if pieces.is_empty() {
            return Err(InsetIssue::TooLarge.into());
        }
    }
    for p in &pieces {
        let g = build_solid(&p.sides);
        if g.aabb.min.min_element() < -MAX_COORD || g.aabb.max.max_element() > MAX_COORD {
            return Err(InsetIssue::PastLimits.into());
        }
    }
    Ok(pieces)
}

pub fn face_inset(
    sides: &[Side],
    side_index: usize,
    params: &InsetParams,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<ClipPiece>, GeomError> {
    faces_inset(sides, &[side_index], params, next_side_id)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use glam::DVec3;

    use super::*;
    use crate::Plane;
    use crate::test_support::*;

    fn counter() -> impl FnMut() -> u32 {
        let mut n = 1000;
        move || {
            n += 1;
            n
        }
    }

    fn square(a: f64) -> Vec<DVec2> {
        vec![DVec2::new(0.0, 0.0), DVec2::new(a, 0.0), DVec2::new(a, a), DVec2::new(0.0, a)]
    }

    fn top(sides: &[Side]) -> usize {
        sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == DVec3::Z)).expect("top face")
    }

    fn volume(piece: &ClipPiece) -> f64 {
        let g = build_solid(&piece.sides);
        assert!(g.valid);
        g.volume()
    }

    #[test]
    fn polygon_shrinks_and_vanishes() {
        let p = inset_polygon(&square(64.0), 16.0);
        assert!((signed_area(&p) - 32.0 * 32.0).abs() < 1e-9);
        assert!(inset_polygon(&square(64.0), 32.0).is_empty());
        assert!(inset_polygon(&square(64.0), 40.0).is_empty());
        let cw: Vec<DVec2> = square(64.0).into_iter().rev().collect();
        assert_eq!(inset_polygon(&cw, 16.0).len(), 4);
    }

    #[test]
    fn box_face_gives_five_brushes_with_the_same_volume() {
        let cube = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let pieces =
            face_inset(&cube, top(&cube), &InsetParams { distance: 16.0, ..Default::default() }, &mut counter())
                .expect("inset");
        assert_eq!(pieces.len(), 5);
        let total: f64 = pieces.iter().map(volume).sum();
        assert!((total - 128.0 * 128.0 * 64.0).abs() < 1e-6, "{total}");
        let block = pieces.last().expect("block");
        assert!((volume(block) - 96.0 * 96.0 * 64.0).abs() < 1e-6);
        let mut ids: Vec<u32> = pieces.iter().flat_map(|p| p.sides.iter().map(|s| s.id)).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert!(n - ids.len() <= pieces.len() * 6);
        for p in &pieces {
            let fresh: Vec<&Side> =
                p.sides.iter().zip(&p.origins).filter(|(_, o)| o.is_none()).map(|(s, _)| s).collect();
            assert!(!fresh.is_empty());
            assert!(fresh.iter().all(|s| s.material == cube[top(&cube)].material));
        }
    }

    #[test]
    fn depth_raises_or_sinks_the_inner_block() {
        let cube = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let up = InsetParams { distance: 16.0, depth: 8.0, material: Some("DEV/TRIM".to_owned()) };
        let pieces = face_inset(&cube, top(&cube), &up, &mut counter()).expect("raise");
        assert_eq!(pieces.len(), 5);
        let block = pieces.last().expect("block");
        assert!((volume(block) - 96.0 * 96.0 * 72.0).abs() < 1e-6);
        assert!(block.sides.iter().any(|s| s.material == "DEV/TRIM"));
        let down = InsetParams { distance: 16.0, depth: -8.0, material: None };
        let pieces = face_inset(&cube, top(&cube), &down, &mut counter()).expect("sink");
        assert!((volume(pieces.last().expect("block")) - 96.0 * 96.0 * 56.0).abs() < 1e-6);
        let deep = InsetParams { distance: 16.0, depth: -64.0, material: None };
        assert!(face_inset(&cube, top(&cube), &deep, &mut counter()).is_err());
    }

    #[test]
    fn refuses_bad_requests() {
        let cube = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let t = top(&cube);
        for distance in [0.0, 64.0, 80.0, f64::NAN] {
            let p = InsetParams { distance, ..Default::default() };
            assert!(face_inset(&cube, t, &p, &mut counter()).is_err(), "{distance}");
        }
        assert!(face_inset(&cube, 99, &InsetParams::default(), &mut counter()).is_err());
        let err = face_inset(&cube, t, &InsetParams { distance: 64.0, ..Default::default() }, &mut counter());
        assert_eq!(err.err().and_then(|e| InsetIssue::of(&e)), Some(InsetIssue::TooLarge));
    }

    fn prism_of(points: &[DVec2], height: f64) -> Vec<Side> {
        let mut pts: Vec<DVec3> = points.iter().map(|p| DVec3::new(p.x, p.y, 0.0)).collect();
        pts.extend(points.iter().map(|p| DVec3::new(p.x, p.y, height)));
        crate::hull_solid(&pts, "TEST/PRISM", 0.25, 16, &mut counter()).expect("prism")
    }

    fn side_facing(sides: &[Side], normal: DVec3) -> usize {
        sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal.dot(normal) > 0.999)).expect("side")
    }

    fn top_vertices(piece: &ClipPiece, top: usize) -> Option<usize> {
        let g = build_solid(&piece.sides);
        let i = piece.origins.iter().position(|o| *o == Some(top))?;
        g.face_for_side(i).map(|f| f.vertices.len())
    }

    fn regular(n: usize, radius: f64) -> Vec<DVec2> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                DVec2::new((radius * a.cos()).round(), (radius * a.sin()).round())
            })
            .collect()
    }

    fn check_quad_frame(sides: &[Side], distance: f64, edges: usize) {
        let top = side_facing(sides, DVec3::Z);
        let before = volume(&ClipPiece::whole(sides));
        let params = InsetParams { distance, ..Default::default() };
        let pieces = face_inset(sides, top, &params, &mut counter()).expect("inset");
        assert_eq!(pieces.len(), edges + 1);
        let (frame, block) = pieces.split_at(edges);
        for p in frame {
            assert_eq!(top_vertices(p, top), Some(4), "every frame piece shows one quad");
        }
        assert_eq!(top_vertices(&block[0], top), Some(edges), "the inner face keeps the shape");
        let total: f64 = pieces.iter().map(volume).sum();
        assert!((total - before).abs() < 1e-6 * before, "{total} {before}");
    }

    #[test]
    fn a_triangle_gives_three_quads() {
        let tri = prism_of(&[DVec2::new(0.0, 0.0), DVec2::new(128.0, 0.0), DVec2::new(0.0, 128.0)], 64.0);
        check_quad_frame(&tri, 12.0, 3);
    }

    #[test]
    fn a_box_gives_four_quads() {
        check_quad_frame(&hammer_box(DVec3::ZERO, DVec3::new(128.0, 96.0, 64.0)), 16.0, 4);
    }

    #[test]
    fn n_gons_give_one_quad_per_edge() {
        for n in [5, 6, 8, 12] {
            check_quad_frame(&prism_of(&regular(n, 160.0), 48.0), 10.0, n);
        }
    }

    #[test]
    fn a_vanishing_edge_is_refused() {
        let trap = prism_of(
            &[DVec2::new(0.0, 0.0), DVec2::new(128.0, 0.0), DVec2::new(72.0, 32.0), DVec2::new(56.0, 32.0)],
            64.0,
        );
        let top = side_facing(&trap, DVec3::Z);
        let params = InsetParams { distance: 10.0, ..Default::default() };
        assert_eq!(face_inset(&trap, top, &params, &mut counter()).map(|p| p.len()).map_err(|e| e.to_string()), Ok(5));
        let params = InsetParams { distance: 24.0, ..Default::default() };
        let err = face_inset(&trap, top, &params, &mut counter()).err();
        assert_eq!(err.and_then(|e| InsetIssue::of(&e)), Some(InsetIssue::TooLarge));
    }

    #[test]
    fn two_faces_of_one_brush_tile_the_brush() {
        let cube = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let faces = [side_facing(&cube, DVec3::Z), side_facing(&cube, DVec3::NEG_Y)];
        let params = InsetParams { distance: 12.0, ..Default::default() };
        let pieces = faces_inset(&cube, &faces, &params, &mut counter()).expect("inset");
        let total: f64 = pieces.iter().map(volume).sum();
        assert!((total - 128.0 * 128.0 * 64.0).abs() < 1e-6, "{total}");
        assert!(pieces.len() > 5);
        let all: Vec<usize> = (0..cube.len()).collect();
        let pieces = faces_inset(&cube, &all, &params, &mut counter()).expect("all faces");
        let total: f64 = pieces.iter().map(volume).sum();
        assert!((total - 128.0 * 128.0 * 64.0).abs() < 1e-6, "{total}");
    }

    #[test]
    fn two_faces_with_depth_keep_every_piece_valid() {
        let cube = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let faces = [side_facing(&cube, DVec3::Z), side_facing(&cube, DVec3::NEG_Y)];
        for depth in [6.0, -6.0] {
            let params = InsetParams { distance: 12.0, depth, material: None };
            let pieces = faces_inset(&cube, &faces, &params, &mut counter()).expect("inset");
            assert!(pieces.iter().all(|p| build_solid(&p.sides).valid));
        }
    }

    #[test]
    fn a_sink_deeper_than_the_inner_pieces_still_cuts_the_hole() {
        let cube = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let faces = [side_facing(&cube, DVec3::Z), side_facing(&cube, DVec3::NEG_Y)];
        let params = InsetParams { distance: 12.0, depth: -20.0, material: None };
        let pieces = faces_inset(&cube, &faces, &params, &mut counter()).expect("inset");
        let total: f64 = pieces.iter().map(volume).sum();
        assert!(total < 128.0 * 128.0 * 64.0 - 1.0);
        assert!(pieces.iter().all(|p| build_solid(&p.sides).valid));
    }
}
