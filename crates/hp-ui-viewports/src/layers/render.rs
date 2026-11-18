use glam::DVec3;
use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::tf;
use hp_app::view::ViewInfo;
use hp_render::{OverlayLine, RenderStats};

use super::OverlayLayer;
use crate::overlay::pixel_world_size;

const CAMERA_COLOR: [f32; 4] = [0.25, 0.85, 1.0, 1.0];
const RADIUS_COLOR: [f32; 4] = [0.95, 0.75, 0.2, 0.8];
const CAMERA_RAY_PX: f64 = 48.0;
const CAMERA_EYE_PX: f64 = 6.0;
const CIRCLE_SEGMENTS: usize = 96;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(Camera2dLayer), Box::new(CullRadiusLayer)]
}

pub(crate) const TR: TrTable =
    &[("viewport.info_culling", "Chunks: {0} in view, {1} skipped", "Blocs : {0} dans la vue, {1} ignorés")];

pub(crate) fn culling_line(s: &RenderStats) -> String {
    tf!("viewport.info_culling", s.chunks_in_frustum, s.chunks_occluded)
}

fn f3(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

fn flatten(view: &ViewInfo, p: DVec3) -> Option<DVec3> {
    let (r, u, _) = view.kind.axes_2d()?;
    let mut out = DVec3::ZERO;
    out[r] = p[r];
    out[u] = p[u];
    Some(out)
}

struct Camera2dLayer;

impl OverlayLayer for Camera2dLayer {
    fn name(&self) -> &'static str {
        "camera_2d"
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        paint_lines(view, painter, &camera_lines(state, view));
    }
}

fn paint_lines(view: &ViewInfo, painter: &egui::Painter, lines: &[OverlayLine]) {
    for l in lines {
        let a = DVec3::new(f64::from(l.a[0]), f64::from(l.a[1]), f64::from(l.a[2]));
        let b = DVec3::new(f64::from(l.b[0]), f64::from(l.b[1]), f64::from(l.b[2]));
        let (Some(pa), Some(pb)) = (view.world_to_screen(a), view.world_to_screen(b)) else { continue };
        let c = l.color.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        let color = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
        painter.line_segment([pa, pb], egui::Stroke::new(1.0, color));
    }
}

fn camera_lines(state: &AppState, view: &ViewInfo) -> Vec<OverlayLine> {
    let mut out = Vec::new();
    if !view.is_2d() || !state.settings.render.camera_in_2d {
        return out;
    }
    let cam = &view.cam3d;
    let Some(eye) = flatten(view, cam.pos).filter(|e| e.is_finite()) else { return out };
    let px = pixel_world_size(view, eye);
    let Some((r, u, _)) = view.kind.axes_2d() else { return out };
    let mut push = |a: DVec3, b: DVec3, color: [f32; 4]| {
        if a.is_finite() && b.is_finite() {
            out.push(OverlayLine { a: f3(a), b: f3(b), color });
        }
    };
    let radius = CAMERA_EYE_PX * px;
    let point = |angle: f64| {
        let mut p = eye;
        p[r] += radius * angle.cos();
        p[u] += radius * angle.sin();
        p
    };
    for i in 0..16 {
        let (a0, a1) = (i as f64 / 16.0 * std::f64::consts::TAU, (i + 1) as f64 / 16.0 * std::f64::consts::TAU);
        push(point(a0), point(a1), CAMERA_COLOR);
    }
    let forward = flatten(view, cam.forward()).unwrap_or(DVec3::ZERO);
    let len = forward.length();
    if len >= 1e-3 {
        let dir = forward / len;
        let ray = CAMERA_RAY_PX * px;
        push(eye + dir * radius, eye + dir * ray, CAMERA_COLOR);
        let aspect = f64::from(view.rect.width().max(1.0) / view.rect.height().max(1.0));
        let half = ((cam.fov_y_deg.to_radians() * 0.5).tan() * aspect).atan();
        let side = |sign: f64| {
            let (s, c) = (sign * half).sin_cos();
            let mut d = DVec3::ZERO;
            d[r] = dir[r] * c - dir[u] * s;
            d[u] = dir[r] * s + dir[u] * c;
            d
        };
        let faded = [CAMERA_COLOR[0], CAMERA_COLOR[1], CAMERA_COLOR[2], 0.55];
        for sign in [-1.0, 1.0] {
            push(eye, eye + side(sign) * ray * 0.75, faded);
        }
    }
    out
}

