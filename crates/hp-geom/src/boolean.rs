use hp_vmf::Side;

use crate::brush::validate_solid;
use crate::csg::{ClipPiece, carve_impl, clip_geom};
use crate::{GeomError, PLANE_EPS, Plane, build_solid, merge_exact};

const MIN_VOLUME: f64 = 1e-3;

pub fn intersect_traced(
    target: &[Side],
    cutter: &[Side],
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Option<ClipPiece>, GeomError> {
    let tg = validate_solid(target)?;
    let cg = validate_solid(cutter)?;
    if tg.aabb.intersection(&cg.aabb).size().min_element() <= PLANE_EPS {
        return Ok(None);
    }
    let mut remaining = ClipPiece::whole(target);
    let mut geom = tg;
    for face in &cg.faces {
        let template = cutter.get(face.side_index);
        let (front, back) = clip_geom(&remaining.sides, &geom, &face.plane, template, next_side_id)?;
        match (front, back) {
            (Some(_), Some(b)) => {
                remaining = b.compose(&remaining.origins);
                geom = build_solid(&remaining.sides);
                if !geom.valid {
                    return Ok(None);
                }
            }
            (None, Some(_)) => {}
            (Some(_), None) | (None, None) => return Ok(None),
        }
    }
    Ok((geom.volume() > MIN_VOLUME).then_some(remaining))
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnionPart {
    pub origin: usize,
    pub sides: Vec<Side>,
    pub sources: Vec<Vec<u32>>,
}

pub fn union_pieces(solids: &[Vec<Side>], next_side_id: &mut dyn FnMut() -> u32) -> Result<Vec<UnionPart>, GeomError> {
    let geoms = solids.iter().map(|s| validate_solid(s)).collect::<Result<Vec<_>, _>>()?;
    let mut parts: Vec<UnionPart> = Vec::new();
    for (k, (sides, geom)) in solids.iter().zip(&geoms).enumerate() {
        let mut pieces: Vec<ClipPiece> = vec![ClipPiece::whole(sides)];
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
                rest.extend(
                    carve_impl(&p.sides, earlier, false, next_side_id)?.into_iter().map(|c| c.compose(&p.origins)),
                );
            }
            pieces = rest;
        }
        parts.extend(pieces.into_iter().filter(|p| build_solid(&p.sides).volume() > MIN_VOLUME).map(|p| {
            let sources = p
                .origins
                .iter()
                .map(|o| o.and_then(|j| sides.get(j)).map(|s| vec![s.id]).unwrap_or_default())
                .collect();
            UnionPart { origin: k, sides: p.sides, sources }
        }));
    }
    join_touching(&mut parts, next_side_id);
    Ok(parts)
}

fn coplanar_sources(merged: &[Side], from: &[&UnionPart]) -> Vec<Vec<u32>> {
    merged
        .iter()
        .map(|m| {
            let Some(mp) = Plane::from_side(m) else { return Vec::new() };
            let mut ids: Vec<u32> = Vec::new();
            for part in from {
                for (side, srcs) in part.sides.iter().zip(&part.sources) {
                    if Plane::from_side(side).is_some_and(|p| p.approx_eq(&mp, 1e-9, 1e-6)) {
                        ids.extend_from_slice(srcs);
                    }
                }
            }
            ids.sort_unstable();
            ids.dedup();
            ids
        })
        .collect()
}

