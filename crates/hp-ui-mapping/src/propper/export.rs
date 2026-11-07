use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hp_app::runtime::PropCreationRequest;
use hp_app::{t, tf};
use hp_editor::prop_export::PropDirectives;
use hp_fs::GameFs;

use super::{PreparedProp, bake, creation, prepare_material_files, prop_lods};

#[derive(Clone, Debug, PartialEq)]
pub struct ExportTarget {
    pub destination: PathBuf,
    pub source_serial: u64,
    pub directives: PropDirectives,
    files: Vec<String>,
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let failed = |error: std::io::Error| tf!("propper.write_failed", format!("{}: {error}", path.display()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    std::fs::write(path, bytes).map_err(failed)
}

fn destination(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty() || path.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(tf!("propper.write_failed", relative));
    }
    Ok(root.join(path))
}

/// Prepare exactly the geometry accepted in the creation preview. studiomdl writes
/// only into this private build tree; the selected export folder is filled on success.
pub fn prepare_creation(fs: &GameFs, request: PropCreationRequest) -> Result<PreparedProp, String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let model = creation::model_path(&request.model).ok_or_else(|| t!("propper.invalid_model_path").to_owned())?;
    request.directives.validate().map_err(|error| tf!("propper.write_failed", error))?;
    if !request.source_dir.is_absolute() || !request.destination.is_absolute() {
        return Err(t!("propper.absolute_directory").into());
    }
    let options = request.options.clamped();
    if options.collision && request.geometry.pieces.is_empty() {
        return Err(t!("propper.physics_empty").into());
    }
    let mut geometry = request.geometry;
    let surface = if request.surface_prop.is_empty() {
        bake::dominant_surface_prop(fs, &geometry.tris).unwrap_or_else(|| "default".into())
    } else {
        request.surface_prop
    };
    if !fs.surface_properties().iter().any(|name| name.eq_ignore_ascii_case(&surface)) {
        return Err(t!("propper.invalid_surface").into());
    }
    let baked = bake::bake_blends(fs, &mut geometry, &format!("models/{model}"), &surface);
    let generated = baked.as_ref().map_or(&[][..], |baked| baked.files.as_slice());
    let materials = prepare_material_files(fs, &mut geometry, &request.directives.cdmaterials, generated)?;
    let lods = prop_lods(&geometry.tris, request.lod);
    let export = geometry.files_with_directives(&model, &surface, &lods, options, &request.directives);
    let build = hp_app::settings::Settings::cache_dir().join("prop_builds").join(format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let game = build.join("game");
    let source = build.join("source");
    let mut paths = String::from("Game \"|gameinfo_path|.\"\n");
    if let Some(base) = fs.game_dir() {
        paths.push_str(&format!("Game \"{}\"\n", base.to_string_lossy().replace('\\', "/")));
        if let Some(parent) = base.parent() {
            paths.push_str(&format!("Game \"{}/hl2\"\n", parent.to_string_lossy().replace('\\', "/")));
        }
    }
    write(&game.join("gameinfo.txt"), format!("\"GameInfo\" {{ game \"Hammer Pro prop build\" FileSystem {{ SteamAppId 4000 SearchPaths {{ {paths} }} }} }}").as_bytes())?;
    let mut files = Vec::new();
    for (path, bytes) in materials {
        write(&destination(&game, &path)?, &bytes)?;
        files.push(path);
    }
    for (path, bytes) in fs.surface_property_files() {
        write(&destination(&game, &path)?, &bytes)?;
    }
    let stem = model.rsplit('/').next().unwrap();
    let mut sources = vec![(format!("{stem}.qc"), export.qc), (format!("{stem}_ref.smd"), export.smd)];
    if !export.phys_smd.is_empty() {
        sources.push((format!("{stem}_phys.smd"), export.phys_smd));
    }
    for (index, smd) in export.lod_smds.into_iter().enumerate() {
        sources.push((format!("{stem}_lod{}.smd", index + 1), smd));
    }
    for (name, bytes) in sources {
        write(&source.join(&name), bytes.as_bytes())?;
        write(&request.source_dir.join(name), bytes.as_bytes())?;
    }
    let mut lod_triangles = vec![export.triangles];
    lod_triangles.extend(lods.iter().map(|lod| lod.tris.len()));
    Ok(PreparedProp {
        qc: source.join(format!("{stem}.qc")),
        model,
        origin: export.origin,
        lod_triangles,
        pieces: export.pieces,
        surface_prop: surface,
        baked: baked.map(|baked| (baked.faces, baked.size, baked.bump)),
        options,
        game_dir: game,
        target: Some(ExportTarget {
            destination: request.destination,
            source_serial: request.source_serial,
            directives: request.directives,
            files,
        }),
    })
}

impl PreparedProp {
    /// Called on a worker after successful compilation, before replacing any brushes.
    pub fn finish_creation(&mut self) -> Result<(), String> {
        let Some(target) = &self.target else { return Ok(()) };
        let base = format!("models/{}", self.model);
        let path = self.game_dir.join(format!("{base}.mdl"));
        let bytes = std::fs::read(&path).map_err(|error| tf!("propper.write_failed", error))?;
        let header = hp_mdl::read_header(&bytes).map_err(|error| tf!("propper.write_failed", error))?;
        if header.is_static_prop() != self.options.static_prop {
            return Err(t!("propper.compiled_settings_mismatch").into());
        }
        let close = |a: [f32; 3], b: glam::DVec3| {
            a.into_iter().zip(b.to_array()).all(|(a, b)| (f64::from(a) - b).abs() <= 1e-4 * b.abs().max(1.0))
        };
        if target.directives.illumposition.is_some_and(|point| !close(header.illum_position, point))
            || target
                .directives
                .bbox
                .is_some_and(|bounds| !close(header.hull_min, bounds.min) || !close(header.hull_max, bounds.max))
        {
            return Err(t!("propper.compiled_settings_mismatch").into());
        }
        let mut files = target.files.clone();
        for extension in [".mdl", ".vvd", ".dx90.vtx", ".dx80.vtx", ".sw.vtx", ".vtx", ".phy", ".ani"] {
            let name = format!("{base}{extension}");
            if self.game_dir.join(&name).is_file() {
                files.push(name);
            }
        }
        // Read back the complete model before publishing its files to the chosen folder.
        let model_name = format!("{base}.mdl");
        hp_mdl::load_from(
            &model_name,
            |name| std::fs::read(self.game_dir.join(name)).ok(),
            &hp_mdl::LoadOptions::default(),
        )
        .map_err(|error| tf!("propper.write_failed", error))?;
        if self.options.collision && !self.game_dir.join(format!("{base}.phy")).is_file() {
            return Err(t!("propper.compiled_collision_missing").into());
        }
        for name in &files {
            std::fs::metadata(destination(&self.game_dir, name)?)
                .map_err(|error| tf!("propper.write_failed", error))?;
        }
        // Stream large VTFs instead of collecting the whole material set in RAM.
        // Publish the MDL after its vertex data and materials are in place.
        files.sort_by_key(|name| name.ends_with(".mdl"));
        for name in &files {
            let source = destination(&self.game_dir, name)?;
            let output = destination(&target.destination, name)?;
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).map_err(|error| tf!("propper.write_failed", error))?;
            }
            let temporary = output.with_file_name(format!(
                ".{}.hp-prop-{}.tmp",
                output.file_name().unwrap().to_string_lossy(),
                std::process::id()
            ));
            std::fs::copy(source, &temporary).map_err(|error| tf!("propper.write_failed", error))?;
            std::fs::rename(temporary, output).map_err(|error| tf!("propper.write_failed", error))?;
        }
        if !self.options.collision {
            let old = destination(&target.destination, &format!("{base}.phy"))?;
            if let Err(error) = std::fs::remove_file(old)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(tf!("propper.write_failed", error));
            }
        }
        self.game_dir = target.destination.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{DVec3, Vec3};
    use hp_editor::prop_export::{EditableCollision, PropGeometry, PropOptions, PropTri};
    use hp_geom::Aabb;

    /// Run explicitly with HP_TEST_STUDIOMDL pointing to an installed Source compiler.
    /// Every output stays in this test's own tree or the private prop-build cache.
    #[test]
    #[ignore = "requires an installed native studiomdl; set HP_TEST_STUDIOMDL"]
    fn native_creation_exports_the_accepted_settings_and_self_contained_materials() {
        let exe = PathBuf::from(std::env::var_os("HP_TEST_STUDIOMDL").expect("HP_TEST_STUDIOMDL"));
        assert!(exe.is_file());
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/propper/native").join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let mounted = root.join("mounted");
        let destination = root.join("export");
        let source = root.join("source");
        let texture = hp_vtf::encode(
            &hp_vtf::RgbaImage::new(8, 8, [210, 90, 20, 255].repeat(64)).unwrap(),
            &hp_vtf::EncodeOptions::default(),
        )
        .unwrap();
        write(&mounted.join("materials/fixture/color.vtf"), &texture).unwrap();
        let original_vmt = br#"LightmappedGeneric { "$basetexture" "fixture/color" "$surfaceprop" "fixture_foam" }"#;
        write(&mounted.join("materials/fixture/wall.vmt"), original_vmt).unwrap();
        write(
            &mounted.join("scripts/surfaceproperties_manifest.txt"),
            b"surfaceproperties_manifest { file scripts/surfaceproperties.txt }",
        )
        .unwrap();
        write(
            &mounted.join("scripts/surfaceproperties.txt"),
            b"default { density 2000 friction 0.8 elasticity 0.25 } fixture_foam { base default density 100 }",
        )
        .unwrap();
        let fs = GameFs::empty();
        fs.add_game_dir(&mounted).unwrap();
        let bounds = Aabb::new(DVec3::new(-12.0, -24.0, -8.0), DVec3::new(16.0, 32.0, 48.0));
        let mut collision = EditableCollision::box_at(bounds).unwrap();
        collision.set_point(7, DVec3::new(20.0, 36.0, 52.0)).unwrap();
        let geometry = PropGeometry {
            tris: collision
                .triangles()
                .iter()
                .map(|&pos| PropTri {
                    material: "fixture/wall".into(),
                    pos,
                    normal: [(pos[1] - pos[0]).cross(pos[2] - pos[0]).normalize(); 3],
                    uv: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                    disp: None,
                })
                .collect(),
            disps: Vec::new(),
            pieces: vec![collision.triangles().to_vec()],
            origin: DVec3::new(0.25, 1.5, 0.75),
        };
        for (index, options) in [
            PropOptions::default(),
            PropOptions { static_prop: false, collision: false, ..Default::default() },
            PropOptions { physics: true, ..Default::default() },
        ]
        .into_iter()
        .enumerate()
        {
            let request = PropCreationRequest {
                source_serial: 47,
                solid_ids: vec![12],
                geometry: geometry.clone(),
                options,
                directives: PropDirectives {
                    illumposition: Some(DVec3::new(1.25, -2.5, 30.0)),
                    bbox: Some(bounds),
                    cdmaterials: "models/chosen/materials".into(),
                },
                model: "models/chosen/subfolder/native.mdl".into(),
                source_dir: source.clone(),
                destination: destination.clone(),
                surface_prop: "fixture_foam".into(),
                lod: hp_app::settings::ModelLodSettings { enabled: false, ..Default::default() },
            };
            let mut prepared = prepare_creation(&fs, request).unwrap();
            assert_eq!(prepared.target.as_ref().unwrap().source_serial, 47);
            assert_ne!(prepared.game_dir, destination);
            if index == 0 {
                assert!(!destination.exists());
            }
            assert_eq!(prepared.origin, geometry.origin);
            let log_path = root.join(format!("studiomdl-{index}.log"));
            let log = std::fs::File::create(&log_path).unwrap();
            let mut command = std::process::Command::new(&exe);
            command
                .args(["-nop4", "-game"])
                .arg(&prepared.game_dir)
                .arg(&prepared.qc)
                .current_dir(prepared.qc.parent().unwrap())
                .stdout(log.try_clone().unwrap())
                .stderr(log);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            let mut child = command.spawn().unwrap();
            let start = std::time::Instant::now();
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if start.elapsed().as_secs() > 30 {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("studiomdl timeout: {}", log_path.display());
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            };
            assert!(status.success(), "{}\n{}", log_path.display(), std::fs::read_to_string(&log_path).unwrap());
            prepared.finish_creation().unwrap();
            assert_eq!(prepared.game_dir, destination);
            assert_eq!(destination.join("models/chosen/subfolder/native.phy").is_file(), options.clamped().collision);
            let bytes = std::fs::read(destination.join("models/chosen/subfolder/native.mdl")).unwrap();
            let header = hp_mdl::read_header(&bytes).unwrap();
            assert_eq!(header.is_static_prop(), options.clamped().static_prop);
            assert_eq!(header.illum_position, [1.25, -2.5, 30.0]);
            assert_eq!(header.hull_min, bounds.min.as_vec3().to_array());
            assert_eq!(header.hull_max, bounds.max.as_vec3().to_array());
            let model = hp_mdl::load_from(
                "models/chosen/subfolder/native.mdl",
                |name| std::fs::read(destination.join(name)).ok(),
                &Default::default(),
            )
            .unwrap();
            assert_eq!(model.surface_prop, "fixture_foam");
            assert!(
                model
                    .material_dirs
                    .iter()
                    .any(|path| path.replace('\\', "/").trim_matches('/') == "models/chosen/materials")
            );
            for vertex in model.meshes.iter().flat_map(|mesh| &mesh.vertices) {
                assert!(
                    geometry
                        .tris
                        .iter()
                        .flat_map(|tri| tri.pos)
                        .any(|point| point.distance(Vec3::from(vertex.pos).as_dvec3()) < 0.001)
                );
            }
            if options.clamped().collision {
                let phy = std::fs::read(destination.join("models/chosen/subfolder/native.phy")).unwrap();
                let collision = hp_mdl::parse_phy_for_model(&phy, &bytes, Default::default()).unwrap();
                let (min, max) = collision.bounds().unwrap();
                assert!(Vec3::from(min).as_dvec3().distance(bounds.min) < 0.01, "{min:?}");
                assert!(Vec3::from(max).as_dvec3().distance(DVec3::new(20.0, 36.0, 52.0)) < 0.01, "{max:?}");
            }
            let material_dir = destination.join("materials/models/chosen/materials");
            let files: Vec<_> = std::fs::read_dir(material_dir).unwrap().map(|entry| entry.unwrap().path()).collect();
            assert_eq!(files.len(), 2);
            let vtf = files.iter().find(|path| path.extension().is_some_and(|ext| ext == "vtf")).unwrap();
            assert_eq!(std::fs::read(vtf).unwrap(), texture);
            assert_eq!(std::fs::read(mounted.join("materials/fixture/wall.vmt")).unwrap(), original_vmt);
            assert!(source.join("native.qc").is_file());
            eprintln!("native mode {index} passed: {}", destination.display());
        }
    }
}
