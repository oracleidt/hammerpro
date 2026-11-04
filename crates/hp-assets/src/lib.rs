mod bump;
mod cubemaps;
mod decals;
mod detail;
mod entities;
mod env_scan;
mod envmap;
mod fog_volumes;
mod gpu_memory;
mod import;
mod jobs;
mod lights;
mod lods;
mod material;
mod meshes;
mod model_thumbs;
mod models;
mod mount;
pub mod parallax;
mod particles;
mod pick;
mod player;
mod playworld;
mod preview;
mod producer;
mod projectors;
mod queue;
mod registry;
mod scene;
mod shadow_hulls;
mod texture;
mod thumb_cache;
mod thumbs;
mod util;
mod watcher;
mod water;

pub fn entity_render_object(id: hp_vmf::Id) -> u32 {
    entities::entity_object(id)
}

mod helpers;
mod instances;
mod render;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use ahash::{AHashMap, AHashSet};
use crossbeam_channel::Receiver;
use hp_app::settings::Settings;
use hp_app::state::{
    AssetStatus, Gfx, ImportOptions, MaterialInfo, ModelState, PreviewCamera, SceneOptions, ThumbState,
};
use hp_app::{t, tf};

use crate::jobs::JobResult;
use crate::queue::Shared;

pub const TR: hp_app::i18n::TrTable = &[
    ("assets.not_configured", "Garry's Mod folder not set", "Dossier de Garry's Mod non défini"),
    ("assets.mount_failed", "Game content failed to load: {0}", "Échec du chargement du contenu du jeu : {0}"),
    ("assets.loading_textures", "Loading textures ({0} left)", "Chargement des textures ({0} restantes)"),
    (
        "assets.loading_late",
        "Loading the workshop and downloaded content ({0} addons)...",
        "Chargement du workshop et du contenu téléchargé ({0} addons)...",
    ),
    (
        "assets.late_mounted",
        "{0} workshop and downloaded addons added",
        "{0} addons du workshop et téléchargés ajoutés",
    ),
    ("assets.no_game", "Game content is not loaded", "Le contenu du jeu n'est pas chargé"),
    ("assets.model_failed", "Cannot load the model: {0}", "Impossible de charger le modèle : {0}"),
    ("assets.import_bad_name", "Invalid file name: {0}", "Nom de fichier invalide : {0}"),
    ("assets.import_exists", "Already exists: {0}", "Existe déjà : {0}"),
    ("assets.import_read_failed", "Cannot read {0}: {1}", "Impossible de lire {0} : {1}"),
    ("assets.import_decode_failed", "Cannot decode {0}: {1}", "Impossible de décoder {0} : {1}"),
    ("assets.import_encode_failed", "Cannot convert {0}: {1}", "Impossible de convertir {0} : {1}"),
    ("assets.import_write_failed", "Cannot write {0}: {1}", "Impossible d'écrire {0} : {1}"),
    ("assets.import_folder_failed", "Cannot create the folder {0}: {1}", "Impossible de créer le dossier {0} : {1}"),
    ("assets.texture_broken", "Texture {0} cannot be loaded: {1}", "Impossible de charger la texture {0} : {1}"),
    ("assets.fgd_note", "Entity definitions: {0}", "Définitions d'entités : {0}"),
    (
        "assets.fgd_more",
        "Entity definitions: {0} more problems (see the log file)",
        "Définitions d'entités : {0} problèmes de plus (voir le fichier journal)",
    ),
    (
        "assets.mounted_sources",
        "Game content, highest priority first: {0}",
        "Contenu du jeu, du plus prioritaire au moins prioritaire : {0}",
    ),
    ("assets.workshop_items", "Workshop ({0} items)", "Workshop ({0} éléments)"),
    ("assets.reloaded", "Reloaded from disk: {0}", "Rechargé depuis le disque : {0}"),
    ("assets.more_sources", "{0} more", "{0} de plus"),
];

pub fn register_translations() {
    hp_app::i18n::register(TR);
}

