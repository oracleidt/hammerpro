use hp_render::BumpDesc;
use hp_vmt::{Material, ShaderKind};

use crate::material::{TexLook, texture_scroll_of, texture_transform_cross_of, texture_transform_of};

pub(crate) const NORMAL_SUFFIX: &str = "|normal";

pub(crate) const MAX_NORMAL_DIM: u32 = 1024;

pub(crate) fn normal_key(name: &str, frame: u32) -> String {
    if frame == 0 { format!("{name}{NORMAL_SUFFIX}") } else { format!("{name}{NORMAL_SUFFIX}{frame}") }
}

pub(crate) fn parse_normal_key(key: &str) -> Option<(&str, u32)> {
    let at = key.rfind(NORMAL_SUFFIX)?;
    let (name, rest) = (&key[..at], &key[at + NORMAL_SUFFIX.len()..]);
    let frame = if rest.is_empty() { 0 } else { rest.parse().ok()? };
    (!name.is_empty()).then_some((name, frame))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BumpAnalysis {
    pub texture: String,
    pub texture2: Option<String>,
    pub blend4: Option<[f32; 4]>,
    pub frame: u32,
    pub ssbump: bool,
    pub diffuse: bool,
    pub uv: [f32; 4],
    pub uv_cross: [f32; 2],
    pub scroll: [f32; 2],
}

impl BumpAnalysis {
    pub(crate) fn keys(&self) -> Vec<String> {
        let mut keys = vec![normal_key(&self.texture, self.frame)];
        if let Some(t) = &self.texture2 {
            keys.push(normal_key(t, self.frame));
        }
        keys
    }

    pub(crate) fn desc(&self, tex: &dyn Fn(&str) -> TexLook) -> Option<BumpDesc> {
        let ready = |name: &str| match tex(&normal_key(name, self.frame)) {
            TexLook::Ready(id, _) => Some(id),
            TexLook::Loading | TexLook::Missing => None,
        };
        let texture = ready(&self.texture)?;
        let texture2 = self.texture2.as_deref().and_then(ready);
        Some(BumpDesc {
            texture,
            texture2,
            blend4: self.blend4,
            ssbump: self.ssbump,
            diffuse: self.diffuse,
            uv: self.uv,
            uv_cross: self.uv_cross,
            scroll: self.scroll,
        })
    }
}

pub(crate) fn analyze_bump(mat: &Material) -> Option<BumpAnalysis> {
    let kind = mat.shader_kind();
    let lit = matches!(
        kind,
        ShaderKind::LightmappedGeneric
            | ShaderKind::WorldVertexTransition
            | ShaderKind::Lightmapped4WayBlend
            | ShaderKind::WorldTwoTextureBlend
            | ShaderKind::VertexLitGeneric
    );
    if !lit {
        return None;
    }
    let texture = mat.bump_map()?;
    let four = kind == ShaderKind::Lightmapped4WayBlend;
    let texture2 = if kind == ShaderKind::WorldVertexTransition || four { mat.texture("$bumpmap2") } else { None };
    let blend4 = four.then(|| four_way_bump_factors(mat));
    let frame = mat.get_i32("$bumpframe").and_then(|f| u32::try_from(f).ok()).unwrap_or(0).min(1023);
    Some(BumpAnalysis {
        texture,
        texture2,
        blend4,
        frame,
        ssbump: mat.flag("$ssbump"),
        diffuse: !mat.flag("$nodiffusebumplighting"),
        uv: texture_transform_of(mat, "$bumptransform"),
        uv_cross: texture_transform_cross_of(mat, "$bumptransform"),
        scroll: texture_scroll_of(mat, "$bumptransform"),
    })
}

pub(crate) const DEFAULT_BUMP_FACTORS: [f32; 4] = [0.0, 1.0, 0.0, 0.0];

fn four_way_bump_factors(mat: &Material) -> [f32; 4] {
    std::array::from_fn(|i| {
        mat.get_f32(&format!("$texture{}_bumpblendfactor", i + 1))
            .filter(|v| v.is_finite())
            .map_or(DEFAULT_BUMP_FACTORS[i], |v| v.clamp(0.0, 1.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_render::TexId;

    fn bump(src: &str) -> Option<BumpAnalysis> {
        analyze_bump(&hp_vmt::parse(src).unwrap())
    }

    #[test]
    fn bump_keys_of_the_source_shaders() {
        let b = bump(
            r#"LightmappedGeneric { "$basetexture" "brick/brickwall031d" "$bumpmap" "Brick/BrickWall031d_Normal" }"#,
        )
        .unwrap();
        assert_eq!(b.texture, "brick/brickwall031d_normal");
        assert_eq!((b.texture2.as_deref(), b.frame, b.ssbump), (None, 0, false));
        assert_eq!(b.uv, [1.0, 1.0, 0.0, 0.0]);
        let n = bump(r#"VertexLitGeneric { "$basetexture" "a" "$normalmap" "a_n" }"#).unwrap();
        assert_eq!(n.texture, "a_n");
        let s = bump(
            r#"LightmappedGeneric { "$basetexture" "c/c" "$bumpmap" "c/c_ssbump" "$ssbump" 1 "$bumpframe" 2
               "$bumptransform" "center .5 .5 scale 2 4 rotate 0 translate 0 0"
               Proxies { TextureScroll { texturescrollvar $bumptransform texturescrollrate 1 texturescrollangle 0 } } }"#,
        )
        .unwrap();
        assert!(s.ssbump);
        assert_eq!(s.frame, 2);
        assert_eq!(s.uv, [2.0, 4.0, -0.5, -1.5]);
        assert!((s.scroll[0] - 1.0).abs() < 1e-6 && s.scroll[1].abs() < 1e-6);
        assert_eq!(s.keys(), ["c/c_ssbump|normal2"]);
        let reflection_only =
            bump(r#"LightmappedGeneric { "$basetexture" "a" "$bumpmap" "n" "$nodiffusebumplighting" 1 }"#).unwrap();
        assert!(!reflection_only.diffuse);
        assert_eq!(reflection_only.texture, "n");
        assert!(b.diffuse);
        assert!(bump(r#"UnlitGeneric { "$basetexture" "a" "$bumpmap" "n" }"#).is_none());
        assert!(bump(r#"Water { "$normalmap" "n" }"#).is_none());
        assert!(bump(r#"LightmappedGeneric { "$basetexture" "a" }"#).is_none());
    }

    #[test]
    fn world_vertex_transition_blends_two_normal_maps() {
        let b = bump(
            r#"WorldVertexTransition { "$basetexture" "nature/grass" "$basetexture2" "nature/rock"
               "$bumpmap" "nature/grass_n" "$bumpmap2" "nature/rock_n" }"#,
        )
        .unwrap();
        assert_eq!(b.texture2.as_deref(), Some("nature/rock_n"));
        assert_eq!(b.keys(), ["nature/grass_n|normal", "nature/rock_n|normal"]);
        assert_eq!(b.blend4, None, "the vertex alpha blends them");
        let one = bump(r#"LightmappedGeneric { "$basetexture" "a" "$bumpmap" "n" "$bumpmap2" "m" }"#).unwrap();
        assert_eq!(one.texture2, None);
        let ready = |k: &str| {
            if k == "nature/grass_n|normal" { TexLook::Ready(TexId::WHITE, [4, 4]) } else { TexLook::Loading }
        };
        let d = b.desc(&ready).unwrap();
        assert_eq!(d.texture, TexId::WHITE);
        assert_eq!(d.texture2, None, "the first one stands for both while the second loads");
        assert!(b.desc(&|_| TexLook::Loading).is_none(), "flat until the first one is loaded");
    }

    #[test]
    fn four_way_blends_mix_their_normal_maps_by_texture() {
        let b = bump(
            r#"Lightmapped_4WayBlend { "$basetexture" "a/1" "$basetexture2" "a/2" "$basetexture3" "a/3"
               "$basetexture4" "a/4" "$bumpmap" "a/1_n" "$bumpmap2" "a/2_n" "$bumpmap3" "a/3_n" }"#,
        )
        .unwrap();
        assert_eq!(b.texture, "a/1_n");
        assert_eq!(b.texture2.as_deref(), Some("a/2_n"));
        assert_eq!(b.blend4, Some(DEFAULT_BUMP_FACTORS));
        assert_eq!(b.keys(), ["a/1_n|normal", "a/2_n|normal"]);
        let f = bump(
            r#"Lightmapped_4WayBlend { "$basetexture" "a/1" "$bumpmap" "a/1_n" "$bumpmap2" "a/2_n"
               "$texture1_bumpblendfactor" "0.25" "$texture2_bumpblendfactor" "0"
               "$texture3_bumpblendfactor" "7" "$texture4_bumpblendfactor" "nope" }"#,
        )
        .unwrap();
        assert_eq!(f.blend4, Some([0.25, 0.0, 1.0, 0.0]));
        let one = bump(r#"Lightmapped_4WayBlend { "$basetexture" "a/1" "$bumpmap" "a/1_n" }"#).unwrap();
        assert_eq!((one.texture2.as_deref(), one.blend4), (None, Some(DEFAULT_BUMP_FACTORS)));
        let ready = |_: &str| TexLook::Ready(TexId::WHITE, [4, 4]);
        let d = b.desc(&ready).unwrap();
        assert_eq!((d.texture2, d.blend4), (Some(TexId::WHITE), Some(DEFAULT_BUMP_FACTORS)));
    }

    #[test]
    fn patch_materials_keep_their_bump_keys() {
        let patch = hp_vmt::parse(
            r#"Patch { include "materials/brick/base.vmt" insert { "$bumpmap" "brick/base_normal" "$ssbump" 1 } }"#,
        )
        .unwrap();
        let base = r#"LightmappedGeneric { "$basetexture" "brick/base" }"#;
        let resolved = hp_vmt::resolve_patch(&patch, &|_| Some(base.to_owned())).unwrap();
        let b = analyze_bump(&resolved).unwrap();
        assert_eq!(b.texture, "brick/base_normal");
        assert!(b.ssbump);
    }

    #[test]
    fn normal_keys_round_trip() {
        assert_eq!(parse_normal_key(&normal_key("a/b", 0)), Some(("a/b", 0)));
        assert_eq!(parse_normal_key(&normal_key("a/b", 7)), Some(("a/b", 7)));
        assert_eq!(parse_normal_key("a/b"), None);
        assert_eq!(parse_normal_key("|normal"), None);
        assert_eq!(parse_normal_key("a|normalx"), None);
    }
}
