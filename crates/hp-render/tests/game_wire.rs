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
        let features =
            adapter.features() & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::POLYGON_MODE_LINE);
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hp-render game wire test"),
            required_features: features,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

type Img = (u32, u32, Vec<u8>);

const SIZE: u32 = 240;
const FOV: f32 = 60.0;
const PINK: [u8; 3] = [231, 129, 129];
const GREEN: [u8; 3] = [0, 255, 129];
const CYAN: [u8; 3] = [0, 255, 255];

const WORLD: MaterialId = 1;
const PROP: MaterialId = 2;
const DISP: MaterialId = 3;

fn save(r: &Renderer, name: &str) -> Img {
    let (w, h, px) = r.read_view_rgba8(1).expect("readback");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hp-render-scratch");
    let _ = std::fs::create_dir_all(&dir);
    image::save_buffer(dir.join(format!("gamewire_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
    (w, h, px)
}

fn near(p: &[u8], rgb: [u8; 3], tol: i32) -> bool {
    (0..3).all(|i| (i32::from(p[i]) - i32::from(rgb[i])).abs() <= tol)
}

fn count(img: &Img, rgb: [u8; 3], tol: i32, rect: Option<[u32; 4]>) -> usize {
    let [x0, y0, x1, y1] = rect.unwrap_or([0, 0, img.0, img.1]);
    let mut n = 0;
    for y in y0..y1.min(img.1) {
        for x in x0..x1.min(img.0) {
            let o = ((y * img.0 + x) * 4) as usize;
            if near(&img.2[o..o + 3], rgb, tol) {
                n += 1;
            }
        }
    }
    n
}

fn project(eye: [f32; 3], p: [f32; 3]) -> (u32, u32) {
    let depth = p[1] - eye[1];
    let t = (FOV.to_radians() * 0.5).tan();
    let x = SIZE as f32 * 0.5 * (1.0 + (p[0] - eye[0]) / (depth * t));
    let y = SIZE as f32 * 0.5 * (1.0 - (p[2] - eye[2]) / (depth * t));
    (x.round() as u32, y.round() as u32)
}

fn view(eye: [f32; 3]) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [SIZE, SIZE],
        kind: ViewKind::Perspective {
            eye,
            forward: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: FOV,
            near: 1.0,
            far: 30000.0,
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

fn mat(r: &mut Renderer, id: MaterialId, rgba: [u8; 4]) {
    let up = TextureUpload { width: 4, height: 4, format: TexFormat::Rgba8Srgb, levels: vec![rgba.repeat(16)] };
    let t = r.create_texture(&up);
    r.set_material(id, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], ..Default::default() });
}

fn quad(x0: f32, x1: f32, z0: f32, z1: f32, y: f32, ny: f32, material: MaterialId) -> ObjectMesh {
    let n = [0.0, ny, 0.0];
    let v = |x: f32, z: f32| Vertex { pos: [x, y, z], normal: n, uv: [x, z], alpha: 1.0 };
    let vertices = vec![v(x0, z0), v(x1, z0), v(x1, z1), v(x0, z1)];
    ObjectMesh {
        batches: vec![MeshBatch {
            material,
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
            blend: Vec::new(),
            cubemaps: Vec::new(),
        }],
        aabb_min: [x0, y, z0],
        aabb_max: [x1, y, z1],
        wire_color: [200; 3],
        ..Default::default()
    }
}

fn tri(x0: f32, x1: f32, z1: f32) -> ModelPart {
    let n = [0.0, -1.0, 0.0];
    let v = |x: f32, z: f32| Vertex { pos: [x, 0.0, z], normal: n, uv: [0.0, 0.0], alpha: 1.0 };
    ModelPart {
        material: PROP,
        vertices: vec![v(x0, 0.0), v(x1, 0.0), v(x0, z1)],
        indices: vec![0, 1, 2],
        tangents: Vec::new(),
    }
}

#[test]
fn game_wireframe_levels_follow_garrys_mod() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let lines = device.features().contains(wgpu::Features::POLYGON_MODE_LINE);
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [128, 128, 128, 255]);
    mat(&mut r, PROP, [255, 0, 0, 255]);
    mat(&mut r, DISP, [0, 0, 255, 255]);
    r.upsert_object(1, quad(0.0, 200.0, -100.0, 100.0, 200.0, -1.0, WORLD));
    r.upsert_object(2, quad(-150.0, -60.0, -100.0, -40.0, 150.0, -1.0, DISP));
    r.set_object_flags(2, ObjectFlags::DISPLACEMENT);
    r.upsert_object(3, quad(-150.0, -60.0, 40.0, 100.0, 150.0, 1.0, WORLD));
    let model = r.create_model_lods(&[
        ModelLod { switch_point: 0.0, parts: vec![tri(-40.0, -5.0, 40.0)] },
        ModelLod { switch_point: 100.0, parts: vec![tri(5.0, 40.0, 40.0)] },
    ]);
    assert_eq!(r.model_lod_count(model), 2);
    let mut m = ModelInstance { model, object: 10, ..Default::default() };
    m.transform[3] = [-40.0, 0.0, -20.0, 1.0];
    r.set_model_instances(&[m]);
    let eye = [0.0, -200.0, 0.0];
    let render = |r: &mut Renderer, level: u8, eye: [f32; 3], name: &str| -> Img {
        r.set_view_options(1, ViewOptions { game_wireframe: level, ..ViewOptions::default() });
        r.render_view(1, &view(eye), &Overlay::default());
        save(r, name)
    };

    let img = render(&mut r, 0, eye, "off");
    assert!(img.2.chunks_exact(4).filter(|p| p[2] > 60 && p[0] < 20).count() > 100, "displacement drawn");
    assert!(img.2.chunks_exact(4).filter(|p| p[0] > 60 && p[2] < 20 && p[1] < 20).count() > 50, "prop drawn");
    assert_eq!(count(&img, PINK, 8, None), 0);

    let (dx, dy) = project(eye, [100.0, 200.0, 0.0]);
    let diag = [dx - 2, dy - 2, dx + 3, dy + 3];
    let (ox, oy) = project(eye, [100.0, 200.0, 100.0]);
    let outline = [ox - 3, oy - 2, ox + 3, oy + 3];
    let (bx0, by0) = project(eye, [-140.0, 150.0, 90.0]);
    let (bx1, by1) = project(eye, [-70.0, 150.0, 50.0]);
    let away = [bx0, by0, bx1, by1];

    for level in [1u8, 2, 3] {
        let img = render(&mut r, level, eye, &format!("level{level}"));
        let blue = img.2.chunks_exact(4).filter(|p| p[2] > 60 && p[0] < 20 && p[1] < 20).count();
        assert_eq!(blue, 0, "level {level}: no displacement");
        let red = img.2.chunks_exact(4).filter(|p| p[0] > 60 && p[1] < 20 && p[2] < 20).count();
        assert_eq!(red, 0, "level {level}: the prop is not filled");
        assert!(count(&img, CYAN, 8, None) > 20, "level {level}: the prop as lines");
        let grey = img.2.chunks_exact(4).filter(|p| p[0] > 30 && p[0] == p[1] && p[1] == p[2]).count();
        assert!(grey > 1000, "level {level}: the wall keeps its texture ({grey})");
        let color = if level == 2 { GREEN } else { PINK };
        let other = if level == 2 { PINK } else { GREEN };
        assert_eq!(count(&img, other, 8, None), 0, "level {level}");
        assert!(count(&img, color, 8, Some(outline)) > 0, "level {level}: outline");
        assert_eq!(count(&img, color, 8, Some(away)), 0, "level {level}: no lines on faces turned away");
        let on_diagonal = count(&img, color, 8, Some(diag)) > 0;
        if level == 3 || !lines {
            assert!(!on_diagonal, "level {level}: no inner triangles");
        } else {
            assert!(on_diagonal, "level {level}: the triangles of the fan");
        }
    }

    let left = |eye: [f32; 3]| {
        let (x0, y0) = project(eye, [-82.0, 0.0, 22.0]);
        let (x1, y1) = project(eye, [-43.0, 0.0, -22.0]);
        [x0, y0, x1, y1]
    };
    let right = |eye: [f32; 3]| {
        let (x0, y0) = project(eye, [-37.0, 0.0, 22.0]);
        let (x1, y1) = project(eye, [2.0, 0.0, -22.0]);
        [x0, y0, x1, y1]
    };
    let img = render(&mut r, 1, eye, "lod_near");
    assert!(count(&img, CYAN, 8, Some(left(eye))) > 10, "LOD 0 near");
    assert_eq!(count(&img, CYAN, 8, Some(right(eye))), 0, "not LOD 1 near");
    let far = [0.0, -1000.0, 0.0];
    let img = render(&mut r, 1, far, "lod_far");
    assert_eq!(count(&img, CYAN, 8, Some(left(far))), 0, "not LOD 0 far");
    assert!(count(&img, CYAN, 8, Some(right(far))) > 3, "LOD 1 far");
    r.set_model_lods(model, &[ModelLod { switch_point: 0.0, parts: vec![tri(-40.0, -5.0, 40.0)] }]);
    assert_eq!(r.model_lod_count(model), 1);
    let img = render(&mut r, 1, far, "lod_single");
    assert!(count(&img, CYAN, 8, Some(left(far))) > 3, "a single LOD is always drawn");
}

