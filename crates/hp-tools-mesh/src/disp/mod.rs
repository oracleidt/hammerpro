pub mod blend;
mod brush_math;
mod brushes;
pub mod commands;
pub mod erode;
pub(crate) mod highlight;
pub mod loops;
pub(crate) mod paint;
mod relax;
mod resample;
pub mod roadblend;
mod script;
pub mod sculpt;
pub(crate) mod seams;
pub mod slope;
pub(crate) mod slope_ui;
pub(crate) mod smooth;
pub mod subdivision;
pub mod tags;
pub mod terrain_windows;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use egui::{Align2, Button, Color32, CursorIcon, FontId, Key, Modifiers, Pos2, RichText, Ui};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::state::AppState;
use hp_app::theme::{AXIS_Z, VIEW_ACCENT, VIEW_SUCCESS, VIEW_WARNING};
use hp_app::widgets::{grid_row, section, vec3_edit};
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, t, tf};
use hp_editor::{FaceRef, ObjRef, SelectOp};
use hp_geom::{Falloff, Ray};

use crate::common::{DocKey, edit_error_text, line_on_top, point, rgba, view_axis};
pub use paint::{AlphaEffect, BrushSettings, GeoEffect, Interaction, PaintAxis};
use paint::{
    ELEVATION_TAG, Sample, UNDO_ELEVATION, UNDO_STROKE, UP_FACING_Z, build_sample, count_disp_faces, destroy_targets,
    mesh_border, pick_disp, selected_disp_faces, surface_ring, visible_from,
};
pub use sculpt::BoundsLimit;
#[cfg(test)]
use seams::nearest_disp_face;
use seams::{UNDO_SEW, brush_faces, edge_neighbours, plan_live_sew, plan_sew};
pub use subdivision::SubdivisionShape;
pub use tags::{TagKind, TagSettings, TagValue};

const DRAG_START_PX: f32 = 3.0;
const HOLD_START_S: f64 = 0.2;
const UNDO_FLATTEN: &str = "Flatten displacement";
const UNDO_LIGHTMAP: &str = "Displacement lightmap scale";
const LIGHTMAP_TAG: &str = "hp_tools_mesh.disp_lightmap";
const RADIUS_DRAG_PX: f32 = 150.0;
pub(crate) const BLOCKED: Color32 = Color32::from_rgb(190, 190, 190);

#[derive(Clone, Copy, Debug)]
struct Press {
    view: usize,
    pos: Pos2,
    time: f64,
    moved: f32,
    sweep: bool,
    paints: bool,
    alt: bool,
}

#[derive(Clone, Debug)]
struct StrokeState {
    view: usize,
    filter: Vec<FaceRef>,
    last_frame: Option<u64>,
    changed: usize,
    painted: Vec<FaceRef>,
    sculpt: sculpt::SculptStroke,
    toward_eye: Option<DVec3>,
    toward_eye_flat: Option<DVec3>,
    neighbours: HashMap<FaceRef, Vec<FaceRef>>,
    told: bool,
    last_spot: Option<(Pos2, [u64; 5])>,
    grab_anchor: Option<(FaceRef, DVec3)>,
}

fn locked_neighbours(allowed: &[FaceRef], faces: &[FaceRef]) -> Vec<FaceRef> {
    if allowed.is_empty() {
        return Vec::new();
    }
    faces.iter().copied().filter(|f| !allowed.contains(f)).collect()
}

impl StrokeState {
    fn neighbours(&mut self, doc: &hp_editor::Document, face: FaceRef) -> &[FaceRef] {
        self.neighbours.entry(face).or_insert_with(|| edge_neighbours(doc, face))
    }
}

#[derive(Clone, Copy, Debug)]
struct RadiusDrag {
    view: usize,
    right: bool,
    start: Pos2,
    radius: f64,
}

