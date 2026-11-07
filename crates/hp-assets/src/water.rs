use hp_render::{TexId, WaterBottom, WaterDesc};
use hp_vmt::Material;

use crate::material::TexLook;
use crate::texture::{MAX_TEXTURE_DIM, PreparedTexture, build_mips};

pub(crate) const FRAMES_SUFFIX: &str = "|frames";

const MAX_FRAMES: u32 = 64;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WaterAnalysis {
    pub desc: WaterDesc,
    pub normal_map: Option<String>,
    pub base_texture: Option<String>,
    pub animated: Option<f32>,
    pub bottom_material: Option<String>,
}

fn num(mat: &Material, key: &str) -> Option<f32> {
    mat.get_f32(key).filter(|v| v.is_finite())
}

fn color(mat: &Material, key: &str, default: [f32; 3]) -> [f32; 3] {
    mat.get(key).and_then(hp_vmt::parse_vec3).filter(|c| c.iter().all(|v| v.is_finite())).unwrap_or(default)
}

fn vec2(mat: &Material, key: &str) -> [f32; 2] {
    let Some(text) = mat.get(key) else { return [0.0; 2] };
    let inner = text.trim().trim_start_matches(['[', '{']).trim_end_matches([']', '}']);
    let mut it = inner.split_ascii_whitespace().filter_map(|w| w.parse::<f32>().ok()).filter(|v| v.is_finite());
    match (it.next(), it.next()) {
        (Some(x), Some(y)) => [x, y],
        (Some(x), None) => [x, x],
        _ => [0.0; 2],
    }
}

fn fog(mat: &Material) -> ([f32; 3], [f32; 2]) {
    let c = color(mat, "$fogcolor", [1.0, 0.0, 0.0]);
    (c, [num(mat, "$fogstart").unwrap_or(0.0), num(mat, "$fogend").unwrap_or(0.0)])
}

fn normal_animation(mat: &Material) -> Option<f32> {
    let proxies = mat.proxies.as_ref()?;
    proxies.children_named("animatedtexture").find_map(|p| {
        let var = p.get("animatedtexturevar").unwrap_or("").trim();
        if !var.eq_ignore_ascii_case("$normalmap") {
            return None;
        }
        let rate = p.get("animatedtextureframerate").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(15.0);
        Some(if rate.is_finite() { rate.clamp(0.0, 240.0) } else { 15.0 })
    })
}

pub(crate) fn analyze_water(mat: &Material) -> Option<WaterAnalysis> {
    if mat.shader_kind() != hp_vmt::ShaderKind::Water {
        return None;
    }
    let force_cheap = mat.flag("$forcecheap");
    let expensive = !force_cheap && mat.get_bool("$forceexpensive").unwrap_or(true);
    let refract = mat.has("$refracttexture");
    let reflect = expensive && mat.has("$reflecttexture");
    let envmap = mat.has("$envmap") && !mat.flag("$decal");
    let (fog_color, fog_range) = fog(mat);
    let texture = |key: &str| mat.texture(key).filter(|t| !t.starts_with("_rt_"));
    let desc = WaterDesc {
        normal_map: None,
        frames: [1, 1, 1],
        frame_rate: 0.0,
        bump_uv: crate::material::texture_transform_of(mat, "$bumptransform"),
        bump_scroll: crate::material::texture_scroll_of(mat, "$bumptransform"),
        scroll1: vec2(mat, "$scroll1"),
        scroll2: vec2(mat, "$scroll2"),
        refract,
        refract_tint: color(mat, "$refracttint", [1.0; 3]),
        refract_amount: num(mat, "$refractamount").unwrap_or(0.0),
        reflect,
        reflect_tint: color(mat, "$reflecttint", [1.0; 3]),
        reflect_amount: num(mat, "$reflectamount").unwrap_or(0.8),
        fog_color,
        fog_range,
        cheap: !reflect && envmap,
        cheap_blend: !force_cheap,
        cheap_range: [
            num(mat, "$cheapwaterstartdistance").unwrap_or(500.0),
            num(mat, "$cheapwaterenddistance").unwrap_or(1000.0),
        ],
        fresnel: !mat.flag("$nofresnel"),
        reflect_blend: num(mat, "$reflectblendfactor").unwrap_or(1.0),
        base_texture: None,
        bottom: None,
        envmap,
        above_water: mat.get_bool("$abovewater").unwrap_or(true),
        reflect_entities: mat.flag("$reflectentities"),
        reflect_skybox_only: mat.flag("$reflectskyboxonly"),
        reflect_3d_sky: mat.get_bool("$reflect3dskybox").unwrap_or(true),
    };
    Some(WaterAnalysis {
        desc,
        normal_map: texture("$normalmap"),
        base_texture: texture("$basetexture"),
        animated: normal_animation(mat),
        bottom_material: mat.get("$bottommaterial").map(hp_vmt::normalize_material_name).filter(|s| !s.is_empty()),
    })
}

