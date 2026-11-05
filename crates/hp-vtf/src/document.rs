use crate::compose::{ComposeOptions, can_encode, compose};
use crate::ops::{ResizeFilter, resize};
use crate::{
    DecodeOptions, EncodeQuality, ImageFormat, MipFilter, MipSelect, RgbaImage, VtfError, VtfInfo, decode_rgba8,
    decode_slice_rgba8, flags, patch_header, read_info,
};

#[derive(Debug, Clone)]
pub struct VtfDocument {
    pub frames: Vec<Vec<RgbaImage>>,
    pub flags: u32,
    pub version_minor: u32,
    pub format: ImageFormat,
    pub mipmaps: bool,
    pub bumpmap_scale: f32,
    pub reflectivity: [f32; 3],
    pub auto_reflectivity: bool,
    pub first_frame: u16,
    pub quality: EncodeQuality,
    pub mip_filter: MipFilter,
    pub resize_to_pow2: bool,
    pub recompute_alpha_flags: bool,
    info: Option<VtfInfo>,
    original: Option<Vec<u8>>,
    pub pixels_dirty: bool,
    pub rebuild: bool,
}

fn default_format(img: &RgbaImage) -> ImageFormat {
    if img.is_opaque() { ImageFormat::Dxt1 } else { ImageFormat::Dxt5 }
}

impl VtfDocument {
    pub fn from_bytes(data: &[u8]) -> Result<Self, VtfError> {
        let info = read_info(data)?;
        if !info.format.is_decodable() {
            return Err(VtfError::UnsupportedFormat(info.format));
        }
        let mut frames = Vec::with_capacity(info.frames as usize);
        for frame in 0..info.frames {
            let mut faces = Vec::with_capacity(info.faces as usize);
            for face in 0..info.faces {
                faces.push(decode_rgba8(data, &DecodeOptions { mip: MipSelect::Largest, frame, face })?);
            }
            frames.push(faces);
        }
        let format = if can_encode(info.format) {
            info.format
        } else if info.format.has_alpha() {
            ImageFormat::Dxt5
        } else {
            ImageFormat::Dxt1
        };
        Ok(Self {
            frames,
            flags: info.flags,
            version_minor: info.version.1.min(5),
            format,
            mipmaps: info.mip_count > 1,
            bumpmap_scale: info.bumpmap_scale,
            reflectivity: info.reflectivity,
            auto_reflectivity: false,
            first_frame: if info.faces == 6 && info.version.1 < 5 && info.first_frame == 0xFFFF {
                0
            } else {
                info.first_frame.min(u32::from(u16::MAX)) as u16
            },
            quality: EncodeQuality::Normal,
            mip_filter: MipFilter::FromFlags,
            resize_to_pow2: true,
            recompute_alpha_flags: true,
            info: Some(info),
            original: Some(data.to_vec()),
            pixels_dirty: false,
            rebuild: false,
        })
    }

    #[must_use]
    pub fn from_image(img: RgbaImage) -> Self {
        let format = default_format(&img);
        Self {
            frames: vec![vec![img]],
            flags: 0,
            version_minor: 4,
            format,
            mipmaps: true,
            bumpmap_scale: 1.0,
            reflectivity: [0.5; 3],
            auto_reflectivity: true,
            first_frame: 0,
            quality: EncodeQuality::Normal,
            mip_filter: MipFilter::FromFlags,
            resize_to_pow2: true,
            recompute_alpha_flags: true,
            info: None,
            original: None,
            pixels_dirty: true,
            rebuild: true,
        }
    }

    #[must_use]
    pub fn info(&self) -> Option<&VtfInfo> {
        self.info.as_ref()
    }

    #[must_use]
    pub fn original(&self) -> Option<&[u8]> {
        self.original.as_deref()
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.frames.first().and_then(|f| f.first()).map_or(0, |i| i.width)
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.frames.first().and_then(|f| f.first()).map_or(0, |i| i.height)
    }

    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub fn face_count(&self) -> usize {
        self.frames.first().map_or(1, Vec::len)
    }

    #[must_use]
    pub fn depth(&self) -> u32 {
        if self.rebuild { 1 } else { self.info.as_ref().map_or(1, |i| i.depth) }
    }

    #[must_use]
    pub fn source_format(&self) -> Option<ImageFormat> {
        self.info.as_ref().map(|i| i.format)
    }

    #[must_use]
    pub fn is_volume(&self) -> bool {
        self.info.as_ref().is_some_and(|i| i.depth > 1)
    }

    #[must_use]
    pub fn pixels_editable(&self) -> bool {
        !self.is_volume()
    }

    #[must_use]
    pub fn image(&self, frame: usize, face: usize) -> Option<&RgbaImage> {
        self.frames.get(frame).and_then(|f| f.get(face))
    }

