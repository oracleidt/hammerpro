use std::borrow::Cow;
use std::sync::OnceLock;

use crate::types::{RenderError, TexFormat, TextureUpload};

pub(crate) struct TexEntry {
    pub view: wgpu::TextureView,
    pub bytes: u64,
}

pub(crate) struct Prepared<'a> {
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub levels: Vec<Cow<'a, [u8]>>,
}

fn full_mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

fn mip_dim(v: u32, level: u32) -> u32 {
    v.checked_shr(level).unwrap_or(0).max(1)
}

pub(crate) fn prepare<'a>(up: &'a TextureUpload, supports_bc: bool, max_dim: u32) -> Result<Prepared<'a>, RenderError> {
    prepare_with_mips(up, supports_bc, max_dim, true)
}

pub(crate) fn prepare_with_mips<'a>(
    up: &'a TextureUpload,
    supports_bc: bool,
    max_dim: u32,
    generate: bool,
) -> Result<Prepared<'a>, RenderError> {
    let (w, h) = (up.width, up.height);
    if w == 0 || h == 0 {
        return Err(RenderError::InvalidTexture(format!("empty size {w}x{h}")));
    }
    if w > max_dim || h > max_dim {
        return Err(RenderError::InvalidTexture(format!("size {w}x{h} exceeds the device limit {max_dim}")));
    }
    if up.levels.is_empty() {
        return Err(RenderError::InvalidTexture("no mip level".into()));
    }
    let max_levels = full_mip_count(w, h) as usize;
    let count = up.levels.len().min(max_levels);
    let mut levels: Vec<Cow<'a, [u8]>> = Vec::with_capacity(count);
    for (i, data) in up.levels.iter().take(count).enumerate() {
        let (lw, lh) = (mip_dim(w, i as u32), mip_dim(h, i as u32));
        let need = up.format.level_size(lw, lh);
        let Some(bytes) = data.get(..need) else {
            if i == 0 {
                return Err(RenderError::InvalidTexture(format!("level 0 has {} bytes, {need} expected", data.len())));
            }
            break;
        };
        levels.push(Cow::Borrowed(bytes));
    }
    let linear = up.format.is_linear();
    let bc = match up.format {
        TexFormat::Rgba8Srgb | TexFormat::Rgba8Linear | TexFormat::Rgba16Float => None,
        TexFormat::Bc1Srgb => Some((texpresso::Format::Bc1, wgpu::TextureFormat::Bc1RgbaUnormSrgb)),
        TexFormat::Bc2Srgb => Some((texpresso::Format::Bc2, wgpu::TextureFormat::Bc2RgbaUnormSrgb)),
        TexFormat::Bc3Srgb => Some((texpresso::Format::Bc3, wgpu::TextureFormat::Bc3RgbaUnormSrgb)),
        TexFormat::Bc1Linear => Some((texpresso::Format::Bc1, wgpu::TextureFormat::Bc1RgbaUnorm)),
        TexFormat::Bc2Linear => Some((texpresso::Format::Bc2, wgpu::TextureFormat::Bc2RgbaUnorm)),
        TexFormat::Bc3Linear => Some((texpresso::Format::Bc3, wgpu::TextureFormat::Bc3RgbaUnorm)),
    };
    let rgba = if up.format == TexFormat::Rgba16Float {
        wgpu::TextureFormat::Rgba16Float
    } else if linear {
        wgpu::TextureFormat::Rgba8Unorm
    } else {
        wgpu::TextureFormat::Rgba8UnormSrgb
    };
    match bc {
        Some((_, gpu_format)) if supports_bc && w.is_multiple_of(4) && h.is_multiple_of(4) => {
            Ok(Prepared { format: gpu_format, width: w, height: h, levels })
        }
        Some((codec, _)) => {
            let decoded: Vec<Cow<'a, [u8]>> = levels
                .iter()
                .enumerate()
                .map(|(i, data)| {
                    let (lw, lh) = (mip_dim(w, i as u32) as usize, mip_dim(h, i as u32) as usize);
                    let mut out = vec![0u8; lw * lh * 4];
                    if data.len() >= codec.compressed_size(lw, lh) {
                        codec.decompress(data, lw, lh, &mut out);
                    }
                    Cow::Owned(out)
                })
                .collect();
            let mut p = Prepared { format: rgba, width: w, height: h, levels: decoded };
            if generate && p.levels.len() == 1 {
                generate_mips(&mut p, linear);
            }
            Ok(p)
        }
        None => {
            let mut p = Prepared { format: rgba, width: w, height: h, levels };
            if generate && p.levels.len() == 1 && (w > 1 || h > 1) {
                generate_mips(&mut p, linear);
            }
            Ok(p)
        }
    }
}

