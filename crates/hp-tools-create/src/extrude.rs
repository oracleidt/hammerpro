use std::cell::RefCell;

use egui::{Align2, CursorIcon, Key, Modifiers, Pos2, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::{Action, key_label, shortcut_label};
use hp_app::settings::{ExtrudeAxisSetting, ExtrudeDirectionSetting};
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, t, tf};
use hp_editor::{EditError, FaceRef, Id, ObjRef, PickOptions};
use hp_geom::{
    Aabb, ExtrudeIssue, ExtrudeMode, FaceExtrudeParams, FaceFrame, GeomError, MAX_OUTLINE_POINTS, MIN_EXTRUDE_DEPTH,
    Plane, Ray, face_extrude, face_thickness, outline_is_simple, path_is_simple,
};
use hp_render::Overlay;

use crate::clip3d::{MAGNET_PX, magnet, snap_targets};
use crate::common::{
    COLOR_FRONT, COLOR_PENDING, COLOR_REMOVED, DistanceSnap, HANDLE_TOLERANCE, Mouse, Ov, Snap, active_material,
    clean_vec, consume_plain_key, consume_tabs, fmt_len, fresh, material_chip, paint_label, pick_surface_with, rgba,
};

pub const MAX_EXTRUDE_DEPTH: f64 = 16384.0;

#[derive(Clone, Debug, PartialEq)]
pub struct ExtrudeOptions {
    pub mode: ExtrudeMode,
    pub depth: f64,
    pub through: bool,
    pub current_material: bool,
}