#[derive(Clone, Debug)]
struct CursorState {
    view: usize,
    frame: u64,
    center: DVec3,
    normal: DVec3,
    ring: Vec<DVec3>,
    faces: Vec<FaceRef>,
    mods: Modifiers,
    blocked: bool,
    key: (Pos2, DocKey, u64),
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PointerFrame {
    pub pos: Option<Pos2>,
    pub pressed: bool,
    pub down: bool,
    pub mods: Modifiers,
    pub right_pressed: bool,
    pub right_down: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PanelAction {
    Create(u32),
    Destroy,
    SelectAll,
    ClearFaces,
    SetPower(u32),
    Flatten,
    Elevation(f64),
    Sew,
    InvertAlpha,
    Subdivide,
    Noise,
    Terrain(erode::TerrainOp),
    Blend,
    BlendSet(usize),
    BlendClear,
    Slope(slope::SlopeOp),
    SlopeMeasure,
    SlopeTake,
    SlopeSelect,
    PathFromEntities,
    PathClear,
    Flag(i32, bool),
    Shadows(bool),
    Scale(f64),
    SelectAdjacent,
    SelectOfSolids,
    Lightmap(i32),
    TagAll(TagKind, TagValue),
    LoopSmooth,
    LoopHeight,
    LoopClear,
}

pub struct DisplacementTool {
    pub interaction: Interaction,
    pub brush: BrushSettings,
    stroke: Option<StrokeState>,
    press: Option<Press>,
    radius_drag: Option<RadiusDrag>,
    scale_factor: f64,
    subdivision_shape: SubdivisionShape,
    subdivision_amount: f64,
    full_displacement: bool,
    pub terrain: erode::TerrainOptions,
    pub blend: blend::BlendOptions,
    pub slope: slope::SlopeOptions,
    blend_sets: [Vec<FaceRef>; 2],
    blend_preview: bool,
    preview_cache: slope_ui::PreviewCache,
    measured: Option<slope::Measure>,
    pick_path: bool,
    cursor: Option<CursorState>,
    hover_face: Option<(usize, FaceRef)>,
    disp_count: Cell<Option<(DocKey, usize)>>,
    last_warning: Option<(String, u64)>,
    pub(crate) toward_eye: Option<DVec3>,
    pub(crate) toward_eye_flat: Option<DVec3>,
    pub mask_grid: bool,
    pub mask_select: bool,
    pub keep_shape: bool,
    loops: loops::LoopState,
    loaded: bool,
}

impl Default for DisplacementTool {
    fn default() -> Self {
        Self {
            interaction: Interaction::Geometry,
            brush: BrushSettings::default(),
            stroke: None,
            press: None,
            radius_drag: None,
            scale_factor: 2.0,
            subdivision_shape: SubdivisionShape::Smooth,
            subdivision_amount: 1.0,
            full_displacement: false,
            terrain: erode::TerrainOptions::default(),
            blend: blend::BlendOptions::default(),
            slope: slope::SlopeOptions::default(),
            blend_sets: [Vec::new(), Vec::new()],
            blend_preview: true,
            preview_cache: RefCell::new(None),
            measured: None,
            pick_path: false,
            cursor: None,
            hover_face: None,
            disp_count: Cell::new(None),
            last_warning: None,
            toward_eye: None,
            toward_eye_flat: None,
            mask_grid: false,
            mask_select: false,
            keep_shape: true,
            loops: loops::LoopState::default(),
            loaded: false,
        }
    }
}

impl DisplacementTool {
    fn right_drag_resizes(&self, kind: hp_app::ViewKind) -> bool {
        kind == hp_app::ViewKind::Perspective && self.interaction != Interaction::Select
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_painting(&self) -> bool {
        self.stroke.is_some()
    }

    pub(crate) fn needs_frames(&self) -> bool {
        self.stroke.is_some() || self.press.is_some_and(|p| p.paints)
    }

    #[cfg(test)]
    pub(crate) fn cursor_blocked(&self) -> bool {
        self.cursor.as_ref().is_some_and(|c| c.blocked)
    }

    fn disp_count(&self, state: &AppState) -> usize {
        let rev = DocKey::of(&state.doc);
        match self.disp_count.get() {
            Some((r, n)) if r == rev => n,
            _ => {
                let n = count_disp_faces(&state.doc);
                self.disp_count.set(Some((rev, n)));
                n
            }
        }
    }

    fn warn(&mut self, state: &mut AppState, text: String) {
        let frame = state.frame.index;
        let repeated = self.last_warning.as_ref().is_some_and(|(t, f)| *t == text && frame.saturating_sub(*f) < 120);
        if repeated {
            state.log(Severity::Warning, text.clone());
        } else {
            state.notify(Severity::Warning, text.clone());
        }
        self.last_warning = Some((text, frame));
    }

    pub fn create(&mut self, state: &mut AppState, power: u32) -> usize {
        let targets = paint::create_targets_mode(&state.doc, self.full_displacement);
        if targets.faces.is_empty() {
            let text = if targets.skipped > 0 {
                tf!("disp.skipped", targets.skipped)
            } else {
                t!("disp.nothing_to_create").to_owned()
            };
            self.warn(state, text);
            return 0;
        }
        match state.doc.create_displacements(&targets.faces, power) {
            Ok(n) => {
                state.doc.select_faces(&targets.faces, SelectOp::Replace);
                state.notify(Severity::Success, tf!("disp.created", n, power));
                if targets.skipped > 0 {
                    let text = tf!("disp.skipped", targets.skipped);
                    self.warn(state, text);
                }
                n
            }
            Err(e) => {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
                0
            }
        }
    }

    pub fn destroy(&mut self, state: &mut AppState) -> usize {
        let faces = destroy_targets(&state.doc);
        if faces.is_empty() {
            let text = t!("disp.nothing_to_destroy").to_owned();
            self.warn(state, text);
            return 0;
        }
        match state.doc.destroy_displacements(&faces) {
            Ok(n) => {
                state.notify(Severity::Success, tf!("disp.destroyed", n));
                n
            }
            Err(e) => {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
                0
            }
        }
    }

    pub fn select_all(&mut self, state: &mut AppState) -> usize {
        let faces = state.doc.disp_faces();
        state.doc.select_faces(&faces, SelectOp::Replace);
        faces.len()
    }

    pub fn set_power(&mut self, state: &mut AppState, power: u32) -> usize {
        let faces = selected_disp_faces(&state.doc);
        if faces.is_empty() {
            return 0;
        }
        match state.doc.create_displacements(&faces, power) {
            Ok(n) => n,
            Err(e) => {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
                0
            }
        }
    }

    pub fn flatten(&mut self, state: &mut AppState) -> usize {
        let faces = selected_disp_faces(&state.doc);
        let mut by_power: Vec<(u32, Vec<FaceRef>)> = Vec::new();
        for f in faces {
            let Some(power) = state.doc.face_side(f).and_then(|s| s.disp.as_ref()).map(|d| d.power) else { continue };
            match by_power.iter_mut().find(|(p, _)| *p == power) {
                Some((_, list)) => list.push(f),
                None => by_power.push((power, vec![f])),
            }
        }
        if by_power.is_empty() {
            return 0;
        }
        let result = state.doc.transaction(UNDO_FLATTEN, |doc| {
            let mut total = 0;
            for (power, list) in &by_power {
                total += doc.create_displacements(list, (*power).clamp(2, 4))?;
            }
            Ok::<usize, hp_editor::EditError>(total)
        });
        match result {
            Ok(n) => n,
            Err(e) => {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
                0
            }
        }
    }

    pub fn sew(&mut self, state: &mut AppState) -> usize {
        let faces = destroy_targets(&state.doc);
        if faces.len() < 2 {
            let text = t!("disp.sew_disabled").to_owned();
            self.warn(state, text);
            return 0;
        }
        let plan = plan_sew(&state.doc, &faces, &[]);
        if plan.updates.is_empty() {
            state.notify(Severity::Info, t!("disp.sew_nothing"));
            return 0;
        }
        let mut by_solid: Vec<(hp_editor::Id, Vec<(usize, hp_vmf::DispInfo)>)> = Vec::new();
        for (f, d) in plan.updates {
            match by_solid.iter_mut().find(|(id, _)| *id == f.solid) {
                Some((_, list)) => list.push((f.side, d)),
                None => by_solid.push((f.solid, vec![(f.side, d)])),
            }
        }
        let result = state.doc.transaction(UNDO_SEW, |doc| {
            for (id, list) in by_solid {
                doc.edit_solid(id, |s| {
                    for (side, d) in list {
                        if let Some(sd) = s.sides.get_mut(side) {
                            sd.disp = Some(Box::new(d));
                        }
                    }
                })?;
            }
            Ok::<(), hp_editor::EditError>(())
        });
        match result {
            Ok(()) => {
                state.notify(Severity::Success, tf!("disp.sewn", plan.vertices));
                plan.vertices
            }
            Err(e) => {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
                0
            }
        }
    }

    pub fn set_elevation(&mut self, state: &mut AppState, value: f64) -> usize {
        if !value.is_finite() {
            return 0;
        }
        let faces = selected_disp_faces(&state.doc);
        let mut by_solid: Vec<(hp_editor::Id, Vec<usize>)> = Vec::new();
        for f in faces {
            match by_solid.iter_mut().find(|(id, _)| *id == f.solid) {
                Some((_, list)) => list.push(f.side),
                None => by_solid.push((f.solid, vec![f.side])),
            }
        }
        if by_solid.is_empty() {
            return 0;
        }
        let doc = &mut state.doc;
        doc.begin_merge(UNDO_ELEVATION, ELEVATION_TAG);
        let mut count = 0;
        for (id, sides) in &by_solid {
            let r = doc.edit_solid(*id, |s| {
                for side in sides {
                    if let Some(d) = s.sides.get_mut(*side).and_then(|s| s.disp.as_mut()) {
                        d.elevation = value;
                    }
                }
            });
            match r {
                Ok(_) => count += sides.len(),
                Err(e) => {
                    doc.cancel();
                    let text = tf!("disp.failed", edit_error_text(&e));
                    self.warn(state, text);
                    return 0;
                }
            }
        }
        doc.commit();
        count
    }

    pub fn select_adjacent(&mut self, state: &mut AppState) -> usize {
        let selected = selected_disp_faces(&state.doc);
        let mut add: Vec<FaceRef> = Vec::new();
        for f in &selected {
            for g in edge_neighbours(&state.doc, *f) {
                if !selected.contains(&g) && !add.contains(&g) {
                    add.push(g);
                }
            }
        }
        if add.is_empty() {
            state.notify(Severity::Info, t!("disp.select_adjacent_none"));
            return 0;
        }
        state.doc.select_faces(&add, SelectOp::Add);
        add.len()
    }

    pub fn select_of_solids(&mut self, state: &mut AppState) -> usize {
        let ids = state.doc.selected_solid_ids();
        let faces: Vec<FaceRef> = state.doc.disp_faces().into_iter().filter(|f| ids.contains(&f.solid)).collect();
        if faces.is_empty() {
            state.notify(Severity::Info, t!("disp.select_of_solids_none"));
            return 0;
        }
        state.doc.select_faces(&faces, SelectOp::Replace);
        faces.len()
    }

    pub fn set_lightmap_scale(&mut self, state: &mut AppState, value: i32) -> usize {
        let value = value.clamp(1, 1024);
        let faces = selected_disp_faces(&state.doc);
        let mut by_solid: Vec<(hp_editor::Id, Vec<usize>)> = Vec::new();
        for f in faces {
            if state.doc.face_side(f).is_some_and(|s| s.lightmap_scale == value) {
                continue;
            }
            match by_solid.iter_mut().find(|(id, _)| *id == f.solid) {
                Some((_, list)) => list.push(f.side),
                None => by_solid.push((f.solid, vec![f.side])),
            }
        }
        if by_solid.is_empty() {
            return 0;
        }
        let doc = &mut state.doc;
        doc.begin_merge(UNDO_LIGHTMAP, LIGHTMAP_TAG);
        let mut count = 0;
        for (id, sides) in &by_solid {
            let r = doc.edit_solid(*id, |s| {
                for side in sides {
                    if let Some(s) = s.sides.get_mut(*side) {
                        s.lightmap_scale = value;
                    }
                }
            });
            match r {
                Ok(_) => count += sides.len(),
                Err(e) => {
                    doc.cancel();
                    let text = tf!("disp.failed", edit_error_text(&e));
                    self.warn(state, text);
                    return 0;
                }
            }
        }
        doc.commit();
        count
    }

    pub fn tag_selection(&mut self, state: &mut AppState, kind: TagKind, value: TagValue) -> usize {
        let faces = destroy_targets(&state.doc);
        if faces.is_empty() {
            state.notify(Severity::Info, t!("disp.nothing_selected"));
            return 0;
        }
        match tags::tag_all(&mut state.doc, &faces, kind, value, tags::UNDO_TAG) {
            Ok(n) => {
                state.notify(Severity::Success, tf!("disp.tagged", n));
                n
            }
            Err(e) => {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
                0
            }
        }
    }

    pub(crate) fn sync_settings(&mut self, state: &mut AppState) {
        if !self.loaded {
            self.loaded = true;
            let saved = state.settings.mesh.disp_tool.clone();
            for (k, v) in &saved {
                self.script_set_option(state, k, v);
            }
        }
        self.save_settings(state);
    }

    pub(crate) fn save_settings(&self, state: &mut AppState) {
        if !self.loaded {
            return;
        }
        let current: std::collections::BTreeMap<String, String> =
            self.script_option_values().into_iter().map(|o| (o.key.to_owned(), o.value)).collect();
        if current != state.settings.mesh.disp_tool {
            state.settings.mesh.disp_tool = current;
            state.settings_dirty = true;
        }
    }

    fn highlight_opts(&self) -> highlight::HighlightOpts {
        highlight::HighlightOpts { grid: !self.mask_grid, tint: !self.mask_select, removed: true }
    }

    pub(crate) fn begin_stroke(&mut self, state: &mut AppState, view: usize) {
        let filter = if self.brush.selected_only { selected_disp_faces(&state.doc) } else { Vec::new() };
        let label = match self.interaction {
            Interaction::Sculpt => paint::UNDO_SCULPT,
            Interaction::Tag => tags::UNDO_TAG,
            _ => UNDO_STROKE,
        };
        self.begin_stroke_with(state, view, filter, label.to_owned());
    }

    pub(crate) fn begin_stroke_with(&mut self, state: &mut AppState, view: usize, filter: Vec<FaceRef>, label: String) {
        if self.stroke.is_some() {
            self.end_stroke(state);
        }
        state.doc.begin(label);
        self.stroke = Some(StrokeState {
            view,
            filter,
            last_frame: None,
            changed: 0,
            painted: Vec::new(),
            sculpt: sculpt::SculptStroke::default(),
            toward_eye: None,
            toward_eye_flat: None,
            neighbours: HashMap::new(),
            told: false,
            last_spot: None,
            grab_anchor: None,
        });
    }

    pub(crate) fn end_stroke(&mut self, state: &mut AppState) {
        let Some(mut stroke) = self.stroke.take() else { return };
        if self.brush.autosew && stroke.changed > 0 && !stroke.painted.is_empty() {
            let mut faces = stroke.painted.clone();
            for f in stroke.painted.clone() {
                for g in stroke.neighbours(&state.doc, f) {
                    if !faces.contains(g) {
                        faces.push(*g);
                    }
                }
            }
            let locked = locked_neighbours(&stroke.filter, &faces);
            let plan = plan_sew(&state.doc, &faces, &locked);
            if !plan.updates.is_empty()
                && let Err(e) = state.doc.set_disp_data(UNDO_SEW, plan.updates)
            {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
            }
        }
        state.doc.commit();
    }

    fn cancel_stroke(&mut self, state: &mut AppState) -> bool {
        if self.stroke.take().is_some() {
            state.doc.cancel();
            return true;
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn paint_at(&mut self, state: &mut AppState, center: DVec3, invert: bool) -> usize {
        let mods = if self.interaction == Interaction::Sculpt || self.relaxing() {
            Modifiers { ctrl: invert, command: invert, ..Modifiers::default() }
        } else {
            Modifiers { shift: invert, ..Modifiers::default() }
        };
        self.paint_at_mods(state, center, mods)
    }

    #[cfg(test)]
    pub(crate) fn paint_at_mods(&mut self, state: &mut AppState, center: DVec3, mods: Modifiers) -> usize {
        match nearest_disp_face(&state.doc, center) {
            Some(face) => self.paint_on(state, face, center, mods, None),
            None => 0,
        }
    }

    fn inverts(&self, mods: Modifiers) -> bool {
        if self.relaxing() {
            return false;
        }
        match self.interaction {
            Interaction::Sculpt => mods.ctrl || mods.command,
            _ => mods.shift,
        }
    }

    fn relaxing(&self) -> bool {
        (self.interaction == Interaction::Geometry && self.brush.geo_effect == GeoEffect::Relax)
            || (self.interaction == Interaction::Sculpt && self.brush.sculpt.op == sculpt::SculptOp::Relax)
    }

    fn paint_on(
        &mut self,
        state: &mut AppState,
        face: FaceRef,
        center: DVec3,
        mods: Modifiers,
        eye: Option<DVec3>,
    ) -> usize {
        self.brush.sanitize();
        let invert = self.inverts(mods);
        let current = (self.toward_eye, self.toward_eye_flat);
        let (toward_eye, toward_eye_flat) = match self.stroke.as_mut() {
            Some(s) => {
                if s.toward_eye.is_none() && s.toward_eye_flat.is_none() {
                    (s.toward_eye, s.toward_eye_flat) = current;
                }
                (s.toward_eye, s.toward_eye_flat)
            }
            None => current,
        };
        let mut brush = self.brush.resolved_view(toward_eye, toward_eye_flat, center_normal(&state.doc, face, center));
        if self.relaxing() {
            brush.sculpt.op = sculpt::SculptOp::Relax;
        }
        let sample = match self.interaction {
            _ if mods.shift && (self.interaction == Interaction::Sculpt || self.relaxing()) => {
                let mut smooth = brush.clone();
                smooth.geo_effect = GeoEffect::Smooth;
                build_sample(&smooth, Interaction::Geometry, center, false, state.frame.dt)
            }
            Interaction::Sculpt | Interaction::Tag => None,
            _ => build_sample(&brush, self.interaction, center, invert, state.frame.dt),
        };
        let own_sample = self.relaxing() || matches!(self.interaction, Interaction::Sculpt | Interaction::Tag);
        if sample.is_none() && !own_sample {
            return 0;
        }
        if sample.is_none()
            && self.interaction == Interaction::Sculpt
            && brush.sculpt.op == sculpt::SculptOp::Project
            && sculpt::project_targets(&state.doc).is_empty()
        {
            self.tell_no_project_target(state);
            return 0;
        }
        let mut filter = self.brush_targets(&state.doc, face, center, eye, &brush);
        if self.interaction == Interaction::Sculpt
            && brush.sculpt.op == sculpt::SculptOp::Grab
            && let Some(stroke) = self.stroke.as_ref()
        {
            for f in stroke.sculpt.brush.grabbed_faces() {
                if !filter.contains(&f) {
                    filter.push(f);
                }
            }
        }
        if filter.is_empty() {
            self.tell_unselected(state);
            return 0;
        }
        let live_sew = brush.autosew && matches!(self.interaction, Interaction::Geometry | Interaction::Sculpt);
        let before: HashMap<FaceRef, Vec<DVec3>> = if live_sew {
            filter
                .iter()
                .filter_map(|f| {
                    let mesh = state.doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side))?;
                    Some((*f, mesh.positions.clone()))
                })
                .collect()
        } else {
            HashMap::new()
        };
        let dt = f64::from(state.frame.dt);
        let pinned = match sample {
            Some(Sample::Local(b))
                if matches!(b.op, paint::LocalOp::Smooth { .. } | paint::LocalOp::AlphaSmooth { .. }) =>
            {
                self.smooth_pins(&state.doc, &filter, brush.autosew)
            }
            _ => Vec::new(),
        };
        let result = match (sample, self.stroke.as_mut()) {
            (Some(Sample::Stock(b)), _) => state.doc.disp_paint(&filter, &b),
            (Some(Sample::Local(b)), _) => paint::paint_local(&mut state.doc, &filter, &pinned, &b),
            (None, _) if self.interaction == Interaction::Tag => {
                let b = tags::TagBrush {
                    center,
                    radius: brush.radius,
                    spatial: brush.spatial,
                    kind: brush.tag.kind,
                    value: brush.tag.effective(invert),
                };
                tags::tag_sample(&mut state.doc, &filter, &b)
            }
            (None, Some(stroke)) => sculpt::sculpt_sample(
                &mut state.doc,
                &filter,
                center,
                &brush,
                &brush.sculpt,
                &mut stroke.sculpt,
                invert,
                dt,
            ),
            (None, None) => Ok(0),
        };
        let n = match result {
            Ok(n) => n,
            Err(e) => {
                let text = tf!("disp.failed", edit_error_text(&e));
                self.warn(state, text);
                return 0;
            }
        };
        let mut touched = filter;
        if n > 0 && !before.is_empty() {
            match self.live_sew(state, &touched, &before) {
                Ok(sewn) => {
                    for f in sewn {
                        if !touched.contains(&f) {
                            touched.push(f);
                        }
                    }
                }
                Err(e) => {
                    let text = tf!("disp.failed", edit_error_text(&e));
                    self.warn(state, text);
                }
            }
        }
        if let Some(s) = self.stroke.as_mut() {
            s.changed += n;
            if n > 0 {
                for f in touched {
                    if !s.painted.contains(&f) {
                        s.painted.push(f);
                    }
                }
            }
        }
        n
    }

    fn tell_unselected(&mut self, state: &mut AppState) {
        let Some(stroke) = self.stroke.as_mut() else { return };
        if stroke.filter.is_empty() || stroke.told {
            return;
        }
        stroke.told = true;
        let text = tf!("disp.paint_not_selected", stroke.filter.len(), key_label(Key::Escape));
        self.warn(state, text);
    }

    fn tell_no_project_target(&mut self, state: &mut AppState) {
        let Some(stroke) = self.stroke.as_mut() else { return };
        if stroke.told {
            return;
        }
        stroke.told = true;
        self.warn(state, t!("disp.sculpt.project_none").to_owned());
    }

    fn smooth_pins(&mut self, doc: &hp_editor::Document, targets: &[FaceRef], autosew: bool) -> Vec<FaceRef> {
        let allowed: Vec<FaceRef> = self.stroke.as_ref().map(|s| s.filter.clone()).unwrap_or_default();
        let mut out: Vec<FaceRef> = Vec::new();
        for f in targets {
            for g in self.neighbours_of(doc, *f) {
                if targets.contains(&g) || out.contains(&g) {
                    continue;
                }
                let locked = !allowed.is_empty() && !allowed.contains(&g);
                if !autosew || locked {
                    out.push(g);
                }
            }
        }
        out
    }

    fn neighbours_of(&mut self, doc: &hp_editor::Document, face: FaceRef) -> Vec<FaceRef> {
        match self.stroke.as_mut() {
            Some(s) => s.neighbours(doc, face).to_vec(),
            None => edge_neighbours(doc, face),
        }
    }

    fn brush_targets(
        &mut self,
        doc: &hp_editor::Document,
        face: FaceRef,
        center: DVec3,
        eye: Option<DVec3>,
        brush: &BrushSettings,
    ) -> Vec<FaceRef> {
        let candidates = brush_faces(doc, face, center, brush.radius, brush.spatial, &[]);
        let allowed = self.stroke.as_ref().map(|s| s.filter.as_slice()).unwrap_or_default();
        let mut seeds: Vec<FaceRef> =
            candidates.iter().copied().filter(|f| allowed.is_empty() || allowed.contains(f)).collect();
        if brush.line_of_sight
            && let Some(eye) = eye
        {
            seeds.retain(|f| *f == face || visible_from(doc, *f, eye, center));
        }
        if !brush.autosew || !allowed.is_empty() || seeds.is_empty() || seeds.len() == candidates.len() {
            return seeds;
        }
        let under: HashSet<FaceRef> = candidates.iter().copied().collect();
        let mut kept: HashSet<FaceRef> = seeds.iter().copied().collect();
        let mut queue = seeds;
        while let Some(f) = queue.pop() {
            for g in self.neighbours_of(doc, f) {
                if under.contains(&g) && kept.insert(g) {
                    queue.push(g);
                }
            }
        }
        candidates.into_iter().filter(|f| kept.contains(f)).collect()
    }

    fn live_sew(
        &mut self,
        state: &mut AppState,
        painted: &[FaceRef],
        before: &HashMap<FaceRef, Vec<DVec3>>,
    ) -> hp_editor::EditResult<Vec<FaceRef>> {
        let mut faces = painted.to_vec();
        for f in painted {
            for g in self.neighbours_of(&state.doc, *f) {
                if !faces.contains(&g) {
                    faces.push(g);
                }
            }
        }
        let allowed = self.stroke.as_ref().map(|s| s.filter.as_slice()).unwrap_or_default();
        let locked = locked_neighbours(allowed, &faces);
        let plan = plan_live_sew(&state.doc, &faces, before, &locked);
        if plan.updates.is_empty() {
            return Ok(Vec::new());
        }
        let sewn: Vec<FaceRef> = plan.updates.iter().map(|(f, _)| *f).collect();
        state.doc.set_disp_data(UNDO_SEW, plan.updates)?;
        Ok(sewn)
    }

    fn paint_at_screen(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        let frame = state.frame.index;
        if self.stroke.as_ref().is_some_and(|s| s.last_frame == Some(frame)) {
            return;
        }
        let (o, d) = view.ray(pos);
        let grabbing =
            self.interaction == Interaction::Sculpt && self.brush.sculpt.op == sculpt::SculptOp::Grab && !mods.shift;
        let anchor = self.stroke.as_ref().and_then(|s| s.grab_anchor).filter(|_| grabbing);
        let (face, point) = match anchor {
            Some((face, origin)) => {
                let forward = if view.is_2d() { -view.kind.toward_viewer() } else { view.cam3d.forward() };
                let Some(p) = brush_math::view_plane_point(o, d, origin, forward) else { return };
                (face, p)
            }
            None => {
                let Some(hit) = pick_disp(&state.doc, &Ray::new(o, d)) else { return };
                if grabbing && let Some(s) = self.stroke.as_mut() {
                    s.grab_anchor = Some((hit.face, hit.point));
                }
                (hit.face, hit.point)
            }
        };
        let continuous = self.brush.continuous || self.relaxing();
        let cam = &view.cam3d;
        let spot = (pos, [cam.pos.x, cam.pos.y, cam.pos.z, cam.yaw_deg, cam.pitch_deg].map(f64::to_bits));
        if let Some(s) = self.stroke.as_mut() {
            if !continuous && s.last_spot == Some(spot) {
                return;
            }
            s.last_frame = Some(frame);
            s.last_spot = Some(spot);
        }
        self.look_from(&view.cam3d);
        self.paint_on(state, face, point, mods, Some(view.cam3d.pos));
        state.hover_world = Some(point);
    }

    fn cursor_inputs(&self, view: &ViewInfo) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        let cam = &view.cam3d;
        for v in [cam.pos.x, cam.pos.y, cam.pos.z, cam.yaw_deg, cam.pitch_deg, cam.fov_y_deg, self.brush.radius] {
            v.to_bits().hash(&mut h);
        }
        for v in [view.rect.min.x, view.rect.min.y, view.rect.max.x, view.rect.max.y] {
            v.to_bits().hash(&mut h);
        }
        self.brush.spatial.hash(&mut h);
        h.finish()
    }

    fn update_cursor(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) {
        self.look_from(&view.cam3d);
        let key = (pos, DocKey::of(&state.doc), self.cursor_inputs(view));
        let selected = selected_disp_faces(&state.doc);
        let blocked = |faces: &[FaceRef]| {
            self.interaction != Interaction::Select
                && self.brush.selected_only
                && !selected.is_empty()
                && !faces.iter().any(|f| selected.contains(f))
        };
        if let Some(c) = self.cursor.as_ref()
            && c.view == view.index
            && c.key == key
        {
            let blocked = blocked(&c.faces);
            if let Some(c) = self.cursor.as_mut() {
                c.frame = state.frame.index;
                c.mods = mods;
                c.blocked = blocked;
            }
            return;
        }
        let (o, d) = view.ray(pos);
        match pick_disp(&state.doc, &Ray::new(o, d)) {
            Some(hit) => {
                let normal = self
                    .brush
                    .resolved_view(
                        self.toward_eye,
                        self.toward_eye_flat,
                        center_normal(&state.doc, hit.face, hit.point),
                    )
                    .axis_vector()
                    .unwrap_or(hit.face_normal);
                let (ring, faces) = if self.interaction == Interaction::Select {
                    (Vec::new(), Vec::new())
                } else {
                    let radius = self.brush.radius;
                    let faces = brush_faces(&state.doc, hit.face, hit.point, radius, self.brush.spatial, &[]);
                    (surface_ring(&state.doc, &faces, hit.point, hit.face_normal, radius), faces)
                };
                let blocked = blocked(&faces);
                self.cursor = Some(CursorState {
                    view: view.index,
                    frame: state.frame.index,
                    center: hit.point,
                    normal,
                    ring,
                    faces,
                    mods,
                    blocked,
                    key,
                });
                self.hover_face = Some((view.index, hit.face));
                state.hover_world = Some(hit.point);
            }
            None => {
                self.cursor = None;
                self.hover_face = None;
            }
        }
    }

    pub(crate) fn click_select(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, ctrl: bool, shift: bool) {
        let op = if ctrl || shift { SelectOp::Toggle } else { SelectOp::Replace };
        let mut faces = pick_faces(state, view, pos);
        if shift && let Some(id) = faces.first().map(|f| f.solid) {
            faces = solid_faces(state, id);
        }
        faces = with_groups(state, faces);
        if faces.is_empty() {
            if !ctrl {
                clear_targets(state);
            }
            return;
        }
        if op != SelectOp::Replace {
            adopt_object_targets(state);
        }
        state.doc.select_faces(&faces, op);
    }

    pub(crate) fn look_from(&mut self, cam: &hp_app::view::Camera3d) {
        self.toward_eye = Some(-cam.forward());
        self.toward_eye_flat = paint::flat_toward_camera(cam);
    }

    pub(crate) fn axis_from_face(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2) -> Option<DVec3> {
        let (o, d) = view.ray(pos);
        let opts = hp_editor::PickOptions { include_entities: false, ..Default::default() };
        let hit = state.view_pick(&Ray::new(o, d), &opts)?;
        let (ObjRef::Solid(id), Some(side)) = (hit.obj, hit.face) else { return None };
        let face = FaceRef::new(id, side);
        let normal = center_normal(&state.doc, face, hit.point)
            .or_else(|| state.doc.scene().solid(id)?.geom.face_for_side(side).map(|g| g.normal()))?;
        let normal = normal.try_normalize()?;
        self.brush.custom_axis = normal;
        self.brush.axis = PaintAxis::Custom;
        self.cursor = None;
        let text = tf!("disp.axis_picked", format!("{:.2} {:.2} {:.2}", normal.x, normal.y, normal.z));
        state.notify(Severity::Info, text);
        Some(normal)
    }

    pub(crate) fn handle_pointer(&mut self, state: &mut AppState, view: &ViewInfo, f: PointerFrame) -> ToolFlow {
        let ctrl = f.mods.command || f.mods.ctrl;
        if !view.is_2d() {
            match f.pos {
                Some(p) => {
                    let at = self.radius_drag.filter(|r| r.view == view.index).map_or(p, |r| r.start);
                    self.update_cursor(state, view, at, f.mods);
                }
                None => {
                    if self.cursor.as_ref().is_some_and(|c| c.view == view.index) {
                        self.cursor = None;
                        self.hover_face = None;
                    }
                }
            }
        }
        if f.pressed && f.pos.is_some_and(|p| view.rect.contains(p)) {
            self.end_stroke(state);
            self.press = None;
            self.radius_drag = None;
        }
        if let Some(r) = self.radius_drag.filter(|r| r.view == view.index) {
            if (r.right && f.right_down) || (!r.right && f.down) {
                if let Some(p) = f.pos {
                    let factor = 2f64.powf(f64::from((p.x - r.start.x) / RADIUS_DRAG_PX));
                    self.brush.radius = (r.radius * factor).clamp(1.0, 8192.0).round().max(1.0);
                    if !view.is_2d() {
                        self.update_cursor(state, view, r.start, f.mods);
                    }
                }
            } else {
                self.radius_drag = None;
                let still = f.pos.is_none_or(|p| p.distance(r.start) <= DRAG_START_PX * 2.0);
                if !r.right && still && r.radius == self.brush.radius {
                    self.loop_click(state, view, r.start);
                }
            }
            return ToolFlow::Handled;
        }
        if f.pressed
            && f.mods.alt
            && !view.is_2d()
            && self.interaction != Interaction::Select
            && let Some(p) = f.pos.filter(|p| view.rect.contains(*p))
        {
            self.radius_drag = Some(RadiusDrag { view: view.index, right: false, start: p, radius: self.brush.radius });
            return ToolFlow::Handled;
        }
        if f.right_pressed
            && f.mods.alt
            && !view.is_2d()
            && self.stroke.is_none()
            && let Some(p) = f.pos.filter(|p| view.rect.contains(*p))
        {
            self.axis_from_face(state, view, p);
            return ToolFlow::Handled;
        }
        if f.right_pressed
            && self.right_drag_resizes(view.kind)
            && self.stroke.is_none()
            && let Some(p) = f.pos.filter(|p| view.rect.contains(*p))
        {
            self.radius_drag = Some(RadiusDrag { view: view.index, right: true, start: p, radius: self.brush.radius });
            return ToolFlow::Handled;
        }
        if self.stroke.as_ref().is_some_and(|s| s.view == view.index) {
            if f.down {
                if let Some(p) = f.pos {
                    self.paint_at_screen(state, view, p, f.mods);
                }
            } else {
                self.end_stroke(state);
            }
            return ToolFlow::Handled;
        }
        if let Some(mut press) = self.press.filter(|p| p.view == view.index) {
            if f.down {
                if let Some(p) = f.pos {
                    press.moved = press.moved.max(p.distance(press.pos));
                }
                let painting = self.interaction != Interaction::Select && !view.is_2d() && !self.pick_path;
                if !painting && press.moved > DRAG_START_PX {
                    if let Some(p) = f.pos {
                        let faces = with_groups(state, pick_faces(state, view, p));
                        if !faces.is_empty() {
                            let op =
                                if press.sweep || ctrl || f.mods.shift { SelectOp::Add } else { SelectOp::Replace };
                            let new = !press.sweep || faces.iter().any(|x| !state.doc.selection().faces.contains(x));
                            if new {
                                if op != SelectOp::Replace {
                                    adopt_object_targets(state);
                                }
                                state.doc.select_faces(&faces, op);
                            }
                            press.sweep = true;
                        }
                    }
                    self.press = Some(press);
                    return ToolFlow::Handled;
                }
                self.press = Some(press);
                let held = state.frame.time - press.time >= HOLD_START_S;
                if painting && (press.moved > DRAG_START_PX || held) {
                    self.press = None;
                    self.begin_stroke(state, view.index);
                    let at = f.pos.unwrap_or(press.pos);
                    self.paint_at_screen(state, view, at, f.mods);
                }
                return ToolFlow::Handled;
            }
            self.press = None;
            if press.moved <= DRAG_START_PX * 2.0 && !press.sweep {
                if press.alt {
                    self.loop_click(state, view, press.pos);
                } else {
                    if self.pick_path && !view.is_2d() {
                        self.pick_path_point(state, view, press.pos);
                    } else {
                        self.click_select(state, view, press.pos, ctrl, f.mods.shift);
                    }
                }
            }
            return ToolFlow::Handled;
        }
        if f.pressed
            && let Some(p) = f.pos
            && view.rect.contains(p)
        {
            let paints = self.interaction != Interaction::Select && !view.is_2d() && !self.pick_path;
            self.press = Some(Press {
                view: view.index,
                pos: p,
                time: state.frame.time,
                moved: 0.0,
                sweep: false,
                paints,
                alt: f.mods.alt,
            });
            return ToolFlow::Handled;
        }
        ToolFlow::Ignored
    }

    fn panel(&mut self, ui: &mut Ui, state: &mut AppState) -> Vec<PanelAction> {
        let mut actions = Vec::new();
        let total = self.disp_count(state);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(tf!("disp.count", total));
            });
        });
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            let modes = [
                (Interaction::Select, "disp.mode.select", "disp.mode.select_tip"),
                (Interaction::Geometry, "disp.mode.geometry", "disp.mode.geometry_tip"),
                (Interaction::Alpha, "disp.mode.alpha", "disp.mode.alpha_tip"),
                (Interaction::Sculpt, "disp.mode.sculpt", "disp.mode.sculpt_tip"),
                (Interaction::Tag, "disp.mode.tag", "disp.mode.tag_tip"),
            ];
            for (mode, label, tip) in modes {
                if ui.add(Button::selectable(self.interaction == mode, t!(label))).on_hover_text(t!(tip)).clicked() {
                    self.interaction = mode;
                    self.cursor = None;
                }
            }
        });

        section(ui, t!("disp.create"));
        ui.checkbox(&mut self.full_displacement, t!("disp.full_displacement"))
            .on_hover_text(t!("disp.full_displacement_tip"));
        let targets = paint::create_targets_mode(&state.doc, self.full_displacement);
        let target_text = if targets.faces.is_empty() {
            t!("disp.target_none").to_owned()
        } else if targets.from_faces || self.full_displacement {
            tf!("disp.target_faces", targets.faces.len())
        } else {
            tf!("disp.target_up", targets.faces.len())
        };
        ui.label(RichText::new(target_text).weak());
        if targets.skipped > 0 {
            ui.colored_label(hp_app::theme::warning(), tf!("disp.skipped", targets.skipped));
        }
        ui.horizontal(|ui| {
            ui.label(t!("disp.power"));
            for p in 2..=4u32 {
                let size = (1u32 << p) + 1;
                let r = ui
                    .add_enabled(!targets.faces.is_empty(), Button::new(p.to_string()).min_size(egui::vec2(34.0, 22.0)))
                    .on_hover_text(tf!("disp.create_tip", p, size))
                    .on_disabled_hover_text(t!("disp.target_none"));
                if r.clicked() {
                    actions.push(PanelAction::Create(p));
                }
            }
        });

        section(ui, t!("disp.selection"));
        let sel_disps = selected_disp_faces(&state.doc);
        let sel_faces =
            if object_targets_win(&state.doc) { sel_disps.len() } else { state.doc.selection().faces.len() };
        ui.label(RichText::new(tf!("disp.selected_faces", sel_disps.len(), sel_faces)).weak());
        ui.horizontal(|ui| {
            ui.label(t!("disp.selection_opacity"));
            let mut opacity = state.settings.mesh.displacement_selection_opacity() * 100.0;
            if ui
                .add(egui::Slider::new(&mut opacity, 0.0..=100.0).suffix(" %").max_decimals(1).step_by(0.1))
                .on_hover_text(t!("disp.selection_opacity_tip"))
                .changed()
            {
                state.settings.mesh.disp_selection_opacity = opacity / 100.0;
                state.settings_dirty = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(total > 0, Button::new(t!("disp.select_all")))
                .on_hover_text(t!("disp.select_all_tip"))
                .clicked()
            {
                actions.push(PanelAction::SelectAll);
            }
            if ui
                .add_enabled(sel_faces > 0, Button::new(t!("common.clear")))
                .on_hover_text(tf!("disp.clear_tip", key_label(Key::Escape)))
                .clicked()
            {
                actions.push(PanelAction::ClearFaces);
            }
            if ui
                .add_enabled(!sel_disps.is_empty(), Button::new(t!("disp.select_adjacent")))
                .on_hover_text(t!("disp.select_adjacent_tip"))
                .clicked()
            {
                actions.push(PanelAction::SelectAdjacent);
            }
            let solids = !state.doc.selection().objects.is_empty();
            if ui
                .add_enabled(solids, Button::new(t!("disp.select_of_solids")))
                .on_hover_text(t!("disp.select_of_solids_tip"))
                .clicked()
            {
                actions.push(PanelAction::SelectOfSolids);
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("disp.masks")).on_hover_text(t!("disp.masks_tip"));
            ui.checkbox(&mut self.mask_grid, t!("disp.mask_grid")).on_hover_text(t!("disp.mask_grid_tip"));
            ui.checkbox(&mut self.mask_select, t!("disp.mask_select")).on_hover_text(t!("disp.mask_select_tip"));
        });
        ui.horizontal_wrapped(|ui| {
            let targets = destroy_targets(&state.doc);
            let can_destroy = !targets.is_empty();
            let sew_count = targets.len();
            if ui
                .add_enabled(can_destroy, Button::new(t!("disp.destroy")))
                .on_hover_text(t!("disp.destroy_tip"))
                .on_disabled_hover_text(t!("disp.destroy_disabled"))
                .clicked()
            {
                actions.push(PanelAction::Destroy);
            }
            if ui
                .add_enabled(sew_count >= 2, Button::new(t!("disp.sew")))
                .on_hover_text(t!("disp.sew_tip"))
                .on_disabled_hover_text(t!("disp.sew_disabled"))
                .clicked()
            {
                actions.push(PanelAction::Sew);
            }
        });
        if !sel_disps.is_empty() {
            let infos: Vec<(u32, f64, i32)> = sel_disps
                .iter()
                .filter_map(|f| {
                    let side = state.doc.face_side(*f)?;
                    side.disp.as_ref().map(|d| (d.power, d.elevation, side.lightmap_scale))
                })
                .collect();
            let power = infos.first().map(|i| i.0).filter(|p| infos.iter().all(|i| i.0 == *p));
            let elevation = infos.first().map(|i| i.1).filter(|e| infos.iter().all(|i| i.1 == *e));
            let lightmap = infos.first().map(|i| i.2).filter(|l| infos.iter().all(|i| i.2 == *l));
            egui::Grid::new("hp_disp_face_props").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                grid_row(ui, t!("disp.power"), |ui| {
                    let text = power.map_or_else(|| t!("common.multiple_values").to_owned(), |p| p.to_string());
                    egui::ComboBox::from_id_salt("hp_disp_power")
                        .selected_text(text)
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            for p in 2..=4u32 {
                                if ui.selectable_label(power == Some(p), p.to_string()).clicked() && power != Some(p) {
                                    actions.push(PanelAction::SetPower(p));
                                }
                            }
                        })
                        .response
                        .on_hover_text(t!("disp.power_tip"));
                    ui.checkbox(&mut self.keep_shape, t!("disp.keep_shape")).on_hover_text(t!("disp.keep_shape_tip"));
                });
                grid_row(ui, t!("disp.elevation"), |ui| {
                    let mut value = elevation.unwrap_or(0.0);
                    let r = ui
                        .add(egui::DragValue::new(&mut value).speed(0.5).range(-4096.0..=4096.0).max_decimals(2))
                        .on_hover_text(t!("disp.elevation_tip"));
                    if r.changed() {
                        actions.push(PanelAction::Elevation(value));
                    }
                });
                grid_row(ui, t!("disp.lightmap_scale"), |ui| {
                    let mut value = lightmap.unwrap_or(16);
                    let r = ui
                        .add(egui::DragValue::new(&mut value).speed(0.25).range(1..=1024))
                        .on_hover_text(t!("disp.lightmap_scale_tip"));
                    if r.changed() {
                        actions.push(PanelAction::Lightmap(value));
                    }
                });
            });
            if ui.button(t!("disp.flatten")).on_hover_text(t!("disp.flatten_tip")).clicked() {
                actions.push(PanelAction::Flatten);
            }
        }

        match self.interaction {
            Interaction::Select => {
                section(ui, t!("disp.paint"));
                ui.label(RichText::new(t!("disp.select_hint")).weak());
            }
            Interaction::Geometry => self.geometry_ui(ui),
            Interaction::Alpha => self.alpha_ui(ui, state.settings.mesh.four_way_blends),
            Interaction::Sculpt => self.sculpt_ui(ui),
            Interaction::Tag => self.tag_ui(ui, state, &mut actions),
        }
        if self.interaction != Interaction::Select {
            self.brush_ui(ui);
            ui.add_space(4.0);
            let hint = match self.interaction {
                _ if self.relaxing() => t!("disp.hint_relax"),
                Interaction::Sculpt => t!("disp.hint_sculpt"),
                Interaction::Tag => t!("disp.hint_tag"),
                _ => t!("disp.paint_hint"),
            };
            ui.label(RichText::new(hint).weak());
        }
        self.tools_ui(ui, state, &mut actions);
        self.loop_ui(ui, state, &mut actions);
        actions
    }

    fn tools_ui(&mut self, ui: &mut Ui, state: &mut AppState, actions: &mut Vec<PanelAction>) {
        section(ui, t!("disp.tools"));
        let targets = commands::targets(&state.doc);
        let any = !targets.is_empty();
        ui.label(RichText::new(tf!("disp.tools_targets", targets.len())).weak());
        ui.horizontal_wrapped(|ui| {
            for (flag, label, tip) in commands::COLLISION_FLAGS {
                let current = commands::flag_state(&state.doc, &targets, flag);
                let mut on = current.unwrap_or(false);
                let r = ui
                    .add_enabled(any, egui::Checkbox::new(&mut on, t!(label)).indeterminate(any && current.is_none()));
                if r.on_hover_text(t!(tip)).clicked() {
                    actions.push(PanelAction::Flag(flag, current != Some(true)));
                }
            }
            let current = commands::cast_shadows_state(&state.doc, &targets);
            let mut on = current.unwrap_or(true);
            let r = ui.add_enabled(
                any,
                egui::Checkbox::new(&mut on, t!("disp.cast_shadows")).indeterminate(any && current.is_none()),
            );
            if r.on_hover_text(t!("disp.cast_shadows_tip")).clicked() {
                actions.push(PanelAction::Shadows(current != Some(true)));
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("disp.scale"));
            ui.add(egui::DragValue::new(&mut self.scale_factor).speed(0.05).range(-16.0..=16.0).max_decimals(3))
                .on_hover_text(t!("disp.scale_tip"));
            if ui
                .add_enabled(any && self.scale_factor != 1.0, Button::new(t!("common.apply")))
                .on_hover_text(t!("disp.scale_tip"))
                .clicked()
            {
                actions.push(PanelAction::Scale(self.scale_factor));
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(any, Button::new(t!("disp.invert_alpha")))
                .on_hover_text(t!("disp.invert_alpha_tip"))
                .clicked()
            {
                actions.push(PanelAction::InvertAlpha);
            }
        });
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("hp_disp_subdivision_shape")
                .selected_text(match self.subdivision_shape {
                    SubdivisionShape::Smooth => t!("disp.subdivision_smooth"),
                    SubdivisionShape::Sphere => t!("disp.subdivision_sphere"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.subdivision_shape,
                        SubdivisionShape::Smooth,
                        t!("disp.subdivision_smooth"),
                    );
                    ui.selectable_value(
                        &mut self.subdivision_shape,
                        SubdivisionShape::Sphere,
                        t!("disp.subdivision_sphere"),
                    );
                })
                .response
                .on_hover_text(t!("disp.subdivision_shape_tip"));
            ui.add(egui::Slider::new(&mut self.subdivision_amount, 0.0..=1.0).text(t!("disp.subdivision_amount")))
                .on_hover_text(t!("disp.subdivision_amount_tip"));
            if ui
                .add_enabled(any && self.subdivision_amount > 0.0, Button::new(t!("disp.subdivide")))
                .on_hover_text(t!("disp.subdivide_tip"))
                .clicked()
            {
                actions.push(PanelAction::Subdivide);
            }
        });
        if commands::noise_range_ui(ui, state) {
            state.settings_dirty = true;
        }
        self.noise_kind_ui(ui);
        if ui.add_enabled(any, Button::new(t!("disp.noise"))).on_hover_text(t!("disp.noise_tip")).clicked() {
            actions.push(PanelAction::Noise);
        }
        self.erosion_ui(ui, any, actions);
        self.blend_ui(ui, state, actions);
        self.slope_ui(ui, state, any, actions);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(any, Button::new(t!("disp.to_mesh"))).on_hover_text(t!("disp.to_mesh_tip")).clicked() {
                state.request(hp_app::Action::DispToMesh);
            }
            let m = &mut state.settings.mesh;
            let meters = ui
                .checkbox(&mut m.mesh_export_meters, t!("disp.to_mesh_meters"))
                .on_hover_text(t!("disp.to_mesh_meters_tip"));
            let z_up = ui
                .checkbox(&mut m.mesh_export_z_up, t!("disp.to_mesh_z_up"))
                .on_hover_text(t!("disp.to_mesh_z_up_tip"));
            if meters.changed() || z_up.changed() {
                state.settings_dirty = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("disp.mesh_to_disp")).on_hover_text(t!("disp.mesh_to_disp_tip")).clicked() {
                state.request(hp_app::Action::MeshToDisp);
            }
            if ui
                .add_enabled(any, Button::new(t!("disp.conform_mesh")))
                .on_hover_text(t!("disp.conform_mesh_tip"))
                .clicked()
            {
                state.request(hp_app::Action::DispConformMesh);
            }
            let tileable = !state.doc.selected_objects().is_empty() || !state.doc.selected_faces().is_empty();
            if ui
                .add_enabled(tileable, Button::new(t!("disp.tile_selection")))
                .on_hover_text(t!("disp.tile_selection_tip"))
                .clicked()
            {
                state.request(hp_app::Action::DispTileSelection);
            }
        });
    }

    fn geometry_ui(&mut self, ui: &mut Ui) {
        section(ui, t!("disp.paint_geometry"));
        ui.horizontal_wrapped(|ui| {
            let effects = [
                (GeoEffect::Raise, "disp.effect.raise", "disp.effect.raise_tip"),
                (GeoEffect::Lower, "disp.effect.lower", "disp.effect.lower_tip"),
                (GeoEffect::Smooth, "disp.effect.smooth", "disp.effect.smooth_tip"),
                (GeoEffect::Set, "disp.effect.set", "disp.effect.set_tip"),
                (GeoEffect::Relax, "disp.sculpt.relax", "disp.sculpt.relax_tip"),
            ];
            for (e, label, tip) in effects {
                if ui.add(Button::selectable(self.brush.geo_effect == e, t!(label))).on_hover_text(t!(tip)).clicked() {
                    self.brush.geo_effect = e;
                }
            }
        });
        egui::Grid::new("hp_disp_geo").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            match self.brush.geo_effect {
                GeoEffect::Relax => {
                    grid_row(ui, t!("disp.strength"), |ui| {
                        ui.add(egui::Slider::new(&mut self.brush.sculpt.relax_strength, 0.0..=1.0).max_decimals(2))
                            .on_hover_text(t!("disp.sculpt.relax_strength_tip"));
                    });
                }
                GeoEffect::Smooth => {
                    grid_row(ui, t!("disp.strength"), |ui| {
                        ui.add(egui::Slider::new(&mut self.brush.smooth, 0.0..=1.0).max_decimals(2))
                            .on_hover_text(t!("disp.smooth_tip"));
                    });
                }
                GeoEffect::Set => {
                    grid_row(ui, t!("disp.target_height"), |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.brush.target_height).speed(0.5).range(-16384.0..=16384.0),
                        )
                        .on_hover_text(t!("disp.target_height_tip"));
                    });
                }
                GeoEffect::Raise | GeoEffect::Lower => {
                    grid_row(ui, t!("disp.strength"), |ui| {
                        ui.add(
                            egui::Slider::new(&mut self.brush.strength, 1.0..=8192.0)
                                .logarithmic(true)
                                .max_decimals(0)
                                .suffix(t!("disp.per_second")),
                        )
                        .on_hover_text(t!("disp.strength_tip"));
                    });
                }
            }
            if self.brush.geo_effect != GeoEffect::Relax {
                self.axis_row(ui, true);
            }
        });
        if self.brush.geo_effect != GeoEffect::Relax && self.brush.axis == PaintAxis::Custom {
            vec3_edit(ui, &mut self.brush.custom_axis, 0.01);
        }
    }

    fn axis_row(&mut self, ui: &mut Ui, surface: bool) {
        grid_row(ui, t!("disp.axis"), |ui| {
            let label = |a: PaintAxis| match a {
                PaintAxis::FaceNormal => t!("disp.axis.normal"),
                PaintAxis::X => t!("disp.axis.x"),
                PaintAxis::Y => t!("disp.axis.y"),
                PaintAxis::Z => t!("disp.axis.z"),
                PaintAxis::Custom => t!("disp.axis.custom"),
                PaintAxis::SubdivNormal => t!("disp.axis.subdiv"),
                PaintAxis::View => t!("disp.axis.view"),
                PaintAxis::ViewFlat => t!("disp.axis.view_flat"),
                PaintAxis::BrushCenter => t!("disp.axis.center"),
            };
            use PaintAxis::{BrushCenter, Custom, FaceNormal, SubdivNormal, View, ViewFlat, X, Y, Z};
            let axes: &[PaintAxis] = if surface {
                &[FaceNormal, SubdivNormal, BrushCenter, View, ViewFlat, X, Y, Z, Custom]
            } else {
                &[FaceNormal, BrushCenter, View, ViewFlat, X, Y, Z, Custom]
            };
            if !surface && self.brush.axis == PaintAxis::SubdivNormal {
                self.brush.axis = PaintAxis::FaceNormal;
            }
            egui::ComboBox::from_id_salt("hp_disp_axis")
                .selected_text(label(self.brush.axis))
                .show_ui(ui, |ui| {
                    for a in axes {
                        let r = ui.selectable_value(&mut self.brush.axis, *a, label(*a));
                        match a {
                            PaintAxis::View => {
                                r.on_hover_text(t!("disp.axis.view_tip"));
                            }
                            PaintAxis::ViewFlat => {
                                r.on_hover_text(t!("disp.axis.view_flat_tip"));
                            }
                            PaintAxis::BrushCenter => {
                                r.on_hover_text(t!("disp.axis.center_tip"));
                            }
                            PaintAxis::Custom => {
                                r.on_hover_text(t!("disp.axis.custom_tip"));
                            }
                            _ => {}
                        }
                    }
                })
                .response
                .on_hover_text(t!("disp.axis_tip"));
        });
    }

    fn sculpt_ui(&mut self, ui: &mut Ui) {
        use sculpt::{PROFILE_FLAT, PROFILE_U, PROFILE_V, PushMode, SculptOp};
        section(ui, t!("disp.sculpt"));
        ui.horizontal_wrapped(|ui| {
            let ops = [
                (SculptOp::Push, "disp.sculpt.push", "disp.sculpt.push_tip"),
                (SculptOp::Carve, "disp.sculpt.carve", "disp.sculpt.carve_tip"),
                (SculptOp::Project, "disp.sculpt.project", "disp.sculpt.project_tip"),
                (SculptOp::Pinch, "disp.sculpt.pinch", "disp.sculpt.pinch_tip"),
                (SculptOp::Grab, "disp.sculpt.grab", "disp.sculpt.grab_tip"),
                (SculptOp::Inflate, "disp.sculpt.inflate", "disp.sculpt.inflate_tip"),
                (SculptOp::Flatten, "disp.sculpt.flatten", "disp.sculpt.flatten_tip"),
                (SculptOp::Relax, "disp.sculpt.relax", "disp.sculpt.relax_tip"),
            ];
            for (op, label, tip) in ops {
                if ui.add(Button::selectable(self.brush.sculpt.op == op, t!(label))).on_hover_text(t!(tip)).clicked() {
                    self.brush.sculpt.op = op;
                }
            }
        });
        let s = &mut self.brush.sculpt;
        egui::Grid::new("hp_disp_sculpt").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| match s.op {
            SculptOp::Push => {
                bounds_row(ui, s);
                grid_row(ui, t!("disp.sculpt.falloff_position"), |ui| {
                    ui.add(egui::DragValue::new(&mut s.falloff_position).speed(0.5).range(0.0..=100.0).suffix(" %"))
                        .on_hover_text(t!("disp.sculpt.falloff_position_tip"));
                });
                grid_row(ui, t!("disp.sculpt.falloff_final"), |ui| {
                    ui.add(egui::DragValue::new(&mut s.falloff_final).speed(0.5).range(0.0..=100.0).suffix(" %"))
                        .on_hover_text(t!("disp.sculpt.falloff_final_tip"));
                });
                grid_row(ui, t!("disp.sculpt.offset_mode"), |ui| {
                    ui.selectable_value(&mut s.push_mode, PushMode::Absolute, t!("disp.sculpt.absolute"))
                        .on_hover_text(t!("disp.sculpt.absolute_tip"));
                    ui.selectable_value(&mut s.push_mode, PushMode::Adaptive, t!("disp.sculpt.adaptive"))
                        .on_hover_text(t!("disp.sculpt.adaptive_tip"));
                });
                match s.push_mode {
                    PushMode::Absolute => grid_row(ui, t!("disp.sculpt.distance"), |ui| {
                        ui.add(egui::DragValue::new(&mut s.distance).speed(0.5).range(0.0..=4096.0))
                            .on_hover_text(t!("disp.sculpt.distance_tip"));
                    }),
                    PushMode::Adaptive => grid_row(ui, t!("disp.sculpt.percent"), |ui| {
                        ui.add(egui::DragValue::new(&mut s.percent).speed(0.5).range(0.0..=400.0).suffix(" %"))
                            .on_hover_text(t!("disp.sculpt.percent_tip"));
                    }),
                }
            }
            SculptOp::Carve => {
                bounds_row(ui, s);
                grid_row(ui, t!("disp.sculpt.depth"), |ui| {
                    ui.add(egui::DragValue::new(&mut s.carve_depth).speed(0.5).range(0.0..=4096.0))
                        .on_hover_text(t!("disp.sculpt.depth_tip"));
                });
                grid_row(ui, t!("disp.sculpt.profile"), |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            for (i, v) in s.profile.iter_mut().enumerate() {
                                ui.add(egui::Slider::new(v, 0.0..=1.0).vertical().show_value(false))
                                    .on_hover_text(tf!("disp.sculpt.profile_point", i * 25));
                            }
                        });
                        ui.horizontal(|ui| {
                            for (preset, label) in [(PROFILE_V, "V"), (PROFILE_U, "U"), (PROFILE_FLAT, "|_|")] {
                                if ui.small_button(label).on_hover_text(t!("disp.sculpt.preset_tip")).clicked() {
                                    s.profile = preset;
                                }
                            }
                        });
                    })
                    .response
                    .on_hover_text(t!("disp.sculpt.profile_tip"));
                });
            }
            SculptOp::Pinch | SculptOp::Inflate => {
                grid_row(ui, t!("disp.strength"), |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.brush.strength, 1.0..=8192.0)
                            .logarithmic(true)
                            .max_decimals(0)
                            .suffix(t!("disp.per_second")),
                    )
                    .on_hover_text(t!("disp.strength_tip"));
                });
            }
            SculptOp::Grab => {}
            SculptOp::Relax => {
                grid_row(ui, t!("disp.strength"), |ui| {
                    ui.add(egui::Slider::new(&mut s.relax_strength, 0.0..=1.0).max_decimals(2))
                        .on_hover_text(t!("disp.sculpt.relax_strength_tip"));
                });
            }
            SculptOp::Flatten => {
                grid_row(ui, t!("disp.strength"), |ui| {
                    ui.add(egui::Slider::new(&mut s.flatten_strength, 0.0..=1.0).max_decimals(2))
                        .on_hover_text(t!("disp.sculpt.flatten_strength_tip"));
                });
                grid_row(ui, t!("disp.sculpt.flatten_plane"), |ui| {
                    ui.selectable_value(&mut s.flatten_fixed, false, t!("disp.sculpt.flatten_average"))
                        .on_hover_text(t!("disp.sculpt.flatten_average_tip"));
                    ui.selectable_value(&mut s.flatten_fixed, true, t!("disp.sculpt.flatten_fixed"))
                        .on_hover_text(t!("disp.sculpt.flatten_fixed_tip"));
                });
            }
            SculptOp::Project => {
                grid_row(ui, t!("disp.sculpt.range"), |ui| {
                    ui.add(egui::DragValue::new(&mut s.project_range).speed(1.0).range(1.0..=16384.0))
                        .on_hover_text(t!("disp.sculpt.range_tip"));
                });
            }
        });
        egui::Grid::new("hp_disp_sculpt_axis").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            if self.brush.sculpt.op != SculptOp::Relax {
                self.axis_row(ui, false);
            }
            grid_row(ui, t!("disp.sculpt.smoothing"), |ui| {
                ui.add(egui::Slider::new(&mut self.brush.smooth, 0.0..=1.0).max_decimals(2))
                    .on_hover_text(t!("disp.sculpt.smoothing_tip"));
            });
        });
        if self.brush.axis == PaintAxis::Custom {
            vec3_edit(ui, &mut self.brush.custom_axis, 0.01);
        }
        if self.brush.sculpt.op == SculptOp::Project {
            ui.label(RichText::new(t!("disp.sculpt.project_hint")).weak());
        }
    }

    fn alpha_ui(&mut self, ui: &mut Ui, four_way: bool) {
        section(ui, t!("disp.paint_alpha"));
        if !four_way {
            self.brush.blend_channel = None;
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.label(t!("disp.blend")).on_hover_text(t!("disp.blend_tip"));
                if ui
                    .add(Button::selectable(self.brush.blend_channel.is_none(), t!("disp.blend.alpha")))
                    .on_hover_text(t!("disp.blend.alpha_tip"))
                    .clicked()
                {
                    self.brush.blend_channel = None;
                }
                for ch in 0..hp_vmf::multiblend::CHANNELS {
                    let label = (ch + 1).to_string();
                    if ui
                        .add(Button::selectable(self.brush.blend_channel == Some(ch), label))
                        .on_hover_text(tf!("disp.blend.channel_tip", ch + 1))
                        .clicked()
                    {
                        self.brush.blend_channel = Some(ch);
                    }
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            let effects = [
                (AlphaEffect::Add, "disp.alpha.add", "disp.alpha.add_tip"),
                (AlphaEffect::Remove, "disp.alpha.remove", "disp.alpha.remove_tip"),
                (AlphaEffect::Set, "disp.alpha.set", "disp.alpha.set_tip"),
                (AlphaEffect::Smooth, "disp.alpha.smooth", "disp.alpha.smooth_tip"),
            ];
            for (e, label, tip) in effects {
                if ui.add(Button::selectable(self.brush.alpha_effect == e, t!(label))).on_hover_text(t!(tip)).clicked()
                {
                    self.brush.alpha_effect = e;
                }
            }
        });
        egui::Grid::new("hp_disp_alpha").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            if self.brush.alpha_effect == AlphaEffect::Set {
                grid_row(ui, t!("disp.alpha_value"), |ui| {
                    ui.add(egui::Slider::new(&mut self.brush.alpha_value, 0.0..=255.0).max_decimals(0))
                        .on_hover_text(t!("disp.alpha_value_tip"));
                });
            } else if self.brush.alpha_effect == AlphaEffect::Smooth {
                grid_row(ui, t!("disp.strength"), |ui| {
                    ui.add(egui::Slider::new(&mut self.brush.smooth, 0.0..=1.0).max_decimals(2))
                        .on_hover_text(t!("disp.alpha.smooth_strength_tip"));
                });
            } else {
                grid_row(ui, t!("disp.strength"), |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.brush.alpha_strength, 1.0..=16384.0)
                            .logarithmic(true)
                            .max_decimals(0)
                            .suffix(t!("disp.per_second")),
                    )
                    .on_hover_text(t!("disp.alpha_strength_tip"));
                });
            }
        });
    }

    fn brush_ui(&mut self, ui: &mut Ui) {
        section(ui, t!("disp.brush"));
        egui::Grid::new("hp_disp_brush").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            grid_row(ui, t!("disp.radius"), |ui| {
                ui.add(egui::Slider::new(&mut self.brush.radius, 1.0..=2048.0).logarithmic(true).max_decimals(0))
                    .on_hover_text(t!("disp.radius_tip"));
            });
            grid_row(ui, t!("disp.falloff"), |ui| {
                let label = |f: Falloff| match f {
                    Falloff::Constant => t!("disp.falloff.constant"),
                    Falloff::Linear => t!("disp.falloff.linear"),
                    Falloff::Smooth => t!("disp.falloff.smooth"),
                };
                egui::ComboBox::from_id_salt("hp_disp_falloff").selected_text(label(self.brush.falloff)).show_ui(
                    ui,
                    |ui| {
                        for f in [Falloff::Constant, Falloff::Linear, Falloff::Smooth] {
                            ui.selectable_value(&mut self.brush.falloff, f, label(f));
                        }
                    },
                );
            });
        });
        ui.checkbox(&mut self.brush.spatial, t!("disp.spatial")).on_hover_text(t!("disp.spatial_tip"));
        ui.checkbox(&mut self.brush.line_of_sight, t!("disp.line_of_sight"))
            .on_hover_text(t!("disp.line_of_sight_tip"));
        ui.checkbox(&mut self.brush.autosew, t!("disp.autosew")).on_hover_text(t!("disp.autosew_tip"));
        ui.checkbox(&mut self.brush.selected_only, t!("disp.selected_only"))
            .on_hover_text(t!("disp.selected_only_tip"));
        let relax = self.relaxing();
        let mut continuous = self.brush.continuous || relax;
        if ui
            .add_enabled(!relax, egui::Checkbox::new(&mut continuous, t!("disp.continuous")))
            .on_hover_text(t!("disp.continuous_tip"))
            .changed()
        {
            self.brush.continuous = continuous;
        }
    }

    fn tag_ui(&mut self, ui: &mut Ui, state: &mut AppState, actions: &mut Vec<PanelAction>) {
        section(ui, t!("disp.tag"));
        ui.horizontal_wrapped(|ui| {
            let kinds = [
                (TagKind::Walkable, "disp.tag.walkable", "disp.tag.walkable_tip"),
                (TagKind::Buildable, "disp.tag.buildable", "disp.tag.buildable_tip"),
                (TagKind::Remove, "disp.tag.remove", "disp.tag.remove_tip"),
            ];
            for (k, label, tip) in kinds {
                if ui.add(Button::selectable(self.brush.tag.kind == k, t!(label))).on_hover_text(t!(tip)).clicked() {
                    self.brush.tag.kind = k;
                }
            }
        });
        if self.brush.tag.kind == TagKind::Remove && self.brush.tag.value == TagValue::Auto {
            self.brush.tag.value = TagValue::Off;
        }
        ui.horizontal_wrapped(|ui| {
            let values: &[(TagValue, &str, &str)] = if self.brush.tag.kind == TagKind::Remove {
                &[
                    (TagValue::On, "disp.tag.remove_on", "disp.tag.remove_on_tip"),
                    (TagValue::Off, "disp.tag.remove_off", "disp.tag.remove_off_tip"),
                ]
            } else {
                &[
                    (TagValue::On, "disp.tag.on", "disp.tag.on_tip"),
                    (TagValue::Off, "disp.tag.off", "disp.tag.off_tip"),
                    (TagValue::Auto, "disp.tag.auto", "disp.tag.auto_tip"),
                ]
            };
            for (v, label, tip) in values {
                if ui.add(Button::selectable(self.brush.tag.value == *v, t!(label))).on_hover_text(t!(tip)).clicked() {
                    self.brush.tag.value = *v;
                }
            }
        });
        let mut show = state.settings.render.disp_walkable;
        if ui.checkbox(&mut show, t!("disp.tag.show")).on_hover_text(t!("disp.tag.show_tip")).changed() {
            state.settings.render.disp_walkable = show;
            state.settings_dirty = true;
        }
        let any = !destroy_targets(&state.doc).is_empty();
        if ui
            .add_enabled(any, Button::new(t!("disp.tag.apply_selection")))
            .on_hover_text(t!("disp.tag.apply_selection_tip"))
            .clicked()
        {
            actions.push(PanelAction::TagAll(self.brush.tag.kind, self.brush.tag.value));
        }
    }

    fn run(&mut self, state: &mut AppState, action: PanelAction) {
        match action {
            PanelAction::Create(p) => {
                self.create(state, p);
            }
            PanelAction::Destroy => {
                self.destroy(state);
            }
            PanelAction::SelectAll => {
                self.select_all(state);
            }
            PanelAction::ClearFaces => {
                clear_targets(state);
            }
            PanelAction::SetPower(p) => {
                self.set_power_with(state, p, self.keep_shape);
            }
            PanelAction::Flatten => {
                self.flatten(state);
            }
            PanelAction::Elevation(v) => {
                self.set_elevation(state, v);
            }
            PanelAction::Sew => {
                self.sew(state);
            }
            PanelAction::InvertAlpha => {
                commands::invert_alpha(state);
            }
            PanelAction::Subdivide => {
                commands::subdivide_with_shape(state, self.subdivision_shape, self.subdivision_amount);
            }
            PanelAction::Noise => {
                erode::apply(state, erode::TerrainOp::Noise, &self.terrain);
            }
            PanelAction::Terrain(op) => {
                erode::apply(state, op, &self.terrain);
            }
            PanelAction::Flag(flag, on) => {
                commands::set_flag(state, flag, on);
            }
            PanelAction::Shadows(on) => {
                commands::set_cast_shadows(state, on);
            }
            PanelAction::Scale(f) => {
                commands::scale(state, f);
            }
            PanelAction::SelectAdjacent => {
                self.select_adjacent(state);
            }
            PanelAction::SelectOfSolids => {
                self.select_of_solids(state);
            }
            PanelAction::Lightmap(v) => {
                self.set_lightmap_scale(state, v);
            }
            PanelAction::TagAll(kind, value) => {
                self.tag_selection(state, kind, value);
            }
            PanelAction::LoopSmooth => {
                self.smooth_loop(state);
            }
            PanelAction::LoopHeight => {
                self.set_loop_height(state);
            }
            PanelAction::LoopClear => self.loops.sel = None,
            other => self.run_slope(state, other),
        }
    }

    fn brush_color(&self, mods: Modifiers) -> Color32 {
        let invert = self.inverts(mods);
        match self.interaction {
            Interaction::Select => Color32::WHITE,
            Interaction::Alpha => match (self.brush.alpha_effect, invert) {
                (AlphaEffect::Add, false) | (AlphaEffect::Remove, true) => Color32::from_rgb(200, 120, 255),
                (AlphaEffect::Set, _) => VIEW_WARNING,
                (AlphaEffect::Smooth, _) => VIEW_SUCCESS,
                _ => Color32::from_rgb(140, 140, 255),
            },
            Interaction::Tag => match (self.brush.tag.kind, self.brush.tag.effective(invert)) {
                (_, TagValue::Auto) => Color32::WHITE,
                (TagKind::Remove, TagValue::On) => Color32::from_rgb(170, 70, 230),
                (_, TagValue::On) => VIEW_SUCCESS,
                (_, TagValue::Off) => AXIS_Z,
            },
            Interaction::Geometry => match (self.brush.geo_effect, invert) {
                (GeoEffect::Raise, false) | (GeoEffect::Lower, true) => VIEW_ACCENT,
                (GeoEffect::Lower, false) | (GeoEffect::Raise, true) => AXIS_Z,
                (GeoEffect::Smooth, _) => VIEW_SUCCESS,
                (GeoEffect::Set, _) => VIEW_WARNING,
                (GeoEffect::Relax, _) => VIEW_SUCCESS,
            },
            Interaction::Sculpt if mods.shift => VIEW_SUCCESS,
            Interaction::Sculpt => match (self.brush.sculpt.op, invert) {
                (sculpt::SculptOp::Project, _) => VIEW_WARNING,
                (sculpt::SculptOp::Flatten, _) => VIEW_WARNING,
                (sculpt::SculptOp::Pinch | sculpt::SculptOp::Grab | sculpt::SculptOp::Inflate, false) => VIEW_ACCENT,
                (sculpt::SculptOp::Push, false) | (sculpt::SculptOp::Carve, true) => VIEW_ACCENT,
                _ => AXIS_Z,
            },
        }
    }

    fn effect_label(&self, mods: Modifiers) -> &'static str {
        let invert = self.inverts(mods);
        match self.interaction {
            Interaction::Select => t!("disp.mode.select"),
            _ if self.relaxing() && mods.shift => t!("disp.effect.smooth"),
            Interaction::Geometry => match (self.brush.geo_effect, invert) {
                (GeoEffect::Raise, false) | (GeoEffect::Lower, true) => t!("disp.effect.raise"),
                (GeoEffect::Lower, false) | (GeoEffect::Raise, true) => t!("disp.effect.lower"),
                (GeoEffect::Smooth, _) => t!("disp.effect.smooth"),
                (GeoEffect::Set, _) => t!("disp.effect.set"),
                (GeoEffect::Relax, _) => t!("disp.sculpt.relax"),
            },
            Interaction::Alpha => match (self.brush.alpha_effect, invert) {
                (AlphaEffect::Add, false) | (AlphaEffect::Remove, true) => t!("disp.alpha.add"),
                (AlphaEffect::Remove, false) | (AlphaEffect::Add, true) => t!("disp.alpha.remove"),
                (AlphaEffect::Set, _) => t!("disp.alpha.set"),
                (AlphaEffect::Smooth, _) => t!("disp.alpha.smooth"),
            },
            Interaction::Tag => match (self.brush.tag.kind, self.brush.tag.effective(invert)) {
                (TagKind::Remove, TagValue::On) => t!("disp.tag.remove_on"),
                (TagKind::Remove, _) => t!("disp.tag.remove_off"),
                (_, TagValue::On) => t!("disp.tag.on"),
                (_, TagValue::Off) => t!("disp.tag.off"),
                (_, TagValue::Auto) => t!("disp.tag.auto"),
            },
            Interaction::Sculpt if mods.shift => t!("disp.effect.smooth"),
            Interaction::Sculpt => match self.brush.sculpt.op {
                sculpt::SculptOp::Push => t!("disp.sculpt.push"),
                sculpt::SculptOp::Carve => t!("disp.sculpt.carve"),
                sculpt::SculptOp::Project => t!("disp.sculpt.project"),
                sculpt::SculptOp::Pinch => t!("disp.sculpt.pinch"),
                sculpt::SculptOp::Grab => t!("disp.sculpt.grab"),
                sculpt::SculptOp::Inflate => t!("disp.sculpt.inflate"),
                sculpt::SculptOp::Flatten => t!("disp.sculpt.flatten"),
                sculpt::SculptOp::Relax => t!("disp.sculpt.relax"),
            },
        }
    }

    fn cursor_fresh(&self, state: &AppState) -> Option<&CursorState> {
        self.cursor.as_ref().filter(|c| state.frame.index.saturating_sub(c.frame) <= 2)
    }
}

