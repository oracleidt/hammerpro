use std::path::{Path, PathBuf};

use glam::{DMat4, DVec2, DVec3};

pub mod export;
mod extras;
mod fbx;
mod gltf;
pub mod lod;
mod obj;
pub mod smd;
pub mod studio;

pub use extras::{MaterialExtras, NormalKind, TextureSource};

#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    #[error("cannot read {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("unsupported file type: {0} (OBJ, FBX, glTF and GLB are supported)")]
    Unsupported(String),
    #[error("{0}")]
    Invalid(String),
    #[error("the file has no mesh")]
    Empty,
}

pub type Result<T> = std::result::Result<T, MeshError>;

fn invalid(msg: impl Into<String>) -> MeshError {
    MeshError::Invalid(msg.into())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alpha {
    #[default]
    Auto,
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    pub texture: Option<PathBuf>,
    pub embedded: Option<(Vec<u8>, String)>,
    pub color: [f64; 4],
    pub alpha: Alpha,
    pub double_sided: bool,
    pub extras: MaterialExtras,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::new(),
            texture: None,
            embedded: None,
            color: [1.0; 4],
            alpha: Alpha::Auto,
            double_sided: false,
            extras: MaterialExtras::default(),
        }
    }
}

impl Material {
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub pos: [DVec3; 3],
    pub normal: [DVec3; 3],
    pub uv: [DVec2; 3],
    pub material: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UpAxis {
    #[default]
    Y,
    Z,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshObject {
    pub name: String,
    pub extras: Option<serde_json::Value>,
    pub first: usize,
    pub count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
    pub up: UpAxis,
    pub meters_per_unit: f64,
    pub objects: Vec<MeshObject>,
}

impl Model {
    pub fn object_triangles(&self, object: &MeshObject) -> &[Triangle] {
        let end = object.first.saturating_add(object.count).min(self.triangles.len());
        self.triangles.get(object.first.min(end)..end).unwrap_or(&[])
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImportOptions {
    pub up: Option<UpAxis>,
    pub scale: f64,
    pub convert_units: bool,
    pub center: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { up: None, scale: 1.0, convert_units: true, center: true }
    }
}

pub const UNITS_PER_METER: f64 = 39.370_078_740_157_48;

pub fn read(path: &Path) -> Result<Model> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| MeshError::Io(path.to_path_buf(), e))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let model = match ext.as_str() {
        "obj" => obj::read(&bytes, dir)?,
        "gltf" | "glb" => gltf::read(&bytes, dir)?,
        "fbx" => fbx::read(&bytes, dir)?,
        _ => return Err(MeshError::Unsupported(ext)),
    };
    if model.triangles.is_empty() {
        return Err(MeshError::Empty);
    }
    let mut model = model;
    model.merge_materials();
    Ok(model)
}

pub(crate) fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim().trim_matches('"').replace('\\', "/");
    if name.is_empty() {
        return None;
    }
    let file = Path::new(&name).file_name().map(PathBuf::from)?;
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut dirs = vec![dir.to_path_buf()];
    for sub in ["textures", "Textures", "texture", "tex", "maps", "images", "materials"] {
        dirs.push(dir.join(sub));
    }
    if let Some(parent) = dir.parent() {
        dirs.push(parent.to_path_buf());
        dirs.push(parent.join("textures"));
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fbm")) {
                dirs.push(p);
            }
        }
    }
    let mut candidates = vec![dir.join(&name), PathBuf::from(&name)];
    candidates.extend(dirs.iter().map(|d| d.join(&file)));
    for ext in ["png", "jpg", "jpeg", "tga", "bmp"] {
        candidates.extend(dirs.iter().map(|d| d.join(format!("{stem}.{ext}"))));
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub(crate) fn smooth_normals(positions: &[DVec3], polygons: &[(Vec<usize>, u32)], crease_deg: f64) -> Vec<Vec<DVec3>> {
    let face: Vec<DVec3> = polygons
        .iter()
        .map(|(idx, _)| {
            let mut n = DVec3::ZERO;
            for i in 0..idx.len() {
                let (a, b) = (positions[idx[i]], positions[idx[(i + 1) % idx.len()]]);
                n += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
            }
            n * 0.5
        })
        .collect();
    let mut around: std::collections::HashMap<(usize, u32), Vec<usize>> = std::collections::HashMap::new();
    for (f, (idx, group)) in polygons.iter().enumerate() {
        if *group != 0 {
            for &i in idx {
                around.entry((i, *group)).or_default().push(f);
            }
        }
    }
    let cos = crease_deg.to_radians().cos();
    polygons
        .iter()
        .enumerate()
        .map(|(f, (idx, group))| {
            let own = face[f].normalize_or_zero();
            idx.iter()
                .map(|&i| {
                    if *group == 0 {
                        return own;
                    }
                    let mut n = DVec3::ZERO;
                    for &g in around.get(&(i, *group)).map_or(&[][..], Vec::as_slice) {
                        if face[g].normalize_or_zero().dot(own) >= cos {
                            n += face[g];
                        }
                    }
                    let n = n.normalize_or_zero();
                    if n == DVec3::ZERO { own } else { n }
                })
                .collect()
        })
        .collect()
}

impl Model {
    pub fn to_source(&self, opts: &ImportOptions) -> Model {
        let up = opts.up.unwrap_or(self.up);
        let unit = if opts.convert_units { self.meters_per_unit * UNITS_PER_METER } else { 1.0 };
        let scale = unit * if opts.scale.is_finite() && opts.scale > 0.0 { opts.scale } else { 1.0 };
        let axes = |v: DVec3| match up {
            UpAxis::Y => DVec3::new(v.x, -v.z, v.y),
            UpAxis::Z => v,
        };
        let mut out = self.clone();
        out.up = UpAxis::Z;
        out.meters_per_unit = 1.0 / UNITS_PER_METER;
        for t in &mut out.triangles {
            for k in 0..3 {
                t.pos[k] = axes(t.pos[k]) * scale;
                t.normal[k] = axes(t.normal[k]).normalize_or_zero();
            }
        }
        if opts.center
            && let Some((lo, hi)) = out.bounds()
        {
            let shift = DVec3::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, lo.z);
            for t in &mut out.triangles {
                for p in &mut t.pos {
                    *p -= shift;
                }
            }
        }
        out
    }

    pub fn merge_materials(&mut self) {
        let used: std::collections::HashSet<usize> = self.triangles.iter().map(|t| t.material).collect();
        let mut kept: Vec<Material> = Vec::new();
        let mut remap = vec![0usize; self.materials.len()];
        for (i, m) in self.materials.iter().enumerate() {
            if !used.contains(&i) {
                continue;
            }
            remap[i] = match kept.iter().position(|k| k == m) {
                Some(k) => k,
                None => {
                    kept.push(m.clone());
                    kept.len() - 1
                }
            };
        }
        if kept.is_empty() {
            return;
        }
        for t in &mut self.triangles {
            t.material = remap.get(t.material).copied().unwrap_or(0);
        }
        self.materials = kept;
    }

    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut it = self.triangles.iter().flat_map(|t| t.pos.iter());
        let first = *it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))))
    }
}

