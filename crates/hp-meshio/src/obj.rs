use std::path::Path;

use glam::{DMat4, DVec2, DVec3};

use crate::{Alpha, Material, Model, Result, Triangle, UpAxis, find_file, invalid, push_polygon, smooth_normals};

const MAX_ITEMS: usize = 20_000_000;
const CREASE_DEG: f64 = 60.0;

type Corner = (usize, Option<usize>, Option<usize>);

fn floats<'a>(it: impl Iterator<Item = &'a str>) -> Vec<f64> {
    it.filter_map(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite()).collect()
}

fn index(token: &str, len: usize) -> Option<usize> {
    let i: i64 = token.parse().ok()?;
    let idx = if i < 0 { len as i64 + i } else { i - 1 };
    (0..len as i64).contains(&idx).then_some(idx as usize)
}

fn map_file(rest: &str) -> &str {
    let arity = |o: &str| match o {
        "-blendu" | "-blendv" | "-cc" | "-clamp" | "-imfchan" | "-type" | "-bm" | "-boost" | "-texres" => Some(1),
        "-mm" => Some(2),
        "-o" | "-s" | "-t" => Some(3),
        _ => None,
    };
    let mut s = rest.trim();
    while s.starts_with('-') {
        let (opt, tail) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
        let Some(n) = arity(opt) else { break };
        let mut tail = tail.trim_start();
        let mut taken = 0;
        while taken < n {
            let (word, more) = tail.split_once(char::is_whitespace).unwrap_or((tail, ""));
            if taken > 0 && word.parse::<f64>().is_err() {
                break;
            }
            tail = more.trim_start();
            taken += 1;
        }
        s = tail;
    }
    s.trim().trim_matches('"')
}

fn read_mtl(text: &str, dir: &Path, materials: &mut Vec<Material>) {
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let line = line.trim();
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let Some(i) = current else {
            if key == "newmtl" {
                current = Some(material_index(materials, rest.trim()));
            }
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "newmtl" => current = Some(material_index(materials, rest.trim())),
            "map_kd" => {
                if materials[i].texture.is_none() {
                    materials[i].texture = find_file(dir, map_file(rest));
                }
            }
            "kd" => {
                let f = floats(rest.split_whitespace());
                if f.len() >= 3 {
                    materials[i].color[..3].copy_from_slice(&f[..3]);
                }
            }
            "d" => {
                if let Some(&d) = floats(rest.split_whitespace()).first() {
                    materials[i].color[3] = d.clamp(0.0, 1.0);
                }
            }
            "tr" => {
                if let Some(&t) = floats(rest.split_whitespace()).first() {
                    materials[i].color[3] = (1.0 - t).clamp(0.0, 1.0);
                }
            }
            "map_d" if materials[i].alpha == Alpha::Auto => {
                materials[i].alpha = Alpha::Mask;
            }
            _ => {}
        }
    }
    for m in materials.iter_mut() {
        if m.color[3] < 0.999 && m.alpha == Alpha::Auto {
            m.alpha = Alpha::Blend;
        }
        if m.texture.is_some() {
            m.color[..3].copy_from_slice(&[1.0; 3]);
        }
    }
}

fn material_index(materials: &mut Vec<Material>, name: &str) -> usize {
    match materials.iter().position(|m| m.name == name) {
        Some(i) => i,
        None => {
            materials.push(Material::named(name));
            materials.len() - 1
        }
    }
}

