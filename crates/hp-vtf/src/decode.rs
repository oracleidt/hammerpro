use rayon::prelude::*;

use crate::{ImageFormat, RgbaFloatImage, RgbaImage, VtfError, bc};

const PARALLEL_PIXELS: usize = 256 * 256;

const PIXELS_PER_JOB: usize = 16 * 1024;

/// Preserve floating point radiance and the precision of 16-bit UNORM textures.
/// Other encodings return normalized numeric channels in their original space.
pub fn convert_to_rgba32f(
    format: ImageFormat,
    src: &[u8],
    width: u32,
    height: u32,
) -> Result<RgbaFloatImage, VtfError> {
    use ImageFormat as F;
    if width == 0 || height == 0 {
        return Err(VtfError::InvalidDimensions { width, height });
    }
    let needed = format.image_size(width, height).ok_or(VtfError::TooLarge)?;
    let count = usize::try_from(needed).map_err(|_| VtfError::TooLarge)?;
    let src = src.get(..count).ok_or(VtfError::Truncated { needed, available: src.len() as u64 })?;
    let float = |p: &[u8], index: usize| {
        let start = index * 4;
        f32::from_le_bytes([p[start], p[start + 1], p[start + 2], p[start + 3]])
    };
    let pixels = match format {
        F::Rgba16161616F => src
            .chunks_exact(8)
            .map(|p| std::array::from_fn(|i| f16_to_f32(u16::from_le_bytes([p[2 * i], p[2 * i + 1]]))))
            .collect(),
        F::Rgba16161616 => src
            .chunks_exact(8)
            .map(|p| std::array::from_fn(|i| f32::from(u16::from_le_bytes([p[2 * i], p[2 * i + 1]])) / 65535.0))
            .collect(),
        F::Rgba32323232F => src.chunks_exact(16).map(|p| std::array::from_fn(|i| float(p, i))).collect(),
        F::Rgb323232F => src.chunks_exact(12).map(|p| [float(p, 0), float(p, 1), float(p, 2), 1.0]).collect(),
        F::R32F => src
            .chunks_exact(4)
            .map(|p| {
                let v = float(p, 0);
                [v, v, v, 1.0]
            })
            .collect(),
        _ => convert_to_rgba8(format, src, width, height)?
            .pixels
            .chunks_exact(4)
            .map(|p| std::array::from_fn(|i| f32::from(p[i]) / 255.0))
            .collect(),
    };
    Ok(RgbaFloatImage { width, height, pixels })
}

pub fn convert_to_rgba8(format: ImageFormat, src: &[u8], width: u32, height: u32) -> Result<RgbaImage, VtfError> {
    if width == 0 || height == 0 {
        return Err(VtfError::InvalidDimensions { width, height });
    }
    if !format.is_decodable() {
        return Err(VtfError::UnsupportedFormat(format));
    }
    let needed = format.image_size(width, height).ok_or(VtfError::TooLarge)?;
    let needed_usize = usize::try_from(needed).map_err(|_| VtfError::TooLarge)?;
    let src = src.get(..needed_usize).ok_or(VtfError::Truncated { needed, available: src.len() as u64 })?;
    let (w, h) = (width as usize, height as usize);
    let out_len = w.checked_mul(h).and_then(|p| p.checked_mul(4)).ok_or(VtfError::TooLarge)?;
    let mut out = vec![0u8; out_len];
    let parallel = w * h >= PARALLEL_PIXELS;

    if let Some(bcf) = format.bc_format() {
        let src_row = w.div_ceil(4) * bcf.block_bytes() as usize;
        let out_row = w * 16;
        let job = |(dst, blocks): (&mut [u8], &[u8])| bc::decode_block_row(bcf, blocks, w, dst);
        if parallel {
            out.par_chunks_mut(out_row).zip(src.par_chunks(src_row)).for_each(job);
        } else {
            out.chunks_mut(out_row).zip(src.chunks(src_row)).for_each(job);
        }
    } else {
        convert_pixels(format, src, &mut out, parallel);
    }
    Ok(RgbaImage { width, height, pixels: out })
}

