use hp_render::{EnvmapAlphaMask, EnvmapDesc};
use hp_vmt::{Material, ShaderKind};

use crate::material::{TexLook, texture_transform_cross_of, texture_transform_of};

pub(crate) const MASK_SUFFIX: &str = "|mask";
pub(crate) const CUBE_SUFFIX: &str = "|cube";
pub(crate) const MAX_CUBE_FACE: u32 = 256;

pub(crate) fn mask_key(name: &str, frame: u32) -> String {
    if frame == 0 { format!("{name}{MASK_SUFFIX}") } else { format!("{name}{MASK_SUFFIX}{frame}") }
}

pub(crate) fn parse_mask_key(key: &str) -> Option<(&str, u32)> {
    let at = key.rfind(MASK_SUFFIX)?;
    let (name, rest) = (&key[..at], &key[at + MASK_SUFFIX.len()..]);
    let frame = if rest.is_empty() { 0 } else { rest.parse().ok()? };
    (!name.is_empty()).then_some((name, frame))
}

pub(crate) fn cube_key(name: &str) -> String {
    format!("{name}{CUBE_SUFFIX}")
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MaskSource {
    Texture { name: String, frame: u32 },
    NormalAlpha { name: String, frame: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EnvmapAnalysis {
    pub texture: Option<String>,
    pub tint: [f32; 3],
    pub contrast: f32,
    pub saturation: [f32; 3],
    pub mask: Option<MaskSource>,
    pub mask_uv: [f32; 4],
    pub mask_uv_cross: [f32; 2],
    pub mask_scroll: [f32; 2],
    pub alpha_mask: EnvmapAlphaMask,
    pub fresnel: f32,
    pub phong_fresnel: f32,
    pub fresnel_ranges: [f32; 3],
    pub light_scale: f32,
    pub no_envmap: [bool; 2],
    pub parallax: bool,
}

impl EnvmapAnalysis {
    pub(crate) fn keys(&self, normal_maps: bool) -> Vec<String> {
        let mut keys = Vec::new();
        if let Some(t) = &self.texture {
            keys.push(cube_key(t));
        }
        if let Some(m) = &self.mask {
            keys.push(self.mask_registry_key(m, normal_maps));
        }
        keys
    }

    fn mask_registry_key(&self, m: &MaskSource, normal_maps: bool) -> String {
        match m {
            MaskSource::Texture { name, frame } => mask_key(name, *frame),
            MaskSource::NormalAlpha { name, frame } if normal_maps => crate::bump::normal_key(name, *frame),
            MaskSource::NormalAlpha { name, frame } => mask_key(name, *frame),
        }
    }

    pub(crate) fn desc(&self, tex: &dyn Fn(&str) -> TexLook, normal_maps: bool) -> Option<EnvmapDesc> {
        let ready = |key: &str| match tex(key) {
            TexLook::Ready(id, _) => Some(id),
            TexLook::Loading | TexLook::Missing => None,
        };
        let texture = match &self.texture {
            Some(t) => Some(ready(&cube_key(t))?),
            None => None,
        };
        let mask = match &self.mask {
            Some(m) => Some(ready(&self.mask_registry_key(m, normal_maps))?),
            None => None,
        };
        Some(EnvmapDesc {
            cubemap: None,
            texture,
            tint: self.tint,
            contrast: self.contrast,
            saturation: self.saturation,
            mask,
            mask_alpha: matches!(self.mask, Some(MaskSource::NormalAlpha { .. })),
            mask_uv: self.mask_uv,
            mask_uv_cross: self.mask_uv_cross,
            mask_scroll: self.mask_scroll,
            alpha_mask: self.alpha_mask,
            fresnel: self.fresnel,
            phong_fresnel: self.phong_fresnel,
            fresnel_ranges: self.fresnel_ranges,
            light_scale: self.light_scale,
            no_envmap: self.no_envmap,
            parallax: self.parallax,
        })
    }
}

fn num(mat: &Material, key: &str, default: f32) -> f32 {
    mat.get_f32(key).filter(|v| v.is_finite()).unwrap_or(default)
}

fn vec3(mat: &Material, key: &str, default: [f32; 3]) -> [f32; 3] {
    mat.get_vec3(key).filter(|v| v.iter().all(|c| c.is_finite())).unwrap_or(default)
}

fn frame(mat: &Material, key: &str) -> u32 {
    mat.get_i32(key).and_then(|f| u32::try_from(f).ok()).unwrap_or(0).min(1023)
}

pub(crate) fn analyze_envmap(mat: &Material) -> Option<EnvmapAnalysis> {
    let kind = mat.shader_kind();
    let lightmapped = matches!(
        kind,
        ShaderKind::LightmappedGeneric
            | ShaderKind::WorldVertexTransition
            | ShaderKind::Lightmapped4WayBlend
            | ShaderKind::WorldTwoTextureBlend
    );
    let vertexlit = matches!(kind, ShaderKind::VertexLitGeneric | ShaderKind::UnlitGeneric);
    if !lightmapped && !vertexlit {
        return None;
    }
    let envmap = mat.env_map()?;
    let texture = (!envmap.eq_ignore_ascii_case("env_cubemap")).then_some(envmap);
    let bump = mat.bump_map();
    let bump_frame = frame(mat, "$bumpframe");
    let phong = kind == ShaderKind::VertexLitGeneric && mat.flag("$phong") && bump.is_some();
    let mut alpha_mask = EnvmapAlphaMask::None;
    let mut mask = None;
    let mask_key = crate::material_uv::env_mask_key(mat);
    let normal_key = crate::material_uv::bump_keys(mat).0;
    let mut mask_uv = texture_transform_of(mat, mask_key);
    let mut mask_uv_cross = texture_transform_cross_of(mat, mask_key);
    let mut effective_key = mask_key;
    if phong {
        if mat.flag("$basealphaenvmapmask") || mat.flag("$basemapalphaphongmask") {
            alpha_mask = EnvmapAlphaMask::Base;
        } else if let Some(name) = bump.clone() {
            mask = Some(MaskSource::NormalAlpha { name, frame: bump_frame });
            mask_uv = texture_transform_of(mat, normal_key);
            mask_uv_cross = texture_transform_cross_of(mat, normal_key);
            effective_key = normal_key;
        }
    } else {
        if mat.flag("$normalmapalphaenvmapmask")
            && let Some(name) = bump.clone()
        {
            mask = Some(MaskSource::NormalAlpha { name, frame: bump_frame });
            mask_uv = texture_transform_of(mat, normal_key);
            mask_uv_cross = texture_transform_cross_of(mat, normal_key);
            effective_key = normal_key;
        } else if let Some(name) = mat.texture("$envmapmask") {
            mask = Some(MaskSource::Texture { name, frame: frame(mat, "$envmapmaskframe") });
        }
        if mat.flag("$basealphaenvmapmask") {
            alpha_mask = EnvmapAlphaMask::InverseBase;
        }
    }
    let wvt = kind == ShaderKind::WorldVertexTransition;
    Some(EnvmapAnalysis {
        texture,
        tint: vec3(mat, "$envmaptint", [1.0; 3]).map(|v| v.clamp(0.0, 64.0)),
        contrast: num(mat, "$envmapcontrast", 0.0).clamp(0.0, 1.0),
        saturation: vec3(mat, "$envmapsaturation", [1.0; 3]).map(|v| v.clamp(-8.0, 8.0)),
        mask,
        mask_uv,
        mask_uv_cross,
        mask_scroll: crate::material::texture_scroll_of(mat, effective_key),
        alpha_mask,
        fresnel: if lightmapped { num(mat, "$fresnelreflection", 1.0).clamp(0.0, 1.0) } else { 1.0 },
        phong_fresnel: if phong { num(mat, "$envmapfresnel", 0.0).clamp(0.0, 1.0) } else { 0.0 },
        fresnel_ranges: vec3(mat, "$phongfresnelranges", [0.0, 0.5, 1.0]),
        light_scale: if lightmapped { num(mat, "$envmaplightscale", 0.0).clamp(0.0, 1.0) } else { 0.0 },
        no_envmap: [wvt && mat.flag("$basetexturenoenvmap"), wvt && mat.flag("$basetexture2noenvmap")],
        parallax: matches!(kind, ShaderKind::LightmappedGeneric | ShaderKind::WorldVertexTransition),
    })
}

pub(crate) fn cube_strip(bytes: &[u8]) -> Result<(u32, Vec<u8>), String> {
    let info = hp_vtf::read_info(bytes).map_err(|e| e.to_string())?;
    if !info.is_cubemap() {
        return Err("not a cubemap".to_owned());
    }
    let mip = info.mip_for_max_dimension(MAX_CUBE_FACE);
    let mut size = 0;
    let mut out = Vec::new();
    for face in 0..6 {
        let level = hp_vtf::raw_level(bytes, mip, 0, face).map_err(|e| e.to_string())?;
        let img = level.to_rgba8().map_err(|e| e.to_string())?;
        if img.width != img.height || img.width == 0 || (size != 0 && img.width != size) {
            return Err(format!("face {face} is {}x{}", img.width, img.height));
        }
        size = img.width;
        out.extend_from_slice(&img.pixels);
    }
    Ok((size, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_render::TexId;

    fn env(src: &str) -> Option<EnvmapAnalysis> {
        analyze_envmap(&hp_vmt::parse(src).unwrap())
    }

    #[test]
    fn lightmapped_generic_reflects_like_the_sdk_shader() {
        let e = env(r#"LightmappedGeneric { "$basetexture" "metal/floor" "$envmap" "env_cubemap"
            "$envmaptint" "[.5 .25 1]" "$envmapcontrast" 1 "$envmapsaturation" ".5" "$envmapmask" "metal/floor_mask"
            "$envmapmasktransform" "center .5 .5 scale 2 2 rotate 0 translate 0 0" "$fresnelreflection" ".3"
            "$envmaplightscale" ".75" }"#)
        .unwrap();
        assert_eq!(e.texture, None, "env_cubemap: the surface's cubemap");
        assert_eq!(e.tint, [0.5, 0.25, 1.0]);
        assert_eq!((e.contrast, e.saturation), (1.0, [0.5; 3]));
        assert_eq!(e.mask, Some(MaskSource::Texture { name: "metal/floor_mask".into(), frame: 0 }));
        assert_eq!(e.mask_uv, [2.0, 2.0, -0.5, -0.5]);
        assert_eq!((e.fresnel, e.light_scale), (0.3, 0.75));
        assert_eq!(e.alpha_mask, EnvmapAlphaMask::None);
        assert_eq!(e.keys(true), ["metal/floor_mask|mask"]);
        let d = env(r#"LightmappedGeneric { "$basetexture" "a" "$envmap" "env_cubemap" }"#).unwrap();
        assert_eq!((d.tint, d.contrast, d.saturation, d.fresnel), ([1.0; 3], 0.0, [1.0; 3], 1.0));
        assert!(d.keys(true).is_empty());
        assert!(d.parallax);
        let wvt = env(r#"WorldVertexTransition { "$basetexture" "a" "$envmap" "env_cubemap" }"#).unwrap();
        let prop = env(r#"VertexLitGeneric { "$basetexture" "a" "$envmap" "env_cubemap" }"#).unwrap();
        assert!(wvt.parallax && !prop.parallax);
        assert!(env(r#"LightmappedGeneric { "$basetexture" "a" }"#).is_none());
        assert!(env(r#"Water { "$envmap" "env_cubemap" }"#).is_none());
        assert!(env(r#"Refract { "$envmap" "env_cubemap" }"#).is_none());
    }

    #[test]
    fn alpha_masks_follow_the_shaders() {
        let base = env(r#"LightmappedGeneric { "$basetexture" "a" "$envmap" "env_cubemap" "$basealphaenvmapmask" 1 }"#)
            .unwrap();
        assert_eq!(base.alpha_mask, EnvmapAlphaMask::InverseBase);
        let normal = env(
            r#"LightmappedGeneric { "$basetexture" "a" "$bumpmap" "a_n" "$envmap" "env_cubemap" "$normalmapalphaenvmapmask" 1
               "$basetexturetransform" "center .5 .5 scale 4 4 rotate 0 translate 0 0"
               "$bumptransform" "center .5 .5 scale 8 8 rotate 0 translate 0 0" }"#,
        )
        .unwrap();
        assert_eq!(normal.mask, Some(MaskSource::NormalAlpha { name: "a_n".into(), frame: 0 }));
        assert_eq!(normal.mask_uv, [4.0, 4.0, -1.5, -1.5], "the normal map coordinate");
        assert_eq!(normal.keys(true), ["a_n|normal"], "the normal map already loaded");
        assert_eq!(normal.keys(false), ["a_n|mask"], "loaded for the mask alone");
        let none =
            env(r#"VertexLitGeneric { "$basetexture" "a" "$envmap" "env_cubemap" "$normalmapalphaenvmapmask" 1 }"#)
                .unwrap();
        assert_eq!(none.mask, None);
        let phong = env(r#"VertexLitGeneric { "$basetexture" "m" "$bumpmap" "m_n" "$phong" 1 "$envmap" "env_cubemap"
               "$envmapfresnel" 1 "$phongfresnelranges" "[.1 .5 1]" "$envmapmask" "ignored" }"#)
        .unwrap();
        assert_eq!(phong.mask, Some(MaskSource::NormalAlpha { name: "m_n".into(), frame: 0 }));
        assert_eq!((phong.phong_fresnel, phong.fresnel_ranges), (1.0, [0.1, 0.5, 1.0]));
        assert_eq!(phong.fresnel, 1.0, "no $fresnelreflection on models");
        let phong_base = env(
            r#"VertexLitGeneric { "$basetexture" "m" "$bumpmap" "m_n" "$phong" 1 "$envmap" "env_cubemap" "$basealphaenvmapmask" 1 }"#,
        )
        .unwrap();
        assert_eq!((phong_base.mask, phong_base.alpha_mask), (None, EnvmapAlphaMask::Base));
        let wvt = env(
            r#"WorldVertexTransition { "$basetexture" "a" "$basetexture2" "b" "$envmap" "env_cubemap" "$basetexturenoenvmap" 1 }"#,
        )
        .unwrap();
        assert_eq!(wvt.no_envmap, [true, false]);
    }

    #[test]
    fn named_cubemaps_wait_for_their_textures() {
        let e = env(r#"VertexLitGeneric { "$basetexture" "car" "$envmap" "Environment Maps/PipeSet_ND" "$envmaptint" "[.3 .3 .3]" }"#)
            .unwrap();
        assert_eq!(e.texture.as_deref(), Some("environment maps/pipeset_nd"));
        assert_eq!(e.keys(true), ["environment maps/pipeset_nd|cube"]);
        let loading = |_: &str| TexLook::Loading;
        assert!(e.desc(&loading, true).is_none(), "no reflection while the cubemap loads");
        let ready = |_: &str| TexLook::Ready(TexId::WHITE, [4, 24]);
        let d = e.desc(&ready, true).unwrap();
        assert_eq!((d.texture, d.cubemap), (Some(TexId::WHITE), None));
        assert_eq!(d.tint, [0.3; 3]);
        let m = env(r#"LightmappedGeneric { "$basetexture" "a" "$envmap" "env_cubemap" "$envmapmask" "m" }"#).unwrap();
        let only_mask = |k: &str| if k == "m|mask" { TexLook::Ready(TexId::WHITE, [1, 1]) } else { TexLook::Missing };
        let d = m.desc(&only_mask, true).unwrap();
        assert_eq!((d.mask, d.mask_alpha, d.texture), (Some(TexId::WHITE), false, None));
        assert!(m.desc(&|_| TexLook::Missing, true).is_none());
    }

    #[test]
    fn mask_keys_round_trip() {
        assert_eq!(parse_mask_key(&mask_key("a/b", 0)), Some(("a/b", 0)));
        assert_eq!(parse_mask_key(&mask_key("a/b", 3)), Some(("a/b", 3)));
        assert_eq!(parse_mask_key("a/b"), None);
        assert_eq!(parse_mask_key("|mask"), None);
        assert_eq!(cube_key("x"), "x|cube");
    }
}