#[test]
fn detail_sprites_fade_turn_and_show_as_lines() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [255, 0, 0, 255]);
    let sprite = |origin: [f32; 3], orientation, object| DetailSprite {
        origin,
        right: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        ul: [-30.0, 60.0],
        lr: [30.0, 0.0],
        tex: [0.0, 0.0, 1.0, 1.0],
        orientation,
        sway: 0,
        color: [255; 4],
        object,
    };
    r.set_detail_sprites(
        PROP,
        &[
            sprite([-60.0, 300.0, -30.0], DetailSpriteOrientation::Vertical, 20),
            sprite([60.0, 300.0, -30.0], DetailSpriteOrientation::Fixed, 21),
            sprite([0.0, 1500.0, -30.0], DetailSpriteOrientation::Vertical, 22),
        ],
    );
    assert_eq!(r.detail_sprite_count(), 3);
    let eye = [0.0, 0.0, 0.0];
    let red = |img: &Img, rect: [u32; 4]| count(img, [255, 0, 0], 40, Some(rect));
    let (lx, ly) = project(eye, [-60.0, 300.0, 0.0]);
    let (rx, ry) = project(eye, [60.0, 300.0, 0.0]);
    let (fx, fy) = project(eye, [0.0, 1500.0, 0.0]);
    let around = |x: u32, y: u32, h: u32| [x.saturating_sub(h), y.saturating_sub(h), x + h, y + h];
    r.set_view_options(1, ViewOptions::default());
    r.render_view(1, &view(eye), &Overlay::default());
    let img = save(&r, "detail_off");
    assert!(red(&img, around(lx, ly, 5)) > 50 && red(&img, around(rx, ry, 5)) > 50, "both sprites drawn");
    assert_eq!(red(&img, around(fx, fy, 3)), 0, "beyond cl_detaildist");
    let mut dim = sprite([-60.0, 300.0, -30.0], DetailSpriteOrientation::Vertical, 20);
    dim.color = [128, 128, 128, 255];
    r.set_detail_sprites(PROP, &[dim]);
    r.render_view(1, &view(eye), &Overlay::default());
    let img = save(&r, "detail_tinted");
    assert!(count(&img, [128, 0, 0], 20, Some(around(lx, ly, 5))) > 50, "tinted by the lighting");
    r.set_detail_sprites(
        PROP,
        &[
            sprite([-60.0, 300.0, -30.0], DetailSpriteOrientation::Vertical, 20),
            sprite([60.0, 300.0, -30.0], DetailSpriteOrientation::Fixed, 21),
            sprite([0.0, 1500.0, -30.0], DetailSpriteOrientation::Vertical, 22),
        ],
    );
    r.set_object_flags(20, ObjectFlags::HIDDEN);
    r.render_view(1, &view(eye), &Overlay::default());
    let img = save(&r, "detail_hidden");
    assert_eq!(red(&img, around(lx, ly, 5)), 0);
    r.set_object_flags(20, ObjectFlags::empty());
    r.set_view_options(1, ViewOptions { game_wireframe: 1, ..ViewOptions::default() });
    r.render_view(1, &view(eye), &Overlay::default());
    let img = save(&r, "detail_wire");
    assert_eq!(count(&img, [255, 0, 0], 40, None), 0, "no texture in the wireframe");
    assert!(count(&img, [0, 255, 0], 8, None) > 40, "green lines");
    r.set_detail_sprites(PROP, &[]);
    assert_eq!(r.detail_sprite_count(), 0);
}

