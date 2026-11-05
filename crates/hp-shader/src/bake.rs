use std::path::{Path, PathBuf};

use hp_vtf::{EncodeFormat, EncodeOptions, EncodeQuality, MipFilter, RgbaImage, flags};

use crate::eval::{EvalOptions, ImageCache, ImageLoader, OutputImage, evaluate};
use crate::graph::{Diagnostic, Graph, Severity};

#[derive(Clone, Debug, Default)]
pub struct BakeOptions {
    pub dest_dir: PathBuf,
    pub prefix: String,
    pub stem: String,
    pub surface_prop: Option<String>,
    pub overwrite: bool,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct BakedFile {
    pub node: u32,
    pub target: String,
    pub texture: String,
    pub vtf: PathBuf,
    pub bytes: usize,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default)]
pub struct BakeReport {
    pub files: Vec<BakedFile>,
    pub material: Option<(String, PathBuf)>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn sanitize_stem(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').chars().take(48).collect()
}

fn normalize_prefix(p: &str) -> String {
    p.replace('\\', "/").split('/').filter(|s| !s.is_empty()).map(str::to_ascii_lowercase).collect::<Vec<_>>().join("/")
}

pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_extension(format!("{ext}.tmp"));
    std::fs::write(&tmp, data).map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

#[derive(PartialEq, Eq)]
enum AlphaKind {
    Opaque,
    Binary,
    Blended,
}

fn alpha_kind(img: &RgbaImage) -> AlphaKind {
    let mut kind = AlphaKind::Opaque;
    for px in img.pixels.chunks_exact(4) {
        match px[3] {
            255 => {}
            0 => kind = AlphaKind::Binary,
            _ => return AlphaKind::Blended,
        }
    }
    kind
}

fn encode_options(out: &OutputImage, opaque: bool) -> EncodeOptions {
    let normal = out.target == "normalmap";
    let format = match out.format.as_str() {
        "dxt1" => EncodeFormat::Dxt1,
        "dxt5" => EncodeFormat::Dxt5,
        "bgra8888" => EncodeFormat::Bgra8888,
        "bgr888" => EncodeFormat::Bgr888,
        _ if normal && opaque => EncodeFormat::Bgr888,
        _ if normal => EncodeFormat::Bgra8888,
        _ => EncodeFormat::Auto,
    };
    let mut fl = 0;
    if normal {
        fl |= flags::NORMAL;
    }
    if out.clamp {
        fl |= flags::CLAMPS | flags::CLAMPT;
    }
    EncodeOptions {
        format,
        mipmaps: out.mipmaps,
        flags: fl,
        quality: EncodeQuality::Normal,
        mip_filter: if normal { MipFilter::Normal } else { MipFilter::FromFlags },
        ..EncodeOptions::default()
    }
}

pub fn bake(
    graph: &Graph,
    loader: &dyn ImageLoader,
    cache: &ImageCache,
    opts: &BakeOptions,
) -> Result<BakeReport, String> {
    let problems: Vec<String> =
        graph.validate().into_iter().filter(|d| d.severity == Severity::Error).map(|d| d.message()).collect();
    if let Some(first) = problems.first() {
        return Err(first.clone());
    }
    if graph.output_ids().is_empty() {
        return Err("the graph has no output node".to_owned());
    }
    let stem = sanitize_stem(&opts.stem);
    if stem.is_empty() {
        return Err("the name of the texture is empty".to_owned());
    }
    let prefix = normalize_prefix(&opts.prefix);
    let mut size = graph.settings;
    if opts.width > 0 && opts.height > 0 {
        size.width = opts.width;
        size.height = opts.height;
    }
    let mut sized = graph.clone();
    sized.settings = size;
    let eopts = EvalOptions { width: 0, height: 0, outputs: true, ..EvalOptions::default() };
    let eval = evaluate(&sized, loader, cache, &eopts);
    if eval.outputs.is_empty() {
        return Err("nothing to write: the outputs could not be computed".to_owned());
    }
    let tex_name = |suffix: &str| {
        let file = format!("{stem}{suffix}");
        if prefix.is_empty() { file } else { format!("{prefix}/{file}") }
    };
    let mut planned: Vec<(&OutputImage, PathBuf, String)> = Vec::new();
    for out in &eval.outputs {
        let file = format!("{stem}{}", out.suffix);
        planned.push((out, opts.dest_dir.join(format!("{file}.vtf")), tex_name(&out.suffix)));
    }
    let base = eval.outputs.iter().find(|o| o.target == "basetexture");
    let vmt_path = opts.dest_dir.join(format!("{stem}.vmt"));
    if !opts.overwrite {
        for (_, vtf, _) in &planned {
            if vtf.exists() {
                return Err(format!("{} already exists", vtf.display()));
            }
        }
        if base.is_some() && vmt_path.exists() {
            return Err(format!("{} already exists", vmt_path.display()));
        }
    }
    std::fs::create_dir_all(&opts.dest_dir).map_err(|e| format!("{}: {e}", opts.dest_dir.display()))?;
    let mut report = BakeReport { diagnostics: eval.diagnostics.clone(), ..BakeReport::default() };
    for (out, vtf_path, texture) in &planned {
        let opaque = alpha_kind(&out.image) == AlphaKind::Opaque;
        let bytes = hp_vtf::encode(&out.image, &encode_options(out, opaque)).map_err(|e| format!("{texture}: {e}"))?;
        write_atomic(vtf_path, &bytes)?;
        report.files.push(BakedFile {
            node: out.node,
            target: out.target.clone(),
            texture: texture.clone(),
            vtf: vtf_path.clone(),
            bytes: bytes.len(),
            width: out.image.width,
            height: out.image.height,
        });
    }
    if let Some(base) = base {
        let kind = alpha_kind(&base.image);
        let mopts = hp_vmt::ImageMaterialOptions {
            surfaceprop: opts.surface_prop.clone().filter(|s| !s.trim().is_empty()),
            alphatest: kind == AlphaKind::Binary,
            translucent: kind == AlphaKind::Blended,
            ..hp_vmt::ImageMaterialOptions::default()
        };
        let name = tex_name("");
        let mut mat = hp_vmt::for_image(&name, &mopts);
        for f in &report.files {
            match f.target.as_str() {
                "normalmap" => mat.params.push(("$bumpmap".to_owned(), f.texture.clone())),
                "envmapmask" => {
                    mat.params.push(("$envmap".to_owned(), "env_cubemap".to_owned()));
                    mat.params.push(("$envmapmask".to_owned(), f.texture.clone()));
                }
                "selfillummask" => {
                    mat.params.push(("$selfillum".to_owned(), "1".to_owned()));
                    mat.params.push(("$selfillummask".to_owned(), f.texture.clone()));
                }
                _ => {}
            }
        }
        write_atomic(&vmt_path, hp_vmt::write(&mat).as_bytes())?;
        report.material = Some((name, vmt_path));
    } else {
        report.diagnostics.push(Diagnostic::new(Severity::Info, None, "no_base", ""));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;

    struct Mem(HashMap<String, RgbaImage>);

    impl ImageLoader for Mem {
        fn load(&self, source: &str, max_dim: u32) -> Result<RgbaImage, String> {
            self.0.get(source).cloned().map(|i| crate::downscale_to_fit(i, max_dim)).ok_or_else(|| "missing".into())
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::var_os("HP_SCRATCH").map_or_else(std::env::temp_dir, PathBuf::from);
        let dir = root.join("hp-shader").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn loader() -> Mem {
        let px: Vec<u8> = (0..32 * 32).flat_map(|i| [(i % 32 * 8) as u8, 90, 40, 255]).collect();
        let mut m = HashMap::new();
        m.insert("brick.png".to_owned(), RgbaImage { width: 32, height: 32, pixels: px });
        Mem(m)
    }

    #[test]
    fn bakes_base_and_normal_with_a_material() {
        let g = crate::preset("normal_from_height", "brick.png").unwrap();
        let dir = scratch("bake");
        let opts = BakeOptions {
            dest_dir: dir.clone(),
            prefix: "Custom/Shader".into(),
            stem: "Brick Wall".into(),
            surface_prop: Some("brick".into()),
            overwrite: false,
            width: 0,
            height: 0,
        };
        let report = bake(&g, &loader(), &ImageCache::new(), &opts).unwrap();
        assert_eq!(report.files.len(), 2);
        assert!(dir.join("brick_wall.vtf").is_file() && dir.join("brick_wall_normal.vtf").is_file());
        let (name, vmt) = report.material.clone().unwrap();
        assert_eq!(name, "custom/shader/brick_wall");
        let m = hp_vmt::parse(&std::fs::read_to_string(vmt).unwrap()).unwrap();
        assert_eq!(m.base_texture().as_deref(), Some("custom/shader/brick_wall"));
        assert_eq!(m.surface_prop(), Some("brick"));
        assert!(m.params.iter().any(|(k, v)| k == "$bumpmap" && v == "custom/shader/brick_wall_normal"));
        let info = hp_vtf::read_info(&std::fs::read(dir.join("brick_wall_normal.vtf")).unwrap()).unwrap();
        assert_ne!(info.flags & flags::NORMAL, 0);
        assert_eq!((info.width, info.height), (32, 32));
        assert!(bake(&g, &loader(), &ImageCache::new(), &opts).is_err(), "no overwrite");
        assert!(bake(&g, &loader(), &ImageCache::new(), &BakeOptions { overwrite: true, ..opts.clone() }).is_ok());
        assert!(!dir.join("brick_wall.vtf.tmp").exists());
    }

    #[test]
    fn refuses_bad_graphs_and_names() {
        let dir = scratch("refuse");
        let opts = BakeOptions { dest_dir: dir, stem: "x".into(), ..BakeOptions::default() };
        assert!(bake(&Graph::new(), &loader(), &ImageCache::new(), &opts).is_err());
        let mut g = crate::preset("blank", "brick.png").unwrap();
        let o2 = g.add_node("output", [0.0, 0.0]).unwrap();
        assert!(bake(&g, &loader(), &ImageCache::new(), &opts).is_err(), "two base outputs");
        g.set_param(o2, "target", json!("normalmap")).unwrap();
        assert!(bake(&g, &loader(), &ImageCache::new(), &BakeOptions { stem: "___".into(), ..opts.clone() }).is_err());
        assert!(bake(&g, &loader(), &ImageCache::new(), &opts).is_ok());
    }

    #[test]
    fn size_override_and_alpha_flags() {
        let mut g = Graph::new();
        let c = g.add_with("color", [0.0, 0.0], &[("color", json!([1.0, 0.0, 0.0, 0.5]))]).unwrap();
        let o = g.add_node("output", [0.0, 0.0]).unwrap();
        g.connect(c, "color", o, "color").unwrap();
        let dir = scratch("alpha");
        let opts =
            BakeOptions { dest_dir: dir.clone(), stem: "glass".into(), width: 16, height: 8, ..BakeOptions::default() };
        let r = bake(&g, &loader(), &ImageCache::new(), &opts).unwrap();
        assert_eq!((r.files[0].width, r.files[0].height), (16, 8));
        let m = hp_vmt::parse(&std::fs::read_to_string(dir.join("glass.vmt")).unwrap()).unwrap();
        assert!(m.translucent());
    }
}