fn join_touching(parts: &mut Vec<UnionPart>, next_side_id: &mut dyn FnMut() -> u32) {
    'again: loop {
        let boxes: Vec<_> = parts.iter().map(|p| build_solid(&p.sides).aabb.expand(PLANE_EPS * 2.0)).collect();
        for i in 0..parts.len() {
            for j in (i + 1)..parts.len() {
                if !boxes[i].intersects(&boxes[j]) {
                    continue;
                }
                let pair = [parts[i].sides.clone(), parts[j].sides.clone()];
                if let Ok(Some(merged)) = merge_exact(&pair, next_side_id) {
                    let sources = coplanar_sources(&merged, &[&parts[i], &parts[j]]);
                    let origin = parts[i].origin.min(parts[j].origin);
                    parts[i] = UnionPart { origin, sides: merged, sources };
                    parts.remove(j);
                    continue 'again;
                }
            }
        }
        return;
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::*;
    use crate::test_support::*;

    fn cube(a: [f64; 3], b: [f64; 3]) -> Vec<Side> {
        hammer_box(DVec3::from_array(a), DVec3::from_array(b))
    }

    fn volume(sides: &[Side]) -> f64 {
        build_solid(sides).volume()
    }

    #[test]
    fn intersect_overlapping_boxes() {
        let a = cube([0.0; 3], [64.0; 3]);
        let b = cube([32.0; 3], [96.0; 3]);
        let mut next = id_counter(1000);
        let piece = intersect_traced(&a, &b, &mut next).expect("intersect").expect("overlap");
        assert!((volume(&piece.sides) - 32.0 * 32.0 * 32.0).abs() < 1e-6);
        assert_eq!(piece.sides.len(), piece.origins.len());
        assert_eq!(piece.origins.iter().filter(|o| o.is_none()).count(), 3);
        let cuts: Vec<_> =
            piece.sides.iter().zip(&piece.origins).filter(|(_, o)| o.is_none()).map(|(s, _)| &s.material).collect();
        assert!(cuts.iter().all(|m| m.starts_with("TEST/FACE")));
    }

    #[test]
    fn intersect_touching_or_apart_is_empty() {
        let a = cube([0.0; 3], [64.0; 3]);
        let mut next = id_counter(1000);
        assert!(intersect_traced(&a, &cube([64.0, 0.0, 0.0], [128.0, 64.0, 64.0]), &mut next).expect("ok").is_none());
        assert!(intersect_traced(&a, &cube([200.0; 3], [264.0; 3]), &mut next).expect("ok").is_none());
    }

    #[test]
    fn union_overlapping_boxes_in_line_is_one_box() {
        let a = cube([0.0; 3], [64.0; 3]);
        let b = cube([32.0, 0.0, 0.0], [96.0, 64.0, 64.0]);
        let mut next = id_counter(1000);
        let parts = union_pieces(&[a, b], &mut next).expect("union");
        assert_eq!(parts.len(), 1);
        assert!((volume(&parts[0].sides) - 96.0 * 64.0 * 64.0).abs() < 1e-6);
    }

    #[test]
    fn union_touching_boxes_in_line_is_one_box() {
        let a = cube([0.0; 3], [64.0; 3]);
        let b = cube([64.0, 0.0, 0.0], [128.0, 64.0, 64.0]);
        let mut next = id_counter(1000);
        let parts = union_pieces(&[a, b], &mut next).expect("union");
        assert_eq!(parts.len(), 1);
        assert!((volume(&parts[0].sides) - 128.0 * 64.0 * 64.0).abs() < 1e-6);
    }

    #[test]
    fn union_l_shape_keeps_disjoint_pieces_with_exact_volume() {
        let a = cube([0.0; 3], [128.0, 32.0, 32.0]);
        let b = cube([0.0, 0.0, 16.0], [32.0, 96.0, 48.0]);
        let mut next = id_counter(1000);
        let parts = union_pieces(&[a.clone(), b.clone()], &mut next).expect("union");
        assert!(parts.len() >= 2);
        let total: f64 = parts.iter().map(|p| volume(&p.sides)).sum();
        let expected = crate::union_volume(&[a, b]).expect("volume");
        assert!((total - expected).abs() < 1e-6);
        let mut ids: Vec<u32> = parts.iter().flat_map(|p| p.sides.iter().map(|s| s.id)).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn union_contained_box_vanishes() {
        let a = cube([0.0; 3], [64.0; 3]);
        let b = cube([16.0; 3], [32.0; 3]);
        let mut next = id_counter(1000);
        let parts = union_pieces(&[a, b], &mut next).expect("union");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].origin, 0);
    }
}
