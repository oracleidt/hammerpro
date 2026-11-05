use std::path::PathBuf;
use std::sync::OnceLock;

use hp_render::*;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    static GPU: OnceLock<Option<(wgpu::Device, wgpu::Queue)>> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .ok()?;
        if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
            return None;
        }
        let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hp-render compiled light test"),
            required_features: features,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

type Img = (u32, u32, Vec<u8>);

const SIZE: u32 = 200;
const WORLD: MaterialId = 1;
const GLASS: MaterialId = 2;
const PROP: MaterialId = 3;

fn save(r: &Renderer, name: &str) -> Img {
    let (w, h, px) = r.read_view_rgba8(1).expect("readback");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hp-render-scratch");
    let _ = std::fs::create_dir_all(&dir);
    image::save_buffer(dir.join(format!("compiled_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
    (w, h, px)
}

fn px(img: &Img, x: u32, y: u32) -> [u8; 3] {
    let o = ((y * img.0 + x) * 4) as usize;
    [img.2[o], img.2[o + 1], img.2[o + 2]]
}

fn srgb(linear: f32) -> f32 {
    let c = linear.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 * 255.0 } else { (1.055 * c.powf(1.0 / 2.4) - 0.055) * 255.0 }
}

fn linear(srgb_byte: u8) -> f32 {
    let c = f32::from(srgb_byte) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn close(got: u8, want: f32, tol: f32, what: &str) {
    assert!((f32::from(got) - want).abs() <= tol, "{what}: got {got}, want {want:.1}");
}

fn view(mode: RenderMode) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [SIZE, SIZE],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 0.0],
            forward: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 90.0,
            near: 1.0,
            far: 10000.0,
        },
        mode,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: false,
    }
}

fn mat(r: &mut Renderer, id: MaterialId, rgba: [u8; 4], flags: MaterialFlags) {
    let up = TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![rgba.repeat(16)] };
    let t = r.create_texture(&up);
    r.set_material(id, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], flags, ..Default::default() });
}

fn face(x0: f32, x1: f32, z0: f32, z1: f32, y: f32, lightmap: u32, lm: [f32; 2]) -> Vec<CompiledVertex> {
    let v = |x: f32, z: f32, s: f32| CompiledVertex {
        pos: [x, y, z],
        normal: [0.0, -1.0, 0.0],
        uv: [x, z],
        alpha: 0.0,
        lm: [s, 0.5],
        lightmap,
        cubemap: hp_render::NO_CUBEMAP,
        blend: Default::default(),
    };
    vec![v(x0, z0, lm[0]), v(x1, z0, lm[1]), v(x1, z1, lm[1]), v(x0, z1, lm[0])]
}

fn mesh(verts: &[CompiledVertex], material: MaterialId) -> ObjectMesh {
    let vertices = verts.iter().map(|v| Vertex { pos: v.pos, normal: v.normal, uv: v.uv, alpha: v.alpha }).collect();
    ObjectMesh {
        batches: vec![MeshBatch {
            material,
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }],
        aabb_min: [-100.0, 100.0, -100.0],
        aabb_max: [100.0, 100.0, 100.0],
        wire_color: [200; 3],
        ..Default::default()
    }
}

