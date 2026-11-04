use std::path::Path;

use glam::{DMat3, DMat4, DQuat, DVec2, DVec3};
use serde_json::Value;

use crate::{Alpha, Material, MeshObject, Model, Result, Triangle, UpAxis, base64, invalid, push_polygon};

const MAX_TRIANGLES: usize = 10_000_000;
const UNSUPPORTED: &[(&str, &str)] = &[
    ("KHR_draco_mesh_compression", "Draco compressed meshes"),
    ("EXT_meshopt_compression", "meshopt compressed meshes"),
    ("KHR_meshopt_compression", "meshopt compressed meshes"),
];

struct Doc<'a> {
    json: Value,
    buffers: Vec<std::borrow::Cow<'a, [u8]>>,
}

fn arr<'v>(v: &'v Value, key: &str) -> &'v [Value] {
    v.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn num(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

fn floats_of(v: Option<&Value>) -> Option<Vec<f64>> {
    Some(v?.as_array()?.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
}

fn load_uri(uri: &str, dir: &Path) -> Option<Vec<u8>> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let (_, data) = rest.split_once(";base64,")?;
        return base64(data);
    }
    std::fs::read(dir.join(percent_decode(uri))).ok()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn image_ext(mime: Option<&str>, uri: &str) -> String {
    match mime {
        Some("image/jpeg") => "jpg".to_owned(),
        Some("image/png") => "png".to_owned(),
        Some(m) => m.rsplit('/').next().unwrap_or("png").to_owned(),
        None if uri.starts_with("data:image/jpeg") => "jpg".to_owned(),
        None => Path::new(uri.split('?').next().unwrap_or(uri))
            .extension()
            .map_or_else(|| "png".to_owned(), |e| e.to_string_lossy().to_ascii_lowercase()),
    }
}

impl Doc<'_> {
    fn view(&self, index: usize) -> Option<(&[u8], usize)> {
        let v = arr(&self.json, "bufferViews").get(index)?;
        let buf = self.buffers.get(num(v, "buffer")? as usize)?;
        let off = num(v, "byteOffset").unwrap_or(0) as usize;
        let len = num(v, "byteLength")? as usize;
        let stride = num(v, "byteStride").unwrap_or(0) as usize;
        Some((buf.get(off..off.checked_add(len)?)?, stride))
    }

    fn read_floats(&self, index: usize, n: usize) -> Option<Vec<f64>> {
        let a = arr(&self.json, "accessors").get(index)?;
        let count = (num(a, "count")? as usize).min(MAX_TRIANGLES * 3);
        let comp = num(a, "componentType")?;
        let normalized = a.get("normalized").and_then(Value::as_bool).unwrap_or(false);
        let mut out = match num(a, "bufferView") {
            Some(view) => {
                let (data, stride) = self.view(view as usize)?;
                let base = num(a, "byteOffset").unwrap_or(0) as usize;
                read_values(data, base, stride, comp, normalized, count, n)?
            }
            None => vec![0.0; count * n],
        };
        if let Some(sparse) = a.get("sparse") {
            let k = num(sparse, "count")? as usize;
            let idx = sparse.get("indices")?;
            let (idata, _) = self.view(num(idx, "bufferView")? as usize)?;
            let ioff = num(idx, "byteOffset").unwrap_or(0) as usize;
            let indices = read_values(idata, ioff, 0, num(idx, "componentType")?, false, k, 1)?;
            let vals = sparse.get("values")?;
            let (vdata, _) = self.view(num(vals, "bufferView")? as usize)?;
            let voff = num(vals, "byteOffset").unwrap_or(0) as usize;
            let values = read_values(vdata, voff, 0, comp, normalized, k, n)?;
            for (j, &i) in indices.iter().enumerate() {
                let i = i as usize;
                if i < count {
                    out[i * n..i * n + n].copy_from_slice(&values[j * n..j * n + n]);
                }
            }
        }
        Some(out)
    }

    fn read_matrices(&self, index: usize) -> Option<Vec<DMat4>> {
        Some(self.read_floats(index, 16)?.chunks_exact(16).map(DMat4::from_cols_slice).collect())
    }
}

