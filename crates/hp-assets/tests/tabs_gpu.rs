mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_install, fake_settings};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::state::{Gfx, SceneOptions};
use hp_assets::AssetManager;
use hp_editor::{Document, ObjRef};
use hp_render::{ObjectFlags, Overlay, RenderMode, ViewKind, ViewParams};

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
        label: Some("hp-assets tabs test"),
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

fn persp(eye: DVec3, target: DVec3) -> ViewParams {
    let forward = (target - eye).normalize_or(DVec3::X);
    ViewParams {
        game_pixel_grid: false,
        size: [480, 300],
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

fn boxes(n: usize, material: &str) -> hp_vmf::Map {
    let mut map = hp_vmf::Map::new_empty();
    let mut side_id = 1;
    for i in 0..n {
        let (x, y, z) = ((i % 50) as f64, ((i / 50) % 50) as f64, (i / 2500) as f64);
        let lo = DVec3::new(x * 96.0, y * 96.0, z * 96.0);
        let hi = lo + DVec3::splat(64.0);
        let p = DVec3::new;
        let planes = [
            [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
            [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
            [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
            [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
            [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
            [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
        ];
        let sides = planes
            .iter()
            .map(|plane| {
                side_id += 1;
                hp_vmf::Side { id: side_id, plane: *plane, material: material.to_owned(), ..Default::default() }
            })
            .collect();
        map.world.solids.push(hp_vmf::Solid { id: (i + 10) as u32, sides, ..Default::default() });
    }
    map
}

fn frame(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, view: &ViewParams) -> Duration {
    let ctx = egui::Context::default();
    let t = Instant::now();
    am.begin_frame(&ctx, Some(gfx));
    am.sync_scene(doc, gfx, &opts());
    gfx.renderer.render_view(1, view, &Overlay::default());
    t.elapsed()
}

fn switch(am: &mut AssetManager, gfx: &mut Gfx, from: &mut Document, to: &mut Document) {
    am.park_scene(from, gfx);
    to.mark_everything_changed();
}

fn tabs_park_and_restore(gfx: &mut Gfx) {
    let root = fake_install("gpu_tabs");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    let view = persp(DVec3::new(-500.0, -500.0, 600.0), DVec3::new(1000.0, 1000.0, 0.0));
    let mut a = Document::from_map(boxes(300, "TEST/BRICK"));
    let mut b = Document::from_map(boxes(40, "DEV/DEV_MEASUREGENERIC01B"));
    frame(&mut am, &mut a, gfx, &view);
    assert_eq!(gfx.renderer.stats().objects, 300);
    a.set_selection(&[ObjRef::Solid(15)]);
    frame(&mut am, &mut a, gfx, &view);
    assert!(gfx.renderer.object_flags(15).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));

    let brick = am.render_material_id("TEST/BRICK");
    let texture_alive = |gfx: &Gfx| {
        gfx.renderer.material(brick).and_then(|d| d.texture).is_some_and(|t| gfx.renderer.texture_view(t).is_some())
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while (am.pending_work() > 0 || !texture_alive(gfx)) && Instant::now() < deadline {
        frame(&mut am, &mut a, gfx, &view);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(texture_alive(gfx), "the brick texture loads");

    switch(&mut am, gfx, &mut a, &mut b);
    assert_eq!(am.parked_scenes(), 1);
    assert_eq!(gfx.renderer.stats().objects, 0, "the renderer holds the shown map only");
    frame(&mut am, &mut b, gfx, &view);
    assert_eq!(gfx.renderer.stats().objects, 40);
    assert!(gfx.renderer.object_flags(15).is_some_and(|f| !f.contains(ObjectFlags::SELECTED)));
    assert!(texture_alive(gfx), "a parked scene keeps its materials");

    switch(&mut am, gfx, &mut b, &mut a);
    let gen_before = gfx.renderer.content_generation();
    frame(&mut am, &mut a, gfx, &view);
    assert_eq!(am.parked_scenes(), 1, "A restored, B parked");
    assert_eq!(gfx.renderer.stats().objects, 300);
    assert!(gfx.renderer.object_flags(15).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));
    assert!(gfx.renderer.content_generation() > gen_before, "the views draw again");
    assert_eq!(am.solids_to_upload(), 0);

    assert!(texture_alive(gfx));
    for (name, additive) in [("sprites/glow01.vmt", true), ("", false), ("../../x.spr", false), ("models/a.mdl", true)]
    {
        let id = am.sprite_material_id(name, additive);
        gfx.renderer.set_sprite_layer(
            hp_render::SPRITE_LAYER_PARTICLES,
            &[hp_render::SpriteInstance { pos: [0.0; 3], size: 8.0, material: id, color: [1.0; 4], object: 0 }],
        );
        frame(&mut am, &mut a, gfx, &view);
    }

    a.translate(&[ObjRef::Solid(15)], DVec3::new(0.0, 0.0, 16.0)).expect("move");
    frame(&mut am, &mut a, gfx, &view);
    assert_eq!(gfx.renderer.stats().objects, 300);

    switch(&mut am, gfx, &mut a, &mut b);
    frame(&mut am, &mut b, gfx, &view);
    a.translate(&[ObjRef::Solid(16)], DVec3::new(0.0, 0.0, 16.0)).expect("move");
    switch(&mut am, gfx, &mut b, &mut a);
    frame(&mut am, &mut a, gfx, &view);
    assert_eq!(gfx.renderer.stats().objects, 300);

    let mut c = Document::from_map(boxes(7, "TEST/BRICK"));
    c.mark_everything_changed();
    frame(&mut am, &mut c, gfx, &view);
    assert_eq!(gfx.renderer.stats().objects, 7);
    am.park_scene(&mut c, gfx);
    am.park_scene(&mut c, gfx);
    let mut d = Document::from_map(boxes(3, "TEST/BRICK"));
    frame(&mut am, &mut d, gfx, &view);
    assert_eq!(gfx.renderer.stats().objects, 3);
    c.mark_everything_changed();
    frame(&mut am, &mut c, gfx, &view);
    assert_eq!(gfx.renderer.stats().objects, 7);
}

fn texlights_light_the_lit_mode(gfx: &mut Gfx) {
    let root = fake_install("gpu_texlights");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    let dir = common::scratch("texlights");
    let path = dir.join("lit.vmf");
    hp_vmf::save(&boxes(3, "LIGHTS/WHITE001"), &path).expect("save");
    std::fs::write(
        dir.join("lit.rad"),
        "// test
lights/white001 255 255 255 400
hdr:lights/white001 1 1 1 1
",
    )
    .expect("rad");
    let mut doc = Document::open(&path).expect("open");
    let view = persp(DVec3::new(-300.0, -300.0, 300.0), DVec3::ZERO);
    let ctx = egui::Context::default();
    let mut lit = |am: &mut AssetManager, doc: &mut Document, on: bool| {
        let o = SceneOptions { lighting_preview: on, ..opts() };
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, &o);
        gfx.renderer.render_view(1, &view, &Overlay::default());
        gfx.renderer.light_count()
    };
    assert_eq!(lit(&mut am, &mut doc, false), 0);
    assert_eq!(lit(&mut am, &mut doc, true), 18);
    doc.delete(&[ObjRef::Solid(10)]).expect("delete");
    assert_eq!(lit(&mut am, &mut doc, true), 12);
    assert_eq!(lit(&mut am, &mut doc, false), 0);
    assert_eq!(lit(&mut am, &mut doc, true), 12);
}

fn big_map_frames(gfx: &mut Gfx, map: hp_vmf::Map) {
    let root = fake_install("gpu_tabs_big");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    let view = persp(DVec3::new(-1500.0, -1500.0, 5000.0), DVec3::new(2400.0, 2400.0, 1900.0));
    let t = Instant::now();
    let mut big = Document::from_map(map);
    eprintln!("document built in {:?}", t.elapsed());
    let mut small = Document::from_map(boxes(50, "TEST/BRICK"));
    frame(&mut am, &mut small, gfx, &view);
    switch(&mut am, gfx, &mut small, &mut big);
    let start = Instant::now();
    let first = frame(&mut am, &mut big, gfx, &view);
    let mut worst = Duration::ZERO;
    let mut frames = 1;
    while am.solids_to_upload() > 0 && frames < 5000 {
        worst = worst.max(frame(&mut am, &mut big, gfx, &view));
        frames += 1;
    }
    let total = start.elapsed();
    eprintln!("first upload: first frame {first:?}, then {frames} frames, worst {worst:?}, total {total:?}");
    switch(&mut am, gfx, &mut big, &mut small);
    let to_small = frame(&mut am, &mut small, gfx, &view);
    switch(&mut am, gfx, &mut small, &mut big);
    let back = frame(&mut am, &mut big, gfx, &view);
    let next = frame(&mut am, &mut big, gfx, &view);
    eprintln!("switch to the small map {to_small:?}, back to the big one {back:?}, next frame {next:?}");
    assert_eq!(am.solids_to_upload(), 0);
    assert!(first < Duration::from_millis(400), "first frame {first:?}");
    assert!(back < Duration::from_millis(200), "restore {back:?}");
}

#[test]
fn gpu_tabs() {
    hp_app::register_translations();
    hp_assets::register_translations();
    let Some(mut gfx) = gfx() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    tabs_park_and_restore(&mut gfx);
    texlights_light_the_lit_mode(&mut gfx);
    big_map_frames(&mut gfx, boxes(100_000, "TEST/BRICK"));
}

#[test]
#[ignore = "probe on a real map: HP_BIG_VMF=big.vmf cargo test -p hp-assets --release --test tabs_gpu -- --ignored --nocapture"]
fn probe_big_vmf() {
    let Ok(path) = std::env::var("HP_BIG_VMF") else { return };
    let Some(mut gfx) = gfx() else { return };
    let t = Instant::now();
    let map = hp_vmf::load(std::path::Path::new(&path)).expect("load");
    eprintln!("parsed in {:?}", t.elapsed());
    big_map_frames(&mut gfx, map);
}
