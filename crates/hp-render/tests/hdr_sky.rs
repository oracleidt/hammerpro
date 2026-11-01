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
            label: Some("hp-render HDR sky test"),
            required_features: features,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

const SIZE: u32 = 120;

fn view() -> ViewParams {
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
        mode: RenderMode::Textured,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: false,
    }
}

fn render(r: &mut Renderer, player: bool, name: &str) -> u8 {
    r.set_view_options(1, ViewOptions { compiled_lighting: player, show_sky: true, ..ViewOptions::default() });
    r.render_view(1, &view(), &Overlay::default());
    let (w, h, px) = r.read_view_rgba8(1).expect("readback");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hp-render-scratch");
    let _ = std::fs::create_dir_all(&dir);
    image::save_buffer(dir.join(format!("hdr_sky_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
    px[(((h / 2) * w + w / 2) * 4) as usize]
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

#[test]
fn the_sky_of_the_player_view_follows_the_hdr_exposure() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let ldr = 100u8;
    let up = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[ldr, ldr, ldr, 255].repeat(16)],
    };
    let t = r.create_texture(&up);
    r.set_environment(Environment { sky: Some(SkyView::new([Some(t); 6], 0.0)), ..Default::default() });
    r.set_compiled_bloom(false);
    r.set_compiled_lighting(CompiledLightMode::Hdr, &[], &[]);

    close(render(&mut r, false, "editor"), f32::from(ldr), 1.0, "editor sky");

    let sky = linear(ldr);
    close(render(&mut r, true, "player_start"), srgb(sky), 1.5, "sky at exposure 1");
    for _ in 0..12 {
        render(&mut r, true, "player");
        r.step_compiled_exposure(1, 0.5);
    }
    let e = r.compiled_exposure();
    assert!((e - 2.0).abs() < 0.01, "a dark sky raises the exposure: {e}");
    close(render(&mut r, true, "player_exposed"), srgb(sky * e), 1.5, "LDR sky exposed");
    close(render(&mut r, false, "editor_again"), f32::from(ldr), 1.0, "editor sky unscaled");

    let texels = vec![[1.2f32; 3]; 16];
    let face = HdrSkyFace { width: 4, height: 4, texels: &texels };
    r.set_hdr_sky(Some(&[face; 6]));
    assert!(r.has_hdr_sky());
    assert_eq!(render(&mut r, true, "hdr_saturated"), 255, "1.2 x 2 saturates");
    for _ in 0..20 {
        render(&mut r, true, "hdr");
        r.step_compiled_exposure(1, 0.5);
    }
    let e = r.compiled_exposure();
    assert!((1.2 * e - 0.7).abs() < 0.03, "the bright sky brings the exposure down: {e}");
    close(render(&mut r, true, "hdr_settled"), srgb(1.2 * e), 2.0, "HDR sky exposed");
    close(render(&mut r, false, "editor_hdr"), f32::from(ldr), 1.0, "the editor keeps the LDR sky");

    r.set_compiled_lighting(CompiledLightMode::Ldr, &[], &[]);
    assert_eq!(r.step_compiled_exposure(1, 0.5), 1.0);
    close(render(&mut r, true, "ldr_lighting"), f32::from(ldr), 1.0, "LDR lighting sky");
    r.set_hdr_sky(None);
    assert!(!r.has_hdr_sky());
    let short = HdrSkyFace { width: 8, height: 8, texels: &texels };
    r.set_hdr_sky(Some(&[short; 6]));
    assert!(!r.has_hdr_sky());
}

fn quad(y: f32, z0: f32) -> Vec<CompiledVertex> {
    let v = |x: f32, z: f32| CompiledVertex {
        pos: [x, y, z],
        normal: [0.0, -1.0, 0.0],
        uv: [x, z],
        alpha: 0.0,
        lm: [0.5, 0.5],
        lightmap: 0,
        cubemap: hp_render::NO_CUBEMAP,
        blend: Default::default(),
    };
    vec![v(-100.0, z0), v(100.0, z0), v(100.0, z0 + 200.0), v(-100.0, z0 + 200.0)]
}

#[test]
fn the_compiled_skybox_is_drawn_from_the_sky_camera() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    const WALL: MaterialId = 1;
    let up = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[128u8, 128, 128, 255].repeat(16)],
    };
    let t = r.create_texture(&up);
    r.set_material(WALL, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], ..Default::default() });
    let wall = quad(100.0, -1100.0);
    let vertices = wall.iter().map(|v| Vertex { pos: v.pos, normal: v.normal, uv: v.uv, alpha: v.alpha }).collect();
    r.upsert_object(
        1,
        ObjectMesh {
            batches: vec![MeshBatch {
                material: WALL,
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            aabb_min: [-100.0, 100.0, -1100.0],
            aabb_max: [100.0, 100.0, -900.0],
            wire_color: [200; 3],
            ..Default::default()
        },
    );
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT | ObjectFlags::HIDDEN);
    let half = [[0.5f32; 3]];
    let maps = [CompiledLightmap { width: 1, height: 1, texels: &half, styles: &[], bumped: false }];
    let blocks = [CompiledBlock {
        object: 1,
        batches: vec![CompiledBatch { material: WALL, vertices: wall, indices: vec![0, 1, 2, 0, 2, 3] }],
    }];
    r.set_compiled_bloom(false);
    r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    r.set_environment(Environment {
        sky_3d: Some(Sky3d { origin: [0.0, 0.0, -1000.0], scale: 16.0 }),
        ..Default::default()
    });
    let shot = |r: &mut Renderer, name: &str| {
        r.set_view_options(1, ViewOptions { compiled_lighting: true, show_sky_3d: true, ..ViewOptions::default() });
        r.render_view(1, &view(), &Overlay::default());
        let (w, h, px) = r.read_view_rgba8(1).expect("readback");
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hp-render-scratch");
        let _ = std::fs::create_dir_all(&dir);
        image::save_buffer(dir.join(format!("compiled_sky_{name}.png")), &px, w, h, image::ColorType::Rgba8)
            .expect("png");
        px[(((h / 2) * w + w / 2) * 4) as usize]
    };
    assert_eq!(shot(&mut r, "none"), 0, "not in the skybox: hidden");
    r.set_compiled_sky(&[1]);
    assert_eq!(r.compiled_sky(), vec![1]);
    close(shot(&mut r, "sky"), srgb(linear(128) * 0.5), 2.0, "skybox wall");
    r.set_view_options(1, ViewOptions { show_sky_3d: true, ..ViewOptions::default() });
    r.render_view(1, &view(), &Overlay::default());
    let (w, h, px) = r.read_view_rgba8(1).expect("readback");
    assert_eq!(px[(((h / 2) * w + w / 2) * 4) as usize], 0, "editor views keep their skybox");
    r.set_compiled_sky(&[]);
    assert_eq!(shot(&mut r, "cleared"), 0);
}
