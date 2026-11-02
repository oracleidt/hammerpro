use hp_app::runtime::{GpuRelease, GpuReleased};
use hp_render::Renderer;

use crate::AssetManager;
use crate::texture::TexTarget;

pub(crate) const DEFAULT_PARKED_LIMIT: u64 = 3 << 30;

impl AssetManager {
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