pub(crate) fn bottom_of(mat: &Material) -> WaterBottom {
    let (fog_color, fog_range) = fog(mat);
    WaterBottom {
        refract_tint: color(mat, "$refracttint", [1.0; 3]),
        refract_amount: num(mat, "$refractamount").unwrap_or(0.0),
        fog_color,
        fog_range,
    }
}

pub(crate) fn frame_grid(count: u32) -> [u32; 2] {
    let count = count.clamp(1, MAX_FRAMES);
    let cols = ((count as f32).sqrt().ceil() as u32).next_power_of_two();
    let rows = count.div_ceil(cols).next_power_of_two();
    [cols, rows]
}

pub(crate) fn finish(fs: &hp_fs::GameFs, w: &mut WaterAnalysis) {
    if let Some(name) = &w.bottom_material
        && let Some(m) = fs.material(name)
    {
        let m = m.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
        if m.shader_kind() == hp_vmt::ShaderKind::Water {
            w.desc.bottom = Some(bottom_of(&m));
        }
    }
    if let (Some(rate), Some(name)) = (w.animated, w.normal_map.clone()) {
        let frames = fs
            .load_texture_raw(&name)
            .and_then(|b| hp_vtf::read_info(&b).ok())
            .map_or(1, |i| i.frames.clamp(1, MAX_FRAMES));
        if frames > 1 {
            let [cols, rows] = frame_grid(frames);
            w.desc.frames = [cols, rows, frames];
            w.desc.frame_rate = rate;
            w.normal_map = Some(format!("{name}{FRAMES_SUFFIX}"));
        }
    }
}

pub(crate) fn desc(w: &WaterAnalysis, tex: &dyn Fn(&str) -> TexLook) -> WaterDesc {
    let ready = |key: &Option<String>| -> Option<TexId> {
        match tex(key.as_deref()?) {
            TexLook::Ready(id, _) => Some(id),
            TexLook::Loading | TexLook::Missing => None,
        }
    };
    WaterDesc { normal_map: ready(&w.normal_map), base_texture: ready(&w.base_texture), ..w.desc }
}

pub(crate) fn texture_keys(w: &WaterAnalysis) -> impl Iterator<Item = String> + '_ {
    [w.normal_map.as_ref(), w.base_texture.as_ref()].into_iter().flatten().cloned()
}

