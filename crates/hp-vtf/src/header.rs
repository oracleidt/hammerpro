use std::ops::Range;

use crate::{ImageFormat, VtfError, flags};

pub const SIGNATURE: [u8; 4] = *b"VTF\0";

pub const MAX_RESOURCES: u32 = 32;

pub const MAX_HEADER_LEN: usize = 80 + 8 * MAX_RESOURCES as usize;

pub(crate) const RESOURCE_LOW_RES: [u8; 3] = [0x01, 0, 0];
pub(crate) const RESOURCE_HIGH_RES: [u8; 3] = [0x30, 0, 0];

const FIXED_HEADER_LEN: usize = 80;

#[derive(Debug, Clone, PartialEq)]
pub struct VtfInfo {
    pub version: (u32, u32),
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub frames: u32,
    pub first_frame: u32,
    pub faces: u32,
    pub mip_count: u32,
    pub format: ImageFormat,
    pub flags: u32,
    pub reflectivity: [f32; 3],
    pub bumpmap_scale: f32,
    pub low_res_format: Option<ImageFormat>,
    pub low_res_width: u32,
    pub low_res_height: u32,
    pub header_size: u32,
}

impl VtfInfo {
    #[must_use]
    pub fn mip_dimensions(&self, mip: u32) -> (u32, u32, u32) {
        (shrink(self.width, mip), shrink(self.height, mip), shrink(self.depth, mip))
    }

    #[must_use]
    pub fn has_flag(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    #[must_use]
    pub fn is_cubemap(&self) -> bool {
        self.faces > 1
    }

    #[must_use]
    pub fn mip_for_max_dimension(&self, max_dim: u32) -> u32 {
        let max_dim = max_dim.max(1);
        (0..self.mip_count)
            .find(|&m| {
                let (w, h, _) = self.mip_dimensions(m);
                w.max(h) <= max_dim
            })
            .unwrap_or(self.mip_count.saturating_sub(1))
    }

    #[must_use]
    pub fn source_mip_count(&self) -> u32 {
        if self.has_flag(flags::NOMIP) {
            return 1;
        }
        let mut count = 1;
        while count < self.mip_count {
            let (w, h, _) = self.mip_dimensions(count - 1);
            if w <= 32 || h <= 32 {
                break;
            }
            count += 1;
        }
        count
    }

    #[must_use]
    pub fn slice_size(&self, mip: u32) -> Option<u64> {
        let (w, h, _) = self.mip_dimensions(mip);
        self.format.image_size(w, h)
    }
}

pub(crate) fn shrink(v: u32, mip: u32) -> u32 {
    v.checked_shr(mip).unwrap_or(0).max(1)
}

#[derive(Debug, Clone)]
pub(crate) struct Parsed {
    pub info: VtfInfo,
    pub hi_offset: u64,
    pub low_offset: Option<u64>,
}

#[inline]
fn u16_at<const O: usize>(h: &[u8; FIXED_HEADER_LEN]) -> u16 {
    u16::from_le_bytes([h[O], h[O + 1]])
}

#[inline]
fn u32_at<const O: usize>(h: &[u8; FIXED_HEADER_LEN]) -> u32 {
    u32::from_le_bytes([h[O], h[O + 1], h[O + 2], h[O + 3]])
}

#[inline]
fn f32_at<const O: usize>(h: &[u8; FIXED_HEADER_LEN]) -> f32 {
    f32::from_bits(u32_at::<O>(h))
}

fn truncated(needed: u64, available: usize) -> VtfError {
    VtfError::Truncated { needed, available: available as u64 }
}

fn image_data_size(info: &VtfInfo, faces: u32) -> Option<u64> {
    let per_mip_count = u64::from(info.frames).checked_mul(u64::from(faces))?;
    let mut total = 0u64;
    for mip in 0..info.mip_count {
        let (_, _, d) = info.mip_dimensions(mip);
        let level = info.slice_size(mip)?.checked_mul(u64::from(d))?.checked_mul(per_mip_count)?;
        total = total.checked_add(level)?;
    }
    Some(total)
}

pub(crate) fn parse(data: &[u8]) -> Result<Parsed, VtfError> {
    let mut h = [0u8; FIXED_HEADER_LEN];
    let n = data.len().min(FIXED_HEADER_LEN);
    if let (Some(dst), Some(src)) = (h.get_mut(..n), data.get(..n)) {
        dst.copy_from_slice(src);
    }
    if n < 16 {
        if data.len() >= 4 && h[..4] != SIGNATURE {
            return Err(VtfError::BadSignature);
        }
        return Err(truncated(16, data.len()));
    }
    if h[..4] != SIGNATURE {
        return Err(VtfError::BadSignature);
    }
    let major = u32_at::<4>(&h);
    let minor = u32_at::<8>(&h);
    if major != 7 || minor > 5 {
        return Err(VtfError::UnsupportedVersion(major, minor));
    }
    let min_len: usize = match minor {
        0 | 1 => 63,
        2 => 65,
        _ => FIXED_HEADER_LEN,
    };
    if data.len() < min_len {
        return Err(truncated(min_len as u64, data.len()));
    }

    let header_size = u32_at::<12>(&h);
    let width = u32::from(u16_at::<16>(&h));
    let height = u32::from(u16_at::<18>(&h));
    if width == 0 || height == 0 {
        return Err(VtfError::InvalidDimensions { width, height });
    }
    let tex_flags = u32_at::<20>(&h);
    let frames = u32::from(u16_at::<24>(&h)).max(1);
    let first_frame = u32::from(u16_at::<26>(&h));
    let reflectivity = [f32_at::<32>(&h), f32_at::<36>(&h), f32_at::<40>(&h)];
    let bumpmap_scale = f32_at::<48>(&h);
    let raw_format = u32_at::<52>(&h);
    let format = ImageFormat::from_raw(raw_format).ok_or(VtfError::UnknownFormat(raw_format))?;
    let mip_count = u32::from(h[56]).max(1);
    let raw_low_format = u32_at::<57>(&h);
    let low_res_width = u32::from(h[61]);
    let low_res_height = u32::from(h[62]);
    let depth = if minor >= 2 { u32::from(u16_at::<63>(&h)).max(1) } else { 1 };

    let mut hi_offset = None;
    let mut low_offset = None;
    let mut header_len = u64::from(header_size);
    if minor >= 3 {
        let count = u32_at::<68>(&h);
        if count > MAX_RESOURCES {
            return Err(VtfError::Corrupt("too many resources"));
        }
        let dict_end = FIXED_HEADER_LEN + 8 * count as usize;
        let dict = data.get(FIXED_HEADER_LEN..dict_end).ok_or_else(|| truncated(dict_end as u64, data.len()))?;
        for entry in dict.as_chunks::<8>().0 {
            let tag = [entry[0], entry[1], entry[2]];
            let offset = u64::from(u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]));
            if tag == RESOURCE_HIGH_RES {
                hi_offset = Some(offset);
            } else if tag == RESOURCE_LOW_RES {
                low_offset = Some(offset);
            }
        }
        header_len = header_len.max(dict_end as u64);
    }

    let low_res_format = if raw_low_format == u32::MAX || low_res_width == 0 || low_res_height == 0 {
        None
    } else {
        match ImageFormat::from_raw(raw_low_format) {
            Some(f) => Some(f),
            None if hi_offset.is_some() => None,
            None => return Err(VtfError::UnknownFormat(raw_low_format)),
        }
    };

    let hi_offset = match hi_offset {
        Some(o) => o,
        None => {
            if (header_size as usize) < min_len {
                return Err(VtfError::Corrupt("header size smaller than the header"));
            }
            let low_size = match low_res_format {
                Some(f) => f.image_size(low_res_width, low_res_height).ok_or(VtfError::TooLarge)?,
                None => 0,
            };
            if low_res_format.is_some() {
                low_offset = Some(header_len);
            }
            header_len.checked_add(low_size).ok_or(VtfError::TooLarge)?
        }
    };
    if low_res_format.is_none() {
        low_offset = None;
    }

    let mut info = VtfInfo {
        version: (major, minor),
        width,
        height,
        depth,
        frames,
        first_frame,
        faces: 1,
        mip_count,
        format,
        flags: tex_flags,
        reflectivity,
        bumpmap_scale,
        low_res_format,
        low_res_width,
        low_res_height,
        header_size,
    };

    if tex_flags & flags::ENVMAP != 0 {
        let faces = if minor < 5 && first_frame != 0xFFFF { 7 } else { 6 };
        info.faces = faces;
        if faces == 7 {
            let available = (data.len() as u64).saturating_sub(hi_offset);
            let fits = |f| image_data_size(&info, f).is_some_and(|s| s <= available);
            if !fits(7) && fits(6) {
                info.faces = 6;
            }
        }
    }
    image_data_size(&info, info.faces).ok_or(VtfError::TooLarge)?;

    Ok(Parsed { info, hi_offset, low_offset })
}

