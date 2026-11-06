use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

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
            label: Some("hp-render features test"),
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

fn save(r: &Renderer, view: u32, name: &str) -> Img {
    let (w, h, px) = r.read_view_rgba8(view).expect("readback");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-render/scratch");
    let _ = std::fs::create_dir_all(&dir);
    image::save_buffer(dir.join(format!("feat_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
    (w, h, px)
}

fn pixel(img: &Img, x: u32, y: u32) -> [u8; 4] {
    let o = ((y * img.0 + x) * 4) as usize;
    [img.2[o], img.2[o + 1], img.2[o + 2], img.2[o + 3]]
}

fn count_near(img: &Img, rgb: [u8; 3], tol: i32) -> usize {
    img.2.chunks_exact(4).filter(|p| (0..3).all(|i| (i32::from(p[i]) - i32::from(rgb[i])).abs() <= tol)).count()
}

fn solid(rgba: [u8; 4]) -> TextureUpload {
    TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![rgba.repeat(16)] }
}

fn mat(r: &mut Renderer, id: MaterialId, rgba: [u8; 4], flags: MaterialFlags) {
    let t = r.create_texture(&solid(rgba));
    r.set_material(id, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], flags, ..Default::default() });
}

fn box_mesh(min: [f32; 3], max: [f32; 3], material: MaterialId, wire: [u8; 3]) -> ObjectMesh {
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
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (corners, n) in faces {
        let s = vertices.len() as u32;
        for i in corners {
            let p = c(i);
            vertices.push(Vertex { pos: p, normal: n, uv: [p[0], p[1] + p[2]], alpha: 1.0 });
        }
        indices.extend_from_slice(&[s, s + 1, s + 2, s, s + 2, s + 3]);
    }
    let pairs = [(0, 1), (1, 3), (3, 2), (2, 0), (4, 5), (5, 7), (7, 6), (6, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
    let edges = pairs.iter().flat_map(|&(a, b)| [c(a), c(b)]).collect();
    ObjectMesh {
        batches: vec![MeshBatch { material, vertices, indices, blend: Vec::new(), cubemaps: Vec::new() }],
        edges,
        aabb_min: min,
        aabb_max: max,
        wire_color: wire,
        ..Default::default()
    }
}

fn persp(eye: [f32; 3], target: [f32; 3], size: [u32; 2], mode: RenderMode) -> ViewParams {
    let f = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
    ViewParams {
        game_pixel_grid: false,
        size,
        kind: ViewKind::Perspective { eye, forward: f, up: [0.0, 0.0, 1.0], fov_y_deg: 70.0, near: 1.0, far: 30000.0 },
        mode,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: true,
        show_models: true,
        msaa: false,
    }
}

fn top(center: [f32; 3], ppu: f32, size: [u32; 2]) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size,
        kind: ViewKind::Ortho { axis: OrthoAxis::Top, center, pixels_per_unit: ppu },
        mode: RenderMode::Wireframe,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: true,
        show_models: true,
        msaa: false,
    }
}

const GREY: MaterialId = 1;

#[test]
fn flags_tint_dim_invalid_and_edges() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [128, 128, 128, 255], MaterialFlags::empty());
    for (i, id) in [1u32, 2, 3, 4].into_iter().enumerate() {
        let x = i as f32 * 160.0;
        r.upsert_object(id, box_mesh([x, 0.0, 0.0], [x + 128.0, 128.0, 128.0], GREY, [0, 255, 0]));
    }
    r.set_object_flags(2, ObjectFlags::INVALID);
    r.set_object_flags(3, ObjectFlags::DIMMED);
    r.set_object_flags(4, ObjectFlags::INSTANCE);
    let g0 = r.content_generation();
    let p = persp([304.0, -500.0, 64.0], [304.0, 64.0, 64.0], [640, 320], RenderMode::Textured);
    r.render_view(1, &p, &Overlay::default());
    let img = save(&r, 1, "flags_3d");
    let at = |x: f32| {
        let Some(s) = p.world_to_screen([x, 0.0, 64.0]) else { panic!("visible") };
        pixel(&img, s[0] as u32, s[1] as u32)
    };
    let (plain, invalid, dimmed, instance) = (at(64.0), at(224.0), at(384.0), at(544.0));
    assert!(invalid[0] > plain[0] + 30 && invalid[1] < plain[1], "invalid is red: {invalid:?} vs {plain:?}");
    assert!(u32::from(dimmed[0]) * 10 < u32::from(plain[0]) * 6, "dimmed is darker: {dimmed:?} vs {plain:?}");
    assert!(instance[2] > instance[0] + 20, "instances are tinted blue: {instance:?}");
    assert_eq!(r.content_generation(), g0, "rendering is not a change");
    r.set_object_flags(2, ObjectFlags::INVALID);
    assert_eq!(r.content_generation(), g0, "same flags are not a change");
    r.set_object_flags(2, ObjectFlags::empty());
    assert!(r.content_generation() > g0);

    r.set_object_flags(2, ObjectFlags::INVALID);
    let t = top([304.0, 64.0, 0.0], 1.0, [701, 301]);
    r.render_view(2, &t, &Overlay::default());
    let img = save(&r, 2, "flags_2d");
    let at2 = |x: f32, y: f32| {
        let Some(s) = t.world_to_screen([x, y, 0.0]) else { panic!("2D") };
        pixel(&img, s[0] as u32, s[1] as u32)
    };
    let plain_edge = at2(64.0, 0.0);
    let invalid_edge = at2(224.0, 0.0);
    let dimmed_edge = at2(384.0, 0.0);
    assert!(plain_edge[1] > 200, "{plain_edge:?}");
    assert!(invalid_edge[0] > 200 && invalid_edge[1] < 80, "invalid edges are red: {invalid_edge:?}");
    assert!(dimmed_edge[1] < 120 && dimmed_edge[1] > 20, "dimmed edges: {dimmed_edge:?}");

    r.set_object_flags(2, ObjectFlags::empty());
    r.set_object_flags(3, ObjectFlags::empty());
    r.set_object_flags(4, ObjectFlags::empty());
    r.render_view(1, &p, &Overlay::default());
    let before = count_near(&save(&r, 1, "edges_none"), [0, 255, 0], 40);
    r.set_object_flags(1, ObjectFlags::ALWAYS_EDGES);
    r.render_view(1, &p, &Overlay::default());
    let after = count_near(&save(&r, 1, "edges_always"), [0, 255, 0], 40);
    assert!(before < 10 && after > 100, "ALWAYS_EDGES edges: {before} -> {after}");
    let wire = ViewParams { mode: RenderMode::TexturedWire, ..p };
    r.set_object_flags(1, ObjectFlags::ROPE);
    r.render_view(1, &wire, &Overlay::default());
    let rope = save(&r, 1, "edges_rope");
    let s = p.world_to_screen([0.0, 0.0, 128.0]).expect("visible corner");
    let around = (0..5)
        .flat_map(|dx| (0..5).map(move |dy| (dx, dy)))
        .filter(|(dx, dy)| {
            let px = pixel(&rope, (s[0] as u32 + dx).saturating_sub(2), (s[1] as u32 + dy).saturating_sub(2));
            px[1] > 200 && px[0] < 80
        })
        .count();
    assert_eq!(around, 0, "rope edges are hidden in the shaded 3D modes");
}

#[test]
fn tinted_objects_are_translucent() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [255, 255, 255, 255], MaterialFlags::empty());
    mat(&mut r, 2, [0, 0, 255, 255], MaterialFlags::empty());
    r.upsert_object(1, box_mesh([-256.0, 300.0, -256.0], [256.0, 320.0, 256.0], 2, [255; 3]));
    r.upsert_object(2, box_mesh([-64.0, 0.0, -64.0], [64.0, 128.0, 64.0], GREY, [255; 3]));
    let p = persp([0.0, -300.0, 0.0], [0.0, 0.0, 0.0], [256, 256], RenderMode::Unshaded);
    r.render_view(1, &p, &Overlay::default());
    let img = save(&r, 1, "tint_opaque");
    let c = pixel(&img, 128, 128);
    assert!(c[0] > 240 && c[2] > 240, "opaque white box: {c:?}");
    r.set_object_tint(2, [1.0, 0.0, 0.0, 0.5]);
    assert_eq!(r.object_tint(2).map(|t| (t[0] * 255.0) as u8), Some(255));
    r.render_view(1, &p, &Overlay::default());
    let img = save(&r, 1, "tint_half");
    let c = pixel(&img, 128, 128);
    assert!(c[0] > 120 && c[2] > 100 && c[1] < 40, "half red over blue: {c:?}");
    r.set_object_tint(2, [1.0; 4]);
    r.render_view(1, &p, &Overlay::default());
    let c = pixel(&save(&r, 1, "tint_back"), 128, 128);
    assert!(c[1] > 240, "untinted again: {c:?}");
}

#[test]
fn static_lines_thin_wide_and_hidden() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let lines: Vec<OverlayLine> = (0..20_000)
        .map(|i| {
            let y = (i % 200) as f32 * 4.0 - 400.0;
            let x = (i / 200) as f32 * 8.0 - 400.0;
            OverlayLine { a: [x, y, 0.0], b: [x + 6.0, y, 0.0], color: [1.0, 0.5, 0.0, 1.0] }
        })
        .collect();
    let g = r.content_generation();
    r.set_static_lines(7, &lines);
    assert!(r.content_generation() > g);
    assert_eq!(r.static_lines_count(7), 20_000);
    assert_eq!(r.stats().static_lines, 20_000);
    let t = top([0.0, 0.0, 0.0], 0.5, [401, 401]);
    r.render_view(1, &t, &Overlay::default());
    let thin = count_near(&save(&r, 1, "static_thin"), [255, 128, 0], 40);
    r.set_static_lines_style(7, StaticLineStyle { width_px: 4.0, ..Default::default() });
    r.render_view(1, &t, &Overlay::default());
    let wide = count_near(&save(&r, 1, "static_wide"), [255, 128, 0], 40);
    assert!(thin > 1000 && wide > thin * 2, "thin {thin}, wide {wide}");
    r.set_static_lines_style(7, StaticLineStyle { in_2d: false, ..Default::default() });
    r.render_view(1, &t, &Overlay::default());
    assert_eq!(count_near(&save(&r, 1, "static_hidden"), [255, 128, 0], 40), 0);
    r.set_static_lines(7, &lines[..10]);
    assert_eq!(r.static_lines_style(7).map(|s| s.in_2d), Some(false));
    r.clear_static_lines(7);
    assert_eq!(r.static_lines_count(7), 0);
}