struct CullRadiusLayer;

impl OverlayLayer for CullRadiusLayer {
    fn name(&self) -> &'static str {
        "cull_radius"
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        paint_lines(view, painter, &cull_lines(state, view));
    }
}

fn cull_lines(state: &AppState, view: &ViewInfo) -> Vec<OverlayLine> {
    let r = &state.settings.render;
    let radius = f64::from(r.cull_radius);
    let usable = radius.is_finite() && radius > 0.0;
    if !view.is_2d() || !r.radius_culling || !usable {
        return Vec::new();
    }
    let Some(center) = flatten(view, view.cam3d.pos).filter(|c| c.is_finite()) else { return Vec::new() };
    let Some((a, b, _)) = view.kind.axes_2d() else { return Vec::new() };
    let point = |i: usize| {
        let t = i as f64 / CIRCLE_SEGMENTS as f64 * std::f64::consts::TAU;
        let mut p = center;
        p[a] += radius * t.cos();
        p[b] += radius * t.sin();
        p
    };
    (0..CIRCLE_SEGMENTS).map(|i| OverlayLine { a: f3(point(i)), b: f3(point(i + 1)), color: RADIUS_COLOR }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2, vec2};
    use hp_app::view::{Camera2d, Camera3d, ViewKind};
    use hp_render::Overlay;

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 1,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0)),
            cam3d: Camera3d { pos: DVec3::new(100.0, 50.0, 30.0), yaw_deg: 90.0, pitch_deg: 0.0, ..Default::default() },
            cam2d: Camera2d::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn camera_is_drawn_in_2d_views_only() {
        let state = hp_app::testing::test_state();
        assert!(camera_lines(&state, &view(ViewKind::Perspective)).is_empty());
        let out = camera_lines(&state, &view(ViewKind::Top));
        assert_eq!(out.len(), 16 + 3, "eye circle, direction and field of view");
        let ray = out[16];
        assert!(ray.b[1] > ray.a[1] && (ray.b[0] - ray.a[0]).abs() < 1e-3, "{ray:?}");
        assert!(ray.a[2] == 0.0 && ray.b[2] == 0.0, "flattened on the view plane");
        assert_eq!(camera_lines(&state, &view(ViewKind::Side)).len(), 16);
        let mut overlay = Overlay::default();
        Camera2dLayer.add(&state, &view(ViewKind::Top), &mut overlay);
        assert!(overlay.is_empty(), "painted over the cached image, so moving the 3D camera never redraws a 2D view");
        let mut off = state;
        off.settings.render.camera_in_2d = false;
        assert!(camera_lines(&off, &view(ViewKind::Top)).is_empty());
    }

    #[test]
    fn culling_circle_follows_the_setting() {
        let mut state = hp_app::testing::test_state();
        assert!(cull_lines(&state, &view(ViewKind::Top)).is_empty(), "off by default");
        state.settings.render.radius_culling = true;
        state.settings.render.cull_radius = 1000.0;
        let out = cull_lines(&state, &view(ViewKind::Side));
        assert_eq!(out.len(), CIRCLE_SEGMENTS);
        let p = out[0].a;
        assert!((p[0] - 1100.0).abs() < 1e-3 && (p[2] - 30.0).abs() < 1e-3, "{p:?}");
        state.settings.render.cull_radius = f32::NAN;
        assert!(cull_lines(&state, &view(ViewKind::Top)).is_empty());
        crate::register_translations();
        let s = RenderStats { chunks_in_frustum: 12, chunks_occluded: 5, ..Default::default() };
        assert!(culling_line(&s).contains("12") && culling_line(&s).contains('5'));
    }
}
