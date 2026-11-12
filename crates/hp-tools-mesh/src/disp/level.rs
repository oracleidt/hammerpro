use egui::{Align2, Color32, FontId, Key, Pos2, RichText, Ui};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::state::AppState;
use hp_app::theme::{VIEW_SUCCESS, VIEW_WARNING};
use hp_app::tool::ToolOption;
use hp_app::widgets::grid_row;
use hp_app::{Severity, ViewInfo, t, tf};
use hp_editor::{Document, FaceRef, ObjRef, PickOptions};
use hp_geom::Ray;

use super::brush_math::Plane;
use super::paint::{BrushSettings, GeoEffect, Interaction, PaintAxis, RING_SEGMENTS, level_height, pick_disp};
use super::sculpt::SculptOp;
use super::{AlphaEffect, DisplacementTool};
use crate::common::{line_on_top, point, rgba, tangent_basis};

pub const PROBE_BACK: f64 = 64.0;
const SLOPE_A: Color32 = Color32::from_rgb(90, 220, 120);
const SLOPE_B: Color32 = Color32::from_rgb(255, 160, 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleInto {
    Target,
    SlopeStart,
    SlopeEnd,
    Alpha,
}

impl SampleInto {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "target" | "target_height" | "height" => Some(Self::Target),
            "slope_start" | "start" => Some(Self::SlopeStart),
            "slope_end" | "end" => Some(Self::SlopeEnd),
            "alpha" | "alpha_value" => Some(Self::Alpha),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    pub point: DVec3,
    pub normal: DVec3,
    pub disp: Option<FaceRef>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleReport {
    pub hit: SurfaceHit,
    pub axis: DVec3,
    pub world_height: f64,
    pub face: Option<FaceRef>,
    pub face_height: Option<f64>,
    pub target: f64,
    pub world: bool,
    pub alpha: Option<f64>,
}

pub fn pick_surface(state: &AppState, ray: &Ray) -> Option<SurfaceHit> {
    let disp =
        pick_disp(&state.doc, ray).map(|h| SurfaceHit { point: h.point, normal: h.face_normal, disp: Some(h.face) });
    let opts = PickOptions { include_entities: false, ..PickOptions::default() };
    let other = state
        .view_pick(ray, &opts)
        .filter(|h| match h.obj {
            ObjRef::Solid(id) => !state.doc.scene().solid(id).is_some_and(|n| n.has_disp()),
            ObjRef::Entity(_) => false,
        })
        .map(|h| SurfaceHit { point: h.point, normal: h.normal, disp: None });
    let dist = |h: &SurfaceHit| h.point.distance(ray.origin);
    match (disp, other) {
        (Some(a), Some(b)) => Some(if dist(&a) <= dist(&b) + 1e-6 { a } else { b }),
        (a, b) => a.or(b),
    }
}

pub fn probe_ray(point: DVec3, direction: DVec3) -> Ray {
    let dir = direction.try_normalize().filter(|d| d.is_finite()).unwrap_or(DVec3::NEG_Z);
    Ray::new(point - dir * PROBE_BACK, dir)
}

fn face_normal(doc: &Document, face: FaceRef) -> Option<DVec3> {
    doc.scene().solid(face.solid)?.disp_mesh(face.side).map(|m| m.face_normal)
}

pub fn level_axis(brush: &BrushSettings, normal: Option<DVec3>) -> DVec3 {
    brush.axis_vector().or(normal).and_then(DVec3::try_normalize).filter(|a| a.is_finite()).unwrap_or(DVec3::Z)
}

pub fn face_height(doc: &Document, face: FaceRef, axis: DVec3, p: DVec3) -> Option<f64> {
    let node = doc.scene().solid(face.solid)?;
    let mesh = node.disp_mesh(face.side)?;
    let disp = node.solid.sides.get(face.side)?.disp.as_ref()?;
    let n = mesh.face_normal;
    let p0 = *mesh.base_positions.first()?;
    let denom = axis.dot(n);
    if denom.abs() < 1e-6 {
        return None;
    }
    let elevation = if disp.elevation.is_finite() { disp.elevation } else { 0.0 };
    let h = ((p - p0).dot(n) - elevation) / denom;
    h.is_finite().then_some(h)
}

fn disp_under(doc: &Document, p: DVec3, axis: DVec3) -> Option<FaceRef> {
    pick_disp(doc, &Ray::new(p + axis * 0.5, -axis))
        .or_else(|| pick_disp(doc, &Ray::new(p - axis * 0.5, axis)))
        .map(|h| h.face)
}

fn nearest_alpha(doc: &Document, face: FaceRef, p: DVec3) -> Option<f64> {
    let node = doc.scene().solid(face.solid)?;
    let mesh = node.disp_mesh(face.side)?;
    let disp = node.solid.sides.get(face.side)?.disp.as_ref()?;
    let k =
        mesh.positions.iter().enumerate().min_by(|a, b| a.1.distance_squared(p).total_cmp(&b.1.distance_squared(p)))?.0;
    Some(disp.alphas.get(k).copied().filter(|a| a.is_finite()).unwrap_or(0.0).clamp(0.0, 255.0))
}

pub fn sample_report(doc: &Document, hit: SurfaceHit, brush: &BrushSettings) -> SampleReport {
    let up = hit.disp.and_then(|f| face_normal(doc, f)).unwrap_or(if hit.normal.z.abs() > 0.5 {
        DVec3::Z
    } else {
        hit.normal
    });
    let axis = level_axis(brush, Some(up));
    let world_height = hit.point.dot(axis);
    let face = hit.disp.or_else(|| disp_under(doc, hit.point, axis));
    let face_height = face.and_then(|f| face_height(doc, f, axis, hit.point));
    let world = brush.height_world || face_height.is_none();
    let target = if world { world_height } else { face_height.unwrap_or(world_height) };
    let alpha = hit.disp.and_then(|f| nearest_alpha(doc, f, hit.point));
    SampleReport { hit, axis, world_height, face, face_height, target, world, alpha }
}

pub fn slope_grade(a: DVec3, b: DVec3, axis: DVec3) -> Option<(f64, f64, f64)> {
    let d = b - a;
    let rise = d.dot(axis);
    let run = (d - axis * rise).length();
    if run < 1e-6 {
        return None;
    }
    Some((rise.atan2(run).to_degrees(), rise / run * 100.0, run))
}

pub fn target_plane(doc: &Document, face: FaceRef, brush: &BrushSettings) -> Option<Plane> {
    let node = doc.scene().solid(face.solid)?;
    let mesh = node.disp_mesh(face.side)?;
    let axis = level_axis(brush, Some(mesh.face_normal));
    if brush.height_world {
        return Some(Plane { origin: axis * brush.target_height, normal: axis });
    }
    let disp = node.solid.sides.get(face.side)?.disp.as_ref()?;
    let n = mesh.face_normal;
    let elevation = if disp.elevation.is_finite() { disp.elevation } else { 0.0 };
    let p0 = *mesh.base_positions.first()? + n * elevation;
    Some(Plane { origin: p0 + axis * brush.target_height, normal: n })
}

pub fn parse_point(text: &str) -> Option<Option<DVec3>> {
    let v = text.trim().to_ascii_lowercase();
    if v.is_empty() || v == "none" || v == "off" {
        return Some(None);
    }
    let n: Vec<f64> = v
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .map(|w| w.parse::<f64>().ok().filter(|x| x.is_finite() && x.abs() <= 131_072.0))
        .collect::<Option<_>>()?;
    match n.as_slice() {
        [x, y, z] => Some(Some(DVec3::new(*x, *y, *z))),
        _ => None,
    }
}

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

pub fn point_text(p: Option<DVec3>) -> String {
    match p {
        Some(p) => format!("{} {} {}", num(p.x), num(p.y), num(p.z)),
        None => "none".to_owned(),
    }
}

pub fn parse_axis(text: &str) -> Option<PaintAxis> {
    match text.trim().to_ascii_lowercase().as_str() {
        "normal" => Some(PaintAxis::FaceNormal),
        "subdiv" => Some(PaintAxis::SubdivNormal),
        "view" => Some(PaintAxis::View),
        "view_flat" | "view_horizontal" => Some(PaintAxis::ViewFlat),
        "center" | "brush_center" => Some(PaintAxis::BrushCenter),
        "x" => Some(PaintAxis::X),
        "y" => Some(PaintAxis::Y),
        "z" => Some(PaintAxis::Z),
        "custom" => Some(PaintAxis::Custom),
        _ => None,
    }
}

pub fn brush_from_values(values: &[ToolOption]) -> BrushSettings {
    let mut b = BrushSettings::default();
    for o in values {
        let v = o.value.as_str();
        match o.key {
            "axis" => {
                if let Some(a) = parse_axis(v) {
                    b.axis = a;
                }
            }
            "custom_axis" => {
                if let Some(Some(p)) = parse_point(v) {
                    b.custom_axis = p;
                }
            }
            "height_reference" => b.height_world = v.eq_ignore_ascii_case("world"),
            "target_height" => b.target_height = v.parse().unwrap_or(b.target_height),
            "slope_brush_start" => b.slope_start = parse_point(v).flatten(),
            "slope_brush_end" => b.slope_end = parse_point(v).flatten(),
            "radius" => b.radius = v.parse().unwrap_or(b.radius),
            _ => {}
        }
    }
    b.sanitize();
    b
}

impl DisplacementTool {
    pub(crate) fn slope_brush(&self) -> bool {
        self.interaction == Interaction::Geometry && self.brush.geo_effect == GeoEffect::Slope
    }

    pub(crate) fn level_target(&self) -> bool {
        match self.interaction {
            Interaction::Geometry => self.brush.geo_effect == GeoEffect::Set,
            Interaction::Sculpt => self.brush.sculpt.op == SculptOp::Flatten && self.brush.sculpt.flatten_target,
            _ => false,
        }
    }

    pub(crate) fn sample_slot(&self) -> Option<SampleInto> {
        if self.level_target() {
            return Some(SampleInto::Target);
        }
        if self.slope_brush() {
            return Some(if self.brush.slope_start.is_none() || self.brush.slope_end.is_some() {
                SampleInto::SlopeStart
            } else {
                SampleInto::SlopeEnd
            });
        }
        (self.interaction == Interaction::Alpha
            && self.brush.alpha_effect == AlphaEffect::Set
            && self.brush.blend_channel.is_none())
        .then_some(SampleInto::Alpha)
    }

    fn resolved_for_sample(&self) -> BrushSettings {
        self.brush.resolved_view(self.toward_eye, self.toward_eye_flat, None)
    }

    pub(crate) fn right_click(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, keep_height: bool) -> bool {
        let Some(slot) = self.sample_slot() else { return false };
        self.look_from(&view.cam3d);
        let (o, d) = view.ray(pos);
        let Some(hit) = pick_surface(state, &Ray::new(o, d)) else {
            state.notify(Severity::Info, t!("disp.sample_nothing"));
            return true;
        };
        if keep_height && self.slope_brush() {
            self.move_slope_point(state, hit.point);
            return true;
        }
        if slot == SampleInto::SlopeStart && self.brush.slope_start.is_some() {
            self.brush.slope_end = None;
        }
        let into = slot;
        let report = sample_report(&state.doc, hit, &self.resolved_for_sample());
        let text = self.apply_sample(&report, into);
        state.notify(Severity::Info, text);
        self.cursor = None;
        true
    }

    pub fn apply_sample(&mut self, report: &SampleReport, into: SampleInto) -> String {
        self.level_source = Some(report.hit.point);
        match into {
            SampleInto::Target => {
                let switched = report.world && !self.brush.height_world;
                self.brush.target_height = report.target.clamp(-16384.0, 16384.0);
                self.brush.height_world = report.world;
                let h = num(self.brush.target_height);
                if switched {
                    tf!("disp.sample_target_world_switch", h)
                } else if report.world {
                    tf!("disp.sample_target_world", h)
                } else {
                    tf!("disp.sample_target_face", h)
                }
            }
            SampleInto::SlopeStart => {
                self.brush.slope_start = Some(report.hit.point);
                tf!("disp.slope_start_set", num(report.world_height))
            }
            SampleInto::SlopeEnd => {
                self.brush.slope_end = Some(report.hit.point);
                tf!("disp.slope_end_set", num(report.world_height))
            }
            SampleInto::Alpha => match report.alpha {
                Some(a) => {
                    self.brush.alpha_value = a.round();
                    tf!("disp.sample_alpha", num(a.round()))
                }
                None => t!("disp.sample_alpha_none").to_owned(),
            },
        }
    }

    fn move_slope_point(&mut self, state: &mut AppState, p: DVec3) {
        let axis = level_axis(&self.resolved_for_sample(), Some(DVec3::Z));
        let flat = |q: DVec3| {
            let d = q - p;
            (d - axis * d.dot(axis)).length()
        };
        let slot = match (self.brush.slope_start, self.brush.slope_end) {
            (Some(a), Some(b)) => {
                if flat(a) <= flat(b) {
                    &mut self.brush.slope_start
                } else {
                    &mut self.brush.slope_end
                }
            }
            (Some(_), None) => &mut self.brush.slope_end,
            (None, _) => &mut self.brush.slope_start,
        };
        let moved = match *slot {
            Some(old) => p + axis * (old.dot(axis) - p.dot(axis)),
            None => p,
        };
        *slot = Some(moved);
        self.cursor = None;
        state.notify(Severity::Info, tf!("disp.slope_point_moved", num(moved.dot(axis))));
    }

    pub(crate) fn clear_slope_points(&mut self) -> bool {
        if !self.slope_brush() || (self.brush.slope_start.is_none() && self.brush.slope_end.is_none()) {
            return false;
        }
        self.brush.slope_start = None;
        self.brush.slope_end = None;
        true
    }

    pub(crate) fn slope_points_pending(&self) -> bool {
        self.slope_brush() && (self.brush.slope_start.is_some() || self.brush.slope_end.is_some())
    }

    pub(crate) fn begin_slope_stroke(&mut self, state: &mut AppState, center: DVec3) -> bool {
        if !self.slope_brush() {
            return true;
        }
        match (self.brush.slope_start, self.brush.slope_end) {
            (Some(_), Some(_)) => true,
            (Some(_), None) => {
                self.brush.slope_end = Some(center);
                let h = center.dot(level_axis(&self.resolved_for_sample(), Some(DVec3::Z)));
                state.notify(Severity::Info, tf!("disp.slope_end_set", num(h)));
                true
            }
            (None, _) => false,
        }
    }

    fn level_point(&self, doc: &Document, face: Option<FaceRef>, axis: DVec3, p: DVec3) -> Option<DVec3> {
        if self.slope_brush() {
            let (a, b) = self.brush.slope_start.zip(self.brush.slope_end)?;
            let h = level_height(a, b, None, axis, p);
            return Some(p + axis * (h - p.dot(axis)));
        }
        if !self.level_target() {
            return None;
        }
        let h = self.brush.target_height;
        if self.brush.height_world {
            return Some(p + axis * (h - p.dot(axis)));
        }
        let now = face_height(doc, face?, axis, p)?;
        Some(p + axis * (h - now))
    }

    pub(crate) fn level_overlay(&self, state: &AppState, out: &mut hp_render::Overlay) {
        let slope = self.slope_brush();
        if !slope && !self.level_target() {
            return;
        }
        let doc = &state.doc;
        let warn = rgba(VIEW_WARNING);
        if let Some((src, target, axis)) = self.source_level(doc).filter(|_| !slope) {
            let green = rgba(VIEW_SUCCESS);
            point(out, src, green, 8.0);
            line_on_top(out, src, target, warn);
            point(out, target, warn, 6.0);
            let r = (self.brush.radius * 0.5).clamp(16.0, 256.0);
            let (u, v) = tangent_basis(axis);
            let ring: Vec<DVec3> = (0..=RING_SEGMENTS)
                .map(|i| {
                    let a = i as f64 / RING_SEGMENTS as f64 * std::f64::consts::TAU;
                    target + (u * a.cos() + v * a.sin()) * r
                })
                .collect();
            for w in ring.windows(2) {
                if let [p, q] = w {
                    line_on_top(out, *p, *q, green);
                }
            }
        } else if let Some(src) = self.level_source.filter(|_| !slope) {
            point(out, src, rgba(VIEW_SUCCESS), 7.0);
        }
        if slope {
            let (a, b) = (self.brush.slope_start, self.brush.slope_end);
            if let Some(a) = a {
                point(out, a, rgba(SLOPE_A), 10.0);
            }
            if let Some(b) = b {
                point(out, b, rgba(SLOPE_B), 10.0);
            }
            match (a, b, self.cursor_fresh(state)) {
                (Some(a), Some(b), _) => line_on_top(out, a, b, warn),
                (Some(a), None, Some(c)) => {
                    let mut dim = warn;
                    dim[3] = 0.45;
                    line_on_top(out, a, c.center, dim);
                }
                _ => {}
            }
        }
        let Some(c) = self.cursor_fresh(state) else { return };
        let face = self.hover_face.map(|(_, f)| f);
        let axis = c.normal.try_normalize().unwrap_or(DVec3::Z);
        let ring: Vec<DVec3> = c.ring.iter().filter_map(|r| self.level_point(doc, face, axis, *r)).collect();
        if ring.len() == c.ring.len() {
            let mut col = warn;
            col[3] = 0.8;
            for w in ring.windows(2) {
                if let [p, q] = w {
                    line_on_top(out, *p, *q, col);
                }
            }
        }
        if let Some(t) = self.level_point(doc, face, axis, c.center) {
            line_on_top(out, c.center, t, warn);
            point(out, t, warn, 6.0);
        }
    }

    pub(crate) fn level_paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let slope = self.slope_brush();
        if !slope && !self.level_target() {
            return;
        }
        let font = FontId::proportional(12.0);
        let axis = level_axis(&self.resolved_for_sample(), Some(DVec3::Z));
        if slope {
            let label = |p: DVec3, name: &str, color: Color32| {
                if let Some(s) = view.world_to_screen(p) {
                    painter.text(
                        s + egui::vec2(10.0, -18.0),
                        Align2::LEFT_BOTTOM,
                        format!("{name} {}", num(p.dot(axis))),
                        font.clone(),
                        color,
                    );
                }
            };
            if let Some(a) = self.brush.slope_start {
                label(a, "A", SLOPE_A);
            }
            if let Some(b) = self.brush.slope_end {
                label(b, "B", SLOPE_B);
            }
            if let Some((a, b)) = self.brush.slope_start.zip(self.brush.slope_end)
                && let Some((deg, pct, _)) = slope_grade(a, b, axis)
                && let Some(s) = view.world_to_screen((a + b) * 0.5)
            {
                painter.text(
                    s + egui::vec2(0.0, -14.0),
                    Align2::CENTER_BOTTOM,
                    format!("{deg:.1}\u{b0}  {pct:.1} %"),
                    font.clone(),
                    VIEW_WARNING,
                );
            }
        }
        if let Some((_, target, _)) = self.source_level(&state.doc).filter(|_| !slope)
            && let Some(s) = view.world_to_screen(target)
        {
            let text = tf!("disp.level_label_sampled", self.level_text());
            painter.text(s + egui::vec2(12.0, -10.0), Align2::LEFT_BOTTOM, text, font.clone(), VIEW_SUCCESS);
        }
        let Some(c) = self.cursor_fresh(state).filter(|c| c.view == view.index) else { return };
        let face = self.hover_face.map(|(_, f)| f);
        let cax = c.normal.try_normalize().unwrap_or(DVec3::Z);
        let Some(t) = self.level_point(&state.doc, face, cax, c.center) else { return };
        let Some(s) = view.world_to_screen(t) else { return };
        let text = if slope { tf!("disp.level_label_slope", num(t.dot(cax))) } else { self.level_text() };
        painter.text(s + egui::vec2(14.0, -4.0), Align2::LEFT_BOTTOM, text, font, VIEW_WARNING);
    }

    fn level_text(&self) -> String {
        if self.brush.height_world {
            tf!("disp.level_label_world", num(self.brush.target_height))
        } else {
            tf!("disp.level_label_face", num(self.brush.target_height))
        }
    }

    pub(crate) fn source_level(&self, doc: &Document) -> Option<(DVec3, DVec3, DVec3)> {
        let src = self.level_source?;
        if !self.level_target() {
            return None;
        }
        let brush = self.resolved_for_sample();
        let up = level_axis(&brush, Some(DVec3::Z));
        let face = disp_under(doc, src, up);
        let axis = level_axis(&brush, face.and_then(|f| face_normal(doc, f)).or(Some(DVec3::Z)));
        let target = self.level_point(doc, face, axis, src)?;
        Some((src, target, axis))
    }

    pub fn sample_point(&self) -> Option<DVec3> {
        self.level_source
    }

    pub fn set_sample_point(&mut self, p: Option<DVec3>) {
        self.level_source = p;
    }

    pub(crate) fn level_hint(&self) -> Option<String> {
        if self.slope_brush() {
            return Some(match (self.brush.slope_start, self.brush.slope_end) {
                (None, _) => t!("disp.hint_slope_start").to_owned(),
                (Some(_), None) => t!("disp.hint_slope_end").to_owned(),
                (Some(_), Some(_)) => tf!("disp.hint_slope_paint", key_label(Key::Escape)),
            });
        }
        match self.sample_slot() {
            Some(SampleInto::Target) => Some(t!("disp.hint_level").to_owned()),
            Some(SampleInto::Alpha) => Some(t!("disp.hint_sample_alpha").to_owned()),
            _ => None,
        }
    }

    pub(crate) fn height_reference_ui(&mut self, ui: &mut Ui) {
        grid_row(ui, t!("disp.height_reference"), |ui| {
            ui.selectable_value(&mut self.brush.height_world, false, t!("disp.height_face"))
                .on_hover_text(t!("disp.height_face_tip"));
            ui.selectable_value(&mut self.brush.height_world, true, t!("disp.height_world"))
                .on_hover_text(t!("disp.height_world_tip"));
        });
    }

    pub(crate) fn slope_brush_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let point_row = |ui: &mut Ui, label: &str, p: &mut Option<DVec3>, color: Color32| {
            grid_row(ui, label, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(color, "\u{25cf}");
                    match p {
                        Some(v) => {
                            for c in [&mut v.x, &mut v.y, &mut v.z] {
                                ui.add(
                                    egui::DragValue::new(c).speed(1.0).range(-131_072.0..=131_072.0).max_decimals(1),
                                );
                            }
                            if ui.small_button("x").on_hover_text(t!("disp.slope_clear_point")).clicked() {
                                *p = None;
                            }
                        }
                        None => {
                            ui.weak(t!("disp.slope_not_set"));
                        }
                    }
                });
            });
        };
        point_row(ui, t!("disp.slope_start"), &mut self.brush.slope_start, SLOPE_A);
        point_row(ui, t!("disp.slope_end"), &mut self.brush.slope_end, SLOPE_B);
        let axis = level_axis(&self.resolved_for_sample(), Some(DVec3::Z));
        if let Some((a, b)) = self.brush.slope_start.zip(self.brush.slope_end) {
            let text = match slope_grade(a, b, axis) {
                Some((deg, pct, run)) => {
                    tf!("disp.slope_grade_value", format!("{deg:.1}"), format!("{pct:.1}"), num(run))
                }
                None => t!("disp.slope_grade_vertical").to_owned(),
            };
            grid_row(ui, t!("disp.slope_grade"), |ui| {
                ui.label(text);
            });
        }
        grid_row(ui, "", |ui| {
            ui.horizontal_wrapped(|ui| {
                let both = self.brush.slope_start.is_some() && self.brush.slope_end.is_some();
                if ui.add_enabled(both, egui::Button::new(t!("disp.slope_swap"))).clicked() {
                    std::mem::swap(&mut self.brush.slope_start, &mut self.brush.slope_end);
                }
                let any = self.brush.slope_start.is_some() || self.brush.slope_end.is_some();
                if ui.add_enabled(any, egui::Button::new(t!("disp.slope_clear"))).clicked() {
                    self.brush.slope_start = None;
                    self.brush.slope_end = None;
                }
                if ui
                    .add_enabled(both, egui::Button::new(t!("disp.slope_to_path")))
                    .on_hover_text(t!("disp.slope_to_path_tip"))
                    .clicked()
                    && let Some((a, b)) = self.brush.slope_start.zip(self.brush.slope_end)
                {
                    self.slope.path = vec![a, b];
                    state.notify(Severity::Info, t!("disp.slope_to_path_done"));
                }
                if ui
                    .add_enabled(self.slope.path.len() >= 2, egui::Button::new(t!("disp.slope_from_path")))
                    .on_hover_text(t!("disp.slope_from_path_tip"))
                    .clicked()
                {
                    self.brush.slope_start = self.slope.path.first().copied();
                    self.brush.slope_end = self.slope.path.last().copied();
                }
            });
        });
    }

    pub(crate) fn slope_brush_help(&self, ui: &mut Ui) {
        ui.label(RichText::new(t!("disp.slope_help")).weak());
    }
}

