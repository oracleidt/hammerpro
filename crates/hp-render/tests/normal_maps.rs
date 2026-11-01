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
            label: Some("hp-render normal map test"),
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
const WALL: MaterialId = 1;
const PROP: MaterialId = 2;

fn save(r: &Renderer, name: &str) -> Img {
    let (w, h, px) = r.read_view_rgba8(1).expect("readback");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hp-render-scratch");
    let _ = std::fs::create_dir_all(&dir);
    image::save_buffer(dir.join(format!("bump_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
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

fn normal_map(r: &mut Renderer, left: [u8; 3], right: [u8; 3]) -> TexId {
    let mut px = Vec::with_capacity(16 * 16 * 4);
    for _y in 0..16 {
        for x in 0..16 {
            let c = if x < 8 { left } else { right };
            px.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    r.create_texture(&TextureUpload { width: 16, height: 16, format: TexFormat::Rgba8Linear, levels: vec![px] })
}

fn encode(n: [f32; 3]) -> [u8; 3] {
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    n.map(|v| ((v / l * 0.5 + 0.5) * 255.0).round() as u8)
}

fn wall_material(r: &mut Renderer, bump: Option<BumpDesc>) {
    let up = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[128u8, 128, 128, 255].repeat(16)],
    };
    let t = r.create_texture(&up);
    r.set_material(WALL, &MaterialDesc { texture: Some(t), tex_size: [200.0, 200.0], bump, ..Default::default() });
}

fn wall_vertices() -> Vec<CompiledVertex> {
    let v = |x: f32, z: f32| CompiledVertex {
        pos: [x, 100.0, z],
        normal: [0.0, -1.0, 0.0],
        uv: [x + 100.0, z + 100.0],
        alpha: 0.0,
        lm: [0.5, 0.5],
        lightmap: 0,
        cubemap: hp_render::NO_CUBEMAP,
        blend: Default::default(),
    };
    vec![v(-100.0, -100.0), v(100.0, -100.0), v(100.0, 100.0), v(-100.0, 100.0)]
}

fn wall_mesh() -> ObjectMesh {
    let vertices =
        wall_vertices().iter().map(|v| Vertex { pos: v.pos, normal: v.normal, uv: v.uv, alpha: v.alpha }).collect();
    ObjectMesh {
        batches: vec![MeshBatch {
            material: WALL,
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

fn shot(r: &mut Renderer, mode: RenderMode, options: ViewOptions, name: &str) -> Img {
    r.set_view_options(1, options);
    r.render_view(1, &view(mode), &Overlay::default());
    save(r, name)
}

fn on() -> ViewOptions {
    ViewOptions { normal_maps: true, ..ViewOptions::default() }
}

#[test]
fn normal_maps_give_the_editor_shading_relief() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.upsert_object(1, wall_mesh());
    wall_material(&mut r, None);
    let plain = shot(&mut r, RenderMode::Textured, on(), "textured_plain");
    let n = normal_map(&mut r, encode([0.7, 0.0, 0.7]), encode([-0.7, 0.0, 0.7]));
    wall_material(&mut r, Some(BumpDesc::new(n)));
    let bumped = shot(&mut r, RenderMode::Textured, on(), "textured_bumped");
    let (l, rr, p) = (px(&bumped, 40, 100)[0], px(&bumped, 160, 100)[0], px(&plain, 40, 100)[0]);
    assert!(l > p + 10 && rr + 10 < p, "relief: left {l}, right {rr}, flat {p}");
    let off = shot(&mut r, RenderMode::Textured, ViewOptions::default(), "textured_off");
    assert_eq!(px(&off, 40, 100), px(&plain, 40, 100));
    assert_eq!(px(&off, 160, 100), px(&plain, 160, 100));
    let flat = normal_map(&mut r, [128, 128, 255], [128, 128, 255]);
    wall_material(&mut r, Some(BumpDesc::new(flat)));
    let same = shot(&mut r, RenderMode::Textured, on(), "textured_flat");
    for x in [40, 160] {
        let (a, b) = (px(&same, x, 100)[0], px(&plain, x, 100)[0]);
        assert!(a.abs_diff(b) <= 2, "flat normal map: {a} vs {b}");
    }
    wall_material(&mut r, Some(BumpDesc::new(n)));
    let unshaded = shot(&mut r, RenderMode::Unshaded, on(), "unshaded");
    assert_eq!(px(&unshaded, 40, 100), px(&unshaded, 160, 100));
    wall_material(&mut r, Some(BumpDesc { uv: [2.0, 1.0, 0.0, 0.0], ..BumpDesc::new(n) }));
    let scaled = shot(&mut r, RenderMode::Textured, on(), "textured_scaled");
    assert!(px(&scaled, 20, 100)[0] > px(&scaled, 70, 100)[0] + 10, "first copy");
    assert!(px(&scaled, 120, 100)[0] > px(&scaled, 170, 100)[0] + 10, "second copy");
}

#[test]
fn self_shadowed_bump_maps_shade_like_source() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.upsert_object(1, wall_mesh());
    wall_material(&mut r, None);
    let plain = shot(&mut r, RenderMode::Textured, on(), "ss_plain");
    let f = (255.0f32 / 3f32.sqrt()).round() as u8;
    let dark = f / 3;
    let ss = normal_map(&mut r, [f, f, f], [dark, dark, dark]);
    wall_material(&mut r, Some(BumpDesc { ssbump: true, ..BumpDesc::new(ss) }));
    let img = shot(&mut r, RenderMode::Textured, on(), "ss_bumped");
    let (flat, shadowed, p) = (px(&img, 40, 100)[0], px(&img, 160, 100)[0], px(&plain, 40, 100)[0]);
    assert!(flat.abs_diff(p) <= 3, "flat ssbump: {flat} vs {p}");
    assert!(shadowed + 10 < flat, "self shadowed: {shadowed} vs {flat}");
    let toward = normal_map(&mut r, [255, 0, 0], [0, 180, 180]);
    wall_material(&mut r, Some(BumpDesc { ssbump: true, ..BumpDesc::new(toward) }));
    let dir = shot(&mut r, RenderMode::Textured, on(), "ss_direction");
    assert!(px(&dir, 40, 100)[0] > px(&dir, 160, 100)[0] + 10);
}

#[test]
fn normal_maps_light_the_preview_lights() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.upsert_object(1, wall_mesh());
    let light = PreviewLight {
        kind: LightKind::Point,
        pos: [400.0, 60.0, 0.0],
        color: [1.0; 3],
        intensity: 40.0,
        range: 4000.0,
        falloff: None,
    };
    r.set_lights(&[light], [0.0; 3]);
    let n = normal_map(&mut r, encode([0.7, 0.0, 0.7]), encode([-0.7, 0.0, 0.7]));
    wall_material(&mut r, None);
    let plain = shot(&mut r, RenderMode::Lit, on(), "lit_plain");
    wall_material(&mut r, Some(BumpDesc::new(n)));
    let bumped = shot(&mut r, RenderMode::Lit, on(), "lit_bumped");
    let off = shot(&mut r, RenderMode::Lit, ViewOptions::default(), "lit_off");
    assert!(px(&bumped, 40, 100)[0] > px(&plain, 40, 100)[0] + 5, "toward the light");
    assert!(px(&bumped, 160, 100)[0] + 5 < px(&plain, 160, 100)[0], "away from the light");
    assert_eq!(px(&off, 40, 100), px(&plain, 40, 100));
}

#[test]
fn bumped_lightmaps_combine_like_the_game() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.upsert_object(1, wall_mesh());
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT);
    r.set_compiled_bloom(false);
    let verts = wall_vertices();
    let blocks = vec![CompiledBlock {
        object: 1,
        batches: vec![CompiledBatch { material: WALL, vertices: verts, indices: vec![0, 1, 2, 0, 2, 3] }],
    }];
    let texels = [[0.2f32; 3], [0.9; 3], [0.3; 3], [0.06; 3]];
    let bumped = [CompiledLightmap { width: 1, height: 1, texels: &texels, styles: &[], bumped: true }];
    r.set_compiled_lighting(CompiledLightMode::Hdr, &bumped, &blocks);
    let compiled = ViewOptions { compiled_lighting: true, normal_maps: true, ..ViewOptions::default() };
    let gray = linear(128);
    wall_material(&mut r, None);
    let flat = shot(&mut r, RenderMode::Textured, compiled, "lm_flat");
    close(px(&flat, 100, 100)[0], srgb(gray * 0.2), 2.0, "flat lightmap");
    let up = normal_map(&mut r, [128, 128, 255], [128, 128, 255]);
    wall_material(&mut r, Some(BumpDesc::new(up)));
    let avg = shot(&mut r, RenderMode::Textured, compiled, "lm_average");
    close(px(&avg, 100, 100)[0], srgb(gray * (0.9 + 0.3 + 0.06) / 3.0), 3.0, "averaged bump lightmaps");
    let b0 = encode([0.816_496_6, 0.0, 0.577_350_3]);
    let along = normal_map(&mut r, b0, b0);
    wall_material(&mut r, Some(BumpDesc::new(along)));
    let first = shot(&mut r, RenderMode::Textured, compiled, "lm_first");
    close(px(&first, 100, 100)[0], srgb(gray * 0.9), 4.0, "first bump lightmap");
    let ss = normal_map(&mut r, [128, 0, 255], [128, 0, 255]);
    wall_material(&mut r, Some(BumpDesc { ssbump: true, ..BumpDesc::new(ss) }));
    let ssb = shot(&mut r, RenderMode::Textured, compiled, "lm_ssbump");
    let want = (128.0 / 255.0) * 0.9 + 0.06;
    close(px(&ssb, 100, 100)[0], srgb(gray * want), 4.0, "ssbump weights");
    let off = shot(&mut r, RenderMode::Textured, ViewOptions { normal_maps: false, ..compiled }, "lm_off");
    close(px(&off, 100, 100)[0], srgb(gray * 0.2), 2.0, "normal maps off");
    wall_material(&mut r, Some(BumpDesc::new(along)));
    let one = [[0.2f32; 3]];
    let plain = [CompiledLightmap { width: 1, height: 1, texels: &one, styles: &[], bumped: false }];
    r.set_compiled_lighting(CompiledLightMode::Hdr, &plain, &blocks);
    let unbumped = shot(&mut r, RenderMode::Textured, compiled, "lm_unbumped");
    close(px(&unbumped, 100, 100)[0], srgb(gray * 0.2), 2.0, "no bumped lightmaps");
    let styled = [[0.0f32; 3], [0.4; 3], [0.4; 3], [0.4; 3], [0.0; 3], [0.2; 3], [0.2; 3], [0.2; 3]];
    let maps = [CompiledLightmap { width: 1, height: 1, texels: &styled, styles: &[0, 10], bumped: true }];
    r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    wall_material(&mut r, Some(BumpDesc::new(up)));
    let both = shot(&mut r, RenderMode::Textured, compiled, "lm_styles");
    close(px(&both, 100, 100)[0], srgb(gray * 0.6), 3.0, "two styles");
    let mut values = vec![1.0f32; LIGHT_STYLES];
    values[10] = 0.0;
    r.set_light_styles(&values);
    let one_style = shot(&mut r, RenderMode::Textured, compiled, "lm_style_off");
    close(px(&one_style, 100, 100)[0], srgb(gray * 0.4), 3.0, "style 10 off");
}

#[test]
fn models_use_their_tangents() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let up = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[200u8, 200, 200, 255].repeat(16)],
    };
    let t = r.create_texture(&up);
    let n = normal_map(&mut r, encode([0.7, 0.0, 0.7]), encode([-0.7, 0.0, 0.7]));
    let desc = MaterialDesc { texture: Some(t), tex_size: [1.0, 1.0], ..Default::default() };
    r.set_material(PROP, &desc);
    let normal = [0.0, -1.0, 0.0];
    let v = |x: f32, z: f32| Vertex {
        pos: [x, 100.0, z],
        normal,
        uv: [(x + 90.0) / 180.0, (90.0 - z) / 180.0],
        alpha: 1.0,
    };
    let vertices = vec![v(-90.0, -90.0), v(90.0, -90.0), v(90.0, 90.0), v(-90.0, 90.0)];
    let indices = vec![0, 1, 2, 0, 2, 3];
    let computed = r.create_model(&[ModelPart {
        material: PROP,
        vertices: vertices.clone(),
        indices: indices.clone(),
        tangents: Vec::new(),
    }]);
    let given =
        r.create_model(&[ModelPart { material: PROP, vertices, indices, tangents: vec![[1.0, 0.0, 0.0, -1.0]; 4] }]);
    let instance = |model| ModelInstance { model, object: 50, ..Default::default() };
    r.set_model_instances(&[instance(computed)]);
    let flat = shot(&mut r, RenderMode::Textured, on(), "model_flat");
    r.set_material(PROP, &MaterialDesc { bump: Some(BumpDesc::new(n)), ..desc });
    let a = shot(&mut r, RenderMode::Textured, on(), "model_computed");
    r.set_model_instances(&[instance(given)]);
    let b = shot(&mut r, RenderMode::Textured, on(), "model_given");
    for img in [&a, &b] {
        assert!(px(img, 50, 100)[0] > px(&flat, 50, 100)[0] + 10, "toward +u");
        assert!(px(img, 150, 100)[0] + 10 < px(&flat, 150, 100)[0], "toward -u");
    }
    for x in [50, 150] {
        assert!(px(&a, x, 100)[0].abs_diff(px(&b, x, 100)[0]) <= 2, "given and computed tangents agree");
    }
    r.set_compiled_bloom(false);
    r.set_compiled_lighting(CompiledLightMode::Hdr, &[], &[]);
    let mut ambient = [[0.0f32; 3]; 6];
    ambient[0] = [1.0; 3];
    ambient[3] = [0.3; 3];
    r.set_model_lighting(&[ModelLighting { object: 50, ambient, ..Default::default() }]);
    let compiled = ViewOptions { compiled_lighting: true, normal_maps: true, ..ViewOptions::default() };
    let lit = shot(&mut r, RenderMode::Textured, compiled, "model_compiled");
    let albedo = linear(200);
    close(px(&lit, 50, 100)[0], srgb(albedo * 0.65), 4.0, "ambient along the bumped normal");
    close(px(&lit, 150, 100)[0], srgb(albedo * 0.15), 4.0, "the -X side is dark");
    let flat_lit =
        shot(&mut r, RenderMode::Textured, ViewOptions { normal_maps: false, ..compiled }, "model_compiled_off");
    close(px(&flat_lit, 50, 100)[0], srgb(albedo * 0.3), 3.0, "flat: the -Y side");
}

