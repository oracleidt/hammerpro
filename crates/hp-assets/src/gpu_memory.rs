use hp_app::runtime::{GpuRelease, GpuReleased};
use hp_render::Renderer;

use crate::AssetManager;
use crate::texture::TexTarget;

pub(crate) const DEFAULT_PARKED_LIMIT: u64 = 3 << 30;
const MODEL_CPU_BUDGET: usize = 128 << 20;

impl AssetManager {
    pub(crate) fn trim_model_geometry(&mut self) {
        let frame = self.shared.current_frame();
        if !frame.is_multiple_of(60) {
            return;
        }
        self.trim_model_geometry_to(MODEL_CPU_BUDGET, frame);
    }

    fn trim_model_geometry_to(&mut self, budget: usize, frame: u64) {
        self.lods.data.retain(|_, (source, _)| source.strong_count() > 0);
        self.lods.requested.retain(|_, source| source.strong_count() > 0);
        let mut bytes: usize = self
            .models
            .map
            .values()
            .filter_map(|entry| match entry {
                crate::models::ModelEntry::Ready(model) => Some(model.geometry_bytes()),
                _ => None,
            })
            .sum();
        bytes += self.lods.data.values().map(|(_, lods)| lods.geometry_bytes()).sum::<usize>();
        if bytes <= budget {
            return;
        }
        let epoch = self.gc_epoch();
        let pinned: ahash::AHashSet<&str> = self
            .models
            .render
            .iter()
            .filter(|(_, model)| model.epoch >= epoch)
            .map(|((path, _), _)| path.as_str())
            .chain(self.models.variants.keys().map(|(path, _, _)| path.as_str()))
            .collect();
        let mut candidates: Vec<(u64, String)> = self
            .models
            .map
            .iter()
            .filter_map(|(key, entry)| {
                let crate::models::ModelEntry::Ready(model) = entry else { return None };
                let used = self.models.last_used.get(key).copied().unwrap_or(0);
                (std::sync::Arc::strong_count(model) == 1
                    && !pinned.contains(key.as_str())
                    && frame.saturating_sub(used) > 60)
                    .then(|| (used, key.clone()))
            })
            .collect();
        candidates.sort_unstable();
        for (_, key) in candidates {
            if bytes <= budget {
                break;
            }
            if let Some(entry) = self.models.map.get_mut(&key)
                && let crate::models::ModelEntry::Ready(model) = entry
            {
                bytes = bytes.saturating_sub(model.geometry_bytes());
                *entry = crate::models::ModelEntry::Cached(std::sync::Arc::clone(&model.info));
                self.memory_generation = self.memory_generation.wrapping_add(1);
                if let Some((_, lods)) = self.lods.data.remove(&key) {
                    bytes = bytes.saturating_sub(lods.geometry_bytes());
                }
            }
        }
        self.models.last_used.retain(|key, _| self.models.map.contains_key(key));
        self.lods.data.retain(|_, (source, _)| source.strong_count() > 0);
        self.lods.requested.retain(|_, source| source.strong_count() > 0);
    }

    pub(crate) fn parked_gpu_bytes_impl(&self) -> u64 {
        self.parked.iter().map(|p| p.render_bytes()).sum()
    }

    pub(crate) fn set_parked_limit_impl(&mut self, bytes: u64) {
        self.parked_limit = bytes;
        self.enforce_parked_limit();
    }

    pub(crate) fn enforce_parked_limit(&mut self) -> usize {
        let mut bytes = self.parked_gpu_bytes_impl();
        let mut dropped = 0;
        while !self.parked.is_empty() && (self.parked.len() > crate::scene::MAX_PARKED || bytes > self.parked_limit) {
            let old = self.parked.remove(0);
            bytes = bytes.saturating_sub(old.render_bytes());
            old.drop_in_background();
            dropped += 1;
        }
        dropped
    }

