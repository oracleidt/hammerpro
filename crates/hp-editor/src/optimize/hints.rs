use glam::DVec3;
use hp_geom::Aabb;
use hp_vmf::Side;

use super::{Finding, Impact, Topic, fmt_point};
use crate::limits::portals::{BspVis, PortalFile};
use crate::scene::Owner;
use crate::{Document, EditError, EditResult, Id};

pub const HINT: &str = "TOOLS/TOOLSHINT";
pub const SKIP: &str = "TOOLS/TOOLSSKIP";
const THICKNESS: f64 = 8.0;
const MARGIN: f64 = 32.0;
const PAD: f64 = 8.0;
const MAX_CANDIDATES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HintReason {
    DoorwayTop,
    Jamb,
}

impl HintReason {
    pub fn id(self) -> &'static str {
        match self {
            HintReason::DoorwayTop => "doorway_top",
            HintReason::Jamb => "opening_edge",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HintCandidate {
    pub axis: usize,
    pub at: f64,
    pub rect: Aabb,
    pub cluster: u32,
    pub reason: HintReason,
    pub weight: f64,
    pub gain: Option<i64>,
}

impl HintCandidate {
    pub fn center(&self) -> DVec3 {
        let mut c = self.rect.center();
        c[self.axis] = self.at;
        c
    }

    pub fn normal(&self) -> DVec3 {
        let mut n = DVec3::ZERO;
        n[self.axis] = 1.0;
        n
    }

    pub fn brush_box(&self) -> Aabb {
        let mut b = self.rect;
        b.min[self.axis] = self.at - THICKNESS;
        b.max[self.axis] = self.at;
        b
    }

    pub fn sides(&self) -> EditResult<Vec<Side>> {
        let b = self.brush_box();
        if b.is_empty() || b.size().min_element() <= 0.0 || !b.min.is_finite() || !b.max.is_finite() {
            return Err(EditError::Invalid("empty hint brush".to_owned()));
        }
        let mut next = 1u32;
        let mut sides = hp_geom::hull_solid(&b.corners(), SKIP, 0.25, 16, &mut || {
            next += 1;
            next
        })?;
        let n = self.normal();
        for s in &mut sides {
            if s.normal().dot(n) > 0.999 {
                s.material = HINT.to_owned();
            }
        }
        Ok(sides)
    }

    pub fn finding(&self) -> Finding {
        let c = self.center();
        let axis = ["x", "y", "z"][self.axis.min(2)];
        let gain = self.gain.map_or_else(String::new, |g| g.to_string());
        let mut f = Finding::new(
            Topic::Hints,
            if self.reason == HintReason::DoorwayTop { "hint_doorway_top" } else { "hint_opening_edge" },
            if self.gain.is_some_and(|g| g > 0) { Impact::Medium } else { Impact::Low },
            match self.reason {
                HintReason::DoorwayTop => format!(
                    "Add a horizontal hint brush at the top of the opening near {} ({axis} = {:.0}): the tall visleaf above \
                     the doorway sees over it into the next rooms; the hint cuts it at the height of the opening.{}",
                    fmt_point(c),
                    self.at,
                    if gain.is_empty() { String::new() } else { format!(" Measured: {gain} fewer visible leaf pairs.") }
                ),
                HintReason::Jamb => format!(
                    "Add a hint brush that extends the edge of the opening near {} ({axis} = {:.0}): the visleaf at this \
                     corner sees down both sides; the hint splits it where the view through the opening ends.{}",
                    fmt_point(c),
                    self.at,
                    if gain.is_empty() { String::new() } else { format!(" Measured: {gain} fewer visible leaf pairs.") }
                ),
            },
        )
        .score(self.gain.map_or(self.weight, |g| g.max(0) as f64 * 10.0))
        .points(vec![c])
        .boxes(vec![self.brush_box()])
        .args(vec![fmt_point(c), axis.to_owned(), format!("{:.0}", self.at), gain]);
        if self.gain.is_some_and(|g| g > 0) {
            f = f.fix("hint");
        }
        f
    }
}

fn polygon_box(points: &[DVec3]) -> Aabb {
    Aabb::from_points(points.iter().copied())
}

pub fn existing_hints(doc: &Document) -> Vec<Aabb> {
    doc.scene
        .solids
        .values()
        .filter(|s| s.owner == Owner::World && s.solid.sides.iter().any(|x| x.material.eq_ignore_ascii_case(HINT)))
        .map(|s| s.geom.aabb.expand(4.0))
        .collect()
}

pub fn visible_counts(vis: &BspVis) -> Vec<u32> {
    (0..vis.cluster_bounds().len() as u32)
        .map(|c| vis.visible_from(c).map_or(0, |row| row.iter().filter(|v| **v).count() as u32))
        .collect()
}

pub fn candidates(vis: &BspVis, portals: &PortalFile, visible: &[u32], existing: &[Aabb]) -> Vec<HintCandidate> {
    let bounds = vis.cluster_bounds();
    let n = bounds.len();
    if n == 0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..n).filter(|c| visible.get(*c).copied().unwrap_or(0) > 1).collect();
    order.sort_by(|a, b| {
        let wa = f64::from(visible[*a]) * bounds[*a].volume().max(1.0).ln();
        let wb = f64::from(visible[*b]) * bounds[*b].volume().max(1.0).ln();
        wb.total_cmp(&wa)
    });
    order.truncate(24);
    let mut out: Vec<HintCandidate> = Vec::new();
    for &c in &order {
        let cb = bounds[c];
        if cb.is_empty() {
            continue;
        }
        let weight = f64::from(visible[c]);
        for p in portals.portals_of(c as u32) {
            let pb = polygon_box(&p.points);
            if pb.is_empty() || p.points.len() < 3 {
                continue;
            }
            let normal = (p.points[1] - p.points[0]).cross(p.points[2] - p.points[0]).normalize_or_zero();
            if normal.z.abs() > 0.3 || normal == DVec3::ZERO {
                continue;
            }
            let k = if normal.x.abs() >= normal.y.abs() { 0 } else { 1 };
            let u = 1 - k;
            let open = pb.size();
            if open[u] > 512.0 || open.z > 512.0 || open[u] < 16.0 || open.z < 16.0 {
                continue;
            }
            let mut push = |axis: usize, at: f64, reason: HintReason| {
                let mut rect = cb.expand(PAD);
                rect.min[axis] = at;
                rect.max[axis] = at;
                if reason == HintReason::Jamb {
                    rect.min.z = rect.min.z.max(pb.min.z - PAD);
                }
                let center = {
                    let mut x = rect.center();
                    x[axis] = at;
                    x
                };
                if existing.iter().any(|e| e.contains_point(center)) {
                    return;
                }
                out.push(HintCandidate { axis, at, rect, cluster: c as u32, reason, weight, gain: None });
            };
            if cb.max.z - pb.max.z > MARGIN && pb.max.z - cb.min.z > 32.0 {
                push(2, pb.max.z.round(), HintReason::DoorwayTop);
            }
            if pb.min[u] - cb.min[u] > MARGIN {
                push(u, pb.min[u].round(), HintReason::Jamb);
            }
            if cb.max[u] - pb.max[u] > MARGIN {
                push(u, pb.max[u].round(), HintReason::Jamb);
            }
        }
    }
    let mut merged: Vec<HintCandidate> = Vec::new();
    for cand in out {
        if let Some(m) = merged
            .iter_mut()
            .find(|m| m.axis == cand.axis && (m.at - cand.at).abs() < 4.0 && m.rect.expand(16.0).intersects(&cand.rect))
        {
            let at = m.at;
            m.rect = m.rect.union(&cand.rect);
            m.rect.min[m.axis] = at;
            m.rect.max[m.axis] = at;
            m.weight += cand.weight;
        } else {
            merged.push(cand);
        }
    }
    merged.retain(|m| m.rect.size().max_element() <= 4096.0);
    merged.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    merged.truncate(MAX_CANDIDATES);
    merged
}

impl Document {
    pub fn add_hint_brushes(&mut self, list: &[HintCandidate]) -> EditResult<Vec<Id>> {
        if list.is_empty() {
            return Err(EditError::Nothing("no hint brush to add"));
        }
        let mut solids = Vec::with_capacity(list.len());
        for c in list {
            solids.push(c.sides()?);
        }
        self.tx("Add hint brushes", |doc| doc.create_solids(solids, Owner::World))
    }
}
