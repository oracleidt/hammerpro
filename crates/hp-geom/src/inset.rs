use glam::DVec2;
use hp_vmf::Side;

use crate::brush::validate_solid;
use crate::csg::ClipPiece;
use crate::extrude::{
    ExtrudeMode, FaceExtrudeParams, FaceFrame, MIN_EXTRUDE_DEPTH, dress, face_extrude, prism, signed_area,
};
use crate::util::clean_vec;
use crate::{GeomError, MAX_COORD, build_solid};

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

pub fn face_inset(
    sides: &[Side],
    side_index: usize,
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
    let face = geom.face_for_side(side_index).ok_or(InsetIssue::NotAFace)?;
    let source = sides.get(side_index).ok_or(InsetIssue::NotAFace)?;
    if source.disp.is_some() {
        return Err(InsetIssue::Displacement.into());
    }
    let plane = face.plane;
    let frame = FaceFrame::new(&plane);
    let flat: Vec<DVec2> = face.vertices.iter().map(|v| frame.to_2d(*v)).collect();
    let inner = inset_polygon(&flat, params.distance);
    if inner.is_empty() {
        return Err(InsetIssue::TooLarge.into());
    }
    let thickness = geom.vertices().map(|v| -plane.distance(v)).fold(0.0f64, f64::max);
    if params.depth < 0.0 && -params.depth >= thickness - MIN_EXTRUDE_DEPTH {
        return Err(InsetIssue::TooDeep.into());
    }
    let ring: Vec<_> = inner.iter().map(|p| clean_vec(frame.to_3d(*p, 0.0))).collect();
    let material = params
        .material
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map_or_else(|| source.material.clone(), str::to_owned);

    let cut = FaceExtrudeParams {
        mode: ExtrudeMode::Carve,
        depth: thickness.max(MIN_EXTRUDE_DEPTH),
        through: true,
        material: Some(material.clone()),
    };
    let mut result = face_extrude(sides, side_index, &ring, &cut, next_side_id)?;
    let walls =
        prism(&ring, plane.normal, -(thickness + MARGIN), MARGIN, source, next_side_id).ok_or(InsetIssue::TooLarge)?;
    let lateral = walls.len().saturating_sub(2);
    let mut all: Vec<Side> = sides.to_vec();
    let mut origins: Vec<Option<usize>> = (0..sides.len()).map(Some).collect();
    for mut s in walls.into_iter().take(lateral) {
        dress(&mut s, source, plane.normal, &material);
        all.push(s);
        origins.push(None);
    }
    let live = build_solid(&all);
    if !live.valid {
        return Err(InsetIssue::TooLarge.into());
    }
    let keep: Vec<usize> = live.faces.iter().map(|f| f.side_index).collect();
    let mut block = ClipPiece {
        sides: keep.iter().filter_map(|&i| all.get(i).cloned()).collect(),
        origins: keep.iter().filter_map(|&i| origins.get(i).copied()).collect(),
    };
    if params.depth != 0.0 {
        let top = block.origins.iter().position(|o| *o == Some(side_index)).ok_or(InsetIssue::NotAFace)?;
        let shift = plane.normal * params.depth;
        for p in &mut block.sides[top].plane {
            *p = clean_vec(*p + shift);
        }
        let g = build_solid(&block.sides);
        if !g.valid {
            return Err(InsetIssue::TooDeep.into());
        }
        if g.aabb.min.min_element() < -MAX_COORD || g.aabb.max.max_element() > MAX_COORD {
            return Err(InsetIssue::PastLimits.into());
        }
    }
    result.pieces.push(block);
    Ok(result.pieces)
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
}