#[test]
fn compiled_world_is_lit_by_its_lightmaps_like_the_game() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [128, 128, 128, 255], MaterialFlags::empty());
    mat(&mut r, GLASS, [255, 255, 255, 128], MaterialFlags::TRANSLUCENT);
    let left = face(-100.0, -34.0, -100.0, 100.0, 100.0, 0, [0.5, 0.5]);
    let middle = face(-34.0, 34.0, -100.0, 100.0, 100.0, 1, [0.5, 1.5]);
    let right = face(34.0, 100.0, -100.0, 100.0, 100.0, 2, [0.5, 0.5]);
    let glass = face(-50.0, -30.0, -50.0, -30.0, 50.0, NO_COMPILED_LIGHTMAP, [0.5, 0.5]);
    let all: Vec<CompiledVertex> = left.iter().chain(&middle).chain(&right).copied().collect();
    let indices: Vec<u32> = (0..3u32).flat_map(|f| [0, 1, 2, 0, 2, 3].map(|i| f * 4 + i)).collect();
    let mut world = mesh(&all, WORLD);
    world.batches[0].indices = indices.clone();
    r.upsert_object(1, world);
    r.upsert_object(2, mesh(&glass, GLASS));
    for id in [1, 2] {
        r.set_object_flags(id, ObjectFlags::COMPILED_LIGHT);
    }
    let blocks = vec![
        CompiledBlock { object: 1, batches: vec![CompiledBatch { material: WORLD, vertices: all.clone(), indices }] },
        CompiledBlock {
            object: 2,
            batches: vec![CompiledBatch { material: GLASS, vertices: glass.clone(), indices: vec![0, 1, 2, 0, 2, 3] }],
        },
    ];
    let half = [[0.5f32; 3]];
    let ramp = [[0.0f32; 3], [1.0; 3]];
    let bright = [[1.5f32; 3]];
    let maps = [
        CompiledLightmap { width: 1, height: 1, texels: &half, styles: &[], bumped: false },
        CompiledLightmap { width: 2, height: 1, texels: &ramp, styles: &[], bumped: false },
        CompiledLightmap { width: 1, height: 1, texels: &bright, styles: &[], bumped: false },
    ];
    let render = |r: &mut Renderer, mode: RenderMode, on: bool, name: &str| {
        r.set_view_options(1, ViewOptions { compiled_lighting: on, ..ViewOptions::default() });
        r.render_view(1, &view(mode), &Overlay::default());
        save(r, name)
    };
    let editor = render(&mut r, RenderMode::Textured, true, "editor");

    r.set_tone_settings(ToneSettings { bloom: 0.0, ..ToneSettings::default() });
    let stats = r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    assert_eq!((stats.lightmaps, stats.dropped, stats.blocks), (3, 0, 2));
    let hdr = render(&mut r, RenderMode::Textured, true, "hdr");
    let gray = linear(128);
    close(px(&hdr, 20, 60)[0], srgb(gray * 0.5), 2.0, "HDR half");
    close(px(&hdr, 180, 100)[0], srgb(gray * 1.5), 2.0, "HDR 1.5");
    let (l, m, rr) = (px(&hdr, 72, 100)[0], px(&hdr, 100, 100)[0], px(&hdr, 127, 100)[0]);
    let ramp_at = |x: f32| gray * (x + 34.0) / 68.0;
    close(m, srgb(ramp_at(0.5)), 3.0, "ramp middle");
    assert!(l < m && m < rr, "the ramp grows: {l} {m} {rr}");
    close(l, srgb(ramp_at(-27.5)), 3.0, "ramp left");
    close(rr, srgb(ramp_at(27.5)), 3.0, "ramp right");
    let g = px(&hdr, 20, 180)[0];
    let behind = gray * 0.5;
    let want = srgb(behind + (1.0 - behind) * (128.0 / 255.0));
    close(g, want, 4.0, "glass");

    assert_eq!(r.tone_scale(1), 1.0);
    let e = r.step_tone(1, 0.5);
    assert!(e > 1.03 && e <= 2.0, "{e}");
    let exposed = render(&mut r, RenderMode::Textured, true, "hdr_exposed");
    close(px(&exposed, 20, 60)[0], srgb(gray * 0.5 * e), 2.0, "HDR half exposed");
    r.set_tone_settings(ToneSettings::default());
    let bloom = render(&mut r, RenderMode::Textured, true, "hdr_bloom");
    assert!(px(&bloom, 20, 180)[0] > px(&exposed, 20, 180)[0], "on the glass");
    assert!(px(&bloom, 20, 100)[0] >= px(&exposed, 20, 100)[0], "everywhere a little");
    r.set_tone_settings(ToneSettings { bloom: 0.0, ..ToneSettings::default() });
    r.set_view_options(1, ViewOptions::default());
    assert_eq!(r.step_tone(1, 0.5), 1.0);

    r.set_compiled_lighting(CompiledLightMode::Ldr, &maps, &blocks);
    let ldr = render(&mut r, RenderMode::Textured, true, "ldr");
    close(px(&ldr, 20, 60)[0], 128.0 * 0.5, 2.0, "LDR half");
    close(px(&ldr, 180, 100)[0], 128.0 * 1.5, 2.0, "LDR 1.5");

    let off = render(&mut r, RenderMode::Textured, false, "off");
    assert_eq!(px(&off, 20, 60), px(&editor, 20, 60), "the editor shading");
    let flat = render(&mut r, RenderMode::Flat, true, "flat");
    assert_ne!(px(&flat, 20, 60), px(&ldr, 20, 60));

    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT | ObjectFlags::HIDDEN);
    let hidden = render(&mut r, RenderMode::Textured, true, "hidden");
    assert_eq!(px(&hidden, 180, 100), [0, 0, 0]);
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT);

    r.set_compiled_lighting(CompiledLightMode::Fullbright, &[], &blocks);
    let full = render(&mut r, RenderMode::Textured, true, "fullbright");
    close(px(&full, 20, 60)[0], 128.0, 2.0, "fullbright");
    close(px(&full, 180, 100)[0], 128.0, 2.0, "fullbright");

    assert_eq!(r.lightmap_stats().faces, 0);
    r.clear_compiled_lighting();
    assert!(r.compiled_lighting().is_none());
    let cleared = render(&mut r, RenderMode::Textured, true, "cleared");
    assert_eq!(px(&cleared, 20, 60), px(&editor, 20, 60));
}

