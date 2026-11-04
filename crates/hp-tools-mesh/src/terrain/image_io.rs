use std::path::Path;

use hp_geom::HeightField;
use image::{DynamicImage, ImageBuffer, Luma, Rgba};

pub const MAX_IMAGE_SIDE: u32 = 16384;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawSpec {
    pub width: u32,
    pub height: u32,
    pub big_endian: bool,
}

pub fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "raw" | "r16" | "bin" | "data"))
}

fn open(path: &Path) -> Result<DynamicImage, String> {
    let reader = image::ImageReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let reader = reader.with_guessed_format().map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = reader.into_dimensions().map_err(|e| format!("{}: {e}", path.display()))?;
    if w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE {
        return Err(format!("{}: the image is {w} x {h}, the limit is {MAX_IMAGE_SIDE} per side", path.display()));
    }
    image::open(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn normalize_floats(data: &mut [f64]) {
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

fn luma_field(img: &DynamicImage) -> Result<HeightField, String> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let float = matches!(img, DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_));
    let mut data: Vec<f64> = if float {
        img.to_luma32f().pixels().map(|p| f64::from(p.0[0])).collect()
    } else {
        img.to_luma16().pixels().map(|p| f64::from(p.0[0]) / 65535.0).collect()
    };
    if float {
        normalize_floats(&mut data);
    }
    HeightField::new(w, h, data).ok_or_else(|| "the image is empty".to_owned())
}

fn raw_field(path: &Path, spec: &RawSpec) -> Result<HeightField, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pixels = bytes.len() / 2;
    let (w, h) = if spec.width == 0 || spec.height == 0 {
        let side = (pixels as f64).sqrt().round() as usize;
        if side == 0 || side * side * 2 != bytes.len() {
            return Err(format!(
                "{}: {} bytes is not a square 16 bit image, give the width and the height",
                path.display(),
                bytes.len()
            ));
        }
        (side, side)
    } else {
        (spec.width as usize, spec.height as usize)
    };
    if w > MAX_IMAGE_SIDE as usize || h > MAX_IMAGE_SIDE as usize {
        return Err(format!("the RAW size {w} x {h} is over the limit of {MAX_IMAGE_SIDE} per side"));
    }
    if w * h * 2 != bytes.len() {
        return Err(format!(
            "{}: {w} x {h} at 16 bit needs {} bytes, the file has {}",
            path.display(),
            w * h * 2,
            bytes.len()
        ));
    }
    let data = bytes
        .chunks_exact(2)
        .map(|c| {
            let pair = [c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0)];
            let v = if spec.big_endian { u16::from_be_bytes(pair) } else { u16::from_le_bytes(pair) };
            f64::from(v) / 65535.0
        })
        .collect();
    HeightField::new(w, h, data).ok_or_else(|| "the image is empty".to_owned())
}

pub fn load_heightmap(path: &Path, raw: &RawSpec) -> Result<HeightField, String> {
    if is_raw(path) { raw_field(path, raw) } else { luma_field(&open(path)?) }
}

pub fn load_gray(path: &Path) -> Result<HeightField, String> {
    luma_field(&open(path)?)
}

pub fn load_splat_rgba(path: &Path) -> Result<[HeightField; 4], String> {
    let img = open(path)?;
    let (w, h) = (img.width() as usize, img.height() as usize);
    let has_alpha = img.color().has_alpha();
    let rgba = img.to_rgba16();
    let mut channels: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::with_capacity(w * h));
    for p in rgba.pixels() {
        for (c, out) in channels.iter_mut().enumerate() {
            let v = if c == 3 && !has_alpha { 0.0 } else { f64::from(p.0.get(c).copied().unwrap_or(0)) / 65535.0 };
            out.push(v);
        }
    }
    let make = |data: Vec<f64>| HeightField::new(w, h, data).ok_or_else(|| "the image is empty".to_owned());
    let [a, b, c, d] = channels;
    Ok([make(a)?, make(b)?, make(c)?, make(d)?])
}

pub fn write_heights(path: &Path, width: usize, height: usize, values: &[u16]) -> Result<(), String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    if is_raw(path) {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        return std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()));
    }
    let format = match ext.as_str() {
        "png" => image::ImageFormat::Png,
        "tif" | "tiff" => image::ImageFormat::Tiff,
        other => return Err(format!("heightmaps are written as png, tif or raw, not .{other}")),
    };
    let buffer: ImageBuffer<Luma<u16>, Vec<u16>> =
        ImageBuffer::from_raw(width as u32, height as u32, values.to_vec()).ok_or("the heightmap size is wrong")?;
    buffer.save_with_format(path, format).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn write_splat(path: &Path, width: usize, height: usize, rgba: &[u8]) -> Result<(), String> {
    let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width as u32, height as u32, rgba.to_vec()).ok_or("the splat map size is wrong")?;
    buffer.save_with_format(path, image::ImageFormat::Png).map_err(|e| format!("{}: {e}", path.display()))
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
        let spec = RawSpec { width: 4, height: 1, big_endian: true };
        let b = load_heightmap(&d.join("b.r16"), &spec).unwrap();
        assert_eq!((b.width, b.height), (4, 1));
        assert!((b.data[2] - 32768.0 / 65535.0).abs() < 1e-9);
        assert!(load_heightmap(&d.join("a.raw"), &RawSpec { width: 3, height: 3, big_endian: false }).is_err());
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
}
