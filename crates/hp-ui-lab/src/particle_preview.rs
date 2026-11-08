use std::collections::HashSet;
use std::sync::Arc;

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, vec2};
use hp_app::runtime::AssetKind;
use hp_app::state::{ParticleCatalog, ParticleSystemInfo};
use hp_app::{AppState, Severity, t, tf};
use hp_pcf::{Effect, Plan, Points};

use crate::particles::{EffectStatus, ParticleHandle, ParticleSprite, append_sprites, material_key, sort_far_first};

pub const CANVAS: f32 = 300.0;
pub const MAX_PARTICLES: usize = crate::particles::MAX_PER_EFFECT;
pub const MAX_ROWS: usize = 20_000;
pub const VIEW_ID: u32 = 0x5043_4650;
const ROW_H: f32 = 20.0;
const SPIN_DEG_PER_SEC: f32 = 24.0;
const WARM_UP: f32 = 1.5;
const MIN_EXTENT: f32 = 48.0;
const FOV_DEG: f32 = 60.0;
const BACKGROUND: [f32; 3] = [0.09, 0.1, 0.12];
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 6.0;
const DEFAULT_PITCH: f32 = 20.0;
const DEFAULT_YAW: f32 = 215.0;

pub fn matches(query: &str, name: &str, file: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let file = file.to_ascii_lowercase();
    query.split_whitespace().all(|word| {
        let word = word.to_ascii_lowercase();
        match word.strip_prefix('!') {
            Some(neg) => neg.is_empty() || !(name.contains(neg) || file.contains(neg)),
            None => name.contains(&word) || file.contains(&word),
        }
    })
}

struct Playing {
    name: String,
    effect: Effect,
    rebuild_after: Option<u64>,
}

pub struct ParticlePreviewWindow {
    handle: ParticleHandle,
    catalog: Option<Arc<ParticleCatalog>>,
    rows: Vec<u32>,
    rows_key: Option<(usize, usize, String, u64)>,
    playing: Option<Playing>,
    paused: bool,
    spin: bool,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    extent: f32,
    rendered: usize,
    center_z: f32,
    scroll_to_selected: bool,
    last_selected: Option<String>,
}

