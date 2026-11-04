mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fake_install, fake_settings, real_gmod, real_settings, scratch};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::state::{Gfx, PreviewCamera, SceneOptions, ThumbState};
use hp_assets::AssetManager;
use hp_editor::{Document, ObjRef};
use hp_render::{GridParams, MaterialFlags, ObjectFlags, OrthoAxis, Overlay, RenderMode, TexId, ViewKind, ViewParams};

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

fn opts() -> SceneOptions {
    SceneOptions {
        show_models: true,
        show_helpers: true,
        show_tool_textures: true,
        lighting_preview: false,
        model_lods: false,
    }
}

fn settle(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, o: &SceneOptions, timeout: Duration) -> bool {
    let ctx = egui::Context::default();
    let deadline = Instant::now() + timeout;
    loop {
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, o);
        let s = am.stats();
        if s.queued_jobs == 0
            && s.pending_uploads == 0
            && s.textures_loading == 0
            && s.models_loading == 0
            && s.materials_loading == 0
        {
            am.begin_frame(&ctx, Some(gfx));
            am.sync_scene(doc, gfx, o);
            return true;
        }
        if Instant::now() > deadline {
            eprintln!("not settled: {s:?}");
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn save(gfx: &Gfx, view: u32, name: &str) -> (u32, u32, Vec<u8>) {
    let (w, h, px) = gfx.renderer.read_view_rgba8(view).expect("readback");
    let path = scratch("png").join(format!("{name}.png"));
    image::save_buffer(&path, &px, w, h, image::ColorType::Rgba8).expect("png");
    eprintln!("wrote {}", path.display());
    (w, h, px)
}

fn distinct_colors(px: &[u8]) -> usize {
    let mut set = std::collections::HashSet::new();
    for p in px.chunks_exact(4) {
        set.insert([p[0] >> 2, p[1] >> 2, p[2] >> 2]);
    }
    set.len()
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

fn top(center: DVec3, ppu: f32, size: [u32; 2]) -> ViewParams {
    ViewParams {
        game_pixel_grid: false,
        size,
        kind: ViewKind::Ortho { axis: OrthoAxis::Top, center: center.as_vec3().to_array(), pixels_per_unit: ppu },
        mode: RenderMode::Wireframe,
        grid: Some(GridParams { size: 64.0, major_every: 8, show_1024: true, opacity: 0.6, cells: true }),
        background: [0.08, 0.08, 0.09],
        selection_color: [1.0, 0.25, 0.25, 0.45],
        show_sprites: false,
        show_models: true,
        msaa: true,
    }
}

fn map_bounds(doc: &Document) -> hp_geom::Aabb {
    let mut b = hp_geom::Aabb::EMPTY;
    for n in doc.scene().solids().values() {
        if !n.bounds.is_empty() {
            b = b.union(&n.bounds);
        }
    }
    b
}

fn synthetic_scene(gfx: &mut Gfx) {
    let root = fake_install("gpu_fake");
    let mut am = AssetManager::new(&fake_settings(&root));
    let mut doc = Document::from_map(hp_vmf::parse(hp_app::testing::TEST_VMF).expect("test map"));
    let o = opts();
    am.sync_scene(&mut doc, gfx, &o);
    assert!(settle(&mut am, &mut doc, gfx, &o, Duration::from_secs(30)));
    gfx.renderer.flush();
    let stats = gfx.renderer.stats();
    eprintln!("synthetic: {stats:?} {:?}", am.stats());
    assert!(stats.objects >= 5, "3 solids + 2 point entities");
    let dev = am.render_material_id("DEV/DEV_MEASUREGENERIC01B");
    assert_eq!(dev, am.render_material_id("dev/dev_measuregeneric01b"), "stable ids");
    let desc = gfx.renderer.material(dev).expect("material set");
    assert!(desc.texture.is_some_and(|t| t != TexId::LOADING && t != TexId::MISSING), "{desc:?}");
    assert_eq!(desc.tex_size, [128.0, 128.0]);
    let trig = am.render_material_id("tools/toolstrigger");
    assert!(gfx.renderer.material(trig).is_some_and(|d| d.flags.contains(MaterialFlags::TOOL)));
    let missing = am.render_material_id("no/such/material");
    let _ = settle(&mut am, &mut doc, gfx, &o, Duration::from_secs(10));
    assert_eq!(gfx.renderer.material(missing).and_then(|d| d.texture), Some(TexId::MISSING));

    doc.set_selection(&[ObjRef::Solid(2)]);
    am.sync_scene(&mut doc, gfx, &o);
    assert!(gfx.renderer.object_flags(2).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));
    doc.set_selection(&[ObjRef::Entity(6)]);
    am.sync_scene(&mut doc, gfx, &o);
    assert!(!gfx.renderer.object_flags(2).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));
    assert!(gfx.renderer.object_flags(7).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));
    doc.set_selection(&[ObjRef::Entity(4)]);
    am.sync_scene(&mut doc, gfx, &o);
    let ent4 = 4 | 0x8000_0000;
    assert!(gfx.renderer.object_flags(ent4).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));
    assert_eq!(doc.hide_selection(), 1);
    am.sync_scene(&mut doc, gfx, &o);
    assert!(gfx.renderer.object_flags(ent4).is_some_and(|f| f.contains(ObjectFlags::HIDDEN)));
    doc.unhide_all();
    am.sync_scene(&mut doc, gfx, &o);
    assert!(!gfx.renderer.object_flags(ent4).is_some_and(|f| f.contains(ObjectFlags::HIDDEN)));

    let hide = SceneOptions { show_tool_textures: false, lighting_preview: true, ..o };
    am.sync_scene(&mut doc, gfx, &hide);
    assert!(gfx.renderer.material(trig).is_some_and(|d| d.flags.contains(MaterialFlags::NODRAW_IN_3D)));
    am.sync_scene(&mut doc, gfx, &o);
    assert!(!gfx.renderer.material(trig).is_some_and(|d| d.flags.contains(MaterialFlags::NODRAW_IN_3D)));

    let before = gfx.renderer.stats().objects;
    doc.translate(&[ObjRef::Solid(3)], DVec3::new(0.0, 0.0, 64.0)).expect("move");
    doc.set_entity_key(&[5], "_light", "255 0 0 400").expect("key");
    let id = doc.create_point_entity("info_player_start", DVec3::new(128.0, 0.0, 0.0), &[]).expect("create");
    am.sync_scene(&mut doc, gfx, &o);
    gfx.renderer.flush();
    assert_eq!(gfx.renderer.stats().objects, before + 1);
    doc.delete(&[ObjRef::Entity(id), ObjRef::Solid(2)]).expect("delete");
    am.sync_scene(&mut doc, gfx, &o);
    gfx.renderer.flush();
    assert_eq!(gfx.renderer.stats().objects, before - 1);
    assert!(doc.undo());
    am.sync_scene(&mut doc, gfx, &o);
    gfx.renderer.flush();
    assert_eq!(gfx.renderer.stats().objects, before + 1);

    let p = persp(DVec3::new(-600.0, -700.0, 450.0), DVec3::new(0.0, 0.0, 0.0), [960, 600], RenderMode::Textured);
    gfx.renderer.render_view(1, &p, &Overlay::default());
    let img = save(gfx, 1, "synthetic_3d");
    assert!(distinct_colors(&img.2) > 20);

    let objects = |gfx: &mut Gfx| {
        gfx.renderer.flush();
        gfx.renderer.stats().objects
    };
    let before = objects(gfx);
    let kv = [("texture".to_owned(), "decals/test".to_owned())];
    let decal = doc.create_point_entity("infodecal", DVec3::new(0.0, 255.0, 128.0), &kv).expect("decal");
    assert!(settle(&mut am, &mut doc, gfx, &o, Duration::from_secs(10)));
    assert_eq!(objects(gfx), before + 2, "the entity box and its decal");
    let decal_obj = decal | 0xC000_0000;
    doc.set_selection(&[ObjRef::Entity(decal)]);
    am.sync_scene(&mut doc, gfx, &o);
    assert!(gfx.renderer.object_flags(decal_obj).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));
    let p = persp(DVec3::new(0.0, 0.0, 128.0), DVec3::new(0.0, 256.0, 128.0), [480, 300], RenderMode::Textured);
    gfx.renderer.render_view(2, &p, &Overlay::default());
    save(gfx, 2, "synthetic_decal");
    doc.translate(&[ObjRef::Solid(3)], DVec3::new(0.0, 0.0, 1024.0)).expect("move");
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), before + 1, "no surface, no decal");
    assert!(doc.undo());
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), before + 2);
    doc.delete(&[ObjRef::Entity(decal)]).expect("delete");
    am.sync_scene(&mut doc, gfx, &o);
    assert_eq!(objects(gfx), before);
}

