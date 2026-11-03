mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_install, fake_settings, scratch, vtf};
use hp_app::AssetsApi;
use hp_app::runtime::render::{RenderDisplay, publish_render_display, render_display};
use hp_app::state::{Gfx, SceneOptions};
use hp_assets::AssetManager;
use hp_editor::Document;
use hp_render::{Overlay, RenderMode, ViewKind, ViewOptions, ViewParams};

fn gfx() -> Option<Gfx> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .ok()?;
    let caps = adapter.get_downlevel_capabilities().flags;
    if !caps.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) || !caps.contains(wgpu::DownlevelFlags::CUBE_ARRAY_TEXTURES)
    {
        return None;
    }
    let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hp-assets cubemap test"),
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

fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
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

fn settle(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx) {
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, &opts());
        let s = am.stats();
        if s.queued_jobs == 0 && s.pending_uploads == 0 && s.textures_loading == 0 && s.materials_loading == 0 {
            am.begin_frame(&ctx, Some(gfx));
            am.sync_scene(doc, gfx, &opts());
            return;
        }
        assert!(Instant::now() < deadline, "the scene did not settle");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn down() -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size: [320, 240],
        kind: ViewKind::Perspective {
            eye: [0.0, -400.0, 120.0],
            forward: [0.0, 1.0, -0.35],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 60.0,
            near: 4.0,
            far: 30000.0,
        },
        mode: RenderMode::Textured,
        grid: None,
        background: [0.0, 0.0, 0.0],
        selection_color: [1.0, 0.25, 0.25, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: false,
    }
}

fn shot(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, cubemaps: bool, name: &str) -> Vec<u8> {
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        gfx.renderer.set_view_options(1, ViewOptions { cubemaps, ..ViewOptions::default() });
        gfx.renderer.render_view(1, &down(), &Overlay::default());
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, &opts());
        if !cubemaps || gfx.renderer.cubemap_stats().pending_faces == 0 {
            break;
        }
        assert!(Instant::now() < deadline, "the probes were not drawn");
    }
    gfx.renderer.render_view(1, &down(), &Overlay::default());
    let (w, h, px) = gfx.renderer.read_view_rgba8(1).expect("readback");
    image::save_buffer(scratch("png").join(format!("cubemaps_{name}.png")), &px, w, h, image::ColorType::Rgba8)
        .expect("png");
    px
}

const OBB_LO: [f32; 3] = [-512.0, -512.0, 0.0];
const OBB_HI: [f32; 3] = [512.0, 512.0, 512.0];

fn vmf(cubemap_z: f32, parallax: bool) -> String {
    let block = |id: u32, lo: [f32; 3], hi: [f32; 3], top: &str| {
        let [x0, y0, z0] = lo;
        let [x1, y1, z1] = hi;
        let side = |sid: u32, p: [[f32; 3]; 3], m: &str| {
            format!(
                "side {{ \"id\" \"{sid}\" \"plane\" \"({} {} {}) ({} {} {}) ({} {} {})\" \"material\" \"{m}\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" \"lightmapscale\" \"16\" }}\n",
                p[0][0], p[0][1], p[0][2], p[1][0], p[1][1], p[1][2], p[2][0], p[2][1], p[2][2]
            )
        };
        let b = id * 10;
        let mut s = format!("solid {{ \"id\" \"{id}\"\n");
        s += &side(b, [[x0, y1, z1], [x1, y1, z1], [x1, y0, z1]], top);
        s += &side(b + 1, [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0]], "test/brick");
        s += &side(b + 2, [[x0, y1, z1], [x0, y0, z1], [x0, y0, z0]], "test/brick");
        s += &side(b + 3, [[x1, y1, z0], [x1, y0, z0], [x1, y0, z1]], "test/brick");
        s += &side(b + 4, [[x1, y1, z1], [x0, y1, z1], [x0, y1, z0]], "test/brick");
        s += &side(b + 5, [[x1, y0, z0], [x0, y0, z0], [x0, y0, z1]], "test/brick");
        s + "}\n"
    };
    let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
    text += &block(2, [-512.0, -512.0, -16.0], [512.0, 512.0, 0.0], "test/mirror");
    text += &block(3, [-512.0, 512.0, 0.0], [512.0, 528.0, 512.0], "test/brick");
    text += "}\n";
    let obb = if parallax { "\"parallaxobb\" \"room\"" } else { "" };
    text += &format!("entity {{ \"id\" \"20\" \"classname\" \"env_cubemap\" \"origin\" \"0 0 {cubemap_z}\" {obb} }}\n");
    if parallax {
        text += "entity { \"id\" \"30\" \"classname\" \"parallax_obb\" \"targetname\" \"room\"\n";
        text += &block(4, OBB_LO, OBB_HI, "tools/toolstrigger");
        text += "}\n";
    }
    text += "entity { \"id\" \"21\" \"classname\" \"env_cubemap\" \"origin\" \"0 0 -300\" \"cubemapsize\" \"6\" }\n";
    text
}