pub(crate) struct Upload {
    pub name: String,
    pub target: texture::TexTarget,
    pub epoch: u64,
    pub data: Option<texture::PreparedTexture>,
    pub _held: queue::Held,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetStats {
    pub render_materials: usize,
    pub materials_loading: usize,
    pub textures_ready: usize,
    pub textures_loading: usize,
    pub textures_missing: usize,
    pub models_ready: usize,
    pub models_loading: usize,
    pub models_failed: usize,
    pub thumbnails: usize,
    pub queued_jobs: usize,
    pub pending_uploads: usize,
}

pub struct AssetManager {
    pub(crate) shared: Arc<Shared>,
    pub(crate) rx: Receiver<JobResult>,
    _workers: Vec<JoinHandle<()>>,
    pub(crate) status: AssetStatus,
    pub(crate) root: Option<PathBuf>,
    pub(crate) fs: Option<hp_fs::GameFs>,
    pub(crate) fgd: Option<Arc<hp_fgd::ResolvedFgd>>,
    pub(crate) material_list: Arc<Vec<hp_fs::AssetEntry>>,
    pub(crate) model_list: Arc<Vec<hp_fs::AssetEntry>>,
    pub(crate) sound_list: Arc<Vec<hp_fs::AssetEntry>>,
    pub(crate) labels: AHashMap<u32, String>,
    pub(crate) mount_ticket: u64,
    pub(crate) late_sources: usize,
    pub(crate) mount_progress: Arc<hp_fs::MountProgress>,
    pub(crate) late_progress: Arc<hp_fs::MountProgress>,
    pub(crate) fgd_ticket: u64,
    pub(crate) fgd_done: u64,
    pub(crate) fgd_report: hp_app::FgdReport,
    pub(crate) mount_gen: u64,
    pub(crate) fs_gen_seen: u64,
    pub(crate) lists_pending: bool,
    pub(crate) generation: u64,
    pub(crate) mats: registry::Materials,
    pub(crate) thumbs: thumbs::Thumbs,
    pub(crate) models: models::Models,
    pub(crate) scene: scene::SceneSync,
    pub(crate) parked: Vec<scene::ParkedSync>,
    pub(crate) preview: Option<preview::Preview>,
    pub(crate) model_thumbs: model_thumbs::ModelThumbs,
    pub(crate) player: player::PlayerModel,
    pub(crate) play_world: playworld::PlayWorld,
    pub(crate) play_transforms: AHashMap<hp_vmf::Id, glam::DMat4>,
    pub(crate) play_dirty: Vec<hp_vmf::Id>,
    pub(crate) game_hidden: AHashSet<u32>,
    pub(crate) game_hidden_dirty: bool,
    pub(crate) play_hidden: AHashSet<u32>,
    pub(crate) play_hidden_dirty: bool,
    pub(crate) uploads: VecDeque<Upload>,
    pub(crate) fresh: AHashSet<String>,
    pub(crate) messages: Vec<(hp_app::Severity, String)>,
    pub(crate) watcher: Option<watcher::Watcher>,
    pub(crate) particles: particles::ParticleIndex,
    pub(crate) lods: lods::Lods,
    pub(crate) editor_cubes: cubemaps::EditorCubes,
    pub(crate) pick_meshes: pick::PickMeshes,
    pub(crate) parked_limit: u64,
}

impl AssetManager {
    pub fn new(settings: &Settings) -> AssetManager {
        let (tx, rx) = crossbeam_channel::unbounded();
        let shared = Arc::new(Shared::new(tx));
        let workers = queue::spawn_workers(&shared, queue::worker_count());
        let mut m = AssetManager {
            shared,
            rx,
            _workers: workers,
            status: AssetStatus::Mounting,
            root: None,
            fs: None,
            fgd: None,
            material_list: Arc::new(Vec::new()),
            model_list: Arc::new(Vec::new()),
            sound_list: Arc::new(Vec::new()),
            labels: AHashMap::new(),
            mount_ticket: 0,
            late_sources: 0,
            mount_progress: Arc::default(),
            late_progress: Arc::default(),
            fgd_ticket: 0,
            fgd_done: 0,
            fgd_report: hp_app::FgdReport::default(),
            mount_gen: 0,
            fs_gen_seen: 0,
            lists_pending: false,
            generation: 1,
            mats: registry::Materials::default(),
            thumbs: thumbs::Thumbs::default(),
            models: models::Models::default(),
            scene: scene::SceneSync::default(),
            parked: Vec::new(),
            preview: None,
            model_thumbs: model_thumbs::ModelThumbs::default(),
            player: player::PlayerModel::default(),
            play_transforms: AHashMap::new(),
            play_dirty: Vec::new(),
            game_hidden: AHashSet::new(),
            game_hidden_dirty: false,
            play_hidden: AHashSet::new(),
            play_hidden_dirty: false,
            play_world: playworld::PlayWorld::default(),
            uploads: VecDeque::new(),
            fresh: AHashSet::new(),
            messages: Vec::new(),
            watcher: None,
            particles: particles::ParticleIndex::default(),
            lods: lods::Lods::default(),
            editor_cubes: cubemaps::EditorCubes::default(),
            pick_meshes: pick::PickMeshes::default(),
            parked_limit: gpu_memory::DEFAULT_PARKED_LIMIT,
        };
        m.start_mount(settings);
        m.start_fgd(settings);
        m
    }

