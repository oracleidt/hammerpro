use egui::{
    Align2, Color32, CursorIcon, FontId, Key, Modifiers, Painter, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind,
    Vec2,
};
use glam::{DVec2, DVec3};
use hp_app::grid::GridFrame;
use hp_app::state::{RightTab, ThumbState};
use hp_app::{AppState, ViewInfo, ViewInput, ViewKind, t};
use hp_editor::{Id, ObjRef, PickOptions};
use hp_geom::{Aabb, Plane, Ray};
use hp_render::{Overlay, OverlayLine, OverlayPoint, OverlayTri};

pub(crate) const DEFAULT_TEX_SIZE: f64 = 64.0;

pub(crate) const MIN_EXTENT: f64 = 1e-3;

pub(crate) const COLOR_PENDING: Color32 = Color32::from_rgb(255, 214, 64);
pub(crate) const COLOR_SHAPE: Color32 = Color32::from_rgb(255, 245, 200);
pub(crate) const COLOR_GHOST: Color32 = Color32::from_rgb(120, 220, 255);
pub(crate) const COLOR_FRONT: Color32 = Color32::from_rgb(110, 210, 90);
pub(crate) const COLOR_BACK: Color32 = Color32::from_rgb(80, 150, 255);
pub(crate) const COLOR_REMOVED: Color32 = Color32::from_rgb(255, 95, 90);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Mouse {
    pub pos: Option<Pos2>,
    pub press_origin: Option<Pos2>,
    pub drag_started: bool,
    pub dragging: bool,
    pub drag_stopped: bool,
    pub clicked: bool,
    pub double_clicked: bool,
    pub modifiers: Modifiers,
}

impl Mouse {
    pub(crate) fn from_input(input: &ViewInput<'_>) -> Self {
        let r = input.response;
        let b = PointerButton::Primary;
        Self {
            pos: input.pointer,
            press_origin: r.ctx.input(|i| i.pointer.press_origin()),
            drag_started: r.drag_started_by(b),
            dragging: r.dragged_by(b),
            drag_stopped: r.drag_stopped_by(b),
            clicked: r.clicked_by(b),
            double_clicked: r.double_clicked_by(b) || r.triple_clicked_by(b),
            modifiers: input.modifiers,
        }
    }

    pub(crate) fn origin(&self) -> Option<Pos2> {
        self.press_origin.or(self.pos)
    }

    pub(crate) fn any_button(&self) -> bool {
        self.drag_started || self.dragging || self.drag_stopped || self.clicked
    }

    pub(crate) fn activated(&self) -> bool {
        self.clicked || self.drag_started
    }

    pub(crate) fn hovering(&self) -> bool {
        self.pos.is_some() && !self.any_button()
    }
}

#[cfg(test)]
impl Mouse {
    pub(crate) fn hover(p: Pos2) -> Self {
        Self { pos: Some(p), ..Self::default() }
    }
    pub(crate) fn drag_start(origin: Pos2, p: Pos2) -> Self {
        Self { pos: Some(p), press_origin: Some(origin), drag_started: true, dragging: true, ..Self::default() }
    }
    pub(crate) fn drag(origin: Pos2, p: Pos2) -> Self {
        Self { pos: Some(p), press_origin: Some(origin), dragging: true, ..Self::default() }
    }
    pub(crate) fn drag_end(origin: Pos2, p: Pos2) -> Self {
        Self { pos: Some(p), press_origin: Some(origin), drag_stopped: true, ..Self::default() }
    }
    pub(crate) fn click(p: Pos2) -> Self {
        Self { pos: Some(p), press_origin: Some(p), clicked: true, ..Self::default() }
    }
    pub(crate) fn double_click(p: Pos2) -> Self {
        Self { double_clicked: true, ..Self::click(p) }
    }
    pub(crate) fn with_modifiers(mut self, modifiers: Modifiers) -> Self {
        self.modifiers = modifiers;
        self
    }
}

pub(crate) fn consume_plain_key(ctx: &egui::Context, key: Key) -> bool {
    ctx.input_mut(|i| {
        if i.modifiers.alt || i.modifiers.ctrl || i.modifiers.command || i.modifiers.mac_cmd {
            return false;
        }
        i.consume_key(Modifiers::NONE, key)
    })
}

pub(crate) fn consume_tabs(ctx: &egui::Context) -> Vec<bool> {
    ctx.input_mut(|i| {
        let mut presses = Vec::new();
        i.events.retain(|e| match e {
            egui::Event::Key { key: Key::Tab, pressed: true, modifiers: m, .. }
                if !(m.alt || m.ctrl || m.command || m.mac_cmd) =>
            {
                presses.push(m.shift);
                false
            }
            _ => true,
        });
        presses
    })
}

pub(crate) fn fresh(state: &AppState, frame: u64) -> bool {
    frame.saturating_add(1) >= state.frame.index
}

