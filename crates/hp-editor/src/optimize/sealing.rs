use std::sync::atomic::AtomicBool;

use hp_geom::Aabb;

use super::detail::{DetailCandidate, DetailJob, DetailKind, prepare};
use crate::limits::checks::MaterialFacts;
use crate::limits::leak::{LeakInput, LeakReport, find_leaks};
use crate::{Document, Id, IdSet};

const MAX_ROUNDS: usize = 12;
const LEAK_REACH: f64 = 48.0;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SealCheck {
    pub sealing: Vec<Id>,
    pub verified: bool,
    pub map_leaks: bool,
    pub incomplete: bool,
    pub floods: usize,
}

impl SealCheck {
    fn safe() -> SealCheck {
        SealCheck { verified: true, ..SealCheck::default() }
    }
}

pub fn seal_input(doc: &Document) -> LeakInput {
    doc.leak_input(&|m: &str| MaterialFacts::guess(m))
}

pub fn could_seal(input: &LeakInput, ids: &[Id]) -> Vec<Id> {
    let set: IdSet = ids.iter().copied().collect();
    let mut out: Vec<Id> = input.brushes.iter().map(|b| b.id).filter(|id| set.contains(id)).collect();
    out.sort_unstable();
    out
}

pub fn quick_safe(ids: &[Id], report: &LeakReport) -> bool {
    if report.leaks() || report.incomplete || report.no_world {
        return false;
    }
    let hull: IdSet = report.hull.iter().copied().collect();
    !ids.iter().any(|id| hull.contains(id))
}

