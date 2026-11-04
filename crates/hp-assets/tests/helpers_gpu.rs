mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_install, fake_settings, scratch};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::runtime::helpers::{SceneDisplay, publish_scene_display};
use hp_app::state::{Gfx, SceneOptions};
use hp_assets::AssetManager;
use hp_editor::{Document, ObjRef};
use hp_render::{GridParams, OrthoAxis, Overlay, RenderMode, ViewKind, ViewParams};

fn gfx() -> Option<Gfx> {
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
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hp-assets helpers test"),
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

fn opts() -> SceneOptions {
    SceneOptions {
        show_models: true,
        show_helpers: true,
        show_tool_textures: true,
        lighting_preview: false,
        model_lods: false,
    }
}

fn settle(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, o: &SceneOptions) -> bool {
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, o);
        let s = am.stats();
        if s.queued_jobs == 0 && s.pending_uploads == 0 && s.textures_loading == 0 && s.materials_loading == 0 {
            am.begin_frame(&ctx, Some(gfx));
            am.sync_scene(doc, gfx, o);
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn objects(gfx: &mut Gfx) -> u32 {
    gfx.renderer.flush();
    gfx.renderer.stats().objects
}

fn save(gfx: &Gfx, view: u32, name: &str) {
    let (w, h, px) = gfx.renderer.read_view_rgba8(view).expect("readback");
    let path = scratch("png").join(format!("{name}.png"));
    image::save_buffer(&path, &px, w, h, image::ColorType::Rgba8).expect("png");
    eprintln!("wrote {}", path.display());
}

fn view(eye: DVec3, target: DVec3) -> ViewParams {
    let forward = (target - eye).normalize_or(DVec3::X);
    ViewParams {
        game_pixel_grid: false,
        size: [640, 400],
        kind: ViewKind::Perspective {
            eye: eye.as_vec3().to_array(),
            forward: forward.as_vec3().to_array(),
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 70.0,
            near: 4.0,
            far: 30000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.14, 0.15, 0.17],
        selection_color: [1.0, 0.25, 0.25, 0.45],
        show_sprites: true,
        show_models: true,
        msaa: true,
    }
}

const EXTRA: &str = r#"entity
{
	"id" "40"
	"classname" "info_overlay"
	"material" "decals/test"
	"sides" "1"
	"RenderOrder" "0"
	"StartU" "0"
	"EndU" "4"
	"StartV" "0"
	"EndV" "2"
	"BasisOrigin" "0 0 0"
	"BasisU" "1 0 0"
	"BasisV" "0 1 0"
	"BasisNormal" "0 0 1"
	"uv0" "-128 -64 0"
	"uv1" "-128 64 0"
	"uv2" "128 64 0"
	"uv3" "128 -64 0"
	"origin" "0 0 0"
}
entity
{
	"id" "41"
	"classname" "move_rope"
	"targetname" "r1"
	"NextKey" "r2"
	"Slack" "120"
	"Subdiv" "6"
	"Width" "3"
	"RopeMaterial" "cable/cable.vmt"
	"origin" "-200 -100 120"
}
entity
{
	"id" "42"
	"classname" "keyframe_rope"
	"targetname" "r2"
	"origin" "200 -100 120"
}
entity
{
	"id" "43"
	"classname" "move_rope"
	"targetname" "r3"
	"NextKey" "R1"
	"Type" "1"
	"Slack" "150"
	"origin" "-200 100 160"
}
"#;

#[test]
fn overlays_and_ropes_follow_the_document() {
    hp_app::register_translations();
    hp_assets::register_translations();
    let Some(mut gfx) = gfx() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let gfx = &mut gfx;
    publish_scene_display(SceneDisplay::default());
    let root = fake_install("helpers_gpu");
    let mut am = AssetManager::new(&fake_settings(&root));
    let vmf = hp_app::testing::TEST_VMF.replacen("cordon\n{", &format!("{EXTRA}cordon\n{{"), 1);
    let mut doc = Document::from_map(hp_vmf::parse(&vmf).expect("test map"));
    let o = opts();
    am.sync_scene(&mut doc, gfx, &o);
    assert!(settle(&mut am, &mut doc, gfx, &o));
    let all = objects(gfx);

    publish_scene_display(SceneDisplay { overlays: false, ..SceneDisplay::default() });
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all - 1, "the overlay object is removed");
    publish_scene_display(SceneDisplay::default());
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all);
    publish_scene_display(SceneDisplay { ropes: false, ..SceneDisplay::default() });
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all - 2, "the rope segments are removed (r3 links to r1, names ignore case)");
    publish_scene_display(SceneDisplay::default());
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all);

    gfx.renderer.render_view(1, &view(DVec3::new(-300.0, -420.0, 260.0), DVec3::ZERO), &Overlay::default());
    save(gfx, 1, "helpers_overlay_rope_3d");
    let top = ViewParams {
        game_pixel_grid: false,
        size: [640, 400],
        kind: ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0, 0.0, 0.0], pixels_per_unit: 0.9 },
        mode: RenderMode::Wireframe,
        grid: Some(GridParams { size: 64.0, major_every: 8, show_1024: true, opacity: 0.6, cells: true }),
        background: [0.08, 0.08, 0.09],
        selection_color: [1.0, 0.25, 0.25, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: true,
    };
    gfx.renderer.render_view(2, &top, &Overlay::default());
    save(gfx, 2, "helpers_overlay_rope_top");

    doc.set_selection(&[ObjRef::Entity(40)]);
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all);
    doc.delete(&[ObjRef::Solid(2)]).expect("delete floor");
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all - 2, "the floor and the overlay");
    assert!(doc.undo());
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all);
    publish_scene_display(SceneDisplay { simulate_ropes: true, ..SceneDisplay::default() });
    am.sync_scene(&mut doc, gfx, &o);
    doc.translate(&[ObjRef::Entity(41)], DVec3::new(0.0, 0.0, 32.0)).expect("move start");
    for _ in 0..120 {
        am.sync_scene(&mut doc, gfx, &o);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(objects(gfx), all);
    publish_scene_display(SceneDisplay::default());
    am.sync_scene(&mut doc, gfx, &o);
    doc.translate(&[ObjRef::Entity(42)], DVec3::new(0.0, 0.0, 64.0)).expect("move");
    doc.set_entity_key(&[40], "uv2", "200 64 0").expect("key");
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all);
    doc.delete(&[ObjRef::Entity(42)]).expect("delete");
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all - 2);
    doc.set_entity_key(&[41], "Dangling", "1").expect("key");
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all - 1);
    doc.create_point_entity("env_sun", DVec3::new(0.0, 0.0, 64.0), &[]).expect("sun");
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), all, "the sun entity box only");
}
