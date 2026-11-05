use std::io::Cursor;

use crate::encode::{Filter, resample};
use crate::{RgbaImage, VtfError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ResizeFilter {
    Nearest,
    #[default]
    Color,
    Data,
    Normal,
}

impl ResizeFilter {
    pub const ALL: [ResizeFilter; 4] = [Self::Nearest, Self::Color, Self::Data, Self::Normal];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Nearest => "nearest",
            Self::Color => "color",
            Self::Data => "data",
            Self::Normal => "normal",
        }
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id().eq_ignore_ascii_case(id.trim()))
    }
}

pub fn resize(img: &RgbaImage, width: u32, height: u32, filter: ResizeFilter) -> Result<RgbaImage, VtfError> {
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err(VtfError::InvalidDimensions { width, height });
    }
    if (img.width, img.height) == (width, height) {
        return Ok(img.clone());
    }
    let (sw, sh, tw, th) = (img.width as usize, img.height as usize, width as usize, height as usize);
    let pixels = match filter {
        ResizeFilter::Nearest => {
            let src = img.pixels.as_chunks::<4>().0;
            let mut out = Vec::with_capacity(tw * th * 4);
            for y in 0..th {
                let sy = (y * sh / th).min(sh - 1);
                for x in 0..tw {
                    let sx = (x * sw / tw).min(sw - 1);
                    out.extend_from_slice(&src[sy * sw + sx]);
                }
            }
            out
        }
        ResizeFilter::Color => resample(&img.pixels, sw, sh, tw, th, Filter::Gamma),
        ResizeFilter::Data => resample(&img.pixels, sw, sh, tw, th, Filter::Linear),
        ResizeFilter::Normal => resample(&img.pixels, sw, sh, tw, th, Filter::Normal),
    };
    RgbaImage::new(width, height, pixels)
}

#[must_use]
pub fn next_pow2(v: u32) -> u32 {
    v.max(1).next_power_of_two()
}

#[must_use]
pub fn nearest_power_of_two(v: u32) -> u32 {
    let v = v.clamp(1, crate::MAX_POW2_SIZE);
    let up = v.next_power_of_two();
    let down = up >> 1;
    if down > 0 && v - down < up - v { down } else { up }
}

#[must_use]
pub fn is_pow2(v: u32) -> bool {
    v.is_power_of_two()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum HeightSource {
    #[default]
    Luminance,
    Red,
    Green,
    Blue,
    Alpha,
    Average,
}

impl HeightSource {
    pub const ALL: [HeightSource; 6] =
        [Self::Luminance, Self::Red, Self::Green, Self::Blue, Self::Alpha, Self::Average];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Luminance => "luminance",
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Alpha => "alpha",
            Self::Average => "average",
        }
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id().eq_ignore_ascii_case(id.trim()))
    }

    fn height(self, p: &[u8]) -> f32 {
        let v = match self {
            Self::Luminance => 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]),
            Self::Red => f32::from(p[0]),
            Self::Green => f32::from(p[1]),
            Self::Blue => f32::from(p[2]),
            Self::Alpha => f32::from(p[3]),
            Self::Average => (f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2])) / 3.0,
        };
        v / 255.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum NormalKernel {
    Simple,
    #[default]
    Sobel3,
    Sobel5,
}