impl Tool for DisplacementTool {
    fn takes_right_drag(&self, kind: hp_app::ViewKind, modifiers: Modifiers) -> bool {
        !modifiers.alt && !modifiers.shift && self.right_drag_resizes(kind)
    }

    fn id(&self) -> ToolId {
        ToolId::Displacement
    }

    fn activate(&mut self, state: &mut AppState) {
        self.sync_settings(state);
        self.press = None;
        self.radius_drag = None;
        self.cursor = None;
        self.hover_face = None;
    }

    fn update(&mut self, state: &mut AppState, _ctx: &egui::Context) {
        if self.stroke.is_none() && object_targets_win(&state.doc) {
            let on = state.doc.records_selection();
            state.doc.set_record_selection(false);
            adopt_object_targets(state);
            state.doc.set_record_selection(on);
        }
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.end_stroke(state);
        self.press = None;
        self.radius_drag = None;
        self.cursor = None;
        self.hover_face = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if !self.loaded {
            self.sync_settings(state);
        }
        let view = input.view;
        if input.navigating && view.is_2d() {
            self.press = None;
            if self.stroke.as_ref().is_some_and(|s| s.view == view.index) {
                self.end_stroke(state);
            }
            return ToolFlow::Ignored;
        }
        let (pressed, down, right_pressed, right_down) = input.response.ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.secondary_pressed(),
                i.pointer.secondary_down(),
            )
        });
        let mut frame =
            PointerFrame { pos: input.pointer, pressed, down, mods: input.modifiers, right_pressed, right_down };
        if input.navigating {
            frame = navigating_frame(frame);
        }
        let radius = self.brush.radius;
        let flow = self.handle_pointer(state, view, frame);
        if self.needs_frames() {
            input.response.ctx.request_repaint();
        }
        if self.radius_drag.is_none() && radius != self.brush.radius {
            self.save_settings(state);
        }
        flow
    }

    fn action(&mut self, state: &mut AppState, action: hp_app::Action) -> bool {
        self.loop_action(state, action) || self.run_menu_action(state, action)
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        self.take_loop_request(state);
        if self.stroke.is_some() && !ctx.input(|i| i.pointer.primary_down()) {
            self.end_stroke(state);
        }
        let pending = self.stroke.is_some()
            || self.press.is_some()
            || !state.doc.selection().faces.is_empty()
            || object_targets_win(&state.doc);
        if pending && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            return self.cancel(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, _view: &ViewInfo, out: &mut hp_render::Overlay) {
        self.loop_overlay(state, out);
        let doc = &state.doc;
        let accent = rgba(VIEW_ACCENT);
        let mut dim = accent;
        dim[3] = 0.55;
        let shown: Vec<FaceRef> = if object_targets_win(doc) {
            doc.selected_disp_targets()
        } else {
            doc.selection().faces.iter().copied().collect()
        };
        for f in &shown {
            let Some(node) = doc.scene().solid(f.solid) else { continue };
            if let Some(mesh) = node.disp_mesh(f.side) {
                mesh_border(mesh, |a, b| line_on_top(out, a, b, accent));
            } else if let Some(poly) = node.geom.face_for_side(f.side).map(|g| g.vertices.as_slice()) {
                for (i, a) in poly.iter().enumerate() {
                    if let Some(b) = poly.get((i + 1) % poly.len()) {
                        line_on_top(out, *a, *b, dim);
                    }
                }
            }
        }
        if let Some((_, f)) = self.hover_face
            && !shown.contains(&f)
            && let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side))
        {
            let c = [1.0, 1.0, 1.0, 0.45];
            mesh_border(mesh, |a, b| line_on_top(out, a, b, c));
        }
        self.slope_overlay(state, out);
        if self.interaction == Interaction::Select || self.pick_path {
            return;
        }
        if let Some(c) = self.cursor_fresh(state) {
            if self.interaction == Interaction::Tag {
                let color = rgba(self.brush_color(c.mods));
                for f in &c.faces {
                    let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) else { continue };
                    for t in tags::triangles_under(mesh, c.center, self.brush.radius, self.brush.spatial) {
                        let Some(idx) = mesh.indices.get(t * 3..t * 3 + 3) else { continue };
                        let p = |k: u32| mesh.positions.get(k as usize).copied().unwrap_or(DVec3::ZERO);
                        let mid = (p(idx[0]) + p(idx[1]) + p(idx[2])) / 3.0;
                        point(out, mid, color, 3.0);
                    }
                }
            }
            if self.interaction == Interaction::Alpha && self.brush.blend_channel.is_some() {
                for f in &c.faces {
                    let Some(node) = doc.scene().solid(f.solid) else { continue };
                    let (Some(mesh), Some(disp)) =
                        (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
                    else {
                        continue;
                    };
                    let Some(colors) = paint::blend_colors(disp) else { continue };
                    for (p, color) in mesh.positions.iter().zip(colors) {
                        if p.distance(c.center) <= self.brush.radius * 1.5 {
                            point(out, *p, color, 5.0);
                        }
                    }
                }
            }
            let color = rgba(if c.blocked { BLOCKED } else { self.brush_color(c.mods) });
            for w in c.ring.windows(2) {
                if let [a, b] = w {
                    line_on_top(out, *a, *b, color);
                }
            }
            let tip = c.center + c.normal * (self.brush.radius * 0.35).clamp(4.0, 256.0);
            line_on_top(out, c.center, tip, color);
            point(out, c.center, color, 5.0);
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if self.interaction == Interaction::Select || self.pick_path {
            return;
        }
        let Some(c) = self.cursor_fresh(state) else { return };
        if c.view != view.index {
            return;
        }
        if let Some(p) = view.world_to_screen(c.center) {
            let (text, color) = if c.blocked {
                (t!("disp.not_selected_label").to_owned(), BLOCKED)
            } else {
                (format!("{}  r {:.0}", self.effect_label(c.mods), self.brush.radius), self.brush_color(c.mods))
            };
            painter.text(p + egui::vec2(14.0, 14.0), Align2::LEFT_TOP, text, FontId::proportional(12.0), color);
        }
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        if !self.loaded {
            self.sync_settings(state);
        }
        if self.stroke.is_some() && !ui.input(|i| i.pointer.primary_down()) {
            self.end_stroke(state);
        }
        let actions = egui::ScrollArea::vertical()
            .id_salt("hp_disp_panel")
            .auto_shrink([false, true])
            .show(ui, |ui| self.panel(ui, state))
            .inner;
        for a in actions {
            self.run(state, a);
        }
        self.brush.sanitize();
        self.save_settings(state);
    }

    fn retained_overlay_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        Some(highlight::version(state, view, self.highlight_opts()))
    }

    fn retained_overlay(&self, state: &AppState, view: &ViewInfo, out: &mut hp_render::Overlay) {
        highlight::build(state, view, self.highlight_opts(), out);
    }

    fn wants_side_panel(&self) -> bool {
        true
    }

    fn cursor(&self, state: &AppState, view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        if self.pick_path && !view.is_2d() {
            return Some(CursorIcon::Crosshair);
        }
        if self.interaction == Interaction::Select || view.is_2d() {
            return None;
        }
        if self.radius_drag.is_some_and(|r| r.view == view.index) {
            return Some(CursorIcon::ResizeHorizontal);
        }
        self.cursor_fresh(state).filter(|c| c.view == view.index).map(|_| CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        if self.stroke.is_some() {
            return Some(tf!("disp.hint_painting", key_label(Key::Escape)));
        }
        if self.radius_drag.is_some() {
            return Some(tf!("disp.hint_radius", format!("{:.0}", self.brush.radius)));
        }
        if self.disp_count(state) == 0 {
            return Some(t!("disp.hint_empty").to_owned());
        }
        if let Some(text) = self.loop_status(state) {
            return Some(text);
        }
        if self.cursor_fresh(state).is_some_and(|c| c.blocked) {
            return Some(tf!("disp.hint_not_selected", key_label(Key::Escape)));
        }
        Some(match self.interaction {
            Interaction::Select => t!("disp.hint_select").to_owned(),
            _ if self.relaxing() => t!("disp.hint_relax").to_owned(),
            Interaction::Sculpt => t!("disp.hint_sculpt").to_owned(),
            Interaction::Tag => t!("disp.hint_tag").to_owned(),
            _ => t!("disp.hint_paint").to_owned(),
        })
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        if self.cancel_stroke(state) {
            return true;
        }
        if self.press.take().is_some() || self.radius_drag.take().is_some() {
            return true;
        }
        if self.loops.sel.take().is_some() {
            return true;
        }
        clear_targets(state)
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        if !self.loaded {
            self.sync_settings(state);
        }
        if let Some(done) = self.loops.set_option(key, value) {
            return done;
        }
        let done = self.script_set_option(state, key, value);
        self.save_settings(state);
        done
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        let mut list = self.script_option_values();
        list.extend(self.loops.option_values());
        list
    }

    fn scripted_stroke(
        &mut self,
        state: &mut AppState,
        stroke: &hp_app::tool::ScriptedStroke,
    ) -> Option<Result<hp_app::tool::StrokeOutcome, String>> {
        if !self.loaded {
            self.sync_settings(state);
        }
        Some(self.script_stroke(state, stroke))
    }
}

