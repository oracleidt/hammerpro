use std::collections::HashSet;
use std::ops::RangeInclusive;

use glam::{DMat3, DQuat, DVec3};
use hp_editor::{Document, Id};
use hp_geom::VertexEditOptions;

use super::loops::{EdgeCmd, Seed, scope_of, selection_in};
use super::model::{
    DragSolid, EditSolid, Preview, Reselect, SolidEdit, VertexCache, VertexError, apply_edits, drag_solids,
    preview_edits, previews_for, selected_moving,
};
use crate::common::vertex_error_text;

pub(crate) const ROTATE_RANGE: RangeInclusive<f64> = -360.0..=360.0;
pub(crate) const SLIDE_RANGE: RangeInclusive<f64> = -1.0..=1.0;
pub(crate) const UNDO_ROTATE: &str = "Rotate vertices";
pub(crate) const UNDO_SLIDE: &str = "Slide edge loop";
pub(crate) const UNDO_LOOP_MOVE: &str = "Move edge loop";
pub(crate) const UNDO_LOOP_SCALE: &str = "Scale edge loop";
pub(crate) const UNDO_LOOP_ROTATE: &str = "Rotate edge loop";
const PLAN_EPS: f64 = 0.02;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RotateAxis {
    X,
    Y,
    #[default]
    Z,
    Normal,
}

impl RotateAxis {
    pub const ALL: [RotateAxis; 4] = [RotateAxis::X, RotateAxis::Y, RotateAxis::Z, RotateAxis::Normal];

    pub fn name(self) -> &'static str {
        match self {
            RotateAxis::X => "x",
            RotateAxis::Y => "y",
            RotateAxis::Z => "z",
            RotateAxis::Normal => "normal",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == text.trim().to_ascii_lowercase())
    }
}

pub(crate) type PlanPoint = (DVec3, DVec3, DVec3);

#[derive(Clone, Debug, PartialEq)]
pub enum LoopOp {
    Move(DVec3),
    Scale { factor: f64, center: DVec3 },
    Rotate { axis: DVec3, degrees: f64, center: DVec3 },
    Slide { amount: f64 },
}

impl LoopOp {
    pub(crate) fn map(&self, plan: &[PlanPoint], p: DVec3) -> DVec3 {
        match self {
            LoopOp::Move(d) => p + *d,
            LoopOp::Scale { factor, center } => *center + (p - *center) * *factor,
            LoopOp::Rotate { axis, degrees, center } => {
                let axis = axis.try_normalize().unwrap_or(DVec3::Z);
                *center + DQuat::from_axis_angle(axis, degrees.to_radians()) * (p - *center)
            }
            LoopOp::Slide { amount } => match plan.iter().find(|(q, _, _)| q.distance(p) <= PLAN_EPS) {
                Some((_, up, _)) if *amount >= 0.0 => p + (*up - p) * *amount,
                Some((_, _, down)) => p + (*down - p) * -*amount,
                None => p,
            },
        }
    }
}

pub fn plane_normal(points: &[DVec3]) -> Option<DVec3> {
    if points.len() < 3 {
        return None;
    }
    let c = points.iter().copied().sum::<DVec3>() / points.len() as f64;
    let mut cov = DMat3::ZERO;
    for p in points {
        let d = *p - c;
        cov += DMat3::from_cols(d * d.x, d * d.y, d * d.z);
    }
    let power = |m: DMat3, start: DVec3| -> (DVec3, f64) {
        let mut v = start.try_normalize().unwrap_or(DVec3::X);
        let mut len = 0.0;
        for _ in 0..64 {
            let w = m * v;
            len = w.length();
            match w.try_normalize() {
                Some(n) => v = n,
                None => break,
            }
        }
        (v, len)
    };
    let far = points.iter().map(|p| *p - c).max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))?;
    let (v1, l1) = power(cov, far + DVec3::new(0.01, 0.02, 0.03));
    if l1 <= 1e-12 {
        return None;
    }
    let deflated = cov - DMat3::from_cols(v1 * (l1 * v1.x), v1 * (l1 * v1.y), v1 * (l1 * v1.z));
    let seed = v1.cross(DVec3::new(0.3, 0.5, 0.8)) + v1.any_orthonormal_vector() * 0.1;
    let (v2, l2) = power(deflated, seed);
    if l2 <= 1e-9 * l1 {
        return None;
    }
    v1.cross(v2).try_normalize()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlideIssue {
    Nothing,
    Branching,
}

