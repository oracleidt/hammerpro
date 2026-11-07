use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

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
        let caps = adapter.get_downlevel_capabilities().flags;
        if !caps.contains(wgpu::DownlevelFlags::VERTEX_STORAGE)
            || !caps.contains(wgpu::DownlevelFlags::CUBE_ARRAY_TEXTURES)
        {
            return None;
        }
        let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hp-render cubemap test"),
            required_features: features,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

type Img = (u32, u32, Vec<u8>);

const SIZE: u32 = 160;
const H: f32 = 200.0;
const MIRROR: MaterialId = 100;

fn save(r: &Renderer, name: &str) -> Img {
    let (w, h, px) = r.read_view_rgba8(1).expect("readback");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hp-render-scratch");
    let _ = std::fs::create_dir_all(&dir);
    image::save_buffer(dir.join(format!("cube_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
    (w, h, px)
}

fn px(img: &Img, x: u32, y: u32) -> [u8; 3] {
    let o = ((y * img.0 + x) * 4) as usize;
    [img.2[o], img.2[o + 1], img.2[o + 2]]
}

fn color(r: &mut Renderer, id: MaterialId, rgb: [f32; 3]) {
    r.set_material(
        id,
        &MaterialDesc {
            texture: Some(TexId::WHITE),
            tex_size: [1.0, 1.0],
            flags: MaterialFlags::UNLIT,
            tint: [rgb[0], rgb[1], rgb[2], 1.0],
            ..Default::default()
        },
    );
}

fn quad(batches: &mut Vec<MeshBatch>, material: MaterialId, c: [[f32; 3]; 4], normal: [f32; 3], cube: u16) {
    let vertices: Vec<Vertex> = c.iter().map(|p| Vertex { pos: *p, normal, uv: [p[0], p[1]], alpha: 0.0 }).collect();
    batches.push(MeshBatch {
        material,
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
        blend: Vec::new(),
        cubemaps: vec![cube; 4],
    });
}

fn room(r: &mut Renderer, floor: MaterialId, cube: u16) {
    let colors: [(MaterialId, [f32; 3]); 7] = [
        (1, [1.0, 0.0, 0.0]),
        (2, [1.0, 0.0, 1.0]),
        (3, [0.0, 1.0, 0.0]),
        (4, [0.0, 0.0, 1.0]),
        (5, [1.0, 1.0, 0.0]),
        (6, [1.0, 1.0, 1.0]),
        (7, [0.0, 1.0, 1.0]),
    ];
    for (id, c) in colors {
        color(r, id, c);
    }
    let mut b = Vec::new();
    let n = NO_CUBEMAP;
    quad(&mut b, 1, [[H, -H, 0.0], [H, H, 0.0], [H, H, H], [H, -H, H]], [-1.0, 0.0, 0.0], n);
    quad(&mut b, 2, [[H, -H, -H], [H, H, -H], [H, H, 0.0], [H, -H, 0.0]], [-1.0, 0.0, 0.0], n);
    quad(&mut b, 3, [[-H, -H, -H], [-H, H, -H], [-H, H, H], [-H, -H, H]], [1.0, 0.0, 0.0], n);
    quad(&mut b, 4, [[-H, H, -H], [H, H, -H], [H, H, H], [-H, H, H]], [0.0, -1.0, 0.0], n);
    quad(&mut b, 5, [[-H, -H, -H], [H, -H, -H], [H, -H, H], [-H, -H, H]], [0.0, 1.0, 0.0], n);
    quad(&mut b, 6, [[-H, 0.0, H], [H, 0.0, H], [H, H, H], [-H, H, H]], [0.0, 0.0, -1.0], n);
    quad(&mut b, 7, [[-H, -H, H], [H, -H, H], [H, 0.0, H], [-H, 0.0, H]], [0.0, 0.0, -1.0], n);
    quad(&mut b, floor, [[-H, -H, -H], [H, -H, -H], [H, H, -H], [-H, H, -H]], [0.0, 0.0, 1.0], cube);
    r.upsert_object(
        1,
        ObjectMesh { batches: b, aabb_min: [-H; 3], aabb_max: [H; 3], wire_color: [200; 3], ..Default::default() },
    );
}

fn mirror(r: &mut Renderer, envmap: EnvmapDesc) {
    r.set_material(
        MIRROR,
        &MaterialDesc {
            texture: Some(TexId::WHITE),
            tex_size: [64.0, 64.0],
            tint: [0.0, 0.0, 0.0, 1.0],
            envmap: Some(envmap),
            ..Default::default()
        },
    );
}

fn down() -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [SIZE, SIZE],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 0.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            fov_y_deg: 60.0,
            near: 1.0,
            far: 10000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: false,
    }
}

fn shot(r: &mut Renderer, params: &ViewParams, cubemaps: bool, name: &str) -> Img {
    r.set_view_options(1, ViewOptions { cubemaps, ..ViewOptions::default() });
    r.render_view(1, params, &Overlay::default());
    save(r, name)
}

fn bake(r: &mut Renderer) {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while r.step_cubemaps(Duration::from_secs(5)) > 0 {
        assert!(std::time::Instant::now() < deadline, "the probes never finished");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn named_cubemaps_keep_the_detail_of_their_source_images() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    room(&mut r, MIRROR, NO_CUBEMAP);
    let size = 256u32;
    let face: Vec<[f32; 3]> = (0..size * size)
        .map(|i| if (i % size / 4).is_multiple_of(2) { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 1.0] })
        .collect();
    let strip: Vec<u8> = (0..6)
        .flat_map(|_| face.iter().flat_map(|c| [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]))
        .collect();
    let texture = r.create_texture(&TextureUpload {
        width: size,
        height: size * 6,
        format: TexFormat::Rgba8Srgb,
        levels: vec![strip],
    });
    mirror(&mut r, EnvmapDesc { texture: Some(texture), ..Default::default() });
    shot(&mut r, &down(), true, "named_detail_initial");
    bake(&mut r);
    let named_resolution = r.cubemap_stats().resolution;
    let named = shot(&mut r, &down(), true, "named_detail_texture");
    // Create the reference AFTER capturing the named cube: otherwise its size
    // would raise the shared atlas resolution and conceal the named-size bug.
    let image = r.create_cubemap_image(&CubemapImage { size, faces: [&face; 6], origin: None }).unwrap();
    mirror(&mut r, EnvmapDesc { cubemap: Some(image), ..Default::default() });
    bake(&mut r);
    let reference = shot(&mut r, &down(), true, "named_detail_reference");
    let mut different = 0;
    let mut min = 255;
    let mut max = 0;
    for y in 24..SIZE - 24 {
        for x in 24..SIZE - 24 {
            let a = px(&named, x, y);
            let b = px(&reference, x, y);
            min = min.min(b[0]);
            max = max.max(b[0]);
            if a.iter().zip(b).any(|(a, b)| a.abs_diff(b) > 4) {
                different += 1;
            }
        }
    }
    assert!(max - min > 100, "the reference must show the source pattern");
    assert!(
        named_resolution >= size && different < 100,
        "named cube was reduced to {named_resolution}px from {size}px; {different} interior pixels differ from the source-sized image"
    );
}

fn texel(faces: &(u32, [Vec<[f32; 4]>; 6]), face: usize, u: f32, v: f32) -> [f32; 3] {
    let size = faces.0;
    let x = ((u * size as f32) as u32).min(size - 1);
    let y = ((v * size as f32) as u32).min(size - 1);
    let t = faces.1[face][(y * size + x) as usize];
    [t[0], t[1], t[2]]
}

fn is(c: [f32; 3], want: [f32; 3], what: &str) {
    let ok = c.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.15);
    assert!(ok, "{what}: got {c:?}, want {want:?}");
}

