use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

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
        let info = adapter.get_info();
        eprintln!("adapter: {} ({:?})", info.name, info.backend);
        if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
            eprintln!("adapter lacks vertex storage buffers: GPU tests skipped");
            return None;
        }
        let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hp-render test"),
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

fn scratch() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-render/scratch");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn save(r: &Renderer, view: u32, name: &str) -> (u32, u32, Vec<u8>) {
    let (w, h, px) = r.read_view_rgba8(view).expect("readback");
    let path = scratch().join(format!("{name}.png"));
    image::save_buffer(&path, &px, w, h, image::ColorType::Rgba8).expect("png");
    (w, h, px)
}

fn pixel(img: &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 4] {
    let o = ((y * img.0 + x) * 4) as usize;
    [img.2[o], img.2[o + 1], img.2[o + 2], img.2[o + 3]]
}

fn distinct_colors(img: &(u32, u32, Vec<u8>)) -> usize {
    let mut set = std::collections::HashSet::new();
    for p in img.2.chunks_exact(4) {
        set.insert([p[0], p[1], p[2]]);
    }
    set.len()
}

fn assert_not_uniform(img: &(u32, u32, Vec<u8>), name: &str) {
    let n = distinct_colors(img);
    assert!(n > 8, "{name}: image looks uniform ({n} colors)");
}

fn checker(size: u32, cell: u32, a: [u8; 4], b: [u8; 4]) -> TextureUpload {
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            px.extend_from_slice(if ((x / cell) + (y / cell)).is_multiple_of(2) { &a } else { &b });
        }
    }
    TextureUpload { width: size, height: size, format: TexFormat::Rgba8Srgb, levels: vec![px] }
}

fn bricks(size: u32) -> TextureUpload {
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let row = y / 16;
            let off = if row.is_multiple_of(2) { 0 } else { 16 };
            let mortar = y % 16 < 2 || (x + off) % 32 < 2;
            let n = ((x * 7 + y * 13) % 11) as u8 * 3;
            px.extend_from_slice(&if mortar { [200, 200, 190, 255] } else { [150 + n, 70 + n, 40, 255] });
        }
    }
    TextureUpload { width: size, height: size, format: TexFormat::Rgba8Srgb, levels: vec![px] }
}

fn circle(size: u32) -> TextureUpload {
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    let c = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt() / c;
            let a = if d < 0.8 {
                255
            } else if d < 1.0 {
                ((1.0 - d) / 0.2 * 255.0) as u8
            } else {
                0
            };
            let ring = d > 0.55 && d < 0.7;
            px.extend_from_slice(&if ring { [255, 255, 255, a] } else { [255, 220, 40, a] });
        }
    }
    TextureUpload { width: size, height: size, format: TexFormat::Rgba8Srgb, levels: vec![px] }
}

fn bc1_solid(size: u32, rgb565: u16) -> TextureUpload {
    let mut levels = Vec::new();
    let mut s = size;
    loop {
        let blocks = (s.div_ceil(4) * s.div_ceil(4)) as usize;
        let mut data = Vec::with_capacity(blocks * 8);
        for _ in 0..blocks {
            data.extend_from_slice(&rgb565.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes());
            data.extend_from_slice(&[0, 0, 0, 0]);
        }
        levels.push(data);
        if s == 1 {
            break;
        }
        s /= 2;
    }
    TextureUpload { width: size, height: size, format: TexFormat::Bc1Srgb, levels }
}

fn box_mesh(min: [f32; 3], max: [f32; 3], material: MaterialId, wire: [u8; 3]) -> ObjectMesh {
    box_mesh_multi(min, max, [material; 6], wire)
}

fn box_mesh_multi(min: [f32; 3], max: [f32; 3], mats: [MaterialId; 6], wire: [u8; 3]) -> ObjectMesh {
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
    for (fi, (corners, n)) in faces.iter().enumerate() {
        let verts: Vec<Vertex> = corners
            .iter()
            .map(|&i| {
                let p = c(i);
                let uv = if n[2] != 0.0 {
                    [p[0], -p[1]]
                } else if n[0] != 0.0 {
                    [p[1], -p[2]]
                } else {
                    [p[0], -p[2]]
                };
                Vertex { pos: p, normal: *n, uv, alpha: 1.0 }
            })
            .collect();
        let idx = if fi.is_multiple_of(2) { vec![0, 1, 2, 0, 2, 3] } else { vec![0, 2, 1, 0, 3, 2] };
        batches.push(MeshBatch {
            material: mats[fi],
            vertices: verts,
            indices: idx,
            blend: Vec::new(),
            cubemaps: Vec::new(),
        });
    }
    let pairs = [(0, 1), (1, 3), (3, 2), (2, 0), (4, 5), (5, 7), (7, 6), (6, 4), (0, 4), (1, 5), (2, 6), (3, 7)];
    let edges = pairs.iter().flat_map(|&(a, b)| [c(a), c(b)]).collect();
    ObjectMesh { batches, edges, aabb_min: min, aabb_max: max, wire_color: wire, ..Default::default() }
}

fn pyramid(material: MaterialId) -> Vec<ModelPart> {
    let apex: [f32; 3] = [0.0, 0.0, 64.0];
    let base: [[f32; 3]; 4] = [[-32.0, -32.0, 0.0], [32.0, -32.0, 0.0], [32.0, 32.0, 0.0], [-32.0, 32.0, 0.0]];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for i in 0..4 {
        let (a, b) = (base[i], base[(i + 1) % 4]);
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [apex[0] - a[0], apex[1] - a[1], apex[2] - a[2]];
        let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = [n[0] / l, n[1] / l, n[2] / l];
        let s = vertices.len() as u32;
        for (p, uv) in [(a, [0.0, 64.0]), (b, [64.0, 64.0]), (apex, [32.0, 0.0])] {
            vertices.push(Vertex { pos: p, normal: n, uv, alpha: 1.0 });
        }
        indices.extend_from_slice(&[s, s + 1, s + 2]);
    }
    vec![ModelPart { material, vertices, indices, tangents: Vec::new() }]
}

const MAT_BRICK: MaterialId = 1;
const MAT_CHECK: MaterialId = 2;
const MAT_TOOL: MaterialId = 3;
const MAT_SPRITE: MaterialId = 4;
const MAT_BC: MaterialId = 5;
const MAT_MODEL: MaterialId = 6;
const MAT_GLASS: MaterialId = 7;
const MAT_UNKNOWN: MaterialId = 99;

const OBJ_FLOOR: ObjectId = 1;
const OBJ_CUBE: ObjectId = 2;
const OBJ_SELECTED: ObjectId = 3;
const OBJ_TOOL: ObjectId = 4;
const OBJ_PLUS_X: ObjectId = 5;
const OBJ_PLUS_Y: ObjectId = 6;
const OBJ_PLUS_Z: ObjectId = 7;
const OBJ_UNKNOWN: ObjectId = 8;
const OBJ_GLASS: ObjectId = 9;
const OBJ_PROP_A: ObjectId = 100;
const OBJ_PROP_B: ObjectId = 101;
const OBJ_SPRITE_A: ObjectId = 200;
const OBJ_SPRITE_B: ObjectId = 201;

