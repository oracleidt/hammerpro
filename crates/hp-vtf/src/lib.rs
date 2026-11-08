#![forbid(unsafe_code)]

mod bc;
mod compose;
mod decode;
mod document;
mod encode;
mod error;
pub mod flags;
mod format;
mod header;
pub mod ops;
mod remap;

pub use compose::{ComposeOptions, ENCODABLE, can_encode, compose};
pub use decode::{convert_to_rgba8, convert_to_rgba32f};
pub use document::VtfDocument;
pub use encode::{
    EncodeFormat, EncodeOptions, EncodeQuality, MAX_POW2_SIZE, MipFilter, encode, encode_frames, load_image_file,
};
pub use error::VtfError;
pub use format::{BcFormat, ImageFormat};
pub use header::{MAX_HEADER_LEN, SIGNATURE, VtfInfo};
pub use remap::{MAX_REPEAT, Refusal, TexelKind, TexelMap, mip_filter_for, remap_format, remap_image, remap_vtf};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RgbaFloatImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, VtfError> {
        let expected = u64::from(width) * u64::from(height) * 4;
        if pixels.len() as u64 != expected {
            return Err(VtfError::InvalidImage(format!(
                "{width}x{height} image needs {expected} bytes, got {}",
                pixels.len()
            )));
        }
        Ok(Self { width, height, pixels })
    }

    #[must_use]
    pub fn is_opaque(&self) -> bool {
        self.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MipSelect {
    #[default]
    Largest,
    Index(u32),
    MaxDimension(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DecodeOptions {
    pub mip: MipSelect,
    pub frame: u32,
    pub face: u32,
}

impl DecodeOptions {
    #[must_use]
    pub fn thumbnail(max_dim: u32) -> Self {
        Self { mip: MipSelect::MaxDimension(max_dim), frame: 0, face: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawLevel<'a> {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

impl RawLevel<'_> {
    pub fn to_rgba8(&self) -> Result<RgbaImage, VtfError> {
        convert_to_rgba8(self.format, self.data, self.width, self.height)
    }

    pub fn to_rgba32f(&self) -> Result<RgbaFloatImage, VtfError> {
        convert_to_rgba32f(self.format, self.data, self.width, self.height)
    }
}

#[must_use]
pub fn is_vtf(data: &[u8]) -> bool {
    data.starts_with(&SIGNATURE)
}

pub fn read_info(data: &[u8]) -> Result<VtfInfo, VtfError> {
    header::parse(data).map(|p| p.info)
}

fn resolve_mip(info: &VtfInfo, select: MipSelect) -> Result<u32, VtfError> {
    match select {
        MipSelect::Largest => Ok(0),
        MipSelect::Index(i) if i < info.mip_count => Ok(i),
        MipSelect::Index(i) => Err(VtfError::MipOutOfRange { index: i, count: info.mip_count }),
        MipSelect::MaxDimension(n) => Ok(info.mip_for_max_dimension(n)),
    }
}

pub fn decode_rgba8(data: &[u8], opts: &DecodeOptions) -> Result<RgbaImage, VtfError> {
    let parsed = header::parse(data)?;
    let mip = resolve_mip(&parsed.info, opts.mip)?;
    let range = parsed.level_range(mip, opts.frame, opts.face)?;
    let bytes = header::slice_range(data, range)?;
    let (w, h, _) = parsed.info.mip_dimensions(mip);
    let img = convert_to_rgba8(parsed.info.format, bytes, w, h)?;
    Ok(match opts.mip {
        MipSelect::MaxDimension(n) => decode::downscale_to_fit(img, n),
        _ => img,
    })
}

pub fn decode_slice_rgba8(data: &[u8], mip: u32, frame: u32, face: u32, slice: u32) -> Result<RgbaImage, VtfError> {
    let parsed = header::parse(data)?;
    let range = parsed.level_range(mip, frame, face)?;
    let (w, h, d) = parsed.info.mip_dimensions(mip);
    if slice >= d {
        return Err(VtfError::Corrupt("slice out of range"));
    }
    let size = parsed.info.slice_size(mip).ok_or(VtfError::TooLarge)?;
    let shift = size.checked_mul(u64::from(slice)).ok_or(VtfError::TooLarge)?;
    let bytes = header::slice_range(data, range.start + shift..range.end + shift)?;
    convert_to_rgba8(parsed.info.format, bytes, w, h)
}

pub fn patch_header(
    data: &[u8],
    tex_flags: u32,
    bumpmap_scale: f32,
    reflectivity: [f32; 3],
    first_frame: u16,
) -> Result<Vec<u8>, VtfError> {
    let parsed = header::parse(data)?;
    let mut out = data.to_vec();
    let keep_env = parsed.info.flags & flags::ENVMAP;
    let new_flags = (tex_flags & !flags::ENVMAP) | keep_env;
    out[20..24].copy_from_slice(&new_flags.to_le_bytes());
    let cube_without_start = parsed.info.faces == 6 && parsed.info.first_frame == 0xFFFF;
    if !cube_without_start {
        out[26..28].copy_from_slice(&first_frame.to_le_bytes());
    }
    for (i, c) in reflectivity.iter().enumerate() {
        out[32 + 4 * i..36 + 4 * i].copy_from_slice(&c.to_le_bytes());
    }
    out[48..52].copy_from_slice(&bumpmap_scale.to_le_bytes());
    Ok(out)
}

pub fn raw_level(data: &[u8], mip: u32, frame: u32, face: u32) -> Result<RawLevel<'_>, VtfError> {
    let parsed = header::parse(data)?;
    raw_level_parsed(data, &parsed, mip, frame, face)
}

fn raw_level_parsed<'a>(
    data: &'a [u8],
    parsed: &header::Parsed,
    mip: u32,
    frame: u32,
    face: u32,
) -> Result<RawLevel<'a>, VtfError> {
    let range = parsed.level_range(mip, frame, face)?;
    let bytes = header::slice_range(data, range)?;
    let (width, height, _) = parsed.info.mip_dimensions(mip);
    Ok(RawLevel { format: parsed.info.format, width, height, data: bytes })
}

pub fn raw_mips(data: &[u8], frame: u32, face: u32) -> Result<Vec<RawLevel<'_>>, VtfError> {
    let parsed = header::parse(data)?;
    (0..parsed.info.mip_count).map(|mip| raw_level_parsed(data, &parsed, mip, frame, face)).collect()
}

pub fn decode_low_res_rgba8(data: &[u8]) -> Result<Option<RgbaImage>, VtfError> {
    let parsed = header::parse(data)?;
    let (Some(format), Some(range)) = (parsed.info.low_res_format, parsed.low_res_range()?) else {
        return Ok(None);
    };
    let bytes = header::slice_range(data, range)?;
    convert_to_rgba8(format, bytes, parsed.info.low_res_width, parsed.info.low_res_height).map(Some)
}

pub fn required_len(prefix: &[u8], opts: &DecodeOptions) -> Result<usize, VtfError> {
    let parsed = header::parse(prefix)?;
    let mip = resolve_mip(&parsed.info, opts.mip)?;
    let range = parsed.level_range(mip, opts.frame, opts.face)?;
    usize::try_from(range.end).map_err(|_| VtfError::TooLarge)
}

#[cfg(test)]
mod tests;