#[test]
fn probes_are_drawn_in_the_layout_of_a_cube_vtf() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let probe =
        r.create_cubemap_probe(&CubemapProbe { origin: [0.0, 0.0, 0.0], size: 32, sides: Vec::new() }).expect("probe");
    room(&mut r, MIRROR, probe.raw());
    mirror(&mut r, EnvmapDesc::default());
    assert!(!r.cubemap_ready(probe));
    assert!(r.step_cubemaps(Duration::from_secs(1)) > 0);
    assert!(!r.cubemap_ready(probe));
    shot(&mut r, &down(), true, "first");
    bake(&mut r);
    assert!(r.cubemap_ready(probe));
    let stats = r.cubemap_stats();
    assert_eq!((stats.probes, stats.ready, stats.pending_faces), (1, 1, 0));
    assert!(stats.faces_rendered >= 12, "the sky and the probe: {stats:?}");
    let faces = r.read_cubemap(probe).expect("read");
    assert_eq!(faces.0, 64);
    let left = texel(&faces, 0, 0.25, 0.5);
    let right = texel(&faces, 0, 0.75, 0.5);
    is(left, [1.0, 0.0, 0.0], "+X face, u small: +Z (red)");
    is(right, [1.0, 0.0, 1.0], "+X face, u large: -Z (magenta)");
    is(texel(&faces, 1, 0.5, 0.5), [0.0, 1.0, 0.0], "-X face");
    is(texel(&faces, 2, 0.5, 0.5), [0.0, 0.0, 1.0], "+Y face");
    is(texel(&faces, 3, 0.5, 0.5), [1.0, 1.0, 0.0], "-Y face");
    is(texel(&faces, 4, 0.5, 0.25), [1.0, 1.0, 1.0], "+Z face, +Y half");
    is(texel(&faces, 4, 0.5, 0.75), [0.0, 1.0, 1.0], "+Z face, -Y half");
    is(texel(&faces, 5, 0.5, 0.5), [0.0, 0.0, 0.0], "-Z face: the black floor (no reflection in a probe)");

    let on = shot(&mut r, &down(), true, "on");
    let off = shot(&mut r, &down(), false, "off");
    let (top, bottom) = (px(&on, SIZE / 2, SIZE / 4), px(&on, SIZE / 2, SIZE * 3 / 4));
    assert!(top.iter().all(|c| *c > 200), "white reflected: {top:?}");
    assert!(bottom[0] < 40 && bottom[1] > 200 && bottom[2] > 200, "cyan reflected: {bottom:?}");
    assert!(px(&off, SIZE / 2, SIZE / 4).iter().all(|c| *c < 10), "no reflection with the option off");

    mirror(&mut r, EnvmapDesc { tint: [0.5; 3], ..Default::default() });
    let tinted = shot(&mut r, &down(), true, "tinted");
    let t = px(&tinted, SIZE / 2, SIZE / 4);
    assert!(t[0] > 178 && t[0] < 198, "native world tint 0.5 is linear: {t:?}");
    let black = r.create_texture(&TextureUpload {
        width: 1,
        height: 1,
        format: TexFormat::Rgba8Linear,
        levels: vec![vec![0, 0, 0, 255]],
    });
    mirror(&mut r, EnvmapDesc { mask: Some(black), ..Default::default() });
    let masked = shot(&mut r, &down(), true, "masked");
    assert!(px(&masked, SIZE / 2, SIZE / 4).iter().all(|c| *c < 10), "masked out");
    mirror(&mut r, EnvmapDesc { alpha_mask: EnvmapAlphaMask::InverseBase, ..Default::default() });
    let based = shot(&mut r, &down(), true, "base_alpha");
    assert!(px(&based, SIZE / 2, SIZE / 4).iter().all(|c| *c < 10), "base alpha mask");

    r.update_cubemap_probe(probe, &CubemapProbe { origin: [0.0, 0.0, 100.0], size: 32, sides: Vec::new() });
    assert!(r.cubemap_stats().pending_faces >= 6);
    bake(&mut r);
    r.destroy_cubemap(probe);
    mirror(&mut r, EnvmapDesc::default());
    let gone = shot(&mut r, &down(), true, "destroyed");
    assert!(px(&gone, SIZE / 2, SIZE / 4).iter().all(|c| *c < 10), "the sky, black");
}

