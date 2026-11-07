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
            label: Some("hp-render water test"),
            required_features: features,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

macro_rules! gpu_or_skip {
    () => {
        match gpu() {
            Some(g) => g,
            None => {
                eprintln!("no GPU adapter available: test skipped");
                return;
            }
        }
    };
}

type Img = (u32, u32, Vec<u8>);

fn grab(r: &Renderer, view: u32, name: &str) -> Img {
    let (w, h, px) = r.read_view_rgba8(view).expect("readback");
    if let Some(dir) = std::env::var_os("HP_TEST_SCRATCH") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        image::save_buffer(dir.join(format!("water_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
    }
    (w, h, px)
}

fn pixel(img: &Img, x: u32, y: u32) -> [i32; 3] {
    let o = ((y * img.0 + x) * 4) as usize;
    [i32::from(img.2[o]), i32::from(img.2[o + 1]), i32::from(img.2[o + 2])]
}

fn dist(a: [i32; 3], b: [i32; 3]) -> i32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).sum()
}

fn solid(rgba: [u8; 4]) -> TextureUpload {
    TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![rgba.repeat(16)] }
}

fn box_mesh(min: [f32; 3], max: [f32; 3], top: MaterialId, rest: MaterialId) -> ObjectMesh {
    let c = |i: usize| -> [f32; 3] {
        [
            if i & 1 != 0 { max[0] } else { min[0] },
            if i & 2 != 0 { max[1] } else { min[1] },
            if i & 4 != 0 { max[2] } else { min[2] },
        ]
    };
    let faces: [([usize; 4], [f32; 3]); 6] = [
        ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
        ([0, 2, 3, 1], [0.0, 0.0, -1.0]),
        ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
        ([0, 4, 6, 2], [-1.0, 0.0, 0.0]),
        ([2, 6, 7, 3], [0.0, 1.0, 0.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
    ];
    let mut batches: Vec<MeshBatch> = Vec::new();
    for (k, (corners, n)) in faces.into_iter().enumerate() {
        let material = if k == 0 { top } else { rest };
        let mut b = MeshBatch { material, ..Default::default() };
        for i in corners {
            let p = c(i);
            b.vertices.push(Vertex { pos: p, normal: n, uv: [p[0], -p[1]], alpha: 1.0 });
        }
        b.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        batches.push(b);
    }
    let pairs = [(0, 1), (1, 3), (3, 2), (2, 0), (4, 5), (5, 7), (7, 6), (6, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
    ObjectMesh {
        batches,
        edges: pairs.iter().flat_map(|&(a, b)| [c(a), c(b)]).collect(),
        aabb_min: min,
        aabb_max: max,
        wire_color: [255, 255, 255],
        ..Default::default()
    }
}

fn persp(eye: [f32; 3], target: [f32; 3], msaa: bool) -> ViewParams {
    let f = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
    ViewParams {
        game_pixel_grid: false,
        size: [256, 192],
        kind: ViewKind::Perspective { eye, forward: f, up: [0.0, 0.0, 1.0], fov_y_deg: 70.0, near: 1.0, far: 30000.0 },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: true,
        show_models: true,
        msaa,
    }
}

const BED: MaterialId = 1;
const WATER: MaterialId = 2;
const HIDDEN: MaterialId = 3;
const WALL: MaterialId = 4;
const GLASS: MaterialId = 5;

const BED_RGB: [u8; 3] = [200, 170, 90];
const FOG_RGB: [u8; 3] = [10, 60, 70];
const SKY_RGB: [u8; 3] = [60, 120, 255];
const TOOL_RGB: [u8; 3] = [255, 0, 255];
const WALL_RGB: [u8; 3] = [230, 30, 30];

fn as3(c: [u8; 3]) -> [i32; 3] {
    c.map(i32::from)
}

fn water_desc(normal: TexId) -> WaterDesc {
    let f = |c: [u8; 3]| c.map(|v| f32::from(v) / 255.0);
    WaterDesc {
        normal_map: Some(normal),
        refract_amount: 1.0,
        reflect_amount: 0.4,
        fog_color: f(FOG_RGB),
        fog_range: [0.0, 512.0],
        bump_scroll: [0.02, 0.01],
        bottom: Some(WaterBottom {
            refract_tint: [1.0; 3],
            refract_amount: 1.0,
            fog_color: f(FOG_RGB),
            fog_range: [0.0, 1024.0],
        }),
        ..Default::default()
    }
}

fn scene(r: &mut Renderer) {
    let tex = |r: &mut Renderer, c: [u8; 3]| r.create_texture(&solid([c[0], c[1], c[2], 255]));
    let bed = tex(r, BED_RGB);
    let tool = tex(r, TOOL_RGB);
    let wall = tex(r, WALL_RGB);
    let sky = tex(r, SKY_RGB);
    let normal = r.create_texture(&solid([128, 128, 255, 255]));
    let glass = r.create_texture(&solid([255, 255, 255, 120]));
    r.set_material(
        BED,
        &MaterialDesc { texture: Some(bed), tex_size: [4.0, 4.0], flags: MaterialFlags::UNLIT, ..Default::default() },
    );
    r.set_material(
        WALL,
        &MaterialDesc { texture: Some(wall), tex_size: [4.0, 4.0], flags: MaterialFlags::UNLIT, ..Default::default() },
    );
    r.set_material(
        GLASS,
        &MaterialDesc {
            texture: Some(glass),
            tex_size: [4.0, 4.0],
            flags: MaterialFlags::UNLIT | MaterialFlags::TRANSLUCENT,
            ..Default::default()
        },
    );
    r.set_material(HIDDEN, &MaterialDesc { flags: MaterialFlags::NODRAW_IN_3D, ..Default::default() });
    r.set_material(
        WATER,
        &MaterialDesc {
            texture: Some(tool),
            tex_size: [256.0, 256.0],
            flags: MaterialFlags::UNLIT | MaterialFlags::OPAQUE_TEXTURE,
            water: Some(water_desc(normal)),
            ..Default::default()
        },
    );
    for i in 0..8 {
        let x0 = -512.0 + i as f32 * 128.0;
        let depth = 32.0 + i as f32 * 64.0;
        r.upsert_object(10 + i, box_mesh([x0, -512.0, -depth - 16.0], [x0 + 128.0, 512.0, -depth], BED, BED));
    }
    r.upsert_object(1, box_mesh([-512.0, -512.0, -512.0], [512.0, 512.0, 0.0], WATER, HIDDEN));
    r.upsert_object(2, box_mesh([-512.0, 600.0, -64.0], [512.0, 616.0, 400.0], WALL, WALL));
    r.set_environment(Environment { sky: Some(SkyView::new([Some(sky); 6], 0.0)), ..Default::default() });
}

fn opts(water: bool) -> ViewOptions {
    ViewOptions { water_preview: water, show_sky: true, time: 3.0, ..Default::default() }
}

#[test]
fn water_normal_rotation_matches_texels_baked_with_the_native_material_matrix() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    scene(&mut r);
    let report: serde_json::Value =
        serde_json::from_str(include_str!("../../hp-vmt/tests/data/native_texture_transform.json")).unwrap();
    let case = &report["cases"][29];
    let matrix: [[f32; 3]; 2] = std::array::from_fn(|row| {
        [0, 1, 3].map(|column| case["matrix"][row][column].as_str().unwrap().parse().unwrap())
    });
    let source = hp_vmt::TextureTransform::parse(case["input"].as_str().unwrap()).matrix();
    let normal: Vec<u8> = (0..16)
        .flat_map(|i| {
            let x = ((i % 4) as f32 - 1.5) * 0.35;
            let y = ((i / 4) as f32 - 1.5) * 0.35;
            [x, y, (1.0 - x * x - y * y).sqrt()]
                .map(|v| ((v * 0.5 + 0.5) * 255.0).round() as u8)
                .into_iter()
                .chain([255])
        })
        .collect();
    let mut baked = Vec::new();
    for y in 0..8 {
        for x in 0..4 {
            let uv = [(x as f32 + 0.5) / 4.0, (y as f32 + 0.5) / 8.0];
            let [u, v] = matrix.map(|row| row[0] * uv[0] + row[1] * uv[1] + row[2]);
            let at =
                (((v * 4.0).floor() as i32).rem_euclid(4) * 4 + ((u * 4.0).floor() as i32).rem_euclid(4)) as usize * 4;
            baked.extend_from_slice(&normal[at..at + 4]);
        }
    }
    let normal =
        r.create_texture(&TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Linear, levels: vec![normal] });
    let baked =
        r.create_texture(&TextureUpload { width: 4, height: 8, format: TexFormat::Rgba8Linear, levels: vec![baked] });
    let checker: Vec<u8> =
        (0..16).flat_map(|i| if (i % 4 + i / 4) % 2 == 0 { [240, 40, 30, 255] } else { [30, 220, 70, 255] }).collect();
    let checker =
        r.create_texture(&TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![checker] });
    r.set_material(
        BED,
        &MaterialDesc {
            texture: Some(checker),
            tex_size: [128.0; 2],
            flags: MaterialFlags::UNLIT,
            ..Default::default()
        },
    );
    let mut water = WaterDesc {
        normal_map: Some(normal),
        bump_uv: [source[0][0], source[1][1], source[0][2], source[1][2]],
        bump_uv_cross: [source[0][1], source[1][0]],
        bump_scroll: [0.0; 2],
        fog_range: [0.0, 4096.0],
        ..water_desc(normal)
    };
    let desc = |water| MaterialDesc {
        texture: Some(TexId::WHITE),
        tex_size: [256.0; 2],
        flags: MaterialFlags::UNLIT | MaterialFlags::OPAQUE_TEXTURE,
        water: Some(water),
        ..Default::default()
    };
    let view = persp([0.0, -700.0, 400.0], [0.0, 0.0, -100.0], false);
    r.set_view_options(1, ViewOptions { time: 0.0, ..opts(true) });
    r.set_material(WATER, &desc(water));
    r.render_view(1, &view, &Overlay::default());
    let actual = grab(&r, 1, "affine_native_normal");
    water.bump_uv = [1.0, 1.0, 0.0, 0.0];
    water.bump_uv_cross = [0.0; 2];
    water.normal_map = Some(baked);
    r.set_material(WATER, &desc(water));
    r.render_view(1, &view, &Overlay::default());
    let expected = grab(&r, 1, "affine_native_baked");
    let mut bad = 0;
    for y in 60..180 {
        for x in 20..236 {
            if dist(pixel(&actual, x, y), pixel(&expected, x, y)) > 6 {
                bad += 1;
            }
        }
    }
    assert!(bad <= 12, "native matrix and pre-mapped normal texels disagree at {bad} water pixels");
    water.normal_map = Some(normal);
    r.set_material(WATER, &desc(water));
    r.render_view(1, &view, &Overlay::default());
    let unchanged = grab(&r, 1, "affine_without_rotation");
    let changed = (60..180)
        .flat_map(|y| (20..236).map(move |x| (x, y)))
        .filter(|&(x, y)| dist(pixel(&actual, x, y), pixel(&unchanged, x, y)) > 12)
        .count();
    assert!(changed > 100, "the normal transform must visibly affect the water: {changed} changed pixels");
}

#[test]
fn water_refracts_fogs_and_reflects() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    scene(&mut r);
    let down = persp([0.0, -700.0, 400.0], [0.0, 0.0, -100.0], false);

    r.set_view_options(1, opts(false));
    r.render_view(1, &down, &Overlay::default());
    let off = grab(&r, 1, "off");
    let center = pixel(&off, 128, 110);
    assert!(dist(center, as3(TOOL_RGB)) < 40, "preview off: the tool texture {center:?}");
    assert!(!r.view_shows_animation(1), "the tool texture does not move");

    r.set_view_options(1, opts(true));
    r.render_view(1, &down, &Overlay::default());
    let on = grab(&r, 1, "on");
    let center = pixel(&on, 128, 110);
    assert!(dist(center, as3(TOOL_RGB)) > 200, "preview on: no tool texture {center:?}");
    let shallow = pixel(&on, 40, 120);
    let deep = pixel(&on, 216, 120);
    assert!(dist(shallow, as3(BED_RGB)) < dist(deep, as3(BED_RGB)), "shallow {shallow:?} deep {deep:?}");
    assert!(dist(deep, as3(FOG_RGB)) < dist(shallow, as3(FOG_RGB)), "shallow {shallow:?} deep {deep:?}");
    assert!(dist(deep, as3(FOG_RGB)) < 30, "deep water shows its fog and nothing of the bed {deep:?}");
    assert!(r.view_shows_animation(1), "the normal map scrolls");

    let grazing = persp([0.0, -400.0, 24.0], [0.0, 600.0, 40.0], false);
    r.render_view(2, &grazing, &Overlay::default());
    r.set_view_options(2, opts(true));
    r.render_view(2, &grazing, &Overlay::default());
    let g = grab(&r, 2, "grazing");
    let mut reds = 0;
    for y in 100..190 {
        let p = pixel(&g, 128, y);
        if p[0] > p[1] + 60 && p[0] > p[2] + 60 {
            reds += 1;
        }
    }
    assert!(reds > 5, "the wall is reflected under itself: {reds} red rows");
    let counts = r.view_counts(2).expect("counts");
    assert!(counts.draw_calls > 0);
    assert!(r.frame_times().reflection_cpu_ms > 0.0, "visible water renders its reflection");
    r.set_object_flags(1, ObjectFlags::HIDDEN);
    r.render_view(2, &grazing, &Overlay::default());
    assert_eq!(r.frame_times().reflection_cpu_ms, 0.0, "hidden water renders no reflection");
    assert_eq!(
        r.frame_times().reflection_gpu_ms,
        0.0,
        "a previous GPU sample is not reported for an absent reflection"
    );
    r.set_object_flags(1, ObjectFlags::empty());
    let away = persp([0.0, -700.0, 400.0], [0.0, -1600.0, 800.0], false);
    r.render_view(2, &away, &Overlay::default());
    assert_eq!(r.frame_times().reflection_cpu_ms, 0.0, "offscreen water renders no reflection");
}

