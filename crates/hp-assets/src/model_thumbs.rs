use std::time::{Duration, Instant};

use ahash::AHashMap;
use glam::Vec3;
use hp_app::state::{Gfx, PreviewCamera, ThumbState};
use hp_render::{MaterialId, ModelId, ModelInstance, ModelPart, Overlay, RenderMode, ViewOptions, ViewParams};

use crate::AssetManager;
use crate::material::{AnaLook, MatKind, texture_keys};
use crate::models::PreparedModel;
use crate::preview::{BACKGROUND, Preview, orbit_camera};
use crate::util::model_key;

pub(crate) const THUMB_VIEW: u32 = 1;
const THUMB_OBJECT: u32 = 10;
const THUMB_PX: u32 = 160;
const MAX_THUMBS: usize = 768;
const MAX_JOBS: usize = 8;
const RENDERS_PER_FRAME: usize = 3;
const TEXTURE_WAIT: Duration = Duration::from_secs(4);
const STALE_FRAMES: u64 = 3;

enum Entry {
    Ready { texture: egui::TextureId, _gpu: wgpu::Texture, used: u64 },
    Missing,
}

struct Job {
    key: String,
    loaded_for_thumb: bool,
    model: Option<(ModelId, Vec<String>, (Vec3, Vec3))>,
    started: Instant,
}

#[derive(Default)]
pub(crate) struct ModelThumbs {
    generation: u64,
    map: AHashMap<String, Entry>,
    wanted: AHashMap<String, u64>,
    jobs: Vec<Job>,
    frame: u64,
    to_free: Vec<egui::TextureId>,
    max: usize,
}

impl AssetManager {
    pub(crate) fn model_thumbnail_impl(&mut self, ctx: &egui::Context, path: &str) -> ThumbState {
        let key = model_key(path);
        let t = &mut self.model_thumbs;
        match t.map.get_mut(&key) {
            Some(Entry::Ready { texture, used, .. }) => {
                *used = t.frame;
                ThumbState::Ready { texture: *texture, size: [THUMB_PX, THUMB_PX] }
            }
            Some(Entry::Missing) => ThumbState::Missing,
            None => {
                if self.fs.is_none() {
                    return ThumbState::Missing;
                }
                t.wanted.insert(key, t.frame);
                ctx.request_repaint_after(Duration::from_millis(60));
                ThumbState::Loading
            }
        }
    }

    pub(crate) fn process_model_thumbs(&mut self, gfx: &mut Gfx) {
        if self.model_thumbs.generation != self.generation {
            self.reset_model_thumbs();
            self.model_thumbs.generation = self.generation;
        }
        self.preview_sync_normal_maps();
        let t = &mut self.model_thumbs;
        t.frame += 1;
        for id in t.to_free.drain(..) {
            gfx.free_texture(id);
        }
        let frame = t.frame;
        t.wanted.retain(|_, f| frame.saturating_sub(*f) <= STALE_FRAMES);
        if t.wanted.is_empty() && t.jobs.is_empty() {
            return;
        }
        if self.preview.is_none() {
            self.preview = Some(Preview::new(gfx));
        }
        while self.model_thumbs.jobs.len() < MAX_JOBS {
            let t = &self.model_thumbs;
            let next = t
                .wanted
                .iter()
                .filter(|(k, _)| !t.map.contains_key(*k) && !t.jobs.iter().any(|j| &j.key == *k))
                .max_by_key(|(_, f)| **f)
                .map(|(k, _)| k.clone());
            let Some(key) = next else { break };
            let loaded_for_thumb = !self.models.map.contains_key(&key);
            self.model_thumbs.jobs.push(Job { key, loaded_for_thumb, model: None, started: Instant::now() });
        }
        let mut rendered = 0;
        let jobs = std::mem::take(&mut self.model_thumbs.jobs);
        let mut keep = Vec::with_capacity(jobs.len());
        for mut job in jobs {
            if job.model.is_none() {
                match self.model_data(&job.key) {
                    Some(data) => {
                        job.model = self.thumb_build(&data);
                        job.started = Instant::now();
                        if job.model.is_none() {
                            self.model_thumbs.map.insert(job.key.clone(), Entry::Missing);
                            self.drop_thumb_data(&job);
                            continue;
                        }
                    }
                    None => {
                        if matches!(self.models.map.get(&job.key), Some(crate::models::ModelEntry::Failed(_))) {
                            self.model_thumbs.map.insert(job.key.clone(), Entry::Missing);
                            self.drop_thumb_data(&job);
                        } else if self.model_thumbs.wanted.contains_key(&job.key) {
                            keep.push(job);
                        }
                        continue;
                    }
                }
            }
            let ready = job.model.as_ref().is_some_and(|(_, mats, _)| self.thumb_textures_ready(mats));
            if rendered < RENDERS_PER_FRAME && (ready || job.started.elapsed() > TEXTURE_WAIT) {
                self.preview_apply_dirty();
                self.thumb_render(gfx, &job);
                self.drop_thumb_data(&job);
                rendered += 1;
            } else if self.model_thumbs.wanted.contains_key(&job.key) || ready {
                keep.push(job);
            } else {
                self.thumb_destroy(&job);
                self.drop_thumb_data(&job);
            }
        }
        self.model_thumbs.jobs = keep;
        self.evict_model_thumbs();
    }