#[test]
fn brush_entities_cover_the_world_lines_behind_them() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [128, 128, 128, 255]);
    mat(&mut r, PROP, [0, 0, 200, 255]);
    r.upsert_object(1, quad(0.0, 200.0, -100.0, 100.0, 200.0, -1.0, WORLD));
    r.upsert_object(2, quad(50.0, 150.0, -50.0, 50.0, 100.0, -1.0, PROP));
    let eye = [0.0, -200.0, 0.0];
    let (x, y) = project(eye, [80.0, 200.0, -20.0]);
    let spot = [x - 2, y - 2, x + 3, y + 3];
    for level in [1u8, 3] {
        r.set_view_options(1, ViewOptions { game_wireframe: level, ..ViewOptions::default() });
        r.set_object_flags(2, ObjectFlags::empty());
        r.render_view(1, &view(eye), &Overlay::default());
        let img = save(&r, &format!("entity_as_world_{level}"));
        if level == 1 && device.features().contains(wgpu::Features::POLYGON_MODE_LINE) {
            assert!(count(&img, PINK, 8, Some(spot)) > 0, "the wall diagonal is seen through");
        }
        r.set_object_flags(2, ObjectFlags::BRUSH_ENTITY);
        r.render_view(1, &view(eye), &Overlay::default());
        let img = save(&r, &format!("entity_{level}"));
        assert_eq!(count(&img, PINK, 8, Some(spot)), 0, "level {level}: covered by the door");
        let (ox, oy) = project(eye, [100.0, 100.0, 50.0]);
        assert!(count(&img, PINK, 8, Some([ox - 3, oy - 2, ox + 3, oy + 3])) > 0, "level {level}: door outline");
    }
}

