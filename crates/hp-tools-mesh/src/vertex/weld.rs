use glam::DVec3;
use hp_app::Severity;
use hp_app::state::AppState;
use hp_app::{t, tf};
use hp_editor::{Document, Id};
use hp_geom::{VertexEdit, VertexEditOptions, WELD_EPS};

use super::VertexTool;
use super::model::{EditSolid, Reselect, SolidEdit, VertexError, preview_solid};
use crate::common::{edit_error_text, vertex_error_text};

pub const DEFAULT_WELD_DISTANCE: f64 = 1.0;
pub const MAX_WELD_DISTANCE: f64 = 64.0;
pub const CLEANUP_DISTANCE: f64 = 0.01;
pub(crate) const UNDO_WELD: &str = "Weld vertices";
pub(crate) const UNDO_WELD_DISTANCE: &str = "Weld vertices by distance";
pub(crate) const UNDO_CLEANUP: &str = "Clean up solids";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WeldTarget {
    #[default]
    Center,
    First,
    Last,
}

impl WeldTarget {
    pub fn name(self) -> &'static str {
        match self {
            WeldTarget::Center => "center",
            WeldTarget::First => "first",
            WeldTarget::Last => "last",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "center" => Some(WeldTarget::Center),
            "first" => Some(WeldTarget::First),
            "last" => Some(WeldTarget::Last),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            WeldTarget::Center => t!("vertex.weld_center"),
            WeldTarget::First => t!("vertex.weld_first"),
            WeldTarget::Last => t!("vertex.weld_last"),
        }
    }
}

pub fn weld_group(points: &[DVec3], group: &[usize], target: WeldTarget) -> Vec<DVec3> {
    let mut out = points.to_vec();
    let mut members: Vec<usize> = group.iter().copied().filter(|i| *i < points.len()).collect();
    members.sort_unstable();
    members.dedup();
    let (Some(&first), Some(&last)) = (members.first(), members.last()) else { return out };
    let to = match target {
        WeldTarget::Center => members.iter().map(|i| points[*i]).sum::<DVec3>() / members.len() as f64,
        WeldTarget::First => points[first],
        WeldTarget::Last => points[last],
    };
    for i in members {
        out[i] = to;
    }
    out
}

pub fn weld_distance(points: &[DVec3], distance: f64) -> Vec<DVec3> {
    let mut out = points.to_vec();
    if !(distance > 0.0 && distance.is_finite()) {
        return out;
    }
    for _ in 0..points.len() {
        let mut best: Option<(f64, usize, usize)> = None;
        for i in 0..out.len() {
            for j in (i + 1)..out.len() {
                let d = out[i].distance(out[j]);
                if d > WELD_EPS && d <= distance && best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, i, j));
                }
            }
        }
        let Some((_, i, j)) = best else { break };
        let (a, b) = (out[i], out[j]);
        let mid = (a + b) * 0.5;
        for p in &mut out {
            if p.distance(a) <= WELD_EPS || p.distance(b) <= WELD_EPS {
                *p = mid;
            }
        }
    }
    out
}

pub fn distinct_count(points: &[DVec3]) -> usize {
    let mut seen: Vec<DVec3> = Vec::with_capacity(points.len());
    for p in points {
        if !seen.iter().any(|q| q.distance(*p) <= WELD_EPS) {
            seen.push(*p);
        }
    }
    seen.len()
}

pub fn welded_vertices(before: &[DVec3], after: &[DVec3]) -> usize {
    distinct_count(before).saturating_sub(distinct_count(after))
}

