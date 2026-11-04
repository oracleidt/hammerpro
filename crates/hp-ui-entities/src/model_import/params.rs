use std::path::Path;

use hp_app::{t, tf};
use hp_meshio::{Material, NormalKind, TextureSource};
use hp_vtf::RgbaImage;

const NORMAL_FLAG: u32 = 0x80;
const BIG_NORMAL: u32 = 1024;
const HEIGHT_STRENGTH: f64 = 2.0;
const MIN_GLOSS: f64 = 0.2;
const MIN_SPECULAR: f64 = 0.55;
const MIN_SHININESS: f64 = 50.0;
const EMISSIVE_MIN: f64 = 0.01;

#[derive(Debug, Default)]
pub(super) struct Built {
    pub params: Vec<(String, String)>,
    pub textures: usize,
    pub converted: Vec<String>,
    pub ignored: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub(super) struct Shading {
    pub exponent: f64,
    pub boost: f64,
    pub envmap: Option<f64>,
    pub albedo_tint: bool,
}

pub(super) fn shading(
    metallic: Option<f64>,
    roughness: Option<f64>,
    specular: Option<f64>,
    shininess: Option<f64>,
) -> Option<Shading> {
    if metallic.is_some() || roughness.is_some() {
        let m = metallic.unwrap_or(0.0).clamp(0.0, 1.0);
        let r = roughness.unwrap_or(0.5).clamp(0.0, 1.0);
        let gloss = 1.0 - r;
        if gloss < MIN_GLOSS || specular.is_some_and(|s| s < MIN_GLOSS) {
            return None;
        }
        return Some(Shading {
            exponent: (5.0 + gloss * gloss * 145.0).round(),
            boost: ((1.0 + m * 3.0 + gloss) * 100.0).round() / 100.0,
            envmap: (m >= 0.6 && r < 0.6).then(|| (m * gloss * 100.0).round() / 100.0),
            albedo_tint: m >= 0.5,
        });
    }
    let s = specular?;
    let n = shininess.unwrap_or(MIN_SHININESS);
    if s < MIN_SPECULAR || n < MIN_SHININESS {
        return None;
    }
    Some(Shading {
        exponent: (n / 4.0).clamp(5.0, 150.0).round(),
        boost: ((s * 3.0).max(1.0) * 100.0).round() / 100.0,
        envmap: None,
        albedo_tint: false,
    })
}

pub(super) fn uv_transform(scale: [f64; 2], offset: [f64; 2]) -> String {
    format!("center 0 0 scale {:.4} {:.4} rotate 0 translate {:.4} {:.4}", scale[0], scale[1], offset[0], offset[1])
}

fn source_image(src: &TextureSource, material: &str, warnings: &mut Vec<String>) -> Option<RgbaImage> {
    let (bytes, ext) = match (&src.path, &src.embedded) {
        (Some(p), _) => match std::fs::read(p) {
            Ok(b) => (b, p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()),
            Err(e) => {
                warnings.push(tf!("entities.mi_texture_failed", material, e.to_string()));
                return None;
            }
        },
        (None, Some((b, ext))) => (b.clone(), ext.clone()),
        (None, None) => return None,
    };
    match hp_vtf::load_image_file(&bytes, &ext) {
        Ok(img) => Some(img),
        Err(e) => {
            warnings.push(tf!("entities.mi_texture_failed", material, e.to_string()));
            None
        }
    }
}

pub(super) fn metal_rough_means(img: &RgbaImage) -> (f64, f64) {
    let n = (img.pixels.len() / 4).max(1) as f64;
    let (mut g, mut b) = (0.0, 0.0);
    for p in img.pixels.chunks_exact(4) {
        g += f64::from(p[1]);
        b += f64::from(p[2]);
    }
    (g / n / 255.0, b / n / 255.0)
}

fn looks_like_normal_map(img: &RgbaImage) -> bool {
    let n = (img.pixels.len() / 4).max(1) as f64;
    let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
    for p in img.pixels.chunks_exact(4) {
        r += f64::from(p[0]);
        g += f64::from(p[1]);
        b += f64::from(p[2]);
    }
    let (r, g, b) = (r / n, g / n, b / n);
    b > r + 20.0 && b > g + 20.0
}

fn height_to_normal(img: &RgbaImage) -> RgbaImage {
    let (w, h) = (img.width as usize, img.height as usize);
    let height = |x: i64, y: i64| {
        let x = x.clamp(0, w as i64 - 1) as usize;
        let y = y.clamp(0, h as i64 - 1) as usize;
        let p = &img.pixels[(y * w + x) * 4..(y * w + x) * 4 + 3];
        (f64::from(p[0]) + f64::from(p[1]) + f64::from(p[2])) / (3.0 * 255.0)
    };
    let mut pixels = Vec::with_capacity(w * h * 4);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let nx = -(height(x + 1, y) - height(x - 1, y)) * HEIGHT_STRENGTH;
            let ny = (height(x, y - 1) - height(x, y + 1)) * HEIGHT_STRENGTH;
            let len = (nx * nx + ny * ny + 1.0).sqrt();
            let enc = |v: f64| ((v / len * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            pixels.extend_from_slice(&[enc(nx), enc(ny), enc(1.0), 255]);
        }
    }
    RgbaImage { width: img.width, height: img.height, pixels }
}

pub(super) fn normal_image(mut img: RgbaImage, kind: NormalKind) -> RgbaImage {
    let kind = match kind {
        NormalKind::Bump if looks_like_normal_map(&img) => NormalKind::DirectX,
        k => k,
    };
    match kind {
        NormalKind::Bump => height_to_normal(&img),
        NormalKind::OpenGl => {
            for p in img.pixels.chunks_exact_mut(4) {
                p[1] = 255 - p[1];
                p[3] = 255;
            }
            img
        }
        NormalKind::DirectX => {
            for p in img.pixels.chunks_exact_mut(4) {
                p[3] = 255;
            }
            img
        }
    }
}

fn solid(width: u32, height: u32, px: [u8; 4]) -> RgbaImage {
    RgbaImage { width, height, pixels: px.repeat((width * height) as usize) }
}

fn write_vtf(path: &Path, img: &RgbaImage, normal: bool) -> Result<(), String> {
    let big = img.width.max(img.height) > BIG_NORMAL;
    let enc = hp_vtf::EncodeOptions {
        format: if normal && !big { hp_vtf::EncodeFormat::Bgr888 } else { hp_vtf::EncodeFormat::Auto },
        flags: if normal { NORMAL_FLAG } else { 0 },
        mipmaps: true,
        quality: hp_vtf::EncodeQuality::Normal,
        ..hp_vtf::EncodeOptions::default()
    };
    let vtf = hp_vtf::encode(img, &enc).map_err(|e| e.to_string())?;
    std::fs::write(path, vtf).map_err(|e| format!("{}: {e}", path.display()))
}

fn ignored_text(id: &str) -> String {
    match id {
        "occlusion_texture" => t!("entities.mi_ig_occlusion").to_owned(),
        "normal_missing" => t!("entities.mi_ig_normal_missing").to_owned(),
        "emissive_missing" => t!("entities.mi_ig_emissive_missing").to_owned(),
        "metal_rough_missing" => t!("entities.mi_ig_metal_missing").to_owned(),
        "specular_texture" => t!("entities.mi_ig_specular_texture").to_owned(),
        "height_texture" => t!("entities.mi_ig_height").to_owned(),
        other => other.strip_prefix("ext:").unwrap_or(other).to_owned(),
    }
}

pub(super) fn build(
    m: &Material,
    tex_prefix: &str,
    file: &str,
    mat_dir: &Path,
    textured: bool,
    warnings: &mut Vec<String>,
) -> Result<Built, String> {
    let e = &m.extras;
    let mut out = Built { ignored: e.ignored.iter().map(|i| ignored_text(i)).collect(), ..Built::default() };
    let mut normal_written = false;

    let mut metallic = e.metallic;
    let mut roughness = e.roughness;
    if let Some(src) = &e.metal_rough {
        match source_image(src, &m.name, warnings) {
            Some(img) => {
                let (g, b) = metal_rough_means(&img);
                metallic = metallic.map(|v| v * b);
                roughness = roughness.map(|v| v * g);
            }
            None => out.ignored.push(ignored_text("metal_rough_missing")),
        }
    }
    let shade = shading(metallic, roughness, e.specular, e.shininess);

    if let Some((src, kind)) = &e.normal {
        match source_image(src, &m.name, warnings) {
            Some(img) => {
                write_vtf(&mat_dir.join(format!("{file}_n.vtf")), &normal_image(img, *kind), true)?;
                normal_written = true;
                out.textures += 1;
                out.converted.push(t!("entities.mi_cv_normal").to_owned());
            }
            None => out.ignored.push(ignored_text("normal_missing")),
        }
    }
    if let Some(s) = &shade {
        if !normal_written {
            write_vtf(&mat_dir.join(format!("{file}_n.vtf")), &solid(4, 4, [128, 128, 255, 255]), true)?;
            normal_written = true;
        }
        let params = &mut out.params;
        params.push(("$phong".to_owned(), "1".to_owned()));
        params.push(("$phongexponent".to_owned(), format!("{}", s.exponent)));
        params.push(("$phongboost".to_owned(), format!("{:.2}", s.boost)));
        params.push(("$phongfresnelranges".to_owned(), "[0.3 0.7 1]".to_owned()));
        if s.albedo_tint {
            params.push(("$phongalbedotint".to_owned(), "1".to_owned()));
        }
        out.converted.push(t!("entities.mi_cv_phong").to_owned());
        if let Some(tint) = s.envmap {
            params.push(("$envmap".to_owned(), "env_cubemap".to_owned()));
            params.push(("$envmaptint".to_owned(), format!("[{tint:.2} {tint:.2} {tint:.2}]")));
            out.converted.push(t!("entities.mi_cv_envmap").to_owned());
        }
    }
    if normal_written {
        out.params.push(("$bumpmap".to_owned(), format!("{tex_prefix}_n")));
    }

    if let Some(c) = e.emissive.filter(|c| c.iter().any(|v| *v > EMISSIVE_MIN)) {
        let mask = e.emissive_texture.as_ref().and_then(|s| source_image(s, &m.name, warnings));
        let has_mask = mask.is_some();
        let img = mask.unwrap_or_else(|| solid(4, 4, [255, 255, 255, 255]));
        write_vtf(&mat_dir.join(format!("{file}_illum.vtf")), &img, false)?;
        out.textures += usize::from(has_mask);
        out.params.push(("$selfillum".to_owned(), "1".to_owned()));
        out.params.push(("$selfillummask".to_owned(), format!("{tex_prefix}_illum")));
        if c.iter().any(|v| (v - 1.0).abs() > 0.004) {
            out.params.push(("$selfillumtint".to_owned(), format!("[{:.3} {:.3} {:.3}]", c[0], c[1], c[2])));
        }
        out.converted.push(t!("entities.mi_cv_selfillum").to_owned());
    }

    if textured && e.has_uv_transform() {
        let xf = uv_transform(e.uv_scale, e.uv_offset);
        if normal_written {
            out.params.push(("$bumptransform".to_owned(), xf.clone()));
        }
        out.params.push(("$basetexturetransform".to_owned(), xf));
        out.converted.push(t!("entities.mi_cv_uv").to_owned());
    }
    Ok(out)
}

pub(super) fn report(name: &str, built: &Built) -> String {
    let none = t!("entities.mi_nothing");
    let join = |v: &[String]| if v.is_empty() { none.to_owned() } else { v.join(", ") };
    tf!("entities.mi_report", name, join(&built.converted), join(&built.ignored))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shading_from_metal_and_rough_values() {
        assert_eq!(shading(Some(1.0), Some(1.0), None, None), None, "a rough metal is dull");
        let s = shading(Some(1.0), Some(0.1), None, None).expect("shiny metal");
        assert!(s.envmap.is_some() && s.albedo_tint && s.exponent > 100.0, "{s:?}");
        let s = shading(Some(0.0), Some(0.3), None, None).expect("glossy plastic");
        assert!(s.envmap.is_none() && !s.albedo_tint);
        assert_eq!(shading(None, None, Some(0.5), Some(250.0)), None, "the default specular of Blender stays off");
        let s = shading(None, None, Some(0.9), Some(300.0)).expect("hard specular");
        assert_eq!(s.exponent, 75.0);
        assert_eq!(shading(None, None, None, None), None);
    }

    #[test]
    fn normal_maps_are_converted() {
        let img = RgbaImage { width: 1, height: 1, pixels: vec![128, 200, 255, 0] };
        assert_eq!(normal_image(img.clone(), NormalKind::OpenGl).pixels, [128, 55, 255, 255]);
        assert_eq!(normal_image(img.clone(), NormalKind::DirectX).pixels, [128, 200, 255, 255]);
        assert_eq!(normal_image(img, NormalKind::Bump).pixels, [128, 200, 255, 255], "a bluish bump is a normal map");
        let height =
            RgbaImage { width: 3, height: 1, pixels: vec![0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255] };
        let n = normal_image(height, NormalKind::Bump);
        assert!(n.pixels[4] < 128, "the slope rising to the right leans left: {:?}", n.pixels);
        assert_eq!(n.pixels[7], 255);
    }

    #[test]
    fn transform_text() {
        assert_eq!(
            uv_transform([2.0, 3.0], [0.25, 0.5]),
            "center 0 0 scale 2.0000 3.0000 rotate 0 translate 0.2500 0.5000"
        );
    }
}
