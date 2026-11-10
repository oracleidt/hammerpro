use std::cell::RefCell;

use egui::{Align2, CursorIcon, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use glam::{DVec2, DVec3};
use hp_app::actions::key_label;
use hp_app::state::AppState;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ToolMenuEntry, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::{EditError, ObjRef, Owner};
use hp_geom::{
    Aabb, MAX_DOOR_ARCH_SEGMENTS, MAX_PRIMITIVE_SIDES, MAX_STAIR_STEPS, PrimitiveParams, make_primitive, step_count,
};
use hp_render::Overlay;

pub use hp_geom::Primitive;

use crate::block_preview::{self, PreviewStyle, Projection};
use crate::common::{
    BoxHandle, COLOR_PENDING, COLOR_SHAPE, MIN_EXTENT, Mouse, Ov, Snap, box_screen_rect, clean, consume_plain_key,
    depth_axis, edit_box, fgd_defaults, fmt_len, fmt_size, footprint_box, fresh, ground_point, hit_handle,
    material_chip, paint_handle, paint_label, pick_surface, rgba,
};
use crate::picker::{ClassPicker, PickerSource};

pub const DEFAULT_DEPTH: f64 = 64.0;
pub const WHEEL_FAST_STEP: u32 = 4;
const WORLD_ID: &str = "@world";
const CTX_CREATE: u32 = 1;
const CTX_CYLINDER: u32 = 2;
const CTX_ARCH: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateAs {
    World,
    Entity(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockOptions {
    pub kind: Primitive,
    pub sides: u32,
    pub arch_wall_width: f64,
    pub arch_arc: f64,
    pub arch_start_angle: f64,
    pub arch_add_height: f64,
    pub torus_ring_width: f64,
    pub torus_tube_sides: u32,
    pub torus_arc: f64,
    pub torus_start_angle: f64,
    pub torus_height: f64,
    pub stair_steps: u32,
    pub stair_step_height: f64,
    pub stair_open: bool,
    pub spiral_inner: f64,
    pub spiral_angle: f64,
    pub spiral_start: f64,
    pub ramp_angle: f64,
    pub pillar_base: f64,
    pub pillar_capital: f64,
    pub pillar_inset: f64,
    pub door_width: f64,
    pub door_height: f64,
    pub door_thickness: f64,
    pub door_arch: bool,
    pub door_arch_radius: f64,
    pub door_arch_segments: u32,
    pub direction: hp_geom::RiseDirection,
    pub reorient: bool,
    pub tex_scale: f64,
    pub create_as: CreateAs,
    pub keep_active: bool,
}

impl Default for BlockOptions {
    fn default() -> Self {
        Self {
            kind: Primitive::Block,
            sides: 8,
            arch_wall_width: 16.0,
            arch_arc: 360.0,
            arch_start_angle: 0.0,
            arch_add_height: 0.0,
            torus_ring_width: 32.0,
            torus_tube_sides: 8,
            torus_arc: 360.0,
            torus_start_angle: 0.0,
            torus_height: 0.0,
            stair_steps: 8,
            stair_step_height: 0.0,
            stair_open: false,
            spiral_inner: 16.0,
            spiral_angle: 270.0,
            spiral_start: 0.0,
            ramp_angle: 30.0,
            pillar_base: 16.0,
            pillar_capital: 16.0,
            pillar_inset: 8.0,
            door_width: 0.0,
            door_height: 0.0,
            door_thickness: 0.0,
            door_arch: false,
            door_arch_radius: 0.0,
            door_arch_segments: 8,
            direction: hp_geom::RiseDirection::PlusY,
            reorient: true,
            tex_scale: hp_geom::DEFAULT_TEXTURE_SCALE,
            create_as: CreateAs::World,
            keep_active: true,
        }
    }
}

pub fn new_depth_range(state: &AppState, last: Option<Aabb>, d: usize) -> (f64, f64) {
    let selection = state
        .doc
        .selection_bounds(&|e| state.entity_bounds(e))
        .filter(|b| b.min[d].is_finite() && b.max[d].is_finite());
    if let Some(b) = selection
        && b.size()[d] > MIN_EXTENT
    {
        return (clean(b.min[d]), clean(b.max[d]));
    }
    if let Some(b) = last
        && b.size()[d] > MIN_EXTENT
    {
        return (b.min[d], b.max[d]);
    }
    let base = selection.map_or(0.0, |b| clean(b.max[d]));
    (base, base + DEFAULT_DEPTH)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeCount {
    Sides,
    Steps,
    ArchSegments,
}

impl ShapeCount {
    pub fn of(o: &BlockOptions) -> Option<Self> {
        if is_round(o.kind) {
            return Some(Self::Sides);
        }
        match o.kind {
            Primitive::Stairs | Primitive::SpiralStairs => Some(Self::Steps),
            Primitive::Doorway if o.door_arch => Some(Self::ArchSegments),
            _ => None,
        }
    }

    pub fn range(self) -> (u32, u32) {
        match self {
            Self::Sides => (3, MAX_PRIMITIVE_SIDES),
            Self::Steps => (1, MAX_STAIR_STEPS),
            Self::ArchSegments => (2, MAX_DOOR_ARCH_SEGMENTS),
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::Sides => "create.block.count_sides",
            Self::Steps => "create.block.count_steps",
            Self::ArchSegments => "create.block.count_segments",
        }
    }

    fn hint_key(self) -> &'static str {
        match self {
            Self::Sides => "create.block.wheel_sides",
            Self::Steps => "create.block.wheel_steps",
            Self::ArchSegments => "create.block.wheel_segments",
        }
    }
}

pub(crate) fn primitive_key(kind: Primitive) -> &'static str {
    match kind {
        Primitive::Block => "create.prim.block",
        Primitive::Wedge => "create.prim.wedge",
        Primitive::Cylinder => "create.prim.cylinder",
        Primitive::Spike => "create.prim.spike",
        Primitive::Sphere => "create.prim.sphere",
        Primitive::Arch => "create.prim.arch",
        Primitive::Torus => "create.prim.torus",
        Primitive::Stairs => "create.prim.stairs",
        Primitive::SpiralStairs => "create.prim.spiral_stairs",
        Primitive::Ramp => "create.prim.ramp",
        Primitive::Pillar => "create.prim.pillar",
        Primitive::Doorway => "create.prim.doorway",
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase3d {
    Base,
    Height { anchor: Pos2, base_height: f64, height: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Draw3d {
    view: usize,
    axis: usize,
    sign: f64,
    level: f64,
    start: DVec3,
    end: DVec3,
    phase: Phase3d,
    surface: Option<ObjRef>,
}

impl Draw3d {
    fn plane_point(&self, origin: DVec3, dir: DVec3, snap: &Snap) -> Option<DVec3> {
        let a = self.axis;
        if dir[a].abs() < 1e-9 {
            return None;
        }
        let t = (self.level - origin[a]) / dir[a];
        if !(t > 0.0 && t.is_finite() && t < 1e6) {
            return None;
        }
        let mut p = origin + dir * t;
        for i in 0..3 {
            if i != a {
                p[i] = snap.v(p[i], i);
            }
        }
        p[a] = self.level;
        p.is_finite().then_some(p)
    }

    fn base_valid(&self) -> bool {
        (0..3).filter(|&i| i != self.axis).all(|i| (self.start[i] - self.end[i]).abs() > MIN_EXTENT)
    }

    fn height(&self) -> f64 {
        match self.phase {
            Phase3d::Base => 0.0,
            Phase3d::Height { height, .. } => height,
        }
    }

    fn bounds(&self) -> Aabb {
        let mut min = self.start.min(self.end);
        let mut max = self.start.max(self.end);
        let top = self.level + self.sign * self.height();
        min[self.axis] = self.level.min(top);
        max[self.axis] = self.level.max(top);
        Aabb { min, max }
    }

    fn final_bounds(&self) -> Option<Aabb> {
        let b = self.bounds();
        (matches!(self.phase, Phase3d::Height { .. }) && b.size().min_element() > MIN_EXTENT).then_some(b)
    }

    fn base_center(&self) -> DVec3 {
        let mut c = (self.start + self.end) * 0.5;
        c[self.axis] = self.level;
        c
    }

    fn axis_dir(&self) -> DVec3 {
        let mut d = DVec3::ZERO;
        d[self.axis] = self.sign;
        d
    }
}

fn height_from_mouse(view: &ViewInfo, d: &Draw3d, anchor: Pos2, pos: Pos2, base: f64, snap: &Snap) -> f64 {
    let center = d.base_center();
    let dir = d.axis_dir();
    let delta = pos - anchor;
    let probe = 16.0;
    let units = match (view.world_to_screen(center), view.world_to_screen(center + dir * probe)) {
        (Some(a), Some(b)) if (b - a).length() > probe as f32 * 0.05 => {
            let per_unit = (b - a) / probe as f32;
            f64::from(delta.dot(per_unit) / per_unit.length_sq())
        }
        _ => {
            let dist = (center - view.cam3d.pos).length().max(1.0);
            let per_point =
                2.0 * (view.cam3d.fov_y_deg.to_radians() * 0.5).tan() * dist / f64::from(view.rect.height().max(1.0));
            -f64::from(delta.y) * per_point
        }
    };
    let raw = base + units;
    let h = snap.len(raw, d.axis);
    let step = snap.step_along(d.axis);
    if h.abs() < MIN_EXTENT { if raw < 0.0 { -step } else { step } } else { h }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    New { view: usize, kind: ViewKind, start: DVec2, depth: (f64, f64) },
    Edit { view: usize, kind: ViewKind, handle: BoxHandle, start_box: Aabb, from: DVec2 },
    Swallow,
}

#[derive(Debug, Default)]
struct ShapePreview {
    params: Option<PrimitiveParams>,
    edges: Vec<[DVec3; 2]>,
    solids: Vec<hp_geom::SolidGeom>,
    error: Option<String>,
    projection: Option<Projection>,
}

pub struct BlockTool {
    pub options: BlockOptions,
    pending: Option<Aabb>,
    draw_up: usize,
    last_bounds: Option<Aabb>,
    drag: Option<Drag>,
    draw3d: Option<Draw3d>,
    hover: Option<(usize, BoxHandle, u64)>,
    hover_3d: Option<(DVec3, u64)>,
    initialized: bool,
    seen_reorient: Option<bool>,
    picker: ClassPicker,
    shape: RefCell<ShapePreview>,
    preview_open: bool,
    wheel: f32,
}

impl Default for BlockTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockTool {
    pub fn new() -> Self {
        Self {
            options: BlockOptions::default(),
            pending: None,
            draw_up: 2,
            last_bounds: None,
            drag: None,
            draw3d: None,
            hover: None,
            hover_3d: None,
            initialized: false,
            seen_reorient: None,
            picker: ClassPicker::default(),
            shape: RefCell::new(ShapePreview::default()),
            preview_open: false,
            wheel: 0.0,
        }
    }

    pub fn is_drawing(&self) -> bool {
        self.pending.is_some()
            || self.draw3d.is_some()
            || matches!(self.drag, Some(Drag::New { .. } | Drag::Edit { .. }))
    }

    pub fn shape_count(&self) -> Option<(ShapeCount, u32)> {
        let count = ShapeCount::of(&self.options)?;
        let o = &self.options;
        let value = match count {
            ShapeCount::Sides => o.sides,
            ShapeCount::Steps => match self.current_box() {
                Some(b) if o.stair_step_height > 0.0 => {
                    step_count(b.size()[self.up_axis()], o.stair_steps, o.stair_step_height).unwrap_or(o.stair_steps)
                }
                _ => o.stair_steps,
            },
            ShapeCount::ArchSegments => o.door_arch_segments,
        };
        Some((count, value))
    }

    pub fn step_shape_count(&mut self, delta: i64) -> Option<u32> {
        let (count, value) = self.shape_count()?;
        let (lo, hi) = count.range();
        let next = (i64::from(value) + delta).clamp(i64::from(lo), i64::from(hi)) as u32;
        let o = &mut self.options;
        match count {
            ShapeCount::Sides => o.sides = next,
            ShapeCount::Steps => {
                o.stair_steps = next;
                o.stair_step_height = 0.0;
            }
            ShapeCount::ArchSegments => o.door_arch_segments = next,
        }
        Some(next)
    }

    pub fn pending_box(&self) -> Option<Aabb> {
        self.pending
    }

    pub fn set_pending_box(&mut self, b: Option<Aabb>) {
        self.pending = b.filter(|b| b.size().min_element() > MIN_EXTENT);
        self.draw_up = 2;
    }

    pub fn up_axis(&self) -> usize {
        if !self.options.reorient {
            return 2;
        }
        match self.draw3d {
            Some(d) => d.axis,
            None => self.draw_up,
        }
    }

    pub fn last_bounds(&self) -> Option<Aabb> {
        self.last_bounds
    }

    fn ensure_init(&mut self, state: &AppState) {
        let reorient = state.settings.brush.reorient_primitives;
        if self.seen_reorient != Some(reorient) {
            self.seen_reorient = Some(reorient);
            self.options.reorient = reorient;
        }
        if self.initialized {
            return;
        }
        self.initialized = true;
        let s = state.settings.edit.default_texture_scale;
        if s.is_finite() && s.abs() > 1e-6 {
            self.options.tex_scale = s;
        }
    }

    fn params(&self, material: String, lightmap_scale: i32, bounds: Aabb) -> PrimitiveParams {
        let o = &self.options;
        let mut params = PrimitiveParams {
            kind: o.kind,
            bounds,
            material,
            sides: o.sides,
            arch_wall_width: o.arch_wall_width,
            arch_arc_degrees: o.arch_arc,
            arch_start_angle: o.arch_start_angle,
            torus_rotation_arc: o.torus_arc,
            torus_rotation_start: o.torus_start_angle,
            torus_rotation_height: o.torus_height,
            up_axis: self.up_axis(),
            up_flip: self.options.reorient && self.draw3d.is_some_and(|d| d.sign < 0.0),
            arch_add_height: o.arch_add_height,
            torus_ring_width: o.torus_ring_width,
            torus_tube_sides: o.torus_tube_sides,
            tex_scale: o.tex_scale,
            lightmap_scale: if lightmap_scale > 0 { lightmap_scale } else { 16 },
            round_vertices: true,
            ..PrimitiveParams::default()
        };
        crate::block_structures::apply(o, &mut params);
        params
    }

    pub fn create(&mut self, state: &mut AppState, bounds: Aabb) -> Option<usize> {
        let material = crate::common::active_material(state);
        let params = self.params(material, state.settings.edit.default_lightmap_scale, bounds);
        let result: Result<usize, EditError> = match &self.options.create_as {
            CreateAs::World => state.doc.create_primitive(&params, Owner::World).map(|ids| ids.len()),
            CreateAs::Entity(class) => {
                let mut next = state.doc.peek_next_side_id();
                match make_primitive(&params, &mut || {
                    let id = next;
                    next = next.saturating_add(1);
                    id
                }) {
                    Ok(solids) => {
                        let n = solids.len();
                        let kv = fgd_defaults(state, class);
                        state.doc.create_brush_entity(class, &kv, solids).map(|_| n)
                    }
                    Err(e) => Err(EditError::from(e)),
                }
            }
        };
        match result {
            Ok(n) => {
                self.last_bounds = Some(bounds);
                state.runtime.brush.last_created = Some(bounds);
                state.log(
                    Severity::Info,
                    tf!("create.block.created", t!(primitive_key(self.options.kind)), fmt_size(&bounds)),
                );
                if !self.options.keep_active {
                    state.set_tool(ToolId::Select);
                }
                Some(n)
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("create.block.failed", e));
                None
            }
        }
    }

    pub fn confirm(&mut self, state: &mut AppState) -> bool {
        if let Some(d) = self.draw3d {
            let Some(b) = d.final_bounds() else { return false };
            let created = self.create(state, b).is_some();
            if created {
                self.draw3d = None;
            }
            return created;
        }
        let Some(b) = self.pending else { return false };
        if self.create(state, b).is_some() {
            self.pending = None;
            self.hover = None;
            true
        } else {
            false
        }
    }

    pub fn is_ready(&self) -> bool {
        self.pending.is_some() || self.draw3d.is_some_and(|d| d.final_bounds().is_some())
    }

    pub fn confirm_as(&mut self, state: &mut AppState, kind: Primitive) -> bool {
        let saved = std::mem::replace(&mut self.options.kind, kind);
        let created = self.confirm(state);
        self.options.kind = saved;
        created
    }

    fn current_box(&self) -> Option<Aabb> {
        self.draw3d.map(|d| d.bounds()).or(self.pending)
    }

    fn depth_range(&self, state: &AppState, d: usize) -> (f64, f64) {
        new_depth_range(state, state.runtime.brush.last_created.or(self.last_bounds), d)
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        self.ensure_init(state);
        if m.pos.is_none() && !m.any_button() {
            return ToolFlow::Ignored;
        }
        if view.is_2d() { self.mouse_2d(state, view, m) } else { self.mouse_3d(state, view, m) }
    }

    fn mouse_2d(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::new(state, view, m.modifiers);
        if m.hovering() {
            self.hover =
                self.pending.and_then(|b| hit_handle(view, &b, pos)).map(|h| (view.index, h, state.frame.index));
            return ToolFlow::Ignored;
        }
        if m.drag_started {
            let origin = m.origin().unwrap_or(pos);
            self.draw3d = None;
            match self.pending.and_then(|b| hit_handle(view, &b, origin).map(|h| (b, h))) {
                Some((b, handle)) => {
                    self.drag = Some(Drag::Edit {
                        view: view.index,
                        kind: view.kind,
                        handle,
                        start_box: b,
                        from: view.screen_to_2d(origin),
                    });
                }
                None => {
                    let start = snap.v2(view.screen_to_2d(origin));
                    let depth = self.depth_range(state, depth_axis(view.kind));
                    self.drag = Some(Drag::New { view: view.index, kind: view.kind, start, depth });
                    self.pending = None;
                    self.draw_up = depth_axis(view.kind);
                }
            }
        }
        if m.dragging || m.drag_stopped {
            match self.drag {
                Some(Drag::New { view: vi, kind, start, depth }) if vi == view.index => {
                    let end = snap.v2(view.screen_to_2d(pos));
                    self.pending = footprint_box(kind, start, end, depth);
                }
                Some(Drag::Edit { view: vi, kind, handle, start_box, from }) if vi == view.index => {
                    self.pending = Some(edit_box(&start_box, kind, handle, from, view.screen_to_2d(pos), &snap));
                    self.hover = Some((view.index, handle, state.frame.index));
                }
                _ => {}
            }
        }
        if m.drag_stopped {
            self.drag = None;
        }
        ToolFlow::Handled
    }

    fn mouse_3d(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::new(state, view, m.modifiers);
        let surface_snap = |surface: Option<ObjRef>| Snap::on_object(state, view, m.modifiers, surface);

        if let Some(d) = self.draw3d.as_mut()
            && d.view == view.index
            && let Phase3d::Height { anchor, base_height, .. } = d.phase
        {
            let h = height_from_mouse(view, d, anchor, pos, base_height, &surface_snap(d.surface));
            d.phase = Phase3d::Height { anchor, base_height, height: h };
            if m.activated() {
                if m.drag_started {
                    self.drag = Some(Drag::Swallow);
                }
                self.confirm(state);
                return ToolFlow::Handled;
            }
            return if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored };
        }

        if m.hovering() {
            self.hover_3d = pick_surface(state, view, pos)
                .map(|h| surface_snap(Some(ObjRef::Solid(h.solid))).on_plane(h.point, h.normal))
                .or_else(|| ground_point(view, pos).map(|p| DVec3::new(snap.v(p.x, 0), snap.v(p.y, 1), 0.0)))
                .map(|p| (p, state.frame.index));
            return ToolFlow::Ignored;
        }

        if m.drag_started {
            let origin = m.origin().unwrap_or(pos);
            self.hover_3d = None;
            let (ray_o, ray_d) = view.ray(origin);
            let start = match pick_surface(state, view, origin) {
                Some(hit) => {
                    let surface = Some(ObjRef::Solid(hit.solid));
                    let axis = crate::common::dominant_axis(hit.normal);
                    let aligned = hit.normal[axis].abs() > 0.999;
                    let level =
                        if aligned { clean(hit.point[axis]) } else { surface_snap(surface).v(hit.point[axis], axis) };
                    Some((axis, if hit.normal[axis] >= 0.0 { 1.0 } else { -1.0 }, level, surface))
                }
                None => ground_point(view, origin).map(|_| (2, if ray_o.z >= 0.0 { 1.0 } else { -1.0 }, 0.0, None)),
            };
            self.draw3d = start.and_then(|(axis, sign, level, surface)| {
                let mut d = Draw3d {
                    view: view.index,
                    axis,
                    sign,
                    level,
                    start: DVec3::ZERO,
                    end: DVec3::ZERO,
                    phase: Phase3d::Base,
                    surface,
                };
                let p = d.plane_point(ray_o, ray_d, &surface_snap(surface))?;
                d.start = p;
                d.end = p;
                Some(d)
            });
            if self.draw3d.is_some() {
                self.pending = None;
            }
        }
        if (m.dragging || m.drag_stopped)
            && let Some(d) = self.draw3d.as_mut()
            && d.view == view.index
            && d.phase == Phase3d::Base
        {
            let (o, dir) = view.ray(pos);
            if let Some(p) = d.plane_point(o, dir, &surface_snap(d.surface)) {
                d.end = p;
            }
        }
        if m.drag_stopped {
            if self.drag == Some(Drag::Swallow) {
                self.drag = None;
                return ToolFlow::Handled;
            }
            if let Some(d) = self.draw3d.as_mut()
                && d.phase == Phase3d::Base
            {
                if d.base_valid() {
                    let base_height = 0.0;
                    let step = surface_snap(d.surface).step_along(d.axis);
                    d.phase = Phase3d::Height { anchor: pos, base_height, height: step };
                } else {
                    self.draw3d = None;
                }
            }
        }
        ToolFlow::Handled
    }

    fn with_shape<R>(&self, b: Aabb, f: impl FnOnce(&ShapePreview) -> R) -> Option<R> {
        let mut cache = self.shape.try_borrow_mut().ok()?;
        let params = self.params(String::new(), 16, b);
        if cache.params.as_ref() != Some(&params) {
            let mut next = 1u32;
            let result = make_primitive(&params, &mut || {
                next = next.wrapping_add(1);
                next
            });
            cache.edges.clear();
            cache.solids.clear();
            cache.error = None;
            cache.projection = None;
            match result {
                Ok(solids) => {
                    for sides in &solids {
                        let geom = hp_geom::build_solid(sides);
                        cache.edges.extend(geom.edges());
                        cache.solids.push(geom);
                    }
                }
                Err(e) => cache.error = Some(e.to_string()),
            }
            cache.params = Some(params);
        }
        Some(f(&cache))
    }

    fn with_preview<R>(&self, bounds: Aabb, f: impl FnOnce(&Projection, Option<&str>, usize) -> R) -> Option<R> {
        self.with_shape(bounds, |_| ())?;
        let mut cache = self.shape.try_borrow_mut().ok()?;
        let cache = &mut *cache;
        let up = self.up_axis();
        let projection = cache.projection.get_or_insert_with(|| block_preview::project(&cache.solids, &bounds, up));
        Some(f(projection, cache.error.as_deref(), cache.solids.len()))
    }

    fn preview_bounds(&self, state: &AppState) -> (Aabb, bool) {
        if let Some(b) = self.current_box() {
            return (b, true);
        }
        let last =
            state.runtime.brush.last_created.or(self.last_bounds).filter(|b| b.size().min_element() > MIN_EXTENT);
        let fallback = Aabb::new(DVec3::new(-128.0, -128.0, 0.0), DVec3::new(128.0, 128.0, DEFAULT_DEPTH));
        (last.unwrap_or(fallback), false)
    }

    fn preview_style(&self, state: &AppState, detailed: bool) -> PreviewStyle {
        let o = &self.options;
        let rising = match o.kind {
            Primitive::Arch => o.arch_add_height != 0.0,
            Primitive::Torus => o.torus_height != 0.0,
            Primitive::Stairs | Primitive::SpiralStairs | Primitive::Ramp => true,
            _ => false,
        };
        PreviewStyle {
            background: hp_app::theme::rgb(state.settings.view.background_2d),
            detailed,
            rising,
            mark_first: matches!(o.kind, Primitive::Arch | Primitive::Torus),
        }
    }

    fn preview_ui(&mut self, ui: &mut Ui, state: &AppState) {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
        let resp = resp.on_hover_text(t!("create.block.preview_tip"));
        if resp.clicked() {
            self.preview_open = !self.preview_open;
        }
        if ui.is_rect_visible(rect) {
            let (bounds, _) = self.preview_bounds(state);
            let style = self.preview_style(state, false);
            let painter = ui.painter();
            self.with_preview(bounds, |p, error, _| block_preview::paint(painter, rect, p, error, style));
            let stroke = if self.preview_open {
                Stroke::new(1.0, hp_app::theme::accent())
            } else {
                ui.style().interact_selectable(&resp, false).bg_stroke
            };
            painter.rect_stroke(rect, 2, stroke, StrokeKind::Inside);
        }
        if !self.preview_open {
            return;
        }
        let mut open = true;
        hp_app::chrome::Window::new(tf!("create.block.preview_title", t!(primitive_key(self.options.kind))))
            .id(egui::Id::unique("hp_block_preview"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(Pos2::new(ui.ctx().content_rect().left() + 56.0, rect.bottom() + 48.0))
            .show(ui.ctx(), |ui| {
                let (bounds, drawn) = self.preview_bounds(state);
                ui.horizontal_top(|ui| {
                    let (canvas, _) = ui.allocate_exact_size(Vec2::splat(PREVIEW_SIZE), Sense::hover());
                    let style = self.preview_style(state, true);
                    let solids = self
                        .with_preview(bounds, |p, error, n| {
                            block_preview::paint(ui.painter(), canvas, p, error, style);
                            if error.is_some() { 0 } else { n }
                        })
                        .unwrap_or(0);
                    ui.vertical(|ui| {
                        ui.set_width(PREVIEW_SIDE_WIDTH);
                        egui::Grid::new("hp_block_preview_params").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                            self.params_ui(ui, true);
                        });
                        ui.add_space(6.0);
                        let size = tf!("create.block.preview_size", fmt_size(&bounds), solids);
                        ui.add(egui::Label::new(egui::RichText::new(size).weak()).wrap());
                        if !drawn {
                            let note = egui::RichText::new(t!("create.block.preview_no_box")).weak().italics();
                            ui.add(egui::Label::new(note).wrap());
                        }
                    });
                });
            });
        if !open {
            self.preview_open = false;
        }
    }

    fn paint_2d(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(b) = self.current_box() else { return };
        let Some(rect) = box_screen_rect(view, &b) else { return };
        let (Some((r, u, _)), size) = (view.kind.axes_2d(), b.size()) else { return };
        paint_label(
            painter,
            Pos2::new(rect.center().x, rect.top() - 4.0),
            Align2::CENTER_BOTTOM,
            &fmt_len(size[r]),
            COLOR_PENDING,
        );
        paint_label(
            painter,
            Pos2::new(rect.left() - 6.0, rect.center().y),
            Align2::RIGHT_CENTER,
            &fmt_len(size[u]),
            COLOR_PENDING,
        );
        if self.draw3d.is_none() {
            let hovered =
                self.hover.filter(|(vi, _, frame)| *vi == view.index && fresh(state, *frame)).map(|(_, h, _)| h);
            for h in BoxHandle::RESIZE {
                paint_handle(painter, h.screen_pos(rect), hovered == Some(h));
            }
        }
        let mut below = rect.bottom() + 6.0;
        if let Some(text) = self.count_label() {
            paint_label(painter, Pos2::new(rect.center().x, below), Align2::CENTER_TOP, &text, COLOR_SHAPE);
            below += 20.0;
        }
        if self.options.kind != Primitive::Block && self.with_shape(b, |s| s.error.is_some()).unwrap_or(false) {
            paint_label(
                painter,
                Pos2::new(rect.center().x, below),
                Align2::CENTER_TOP,
                t!("create.block.invalid"),
                hp_app::theme::VIEW_ERROR,
            );
        }
    }

    fn count_label(&self) -> Option<String> {
        let (count, value) = self.shape_count()?;
        Some(tf!(count.label_key(), value))
    }

    fn paint_3d(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(b) = self.current_box() else {
            if let Some((p, frame)) = self.hover_3d
                && fresh(state, frame)
                && let Some(s) = view.world_to_screen(p)
                && view.rect.contains(s)
            {
                painter.circle_stroke(s, 4.0, Stroke::new(1.5, COLOR_PENDING));
            }
            return;
        };
        let mut top = b.center();
        top.z = b.max.z;
        if let Some(s) = view.world_to_screen(top)
            && view.rect.contains(s)
        {
            paint_label(painter, s - Vec2::new(0.0, 8.0), Align2::CENTER_BOTTOM, &fmt_size(&b), COLOR_PENDING);
            if let Some(text) = self.count_label() {
                paint_label(painter, s - Vec2::new(0.0, 28.0), Align2::CENTER_BOTTOM, &text, COLOR_SHAPE);
            }
        }
    }
}

impl Tool for BlockTool {
    fn id(&self) -> ToolId {
        ToolId::Block
    }

    fn activate(&mut self, state: &mut AppState) {
        self.ensure_init(state);
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.cancel(state);
    }

    fn context_entries(&self, _state: &AppState) -> Vec<ToolMenuEntry> {
        if !self.is_ready() {
            return Vec::new();
        }
        let entry = |key: &str, id: u32| ToolMenuEntry { label: t!(key).to_owned(), id, enabled: true };
        vec![
            entry("create.block.ctx_create", CTX_CREATE),
            entry("create.block.ctx_cylinder", CTX_CYLINDER),
            entry("create.block.ctx_arch", CTX_ARCH),
        ]
    }

    fn context_command(&mut self, state: &mut AppState, id: u32) {
        match id {
            CTX_CREATE => {
                self.confirm(state);
            }
            CTX_CYLINDER => {
                self.confirm_as(state, Primitive::Cylinder);
            }
            CTX_ARCH => {
                self.confirm_as(state, Primitive::Arch);
            }
            _ => {}
        }
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn wheel(&mut self, _state: &mut AppState, _view: &ViewInfo, notches: f32, mods: egui::Modifiers) -> ToolFlow {
        if !notches.is_finite() || mods.ctrl || mods.command || mods.alt {
            return ToolFlow::Ignored;
        }
        if !self.is_drawing() || ShapeCount::of(&self.options).is_none() {
            self.wheel = 0.0;
            return ToolFlow::Ignored;
        }
        self.wheel += notches;
        let mut delta = 0i64;
        while self.wheel >= 1.0 {
            self.wheel -= 1.0;
            delta += 1;
        }
        while self.wheel <= -1.0 {
            self.wheel += 1.0;
            delta -= 1;
        }
        if delta != 0 {
            let step = if mods.shift { i64::from(WHEEL_FAST_STEP) } else { 1 };
            self.step_shape_count(delta * step);
        }
        ToolFlow::Handled
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.is_ready() && consume_plain_key(ctx, egui::Key::Enter) {
            self.confirm(state);
            return true;
        }
        let busy = self.pending.is_some() || self.draw3d.is_some() || self.drag.is_some();
        if busy && consume_plain_key(ctx, egui::Key::Escape) {
            return self.cancel(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let mut ov = Ov::new(out, view);
        if let Some(d) = self.draw3d
            && d.phase == Phase3d::Base
        {
            let b = d.bounds();
            let (i, j) = ((d.axis + 1) % 3, (d.axis + 2) % 3);
            let corner = |a: f64, c: f64| {
                let mut p = DVec3::ZERO;
                p[d.axis] = d.level;
                p[i] = a;
                p[j] = c;
                p
            };
            let quad = [
                corner(b.min[i], b.min[j]),
                corner(b.max[i], b.min[j]),
                corner(b.max[i], b.max[j]),
                corner(b.min[i], b.max[j]),
            ];
            ov.polygon_fill(&quad, rgba(COLOR_PENDING, 0.18));
            ov.polygon_outline(&quad, rgba(COLOR_PENDING, 1.0), true);
            return;
        }
        let Some(b) = self.current_box() else {
            if !view.is_2d()
                && let Some((p, frame)) = self.hover_3d
                && fresh(state, frame)
            {
                ov.point(p, rgba(COLOR_PENDING, 1.0), 6.0);
            }
            return;
        };
        if !view.is_2d() {
            ov.box_faces(&b, rgba(COLOR_PENDING, 0.10));
        }
        ov.box_edges(&b, rgba(COLOR_PENDING, 1.0), true);
        if self.options.kind != Primitive::Block {
            let color = rgba(COLOR_SHAPE, if view.is_2d() { 0.75 } else { 0.9 });
            self.with_shape(b, |s| {
                for e in &s.edges {
                    ov.xray_line(e[0], e[1], color);
                }
            });
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        if view.is_2d() {
            self.paint_2d(state, view, &painter);
        } else {
            self.paint_3d(state, view, &painter);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.ensure_init(state);
        ui.horizontal_wrapped(|ui| {
            for kind in Primitive::ALL {
                if primitive_button(ui, kind, self.options.kind == kind, t!(primitive_key(kind))).clicked() {
                    if kind == Primitive::Arch && self.options.kind != kind {
                        self.preview_open = true;
                    }
                    self.options.kind = kind;
                }
            }
            ui.separator();
            self.params_ui(ui, false);
            if has_preview(self.options.kind) {
                self.preview_ui(ui, state);
                ui.separator();
            }
            if self.options.kind != Primitive::Block
                && ui
                    .checkbox(&mut self.options.reorient, t!("create.block.reorient"))
                    .on_hover_text(t!("create.block.reorient_tip"))
                    .changed()
            {
                state.settings.brush.reorient_primitives = self.options.reorient;
                self.seen_reorient = Some(self.options.reorient);
                state.settings_dirty = true;
            }
            let scale =
                egui::DragValue::new(&mut self.options.tex_scale).speed(0.01).range(0.01..=64.0).max_decimals(3);
            param(ui, "create.block.tex_scale", "create.block.tex_scale_tip", scale);
            ui.separator();
            self.create_as_ui(ui, state);
            ui.separator();
            material_chip(ui, state);
            ui.separator();
            if hp_app::widgets::icon_toggle(ui, "📌", self.options.keep_active, t!("create.block.keep_active_tip"))
                .clicked()
            {
                self.options.keep_active = !self.options.keep_active;
            }
            let ready = self.is_ready();
            let button = egui::Button::new(t!("create.block.create"));
            if ui
                .add_enabled(ready, button)
                .on_hover_text(tf!("create.block.create_tip", key_label(egui::Key::Enter)))
                .clicked()
            {
                self.confirm(state);
            }
        });
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if view.is_2d()
            && self.draw3d.is_none()
            && let Some(h) = self.pending.and_then(|b| hit_handle(view, &b, pos))
        {
            return Some(h.cursor());
        }
        if let Some(Drag::Edit { handle, .. }) = self.drag {
            return Some(handle.cursor());
        }
        Some(if self.draw3d.is_some_and(|d| d.phase != Phase3d::Base) {
            CursorIcon::ResizeVertical
        } else {
            CursorIcon::Crosshair
        })
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        let hint = match self.draw3d.map(|d| d.phase) {
            Some(Phase3d::Base) => t!("create.block.hint_base").to_owned(),
            Some(Phase3d::Height { .. }) => tf!("create.block.hint_height", key_label(egui::Key::Escape)),
            None if self.pending.is_some() => tf!(
                "create.block.hint_pending",
                t!(primitive_key(self.options.kind)),
                key_label(egui::Key::Enter),
                key_label(egui::Key::Escape)
            ),
            None => t!("create.block.hint_idle").to_owned(),
        };
        match ShapeCount::of(&self.options) {
            Some(count) => Some(format!("{hint} {}", tf!(count.hint_key(), WHEEL_FAST_STEP))),
            None => Some(hint),
        }
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        let had = self.pending.is_some() || self.draw3d.is_some() || self.drag.is_some();
        self.pending = None;
        self.draw3d = None;
        self.drag = None;
        self.hover = None;
        self.hover_3d = None;
        self.wheel = 0.0;
        had
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        self.ensure_init(state);
        let o = &mut self.options;
        let flag = match value.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        let num = value.parse::<f64>().ok().filter(|v| v.is_finite());
        let count = value.parse::<u32>().ok().filter(|v| (3..=MAX_PRIMITIVE_SIDES).contains(v));
        let set = |slot: &mut f64, v: Option<f64>| v.map(|v| *slot = v).is_some();
        match key {
            "kind" => match Primitive::ALL
                .into_iter()
                .find(|k| format!("{k:?}").eq_ignore_ascii_case(value) || k.id().eq_ignore_ascii_case(value))
            {
                Some(k) => {
                    o.kind = k;
                    true
                }
                None => false,
            },
            "sides" => count.map(|v| o.sides = v).is_some(),
            "tube_sides" => count.map(|v| o.torus_tube_sides = v).is_some(),
            "wall" => set(&mut o.arch_wall_width, num.filter(|v| *v >= 1.0)),
            "arc" => set(&mut o.arch_arc, num.filter(|v| (1.0..=360.0).contains(v))),
            "start" => set(&mut o.arch_start_angle, num),
            "rise" => set(&mut o.arch_add_height, num),
            "ring" => set(&mut o.torus_ring_width, num.filter(|v| *v >= 1.0)),
            "torus_arc" => set(&mut o.torus_arc, num.filter(|v| *v >= 1.0)),
            "torus_start" => set(&mut o.torus_start_angle, num),
            "torus_rise" => set(&mut o.torus_height, num),
            "reorient" => match flag {
                Some(v) => {
                    o.reorient = v;
                    self.seen_reorient = Some(v);
                    state.settings.brush.reorient_primitives = v;
                    state.settings_dirty = true;
                    true
                }
                None => false,
            },
            "preview" => flag.map(|v| self.preview_open = v).is_some(),
            "tex_scale" => set(&mut o.tex_scale, num.filter(|v| (0.01..=64.0).contains(v))),
            "keep_active" => flag.map(|v| o.keep_active = v).is_some(),
            "create_as" => {
                let v = value.trim();
                if v.eq_ignore_ascii_case("world") {
                    o.create_as = CreateAs::World;
                    true
                } else if !v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    o.create_as = CreateAs::Entity(v.to_ascii_lowercase());
                    true
                } else {
                    false
                }
            }
            _ => crate::block_structures::set_option(o, key, value).unwrap_or(false),
        }
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        use hp_app::tool::ToolOption;
        let o = &self.options;
        let num = |v: f64| {
            let s = format!("{v:.4}");
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        };
        let on = |b: bool| if b { "on" } else { "off" };
        vec![
            ToolOption::new(
                "kind",
                format!("{:?}", o.kind).to_ascii_lowercase(),
                "primitive: block, wedge, cylinder, spike, sphere, arch, torus, stairs, spiral_stairs, ramp, pillar or doorway",
            ),
            ToolOption::new(
                "sides",
                o.sides.to_string(),
                "sides of round primitives (3 to 128; the mouse wheel changes it while a shape is drawn)",
            ),
            ToolOption::new("wall", num(o.arch_wall_width), "arch wall width in units (1 or more)"),
            ToolOption::new("arc", num(o.arch_arc), "arch arc in degrees (1 to 360)"),
            ToolOption::new("start", num(o.arch_start_angle), "arch start angle in degrees"),
            ToolOption::new("rise", num(o.arch_add_height), "arch added height in units"),
            ToolOption::new("tube_sides", o.torus_tube_sides.to_string(), "torus tube sides (3 to 128)"),
            ToolOption::new("ring", num(o.torus_ring_width), "torus ring width in units (1 or more)"),
            ToolOption::new(
                "torus_arc",
                num(o.torus_arc),
                "torus sweep in degrees (over 360 for a spiral with a rise)",
            ),
            ToolOption::new("torus_start", num(o.torus_start_angle), "torus start angle in degrees"),
            ToolOption::new("torus_rise", num(o.torus_height), "height gained over the torus sweep in units"),
            ToolOption::new(
                "reorient",
                on(o.reorient),
                "on: primitives drawn in the front or side view or on a wall stand along the view depth (saved)",
            ),
            ToolOption::new(
                "preview",
                on(self.preview_open),
                "on: the large shape preview window with the parameters (opens by itself for the arch)",
            ),
            ToolOption::new("tex_scale", num(o.tex_scale), "texture scale of the new faces (0.01 to 64)"),
            ToolOption::new(
                "create_as",
                match &o.create_as {
                    CreateAs::World => "world".to_owned(),
                    CreateAs::Entity(c) => c.clone(),
                },
                "world, or a brush entity class (func_detail, trigger_multiple...)",
            ),
            ToolOption::new("keep_active", on(o.keep_active), "on: stay in the block tool after creating"),
        ]
        .into_iter()
        .chain(crate::block_structures::option_values(o, &num))
        .collect()
    }
}

impl BlockTool {
    fn params_ui(&mut self, ui: &mut Ui, grid: bool) {
        let param = |ui: &mut Ui, label: &str, tip: &str, value: egui::DragValue<'_>| {
            param(ui, label, tip, value);
            if grid {
                ui.end_row();
            }
        };
        let o = &mut self.options;
        if is_round(o.kind) {
            param(ui, "create.block.sides", "create.block.sides_tip", sides(&mut o.sides));
        }
        if o.kind == Primitive::Arch {
            param(ui, "create.block.wall_width", "create.block.wall_width_tip", length(&mut o.arch_wall_width));
        }
        if o.kind == Primitive::Torus {
            param(ui, "create.block.tube_sides", "create.block.tube_sides_tip", sides(&mut o.torus_tube_sides));
            param(ui, "create.block.ring_width", "create.block.ring_width_tip", length(&mut o.torus_ring_width));
        }
        if o.kind == Primitive::Arch {
            let arc = egui::DragValue::new(&mut o.arch_arc).range(1.0..=360.0).speed(1.0).suffix("°");
            param(ui, "create.block.arc", "create.block.arc_tip", arc);
            let start = egui::DragValue::new(&mut o.arch_start_angle).range(-360.0..=360.0).speed(1.0).suffix("°");
            param(ui, "create.block.start_angle", "create.block.start_angle_tip", start);
        }
        if o.kind == Primitive::Torus {
            let max_arc = if o.torus_height == 0.0 { 360.0 } else { hp_geom::MAX_SPIRAL_ARC };
            let arc = egui::DragValue::new(&mut o.torus_arc).range(1.0..=max_arc).speed(1.0).suffix("°");
            param(ui, "create.block.arc", "create.block.torus_arc_tip", arc);
            let start = egui::DragValue::new(&mut o.torus_start_angle).range(-360.0..=360.0).speed(1.0).suffix("°");
            param(ui, "create.block.start_angle", "create.block.start_angle_tip", start);
            let rise = egui::DragValue::new(&mut o.torus_height).range(-16384.0..=16384.0).speed(1.0);
            param(ui, "create.block.torus_height", "create.block.torus_height_tip", rise);
            if o.torus_height == 0.0 {
                o.torus_arc = o.torus_arc.min(360.0);
            }
        }
        crate::block_structures::params_ui(ui, o, grid);
        if o.kind == Primitive::Arch {
            let rise = egui::DragValue::new(&mut o.arch_add_height).range(-4096.0..=4096.0).speed(1.0);
            param(ui, "create.block.add_height", "create.block.add_height_tip", rise);
        }
    }

    fn create_as_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        create_as_ui(ui, state, &mut self.picker, &mut self.options.create_as, "block_create_as");
    }
}

pub(crate) fn create_as_ui(
    ui: &mut Ui,
    state: &mut AppState,
    picker: &mut ClassPicker,
    create_as: &mut CreateAs,
    id_salt: &str,
) {
    let fixed = [
        (WORLD_ID.to_owned(), t!("create.block.world").to_owned()),
        ("func_detail".to_owned(), "func_detail".to_owned()),
    ];
    let current = match create_as {
        CreateAs::World => t!("create.block.world").to_owned(),
        CreateAs::Entity(c) => c.clone(),
    };
    let generation = state.assets.generation();
    let fgd = state.assets.fgd();
    let classes: &[String] = fgd.map(|f| f.solid_classes()).unwrap_or(&[]);
    let src = PickerSource { classes, recent: &[], fixed: &fixed, fgd, generation };
    if let Some(v) = picker.show(ui, id_salt, &current, t!("create.block.create_as_tip"), &src) {
        *create_as = if v == WORLD_ID { CreateAs::World } else { CreateAs::Entity(v) };
    }
}

fn is_round(kind: Primitive) -> bool {
    matches!(
        kind,
        Primitive::Cylinder
            | Primitive::Spike
            | Primitive::Sphere
            | Primitive::Arch
            | Primitive::Torus
            | Primitive::Pillar
    )
}

fn has_preview(kind: Primitive) -> bool {
    is_round(kind) || crate::block_structures::is_structure(kind)
}

const PREVIEW_SIZE: f32 = 220.0;
const PREVIEW_SIDE_WIDTH: f32 = 150.0;

fn sides(v: &mut u32) -> egui::DragValue<'_> {
    egui::DragValue::new(v).range(3..=MAX_PRIMITIVE_SIDES).speed(0.1)
}

fn length(v: &mut f64) -> egui::DragValue<'_> {
    egui::DragValue::new(v).range(1.0..=16384.0).speed(1.0)
}

pub(crate) fn param(ui: &mut Ui, label: &str, tip: &str, value: impl egui::Widget) {
    let tip = t!(tip);
    ui.label(t!(label)).on_hover_text(tip);
    ui.add(value).on_hover_text(tip);
}

fn primitive_button(ui: &mut Ui, kind: Primitive, selected: bool, tooltip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(28.0, 26.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&resp, selected);
        let fill = if selected || resp.hovered() { visuals.weak_bg_fill } else { egui::Color32::TRANSPARENT };
        ui.painter().rect(rect, visuals.corner_radius, fill, visuals.bg_stroke, StrokeKind::Inside);
        let color = if selected { hp_app::theme::accent() } else { visuals.fg_stroke.color };
        paint_primitive_icon(ui.painter(), rect.shrink(6.0), kind, color);
    }
    resp.on_hover_text(tooltip)
}

fn arc_points(c: Pos2, rx: f32, ry: f32, from: f32, to: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|k| {
            let a = (from + (to - from) * k as f32 / n as f32).to_radians();
            Pos2::new(c.x + rx * a.cos(), c.y - ry * a.sin())
        })
        .collect()
}

fn paint_primitive_icon(painter: &egui::Painter, r: Rect, kind: Primitive, color: egui::Color32) {
    let stroke = Stroke::new(1.3, color);
    let p = |x: f32, y: f32| Pos2::new(r.left() + x * r.width(), r.top() + y * r.height());
    let poly = |pts: Vec<Pos2>| Shape::closed_line(pts, stroke);
    let line = |pts: Vec<Pos2>| Shape::line(pts, stroke);
    if crate::block_structures::paint_icon(painter, r, kind, color) {
        return;
    }
    let mut shapes = Vec::new();
    match kind {
        Primitive::Block => {
            shapes.push(poly(vec![p(0.0, 0.3), p(0.7, 0.3), p(0.7, 1.0), p(0.0, 1.0)]));
            shapes.push(line(vec![p(0.0, 0.3), p(0.3, 0.0), p(1.0, 0.0), p(1.0, 0.7), p(0.7, 1.0)]));
            shapes.push(line(vec![p(0.7, 0.3), p(1.0, 0.0)]));
        }
        Primitive::Wedge => {
            shapes.push(poly(vec![p(0.0, 1.0), p(0.7, 1.0), p(0.7, 0.3)]));
            shapes.push(line(vec![p(0.7, 0.3), p(1.0, 0.0), p(1.0, 0.7), p(0.7, 1.0)]));
            shapes.push(line(vec![p(0.0, 1.0), p(0.3, 0.7), p(1.0, 0.0)]));
        }
        Primitive::Cylinder => {
            let c_top = p(0.5, 0.15);
            let c_bot = p(0.5, 0.85);
            let (rx, ry) = (r.width() * 0.45, r.height() * 0.15);
            shapes.push(poly(arc_points(c_top, rx, ry, 0.0, 360.0, 24)));
            shapes.push(line(arc_points(c_bot, rx, ry, 180.0, 360.0, 12)));
            shapes.push(line(vec![Pos2::new(c_top.x - rx, c_top.y), Pos2::new(c_bot.x - rx, c_bot.y)]));
            shapes.push(line(vec![Pos2::new(c_top.x + rx, c_top.y), Pos2::new(c_bot.x + rx, c_bot.y)]));
        }
        Primitive::Spike => {
            let c = p(0.5, 0.85);
            let (rx, ry) = (r.width() * 0.45, r.height() * 0.15);
            shapes.push(poly(arc_points(c, rx, ry, 0.0, 360.0, 24)));
            shapes.push(line(vec![Pos2::new(c.x - rx, c.y), p(0.5, 0.0), Pos2::new(c.x + rx, c.y)]));
        }
        Primitive::Sphere => {
            let c = r.center();
            let rad = r.width().min(r.height()) * 0.5;
            shapes.push(poly(arc_points(c, rad, rad, 0.0, 360.0, 32)));
            shapes.push(line(arc_points(c, rad, rad * 0.32, 180.0, 360.0, 16)));
        }
        Primitive::Arch => {
            let c = p(0.5, 0.95);
            let (ro, ri) = (r.width() * 0.5, r.width() * 0.22);
            let mut pts = arc_points(c, ro, ro * 1.6, 0.0, 180.0, 18);
            let mut inner = arc_points(c, ri, ri * 1.6, 0.0, 180.0, 12);
            inner.reverse();
            pts.extend(inner);
            shapes.push(poly(pts));
        }
        Primitive::Torus => {
            let c = r.center();
            shapes.push(poly(arc_points(c, r.width() * 0.5, r.height() * 0.36, 0.0, 360.0, 32)));
            shapes.push(poly(arc_points(c, r.width() * 0.2, r.height() * 0.12, 0.0, 360.0, 20)));
        }
        _ => {}
    }
    painter.extend(shapes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{info_2d, info_3d_looking_down, settle};

    fn b(min: [f64; 3], max: [f64; 3]) -> Aabb {
        Aabb::new(DVec3::from_array(min), DVec3::from_array(max))
    }

    #[test]
    fn footprint_and_depth_memory() {
        let mut state = settle();
        state.doc.clear_selection();
        let mut tool = BlockTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        let a = top.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).expect("a");
        let c = top.world_to_screen(DVec3::new(62.0, 33.0, 0.0)).expect("c");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &top, &Mouse::drag(a, c));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(a, c));
        assert_eq!(tool.pending_box(), Some(b([0.0, 0.0, 0.0], [64.0, 32.0, 64.0])));
        assert!(tool.confirm(&mut state));
        assert_eq!(tool.last_bounds(), Some(b([0.0, 0.0, 0.0], [64.0, 32.0, 64.0])));
        assert!(tool.pending_box().is_none());
        let front = info_2d(ViewKind::Front, 1.0);
        let a = front.world_to_screen(DVec3::new(0.0, 128.0, 0.0)).expect("a");
        let c = front.world_to_screen(DVec3::new(0.0, 160.0, 48.0)).expect("c");
        tool.handle_mouse(&mut state, &front, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &front, &Mouse::drag_end(a, c));
        assert_eq!(tool.pending_box(), Some(b([0.0, 128.0, 0.0], [64.0, 160.0, 48.0])));
        let far = front.world_to_screen(DVec3::new(0.0, -200.0, -96.0)).expect("far");
        tool.handle_mouse(&mut state, &front, &Mouse::drag_start(far, far));
        tool.handle_mouse(&mut state, &front, &Mouse::drag_end(far, far + Vec2::new(2.0, 0.0)));
        assert!(tool.pending_box().is_none());
    }

    #[test]
    fn drawing_snaps_to_the_grid_of_its_view() {
        let mut state = settle();
        state.doc.clear_selection();
        state.set_grid_mode(hp_app::settings::GridMode::PerView);
        state.settings.grid.per_view.top.size = 12.0;
        state.settings.grid.per_view.front.size = 5.0;
        state.settings.grid.per_view.front.origin = [0.0, 1.0, 0.0];
        let mut tool = BlockTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        let a = top.world_to_screen(DVec3::new(1.0, -2.0, 0.0)).expect("a");
        let c = top.world_to_screen(DVec3::new(50.0, 31.0, 0.0)).expect("c");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(a, c));
        let p = tool.pending_box().expect("box");
        assert_eq!((p.min.x, p.min.y, p.max.x, p.max.y), (0.0, 0.0, 48.0, 36.0), "the 12 grid of the top view");
        tool.pending = None;
        let front = info_2d(ViewKind::Front, 1.0);
        let a = front.world_to_screen(DVec3::new(0.0, 2.0, 1.0)).expect("a");
        let c = front.world_to_screen(DVec3::new(0.0, 19.0, 12.0)).expect("c");
        tool.handle_mouse(&mut state, &front, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &front, &Mouse::drag_end(a, c));
        let p = tool.pending_box().expect("box");
        assert_eq!((p.min.y, p.min.z, p.max.y, p.max.z), (1.0, 0.0, 21.0, 10.0), "the 5 grid of the front view");
    }

    #[test]
    fn drawing_snaps_to_a_rotated_custom_grid() {
        let mut state = settle();
        state.doc.clear_selection();
        let spec = hp_app::settings::GridSpec {
            name: "R".into(),
            step: [16.0, 8.0, 16.0],
            angles: [0.0, 45.0, 0.0],
            ..Default::default()
        };
        state.put_custom_grid("", spec.clone(), true, false).unwrap();
        state.use_custom_grid("R").unwrap();
        let frame = hp_app::grid::GridFrame::from_spec(&spec, 16.0);
        let mut tool = BlockTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        let a = top.world_to_screen(DVec3::new(3.0, 1.0, 0.0)).expect("a");
        let c = top.world_to_screen(DVec3::new(41.0, 70.0, 0.0)).expect("c");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(a, c));
        let p = tool.pending_box().expect("box");
        for corner in [DVec3::new(p.min.x, p.min.y, 0.0), DVec3::new(p.max.x, p.max.y, 0.0)] {
            let l = frame.to_local(corner) / frame.step;
            assert!((l.x - l.x.round()).abs() < 1e-6 && (l.y - l.y.round()).abs() < 1e-6, "{corner} -> {l}");
        }
        let mut m = Mouse::drag_start(a, a);
        m.modifiers.alt = true;
        tool.pending = None;
        tool.handle_mouse(&mut state, &top, &m);
        let mut m = Mouse::drag_end(a, c);
        m.modifiers.alt = true;
        tool.handle_mouse(&mut state, &top, &m);
        assert_eq!(tool.pending_box().map(|b| b.min.x), Some(3.0));
    }

    #[test]
    fn depth_from_the_selected_floor() {
        let mut state = settle();
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(2)]);
        let tool = BlockTool::new();
        assert_eq!(tool.depth_range(&state, 2), (-16.0, 0.0));
        state.doc.clear_selection();
        assert_eq!(tool.depth_range(&state, 0), (0.0, 64.0));
    }

    fn draw(tool: &mut BlockTool, state: &mut AppState, kind: ViewKind, from: DVec2, to: DVec2) -> Option<Aabb> {
        let view = info_2d(kind, 1.0);
        let a = view.world_to_screen(kind.from_2d(from, 0.0)).expect("a");
        let c = view.world_to_screen(kind.from_2d(to, 0.0)).expect("c");
        tool.handle_mouse(state, &view, &Mouse::drag_start(a, a));
        tool.handle_mouse(state, &view, &Mouse::drag(a, c));
        tool.handle_mouse(state, &view, &Mouse::drag_end(a, c));
        tool.pending_box()
    }

    #[test]
    fn depth_comes_from_the_selection_in_every_view() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        tool.create(&mut state, b([0.0, 0.0, 128.0], [64.0, 96.0, 256.0])).expect("reference");
        let reference = state.doc.selected_solid_ids()[0];
        tool.create(&mut state, b([512.0, 512.0, 0.0], [576.0, 576.0, 32.0])).expect("other");
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(reference)]);
        let top = draw(&mut tool, &mut state, ViewKind::Top, DVec2::new(-256.0, -256.0), DVec2::new(-128.0, -192.0));
        assert_eq!(top, Some(b([-256.0, -256.0, 128.0], [-128.0, -192.0, 256.0])));
        let front = draw(&mut tool, &mut state, ViewKind::Front, DVec2::new(256.0, 0.0), DVec2::new(320.0, 64.0));
        assert_eq!(front, Some(b([0.0, 256.0, 0.0], [64.0, 320.0, 64.0])));
        let side = draw(&mut tool, &mut state, ViewKind::Side, DVec2::new(256.0, 0.0), DVec2::new(320.0, 64.0));
        assert_eq!(side, Some(b([256.0, 0.0, 0.0], [320.0, 96.0, 64.0])));
        tool.options.kind = Primitive::Arch;
        let arch = draw(&mut tool, &mut state, ViewKind::Top, DVec2::new(-256.0, -256.0), DVec2::new(0.0, 0.0));
        assert_eq!(arch, Some(b([-256.0, -256.0, 128.0], [0.0, 0.0, 256.0])));
        assert!(tool.confirm(&mut state));
        let next = draw(&mut tool, &mut state, ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(64.0, 64.0));
        assert_eq!(next.map(|b| (b.min.z, b.max.z)), Some((128.0, 256.0)));
        tool.cancel(&mut state);
        state.doc.clear_selection();
        tool.options.kind = Primitive::Block;
        tool.create(&mut state, b([0.0, 0.0, 32.0], [16.0, 16.0, 48.0])).expect("last");
        state.doc.clear_selection();
        let free = draw(&mut tool, &mut state, ViewKind::Top, DVec2::new(0.0, 0.0), DVec2::new(64.0, 64.0));
        assert_eq!(free.map(|b| (b.min.z, b.max.z)), Some((32.0, 48.0)));
    }

    #[test]
    fn several_selected_objects_give_their_union() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        tool.create(&mut state, b([0.0, 0.0, 128.0], [64.0, 64.0, 256.0])).expect("high");
        let high = state.doc.selected_solid_ids()[0];
        tool.create(&mut state, b([256.0, 0.0, 32.0], [320.0, 64.0, 96.0])).expect("low");
        let low = state.doc.selected_solid_ids()[0];
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(high), hp_editor::ObjRef::Solid(low)]);
        let top = draw(&mut tool, &mut state, ViewKind::Top, DVec2::new(-64.0, -64.0), DVec2::new(0.0, 0.0));
        assert_eq!(top.map(|b| (b.min.z, b.max.z)), Some((32.0, 256.0)));
        let front = draw(&mut tool, &mut state, ViewKind::Front, DVec2::new(-64.0, 0.0), DVec2::new(0.0, 64.0));
        assert_eq!(front.map(|b| (b.min.x, b.max.x)), Some((0.0, 320.0)));
    }

    #[test]
    fn arch_preview_follows_the_options() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        assert!(tool.set_option(&mut state, "kind", "arch"));
        assert!(tool.set_option(&mut state, "sides", "8"));
        assert!(tool.set_option(&mut state, "arc", "180"));
        assert!(tool.set_option(&mut state, "wall", "32"));
        assert!(!tool.set_option(&mut state, "sides", "2"));
        assert!(!tool.set_option(&mut state, "nope", "1"));
        let (bounds, drawn) = tool.preview_bounds(&state);
        assert!(!drawn);
        assert_eq!(tool.with_preview(bounds, |p, error, n| (p.hulls.len(), error.is_none(), n)), Some((8, true, 8)));
        assert!(tool.set_option(&mut state, "sides", "16"));
        assert!(tool.set_option(&mut state, "arc", "360"));
        assert_eq!(tool.with_preview(bounds, |p, _, _| p.hulls.len()), Some(16));
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [512.0, 256.0, 32.0])));
        let (bounds, drawn) = tool.preview_bounds(&state);
        assert!(drawn);
        assert_eq!(tool.with_preview(bounds, |p, _, _| p.footprint), Some((DVec2::ZERO, DVec2::new(512.0, 256.0))));
        assert!(tool.with_shape(bounds, |s| !s.edges.is_empty()).unwrap_or(false));
        assert!(tool.set_option(&mut state, "wall", "10000"));
        assert_eq!(tool.with_preview(bounds, |_, error, _| error.is_none()), Some(true));
        assert!(tool.set_option(&mut state, "preview", "on"));
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        assert!(tool.preview_open);
        tool.preview_open = false;
        tool.options.kind = Primitive::Block;
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        assert!(!tool.preview_open);
    }

    #[test]
    fn dragging_handles_of_the_pending_box() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])));
        let side = info_2d(ViewKind::Side, 1.0);
        let from = side.world_to_screen(DVec3::new(32.0, 0.0, 64.0)).expect("from");
        let to = side.world_to_screen(DVec3::new(32.0, 0.0, 100.0)).expect("to");
        tool.handle_mouse(&mut state, &side, &Mouse::hover(from));
        assert!(tool.cursor(&state, &side, from) == Some(CursorIcon::ResizeVertical));
        tool.handle_mouse(&mut state, &side, &Mouse::drag_start(from, from));
        tool.handle_mouse(&mut state, &side, &Mouse::drag(from, to));
        tool.handle_mouse(&mut state, &side, &Mouse::drag_end(from, to));
        assert_eq!(tool.pending_box(), Some(b([0.0, 0.0, 0.0], [64.0, 64.0, 96.0])));
        let top = info_2d(ViewKind::Top, 1.0);
        let from = top.world_to_screen(DVec3::new(32.0, 32.0, 0.0)).expect("from");
        let to = top.world_to_screen(DVec3::new(64.0, 32.0, 0.0)).expect("to");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(from, from));
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(from, to));
        assert_eq!(tool.pending_box(), Some(b([32.0, 0.0, 0.0], [96.0, 64.0, 96.0])));
        assert!(tool.cancel(&mut state));
        assert!(tool.pending_box().is_none());
        assert!(!tool.cancel(&mut state));
    }

    #[test]
    fn every_primitive_creates_valid_solids_in_one_undo_step() {
        for kind in Primitive::ALL {
            let mut state = settle();
            let before = state.doc.stats().solids;
            let (undo_before, _) = state.doc.history_len();
            let mut tool = BlockTool::new();
            tool.options.kind = kind;
            tool.options.sides = 8;
            let n = tool.create(&mut state, b([0.0, 0.0, 0.0], [128.0, 128.0, 64.0])).expect("created");
            let stats = state.doc.stats();
            assert_eq!(stats.solids, before + n, "{kind:?}");
            assert_eq!(stats.invalid_solids, 0, "{kind:?}");
            assert_eq!(state.doc.history_len().0, undo_before + 1, "{kind:?}: one undo step");
            let selected = state.doc.selected_solid_ids();
            assert_eq!(selected.len(), n, "{kind:?}");
            for id in &selected {
                let node = state.doc.scene().solid(*id).expect("solid");
                assert!(hp_geom::validate_solid(&node.solid.sides).is_ok());
                assert!(node.solid.sides.iter().all(|s| s.material == "dev/dev_measuregeneric01b"));
            }
            assert!(state.doc.undo());
            assert_eq!(state.doc.stats().solids, before, "{kind:?}");
        }
    }

    fn axis_faces(state: &AppState, axis: usize) -> usize {
        let id = state.doc.selected_solid_ids()[0];
        let node = state.doc.scene().solid(id).expect("solid");
        node.geom.faces.iter().filter(|f| f.plane.normal[axis].abs() > 0.999).count()
    }

    #[test]
    fn primitives_follow_the_drawing_view() {
        for reorient in [true, false] {
            let mut state = settle();
            state.doc.clear_selection();
            let mut tool = BlockTool::new();
            tool.handle_mouse(&mut state, &info_2d(ViewKind::Top, 1.0), &Mouse::hover(Pos2::new(1.0, 1.0)));
            tool.options.kind = Primitive::Cylinder;
            tool.options.sides = 8;
            tool.options.reorient = reorient;
            let front = info_2d(ViewKind::Front, 1.0);
            let a = front.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).expect("a");
            let c = front.world_to_screen(DVec3::new(0.0, 64.0, 64.0)).expect("c");
            tool.handle_mouse(&mut state, &front, &Mouse::drag_start(a, a));
            tool.handle_mouse(&mut state, &front, &Mouse::drag_end(a, c));
            assert_eq!(tool.up_axis(), if reorient { 0 } else { 2 });
            assert!(tool.confirm(&mut state));
            assert_eq!(axis_faces(&state, if reorient { 0 } else { 2 }), 2, "reorient {reorient}");
        }
        let mut state = settle();
        state.settings.brush.reorient_primitives = false;
        let mut tool = BlockTool::new();
        tool.activate(&mut state);
        assert!(!tool.options.reorient);
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])));
        assert_eq!(tool.up_axis(), 2);
    }

    #[test]
    fn context_menu_creates_the_pending_box() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        assert!(tool.context_entries(&state).is_empty(), "nothing to create");
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])));
        let entries = tool.context_entries(&state);
        assert_eq!(entries.iter().map(|e| e.id).collect::<Vec<_>>(), vec![CTX_CREATE, CTX_CYLINDER, CTX_ARCH]);
        assert!(entries.iter().all(|e| e.enabled && !e.label.is_empty()));
        let before = state.doc.stats().solids;
        tool.options.sides = 8;
        tool.context_command(&mut state, CTX_CYLINDER);
        assert_eq!(state.doc.stats().solids, before + 1);
        assert_eq!(axis_faces(&state, 2), 2);
        assert_eq!(tool.options.kind, Primitive::Block);
        assert!(tool.pending_box().is_none());
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [128.0, 128.0, 32.0])));
        tool.context_command(&mut state, CTX_ARCH);
        assert_eq!(state.doc.stats().solids, before + 1 + 8);
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])));
        tool.context_command(&mut state, CTX_CREATE);
        assert_eq!(state.doc.stats().solids, before + 10);
        tool.context_command(&mut state, 99);
    }

    #[test]
    fn spiral_torus_from_the_options() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        tool.options.kind = Primitive::Torus;
        tool.options.sides = 12;
        tool.options.torus_arc = 720.0;
        tool.options.torus_height = 128.0;
        let n = tool.create(&mut state, b([-128.0, -128.0, 0.0], [128.0, 128.0, 32.0])).expect("spiral");
        assert_eq!(n, 12);
        let top = state
            .doc
            .selected_solid_ids()
            .iter()
            .filter_map(|id| state.doc.scene().solid(*id))
            .map(|n| n.bounds.max.z)
            .fold(f64::NEG_INFINITY, f64::max);
        let expected = 16.0 + 16.0 * 112.5f64.to_radians().sin() + 128.0;
        assert!((top - expected).abs() < 1e-6, "{top}");
        tool.options.torus_height = 0.0;
        assert!(tool.create(&mut state, b([-128.0, -128.0, 0.0], [128.0, 128.0, 32.0])).is_none());
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        assert_eq!(tool.options.torus_arc, 360.0, "the options bar clamps it");
    }

    #[test]
    fn create_as_func_detail() {
        let mut state = settle();
        let entities = state.doc.stats().entities;
        let mut tool = BlockTool::new();
        tool.options.create_as = CreateAs::Entity("func_detail".to_owned());
        tool.options.kind = Primitive::Cylinder;
        tool.create(&mut state, b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])).expect("created");
        assert_eq!(state.doc.stats().entities, entities + 1);
        let ents = state.doc.selected_entity_ids();
        assert_eq!(ents.len(), 1);
        let e = state.doc.scene().entity(ents[0]).expect("entity");
        assert_eq!(e.entity.classname, "func_detail");
        assert_eq!(state.doc.scene().entity_solid_count(ents[0]), 1);
        assert!(state.doc.undo());
        assert_eq!(state.doc.stats().entities, entities);
    }

    #[test]
    fn invalid_parameters_are_reported() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        tool.options.kind = Primitive::Torus;
        tool.options.torus_ring_width = 10_000.0;
        assert!(tool.create(&mut state, b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])).is_none());
        assert!(!state.console.is_empty());
        assert!(tool.with_shape(b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0]), |s| s.error.is_some()).unwrap_or(false));
    }

    #[test]
    fn keep_active_off_switches_to_selection() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        tool.options.keep_active = false;
        tool.create(&mut state, b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])).expect("created");
        assert_eq!(state.requested_tool, Some(ToolId::Select));
    }

    #[test]
    fn drawing_on_the_floor_in_3d() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 512.0));
        let a = view.world_to_screen(DVec3::new(-30.0, -30.0, 0.0)).expect("a");
        let c = view.world_to_screen(DVec3::new(33.0, 15.0, 0.0)).expect("c");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(a, c));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(a, c));
        let d = tool.draw3d.expect("drawing");
        assert_eq!(d.axis, 2);
        assert_eq!(d.sign, 1.0);
        assert_eq!(d.level, 0.0, "floor top");
        assert!(matches!(d.phase, Phase3d::Height { .. }));
        assert_eq!(d.height(), 16.0);
        let up = c - Vec2::new(0.0, 40.0);
        tool.handle_mouse(&mut state, &view, &Mouse::hover(up));
        let d = tool.draw3d.expect("drawing");
        let h = d.height();
        assert!(h > 16.0, "{h}");
        assert_eq!(h % 16.0, 0.0);
        let before = state.doc.stats().solids;
        tool.handle_mouse(&mut state, &view, &Mouse::click(up));
        assert!(tool.draw3d.is_none());
        assert_eq!(state.doc.stats().solids, before + 1);
        let last = tool.last_bounds().expect("last");
        assert_eq!(last.min, DVec3::new(-32.0, -32.0, 0.0));
        assert_eq!(last.max, DVec3::new(32.0, 16.0, h));
    }

    #[test]
    fn create_as_picker_lists_brush_classes() {
        let mut state = crate::tests_support::fgd_state();
        let mut tool = BlockTool::new();
        tool.picker.open = true;
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        assert_eq!(tool.picker.filtered_len(), 2, "func_detail and trigger_multiple");
        *tool.picker.search_mut() = "trig".to_owned();
        hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
        assert_eq!(tool.picker.filtered_len(), 1);
    }

    #[test]
    fn the_wheel_changes_the_count_only_while_a_countable_shape_is_drawn() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        let none = egui::Modifiers::NONE;
        tool.options.kind = Primitive::Cylinder;
        assert_eq!(tool.wheel(&mut state, &top, 1.0, none), ToolFlow::Ignored, "nothing drawn: the view zooms");
        assert_eq!(tool.options.sides, 8);
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [128.0, 128.0, 64.0])));
        assert_eq!(tool.wheel(&mut state, &top, 1.0, none), ToolFlow::Handled);
        assert_eq!(tool.options.sides, 9);
        assert_eq!(tool.wheel(&mut state, &top, -2.0, none), ToolFlow::Handled);
        assert_eq!(tool.options.sides, 7);
        assert_eq!(tool.wheel(&mut state, &top, 1.0, egui::Modifiers::SHIFT), ToolFlow::Handled);
        assert_eq!(tool.options.sides, 7 + WHEEL_FAST_STEP);
        tool.wheel(&mut state, &top, 0.5, none);
        assert_eq!(tool.options.sides, 11, "half a notch waits for the other half");
        tool.wheel(&mut state, &top, 0.5, none);
        assert_eq!(tool.options.sides, 12);
        tool.wheel(&mut state, &top, -100.0, egui::Modifiers::SHIFT);
        assert_eq!(tool.options.sides, 3, "clamped to the smallest valid count");
        tool.wheel(&mut state, &top, 100.0, egui::Modifiers::SHIFT);
        assert_eq!(tool.options.sides, MAX_PRIMITIVE_SIDES, "clamped to the largest valid count");
        assert_eq!(tool.wheel(&mut state, &top, 1.0, egui::Modifiers::CTRL), ToolFlow::Ignored, "grid size");
        assert_eq!(tool.wheel(&mut state, &top, 1.0, egui::Modifiers::ALT), ToolFlow::Ignored);
        assert_eq!(tool.wheel(&mut state, &top, f32::NAN, none), ToolFlow::Ignored);
        tool.options.sides = 8;
        for kind in [Primitive::Block, Primitive::Wedge, Primitive::Ramp, Primitive::Doorway] {
            tool.options.kind = kind;
            assert_eq!(tool.wheel(&mut state, &top, 1.0, none), ToolFlow::Ignored, "{kind:?} has no count");
        }
        tool.options.door_arch = true;
        assert_eq!(tool.wheel(&mut state, &top, 3.0, none), ToolFlow::Handled);
        assert_eq!(tool.options.door_arch_segments, 11);
        for kind in [Primitive::Spike, Primitive::Sphere, Primitive::Arch, Primitive::Torus, Primitive::Pillar] {
            tool.options.kind = kind;
            tool.options.sides = 8;
            assert_eq!(tool.wheel(&mut state, &top, 1.0, none), ToolFlow::Handled, "{kind:?}");
            assert_eq!(tool.options.sides, 9, "{kind:?}");
        }
        tool.options.kind = Primitive::Cylinder;
        tool.options.sides = 10;
        assert!(tool.confirm(&mut state));
        assert_eq!(axis_faces(&state, 2), 2);
        let id = state.doc.selected_solid_ids()[0];
        assert_eq!(state.doc.scene().solid(id).expect("solid").geom.faces.len(), 12, "10 sides and two caps");
        assert_eq!(tool.wheel(&mut state, &top, 1.0, none), ToolFlow::Ignored, "created: the wheel zooms again");
        assert_eq!(tool.options.sides, 10, "the count stays for the next shape");
        assert_eq!(tool.option_values().iter().find(|o| o.key == "sides").map(|o| o.value.clone()), Some("10".into()));
    }

    #[test]
    fn the_wheel_switches_stairs_from_step_height_to_a_step_count() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        let top = info_2d(ViewKind::Top, 1.0);
        tool.options.kind = Primitive::Stairs;
        tool.options.stair_step_height = 16.0;
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [64.0, 256.0, 96.0])));
        assert_eq!(tool.shape_count(), Some((ShapeCount::Steps, 6)), "96 high in 16 unit steps");
        assert_eq!(tool.wheel(&mut state, &top, 1.0, egui::Modifiers::NONE), ToolFlow::Handled);
        assert_eq!((tool.options.stair_steps, tool.options.stair_step_height), (7, 0.0));
        tool.wheel(&mut state, &top, -50.0, egui::Modifiers::NONE);
        assert_eq!(tool.options.stair_steps, 1);
        tool.wheel(&mut state, &top, 300.0, egui::Modifiers::SHIFT);
        assert_eq!(tool.options.stair_steps, MAX_STAIR_STEPS);
        tool.options.stair_steps = 5;
        let n = tool.create(&mut state, b([0.0, 0.0, 0.0], [64.0, 256.0, 96.0])).expect("stairs");
        assert_eq!(n, 5);
        tool.options.kind = Primitive::SpiralStairs;
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [256.0, 256.0, 96.0])));
        assert_eq!(tool.wheel(&mut state, &top, 2.0, egui::Modifiers::NONE), ToolFlow::Handled);
        assert_eq!(tool.options.stair_steps, 7);
    }

    #[test]
    fn the_wheel_works_during_the_drag_and_the_3d_height_phase() {
        let mut state = settle();
        state.doc.clear_selection();
        let mut tool = BlockTool::new();
        tool.options.kind = Primitive::Sphere;
        let top = info_2d(ViewKind::Top, 1.0);
        let a = top.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).expect("a");
        let c = top.world_to_screen(DVec3::new(64.0, 64.0, 0.0)).expect("c");
        tool.handle_mouse(&mut state, &top, &Mouse::drag_start(a, a));
        assert!(tool.is_drawing());
        assert_eq!(tool.wheel(&mut state, &top, 1.0, egui::Modifiers::NONE), ToolFlow::Handled, "before any motion");
        tool.handle_mouse(&mut state, &top, &Mouse::drag(a, c));
        assert_eq!(tool.wheel(&mut state, &top, 1.0, egui::Modifiers::NONE), ToolFlow::Handled);
        assert_eq!(tool.options.sides, 10);
        tool.handle_mouse(&mut state, &top, &Mouse::drag_end(a, c));
        assert!(tool.count_label().is_some_and(|l| l.contains("10")));
        assert!(tool.status_hint(&state).is_some_and(|h| h.contains(&WHEEL_FAST_STEP.to_string())));
        tool.cancel(&mut state);
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 512.0));
        let a = view.world_to_screen(DVec3::new(-30.0, -30.0, 0.0)).expect("a");
        let c = view.world_to_screen(DVec3::new(33.0, 15.0, 0.0)).expect("c");
        assert_eq!(tool.wheel(&mut state, &view, 1.0, egui::Modifiers::NONE), ToolFlow::Ignored, "3D dolly");
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(a, a));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(a, c));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(a, c));
        assert!(matches!(tool.draw3d.map(|d| d.phase), Some(Phase3d::Height { .. })));
        assert_eq!(tool.wheel(&mut state, &view, -1.0, egui::Modifiers::NONE), ToolFlow::Handled);
        assert_eq!(tool.options.sides, 9);
        tool.options.kind = Primitive::Block;
        assert!(tool.count_label().is_none());
        assert!(tool.status_hint(&state).is_some_and(|h| !h.contains(&WHEEL_FAST_STEP.to_string())));
    }

    #[test]
    fn hints_and_cursor() {
        let mut state = settle();
        let mut tool = BlockTool::new();
        assert!(tool.status_hint(&state).is_some());
        tool.set_pending_box(Some(b([0.0, 0.0, 0.0], [64.0, 64.0, 64.0])));
        assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
        let top = info_2d(ViewKind::Top, 1.0);
        assert_eq!(tool.cursor(&state, &top, top.rect.left_top()), Some(CursorIcon::Crosshair));
        tool.deactivate(&mut state);
        assert!(tool.pending_box().is_none());
    }
}