fn build_scene(r: &mut Renderer) {
    let brick = r.create_texture(&bricks(64));
    let check = r.create_texture(&checker(64, 16, [40, 90, 200, 255], [230, 230, 230, 255]));
    let tool = r.create_texture(&checker(32, 8, [250, 140, 0, 255], [200, 90, 0, 255]));
    let sprite = r.create_texture(&circle(64));
    let bc = r.create_texture(&bc1_solid(16, 0x07e0));
    let glass = r.create_texture(&checker(16, 8, [120, 220, 255, 110], [80, 160, 220, 140]));
    assert_ne!(brick, TexId::MISSING);
    assert!(r.texture_view(brick).is_some() && r.texture_view(TexId::WHITE).is_some());
    assert_ne!(bc, TexId::MISSING);
    let mat = |t: TexId, s: f32, flags: MaterialFlags| MaterialDesc {
        texture: Some(t),
        tex_size: [s, s],
        flags,
        tint: [1.0; 4],
        bump: None,
        texture2: None,
        scroll: [0.0; 2],
        texture3: None,
        texture4: None,
        alpha_ref: 0.5,
        detail: None,
        water: None,
        envmap: None,
        self_illum: None,
        four_way: None,
    };
    r.set_material(MAT_BRICK, &mat(brick, 64.0, MaterialFlags::empty()));
    r.set_material(MAT_CHECK, &mat(check, 64.0, MaterialFlags::empty()));
    r.set_material(MAT_TOOL, &mat(tool, 32.0, MaterialFlags::TOOL));
    r.set_material(MAT_SPRITE, &mat(sprite, 64.0, MaterialFlags::TRANSLUCENT));
    r.set_material(MAT_BC, &mat(bc, 16.0, MaterialFlags::empty()));
    r.set_material(MAT_MODEL, &mat(check, 32.0, MaterialFlags::empty()));
    r.set_material(MAT_GLASS, &mat(glass, 32.0, MaterialFlags::TRANSLUCENT | MaterialFlags::TWO_SIDED));

    r.upsert_object(OBJ_FLOOR, box_mesh([-512.0, -512.0, -16.0], [512.0, 512.0, 0.0], MAT_BRICK, [200, 200, 200]));
    r.upsert_object(
        OBJ_CUBE,
        box_mesh_multi(
            [0.0, 0.0, 0.0],
            [128.0, 128.0, 128.0],
            [MAT_CHECK, MAT_CHECK, MAT_CHECK, MAT_BC, MAT_CHECK, MAT_BC],
            [255, 255, 255],
        ),
    );
    r.upsert_object(OBJ_SELECTED, box_mesh([-300.0, 100.0, 0.0], [-180.0, 220.0, 96.0], MAT_BRICK, [255, 255, 255]));
    r.upsert_object(OBJ_TOOL, box_mesh([100.0, -300.0, 0.0], [260.0, -140.0, 160.0], MAT_TOOL, [255, 160, 0]));
    r.upsert_object(OBJ_PLUS_X, box_mesh([400.0, -16.0, 0.0], [432.0, 16.0, 32.0], MAT_CHECK, [0, 255, 0]));
    r.upsert_object(OBJ_PLUS_Y, box_mesh([-16.0, 400.0, 0.0], [16.0, 432.0, 32.0], MAT_CHECK, [0, 128, 255]));
    r.upsert_object(OBJ_PLUS_Z, box_mesh([-400.0, -400.0, 300.0], [-368.0, -368.0, 332.0], MAT_CHECK, [255, 0, 255]));
    r.upsert_object(OBJ_UNKNOWN, box_mesh([-200.0, -300.0, 0.0], [-100.0, -200.0, 100.0], MAT_UNKNOWN, [255, 255, 0]));
    r.upsert_object(OBJ_GLASS, box_mesh([-80.0, -250.0, 0.0], [-60.0, -100.0, 140.0], MAT_GLASS, [120, 220, 255]));
    r.set_object_flags(OBJ_SELECTED, ObjectFlags::SELECTED);

    let model = r.create_model(&pyramid(MAT_MODEL));
    let gone = r.create_model(&pyramid(MAT_MODEL));
    r.destroy_model(gone);
    let t = |x: f32, y: f32, s: f32| [[s, 0.0, 0.0, 0.0], [0.0, s, 0.0, 0.0], [0.0, 0.0, s, 0.0], [x, y, 0.0, 1.0]];
    r.set_model_instances(&[
        ModelInstance { model, transform: t(300.0, 250.0, 1.0), object: OBJ_PROP_A, tint: [1.0; 4], fade: [0.0; 2] },
        ModelInstance {
            model,
            transform: t(380.0, 120.0, 1.5),
            object: OBJ_PROP_B,
            tint: [1.0, 0.6, 0.6, 1.0],
            fade: [0.0; 2],
        },
        ModelInstance { model: gone, transform: t(-400.0, 300.0, 1.0), object: 102, tint: [1.0; 4], fade: [0.0; 2] },
    ]);
    r.set_object_flags(OBJ_PROP_B, ObjectFlags::SELECTED);
    r.set_object_wire_color(OBJ_PROP_A, [160, 60, 255]);
    r.set_sprites(&[
        SpriteInstance {
            pos: [64.0, 64.0, 200.0],
            size: 48.0,
            material: MAT_SPRITE,
            color: [1.0; 4],
            object: OBJ_SPRITE_A,
        },
        SpriteInstance {
            pos: [-240.0, 160.0, 160.0],
            size: 48.0,
            material: MAT_SPRITE,
            color: [0.4, 1.0, 0.4, 1.0],
            object: OBJ_SPRITE_B,
        },
    ]);
    r.set_object_flags(OBJ_SPRITE_B, ObjectFlags::SELECTED);
    r.set_lights(
        &[
            PreviewLight {
                kind: LightKind::Directional { dir: [0.5, 0.35, -0.8] },
                pos: [0.0; 3],
                color: [1.0, 0.95, 0.85],
                intensity: 0.6,
                range: 0.0,
                falloff: None,
            },
            PreviewLight {
                kind: LightKind::Point,
                pos: [64.0, -120.0, 80.0],
                color: [1.0, 0.8, 0.5],
                intensity: 0.7,
                range: 600.0,
                falloff: None,
            },
            PreviewLight {
                kind: LightKind::Spot { dir: [0.0, 0.0, -1.0], inner_deg: 20.0, outer_deg: 35.0 },
                pos: [-240.0, -300.0, 300.0],
                color: [0.3, 0.5, 1.0],
                intensity: 2.0,
                range: 1000.0,
                falloff: None,
            },
            PreviewLight {
                kind: LightKind::Point,
                pos: [-350.0, 350.0, 60.0],
                color: [0.2, 1.0, 0.3],
                intensity: 0.4,
                range: 300.0,
                falloff: None,
            },
        ],
        [0.03, 0.03, 0.04],
    );
}

fn params(kind: ViewKind, mode: RenderMode, msaa: bool) -> ViewParams {
    ViewParams {
        size: [640, 400],
        kind,
        mode,
        grid: Some(GridParams { size: 16.0, major_every: 8, show_1024: true, opacity: 1.0 }),
        background: [0.12, 0.12, 0.14],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: true,
        show_models: true,
        msaa,
    }
}

fn persp() -> ViewKind {
    let eye = [-700.0f32, -620.0, 460.0];
    let target = [0.0f32, 0.0, 40.0];
    let f = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
    ViewKind::Perspective { eye, forward: f, up: [0.0, 0.0, 1.0], fov_y_deg: 70.0, near: 4.0, far: 20000.0 }
}

fn ortho(axis: OrthoAxis) -> ViewKind {
    ViewKind::Ortho { axis, center: [0.0, 0.0, 0.0], pixels_per_unit: 0.35 }
}

fn overlay() -> Overlay {
    let mut o = Overlay::default();
    o.lines.push(OverlayLine { a: [-512.0, -512.0, 2.0], b: [512.0, 512.0, 2.0], color: [1.0, 1.0, 0.0, 1.0] });
    o.lines_on_top.push(OverlayLine { a: [-512.0, 512.0, 2.0], b: [512.0, -512.0, 2.0], color: [0.0, 1.0, 1.0, 1.0] });
    o.triangles.push(OverlayTri {
        p: [[-500.0, -500.0, 1.0], [-300.0, -500.0, 1.0], [-400.0, -300.0, 1.0]],
        color: [0.2, 1.0, 0.2, 0.5],
    });
    o.triangles_on_top.push(OverlayTri {
        p: [[300.0, -500.0, 1.0], [500.0, -500.0, 1.0], [400.0, -300.0, 1.0]],
        color: [1.0, 0.2, 1.0, 0.5],
    });
    for p in [[0.0, 0.0, 128.0], [128.0, 128.0, 128.0], [128.0, 0.0, 128.0]] {
        o.points.push(OverlayPoint { pos: p, color: [1.0, 1.0, 1.0, 1.0], size_px: 7.0 });
    }
    o
}