fn lm_texel(v: f32) -> u32 {
    let enc = |v: f32, m: u32| -> u32 {
        if v <= 0.0 {
            return 0;
        }
        let bits = v.to_bits();
        let e = ((bits >> 23) & 0xff) as i32 - 127 + 15;
        let mant = (bits & 0x7f_ffff) >> (23 - m);
        ((e.clamp(1, 30) as u32) << m) | mant
    };
    enc(v, 6) | (enc(v, 6) << 11) | (enc(v, 5) << 22)
}

fn wall_lightmap(r: &mut Renderer, light: &[f32], bumped: bool, blend: u32) {
    use hp_render::lightmap::{LightmapBatch, LightmapFace, LightmapRect, LightmapVertex};
    let generation = r.lightmap_generation(1);
    r.add_lightmap_faces(&[(0, LightmapFace { object: 1, generation, scale: 16.0, flags: 0 })]);
    let verts: Vec<LightmapVertex> = wall_vertices()
        .iter()
        .map(|v| LightmapVertex { pos: v.pos, uv: v.uv, lm: [0.0, 0.0], alpha: 0.0, face: 0, blend })
        .collect();
    r.add_lightmap_block(1, &[LightmapBatch { material: WALL, vertices: &verts, indices: &[0, 1, 2, 0, 2, 3] }]);
    assert!(r.ensure_lightmap_layers(64, 1));
    let texels: Vec<u32> = light.iter().map(|v| lm_texel(*v)).collect();
    r.write_lightmap(0, LightmapRect { layer: 0, x: 2, y: 3, w: 1, h: 1, density: 1.0, bumped }, &texels);
    r.set_lightmap_display(hp_render::lightmap::LightmapDisplay::Baked, 1.0);
}