#[test]
fn grid_3d_unshaded_walkable_and_fog() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [100, 100, 100, 255], MaterialFlags::empty());
    r.upsert_object(1, box_mesh([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 0.0], GREY, [255; 3]));
    let p = persp([0.0, -400.0, 300.0], [0.0, 0.0, 0.0], [320, 240], RenderMode::Textured);
    r.render_view(1, &p, &Overlay::default());
    let plain = save(&r, 1, "grid3d_off");
    r.set_view_options(1, ViewOptions { grid_3d: Some(Grid3d::default()), ..Default::default() });
    r.render_view(1, &p, &Overlay::default());
    let grid = save(&r, 1, "grid3d_on");
    let bright = |img: &Img| img.2.chunks_exact(4).filter(|p| p[0] > 160).count();
    assert!(bright(&grid) > bright(&plain) + 500, "grid lines: {} vs {}", bright(&grid), bright(&plain));

    r.set_view_options(1, ViewOptions::default());
    let un = ViewParams { mode: RenderMode::Unshaded, ..p };
    r.render_view(1, &un, &Overlay::default());
    let c = pixel(&save(&r, 1, "unshaded"), 160, 160);
    assert!((i32::from(c[0]) - 100).abs() <= 3, "unshaded keeps the texture color: {c:?}");

    r.set_environment(Environment {
        fog: Some(Fog {
            color: [1.0, 0.0, 1.0],
            color2: [1.0, 0.0, 1.0],
            dir: [1.0, 0.0, 0.0],
            blend: false,
            start: 0.0,
            end: 600.0,
            max_density: 1.0,
            far_z: 0.0,
        }),
        ..Default::default()
    });
    r.set_view_options(1, ViewOptions { show_fog: true, ..Default::default() });
    r.render_view(1, &p, &Overlay::default());
    let fog = save(&r, 1, "fog");
    let far = pixel(&fog, 160, 60);
    assert!(far[0] > 200 && far[2] > 200 && far[1] < 60, "fogged far floor: {far:?}");
    let thin = Fog {
        color: [1.0, 0.0, 1.0],
        color2: [1.0, 0.0, 1.0],
        dir: [1.0, 0.0, 0.0],
        blend: false,
        start: 0.0,
        end: 600.0,
        max_density: 0.1,
        far_z: 0.0,
    };
    r.set_environment(Environment { fog: Some(thin), ..Default::default() });
    r.render_view(1, &p, &Overlay::default());
    let c = pixel(&save(&r, 1, "fog_thin"), 160, 60);
    assert!(c[1] > 60, "a thin fog keeps the floor: {c:?}");
    r.set_environment(Environment { fog: Some(Fog { far_z: 1.0, ..thin }), ..Default::default() });
    r.render_view(1, &p, &Overlay::default());
    let c = pixel(&save(&r, 1, "fog_farz"), 160, 60);
    assert!(c[0] > 200 && c[2] > 200 && c[1] < 60, "past farz only the fog color: {c:?}");

    r.set_environment(Environment::default());
    r.upsert_object(
        2,
        ObjectMesh {
            batches: vec![MeshBatch {
                material: GREY,
                vertices: [[-100.0, -50.0, 0.0], [100.0, -50.0, 0.0], [100.0, 50.0, 150.0], [-100.0, 50.0, 150.0]]
                    .iter()
                    .map(|p| Vertex { pos: *p, normal: [0.0, -0.8, 0.6], uv: [0.0; 2], alpha: 1.0 })
                    .collect(),
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            edges: Vec::new(),
            aabb_min: [-100.0, -50.0, 0.0],
            aabb_max: [100.0, 50.0, 150.0],
            wire_color: [255; 3],
            ..Default::default()
        },
    );
    r.set_view_options(1, ViewOptions { walkable: true, ..Default::default() });
    r.render_view(1, &un, &Overlay::default());
    let walk = save(&r, 1, "walkable");
    let Some(s) = p.world_to_screen([0.0, 0.0, 75.0]) else { panic!("visible") };
    let c = pixel(&walk, s[0] as u32, s[1] as u32);
    assert!(c[0] > c[2] + 60, "steep slope tinted: {c:?}");
}

#[test]
fn sky_background_and_3d_skybox() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let colors: [[u8; 4]; 6] = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
        [255, 255, 255, 255],
        [0, 255, 255, 255],
    ];
    let faces = colors.map(|c| Some(r.create_texture(&solid(c))));
    r.set_environment(Environment { sky: Some(SkyView::new(faces, 0.0)), ..Default::default() });
    r.set_view_options(1, ViewOptions { show_sky: true, sky_see_through: true, ..Default::default() });
    let look = |dir: [f32; 3]| persp([0.0; 3], dir, [64, 64], RenderMode::Textured);
    for (dir, want, name) in [
        ([1.0, 0.0, 0.0], [255, 0, 0], "px"),
        ([0.0, 1.0, 0.0], [0, 0, 255], "py"),
        ([-1.0, 0.0, 0.0], [0, 255, 0], "nx"),
        ([0.0, -1.0, 0.0], [255, 255, 0], "ny"),
        ([0.01, 0.0, 1.0], [255, 255, 255], "pz"),
        ([0.01, 0.0, -1.0], [0, 255, 255], "nz"),
    ] {
        r.render_view(1, &look(dir), &Overlay::default());
        let c = pixel(&save(&r, 1, &format!("sky_{name}")), 32, 32);
        assert!((0..3).all(|i| (i32::from(c[i]) - want[i]).abs() < 8), "{name}: {c:?}");
    }
    mat(&mut r, 9, [10, 10, 10, 255], MaterialFlags::SKY);
    r.upsert_object(1, box_mesh([200.0, -500.0, -500.0], [220.0, 500.0, 500.0], 9, [255; 3]));
    r.render_view(1, &look([1.0, 0.0, 0.0]), &Overlay::default());
    let c = pixel(&save(&r, 1, "sky_hole"), 32, 32);
    assert!(c[0] > 240 && c[1] < 10, "sky shows through sky faces: {c:?}");

    mat(&mut r, 3, [0, 200, 0, 255], MaterialFlags::empty());
    r.upsert_object(2, box_mesh([10_000.0 + 40.0, -4.0, -4.0], [10_000.0 + 48.0, 4.0, 4.0], 3, [255; 3]));
    r.set_object_flags(2, ObjectFlags::SKY_3D);
    r.set_environment(Environment {
        sky: Some(SkyView::new(faces, 0.0)),
        sky_paint: None,
        sky_3d: Some(Sky3d { origin: [10_000.0, 0.0, 0.0], scale: 16.0 }),
        fog: None,
        sky_fog: None,
    });
    r.set_view_options(
        1,
        ViewOptions { show_sky: true, show_sky_3d: true, sky_see_through: true, ..Default::default() },
    );
    r.render_view(1, &look([1.0, 0.0, 0.0]), &Overlay::default());
    let c = pixel(&save(&r, 1, "sky_3d"), 32, 32);
    assert!(c[1] > 90 && c[0] < 60, "the skybox box is drawn around the eye: {c:?}");
}

#[test]
fn models_in_2d_fill_dots_and_icons() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [128, 128, 128, 255], MaterialFlags::empty());
    let v = |p: [f32; 3]| Vertex { pos: p, normal: [0.0, 0.0, 1.0], uv: [0.0; 2], alpha: 1.0 };
    let part = ModelPart {
        material: GREY,
        vertices: vec![v([0.0, -64.0, 0.0]), v([64.0, 0.0, 0.0]), v([0.0, 64.0, 0.0]), v([-64.0, 0.0, 0.0])],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    };
    let model = r.create_model(&[part]);
    let t = |x: f32| [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, 0.0, 0.0, 1.0]];
    r.set_model_instances(&[ModelInstance { model, transform: t(0.0), object: 50, ..Default::default() }]);
    r.set_object_wire_color(50, [255, 0, 255]);
    let view = top([0.0, 0.0, 0.0], 1.0, [201, 201]);
    let corner = |img: &Img| pixel(img, 100 + 64, 100 - 64);
    r.set_view_options(1, ViewOptions { models_2d: Models2d::Boxes, ..Default::default() });
    r.render_view(1, &view, &Overlay::default());
    assert!(corner(&save(&r, 1, "models2d_boxes"))[0] > 200);
    r.set_view_options(1, ViewOptions { models_2d: Models2d::Wireframe, ..Default::default() });
    r.render_view(1, &view, &Overlay::default());
    let wire = save(&r, 1, "models2d_wire");
    assert!(corner(&wire)[0] < 50, "no box in wireframe mode");
    assert!(pixel(&wire, 100 + 32, 100 - 32)[0] > 150, "diamond edge drawn: {:?}", pixel(&wire, 132, 68));
    r.set_view_options(1, ViewOptions { models_2d: Models2d::SelectedOnly, ..Default::default() });
    r.render_view(1, &view, &Overlay::default());
    assert!(corner(&save(&r, 1, "models2d_unselected"))[0] > 200);
    r.set_object_flags(50, ObjectFlags::SELECTED);
    r.render_view(1, &view, &Overlay::default());
    assert!(corner(&save(&r, 1, "models2d_selected"))[0] < 50);
    r.set_model_instances(&[]);

    r.upsert_object(1, box_mesh([-50.0, -50.0, 0.0], [50.0, 50.0, 50.0], GREY, [255, 255, 255]));
    r.set_object_flags(1, ObjectFlags::SELECTED);
    r.set_view_options(1, ViewOptions { fill_selected_2d: 0.5, vertex_dots_min_zoom: 0.5, ..Default::default() });
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "fill_dots");
    let inside = pixel(&img, 100, 100);
    assert!(inside[0] > 80 && inside[1] < 40, "selected fill: {inside:?}");
    let dot = pixel(&img, 100 + 51, 100 - 51);
    assert!(dot[0] > 200, "vertex dot outside the corner: {dot:?}");
    r.set_view_options(1, ViewOptions { fill_selected_2d: 0.0, vertex_dots_min_zoom: 2.0, ..Default::default() });
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "no_fill_dots");
    assert!(pixel(&img, 100, 100)[0] < 10 && pixel(&img, 151, 49)[0] < 10);

    r.remove_object(1);
    r.set_material(4, &MaterialDesc { texture: Some(TexId::WHITE), ..Default::default() });
    r.set_sprites(&[SpriteInstance {
        pos: [0.0; 3],
        size: 200.0,
        material: 4,
        color: [0.0, 1.0, 0.0, 1.0],
        object: 60,
    }]);
    r.set_view_options(1, ViewOptions { sprites_2d_px: 16.0, ..Default::default() });
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "icons2d");
    let green = count_near(&img, [0, 255, 0], 30);
    assert!((200..=330).contains(&green), "16 px icon: {green} pixels");
}