fn read_values(
    data: &[u8],
    base: usize,
    stride: usize,
    comp: u64,
    normalized: bool,
    count: usize,
    n: usize,
) -> Option<Vec<f64>> {
    let size = match comp {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return None,
    };
    let stride = if stride == 0 { size * n } else { stride };
    let mut out = Vec::with_capacity(count * n);
    for i in 0..count {
        for c in 0..n {
            let at = base + i * stride + c * size;
            let b = data.get(at..at + size)?;
            let v = match comp {
                5126 => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                5125 => f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                5123 => {
                    let v = f64::from(u16::from_le_bytes([b[0], b[1]]));
                    if normalized { v / 65535.0 } else { v }
                }
                5122 => {
                    let v = f64::from(i16::from_le_bytes([b[0], b[1]]));
                    if normalized { (v / 32767.0).max(-1.0) } else { v }
                }
                5121 => {
                    let v = f64::from(b[0]);
                    if normalized { v / 255.0 } else { v }
                }
                _ => {
                    let v = f64::from(b[0] as i8);
                    if normalized { (v / 127.0).max(-1.0) } else { v }
                }
            };
            out.push(v);
        }
    }
    Some(out)
}

fn node_matrix(n: &Value) -> DMat4 {
    if let Some(m) = floats_of(n.get("matrix"))
        && m.len() == 16
    {
        return DMat4::from_cols_slice(&m);
    }
    let vec = |key: &str, default: &[f64]| floats_of(n.get(key)).unwrap_or_else(|| default.to_vec());
    let t = vec("translation", &[0.0, 0.0, 0.0]);
    let r = vec("rotation", &[0.0, 0.0, 0.0, 1.0]);
    let s = vec("scale", &[1.0, 1.0, 1.0]);
    if t.len() < 3 || r.len() < 4 || s.len() < 3 {
        return DMat4::IDENTITY;
    }
    let q = DQuat::from_xyzw(r[0], r[1], r[2], r[3]);
    let q = if q.length_squared() > 1e-12 { q.normalize() } else { DQuat::IDENTITY };
    DMat4::from_scale_rotation_translation(DVec3::new(s[0], s[1], s[2]), q, DVec3::new(t[0], t[1], t[2]))
}

fn uv_transform(info: &Value) -> (usize, DMat3) {
    let mut set = num(info, "texCoord").unwrap_or(0) as usize;
    let Some(t) = info.pointer("/extensions/KHR_texture_transform") else {
        return (set, DMat3::IDENTITY);
    };
    if let Some(s) = num(t, "texCoord") {
        set = s as usize;
    }
    let off = floats_of(t.get("offset")).unwrap_or_else(|| vec![0.0, 0.0]);
    let scale = floats_of(t.get("scale")).unwrap_or_else(|| vec![1.0, 1.0]);
    let rot = t.get("rotation").and_then(Value::as_f64).unwrap_or(0.0);
    if off.len() < 2 || scale.len() < 2 {
        return (set, DMat3::IDENTITY);
    }
    let (s, c) = rot.sin_cos();
    let m = DMat3::from_cols(DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, 1.0, 0.0), DVec3::new(off[0], off[1], 1.0))
        * DMat3::from_cols(DVec3::new(c, s, 0.0), DVec3::new(-s, c, 0.0), DVec3::new(0.0, 0.0, 1.0))
        * DMat3::from_diagonal(DVec3::new(scale[0], scale[1], 1.0));
    (set, m)
}

mod material;
use material::{plain_transform, read_extras, texture_source};

struct MaterialInfo {
    uv_set: usize,
    uv_xf: DMat3,
}

