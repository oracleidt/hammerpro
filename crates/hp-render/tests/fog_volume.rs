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
            label: Some("hp-render fog volume test"),
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
        image::save_buffer(dir.join(format!("fog_volume_{name}.png")), &px, w, h, image::ColorType::Rgba8)
            .expect("png");
    }
    (w, h, px)
}

fn pixel(img: &Img, x: u32, y: u32) -> [i32; 3] {
    let o = ((y * img.0 + x) * 4) as usize;
    [i32::from(img.2[o]), i32::from(img.2[o + 1]), i32::from(img.2[o + 2])]
}

fn floor(r: &mut Renderer) {
    let t = r.create_texture(&TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[100, 100, 100, 255].repeat(16)],
    });
    r.set_material(1, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], ..Default::default() });
    let corners = [[-4096.0, -4096.0, 0.0], [4096.0, -4096.0, 0.0], [4096.0, 4096.0, 0.0], [-4096.0, 4096.0, 0.0]];
    r.upsert_object(
        1,
        ObjectMesh {
            batches: vec![MeshBatch {
                material: 1,
                vertices: corners
                    .iter()
                    .map(|p| Vertex { pos: *p, normal: [0.0, 0.0, 1.0], uv: [0.0; 2], alpha: 1.0 })
                    .collect(),
                indices: vec![0, 1, 2, 0, 2, 3],
                blend: Vec::new(),
                cubemaps: Vec::new(),
            }],
            aabb_min: [-4096.0, -4096.0, -1.0],
            aabb_max: [4096.0, 4096.0, 1.0],
            wire_color: [255; 3],
            ..Default::default()
        },
    );
}

fn persp(eye: [f32; 3], target: [f32; 3], msaa: bool) -> ViewParams {
    let f = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
    ViewParams {
        game_pixel_grid: false,
        size: [320, 240],
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

const MAGENTA: Fog = Fog {
    color: [1.0, 0.0, 1.0],
    color2: [1.0, 0.0, 1.0],
    dir: [1.0, 0.0, 0.0],
    blend: false,
    start: 0.0,
    end: 256.0,
    max_density: 1.0,
    far_z: 0.0,
};

fn volume(active: bool, fog: Option<Fog>) -> FogVolume {
    FogVolume { min: [-128.0, -128.0, 0.0], max: [128.0, 128.0, 256.0], fog, preview: MAGENTA, active }
}

fn fogged(c: [i32; 3]) -> bool {
    c[0] - c[1] > 50 && c[2] - c[1] > 50
}

fn grey(c: [i32; 3]) -> bool {
    (c[0] - c[1]).abs() < 8 && (c[2] - c[1]).abs() < 8 && c[1] > 40
}

#[test]
fn the_volume_fogs_what_is_seen_through_it() {
    let (device, queue) = gpu_or_skip!();
    for msaa in [false, true] {
        let mut r = Renderer::new(&device, &queue);
        floor(&mut r);
        r.set_fog_volumes(&[volume(true, Some(MAGENTA))]);
        assert_eq!(r.fog_volumes().len(), 1);
        let p = persp([0.0, -600.0, 300.0], [0.0, 0.0, 0.0], msaa);
        r.render_view(1, &p, &Overlay::default());
        let off = grab(&r, 1, &format!("off_{msaa}"));
        assert!(grey(pixel(&off, 160, 120)), "preview off: {:?}", pixel(&off, 160, 120));
        r.set_view_options(1, ViewOptions { fog_volumes: true, ..Default::default() });
        r.render_view(1, &p, &Overlay::default());
        let on = grab(&r, 1, &format!("on_{msaa}"));
        assert!(fogged(pixel(&on, 160, 120)), "inside the volume: {:?}", pixel(&on, 160, 120));
        assert!(grey(pixel(&on, 20, 220)), "outside the volume: {:?}", pixel(&on, 20, 220));
        assert!(grey(pixel(&on, 300, 60)), "beside the volume: {:?}", pixel(&on, 300, 60));
        r.set_fog_volumes(&[volume(true, None)]);
        r.render_view(1, &p, &Overlay::default());
        let disabled = pixel(&grab(&r, 1, &format!("disabled_{msaa}")), 160, 120);
        assert!(fogged(disabled), "a controller with fog off keeps the preview: {disabled:?}");
        let clear = FogVolume { preview: Fog { max_density: 0.0, ..MAGENTA }, ..volume(true, None) };
        r.set_fog_volumes(&[clear]);
        r.render_view(1, &p, &Overlay::default());
        assert!(grey(pixel(&grab(&r, 1, &format!("clear_{msaa}")), 160, 120)), "a preview without density");
        let wire = ViewParams { mode: RenderMode::Wireframe, ..p };
        r.set_fog_volumes(&[volume(true, Some(MAGENTA))]);
        r.render_view(1, &wire, &Overlay::default());
        assert!(!fogged(pixel(&grab(&r, 1, &format!("wire_{msaa}")), 160, 120)), "wireframe has no fog");
    }
}

#[test]
fn the_player_view_takes_the_fog_of_the_volume_it_is_in() {
    let (device, queue) = gpu_or_skip!();
    let mut r = Renderer::new(&device, &queue);
    floor(&mut r);
    let p = persp([0.0, 0.0, 64.0], [0.0, 1000.0, 0.0], false);
    let inside = ViewOptions { fog_volume_inside: true, ..Default::default() };
    r.set_fog_volumes(&[volume(true, Some(MAGENTA))]);
    r.set_view_options(1, inside);
    r.render_view(1, &p, &Overlay::default());
    let img = grab(&r, 1, "player_inside");
    assert!(fogged(pixel(&img, 160, 140)), "the far floor gets the volume fog: {:?}", pixel(&img, 160, 140));
    r.set_fog_volumes(&[volume(false, Some(MAGENTA))]);
    r.render_view(1, &p, &Overlay::default());
    assert!(grey(pixel(&grab(&r, 1, "player_start_disabled"), 160, 140)), "a disabled volume does nothing");
    r.set_environment(Environment {
        fog: Some(Fog { color: [0.0, 1.0, 0.0], color2: [0.0, 1.0, 0.0], ..MAGENTA }),
        ..Default::default()
    });
    r.set_fog_volumes(&[volume(true, None)]);
    r.set_view_options(1, ViewOptions { show_fog: true, ..inside });
    r.render_view(1, &p, &Overlay::default());
    assert!(grey(pixel(&grab(&r, 1, "player_fog_off"), 160, 140)), "its controller turns the map fog off");
    let out = persp([0.0, -1000.0, 64.0], [0.0, 1000.0, 0.0], false);
    r.render_view(1, &out, &Overlay::default());
    let c = pixel(&grab(&r, 1, "player_outside"), 160, 140);
    assert!(c[1] - c[0] > 50, "outside the volume the map fog stays: {c:?}");
}
