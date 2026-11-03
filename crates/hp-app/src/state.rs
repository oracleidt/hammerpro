use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use glam::{DVec2, DVec3};

use crate::actions::{Action, KeyBindings};
use crate::console::{Console, Severity, Toasts};
use crate::settings::{RenderModeSetting, SelectModeSetting, Settings};
use crate::tool::ToolId;
use crate::view::{Camera2d, Camera3d, ViewKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetStatus {
    NotConfigured,
    Mounting,
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, Default)]
pub struct MaterialInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub shader: String,
    pub tool: Option<hp_vmt::ToolKind>,
    pub translucent: bool,
    pub alpha_test: bool,
    pub shows_alpha: bool,
    pub additive: bool,
    pub is_water: bool,
    pub is_blend: bool,
    pub surface_prop: Option<String>,
    pub keywords: Vec<String>,
    pub base_texture: Option<String>,
    pub source: u32,
    pub missing: bool,
    pub decal_scale: Option<f32>,
    pub self_illum: bool,
    pub env_map: bool,
}

pub type CollisionTriangles = Arc<Vec<[[f32; 3]; 3]>>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectorOverride {
    pub id: hp_vmf::Id,
    pub on: Option<bool>,
    pub fov: Option<f64>,
    pub shadows: Option<bool>,
    pub texture: Option<String>,
    pub light_world: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerModelPose {
    pub model: String,
    pub feet: glam::DVec3,
    pub yaw: f64,
    pub sequence: String,
    pub move_x: f32,
    pub move_y: f32,
    pub time: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThumbState {
    Loading,
    Ready { texture: egui::TextureId, size: [u32; 2] },
    Missing,
}

pub type ImageConverter = fn(&[PathBuf], &ImportOptions) -> Vec<Result<String, String>>;

#[derive(Clone, Debug, Default)]
pub struct ModelInfo {
    pub path: String,
    pub bounds_min: DVec3,
    pub bounds_max: DVec3,
    pub skins: u32,
    pub body_parts: u32,
    pub triangles: u32,
    pub materials: Vec<String>,
    pub is_static_prop: bool,
    pub surface_prop: String,
    pub hull_min: Option<DVec3>,
    pub hull_max: Option<DVec3>,
    pub body_groups: Vec<BodyGroupInfo>,
    pub sequences: Vec<String>,
    pub sequence_frames: Vec<u32>,
    pub stats: ModelStats,
}

impl ModelInfo {
    pub fn preview_sequence(&self, preferred: Option<&str>) -> Option<usize> {
        let animated = |i: usize| self.sequence_frames.get(i).copied().unwrap_or(1) > 1;
        if let Some(p) = preferred.map(str::trim).filter(|p| !p.is_empty())
            && let Some(i) = self.sequences.iter().position(|s| s.eq_ignore_ascii_case(p))
        {
            return Some(i);
        }
        let n = self.sequences.len();
        (0..n)
            .find(|&i| animated(i) && self.sequences[i].to_ascii_lowercase().contains("idle"))
            .or_else(|| (0..n).find(|&i| animated(i)))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelStats {
    pub vertices: u32,
    pub meshes: u32,
    pub triangles_all: u32,
    pub vertices_all: u32,
    pub bones: u32,
    pub lods: u32,
    pub mass: f32,
    pub version: i32,
    pub hitboxes: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BodyGroupInfo {
    pub name: String,
    pub base: i32,
    pub choices: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParticleSystemInfo {
    pub name: String,
    pub file: String,
    pub material: String,
    pub source: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ParticleCatalog {
    pub systems: Vec<ParticleSystemInfo>,
    pub files: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum ModelState {
    Loading,
    Ready(Arc<ModelInfo>),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub dest_dir: PathBuf,
    pub prefix: String,
    pub surface_prop: Option<String>,
    pub generate_mipmaps: bool,
    pub overwrite: bool,
}

pub mod preview_overlay {
    pub const BBOX: u32 = 1;
    pub const HULL: u32 = 1 << 1;
    pub const HITBOXES: u32 = 1 << 2;
    pub const SEQ_BOX: u32 = 1 << 3;
    pub const BONES: u32 = 1 << 4;
    pub const ATTACHMENTS: u32 = 1 << 5;
    pub const NORMALS: u32 = 1 << 6;
    pub const ORIGIN: u32 = 1 << 7;
    pub const ILLUM: u32 = 1 << 8;
    pub const GROUND: u32 = 1 << 9;
    pub const PHYSICS: u32 = 1 << 10;
    pub const ALL: [u32; 11] =
        [BBOX, HULL, HITBOXES, SEQ_BOX, BONES, ATTACHMENTS, NORMALS, ORIGIN, ILLUM, GROUND, PHYSICS];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewCamera {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub zoom: f32,
    pub skin: u32,
    pub wireframe: bool,
    pub overlays: u32,
    pub fov_deg: f32,
    pub anim: Option<usize>,
    pub anim_time: f32,
}

impl PreviewCamera {
    pub fn shows(&self, overlay: u32) -> bool {
        self.overlays & overlay != 0
    }
}

impl Default for PreviewCamera {
    fn default() -> Self {
        Self {
            yaw_deg: 135.0,
            pitch_deg: 20.0,
            zoom: 1.0,
            skin: 0,
            wireframe: false,
            overlays: 0,
            fov_deg: 40.0,
            anim: None,
            anim_time: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneOptions {
    pub show_models: bool,
    pub show_helpers: bool,
    pub show_tool_textures: bool,
    pub lighting_preview: bool,
    pub model_lods: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SunReport {
    pub direction: Option<[f32; 3]>,
    pub color: [f32; 3],
    pub ambient: [f32; 3],
    pub spread_deg: f32,
    pub origin: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightPreviewReport {
    pub lights: u32,
    pub directional: u32,
    pub left_out: u32,
    pub crowded_clusters: u32,
    pub max_per_cluster: u32,
    pub sky_occlusion: bool,
    pub sky_layers: [u32; 2],
    pub hdr: bool,
    pub sun: Option<SunReport>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FgdReport {
    pub files: Vec<std::path::PathBuf>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub loading: bool,
}

pub trait AssetsApi {
    fn status(&self) -> AssetStatus;
    fn fs(&self) -> Option<&hp_fs::GameFs>;
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd>;
    fn generation(&self) -> u64;
    fn late_mount(&self) -> usize {
        0
    }
    fn mount_progress(&self) -> Option<hp_fs::MountSnapshot> {
        None
    }
    fn remount(&mut self, settings: &Settings);
    fn reload_fgd(&mut self, settings: &Settings);
    fn fgd_report(&self) -> FgdReport {
        FgdReport::default()
    }
    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>>;
    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>>;
    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>>;
    fn source_label(&self, source: u32) -> String;
    fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>>;
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> ThumbState;
    fn model_info(&mut self, path: &str) -> ModelState;
    fn set_player_model(&mut self, _pose: Option<PlayerModelPose>) {}
    fn set_play_transforms(&mut self, _transforms: &[(hp_vmf::Id, glam::DMat4)]) {}
    fn set_game_hidden(&mut self, _objs: &[hp_editor::ObjRef]) {}
    fn set_model_lods(&mut self, _on: bool) {}
    fn set_play_hidden(&mut self, _objs: &[hp_editor::ObjRef]) {}
    fn set_play_world(
        &mut self,
        _world: Option<Arc<hp_editor::limits::world::BspWorld>>,
        _visible: Option<Arc<Vec<bool>>>,
    ) {
    }
    fn set_baked_prop_lighting(
        &mut self,
        _props: Option<Arc<hp_editor::limits::world_light::PropLighting>>,
        _region: Option<[[f64; 3]; 2]>,
    ) {
    }
    fn set_light_styles(&mut self, _values: &[f32]) {}
    fn set_projector_overrides(&mut self, _overrides: &[ProjectorOverride]) {}
    fn light_preview(&self) -> Option<LightPreviewReport> {
        None
    }
    fn play_cubemap_sources(&self) -> Option<(usize, usize)> {
        None
    }
    fn play_cubemap_targets(&self) -> Vec<([i32; 3], Option<hp_render::CubemapId>)> {
        Vec::new()
    }
    fn collision_triangles(&mut self, _path: &str) -> Option<Option<CollisionTriangles>> {
        Some(None)
    }
    fn model_thumbnail(&mut self, _ctx: &egui::Context, _path: &str) -> ThumbState {
        ThumbState::Missing
    }
    fn model_preview(
        &mut self,
        gfx: &mut Gfx,
        path: &str,
        camera: &PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId>;
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb>;
    fn entity_ray(&self, entity: &hp_vmf::Entity, ray: &hp_geom::Ray) -> hp_editor::EntityRay {
        let _ = (entity, ray);
        hp_editor::EntityRay::Bounds
    }
    fn material_tool(&self, name: &str) -> Option<hp_vmt::ToolKind> {
        crate::pick::guess_tool(name)
    }
    fn invalidate_material(&mut self, name: &str);
    fn import_images(&mut self, files: &[PathBuf], opts: &ImportOptions) -> Vec<Result<String, String>>;
    fn image_converter(&self) -> Option<ImageConverter> {
        None
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut Gfx>);
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut Gfx, opts: &SceneOptions);
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId;
    fn status_line(&self) -> Option<String>;
    fn pending_work(&self) -> usize {
        0
    }
    fn take_messages(&mut self) -> Vec<(crate::Severity, String)> {
        Vec::new()
    }
    fn particles(&mut self) -> Option<Arc<ParticleCatalog>> {
        None
    }
    fn park_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut Gfx) {
        let _ = (doc, gfx);
    }
    fn parked_scenes(&self) -> usize {
        0
    }
    fn parked_gpu_bytes(&self) -> u64 {
        0
    }
    fn set_parked_gpu_limit(&mut self, bytes: u64) {
        let _ = bytes;
    }
    fn release_gpu_memory(&mut self, gfx: &mut Gfx, what: crate::runtime::GpuRelease) -> crate::runtime::GpuReleased {
        let _ = (gfx, what);
        crate::runtime::GpuReleased::default()
    }
    fn apply_texture_limit(&mut self, gfx: &mut Gfx, size: u32) -> usize {
        let _ = (gfx, size);
        0
    }
    fn sprite_material_id(&mut self, name: &str, additive: bool) -> hp_render::MaterialId {
        let _ = additive;
        self.render_material_id(name)
    }
}

pub struct Gfx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub renderer: hp_render::Renderer,
    pub egui_renderer: Arc<egui::mutex::RwLock<egui_wgpu::Renderer>>,
    view_textures: HashMap<u32, (egui::TextureId, u64)>,
}

impl Gfx {
    pub fn new(render_state: &egui_wgpu::RenderState) -> Self {
        let renderer = hp_render::Renderer::new(&render_state.device, &render_state.queue);
        crate::native_window::install_texture_uploader(render_state);
        Self {
            device: render_state.device.clone(),
            queue: render_state.queue.clone(),
            renderer,
            egui_renderer: render_state.renderer.clone(),
            view_textures: HashMap::new(),
        }
    }

    pub fn view_egui_texture(&mut self, view_id: u32) -> Option<egui::TextureId> {
        let generation = self.renderer.view_texture_generation(view_id);
        let view = self.renderer.view_texture(view_id)?;
        let mut egui_renderer = self.egui_renderer.write();
        match self.view_textures.get_mut(&view_id) {
            Some((id, generation_seen)) if *generation_seen == generation => Some(*id),
            Some((id, generation_seen)) => {
                egui_renderer.update_egui_texture_from_wgpu_texture(&self.device, view, wgpu::FilterMode::Linear, *id);
                *generation_seen = generation;
                Some(*id)
            }
            None => {
                let id = egui_renderer.register_native_texture(&self.device, view, wgpu::FilterMode::Linear);
                self.view_textures.insert(view_id, (id, generation));
                Some(id)
            }
        }
    }

    pub fn register_texture(&self, view: &wgpu::TextureView) -> egui::TextureId {
        self.egui_renderer.write().register_native_texture(&self.device, view, wgpu::FilterMode::Linear)
    }

    pub fn update_texture(&self, id: egui::TextureId, view: &wgpu::TextureView) {
        self.egui_renderer.write().update_egui_texture_from_wgpu_texture(
            &self.device,
            view,
            wgpu::FilterMode::Linear,
            id,
        );
    }

    pub fn free_texture(&self, id: egui::TextureId) {
        self.egui_renderer.write().free_texture(&id);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Cameras {
    pub cam3d: Camera3d,
    pub top: Camera2d,
    pub front: Camera2d,
    pub side: Camera2d,
}

impl Cameras {
    pub fn cam2d(&self, kind: ViewKind) -> Camera2d {
        match kind {
            ViewKind::Front => self.front,
            ViewKind::Side => self.side,
            _ => self.top,
        }
    }

    pub fn cam2d_mut(&mut self, kind: ViewKind) -> &mut Camera2d {
        match kind {
            ViewKind::Front => &mut self.front,
            ViewKind::Side => &mut self.side,
            _ => &mut self.top,
        }
    }

    pub fn center_2d(&mut self, p: DVec3) {
        for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
            self.cam2d_mut(kind).center = kind.to_2d(p);
        }
    }

    pub fn frame_2d(&mut self, min: DVec3, max: DVec3, view_size: egui::Vec2) {
        let center = (min + max) * 0.5;
        for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
            let size = kind.to_2d(max) - kind.to_2d(min);
            let size = DVec2::new(size.x.abs().max(16.0), size.y.abs().max(16.0)) * 1.25;
            let zoom = (view_size.x as f64 / size.x).min(view_size.y as f64 / size.y);
            let cam = self.cam2d_mut(kind);
            cam.center = kind.to_2d(center);
            cam.zoom = zoom.clamp(Camera2d::MIN_ZOOM, Camera2d::MAX_ZOOM);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RightTab {
    #[default]
    Textures,
    Entity,
    Visgroups,
    Outliner,
    Models,
    Tool,
    Package(&'static str),
}

#[derive(Clone, Debug, Default)]
pub struct UiState {
    pub right_tab: RightTab,
    pub show_settings: bool,
    pub show_shortcuts: bool,
    pub show_about: bool,
    pub show_command_palette: bool,
    pub show_compile: bool,
    pub show_problems: bool,
    pub show_find_entities: bool,
    pub show_replace_textures: bool,
    pub show_map_properties: bool,
    pub show_entity_report: bool,
    pub show_map_info: bool,
    pub show_transform: bool,
    pub transform_random: bool,
    pub show_paste_special: bool,
    pub show_goto: bool,
    pub show_material_editor: bool,
    pub show_first_run: bool,
    pub show_grid_manager: bool,
    pub maximized_view: Option<usize>,
    pub hovered_view: Option<usize>,
    pub hovered_logical: Option<usize>,
    pub focused_view: Option<usize>,
    pub focus_texture_search: bool,
    pub quick_input: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameInfo {
    pub time: f64,
    pub dt: f32,
    pub fps: f32,
    pub index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickKind {
    Entity,
    Faces,
    Angles,
    Point,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickRequest {
    pub kind: PickKind,
    pub requester: String,
    pub result: Option<PickResult>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PickResult {
    Entity(hp_vmf::Id),
    Faces(Vec<(hp_vmf::Id, usize)>),
    Point(DVec3),
    Cancelled,
}

#[derive(Clone, Debug, Default)]
pub struct Pointfile {
    pub path: PathBuf,
    pub points: Vec<DVec3>,
    pub cursor: usize,
}

pub struct AppState {
    pub settings: Settings,
    pub settings_dirty: bool,
    pub keys: KeyBindings,
    pub doc: hp_editor::Document,
    pub assets: Box<dyn AssetsApi>,
    pub gfx: Option<Gfx>,
    pub tool: ToolId,
    pub requested_tool: Option<ToolId>,
    pub requested_actions: Vec<Action>,
    pub requested_open: Option<PathBuf>,
    pub ui: UiState,
    pub cameras: Cameras,
    pub current_material: String,
    pub current_entity_class: String,
    pub current_model: Option<String>,
    pub current_model_skin: u32,
    pub pointfile: Option<Pointfile>,
    pub console: Console,
    pub toasts: Toasts,
    pub hover_world: Option<DVec3>,
    pub frame: FrameInfo,
    pub pivot: Option<DVec3>,
    pub pick: Option<PickRequest>,
    pub sidecar: crate::sidecar::Sidecar,
    pub runtime: crate::runtime::Runtime,
    pub grid_cache: crate::grid::GridCache,
}

impl AppState {
    pub fn new(settings: Settings, assets: Box<dyn AssetsApi>) -> Self {
        let keys = KeyBindings::from_settings(&settings.keybindings);
        let current_material = settings.edit.default_material.clone();
        let current_entity_class = settings.edit.default_point_entity.clone();
        let mut doc = hp_editor::Document::new_empty();
        doc.options_mut().texture_lock = settings.edit.texture_lock;
        doc.options_mut().texture_scale_lock = settings.edit.texture_scale_lock;
        Self {
            settings,
            settings_dirty: false,
            keys,
            doc,
            assets,
            gfx: None,
            tool: ToolId::Select,
            requested_tool: None,
            requested_actions: Vec::new(),
            requested_open: None,
            ui: UiState::default(),
            cameras: Cameras::default(),
            current_material,
            current_entity_class,
            current_model: None,
            current_model_skin: 0,
            pointfile: None,
            console: Console::default(),
            toasts: Toasts::default(),
            hover_world: None,
            frame: FrameInfo::default(),
            pivot: None,
            pick: None,
            sidecar: crate::sidecar::Sidecar::in_memory(),
            runtime: crate::runtime::Runtime::default(),
            grid_cache: crate::grid::GridCache::default(),
        }
    }

    pub fn request(&mut self, action: Action) {
        if !self.requested_actions.contains(&action) {
            self.requested_actions.push(action);
        }
    }

    pub fn set_tool(&mut self, tool: ToolId) {
        self.requested_tool = Some(tool);
    }

    pub fn apply_selection_undo_setting(&mut self) {
        self.doc.set_record_selection(self.settings.edit.undo_restores_selection);
        self.doc.options_mut().new_brush_color = self.settings.edit.new_brush_color;
    }

    pub fn notify(&mut self, severity: Severity, text: impl Into<String>) {
        let text = text.into();
        self.console.push(severity, text.clone());
        self.toasts.push(severity, text);
    }

    pub fn log(&mut self, severity: Severity, text: impl Into<String>) {
        self.console.push(severity, text);
    }

    pub fn grid_size(&self) -> f64 {
        self.target_grid().frame.nominal()
    }

    pub fn snap_enabled(&self) -> bool {
        self.settings.edit.snap_to_grid
    }

    pub fn snap(&self, v: f64) -> f64 {
        let g = self.grid_size();
        let v = if self.snap_enabled() && g > 0.0 { (v / g).round() * g } else { v };
        v.clamp(-hp_geom::MAX_COORD, hp_geom::MAX_COORD)
    }

    pub fn snap_vec(&self, v: DVec3) -> DVec3 {
        let v = if self.snap_enabled() { self.target_grid().frame.snap_point(v) } else { v };
        v.clamp(DVec3::splat(-hp_geom::MAX_COORD), DVec3::splat(hp_geom::MAX_COORD))
    }

    pub fn select_mode(&self) -> hp_editor::SelectMode {
        match self.settings.edit.select_mode {
            SelectModeSetting::Groups => hp_editor::SelectMode::Groups,
            SelectModeSetting::Objects => hp_editor::SelectMode::Objects,
            SelectModeSetting::Solids => hp_editor::SelectMode::Solids,
        }
    }

    pub fn render_mode(&self) -> hp_render::RenderMode {
        match self.settings.view.render_mode {
            RenderModeSetting::Textured => hp_render::RenderMode::Textured,
            RenderModeSetting::Flat => hp_render::RenderMode::Flat,
            RenderModeSetting::Wireframe => hp_render::RenderMode::Wireframe,
            RenderModeSetting::TexturedWire => hp_render::RenderMode::TexturedWire,
            RenderModeSetting::Lit => hp_render::RenderMode::Lit,
            RenderModeSetting::Unshaded => hp_render::RenderMode::Unshaded,
            RenderModeSetting::SmoothingGroups => hp_render::RenderMode::SmoothingGroups,
            RenderModeSetting::Baked => hp_render::RenderMode::Lit,
            RenderModeSetting::LightmapGrid => hp_render::RenderMode::Flat,
        }
    }

    pub fn game_wireframe(&self) -> u8 {
        let player = &self.runtime.lab.player;
        if player.active { player.wireframe.min(3) } else { 0 }
    }

    pub fn scene_options(&self) -> SceneOptions {
        SceneOptions {
            show_models: self.settings.view.show_models,
            show_helpers: self.settings.view.show_helpers,
            show_tool_textures: self.settings.view.show_tool_textures
                && !(self.runtime.lab.player.active && self.runtime.lab.player.hide_tools),
            lighting_preview: matches!(
                self.settings.view.render_mode,
                RenderModeSetting::Lit | RenderModeSetting::Baked
            ),
            model_lods: self.settings.render.model_lods,
        }
    }

    pub fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        self.assets.entity_bounds(entity)
    }

    pub fn map_title(&self) -> String {
        if let Some(name) = self.doc.path().and_then(|p| p.file_name()) {
            return name.to_string_lossy().into_owned();
        }
        match self.doc.opened_from().and_then(|p| p.file_name()) {
            Some(name) => crate::tf!("app.damaged_title", name.to_string_lossy()),
            None => crate::t!("app.untitled").to_owned(),
        }
    }

    pub fn use_material(&mut self, name: &str) {
        self.current_material = name.to_ascii_lowercase();
        self.settings.add_recent_material(name);
        self.settings_dirty = true;
    }
}