impl Default for ExtrudeOptions {
    fn default() -> Self {
        Self { mode: ExtrudeMode::Carve, depth: 16.0, through: false, current_material: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointCheck {
    Ok,
    Duplicate,
    Crossing,
    Full,
    OffFace,
}

#[derive(Clone, Debug, PartialEq)]
struct Target {
    face: FaceRef,
    plane: Plane,
    frame: FaceFrame,
    polygon: Vec<DVec3>,
}

#[derive(Clone, Debug, PartialEq)]
struct Outline {
    target: Target,
    points: Vec<DVec3>,
    closed: bool,
}

impl Outline {
    fn flat(&self) -> Vec<DVec2> {
        self.points.iter().map(|p| self.target.frame.to_2d(*p)).collect()
    }

    fn center(&self) -> DVec3 {
        if self.points.is_empty() {
            return self.target.frame.origin;
        }
        self.points.iter().copied().sum::<DVec3>() / self.points.len() as f64
    }

    fn ready(&self) -> bool {
        self.points.len() >= 3 && outline_is_simple(&self.flat())
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Hover {
    view: usize,
    point: Option<DVec3>,
    check: PointCheck,
    face: Option<Target>,
    frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Point { view: usize, index: usize },
    Depth { view: usize, start: Pos2, plane: Option<DVec2>, from: f64 },
    Swallow,
}

#[derive(Debug, Default)]
struct Preview {
    key: Option<u64>,
    keep_edges: Vec<[DVec3; 2]>,
    prism_edges: Vec<[DVec3; 2]>,
    prism_tris: Vec<[DVec3; 3]>,
    brushes: usize,
    bounds: Aabb,
    error: Option<String>,
}

pub struct ExtrudeTool {
    pub options: ExtrudeOptions,
    outline: Option<Outline>,
    hover: Option<Hover>,
    drag: Option<Drag>,
    preview: RefCell<Preview>,
}

impl Default for ExtrudeTool {
    fn default() -> Self {
        Self::new()
    }
}

fn target_of(state: &AppState, face: FaceRef) -> Option<Target> {
    let node = state.doc.scene().solid(face.solid)?;
    let f = node.geom.face_for_side(face.side)?;
    let side = node.solid.sides.get(face.side)?;
    if side.disp.is_some() || !node.geom.valid {
        return None;
    }
    Some(Target { face, plane: f.plane, frame: FaceFrame::new(&f.plane), polygon: f.vertices.clone() })
}

fn pick_face(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<FaceRef> {
    let selected = state.doc.selected_solid_ids();
    if !selected.is_empty() {
        let only = |o: ObjRef| matches!(o, ObjRef::Solid(id) if selected.contains(&id));
        if view.is_2d() {
            let (origin, dir) = view.ray(pos);
            let opts = PickOptions { include_entities: false, filter: Some(&only), ..PickOptions::default() };
            if let Some(hit) = state.doc.pick_ray(&Ray::new(origin, dir), &opts, &|e| state.entity_bounds(e))
                && let (ObjRef::Solid(solid), Some(side)) = (hit.obj, hit.face)
            {
                return Some(FaceRef::new(solid, side));
            }
        } else if let Some(h) = pick_surface_with(state, view, pos, Some(&only)) {
            return Some(FaceRef::new(h.solid, h.side_index));
        }
    }
    pick_surface_with(state, view, pos, None).map(|h| FaceRef::new(h.solid, h.side_index))
}

fn convex_contains(poly: &[DVec2], p: DVec2) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let area: f64 = (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum();
    let sign = area.signum();
    (0..n).all(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        (b - a).perp_dot(p - a) * sign >= -1e-6 * (b - a).length()
    })
}

fn closest_on_boundary(poly: &[DVec2], p: DVec2) -> Option<DVec2> {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let d = b - a;
            let t =
                if d.length_squared() > 1e-12 { ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
            a + d * t
        })
        .min_by(|x, y| x.distance_squared(p).total_cmp(&y.distance_squared(p)))
}

fn pixel_size(view: &ViewInfo, at: DVec3) -> f64 {
    if view.is_2d() {
        return 1.0 / view.cam2d.zoom.max(1e-6);
    }
    let h = f64::from(view.rect.height().max(1.0));
    let half = (view.cam3d.fov_y_deg.to_radians() * 0.5).tan().max(1e-3);
    let depth = (at - view.cam3d.pos).dot(view.cam3d.forward()).max(view.cam3d.near.max(0.01));
    2.0 * half * depth / h
}

fn depth_delta(view: &ViewInfo, start: Pos2, pos: Pos2, center: DVec3, normal: DVec3) -> Option<f64> {
    let units = pixel_size(view, center);
    let k = units * 40.0;
    let (sc, sn) = (view.world_to_screen(center)?, view.world_to_screen(center + normal * k)?);
    let along = sn - sc;
    let moved = pos - start;
    if !(moved.x.is_finite() && moved.y.is_finite()) {
        return None;
    }
    let delta = if along.length() > 12.0 {
        f64::from(moved.dot(along.normalized())) * k / f64::from(along.length())
    } else {
        let eye = if view.is_2d() { center + view.kind.toward_viewer() } else { view.cam3d.pos };
        let facing = if normal.dot(eye - center) >= 0.0 { 1.0 } else { -1.0 };
        -f64::from(moved.y) * units * facing
    };
    delta.is_finite().then_some(delta)
}

fn issue_text(e: &GeomError) -> String {
    let key = match ExtrudeIssue::of(e) {
        Some(ExtrudeIssue::TooManyPoints) => "create.extrude.err_too_many",
        Some(ExtrudeIssue::TooFewPoints) => "create.extrude.err_too_few",
        Some(ExtrudeIssue::Crossing) => "create.extrude.crossing",
        Some(ExtrudeIssue::NotAFace) => "create.extrude.err_not_face",
        Some(ExtrudeIssue::Displacement) => "create.extrude.bad_face",
        Some(ExtrudeIssue::OutsideFace) => "create.extrude.err_outside",
        Some(ExtrudeIssue::NoDepth) => "create.extrude.err_depth",
        Some(ExtrudeIssue::TooThin) => "create.extrude.err_thin",
        Some(ExtrudeIssue::PastLimits) => "create.extrude.err_limits",
        Some(ExtrudeIssue::NoCut) => "create.extrude.err_no_cut",
        Some(ExtrudeIssue::ThroughBrush) => "create.extrude.err_through",
        Some(ExtrudeIssue::MixedDistances) => "create.extrude.err_mixed",
        Some(ExtrudeIssue::AwayFromDirection) => "create.extrude.err_away",
        Some(ExtrudeIssue::DisplacementShape) => "create.extrude.err_disp_shape",
        None => return e.to_string(),
    };
    t!(key).to_owned()
}

pub(crate) fn edit_issue_text(e: &EditError) -> String {
    match e {
        EditError::Geom(g) => issue_text(g),
        other => other.to_string(),
    }
}

pub(crate) fn face_extrude_ui(ui: &mut Ui, state: &mut AppState) {
    let tip = tf!(
        "create.extrude.face_extrude_tip",
        state.keys.shortcut(Action::ExtrudeInteractive).map(|s| shortcut_label(&s)).unwrap_or_default(),
        state.keys.shortcut(Action::ExtrudeAlongNormal).map(|s| shortcut_label(&s)).unwrap_or_default()
    );
    ui.label(t!("create.extrude.face_extrude")).on_hover_text(&tip);
    let mut changed = false;
    let brush = &mut state.settings.brush;
    for (dir, key) in [
        (ExtrudeDirectionSetting::Normal, "create.extrude.dir_normal"),
        (ExtrudeDirectionSetting::Average, "create.extrude.dir_average"),
        (ExtrudeDirectionSetting::Axis, "create.extrude.dir_axis"),
    ] {
        if ui
            .add(egui::Button::selectable(brush.extrude_mode == dir, t!(key)))
            .on_hover_text(t!("create.extrude.dir_tip"))
            .clicked()
        {
            brush.extrude_mode = dir;
            changed = true;
        }
    }
    if brush.extrude_mode == ExtrudeDirectionSetting::Axis {
        for axis in ExtrudeAxisSetting::ALL {
            let label = match axis {
                ExtrudeAxisSetting::Auto => t!("create.extrude.axis_auto").to_owned(),
                other => other.id().to_ascii_uppercase(),
            };
            if ui
                .add(egui::Button::selectable(brush.extrude_axis == axis, label))
                .on_hover_text(t!("create.extrude.axis_tip"))
                .clicked()
            {
                brush.extrude_axis = axis;
                changed = true;
            }
        }
    }
    ui.separator();
    for (grow, key) in [(false, "create.extrude.result_new"), (true, "create.extrude.result_grow")] {
        if ui
            .add(egui::Button::selectable(brush.extrude_grow == grow, t!(key)))
            .on_hover_text(t!("create.extrude.result_tip"))
            .clicked()
        {
            brush.extrude_grow = grow;
            changed = true;
        }
    }
    if changed {
        state.settings_dirty = true;
    }
    let has_faces = !state.doc.selected_faces().is_empty();
    if ui.add_enabled(has_faces, egui::Button::new(Action::ExtrudeAlongNormal.label())).on_hover_text(&tip).clicked() {
        state.request(Action::ExtrudeAlongNormal);
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

impl ExtrudeTool {
    pub fn new() -> Self {
        Self {
            options: ExtrudeOptions::default(),
            outline: None,
            hover: None,
            drag: None,
            preview: RefCell::new(Preview::default()),
        }
    }

    pub fn points(&self) -> &[DVec3] {
        self.outline.as_ref().map_or(&[], |o| &o.points)
    }

    pub fn face(&self) -> Option<FaceRef> {
        self.outline.as_ref().map(|o| o.target.face)
    }

    pub fn is_closed(&self) -> bool {
        self.outline.as_ref().is_some_and(|o| o.closed)
    }

    pub fn is_ready(&self) -> bool {
        self.outline.as_ref().is_some_and(Outline::ready)
    }

    pub fn params(&self, state: &AppState) -> FaceExtrudeParams {
        FaceExtrudeParams {
            mode: self.options.mode,
            depth: self.options.depth,
            through: self.options.through,
            material: self.options.current_material.then(|| active_material(state)),
        }
    }

    pub fn start(&mut self, state: &AppState, face: FaceRef) -> bool {
        match target_of(state, face) {
            Some(target) => {
                self.outline = Some(Outline { target, points: Vec::new(), closed: false });
                self.drag = None;
                true
            }
            None => false,
        }
    }

    fn check(&self, o: &Outline, p: DVec3) -> PointCheck {
        if !p.is_finite() || o.points.iter().any(|q| q.distance_squared(p) < 1e-9) {
            return PointCheck::Duplicate;
        }
        if o.points.len() >= MAX_OUTLINE_POINTS {
            return PointCheck::Full;
        }
        let mut flat = o.flat();
        flat.push(o.target.frame.to_2d(p));
        if path_is_simple(&flat) { PointCheck::Ok } else { PointCheck::Crossing }
    }

    pub fn add_point(&mut self, p: DVec3) -> PointCheck {
        let Some(o) = self.outline.as_ref() else { return PointCheck::OffFace };
        if o.closed {
            return PointCheck::Full;
        }
        let p = clean_vec(o.target.plane.project(p));
        let check = self.check(o, p);
        if check == PointCheck::Ok
            && let Some(o) = self.outline.as_mut()
        {
            o.points.push(p);
        }
        check
    }

    pub fn close(&mut self) -> bool {
        match self.outline.as_mut() {
            Some(o) if o.ready() => {
                o.closed = true;
                true
            }
            _ => false,
        }
    }

    pub fn remove_last_point(&mut self) -> bool {
        let Some(o) = self.outline.as_mut() else { return false };
        if o.closed {
            o.closed = false;
        } else {
            o.points.pop();
        }
        self.drag = None;
        true
    }

    pub fn set_depth(&mut self, signed: f64) -> bool {
        if !signed.is_finite() || signed.abs() < MIN_EXTRUDE_DEPTH {
            return false;
        }
        self.options.mode = if signed > 0.0 { ExtrudeMode::Extrude } else { ExtrudeMode::Carve };
        self.options.depth = signed.abs().min(MAX_EXTRUDE_DEPTH);
        self.options.through = false;
        true
    }

    pub fn toggle_mode(&mut self) {
        self.options.mode = match self.options.mode {
            ExtrudeMode::Carve => ExtrudeMode::Extrude,
            ExtrudeMode::Extrude => ExtrudeMode::Carve,
        };
    }

    pub fn clear(&mut self) -> bool {
        let had = self.outline.is_some() || self.drag.is_some();
        self.outline = None;
        self.drag = None;
        self.hover = None;
        had
    }

    pub fn apply(&mut self, state: &mut AppState) -> Option<Vec<Id>> {
        let o = self.outline.as_ref()?;
        if !o.ready() {
            state.notify(Severity::Warning, t!("create.extrude.not_ready"));
            return None;
        }
        let params = self.params(state);
        match state.doc.extrude_face(o.target.face, &o.points, &params) {
            Ok(ids) => {
                let key = match params.mode {
                    ExtrudeMode::Carve => "create.extrude.carved",
                    ExtrudeMode::Extrude => "create.extrude.extruded",
                };
                state.log(Severity::Info, tf!(key, ids.len()));
                self.clear();
                Some(ids)
            }
            Err(e) => {
                let text = match &e {
                    EditError::Geom(g) => issue_text(g),
                    other => other.to_string(),
                };
                state.notify(Severity::Error, tf!("create.extrude.failed", text));
                None
            }
        }
    }

    fn drop_stale(&mut self, state: &AppState) {
        let stale = self.outline.as_ref().is_some_and(|o| target_of(state, o.target.face).as_ref() != Some(&o.target));
        if stale {
            self.clear();
        }
    }

    fn point_from(
        view: &ViewInfo,
        pos: Pos2,
        snap: &Snap,
        plane: &Plane,
        face: &[DVec3],
        mods: Modifiers,
    ) -> Option<DVec3> {
        let (o, d) = if view.is_2d() {
            let p = snap.v2(view.screen_to_2d(pos));
            let toward = view.kind.toward_viewer();
            (view.kind.from_2d(p, 0.0) + toward * 65536.0, -toward)
        } else {
            view.ray(pos)
        };
        let denom = plane.normal.dot(d);
        if denom.abs() < 0.05 {
            return None;
        }
        let t = -plane.distance(o) / denom;
        if !t.is_finite() || (!view.is_2d() && t <= 0.0) {
            return None;
        }
        let hit = o + d * t;
        if view.is_2d() {
            let p = clean_vec(plane.project(hit));
            return p.is_finite().then_some(p);
        }
        if !mods.alt
            && let Some((m, _)) = magnet(view, pos, &snap_targets(face), MAGNET_PX)
        {
            let p = clean_vec(plane.project(m));
            return p.is_finite().then_some(p);
        }
        let mut p = plane.project(snap.on_plane(hit, plane.normal));
        let frame = FaceFrame::new(plane);
        let poly: Vec<DVec2> = face.iter().map(|v| frame.to_2d(*v)).collect();
        if convex_contains(&poly, frame.to_2d(hit))
            && !convex_contains(&poly, frame.to_2d(p))
            && let Some(edge) = closest_on_boundary(&poly, frame.to_2d(p))
        {
            p = frame.to_3d(edge, 0.0);
        }
        let p = clean_vec(plane.project(p));
        p.is_finite().then_some(p)
    }

    fn point_at(&self, view: &ViewInfo, pos: Pos2) -> Option<usize> {
        let o = self.outline.as_ref()?;
        o.points
            .iter()
            .enumerate()
            .filter_map(|(i, p)| view.world_to_screen(*p).map(|s| (i, s.distance(pos))))
            .filter(|(_, d)| *d <= HANDLE_TOLERANCE + 2.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn signed(&self) -> f64 {
        match self.options.mode {
            ExtrudeMode::Extrude => self.options.depth,
            ExtrudeMode::Carve => -self.options.depth,
        }
    }

    fn depth_from(&self, view: &ViewInfo, start: Pos2, pos: Pos2, from: f64, snap: &DistanceSnap) -> Option<f64> {
        let o = self.outline.as_ref()?;
        let n = o.target.plane.normal;
        let delta = depth_delta(view, start, pos, o.center(), n)?;
        Some(snap.distance(o.center(), n, from + delta))
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        self.drop_stale(state);
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::new(state, view, m.modifiers);
        if m.hovering() {
            self.hover = Some(self.hover_at(state, view, pos, &snap, m.modifiers));
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        if m.drag_started {
            self.drag = None;
            if let Some(index) = self.point_at(view, origin) {
                let closing = !self.is_closed() && index == 0 && self.points().len() >= 3;
                if !closing {
                    self.drag = Some(Drag::Point { view: view.index, index });
                }
            } else if self.is_closed() {
                let plane = view.is_2d().then(|| view.screen_to_2d(origin));
                self.drag = Some(Drag::Depth { view: view.index, start: origin, plane, from: self.signed() });
            }
        }
        if m.dragging || m.drag_stopped {
            match self.drag {
                Some(Drag::Point { view: vi, index }) if vi == view.index => {
                    self.move_point(view, pos, &snap, index, m.modifiers);
                }
                Some(Drag::Depth { view: vi, start, plane, from }) if vi == view.index => {
                    let start = plane.map_or(start, |p| view.screen_of_2d(p));
                    let depth_snap = DistanceSnap::for_extrude(state, view, m.modifiers);
                    if let Some(d) = self.depth_from(view, start, pos, from, &depth_snap) {
                        self.set_depth(d);
                    }
                }
                _ => {}
            }
        }
        if m.activated() && self.drag.is_none() {
            self.click(state, view, origin, &snap, m.double_clicked, m.modifiers);
            if m.drag_started {
                self.drag = Some(Drag::Swallow);
            }
        }
        if m.drag_stopped {
            self.drag = None;
        }
        ToolFlow::Handled
    }

    fn click(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, snap: &Snap, double: bool, mods: Modifiers) {
        if self.is_closed() {
            return;
        }
        if double {
            if self.close() {
                return;
            }
            if self.points().len() >= 3 {
                state.notify(Severity::Warning, t!("create.extrude.crossing"));
            }
            return;
        }
        if self.points().len() >= 3 && self.point_at(view, pos) == Some(0) {
            if !self.close() {
                state.notify(Severity::Warning, t!("create.extrude.crossing"));
            }
            return;
        }
        if self.outline.is_none() {
            let Some(face) = pick_face(state, view, pos) else { return };
            if !self.start(state, face) {
                state.notify(Severity::Warning, t!("create.extrude.bad_face"));
                return;
            }
        }
        let Some((plane, face)) = self.outline.as_ref().map(|o| (o.target.plane, o.target.polygon.clone())) else {
            return;
        };
        let Some(p) = Self::point_from(view, pos, snap, &plane, &face, mods) else {
            state.notify(Severity::Info, t!("create.extrude.edge_on"));
            return;
        };
        self.add_point(p);
        if self.points().is_empty() {
            self.outline = None;
        }
    }

    fn move_point(&mut self, view: &ViewInfo, pos: Pos2, snap: &Snap, index: usize, mods: Modifiers) {
        let Some(o) = self.outline.as_ref() else { return };
        let Some(p) = Self::point_from(view, pos, snap, &o.target.plane, &o.target.polygon, mods) else { return };
        let mut moved = o.points.clone();
        let Some(slot) = moved.get_mut(index) else { return };
        *slot = p;
        let flat: Vec<DVec2> = moved.iter().map(|q| o.target.frame.to_2d(*q)).collect();
        let ok = if o.closed { outline_is_simple(&flat) } else { path_is_simple(&flat) };
        if ok && let Some(o) = self.outline.as_mut() {
            o.points = moved;
        }
    }

    fn hover_at(&self, state: &AppState, view: &ViewInfo, pos: Pos2, snap: &Snap, mods: Modifiers) -> Hover {
        let frame = state.frame.index;
        match &self.outline {
            Some(o) => {
                let point = Self::point_from(view, pos, snap, &o.target.plane, &o.target.polygon, mods);
                let check = match point {
                    _ if o.closed => PointCheck::Full,
                    Some(p) => self.check(o, p),
                    None => PointCheck::OffFace,
                };
                Hover { view: view.index, point, check, face: None, frame }
            }
            None => {
                let face = pick_face(state, view, pos).and_then(|f| target_of(state, f));
                let point = face.as_ref().and_then(|t| Self::point_from(view, pos, snap, &t.plane, &t.polygon, mods));
                let check = if point.is_some() { PointCheck::Ok } else { PointCheck::OffFace };
                Hover { view: view.index, point, check, face, frame }
            }
        }
    }

    fn hover_in(&self, state: &AppState, view: &ViewInfo) -> Option<&Hover> {
        self.hover.as_ref().filter(|h| h.view == view.index && fresh(state, h.frame) && self.drag.is_none())
    }

    fn refresh_preview(&self, state: &AppState) {
        let Ok(mut guard) = self.preview.try_borrow_mut() else { return };
        let pv: &mut Preview = &mut guard;
        let Some(o) = self.outline.as_ref().filter(|o| o.ready()) else {
            if pv.key.is_some() {
                *pv = Preview::default();
            }
            return;
        };
        let params = self.params(state);
        let key = hash_f64s(
            [
                state.doc.revision() as f64,
                o.target.face.solid as f64,
                o.target.face.side as f64,
                params.depth,
                f64::from(u8::from(params.through)),
                f64::from(u8::from(params.mode == ExtrudeMode::Extrude)),
                params.material.as_deref().map_or(0.0, |m| m.len() as f64),
            ]
            .into_iter()
            .chain(o.points.iter().flat_map(|p| p.to_array())),
        );
        if pv.key == Some(key) {
            return;
        }
        *pv = Preview { key: Some(key), bounds: Aabb::EMPTY, ..Preview::default() };
        let Some(node) = state.doc.scene().solid(o.target.face.solid) else { return };
        let mut next = 1u32;
        let result = face_extrude(&node.solid.sides, o.target.face.side, &o.points, &params, &mut || {
            next = next.wrapping_add(1);
            next
        });
        match result {
            Ok(r) => {
                let changed = r.changes_brush();
                let keep: Vec<&Vec<hp_vmf::Side>> = if changed {
                    r.pieces.iter().map(|p| &p.sides).chain(r.added.iter()).collect()
                } else {
                    r.added.iter().collect()
                };
                pv.brushes = keep.len();
                for sides in keep {
                    let g = hp_geom::build_solid(sides);
                    pv.bounds = pv.bounds.union(&g.aabb);
                    pv.keep_edges.extend(g.edges());
                }
                for sides in &r.prism {
                    let g = hp_geom::build_solid(sides);
                    pv.bounds = pv.bounds.union(&g.aabb);
                    pv.prism_edges.extend(g.edges());
                    for f in &g.faces {
                        for [i, j, k] in f.triangles() {
                            if let (Some(&a), Some(&b), Some(&c)) =
                                (f.vertices.get(i), f.vertices.get(j), f.vertices.get(k))
                            {
                                pv.prism_tris.push([a, b, c]);
                            }
                        }
                    }
                }
            }
            Err(e) => pv.error = Some(issue_text(&e)),
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

    fn mode_keys(state: &AppState) -> String {
        let tab = key_label(Key::Tab);
        match state.keys.shortcut(Action::ToolExtrude) {
            Some(sc) => format!("{} / {tab}", shortcut_label(&sc)),
            None => tab,
        }
    }

    fn mode_ui(&mut self, ui: &mut Ui, state: &AppState) {
        let tip = tf!("create.extrude.mode_tip", Self::mode_keys(state));
        for (mode, key) in
            [(ExtrudeMode::Carve, "create.extrude.carve"), (ExtrudeMode::Extrude, "create.extrude.extrude")]
        {
            if ui.add(egui::Button::selectable(self.options.mode == mode, t!(key))).on_hover_text(&tip).clicked() {
                self.options.mode = mode;
            }
        }
    }
}

impl Tool for ExtrudeTool {
    fn id(&self) -> ToolId {
        ToolId::Extrude
    }

    fn reactivate(&mut self, _state: &mut AppState) {
        self.toggle_mode();
    }

    fn deactivate(&mut self, _state: &mut AppState) {
        self.clear();
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if ctx.memory(|m| m.focused()).is_none() {
            let tabs = consume_tabs(ctx);
            if !tabs.is_empty() {
                for _ in tabs {
                    self.toggle_mode();
                }
                return true;
            }
        }
        if self.is_ready() && consume_plain_key(ctx, Key::Enter) {
            self.apply(state);
            return true;
        }
        if self.outline.is_some() && consume_plain_key(ctx, Key::Backspace) {
            return self.remove_last_point();
        }
        if self.outline.is_some() && consume_plain_key(ctx, Key::Escape) {
            return self.clear();
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        self.refresh_preview(state);
        let mut ov = Ov::new(out, view);
        let line = rgba(COLOR_PENDING, 1.0);
        let Some(o) = &self.outline else {
            if let Some(h) = self.hover_in(state, view) {
                if let Some(t) = &h.face {
                    ov.polygon_outline(&t.polygon, rgba(COLOR_PENDING, 0.8), false);
                    if !view.is_2d() {
                        ov.polygon_fill(&t.polygon, rgba(COLOR_PENDING, 0.12));
                    }
                }
                if let Some(p) = h.point {
                    ov.point(p, line, 6.0);
                }
            }
            return;
        };
        if let Ok(pv) = self.preview.try_borrow() {
            let (keep, prism) = match self.options.mode {
                ExtrudeMode::Carve => (rgba(COLOR_FRONT, 1.0), rgba(COLOR_REMOVED, 1.0)),
                ExtrudeMode::Extrude => (rgba(COLOR_FRONT, 1.0), rgba(COLOR_PENDING, 1.0)),
            };
            for e in &pv.keep_edges {
                ov.xray_line(e[0], e[1], keep);
            }
            for e in &pv.prism_edges {
                ov.xray_line(e[0], e[1], prism);
            }
            if !view.is_2d() {
                let fill = [prism[0], prism[1], prism[2], 0.18];
                for t in &pv.prism_tris {
                    ov.tri(t[0], t[1], t[2], fill);
                }
            }
        }
        ov.polygon_outline(&o.target.polygon, rgba(COLOR_PENDING, 0.35), false);
        let n = o.points.len();
        for w in o.points.windows(2) {
            ov.xray_line(w[0], w[1], line);
        }
        if n >= 3 {
            let alpha = if o.closed { 1.0 } else { 0.45 };
            ov.xray_line(o.points[n - 1], o.points[0], rgba(COLOR_PENDING, alpha));
        }
        for p in &o.points {
            ov.point(*p, line, 7.0);
        }
        if let Some(h) = self.hover_in(state, view)
            && !o.closed
            && let Some(p) = h.point
        {
            let color = match h.check {
                PointCheck::Crossing | PointCheck::Full => rgba(COLOR_REMOVED, 1.0),
                _ => rgba(COLOR_PENDING, 0.7),
            };
            if let Some(last) = o.points.last() {
                ov.xray_line(*last, p, color);
                if n >= 2 && h.check == PointCheck::Ok {
                    ov.xray_line(p, o.points[0], rgba(COLOR_PENDING, 0.3));
                }
            }
            ov.point(p, color, if h.check == PointCheck::Ok { 6.0 } else { 9.0 });
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let Some(o) = &self.outline else { return };
        if let Some(h) = self.hover_in(state, view)
            && !o.closed
            && let Some(p) = h.point
            && let Some(s) = view.world_to_screen(p)
        {
            let at = s + Vec2::new(12.0, 12.0);
            match h.check {
                PointCheck::Crossing => {
                    paint_label(
                        &painter,
                        at,
                        Align2::LEFT_TOP,
                        t!("create.extrude.crossing"),
                        hp_app::theme::VIEW_ERROR,
                    );
                }
                PointCheck::Full => {
                    paint_label(&painter, at, Align2::LEFT_TOP, t!("create.extrude.full"), hp_app::theme::VIEW_ERROR);
                }
                PointCheck::Ok => {
                    if let Some(last) = o.points.last() {
                        paint_label(&painter, at, Align2::LEFT_TOP, &fmt_len(last.distance(p)), COLOR_PENDING);
                    }
                }
                PointCheck::Duplicate | PointCheck::OffFace => {}
            }
        }
        if !o.ready() {
            return;
        }
        let Some(s) = view.world_to_screen(o.center()) else { return };
        if !view.rect.contains(s) {
            return;
        }
        let error = self.preview.try_borrow().ok().and_then(|p| p.error.clone());
        match error {
            Some(e) => paint_label(&painter, s, Align2::CENTER_BOTTOM, &e, hp_app::theme::VIEW_ERROR),
            None => {
                let text = match (self.options.mode, self.options.through) {
                    (ExtrudeMode::Carve, true) => t!("create.extrude.through").to_owned(),
                    (ExtrudeMode::Carve, false) => tf!("create.extrude.label_carve", fmt_len(self.options.depth)),
                    (ExtrudeMode::Extrude, _) => tf!("create.extrude.label_extrude", fmt_len(self.options.depth)),
                };
                paint_label(&painter, s - Vec2::new(0.0, 8.0), Align2::CENTER_BOTTOM, &text, COLOR_PENDING);
            }
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            self.mode_ui(ui, state);
            ui.separator();
            let carve = self.options.mode == ExtrudeMode::Carve;
            let through = carve && self.options.through;
            ui.label(t!("create.extrude.depth")).on_hover_text(t!("create.extrude.depth_tip"));
            let drag = egui::DragValue::new(&mut self.options.depth)
                .speed(1.0)
                .range(MIN_EXTRUDE_DEPTH..=MAX_EXTRUDE_DEPTH)
                .max_decimals(3);
            ui.add_enabled(!through, drag).on_hover_text(t!("create.extrude.depth_tip"));
            if carve {
                let mut through = self.options.through;
                if ui
                    .checkbox(&mut through, t!("create.extrude.through"))
                    .on_hover_text(t!("create.extrude.through_tip"))
                    .changed()
                {
                    self.options.through = through;
                }
                if let Some(o) = &self.outline
                    && let Some(node) = state.doc.scene().solid(o.target.face.solid)
                    && let Some(th) = face_thickness(&node.solid.sides, o.target.face.side)
                {
                    ui.label(tf!("create.extrude.thickness", fmt_len(th)));
                }
            }
            ui.separator();
            ui.label(t!("create.extrude.new_faces")).on_hover_text(t!("create.extrude.new_faces_tip"));
            for (current, key) in [(false, "create.extrude.face_texture"), (true, "create.extrude.current_texture")] {
                if ui
                    .add(egui::Button::selectable(self.options.current_material == current, t!(key)))
                    .on_hover_text(t!("create.extrude.new_faces_tip"))
                    .clicked()
                {
                    self.options.current_material = current;
                }
            }
            if self.options.current_material {
                material_chip(ui, state);
            }
            ui.separator();
            let count = self.points().len();
            if ui
                .add_enabled(count > 0, egui::Button::new(t!("create.extrude.undo_point")))
                .on_hover_text(tf!("create.extrude.undo_point_tip", key_label(Key::Backspace)))
                .clicked()
            {
                self.remove_last_point();
            }
            let ready = self.is_ready() && self.preview_error(state).is_none();
            if ui
                .add_enabled(ready, egui::Button::new(t!("create.extrude.apply")))
                .on_hover_text(tf!("create.extrude.apply_tip", key_label(Key::Enter)))
                .clicked()
            {
                self.apply(state);
            }
            if ui.add_enabled(self.outline.is_some(), egui::Button::new(t!("common.clear"))).clicked() {
                self.clear();
            }
            let has_faces = !state.doc.selected_faces().is_empty();
            if ui
                .add_enabled(has_faces, egui::Button::new(Action::InsetFace.label()))
                .on_hover_text(t!("create.extrude.inset_tip"))
                .clicked()
            {
                state.request(Action::InsetFace);
            }
            if ui
                .add_enabled(has_faces, egui::Button::new(Action::InsetInteractive.label()))
                .on_hover_text(t!("create.extrude.inset_live_tip"))
                .clicked()
            {
                state.request(Action::InsetInteractive);
            }
            if ui
                .add_enabled(has_faces, egui::Button::new(Action::ExtrudeInteractive.label()))
                .on_hover_text(t!("create.extrude.extrude_live_tip"))
                .clicked()
            {
                state.request(Action::ExtrudeInteractive);
            }
        });
        ui.horizontal_wrapped(|ui| {
            face_extrude_ui(ui, state);
        });
        ui.horizontal_wrapped(|ui| {
            let count = self.points().len();
            if count > 0 {
                ui.label(tf!("create.extrude.count", count));
                if self.is_ready() {
                    match self.preview_error(state) {
                        Some(e) => {
                            ui.colored_label(ui.visuals().warn_fg_color, e);
                        }
                        None => {
                            ui.label(tf!("create.extrude.brushes", self.preview_brushes(state)));
                        }
                    }
                }
            }
        });
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        match self.drag {
            Some(Drag::Point { .. }) => return Some(CursorIcon::Grabbing),
            Some(Drag::Depth { .. }) => return Some(CursorIcon::ResizeVertical),
            _ => {}
        }
        if let Some(i) = self.point_at(view, pos) {
            let closing = !self.is_closed() && i == 0 && self.points().len() >= 3;
            return Some(if closing { CursorIcon::PointingHand } else { CursorIcon::Grab });
        }
        if self.is_closed() {
            return Some(CursorIcon::ResizeVertical);
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        let back = key_label(Key::Backspace);
        let escape = key_label(Key::Escape);
        let enter = key_label(Key::Enter);
        Some(match &self.outline {
            None => tf!("create.extrude.hint_idle", Self::mode_keys(state)),
            Some(o) if o.closed => tf!("create.extrude.hint_closed", enter, Self::mode_keys(state), escape),
            Some(o) if o.ready() => tf!("create.extrude.hint_ready", enter, back, escape),
            Some(o) => tf!("create.extrude.hint_points", 3usize.saturating_sub(o.points.len()).max(1), back, escape),
        })
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        self.clear()
    }

    fn set_option(&mut self, _state: &mut AppState, key: &str, value: &str) -> bool {
        let flag = match value.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        match key {
            "mode" => ExtrudeMode::from_id(value).map(|m| self.options.mode = m).is_some(),
            "depth" => value
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && (MIN_EXTRUDE_DEPTH..=MAX_EXTRUDE_DEPTH).contains(v))
                .map(|v| self.options.depth = v)
                .is_some(),
            "through" => flag.map(|v| self.options.through = v).is_some(),
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
        let depth = format!("{:.4}", o.depth);
        let depth = depth.trim_end_matches('0').trim_end_matches('.').to_owned();
        vec![
            ToolOption::new("mode", o.mode.id(), "carve (dig into the brush) or extrude (pull out of the face)"),
            ToolOption::new("depth", depth, "depth in units (0.125 to 16384)"),
            ToolOption::new("through", if o.through { "on" } else { "off" }, "on: carve the whole thickness"),
            ToolOption::new(
                "new_faces",
                if o.current_material { "current" } else { "face" },
                "texture of the new faces: face (the clicked face's) or current (the current material)",
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{info_2d, info_3d, settle};
    use hp_app::ViewKind;

    fn wall_view() -> ViewInfo {
        info_3d(DVec3::new(0.0, 0.0, 64.0), 90.0, 0.0)
    }

    fn click(tool: &mut ExtrudeTool, state: &mut AppState, view: &ViewInfo, p: DVec3) {
        let s = view.world_to_screen(p).expect("screen");
        tool.handle_mouse(state, view, &Mouse::click(s));
    }

    fn window_points() -> [DVec3; 4] {
        [
            DVec3::new(-63.0, 256.0, 33.0),
            DVec3::new(65.0, 256.0, 31.0),
            DVec3::new(63.0, 256.0, 97.0),
            DVec3::new(-65.0, 256.0, 95.0),
        ]
    }

    #[test]
    fn outline_a_window_in_3d_and_carve_it_through() {
        let mut state = settle();
        let mut tool = ExtrudeTool::new();
        let view = wall_view();
        for p in window_points() {
            click(&mut tool, &mut state, &view, p);
        }
        assert_eq!(tool.face(), Some(FaceRef::new(3, 5)));
        assert_eq!(
            tool.points(),
            [
                DVec3::new(-64.0, 256.0, 32.0),
                DVec3::new(64.0, 256.0, 32.0),
                DVec3::new(64.0, 256.0, 96.0),
                DVec3::new(-64.0, 256.0, 96.0)
            ],
            "snapped to the grid on the face"
        );
        assert!(tool.is_ready() && !tool.is_closed());
        click(&mut tool, &mut state, &view, DVec3::new(-64.0, 256.0, 32.0));
        assert!(tool.is_closed(), "clicking the first point closes the outline");
        tool.options.through = true;
        assert!(tool.preview_error(&state).is_none());
        assert_eq!(tool.preview_brushes(&state), 4);
        let (undo, _) = state.doc.history_len();
        let ids = tool.apply(&mut state).expect("carved");
        assert_eq!(ids.len(), 4);
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
        assert!(tool.points().is_empty());
        let vol: f64 = ids.iter().map(|id| state.doc.scene().solid(*id).expect("piece").geom.volume()).sum();
        assert!((vol - (512.0 * 16.0 * 128.0 - 128.0 * 16.0 * 64.0)).abs() < 1e-6);
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().solid(3).map(|n| n.geom.volume()), Some(512.0 * 16.0 * 128.0));
    }

    #[test]
    fn depth_drag_switches_between_carve_and_extrude() {
        let mut state = settle();
        let mut tool = ExtrudeTool::new();
        let view = wall_view();
        for p in window_points() {
            click(&mut tool, &mut state, &view, p);
        }
        let first = view.world_to_screen(DVec3::new(-64.0, 256.0, 32.0)).expect("first");
        tool.handle_mouse(&mut state, &view, &Mouse::double_click(first));
        assert!(tool.is_closed());
        let top = info_2d(ViewKind::Top, 1.0);
        let from = top.world_to_screen(DVec3::new(0.0, 256.0, 0.0)).expect("from");
        let out = top.world_to_screen(DVec3::new(0.0, 222.0, 0.0)).expect("out");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(from, from));
        tool.handle_mouse(&mut state, &top, &Mouse::drag(from, out));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(from, out));
        assert_eq!(tool.options.mode, ExtrudeMode::Extrude);
        assert_eq!(tool.options.depth, 16.0, "16 deep inward plus 34 outward, snapped to the grid");
        assert_eq!(tool.points().len(), 4, "a depth drag adds no point");
        let inside = top.world_to_screen(DVec3::new(0.0, 280.0, 0.0)).expect("in");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(from, from));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(from, inside));
        assert_eq!(tool.options.mode, ExtrudeMode::Carve);
        assert_eq!(tool.options.depth, 16.0);
        tool.set_depth(40.0);
        let ids = tool.apply(&mut state).expect("extruded");
        assert_eq!(ids.len(), 1);
        let b = state.doc.scene().solid(ids[0]).expect("new").bounds;
        assert_eq!(b, Aabb::new(DVec3::new(-64.0, 216.0, 32.0), DVec3::new(64.0, 256.0, 96.0)));
        assert!(!tool.set_depth(0.0));
        assert!(!tool.set_depth(f64::NAN));
    }

    #[test]
    fn concave_outline_in_a_2d_view_and_keys() {
        let mut state = settle();
        let mut tool = ExtrudeTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        let l = [(-128.0, -128.0), (128.0, -128.0), (128.0, 0.0), (0.0, 0.0), (0.0, 128.0), (-128.0, 128.0)];
        for (x, y) in l {
            click(&mut tool, &mut state, &top, DVec3::new(x + 3.0, y - 2.0, 0.0));
        }
        assert_eq!(tool.face(), Some(FaceRef::new(2, 0)), "the floor's top face");
        assert_eq!(tool.points().len(), 6);
        assert!(tool.points().iter().all(|p| p.z == 0.0 && p.x % 16.0 == 0.0));
        let crossing = top.world_to_screen(DVec3::new(64.0, -200.0, 0.0)).expect("cross");
        tool.handle_mouse(&mut state, &top, &Mouse::hover(crossing));
        assert_eq!(tool.hover.as_ref().map(|h| h.check), Some(PointCheck::Crossing));
        assert_eq!(tool.add_point(DVec3::new(64.0, -200.0, 0.0)), PointCheck::Crossing);
        assert_eq!(tool.add_point(DVec3::new(0.0, 0.0, 0.0)), PointCheck::Duplicate);
        tool.options.depth = 8.0;
        assert!(tool.preview_error(&state).is_none());
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let ctx = egui::Context::default();
        let input = egui::RawInput { events: vec![press(Key::Tab)], ..Default::default() };
        let mut handled = false;
        let mut out = ctx.run_ui(input, |ui| handled = tool.key_input(&mut state, ui.ctx()));
        out.textures_delta.clear();
        assert!(handled);
        assert_eq!(tool.options.mode, ExtrudeMode::Extrude);
        tool.toggle_mode();
        let input = egui::RawInput { events: vec![press(Key::Enter)], ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| handled = tool.key_input(&mut state, ui.ctx()));
        out.textures_delta.clear();
        assert!(handled && tool.points().is_empty(), "Enter applies");
        let vol: f64 = state
            .doc
            .selected_solid_ids()
            .iter()
            .map(|id| state.doc.scene().solid(*id).expect("s").geom.volume())
            .sum();
        let removed = (256.0 * 128.0 + 128.0 * 128.0) * 8.0;
        assert!((vol - (512.0 * 512.0 * 16.0 - removed)).abs() < 1e-6, "{vol}");
        click(&mut tool, &mut state, &top, DVec3::new(-200.0, -200.0, 0.0));
        assert_eq!(tool.points().len(), 1);
        let input = egui::RawInput { events: vec![press(Key::Escape)], ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| handled = tool.key_input(&mut state, ui.ctx()));
        out.textures_delta.clear();
        assert!(handled && tool.face().is_none(), "Escape cancels");
    }

    fn add_box(state: &mut AppState, min: DVec3, max: DVec3) -> u32 {
        let pts: Vec<DVec3> = (0..8)
            .map(|i| {
                DVec3::new(
                    if i & 1 == 0 { min.x } else { max.x },
                    if i & 2 == 0 { min.y } else { max.y },
                    if i & 4 == 0 { min.z } else { max.z },
                )
            })
            .collect();
        let mut next = 70_000u32;
        let mut ids = || {
            next += 1;
            next
        };
        let sides = hp_geom::hull_solid(&pts, "DEV/DEV_MEASUREGENERIC01B", 0.25, 16, &mut ids).expect("box");
        state.doc.create_solid(sides, hp_editor::Owner::World).expect("solid")
    }

    #[test]
    fn depth_drags_are_relative_and_work_facing_the_face() {
        let mut state = settle();
        let mut tool = ExtrudeTool::new();
        let view = wall_view();
        for p in window_points() {
            click(&mut tool, &mut state, &view, p);
        }
        let first = view.world_to_screen(DVec3::new(-64.0, 256.0, 32.0)).expect("first");
        tool.handle_mouse(&mut state, &view, &Mouse::double_click(first));
        assert!(tool.is_closed());
        let far = Pos2::new(700.0, 80.0);
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(far, far));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(far, far));
        assert_eq!((tool.options.mode, tool.options.depth), (ExtrudeMode::Carve, 16.0), "no jump when the drag starts");
        let up = far - Vec2::new(0.0, 60.0);
        tool.handle_mouse(&mut state, &view, &Mouse::drag(far, up));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(far, up));
        assert_eq!(tool.options.mode, ExtrudeMode::Extrude, "dragging up pulls the face toward the camera");
        assert!(tool.options.depth > 0.0 && tool.options.depth < 512.0, "{}", tool.options.depth);
        let depth = tool.options.depth;
        let down = far + Vec2::new(0.0, 600.0);
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(far, far));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(far, down));
        assert_eq!(tool.options.mode, ExtrudeMode::Carve);
        assert!(tool.options.depth > depth, "a long drag down digs deeper: {}", tool.options.depth);
    }

    #[test]
    fn a_depth_drag_in_2d_follows_the_world_while_the_view_scrolls() {
        let closed = || {
            let mut state = settle();
            let mut tool = ExtrudeTool::new();
            let view = wall_view();
            for p in window_points() {
                click(&mut tool, &mut state, &view, p);
            }
            let first = view.world_to_screen(DVec3::new(-64.0, 256.0, 32.0)).expect("first");
            tool.handle_mouse(&mut state, &view, &Mouse::double_click(first));
            assert!(tool.is_closed());
            (state, tool)
        };
        let top = info_2d(ViewKind::Top, 1.0);
        let start = top.world_to_screen(DVec3::new(300.0, 200.0, 0.0)).expect("start");
        let mut scrolled = top;
        scrolled.cam2d.center.y -= 96.0;
        let (mut state, mut tool) = closed();
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(start, start));
        tool.handle_mouse(&mut state, &top, &Mouse::drag(start, start));
        tool.handle_mouse(&mut state, &scrolled, &Mouse::drag(start, start));
        tool.handle_mouse(&mut state, &scrolled, &Mouse::drag_end(start, start));
        let by_scroll = (tool.options.mode, tool.options.depth);
        let (mut state, mut tool) = closed();
        let same_world = top.screen_of_2d(scrolled.screen_to_2d(start));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(start, start));
        tool.handle_mouse(&mut state, &top, &Mouse::drag(start, same_world));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(start, same_world));
        assert_eq!(by_scroll, (tool.options.mode, tool.options.depth), "scrolling equals moving the mouse");
        assert_ne!(by_scroll, (ExtrudeMode::Carve, 16.0), "the scroll changed the depth");
    }

    #[test]
    fn the_3d_view_prefers_the_selected_brush_behind_another() {
        let mut state = settle();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        let mut tool = ExtrudeTool::new();
        let view = info_3d(DVec3::new(96.0, -200.0, 32.0), 90.0, 0.0);
        click(&mut tool, &mut state, &view, DVec3::new(96.0, 0.0, 32.0));
        assert_eq!(tool.face().map(|f| f.solid), Some(3), "the selected wall, not the trigger in front");
        assert!((tool.points()[0].y - 256.0).abs() < 1e-9);
        tool.clear();
        state.doc.clear_selection();
        click(&mut tool, &mut state, &view, DVec3::new(96.0, 0.0, 32.0));
        assert_eq!(tool.face().map(|f| f.solid), Some(7), "nothing selected: the closest face");
    }

    #[test]
    fn points_snap_to_corners_and_stay_on_the_face_near_off_grid_edges() {
        let mut state = settle();
        let id = add_box(&mut state, DVec3::new(3.0, -100.0, 5.0), DVec3::new(203.0, -84.0, 205.0));
        let view = info_3d(DVec3::new(103.0, -500.0, 105.0), 90.0, 0.0);
        let mut tool = ExtrudeTool::new();
        click(&mut tool, &mut state, &view, DVec3::new(7.0, -100.0, 60.0));
        assert_eq!(tool.face().map(|f| f.solid), Some(id));
        assert_eq!(tool.points(), [DVec3::new(3.0, -100.0, 64.0)], "grid snap kept on the face edge");
        click(&mut tool, &mut state, &view, DVec3::new(202.0, -100.0, 204.0));
        assert_eq!(tool.points()[1], DVec3::new(203.0, -100.0, 205.0), "snapped to the corner");
        let alt = Modifiers { alt: true, ..Modifiers::NONE };
        let s = view.world_to_screen(DVec3::new(120.3, -100.0, 30.7)).expect("screen");
        tool.handle_mouse(&mut state, &view, &Mouse::click(s).with_modifiers(alt));
        let free = tool.points()[2];
        assert!((free - DVec3::new(120.3, -100.0, 30.7)).length() < 0.6, "no snapping with Alt: {free:?}");
    }

    #[test]
    fn a_2d_view_prefers_the_selected_brush() {
        let mut state = settle();
        let params = hp_geom::PrimitiveParams {
            kind: hp_geom::Primitive::Block,
            bounds: Aabb::new(DVec3::new(300.0, 200.0, 0.0), DVec3::new(316.0, 300.0, 128.0)),
            ..Default::default()
        };
        let blocker = state.doc.create_primitive(&params, hp_editor::Owner::World).expect("blocker")[0];
        let front = info_2d(ViewKind::Front, 1.0);
        let at = front.world_to_screen(DVec3::new(0.0, 260.0, 64.0)).expect("at");
        state.doc.clear_selection();
        let mut tool = ExtrudeTool::new();
        tool.handle_mouse(&mut state, &front, &Mouse::click(at));
        assert_eq!(tool.face().map(|f| f.solid), Some(blocker), "the first brush along the view");
        tool.clear();
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        tool.handle_mouse(&mut state, &front, &Mouse::click(at));
        assert_eq!(tool.face(), Some(FaceRef::new(3, 3)), "the selected brush behind it");
        assert_eq!(tool.points(), [DVec3::new(256.0, 256.0, 64.0)], "on the face plane, on the grid");
        let top = info_2d(ViewKind::Top, 1.0);
        let edge = top.world_to_screen(DVec3::new(256.0, 300.0, 0.0)).expect("edge");
        tool.handle_mouse(&mut state, &top, &Mouse::click(edge));
        assert_eq!(tool.points().len(), 1, "a view along the face places no point");
    }

    #[test]
    fn options_overlay_and_paint() {
        let mut state = settle();
        let mut tool = ExtrudeTool::new();
        let view = wall_view();
        tool.handle_mouse(&mut state, &view, &Mouse::hover(view.rect.center()));
        let mut out = Overlay::default();
        tool.overlay(&state, &view, &mut out);
        assert!(!out.lines.is_empty(), "the hovered face is outlined");
        for p in window_points().iter().take(3) {
            click(&mut tool, &mut state, &view, *p);
        }
        let hover = view.world_to_screen(DVec3::new(-64.0, 256.0, 100.0)).expect("hover");
        tool.handle_mouse(&mut state, &view, &Mouse::hover(hover));
        let views = [view, info_2d(ViewKind::Top, 1.0), info_2d(ViewKind::Front, 1.0), info_2d(ViewKind::Side, 1.0)];
        for v in &views {
            let mut out = Overlay::default();
            tool.overlay(&state, v, &mut out);
            assert!(!out.lines.is_empty() || !out.lines_on_top.is_empty(), "{:?}", v.kind);
        }
        hp_app::testing::run_ctx(&mut state, |ctx, state| {
            for v in &views {
                tool.paint(state, v, &ctx.layer_painter(egui::LayerId::background()));
            }
        });
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        for o in tool.option_values() {
            assert!(tool.set_option(&mut state, o.key, &o.value), "{} {}", o.key, o.value);
        }
        assert!(tool.set_option(&mut state, "mode", "extrude"));
        assert!(tool.set_option(&mut state, "new_faces", "current"));
        assert!(tool.set_option(&mut state, "depth", "24"));
        assert!(!tool.set_option(&mut state, "depth", "0"));
        assert!(!tool.set_option(&mut state, "mode", "cut"));
        assert_eq!(tool.params(&state).material.as_deref(), Some("dev/dev_measuregeneric01b"));
        assert_eq!(tool.params(&state).depth, 24.0);
        assert!(tool.cancel(&mut state));
        assert!(!tool.cancel(&mut state));
        for issue in ExtrudeIssue::ALL {
            let text = issue_text(&issue.into());
            assert!(!text.is_empty() && text != issue.message(), "{issue:?} is translated");
        }
        assert_eq!(issue_text(&GeomError::NoVolume), GeomError::NoVolume.to_string());
        let font = egui::FontId::proportional(14.0);
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, ToolId::Extrude.icon())), "the tool icon has a glyph");
    }
}