pub const TR: hp_app::i18n::TrTable = &[
    ("disp.sample_nothing", "Nothing under the cursor to sample", "Rien sous le curseur à échantillonner"),
    (
        "disp.sample_target_world_switch",
        "Target height {0} (world): no displacement under the sampled point, so the height reference is now World",
        "Hauteur cible {0} (monde) : aucun displacement sous le point échantillonné, la référence de hauteur passe donc à Monde",
    ),
    ("disp.sample_target_world", "Target height {0} (world)", "Hauteur cible {0} (monde)"),
    ("disp.sample_target_face", "Target height {0} above the face", "Hauteur cible {0} au-dessus de la face"),
    ("disp.slope_start_set", "Slope start set at height {0}", "Début de la pente placé à la hauteur {0}"),
    ("disp.slope_end_set", "Slope end set at height {0}", "Fin de la pente placée à la hauteur {0}"),
    ("disp.sample_alpha", "Alpha value {0} sampled", "Valeur alpha {0} échantillonnée"),
    (
        "disp.sample_alpha_none",
        "Alpha is sampled on displacements only",
        "L'alpha ne s'échantillonne que sur les displacements",
    ),
    ("disp.slope_point_moved", "Slope point moved, height kept at {0}", "Point de pente déplacé, hauteur gardée à {0}"),
    ("disp.level_label_slope", "slope {0}", "pente {0}"),
    ("disp.level_label_world", "level {0} (world)", "niveau {0} (monde)"),
    ("disp.level_label_face", "level {0} above face", "niveau {0} sur la face"),
    ("disp.level_label_sampled", "sampled: {0}", "échantillonné : {0}"),
    (
        "disp.hint_slope_start",
        "Slope: right click the surface to set the start point (its height is sampled there).",
        "Pente : clic droit sur la surface pour placer le point de départ (sa hauteur y est échantillonnée).",
    ),
    (
        "disp.hint_slope_end",
        "Slope: right click to set the end point, or hold the left button to paint: the end is then where you start painting.",
        "Pente : clic droit pour placer le point d'arrivée, ou maintenez le bouton gauche pour peindre : l'arrivée est alors là où vous commencez.",
    ),
    (
        "disp.hint_slope_paint",
        "Slope: hold the left button to bring the surface onto the slope. Right click starts a new slope, Ctrl + right click moves the nearest point and keeps its height, {0} clears the points.",
        "Pente : maintenez le bouton gauche pour amener la surface sur la pente. Clic droit : nouvelle pente, Ctrl + clic droit : déplace le point le plus proche en gardant sa hauteur, {0} efface les points.",
    ),
    (
        "disp.hint_level",
        "Hold the left button to bring the surface to the target level. Right click samples the height under the cursor, right drag changes the radius.",
        "Maintenez le bouton gauche pour amener la surface au niveau cible. Clic droit : échantillonne la hauteur sous le curseur, clic droit glissé : rayon.",
    ),
    (
        "disp.hint_sample_alpha",
        "Hold the left button to paint the alpha value. Right click samples the alpha under the cursor, right drag changes the radius.",
        "Maintenez le bouton gauche pour peindre la valeur alpha. Clic droit : échantillonne l'alpha sous le curseur, clic droit glissé : rayon.",
    ),
    ("disp.height_reference", "Height from", "Hauteur depuis"),
    ("disp.height_face", "Face", "Face"),
    (
        "disp.height_face_tip",
        "The target height is measured above the base face of each displacement (Hammer)",
        "La hauteur cible est mesurée au-dessus de la face de base de chaque displacement (Hammer)",
    ),
    ("disp.height_world", "World", "Monde"),
    (
        "disp.height_world_tip",
        "The target height is a world coordinate along the paint axis: the same level on every displacement, whatever its base face",
        "La hauteur cible est une coordonnée du monde le long de l'axe : le même niveau sur chaque displacement, quelle que soit sa face de base",
    ),
    ("disp.slope_start", "Start", "Départ"),
    ("disp.slope_end", "End", "Arrivée"),
    ("disp.slope_not_set", "not set (right click)", "non placé (clic droit)"),
    ("disp.slope_clear_point", "Clear this point", "Effacer ce point"),
    ("disp.slope_grade", "Grade", "Pente"),
    ("disp.slope_grade_value", "{0} degrees, {1} %, over {2} units", "{0} degrés, {1} %, sur {2} unités"),
    (
        "disp.slope_grade_vertical",
        "vertical (both points on one line of the axis)",
        "verticale (les deux points sur une même ligne de l'axe)",
    ),
    ("disp.slope_swap", "Swap", "Inverser"),
    ("disp.slope_clear", "Clear", "Effacer"),
    ("disp.slope_to_path", "Use as path", "Utiliser comme chemin"),
    (
        "disp.slope_to_path_tip",
        "Copy the two points to the path of Constant slope along a path (Slope control)",
        "Copie les deux points dans le chemin de Pente constante le long d'un chemin (Contrôle des pentes)",
    ),
    (
        "disp.slope_to_path_done",
        "The slope points are now the path of the slope control",
        "Les points de la pente sont maintenant le chemin du contrôle des pentes",
    ),
    ("disp.slope_from_path", "From path", "Depuis le chemin"),
    (
        "disp.slope_from_path_tip",
        "Take the first and last points of the slope control path",
        "Prend le premier et le dernier point du chemin du contrôle des pentes",
    ),
    (
        "disp.slope_help",
        "Right click the surface to set the start, right click again for the end (or start painting: the end is taken there), then paint along. Ctrl + right click moves the nearest point and keeps its height; type X, Y, Z above to set heights by hand.",
        "Clic droit sur la surface pour le départ, encore un clic droit pour l'arrivée (ou commencez à peindre : l'arrivée est prise là), puis peignez le long. Ctrl + clic droit déplace le point le plus proche en gardant sa hauteur ; tapez X, Y, Z ci-dessus pour régler les hauteurs à la main.",
    ),
    (
        "disp.sample_hint",
        "Right click (without dragging) samples the value under the cursor.",
        "Un clic droit (sans glisser) échantillonne la valeur sous le curseur.",
    ),
    ("disp.effect.slope", "Slope", "Pente"),
    (
        "disp.effect.slope_tip",
        "Bring the surface onto the slope between two points (road ramps, blending two heights). Right click sets the points",
        "Amène la surface sur la pente entre deux points (rampes de route, raccord entre deux hauteurs). Le clic droit place les points",
    ),
    ("disp.smooth_boost", "Multiplier", "Multiplicateur"),
    (
        "disp.smooth_boost_tip",
        "Makes smoothing much stronger: up to 16 averaging passes per brush sample. Hold Ctrl while smoothing for 4 times more",
        "Rend le lissage bien plus fort : jusqu'à 16 passes de moyenne par échantillon du pinceau. Maintenez Ctrl en lissant pour 4 fois plus",
    ),
    (
        "disp.slope_brush_strength_tip",
        "Share of the way to the slope per brush sample (1: exactly on the slope at once)",
        "Part du chemin vers la pente par échantillon du pinceau (1 : directement sur la pente)",
    ),
    ("disp.sculpt.flatten_target", "Target height", "Hauteur cible"),
    (
        "disp.sculpt.flatten_target_tip",
        "Flatten toward the level of the target height (right click samples it)",
        "Aplanit vers le niveau de la hauteur cible (le clic droit l'échantillonne)",
    ),
    (
        "disp.slope_need_start",
        "Slope: right click the surface first to set the start point",
        "Pente : faites d'abord un clic droit sur la surface pour placer le point de départ",
    ),
];
