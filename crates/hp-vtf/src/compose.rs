use std::borrow::Cow;

use texpresso::Params;

use crate::encode::{Filter, check_image, filter_for, nearest_pow2, reflectivity, resample, texpresso_params};
use crate::header::{RESOURCE_HIGH_RES, RESOURCE_LOW_RES, SIGNATURE};
use crate::{EncodeQuality, ImageFormat, MipFilter, RgbaImage, VtfError, flags};

const LOW_RES_MAX: u32 = 16;

pub const ENCODABLE: &[ImageFormat] = &[
    ImageFormat::Dxt1,
    ImageFormat::Dxt3,
    ImageFormat::Dxt5,
    ImageFormat::Ati1n,
    ImageFormat::Ati2n,
    ImageFormat::Bgra8888,
    ImageFormat::Rgba8888,
    ImageFormat::Abgr8888,
    ImageFormat::Argb8888,
    ImageFormat::Bgrx8888,
    ImageFormat::Bgr888,
    ImageFormat::Rgb888,
    ImageFormat::Bgr565,
    ImageFormat::Rgb565,
    ImageFormat::Bgra4444,
    ImageFormat::Bgra5551,
    ImageFormat::Ia88,
    ImageFormat::I8,
    ImageFormat::A8,
    ImageFormat::Uv88,
];

#[must_use]
pub fn can_encode(format: ImageFormat) -> bool {
    ENCODABLE.contains(&format) || format == ImageFormat::Dxt1OneBitAlpha
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComposeOptions {
    pub format: ImageFormat,
    pub mipmaps: bool,
    pub flags: u32,
    pub version_minor: u32,
    pub resize_to_pow2: bool,
    pub quality: EncodeQuality,
    pub mip_filter: MipFilter,
    pub bumpmap_scale: f32,
    pub first_frame: u16,
    pub reflectivity: Option<[f32; 3]>,
    pub recompute_alpha_flags: bool,
}

impl Default for ComposeOptions {
    fn default() -> Self {
        Self {
            format: ImageFormat::Dxt5,
            mipmaps: true,
            flags: 0,
            version_minor: 4,
            resize_to_pow2: true,
            quality: EncodeQuality::Normal,
            mip_filter: MipFilter::FromFlags,
            bumpmap_scale: 1.0,
            first_frame: 0,
            reflectivity: None,
            recompute_alpha_flags: false,
        }
    }
}

fn luma(p: &[u8]) -> u8 {
    ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114 + 500) / 1000) as u8
}

fn q(v: u8, max: u32) -> u16 {
    ((u32::from(v) * max + 127) / 255) as u16
}

fn map_px<const N: usize>(rgba: &[u8], f: impl Fn(&[u8]) -> [u8; N]) -> Vec<u8> {
    rgba.chunks_exact(4).flat_map(f).collect()
}

fn map_px16(rgba: &[u8], f: impl Fn(&[u8]) -> u16) -> Vec<u8> {
    rgba.chunks_exact(4).flat_map(|p| f(p).to_le_bytes()).collect()
}

fn pack_level(format: ImageFormat, rgba: &[u8], w: usize, h: usize, params: Params) -> Vec<u8> {
    use ImageFormat as F;
    let bc = |f: texpresso::Format| {
        let mut out = vec![0u8; f.compressed_size(w, h)];
        f.compress(rgba, w, h, params, &mut out);
        out
    };
    match format {
        F::Dxt1 | F::Dxt1OneBitAlpha => bc(texpresso::Format::Bc1),
        F::Dxt3 => bc(texpresso::Format::Bc2),
        F::Dxt5 => bc(texpresso::Format::Bc3),
        F::Ati1n => bc(texpresso::Format::Bc4),
        F::Ati2n => bc(texpresso::Format::Bc5),
        F::Bgra8888 => map_px(rgba, |p| [p[2], p[1], p[0], p[3]]),
        F::Rgba8888 => rgba.to_vec(),
        F::Abgr8888 => map_px(rgba, |p| [p[3], p[2], p[1], p[0]]),
        F::Argb8888 => map_px(rgba, |p| [p[3], p[0], p[1], p[2]]),
        F::Bgrx8888 => map_px(rgba, |p| [p[2], p[1], p[0], 255]),
        F::Bgr888 => map_px(rgba, |p| [p[2], p[1], p[0]]),
        F::Rgb888 => map_px(rgba, |p| [p[0], p[1], p[2]]),
        F::Rgb565 => map_px16(rgba, |p| q(p[0], 31) | (q(p[1], 63) << 5) | (q(p[2], 31) << 11)),
        F::Bgr565 => map_px16(rgba, |p| q(p[2], 31) | (q(p[1], 63) << 5) | (q(p[0], 31) << 11)),
        F::Bgra5551 => map_px16(rgba, |p| {
            q(p[2], 31) | (q(p[1], 31) << 5) | (q(p[0], 31) << 10) | if p[3] >= 128 { 0x8000 } else { 0 }
        }),
        F::Bgra4444 => map_px16(rgba, |p| q(p[2], 15) | (q(p[1], 15) << 4) | (q(p[0], 15) << 8) | (q(p[3], 15) << 12)),
        F::Ia88 => map_px(rgba, |p| [luma(p), p[3]]),
        F::I8 => map_px(rgba, |p| [luma(p)]),
        F::A8 => map_px(rgba, |p| [p[3]]),
        F::Uv88 => map_px(rgba, |p| [p[0], p[1]]),
        _ => Vec::new(),
    }
}

