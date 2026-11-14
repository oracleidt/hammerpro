use std::sync::OnceLock;

use hp_render::*;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    static GPU: OnceLock<Option<(wgpu::Device, wgpu::Queue)>> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hp-render memory budget test"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .ok()
    })
    .clone()
}

fn texture(size: u32) -> TextureUpload {
    TextureUpload {
        width: size,
        height: size,
        format: TexFormat::Rgba8Srgb,
        levels: vec![vec![200u8; (size * size * 4) as usize]],
    }
}

#[test]
fn textures_that_would_pass_the_budget_are_loaded_smaller() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    r.set_texture_limit(4096);
    let base = r.memory().total;
    let full = r.create_texture(&texture(1024));
    assert_ne!(full, TexId::MISSING);
    let full_bytes = r.memory().total - base;
    assert!(full_bytes >= 4 << 20, "{full_bytes}");
    assert_eq!(r.memory_budget_squeezes().0, 0);
    assert!(!r.take_wants_room());

    let tracked = r.memory().total;
    r.set_memory_budget(tracked + (1 << 20), 0);
    assert!(r.fits_in_memory_budget(1 << 20));
    assert!(!r.fits_in_memory_budget(2 << 20));
    let small = r.create_texture(&texture(1024));
    assert_ne!(small, TexId::MISSING, "the texture is still made, only smaller");
    let small_bytes = r.memory().total - tracked;
    assert!(small_bytes <= 1 << 20, "{small_bytes} bytes for a texture squeezed under the budget");
    assert_eq!(r.memory_budget_squeezes().0, 1);
    assert!(r.take_wants_room(), "the memory monitor is asked to make room");
    assert!(!r.take_wants_room());

    r.set_memory_budget(0, 0);
    let again = r.create_texture(&texture(1024));
    assert_ne!(again, TexId::MISSING);
    assert_eq!(r.memory_budget_squeezes().0, 1, "no budget, no squeeze");
}

#[test]
fn lightmap_layers_stay_under_the_cap() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue);
    assert_eq!(r.lightmap_layer_cap(), u32::MAX);
    r.set_lightmap_layer_cap(2);
    assert!(!r.ensure_lightmap_layers(256, 3), "more layers than the cap are refused");
    assert_eq!(r.memory_budget_squeezes().1, 1);
    assert!(r.ensure_lightmap_layers(256, 1));
    assert!(r.ensure_lightmap_layers(256, 2), "growing stops at the cap");
    assert!(r.lightmap_stats().bytes <= 2 * 256 * 256 * 4 + (1 << 20));
    r.set_low_memory(true);
    assert!(r.low_memory());
    r.set_low_memory(false);
    assert!(!r.low_memory());
}