    pub fn set_thumbnail_cache(&mut self, dir: Option<PathBuf>) {
        *self.shared.thumb_cache.write() = dir.map(|d| Arc::new(thumb_cache::ThumbDiskCache::new(d)));
    }

    pub fn game_root(&self) -> Option<&std::path::Path> {
        self.root.as_deref()
    }

    pub fn pending_work(&self) -> usize {
        self.shared.queued()
            + self.uploads.len()
            + self.mats.textures.loading_count()
            + self.mats.analyses.values().filter(|a| a.loading).count()
            + usize::from(self.scene.instances_streamed)
            + self.scene.realize_later.len()
    }

    pub fn stats(&self) -> AssetStats {
        let tex = &self.mats.textures.map;
        AssetStats {
            render_materials: self.mats.records.len(),
            materials_loading: self.mats.analyses.values().filter(|a| a.loading).count(),
            textures_ready: tex.values().filter(|e| e.current.is_some()).count(),
            textures_loading: tex.values().filter(|e| e.loading).count(),
            textures_missing: tex.values().filter(|e| e.missing && !e.loading).count(),
            models_ready: self.models.map.values().filter(|m| matches!(m, models::ModelEntry::Ready(_))).count(),
            models_loading: self.models.map.values().filter(|m| matches!(m, models::ModelEntry::Loading)).count()
                + self.scene.realize_later.len(),
            models_failed: self.models.map.values().filter(|m| matches!(m, models::ModelEntry::Failed(_))).count(),
            thumbnails: self.thumbs.map.values().filter(|t| matches!(t, thumbs::ThumbEntry::Ready { .. })).count(),
            queued_jobs: self.shared.queued(),
            pending_uploads: self.uploads.len(),
        }
    }

    pub fn preview_texture_counts(&self) -> (usize, usize, usize) {
        let Some(p) = self.preview.as_ref() else { return (0, 0, 0) };
        let map = &p.textures.map;
        let ready = || map.iter().filter(|(_, e)| e.current.is_some());
        (
            ready().count(),
            map.values().filter(|e| e.loading).count(),
            ready().filter(|(k, _)| bump::parse_normal_key(k).is_some()).count(),
        )
    }

    pub fn read_preview_rgba8(&self) -> Option<(u32, u32, Vec<u8>)> {
        self.preview.as_ref()?.renderer.read_view_rgba8(0).ok()
    }

    pub fn wait_until_mounted(&mut self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            self.poll_results();
            let fgd_done = self.fgd_ticket == self.fgd_done;
            if self.status != AssetStatus::Mounting && fgd_done {
                return true;
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            if let Ok(r) = self.rx.recv_timeout(left.min(std::time::Duration::from_millis(50))) {
                self.handle_result(r);
            }
        }
    }
}

impl Drop for AssetManager {
    fn drop(&mut self) {
        self.shared.stop();
    }
}

impl hp_app::AssetsApi for AssetManager {
    fn status(&self) -> AssetStatus {
        self.status.clone()
    }

    fn fs(&self) -> Option<&hp_fs::GameFs> {
        self.fs.as_ref()
    }

    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        self.fgd.as_deref()
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn late_mount(&self) -> usize {
        self.late_sources
    }

    fn mount_progress(&self) -> Option<hp_fs::MountSnapshot> {
        if self.status == AssetStatus::Mounting {
            Some(self.mount_progress.snapshot())
        } else if self.late_sources > 0 {
            Some(self.late_progress.snapshot())
        } else {
            None
        }
    }

