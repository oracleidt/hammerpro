use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

use hp_app::state::ThumbState;

#[derive(Default)]
pub(crate) struct FilePreview {
    path: PathBuf,
    text: Option<String>,
    job: Option<Receiver<Option<hp_vtf::RgbaImage>>>,
    handle: Option<egui::TextureHandle>,
    size: [u32; 2],
}

impl FilePreview {
    pub fn new(path: PathBuf) -> Self {
        Self { path, ..Default::default() }
    }

    pub fn invalidate(&mut self) {
        self.text = None;
        self.job = None;
        self.handle = None;
    }

    pub fn thumbnail(&mut self, ctx: &egui::Context, text: &str, fallback: Option<&hp_fs::GameFs>) -> ThumbState {
        if self.text.as_deref() != Some(text) {
            self.text = Some(text.to_owned());
            self.handle = None;
            self.job = None;
            let (tx, rx) = std::sync::mpsc::channel();
            let path = self.path.clone();
            let text = text.to_owned();
            let fs = fallback.cloned();
            let wake = ctx.clone();
            if std::thread::Builder::new()
                .name("hp-vmt-file-preview".to_owned())
                .spawn(move || {
                    let image = load_preview(&path, &text, fs.as_ref());
                    let _ = tx.send(image);
                    wake.request_repaint();
                })
                .is_ok()
            {
                self.job = Some(rx);
            }
        }
        if let Some(job) = &self.job {
            match job.try_recv() {
                Ok(image) => {
                    self.job = None;
                    if let Some(image) = image {
                        self.size = [image.width, image.height];
                        self.handle = Some(ctx.load_texture(
                            format!("vmt-file:{}", self.path.display()),
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width as usize, image.height as usize],
                                &image.pixels,
                            ),
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                Err(TryRecvError::Empty) => return ThumbState::Loading,
                Err(TryRecvError::Disconnected) => self.job = None,
            }
        }
        self.handle
            .as_ref()
            .map_or(ThumbState::Missing, |handle| ThumbState::Ready { texture: handle.id(), size: self.size })
    }
}

fn local_texture(path: &Path, texture: &str) -> Option<Vec<u8>> {
    let texture = hp_vmt::normalize_texture_path(texture);
    if texture.is_empty() || texture.split('/').any(|part| part == ".." || part.contains(':')) {
        return None;
    }
    let root = path.ancestors().find(|dir| dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("materials")));
    let root = root.or_else(|| path.parent())?;
    std::fs::read(root.join(format!("{texture}.vtf"))).ok()
}

fn load_preview(path: &Path, text: &str, fallback: Option<&hp_fs::GameFs>) -> Option<hp_vtf::RgbaImage> {
    let material = hp_vmt::parse(text).ok()?.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
    let texture = material.preview_texture()?;
    let bytes = local_texture(path, &texture).or_else(|| fallback.and_then(|fs| fs.load_texture_raw(&texture)))?;
    let flags = hp_vtf::read_info(&bytes).ok()?.flags;
    let mut image = hp_vtf::decode_rgba8(
        &bytes,
        &hp_vtf::DecodeOptions { mip: hp_vtf::MipSelect::MaxDimension(220), frame: 0, face: 0 },
    )
    .ok()?;
    let usage = material.alpha_usage(&|_| flags & (hp_vtf::flags::ONEBITALPHA | hp_vtf::flags::EIGHTBITALPHA) != 0);
    for pixel in image.pixels.chunks_exact_mut(4) {
        pixel[3] = (usage.coverage(f32::from(pixel[3]) / 255.0) * 255.0).round() as u8;
    }
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalidation_reloads_a_texture_overwritten_without_changing_its_vmt() {
        let root = std::env::temp_dir().join(format!("hp-vmt-preview-refresh-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sample.vmt");
        let texture = root.join("sample.vtf");
        let write = |size: u32| {
            let image = hp_vtf::RgbaImage { width: size, height: size, pixels: vec![127; (size * size * 4) as usize] };
            std::fs::write(&texture, hp_vtf::encode(&image, &Default::default()).unwrap()).unwrap();
        };
        let ctx = egui::Context::default();
        let mut preview = FilePreview::new(path);
        let text = "LightmappedGeneric { $basetexture sample }";
        let ready = |preview: &mut FilePreview| {
            for _ in 0..400 {
                match preview.thumbnail(&ctx, text, None) {
                    ThumbState::Ready { size, .. } => return size,
                    ThumbState::Missing => panic!("local file preview missing"),
                    ThumbState::Loading => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
            panic!("local file preview timed out")
        };
        write(16);
        assert_eq!(ready(&mut preview), [16, 16]);
        write(32);
        preview.invalidate();
        assert_eq!(ready(&mut preview), [32, 32]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_material_preview_reads_an_unmounted_texture_beside_the_material() {
        let root = std::env::temp_dir().join(format!("hp-vmt-local-preview-{}", std::process::id()));
        let dir = root.join("materials/qa");
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../hp-assets/tests/data/native_mips/tail_rgb.vtf");
        std::fs::copy(fixture, dir.join("sample.v2.vtf")).unwrap();
        let image =
            load_preview(&dir.join("sample.vmt"), "LightmappedGeneric { $basetexture qa/sample.v2 }", None).unwrap();
        assert_eq!([image.width, image.height], [64, 64]);
        assert!(local_texture(&dir.join("sample.vmt"), "../../sample").is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
