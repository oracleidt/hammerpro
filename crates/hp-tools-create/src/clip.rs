use std::cell::RefCell;

use egui::{Align2, CursorIcon, Key, Modifiers, Pos2, Stroke, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::{Action, key_label, shortcut_label};
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::{ClipKeep, Id};
use hp_geom::{Aabb, Plane};
use hp_render::Overlay;

use crate::clip3d::{CutAxis, Pick3d, SnapKind, axis_plane, face_cut_plane, facing, keep_side, pick_3d};
use crate::common::{
    COLOR_BACK, COLOR_FRONT, COLOR_GHOST, COLOR_PENDING, COLOR_REMOVED, Mouse, Ov, Snap, active_material,
    consume_plain_key, consume_tabs, fmt_len, fmt_num, fresh, paint_handle, paint_label, plane_box_polygon, rgba,
    round_to,
};

const HANDLE_PICK: f32 = 8.0;

pub fn clip_plane_2d(kind: ViewKind, a: DVec2, b: DVec2) -> Option<Plane> {
    let (r, u, _) = kind.axes_2d()?;
    let d = b - a;
    if !d.is_finite() || d.length_squared() < 1e-12 {
        return None;
    }
    let mut normal = DVec3::ZERO;
    normal[r] = d.y;
    normal[u] = -d.x;
    Plane::from_normal_point(normal, kind.from_2d(a, 0.0))
}

pub fn clip_plane_3_points(kind: ViewKind, a: DVec2, b: DVec2, depth: f64, c: DVec3) -> Option<Plane> {
    let base = clip_plane_2d(kind, a, b)?;
    let (pa, pb) = (kind.from_2d(a, depth), kind.from_2d(b, depth));
    match Plane::from_points(pa, pb, c) {
        Some(p) if p.normal.dot(base.normal) < 0.0 => Some(p.flip()),
        Some(p) => Some(p),
        None => Some(base),
    }
}

pub fn next_keep(keep: ClipKeep, backwards: bool) -> ClipKeep {
    match (keep, backwards) {
        (ClipKeep::Both, false) | (ClipKeep::Back, true) => ClipKeep::Front,
        (ClipKeep::Front, false) | (ClipKeep::Both, true) => ClipKeep::Back,
        (ClipKeep::Back, false) | (ClipKeep::Front, true) => ClipKeep::Both,
    }
}

fn keep_key(keep: ClipKeep) -> &'static str {
    match keep {
        ClipKeep::Front => "create.clip.keep_front",
        ClipKeep::Back => "create.clip.keep_back",
        ClipKeep::Both => "create.clip.keep_both",
    }
}

fn keep_id(keep: ClipKeep) -> &'static str {
    match keep {
        ClipKeep::Front => "front",
        ClipKeep::Back => "back",
        ClipKeep::Both => "both",
    }
}

fn axis_key(axis: CutAxis) -> &'static str {
    match axis {
        CutAxis::Auto => "create.clip.axis_auto",
        CutAxis::X => "create.clip.axis_x",
        CutAxis::Y => "create.clip.axis_y",
        CutAxis::Z => "create.clip.axis_z",
    }
}

fn fmt_angle(deg: f64) -> String {
    fmt_num(round_to(deg, 0.1))
}

fn points_plane(points: &[DVec3], face: Option<DVec3>) -> Option<Option<Plane>> {
    match points {
        [a, b] => Some(face.and_then(|n| face_cut_plane(*a, *b, n))),
        [a, b, c] => Plane::from_points(*a, *b, *c).filter(|p| !along_face(p, face)).map(Some),
        _ => Some(None),
    }
}

fn along_face(plane: &Plane, face: Option<DVec3>) -> bool {
    face.is_some_and(|n| plane.normal.dot(n).abs() > 1.0 - 1e-9)
}

fn toward(normal: DVec3, eye: DVec3, at: DVec3) -> DVec3 {
    if normal.dot(eye - at) < 0.0 { -normal } else { normal }
}

fn consume_letter(ctx: &egui::Context, key: Key) -> bool {
    ctx.input_mut(|i| {
        if i.modifiers.any() {
            return false;
        }
        let found = i.events.iter().position(
            |e| matches!(e, egui::Event::Key { key: k, pressed: true, modifiers: m, .. } if *k == key && !m.any()),
        );
        found.map(|at| i.events.remove(at)).is_some()
    })
}