const BUMPED_LIGHT: [f32; 4] = [0.25, 1.0, 0.5, 0.0625];

#[test]
fn baked_preview_has_the_relief_of_its_bumped_lightmaps() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    wall_material(&mut r, None);
    r.upsert_object(1, wall_mesh());
    wall_lightmap(&mut r, &BUMPED_LIGHT, true, 0);
    let gray = linear(128);
    let flat = shot(&mut r, RenderMode::Textured, on(), "baked_flat");
    close(px(&flat, 100, 100)[0], srgb(gray * 0.25), 2.0, "flat lightmap");
    let up = normal_map(&mut r, [128, 128, 255], [128, 128, 255]);
    wall_material(&mut r, Some(BumpDesc::new(up)));
    let avg = shot(&mut r, RenderMode::Textured, on(), "baked_average");
    close(px(&avg, 100, 100)[0], srgb(gray * (1.0 + 0.5 + 0.0625) / 3.0), 3.0, "averaged bump lightmaps");
    let b0 = encode([0.816_496_6, 0.0, 0.577_350_3]);
    let b1 = encode([-0.408_248_3, std::f32::consts::FRAC_1_SQRT_2, 0.577_350_3]);
    let halves = normal_map(&mut r, b0, b1);
    wall_material(&mut r, Some(BumpDesc::new(halves)));
    let relief = shot(&mut r, RenderMode::Textured, on(), "baked_relief");
    close(px(&relief, 40, 100)[0], srgb(gray), 4.0, "first bump lightmap");
    close(px(&relief, 160, 100)[0], srgb(gray * 0.5), 4.0, "second bump lightmap");
    let ss = normal_map(&mut r, [128, 0, 255], [128, 0, 255]);
    wall_material(&mut r, Some(BumpDesc { ssbump: true, ..BumpDesc::new(ss) }));
    let ssb = shot(&mut r, RenderMode::Textured, on(), "baked_ssbump");
    close(px(&ssb, 100, 100)[0], srgb(gray * ((128.0 / 255.0) + 0.0625)), 4.0, "ssbump weights");
    wall_material(&mut r, Some(BumpDesc::new(halves)));
    let off = shot(&mut r, RenderMode::Textured, ViewOptions::default(), "baked_off");
    for x in [40, 160] {
        close(px(&off, x, 100)[0], srgb(gray * 0.25), 2.0, "normal maps off");
    }
    assert_ne!(px(&off, 40, 100), px(&relief, 40, 100), "the setting changes the image");
    wall_lightmap(&mut r, &BUMPED_LIGHT[..1], false, 0);
    let unbumped = shot(&mut r, RenderMode::Textured, on(), "baked_unbumped");
    for x in [40, 160] {
        close(px(&unbumped, x, 100)[0], srgb(gray * 0.25), 2.0, "no bumped lightmaps");
    }
}