fn faces_of(size: u32, colors: [[f32; 3]; 6]) -> Vec<Vec<[f32; 3]>> {
    colors.iter().map(|c| vec![*c; (size * size) as usize]).collect()
}

#[test]
fn hdr_images_and_cubemap_textures_are_reflected() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    room(&mut r, MIRROR, NO_CUBEMAP);
    let hdr = faces_of(8, [[0.0; 3], [0.0; 3], [0.0; 3], [0.0; 3], [2.0, 2.0, 2.0], [0.0; 3]]);
    let refs: [&[[f32; 3]]; 6] = std::array::from_fn(|i| hdr[i].as_slice());
    let id = r.create_cubemap_image(&CubemapImage { size: 8, faces: refs, origin: None }).expect("image");
    assert!(r.create_cubemap_image(&CubemapImage { size: 8, faces: [&[]; 6], origin: None }).is_none(), "malformed");
    let tint = 0.25;
    mirror(&mut r, EnvmapDesc { cubemap: Some(id), tint: [tint; 3], ..Default::default() });
    shot(&mut r, &down(), true, "hdr_first");
    bake(&mut r);
    assert!(r.cubemap_ready(id));
    let back = r.read_cubemap(id).expect("read");
    assert!((texel(&back, 4, 0.5, 0.5)[0] - 2.0).abs() < 0.01, "HDR kept above white");
    let img = shot(&mut r, &down(), true, "hdr");
    let c = px(&img, SIZE / 2, SIZE / 2);
    assert!(c[0] > 170 && c[0] < 205, "HDR reflection: {c:?}");

    let size = 4u32;
    let mut px_strip = Vec::new();
    for face in 0..6 {
        let rgb = if face == 4 { [0u8, 255, 0] } else { [255, 0, 0] };
        for _ in 0..size * size {
            px_strip.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    let strip = r.create_texture(&TextureUpload {
        width: size,
        height: size * 6,
        format: TexFormat::Rgba8Srgb,
        levels: vec![px_strip],
    });
    mirror(&mut r, EnvmapDesc { texture: Some(strip), ..Default::default() });
    bake(&mut r);
    assert_eq!(r.cubemap_stats().textures, 1);
    let green = shot(&mut r, &down(), true, "texture");
    let g = px(&green, SIZE / 2, SIZE / 2);
    assert!(g[1] > 200 && g[0] < 40, "the +Z face of the texture: {g:?}");
    r.destroy_texture(strip);
    assert_eq!(r.cubemap_stats().textures, 0);
}

#[test]
fn models_reflect_the_nearest_probe() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    assert_eq!(r.nearest_cubemap([0.0; 3]), None);
    let a = r.create_cubemap_probe(&CubemapProbe { origin: [0.0, 0.0, 0.0], ..Default::default() }).expect("a");
    let b = r.create_cubemap_probe(&CubemapProbe { origin: [1000.0, 0.0, 0.0], ..Default::default() }).expect("b");
    assert_eq!(r.nearest_cubemap([100.0, 50.0, 0.0]), Some(a));
    assert_eq!(r.nearest_cubemap([700.0, -50.0, 0.0]), Some(b));
    r.set_model_cubemaps(Some(&[b]));
    assert_eq!(r.nearest_cubemap([100.0, 50.0, 0.0]), Some(b));
    r.set_model_cubemaps(None);
    let one = faces_of(1, [[1.0; 3]; 6]);
    let refs: [&[[f32; 3]]; 6] = std::array::from_fn(|i| one[i].as_slice());
    let c = r.create_cubemap_image(&CubemapImage { size: 1, faces: refs, origin: Some([90.0, 50.0, 0.0]) }).expect("c");
    r.create_cubemap_image(&CubemapImage { size: 1, faces: refs, origin: None }).expect("d");
    assert_eq!(r.nearest_cubemap([100.0, 50.0, 0.0]), Some(c));
    assert_eq!(r.cubemap_stats().images, 2);
    r.destroy_cubemap(a);
    let e = r.create_cubemap_probe(&CubemapProbe::default()).expect("e");
    assert_eq!(e, a);
}