fn map_pixels<const N: usize, F>(src: &[u8], out: &mut [u8], parallel: bool, f: F)
where
    F: Fn([u8; N]) -> [u8; 4] + Sync,
{
    let run = |(dst, s): (&mut [u8], &[u8])| {
        for (d, p) in dst.as_chunks_mut::<4>().0.iter_mut().zip(s.as_chunks::<N>().0) {
            *d = f(*p);
        }
    };
    if parallel {
        out.par_chunks_mut(PIXELS_PER_JOB * 4).zip(src.par_chunks(PIXELS_PER_JOB * N)).for_each(run);
    } else {
        run((out, src));
    }
}

#[inline]
fn expand5(v: u16) -> u8 {
    let v = (v & 31) as u8;
    (v << 3) | (v >> 2)
}

#[inline]
fn expand6(v: u16) -> u8 {
    let v = (v & 63) as u8;
    (v << 2) | (v >> 4)
}

#[inline]
fn expand4(v: u16) -> u8 {
    (v & 15) as u8 * 17
}

#[inline]
pub(crate) fn f16_to_f32(h: u16) -> f32 {
    let sign = u32::from(h >> 15) << 31;
    let exp = u32::from((h >> 10) & 0x1f);
    let man = u32::from(h & 0x3ff);
    if exp == 0 {
        let v = man as f32 * (1.0 / 16_777_216.0);
        return if sign != 0 { -v } else { v };
    }
    let bits = if exp == 31 { sign | 0x7f80_0000 | (man << 13) } else { sign | ((exp + 112) << 23) | (man << 13) };
    f32::from_bits(bits)
}

