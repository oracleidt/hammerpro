use glam::DVec3;
use hp_vmf::{Side, TexAxis};
use rayon::prelude::*;

use crate::brush::{BASE_POLYGON_HALF_SIZE, base_polygon, validate_solid};
use crate::texture::{effective_scale, world_axes};
use crate::util::{clean_vec, dedupe_ring, plane_points_from_polygon};
use crate::{Aabb, CLIP_EPS, GeomError, PLANE_EPS, Plane, SolidGeom, WELD_EPS, build_solid, rebuild_from_points};

type SplitResult = (Option<Vec<Side>>, Option<Vec<Side>>);

pub const MIN_CLIP_PIECE: f64 = 0.25;

pub fn plane_splits(geom: &SolidGeom, plane: &Plane) -> bool {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in geom.vertices() {
        let d = plane.distance(v);
        lo = lo.min(d);
        hi = hi.max(d);
    }
    geom.valid && hi >= MIN_CLIP_PIECE && -lo >= MIN_CLIP_PIECE
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipPiece {
    pub sides: Vec<Side>,
    pub origins: Vec<Option<usize>>,
}

impl ClipPiece {
    pub fn whole(sides: &[Side]) -> Self {
        Self { sides: sides.to_vec(), origins: (0..sides.len()).map(Some).collect() }
    }

    #[must_use]
    pub fn compose(mut self, parent: &[Option<usize>]) -> Self {
        for o in &mut self.origins {
            *o = o.and_then(|j| parent.get(j).copied().flatten());
        }
        self
    }
}

pub fn clip_solid(
    sides: &[Side],
    plane: &Plane,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<SplitResult, GeomError> {
    let (front, back) = clip_solid_traced(sides, plane, next_side_id)?;
    Ok((front.map(|p| p.sides), back.map(|p| p.sides)))
}

pub fn clip_solid_traced(
    sides: &[Side],
    plane: &Plane,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<(Option<ClipPiece>, Option<ClipPiece>), GeomError> {
    let geom = validate_solid(sides)?;
    clip_geom(sides, &geom, plane, None, next_side_id)
}

pub(crate) fn clip_geom(
    sides: &[Side],
    geom: &SolidGeom,
    plane: &Plane,
    cap_template: Option<&Side>,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<(Option<ClipPiece>, Option<ClipPiece>), GeomError> {
    let plane = Plane::new(plane.normal, plane.dist).ok_or(GeomError::DegeneratePlane)?;
    let (mut min_d, mut max_d) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in geom.vertices() {
        let d = plane.distance(v);
        min_d = min_d.min(d);
        max_d = max_d.max(d);
    }
    if max_d <= PLANE_EPS {
        return Ok((None, Some(ClipPiece::whole(sides))));
    }
    if min_d >= -PLANE_EPS {
        return Ok((Some(ClipPiece::whole(sides)), None));
    }
    let mut poly: Vec<DVec3> = base_polygon(&plane, BASE_POLYGON_HALF_SIZE).to_vec();
    for f in &geom.faces {
        let (_, back) = crate::clip_polygon(&poly, &f.plane, CLIP_EPS);
        poly = back;
        if poly.len() < 3 {
            break;
        }
    }
    dedupe_ring(&mut poly, WELD_EPS);
    let poly: Vec<DVec3> = poly.into_iter().map(clean_vec).collect();
    let back_pts = plane_points_from_polygon(&poly).unwrap_or_else(|| plane.to_points());
    let front_pts = [back_pts[2], back_pts[1], back_pts[0]];
    let back = make_piece(sides, geom, &plane, back_pts, cap_template, true, next_side_id);
    let front = make_piece(sides, geom, &plane.flip(), front_pts, cap_template, false, next_side_id);
    Ok((front, back))
}

fn make_piece(
    sides: &[Side],
    geom: &SolidGeom,
    cap: &Plane,
    cap_points: [DVec3; 3],
    cap_template: Option<&Side>,
    keep_ids: bool,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Option<ClipPiece> {
    let mut out: Vec<Side> = Vec::with_capacity(geom.faces.len() + 1);
    let mut origins: Vec<Option<usize>> = Vec::with_capacity(geom.faces.len() + 1);
    let mut best: Option<(f64, usize)> = None;
    for f in &geom.faces {
        if !f.vertices.iter().any(|&v| cap.distance(v) < -PLANE_EPS) {
            continue;
        }
        let Some(src) = sides.get(f.side_index) else { continue };
        let untouched = f.vertices.iter().all(|&v| cap.distance(v) <= PLANE_EPS);
        let mut s = src.clone();
        if !keep_ids {
            s.id = next_side_id();
        }
        if !untouched {
            s.disp = None;
            s.vertices_plus = None;
        }
        let d = f.plane.normal.dot(cap.normal);
        if best.is_none_or(|(bd, _)| d > bd + 1e-9) {
            best = Some((d, out.len()));
        }
        out.push(s);
        origins.push(Some(f.side_index));
    }
    if out.len() < 3 {
        return None;
    }
    let template = match cap_template {
        Some(t) => t.clone(),
        None => out.get(best?.1)?.clone(),
    };
    out.push(cap_side(cap_points, cap.normal, &template, next_side_id()));
    origins.push(None);
    build_solid(&out).valid.then_some(ClipPiece { sides: out, origins })
}

fn cap_side(points: [DVec3; 3], normal: DVec3, template: &Side, id: u32) -> Side {
    let parallel = Plane::from_side(template).is_some_and(|p| p.normal.dot(normal) > 1.0 - 1e-9);
    let (uaxis, vaxis, rotation) = if parallel {
        (template.uaxis, template.vaxis, template.rotation)
    } else {
        let (u, v) = world_axes(normal);
        (
            TexAxis { axis: u, shift: 0.0, scale: effective_scale(template.uaxis.scale) },
            TexAxis { axis: v, shift: 0.0, scale: effective_scale(template.vaxis.scale) },
            0.0,
        )
    };
    Side {
        id,
        plane: points,
        material: template.material.clone(),
        uaxis,
        vaxis,
        rotation,
        lightmap_scale: template.lightmap_scale,
        smoothing_groups: template.smoothing_groups,
        disp: None,
        vertices_plus: None,
        extra_props: Vec::new(),
        extra_blocks: Vec::new(),
    }
}

pub fn carve(
    target: &[Side],
    carver: &[Side],
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<Vec<Side>>, GeomError> {
    Ok(carve_traced(target, carver, next_side_id)?.into_iter().map(|p| p.sides).collect())
}

pub fn carve_traced(
    target: &[Side],
    carver: &[Side],
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<ClipPiece>, GeomError> {
    carve_impl(target, carver, false, next_side_id)
}

pub(crate) fn carve_impl(
    target: &[Side],
    carver: &[Side],
    cap_from_carver: bool,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<ClipPiece>, GeomError> {
    let cg = validate_solid(carver)?;
    carve_with_geom(target, carver, &cg, cap_from_carver, next_side_id)
}

pub(crate) fn carve_with_geom(
    target: &[Side],
    carver: &[Side],
    cg: &SolidGeom,
    cap_from_carver: bool,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<ClipPiece>, GeomError> {
    let tg = validate_solid(target)?;
    let overlap = tg.aabb.intersection(&cg.aabb);
    let size = overlap.size();
    if overlap.is_empty() || size.min_element() <= PLANE_EPS {
        return Ok(vec![ClipPiece::whole(target)]);
    }
    let mut pieces = Vec::new();
    let mut remaining = ClipPiece::whole(target);
    let mut remaining_geom = tg;
    for face in &cg.faces {
        let template = if cap_from_carver { carver.get(face.side_index) } else { None };
        let (front, back) = clip_geom(&remaining.sides, &remaining_geom, &face.plane, template, next_side_id)?;
        match (front, back) {
            (Some(f), Some(b)) => {
                pieces.push(f.compose(&remaining.origins));
                remaining = b.compose(&remaining.origins);
                remaining_geom = build_solid(&remaining.sides);
                if !remaining_geom.valid {
                    return Ok(pieces);
                }
            }
            (None, Some(_)) => {}
            (Some(_), None) | (None, None) => return Ok(vec![ClipPiece::whole(target)]),
        }
    }
    Ok(pieces)
}

pub fn carve_many(
    targets: &[(&[Side], Aabb)],
    carvers: &[(&[Side], Aabb)],
    next_side_id: &mut dyn FnMut() -> u32,
) -> Vec<Option<Vec<ClipPiece>>> {
    let cutters: Vec<(&[Side], Aabb, SolidGeom)> =
        carvers.par_iter().filter_map(|(sides, b)| validate_solid(sides).ok().map(|g| (*sides, *b, g))).collect();
    let base = targets
        .iter()
        .flat_map(|(s, _)| s.iter())
        .chain(cutters.iter().flat_map(|(s, _, _)| s.iter()))
        .map(|s| s.id)
        .max()
        .map_or(1, |m| m.saturating_add(1));
    let carved: Vec<Option<Vec<ClipPiece>>> = targets
        .par_iter()
        .map(|(original, tb)| {
            let mut local = base;
            let mut fresh = || {
                let id = local;
                local = local.saturating_add(1);
                id
            };
            let mut pieces = vec![ClipPiece::whole(original)];
            let mut touched = false;
            for (cs, cb, cg) in &cutters {
                let overlap = tb.intersection(cb);
                if overlap.is_empty() || overlap.size().min_element() <= PLANE_EPS {
                    continue;
                }
                let mut next_pieces = Vec::with_capacity(pieces.len() + 4);
                for p in pieces {
                    match carve_with_geom(&p.sides, cs, cg, false, &mut fresh) {
                        Ok(v) => {
                            if v.len() != 1 || v.first().map(|q| &q.sides) != Some(&p.sides) {
                                touched = true;
                            }
                            next_pieces.extend(v.into_iter().map(|q| q.compose(&p.origins)));
                        }
                        Err(_) => next_pieces.push(p),
                    }
                }
                pieces = next_pieces;
            }
            touched.then_some(pieces)
        })
        .collect();
    carved
        .into_iter()
        .map(|pieces| {
            pieces.map(|mut pieces| {
                for s in pieces.iter_mut().flat_map(|p| p.sides.iter_mut()) {
                    if s.id >= base {
                        s.id = next_side_id();
                    }
                }
                pieces
            })
        })
        .collect()
}

pub fn offset_side(side: &Side, offset: f64) -> Option<Side> {
    if !offset.is_finite() {
        return None;
    }
    let plane = Plane::from_side(side)?;
    let d = plane.normal * offset;
    let mut s = side.clone();
    s.plane = side.plane.map(|p| clean_vec(p + d));
    s.disp = None;
    s.vertices_plus = None;
    Some(s)
}

pub fn hollow(
    sides: &[Side],
    wall_thickness: f64,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<Vec<Side>>, GeomError> {
    Ok(hollow_traced(sides, wall_thickness, next_side_id)?.into_iter().map(|p| p.sides).collect())
}

pub fn hollow_traced(
    sides: &[Side],
    wall_thickness: f64,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<ClipPiece>, GeomError> {
    if !wall_thickness.is_finite() || wall_thickness.abs() < WELD_EPS {
        return Err(GeomError::InvalidInput(format!("invalid wall thickness {wall_thickness}")));
    }
    let geom = validate_solid(sides)?;
    let contributing: Vec<(usize, &Side)> =
        geom.faces.iter().filter_map(|f| sides.get(f.side_index).map(|s| (f.side_index, s))).collect();
    if wall_thickness > 0.0 {
        let inner: Vec<Side> = contributing.iter().filter_map(|(_, s)| offset_side(s, -wall_thickness)).collect();
        let ig = build_solid(&inner);
        if !ig.valid || inner.len() != contributing.len() {
            return Err(GeomError::WallTooThick);
        }
        let inside = ig.vertices().all(|v| geom.faces.iter().all(|f| f.plane.distance(v) < PLANE_EPS));
        if !inside {
            return Err(GeomError::WallTooThick);
        }
        carve_impl(sides, &inner, true, next_side_id)
    } else {
        let mut outer: Vec<Side> = Vec::with_capacity(contributing.len());
        let mut outer_src: Vec<Option<usize>> = Vec::with_capacity(contributing.len());
        for (i, s) in &contributing {
            if let Some(o) = offset_side(s, -wall_thickness) {
                outer.push(o);
                outer_src.push(Some(*i));
            }
        }
        validate_solid(&outer)?;
        let original: Vec<Side> = contributing.into_iter().map(|(_, s)| s.clone()).collect();
        let walls = carve_impl(&outer, &original, true, next_side_id)?;
        Ok(walls.into_iter().map(|w| w.compose(&outer_src)).collect())
    }
}

pub fn merge_convex(solids: &[Vec<Side>], next_side_id: &mut dyn FnMut() -> u32) -> Result<Vec<Side>, GeomError> {
    if solids.is_empty() {
        return Err(GeomError::InvalidInput("no solids to merge".to_owned()));
    }
    let mut points = Vec::new();
    let mut templates = Vec::new();
    for s in solids {
        let g = validate_solid(s)?;
        points.extend(g.vertices());
        templates.extend(s.iter().cloned());
    }
    rebuild_from_points(&points, &templates, next_side_id)
}

pub fn union_volume(solids: &[Vec<Side>]) -> Result<f64, GeomError> {
    let geoms: Vec<SolidGeom> = solids.iter().map(|s| validate_solid(s)).collect::<Result<_, _>>()?;
    let mut scratch = 0u32;
    let mut next = || {
        scratch = scratch.wrapping_add(1);
        scratch
    };
    let mut total = 0.0;
    for (k, (sides, geom)) in solids.iter().zip(&geoms).enumerate() {
        let mut pieces: Vec<Vec<Side>> = vec![sides.clone()];
        for (earlier, eg) in solids.iter().zip(&geoms).take(k) {
            if pieces.is_empty() {
                break;
            }
            let overlap = geom.aabb.intersection(&eg.aabb);
            if overlap.is_empty() || overlap.size().min_element() <= PLANE_EPS {
                continue;
            }
            let mut rest = Vec::with_capacity(pieces.len() + 4);
            for p in pieces {
                rest.extend(carve_impl(&p, earlier, false, &mut next)?.into_iter().map(|c| c.sides));
            }
            pieces = rest;
        }
        total += pieces.iter().map(|p| build_solid(p).volume()).sum::<f64>();
    }
    Ok(total)
}

pub fn merge_exact(
    solids: &[Vec<Side>],
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Option<Vec<Side>>, GeomError> {
    let merged = merge_convex(solids, next_side_id)?;
    let hull = build_solid(&merged);
    if !hull.valid {
        return Err(GeomError::NoVolume);
    }
    let hull_volume = hull.volume();
    let area: f64 = hull.faces.iter().map(|f| crate::polygon_area_vector(&f.vertices).length()).sum();
    let tolerance = PLANE_EPS * area + 1e-9 * hull_volume;
    let sum: f64 = solids.iter().map(|s| build_solid(s).volume()).sum();
    if hull_volume - sum > tolerance {
        return Ok(None);
    }
    let geoms: Vec<SolidGeom> = solids.iter().map(|s| build_solid(s)).collect();
    if !hull_is_covered(&hull, &geoms) {
        return Ok(None);
    }
    let union = union_volume(solids)?;
    if hull_volume - union > tolerance {
        return Ok(None);
    }
    Ok(hull_left_over_is_thin(&merged, solids).then_some(merged))
}

const COVER_EPS: f64 = PLANE_EPS * 2.0;

fn hull_left_over_is_thin(hull: &[Side], parts: &[Vec<Side>]) -> bool {
    let mut scratch = 0u32;
    let mut next = || {
        scratch = scratch.wrapping_add(1);
        scratch
    };
    let mut left: Vec<Vec<Side>> = vec![hull.to_vec()];
    for part in parts {
        let Ok(cg) = validate_solid(part) else { return false };
        let mut rest = Vec::with_capacity(left.len() + 4);
        for piece in left {
            let g = build_solid(&piece);
            if !g.valid || g.volume() <= 1e-9 {
                continue;
            }
            match carve_with_geom(&piece, part, &cg, false, &mut next) {
                Ok(pieces) => rest.extend(pieces.into_iter().map(|p| p.sides)),
                Err(_) => {
                    if 2.0 * g.volume() > COVER_EPS * g.surface_area() {
                        return false;
                    }
                }
            }
        }
        left = rest;
    }
    left.iter().all(|piece| {
        let g = build_solid(piece);
        !g.valid || 2.0 * g.volume() <= COVER_EPS * g.surface_area()
    })
}

fn outside(geom: &SolidGeom, p: DVec3) -> f64 {
    geom.faces.iter().map(|f| f.plane.distance(p)).fold(f64::NEG_INFINITY, f64::max)
}

fn hull_is_covered(hull: &SolidGeom, parts: &[SolidGeom]) -> bool {
    let covered = |p: DVec3| parts.iter().any(|g| g.valid && outside(g, p) <= COVER_EPS);
    hull.faces.iter().all(|f| {
        let n = f.vertices.len();
        if n < 3 {
            return true;
        }
        let c = f.vertices.iter().copied().sum::<DVec3>() / n as f64;
        if !covered(c) {
            return false;
        }
        (0..n).all(|i| {
            let a = f.vertices[i];
            let b = f.vertices[(i + 1) % n];
            covered((a + b) * 0.5) && covered((a + b + c) / 3.0) && covered((a * 2.0 + c) / 3.0)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn volume(sides: &[Side]) -> f64 {
        let g = build_solid(sides);
        assert!(g.valid, "piece must be valid");
        g.volume()
    }

    fn total(pieces: &[Vec<Side>]) -> f64 {
        pieces.iter().map(|p| volume(p)).sum()
    }

    fn unique_ids(pieces: &[Vec<Side>]) -> bool {
        let mut ids: Vec<u32> = pieces.iter().flatten().map(|s| s.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        ids.len() == n
    }

    #[test]
    fn plane_splits_ignores_slivers() {
        let geom = build_solid(&hammer_box(DVec3::ZERO, DVec3::new(128.0, 64.0, 32.0)));
        let at = |x: f64| Plane::new(DVec3::X, x).expect("plane");
        assert!(plane_splits(&geom, &at(64.0)));
        assert!(plane_splits(&geom, &at(MIN_CLIP_PIECE)));
        assert!(!plane_splits(&geom, &at(0.1)));
        assert!(!plane_splits(&geom, &at(127.9)));
        assert!(!plane_splits(&geom, &at(-10.0)));
        assert!(!plane_splits(&SolidGeom::default(), &at(64.0)));
    }

    #[test]
    fn clip_box_diagonally() {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(128.0, 64.0, 32.0));
        let mut next = id_counter(1000);
        let plane = Plane::from_normal_point(DVec3::new(1.0, 1.0, 0.0), DVec3::new(64.0, 32.0, 0.0)).expect("plane");
        let (front, back) = clip_solid(&sides, &plane, &mut next).expect("clip");
        let (front, back) = (front.expect("front"), back.expect("back"));
        let (vf, vb) = (volume(&front), volume(&back));
        assert!((vf + vb - 128.0 * 64.0 * 32.0).abs() < 1e-6);
        assert!(vf > 0.0 && vb > 0.0);
        assert!(back.iter().filter(|s| s.id < 1000).count() >= 3);
        assert!(front.iter().all(|s| s.id > 1000));
        assert!(unique_ids(&[front.clone(), back.clone()]));
        let cap = back.last().expect("cap");
        let cp = Plane::from_side(cap).expect("cap plane");
        assert!(cp.approx_eq(&plane, 1e-9, 1e-6));
        assert!(cap.material.starts_with("TEST/"));
        for p in cap.plane {
            assert_eq!(p, p.round());
        }
        let fcap = front.last().expect("cap");
        assert!(Plane::from_side(fcap).expect("p").approx_eq(&plane.flip(), 1e-9, 1e-6));
    }

    #[test]
    fn traced_clip_reports_side_origins() {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(128.0, 64.0, 32.0));
        let mut next = id_counter(1000);
        let plane = Plane::new(DVec3::X, 64.0).expect("plane");
        let (front, back) = clip_solid_traced(&sides, &plane, &mut next).expect("clip");
        let (front, back) = (front.expect("front"), back.expect("back"));
        for piece in [&front, &back] {
            assert_eq!(piece.sides.len(), piece.origins.len());
            assert_eq!(piece.origins.last(), Some(&None), "the cap is new");
            for (s, o) in piece.sides.iter().zip(&piece.origins).filter(|(_, o)| o.is_some()) {
                let src = &sides[o.expect("origin")];
                assert_eq!(s.plane, src.plane, "a kept face stays on its plane");
            }
        }
        for (s, o) in back.sides.iter().zip(&back.origins) {
            if let Some(o) = o {
                assert_eq!(s.id, sides[*o].id);
            }
        }
        assert!(front.sides.iter().all(|s| s.id >= 1000));
        assert!(front.origins.contains(&Some(0)) && back.origins.contains(&Some(0)));
        let (f, b) = clip_solid_traced(&sides, &Plane::new(DVec3::X, 500.0).expect("p"), &mut next).expect("clip");
        assert!(f.is_none());
        assert_eq!(b.expect("back").origins, (0..6).map(Some).collect::<Vec<_>>());
    }

    #[test]
    fn clip_miss_and_touch() {
        let sides = hammer_box(DVec3::ZERO, DVec3::splat(64.0));
        let mut next = id_counter(1000);
        let above = Plane::new(DVec3::Z, 100.0).expect("p");
        let (f, b) = clip_solid(&sides, &above, &mut next).expect("clip");
        assert!(f.is_none());
        assert_eq!(b.as_deref(), Some(&sides[..]));
        let touch = Plane::new(DVec3::Z, 64.0).expect("p");
        let (f, b) = clip_solid(&sides, &touch, &mut next).expect("clip");
        assert!(f.is_none() && b.is_some());
        let below = Plane::new(DVec3::Z, -1.0).expect("p");
        let (f, b) = clip_solid(&sides, &below, &mut next).expect("clip");
        assert!(f.is_some() && b.is_none());
        let bad = Plane { normal: DVec3::ZERO, dist: 0.0 };
        assert_eq!(clip_solid(&sides, &bad, &mut next), Err(GeomError::DegeneratePlane));
    }

    #[test]
    fn carve_cases() {
        let target = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let mut next = id_counter(1000);
        let carver = hammer_box(DVec3::new(32.0, 32.0, 32.0), DVec3::new(96.0, 96.0, 128.0));
        let pieces = carve(&target, &carver, &mut next).expect("carve");
        assert!(pieces.len() >= 2);
        let expected = 128.0 * 128.0 * 64.0 - 64.0 * 64.0 * 32.0;
        assert!((total(&pieces) - expected).abs() < 1e-6);
        assert!(unique_ids(&pieces));
        let far = hammer_box(DVec3::splat(500.0), DVec3::splat(600.0));
        assert_eq!(carve(&target, &far, &mut next).expect("carve"), vec![target.clone()]);
        let touching = hammer_box(DVec3::new(128.0, 0.0, 0.0), DVec3::new(256.0, 128.0, 64.0));
        assert_eq!(carve(&target, &touching, &mut next).expect("carve"), vec![target.clone()]);
        let big = hammer_box(DVec3::splat(-10.0), DVec3::splat(300.0));
        assert!(carve(&target, &big, &mut next).expect("carve").is_empty());
        let wedge_params = crate::PrimitiveParams {
            kind: crate::Primitive::Wedge,
            bounds: crate::Aabb::new(DVec3::new(0.0, 100.0, -200.0), DVec3::new(128.0, 300.0, 20.0)),
            ..Default::default()
        };
        let wedge = crate::make_primitive(&wedge_params, &mut next).expect("wedge").remove(0);
        let res = carve(&target, &wedge, &mut next).expect("carve");
        assert!((total(&res) - 128.0 * 128.0 * 64.0).abs() < 1e-6);
    }

    #[test]
    fn hollow_inward_and_outward() {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(256.0, 128.0, 128.0));
        let mut next = id_counter(1000);
        let walls = hollow(&sides, 16.0, &mut next).expect("hollow");
        assert_eq!(walls.len(), 6);
        let expected = 256.0 * 128.0 * 128.0 - 224.0 * 96.0 * 96.0;
        assert!((total(&walls) - expected).abs() < 1e-6);
        assert!(unique_ids(&walls));
        for w in &walls {
            for s in w {
                assert!(s.material.starts_with("TEST/"));
            }
        }
        let out = hollow(&sides, -8.0, &mut next).expect("hollow out");
        let expected = 272.0 * 144.0 * 144.0 - 256.0 * 128.0 * 128.0;
        assert!((total(&out) - expected).abs() < 1e-6);
        assert_eq!(hollow(&sides, 70.0, &mut next), Err(GeomError::WallTooThick));
        assert!(hollow(&sides, 0.0, &mut next).is_err());
        assert!(hollow(&sides, f64::NAN, &mut next).is_err());
    }

    #[test]
    fn hollow_slanted_solid() {
        let mut next = id_counter(1000);
        let p = crate::PrimitiveParams {
            kind: crate::Primitive::Cylinder,
            sides: 12,
            bounds: crate::Aabb::new(DVec3::new(-128.0, -128.0, 0.0), DVec3::new(128.0, 128.0, 64.0)),
            ..Default::default()
        };
        let cyl = crate::make_primitive(&p, &mut next).expect("cyl").remove(0);
        let v0 = volume(&cyl);
        let walls = hollow(&cyl, 8.0, &mut next).expect("hollow");
        let inner: Vec<Side> = cyl.iter().filter_map(|s| offset_side(s, -8.0)).collect();
        let vi = volume(&inner);
        assert!((total(&walls) - (v0 - vi)).abs() < 1e-3);
    }

    #[test]
    fn merge_two_boxes() {
        let a = hammer_box(DVec3::ZERO, DVec3::splat(64.0));
        let b = hammer_box(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0));
        let mut next = id_counter(1000);
        let m = merge_convex(&[a.clone(), b], &mut next).expect("merge");
        assert_eq!(m.len(), 6);
        assert!((volume(&m) - 128.0 * 64.0 * 64.0).abs() < 1e-6);
        let c = hammer_box(DVec3::new(100.0, 100.0, 0.0), DVec3::new(164.0, 164.0, 64.0));
        let m = merge_convex(&[a, c], &mut next).expect("merge");
        assert!(volume(&m) > 2.0 * 64.0f64.powi(3));
        assert!(merge_convex(&[], &mut next).is_err());
    }

    fn check_trace(original: &[Side], pieces: &[ClipPiece]) {
        for p in pieces {
            assert_eq!(p.sides.len(), p.origins.len());
            for (s, o) in p.sides.iter().zip(&p.origins) {
                if let Some(o) = o {
                    let src = Plane::from_side(&original[*o]).expect("source plane");
                    let pl = Plane::from_side(s).expect("plane");
                    assert!(pl.approx_eq(&src, 1e-9, 1e-6), "traced side is off its source plane");
                }
            }
        }
    }

    #[test]
    fn traced_carve_reports_side_origins() {
        let target = hammer_box(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let mut next = id_counter(1000);
        let carver = hammer_box(DVec3::new(32.0, 32.0, 32.0), DVec3::new(96.0, 96.0, 128.0));
        let pieces = carve_traced(&target, &carver, &mut next).expect("carve");
        check_trace(&target, &pieces);
        let from = |i: usize| pieces.iter().flat_map(|p| &p.origins).filter(|o| **o == Some(i)).count();
        assert!(from(0) >= 2, "top face pieces");
        assert!(from(1) >= 1, "bottom face pieces");
        assert!(pieces.iter().flat_map(|p| &p.origins).any(Option::is_none), "pit faces are new");
        let far = hammer_box(DVec3::splat(500.0), DVec3::splat(600.0));
        let same = carve_traced(&target, &far, &mut next).expect("carve");
        assert_eq!(same, vec![ClipPiece::whole(&target)]);
        let big = hammer_box(DVec3::splat(-10.0), DVec3::splat(300.0));
        assert!(carve_traced(&target, &big, &mut next).expect("carve").is_empty());
        let second = hammer_box(DVec3::new(-16.0, 48.0, -16.0), DVec3::new(16.0, 80.0, 16.0));
        let mut chained = Vec::new();
        for p in &pieces {
            for q in carve_traced(&p.sides, &second, &mut next).expect("carve") {
                chained.push(q.compose(&p.origins));
            }
        }
        check_trace(&target, &chained);
        let expected = 128.0 * 128.0 * 64.0 - 64.0 * 64.0 * 32.0 - 16.0 * 32.0 * 16.0;
        let vol: f64 = chained.iter().map(|p| volume(&p.sides)).sum();
        assert!((vol - expected).abs() < 1e-6, "{vol} vs {expected}");
    }

    #[test]
    fn traced_hollow_reports_side_origins() {
        let sides = hammer_box(DVec3::ZERO, DVec3::new(256.0, 128.0, 128.0));
        let mut next = id_counter(1000);
        for thickness in [16.0, -16.0] {
            let walls = hollow_traced(&sides, thickness, &mut next).expect("hollow");
            assert_eq!(walls.len(), 6);
            for (i, s) in sides.iter().enumerate() {
                let n = Plane::from_side(s).expect("plane").normal;
                let outer: Vec<&Side> = walls
                    .iter()
                    .flat_map(|w| w.sides.iter().zip(&w.origins))
                    .filter(|(_, o)| **o == Some(i))
                    .map(|(s, _)| s)
                    .collect();
                assert!(!outer.is_empty(), "face {i} has an outer face");
                for o in outer {
                    assert!(Plane::from_side(o).expect("plane").normal.dot(n) > 1.0 - 1e-9);
                }
            }
            if thickness > 0.0 {
                check_trace(&sides, &walls);
            }
        }
    }

    #[test]
    fn exact_merge_only_when_convex() {
        let mut next = id_counter(1000);
        let a = hammer_box(DVec3::ZERO, DVec3::splat(64.0));
        let b = hammer_box(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0));
        let m = merge_exact(&[a.clone(), b.clone()], &mut next).expect("merge").expect("convex");
        assert!((volume(&m) - 128.0 * 64.0 * 64.0).abs() < 1e-6);
        assert_eq!(m.len(), 6);
        let c = hammer_box(DVec3::new(32.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0));
        assert!(merge_exact(&[a.clone(), c], &mut next).expect("merge").is_some());
        let l = hammer_box(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 32.0, 64.0));
        assert_eq!(merge_exact(&[a.clone(), l], &mut next).expect("merge"), None);
        let o = hammer_box(DVec3::new(32.0, 32.0, 0.0), DVec3::new(96.0, 96.0, 64.0));
        assert_eq!(merge_exact(&[a.clone(), o], &mut next).expect("merge"), None);
        let gap = hammer_box(DVec3::new(65.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0));
        assert_eq!(merge_exact(&[a.clone(), gap], &mut next).expect("merge"), None);
        let p = |x, y, z| DVec3::new(x, y, z);
        let cut = Plane::from_points(p(0.0, 0.0, 0.0), p(0.0, 64.0, 64.0), p(64.0, 64.0, 64.0)).expect("plane");
        let (f, bk) = clip_solid(&a, &cut, &mut next).expect("clip");
        let m = merge_exact(&[f.expect("front"), bk.expect("back")], &mut next).expect("merge").expect("convex");
        assert!((volume(&m) - 64.0f64.powi(3)).abs() < 1e-6);
        let r = hammer_box(DVec3::new(128.0, 0.0, 0.0), DVec3::new(192.0, 64.0, 64.0));
        assert!(merge_exact(&[a.clone(), r.clone(), b], &mut next).expect("merge").is_some());
        assert_eq!(merge_exact(&[a.clone(), r], &mut next).expect("merge"), None, "gap of 64");
        let vol = union_volume(&[a.clone(), a.clone()]).expect("union");
        assert!((vol - 64.0f64.powi(3)).abs() < 1e-6);
    }

    #[test]
    fn exact_merge_never_fills_a_small_notch() {
        let mut next = id_counter(3000);
        let p = |x, y, z| DVec3::new(x, y, z);
        let base = hammer_box(p(0.0, 0.0, 0.0), p(128.0, 128.0, 64.0));
        let s1 = hammer_box(p(0.0, 0.0, 64.0), p(60.0, 128.0, 68.0));
        let s2 = hammer_box(p(62.0, 0.0, 64.0), p(128.0, 128.0, 68.0));
        let short = hammer_box(p(60.0, 0.0, 64.0), p(62.0, 120.0, 68.0));
        let parts = [base.clone(), s1.clone(), s2.clone(), short];
        assert_eq!(merge_exact(&parts, &mut next).expect("merge"), None, "notch of 2x8x4 inside the top face");
        let full = hammer_box(p(60.0, 0.0, 64.0), p(62.0, 128.0, 68.0));
        let m = merge_exact(&[base, s1, s2, full], &mut next).expect("merge").expect("flush pieces merge");
        assert!((volume(&m) - 128.0 * 128.0 * 68.0).abs() < 1e-6);
    }

    #[test]
    fn exact_merge_keeps_sub_epsilon_seams() {
        let mut next = id_counter(4000);
        let p = |x, y, z| DVec3::new(x, y, z);
        let a = hammer_box(p(0.0, 0.0, 0.0), p(64.0, 64.0, 64.0));
        let b = hammer_box(p(64.004, 0.0, 0.0), p(128.0, 64.0, 64.0));
        assert!(
            merge_exact(&[a, b], &mut next).expect("merge").is_some(),
            "a seam below the plane epsilon is not a gap"
        );
    }

    #[test]
    fn offset_moves_plane() {
        let sides = hammer_box(DVec3::ZERO, DVec3::splat(64.0));
        let s = offset_side(&sides[0], 8.0).expect("offset");
        assert_eq!(Plane::from_side(&s).expect("p").dist, 72.0);
        assert!(offset_side(&sides[0], f64::INFINITY).is_none());
    }
}