    fn remount(&mut self, settings: &Settings) {
        self.start_mount(settings);
        self.start_fgd(settings);
    }

    fn reload_fgd(&mut self, settings: &Settings) {
        self.start_fgd(settings);
    }

    fn fgd_report(&self) -> hp_app::FgdReport {
        hp_app::FgdReport { loading: self.fgd_ticket != self.fgd_done, ..self.fgd_report.clone() }
    }

    fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        Arc::clone(&self.material_list)
    }

    fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        Arc::clone(&self.model_list)
    }

    fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
        Arc::clone(&self.sound_list)
    }

    fn source_label(&self, source: u32) -> String {
        self.labels.get(&source).cloned().unwrap_or_else(|| format!("#{source}"))
    }

    fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
        self.material_info_impl(name)
    }

    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> ThumbState {
        self.thumbnail_impl(ctx, name)
    }

    fn model_info(&mut self, path: &str) -> ModelState {
        self.model_info_impl(path)
    }

    fn model_thumbnail(&mut self, ctx: &egui::Context, path: &str) -> ThumbState {
        self.model_thumbnail_impl(ctx, path)
    }

    fn set_player_model(&mut self, pose: Option<hp_app::state::PlayerModelPose>) {
        self.set_player_model_impl(pose);
    }

    fn collision_triangles(&mut self, path: &str) -> Option<Option<hp_app::state::CollisionTriangles>> {
        self.collision_triangles_impl(path)
    }

    fn set_play_world(
        &mut self,
        world: Option<Arc<hp_editor::limits::world::BspWorld>>,
        visible: Option<Arc<Vec<bool>>>,
    ) {
        self.set_play_world_impl(world, visible);
    }

    fn set_baked_prop_lighting(
        &mut self,
        props: Option<Arc<hp_editor::limits::world_light::PropLighting>>,
        region: Option<[[f64; 3]; 2]>,
    ) {
        self.set_baked_prop_lighting_impl(props, region);
    }

    fn set_light_styles(&mut self, values: &[f32]) {
        self.set_light_styles_impl(values);
    }

    fn set_projector_overrides(&mut self, overrides: &[hp_app::state::ProjectorOverride]) {
        if self.scene.projector_overrides != overrides {
            self.scene.projector_overrides = overrides.to_vec();
            self.scene.lights_dirty = true;
        }
    }

    fn light_preview(&self) -> Option<hp_app::state::LightPreviewReport> {
        self.scene.opts.is_some_and(|o| o.lighting_preview).then_some(self.scene.light_report)
    }

    fn play_cubemap_sources(&self) -> Option<(usize, usize)> {
        self.play_world_shown().then(|| self.play_cubemap_sources()).flatten()
    }

    fn play_cubemap_targets(&self) -> Vec<([i32; 3], Option<hp_render::CubemapId>)> {
        if !self.play_world_shown() {
            return Vec::new();
        }
        self.play_cubemap_targets()
    }

    fn set_play_hidden(&mut self, objs: &[hp_editor::ObjRef]) {
        let ids: AHashSet<u32> = objs
            .iter()
            .flat_map(|o| match o {
                hp_editor::ObjRef::Solid(id) => [Some(*id), None],
                hp_editor::ObjRef::Entity(id) => [Some(entities::entity_object(*id)), entities::decal_object(*id)],
            })
            .flatten()
            .collect();
        if ids != self.play_hidden {
            self.play_hidden = ids;
            self.play_hidden_dirty = true;
            self.scene.instances_dirty = true;
        }
    }

    fn set_model_lods(&mut self, on: bool) {
        self.set_model_lods_impl(on);
    }

    fn set_game_hidden(&mut self, objs: &[hp_editor::ObjRef]) {
        let ids: AHashSet<u32> = objs
            .iter()
            .map(|o| match o {
                hp_editor::ObjRef::Solid(id) => *id,
                hp_editor::ObjRef::Entity(id) => entities::entity_object(*id),
            })
            .collect();
        if ids != self.game_hidden {
            self.game_hidden = ids;
            self.game_hidden_dirty = true;
        }
    }

    fn set_play_transforms(&mut self, transforms: &[(hp_vmf::Id, glam::DMat4)]) {
        let next: AHashMap<hp_vmf::Id, glam::DMat4> = transforms.iter().copied().collect();
        for (id, m) in &next {
            if self.play_transforms.get(id) != Some(m) {
                self.play_dirty.push(*id);
            }
        }
        for id in self.play_transforms.keys() {
            if !next.contains_key(id) {
                self.play_dirty.push(*id);
            }
        }
        self.play_transforms = next;
    }

    fn model_preview(
        &mut self,
        gfx: &mut Gfx,
        path: &str,
        camera: &PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.model_preview_impl(gfx, path, camera, size_px)
    }

    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        self.entity_bounds_impl(entity)
    }

    fn entity_ray(&self, entity: &hp_vmf::Entity, ray: &hp_geom::Ray) -> hp_editor::EntityRay {
        self.entity_ray_impl(entity, ray)
    }

    fn material_tool(&self, name: &str) -> Option<hp_vmt::ToolKind> {
        self.material_tool_impl(name)
    }

    fn invalidate_material(&mut self, name: &str) {
        self.invalidate_material_impl(name);
    }

    fn import_images(&mut self, files: &[PathBuf], opts: &ImportOptions) -> Vec<Result<String, String>> {
        self.import_images_impl(files, opts)
    }

    fn image_converter(&self) -> Option<hp_app::state::ImageConverter> {
        Some(import::import_files)
    }

    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut Gfx>) {
        self.begin_frame_impl(ctx, gfx);
    }

    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut Gfx, opts: &SceneOptions) {
        self.sync_scene_impl(doc, &mut gfx.renderer, opts);
    }

    fn park_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut Gfx) {
        self.park_scene_impl(doc, &mut gfx.renderer);
    }

    fn parked_scenes(&self) -> usize {
        self.parked.len()
    }

    fn parked_gpu_bytes(&self) -> u64 {
        self.parked_gpu_bytes_impl()
    }

    fn set_parked_gpu_limit(&mut self, bytes: u64) {
        self.set_parked_limit_impl(bytes);
    }

    fn release_gpu_memory(&mut self, gfx: &mut Gfx, what: hp_app::runtime::GpuRelease) -> hp_app::runtime::GpuReleased {
        self.release_gpu_memory_impl(&mut gfx.renderer, what)
    }

    fn apply_texture_limit(&mut self, gfx: &mut Gfx, size: u32) -> usize {
        self.apply_texture_limit(&mut gfx.renderer, size)
    }

    fn sprite_material_id(&mut self, name: &str, additive: bool) -> hp_render::MaterialId {
        self.sprite_material_impl(name, additive)
    }

    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.world_material(name)
    }

    fn pending_work(&self) -> usize {
        AssetManager::pending_work(self)
    }

    fn take_messages(&mut self) -> Vec<(hp_app::Severity, String)> {
        std::mem::take(&mut self.messages)
    }

    fn particles(&mut self) -> Option<Arc<hp_app::state::ParticleCatalog>> {
        self.particles.get(self.fs.as_ref(), self.generation)
    }

    fn status_line(&self) -> Option<String> {
        Some(match &self.status {
            AssetStatus::Mounting => t!("status.loading_assets").to_owned(),
            AssetStatus::NotConfigured => t!("assets.not_configured").to_owned(),
            AssetStatus::Failed(msg) => tf!("assets.mount_failed", msg),
            AssetStatus::Ready => {
                let loading = self.mats.textures.loading_count();
                if self.late_sources > 0 {
                    tf!("assets.loading_late", self.late_sources)
                } else if loading > 0 {
                    tf!("assets.loading_textures", loading)
                } else {
                    tf!("status.assets_ready", self.material_list.len(), self.model_list.len())
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_are_clean() {
        let problems = hp_app::i18n::lint_table(TR);
        assert!(problems.is_empty(), "{problems:#?}");
        for (k, _, _) in TR {
            assert!(k.starts_with("assets."), "{k}");
            assert!(!hp_app::strings::TR.iter().any(|(c, _, _)| c == k), "{k} collides with a core key");
        }
    }

    #[test]
    fn source_has_no_long_dashes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = vec![dir.join("Cargo.toml")];
        for sub in ["src", "tests", "tests/common"] {
            if let Ok(rd) = std::fs::read_dir(dir.join(sub)) {
                files.extend(rd.flatten().map(|e| e.path()));
            }
        }
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap_or_default();
            assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'), "{}", f.display());
        }
    }
}
