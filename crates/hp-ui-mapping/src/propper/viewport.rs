use std::sync::Arc;

use egui::{PointerButton, Pos2, Sense, Vec2};
use glam::{DVec3, Vec3};
use hp_app::{AppState, state::PropPreviewRequest};
use hp_editor::prop_export::PropGeometry;
use hp_geom::Aabb;
use hp_render::{Overlay, OverlayLine, OverlayPoint, RenderMode, ViewKind, ViewParams};

pub(super) struct EditPoint {
    pub key: usize,
    pub position: DVec3,
    pub color: [f32; 4],
}

pub(super) struct Scene<'a> {
    pub geometry: &'a Arc<PropGeometry>,
    pub overlay: &'a Arc<Overlay>,
    pub points: &'a [EditPoint],
    pub revision: u64,
}

pub(super) struct Edit {
    pub key: usize,
    pub position: DVec3,
}

struct Drag {
    key: usize,
    point: DVec3,
    axis: Option<usize>,
    initial: DVec3,
    axis_initial: f64,
    plane_normal: DVec3,
}

pub(super) struct Viewport {
    pub selected: Option<usize>,
    pub wireframe: bool,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    pan: Vec3,
    bounds: Option<Aabb>,
    drag: Option<Drag>,
    overlay: Option<(u64, Option<usize>, Arc<Overlay>)>,
    overlay_revision: u64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            selected: None,
            wireframe: false,
            yaw: 135.0,
            pitch: 25.0,
            zoom: 1.0,
            pan: Vec3::ZERO,
            bounds: None,
            drag: None,
            overlay: None,
            overlay_revision: 0,
        }
    }
}

fn axis(index: usize) -> DVec3 {
    [DVec3::X, DVec3::Y, DVec3::Z][index]
}

fn plane_hit(origin: DVec3, direction: DVec3, point: DVec3, normal: DVec3) -> Option<DVec3> {
    let denominator = direction.dot(normal);
    (denominator.abs() > 1e-8)
        .then(|| origin + direction * (point - origin).dot(normal) / denominator)
        .filter(|p| p.is_finite())
}

fn axis_hit(origin: DVec3, direction: DVec3, point: DVec3, axis: DVec3) -> Option<f64> {
    let w = origin - point;
    let b = direction.dot(axis);
    let denominator = 1.0 - b * b;
    (denominator > 1e-8).then(|| (w.dot(axis) - b * w.dot(direction)) / denominator).filter(|v| v.is_finite())
}

