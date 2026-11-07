//! Which material matrix the native shader actually samples for each layer.
//! A declared `$bumptransform` is not necessarily the primary normal's UV.
//! See tests/data/native_uv_routing.json and its native render probe.
use hp_vmt::{Material, ShaderKind};

pub(crate) const BASE: &str = "$basetexturetransform";

pub(crate) fn bump_keys(mat: &Material) -> (&'static str, &'static str) {
    if mat.shader_kind() == ShaderKind::Lightmapped4WayBlend {
        // This shader has its own normal coordinates, unlike LightmappedGeneric.
        return ("$bumptransform", "$bumptransform");
    }
    let has = |key| mat.texture(key).is_some();
    let mask = mat.shader_kind() == ShaderKind::LightmappedGeneric
        && has("$bumpmap2")
        && has("$bumpmask")
        && !has("$detail")
        && !has("$basetexture2")
        && !mat.flag("$selfillum")
        && !mat.flag("$basetexturenoenvmap");
    if mask {
        return ("$bumptransform", "$bumptransform2");
    }
    if mat.shader_kind() == ShaderKind::WorldVertexTransition && !has("$detail") {
        (BASE, "$bumptransform")
    } else {
        (BASE, BASE)
    }
}

pub(crate) fn detail_key(mat: &Material) -> Option<&'static str> {
    match mat.shader_kind() {
        ShaderKind::LightmappedGeneric
        | ShaderKind::WorldVertexTransition
        | ShaderKind::WorldTwoTextureBlend
        | ShaderKind::Lightmapped4WayBlend => Some(BASE),
        ShaderKind::VertexLitGeneric | ShaderKind::UnlitGeneric => Some("$detailtexturetransform"),
        _ => None,
    }
}

pub(crate) fn env_mask_key(mat: &Material) -> &'static str {
    match mat.shader_kind() {
        ShaderKind::VertexLitGeneric | ShaderKind::UnlitGeneric => BASE,
        _ => "$envmapmasktransform",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_layers_follow_the_native_rendered_uv_variants() {
        let report: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/native_uv_routing.json")).unwrap();
        assert!(report["errors"].as_array().unwrap().is_empty());
        let native_matrix: [[f32; 3]; 2] = std::array::from_fn(|row| {
            std::array::from_fn(|col| report["transforms"][row][col].as_str().unwrap().parse().unwrap())
        });
        let ready = |_: &str| crate::material::TexLook::Ready(hp_render::TexId::WHITE, [128; 2]);
        let mut verified = 0;
        for case in report["cases"].as_array().unwrap() {
            let Some(layer) = case["tested_layer"].as_str() else { continue };
            let key = case["sampled_transform"].as_str().unwrap();
            let samples = case["variants"]["identity"].as_array().unwrap();
            let values: Vec<u64> =
                samples.iter().flat_map(|p| p.as_array().unwrap().iter().map(|v| v.as_u64().unwrap())).collect();
            assert!(
                values.iter().max().unwrap() - values.iter().min().unwrap() > 30,
                "uninformative native case {case}"
            );
            for variant in ["identity", "parameter", "base"] {
                let changed_key = match variant {
                    "parameter" => case["parameter"].as_str().unwrap(),
                    "base" => BASE,
                    _ => "",
                };
                let changed = changed_key == key;
                let pixels = case["variants"][variant].as_array().unwrap();
                let delta = samples
                    .iter()
                    .zip(pixels)
                    .flat_map(|(a, b)| {
                        a.as_array()
                            .unwrap()
                            .iter()
                            .zip(b.as_array().unwrap())
                            .map(|(a, b)| a.as_u64().unwrap().abs_diff(b.as_u64().unwrap()))
                    })
                    .max()
                    .unwrap();
                assert_eq!(delta > 20, changed, "native evidence disagrees with route: {} {variant}", case["name"]);
                let mut properties = case["properties"].as_object().unwrap().clone();
                if variant != "identity" {
                    properties.insert(changed_key.into(), "center .5 .5 scale 1 1 rotate 90 translate .25 0".into());
                }
                let body: String =
                    properties.iter().map(|(k, v)| format!("\"{k}\" \"{}\"\n", v.as_str().unwrap())).collect();
                let mat = hp_vmt::parse(&format!("{} {{ {body} }}", case["shader"].as_str().unwrap())).unwrap();
                let analysis = crate::material::analyze("native/uv", &mat, &|_| 0);
                let (uv, cross) = match layer {
                    "bump" => {
                        let b = analysis.bump.unwrap().desc(&ready).unwrap();
                        (b.uv, b.uv_cross)
                    }
                    "bump2" => {
                        let b = analysis.bump.unwrap().desc(&ready).unwrap();
                        (b.uv2.unwrap(), b.uv2_cross)
                    }
                    "detail" => {
                        let d = analysis.detail.unwrap().desc(hp_render::TexId::WHITE);
                        (d.uv, d.uv_cross)
                    }
                    "envmask" => {
                        assert_eq!(case["env"]["texture"], "hp_uvrouting117/white_cube");
                        assert_eq!(case["env"]["width"], 4);
                        let e = analysis.envmap.unwrap().desc(&ready, true).unwrap();
                        (e.mask_uv, e.mask_uv_cross)
                    }
                    "base" | "base2" => (analysis.uv, analysis.uv_cross),
                    _ => panic!("unknown fixture layer"),
                };
                let expected = if changed { native_matrix } else { [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] };
                let actual = [[uv[0], cross[0], uv[2]], [cross[1], uv[1], uv[3]]];
                for row in 0..2 {
                    for col in 0..3 {
                        assert!(
                            (actual[row][col] - expected[row][col]).abs() < 2e-6,
                            "{} {variant}: {actual:?} vs {expected:?}",
                            case["name"]
                        );
                    }
                }
            }
            verified += 1;
        }
        assert_eq!(verified, 22);
    }

    #[test]
    fn scrolling_uses_the_same_matrix_route_as_the_texture() {
        let src = r#"WorldVertexTransition { "$basetexture" "a" "$basetexture2" "b" "$bumpmap" "n" "$bumpmap2" "n2"
            Proxies {
                TextureScroll { texturescrollvar $basetexturetransform texturescrollrate 2 }
                TextureScroll { texturescrollvar $bumptransform texturescrollrate 3 }
            } }"#;
        let mat = hp_vmt::parse(src).unwrap();
        let b = crate::bump::analyze_bump(&mat).unwrap();
        assert_eq!(b.scroll, [2.0, 0.0]);
        assert_eq!(b.scroll2, [3.0, 0.0]);
        let with_detail =
            hp_vmt::parse(&src.replace("Proxies", "\"$detail\" \"d\" \"$detailscale\" \"4\" Proxies")).unwrap();
        let b = crate::bump::analyze_bump(&with_detail).unwrap();
        assert_eq!((b.scroll, b.scroll2), ([2.0, 0.0], [2.0, 0.0]));
        let d = crate::detail::analyze_detail(&with_detail, &|_| 0).unwrap();
        assert_eq!(d.scroll, [8.0, 0.0]);
        let model = hp_vmt::parse(
            r#"VertexLitGeneric { "$basetexture" "a" "$detail" "d" "$envmap" "env_cubemap" "$envmapmask" "m"
            Proxies { TextureScroll { texturescrollvar $basetexturetransform texturescrollrate 2 } } }"#,
        )
        .unwrap();
        assert_eq!(crate::detail::analyze_detail(&model, &|_| 0).unwrap().scroll, [0.0; 2]);
        assert_eq!(crate::envmap::analyze_envmap(&model).unwrap().mask_scroll, [2.0, 0.0]);
    }
}