pub(crate) fn slide_plan_of(scope: &hp_editor::LoopScope, edges: &[usize]) -> Result<Vec<PlanPoint>, SlideIssue> {
    if edges.is_empty() {
        return Err(SlideIssue::Nothing);
    }
    let mut sorted = edges.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let plan = scope.net.slide_plan(&sorted).ok_or(SlideIssue::Branching)?;
    let out: Vec<PlanPoint> =
        plan.iter().filter_map(|s| Some((*scope.net.points().get(s.vertex)?, s.up, s.down))).collect();
    if out.is_empty() { Err(SlideIssue::Nothing) } else { Ok(out) }
}

#[derive(Clone, Debug)]
pub(crate) struct XformState {
    pub op: LoopOp,
    pub axis: RotateAxis,
    pub normal: DVec3,
    pub plan: Vec<PlanPoint>,
    pub solids: Vec<DragSolid>,
    pub previews: Vec<Preview>,
    pub mask: Vec<bool>,
    pub generation: u64,
}

impl XformState {
    fn base(doc: &Document, cache: &VertexCache, op: LoopOp) -> Option<Self> {
        let (solids, mask) = drag_solids(doc, cache, selected_moving(cache));
        if solids.is_empty() {
            return None;
        }
        let mut s = Self {
            op,
            axis: RotateAxis::Z,
            normal: DVec3::Z,
            plan: Vec::new(),
            solids,
            previews: Vec::new(),
            mask,
            generation: cache.generation,
        };
        s.rebuild();
        Some(s)
    }

    pub fn rotate(doc: &Document, cache: &VertexCache) -> Option<Self> {
        let scope = scope_of(&cache.solids);
        let (verts, _) = selection_in(&scope, &cache.solids);
        let pts: Vec<DVec3> = verts.iter().filter_map(|v| scope.net.points().get(*v).copied()).collect();
        if pts.is_empty() {
            return None;
        }
        let center = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        let normal = plane_normal(&pts).unwrap_or(DVec3::Z);
        let mut s = Self::base(doc, cache, LoopOp::Rotate { axis: DVec3::Z, degrees: 0.0, center })?;
        s.normal = normal;
        Some(s)
    }

    pub fn slide(doc: &Document, cache: &VertexCache) -> Result<Self, SlideIssue> {
        let scope = scope_of(&cache.solids);
        let (_, edges) = selection_in(&scope, &cache.solids);
        let mut list: Vec<usize> = edges.into_iter().collect();
        list.sort_unstable();
        let plan = slide_plan_of(&scope, &list)?;
        let mut s = Self::base(doc, cache, LoopOp::Slide { amount: 0.0 }).ok_or(SlideIssue::Nothing)?;
        s.plan = plan;
        s.rebuild();
        Ok(s)
    }

    pub fn is_rotate(&self) -> bool {
        matches!(self.op, LoopOp::Rotate { .. })
    }

    pub fn value(&self) -> f64 {
        match self.op {
            LoopOp::Rotate { degrees, .. } => degrees,
            LoopOp::Slide { amount } => amount,
            _ => 0.0,
        }
    }

    pub fn range(&self) -> RangeInclusive<f64> {
        if self.is_rotate() { ROTATE_RANGE } else { SLIDE_RANGE }
    }

    pub fn set_value(&mut self, v: f64) -> bool {
        if !v.is_finite() {
            return false;
        }
        let r = self.range();
        let v = v.clamp(*r.start(), *r.end());
        if v == self.value() {
            return false;
        }
        match &mut self.op {
            LoopOp::Rotate { degrees, .. } => *degrees = v,
            LoopOp::Slide { amount } => *amount = v,
            _ => return false,
        }
        self.rebuild();
        true
    }