    #[must_use]
    pub fn mip_count(&self) -> u32 {
        if !self.rebuild
            && !self.pixels_dirty
            && let Some(i) = &self.info
        {
            return i.mip_count;
        }
        if self.mipmaps { 32 - self.width().max(self.height()).max(1).leading_zeros() } else { 1 }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.pixels_dirty || self.rebuild || self.header_changed()
    }

    #[must_use]
    pub fn header_changed(&self) -> bool {
        let Some(i) = &self.info else { return true };
        i.flags != self.flags
            || i.bumpmap_scale.to_bits() != self.bumpmap_scale.to_bits()
            || i.reflectivity.map(f32::to_bits) != self.reflectivity.map(f32::to_bits)
            || (i.first_frame != u32::from(self.first_frame) && !(i.faces == 6 && i.first_frame == 0xFFFF))
    }

    pub fn mip_image(&self, mip: u32, frame: usize, face: usize, slice: u32) -> Result<RgbaImage, VtfError> {
        if !self.pixels_dirty
            && !self.rebuild
            && let Some(data) = &self.original
        {
            return decode_slice_rgba8(data, mip, frame as u32, face as u32, slice);
        }
        let base = self
            .image(frame, face)
            .ok_or(VtfError::FrameOutOfRange { index: frame as u32, count: self.frames.len() as u32 })?;
        if mip == 0 {
            return Ok(base.clone());
        }
        let count = self.mip_count();
        if mip >= count {
            return Err(VtfError::MipOutOfRange { index: mip, count });
        }
        let (w, h) = ((base.width >> mip).max(1), (base.height >> mip).max(1));
        let filter = if self.flags & flags::NORMAL != 0 { ResizeFilter::Normal } else { ResizeFilter::Color };
        resize(base, w, h, filter)
    }

    pub fn map_images(&mut self, f: impl Fn(&RgbaImage) -> Result<RgbaImage, VtfError>) -> Result<(), VtfError> {
        self.check_editable()?;
        let mut next = Vec::with_capacity(self.frames.len());
        for frame in &self.frames {
            let mut faces = Vec::with_capacity(frame.len());
            for img in frame {
                faces.push(f(img)?);
            }
            next.push(faces);
        }
        let size = next.first().and_then(|f| f.first()).map(|i| (i.width, i.height));
        if next.iter().flatten().any(|i| Some((i.width, i.height)) != size) {
            return Err(VtfError::InvalidImage("the operation changed the size of only some images".into()));
        }
        self.frames = next;
        self.pixels_dirty = true;
        self.rebuild = true;
        Ok(())
    }

    pub fn map_image(
        &mut self,
        frame: usize,
        face: usize,
        f: impl Fn(&RgbaImage) -> Result<RgbaImage, VtfError>,
    ) -> Result<(), VtfError> {
        self.check_editable()?;
        let (w, h) = (self.width(), self.height());
        let src = self
            .image(frame, face)
            .ok_or(VtfError::FrameOutOfRange { index: frame as u32, count: self.frames.len() as u32 })?;
        let out = f(src)?;
        if (out.width, out.height) != (w, h) {
            return Err(VtfError::InvalidImage("the result must keep the size of the texture".into()));
        }
        self.frames[frame][face] = out;
        self.pixels_dirty = true;
        self.rebuild = true;
        Ok(())
    }

    pub fn replace_image(
        &mut self,
        frame: usize,
        face: usize,
        img: RgbaImage,
        filter: ResizeFilter,
    ) -> Result<(), VtfError> {
        self.check_editable()?;
        if frame >= self.frames.len() || face >= self.face_count() {
            return Err(VtfError::FrameOutOfRange { index: frame as u32, count: self.frames.len() as u32 });
        }
        if self.frames.len() == 1 && self.face_count() == 1 {
            self.frames[0][0] = img;
        } else {
            let (w, h) = (self.width(), self.height());
            self.frames[frame][face] = resize(&img, w, h, filter)?;
        }
        self.pixels_dirty = true;
        self.rebuild = true;
        Ok(())
    }

    pub fn resize_all(&mut self, width: u32, height: u32, filter: ResizeFilter) -> Result<(), VtfError> {
        self.map_images(|img| resize(img, width, height, filter))
    }

    pub fn add_frame(&mut self, img: Option<RgbaImage>, filter: ResizeFilter) -> Result<(), VtfError> {
        self.check_editable()?;
        if self.frames.len() >= usize::from(u16::MAX) {
            return Err(VtfError::InvalidImage("too many frames".into()));
        }
        let (w, h) = (self.width(), self.height());
        let faces = self.face_count();
        let new = match img {
            Some(i) => vec![resize(&i, w, h, filter)?; faces],
            None => self.frames.last().cloned().unwrap_or_default(),
        };
        self.frames.push(new);
        self.pixels_dirty = true;
        self.rebuild = true;
        Ok(())
    }

