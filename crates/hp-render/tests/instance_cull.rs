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
            label: Some("hp-render instance cull test"),
            required_features: features,
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

const PROP: MaterialId = 1;

fn view() -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [160, 120],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 0.0],
            forward: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 80.0,
            near: 1.0,
            far: 20000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.1, 0.1, 0.1],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: false,
    }
}

fn quad(r: &mut Renderer) -> ModelId {
    let v = |y: f32, z: f32| Vertex { pos: [0.0, y, z], normal: [1.0, 0.0, 0.0], uv: [0.0, 0.0], alpha: 1.0 };
    r.create_model(&[ModelPart {
        material: PROP,
        vertices: vec![v(-20.0, -20.0), v(20.0, -20.0), v(20.0, 20.0), v(-20.0, 20.0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        tangents: Vec::new(),
    }])
}

fn at(model: ModelId, object: ObjectId, x: f32, y: f32, z: f32) -> ModelInstance {
    ModelInstance {
        model,
        transform: [[0.0, -1.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, y, z, 1.0]],
        object,
        tint: [1.0; 4],
        fade: [0.0; 2],
    }
}

fn render(r: &mut Renderer, id: u32) -> Vec<u8> {
    r.render_view(id, &view(), &Overlay::default());
    r.read_view_rgba8(id).expect("readback").2
}

#[test]
fn only_the_instances_in_view_are_drawn_and_the_picture_is_the_same() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let up = TextureUpload {
        width: 4,
        height: 4,
        format: TexFormat::Rgba8Srgb,
        levels: vec![[200, 120, 60, 255].repeat(16)],
    };
    let t = r.create_texture(&up);
    r.set_material(PROP, &MaterialDesc { texture: Some(t), tex_size: [4.0, 4.0], ..Default::default() });
    let model = quad(&mut r);
    let mut list = Vec::new();
    for i in 0..300u32 {
        let (x, y) = if i < 12 {
            (-120.0 + 20.0 * i as f32, 300.0)
        } else {
            (3000.0 + 90.0 * i as f32, -4000.0 - 40.0 * i as f32)
        };
        list.push(at(model, 100 + i, x, y, 0.0));
    }
    r.set_model_instances(&list);
    r.set_instance_culling(false);
    let full = render(&mut r, 1);
    let counted_full = r.view_counts(1).expect("counts").model_instances;
    r.set_instance_culling(true);
    let culled = render(&mut r, 2);
    let counted = r.view_counts(2).expect("counts").model_instances;
    assert_eq!(counted_full, 300, "without culling every instance goes to the GPU");
    assert!(counted < 40, "only the instances near the view are sent: {counted}");
    assert!(counted >= 12, "the visible instances are kept: {counted}");
    assert_eq!(full, culled, "culling does not change a pixel");
    let background = [full[0], full[1], full[2]];
    let drawn = full.chunks(4).filter(|p| [p[0], p[1], p[2]] != background).count();
    assert!(drawn > 40, "the models are visible: {drawn} pixels differ from the background");
}
