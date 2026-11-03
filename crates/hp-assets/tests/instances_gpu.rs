mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_install, fake_settings, scratch};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::runtime::instances::{
    InstanceCache, InstanceContext, InstanceScene, publish_context, publish_main_map, publish_scene,
};
use hp_app::settings::InstanceDisplay;
use hp_app::state::{Gfx, SceneOptions};
use hp_assets::AssetManager;
use hp_editor::instances::{LoadRequest, Loaded};
use hp_editor::{Document, ObjRef};
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
    if !adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) {
        return None;
    }
    let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hp-assets instances test"),
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

fn settle(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, o: &SceneOptions) {
    let ctx = egui::Context::default();
    let cache = InstanceCache::shared();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let generation = cache.generation();
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, o);
        let idle = cache.wait_idle(Duration::from_millis(50)) && cache.generation() == generation;
        let s = am.stats();
        if idle && s.queued_jobs == 0 && s.pending_uploads == 0 && s.materials_loading == 0 {
            am.begin_frame(&ctx, Some(gfx));
            am.sync_scene(doc, gfx, o);
            return;
        }
        assert!(Instant::now() < deadline, "the scene never settled");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn objects(gfx: &mut Gfx) -> u32 {
    gfx.renderer.flush();
    gfx.renderer.stats().objects
}

fn triangles(gfx: &mut Gfx, name: &str) -> u64 {
    let eye = DVec3::new(-200.0, -900.0, 500.0);
    let target = DVec3::new(300.0, 0.0, 0.0);
    let forward = (target - eye).normalize_or(DVec3::X);
    let params = ViewParams {
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
    };
    gfx.renderer.render_view(1, &params, &Overlay::default());
    let (w, h, px) = gfx.renderer.read_view_rgba8(1).expect("readback");
    let path = scratch("png").join(format!("{name}.png"));
    image::save_buffer(&path, &px, w, h, image::ColorType::Rgba8).expect("png");
    gfx.renderer.stats().triangles
}

fn cube(id: u32, min: [f64; 3], max: [f64; 3], material: &str) -> String {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let side = |i: u32, a: [f64; 3], b: [f64; 3], c: [f64; 3], u: &str, v: &str| {
        format!(
            "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{}\"\n\t\t\t\"plane\" \"({} {} {}) ({} {} {}) ({} {} {})\"\n\t\t\t\"material\" \"{material}\"\n\t\t\t\"uaxis\" \"[{u} 0] 0.25\"\n\t\t\t\"vaxis\" \"[{v} 0] 0.25\"\n\t\t}}\n",
            id * 10 + i,
            a[0],
            a[1],
            a[2],
            b[0],
            b[1],
            b[2],
            c[0],
            c[1],
            c[2]
        )
    };
    let mut s = format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n");
    s += &side(1, [x0, y1, z1], [x1, y1, z1], [x1, y0, z1], "1 0 0", "0 -1 0");
    s += &side(2, [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], "1 0 0", "0 -1 0");
    s += &side(3, [x1, y1, z1], [x0, y1, z1], [x0, y1, z0], "1 0 0", "0 0 -1");
    s += &side(4, [x1, y0, z0], [x0, y0, z0], [x0, y0, z1], "1 0 0", "0 0 -1");
    s += &side(5, [x1, y1, z0], [x1, y0, z0], [x1, y0, z1], "0 1 0", "0 0 -1");
    s += &side(6, [x0, y1, z1], [x0, y0, z1], [x0, y0, z0], "0 1 0", "0 0 -1");
    s += "\t}\n";
    s
}

fn vmf(solids: &str, entities: &str) -> String {
    format!("world\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n{solids}}}\n{entities}")
}