pub fn sealing_brushes(
    input: &LeakInput,
    ids: &[Id],
    known: Option<&LeakReport>,
    cancel: &AtomicBool,
) -> Option<SealCheck> {
    let candidates = could_seal(input, ids);
    if candidates.is_empty() {
        return Some(SealCheck::safe());
    }
    let mut floods = 0;
    let owned;
    let report = match known {
        Some(r) => r,
        None => {
            floods += 1;
            owned = find_leaks(input, cancel)?;
            &owned
        }
    };
    if report.incomplete || report.leaks() || report.no_world {
        return Some(SealCheck {
            verified: false,
            map_leaks: report.leaks(),
            incomplete: report.incomplete,
            floods,
            ..SealCheck::default()
        });
    }
    let hull: IdSet = report.hull.iter().copied().collect();
    if !candidates.iter().any(|id| hull.contains(id)) {
        return Some(SealCheck { floods, ..SealCheck::safe() });
    }
    let boxes: std::collections::HashMap<Id, Aabb> =
        input.brushes.iter().filter(|b| candidates.binary_search(&b.id).is_ok()).map(|b| (b.id, b.aabb)).collect();
    let mut removed: Vec<Id> = candidates;
    let mut sealing: Vec<Id> = Vec::new();
    for _ in 0..MAX_ROUNDS {
        if removed.is_empty() {
            break;
        }
        let set: IdSet = removed.iter().copied().collect();
        let reduced = LeakInput {
            brushes: input.brushes.iter().filter(|b| !set.contains(&b.id)).cloned().collect(),
            seeds: input.seeds.clone(),
        };
        floods += 1;
        let r = find_leaks(&reduced, cancel)?;
        if r.incomplete {
            return Some(SealCheck { incomplete: true, floods, ..SealCheck::default() });
        }
        let Some(path) = r.leak else {
            sealing.sort_unstable();
            return Some(SealCheck { sealing, verified: true, floods, ..SealCheck::default() });
        };
        let near = |id: &Id| {
            boxes.get(id).map(|b| b.expand(LEAK_REACH)).is_some_and(|b| {
                path.points.windows(2).any(|w| (0..=8).any(|i| b.contains_point(w[0].lerp(w[1], f64::from(i) / 8.0))))
                    || path.points.iter().any(|p| b.contains_point(*p))
            })
        };
        let before = removed.len();
        let (hit, stay): (Vec<Id>, Vec<Id>) = removed.into_iter().partition(|id| near(id));
        sealing.extend(hit);
        removed = stay;
        if removed.len() == before {
            let (mut on_hull, others): (Vec<Id>, Vec<Id>) = removed.into_iter().partition(|id| hull.contains(id));
            if on_hull.is_empty() {
                sealing.extend(others);
                removed = Vec::new();
            } else {
                let half = on_hull.len().div_ceil(2);
                sealing.extend(on_hull.drain(..half));
                on_hull.extend(others);
                removed = on_hull;
            }
        }
    }
    sealing.extend(removed);
    sealing.sort_unstable();
    sealing.dedup();
    Some(SealCheck { sealing, verified: true, floods, ..SealCheck::default() })
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateFilter {
    pub max_size: f64,
    pub kinds: Vec<DetailKind>,
    pub within: Option<Vec<Id>>,
}

impl Default for CandidateFilter {
    fn default() -> Self {
        CandidateFilter { max_size: 1024.0, kinds: DetailKind::ALL.to_vec(), within: None }
    }
}

impl CandidateFilter {
    pub fn accepts(&self, c: &DetailCandidate) -> bool {
        let size_ok =
            !self.max_size.is_finite() || self.max_size <= 0.0 || c.size.max_element() <= self.max_size + 1e-6;
        size_ok && self.kinds.contains(&c.kind) && self.within.as_ref().is_none_or(|w| w.binary_search(&c.id).is_ok())
    }
}

pub fn candidate_job(doc: &Document, filter: &CandidateFilter) -> DetailJob {
    let mut job = prepare(doc);
    let mut f = filter.clone();
    if let Some(w) = f.within.as_mut() {
        w.sort_unstable();
    }
    job.items.retain(|(c, _)| f.accepts(c));
    job
}

impl DetailKind {
    pub fn parse(s: &str) -> Option<DetailKind> {
        let s = s.trim().to_ascii_lowercase();
        let s = s.trim_end_matches('s');
        match s {
            "stair" | "step" => Some(DetailKind::Step),
            _ => DetailKind::ALL.into_iter().find(|k| k.id() == s),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use glam::DVec3;

    use super::*;
    use crate::{ObjRef, Owner};

    fn box_sides(min: DVec3, max: DVec3, material: &str) -> Vec<hp_vmf::Side> {
        let (x0, y0, z0, x1, y1, z1) = (min.x, min.y, min.z, max.x, max.y, max.z);
        let v = DVec3::new;
        let faces = [
            [v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)],
            [v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)],
            [v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)],
            [v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)],
            [v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)],
            [v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)],
        ];
        faces
            .iter()
            .map(|plane| hp_vmf::Side {
                plane: *plane,
                material: material.to_owned(),
                uaxis: hp_vmf::TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
                vaxis: hp_vmf::TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
                ..Default::default()
            })
            .collect()
    }

    struct Room {
        doc: Document,
        walls: Vec<Id>,
        pillar: Id,
        step: Id,
        trim: Id,
    }

    fn room() -> Room {
        let mut doc = Document::new_empty();
        let (s, w) = (256.0, 16.0);
        let wall = |doc: &mut Document, a: DVec3, b: DVec3| {
            doc.create_solid(box_sides(a, b, "BRICK/WALL"), Owner::World).expect("wall")
        };
        let walls = vec![
            wall(&mut doc, DVec3::new(-s - w, -s - w, -s - w), DVec3::new(-s, s + w, s + w)),
            wall(&mut doc, DVec3::new(s, -s - w, -s - w), DVec3::new(s + w, s + w, s + w)),
            wall(&mut doc, DVec3::new(-s, -s - w, -s - w), DVec3::new(s, -s, s + w)),
            wall(&mut doc, DVec3::new(-s, s, -s - w), DVec3::new(s, s + w, s + w)),
            wall(&mut doc, DVec3::new(-s, -s, -s - w), DVec3::new(s, s, -s)),
            wall(&mut doc, DVec3::new(-s, -s, s), DVec3::new(s, s, s + w)),
        ];
        let pillar = wall(&mut doc, DVec3::new(-232.0, 120.0, -256.0), DVec3::new(-168.0, 184.0, 0.0));
        let step = wall(&mut doc, DVec3::new(100.0, -200.0, -256.0), DVec3::new(132.0, 100.0, -240.0));
        let trim = wall(&mut doc, DVec3::new(-250.0, 200.0, -256.0), DVec3::new(-242.0, 208.0, 100.0));
        doc.create_point_entity("info_player_start", DVec3::new(150.0, 150.0, -240.0), &[]).expect("spawn");
        doc.clear_history();
        Room { doc, walls, pillar, step, trim }
    }

    #[test]
    fn walls_of_the_shell_are_sealing_and_props_inside_are_not() {
        let r = room();
        let input = seal_input(&r.doc);
        let cancel = AtomicBool::new(false);
        let inner = sealing_brushes(&input, &[r.pillar, r.step], None, &cancel).expect("check");
        assert!(inner.verified);
        assert!(inner.sealing.is_empty());
        let mixed = sealing_brushes(&input, &[r.pillar, r.walls[2]], None, &cancel).expect("check");
        assert!(mixed.verified);
        assert_eq!(mixed.sealing, vec![r.walls[2]]);
        let all = sealing_brushes(&input, &r.walls, None, &cancel).expect("check");
        assert!(all.verified);
        assert!(!all.sealing.is_empty());
        let report = find_leaks(&input, &cancel).expect("report");
        assert!(quick_safe(&[r.pillar], &report));
        assert!(!quick_safe(&[r.walls[0]], &report));
        let fast = sealing_brushes(&input, &[r.pillar], Some(&report), &cancel).expect("check");
        assert_eq!(fast.floods, 0);
        assert!(fast.verified && fast.sealing.is_empty());
    }

    #[test]
    fn a_leaking_map_cannot_be_verified() {
        let mut r = room();
        r.doc.delete(&[ObjRef::Solid(r.walls[5])]).expect("delete");
        let input = seal_input(&r.doc);
        let check = sealing_brushes(&input, &[r.walls[0]], None, &AtomicBool::new(false)).expect("check");
        assert!(!check.verified);
        assert!(check.map_leaks);
    }

    #[test]
    fn non_sealing_materials_need_no_flood() {
        let mut r = room();
        let glass = r
            .doc
            .create_solid(
                box_sides(DVec3::new(0.0, 0.0, -256.0), DVec3::new(8.0, 8.0, -200.0), "TOOLS/TOOLSCLIP"),
                Owner::World,
            )
            .expect("glass");
        let input = seal_input(&r.doc);
        let check = sealing_brushes(&input, &[glass], None, &AtomicBool::new(false)).expect("check");
        assert_eq!(check.floods, 0);
        assert!(check.verified);
    }

    #[test]
    fn candidates_follow_the_filters_and_skip_the_shell() {
        let r = room();
        let cancel = AtomicBool::new(false);
        let all = candidate_job(&r.doc, &CandidateFilter::default()).run(None, &cancel);
        assert!(all.verified);
        let ids = all.ids();
        for id in [r.pillar, r.step, r.trim] {
            assert!(ids.contains(&id), "{id} in {ids:?}");
        }
        for w in &r.walls {
            assert!(!ids.contains(w));
        }
        let no_steps = CandidateFilter {
            kinds: vec![DetailKind::Pillar, DetailKind::Trim, DetailKind::Small, DetailKind::Complex],
            ..CandidateFilter::default()
        };
        let ids = candidate_job(&r.doc, &no_steps).run(None, &cancel).ids();
        assert!(!ids.contains(&r.step) && ids.contains(&r.pillar));
        let small = CandidateFilter { max_size: 200.0, ..CandidateFilter::default() };
        let ids = candidate_job(&r.doc, &small).run(None, &cancel).ids();
        assert!(!ids.contains(&r.step) && !ids.contains(&r.trim));
        let within = CandidateFilter { within: Some(vec![r.trim]), ..CandidateFilter::default() };
        assert_eq!(candidate_job(&r.doc, &within).run(None, &cancel).ids(), vec![r.trim]);
    }

    #[test]
    fn kinds_parse_with_plurals_and_stairs() {
        assert_eq!(DetailKind::parse("stairs"), Some(DetailKind::Step));
        assert_eq!(DetailKind::parse("Pillars"), Some(DetailKind::Pillar));
        assert_eq!(DetailKind::parse("small"), Some(DetailKind::Small));
        assert_eq!(DetailKind::parse("trim"), Some(DetailKind::Trim));
        assert_eq!(DetailKind::parse("complex"), Some(DetailKind::Complex));
        assert_eq!(DetailKind::parse("x"), None);
    }
}
