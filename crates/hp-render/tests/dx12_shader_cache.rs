#![cfg(windows)]

use hp_render::pipeline_cache;

const WGSL: &str = "@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> { \
    return vec4<f32>(f32(i & 1u), f32(i >> 1u), 0.0, 1.0); } \
    @fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.25, 0.5, 0.75, 1.0); }";

fn dx12_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

fn build(device: &wgpu::Device) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cache test"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let _ = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cache test"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
        }),
        multiview_mask: None,
        cache: None,
    });
}

#[test]
fn dx12_shaders_are_reused_from_disk_by_a_new_device() {
    let dir = std::env::temp_dir().join(format!("hp_dx12_shader_cache_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    pipeline_cache::set_dx12_shader_cache(Some(dir.clone()));
    let Some((first, _q1)) = dx12_device() else {
        eprintln!("no DirectX 12 adapter: skipped");
        return;
    };
    assert!(pipeline_cache::dx12_compiler().is_some());
    build(&first);
    let after_first = pipeline_cache::compile_stats().dx12.expect("dx12 stats");
    assert!(after_first.compiled >= 2, "{after_first:?}");
    let files = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
    assert!(files >= 2, "compiled shaders are written to the cache folder");
    drop(first);
    let (second, _q2) = dx12_device().expect("second device");
    build(&second);
    let after_second = pipeline_cache::compile_stats().dx12.expect("dx12 stats");
    assert_eq!(after_second.compiled, after_first.compiled, "nothing compiled again: {after_second:?}");
    assert!(after_second.disk_hits >= after_first.disk_hits + 2, "{after_second:?}");
    pipeline_cache::set_dx12_shader_cache(None);
    let _ = std::fs::remove_dir_all(&dir);
}