impl NormalKernel {
    pub const ALL: [NormalKernel; 3] = [Self::Simple, Self::Sobel3, Self::Sobel5];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Sobel3 => "sobel3",
            Self::Sobel5 => "sobel5",
        }
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id().eq_ignore_ascii_case(id.trim()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum NormalAlpha {
    #[default]
    Opaque,
    Keep,
    Height,
}

impl NormalAlpha {
    pub const ALL: [NormalAlpha; 3] = [Self::Opaque, Self::Keep, Self::Height];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Keep => "keep",
            Self::Height => "height",
        }
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id().eq_ignore_ascii_case(id.trim()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalOptions {
    pub source: HeightSource,
    pub kernel: NormalKernel,
    pub scale: f32,
    pub wrap: bool,
    pub opengl: bool,
    pub alpha: NormalAlpha,
}

impl Default for NormalOptions {
    fn default() -> Self {
        Self {
            source: HeightSource::Luminance,
            kernel: NormalKernel::Sobel3,
            scale: 2.0,
            wrap: true,
            opengl: false,
            alpha: NormalAlpha::Opaque,
        }
    }
}

const SOBEL5_X: [[f32; 5]; 5] = [
    [-1.0, -2.0, 0.0, 2.0, 1.0],
    [-4.0, -8.0, 0.0, 8.0, 4.0],
    [-6.0, -12.0, 0.0, 12.0, 6.0],
    [-4.0, -8.0, 0.0, 8.0, 4.0],
    [-1.0, -2.0, 0.0, 2.0, 1.0],
];

pub fn normal_from_height(img: &RgbaImage, opts: &NormalOptions) -> Result<RgbaImage, VtfError> {
    if img.width == 0 || img.height == 0 {
        return Err(VtfError::InvalidDimensions { width: img.width, height: img.height });
    }
    let (w, h) = (img.width as i64, img.height as i64);
    let src = img.pixels.as_chunks::<4>().0;
    let heights: Vec<f32> = src.iter().map(|p| opts.source.height(p)).collect();
    let at = |x: i64, y: i64| -> f32 {
        let (x, y) =
            if opts.wrap { (x.rem_euclid(w), y.rem_euclid(h)) } else { (x.clamp(0, w - 1), y.clamp(0, h - 1)) };
        heights[(y * w + x) as usize]
    };
    let mut out = vec![0u8; src.len() * 4];
    for y in 0..h {
        for x in 0..w {
            let (gx, gy) = match opts.kernel {
                NormalKernel::Simple => ((at(x + 1, y) - at(x - 1, y)) * 0.5, (at(x, y + 1) - at(x, y - 1)) * 0.5),
                NormalKernel::Sobel3 => {
                    let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                        - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
                    let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                        - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
                    (gx / 8.0, gy / 8.0)
                }
                NormalKernel::Sobel5 => {
                    let (mut gx, mut gy) = (0.0f32, 0.0f32);
                    for (j, row) in SOBEL5_X.iter().enumerate() {
                        for (i, k) in row.iter().enumerate() {
                            let v = at(x + i as i64 - 2, y + j as i64 - 2);
                            gx += k * v;
                            gy += SOBEL5_X[i][j] * v;
                        }
                    }
                    (gx / 128.0, gy / 128.0)
                }
            };
            let nx = -gx * opts.scale;
            let ny = if opts.opengl { gy * opts.scale } else { -gy * opts.scale };
            let len = (nx * nx + ny * ny + 1.0).sqrt();
            let enc = |v: f32| ((v / len * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            let i = ((y * w + x) * 4) as usize;
            out[i] = enc(nx);
            out[i + 1] = enc(ny);
            out[i + 2] = enc(1.0);
            out[i + 3] = match opts.alpha {
                NormalAlpha::Opaque => 255,
                NormalAlpha::Keep => src[(y * w + x) as usize][3],
                NormalAlpha::Height => (heights[(y * w + x) as usize] * 255.0 + 0.5) as u8,
            };
        }
    }
    RgbaImage::new(img.width, img.height, out)
}

#[must_use]
pub fn reflectivity_of(img: &RgbaImage) -> [f32; 3] {
    crate::encode::reflectivity(&img.pixels)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaSource {
    Opaque,
    Constant(u8),
    Luminance,
    Red,
    Green,
    Blue,
    Invert,
}

impl AlphaSource {
    #[must_use]
    pub fn from_id(id: &str, value: u8) -> Option<Self> {
        Some(match id.trim().to_ascii_lowercase().as_str() {
            "opaque" => Self::Opaque,
            "constant" => Self::Constant(value),
            "luminance" => Self::Luminance,
            "red" => Self::Red,
            "green" => Self::Green,
            "blue" => Self::Blue,
            "invert" => Self::Invert,
            _ => return None,
        })
    }
}

pub fn set_alpha(img: &RgbaImage, source: AlphaSource) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels.as_chunks_mut::<4>().0 {
        p[3] = match source {
            AlphaSource::Opaque => 255,
            AlphaSource::Constant(v) => v,
            AlphaSource::Luminance => {
                ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114 + 500) / 1000) as u8
            }
            AlphaSource::Red => p[0],
            AlphaSource::Green => p[1],
            AlphaSource::Blue => p[2],
            AlphaSource::Invert => 255 - p[3],
        };
    }
    out
}

pub fn set_alpha_from_image(img: &RgbaImage, mask: &RgbaImage) -> Result<RgbaImage, VtfError> {
    let mask = resize(mask, img.width, img.height, ResizeFilter::Data)?;
    let mut out = img.clone();
    for (p, m) in out.pixels.as_chunks_mut::<4>().0.iter_mut().zip(mask.pixels.as_chunks::<4>().0) {
        p[3] = ((u32::from(m[0]) * 299 + u32::from(m[1]) * 587 + u32::from(m[2]) * 114 + 500) / 1000) as u8;
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channels {
    pub r: bool,
    pub g: bool,
    pub b: bool,
    pub a: bool,
}

impl Default for Channels {
    fn default() -> Self {
        Self { r: true, g: true, b: true, a: true }
    }
}

impl Channels {
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        let id = id.trim().to_ascii_lowercase();
        if id.is_empty() || !id.chars().all(|c| "rgba".contains(c)) {
            return None;
        }
        Some(Self { r: id.contains('r'), g: id.contains('g'), b: id.contains('b'), a: id.contains('a') })
    }
}

pub fn view_channels(img: &RgbaImage, ch: Channels) -> RgbaImage {
    let mut out = img.clone();
    let color = ch.r || ch.g || ch.b;
    for p in out.pixels.as_chunks_mut::<4>().0 {
        if !color {
            let a = if ch.a { p[3] } else { 0 };
            *p = [a, a, a, 255];
            continue;
        }
        if !ch.r {
            p[0] = 0;
        }
        if !ch.g {
            p[1] = 0;
        }
        if !ch.b {
            p[2] = 0;
        }
        if !ch.a {
            p[3] = 255;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFileFormat {
    Png,
    Tga,
    Bmp,
    Jpeg,
}

impl ImageFileFormat {
    pub const ALL: [ImageFileFormat; 4] = [Self::Png, Self::Tga, Self::Bmp, Self::Jpeg];

    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Tga => "tga",
            Self::Bmp => "bmp",
            Self::Jpeg => "jpg",
        }
    }

    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "tga" => Some(Self::Tga),
            "bmp" => Some(Self::Bmp),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }
}

pub fn encode_image_file(img: &RgbaImage, format: ImageFileFormat) -> Result<Vec<u8>, VtfError> {
    let buffer = image::RgbaImage::from_raw(img.width, img.height, img.pixels.clone())
        .ok_or_else(|| VtfError::InvalidImage("pixel buffer does not match the size".into()))?;
    let dynamic = image::DynamicImage::ImageRgba8(buffer);
    let (dynamic, fmt) = match format {
        ImageFileFormat::Png => (dynamic, image::ImageFormat::Png),
        ImageFileFormat::Tga => (dynamic, image::ImageFormat::Tga),
        ImageFileFormat::Bmp => (dynamic, image::ImageFormat::Bmp),
        ImageFileFormat::Jpeg => (image::DynamicImage::ImageRgb8(dynamic.to_rgb8()), image::ImageFormat::Jpeg),
    };
    let mut out = Cursor::new(Vec::new());
    dynamic.write_to(&mut out, fmt).map_err(|e| VtfError::InvalidImage(e.to_string()))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> RgbaImage {
        let px: Vec<u8> = (0..w * h).flat_map(|i| [((i % w) * 255 / w.max(1)) as u8, 40, 90, 255]).collect();
        RgbaImage::new(w, h, px).unwrap()
    }

    #[test]
    fn normal_map_of_a_ramp_leans_one_way() {
        let n =
            normal_from_height(&gradient(8, 8), &NormalOptions { wrap: false, ..NormalOptions::default() }).unwrap();
        let p = &n.pixels[4 * 8 * 3 + 4 * 3..][..4];
        assert!(p[0] < 120, "slope to the right tilts the normal left: {p:?}");
        assert!(p[1].abs_diff(128) <= 2, "{p:?}");
        assert!(p[2] > 200 && p[3] == 255);
        let flat =
            normal_from_height(&RgbaImage::new(4, 4, vec![100; 64]).unwrap(), &NormalOptions::default()).unwrap();
        assert_eq!(&flat.pixels[..4], &[128, 128, 255, 255]);
    }

    #[test]
    fn alpha_sources() {
        let img = RgbaImage::new(1, 1, vec![200, 100, 50, 10]).unwrap();
        assert_eq!(set_alpha(&img, AlphaSource::Opaque).pixels[3], 255);
        assert_eq!(set_alpha(&img, AlphaSource::Constant(77)).pixels[3], 77);
        assert_eq!(set_alpha(&img, AlphaSource::Red).pixels[3], 200);
        assert_eq!(set_alpha(&img, AlphaSource::Invert).pixels[3], 245);
        assert_eq!(AlphaSource::from_id("Constant", 5), Some(AlphaSource::Constant(5)));
    }

    #[test]
    fn channels_and_resize() {
        let img = RgbaImage::new(1, 1, vec![200, 100, 50, 10]).unwrap();
        let only_alpha = view_channels(&img, Channels::from_id("a").unwrap());
        assert_eq!(only_alpha.pixels, vec![10, 10, 10, 255]);
        let red = view_channels(&img, Channels::from_id("r").unwrap());
        assert_eq!(red.pixels, vec![200, 0, 0, 255]);
        let big = resize(&gradient(4, 4), 16, 8, ResizeFilter::Color).unwrap();
        assert_eq!((big.width, big.height), (16, 8));
        assert!(resize(&gradient(4, 4), 0, 8, ResizeFilter::Color).is_err());
        let near = resize(&gradient(4, 4), 2, 2, ResizeFilter::Nearest).unwrap();
        assert_eq!(near.pixels.len(), 16);
        assert_eq!(nearest_power_of_two(300), 256);
        assert_eq!(next_pow2(300), 512);
    }

    #[test]
    fn image_files_round_trip() {
        let img = gradient(4, 4);
        for f in ImageFileFormat::ALL {
            let bytes = encode_image_file(&img, f).unwrap();
            let back = crate::load_image_file(&bytes, f.extension()).unwrap();
            assert_eq!((back.width, back.height), (4, 4), "{f:?}");
        }
    }
}
