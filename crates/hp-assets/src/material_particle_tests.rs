use hp_render::*;
use std::collections::{HashMap, hash_map::Entry};

#[test]
fn native_particle_colors_survive_material_loading_and_texture_preparation() {
    let Some((device, queue)) = crate::test_gpu::gpu() else {
        eprintln!("no GPU adapter: native particle material reference skipped");
        return;
    };
    let mut renderer = Renderer::new(&device, &queue);
    renderer.set_tone_settings(ToneSettings { bloom: 0.0, ..Default::default() });
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let render_data = data.join("../../../hp-render/tests/data");
    let fs = hp_fs::GameFs::from_source_mounts(
        None,
        None,
        &[
            hp_fs::SourceMount {
                kind: hp_fs::SourceMountKind::Gma,
                label: "Owned native sprite color reference".into(),
                path: data.join("native_card_color.gma"),
                enabled: true,
                origin: hp_fs::SourceOrigin::Custom,
            },
            hp_fs::SourceMount {
                kind: hp_fs::SourceMountKind::Gma,
                label: "Owned native trail color reference".into(),
                path: render_data.join("native_trail_color.gma"),
                enabled: true,
                origin: hp_fs::SourceOrigin::Custom,
            },
        ],
    )
    .unwrap();
    let sprites: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/native_card_color.json")).unwrap();
    let trails: serde_json::Value =
        serde_json::from_str(include_str!("../../hp-render/tests/data/native_trail_color.json")).unwrap();
    let tints: serde_json::Value = serde_json::from_str(include_str!("../tests/data/native_card_tints.json")).unwrap();
    let mut textures = HashMap::new();
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut largest_delta = 0;
    let mut full_images = 0;
    let proof = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/particle-proof");
    std::fs::create_dir_all(&proof).unwrap();
    for (trail, report) in [(false, &sprites), (true, &trails), (false, &tints)] {
        for case in report["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let prefix = if trail { "hp_trail117" } else { "hp_cardcolor117" };
            let kind = name.split('_').next().unwrap();
            let material_name = format!("{prefix}/{kind}");
            let mut material = (*fs.material(&material_name).unwrap()).clone();
            // The native probe changes only the base texture between captures.
            let texture_name = format!("{prefix}/{}", name.split('-').nth(1).unwrap());
            material.set("$basetexture", &texture_name);
            if let Some(properties) = case["properties"].as_object() {
                for (key, value) in properties {
                    material.set(key, value.as_str().unwrap());
                }
            }
            let analysis = crate::material::analyze(&material_name, &material, &|t| fs.texture_flags(t).unwrap_or(0));
            let blend = match case["blend"].as_str().unwrap() {
                "additive" => ParticleBlend::Additive,
                "addself" => ParticleBlend::AddSelf,
                _ => ParticleBlend::Alpha,
            };
            let material_kind = if blend == ParticleBlend::Additive {
                crate::material::MatKind::SpriteAdditive
            } else {
                crate::material::MatKind::Sprite
            };
            for key in crate::material::texture_keys(material_kind, &analysis) {
                if let Entry::Vacant(entry) = textures.entry(key) {
                    let prepared = crate::texture::prepare_texture(&fs, entry.key(), false).unwrap();
                    entry.insert((renderer.create_texture(&prepared.upload), prepared.size));
                }
            }
            let lookup = |key: &str| {
                textures
                    .get(key)
                    .map_or(crate::material::TexLook::Missing, |(id, size)| crate::material::TexLook::Ready(*id, *size))
            };
            let desc = crate::material::asset_desc(
                material_kind,
                &material_name,
                crate::material::AnaLook::Ready(&analysis),
                &lookup,
                true,
            );
            assert_ne!(desc.texture, Some(TexId::MISSING));
            assert_ne!(desc.texture, Some(TexId::LOADING));
            renderer.set_material(1, &desc);
            let array3 = |v: &serde_json::Value| std::array::from_fn(|i| v[i].as_f64().unwrap() as f32);
            renderer.set_particles(&[ParticleInstance {
                animation: ParticleAnimation::default(),
                pos: array3(&case["head"]),
                size: if trail {
                    case["width"].as_f64().unwrap() as f32
                } else {
                    case["radius"].as_f64().unwrap() as f32 * 2.0
                },
                axis: if trail { array3(&case["delta"]) } else { [0.0; 3] },
                roll: 0.0,
                yaw: 0.0,
                uv: [0.0, 0.0, 1.0, 1.0],
                color: std::array::from_fn(|i| case["color"][i].as_f64().unwrap() as f32 / 255.0),
                bright: 1.0,
                add_self: case["addself"].as_f64().unwrap() as f32,
                screen_fade: [0.0; 2],
                refraction_blur: 0.0,
                no_fog: true,
                orient: if !trail {
                    ParticleOrient::Screen
                } else if case["path"] == "cpu" {
                    ParticleOrient::TrailCpu
                } else {
                    ParticleOrient::TrailSpline
                },
                material: 1,
                blend,
            }]);
            let view = ViewParams {
                game_pixel_grid: true,
                size: [128; 2],
                kind: ViewKind::Perspective {
                    eye: array3(&case["eye"]),
                    forward: [0.0, 1.0, 0.0],
                    up: [0.0, 0.0, 1.0],
                    fov_y_deg: 60.0,
                    near: 1.0,
                    far: 2000.0,
                },
                mode: RenderMode::Textured,
                grid: None,
                background: [0.0; 3],
                selection_color: [0.0; 4],
                show_sprites: false,
                show_models: false,
                msaa: false,
            };
            renderer.render_view(1, &view, &Overlay::default());
            let (_, _, pixels) = renderer.read_view_rgba8(1).unwrap();
            let mut worst = 0;
            for sample in case["samples"].as_array().unwrap() {
                let x = sample["x"].as_u64().unwrap() as usize;
                for channel in 0..3 {
                    let target = sample["pixel"][channel].as_u64().unwrap() as u8;
                    worst = worst.max(pixels[(64 * 128 + x) * 4 + channel].abs_diff(target));
                }
            }
            if worst > 3 {
                failures.push(format!("{} {name}: max RGB delta {worst}", if trail { "trail" } else { "sprite" }));
            }
            largest_delta = largest_delta.max(worst);
            if matches!(
                name,
                "spline_default-white"
                    | "splineaddself_default-rgba128"
                    | "splineadd_default-dark64"
                    | "cpu_white-white-shadow_tint"
                    | "spline_default-white-combined"
            ) {
                let prefix = if trail { "trail" } else { "sprite" };
                image::save_buffer(
                    proof.join(format!("{prefix}_{name}.png")),
                    &pixels,
                    128,
                    128,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                let reference =
                    image::open(data.join(format!("native_particle_{prefix}_{name}.png"))).unwrap().to_rgb8();
                assert_eq!(reference.dimensions(), (128, 128));
                let mut full_delta = 0;
                for (actual, expected) in pixels.chunks_exact(4).zip(reference.pixels()) {
                    for (actual, expected) in actual[..3].iter().zip(expected.0) {
                        full_delta = full_delta.max(actual.abs_diff(expected));
                    }
                }
                assert!(full_delta <= 3, "{prefix} {name}: whole-image RGB delta {full_delta}");
                largest_delta = largest_delta.max(full_delta);
                full_images += 1;
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 568);
    assert_eq!(full_images, 8);
    assert!(failures.is_empty(), "{} native particle material cases failed:\n{}", failures.len(), failures.join("\n"));
    eprintln!(
        "Native material-to-pixel particle reference: {checked} cases and {full_images} whole images, maximum RGB delta {largest_delta}/255"
    );
}