#[test]
fn model_distance_fade_and_hidden_chunks() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [200, 200, 200, 255], MaterialFlags::empty());
    let v = |p: [f32; 3]| Vertex { pos: p, normal: [-1.0, 0.0, 0.0], uv: [0.0; 2], alpha: 1.0 };
    let part = ModelPart {
        material: GREY,
        vertices: vec![v([0.0, -64.0, -64.0]), v([0.0, 64.0, -64.0]), v([0.0, 64.0, 64.0]), v([0.0, -64.0, 64.0])],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    };
    let model = r.create_model(&[part]);
    let t = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [1000.0, 0.0, 0.0, 1.0]];
    let faded = ModelInstance { model, transform: t, object: 5, fade: [200.0, 600.0], ..Default::default() };
    r.set_model_instances(&[faded]);
    let p = persp([0.0; 3], [1.0, 0.0, 0.0], [128, 128], RenderMode::Unshaded);
    r.render_view(1, &p, &Overlay::default());
    assert!(pixel(&save(&r, 1, "fade_off"), 64, 64)[0] > 150, "model drawn");
    r.set_view_options(1, ViewOptions { model_fade: true, ..Default::default() });
    r.render_view(1, &p, &Overlay::default());
    assert!(pixel(&save(&r, 1, "fade_on"), 64, 64)[0] < 20, "faded out beyond fademaxdist");
    r.set_model_instances(&[ModelInstance { fade: [0.0; 2], ..faded }]);
    r.render_view(1, &p, &Overlay::default());
    assert!(pixel(&save(&r, 1, "fade_removed"), 64, 64)[0] > 150, "no fade any more");
    r.set_model_instances(&[faded]);
    r.set_view_options(1, ViewOptions { model_distance: 500.0, ..Default::default() });
    r.render_view(1, &p, &Overlay::default());
    let boxed = save(&r, 1, "model_distance");
    assert!(pixel(&boxed, 64, 64)[0] < 20, "far model not drawn");
    assert!(count_near(&boxed, [255, 255, 255], 30) > 20, "its box is drawn");

    r.set_view_options(1, ViewOptions::default());
    r.set_model_instances(&[]);
    for i in 0..10u32 {
        let x = 200.0 + i as f32 * 10.0;
        r.upsert_object(100 + i, box_mesh([x, -4.0, -4.0], [x + 4.0, 4.0, 4.0], GREY, [255; 3]));
    }
    r.render_view(1, &p, &Overlay::default());
    let s = r.stats();
    assert!(s.chunks_in_frustum >= 1 && s.chunks_occluded == 0, "{s:?}");
    for i in 0..10u32 {
        r.set_object_flags(100 + i, ObjectFlags::HIDDEN);
    }
    r.render_view(1, &p, &Overlay::default());
    let s = r.stats();
    assert!(s.chunks_occluded >= 1, "hidden chunks skipped: {s:?}");
    r.set_object_flags(100, ObjectFlags::empty());
    r.render_view(1, &p, &Overlay::default());
    assert_eq!(r.stats().chunks_occluded, 0, "one visible member draws the chunk again");
}

struct CountingHook {
    calls: Arc<AtomicU32>,
}

impl PassHook for CountingHook {
    fn name(&self) -> &'static str {
        "counting"
    }
    fn prepare(&mut self, cx: &HookContext<'_>, _encoder: &mut wgpu::CommandEncoder) {
        assert!(cx.size[0] > 0 && cx.renderer().view_texture_generation(cx.view_id) > 0);
        self.calls.fetch_add(1, Ordering::Relaxed);
    }
    fn draw_opaque(&self, _cx: &HookContext<'_>, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, None, &[]);
        self.calls.fetch_add(10, Ordering::Relaxed);
    }
    fn draw_translucent(&self, _cx: &HookContext<'_>, _pass: &mut wgpu::RenderPass<'_>) {
        self.calls.fetch_add(100, Ordering::Relaxed);
    }
    fn finish(&mut self, cx: &HookContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test post"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.calls.fetch_add(1000, Ordering::Relaxed);
    }
}

#[test]
fn pass_hooks_run_in_order_for_every_view() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [128, 128, 128, 255], MaterialFlags::empty());
    r.upsert_object(1, box_mesh([100.0, -50.0, -50.0], [150.0, 50.0, 50.0], GREY, [255; 3]));
    let calls = Arc::new(AtomicU32::new(0));
    let g = r.content_generation();
    r.add_pass_hook(Box::new(CountingHook { calls: calls.clone() }));
    assert!(r.content_generation() > g);
    assert!(r.pass_hook_names().contains(&"counting"));
    let p = persp([0.0; 3], [1.0, 0.0, 0.0], [64, 64], RenderMode::Textured);
    r.render_view(1, &p, &Overlay::default());
    assert_eq!(calls.load(Ordering::Relaxed), 1111);
    let img = save(&r, 1, "hooks");
    assert!(pixel(&img, 32, 32)[0] > 60, "the scene is still drawn after the hooks");
    r.render_view(2, &top([0.0; 3], 1.0, [33, 33]), &Overlay::default());
    assert_eq!(calls.load(Ordering::Relaxed), 2112);
}

#[test]
fn occlusion_culling_skips_hidden_chunks_without_popping() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [128, 128, 128, 255], MaterialFlags::empty());
    mat(&mut r, 2, [255, 0, 0, 255], MaterialFlags::empty());
    r.upsert_object(1, box_mesh([500.0, -400.0, -300.0], [520.0, 400.0, 300.0], GREY, [255; 3]));
    for i in 0..8u32 {
        let y = i as f32 * 40.0 - 160.0;
        r.upsert_object(10 + i, box_mesh([700.0, y, -20.0], [730.0, y + 30.0, 20.0], 2, [255; 3]));
    }
    r.set_view_options(1, ViewOptions { occlusion_culling: true, ..Default::default() });
    let behind = persp([0.0; 3], [1.0, 0.0, 0.0], [128, 128], RenderMode::Unshaded);
    let reds = |img: &Img| count_near(img, [255, 0, 0], 30);
    for frame in 0..6 {
        r.render_view(1, &behind, &Overlay::default());
        let img = save(&r, 1, "occlusion_behind");
        assert_eq!(reds(&img), 0, "frame {frame}: the boxes are behind the wall");
        assert!(count_near(&img, [128, 128, 128], 10) > 1000, "frame {frame}: the wall is drawn");
    }
    let s = r.stats();
    assert!(s.chunks_occluded >= 1, "the hidden chunk is culled: {s:?}");
    let past = persp([600.0, 0.0, 500.0], [715.0, 0.0, 0.0], [128, 128], RenderMode::Unshaded);
    r.render_view(1, &past, &Overlay::default());
    let seen = reds(&save(&r, 1, "occlusion_past"));
    let msaa = ViewParams { msaa: true, ..behind };
    for _ in 0..3 {
        r.render_view(1, &msaa, &Overlay::default());
    }
    assert_eq!(reds(&save(&r, 1, "occlusion_msaa")), 0);
    r.render_view(1, &ViewParams { msaa: true, ..past }, &Overlay::default());
    let seen_msaa = reds(&save(&r, 1, "occlusion_msaa_past"));
    r.set_view_options(1, ViewOptions::default());
    r.render_view(1, &past, &Overlay::default());
    let expected = reds(&save(&r, 1, "occlusion_off"));
    assert!(expected > 300, "{expected} red pixels without culling");
    assert_eq!(seen, expected, "no popping");
    r.render_view(1, &ViewParams { msaa: true, ..past }, &Overlay::default());
    assert_eq!(seen_msaa, reds(&save(&r, 1, "occlusion_off_msaa")), "no popping with msaa");
}

#[test]
fn grid_styles_dotted_and_64_units() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let grid = GridParams { size: 16.0, major_every: 8, show_1024: false, opacity: 1.0, cells: true };
    let view = ViewParams { grid: Some(grid), ..top([8.0, 8.0, 0.0], 1.0, [201, 201]) };
    let lit = |img: &Img| img.2.chunks_exact(4).filter(|p| p[0] > 30).count();
    r.render_view(1, &view, &Overlay::default());
    let lines = lit(&save(&r, 1, "grid_lines"));
    r.set_view_options(1, ViewOptions { grid_dotted: true, ..Default::default() });
    r.render_view(1, &view, &Overlay::default());
    let dots = lit(&save(&r, 1, "grid_dots"));
    assert!(dots > 20 && dots * 5 < lines, "dots {dots}, lines {lines}");
    r.set_view_options(1, ViewOptions { grid_highlight_64: true, ..Default::default() });
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "grid_64");
    let Some(s64) = view.world_to_screen([64.0, 30.0, 0.0]) else { panic!("2D") };
    let Some(s16) = view.world_to_screen([16.0, 30.0, 0.0]) else { panic!("2D") };
    let (a, b) = (pixel(&img, s64[0] as u32, s64[1] as u32), pixel(&img, s16[0] as u32, s16[1] as u32));
    assert!(a[0] > b[0] + 20, "64 unit line {a:?} vs minor line {b:?}");
}