fn segment_distance(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let delta = b - a;
    let fraction =
        if delta.length_sq() > 1e-6 { ((point - a).dot(delta) / delta.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
    point.distance(a + delta * fraction)
}

impl Viewport {
    pub fn fit(&mut self, geometry: &PropGeometry, points: &[EditPoint]) {
        let bounds =
            Aabb::from_points(geometry.tris.iter().flat_map(|t| t.pos).chain(points.iter().map(|p| p.position)));
        self.bounds = Some(bounds);
        self.pan = Vec3::ZERO;
        self.zoom = 1.0;
        self.overlay = None;
    }

    fn view(&self, size: [u32; 2]) -> ViewParams {
        let bounds = self.bounds.unwrap_or_else(|| Aabb::new(DVec3::splat(-16.0), DVec3::splat(16.0)));
        let center = bounds.center().as_vec3() + self.pan;
        let radius = (bounds.size().length() as f32 * 0.5).max(1.0);
        let distance = radius / (40.0f32.to_radians() * 0.5).sin() * self.zoom;
        let yaw = self.yaw.to_radians();
        let pitch = self.pitch.to_radians();
        let forward = Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), -pitch.sin());
        ViewParams {
            size,
            kind: ViewKind::Perspective {
                eye: (center - forward * distance).to_array(),
                forward: forward.to_array(),
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 40.0,
                near: (distance * 0.01).max(0.01),
                far: (distance + radius * 100.0).max(1024.0),
            },
            mode: if self.wireframe { RenderMode::TexturedWire } else { RenderMode::Textured },
            grid: None,
            background: [0.15, 0.16, 0.18],
            selection_color: [0.0; 4],
            show_sprites: false,
            show_models: false,
            msaa: true,
            game_pixel_grid: false,
        }
    }

    fn decorated_overlay(&mut self, scene: &Scene<'_>, arm: f64) -> Arc<Overlay> {
        if let Some((revision, selected, overlay)) = &self.overlay
            && *revision == scene.revision
            && *selected == self.selected
        {
            return Arc::clone(overlay);
        }
        let mut overlay = (**scene.overlay).clone();
        for point in scene.points {
            overlay.points.push(OverlayPoint {
                pos: point.position.as_vec3().to_array(),
                color: if self.selected == Some(point.key) { [1.0, 1.0, 0.1, 1.0] } else { point.color },
                size_px: 8.0,
            });
        }
        if let Some(point) = scene.points.iter().find(|p| Some(p.key) == self.selected) {
            for (index, color) in
                [[1.0, 0.2, 0.15, 1.0], [0.2, 1.0, 0.25, 1.0], [0.2, 0.5, 1.0, 1.0]].into_iter().enumerate()
            {
                let end = point.position + axis(index) * arm;
                overlay.lines_on_top.push(OverlayLine {
                    a: point.position.as_vec3().to_array(),
                    b: end.as_vec3().to_array(),
                    color,
                });
                overlay.points.push(OverlayPoint { pos: end.as_vec3().to_array(), color, size_px: 7.0 });
            }
        }
        let overlay = Arc::new(overlay);
        self.overlay_revision = self.overlay_revision.wrapping_add(1);
        self.overlay = Some((scene.revision, self.selected, Arc::clone(&overlay)));
        overlay
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut AppState, scene: Scene<'_>) -> Option<Edit> {
        if self.bounds.is_none() {
            self.fit(scene.geometry, &[]);
        }
        let size = Vec2::new(ui.available_width().max(160.0), (ui.available_height() - 40.0).clamp(200.0, 620.0));
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let pixels = ui.ctx().pixels_per_point();
        let render_size = [
            (size.x * pixels).round().clamp(16.0, 2048.0) as u32,
            (size.y * pixels).round().clamp(16.0, 2048.0) as u32,
        ];
        let delta = ui.input(|i| i.pointer.delta());
        if response.dragged_by(PointerButton::Secondary) {
            self.yaw = (self.yaw - delta.x * 0.4).rem_euclid(360.0);
            self.pitch = (self.pitch + delta.y * 0.4).clamp(-89.0, 89.0);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.zoom = (self.zoom * (-scroll * 0.002).exp()).clamp(0.05, 50.0);
        }
        let mut params = self.view(render_size);
        let screen_pixel = |position: Pos2| {
            [
                ((position.x - rect.min.x) / size.x) * render_size[0] as f32,
                ((position.y - rect.min.y) / size.y) * render_size[1] as f32,
            ]
        };
        let to_screen = |position: DVec3| {
            params.world_to_screen(position.as_vec3().to_array()).map(|p| {
                rect.min + Vec2::new(p[0] / render_size[0] as f32 * size.x, p[1] / render_size[1] as f32 * size.y)
            })
        };
        let radius = self.bounds.map_or(32.0, |b| b.size().length() * 0.5).max(1.0);
        let arm = radius * 0.35;
        if response.dragged_by(PointerButton::Middle)
            && let Some(pointer) = response.interact_pointer_pos()
            && let ViewKind::Perspective { forward, .. } = params.kind
        {
            let normal = Vec3::from(forward).as_dvec3();
            let center = self.bounds.unwrap().center() + self.pan.as_dvec3();
            let (o1, d1) = params.screen_ray(screen_pixel(pointer - delta));
            let (o2, d2) = params.screen_ray(screen_pixel(pointer));
            if let (Some(a), Some(b)) = (
                plane_hit(Vec3::from(o1).as_dvec3(), Vec3::from(d1).as_dvec3(), center, normal),
                plane_hit(Vec3::from(o2).as_dvec3(), Vec3::from(d2).as_dvec3(), center, normal),
            ) {
                self.pan += (a - b).as_vec3();
            }
        }
        if (response.drag_started_by(PointerButton::Primary) || response.clicked_by(PointerButton::Primary))
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let nearest = scene
                .points
                .iter()
                .filter_map(|point| to_screen(point.position).map(|p| (point, p.distance(pointer))))
                .filter(|(_, distance)| *distance <= 10.0)
                .min_by(|a, b| {
                    a.1.total_cmp(&b.1)
                        .then_with(|| (Some(b.0.key) == self.selected).cmp(&(Some(a.0.key) == self.selected)))
                });
            let mut picked = nearest.map(|(point, _)| (point, None));
            if picked.is_none()
                && let Some(point) = scene.points.iter().find(|p| Some(p.key) == self.selected)
                && let Some(start) = to_screen(point.position)
            {
                picked = (0..3)
                    .filter_map(|index| {
                        to_screen(point.position + axis(index) * arm)
                            .filter(|end| end.distance(start) > 12.0)
                            .map(|end| (index, segment_distance(pointer, start, end)))
                    })
                    .filter(|(_, distance)| *distance < 7.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(index, _)| (point, Some(index)));
            }
            if let Some((point, axis_index)) = picked {
                self.selected = Some(point.key);
                let (origin, direction) = params.screen_ray(screen_pixel(pointer));
                let origin = Vec3::from(origin).as_dvec3();
                let direction = Vec3::from(direction).as_dvec3().normalize();
                let normal = match params.kind {
                    ViewKind::Perspective { forward, .. } => Vec3::from(forward).as_dvec3(),
                    _ => DVec3::Z,
                };
                let initial = plane_hit(origin, direction, point.position, normal).unwrap_or(point.position);
                self.drag = Some(Drag {
                    key: point.key,
                    point: point.position,
                    axis: axis_index,
                    initial,
                    axis_initial: axis_index
                        .and_then(|index| axis_hit(origin, direction, point.position, axis(index)))
                        .unwrap_or(0.0),
                    plane_normal: normal,
                });
            }
        }
        let mut edit = None;
        if response.dragged_by(PointerButton::Primary)
            && let (Some(drag), Some(pointer)) = (&self.drag, response.interact_pointer_pos())
        {
            let (origin, direction) = params.screen_ray(screen_pixel(pointer));
            let origin = Vec3::from(origin).as_dvec3();
            let direction = Vec3::from(direction).as_dvec3().normalize();
            let position = match drag.axis {
                Some(index) => axis_hit(origin, direction, drag.point, axis(index))
                    .map(|t| drag.point + axis(index) * (t - drag.axis_initial)),
                None => plane_hit(origin, direction, drag.point, drag.plane_normal)
                    .map(|point| drag.point + point - drag.initial),
            };
            if let Some(position) = position.filter(|p| p.is_finite()) {
                edit = Some(Edit { key: drag.key, position });
            }
        }
        if !ui.input(|i| i.pointer.button_down(PointerButton::Primary)) {
            self.drag = None;
        }
        params = self.view(render_size);
        let overlay = self.decorated_overlay(&scene, arm);
        let texture = state.gfx.as_mut().and_then(|gfx| {
            state.assets.prop_preview(
                gfx,
                &PropPreviewRequest {
                    geometry: scene.geometry,
                    params: &params,
                    overlay: &overlay,
                    revision: self.overlay_revision,
                },
            )
        });
        if let Some(texture) = texture {
            ui.painter().image(
                texture,
                rect,
                egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(38, 41, 46));
        }
        edit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perspective_axis_handles_match_the_rendered_projection_on_all_axes() {
        let view = Viewport { bounds: Some(Aabb::new(DVec3::splat(-32.0), DVec3::splat(32.0))), ..Default::default() };
        for size in [[800, 600], [1600, 1200], [340, 620]] {
            let params = view.view(size);
            let point = DVec3::new(2.5, -3.0, 8.25);
            for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
                for offset in [-11.0, 0.0, 21.5] {
                    let pixel = params.world_to_screen((point + axis * offset).as_vec3().to_array()).unwrap();
                    let (origin, direction) = params.screen_ray([pixel[0], pixel[1]]);
                    let hit = axis_hit(
                        Vec3::from(origin).as_dvec3(),
                        Vec3::from(direction).as_dvec3().normalize(),
                        point,
                        axis,
                    )
                    .unwrap();
                    assert!((hit - offset).abs() < 0.002, "{size:?} {axis:?}: {hit} != {offset}");
                }
            }
        }
        assert!(axis_hit(DVec3::ZERO, DVec3::X, DVec3::ZERO, DVec3::X).is_none());
        assert!(plane_hit(DVec3::ZERO, DVec3::X, DVec3::ZERO, DVec3::Z).is_none());
    }
}
