use std::collections::HashMap;
use std::fmt::Write as _;

use glam::{DVec2, DVec3};
use serde_json::{Value, json};

use crate::{Alpha, Material, Model, Triangle, UNITS_PER_METER, UpAxis};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportFormat {
    #[default]
    Gltf,
    Obj,
    Fbx,
}

impl ExportFormat {
    pub fn id(self) -> &'static str {
        match self {
            ExportFormat::Gltf => "gltf",
            ExportFormat::Obj => "obj",
            ExportFormat::Fbx => "fbx",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            ExportFormat::Gltf => "glb",
            ExportFormat::Obj => "obj",
            ExportFormat::Fbx => "fbx",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "gltf" | "glb" => Some(ExportFormat::Gltf),
            "obj" => Some(ExportFormat::Obj),
            "fbx" => Some(ExportFormat::Fbx),
            _ => None,
        }
    }

    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        Self::from_id(&path.extension()?.to_string_lossy())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportFile {
    pub name: String,
    pub data: Vec<u8>,
}

pub fn exchange(source: &Model) -> Model {
    let axes = |v: DVec3| DVec3::new(v.x, v.z, -v.y);
    let mut out = source.clone();
    out.up = UpAxis::Y;
    out.meters_per_unit = 1.0;
    for t in &mut out.triangles {
        for k in 0..3 {
            t.pos[k] = axes(t.pos[k]) / UNITS_PER_METER;
            t.normal[k] = axes(t.normal[k]);
        }
    }
    out
}

pub fn export(source: &Model, format: ExportFormat, stem: &str) -> Vec<ExportFile> {
    let model = exchange(source);
    let stem = file_stem(stem);
    match format {
        ExportFormat::Gltf => vec![ExportFile { name: format!("{stem}.glb"), data: glb(&model) }],
        ExportFormat::Obj => obj_files(&model, &stem),
        ExportFormat::Fbx => fbx_files(&model, &stem),
    }
}

fn file_stem(stem: &str) -> String {
    let s: String =
        stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    if s.is_empty() { "model".to_owned() } else { s }
}

fn used_materials(model: &Model) -> Vec<usize> {
    let mut used: Vec<usize> =
        model.triangles.iter().map(|t| t.material.min(model.materials.len().saturating_sub(1))).collect();
    used.sort_unstable();
    used.dedup();
    used
}

fn material_of(model: &Model, index: usize) -> Option<&Material> {
    model.materials.get(index)
}

fn material_label(model: &Model, index: usize) -> String {
    match material_of(model, index) {
        Some(m) if !m.name.trim().is_empty() => m.name.clone(),
        _ => format!("material_{index}"),
    }
}

fn texture_file(model: &Model, index: usize, stem: &str) -> Option<ExportFile> {
    let m = material_of(model, index)?;
    let (data, ext) = m.embedded.as_ref()?;
    let ext = if ext.is_empty() { "png" } else { ext.as_str() };
    Some(ExportFile { name: format!("{stem}_{}.{ext}", file_stem(&material_label(model, index))), data: data.clone() })
}

type VertexKey = ([u32; 3], [u32; 3], [u32; 2]);

fn quantized(t: &Triangle, k: usize) -> VertexKey {
    let f = |v: f64| (v as f32).to_bits();
    (
        [f(t.pos[k].x), f(t.pos[k].y), f(t.pos[k].z)],
        [f(t.normal[k].x), f(t.normal[k].y), f(t.normal[k].z)],
        [f(t.uv[k].x), f(t.uv[k].y)],
    )
}

fn align4(v: &mut Vec<u8>, pad: u8) {
    while !v.len().is_multiple_of(4) {
        v.push(pad);
    }
}

type Group<'a> = (String, Option<Value>, &'a [Triangle]);

fn groups(model: &Model) -> Vec<Group<'_>> {
    if model.objects.is_empty() {
        return vec![("HammerPro".to_owned(), None, &model.triangles[..])];
    }
    model.objects.iter().map(|o| (o.name.clone(), o.extras.clone(), model.object_triangles(o))).collect()
}