fn check(doc: &Document, id: Id, points: &[DVec3]) -> Result<(), VertexError> {
    let Some(node) = doc.scene().solid(id) else {
        let err = hp_editor::EditError::NotFound(hp_editor::ObjRef::Solid(id));
        return Err(VertexError::Edit(std::sync::Arc::new(err)));
    };
    preview_solid(points, &node.solid.sides, None).map(|_| ())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeldReport {
    pub solids_changed: usize,
    pub vertices_welded: usize,
    pub faces_removed: usize,
    pub refused: Vec<(Id, String)>,
}

fn side_count(doc: &Document, id: Id) -> usize {
    doc.scene().solid(id).map_or(0, |n| n.solid.sides.len())
}

type Job = (Id, Vec<DVec3>, Vec<DVec3>);

fn run_jobs(doc: &mut Document, jobs: Vec<Job>, opts: VertexEditOptions) -> WeldReport {
    let mut report = WeldReport::default();
    for (id, points, base) in jobs {
        if let Err(e) = check(doc, id, &points) {
            report.refused.push((id, vertex_error_text(&e)));
            continue;
        }
        let sides = side_count(doc, id);
        let edit = VertexEdit { points: &points, base: None, topology: None };
        if let Err(e) = doc.edit_solid_vertices(id, &edit, opts) {
            report.refused.push((id, edit_error_text(&e)));
            continue;
        }
        report.solids_changed += 1;
        report.vertices_welded += welded_vertices(&base, &points);
        report.faces_removed += sides.saturating_sub(side_count(doc, id));
    }
    report
}

pub fn weld_vertices_in_solid(
    doc: &mut Document,
    id: Id,
    indices: &[usize],
    target: WeldTarget,
    opts: VertexEditOptions,
) -> Result<WeldReport, String> {
    let (base, _) = doc.solid_vertices(id).ok_or("the solid has no valid geometry")?;
    if let Some(bad) = indices.iter().find(|i| **i >= base.len()) {
        return Err(format!("vertex index {bad} is out of range (0 to {})", base.len().saturating_sub(1)));
    }
    let mut group = indices.to_vec();
    group.sort_unstable();
    group.dedup();
    if group.len() < 2 {
        return Err("give at least two different vertex indices".to_owned());
    }
    let points = weld_group(&base, &group, target);
    if points == base {
        return Ok(WeldReport::default());
    }
    let report = run_jobs(doc, vec![(id, points, base)], opts);
    match report.refused.first() {
        Some((_, why)) => Err(format!("welding would make the solid invalid: {why}")),
        None => Ok(report),
    }
}

pub fn weld_solids_by_distance(doc: &mut Document, ids: &[Id], distance: f64, opts: VertexEditOptions) -> WeldReport {
    let mut jobs = Vec::new();
    for id in ids {
        let Some((base, _)) = doc.solid_vertices(*id) else { continue };
        let points = weld_distance(&base, distance);
        if points != base {
            jobs.push((*id, points, base));
        }
    }
    run_jobs(doc, jobs, opts)
}

fn needs_cleanup(doc: &Document, id: Id) -> bool {
    doc.scene().solid(id).is_some_and(|n| n.solid.sides.len() != n.geom.faces.len())
}

pub fn cleanup_solids(doc: &mut Document, ids: &[Id], opts: VertexEditOptions) -> WeldReport {
    let mut jobs = Vec::new();
    for id in ids {
        let Some((base, _)) = doc.solid_vertices(*id) else { continue };
        let points = weld_distance(&base, CLEANUP_DISTANCE);
        if points != base || needs_cleanup(doc, *id) {
            jobs.push((*id, points, base));
        }
    }
    run_jobs(doc, jobs, opts)
}

fn kept_selection(s: &EditSolid, points: &[DVec3], out: &mut Reselect) {
    out.extend(s.selected_indices().into_iter().filter_map(|i| points.get(i).map(|p| (s.id, *p))));
}

impl VertexTool {
    pub(crate) fn selected_weld_count(&self) -> usize {
        self.cache
            .borrow()
            .solids
            .iter()
            .map(|s| {
                let group = s.selected_indices();
                if group.len() < 2 {
                    return 0;
                }
                welded_vertices(&s.points, &weld_group(&s.points, &group, self.weld_target))
            })
            .sum()
    }

    pub(crate) fn distance_weld_count(&mut self) -> usize {
        let cache = self.cache.get_mut();
        let key = (cache.generation, self.weld_distance.to_bits());
        if let Some((k, n)) = self.weld_memo
            && k == key
        {
            return n;
        }
        let distance = self.weld_distance;
        let n = cache.solids.iter().map(|s| welded_vertices(&s.points, &weld_distance(&s.points, distance))).sum();
        self.weld_memo = Some((key, n));
        n
    }

    pub(crate) fn weld_selected(&mut self, state: &mut AppState) -> bool {
        let target = self.weld_target;
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        let mut edits = Vec::new();
        let mut reselect: Reselect = Vec::new();
        let mut candidates = 0;
        for s in &cache.solids {
            let group = s.selected_indices();
            if group.len() < 2 {
                kept_selection(s, &s.points, &mut reselect);
                continue;
            }
            candidates += 1;
            let points = weld_group(&s.points, &group, target);
            kept_selection(s, &points, &mut reselect);
            if points != s.points {
                edits.push(SolidEdit { id: s.id, points, base: s.points.clone(), topology: s.topology() });
            }
        }
        if candidates == 0 {
            let text = t!("vertex.weld_need").to_owned();
            self.warn(state, text);
            return false;
        }
        if edits.is_empty() {
            state.notify(Severity::Info, t!("vertex.weld_nothing"));
            return false;
        }
        let welded: usize = edits.iter().map(|e| welded_vertices(&e.base, &e.points)).sum();
        if self.apply(state, &edits, reselect, UNDO_WELD, None) {
            state.notify(Severity::Info, tf!("vertex.weld_done", welded));
            return true;
        }
        false
    }

    fn run_on_edited(
        &mut self,
        state: &mut AppState,
        label: &str,
        plan: impl Fn(&Document, &EditSolid) -> Option<Vec<DVec3>>,
    ) -> bool {
        let cache = self.cache.get_mut();
        cache.refresh(&state.doc);
        let mut edits = Vec::new();
        let mut reselect: Reselect = Vec::new();
        let mut refused = 0usize;
        let mut first_error: Option<VertexError> = None;
        for s in &cache.solids {
            let Some(points) = plan(&state.doc, s) else {
                kept_selection(s, &s.points, &mut reselect);
                continue;
            };
            if let Err(e) = check(&state.doc, s.id, &points) {
                refused += 1;
                first_error.get_or_insert(e);
                kept_selection(s, &s.points, &mut reselect);
                continue;
            }
            kept_selection(s, &points, &mut reselect);
            edits.push(SolidEdit { id: s.id, points, base: s.points.clone(), topology: s.topology() });
        }
        if edits.is_empty() {
            match first_error {
                Some(e) => {
                    let text = tf!("vertex.cancelled", vertex_error_text(&e));
                    self.warn(state, text);
                }
                None => state.notify(Severity::Info, t!("vertex.weld_nothing")),
            }
            return false;
        }
        let welded: usize = edits.iter().map(|e| welded_vertices(&e.base, &e.points)).sum();
        let ids: Vec<Id> = edits.iter().map(|e| e.id).collect();
        let sides_before: usize = ids.iter().map(|id| side_count(&state.doc, *id)).sum();
        if !self.apply(state, &edits, reselect, label, None) {
            return false;
        }
        let sides_after: usize = ids.iter().map(|id| side_count(&state.doc, *id)).sum();
        let removed = sides_before.saturating_sub(sides_after);
        state.notify(Severity::Info, tf!("vertex.weld_done_report", welded, ids.len(), removed));
        if refused > 0 {
            let text = tf!("vertex.weld_refused", refused);
            self.warn(state, text);
        }
        true
    }

    pub(crate) fn weld_by_distance(&mut self, state: &mut AppState) -> bool {
        let distance = self.weld_distance;
        self.run_on_edited(state, UNDO_WELD_DISTANCE, |_, s| {
            let points = weld_distance(&s.points, distance);
            (points != s.points).then_some(points)
        })
    }

    pub(crate) fn cleanup_edited(&mut self, state: &mut AppState) -> bool {
        self.run_on_edited(state, UNDO_CLEANUP, |doc, s| {
            let points = weld_distance(&s.points, CLEANUP_DISTANCE);
            (points != s.points || needs_cleanup(doc, s.id)).then_some(points)
        })
    }
}
