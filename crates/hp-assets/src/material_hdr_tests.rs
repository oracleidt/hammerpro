use hp_render::*;

#[test]
fn native_hdr_cubemap_colors_survive_loading_and_shader_tint() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("no GPU adapter: native HDR material test skipped");
        return;
    };
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() }),
    )
    .unwrap();
    let mut renderer = Renderer::new(&device, &queue);
    renderer.set_tone_settings(ToneSettings { bloom: 0.0, ..Default::default() });
    renderer.set_view_options(1, ViewOptions { cubemaps: true, ..Default::default() });
    let black = renderer.create_texture(&TextureUpload {
        width: 1,
        height: 1,
        format: TexFormat::Rgba8Srgb,
        levels: vec![vec![0, 0, 0, 255]],
    });
    let flat = renderer.create_texture(&TextureUpload {
        width: 1,
        height: 1,
        format: TexFormat::Rgba8Linear,
        levels: vec![vec![128, 128, 255, 255]],
    });
    let vertices: Vec<Vertex> = [[-64.0, -64.0], [64.0, -64.0], [64.0, 64.0], [-64.0, 64.0]]
        .map(|p| Vertex { pos: [0.0, p[0], p[1]], normal: [-1.0, 0.0, 0.0], uv: [0.5, 0.5], alpha: 1.0 })
        .to_vec();
    renderer.upsert_object(
        1,
        ObjectMesh {
            batches: vec![MeshBatch {
                material: 1,
                vertices: vertices.clone(),
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            aabb_min: [0.0, -64.0, -64.0],
            aabb_max: [0.0, 64.0, 64.0],
            ..Default::default()
        },
    );
    let generation = renderer.lightmap_generation(1);
    renderer.add_lightmap_faces(&[(0, lightmap::LightmapFace { object: 1, generation, scale: 16.0, flags: 0 })]);
    let baked: Vec<lightmap::LightmapVertex> = vertices
        .iter()
        .map(|v| lightmap::LightmapVertex { pos: v.pos, uv: v.uv, lm: [0.0; 2], alpha: 1.0, face: 0, blend: 0 })
        .collect();
    renderer.add_lightmap_block(
        1,
        &[lightmap::LightmapBatch { material: 1, vertices: &baked, indices: &[0, 1, 2, 0, 2, 3] }],
    );
    assert!(renderer.ensure_lightmap_layers(64, 1));
    renderer.write_lightmap(
        0,
        lightmap::LightmapRect { layer: 0, x: 2, y: 3, w: 1, h: 1, density: 1.0, bumped: false },
        &[0],
    );
    let compiled: Vec<CompiledVertex> = vertices
        .iter()
        .map(|v| CompiledVertex {
            pos: v.pos,
            normal: v.normal,
            uv: v.uv,
            alpha: 1.0,
            lm: [0.5; 2],
            lightmap: 0,
            cubemap: NO_CUBEMAP,
            blend: Default::default(),
        })
        .collect();
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let fs = hp_fs::GameFs::from_source_mounts(
        None,
        None,
        &[hp_fs::SourceMount {
            kind: hp_fs::SourceMountKind::Gma,
            label: "Owned native HDR reference".into(),
            path: data.join("native_hdr_cube_assets.gma"),
            enabled: true,
            origin: hp_fs::SourceOrigin::Custom,
        }],
    )
    .unwrap();
    let report: serde_json::Value = serde_json::from_str(include_str!("../tests/data/native_hdr_cube.json")).unwrap();
    assert!(report["errors"].as_array().unwrap().is_empty());
    let view = ViewParams {
        game_pixel_grid: false,
        size: [128, 128],
        kind: ViewKind::Perspective {
            eye: [-256.0, 0.0, 0.0],
            forward: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 40.0,
            near: 1.0,
            far: 1000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.0; 3],
        selection_color: [0.0; 4],
        show_sprites: false,
        show_models: true,
        msaa: false,
    };
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in report["cases"].as_array().unwrap() {
        if case["shader"] == "WorldTwoTextureBlend" {
            let src = r#"WorldTwoTextureBlend { "$basetexture" "b" "$detail" "d" "$envmap" "hp_hdrcube117/half" }"#;
            assert!(crate::envmap::analyze_envmap(&hp_vmt::parse(src).unwrap()).is_none());
            continue;
        }
        assert_eq!(case["env"]["mounted"], true);
        assert_eq!(case["env"]["width"], 4);
        let properties = case["properties"].as_object().unwrap();
        let body: String = properties.iter().map(|(k, v)| format!("\"{k}\" \"{}\"\n", v.as_str().unwrap())).collect();
        let material = hp_vmt::parse(&format!("{} {{ {body} }}", case["shader"].as_str().unwrap())).unwrap();
        let env = crate::envmap::analyze_envmap(&material).unwrap();
        let key = crate::envmap::cube_key(env.texture.as_deref().unwrap());
        let prepared = crate::texture::prepare_texture(&fs, &key, false).unwrap();
        let texture = renderer.create_texture(&prepared.upload);
        let lookup =
            |key: &str| crate::material::TexLook::Ready(if key.ends_with("|cube") { texture } else { flat }, [4, 24]);
        renderer.set_material(
            1,
            &MaterialDesc {
                texture: Some(black),
                tex_size: [1.0; 2],
                flags: MaterialFlags::TWO_SIDED,
                envmap: env.desc(&lookup, true),
                ..Default::default()
            },
        );
        renderer.set_lightmap_display(lightmap::LightmapDisplay::Off, 1.0);
        renderer.set_object_flags(1, ObjectFlags::empty());
        renderer.set_view_options(1, ViewOptions { cubemaps: true, ..Default::default() });
        renderer.render_view(1, &view, &Overlay::default());
        renderer.read_view_rgba8(1).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while renderer.step_cubemaps(std::time::Duration::from_millis(50)) > 0 {
            assert!(std::time::Instant::now() < deadline, "cube upload did not finish");
        }
        for mode in 0..5 {
            renderer.set_object_flags(1, if mode >= 3 { ObjectFlags::COMPILED_LIGHT } else { ObjectFlags::empty() });
            renderer.set_lightmap_display(
                if matches!(mode, 1 | 2) { lightmap::LightmapDisplay::Baked } else { lightmap::LightmapDisplay::Off },
                1.0,
            );
            renderer.set_lightmap_ldr(mode == 2);
            if mode >= 3 {
                renderer.set_compiled_lighting(
                    if mode == 3 { CompiledLightMode::Hdr } else { CompiledLightMode::Ldr },
                    &[CompiledLightmap { width: 1, height: 1, texels: &[[0.0; 3]], styles: &[], bumped: false }],
                    &[CompiledBlock {
                        object: 1,
                        batches: vec![CompiledBatch {
                            material: 1,
                            vertices: compiled.clone(),
                            indices: vec![0, 1, 2, 0, 2, 3],
                        }],
                    }],
                );
            }
            renderer.set_view_options(
                1,
                ViewOptions { cubemaps: true, compiled_lighting: mode >= 3, ..Default::default() },
            );
            renderer.render_view(1, &view, &Overlay::default());
            let (_, _, pixels) = renderer.read_view_rgba8(1).unwrap();
            let at = (64 * 128 + 64) * 4;
            let actual = &pixels[at..at + 3];
            let expected = case["variants"]["identity"][0].as_array().unwrap();
            if actual.iter().zip(expected).any(|(a, b)| u64::from(*a).abs_diff(b.as_u64().unwrap()) > 3) {
                failures.push(format!("{} mode{mode}: actual {actual:?}, native {expected:?}", case["name"]));
            }
            if mode == 0
                && matches!(
                    case["name"].as_str(),
                    Some("LightmappedGeneric_half_1" | "VertexLitGeneric_half_1" | "Phong_half_1")
                )
            {
                let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/hdr-proof");
                std::fs::create_dir_all(&output).unwrap();
                image::save_buffer(
                    output.join(format!("{}.png", case["name"].as_str().unwrap())),
                    &pixels,
                    128,
                    128,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            checked += 1;
        }
        renderer.destroy_texture(texture);
    }
    assert_eq!(checked, 235);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