#[test]
fn water_from_below_msaa_and_translucent_behind() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    scene(&mut r);
    r.upsert_object(3, box_mesh([-64.0, -64.0, -200.0], [64.0, 64.0, -190.0], GLASS, GLASS));
    let down = persp([0.0, -700.0, 400.0], [0.0, 0.0, -100.0], true);
    r.set_view_options(1, opts(true));
    r.render_view(1, &down, &Overlay::default());
    let img = grab(&r, 1, "msaa");
    let c = pixel(&img, 128, 110);
    assert!(dist(c, [255, 255, 255]) > 150, "the pane under the water stays under it {c:?}");

    let under = persp([0.0, -300.0, -40.0], [0.0, 600.0, -40.0], true);
    r.set_view_options(3, opts(true));
    r.render_view(3, &under, &Overlay::default());
    let u = grab(&r, 3, "under");
    let mid = pixel(&u, 128, 120);
    assert!(dist(mid, as3(FOG_RGB)) < 140, "under water the view is fogged {mid:?}");
    let up = persp([0.0, 0.0, -100.0], [0.0, 1.0, 400.0], true);
    r.set_view_options(4, opts(true));
    r.render_view(4, &up, &Overlay::default());
    let u = grab(&r, 4, "up");
    let top = pixel(&u, 128, 60);
    assert!(top[2] > top[0] + 20, "the sky through the surface, bluish {top:?}");

    r.set_view_options(1, opts(false));
    r.render_view(1, &down, &Overlay::default());
    let off = grab(&r, 1, "msaa_off");
    assert!(dist(pixel(&off, 128, 110), as3(TOOL_RGB)) < 60);
    let s = r.stats();
    assert!(s.approx_gpu_bytes > 0);
    r.remove_view(3);
}