    pub(crate) fn drop_parked_scenes(&mut self) -> usize {
        let n = self.parked.len();
        for p in self.parked.drain(..) {
            p.drop_in_background();
        }
        n
    }

    pub(crate) fn release_gpu_memory_impl(&mut self, renderer: &mut Renderer, what: GpuRelease) -> GpuReleased {
        let mut out = GpuReleased { background_maps: self.drop_parked_scenes(), ..GpuReleased::default() };
        let (textures, models) = self.collect_garbage(renderer);
        out.textures = textures;
        out.models = models;
        if matches!(what, GpuRelease::Caches | GpuRelease::Textures(_)) {
            out.thumbnails = self.release_thumbnails();
        }
        if let GpuRelease::Textures(limit) = what {
            out.reloading = self.apply_texture_limit(renderer, limit);
        }
        out
    }

    fn release_thumbnails(&mut self) -> usize {
        let n = self.thumbs.ready_count() + self.model_thumbs_ready();
        self.thumbs.clear();
        self.shared.clear_thumbs();
        self.reset_model_thumbs();
        n
    }

    pub(crate) fn apply_texture_limit(&mut self, renderer: &mut Renderer, limit: u32) -> usize {
        let limit = limit.clamp(64, renderer.max_texture_size());
        let before = renderer.texture_limit();
        if limit == before {
            return 0;
        }
        renderer.set_texture_limit(limit);
        let threshold = before.min(limit);
        let names: Vec<String> = self
            .mats
            .textures
            .map
            .iter()
            .filter(|(_, e)| e.current.is_some_and(|(_, [w, h])| w.max(h) > threshold))
            .map(|(k, _)| k.clone())
            .collect();
        for name in &names {
            self.queue_texture(name, TexTarget::Main);
        }
        if !names.is_empty() {
            log::info!("textures limited to {limit} pixels (were {before}): {} textures are loaded again", names.len());
        }
        names.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ModelEntry, PreparedModel, PreparedPart, RenderModel};
    use std::sync::Arc;

    fn model() -> PreparedModel {
        PreparedModel {
            info: Arc::new(hp_app::state::ModelInfo::default()),
            parts: vec![PreparedPart {
                skin_ref: 0,
                body_part: 0,
                choice: 0,
                vertices: vec![hp_render::Vertex::default(); 16],
                indices: vec![0, 1, 2],
                tangents: Vec::new(),
                color_index: Vec::new(),
            }],
            variant_parts: Vec::new(),
            body_parts: Vec::new(),
            skin_materials: Vec::new(),
            bounds: ([0.0; 3], [0.0; 3]),
            helpers: Default::default(),
            lod_switch: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn geometry_cache_evicts_oldest_and_keeps_metadata_and_active_models() {
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        assets.mats.scene_epoch = 1;
        let bytes = model().geometry_bytes();
        for (key, used) in [("old", 1), ("recent", 100), ("visible", 1)] {
            assets.models.map.insert(key.into(), ModelEntry::Ready(Arc::new(model())));
            assets.models.last_used.insert(key.into(), used);
        }
        assets.models.render.insert(("visible".into(), 0), RenderModel { id: None, materials: Vec::new(), epoch: 1 });
        assets.trim_model_geometry_to(bytes * 2, 240);
        assert!(matches!(assets.models.map.get("old"), Some(ModelEntry::Cached(_))));
        assert!(matches!(assets.model_info_impl("old"), hp_app::state::ModelState::Ready(_)));
        assert!(matches!(assets.models.map.get("recent"), Some(ModelEntry::Ready(_))));
        assert!(matches!(assets.models.map.get("visible"), Some(ModelEntry::Ready(_))));
        assets.trim_model_geometry_to(0, 240);
        assert!(matches!(assets.models.map.get("recent"), Some(ModelEntry::Cached(_))));
        assert!(matches!(assets.models.map.get("visible"), Some(ModelEntry::Ready(_))));
    }
}