pub(crate) fn prepared_bytes(p: &Prepared<'_>) -> u64 {
    p.levels.iter().map(|l| l.len() as u64).sum()
}

pub(crate) fn limit_size(p: &mut Prepared<'_>, limit: u32) {
    let (bw, bh) = p.format.block_dimensions();
    while (p.width > limit || p.height > limit) && p.levels.len() > 1 {
        let (w, h) = (mip_dim(p.width, 1), mip_dim(p.height, 1));
        if !w.is_multiple_of(bw) || !h.is_multiple_of(bh) {
            break;
        }
        p.levels.remove(0);
        p.width = w;
        p.height = h;
    }
}

fn srgb_to_linear_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| core::array::from_fn(|i| crate::gpu::srgb_to_linear(i as f32 / 255.0)))
}

fn linear_to_srgb_lut() -> &'static [u8; 4096] {
    static LUT: OnceLock<[u8; 4096]> = OnceLock::new();
    LUT.get_or_init(|| {
        core::array::from_fn(|i| {
            let l = i as f32 / 4095.0;
            let s = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
            (s.clamp(0.0, 1.0) * 255.0).round() as u8
        })
    })
}

fn generate_mips(p: &mut Prepared<'_>, linear: bool) {
    if p.format == wgpu::TextureFormat::Rgba16Float {
        generate_float_mips(p);
        return;
    }
    let to_lin = srgb_to_linear_lut();
    let to_srgb = linear_to_srgb_lut();
    let count = full_mip_count(p.width, p.height);
    let (mut w, mut h) = (p.width as usize, p.height as usize);
    for _ in 1..count {
        let Some(src) = p.levels.last() else { return };
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        if src.len() < w * h * 4 {
            return;
        }
        let mut dst = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            let (y0, y1) = ((y * 2).min(h - 1), (y * 2 + 1).min(h - 1));
            for x in 0..nw {
                let (x0, x1) = ((x * 2).min(w - 1), (x * 2 + 1).min(w - 1));
                let taps = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)];
                let mut acc = [0f32; 4];
                for (tx, ty) in taps {
                    let o = (ty * w + tx) * 4;
                    for c in 0..3 {
                        acc[c] += if linear { f32::from(src[o + c]) } else { to_lin[src[o + c] as usize] };
                    }
                    acc[3] += f32::from(src[o + 3]);
                }
                let o = (y * nw + x) * 4;
                for c in 0..3 {
                    dst[o + c] = if linear {
                        (acc[c] * 0.25).round() as u8
                    } else {
                        to_srgb[((acc[c] * 0.25).clamp(0.0, 1.0) * 4095.0).round() as usize]
                    };
                }
                dst[o + 3] = (acc[3] * 0.25).round() as u8;
            }
        }
        p.levels.push(Cow::Owned(dst));
        w = nw;
        h = nh;
    }
}

fn generate_float_mips(p: &mut Prepared<'_>) {
    let (mut w, mut h) = (p.width as usize, p.height as usize);
    while w > 1 || h > 1 {
        let Some(src) = p.levels.last() else { return };
        if src.len() < w * h * 8 {
            return;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut data = Vec::with_capacity(nw * nh * 8);
        for y in 0..nh {
            for x in 0..nw {
                for channel in 0..4 {
                    let mut sum = 0.0;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let at = (((y * 2 + dy).min(h - 1) * w + (x * 2 + dx).min(w - 1)) * 4 + channel) * 2;
                            let v = crate::cubemaps::f16_to_f32(u16::from_le_bytes([src[at], src[at + 1]]));
                            sum += if v.is_finite() { v } else { 0.0 };
                        }
                    }
                    data.extend_from_slice(
                        &crate::compiled_light::f16_bits((sum * 0.25).clamp(-65504.0, 65504.0)).to_le_bytes(),
                    );
                }
            }
        }
        p.levels.push(Cow::Owned(data));
        w = nw;
        h = nh;
    }
}

fn level_layout(format: wgpu::TextureFormat, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let (bw, bh) = format.block_dimensions();
    let block_bytes = format.block_copy_size(None).unwrap_or(4);
    let (cols, rows) = (w.div_ceil(bw), h.div_ceil(bh));
    (cols * block_bytes, rows, cols * bw, rows * bh)
}