#[test]
fn renders_every_view_kind_and_mode() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    eprintln!("supports_bc = {}", r.supports_bc());
    build_scene(&mut r);
    let ov = overlay();

    let t = Instant::now();
    let modes = [
        (RenderMode::Textured, "textured"),
        (RenderMode::Flat, "flat"),
        (RenderMode::Wireframe, "wireframe"),
        (RenderMode::TexturedWire, "textured_wire"),
        (RenderMode::Lit, "lit"),
    ];
    for msaa in [true, false] {
        for (mode, name) in modes {
            r.render_view(0, &params(persp(), mode, msaa), &ov);
            let img = save(&r, 0, &format!("3d_{name}{}", if msaa { "" } else { "_nomsaa" }));
            assert_not_uniform(&img, name);
        }
    }
    eprintln!("3D modes rendered in {:?}", t.elapsed());

    for (axis, name) in [(OrthoAxis::Top, "top"), (OrthoAxis::Front, "front"), (OrthoAxis::Side, "side")] {
        for msaa in [false, true] {
            r.render_view(1, &params(ortho(axis), RenderMode::Wireframe, msaa), &ov);
            let img = save(&r, 1, &format!("2d_{name}{}", if msaa { "_msaa" } else { "" }));
            assert_not_uniform(&img, name);
        }
    }
    let s = r.stats();
    eprintln!("stats: {s:?}");
    assert!(s.draw_calls > 0 && s.triangles > 0);
    assert!(s.objects >= 9 && s.chunks > 0 && s.textures >= 9);
}

fn find_color(img: &(u32, u32, Vec<u8>), rgb: [u8; 3], tol: i32) -> Option<(f32, f32, usize)> {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0usize);
    for y in 0..img.1 {
        for x in 0..img.0 {
            let p = pixel(img, x, y);
            if (0..3).all(|i| (i32::from(p[i]) - i32::from(rgb[i])).abs() <= tol) {
                sx += x as f32;
                sy += y as f32;
                n += 1;
            }
        }
    }
    (n > 0).then(|| (sx / n as f32, sy / n as f32, n))
}

#[test]
fn ortho_views_follow_axis_conventions() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    build_scene(&mut r);
    let empty = Overlay::default();
    let no_grid =
        |axis| ViewParams { grid: None, background: [0.0; 3], ..params(ortho(axis), RenderMode::Wireframe, false) };
    r.render_view(7, &no_grid(OrthoAxis::Top), &empty);
    let img = save(&r, 7, "axes_top");
    let (cx, cy) = (img.0 as f32 / 2.0, img.1 as f32 / 2.0);
    let green = find_color(&img, [0, 255, 0], 10).expect("+X marker visible in top view");
    let blue = find_color(&img, [0, 128, 255], 10).expect("+Y marker visible in top view");
    assert!(green.0 > cx + 100.0 && (green.1 - cy).abs() < 10.0, "top: +X is right {green:?}");
    assert!(blue.1 < cy - 100.0 && (blue.0 - cx).abs() < 10.0, "top: +Y is up {blue:?}");
    r.render_view(7, &no_grid(OrthoAxis::Front), &empty);
    let img = save(&r, 7, "axes_front");
    let blue = find_color(&img, [0, 128, 255], 10).expect("+Y marker visible in front view");
    let magenta = find_color(&img, [255, 0, 255], 10).expect("+Z marker visible in front view");
    assert!(blue.0 > cx + 100.0, "front: +Y is right {blue:?}");
    assert!(magenta.1 < cy - 80.0, "front: +Z is up {magenta:?}");
    assert!(magenta.0 < cx - 100.0, "front: marker at y = -400 is left {magenta:?}");
    r.render_view(7, &no_grid(OrthoAxis::Side), &empty);
    let img = save(&r, 7, "axes_side");
    let green = find_color(&img, [0, 255, 0], 10).expect("+X marker visible in side view");
    let magenta = find_color(&img, [255, 0, 255], 10).expect("+Z marker visible in side view");
    assert!(green.0 > cx + 100.0, "side: +X is right {green:?}");
    assert!(magenta.1 < cy - 80.0 && magenta.0 < cx - 100.0, "side: +Z is up {magenta:?}");
    let red = find_color(&img, [255, 26, 26], 40);
    assert!(red.is_some(), "selected edges drawn in the selection color");
}

#[test]
fn grid_lines_are_crisp_and_aligned() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let p = ViewParams {
        size: [256, 256],
        kind: ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0; 3], pixels_per_unit: 1.0 },
        grid: Some(GridParams { size: 8.0, major_every: 4, show_1024: true, opacity: 1.0 }),
        background: [0.0; 3],
        ..params(ortho(OrthoAxis::Top), RenderMode::Wireframe, false)
    };
    r.render_view(3, &p, &Overlay::default());
    let img = save(&r, 3, "grid_crisp");
    let axis = pixel(&img, 128, 10);
    assert!(axis[1] > 100 && axis[0] < 40, "axis color at column 128: {axis:?}");
    let left = pixel(&img, 127, 10);
    let right = pixel(&img, 129, 10);
    assert!(left[1] < 30 && right[1] < 30, "{left:?} {right:?}");
    let minor = pixel(&img, 136, 10);
    let major = pixel(&img, 160, 10);
    let between = pixel(&img, 140, 10);
    assert!(minor[0] > 30 && major[0] > minor[0] && between[0] < 10, "{minor:?} {major:?} {between:?}");
    r.upsert_object(10, box_mesh([8.0, 8.0, 0.0], [24.0, 24.0, 8.0], 1, [255, 255, 0]));
    let p2 = ViewParams { grid: None, ..p };
    r.render_view(3, &p2, &Overlay::default());
    let img = save(&r, 3, "grid_edges");
    let on = pixel(&img, 136, 128 - 16);
    assert!(on[0] > 200 && on[1] > 200, "edge at x = 8 drawn in column 136: {on:?}");
    let off = pixel(&img, 135, 128 - 16);
    assert!(off[0] < 50, "column 135 empty: {off:?}");
}

