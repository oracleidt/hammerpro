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

fn save(r: &Renderer, name: &str) -> Img {
    let (w, h, px) = r.read_view_rgba8(1).expect("readback");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hp-render-scratch");
    let _ = std::fs::create_dir_all(&dir);
    image::save_buffer(dir.join(format!("projector_{name}.png")), &px, w, h, image::ColorType::Rgba8).expect("png");
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
fn projected_textures_light_the_compiled_world_inside_their_frustum() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [255, 255, 255, 255], MaterialFlags::empty());
    let quad = face(-100.0, 100.0, -100.0, 100.0, 100.0, 0, [0.5, 0.5]);
    r.upsert_object(1, mesh(&quad, WORLD));
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT);
    let blocks = vec![CompiledBlock {
        object: 1,
        batches: vec![CompiledBatch { material: WORLD, vertices: quad.clone(), indices: vec![0, 1, 2, 0, 2, 3] }],
    }];
    let dark = [[0.0f32; 3]];
    let maps = [CompiledLightmap { width: 1, height: 1, texels: &dark, styles: &[], bumped: false }];
    r.set_tone_settings(ToneSettings { mode: ToneMode::Fixed, bloom: 0.0, ..ToneSettings::default() });
    r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    r.set_view_options(1, ViewOptions { compiled_lighting: true, ..ViewOptions::default() });
    let params = view(RenderMode::Textured);
    r.render_view(1, &params, &Overlay::default());
    let off = save(&r, "off");
    assert_eq!(px(&off, 100, 100), [0, 0, 0], "no projector, no light");

    let projector = Projector {
        pos: [0.0, 0.0, 0.0],
        forward: [0.0, 1.0, 0.0],
        right: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        fov_deg: 40.0,
        near_z: 4.0,
        far_z: 400.0,
        color: [1.0, 0.5, 0.0],
        attenuation: [1.0, 0.0, 0.0],
        shadows: false,
        image: None,
    };
    r.set_projectors(std::slice::from_ref(&projector));
    assert_eq!(r.projector_count(), 1);
    r.render_view(1, &params, &Overlay::default());
    let on = save(&r, "on");
    let centre = px(&on, 100, 100);
    close(centre[0], srgb(1.0), 3.0, "red channel at full strength");
    close(centre[1], srgb(0.5), 3.0, "green channel at half strength");
    assert_eq!(centre[2], 0);
    assert_eq!(px(&on, 5, 5), [0, 0, 0], "outside the frustum stays dark");

    let tight = Projector { fov_deg: 10.0, ..projector.clone() };
    r.set_projectors(&[tight]);
    r.render_view(1, &params, &Overlay::default());
    let narrow = save(&r, "narrow");
    assert!(px(&narrow, 100, 100)[0] > 0);
    assert_eq!(px(&narrow, 100, 75), [0, 0, 0], "a narrow frustum covers less");
    assert!(px(&on, 100, 75)[0] > 0, "the wide frustum covers it");

    let far = Projector { far_z: 50.0, ..projector.clone() };
    r.set_projectors(&[far]);
    r.render_view(1, &params, &Overlay::default());
    let short = save(&r, "short");
    assert_eq!(px(&short, 100, 100), [0, 0, 0], "the wall is past the far plane");

    let image = ProjectorImage {
        width: 2,
        height: 2,
        rgba: vec![255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255],
    };
    r.set_projectors(&[Projector { image: Some(image), color: [1.0; 3], ..projector }]);
    r.render_view(1, &params, &Overlay::default());
    let textured = save(&r, "textured");
    let (a, b) = (px(&textured, 85, 100)[0], px(&textured, 115, 100)[0]);
    assert!(a > 200 && b < 40, "the white half of the texture is on the left: {a} {b}");

    r.set_projectors(&[]);
    r.render_view(1, &params, &Overlay::default());
    let gone = save(&r, "gone");
    assert_eq!(px(&gone, 100, 100), [0, 0, 0]);
}

#[test]
fn projected_textures_cast_shadows_when_asked() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    mat(&mut r, WORLD, [255, 255, 255, 255], MaterialFlags::empty());
    let wall = face(-100.0, 100.0, -100.0, 100.0, 100.0, 0, [0.5, 0.5]);
    r.upsert_object(1, mesh(&wall, WORLD));
    r.set_object_flags(1, ObjectFlags::COMPILED_LIGHT);
    let blocker = face(-10.0, 10.0, -10.0, 10.0, 50.0, NO_COMPILED_LIGHTMAP, [0.5, 0.5]);
    r.upsert_object(2, mesh(&blocker, WORLD));
    let blocks = vec![CompiledBlock {
        object: 1,
        batches: vec![CompiledBatch { material: WORLD, vertices: wall.clone(), indices: vec![0, 1, 2, 0, 2, 3] }],
    }];
    let dark = [[0.0f32; 3]];
    let maps = [CompiledLightmap { width: 1, height: 1, texels: &dark, styles: &[], bumped: false }];
    r.set_tone_settings(ToneSettings { mode: ToneMode::Fixed, bloom: 0.0, ..ToneSettings::default() });
    r.set_compiled_lighting(CompiledLightMode::Hdr, &maps, &blocks);
    r.set_view_options(1, ViewOptions { compiled_lighting: true, ..ViewOptions::default() });
    let mut params = view(RenderMode::Textured);
    if let ViewKind::Perspective { eye, .. } = &mut params.kind {
        *eye = [80.0, 0.0, 0.0];
    }
    let projector = Projector {
        pos: [0.0, 0.0, 0.0],
        forward: [0.0, 1.0, 0.0],
        right: [1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
        fov_deg: 40.0,
        near_z: 4.0,
        far_z: 400.0,
        color: [1.0; 3],
        attenuation: [1.0, 0.0, 0.0],
        shadows: false,
        image: None,
    };
    r.set_projectors(std::slice::from_ref(&projector));
    r.render_view(1, &params, &Overlay::default());
    let open = save(&r, "shadow_off");
    assert!(px(&open, 20, 100)[0] > 100 && px(&open, 50, 100)[0] > 100, "lit without shadows");
    r.set_projectors(&[Projector { shadows: true, ..projector }]);
    r.render_view(1, &params, &Overlay::default());
    let shaded = save(&r, "shadow_on");
    assert!(px(&shaded, 20, 100)[0] < 30, "the blocker shades the wall behind it: {:?}", px(&shaded, 20, 100));
    assert!(px(&shaded, 50, 100)[0] > 100, "away from the blocker the wall is lit");
}
