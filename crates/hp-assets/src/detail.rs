use hp_render::DetailDesc;
use hp_vmt::{Material, ShaderKind};

use crate::material::texture_transform_of;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DetailAnalysis {
    pub texture: String,
    pub mode: u32,
    pub raw: bool,
    pub sets_alpha: bool,
    pub uv: [f32; 4],
    pub uv_cross: [f32; 2],
    pub scroll: [f32; 2],
    pub tint_blend: [f32; 4],
}

impl DetailAnalysis {
    pub(crate) fn desc(&self, texture: hp_render::TexId) -> DetailDesc {
        DetailDesc {
            texture,
            mode: self.mode,
            raw: self.raw,
            sets_alpha: self.sets_alpha,
            uv: self.uv,
            uv_cross: self.uv_cross,
            scroll: self.scroll,
            tint_blend: self.tint_blend,
        }
    }
}

fn detail_scale(mat: &Material) -> [f32; 2] {
    let ok = |v: f32| v.is_finite() && v.abs() < 1e4;
    let Some(text) = mat.get("$detailscale") else { return [4.0; 2] };
    let nums: Vec<f32> = text
        .split(|c: char| c == '[' || c == ']' || c == '{' || c == '}' || c.is_whitespace())
        .filter_map(|w| w.parse::<f32>().ok())
        .filter(|v| ok(*v))
        .collect();
    match nums.as_slice() {
        [s] => [*s; 2],
        [u, v, ..] => [*u, *v],
        [] => [4.0; 2],
    }
}

