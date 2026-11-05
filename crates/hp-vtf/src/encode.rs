use std::io::Cursor;
use std::sync::LazyLock;

use rayon::prelude::*;
use texpresso::{Algorithm, COLOUR_WEIGHTS_PERCEPTUAL, COLOUR_WEIGHTS_UNIFORM, Params};

use crate::{ImageFormat, RgbaImage, VtfError, flags};

const GAMMA: f32 = 2.2;

pub const MAX_POW2_SIZE: u32 = 4096;

const MAX_IMPORT_DIM: u32 = 32_768;

const PARALLEL_PIXELS: usize = 128 * 128;

static GAMMA_TO_LINEAR: LazyLock<[f32; 256]> =
    LazyLock::new(|| std::array::from_fn(|i| (i as f32 / 255.0).powf(GAMMA)));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum EncodeFormat {
    #[default]
    Auto,
    Dxt1,
    Dxt5,
    Bgra8888,
    Bgr888,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum EncodeQuality {
    Fast,
    #[default]
    Normal,
    Best,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    pub format: EncodeFormat,
    pub mipmaps: bool,
    pub flags: u32,
    pub version_minor: u32,
    pub resize_to_pow2: bool,
    pub quality: EncodeQuality,
    pub mip_filter: MipFilter,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            format: EncodeFormat::Auto,
            mipmaps: true,
            flags: 0,
            version_minor: 4,
            resize_to_pow2: true,
            quality: EncodeQuality::Normal,
            mip_filter: MipFilter::FromFlags,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum MipFilter {
    #[default]
    FromFlags,
    Color,
    Data,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filter {
    Gamma,
    Linear,
    Normal,
}

pub(crate) fn filter_for(tex_flags: u32) -> Filter {
    if tex_flags & flags::NORMAL != 0 {
        Filter::Normal
    } else if tex_flags & flags::SSBUMP != 0 {
        Filter::Linear
    } else {
        Filter::Gamma
    }
}

#[inline]
fn linear_to_gamma8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0).powf(1.0 / GAMMA) * 255.0 + 0.5) as u8
}

#[inline]
fn unorm8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

struct AxisTaps {
    spans: Vec<(usize, usize)>,
    taps: Vec<(usize, f32)>,
}

impl AxisTaps {
    fn new(src: usize, dst: usize) -> Self {
        let mut spans = Vec::with_capacity(dst);
        let mut taps = Vec::new();
        let scale = src as f64 / dst as f64;
        let last = src.saturating_sub(1);
        for i in 0..dst {
            let start = taps.len();
            if scale >= 1.0 {
                let lo = i as f64 * scale;
                let hi = (i + 1) as f64 * scale;
                let mut j = lo.floor() as usize;
                while (j as f64) < hi && j < src {
                    let overlap = hi.min(j as f64 + 1.0) - lo.max(j as f64);
                    if overlap > 1e-9 {
                        taps.push((j, (overlap / scale) as f32));
                    }
                    j += 1;
                }
            } else {
                let center = ((i as f64 + 0.5) * scale - 0.5).max(0.0);
                let j0 = (center.floor() as usize).min(last);
                let j1 = (j0 + 1).min(last);
                let t = (center - j0 as f64).clamp(0.0, 1.0) as f32;
                taps.push((j0, 1.0 - t));
                if j1 != j0 && t > 0.0 {
                    taps.push((j1, t));
                }
            }
            if taps.len() == start {
                taps.push((i.min(last), 1.0));
            }
            let sum: f32 = taps.get(start..).map_or(0.0, |s| s.iter().map(|t| t.1).sum());
            if sum > 0.0
                && let Some(span) = taps.get_mut(start..)
            {
                for t in span {
                    t.1 /= sum;
                }
            }
            spans.push((start, taps.len()));
        }
        Self { spans, taps }
    }

    fn get(&self, i: usize) -> &[(usize, f32)] {
        self.spans.get(i).and_then(|&(a, b)| self.taps.get(a..b)).unwrap_or(&[])
    }
}

