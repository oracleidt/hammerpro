pub const POINTSAMPLE: u32 = 0x0000_0001;
pub const TRILINEAR: u32 = 0x0000_0002;
pub const CLAMPS: u32 = 0x0000_0004;
pub const CLAMPT: u32 = 0x0000_0008;
pub const ANISOTROPIC: u32 = 0x0000_0010;
pub const HINT_DXT5: u32 = 0x0000_0020;
pub const SRGB: u32 = 0x0000_0040;
pub const NORMAL: u32 = 0x0000_0080;
pub const NOMIP: u32 = 0x0000_0100;
pub const NOLOD: u32 = 0x0000_0200;
pub const ALL_MIPS: u32 = 0x0000_0400;
pub const PROCEDURAL: u32 = 0x0000_0800;
pub const ONEBITALPHA: u32 = 0x0000_1000;
pub const EIGHTBITALPHA: u32 = 0x0000_2000;
pub const ENVMAP: u32 = 0x0000_4000;
pub const RENDERTARGET: u32 = 0x0000_8000;
pub const DEPTHRENDERTARGET: u32 = 0x0001_0000;
pub const NODEBUGOVERRIDE: u32 = 0x0002_0000;
pub const SINGLECOPY: u32 = 0x0004_0000;
pub const PRE_SRGB: u32 = 0x0008_0000;
pub const NODEPTHBUFFER: u32 = 0x0080_0000;
pub const CLAMPU: u32 = 0x0200_0000;
pub const VERTEXTEXTURE: u32 = 0x0400_0000;
pub const SSBUMP: u32 = 0x0800_0000;
pub const BORDER: u32 = 0x2000_0000;

pub const NAMED: &[(u32, &str)] = &[
    (POINTSAMPLE, "point_sample"),
    (TRILINEAR, "trilinear"),
    (CLAMPS, "clamp_s"),
    (CLAMPT, "clamp_t"),
    (CLAMPU, "clamp_u"),
    (ANISOTROPIC, "anisotropic"),
    (HINT_DXT5, "hint_dxt5"),
    (SRGB, "srgb"),
    (NORMAL, "normal"),
    (NOMIP, "no_mip"),
    (NOLOD, "no_lod"),
    (ALL_MIPS, "all_mips"),
    (PROCEDURAL, "procedural"),
    (ONEBITALPHA, "one_bit_alpha"),
    (EIGHTBITALPHA, "eight_bit_alpha"),
    (ENVMAP, "env_map"),
    (RENDERTARGET, "render_target"),
    (DEPTHRENDERTARGET, "depth_render_target"),
    (NODEBUGOVERRIDE, "no_debug_override"),
    (SINGLECOPY, "single_copy"),
    (PRE_SRGB, "pre_srgb"),
    (NODEPTHBUFFER, "no_depth_buffer"),
    (VERTEXTEXTURE, "vertex_texture"),
    (SSBUMP, "ss_bump"),
    (BORDER, "border"),
];

#[must_use]
pub fn from_id(id: &str) -> Option<u32> {
    let id = id.trim().to_ascii_lowercase();
    NAMED.iter().find(|(_, n)| *n == id).map(|(b, _)| *b)
}

#[must_use]
pub fn id_of(bit: u32) -> Option<&'static str> {
    NAMED.iter().find(|(b, _)| *b == bit).map(|(_, n)| *n)
}

#[must_use]
pub fn names(flags: u32) -> Vec<&'static str> {
    NAMED.iter().filter(|(b, _)| flags & b != 0).map(|(_, n)| *n).collect()
}
