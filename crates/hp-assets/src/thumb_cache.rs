use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use hp_fs::{GameFs, SourceKind};

use crate::jobs::{THUMB_SIZE, ThumbPixels};

const MAGIC: &[u8; 8] = b"HPTHUMB3";
const HEADER_LEN: usize = 8 + 16;
const MAX_FILE: u64 = 1 << 20;
const EXT: &str = "hpthumb";
pub(crate) const MAX_BYTES: u64 = 256 << 20;
const PRUNE_TO: u64 = MAX_BYTES / 4 * 3;
pub(crate) const MIN_COST: Duration = Duration::from_millis(2);

fn fnv1a(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = seed;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;

fn secs(t: Option<SystemTime>) -> u64 {
    t.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

#[derive(Debug)]
pub(crate) struct ThumbDiskCache {
    dir: PathBuf,
    tmp: AtomicU64,
}

impl ThumbDiskCache {
    pub(crate) fn new(dir: PathBuf) -> Self {
        let prune_dir = dir.clone();
        let _ = std::thread::Builder::new().name("hp-thumbs-prune".to_owned()).spawn(move || {
            prune(&prune_dir, MAX_BYTES, PRUNE_TO);
        });
        Self { dir, tmp: AtomicU64::new(0) }
    }

    pub(crate) fn key(fs: &GameFs, name: &str) -> Option<u64> {
        let vpath = format!("materials/{name}.vmt");
        let source = fs.source(fs.which_source(&vpath)?)?;
        if !matches!(source.kind, SourceKind::Vpk(_) | SourceKind::Gma(_)) {
            return None;
        }
        let meta = fs::metadata(&source.path).ok()?;
        let mut h = fnv1a(fnv1a(FNV_OFFSET, MAGIC), source.path.to_string_lossy().to_lowercase().as_bytes());
        h = fnv1a(h, &meta.len().to_le_bytes());
        h = fnv1a(h, &secs(meta.modified().ok()).to_le_bytes());
        h = fnv1a(h, &[0]);
        Some(fnv1a(h, name.as_bytes()))
    }

    fn path(&self, key: u64) -> PathBuf {
        self.dir.join(format!("{key:016x}.{EXT}"))
    }

    pub(crate) fn load(&self, key: u64) -> Option<ThumbPixels> {
        let path = self.path(key);
        let meta = fs::metadata(&path).ok()?;
        if meta.len() > MAX_FILE {
            return None;
        }
        decode(&fs::read(&path).ok()?)
    }

    pub(crate) fn store(&self, key: u64, px: &ThumbPixels) {
        let Some(bytes) = encode(px) else { return };
        if fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let n = self.tmp.fetch_add(1, Ordering::Relaxed);
        let tmp = self.dir.join(format!("{key:016x}.{}-{n}.tmp", std::process::id()));
        let written = fs::File::create(&tmp).and_then(|mut f| f.write_all(&bytes));
        if written.is_err() || fs::rename(&tmp, self.path(key)).is_err() {
            let _ = fs::remove_file(&tmp);
        }
    }
}

pub(crate) fn encode(px: &ThumbPixels) -> Option<Vec<u8>> {
    let (w, h) = (px.width, px.height);
    if w == 0 || h == 0 || w > THUMB_SIZE * 8 || h > THUMB_SIZE * 8 {
        return None;
    }
    if px.rgba.len() != (w as usize) * (h as usize) * 4 {
        return None;
    }
    let mut out = Vec::with_capacity(HEADER_LEN + px.rgba.len() / 3);
    out.extend_from_slice(MAGIC);
    for v in [w, h, px.size[0], px.size[1]] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    );
    image::ImageEncoder::write_image(encoder, &px.rgba, w, h, image::ExtendedColorType::Rgba8).ok()?;
    Some(out)
}

pub(crate) fn decode(bytes: &[u8]) -> Option<ThumbPixels> {
    let header = bytes.get(..HEADER_LEN)?;
    if &header[..8] != MAGIC {
        return None;
    }
    let num = |i: usize| header.get(8 + i * 4..12 + i * 4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes);
    let (w, h, full_w, full_h) = (num(0)?, num(1)?, num(2)?, num(3)?);
    if w == 0 || h == 0 || w > THUMB_SIZE * 8 || h > THUMB_SIZE * 8 {
        return None;
    }
    let image = image::load_from_memory_with_format(&bytes[HEADER_LEN..], image::ImageFormat::Png).ok()?;
    if image.width() != w || image.height() != h {
        return None;
    }
    let rgba = image.into_rgba8().into_raw();
    Some(ThumbPixels { width: w, height: h, rgba, size: [full_w, full_h] })
}

pub(crate) fn prune(dir: &Path, max: u64, target: u64) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let start = Instant::now();
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
        if ext == "tmp" {
            if modified.elapsed().is_ok_and(|age| age > Duration::from_secs(3600)) {
                let _ = fs::remove_file(&path);
            }
            continue;
        }
        if ext != EXT {
            continue;
        }
        total += meta.len();
        files.push((modified, meta.len(), path));
    }
    if total <= max {
        return;
    }
    files.sort_by_key(|(t, _, _)| *t);
    let mut removed = 0usize;
    for (_, len, path) in files {
        if total <= target {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
            removed += 1;
        }
    }
    log::info!("thumbnail cache pruned: {removed} files removed in {:?}", start.elapsed());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch").join(name);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn pixels(w: u32, h: u32) -> ThumbPixels {
        let rgba = (0..w * h).flat_map(|i| [(i % 251) as u8, (i % 13) as u8 * 9, 40, 255]).collect();
        ThumbPixels { width: w, height: h, rgba, size: [w * 4, h * 4] }
    }

    #[test]
    fn thumbnails_round_trip_and_damage_reads_as_a_miss() {
        let px = pixels(128, 64);
        let bytes = encode(&px).expect("encodes");
        let back = decode(&bytes).expect("decodes");
        assert_eq!((back.width, back.height, back.size), (128, 64, [512, 256]));
        assert_eq!(back.rgba, px.rgba);
        assert!(decode(&bytes[..bytes.len() / 2]).is_none());
        assert!(decode(b"HPTHUMB2").is_none());
        let mut old = encode(&px).expect("encodes");
        old[..8].copy_from_slice(b"HPTHUMB2");
        assert!(decode(&old).is_none());
        assert!(decode(&[0u8; 64]).is_none());
        let mut lying = bytes.clone();
        lying[8..12].copy_from_slice(&64u32.to_le_bytes());
        assert!(decode(&lying).is_none(), "the header and the image disagree");
        let mut huge = bytes;
        huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&huge).is_none());
        assert!(encode(&ThumbPixels { width: 4, height: 4, rgba: vec![0; 3], size: [4, 4] }).is_none());
    }

    #[test]
    fn the_cache_stores_loads_and_prunes() {
        let dir = scratch("thumb_cache");
        let cache = ThumbDiskCache { dir: dir.join("thumbs"), tmp: AtomicU64::new(0) };
        assert!(cache.load(42).is_none());
        cache.store(42, &pixels(32, 32));
        let back = cache.load(42).expect("stored");
        assert_eq!((back.width, back.height), (32, 32));
        for k in 0..10 {
            cache.store(100 + k, &pixels(64, 64));
        }
        let size: u64 = fs::read_dir(dir.join("thumbs")).unwrap().flatten().map(|e| e.metadata().unwrap().len()).sum();
        prune(&dir.join("thumbs"), size / 2, size / 4);
        let left: u64 = fs::read_dir(dir.join("thumbs")).unwrap().flatten().map(|e| e.metadata().unwrap().len()).sum();
        assert!(left <= size / 4 + 1, "{left} of {size}");
        prune(&dir.join("missing"), 0, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keys_are_stable_and_only_for_archives() {
        assert_eq!(fnv1a(FNV_OFFSET, b"abc"), fnv1a(FNV_OFFSET, b"abc"));
        assert_ne!(fnv1a(FNV_OFFSET, b"abc"), fnv1a(FNV_OFFSET, b"abd"));
        let dir = scratch("thumb_keys");
        fs::create_dir_all(dir.join("brick")).unwrap();
        fs::write(dir.join("brick/a.vmt"), "LightmappedGeneric { $basetexture brick/a }").unwrap();
        let gfs = GameFs::empty();
        gfs.add_custom_dir(hp_fs::CustomDir::with_prefix(dir.join("brick"), "brick")).unwrap();
        assert_eq!(ThumbDiskCache::key(&gfs, "brick/a"), None);
        assert_eq!(ThumbDiskCache::key(&gfs, "brick/missing"), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
