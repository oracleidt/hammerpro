//! Stage compiler inputs and distinguish copied materials from reused source assets.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use hp_editor::prop_export::PropGeometry;
use hp_fs::GameFs;

fn file_stem(name: &str, kind: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in name.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    let leaf: String = name
        .rsplit('/')
        .next()
        .unwrap_or(kind)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .take(32)
        .collect();
    format!("{kind}_{leaf}_{hash:016x}")
}

fn qualified(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_owned() } else { format!("{folder}/{name}") }
}

fn texture_parameter(key: &str, value: &str) -> bool {
    if !hp_fs::deps::is_material_texture_key(key) {
        return false;
    }
    let value = value.trim();
    !value.is_empty()
        && !value.starts_with(['[', '{', '$', '%'])
        && value.parse::<f64>().is_err()
        && !value.eq_ignore_ascii_case("env_cubemap")
        && !value.to_ascii_lowercase().starts_with("_rt_")
}

struct Export<'a> {
    fs: &'a GameFs,
    generated: BTreeMap<String, &'a [u8]>,
    folder: &'a str,
    textures: HashMap<(bool, String), String>,
    files: BTreeMap<String, Vec<u8>>,
    publish: BTreeSet<String>,
    preserve_paths: bool,
}

impl Export<'_> {
    fn texture(&mut self, name: &str, normal: bool) -> Result<String, String> {
        let name = hp_vmt::normalize_texture_path(name);
        if let Some(new) = self.textures.get(&(self.preserve_paths, name.clone())) {
            return Ok(new.clone());
        }
        let original = format!("materials/{name}.vtf");
        let (bytes, created) = match self.generated.get(&original) {
            Some(bytes) => (bytes.to_vec(), true),
            None => match self.fs.load_texture_raw(&name) {
                Some(bytes) => (bytes, false),
                None => {
                    let image = self.fs.load_texture(&name, None).map_err(|e| format!("{name}: {e}"))?;
                    let image = hp_vtf::RgbaImage::new(image.width, image.height, image.rgba)
                        .map_err(|e| format!("{name}: {e}"))?;
                    let options = hp_vtf::EncodeOptions {
                        flags: if normal { hp_vtf::flags::NORMAL } else { 0 },
                        ..Default::default()
                    };
                    (hp_vtf::encode(&image, &options).map_err(|e| format!("{name}: {e}"))?, true)
                }
            },
        };
        let texture = if self.preserve_paths { name.clone() } else { qualified(self.folder, &file_stem(&name, "t")) };
        let path = format!("materials/{texture}.vtf");
        if !self.preserve_paths || created {
            self.publish.insert(path.clone());
        }
        self.files.insert(path, bytes);
        self.textures.insert((self.preserve_paths, name), texture.clone());
        Ok(texture)
    }

    fn parameters(&mut self, pairs: &mut [(String, String)]) -> Result<(), String> {
        for (key, value) in pairs {
            if texture_parameter(key, value) {
                // The exact Source VTF is copied when present, including its mipmaps,
                // alpha, format, normal-map flags, animation frames and cubemap faces.
                *value = self.texture(
                    value,
                    key.to_ascii_lowercase().contains("bump") || key.eq_ignore_ascii_case("$normalmap"),
                )?;
            }
        }
        Ok(())
    }

    fn block(&mut self, block: &mut hp_vmt::Block) -> Result<(), String> {
        if block.name.eq_ignore_ascii_case("proxies") {
            return Ok(());
        }
        self.parameters(&mut block.props)?;
        for child in &mut block.children {
            self.block(child)?;
        }
        Ok(())
    }
}

pub fn stage(
    fs: &GameFs,
    geometry: &mut PropGeometry,
    folder: &str,
    generated: &[(String, Vec<u8>)],
) -> Result<Vec<(String, Vec<u8>)>, String> {
    Ok(stage_creation(fs, geometry, folder, generated, false)?.files)
}

pub struct MaterialSet {
    /// Compiler inputs, including original model materials at their original paths.
    pub files: Vec<(String, Vec<u8>)>,
    /// Only converted/copied materials are published; original assets stay untouched.
    pub publish: Vec<String>,
}

