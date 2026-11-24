use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{PointerButton, Pos2, Sense, Vec2};
use glam::{DVec2, DVec3, Vec3};
use hp_app::state::PropPreviewRequest;
use hp_app::{AppState, t, tf};
use hp_editor::prop_export::{PropGeometry, PropTri};
use hp_geom::Aabb;
use hp_meshio::collision::CollisionLimits;
use hp_meshio::smd::Collision;
use hp_meshio::studio::StudioOptions;
use hp_meshio::{ImportOptions, Model};
use hp_render::{Overlay, OverlayLine, RenderMode, ViewKind, ViewParams};

const PREVIEW_MATERIAL: &str = "dev/dev_measuregeneric01b";

const PIECE_COLORS: [[f32; 4]; 6] = [
    [0.1, 1.0, 0.25, 0.95],
    [1.0, 0.75, 0.1, 0.95],
    [0.25, 0.7, 1.0, 0.95],
    [1.0, 0.3, 0.8, 0.95],
    [0.6, 1.0, 0.9, 0.95],
    [1.0, 0.45, 0.25, 0.95],
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Key {
    pub import: ImportOptions,
    pub collision: Collision,
    pub limits: CollisionLimits,
    pub own: bool,
}

type Job = (Key, Receiver<(Vec<Vec<DVec3>>, bool)>);

pub(super) struct CollisionPreview {
    pub show: bool,
    job: Option<Job>,
    result: Option<(Key, Vec<Vec<DVec3>>, bool)>,
    geometry: Option<(ImportOptions, Arc<PropGeometry>)>,
    overlay: Option<(u64, Arc<Overlay>)>,
    revision: u64,
    yaw: f32,
    pitch: f32,
    zoom: f32,
}

impl Default for CollisionPreview {
    fn default() -> Self {
        Self {
            show: true,
            job: None,
            result: None,
            geometry: None,
            overlay: None,
            revision: 1,
            yaw: 135.0,
            pitch: 25.0,
            zoom: 1.0,
        }
    }
}

pub(super) fn pieces_for(model: &Model, key: Key) -> (Vec<Vec<DVec3>>, bool) {
    let opts = StudioOptions {
        model: String::new(),
        cdmaterials: String::new(),
        static_prop: true,
        surface_prop: String::new(),
        mass: 1.0,
        collision: key.collision,
        limits: key.limits,
        own_collision: key.own,
        lod: hp_meshio::lod::LodOptions { enabled: false, ..hp_meshio::lod::LodOptions::default() },
    };
    hp_meshio::studio::collision_preview(model, &opts)
}

fn geometry_of(model: &Model) -> PropGeometry {
    let tris = model
        .triangles
        .iter()
        .filter(|t| t.pos.iter().all(|p| p.is_finite()))
        .map(|t| PropTri {
            material: PREVIEW_MATERIAL.to_owned(),
            pos: t.pos,
            normal: t.normal,
            uv: t.uv.map(|uv: DVec2| [uv.x, uv.y]),
            disp: None,
        })
        .collect();
    PropGeometry { tris, disps: Vec::new(), pieces: Vec::new(), origin: DVec3::ZERO }
}

impl CollisionPreview {
    pub fn reset(&mut self) {
        *self = Self { show: self.show, ..Self::default() };
    }

    pub fn current(&self, key: Key) -> Option<(&[Vec<DVec3>], bool)> {
        self.result.as_ref().filter(|(k, _, _)| *k == key).map(|(_, pieces, own)| (pieces.as_slice(), *own))
    }

    pub fn update(&mut self, ctx: &egui::Context, source: &Model, key: Key) {
        if let Some((job_key, rx)) = &self.job {
            match rx.try_recv() {
                Ok((pieces, own)) => {
                    self.result = Some((*job_key, pieces, own));
                    self.job = None;
                    self.revision += 1;
                }
                Err(TryRecvError::Disconnected) => self.job = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.job.is_none() && self.result.as_ref().is_none_or(|(k, _, _)| *k != key) {
            let (tx, rx) = channel();
            let model = source.clone();
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(pieces_for(&model, key));
                repaint.request_repaint();
            });
            self.job = Some((key, rx));
        }
        if self.geometry.as_ref().is_none_or(|(opts, _)| *opts != key.import) {
            self.geometry = Some((key.import, Arc::new(geometry_of(source))));
            self.revision += 1;
        }
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    fn overlay(&mut self) -> Arc<Overlay> {
        if let Some((revision, overlay)) = &self.overlay
            && *revision == self.revision
        {
            return Arc::clone(overlay);
        }
        let mut overlay = Overlay::default();
        if self.show
            && let Some((_, pieces, _)) = &self.result
        {
            for (index, piece) in pieces.iter().enumerate() {
                let color = PIECE_COLORS[index % PIECE_COLORS.len()];
                for tri in hp_geom::hull_triangles(piece) {
                    for corner in 0..3 {
                        overlay.lines_on_top.push(OverlayLine {
                            a: tri[corner].as_vec3().to_array(),
                            b: tri[(corner + 1) % 3].as_vec3().to_array(),
                            color,
                        });
                    }
                }
            }
        }
        let overlay = Arc::new(overlay);
        self.overlay = Some((self.revision, Arc::clone(&overlay)));
        overlay
    }

    fn view(&self, bounds: Aabb, size: [u32; 2]) -> ViewParams {
        let center = bounds.center().as_vec3();
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
            mode: RenderMode::Textured,
            grid: None,
            background: [0.15, 0.16, 0.18],
            selection_color: [0.0; 4],
            show_sprites: false,
            show_models: false,
            msaa: true,
            game_pixel_grid: false,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, key: Key) {
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.show, t!("entities.mi_show_collision")).changed() {
                self.revision += 1;
            }
            let label = match self.current(key) {
                _ if key.collision == Collision::None && !key.own => t!("entities.mi_coll_none").to_owned(),
                Some((pieces, own)) if own => tf!("entities.mi_own_pieces", pieces.len()),
                Some((pieces, _)) => tf!("entities.mi_pieces_count", pieces.len()),
                None => t!("entities.mi_coll_computing").to_owned(),
            };
            ui.label(egui::RichText::new(label).weak());
            if self.busy() {
                hp_app::widgets::spinner(ui);
            }
        });
        let Some((_, geometry)) = self.geometry.clone() else { return };
        let size = Vec2::new(ui.available_width().max(200.0), 240.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let delta = ui.input(|i| i.pointer.delta());
        if response.dragged_by(PointerButton::Primary) || response.dragged_by(PointerButton::Secondary) {
            self.yaw = (self.yaw - delta.x * 0.4).rem_euclid(360.0);
            self.pitch = (self.pitch + delta.y * 0.4).clamp(-89.0, 89.0);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (-scroll * 0.002).exp()).clamp(0.1, 20.0);
            }
        }
        let bounds = Aabb::from_points(geometry.tris.iter().flat_map(|t| t.pos));
        if !bounds.min.is_finite() || !bounds.max.is_finite() {
            return;
        }
        let pixels = ui.ctx().pixels_per_point();
        let render_size = [
            (size.x * pixels).round().clamp(16.0, 2048.0) as u32,
            (size.y * pixels).round().clamp(16.0, 2048.0) as u32,
        ];
        let params = self.view(bounds, render_size);
        let overlay = self.overlay();
        let texture = state.gfx.as_mut().and_then(|gfx| {
            state.assets.prop_preview(
                gfx,
                &PropPreviewRequest {
                    geometry: &geometry,
                    params: &params,
                    overlay: &overlay,
                    revision: self.revision,
                },
            )
        });
        match texture {
            Some(texture) => {
                ui.painter().image(
                    texture,
                    rect,
                    egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                ui.painter().rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            }
        }
    }
}
