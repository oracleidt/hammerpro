use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ahash::AHashMap;
use hp_app::state::{AssetStatus, ThumbState};

use crate::AssetManager;
use crate::jobs::{Job, ThumbPixels};
use crate::queue::Lane;
use crate::util::{is_normalized_material, material_key};

pub(crate) const THUMB_CAPACITY: usize = 3000;
pub(crate) const THUMBS_PER_FRAME: usize = 48;

pub(crate) enum ThumbEntry {
    Pending(Arc<AtomicU64>),
    Ready { handle: egui::TextureHandle, size: [u32; 2], last_used: u64 },
    Missing,
}

#[derive(Default)]
pub(crate) struct Thumbs {
    pub map: AHashMap<String, ThumbEntry>,
    ready: usize,
    decoded: VecDeque<(String, ThumbPixels)>,
    budget: (u64, usize),
    capacity: usize,
}

impl Thumbs {
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.decoded.clear();
        self.ready = 0;
    }

    pub(crate) fn remove(&mut self, name: &str) {
        if let Some(ThumbEntry::Ready { .. }) = self.map.remove(name) {
            self.ready = self.ready.saturating_sub(1);
        }
    }

    pub(crate) fn forget_pending(&mut self, name: &str) {
        if matches!(self.map.get(name), Some(ThumbEntry::Pending(_))) {
            self.map.remove(name);
        }
    }

    pub(crate) fn ready_count(&self) -> usize {
        self.ready
    }

    pub(crate) fn capacity(&self) -> usize {
        if self.capacity == 0 { THUMB_CAPACITY } else { self.capacity }
    }

    pub(crate) fn set_capacity(&mut self, capacity: usize, frame: u64) {
        self.capacity = capacity.max(1);
        self.evict(frame.saturating_add(1));
    }

    fn insert_ready(&mut self, name: String, handle: egui::TextureHandle, size: [u32; 2], frame: u64) {
        let old = self.map.insert(name, ThumbEntry::Ready { handle, size, last_used: frame });
        if !matches!(old, Some(ThumbEntry::Ready { .. })) {
            self.ready += 1;
        }
        self.evict(frame);
    }

    fn evict(&mut self, frame: u64) {
        let capacity = self.capacity();
        if self.ready <= capacity {
            return;
        }
        let target = capacity * 9 / 10;
        let mut candidates: Vec<(u64, &String)> = self
            .map
            .iter()
            .filter_map(|(k, e)| match e {
                ThumbEntry::Ready { last_used, .. } if *last_used < frame => Some((*last_used, k)),
                _ => None,
            })
            .collect();
        let excess = self.ready.saturating_sub(target).min(candidates.len());
        if excess == 0 {
            return;
        }
        candidates.select_nth_unstable_by_key(excess - 1, |(t, _)| *t);
        let victims: Vec<String> = candidates.iter().take(excess).map(|(_, k)| (*k).clone()).collect();
        for k in victims {
            self.remove(&k);
        }
    }
}

impl AssetManager {
    pub(crate) fn thumbnail_impl(&mut self, ctx: &egui::Context, name: &str) -> ThumbState {
        self.shared.set_ctx(ctx);
        self.poll_results();
        self.create_thumb_textures();
        let frame = self.shared.current_frame();
        let key: Cow<'_, str> =
            if is_normalized_material(name) { Cow::Borrowed(name) } else { Cow::Owned(material_key(name)) };
        match self.thumbs.map.get_mut(key.as_ref()) {
            Some(ThumbEntry::Ready { handle, size, last_used }) => {
                *last_used = frame;
                return ThumbState::Ready { texture: handle.id(), size: *size };
            }
            Some(ThumbEntry::Missing) => return ThumbState::Missing,
            Some(ThumbEntry::Pending(seen)) => {
                seen.store(frame, Ordering::Relaxed);
                return ThumbState::Loading;
            }
            None => {}
        }
        if key.is_empty() {
            return ThumbState::Missing;
        }
        let Some(env) = self.shared.env() else {
            return if self.status == AssetStatus::Mounting { ThumbState::Loading } else { ThumbState::Missing };
        };
        let seen = Arc::new(AtomicU64::new(frame));
        let key = key.into_owned();
        self.thumbs.map.insert(key.clone(), ThumbEntry::Pending(Arc::clone(&seen)));
        let fresh = self.fresh.contains(&key);
        self.shared.push(Lane::Thumb, Job::Thumb { name: key, fresh, last_seen: seen, env });
        ThumbState::Loading
    }

    pub(crate) fn on_thumb(&mut self, name: String, epoch: u64, data: Option<ThumbPixels>) {
        if epoch != self.mount_gen || !matches!(self.thumbs.map.get(&name), Some(ThumbEntry::Pending(_))) {
            self.thumbs.forget_pending(&name);
            return;
        }
        let Some(px) = data else {
            self.thumbs.map.insert(name, ThumbEntry::Missing);
            return;
        };
        self.thumbs.decoded.push_back((name, px));
        self.create_thumb_textures();
    }

    pub(crate) fn create_thumb_textures(&mut self) {
        if self.thumbs.decoded.is_empty() {
            return;
        }
        let Some(ctx) = self.shared.ctx() else { return };
        let frame = self.shared.current_frame();
        if self.thumbs.budget.0 != frame {
            self.thumbs.budget = (frame, 0);
        }
        while self.thumbs.budget.1 < THUMBS_PER_FRAME {
            let Some((name, px)) = self.thumbs.decoded.pop_front() else { break };
            if !matches!(self.thumbs.map.get(&name), Some(ThumbEntry::Pending(_))) {
                continue;
            }
            let (w, h) = (px.width as usize, px.height as usize);
            if w == 0 || h == 0 || px.rgba.len() != w * h * 4 {
                self.thumbs.map.insert(name, ThumbEntry::Missing);
                continue;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &px.rgba);
            let handle = ctx.load_texture(
                format!("thumb:{name}"),
                image,
                egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)),
            );
            self.thumbs.insert_ready(name, handle, px.size, frame);
            self.thumbs.budget.1 += 1;
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_eviction() {
        let ctx = egui::Context::default();
        let mut t = Thumbs::default();
        let img = || egui::ColorImage::from_rgba_unmultiplied([1, 1], &[1, 2, 3, 255]);
        for i in 0..THUMB_CAPACITY + 1 {
            let h = ctx.load_texture(format!("t{i}"), img(), egui::TextureOptions::LINEAR);
            t.insert_ready(format!("t{i}"), h, [1, 1], i as u64);
        }
        assert!(t.ready_count() <= THUMB_CAPACITY * 9 / 10 + 1, "{}", t.ready_count());
        assert!(!t.map.contains_key("t0"), "oldest evicted");
        assert!(t.map.contains_key(&format!("t{THUMB_CAPACITY}")), "newest kept");
        t.remove(&format!("t{THUMB_CAPACITY}"));
        let before = t.ready_count();
        t.forget_pending("t1");
        assert_eq!(t.ready_count(), before);
        t.clear();
        assert_eq!(t.ready_count(), 0);
    }
}