#[test]
fn selection_tool_and_flags() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    build_scene(&mut r);
    let empty = Overlay::default();
    let down = |eye: [f32; 3]| ViewKind::Perspective {
        eye,
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        fov_y_deg: 30.0,
        near: 1.0,
        far: 5000.0,
    };
    let p = ViewParams { size: [128, 128], ..params(down([64.0, 64.0, 600.0]), RenderMode::Flat, false) };
    r.render_view(4, &p, &empty);
    let base = save(&r, 4, "flags_base");
    let (sx, sy) = (36, 64);
    let c0 = pixel(&base, sx, sy);
    assert!(c0[0] > 200 && c0[1] > 200, "{c0:?}");
    r.set_object_flags(OBJ_CUBE, ObjectFlags::SELECTED);
    r.render_view(4, &p, &empty);
    let sel = save(&r, 4, "flags_selected");
    let c1 = pixel(&sel, sx, sy);
    assert!(i32::from(c1[0]) > i32::from(c1[1]) + 40, "selection tint is red: {c1:?}");
    r.set_object_flags(OBJ_CUBE, ObjectFlags::HIDDEN);
    r.render_view(4, &p, &empty);
    let hidden = save(&r, 4, "flags_hidden");
    let c2 = pixel(&hidden, sx, sy);
    assert_ne!(c2, c0, "hidden cube shows the floor");
    r.set_object_flags(OBJ_CUBE, ObjectFlags::empty());
    r.set_object_wire_color(OBJ_CUBE, [0, 255, 0]);
    r.render_view(4, &p, &empty);
    let green = save(&r, 4, "flags_green");
    let c3 = pixel(&green, sx, sy);
    assert!(c3[1] > 200 && c3[0] < 50, "{c3:?}");

    let pt = ViewParams { size: [128, 128], ..params(down([180.0, -220.0, 800.0]), RenderMode::Textured, false) };
    r.render_view(4, &pt, &empty);
    let tool = save(&r, 4, "tool_translucent");
    let tc = pixel(&tool, 64, 64);
    r.set_object_flags(OBJ_TOOL, ObjectFlags::HIDDEN);
    r.render_view(4, &pt, &empty);
    let floor = save(&r, 4, "tool_hidden");
    let fc = pixel(&floor, 64, 64);
    r.set_material(
        MAT_TOOL,
        &MaterialDesc { flags: MaterialFlags::empty(), ..r.material(MAT_TOOL).expect("tool material") },
    );
    r.set_object_flags(OBJ_TOOL, ObjectFlags::empty());
    r.render_view(4, &pt, &empty);
    let opaque = save(&r, 4, "tool_opaque");
    let oc = pixel(&opaque, 64, 64);
    for i in 0..3 {
        let (lo, hi) = (i32::from(fc[i].min(oc[i])), i32::from(fc[i].max(oc[i])));
        let v = i32::from(tc[i]);
        assert!(v + 3 >= lo && v <= hi + 3, "channel {i}: tool {tc:?} floor {fc:?} opaque {oc:?}");
    }
    assert_ne!(tc, oc);
}

#[test]
fn lit_mode_shows_light_falloff() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    r.set_material(1, &MaterialDesc { texture: Some(TexId::WHITE), tex_size: [1.0, 1.0], ..Default::default() });
    r.upsert_object(1, box_mesh([-2048.0, -2048.0, -16.0], [2048.0, 2048.0, 0.0], 1, [255; 3]));
    r.set_lights(
        &[PreviewLight {
            kind: LightKind::Point,
            pos: [0.0, 0.0, 64.0],
            color: [1.0, 0.9, 0.7],
            intensity: 0.5,
            range: 900.0,
            falloff: None,
        }],
        [0.0; 3],
    );
    let p = ViewParams {
        size: [256, 256],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 1500.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            fov_y_deg: 80.0,
            near: 1.0,
            far: 10000.0,
        },
        ..params(persp(), RenderMode::Lit, true)
    };
    r.render_view(5, &p, &Overlay::default());
    let img = save(&r, 5, "lit_falloff");
    let center = pixel(&img, 128, 128);
    let mid = pixel(&img, 128 + 40, 128);
    let far = pixel(&img, 250, 128);
    assert!(center[0] > mid[0] && mid[0] > far[0], "falloff: {center:?} {mid:?} {far:?}");
    assert!(far[0] < 10, "outside the range the floor is dark: {far:?}");
    let lights: Vec<PreviewLight> = (0..1024)
        .map(|i| PreviewLight {
            kind: LightKind::Point,
            pos: [((i % 32) as f32 - 16.0) * 128.0, ((i / 32) as f32 - 16.0) * 128.0, 32.0],
            color: [((i * 37) % 255) as f32 / 255.0, ((i * 91) % 255) as f32 / 255.0, ((i * 53) % 255) as f32 / 255.0],
            intensity: 0.05,
            range: 200.0,
            falloff: None,
        })
        .collect();
    r.set_lights(&lights, [0.02; 3]);
    let t = Instant::now();
    r.render_view(5, &p, &Overlay::default());
    eprintln!("1024 lights: frame recorded in {:?}", t.elapsed());
    let img = save(&r, 5, "lit_1024");
    assert_not_uniform(&img, "lit 1024");
}

#[test]
fn resize_generation_edit_and_destroy() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    build_scene(&mut r);
    let empty = Overlay::default();
    let mut p = params(persp(), RenderMode::Textured, true);
    assert_eq!(r.view_texture_generation(9), 0);
    r.render_view(9, &p, &empty);
    let g1 = r.view_texture_generation(9);
    r.render_view(9, &p, &empty);
    assert_eq!(r.view_texture_generation(9), g1, "no resize, same texture");
    p.size = [320, 200];
    r.render_view(9, &p, &empty);
    let g2 = r.view_texture_generation(9);
    assert_ne!(g1, g2);
    assert!(r.view_texture(9).is_some());
    let img = r.read_view_rgba8(9).expect("readback");
    assert_eq!((img.0, img.1), (320, 200));
    p.size = [0, 0];
    r.render_view(9, &p, &empty);
    assert_eq!(r.read_view_rgba8(9).expect("readback").0, 1);
    p.size = [640, 400];

    r.upsert_object(OBJ_CUBE, box_mesh([0.0, 0.0, 0.0], [128.0, 128.0, 200.0], MAT_CHECK, [255; 3]));
    r.render_view(9, &p, &empty);
    let moved = save(&r, 9, "edit_moved");
    let mut tall = box_mesh([0.0, 0.0, 0.0], [128.0, 128.0, 260.0], MAT_CHECK, [255; 3]);
    tall.batches.truncate(5);
    r.upsert_object(OBJ_CUBE, tall);
    r.render_view(9, &p, &empty);
    let _ = save(&r, 9, "edit_topology");
    r.remove_object(OBJ_CUBE);
    r.render_view(9, &p, &empty);
    let removed = save(&r, 9, "edit_removed");
    assert_ne!(moved.2, removed.2);

    let tex = r.create_texture(&checker(8, 4, [0, 0, 255, 255], [0, 0, 255, 255]));
    r.set_material(MAT_BRICK, &MaterialDesc { texture: Some(tex), tex_size: [8.0, 8.0], ..Default::default() });
    r.destroy_texture(tex);
    r.destroy_texture(TexId::MISSING);
    r.render_view(9, &p, &empty);
    let img = save(&r, 9, "destroyed_texture");
    assert!(find_color(&img, [255, 0, 255], 60).is_some(), "missing checker visible");
    let bad = TextureUpload { width: 4, height: 4, format: TexFormat::Bc3Srgb, levels: vec![vec![0; 3]] };
    assert_eq!(r.create_texture(&bad), TexId::MISSING);
    assert!(r.try_create_texture(&bad).is_err());

    r.clear_objects();
    r.render_view(9, &p, &empty);
    assert_eq!(r.stats().objects, 0);
    r.remove_view(9);
    assert!(r.view_texture(9).is_none());
    assert!(r.read_view_rgba8(9).is_err());
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.next() % 1_000_000) as f32 / 1_000_000.0 * (hi - lo)
    }
}