#[test]
fn material_color_tints_multiply_the_texture_directly_in_hdr_like_the_game() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let up = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[128u8, 128, 128, 255].repeat(16)],
    };
    let t = r.create_texture(&up);
    r.set_material(
        WORLD,
        &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], tint: [0.5, 0.5, 0.5, 1.0], ..Default::default() },
    );
    let quad = face(-100.0, 100.0, -100.0, 100.0, 100.0, 0, [0.5, 0.5]);
    r.upsert_object(1, mesh(&quad, WORLD));
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT);
    let blocks = vec![CompiledBlock {
        object: 1,
        batches: vec![CompiledBatch { material: WORLD, vertices: quad.clone(), indices: vec![0, 1, 2, 0, 2, 3] }],
    }];
    let one = [[1.0f32; 3]];
    let maps = [CompiledLightmap { width: 1, height: 1, texels: &one, styles: &[], bumped: false }];
    r.set_tone_settings(ToneSettings { mode: ToneMode::Fixed, bloom: 0.0, ..ToneSettings::default() });
    r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    r.set_view_options(1, ViewOptions { compiled_lighting: true, ..ViewOptions::default() });
    let mut params = view(RenderMode::Textured);
    params.msaa = true;
    r.render_view(1, &params, &Overlay::default());
    let hdr = save(&r, "tint_hdr");
    close(px(&hdr, 100, 100)[0], srgb(linear(128) * 0.5), 2.0, "HDR tint is a plain multiplier");
    r.set_compiled_lighting(CompiledLightMode::Ldr, &maps, &blocks);
    r.render_view(1, &view(RenderMode::Textured), &Overlay::default());
    let ldr = save(&r, "tint_ldr");
    close(px(&ldr, 100, 100)[0], srgb(linear(128) * linear(128)), 2.0, "LDR tint is read as a color value");
}

