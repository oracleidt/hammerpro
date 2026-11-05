use std::cell::RefCell;

use egui::{Align2, CursorIcon, Key, PointerButton, Pos2, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::{Action, key_label, shortcut_label};
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, t, tf};
use hp_editor::{EdgeRef, EditError, Id, RunKind};
use hp_geom::{
    Aabb, BevelIssue, BevelKind, BevelParams, GeomError, MAX_BEVEL_RADIUS, MAX_BEVEL_SEGMENTS, MIN_BEVEL_RADIUS, Ray,
    point_segment_distance_2d,
};
use hp_render::Overlay;

use crate::common::{
    COLOR_FRONT, COLOR_GHOST, COLOR_PENDING, Mouse, Ov, Snap, active_material, clean, consume_plain_key, fmt_len,
    fresh, material_chip, paint_label, pick_surface, rgba,
};

pub const EDGE_TOLERANCE: f32 = 8.0;

const NEAR_HIT: f64 = 2.0;

const SELECTED_FIRST: f64 = 1e7;

const MOVE_DEADZONE: f32 = 3.0;

#[derive(Clone, Debug, PartialEq)]
pub struct BevelOptions {
    pub radius: f64,
    pub segments: u32,
    pub inverse: bool,
    pub current_material: bool,
}

impl Default for BevelOptions {
    fn default() -> Self {
        Self { radius: 16.0, segments: 4, inverse: false, current_material: false }
    }
}

#[derive(Clone, Copy, Debug)]
struct Hover {
    view: ViewInfo,
    pos: Pos2,
    edge: Option<EdgeRef>,
    frame: u64,
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    view: usize,
    pos: Pos2,
    per_px: f64,
    radius: f64,
}

#[derive(Clone, Debug)]
struct Op {
    edges: Vec<EdgeRef>,
    anchor: Option<Anchor>,
    limit: f64,
    revision: u64,
}

#[derive(Clone, Debug)]
struct RunHover {
    edge: EdgeRef,
    ring: bool,
    revision: u64,
    edges: Vec<EdgeRef>,
}

type PreviewSolid<'a> = (&'a [hp_vmf::Side], Option<&'a [Option<usize>]>);

#[derive(Debug, Default)]
struct Preview {
    key: Option<u64>,
    edges: Vec<[DVec3; 2]>,
    tris: Vec<[DVec3; 3]>,
    radius: f64,
    segments: u32,
    kinds: Vec<BevelKind>,
    brushes: usize,
    error: Option<String>,
}

pub struct BevelTool {
    pub options: BevelOptions,
    picked: Vec<EdgeRef>,
    picked_revision: u64,
    op: Option<Op>,
    hover: Option<Hover>,
    wheel: f32,
    preview: RefCell<Preview>,
    run_hover: Option<RunHover>,
}

impl Default for BevelTool {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn issue_text(e: &GeomError) -> String {
    let key = match BevelIssue::of(e) {
        Some(BevelIssue::NoEdge) => "bevel.err_no_edge",
        Some(BevelIssue::NotAnEdge) => "bevel.err_not_edge",
        Some(BevelIssue::Displacement) => "bevel.err_disp",
        Some(BevelIssue::NoRadius) => "bevel.err_radius",
        Some(BevelIssue::Flat) => "bevel.err_flat",
        Some(BevelIssue::NoRoom) => "bevel.err_room",
        Some(BevelIssue::Failed) => "bevel.err_failed",
        None => return e.to_string(),
    };
    t!(key).to_owned()
}

fn edit_text(e: &EditError) -> String {
    match e {
        EditError::Geom(g) => issue_text(g),
        other => other.to_string(),
    }
}

fn kind_text(kind: BevelKind, segments: u32) -> &'static str {
    match kind {
        BevelKind::Round if segments == 1 => t!("bevel.kind_chamfer"),
        BevelKind::Round => t!("bevel.kind_round"),
        BevelKind::Cove => t!("bevel.kind_cove"),
        BevelKind::Fill => t!("bevel.kind_fill"),
    }
}