pub(crate) fn read(bytes: &[u8], dir: &Path) -> Result<Model> {
    let text = String::from_utf8_lossy(bytes);
    let mut v: Vec<DVec3> = Vec::new();
    let mut vt: Vec<DVec2> = Vec::new();
    let mut vn: Vec<DVec3> = Vec::new();
    let mut materials: Vec<Material> = Vec::new();
    let mut current = usize::MAX;
    let mut group: u32 = 1;
    let mut has_groups = false;
    let mut faces: Vec<(Vec<Corner>, usize, u32)> = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match key {
            "v" => {
                let f = floats(rest.split_whitespace());
                if f.len() >= 3 && v.len() < MAX_ITEMS {
                    v.push(DVec3::new(f[0], f[1], f[2]));
                }
            }
            "vt" => {
                let f = floats(rest.split_whitespace());
                if !f.is_empty() && vt.len() < MAX_ITEMS {
                    vt.push(DVec2::new(f[0], f.get(1).copied().unwrap_or(0.0)));
                }
            }
            "vn" => {
                let f = floats(rest.split_whitespace());
                if f.len() >= 3 && vn.len() < MAX_ITEMS {
                    vn.push(DVec3::new(f[0], f[1], f[2]));
                }
            }
            "mtllib" => {
                let rest = rest.trim();
                let mut names = vec![rest.to_owned()];
                names.extend(rest.split_whitespace().map(str::to_owned));
                for name in names {
                    if let Some(t) = find_file(dir, &name).and_then(|p| std::fs::read(p).ok()) {
                        read_mtl(&String::from_utf8_lossy(&t), dir, &mut materials);
                        break;
                    }
                }
            }
            "usemtl" => current = material_index(&mut materials, rest.trim()),
            "s" => {
                has_groups = true;
                group = match rest.trim() {
                    "off" | "0" => 0,
                    g => g.parse::<u32>().unwrap_or(1).max(1),
                };
            }
            "f" => {
                let corners: Vec<Corner> = rest
                    .split_whitespace()
                    .filter_map(|c| {
                        let mut parts = c.split('/');
                        let pi = index(parts.next()?, v.len())?;
                        let ti = parts.next().filter(|s| !s.is_empty()).and_then(|s| index(s, vt.len()));
                        let ni = parts.next().filter(|s| !s.is_empty()).and_then(|s| index(s, vn.len()));
                        Some((pi, ti, ni))
                    })
                    .collect();
                if corners.len() >= 3 {
                    faces.push((corners, current, group));
                }
            }
            _ => {}
        }
    }
    if v.is_empty() && !text.trim().is_empty() && !text.contains("v ") {
        return Err(invalid("not an OBJ file"));
    }
    let default = materials.len();
    materials.push(Material::named("default"));
    let without: Vec<usize> = (0..faces.len()).filter(|&f| faces[f].0.iter().any(|c| c.2.is_none())).collect();
    let polys: Vec<(Vec<usize>, u32)> = without
        .iter()
        .map(|&f| (faces[f].0.iter().map(|c| c.0).collect(), if has_groups { faces[f].2 } else { 1 }))
        .collect();
    let crease = if has_groups { 180.0 } else { CREASE_DEG };
    let mut computed = smooth_normals(&v, &polys, crease).into_iter();
    let mut triangles: Vec<Triangle> = Vec::new();
    let mut next_without = without.iter().peekable();
    for (f, (corners, mat, _)) in faces.iter().enumerate() {
        let smooth = if next_without.peek() == Some(&&f) {
            next_without.next();
            computed.next()
        } else {
            None
        };
        let pts: Vec<(DVec3, Option<DVec3>, DVec2)> = corners
            .iter()
            .enumerate()
            .map(|(k, &(p, t, n))| {
                let normal = n.map(|n| vn[n]).or_else(|| smooth.as_ref().and_then(|s| s.get(k).copied()));
                let uv = t.map_or(DVec2::ZERO, |t| DVec2::new(vt[t].x, 1.0 - vt[t].y));
                (v[p], normal, uv)
            })
            .collect();
        let material = if *mat == usize::MAX { default } else { (*mat).min(default) };
        push_polygon(&mut triangles, &pts, material, &DMat4::IDENTITY);
    }
    Ok(Model { triangles, materials, up: UpAxis::Y, meters_per_unit: 1.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quad_with_a_material_and_negative_indices() {
        let dir = std::env::temp_dir().join("hp_meshio_obj");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("m.mtl"), "newmtl brick\nKd 0.5 0.25 1\nmap_Kd -s 1 1 1 -bm 0.5 tex.png\n").unwrap();
        std::fs::write(dir.join("tex.png"), b"x").unwrap();
        let obj = "mtllib m.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvn 0 0 1\nusemtl brick\nf -4/1/1 -3/2/1 -2/3/1 -1/4/1\n";
        let m = read(obj.as_bytes(), &dir).expect("read");
        assert_eq!(m.triangles.len(), 2, "a quad is two triangles");
        assert_eq!(m.materials[0].name, "brick");
        assert_eq!(m.materials[0].texture.as_deref(), Some(dir.join("tex.png").as_path()));
        assert_eq!(m.materials[0].color, [1.0, 1.0, 1.0, 1.0], "the texture replaces Kd");
        let t = &m.triangles[0];
        assert_eq!(t.pos[1], DVec3::new(1.0, 0.0, 0.0));
        assert_eq!(t.uv[0], DVec2::new(0.0, 1.0), "V flipped");
        assert_eq!(t.normal[0], DVec3::Z);
        let m = read(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n", &dir).expect("read");
        assert_eq!(m.triangles[0].normal[2], DVec3::Z);
        assert_eq!(m.materials[m.triangles[0].material].name, "default");
        let m = read(b"v 0 0 0\nf 1 2 3\n", &dir).expect("read");
        assert!(m.triangles.is_empty());
    }

    #[test]
    fn map_options_and_names_with_spaces() {
        assert_eq!(map_file("-s 1 1 1 -o 0 0 my texture.png"), "my texture.png");
        assert_eq!(map_file("-bm 0.3 -clamp on wood.jpg"), "wood.jpg");
        assert_eq!(map_file("-s 2 2 tex.png"), "tex.png");
        assert_eq!(map_file("\"quoted name.tga\""), "quoted name.tga");
        let dir = std::env::temp_dir().join("hp_meshio_obj_spaces");
        let _ = std::fs::create_dir_all(dir.join("textures"));
        std::fs::write(dir.join("my lib.mtl"), "newmtl a b\nmap_Kd C:\\elsewhere\\my wood.png\nd 0.5\n").unwrap();
        std::fs::write(dir.join("textures").join("my wood.png"), b"x").unwrap();
        let m = read(b"mtllib my lib.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl a b\nf 1 2 3\n", &dir).expect("read");
        assert_eq!(m.materials[0].name, "a b");
        assert_eq!(m.materials[0].texture.as_deref(), Some(dir.join("textures").join("my wood.png").as_path()));
        assert_eq!(m.materials[0].alpha, Alpha::Blend);
    }

    #[test]
    fn smooth_normals_without_vn() {
        let obj = "v 0 0 0\nv 1 0 0.2\nv 1 1 0.2\nv 0 1 0\nv 2 0 0\nv 2 1 0\nv 2 0 -1\nv 2 1 -1\n\
                   f 1 2 3 4\nf 2 5 6 3\nf 5 7 8 6\n";
        let m = read(obj.as_bytes(), Path::new(".")).expect("read");
        let shared = m.triangles[0].normal[1];
        assert!((shared - DVec3::Z).length() < 1e-9, "smoothed across the shallow ridge: {shared}");
        let wall = m.triangles[4].normal[0];
        assert!((wall - DVec3::X).length() < 1e-9, "hard against the wall: {wall}");
        let m = read(format!("s off\n{obj}").as_bytes(), Path::new(".")).expect("read");
        assert!((m.triangles[0].normal[1] - DVec3::Z).length() > 0.01);
    }
}