pub(crate) fn pack_frames(bytes: &[u8]) -> Result<PreparedTexture, String> {
    let info = hp_vtf::read_info(bytes).map_err(|e| e.to_string())?;
    let count = info.frames.clamp(1, MAX_FRAMES);
    let [cols, rows] = frame_grid(count);
    let first = hp_vtf::raw_mips(bytes, 0, 0).map_err(|e| e.to_string())?;
    let level = first
        .iter()
        .position(|l| l.width * cols <= MAX_TEXTURE_DIM && l.height * rows <= MAX_TEXTURE_DIM)
        .ok_or("no mip level small enough for the frame grid")?;
    let (fw, fh) = first.get(level).map(|l| (l.width, l.height)).ok_or("no mip level")?;
    let (w, h) = (fw * cols, fh * rows);
    let mut atlas = vec![0u8; (w * h * 4) as usize];
    for frame in 0..cols * rows {
        let src = frame % count;
        let mips = hp_vtf::raw_mips(bytes, src, 0).map_err(|e| e.to_string())?;
        let img = mips.get(level).ok_or("missing mip level")?.to_rgba8().map_err(|e| e.to_string())?;
        if img.width != fw || img.height != fh {
            return Err(format!("frame {src} decoded as {}x{}", img.width, img.height));
        }
        let (cx, cy) = (frame % cols, frame / cols);
        for y in 0..fh {
            let s = (y * fw * 4) as usize;
            let d = (((cy * fh + y) * w + cx * fw) * 4) as usize;
            let row = img.pixels.get(s..s + (fw * 4) as usize).ok_or("short frame")?;
            atlas.get_mut(d..d + row.len()).ok_or("frame outside the grid")?.copy_from_slice(row);
        }
    }
    let levels = build_mips(w, h, atlas).ok_or("bad frame grid")?;
    Ok(PreparedTexture {
        upload: hp_render::TextureUpload { width: w, height: h, format: hp_render::TexFormat::Rgba8Srgb, levels },
        size: [info.width.max(1), info.height.max(1)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mat(src: &str) -> Material {
        hp_vmt::parse(src).unwrap_or_default()
    }

    const CONSTRUCT: &str = r#""Water"
{
	"$abovewater" "1"
	"$forceexpensive" "1"
	"$refracttexture" "_rt_WaterRefraction"
	"$refractamount" "1"
	"$reflecttexture" "_rt_WaterReflection"
	"$reflectamount" "0.4"
	"$reflect3dskybox" "1"
	"$reflectentities" "1"
	"$scale" "[1 1]"
	"$bumpmap" "gm_construct/water_13_dudv"
	"$normalmap" "gm_construct/water_13_normal"
	"$bumpframe" "0"
	"$surfaceprop" "water"
	"$bottommaterial" "gm_construct/water_13_beneath"
	"$fogenable" "1"
	"$fogcolor" "{7 58 66}"
	"$fogstart" "0"
	"$fogend" "1024.00"
	"$scroll1" "[.01 .01 .05]"
	"$scroll2" "[-.025 .025 .075]"
	"%tooltexture" "gm_construct/water_13_normal"
	"%compilewater" "1"
	"Proxies"
	{
		"TextureScroll"
		{
			"texturescrollvar" "$bumptransform"
			"texturescrollrate" "0.02"
			"texturescrollangle" "25.00"
		}
		"WaterLOD"
		{
			"dummy" "0"
		}
	}
}"#;

    #[test]
    fn construct_water_is_expensive_with_fog_and_scroll() {
        let w = analyze_water(&mat(CONSTRUCT)).expect("water");
        let d = w.desc;
        assert!(d.refract && d.reflect && !d.cheap);
        assert_eq!(d.refract_amount, 1.0);
        assert_eq!(d.reflect_amount, 0.4);
        assert!((d.fog_color[1] - 58.0 / 255.0).abs() < 1e-6);
        assert_eq!(d.fog_range, [0.0, 1024.0]);
        assert_eq!(d.scroll1, [0.01, 0.01]);
        assert_eq!(d.scroll2, [-0.025, 0.025]);
        let rad = 25.0f32.to_radians();
        assert!(
            (d.bump_scroll[0] - 0.02 * rad.cos()).abs() < 1e-6 && (d.bump_scroll[1] - 0.02 * rad.sin()).abs() < 1e-6
        );
        assert_eq!(w.normal_map.as_deref(), Some("gm_construct/water_13_normal"));
        assert_eq!(w.bottom_material.as_deref(), Some("gm_construct/water_13_beneath"));
        assert_eq!(w.animated, None);
        assert_eq!(w.base_texture, None);
        let alias = CONSTRUCT.replacen("\"Water\"", "\"Water_DX9_HDR\"", 1);
        assert_eq!(analyze_water(&mat(&alias)), Some(w), "the game's HDR shader alias is still water");
    }

    #[test]
    fn cheap_animated_and_defaults() {
        let cheap = mat(r#"Water { "$forcecheap" 1 "$envmap" "env_cubemap" "$refracttexture" "_rt_WaterRefraction"
            "$reflecttexture" "_rt_WaterReflection" "$reflecttint" "[0.7 0.7 0.7]" "$normalmap" "dev/water_normal"
            "$nofresnel" 1 "$CHEAPWATERSTARTDISTANCE" 200
            Proxies { AnimatedTexture { animatedtexturevar $normalmap animatedtextureframenumvar $bumpframe animatedtextureframerate 16 } } }"#);
        let w = analyze_water(&cheap).expect("water");
        assert!(!w.desc.reflect && w.desc.cheap && !w.desc.cheap_blend && !w.desc.fresnel);
        assert_eq!(w.desc.cheap_range, [200.0, 1000.0]);
        assert_eq!(w.desc.reflect_tint, [0.7; 3]);
        assert_eq!(w.animated, Some(16.0));
        let bare = analyze_water(&mat(r#"Water { "$normalmap" "x/n" }"#)).expect("water");
        assert_eq!(bare.desc.fog_color, [1.0, 0.0, 0.0]);
        assert_eq!(bare.desc.reflect_amount, 0.8);
        assert!(!bare.desc.refract && !bare.desc.reflect && !bare.desc.cheap);
        assert!(analyze_water(&mat(r#"LightmappedGeneric { "%compilewater" 1 "$basetexture" "x" }"#)).is_none());
        let exp0 = mat(r#"Water { "$forceexpensive" 0 "$reflecttexture" "r" "$envmap" "env_cubemap" }"#);
        let w = analyze_water(&exp0).expect("water");
        assert!(!w.desc.reflect && w.desc.cheap && w.desc.cheap_blend);
        let b = bottom_of(&mat(
            r#"Water { "$fogcolor" "{24 64 72}" "$fogstart" "-1024" "$fogend" 2048 "$refractamount" 1 }"#,
        ));
        assert_eq!(b.fog_range, [-1024.0, 2048.0]);
        assert_eq!(b.refract_amount, 1.0);
    }

    #[test]
    fn frame_grids_hold_every_frame() {
        for n in 1..=64 {
            let [c, r] = frame_grid(n);
            assert!(c * r >= n && c.is_power_of_two() && r.is_power_of_two(), "{n}");
        }
        assert_eq!(frame_grid(29), [8, 4]);
        assert_eq!(frame_grid(1), [1, 1]);
    }

    #[test]
    fn descs_wait_for_textures() {
        let w = analyze_water(&mat(CONSTRUCT)).expect("water");
        let loading = desc(&w, &|_| TexLook::Loading);
        assert_eq!(loading.normal_map, None);
        let ready = desc(&w, &|_| TexLook::Ready(TexId::WHITE, [256, 256]));
        assert_eq!(ready.normal_map, Some(TexId::WHITE));
        assert_eq!(texture_keys(&w).collect::<Vec<_>>(), vec!["gm_construct/water_13_normal".to_owned()]);
    }

    #[test]
    fn animated_frames_pack_in_a_grid() {
        let frames: Vec<hp_vtf::RgbaImage> =
            (0..3u8).filter_map(|f| hp_vtf::RgbaImage::new(8, 8, [f * 100, 0, 0, 255].repeat(64)).ok()).collect();
        let opts =
            hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Bgra8888, mipmaps: true, ..Default::default() };
        let bytes = hp_vtf::encode_frames(&frames, &opts).expect("encode");
        let p = pack_frames(&bytes).expect("pack");
        assert_eq!((p.upload.width, p.upload.height), (16, 16), "2 x 2 grid of 8 x 8 frames");
        assert_eq!(p.size, [8, 8]);
        let top = &p.upload.levels[0];
        let at = |x: usize, y: usize| top[(y * 16 + x) * 4];
        assert_eq!((at(0, 0), at(8, 0), at(0, 8), at(8, 8)), (0, 100, 200, 0));
    }
}
