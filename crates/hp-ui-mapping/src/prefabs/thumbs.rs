use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::UNIX_EPOCH;

use hp_app::runtime::FileStamp;
use hp_app::settings::Settings;

use super::render::{Thumbnail, render_text};

pub(crate) const THUMB_PX: usize = 128;
const MEMORY_CAPACITY: usize = 400;
const UPLOADS_PER_FRAME: usize = 8;
const MAX_SOURCE_BYTES: u64 = 64 << 20;
const MAX_PNG_BYTES: u64 = 1 << 20;
const VERSION: u64 = 1;

enum Entry {
    Pending(FileStamp),
    Ready { texture: egui::TextureHandle, stamp: FileStamp, last_used: u64 },
    Missing(FileStamp),
}

type Job = (PathBuf, FileStamp);
type Done = (PathBuf, FileStamp, Option<Thumbnail>);

pub(crate) struct Thumbs {
    entries: HashMap<PathBuf, Entry>,
    jobs: Option<Sender<Job>>,
    done_tx: Sender<Done>,
    done: Receiver<Done>,
    ready: Vec<Done>,
    clock: u64,
    cache_dir: Option<PathBuf>,
}

impl Default for Thumbs {
    fn default() -> Self {
        Self::new(Some(Settings::cache_dir().join("prefab_thumbs")))
    }
}

fn fnv(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = seed;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

pub(crate) fn cache_name(path: &Path, stamp: FileStamp) -> String {
    let mut h = fnv(0xCBF2_9CE4_8422_2325, path.to_string_lossy().to_lowercase().as_bytes());
    if let Some((time, len)) = stamp {
        let t = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        h = fnv(h, &t.to_le_bytes());
        h = fnv(h, &len.to_le_bytes());
    }
    h = fnv(h, &VERSION.to_le_bytes());
    h = fnv(h, &(THUMB_PX as u64).to_le_bytes());
    format!("{h:016x}.png")
}

fn read_cached(file: &Path) -> Option<Thumbnail> {
    let meta = std::fs::metadata(file).ok()?;
    if meta.len() > MAX_PNG_BYTES {
        return None;
    }
    let bytes = std::fs::read(file).ok()?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?;
    if img.width() as usize != THUMB_PX || img.height() as usize != THUMB_PX {
        return None;
    }
    Some(Thumbnail { size: THUMB_PX, rgba: img.into_rgba8().into_raw() })
}

fn write_cached(file: &Path, t: &Thumbnail) -> std::io::Result<()> {
    let dir = file.parent().ok_or_else(|| std::io::Error::other("no folder"))?;
    std::fs::create_dir_all(dir)?;
    let size = u32::try_from(t.size).map_err(std::io::Error::other)?;
    let img =
        image::RgbaImage::from_raw(size, size, t.rgba.clone()).ok_or_else(|| std::io::Error::other("bad size"))?;
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).map_err(std::io::Error::other)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        file.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
        std::process::id()
    ));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn produce(path: &Path, stamp: FileStamp, cache_dir: Option<&Path>) -> Option<Thumbnail> {
    let cached = cache_dir.map(|d| d.join(cache_name(path, stamp)));
    if let Some(file) = &cached
        && let Some(t) = read_cached(file)
    {
        return Some(t);
    }
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let t = render_text(&text, THUMB_PX)?;
    if let Some(file) = &cached
        && stamp.is_some()
        && let Err(e) = write_cached(file, &t)
    {
        log::debug!("prefab thumbnail cache {}: {e}", file.display());
    }
    Some(t)
}