    pub fn remove_frame(&mut self, index: usize) -> Result<(), VtfError> {
        self.check_editable()?;
        if self.frames.len() <= 1 || index >= self.frames.len() {
            return Err(VtfError::InvalidImage("a texture keeps at least one frame".into()));
        }
        self.frames.remove(index);
        self.first_frame = self.first_frame.min((self.frames.len() - 1) as u16);
        self.pixels_dirty = true;
        self.rebuild = true;
        Ok(())
    }

    pub fn set_cubemap(&mut self, on: bool) -> Result<(), VtfError> {
        self.check_editable()?;
        let faces = self.face_count();
        if on == (faces > 1) {
            return Ok(());
        }
        for frame in &mut self.frames {
            if on {
                let first = frame[0].clone();
                *frame = vec![first; 6];
            } else {
                frame.truncate(1);
            }
        }
        if on {
            self.flags |= flags::ENVMAP;
        } else {
            self.flags &= !flags::ENVMAP;
        }
        self.pixels_dirty = true;
        self.rebuild = true;
        Ok(())
    }

    pub fn set_format(&mut self, format: ImageFormat) -> Result<(), VtfError> {
        if !can_encode(format) {
            return Err(VtfError::UnsupportedFormat(format));
        }
        self.check_editable()?;
        if self.format != format || self.source_format() != Some(format) {
            self.format = format;
            self.rebuild = true;
        }
        Ok(())
    }

    pub fn regenerate_mipmaps(&mut self, enabled: bool, filter: MipFilter) -> Result<(), VtfError> {
        self.check_editable()?;
        self.mipmaps = enabled;
        self.mip_filter = filter;
        self.rebuild = true;
        Ok(())
    }

    pub fn set_version(&mut self, minor: u32) -> Result<(), VtfError> {
        if !(1..=5).contains(&minor) {
            return Err(VtfError::UnsupportedVersion(7, minor));
        }
        self.check_editable()?;
        if self.version_minor != minor {
            self.version_minor = minor;
            self.rebuild = true;
        }
        Ok(())
    }

    fn check_editable(&self) -> Result<(), VtfError> {
        if self.is_volume() && !self.rebuild {
            return Err(VtfError::InvalidImage("volume textures (slices) can only have their header edited".into()));
        }
        Ok(())
    }