#[test]
fn selection_outline_shows_through_walls() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [128, 128, 128, 255], MaterialFlags::empty());
    r.upsert_object(1, box_mesh([300.0, -400.0, -300.0], [320.0, 400.0, 300.0], GREY, [255; 3]));
    r.upsert_object(2, box_mesh([500.0, -40.0, -40.0], [580.0, 40.0, 40.0], GREY, [255; 3]));
    let p = ViewParams {
        selection_color: [0.0, 1.0, 0.0, 0.4],
        ..persp([0.0; 3], [1.0, 0.0, 0.0], [128, 128], RenderMode::Unshaded)
    };
    let greens = |img: &Img| count_near(img, [0, 255, 0], 30);
    r.set_object_flags(2, ObjectFlags::SELECTED);
    r.render_view(1, &p, &Overlay::default());
    let edges = greens(&save(&r, 1, "outline_off"));
    for msaa in [false, true] {
        r.set_view_options(1, ViewOptions { selection_outline: true, occlusion_culling: msaa, ..Default::default() });
        r.render_view(1, &ViewParams { msaa, ..p }, &Overlay::default());
        let n = greens(&save(&r, 1, "outline_on"));
        assert!(n > edges + 40, "msaa {msaa}: outline through the wall: {n} pixels, {edges} edge pixels");
    }
    r.set_object_flags(2, ObjectFlags::empty());
    r.render_view(1, &p, &Overlay::default());
    assert_eq!(greens(&save(&r, 1, "outline_none")), 0);
}

#[test]
fn scrolling_materials_follow_the_time() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let mut px = Vec::new();
    for y in 0..8u32 {
        for x in 0..8u32 {
            px.extend_from_slice(&if x < 4 { [255, 255, 255, 255] } else { [0, 0, 0, 255] });
            let _ = y;
        }
    }
    let t = r.create_texture(&TextureUpload { width: 8, height: 8, format: TexFormat::Rgba8Srgb, levels: vec![px] });
    let desc = MaterialDesc {
        texture: Some(t),
        tex_size: [256.0, 256.0],
        scroll: [0.5, 0.0],
        flags: MaterialFlags::UNLIT,
        ..Default::default()
    };
    r.set_material(1, &desc);
    let v = |x: f32, y: f32| Vertex { pos: [x, y, 0.0], normal: [0.0, 0.0, 1.0], uv: [x, -y], alpha: 1.0 };
    r.upsert_object(
        1,
        ObjectMesh {
            batches: vec![MeshBatch {
                material: 1,
                vertices: vec![v(0.0, 0.0), v(256.0, 0.0), v(256.0, 256.0), v(0.0, 256.0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            edges: Vec::new(),
            aabb_min: [0.0; 3],
            aabb_max: [256.0, 256.0, 0.0],
            wire_color: [0; 3],
            ..Default::default()
        },
    );
    r.flush();
    assert!(r.has_animated_materials(), "uploaded geometry uses a scrolling material");
    let p = persp([128.0, 128.0, 300.0], [128.0, 128.0, 0.0], [64, 64], RenderMode::Textured);
    let shot = |r: &mut Renderer, time: f32, name: &str| {
        r.set_view_options(1, ViewOptions { time, ..Default::default() });
        r.render_view(1, &p, &Overlay::default());
        save(r, 1, name).2
    };
    let a = shot(&mut r, 0.0, "scroll_0");
    let b = shot(&mut r, 1.0, "scroll_half");
    let c = shot(&mut r, 2.0, "scroll_full");
    assert_ne!(a, b, "half a texture later the stripes moved");
    assert_eq!(a, c, "a whole texture later they are back");
    assert!(r.view_shows_animation(1));
    let away = persp([128.0, 128.0, 300.0], [128.0, 128.0, 600.0], [64, 64], RenderMode::Textured);
    r.render_view(1, &away, &Overlay::default());
    assert!(!r.view_shows_animation(1));
    r.set_material(1, &MaterialDesc { scroll: [0.0; 2], ..desc });
    assert!(!r.has_animated_materials());
}

#[test]
fn background_images_show_in_their_views_only() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let red = r.create_texture(&solid([255, 0, 0, 255]));
    let corners = [[-50.0, 50.0, 0.0], [50.0, 50.0, 0.0], [50.0, -50.0, 0.0], [-50.0, -50.0, 0.0]];
    let g = r.content_generation();
    r.set_background_images(&[BackgroundImage { texture: red, axis: OrthoAxis::Top, corners, opacity: 0.5 }]);
    assert!(r.content_generation() > g);
    let view = top([0.0; 3], 1.0, [201, 201]);
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "bgimage_top");
    let c = pixel(&img, 100, 100);
    assert!(c[0] > 150 && c[1] < 30, "half transparent red: {c:?}");
    assert_eq!(pixel(&img, 10, 10)[0], 0, "outside the image");
    let front =
        ViewParams { kind: ViewKind::Ortho { axis: OrthoAxis::Front, center: [0.0; 3], pixels_per_unit: 1.0 }, ..view };
    r.render_view(2, &front, &Overlay::default());
    assert_eq!(count_near(&save(&r, 2, "bgimage_front"), [188, 0, 0], 60), 0, "other views do not show it");
    r.destroy_texture(red);
    r.render_view(1, &view, &Overlay::default());
    assert_eq!(pixel(&save(&r, 1, "bgimage_gone"), 100, 100)[0], 0, "its texture is gone");
    r.set_background_images(&[]);
}

#[test]
fn parked_scenes_come_back_as_they_were() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, 2, [255, 0, 0, 255], MaterialFlags::empty());
    let view = persp([-300.0, 0.0, 0.0], [0.0, 0.0, 0.0], [96, 96], RenderMode::Unshaded);
    r.upsert_object(1, box_mesh([0.0, -200.0, -200.0], [16.0, 200.0, 200.0], 2, [255, 255, 255]));
    r.set_object_flags(1, ObjectFlags::SELECTED);
    r.render_view(1, &view, &Overlay::default());
    let red = |img: &Img| count_near(img, [255, 0, 0], 40) + count_near(img, [255, 26, 26], 60);
    assert!(red(&save(&r, 1, "park_a")) > 4000);

    let parked = r.park_scene();
    assert_eq!((parked.objects(), r.stats().objects), (1, 0));
    assert!(parked.gpu_bytes() > 0);
    r.render_view(1, &view, &Overlay::default());
    assert_eq!(red(&save(&r, 1, "park_empty")), 0, "nothing of A is drawn");
    mat(&mut r, 3, [0, 255, 0, 255], MaterialFlags::empty());
    r.upsert_object(7, box_mesh([0.0, -200.0, -200.0], [16.0, 200.0, 200.0], 3, [255, 255, 255]));
    r.render_view(1, &view, &Overlay::default());
    assert!(count_near(&save(&r, 1, "park_b"), [0, 255, 0], 40) > 4000);
    mat(&mut r, 2, [0, 0, 255, 255], MaterialFlags::empty());

    let b = r.restore_scene(parked);
    assert_eq!((b.objects(), r.stats().objects), (1, 1));
    assert!(r.object_flags(1).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));
    assert!(r.object_flags(7).is_none());
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "park_restored");
    assert!(count_near(&img, [179, 14, 196], 40) > 4000, "material refreshed, still selected");
    r.remove_object(1);
    r.render_view(1, &view, &Overlay::default());
    assert_eq!(count_near(&save(&r, 1, "park_edit"), [179, 14, 196], 40), 0);
    let a = r.restore_scene(b);
    assert_eq!(a.objects(), 0);
    r.render_view(1, &view, &Overlay::default());
    assert!(count_near(&save(&r, 1, "park_b_again"), [0, 255, 0], 40) > 4000);
    a.drop_in_background();
}

#[test]
fn sprite_layers_draw_in_3d_only() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    r.set_material(4, &MaterialDesc { texture: Some(TexId::WHITE), ..Default::default() });
    let sprite = |pos: [f32; 3], color: [f32; 4]| SpriteInstance { pos, size: 100.0, material: 4, color, object: 0 };
    r.set_sprites(&[sprite([0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 1.0])]);
    r.set_sprite_layer(SPRITE_LAYER_PARTICLES, &[sprite([0.0, 0.0, 120.0], [1.0, 0.0, 1.0, 1.0])]);
    assert_eq!(r.sprite_layer_len(SPRITE_LAYER_PARTICLES), 1);
    let gen0 = r.content_generation();
    let view = persp([-400.0, 0.0, 60.0], [0.0, 0.0, 60.0], [128, 128], RenderMode::Textured);
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "layer_3d");
    assert!(count_near(&img, [0, 255, 0], 40) > 100, "entity sprite");
    assert!(count_near(&img, [255, 0, 255], 40) > 100, "layer sprite");
    r.set_sprites(&[]);
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "layer_alone");
    assert_eq!(count_near(&img, [0, 255, 0], 40), 0);
    assert!(count_near(&img, [255, 0, 255], 40) > 100);
    r.set_view_options(2, ViewOptions { sprites_2d_px: 16.0, ..Default::default() });
    r.render_view(2, &top([0.0; 3], 1.0, [128, 128]), &Overlay::default());
    assert_eq!(count_near(&save(&r, 2, "layer_2d"), [255, 0, 255], 40), 0);
    r.upsert_object(9, box_mesh([500.0; 3], [501.0; 3], 4, [0, 0, 0]));
    r.set_object_flags(9, ObjectFlags::HIDDEN);
    let hidden = SpriteInstance { object: 9, ..sprite([0.0, 0.0, 120.0], [1.0, 0.0, 1.0, 1.0]) };
    r.set_sprite_layer(SPRITE_LAYER_PARTICLES, &[hidden, sprite([f32::NAN, 0.0, 0.0], [1.0; 4])]);
    r.render_view(1, &view, &Overlay::default());
    assert_eq!(count_near(&save(&r, 1, "layer_hidden"), [255, 0, 255], 40), 0);
    r.set_sprite_layer(SPRITE_LAYER_PARTICLES, &[]);
    let after = r.content_generation();
    r.set_sprite_layer(SPRITE_LAYER_PARTICLES, &[]);
    assert_eq!(r.content_generation(), after, "removing nothing changes nothing");
    assert!(after > gen0);
    assert_eq!(r.sprite_layer_len(SPRITE_LAYER_PARTICLES), 0);
}