fn worker(jobs: Receiver<Job>, done: Sender<Done>, cache_dir: Option<PathBuf>, repaint: egui::Context) {
    let mut stack: Vec<Job> = Vec::new();
    loop {
        if stack.is_empty() {
            match jobs.recv() {
                Ok(j) => stack.push(j),
                Err(_) => return,
            }
        }
        loop {
            match jobs.try_recv() {
                Ok(j) => stack.push(j),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let Some((path, stamp)) = stack.pop() else { continue };
        let t = produce(&path, stamp, cache_dir.as_deref());
        if done.send((path, stamp, t)).is_err() {
            return;
        }
        repaint.request_repaint();
    }
}

impl Thumbs {
    pub(crate) fn new(cache_dir: Option<PathBuf>) -> Self {
        let (done_tx, done) = channel();
        Self { entries: HashMap::new(), jobs: None, done_tx, done, ready: Vec::new(), clock: 0, cache_dir }
    }

    pub(crate) fn get(&mut self, ctx: &egui::Context, path: &Path, stamp: FileStamp) -> Option<egui::TextureId> {
        self.clock += 1;
        let clock = self.clock;
        match self.entries.get_mut(path) {
            Some(Entry::Ready { texture, stamp: s, last_used }) if *s == stamp => {
                *last_used = clock;
                return Some(texture.id());
            }
            Some(Entry::Pending(s) | Entry::Missing(s)) if *s == stamp => return None,
            _ => {}
        }
        self.request(ctx, path, stamp);
        None
    }

    pub(crate) fn is_missing(&self, path: &Path) -> bool {
        matches!(self.entries.get(path), Some(Entry::Missing(_)))
    }

    fn request(&mut self, ctx: &egui::Context, path: &Path, stamp: FileStamp) {
        if self.jobs.is_none() {
            let (tx, rx) = channel();
            let done = self.done_tx.clone();
            let dir = self.cache_dir.clone().filter(|_| !hp_compile::sandbox::is_sandboxed());
            let repaint = ctx.clone();
            let spawned = std::thread::Builder::new()
                .name("hp-prefab-thumbs".to_owned())
                .spawn(move || worker(rx, done, dir, repaint));
            match spawned {
                Ok(_) => self.jobs = Some(tx),
                Err(e) => {
                    log::warn!("cannot start the prefab thumbnail worker: {e}");
                    self.entries.insert(path.to_path_buf(), Entry::Missing(stamp));
                    return;
                }
            }
        }
        let sent = self.jobs.as_ref().is_some_and(|tx| tx.send((path.to_path_buf(), stamp)).is_ok());
        let entry = if sent { Entry::Pending(stamp) } else { Entry::Missing(stamp) };
        self.entries.insert(path.to_path_buf(), entry);
    }

    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(d) = self.done.try_recv() {
            self.ready.push(d);
        }
        let n = self.ready.len().min(UPLOADS_PER_FRAME);
        let batch: Vec<Done> = self.ready.drain(..n).collect();
        if !self.ready.is_empty() {
            ctx.request_repaint();
        }
        for (path, stamp, t) in batch {
            if !matches!(self.entries.get(&path), Some(Entry::Pending(s)) if *s == stamp) {
                continue;
            }
            let entry = match t {
                Some(t) if t.rgba.len() == t.size * t.size * 4 => {
                    let image = egui::ColorImage::from_rgba_unmultiplied([t.size, t.size], &t.rgba);
                    let name = format!("prefab_thumb:{}", path.display());
                    let texture = ctx.load_texture(
                        name,
                        image,
                        egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)),
                    );
                    self.clock += 1;
                    Entry::Ready { texture, stamp, last_used: self.clock }
                }
                _ => Entry::Missing(stamp),
            };
            self.entries.insert(path, entry);
        }
        self.evict();
    }

    fn evict(&mut self) {
        let ready = self.entries.values().filter(|e| matches!(e, Entry::Ready { .. })).count();
        if ready <= MEMORY_CAPACITY {
            return;
        }
        let mut used: Vec<(u64, PathBuf)> = self
            .entries
            .iter()
            .filter_map(|(p, e)| match e {
                Entry::Ready { last_used, .. } => Some((*last_used, p.clone())),
                _ => None,
            })
            .collect();
        used.sort_unstable_by_key(|(t, _)| *t);
        for (_, p) in used.into_iter().take(ready - MEMORY_CAPACITY) {
            self.entries.remove(&p);
        }
    }

    #[cfg(test)]
    pub(crate) fn ready_count(&self) -> usize {
        self.entries.values().filter(|e| matches!(e, Entry::Ready { .. })).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: &str = "solid { side { \"plane\" \"(0 64 64) (64 64 64) (64 0 64)\" \"material\" \"A\" } side { \"plane\" \"(0 0 0) (64 0 0) (64 64 0)\" \"material\" \"A\" } side { \"plane\" \"(0 64 64) (0 0 64) (0 0 0)\" \"material\" \"A\" } side { \"plane\" \"(64 64 0) (64 0 0) (64 0 64)\" \"material\" \"A\" } side { \"plane\" \"(64 64 64) (0 64 64) (0 64 0)\" \"material\" \"A\" } side { \"plane\" \"(64 0 0) (0 0 0) (0 0 64)\" \"material\" \"A\" } }";

    #[test]
    fn thumbnails_are_cached_on_disk_and_in_memory() {
        let dir = crate::test_scratch("prefab_thumbs");
        let file = dir.join("box.vmf");
        std::fs::write(&file, BOX).expect("write");
        let empty = dir.join("empty.vmf");
        std::fs::write(&empty, "versioninfo { }").expect("write");
        let cache = dir.join("cache");
        let stamp = hp_app::runtime::file_stamp(&file);
        let t = produce(&file, stamp, Some(&cache)).expect("rendered");
        assert_eq!(t.size, THUMB_PX);
        let cached = cache.join(cache_name(&file, stamp));
        assert!(cached.is_file());
        assert_eq!(read_cached(&cached), Some(t));
        assert!(produce(&empty, hp_app::runtime::file_stamp(&empty), Some(&cache)).is_none());
        assert_ne!(cache_name(&file, stamp), cache_name(&file, None));
        std::fs::write(&cached, b"not a png").expect("write");
        assert!(read_cached(&cached).is_none());

        let ctx = egui::Context::default();
        let mut thumbs = Thumbs::new(Some(cache));
        assert!(thumbs.get(&ctx, &file, stamp).is_none());
        let _ = thumbs.get(&ctx, &empty, None);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            thumbs.poll(&ctx);
            if thumbs.get(&ctx, &file, stamp).is_some() && thumbs.is_missing(&empty) {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "thumbnail produced");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(thumbs.ready_count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