fn image_dims(images: &[Vec<RgbaImage>]) -> Result<(usize, usize, usize, usize), VtfError> {
    let first = images.first().and_then(|f| f.first()).ok_or_else(|| VtfError::InvalidImage("no frames".into()))?;
    if images.len() > usize::from(u16::MAX) {
        return Err(VtfError::InvalidImage("too many frames".into()));
    }
    let faces = images[0].len();
    if !matches!(faces, 1 | 6 | 7) {
        return Err(VtfError::InvalidImage("a texture has 1 face, or 6 or 7 for a cubemap".into()));
    }
    let (sw, sh) = check_image(first)?;
    for frame in images {
        if frame.len() != faces {
            return Err(VtfError::InvalidImage("all frames must have the same number of faces".into()));
        }
        for img in frame {
            if check_image(img)? != (sw, sh) {
                return Err(VtfError::InvalidImage("all frames must have the same size".into()));
            }
        }
    }
    Ok((sw, sh, images.len(), faces))
}

pub fn compose(images: &[Vec<RgbaImage>], opts: &ComposeOptions) -> Result<Vec<u8>, VtfError> {
    if opts.version_minor > 5 {
        return Err(VtfError::UnsupportedVersion(7, opts.version_minor));
    }
    if !can_encode(opts.format) {
        return Err(VtfError::UnsupportedFormat(opts.format));
    }
    let (sw, sh, frame_count, faces) = image_dims(images)?;
    let first = &images[0][0];

    let (tw, th) = if opts.resize_to_pow2 {
        (nearest_pow2(first.width), nearest_pow2(first.height))
    } else {
        (first.width, first.height)
    };
    if tw > u32::from(u16::MAX) || th > u32::from(u16::MAX) {
        return Err(VtfError::TooLarge);
    }
    let (w, h) = (tw as usize, th as usize);

    let mut tex_flags = opts.flags & !flags::ENVMAP;
    if faces > 1 {
        tex_flags |= flags::ENVMAP;
    }
    if opts.recompute_alpha_flags {
        tex_flags &= !(flags::ONEBITALPHA | flags::EIGHTBITALPHA);
    }
    let filter = match opts.mip_filter {
        MipFilter::FromFlags => filter_for(tex_flags),
        MipFilter::Color => Filter::Gamma,
        MipFilter::Data => Filter::Linear,
        MipFilter::Normal => Filter::Normal,
    };
    let opaque = images.iter().flatten().all(RgbaImage::is_opaque);
    let format = opts.format;
    if !opaque {
        match format {
            ImageFormat::Dxt1 | ImageFormat::Dxt1OneBitAlpha => tex_flags |= flags::ONEBITALPHA,
            f if f.has_alpha() && tex_flags & flags::ONEBITALPHA == 0 => tex_flags |= flags::EIGHTBITALPHA,
            _ => {}
        }
    }

    let mip_count = if opts.mipmaps { 32 - tw.max(th).leading_zeros() } else { 1 };

    let (mut lw, mut lh) = (tw, th);
    let mut low_mip = 0u32;
    while lw > LOW_RES_MAX || lh > LOW_RES_MAX {
        lw = (lw / 2).max(1);
        lh = (lh / 2).max(1);
        low_mip += 1;
    }

    let params = texpresso_params(opts.quality, filter);
    let mut levels: Vec<Vec<Vec<u8>>> = Vec::with_capacity(frame_count * faces);
    let mut low_src: Option<Vec<u8>> = None;
    let mut base_reflectivity = [0f32; 3];
    for (fi, frame) in images.iter().enumerate() {
        for (face, img) in frame.iter().enumerate() {
            let mut cur: Cow<'_, [u8]> = if (sw, sh) == (w, h) {
                Cow::Borrowed(&img.pixels)
            } else {
                Cow::Owned(resample(&img.pixels, sw, sh, w, h, filter))
            };
            if fi == 0 && face == 0 {
                base_reflectivity = reflectivity(&cur);
            }
            let (mut cw, mut ch) = (w, h);
            let mut out_levels = Vec::with_capacity(mip_count as usize);
            for mip in 0..mip_count {
                if mip > 0 {
                    let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
                    cur = Cow::Owned(resample(&cur, cw, ch, nw, nh, filter));
                    (cw, ch) = (nw, nh);
                }
                if fi == 0 && face == 0 && mip == low_mip {
                    low_src = Some(cur.to_vec());
                }
                out_levels.push(pack_level(format, &cur, cw, ch, params));
            }
            levels.push(out_levels);
        }
    }
    let mut low_rgba = match low_src {
        Some(v) => v,
        None => resample(&first.pixels, sw, sh, lw as usize, lh as usize, filter),
    };
    for p in low_rgba.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    let low_data = pack_level(
        ImageFormat::Dxt1,
        &low_rgba,
        lw as usize,
        lh as usize,
        texpresso_params(EncodeQuality::Fast, filter),
    );

    let hi_size: usize = levels.iter().flatten().map(Vec::len).sum();
    let minor = opts.version_minor;
    let header_size: usize = match minor {
        0 | 1 => 64,
        2 => 80,
        _ => 80 + 2 * 8,
    };
    let low_offset = header_size;
    let hi_offset = low_offset + low_data.len();
    let total = hi_offset + hi_size;
    let hi_offset_u32 = u32::try_from(hi_offset).map_err(|_| VtfError::TooLarge)?;
    u32::try_from(total).map_err(|_| VtfError::TooLarge)?;

    let first_frame = if faces == 6 && minor < 5 { 0xFFFF } else { opts.first_frame };
    let refl = opts.reflectivity.unwrap_or(base_reflectivity);

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&SIGNATURE);
    out.extend_from_slice(&7u32.to_le_bytes());
    out.extend_from_slice(&minor.to_le_bytes());
    out.extend_from_slice(&(header_size as u32).to_le_bytes());
    out.extend_from_slice(&(tw as u16).to_le_bytes());
    out.extend_from_slice(&(th as u16).to_le_bytes());
    out.extend_from_slice(&tex_flags.to_le_bytes());
    out.extend_from_slice(&(frame_count as u16).to_le_bytes());
    out.extend_from_slice(&first_frame.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    for c in refl {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&opts.bumpmap_scale.to_le_bytes());
    out.extend_from_slice(&format.raw().to_le_bytes());
    out.push(mip_count as u8);
    out.extend_from_slice(&ImageFormat::Dxt1.raw().to_le_bytes());
    out.push(lw as u8);
    out.push(lh as u8);
    if minor >= 2 {
        out.extend_from_slice(&1u16.to_le_bytes());
    }
    if minor >= 3 {
        out.extend_from_slice(&[0; 3]);
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&RESOURCE_LOW_RES);
        out.push(0);
        out.extend_from_slice(&(low_offset as u32).to_le_bytes());
        out.extend_from_slice(&RESOURCE_HIGH_RES);
        out.push(0);
        out.extend_from_slice(&hi_offset_u32.to_le_bytes());
    }
    out.resize(header_size, 0);
    out.extend_from_slice(&low_data);
    for mip in (0..mip_count as usize).rev() {
        for image_levels in &levels {
            if let Some(level) = image_levels.get(mip) {
                out.extend_from_slice(level);
            }
        }
    }
    Ok(out)
}