#[test]
fn disabling_diffuse_bump_keeps_the_normal_mapped_reflection() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    room(&mut r, MIRROR, NO_CUBEMAP);
    let faces = faces_of(
        8,
        [[1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
    );
    let cube = r
        .create_cubemap_image(&CubemapImage {
            size: 8,
            faces: std::array::from_fn(|i| faces[i].as_slice()),
            origin: None,
        })
        .unwrap();
    let normal = r.create_texture(&TextureUpload {
        width: 2,
        height: 2,
        format: TexFormat::Rgba8Linear,
        levels: vec![[255u8, 128, 128, 255].repeat(4)],
    });
    let mut capture = |bump, name| {
        r.set_material(
            MIRROR,
            &MaterialDesc {
                texture: Some(TexId::WHITE),
                tint: [0.0, 0.0, 0.0, 1.0],
                bump,
                envmap: Some(EnvmapDesc { cubemap: Some(cube), ..Default::default() }),
                ..Default::default()
            },
        );
        r.set_view_options(1, ViewOptions { cubemaps: true, normal_maps: true, ..Default::default() });
        let view = ViewParams { mode: RenderMode::Lit, ..down() };
        r.render_view(1, &view, &Overlay::default());
        bake(&mut r);
        r.render_view(1, &view, &Overlay::default());
        save(&r, name)
    };
    let flat = capture(None, "reflection_flat_normal");
    let bumped = capture(Some(BumpDesc::new(normal)), "reflection_bumped_normal");
    let reflection_only = capture(Some(BumpDesc { diffuse: false, ..BumpDesc::new(normal) }), "reflection_only_normal");
    let (f, b) = (px(&flat, SIZE / 2, SIZE / 2), px(&bumped, SIZE / 2, SIZE / 2));
    assert!(f[1] > 150 && f[0] < 30, "flat normal reflects the green ceiling: {f:?}");
    assert!(b[0] > 150 && b[1] < 30, "tilted normal changes the reflected face: {b:?}");
    assert_eq!(reflection_only.2, bumped.2, "the diffuse flag must not flatten reflections");
}

fn compiled_floor(r: &mut Renderer, cube: u16) {
    let v = |x: f32, y: f32| CompiledVertex {
        pos: [x, y, -100.0],
        normal: [0.0, 0.0, 1.0],
        uv: [x, y],
        alpha: 0.0,
        lm: [0.5, 0.5],
        lightmap: 0,
        cubemap: cube,
        blend: Default::default(),
    };
    let verts = vec![v(-400.0, -400.0), v(400.0, -400.0), v(400.0, 400.0), v(-400.0, 400.0)];
    let vertices: Vec<Vertex> =
        verts.iter().map(|v| Vertex { pos: v.pos, normal: v.normal, uv: v.uv, alpha: v.alpha }).collect();
    r.upsert_object(
        5,
        ObjectMesh {
            batches: vec![MeshBatch {
                material: MIRROR,
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: vec![cube; 4],
            }],
            aabb_min: [-400.0, -400.0, -100.0],
            aabb_max: [400.0, 400.0, -100.0],
            wire_color: [200; 3],
            ..Default::default()
        },
    );
    r.set_object_flags(5, ObjectFlags::COMPILED_LIGHT);
    let block = CompiledBlock {
        object: 5,
        batches: vec![CompiledBatch { material: MIRROR, vertices: verts, indices: vec![0, 1, 2, 0, 2, 3] }],
    };
    let black = [[0.0f32; 3]];
    let maps = [CompiledLightmap { width: 1, height: 1, texels: &black, styles: &[], bumped: false }];
    r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, std::slice::from_ref(&block));
}