#[test]
fn four_way_blend_mixes_textures_by_vertex_weights() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let [red, green, blue] =
        [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]].map(|c| r.create_texture(&solid(c)));
    let desc = MaterialDesc {
        texture: Some(red),
        texture2: Some(green),
        texture3: Some(blue),
        texture4: Some(TexId::WHITE),
        tex_size: [4.0, 4.0],
        ..Default::default()
    };
    r.set_material(5, &desc);
    let view = persp([-300.0, 0.0, 0.0], [0.0, 0.0, 0.0], [64, 64], RenderMode::Unshaded);
    let wall = |blend: BlendVertex| {
        let mut m = box_mesh([0.0, -200.0, -200.0], [16.0, 200.0, 200.0], 5, [0, 0, 0]);
        m.batches[0].blend = vec![blend; m.batches[0].vertices.len()];
        m
    };
    let shot = |r: &mut Renderer, name: &str| {
        r.render_view(1, &view, &Overlay::default());
        save(r, 1, name)
    };
    r.upsert_object(1, wall(BlendVertex::painted([0.0, 0.0, 1.0, 0.0])));
    assert!(count_near(&shot(&mut r, "blend4_blue"), [0, 0, 255], 12) > 3000);
    r.upsert_object(1, wall(BlendVertex::painted([0.0, 1.0, 0.0, 1.0])));
    let img = shot(&mut r, "blend4_top_layer");
    assert!(count_near(&img, [255, 255, 255], 12) > 3000, "texture 4 lies over texture 2: {:?}", pixel(&img, 32, 32));
    r.upsert_object(1, wall(BlendVertex::painted([0.6, 0.4, 0.0, 0.0])));
    let soft = pixel(&shot(&mut r, "blend4_soft"), 32, 32);
    assert!(soft[1] > soft[0], "the default ranges show most of texture 2: {soft:?}");
    let sharp = FourWayDesc { blend: [[0.0, 1.0], [0.9, 1.0], [0.0, 1.0], [0.0, 1.0]], ..FourWayDesc::default() };
    r.set_material(5, &MaterialDesc { four_way: Some(sharp), ..desc });
    assert!(count_near(&shot(&mut r, "blend4_sharp"), [255, 0, 0], 12) > 3000, "below $texture2_blendstart");
    r.set_material(5, &desc);
    r.upsert_object(1, box_mesh([0.0, -200.0, -200.0], [16.0, 200.0, 200.0], 5, [0, 0, 0]));
    assert!(count_near(&shot(&mut r, "blend4_alpha"), [0, 255, 0], 12) > 3000, "unpainted: the alpha shows texture 2");
    let mut clear = box_mesh([0.0, -200.0, -200.0], [16.0, 200.0, 200.0], 5, [0, 0, 0]);
    for v in &mut clear.batches[0].vertices {
        v.alpha = 0.0;
    }
    r.upsert_object(1, clear);
    assert!(count_near(&shot(&mut r, "blend4_none"), [255, 0, 0], 12) > 3000);
    r.upsert_object(1, box_mesh([0.0, -200.0, -200.0], [16.0, 200.0, 200.0], 5, [0, 0, 0]));
    r.set_material(5, &MaterialDesc { texture3: None, texture4: None, ..desc });
    assert!(count_near(&shot(&mut r, "blend4_two_way"), [0, 255, 0], 12) > 3000);
}

fn grid_square() -> ObjectMesh {
    let v = |x: f32, y: f32| Vertex { pos: [x, y, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0; 2], alpha: 0.0 };
    let tri = |a: [f32; 2], b: [f32; 2], c: [f32; 2], class: WalkClass| MarkTri {
        p: [[a[0], a[1], 0.0], [b[0], b[1], 0.0], [c[0], c[1], 0.0]],
        class,
    };
    ObjectMesh {
        batches: vec![MeshBatch {
            material: GREY,
            vertices: vec![v(-64.0, -64.0), v(64.0, -64.0), v(64.0, 64.0), v(-64.0, 64.0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }],
        edges: vec![
            [-64.0, -64.0, 0.0],
            [64.0, -64.0, 0.0],
            [64.0, -64.0, 0.0],
            [64.0, 64.0, 0.0],
            [64.0, 64.0, 0.0],
            [-64.0, 64.0, 0.0],
            [-64.0, 64.0, 0.0],
            [-64.0, -64.0, 0.0],
        ],
        detail_edges: vec![[0.0, -64.0, 0.0], [0.0, 64.0, 0.0], [-64.0, 0.0, 0.0], [64.0, 0.0, 0.0]],
        marks: vec![
            tri([-64.0, -64.0], [0.0, -64.0], [0.0, 64.0], WalkClass::Buildable),
            tri([-64.0, -64.0], [0.0, 64.0], [-64.0, 64.0], WalkClass::Buildable),
            tri([0.0, -64.0], [64.0, -64.0], [64.0, 64.0], WalkClass::Steep),
            tri([0.0, -64.0], [64.0, 64.0], [0.0, 64.0], WalkClass::Steep),
        ],
        aabb_min: [-64.0, -64.0, 0.0],
        aabb_max: [64.0, 64.0, 0.0],
        wire_color: [255, 255, 255],
        ..Default::default()
    }
}

#[test]
fn displacement_detail_lines_and_walk_marks() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [100, 100, 100, 255], MaterialFlags::empty());
    r.upsert_object(1, grid_square());
    let view = top([0.0, 0.0, 0.0], 1.0, [201, 201]);
    let white = |img: &Img, x: u32, y: u32| pixel(img, x, y)[0] > 200 && pixel(img, x, y)[1] > 200;
    let render = |r: &mut Renderer, opts: ViewOptions, name: &str| {
        r.set_view_options(1, opts);
        r.render_view(1, &view, &Overlay::default());
        save(r, 1, name)
    };
    let all = render(&mut r, ViewOptions::default(), "detail_all");
    assert!(white(&all, 100, 70) && white(&all, 164, 70), "grid and outline");
    let none = render(&mut r, ViewOptions { detail_2d: Detail2d::None, ..Default::default() }, "detail_none");
    assert!(!white(&none, 100, 70), "no inner lines: {:?}", pixel(&none, 100, 70));
    assert!(white(&none, 164, 70), "the outline stays");
    let sel = ViewOptions { detail_2d: Detail2d::Selected, ..Default::default() };
    assert!(!white(&render(&mut r, sel, "detail_unselected"), 100, 70));
    r.set_object_flags(1, ObjectFlags::SELECTED);
    let img = render(&mut r, sel, "detail_selected");
    assert!(pixel(&img, 100, 70)[0] > 200, "selected grid: {:?}", pixel(&img, 100, 70));
    r.set_object_flags(1, ObjectFlags::empty());
    let dots = ViewOptions { vertex_dots_min_zoom: 0.5, detail_2d: Detail2d::None, ..Default::default() };
    let img = render(&mut r, dots, "detail_none_dots");
    assert!(!white(&img, 101, 101), "no dot at the grid center: {:?}", pixel(&img, 101, 101));

    let green = WalkClass::Buildable.srgb();
    let red = WalkClass::Steep.srgb();
    let tinted = |img: &Img, x: u32, y: u32, c: [u8; 3]| {
        let p = pixel(img, x, y);
        let dominant = (0..3).max_by_key(|&i| c[i]).unwrap_or(0);
        (0..3).all(|i| i == dominant || p[dominant] > p[i] + 40)
    };
    let off = render(&mut r, ViewOptions::default(), "walk_off_2d");
    assert!(pixel(&off, 70, 130)[1] < 10, "no marks: {:?}", pixel(&off, 70, 130));
    let walk = ViewOptions { walk_marks: true, ..Default::default() };
    let img = render(&mut r, walk, "walk_2d");
    assert!(tinted(&img, 70, 130, green), "buildable half: {:?}", pixel(&img, 70, 130));
    assert!(tinted(&img, 130, 130, red), "steep half: {:?}", pixel(&img, 130, 130));
    let img = render(&mut r, ViewOptions { detail_2d: Detail2d::None, ..walk }, "walk_2d_outline");
    assert!(pixel(&img, 70, 130)[1] < 10, "no marks with the outline: {:?}", pixel(&img, 70, 130));
    let p = persp([0.0, -150.0, 150.0], [0.0, 0.0, 0.0], [200, 150], RenderMode::Textured);
    r.set_view_options(1, walk);
    r.render_view(1, &p, &Overlay::default());
    let img = save(&r, 1, "walk_3d");
    assert!(tinted(&img, 85, 85, green), "3D buildable: {:?}", pixel(&img, 85, 85));
    assert!(tinted(&img, 115, 85, red), "3D steep: {:?}", pixel(&img, 115, 85));
    r.render_view(1, &ViewParams { mode: RenderMode::Wireframe, ..p }, &Overlay::default());
    let img = save(&r, 1, "walk_3d_wire");
    assert!(tinted(&img, 85, 85, green) && tinted(&img, 115, 85, red), "walk_3d_wire: {:?}", pixel(&img, 85, 85));
    r.set_view_options(1, ViewOptions::default());
    r.render_view(1, &ViewParams { mode: RenderMode::Wireframe, ..p }, &Overlay::default());
    let img = save(&r, 1, "walk_3d_wire_off");
    assert_eq!(pixel(&img, 85, 85)[1], 0, "no marks, no faces in wireframe");
    r.render_view(1, &p, &Overlay::default());
    let img = save(&r, 1, "walk_3d_off");
    let c = pixel(&img, 85, 85);
    assert!(c[0].abs_diff(c[1]) < 10, "grey without the marks: {c:?}");
    r.set_view_options(1, walk);
    r.set_object_flags(1, ObjectFlags::HIDDEN);
    r.render_view(1, &p, &Overlay::default());
    assert_eq!(pixel(&save(&r, 1, "walk_3d_hidden"), 85, 85)[1], 0);
}

#[test]
fn walk_colors_match_the_shader() {
    let src = include_str!("../src/shaders/lines.wgsl");
    for c in WalkClass::GRADIENT {
        let [r, g, b] = c.srgb();
        let line = format!("vec3<f32>({r}.0, {g}.0, {b}.0)");
        assert!(src.contains(&line), "{c:?} color {line} in lines.wgsl");
    }
}