pub(crate) fn navigating_frame(f: PointerFrame) -> PointerFrame {
    PointerFrame { right_pressed: false, right_down: false, mods: Modifiers::NONE, ..f }
}

fn bounds_row(ui: &mut Ui, s: &mut sculpt::SculptSettings) {
    grid_row(ui, t!("disp.sculpt.bounds"), |ui| {
        ui.selectable_value(&mut s.bounds, BoundsLimit::Attenuated, t!("disp.sculpt.attenuated"))
            .on_hover_text(t!("disp.sculpt.attenuated_tip"));
        ui.selectable_value(&mut s.bounds, BoundsLimit::Additive, t!("disp.sculpt.additive"))
            .on_hover_text(t!("disp.sculpt.additive_tip"));
    });
}

pub(crate) fn center_normal(doc: &hp_editor::Document, face: FaceRef, point: DVec3) -> Option<DVec3> {
    let mesh = doc.scene().solid(face.solid)?.disp_mesh(face.side)?;
    let (k, _) = mesh
        .positions
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.distance_squared(point).total_cmp(&b.1.distance_squared(point)))?;
    mesh.normals.get(k).copied().filter(|n| n.is_finite()).and_then(DVec3::try_normalize)
}

pub(crate) fn with_groups(state: &AppState, faces: Vec<FaceRef>) -> Vec<FaceRef> {
    if state.select_mode() != hp_editor::SelectMode::Groups {
        return faces;
    }
    let mut out: Vec<FaceRef> = Vec::with_capacity(faces.len());
    let mut expanded: HashSet<hp_editor::Id> = HashSet::new();
    let push = |out: &mut Vec<FaceRef>, f: FaceRef| {
        if !out.contains(&f) {
            out.push(f);
        }
    };
    for f in faces {
        if expanded.contains(&f.solid) {
            continue;
        }
        let is_disp = state.doc.face_side(f).is_some_and(|s| s.disp.is_some());
        let group = if is_disp { state.doc.group_disp_faces(f.solid) } else { Vec::new() };
        if group.iter().any(|g| g.solid != f.solid) {
            expanded.extend(group.iter().map(|g| g.solid));
            for g in group {
                push(&mut out, g);
            }
        } else {
            push(&mut out, f);
        }
    }
    out
}