#[test]
fn the_compiled_world_reflects_like_the_game() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let cube = faces_of(4, [[0.0; 3], [0.0; 3], [0.0; 3], [0.0; 3], [0.25; 3], [0.0; 3]]);
    let refs: [&[[f32; 3]]; 6] = std::array::from_fn(|i| cube[i].as_slice());
    let id = r.create_cubemap_image(&CubemapImage { size: 4, faces: refs, origin: None }).expect("image");
    mirror(&mut r, EnvmapDesc::default());
    compiled_floor(&mut r, id.raw());
    r.set_tone_settings(ToneSettings { bloom: 0.0, ..ToneSettings::default() });
    let shot_compiled = |r: &mut Renderer, cubemaps: bool, name: &str| {
        r.set_view_options(1, ViewOptions { cubemaps, compiled_lighting: true, ..ViewOptions::default() });
        r.render_view(1, &down(), &Overlay::default());
        save(r, name)
    };
    shot_compiled(&mut r, true, "compiled_first");
    bake(&mut r);
    let exposure = r.tone_scale(1);
    let hdr = shot_compiled(&mut r, true, "compiled_hdr");
    let c = px(&hdr, SIZE / 2, SIZE / 2)[0];
    let want = {
        let l = (0.25 * exposure).min(1.0);
        (1.055 * l.powf(1.0 / 2.4) - 0.055) * 255.0
    };
    assert!((f32::from(c) - want).abs() < 8.0, "HDR reflection: {c}, want {want:.0} (exposure {exposure})");
    let off = shot_compiled(&mut r, false, "compiled_off");
    assert!(px(&off, SIZE / 2, SIZE / 2)[0] < 5, "no reflection with the option off");
    let black = [[0.0f32; 3]];
    let maps = [CompiledLightmap { width: 1, height: 1, texels: &black, styles: &[], bumped: false }];
    let v = |x: f32, y: f32| CompiledVertex {
        pos: [x, y, -100.0],
        normal: [0.0, 0.0, 1.0],
        uv: [x, y],
        alpha: 0.0,
        lm: [0.5, 0.5],
        lightmap: 0,
        cubemap: id.raw(),
        blend: Default::default(),
    };
    let verts = vec![v(-400.0, -400.0), v(400.0, -400.0), v(400.0, 400.0), v(-400.0, 400.0)];
    let block = CompiledBlock {
        object: 5,
        batches: vec![CompiledBatch { material: MIRROR, vertices: verts, indices: vec![0, 1, 2, 0, 2, 3] }],
    };
    r.set_compiled_lighting(CompiledLightMode::Ldr, &maps, std::slice::from_ref(&block));
    let ldr = shot_compiled(&mut r, true, "compiled_ldr");
    let c = px(&ldr, SIZE / 2, SIZE / 2)[0];
    assert!((i32::from(c) - 137).abs() < 8, "LDR reflection: {c}");
}