pub(crate) fn resample(src: &[u8], w: usize, h: usize, tw: usize, th: usize, filter: Filter) -> Vec<u8> {
    let px = src.as_chunks::<4>().0;
    let xt = AxisTaps::new(w, tw);
    let yt = AxisTaps::new(h, th);
    let lut = &*GAMMA_TO_LINEAR;
    let mut out = vec![0u8; tw * th * 4];
    let row = |(ty, dst): (usize, &mut [u8])| {
        let ytaps = yt.get(ty);
        for (tx, d) in dst.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let xtaps = xt.get(tx);
            let mut acc = [0f32; 4];
            let mut plain = [0f32; 3];
            for &(sy, wy) in ytaps {
                let line = sy * w;
                for &(sx, wx) in xtaps {
                    let Some(p) = px.get(line + sx) else { continue };
                    let wgt = wx * wy;
                    match filter {
                        Filter::Gamma => {
                            let a = f32::from(p[3]) * (1.0 / 255.0) * wgt;
                            for c in 0..3 {
                                let l = lut[usize::from(p[c])];
                                acc[c] += l * a;
                                plain[c] += l * wgt;
                            }
                            acc[3] += a;
                        }
                        Filter::Linear => {
                            for c in 0..4 {
                                acc[c] += f32::from(p[c]) * wgt;
                            }
                        }
                        Filter::Normal => {
                            for c in 0..3 {
                                acc[c] += (f32::from(p[c]) / 127.5 - 1.0) * wgt;
                            }
                            acc[3] += f32::from(p[3]) * wgt;
                        }
                    }
                }
            }
            *d = match filter {
                Filter::Gamma => {
                    let a = acc[3];
                    let rgb = if a > 1e-6 { [acc[0] / a, acc[1] / a, acc[2] / a] } else { plain };
                    [linear_to_gamma8(rgb[0]), linear_to_gamma8(rgb[1]), linear_to_gamma8(rgb[2]), unorm8(a)]
                }
                Filter::Linear => acc.map(|v| (v + 0.5).clamp(0.0, 255.0) as u8),
                Filter::Normal => {
                    let len = (acc[0] * acc[0] + acc[1] * acc[1] + acc[2] * acc[2]).sqrt();
                    let n = if len > 1e-6 { [acc[0] / len, acc[1] / len, acc[2] / len] } else { [0.0, 0.0, 1.0] };
                    let enc = |v: f32| unorm8(v * 0.5 + 0.5);
                    [enc(n[0]), enc(n[1]), enc(n[2]), (acc[3] + 0.5).clamp(0.0, 255.0) as u8]
                }
            };
        }
    };
    if tw * th >= PARALLEL_PIXELS {
        out.par_chunks_mut(tw * 4).enumerate().for_each(row);
    } else {
        out.chunks_mut(tw * 4).enumerate().for_each(row);
    }
    out
}

pub(crate) fn nearest_pow2(v: u32) -> u32 {
    if v >= MAX_POW2_SIZE {
        return MAX_POW2_SIZE;
    }
    if v <= 1 {
        return 1;
    }
    let up = v.next_power_of_two();
    let down = up >> 1;
    if v - down < up - v { down } else { up }
}