impl ParticlePreviewWindow {
    pub fn new(handle: ParticleHandle) -> Self {
        Self {
            handle,
            catalog: None,
            rows: Vec::new(),
            rows_key: None,
            playing: None,
            paused: false,
            spin: false,
            yaw: DEFAULT_YAW,
            pitch: DEFAULT_PITCH,
            zoom: 1.0,
            extent: 128.0,
            rendered: 0,
            center_z: 32.0,
            scroll_to_selected: false,
            last_selected: None,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn particle_count(&self) -> usize {
        self.playing.as_ref().map_or(0, |p| p.effect.particle_count())
    }

    fn system(&self, row: usize) -> Option<&ParticleSystemInfo> {
        let i = *self.rows.get(row)? as usize;
        self.catalog.as_ref()?.systems.get(i)
    }

    fn refresh_rows(&mut self, state: &mut AppState) {
        self.catalog = state.assets.particles();
        let search = state.runtime.lab.particle_preview.search.clone();
        let (ptr, len) = self.catalog.as_ref().map_or((0, 0), |c| (Arc::as_ptr(c) as usize, c.systems.len()));
        let key = (ptr, len, search.clone(), state.assets.generation());
        if self.rows_key.as_ref() == Some(&key) {
            return;
        }
        self.rows = match &self.catalog {
            Some(c) => c
                .systems
                .iter()
                .enumerate()
                .filter(|(_, s)| matches(&search, &s.name, &s.file))
                .map(|(i, _)| i as u32)
                .take(MAX_ROWS)
                .collect(),
            None => Vec::new(),
        };
        self.rows_key = Some(key);
    }

    fn status(&self, name: &str) -> Option<EffectStatus> {
        self.handle.lock().ok().map(|s| s.status(name))
    }

    fn start(&mut self, name: &str) {
        let Ok(mut shared) = self.handle.lock() else { return };
        let key = name.trim().to_ascii_lowercase();
        let library = &shared.library;
        let lookup = |n: &str| library.get(&n.to_ascii_lowercase()).cloned();
        let Some(mut effect) = Effect::new(&key, &lookup, 7) else {
            if !shared.missing.contains(&key) {
                shared.wanted.insert(key);
            }
            self.playing = None;
            return;
        };
        let mut rebuild_after = None;
        for child in &effect.missing {
            let child = child.to_ascii_lowercase();
            if !shared.missing.contains(&child) {
                shared.wanted.insert(child);
                rebuild_after = Some(shared.library_gen);
            }
        }
        let looks = &shared.looks;
        effect.set_sequence_frames(&|plan: &Plan| {
            let sheet = looks.get(&material_key(&plan.material))?.sheet.as_ref()?;
            let n = sheet.sequences.iter().map(|q| q.number).max()?.min(63) as usize;
            Some((0..=n as u32).map(|i| sheet.frame_count(i) as u32).collect())
        });
        let points = Points::with_angles([0.0; 3], [0.0; 3]);
        if effect.continuous() {
            for _ in 0..(WARM_UP / 0.05) as usize {
                effect.step_at(0.05, &points, MAX_PARTICLES);
            }
        }
        self.playing = Some(Playing { name: key, effect, rebuild_after });
        self.extent = 128.0;
    }

    fn follow(&mut self, state: &AppState) {
        let selected = state.runtime.lab.particle_preview.selected.clone();
        if selected != self.last_selected {
            self.last_selected.clone_from(&selected);
            self.playing = None;
            self.scroll_to_selected = true;
        }
        let Some(name) = selected else { return };
        let gen_now = self.handle.lock().map_or(0, |s| s.library_gen);
        let stale = self.playing.as_ref().is_none_or(|p| {
            p.name != name.trim().to_ascii_lowercase() || p.rebuild_after.is_some_and(|g| g != gen_now)
        });
        if stale {
            self.start(&name);
        }
    }

    fn step(&mut self, dt: f32) {
        if self.paused {
            return;
        }
        let Some(p) = self.playing.as_mut() else { return };
        p.effect.step_at(dt.clamp(0.0, 0.1), &Points::with_angles([0.0; 3], [0.0; 3]), MAX_PARTICLES);
        if self.spin {
            self.yaw = (self.yaw + SPIN_DEG_PER_SEC * dt.clamp(0.0, 0.1)).rem_euclid(360.0);
        }
        let mut max_r = 0.0f32;
        let (mut sum_z, mut n) = (0.0f32, 0u32);
        for (_, q) in p.effect.particles() {
            if !q.pos.iter().all(|v| v.is_finite()) {
                continue;
            }
            max_r = max_r.max((q.pos[0] * q.pos[0] + q.pos[1] * q.pos[1]).sqrt()).max((q.pos[2] - self.center_z).abs());
            sum_z += q.pos[2];
            n += 1;
        }
        let want = (max_r * 1.15).clamp(MIN_EXTENT, 4096.0);
        self.extent += (want - self.extent) * (dt.clamp(0.0, 0.1) * 2.0).min(1.0);
        if n > 0 {
            self.center_z += (sum_z / n as f32 - self.center_z) * (dt.clamp(0.0, 0.1) * 2.0).min(1.0);
        }
    }

    fn eye_basis(&self) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        let forward = [-cp * cy, -cp * sy, -sp];
        let target = [0.0, 0.0, self.center_z];
        let half = (FOV_DEG.to_radians() * 0.5).tan();
        let dist = self.extent * 1.1 / half * self.zoom;
        let eye = [target[0] - forward[0] * dist, target[1] - forward[1] * dist, target[2] - forward[2] * dist];
        let mut right = [forward[1], -forward[0], 0.0];
        let n = (right[0] * right[0] + right[1] * right[1]).sqrt().max(1e-6);
        right = right.map(|c| c / n);
        let up = [
            right[1] * forward[2] - right[2] * forward[1],
            right[2] * forward[0] - right[0] * forward[2],
            right[0] * forward[1] - right[1] * forward[0],
        ];
        (eye, forward, right, up)
    }

    fn project(&self, p: [f32; 3], rect: Rect) -> Option<Pos2> {
        let (eye, f, r, u) = self.eye_basis();
        let d = [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let depth = dot(d, f);
        if depth < 1.0 {
            return None;
        }
        let half = (FOV_DEG.to_radians() * 0.5).tan();
        let aspect = rect.width() / rect.height().max(1.0);
        let x = dot(d, r) / depth / (half * aspect);
        let y = dot(d, u) / depth / half;
        Some(Pos2::new(rect.center().x + x * rect.width() * 0.5, rect.center().y - y * rect.height() * 0.5))
    }

    fn sprites(&self) -> (Vec<ParticleSprite>, Vec<String>) {
        let mut sprites = Vec::new();
        let mut materials = Vec::new();
        let Some(playing) = &self.playing else { return (sprites, materials) };
        let Ok(shared) = self.handle.lock() else { return (sprites, materials) };
        append_sprites(std::iter::once((7, &playing.effect)), &shared.looks, &mut sprites, &mut materials);
        let (eye, ..) = self.eye_basis();
        sort_far_first(&mut sprites, eye);
        (sprites, materials)
    }

    fn render_gpu(&mut self, state: &mut AppState, size_px: [u32; 2]) -> Option<egui::TextureId> {
        let (eye, forward, right, up) = self.eye_basis();
        if let Some(playing) = &mut self.playing {
            playing.effect.prepare_render(hp_pcf::screen_velocity::view(eye, right, up));
        }
        let (sprites, materials) = self.sprites();
        let AppState { assets, gfx, .. } = state;
        let gfx = gfx.as_mut()?;
        let mut draws = Vec::new();
        if let Some(playing) = &self.playing {
            crate::particles::append_models(std::iter::once((7, &playing.effect)), &mut draws);
        }
        let draws =
            crate::particles::prepare_draws(&sprites, &materials, &draws, eye, &mut **assets, &mut gfx.renderer);
        self.rendered = draws.sprites.len() + draws.models.len();
        let params = hp_render::ViewParams {
            size: size_px,
            kind: hp_render::ViewKind::Perspective { eye, forward, up, fov_y_deg: FOV_DEG, near: 1.0, far: 65_536.0 },
            mode: hp_render::RenderMode::Textured,
            grid: None,
            background: BACKGROUND,
            selection_color: [1.0, 0.3, 0.3, 0.0],
            show_sprites: false,
            show_models: false,
            msaa: true,
            game_pixel_grid: false,
        };
        gfx.renderer.render_particle_preview_ordered(VIEW_ID, &params, &draws.sprites, &draws.models, &draws.batches);
        gfx.view_egui_texture(VIEW_ID)
    }

    fn paint_canvas(&mut self, ui: &egui::Ui, state: &mut AppState, rect: Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, hp_app::data_colors::PREVIEW_BACKDROP);
        let ppp = ui.ctx().pixels_per_point();
        let size_px = [(rect.width() * ppp).round().max(16.0) as u32, (rect.height() * ppp).round().max(16.0) as u32];
        if let Some(tex) = self.render_gpu(state, size_px) {
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            painter.image(tex, rect, uv, Color32::WHITE);
        }
        let grid = Stroke::new(1.0, Color32::from_white_alpha(28));
        let ring: Vec<Option<Pos2>> = (0..=48)
            .map(|k| {
                let a = k as f32 / 48.0 * std::f32::consts::TAU;
                self.project([a.cos() * self.extent, a.sin() * self.extent, 0.0], rect)
            })
            .collect();
        for w in ring.windows(2) {
            if let (Some(a), Some(b)) = (w[0], w[1]) {
                painter.line_segment([a, b], grid);
            }
        }
        if let Some(o) = self.project([0.0; 3], rect) {
            let len = self.extent * 0.35;
            for (dir, color) in [
                ([len, 0.0, 0.0], hp_app::data_colors::CHANNEL_RED),
                ([0.0, len, 0.0], hp_app::data_colors::CHANNEL_GREEN),
                ([0.0, 0.0, len], hp_app::data_colors::CHANNEL_BLUE),
            ] {
                if let Some(e) = self.project(dir, rect) {
                    painter.line_segment([o, e], Stroke::new(1.5, color));
                }
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.lab.particle_preview.open {
            self.playing = None;
            self.last_selected = None;
            return;
        }
        self.refresh_rows(state);
        self.follow(state);
        let dt = if state.frame.dt > 0.0 { state.frame.dt } else { ctx.input(|i| i.stable_dt) };
        self.step(dt);
        let mut open = true;
        hp_app::chrome::Window::new(t!("lab.pp.title"))
            .id(egui::Id::unique("lab_particle_preview"))
            .open(&mut open)
            .default_size([680.0, 420.0])
            .min_size([520.0, 340.0])
            .resizable(true)
            .show(ctx, |ui| self.contents(ui, state));
        if !open {
            state.runtime.lab.particle_preview.open = false;
            if let Some(p) = state.runtime.entities.asset_pick.as_ref()
                && p.kind == AssetKind::Particle
                && p.result.is_none()
            {
                state.runtime.entities.asset_pick = None;
            }
        }
        if state.runtime.lab.particle_preview.open && self.playing.is_some() && !self.paused {
            ctx.request_repaint();
        } else if state.runtime.lab.particle_preview.open {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(300.0, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    self.list_ui(ui, state);
                },
            );
            ui.separator();
            ui.vertical(|ui| self.preview_ui(ui, state));
        });
    }

    fn list_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.set_min_width(280.0);
        let mut search = state.runtime.lab.particle_preview.search.clone();
        let edit =
            ui.add(egui::TextEdit::singleline(&mut search).hint_text(t!("lab.pp.search")).desired_width(f32::INFINITY));
        if edit.changed() {
            state.runtime.lab.particle_preview.search = search;
            self.refresh_rows(state);
        }
        let indexing = self.catalog.is_none() && state.assets.fs().is_some();
        if indexing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(t!("entities.pb_indexing")).weak().small());
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
        } else {
            ui.label(RichText::new(tf!("entities.pb_count", self.rows.len())).weak().small());
        }
        let height = (ui.available_height() - 4.0).max(ROW_H * 6.0);
        let mut area =
            ScrollArea::vertical().id_salt("hp_ui_lab.pp_rows").max_height(height).auto_shrink([false, false]);
        let selected = state.runtime.lab.particle_preview.selected.clone();
        if std::mem::take(&mut self.scroll_to_selected)
            && let Some(sel) = &selected
            && let Some(i) =
                (0..self.rows.len()).find(|&i| self.system(i).is_some_and(|s| s.name.eq_ignore_ascii_case(sel)))
        {
            area = area.vertical_scroll_offset((i as f32 * (ROW_H + ui.spacing().item_spacing.y) - 60.0).max(0.0));
        }
        let mut clicked = None;
        let mut activated = None;
        area.show_rows(ui, ROW_H, self.rows.len(), |ui, range| {
            for i in range {
                let Some(sys) = self.system(i) else { continue };
                let is_sel = selected.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(&sys.name));
                let resp = ui.selectable_label(is_sel, &sys.name).on_hover_text(&sys.file);
                if resp.clicked() {
                    clicked = Some(sys.name.clone());
                }
                if resp.double_clicked() {
                    activated = Some(sys.name.clone());
                }
            }
        });
        if let Some(name) = clicked {
            state.runtime.lab.particle_preview.selected = Some(name);
        }
        if let Some(name) = activated {
            state.runtime.lab.particle_preview.selected = Some(name.clone());
            assign(state, &name);
        }
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let selected = state.runtime.lab.particle_preview.selected.clone();
        let width = ui.available_width().clamp(CANVAS, 640.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, CANVAS), Sense::click_and_drag());
        if response.dragged() {
            let delta = response.drag_delta();
            self.yaw = (self.yaw - delta.x * 0.4).rem_euclid(360.0);
            self.pitch = (self.pitch + delta.y * 0.4).clamp(-85.0, 85.0);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 - scroll * 0.002)).clamp(MIN_ZOOM, MAX_ZOOM);
            }
        }
        if response.double_clicked() {
            self.yaw = DEFAULT_YAW;
            self.pitch = DEFAULT_PITCH;
            self.zoom = 1.0;
        }
        self.paint_canvas(ui, state, rect);
        let status = selected.as_deref().and_then(|s| self.status(s));
        if let Some(status) = status {
            let (text, warn) = match status {
                EffectStatus::Loaded => (None, false),
                EffectStatus::Loading => (Some(t!("lab.pp.loading")), false),
                EffectStatus::NotFound => (Some(t!("lab.pp.missing")), true),
            };
            if let Some(text) = text {
                let color = if warn { hp_app::theme::warning() } else { Color32::from_gray(180) };
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(13.0), color);
            }
        } else if selected.is_none() {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                t!("entities.pb_select_hint"),
                FontId::proportional(13.0),
                Color32::from_gray(150),
            );
        }
        ui.label(RichText::new(t!("lab.pp.hint")).small().weak());
        ui.horizontal(|ui| {
            let label = if self.paused { t!("lab.pp.play") } else { t!("lab.pp.pause") };
            if ui.button(label).clicked() {
                self.paused = !self.paused;
            }
            if ui.button(t!("lab.pp.restart")).clicked()
                && let Some(s) = &selected
            {
                self.start(s);
            }
            ui.checkbox(&mut self.spin, t!("lab.pp.spin"));
        });
        if let Some(sel) = &selected {
            ui.add(egui::Label::new(RichText::new(sel).strong()).truncate());
            let info = self.catalog.as_ref().and_then(|c| c.systems.iter().find(|s| s.name.eq_ignore_ascii_case(sel)));
            if let Some(i) = info {
                ui.label(RichText::new(tf!("entities.pb_file", i.file)).small().weak());
            }
            ui.label(RichText::new(tf!("lab.pp.particles", self.particle_count())).small().weak());
            self.unsupported_ui(ui, sel);
            if self.playing.as_ref().is_some_and(|p| p.effect.hierarchy_limited) {
                ui.label(
                    RichText::new(tf!("lab.pp.hierarchy_limit", hp_pcf::sim::MAX_SYSTEMS))
                        .small()
                        .color(hp_app::theme::warning()),
                );
            }
        }
        let pick_waiting = state
            .runtime
            .entities
            .asset_pick
            .as_ref()
            .is_some_and(|p| p.kind == AssetKind::Particle && p.result.is_none());
        let targets = particle_entities(state).len();
        ui.horizontal_wrapped(|ui| {
            let enabled = selected.is_some() && (pick_waiting || targets > 0);
            let label = if pick_waiting { t!("lab.pp.assign_field").to_owned() } else { tf!("lab.pp.assign", targets) };
            if ui.add_enabled(enabled, egui::Button::new(label)).on_hover_text(t!("lab.pp.assign_tip")).clicked()
                && let Some(s) = &selected
            {
                assign(state, s);
            }
            if ui.add_enabled(selected.is_some(), egui::Button::new(t!("entities.sb_copy"))).clicked()
                && let Some(s) = &selected
            {
                ui.ctx().copy_text(s.clone());
            }
            if ui.add_enabled(selected.is_some(), egui::Button::new(t!("pe.edit_file"))).clicked() {
                state.request(hp_app::Action::ParticleEditor);
            }
        });
        if state.assets.fs().is_none() {
            ui.label(RichText::new(t!("entities.sb_no_content")).color(hp_app::theme::warning()).small());
        }
    }

    fn unsupported_ui(&self, ui: &mut egui::Ui, name: &str) {
        let Ok(shared) = self.handle.lock() else { return };
        let Some(plan) = shared.library.get(&name.trim().to_ascii_lowercase()) else { return };
        if plan.unsupported.is_empty() {
            return;
        }
        let list: HashSet<&str> = plan.unsupported.iter().map(String::as_str).collect();
        let mut sorted: Vec<&str> = list.into_iter().collect();
        sorted.sort_unstable();
        ui.label(RichText::new(tf!("lab.pp.unsupported", sorted.join(", "))).small().color(hp_app::theme::warning()));
    }
}