pub fn stage_creation(
    fs: &GameFs,
    geometry: &mut PropGeometry,
    folder: &str,
    generated: &[(String, Vec<u8>)],
    reuse_materials: bool,
) -> Result<MaterialSet, String> {
    hp_editor::prop_export::PropDirectives { cdmaterials: folder.into(), ..Default::default() }
        .validate()
        .map_err(str::to_owned)?;
    let folder = folder.trim_matches('/');
    let mut export = Export {
        fs,
        generated: generated.iter().map(|(path, bytes)| (path.to_ascii_lowercase(), bytes.as_slice())).collect(),
        folder,
        textures: HashMap::new(),
        files: BTreeMap::new(),
        publish: BTreeSet::new(),
        preserve_paths: false,
    };
    let mut materials: Vec<String> = geometry.tris.iter().map(|tri| tri.material.clone()).collect();
    materials.sort();
    materials.dedup();
    let mut names = HashMap::new();
    for name in materials {
        let path = format!("materials/{name}.vmt");
        let mut material = match export.generated.get(&path.to_ascii_lowercase()) {
            Some(bytes) => hp_vmt::parse_bytes(bytes).map_err(|e| format!("{name}: {e}"))?,
            None => fs
                .material(&name)
                .map(|material| (*material).clone())
                .ok_or_else(|| format!("{name}: material not found"))?,
        };
        // Brush lighting comes from lightmaps. The exported prop instead receives
        // model lighting while retaining the authored material parameters.
        let shader = hp_vmt::shader_base_name(&material.shader);
        export.preserve_paths = reuse_materials
            && shader == "vertexlitgeneric"
            && !export.generated.contains_key(&path.to_ascii_lowercase())
            && fs.read(&path).is_some();
        if matches!(shader.as_str(), "lightmappedgeneric" | "worldvertextransition" | "lightmappedtwotexture") {
            material.shader = "VertexLitGeneric".to_owned();
        }
        export.parameters(&mut material.params)?;
        for block in &mut material.other_blocks {
            export.block(block)?;
        }
        let (model_name, output) = if export.preserve_paths {
            (name.clone(), path)
        } else {
            let stem = file_stem(&name, "m");
            let output = format!("materials/{}.vmt", qualified(folder, &stem));
            export.publish.insert(output.clone());
            (stem, output)
        };
        export.files.insert(output, hp_vmt::write(&material).into_bytes());
        names.insert(name, model_name);
    }
    // Copied names use $cdmaterials; reused names use the root search fallback.
    // Update only after every required material and texture has been prepared.
    for triangle in &mut geometry.tris {
        triangle.material = names[&triangle.material].clone();
    }
    Ok(MaterialSet { files: export.files.into_iter().collect(), publish: export.publish.into_iter().collect() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use hp_editor::prop_export::PropTri;

    fn triangle(material: &str) -> PropTri {
        PropTri {
            material: material.into(),
            pos: [DVec3::ZERO, DVec3::X, DVec3::Y],
            normal: [DVec3::Z; 3],
            uv: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            disp: None,
        }
    }

    fn geometry(materials: &[&str]) -> PropGeometry {
        PropGeometry {
            tris: materials.iter().map(|name| triangle(name)).collect(),
            disps: Vec::new(),
            pieces: Vec::new(),
            origin: DVec3::ZERO,
        }
    }

    fn vtf(rgba: [u8; 4]) -> Vec<u8> {
        let image = hp_vtf::RgbaImage::new(8, 8, rgba.repeat(64)).unwrap();
        hp_vtf::encode(&image, &hp_vtf::EncodeOptions::default()).unwrap()
    }

    #[test]
    fn all_materials_and_textures_are_relocated_without_changing_the_source_or_uvs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/propper/materials")
            .join(std::process::id().to_string());
        let write = |name: &str, bytes: &[u8]| {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        let red = vtf([255, 0, 0, 255]);
        let green = vtf([0, 255, 0, 255]);
        let normal = vtf([128, 128, 255, 255]);
        write("materials/a/color.vtf", &red);
        write("materials/b/color.vtf", &green);
        write("materials/a/normal.vtf", &normal);
        let source = br#"LightmappedGeneric {
            "$basetexture" "a/color" "$bumpmap" "a/normal"
            "$basetexturetransform" "center .5 .5 scale 4 4 rotate 30"
            "$surfaceprop" "wood" "$envmap" "env_cubemap"
            ">=DX90" { "$detail" "b/color" "$detailscale" "4" }
            Proxies { TextureScroll { textureScrollVar "$basetexturetransform" } }
        }"#;
        write("materials/a/wall.vmt", source);
        write("materials/b/wall.vmt", br#"UnlitGeneric { "$basetexture" "b/color" }"#);
        let fs = GameFs::empty();
        fs.add_game_dir(&root).unwrap();
        let mut geometry = geometry(&["a/wall", "b/wall"]);
        let before = geometry.clone();
        let files: BTreeMap<_, _> =
            stage(&fs, &mut geometry, "models/custom/props", &[]).unwrap().into_iter().collect();
        assert_eq!(files.len(), 5, "two VMTs and three distinct VTFs");
        assert_ne!(geometry.tris[0].material, geometry.tris[1].material, "same leaf filenames must not collide");
        assert!(files.keys().all(|path| path.starts_with("materials/models/custom/props/")));
        for (triangle, original) in geometry.tris.iter().zip(&before.tris) {
            assert!(!triangle.material.contains('/'));
            assert_eq!(triangle.pos, original.pos);
            assert_eq!(triangle.normal, original.normal);
            assert_eq!(triangle.uv, original.uv);
            let material =
                hp_vmt::parse_bytes(&files[&format!("materials/models/custom/props/{}.vmt", triangle.material)])
                    .unwrap();
            let texture = material.texture("$basetexture").unwrap();
            let expected = if original.material == "a/wall" { &red } else { &green };
            assert_eq!(&files[&format!("materials/{texture}.vtf")], expected);
            if original.material == "a/wall" {
                assert_eq!(material.shader, "VertexLitGeneric");
                assert_eq!(material.get("$surfaceprop"), Some("wood"));
                assert_eq!(material.get("$envmap"), Some("env_cubemap"));
                assert_eq!(material.get("$basetexturetransform"), Some("center .5 .5 scale 4 4 rotate 30"));
                assert!(material.proxies.is_some());
            }
        }
        assert_eq!(std::fs::read(root.join("materials/a/wall.vmt")).unwrap(), source);
    }

    #[test]
    fn generated_blend_atlases_are_relocated_and_failures_leave_geometry_untouched() {
        let fs = GameFs::empty();
        let generated = vec![
            ("materials/baked/blend.vmt".into(), br#"VertexLitGeneric { "$basetexture" "baked/blend" }"#.to_vec()),
            ("materials/baked/blend.vtf".into(), vtf([120, 80, 40, 255])),
        ];
        let mut valid = geometry(&["baked/blend"]);
        let files = stage(&fs, &mut valid, "models/new_place", &generated).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|(path, _)| path.starts_with("materials/models/new_place/")));
        let mut missing = geometry(&["baked/blend", "missing/material"]);
        let before = missing.clone();
        assert!(stage(&fs, &mut missing, "models/new_place", &generated).is_err());
        assert_eq!(missing, before);
    }

    #[test]
    fn model_material_paths_are_reused_even_when_a_converted_material_shares_the_texture() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/propper/reuse")
            .join(std::process::id().to_string());
        let original = "models/hammerpro/arquiten_remake_vis_optim_95c9123f_visual/anderon_metal_prop_metal_prop_004";
        let source = br#"VertexLitGeneric { "$basetexture" "shared/metal" "$bumpmap" "shared/normal" }"#;
        let color = vtf([120, 90, 70, 255]);
        let normal = vtf([128, 128, 255, 255]);
        let write = |name: &str, bytes: &[u8]| {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        write(&format!("materials/{original}.vmt"), source);
        write("materials/shared/metal.vtf", &color);
        write("materials/shared/normal.vtf", &normal);
        write("materials/a_brush.vmt", br#"LightmappedGeneric { "$basetexture" "shared/metal" }"#);
        let fs = GameFs::empty();
        fs.add_game_dir(&root).unwrap();
        let mut geo = geometry(&["a_brush", original]);
        let staged = stage_creation(&fs, &mut geo, "models/new_export", &[], true).unwrap();
        assert_eq!(geo.tris[1].material, original, "an already valid model material must keep its complete path");
        assert!(!geo.tris[0].material.contains('/'));
        assert!(staged.publish.iter().all(|name| name.starts_with("materials/models/new_export/")));
        let files: BTreeMap<_, _> = staged.files.into_iter().collect();
        let preserved = hp_vmt::parse_bytes(&files[&format!("materials/{original}.vmt")]).unwrap();
        assert_eq!(preserved.texture("$basetexture").as_deref(), Some("shared/metal"));
        assert_eq!(preserved.texture("$bumpmap").as_deref(), Some("shared/normal"));
        assert_eq!(files["materials/shared/metal.vtf"], color);
        assert_eq!(files["materials/shared/normal.vtf"], normal);
        assert_eq!(std::fs::read(root.join(format!("materials/{original}.vmt"))).unwrap(), source);
        let mut copied = geometry(&[original]);
        let copied = stage_creation(&fs, &mut copied, "models/new_export", &[], false).unwrap();
        assert!(copied.files.iter().all(|(name, _)| name.starts_with("materials/models/new_export/")));
        assert_eq!(copied.publish.len(), 3, "explicit copy mode still exports the VMT and both VTF dependencies");
    }

    #[test]
    fn reused_materials_publish_new_vtfs_when_the_source_texture_is_an_editor_image() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/propper/reuse-image")
            .join(std::process::id().to_string());
        std::fs::create_dir_all(root.join("materials/models")).unwrap();
        std::fs::create_dir_all(root.join("images")).unwrap();
        let source = br#"VertexLitGeneric { "$basetexture" "pictures/color" }"#;
        std::fs::write(root.join("materials/models/original.vmt"), source).unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([120, 90, 70, 255]))
            .save(root.join("images/color.png"))
            .unwrap();
        let fs = GameFs::empty();
        fs.add_game_dir(&root).unwrap();
        fs.add_custom_dir(hp_fs::CustomDir::with_prefix(root.join("images"), "pictures")).unwrap();
        let mut geo = geometry(&["models/original"]);
        let staged = stage_creation(&fs, &mut geo, "models/copied", &[], true).unwrap();
        assert_eq!(geo.tris[0].material, "models/original");
        assert_eq!(staged.publish, ["materials/pictures/color.vtf"]);
        let files: BTreeMap<_, _> = staged.files.into_iter().collect();
        assert_eq!(hp_vtf::read_info(&files["materials/pictures/color.vtf"]).unwrap().width, 8);
        assert_eq!(std::fs::read(root.join("materials/models/original.vmt")).unwrap(), source);
        assert!(!root.join("materials/pictures/color.vtf").exists());
    }
}
