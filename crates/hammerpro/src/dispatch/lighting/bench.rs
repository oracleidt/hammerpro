use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use glam::{DVec3, Vec3};
use hp_app::AssetsApi;
use hp_app::state::{Gfx, SceneOptions};
use hp_assets::AssetManager;
use hp_editor::Document;
use hp_render::lightmap::LightmapDisplay;
use hp_render::{Overlay, RenderMode, ViewKind, ViewParams};

use super::{bake, bsp, child, memory, show_geometry, write_texels};

const GMOD: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

struct Log;

impl log::Log for Log {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.target().starts_with("hammerpro")
    }

    fn log(&self, r: &log::Record<'_>) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let text = r.args().to_string();
        if r.level() <= log::Level::Info
            || text.starts_with("preview lighting")
            || text.starts_with("preview compile")
            || text.contains("memory")
        {
            static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let t = START.get_or_init(Instant::now).elapsed().as_secs_f32();
            eprintln!("LOG {t:8.2} {text}");
        }
    }

    fn flush(&self) {}
}

fn mb(b: u64) -> u64 {
    b >> 20
}

struct Sampler {
    peak: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl Sampler {
    fn start() -> Self {
        let peak = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (p, s) = (peak.clone(), stop.clone());
        let trace = std::env::var_os("HP_BENCH_TRACE").is_some();
        std::thread::spawn(move || {
            let mut last = 0u64;
            while !s.load(Ordering::Relaxed) {
                if let Some(u) = memory::of_self() {
                    p.fetch_max(u.working_set, Ordering::Relaxed);
                    if trace && u.working_set.abs_diff(last) > (6 << 20) {
                        last = u.working_set;
                        eprintln!("MEM {}", u.working_set >> 20);
                    }
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        });
        Self { peak, stop }
    }

    fn reset(&self) {
        self.peak.store(memory::of_self().map_or(0, |u| u.working_set), Ordering::Relaxed);
    }

    fn peak(&self) -> u64 {
        self.peak.load(Ordering::Relaxed)
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn now() -> u64 {
    memory::of_self().map_or(0, |u| u.working_set)
}

fn gfx() -> Option<Gfx> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .ok()?;
    eprintln!("adapter: {}", adapter.get_info().name);
    let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hammerpro bake bench"),
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

fn settle(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, o: &SceneOptions, eye: Vec3, target: Vec3) {
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut rounds = 0;
    loop {
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, o);
        let mut p = persp(eye, target, 0.0);
        p.mode = RenderMode::Textured;
        gfx.renderer.render_view(41, &p, &Overlay::default());
        rounds += 1;
        let s = am.stats();
        let idle = s.queued_jobs == 0
            && s.pending_uploads == 0
            && s.textures_loading == 0
            && s.models_loading == 0
            && s.materials_loading == 0
            && am.solids_to_upload() == 0;
        if (idle && rounds > 60) || Instant::now() > deadline {
            am.begin_frame(&ctx, Some(gfx));
            am.sync_scene(doc, gfx, o);
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn persp(eye: Vec3, target: Vec3, turn: f32) -> ViewParams {
    let forward = glam::Quat::from_rotation_z(turn) * (target - eye).normalize_or(Vec3::X);
    ViewParams {
        game_pixel_grid: false,
        size: [1600, 900],
        kind: ViewKind::Perspective {
            eye: eye.to_array(),
            forward: forward.to_array(),
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 70.0,
            near: 4.0,
            far: 30000.0,
        },
        mode: RenderMode::Lit,
        grid: None,
        background: [0.14, 0.15, 0.17],
        selection_color: [1.0, 0.25, 0.25, 0.45],
        show_sprites: true,
        show_models: true,
        msaa: true,
    }
}

fn frames(gfx: &mut Gfx, label: &str, eye: Vec3, target: Vec3, mode: RenderMode) {
    let n = 40;
    let mut cpu = Vec::with_capacity(n);
    let mut total = Vec::with_capacity(n);
    let mut draws = 0;
    for i in 0..n + 4 {
        let mut p = persp(eye, target, (i % 10) as f32 * 0.02);
        p.mode = mode;
        let t = Instant::now();
        gfx.renderer.render_view(40, &p, &Overlay::default());
        let c = t.elapsed();
        let _ = gfx.device.poll(wgpu::PollType::wait_indefinitely());
        if i >= 4 {
            cpu.push(c.as_secs_f64() * 1000.0);
            total.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        draws = gfx.renderer.stats().draw_calls;
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let worst = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
    eprintln!(
        "BENCH frame {label}: cpu {:.2} ms (worst {:.2}), cpu+gpu {:.2} ms (worst {:.2}), {draws} draws",
        mean(&cpu),
        worst(&cpu),
        mean(&total),
        worst(&total)
    );
}

fn job(doc: &Document, gfx: &Gfx, fs: Option<hp_fs::GameFs>, around: Option<(DVec3, f64)>, quick: bool) -> bake::Job {
    let export = hp_editor::compile_export::CompileExport::default();
    let started = Instant::now();
    let before = now();
    let snap = match around {
        None => doc.compile_snapshot(&export),
        Some((center, radius)) => {
            let r = DVec3::splat(radius);
            doc.compile_snapshot_in(&export, &[hp_geom::Aabb::new(center - r, center + r)])
        }
    };
    eprintln!(
        "BENCH snapshot of the map on the editor thread: {:.1} ms, +{} MB",
        started.elapsed().as_secs_f64() * 1000.0,
        mb(now().saturating_sub(before))
    );
    let solids: HashMap<u32, bsp::SolidInfo> = doc
        .scene()
        .solids()
        .iter()
        .map(|(id, n)| {
            let info = bsp::SolidInfo {
                lo: n.bounds.min.as_vec3(),
                hi: n.bounds.max.as_vec3(),
                generation: gfx.renderer.lightmap_generation(*id),
            };
            (*id, info)
        })
        .collect();
    let options = doc.light_plus();
    let threads = std::env::var("HP_BENCH_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map_or_else(|| super::bake_threads(0), |n| (n.max(1), n.max(1)));
    let exe = std::env::var_os("HP_BENCH_EXE").map(PathBuf::from).filter(|p| p.is_file());
    bake::Job {
        id: if quick { 1 } else { 2 },
        serial: 1,
        map: (quick || std::env::var_os("HP_BENCH_NO_REUSE").is_some()).then_some(snap.map),
        keep: quick,
        solids,
        fs,
        resources_revision: 0,
        hdr: true,
        quality: hp_bspc::rad::RadQuality::Draft,
        bounces: super::bounces(options.bounced),
        prop_shadows: true,
        prop_lighting: true,
        threads,
        layer_size: super::LAYER_SIZE,
        max_layers: 64,
        coarsen: 1.0,
        lit: false,
        tuning: if quick { child::Tuning::of(&options).quick() } else { child::Tuning::of(&options) },
        exe,
    }
}

#[test]
#[ignore]
fn bake_and_baked_view() {
    let Some(path) = std::env::var_os("HP_BENCH_MAP").map(PathBuf::from) else {
        eprintln!("HP_BENCH_MAP names no map: skipped");
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("no map at {}: skipped", path.display());
        return;
    };
    hp_app::register_translations();
    hp_assets::register_translations();
    let _ = log::set_logger(&Log);
    log::set_max_level(log::LevelFilter::Debug);
    let sampler = Sampler::start();
    let mut doc = Document::from_map(hp_vmf::parse(&text).expect("parse"));
    drop(text);
    let Some(mut gfx) = gfx() else {
        eprintln!("no GPU: skipped");
        return;
    };
    let mut settings = match std::env::var_os("HP_BENCH_SETTINGS") {
        Some(p) => hp_app::settings::Settings::load_or_default(std::path::Path::new(&p)),
        None => hp_app::settings::Settings::default(),
    };
    if settings.game.gmod_root.is_none() || std::env::var_os("HP_BENCH_GMOD").is_some() {
        settings.game.gmod_root =
            Some(std::env::var_os("HP_BENCH_GMOD").map_or_else(|| PathBuf::from(GMOD), PathBuf::from));
    }
    let mut am = AssetManager::new(&settings);
    assert!(am.wait_until_mounted(Duration::from_secs(120)), "Garry's Mod mounted");
    let o = SceneOptions {
        show_models: true,
        show_helpers: true,
        show_tool_textures: true,
        lighting_preview: true,
        model_lods: false,
    };
    let (lo, hi) = doc
        .scene()
        .solids()
        .values()
        .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), n| (lo.min(n.bounds.min), hi.max(n.bounds.max)));
    let (lo, hi) = (lo.as_vec3(), hi.as_vec3());
    let (c, size) = ((lo + hi) * 0.5, hi - lo);
    let center = std::env::var("HP_BENCH_EYE")
        .ok()
        .and_then(|v| {
            let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (n.len() == 3).then(|| Vec3::new(n[0], n[1], n[2]))
        })
        .unwrap_or(c);
    let over = c + Vec3::new(0.0, -0.55 * size.y, 0.45 * size.z);
    let near = std::env::var("HP_BENCH_NEAR")
        .ok()
        .and_then(|v| {
            let n: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (n.len() == 3).then(|| Vec3::new(n[0], n[1], n[2]))
        })
        .unwrap_or(center + Vec3::new(-300.0, -300.0, 200.0));
    eprintln!("BENCH map {lo} to {hi}, bake center {center}");
    let t = Instant::now();
    settle(&mut am, &mut doc, &mut gfx, &o, over, c);
    eprintln!("BENCH loaded in {:.1} s, {} MB", t.elapsed().as_secs_f32(), mb(now()));
    frames(&mut gfx, "lit (no bake)", over, c, RenderMode::Lit);
    frames(&mut gfx, "textured", over, c, RenderMode::Textured);
    frames(&mut gfx, "lit near (no bake)", near, center, RenderMode::Lit);
    save(&gfx, "lit_near");

    let fs = am.fs().cloned();
    let radius: f64 = std::env::var("HP_BENCH_RADIUS").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let around = (radius > 0.0).then(|| (center.as_dvec3(), radius));
    let worker = bake::Worker::start(Box::new(|| {})).expect("worker");
    let quick =
        run_bake(&worker, &sampler, job(&doc, &gfx, fs.clone(), around, true), "quick", &mut gfx, (near, center));
    show(&mut gfx, &mut am, &quick);
    frames(&mut gfx, "quick near", near, center, RenderMode::Lit);
    save(&gfx, "quick_near");
    let baked = run_bake(&worker, &sampler, job(&doc, &gfx, fs, around, false), "refined", &mut gfx, (near, center));
    compare(&quick, &baked);
    drop(quick);
    sampler.reset();
    let t = Instant::now();
    show(&mut gfx, &mut am, &baked);
    drop(baked);
    let stats = gfx.renderer.lightmap_stats();
    eprintln!(
        "BENCH upload {:.2} s, peak {} MB, after {} MB; lightmaps on the GPU {} MB in {} layers, {} blocks",
        t.elapsed().as_secs_f32(),
        mb(sampler.peak()),
        mb(now()),
        mb(stats.bytes),
        stats.layers,
        stats.blocks
    );
    gfx.renderer.set_lightmap_display(LightmapDisplay::Baked, 1.0);
    gfx.renderer.set_lightmap_tracking(true);
    frames(&mut gfx, "baked overview", over, c, RenderMode::Lit);
    save(&gfx, "overview");
    frames(&mut gfx, "baked near", near, center, RenderMode::Lit);
    save(&gfx, "near");
    edits(&mut am, &mut doc, &mut gfx, &o);
}

fn edits(am: &mut AssetManager, doc: &mut Document, gfx: &mut Gfx, o: &SceneOptions) {
    let ctx = egui::Context::default();
    let Some(id) = doc.scene().solids().keys().copied().min() else { return };
    for lit in [true, false] {
        let o = SceneOptions { lighting_preview: lit, ..*o };
        am.begin_frame(&ctx, Some(gfx));
        am.sync_scene(doc, gfx, &o);
        let mut worst = Duration::ZERO;
        let mut total = Duration::ZERO;
        for k in 0..10 {
            let step = if k % 2 == 0 { 16.0 } else { -16.0 };
            doc.translate(&[hp_editor::ObjRef::Solid(id)], DVec3::new(0.0, 0.0, step)).expect("move");
            let t = Instant::now();
            am.begin_frame(&ctx, Some(gfx));
            am.sync_scene(doc, gfx, &o);
            let e = t.elapsed();
            worst = worst.max(e);
            total += e;
        }
        eprintln!(
            "BENCH edit sync with lighting_preview {lit}: mean {:.2} ms, worst {:.2} ms",
            total.as_secs_f64() * 100.0,
            worst.as_secs_f64() * 1000.0
        );
    }
}

fn save(gfx: &Gfx, name: &str) {
    let tag = std::env::var("HP_BENCH_TAG").unwrap_or_else(|_| "run".to_owned());
    let Ok((w, h, px)) = gfx.renderer.read_view_rgba8(40) else { return };
    let dir = std::env::var_os("HP_BENCH_OUT").map_or_else(std::env::temp_dir, PathBuf::from);
    let out = dir.join(format!("{tag}_{name}.png"));
    let _ = image::save_buffer(&out, &px, w, h, image::ColorType::Rgba8);
}

fn run_bake(
    worker: &bake::Worker,
    sampler: &Sampler,
    job: bake::Job,
    label: &str,
    gfx: &mut Gfx,
    view: (Vec3, Vec3),
) -> Box<bake::Baked> {
    let mut frame_ms: Vec<f64> = Vec::new();
    let mut idle: Vec<f64> = Vec::new();
    for i in 0..60 {
        let t = Instant::now();
        let mut p = persp(view.0, view.1, (i % 10) as f32 * 0.02);
        p.mode = RenderMode::Textured;
        gfx.renderer.render_view(40, &p, &Overlay::default());
        let _ = gfx.device.poll(wgpu::PollType::wait_indefinitely());
        idle.push(t.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(16).saturating_sub(t.elapsed()));
    }
    idle.sort_by(|a, b| a.total_cmp(b));
    eprintln!(
        "BENCH frames before the {label} bake: median {:.2} ms, worst {:.2} ms",
        idle[idle.len() / 2],
        idle[idle.len() - 1]
    );
    let base = now();
    sampler.reset();
    let t = Instant::now();
    worker.submit(job);
    let baked = loop {
        let mut done = None;
        for m in worker.poll() {
            match m {
                bake::Msg::Done { outcome: bake::Outcome::Baked(b), .. } => done = Some(b),
                bake::Msg::Done { outcome: bake::Outcome::Failed(e), .. } => panic!("bake failed: {e}"),
                bake::Msg::Done { .. } => panic!("cancelled"),
                bake::Msg::Progress { .. } => {}
            }
        }
        if let Some(b) = done {
            break b;
        }
        let t = Instant::now();
        let mut p = persp(view.0, view.1, (frame_ms.len() % 10) as f32 * 0.02);
        p.mode = RenderMode::Textured;
        gfx.renderer.render_view(40, &p, &Overlay::default());
        let _ = gfx.device.poll(wgpu::PollType::wait_indefinitely());
        frame_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(16).saturating_sub(t.elapsed()));
    };
    let peak = sampler.peak();
    frame_ms.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f64| frame_ms.get(((frame_ms.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
    eprintln!(
        "BENCH frames during the {label} bake: {} frames, median {:.2} ms, p95 {:.2} ms, worst {:.2} ms",
        frame_ms.len(),
        at(0.5),
        at(0.95),
        at(1.0)
    );
    eprintln!(
        "BENCH bake {label} {:.2} s: {} faces, {} luxels, {} layers of {}, {} texels; editor {} MB before, peak {} MB          (+{} MB), {} MB after; lighting process peak {:?} MB",
        t.elapsed().as_secs_f32(),
        baked.faces.len(),
        baked.luxels,
        baked.layers,
        baked.layer_size,
        baked.texels.len(),
        mb(base),
        mb(peak),
        mb(peak.saturating_sub(base)),
        mb(now()),
        baked.child_peak.map(|p| (mb(p.peak_working_set), mb(p.peak_committed)))
    );
    baked
}

fn show(gfx: &mut Gfx, am: &mut AssetManager, baked: &bake::Baked) {
    let shown = gfx.renderer.lightmap_stats();
    show_geometry(&mut gfx.renderer, am, baked, (shown.faces.max(baked.faces.len() as u32), shown.blocks));
    let mut written = 0;
    while written < baked.faces.len() {
        written = write_texels(&mut gfx.renderer, baked, written, super::MAX_TEXELS_PER_FRAME);
    }
    let _ = gfx.device.poll(wgpu::PollType::wait_indefinitely());
    gfx.renderer.set_lightmap_display(LightmapDisplay::Baked, 1.0);
}

fn compare(a: &bake::Baked, b: &bake::Baked) {
    let lum = |t: u32| {
        let c = hp_light::pack::unpack_rg11b10(t);
        0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
    };
    let mean = |v: &[u32]| v.iter().map(|t| f64::from(lum(*t))).sum::<f64>() / v.len().max(1) as f64;
    let (ma, mb_) = (mean(&a.texels), mean(&b.texels));
    let same = a.texels.len() == b.texels.len();
    let diff = if same {
        a.texels.iter().zip(&b.texels).map(|(x, y)| f64::from((lum(*x) - lum(*y)).abs())).sum::<f64>()
            / a.texels.len().max(1) as f64
    } else {
        f64::NAN
    };
    eprintln!(
        "BENCH quick vs refined: mean luminance {ma:.4} vs {mb_:.4} ({:+.2} %), mean absolute texel difference {:.2} % of the mean",
        (ma / mb_ - 1.0) * 100.0,
        diff / mb_ * 100.0
    );
}
