use std::sync::OnceLock;

use hp_render::hooks::{HookContext, PassHook};
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
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hp-render opaque view test"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

const SIZE: u32 = 96;
const H: f32 = 100.0;

fn inside_box(r: &mut Renderer, material: MaterialId) {
    let quad = |c: [[f32; 3]; 4], normal: [f32; 3]| MeshBatch {
        material,
        vertices: c.iter().map(|p| Vertex { pos: *p, normal, uv: [p[0] + p[2], p[1] + p[2]], alpha: 0.0 }).collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
        blend: Vec::new(),
        cubemaps: vec![NO_CUBEMAP; 4],
    };
    let batches = vec![
        quad([[H, -H, -H], [H, H, -H], [H, H, H], [H, -H, H]], [-1.0, 0.0, 0.0]),
        quad([[-H, -H, -H], [-H, -H, H], [-H, H, H], [-H, H, -H]], [1.0, 0.0, 0.0]),
        quad([[-H, H, -H], [-H, H, H], [H, H, H], [H, H, -H]], [0.0, -1.0, 0.0]),
        quad([[-H, -H, -H], [H, -H, -H], [H, -H, H], [-H, -H, H]], [0.0, 1.0, 0.0]),
        quad([[-H, -H, H], [H, -H, H], [H, H, H], [-H, H, H]], [0.0, 0.0, -1.0]),
        quad([[-H, -H, -H], [-H, H, -H], [H, H, -H], [H, -H, -H]], [0.0, 0.0, 1.0]),
    ];
    r.upsert_object(
        1,
        ObjectMesh { batches, aabb_min: [-H; 3], aabb_max: [H; 3], wire_color: [200; 3], ..Default::default() },
    );
}

fn params(msaa: bool) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [SIZE, SIZE],
        kind: ViewKind::Perspective {
            eye: [0.0, 0.0, 0.0],
            forward: [1.0, 0.2, -0.1],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 90.0,
            near: 1.0,
            far: 10000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.1, 0.2, 0.3],
        selection_color: [1.0, 0.1, 0.1, 0.45],
        show_sprites: true,
        show_models: true,
        msaa,
    }
}

fn shot(r: &mut Renderer, msaa: bool) -> Vec<u8> {
    r.render_view(1, &params(msaa), &Overlay::default());
    r.read_view_rgba8(1).expect("readback").2
}

fn transparent_pixels(px: &[u8]) -> usize {
    px.chunks_exact(4).filter(|p| p[3] != 255).count()
}

struct ClearAlpha;

impl PassHook for ClearAlpha {
    fn name(&self) -> &'static str {
        "clear alpha"
    }

    fn finish(&mut self, cx: &HookContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test clear alpha"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 0.0, b: 0.0, a: 0.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}

#[test]
fn missing_materials_inside_their_brush_keep_the_view_opaque() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    let missing = 10;
    r.set_material(
        missing,
        &MaterialDesc { texture: Some(TexId::MISSING), tex_size: [64.0, 64.0], ..Default::default() },
    );
    inside_box(&mut r, missing);
    for msaa in [false, true] {
        let px = shot(&mut r, msaa);
        assert_eq!(transparent_pixels(&px), 0, "missing material, msaa {msaa}");
        let magenta = px.chunks_exact(4).filter(|p| p[0] > 120 && p[1] < 60 && p[2] > 120).count();
        let black = px.chunks_exact(4).filter(|p| p[0] < 40 && p[1] < 40 && p[2] < 40).count();
        assert!(magenta > 100 && black > 100, "the checkerboard: {magenta} magenta, {black} black");
    }
    r.set_material(
        missing,
        &MaterialDesc {
            texture: Some(TexId::MISSING),
            tex_size: [64.0, 64.0],
            flags: MaterialFlags::TOOL,
            ..Default::default()
        },
    );
    for msaa in [false, true] {
        let px = shot(&mut r, msaa);
        assert_eq!(transparent_pixels(&px), 0, "missing tool texture, msaa {msaa}");
    }
}

#[test]
fn views_stay_opaque_whatever_a_pass_writes_into_their_alpha() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.add_pass_hook(Box::new(ClearAlpha));
    for msaa in [false, true] {
        let px = shot(&mut r, msaa);
        assert_eq!(transparent_pixels(&px), 0, "every pixel opaque, msaa {msaa}");
        assert!(px.chunks_exact(4).all(|p| p[0] == 255 && p[1] == 0 && p[2] == 0), "the colors are kept");
    }
}