fn render_map(gfx: &mut Gfx, am: &mut AssetManager, path: &std::path::Path, name: &str) -> Option<AssetStatsLite> {
    let Ok(mut doc) = Document::open(path) else {
        eprintln!("{}: not found, skipped", path.display());
        return None;
    };
    let o = opts();
    let t = Instant::now();
    let ctx = egui::Context::default();
    am.begin_frame(&ctx, Some(gfx));
    am.sync_scene(&mut doc, gfx, &o);
    eprintln!("{name}: first sync {:?}", t.elapsed());
    let settled = settle(am, &mut doc, gfx, &o, Duration::from_secs(120));
    gfx.renderer.flush();
    eprintln!("{name}: loaded in {:?} (settled: {settled}) {:?} {:?}", t.elapsed(), am.stats(), gfx.renderer.stats());
    assert!(settled, "{name}");

    let b = map_bounds(&doc);
    let c = b.center();
    let size = b.size();
    let inside = b.min + size * DVec3::new(0.05, 0.05, 0.8);
    let below = DVec3::new(c.x, c.y, b.min.z + size.z * 0.15);
    gfx.renderer.render_view(10, &persp(inside, below, [1280, 800], RenderMode::Textured), &Overlay::default());
    let img = save(gfx, 10, &format!("{name}_3d_overview"));
    let colors = distinct_colors(&img.2);

    let spawn = doc
        .scene()
        .entities()
        .values()
        .find(|n| n.entity.classname.starts_with("info_player"))
        .and_then(|n| n.entity.origin())
        .unwrap_or(c);
    let eye = spawn + DVec3::new(0.0, 0.0, 64.0);
    let look = DVec3::new(c.x, c.y, eye.z - 40.0);
    gfx.renderer.render_view(11, &persp(eye, look, [1280, 800], RenderMode::Textured), &Overlay::default());
    save(gfx, 11, &format!("{name}_3d_spawn"));

    let lit = SceneOptions { lighting_preview: true, ..o };
    am.sync_scene(&mut doc, gfx, &lit);
    gfx.renderer.render_view(12, &persp(inside, below, [1280, 800], RenderMode::Lit), &Overlay::default());
    save(gfx, 12, &format!("{name}_3d_lit"));
    am.sync_scene(&mut doc, gfx, &o);

    let ppu = (1200.0 / size.x.max(size.y).max(1.0)) as f32;
    gfx.renderer.render_view(13, &top(c, ppu, [1280, 1280]), &Overlay::default());
    let img = save(gfx, 13, &format!("{name}_top"));
    assert!(distinct_colors(&img.2) > 5);
    let s = am.stats();
    Some(AssetStatsLite {
        colors,
        textures: s.textures_ready,
        models: s.models_ready,
        gpu_textures: gfx.renderer.stats().textures,
    })
}

