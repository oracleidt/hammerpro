use super::*;
use crate::{NormalKind, TextureSource};

const HANDLED: &[&str] = &[
    "KHR_materials_pbrSpecularGlossiness",
    "KHR_materials_emissive_strength",
    "KHR_materials_specular",
    "KHR_texture_transform",
];

pub(super) fn plain_transform(info: &Value) -> Option<([f64; 2], [f64; 2])> {
    let t = info.pointer("/extensions/KHR_texture_transform")?;
    if t.get("rotation").and_then(Value::as_f64).unwrap_or(0.0).abs() > 1e-6 {
        return None;
    }
    let pair =
        |key: &str, default: f64| floats_of(t.get(key)).filter(|v| v.len() >= 2).map_or([default; 2], |v| [v[0], v[1]]);
    Some((pair("scale", 1.0), pair("offset", 0.0)))
}

pub(super) fn texture_source(doc: &Doc, dir: &Path, info: &Value) -> Option<TextureSource> {
    let texture = num(info, "index").and_then(|t| arr(&doc.json, "textures").get(t as usize))?;
    let source = num(texture, "source").or_else(|| {
        ["EXT_texture_webp", "KHR_texture_basisu", "MSFT_texture_dds"]
            .iter()
            .find_map(|e| texture.pointer(&format!("/extensions/{e}/source")).and_then(Value::as_u64))
    })?;
    let img = arr(&doc.json, "images").get(source as usize)?;
    let mime = img.get("mimeType").and_then(Value::as_str);
    let mut out = TextureSource::default();
    if let Some(uri) = img.get("uri").and_then(Value::as_str) {
        if uri.starts_with("data:") {
            out.embedded = load_uri(uri, dir).map(|b| (b, image_ext(mime, uri)));
        } else {
            let path = dir.join(percent_decode(uri));
            if path.is_file() {
                out.path = Some(path);
            }
        }
    } else if let Some(view) = num(img, "bufferView") {
        out.embedded = doc.view(view as usize).map(|(b, _)| (b.to_vec(), image_ext(mime, "")));
    }
    out.is_some().then_some(out)
}

fn mean3(v: &[f64]) -> f64 {
    v.iter().take(3).sum::<f64>() / 3.0
}

pub(super) fn read_extras(doc: &Doc, dir: &Path, m: &Value, mat: &mut Material) {
    let pbr = m.get("pbrMetallicRoughness");
    let spec_gloss = m.pointer("/extensions/KHR_materials_pbrSpecularGlossiness");
    let e = &mut mat.extras;
    if let Some(info) = m.get("normalTexture") {
        match texture_source(doc, dir, info) {
            Some(src) => e.normal = Some((src, NormalKind::OpenGl)),
            None => e.ignore("normal_missing"),
        }
    }
    if let Some(p) = pbr {
        e.metallic = Some(p.get("metallicFactor").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0));
        e.roughness = Some(p.get("roughnessFactor").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0));
        if let Some(info) = p.get("metallicRoughnessTexture") {
            match texture_source(doc, dir, info) {
                Some(src) => e.metal_rough = Some(src),
                None => e.ignore("metal_rough_missing"),
            }
        }
    }
    if let Some(sg) = spec_gloss {
        let spec = floats_of(sg.get("specularFactor")).filter(|v| v.len() >= 3).unwrap_or_else(|| vec![1.0; 3]);
        e.specular = Some(mean3(&spec).clamp(0.0, 1.0));
        e.roughness = Some(1.0 - sg.get("glossinessFactor").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0));
        if sg.get("specularGlossinessTexture").is_some() {
            e.ignore("specular_texture");
        }
    }
    if let Some(f) = m.pointer("/extensions/KHR_materials_specular/specularFactor").and_then(Value::as_f64) {
        e.specular.get_or_insert(f.clamp(0.0, 1.0));
    }
    let strength = m
        .pointer("/extensions/KHR_materials_emissive_strength/emissiveStrength")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    let texture = m.get("emissiveTexture").and_then(|i| texture_source(doc, dir, i));
    if m.get("emissiveTexture").is_some() && texture.is_none() {
        e.ignore("emissive_missing");
    }
    let factor = floats_of(m.get("emissiveFactor")).filter(|v| v.len() >= 3);
    let color = match (&factor, &texture) {
        (Some(f), _) => Some([f[0], f[1], f[2]]),
        (None, Some(_)) => Some([1.0; 3]),
        (None, None) => None,
    };
    if let Some(c) = color.map(|c| c.map(|v| (v * strength).max(0.0))).filter(|c| c.iter().any(|v| *v > 0.01)) {
        e.emissive = Some(c);
        e.emissive_texture = texture;
    }
    if m.get("occlusionTexture").is_some() {
        e.ignore("occlusion_texture");
    }
    if let Some(ext) = m.get("extensions").and_then(Value::as_object) {
        for k in ext.keys().filter(|k| !HANDLED.contains(&k.as_str())) {
            e.ignore(&format!("ext:{k}"));
        }
    }
}