#[test]
fn instance_contents_follow_their_files_and_the_display() {
    hp_app::register_translations();
    hp_assets::register_translations();
    let Some(mut gfx) = gfx() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    let gfx = &mut gfx;
    publish_scene(InstanceScene::default());
    let root = fake_install("instances_gpu");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    let cache = InstanceCache::shared();
    cache.set_content(am.fs());

    let src = root.join("mapsrc");
    std::fs::create_dir_all(&src).expect("dir");
    let pillar_dir = root.join("garrysmod").join("maps").join("instances");
    std::fs::create_dir_all(&pillar_dir).expect("dir");
    let room_path = src.join("room.vmf");
    let room = vmf(
        &cube(2, [0.0, 0.0, 0.0], [256.0, 256.0, 16.0], "test/brick"),
        "entity\n{\n\t\"id\" \"5\"\n\t\"classname\" \"func_instance\"\n\t\"file\" \"instances/pillar\"\n\t\"origin\" \"128 128 16\"\n}\nentity\n{\n\t\"id\" \"6\"\n\t\"classname\" \"light\"\n\t\"origin\" \"64 64 64\"\n\t\"_light\" \"$color 200\"\n}\n",
    );
    std::fs::write(&room_path, &room).expect("room");
    std::fs::write(
        pillar_dir.join("pillar.vmf"),
        vmf(&cube(3, [-16.0, -16.0, 0.0], [16.0, 16.0, 128.0], "test/brick"), ""),
    )
    .expect("pillar");

    let mut doc = Document::from_map(hp_vmf::parse(hp_app::testing::TEST_VMF).expect("test map"));
    let main = src.join("main.vmf");
    doc.set_path(Some(main.clone()));
    publish_main_map(Some(&main));
    let o = opts();
    settle(&mut am, &mut doc, gfx, &o);
    let base = objects(gfx);
    let base_tris = triangles(gfx, "instances_none");

    let inst = doc
        .create_point_entity(
            "func_instance",
            DVec3::new(512.0, 0.0, 0.0),
            &[
                ("file".into(), "room.vmf".into()),
                ("angles".into(), "0 90 0".into()),
                ("replace01".into(), "$color 255 0 0".into()),
            ],
        )
        .expect("instance");
    doc.set_selection(&[]);
    settle(&mut am, &mut doc, gfx, &o);
    let shown = objects(gfx);
    assert!(shown > base + 1, "contents drawn: {base} then {shown}");
    let tris = triangles(gfx, "instances_tinted");
    assert!(tris > base_tris, "the room and the nested pillar are drawn: {base_tris} then {tris}");
    let req = LoadRequest { file: "room", containing: None, depth: 0 };
    let Loaded::Ready { path, .. } = cache.load(&req, Some(&main)) else { panic!("room not loaded") };
    let first = cache.file(&path).expect("room").revision;

    publish_scene(InstanceScene { display: InstanceDisplay::Hidden, ..InstanceScene::default() });
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), base + 1, "only the instance entity box");
    publish_scene(InstanceScene { display: InstanceDisplay::Normal, ..InstanceScene::default() });
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), shown);
    triangles(gfx, "instances_normal");
    publish_scene(InstanceScene::default());
    am.sync_scene(&mut doc, gfx, &o);

    doc.set_selection(&[ObjRef::Entity(inst)]);
    am.sync_scene(&mut doc, gfx, &o);
    triangles(gfx, "instances_selected");

    std::thread::sleep(Duration::from_millis(30));
    let bigger = room.replacen(
        "\tsolid\n",
        &format!("{}\tsolid\n", cube(4, [0.0, 0.0, 16.0], [32.0, 32.0, 300.0], "test/brick")),
        1,
    );
    std::fs::write(&room_path, bigger).expect("room");
    let deadline = Instant::now() + Duration::from_secs(10);
    while cache.file(&path).is_none_or(|f| f.revision == first) {
        assert!(Instant::now() < deadline, "the changed file was not reloaded");
        cache.poll();
        cache.wait_idle(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(50));
    }
    settle(&mut am, &mut doc, gfx, &o);
    assert_eq!(objects(gfx), shown);
    let after = triangles(gfx, "instances_reloaded");
    assert!(after > tris, "the new brush is drawn: {tris} then {after}");

    doc.delete(&[ObjRef::Entity(inst)]).expect("delete");
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), base);
    assert!(doc.undo());
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), shown);

    doc.save(&main).expect("save main");
    let mut room_doc = Document::open(&room_path).expect("room");
    publish_main_map(Some(&room_path));
    settle(&mut am, &mut room_doc, gfx, &o);
    let alone = objects(gfx);
    publish_context(Some(&InstanceContext { parent: main.clone(), entity: inst }));
    settle(&mut am, &mut room_doc, gfx, &o);
    let with_context = objects(gfx);
    assert!(with_context > alone, "the parent map is drawn around the instance: {alone} then {with_context}");
    triangles(gfx, "instances_context");
    publish_scene(InstanceScene { show_context: false, ..InstanceScene::default() });
    am.sync_scene(&mut room_doc, gfx, &o);
    assert_eq!(objects(gfx), alone, "the context display can be turned off");
    publish_scene(InstanceScene::default());
    publish_context(None);
    am.sync_scene(&mut room_doc, gfx, &o);
    assert_eq!(objects(gfx), alone);

    publish_main_map(None);
    let _ = std::fs::remove_dir_all(&root);
}