#[test]
fn compiled_world_faces_glow_with_selfillum() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [128, 128, 128, 128], MaterialFlags::SELF_ILLUM | MaterialFlags::OPAQUE_TEXTURE);
    let quad = face(-100.0, 100.0, -100.0, 100.0, 100.0, 0, [0.5, 0.5]);
    r.upsert_object(1, mesh(&quad, WORLD));
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT);
    let blocks = vec![CompiledBlock {
        object: 1,
        batches: vec![CompiledBatch { material: WORLD, vertices: quad.clone(), indices: vec![0, 1, 2, 0, 2, 3] }],
    }];
    let quarter = [[0.25f32; 3]];
    let maps = [CompiledLightmap { width: 1, height: 1, texels: &quarter, styles: &[], bumped: false }];
    r.set_tone_settings(ToneSettings { bloom: 0.0, ..ToneSettings::default() });
    r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    r.set_view_options(1, ViewOptions { compiled_lighting: true, ..ViewOptions::default() });
    let render = |r: &mut Renderer, name: &str| {
        r.render_view(1, &view(RenderMode::Textured), &Overlay::default());
        save(r, name)
    };
    let gray = linear(128);
    let glow = 128.0 / 255.0;
    let img = render(&mut r, "selfillum");
    close(px(&img, 100, 100)[0], srgb(gray * (0.25 * (1.0 - glow) + glow)), 3.0, "glow by the alpha");
    let mask = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Linear,
        levels: vec![[255, 0, 0, 255].repeat(16)],
    };
    let mask = r.create_texture(&mask);
    let base = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[128, 128, 128, 255].repeat(16)],
    };
    let base = r.create_texture(&base);
    r.set_material(
        WORLD,
        &MaterialDesc {
            texture: Some(base),
            tex_size: [4.0, 4.0],
            flags: MaterialFlags::SELF_ILLUM | MaterialFlags::OPAQUE_TEXTURE,
            self_illum: Some(SelfIllumDesc { tint: [2.0, 1.0, 1.0], mask: Some(mask), mask_alpha: false }),
            ..Default::default()
        },
    );
    let img = render(&mut r, "selfillum_mask");
    let c = px(&img, 100, 100);
    close(c[0], srgb(gray * 2.0), 3.0, "masked red, tinted");
    close(c[1], srgb(gray * 0.25), 3.0, "unmasked green keeps the lightmap");
    mat(&mut r, WORLD, [128, 128, 128, 128], MaterialFlags::OPAQUE_TEXTURE);
    let img = render(&mut r, "selfillum_off");
    close(px(&img, 100, 100)[0], srgb(gray * 0.25), 3.0, "no glow");
}