    pub(crate) fn set_model_thumb_capacity(&mut self, max: usize) {
        self.model_thumbs.max = max.max(1);
        self.evict_model_thumbs();
    }

    pub(crate) fn model_thumbs_ready(&self) -> usize {
        self.model_thumbs.map.values().filter(|e| matches!(e, Entry::Ready { .. })).count()
    }

    pub(crate) fn model_thumbs_busy(&self) -> bool {
        !self.model_thumbs.jobs.is_empty()
    }

    pub(crate) fn reset_model_thumbs(&mut self) {
        let jobs = std::mem::take(&mut self.model_thumbs.jobs);
        for job in &jobs {
            self.thumb_destroy(job);
        }
        let t = &mut self.model_thumbs;
        for (_, e) in t.map.drain() {
            if let Entry::Ready { texture, .. } = e {
                t.to_free.push(texture);
            }
        }
        t.wanted.clear();
    }

    fn evict_model_thumbs(&mut self) {
        let t = &mut self.model_thumbs;
        let ready = t.map.values().filter(|e| matches!(e, Entry::Ready { .. })).count();
        let max = if t.max == 0 { MAX_THUMBS } else { t.max };
        if ready <= max {
            return;
        }
        let mut by_age: Vec<(u64, String)> = t
            .map
            .iter()
            .filter_map(|(k, e)| match e {
                Entry::Ready { used, .. } => Some((*used, k.clone())),
                Entry::Missing => None,
            })
            .collect();
        by_age.sort_unstable();
        for (_, k) in by_age.into_iter().take(ready - max) {
            if let Some(Entry::Ready { texture, .. }) = t.map.remove(&k) {
                t.to_free.push(texture);
            }
        }
    }

    fn thumb_build(&mut self, data: &PreparedModel) -> Option<(ModelId, Vec<String>, (Vec3, Vec3))> {
        let mut parts = Vec::with_capacity(data.parts.len());
        let mut names = Vec::new();
        for part in &data.parts {
            let name = data.part_material(0, part.skin_ref).to_owned();
            let material: MaterialId = self.preview_material(&name);
            if !names.contains(&name) {
                names.push(name);
            }
            parts.push(ModelPart {
                material,
                vertices: part.vertices.clone(),
                indices: part.indices.clone(),
                tangents: part.tangents.clone(),
            });
        }
        if parts.is_empty() {
            return None;
        }
        let p = self.preview.as_mut()?;
        let id = p.renderer.create_model(&parts);
        let (lo, hi) = data.bounds;
        Some((id, names, (Vec3::from(lo), Vec3::from(hi))))
    }

    fn thumb_textures_ready(&self, names: &[String]) -> bool {
        let Some(p) = self.preview.as_ref() else { return true };
        names.iter().all(|n| match self.mats.ana_look(n) {
            AnaLook::Loading => false,
            AnaLook::Missing => true,
            AnaLook::Ready(a) => {
                texture_keys(MatKind::Model, a).iter().all(|t| p.textures.map.get(t).is_none_or(|e| !e.loading))
            }
        })
    }

    fn thumb_render(&mut self, gfx: &mut Gfx, job: &Job) {
        let Some((id, _, (min, max))) = job.model.as_ref() else { return };
        let Some(p) = self.preview.as_mut() else { return };
        p.hide_for_thumbnail();
        p.renderer.set_model_instances(&[ModelInstance {
            model: *id,
            transform: glam::Mat4::IDENTITY.to_cols_array_2d(),
            object: THUMB_OBJECT,
            tint: [1.0; 4],
            fade: [0.0; 2],
        }]);
        let opts = ViewOptions {
            normal_maps: hp_app::runtime::render::render_display().normal_maps,
            ..ViewOptions::default()
        };
        if p.renderer.view_options(THUMB_VIEW) != opts {
            p.renderer.set_view_options(THUMB_VIEW, opts);
        }
        let camera = PreviewCamera::default();
        let params = ViewParams {
            size: [THUMB_PX, THUMB_PX],
            kind: orbit_camera(*min, *max, &camera),
            mode: RenderMode::Textured,
            grid: None,
            background: BACKGROUND,
            selection_color: [1.0, 0.3, 0.3, 0.0],
            show_sprites: false,
            show_models: true,
            msaa: true,
            game_pixel_grid: false,
        };
        p.renderer.render_view(THUMB_VIEW, &params, &Overlay::default());
        let snapshot = p.renderer.snapshot_view(THUMB_VIEW);
        p.renderer.destroy_model(*id);
        p.restore_after_thumbnail();
        let entry = match snapshot {
            Some((tex, view)) => {
                let texture = gfx.register_texture(&view);
                Entry::Ready { texture, _gpu: tex, used: self.model_thumbs.frame }
            }
            None => Entry::Missing,
        };
        self.model_thumbs.map.insert(job.key.clone(), entry);
    }

    fn thumb_destroy(&mut self, job: &Job) {
        if let Some((id, _, _)) = &job.model
            && let Some(p) = self.preview.as_mut()
        {
            p.renderer.destroy_model(*id);
        }
    }

    fn drop_thumb_data(&mut self, job: &Job) {
        if !job.loaded_for_thumb {
            return;
        }
        let in_scene = self.models.render.keys().any(|(p, _)| p == &job.key);
        let in_preview = self.preview.as_ref().and_then(Preview::model_path) == Some(job.key.as_str());
        if !in_scene && !in_preview {
            self.models.map.remove(&job.key);
        }
    }
}