pub(crate) fn push_polygon(
    out: &mut Vec<Triangle>,
    corners: &[(DVec3, Option<DVec3>, DVec2)],
    material: usize,
    xf: &DMat4,
) {
    if corners.len() < 3 {
        return;
    }
    let normal_xf = xf.inverse().transpose();
    let reversed: Vec<(DVec3, Option<DVec3>, DVec2)>;
    let corners = if xf.determinant() < 0.0 {
        reversed = corners.iter().rev().copied().collect();
        &reversed[..]
    } else {
        corners
    };
    let pts: Vec<DVec3> = corners.iter().map(|c| xf.transform_point3(c.0)).collect();
    let mut flat = DVec3::ZERO;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        flat += DVec3::new((a.y - b.y) * (a.z + b.z), (a.z - b.z) * (a.x + b.x), (a.x - b.x) * (a.y + b.y));
    }
    let flat = flat.normalize_or_zero();
    let n = |i: usize| {
        corners[i]
            .1
            .map(|v| normal_xf.transform_vector3(v).normalize_or_zero())
            .filter(|v| *v != DVec3::ZERO)
            .unwrap_or(flat)
    };
    for i in 1..corners.len() - 1 {
        let idx = [0, i, i + 1];
        let pos = idx.map(|k| pts[k]);
        if !pos.iter().all(|p| p.is_finite()) {
            continue;
        }
        out.push(Triangle { pos, normal: idx.map(n), uv: idx.map(|k| corners[k].2), material });
    }
}

pub(crate) fn base64(text: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_coordinates_and_base64() {
        let t = Triangle {
            pos: [DVec3::new(0.0, 0.0, 0.0), DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, 1.0, -1.0)],
            normal: [DVec3::Y; 3],
            uv: [DVec2::ZERO; 3],
            material: 0,
        };
        let m =
            Model { triangles: vec![t], materials: vec![], up: UpAxis::Y, meters_per_unit: 1.0, objects: Vec::new() };
        let s = m.to_source(&ImportOptions { center: false, ..ImportOptions::default() });
        assert!((s.triangles[0].pos[2] - DVec3::new(0.0, 1.0, 1.0) * UNITS_PER_METER).length() < 1e-9);
        assert_eq!(s.triangles[0].normal[0], DVec3::Z);
        let c = m.to_source(&ImportOptions { convert_units: false, ..ImportOptions::default() });
        let (lo, hi) = c.bounds().expect("bounds");
        assert!((lo.z).abs() < 1e-9 && (lo.x + hi.x).abs() < 1e-9 && (lo.y + hi.y).abs() < 1e-9, "{lo} {hi}");
        assert_eq!(base64("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("aGVsbG8").as_deref(), Some(&b"hello"[..]));
        assert_eq!(base64("a!"), None);
    }
}