#[test]
fn models_take_their_compiled_lighting() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    let n = [0.0, -1.0, 0.0];
    let v = |x: f32, z: f32| Vertex { pos: [x, 100.0, z], normal: n, uv: [0.0, 0.0], alpha: 1.0 };
    let model = r.create_model(&[ModelPart {
        material: PROP,
        vertices: vec![v(-50.0, -50.0), v(50.0, -50.0), v(50.0, 50.0), v(-50.0, 50.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    }]);
    r.set_model_instances(&[ModelInstance { model, object: 50, ..Default::default() }]);
    r.set_tone_settings(ToneSettings { bloom: 0.0, ..ToneSettings::default() });
    r.set_compiled_lighting(CompiledLightMode::Hdr, &[], &[]);
    r.set_view_options(1, ViewOptions { compiled_lighting: true, ..ViewOptions::default() });
    let shot = |r: &mut Renderer, name: &str| {
        r.render_view(1, &view(RenderMode::Textured), &Overlay::default());
        save(r, name)
    };
    let albedo = linear(200);
    let mut ambient = [[0.0f32; 3]; 6];
    ambient[3] = [0.25; 3];
    ambient[2] = [9.0; 3];
    let light = ModelLocalLight {
        kind: 0,
        origin: [0.0, 0.0, 0.0],
        color: [2500.0; 3],
        attenuation: [0.0, 0.0, 1.0],
        ..Default::default()
    };
    r.set_model_lighting(&[ModelLighting { object: 50, ambient, lights: vec![light], ..Default::default() }]);
    let lit = shot(&mut r, "model_ambient");
    let d2 = 50.0f32 * 50.0 * 2.0 + 100.0 * 100.0;
    let corner = 2500.0 / d2 * (100.0 / d2.sqrt());
    close(px(&lit, 100, 100)[0], srgb(albedo * (0.25 + corner)), 3.0, "ambient and light");
    let colors = vec![[128u8, 128, 128, 255]; 4];
    r.set_model_lighting(&[ModelLighting {
        object: 50,
        model: Some(model),
        vertex_colors: colors.clone(),
        ambient,
        ..Default::default()
    }]);
    let baked = shot(&mut r, "model_vertex");
    close(px(&baked, 100, 100)[0], srgb(albedo * (256.0f32 / 255.0).powf(2.2)), 3.0, "vertex colors");
    let another = r.create_model(&[]);
    r.set_model_lighting(&[ModelLighting {
        object: 50,
        model: Some(another),
        vertex_colors: colors,
        ambient,
        ..Default::default()
    }]);
    let other = shot(&mut r, "model_other");
    close(px(&other, 100, 100)[0], srgb(albedo * 0.25), 3.0, "ambient only");
    r.set_compiled_lighting(CompiledLightMode::Fullbright, &[], &[]);
    let full = shot(&mut r, "model_fullbright");
    close(px(&full, 100, 100)[0], 200.0, 2.0, "fullbright model");
    r.set_view_options(1, ViewOptions::default());
    r.set_compiled_lighting(CompiledLightMode::Hdr, &[], &[]);
    let editor = shot(&mut r, "model_editor");
    assert_ne!(px(&editor, 100, 100), px(&other, 100, 100));
}

#[test]
fn light_styles_scale_their_lightmaps_and_lights() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [128, 128, 128, 255], MaterialFlags::empty());
    let left = face(-100.0, -34.0, -100.0, 100.0, 100.0, 0, [0.5, 0.5]);
    let right = face(34.0, 100.0, -100.0, 100.0, 100.0, 1, [0.5, 1.5]);
    let all: Vec<CompiledVertex> = left.iter().chain(&right).copied().collect();
    let indices: Vec<u32> = (0..2u32).flat_map(|f| [0, 1, 2, 0, 2, 3].map(|i| f * 4 + i)).collect();
    let mut world = mesh(&all, WORLD);
    world.batches[0].indices = indices.clone();
    r.upsert_object(1, world);
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT);
    let blocks =
        vec![CompiledBlock { object: 1, batches: vec![CompiledBatch { material: WORLD, vertices: all, indices }] }];
    let half = [[0.5f32; 3]];
    let styled = [[0.25f32; 3], [0.25; 3], [0.5; 3], [0.5; 3]];
    let maps = [
        CompiledLightmap { width: 1, height: 1, texels: &half, styles: &[], bumped: false },
        CompiledLightmap { width: 2, height: 1, texels: &styled, styles: &[0, 10], bumped: false },
    ];
    r.set_tone_settings(ToneSettings { bloom: 0.0, ..ToneSettings::default() });
    let stats = r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    assert_eq!((stats.lightmaps, stats.animated), (2, 1));
    r.set_view_options(1, ViewOptions { compiled_lighting: true, ..ViewOptions::default() });
    let render = |r: &mut Renderer, name: &str| {
        r.render_view(1, &view(RenderMode::Textured), &Overlay::default());
        save(r, name)
    };
    let gray = linear(128);
    let mut values = vec![1.0f32; LIGHT_STYLES];
    for (v10, name) in [(1.0f32, "styles_m"), (0.0, "styles_a"), (550.0 / 264.0, "styles_z")] {
        values[10] = v10;
        r.set_light_styles(&values);
        assert_eq!(r.light_styles()[10], v10);
        let img = render(&mut r, name);
        close(px(&img, 20, 100)[0], srgb(gray * 0.5), 2.0, "style 0 alone, unchanged");
        close(px(&img, 180, 100)[0], srgb(gray * (0.25 + 0.5 * v10)), 2.0, name);
    }
    values[10] = 1.0;
    r.set_light_styles(&values);
    r.set_compiled_lighting(CompiledLightMode::Ldr, &maps, &blocks);
    let ldr = render(&mut r, "styles_ldr");
    close(px(&ldr, 180, 100)[0], 128.0 * 0.75f32.powf(1.0 / 2.2), 2.0, "LDR styles");
    values[10] = 0.0;
    r.set_light_styles(&values);
    let ldr_off = render(&mut r, "styles_ldr_off");
    close(px(&ldr_off, 180, 100)[0], 128.0 * 0.25f32.powf(1.0 / 2.2), 2.0, "LDR style off");
    r.clear_compiled_lighting();
    r.remove_object(1);

    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    let n = [0.0, -1.0, 0.0];
    let v = |x: f32, z: f32| Vertex { pos: [x, 100.0, z], normal: n, uv: [0.0, 0.0], alpha: 1.0 };
    let model = r.create_model(&[ModelPart {
        material: PROP,
        vertices: vec![v(-50.0, -50.0), v(50.0, -50.0), v(50.0, 50.0), v(-50.0, 50.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    }]);
    r.set_model_instances(&[ModelInstance { model, object: 50, ..Default::default() }]);
    r.set_compiled_lighting(CompiledLightMode::Hdr, &[], &[]);
    let mut ambient = [[0.0f32; 3]; 6];
    ambient[3] = [0.25; 3];
    let light = ModelLocalLight { kind: 2, dir: [0.0, 1.0, 0.0], color: [0.5; 3], style: 10, ..Default::default() };
    let albedo = linear(200);
    r.set_model_lighting(&[ModelLighting { object: 50, ambient, lights: vec![light], ..Default::default() }]);
    values[10] = 1.0;
    r.set_light_styles(&values);
    close(px(&render(&mut r, "styles_model_on"), 100, 100)[0], srgb(albedo * 0.75), 3.0, "styled light on");
    values[10] = 0.0;
    r.set_light_styles(&values);
    close(px(&render(&mut r, "styles_model_off"), 100, 100)[0], srgb(albedo * 0.25), 3.0, "styled light off");
    let c = (0.25f32.powf(1.0 / 2.2) * 127.5).round() as u8;
    let colored = linear_of_vertex(c);
    r.set_model_lighting(&[ModelLighting {
        object: 50,
        model: Some(model),
        vertex_colors: vec![[c, c, c, 255]; 4],
        ambient,
        lights: vec![light],
        styled_lights: vec![light],
    }]);
    close(px(&render(&mut r, "styles_vertex_off"), 100, 100)[0], srgb(albedo * colored), 3.0, "vertex colors alone");
    values[10] = 2.0;
    r.set_light_styles(&values);
    let on = render(&mut r, "styles_vertex_on");
    close(px(&on, 100, 100)[0], srgb(albedo * (colored + 1.0)), 3.0, "vertex colors and the styled light");
}

fn linear_of_vertex(c: u8) -> f32 {
    (f32::from(c) / 255.0 * 2.0).powf(2.2)
}

fn lin(c: u8) -> f32 {
    linear(c)
}

#[test]
fn brush_entities_blend_their_compiled_faces_back_to_front() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    const WHITE: MaterialId = 10;
    const RED: MaterialId = 11;
    const GREEN: MaterialId = 12;
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [128, 128, 128, 255], MaterialFlags::empty());
    mat(&mut r, GLASS, [255, 255, 255, 128], MaterialFlags::TRANSLUCENT);
    mat(&mut r, WHITE, [255, 255, 255, 255], MaterialFlags::empty());
    mat(&mut r, RED, [255, 0, 0, 128], MaterialFlags::TRANSLUCENT);
    mat(&mut r, GREEN, [0, 255, 0, 128], MaterialFlags::TRANSLUCENT);
    let wall = face(-100.0, 100.0, -100.0, 100.0, 100.0, 0, [0.5, 0.5]);
    let glass = face(-90.0, -10.0, -50.0, 50.0, 50.0, 1, [0.5, 0.5]);
    let faded = face(10.0, 90.0, -50.0, 50.0, 50.0, 2, [0.5, 0.5]);
    let near = face(-10.0, 10.0, -35.0, -25.0, 40.0, NO_COMPILED_LIGHTMAP, [0.5, 0.5]);
    let far = face(-40.0, 40.0, -70.0, -10.0, 80.0, NO_COMPILED_LIGHTMAP, [0.5, 0.5]);
    r.upsert_object(1, mesh(&wall, WORLD));
    r.upsert_object(10, mesh(&glass, GLASS));
    r.upsert_object(11, mesh(&faded, WHITE));
    r.upsert_object(12, mesh(&near, RED));
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT);
    for id in [10, 11, 12] {
        r.set_object_flags(id, ObjectFlags::BRUSH_ENTITY);
    }
    r.set_object_tint(11, [1.0, 1.0, 1.0, 128.0 / 255.0]);
    let quad = vec![0, 1, 2, 0, 2, 3];
    let block = |object: ObjectId, batches: Vec<CompiledBatch>| CompiledBlock { object, batches };
    let batch = |material: MaterialId, vertices: &[CompiledVertex]| CompiledBatch {
        material,
        vertices: vertices.to_vec(),
        indices: quad.clone(),
    };
    let blocks = vec![
        block(1, vec![batch(WORLD, &wall)]),
        block(10, vec![batch(GLASS, &glass)]),
        block(11, vec![batch(WHITE, &faded)]),
        block(12, vec![batch(RED, &near), batch(GREEN, &far)]),
    ];
    let half = [[0.5f32; 3]];
    let quarter = [[0.25f32; 3]];
    let bright = [[4.0f32; 3]];
    let maps = [
        CompiledLightmap { width: 1, height: 1, texels: &half, styles: &[], bumped: false },
        CompiledLightmap { width: 1, height: 1, texels: &quarter, styles: &[], bumped: false },
        CompiledLightmap { width: 1, height: 1, texels: &bright, styles: &[], bumped: false },
    ];
    r.set_tone_settings(ToneSettings { bloom: 0.0, ..ToneSettings::default() });
    r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    r.set_view_options(1, ViewOptions { compiled_lighting: true, ..ViewOptions::default() });
    let shot = |r: &mut Renderer, name: &str| {
        r.render_view(1, &view(RenderMode::Textured), &Overlay::default());
        save(r, name)
    };
    let editor = shot(&mut r, "entities_editor");
    r.set_compiled_objects([10, 11, 12].into_iter().collect());
    assert!(r.object_flags(10).is_some_and(|f| f.contains(ObjectFlags::COMPILED_LIGHT | ObjectFlags::BRUSH_ENTITY)));
    let lit = shot(&mut r, "entities_compiled");
    let behind = lin(128) * 0.5;
    let a = 128.0 / 255.0;
    let glass_px = px(&lit, 50, 100)[0];
    close(glass_px, srgb(behind * (1.0 - a) + 0.25 * a), 3.0, "lit glass");
    assert_ne!(px(&editor, 50, 100)[0], glass_px, "not the editor shading");
    close(px(&lit, 150, 100)[0], srgb(behind * (1.0 - a) + a), 3.0, "faded entity");
    let p = px(&lit, 100, 175);
    let red = a + behind * (1.0 - a) * (1.0 - a);
    let green = a * (1.0 - a) + behind * (1.0 - a) * (1.0 - a);
    close(p[0], srgb(red), 4.0, "near pane red");
    close(p[1], srgb(green), 4.0, "far pane green under it");
    r.set_object_tint(11, [1.0, 1.0, 1.0, 0.0]);
    let gone = shot(&mut r, "entities_gone");
    close(px(&gone, 150, 100)[0], srgb(behind), 2.0, "render amount 0");
    r.set_compiled_objects(Default::default());
    assert!(r.object_flags(10).is_some_and(|f| !f.contains(ObjectFlags::COMPILED_LIGHT)));
    let back = shot(&mut r, "entities_back");
    assert_eq!(px(&back, 50, 100), px(&editor, 50, 100), "the main pass again");
}