    pub fn mark_saved(&mut self, bytes: Vec<u8>) -> Result<(), VtfError> {
        let info = read_info(&bytes)?;
        self.flags = info.flags;
        self.bumpmap_scale = info.bumpmap_scale;
        self.reflectivity = info.reflectivity;
        self.version_minor = info.version.1.min(5);
        self.first_frame = if info.faces == 6 && info.first_frame == 0xFFFF {
            0
        } else {
            info.first_frame.min(u32::from(u16::MAX)) as u16
        };
        self.mipmaps = info.mip_count > 1;
        self.format = if can_encode(info.format) { info.format } else { self.format };
        self.info = Some(info);
        self.original = Some(bytes);
        self.pixels_dirty = false;
        self.rebuild = false;
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, VtfError> {
        if !self.pixels_dirty
            && !self.rebuild
            && let Some(data) = &self.original
        {
            if !self.header_changed() {
                return Ok(data.clone());
            }
            return patch_header(data, self.flags, self.bumpmap_scale, self.reflectivity, self.first_frame);
        }
        let opts = ComposeOptions {
            format: self.format,
            mipmaps: self.mipmaps,
            flags: self.flags,
            version_minor: self.version_minor,
            resize_to_pow2: self.resize_to_pow2,
            quality: self.quality,
            mip_filter: self.mip_filter,
            bumpmap_scale: self.bumpmap_scale,
            first_frame: self.first_frame,
            reflectivity: (!self.auto_reflectivity).then_some(self.reflectivity),
            recompute_alpha_flags: self.recompute_alpha_flags,
        };
        compose(&self.frames, &opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{AlphaSource, set_alpha};
    use crate::{EncodeOptions, encode};

    fn img(w: u32, h: u32) -> RgbaImage {
        let px: Vec<u8> = (0..w * h).flat_map(|i| [(i * 7 % 256) as u8, (i * 3 % 256) as u8, 90, 255]).collect();
        RgbaImage::new(w, h, px).unwrap()
    }

    fn sample() -> Vec<u8> {
        encode(&img(32, 16), &EncodeOptions { format: crate::EncodeFormat::Bgra8888, ..EncodeOptions::default() })
            .unwrap()
    }

    #[test]
    fn untouched_documents_save_the_same_bytes() {
        let data = sample();
        let doc = VtfDocument::from_bytes(&data).unwrap();
        assert!(!doc.is_dirty());
        assert_eq!(doc.to_bytes().unwrap(), data);
        assert_eq!((doc.width(), doc.height(), doc.mip_count()), (32, 16, 6));
    }

    #[test]
    fn header_edits_keep_the_pixels() {
        let data = sample();
        let mut doc = VtfDocument::from_bytes(&data).unwrap();
        doc.flags |= flags::CLAMPS | flags::NOLOD;
        doc.bumpmap_scale = 2.5;
        doc.reflectivity = [0.1, 0.2, 0.3];
        assert!(doc.is_dirty());
        let out = doc.to_bytes().unwrap();
        assert_eq!(out.len(), data.len());
        let info = read_info(&out).unwrap();
        assert_eq!(info.flags & (flags::CLAMPS | flags::NOLOD), flags::CLAMPS | flags::NOLOD);
        assert_eq!(info.bumpmap_scale, 2.5);
        assert_eq!(info.reflectivity, [0.1, 0.2, 0.3]);
        let a = decode_rgba8(&data, &DecodeOptions::default()).unwrap();
        let b = decode_rgba8(&out, &DecodeOptions::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn convert_resize_and_alpha() {
        let mut doc = VtfDocument::from_bytes(&sample()).unwrap();
        doc.set_format(ImageFormat::Dxt5).unwrap();
        doc.resize_all(16, 8, ResizeFilter::Color).unwrap();
        doc.map_images(|i| Ok(set_alpha(i, AlphaSource::Constant(128)))).unwrap();
        let out = doc.to_bytes().unwrap();
        let info = read_info(&out).unwrap();
        assert_eq!((info.width, info.height, info.format), (16, 8, ImageFormat::Dxt5));
        assert!(info.flags & flags::EIGHTBITALPHA != 0);
        doc.regenerate_mipmaps(false, MipFilter::Color).unwrap();
        assert_eq!(read_info(&doc.to_bytes().unwrap()).unwrap().mip_count, 1);
    }

    #[test]
    fn every_encodable_format_round_trips() {
        let base = img(16, 16);
        for &f in crate::compose::ENCODABLE {
            let mut doc = VtfDocument::from_image(base.clone());
            doc.format = f;
            let out = doc.to_bytes().unwrap();
            let info = read_info(&out).unwrap();
            assert_eq!(info.format, f);
            let back = decode_rgba8(&out, &DecodeOptions::default()).unwrap();
            assert_eq!((back.width, back.height), (16, 16), "{f:?}");
            if matches!(
                f,
                ImageFormat::Bgra8888 | ImageFormat::Rgba8888 | ImageFormat::Abgr8888 | ImageFormat::Argb8888
            ) {
                assert_eq!(back.pixels, base.pixels, "{f:?}");
            }
        }
    }

    #[test]
    fn frames_and_cubemaps() {
        let mut doc = VtfDocument::from_image(img(8, 8));
        doc.add_frame(Some(img(4, 4)), ResizeFilter::Color).unwrap();
        doc.add_frame(None, ResizeFilter::Color).unwrap();
        assert_eq!(doc.frame_count(), 3);
        let out = doc.to_bytes().unwrap();
        assert_eq!(read_info(&out).unwrap().frames, 3);
        let doc2 = VtfDocument::from_bytes(&out).unwrap();
        assert_eq!(doc2.frame_count(), 3);
        doc.remove_frame(2).unwrap();
        assert!(doc.remove_frame(5).is_err());
        let mut cube = VtfDocument::from_image(img(8, 8));
        cube.set_cubemap(true).unwrap();
        let out = cube.to_bytes().unwrap();
        let info = read_info(&out).unwrap();
        assert_eq!(info.faces, 6);
        assert!(info.flags & flags::ENVMAP != 0);
        let back = VtfDocument::from_bytes(&out).unwrap();
        assert_eq!(back.face_count(), 6);
        assert_eq!(back.image(0, 3).unwrap(), back.image(0, 0).unwrap());
        for minor in [3, 5] {
            cube.set_version(minor).unwrap();
            let info = read_info(&cube.to_bytes().unwrap()).unwrap();
            assert_eq!((info.faces, info.version.1), (6, minor));
        }
    }

    #[test]
    fn mip_views() {
        let mut doc = VtfDocument::from_bytes(&sample()).unwrap();
        let m2 = doc.mip_image(2, 0, 0, 0).unwrap();
        assert_eq!((m2.width, m2.height), (8, 4));
        doc.map_images(|i| Ok(set_alpha(i, AlphaSource::Constant(9)))).unwrap();
        let m2 = doc.mip_image(2, 0, 0, 0).unwrap();
        assert_eq!((m2.width, m2.height), (8, 4));
        assert!(doc.mip_image(9, 0, 0, 0).is_err());
    }
}
