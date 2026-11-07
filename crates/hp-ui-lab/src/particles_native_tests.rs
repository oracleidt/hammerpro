use super::*;
use hp_render::{MaterialDesc, MaterialFlags, RenderMode, Renderer, TexFormat, TextureUpload, ViewKind, ViewParams};

#[test]
fn each_renderer_keeps_its_primary_and_secondary_animation_clock() {
    use hp_pcf::dmx::Value;
    use hp_pcf::sheet::{SheetFrame, SheetSequence};
    let definitions = hp_pcf::parse_file(include_bytes!("../tests/data/native_multiple_renderers.pcf")).unwrap();
    let mut definition = definitions.iter().find(|d| d.name == "hp_multirender117_double").unwrap().clone();
    for (index, render) in definition.renderers.iter_mut().enumerate() {
        let names = ["animation rate", "second sequence animation rate", "use animation rate as FPS"];
        render.params.attrs.retain(|(name, _)| !names.contains(&name.as_str()));
        render.params.attrs.extend([
            (names[0].into(), Value::Float(if index == 0 { 0.0 } else { 4.0 })),
            (names[1].into(), Value::Float(if index == 0 { 0.0 } else { 8.0 })),
            (names[2].into(), Value::Bool(true)),
        ]);
    }
    let plan = Arc::new(Plan::new(&definition));
    let mut effect = Effect::new(&definition.name, &|_| Some(plan.clone()), 17).unwrap();
    effect.step(0.02, [0.0; 3], 1);
    effect.sims[0].1.particles[0].age = 0.25;
    let left = [0.0, 0.0, 0.5, 1.0];
    let right = [0.5, 0.0, 1.0, 1.0];
    let sheet = Sheet {
        sequences: vec![SheetSequence {
            number: 0,
            clamp: false,
            frames: vec![SheetFrame { duration: 1.0, uv: left }, SheetFrame { duration: 1.0, uv: right }],
            total: 2.0,
        }],
    };
    let looks = HashMap::from([(
        material_key(&plan.material),
        Arc::new(MaterialLook {
            sheet: Some(sheet),
            spritecard: true,
            blend_frames: true,
            dual_sequence: true,
            ..Default::default()
        }),
    )]);
    let (mut sprites, mut materials) = (Vec::new(), Vec::new());
    append_sprites(std::iter::once((17, &effect)), &looks, &mut sprites, &mut materials);
    assert_eq!(sprites.len(), 2);
    assert_eq!(sprites[0].uv, left, "the first renderer is frozen");
    assert_eq!(sprites[1].uv, right, "the second advances one frame at 4 FPS");
    assert_eq!(sprites[0].animation.frames[1], left);
    assert_eq!(sprites[1].animation.frames[1], left, "secondary animation wraps after two frames at 8 FPS");
}

#[test]
fn multiple_renderers_reach_the_gpu_with_native_opacity_and_orientation() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    })) else {
        eprintln!("no GPU adapter: native multiple-renderer reference skipped");
        return;
    };
    if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
        eprintln!("no vertex storage support: native multiple-renderer reference skipped");
        return;
    }
    let (device, queue) = pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() }),
    )
    .expect("native multiple-renderer GPU device");
    let mut renderer = Renderer::new(&device, &queue);
    renderer.set_tone_settings(hp_render::ToneSettings { bloom: 0.0, ..Default::default() });
    let mut texels = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            texels.extend_from_slice(match (x < 4, y < 4) {
                (true, true) => &[255, 0, 0, 255],
                (false, true) => &[0, 0, 255, 255],
                (true, false) => &[0, 255, 0, 255],
                (false, false) => &[255, 255, 0, 255],
            });
        }
    }
    let texture = renderer.create_texture(&TextureUpload {
        width: 8,
        height: 8,
        format: TexFormat::Rgba8Srgb,
        levels: vec![texels],
    });
    // MockAssets resolves material names to ID 0. Texture pixels match the
    // authored GMA; material behavior is loaded through the real look reader.
    renderer.set_material(
        0,
        &MaterialDesc {
            texture: Some(texture),
            tex_size: [8.0; 2],
            flags: MaterialFlags::SPRITE_CARD,
            ..Default::default()
        },
    );
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let fs = hp_fs::GameFs::from_source_mounts(
        None,
        None,
        &[hp_fs::SourceMount {
            kind: hp_fs::SourceMountKind::Gma,
            label: "Owned native multiple-renderer reference".into(),
            path: data.join("native_multiple_renderers.gma"),
            enabled: true,
            origin: hp_fs::SourceOrigin::Custom,
        }],
    )
    .unwrap();
    let definitions = hp_pcf::parse_file(include_bytes!("../tests/data/native_multiple_renderers.pcf")).unwrap();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/native_multiple_renderers.json")).unwrap();
    let cases = fixture["cases"].as_object().unwrap();
    assert_eq!(cases.len(), 6);
    let mut state = hp_app::testing::test_state();
    let mut failures = Vec::new();
    let proof = data.join("../../../../target/hp-ui-lab/multiple-renderers");
    std::fs::create_dir_all(&proof).unwrap();
    let mut maximum = 0;
    for (name, case) in cases {
        let definition = definitions.iter().find(|d| d.name == format!("hp_multirender117_{name}")).unwrap();
        let plan = Arc::new(Plan::new(definition));
        assert!(plan.unsupported.is_empty(), "{name}: {:?}", plan.unsupported);
        let mut effect = Effect::new(&definition.name, &|_| Some(plan.clone()), 17).unwrap();
        effect.step(0.02, [0.0, 0.0, 64.0], 1);
        let look = read_material_look(&fs, &plan.material);
        assert!(look.found && look.spritecard && look.no_fog);
        let looks = HashMap::from([(material_key(&plan.material), Arc::new(look))]);
        let (mut sprites, mut materials) = (Vec::new(), Vec::new());
        append_sprites(std::iter::once((17, &effect)), &looks, &mut sprites, &mut materials);
        let instances = instances_of(&sprites, &materials, &mut *state.assets);
        renderer.set_particles(&instances);
        let eye = std::array::from_fn(|i| case["eye"][i].as_f64().unwrap() as f32);
        let view = ViewParams {
            size: [128, 128],
            game_pixel_grid: true,
            kind: ViewKind::Perspective {
                eye,
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
        image::save_buffer(proof.join(format!("{name}.png")), &pixels, 128, 128, image::ColorType::Rgba8).unwrap();
        let reference = image::open(data.join(format!("native_multiple_renderers_{name}.png"))).unwrap().to_rgb8();
        assert_eq!(reference.dimensions(), (128, 128));
        let difference = pixels
            .chunks_exact(4)
            .zip(reference.pixels())
            .flat_map(|(a, b)| (0..3).map(move |c| a[c].abs_diff(b[c])))
            .max()
            .unwrap();
        maximum = maximum.max(difference);
        if difference > 3 {
            failures.push(format!("{name}: {} instances, RGB delta {difference}", instances.len()));
        }
    }
    assert!(failures.is_empty(), "Native multiple-renderer image mismatches:\n{}", failures.join("\n"));
    eprintln!("Six native multiple-renderer whole images: maximum RGB delta {maximum}/255");
}