#[test]
#[ignore = "performance test: cargo test -p hp-render --release -- --ignored perf"]
fn perf_50k_boxes_4_views() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for m in 0..50u32 {
        let c = |s: u32| (((m * s) % 200) + 40) as u8;
        let t = r.create_texture(&checker(64, 16, [c(37), c(91), c(53), 255], [200, 200, 200, 255]));
        r.set_material(m, &MaterialDesc { texture: Some(t), tex_size: [64.0, 64.0], ..Default::default() });
    }
    let t0 = Instant::now();
    let meshes: Vec<ObjectMesh> = (0..50_000u32)
        .map(|_| {
            let p = [rng.range(-15000.0, 15000.0), rng.range(-15000.0, 15000.0), rng.range(-2000.0, 2000.0)];
            let s = [rng.range(16.0, 256.0), rng.range(16.0, 256.0), rng.range(16.0, 256.0)];
            let mats: [MaterialId; 6] = std::array::from_fn(|_| rng.next() % 50);
            box_mesh_multi(p, [p[0] + s[0], p[1] + s[1], p[2] + s[2]], mats, [200, 200, 200])
        })
        .collect();
    eprintln!("generated meshes in {:?}", t0.elapsed());
    let t1 = Instant::now();
    for (i, m) in meshes.into_iter().enumerate() {
        r.upsert_object(i as u32, m);
    }
    r.flush();
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    eprintln!("upsert + flush of 50k boxes: {:?}", t1.elapsed());

    let model = r.create_model(&pyramid(1));
    let inst: Vec<ModelInstance> = (0..2000u32)
        .map(|i| {
            let (x, y) = (rng.range(-15000.0, 15000.0), rng.range(-15000.0, 15000.0));
            ModelInstance {
                model,
                transform: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, y, 0.0, 1.0]],
                object: 100_000 + i,
                tint: [1.0; 4],
                fade: [0.0; 2],
            }
        })
        .collect();
    r.set_model_instances(&inst);
    for i in (0..50_000u32).step_by(97) {
        r.set_object_flags(i, ObjectFlags::SELECTED);
    }

    let views = [
        ViewParams {
            size: [1280, 720],
            ..params(
                ViewKind::Perspective {
                    eye: [-16000.0, -16000.0, 6000.0],
                    forward: [1.0, 1.0, -0.35],
                    up: [0.0, 0.0, 1.0],
                    fov_y_deg: 75.0,
                    near: 4.0,
                    far: 65536.0,
                },
                RenderMode::Textured,
                true,
            )
        },
        ViewParams {
            size: [960, 540],
            ..params(
                ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0; 3], pixels_per_unit: 0.03 },
                RenderMode::Wireframe,
                false,
            )
        },
        ViewParams {
            size: [960, 540],
            ..params(
                ViewKind::Ortho { axis: OrthoAxis::Front, center: [0.0; 3], pixels_per_unit: 0.03 },
                RenderMode::Wireframe,
                false,
            )
        },
        ViewParams {
            size: [960, 540],
            ..params(
                ViewKind::Ortho { axis: OrthoAxis::Side, center: [0.0; 3], pixels_per_unit: 0.03 },
                RenderMode::Wireframe,
                false,
            )
        },
    ];
    let ov = Overlay::default();
    let t2 = Instant::now();
    for (i, v) in views.iter().enumerate() {
        r.render_view(i as u32, v, &ov);
    }
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    eprintln!("first frame (pipelines, targets): {:?}", t2.elapsed());
    for (i, name) in ["perf_3d", "perf_top", "perf_front", "perf_side"].iter().enumerate() {
        save(&r, i as u32, name);
    }

    let frames = 100;
    let mut cpu = 0.0f64;
    let t3 = Instant::now();
    for _ in 0..frames {
        let c = Instant::now();
        for (i, v) in views.iter().enumerate() {
            r.render_view(i as u32, v, &ov);
        }
        cpu += c.elapsed().as_secs_f64();
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
    }
    let total = t3.elapsed().as_secs_f64();
    let s = r.stats();
    eprintln!(
        "PERF 4 views: {:.2} ms/frame (GPU waited), CPU record {:.2} ms/frame, {} draws, {} tris, {} chunks, {:.1} MB GPU",
        total * 1000.0 / f64::from(frames),
        cpu * 1000.0 / f64::from(frames),
        s.draw_calls,
        s.triangles,
        s.chunks,
        s.approx_gpu_bytes as f64 / 1048576.0
    );

    let lights: Vec<PreviewLight> = (0..1024)
        .map(|_| PreviewLight {
            kind: LightKind::Point,
            pos: [rng.range(-15000.0, 15000.0), rng.range(-15000.0, 15000.0), rng.range(-2000.0, 2000.0)],
            color: [1.0, 0.9, 0.8],
            intensity: 1.0,
            range: 1500.0,
            falloff: None,
        })
        .collect();
    r.set_lights(&lights, [0.05; 3]);
    let lit = ViewParams { mode: RenderMode::Lit, ..views[0] };
    r.render_view(0, &lit, &ov);
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    let t4 = Instant::now();
    for _ in 0..frames {
        r.render_view(0, &lit, &ov);
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
    }
    eprintln!("PERF lit 3D view, 1024 lights: {:.2} ms/frame", t4.elapsed().as_secs_f64() * 1000.0 / f64::from(frames));
    save(&r, 0, "perf_lit");

    let mut with_sun = vec![PreviewLight {
        kind: LightKind::Directional { dir: [0.4, 0.3, -1.0] },
        pos: [0.0; 3],
        color: [1.0, 0.95, 0.85],
        intensity: 0.9,
        range: 0.0,
        falloff: None,
    }];
    with_sun.extend_from_slice(&lights);
    r.set_lights(&with_sun, [0.05; 3]);
    let t5 = Instant::now();
    for i in 0..frames {
        let mut v = lit;
        if let ViewKind::Perspective { eye, .. } = &mut v.kind {
            eye[0] += i as f32 * 120.0;
        }
        r.render_view(0, &v, &ov);
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
    }
    eprintln!(
        "PERF lit 3D view, sun shadow re-rendered each frame + 1024 lights: {:.2} ms/frame",
        t5.elapsed().as_secs_f64() * 1000.0 / f64::from(frames)
    );
    save(&r, 0, "perf_lit_sun");

    let t6 = Instant::now();
    let edits = 200;
    for k in 0..edits {
        let p = [rng.range(-15000.0, 15000.0), rng.range(-15000.0, 15000.0), 0.0];
        r.upsert_object(1234, box_mesh_multi(p, [p[0] + 64.0, p[1] + 64.0, 64.0], [k % 50; 6], [255, 255, 0]));
        r.flush();
    }
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    eprintln!("PERF single brush edit + flush: {:.3} ms", t6.elapsed().as_secs_f64() * 1000.0 / f64::from(edits));
    let t7 = Instant::now();
    for i in 0..1000u32 {
        r.set_object_flags(i * 50, ObjectFlags::SELECTED | ObjectFlags::HOVERED);
    }
    r.flush();
    eprintln!("PERF 1000 flag changes + flush: {:.3} ms", t7.elapsed().as_secs_f64() * 1000.0);
}