struct AssetStatsLite {
    colors: usize,
    textures: usize,
    models: usize,
    gpu_textures: u32,
}

fn real_maps(gfx: &mut Gfx) {
    let Some(root) = real_gmod() else {
        eprintln!("Garry's Mod not found: real map renders skipped");
        return;
    };
    let mut am = AssetManager::new(&real_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(120)));
    let examples = root.join("garrysmod/gamemodes/terrortown/mapexamples");
    let traps = render_map(gfx, &mut am, &examples.join("ttt_traps.vmf"), "ttt_traps");
    if let Some(s) = &traps {
        assert!(s.colors > 200 && s.textures >= 3 && s.models >= 1, "{} {} {}", s.colors, s.textures, s.models);
    }
    if let Some(s) = render_map(gfx, &mut am, &examples.join("ttt_weaponcheck.vmf"), "ttt_weaponcheck") {
        assert!(s.models >= 2, "keypad props: {}", s.models);
    }
    let _ = render_map(gfx, &mut am, &examples.join("ttt_traitorcheck.vmf"), "ttt_traitorcheck");
    let _ = render_map(
        gfx,
        &mut am,
        std::path::Path::new(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf"),
        "hammermcp_test",
    );
    let again = render_map(gfx, &mut am, &examples.join("ttt_traps.vmf"), "ttt_traps_again");
    if let (Some(first), Some(again)) = (traps, again) {
        assert_eq!(again.textures, first.textures, "only the textures of the open map stay on the GPU");
        assert_eq!(again.gpu_textures, first.gpu_textures);
    }

    showcase(gfx, &mut am);

    let cam = PreviewCamera::default();
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut tex = None;
    while Instant::now() < deadline {
        am.begin_frame(&ctx, Some(gfx));
        tex = am.model_preview(gfx, "models/props_c17/oildrum001.mdl", &cam, [384, 384]);
        if tex.is_some() && am.stats().queued_jobs == 0 && am.stats().pending_uploads == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(tex.is_some());
    for _ in 0..5 {
        am.begin_frame(&ctx, Some(gfx));
        std::thread::sleep(Duration::from_millis(20));
    }
    let again = am.model_preview(gfx, "models/props_c17/oildrum001.mdl", &cam, [384, 384]);
    assert_eq!(again, tex, "same egui texture while the size is unchanged");
    let (w, h, px) = am.read_preview_rgba8().expect("preview readback");
    image::save_buffer(scratch("png").join("preview_oildrum.png"), &px, w, h, image::ColorType::Rgba8).expect("png");
    assert!(distinct_colors(&px) > 30);
    let wire = PreviewCamera { wireframe: true, yaw_deg: 45.0, skin: 2, ..cam };
    let _ = am.model_preview(gfx, "models/props_c17/oildrum001.mdl", &wire, [384, 384]);
    for _ in 0..50 {
        am.begin_frame(&ctx, Some(gfx));
        let _ = am.model_preview(gfx, "models/props_c17/oildrum001.mdl", &wire, [384, 384]);
        if am.stats().queued_jobs == 0 && am.stats().pending_uploads == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let (w, h, px) = am.read_preview_rgba8().expect("preview readback");
    image::save_buffer(scratch("png").join("preview_oildrum_wire_skin2.png"), &px, w, h, image::ColorType::Rgba8)
        .expect("png");
    preview_follows_normal_maps(gfx, &mut am);
}

const BUMPED_PROP: &str = "models/items/ammocrate_pistol.mdl";

fn differing_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 8))
        .count()
}