#[test]
fn textured_view_shades_slopes_like_hammer() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, GREY, [200, 200, 200, 255], MaterialFlags::empty());
    let t = 15f32.to_radians();
    let quad = |c: [f32; 3], tilt: f32| {
        let (s, k) = (tilt.sin(), tilt.cos());
        let u = [k * 80.0, 0.0, -s * 80.0];
        let normal = [s, 0.0, k];
        let corner = |a: f32, b: f32| [c[0] + u[0] * a, c[1] + 80.0 * b, c[2] + u[2] * a];
        let pos = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
        MeshBatch {
            material: GREY,
            vertices: pos.iter().map(|p| Vertex { pos: *p, normal, uv: [0.0; 2], alpha: 0.0 }).collect(),
            indices: vec![0, 1, 2, 0, 2, 3],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }
    };
    let centers = [[-200.0, 0.0, 0.0], [0.0, 0.0, 0.0], [200.0, 0.0, 0.0]];
    r.upsert_object(
        1,
        ObjectMesh {
            batches: vec![quad(centers[0], -t), quad(centers[1], 0.0), quad(centers[2], t)],
            edges: Vec::new(),
            aabb_min: [-300.0, -100.0, -30.0],
            aabb_max: [300.0, 100.0, 30.0],
            wire_color: [255; 3],
            ..Default::default()
        },
    );
    let p = persp([0.0, -250.0, 700.0], [0.0, 0.0, 0.0], [480, 320], RenderMode::Textured);
    r.render_view(1, &p, &Overlay::default());
    let img = save(&r, 1, "textured_slopes");
    let at = |c: [f32; 3]| {
        let Some(s) = p.world_to_screen(c) else { panic!("visible") };
        i32::from(pixel(&img, s[0] as u32, s[1] as u32)[0])
    };
    let (away, flat, toward) = (at(centers[0]), at(centers[1]), at(centers[2]));
    assert!(toward - away >= 30, "slopes read: away {away}, floor {flat}, toward the light {toward}");
    assert!(toward >= flat + 10 && flat >= away + 10, "away {away}, floor {flat}, toward {toward}");
    r.render_view(1, &ViewParams { mode: RenderMode::Unshaded, ..p }, &Overlay::default());
    let img = save(&r, 1, "textured_slopes_unshaded");
    for c in centers {
        let Some(s) = p.world_to_screen(c) else { panic!("visible") };
        let v = i32::from(pixel(&img, s[0] as u32, s[1] as u32)[0]);
        assert!((v - 200).abs() <= 3, "unshaded {v}");
    }
}

#[test]
fn particles_blend_add_turn_and_show_sheet_frames() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    r.set_material(4, &MaterialDesc { texture: Some(TexId::WHITE), ..Default::default() });
    let mut px = Vec::new();
    for _y in 0..4 {
        for x in 0..4 {
            px.extend_from_slice(if x < 2 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    let halves =
        r.create_texture(&TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![px] });
    r.set_material(5, &MaterialDesc { texture: Some(halves), tex_size: [4.0, 4.0], ..Default::default() });
    let particle = |pos: [f32; 3], material: MaterialId, color: [f32; 4]| ParticleInstance {
        animation: ParticleAnimation::default(),
        pos,
        size: 60.0,
        axis: [0.0; 3],
        roll: 0.0,
        yaw: 0.0,
        uv: [0.0, 0.0, 1.0, 1.0],
        color,
        bright: 1.0,
        add_self: 0.0,
        orient: ParticleOrient::Screen,
        material,
        screen_fade: [0.0; 2],
        refraction_blur: 0.0,
        no_fog: false,
        blend: ParticleBlend::Alpha,
    };
    let view = ViewParams {
        show_sprites: false,
        ..persp([-400.0, 0.0, 0.0], [0.0, 0.0, 0.0], [128, 128], RenderMode::Textured)
    };
    let blended = particle([0.0, 60.0, 0.0], 4, [1.0, 0.0, 1.0, 1.0]);
    let added =
        ParticleInstance { blend: ParticleBlend::Additive, ..particle([0.0, -60.0, 0.0], 4, [0.0, 1.0, 0.0, 1.0]) };
    r.set_particles(&[blended, added]);
    assert_eq!(r.particle_count(), 2);
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "particles");
    assert!(count_near(&img, [255, 0, 255], 40) > 100, "blended particle");
    assert!(count_near(&img, [0, 255, 0], 40) > 100, "additive particle");
    let dim = |blend| ParticleInstance { blend, ..particle([0.0, 0.0, 0.0], 4, [0.5, 0.5, 0.5, 1.0]) };
    r.set_particles(&[dim(ParticleBlend::Additive), dim(ParticleBlend::Additive)]);
    r.render_view(1, &view, &Overlay::default());
    let summed = pixel(&save(&r, 1, "particles_add"), 64, 64);
    r.set_particles(&[dim(ParticleBlend::Alpha), dim(ParticleBlend::Alpha)]);
    r.render_view(1, &view, &Overlay::default());
    let covered = pixel(&save(&r, 1, "particles_blend"), 64, 64);
    assert!(summed[0] > covered[0] + 40, "{summed:?} vs {covered:?}");
    r.set_particles(&[ParticleInstance { uv: [0.5, 0.0, 1.0, 1.0], size: 200.0, ..particle([0.0; 3], 5, [1.0; 4]) }]);
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "particles_uv");
    assert!(count_near(&img, [0, 0, 255], 30) > 1000);
    assert_eq!(count_near(&img, [255, 0, 0], 30), 0);
    let tall = ParticleInstance {
        orient: ParticleOrient::Stretched,
        axis: [0.0, 0.0, 200.0],
        size: 10.0,
        ..particle([0.0; 3], 4, [1.0; 4])
    };
    r.set_particles(&[tall]);
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "particles_stretched");
    let white = |x: u32, y: u32| pixel(&img, x, y)[1] > 200;
    let rows = (0..128).filter(|&y| white(64, y)).count();
    let cols = (0..128).filter(|&x| white(x, 64)).count();
    assert!(rows > cols * 4, "{rows} rows, {cols} columns");
    let flat = ParticleInstance { orient: ParticleOrient::Flat, size: 100.0, ..particle([0.0; 3], 4, [1.0; 4]) };
    r.set_particles(&[flat]);
    r.render_view(1, &view, &Overlay::default());
    let img = save(&r, 1, "particles_flat");
    assert!((0..128).filter(|&y| pixel(&img, 64, y)[1] > 200).count() <= 3);
    let magenta = particle([0.0; 3], 4, [1.0, 0.0, 1.0, 1.0]);
    r.set_particles(&[ParticleInstance { blend: ParticleBlend::AddSelf, ..magenta }]);
    r.render_view(1, &view, &Overlay::default());
    assert!(count_near(&save(&r, 1, "particles_addself"), [255, 0, 255], 40) > 100);
    let near = ParticleInstance { size: 400.0, screen_fade: [0.2, 0.4], ..magenta };
    r.set_particles(&[near]);
    r.render_view(1, &view, &Overlay::default());
    assert_eq!(count_near(&save(&r, 1, "particles_faded"), [255, 0, 255], 40), 0);
    r.set_particles(&[ParticleInstance { screen_fade: [0.0; 2], ..near }]);
    r.render_view(1, &view, &Overlay::default());
    assert!(count_near(&save(&r, 1, "particles_unfaded"), [255, 0, 255], 40) > 1000);
    r.set_particles(&[particle([0.0; 3], 4, [1.0, 0.0, 1.0, 1.0])]);
    r.render_view(2, &top([0.0; 3], 1.0, [128, 128]), &Overlay::default());
    assert_eq!(count_near(&save(&r, 2, "particles_2d"), [255, 0, 255], 40), 0);
    r.set_particles(&[ParticleInstance { pos: [f32::NAN; 3], ..particle([0.0; 3], 4, [1.0; 4]) }]);
    assert_eq!(r.particle_count(), 0);
    r.set_particles(&[]);
    let gen0 = r.content_generation();
    r.set_particles(&[]);
    assert_eq!(r.content_generation(), gen0, "removing nothing changes nothing");
}

#[test]
fn particles_refract_the_scene_and_keep_foreground_depth_with_and_without_msaa() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, 4, [255, 0, 0, 255], MaterialFlags::UNLIT);
    mat(&mut r, 5, [0, 0, 255, 255], MaterialFlags::UNLIT);
    mat(&mut r, 6, [0, 255, 0, 255], MaterialFlags::UNLIT);
    r.upsert_object(1, box_mesh([50.0, -400.0, -200.0], [60.0, 0.0, 200.0], 4, [0; 3]));
    r.upsert_object(2, box_mesh([50.0, 0.0, -200.0], [60.0, 400.0, 200.0], 5, [0; 3]));
    let normal = r.create_texture(&TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Linear,
        levels: vec![[255, 128, 255, 255].repeat(16)],
    });
    r.set_material(7, &MaterialDesc { texture: Some(normal), ..Default::default() });
    let particle = ParticleInstance {
        animation: ParticleAnimation::default(),
        pos: [0.0; 3],
        size: 220.0,
        axis: [0.0; 3],
        roll: 0.0,
        yaw: 0.0,
        uv: [0.0, 0.0, 1.0, 1.0],
        color: [1.0; 4],
        bright: 1.0,
        add_self: 0.08,
        screen_fade: [0.0; 2],
        refraction_blur: 0.0,
        no_fog: false,
        orient: ParticleOrient::Screen,
        material: 7,
        blend: ParticleBlend::Refract,
    };
    for msaa in [false, true] {
        let view = ViewParams { msaa, ..persp([-400.0, 0.0, 0.0], [0.0; 3], [128, 128], RenderMode::Unshaded) };
        r.set_particles(&[]);
        r.render_view(1, &view, &Overlay::default());
        let plain = save(&r, 1, &format!("heat_plain_{msaa}"));
        r.set_particles(&[particle]);
        r.render_view(1, &view, &Overlay::default());
        let heat = save(&r, 1, &format!("heat_refract_{msaa}"));
        let changed = plain.2.chunks_exact(4).zip(heat.2.chunks_exact(4)).filter(|(a, b)| a != b).count();
        assert!(changed > 100, "scene must be distorted, {changed} pixels changed");
        r.set_particles(&[ParticleInstance { add_self: -0.08, refraction_blur: 1.0, no_fog: true, ..particle }]);
        r.render_view(1, &view, &Overlay::default());
        let reversed = save(&r, 1, &format!("heat_negative_{msaa}"));
        assert_ne!(reversed.2, plain.2, "negative refraction must stay visible");
        assert_ne!(reversed.2, heat.2, "negative refraction has the opposite direction");
        r.set_particles(&[ParticleInstance { add_self: 0.0, ..particle }]);
        r.render_view(1, &view, &Overlay::default());
        assert_eq!(r.read_view_rgba8(1).unwrap().2, plain.2, "zero refraction is invisible");
        r.upsert_object(3, box_mesh([-80.0, -30.0, -30.0], [-60.0, 30.0, 30.0], 6, [0; 3]));
        r.set_particles(&[]);
        r.render_view(1, &view, &Overlay::default());
        let front = save(&r, 1, &format!("heat_front_{msaa}"));
        r.set_particles(&[particle]);
        r.render_view(1, &view, &Overlay::default());
        let occluded = save(&r, 1, &format!("heat_occluded_{msaa}"));
        assert_eq!(pixel(&front, 64, 64), pixel(&occluded, 64, 64));
        r.remove_object(3);
    }
    r.remove_view(1);
    r.render_view(1, &persp([-400.0, 0.0, 0.0], [0.0; 3], [192, 128], RenderMode::Textured), &Overlay::default());
    assert_eq!(r.read_view_rgba8(1).unwrap().0, 192);
}

