use hp_app::runtime::{GpuRelease, GpuReleased};
use hp_render::Renderer;

use crate::AssetManager;
use crate::texture::TexTarget;

pub(crate) const DEFAULT_PARKED_LIMIT: u64 = 3 << 30;
const MODEL_CPU_BUDGET: usize = 128 << 20;
pub(crate) const RESIDENT_IDLE_FRAMES: u64 = 120;
pub(crate) const RESIDENT_IDLE: std::time::Duration = std::time::Duration::from_secs(5);

impl AssetManager {
    pub(crate) fn trim_model_geometry(&mut self) {
        let frame = self.shared.current_frame();
        if frame.is_multiple_of(60) {
            self.trim_model_geometry_to(MODEL_CPU_BUDGET, frame);
        }
        self.trim_resident_models(frame);
    }

    fn models_settled(&self) -> bool {
        self.scene.initialized
            && self.scene.prep.is_none()
            && self.scene.upload_pending.is_empty()
            && self.scene.realize_later.is_empty()
            && self.lods.ready.is_empty()
            && self.shared.queued() == 0
            && !self.models.map.values().any(|m| matches!(m, crate::models::ModelEntry::Loading))
    }

    pub(crate) fn trim_resident_models(&mut self, frame: u64) -> usize {
        use crate::models::{ModelEntry, PickGeometry};
        use std::sync::Arc;
        if !self.models_settled() {
            self.models.settled = None;
            self.models.trim_waiting = false;
            return 0;
        }
        let now = std::time::Instant::now();
        let first = self.models.settled.is_none();
        let (since_frame, since) = *self.models.settled.get_or_insert((frame, now));
        let aged = now.duration_since(since) >= RESIDENT_IDLE;
        if !first && !aged && !frame.is_multiple_of(60) {
            if self.models.trim_waiting
                && let Some(ctx) = self.shared.ctx()
            {
                ctx.request_repaint_after(RESIDENT_IDLE.saturating_sub(now.duration_since(since)));
            }
            return 0;
        }
        let mut waiting = false;
        let collision =
            hp_app::runtime::render::render_display().prop_shadows == hp_app::settings::StaticPropShadows::Collision;
        let mut ids: ahash::AHashMap<&str, Vec<hp_render::ModelId>> = ahash::AHashMap::new();
        for ((path, _), m) in &self.models.render {
            ids.entry(path.as_str()).or_default().extend(m.id);
        }
        for ((key, _, _), (m, _)) in &self.models.variants {
            ids.entry(key.as_str()).or_default().extend(m.id);
        }
        let lods_on = self.lods.on;
        let picked: Vec<(String, bool)> = self
            .models
            .map
            .iter()
            .filter_map(|(key, entry)| {
                let ModelEntry::Ready(model) = entry else { return None };
                let used = self.models.last_used.get(key).copied().unwrap_or(0);
                if Arc::strong_count(model) != 1 {
                    return None;
                }
                let own = ids.get(key.as_str());
                let made_from = Arc::downgrade(model);
                let lods_done = !lods_on
                    || model.lod_switch.len() <= 1
                    || self.lods.data.get(key).is_some_and(|(w, _)| w.ptr_eq(&made_from))
                    || self.lods.requested.get(key).is_some_and(|w| w.ptr_eq(&made_from));
                let hulls_done = !collision || self.scene.shadow_hulls.settled(key, own.map_or(&[][..], Vec::as_slice));
                let idle = frame.saturating_sub(used) >= RESIDENT_IDLE_FRAMES || (aged && used <= since_frame);
                if !lods_done || !hulls_done || !idle {
                    waiting |= used <= since_frame;
                    return None;
                }
                Some((key.clone(), own.is_some()))
            })
            .collect();
        let count = picked.len();
        for (key, used_by_map) in picked {
            if let Some(ModelEntry::Ready(model)) = self.models.map.get(&key) {
                let info = Arc::clone(&model.info);
                let pick = used_by_map.then(|| PickGeometry::of(model));
                self.models.map.insert(key.clone(), ModelEntry::Cached(info));
                match pick {
                    Some(geo) => {
                        self.models.pick.insert(key, Arc::new(geo));
                    }
                    None => {
                        self.models.pick.remove(&key);
                    }
                }
            }
        }
        if count > 0 {
            self.memory_generation = self.memory_generation.wrapping_add(1);
            self.lods.data.retain(|_, (source, _)| source.strong_count() > 0);
            self.lods.requested.retain(|_, source| source.strong_count() > 0);
        }
        self.models.trim_waiting = waiting && !aged;
        if aged {
            self.models.settled = Some((frame, now));
        } else if waiting && let Some(ctx) = self.shared.ctx() {
            ctx.request_repaint_after(RESIDENT_IDLE.saturating_sub(now.duration_since(since)));
        }
        count
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
    fn idle_models_of_the_map_keep_only_pick_geometry_until_needed() {
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        assets.scene.initialized = true;
        assets.mats.scene_epoch = 1;
        let mut lod_model = model();
        lod_model.lod_switch = vec![0.0, 40.0];
        for (key, data, used) in [("busy", model(), 200), ("idle", model(), 1), ("lods", lod_model, 1)] {
            assets.models.map.insert(key.into(), ModelEntry::Ready(Arc::new(data)));
            assets.models.last_used.insert(key.into(), used);
            assets.models.render.insert((key.into(), 0), RenderModel { id: None, materials: Vec::new(), epoch: 1 });
        }
        assets.models.map.insert("other".into(), ModelEntry::Ready(Arc::new(model())));
        assets.lods.on = true;
        let frame = 1 + RESIDENT_IDLE_FRAMES + 50;
        assert_eq!(assets.trim_resident_models(frame), 2);
        assert!(matches!(assets.models.map.get("idle"), Some(ModelEntry::Cached(_))));
        assert!(assets.models.pick.contains_key("idle"), "used by the map: precise picking is kept");
        assert!(matches!(assets.models.map.get("other"), Some(ModelEntry::Cached(_))));
        assert!(!assets.models.pick.contains_key("other"), "not used by the map: nothing kept");
        assert!(matches!(assets.models.map.get("busy"), Some(ModelEntry::Ready(_))), "used a moment ago");
        assert!(matches!(assets.models.map.get("lods"), Some(ModelEntry::Ready(_))), "levels of detail not built yet");
        let geo = Arc::clone(&assets.models.pick["idle"]);
        assert_eq!(geo.parts.len(), 1);
        assert_eq!(geo.parts[0].positions.len(), 16);
        assets.models.map.insert("busy".into(), ModelEntry::Loading);
        assert_eq!(assets.trim_resident_models(frame + 1000), 0, "nothing is trimmed while models load");
        assert!(assets.models.settled.is_none());
    }

    #[test]
    fn a_still_editor_trims_idle_models_after_a_few_seconds_without_new_frames() {
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        assets.scene.initialized = true;
        assets.mats.scene_epoch = 1;
        for (key, used) in [("placed", 10), ("later", 12)] {
            assets.models.map.insert(key.into(), ModelEntry::Ready(Arc::new(model())));
            assets.models.last_used.insert(key.into(), used);
            assets.models.render.insert((key.into(), 0), RenderModel { id: None, materials: Vec::new(), epoch: 1 });
        }
        assert_eq!(assets.trim_resident_models(11), 0, "a few frames after loading: kept");
        let (since_frame, _) = assets.models.settled.expect("the settle time is noted");
        assert_eq!(since_frame, 11);
        assert!(assets.models.trim_waiting, "a repaint stays requested until the models can be trimmed");
        assert_eq!(assets.trim_resident_models(12), 0, "not idle long enough yet");
        assets.models.settled = Some((11, std::time::Instant::now() - RESIDENT_IDLE));
        assert_eq!(assets.trim_resident_models(13), 1);
        assert!(matches!(assets.models.map.get("placed"), Some(ModelEntry::Cached(_))));
        assert!(assets.models.pick.contains_key("placed"));
        assert!(matches!(assets.models.map.get("later"), Some(ModelEntry::Ready(_))), "used after things settled");
        assert!(!assets.models.trim_waiting);
        let (frame, at) = assets.models.settled.unwrap();
        assert_eq!(frame, 13, "the next check waits a full delay again");
        assert!(at.elapsed() < RESIDENT_IDLE);
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