#[test]
fn editor_cubemaps_follow_the_env_cubemaps() {
    let _serial = serial();
    let Some(mut gfx) = gfx() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let root = fake_install("cubemaps_gpu");
    let mats = root.join("garrysmod").join("materials").join("test");
    std::fs::write(
        mats.join("mirror.vmt"),
        r#""LightmappedGeneric" { "$basetexture" "test/mirror" "$envmap" "env_cubemap" "$envmaptint" "[1 1 1]" "$color" "[0 0 0]" }"#,
    )
    .expect("vmt");
    std::fs::write(mats.join("mirror.vtf"), vtf(16, [255, 255, 255, 255])).expect("vtf");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)), "mounted");
    let saved = render_display();
    publish_render_display(RenderDisplay { cubemaps: true, cubemaps_auto: true, ..saved });
    let map = hp_vmf::parse(&vmf(64.0, false)).expect("vmf");
    let mut doc = Document::from_map(map);
    settle(&mut am, &mut doc, &mut gfx);
    let stats = gfx.renderer.cubemap_stats();
    assert_eq!(stats.probes, 2, "one probe per env_cubemap: {stats:?}");
    let mirror = am.render_material_id("test/mirror");
    settle(&mut am, &mut doc, &mut gfx);
    let desc = gfx.renderer.material(mirror).expect("material");
    let env = desc.envmap.expect("the reflection of the material");
    assert_eq!((env.cubemap, env.texture, env.tint), (None, None, [1.0; 3]));

    let on = shot(&mut am, &mut doc, &mut gfx, true, "on");
    assert_eq!(gfx.renderer.cubemap_stats().ready, 2);
    let off = shot(&mut am, &mut doc, &mut gfx, false, "off");
    let lit = |px: &[u8]| px.chunks_exact(4).filter(|p| p[0] > 40).count();
    assert!(lit(&on) > lit(&off) + 2000, "the reflection: {} lit pixels on, {} off", lit(&on), lit(&off));

    let id = 20;
    let before = gfx.renderer.cubemap_stats().faces_rendered;
    doc.set_entity_key(&[id], "origin", "0 0 200").expect("move");
    settle(&mut am, &mut doc, &mut gfx);
    shot(&mut am, &mut doc, &mut gfx, true, "moved");
    let after = gfx.renderer.cubemap_stats();
    assert!(after.faces_rendered >= before + 6, "the moved probe drawn again: {before} then {after:?}");
    assert_eq!(after.probes, 2);
    doc.delete(&[hp_editor::ObjRef::Entity(id)]).expect("delete");
    settle(&mut am, &mut doc, &mut gfx);
    assert_eq!(gfx.renderer.cubemap_stats().probes, 1);

    publish_render_display(RenderDisplay { cubemaps: false, ..saved });
    settle(&mut am, &mut doc, &mut gfx);
    assert!(gfx.renderer.material(mirror).expect("material").envmap.is_none());
    publish_render_display(saved);
}

#[test]
fn editor_probes_take_the_box_of_their_parallax_obb() {
    let _serial = serial();
    let Some(mut gfx) = gfx() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let root = fake_install("cubemaps_parallax_gpu");
    let mats = root.join("garrysmod").join("materials").join("test");
    std::fs::write(
        mats.join("mirror.vmt"),
        r#""LightmappedGeneric" { "$basetexture" "test/mirror" "$envmap" "env_cubemap" "$color" "[0 0 0]" }"#,
    )
    .expect("vmt");
    std::fs::write(mats.join("mirror.vtf"), vtf(16, [255, 255, 255, 255])).expect("vtf");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)), "mounted");
    let saved = render_display();
    publish_render_display(RenderDisplay { cubemaps: true, cubemaps_auto: true, ..saved });
    let mut doc = Document::from_map(hp_vmf::parse(&vmf(64.0, true)).expect("vmf"));
    settle(&mut am, &mut doc, &mut gfx);
    let mirror = am.render_material_id("test/mirror");
    settle(&mut am, &mut doc, &mut gfx);
    let env = gfx.renderer.material(mirror).and_then(|m| m.envmap).expect("the reflection");
    assert!(env.parallax, "LightmappedGeneric corrects to the box");
    let probe = gfx.renderer.nearest_cubemap([0.0, 0.0, 64.0]).expect("the probe");
    let other = gfx.renderer.nearest_cubemap([0.0, 0.0, -300.0]).expect("the other probe");
    let b = gfx.renderer.cubemap_parallax(probe).expect("the box of the parallax_obb");
    assert_eq!(b.origin, [0.0, 0.0, 64.0]);
    let unit = |p: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|i| b.rows[i][0] * p[0] + b.rows[i][1] * p[1] + b.rows[i][2] * p[2] + b.rows[i][3])
    };
    for c in unit(OBB_LO).into_iter().chain(unit(OBB_HI)) {
        assert!(c.abs() < 1e-3 || (c - 1.0).abs() < 1e-3, "a corner of the box: {c}");
    }
    assert_eq!(gfx.renderer.cubemap_parallax(other), None, "no parallax_obb, no box");
    let on = shot(&mut am, &mut doc, &mut gfx, true, "parallax");
    doc.set_entity_key(&[20], "parallaxobb", "").expect("key");
    settle(&mut am, &mut doc, &mut gfx);
    assert_eq!(gfx.renderer.cubemap_parallax(probe), None, "the key cleared");
    let plain = shot(&mut am, &mut doc, &mut gfx, true, "parallax_plain");
    assert_ne!(on, plain, "the box moves the reflection");
    doc.set_entity_key(&[20], "parallaxobb", "room").expect("key");
    settle(&mut am, &mut doc, &mut gfx);
    assert_eq!(gfx.renderer.cubemap_parallax(probe), Some(b));
    publish_render_display(saved);
}
