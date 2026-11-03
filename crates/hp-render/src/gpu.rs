use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct GpuVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub nrm_alpha: u32,
    pub slot: u32,
    pub blend: u32,
    pub cube: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct EdgeVertex {
    pub pos: [f32; 3],
    pub slot: u32,
}

pub(crate) const DETAIL_BIT: u32 = 1 << 31;
pub(crate) const SOURCE_BIT: u32 = 1 << 30;
pub(crate) const CLASS_SHIFT: u32 = 29;
pub(crate) const SLOT_MASK: u32 = (1 << CLASS_SHIFT) - 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct ColorVertex {
    pub pos: [f32; 3],
    pub color: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct PointInstance {
    pub pos: [f32; 3],
    pub size: f32,
    pub color: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct SpriteGpu {
    pub pos: [f32; 3],
    pub size: f32,
    pub color: u32,
    pub slot: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct InstanceGpu {
    pub m0: [f32; 4],
    pub m1: [f32; 4],
    pub m2: [f32; 4],
    pub m3: [f32; 4],
    pub tint: [f32; 4],
    pub bmin: [f32; 3],
    pub slot: u32,
    pub bmax: [f32; 3],
    pub fade: [f32; 2],
    pub light: u32,
    pub cube: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub(crate) struct ObjState {
    pub flags: u32,
    pub color: u32,
    pub tint: u32,
    pub classic: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct MaterialUniform {
    pub tint: [f32; 4],
    pub inv_tex_size: [f32; 2],
    pub scroll: [f32; 2],
    pub alpha_scale: f32,
    pub flags: u32,
    pub has_tex2: u32,
    pub blend4: u32,
    pub alpha_ref: f32,
    pub detail_mode: u32,
    pub detail_flags: u32,
    pub pad: u32,
    pub detail_uv: [f32; 4],
    pub detail_tint: [f32; 4],
    pub bump_uv: [f32; 4],
    pub bump_scroll: [f32; 2],
    pub bump_flags: u32,
    pub bump_layers: u32,
    pub env_flags: u32,
    pub env_cube: u32,
    pub env_pad: [u32; 2],
    pub env_tint: [f32; 4],
    pub env_sat: [f32; 4],
    pub env_mask_uv: [f32; 4],
    pub env_fresnel: [f32; 4],
    pub env_ranges: [f32; 4],
    pub self_illum: [f32; 4],
    pub fw_lum_start: [f32; 4],
    pub fw_lum_end: [f32; 4],
    pub fw_blend_start: [f32; 4],
    pub fw_blend_end: [f32; 4],
    pub fw_lum_factor: [f32; 4],
    pub fw_mod2x: [f32; 4],
    pub fw_uv12: [f32; 4],
    pub fw_uv34: [f32; 4],
    pub base_uv: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct LightGpu {
    pub pos_range: [f32; 4],
    pub color: [f32; 4],
    pub dir: [f32; 4],
    pub cone: [f32; 4],
    pub atten: [f32; 4],
    pub extra: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct ViewUniform {
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub right: [f32; 4],
    pub up: [f32; 4],
    pub forward: [f32; 4],
    pub viewport: [f32; 4],
    pub selection: [f32; 4],
    pub grid_a: [f32; 4],
    pub grid_b: [f32; 4],
    pub ortho: [f32; 4],
    pub ambient: [f32; 4],
    pub cluster_dims: [u32; 4],
    pub cluster_z: [f32; 4],
    pub misc: [u32; 4],
    pub shadow_vp: [[f32; 4]; 4],
    pub shadow: [f32; 4],
    pub opts: [u32; 4],
    pub grid3d: [f32; 4],
    pub fog_a: [f32; 4],
    pub fog_b: [f32; 4],
    pub fog_c: [f32; 4],
    pub sky: [f32; 4],
    pub sky3d: [f32; 4],
    pub proj: [f32; 4],
    pub sky_uv: [[f32; 4]; 6],
    pub sky_vis_a: [f32; 4],
    pub sky_vis_b: [f32; 4],
    pub sun_soft: [f32; 4],
    pub sky_paint: [[f32; 4]; 6],
    pub indirect_a: [f32; 4],
    pub indirect_b: [f32; 4],
}

pub(crate) const BLEND_PAINTED: u32 = 1 << 30;

pub(crate) fn pack_blend(weights: [f32; 4]) -> u32 {
    let q = |v: f32| if v.is_finite() { (v.clamp(0.0, 1.0) * 1023.0).round() as u32 } else { 0 };
    q(weights[1]) | (q(weights[2]) << 10) | (q(weights[3]) << 20) | BLEND_PAINTED
}

pub(crate) fn pack_blend_vertex(b: &crate::types::BlendVertex) -> u32 {
    if b.painted { pack_blend(b.weights) } else { 0 }
}

pub(crate) fn pack_normal_alpha(n: [f32; 3], alpha: f32) -> u32 {
    let s = |v: f32| {
        let v = if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        ((v * 127.0).round() as i8) as u8 as u32
    };
    let a = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 1.0 };
    let a = (a * 255.0).round() as u32;
    s(n[0]) | (s(n[1]) << 8) | (s(n[2]) << 16) | (a << 24)
}

pub(crate) fn pack_rgba(c: [f32; 4]) -> u32 {
    let q = |v: f32| {
        let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        (v * 255.0).round() as u32
    };
    q(c[0]) | (q(c[1]) << 8) | (q(c[2]) << 16) | (q(c[3]) << 24)
}

pub(crate) fn pack_rgb8(c: [u8; 3]) -> u32 {
    u32::from(c[0]) | (u32::from(c[1]) << 8) | (u32::from(c[2]) << 16) | (255 << 24)
}

pub(crate) fn srgb_to_linear(c: f32) -> f32 {
    let c = if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.0 };
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub(crate) fn gamma_to_linear(c: f32) -> f32 {
    let c = if c.is_finite() { c.max(0.0) } else { 0.0 };
    if c >= 0.95 { 1.0 } else { c.powf(2.2) }
}

pub(crate) fn srgba_to_linear(c: [f32; 4]) -> [f32; 4] {
    let a = if c[3].is_finite() { c[3].clamp(0.0, 1.0) } else { 1.0 };
    [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), a]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_shaders() {
        assert_eq!(size_of::<GpuVertex>(), 36);
        assert_eq!(size_of::<EdgeVertex>(), 16);
        assert_eq!(size_of::<ColorVertex>(), 16);
        assert_eq!(size_of::<PointInstance>(), 20);
        assert_eq!(size_of::<SpriteGpu>(), 24);
        assert_eq!(size_of::<InstanceGpu>(), 124);
        assert_eq!(size_of::<ObjState>(), 16);
        assert_eq!(size_of::<MaterialUniform>(), 384);
        assert_eq!(size_of::<LightGpu>(), 96);
        assert_eq!(size_of::<ViewUniform>(), 64 * 2 + 16 * 39);
    }

    #[test]
    fn blend_packing() {
        assert_eq!(pack_blend([1.0, 0.0, 0.0, 0.0]), BLEND_PAINTED, "texture 1 only");
        let p = pack_blend([0.0, 2.0, 0.5, 0.25]);
        assert_eq!((p & 1023, (p >> 10) & 1023, (p >> 20) & 1023), (1023, 512, 256), "raw layer amounts, clamped");
        assert_eq!(p & BLEND_PAINTED, BLEND_PAINTED);
        assert_eq!(pack_blend([f32::NAN, -1.0, f32::INFINITY, f32::NAN]), BLEND_PAINTED);
        assert_eq!(pack_blend_vertex(&crate::types::BlendVertex::default()), 0, "unpainted: the vertex alpha blends");
        assert_eq!(pack_blend_vertex(&crate::types::BlendVertex::painted([0.0, 1.0, 0.0, 0.0])), 1023 | BLEND_PAINTED);
    }

    #[test]
    fn packing() {
        let p = pack_normal_alpha([0.0, 0.0, 1.0], 1.0);
        assert_eq!(p & 0xff, 0);
        assert_eq!((p >> 16) & 0xff, 127);
        assert_eq!(p >> 24, 255);
        let n = pack_normal_alpha([-1.0, f32::NAN, 0.0], f32::NAN);
        assert_eq!(n & 0xff, 0x81);
        assert_eq!((n >> 8) & 0xff, 0);
        assert_eq!(pack_rgba([1.0, 0.0, 0.5, 1.0]), 0xff80_00ff);
        assert_eq!(pack_rgb8([1, 2, 3]), 0xff03_0201);
        assert!((srgb_to_linear(0.5) - 0.214).abs() < 1e-3);
        assert!((gamma_to_linear(0.5) - 0.5f32.powf(2.2)).abs() < 1e-6);
        assert_eq!(gamma_to_linear(0.96), 1.0);
        assert_eq!(gamma_to_linear(-1.0), 0.0);
    }
}
