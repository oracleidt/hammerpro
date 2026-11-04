use egui::{Pos2, Rect};
use glam::{DVec2, DVec3};
use hp_app::grid::GridFrame;
use hp_app::{ViewInfo, ViewKind};
use hp_editor::{Document, EditError, Id};
use hp_geom::{
    FaceTopology, GeomError, PLANE_EPS, SolidGeom, VertexEdit, VertexEditOptions, build_solid, rebuild_solid,
};
use hp_vmf::Side;

use crate::common::{DocKey, ray_line_param, ray_plane};

pub(crate) const MAX_EDIT_SOLIDS: usize = 2048;
pub(crate) const VERTEX_HIT_PX: f32 = 7.0;
pub(crate) const EDGE_HIT_PX: f32 = 6.0;
pub(crate) const MIN_EDGE_PX: f32 = 16.0;
pub(crate) const SELECT_MATCH_EPS: f64 = 0.01;
pub(crate) const UNDO_EDIT: &str = "Edit vertices";
pub(crate) const UNDO_NUDGE: &str = "Nudge vertices";
pub(crate) const UNDO_SCALE: &str = "Scale vertices";
pub(crate) const NUDGE_TAG: &str = "hp_tools_mesh.vertex_nudge";

pub(crate) const KEPT_VERTEX_EPS: f64 = hp_geom::PLANE_EPS * 5.0;

#[derive(Clone, Debug)]
pub(crate) enum VertexError {
    Geom(GeomError),
    Edit(std::sync::Arc<EditError>),
    Concave(usize),
    NotFlat(usize),
}

