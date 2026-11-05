use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use egui::{Align2, CursorIcon, Key, Pos2, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::{Action, key_label};
use hp_app::state::AppState;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::CurveRecord;
use hp_geom::Aabb;
use hp_render::Overlay;
use hp_tools_select::gizmo::GizmoMode;

mod handles;
#[cfg(test)]
mod history_tests;
mod user_presets;

use super::apply::{
    ApplyError, RoadCreated, build_in_doc, create_road, ground_height, list_roads, read_road, replace_road,
    road_piece_ids, split_road,
};
use super::build::{Road, RoadError, elevations};
use super::model::{
    Attach, Junction, JunctionEnd, LineKind, MAX_DASH, MAX_FLARE, MAX_LANE_WIDTH, MAX_LANES, MAX_LIFT, MAX_LINE_WIDTH,
    MAX_POINTS, MAX_RADIUS, MAX_SHOULDER, MAX_STEP, MAX_THICKNESS, MIN_DASH, MIN_LANE_WIDTH, MIN_LINE_WIDTH, MIN_STEP,
    MIN_THICKNESS, RoadLayout, RoadMeta, RoadPoint,
};
use crate::block::{CreateAs, create_as_ui};
use crate::common::{
    COLOR_PENDING, COLOR_REMOVED, COLOR_SHAPE, HANDLE_TOLERANCE, Mouse, Ov, Snap, active_material, consume_plain_key,
    fgd_defaults, fmt_len, fmt_size, fresh, material_chip, paint_label, pick_surface, rgba,
};
use crate::curve::geom::nearest_on_spline;
use crate::curve::section::{MAX_AUTO_GAIN, MAX_AUTO_LIMIT, MAX_ROLL};
use crate::picker::ClassPicker;

const DEFAULT_FLARE: f64 = 48.0;
const POINT_TOLERANCE: f32 = HANDLE_TOLERANCE + 4.0;
const MAX_BASE_Z: f64 = 16384.0;

#[derive(Clone, Debug, PartialEq)]
pub struct RoadOptions {
    pub meta: RoadMeta,
    pub base_z: f64,
    pub create_as: CreateAs,
    pub keep_active: bool,
}

impl Default for RoadOptions {
    fn default() -> Self {
        Self { meta: RoadMeta::default(), base_z: 0.0, create_as: CreateAs::World, keep_active: true }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Carry {
    uid: u32,
    u0: f64,
    start_dir: Option<DVec2>,
    end_dir: Option<DVec2>,
    start_slope: Option<f64>,
    end_slope: Option<f64>,
    junction: Option<Junction>,
}

struct Editing {
    record: CurveRecord,
    saved: Box<RoadOptions>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hover {
    pos: DVec3,
    kind: ViewKind,
    frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Point(usize),
    Gizmo(handles::PointDrag),
    Swallow,
}

type Cached = (u64, u64, Rc<Result<Road, RoadError>>);

#[derive(Clone)]
struct Snapshot {
    points: Vec<RoadPoint>,
    selected: Option<usize>,
    options: RoadOptions,
    carry: Carry,
    follow_material: bool,
    preset_name: String,
}

pub struct RoadTool {
    pub options: RoadOptions,
    points: Vec<RoadPoint>,
    carry: Carry,
    editing: Option<Editing>,
    selected: Option<usize>,
    drag: Option<Drag>,
    hover: Option<Hover>,
    version: u64,
    cache: RefCell<Option<Cached>>,
    visual_key: Option<(u64, u64)>,
    road_ids: RefCell<Option<(u64, Rc<HashSet<hp_editor::Id>>)>>,
    roads: Vec<(u32, String)>,
    roads_rev: u64,
    show_window: bool,
    join_target: Option<u32>,
    join_end: JunctionEnd,
    join_flare: f64,
    join_attach: Attach,
    initialized: bool,
    follow_material: bool,
    last_bounds: Option<Aabb>,
    picker: ClassPicker,
    baseline: Option<Snapshot>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    history_group: bool,
    history_active: bool,
    preset_name: String,
}

impl Default for RoadTool {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn error_text(e: &RoadError) -> String {
    use super::build::JunctionProblem as J;
    match e {
        RoadError::LayoutPoints => t!("road.err.layout_points").to_owned(),
        RoadError::LayoutSize => t!("road.err.layout_size").to_owned(),
        RoadError::LayoutOptions => t!("road.err.layout_options").to_owned(),
        RoadError::TooFewPoints => t!("road.err.points").to_owned(),
        RoadError::OutOfRange => t!("road.err.range").to_owned(),
        RoadError::TooShort(l) => tf!("road.err.short", fmt_len(*l)),
        RoadError::Folded(n) => tf!("road.err.folded", n),
        RoadError::LinesTooWide => t!("road.err.lines").to_owned(),
        RoadError::ShoulderTooSmall => t!("road.err.shoulder").to_owned(),
        RoadError::TooWide => t!("road.err.wide").to_owned(),
        RoadError::TooManyPieces(n) => tf!("road.err.many", n),
        RoadError::Junction(J::MissingTarget) => t!("road.err.j_missing").to_owned(),
        RoadError::Junction(J::SelfTarget) => t!("road.err.j_self").to_owned(),
        RoadError::Junction(J::Parallel) => t!("road.err.j_parallel").to_owned(),
        RoadError::Junction(J::TooFar) => t!("road.err.j_far").to_owned(),
        RoadError::Junction(J::TooClose) => t!("road.err.j_close").to_owned(),
        RoadError::Junction(J::Shallow) => t!("road.err.j_shallow").to_owned(),
        RoadError::Junction(J::Preset) => t!("road.err.j_preset").to_owned(),
    }
}

fn apply_error_text(e: &ApplyError) -> String {
    match e {
        ApplyError::Build(b) => error_text(b),
        other => tf!("road.failed", other),
    }
}

impl RoadTool {
    fn layout_ui(ui: &mut Ui, meta: &mut RoadMeta) {
        let label = |layout: RoadLayout| match layout {
            RoadLayout::Path => t!("road.layout_path"),
            RoadLayout::Tee => t!("road.layout_tee"),
            RoadLayout::Cross => t!("road.layout_cross"),
            RoadLayout::Wye => t!("road.layout_wye"),
            RoadLayout::CulDeSac => t!("road.layout_cul_de_sac"),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("road.layout"));
            egui::ComboBox::from_id_salt(ui.scope_id().with("road_layout"))
                .selected_text(label(meta.layout))
                .show_ui(ui, |ui| {
                    for layout in RoadLayout::ALL {
                        ui.selectable_value(&mut meta.layout, layout, label(layout));
                    }
                })
                .response
                .on_hover_text(t!("road.layout_tip"));
            if meta.layout != RoadLayout::Path {
                ui.label(t!("road.radius"));
                Self::number(ui, &mut meta.radius, 0.0..=MAX_RADIUS, t!("road.radius_tip"));
            }
        });
    }

    pub fn new() -> Self {
        Self {
            options: RoadOptions::default(),
            points: Vec::new(),
            carry: Carry::default(),
            editing: None,
            selected: None,
            drag: None,
            hover: None,
            version: 0,
            cache: RefCell::new(None),
            visual_key: None,
            road_ids: RefCell::new(None),
            roads: Vec::new(),
            roads_rev: u64::MAX,
            show_window: false,
            join_target: None,
            join_end: JunctionEnd::End,
            join_flare: DEFAULT_FLARE,
            join_attach: Attach::Outer,
            initialized: false,
            follow_material: true,
            last_bounds: None,
            picker: ClassPicker::default(),
            baseline: Some(Snapshot {
                points: Vec::new(),
                selected: None,
                options: RoadOptions::default(),
                carry: Carry::default(),
                follow_material: true,
                preset_name: String::new(),
            }),
            undo: Vec::new(),
            redo: Vec::new(),
            history_group: false,
            history_active: false,
            preset_name: String::new(),
        }
    }

    pub fn points(&self) -> &[RoadPoint] {
        &self.points
    }

    pub fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    pub fn is_ready(&self) -> bool {
        self.points.len() >= 2
    }

    fn bump(&mut self) {
        self.version = self.version.wrapping_add(1);
        if !self.history_group {
            self.record_history();
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            points: self.points.clone(),
            selected: self.selected,
            options: self.options.clone(),
            carry: self.carry.clone(),
            follow_material: self.follow_material,
            preset_name: self.preset_name.clone(),
        }
    }

    fn record_history(&mut self) {
        let next = self.snapshot();
        if let Some(previous) = self.baseline.take()
            && (previous.points != next.points
                || previous.options != next.options
                || previous.carry != next.carry
                || previous.follow_material != next.follow_material)
            && (!previous.points.is_empty() || !next.points.is_empty() || self.history_active)
        {
            if self.undo.len() >= 128 {
                self.undo.remove(0);
            }
            self.undo.push(previous);
            self.redo.clear();
            self.history_active = true;
        }
        self.baseline = Some(next);
    }

    fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.history_group = false;
        self.history_active = self.editing.is_some();
        self.baseline = Some(self.snapshot());
    }

    fn sync_history(&self, state: &mut AppState) {
        state.runtime.brush.road_history =
            self.history_active.then_some((!self.undo.is_empty(), !self.redo.is_empty()));
    }

    fn restore_history(&mut self, redo: bool) {
        self.history_group = false;
        self.record_history();
        let previous = if redo { self.redo.pop() } else { self.undo.pop() };
        let Some(previous) = previous else { return };
        let current = self.snapshot();
        if redo {
            self.undo.push(current)
        } else {
            self.redo.push(current)
        }
        self.points = previous.points;
        self.selected = previous.selected.filter(|i| *i < self.points.len());
        self.options = previous.options;
        self.carry = previous.carry;
        self.follow_material = previous.follow_material;
        self.preset_name = previous.preset_name;
        self.drag = None;
        self.hover = None;
        self.baseline = Some(self.snapshot());
        self.version = self.version.wrapping_add(1);
    }

    fn sync_preview(&mut self, state: &mut AppState) {
        self.sync_history(state);
        let key = (state.doc.revision(), self.version);
        if self.visual_key == Some(key) {
            return;
        }
        self.visual_key = Some(key);
        match &*self.preview(state) {
            Ok(road) => {
                let mesh = super::preview::mesh(road, |name| state.assets.render_material_id(name));
                state.assets.set_tool_preview(Some(Arc::new(mesh)), &self.own_ids());
            }
            Err(_) => state.assets.set_tool_preview(None, &[]),
        }
    }

    fn ensure_init(&mut self, state: &AppState) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        let s = state.settings.edit.default_texture_scale;
        if s.is_finite() && s.abs() > 1e-6 {
            self.options.meta.tex_scale = s;
        }
        self.options.meta.material = active_material(state);
        if self.points.is_empty() && !self.history_active {
            self.baseline = Some(self.snapshot());
        }
    }

    fn sync_material(&mut self, state: &AppState) {
        if self.follow_material && self.editing.is_none() {
            let current = active_material(state);
            if self.options.meta.material != current {
                self.options.meta.material = current;
                self.bump();
            }
        }
    }

    pub fn meta(&self) -> RoadMeta {
        let mut m = self.options.meta.clone();
        m.points = self.points.clone();
        m.uid = self.carry.uid;
        m.u0 = self.carry.u0;
        m.start_dir = self.carry.start_dir;
        m.end_dir = self.carry.end_dir;
        m.start_slope = self.carry.start_slope;
        m.end_slope = self.carry.end_slope;
        m.junction = self.carry.junction;
        m.clamped()
    }

    fn own_ids(&self) -> Vec<hp_editor::Id> {
        self.editing.as_ref().map(|e| e.record.ids.clone()).unwrap_or_default()
    }

    pub fn preview(&self, state: &AppState) -> Rc<Result<Road, RoadError>> {
        let key = (state.doc.revision(), self.version);
        if let Some((r, v, built)) = self.cache.borrow().as_ref()
            && (*r, *v) == key
        {
            return built.clone();
        }
        let built = Rc::new(if self.points.len() < 2 {
            Err(RoadError::TooFewPoints)
        } else {
            build_in_doc(&state.doc, &self.meta(), &self.own_ids())
        });
        *self.cache.borrow_mut() = Some((key.0, key.1, built.clone()));
        built
    }

    fn take_request(&mut self, state: &mut AppState) {
        let Some(piece) = state.runtime.brush.road_edit.take() else { return };
        self.ensure_init(state);
        let Some((record, meta)) = read_road(&state.doc, piece) else {
            state.notify(Severity::Warning, t!("road.edit_missing").to_owned());
            return;
        };
        self.finish_edit();
        let saved = Box::new(self.options.clone());
        self.points = meta.points.clone();
        self.carry = Carry {
            uid: meta.uid,
            u0: meta.u0,
            start_dir: meta.start_dir,
            end_dir: meta.end_dir,
            start_slope: meta.start_slope,
            end_slope: meta.end_slope,
            junction: meta.junction,
        };
        if let Some(j) = meta.junction {
            self.join_target = Some(j.target);
            self.join_end = j.end;
            self.join_flare = j.flare;
            self.join_attach = j.attach;
        }
        self.options.meta = RoadMeta { points: Vec::new(), junction: None, ..meta };
        self.match_user_preset(state);
        self.selected = None;
        self.drag = None;
        self.hover = None;
        self.show_window = true;
        self.roads_rev = u64::MAX;
        self.editing = Some(Editing { record, saved });
        self.bump();
        self.reset_history();
    }

    fn finish_edit(&mut self) {
        if let Some(e) = self.editing.take() {
            self.options = *e.saved;
        }
        self.carry = Carry::default();
    }

    fn clear(&mut self) {
        self.finish_edit();
        self.points.clear();
        self.selected = None;
        self.drag = None;
        self.hover = None;
        self.bump();
        self.reset_history();
    }

    fn last_z(&self) -> f64 {
        self.points.last().map_or(self.options.base_z, |p| p.z)
    }

    fn road_ids(&self, state: &AppState) -> Rc<HashSet<hp_editor::Id>> {
        let rev = state.doc.revision();
        if let Some((r, ids)) = self.road_ids.borrow().as_ref()
            && *r == rev
        {
            return ids.clone();
        }
        let ids = Rc::new(road_piece_ids(&state.doc));
        *self.road_ids.borrow_mut() = Some((rev, ids.clone()));
        ids
    }

    fn auto_z(&self, state: &AppState, xy: DVec2, fallback: f64) -> f64 {
        if self.options.meta.follow_terrain {
            let skip = self.road_ids(state);
            if let Some(g) = ground_height(&state.doc, &skip, xy, fallback) {
                return g + self.options.meta.lift;
            }
        }
        fallback
    }

    pub fn add_point(&mut self, p: RoadPoint) -> bool {
        if !p.is_valid()
            || self.points.len() >= MAX_POINTS
            || (self.options.meta.layout != RoadLayout::Path && self.points.len() >= 2)
        {
            return false;
        }
        if self.points.last().is_some_and(|q| q.xyz().distance_squared(p.xyz()) < 1e-12) {
            return false;
        }
        self.points.push(p);
        self.selected = Some(self.points.len() - 1);
        self.bump();
        true
    }

    pub fn remove_point_at(&mut self, index: usize) -> bool {
        let floor = if self.editing.is_some() { 2 } else { 0 };
        if index >= self.points.len() || self.points.len() <= floor {
            return false;
        }
        self.points.remove(index);
        self.selected = None;
        self.drag = None;
        self.bump();
        true
    }

    pub fn insert_point_near(&mut self, xy: DVec2) -> bool {
        if self.options.meta.layout != RoadLayout::Path
            || self.points.len() < 2
            || self.points.len() >= MAX_POINTS
            || !xy.is_finite()
        {
            return false;
        }
        let list: Vec<DVec2> = self.points.iter().map(|p| p.xy()).collect();
        let Some((index, on, _)) = nearest_on_spline(&list, xy) else { return false };
        let z = match elevations(&self.meta()) {
            Ok(e) => e.dense.nearest(xy).map_or(self.last_z(), |n| e.elevation.at(n.0)),
            Err(_) => self.last_z(),
        };
        let at = index.min(self.points.len());
        self.points.insert(at, RoadPoint::new(on.x, on.y, z));
        self.selected = Some(at);
        self.drag = None;
        self.bump();
        true
    }

    pub fn reverse(&mut self) {
        self.points.reverse();
        std::mem::swap(&mut self.carry.start_dir, &mut self.carry.end_dir);
        self.carry.start_dir = self.carry.start_dir.map(|d| -d);
        self.carry.end_dir = self.carry.end_dir.map(|d| -d);
        std::mem::swap(&mut self.carry.start_slope, &mut self.carry.end_slope);
        self.carry.start_slope = self.carry.start_slope.map(|s| -s);
        self.carry.end_slope = self.carry.end_slope.map(|s| -s);
        if let Some(j) = &mut self.carry.junction {
            j.end = match j.end {
                JunctionEnd::Start => JunctionEnd::End,
                JunctionEnd::End => JunctionEnd::Start,
            };
        }
        let b = &mut self.options.meta.bank;
        std::mem::swap(&mut b.roll_start, &mut b.roll_end);
        b.roll_start = -b.roll_start;
        b.roll_end = -b.roll_end;
        self.selected = self.selected.map(|i| self.points.len().saturating_sub(1).saturating_sub(i));
        self.bump();
    }

    pub fn set_point_height(&mut self, index: usize, z: f64) -> bool {
        match self.points.get_mut(index) {
            Some(p) if z.is_finite() && z.abs() <= MAX_BASE_Z => {
                p.z = z;
                self.options.meta.follow_terrain = false;
                self.bump();
                true
            }
            _ => false,
        }
    }

    pub fn fit_width_to_bends(&mut self, state: &AppState) -> bool {
        if !matches!(&*self.preview(state), Err(RoadError::Folded(_))) {
            return false;
        }
        let original = self.meta();
        let scaled = |factor: f64| {
            let mut m = original.clone();
            m.lane_width = (original.lane_width * factor).max(MIN_LANE_WIDTH);
            m.shoulder = if original.shoulder == 0.0 { 0.0 } else { (original.shoulder * factor).max(1.0) };
            m.line_width = (original.line_width * factor).max(MIN_LINE_WIDTH);
            m
        };
        let own = self.own_ids();
        let valid = |factor| build_in_doc(&state.doc, &scaled(factor), &own).is_ok();
        let mut lo = MIN_LANE_WIDTH / original.lane_width;
        if !valid(lo) {
            return false;
        }
        let mut hi = 1.0;
        for _ in 0..18 {
            let mid = (lo + hi) * 0.5;
            if valid(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let fitted = scaled(lo);
        self.options.meta.lane_width = fitted.lane_width;
        self.options.meta.shoulder = fitted.shoulder;
        self.options.meta.line_width = fitted.line_width;
        self.bump();
        true
    }

    fn point_at(&self, view: &ViewInfo, pos: Pos2) -> Option<usize> {
        self.points
            .iter()
            .enumerate()
            .filter_map(|(i, p)| view.world_to_screen(p.xyz()).map(|q| (i, q.distance(pos))))
            .filter(|(_, d)| *d <= POINT_TOLERANCE)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn new_point_at(&self, state: &AppState, view: &ViewInfo, pos: Pos2, snap: &Snap) -> Option<DVec3> {
        match view.kind {
            ViewKind::Top => {
                let xy = snap.v2(view.screen_to_2d(pos));
                let z = self.auto_z(state, xy, self.last_z());
                Some(DVec3::new(xy.x, xy.y, z))
            }
            ViewKind::Perspective => {
                let (o, d) = view.ray(pos);
                let hit = pick_surface(state, view, pos).map(|h| h.point).or_else(|| {
                    let z = self.last_z();
                    (d.z.abs() > 1e-9).then(|| (z - o.z) / d.z).filter(|t| *t > 0.0 && *t < 1e6).map(|t| o + d * t)
                })?;
                let p = snap.on_plane(hit, DVec3::Z);
                let z = if self.options.meta.follow_terrain { self.auto_z(state, p.truncate(), hit.z) } else { hit.z };
                Some(DVec3::new(p.x, p.y, z))
            }
            _ => None,
        }
    }

    fn drag_point(&mut self, state: &AppState, view: &ViewInfo, pos: Pos2, snap: &Snap, index: usize) {
        let Some(old) = self.points.get(index).copied() else { return };
        let moved = match view.kind {
            ViewKind::Top => {
                let xy = snap.v2(view.screen_to_2d(pos));
                let z = if self.options.meta.follow_terrain { self.auto_z(state, xy, old.z) } else { old.z };
                Some(RoadPoint::new(xy.x, xy.y, z))
            }
            ViewKind::Front => {
                let v = snap.v2(view.screen_to_2d(pos));
                Some(RoadPoint::new(old.x, v.x, v.y))
            }
            ViewKind::Side => {
                let v = snap.v2(view.screen_to_2d(pos));
                Some(RoadPoint::new(v.x, old.y, v.y))
            }
            ViewKind::Perspective => {
                let (o, d) = view.ray(pos);
                (d.z.abs() > 1e-9).then(|| (old.z - o.z) / d.z).filter(|t| *t > 0.0 && *t < 1e6).map(|t| {
                    let p = snap.on_plane(o + d * t, DVec3::Z);
                    RoadPoint::new(p.x, p.y, old.z)
                })
            }
        };
        if let Some(p) = moved.filter(|p| p.is_valid())
            && let Some(slot) = self.points.get_mut(index)
        {
            *slot = p;
            self.bump();
        }
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        self.ensure_init(state);
        self.sync_material(state);
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::new(state, view, m.modifiers);
        if m.hovering() {
            self.hover = (self.point_at(view, pos).is_none() && self.gizmo_at(view, pos).is_none())
                .then(|| self.new_point_at(state, view, pos, &snap))
                .flatten()
                .map(|p| Hover { pos: p, kind: view.kind, frame: state.frame.index });
            return ToolFlow::Ignored;
        }
        let origin = m.origin().unwrap_or(pos);
        if m.drag_started {
            self.record_history();
            self.history_group = true;
            self.begin_point_drag(view, origin, m.modifiers);
        }
        if m.dragging || m.drag_stopped {
            match self.drag {
                Some(Drag::Point(i)) => {
                    let old = self.points[i].z;
                    self.drag_point(state, view, pos, &snap, i);
                    if matches!(view.kind, ViewKind::Front | ViewKind::Side) && (self.points[i].z - old).abs() > 1e-6 {
                        self.options.meta.follow_terrain = false;
                    }
                }
                Some(Drag::Gizmo(d)) => self.drag_gizmo(view, pos, &snap, d),
                _ => {}
            }
        }
        if m.clicked && !m.drag_started && self.drag.is_none() {
            let hit = self.point_at(view, origin);
            if m.double_clicked {
                self.create(state);
            } else if let (true, Some(i)) = (m.modifiers.command, hit) {
                self.remove_point_at(i);
            } else if self.gizmo_at(view, origin).is_some() {
            } else if m.modifiers.shift && self.points.len() >= 2 {
                if let Some(p) = self.new_point_at(state, view, origin, &snap) {
                    self.insert_point_near(p.truncate());
                }
            } else if let Some(i) = hit {
                self.selected = Some(i);
            } else if let Some(p) = self.new_point_at(state, view, origin, &snap) {
                self.add_point(RoadPoint::new(p.x, p.y, p.z));
            }
        }
        if m.drag_stopped {
            self.drag = None;
            self.history_group = false;
            self.record_history();
        }
        self.sync_preview(state);
        ToolFlow::Handled
    }

    pub fn create(&mut self, state: &mut AppState) -> Option<RoadCreated> {
        self.ensure_init(state);
        self.sync_material(state);
        if !self.is_ready() {
            return None;
        }
        let meta = self.meta();
        let lightmap = match state.settings.edit.default_lightmap_scale {
            v if v > 0 => v,
            _ => 16,
        };
        let bounds = match &*self.preview(state) {
            Ok(r) => r.bounds().unwrap_or_default(),
            Err(e) => {
                state.notify(Severity::Warning, error_text(e));
                return None;
            }
        };
        let result = match &self.editing {
            Some(e) => replace_road(&mut state.doc, &e.record, &meta, lightmap),
            None => match &self.options.create_as {
                CreateAs::World => create_road(&mut state.doc, &meta, lightmap, crate::curve::apply::Target::World),
                CreateAs::Entity(class) => {
                    let kv = fgd_defaults(state, class);
                    create_road(
                        &mut state.doc,
                        &meta,
                        lightmap,
                        crate::curve::apply::Target::NewEntity { class, keyvalues: &kv },
                    )
                }
            },
        };
        match result {
            Ok(made) => {
                let edited = self.editing.is_some();
                let n = made.ids.len();
                let text = if edited {
                    tf!("road.edited", n, fmt_size(&bounds))
                } else {
                    tf!("road.created", n, fmt_size(&bounds))
                };
                state.log(Severity::Info, text);
                if !made.stale.is_empty() {
                    state.notify(Severity::Warning, tf!("road.stale", made.stale.len()));
                }
                self.last_bounds = Some(bounds);
                state.runtime.brush.last_created = Some(bounds);
                self.clear();
                self.sync_history(state);
                state.assets.set_tool_preview(None, &[]);
                if edited || !self.options.keep_active {
                    state.set_tool(ToolId::Select);
                }
                Some(made)
            }
            Err(e) => {
                state.notify(Severity::Warning, apply_error_text(&e));
                None
            }
        }
    }

    pub fn split_selected(&mut self, state: &mut AppState) -> bool {
        let (Some(index), Some(editing)) = (self.selected, self.editing.as_ref()) else { return false };
        let meta = self.meta();
        let lightmap = match state.settings.edit.default_lightmap_scale {
            v if v > 0 => v,
            _ => 16,
        };
        let record = editing.record.clone();
        match split_road(&mut state.doc, &record, &meta, super::apply::SplitAt::Point(index), lightmap) {
            Ok((a, b)) => {
                state.log(Severity::Info, tf!("road.split_done", a.uid, b.uid));
                self.clear();
                state.set_tool(ToolId::Select);
                true
            }
            Err(e) => {
                state.notify(Severity::Warning, apply_error_text(&e));
                false
            }
        }
    }

    fn refresh_roads(&mut self, state: &AppState) {
        let rev = state.doc.revision();
        if self.roads_rev == rev {
            return;
        }
        self.roads_rev = rev;
        self.roads = list_roads(&state.doc)
            .into_iter()
            .map(|e| (e.meta.uid, tf!("road.list_item", e.meta.uid, e.record.ids.first().copied().unwrap_or(0))))
            .collect();
    }

    fn summary(&self, state: &AppState) -> Option<(String, bool)> {
        if self.points.is_empty() {
            return None;
        }
        Some(match &*self.preview(state) {
            Ok(r) => (tf!("road.summary", r.pieces.len(), fmt_len(r.length)), true),
            Err(RoadError::TooFewPoints) => (tf!("road.count", self.points.len()), true),
            Err(e) => (error_text(e), false),
        })
    }

    fn number(ui: &mut Ui, v: &mut f64, range: std::ops::RangeInclusive<f64>, tip: &str) {
        ui.add(egui::DragValue::new(v).speed(0.5).range(range).max_decimals(2)).on_hover_text(tip);
    }

    fn line_combo(ui: &mut Ui, salt: &str, kind: &mut LineKind) {
        let name = |k: LineKind| match k {
            LineKind::None => t!("road.line.none"),
            LineKind::Solid => t!("road.line.solid"),
            LineKind::Dashed => t!("road.line.dashed"),
            LineKind::Double => t!("road.line.double"),
        };
        egui::ComboBox::from_id_salt(salt).selected_text(name(*kind)).show_ui(ui, |ui| {
            for k in LineKind::ALL {
                ui.selectable_value(kind, k, name(k));
            }
        });
    }

    fn material_row(ui: &mut Ui, label: &str, tip: &str, value: &mut String, current: &str, allow_empty: bool) {
        ui.label(label).on_hover_text(tip);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(value).desired_width(170.0)).on_hover_text(tip);
            if ui.small_button(t!("road.use_current")).on_hover_text(t!("road.use_current_tip")).clicked() {
                *value = current.to_owned();
            }
            if allow_empty && ui.small_button("x").on_hover_text(t!("road.same_as_road_tip")).clicked() {
                value.clear();
            }
        });
        ui.end_row();
    }

    fn settings_window(&mut self, ui: &mut Ui, state: &mut AppState) {
        if !self.show_window {
            return;
        }
        self.refresh_roads(state);
        let current = active_material(state);
        let mut open = true;
        let mut action: Option<u8> = None;
        let own_uid = self.carry.uid;
        let editing = self.editing.is_some();
        let interior = self.selected.is_some_and(|i| i > 0 && i + 1 < self.points.len());
        let before = (self.options.clone(), self.carry.junction);
        let material_before = self.options.meta.material.clone();
        let rect = ui.ctx().content_rect();
        hp_app::chrome::Window::new(if editing { t!("road.window_edit") } else { t!("road.window_title") })
            .id(egui::Id::unique("hp_road_settings"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(Pos2::new(rect.left() + 64.0, rect.bottom() - 560.0))
            .show(ui.ctx(), |ui| {
                self.presets_ui(ui, state);
                ui.separator();
                let m = &mut self.options.meta;
                Self::layout_ui(ui, m);
                if m.layout != RoadLayout::Path {
                    ui.label(t!("road.layout_tip"));
                    if (m.follow_terrain
                        || m.bank.roll_start != 0.0
                        || m.bank.roll_end != 0.0
                        || m.bank.auto_gain != 0.0
                        || self.carry.junction.is_some())
                        && ui.button(t!("road.preset_plane")).clicked()
                    {
                        m.follow_terrain = false;
                        m.bank = crate::curve::Section::default();
                        self.carry.junction = None;
                    }
                }
                egui::Grid::new("hp_road_grid").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                    let mut lanes = m.lanes;
                    ui.label(t!("road.lanes")).on_hover_text(t!("road.lanes_tip"));
                    ui.add(egui::DragValue::new(&mut lanes).range(1..=MAX_LANES).speed(0.1))
                        .on_hover_text(t!("road.lanes_tip"));
                    m.lanes = lanes;
                    ui.end_row();
                    ui.label(t!("road.lane_width")).on_hover_text(t!("road.lane_width_tip"));
                    Self::number(ui, &mut m.lane_width, MIN_LANE_WIDTH..=MAX_LANE_WIDTH, t!("road.lane_width_tip"));
                    ui.end_row();
                    ui.label(t!("road.shoulder")).on_hover_text(t!("road.shoulder_tip"));
                    Self::number(ui, &mut m.shoulder, 0.0..=MAX_SHOULDER, t!("road.shoulder_tip"));
                    ui.end_row();
                    ui.label(t!("road.total")).on_hover_text(t!("road.total_tip"));
                    ui.label(fmt_len(m.total_width()));
                    ui.end_row();
                    ui.label(t!("road.thickness")).on_hover_text(t!("road.thickness_tip"));
                    Self::number(ui, &mut m.thickness, MIN_THICKNESS..=MAX_THICKNESS, t!("road.thickness_tip"));
                    ui.end_row();
                    ui.label(t!("road.step")).on_hover_text(t!("road.step_tip"));
                    Self::number(ui, &mut m.step, MIN_STEP..=MAX_STEP, t!("road.step_tip"));
                    ui.end_row();
                    ui.label(t!("create.block.tex_scale")).on_hover_text(t!("create.block.tex_scale_tip"));
                    ui.add(egui::DragValue::new(&mut m.tex_scale).speed(0.01).range(0.01..=64.0).max_decimals(3))
                        .on_hover_text(t!("create.block.tex_scale_tip"));
                    ui.end_row();
                    Self::material_row(
                        ui,
                        t!("road.material"),
                        t!("road.material_tip"),
                        &mut m.material,
                        &current,
                        false,
                    );
                    Self::material_row(
                        ui,
                        t!("road.shoulder_material"),
                        t!("road.shoulder_material_tip"),
                        &mut m.shoulder_material,
                        &current,
                        true,
                    );
                    Self::material_row(
                        ui,
                        t!("road.line_material"),
                        t!("road.line_material_tip"),
                        &mut m.line_material,
                        &current,
                        false,
                    );
                    Self::material_row(
                        ui,
                        t!("road.center_material"),
                        t!("road.center_material_tip"),
                        &mut m.center_material,
                        &current,
                        true,
                    );
                    ui.label(t!("road.center_line")).on_hover_text(t!("road.center_line_tip"));
                    Self::line_combo(ui, "hp_road_center", &mut m.center);
                    ui.end_row();
                    ui.label(t!("road.edge_line")).on_hover_text(t!("road.edge_line_tip"));
                    Self::line_combo(ui, "hp_road_edge", &mut m.edge);
                    ui.end_row();
                    ui.label(t!("road.divider_line")).on_hover_text(t!("road.divider_line_tip"));
                    Self::line_combo(ui, "hp_road_divider", &mut m.divider);
                    ui.end_row();
                    ui.label(t!("road.line_width")).on_hover_text(t!("road.line_width_tip"));
                    Self::number(ui, &mut m.line_width, MIN_LINE_WIDTH..=MAX_LINE_WIDTH, t!("road.line_width_tip"));
                    ui.end_row();
                    ui.label(t!("road.dash")).on_hover_text(t!("road.dash_tip"));
                    Self::number(ui, &mut m.dash, MIN_DASH..=MAX_DASH, t!("road.dash_tip"));
                    ui.end_row();
                    ui.label(t!("road.gap")).on_hover_text(t!("road.gap_tip"));
                    Self::number(ui, &mut m.gap, MIN_DASH..=MAX_DASH, t!("road.gap_tip"));
                    ui.end_row();
                    ui.label(t!("curve.roll_start")).on_hover_text(t!("curve.roll_start_tip"));
                    Self::number(ui, &mut m.bank.roll_start, -MAX_ROLL..=MAX_ROLL, t!("curve.roll_start_tip"));
                    ui.end_row();
                    ui.label(t!("curve.roll_end")).on_hover_text(t!("curve.roll_end_tip"));
                    Self::number(ui, &mut m.bank.roll_end, -MAX_ROLL..=MAX_ROLL, t!("curve.roll_end_tip"));
                    ui.end_row();
                    ui.label(t!("curve.auto_bank")).on_hover_text(t!("curve.auto_bank_tip"));
                    Self::number(ui, &mut m.bank.auto_gain, 0.0..=MAX_AUTO_GAIN, t!("curve.auto_bank_tip"));
                    ui.end_row();
                    ui.label(t!("curve.auto_limit")).on_hover_text(t!("curve.auto_limit_tip"));
                    Self::number(ui, &mut m.bank.auto_limit, 0.0..=MAX_AUTO_LIMIT, t!("curve.auto_limit_tip"));
                    ui.end_row();
                    ui.label(t!("road.follow_terrain")).on_hover_text(t!("road.follow_terrain_tip"));
                    ui.checkbox(&mut m.follow_terrain, "").on_hover_text(t!("road.follow_terrain_tip"));
                    ui.end_row();
                    ui.label(t!("road.lift")).on_hover_text(t!("road.lift_tip"));
                    Self::number(ui, &mut m.lift, -MAX_LIFT..=MAX_LIFT, t!("road.lift_tip"));
                    ui.end_row();
                    ui.label(t!("road.base_height")).on_hover_text(t!("road.base_height_tip"));
                    Self::number(ui, &mut self.options.base_z, -MAX_BASE_Z..=MAX_BASE_Z, t!("road.base_height_tip"));
                    ui.end_row();
                });
                if editing && self.options.meta.layout == RoadLayout::Path {
                    ui.separator();
                    ui.label(egui::RichText::new(t!("road.junction")).strong()).on_hover_text(t!("road.junction_tip"));
                    let candidates: Vec<(u32, String)> =
                        self.roads.iter().filter(|(uid, _)| *uid != own_uid).cloned().collect();
                    ui.horizontal(|ui| {
                        let label = self
                            .join_target
                            .and_then(|u| candidates.iter().find(|(c, _)| *c == u).map(|(_, l)| l.clone()))
                            .unwrap_or_else(|| t!("road.junction_pick").to_owned());
                        egui::ComboBox::from_id_salt("hp_road_join_target").selected_text(label).show_ui(ui, |ui| {
                            for (uid, text) in &candidates {
                                ui.selectable_value(&mut self.join_target, Some(*uid), text);
                            }
                        });
                        let end_name = |e: JunctionEnd| match e {
                            JunctionEnd::Start => t!("road.end_start"),
                            JunctionEnd::End => t!("road.end_end"),
                        };
                        egui::ComboBox::from_id_salt("hp_road_join_end")
                            .selected_text(end_name(self.join_end))
                            .show_ui(ui, |ui| {
                                for e in [JunctionEnd::Start, JunctionEnd::End] {
                                    ui.selectable_value(&mut self.join_end, e, end_name(e));
                                }
                            });
                    });
                    ui.horizontal(|ui| {
                        ui.label(t!("road.attach")).on_hover_text(t!("road.attach_tip"));
                        let name = |a: Attach| match a {
                            Attach::Outer => t!("road.attach_outer"),
                            Attach::Lane => t!("road.attach_lane"),
                        };
                        egui::ComboBox::from_id_salt("hp_road_join_attach")
                            .selected_text(name(self.join_attach))
                            .show_ui(ui, |ui| {
                                for a in [Attach::Outer, Attach::Lane] {
                                    ui.selectable_value(&mut self.join_attach, a, name(a));
                                }
                            })
                            .response
                            .on_hover_text(t!("road.attach_tip"));
                    });
                    ui.horizontal(|ui| {
                        ui.label(t!("road.flare")).on_hover_text(t!("road.flare_tip"));
                        Self::number(ui, &mut self.join_flare, 0.0..=MAX_FLARE, t!("road.flare_tip"));
                        if ui
                            .add_enabled(self.join_target.is_some(), egui::Button::new(t!("road.connect")))
                            .on_hover_text(t!("road.connect_tip"))
                            .clicked()
                        {
                            action = Some(1);
                        }
                        if ui
                            .add_enabled(self.carry.junction.is_some(), egui::Button::new(t!("road.disconnect")))
                            .on_hover_text(t!("road.disconnect_tip"))
                            .clicked()
                        {
                            action = Some(2);
                        }
                    });
                    if ui
                        .add_enabled(interior, egui::Button::new(t!("road.split")))
                        .on_hover_text(t!("road.split_tip"))
                        .clicked()
                    {
                        action = Some(3);
                    }
                }
            });
        if !open {
            self.show_window = false;
        }
        if self.options.meta.material != material_before {
            self.follow_material = self.options.meta.material == current;
        }
        match action {
            Some(1) => {
                if let Some(target) = self.join_target {
                    self.carry.junction =
                        Some(Junction { end: self.join_end, target, flare: self.join_flare, attach: self.join_attach });
                }
            }
            Some(2) => self.carry.junction = None,
            Some(3) => {
                self.split_selected(state);
            }
            _ => {}
        }
        self.options.meta.bank =
            crate::curve::Section { profile: crate::curve::Profile::Flat, ..self.options.meta.bank };
        if (self.options.clone(), self.carry.junction) != before {
            self.bump();
        }
    }
}

impl Tool for RoadTool {
    fn id(&self) -> ToolId {
        ToolId::Road
    }

    fn activate(&mut self, state: &mut AppState) {
        self.ensure_init(state);
        let auto_edit = state.runtime.brush.road_edit.is_none() && self.points.is_empty();
        if auto_edit {
            state.runtime.brush.road_edit = super::apply::selected_road_piece(&state.doc);
        }
        self.take_request(state);
        if auto_edit {
            self.show_window = false;
        }
        self.sync_preview(state);
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.cancel(state);
    }

    fn action(&mut self, state: &mut AppState, action: Action) -> bool {
        if !matches!(action, Action::Undo | Action::Redo) || !self.history_active {
            return false;
        }
        self.restore_history(action == Action::Redo);
        if self.options.meta.material != active_material(state) {
            self.follow_material = false;
            self.baseline = Some(self.snapshot());
        }
        self.sync_preview(state);
        true
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        self.take_request(state);
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn click_while_flying(
        &mut self,
        state: &mut AppState,
        view: &ViewInfo,
        pos: Pos2,
        modifiers: egui::Modifiers,
    ) -> ToolFlow {
        self.handle_mouse(
            state,
            view,
            &Mouse { pos: Some(pos), press_origin: Some(pos), clicked: true, modifiers, ..Default::default() },
        )
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.points.is_empty() && !self.history_active {
            return false;
        }
        if consume_plain_key(ctx, Key::Enter) {
            self.create(state);
            return true;
        }
        if consume_plain_key(ctx, Key::Backspace)
            && let Some(last) = self.points.len().checked_sub(1)
        {
            return self.remove_point_at(last);
        }
        if consume_plain_key(ctx, Key::Delete)
            && let Some(i) = self.selected
        {
            return self.remove_point_at(i);
        }
        if consume_plain_key(ctx, Key::Escape) {
            return self.cancel(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let mut ov = Ov::new(out, view);
        let line = rgba(COLOR_PENDING, 1.0);
        let faint = rgba(COLOR_PENDING, 0.35);
        let built = self.preview(state);
        let good = built.is_ok();
        if !good {
            for w in self.points.windows(2) {
                ov.line(w[0].xyz(), w[1].xyz(), faint);
            }
        }
        if let Ok(road) = &*built {
            let shape = rgba(COLOR_SHAPE, 0.9);
            let centre = rgba(COLOR_PENDING, 0.8);
            if view.is_2d() {
                if road.meta.layout != RoadLayout::Path {
                    for piece in &road.pieces {
                        let n = piece.corners.len() / 2;
                        for i in 0..n {
                            ov.line(piece.corners[i], piece.corners[(i + 1) % n], shape);
                        }
                    }
                }
                let polyline = |ov: &mut Ov<'_>, pts: &[DVec3], color: [f32; 4]| {
                    for w in pts.windows(2) {
                        ov.line(w[0], w[1], color);
                    }
                };
                if view.kind == ViewKind::Top && road.meta.layout == RoadLayout::Path {
                    for (side, lanes) in [(false, false), (false, true), (true, false), (true, true)] {
                        let edge = road.edge(
                            if side { super::build::LateralSide::Left } else { super::build::LateralSide::Right },
                            lanes,
                        );
                        polyline(&mut ov, &edge, if lanes { rgba(COLOR_SHAPE, 0.45) } else { shape });
                    }
                }
                let c: Vec<DVec3> = road.samples.iter().map(|s| s.center).collect();
                polyline(&mut ov, &c, centre);
            } else {
                for w in road.samples.windows(2) {
                    ov.line(w[0].center + DVec3::Z * 0.5, w[1].center + DVec3::Z * 0.5, centre);
                }
            }
        }
        let bad = rgba(COLOR_REMOVED, 1.0);
        for (i, p) in self.points.iter().enumerate() {
            let size = if self.selected == Some(i) { 10.0 } else { 7.0 };
            let color = if !good && self.points.len() >= 2 {
                bad
            } else if self.selected == Some(i) {
                line
            } else {
                rgba(egui::Color32::WHITE, 1.0)
            };
            ov.point(p.xyz(), color, size);
        }
        if let Some(h) = self.hover.filter(|h| h.kind == view.kind && fresh(state, h.frame) && self.drag.is_none())
            && let Some(last) = self.points.last()
        {
            ov.line(last.xyz(), h.pos, rgba(COLOR_PENDING, 0.7));
            ov.point(h.pos, rgba(COLOR_PENDING, 0.7), 6.0);
        } else if let Some(h) = self.hover.filter(|h| h.kind == view.kind && fresh(state, h.frame)) {
            ov.point(h.pos, rgba(COLOR_PENDING, 0.7), 6.0);
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        if let Some(layout) = self.gizmo_layout(view) {
            let active = match self.drag {
                Some(Drag::Gizmo(d)) => Some(d.part),
                _ => painter.ctx().pointer_hover_pos().and_then(|p| self.gizmo_at(view, p)),
            };
            layout.paint(view, &painter, GizmoMode::Move, active);
        }
        if let Some(i) = self.selected
            && let Some(p) = self.points.get(i)
            && let Some(s) = view.world_to_screen(p.xyz())
            && view.rect.contains(s)
        {
            paint_label(
                &painter,
                s + Vec2::new(10.0, -10.0),
                Align2::LEFT_BOTTOM,
                &tf!("road.point_label", i + 1, fmt_len(p.z)),
                COLOR_PENDING,
            );
        }
        if let Some((text, ok)) = self.summary(state)
            && let Some(last) = self.points.last()
            && let Some(s) = view.world_to_screen(last.xyz())
            && view.rect.contains(s)
        {
            let color = if ok { COLOR_PENDING } else { hp_app::theme::VIEW_ERROR };
            paint_label(&painter, s - Vec2::new(0.0, 14.0), Align2::CENTER_BOTTOM, &text, color);
        }
        if let Some(h) = self.hover.filter(|h| h.kind == view.kind && fresh(state, h.frame))
            && let Some(last) = self.points.last()
            && let Some(s) = view.world_to_screen(h.pos)
        {
            let text = tf!("road.hover", fmt_len(last.xyz().distance(h.pos)), fmt_len(h.pos.z));
            paint_label(&painter, s + Vec2::new(12.0, 12.0), Align2::LEFT_TOP, &text, COLOR_PENDING);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.ensure_init(state);
        self.sync_material(state);
        self.take_request(state);
        if ui.input(|i| i.pointer.any_down()) && self.drag.is_none() && !self.history_group {
            self.record_history();
            self.history_group = true;
        }
        let before = (self.options.clone(), self.selected, self.points.clone());
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("road.new")).on_hover_text(t!("road.new_tip")).clicked() {
                self.clear();
                state.assets.set_tool_preview(None, &[]);
            }
            if self.editing.is_some() {
                ui.label(egui::RichText::new(t!("road.editing")).strong());
            }
            self.preset_combo(ui, state, "strip");
            let m = &mut self.options.meta;
            Self::layout_ui(ui, m);
            ui.label(t!("road.lanes")).on_hover_text(t!("road.lanes_tip"));
            let mut lanes = m.lanes;
            ui.add(egui::DragValue::new(&mut lanes).range(1..=MAX_LANES).speed(0.1))
                .on_hover_text(t!("road.lanes_tip"));
            m.lanes = lanes;
            ui.label(t!("road.lane_width")).on_hover_text(t!("road.lane_width_tip"));
            Self::number(ui, &mut m.lane_width, MIN_LANE_WIDTH..=MAX_LANE_WIDTH, t!("road.lane_width_tip"));
            ui.label(t!("road.shoulder")).on_hover_text(t!("road.shoulder_tip"));
            Self::number(ui, &mut m.shoulder, 0.0..=MAX_SHOULDER, t!("road.shoulder_tip"));
            ui.checkbox(&mut m.follow_terrain, t!("road.follow_terrain")).on_hover_text(t!("road.follow_terrain_tip"));
            ui.separator();
            if self.editing.is_none() {
                create_as_ui(ui, state, &mut self.picker, &mut self.options.create_as, "road_create_as");
                ui.separator();
                material_chip(ui, state);
                ui.separator();
            }
            if ui
                .selectable_label(self.show_window, t!("road.settings"))
                .on_hover_text(t!("road.settings_tip"))
                .clicked()
            {
                self.show_window = !self.show_window;
            }
            if hp_app::widgets::icon_toggle(ui, "📌", self.options.keep_active, t!("road.keep_active_tip")).clicked()
            {
                self.options.keep_active = !self.options.keep_active;
            }
            let floor = if self.editing.is_some() { 2 } else { 0 };
            if ui
                .add_enabled(self.points.len() > floor, egui::Button::new(t!("road.undo_point")))
                .on_hover_text(tf!("road.undo_point_tip", key_label(Key::Backspace)))
                .clicked()
            {
                let last = self.points.len().saturating_sub(1);
                self.remove_point_at(last);
            }
            if ui
                .add_enabled(self.points.len() >= 2, egui::Button::new(t!("road.reverse")))
                .on_hover_text(t!("road.reverse_tip"))
                .clicked()
            {
                self.reverse();
            }
            let apply = if self.editing.is_some() { t!("road.apply") } else { t!("road.create") };
            if ui
                .add_enabled(self.preview(state).is_ok(), egui::Button::new(apply))
                .on_hover_text(tf!("road.create_tip", key_label(Key::Enter)))
                .clicked()
            {
                self.create(state);
            }
            if let Some(i) = self.selected.filter(|i| *i < self.points.len()) {
                ui.separator();
                ui.label(tf!("road.point", i + 1, self.points.len())).on_hover_text(t!("road.point_tip"));
                let mut point = self.points[i].xyz();
                for axis in 0..3 {
                    if ui
                        .add(
                            egui::DragValue::new(&mut point[axis])
                                .prefix(["X ", "Y ", "Z "][axis])
                                .speed(0.5)
                                .range(-MAX_BASE_Z..=MAX_BASE_Z)
                                .max_decimals(2),
                        )
                        .on_hover_text(t!("road.point_height_tip"))
                        .changed()
                    {
                        if axis == 2 {
                            self.options.meta.follow_terrain = false;
                        }
                        self.points[i] = RoadPoint::new(point.x, point.y, point.z);
                    }
                }
            }
            if let Some((text, ok)) = self.summary(state) {
                ui.separator();
                let color = if ok { ui.visuals().weak_text_color() } else { hp_app::theme::VIEW_ERROR };
                ui.label(egui::RichText::new(text).color(color));
                if matches!(&*self.preview(state), Err(RoadError::Folded(_)))
                    && ui.button(t!("road.fit_width")).on_hover_text(t!("road.fit_width_tip")).clicked()
                    && !self.fit_width_to_bends(state)
                {
                    state.notify(Severity::Warning, t!("road.fit_failed").to_owned());
                }
            }
        });
        self.options.meta = RoadMeta { bank: self.options.meta.bank.clamped(), ..self.options.meta.clone() };
        if (self.options.clone(), self.selected, self.points.clone()) != before {
            self.bump();
        }
        self.settings_window(ui, state);
        if !ui.input(|i| i.pointer.any_down()) && self.drag.is_none() && self.history_group {
            self.history_group = false;
            self.record_history();
        }
        self.sync_preview(state);
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if matches!(self.drag, Some(Drag::Point(_) | Drag::Gizmo(_))) {
            return Some(CursorIcon::Grabbing);
        }
        if self.point_at(view, pos).is_some() || self.gizmo_at(view, pos).is_some() {
            return Some(CursorIcon::Grab);
        }
        Some(if view.kind == ViewKind::Top || !view.is_2d() { CursorIcon::Crosshair } else { CursorIcon::Default })
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        if self.options.meta.layout != RoadLayout::Path {
            return Some(format!("{} {}", t!("road.layout_tip"), t!("road.history_tip")));
        }
        let enter = key_label(Key::Enter);
        let back = key_label(Key::Backspace);
        let escape = key_label(Key::Escape);
        let ctrl = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
        let hint = match (self.points.len(), self.editing.is_some()) {
            (0, _) => t!("road.hint_idle").to_owned(),
            (_, true) => tf!("road.hint_edit", enter, escape, ctrl),
            (1, false) => tf!("road.hint_points", back, escape),
            _ => tf!("road.hint_ready", enter, back, escape, ctrl),
        };
        Some(format!("{hint}  Ctrl+Z / Ctrl+Y"))
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        let had = !self.points.is_empty() || self.drag.is_some() || self.editing.is_some() || self.history_active;
        self.clear();
        self.sync_history(state);
        state.assets.set_tool_preview(None, &[]);
        had
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        self.ensure_init(state);
        let v = value.trim();
        match key {
            "preset_save" => return self.save_user_preset(state, v),
            "preset_load" => return self.load_user_preset(state, v),
            "preset_delete" => return self.delete_user_preset(state, v),
            _ => {}
        }
        if key == "fit_width" {
            return match v.to_ascii_lowercase().as_str() {
                "on" | "true" | "1" => self.fit_width_to_bends(state),
                "off" | "false" | "0" => true,
                _ => false,
            };
        }
        if key == "selected_point" {
            let Some(index) = v.parse::<usize>().ok().filter(|i| *i <= self.points.len()) else {
                return false;
            };
            self.selected = index.checked_sub(1);
            return true;
        }
        if let Some(axis) = ["point_x", "point_y", "point_z"].iter().position(|key_name| *key_name == key) {
            let Some(value) = v.parse::<f64>().ok().filter(|v| v.is_finite() && v.abs() <= MAX_BASE_Z) else {
                return false;
            };
            let Some(index) = self.selected else { return value == 0.0 };
            let Some(point) = self.points.get(index) else { return false };
            let mut point = point.xyz();
            point[axis] = value;
            self.points[index] = RoadPoint::new(point.x, point.y, point.z);
            if axis == 2 {
                self.options.meta.follow_terrain = false;
            }
            self.bump();
            return true;
        }
        let flag = match v.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        let num = v.parse::<f64>().ok().filter(|x| x.is_finite());
        let ranged = |lo: f64, hi: f64| num.filter(|x| (lo..=hi).contains(x));
        let name =
            |s: &str| (!s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)).then(|| s.to_owned());
        let m = &mut self.options.meta;
        let done = match key {
            "layout" => RoadLayout::parse(v).map(|x| m.layout = x).is_some(),
            "radius" => ranged(0.0, MAX_RADIUS).map(|x| m.radius = x).is_some(),
            "lanes" => num
                .filter(|n| n.fract() == 0.0 && (1.0..=f64::from(MAX_LANES)).contains(n))
                .map(|n| m.lanes = n as u32)
                .is_some(),
            "lane_width" => ranged(MIN_LANE_WIDTH, MAX_LANE_WIDTH).map(|x| m.lane_width = x).is_some(),
            "shoulder" => ranged(0.0, MAX_SHOULDER).map(|x| m.shoulder = x).is_some(),
            "thickness" => ranged(MIN_THICKNESS, MAX_THICKNESS).map(|x| m.thickness = x).is_some(),
            "step" => ranged(MIN_STEP, MAX_STEP).map(|x| m.step = x).is_some(),
            "tex_scale" => ranged(0.01, 64.0).map(|x| m.tex_scale = x).is_some(),
            "material" => name(v).map(|s| m.material = s).is_some_and(|_| {
                self.follow_material = false;
                true
            }),
            "shoulder_material" => {
                m.shoulder_material = if v.is_empty() { String::new() } else { name(v).unwrap_or_default() };
                v.is_empty() || !m.shoulder_material.is_empty()
            }
            "line_material" => name(v).map(|s| m.line_material = s).is_some(),
            "center_material" => {
                m.center_material = if v.is_empty() { String::new() } else { name(v).unwrap_or_default() };
                v.is_empty() || !m.center_material.is_empty()
            }
            "center_line" => LineKind::parse(v).map(|k| m.center = k).is_some(),
            "edge_line" => LineKind::parse(v).map(|k| m.edge = k).is_some(),
            "divider_line" => LineKind::parse(v).map(|k| m.divider = k).is_some(),
            "line_width" => ranged(MIN_LINE_WIDTH, MAX_LINE_WIDTH).map(|x| m.line_width = x).is_some(),
            "dash" => ranged(MIN_DASH, MAX_DASH).map(|x| m.dash = x).is_some(),
            "gap" => ranged(MIN_DASH, MAX_DASH).map(|x| m.gap = x).is_some(),
            "roll_start" => ranged(-MAX_ROLL, MAX_ROLL).map(|x| m.bank.roll_start = x).is_some(),
            "roll_end" => ranged(-MAX_ROLL, MAX_ROLL).map(|x| m.bank.roll_end = x).is_some(),
            "auto_bank" => ranged(0.0, MAX_AUTO_GAIN).map(|x| m.bank.auto_gain = x).is_some(),
            "auto_bank_limit" => ranged(0.0, MAX_AUTO_LIMIT).map(|x| m.bank.auto_limit = x).is_some(),
            "follow_terrain" => flag.map(|b| m.follow_terrain = b).is_some(),
            "lift" => ranged(-MAX_LIFT, MAX_LIFT).map(|x| m.lift = x).is_some(),
            "base_height" => ranged(-MAX_BASE_Z, MAX_BASE_Z).map(|x| self.options.base_z = x).is_some(),
            "keep_active" => flag.map(|b| self.options.keep_active = b).is_some(),
            "create_as" => {
                if v.eq_ignore_ascii_case("world") {
                    self.options.create_as = CreateAs::World;
                    true
                } else if !v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    self.options.create_as = CreateAs::Entity(v.to_ascii_lowercase());
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if done {
            self.bump();
        }
        done
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        use hp_app::tool::ToolOption;
        let m = &self.options.meta;
        let num = |v: f64| {
            let s = format!("{v:.4}");
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        };
        let flag = |b: bool| if b { "on" } else { "off" };
        let mut values = vec![
            ToolOption::new(
                "preset_save",
                "",
                "save or update the current road settings under this name; saved in the editor settings",
            ),
            ToolOption::new(
                "preset_load",
                self.preset_name.clone(),
                "load a named road settings preset without replacing the current points; supports local Undo/Redo",
            ),
            ToolOption::new("preset_delete", "", "delete the named saved road settings preset"),
            ToolOption::new(
                "layout",
                m.layout.name(),
                "path, tee, cross, wye or cul_de_sac; presets use exactly two points and no banking or terrain following",
            ),
            ToolOption::new("radius", num(m.radius), "corner radius, or bulb radius for cul_de_sac (0 to 4096)"),
            ToolOption::new(
                "fit_width",
                "off",
                "set on to fit lane, shoulder and line widths to the current tight bend without moving points",
            ),
            ToolOption::new("lanes", m.lanes.to_string(), "number of lanes (1 to 8)"),
            ToolOption::new(
                "lane_width",
                num(m.lane_width),
                "width of one lane in units (16 to 32768), within the map bounds",
            ),
            ToolOption::new(
                "shoulder",
                num(m.shoulder),
                "width of the shoulder on each side in units (0, or 1 to 256)",
            ),
            ToolOption::new(
                "thickness",
                num(m.thickness),
                "depth of the road slab under the surface in units (2 to 128)",
            ),
            ToolOption::new("step", num(m.step), "target length of one road segment in units (16 to 512)"),
            ToolOption::new("tex_scale", num(m.tex_scale), "texture scale of the road faces (0.01 to 64)"),
            ToolOption::new("material", m.material.clone(), "material of the lanes"),
            ToolOption::new(
                "shoulder_material",
                m.shoulder_material.clone(),
                "material of the shoulders, empty: same as the lanes",
            ),
            ToolOption::new("line_material", m.line_material.clone(), "material of the lines and stripes"),
            ToolOption::new(
                "center_material",
                m.center_material.clone(),
                "material of the center line, empty: same as the lines",
            ),
            ToolOption::new("center_line", m.center.name(), "center line: none, solid, dashed or double"),
            ToolOption::new("edge_line", m.edge.name(), "edge lines: none, solid, dashed or double"),
            ToolOption::new("divider_line", m.divider.name(), "lines between lanes: none, solid, dashed or double"),
            ToolOption::new("line_width", num(m.line_width), "width of a line in units (1 to 32)"),
            ToolOption::new("dash", num(m.dash), "length of a dash in units (4 to 1024)"),
            ToolOption::new("gap", num(m.gap), "length of the gap between dashes in units (4 to 1024)"),
            ToolOption::new("roll_start", num(m.bank.roll_start), "banking at the start in degrees (-60 to 60)"),
            ToolOption::new("roll_end", num(m.bank.roll_end), "banking at the end in degrees (-60 to 60)"),
            ToolOption::new(
                "auto_bank",
                num(m.bank.auto_gain),
                "automatic banking for a bend of radius 1000 (0 to 90)",
            ),
            ToolOption::new(
                "auto_bank_limit",
                num(m.bank.auto_limit),
                "largest automatic banking in degrees (0 to 45)",
            ),
            ToolOption::new("follow_terrain", flag(m.follow_terrain), "on: the road follows the ground under it"),
            ToolOption::new("lift", num(m.lift), "height above the ground when following the terrain (-256 to 256)"),
            ToolOption::new(
                "base_height",
                num(self.options.base_z),
                "height of the first point when no ground is found (-16384 to 16384)",
            ),
            ToolOption::new(
                "create_as",
                match &self.options.create_as {
                    CreateAs::World => "world".to_owned(),
                    CreateAs::Entity(c) => c.clone(),
                },
                "world, or a brush entity class (func_detail...) holding every brush",
            ),
            ToolOption::new("keep_active", flag(self.options.keep_active), "on: stay in the road tool after creating"),
        ];
        let point = self.selected.and_then(|i| self.points.get(i)).copied().unwrap_or(RoadPoint::new(0.0, 0.0, 0.0));
        values.extend([
            ToolOption::new(
                "selected_point",
                self.selected.map_or(0, |i| i + 1).to_string(),
                "select a control point numbered from 1; 0 means none selected",
            ),
            ToolOption::new("point_x", num(point.x), "X coordinate, requires a selected point"),
            ToolOption::new("point_y", num(point.y), "Y coordinate, requires a selected point"),
            ToolOption::new(
                "point_z",
                num(point.z),
                "height, requires a selected point; manual height turns off terrain following",
            ),
        ]);
        values
    }
}