fn preview_follows_normal_maps(gfx: &mut Gfx, am: &mut AssetManager) {
    use hp_app::runtime::render::{RenderDisplay, publish_render_display, render_display};
    let ctx = egui::Context::default();
    let cam = PreviewCamera { yaw_deg: 30.0, ..PreviewCamera::default() };
    let settle = |am: &mut AssetManager, gfx: &mut Gfx| -> (u32, u32, Vec<u8>) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            am.begin_frame(&ctx, Some(gfx));
            let tex = am.model_preview(gfx, BUMPED_PROP, &cam, [256, 256]);
            let s = am.stats();
            let idle = am.pending_work() == 0 && s.materials_loading == 0;
            if tex.is_some()
                && idle
                && s.queued_jobs == 0
                && s.pending_uploads == 0
                && am.preview_texture_counts().1 == 0
            {
                am.begin_frame(&ctx, Some(gfx));
                let _ = am.model_preview(gfx, BUMPED_PROP, &cam, [256, 256]);
                return am.read_preview_rgba8().expect("preview readback");
            }
            assert!(Instant::now() < deadline, "the preview of {BUMPED_PROP} did not settle");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let thumb_ready = |am: &mut AssetManager, gfx: &mut Gfx| {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            am.begin_frame(&ctx, Some(gfx));
            if matches!(am.model_thumbnail(&ctx, BUMPED_PROP), ThumbState::Ready { .. }) {
                return;
            }
            assert!(Instant::now() < deadline, "the thumbnail of {BUMPED_PROP} was not drawn");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let saved = render_display();
    publish_render_display(RenderDisplay { normal_maps: true, ..saved });
    let on = settle(am, gfx);
    assert!(am.preview_texture_counts().2 >= 1, "the normal map of the prop is loaded");
    thumb_ready(am, gfx);
    image::save_buffer(scratch("png").join("preview_bump_on.png"), &on.2, on.0, on.1, image::ColorType::Rgba8)
        .expect("png");

    publish_render_display(RenderDisplay { normal_maps: false, ..saved });
    am.begin_frame(&ctx, Some(gfx));
    let _ = am.model_preview(gfx, BUMPED_PROP, &cam, [256, 256]);
    let now = am.read_preview_rgba8().expect("preview readback");
    assert!(differing_pixels(&on.2, &now.2) > 200, "flat at once: {} pixels changed", differing_pixels(&on.2, &now.2));
    assert_eq!(am.preview_texture_counts().2, 0, "no normal map left in the preview");
    assert!(matches!(am.model_thumbnail(&ctx, BUMPED_PROP), ThumbState::Loading), "the thumbnail is drawn again");
    thumb_ready(am, gfx);
    let off = settle(am, gfx);
    assert!(differing_pixels(&now.2, &off.2) < 50, "nothing else changes when settled");
    image::save_buffer(scratch("png").join("preview_bump_off.png"), &off.2, off.0, off.1, image::ColorType::Rgba8)
        .expect("png");

    publish_render_display(RenderDisplay { normal_maps: true, ..saved });
    am.begin_frame(&ctx, Some(gfx));
    assert!(matches!(am.model_thumbnail(&ctx, BUMPED_PROP), ThumbState::Loading), "the thumbnail is drawn again");
    let again = settle(am, gfx);
    assert!(
        differing_pixels(&on.2, &again.2) < 50,
        "the same relief: {} pixels differ",
        differing_pixels(&on.2, &again.2)
    );
    thumb_ready(am, gfx);
    publish_render_display(saved);
}

fn block_sides(lo: DVec3, hi: DVec3, materials: [&str; 6], first_id: u32) -> Vec<hp_vmf::Side> {
    let p = DVec3::new;
    let planes = [
        [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
        [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
        [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
        [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
        [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
        [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
    ];
    planes
        .iter()
        .enumerate()
        .map(|(k, plane)| {
            let (u, v) = if k < 2 {
                (DVec3::X, DVec3::NEG_Y)
            } else if k < 4 {
                (DVec3::Y, DVec3::NEG_Z)
            } else {
                (DVec3::X, DVec3::NEG_Z)
            };
            hp_vmf::Side {
                id: first_id + k as u32,
                plane: *plane,
                material: materials[k].to_owned(),
                uaxis: hp_vmf::TexAxis { axis: u, shift: 0.0, scale: 0.25 },
                vaxis: hp_vmf::TexAxis { axis: v, shift: 0.0, scale: 0.25 },
                ..Default::default()
            }
        })
        .collect()
}

fn showcase(gfx: &mut Gfx, am: &mut AssetManager) {
    let mut map = hp_vmf::Map::new_empty();
    let blend = "NATURE/BLENDGRASSGRAVEL001A";
    let nodraw = "TOOLS/TOOLSNODRAW";
    let mut ground = block_sides(
        DVec3::new(-1024.0, -1024.0, -64.0),
        DVec3::splat(1024.0).with_z(0.0),
        [blend, nodraw, nodraw, nodraw, nodraw, nodraw],
        1,
    );
    let mut disp = hp_vmf::DispInfo::new_flat(4, DVec3::new(-1024.0, -1024.0, 0.0), DVec3::Z);
    let n = disp.row_len();
    for i in 0..n {
        for j in 0..n {
            let (x, y) = (i as f64 / (n - 1) as f64, j as f64 / (n - 1) as f64);
            let k = i * n + j;
            disp.distances[k] = 160.0 * (x * std::f64::consts::PI).sin() * (y * std::f64::consts::PI).sin();
            disp.alphas[k] = if (x - 0.5).abs() < 0.2 || (y - 0.5).abs() < 0.12 { 255.0 } else { 0.0 };
        }
    }
    ground[0].disp = Some(Box::new(disp));
    map.world.solids.push(hp_vmf::Solid { id: 100, sides: ground, ..Default::default() });
    let wall = "BRICK/BRICKWALL001A";
    let concrete = "CONCRETE/CONCRETEFLOOR001A";
    map.world.solids.push(hp_vmf::Solid {
        id: 101,
        sides: block_sides(
            DVec3::new(-900.0, 600.0, 0.0),
            DVec3::new(900.0, 640.0, 320.0),
            [concrete, concrete, wall, wall, wall, wall],
            20,
        ),
        ..Default::default()
    });
    let glass = "GLASS/GLASSWINDOW007A";
    map.world.solids.push(hp_vmf::Solid {
        id: 102,
        sides: block_sides(DVec3::new(-200.0, 560.0, 64.0), DVec3::new(200.0, 576.0, 256.0), [glass; 6], 40),
        ..Default::default()
    });
    let mut ent = |id: u32, class: &str, props: &[(&str, &str)]| {
        let mut e = hp_vmf::Entity { id, classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        map.entities.push(e);
    };
    for (i, (x, y)) in [(-500.0, 200.0), (-380.0, 260.0), (300.0, 150.0), (420.0, 330.0)].iter().enumerate() {
        let model = if i % 2 == 0 { "models/props_c17/oildrum001.mdl" } else { "models/props_junk/wood_crate001a.mdl" };
        ent(
            200 + i as u32,
            "prop_static",
            &[
                ("origin", &format!("{x} {y} 140")),
                ("angles", &format!("0 {} 0", i * 35)),
                ("model", model),
                ("skin", &i.to_string()),
            ],
        );
    }
    ent(210, "prop_physics", &[("origin", "0 300 120"), ("model", "models/props_c17/furniturecouch002a.mdl")]);
    ent(
        211,
        "light_environment",
        &[
            ("origin", "0 0 600"),
            ("angles", "-50 30 0"),
            ("pitch", "-50"),
            ("_light", "255 240 210 400"),
            ("_ambient", "120 140 170 60"),
        ],
    );
    ent(212, "light", &[("origin", "-300 450 200"), ("_light", "255 120 40 300")]);
    ent(
        213,
        "env_sprite",
        &[
            ("origin", "300 450 220"),
            ("model", "sprites/glow01.spr"),
            ("rendermode", "5"),
            ("rendercolor", "60 160 255"),
            ("scale", "1"),
        ],
    );
    ent(214, "info_player_start", &[("origin", "-100 -300 120"), ("angles", "0 90 0")]);
    let mut doc = Document::from_map(map);
    let o = opts();
    assert!(settle(am, &mut doc, gfx, &o, Duration::from_secs(60)));
    eprintln!("showcase: {:?}", am.stats());
    let eye = DVec3::new(-1100.0, -1300.0, 700.0);
    let target = DVec3::new(0.0, 250.0, 60.0);
    gfx.renderer.render_view(30, &persp(eye, target, [1280, 800], RenderMode::Textured), &Overlay::default());
    let img = save(gfx, 30, "showcase_textured");
    assert!(distinct_colors(&img.2) > 500);
    let lit = SceneOptions { lighting_preview: true, ..o };
    am.sync_scene(&mut doc, gfx, &lit);
    gfx.renderer.render_view(31, &persp(eye, target, [1280, 800], RenderMode::Lit), &Overlay::default());
    save(gfx, 31, "showcase_lit");
    doc.set_selection(&[ObjRef::Entity(202), ObjRef::Solid(101)]);
    am.sync_scene(&mut doc, gfx, &o);
    gfx.renderer.render_view(32, &persp(eye, target, [1280, 800], RenderMode::TexturedWire), &Overlay::default());
    save(gfx, 32, "showcase_selected_wire");
    gfx.renderer.render_view(33, &top(DVec3::ZERO, 0.55, [1280, 1280]), &Overlay::default());
    save(gfx, 33, "showcase_top");
}

fn big_map(gfx: &mut Gfx) {
    let root = fake_install("gpu_big");
    let mut am = AssetManager::new(&fake_settings(&root));
    assert!(am.wait_until_mounted(Duration::from_secs(30)));
    let materials = ["TEST/BRICK", "DEV/DEV_MEASUREGENERIC01B", "CONCRETE/CONCRETEFLOOR001A", "TOOLS/TOOLSNODRAW"];
    let mut map = hp_vmf::Map::new_empty();
    let n = 100_000usize;
    let mut side_id = 1;
    for i in 0..n {
        let (x, y, z) = ((i % 50) as f64, ((i / 50) % 50) as f64, (i / 2500) as f64);
        let lo = DVec3::new(x * 96.0, y * 96.0, z * 96.0);
        let hi = lo + DVec3::splat(64.0);
        let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
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
            .enumerate()
            .map(|(k, plane)| {
                side_id += 1;
                let (u, v) = if k < 2 {
                    (DVec3::X, DVec3::NEG_Y)
                } else if k < 4 {
                    (DVec3::Y, DVec3::NEG_Z)
                } else {
                    (DVec3::X, DVec3::NEG_Z)
                };
                hp_vmf::Side {
                    id: side_id,
                    plane: *plane,
                    material: materials[(i + k) % materials.len()].to_owned(),
                    uaxis: hp_vmf::TexAxis { axis: u, shift: 0.0, scale: 0.25 },
                    vaxis: hp_vmf::TexAxis { axis: v, shift: 0.0, scale: 0.25 },
                    ..Default::default()
                }
            })
            .collect();
        map.world.solids.push(hp_vmf::Solid { id: (i + 10) as u32, sides, ..Default::default() });
    }
    let t = Instant::now();
    let mut doc = Document::from_map(map.clone());
    let build = t.elapsed();
    let o = opts();
    let ctx = egui::Context::default();
    let view = persp(
        DVec3::new(-1500.0, -1500.0, 5000.0),
        DVec3::new(2400.0, 2400.0, 1900.0),
        [960, 600],
        RenderMode::Textured,
    );
    {
        let mut doc = Document::from_map(map);
        am.begin_frame(&ctx, Some(gfx));
        let start = Instant::now();
        let mut frames = 0;
        let mut worst = Duration::ZERO;
        loop {
            let t = Instant::now();
            am.sync_scene(&mut doc, gfx, &o);
            gfx.renderer.render_view(21, &view, &Overlay::default());
            worst = worst.max(t.elapsed());
            frames += 1;
            if am.solids_to_upload() == 0 || frames > 2000 {
                break;
            }
        }
        let t = Instant::now();
        gfx.renderer.render_view(21, &view, &Overlay::default());
        let full_frame = t.elapsed();
        eprintln!(
            "100k solids progressive: {frames} frames in {:?}, worst frame {worst:?}, frame with everything {full_frame:?}",
            start.elapsed()
        );
        assert_eq!(am.solids_to_upload(), 0);
        assert_eq!(gfx.renderer.stats().objects, 100_000);
    }
    am.set_progressive_upload(false);
    am.begin_frame(&ctx, Some(gfx));
    let t = Instant::now();
    am.sync_scene(&mut doc, gfx, &o);
    let sync = t.elapsed();
    let t = Instant::now();
    gfx.renderer.flush();
    let flush = t.elapsed();
    let t = Instant::now();
    doc.set_selection(&[ObjRef::Solid(500)]);
    am.sync_scene(&mut doc, gfx, &o);
    let select = t.elapsed();
    let t = Instant::now();
    doc.translate(&[ObjRef::Solid(500)], DVec3::new(0.0, 0.0, 16.0)).expect("move");
    am.sync_scene(&mut doc, gfx, &o);
    let edit = t.elapsed();
    eprintln!(
        "100k solids: document {build:?}, full sync {sync:?}, renderer flush {flush:?}, selection sync {select:?}, one brush edit {edit:?}"
    );
    assert_eq!(gfx.renderer.stats().objects, 100_000);
    assert!(gfx.renderer.object_flags(500).is_some_and(|f| f.contains(ObjectFlags::SELECTED)));
    let _ = settle(&mut am, &mut doc, gfx, &o, Duration::from_secs(30));
    let c = DVec3::new(2400.0, 2400.0, 1900.0);
    gfx.renderer.render_view(
        20,
        &persp(DVec3::new(-1500.0, -1500.0, 5000.0), c, [960, 600], RenderMode::Textured),
        &Overlay::default(),
    );
    save(gfx, 20, "big_100k");
}

#[test]
fn gpu_scene_textures_models_and_benchmark() {
    hp_app::register_translations();
    hp_assets::register_translations();
    let Some(mut gfx) = gfx() else {
        eprintln!("no GPU adapter available: test skipped");
        return;
    };
    eprintln!("BC textures: {}", gfx.renderer.supports_bc());
    synthetic_scene(&mut gfx);
    real_maps(&mut gfx);
    big_map(&mut gfx);
}
