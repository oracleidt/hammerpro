use hp_render::*;
use std::path::Path;

#[test]
fn vtf_mip_selection_matches_native_source_for_mixed_size_blends() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Some(adapter) = pollster::block_on(instance.request_adapter(&Default::default())).ok() else { return };
    let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let mut renderer = Renderer::new(&device, &queue);
    renderer.set_tone_settings(ToneSettings { mode: ToneMode::Off, ..Default::default() });
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/native_mips");
    let reference: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("reference.json")).unwrap()).unwrap();
    let view = ViewParams {
        size: [128; 2],
        game_pixel_grid: false,
        kind: ViewKind::Perspective {
            eye: [0.0, -128.0, 0.0],
            forward: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 90.0,
            near: 1.0,
            far: 1000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.0; 3],
        selection_color: [0.0; 4],
        show_models: false,
        show_sprites: false,
        msaa: false,
    };
    let linear = |v: f64| {
        let v = v / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let gamma = |v: f64| if v <= 0.0031308 { v * 12.92 * 255.0 } else { (1.055 * v.powf(1.0 / 2.4) - 0.055) * 255.0 };
    let mut comparisons = 0;
    for compressed in [false, true] {
        for name in
            ["levels_bc", "levels_1024", "levels_all", "levels_nomip", "levels_wide", "tail_rgb", "tail_rgb_wide"]
        {
            let data = std::fs::read(root.join(format!("{name}.vtf"))).unwrap();
            let prepared = crate::texture::prepare_vtf(&data, compressed).unwrap();
            let tex = renderer.create_texture_exact(&prepared.upload);
            assert_ne!(tex, TexId::MISSING);
            let secondary =
                crate::texture::prepare_vtf(&std::fs::read(root.join("levels_1024.vtf")).unwrap(), compressed).unwrap();
            let tex2 = renderer.create_texture_exact(&secondary.upload);
            for repeat in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512] {
                for blend in [0.0f32, 0.5, 1.0] {
                    renderer.set_material(
                        1,
                        &MaterialDesc {
                            texture: Some(tex),
                            texture2: Some(tex2),
                            tex_size: prepared.size.map(|v| v as f32),
                            flags: MaterialFlags::UNLIT | MaterialFlags::TWO_SIDED | MaterialFlags::OPAQUE_TEXTURE,
                            ..Default::default()
                        },
                    );
                    let vertices = [
                        [-128.0, -128.0, 0.0, 1.0],
                        [-128.0, 128.0, 0.0, 0.0],
                        [128.0, 128.0, 1.0, 0.0],
                        [128.0, -128.0, 1.0, 1.0],
                    ]
                    .map(|p| Vertex {
                        pos: [p[0], 0.0, p[1]],
                        normal: [0.0, -1.0, 0.0],
                        uv: [
                            p[2] * repeat as f32 * prepared.size[0] as f32,
                            p[3] * repeat as f32 * prepared.size[1] as f32,
                        ],
                        alpha: blend,
                    });
                    renderer.upsert_object(
                        1,
                        ObjectMesh {
                            batches: vec![MeshBatch {
                                material: 1,
                                vertices: vertices.to_vec(),
                                indices: vec![0, 1, 2, 0, 2, 3],
                                blend: Vec::new(),
                                cubemaps: Vec::new(),
                            }],
                            aabb_min: [-128.0, 0.0, -128.0],
                            aabb_max: [128.0, 0.0, 128.0],
                            ..Default::default()
                        },
                    );
                    renderer.render_view(118, &view, &Overlay::default());
                    let (_, _, pixels) = renderer.read_view_rgba8(118).unwrap();
                    let a = &reference["cases"][format!("{name}_{repeat}")]["mean"];
                    let b = &reference["cases"][format!("levels_1024_{repeat}")]["mean"];
                    let expected: [f64; 3] = std::array::from_fn(|c| {
                        gamma(
                            linear(a[c].as_f64().unwrap()) * (1.0 - f64::from(blend))
                                + linear(b[c].as_f64().unwrap()) * f64::from(blend),
                        )
                    });
                    let maximum = pixels
                        .chunks_exact(4)
                        .flat_map(|p| (0..3).map(|c| (f64::from(p[c]) - expected[c]).abs()))
                        .fold(0.0, f64::max);
                    assert!(
                        maximum <= 2.0,
                        "{name} repeat{repeat} blend{blend} compressed{compressed}: RGB error{maximum}, expected{expected:?}"
                    );
                    comparisons += 1;
                }
            }
            renderer.destroy_texture(tex);
            renderer.destroy_texture(tex2);
        }
    }
    assert_eq!(comparisons, 420);
}