pub fn particle_entities(state: &AppState) -> Vec<hp_editor::Id> {
    hp_ui_entities::SelectionSummary::of(&state.doc)
        .entities
        .into_iter()
        .filter(|id| {
            state
                .doc
                .scene()
                .entity(*id)
                .is_some_and(|n| n.entity.classname.eq_ignore_ascii_case("info_particle_system"))
        })
        .collect()
}

pub fn assign(state: &mut AppState, name: &str) -> bool {
    if let Some(p) = state.runtime.entities.asset_pick.as_mut()
        && p.kind == AssetKind::Particle
        && p.result.is_none()
    {
        p.result = Some(name.to_owned());
        return true;
    }
    let ids = particle_entities(state);
    if ids.is_empty() {
        return false;
    }
    let op = hp_ui_entities::Op::SetKey {
        target: hp_ui_entities::Target::Entities(ids),
        key: "effect_name".to_owned(),
        value: name.to_owned(),
        merge: false,
    };
    match hp_ui_entities::apply(&mut state.doc, None, op) {
        Ok(_) => true,
        Err(e) => {
            state.notify(Severity::Error, tf!("entities.edit_failed", e));
            false
        }
    }
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("lab.pp.title", "Particle preview", "Aperçu des particules"),
    ("lab.pp.search", "Search particle systems (!word excludes)", "Chercher un système de particules (!mot exclut)"),
    ("lab.pp.loading", "Loading the particle file...", "Chargement du fichier de particules..."),
    ("lab.pp.missing", "No particle file defines this system", "Aucun fichier de particules ne définit ce système"),
    ("lab.pp.play", "Play", "Lecture"),
    ("lab.pp.pause", "Pause", "Pause"),
    ("lab.pp.restart", "Restart", "Relancer"),
    ("lab.pp.spin", "Turn", "Tourner"),
    (
        "lab.pp.hint",
        "Same renderer as the 3D view. Drag to orbit, scroll to zoom, double click to reset the camera",
        "Même moteur de rendu que la vue 3D. Glisser pour orbiter, molette pour zoomer, double-clic pour recentrer la caméra",
    ),
    ("lab.pp.particles", "{0} particles", "{0} particules"),
    ("lab.pp.unsupported", "Not simulated: {0}", "Non simulé : {0}"),
    (
        "lab.pp.hierarchy_limit",
        "The effect contains a child cycle or exceeds {0} collections. Its preview is incomplete.",
        "L'effet contient un cycle d'enfants ou dépasse {0} collections. Son aperçu est incomplet.",
    ),
    ("lab.pp.assign", "Assign to {0} entities", "Assigner à {0} entités"),
    ("lab.pp.assign_field", "Use for this field", "Utiliser pour ce champ"),
    (
        "lab.pp.assign_tip",
        "Write this name in the Particle System Name of the info_particle_system entities (double click does it too)",
        "Écrire ce nom dans le nom du système de particules des entités info_particle_system (le double-clic aussi)",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_words_and_exclusions() {
        assert!(matches("fire", "env_fire_large", "particles/fire.pcf"));
        assert!(matches("fire !large", "env_fire_small", "x.pcf"));
        assert!(!matches("fire !large", "env_fire_large", "x.pcf"));
        assert!(matches("smoke.pcf", "a", "particles/smoke.pcf"));
        assert!(matches("", "anything", "x"));
        assert!(matches("!", "a", "b"));
    }

    #[test]
    fn selection_loads_lists_and_assigns() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let handle = crate::particles::new_handle();
        let mut w = ParticlePreviewWindow::new(handle.clone());
        state.runtime.lab.particle_preview.open = true;
        hp_app::testing::run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        assert_eq!(w.row_count(), 3, "the mock catalog");
        state.runtime.lab.particle_preview.search = "fire !large".into();
        hp_app::testing::run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        assert_eq!(w.row_count(), 1);
        state.runtime.lab.particle_preview.selected = Some("Env_Fire_Small".into());
        hp_app::testing::run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        assert!(handle.lock().unwrap().wanted.contains("env_fire_small"), "the system is requested from the loader");
        let def = hp_pcf::SystemDef {
            name: "Env_Fire_Small".to_owned(),
            material: "particle/fire".to_owned(),
            emitters: vec![hp_pcf::Operator { function: "emit_continuously".into(), ..Default::default() }],
            renderers: vec![hp_pcf::Operator { function: "render_animated_sprites".into(), ..Default::default() }],
            ..Default::default()
        };
        {
            let mut s = handle.lock().unwrap();
            s.library.insert("env_fire_small".into(), Arc::new(Plan::new(&def)));
            s.library_gen += 1;
        }
        for _ in 0..5 {
            w.step(0.05);
            w.follow(&state);
        }
        assert!(w.particle_count() > 0, "the live preview plays");
        assert!(w.particle_count() <= MAX_PARTICLES);
        let id = state.doc.create_point_entity("info_particle_system", glam::DVec3::ZERO, &[]).unwrap();
        state.doc.set_selection(&[hp_editor::ObjRef::Entity(id)]);
        assert!(assign(&mut state, "Env_Fire_Small"));
        assert_eq!(state.doc.scene().entity(id).unwrap().entity.get("effect_name"), Some("Env_Fire_Small"));
        state.runtime.entities.asset_pick = Some(hp_app::runtime::AssetPick {
            kind: AssetKind::Particle,
            entities: vec![id],
            world: false,
            key: "effect_name".into(),
            label: String::new(),
            current: String::new(),
            result: None,
        });
        assert!(assign(&mut state, "other"));
        assert_eq!(state.runtime.entities.asset_pick.as_ref().and_then(|p| p.result.clone()), Some("other".into()));
    }

    #[test]
    fn a_closed_window_stays_idle() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut w = ParticlePreviewWindow::new(crate::particles::new_handle());
        state.runtime.lab.particle_preview.open = false;
        state.runtime.entities.asset_pick = Some(hp_app::runtime::AssetPick {
            kind: AssetKind::Particle,
            entities: vec![1],
            world: false,
            key: "effect_name".into(),
            label: String::new(),
            current: String::new(),
            result: None,
        });
        hp_app::testing::run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        assert!(w.particle_count() == 0);
    }
}