pub(crate) fn upload(device: &wgpu::Device, queue: &wgpu::Queue, p: &Prepared<'_>, label: &str) -> TexEntry {
    let levels = p.levels.len().max(1) as u32;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: p.width, height: p.height, depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: p.format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut bytes = 0u64;
    for (i, data) in p.levels.iter().enumerate() {
        let (lw, lh) = (mip_dim(p.width, i as u32), mip_dim(p.height, i as u32));
        let (bpr, rows, pw, ph) = level_layout(p.format, lw, lh);
        let need = bpr as usize * rows as usize;
        let Some(data) = data.get(..need) else { continue };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: i as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(rows) },
            wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
        );
        bytes += need as u64;
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    TexEntry { view, bytes }
}

pub(crate) fn builtin_missing() -> TextureUpload {
    let size = 64u32;
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let on = ((x / 32) + (y / 32)).is_multiple_of(2);
            px.extend_from_slice(if on { &[255, 0, 255, 255] } else { &[0, 0, 0, 255] });
        }
    }
    TextureUpload { width: size, height: size, format: TexFormat::Rgba8Srgb, levels: vec![px] }
}

pub(crate) fn builtin_loading() -> TextureUpload {
    TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![[128u8, 128, 128, 255].repeat(16)] }
}

pub(crate) fn builtin_white() -> TextureUpload {
    TextureUpload { width: 1, height: 1, format: TexFormat::Rgba8Srgb, levels: vec![vec![255; 4]] }
}