#[test]
fn particles_respect_scene_lighting_emissive_and_material_opacity() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    r.set_material(4, &MaterialDesc { texture: Some(TexId::WHITE), ..Default::default() });
    let view = ViewParams { background: [0.0; 3], ..persp([-400.0, 0.0, 0.0], [0.0; 3], [64, 64], RenderMode::Lit) };
    let p = ParticleInstance {
        animation: ParticleAnimation::default(),
        pos: [0.0; 3],
        size: 220.0,
        axis: [0.0; 3],
        roll: 0.0,
        yaw: 0.0,
        uv: [0.0, 0.0, 1.0, 1.0],
        color: [0.5, 0.5, 0.5, 1.0],
        bright: 1.0,
        add_self: 0.0,
        screen_fade: [0.0; 2],
        refraction_blur: 0.0,
        no_fog: false,
        orient: ParticleOrient::ScreenLit,
        material: 4,
        blend: ParticleBlend::Alpha,
    };
    r.set_lights(&[], [0.0; 3]);
    r.set_particles(&[p]);
    r.render_view(1, &view, &Overlay::default());
    let dark = pixel(&save(&r, 1, "particles_lit_dark"), 32, 32)[0];
    r.set_particles(&[ParticleInstance { orient: ParticleOrient::Screen, ..p }]);
    r.render_view(1, &view, &Overlay::default());
    let unlit = pixel(&save(&r, 1, "particles_unlit"), 32, 32)[0];
    assert!(dark < 5 && unlit > 100);
    r.set_lights(&[], [0.25; 3]);
    r.set_particles(&[p]);
    r.render_view(1, &view, &Overlay::default());
    let lit = pixel(&save(&r, 1, "particles_lit_ambient"), 32, 32)[0];
    assert!(lit > dark + 20 && lit < unlit);
    r.set_particles(&[ParticleInstance { orient: ParticleOrient::ScreenEmissive, ..p }]);
    r.render_view(1, &view, &Overlay::default());
    let emissive = pixel(&save(&r, 1, "particles_emissive"), 32, 32)[0];
    assert!(emissive > lit && emissive >= unlit);
    r.set_material(4, &MaterialDesc { texture: Some(TexId::WHITE), tint: [1.0, 1.0, 1.0, 0.25], ..Default::default() });
    r.set_particles(&[ParticleInstance { color: [1.0; 4], orient: ParticleOrient::Screen, ..p }]);
    r.render_view(1, &view, &Overlay::default());
    let alpha = pixel(&save(&r, 1, "particles_material_alpha"), 32, 32)[0];
    assert!((130..=142).contains(&alpha), "one quarter linear opacity: {alpha}");
}

#[test]
fn the_particle_preview_view_draws_exactly_like_the_scene_view() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    r.set_material(4, &MaterialDesc { texture: Some(TexId::WHITE), ..Default::default() });
    let mut px = Vec::new();
    for y in 0..8u32 {
        for x in 0..8u32 {
            let d = ((x as f32 - 3.5).powi(2) + (y as f32 - 3.5).powi(2)).sqrt() / 4.5;
            let a = ((1.0 - d).clamp(0.0, 1.0) * 255.0) as u8;
            px.extend_from_slice(&[255, 200, 120, a]);
        }
    }
    let blob = r.create_texture(&TextureUpload { width: 8, height: 8, format: TexFormat::Rgba8Srgb, levels: vec![px] });
    r.set_material(5, &MaterialDesc { texture: Some(blob), tex_size: [8.0, 8.0], ..Default::default() });
    let particle = |pos: [f32; 3], material: MaterialId, blend: ParticleBlend| ParticleInstance {
        animation: ParticleAnimation::default(),
        pos,
        size: 70.0,
        axis: [0.0; 3],
        roll: 0.4,
        yaw: 0.0,
        uv: [0.0, 0.0, 1.0, 1.0],
        color: [1.0, 0.8, 0.6, 0.8],
        bright: 1.0,
        add_self: 0.0,
        orient: ParticleOrient::Screen,
        material,
        screen_fade: [0.0; 2],
        refraction_blur: 0.0,
        no_fog: false,
        blend,
    };
    let list = [
        particle([0.0, 40.0, 0.0], 5, ParticleBlend::Alpha),
        particle([10.0, -30.0, 10.0], 5, ParticleBlend::Additive),
        particle([-20.0, 0.0, -10.0], 4, ParticleBlend::AddSelf),
    ];
    let view = ViewParams {
        show_sprites: false,
        msaa: false,
        ..persp([-400.0, 0.0, 0.0], [0.0, 0.0, 0.0], [128, 128], RenderMode::Textured)
    };
    r.set_particles(&list);
    r.render_view(1, &view, &Overlay::default());
    let scene = save(&r, 1, "preview_scene");
    r.set_particles(&[]);
    r.render_particle_preview(7, &view, &list);
    assert_eq!(r.preview_particle_count(), 3);
    assert_eq!(r.particle_count(), 0, "the scene particles are untouched");
    let preview = save(&r, 7, "preview_view");
    let bad = scene.2.iter().zip(&preview.2).filter(|(a, b)| (i32::from(**a) - i32::from(**b)).abs() > 2).count();
    assert!(bad < 40, "{bad} differing channels");
    assert!(count_near(&preview, [255, 200, 120], 120) > 200, "the sprites are visible");
    r.render_particle_preview(7, &view, &[]);
    assert_eq!(r.preview_particle_count(), 0);
}

#[test]
fn spritecard_mixes_texture_frames_and_both_sequences_before_alpha_blending() {
    let (device, queue) = gpu_or_skip!();
    let mut renderer = Renderer::new(&device, &queue);
    let mut atlas = Vec::new();
    for _ in 0..4 {
        for x in 0..4 {
            atlas.extend_from_slice(if x < 2 { &[255, 0, 0, 64] } else { &[0, 0, 255, 192] });
        }
    }
    let texture = renderer.create_texture(&TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Linear,
        levels: vec![atlas],
    });
    renderer.set_material(4, &MaterialDesc { texture: Some(texture), tex_size: [4.0, 4.0], ..Default::default() });
    let red = [0.125, 0.125, 0.375, 0.875];
    let blue = [0.625, 0.125, 0.875, 0.875];
    let base = ParticleInstance {
        animation: ParticleAnimation { frames: [blue, blue, blue], ..Default::default() },
        pos: [0.0; 3],
        size: 200.0,
        axis: [0.0; 3],
        roll: 0.0,
        yaw: 0.0,
        uv: red,
        color: [1.0; 4],
        bright: 1.0,
        add_self: 0.0,
        screen_fade: [0.0; 2],
        refraction_blur: 0.0,
        no_fog: true,
        orient: ParticleOrient::Screen,
        material: 4,
        blend: ParticleBlend::Alpha,
    };
    let view = persp([-400.0, 0.0, 0.0], [0.0; 3], [128, 128], RenderMode::Textured);
    for (name, animation, expected) in [
        ("frame", ParticleAnimation { blend: [0.25, 0.0], ..base.animation }, [191, 0, 64, 96]),
        ("average", ParticleAnimation { dual_sequence: true, ..base.animation }, [128, 0, 128, 128]),
        ("mask", ParticleAnimation { dual_sequence: true, sequence_mode: 1, ..base.animation }, [0, 0, 255, 64]),
        ("over", ParticleAnimation { dual_sequence: true, sequence_mode: 2, ..base.animation }, [63, 0, 192, 64]),
        (
            "max_first",
            ParticleAnimation { blend: [0.8, 0.0], max_luminance: [true, false], ..base.animation },
            [0, 0, 255, 192],
        ),
        (
            "max_second",
            ParticleAnimation {
                frames: [blue, red, blue],
                blend: [0.8, 0.0],
                dual_sequence: true,
                sequence_mode: 1,
                max_luminance: [false, true],
            },
            [0, 0, 255, 166],
        ),
    ] {
        renderer.set_particles(&[ParticleInstance { animation, ..base }]);
        renderer.render_view(1, &view, &Overlay::default());
        let actual = pixel(&save(&renderer, 1, &format!("spritecard_{name}")), 64, 64);
        let reference = renderer.create_texture(&TextureUpload {
            width: 1,
            height: 1,
            format: TexFormat::Rgba8Linear,
            levels: vec![expected.to_vec()],
        });
        renderer.set_material(5, &MaterialDesc { texture: Some(reference), ..Default::default() });
        renderer.set_particles(&[ParticleInstance { material: 5, animation: ParticleAnimation::default(), ..base }]);
        renderer.render_view(1, &view, &Overlay::default());
        let target = pixel(&save(&renderer, 1, &format!("spritecard_{name}_reference")), 64, 64);
        for c in 0..3 {
            assert!(actual[c].abs_diff(target[c]) <= 2, "{name}: {actual:?} versus {target:?}");
        }
    }
}

