use std::borrow::Cow;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use hp_vtf::{DecodeOptions, MipSelect, RgbaImage};
use rayon::prelude::*;

use crate::custom::{CustomIndex, ImageFile};
use crate::error::FsError;
use crate::fs::GameFs;
use crate::source::{Source, SourceData};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedTexture {
    pub width: u32,
    pub height: u32,
    pub original_width: u32,
    pub original_height: u32,
    pub rgba: Vec<u8>,
    pub source: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomImageInfo {
    pub path: std::path::PathBuf,
    pub width: u32,
    pub height: u32,
    pub game_width: u32,
    pub game_height: u32,
}

impl CustomImageInfo {
    pub fn is_resized(&self) -> bool {
        (self.width, self.height) != (self.game_width, self.game_height)
    }
}

pub(crate) enum TextureHit<'a> {
    Vtf { source: &'a Source, path: String },
    Image { source: &'a Source, index: &'a CustomIndex, image: &'a ImageFile },
}

pub(crate) fn find_texture<'a>(snapshot: &'a [Arc<Source>], name: &str) -> Option<TextureHit<'a>> {
    let path = format!("materials/{name}.vtf");
    for source in snapshot.iter().filter(|s| s.is_enabled()) {
        if source.contains(&path) {
            return Some(TextureHit::Vtf { source, path });
        }
        if let SourceData::Custom(index) = &source.data
            && let Some(image) = index.image_for_texture(name)
        {
            return Some(TextureHit::Image { source, index, image });
        }
    }
    None
}

fn read_vtf_prefix(path: &Path, opts: &DecodeOptions) -> Result<Vec<u8>, FsError> {
    let mut file = File::open(path).map_err(|e| FsError::io(path, e))?;
    let mut buf = Vec::with_capacity(hp_vtf::MAX_HEADER_LEN);
    (&mut file).take(hp_vtf::MAX_HEADER_LEN as u64).read_to_end(&mut buf).map_err(|e| FsError::io(path, e))?;
    let needed = hp_vtf::required_len(&buf, opts).unwrap_or(usize::MAX);
    if needed > buf.len() {
        let more = needed.saturating_sub(buf.len()) as u64;
        file.take(more).read_to_end(&mut buf).map_err(|e| FsError::io(path, e))?;
    }
    Ok(buf)
}

fn vtf_bytes<'a>(source: &'a Source, path: &str, opts: &DecodeOptions) -> Result<Cow<'a, [u8]>, FsError> {
    if matches!(opts.mip, MipSelect::MaxDimension(_))
        && let Some(disk) = source.disk_path(path)
    {
        return read_vtf_prefix(&disk, opts).map(Cow::Owned);
    }
    source.read_cow(path).ok_or_else(|| FsError::NotFound(path.to_owned()))
}

pub(crate) fn downscale_to_fit(img: RgbaImage, max_dim: u32) -> RgbaImage {
    let max_dim = max_dim.max(1);
    let (w, h) = (img.width, img.height);
    let largest = w.max(h);
    if largest <= max_dim || w == 0 || h == 0 {
        return img;
    }
    let scale = |v: u32| ((u64::from(v) * u64::from(max_dim)) / u64::from(largest)).max(1) as u32;
    let (tw, th) = (scale(w), scale(h));
    let (w, h, tw_us, th_us) = (w as usize, h as usize, tw as usize, th as usize);
    if img.pixels.len() < w * h * 4 {
        return img;
    }
    let src = img.pixels.as_chunks::<4>().0;
    let mut out = vec![0u8; tw_us * th_us * 4];
    out.par_chunks_mut(tw_us * 4).enumerate().for_each(|(ty, row)| {
        let y0 = ty * h / th_us;
        let y1 = ((ty + 1) * h / th_us).max(y0 + 1).min(h);
        for (tx, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let x0 = tx * w / tw_us;
            let x1 = ((tx + 1) * w / tw_us).max(x0 + 1).min(w);
            let mut sum = [0u64; 4];
            let mut count = 0u64;
            for y in y0..y1 {
                if let Some(line) = src.get(y * w + x0..y * w + x1) {
                    for p in line {
                        for (s, v) in sum.iter_mut().zip(p) {
                            *s += u64::from(*v);
                        }
                    }
                    count += line.len() as u64;
                }
            }
            for (o, s) in px.iter_mut().zip(sum) {
                if let Some(avg) = (s + count / 2).checked_div(count) {
                    *o = avg.min(255) as u8;
                }
            }
        }
    });
    RgbaImage { width: tw, height: th, pixels: out }
}