#[test]
fn detail_models_fade_and_take_their_color_like_the_client() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::empty());
    let n = [0.0, -1.0, 0.0];
    let v = |x: f32, z: f32| Vertex { pos: [x, 0.0, z], normal: n, uv: [0.0, 0.0], alpha: 1.0 };
    let model = r.create_model(&[ModelPart {
        material: PROP,
        vertices: vec![v(-900.0, -900.0), v(900.0, -900.0), v(900.0, 900.0), v(-900.0, 900.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    }]);
    let at = |d: f32| ModelInstance {
        model,
        object: 70,
        transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, d, 0.0, 1.0]],
        tint: [0.5, 0.5, 0.5, 1.0],
        fade: ModelInstance::detail_fade(1200.0, 400.0),
    };
    r.set_tone_settings(ToneSettings { bloom: 0.0, ..ToneSettings::default() });
    r.set_compiled_lighting(CompiledLightMode::Hdr, &[], &[]);
    r.set_view_options(1, ViewOptions { compiled_lighting: true, ..ViewOptions::default() });
    let mut ambient = [[0.0f32; 3]; 6];
    ambient[3] = [1.0; 3];
    r.set_model_lighting(&[ModelLighting { object: 70, ambient, ..Default::default() }]);
    let shot = |r: &mut Renderer, d: f32, name: &str| {
        r.set_model_instances(&[at(d)]);
        r.render_view(1, &view(RenderMode::Textured), &Overlay::default());
        px(&save(r, name), 100, 100)[0]
    };
    let lit = lin(200) * lin(128);
    close(shot(&mut r, 100.0, "detail_near"), srgb(lit), 3.0, "near: opaque");
    let alpha = (1200.0f32 * 1200.0 - 1000.0 * 1000.0) / (1200.0 * 1200.0 - 800.0 * 800.0);
    close(shot(&mut r, 1000.0, "detail_fading"), srgb(lit * alpha), 3.0, "fading");
    assert_eq!(shot(&mut r, 1300.0, "detail_gone"), 0, "beyond cl_detaildist");
    r.set_model_lighting(&[ModelLighting { object: 70, ..Default::default() }]);
    mat(&mut r, PROP, [200, 200, 200, 255], MaterialFlags::UNLIT);
    close(shot(&mut r, 100.0, "detail_unlit"), srgb(lit), 3.0, "unlit: the tint only");
    let f = ModelInstance::detail_fade(300.0, 500.0);
    assert!(f[0] == 0.0 && f[1] == -300.0, "{f:?}");
}

