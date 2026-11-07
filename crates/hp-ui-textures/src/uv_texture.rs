use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender};

use crate::uv_layers::LayerTexture;

const MAX_SIDE: u32 = 4096;
const MAX_BYTES: usize = 64 << 20;
const MAX_ENTRIES: usize = 24;

struct Request {
    fs: hp_fs::GameFs,
    name: String,
    epoch: u64,
    ctx: egui::Context,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_upload_preserves_fine_texels_and_refreshes_after_content_changes() {
        let dir = crate::tests::scratch("uv_texture_detail");
        let materials = dir.join("materials/t");
        std::fs::create_dir_all(&materials).unwrap();
        let write = |reverse: bool| {
            let pixels: Vec<u8> = (0..256)
                .flat_map(|y| {
                    (0..512).flat_map(move |x| {
                        let value = if ((x + y) % 2 == 0) ^ reverse { 255 } else { 0 };
                        [value, 30, 80, 0]
                    })
                })
                .collect();
            let image = hp_vtf::RgbaImage::new(512, 256, pixels).unwrap();
            let options =
                hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Bgra8888, mipmaps: true, ..Default::default() };
            std::fs::write(materials.join("fine.vtf"), hp_vtf::encode(&image, &options).unwrap()).unwrap();
        };
        write(false);
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir).unwrap();
        let ctx = egui::Context::default();
        let mut textures = Textures::default();
        let mut wait = |generation| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if let Some(texture) = textures.get(&ctx, &fs, generation, "t/fine") {
                    break texture;
                }
                assert!(std::time::Instant::now() < until, "UV texture finishes loading");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        };
        let texture = wait(1);
        assert_eq!(texture.size, [512, 256]);
        assert_eq!(texture.handle.size(), [512, 256]);
        let delta = ctx.tex_manager().write().take_delta();
        let upload = &delta.set.get(&texture.handle.id()).unwrap()[0];
        let egui::ImageData::Color(image) = &upload.image;
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(255, 30, 80));
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(0, 30, 80));
        assert_eq!(upload.options.magnification, egui::TextureFilter::Nearest);
        write(true);
        fs.refresh();
        let refreshed = wait(2);
        assert_ne!(texture.handle.id(), refreshed.handle.id());
        let delta = ctx.tex_manager().write().take_delta();
        let upload = &delta.set.get(&refreshed.handle.id()).unwrap()[0];
        let egui::ImageData::Color(image) = &upload.image;
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(0, 30, 80));
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(255, 30, 80));
        drop(textures);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

struct Done {
    name: String,
    epoch: u64,
    image: Option<(egui::ColorImage, [u32; 2])>,
}

struct Worker {
    tx: SyncSender<Request>,
    rx: Receiver<Done>,
}

impl Worker {
    fn new() -> Option<Self> {
        let (tx, requests) = mpsc::sync_channel::<Request>(4);
        let (results, rx) = mpsc::sync_channel(2);
        std::thread::Builder::new()
            .name("hp-uv-textures".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    let image = load(&request.fs, &request.name);
                    if results.send(Done { name: request.name, epoch: request.epoch, image }).is_err() {
                        break;
                    }
                    request.ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Self { tx, rx })
    }
}

fn load(fs: &hp_fs::GameFs, name: &str) -> Option<(egui::ColorImage, [u32; 2])> {
    let mut texture = fs.load_texture(name, Some(MAX_SIDE)).ok()?;
    let (w, h) = (texture.width as usize, texture.height as usize);
    if w == 0 || h == 0 || texture.rgba.len() != w * h * 4 {
        return None;
    }
    for pixel in texture.rgba.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    Some((
        egui::ColorImage::from_rgba_unmultiplied([w, h], &texture.rgba),
        [texture.original_width.max(1), texture.original_height.max(1)],
    ))
}

struct Entry {
    loaded: bool,
    texture: Option<LayerTexture>,
    bytes: usize,
    used: u64,
}

#[derive(Default)]
pub(crate) struct Textures {
    entries: HashMap<String, Entry>,
    generation: Option<(u64, u64)>,
    epoch: u64,
    clock: u64,
    bytes: usize,
    worker: Option<Worker>,
}

impl Textures {
    pub(crate) fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.entries.clear();
        self.bytes = 0;
    }

    pub(crate) fn get(
        &mut self,
        ctx: &egui::Context,
        fs: &hp_fs::GameFs,
        generation: u64,
        name: &str,
    ) -> Option<LayerTexture> {
        let generation = (generation, fs.generation());
        if self.generation != Some(generation) {
            self.clear();
            self.generation = Some(generation);
        }
        self.clock = self.clock.wrapping_add(1);
        let key = hp_vmt::normalize_texture_path(name);
        if self.worker.is_none() {
            self.worker = Worker::new();
        }
        let worker = self.worker.as_ref()?;
        while let Ok(done) = worker.rx.try_recv() {
            if done.epoch != self.epoch {
                continue;
            }
            let Some(entry) = self.entries.get_mut(&done.name) else { continue };
            entry.loaded = true;
            if let Some((image, size)) = done.image {
                entry.bytes = image.pixels.len() * 4;
                let options = egui::TextureOptions {
                    magnification: egui::TextureFilter::Nearest,
                    ..egui::TextureOptions::LINEAR_REPEAT
                };
                let handle = ctx.load_texture(format!("hp_uv:{}:{}", self.epoch, done.name), image, options);
                entry.texture = Some(LayerTexture { handle, size });
                self.bytes += entry.bytes;
            }
        }
        self.evict(&key);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.clock;
            return entry.texture.clone();
        }
        let request = Request { fs: fs.clone(), name: key.clone(), epoch: self.epoch, ctx: ctx.clone() };
        if self.worker.as_ref()?.tx.try_send(request).is_ok() {
            self.entries.insert(key, Entry { loaded: false, texture: None, bytes: 0, used: self.clock });
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        None
    }

    fn evict(&mut self, keep: &str) {
        while self.bytes > MAX_BYTES || self.entries.len() >= MAX_ENTRIES {
            let oldest =
                self.entries.iter().filter(|(key, e)| key.as_str() != keep && e.loaded).min_by_key(|(_, e)| e.used);
            let Some((key, _)) = oldest else { break };
            let key = key.clone();
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes -= entry.bytes;
            }
        }
    }
}