impl From<GeomError> for VertexError {
    fn from(e: GeomError) -> Self {
        VertexError::Geom(e)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Topology {
    pub faces: Vec<Vec<usize>>,
    pub sides: Vec<usize>,
}

impl Topology {
    pub fn insert_on_edge(&mut self, a: usize, b: usize, new: usize) {
        insert_on_edge(&mut self.faces, a, b, new);
    }

    fn as_geom(&self) -> FaceTopology<'_> {
        FaceTopology { faces: &self.faces, templates: &self.sides }
    }
}

fn insert_on_edge(faces: &mut [Vec<usize>], a: usize, b: usize, new: usize) {
    for ring in faces {
        let n = ring.len();
        let pos = (0..n).find(|&i| {
            let (x, y) = (ring.get(i).copied(), ring.get((i + 1) % n).copied());
            (x == Some(a) && y == Some(b)) || (x == Some(b) && y == Some(a))
        });
        if let Some(i) = pos {
            ring.insert(i + 1, new);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SolidEdit {
    pub id: Id,
    pub points: Vec<DVec3>,
    pub base: Vec<DVec3>,
    pub topology: Option<Topology>,
}

impl SolidEdit {
    #[cfg(test)]
    pub fn points(id: Id, points: Vec<DVec3>) -> Self {
        Self { id, points, base: Vec::new(), topology: None }
    }

    fn as_geom(&self) -> VertexEdit<'_> {
        let base = (self.base.len() == self.points.len()).then_some(self.base.as_slice());
        VertexEdit { points: &self.points, base, topology: self.topology.as_ref().map(Topology::as_geom) }
    }
}

pub(crate) type Reselect = Vec<(Id, DVec3)>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditSolid {
    pub id: Id,
    pub points: Vec<DVec3>,
    pub edges: Vec<[usize; 2]>,
    pub selected: Vec<bool>,
    pub edge_selected: Vec<bool>,
    pub disp_faces: usize,
    pub faces: Vec<Vec<usize>>,
    pub face_sides: Vec<usize>,
    pub split: bool,
}

impl EditSolid {
    pub fn from_parts(id: Id, points: Vec<DVec3>, faces: Vec<Vec<usize>>, disp_faces: usize) -> Self {
        let face_sides = (0..faces.len()).collect();
        let mut s = Self {
            id,
            points,
            edges: Vec::new(),
            selected: Vec::new(),
            edge_selected: Vec::new(),
            disp_faces,
            faces,
            face_sides,
            split: false,
        };
        s.refresh_edges();
        s
    }

    pub fn from_doc(doc: &Document, id: Id) -> Option<Self> {
        let (points, faces) = doc.solid_vertices(id)?;
        let node = doc.scene().solid(id)?;
        let disp_faces = node.solid.sides.iter().filter(|s| s.disp.is_some()).count();
        let mut s = Self::from_parts(id, points, faces, disp_faces);
        s.face_sides = node.geom.faces.iter().map(|f| f.side_index).collect();
        if s.face_sides.len() != s.faces.len() {
            s.face_sides = (0..s.faces.len()).collect();
        }
        Some(s)
    }

    pub fn refresh_edges(&mut self) {
        let old: Vec<([usize; 2], bool)> =
            self.edges.iter().copied().zip(self.edge_selected.iter().copied()).filter(|(_, s)| *s).collect();
        self.edges = unique_edges(&self.faces);
        self.selected.resize(self.points.len(), false);
        self.edge_selected = self.edges.iter().map(|e| old.iter().any(|(o, _)| o == e)).collect();
    }

    pub fn midpoint(&self, edge: usize) -> Option<DVec3> {
        let [a, b] = *self.edges.get(edge)?;
        Some((*self.points.get(a)? + *self.points.get(b)?) * 0.5)
    }

    pub fn selected_indices(&self) -> Vec<usize> {
        self.selected.iter().enumerate().filter_map(|(i, s)| s.then_some(i)).collect()
    }

    pub fn selected_count(&self) -> usize {
        self.selected.iter().filter(|s| **s).count()
    }

    pub fn topology(&self) -> Option<Topology> {
        self.split.then(|| Topology { faces: self.faces.clone(), sides: self.face_sides.clone() })
    }

    fn locate_or_insert(&mut self, p: DVec3) -> Option<usize> {
        if let Some(i) = self.points.iter().position(|q| q.distance(p) < SELECT_MATCH_EPS) {
            return Some(i);
        }
        let [a, b] = *self.edges.iter().find(|[a, b]| {
            let (Some(&pa), Some(&pb)) = (self.points.get(*a), self.points.get(*b)) else { return false };
            hp_geom::point_segment_distance(p, pa, pb) < SELECT_MATCH_EPS
        })?;
        self.points.push(p);
        let new = self.points.len() - 1;
        insert_on_edge(&mut self.faces, a, b, new);
        self.refresh_edges();
        Some(new)
    }

    pub fn apply_split(&mut self, pa: DVec3, pb: DVec3) -> Result<[usize; 2], SplitError> {
        let mut s = self.clone();
        let ia = s.locate_or_insert(pa).ok_or(SplitError::NotOnSolid)?;
        let ib = s.locate_or_insert(pb).ok_or(SplitError::NotOnSolid)?;
        if ia == ib {
            return Err(SplitError::SamePoint);
        }
        let mut found_adjacent = false;
        let mut target: Option<(usize, usize, usize)> = None;
        for (fi, ring) in s.faces.iter().enumerate() {
            let (Some(x), Some(y)) = (ring.iter().position(|&i| i == ia), ring.iter().position(|&i| i == ib)) else {
                continue;
            };
            let n = ring.len();
            if (x + 1) % n == y || (y + 1) % n == x {
                found_adjacent = true;
                continue;
            }
            target = Some((fi, x, y));
            break;
        }
        let Some((fi, x, y)) = target else {
            return Err(if found_adjacent { SplitError::AlreadyAnEdge } else { SplitError::NotSameFace });
        };
        let ring = s.faces.get(fi).cloned().ok_or(SplitError::NotSameFace)?;
        let n = ring.len();
        let walk = |from: usize, to: usize| -> Vec<usize> {
            let mut out = Vec::with_capacity(n);
            let mut k = from;
            loop {
                if let Some(&v) = ring.get(k) {
                    out.push(v);
                }
                if k == to || out.len() > n {
                    break;
                }
                k = (k + 1) % n;
            }
            out
        };
        let (first, second) = (walk(x, y), walk(y, x));
        let side = s.face_sides.get(fi).copied().unwrap_or(fi);
        if let Some(slot) = s.faces.get_mut(fi) {
            *slot = first;
        }
        s.faces.push(second);
        s.face_sides.push(side);
        s.split = true;
        s.refresh_edges();
        *self = s;
        Ok([ia, ib])
    }

    pub fn split_points(&self) -> Vec<DVec3> {
        let mut out: Vec<DVec3> = Vec::new();
        let mut covered = vec![false; self.points.len()];
        for (e, sel) in self.edges.iter().zip(&self.edge_selected) {
            if !*sel {
                continue;
            }
            for &i in e {
                if let Some(c) = covered.get_mut(i) {
                    *c = true;
                }
            }
            if let (Some(a), Some(b)) = (self.points.get(e[0]), self.points.get(e[1])) {
                out.push((*a + *b) * 0.5);
            }
        }
        for (i, (p, sel)) in self.points.iter().zip(&self.selected).enumerate() {
            if *sel && !covered.get(i).copied().unwrap_or(false) {
                out.push(*p);
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitError {
    NotOnSolid,
    SamePoint,
    AlreadyAnEdge,
    NotSameFace,
}

pub(crate) fn unique_edges(faces: &[Vec<usize>]) -> Vec<[usize; 2]> {
    let mut edges: Vec<[usize; 2]> = Vec::with_capacity(faces.iter().map(Vec::len).sum::<usize>() / 2 + 1);
    for f in faces {
        let n = f.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            let (Some(&a), Some(&b)) = (f.get(i), f.get((i + 1) % n)) else { continue };
            if a != b {
                edges.push([a.min(b), a.max(b)]);
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();
    edges
}

pub(crate) type PendingSplit = (Id, DVec3, DVec3);

#[derive(Debug, Default)]
pub(crate) struct VertexCache {
    key: Option<DocKey>,
    pub generation: u64,
    pub solids: Vec<EditSolid>,
    pub truncated: bool,
    pub pending_select: Option<Vec<(Id, DVec3)>>,
    pub pending_edges: Option<Vec<(Id, DVec3)>>,
    pub splits: Vec<PendingSplit>,
    pub frozen: bool,
}

impl VertexCache {
    pub fn refresh(&mut self, doc: &Document) {
        if self.frozen || self.key == Some(DocKey::of(doc)) {
            return;
        }
        self.rebuild(doc);
    }

    pub fn invalidate(&mut self) {
        self.key = None;
    }

    fn rebuild(&mut self, doc: &Document) {
        let keep = self.pending_select.take().unwrap_or_else(|| self.selected_positions());
        let keep_edges = self.pending_edges.take().unwrap_or_else(|| self.selected_edge_midpoints());
        let mut ids = doc.selected_solid_ids();
        ids.retain(|id| doc.is_solid_visible(*id));
        self.truncated = ids.len() > MAX_EDIT_SOLIDS;
        ids.truncate(MAX_EDIT_SOLIDS);
        self.solids = ids.iter().filter_map(|id| EditSolid::from_doc(doc, *id)).collect();
        let splits = std::mem::take(&mut self.splits);
        for (id, a, b) in splits {
            let Some(s) = self.solids.iter_mut().find(|s| s.id == id) else { continue };
            if s.apply_split(a, b).is_ok() {
                self.splits.push((id, a, b));
            }
        }
        for s in &mut self.solids {
            for (i, p) in s.points.iter().enumerate() {
                if keep.iter().any(|(id, q)| *id == s.id && p.distance(*q) < SELECT_MATCH_EPS)
                    && let Some(flag) = s.selected.get_mut(i)
                {
                    *flag = true;
                }
            }
            for e in 0..s.edges.len() {
                let Some(m) = s.midpoint(e) else { continue };
                if keep_edges.iter().any(|(id, q)| *id == s.id && m.distance(*q) < SELECT_MATCH_EPS)
                    && let Some(flag) = s.edge_selected.get_mut(e)
                {
                    *flag = true;
                }
            }
        }
        self.key = Some(DocKey::of(doc));
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn selected_positions(&self) -> Vec<(Id, DVec3)> {
        let mut out = Vec::new();
        for s in &self.solids {
            for (p, sel) in s.points.iter().zip(&s.selected) {
                if *sel {
                    out.push((s.id, *p));
                }
            }
        }
        out
    }

    fn selected_edge_midpoints(&self) -> Vec<(Id, DVec3)> {
        let mut out = Vec::new();
        for s in &self.solids {
            for (e, sel) in s.edge_selected.iter().enumerate() {
                if *sel && let Some(m) = s.midpoint(e) {
                    out.push((s.id, m));
                }
            }
        }
        out
    }

    pub fn selected_count(&self) -> usize {
        self.solids.iter().map(EditSolid::selected_count).sum()
    }

    pub fn vertex_count(&self) -> usize {
        self.solids.iter().map(|s| s.points.len()).sum()
    }

    pub fn clear_selection(&mut self) {
        for s in &mut self.solids {
            s.selected.iter_mut().for_each(|f| *f = false);
            s.edge_selected.iter_mut().for_each(|f| *f = false);
        }
    }

    pub fn select_all(&mut self) {
        for s in &mut self.solids {
            s.selected.iter_mut().for_each(|f| *f = true);
        }
    }

    pub fn set_selected(&mut self, verts: &[(usize, usize)], value: bool) {
        for &(s, i) in verts {
            let Some(solid) = self.solids.get_mut(s) else { continue };
            if let Some(flag) = solid.selected.get_mut(i) {
                *flag = value;
            }
            if !value {
                for (e, sel) in solid.edges.iter().zip(solid.edge_selected.iter_mut()) {
                    if e.contains(&i) {
                        *sel = false;
                    }
                }
            }
        }
    }

    pub fn set_edges_selected(&mut self, edges: &[(usize, usize)], value: bool) {
        for &(s, e) in edges {
            let Some(solid) = self.solids.get_mut(s) else { continue };
            let Some([a, b]) = solid.edges.get(e).copied() else { continue };
            if let Some(flag) = solid.edge_selected.get_mut(e) {
                *flag = value;
            }
            for i in [a, b] {
                if let Some(flag) = solid.selected.get_mut(i) {
                    *flag = value;
                }
            }
            if !value {
                for (other, sel) in solid.edges.iter().zip(solid.edge_selected.iter_mut()) {
                    if other.contains(&a) || other.contains(&b) {
                        *sel = false;
                    }
                }
            }
        }
    }

    pub fn toggle_group(&mut self, verts: &[(usize, usize)]) {
        let all = verts.iter().all(|&(s, i)| self.is_selected(s, i));
        self.set_selected(verts, !all);
    }

    pub fn toggle_edges(&mut self, edges: &[(usize, usize)]) {
        let all = edges.iter().all(|&(s, e)| self.is_edge_selected(s, e));
        self.set_edges_selected(edges, !all);
    }

    pub fn is_selected(&self, solid: usize, index: usize) -> bool {
        self.solids.get(solid).and_then(|s| s.selected.get(index)).copied().unwrap_or(false)
    }

    pub fn is_edge_selected(&self, solid: usize, edge: usize) -> bool {
        self.solids.get(solid).and_then(|s| s.edge_selected.get(edge)).copied().unwrap_or(false)
    }

    pub fn point(&self, solid: usize, index: usize) -> Option<DVec3> {
        self.solids.get(solid).and_then(|s| s.points.get(index)).copied()
    }

    pub fn selection_center(&self) -> Option<DVec3> {
        let mut sum = DVec3::ZERO;
        let mut n = 0usize;
        for s in &self.solids {
            for (p, sel) in s.points.iter().zip(&s.selected) {
                if *sel {
                    sum += *p;
                    n += 1;
                }
            }
        }
        (n > 0).then(|| sum / n as f64)
    }

    pub fn transformed_selection(&self, f: impl Fn(DVec3) -> DVec3) -> (Vec<SolidEdit>, Reselect) {
        let mut edits = Vec::new();
        let mut reselect = Vec::new();
        for s in &self.solids {
            if !s.selected.iter().any(|f| *f) {
                continue;
            }
            let pts: Vec<DVec3> =
                s.points.iter().zip(&s.selected).map(|(p, sel)| if *sel { f(*p) } else { *p }).collect();
            if pts == s.points {
                continue;
            }
            reselect.extend(pts.iter().zip(&s.selected).filter(|(_, sel)| **sel).map(|(p, _)| (s.id, *p)));
            edits.push(SolidEdit { id: s.id, points: pts, base: s.points.clone(), topology: s.topology() });
        }
        (edits, reselect)
    }

    pub fn moved_selection(&self, delta: DVec3) -> (Vec<SolidEdit>, Reselect) {
        self.transformed_selection(|p| p + delta)
    }

    pub fn snapped_selection(&self, grid: &GridFrame) -> (Vec<SolidEdit>, Reselect) {
        self.transformed_selection(|p| grid.snap_point(p))
    }

    pub fn follow_splits(&mut self, edit: &SolidEdit) {
        if edit.base.len() != edit.points.len() {
            return;
        }
        let map = |p: DVec3| -> DVec3 {
            edit.base
                .iter()
                .position(|b| b.distance(p) < SELECT_MATCH_EPS)
                .and_then(|i| edit.points.get(i).copied())
                .unwrap_or(p)
        };
        for (id, a, b) in &mut self.splits {
            if *id == edit.id {
                *a = map(*a);
                *b = map(*b);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HandleRef {
    Vertex { solid: usize, index: usize },
    Edge { solid: usize, edge: usize },
}

fn screen(view: &ViewInfo, p: DVec3) -> Option<Pos2> {
    let s = view.world_to_screen(p)?;
    (s.x.is_finite() && s.y.is_finite()).then_some(s)
}

fn depth(view: &ViewInfo, p: DVec3) -> f64 {
    if view.is_2d() { 0.0 } else { (p - view.cam3d.pos).dot(view.cam3d.forward()) }
}

pub(crate) fn edge_visible(view: &ViewInfo, a: DVec3, b: DVec3) -> bool {
    match (screen(view, a), screen(view, b)) {
        (Some(sa), Some(sb)) => sa.distance(sb) >= MIN_EDGE_PX,
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pickable {
    pub vertices: bool,
    pub edges: bool,
}

pub(crate) fn pick_handle(solids: &[EditSolid], view: &ViewInfo, pos: Pos2, what: Pickable) -> Option<HandleRef> {
    let mut best: Option<(f32, f64, HandleRef)> = None;
    let better = |best: &Option<(f32, f64, HandleRef)>, d: f32, z: f64| match best {
        None => true,
        Some((bd, bz, _)) => d + 0.5 < *bd || ((d - *bd).abs() <= 0.5 && z < *bz),
    };
    if what.vertices {
        for (si, s) in solids.iter().enumerate() {
            for (i, p) in s.points.iter().enumerate() {
                let Some(sp) = screen(view, *p) else { continue };
                let d = sp.distance(pos);
                if d > VERTEX_HIT_PX {
                    continue;
                }
                let z = depth(view, *p);
                if better(&best, d, z) {
                    best = Some((d, z, HandleRef::Vertex { solid: si, index: i }));
                }
            }
        }
        if let Some((_, _, h)) = best {
            return Some(h);
        }
    }
    if !what.edges {
        return None;
    }
    for (si, s) in solids.iter().enumerate() {
        for (ei, [a, b]) in s.edges.iter().enumerate() {
            let (Some(&pa), Some(&pb)) = (s.points.get(*a), s.points.get(*b)) else { continue };
            if !edge_visible(view, pa, pb) {
                continue;
            }
            let m = (pa + pb) * 0.5;
            let Some(sp) = screen(view, m) else { continue };
            let d = sp.distance(pos);
            if d > EDGE_HIT_PX {
                continue;
            }
            let z = depth(view, m);
            if better(&best, d, z) {
                best = Some((d, z, HandleRef::Edge { solid: si, edge: ei }));
            }
        }
    }
    best.map(|(_, _, h)| h)
}

fn overlap_eps(view: &ViewInfo) -> f64 {
    (0.5 / view.cam2d.zoom.max(1e-9)).clamp(0.01, 0.5)
}

pub(crate) fn vertex_group(solids: &[EditSolid], view: &ViewInfo, solid: usize, index: usize) -> Vec<(usize, usize)> {
    let Some(p) = solids.get(solid).and_then(|s| s.points.get(index)).copied() else { return Vec::new() };
    if !view.is_2d() {
        return vec![(solid, index)];
    }
    let p2 = view.kind.to_2d(p);
    let eps = overlap_eps(view);
    let mut out = Vec::new();
    for (si, s) in solids.iter().enumerate() {
        for (i, q) in s.points.iter().enumerate() {
            if view.kind.to_2d(*q).distance(p2) <= eps {
                out.push((si, i));
            }
        }
    }
    out
}

pub(crate) fn edge_group(solids: &[EditSolid], view: &ViewInfo, solid: usize, edge: usize) -> Vec<(usize, usize)> {
    let Some(m) = solids.get(solid).and_then(|s| s.midpoint(edge)) else { return Vec::new() };
    if !view.is_2d() {
        return vec![(solid, edge)];
    }
    let m2 = view.kind.to_2d(m);
    let eps = overlap_eps(view);
    let mut out = Vec::new();
    for (si, s) in solids.iter().enumerate() {
        for (ei, [a, b]) in s.edges.iter().enumerate() {
            let (Some(&pa), Some(&pb)) = (s.points.get(*a), s.points.get(*b)) else { continue };
            if !edge_visible(view, pa, pb) {
                continue;
            }
            if view.kind.to_2d((pa + pb) * 0.5).distance(m2) <= eps {
                out.push((si, ei));
            }
        }
    }
    out
}

pub(crate) fn vertices_in_rect(solids: &[EditSolid], view: &ViewInfo, rect: Rect) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (si, s) in solids.iter().enumerate() {
        for (i, p) in s.points.iter().enumerate() {
            if screen(view, *p).is_some_and(|sp| rect.contains(sp)) {
                out.push((si, i));
            }
        }
    }
    out
}

pub(crate) fn edges_in_rect(solids: &[EditSolid], view: &ViewInfo, rect: Rect) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (si, s) in solids.iter().enumerate() {
        for (ei, [a, b]) in s.edges.iter().enumerate() {
            let (Some(&pa), Some(&pb)) = (s.points.get(*a), s.points.get(*b)) else { continue };
            if edge_visible(view, pa, pb) && screen(view, (pa + pb) * 0.5).is_some_and(|sp| rect.contains(sp)) {
                out.push((si, ei));
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn moved_points(base: &[DVec3], moving: &[usize], delta: DVec3, merge_tolerance: f64) -> Vec<DVec3> {
    let mut pts = base.to_vec();
    for &i in moving {
        if let Some(p) = pts.get_mut(i) {
            *p += delta;
        }
    }
    merge_moved(&mut pts, moving, merge_tolerance);
    pts
}

fn merge_moved(pts: &mut [DVec3], moving: &[usize], merge_tolerance: f64) {
    if !(merge_tolerance > 0.0 && merge_tolerance.is_finite()) {
        return;
    }
    for &i in moving {
        let Some(p) = pts.get(i).copied() else { continue };
        let mut best: Option<(f64, DVec3)> = None;
        for (j, q) in pts.iter().enumerate() {
            if j == i || moving.contains(&j) {
                continue;
            }
            let d = p.distance(*q);
            if d <= merge_tolerance && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, *q));
            }
        }
        if let (Some((_, q)), Some(slot)) = (best, pts.get_mut(i)) {
            *slot = q;
        }
    }
}

#[cfg(test)]
pub(crate) fn snap_delta(anchor: DVec3, raw: DVec3, grid: Option<f64>, mask: [bool; 3]) -> DVec3 {
    let frame = grid.filter(|g| *g > 0.0 && g.is_finite()).map(GridFrame::uniform);
    snap_delta_on(anchor, raw, frame.as_ref(), mask, None)
}

pub(crate) fn snap_delta_on(
    anchor: DVec3,
    raw: DVec3,
    frame: Option<&GridFrame>,
    mask: [bool; 3],
    plane: Option<ViewKind>,
) -> DVec3 {
    let mut out = DVec3::ZERO;
    let Some(f) = frame else {
        for k in 0..3 {
            if mask[k] && raw[k].is_finite() {
                out[k] = raw[k];
            }
        }
        return out;
    };
    if f.is_rotated() && raw.is_finite() {
        if let Some(kind) = plane
            && let Some((r, u, _)) = kind.axes_2d()
        {
            let d = f.snap_delta_2d(kind, DVec2::new(anchor[r], anchor[u]), DVec2::new(raw[r], raw[u]));
            out[r] = d.x;
            out[u] = d.y;
            return out;
        }
        if mask == [true; 3] {
            return f.snap_delta(anchor, raw);
        }
    }
    for k in 0..3 {
        if !mask[k] || !raw[k].is_finite() {
            continue;
        }
        let g = f.axis(k);
        out[k] = if raw[k].abs() < g.step * 0.5 { 0.0 } else { g.snap(anchor[k] + raw[k]) - anchor[k] };
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DragKind {
    View2d(ViewKind),
    Plane { normal: DVec3 },
    Axis(usize),
}

impl DragKind {
    pub fn plane(self) -> Option<ViewKind> {
        match self {
            DragKind::View2d(kind) if kind.is_2d() => Some(kind),
            _ => None,
        }
    }

    pub fn mask(self) -> [bool; 3] {
        match self {
            DragKind::View2d(kind) => match kind.axes_2d() {
                Some((r, u, _)) => {
                    let mut m = [false; 3];
                    m[r] = true;
                    m[u] = true;
                    m
                }
                None => [true; 3],
            },
            DragKind::Plane { .. } => [true; 3],
            DragKind::Axis(a) => {
                let mut m = [false; 3];
                if let Some(slot) = m.get_mut(a) {
                    *slot = true;
                }
                m
            }
        }
    }
}

pub(crate) fn axis_dir(axis: usize) -> DVec3 {
    match axis {
        0 => DVec3::X,
        1 => DVec3::Y,
        _ => DVec3::Z,
    }
}

pub(crate) fn drag_point(kind: DragKind, view: &ViewInfo, anchor: DVec3, pos: Pos2) -> Option<DVec3> {
    match kind {
        DragKind::View2d(k) => {
            let p: DVec2 = view.screen_to_2d(pos);
            p.is_finite().then(|| k.from_2d(p, 0.0))
        }
        DragKind::Plane { normal } => {
            let (o, d) = view.ray(pos);
            ray_plane(o, d, anchor, normal)
        }
        DragKind::Axis(a) => {
            let (o, d) = view.ray(pos);
            let dir = axis_dir(a);
            ray_line_param(o, d, anchor, dir).map(|s| anchor + dir * s)
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DragSolid {
    pub id: Id,
    pub base: Vec<DVec3>,
    pub moving: Vec<usize>,
    pub template: Vec<Side>,
    pub topology: Option<Topology>,
}

#[derive(Clone, Debug)]
pub(crate) struct Preview {
    pub points: Vec<DVec3>,
    pub result: Result<Vec<[DVec3; 2]>, VertexError>,
}

pub(crate) fn dropped_points(points: &[DVec3], geom: &SolidGeom) -> usize {
    let corners: Vec<DVec3> = geom.faces.iter().flat_map(|f| f.vertices.iter().copied()).collect();
    let mut distinct: Vec<DVec3> = Vec::with_capacity(points.len());
    for p in points {
        if !distinct.iter().any(|q| q.distance(*p) <= hp_geom::WELD_EPS) {
            distinct.push(*p);
        }
    }
    distinct.iter().filter(|p| !corners.iter().any(|c| c.distance(**p) <= KEPT_VERTEX_EPS)).count()
}

fn extra_corners(points: &[DVec3], geom: &SolidGeom) -> usize {
    let mut seen: Vec<DVec3> = Vec::new();
    for c in geom.faces.iter().flat_map(|f| f.vertices.iter().copied()) {
        if !points.iter().any(|p| p.distance(c) <= KEPT_VERTEX_EPS)
            && !seen.iter().any(|q| q.distance(c) <= KEPT_VERTEX_EPS)
        {
            seen.push(c);
        }
    }
    seen.len()
}

fn non_flat_faces(points: &[DVec3], topology: &Topology) -> usize {
    topology
        .faces
        .iter()
        .filter(|ring| {
            let poly: Vec<DVec3> = ring.iter().filter_map(|&i| points.get(i).copied()).collect();
            let a = hp_geom::polygon_area_vector(&poly);
            let len = a.length();
            if len.is_nan() || len <= 1e-6 {
                return false;
            }
            let n = a / len;
            let d = poly.iter().map(|p| n.dot(*p)).sum::<f64>() / poly.len().max(1) as f64;
            poly.iter().any(|p| (n.dot(*p) - d).abs() > PLANE_EPS)
        })
        .count()
}

pub(crate) fn preview_solid(
    points: &[DVec3],
    template: &[Side],
    topology: Option<&Topology>,
) -> Result<Vec<[DVec3; 2]>, VertexError> {
    if points.iter().any(|p| !p.is_finite()) {
        return Err(GeomError::NotFinite.into());
    }
    if let Some(t) = topology {
        let bent = non_flat_faces(points, t);
        if bent > 0 {
            return Err(VertexError::NotFlat(bent));
        }
    }
    let mut next_id = || 0u32;
    let edit = VertexEdit { points, base: None, topology: topology.map(Topology::as_geom) };
    let sides = rebuild_solid(&edit, template, VertexEditOptions::default(), &mut next_id)?;
    let geom = build_solid(&sides);
    if !geom.valid {
        return Err(GeomError::NoVolume.into());
    }
    let lost = dropped_points(points, &geom) + if topology.is_some() { extra_corners(points, &geom) } else { 0 };
    if lost > 0 {
        return Err(VertexError::Concave(lost));
    }
    Ok(match topology {
        Some(t) => unique_edges(&t.faces)
            .into_iter()
            .filter_map(|[a, b]| Some([*points.get(a)?, *points.get(b)?]))
            .filter(|[a, b]| a.distance(*b) > hp_geom::WELD_EPS)
            .collect(),
        None => geom.edges(),
    })
}

#[cfg(test)]
pub(crate) fn preview_edges(points: &[DVec3], template: &[Side]) -> Result<Vec<[DVec3; 2]>, VertexError> {
    preview_solid(points, template, None)
}

pub(crate) fn previews_for(solids: &[DragSolid], merge_tolerance: f64, f: impl Fn(DVec3) -> DVec3) -> Vec<Preview> {
    solids
        .iter()
        .map(|s| {
            let mut points = s.base.clone();
            for &i in &s.moving {
                if let Some(p) = points.get_mut(i) {
                    *p = f(*p);
                }
            }
            merge_moved(&mut points, &s.moving, merge_tolerance);
            let result = preview_solid(&points, &s.template, s.topology.as_ref());
            Preview { points, result }
        })
        .collect()
}

pub(crate) fn preview_edits(solids: &[DragSolid], previews: &[Preview]) -> (Vec<SolidEdit>, Reselect) {
    let mut edits = Vec::with_capacity(solids.len());
    let mut reselect = Vec::new();
    for (s, p) in solids.iter().zip(previews) {
        reselect.extend(s.moving.iter().filter_map(|&i| p.points.get(i).map(|q| (s.id, *q))));
        edits.push(SolidEdit {
            id: s.id,
            points: p.points.clone(),
            base: s.base.clone(),
            topology: s.topology.clone(),
        });
    }
    (edits, reselect)
}

pub(crate) type MovingSolid = (usize, Vec<DVec3>, Vec<usize>, Option<Topology>);

pub(crate) fn drag_solids(
    doc: &Document,
    cache: &VertexCache,
    moving: Vec<MovingSolid>,
) -> (Vec<DragSolid>, Vec<bool>) {
    let mut solids = Vec::with_capacity(moving.len());
    let mut mask = vec![false; cache.solids.len()];
    for (ci, base, idx, topology) in moving {
        let Some(s) = cache.solids.get(ci) else { continue };
        if idx.is_empty() {
            continue;
        }
        let template = doc.scene().solid(s.id).map(|n| n.solid.sides.clone()).unwrap_or_default();
        if let Some(m) = mask.get_mut(ci) {
            *m = true;
        }
        solids.push(DragSolid { id: s.id, base, moving: idx, template, topology });
    }
    (solids, mask)
}

pub(crate) fn selected_moving(cache: &VertexCache) -> Vec<MovingSolid> {
    cache
        .solids
        .iter()
        .enumerate()
        .filter_map(|(ci, s)| {
            let idx = s.selected_indices();
            (!idx.is_empty()).then(|| (ci, s.points.clone(), idx, s.topology()))
        })
        .collect()
}

#[derive(Clone, Debug)]
pub(crate) struct VertexDrag {
    pub view: usize,
    pub kind: DragKind,
    pub anchor: DVec3,
    pub start: DVec3,
    pub delta: DVec3,
    pub solids: Vec<DragSolid>,
    pub previews: Vec<Preview>,
    pub dragged_mask: Vec<bool>,
    pub split: bool,
    pub generation: u64,
}

impl VertexDrag {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        doc: &Document,
        cache: &VertexCache,
        view: &ViewInfo,
        kind: DragKind,
        anchor: DVec3,
        origin: Pos2,
        moving: Vec<MovingSolid>,
        split: bool,
    ) -> Option<Self> {
        let start = drag_point(kind, view, anchor, origin)?;
        let (solids, dragged_mask) = drag_solids(doc, cache, moving);
        if solids.is_empty() {
            return None;
        }
        let mut drag = Self {
            view: view.index,
            kind,
            anchor,
            start,
            delta: DVec3::ZERO,
            solids,
            previews: Vec::new(),
            dragged_mask,
            split,
            generation: cache.generation,
        };
        drag.rebuild_previews(0.0);
        Some(drag)
    }

    pub fn raw_delta(&self, view: &ViewInfo, pos: Pos2) -> Option<DVec3> {
        let p = drag_point(self.kind, view, self.anchor, pos)?;
        let d = p - self.start;
        d.is_finite().then_some(d)
    }

    pub fn update(&mut self, view: &ViewInfo, pos: Pos2, grid: Option<GridFrame>, merge_tolerance: f64) -> bool {
        let Some(raw) = self.raw_delta(view, pos) else { return false };
        let delta = snap_delta_on(self.anchor, raw, grid.as_ref(), self.kind.mask(), self.kind.plane());
        self.set_delta(delta, merge_tolerance)
    }

    pub fn set_delta(&mut self, delta: DVec3, merge_tolerance: f64) -> bool {
        if delta == self.delta && !self.previews.is_empty() {
            return false;
        }
        self.delta = delta;
        self.rebuild_previews(merge_tolerance);
        true
    }

    fn rebuild_previews(&mut self, merge_tolerance: f64) {
        let delta = self.delta;
        self.previews = previews_for(&self.solids, merge_tolerance, |p| p + delta);
    }

    pub fn first_error(&self) -> Option<&VertexError> {
        self.previews.iter().find_map(|p| p.result.as_ref().err())
    }

    pub fn has_motion(&self) -> bool {
        self.delta != DVec3::ZERO
    }

    pub fn edits(&self) -> (Vec<SolidEdit>, Reselect) {
        preview_edits(&self.solids, &self.previews)
    }

    pub fn moving_count(&self) -> usize {
        self.solids.iter().map(|s| s.moving.len()).sum()
    }
}

pub(crate) const LIVE_MAX_SOLIDS: usize = 64;

pub(crate) fn sync_live(
    doc: &mut Document,
    solids: &[DragSolid],
    previews: &[Preview],
    opts: VertexEditOptions,
) -> bool {
    let apply = previews.len() == solids.len() && previews.iter().all(|p| p.result.is_ok());
    for (i, s) in solids.iter().enumerate() {
        let result = match previews.get(i).filter(|_| apply) {
            Some(p) => {
                let edit = VertexEdit {
                    points: &p.points,
                    base: (s.base.len() == p.points.len()).then_some(s.base.as_slice()),
                    topology: s.topology.as_ref().map(Topology::as_geom),
                };
                doc.edit_solid_vertices_from(s.id, &edit, &s.template, opts)
            }
            None => doc.edit_solid(s.id, |solid| solid.sides.clone_from(&s.template)).map(|_| ()),
        };
        if result.is_err() {
            return false;
        }
    }
    true
}

pub(crate) fn template_disps(solids: &[DragSolid]) -> usize {
    solids.iter().map(|s| s.template.iter().filter(|t| t.disp.is_some()).count()).sum()
}

pub(crate) const SCALE_RANGE: std::ops::RangeInclusive<f64> = 0.0..=100.0;

#[derive(Clone, Debug)]
pub(crate) struct ScaleState {
    pub center: DVec3,
    pub factor: f64,
    pub solids: Vec<DragSolid>,
    pub previews: Vec<Preview>,
    pub mask: Vec<bool>,
    pub generation: u64,
    pub dragging: Option<CenterDrag>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CenterDrag {
    pub view: usize,
    pub kind: DragKind,
    pub center: DVec3,
    pub start: DVec3,
}

impl ScaleState {
    pub fn new(doc: &Document, cache: &VertexCache) -> Option<Self> {
        let selected = cache.selected_positions();
        let first = selected.first()?.1;
        let (lo, hi) = selected.iter().fold((first, first), |(lo, hi), (_, p)| (lo.min(*p), hi.max(*p)));
        let center = (lo + hi) * 0.5;
        let (solids, mask) = drag_solids(doc, cache, selected_moving(cache));
        if solids.is_empty() {
            return None;
        }
        let mut s = Self {
            center,
            factor: 1.0,
            solids,
            previews: Vec::new(),
            mask,
            generation: cache.generation,
            dragging: None,
        };
        s.rebuild();
        Some(s)
    }

    pub fn rebuild(&mut self) {
        self.factor =
            if self.factor.is_finite() { self.factor.clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end()) } else { 1.0 };
        let (c, f) = (self.center, self.factor);
        self.previews = previews_for(&self.solids, 0.0, |p| c + (p - c) * f);
    }

    pub fn set_factor(&mut self, factor: f64) -> bool {
        if !factor.is_finite() || factor == self.factor {
            return false;
        }
        self.factor = factor;
        self.rebuild();
        true
    }

    pub fn set_center(&mut self, center: DVec3) -> bool {
        if !center.is_finite() || center == self.center {
            return false;
        }
        self.center = center;
        self.rebuild();
        true
    }

    pub fn first_error(&self) -> Option<&VertexError> {
        self.previews.iter().find_map(|p| p.result.as_ref().err())
    }

    pub fn has_motion(&self) -> bool {
        self.factor != 1.0
    }

    pub fn edits(&self) -> (Vec<SolidEdit>, Reselect) {
        preview_edits(&self.solids, &self.previews)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ApplyReport {
    pub solids: usize,
    pub lost_disps: usize,
}

pub(crate) fn apply_edits(
    doc: &mut Document,
    edits: &[SolidEdit],
    label: &str,
    merge_tag: Option<&str>,
    opts: VertexEditOptions,
) -> Result<ApplyReport, (Id, VertexError)> {
    if edits.is_empty() {
        return Ok(ApplyReport { solids: 0, lost_disps: 0 });
    }
    for e in edits {
        let Some(node) = doc.scene().solid(e.id) else {
            let err = EditError::NotFound(hp_editor::ObjRef::Solid(e.id));
            return Err((e.id, VertexError::Edit(std::sync::Arc::new(err))));
        };
        if let Err(err) = preview_solid(&e.points, &node.solid.sides, e.topology.as_ref()) {
            return Err((e.id, err));
        }
    }
    let count_disps = |doc: &Document, id: Id| {
        doc.scene().solid(id).map_or(0, |n| n.solid.sides.iter().filter(|s| s.disp.is_some()).count())
    };
    let before: usize = edits.iter().map(|e| count_disps(doc, e.id)).sum();
    match merge_tag {
        Some(tag) => doc.begin_merge(label, tag),
        None => doc.begin(label),
    }
    for e in edits {
        if let Err(err) = doc.edit_solid_vertices(e.id, &e.as_geom(), opts) {
            doc.cancel();
            return Err((e.id, VertexError::Edit(std::sync::Arc::new(err))));
        }
    }
    doc.commit();
    let after: usize = edits.iter().map(|e| count_disps(doc, e.id)).sum();
    Ok(ApplyReport { solids: edits.len(), lost_disps: before.saturating_sub(after) })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NudgeKey {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
}

impl NudgeKey {
    pub fn from_key(k: egui::Key) -> Option<Self> {
        Some(match k {
            egui::Key::ArrowLeft => NudgeKey::Left,
            egui::Key::ArrowRight => NudgeKey::Right,
            egui::Key::ArrowUp => NudgeKey::Up,
            egui::Key::ArrowDown => NudgeKey::Down,
            egui::Key::PageUp => NudgeKey::PageUp,
            egui::Key::PageDown => NudgeKey::PageDown,
            _ => return None,
        })
    }
}

fn dominant_horizontal(v: DVec3) -> DVec3 {
    if v.x.abs() >= v.y.abs() { DVec3::new(v.x.signum(), 0.0, 0.0) } else { DVec3::new(0.0, v.y.signum(), 0.0) }
}

pub(crate) fn nudge_delta(view: &ViewInfo, key: NudgeKey, step: f64) -> DVec3 {
    let (right, up, depth) = match view.kind.axes_2d() {
        Some((r, u, _)) => (axis_dir(r), axis_dir(u), view.kind.toward_viewer()),
        None => {
            let cam = &view.cam3d;
            let fwd = cam.forward();
            let flat = DVec3::new(fwd.x, fwd.y, 0.0);
            let up = cam.up();
            let ahead = if flat.length() > 0.2 { flat } else { DVec3::new(up.x, up.y, 0.0) };
            let r = dominant_horizontal(cam.right());
            let mut a = dominant_horizontal(ahead);
            if a.dot(r).abs() > 0.5 {
                a = DVec3::new(-r.y, r.x, 0.0);
            }
            (r, a, DVec3::Z)
        }
    };
    let dir = match key {
        NudgeKey::Left => -right,
        NudgeKey::Right => right,
        NudgeKey::Up => up,
        NudgeKey::Down => -up,
        NudgeKey::PageUp => depth,
        NudgeKey::PageDown => -depth,
    };
    dir * step
}