#[inline]
fn tonemap(v: f32) -> u8 {
    (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8
}

#[inline]
fn unorm(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[inline]
fn bluescreen(rgb: [u8; 3]) -> [u8; 4] {
    if rgb == [0, 0, 255] { [0, 0, 0, 0] } else { [rgb[0], rgb[1], rgb[2], 255] }
}

#[inline]
fn u16_pair(p: [u8; 2]) -> u16 {
    u16::from_le_bytes(p)
}

fn convert_pixels(format: ImageFormat, src: &[u8], out: &mut [u8], par: bool) {
    use ImageFormat as F;
    match format {
        F::Rgba8888 => out.copy_from_slice(src),
        F::Abgr8888 => map_pixels(src, out, par, |[a, b, g, r]: [u8; 4]| [r, g, b, a]),
        F::Argb8888 => map_pixels(src, out, par, |[a, r, g, b]: [u8; 4]| [r, g, b, a]),
        F::Bgra8888 => map_pixels(src, out, par, |[b, g, r, a]: [u8; 4]| [r, g, b, a]),
        F::Bgrx8888 => map_pixels(src, out, par, |[b, g, r, _]: [u8; 4]| [r, g, b, 255]),
        F::Uvwq8888 | F::Uvlx8888 => out.copy_from_slice(src),
        F::Rgb888 => map_pixels(src, out, par, |[r, g, b]: [u8; 3]| [r, g, b, 255]),
        F::Bgr888 => map_pixels(src, out, par, |[b, g, r]: [u8; 3]| [r, g, b, 255]),
        F::Rgb888Bluescreen => map_pixels(src, out, par, |[r, g, b]: [u8; 3]| bluescreen([r, g, b])),
        F::Bgr888Bluescreen => map_pixels(src, out, par, |[b, g, r]: [u8; 3]| bluescreen([r, g, b])),
        F::I8 | F::P8 => map_pixels(src, out, par, |[i]: [u8; 1]| [i, i, i, 255]),
        F::A8 => map_pixels(src, out, par, |[a]: [u8; 1]| [0, 0, 0, a]),
        F::Ia88 => map_pixels(src, out, par, |[i, a]: [u8; 2]| [i, i, i, a]),
        F::Uv88 => map_pixels(src, out, par, |[u, v]: [u8; 2]| [u, v, 0, 255]),
        F::Rgb565 => map_pixels(src, out, par, |p: [u8; 2]| {
            let v = u16_pair(p);
            [expand5(v), expand6(v >> 5), expand5(v >> 11), 255]
        }),
        F::Bgr565 => map_pixels(src, out, par, |p: [u8; 2]| {
            let v = u16_pair(p);
            [expand5(v >> 11), expand6(v >> 5), expand5(v), 255]
        }),
        F::Bgrx5551 => map_pixels(src, out, par, |p: [u8; 2]| {
            let v = u16_pair(p);
            [expand5(v >> 10), expand5(v >> 5), expand5(v), 255]
        }),
        F::Bgra5551 => map_pixels(src, out, par, |p: [u8; 2]| {
            let v = u16_pair(p);
            let a = if v & 0x8000 != 0 { 255 } else { 0 };
            [expand5(v >> 10), expand5(v >> 5), expand5(v), a]
        }),
        F::Bgra4444 => map_pixels(src, out, par, |p: [u8; 2]| {
            let v = u16_pair(p);
            [expand4(v >> 8), expand4(v >> 4), expand4(v), expand4(v >> 12)]
        }),
        F::Rgba16161616F => map_pixels(src, out, par, |p: [u8; 8]| {
            let c = |i: usize| f16_to_f32(u16::from_le_bytes([p[2 * i], p[2 * i + 1]]));
            [tonemap(c(0)), tonemap(c(1)), tonemap(c(2)), unorm(c(3))]
        }),
        F::Rgba16161616 => map_pixels(src, out, par, |p: [u8; 8]| {
            let c = |i: usize| {
                let v = u32::from(u16::from_le_bytes([p[2 * i], p[2 * i + 1]]));
                ((v * 255 + 32_767) / 65_535) as u8
            };
            [c(0), c(1), c(2), c(3)]
        }),
        F::R32F => map_pixels(src, out, par, |p: [u8; 4]| {
            let v = tonemap(f32::from_le_bytes(p));
            [v, v, v, 255]
        }),
        F::Rgb323232F => map_pixels(src, out, par, |p: [u8; 12]| {
            let c = |i: usize| f32::from_le_bytes([p[4 * i], p[4 * i + 1], p[4 * i + 2], p[4 * i + 3]]);
            [tonemap(c(0)), tonemap(c(1)), tonemap(c(2)), 255]
        }),
        F::Rgba32323232F => map_pixels(src, out, par, |p: [u8; 16]| {
            let c = |i: usize| f32::from_le_bytes([p[4 * i], p[4 * i + 1], p[4 * i + 2], p[4 * i + 3]]);
            [tonemap(c(0)), tonemap(c(1)), tonemap(c(2)), unorm(c(3))]
        }),
        F::Dxt1
        | F::Dxt3
        | F::Dxt5
        | F::Dxt1OneBitAlpha
        | F::Ati1n
        | F::Ati2n
        | F::NvDst16
        | F::NvDst24
        | F::NvIntz
        | F::NvRawz
        | F::AtiDst16
        | F::AtiDst24
        | F::NvNull => {}
    }
}

pub(crate) fn downscale_to_fit(img: RgbaImage, max_dim: u32) -> RgbaImage {
    let max_dim = max_dim.max(1);
    let (w, h) = (img.width, img.height);
    let largest = w.max(h);
    if largest <= max_dim || largest == 0 {
        return img;
    }
    let scale = |v: u32| ((u64::from(v) * u64::from(max_dim)) / u64::from(largest)).max(1) as u32;
    let (tw, th) = (scale(w), scale(h));
    let (w, h, tw, th) = (w as usize, h as usize, tw as usize, th as usize);
    let src = img.pixels.as_chunks::<4>().0;
    let mut out = vec![0u8; tw * th * 4];
    for (ty, row) in out.chunks_exact_mut(tw * 4).enumerate() {
        let y0 = ty * h / th;
        let y1 = ((ty + 1) * h / th).max(y0 + 1).min(h);
        for (tx, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let x0 = tx * w / tw;
            let x1 = ((tx + 1) * w / tw).max(x0 + 1).min(w);
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for y in y0..y1 {
                if let Some(line) = src.get(y * w + x0..y * w + x1) {
                    for p in line {
                        for c in 0..4 {
                            sum[c] += u32::from(p[c]);
                        }
                    }
                    count += line.len() as u32;
                }
            }
            if count > 0 {
                *px = sum.map(|s| ((s + count / 2) / count) as u8);
            }
        }
    }
    RgbaImage { width: tw as u32, height: th as u32, pixels: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats() {
        assert_eq!(f16_to_f32(0x3C00), 1.0);
        assert_eq!(f16_to_f32(0xC000), -2.0);
        assert_eq!(f16_to_f32(0x0000), 0.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert!(f16_to_f32(0x7C00).is_infinite());
        assert!(f16_to_f32(0x7E00).is_nan());
        assert!((f16_to_f32(0x0001) - 5.960_464_5e-8).abs() < 1e-12);
    }

    #[test]
    fn floating_decode_keeps_hdr_and_unorm_precision() {
        let half: Vec<u8> = [0x4400u16, 0x3800, 0x3000, 0x3c00].into_iter().flat_map(u16::to_le_bytes).collect();
        assert_eq!(
            convert_to_rgba32f(ImageFormat::Rgba16161616F, &half, 1, 1).unwrap().pixels,
            [[4.0, 0.5, 0.125, 1.0]]
        );
        let wide: Vec<u8> = [65535u16, 32768, 1, 65535].into_iter().flat_map(u16::to_le_bytes).collect();
        let image = convert_to_rgba32f(ImageFormat::Rgba16161616, &wide, 1, 1).unwrap();
        assert_eq!(image.pixels[0], [1.0, 32768.0 / 65535.0, 1.0 / 65535.0, 1.0]);
        let raw: Vec<u8> = [8.0f32, -2.0, 0.0625, 0.25].into_iter().flat_map(f32::to_le_bytes).collect();
        assert_eq!(
            convert_to_rgba32f(ImageFormat::Rgba32323232F, &raw, 1, 1).unwrap().pixels,
            [[8.0, -2.0, 0.0625, 0.25]]
        );
        assert!(convert_to_rgba32f(ImageFormat::Rgba16161616F, &half[..7], 1, 1).is_err());
        assert!(convert_to_rgba32f(ImageFormat::Rgba16161616F, &half, 0, 1).is_err());
    }

    fn one(format: ImageFormat, bytes: &[u8]) -> [u8; 4] {
        let img = convert_to_rgba8(format, bytes, 1, 1).unwrap();
        [img.pixels[0], img.pixels[1], img.pixels[2], img.pixels[3]]
    }

    #[test]
    fn uncompressed_formats() {
        use ImageFormat as F;
        assert_eq!(one(F::Rgba8888, &[1, 2, 3, 4]), [1, 2, 3, 4]);
        assert_eq!(one(F::Abgr8888, &[4, 3, 2, 1]), [1, 2, 3, 4]);
        assert_eq!(one(F::Argb8888, &[4, 1, 2, 3]), [1, 2, 3, 4]);
        assert_eq!(one(F::Bgra8888, &[3, 2, 1, 4]), [1, 2, 3, 4]);
        assert_eq!(one(F::Bgrx8888, &[3, 2, 1, 4]), [1, 2, 3, 255]);
        assert_eq!(one(F::Rgb888, &[1, 2, 3]), [1, 2, 3, 255]);
        assert_eq!(one(F::Bgr888, &[3, 2, 1]), [1, 2, 3, 255]);
        assert_eq!(one(F::Rgb888Bluescreen, &[0, 0, 255]), [0, 0, 0, 0]);
        assert_eq!(one(F::Rgb888Bluescreen, &[0, 1, 255]), [0, 1, 255, 255]);
        assert_eq!(one(F::Bgr888Bluescreen, &[255, 0, 0]), [0, 0, 0, 0]);
        assert_eq!(one(F::I8, &[7]), [7, 7, 7, 255]);
        assert_eq!(one(F::P8, &[7]), [7, 7, 7, 255]);
        assert_eq!(one(F::A8, &[7]), [0, 0, 0, 7]);
        assert_eq!(one(F::Ia88, &[7, 9]), [7, 7, 7, 9]);
        assert_eq!(one(F::Uv88, &[7, 9]), [7, 9, 0, 255]);
        assert_eq!(one(F::Uvwq8888, &[1, 2, 3, 4]), [1, 2, 3, 4]);
        assert_eq!(one(F::Bgr565, &0xF800u16.to_le_bytes()), [255, 0, 0, 255]);
        assert_eq!(one(F::Rgb565, &0xF800u16.to_le_bytes()), [0, 0, 255, 255]);
        assert_eq!(one(F::Bgr565, &0x07E0u16.to_le_bytes()), [0, 255, 0, 255]);
        assert_eq!(one(F::Bgrx5551, &0x7C00u16.to_le_bytes()), [255, 0, 0, 255]);
        assert_eq!(one(F::Bgra5551, &0x7C00u16.to_le_bytes()), [255, 0, 0, 0]);
        assert_eq!(one(F::Bgra5551, &0x801Fu16.to_le_bytes()), [0, 0, 255, 255]);
        assert_eq!(one(F::Bgra4444, &0xF00Fu16.to_le_bytes()), [0, 0, 255, 255]);
        assert_eq!(one(F::Bgra4444, &0x0F00u16.to_le_bytes()), [255, 0, 0, 0]);
        let mut half = Vec::new();
        for v in [0x3C00u16, 0x0000, 0x4000, 0x3800] {
            half.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(one(F::Rgba16161616F, &half), [255, 0, 255, 128]);
        let mut wide = Vec::new();
        for v in [0xFFFFu16, 0, 0x8080, 0xFFFF] {
            wide.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(one(F::Rgba16161616, &wide), [255, 0, 128, 255]);
        assert_eq!(one(F::R32F, &1.0f32.to_le_bytes()), [255, 255, 255, 255]);
    }

    #[test]
    fn rejects_depth_formats_and_short_data() {
        assert_eq!(
            convert_to_rgba8(ImageFormat::NvIntz, &[0; 4], 1, 1),
            Err(VtfError::UnsupportedFormat(ImageFormat::NvIntz))
        );
        assert!(matches!(
            convert_to_rgba8(ImageFormat::Dxt5, &[0; 15], 4, 4),
            Err(VtfError::Truncated { needed: 16, .. })
        ));
        assert!(matches!(convert_to_rgba8(ImageFormat::Rgba8888, &[], 0, 4), Err(VtfError::InvalidDimensions { .. })));
    }

    #[test]
    fn large_images_decode_in_parallel_identically() {
        let (w, h) = (512u32, 260u32);
        let src: Vec<u8> = (0..(w * h * 3)).map(|i| (i * 7 % 251) as u8).collect();
        let img = convert_to_rgba8(ImageFormat::Bgr888, &src, w, h).unwrap();
        for (i, px) in img.pixels.chunks_exact(4).enumerate() {
            assert_eq!(px, [src[i * 3 + 2], src[i * 3 + 1], src[i * 3], 255]);
        }
    }

    #[test]
    fn downscale_keeps_aspect() {
        let img = RgbaImage { width: 512, height: 256, pixels: vec![200; 512 * 256 * 4] };
        let small = downscale_to_fit(img, 128);
        assert_eq!((small.width, small.height), (128, 64));
        assert!(small.pixels.iter().all(|&v| v == 200));
    }

    #[test]
    fn every_format_decodes_any_size_and_parallel_matches_strips() {
        let mut seed = 0x1234_5678u32;
        let mut noise = |n: usize| -> Vec<u8> {
            (0..n)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect()
        };
        for &format in ImageFormat::ALL {
            for (w, h) in [(1, 1), (3, 5), (5, 3), (301, 260)] {
                let size = format.image_size(w, h).unwrap() as usize;
                let src = noise(size);
                let result = convert_to_rgba8(format, &src, w, h);
                if !format.is_decodable() {
                    assert_eq!(result, Err(VtfError::UnsupportedFormat(format)));
                    continue;
                }
                let full = result.unwrap();
                assert_eq!(full.pixels.len(), (w * h * 4) as usize, "{format}");
                if (w, h) != (301, 260) {
                    continue;
                }
                let strip_src = format.image_size(w, 4).unwrap() as usize;
                let strip_out = (w * 4 * 4) as usize;
                for k in 0..(h / 4) as usize {
                    let strip = convert_to_rgba8(format, &src[k * strip_src..(k + 1) * strip_src], w, 4).unwrap();
                    assert_eq!(strip.pixels, full.pixels[k * strip_out..(k + 1) * strip_out], "{format} strip {k}");
                }
            }
        }
    }
}