#[test]
fn displacement_blend_texture2() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let red = r.create_texture(&checker(4, 2, [255, 0, 0, 255], [255, 0, 0, 255]));
    let blue = r.create_texture(&checker(4, 2, [0, 0, 255, 255], [0, 0, 255, 255]));
    r.set_material(
        1,
        &MaterialDesc {
            texture: Some(red),
            texture2: Some(blue),
            tex_size: [4.0, 4.0],
            flags: MaterialFlags::UNLIT,
            ..Default::default()
        },
    );
    let v = |x: f32, y: f32, a: f32| Vertex { pos: [x, y, 0.0], normal: [0.0, 0.0, 1.0], uv: [x, y], alpha: a };
    r.upsert_object(
        1,
        ObjectMesh {
            batches: vec![MeshBatch {
                material: 1,
                vertices: vec![
                    v(-100.0, -100.0, 0.0),
                    v(100.0, -100.0, 1.0),
                    v(100.0, 100.0, 1.0),
                    v(-100.0, 100.0, 0.0),
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            edges: Vec::new(),
            aabb_min: [-100.0, -100.0, 0.0],
            aabb_max: [100.0, 100.0, 0.0],
            wire_color: [255; 3],
            ..Default::default()
        },
    );
    let p = ViewParams {
        size: [100, 100],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 200.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            fov_y_deg: 45.0,
            near: 1.0,
            far: 1000.0,
        },
        ..params(persp(), RenderMode::Textured, false)
    };
    r.render_view(1, &p, &Overlay::default());
    let img = save(&r, 1, "blend_texture2");
    let left = pixel(&img, 2, 50);
    let mid = pixel(&img, 50, 50);
    let right = pixel(&img, 97, 50);
    assert!(left[0] > 150 && left[2] < 110, "left is mostly the first texture: {left:?}");
    assert!(right[2] > 150 && right[0] < 110, "right is mostly the second texture: {right:?}");
    assert!(mid[0] > 60 && mid[2] > 60, "middle is blended: {mid:?}");
}

#[test]
fn texture_alpha_follows_the_material_flags() {
    let (device, queue) = gpu_or_skip!();
    let (mut r, p) = two_quads(&device, &queue);
    let red = r.create_texture(&checker(4, 2, [255, 0, 0, 255], [255, 0, 0, 255]));
    let blue = r.create_texture(&checker(4, 2, [0, 0, 255, 64], [0, 0, 255, 64]));
    r.set_material(
        1,
        &MaterialDesc { texture: Some(red), tex_size: [4.0, 4.0], flags: MaterialFlags::UNLIT, ..Default::default() },
    );
    let f = |extra: MaterialFlags| MaterialFlags::UNLIT | extra;
    let cases = [
        (f(MaterialFlags::TRANSLUCENT), 0.5, 224, 137),
        (f(MaterialFlags::TRANSLUCENT | MaterialFlags::OPAQUE_TEXTURE), 0.5, 0, 255),
        (f(MaterialFlags::OPAQUE_TEXTURE), 0.5, 0, 255),
        (f(MaterialFlags::empty()), 0.5, 0, 255),
        (f(MaterialFlags::ALPHA_TEST), 0.2, 0, 255),
        (f(MaterialFlags::ALPHA_TEST), 0.3, 255, 0),
        (f(MaterialFlags::ALPHA_TEST | MaterialFlags::TRANSLUCENT), 0.3, 255, 0),
        (f(MaterialFlags::ALPHA_TEST | MaterialFlags::TRANSLUCENT), 0.2, 224, 137),
        (f(MaterialFlags::ALPHA_TEST | MaterialFlags::OPAQUE_TEXTURE), 0.7, 0, 255),
        (f(MaterialFlags::ADDITIVE | MaterialFlags::OPAQUE_TEXTURE), 0.5, 255, 255),
        (f(MaterialFlags::ADDITIVE), 0.5, 255, 137),
    ];
    for (i, (flags, alpha_ref, want_red, want_blue)) in cases.into_iter().enumerate() {
        r.set_material(
            2,
            &MaterialDesc { texture: Some(blue), tex_size: [4.0, 4.0], flags, alpha_ref, ..Default::default() },
        );
        r.render_view(1, &p, &Overlay::default());
        let img = save(&r, 1, &format!("texture_alpha_{i}"));
        let c = pixel(&img, 32, 32);
        let near = |a: u8, b: i32| (i32::from(a) - b).abs() <= 24;
        assert!(near(c[0], want_red) && near(c[2], want_blue), "case {i} {flags:?} ref {alpha_ref}: {c:?}");
    }
}

fn two_quads(device: &wgpu::Device, queue: &wgpu::Queue) -> (Renderer, ViewParams) {
    let mut r = Renderer::new(device, queue);
    let quad = |z: f32, material: u32| {
        let v = |x: f32, y: f32| Vertex { pos: [x, y, z], normal: [0.0, 0.0, 1.0], uv: [x, y], alpha: 0.0 };
        ObjectMesh {
            batches: vec![MeshBatch {
                material,
                vertices: vec![v(-100.0, -100.0), v(100.0, -100.0), v(100.0, 100.0), v(-100.0, 100.0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            edges: Vec::new(),
            aabb_min: [-100.0, -100.0, z],
            aabb_max: [100.0, 100.0, z],
            wire_color: [255; 3],
            ..Default::default()
        }
    };
    r.upsert_object(1, quad(0.0, 1));
    r.upsert_object(2, quad(10.0, 2));
    let p = ViewParams {
        size: [64, 64],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 200.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            fov_y_deg: 45.0,
            near: 1.0,
            far: 1000.0,
        },
        ..params(persp(), RenderMode::Textured, false)
    };
    (r, p)
}

#[test]
fn modulate_multiplies_what_is_behind() {
    let (device, queue) = gpu_or_skip!();
    let (mut r, p) = two_quads(&device, &queue);
    let grey = r.create_texture(&checker(4, 2, [128, 128, 128, 255], [128, 128, 128, 255]));
    r.set_material(
        1,
        &MaterialDesc { texture: Some(grey), tex_size: [4.0, 4.0], flags: MaterialFlags::UNLIT, ..Default::default() },
    );
    let tex = |r: &mut Renderer, v: u8, a: u8| r.create_texture(&checker(4, 2, [v, v, v, a], [v, v, v, a]));
    let (dark, half, white, cut) =
        (tex(&mut r, 64, 255), tex(&mut r, 128, 255), tex(&mut r, 255, 255), tex(&mut r, 64, 0));
    let cases = [
        (dark, MaterialFlags::DECAL_MODULATE, 64),
        (half, MaterialFlags::DECAL_MODULATE, 128),
        (cut, MaterialFlags::DECAL_MODULATE, 128),
        (half, MaterialFlags::MODULATE, 64),
        (white, MaterialFlags::MODULATE, 128),
        (half, MaterialFlags::MODULATE | MaterialFlags::MODULATE_2X, 128),
        (dark, MaterialFlags::MODULATE | MaterialFlags::MODULATE_2X, 64),
    ];
    for (i, (t, flags, want)) in cases.into_iter().enumerate() {
        r.set_material(2, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], flags, ..Default::default() });
        r.render_view(1, &p, &Overlay::default());
        let img = save(&r, 1, &format!("modulate_{i}"));
        let c = pixel(&img, 32, 32);
        assert!((i32::from(c[0]) - want).abs() <= 8 && c[0] == c[2], "case {i} {flags:?}: {c:?}");
    }
}

#[test]
fn detail_textures_combine_like_the_game() {
    let (device, queue) = gpu_or_skip!();
    let (mut r, p) = two_quads(&device, &queue);
    let tex = |r: &mut Renderer, v: u8| r.create_texture(&checker(4, 2, [v, v, v, 255], [v, v, v, 255]));
    let (white, black, grey, dark) = (tex(&mut r, 255), tex(&mut r, 0), tex(&mut r, 128), tex(&mut r, 64));
    r.set_material(
        1,
        &MaterialDesc { texture: Some(black), tex_size: [4.0, 4.0], flags: MaterialFlags::UNLIT, ..Default::default() },
    );
    let detail = |texture: TexId, mode: u32, raw: bool, tint: f32, f: f32| DetailDesc {
        texture,
        mode,
        raw,
        sets_alpha: false,
        uv: [4.0, 4.0, 0.0, 0.0],
        tint_blend: [tint, tint, tint, f],
    };
    let cases = [
        (white, None, 255),
        (white, Some(detail(dark, DetailDesc::MODE_MOD2X, true, 1.0, 1.0)), 188),
        (white, Some(detail(grey, DetailDesc::MODE_MULTIPLY, false, 1.0, 1.0)), 128),
        (white, Some(detail(grey, DetailDesc::MODE_MULTIPLY, false, 0.5, 1.0)), 95),
        (white, Some(detail(black, DetailDesc::MODE_FADE, false, 1.0, 0.5)), 188),
        (black, Some(detail(grey, DetailDesc::MODE_ADDITIVE_SELFILLUM, false, 1.0, 1.0)), 128),
        (black, Some(detail(grey, DetailDesc::MODE_ADDITIVE, false, 1.0, 1.0)), 128),
        (white, Some(detail(black, 99, false, 1.0, 1.0)), 255),
    ];
    for (i, (base, d, want)) in cases.into_iter().enumerate() {
        let desc = MaterialDesc {
            texture: Some(base),
            tex_size: [4.0, 4.0],
            flags: MaterialFlags::UNLIT,
            detail: d,
            ..Default::default()
        };
        r.set_material(2, &desc);
        r.render_view(1, &p, &Overlay::default());
        let img = save(&r, 1, &format!("detail_{i}"));
        let c = pixel(&img, 32, 32);
        assert!((i32::from(c[0]) - want).abs() <= 8, "case {i}: {c:?}, want {want}");
    }
}

#[test]
fn sun_casts_shadows_in_lit_mode() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    r.set_material(1, &MaterialDesc { texture: Some(TexId::WHITE), tex_size: [1.0, 1.0], ..Default::default() });
    r.upsert_object(1, box_mesh([-1024.0, -1024.0, -16.0], [1024.0, 1024.0, 0.0], 1, [255; 3]));
    r.upsert_object(2, box_mesh([-128.0, -128.0, 256.0], [128.0, 128.0, 288.0], 1, [255; 3]));
    r.set_lights(
        &[PreviewLight {
            kind: LightKind::Directional { dir: [0.0, 0.0, -1.0] },
            pos: [0.0; 3],
            color: [1.0, 1.0, 1.0],
            intensity: 0.8,
            range: 0.0,
            falloff: None,
        }],
        [0.05; 3],
    );
    let p = ViewParams {
        size: [256, 256],
        kind: ViewKind::Perspective {
            eye: [0.0, -1400.0, 1400.0],
            forward: [0.0, 1.0, -1.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 60.0,
            near: 4.0,
            far: 10000.0,
        },
        ..params(persp(), RenderMode::Lit, true)
    };
    r.render_view(6, &p, &Overlay::default());
    let img = save(&r, 6, "lit_sun_shadow");
    let under = p.world_to_screen([0.0, -64.0, 0.0]).expect("visible");
    let beside = p.world_to_screen([400.0, -64.0, 0.0]).expect("visible");
    let a = pixel(&img, under[0] as u32, under[1] as u32);
    let b = pixel(&img, beside[0] as u32, beside[1] as u32);
    assert!(i32::from(b[0]) > i32::from(a[0]) + 80, "shadowed {a:?} vs lit {b:?}");
    r.set_object_flags(2, ObjectFlags::HIDDEN);
    r.render_view(6, &p, &Overlay::default());
    let img = save(&r, 6, "lit_sun_shadow_hidden");
    let c = pixel(&img, under[0] as u32, under[1] as u32);
    assert!(i32::from(c[0]) > i32::from(a[0]) + 80, "no shadow once hidden: {c:?}");
}

fn color_near(img: &(u32, u32, Vec<u8>), x: u32, y: u32, rgb: [u8; 3], tol: i32) -> bool {
    let (x0, x1) = (x.saturating_sub(2), (x + 2).min(img.0 - 1));
    (x0..=x1).any(|xx| {
        let p = pixel(img, xx, y);
        (0..3).all(|i| (i32::from(p[i]) - i32::from(rgb[i])).abs() <= tol)
    })
}

#[test]
fn overlays_crossing_the_camera_plane_stay_visible() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let kind = ViewKind::Perspective {
        eye: [0.0, 0.0, 64.0],
        forward: [1.0, 0.0, -0.35],
        up: [0.0, 0.0, 1.0],
        fov_y_deg: 70.0,
        near: 4.0,
        far: 20000.0,
    };
    let p = ViewParams {
        size: [256, 256],
        kind,
        grid: None,
        background: [0.0; 3],
        ..params(persp(), RenderMode::Flat, false)
    };
    let floor = [[-2000.0f32, -2000.0, 0.0], [2000.0, -2000.0, 0.0], [2000.0, 2000.0, 0.0], [-2000.0, 2000.0, 0.0]];
    let magenta = [1.0, 0.0, 1.0, 1.0];
    let tris = [
        OverlayTri { p: [floor[0], floor[1], floor[2]], color: magenta },
        OverlayTri { p: [floor[0], floor[2], floor[3]], color: magenta },
    ];
    let line = OverlayLine { a: [-2000.0, 0.0, 0.0], b: [2000.0, 0.0, 0.0], color: [0.0, 1.0, 0.0, 1.0] };
    for on_top in [false, true] {
        let name = if on_top { "on_top" } else { "depth" };
        let mut ov = Overlay::default();
        if on_top {
            ov.triangles_on_top.extend(tris);
        } else {
            ov.triangles.extend(tris);
        }
        r.render_view(9, &p, &ov);
        let img = save(&r, 9, &format!("camera_plane_tris_{name}"));
        for y in [100, 160, 220, 254] {
            for x in [10, 128, 245] {
                let c = pixel(&img, x, y);
                assert!(c[0] > 200 && c[2] > 200 && c[1] < 40, "{name}: floor tint missing at ({x}, {y}): {c:?}");
            }
        }
        let mut ov = Overlay::default();
        if on_top {
            ov.lines_on_top.push(line);
        } else {
            ov.lines.push(line);
        }
        r.render_view(9, &p, &ov);
        let img = save(&r, 9, &format!("camera_plane_line_{name}"));
        for y in [100, 160, 220, 254] {
            assert!(color_near(&img, 128, y, [0, 255, 0], 60), "{name}: outline missing at row {y}");
        }
    }
}

fn quad_tris(half: f32, z: f32, color: [f32; 4]) -> [OverlayTri; 2] {
    let c = [[-half, -half, z], [half, -half, z], [half, half, z], [-half, half, z]];
    [OverlayTri { p: [c[0], c[1], c[2]], color }, OverlayTri { p: [c[0], c[2], c[3]], color }]
}

#[test]
fn retained_overlay_is_drawn_every_frame_until_replaced() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let p = ViewParams {
        size: [128, 128],
        grid: None,
        background: [0.0; 3],
        ..params(ortho(OrthoAxis::Top), RenderMode::Wireframe, false)
    };
    let magenta = [1.0, 0.0, 1.0, 1.0];
    let mut retained = Overlay::default();
    retained.triangles.extend(quad_tris(100.0, 0.0, magenta));
    retained.points.push(OverlayPoint { pos: [-150.0, -150.0, 0.0], color: [1.0, 1.0, 0.0, 1.0], size_px: 9.0 });
    let uploads = r.stats().retained_uploads;
    r.set_retained_overlay(30, &retained);
    assert!(r.has_retained_overlay(30));
    let empty = Overlay::default();
    for frame in 0..3 {
        r.render_view(30, &p, &empty);
        let img = save(&r, 30, "retained_frames");
        assert!(color_near(&img, 64, 64, [255, 0, 255], 30), "frame {frame}: retained overlay missing");
    }
    let s = r.stats();
    assert_eq!(s.retained_uploads, uploads + 1, "drawing does not upload again");
    assert!(s.retained_overlay_bytes > 0 && s.approx_gpu_bytes >= s.retained_overlay_bytes);
    let img = save(&r, 30, "retained_points");
    assert!(color_near(&img, 12, 116, [255, 255, 0], 40), "retained point missing");

    let mut per_frame = Overlay::default();
    per_frame.triangles.extend(quad_tris(20.0, 0.0, [0.0, 1.0, 0.0, 1.0]));
    r.render_view(30, &p, &per_frame);
    let img = save(&r, 30, "retained_under_per_frame");
    assert!(color_near(&img, 64, 64, [0, 255, 0], 30), "per-frame overlay must be on top");
    assert!(color_near(&img, 64, 40, [255, 0, 255], 30), "retained overlay still around it");

    let mut cyan = Overlay::default();
    cyan.triangles.extend(quad_tris(100.0, 0.0, [0.0, 1.0, 1.0, 1.0]));
    r.set_retained_overlay(30, &cyan);
    r.render_view(30, &p, &empty);
    let img = save(&r, 30, "retained_replaced");
    assert!(color_near(&img, 64, 64, [0, 255, 255], 30), "replaced retained overlay missing");
    r.set_retained_overlay(30, &empty);
    assert!(!r.has_retained_overlay(30));
    r.render_view(30, &p, &empty);
    let img = save(&r, 30, "retained_cleared");
    assert!(pixel(&img, 64, 64)[..3].iter().all(|c| *c < 20), "cleared retained overlay still drawn");
    r.set_retained_overlay(30, &cyan);
    r.clear_retained_overlay(30);
    assert!(!r.has_retained_overlay(30));
    r.set_retained_overlay(30, &cyan);
    r.remove_view(30);
    assert!(!r.has_retained_overlay(30));
    assert_eq!(r.stats().retained_overlay_bytes, 0);
}

