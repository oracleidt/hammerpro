use glam::{DMat3, DVec2, DVec3};
use hp_editor::{GRIDS_BLOCK, ObjRef};
use hp_geom::Aabb;

use crate::settings::{GRID_MAX, GRID_MIN, GridMode, GridSpec, MAX_CUSTOM_GRIDS, ViewGrid, clamp_grid};
use crate::state::AppState;
use crate::view::{QuadrantView, ViewKind};

const SELECTION_GRID_SCAN: usize = 4096;

fn tidy(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    let r = (v * 1e6).round() / 1e6;
    if (v - r).abs() < 1e-9 * v.abs().max(1.0) { r + 0.0 } else { v + 0.0 }
}

fn tidy_vec(v: DVec3) -> DVec3 {
    DVec3::new(tidy(v.x), tidy(v.y), tidy(v.z))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisSnap {
    pub origin: f64,
    pub step: f64,
}

impl AxisSnap {
    pub const OFF: AxisSnap = AxisSnap { origin: 0.0, step: 0.0 };

    pub fn step(step: f64) -> AxisSnap {
        AxisSnap { origin: 0.0, step: if step.is_finite() && step > 0.0 { step } else { 0.0 } }
    }

    pub fn is_on(&self) -> bool {
        self.step > 0.0
    }

    pub fn snap(&self, v: f64) -> f64 {
        if !self.is_on() || !v.is_finite() {
            return v;
        }
        tidy(self.origin + ((v - self.origin) / self.step).round() * self.step)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lattice2 {
    pub origin: DVec2,
    pub a: DVec2,
    pub b: DVec2,
}

impl Lattice2 {
    pub fn coords(&self, p: DVec2) -> Option<DVec2> {
        let det = self.a.x * self.b.y - self.a.y * self.b.x;
        let scale = self.a.length() * self.b.length();
        if !det.is_finite() || det.abs() <= 1e-9 * scale.max(1e-12) {
            return None;
        }
        let d = p - self.origin;
        Some(DVec2::new((d.x * self.b.y - d.y * self.b.x) / det, (self.a.x * d.y - self.a.y * d.x) / det))
    }

    pub fn point(&self, i: f64, j: f64) -> DVec2 {
        self.origin + self.a * i + self.b * j
    }

    pub fn snap(&self, p: DVec2) -> DVec2 {
        match self.coords(p) {
            Some(c) if p.is_finite() => {
                let q = self.point(c.x.round(), c.y.round());
                DVec2::new(tidy(q.x), tidy(q.y))
            }
            _ => p,
        }
    }

    pub fn is_axis_aligned(&self) -> bool {
        (self.a.y.abs() <= 1e-9 * self.a.x.abs() && self.b.x.abs() <= 1e-9 * self.b.y.abs())
            || (self.a.x.abs() <= 1e-9 * self.a.y.abs() && self.b.y.abs() <= 1e-9 * self.b.x.abs())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridFrame {
    pub origin: DVec3,
    pub axes: DMat3,
    pub step: DVec3,
    pub major_every: u32,
    pub bounds: Option<Aabb>,
    pub outside: f64,
    pub color: Option<[u8; 3]>,
}

impl GridFrame {
    pub fn uniform(size: f64) -> GridFrame {
        let size = clamp_grid(size, 16.0);
        GridFrame {
            origin: DVec3::ZERO,
            axes: DMat3::IDENTITY,
            step: DVec3::splat(size),
            major_every: 0,
            bounds: None,
            outside: size,
            color: None,
        }
    }

    pub fn from_view_grid(g: &ViewGrid, kind: ViewKind) -> GridFrame {
        let g = g.sanitized();
        GridFrame { origin: DVec3::from_array(g.origin), major_every: g.major_every, ..GridFrame::uniform(g.size) }
            .rotated_in_view(kind, g.angle)
    }

    pub fn from_spec(spec: &GridSpec, outside: f64) -> GridFrame {
        let s = spec.sanitized();
        let axes = hp_geom::angle_matrix(DVec3::from_array(s.angles));
        let axes = if axes.is_finite() { axes } else { DMat3::IDENTITY };
        GridFrame {
            origin: DVec3::from_array(s.origin),
            axes,
            step: DVec3::from_array(s.step),
            major_every: s.major_every,
            bounds: s.bounds.map(|[a, b]| Aabb::new(DVec3::from_array(a), DVec3::from_array(b))),
            outside: clamp_grid(outside, 16.0),
            color: s.color,
        }
    }

    pub fn scaled(&self, factor: f64) -> GridFrame {
        let f = if factor.is_finite() && factor > 0.0 { factor } else { 1.0 };
        GridFrame { step: (self.step * f).clamp(DVec3::splat(GRID_MIN), DVec3::splat(GRID_MAX)), ..*self }
    }

    pub fn is_rotated(&self) -> bool {
        let m = self.axes;
        [m.x_axis, m.y_axis, m.z_axis].iter().any(|c| {
            let a = c.abs();
            let max = a.max_element();
            (1.0 - max).abs() > 1e-9 || a.element_sum() - max > 1e-9
        })
    }

    pub fn is_plain(&self) -> bool {
        let s = self.step.x;
        let on_lattice = |o: f64| {
            let r = o / s;
            (r - r.round()).abs() < 1e-9
        };
        !self.is_rotated()
            && (self.step.y - s).abs() <= 1e-9 * s
            && (self.step.z - s).abs() <= 1e-9 * s
            && on_lattice(self.origin.x)
            && on_lattice(self.origin.y)
            && on_lattice(self.origin.z)
            && self.bounds.is_none()
            && self.color.is_none()
    }

    pub fn nominal(&self) -> f64 {
        self.step.min_element()
    }

    pub fn contains(&self, p: DVec3) -> bool {
        self.bounds.is_none_or(|b| b.contains_point(p))
    }

    fn contains_2d(&self, kind: ViewKind, p: DVec2) -> bool {
        match (self.bounds, kind.axes_2d()) {
            (Some(b), Some((r, u, _))) => {
                p.x >= b.min[r] - 1e-9 && p.x <= b.max[r] + 1e-9 && p.y >= b.min[u] - 1e-9 && p.y <= b.max[u] + 1e-9
            }
            (Some(b), None) => b.contains_point(DVec3::new(p.x, p.y, b.center().z)),
            (None, _) => true,
        }
    }

    fn outside_frame(&self) -> GridFrame {
        GridFrame::uniform(self.outside)
    }

    pub fn to_local(&self, p: DVec3) -> DVec3 {
        self.axes.transpose() * (p - self.origin)
    }

    pub fn to_world(&self, l: DVec3) -> DVec3 {
        self.origin + self.axes * l
    }

    pub fn snap_point(&self, p: DVec3) -> DVec3 {
        if !p.is_finite() {
            return p;
        }
        if !self.contains(p) {
            return self.outside_frame().snap_point(p);
        }
        let l = self.to_local(p);
        let n = (l / self.step).round() * self.step;
        tidy_vec(self.to_world(n))
    }

    pub fn snap_delta(&self, reference: DVec3, raw: DVec3) -> DVec3 {
        let to = self.snap_point(reference + raw);
        if to.is_finite() { tidy_vec(to - reference) } else { raw }
    }

    fn closest_axis(&self, k: usize) -> usize {
        let m = self.axes;
        let along = [m.x_axis[k].abs(), m.y_axis[k].abs(), m.z_axis[k].abs()];
        let mut best = 0;
        for j in 1..3 {
            if along[j] > along[best] + 1e-12 {
                best = j;
            }
        }
        best
    }

    pub fn axis(&self, k: usize) -> AxisSnap {
        let k = k.min(2);
        AxisSnap { origin: self.origin[k], step: self.step[self.closest_axis(k)] }
    }

    pub fn axis_at(&self, k: usize, p: DVec3) -> AxisSnap {
        if self.contains(p) { self.axis(k) } else { self.outside_frame().axis(k) }
    }

    pub fn step_along(&self, k: usize) -> f64 {
        self.axis(k).step
    }

    pub fn lattice_2d(&self, kind: ViewKind) -> Lattice2 {
        let depth = kind.axes_2d().map_or(2, |a| a.2);
        let m = self.axes;
        let cols = [m.x_axis, m.y_axis, m.z_axis];
        let mut normal = 0;
        for j in 1..3 {
            if cols[j][depth].abs() > cols[normal][depth].abs() + 1e-12 {
                normal = j;
            }
        }
        let (p, q) = match normal {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let a = kind.to_2d(cols[p] * self.step[p]);
        let b = kind.to_2d(cols[q] * self.step[q]);
        let (a, b) = if a.x.abs() * b.length() >= b.x.abs() * a.length() { (a, b) } else { (b, a) };
        Lattice2 { origin: kind.to_2d(self.origin), a, b }
    }

    pub fn snap_2d(&self, kind: ViewKind, p: DVec2) -> DVec2 {
        if !p.is_finite() {
            return p;
        }
        if !self.contains_2d(kind, p) {
            return self.outside_frame().snap_2d(kind, p);
        }
        self.lattice_2d(kind).snap(p)
    }

    pub fn snap_delta_2d(&self, kind: ViewKind, reference: DVec2, raw: DVec2) -> DVec2 {
        let to = self.snap_2d(kind, reference + raw);
        if to.is_finite() { DVec2::new(tidy(to.x - reference.x), tidy(to.y - reference.y)) } else { raw }
    }

    pub fn on_plane(&self, p: DVec3, n: DVec3) -> DVec3 {
        if !p.is_finite() {
            return p;
        }
        if !self.contains(p) {
            return self.outside_frame().on_plane(p, n);
        }
        let Some(n) = n.try_normalize() else { return p };
        let lp = self.to_local(p);
        let ln = self.axes.transpose() * n;
        let a = ln.abs();
        let k = if a.z >= a.x && a.z >= a.y {
            2
        } else if a.x >= a.y {
            0
        } else {
            1
        };
        if ln[k].abs() < 1e-9 {
            return p;
        }
        let d = ln.dot(lp);
        let mut q = lp;
        let mut rest = 0.0;
        for i in 0..3 {
            if i != k {
                q[i] = (lp[i] / self.step[i]).round() * self.step[i];
                rest += ln[i] * q[i];
            }
        }
        q[k] = (d - rest) / ln[k];
        let out = tidy_vec(self.to_world(q));
        if out.is_finite() { out } else { p }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GridSource {
    Global,
    View(ViewKind),
    Map(String),
    Preset(String),
}

impl GridSource {
    pub fn custom_name(&self) -> Option<&str> {
        match self {
            GridSource::Map(n) | GridSource::Preset(n) => Some(n),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveGrid {
    pub frame: GridFrame,
    pub source: GridSource,
}

pub fn grids_from_block(block: &hp_kv::Block) -> Vec<GridSpec> {
    let mut out: Vec<GridSpec> = Vec::new();
    for child in block.children.iter().filter(|c| c.name.eq_ignore_ascii_case("grid")) {
        if let Some(spec) = GridSpec::from_block(child)
            && !out.iter().any(|g| g.name.eq_ignore_ascii_case(&spec.name))
            && out.len() < MAX_CUSTOM_GRIDS
        {
            out.push(spec);
        }
    }
    out
}

pub fn grids_to_block(grids: &[GridSpec]) -> hp_kv::Block {
    let mut block = hp_kv::Block::new(GRIDS_BLOCK);
    for g in grids {
        block.push_child(g.to_block());
    }
    block
}

#[derive(Debug, Default)]
pub struct GridCache {
    inner: parking_lot::Mutex<Option<CachedGrids>>,
}

#[derive(Debug, Clone)]
struct CachedGrids {
    revision: u64,
    map: Vec<GridSpec>,
    selection: Option<String>,
}

impl AppState {
    fn cached_grids(&self) -> CachedGrids {
        let revision = self.doc.revision();
        let mut cache = self.grid_cache.inner.lock();
        if let Some(c) = cache.as_ref().filter(|c| c.revision == revision) {
            return c.clone();
        }
        let c = CachedGrids {
            revision,
            map: self.doc.extra_block(GRIDS_BLOCK).map(grids_from_block).unwrap_or_default(),
            selection: self.doc.selection_grid(SELECTION_GRID_SCAN).map(str::to_owned),
        };
        *cache = Some(c.clone());
        c
    }

    pub fn map_grids(&self) -> Vec<GridSpec> {
        self.cached_grids().map
    }

    pub fn find_grid(&self, name: &str) -> Option<(GridSpec, GridSource)> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        if let Some(g) = self.map_grids().into_iter().find(|g| g.name.eq_ignore_ascii_case(name)) {
            let n = g.name.clone();
            return Some((g, GridSource::Map(n)));
        }
        self.settings.grid.preset(name).map(|g| (g.clone(), GridSource::Preset(g.name.clone())))
    }

    pub fn all_custom_grids(&self) -> Vec<(GridSpec, GridSource)> {
        let map = self.map_grids();
        let mut out: Vec<(GridSpec, GridSource)> =
            map.iter().map(|g| (g.clone(), GridSource::Map(g.name.clone()))).collect();
        for p in &self.settings.grid.presets {
            if !map.iter().any(|g| g.name.eq_ignore_ascii_case(&p.name)) {
                out.push((p.clone(), GridSource::Preset(p.name.clone())));
            }
        }
        out
    }

    pub fn quadrant_kind(&self, index: usize) -> Option<ViewKind> {
        QuadrantView::from_index(self.settings.ui.quadrants.get(index).copied().unwrap_or(0)).kind()
    }

    pub fn grid_target_view(&self) -> ViewKind {
        self.ui
            .hovered_view
            .and_then(|i| self.quadrant_kind(i))
            .or_else(|| self.ui.focused_view.and_then(|i| self.quadrant_kind(i)))
            .unwrap_or(ViewKind::Top)
    }

    fn global_grid(&self, kind: ViewKind) -> ActiveGrid {
        let angle = self.settings.grid.per_view.get(kind).sanitized().angle;
        let frame = GridFrame::uniform(self.settings.edit.grid_size).rotated_in_view(kind, angle);
        ActiveGrid { frame, source: GridSource::Global }
    }

    fn custom_active(&self, name: &str, kind: ViewKind) -> ActiveGrid {
        match self.find_grid(name) {
            Some((spec, source)) => {
                ActiveGrid { frame: GridFrame::from_spec(&spec, self.settings.edit.grid_size), source }
            }
            None => self.global_grid(kind),
        }
    }

    pub fn selection_grid_name(&self) -> Option<String> {
        self.cached_grids().selection
    }

    pub fn active_grid(&self, kind: ViewKind) -> ActiveGrid {
        let g = &self.settings.grid;
        match g.mode {
            GridMode::Global => self.global_grid(kind),
            GridMode::PerView => {
                let frame = GridFrame::from_view_grid(g.per_view.get(kind), kind);
                ActiveGrid { frame, source: GridSource::View(kind) }
            }
            GridMode::Custom => self.custom_active(&g.custom, kind),
            GridMode::Selection => match self.selection_grid_name() {
                Some(name) => self.custom_active(&name, kind),
                None => self.global_grid(kind),
            },
        }
    }

    pub fn grid_frame(&self, kind: ViewKind) -> GridFrame {
        self.active_grid(kind).frame
    }

    pub fn object_grid(&self, obj: ObjRef) -> Option<GridFrame> {
        if !self.settings.grid.object_grids {
            return None;
        }
        let name = self.doc.object_grid(obj)?;
        let (spec, _) = self.find_grid(name)?;
        Some(GridFrame::from_spec(&spec, self.settings.edit.grid_size))
    }

    pub fn edit_grid_frame(&self, kind: ViewKind) -> GridFrame {
        if self.settings.grid.object_grids
            && let Some(name) = self.selection_grid_name()
            && let Some((spec, _)) = self.find_grid(&name)
        {
            return GridFrame::from_spec(&spec, self.settings.edit.grid_size);
        }
        self.grid_frame(kind)
    }

    pub fn display_grid_frame(&self, kind: ViewKind) -> GridFrame {
        match self.tool {
            crate::ToolId::Select | crate::ToolId::Vertex | crate::ToolId::Clip => self.edit_grid_frame(kind),
            _ => self.grid_frame(kind),
        }
    }

    pub fn surface_grid_frame(&self, kind: ViewKind, obj: Option<ObjRef>) -> GridFrame {
        obj.and_then(|o| self.object_grid(o)).unwrap_or_else(|| self.grid_frame(kind))
    }

    pub fn target_grid(&self) -> ActiveGrid {
        self.active_grid(self.grid_target_view())
    }

    pub fn set_target_grid_size(&mut self, size: f64) -> Result<(), String> {
        if !size.is_finite() || size <= 0.0 {
            return Err(format!("the grid size must be a number from {GRID_MIN} to {GRID_MAX}"));
        }
        let size = clamp_grid(size, 16.0);
        let target = self.target_grid();
        match target.source {
            GridSource::Global => {
                if (self.settings.edit.grid_size - size).abs() > f64::EPSILON {
                    self.settings.edit.grid_size = size;
                    self.settings_dirty = true;
                }
            }
            GridSource::View(kind) => {
                let g = self.settings.grid.per_view.get_mut(kind);
                if (g.size - size).abs() > f64::EPSILON {
                    g.size = size;
                    self.settings_dirty = true;
                }
            }
            GridSource::Map(name) | GridSource::Preset(name) => {
                let Some((mut spec, source)) = self.find_grid(&name) else { return Ok(()) };
                let factor = size / spec.step.iter().copied().fold(f64::INFINITY, f64::min);
                spec.step = spec.step.map(|s| clamp_grid(s * factor, s));
                self.put_custom_grid(&name, spec, matches!(source, GridSource::Map(_)), true)?;
            }
        }
        Ok(())
    }

    pub fn step_target_grid(&mut self, larger: bool) -> Result<(), String> {
        let factor = self.settings.workflow.grid_step_factor;
        let next = crate::settings::grid_step(self.grid_size(), factor, larger);
        self.set_target_grid_size(next)
    }

    pub fn put_custom_grid(&mut self, old_name: &str, spec: GridSpec, in_map: bool, merge: bool) -> Result<(), String> {
        let spec = spec.sanitized();
        if spec.name.is_empty() {
            return Err("a grid needs a name".to_owned());
        }
        let old = old_name.trim();
        let old = if old.is_empty() { spec.name.as_str() } else { old }.to_owned();
        let renamed = !old.eq_ignore_ascii_case(&spec.name);
        let place = |grids: &[GridSpec]| -> Result<Option<usize>, String> {
            let at = grids.iter().position(|g| g.name.eq_ignore_ascii_case(&old));
            let clash = grids.iter().enumerate().any(|(i, g)| Some(i) != at && g.name.eq_ignore_ascii_case(&spec.name));
            if clash {
                return Err(format!("a grid is already named \"{}\"", spec.name));
            }
            if at.is_none() && grids.len() >= MAX_CUSTOM_GRIDS {
                return Err(format!("at most {MAX_CUSTOM_GRIDS} grids are kept"));
            }
            Ok(at)
        };
        if in_map {
            let mut grids = self.map_grids();
            match place(&grids)? {
                Some(i) => grids[i] = spec.clone(),
                None => grids.push(spec.clone()),
            }
            if merge {
                self.doc.begin_merge("Edit grid", format!("grid:{}", spec.name.to_lowercase()));
            } else {
                self.doc.begin("Edit grid");
            }
            let result = self.doc.set_extra_block(GRIDS_BLOCK, grids_to_block(&grids));
            if result.is_ok() && renamed {
                self.doc.rename_object_grids(&old, Some(&spec.name));
            }
            match result {
                Ok(()) => {
                    self.doc.commit();
                }
                Err(e) => {
                    self.doc.cancel();
                    return Err(e.to_string());
                }
            }
        } else {
            let at = place(&self.settings.grid.presets)?;
            let presets = &mut self.settings.grid.presets;
            match at {
                Some(i) => presets[i] = spec.clone(),
                None => presets.push(spec.clone()),
            }
            self.settings_dirty = true;
        }
        if renamed && self.settings.grid.custom.eq_ignore_ascii_case(&old) {
            self.settings.grid.custom = spec.name.clone();
            self.settings_dirty = true;
        }
        Ok(())
    }

    pub fn delete_custom_grid(&mut self, name: &str, in_map: bool) -> Result<bool, String> {
        if in_map {
            let mut grids = self.map_grids();
            let before = grids.len();
            grids.retain(|g| !g.name.eq_ignore_ascii_case(name.trim()));
            if grids.len() == before {
                return Ok(false);
            }
            self.doc.begin("Delete grid");
            let result = if grids.is_empty() {
                self.doc.remove_extra_block(GRIDS_BLOCK).map(|_| ())
            } else {
                self.doc.set_extra_block(GRIDS_BLOCK, grids_to_block(&grids))
            };
            match result {
                Ok(()) => {
                    self.doc.rename_object_grids(name.trim(), None);
                    self.doc.commit();
                }
                Err(e) => {
                    self.doc.cancel();
                    return Err(e.to_string());
                }
            }
        } else {
            let presets = &mut self.settings.grid.presets;
            let before = presets.len();
            presets.retain(|g| !g.name.eq_ignore_ascii_case(name.trim()));
            if presets.len() == before {
                return Ok(false);
            }
            self.settings_dirty = true;
        }
        Ok(true)
    }

    pub fn set_grid_mode(&mut self, mode: GridMode) {
        let g = &mut self.settings.grid;
        if mode == GridMode::PerView && g.per_view.is_unset() {
            let mut next = crate::settings::PerViewGrids::uniform(self.settings.edit.grid_size);
            for kind in [ViewKind::Perspective, ViewKind::Top, ViewKind::Front, ViewKind::Side] {
                next.get_mut(kind).angle = g.per_view.get(kind).angle;
            }
            g.per_view = next;
        }
        if g.mode != mode {
            g.mode = mode;
            self.settings_dirty = true;
        }
    }

    pub fn unique_grid_name(&self, base: &str) -> String {
        let base = hp_editor::clean_grid_name(base);
        let base = if base.is_empty() { "Grid".to_owned() } else { base };
        let taken = |n: &str| self.find_grid(n).is_some();
        if !taken(&base) {
            return base;
        }
        (2..).map(|i| format!("{base} {i}")).find(|n| !taken(n)).unwrap_or(base)
    }

    pub fn grid_from_selection(&self, name: &str) -> Option<GridSpec> {
        let scene = self.doc.scene();
        let objs = self.doc.selected_objects();
        let size = clamp_grid(self.settings.edit.grid_size, 16.0);
        let solid = scene.expand_solids(&objs).into_iter().find_map(|id| scene.solid(id));
        let (axes, points) = match solid {
            Some(node) => {
                let faces = &node.geom.faces;
                let area = |f: &hp_geom::FaceGeom| hp_geom::polygon_area_vector(&f.vertices).length();
                let up_face = faces.iter().max_by(|a, b| a.normal().z.total_cmp(&b.normal().z))?;
                let up = if up_face.normal().z > 0.3 { up_face.normal() } else { DVec3::Z };
                let side = faces
                    .iter()
                    .filter_map(|f| {
                        let n = f.normal();
                        let flat = (n - up * n.dot(up)).try_normalize()?;
                        (n.dot(up).abs() < 0.9).then_some((area(f), flat))
                    })
                    .max_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, d)| d);
                let x = side.unwrap_or_else(|| (DVec3::X - up * up.x).try_normalize().unwrap_or(DVec3::Y));
                let y = up.cross(x).normalize();
                let points: Vec<DVec3> = faces.iter().flat_map(|f| f.vertices.iter().copied()).collect();
                (DMat3::from_cols(x, y, up), points)
            }
            None => {
                let id = objs.iter().find_map(|o| match o {
                    ObjRef::Entity(id) => Some(*id),
                    ObjRef::Solid(_) => None,
                })?;
                let e = &scene.entity(id)?.entity;
                let origin = e.origin().filter(|o| o.is_finite())?;
                let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
                (hp_geom::angle_matrix(angles), vec![origin])
            }
        };
        Some(Self::fitted_grid(name, axes, &points, size))
    }

    pub fn grid_from_face(&self, name: &str) -> Option<GridSpec> {
        let face = self.doc.selected_faces().into_iter().next()?;
        let node = self.doc.scene().solid(face.solid)?;
        let geom = node.geom.faces.iter().find(|f| f.side_index == face.side)?;
        let n = geom.normal().try_normalize()?;
        let v = &geom.vertices;
        let longest = (0..v.len())
            .map(|i| v[(i + 1) % v.len()] - v[i])
            .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))?;
        let x = (longest - n * longest.dot(n)).try_normalize()?;
        let y = n.cross(x).normalize();
        let size = clamp_grid(self.settings.edit.grid_size, 16.0);
        Some(Self::fitted_grid(name, DMat3::from_cols(x, y, n), v, size))
    }

    fn fitted_grid(name: &str, axes: DMat3, points: &[DVec3], size: f64) -> GridSpec {
        let t = axes.transpose();
        let mut lo = DVec3::splat(f64::INFINITY);
        for p in points {
            lo = lo.min(t * *p);
        }
        let origin = if lo.is_finite() { axes * lo } else { DVec3::ZERO };
        let angles = hp_geom::clean_angles(hp_geom::matrix_angles(&axes));
        GridSpec {
            name: name.to_owned(),
            step: [size; 3],
            origin: tidy_vec(origin).to_array(),
            angles: angles.to_array().map(tidy),
            ..GridSpec::default()
        }
        .sanitized()
    }

    pub fn use_custom_grid(&mut self, name: &str) -> Result<(), String> {
        let (spec, _) = self.find_grid(name).ok_or_else(|| format!("no grid named \"{}\"", name.trim()))?;
        self.settings.grid.custom = spec.name;
        self.settings.grid.mode = GridMode::Custom;
        self.settings_dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: DVec3, b: DVec3) -> bool {
        (a - b).length() < 1e-6
    }

    #[test]
    fn plain_grid_snaps_like_hammer() {
        let g = GridFrame::uniform(16.0);
        assert!(g.is_plain() && !g.is_rotated());
        assert_eq!(g.snap_point(DVec3::new(7.0, 9.0, -25.0)), DVec3::new(0.0, 16.0, -32.0));
        assert_eq!(g.snap_2d(ViewKind::Front, DVec2::new(23.0, 41.0)), DVec2::new(16.0, 48.0));
        assert_eq!(g.axis(2).snap(-9.0), -16.0);
        assert_eq!(g.snap_delta(DVec3::new(3.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 0.0)), DVec3::new(13.0, 0.0, 0.0));
        assert_eq!(g.nominal(), 16.0);
        let g = GridFrame::uniform(0.1);
        assert_eq!(g.snap_point(DVec3::new(0.31, 0.0, 0.0)).x, 0.3);
        let g = GridFrame::uniform(12.0);
        assert_eq!(g.snap_2d(ViewKind::Top, DVec2::new(19.0, -5.0)), DVec2::new(24.0, 0.0));
    }

    #[test]
    fn non_uniform_offset_grid() {
        let spec =
            GridSpec { name: "Stairs".into(), step: [12.0, 24.0, 8.0], origin: [4.0, 0.0, 2.0], ..GridSpec::default() };
        let g = GridFrame::from_spec(&spec, 16.0);
        assert!(!g.is_plain() && !g.is_rotated());
        assert_eq!(g.snap_point(DVec3::new(10.0, 13.0, 7.0)), DVec3::new(16.0, 24.0, 10.0));
        assert_eq!(g.axis(0), AxisSnap { origin: 4.0, step: 12.0 });
        assert_eq!(g.axis(2).snap(5.0), 2.0);
        assert_eq!(g.snap_2d(ViewKind::Front, DVec2::new(30.0, 7.0)), DVec2::new(24.0, 10.0));
        assert_eq!(g.nominal(), 8.0);
        let p = g.on_plane(DVec3::new(9.0, 11.0, 3.0), DVec3::Z);
        assert_eq!(p, DVec3::new(4.0, 0.0, 3.0));
    }

    #[test]
    fn rotated_grid_snaps_in_its_own_axes() {
        let spec = GridSpec { name: "R".into(), step: [16.0; 3], angles: [0.0, 45.0, 0.0], ..GridSpec::default() };
        let g = GridFrame::from_spec(&spec, 16.0);
        assert!(g.is_rotated() && !g.is_plain());
        let d = 16.0 / 2f64.sqrt();
        let near = DVec3::new(d + 1.0, d - 1.0, 3.0);
        assert!(close(g.snap_point(near), DVec3::new(d, d, 0.0)), "{}", g.snap_point(near));
        let l = g.lattice_2d(ViewKind::Top);
        assert!(!l.is_axis_aligned());
        let s = g.snap_2d(ViewKind::Top, DVec2::new(0.5, 2.0 * d + 0.7));
        assert!((s - DVec2::new(0.0, 2.0 * d)).length() < 1e-6, "{s}");
        for p in [DVec3::new(37.0, -12.0, 5.0), DVec3::new(-100.0, 3.0, 64.0)] {
            let l = g.to_local(g.snap_point(p)) / 16.0;
            assert!(close(l, l.round()), "{l}");
        }
        let tilted =
            GridFrame::from_spec(&GridSpec { name: "T".into(), angles: [30.0, 0.0, 0.0], ..GridSpec::default() }, 16.0);
        let l = tilted.lattice_2d(ViewKind::Side);
        let c = l.coords(l.snap(DVec2::new(40.0, 13.0))).unwrap();
        assert!((c - c.round()).length() < 1e-9);
        let n = tilted.axes.z_axis;
        let on = tilted.on_plane(DVec3::new(30.0, 10.0, -30.0f64.to_radians().tan() * 30.0), n);
        assert!(n.dot(on).abs() < 1e-6, "{on}");
    }

    #[test]
    fn limits_fall_back_to_the_global_grid() {
        let spec = GridSpec {
            name: "Box".into(),
            step: [10.0; 3],
            bounds: Some([[0.0; 3], [100.0; 3]]),
            ..GridSpec::default()
        };
        let g = GridFrame::from_spec(&spec, 64.0);
        assert_eq!(g.snap_point(DVec3::new(23.0, 47.0, 51.0)), DVec3::new(20.0, 50.0, 50.0));
        assert_eq!(g.snap_point(DVec3::new(-23.0, 47.0, 51.0)), DVec3::new(0.0, 64.0, 64.0), "outside");
        assert_eq!(g.snap_2d(ViewKind::Top, DVec2::new(-40.0, 3.0)), DVec2::new(-64.0, 0.0));
        assert_eq!(g.axis_at(0, DVec3::splat(500.0)).step, 64.0);
    }

    fn state_with_box() -> (AppState, hp_editor::Id) {
        let mut state = crate::testing::test_state();
        state.doc = hp_editor::Document::new_empty();
        let ids = state.doc.create_primitive(&hp_geom::PrimitiveParams::default(), hp_editor::Owner::World).unwrap();
        (state, ids[0])
    }

    #[test]
    fn global_mode_is_hammer() {
        let (state, _) = state_with_box();
        assert_eq!(state.settings.grid.mode, GridMode::Global);
        assert_eq!(state.grid_frame(ViewKind::Top), GridFrame::uniform(16.0));
        assert_eq!(state.grid_frame(ViewKind::Perspective), GridFrame::uniform(16.0));
        assert_eq!(state.grid_size(), 16.0);
    }

    #[test]
    fn per_view_mode_uses_the_view_of_the_action() {
        let (mut state, _) = state_with_box();
        state.settings.edit.grid_size = 8.0;
        state.set_grid_mode(GridMode::PerView);
        assert_eq!(state.settings.grid.per_view.top.size, 8.0, "seeded with the global size");
        state.settings.grid.per_view.front.size = 12.0;
        state.settings.grid.per_view.front.origin = [0.0, 4.0, 0.0];
        assert_eq!(state.grid_frame(ViewKind::Top).nominal(), 8.0);
        let front = state.grid_frame(ViewKind::Front);
        assert_eq!(front.nominal(), 12.0);
        assert_eq!(front.snap_2d(ViewKind::Front, DVec2::new(9.0, 7.0)), DVec2::new(4.0, 12.0));
        state.ui.hovered_view = Some(2);
        assert_eq!(state.grid_target_view(), ViewKind::Front);
        assert_eq!(state.grid_size(), 12.0);
        state.set_target_grid_size(24.0).unwrap();
        assert_eq!(state.settings.grid.per_view.front.size, 24.0);
        assert_eq!(state.settings.grid.per_view.top.size, 8.0, "other views keep their grid");
        assert_eq!(state.settings.edit.grid_size, 8.0, "and the global grid too");
        state.step_target_grid(true).unwrap();
        assert_eq!(state.settings.grid.per_view.front.size, 32.0);
        state.step_target_grid(false).unwrap();
        assert_eq!(state.settings.grid.per_view.front.size, 16.0);
        state.settings.grid.per_view.front.size = 24.0;
        state.ui.hovered_view = None;
        state.ui.focused_view = Some(1);
        assert_eq!(state.grid_target_view(), ViewKind::Top);
        assert!(state.set_target_grid_size(f64::NAN).is_err());
    }

    #[test]
    fn custom_grids_of_the_map_are_undoable_and_saved_with_it() {
        let (mut state, id) = state_with_box();
        let spec =
            GridSpec { name: "Roof".into(), step: [12.0, 12.0, 8.0], angles: [0.0, 30.0, 0.0], ..GridSpec::default() };
        state.put_custom_grid("", spec.clone(), true, false).unwrap();
        assert_eq!(state.doc.undo_label(), Some("Edit grid"));
        assert_eq!(state.map_grids(), vec![spec.sanitized()]);
        state.use_custom_grid("roof").unwrap();
        assert_eq!(state.settings.grid.mode, GridMode::Custom);
        let f = state.grid_frame(ViewKind::Top);
        assert!(f.is_rotated());
        assert_eq!(f.nominal(), 8.0);
        state.set_target_grid_size(4.0).unwrap();
        assert_eq!(state.map_grids()[0].step, [6.0, 6.0, 4.0]);
        state.step_target_grid(true).unwrap();
        assert_eq!(state.map_grids()[0].step, [12.0, 12.0, 8.0]);
        state.settings.grid.mode = GridMode::Global;
        let solid = ObjRef::Solid(id);
        state.doc.set_object_grid(&[solid], &[], Some("Roof")).unwrap();
        state.doc.select(&[solid], hp_editor::SelectOp::Replace);
        assert!(state.edit_grid_frame(ViewKind::Top).is_rotated());
        assert!(!state.grid_frame(ViewKind::Top).is_rotated(), "drawing new things keeps the active grid");
        assert!(state.surface_grid_frame(ViewKind::Perspective, Some(solid)).is_rotated());
        state.settings.grid.object_grids = false;
        assert!(!state.edit_grid_frame(ViewKind::Top).is_rotated());
        state.settings.grid.object_grids = true;
        state.set_grid_mode(GridMode::Selection);
        assert!(state.grid_frame(ViewKind::Front).is_rotated());
        let renamed = GridSpec { name: "Attic".into(), ..state.map_grids()[0].clone() };
        state.put_custom_grid("Roof", renamed, true, false).unwrap();
        assert_eq!(state.doc.object_grid(solid), Some("Attic"));
        assert!(state.delete_custom_grid("attic", true).unwrap());
        assert_eq!(state.doc.object_grid(solid), None);
        assert!(state.map_grids().is_empty());
        assert!(state.doc.extra_block(GRIDS_BLOCK).is_none());
        assert!(state.doc.undo());
        assert_eq!(state.doc.object_grid(solid), Some("Attic"), "undo brings both back");
        assert_eq!(state.map_grids().len(), 1);
        let text = state.doc.to_vmf_string();
        let back = hp_editor::Document::from_map(hp_vmf::parse(&text).unwrap());
        assert_eq!(back.extra_block(GRIDS_BLOCK).map(grids_from_block).unwrap(), state.map_grids());
        assert_eq!(back.object_grid(solid), Some("Attic"));
        assert!(state.put_custom_grid("", GridSpec::uniform("attic", 4.0), true, false).is_ok(), "same name: replaced");
        state.put_custom_grid("", GridSpec::uniform("Other", 4.0), true, false).unwrap();
        assert!(state.put_custom_grid("Other", GridSpec::uniform("Attic", 4.0), true, false).is_err());
        assert_eq!(state.unique_grid_name("Attic"), "Attic 2");
    }

    #[test]
    fn presets_live_in_the_settings() {
        let (mut state, _) = state_with_box();
        let steps = state.doc.history_len().0;
        state.put_custom_grid("", GridSpec::uniform("Fine", 1.5), false, false).unwrap();
        assert!(state.settings_dirty);
        assert_eq!(state.doc.history_len().0, steps, "no map change");
        assert_eq!(state.find_grid("fine").unwrap().1, GridSource::Preset("Fine".into()));
        state.put_custom_grid("", GridSpec::uniform("Fine", 3.0), true, false).unwrap();
        assert_eq!(state.find_grid("Fine").unwrap().0.step, [3.0; 3]);
        assert_eq!(state.all_custom_grids().len(), 1);
        assert!(state.delete_custom_grid("Fine", false).unwrap());
        assert!(!state.delete_custom_grid("Fine", false).unwrap());
        assert!(state.use_custom_grid("nothing").is_err());
    }

    #[test]
    fn grids_fitted_to_a_rotated_box_and_a_face() {
        let (mut state, id) = state_with_box();
        let solid = ObjRef::Solid(id);
        let q = glam::DQuat::from_rotation_z(30f64.to_radians());
        state.doc.rotate(&[solid], DVec3::ZERO, q).unwrap();
        state.doc.select(&[solid], hp_editor::SelectOp::Replace);
        let spec = state.grid_from_selection("Box").unwrap();
        let frame = GridFrame::from_spec(&spec, 16.0);
        let corners: Vec<DVec3> =
            state.doc.scene().solid(id).unwrap().geom.faces.iter().flat_map(|f| f.vertices.clone()).collect();
        for c in &corners {
            assert!(close(frame.snap_point(*c + DVec3::new(0.4, -0.3, 0.2)), *c), "{c} {spec:?}");
        }
        let yaw = (spec.angles[1] - 30.0).rem_euclid(90.0);
        assert!(!(1e-6..=90.0 - 1e-6).contains(&yaw), "{spec:?}");
        let face = hp_editor::FaceRef { solid: id, side: 0 };
        state.doc.select_faces(&[face], hp_editor::SelectOp::Replace);
        let spec = state.grid_from_face("Face").unwrap();
        let frame = GridFrame::from_spec(&spec, 16.0);
        let faces = &state.doc.scene().solid(id).unwrap().geom.faces;
        let geom = faces.iter().find(|f| f.side_index == 0).unwrap().clone();
        assert!((frame.axes.z_axis - geom.normal()).length() < 1e-6);
        for v in &geom.vertices {
            assert!(close(frame.snap_point(*v), *v));
        }
        state.doc.clear_selection();
        assert!(state.grid_from_selection("x").is_none());
    }

    #[test]
    fn scaling_and_bad_input() {
        let g = GridFrame::uniform(16.0).scaled(0.5);
        assert_eq!(g.step, DVec3::splat(8.0));
        assert_eq!(GridFrame::uniform(16.0).scaled(f64::NAN).step, DVec3::splat(16.0));
        assert!(GridFrame::uniform(16.0).snap_point(DVec3::splat(f64::NAN)).x.is_nan());
        assert_eq!(GridFrame::uniform(f64::NAN).step.x, 16.0);
        let flat = Lattice2 { origin: DVec2::ZERO, a: DVec2::X, b: DVec2::X * 2.0 };
        assert_eq!(flat.snap(DVec2::new(0.3, 0.4)), DVec2::new(0.3, 0.4), "degenerate lattice: unchanged");
        assert_eq!(AxisSnap::OFF.snap(3.3), 3.3);
    }
}
