use hp_render::*;
use std::path::Path;
use std::sync::OnceLock;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    static GPU: OnceLock<Option<(wgpu::Device, wgpu::Queue)>> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() }),
        )
        .ok()
    })
    .clone()
}

#[test]
fn entity_sprites_match_native_source_size_color_and_render_modes() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    r.set_tone_settings(ToneSettings { mode: ToneMode::Off, ..Default::default() });
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/native_world_sprites");
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("sprite-cases.json")).unwrap()).unwrap();
    let mut failures = Vec::new();
    for c in cases.as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let legacy = c["legacy"].as_bool().unwrap_or(false);
        let generic = c["material"].as_str().unwrap_or("").starts_with("unlit");
        let no_vertex = c["material"].as_str() == Some("unlit_novertex");
        let texture = r.create_texture_exact(&TextureUpload {
            width: 64,
            height: 32,
            format: if legacy { TexFormat::Rgba8Linear } else { TexFormat::Rgba8Srgb },
            levels: vec![[128, 192, 64, 128].repeat(64 * 32)],
        });
        r.set_material(
            1,
            &MaterialDesc {
                texture: Some(texture),
                tex_size: [64.0, 32.0],
                flags: MaterialFlags::UNLIT,
                ..Default::default()
            },
        );
        let scale = c["scale"].as_f64().unwrap_or(1.0) as f32;
        r.set_world_sprites(&[WorldSprite {
            object: 1,
            pos: [0.0, 0.0, 64.0],
            size: [64.0 * scale, 32.0 * scale],
            origin: c["origin"].as_array().map_or(if generic { [0.5; 2] } else { [0.0; 2] }, |v| {
                [v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32]
            }),
            srgb_color: !legacy,
            vertex_color: !no_vertex,
            vertex_alpha: !no_vertex,
            material_mode: generic.then_some(if c["material"].as_str() == Some("unlit_add") { 5 } else { 2 }),
            material: 1,
            color: [1.0, 128.0 / 255.0, 64.0 / 255.0, c["amount"].as_f64().unwrap_or(255.0) as f32 / 255.0],
            mode: c["mode"].as_u64().unwrap() as u32,
            render_fx: c["fx"].as_u64().unwrap_or(0) as u32,
            orientation: 2,
            angles: [0.0, 0.0, c["roll"].as_f64().unwrap_or(0.0) as f32],
            hdr_scale: 1.0,
            glow_proxy: 2.0,
        }]);
        let view = ViewParams {
            size: [128; 2],
            game_pixel_grid: true,
            kind: ViewKind::Perspective {
                eye: [0.0, -c["distance"].as_f64().unwrap() as f32, 64.0],
                forward: [0.0, 1.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 90.0,
                near: 1.0,
                far: 10000.0,
            },
            mode: RenderMode::Textured,
            grid: None,
            background: if c["bg"].as_bool().unwrap_or(false) {
                [32.0f32, 64.0, 128.0].map(|v| {
                    let v = v / 255.0;
                    if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
                })
            } else {
                [0.0; 3]
            },
            selection_color: [0.0; 4],
            show_models: false,
            show_sprites: true,
            msaa: false,
        };
        r.render_view(118, &view, &Overlay::default());
        let (_, _, actual) = r.read_view_rgba8(118).unwrap();
        let expected = image::open(root.join(format!("{name}.png"))).unwrap().into_rgba8();
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("world_sprites");
        std::fs::create_dir_all(&dir).unwrap();
        image::save_buffer(dir.join(format!("{name}.png")), &actual, 128, 128, image::ColorType::Rgba8).unwrap();
        let maximum = actual
            .chunks_exact(4)
            .zip(expected.pixels())
            .flat_map(|(a, b)| (0..3).map(move |k| a[k].abs_diff(b[k])))
            .max()
            .unwrap();
        if maximum > 2 {
            failures.push(format!("{name}: native RGB error {maximum}"));
        }
        if name == "glow_d200" {
            for flags in [ObjectFlags::HIDDEN, ObjectFlags::NO_3D] {
                r.set_object_flags(1, flags);
                r.render_view(118, &view, &Overlay::default());
                let (_, _, hidden) = r.read_view_rgba8(118).unwrap();
                assert!(hidden.chunks_exact(4).all(|p| p[..3] == [0; 3]));
            }
            r.set_object_flags(1, ObjectFlags::default());
            let parked = r.park_scene();
            r.set_world_sprites(&[]);
            r.render_view(118, &view, &Overlay::default());
            let (_, _, empty) = r.read_view_rgba8(118).unwrap();
            assert!(empty.chunks_exact(4).all(|p| p[..3] == [0; 3]));
            r.restore_scene(parked).drop_in_background();
            r.render_view(118, &view, &Overlay::default());
            let (_, _, restored) = r.read_view_rgba8(118).unwrap();
            image::save_buffer(dir.join("restored.png"), &restored, 128, 128, image::ColorType::Rgba8).unwrap();
            let differences = restored.chunks_exact(4).zip(actual.chunks_exact(4)).filter(|(a, b)| a != b).count();
            let maximum = restored.iter().zip(&actual).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            assert!(
                restored == actual,
                "restoring a map restores its own entity sprites: {differences} different pixels, maximum {maximum}, center {:?}, expected {:?}",
                &restored[(64 * 128 + 64) * 4..(64 * 128 + 64) * 4 + 4],
                &actual[(64 * 128 + 64) * 4..(64 * 128 + 64) * 4 + 4]
            );
        }
        r.destroy_texture(texture);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn glow_uses_origin_visibility_while_alpha_sprites_follow_scene_depth() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    r.set_tone_settings(ToneSettings { mode: ToneMode::Off, ..Default::default() });
    r.set_material(1, &MaterialDesc { texture: Some(TexId::WHITE), flags: MaterialFlags::UNLIT, ..Default::default() });
    r.set_material(
        2,
        &MaterialDesc {
            texture: Some(TexId::WHITE),
            flags: MaterialFlags::UNLIT | MaterialFlags::TWO_SIDED,
            tint: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    );
    let sprite = WorldSprite {
        vertex_color: true,
        vertex_alpha: true,
        material_mode: None,
        object: 10,
        pos: [0.0, 0.0, 64.0],
        size: [128.0; 2],
        color: [0.5, 0.25, 0.1, 1.0],
        material: 1,
        mode: 3,
        render_fx: 0,
        orientation: 2,
        origin: [0.5; 2],
        srgb_color: false,
        angles: [0.0; 3],
        hdr_scale: 1.0,
        glow_proxy: 2.0,
    };
    let mut view = ViewParams {
        size: [128; 2],
        game_pixel_grid: true,
        kind: ViewKind::Perspective {
            eye: [0.0, -200.0, 64.0],
            forward: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 90.0,
            near: 1.0,
            far: 10000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.0; 3],
        selection_color: [0.0; 4],
        show_models: false,
        show_sprites: true,
        msaa: false,
    };
    let blocker = |right: f32| ObjectMesh {
        batches: vec![MeshBatch {
            material: 2,
            vertices: [[-50.0, -100.0, 40.0], [right, -100.0, 40.0], [right, -100.0, 90.0], [-50.0, -100.0, 90.0]]
                .map(|pos| Vertex { pos, normal: [0.0, -1.0, 0.0], uv: [0.0; 2], alpha: 1.0 })
                .to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }],
        aabb_min: [-50.0, -100.0, 40.0],
        aabb_max: [right, -100.0, 90.0],
        ..Default::default()
    };
    for msaa in [false, true] {
        view.msaa = msaa;
        r.upsert_object(20, blocker(-4.0));
        for (mode, left_visible) in [(3, true), (2, false)] {
            r.set_world_sprites(&[WorldSprite { mode, ..sprite }]);
            r.render_view(118, &view, &Overlay::default());
            let (_, _, pixels) = r.read_view_rgba8(118).unwrap();
            let at = |x: usize| pixels[(64 * 128 + x) * 4];
            assert_eq!(at(54) > 20, left_visible, "left halo mode{mode} msaa{msaa}: {}", at(54));
            assert!(at(74) > 20);
        }
        r.upsert_object(20, blocker(4.0));
        r.set_world_sprites(&[sprite]);
        r.render_view(118, &view, &Overlay::default());
        let (_, _, pixels) = r.read_view_rgba8(118).unwrap();
        assert!(
            pixels.chunks_exact(4).all(|p| p[..3] == [0; 3]),
            "blocked glow origin hides the whole halo, msaa{msaa}"
        );
    }
}
