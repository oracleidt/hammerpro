use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, vec2};
use hp_app::{AppState, t, tf};
use hp_pcf::{Effect, Plan, Points};

use super::session::{EditorSession, PreviewOptions};
use crate::particles::{
    ParticleHandle, append_sprites, instances_of, material_key, read_material_look, sort_far_first,
};

pub const EDITOR_VIEW_ID: u32 = 0x5043_4645;
const MAX_PREVIEW_PARTICLES: usize = hp_pcf::sim::MAX_EFFECT_PARTICLES;

pub(super) struct Preview {
    handle: ParticleHandle,
    effect: Option<Effect>,
    key: Option<(u64, Option<usize>, u64)>,
    extent: f32,
    center: [f32; 3],
    dragging: Option<usize>,
}

impl Preview {
    pub fn new(handle: ParticleHandle) -> Self {
        Self { handle, effect: None, key: None, extent: 128.0, center: [0.0; 3], dragging: None }
    }

    fn rebuild(&mut self, state: &AppState, session: &EditorSession) {
        let key = (session.revision, session.selected, session.preview.restart);
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.effect = None;
        let Some(name) = session.selected.and_then(|id| session.doc.system_name(id)) else { return };
        let plans: HashMap<String, Arc<Plan>> =
            session.doc.definitions().iter().map(|s| (s.name.to_ascii_lowercase(), Arc::new(Plan::new(s)))).collect();
        let lookup = |name: &str| plans.get(&name.to_ascii_lowercase()).cloned();
        self.effect = Effect::new(name, &lookup, 17);
        if let Some(fs) = state.assets.fs()
            && let Ok(mut shared) = self.handle.lock()
            && let Some(effect) = &self.effect
        {
            for (plan, _) in effect.systems() {
                let key = material_key(&plan.material);
                if !shared.looks.contains_key(&key) {
                    shared.looks.insert(key.clone(), Arc::new(read_material_look(fs, &key)));
                }
            }
        }
        if let Some(effect) = &mut self.effect {
            if let Ok(shared) = self.handle.lock() {
                effect.set_sequence_frames(&|plan: &Plan| {
                    let sheet = shared.looks.get(&material_key(&plan.material))?.sheet.as_ref()?;
                    let n = sheet.sequences.iter().map(|q| q.number).max()?.min(63);
                    Some((0..=n).map(|i| sheet.frame_count(i) as u32).collect())
                });
            }
            if effect.continuous() {
                let points = control_points(&session.preview);
                for _ in 0..30 {
                    effect.step_at(0.05, &points, MAX_PREVIEW_PARTICLES);
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, state: &mut AppState, session: &mut EditorSession) {
        self.rebuild(state, session);
        let dt = state.frame.dt.max(ui.input(|i| i.stable_dt)).clamp(0.0, 0.1);
        let started = Instant::now();
        if !session.preview.paused
            && let Some(effect) = &mut self.effect
        {
            let points = control_points(&session.preview);
            let mut left = dt * session.preview.speed.clamp(0.05, 4.0);
            while left > 0.0 {
                let step = left.min(0.05);
                effect.step_at(step, &points, MAX_PREVIEW_PARTICLES);
                left -= step;
            }
        }
        session.preview.cpu_ms = started.elapsed().as_secs_f32() * 1000.0;
        session.preview.particles = self.effect.as_ref().map_or(0, Effect::particle_count);
        self.fit(dt);
        let width = ui.available_width().max(220.0);
        let height = (width * 0.78).clamp(220.0, 380.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            session.preview.zoom = (session.preview.zoom * (1.0 - scroll * 0.002)).clamp(0.1, 10.0);
        }
        if response.double_clicked() {
            session.preview.yaw = 215.0;
            session.preview.pitch = 20.0;
            session.preview.zoom = 1.0;
        }
        if response.dragged() && self.dragging.is_none() {
            let delta = ui.input(|i| i.pointer.delta());
            session.preview.yaw = (session.preview.yaw - delta.x * 0.4).rem_euclid(360.0);
            session.preview.pitch = (session.preview.pitch + delta.y * 0.4).clamp(-85.0, 85.0);
        }
        self.paint(ui, state, session, rect);
        ui.label(egui::RichText::new(t!("pe.orbit_hint")).weak().small());
        ui.horizontal_wrapped(|ui| {
            let label = if session.preview.paused { t!("lab.pp.play") } else { t!("lab.pp.pause") };
            if ui.button(label).clicked() {
                session.preview.paused = !session.preview.paused;
            }
            if ui.button(t!("lab.pp.restart")).clicked() {
                session.preview.restart = session.preview.restart.wrapping_add(1);
            }
            ui.label(t!("pe.speed"));
            ui.add(egui::DragValue::new(&mut session.preview.speed).speed(0.05).range(0.05..=4.0).suffix("x"));
            ui.checkbox(&mut session.preview.grid, t!("pe.grid"));
            ui.color_edit_button_rgb(&mut session.preview.background);
        });
        ui.label(tf!("pe.preview_stats", session.preview.particles, format!("{:.3}", session.preview.cpu_ms)));
        if session.preview.particles >= MAX_PREVIEW_PARTICLES {
            ui.label(egui::RichText::new(t!("pe.preview_limit")).color(hp_app::theme::warning()));
        }
        if let Some(effect) = &self.effect {
            let mut unsupported: Vec<&str> =
                effect.systems().flat_map(|(p, _)| p.unsupported.iter().map(String::as_str)).collect();
            unsupported.sort_unstable();
            unsupported.dedup();
            if !unsupported.is_empty() {
                ui.label(
                    egui::RichText::new(tf!("lab.pp.unsupported", unsupported.join(", ")))
                        .small()
                        .color(hp_app::theme::warning()),
                );
            }
        }
        egui::CollapsingHeader::new(t!("pe.control_points")).show(ui, |ui| {
            let mut remove = None;
            for (i, point) in session.preview.points.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("CP {i}"));
                    for v in point {
                        ui.add(egui::DragValue::new(v).speed(0.5).range(-131_072.0..=131_072.0));
                    }
                    if i > 0 && ui.small_button(t!("pe.remove")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                session.preview.points.remove(i);
            }
            if session.preview.points.len() < hp_pcf::sim::MAX_CONTROL_POINTS
                && ui.button(t!("pe.add_control_point")).clicked()
            {
                session.preview.points.push([0.0; 3]);
            }
        });
        if !session.preview.paused {
            ui.ctx().request_repaint();
        }
    }

    fn fit(&mut self, dt: f32) {
        let Some(effect) = &self.effect else { return };
        let mut radius = 48.0f32;
        let mut sum = [0.0; 3];
        let mut count = 0.0f32;
        for (_, p) in effect.particles().filter(|(_, p)| p.pos.iter().all(|v| v.is_finite())) {
            for (axis, v) in p.pos.iter().enumerate() {
                radius = radius.max((v - self.center[axis]).abs() + p.radius);
                sum[axis] += v;
            }
            count += 1.0;
        }
        let amount = (dt * 2.0).min(1.0);
        self.extent += (radius.clamp(48.0, 8192.0) * 1.15 - self.extent) * amount;
        if count > 0.0 {
            for (axis, v) in sum.into_iter().enumerate() {
                self.center[axis] += (v / count - self.center[axis]) * amount;
            }
        }
    }

    fn basis(&self, options: &PreviewOptions) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
        let (sy, cy) = options.yaw.to_radians().sin_cos();
        let (sp, cp) = options.pitch.to_radians().sin_cos();
        let forward = [-cp * cy, -cp * sy, -sp];
        let right = [-sy, cy, 0.0];
        let up = [-sp * cy, -sp * sy, cp];
        let distance = self.extent * 1.1 / 30.0f32.to_radians().tan() * options.zoom;
        let eye = std::array::from_fn(|i| self.center[i] - forward[i] * distance);
        (eye, forward, right, up)
    }

    fn project(&self, options: &PreviewOptions, point: [f32; 3], rect: Rect) -> Option<Pos2> {
        let (eye, forward, right, up) = self.basis(options);
        let d: [f32; 3] = std::array::from_fn(|i| point[i] - eye[i]);
        let dot = |v: [f32; 3]| d.iter().zip(v).map(|(a, b)| a * b).sum::<f32>();
        let depth = dot(forward);
        if depth <= 1.0 {
            return None;
        }
        let scale = rect.height() * 0.5 / 30.0f32.to_radians().tan() / depth;
        Some(rect.center() + vec2(dot(right) * scale, -dot(up) * scale))
    }

    fn paint(&mut self, ui: &mut Ui, state: &mut AppState, session: &mut EditorSession, rect: Rect) {
        let painter = ui.painter_at(rect);
        let (eye, forward, right, up) = self.basis(&session.preview);
        let (mut sprites, mut materials) = (Vec::new(), Vec::new());
        if let Some(effect) = &self.effect
            && let Ok(shared) = self.handle.lock()
        {
            append_sprites(std::iter::once((17, effect)), &shared.looks, &mut sprites, &mut materials);
        }
        sort_far_first(&mut sprites, eye);
        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).max(16.0) as u32, (rect.height() * ppp).max(16.0) as u32];
        let instances = instances_of(&sprites, &materials, &mut *state.assets);
        painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
        if let Some(gfx) = &mut state.gfx {
            let params = hp_render::ViewParams {
                size,
                kind: hp_render::ViewKind::Perspective { eye, forward, up, fov_y_deg: 60.0, near: 1.0, far: 65_536.0 },
                mode: hp_render::RenderMode::Textured,
                grid: None,
                background: session.preview.background,
                selection_color: [0.0; 4],
                show_sprites: false,
                show_models: false,
                msaa: true,
                game_pixel_grid: false,
            };
            gfx.renderer.render_particle_preview(EDITOR_VIEW_ID, &params, &instances);
            if let Some(texture) = gfx.view_egui_texture(EDITOR_VIEW_ID) {
                painter.image(texture, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
        }
        if session.preview.grid {
            let step = (self.extent / 8.0).max(8.0);
            for i in -8..=8 {
                for axis in 0..2 {
                    let mut a = [i as f32 * step, -8.0 * step, 0.0];
                    let mut b = [i as f32 * step, 8.0 * step, 0.0];
                    if axis == 1 {
                        a.swap(0, 1);
                        b.swap(0, 1);
                    }
                    if let (Some(a), Some(b)) =
                        (self.project(&session.preview, a, rect), self.project(&session.preview, b, rect))
                    {
                        painter.line_segment([a, b], Stroke::new(1.0, Color32::from_white_alpha(32)));
                    }
                }
            }
        }
        let projections: Vec<_> =
            session.preview.points.iter().map(|p| self.project(&session.preview, *p, rect)).collect();
        for (i, pos) in projections.into_iter().enumerate() {
            let Some(pos) = pos.filter(|p| rect.contains(*p)) else { continue };
            let hit = Rect::from_center_size(pos, vec2(22.0, 22.0));
            let response = ui.interact(hit, ui.scope_id().with(("pe_cp", i)), Sense::drag());
            painter.circle_stroke(pos, 5.0, Stroke::new(1.5, Color32::from_gray(220)));
            painter.text(
                pos + vec2(7.0, -7.0),
                egui::Align2::LEFT_BOTTOM,
                format!("CP {i}"),
                egui::FontId::proportional(11.0),
                Color32::from_gray(230),
            );
            if response.drag_started() {
                self.dragging = Some(i);
            }
            if response.dragged() {
                let delta = ui.input(|input| input.pointer.delta());
                let scale = self.extent * session.preview.zoom * 2.2 / rect.height().max(1.0);
                for axis in 0..3 {
                    session.preview.points[i][axis] += (right[axis] * delta.x - up[axis] * delta.y) * scale;
                }
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            self.dragging = None;
        }
    }
}

fn control_points(options: &PreviewOptions) -> Points {
    let mut points = Points::with_angles(options.points.first().copied().unwrap_or([0.0; 3]), [0.0; 3]);
    points.pos.clone_from(&options.points);
    if points.pos.is_empty() {
        points.pos.push([0.0; 3]);
    }
    points
}