pub(crate) fn object_targets_win(doc: &hp_editor::Document) -> bool {
    doc.object_disp_targets_win()
}

pub(crate) fn adopt_object_targets(state: &mut AppState) {
    if object_targets_win(&state.doc) {
        let faces = state.doc.selected_object_disp_faces();
        state.doc.select_faces(&faces, SelectOp::Replace);
    }
}

pub(crate) fn clear_targets(state: &mut AppState) -> bool {
    let mut changed = false;
    let objs: Vec<ObjRef> =
        state.doc.selected_object_disp_faces().iter().map(|f| state.doc.scene().object_of_solid(f.solid)).collect();
    if !objs.is_empty() {
        state.doc.deselect(&objs);
        changed = true;
    }
    if !state.doc.selection().faces.is_empty() {
        state.doc.clear_face_selection();
        changed = true;
    }
    changed
}

fn solid_faces(state: &AppState, id: hp_editor::Id) -> Vec<FaceRef> {
    let Some(node) = state.doc.scene().solid(id) else { return Vec::new() };
    let disps: Vec<FaceRef> = node
        .solid
        .sides
        .iter()
        .enumerate()
        .filter(|(_, s)| s.disp.is_some())
        .map(|(i, _)| FaceRef::new(id, i))
        .collect();
    if disps.is_empty() { (0..node.solid.sides.len()).map(|i| FaceRef::new(id, i)).collect() } else { disps }
}