pub(crate) fn clean(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    let r = x.round();
    if (x - r).abs() < 1e-6 { r + 0.0 } else { x + 0.0 }
}

pub(crate) fn round_to(x: f64, step: f64) -> f64 {
    if !(step.is_finite() && step > 0.0 && x.is_finite()) {
        return x;
    }
    let r = (x / step).round() * step;
    let digits = (0..12).find(|d| {
        let scaled = step * 10f64.powi(*d);
        (scaled - scaled.round()).abs() < 1e-9 * scaled.max(1.0)
    });
    let scale = 10f64.powi(digits.unwrap_or(12));
    clean((r * scale).round() / scale)
}

pub(crate) fn clean_vec(v: DVec3) -> DVec3 {
    DVec3::new(clean(v.x), clean(v.y), clean(v.z))
}

pub(crate) fn dominant_axis(n: DVec3) -> usize {
    let a = n.abs();
    if a.z >= a.x && a.z >= a.y {
        2
    } else if a.x >= a.y {
        0
    } else {
        1
    }
}

pub(crate) fn fmt_num(x: f64) -> String {
    let x = clean(x);
    if x == x.round() && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

pub(crate) fn fmt_vec(v: DVec3) -> String {
    format!("{} {} {}", fmt_num(v.x), fmt_num(v.y), fmt_num(v.z))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Snap {
    frame: Option<GridFrame>,
    kind: ViewKind,
}

impl Snap {
    pub(crate) const OFF: Snap = Snap { frame: None, kind: ViewKind::Top };

    pub(crate) fn new(state: &AppState, view: &ViewInfo, modifiers: Modifiers) -> Self {
        Self::frame(state, view.kind, modifiers, state.grid_frame(view.kind))
    }

    pub(crate) fn on_object(state: &AppState, view: &ViewInfo, modifiers: Modifiers, obj: Option<ObjRef>) -> Self {
        Self::frame(state, view.kind, modifiers, state.surface_grid_frame(view.kind, obj))
    }

    pub(crate) fn for_selection(state: &AppState, view: &ViewInfo, modifiers: Modifiers) -> Self {
        Self::frame(state, view.kind, modifiers, state.edit_grid_frame(view.kind))
    }

    fn frame(state: &AppState, kind: ViewKind, modifiers: Modifiers, frame: GridFrame) -> Self {
        if !state.snap_enabled() || modifiers.alt {
            return Snap { frame: None, kind };
        }
        Snap { frame: Some(frame), kind }
    }

    pub(crate) fn grid(grid: f64) -> Self {
        if grid.is_finite() && grid > 0.0 {
            Snap { frame: Some(GridFrame::uniform(grid)), kind: ViewKind::Top }
        } else {
            Self::OFF
        }
    }

    #[cfg(test)]
    pub(crate) fn with_frame(frame: GridFrame, kind: ViewKind) -> Self {
        Snap { frame: Some(frame), kind }
    }

    pub(crate) fn is_on(&self) -> bool {
        self.frame.is_some()
    }

    pub(crate) fn step(&self) -> f64 {
        self.frame.map_or(1.0, |f| f.nominal())
    }

    pub(crate) fn step_along(&self, axis: usize) -> f64 {
        self.frame.map_or(1.0, |f| f.step_along(axis))
    }

    pub(crate) fn v(&self, x: f64, axis: usize) -> f64 {
        match self.frame {
            Some(f) if x.is_finite() => clean(f.axis(axis).snap(x)),
            _ => x,
        }
    }

    pub(crate) fn len(&self, x: f64, axis: usize) -> f64 {
        match self.frame {
            Some(f) if x.is_finite() => {
                let step = f.step_along(axis);
                clean((x / step).round() * step)
            }
            _ => x,
        }
    }

    pub(crate) fn tool_step(&self) -> f64 {
        self.frame.map_or(1.0, |f| f.tool_nominal())
    }

    pub(crate) fn tool_radius(&self, x: f64) -> f64 {
        match self.frame {
            Some(f) if x.is_finite() => {
                let step = f.tool_nominal();
                clean((x / step).round() * step)
            }
            _ => x,
        }
    }

    pub(crate) fn v2(&self, p: DVec2) -> DVec2 {
        match self.frame {
            Some(f) => {
                let q = f.snap_2d(self.kind, p);
                DVec2::new(clean(q.x), clean(q.y))
            }
            None => p,
        }
    }

    pub(crate) fn delta2(&self, reference: DVec2, raw: DVec2) -> DVec2 {
        match self.frame {
            Some(f) => f.snap_delta_2d(self.kind, reference, raw),
            None => raw,
        }
    }

    pub(crate) fn on_plane(&self, p: DVec3, n: DVec3) -> DVec3 {
        match self.frame {
            Some(f) => {
                let q = clean_vec(f.on_plane(p, n));
                if q.is_finite() { q } else { p }
            }
            None => p,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DistanceSnap {
    frame: Option<GridFrame>,
    alternate: bool,
}

impl DistanceSnap {
    pub(crate) const OFF: DistanceSnap = DistanceSnap { frame: None, alternate: false };

    pub(crate) fn for_inset(state: &AppState, view: &ViewInfo, modifiers: Modifiers) -> Self {
        let forced = modifiers.ctrl || modifiers.command;
        if (state.snap_enabled() && !modifiers.alt) || forced {
            Self { frame: Some(state.edit_grid_frame(view.kind)), alternate: false }
        } else {
            Self::OFF
        }
    }

    pub(crate) fn for_extrude(state: &AppState, view: &ViewInfo, modifiers: Modifiers) -> Self {
        let forced = modifiers.ctrl || modifiers.command;
        if !(state.snap_enabled() || forced) {
            return Self::OFF;
        }
        let base = state.edit_grid_frame(view.kind);
        if modifiers.alt {
            Self { frame: Some(base.alternate()), alternate: true }
        } else {
            Self { frame: Some(base), alternate: false }
        }
    }

    #[cfg(test)]
    pub(crate) fn with_frame(frame: GridFrame, alternate: bool) -> Self {
        Self { frame: Some(frame), alternate }
    }

    pub(crate) fn is_on(&self) -> bool {
        self.frame.is_some()
    }

    pub(crate) fn is_alternate(&self) -> bool {
        self.alternate
    }

    pub(crate) fn step(&self, normal: DVec3) -> Option<f64> {
        self.frame.map(|f| f.plane_step(normal))
    }

    pub(crate) fn nominal(&self) -> Option<f64> {
        self.frame.map(|f| f.tool_nominal())
    }

    pub(crate) fn thickness(&self, value: f64, normal: DVec3) -> f64 {
        match self.step(normal) {
            Some(step) if value.is_finite() => clean((value / step).round() * step),
            _ => value,
        }
    }

    pub(crate) fn distance(&self, from: DVec3, normal: DVec3, value: f64) -> f64 {
        match self.frame {
            Some(f) => clean(f.snap_distance(from, normal, value)),
            None => value,
        }
    }
}

pub(crate) fn depth_axis(kind: ViewKind) -> usize {
    kind.axes_2d().map_or(2, |a| a.2)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfaceHit {
    pub point: DVec3,
    pub normal: DVec3,
    pub face_normal: DVec3,
    pub face_plane: Plane,
    pub solid: Id,
    pub side_index: usize,
    pub side_id: Id,
}

pub(crate) fn pick_surface(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<SurfaceHit> {
    pick_surface_with(state, view, pos, None)
}

pub(crate) fn pick_surface_with(
    state: &AppState,
    view: &ViewInfo,
    pos: Pos2,
    filter: Option<&dyn Fn(ObjRef) -> bool>,
) -> Option<SurfaceHit> {
    let (origin, dir) = view.ray(pos);
    if !origin.is_finite() || !dir.is_finite() {
        return None;
    }
    let ray = Ray::new(origin, dir);
    let opts = PickOptions { include_entities: false, filter, ..PickOptions::default() };
    let hit = state.doc.pick_ray(&ray, &opts, &|e| state.entity_bounds(e))?;
    let ObjRef::Solid(solid) = hit.obj else { return None };
    let side_index = hit.face?;
    let node = state.doc.scene().solid(solid)?;
    let side_id = node.solid.sides.get(side_index)?.id;
    let face_plane = node
        .geom
        .face_for_side(side_index)
        .map(|f| f.plane)
        .filter(|p| p.normal.is_finite() && p.normal.length_squared() > 0.5 && p.dist.is_finite())
        .or_else(|| Plane::from_normal_point(hit.normal, hit.point))?;
    let normal = hit.normal.try_normalize().unwrap_or(face_plane.normal);
    hit.point.is_finite().then_some(SurfaceHit {
        point: hit.point,
        normal,
        face_normal: face_plane.normal,
        face_plane,
        solid,
        side_index,
        side_id,
    })
}

pub(crate) fn ground_point(view: &ViewInfo, pos: Pos2) -> Option<DVec3> {
    let (o, d) = view.ray(pos);
    if d.z.abs() < 1e-9 {
        return None;
    }
    let t = -o.z / d.z;
    (t > 0.0 && t.is_finite() && t < 1e6).then(|| {
        let p = o + d * t;
        DVec3::new(p.x, p.y, 0.0)
    })
}

pub(crate) fn box_screen_rect(view: &ViewInfo, b: &Aabb) -> Option<Rect> {
    if b.is_empty() {
        return None;
    }
    let a = view.world_to_screen(b.min)?;
    let c = view.world_to_screen(b.max)?;
    let r = Rect::from_two_pos(a, c);
    (r.min.x.is_finite() && r.min.y.is_finite() && r.max.x.is_finite() && r.max.y.is_finite()).then_some(r)
}

pub(crate) const HANDLE_TOLERANCE: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Edge {
    Min,
    Max,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BoxHandle {
    pub h: Option<Edge>,
    pub v: Option<Edge>,
}

impl BoxHandle {
    pub(crate) const MOVE: BoxHandle = BoxHandle { h: None, v: None };

    pub(crate) fn is_move(self) -> bool {
        self == Self::MOVE
    }

    pub(crate) fn cursor(self) -> CursorIcon {
        match (self.h, self.v) {
            (None, None) => CursorIcon::Move,
            (Some(_), None) => CursorIcon::ResizeHorizontal,
            (None, Some(_)) => CursorIcon::ResizeVertical,
            (Some(Edge::Min), Some(Edge::Max)) | (Some(Edge::Max), Some(Edge::Min)) => CursorIcon::ResizeNwSe,
            _ => CursorIcon::ResizeNeSw,
        }
    }

    pub(crate) const RESIZE: [BoxHandle; 8] = [
        BoxHandle { h: Some(Edge::Min), v: Some(Edge::Min) },
        BoxHandle { h: None, v: Some(Edge::Min) },
        BoxHandle { h: Some(Edge::Max), v: Some(Edge::Min) },
        BoxHandle { h: Some(Edge::Max), v: None },
        BoxHandle { h: Some(Edge::Max), v: Some(Edge::Max) },
        BoxHandle { h: None, v: Some(Edge::Max) },
        BoxHandle { h: Some(Edge::Min), v: Some(Edge::Max) },
        BoxHandle { h: Some(Edge::Min), v: None },
    ];

    pub(crate) fn screen_pos(self, rect: Rect) -> Pos2 {
        let x = match self.h {
            Some(Edge::Min) => rect.left(),
            Some(Edge::Max) => rect.right(),
            None => rect.center().x,
        };
        let y = match self.v {
            Some(Edge::Min) => rect.bottom(),
            Some(Edge::Max) => rect.top(),
            None => rect.center().y,
        };
        Pos2::new(x, y)
    }
}

pub(crate) fn footprint_box(kind: ViewKind, a: DVec2, b: DVec2, depth: (f64, f64)) -> Option<Aabb> {
    let (r, u, d) = kind.axes_2d()?;
    let mut min = DVec3::ZERO;
    let mut max = DVec3::ZERO;
    min[r] = a.x.min(b.x);
    max[r] = a.x.max(b.x);
    min[u] = a.y.min(b.y);
    max[u] = a.y.max(b.y);
    min[d] = depth.0.min(depth.1);
    max[d] = depth.0.max(depth.1);
    let b = Aabb { min, max };
    (b.size().min_element() > MIN_EXTENT && b.min.is_finite() && b.max.is_finite()).then_some(b)
}

pub(crate) fn edit_box(start: &Aabb, kind: ViewKind, handle: BoxHandle, from: DVec2, to: DVec2, snap: &Snap) -> Aabb {
    let Some((r, u, _)) = kind.axes_2d() else { return *start };
    let delta = to - from;
    if !delta.is_finite() {
        return *start;
    }
    let mut min = start.min;
    let mut max = start.max;
    if handle.is_move() {
        let d = snap.delta2(DVec2::new(start.min[r], start.min[u]), delta);
        let (dr, du) = (d.x, d.y);
        min[r] += dr;
        max[r] += dr;
        min[u] += du;
        max[u] += du;
        return Aabb { min: clean_vec(min), max: clean_vec(max) };
    }
    match handle.h {
        Some(Edge::Min) => min[r] = snap.v(start.min[r] + delta.x, r),
        Some(Edge::Max) => max[r] = snap.v(start.max[r] + delta.x, r),
        None => {}
    }
    match handle.v {
        Some(Edge::Min) => min[u] = snap.v(start.min[u] + delta.y, u),
        Some(Edge::Max) => max[u] = snap.v(start.max[u] + delta.y, u),
        None => {}
    }
    let b = Aabb::new(min, max);
    if b.size().min_element() < MIN_EXTENT { *start } else { b }
}

pub(crate) fn hit_handle(view: &ViewInfo, b: &Aabb, pos: Pos2) -> Option<BoxHandle> {
    let rect = box_screen_rect(view, b)?;
    if !rect.expand(HANDLE_TOLERANCE).contains(pos) {
        return None;
    }
    let pick = |p: f32, at_min: f32, at_max: f32, span: f32| -> Option<Edge> {
        let (dl, dh) = ((p - at_min).abs(), (p - at_max).abs());
        if dl.min(dh) > HANDLE_TOLERANCE {
            return None;
        }
        let inside = p > at_min.min(at_max) && p < at_min.max(at_max);
        if span < 3.0 * HANDLE_TOLERANCE && inside {
            return None;
        }
        Some(if dl <= dh { Edge::Min } else { Edge::Max })
    };
    let h = pick(pos.x, rect.left(), rect.right(), rect.width());
    let v = pick(pos.y, rect.bottom(), rect.top(), rect.height());
    Some(BoxHandle { h, v })
}

pub(crate) fn plane_box_polygon(plane: &Plane, b: &Aabb) -> Vec<DVec3> {
    if b.is_empty() {
        return Vec::new();
    }
    let half = (b.size().length() * 2.0).max(64.0);
    let mut poly: Vec<DVec3> = hp_geom::base_polygon(plane, half).to_vec();
    let faces = [
        (DVec3::X, b.max.x),
        (DVec3::NEG_X, -b.min.x),
        (DVec3::Y, b.max.y),
        (DVec3::NEG_Y, -b.min.y),
        (DVec3::Z, b.max.z),
        (DVec3::NEG_Z, -b.min.z),
    ];
    for (n, d) in faces {
        let Some(p) = Plane::new(n, d) else { continue };
        poly = hp_geom::clip_polygon(&poly, &p, 1e-6).1;
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly
}

pub(crate) fn f3(p: DVec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

pub(crate) fn rgba(c: Color32, alpha: f32) -> [f32; 4] {
    [f32::from(c.r()) / 255.0, f32::from(c.g()) / 255.0, f32::from(c.b()) / 255.0, alpha.clamp(0.0, 1.0)]
}

const BOX_EDGES: [(usize, usize); 12] =
    [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];

const BOX_FACES: [[usize; 4]; 6] = [[0, 2, 6, 4], [1, 5, 7, 3], [0, 4, 5, 1], [2, 3, 7, 6], [0, 1, 3, 2], [4, 6, 7, 5]];

pub(crate) struct Ov<'a> {
    out: &'a mut Overlay,
    is_2d: bool,
}

impl<'a> Ov<'a> {
    pub(crate) fn new(out: &'a mut Overlay, view: &ViewInfo) -> Self {
        Self { out, is_2d: view.is_2d() }
    }

    pub(crate) fn line(&mut self, a: DVec3, b: DVec3, color: [f32; 4]) {
        let l = OverlayLine { a: f3(a), b: f3(b), color };
        if self.is_2d {
            self.out.lines_on_top.push(l);
        } else {
            self.out.lines.push(l);
        }
    }

    pub(crate) fn xray_line(&mut self, a: DVec3, b: DVec3, color: [f32; 4]) {
        let l = OverlayLine { a: f3(a), b: f3(b), color };
        if self.is_2d {
            self.out.lines_on_top.push(l);
        } else {
            self.out.lines.push(l);
            let mut faint = color;
            faint[3] *= 0.35;
            self.out.lines_on_top.push(OverlayLine { color: faint, ..l });
        }
    }

    pub(crate) fn tri(&mut self, a: DVec3, b: DVec3, c: DVec3, color: [f32; 4]) {
        if !self.is_2d {
            self.out.triangles_on_top.push(OverlayTri { p: [f3(a), f3(b), f3(c)], color });
        }
    }

    pub(crate) fn polygon_fill(&mut self, pts: &[DVec3], color: [f32; 4]) {
        let Some(&first) = pts.first() else { return };
        for w in pts.get(1..).unwrap_or(&[]).windows(2) {
            if let [b, c] = w {
                self.tri(first, *b, *c, color);
            }
        }
    }

    pub(crate) fn polygon_outline(&mut self, pts: &[DVec3], color: [f32; 4], xray: bool) {
        let n = pts.len();
        for i in 0..n {
            if let (Some(&a), Some(&b)) = (pts.get(i), pts.get((i + 1) % n)) {
                if xray {
                    self.xray_line(a, b, color);
                } else {
                    self.line(a, b, color);
                }
            }
        }
    }

    pub(crate) fn hull_edges(&mut self, c: &[DVec3; 8], color: [f32; 4], xray: bool) {
        for (i, j) in BOX_EDGES {
            if xray {
                self.xray_line(c[i], c[j], color);
            } else {
                self.line(c[i], c[j], color);
            }
        }
    }

    pub(crate) fn hull_faces(&mut self, c: &[DVec3; 8], color: [f32; 4]) {
        for f in BOX_FACES {
            self.polygon_fill(&f.map(|i| c[i]), color);
        }
    }

    pub(crate) fn box_edges(&mut self, b: &Aabb, color: [f32; 4], xray: bool) {
        self.hull_edges(&b.corners(), color, xray);
    }

    pub(crate) fn box_faces(&mut self, b: &Aabb, color: [f32; 4]) {
        self.hull_faces(&b.corners(), color);
    }

    pub(crate) fn point(&mut self, p: DVec3, color: [f32; 4], size_px: f32) {
        self.out.points.push(OverlayPoint { pos: f3(p), color, size_px });
    }
}

pub(crate) fn paint_handle(painter: &Painter, center: Pos2, hovered: bool) {
    let size = if hovered { 9.0 } else { 7.0 };
    let r = Rect::from_center_size(center, Vec2::splat(size));
    let fill = if hovered { hp_app::theme::VIEW_ACCENT } else { Color32::WHITE };
    painter.rect_filled(r, 1.0, fill);
    painter.rect_stroke(r, 1.0, Stroke::new(1.0, Color32::from_black_alpha(220)), StrokeKind::Inside);
}

pub(crate) fn paint_label(painter: &Painter, pos: Pos2, anchor: Align2, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(11.5), color);
    let rect = anchor.anchor_size(pos, galley.size() + Vec2::new(8.0, 4.0));
    painter.rect_filled(rect, 3.0, Color32::from_black_alpha(185));
    painter.galley(rect.min + Vec2::new(4.0, 2.0), galley, color);
}

pub(crate) fn fmt_len(v: f64) -> String {
    hp_app::widgets::fmt_coord(clean(v))
}

pub(crate) fn fmt_size(b: &Aabb) -> String {
    let s = b.size();
    format!("{} x {} x {}", fmt_len(s.x), fmt_len(s.y), fmt_len(s.z))
}

pub(crate) fn active_material(state: &AppState) -> String {
    let m = state.current_material.trim();
    if !m.is_empty() {
        return m.to_owned();
    }
    let d = state.settings.edit.default_material.trim();
    if d.is_empty() { "dev/dev_measuregeneric01b".to_owned() } else { d.to_owned() }
}

pub(crate) fn material_size(state: &mut AppState, name: &str) -> DVec2 {
    match state.assets.material_info(name) {
        Some(info) if info.width > 0 && info.height > 0 => {
            let scale = info.decal_scale.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0);
            DVec2::new(f64::from(info.width), f64::from(info.height)) * f64::from(scale)
        }
        _ => DVec2::splat(DEFAULT_TEX_SIZE),
    }
}

pub(crate) fn fgd_defaults(state: &AppState, class: &str) -> Vec<(String, String)> {
    let Some(c) = state.assets.fgd().and_then(|f| f.class(class)) else { return Vec::new() };
    let mut kv: Vec<(String, String)> = c
        .default_keyvalues()
        .into_iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("origin") && !k.eq_ignore_ascii_case("classname"))
        .collect();
    if c.spawnflags().is_some() && !kv.iter().any(|(k, _)| k.eq_ignore_ascii_case("spawnflags")) {
        kv.push(("spawnflags".to_owned(), c.default_spawnflags().to_string()));
    }
    kv
}

pub(crate) fn set_kv(kv: &mut Vec<(String, String)>, key: &str, value: impl Into<String>) {
    let value = value.into();
    match kv.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        Some(slot) => slot.1 = value,
        None => kv.push((key.to_owned(), value)),
    }
}

pub(crate) fn create_entity(
    state: &mut AppState,
    class: &str,
    origin: DVec3,
    kv: &[(String, String)],
    keep_selection: bool,
) -> Result<Id, hp_editor::EditError> {
    if !keep_selection {
        return state.doc.create_point_entity(class, origin, kv);
    }
    let previous = state.doc.selected_objects();
    state.doc.transaction("Create entity", |doc| {
        let id = doc.create_point_entity(class, origin, kv)?;
        doc.select(&previous, hp_editor::SelectOp::Replace);
        Ok::<Id, hp_editor::EditError>(id)
    })
}

pub(crate) fn material_chip(ui: &mut egui::Ui, state: &mut AppState) {
    let name = active_material(state);
    let thumb = state.assets.thumbnail(ui.ctx(), &name);
    let height = 22.0;
    let label_width = 180.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(height + 6.0 + label_width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let visuals = ui.style().interact(&resp);
        if resp.hovered() {
            painter.rect_filled(rect.expand(1.0), 3.0, visuals.weak_bg_fill);
        }
        let img = Rect::from_min_size(rect.min, Vec2::splat(height));
        match thumb {
            ThumbState::Ready { texture, .. } => {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                painter.image(texture, img, uv, Color32::WHITE);
            }
            ThumbState::Loading => {
                painter.rect_filled(img, 2.0, ui.visuals().extreme_bg_color);
                painter.text(
                    img.center(),
                    Align2::CENTER_CENTER,
                    "…",
                    FontId::proportional(12.0),
                    ui.visuals().weak_text_color(),
                );
            }
            ThumbState::Missing => {
                let half = img.size() * 0.5;
                for (i, j) in [(0.0, 0.0), (1.0, 1.0)] {
                    let q = Rect::from_min_size(img.min + Vec2::new(half.x * i, half.y * j), half);
                    painter.rect_filled(q, 0.0, Color32::from_rgb(255, 0, 255));
                }
                for (i, j) in [(1.0, 0.0), (0.0, 1.0)] {
                    let q = Rect::from_min_size(img.min + Vec2::new(half.x * i, half.y * j), half);
                    painter.rect_filled(q, 0.0, Color32::BLACK);
                }
            }
        }
        painter.rect_stroke(img, 2.0, Stroke::new(1.0, visuals.bg_stroke.color), StrokeKind::Inside);
        let text_rect = Rect::from_min_max(Pos2::new(img.right() + 6.0, rect.top()), rect.max);
        let mut job =
            egui::text::LayoutJob::simple_singleline(name.clone(), FontId::proportional(12.5), visuals.text_color());
        job.wrap = egui::text::TextWrapping::truncate_at_width(label_width);
        let galley = painter.layout_job(job);
        let y = text_rect.center().y - galley.size().y * 0.5;
        painter.galley(Pos2::new(text_rect.left(), y), galley, visuals.text_color());
    }
    let resp = resp.on_hover_text(format!("{}\n{}", name, t!("create.material_tip")));
    if resp.clicked() {
        state.ui.right_tab = RightTab::Textures;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::info_2d;

    fn b(min: [f64; 3], max: [f64; 3]) -> Aabb {
        Aabb::new(DVec3::from_array(min), DVec3::from_array(max))
    }

    #[test]
    fn snapping() {
        let s = Snap::grid(16.0);
        assert_eq!(s.v(7.0, 0), 0.0);
        assert_eq!(s.v(9.0, 1), 16.0);
        assert_eq!(s.v(-9.0, 2), -16.0);
        assert_eq!(Snap::OFF.v(9.5, 0), 9.5);
        let spec = hp_app::settings::GridSpec {
            name: "R".into(),
            step: [12.0, 12.0, 8.0],
            angles: [0.0, 45.0, 0.0],
            ..Default::default()
        };
        let frame = GridFrame::from_spec(&spec, 16.0);
        let r = Snap::with_frame(frame, ViewKind::Top);
        let q = r.v2(DVec2::new(10.0, 3.0));
        let l = frame.to_local(DVec3::new(q.x, q.y, 0.0)) / frame.step;
        assert!((l.x - l.x.round()).abs() < 1e-6 && (l.y - l.y.round()).abs() < 1e-6, "{q} {l}");
        assert_eq!(r.len(13.0, 2), 16.0);
        assert_eq!(r.step(), 8.0);
        assert_eq!(r.step_along(2), 8.0);
        let d = r.delta2(DVec2::new(1.0, 1.0), DVec2::new(20.0, 0.0));
        let l = frame.to_local(DVec3::new(1.0 + d.x, 1.0 + d.y, 0.0)) / frame.step;
        assert!((l.x - l.x.round()).abs() < 1e-6 && (l.y - l.y.round()).abs() < 1e-6, "{d}");
        assert_eq!(Snap::grid(0.0), Snap::OFF);
        assert_eq!(Snap::grid(f64::NAN), Snap::OFF);
        let p = s.on_plane(DVec3::new(9.0, 23.0, 5.0), DVec3::Z);
        assert_eq!(p, DVec3::new(16.0, 16.0, 5.0));
        let n = DVec3::new(0.0, 1.0, 1.0).normalize();
        let on = DVec3::new(3.0, 10.0, -10.0);
        let q = s.on_plane(on, n);
        assert!((n.dot(q) - n.dot(on)).abs() < 1e-9);
        assert_eq!(q.x, 0.0);
    }

    #[test]
    fn rounding_to_a_step() {
        assert_eq!(round_to(20.010_789_871_215_82, 1.0), 20.0);
        assert_eq!(round_to(-0.113_388_687_372_207_64, 1.0), 0.0);
        assert_eq!(round_to(0.07 * 3.0, 0.01), 0.21);
        assert_eq!(round_to(13.333_333_333, 0.001), 13.333);
        assert_eq!(round_to(0.13, 0.125), 0.125);
        assert_eq!(round_to(5.5, 0.0), 5.5);
        assert!(round_to(f64::NAN, 1.0).is_nan());
        assert_eq!(fmt_num(round_to(-224.000_000_1, 0.01)), "-224");
    }

    #[test]
    fn number_format() {
        assert_eq!(fmt_num(1.0), "1");
        assert_eq!(fmt_num(-0.0), "0");
        assert_eq!(fmt_num(0.5), "0.5");
        assert_eq!(fmt_num(1.0 / 3.0), "0.333333");
        assert_eq!(fmt_num(-2.000_000_1), "-2");
        assert_eq!(fmt_vec(DVec3::new(1.0, -0.0, 0.25)), "1 0 0.25");
    }

    #[test]
    fn plane_box_section() {
        let b = Aabb::new(DVec3::splat(-10.0), DVec3::splat(10.0));
        let p = Plane::from_normal_point(DVec3::X, DVec3::ZERO).expect("plane");
        let poly = plane_box_polygon(&p, &b);
        assert_eq!(poly.len(), 4);
        for v in &poly {
            assert!(v.x.abs() < 1e-9);
            assert!(b.expand(1e-6).contains_point(*v));
        }
        let miss = Plane::from_normal_point(DVec3::X, DVec3::new(50.0, 0.0, 0.0)).expect("plane");
        assert!(plane_box_polygon(&miss, &b).is_empty());
    }

    #[test]
    fn edit_box_handles_in_every_view() {
        let start = b([0.0, 0.0, 0.0], [64.0, 32.0, 16.0]);
        let snap = Snap::grid(8.0);
        let r = edit_box(
            &start,
            ViewKind::Top,
            BoxHandle { h: Some(Edge::Max), v: None },
            DVec2::ZERO,
            DVec2::new(13.0, 99.0),
            &snap,
        );
        assert_eq!(r, b([0.0, 0.0, 0.0], [80.0, 32.0, 16.0]));
        let r = edit_box(
            &start,
            ViewKind::Top,
            BoxHandle { h: None, v: Some(Edge::Min) },
            DVec2::ZERO,
            DVec2::new(0.0, -10.0),
            &snap,
        );
        assert_eq!(r, b([0.0, -8.0, 0.0], [64.0, 32.0, 16.0]));
        let r = edit_box(
            &start,
            ViewKind::Front,
            BoxHandle { h: Some(Edge::Min), v: Some(Edge::Max) },
            DVec2::ZERO,
            DVec2::new(-16.0, 16.0),
            &snap,
        );
        assert_eq!(r, b([0.0, -16.0, 0.0], [64.0, 32.0, 32.0]));
        let r = edit_box(
            &start,
            ViewKind::Side,
            BoxHandle { h: None, v: Some(Edge::Max) },
            DVec2::ZERO,
            DVec2::new(0.0, 40.0),
            &snap,
        );
        assert_eq!(r, b([0.0, 0.0, 0.0], [64.0, 32.0, 56.0]));
        let r = edit_box(&start, ViewKind::Side, BoxHandle::MOVE, DVec2::ZERO, DVec2::new(5.0, 11.0), &snap);
        assert_eq!(r, b([8.0, 0.0, 8.0], [72.0, 32.0, 24.0]));
        let r = edit_box(
            &start,
            ViewKind::Top,
            BoxHandle { h: Some(Edge::Max), v: None },
            DVec2::ZERO,
            DVec2::new(-96.0, 0.0),
            &snap,
        );
        assert_eq!(r, b([-32.0, 0.0, 0.0], [0.0, 32.0, 16.0]));
        let r = edit_box(
            &start,
            ViewKind::Top,
            BoxHandle { h: Some(Edge::Max), v: None },
            DVec2::ZERO,
            DVec2::new(-64.0, 0.0),
            &snap,
        );
        assert_eq!(r, start);
        let r = edit_box(
            &start,
            ViewKind::Top,
            BoxHandle { h: Some(Edge::Max), v: None },
            DVec2::ZERO,
            DVec2::new(1.5, 0.0),
            &Snap::OFF,
        );
        assert_eq!(r.max.x, 65.5);
    }

    #[test]
    fn handle_hit_testing() {
        for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
            let view = info_2d(kind, 1.0);
            let bx = b([-64.0, -64.0, -64.0], [64.0, 64.0, 64.0]);
            let rect = box_screen_rect(&view, &bx).expect("rect");
            for h in BoxHandle::RESIZE {
                assert_eq!(hit_handle(&view, &bx, h.screen_pos(rect)), Some(h), "{kind:?} {h:?}");
            }
            assert_eq!(hit_handle(&view, &bx, rect.center()), Some(BoxHandle::MOVE));
            assert_eq!(hit_handle(&view, &bx, rect.right_bottom() + Vec2::splat(30.0)), None);
            let top = hit_handle(&view, &bx, Pos2::new(rect.center().x, rect.top())).expect("top");
            assert_eq!(top, BoxHandle { h: None, v: Some(Edge::Max) });
        }
    }

    #[test]
    fn keyvalue_replace() {
        let mut kv = vec![("Angles".to_owned(), "0 0 0".to_owned())];
        set_kv(&mut kv, "angles", "0 90 0");
        set_kv(&mut kv, "model", "x.mdl");
        assert_eq!(kv, vec![("Angles".to_owned(), "0 90 0".to_owned()), ("model".to_owned(), "x.mdl".to_owned())]);
    }
}