impl GameFs {
    pub fn load_texture(&self, texture_name: &str, max_dim: Option<u32>) -> Result<LoadedTexture, FsError> {
        let name = hp_vmt::normalize_texture_path(texture_name);
        let snapshot = self.snapshot();
        let hit = (!name.is_empty())
            .then(|| find_texture(&snapshot, &name))
            .flatten()
            .ok_or_else(|| FsError::NotFound(format!("materials/{name}.vtf")))?;
        match hit {
            TextureHit::Vtf { source, path } => {
                let opts = DecodeOptions {
                    mip: max_dim.map_or(MipSelect::Largest, MipSelect::MaxDimension),
                    frame: 0,
                    face: 0,
                };
                let bytes = vtf_bytes(source, &path, &opts)?;
                let info = hp_vtf::read_info(&bytes)?;
                let img = hp_vtf::decode_rgba8(&bytes, &opts)?;
                Ok(LoadedTexture {
                    width: img.width,
                    height: img.height,
                    original_width: info.width,
                    original_height: info.height,
                    rgba: img.pixels,
                    source: source.id,
                })
            }
            TextureHit::Image { source, index, image } => {
                let img = image.decode(index.root())?;
                let (original_width, original_height) = crate::custom::game_texture_size(img.width, img.height);
                let img = match max_dim {
                    Some(max) => downscale_to_fit(img, max),
                    None => img,
                };
                Ok(LoadedTexture {
                    width: img.width,
                    height: img.height,
                    original_width,
                    original_height,
                    rgba: img.pixels,
                    source: source.id,
                })
            }
        }
    }

    pub fn load_texture_raw(&self, texture_name: &str) -> Option<Vec<u8>> {
        self.with_texture_raw(texture_name, <[u8]>::to_vec)
    }

    pub fn with_texture_raw<R>(&self, texture_name: &str, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let name = hp_vmt::normalize_texture_path(texture_name);
        if name.is_empty() {
            return None;
        }
        let snapshot = self.snapshot();
        match find_texture(&snapshot, &name)? {
            TextureHit::Vtf { source, path } => source.read_cow(&path).map(|bytes| f(&bytes)),
            TextureHit::Image { .. } => None,
        }
    }

    pub fn texture_size(&self, texture_name: &str) -> Option<(u32, u32)> {
        let name = hp_vmt::normalize_texture_path(texture_name);
        if name.is_empty() {
            return None;
        }
        let snapshot = self.snapshot();
        match find_texture(&snapshot, &name)? {
            TextureHit::Vtf { source, path } => {
                let bytes = source.read_prefix(&path, hp_vtf::MAX_HEADER_LEN)?;
                let info = hp_vtf::read_info(&bytes).ok()?;
                Some((info.width, info.height))
            }
            TextureHit::Image { index, image, .. } => image.game_size(index.root()),
        }
    }

    pub fn texture_info(&self, texture_name: &str) -> Option<hp_vtf::VtfInfo> {
        let name = hp_vmt::normalize_texture_path(texture_name);
        if name.is_empty() {
            return None;
        }
        let snapshot = self.snapshot();
        match find_texture(&snapshot, &name)? {
            TextureHit::Vtf { source, path } => {
                let bytes = source.read_prefix(&path, hp_vtf::MAX_HEADER_LEN)?;
                hp_vtf::read_info(&bytes).ok()
            }
            TextureHit::Image { .. } => None,
        }
    }

    pub fn texture_alpha_flag(&self, texture_name: &str) -> Option<bool> {
        self.texture_flags(texture_name).map(|f| f & (hp_vtf::flags::ONEBITALPHA | hp_vtf::flags::EIGHTBITALPHA) != 0)
    }

    pub fn texture_flags(&self, texture_name: &str) -> Option<u32> {
        let name = hp_vmt::normalize_texture_path(texture_name);
        if name.is_empty() {
            return None;
        }
        let snapshot = self.snapshot();
        match find_texture(&snapshot, &name)? {
            TextureHit::Vtf { source, path } => {
                let bytes = source.read_prefix(&path, hp_vtf::MAX_HEADER_LEN);
                Some(bytes.and_then(|b| hp_vtf::read_info(&b).ok()).map_or(0, |i| i.flags))
            }
            TextureHit::Image { .. } => Some(hp_vtf::flags::EIGHTBITALPHA),
        }
    }

    pub fn material_texture_size(&self, name: &str) -> Option<(u32, u32)> {
        let texture = self.material(name)?.preview_texture()?;
        self.texture_size(&texture)
    }

    pub fn custom_image(&self, texture_name: &str) -> Option<CustomImageInfo> {
        let name = hp_vmt::normalize_texture_path(texture_name);
        if name.is_empty() {
            return None;
        }
        let snapshot = self.snapshot();
        match find_texture(&snapshot, &name)? {
            TextureHit::Image { index, image, .. } => {
                let header = image.header(index.root())?;
                let (game_width, game_height) = crate::custom::game_texture_size(header.width, header.height);
                Some(CustomImageInfo {
                    path: index.root().join(&*image.rel),
                    width: header.width,
                    height: header.height,
                    game_width,
                    game_height,
                })
            }
            TextureHit::Vtf { .. } => None,
        }
    }