fn pick_faces(state: &AppState, view: &ViewInfo, pos: Pos2) -> Vec<FaceRef> {
    if let Some(axis) = view_axis(view.kind) {
        let bounds = |e: &hp_vmf::Entity| state.assets.entity_bounds(e);
        let p = view.screen_to_2d(pos);
        let tol = 4.0 / view.cam2d.zoom.max(1e-9);
        let picked = state.doc.pick_2d(axis, p, tol, &bounds).into_iter().find_map(|o| match o {
            ObjRef::Solid(id) => Some(id),
            ObjRef::Entity(_) => None,
        });
        match picked.and_then(|id| state.doc.scene().solid(id)) {
            Some(node) => {
                let id = node.solid.id;
                let disps: Vec<FaceRef> = node
                    .solid
                    .sides
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.disp.is_some())
                    .map(|(i, _)| FaceRef::new(id, i))
                    .collect();
                if disps.is_empty() {
                    node.geom
                        .faces
                        .iter()
                        .filter(|f| f.normal().z > UP_FACING_Z)
                        .map(|f| FaceRef::new(id, f.side_index))
                        .collect()
                } else {
                    disps
                }
            }
            None => Vec::new(),
        }
    } else {
        let (o, d) = view.ray(pos);
        let opts = hp_editor::PickOptions { include_entities: false, ..Default::default() };
        match state.view_pick(&Ray::new(o, d), &opts) {
            Some(hp_editor::Hit { obj: ObjRef::Solid(id), face: Some(side), .. }) => vec![FaceRef::new(id, side)],
            _ => Vec::new(),
        }
    }
}