#[test]
fn detail_sprites_sway_with_time_at_the_top_only() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, PROP, [255, 0, 0, 255]);
    let sprite = |sway| DetailSprite {
        origin: [0.0, 300.0, -30.0],
        right: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        ul: [-30.0, 60.0],
        lr: [30.0, 0.0],
        tex: [0.0, 0.0, 1.0, 1.0],
        orientation: DetailSpriteOrientation::Fixed,
        sway,
        color: [255; 4],
        object: 20,
    };
    let eye = [0.0, 0.0, 0.0];
    let centroid = |img: &Img, rect: [u32; 4]| {
        let (mut sum, mut n) = (0.0f32, 0u32);
        for y in rect[1]..rect[3].min(img.1) {
            for x in rect[0]..rect[2].min(img.0) {
                let i = ((y * img.0 + x) * 4) as usize;
                if near(&img.2[i..i + 4], [255, 0, 0], 40) {
                    sum += x as f32;
                    n += 1;
                }
            }
        }
        (n > 0).then(|| sum / n as f32)
    };
    let (_, top) = project(eye, [0.0, 300.0, 30.0]);
    let (_, bottom) = project(eye, [0.0, 300.0, -30.0]);
    let mid = (top + bottom) / 2;
    let upper = [0, top, SIZE, top + (mid - top) / 2];
    let lower = [0, mid + (bottom - mid) / 2, SIZE, bottom];
    let shot = |r: &mut Renderer, time: f32, name: &str| {
        r.set_view_options(1, ViewOptions { time, ..ViewOptions::default() });
        r.render_view(1, &view(eye), &Overlay::default());
        save(r, name)
    };
    r.set_detail_sprites(PROP, &[sprite(0)]);
    assert!(!r.detail_sprites_sway());
    let still_a = shot(&mut r, 0.0, "sway_off_a");
    let still_b = shot(&mut r, 0.8, "sway_off_b");
    assert_eq!(centroid(&still_a, upper), centroid(&still_b, upper), "no sway, no movement");
    r.set_detail_sprites(PROP, &[sprite(255)]);
    assert!(r.detail_sprites_sway());
    let a = shot(&mut r, 0.0, "sway_a");
    let b = shot(&mut r, 0.8, "sway_b");
    let (ua, ub) = (centroid(&a, upper).expect("top"), centroid(&b, upper).expect("top"));
    assert!((ua - ub).abs() > 2.0, "the top sways: {ua} vs {ub}");
    let (la, lb) = (centroid(&a, lower).expect("foot"), centroid(&b, lower).expect("foot"));
    assert!((la - lb).abs() < (ua - ub).abs() * 0.5, "the foot stays put: {la} vs {lb}");
    r.set_detail_sprites(PROP, &[]);
    assert!(!r.detail_sprites_sway());
}
