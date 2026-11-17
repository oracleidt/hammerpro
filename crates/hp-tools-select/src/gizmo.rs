use egui::{Color32, Painter, Pos2, Rect, Stroke};
use glam::DVec3;
use hp_app::ViewInfo;
use hp_app::theme::{AXIS_X, AXIS_Y, AXIS_Z};

use crate::math::{dist_to_segment, point_in_convex, units_per_point_at};

pub const GIZMO_POINTS: f64 = 90.0;
const PICK_POINTS: f32 = 7.0;
pub const RING_SEGMENTS: usize = 64;
const PLANE_LO: f64 = 0.22;
const PLANE_HI: f64 = 0.46;
const AXIS_GAP: f64 = 0.12;
const RING_RADIUS: f64 = 0.85;
const CUBE_POINTS: f32 = 5.0;
const CENTER_POINTS: f32 = 7.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GizmoMode {
    #[default]
    Move,
    Rotate,
    Scale,
}

impl GizmoMode {
    pub const ALL: [GizmoMode; 3] = [GizmoMode::Move, GizmoMode::Rotate, GizmoMode::Scale];

    pub fn next(self) -> Self {
        match self {
            GizmoMode::Move => GizmoMode::Rotate,
            GizmoMode::Rotate => GizmoMode::Scale,
            GizmoMode::Scale => GizmoMode::Move,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            GizmoMode::Move => "seltool.gizmo.move",
            GizmoMode::Rotate => "seltool.gizmo.rotate",
            GizmoMode::Scale => "seltool.gizmo.scale",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GizmoPart {
    Axis(usize),
    Plane(usize),
    Ring(usize),
    Center,
}

pub fn axis_color(k: usize) -> Color32 {
    match k {
        0 => AXIS_X,
        1 => AXIS_Y,
        _ => AXIS_Z,
    }
}

pub const WORLD_AXES: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];

#[derive(Clone, Copy, Debug)]
pub struct GizmoLayout {
    pub center: DVec3,
    pub size: f64,
    pub axes: [DVec3; 3],
    pub facing: [f64; 3],
    eye: DVec3,
}

impl GizmoLayout {
    pub fn new(view: &ViewInfo, center: DVec3) -> Option<Self> {
        Self::with_axes(view, center, WORLD_AXES)
    }

    pub fn with_axes(view: &ViewInfo, center: DVec3, axes: [DVec3; 3]) -> Option<Self> {
        if view.is_2d() || !center.is_finite() || axes.iter().any(|a| !a.is_finite()) {
            return None;
        }
        let cam = &view.cam3d;
        if (center - cam.pos).dot(cam.forward()) <= cam.near {
            return None;
        }
        let size = units_per_point_at(view, center) * GIZMO_POINTS;
        if !size.is_finite() || size <= 0.0 {
            return None;
        }
        let to_eye = cam.pos - center;
        let mut facing = [1.0; 3];
        for (k, f) in facing.iter_mut().enumerate() {
            *f = if to_eye.dot(axes[k]) < 0.0 { -1.0 } else { 1.0 };
        }
        Some(Self { center, size, axes, facing, eye: cam.pos })
    }

    pub fn axis_segment(&self, k: usize) -> (DVec3, DVec3) {
        let e = self.axes[k % 3];
        (self.center + e * self.size * AXIS_GAP, self.center + e * self.size)
    }

    pub fn plane_quad(&self, n: usize) -> [DVec3; 4] {
        let a = (n + 1) % 3;
        let b = (n + 2) % 3;
        let ea = self.axes[a] * self.facing[a] * self.size;
        let eb = self.axes[b] * self.facing[b] * self.size;
        let c = self.center;
        [
            c + ea * PLANE_LO + eb * PLANE_LO,
            c + ea * PLANE_HI + eb * PLANE_LO,
            c + ea * PLANE_HI + eb * PLANE_HI,
            c + ea * PLANE_LO + eb * PLANE_HI,
        ]
    }

    pub fn ring_point(&self, k: usize, i: usize) -> DVec3 {
        let a = self.axes[(k + 1) % 3];
        let b = self.axes[(k + 2) % 3];
        let t = std::f64::consts::TAU * (i % RING_SEGMENTS) as f64 / RING_SEGMENTS as f64;
        self.center + (a * t.cos() + b * t.sin()) * self.size * RING_RADIUS
    }

    pub fn ring_point_front(&self, p: DVec3) -> bool {
        (p - self.center).dot(self.eye - self.center) >= -1e-9
    }

    pub fn hit(&self, view: &ViewInfo, pos: Pos2, mode: GizmoMode) -> Option<GizmoPart> {
        let s = |p: DVec3| view.world_to_screen(p);
        let mut best: Option<(f32, GizmoPart)> = None;
        let mut consider = |d: f32, part: GizmoPart| {
            if d <= PICK_POINTS && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, part));
            }
        };
        if mode == GizmoMode::Rotate {
            for k in 0..3 {
                for i in 0..RING_SEGMENTS {
                    let p0 = self.ring_point(k, i);
                    let p1 = self.ring_point(k, i + 1);
                    if !self.ring_point_front((p0 + p1) * 0.5) {
                        continue;
                    }
                    if let (Some(a), Some(b)) = (s(p0), s(p1)) {
                        consider(dist_to_segment(pos, a, b), GizmoPart::Ring(k));
                    }
                }
            }
            return best.map(|(_, p)| p);
        }
        if mode == GizmoMode::Scale
            && let Some(c) = s(self.center)
            && Rect::from_center_size(c, egui::Vec2::splat(CENTER_POINTS * 2.0 + 2.0)).contains(pos)
        {
            return Some(GizmoPart::Center);
        }
        for k in 0..3 {
            let (a, b) = self.axis_segment(k);
            if let (Some(a), Some(b)) = (s(a), s(b)) {
                let d = dist_to_segment(pos, a, b);
                let d = if mode == GizmoMode::Scale { d.min((b.distance(pos) - CUBE_POINTS).max(0.0)) } else { d };
                consider(d, GizmoPart::Axis(k));
            }
        }
        if best.is_some() {
            return best.map(|(_, p)| p);
        }
        for n in 0..3 {
            let q = self.plane_quad(n);
            let pts: Option<Vec<Pos2>> = q.iter().map(|p| s(*p)).collect();
            if let Some(pts) = pts
                && point_in_convex(&pts, pos)
            {
                return Some(GizmoPart::Plane(n));
            }
        }
        None
    }

    pub fn paint(&self, view: &ViewInfo, painter: &Painter, mode: GizmoMode, active: Option<GizmoPart>) {
        let s = |p: DVec3| view.world_to_screen(p);
        let highlight = |c: Color32, on: bool| if on { c.lerp_to_gamma(Color32::WHITE, 0.55) } else { c };
        if mode == GizmoMode::Rotate {
            for k in 0..3 {
                let on = active == Some(GizmoPart::Ring(k));
                let color = highlight(axis_color(k), on);
                let width = if on { 3.0 } else { 2.0 };
                for i in 0..RING_SEGMENTS {
                    let p0 = self.ring_point(k, i);
                    let p1 = self.ring_point(k, i + 1);
                    let front = self.ring_point_front((p0 + p1) * 0.5);
                    let (c, w) = if front { (color, width) } else { (color.gamma_multiply(0.3), 1.0) };
                    if let (Some(a), Some(b)) = (s(p0), s(p1)) {
                        painter.line_segment([a, b], Stroke::new(w, c));
                    }
                }
            }
        } else {
            for n in 0..3 {
                let on = active == Some(GizmoPart::Plane(n));
                let color = highlight(axis_color(n), on);
                let pts: Option<Vec<Pos2>> = self.plane_quad(n).iter().map(|p| s(*p)).collect();
                if let Some(pts) = pts {
                    let fill = color.gamma_multiply(if on { 0.55 } else { 0.28 });
                    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(1.0, color)));
                }
            }
            let mut order = [0usize, 1, 2];
            order.sort_by(|a, b| {
                let da = (self.axis_segment(*a).1 - self.eye).length_squared();
                let db = (self.axis_segment(*b).1 - self.eye).length_squared();
                db.total_cmp(&da)
            });
            for k in order {
                let on = active == Some(GizmoPart::Axis(k));
                let color = highlight(axis_color(k), on);
                let (a, b) = self.axis_segment(k);
                let (Some(a), Some(b)) = (s(a), s(b)) else { continue };
                painter.line_segment([a, b], Stroke::new(if on { 3.5 } else { 2.5 }, color));
                let dir = b - a;
                let len = dir.length();
                if mode == GizmoMode::Scale {
                    let cube = Rect::from_center_size(b, egui::Vec2::splat(CUBE_POINTS * 2.0));
                    painter.rect_filled(cube, 1.0, color);
                } else if len > 1.0 {
                    let d = dir / len;
                    let side = egui::vec2(-d.y, d.x);
                    let head = 11.0f32.min(len * 0.5);
                    let tip = b + d * head * 0.6;
                    let base = b - d * head * 0.4;
                    painter.add(egui::Shape::convex_polygon(
                        vec![tip, base + side * head * 0.42, base - side * head * 0.42],
                        color,
                        Stroke::NONE,
                    ));
                }
            }
        }
        if let Some(c) = s(self.center) {
            if mode == GizmoMode::Scale {
                let on = active == Some(GizmoPart::Center);
                let fill = if on { Color32::WHITE } else { Color32::from_gray(200).gamma_multiply(0.8) };
                let r = Rect::from_center_size(c, egui::Vec2::splat(CENTER_POINTS * 2.0));
                painter.rect(r, 1.0, fill, Stroke::new(1.0, Color32::from_black_alpha(180)), egui::StrokeKind::Middle);
            } else {
                painter.circle(c, 3.5, Color32::WHITE, Stroke::new(1.0, Color32::from_black_alpha(180)));
            }
        }
    }
}