pub(crate) fn read(bytes: &[u8], dir: &Path) -> Result<Model> {
    let (json_text, bin): (String, Option<&[u8]>) = if bytes.starts_with(b"glTF") {
        let u32_at = |o: usize| bytes.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let json_len = u32_at(12).ok_or_else(|| invalid("damaged GLB"))?;
        let json = bytes.get(20..20 + json_len).ok_or_else(|| invalid("damaged GLB"))?;
        let bin_at = 20 + json_len;
        let bin = u32_at(bin_at).and_then(|len| bytes.get(bin_at + 8..bin_at + 8 + len));
        (String::from_utf8_lossy(json).into_owned(), bin)
    } else {
        (String::from_utf8_lossy(bytes).into_owned(), None)
    };
    let json: Value = serde_json::from_str(&json_text).map_err(|e| invalid(format!("glTF: {e}")))?;
    let required: Vec<&str> = arr(&json, "extensionsRequired").iter().filter_map(Value::as_str).collect();
    for (ext, what) in UNSUPPORTED {
        if required.contains(ext) {
            return Err(invalid(format!(
                "glTF: {what} ({ext}) are not supported: export the model without compression"
            )));
        }
    }
    let mut buffers = Vec::new();
    for (i, b) in arr(&json, "buffers").iter().enumerate() {
        let data = match b.get("uri").and_then(Value::as_str) {
            Some(uri) => match load_uri(uri, dir) {
                Some(d) => Some(std::borrow::Cow::Owned(d)),
                None => return Err(invalid(format!("glTF: missing buffer file {}", percent_decode(uri)))),
            },
            None if i == 0 => bin.map(std::borrow::Cow::Borrowed),
            None => None,
        };
        buffers.push(data.unwrap_or_default());
    }
    let doc = Doc { json, buffers };

    let mut materials: Vec<Material> = Vec::new();
    let mut infos: Vec<MaterialInfo> = Vec::new();
    for (i, m) in arr(&doc.json, "materials").iter().enumerate() {
        let name = m.get("name").and_then(Value::as_str).map_or_else(|| format!("material_{i}"), str::to_owned);
        let mut mat = Material::named(name);
        let pbr = m.get("pbrMetallicRoughness");
        let spec_gloss = m.pointer("/extensions/KHR_materials_pbrSpecularGlossiness");
        let factor = floats_of(pbr.and_then(|p| p.get("baseColorFactor")))
            .or_else(|| floats_of(spec_gloss.and_then(|p| p.get("diffuseFactor"))));
        if let Some(f) = factor.filter(|f| f.len() >= 4) {
            mat.color = [f[0], f[1], f[2], f[3]].map(|v| v.clamp(0.0, 1.0));
        }
        mat.alpha = match m.get("alphaMode").and_then(Value::as_str) {
            Some("MASK") => Alpha::Mask,
            Some("BLEND") => Alpha::Blend,
            _ => Alpha::Opaque,
        };
        mat.double_sided = m.get("doubleSided").and_then(Value::as_bool).unwrap_or(false);
        let tex_info =
            pbr.and_then(|p| p.get("baseColorTexture")).or_else(|| spec_gloss.and_then(|p| p.get("diffuseTexture")));
        let (uv_set, mut uv_xf) = tex_info.map_or((0, DMat3::IDENTITY), uv_transform);
        if let Some((scale, offset)) = tex_info.and_then(plain_transform) {
            mat.extras.uv_scale = scale;
            mat.extras.uv_offset = offset;
            uv_xf = DMat3::IDENTITY;
        }
        if let Some(src) = tex_info.and_then(|t| texture_source(&doc, dir, t)) {
            mat.texture = src.path;
            mat.embedded = src.embedded;
        }
        read_extras(&doc, dir, m, &mut mat);
        materials.push(mat);
        infos.push(MaterialInfo { uv_set, uv_xf });
    }
    let default_material = materials.len();
    materials.push(Material::named("default"));
    infos.push(MaterialInfo { uv_set: 0, uv_xf: DMat3::IDENTITY });

    let nodes = arr(&doc.json, "nodes");
    let scene = num(&doc.json, "scene").unwrap_or(0) as usize;
    let mut roots: Vec<usize> = arr(&doc.json, "scenes")
        .get(scene)
        .map(|s| arr(s, "nodes").iter().filter_map(|n| n.as_u64().map(|n| n as usize)).collect())
        .unwrap_or_default();
    if roots.is_empty() {
        let children: std::collections::HashSet<u64> =
            nodes.iter().flat_map(|n| arr(n, "children").iter().filter_map(Value::as_u64)).collect();
        roots = (0..nodes.len()).filter(|i| !children.contains(&(*i as u64))).collect();
    }
    let mut world: Vec<Option<DMat4>> = vec![None; nodes.len()];
    let mut order: Vec<usize> = Vec::new();
    let mut stack: Vec<(usize, DMat4, usize)> = roots.into_iter().map(|r| (r, DMat4::IDENTITY, 0)).collect();
    while let Some((i, parent, depth)) = stack.pop() {
        let Some(node) = nodes.get(i) else { continue };
        if depth > 64 || world[i].is_some() {
            continue;
        }
        let xf = parent * node_matrix(node);
        world[i] = Some(xf);
        order.push(i);
        for c in arr(node, "children").iter().filter_map(Value::as_u64) {
            stack.push((c as usize, xf, depth + 1));
        }
    }
    let skins: Vec<Option<Vec<DMat4>>> = arr(&doc.json, "skins")
        .iter()
        .map(|s| {
            let joints: Vec<usize> = arr(s, "joints").iter().filter_map(|j| j.as_u64().map(|j| j as usize)).collect();
            let ibm = num(s, "inverseBindMatrices").and_then(|a| doc.read_matrices(a as usize));
            joints
                .iter()
                .enumerate()
                .map(|(k, &j)| {
                    let w = world.get(j).copied().flatten().or_else(|| nodes.get(j).map(node_matrix))?;
                    Some(w * ibm.as_ref().and_then(|m| m.get(k).copied()).unwrap_or(DMat4::IDENTITY))
                })
                .collect()
        })
        .collect();

    let mut triangles: Vec<Triangle> = Vec::new();
    let mut objects: Vec<MeshObject> = Vec::new();
    for i in order {
        let node = &nodes[i];
        let first = triangles.len();
        let xf = world[i].unwrap_or(DMat4::IDENTITY);
        let Some(mesh) = num(node, "mesh").and_then(|m| arr(&doc.json, "meshes").get(m as usize)) else { continue };
        let skin = num(node, "skin").and_then(|s| skins.get(s as usize)).and_then(Option::as_ref);
        for prim in arr(mesh, "primitives") {
            let mode = num(prim, "mode").unwrap_or(4);
            if !matches!(mode, 4..=6) {
                continue;
            }
            let attrs = prim.get("attributes");
            let attr =
                |key: &str, n: usize| attrs.and_then(|a| num(a, key)).and_then(|a| doc.read_floats(a as usize, n));
            let Some(pos) = attr("POSITION", 3) else { continue };
            let count = pos.len() / 3;
            let normals = attr("NORMAL", 3);
            let material = num(prim, "material").map_or(default_material, |m| (m as usize).min(default_material));
            let info = &infos[material];
            let uvs = attr(&format!("TEXCOORD_{}", info.uv_set), 2).or_else(|| attr("TEXCOORD_0", 2));
            let skinning: Option<Vec<DMat4>> = skin.and_then(|joints| {
                let j = attr("JOINTS_0", 4)?;
                let w = attr("WEIGHTS_0", 4)?;
                Some(
                    (0..count)
                        .map(|v| {
                            let mut m = DMat4::ZERO;
                            let mut total = 0.0;
                            for k in 0..4 {
                                let weight = w.get(v * 4 + k).copied().unwrap_or(0.0);
                                let joint = j.get(v * 4 + k).map(|&x| x as usize);
                                if weight > 0.0
                                    && let Some(jm) = joint.and_then(|x| joints.get(x))
                                {
                                    m += *jm * weight;
                                    total += weight;
                                }
                            }
                            if total > 1e-9 { m * (1.0 / total) } else { xf }
                        })
                        .collect(),
                )
            });
            let mut indices: Vec<usize> = match num(prim, "indices") {
                Some(a) => doc.read_floats(a as usize, 1).unwrap_or_default().into_iter().map(|v| v as usize).collect(),
                None => (0..count).collect(),
            };
            if mode != 4 {
                let mut list = Vec::with_capacity(indices.len().saturating_sub(2) * 3);
                for k in 2..indices.len() {
                    let tri = match (mode, k % 2) {
                        (5, 0) => [indices[k - 2], indices[k - 1], indices[k]],
                        (5, _) => [indices[k - 1], indices[k - 2], indices[k]],
                        _ => [indices[0], indices[k - 1], indices[k]],
                    };
                    list.extend_from_slice(&tri);
                }
                indices = list;
            }
            for tri in indices.chunks_exact(3) {
                if tri.iter().any(|&k| k >= count) {
                    continue;
                }
                let corner = |k: usize| {
                    let p = DVec3::new(pos[k * 3], pos[k * 3 + 1], pos[k * 3 + 2]);
                    let n =
                        normals.as_ref().and_then(|n| n.get(k * 3..k * 3 + 3)).map(|n| DVec3::new(n[0], n[1], n[2]));
                    let uv = uvs.as_ref().and_then(|u| u.get(k * 2..k * 2 + 2)).map_or(DVec2::ZERO, |u| {
                        let t = info.uv_xf * DVec3::new(u[0], u[1], 1.0);
                        DVec2::new(t.x, t.y)
                    });
                    (p, n, uv)
                };
                match &skinning {
                    Some(per_vertex) => {
                        let corners: Vec<(DVec3, Option<DVec3>, DVec2)> = tri
                            .iter()
                            .map(|&k| {
                                let (p, n, uv) = corner(k);
                                let m = per_vertex[k];
                                let nm = m.inverse().transpose();
                                (m.transform_point3(p), n.map(|n| nm.transform_vector3(n)), uv)
                            })
                            .collect();
                        push_polygon(&mut triangles, &corners, material, &DMat4::IDENTITY);
                    }
                    None => {
                        let corners: Vec<(DVec3, Option<DVec3>, DVec2)> = tri.iter().map(|&k| corner(k)).collect();
                        push_polygon(&mut triangles, &corners, material, &xf);
                    }
                }
            }
        }
        if triangles.len() > first {
            let name = node.get("name").or_else(|| mesh.get("name")).and_then(Value::as_str).unwrap_or_default();
            let extras = node.get("extras").or_else(|| mesh.get("extras")).cloned();
            objects.push(MeshObject { name: name.to_owned(), extras, first, count: triangles.len() - first });
        }
    }
    if triangles.is_empty() && arr(&doc.json, "meshes").iter().any(|m| !arr(m, "primitives").is_empty()) {
        let used: Vec<&str> = arr(&doc.json, "extensionsUsed").iter().filter_map(Value::as_str).collect();
        if let Some((ext, what)) = UNSUPPORTED.iter().find(|(e, _)| used.contains(e)) {
            return Err(invalid(format!(
                "glTF: {what} ({ext}) are not supported: export the model without compression"
            )));
        }
    }
    Ok(Model { triangles, materials, up: UpAxis::Y, meters_per_unit: 1.0, objects })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode64(data: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for c in data.chunks(3) {
            let n = (u32::from(c[0]) << 16)
                | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                | u32::from(*c.get(2).unwrap_or(&0));
            for k in 0..4 {
                if k <= c.len() {
                    s.push(T[((n >> (18 - 6 * k)) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }

    #[test]
    fn a_triangle_from_an_embedded_buffer() {
        let mut bin = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        let b64 = encode64(&bin);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"mesh":0,"translation":[10,0,0]}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"material":0}}]}}],
            "materials":[{{"name":"red","pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,1]}},"doubleSided":true}}],
            "buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{b64}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":6}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                         {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}]}}"#,
            len = bin.len()
        );
        let m = read(json.as_bytes(), Path::new(".")).expect("read");
        assert_eq!(m.triangles.len(), 1);
        assert_eq!(m.triangles[0].pos[1], DVec3::new(11.0, 0.0, 0.0), "node translation applied");
        let mat = &m.materials[m.triangles[0].material];
        assert_eq!(mat.name, "red");
        assert_eq!(mat.color, [1.0, 0.0, 0.0, 1.0]);
        assert!(mat.double_sided);
        assert_eq!(mat.alpha, Alpha::Opaque);
        assert_eq!(m.triangles[0].normal[0], DVec3::Z, "flat normal when the file has none");
        assert!(read(b"{ not json", Path::new(".")).is_err());
        let draco = r#"{"asset":{"version":"2.0"},"extensionsRequired":["KHR_draco_mesh_compression"],"meshes":[]}"#;
        let e = read(draco.as_bytes(), Path::new(".")).unwrap_err().to_string();
        assert!(e.contains("Draco"), "{e}");
    }

    #[test]
    fn material_extras_are_read() {
        let dir = std::env::temp_dir().join("hp_meshio_gltf_extras");
        let _ = std::fs::create_dir_all(&dir);
        for f in ["n.png", "e.png", "c.png"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let json = r#"{"asset":{"version":"2.0"},
            "images":[{"uri":"c.png"},{"uri":"n.png"},{"uri":"e.png"}],
            "textures":[{"source":0},{"source":1},{"source":2}],
            "materials":[{"name":"steel",
              "pbrMetallicRoughness":{"metallicFactor":0.9,"roughnessFactor":0.3,
                "baseColorTexture":{"index":0,"extensions":{"KHR_texture_transform":{"scale":[2,3],"offset":[0.25,0.5]}}}},
              "normalTexture":{"index":1},"emissiveTexture":{"index":2},"emissiveFactor":[1,0.5,0],
              "occlusionTexture":{"index":0},"extensions":{"KHR_materials_clearcoat":{}}},
              {"name":"spun","pbrMetallicRoughness":{"baseColorTexture":{"index":0,
                "extensions":{"KHR_texture_transform":{"rotation":0.5}}}}}]}"#;
        let m = read(json.as_bytes(), &dir).expect("read");
        let e = &m.materials[0].extras;
        assert_eq!(e.uv_scale, [2.0, 3.0]);
        assert_eq!(e.uv_offset, [0.25, 0.5]);
        assert_eq!(e.metallic, Some(0.9));
        assert_eq!(e.roughness, Some(0.3));
        assert_eq!(e.normal.as_ref().map(|n| n.1), Some(crate::NormalKind::OpenGl));
        assert_eq!(e.emissive, Some([1.0, 0.5, 0.0]));
        assert!(e.emissive_texture.is_some());
        assert_eq!(e.ignored, ["occlusion_texture", "ext:KHR_materials_clearcoat"]);
        assert!(!m.materials[1].extras.has_uv_transform(), "a rotation stays baked in the UVs");
    }
    #[test]
    fn a_skinned_triangle_in_its_bind_pose() {
        let mut bin = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        for _ in 0..3 {
            bin.extend_from_slice(&[0, 0, 0, 0]);
        }
        for _ in 0..3 {
            for w in [1.0f32, 0.0, 0.0, 0.0] {
                bin.extend_from_slice(&w.to_le_bytes());
            }
        }
        for v in [1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0,1]}}],
            "nodes":[{{"mesh":0,"skin":0,"translation":[100,0,0]}},{{"translation":[0,0,5],"scale":[2,2,2]}}],
            "skins":[{{"joints":[1],"inverseBindMatrices":3}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2}}}}]}}],
            "buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{b64}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":12}},
                           {{"buffer":0,"byteOffset":48,"byteLength":48}},{{"buffer":0,"byteOffset":96,"byteLength":64}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                         {{"bufferView":1,"componentType":5121,"count":3,"type":"VEC4"}},
                         {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},
                         {{"bufferView":3,"componentType":5126,"count":1,"type":"MAT4"}}]}}"#,
            len = bin.len(),
            b64 = encode64(&bin)
        );
        let m = read(json.as_bytes(), Path::new(".")).expect("read");
        assert_eq!(m.triangles.len(), 1);
        assert_eq!(m.triangles[0].pos[1], DVec3::new(2.0, 0.0, 5.0));
    }
}