    pub fn texture_source(&self, texture_name: &str) -> Option<u32> {
        let name = hp_vmt::normalize_texture_path(texture_name);
        if name.is_empty() {
            return None;
        }
        let snapshot = self.snapshot();
        match find_texture(&snapshot, &name)? {
            TextureHit::Vtf { source, .. } | TextureHit::Image { source, .. } => Some(source.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CustomDir;
    use crate::test_util::{TempDir, png, vtf};

    #[test]
    fn loads_vtf_thumbnails_and_custom_images() {
        let tmp = TempDir::new("texture");
        tmp.write("g/materials/Tex/Big.vtf", &vtf(256, 128, [10, 20, 30, 255]));
        tmp.write("c/Photo.png", &png(300, 150, [200, 100, 50, 255]));
        let fs = GameFs::empty();
        let g = fs.add_game_dir(&tmp.path().join("g")).unwrap()[0];
        let c = fs.add_custom_dir(CustomDir::with_prefix(tmp.path().join("c"), "pics")).unwrap();

        let full = fs.load_texture("tex/big", None).unwrap();
        assert_eq!((full.width, full.height, full.original_width, full.original_height), (256, 128, 256, 128));
        assert_eq!(full.rgba.len(), 256 * 128 * 4);
        assert_eq!(full.source, g);

        let thumb = fs.load_texture("Materials/Tex/Big.vtf", Some(64)).unwrap();
        assert_eq!((thumb.width, thumb.height), (64, 32));
        assert_eq!((thumb.original_width, thumb.original_height), (256, 128));
        let px = thumb.rgba.get(..4).unwrap();
        assert!(px[0].abs_diff(10) <= 8 && px[1].abs_diff(20) <= 8 && px[2].abs_diff(30) <= 8, "{px:?}");

        let img = fs.load_texture("pics/photo", Some(100)).unwrap();
        assert_eq!((img.width, img.height, img.original_width, img.original_height), (100, 50, 256, 128));
        assert_eq!(img.source, c);
        assert_eq!(img.rgba.get(..4).unwrap(), &[200, 100, 50, 255]);

        assert_eq!(fs.texture_size("tex/big"), Some((256, 128)));
        assert_eq!(fs.texture_size("pics/photo"), Some((256, 128)));
        let info = fs.custom_image("pics/photo").unwrap();
        assert_eq!((info.width, info.height, info.game_width, info.game_height), (300, 150, 256, 128));
        assert!(info.is_resized());
        assert!(info.path.ends_with("Photo.png"));
        assert!(fs.custom_image("tex/big").is_none(), "VTFs are not custom images");
        assert_eq!(fs.texture_source("pics/photo"), Some(c));
        assert!(fs.load_texture_raw("tex/big").is_some_and(|b| hp_vtf::is_vtf(&b)));
        assert!(fs.load_texture_raw("pics/photo").is_none());
        assert!(matches!(fs.load_texture("nope", None), Err(FsError::NotFound(_))));
        assert!(matches!(fs.load_texture("", None), Err(FsError::NotFound(_))));
        assert_eq!(fs.texture_size("nope"), None);
    }

    #[test]
    fn alpha_flags_of_textures() {
        let tmp = TempDir::new("texture-alpha-flag");
        tmp.write("g/materials/a/opaque.vtf", &vtf(8, 8, [1, 2, 3, 255]));
        tmp.write("g/materials/a/alpha.vtf", &vtf(8, 8, [1, 2, 3, 100]));
        let mut unflagged = vtf(8, 8, [1, 2, 3, 100]);
        let flags = u32::from_le_bytes(unflagged[20..24].try_into().unwrap())
            & !(hp_vtf::flags::ONEBITALPHA | hp_vtf::flags::EIGHTBITALPHA);
        unflagged[20..24].copy_from_slice(&flags.to_le_bytes());
        tmp.write("g/materials/a/unflagged.vtf", &unflagged);
        tmp.write("g/materials/a/broken.vtf", b"VTF\0junk");
        tmp.write("c/pic.png", &png(4, 4, [1, 2, 3, 255]));
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();
        fs.add_custom_dir(CustomDir::with_prefix(tmp.path().join("c"), "pics")).unwrap();
        assert_eq!(fs.texture_alpha_flag("a/opaque"), Some(false));
        assert_eq!(fs.texture_alpha_flag("Materials/A/Alpha.vtf"), Some(true));
        assert_eq!(fs.texture_alpha_flag("a/unflagged"), Some(false));
        assert_eq!(fs.texture_alpha_flag("a/broken"), Some(false));
        assert_eq!(fs.texture_alpha_flag("pics/pic"), Some(true));
        assert_eq!(fs.texture_alpha_flag("a/nothing"), None);
        assert_eq!(fs.texture_flags("a/broken"), Some(0));
        assert_eq!(fs.texture_flags("a/nothing"), None);
        assert!(fs.texture_flags("a/alpha").is_some_and(|f| f & hp_vtf::flags::EIGHTBITALPHA != 0));
        assert_eq!(fs.texture_alpha_flag(""), None);
    }

    #[test]
    fn editor_size_of_images_matches_the_exported_vtf() {
        let tmp = TempDir::new("texture-npot");
        tmp.write("c/npot.png", &png(100, 60, [1, 2, 3, 255]));
        tmp.write("c/tall.png", &png(3, 5, [1, 2, 3, 255]));
        tmp.write("c/tie.png", &png(96, 48, [1, 2, 3, 255]));
        tmp.write("c/pow.png", &png(64, 32, [1, 2, 3, 255]));
        let fs = GameFs::empty();
        fs.add_custom_dir(CustomDir::with_prefix(tmp.path().join("c"), "pics")).unwrap();
        let out = tmp.mkdir("out");
        let names: Vec<String> = ["npot", "tall", "tie", "pow"].iter().map(|n| format!("pics/{n}")).collect();
        let report = fs.export_custom_materials(&names, &out).unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        let mut resized = fs.resized_custom_images(&names);
        resized.sort_unstable_by(|a, b| a.texture.cmp(&b.texture));
        let sizes: Vec<String> =
            resized.iter().map(|r| format!("{} {:?} {:?}", r.texture, r.size, r.game_size)).collect();
        assert_eq!(sizes, ["pics/npot (100, 60) (128, 64)", "pics/tall (3, 5) (4, 4)", "pics/tie (96, 48) (128, 64)"]);
        for name in &names {
            let vtf = std::fs::read(out.join(format!("materials/{name}.vtf"))).unwrap();
            let info = hp_vtf::read_info(&vtf).unwrap();
            assert_eq!(fs.texture_size(name), Some((info.width, info.height)), "{name}");
            let full = fs.load_texture(name, None).unwrap();
            assert_eq!((full.original_width, full.original_height), (info.width, info.height), "{name}");
        }
        assert_eq!(fs.texture_size("pics/npot"), Some((128, 64)));
        assert_eq!(fs.texture_size("pics/tall"), Some((4, 4)));
        assert_eq!(fs.texture_size("pics/tie"), Some((128, 64)));
        assert!(!fs.custom_image("pics/pow").unwrap().is_resized());
        let full = fs.load_texture("pics/npot", None).unwrap();
        assert_eq!((full.width, full.height), (100, 60));
    }

    #[test]
    fn material_texture_size_reads_headers() {
        let tmp = TempDir::new("texture-matsize");
        tmp.write("g/materials/a/wall.vmt", b"LightmappedGeneric { $basetexture a/wall_tex }");
        tmp.write("g/materials/a/wall_tex.vtf", &vtf(64, 16, [1, 1, 1, 255]));
        tmp.write("g/materials/a/broken.vmt", b"LightmappedGeneric { $basetexture a/none }");
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();
        assert_eq!(fs.material_texture_size("A/Wall"), Some((64, 16)));
        assert_eq!(fs.material_texture_size("a/broken"), None);
        assert_eq!(fs.material_texture_size("a/nothing"), None);
    }

    #[test]
    fn corrupt_vtf_is_an_error_not_a_panic() {
        let tmp = TempDir::new("texture-bad");
        tmp.write("g/materials/bad.vtf", b"VTF\0garbage");
        tmp.write("g/materials/worse.vtf", b"");
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();
        assert!(fs.load_texture("bad", Some(32)).is_err());
        assert!(fs.load_texture("worse", None).is_err());
        assert_eq!(fs.texture_size("bad"), None);
    }

    #[test]
    fn downscale_keeps_aspect_and_color() {
        let img = RgbaImage::new(40, 10, [7u8, 8, 9, 255].repeat(400)).unwrap();
        let small = downscale_to_fit(img.clone(), 8);
        assert_eq!((small.width, small.height), (8, 2));
        assert!(small.pixels.chunks(4).all(|p| p == [7, 8, 9, 255]));
        let same = downscale_to_fit(img.clone(), 64);
        assert_eq!(same, img);
        let tiny = downscale_to_fit(RgbaImage::new(1000, 1, vec![0; 4000]).unwrap(), 10);
        assert_eq!((tiny.width, tiny.height), (10, 1));
    }
}
