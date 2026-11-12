use std::path::Path;

use hp_geom::HeightField;
use image::{DynamicImage, ImageBuffer, Luma, Rgba};

pub const MAX_IMAGE_SIDE: u32 = 16384;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RawFormat {
    #[default]
    Auto,
    U8,
    U16,
    F32,
}

impl RawFormat {
    pub const NAMES: [&'static str; 4] = ["auto", "8", "16", "32f"];

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::U8 => "8",
            Self::U16 => "16",
            Self::F32 => "32f",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Auto, Self::U8, Self::U16, Self::F32].into_iter().find(|f| f.name() == name)
    }

    fn bytes(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
            Self::F32 => 4,
            Self::Auto => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawSpec {
    pub width: u32,
    pub height: u32,
    pub big_endian: bool,
    pub format: RawFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    R,
    G,
    B,
    A,
    Luma,
}

impl Channel {
    pub const ALL: [Self; 5] = [Self::R, Self::G, Self::B, Self::A, Self::Luma];

    pub fn name(self) -> &'static str {
        match self {
            Self::R => "r",
            Self::G => "g",
            Self::B => "b",
            Self::A => "a",
            Self::Luma => "l",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "r" | "red" => Some(Self::R),
            "g" | "green" => Some(Self::G),
            "b" | "blue" => Some(Self::B),
            "a" | "alpha" => Some(Self::A),
            "l" | "luma" | "gray" | "grey" | "y" => Some(Self::Luma),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pixels {
    pub width: usize,
    pub height: usize,
    pub channels: usize,
    pub data: Vec<f64>,
    pub float: bool,
    pub bits: u8,
}

impl Pixels {
    fn new(
        width: usize,
        height: usize,
        channels: usize,
        data: Vec<f64>,
        float: bool,
        bits: u8,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("the image is empty".to_owned());
        }
        if width > MAX_IMAGE_SIDE as usize || height > MAX_IMAGE_SIDE as usize {
            return Err(format!("the image is {width} x {height}, the limit is {MAX_IMAGE_SIDE} per side"));
        }
        if !(1..=4).contains(&channels) || data.len() != width * height * channels {
            return Err("the image data does not match its size".to_owned());
        }
        Ok(Self { width, height, channels, data, float, bits })
    }

    pub fn has_alpha(&self) -> bool {
        self.channels == 2 || self.channels == 4
    }

    pub fn is_gray(&self) -> bool {
        if self.channels <= 2 {
            return true;
        }
        self.data.chunks_exact(self.channels).all(|p| p.first() == p.get(1) && p.get(1) == p.get(2))
    }

    pub fn describe(&self) -> String {
        let kind = if self.float { format!("{} bit float", self.bits) } else { format!("{} bit", self.bits) };
        let channels = match self.channels {
            1 => "gray",
            2 => "gray + alpha",
            3 => "RGB",
            _ => "RGBA",
        };
        format!("{} x {}, {kind} {channels}", self.width, self.height)
    }

    fn plane(&self, f: impl Fn(&[f64]) -> f64) -> HeightField {
        let data: Vec<f64> = self.data.chunks_exact(self.channels).map(f).collect();
        HeightField::new(self.width, self.height, data).unwrap_or_else(|| HeightField::filled(1, 1, 0.0))
    }

    pub fn channel(&self, ch: Channel) -> HeightField {
        let get = |p: &[f64], i: usize| p.get(i).copied().unwrap_or(0.0);
        match (ch, self.channels) {
            (Channel::A, 2) => self.plane(|p| get(p, 1)),
            (Channel::A, 4) => self.plane(|p| get(p, 3)),
            (Channel::A, _) => self.plane(|_| 0.0),
            (Channel::Luma, _) => self.gray(),
            (_, 1 | 2) => self.plane(|p| get(p, 0)),
            (Channel::R, _) => self.plane(|p| get(p, 0)),
            (Channel::G, _) => self.plane(|p| get(p, 1)),
            (Channel::B, _) => self.plane(|p| get(p, 2)),
        }
    }

    pub fn gray(&self) -> HeightField {
        if self.is_gray() {
            return self.plane(|p| p.first().copied().unwrap_or(0.0));
        }
        self.plane(|p| {
            let c = |i: usize| p.get(i).copied().unwrap_or(0.0);
            0.2126 * c(0) + 0.7152 * c(1) + 0.0722 * c(2)
        })
    }
}

pub fn is_raw(path: &Path) -> bool {
    ext(path).is_some_and(|e| matches!(e.as_str(), "raw" | "r16" | "r32" | "r8" | "f32" | "bin" | "data"))
}

fn ext(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
}

fn err(path: &Path, e: impl std::fmt::Display) -> String {
    format!("{}: {e}", path.display())
}

fn from_dynamic(img: &DynamicImage) -> Result<Pixels, String> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let n8 = |v: u8| f64::from(v) / 255.0;
    let n16 = |v: u16| f64::from(v) / 65535.0;
    let (channels, data, float, bits) = match img {
        DynamicImage::ImageLuma8(b) => (1, b.as_raw().iter().map(|v| n8(*v)).collect(), false, 8),
        DynamicImage::ImageLumaA8(b) => (2, b.as_raw().iter().map(|v| n8(*v)).collect(), false, 8),
        DynamicImage::ImageRgb8(b) => (3, b.as_raw().iter().map(|v| n8(*v)).collect(), false, 8),
        DynamicImage::ImageRgba8(b) => (4, b.as_raw().iter().map(|v| n8(*v)).collect(), false, 8),
        DynamicImage::ImageLuma16(b) => (1, b.as_raw().iter().map(|v| n16(*v)).collect(), false, 16),
        DynamicImage::ImageLumaA16(b) => (2, b.as_raw().iter().map(|v| n16(*v)).collect(), false, 16),
        DynamicImage::ImageRgb16(b) => (3, b.as_raw().iter().map(|v| n16(*v)).collect(), false, 16),
        DynamicImage::ImageRgba16(b) => (4, b.as_raw().iter().map(|v| n16(*v)).collect(), false, 16),
        DynamicImage::ImageRgb32F(b) => (3, b.as_raw().iter().map(|v| f64::from(*v)).collect(), true, 32),
        DynamicImage::ImageRgba32F(b) => (4, b.as_raw().iter().map(|v| f64::from(*v)).collect(), true, 32),
        other => {
            let b = other.to_rgba16();
            (4, b.as_raw().iter().map(|v| n16(*v)).collect(), false, 16)
        }
    };
    Pixels::new(w, h, channels, data, float, bits)
}

fn read_generic(path: &Path) -> Result<Pixels, String> {
    let reader = image::ImageReader::open(path).map_err(|e| err(path, e))?;
    let reader = reader.with_guessed_format().map_err(|e| err(path, e))?;
    let (w, h) = reader.into_dimensions().map_err(|e| err(path, e))?;
    if w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE {
        return Err(err(path, format!("the image is {w} x {h}, the limit is {MAX_IMAGE_SIDE} per side")));
    }
    let img = image::open(path).map_err(|e| err(path, e))?;
    from_dynamic(&img).map_err(|e| err(path, e))
}

fn read_tiff(path: &Path) -> Result<Pixels, String> {
    use tiff::decoder::{Decoder, DecodingResult, Limits};
    let file = std::fs::File::open(path).map_err(|e| err(path, e))?;
    let mut dec =
        Decoder::new(std::io::BufReader::new(file)).map_err(|e| err(path, e))?.with_limits(Limits::unlimited());
    let (w, h) = dec.dimensions().map_err(|e| err(path, e))?;
    if w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE {
        return Err(err(path, format!("the image is {w} x {h}, the limit is {MAX_IMAGE_SIDE} per side")));
    }
    let channels = match dec.colortype().map_err(|e| err(path, e))? {
        tiff::ColorType::Gray(_) => 1,
        tiff::ColorType::GrayA(_) => 2,
        tiff::ColorType::RGB(_) => 3,
        tiff::ColorType::RGBA(_) => 4,
        other => return Err(err(path, format!("the TIFF color type {other:?} is not supported"))),
    };
    let (data, float, bits): (Vec<f64>, bool, u8) = match dec.read_image().map_err(|e| err(path, e))? {
        DecodingResult::U8(v) => (v.iter().map(|x| f64::from(*x) / 255.0).collect(), false, 8),
        DecodingResult::U16(v) => (v.iter().map(|x| f64::from(*x) / 65535.0).collect(), false, 16),
        DecodingResult::U32(v) => (v.iter().map(|x| f64::from(*x) / f64::from(u32::MAX)).collect(), false, 32),
        DecodingResult::I16(v) => (v.iter().map(|x| f64::from(*x)).collect(), true, 16),
        DecodingResult::I32(v) => (v.iter().map(|x| f64::from(*x)).collect(), true, 32),
        DecodingResult::F16(v) => (v.iter().map(|x| f64::from(x.to_f32())).collect(), true, 16),
        DecodingResult::F32(v) => (v.iter().map(|x| f64::from(*x)).collect(), true, 32),
        DecodingResult::F64(v) => (v, true, 64),
        _ => return Err(err(path, "this TIFF sample format is not supported")),
    };
    Pixels::new(w as usize, h as usize, channels, data, float, bits).map_err(|e| err(path, e))
}

fn read_exr(path: &Path) -> Result<Pixels, String> {
    use exr::prelude::*;
    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .first_valid_layer()
        .all_attributes()
        .from_file(path)
        .map_err(|e| err(path, e))?;
    let layer = image.layer_data;
    let (w, h) = (layer.size.0, layer.size.1);
    let list = &layer.channel_data.list;
    let named = |want: &str| {
        list.iter().position(|c| {
            let name = c.name.to_string();
            let short = name.rsplit('.').next().unwrap_or(&name).to_ascii_uppercase();
            short == want
        })
    };
    let values = |i: usize| -> Vec<f64> {
        match list.get(i).map(|c| &c.sample_data) {
            Some(FlatSamples::F16(v)) => v.iter().map(|x| f64::from(x.to_f32())).collect(),
            Some(FlatSamples::F32(v)) => v.iter().map(|x| f64::from(*x)).collect(),
            Some(FlatSamples::U32(v)) => v.iter().map(|x| f64::from(*x)).collect(),
            None => Vec::new(),
        }
    };
    let rgb = [named("R"), named("G"), named("B")];
    let alpha = named("A");
    let planes: Vec<Vec<f64>> = if rgb.iter().any(Option::is_some) {
        let mut p: Vec<Vec<f64>> = rgb.iter().map(|c| c.map(&values).unwrap_or_else(|| vec![0.0; w * h])).collect();
        if let Some(a) = alpha {
            p.push(values(a));
        }
        p
    } else {
        let gray = named("Y").or_else(|| named("L")).or_else(|| named("Z")).or_else(|| (!list.is_empty()).then_some(0));
        let Some(g) = gray else { return Err(err(path, "the EXR file has no channel")) };
        let mut p = vec![values(g)];
        if let Some(a) = alpha.filter(|a| *a != g) {
            p.push(values(a));
        }
        p
    };
    let channels = planes.len();
    let mut data = Vec::with_capacity(w * h * channels);
    for k in 0..w * h {
        for plane in &planes {
            data.push(plane.get(k).copied().unwrap_or(0.0));
        }
    }
    Pixels::new(w, h, channels, data, true, 32).map_err(|e| err(path, e))
}

fn read_pfm(path: &Path) -> Result<Pixels, String> {
    let bytes = std::fs::read(path).map_err(|e| err(path, e))?;
    let mut pos = 0usize;
    let mut token = || -> Option<String> {
        while bytes.get(pos).is_some_and(u8::is_ascii_whitespace) {
            pos += 1;
        }
        let start = pos;
        while bytes.get(pos).is_some_and(|b| !b.is_ascii_whitespace()) {
            pos += 1;
        }
        let t = bytes.get(start..pos).map(|s| String::from_utf8_lossy(s).into_owned());
        pos += 1;
        t.filter(|t| !t.is_empty())
    };
    let bad = || err(path, "not a PFM file");
    let magic = token().ok_or_else(bad)?;
    let channels = match magic.as_str() {
        "Pf" => 1,
        "PF" => 3,
        _ => return Err(bad()),
    };
    let w: usize = token().and_then(|t| t.parse().ok()).ok_or_else(bad)?;
    let h: usize = token().and_then(|t| t.parse().ok()).ok_or_else(bad)?;
    let scale: f64 = token().and_then(|t| t.parse().ok()).filter(|s: &f64| *s != 0.0).ok_or_else(bad)?;
    if w == 0 || h == 0 || w > MAX_IMAGE_SIDE as usize || h > MAX_IMAGE_SIDE as usize {
        return Err(err(path, format!("the PFM size {w} x {h} is not supported")));
    }
    let body = bytes.get(pos..).unwrap_or(&[]);
    let need = w * h * channels * 4;
    if body.len() < need {
        return Err(err(path, format!("the PFM data is {} bytes, {need} are needed", body.len())));
    }
    let little = scale < 0.0;
    let mut data = vec![0.0; w * h * channels];
    for (k, c) in body.chunks_exact(4).take(w * h * channels).enumerate() {
        let quad = [c[0], c[1], c[2], c[3]];
        let v = if little { f32::from_le_bytes(quad) } else { f32::from_be_bytes(quad) };
        let (row, rest) = (k / (w * channels), k % (w * channels));
        let target = (h - 1 - row) * w * channels + rest;
        if let Some(slot) = data.get_mut(target) {
            *slot = f64::from(v);
        }
    }
    Pixels::new(w, h, channels, data, true, 32).map_err(|e| err(path, e))
}

fn raw_candidates(path: &Path, spec: &RawSpec) -> Vec<RawFormat> {
    if spec.format != RawFormat::Auto {
        return vec![spec.format];
    }
    match ext(path).as_deref() {
        Some("r32" | "f32") => vec![RawFormat::F32],
        Some("r16") => vec![RawFormat::U16],
        Some("r8") => vec![RawFormat::U8],
        _ => vec![RawFormat::U16, RawFormat::F32, RawFormat::U8],
    }
}

pub fn raw_layout(path: &Path, len: usize, spec: &RawSpec) -> Result<(usize, usize, RawFormat), String> {
    let candidates = raw_candidates(path, spec);
    let (w, h) = (spec.width as usize, spec.height as usize);
    for format in &candidates {
        let bytes = format.bytes();
        if bytes == 0 || !len.is_multiple_of(bytes) {
            continue;
        }
        let pixels = len / bytes;
        let size = match (w, h) {
            (0, 0) => {
                let side = (pixels as f64).sqrt().round() as usize;
                (side > 0 && side * side == pixels).then_some((side, side))
            }
            (w, 0) if pixels.is_multiple_of(w) => Some((w, pixels / w)),
            (0, h) if pixels.is_multiple_of(h) => Some((pixels / h, h)),
            (w, h) if w * h == pixels => Some((w, h)),
            _ => None,
        };
        if let Some((w, h)) = size {
            if w > MAX_IMAGE_SIDE as usize || h > MAX_IMAGE_SIDE as usize {
                return Err(format!("the RAW size {w} x {h} is over the limit of {MAX_IMAGE_SIDE} per side"));
            }
            return Ok((w, h, *format));
        }
    }
    let kinds: Vec<&str> = candidates.iter().map(|f| f.name()).collect();
    Err(if w == 0 && h == 0 {
        format!(
            "{}: {len} bytes is not a square {} bit image, give the width and the height",
            path.display(),
            kinds.join(" or ")
        )
    } else {
        format!("{}: {w} x {h} does not match the {len} bytes of the file ({} bit)", path.display(), kinds.join(" or "))
    })
}

fn read_raw(path: &Path, spec: &RawSpec) -> Result<Pixels, String> {
    let bytes = std::fs::read(path).map_err(|e| err(path, e))?;
    let (w, h, format) = raw_layout(path, bytes.len(), spec)?;
    let be = spec.big_endian;
    let data: Vec<f64> = match format {
        RawFormat::U8 => bytes.iter().map(|b| f64::from(*b) / 255.0).collect(),
        RawFormat::U16 => bytes
            .chunks_exact(2)
            .map(|c| {
                let pair = [c[0], c[1]];
                f64::from(if be { u16::from_be_bytes(pair) } else { u16::from_le_bytes(pair) }) / 65535.0
            })
            .collect(),
        RawFormat::F32 | RawFormat::Auto => bytes
            .chunks_exact(4)
            .map(|c| {
                let quad = [c[0], c[1], c[2], c[3]];
                f64::from(if be { f32::from_be_bytes(quad) } else { f32::from_le_bytes(quad) })
            })
            .collect(),
    };
    let (float, bits) = match format {
        RawFormat::U8 => (false, 8),
        RawFormat::U16 => (false, 16),
        _ => (true, 32),
    };
    Pixels::new(w, h, 1, data, float, bits).map_err(|e| err(path, e))
}

pub fn read_pixels(path: &Path, raw: &RawSpec) -> Result<Pixels, String> {
    if !path.is_file() {
        return Err(err(path, "the file does not exist"));
    }
    if is_raw(path) {
        return read_raw(path, raw);
    }
    match ext(path).as_deref() {
        Some("pfm") => read_pfm(path),
        Some("exr") => read_exr(path).or_else(|e| read_generic(path).map_err(|_| e)),
        Some("tif" | "tiff") => read_tiff(path).or_else(|e| read_generic(path).map_err(|_| e)),
        _ => read_generic(path),
    }
}

pub fn auto_normalize(data: &mut [f64]) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in data.iter_mut() {
        if !v.is_finite() {
            *v = 0.0;
        }
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    if lo >= 0.0 && hi <= 1.0 {
        return;
    }
    let span = hi - lo;
    for v in data.iter_mut() {
        *v = if span > 1e-12 { (*v - lo) / span } else { 0.0 };
    }
}

pub fn finite_field(mut field: HeightField) -> HeightField {
    for v in &mut field.data {
        if !v.is_finite() {
            *v = 0.0;
        }
    }
    field
}

pub fn load_heightmap(path: &Path, raw: &RawSpec) -> Result<HeightField, String> {
    let px = read_pixels(path, raw)?;
    let mut field = finite_field(px.gray());
    if px.float {
        auto_normalize(&mut field.data);
    }
    Ok(field)
}

pub fn load_gray(path: &Path) -> Result<HeightField, String> {
    load_heightmap(path, &RawSpec::default())
}

pub fn load_splat_rgba(path: &Path) -> Result<[HeightField; 4], String> {
    let px = read_pixels(path, &RawSpec::default())?;
    Ok([Channel::R, Channel::G, Channel::B, Channel::A].map(|c| finite_field(px.channel(c))))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutFormat {
    Png16,
    Tiff16,
    TiffF32,
    Exr,
    Pfm,
    Raw16,
    RawF32,
}

impl OutFormat {
    pub fn is_float(self) -> bool {
        matches!(self, Self::TiffF32 | Self::Exr | Self::Pfm | Self::RawF32)
    }

    pub fn for_path(path: &Path, float: bool) -> Result<Self, String> {
        let e = ext(path).unwrap_or_default();
        Ok(match (e.as_str(), float) {
            ("png", false) => Self::Png16,
            ("png", true) => return Err("PNG has no float format, use exr, tif, pfm or r32".to_owned()),
            ("tif" | "tiff", false) => Self::Tiff16,
            ("tif" | "tiff", true) => Self::TiffF32,
            ("exr", _) => Self::Exr,
            ("pfm", _) => Self::Pfm,
            ("r32" | "f32", _) => Self::RawF32,
            ("raw" | "r16", false) => Self::Raw16,
            ("raw", true) => Self::RawF32,
            ("r16", true) => return Err("r16 is 16 bit, use r32 for float values".to_owned()),
            (other, _) => {
                return Err(format!("images are written as png, tif, exr, pfm, r16, r32 or raw, not .{other}"));
            }
        })
    }
}

fn q16(v: f64) -> u16 {
    if v.is_finite() { (v.clamp(0.0, 1.0) * 65535.0).round() as u16 } else { 0 }
}

fn f32s(values: &[f64]) -> Vec<f32> {
    values.iter().map(|v| if v.is_finite() { *v as f32 } else { 0.0 }).collect()
}

fn write_tiff_f32(path: &Path, width: usize, height: usize, channels: usize, data: &[f32]) -> Result<(), String> {
    use tiff::encoder::{TiffEncoder, colortype};
    let file = std::fs::File::create(path).map_err(|e| err(path, e))?;
    let mut enc = TiffEncoder::new(std::io::BufWriter::new(file)).map_err(|e| err(path, e))?;
    let (w, h) = (width as u32, height as u32);
    match channels {
        1 => enc.write_image::<colortype::Gray32Float>(w, h, data),
        4 => enc.write_image::<colortype::RGBA32Float>(w, h, data),
        _ => enc.write_image::<colortype::RGB32Float>(w, h, data),
    }
    .map_err(|e| err(path, e))
}

fn write_exr(path: &Path, width: usize, height: usize, planes: &[(&str, Vec<f32>)]) -> Result<(), String> {
    use exr::prelude::*;
    let list: SmallVec<[AnyChannel<FlatSamples>; 4]> =
        planes.iter().map(|(name, v)| AnyChannel::new(*name, FlatSamples::F32(v.clone()))).collect();
    let layer =
        Layer::new((width, height), LayerAttributes::default(), Encoding::SMALL_LOSSLESS, AnyChannels::sort(list));
    Image::from_layer(layer).write().to_file(path).map_err(|e| err(path, e))
}

fn write_pfm(path: &Path, width: usize, height: usize, channels: usize, data: &[f32]) -> Result<(), String> {
    let magic = if channels == 1 { "Pf" } else { "PF" };
    let mut out = format!("{magic}\n{width} {height}\n-1.0\n").into_bytes();
    let row = width * channels;
    for j in (0..height).rev() {
        for v in data.get(j * row..(j + 1) * row).unwrap_or(&[]) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(path, out).map_err(|e| err(path, e))
}

pub fn write_gray(path: &Path, width: usize, height: usize, values: &[f64], format: OutFormat) -> Result<(), String> {
    if values.len() != width * height || width == 0 || height == 0 {
        return Err("the image size is wrong".to_owned());
    }
    match format {
        OutFormat::Png16 | OutFormat::Tiff16 => {
            let buffer: ImageBuffer<Luma<u16>, Vec<u16>> =
                ImageBuffer::from_raw(width as u32, height as u32, values.iter().map(|v| q16(*v)).collect())
                    .ok_or("the image size is wrong")?;
            let f = if format == OutFormat::Png16 { image::ImageFormat::Png } else { image::ImageFormat::Tiff };
            buffer.save_with_format(path, f).map_err(|e| err(path, e))
        }
        OutFormat::Raw16 => {
            let bytes: Vec<u8> = values.iter().flat_map(|v| q16(*v).to_le_bytes()).collect();
            std::fs::write(path, bytes).map_err(|e| err(path, e))
        }
        OutFormat::RawF32 => {
            let bytes: Vec<u8> = f32s(values).iter().flat_map(|v| v.to_le_bytes()).collect();
            std::fs::write(path, bytes).map_err(|e| err(path, e))
        }
        OutFormat::TiffF32 => write_tiff_f32(path, width, height, 1, &f32s(values)),
        OutFormat::Exr => write_exr(path, width, height, &[("Y", f32s(values))]),
        OutFormat::Pfm => write_pfm(path, width, height, 1, &f32s(values)),
    }
}

pub fn write_rgba(
    path: &Path,
    width: usize,
    height: usize,
    values: &[[f64; 4]],
    format: OutFormat,
) -> Result<(), String> {
    if values.len() != width * height || width == 0 || height == 0 {
        return Err("the image size is wrong".to_owned());
    }
    match format {
        OutFormat::Png16 | OutFormat::Tiff16 => {
            let raw: Vec<u16> = values.iter().flat_map(|p| p.map(q16)).collect();
            let buffer: ImageBuffer<Rgba<u16>, Vec<u16>> =
                ImageBuffer::from_raw(width as u32, height as u32, raw).ok_or("the image size is wrong")?;
            let f = if format == OutFormat::Png16 { image::ImageFormat::Png } else { image::ImageFormat::Tiff };
            buffer.save_with_format(path, f).map_err(|e| err(path, e))
        }
        OutFormat::TiffF32 => {
            let flat: Vec<f64> = values.iter().flat_map(|p| *p).collect();
            write_tiff_f32(path, width, height, 4, &f32s(&flat))
        }
        OutFormat::Exr => {
            let plane = |k: usize| f32s(&values.iter().map(|p| p[k]).collect::<Vec<f64>>());
            write_exr(path, width, height, &[("R", plane(0)), ("G", plane(1)), ("B", plane(2)), ("A", plane(3))])
        }
        OutFormat::Pfm | OutFormat::Raw16 | OutFormat::RawF32 => {
            Err("four channel masks are written as png, tif or exr; use one file per texture for pfm or raw".to_owned())
        }
    }
}

pub fn write_heights(path: &Path, width: usize, height: usize, values: &[u16]) -> Result<(), String> {
    let format = OutFormat::for_path(path, false)?;
    if format.is_float() {
        return Err("16 bit heights are written as png, tif or raw".to_owned());
    }
    let unit: Vec<f64> = values.iter().map(|v| f64::from(*v) / 65535.0).collect();
    write_gray(path, width, height, &unit, format)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("hp_terrain_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sixteen_bit_precision_survives_a_png_round_trip() {
        let d = dir("png16");
        let values: Vec<u16> = (0..64u32).map(|i| (i * 1023 + 7) as u16).collect();
        let path = d.join("h.png");
        write_heights(&path, 8, 8, &values).unwrap();
        let f = load_heightmap(&path, &RawSpec::default()).unwrap();
        assert_eq!((f.width, f.height), (8, 8));
        for (v, expect) in f.data.iter().zip(&values) {
            assert_eq!((v * 65535.0).round() as u16, *expect);
        }
    }

    #[test]
    fn eight_bit_gray_and_rgb_load_as_luminance() {
        let d = dir("rgb8");
        let gray: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(4, 2, |x, _| Luma([(x * 85) as u8]));
        gray.save(d.join("g.png")).unwrap();
        let f = load_heightmap(&d.join("g.png"), &RawSpec::default()).unwrap();
        assert_eq!(f.data[0], 0.0);
        assert!((f.data[3] - 1.0).abs() < 1e-9);
        let rgb: ImageBuffer<image::Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(2, 2, |x, _| if x == 0 { image::Rgb([0, 0, 0]) } else { image::Rgb([255, 255, 255]) });
        rgb.save(d.join("c.png")).unwrap();
        let f = load_heightmap(&d.join("c.png"), &RawSpec::default()).unwrap();
        assert_eq!(f.data[0], 0.0);
        assert!((f.data[1] - 1.0).abs() < 1e-3);
        let red: ImageBuffer<image::Rgb<u8>, Vec<u8>> = ImageBuffer::from_pixel(1, 1, image::Rgb([255, 0, 0]));
        red.save(d.join("r.png")).unwrap();
        let f = load_heightmap(&d.join("r.png"), &RawSpec::default()).unwrap();
        assert!((f.data[0] - 0.2126).abs() < 1e-9);
    }

    #[test]
    fn raw_files_read_both_byte_orders_and_check_the_size() {
        let d = dir("raw");
        let values: Vec<u16> = vec![0, 65535, 32768, 1];
        let le: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let be: Vec<u8> = values.iter().flat_map(|v| v.to_be_bytes()).collect();
        std::fs::write(d.join("a.raw"), le).unwrap();
        std::fs::write(d.join("b.r16"), be).unwrap();
        let a = load_heightmap(&d.join("a.raw"), &RawSpec::default()).unwrap();
        assert_eq!((a.width, a.height), (2, 2));
        assert_eq!(a.data[1], 1.0);
        let spec = RawSpec { width: 4, height: 1, big_endian: true, ..RawSpec::default() };
        let b = load_heightmap(&d.join("b.r16"), &spec).unwrap();
        assert_eq!((b.width, b.height), (4, 1));
        assert!((b.data[2] - 32768.0 / 65535.0).abs() < 1e-9);
        assert!(load_heightmap(&d.join("a.raw"), &RawSpec { width: 3, height: 3, ..RawSpec::default() }).is_err());
        let wide = RawSpec { width: 4, ..RawSpec::default() };
        let w = read_pixels(&d.join("a.raw"), &wide).unwrap();
        assert_eq!((w.width, w.height), (4, 1));
    }

    #[test]
    fn r32_float_raw_keeps_every_bit_and_its_size_is_inferred() {
        let d = dir("r32");
        let values: Vec<f32> = (0..9 * 9).map(|k| -12.375 + k as f32 * 0.123_456_7).collect();
        let le: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let be: Vec<u8> = values.iter().flat_map(|v| v.to_be_bytes()).collect();
        std::fs::write(d.join("h.r32"), le).unwrap();
        std::fs::write(d.join("b.r32"), be).unwrap();
        let px = read_pixels(&d.join("h.r32"), &RawSpec::default()).unwrap();
        assert_eq!((px.width, px.height, px.float, px.bits), (9, 9, true, 32));
        for (a, b) in px.data.iter().zip(&values) {
            assert_eq!(*a, f64::from(*b));
        }
        let px = read_pixels(&d.join("b.r32"), &RawSpec { big_endian: true, ..RawSpec::default() }).unwrap();
        assert_eq!(px.data[80], f64::from(values[80]));
        std::fs::write(d.join("odd.raw"), vec![0u8; 9 * 9 * 4]).unwrap();
        let px = read_pixels(&d.join("odd.raw"), &RawSpec::default()).unwrap();
        assert_eq!((px.width, px.float), (9, true), "81 floats is the only square layout");
        let forced = RawSpec { format: RawFormat::U8, ..RawSpec::default() };
        assert_eq!(read_pixels(&d.join("odd.raw"), &forced).unwrap().width, 18);
    }

    #[test]
    fn float_formats_round_trip_exactly() {
        let d = dir("floats");
        let (w, h) = (5usize, 3usize);
        let values: Vec<f64> = (0..w * h).map(|k| f64::from(-300.25f32 + k as f32 * 77.031_25)).collect();
        for name in ["h.exr", "h.tif", "h.pfm", "h.r32"] {
            let path = d.join(name);
            write_gray(&path, w, h, &values, OutFormat::for_path(&path, true).unwrap()).unwrap();
            let spec = RawSpec { width: w as u32, height: h as u32, ..RawSpec::default() };
            let px = read_pixels(&path, &spec).unwrap();
            assert_eq!((px.width, px.height, px.float), (w, h, true), "{name}");
            assert_eq!(px.gray().data, values, "{name}");
        }
        let rgba: Vec<[f64; 4]> = (0..w * h).map(|k| [k as f64 / 16.0, 0.5, 0.25, 1.0 - k as f64 / 16.0]).collect();
        for name in ["m.exr", "m.tif"] {
            let path = d.join(name);
            write_rgba(&path, w, h, &rgba, OutFormat::for_path(&path, true).unwrap()).unwrap();
            let px = read_pixels(&path, &RawSpec::default()).unwrap();
            assert_eq!(px.channels, 4, "{name}");
            assert_eq!(px.channel(Channel::A).data[3], 1.0 - 3.0 / 16.0, "{name}");
            assert_eq!(px.channel(Channel::B).data[7], 0.25, "{name}");
        }
        let path = d.join("m.png");
        write_rgba(&path, w, h, &rgba, OutFormat::Png16).unwrap();
        let px = read_pixels(&path, &RawSpec::default()).unwrap();
        assert_eq!((px.bits, px.channels), (16, 4));
        assert_eq!((px.channel(Channel::R).data[5] * 65535.0).round(), (5.0 / 16.0 * 65535.0f64).round());
        assert!(OutFormat::for_path(&d.join("x.png"), true).is_err());
        assert!(write_rgba(&d.join("x.pfm"), w, h, &rgba, OutFormat::Pfm).is_err());
    }

    #[test]
    fn float_images_are_normalized_and_tiff_works() {
        let d = dir("float");
        let img: ImageBuffer<image::Rgb<f32>, Vec<f32>> =
            ImageBuffer::from_fn(2, 1, |x, _| image::Rgb([x as f32 * 400.0, x as f32 * 400.0, x as f32 * 400.0]));
        DynamicImage::ImageRgb32F(img).save(d.join("f.hdr")).unwrap();
        let f = load_heightmap(&d.join("f.hdr"), &RawSpec::default()).unwrap();
        assert!(f.data[0] < 0.01 && f.data[1] > 0.99, "{:?}", f.data);
        write_heights(&d.join("t.tif"), 2, 2, &[0, 65535, 1000, 40000]).unwrap();
        let t = load_heightmap(&d.join("t.tif"), &RawSpec::default()).unwrap();
        assert_eq!((t.data[1] * 65535.0).round() as u16, 65535);
        assert!(write_heights(&d.join("t.bmp"), 2, 2, &[0; 4]).is_err());
    }

    #[test]
    fn splat_images_split_into_four_channels() {
        let d = dir("splat");
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(2, 1, |x, _| if x == 0 { Rgba([255, 0, 0, 0]) } else { Rgba([0, 0, 0, 255]) });
        img.save(d.join("s.png")).unwrap();
        let s = load_splat_rgba(&d.join("s.png")).unwrap();
        assert_eq!(s[0].data[0], 1.0);
        assert_eq!(s[3].data[1], 1.0);
        let rgb: ImageBuffer<image::Rgb<u8>, Vec<u8>> = ImageBuffer::from_pixel(1, 1, image::Rgb([0, 255, 0]));
        rgb.save(d.join("r.png")).unwrap();
        let s = load_splat_rgba(&d.join("r.png")).unwrap();
        assert_eq!(s[1].data[0], 1.0);
        assert_eq!(s[3].data[0], 0.0);
    }

    #[test]
    fn missing_and_bad_files_are_reported() {
        let d = dir("bad");
        assert!(read_pixels(&d.join("none.png"), &RawSpec::default()).is_err());
        std::fs::write(d.join("x.pfm"), b"P6\n1 1\n255\n").unwrap();
        assert!(read_pixels(&d.join("x.pfm"), &RawSpec::default()).is_err());
        std::fs::write(d.join("short.pfm"), b"Pf\n4 4\n-1.0\n\0\0\0\0").unwrap();
        assert!(read_pixels(&d.join("short.pfm"), &RawSpec::default()).is_err());
        std::fs::write(d.join("x.exr"), b"not an exr").unwrap();
        assert!(read_pixels(&d.join("x.exr"), &RawSpec::default()).is_err());
    }
}
