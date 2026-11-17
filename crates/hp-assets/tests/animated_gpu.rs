mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{frames_until, real_gmod, real_settings};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::state::ModelState;
use hp_app::state::{Gfx, SceneOptions};
use hp_assets::AssetManager;
use hp_editor::Document;
use hp_render::{Overlay, RenderMode, ViewKind, ViewParams};

fn gfx() -> Option<Gfx> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .ok()?;
    let info = adapter.get_info();
    eprintln!("adapter: {} ({:?})", info.name, info.backend);
    if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
        eprintln!("adapter lacks vertex storage buffers");
        return None;
    }
    let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hp-assets test"),
        required_features: features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .ok()?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let egui_renderer = egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
    let rs = egui_wgpu::RenderState {
        adapter,
        available_adapters: Vec::new(),
        instance,
        device,
        queue,
        target_format: format,
        renderer: Arc::new(egui::mutex::RwLock::new(egui_renderer)),
    };
    Some(Gfx::new(&rs))
}

fn persp(eye: DVec3, target: DVec3, size: [u32; 2], mode: RenderMode) -> ViewParams {
    let forward = (target - eye).normalize_or(DVec3::X);
    ViewParams {
        game_pixel_grid: false,
        size,
        kind: ViewKind::Perspective {
            eye: eye.as_vec3().to_array(),
            forward: forward.as_vec3().to_array(),
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 70.0,
            near: 4.0,
            far: 30000.0,
        },
        mode,
        grid: None,
        background: [0.14, 0.15, 0.17],
        selection_color: [1.0, 0.25, 0.25, 0.45],
        show_sprites: true,
        show_models: true,
        msaa: true,
    }
}

fn frame(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, view: &ViewParams) -> Vec<u8> {
    let ctx = egui::Context::default();
    let o = SceneOptions {
        show_models: true,
        show_helpers: false,
        show_tool_textures: true,
        lighting_preview: false,
        model_lods: false,
    };
    am.begin_frame(&ctx, Some(gfx));
    am.note_view(0, view);
    am.sync_scene(doc, gfx, &o);
    gfx.renderer.render_view(0, view, &Overlay::default());
    gfx.renderer.read_view_rgba8(0).expect("readback").2
}

fn changed(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 24))
        .count()
}

#[test]
fn dynamic_props_animate_and_static_props_stay_still() {
    let Some(root) = real_gmod() else {
        eprintln!("Garry's Mod not found: test skipped");
        return;
    };
    let Some(mut gfx) = gfx() else { return };
    hp_app::register_translations();
    hp_assets::register_translations();
    let mut am = AssetManager::new(&real_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(120)), "mount timed out");
    let soldier =
        frames_until(&mut am, Duration::from_secs(30), |am, _| match am.model_info("models/combine_soldier.mdl") {
            ModelState::Ready(info) => Some(info),
            _ => None,
        })
        .expect("soldier info");
    let idle =
        soldier.sequences.iter().position(|s| s.eq_ignore_ascii_case("Idle1")).expect("included sequences listed");
    assert!(soldier.sequence_frames[idle] > 1, "Idle1 comes from combine_soldier_anims.mdl with its frames");
    let thumper = "models/props_combine/combinethumper002.mdl";
    let view = persp(DVec3::new(-420.0, 0.0, 160.0), DVec3::new(0.0, 0.0, 120.0), [320, 240], RenderMode::Textured);
    for (class, animated) in [("prop_static", false), ("prop_dynamic", true)] {
        let mut doc = Document::default();
        doc.create_point_entity(class, DVec3::ZERO, &[("model".to_owned(), thumper.to_owned())]).expect("create");
        let start = Instant::now();
        let mut last = frame(&mut am, &mut doc, &mut gfx, &view);
        let mut moved = 0;
        while start.elapsed() < Duration::from_secs(40) {
            std::thread::sleep(Duration::from_millis(150));
            let next = frame(&mut am, &mut doc, &mut gfx, &view);
            moved = moved.max(if start.elapsed() > Duration::from_secs(8) { changed(&last, &next) } else { 0 });
            last = next;
            if animated && moved > 50 {
                break;
            }
        }
        eprintln!("{class}: {moved} pixels moved, animating {}", am.models_animating());
        if animated {
            assert!(moved > 50 && am.models_animating(), "{class} plays its idle sequence");
        } else {
            assert!(moved < 20 && !am.models_animating(), "{class} stays still");
        }
    }
}
