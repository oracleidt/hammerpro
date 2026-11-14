use std::time::{Duration, Instant};

use hp_render::*;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .ok()?;
    if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
        return None;
    }
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hp-render pipelines test"),
        required_features: adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .ok()
}

fn params(msaa: bool) -> ViewParams {
    ViewParams {
        size: [64, 48],
        kind: ViewKind::Perspective {
            eye: [0.0, -256.0, 64.0],
            forward: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 75.0,
            near: 1.0,
            far: 8192.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.1, 0.1, 0.1],
        selection_color: [1.0, 0.0, 0.0, 0.5],
        show_sprites: true,
        show_models: true,
        msaa,
        game_pixel_grid: false,
    }
}

#[test]
fn pipelines_build_in_the_background_without_blocking() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU: skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    assert!(r.pipelines_ready(true), "without a background build the views draw and build on demand");
    r.prewarm_pipelines(true);
    let asked = Instant::now();
    let _ = r.pipelines_ready(true);
    assert!(asked.elapsed() < Duration::from_millis(500), "asking does not wait for the build");
    let deadline = Instant::now() + Duration::from_secs(600);
    while !r.pipelines_ready(true) {
        assert!(Instant::now() < deadline, "the background build never finished");
        std::thread::sleep(Duration::from_millis(20));
    }
    let stats = r.pipeline_stats();
    assert_eq!(stats.sets.first().map(|s| (s.samples, s.background)), Some((4, true)), "{stats:?}");
    assert!(stats.ready[1]);
    r.render_view(1, &params(true), &Overlay::default());
    r.render_view(2, &params(false), &Overlay::default());
    let stats = r.pipeline_stats();
    assert!(stats.ready[0] && stats.ready[1], "{stats:?}");
    assert_eq!(stats.sets.len(), 2, "each set is built once: {stats:?}");
    assert!(stats.sets.iter().all(|s| s.ms >= 0.0));
    assert!(!stats.building || r.pipelines_ready(false));
    let c = pipeline_cache::compile_stats();
    assert!(c.pipelines_created >= 100, "{c:?}");
}

#[test]
fn rendering_right_after_the_start_of_the_build_waits_for_it() {
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU: skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    r.prewarm_pipelines(false);
    r.render_view(1, &params(false), &Overlay::default());
    let stats = r.pipeline_stats();
    assert!(stats.ready[0], "{stats:?}");
    assert_eq!(stats.sets.first().map(|s| s.samples), Some(1), "{stats:?}");
    assert!(r.view_texture(1).is_some());
}