#[test]
fn probes_see_the_hidden_groups_of_the_compiled_world() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    color(&mut r, 9, [1.0, 0.0, 0.0]);
    let mut b = Vec::new();
    quad(&mut b, 9, [[-H, -H, -H], [H, -H, -H], [H, H, -H], [-H, H, -H]], [0.0, 0.0, 1.0], NO_CUBEMAP);
    r.upsert_object(7, ObjectMesh { batches: b, aabb_min: [-H, -H, -H], aabb_max: [H, H, -H], ..Default::default() });
    r.set_object_flags(7, ObjectFlags::COMPILED_LIGHT | ObjectFlags::HIDDEN);
    let probe = r.create_cubemap_probe(&CubemapProbe::default()).expect("probe");
    shot(&mut r, &down(), true, "hidden_first");
    bake(&mut r);
    let faces = r.read_cubemap(probe).expect("read");
    is(texel(&faces, 5, 0.5, 0.5), [1.0, 0.0, 0.0], "the hidden compiled floor below");
    r.set_object_flags(7, ObjectFlags::HIDDEN);
    r.rebake_cubemaps();
    shot(&mut r, &down(), true, "hidden_editor");
    bake(&mut r);
    let faces = r.read_cubemap(probe).expect("read");
    is(texel(&faces, 5, 0.5, 0.5), [0.0, 0.0, 0.0], "a hidden editor floor");
}

#[test]
fn reflections_off_change_nothing() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    room(&mut r, MIRROR, NO_CUBEMAP);
    let base = r.create_texture(&TextureUpload {
        width: 2,
        height: 2,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[200u8, 60, 30, 255].repeat(4)],
    });
    let normal = r.create_texture(&TextureUpload {
        width: 2,
        height: 2,
        format: TexFormat::Rgba8Linear,
        levels: vec![[128u8, 128, 255, 100].repeat(4)],
    });
    let desc = |envmap: Option<EnvmapDesc>| MaterialDesc {
        texture: Some(base),
        tex_size: [64.0, 64.0],
        bump: Some(BumpDesc::new(normal)),
        envmap,
        ..Default::default()
    };
    r.set_material(MIRROR, &desc(None));
    let plain = shot(&mut r, &down(), false, "off_plain");
    let env = EnvmapDesc { mask: Some(normal), mask_alpha: true, ..Default::default() };
    r.set_material(MIRROR, &desc(Some(env)));
    let with = shot(&mut r, &down(), false, "off_with_envmap");
    let p = px(&plain, SIZE / 2, SIZE / 2);
    assert!(p[0] > 150, "the base texture: {p:?}");
    assert_eq!(p, px(&with, SIZE / 2, SIZE / 2));
    let on = shot(&mut r, &down(), true, "on_with_envmap");
    assert_eq!(p, px(&on, SIZE / 2, SIZE / 2));
}