fn clamp_material(model: &Model, t: &Triangle) -> usize {
    t.material.min(model.materials.len().saturating_sub(1))
}

fn materials_in(model: &Model, tris: &[Triangle]) -> Vec<usize> {
    let mut used: Vec<usize> = tris.iter().map(|t| clamp_material(model, t)).collect();
    used.sort_unstable();
    used.dedup();
    used
}

fn glb(model: &Model) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut views: Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut materials: Vec<Value> = Vec::new();
    let mut textures: Vec<Value> = Vec::new();
    let mut images: Vec<Value> = Vec::new();
    let mut meshes: Vec<Value> = Vec::new();
    let mut nodes: Vec<Value> = Vec::new();
    let mut view = |bin: &mut Vec<u8>, bytes: &[u8], target: Option<u32>| {
        align4(bin, 0);
        let offset = bin.len();
        bin.extend_from_slice(bytes);
        let mut v = json!({ "buffer": 0, "byteOffset": offset, "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        views.push(v);
        views.len() - 1
    };
    let mut slot: HashMap<usize, usize> = HashMap::new();
    for index in used_materials(model) {
        let Some(m) = material_of(model, index) else { continue };
        let mut pbr = json!({
            "baseColorFactor": [m.color[0], m.color[1], m.color[2], m.color[3]],
            "metallicFactor": 0.0,
            "roughnessFactor": 0.8,
        });
        if let Some((data, ext)) = &m.embedded {
            let mime = if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
                "image/jpeg"
            } else {
                "image/png"
            };
            let bv = view(&mut bin, data, None);
            images.push(json!({ "bufferView": bv, "mimeType": mime, "name": material_label(model, index) }));
            textures.push(json!({ "source": images.len() - 1, "sampler": 0 }));
            pbr["baseColorTexture"] = json!({ "index": textures.len() - 1 });
        }
        let mut mat = json!({
            "name": material_label(model, index),
            "pbrMetallicRoughness": pbr,
            "doubleSided": m.double_sided,
        });
        match m.alpha {
            Alpha::Mask => mat["alphaMode"] = json!("MASK"),
            Alpha::Blend => mat["alphaMode"] = json!("BLEND"),
            _ => {}
        }
        materials.push(mat);
        slot.insert(index, materials.len() - 1);
    }
    for (name, extras, tris) in groups(model) {
        let mut primitives: Vec<Value> = Vec::new();
        for index in materials_in(model, tris) {
            let Some(&gltf_index) = slot.get(&index) else { continue };
            let mut lookup: HashMap<VertexKey, u32> = HashMap::new();
            let mut positions: Vec<f32> = Vec::new();
            let mut normals: Vec<f32> = Vec::new();
            let mut uvs: Vec<f32> = Vec::new();
            let mut indices: Vec<u32> = Vec::new();
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for t in tris.iter().filter(|t| clamp_material(model, t) == index) {
                for k in 0..3 {
                    let key = quantized(t, k);
                    let next = (positions.len() / 3) as u32;
                    let id = *lookup.entry(key).or_insert_with(|| {
                        let p = [t.pos[k].x as f32, t.pos[k].y as f32, t.pos[k].z as f32];
                        for a in 0..3 {
                            lo[a] = lo[a].min(p[a]);
                            hi[a] = hi[a].max(p[a]);
                        }
                        positions.extend_from_slice(&p);
                        normals.extend_from_slice(&[t.normal[k].x as f32, t.normal[k].y as f32, t.normal[k].z as f32]);
                        uvs.extend_from_slice(&[t.uv[k].x as f32, t.uv[k].y as f32]);
                        next
                    });
                    indices.push(id);
                }
            }
            let count = positions.len() / 3;
            let bytes = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
            let pv = view(&mut bin, &bytes(&positions), Some(34962));
            let nv = view(&mut bin, &bytes(&normals), Some(34962));
            let uv = view(&mut bin, &bytes(&uvs), Some(34962));
            let iv = view(&mut bin, &indices.iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<u8>>(), Some(34963));
            let mut acc = |v: usize, comp: u32, count: usize, kind: &str, extra: Option<(Value, Value)>| {
                let mut a = json!({ "bufferView": v, "componentType": comp, "count": count, "type": kind });
                if let Some((min, max)) = extra {
                    a["min"] = min;
                    a["max"] = max;
                }
                accessors.push(a);
                accessors.len() - 1
            };
            let pa = acc(pv, 5126, count, "VEC3", Some((json!(lo), json!(hi))));
            let na = acc(nv, 5126, count, "VEC3", None);
            let ua = acc(uv, 5126, count, "VEC2", None);
            let ia = acc(iv, 5125, indices.len(), "SCALAR", None);
            primitives.push(json!({
                "attributes": { "POSITION": pa, "NORMAL": na, "TEXCOORD_0": ua },
                "indices": ia,
                "material": gltf_index,
                "mode": 4,
            }));
        }
        if primitives.is_empty() {
            continue;
        }
        meshes.push(json!({ "name": name, "primitives": primitives }));
        let mut node = json!({ "mesh": meshes.len() - 1, "name": name });
        if let Some(e) = extras {
            node["extras"] = e;
        }
        nodes.push(node);
    }
    align4(&mut bin, 0);
    let node_ids: Vec<usize> = (0..nodes.len()).collect();
    let mut doc = json!({
        "asset": { "version": "2.0", "generator": "Hammer Pro" },
        "scene": 0,
        "scenes": [{ "nodes": node_ids }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{ "byteLength": bin.len() }],
    });
    if !images.is_empty() {
        doc["images"] = json!(images);
        doc["textures"] = json!(textures);
        doc["samplers"] = json!([{ "magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497 }]);
    }
    let mut text = doc.to_string().into_bytes();
    align4(&mut text, b' ');
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

fn obj_files(model: &Model, stem: &str) -> Vec<ExportFile> {
    let mut obj = String::from("# Exported by Hammer Pro (meters, Y up)\n");
    let mut mtl = String::from("# Exported by Hammer Pro\n");
    let _ = writeln!(obj, "mtllib {stem}.mtl");
    let mut files = Vec::new();
    for &index in &used_materials(model) {
        let name = obj_name(&material_label(model, index));
        let _ = writeln!(mtl, "newmtl {name}");
        let c = material_of(model, index).map_or([0.8, 0.8, 0.8, 1.0], |m| m.color);
        let _ = writeln!(mtl, "Kd {} {} {}", c[0], c[1], c[2]);
        if c[3] < 0.999 {
            let _ = writeln!(mtl, "d {}", c[3]);
        }
        if let Some(tex) = texture_file(model, index, stem) {
            let _ = writeln!(mtl, "map_Kd {}", tex.name);
            files.push(tex);
        }
    }
    let mut v: HashMap<[u32; 3], usize> = HashMap::new();
    let mut vt: HashMap<[u32; 2], usize> = HashMap::new();
    let mut vn: HashMap<[u32; 3], usize> = HashMap::new();
    let mut faces = String::new();
    let named = !model.objects.is_empty();
    for (name, _, tris) in groups(model) {
        if named {
            let _ = writeln!(faces, "o {}", obj_name(&name));
        }
        for index in materials_in(model, tris) {
            let _ = writeln!(faces, "usemtl {}", obj_name(&material_label(model, index)));
            for t in tris.iter().filter(|t| clamp_material(model, t) == index) {
                faces.push('f');
                for k in 0..3 {
                    let (p, n, u) = quantized(t, k);
                    let uv_key = [u[0], ((1.0 - t.uv[k].y) as f32).to_bits()];
                    let next = v.len() + 1;
                    let pi = *v.entry(p).or_insert_with(|| {
                        let _ = writeln!(obj, "v {} {} {}", t.pos[k].x, t.pos[k].y, t.pos[k].z);
                        next
                    });
                    let next = vt.len() + 1;
                    let ti = *vt.entry(uv_key).or_insert_with(|| {
                        let _ = writeln!(obj, "vt {} {}", t.uv[k].x, 1.0 - t.uv[k].y);
                        next
                    });
                    let next = vn.len() + 1;
                    let ni = *vn.entry(n).or_insert_with(|| {
                        let _ = writeln!(obj, "vn {} {} {}", t.normal[k].x, t.normal[k].y, t.normal[k].z);
                        next
                    });
                    let _ = write!(faces, " {pi}/{ti}/{ni}");
                }
                faces.push('\n');
            }
        }
    }
    obj.push_str(&faces);
    let mut out = vec![
        ExportFile { name: format!("{stem}.obj"), data: obj.into_bytes() },
        ExportFile { name: format!("{stem}.mtl"), data: mtl.into_bytes() },
    ];
    out.extend(files);
    out
}

fn obj_name(name: &str) -> String {
    name.replace('\\', "/").chars().map(|c| if c.is_whitespace() { '_' } else { c }).collect()
}

enum P<'a> {
    I(i32),
    L(i64),
    D(f64),
    S(&'a str),
    Dd(&'a [f64]),
    Ii(&'a [i32]),
}

struct W(Vec<u8>);

impl W {
    fn node(&mut self, name: &str, props: &[P], children: impl FnOnce(&mut W)) {
        let start = self.0.len();
        self.0.extend_from_slice(&[0u8; 12]);
        self.0.push(name.len() as u8);
        self.0.extend_from_slice(name.as_bytes());
        let props_start = self.0.len();
        for p in props {
            match p {
                P::I(v) => {
                    self.0.push(b'I');
                    self.0.extend_from_slice(&v.to_le_bytes());
                }
                P::L(v) => {
                    self.0.push(b'L');
                    self.0.extend_from_slice(&v.to_le_bytes());
                }
                P::D(v) => {
                    self.0.push(b'D');
                    self.0.extend_from_slice(&v.to_le_bytes());
                }
                P::S(s) => {
                    self.0.push(b'S');
                    self.0.extend_from_slice(&(s.len() as u32).to_le_bytes());
                    self.0.extend_from_slice(s.as_bytes());
                }
                P::Dd(a) => {
                    self.0.push(b'd');
                    self.0.extend_from_slice(&(a.len() as u32).to_le_bytes());
                    self.0.extend_from_slice(&0u32.to_le_bytes());
                    self.0.extend_from_slice(&((a.len() * 8) as u32).to_le_bytes());
                    for v in *a {
                        self.0.extend_from_slice(&v.to_le_bytes());
                    }
                }
                P::Ii(a) => {
                    self.0.push(b'i');
                    self.0.extend_from_slice(&(a.len() as u32).to_le_bytes());
                    self.0.extend_from_slice(&0u32.to_le_bytes());
                    self.0.extend_from_slice(&((a.len() * 4) as u32).to_le_bytes());
                    for v in *a {
                        self.0.extend_from_slice(&v.to_le_bytes());
                    }
                }
            }
        }
        let props_len = self.0.len() - props_start;
        let before = self.0.len();
        children(self);
        if self.0.len() != before {
            self.0.extend_from_slice(&[0u8; 13]);
        }
        let end = self.0.len() as u32;
        self.0[start..start + 4].copy_from_slice(&end.to_le_bytes());
        self.0[start + 4..start + 8].copy_from_slice(&(props.len() as u32).to_le_bytes());
        self.0[start + 8..start + 12].copy_from_slice(&(props_len as u32).to_le_bytes());
    }

    fn p(&mut self, key: &str, kind: &str, values: &[P]) {
        let (label, flag) = match kind {
            "int" => ("Integer", ""),
            "double" => ("Number", ""),
            _ => ("", "A"),
        };
        let mut props = vec![P::S(key), P::S(kind), P::S(label), P::S(flag)];
        for v in values {
            props.push(match v {
                P::I(x) => P::I(*x),
                P::D(x) => P::D(*x),
                P::L(x) => P::L(*x),
                P::S(x) => P::S(x),
                P::Dd(_) | P::Ii(_) => continue,
            });
        }
        self.node("P", &props, |_| {});
    }
}

struct FbxObject {
    name: String,
    vertices: Vec<f64>,
    polygon: Vec<i32>,
    normals: Vec<f64>,
    uv_values: Vec<f64>,
    uv_ids: Vec<i32>,
    mat_ids: Vec<i32>,
    used: Vec<usize>,
}

fn fbx_object(model: &Model, name: String, tris: &[Triangle]) -> FbxObject {
    let used = materials_in(model, tris);
    let mut position_index: HashMap<[u32; 3], i32> = HashMap::new();
    let mut uv_index: HashMap<[u32; 2], i32> = HashMap::new();
    let mut out = FbxObject {
        name,
        vertices: Vec::new(),
        polygon: Vec::new(),
        normals: Vec::new(),
        uv_values: Vec::new(),
        uv_ids: Vec::new(),
        mat_ids: Vec::new(),
        used: used.clone(),
    };
    let mut ordered: Vec<&Triangle> = Vec::new();
    for &index in &used {
        ordered.extend(tris.iter().filter(|t| clamp_material(model, t) == index));
    }
    for t in &ordered {
        let local = used.iter().position(|&u| u == clamp_material(model, t)).unwrap_or(0) as i32;
        out.mat_ids.push(local);
        for k in 0..3 {
            let (p, _, _) = quantized(t, k);
            let next = position_index.len() as i32;
            let id = *position_index.entry(p).or_insert_with(|| {
                out.vertices.extend_from_slice(&[t.pos[k].x, t.pos[k].y, t.pos[k].z]);
                next
            });
            out.polygon.push(if k == 2 { -id - 1 } else { id });
            out.normals.extend_from_slice(&[t.normal[k].x, t.normal[k].y, t.normal[k].z]);
            let uv = DVec2::new(t.uv[k].x, 1.0 - t.uv[k].y);
            let key = [(uv.x as f32).to_bits(), (uv.y as f32).to_bits()];
            let next = uv_index.len() as i32;
            let id = *uv_index.entry(key).or_insert_with(|| {
                out.uv_values.extend_from_slice(&[uv.x, uv.y]);
                next
            });
            out.uv_ids.push(id);
        }
    }
    out
}

fn fbx_files(model: &Model, stem: &str) -> Vec<ExportFile> {
    let all_used = used_materials(model);
    let objects: Vec<FbxObject> =
        groups(model).into_iter().map(|(name, _, tris)| fbx_object(model, name, tris)).collect();
    let mut files = Vec::new();
    let mut w = W(Vec::new());
    w.0.extend_from_slice(b"Kaydara FBX Binary  \0\x1a\0");
    w.0.extend_from_slice(&7400u32.to_le_bytes());
    w.node("FBXHeaderExtension", &[], |w| {
        w.node("FBXHeaderVersion", &[P::I(1003)], |_| {});
        w.node("FBXVersion", &[P::I(7400)], |_| {});
        w.node("Creator", &[P::S("Hammer Pro")], |_| {});
    });
    w.node("GlobalSettings", &[], |w| {
        w.node("Version", &[P::I(1000)], |_| {});
        w.node("Properties70", &[], |w| {
            w.p("UpAxis", "int", &[P::I(1)]);
            w.p("UpAxisSign", "int", &[P::I(1)]);
            w.p("FrontAxis", "int", &[P::I(2)]);
            w.p("FrontAxisSign", "int", &[P::I(1)]);
            w.p("CoordAxis", "int", &[P::I(0)]);
            w.p("CoordAxisSign", "int", &[P::I(1)]);
            w.p("UnitScaleFactor", "double", &[P::D(100.0)]);
            w.p("OriginalUnitScaleFactor", "double", &[P::D(100.0)]);
        });
    });
    w.node("Definitions", &[], |w| {
        w.node("Version", &[P::I(100)], |_| {});
        w.node("Count", &[P::I(2 * objects.len() as i32 + 2 * all_used.len() as i32)], |_| {});
        w.node("ObjectType", &[P::S("Geometry")], |w| w.node("Count", &[P::I(objects.len() as i32)], |_| {}));
        w.node("ObjectType", &[P::S("Model")], |w| w.node("Count", &[P::I(objects.len() as i32)], |_| {}));
        w.node("ObjectType", &[P::S("Material")], |w| w.node("Count", &[P::I(all_used.len() as i32)], |_| {}));
    });
    let tex_ids: Vec<Option<(i64, String)>> = all_used
        .iter()
        .enumerate()
        .map(|(n, &index)| {
            texture_file(model, index, stem).map(|f| {
                let name = f.name.clone();
                files.push(f);
                (3_000_000 + n as i64, name)
            })
        })
        .collect();
    w.node("Objects", &[], |w| {
        for (oi, o) in objects.iter().enumerate() {
            let geometry_id = 1_000_001 + 2 * oi as i64;
            let model_id = geometry_id + 1;
            w.node("Geometry", &[P::L(geometry_id), P::S(&format!("{}\0\x01Geometry", o.name)), P::S("Mesh")], |w| {
                w.node("Properties70", &[], |_| {});
                w.node("GeometryVersion", &[P::I(124)], |_| {});
                w.node("Vertices", &[P::Dd(&o.vertices)], |_| {});
                w.node("PolygonVertexIndex", &[P::Ii(&o.polygon)], |_| {});
                w.node("LayerElementNormal", &[P::I(0)], |w| {
                    w.node("Version", &[P::I(101)], |_| {});
                    w.node("Name", &[P::S("")], |_| {});
                    w.node("MappingInformationType", &[P::S("ByPolygonVertex")], |_| {});
                    w.node("ReferenceInformationType", &[P::S("Direct")], |_| {});
                    w.node("Normals", &[P::Dd(&o.normals)], |_| {});
                });
                w.node("LayerElementUV", &[P::I(0)], |w| {
                    w.node("Version", &[P::I(101)], |_| {});
                    w.node("Name", &[P::S("UVMap")], |_| {});
                    w.node("MappingInformationType", &[P::S("ByPolygonVertex")], |_| {});
                    w.node("ReferenceInformationType", &[P::S("IndexToDirect")], |_| {});
                    w.node("UV", &[P::Dd(&o.uv_values)], |_| {});
                    w.node("UVIndex", &[P::Ii(&o.uv_ids)], |_| {});
                });
                w.node("LayerElementMaterial", &[P::I(0)], |w| {
                    w.node("Version", &[P::I(101)], |_| {});
                    w.node("Name", &[P::S("")], |_| {});
                    w.node("MappingInformationType", &[P::S("ByPolygon")], |_| {});
                    w.node("ReferenceInformationType", &[P::S("IndexToDirect")], |_| {});
                    w.node("Materials", &[P::Ii(&o.mat_ids)], |_| {});
                });
                w.node("Layer", &[P::I(0)], |w| {
                    w.node("Version", &[P::I(100)], |_| {});
                    for kind in ["LayerElementNormal", "LayerElementUV", "LayerElementMaterial"] {
                        w.node("LayerElement", &[], |w| {
                            w.node("Type", &[P::S(kind)], |_| {});
                            w.node("TypedIndex", &[P::I(0)], |_| {});
                        });
                    }
                });
            });
            w.node("Model", &[P::L(model_id), P::S(&format!("{}\0\x01Model", o.name)), P::S("Mesh")], |w| {
                w.node("Version", &[P::I(232)], |_| {});
                w.node("Properties70", &[], |_| {});
                w.node("Culling", &[P::S("CullingOff")], |_| {});
            });
        }
        for (n, &index) in all_used.iter().enumerate() {
            let name = material_label(model, index);
            let c = material_of(model, index).map_or([0.8, 0.8, 0.8, 1.0], |m| m.color);
            let id = 2_000_000 + n as i64;
            w.node("Material", &[P::L(id), P::S(&format!("{name}\0\x01Material")), P::S("")], |w| {
                w.node("Version", &[P::I(102)], |_| {});
                w.node("ShadingModel", &[P::S("phong")], |_| {});
                w.node("Properties70", &[], |w| {
                    w.p("DiffuseColor", "Color", &[P::D(c[0]), P::D(c[1]), P::D(c[2])]);
                    w.p("Opacity", "double", &[P::D(c[3])]);
                });
            });
            if let Some((tid, file)) = &tex_ids[n] {
                w.node("Texture", &[P::L(*tid), P::S(&format!("{name}\0\x01Texture")), P::S("")], |w| {
                    w.node("Type", &[P::S("TextureVideoClip")], |_| {});
                    w.node("FileName", &[P::S(file)], |_| {});
                    w.node("RelativeFilename", &[P::S(file)], |_| {});
                });
            }
        }
    });
    w.node("Connections", &[], |w| {
        for (oi, o) in objects.iter().enumerate() {
            let geometry_id = 1_000_001 + 2 * oi as i64;
            let model_id = geometry_id + 1;
            w.node("C", &[P::S("OO"), P::L(model_id), P::L(0)], |_| {});
            w.node("C", &[P::S("OO"), P::L(geometry_id), P::L(model_id)], |_| {});
            for index in &o.used {
                let Some(n) = all_used.iter().position(|u| u == index) else { continue };
                w.node("C", &[P::S("OO"), P::L(2_000_000 + n as i64), P::L(model_id)], |_| {});
            }
        }
        for (n, tex) in tex_ids.iter().enumerate() {
            if let Some((tid, _)) = tex {
                w.node("C", &[P::S("OP"), P::L(*tid), P::L(2_000_000 + n as i64), P::S("DiffuseColor")], |_| {});
            }
        }
    });
    w.0.extend_from_slice(&[0u8; 13]);
    w.0.extend_from_slice(&[
        0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e,
    ]);
    w.0.extend_from_slice(&[0u8; 4]);
    while !w.0.len().is_multiple_of(16) {
        w.0.push(0);
    }
    w.0.extend_from_slice(&[0u8; 4]);
    w.0.extend_from_slice(&7400u32.to_le_bytes());
    w.0.extend_from_slice(&[0u8; 120]);
    w.0.extend_from_slice(&[
        0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b,
    ]);
    let mut out = vec![ExportFile { name: format!("{stem}.fbx"), data: w.0 }];
    out.extend(files);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ImportOptions, read};

    fn sample() -> Model {
        let tri = |a: [f64; 3], b: [f64; 3], c: [f64; 3], material: usize| Triangle {
            pos: [DVec3::from(a), DVec3::from(b), DVec3::from(c)],
            normal: [DVec3::Z; 3],
            uv: [DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(0.0, 0.25)],
            material,
        };
        Model {
            triangles: vec![
                tri([0.0, 0.0, 0.0], [64.0, 0.0, 0.0], [0.0, 32.0, 0.0], 0),
                tri([0.0, 0.0, 16.0], [64.0, 0.0, 16.0], [0.0, 32.0, 48.0], 1),
            ],
            materials: vec![Material::named("dev/floor 01"), Material::named("brick/wall")],
            up: UpAxis::Z,
            meters_per_unit: 1.0 / UNITS_PER_METER,
            objects: Vec::new(),
        }
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_meshio_export_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn roundtrip(format: ExportFormat) {
        let source = sample();
        let dir = scratch(format.id());
        let files = export(&source, format, "part");
        for f in &files {
            std::fs::write(dir.join(&f.name), &f.data).expect("write");
        }
        let back = read(&dir.join(&files[0].name)).expect("read back");
        let opts = ImportOptions { center: false, ..ImportOptions::default() };
        let back = back.to_source(&opts);
        assert_eq!(back.triangles.len(), 2, "{format:?}");
        let (lo, hi) = back.bounds().expect("bounds");
        let (slo, shi) = source.bounds().expect("bounds");
        assert!((lo - slo).length() < 1e-3 && (hi - shi).length() < 1e-3, "{format:?} {lo} {hi}");
        let names: Vec<&str> = back.materials.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names.len(), 2, "{format:?} {names:?}");
        for t in &back.triangles {
            assert!((t.uv[2] - DVec2::new(0.0, 0.25)).length() < 1e-5, "{format:?} {:?}", t.uv);
            assert!(t.normal[0].z > 0.99, "{format:?} {:?}", t.normal);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glb_round_trips_through_the_reader() {
        roundtrip(ExportFormat::Gltf);
    }

    #[test]
    fn obj_round_trips_through_the_reader() {
        roundtrip(ExportFormat::Obj);
    }

    #[test]
    fn fbx_round_trips_through_the_reader() {
        roundtrip(ExportFormat::Fbx);
    }

    #[test]
    fn textures_are_embedded_or_written_next_to_the_file() {
        let mut m = sample();
        m.materials[0].embedded = Some((vec![0x89, b'P', b'N', b'G', 1, 2, 3], "png".to_owned()));
        let glb = export(&m, ExportFormat::Gltf, "a");
        assert_eq!(glb.len(), 1);
        assert!(glb[0].data.windows(4).any(|w| w == b"\x89PNG"));
        let obj = export(&m, ExportFormat::Obj, "a");
        assert_eq!(obj.len(), 3);
        assert!(String::from_utf8_lossy(&obj[1].data).contains("map_Kd a_dev_floor_01.png"));
        let fbx = export(&m, ExportFormat::Fbx, "a");
        assert_eq!(fbx.len(), 2);
        assert_eq!(fbx[1].name, "a_dev_floor_01.png");
    }

    fn two_objects() -> Model {
        let mut m = sample();
        m.materials.truncate(1);
        for t in &mut m.triangles {
            t.material = 0;
        }
        let mut third = m.triangles[0];
        third.pos[0].z += 8.0;
        m.triangles.push(third);
        m.objects = vec![
            crate::MeshObject {
                name: "HPDISP_7_2_p3".to_owned(),
                extras: Some(serde_json::json!({ "hammerpro": { "solid": 7, "side": 2 } })),
                first: 0,
                count: 2,
            },
            crate::MeshObject { name: "Other".to_owned(), extras: None, first: 2, count: 1 },
        ];
        m
    }

    #[test]
    fn named_objects_survive_every_format_in_order() {
        for format in [ExportFormat::Gltf, ExportFormat::Obj, ExportFormat::Fbx] {
            let source = two_objects();
            let dir = scratch(&format!("objects_{}", format.id()));
            let files = export(&source, format, "set");
            for f in &files {
                std::fs::write(dir.join(&f.name), &f.data).expect("write");
            }
            let back = read(&dir.join(&files[0].name)).expect("read back");
            assert_eq!(back.objects.len(), 2, "{format:?}: {:?}", back.objects);
            let names: Vec<&str> = back.objects.iter().map(|o| o.name.as_str()).collect();
            assert!(names.contains(&"HPDISP_7_2_p3") && names.contains(&"Other"), "{format:?} {names:?}");
            let first = back.objects.iter().find(|o| o.name == "HPDISP_7_2_p3").expect("object");
            assert_eq!(first.count, 2, "{format:?}");
            let tris = back.object_triangles(first);
            assert_eq!(tris.len(), 2);
            let opts = ImportOptions { center: false, ..ImportOptions::default() };
            let tris: Vec<Triangle> = back.to_source(&opts).object_triangles(first).to_vec();
            for (a, b) in tris.iter().zip(&source.triangles[..2]) {
                for k in 0..3 {
                    assert!((a.pos[k] - b.pos[k]).length() < 1e-3, "{format:?} {:?} {:?}", a.pos[k], b.pos[k]);
                    assert!((a.uv[k] - b.uv[k]).length() < 1e-5, "{format:?} corner order is kept");
                }
            }
            if format == ExportFormat::Gltf {
                assert_eq!(first.extras.as_ref().map(|e| e["hammerpro"]["solid"].clone()), Some(serde_json::json!(7)));
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn formats_and_names() {
        assert_eq!(ExportFormat::from_id("GLB"), Some(ExportFormat::Gltf));
        assert_eq!(ExportFormat::from_id("fbx"), Some(ExportFormat::Fbx));
        assert_eq!(ExportFormat::from_id("stl"), None);
        assert_eq!(ExportFormat::default().ext(), "glb");
        assert_eq!(file_stem("my map: 1"), "my_map__1");
        assert_eq!(file_stem(""), "model");
    }

    #[test]
    fn the_exchange_layout_is_y_up_meters() {
        let m = exchange(&sample());
        assert_eq!(m.up, UpAxis::Y);
        let p = m.triangles[1].pos[2];
        assert!((p - DVec3::new(0.0, 48.0, -32.0) / UNITS_PER_METER).length() < 1e-9, "{p}");
    }
}