#[test]
fn cheap_water_and_hidden_surfaces() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    scene(&mut r);
    let normal = r.create_texture(&solid([128, 128, 255, 255]));
    let tool = r.create_texture(&solid([TOOL_RGB[0], TOOL_RGB[1], TOOL_RGB[2], 255]));
    let cheap = WaterDesc { reflect: false, cheap: true, cheap_blend: false, ..water_desc(normal) };
    r.set_material(
        WATER,
        &MaterialDesc {
            texture: Some(tool),
            tex_size: [256.0, 256.0],
            flags: MaterialFlags::UNLIT,
            water: Some(cheap),
            ..Default::default()
        },
    );
    let down = persp([0.0, -700.0, 400.0], [0.0, 0.0, -100.0], false);
    r.set_view_options(1, opts(true));
    r.render_view(1, &down, &Overlay::default());
    let img = grab(&r, 1, "cheap");
    let c = pixel(&img, 40, 120);
    assert!(dist(c, as3(BED_RGB)) > 150, "cheap opaque water hides the bed {c:?}");
    r.set_object_flags(1, ObjectFlags::HIDDEN);
    r.render_view(1, &down, &Overlay::default());
    let img = grab(&r, 1, "hidden");
    let c = pixel(&img, 40, 120);
    assert!(dist(c, as3(BED_RGB)) < 60, "hidden water {c:?}");
    let mut wire = down;
    wire.mode = RenderMode::Flat;
    r.set_object_flags(1, ObjectFlags::empty());
    r.render_view(1, &wire, &Overlay::default());
    let _ = grab(&r, 1, "flat");
}