#[test]
fn detail_sprites_follow_the_detail_distances() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [255, 255, 255, 255], MaterialFlags::empty());
    r.set_object_flags(80, ObjectFlags::empty());
    let sprite = |y: f32| DetailSprite {
        origin: [0.0, y, 0.0],
        right: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        ul: [-400.0, 400.0],
        lr: [400.0, -400.0],
        tex: [0.0, 0.0, 1.0, 1.0],
        orientation: DetailSpriteOrientation::Fixed,
        sway: 0,
        color: [255; 4],
        object: 80,
    };
    let shot = |r: &mut Renderer, y: f32, name: &str| {
        r.set_detail_sprites(PROP, &[sprite(y)]);
        r.render_view(1, &view(RenderMode::Textured), &Overlay::default());
        px(&save(r, name), 100, 100)[0]
    };
    assert!(shot(&mut r, 300.0, "sprite_default") > 200, "drawn at 300");
    r.set_detail_fade(Some([500.0, 100.0]));
    assert!(shot(&mut r, 300.0, "sprite_near") > 200, "still drawn at 300");
    assert_eq!(shot(&mut r, 600.0, "sprite_far"), 0, "gone past 500");
    r.set_detail_fade(None);
    assert!(shot(&mut r, 600.0, "sprite_back") > 200, "the default distance again");
}