#[test]
fn retained_overlays_are_per_view_and_culled() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let kind = ViewKind::Perspective {
        eye: [0.0, 0.0, 300.0],
        forward: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
        fov_y_deg: 60.0,
        near: 4.0,
        far: 20000.0,
    };
    let p = ViewParams { size: [128, 128], grid: None, background: [0.0; 3], ..params(kind, RenderMode::Flat, false) };
    let mut ov = Overlay::default();
    let red = [1.0, 0.0, 0.0, 1.0];
    for i in 0..3000 {
        let x = (i % 60) as f32 * 4.0 - 120.0;
        let y = (i / 60) as f32 * 4.0 - 100.0;
        ov.triangles.push(OverlayTri { p: [[x, y, 0.0], [x + 4.0, y, 0.0], [x, y + 4.0, 0.0]], color: red });
    }
    for i in 0..3000 {
        let x = i as f32;
        ov.triangles.push(OverlayTri { p: [[x, 0.0, 900.0], [x + 4.0, 0.0, 900.0], [x, 4.0, 900.0]], color: red });
    }
    r.set_retained_overlay(31, &ov);
    let empty = Overlay::default();
    r.render_view(31, &p, &empty);
    let img = save(&r, 31, "retained_culled");
    assert!(color_near(&img, 64, 64, [255, 0, 0], 30), "visible retained triangles missing");
    let tris = r.stats().triangles;
    assert!((3000..6000).contains(&tris), "the chunks behind the camera are culled: {tris} triangles drawn");
    r.render_view(32, &p, &empty);
    let img = save(&r, 32, "retained_other_view");
    assert!(pixel(&img, 64, 64)[..3].iter().all(|c| *c < 20), "retained overlays belong to one view");
}

