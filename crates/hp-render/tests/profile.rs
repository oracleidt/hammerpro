use hp_render::*;

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
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
    let features = (adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC) | profile::wanted_features(&adapter);
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hp-render profile test"),
        required_features: features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .ok()
}

fn view(ortho: bool) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [96, 64],
        kind: if ortho {
            ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0; 3], pixels_per_unit: 1.0 }
        } else {
            ViewKind::Perspective {
                eye: [0.0; 3],
                forward: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 80.0,
                near: 1.0,
                far: 5000.0,
            }
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

#[test]
fn the_3d_render_reports_cpu_and_gpu_times() {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    assert_eq!(r.frame_times().cpu, profile::CpuTimes::default());
    for _ in 0..8 {
        r.render_view(1, &view(false), &Overlay::default());
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    }
    let t = r.frame_times();
    assert!(t.cpu.record_ms > 0.0 && t.cpu.submit_ms >= 0.0, "{t:?}");
    if t.gpu_supported {
        let g = t.gpu.expect("a sample comes back once the GPU finished");
        assert!(g.total_ms >= 0.0 && g.total_ms < 5000.0, "{g:?}");
        assert!(g.total_ms + 1e-3 >= g.shadows_ms + g.scene_ms + g.post_ms - 1e-2, "{g:?}");
    }
    let before = r.frame_times();
    r.render_view(2, &view(true), &Overlay::default());
    assert_eq!(r.frame_times().cpu, before.cpu, "2D views do not touch the 3D numbers");
}

#[test]
fn quality_caps_resize_the_shadow_maps_and_back() {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((device, queue)) = gpu() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let mut r = Renderer::new(&device, &queue);
    assert_eq!(r.quality(), Quality::default());
    let low = Quality { sun_shadow_size: 1024, local_shadow_size: 128, adaptive: false };
    r.set_quality(low);
    assert_eq!(r.quality(), low);
    r.set_quality(low);
    r.set_quality(Quality::default());
    assert_eq!(r.quality(), Quality::default());
    assert!(!r.adaptive_stats().enabled, "a fixed quality never adapts");
    let auto = Quality { adaptive: true, ..Quality::default() };
    r.set_quality(auto);
    let stats = r.adaptive_stats();
    assert!(stats.enabled && stats.level == 0 && stats.max_level == 3, "{stats:?}");
    assert_eq!(r.applied_quality(), auto, "nothing is lowered before the card is measured");
}
