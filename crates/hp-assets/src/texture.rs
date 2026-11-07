use ahash::AHashMap;
use hp_render::{TexFormat, TexId, TextureUpload};
use hp_vtf::BcFormat;

use crate::material::TexLook;

pub(crate) const MAX_TEXTURE_DIM: u32 = 2048;

pub(crate) struct PreparedTexture {
    pub upload: TextureUpload,
    pub size: [u32; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TexFailure {
    Missing,
    Broken { texture: String, reason: String },
}

impl TexFailure {
    fn broken(texture: &str, reason: impl ToString) -> Self {
        Self::Broken { texture: texture.to_owned(), reason: reason.to_string() }
    }
}

pub(crate) fn prepare_texture(fs: &hp_fs::GameFs, key: &str, bc: bool) -> Result<PreparedTexture, TexFailure> {
    if let Some(name) = key.strip_suffix(crate::water::FRAMES_SUFFIX) {
        let bytes = fs.load_texture_raw(name).ok_or(TexFailure::Missing)?;
        return crate::water::pack_frames(&bytes).map_err(|e| TexFailure::broken(name, e));
    }
    if let Some(name) = key.strip_suffix(crate::material::LUM_SUFFIX) {
        let mut p = prepare_plain(fs, name, false)?;
        luminance_alpha(&mut p.upload);
        return Ok(p);
    }
    if let Some((name, frame)) = crate::bump::parse_normal_key(key) {
        return prepare_normal(fs, name, frame, bc);
    }
    if let Some((name, frame)) = crate::envmap::parse_mask_key(key) {
        return prepare_normal(fs, name, frame, bc);
    }
    if let Some(name) = key.strip_suffix(crate::envmap::CUBE_SUFFIX) {
        return prepare_cube(fs, name);
    }
    prepare_plain(fs, key, bc)
}

fn prepare_cube(fs: &hp_fs::GameFs, name: &str) -> Result<PreparedTexture, TexFailure> {
    let bytes = fs.load_texture_raw(name).ok_or(TexFailure::Missing)?;
    let upload = crate::envmap::cube_strip(&bytes).map_err(|e| TexFailure::broken(name, e))?;
    Ok(PreparedTexture { size: [upload.width, upload.height], upload })
}

fn prepare_normal(fs: &hp_fs::GameFs, name: &str, frame: u32, bc: bool) -> Result<PreparedTexture, TexFailure> {
    let max = crate::bump::MAX_NORMAL_DIM;
    let mut vtf_error = None;
    if let Some(bytes) = fs.load_texture_raw(name) {
        let frames = hp_vtf::read_info(&bytes).map(|i| i.frames.max(1)).unwrap_or(1);
        let frame = if frame < frames { frame } else { 0 };
        match prepare_vtf_with(&bytes, bc, frame, max) {
            Ok(mut p) => {
                p.upload.format = p.upload.format.to_linear();
                return Ok(p);
            }
            Err(e) => vtf_error = Some(e),
        }
    }
    let tex = match fs.load_texture(name, Some(max)) {
        Ok(tex) => tex,
        Err(hp_fs::FsError::NotFound(_)) => {
            return Err(vtf_error.map_or(TexFailure::Missing, |e| TexFailure::broken(name, e)));
        }
        Err(e) => return Err(TexFailure::broken(name, e)),
    };
    let levels = build_mips(tex.width, tex.height, tex.rgba)
        .ok_or_else(|| TexFailure::broken(name, format!("bad pixel data for {}x{}", tex.width, tex.height)))?;
    Ok(PreparedTexture {
        upload: TextureUpload { width: tex.width, height: tex.height, format: TexFormat::Rgba8Linear, levels },
        size: [tex.original_width.max(1), tex.original_height.max(1)],
    })
}

pub(crate) fn luminance_alpha(up: &mut TextureUpload) {
    if up.format != TexFormat::Rgba8Srgb {
        return;
    }
    for level in &mut up.levels {
        for px in level.chunks_exact_mut(4) {
            let peak = u32::from(px[0].max(px[1]).max(px[2]));
            px[3] = ((peak * u32::from(px[3]) + 127) / 255) as u8;
        }
    }
}

fn prepare_plain(fs: &hp_fs::GameFs, name: &str, bc: bool) -> Result<PreparedTexture, TexFailure> {
    let mut vtf_error = None;
    if let Some(bytes) = fs.load_texture_raw(name) {
        match prepare_vtf(&bytes, bc) {
            Ok(p) => return Ok(p),
            Err(e) => {
                log::debug!("texture {name}: {e}, decoding a single level");
                vtf_error = Some(e);
            }
        }
    }
    let tex = match fs.load_texture(name, Some(MAX_TEXTURE_DIM)) {
        Ok(tex) => tex,
        Err(hp_fs::FsError::NotFound(_)) => {
            return Err(vtf_error.map_or(TexFailure::Missing, |e| TexFailure::broken(name, e)));
        }
        Err(e) => return Err(TexFailure::broken(name, e)),
    };
    let levels = build_mips(tex.width, tex.height, tex.rgba)
        .ok_or_else(|| TexFailure::broken(name, format!("bad pixel data for {}x{}", tex.width, tex.height)))?;
    Ok(PreparedTexture {
        upload: TextureUpload { width: tex.width, height: tex.height, format: TexFormat::Rgba8Srgb, levels },
        size: [tex.original_width.max(1), tex.original_height.max(1)],
    })
}

pub(crate) fn prepare_vtf(bytes: &[u8], bc: bool) -> Result<PreparedTexture, String> {
    prepare_vtf_with(bytes, bc, 0, MAX_TEXTURE_DIM)
}

pub(crate) fn prepare_vtf_with(bytes: &[u8], bc: bool, frame: u32, max_dim: u32) -> Result<PreparedTexture, String> {
    let info = hp_vtf::read_info(bytes).map_err(|e| e.to_string())?;
    let size = [info.width.max(1), info.height.max(1)];
    let mips = hp_vtf::raw_mips(bytes, frame, 0).map_err(|e| e.to_string())?;
    let first = mips
        .iter()
        .position(|l| l.width.max(l.height) <= max_dim)
        .ok_or_else(|| format!("no mip level of at most {max_dim} pixels"))?;
    let levels = mips.get(first..).unwrap_or_default();
    let top = levels.first().ok_or("no mip level")?;
    let (w, h) = (top.width, top.height);
    let bc_format = match top.format.bc_format() {
        Some(BcFormat::Bc1) => Some(TexFormat::Bc1Srgb),
        Some(BcFormat::Bc2) => Some(TexFormat::Bc2Srgb),
        Some(BcFormat::Bc3) => Some(TexFormat::Bc3Srgb),
        _ => None,
    };
    if bc
        && let Some(format) = bc_format
        && w.is_multiple_of(4)
        && h.is_multiple_of(4)
    {
        let data: Vec<Vec<u8>> = levels.iter().map(|l| l.data.to_vec()).collect();
        return Ok(PreparedTexture { upload: TextureUpload { width: w, height: h, format, levels: data }, size });
    }
    let mut data = Vec::with_capacity(levels.len());
    for level in levels {
        match level.to_rgba8() {
            Ok(img) if img.width == level.width && img.height == level.height => data.push(img.pixels),
            _ if !data.is_empty() => break,
            Ok(img) => return Err(format!("mip decoded as {}x{} instead of {}x{}", img.width, img.height, w, h)),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(PreparedTexture {
        upload: TextureUpload { width: w, height: h, format: TexFormat::Rgba8Srgb, levels: data },
        size,
    })
}

pub(crate) fn build_mips(width: u32, height: u32, rgba: Vec<u8>) -> Option<Vec<Vec<u8>>> {
    let (mut w, mut h) = (width.max(1) as usize, height.max(1) as usize);
    if rgba.len() != w * h * 4 {
        return None;
    }
    let mut levels = vec![rgba];
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let src = levels.last()?;
        let mut dst = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            let y0 = (y * 2).min(h - 1);
            let y1 = (y * 2 + 1).min(h - 1);
            for x in 0..nw {
                let x0 = (x * 2).min(w - 1);
                let x1 = (x * 2 + 1).min(w - 1);
                for c in 0..4 {
                    let at = |xx: usize, yy: usize| u32::from(src.get((yy * w + xx) * 4 + c).copied().unwrap_or(0));
                    let sum = at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1);
                    if let Some(d) = dst.get_mut((y * nw + x) * 4 + c) {
                        *d = ((sum + 2) / 4) as u8;
                    }
                }
            }
        }
        levels.push(dst);
        w = nw;
        h = nh;
    }
    Some(levels)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TexTarget {
    Main,
    Preview,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TexEntry {
    pub current: Option<(TexId, [u32; 2])>,
    pub loading: bool,
    pub queued: bool,
    pub missing: bool,
}

#[derive(Default)]
pub(crate) struct TexRegistry {
    pub map: AHashMap<String, TexEntry>,
}

impl TexRegistry {
    pub(crate) fn look(&self, name: &str) -> TexLook {
        match self.map.get(name) {
            Some(TexEntry { current: Some((id, size)), .. }) => TexLook::Ready(*id, *size),
            Some(e) if e.missing && !e.loading => TexLook::Missing,
            _ => TexLook::Loading,
        }
    }

    pub(crate) fn request(&mut self, name: &str) -> bool {
        if self.map.contains_key(name) {
            return false;
        }
        self.map.insert(name.to_owned(), TexEntry { loading: true, ..TexEntry::default() });
        true
    }

    pub(crate) fn loading_count(&self) -> usize {
        self.map.values().filter(|e| e.loading).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain() {
        let px: Vec<u8> = [10u8, 20, 30, 255].repeat(8 * 4);
        let levels = build_mips(8, 4, px).unwrap();
        let sizes: Vec<usize> = levels.iter().map(Vec::len).collect();
        assert_eq!(sizes, vec![8 * 4 * 4, 4 * 2 * 4, 2 * 4, 4]);
        assert_eq!(levels.last().unwrap(), &vec![10, 20, 30, 255]);
        assert!(build_mips(4, 4, vec![0; 3]).is_none());
        let odd = build_mips(3, 1, vec![0, 0, 0, 0, 90, 90, 90, 90, 200, 200, 200, 200]).unwrap();
        assert_eq!(odd.len(), 2);
        assert_eq!(odd[1], vec![45, 45, 45, 45]);
    }

    fn vtf(w: u32, h: u32, format: hp_vtf::EncodeFormat, mipmaps: bool) -> Vec<u8> {
        let img = hp_vtf::RgbaImage::new(w, h, [200u8, 100, 50, 255].repeat((w * h) as usize)).unwrap();
        let opts = hp_vtf::EncodeOptions { format, mipmaps, ..Default::default() };
        hp_vtf::encode(&img, &opts).unwrap()
    }

    #[test]
    fn vtf_bc_passthrough_and_decode() {
        let bytes = vtf(64, 32, hp_vtf::EncodeFormat::Dxt1, true);
        let p = prepare_vtf(&bytes, true).unwrap();
        assert_eq!(p.upload.format, TexFormat::Bc1Srgb);
        assert_eq!((p.upload.width, p.upload.height), (64, 32));
        assert_eq!(p.upload.levels.len(), 7);
        assert_eq!(p.size, [64, 32]);

        let p = prepare_vtf(&bytes, false).unwrap();
        assert_eq!(p.upload.format, TexFormat::Rgba8Srgb);
        assert_eq!(p.upload.levels[0].len(), 64 * 32 * 4);
        assert_eq!(p.upload.levels.len(), 7);

        let single = vtf(16, 16, hp_vtf::EncodeFormat::Bgra8888, false);
        let p = prepare_vtf(&single, true).unwrap();
        assert_eq!(p.upload.format, TexFormat::Rgba8Srgb);
        assert_eq!(p.upload.levels.len(), 1, "the authored VTF has no mip chain");
        let px = &p.upload.levels[0][..4];
        assert!(px[0].abs_diff(200) < 3 && px[1].abs_diff(100) < 3, "{px:?}");

        assert!(prepare_vtf(b"VTF\0junk", true).is_err());
    }

    #[test]
    fn normal_maps_are_linear_and_capped() {
        let bytes = vtf(2048, 64, hp_vtf::EncodeFormat::Dxt5, true);
        let p = prepare_vtf_with(&bytes, true, 0, crate::bump::MAX_NORMAL_DIM).unwrap();
        assert_eq!((p.upload.width, p.upload.height), (1024, 32));
        let mut up = p.upload;
        up.format = up.format.to_linear();
        assert_eq!(up.format, TexFormat::Bc3Linear);
        let rgba = prepare_vtf_with(&vtf(16, 16, hp_vtf::EncodeFormat::Bgra8888, false), true, 0, 1024).unwrap();
        assert_eq!(rgba.upload.format.to_linear(), TexFormat::Rgba8Linear);
    }

    #[test]
    fn big_textures_are_capped() {
        let bytes = vtf(4096, 64, hp_vtf::EncodeFormat::Dxt1, true);
        let p = prepare_vtf(&bytes, true).unwrap();
        assert_eq!((p.upload.width, p.upload.height), (2048, 32));
        assert_eq!(p.size, [4096, 64]);
    }

    #[test]
    fn luminance_alpha_for_additive_sprites() {
        let mut up = TextureUpload {
            width: 2,
            height: 1,
            format: TexFormat::Rgba8Srgb,
            levels: vec![vec![0, 0, 0, 255, 200, 100, 50, 255], vec![10, 20, 30, 128]],
        };
        luminance_alpha(&mut up);
        assert_eq!(up.levels[0], vec![0, 0, 0, 0, 200, 100, 50, 200]);
        assert_eq!(up.levels[1][3], 15);
        let mut bc = TextureUpload { width: 4, height: 4, format: TexFormat::Bc1Srgb, levels: vec![vec![1; 8]] };
        luminance_alpha(&mut bc);
        assert_eq!(bc.levels[0], vec![1; 8]);
    }

    #[test]
    fn registry() {
        let mut r = TexRegistry::default();
        assert_eq!(r.look("a"), TexLook::Loading);
        assert!(r.request("a"));
        assert!(!r.request("a"));
        assert_eq!(r.loading_count(), 1);
        if let Some(e) = r.map.get_mut("a") {
            e.loading = false;
            e.missing = true;
        }
        assert_eq!(r.look("a"), TexLook::Missing);
        if let Some(e) = r.map.get_mut("a") {
            e.current = Some((TexId::WHITE, [4, 4]));
        }
        assert_eq!(r.look("a"), TexLook::Ready(TexId::WHITE, [4, 4]));
    }
}