#[test]
#[ignore = "performance test: cargo test -p hp-render --release -- --ignored perf"]
fn perf_retained_overlay_300k_faces() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let fill = [1.0, 0.2, 0.2, 0.35];
    let line = [1.0, 1.0, 0.0, 1.0];
    let mut ov = Overlay::default();
    let mut faces: Vec<[f32; 3]> = (0..300_000)
        .map(|_| [rng.range(-15000.0, 15000.0), rng.range(-15000.0, 15000.0), rng.range(-2000.0, 2000.0)])
        .collect();
    faces.sort_by_key(|p| ((p[0] / 1024.0).floor() as i32, (p[1] / 1024.0).floor() as i32));
    for p in &faces {
        let c = [
            [p[0], p[1], p[2]],
            [p[0] + 64.0, p[1], p[2]],
            [p[0] + 64.0, p[1] + 64.0, p[2]],
            [p[0], p[1] + 64.0, p[2]],
        ];
        ov.triangles.push(OverlayTri { p: [c[0], c[1], c[2]], color: fill });
        ov.triangles.push(OverlayTri { p: [c[0], c[2], c[3]], color: fill });
        for k in 0..4 {
            ov.lines.push(OverlayLine { a: c[k], b: c[(k + 1) % 4], color: line });
        }
    }
    let persp = ViewKind::Perspective {
        eye: [-16000.0, -16000.0, 6000.0],
        forward: [1.0, 1.0, -0.35],
        up: [0.0, 0.0, 1.0],
        fov_y_deg: 75.0,
        near: 4.0,
        far: 65536.0,
    };
    let ortho_view = |axis| ViewParams {
        size: [960, 540],
        ..params(ViewKind::Ortho { axis, center: [0.0; 3], pixels_per_unit: 0.03 }, RenderMode::Wireframe, false)
    };
    let views = [
        ViewParams { size: [1280, 720], ..params(persp, RenderMode::Textured, true) },
        ortho_view(OrthoAxis::Top),
        ortho_view(OrthoAxis::Front),
        ortho_view(OrthoAxis::Side),
    ];
    let empty = Overlay::default();
    let frames = 60;
    let run = |r: &mut Renderer, per_frame: &Overlay| {
        for (i, v) in views.iter().enumerate() {
            r.render_view(i as u32, v, per_frame);
        }
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let t = Instant::now();
        for _ in 0..frames {
            for (i, v) in views.iter().enumerate() {
                r.render_view(i as u32, v, per_frame);
            }
            device.poll(wgpu::PollType::wait_indefinitely()).ok();
        }
        t.elapsed().as_secs_f64() * 1000.0 / f64::from(frames)
    };
    let per_frame_ms = run(&mut r, &ov);
    let t = Instant::now();
    for i in 0..4 {
        r.set_retained_overlay(i, &ov);
    }
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    let upload_ms = t.elapsed().as_secs_f64() * 1000.0;
    let retained_ms = run(&mut r, &empty);
    save(&r, 0, "perf_retained_3d");
    save(&r, 1, "perf_retained_top");
    let s = r.stats();
    eprintln!(
        "PERF 300k faces, 4 views: per-frame overlay {per_frame_ms:.2} ms/frame, retained {retained_ms:.2} ms/frame \
         (upload of the 4 views {upload_ms:.1} ms, {:.1} MB GPU)",
        s.retained_overlay_bytes as f64 / 1048576.0
    );
    assert!(retained_ms < per_frame_ms, "the retained overlay must be cheaper than re-uploading it");
}

fn count_pixels(img: &(u32, u32, Vec<u8>), f: impl Fn([u8; 3]) -> bool) -> usize {
    img.2.chunks_exact(4).filter(|p| f([p[0], p[1], p[2]])).count()
}

#[test]
fn collision_only_draws_hulls_instead_of_models() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    let with_hull = r.create_model(&pyramid(MAT_MODEL));
    let without = r.create_model(&pyramid(MAT_MODEL));
    let part = &pyramid(MAT_MODEL)[0];
    let hull: Vec<[[f32; 3]; 3]> =
        part.indices.chunks_exact(3).map(|t| [0, 1, 2].map(|k| part.vertices[t[k] as usize].pos)).collect();
    r.set_model_collision_hull(with_hull, &hull);
    r.set_model_no_collision(without);
    let t = |x: f32| [[2.0, 0.0, 0.0, 0.0], [0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 2.0, 0.0], [x, 0.0, 0.0, 1.0]];
    r.set_model_instances(&[
        ModelInstance { model: with_hull, transform: t(-120.0), object: 300, tint: [1.0; 4], fade: [0.0; 2] },
        ModelInstance { model: without, transform: t(120.0), object: 301, tint: [1.0; 4], fade: [0.0; 2] },
    ]);
    let eye = [0.0f32, -700.0, 260.0];
    let kind = ViewKind::Perspective {
        eye,
        forward: [0.0, 700.0, -200.0],
        up: [0.0, 0.0, 1.0],
        fov_y_deg: 60.0,
        near: 4.0,
        far: 20000.0,
    };
    let mut p = params(kind, RenderMode::Flat, false);
    p.grid = None;
    let orange = |c: [u8; 3]| c[0] > 200 && c[1] > 60 && c[1] < 200 && c[2] < 90;
    let green = |c: [u8; 3]| c[1] > 90 && c[1] > c[2] + 40 && c[0] + 20 < c[1] + 60;

    r.render_view(0, &p, &Overlay::default());
    let normal = save(&r, 0, "collision_off");
    assert_eq!(count_pixels(&normal, orange), 0, "no orange box while the models are shown");

    r.set_view_options(0, ViewOptions { collision_only: true, ..Default::default() });
    r.render_view(0, &p, &Overlay::default());
    let only = save(&r, 0, "collision_only");
    assert!(count_pixels(&only, orange) > 20, "the model without collision model is an orange box");
    assert!(count_pixels(&only, green) > 200, "the hull is drawn green");
    assert_ne!(normal.2, only.2, "the models are not drawn");
}
