use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub(crate) fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::var_os("HP_FS_SCRATCH")
            .map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-fs/scratch"), PathBuf::from);
        let path = base.join(format!("{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn write(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }

    pub(crate) fn mkdir(&self, rel: &str) -> PathBuf {
        let path = self.path.join(rel);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_raw(w, h, rgba.repeat((w * h) as usize)).unwrap();
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

pub(crate) fn vtf(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let img = hp_vtf::RgbaImage::new(w, h, rgba.repeat((w * h) as usize)).unwrap();
    hp_vtf::encode(&img, &hp_vtf::EncodeOptions::default()).unwrap()
}

pub(crate) fn vpk(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tree = Vec::new();
    for (path, data) in files {
        let (dir, file) = path.rsplit_once('/').unwrap();
        let (name, ext) = file.rsplit_once('.').unwrap();
        for part in [ext, dir, name] {
            tree.extend_from_slice(part.as_bytes());
            tree.push(0);
        }
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&(data.len() as u16).to_le_bytes());
        tree.extend_from_slice(&0x7FFFu16.to_le_bytes());
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&0xFFFFu16.to_le_bytes());
        tree.extend_from_slice(data);
        tree.extend_from_slice(&[0, 0]);
    }
    tree.push(0);
    let mut out = Vec::new();
    out.extend_from_slice(&0x55AA_1234u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
    out.extend_from_slice(&tree);
    out
}