    pub fn center(&self) -> Option<DVec3> {
        match self.op {
            LoopOp::Rotate { center, .. } => Some(center),
            _ => None,
        }
    }

    pub fn set_center(&mut self, c: DVec3) {
        if !c.is_finite() {
            return;
        }
        if let LoopOp::Rotate { center, .. } = &mut self.op {
            *center = c;
            self.rebuild();
        }
    }

    pub fn set_axis(&mut self, a: RotateAxis) {
        self.axis = a;
        self.rebuild();
    }

    pub fn rebuild(&mut self) {
        if let LoopOp::Rotate { axis, .. } = &mut self.op {
            *axis = match self.axis {
                RotateAxis::X => DVec3::X,
                RotateAxis::Y => DVec3::Y,
                RotateAxis::Z => DVec3::Z,
                RotateAxis::Normal => self.normal,
            };
        }
        let (op, plan) = (self.op.clone(), self.plan.clone());
        self.previews = previews_for(&self.solids, 0.0, |p| op.map(&plan, p));
    }

    pub fn first_error(&self) -> Option<&VertexError> {
        self.previews.iter().find_map(|p| p.result.as_ref().err())
    }

    pub fn has_motion(&self) -> bool {
        self.value() != 0.0
    }

    pub fn edits(&self) -> (Vec<SolidEdit>, Reselect) {
        preview_edits(&self.solids, &self.previews)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoopSpec {
    Move(DVec3),
    Scale { factor: f64, center: Option<DVec3> },
    Rotate { axis: RotateAxis, degrees: f64, center: Option<DVec3> },
    Slide(f64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoopMoveReport {
    pub solids: usize,
    pub vertices: usize,
    pub lost_disps: usize,
}

pub fn transform_loop(
    doc: &mut Document,
    ids: &[Id],
    seeds: &[Seed],
    expand: Option<EdgeCmd>,
    toward: Option<DVec3>,
    spec: &LoopSpec,
    opts: VertexEditOptions,
) -> Result<LoopMoveReport, String> {
    let solids: Vec<EditSolid> =
        ids.iter().filter(|id| doc.is_solid_visible(**id)).filter_map(|id| EditSolid::from_doc(doc, *id)).collect();
    if solids.is_empty() {
        return Err("none of the solids can be edited".to_owned());
    }
    let scope = scope_of(&solids);
    let mut edges: Vec<usize> = Vec::new();
    for (_, a, b) in seeds {
        let e = scope.edge_at(*a, *b).ok_or("a seed edge is not an edge of the given solids")?;
        if !edges.contains(&e) {
            edges.push(e);
        }
    }
    if edges.is_empty() {
        return Err("give at least one seed edge".to_owned());
    }
    if let Some(cmd @ (EdgeCmd::Loop | EdgeCmd::Ring)) = expand {
        let kind = if cmd == EdgeCmd::Ring { hp_editor::RunKind::Ring } else { hp_editor::RunKind::Loop };
        let mut all: Vec<usize> = Vec::new();
        for &e in &edges {
            for x in scope.run(kind, e, toward).edges {
                if !all.contains(&x) {
                    all.push(x);
                }
            }
        }
        edges = all;
    }
    let verts: HashSet<usize> = scope.net.run_vertices(&edges).into_iter().collect();
    let pts: Vec<DVec3> = verts.iter().filter_map(|v| scope.net.points().get(*v).copied()).collect();
    let middle = pts.iter().copied().sum::<DVec3>() / pts.len().max(1) as f64;
    let op = match spec {
        LoopSpec::Move(d) => LoopOp::Move(*d),
        LoopSpec::Scale { factor, center } => LoopOp::Scale { factor: *factor, center: center.unwrap_or(middle) },
        LoopSpec::Rotate { axis, degrees, center } => {
            let dir = match axis {
                RotateAxis::X => DVec3::X,
                RotateAxis::Y => DVec3::Y,
                RotateAxis::Z => DVec3::Z,
                RotateAxis::Normal => plane_normal(&pts).ok_or("the loop vertices do not span a plane")?,
            };
            LoopOp::Rotate { axis: dir, degrees: *degrees, center: center.unwrap_or(middle) }
        }
        LoopSpec::Slide(amount) => LoopOp::Slide { amount: *amount },
    };
    let op = &op;
    let plan = if matches!(op, LoopOp::Slide { .. }) {
        slide_plan_of(&scope, &edges).map_err(|e| match e {
            SlideIssue::Nothing => "the loop has no edge to slide along".to_owned(),
            SlideIssue::Branching => "the edges branch: give separate loops".to_owned(),
        })?
    } else {
        Vec::new()
    };
    let mut edits = Vec::new();
    for (si, s) in solids.iter().enumerate() {
        let mut points = s.points.clone();
        let mut moved = false;
        for (i, p) in points.iter_mut().enumerate() {
            if scope.global_vertex(si, i).is_some_and(|g| verts.contains(&g)) {
                let q = op.map(&plan, *p);
                if q != *p {
                    *p = q;
                    moved = true;
                }
            }
        }
        if moved {
            edits.push(SolidEdit { id: s.id, points, base: s.points.clone(), topology: None });
        }
    }
    if edits.is_empty() {
        return Ok(LoopMoveReport::default());
    }
    let label = match op {
        LoopOp::Move(_) => UNDO_LOOP_MOVE,
        LoopOp::Scale { .. } => UNDO_LOOP_SCALE,
        LoopOp::Rotate { .. } => UNDO_LOOP_ROTATE,
        LoopOp::Slide { .. } => UNDO_SLIDE,
    };
    match apply_edits(doc, &edits, label, None, opts) {
        Ok(r) => Ok(LoopMoveReport { solids: r.solids, vertices: verts.len(), lost_disps: r.lost_disps }),
        Err((id, e)) => Err(format!("solid {id} would become invalid: {}", vertex_error_text(&e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_normal_of_a_flat_ring() {
        let ring: Vec<DVec3> = (0..8)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 8.0;
                DVec3::new(a.cos() * 20.0, a.sin() * 10.0, 5.0)
            })
            .collect();
        let n = plane_normal(&ring).unwrap();
        assert!(n.dot(DVec3::Z).abs() > 0.999, "{n:?}");
        assert!(plane_normal(&[DVec3::ZERO, DVec3::X]).is_none());
        let line: Vec<DVec3> = (0..5).map(|i| DVec3::new(f64::from(i), 0.0, 0.0)).collect();
        assert!(plane_normal(&line).is_none());
    }

    #[test]
    fn ops_map_points() {
        let p = DVec3::new(10.0, 0.0, 0.0);
        assert_eq!(LoopOp::Move(DVec3::Z).map(&[], p), DVec3::new(10.0, 0.0, 1.0));
        let r = LoopOp::Rotate { axis: DVec3::Z, degrees: 90.0, center: DVec3::ZERO }.map(&[], p);
        assert!(r.distance(DVec3::new(0.0, 10.0, 0.0)) < 1e-9);
        let s = LoopOp::Scale { factor: 2.0, center: DVec3::ZERO }.map(&[], p);
        assert_eq!(s, DVec3::new(20.0, 0.0, 0.0));
        let plan = [(p, DVec3::new(10.0, 0.0, 8.0), DVec3::new(10.0, 0.0, -8.0))];
        assert_eq!(LoopOp::Slide { amount: 0.5 }.map(&plan, p), DVec3::new(10.0, 0.0, 4.0));
        assert_eq!(LoopOp::Slide { amount: -0.5 }.map(&plan, p), DVec3::new(10.0, 0.0, -4.0));
        assert_eq!(LoopOp::Slide { amount: 0.5 }.map(&plan, DVec3::ZERO), DVec3::ZERO);
    }

    #[test]
    fn axis_names_round_trip() {
        for a in RotateAxis::ALL {
            assert_eq!(RotateAxis::parse(a.name()), Some(a));
        }
    }
}