pub(crate) fn reflectivity(pixels: &[u8]) -> [f32; 3] {
    let lut = &*GAMMA_TO_LINEAR;
    let px = pixels.as_chunks::<4>().0;
    if px.is_empty() {
        return [0.0; 3];
    }
    let sum = px
        .par_chunks(16 * 1024)
        .map(|chunk| {
            let mut s = [0f64; 3];
            for p in chunk {
                for c in 0..3 {
                    s[c] += f64::from(lut[usize::from(p[c])]);
                }
            }
            s
        })
        .reduce(|| [0.0; 3], |a, b| [a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
    let n = px.len() as f64;
    sum.map(|s| (s / n) as f32)
}

pub(crate) fn texpresso_params(quality: EncodeQuality, filter: Filter) -> Params {
    Params {
        algorithm: match quality {
            EncodeQuality::Fast => Algorithm::RangeFit,
            EncodeQuality::Normal => Algorithm::ClusterFit,
            EncodeQuality::Best => Algorithm::IterativeClusterFit,
        },
        weights: if filter == Filter::Gamma { COLOUR_WEIGHTS_PERCEPTUAL } else { COLOUR_WEIGHTS_UNIFORM },
        weigh_colour_by_alpha: false,
    }
}

pub(crate) fn check_image(img: &RgbaImage) -> Result<(usize, usize), VtfError> {
    if img.width == 0 || img.height == 0 {
        return Err(VtfError::InvalidDimensions { width: img.width, height: img.height });
    }
    let expected = u64::from(img.width) * u64::from(img.height) * 4;
    if img.pixels.len() as u64 != expected {
        return Err(VtfError::InvalidImage(format!(
            "{}x{} image needs {expected} bytes, got {}",
            img.width,
            img.height,
            img.pixels.len()
        )));
    }
    Ok((img.width as usize, img.height as usize))
}

pub fn encode(img: &RgbaImage, opts: &EncodeOptions) -> Result<Vec<u8>, VtfError> {
    encode_frames(std::slice::from_ref(img), opts)
}

pub fn encode_frames(frames: &[RgbaImage], opts: &EncodeOptions) -> Result<Vec<u8>, VtfError> {
    let images: Vec<Vec<RgbaImage>> = frames.iter().map(|f| vec![f.clone()]).collect();
    let opaque = frames.iter().all(RgbaImage::is_opaque);
    let format = match opts.format {
        EncodeFormat::Auto if opaque => ImageFormat::Dxt1,
        EncodeFormat::Auto | EncodeFormat::Dxt5 => ImageFormat::Dxt5,
        EncodeFormat::Dxt1 => ImageFormat::Dxt1,
        EncodeFormat::Bgra8888 => ImageFormat::Bgra8888,
        EncodeFormat::Bgr888 => ImageFormat::Bgr888,
    };
    let compose_opts = crate::compose::ComposeOptions {
        format,
        mipmaps: opts.mipmaps,
        flags: opts.flags,
        version_minor: opts.version_minor,
        resize_to_pow2: opts.resize_to_pow2,
        quality: opts.quality,
        mip_filter: opts.mip_filter,
        ..crate::compose::ComposeOptions::default()
    };
    crate::compose::compose(&images, &compose_opts)
}

pub fn load_image_file(bytes: &[u8], ext_hint: &str) -> Result<RgbaImage, VtfError> {
    use image::ImageFormat as F;
    let supported = |f: &F| matches!(f, F::Png | F::Jpeg | F::Tga | F::Bmp);
    let ext = ext_hint.rsplit('.').next().unwrap_or(ext_hint).trim();
    let hinted = F::from_extension(ext).filter(supported);
    let guessed = image::guess_format(bytes).ok().filter(supported);
    let format = match (hinted, guessed) {
        (Some(F::Tga), _) => F::Tga,
        (h, g) => g.or(h).ok_or_else(|| {
            VtfError::ImageDecode(format!("unsupported image type (hint '{ext_hint}'); expected PNG, JPG, TGA or BMP"))
        })?,
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMPORT_DIM);
    limits.max_image_height = Some(MAX_IMPORT_DIM);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| VtfError::ImageDecode(e.to_string()))?.into_rgba8();
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 {
        return Err(VtfError::InvalidDimensions { width, height });
    }
    Ok(RgbaImage { width, height, pixels: decoded.into_raw() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow2_rounding() {
        assert_eq!(nearest_pow2(1), 1);
        assert_eq!(nearest_pow2(3), 4);
        assert_eq!(nearest_pow2(300), 256);
        assert_eq!(nearest_pow2(384), 512);
        assert_eq!(nearest_pow2(400), 512);
        assert_eq!(nearest_pow2(512), 512);
        assert_eq!(nearest_pow2(5000), 4096);
    }

    #[test]
    fn taps_sum_to_one() {
        for (s, d) in [(4, 2), (3, 1), (5, 4), (2, 3), (300, 256), (7, 7), (1, 4)] {
            let t = AxisTaps::new(s, d);
            for i in 0..d {
                let sum: f32 = t.get(i).iter().map(|x| x.1).sum();
                assert!((sum - 1.0).abs() < 1e-4, "{s}->{d} at {i}: {sum}");
                assert!(t.get(i).iter().all(|x| x.0 < s));
            }
        }
    }

    #[test]
    fn gamma_mip_of_black_and_white_is_bright() {
        let src = [0, 0, 0, 255, 255, 255, 255, 255];
        let out = resample(&src, 2, 1, 1, 1, Filter::Gamma);
        assert!((185..=187).contains(&out[0]), "{}", out[0]);
        assert_eq!(out[3], 255);
    }

    #[test]
    fn transparent_pixels_do_not_bleed() {
        let src = [255, 0, 0, 255, 0, 0, 0, 0];
        let out = resample(&src, 2, 1, 1, 1, Filter::Gamma);
        assert_eq!(&out[..3], &[255, 0, 0]);
        assert_eq!(out[3], 128);
    }

    #[test]
    fn normal_maps_are_renormalized() {
        let src = [204, 128, 230, 255, 51, 128, 230, 255];
        let out = resample(&src, 2, 1, 1, 1, Filter::Normal);
        assert!(out[0].abs_diff(128) <= 1, "{out:?}");
        assert!(out[2] >= 254, "{out:?}");
    }

    #[test]
    fn reflectivity_is_linear_average() {
        let px = [255, 0, 0, 255, 0, 0, 0, 255];
        let r = reflectivity(&px);
        assert!((r[0] - 0.5).abs() < 1e-6);
        assert_eq!(r[1], 0.0);
    }
}
