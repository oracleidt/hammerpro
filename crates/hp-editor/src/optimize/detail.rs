use std::sync::atomic::AtomicBool;

use glam::DVec3;
use hp_geom::Aabb;

use super::{Finding, Impact, Topic};
use crate::limits::checks::{DetailReason, MaterialFacts, detail_reason};
use crate::limits::leak::{LeakInput, find_leaks};
use crate::limits::portals::BspVis;
use crate::scene::Owner;
use crate::selection::ObjRef;
use crate::{Document, Id, IdSet};

const MAX_ROUNDS: usize = 12;
const LEAK_REACH: f64 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DetailKind {
    Step,
    Trim,
    Pillar,
    Small,
    Complex,
}

impl DetailKind {
    pub fn id(self) -> &'static str {
        match self {
            DetailKind::Step => "step",
            DetailKind::Trim => "trim",
            DetailKind::Pillar => "pillar",
            DetailKind::Small => "small",
            DetailKind::Complex => "complex",
        }
    }

    pub const ALL: [DetailKind; 5] =
        [DetailKind::Step, DetailKind::Trim, DetailKind::Pillar, DetailKind::Small, DetailKind::Complex];
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetailCandidate {
    pub id: Id,
    pub kind: DetailKind,
    pub size: DVec3,
    pub center: DVec3,
    pub leaves_touched: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailSuggestion {
    pub candidates: Vec<DetailCandidate>,
    pub kept_for_sealing: Vec<Id>,
    pub verified: bool,
    pub map_leaks: bool,
    pub incomplete: bool,
}

impl DetailSuggestion {
    pub fn ids(&self) -> Vec<Id> {
        self.candidates.iter().map(|c| c.id).collect()
    }

    pub fn count(&self, kind: DetailKind) -> usize {
        self.candidates.iter().filter(|c| c.kind == kind).count()
    }

    pub fn leaves_touched(&self) -> usize {
        self.candidates.iter().map(|c| c.leaves_touched).sum()
    }

    pub fn finding(&self) -> Option<Finding> {
        if self.candidates.is_empty() {
            return None;
        }
        let n = self.candidates.len();
        let touched = self.leaves_touched();
        let counts: Vec<String> = DetailKind::ALL
            .iter()
            .map(|k| (k.id(), self.count(*k)))
            .filter(|(_, c)| *c > 0)
            .map(|(k, c)| format!("{c} {k}"))
            .collect();
        let impact = if n >= 40 || touched >= 200 {
            Impact::High
        } else if n >= 8 || touched >= 40 {
            Impact::Medium
        } else {
            Impact::Low
        };
        let mut f = Finding::new(
            Topic::Detail,
            "detail_candidates",
            impact,
            format!(
                "Turn these {n} world brushes into func_detail ({}): they are small, do not seal the map and cut {touched} \
                 visleaves. Detail brushes still block players and cast shadows, but VBSP stops splitting the visleaves \
                 around them, so VVIS is faster and each leaf sees less.",
                counts.join(", ")
            ),
        )
        .score(touched as f64 * 20.0 + n as f64)
        .objects(self.candidates.iter().map(|c| ObjRef::Solid(c.id)).collect())
        .points(self.candidates.iter().take(64).map(|c| c.center).collect())
        .args(vec![n.to_string(), counts.join(", "), touched.to_string()]);
        if self.verified {
            f = f.fix("func_detail");
        }
        Some(f)
    }
}

fn axial(n: &crate::scene::SolidNode) -> bool {
    n.geom.faces.iter().all(|f| {
        let a = f.plane.normal.abs();
        a.max_element() > 0.9999
    })
}

pub fn classify(size: DVec3, faces: usize, axial: bool) -> Option<DetailKind> {
    if !size.is_finite() || size.min_element() <= 0.0 {
        return None;
    }
    let horiz_min = size.x.min(size.y);
    let horiz_max = size.x.max(size.y);
    let extent = size.max_element();
    let mut dims = [size.x, size.y, size.z];
    dims.sort_by(f64::total_cmp);
    if dims[1] <= 16.0 && dims[2] > 64.0 && dims[2] <= 1024.0 {
        return Some(DetailKind::Trim);
    }
    if size.z <= 32.0 && horiz_min <= 64.0 && horiz_max > 64.0 && horiz_max <= 512.0 {
        return Some(DetailKind::Step);
    }
    if horiz_max <= 64.0 && size.z >= 64.0 && size.z <= 1024.0 {
        return Some(DetailKind::Pillar);
    }
    match detail_reason(extent, faces, axial) {
        Some(DetailReason::Small) => Some(DetailKind::Small),
        Some(DetailReason::Complex) => Some(DetailKind::Complex),
        None => None,
    }
}

#[derive(Clone, Debug, Default)]
pub struct DetailJob {
    pub input: LeakInput,
    pub items: Vec<(DetailCandidate, Aabb)>,
}

pub fn prepare(doc: &Document) -> DetailJob {
    let facts = |m: &str| MaterialFacts::guess(m);
    let input = doc.leak_input(&facts);
    let mut items = Vec::new();
    for n in doc.scene.solids.values() {
        if n.owner != Owner::World || !n.geom.valid || n.has_disp() || !doc.is_visible(ObjRef::Solid(n.solid.id)) {
            continue;
        }
        if !n.solid.sides.iter().all(|s| {
            let f = facts(&s.material);
            f.seals() && !f.sky()
        }) {
            continue;
        }
        let size = n.geom.aabb.size();
        let Some(kind) = classify(size, n.geom.faces.len(), axial(n)) else { continue };
        let c = DetailCandidate { id: n.solid.id, kind, size, center: n.geom.aabb.center(), leaves_touched: 0 };
        items.push((c, n.geom.aabb));
    }
    DetailJob { input, items }
}

pub fn suggest(doc: &Document, vis: Option<&BspVis>, cancel: &AtomicBool) -> DetailSuggestion {
    prepare(doc).run(vis, cancel)
}

impl DetailJob {
    pub fn run(self, vis: Option<&BspVis>, cancel: &AtomicBool) -> DetailSuggestion {
        let DetailJob { input, items } = self;
        let Some(report) = find_leaks(&input, cancel) else {
            return DetailSuggestion { incomplete: true, ..DetailSuggestion::default() };
        };
        let map_leaks = report.leaks();
        let hull: IdSet = report.hull.iter().copied().collect();
        let boxes: std::collections::HashMap<Id, Aabb> = items.iter().map(|(c, b)| (c.id, *b)).collect();
        let mut candidates: Vec<DetailCandidate> = items
            .into_iter()
            .filter(|(c, _)| !hull.contains(&c.id))
            .map(|(mut c, b)| {
                c.leaves_touched = vis.map_or(0, |v| {
                    let b = b.expand(1.0);
                    v.clusters_in_box(b.min, b.max).len()
                });
                c
            })
            .collect();
        candidates.sort_by(|a, b| b.leaves_touched.cmp(&a.leaves_touched).then(a.id.cmp(&b.id)));
        if map_leaks || report.incomplete || report.no_world {
            return DetailSuggestion {
                candidates,
                verified: false,
                map_leaks,
                incomplete: report.incomplete,
                ..DetailSuggestion::default()
            };
        }
        let mut kept: Vec<Id> = Vec::new();
        for _ in 0..MAX_ROUNDS {
            if candidates.is_empty() {
                break;
            }
            let set: IdSet = candidates.iter().map(|c| c.id).collect();
            let reduced = LeakInput {
                brushes: input.brushes.iter().filter(|b| !set.contains(&b.id)).cloned().collect(),
                seeds: input.seeds.clone(),
            };
            let Some(r) = find_leaks(&reduced, cancel) else {
                return DetailSuggestion { incomplete: true, ..DetailSuggestion::default() };
            };
            if r.incomplete {
                return DetailSuggestion { candidates, incomplete: true, ..DetailSuggestion::default() };
            }
            let Some(path) = r.leak else {
                return DetailSuggestion {
                    candidates,
                    kept_for_sealing: kept,
                    verified: true,
                    ..DetailSuggestion::default()
                };
            };
            let near = |c: &DetailCandidate| {
                boxes.get(&c.id).map(|b| b.expand(LEAK_REACH)).is_some_and(|b| {
                    path.points
                        .windows(2)
                        .any(|w| (0..=8).any(|i| b.contains_point(w[0].lerp(w[1], f64::from(i) / 8.0))))
                        || path.points.iter().any(|p| b.contains_point(*p))
                })
            };
            let before = candidates.len();
            let (stay, drop): (Vec<DetailCandidate>, Vec<DetailCandidate>) =
                candidates.into_iter().partition(|c| !near(c));
            kept.extend(drop.iter().map(|c| c.id));
            candidates = stay;
            if candidates.len() == before {
                let half = candidates.len().div_ceil(2);
                kept.extend(candidates.drain(..half).map(|c| c.id));
            }
        }
        kept.extend(candidates.iter().map(|c| c.id));
        DetailSuggestion {
            candidates: Vec::new(),
            kept_for_sealing: kept,
            verified: true,
            ..DetailSuggestion::default()
        }
    }
}