impl Parsed {
    pub fn level_range(&self, mip: u32, frame: u32, face: u32) -> Result<Range<u64>, VtfError> {
        let info = &self.info;
        if mip >= info.mip_count {
            return Err(VtfError::MipOutOfRange { index: mip, count: info.mip_count });
        }
        if frame >= info.frames {
            return Err(VtfError::FrameOutOfRange { index: frame, count: info.frames });
        }
        if face >= info.faces {
            return Err(VtfError::FaceOutOfRange { index: face, count: info.faces });
        }
        let images_per_mip = u64::from(info.frames) * u64::from(info.faces);
        let mip_bytes = |m: u32| -> Option<u64> {
            let (_, _, d) = info.mip_dimensions(m);
            info.slice_size(m)?.checked_mul(u64::from(d))
        };
        let mut offset = self.hi_offset;
        for m in (mip + 1..info.mip_count).rev() {
            let level = mip_bytes(m).and_then(|b| b.checked_mul(images_per_mip)).ok_or(VtfError::TooLarge)?;
            offset = offset.checked_add(level).ok_or(VtfError::TooLarge)?;
        }
        let image_index = u64::from(frame) * u64::from(info.faces) + u64::from(face);
        let skip = mip_bytes(mip).and_then(|b| b.checked_mul(image_index)).ok_or(VtfError::TooLarge)?;
        let start = offset.checked_add(skip).ok_or(VtfError::TooLarge)?;
        let len = info.slice_size(mip).ok_or(VtfError::TooLarge)?;
        let end = start.checked_add(len).ok_or(VtfError::TooLarge)?;
        Ok(start..end)
    }

    pub fn low_res_range(&self) -> Result<Option<Range<u64>>, VtfError> {
        let (Some(format), Some(start)) = (self.info.low_res_format, self.low_offset) else {
            return Ok(None);
        };
        let len = format.image_size(self.info.low_res_width, self.info.low_res_height).ok_or(VtfError::TooLarge)?;
        let end = start.checked_add(len).ok_or(VtfError::TooLarge)?;
        Ok(Some(start..end))
    }
}

pub(crate) fn slice_range(data: &[u8], range: Range<u64>) -> Result<&[u8], VtfError> {
    let start = usize::try_from(range.start).map_err(|_| VtfError::TooLarge)?;
    let end = usize::try_from(range.end).map_err(|_| VtfError::TooLarge)?;
    data.get(start..end).ok_or_else(|| truncated(range.end, data.len()))
}