#[test]
fn model_reflections_off_change_nothing() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let base = r.create_texture(&TextureUpload {
        width: 2,
        height: 2,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[200u8, 60, 30, 255].repeat(4)],
    });
    let normal = r.create_texture(&TextureUpload {
        width: 2,
        height: 2,
        format: TexFormat::Rgba8Linear,
        levels: vec![[128u8, 128, 255, 100].repeat(4)],
    });
    let desc = |envmap: Option<EnvmapDesc>| MaterialDesc {
        texture: Some(base),
        tex_size: [1.0, 1.0],
        bump: Some(BumpDesc::new(normal)),
        envmap,
        ..Default::default()
    };
    r.set_material(MIRROR, &desc(None));
    let v = |x: f32, y: f32| Vertex {
        pos: [x, y, -100.0],
        normal: [0.0, 0.0, 1.0],
        uv: [x / 100.0, y / 100.0],
        alpha: 0.0,
    };
    let model = r.create_model(&[ModelPart {
        material: MIRROR,
        vertices: vec![v(-100.0, -100.0), v(100.0, -100.0), v(100.0, 100.0), v(-100.0, 100.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    }]);
    r.set_model_instances(&[ModelInstance { model, object: 3, ..Default::default() }]);
    let plain = shot(&mut r, &down(), false, "model_plain");
    let env = EnvmapDesc { mask: Some(normal), mask_alpha: true, ..Default::default() };
    r.set_material(MIRROR, &desc(Some(env)));
    let with = shot(&mut r, &down(), false, "model_with_envmap");
    let p = px(&plain, SIZE / 2, SIZE / 2);
    assert!(p[0] > 120, "the base texture: {p:?}");
    assert_eq!(p, px(&with, SIZE / 2, SIZE / 2));
}

#[test]
fn parallax_boxes_correct_the_reflection_of_the_world() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let probe =
        r.create_cubemap_probe(&CubemapProbe { origin: [0.0, 0.0, 0.0], size: 32, sides: Vec::new() }).expect("probe");
    room(&mut r, MIRROR, probe.raw());
    mirror(&mut r, EnvmapDesc { parallax: true, ..Default::default() });
    shot(&mut r, &down(), true, "parallax_first");
    bake(&mut r);
    assert!(r.cubemap_ready(probe));
    let row = SIZE / 2 + (80.0 / (H * (30f32).to_radians().tan()) * (SIZE / 2) as f32) as u32;
    let plain = shot(&mut r, &down(), true, "parallax_none");
    let c = px(&plain, SIZE / 2, row);
    assert!(c[0] < 40 && c[1] > 200 && c[2] > 200, "without box: the ceiling (cyan): {c:?}");
    let room_box = parallax::ParallaxBox::axis_aligned(glam::Vec3::splat(-H), glam::Vec3::splat(H), glam::Vec3::ZERO);
    r.set_cubemap_parallax(probe, Some(room_box));
    assert_eq!(r.cubemap_parallax(probe), Some(room_box));
    let corrected = shot(&mut r, &down(), true, "parallax_on");
    let c = px(&corrected, SIZE / 2, row);
    assert!(c[0] > 200 && c[1] > 200 && c[2] < 40, "with the box: the -Y wall (yellow): {c:?}");
    let mid = |img: &Img| px(img, SIZE / 2, SIZE / 2 + 2);
    assert_eq!(mid(&plain), mid(&corrected), "straight up the box changes nothing");
    mirror(&mut r, EnvmapDesc::default());
    let prop = shot(&mut r, &down(), true, "parallax_prop");
    assert_eq!(px(&prop, SIZE / 2, row), px(&plain, SIZE / 2, row), "no correction without the shader's support");
    mirror(&mut r, EnvmapDesc { parallax: true, ..Default::default() });
    r.set_cubemap_parallax(probe, None);
    let again = shot(&mut r, &down(), true, "parallax_removed");
    assert_eq!(px(&again, SIZE / 2, row), px(&plain, SIZE / 2, row), "the box removed");
    r.set_cubemap_parallax(probe, Some(room_box));
    let off = shot(&mut r, &down(), false, "parallax_off");
    assert!(px(&off, SIZE / 2, row).iter().all(|c| *c < 10), "no reflection with the option off");
    r.set_cubemap_parallax(CubemapId::SKY, Some(room_box));
    assert_eq!(r.cubemap_parallax(CubemapId::SKY), None);
}