fn hash_f64s(values: impl IntoIterator<Item = f64>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in values {
        h ^= v.to_bits();
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn screen(p: Pos2) -> DVec2 {
    DVec2::new(f64::from(p.x), f64::from(p.y))
}

fn segment_px(view: &ViewInfo, a: DVec3, b: DVec3, pos: Pos2) -> Option<f64> {
    let (sa, sb) = (view.world_to_screen(a)?, view.world_to_screen(b)?);
    Some(point_segment_distance_2d(screen(pos), screen(sa), screen(sb)))
}

fn ray_segment_t(o: DVec3, dir: DVec3, a: DVec3, b: DVec3) -> f64 {
    let v = b - a;
    let w0 = o - a;
    let (bb, c, d, e) = (dir.dot(v), v.dot(v), dir.dot(w0), v.dot(w0));
    let den = c - bb * bb;
    let s = if den.abs() > 1e-12 { ((e - bb * d) / den).clamp(0.0, 1.0) } else { 0.0 };
    (a + v * s - o).dot(dir)
}

fn edge_mid(e: &EdgeRef) -> DVec3 {
    (e.a + e.b) * 0.5
}

pub fn edge_exists(state: &AppState, e: &EdgeRef) -> bool {
    state
        .doc
        .scene()
        .solid(e.solid)
        .is_some_and(|n| n.geom.valid && n.geom.edges().iter().any(|[a, b]| EdgeRef::new(e.solid, *a, *b).same_edge(e)))
}

fn resolve(
    state: &AppState,
    cands: Vec<(f64, f64, EdgeRef)>,
    inverse: bool,
    toward: &dyn Fn(&EdgeRef) -> DVec3,
) -> Option<EdgeRef> {
    let best = cands.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let ties: Vec<(f64, EdgeRef)> = cands.into_iter().filter(|c| c.0 <= best + 1.0).map(|c| (c.1, c.2)).collect();
    if ties.len() < 2 {
        return ties.first().map(|t| t.1);
    }
    let score = |depth: f64, e: &EdgeRef| -> (f64, f64) {
        let facing = state.doc.bevel_opening(e, inverse).map_or(-2.0, |o| o.dot(toward(e)));
        ((facing * 1000.0).round(), depth)
    };
    ties.iter()
        .map(|(depth, e)| (score(*depth, e), *e))
        .max_by(|a, b| a.0.0.total_cmp(&b.0.0).then(a.0.1.total_cmp(&b.0.1)))
        .map(|(_, e)| e)
}

pub fn edge_at(state: &AppState, view: &ViewInfo, pos: Pos2, inverse: bool) -> Option<EdgeRef> {
    if view.is_2d() {
        return edge_at_2d(state, view, pos, inverse);
    }
    let (o, dir) = view.ray(pos);
    if !o.is_finite() || !dir.is_finite() {
        return None;
    }
    let hit = pick_surface(state, view, pos);
    let mut pool: Vec<Id> = state.doc.selected_solid_ids();
    let t_hit = hit.map(|h| (h.point - o).dot(dir));
    if let Some(h) = &hit {
        pool.push(h.solid);
    }
    let ray = Ray::new(o, dir);
    let zone = hit.map(|h| Aabb::new(h.point, h.point).expand(NEAR_HIT));
    pool.extend(
        state
            .doc
            .scene()
            .solids()
            .iter()
            .filter(|(_, n)| {
                zone.is_some_and(|z| n.bounds.intersects(&z)) || near_on_screen(view, &ray, &n.bounds, pos)
            })
            .map(|(id, _)| *id),
    );
    pool.sort_unstable();
    pool.dedup();
    let mut cands: Vec<(f64, f64, EdgeRef)> = Vec::new();
    for id in pool {
        let Some(n) = state.doc.scene().solid(id) else { continue };
        if !n.geom.valid || !state.doc.is_solid_visible(id) {
            continue;
        }
        for [a, b] in n.geom.edges() {
            let Some(px) = segment_px(view, a, b, pos) else { continue };
            if px > f64::from(EDGE_TOLERANCE) {
                continue;
            }
            let t = ray_segment_t(o, dir, a, b);
            if t <= 0.0 || t_hit.is_some_and(|th| t > th + 4.0 + th * 0.01) {
                continue;
            }
            cands.push((px, -t, EdgeRef::new(id, a, b)));
        }
    }
    let eye = view.cam3d.pos;
    resolve(state, cands, inverse, &|e| (eye - edge_mid(e)).normalize_or_zero())
}

fn near_on_screen(view: &ViewInfo, ray: &Ray, b: &Aabb, pos: Pos2) -> bool {
    let mut rect: Option<egui::Rect> = None;
    for c in b.corners() {
        let Some(s) = view.world_to_screen(c) else {
            return b.expand(NEAR_HIT).ray_intersect(ray).is_some();
        };
        rect = Some(rect.map_or(egui::Rect::from_two_pos(s, s), |r| r.union(egui::Rect::from_two_pos(s, s))));
    }
    rect.is_some_and(|r| r.expand(EDGE_TOLERANCE).contains(pos))
}

fn edge_at_2d(state: &AppState, view: &ViewInfo, pos: Pos2, inverse: bool) -> Option<EdgeRef> {
    let toward = view.kind.toward_viewer();
    let selected = state.doc.selected_solid_ids();
    let tol = f64::from(EDGE_TOLERANCE);
    let p2 = view.screen_to_2d(pos);
    let reach = tol * view.units_per_point();
    let pool: Vec<Id> = state
        .doc
        .scene()
        .solids()
        .iter()
        .filter(|(id, n)| {
            let lo = view.kind.to_2d(n.bounds.min);
            let hi = view.kind.to_2d(n.bounds.max);
            let (min, max) = (lo.min(hi), lo.max(hi));
            selected.contains(*id)
                || (p2.x >= min.x - reach && p2.x <= max.x + reach && p2.y >= min.y - reach && p2.y <= max.y + reach)
        })
        .map(|(id, _)| *id)
        .collect();
    let mut cands: Vec<(f64, f64, EdgeRef)> = Vec::new();
    for id in pool {
        let Some(n) = state.doc.scene().solid(id) else { continue };
        if !n.geom.valid || !state.doc.is_solid_visible(id) {
            continue;
        }
        for [a, b] in n.geom.edges() {
            let Some(d) = (b - a).try_normalize() else { continue };
            if d.dot(toward).abs() < 0.999 {
                continue;
            }
            let Some(s) = view.world_to_screen(a) else { continue };
            let px = f64::from(s.distance(pos));
            if px > tol {
                continue;
            }
            let bonus = if selected.contains(&id) { SELECTED_FIRST } else { 0.0 };
            cands.push((px, ((a + b) * 0.5).dot(toward) + bonus, EdgeRef::new(id, a, b)));
        }
    }
    let cursor = view.screen_to_2d(pos);
    let kind = view.kind;
    resolve(state, cands, inverse, &|e| {
        let d = cursor - kind.to_2d(e.a);
        if d.length() * view.cam2d.zoom < 0.5 {
            return DVec3::ZERO;
        }
        (kind.from_2d(d, 0.0) - kind.from_2d(DVec2::ZERO, 0.0)).normalize_or_zero()
    })
}

fn per_px(view: &ViewInfo, edges: &[EdgeRef]) -> f64 {
    let upp = view.units_per_point();
    if view.is_2d() {
        return upp;
    }
    let mid = edges.first().map_or(view.cam3d.pos, edge_mid);
    upp * (mid - view.cam3d.pos).length().max(1.0) / 512.0
}

impl BevelTool {
    pub fn new() -> Self {
        Self {
            options: BevelOptions::default(),
            picked: Vec::new(),
            picked_revision: 0,
            op: None,
            hover: None,
            wheel: 0.0,
            preview: RefCell::new(Preview::default()),
            run_hover: None,
        }
    }

    pub fn picked(&self) -> &[EdgeRef] {
        &self.picked
    }

    pub fn is_active(&self) -> bool {
        self.op.is_some()
    }

    pub fn active_edges(&self) -> &[EdgeRef] {
        self.op.as_ref().map_or(&[], |o| &o.edges)
    }

    pub fn limit(&self) -> Option<f64> {
        self.op.as_ref().map(|o| o.limit)
    }

    pub fn hovered_edge(&self) -> Option<EdgeRef> {
        self.hover.and_then(|h| h.edge)
    }

    pub fn params(&self, state: &AppState) -> BevelParams {
        let limit = self.op.as_ref().map_or(MAX_BEVEL_RADIUS, |o| o.limit);
        BevelParams {
            radius: self.options.radius.min(limit).max(MIN_BEVEL_RADIUS),
            segments: self.options.segments.clamp(1, MAX_BEVEL_SEGMENTS),
            inverse: self.options.inverse,
            material: self.options.current_material.then(|| active_material(state)),
        }
    }

    pub fn pick(&mut self, state: &AppState, edge: EdgeRef, add: bool) {
        self.picked_revision = state.doc.revision();
        if !add {
            self.picked = vec![edge];
            return;
        }
        match self.picked.iter().position(|e| e.same_edge(&edge)) {
            Some(i) => {
                self.picked.remove(i);
            }
            None => self.picked.push(edge),
        }
    }

    pub fn clear_picked(&mut self) -> bool {
        let had = !self.picked.is_empty();
        self.picked.clear();
        had
    }

    pub fn start(&mut self, state: &mut AppState, edges: Vec<EdgeRef>, at: Option<(&ViewInfo, Pos2)>) -> bool {
        if edges.is_empty() {
            state.notify(Severity::Info, t!("bevel.no_edge"));
            return false;
        }
        let limit = match state.doc.bevel_limit(&edges, self.options.inverse) {
            Ok(l) if l >= MIN_BEVEL_RADIUS => l,
            Ok(_) => {
                state.notify(Severity::Warning, tf!("bevel.failed", t!("bevel.err_room")));
                return false;
            }
            Err(e) => {
                state.notify(Severity::Warning, tf!("bevel.failed", edit_text(&e)));
                return false;
            }
        };
        self.options.radius = clean(self.options.radius.clamp(MIN_BEVEL_RADIUS, limit));
        let anchor = at.map(|(view, pos)| Anchor {
            view: view.index,
            pos,
            per_px: per_px(view, &edges),
            radius: self.options.radius,
        });
        self.op = Some(Op { edges, anchor, limit, revision: state.doc.revision() });
        self.wheel = 0.0;
        true
    }

    pub fn start_picked_or_hovered(&mut self, state: &mut AppState) -> bool {
        let at = self.hover.filter(|h| fresh(state, h.frame) || h.edge.is_some());
        let edges =
            if self.picked.is_empty() { at.and_then(|h| h.edge).into_iter().collect() } else { self.picked.clone() };
        let view = at.map(|h| h.view);
        self.start(state, edges, view.as_ref().zip(at.map(|h| h.pos)))
    }

    pub fn cancel_op(&mut self) -> bool {
        self.op.take().is_some()
    }

    pub fn toggle_inverse(&mut self, state: &mut AppState) {
        self.options.inverse = !self.options.inverse;
        let inverse = self.options.inverse;
        if let Some(op) = self.op.as_mut()
            && let Ok(l) = state.doc.bevel_limit(&op.edges, inverse)
        {
            op.limit = l.max(MIN_BEVEL_RADIUS);
        }
    }

    pub fn set_radius(&mut self, radius: f64) -> bool {
        if !radius.is_finite() || radius < MIN_BEVEL_RADIUS {
            return false;
        }
        let limit = self.op.as_ref().map_or(MAX_BEVEL_RADIUS, |o| o.limit);
        self.options.radius = clean(radius.min(limit).min(MAX_BEVEL_RADIUS));
        if let Some(op) = self.op.as_mut() {
            op.anchor = None;
        }
        true
    }

    pub fn step_segments(&mut self, delta: i32) {
        let n = i64::from(self.options.segments) + i64::from(delta);
        self.options.segments = n.clamp(1, i64::from(MAX_BEVEL_SEGMENTS)) as u32;
    }

    pub fn apply(&mut self, state: &mut AppState) -> Option<Vec<Id>> {
        let op = self.op.as_ref()?;
        let params = self.params(state);
        match state.doc.bevel_edges(&op.edges, &params) {
            Ok(out) => {
                state.log(Severity::Info, tf!("bevel.done", out.ids.len()));
                self.op = None;
                self.picked.clear();
                self.hover = None;
                Some(out.ids)
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("bevel.failed", edit_text(&e)));
                None
            }
        }
    }

    fn drop_stale(&mut self, state: &AppState) {
        let revision = state.doc.revision();
        if self.picked_revision != revision {
            self.picked.retain(|e| edge_exists(state, e));
            self.picked_revision = revision;
        }
        let stale = self.op.as_ref().is_some_and(|o| o.revision != revision);
        if stale {
            let ok = self.op.as_ref().is_some_and(|o| o.edges.iter().all(|e| edge_exists(state, e)));
            match self.op.as_mut() {
                Some(o) if ok => o.revision = revision,
                _ => self.op = None,
            }
        }
    }

    fn track(&mut self, state: &AppState, view: &ViewInfo, pos: Pos2, modifiers: egui::Modifiers) {
        let radius = self.options.radius;
        let Some(op) = self.op.as_mut() else { return };
        let anchor = *op.anchor.get_or_insert(Anchor {
            view: view.index,
            pos,
            per_px: per_px(view, &op.edges),
            radius: radius.min(op.limit),
        });
        if anchor.view != view.index || (anchor.pos.y - pos.y).abs() < MOVE_DEADZONE {
            return;
        }
        let raw = anchor.radius + f64::from(anchor.pos.y - pos.y) * anchor.per_px;
        let snap = Snap::new(state, view, modifiers);
        let r = if snap.is_on() { snap.tool_radius(raw) } else { (raw * 100.0).round() / 100.0 };
        let r = if r < MIN_BEVEL_RADIUS && snap.is_on() { snap.tool_step() } else { r };
        self.options.radius = clean(r.clamp(MIN_BEVEL_RADIUS, op.limit));
    }

    pub(crate) fn handle_mouse(
        &mut self,
        state: &mut AppState,
        view: &ViewInfo,
        m: &Mouse,
        secondary: bool,
    ) -> ToolFlow {
        self.drop_stale(state);
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        if self.op.is_some() {
            if secondary {
                self.op = None;
                return ToolFlow::Handled;
            }
            self.track(state, view, pos, m.modifiers);
            if m.clicked || m.drag_stopped {
                self.apply(state);
            }
            return ToolFlow::Handled;
        }
        let edge = edge_at(state, view, pos, self.options.inverse);
        self.hover = Some(Hover { view: *view, pos, edge, frame: state.frame.index });
        self.update_run_hover(state, view, edge, m.modifiers);
        if m.double_clicked
            && let Some(e) = edge
        {
            let edges = if self.picked.iter().any(|p| p.same_edge(&e)) { self.picked.clone() } else { vec![e] };
            self.start(state, edges, Some((view, pos)));
            return ToolFlow::Handled;
        }
        if m.clicked && m.modifiers.alt {
            self.pick_run(state, view, edge, m.modifiers);
            return ToolFlow::Handled;
        }
        if m.clicked {
            let add = m.modifiers.ctrl || m.modifiers.command;
            match edge {
                Some(e) => self.pick(state, e, add),
                None if !add => {
                    self.picked.clear();
                }
                None => {}
            }
            return ToolFlow::Handled;
        }
        ToolFlow::Ignored
    }

    fn run_for(&self, state: &AppState, view: &ViewInfo, seed: &EdgeRef, ring: bool) -> Vec<EdgeRef> {
        let kind = if ring { RunKind::Ring } else { RunKind::Loop };
        let toward = if view.is_2d() {
            view.kind.toward_viewer()
        } else {
            (view.cam3d.pos - edge_mid(seed)).normalize_or_zero()
        };
        let selected = state.doc.selected_solid_ids();
        state.doc.edge_run_refs(kind, seed, &selected, Some(toward)).unwrap_or_default()
    }

    fn update_run_hover(&mut self, state: &AppState, view: &ViewInfo, edge: Option<EdgeRef>, mods: egui::Modifiers) {
        let Some(seed) = edge.filter(|_| mods.alt) else {
            self.run_hover = None;
            return;
        };
        let ring = mods.ctrl || mods.command;
        let revision = state.doc.revision();
        if self.run_hover.as_ref().is_some_and(|h| h.ring == ring && h.revision == revision && h.edge.same_edge(&seed))
        {
            return;
        }
        let edges = self.run_for(state, view, &seed, ring);
        self.run_hover = Some(RunHover { edge: seed, ring, revision, edges });
    }

    pub fn pick_run(&mut self, state: &mut AppState, view: &ViewInfo, edge: Option<EdgeRef>, mods: egui::Modifiers) {
        let Some(seed) = edge else {
            state.log(Severity::Info, t!("bevel.loop_none").to_owned());
            return;
        };
        let ring = mods.ctrl || mods.command;
        let run = self.run_for(state, view, &seed, ring);
        self.run_hover = None;
        self.pick_edges(state, run, seed, mods.shift, ring);
    }

    pub fn pick_edges(&mut self, state: &mut AppState, run: Vec<EdgeRef>, seed: EdgeRef, add: bool, ring: bool) {
        let mut bevelable: Vec<EdgeRef> =
            run.iter().copied().filter(|e| state.doc.bevel_opening(e, self.options.inverse).is_some()).collect();
        if bevelable.is_empty() {
            bevelable.push(seed);
        }
        self.picked_revision = state.doc.revision();
        if !add {
            self.picked.clear();
        }
        for e in bevelable.iter() {
            if !self.picked.iter().any(|p| p.same_edge(e)) {
                self.picked.push(*e);
            }
        }
        let key = if ring { "bevel.ring_picked" } else { "bevel.loop_picked" };
        state.log(Severity::Info, tf!(key, bevelable.len()));
    }

    fn refresh_preview(&self, state: &AppState) {
        let Ok(mut guard) = self.preview.try_borrow_mut() else { return };
        let pv: &mut Preview = &mut guard;
        let Some(op) = self.op.as_ref() else {
            if pv.key.is_some() {
                *pv = Preview::default();
            }
            return;
        };
        let params = self.params(state);
        let key = hash_f64s(
            [
                state.doc.revision() as f64,
                params.radius,
                f64::from(params.segments),
                f64::from(u8::from(params.inverse)),
                params.material.as_deref().map_or(0.0, |m| m.len() as f64 + 1.0),
            ]
            .into_iter()
            .chain(op.edges.iter().flat_map(|e| [f64::from(e.solid), e.a.x, e.a.y, e.a.z, e.b.x, e.b.y, e.b.z])),
        );
        if pv.key == Some(key) {
            return;
        }
        *pv = Preview { key: Some(key), ..Preview::default() };
        let mut next = 1u32;
        let result = state.doc.bevel_preview(&op.edges, &params, &mut || {
            next = next.wrapping_add(1);
            next
        });
        match result {
            Ok(list) => {
                pv.radius = list.iter().map(|(_, r)| r.radius).fold(f64::INFINITY, f64::min);
                pv.segments = list.iter().map(|(_, r)| r.segments).min().unwrap_or(1);
                for (_, r) in &list {
                    pv.kinds.extend(r.kinds.iter().copied());
                    let changed = r.changes_brush();
                    let mut solids: Vec<PreviewSolid<'_>> = Vec::new();
                    if changed {
                        solids.extend(r.pieces.iter().map(|p| (p.sides.as_slice(), Some(p.origins.as_slice()))));
                    }
                    solids.extend(r.added.iter().map(|a| (a.as_slice(), None)));
                    pv.brushes += solids.len();
                    for (sides, origins) in solids {
                        let g = hp_geom::build_solid(sides);
                        pv.edges.extend(g.edges());
                        for f in &g.faces {
                            let new = origins.is_none_or(|o| o.get(f.side_index).is_some_and(Option::is_none));
                            if !new {
                                continue;
                            }
                            for [i, j, k] in f.triangles() {
                                if let (Some(&a), Some(&b), Some(&c)) =
                                    (f.vertices.get(i), f.vertices.get(j), f.vertices.get(k))
                                {
                                    pv.tris.push([a, b, c]);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => pv.error = Some(edit_text(&e)),
        }
    }

    pub fn preview_error(&self, state: &AppState) -> Option<String> {
        self.refresh_preview(state);
        self.preview.try_borrow().ok().and_then(|p| p.error.clone())
    }

    pub fn preview_brushes(&self, state: &AppState) -> usize {
        self.refresh_preview(state);
        self.preview.try_borrow().map(|p| p.brushes).unwrap_or(0)
    }

    pub fn preview_kinds(&self, state: &AppState) -> Vec<BevelKind> {
        self.refresh_preview(state);
        self.preview.try_borrow().map(|p| p.kinds.clone()).unwrap_or_default()
    }

    fn label(&self, state: &AppState) -> Option<String> {
        self.refresh_preview(state);
        let pv = self.preview.try_borrow().ok()?;
        if let Some(e) = &pv.error {
            return Some(e.clone());
        }
        let kind = pv.kinds.first().copied()?;
        let mixed = pv.kinds.iter().any(|k| *k != kind);
        let name = if mixed {
            format!("{} / {}", kind_text(BevelKind::Cove, pv.segments), kind_text(BevelKind::Fill, pv.segments))
        } else {
            kind_text(kind, pv.segments).to_owned()
        };
        Some(tf!("bevel.label", name, fmt_len(pv.radius), pv.segments))
    }

    fn start_keys(state: &AppState) -> String {
        state.keys.shortcut(Action::SnapToGrid).map(|sc| shortcut_label(&sc)).unwrap_or_else(|| key_label(Key::Enter))
    }
}

impl Tool for BevelTool {
    fn id(&self) -> ToolId {
        ToolId::Bevel
    }

    fn activate(&mut self, state: &mut AppState) {
        let seeded = std::mem::take(&mut state.runtime.brush.bevel_edges);
        let seeded: Vec<EdgeRef> = seeded.into_iter().filter(|e| edge_exists(state, e)).collect();
        if !seeded.is_empty() {
            self.picked = seeded.clone();
            self.picked_revision = state.doc.revision();
            self.start(state, seeded, None);
            return;
        }
        if let Some((view, pos)) = state.runtime.brush.bevel_start.take()
            && let Some(edge) = edge_at(state, &view, pos, self.options.inverse)
        {
            self.hover = Some(Hover { view, pos, edge: Some(edge), frame: state.frame.index });
            self.start(state, vec![edge], Some((&view, pos)));
        }
    }

    fn deactivate(&mut self, _state: &mut AppState) {
        self.op = None;
        self.hover = None;
        self.run_hover = None;
        self.picked.clear();
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        let r = input.response;
        let secondary = r.secondary_clicked()
            || (r.contains_pointer() && r.ctx.input(|i| i.pointer.button_pressed(PointerButton::Secondary)));
        self.handle_mouse(state, input.view, &m, secondary)
    }

    fn wheel(&mut self, _state: &mut AppState, _view: &ViewInfo, notches: f32, _mods: egui::Modifiers) -> ToolFlow {
        if self.op.is_none() || !notches.is_finite() {
            return ToolFlow::Ignored;
        }
        self.wheel += notches;
        while self.wheel >= 1.0 {
            self.wheel -= 1.0;
            self.step_segments(1);
        }
        while self.wheel <= -1.0 {
            self.wheel += 1.0;
            self.step_segments(-1);
        }
        ToolFlow::Handled
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        self.drop_stale(state);
        if ctx.memory(|m| m.focused()).is_some() {
            return false;
        }
        if self.op.is_some() {
            if ctx.input_mut(|i| state.keys.take_action(&mut i.events, Action::SnapToGrid))
                || consume_plain_key(ctx, Key::B)
            {
                self.toggle_inverse(state);
                return true;
            }
            if consume_plain_key(ctx, Key::Enter) {
                self.apply(state);
                return true;
            }
            if consume_plain_key(ctx, Key::Escape) {
                return self.cancel_op();
            }
            return false;
        }
        let hovered = self.hover.is_some_and(|h| h.edge.is_some() && fresh(state, h.frame));
        if (hovered || !self.picked.is_empty())
            && ctx.input_mut(|i| state.keys.take_action(&mut i.events, Action::SnapToGrid))
        {
            self.start_picked_or_hovered(state);
            return true;
        }
        if !self.picked.is_empty() {
            if consume_plain_key(ctx, Key::Enter) {
                self.start_picked_or_hovered(state);
                return true;
            }
            if consume_plain_key(ctx, Key::Escape) {
                return self.clear_picked();
            }
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        self.refresh_preview(state);
        let mut ov = Ov::new(out, view);
        let pending = rgba(COLOR_PENDING, 1.0);
        let picked = rgba(COLOR_GHOST, 1.0);
        let edges: &[EdgeRef] = self.op.as_ref().map_or(&self.picked, |o| &o.edges);
        for e in edges {
            ov.xray_line(e.a, e.b, picked);
            ov.point(e.a, picked, 6.0);
            ov.point(e.b, picked, 6.0);
        }
        if self.op.is_none() {
            if let Some(h) = self.run_hover.as_ref() {
                for e in &h.edges {
                    ov.xray_line(e.a, e.b, pending);
                }
            }
            if let Some(h) = self.hover.filter(|h| fresh(state, h.frame))
                && let Some(e) = h.edge
            {
                ov.xray_line(e.a, e.b, pending);
                ov.point(e.a, pending, 7.0);
                ov.point(e.b, pending, 7.0);
                if !view.is_2d()
                    && let Some(n) = state.doc.scene().solid(e.solid)
                {
                    for f in &n.geom.faces {
                        let on = |p: DVec3| f.vertices.iter().any(|v| v.distance_squared(p) < 1e-4);
                        if on(e.a) && on(e.b) {
                            ov.polygon_fill(&f.vertices, rgba(COLOR_PENDING, 0.12));
                        }
                    }
                }
            }
            return;
        }
        if let Ok(pv) = self.preview.try_borrow() {
            let line = rgba(COLOR_FRONT, 1.0);
            for e in &pv.edges {
                ov.xray_line(e[0], e[1], line);
            }
            if !view.is_2d() {
                let fill = rgba(COLOR_PENDING, 0.3);
                for t in &pv.tris {
                    ov.tri(t[0], t[1], t[2], fill);
                }
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let Some(op) = &self.op else { return };
        let Some(first) = op.edges.first() else { return };
        let Some(s) = view.world_to_screen(edge_mid(first)) else { return };
        if !view.rect.contains(s) {
            return;
        }
        let Some(text) = self.label(state) else { return };
        let color = if self.preview_error(state).is_some() { hp_app::theme::VIEW_ERROR } else { COLOR_PENDING };
        paint_label(&painter, s - Vec2::new(0.0, 10.0), Align2::CENTER_BOTTOM, &text, color);
        let here = match op.anchor {
            Some(a) => a.view == view.index,
            None => self.hover.is_some_and(|h| h.view.index == view.index) || !view.is_2d(),
        };
        if here {
            let keys = tf!("bevel.label_keys", key_label(Key::B), key_label(Key::Enter), key_label(Key::Escape));
            paint_label(&painter, s + Vec2::new(0.0, 10.0), Align2::CENTER_TOP, &keys, egui::Color32::from_gray(210));
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.drop_stale(state);
        ui.horizontal_wrapped(|ui| {
            let limit = self.op.as_ref().map_or(MAX_BEVEL_RADIUS, |o| o.limit);
            ui.label(t!("bevel.radius")).on_hover_text(t!("bevel.radius_tip"));
            let mut radius = self.options.radius;
            let drag = egui::DragValue::new(&mut radius).speed(1.0).range(MIN_BEVEL_RADIUS..=limit).max_decimals(3);
            if ui.add(drag).on_hover_text(t!("bevel.radius_tip")).changed() {
                self.set_radius(radius);
            }
            ui.label(t!("bevel.segments")).on_hover_text(t!("bevel.segments_tip"));
            let mut segments = self.options.segments;
            let drag = egui::DragValue::new(&mut segments).speed(0.1).range(1..=MAX_BEVEL_SEGMENTS);
            if ui.add(drag).on_hover_text(t!("bevel.segments_tip")).changed() {
                self.options.segments = segments.clamp(1, MAX_BEVEL_SEGMENTS);
            }
            let mut inverse = self.options.inverse;
            if ui
                .checkbox(&mut inverse, t!("bevel.inverse"))
                .on_hover_text(tf!("bevel.inverse_tip", key_label(Key::B)))
                .changed()
            {
                self.toggle_inverse(state);
            }
            ui.separator();
            ui.label(t!("bevel.new_faces")).on_hover_text(t!("bevel.new_faces_tip"));
            for (current, key) in [(false, "bevel.face_texture"), (true, "bevel.current_texture")] {
                if ui
                    .add(egui::Button::selectable(self.options.current_material == current, t!(key)))
                    .on_hover_text(t!("bevel.new_faces_tip"))
                    .clicked()
                {
                    self.options.current_material = current;
                }
            }
            if self.options.current_material {
                material_chip(ui, state);
            }
            ui.separator();
            if self.op.is_some() {
                let ok = self.preview_error(state).is_none();
                if ui
                    .add_enabled(ok, egui::Button::new(t!("bevel.apply")))
                    .on_hover_text(tf!("bevel.apply_tip", key_label(Key::Enter)))
                    .clicked()
                {
                    self.apply(state);
                }
                if ui
                    .button(t!("bevel.cancel"))
                    .on_hover_text(tf!("bevel.cancel_tip", key_label(Key::Escape)))
                    .clicked()
                {
                    self.cancel_op();
                }
            } else {
                if ui
                    .add_enabled(!self.picked.is_empty(), egui::Button::new(t!("bevel.start")))
                    .on_hover_text(tf!("bevel.start_tip", Self::start_keys(state)))
                    .clicked()
                {
                    let edges = self.picked.clone();
                    self.start(state, edges, None);
                }
                if ui.add_enabled(!self.picked.is_empty(), egui::Button::new(t!("bevel.clear_edges"))).clicked() {
                    self.clear_picked();
                }
            }
            let count = self.op.as_ref().map_or(self.picked.len(), |o| o.edges.len());
            if count > 0 {
                ui.separator();
                ui.label(tf!("bevel.picked", count));
            }
            if let Some(op) = &self.op {
                ui.label(tf!("bevel.limit", fmt_len(op.limit)));
                match self.preview_error(state) {
                    Some(e) => {
                        ui.colored_label(ui.visuals().warn_fg_color, e);
                    }
                    None => {
                        ui.label(tf!("bevel.brushes", self.preview_brushes(state)));
                    }
                }
            }
        });
    }

    fn takes_right_drag(&self, _kind: hp_app::ViewKind, _modifiers: egui::Modifiers) -> bool {
        self.op.is_some()
    }

    fn cursor(&self, state: &AppState, _view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        if self.op.is_some() {
            return Some(CursorIcon::ResizeVertical);
        }
        if self.hover.is_some_and(|h| h.edge.is_some() && fresh(state, h.frame)) {
            return Some(CursorIcon::PointingHand);
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        let enter = key_label(Key::Enter);
        let escape = key_label(Key::Escape);
        Some(match &self.op {
            Some(_) => tf!("bevel.hint_active", key_label(Key::B), enter, escape),
            None if !self.picked.is_empty() => {
                tf!("bevel.hint_picked", self.picked.len(), Self::start_keys(state), enter, escape)
            }
            None => tf!("bevel.hint_idle", Self::start_keys(state)),
        })
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        self.cancel_op() || self.clear_picked()
    }

    fn set_option(&mut self, _state: &mut AppState, key: &str, value: &str) -> bool {
        let flag = match value.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        match key {
            "radius" => value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && (MIN_BEVEL_RADIUS..=MAX_BEVEL_RADIUS).contains(v))
                .is_some_and(|v| self.set_radius(v)),
            "segments" => value
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|v| (1..=MAX_BEVEL_SEGMENTS).contains(v))
                .map(|v| self.options.segments = v)
                .is_some(),
            "inverse" => flag.map(|v| self.options.inverse = v).is_some(),
            "new_faces" => match value.trim().to_ascii_lowercase().as_str() {
                "face" => {
                    self.options.current_material = false;
                    true
                }
                "current" => {
                    self.options.current_material = true;
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<ToolOption> {
        let o = &self.options;
        let radius = format!("{:.4}", o.radius);
        let radius = radius.trim_end_matches('0').trim_end_matches('.').to_owned();
        vec![
            ToolOption::new("radius", radius, "radius of the rounding in units (0.25 to 16384, limited by the faces)"),
            ToolOption::new("segments", o.segments.to_string(), "faces of the rounding (1: flat chamfer, up to 32)"),
            ToolOption::new(
                "inverse",
                if o.inverse { "on" } else { "off" },
                "on: concave bevel (a cove carved in the edge, or a round filling an inner corner)",
            ),
            ToolOption::new(
                "new_faces",
                if o.current_material { "current" } else { "face" },
                "texture of the rounded faces: face (the faces around the edge) or current (the current material)",
            ),
        ]
    }
}