#[test]
fn spritecard_pixels_match_native_garrys_mod_captures() {
    let (device, queue) = gpu_or_skip!();
    let mut renderer = Renderer::new(&device, &queue);
    let mut atlas = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            atlas.extend_from_slice(if y < 4 {
                if x < 4 { &[255, 0, 0, 64] } else { &[0, 0, 255, 192] }
            } else if x < 4 {
                &[128, 128, 128, 128]
            } else {
                &[0, 255, 0, 255]
            });
        }
    }
    let texture = renderer.create_texture(&TextureUpload {
        width: 8,
        height: 8,
        format: TexFormat::Rgba8Srgb,
        levels: vec![atlas],
    });
    renderer.set_material(4, &MaterialDesc { texture: Some(texture), tex_size: [8.0, 8.0], ..Default::default() });
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../hp-pcf/tests/data/spritecard_native_samples.json")).unwrap();
    let red = [0.0625, 0.0625, 0.4375, 0.4375];
    let blue = [0.5625, 0.0625, 0.9375, 0.4375];
    let gray = [0.0625, 0.5625, 0.4375, 0.9375];
    let frames = |sequence: u64, age: f32, rate: f32, fps: bool| {
        let pair = match sequence {
            0 => (red, red),
            1 => (blue, blue),
            3 => (gray, gray),
            _ => (red, blue),
        };
        let count = if sequence == 2 { 2.0 } else { 1.0 };
        let frame = (age * rate / if fps { count } else { 1.0 }).rem_euclid(1.0) * count;
        if frame < 1.0 { (pair.0, pair.1, frame) } else { (pair.1, pair.0, frame - 1.0) }
    };
    let view = persp([-128.0, 0.0, 0.0], [0.0; 3], [128, 128], RenderMode::Textured);
    for case in cases {
        for (index, sample) in case["samples"].as_array().unwrap().iter().enumerate() {
            if ![0, 2, 6, 12, 20, 28].contains(&index) {
                continue;
            }
            let age = sample["age"].as_f64().unwrap() as f32;
            let fps = case["fps"].as_bool().unwrap();
            let (uv, next, first_blend) =
                frames(case["sequence"].as_u64().unwrap(), age, case["rate"].as_f64().unwrap() as f32, fps);
            let (second, second_next, second_blend) =
                frames(case["second"].as_u64().unwrap(), age, case["rate2"].as_f64().unwrap() as f32, fps);
            renderer.set_particles(&[ParticleInstance {
                animation: ParticleAnimation {
                    frames: [next, second, second_next],
                    blend: [first_blend, second_blend],
                    dual_sequence: case["dual"].as_bool().unwrap(),
                    sequence_mode: case["mode"].as_u64().unwrap() as u8,
                    max_luminance: [case["maximum"][0].as_bool().unwrap(), case["maximum"][1].as_bool().unwrap()],
                },
                pos: [0.0; 3],
                size: 32.0,
                axis: [0.0; 3],
                roll: 0.0,
                yaw: 0.0,
                uv,
                color: [1.0; 4],
                bright: 1.0,
                add_self: 0.0,
                screen_fade: [0.0; 2],
                refraction_blur: 0.0,
                no_fog: true,
                orient: ParticleOrient::Screen,
                material: 4,
                blend: ParticleBlend::Alpha,
            }]);
            renderer.render_view(1, &view, &Overlay::default());
            let (_, _, pixels) = renderer.read_view_rgba8(1).unwrap();
            let actual = &pixels[(64 * 128 + 64) * 4..][..3];
            for (channel, value) in actual.iter().enumerate() {
                let expected = sample["pixel"][channel].as_u64().unwrap() as u8;
                assert!(
                    value.abs_diff(expected) <= 3,
                    "{} age {age}: {actual:?} versus native {}",
                    case["name"],
                    sample["pixel"]
                );
            }
        }
    }
}

#[test]
fn spritecard_yaw_geometry_matches_native_coverage_and_remains_independent_of_roll() {
    let (device, queue) = gpu_or_skip!();
    let mut renderer = Renderer::new(&device, &queue);
    let texture = renderer.create_texture(&TextureUpload {
        width: 1,
        height: 1,
        format: TexFormat::Rgba8Srgb,
        levels: vec![vec![255, 0, 0, 64]],
    });
    renderer.set_material(4, &MaterialDesc { texture: Some(texture), ..Default::default() });
    let mut view = persp([0.0, -128.0, 0.0], [0.0; 3], [128, 128], RenderMode::Textured);
    view.game_pixel_grid = true;
    view.kind = ViewKind::Perspective {
        eye: [0.0, -128.0, 0.0],
        forward: [0.0, 1.0, 0.0],
        up: [0.0, 0.0, 1.0],
        fov_y_deg: 60.0,
        near: 1.0,
        far: 2000.0,
    };
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../hp-pcf/tests/data/yaw_native_samples.json")).unwrap();
    for case in cases {
        renderer.set_particles(&[ParticleInstance {
            animation: ParticleAnimation::default(),
            pos: [0.0; 3],
            size: 32.0,
            axis: [0.0; 3],
            roll: (case["roll"].as_f64().unwrap() as f32).to_radians(),
            yaw: case["yaw"].as_f64().unwrap() as f32,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            bright: 1.0,
            add_self: 0.0,
            screen_fade: [0.0; 2],
            refraction_blur: 0.0,
            no_fog: true,
            orient: ParticleOrient::Screen,
            material: 4,
            blend: ParticleBlend::Alpha,
        }]);
        renderer.render_view(1, &view, &Overlay::default());
        let (_, _, pixels) = renderer.read_view_rgba8(1).unwrap();
        for row in 0..128 {
            let mut extent = [128, -1];
            for column in 0..128 {
                let pixel = &pixels[(row * 128 + column) * 4..][..3];
                if pixel.iter().any(|channel| *channel > 3) {
                    extent[0] = extent[0].min(column as i32);
                    extent[1] = extent[1].max(column as i32);
                }
            }
            let expected: [i32; 2] = std::array::from_fn(|axis| case["rows"][row][axis].as_i64().unwrap() as i32);
            assert_eq!(extent, expected, "{} row{row}, yaw{}", case["name"], case["yaw"]);
        }
    }
}

#[test]
fn simulated_colors_and_fades_match_native_spritecard_pixels() {
    struct Lighting {
        origin: [f32; 3],
        particle: [f32; 3],
        controls: [[f32; 3]; 2],
    }
    impl hp_pcf::environment::Environment for Lighting {
        fn lighting(&self, position: [f32; 3]) -> [f32; 3] {
            if position[0] < self.origin[0] - 100.0 {
                self.controls[1]
            } else if position[0] > self.origin[0] + 5.0 {
                self.particle
            } else {
                self.controls[0]
            }
        }
    }
    let (device, queue) = gpu_or_skip!();
    let mut renderer = Renderer::new(&device, &queue);
    mat(&mut renderer, 4, [128, 128, 128, 128], MaterialFlags::empty());
    let view = persp([-128.0, 0.0, 0.0], [0.0; 3], [128, 128], RenderMode::Textured);
    let mut check = |particle: &hp_pcf::Particle, expected: &serde_json::Value, context: &str| {
        renderer.set_particles(&[ParticleInstance {
            animation: ParticleAnimation::default(),
            pos: [0.0; 3],
            size: particle.radius * 2.0,
            axis: [0.0; 3],
            roll: particle.rotation,
            yaw: particle.yaw,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [particle.color[0], particle.color[1], particle.color[2], particle.alpha],
            bright: 1.0,
            add_self: 0.0,
            screen_fade: [0.0; 2],
            refraction_blur: 0.0,
            no_fog: true,
            orient: ParticleOrient::Screen,
            material: 4,
            blend: ParticleBlend::Alpha,
        }]);
        renderer.render_view(1, &view, &Overlay::default());
        let (_, _, pixels) = renderer.read_view_rgba8(1).unwrap();
        let actual = &pixels[(64 * 128 + 64) * 4..][..3];
        for (channel, value) in actual.iter().enumerate() {
            let target = expected[channel].as_u64().unwrap() as u8;
            assert!(value.abs_diff(target) <= 2, "{context}: {actual:?} vs native {expected}");
        }
    };
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../../hp-pcf/tests/data/color_light_native_samples.json")).unwrap();
    let read = |value: &serde_json::Value| std::array::from_fn(|axis| value[axis].as_f64().unwrap() as f32);
    let world = Lighting {
        origin: read(&reference["origin"]),
        particle: read(&reference["lighting"]),
        controls: [read(&reference["control_lighting"][0]), read(&reference["control_lighting"][1])],
    };
    let mut points = hp_pcf::Points::at(world.origin);
    points.pos.push([world.origin[0] - 256.0, world.origin[1], world.origin[2]]);
    let definitions = hp_pcf::parse_file(include_bytes!("../../hp-pcf/tests/data/color_light_cases.pcf")).unwrap();
    for case in reference["cases"].as_array().unwrap() {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let mut sim =
            hp_pcf::Sim::new(Arc::new(hp_pcf::Plan::new(definitions.iter().find(|def| def.name == name).unwrap())), 17);
        sim.step_in(0.0, &points, &[], 1, &world);
        check(&sim.particles[0], &case["pixel"], &name);
    }
    let definitions = hp_pcf::parse_file(include_bytes!("../../hp-pcf/tests/data/color_fade_cases.pcf")).unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../hp-pcf/tests/data/color_fade_native_samples.json")).unwrap();
    for case in cases {
        let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
        let mut sim =
            hp_pcf::Sim::new(Arc::new(hp_pcf::Plan::new(definitions.iter().find(|def| def.name == name).unwrap())), 17);
        sim.step(0.0, [0.0; 3], 1);
        for sample in case["samples"].as_array().unwrap() {
            let age = sample["age"].as_f64().unwrap() as f32;
            while age - sim.particles[0].age > 0.000001 {
                sim.step((age - sim.particles[0].age).min(0.05), [0.0; 3], 1);
            }
            check(&sim.particles[0], &sample["pixel"], &format!("{name} age {age}"));
        }
    }
}