fn ctrl_held(m: Modifiers) -> bool {
    m.ctrl || m.command
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AxisSource {
    face_normal: DVec3,
    eye: DVec3,
    forward: DVec3,
}

#[derive(Clone, Debug, PartialEq)]
enum ClipDef {
    None,
    Line { kind: ViewKind, a: DVec2, b: DVec2, depth: f64, c: Option<DVec3> },
    Points { points: Vec<DVec3>, face: Option<DVec3>, plane: Option<Plane> },
    Fixed { anchor: DVec3, plane: Plane, axis: Option<AxisSource> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    A,
    B,
    C,
    Both,
    Point(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    view: usize,
    target: Target,
    from: DVec2,
    start: (DVec2, DVec2),
}

#[derive(Debug, Default)]
struct Preview {
    key: Option<(u64, [u64; 4])>,
    front_edges: Vec<[DVec3; 2]>,
    back_edges: Vec<[DVec3; 2]>,
    front_tris: Vec<[DVec3; 3]>,
    back_tris: Vec<[DVec3; 3]>,
    crossing: usize,
    bounds: Aabb,
}

pub struct ClipTool {
    pub keep: ClipKeep,
    pub three_point: bool,
    pub axis: CutAxis,
    def: ClipDef,
    drag: Option<Drag>,
    hover_handle: Option<(usize, Target, u64)>,
    hover_3d: Option<(Pick3d, Modifiers, u64)>,
    last_focus: Option<egui::Id>,
    preview: RefCell<Preview>,
}

impl Default for ClipTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipTool {
    pub fn new() -> Self {
        Self {
            keep: ClipKeep::Both,
            three_point: false,
            axis: CutAxis::Auto,
            def: ClipDef::None,
            drag: None,
            hover_handle: None,
            hover_3d: None,
            last_focus: None,
            preview: RefCell::new(Preview::default()),
        }
    }

    pub fn set_line(&mut self, kind: ViewKind, a: DVec2, b: DVec2) {
        self.def = if kind.is_2d() { ClipDef::Line { kind, a, b, depth: 0.0, c: None } } else { ClipDef::None };
    }

    pub fn set_third_point(&mut self, p: DVec3) {
        if let ClipDef::Line { c, .. } = &mut self.def
            && p.is_finite()
        {
            *c = Some(p);
        }
    }

    pub fn set_plane(&mut self, anchor: DVec3, plane: Plane) -> bool {
        let Some(plane) = Plane::new(plane.normal, plane.dist).filter(|_| anchor.is_finite()) else { return false };
        let anchor = plane.project(anchor);
        self.def = ClipDef::Fixed { anchor, plane, axis: None };
        true
    }

    pub fn plane(&self) -> Option<Plane> {
        match &self.def {
            ClipDef::None => None,
            ClipDef::Line { kind, a, b, depth, c } => match c {
                Some(c) if self.three_point => clip_plane_3_points(*kind, *a, *b, *depth, *c),
                _ => clip_plane_2d(*kind, *a, *b),
            },
            ClipDef::Points { plane, .. } => *plane,
            ClipDef::Fixed { plane, .. } => Some(*plane),
        }
    }

    pub fn cycle_mode(&mut self, backwards: bool) {
        self.keep = next_keep(self.keep, backwards);
    }

    pub fn set_axis(&mut self, axis: CutAxis) {
        self.axis = axis;
        if let ClipDef::Fixed { anchor, plane, axis: Some(src) } = &mut self.def
            && let Some(next) = axis_plane(axis.resolve(src.face_normal, src.forward), *anchor)
        {
            *plane = facing(next, src.eye);
        }
    }

    fn tab_input(&mut self, ctx: &egui::Context) -> bool {
        let focused = ctx.memory(|m| m.focused());
        let had_focus = std::mem::replace(&mut self.last_focus, focused);
        if had_focus.is_some() {
            return false;
        }
        let presses = consume_tabs(ctx);
        if presses.is_empty() {
            return false;
        }
        if let Some(id) = focused {
            ctx.memory_mut(|m| m.surrender_focus(id));
            self.last_focus = None;
            ctx.request_repaint();
        }
        for back in presses {
            self.cycle_mode(back);
        }
        true
    }

    fn mode_keys(state: &AppState) -> String {
        let tab = key_label(Key::Tab);
        match state.keys.shortcut(Action::ToolClip) {
            Some(sc) => format!("{} / {tab}", shortcut_label(&sc)),
            None => tab,
        }
    }

    pub fn clear(&mut self) -> bool {
        let had = self.def != ClipDef::None;
        self.def = ClipDef::None;
        self.drag = None;
        self.hover_handle = None;
        had
    }

    pub fn apply(&mut self, state: &mut AppState) -> bool {
        let Some(plane) = self.plane() else { return false };
        let ids: Vec<Id> = state.doc.selected_solid_ids();
        if ids.is_empty() {
            state.notify(Severity::Warning, t!("create.clip.no_selection"));
            return false;
        }
        let material = state.settings.brush.clip_current_material.then(|| active_material(state));
        match state.doc.clip_with(&ids, &plane, self.keep, material.as_deref()) {
            Ok(pieces) if pieces.is_empty() => {
                state.notify(Severity::Info, t!("create.clip.nothing"));
                false
            }
            Ok(pieces) => {
                let cut = if self.keep == ClipKeep::Both { pieces.len() / 2 } else { pieces.len() };
                state.log(Severity::Info, tf!("create.clip.done", cut));
                if matches!(self.def, ClipDef::Points { .. } | ClipDef::Fixed { .. }) {
                    self.clear();
                }
                true
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("create.clip.failed", e));
                false
            }
        }
    }

    fn line_points(&self) -> Option<(DVec3, DVec3)> {
        match &self.def {
            ClipDef::Line { kind, a, b, depth, .. } => Some((kind.from_2d(*a, *depth), kind.from_2d(*b, *depth))),
            _ => None,
        }
    }

    fn world_points(&self) -> Vec<DVec3> {
        match &self.def {
            ClipDef::Points { points, .. } => points.clone(),
            ClipDef::Fixed { anchor, .. } => vec![*anchor],
            _ => Vec::new(),
        }
    }

    fn handle_world(&self, view: &ViewInfo, target: Target) -> Option<DVec3> {
        match (&self.def, target) {
            (ClipDef::Line { kind, a, depth, .. }, Target::A) if *kind == view.kind => Some(kind.from_2d(*a, *depth)),
            (ClipDef::Line { kind, b, depth, .. }, Target::B) if *kind == view.kind => Some(kind.from_2d(*b, *depth)),
            (ClipDef::Line { kind, c: Some(c), .. }, Target::C)
                if self.three_point && view.is_2d() && *kind != view.kind =>
            {
                Some(*c)
            }
            (ClipDef::Points { .. } | ClipDef::Fixed { .. }, Target::Point(i)) => self.world_points().get(i).copied(),
            _ => None,
        }
    }

    fn handle_pos(&self, view: &ViewInfo, target: Target) -> Option<Pos2> {
        let p = view.world_to_screen(self.handle_world(view, target)?)?;
        view.rect.expand(HANDLE_PICK).contains(p).then_some(p)
    }

    fn targets(&self) -> Vec<Target> {
        match &self.def {
            ClipDef::None => Vec::new(),
            ClipDef::Line { .. } => vec![Target::C, Target::B, Target::A],
            ClipDef::Points { points, .. } => (0..points.len()).rev().map(Target::Point).collect(),
            ClipDef::Fixed { .. } => vec![Target::Point(0)],
        }
    }

    fn handle_at(&self, view: &ViewInfo, pos: Pos2) -> Option<Target> {
        self.targets().into_iter().find(|t| self.handle_pos(view, *t).is_some_and(|h| h.distance(pos) <= HANDLE_PICK))
    }

    fn on_line(&self, view: &ViewInfo, pos: Pos2) -> bool {
        let (Some(a), Some(b)) = (self.handle_pos(view, Target::A), self.handle_pos(view, Target::B)) else {
            return false;
        };
        let d = |p: Pos2| DVec2::new(f64::from(p.x), f64::from(p.y));
        hp_geom::point_segment_distance_2d(d(pos), d(a), d(b)) <= f64::from(HANDLE_PICK)
    }

    fn set_c_2d(&mut self, view: &ViewInfo, p: DVec2) {
        let Some((pa, pb)) = self.line_points() else { return };
        let Some((_, _, d)) = view.kind.axes_2d() else { return };
        if let ClipDef::Line { c, .. } = &mut self.def {
            let depth = c.map_or((pa[d] + pb[d]) * 0.5, |c| c[d]);
            *c = Some(view.kind.from_2d(p, depth));
        }
    }

    fn move_point(&mut self, index: usize, p: DVec3, face_normal: Option<DVec3>) {
        if !p.is_finite() {
            return;
        }
        match &mut self.def {
            ClipDef::Points { points, face, plane } => {
                let mut moved = points.clone();
                let Some(slot) = moved.get_mut(index) else { return };
                *slot = p;
                let next_face = if index == 0 { face_normal.or(*face) } else { *face };
                let Some(next) = points_plane(&moved, next_face) else { return };
                if plane.is_some() && next.is_none() {
                    return;
                }
                *plane = next.map(|n| keep_side(n, *plane));
                *points = moved;
                *face = next_face;
            }
            ClipDef::Fixed { anchor, plane, .. } if index == 0 => {
                if let Some(next) = Plane::from_normal_point(plane.normal, p) {
                    *plane = next;
                    *anchor = p;
                }
            }
            _ => {}
        }
    }

    fn new_line_depth(state: &AppState, kind: ViewKind) -> f64 {
        let Some((_, _, d)) = kind.axes_2d() else { return 0.0 };
        state
            .doc
            .selection_bounds(&|e| state.entity_bounds(e))
            .map(|b| b.center()[d])
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        if view.is_2d() {
            let snap = Snap::for_selection(state, view, m.modifiers);
            self.mouse_2d(state, view, m, pos, &snap)
        } else {
            self.mouse_3d(state, view, m, pos)
        }
    }

    fn mouse_2d(&mut self, state: &AppState, view: &ViewInfo, m: &Mouse, pos: Pos2, snap: &Snap) -> ToolFlow {
        if m.hovering() {
            self.hover_handle = self.handle_at(view, pos).map(|t| (view.index, t, state.frame.index));
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        let at_origin = snap.v2(view.screen_to_2d(origin));
        let ctrl = ctrl_held(m.modifiers);
        let shift = m.modifiers.shift;
        let line_elsewhere = matches!(&self.def, ClipDef::Line { kind, .. } if *kind != view.kind);
        let third = self.three_point && line_elsewhere && !shift;
        if m.drag_started {
            let hit = if shift { None } else { self.handle_at(view, origin) };
            let start = match &self.def {
                ClipDef::Line { a, b, .. } => (*a, *b),
                _ => (DVec2::ZERO, DVec2::ZERO),
            };
            let drag = |target| Drag { view: view.index, target, from: view.screen_to_2d(origin), start };
            self.drag = match hit {
                Some(Target::A | Target::B) if ctrl => Some(drag(Target::Both)),
                Some(t) => Some(drag(t)),
                None if ctrl && self.on_line(view, origin) => Some(drag(Target::Both)),
                None if third => {
                    self.set_c_2d(view, at_origin);
                    Some(drag(Target::C))
                }
                None => {
                    let depth = Self::new_line_depth(state, view.kind);
                    self.def = ClipDef::Line { kind: view.kind, a: at_origin, b: at_origin, depth, c: None };
                    Some(drag(Target::B))
                }
            };
        } else if m.clicked && third {
            self.set_c_2d(view, at_origin);
        }
        if (m.dragging || m.drag_stopped)
            && let Some(d) = self.drag
            && d.view == view.index
        {
            let p = snap.v2(view.screen_to_2d(pos));
            match d.target {
                Target::A | Target::B | Target::Both => {
                    if let ClipDef::Line { a, b, .. } = &mut self.def {
                        match d.target {
                            Target::A => *a = p,
                            Target::B => *b = p,
                            _ => {
                                let moved = snap.v2(d.start.0 + (view.screen_to_2d(pos) - d.from));
                                let delta = moved - d.start.0;
                                if delta.is_finite() {
                                    *a = moved;
                                    *b = d.start.1 + delta;
                                }
                            }
                        }
                    }
                }
                Target::C => self.set_c_2d(view, p),
                Target::Point(i) => {
                    if let Some((r, u, _)) = view.kind.axes_2d()
                        && let Some(mut q) = self.world_points().get(i).copied()
                    {
                        q[r] = p.x;
                        q[u] = p.y;
                        self.move_point(i, q, None);
                    }
                }
            }
            self.hover_handle = Some((view.index, d.target, state.frame.index));
        }
        if m.drag_stopped {
            self.drag = None;
            if let ClipDef::Line { a, b, .. } = &self.def
                && a.distance(*b) < 1e-6
            {
                self.def = ClipDef::None;
            }
        }
        ToolFlow::Handled
    }

    fn drag_fallback(view: &ViewInfo, pos: Pos2, old: DVec3) -> Option<DVec3> {
        let (o, dir) = view.ray(pos);
        let n = view.cam3d.forward();
        let denom = dir.dot(n);
        let t = (old - o).dot(n) / denom;
        (denom.abs() > 1e-9 && t > 0.0 && t.is_finite()).then(|| o + dir * t)
    }

    fn mouse_3d(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse, pos: Pos2) -> ToolFlow {
        let pick = pick_3d(state, view, pos, m.modifiers);
        if m.hovering() {
            self.hover_3d = pick.map(|p| (p, m.modifiers, state.frame.index));
            self.hover_handle = self.handle_at(view, pos).map(|t| (view.index, t, state.frame.index));
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        let plain = !m.modifiers.shift && !ctrl_held(m.modifiers);
        if m.drag_started
            && plain
            && let Some(Target::Point(i)) = self.handle_at(view, origin)
        {
            self.drag = Some(Self::point_drag(view, i));
            return ToolFlow::Handled;
        }
        if let Some(d) = self.drag
            && d.view == view.index
            && let Target::Point(i) = d.target
        {
            if m.dragging || m.drag_stopped {
                let eye = view.cam3d.pos;
                let target = pick.map(|h| h.point).or_else(|| {
                    let old = self.world_points().get(i).copied()?;
                    Self::drag_fallback(view, pos, old)
                });
                if let Some(p) = target {
                    let face = pick.map(|h| toward(h.face_plane.normal, eye, h.point));
                    self.move_point(i, p, face);
                }
                self.hover_handle = Some((view.index, d.target, state.frame.index));
            }
            if m.drag_stopped {
                self.drag = None;
            }
            return ToolFlow::Handled;
        }
        if m.activated() {
            let at = if m.drag_started { pick_3d(state, view, origin, m.modifiers) } else { pick };
            if let Some(hit) = at
                && let Some(index) = self.place(state, view, &hit, m.modifiers)
                && m.drag_started
            {
                self.drag = Some(Self::point_drag(view, index));
            }
        }
        if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored }
    }

    fn point_drag(view: &ViewInfo, i: usize) -> Drag {
        Drag { view: view.index, target: Target::Point(i), from: DVec2::ZERO, start: Default::default() }
    }

    fn axis_source(view: &ViewInfo, hit: &Pick3d) -> AxisSource {
        AxisSource { face_normal: hit.face_plane.normal, eye: view.cam3d.pos, forward: view.cam3d.forward() }
    }

    fn axis_candidate(&self, src: &AxisSource, at: DVec3) -> Option<Plane> {
        axis_plane(self.axis.resolve(src.face_normal, src.forward), at).map(|p| facing(p, src.eye))
    }

    fn place(&mut self, state: &mut AppState, view: &ViewInfo, hit: &Pick3d, mods: Modifiers) -> Option<usize> {
        let eye = view.cam3d.pos;
        let p = hit.point;
        if ctrl_held(mods) {
            self.def = ClipDef::Fixed { anchor: p, plane: hit.face_plane, axis: None };
            return Some(0);
        }
        if mods.shift {
            let src = Self::axis_source(view, hit);
            let plane = self.axis_candidate(&src, p)?;
            self.def = ClipDef::Fixed { anchor: p, plane, axis: Some(src) };
            return Some(0);
        }
        let (mut points, face, old) = match std::mem::replace(&mut self.def, ClipDef::None) {
            ClipDef::Points { points, face, plane } if points.len() < 3 => (points, face, plane),
            _ => (Vec::new(), None, None),
        };
        if points.iter().any(|q| q.distance(p) < 1e-6) {
            self.def = ClipDef::Points { points, face, plane: old };
            return None;
        }
        let face = if points.is_empty() { Some(toward(hit.face_plane.normal, eye, p)) } else { face };
        points.push(p);
        let (plane, index) = match points_plane(&points, face) {
            Some(next) => {
                let oriented =
                    |n: Plane| if old.is_some() || points.len() < 3 { keep_side(n, old) } else { facing(n, eye) };
                (next.map(oriented), Some(points.len() - 1))
            }
            None => {
                let same = match points[..] {
                    [a, b, c] => Plane::from_points(a, b, c).is_some_and(|p| along_face(&p, face)),
                    _ => false,
                };
                let key = if same { "create.clip.same_face" } else { "create.clip.collinear" };
                state.notify(Severity::Warning, t!(key));
                points.pop();
                (old, None)
            }
        };
        self.def = if points.is_empty() { ClipDef::None } else { ClipDef::Points { points, face, plane } };
        index
    }

    fn hover_candidate(&self, view: &ViewInfo, hit: &Pick3d, mods: Modifiers) -> Option<Plane> {
        if ctrl_held(mods) {
            return Some(hit.face_plane);
        }
        if mods.shift {
            return self.axis_candidate(&Self::axis_source(view, hit), hit.point);
        }
        match &self.def {
            ClipDef::Points { points, face, .. } => match points[..] {
                [a] => face.and_then(|n| face_cut_plane(a, hit.point, n)),
                [a, b] => Plane::from_points(a, b, hit.point),
                _ => None,
            },
            _ => None,
        }
    }

    fn refresh_preview(&self, state: &AppState) {
        let Ok(mut guard) = self.preview.try_borrow_mut() else { return };
        let pv: &mut Preview = &mut guard;
        let Some(plane) = self.plane() else {
            if pv.key.is_some() {
                *pv = Preview::default();
            }
            return;
        };
        let key = (
            state.doc.revision(),
            [plane.normal.x.to_bits(), plane.normal.y.to_bits(), plane.normal.z.to_bits(), plane.dist.to_bits()],
        );
        if pv.key == Some(key) {
            return;
        }
        *pv = Preview { key: Some(key), bounds: Aabb::EMPTY, ..Preview::default() };
        let mut next = 1u32;
        let mut gen_id = || {
            next = next.wrapping_add(1);
            next
        };
        let mut selection_bounds = Aabb::EMPTY;
        for id in state.doc.selected_solid_ids() {
            let Some(node) = state.doc.scene().solid(id) else { continue };
            selection_bounds = selection_bounds.union(&node.bounds);
            if !hp_geom::plane_splits(&node.geom, &plane) {
                continue;
            }
            let Ok((Some(front), Some(back))) = hp_geom::clip_solid(&node.solid.sides, &plane, &mut gen_id) else {
                continue;
            };
            pv.crossing += 1;
            for (sides, is_front) in [(front, true), (back, false)] {
                let geom = hp_geom::build_solid(&sides);
                pv.bounds = pv.bounds.union(&geom.aabb);
                let (edges, tris) = if is_front {
                    (&mut pv.front_edges, &mut pv.front_tris)
                } else {
                    (&mut pv.back_edges, &mut pv.back_tris)
                };
                edges.extend(geom.edges());
                for f in &geom.faces {
                    for [i, j, k] in f.triangles() {
                        if let (Some(&a), Some(&b), Some(&c)) =
                            (f.vertices.get(i), f.vertices.get(j), f.vertices.get(k))
                        {
                            tris.push([a, b, c]);
                        }
                    }
                }
            }
        }
        if pv.bounds.is_empty() {
            pv.bounds = selection_bounds;
        }
    }

    pub fn crossing_count(&self, state: &AppState) -> usize {
        self.refresh_preview(state);
        self.preview.try_borrow().map(|p| p.crossing).unwrap_or(0)
    }

    fn plane_region(&self, pv: &Preview) -> Aabb {
        if !pv.bounds.is_empty() {
            return pv.bounds.expand(16.0);
        }
        let mut b = Aabb::EMPTY;
        match &self.def {
            ClipDef::Line { kind, a, b: pb, c, .. } => {
                b.extend(kind.from_2d(*a, -256.0));
                b.extend(kind.from_2d(*pb, 256.0));
                if let Some(c) = c {
                    b.extend(*c);
                }
            }
            ClipDef::Points { .. } | ClipDef::Fixed { .. } => {
                for p in self.world_points() {
                    b.extend(p);
                }
            }
            ClipDef::None => {}
        }
        if b.is_empty() { b } else { b.expand(64.0) }
    }

    fn hover_region(state: &AppState, around: &[DVec3]) -> Aabb {
        let mut b = Aabb::EMPTY;
        for id in state.doc.selected_solid_ids() {
            if let Some(n) = state.doc.scene().solid(id) {
                b = b.union(&n.bounds);
            }
        }
        for p in around {
            b.extend(*p);
        }
        b.expand(16.0)
    }

    fn colors(&self) -> ([f32; 4], [f32; 4]) {
        match self.keep {
            ClipKeep::Both => (rgba(COLOR_FRONT, 1.0), rgba(COLOR_BACK, 1.0)),
            ClipKeep::Front => (rgba(COLOR_FRONT, 1.0), rgba(COLOR_REMOVED, 0.55)),
            ClipKeep::Back => (rgba(COLOR_REMOVED, 0.55), rgba(COLOR_BACK, 1.0)),
        }
    }

    fn mode_ui(&mut self, ui: &mut Ui, state: &AppState) {
        let tip = tf!("create.clip.mode_tip", Self::mode_keys(state));
        for keep in [ClipKeep::Front, ClipKeep::Back, ClipKeep::Both] {
            let b = egui::Button::selectable(self.keep == keep, t!(keep_key(keep)));
            if ui.add(b).on_hover_text(&tip).clicked() {
                self.keep = keep;
            }
        }
    }

    fn axis_ui(&mut self, ui: &mut Ui) {
        let tip = tf!("create.clip.axis_tip", key_label(Key::X));
        ui.label(t!("create.clip.axis")).on_hover_text(&tip);
        for axis in CutAxis::ALL {
            let b = egui::Button::selectable(self.axis == axis, t!(axis_key(axis)));
            if ui.add(b).on_hover_text(&tip).clicked() {
                self.set_axis(axis);
            }
        }
    }

    fn toggle_measurements(state: &mut AppState) {
        state.settings.brush.clip_measurements = !state.settings.brush.clip_measurements;
        state.settings_dirty = true;
    }

    fn active_handle(&self, state: &AppState, view: &ViewInfo) -> Option<Target> {
        let dragged = self.drag.filter(|d| d.view == view.index).map(|d| d.target);
        dragged.or_else(|| {
            self.hover_handle.filter(|(vi, _, frame)| *vi == view.index && fresh(state, *frame)).map(|(_, t, _)| t)
        })
    }

    fn paint_line(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let ClipDef::Line { kind, a, b, .. } = &self.def else { return };
        let (Some(pa), Some(pb)) =
            (view.world_to_screen(kind.from_2d(*a, 0.0)), view.world_to_screen(kind.from_2d(*b, 0.0)))
        else {
            return;
        };
        let d = pb - pa;
        if d.length() > 0.5 {
            let dir = d.normalized();
            let reach = view.rect.size().length() * 2.0;
            painter.line_segment(
                [pa - dir * reach, pb + dir * reach],
                Stroke::new(1.0, COLOR_PENDING.gamma_multiply(0.35)),
            );
            let mid = pa + d * 0.5;
            let n = egui::vec2(-dir.y, dir.x);
            painter.arrow(mid, n * 14.0, Stroke::new(1.5, self.front_color32()));
            if state.settings.brush.clip_measurements {
                let delta = *b - *a;
                let angle = delta.y.atan2(delta.x).to_degrees();
                let text = tf!("create.clip.measure", fmt_len(delta.length()), fmt_angle(angle));
                paint_label(painter, mid - n * 10.0, Align2::CENTER_CENTER, &text, COLOR_PENDING);
            }
        }
        painter.line_segment([pa, pb], Stroke::new(2.0, COLOR_PENDING));
        let active = self.active_handle(state, view);
        let is = |t: Target| active == Some(t) || active == Some(Target::Both) && matches!(t, Target::A | Target::B);
        paint_handle(painter, pa, is(Target::A));
        paint_handle(painter, pb, is(Target::B));
    }

    fn front_color32(&self) -> egui::Color32 {
        let (front, _) = self.colors();
        egui::Color32::from_rgb((front[0] * 255.0) as u8, (front[1] * 255.0) as u8, (front[2] * 255.0) as u8)
    }

    fn paint_third(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(c) = self.handle_pos(view, Target::C) else { return };
        paint_handle(painter, c, self.active_handle(state, view) == Some(Target::C));
        if state.settings.brush.clip_measurements
            && let ClipDef::Line { kind, a, b, .. } = &self.def
            && let (Some(base), Some(plane)) = (clip_plane_2d(*kind, *a, *b), self.plane())
        {
            let tilt = base.normal.dot(plane.normal).clamp(-1.0, 1.0).acos().to_degrees();
            let text = tf!("create.clip.tilt", fmt_angle(tilt));
            paint_label(painter, c + Vec2::new(10.0, -10.0), Align2::LEFT_BOTTOM, &text, COLOR_PENDING);
        }
    }

    fn paint_trace(&self, view: &ViewInfo, painter: &egui::Painter) {
        let (Some(plane), Some((r, u, d))) = (self.plane(), view.kind.axes_2d()) else { return };
        if plane.normal[d].abs() > 1e-6 {
            return;
        }
        let Some(&anchor) = self.world_points().first() else { return };
        let on = plane.project(anchor);
        let base = DVec2::new(on[r], on[u]);
        let dir = DVec2::new(-plane.normal[u], plane.normal[r]);
        let Some(dir) = dir.try_normalize() else { return };
        let reach = 2.0 * f64::from(view.rect.size().length()) / view.cam2d.zoom.max(1e-6);
        let (Some(pa), Some(pb), Some(pc)) = (
            view.world_to_screen(view.kind.from_2d(base - dir * reach, 0.0)),
            view.world_to_screen(view.kind.from_2d(base + dir * reach, 0.0)),
            view.world_to_screen(on),
        ) else {
            return;
        };
        let stroke = Stroke::new(1.0, COLOR_PENDING.gamma_multiply(0.6));
        painter.extend(egui::Shape::dashed_line(&[pa, pb], stroke, 8.0, 5.0));
        if view.rect.contains(pc)
            && let Some(tip) = view.world_to_screen(on + plane.normal)
            && (tip - pc).length() > 1e-3
        {
            painter.arrow(pc, (tip - pc).normalized() * 14.0, Stroke::new(1.5, self.front_color32()));
        }
    }

    fn paint_points(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let points = self.world_points();
        if view.is_2d() {
            self.paint_trace(view, painter);
        }
        let active = self.active_handle(state, view);
        for i in 0..points.len() {
            if let Some(p) = self.handle_pos(view, Target::Point(i)) {
                paint_handle(painter, p, active == Some(Target::Point(i)));
            }
        }
        if view.is_2d() || !state.settings.brush.clip_measurements || !matches!(self.def, ClipDef::Points { .. }) {
            return;
        }
        for (i, p) in points.iter().enumerate() {
            let Some(q) = points.get((i + 1) % points.len()).filter(|_| points.len() > 1) else { continue };
            if points.len() == 2 && i == 1 {
                break;
            }
            if let Some(s) = view.world_to_screen((*p + *q) * 0.5)
                && view.rect.contains(s)
            {
                paint_label(painter, s, Align2::CENTER_CENTER, &fmt_len(p.distance(*q)), COLOR_PENDING);
            }
        }
    }

    fn overlay_hover(&self, state: &AppState, view: &ViewInfo, ov: &mut Ov<'_>) {
        if view.is_2d() || self.drag.is_some() {
            return;
        }
        let Some((h, mods, _)) = self.hover_3d.filter(|(_, _, f)| fresh(state, *f)) else { return };
        let (color, size) = match h.kind {
            SnapKind::Vertex => (rgba(egui::Color32::WHITE, 1.0), 10.0),
            SnapKind::Midpoint => (rgba(COLOR_GHOST, 1.0), 9.0),
            SnapKind::Grid => (rgba(COLOR_PENDING, 0.9), 7.0),
            SnapKind::Free => (rgba(COLOR_PENDING, 0.6), 6.0),
        };
        let modal = ctrl_held(mods) || mods.shift;
        let adding = matches!(&self.def, ClipDef::Points { points, .. } if points.len() < 3);
        if modal || adding || matches!(self.def, ClipDef::None | ClipDef::Line { .. } | ClipDef::Fixed { .. }) {
            ov.point(h.point, color, size);
        }
        if let ClipDef::Points { points, .. } = &self.def
            && !modal
            && points.len() < 3
            && let Some(last) = points.last()
        {
            ov.line(*last, h.point, rgba(COLOR_PENDING, 0.6));
        }
        let Some(candidate) = self.hover_candidate(view, &h, mods) else { return };
        let mut around = self.world_points();
        if modal {
            around.clear();
        }
        around.push(h.point);
        let poly = plane_box_polygon(&candidate, &Self::hover_region(state, &around));
        if modal {
            ov.polygon_fill(&poly, rgba(COLOR_PENDING, 0.08));
        }
        ov.polygon_outline(&poly, rgba(COLOR_PENDING, 0.5), true);
    }

    fn overlay_normal(&self, ov: &mut Ov<'_>, plane: &Plane) {
        let points = self.world_points();
        if points.is_empty() {
            return;
        }
        let center = points.iter().copied().sum::<DVec3>() / points.len() as f64;
        let base = plane.project(center);
        let (front, _) = self.colors();
        ov.xray_line(base, base + plane.normal * 24.0, front);
        ov.point(base + plane.normal * 24.0, front, 6.0);
    }
}

impl Tool for ClipTool {
    fn id(&self) -> ToolId {
        ToolId::Clip
    }

    fn activate(&mut self, _state: &mut AppState) {
        self.last_focus = None;
    }

    fn reactivate(&mut self, _state: &mut AppState) {
        self.cycle_mode(false);
    }

    fn deactivate(&mut self, _state: &mut AppState) {
        self.clear();
        self.hover_3d = None;
        self.last_focus = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.tab_input(ctx) {
            return true;
        }
        if self.plane().is_some() && consume_plain_key(ctx, Key::Enter) {
            self.apply(state);
            return true;
        }
        if self.def != ClipDef::None && consume_plain_key(ctx, Key::Escape) {
            return self.clear();
        }
        if consume_letter(ctx, Key::C) {
            self.three_point = !self.three_point;
            return true;
        }
        if consume_letter(ctx, Key::X) {
            self.set_axis(self.axis.next());
            return true;
        }
        if consume_letter(ctx, Key::O) {
            Self::toggle_measurements(state);
            return true;
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        self.refresh_preview(state);
        let mut ov = Ov::new(out, view);
        let Ok(pv) = self.preview.try_borrow() else { return };
        let (front, back) = self.colors();
        for (edges, color) in [(&pv.front_edges, front), (&pv.back_edges, back)] {
            for e in edges {
                ov.xray_line(e[0], e[1], color);
            }
        }
        if !view.is_2d() {
            for (tris, color) in [(&pv.front_tris, front), (&pv.back_tris, back)] {
                let fill = [color[0], color[1], color[2], if color[3] < 1.0 { 0.4 } else { 0.22 }];
                for t in tris {
                    ov.tri(t[0], t[1], t[2], fill);
                }
            }
            if let Some(plane) = self.plane() {
                let poly = plane_box_polygon(&plane, &self.plane_region(&pv));
                ov.polygon_fill(&poly, rgba(COLOR_PENDING, 0.16));
                ov.polygon_outline(&poly, rgba(COLOR_PENDING, 0.9), true);
                self.overlay_normal(&mut ov, &plane);
            }
        }
        if let ClipDef::Line { kind, c, .. } = &self.def
            && *kind != view.kind
            && (self.three_point || !view.is_2d())
            && let Some((pa, pb)) = self.line_points()
        {
            let color = rgba(COLOR_PENDING, 1.0);
            ov.xray_line(pa, pb, color);
            ov.point(pa, color, 7.0);
            ov.point(pb, color, 7.0);
            if self.three_point
                && let Some(c) = c
            {
                let mid = (pa + pb) * 0.5;
                ov.xray_line(mid, *c, rgba(COLOR_PENDING, 0.6));
                ov.point(*c, color, 8.0);
            }
        }
        if matches!(self.def, ClipDef::Points { .. } | ClipDef::Fixed { .. }) {
            let points = self.world_points();
            for (i, p) in points.iter().enumerate() {
                ov.point(*p, rgba(COLOR_PENDING, 1.0), 8.0);
                if let Some(q) = points.get(i + 1) {
                    ov.xray_line(*p, *q, rgba(COLOR_PENDING, 1.0));
                }
            }
        }
        self.overlay_hover(state, view, &mut ov);
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        match &self.def {
            ClipDef::Line { kind, .. } if *kind == view.kind => self.paint_line(state, view, &painter),
            ClipDef::Line { .. } => self.paint_third(state, view, &painter),
            ClipDef::Points { .. } | ClipDef::Fixed { .. } => self.paint_points(state, view, &painter),
            ClipDef::None => {}
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            self.mode_ui(ui, state);
            ui.separator();
            let ready = self.plane().is_some();
            let cut = if ready { self.crossing_count(state) } else { 0 };
            if ui
                .add_enabled(ready && cut > 0, egui::Button::new(t!("create.clip.apply")))
                .on_hover_text(tf!("create.clip.apply_tip", key_label(Key::Enter)))
                .clicked()
            {
                self.apply(state);
            }
            if ui.add_enabled(self.def != ClipDef::None, egui::Button::new(t!("common.clear"))).clicked() {
                self.clear();
            }
            ui.separator();
            if ui
                .add(egui::Button::selectable(self.three_point, t!("create.clip.three_point")))
                .on_hover_text(tf!("create.clip.three_point_tip", key_label(Key::C)))
                .clicked()
            {
                self.three_point = !self.three_point;
            }
            ui.separator();
            self.axis_ui(ui);
            ui.separator();
            let mut measure = state.settings.brush.clip_measurements;
            if ui
                .checkbox(&mut measure, t!("create.clip.measurements"))
                .on_hover_text(tf!("create.clip.measurements_tip", key_label(Key::O)))
                .changed()
            {
                Self::toggle_measurements(state);
            }
            let mut current = state.settings.brush.clip_current_material;
            if ui
                .checkbox(&mut current, t!("create.clip.cap_material"))
                .on_hover_text(t!("create.clip.cap_material_tip"))
                .changed()
            {
                state.settings.brush.clip_current_material = current;
                state.settings_dirty = true;
            }
            if ready {
                ui.separator();
                ui.label(tf!("create.clip.crossing", cut));
            }
        });
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if self.drag.is_some_and(|d| d.view == view.index) {
            return Some(CursorIcon::Grabbing);
        }
        if self.handle_at(view, pos).is_some() {
            return Some(CursorIcon::Grab);
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        let escape = key_label(Key::Escape);
        let c = key_label(Key::C);
        let ready = || (t!(keep_key(self.keep)), key_label(Key::Enter), Self::mode_keys(state), escape.clone());
        Some(match &self.def {
            ClipDef::Points { points, plane: None, .. } if points.len() == 1 => tf!("create.clip.hint_second", escape),
            ClipDef::Points { points, plane: None, .. } => {
                tf!("create.clip.hint_points", 3usize.saturating_sub(points.len()), escape)
            }
            ClipDef::Points { points, .. } if points.len() == 2 => {
                let (k, e, m, x) = ready();
                tf!("create.clip.hint_ready_3d", k, e, m, x)
            }
            ClipDef::Points { .. } | ClipDef::Fixed { .. } => {
                let (k, e, m, x) = ready();
                tf!("create.clip.hint_fixed", k, e, m, x)
            }
            ClipDef::Line { c: None, .. } if self.three_point => tf!("create.clip.hint_third", c, escape),
            _ if self.plane().is_some() => {
                let (k, e, m, x) = ready();
                tf!("create.clip.hint_ready", k, e, m, x)
            }
            _ => tf!("create.clip.hint_idle", Self::mode_keys(state), c),
        })
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        self.clear()
    }

    fn set_option(&mut self, _state: &mut AppState, key: &str, value: &str) -> bool {
        let flag = match value.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        match key {
            "keep" => {
                let keep = [ClipKeep::Front, ClipKeep::Back, ClipKeep::Both]
                    .into_iter()
                    .find(|k| keep_id(*k).eq_ignore_ascii_case(value.trim()));
                keep.map(|k| self.keep = k).is_some()
            }
            "three_point" => flag.map(|v| self.three_point = v).is_some(),
            "axis" => CutAxis::from_id(value).map(|a| self.set_axis(a)).is_some(),
            _ => false,
        }
    }

    fn option_values(&self) -> Vec<ToolOption> {
        vec![
            ToolOption::new("keep", keep_id(self.keep), "pieces kept: front, back or both (Shift+X or Tab cycles)"),
            ToolOption::new(
                "three_point",
                if self.three_point { "on" } else { "off" },
                "on: after the 2D line, a click in another 2D view tilts the plane (C)",
            ),
            ToolOption::new(
                "axis",
                self.axis.id(),
                "world axis of the Shift+click cut in the 3D view: auto (the axis most along the view, not the \
                 clicked face's), x, y or z (X cycles)",
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{info_2d, info_3d, settle};
    use hp_editor::ObjRef;

    #[test]
    fn plane_from_two_points_in_each_view() {
        let p = clip_plane_2d(ViewKind::Top, DVec2::new(0.0, 16.0), DVec2::new(64.0, 16.0)).expect("plane");
        assert!((p.normal - DVec3::NEG_Y).length() < 1e-12);
        assert!((p.dist + 16.0).abs() < 1e-12);
        let p = clip_plane_2d(ViewKind::Front, DVec2::new(32.0, 0.0), DVec2::new(32.0, 64.0)).expect("plane");
        assert!((p.normal - DVec3::Y).length() < 1e-12);
        assert!((p.dist - 32.0).abs() < 1e-12);
        let a = DVec2::new(0.0, 0.0);
        let b = DVec2::new(64.0, 64.0);
        let p = clip_plane_2d(ViewKind::Side, a, b).expect("plane");
        assert!(p.normal.y.abs() < 1e-12);
        for q in [ViewKind::Side.from_2d(a, 0.0), ViewKind::Side.from_2d(b, 123.0)] {
            assert!(p.distance(q).abs() < 1e-9);
        }
        assert!(clip_plane_2d(ViewKind::Top, a, a).is_none());
        assert!(clip_plane_2d(ViewKind::Perspective, a, b).is_none());
    }

    #[test]
    fn plane_from_three_points() {
        let a = DVec2::new(0.0, 64.0);
        let b = DVec2::new(0.0, -64.0);
        let base = clip_plane_2d(ViewKind::Top, a, b).expect("base");
        let p = clip_plane_3_points(ViewKind::Top, a, b, 0.0, DVec3::new(64.0, 0.0, 64.0)).expect("plane");
        assert!(p.normal.dot(base.normal) > 0.0, "same front side");
        for q in [DVec3::new(0.0, 64.0, 0.0), DVec3::new(0.0, -64.0, 0.0), DVec3::new(64.0, 0.0, 64.0)] {
            assert!(p.distance(q).abs() < 1e-9);
        }
        assert!((p.normal.dot(base.normal) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
        let flat = clip_plane_3_points(ViewKind::Top, a, b, 0.0, DVec3::new(0.0, 0.0, 0.0)).expect("plane");
        assert!(flat.approx_eq(&base, 1e-9, 1e-9));
        assert!(clip_plane_3_points(ViewKind::Top, a, a, 0.0, DVec3::X).is_none());
    }

    #[test]
    fn mode_cycling() {
        assert_eq!(next_keep(ClipKeep::Both, false), ClipKeep::Front);
        assert_eq!(next_keep(ClipKeep::Front, false), ClipKeep::Back);
        assert_eq!(next_keep(ClipKeep::Back, false), ClipKeep::Both);
        for k in [ClipKeep::Both, ClipKeep::Front, ClipKeep::Back] {
            assert_eq!(next_keep(next_keep(k, false), true), k);
        }
    }

    fn clip_floor(keep: ClipKeep) -> (AppState, Vec<f64>) {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        tool.keep = keep;
        let view = info_2d(ViewKind::Top, 0.5);
        let a = view.world_to_screen(DVec3::new(3.0, 300.0, 0.0)).expect("a");
        let b = view.world_to_screen(DVec3::new(-2.0, -300.0, 0.0)).expect("b");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(a, b));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(a, b));
        let plane = tool.plane().expect("plane");
        assert!((plane.normal - DVec3::NEG_X).length() < 1e-12, "{plane:?}");
        assert_eq!(tool.crossing_count(&state), 1);
        let (undo, _) = state.doc.history_len();
        assert!(tool.apply(&mut state));
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
        let mut volumes: Vec<f64> = state
            .doc
            .selected_solid_ids()
            .iter()
            .filter_map(|id| state.doc.scene().solid(*id))
            .map(|n| n.geom.volume())
            .collect();
        volumes.sort_by(f64::total_cmp);
        (state, volumes)
    }

    #[test]
    fn three_modes_on_the_test_map() {
        let half = 256.0 * 512.0 * 16.0;
        let (mut state, v) = clip_floor(ClipKeep::Both);
        assert_eq!(v.len(), 2);
        assert!((v[0] - half).abs() < 1e-6 && (v[1] - half).abs() < 1e-6);
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().solid(2).map(|n| n.geom.volume()), Some(2.0 * half));

        let (state, v) = clip_floor(ClipKeep::Front);
        assert_eq!(v.len(), 1);
        let b = state.doc.scene().solid(2).expect("floor").bounds;
        assert_eq!((b.min.x, b.max.x), (-256.0, 0.0), "front (-X) kept");
        assert!((v[0] - half).abs() < 1e-6);

        let (state, _) = clip_floor(ClipKeep::Back);
        let b = state.doc.scene().solid(2).expect("floor").bounds;
        assert_eq!((b.min.x, b.max.x), (0.0, 256.0), "back (+X) kept");
    }

    #[test]
    fn nothing_to_clip_and_no_selection() {
        let mut state = settle();
        state.doc.clear_selection();
        let mut tool = ClipTool::new();
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0));
        assert!(!tool.apply(&mut state));
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 1000.0), DVec2::new(10.0, 1000.0));
        assert_eq!(tool.crossing_count(&state), 0);
        assert!(!tool.apply(&mut state));
        assert!(state.console.len() >= 2);
    }

    #[test]
    fn dragging_a_clip_point() {
        let mut state = settle();
        let mut tool = ClipTool::new();
        tool.set_line(ViewKind::Front, DVec2::new(0.0, 0.0), DVec2::new(0.0, 64.0));
        let view = info_2d(ViewKind::Front, 1.0);
        let b = view.world_to_screen(DVec3::new(0.0, 0.0, 64.0)).expect("b");
        let to = view.world_to_screen(DVec3::new(0.0, 64.0, 64.0)).expect("to");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(b));
        assert_eq!(tool.cursor(&state, &view, b), Some(CursorIcon::Grab));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(b, b));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(b, to));
        assert_eq!(
            tool.def,
            ClipDef::Line { kind: ViewKind::Front, a: DVec2::ZERO, b: DVec2::new(64.0, 64.0), depth: 0.0, c: None }
        );
        assert!(tool.cancel(&mut state));
        assert!(tool.plane().is_none());
    }

    #[test]
    fn ctrl_drag_moves_both_points_and_shift_starts_over() {
        let mut state = settle();
        let mut tool = ClipTool::new();
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(0.0, 64.0));
        let view = info_2d(ViewKind::Top, 1.0);
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let from = view.world_to_screen(DVec3::new(0.0, 32.0, 0.0)).expect("from");
        let to = view.world_to_screen(DVec3::new(37.0, 40.0, 0.0)).expect("to");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(from, from).with_modifiers(ctrl));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(from, to).with_modifiers(ctrl));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(from, to).with_modifiers(ctrl));
        let ClipDef::Line { a, b, .. } = tool.def.clone() else { panic!("line") };
        assert_eq!((a, b), (DVec2::new(32.0, 16.0), DVec2::new(32.0, 80.0)), "moved by a snapped offset");
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        let h = view.world_to_screen(DVec3::new(32.0, 16.0, 0.0)).expect("h");
        let end = view.world_to_screen(DVec3::new(96.0, 16.0, 0.0)).expect("end");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(h, h).with_modifiers(shift));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(h, end).with_modifiers(shift));
        let ClipDef::Line { a, b, .. } = tool.def.clone() else { panic!("line") };
        assert_eq!((a, b), (DVec2::new(32.0, 16.0), DVec2::new(96.0, 16.0)));
    }

    #[test]
    fn three_point_mode_tilts_the_plane() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let mut tool = ClipTool::new();
        tool.three_point = true;
        let top = info_2d(ViewKind::Top, 0.5);
        let a = top.world_to_screen(DVec3::new(0.0, 200.0, 0.0)).expect("a");
        let b = top.world_to_screen(DVec3::new(0.0, -200.0, 0.0)).expect("b");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(a, b));
        let base = tool.plane().expect("plane");
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        let ClipDef::Line { depth, .. } = tool.def.clone() else { panic!("line") };
        let side = info_2d(ViewKind::Side, 1.0);
        let c = side.world_to_screen(DVec3::new(64.0, 0.0, depth + 64.0)).expect("c");
        tool.handle_mouse(&mut state, &side, &Mouse::click(c));
        let tilted = tool.plane().expect("plane");
        assert!((tilted.normal.dot(base.normal) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
        assert!(tilted.normal.dot(base.normal) > 0.0);
        let c2 = side.world_to_screen(DVec3::new(128.0, 0.0, depth + 64.0)).expect("c2");
        tool.handle_mouse(&mut state, &side, &Mouse::drag_start(c, c));
        tool.handle_mouse(&mut state, &side, &Mouse::drag_end(c, c2));
        let more = tool.plane().expect("plane");
        assert!(more.normal.dot(base.normal) < tilted.normal.dot(base.normal));
        assert!(more.distance(DVec3::new(128.0, 0.0, depth + 64.0)).abs() < 1e-6);
        tool.three_point = false;
        assert!(tool.plane().expect("plane").approx_eq(&base, 1e-9, 1e-9));
    }

    #[test]
    fn three_points_in_3d() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let mut tool = ClipTool::new();
        let view = info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0);
        let at = |p: DVec3| view.world_to_screen(p).expect("screen");
        tool.handle_mouse(&mut state, &view, &Mouse::click(at(DVec3::new(-64.0, 256.0, 32.0))));
        tool.handle_mouse(&mut state, &view, &Mouse::click(at(DVec3::new(64.0, 256.0, 96.0))));
        let square = tool.plane().expect("two points on a face give a plane");
        assert!(square.normal.y.abs() < 1e-12, "square to the wall: {square:?}");
        assert!(tool.status_hint(&state).is_some());
        tool.handle_mouse(&mut state, &view, &Mouse::click(at(DVec3::new(0.0, 128.0, 0.0))));
        let plane = tool.plane().expect("plane");
        assert!(plane.normal.dot(square.normal) > 0.0, "the third point keeps the front side");
        assert_eq!(tool.crossing_count(&state), 1);
        let from = at(DVec3::new(0.0, 128.0, 0.0));
        let to = at(DVec3::new(0.0, 160.0, 0.0));
        tool.handle_mouse(&mut state, &view, &Mouse::hover(from));
        assert_eq!(tool.cursor(&state, &view, from), Some(CursorIcon::Grab));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(from, from));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(from, to));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(from, to));
        let moved = tool.plane().expect("plane");
        assert!(!moved.approx_eq(&plane, 1e-6, 1e-3), "the plane moved");
        assert!(moved.normal.dot(plane.normal) > 0.0);
        assert!(moved.distance(DVec3::new(0.0, 160.0, 0.0)).abs() < 1e-6);
        let top = info_2d(ViewKind::Top, 1.0);
        let p = top.world_to_screen(DVec3::new(0.0, 160.0, 0.0)).expect("p");
        let q = top.world_to_screen(DVec3::new(0.0, 192.0, 0.0)).expect("q");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(p, p));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(p, q));
        assert!(tool.plane().expect("plane").distance(DVec3::new(0.0, 192.0, 0.0)).abs() < 1e-6);
    }

    #[test]
    fn current_material_on_the_cut_faces() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        state.current_material = "TOOLS/TOOLSNODRAW".to_owned();
        state.settings.brush.clip_current_material = true;
        let mut tool = ClipTool::new();
        tool.keep = ClipKeep::Back;
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 300.0), DVec2::new(0.0, -300.0));
        assert!(tool.apply(&mut state));
        let sides = &state.doc.scene().solid(2).expect("floor").solid.sides;
        assert_eq!(sides.iter().filter(|s| s.material == "TOOLS/TOOLSNODRAW").count(), 1, "only the cut face");
    }

    #[test]
    fn keys_toggle_three_points_and_measurements() {
        let mut state = settle();
        let mut tool = ClipTool::new();
        assert!(state.settings.brush.clip_measurements);
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let input = egui::RawInput { events: vec![press(Key::C), press(Key::O)], ..Default::default() };
        let ctx = egui::Context::default();
        let mut handled = (false, false);
        let mut out = ctx.run_ui(input, |ui| {
            handled.0 = tool.key_input(&mut state, ui.ctx());
            handled.1 = tool.key_input(&mut state, ui.ctx());
        });
        out.textures_delta.clear();
        assert_eq!(handled, (true, true));
        assert!(tool.three_point);
        assert!(!state.settings.brush.clip_measurements);
        assert!(state.settings_dirty);
        tool.set_line(ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(64.0, 64.0));
        tool.set_third_point(DVec3::new(0.0, 64.0, 64.0));
        state.settings.brush.clip_measurements = true;
        hp_app::testing::run_ctx(&mut state, |ctx, state| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
                tool.paint(state, &info_2d(kind, 1.0), &painter);
            }
        });
        assert_eq!(fmt_angle(45.04), "45");
        assert_eq!(fmt_angle(-26.56), "-26.6");
    }

    fn floor_view() -> ViewInfo {
        info_3d(DVec3::new(0.0, -400.0, 300.0), 90.0, -40.0)
    }

    fn click_3d(tool: &mut ClipTool, state: &mut AppState, view: &ViewInfo, p: DVec3, mods: Modifiers) {
        let s = view.world_to_screen(p).expect("screen");
        tool.handle_mouse(state, view, &Mouse::hover(s).with_modifiers(mods));
        tool.handle_mouse(state, view, &Mouse::click(s).with_modifiers(mods));
    }

    fn volumes(state: &AppState, ids: &[Id]) -> Vec<f64> {
        let mut v: Vec<f64> =
            ids.iter().filter_map(|id| state.doc.scene().solid(*id)).map(|n| n.geom.volume()).collect();
        v.sort_by(f64::total_cmp);
        v
    }

    fn all_valid(state: &AppState) -> bool {
        state.doc.scene().solids().values().all(|n| n.geom.valid && n.geom.faces.len() >= 4)
    }

    #[test]
    fn two_points_on_a_face_cut_square_to_it() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        let view = floor_view();
        click_3d(&mut tool, &mut state, &view, DVec3::new(30.0, -100.0, 0.0), Modifiers::NONE);
        assert!(tool.plane().is_none());
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        click_3d(&mut tool, &mut state, &view, DVec3::new(34.0, 100.0, 0.0), Modifiers::NONE);
        let plane = tool.plane().expect("plane");
        assert!((plane.normal.abs() - DVec3::X).length() < 1e-12, "grid snapped, square to the floor: {plane:?}");
        assert!((plane.normal.x * 32.0 - plane.dist).abs() < 1e-9);
        assert!(plane.normal.x > 0.0, "front on the right of the line seen from the camera");
        assert_eq!(tool.crossing_count(&state), 1);
        let (undo, _) = state.doc.history_len();
        let before: Vec<String> =
            state.doc.scene().solid(2).expect("floor").solid.sides.iter().map(|s| s.material.clone()).collect();
        assert!(tool.apply(&mut state));
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
        assert_eq!(tool.def, ClipDef::None, "the 3D points are cleared after the cut");
        let ids = state.doc.selected_solid_ids();
        assert_eq!(ids.len(), 2);
        let v = volumes(&state, &ids);
        assert!((v[0] - 224.0 * 512.0 * 16.0).abs() < 1e-6 && (v[1] - 288.0 * 512.0 * 16.0).abs() < 1e-6, "{v:?}");
        for id in &ids {
            let n = state.doc.scene().solid(*id).expect("piece");
            assert!(n.solid.sides.iter().all(|s| before.contains(&s.material)), "textures kept");
        }
        assert!(all_valid(&state));
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().solid(2).map(|n| n.geom.volume()), Some(512.0 * 512.0 * 16.0));
    }

    #[test]
    fn keep_modes_in_3d() {
        for (keep, xs) in [(ClipKeep::Front, (32.0, 256.0)), (ClipKeep::Back, (-256.0, 32.0))] {
            let mut state = settle();
            state.doc.set_selection(&[ObjRef::Solid(2)]);
            let mut tool = ClipTool::new();
            tool.keep = keep;
            let view = floor_view();
            click_3d(&mut tool, &mut state, &view, DVec3::new(32.0, -100.0, 0.0), Modifiers::NONE);
            click_3d(&mut tool, &mut state, &view, DVec3::new(32.0, 100.0, 0.0), Modifiers::NONE);
            assert!(tool.apply(&mut state));
            assert_eq!(state.doc.selected_solid_ids(), vec![2]);
            let b = state.doc.scene().solid(2).expect("floor").bounds;
            assert_eq!((b.min.x, b.max.x), xs, "{keep:?}");
        }
    }

    #[test]
    fn a_third_point_on_the_same_face_is_refused() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        let view = floor_view();
        click_3d(&mut tool, &mut state, &view, DVec3::new(32.0, -100.0, 0.0), Modifiers::NONE);
        click_3d(&mut tool, &mut state, &view, DVec3::new(32.0, 100.0, 0.0), Modifiers::NONE);
        let square = tool.plane().expect("plane");
        click_3d(&mut tool, &mut state, &view, DVec3::new(-96.0, 32.0, 0.0), Modifiers::NONE);
        assert_eq!(tool.plane(), Some(square), "the floor plane itself would cut nothing");
        assert_eq!(tool.world_points().len(), 2);
        assert!(state.console.lines().any(|l| l.text.contains("same face")));
        assert_eq!(tool.crossing_count(&state), 1);
    }

    #[test]
    fn a_third_point_tilts_and_aligned_points_are_refused() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let mut tool = ClipTool::new();
        let view = info_3d(DVec3::new(-100.0, 0.0, 64.0), 80.0, 0.0);
        click_3d(&mut tool, &mut state, &view, DVec3::new(-64.0, 256.0, 32.0), Modifiers::NONE);
        click_3d(&mut tool, &mut state, &view, DVec3::new(-64.0, 256.0, 96.0), Modifiers::NONE);
        let square = tool.plane().expect("plane");
        click_3d(&mut tool, &mut state, &view, DVec3::new(-64.0, 256.0, 64.0), Modifiers::NONE);
        assert_eq!(tool.plane(), Some(square), "an aligned third point is refused");
        assert!(state.console.lines().any(|l| l.text.contains("aligned")));
        assert_eq!(tool.world_points().len(), 2);
        let view_side = info_3d(DVec3::new(-500.0, 200.0, 64.0), 10.0, 0.0);
        click_3d(&mut tool, &mut state, &view_side, DVec3::new(-256.0, 264.0, 64.0), Modifiers::NONE);
        let tilted = tool.plane().expect("plane");
        assert_eq!(tool.world_points().len(), 3);
        for p in tool.world_points() {
            assert!(tilted.distance(p).abs() < 1e-9);
        }
        assert!(tilted.normal.dot(square.normal) > 0.0, "same front side");
        assert!(!tilted.approx_eq(&square, 1e-6, 1e-3));
        assert!(tool.apply(&mut state));
        assert!(all_valid(&state));
        click_3d(&mut tool, &mut state, &view, DVec3::new(32.0, 256.0, 32.0), Modifiers::NONE);
        assert_eq!(tool.world_points().len(), 1, "a click after the cut starts a new set");
    }

    #[test]
    fn shift_click_cuts_along_an_axis_and_x_changes_it() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let mut tool = ClipTool::new();
        let view = info_3d(DVec3::new(10.0, 0.0, 70.0), 90.0, -2.0);
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        click_3d(&mut tool, &mut state, &view, DVec3::new(-30.0, 256.0, 62.0), shift);
        let plane = tool.plane().expect("plane");
        assert!(
            (plane.normal.abs() - DVec3::Z).length() < 1e-12,
            "auto: a horizontal cut on a wall seen from the front"
        );
        assert!(plane.distance(DVec3::new(0.0, 0.0, 64.0)).abs() < 1e-9, "through the snapped point");
        assert!(plane.distance(view.cam3d.pos) > 0.0, "front faces the camera");
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        tool.set_axis(CutAxis::X);
        let px = tool.plane().expect("plane");
        assert!((px.normal.abs() - DVec3::X).length() < 1e-12);
        assert!(px.distance(DVec3::new(-32.0, 0.0, 0.0)).abs() < 1e-9);
        tool.set_axis(CutAxis::Z);
        assert_eq!(tool.crossing_count(&state), 1);
        assert!(tool.apply(&mut state));
        let v = volumes(&state, &state.doc.selected_solid_ids());
        assert_eq!(v.len(), 2);
        assert!((v[0] - v[1]).abs() < 1e-6, "cut in the middle of the wall");
        assert!(all_valid(&state));
    }

    #[test]
    fn ctrl_click_uses_a_face_plane_on_several_brushes_and_keeps_owners() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3), ObjRef::Solid(7), ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        let view = info_3d(DVec3::new(96.0, -300.0, 200.0), 90.0, -30.0);
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        click_3d(&mut tool, &mut state, &view, DVec3::new(96.0, 32.0, 64.0), ctrl);
        let plane = tool.plane().expect("plane");
        assert!((plane.normal - DVec3::Z).length() < 1e-12 && (plane.dist - 64.0).abs() < 1e-12, "{plane:?}");
        assert_eq!(tool.crossing_count(&state), 1, "only the wall crosses the trigger top");
        assert!(tool.apply(&mut state));
        let wall: Vec<Id> = state.doc.selected_solid_ids().into_iter().filter(|id| *id != 2 && *id != 7).collect();
        assert_eq!(volumes(&state, &wall).len(), 2);
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        tool.axis = CutAxis::X;
        let (undo, _) = state.doc.history_len();
        click_3d(&mut tool, &mut state, &view, DVec3::new(96.0, 0.0, 32.0), shift);
        let plane = tool.plane().expect("plane");
        assert!((plane.normal.abs() - DVec3::X).length() < 1e-12 && (plane.dist.abs() - 96.0).abs() < 1e-9);
        assert_eq!(tool.crossing_count(&state), 4, "floor, both wall pieces and the trigger");
        assert!(tool.apply(&mut state));
        assert_eq!(state.doc.history_len().0, undo + 1);
        let owned: Vec<f64> = state
            .doc
            .scene()
            .solids()
            .values()
            .filter(|n| n.owner == hp_editor::Owner::Entity(6))
            .map(|n| n.geom.volume())
            .collect();
        assert_eq!(owned.len(), 2, "both trigger pieces stay in the brush entity");
        assert!(owned.iter().all(|v| (v - 32.0 * 64.0 * 64.0).abs() < 1e-6));
        assert!(all_valid(&state));
        assert_eq!(state.doc.selected_solid_ids().len(), 8);
    }

    #[test]
    fn dragging_3d_points_in_3d_and_2d() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        let view = floor_view();
        let a = view.world_to_screen(DVec3::new(0.0, -100.0, 0.0)).expect("a");
        let a2 = view.world_to_screen(DVec3::new(0.0, -90.0, 0.0)).expect("a2");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(a, a2));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(a, a2));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(a, a2));
        assert_eq!(tool.world_points(), vec![DVec3::new(0.0, -96.0, 0.0)], "press, drag and release places a point");
        click_3d(&mut tool, &mut state, &view, DVec3::new(0.0, 100.0, 0.0), Modifiers::NONE);
        let p0 = view.world_to_screen(DVec3::new(0.0, -96.0, 0.0)).expect("p0");
        let to = view.world_to_screen(DVec3::new(64.0, -96.0, 0.0)).expect("to");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(p0));
        assert_eq!(tool.cursor(&state, &view, p0), Some(CursorIcon::Grab));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(p0, p0));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(p0, to));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(p0, to));
        let plane = tool.plane().expect("plane");
        assert!(plane.distance(DVec3::new(64.0, -96.0, 0.0)).abs() < 1e-6);
        assert!(plane.normal.z.abs() < 1e-12, "still square to the floor");
        let top = info_2d(ViewKind::Top, 1.0);
        let h = top.world_to_screen(DVec3::new(0.0, 96.0, 0.0)).expect("h");
        let end = top.world_to_screen(DVec3::new(64.0, 96.0, 0.0)).expect("end");
        tool.handle_mouse(&mut state, &top, &Mouse::hover(h));
        assert_eq!(tool.cursor(&state, &top, h), Some(CursorIcon::Grab), "3D points have handles in 2D");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(h, h));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(h, end));
        let moved = tool.plane().expect("plane");
        assert!((moved.normal.abs() - DVec3::X).length() < 1e-12 && moved.normal.dot(plane.normal) > 0.0, "{moved:?}");
        assert!((moved.dist.abs() - 64.0).abs() < 1e-9);
        assert!(moved.distance(DVec3::new(64.0, 96.0, 0.0)).abs() < 1e-6);
    }

    #[test]
    fn fixed_plane_moves_with_its_point_in_2d() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        assert!(tool.set_plane(DVec3::new(0.0, 0.0, 0.0), Plane::new(DVec3::X, 0.0).expect("plane")));
        assert!(!tool.set_plane(DVec3::NAN, Plane::new(DVec3::X, 0.0).expect("plane")));
        let top = info_2d(ViewKind::Top, 1.0);
        let h = top.world_to_screen(DVec3::ZERO).expect("h");
        let end = top.world_to_screen(DVec3::new(48.0, 20.0, 0.0)).expect("end");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(h, h));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(h, end));
        let p = tool.plane().expect("plane");
        assert_eq!((p.normal, p.dist), (DVec3::X, 48.0));
        assert_eq!(tool.crossing_count(&state), 1);
    }

    #[test]
    fn slivers_are_not_cut() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        assert!(tool.set_plane(DVec3::new(255.9, 0.0, 0.0), Plane::new(DVec3::X, 255.9).expect("plane")));
        assert_eq!(tool.crossing_count(&state), 0);
        assert!(!tool.apply(&mut state));
        let tilt = Plane::from_points(
            DVec3::new(255.8, -300.0, 0.0),
            DVec3::new(256.0, 300.0, 0.0),
            DVec3::new(255.8, -300.0, -16.0),
        )
        .expect("plane");
        assert!(tool.set_plane(DVec3::new(256.0, 0.0, 0.0), tilt));
        assert_eq!(tool.crossing_count(&state), 0);
        assert!(tool.set_plane(DVec3::new(250.0, 0.0, 0.0), Plane::new(DVec3::X, 250.0).expect("plane")));
        assert_eq!(tool.crossing_count(&state), 1);
    }

    #[test]
    fn options_and_drawing_in_every_view() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(2)]);
        let mut tool = ClipTool::new();
        assert!(tool.set_option(&mut state, "keep", "front"));
        assert!(tool.set_option(&mut state, "axis", "Y"));
        assert!(tool.set_option(&mut state, "three_point", "on"));
        assert!(!tool.set_option(&mut state, "keep", "middle"));
        assert!(!tool.set_option(&mut state, "axis", "w"));
        assert!(!tool.set_option(&mut state, "nope", "1"));
        assert_eq!((tool.keep, tool.axis, tool.three_point), (ClipKeep::Front, CutAxis::Y, true));
        let values = tool.option_values();
        assert_eq!(values.iter().map(|o| o.key).collect::<Vec<_>>(), vec!["keep", "three_point", "axis"]);
        assert_eq!(values[0].value, "front");
        let view = floor_view();
        let views = [view, info_2d(ViewKind::Top, 0.5), info_2d(ViewKind::Front, 0.5), info_2d(ViewKind::Side, 0.5)];
        let hover = view.world_to_screen(DVec3::new(64.0, 64.0, 0.0)).expect("hover");
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        for mods in [Modifiers::NONE, shift, ctrl] {
            tool.handle_mouse(&mut state, &view, &Mouse::hover(hover).with_modifiers(mods));
            let mut out = Overlay::default();
            tool.overlay(&state, &view, &mut out);
            assert!(!out.points.is_empty(), "the snapped point under the mouse");
            if mods != Modifiers::NONE {
                assert!(!out.lines_on_top.is_empty() || !out.lines.is_empty(), "the candidate plane is outlined");
            }
        }
        click_3d(&mut tool, &mut state, &view, DVec3::new(0.0, -100.0, 0.0), Modifiers::NONE);
        tool.handle_mouse(&mut state, &view, &Mouse::hover(hover));
        click_3d(&mut tool, &mut state, &view, DVec3::new(0.0, 100.0, 0.0), Modifiers::NONE);
        tool.handle_mouse(&mut state, &view, &Mouse::hover(hover));
        for v in &views {
            let mut out = Overlay::default();
            tool.overlay(&state, v, &mut out);
            assert!(!out.lines.is_empty() || !out.lines_on_top.is_empty());
            let _ = tool.cursor(&state, v, v.rect.center());
        }
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        hp_app::testing::run_ui(&mut state, |ui, state| {
            tool.options_ui(ui, state);
            for v in &views {
                tool.paint(state, v, ui.painter());
            }
        });
        click_3d(&mut tool, &mut state, &view, DVec3::new(0.0, -100.0, 0.0), shift);
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        hp_app::testing::run_ui(&mut state, |ui, state| {
            for v in &views {
                tool.paint(state, v, ui.painter());
            }
        });
    }

    #[test]
    fn letter_keys_leave_shifted_shortcuts_alone() {
        let mut state = settle();
        let mut tool = ClipTool::new();
        let press =
            |key, modifiers| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
        let ctx = egui::Context::default();
        let mut run = |events: Vec<egui::Event>| {
            let mut handled = false;
            let mut left = 0;
            let input = egui::RawInput { events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                handled = tool.key_input(&mut state, ui.ctx());
                left = ui.ctx().input(|i| i.events.len());
            });
            out.textures_delta.clear();
            (handled, left)
        };
        assert_eq!(run(vec![press(Key::X, Modifiers::SHIFT)]), (false, 1), "Shift+X stays a shortcut");
        assert_eq!(run(vec![press(Key::C, Modifiers::SHIFT)]), (false, 1));
        assert_eq!(run(vec![press(Key::X, Modifiers::NONE)]), (true, 0));
        assert_eq!(tool.axis, CutAxis::X);
        assert!(!tool.three_point);
    }

    #[test]
    fn every_new_clip_string_is_translated() {
        let keys = [
            "create.clip.axis",
            "create.clip.axis_tip",
            "create.clip.hint_second",
            "create.clip.hint_ready_3d",
            "create.clip.hint_fixed",
        ];
        for k in keys {
            let row = crate::TR.iter().find(|(key, _, _)| *key == k).expect(k);
            assert!(!row.1.is_empty() && !row.2.is_empty() && row.1 != row.2, "{k}");
        }
        for k in ["create.clip.axis_auto", "create.clip.axis_x", "create.clip.axis_y", "create.clip.axis_z"] {
            assert!(crate::TR.iter().any(|(key, _, _)| *key == k), "{k}");
        }
    }
}