fn gamma_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub(crate) fn analyze_detail(mat: &Material, texture_flags: &dyn Fn(&str) -> u32) -> Option<DetailAnalysis> {
    let texture = mat.texture("$detail")?;
    mat.base_texture()?;
    let kind = mat.shader_kind();
    let scale = detail_scale(mat);
    let factor = mat.get_f32("$detailblendfactor").filter(|v| v.is_finite()).unwrap_or(1.0);
    let tint = mat.get_vec3("$detailtint").filter(|c| c.iter().all(|v| v.is_finite())).unwrap_or([1.0; 3]);
    let mode = mat.get_i32("$detailblendmode").unwrap_or(0);
    let (mode, raw, sets_alpha, tint, transform) = match kind {
        ShaderKind::LightmappedGeneric | ShaderKind::WorldVertexTransition | ShaderKind::Lightmapped4WayBlend => {
            let ssbump = texture_flags(&texture) & hp_vtf::flags::SSBUMP != 0;
            let mode = match (ssbump, mat.bump_map().is_some()) {
                (true, true) => 10,
                (true, false) => DetailDesc::MODE_SSBUMP_NOBUMP as i32,
                (false, _) => mode,
            };
            if !matches!(mode, 0..=4 | 7..=11) {
                return None;
            }
            (mode as u32, mode != 1, false, tint, [1.0, 1.0, 0.0, 0.0])
        }
        ShaderKind::WorldTwoTextureBlend => {
            let mode = if mat.flag("$detail_alpha_mask_base_texture") {
                DetailDesc::MODE_TWO_TEXTURE_MASK
            } else {
                DetailDesc::MODE_TWO_TEXTURE_LERP
            };
            (mode, true, false, [1.0; 3], [1.0, 1.0, 0.0, 0.0])
        }
        ShaderKind::VertexLitGeneric | ShaderKind::UnlitGeneric => {
            if !(0..=9).contains(&mode) || (kind == ShaderKind::VertexLitGeneric && mat.flag("$treesway")) {
                return None;
            }
            let transform = if mat.has("$detailtexturetransform") {
                texture_transform_of(mat, "$detailtexturetransform")
            } else {
                [1.0, 1.0, 0.0, 0.0]
            };
            (mode as u32, mode == 0, true, tint.map(gamma_to_linear), transform)
        }
        _ => return None,
    };
    let key = crate::material_uv::detail_key(mat);
    let transform = key.map_or(transform, |key| texture_transform_of(mat, key));
    let factor = if kind == ShaderKind::WorldTwoTextureBlend { 1.0 } else { factor };
    let uv = [transform[0] * scale[0], transform[1] * scale[1], transform[2] * scale[0], transform[3] * scale[1]];
    let cross = key.map_or([0.0; 2], |key| crate::material::texture_transform_cross_of(mat, key));
    let uv_cross = [cross[0] * scale[0], cross[1] * scale[1]];
    let scroll = key.map_or([0.0; 2], |key| crate::material::texture_scroll_of(mat, key));
    Some(DetailAnalysis {
        texture,
        mode,
        raw,
        sets_alpha,
        uv,
        uv_cross,
        scroll: [scroll[0] * scale[0], scroll[1] * scale[1]],
        tint_blend: [tint[0], tint[1], tint[2], factor],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(src: &str, flags: u32) -> Option<DetailAnalysis> {
        analyze_detail(&hp_vmt::parse(src).unwrap(), &|_| flags)
    }

    #[test]
    fn brush_details() {
        let src = r#"LightmappedGeneric { $basetexture a $detail "detail/noise_detail_01" $detailblendfactor .8 }"#;
        let d = detail(src, 0).unwrap();
        assert_eq!(d.texture, "detail/noise_detail_01");
        assert_eq!((d.mode, d.raw, d.sets_alpha), (0, true, false));
        assert_eq!(d.uv, [4.0, 4.0, 0.0, 0.0]);
        assert_eq!(d.tint_blend, [1.0, 1.0, 1.0, 0.8]);
        let src = r#"LightmappedGeneric { $basetexture a $detail d $detailblendmode 1 $detailscale "[2 3]"
            $detailtint "[.5 .5 .5]" }"#;
        let d = detail(src, 0).unwrap();
        assert_eq!((d.mode, d.raw), (1, false));
        assert_eq!(d.uv, [2.0, 3.0, 0.0, 0.0]);
        assert_eq!(d.tint_blend, [0.5, 0.5, 0.5, 1.0]);
        assert!(detail(r#"WorldVertexTransition { $basetexture a $basetexture2 b $detail d }"#, 0).is_some());
        assert!(detail(r#"LightmappedGeneric { $basetexture a $detail d $detailblendmode 5 }"#, 0).is_none());
        let ss = hp_vtf::flags::SSBUMP;
        assert_eq!(detail(r#"LightmappedGeneric { $basetexture a $detail d }"#, ss).unwrap().mode, 11);
        let over_bump = detail(r#"LightmappedGeneric { $basetexture a $detail d $bumpmap n }"#, ss).unwrap();
        assert_eq!((over_bump.mode, over_bump.raw), (DetailDesc::MODE_SSBUMP_BUMP, true));
        assert!(detail(r#"LightmappedGeneric { $detail d }"#, 0).is_none());
        let two = detail(r#"WorldTwoTextureBlend { $basetexture a $detail d $detailscale 2 }"#, 0).unwrap();
        assert_eq!((two.mode, two.raw), (DetailDesc::MODE_TWO_TEXTURE_LERP, true));
        assert_eq!(two.uv, [2.0, 2.0, 0.0, 0.0]);
        let mask = detail(r#"WorldTwoTextureBlend { $basetexture a $detail d $detail_alpha_mask_base_texture 1 }"#, 0);
        assert_eq!(mask.unwrap().mode, DetailDesc::MODE_TWO_TEXTURE_MASK);
    }

    #[test]
    fn model_details() {
        let src = r#"VertexLitGeneric { $basetexture a $detail d $detailblendmode 6 $detailtint "[0.5 1 0]" }"#;
        let d = detail(src, 0).unwrap();
        assert_eq!((d.mode, d.raw, d.sets_alpha), (6, false, true));
        assert!((d.tint_blend[0] - 0.214_041).abs() < 1e-4, "gamma to linear: {:?}", d.tint_blend);
        assert!(detail(r#"VertexLitGeneric { $basetexture a $detail d }"#, 0).unwrap().raw);
        let src = r#"VertexLitGeneric { $basetexture a $detail d $detailscale 2
            $detailtexturetransform "center .5 .5 scale 2 2 rotate 0 translate .25 0" }"#;
        let t = detail(src, 0).unwrap();
        assert_eq!(t.uv, [4.0, 4.0, -0.5, -1.0]);
        assert!(detail(r#"VertexLitGeneric { $basetexture a $detail d $treesway 1 }"#, 0).is_none());
        assert!(detail(r#"VertexLitGeneric { $basetexture a $detail d $detailblendmode 12 }"#, 0).is_none());
        assert_eq!(detail(r#"UnlitGeneric { $basetexture a $detail d $detailblendmode 9 }"#, 0).unwrap().mode, 9);
        assert!(detail(r#"UnlitTwoTexture { $basetexture a $detail d }"#, 0).is_none());
    }
}