pub(crate) fn builtin_flat_normal() -> TextureUpload {
    TextureUpload { width: 1, height: 1, format: TexFormat::Rgba8Linear, levels: vec![vec![128, 128, 255, 255]] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_uploads_keep_radiance_and_linear_mip_averages() {
        let up = TextureUpload::rgba16_float(
            2,
            2,
            &[[4.0, -2.0, 0.125, 0.5], [8.0, -2.0, 0.125, 0.5], [12.0, -2.0, 0.125, 0.5], [16.0, -2.0, 0.125, 0.5]],
        )
        .unwrap();
        assert_eq!(up.format.level_size(2, 2), 32);
        let prepared = prepare(&up, false, 8192).unwrap();
        assert_eq!(prepared.format, wgpu::TextureFormat::Rgba16Float);
        assert_eq!(prepared.levels.len(), 2);
        let values: Vec<f32> = prepared.levels[1]
            .chunks_exact(2)
            .map(|p| crate::cubemaps::f16_to_f32(u16::from_le_bytes([p[0], p[1]])))
            .collect();
        assert_eq!(values, [10.0, -2.0, 0.125, 0.5]);
        let finite = TextureUpload::rgba16_float(1, 1, &[[f32::NAN, f32::INFINITY, 1e30, 1.0]]).unwrap();
        let values: Vec<f32> = finite.levels[0]
            .chunks_exact(2)
            .map(|p| crate::cubemaps::f16_to_f32(u16::from_le_bytes([p[0], p[1]])))
            .collect();
        assert_eq!(values, [0.0, 0.0, 65504.0, 1.0]);
        assert!(TextureUpload::rgba16_float(2, 1, &[[1.0; 4]]).is_err());
    }

    #[test]
    fn rejects_malformed() {
        let mut up = builtin_white();
        up.width = 0;
        assert!(prepare(&up, true, 8192).is_err());
        let up = TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![vec![0; 10]] };
        assert!(prepare(&up, true, 8192).is_err());
        let up = TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![] };
        assert!(prepare(&up, true, 8192).is_err());
        let up = TextureUpload { width: 16384, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![vec![0; 16]] };
        assert!(prepare(&up, true, 8192).is_err());
    }

    #[test]
    fn generates_gamma_correct_mips() {
        let up = TextureUpload {
            width: 2,
            height: 2,
            format: TexFormat::Rgba8Srgb,
            levels: vec![vec![0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255]],
        };
        let p = prepare(&up, true, 8192).unwrap();
        assert_eq!(p.levels.len(), 2);
        let m = &p.levels[1];
        assert!(m[0] > 180 && m[0] < 195, "{}", m[0]);
        assert_eq!(m[3], 255);
    }

    #[test]
    fn authored_single_level_textures_are_preserved_in_all_upload_formats() {
        for format in [TexFormat::Rgba8Srgb, TexFormat::Rgba8Linear, TexFormat::Rgba16Float, TexFormat::Bc1Srgb] {
            let up = TextureUpload { width: 8, height: 8, format, levels: vec![vec![0; format.level_size(8, 8)]] };
            for supports_bc in [false, true] {
                let prepared = prepare_with_mips(&up, supports_bc, 8192, false).unwrap();
                assert_eq!(prepared.levels.len(), 1, "{format:?}, BC {supports_bc}");
            }
        }
    }

    #[test]
    fn non_square_mips_and_truncated_tail() {
        let up = builtin_missing();
        let p = prepare(&up, true, 8192).unwrap();
        assert_eq!(p.levels.len(), 7);
        assert_eq!(p.levels[6].len(), 4);
        let up = TextureUpload {
            width: 8,
            height: 2,
            format: TexFormat::Rgba8Srgb,
            levels: vec![vec![0; 64], vec![0; 16], vec![0; 3]],
        };
        let p = prepare(&up, true, 8192).unwrap();
        assert_eq!(p.levels.len(), 2);
    }

    #[test]
    fn bc_fallback_decodes() {
        let block = vec![0xff, 0xff, 0x00, 0x00, 0, 0, 0, 0];
        let up = TextureUpload { width: 4, height: 4, format: TexFormat::Bc1Srgb, levels: vec![block] };
        let native = prepare(&up, true, 8192).unwrap();
        assert_eq!(native.format, wgpu::TextureFormat::Bc1RgbaUnormSrgb);
        let p = prepare(&up, false, 8192).unwrap();
        assert_eq!(p.format, wgpu::TextureFormat::Rgba8UnormSrgb);
        assert_eq!(&p.levels[0][..4], &[255, 255, 255, 255]);
        let up = TextureUpload {
            width: 2,
            height: 2,
            format: TexFormat::Bc1Srgb,
            levels: vec![vec![0xff, 0xff, 0, 0, 0, 0, 0, 0]],
        };
        let p = prepare(&up, true, 8192).unwrap();
        assert_eq!(p.format, wgpu::TextureFormat::Rgba8UnormSrgb);
        assert_eq!(p.levels[0].len(), 16);
    }

    #[test]
    fn linear_textures_skip_srgb() {
        let up = TextureUpload {
            width: 2,
            height: 2,
            format: TexFormat::Rgba8Linear,
            levels: vec![vec![0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255]],
        };
        let p = prepare(&up, true, 8192).unwrap();
        assert_eq!(p.format, wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(p.levels[1][0], 128);
        let block = vec![0xff, 0xff, 0x00, 0x00, 0, 0, 0, 0];
        let up = TextureUpload { width: 4, height: 4, format: TexFormat::Bc1Linear, levels: vec![block] };
        assert_eq!(prepare(&up, true, 8192).unwrap().format, wgpu::TextureFormat::Bc1RgbaUnorm);
        assert_eq!(prepare(&up, false, 8192).unwrap().format, wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(TexFormat::Bc3Srgb.to_linear(), TexFormat::Bc3Linear);
        assert!(!TexFormat::Rgba8Srgb.is_linear() && TexFormat::Rgba8Srgb.to_linear().is_linear());
        assert_eq!(builtin_flat_normal().levels[0], vec![128, 128, 255, 255]);
    }

    #[test]
    fn big_textures_drop_their_top_mips_under_a_limit() {
        let up =
            TextureUpload { width: 64, height: 32, format: TexFormat::Rgba8Srgb, levels: vec![vec![200; 64 * 32 * 4]] };
        let mut p = prepare(&up, true, 8192).unwrap();
        let full = p.levels.len();
        limit_size(&mut p, 16);
        assert_eq!((p.width, p.height), (16, 8));
        assert_eq!(p.levels.len(), full - 2);
        assert_eq!(p.levels[0].len(), 16 * 8 * 4);
        let mut p = prepare(&up, true, 8192).unwrap();
        limit_size(&mut p, 4096);
        assert_eq!((p.width, p.levels.len()), (64, full), "no change under the limit");
        let block = vec![0u8; 8];
        let up = TextureUpload {
            width: 8,
            height: 8,
            format: TexFormat::Bc1Srgb,
            levels: vec![block.repeat(4), block.clone(), block],
        };
        let mut p = prepare(&up, true, 8192).unwrap();
        limit_size(&mut p, 1);
        assert_eq!((p.width, p.height, p.levels.len()), (4, 4, 2), "block textures keep whole blocks");
    }

    #[test]
    fn bc_layout() {
        assert_eq!(level_layout(wgpu::TextureFormat::Bc1RgbaUnormSrgb, 2, 2), (8, 1, 4, 4));
        assert_eq!(level_layout(wgpu::TextureFormat::Bc3RgbaUnormSrgb, 8, 4), (32, 1, 8, 4));
        assert_eq!(level_layout(wgpu::TextureFormat::Rgba8UnormSrgb, 3, 2), (12, 2, 3, 2));
    }
}