#[test]
fn compiled_water_draws_the_face_toward_the_eye() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    scene(&mut r);
    let down = persp([0.0, -700.0, 400.0], [0.0, 0.0, -100.0], false);
    r.set_view_options(1, opts(true));
    r.render_view(1, &down, &Overlay::default());
    let alone = pixel(&grab(&r, 1, "compiled_alone"), 128, 110);
    const BENEATH: MaterialId = 6;
    let normal = r.create_texture(&solid([128, 128, 255, 255]));
    let green = WaterDesc {
        above_water: false,
        reflect: false,
        fog_color: [0.0, 1.0, 0.0],
        fog_range: [0.0, 1.0],
        bottom: None,
        ..water_desc(normal)
    };
    r.set_material(BENEATH, &MaterialDesc { tex_size: [256.0, 256.0], water: Some(green), ..Default::default() });
    let mut under = MeshBatch { material: BENEATH, ..Default::default() };
    for p in [[-512.0, -512.0], [-512.0, 512.0], [512.0, 512.0], [512.0, -512.0]] {
        under.vertices.push(Vertex { pos: [p[0], p[1], 0.0], normal: [0.0, 0.0, -1.0], uv: [p[0], -p[1]], alpha: 1.0 });
    }
    under.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    r.upsert_object(
        5,
        ObjectMesh {
            batches: vec![under],
            aabb_min: [-512.0, -512.0, 0.0],
            aabb_max: [512.0, 512.0, 0.0],
            wire_color: [255, 255, 255],
            ..Default::default()
        },
    );
    for id in [1, 5] {
        r.set_object_flags(id, ObjectFlags::COMPILED_LIGHT);
    }
    r.render_view(1, &down, &Overlay::default());
    let both = pixel(&grab(&r, 1, "compiled_both"), 128, 110);
    assert!(dist(both, alone) < 30, "from above, the surface as before: {both:?} {alone:?}");
    assert!(both[1] < both[0] + 120, "not the green face under it {both:?}");
    let up = persp([0.0, 0.0, -100.0], [0.0, 1.0, 400.0], false);
    r.set_view_options(2, opts(true));
    r.render_view(2, &up, &Overlay::default());
    let top = pixel(&grab(&r, 2, "compiled_below"), 128, 60);
    assert!(top[1] > top[0] + 40 && top[1] > top[2], "the face under the surface from below {top:?}");
}