fn four_way_material(r: &mut Renderer, bump: TexId, bump2: TexId, factors: [f32; 4]) {
    let up = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[128u8, 128, 128, 255].repeat(16)],
    };
    let t = r.create_texture(&up);
    let desc = MaterialDesc {
        texture: Some(t),
        texture2: Some(t),
        texture3: Some(t),
        texture4: Some(t),
        tex_size: [200.0, 200.0],
        bump: Some(BumpDesc { texture2: Some(bump2), blend4: Some(factors), ..BumpDesc::new(bump) }),
        ..Default::default()
    };
    r.set_material(WALL, &desc);
}

fn blended_wall(weights: [f32; 4]) -> ObjectMesh {
    let mut mesh = wall_mesh();
    mesh.batches[0].blend = vec![BlendVertex::painted(weights); 4];
    mesh
}

#[test]
fn four_way_blends_mix_their_normal_maps_like_their_textures() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let toward = normal_map(&mut r, encode([0.7, 0.0, 0.7]), encode([0.7, 0.0, 0.7]));
    let away = normal_map(&mut r, encode([-0.7, 0.0, 0.7]), encode([-0.7, 0.0, 0.7]));
    let defaults = [0.0, 1.0, 0.0, 0.0];
    wall_material(&mut r, None);
    r.upsert_object(1, blended_wall([1.0, 0.0, 0.0, 0.0]));
    let plain = px(&shot(&mut r, RenderMode::Textured, on(), "four_plain"), 100, 100)[0];
    four_way_material(&mut r, toward, away, defaults);
    let one = px(&shot(&mut r, RenderMode::Textured, on(), "four_texture1"), 100, 100)[0];
    assert!(one > plain + 10, "texture 1 has $bumpmap: {one} vs {plain}");
    r.upsert_object(1, blended_wall([0.0, 1.0, 0.0, 0.0]));
    let two = px(&shot(&mut r, RenderMode::Textured, on(), "four_texture2"), 100, 100)[0];
    assert!(two + 10 < plain, "texture 2 has $bumpmap2: {two} vs {plain}");
    r.upsert_object(1, blended_wall([0.0, 0.0, 1.0, 0.0]));
    let three = px(&shot(&mut r, RenderMode::Textured, on(), "four_texture3"), 100, 100)[0];
    assert!(three.abs_diff(one) <= 2, "texture 3 has $bumpmap: {three} vs {one}");
    four_way_material(&mut r, toward, away, [0.0, 1.0, 1.0, 0.0]);
    let three2 = px(&shot(&mut r, RenderMode::Textured, on(), "four_texture3_bump2"), 100, 100)[0];
    assert!(three2.abs_diff(two) <= 2, "texture 3 with factor 1: {three2} vs {two}");
    four_way_material(&mut r, toward, away, defaults);
    r.upsert_object(1, blended_wall([0.8, 0.2, 0.0, 0.0]));
    let half = px(&shot(&mut r, RenderMode::Textured, on(), "four_half"), 100, 100)[0];
    assert!(half < one && half > two, "mixed: {two} < {half} < {one}");
    let off = px(&shot(&mut r, RenderMode::Textured, ViewOptions::default(), "four_off"), 100, 100)[0];
    assert!(off.abs_diff(plain) <= 2, "off: {off} vs {plain}");

    let b0 = encode([0.816_496_6, 0.0, 0.577_350_3]);
    let b1 = encode([-0.408_248_3, std::f32::consts::FRAC_1_SQRT_2, 0.577_350_3]);
    let (n0, n1) = (normal_map(&mut r, b0, b0), normal_map(&mut r, b1, b1));
    four_way_material(&mut r, n0, n1, defaults);
    r.upsert_object(1, blended_wall([1.0, 0.0, 0.0, 0.0]));
    let gray = linear(128);
    let pack = hp_render::lightmap::pack_blend;
    wall_lightmap(&mut r, &BUMPED_LIGHT, true, pack([1.0, 0.0, 0.0, 0.0]));
    let first = shot(&mut r, RenderMode::Textured, on(), "four_baked_texture1");
    close(px(&first, 100, 100)[0], srgb(gray), 4.0, "baked texture 1: $bumpmap");
    wall_lightmap(&mut r, &BUMPED_LIGHT, true, pack([0.0, 1.0, 0.0, 0.0]));
    let second = shot(&mut r, RenderMode::Textured, on(), "four_baked_texture2");
    close(px(&second, 100, 100)[0], srgb(gray * 0.5), 4.0, "baked texture 2: $bumpmap2");
}
