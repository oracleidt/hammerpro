mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use common::{real_gmod, real_settings};
use glam::DVec3;
use hp_app::AssetsApi;
use hp_app::state::{Gfx, SceneOptions};
use hp_assets::AssetManager;
use hp_editor::Document;
use hp_render::{Overlay, RenderMode, ViewKind, ViewParams};

static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);

struct Counting;

unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let n = LIVE.fetch_add(layout.size() as u64, Ordering::Relaxed) + layout.size() as u64;
        PEAK.fetch_max(n, Ordering::Relaxed);
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        let n = LIVE.fetch_add(layout.size() as u64, Ordering::Relaxed) + layout.size() as u64;
        PEAK.fetch_max(n, Ordering::Relaxed);
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        if new_size >= layout.size() {
            let n = LIVE.fetch_add((new_size - layout.size()) as u64, Ordering::Relaxed)
                + (new_size - layout.size()) as u64;
            PEAK.fetch_max(n, Ordering::Relaxed);
        } else {
            LIVE.fetch_sub((layout.size() - new_size) as u64, Ordering::Relaxed);
        }
        unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[repr(C)]
#[derive(Default)]
struct Counters {
    cb: u32,
    page_faults: u32,
    peak_working_set: usize,
    working_set: usize,
    quota_peak_paged: usize,
    quota_paged: usize,
    quota_peak_nonpaged: usize,
    quota_nonpaged: usize,
    pagefile: usize,
    peak_pagefile: usize,
    private: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> isize;
    fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
}

fn mem() -> (u64, u64) {
    let mut c = Counters { cb: size_of::<Counters>() as u32, ..Default::default() };
    unsafe {
        K32GetProcessMemoryInfo(GetCurrentProcess(), &raw mut c, c.cb);
    }
    (c.working_set as u64, c.private as u64)
}

struct Sampler {
    peak_ws: Arc<AtomicU64>,
    peak_priv: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl Sampler {
    fn start() -> Self {
        let peak_ws = Arc::new(AtomicU64::new(0));
        let peak_priv = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (a, b, s) = (peak_ws.clone(), peak_priv.clone(), stop.clone());
        std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                let (ws, pv) = mem();
                a.fetch_max(ws, Ordering::Relaxed);
                b.fetch_max(pv, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        Self { peak_ws, peak_priv, stop }
    }

    fn take(&self) -> (u64, u64) {
        let (ws, pv) = mem();
        (self.peak_ws.swap(ws, Ordering::Relaxed).max(ws), self.peak_priv.swap(pv, Ordering::Relaxed).max(pv))
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn mb(b: u64) -> f64 {
    b as f64 / 1048576.0
}

fn phase(s: &Sampler, name: &str, t: Instant) {
    let (pws, ppv) = s.take();
    let (ws, pv) = mem();
    let live = LIVE.load(Ordering::Relaxed);
    let peak = PEAK.swap(live, Ordering::Relaxed);
    eprintln!(
        "PHASE {name:<14} {:>6.0} ms  ws {:>5.0} priv {:>5.0} heap {:>5.0} MB  peak ws {:>5.0} priv {:>5.0} heap {:>5.0} MB",
        t.elapsed().as_secs_f64() * 1000.0,
        mb(ws),
        mb(pv),
        mb(live),
        mb(pws),
        mb(ppv),
        mb(peak)
    );
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
    let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("hp-assets load profile"),
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

fn view(eye: DVec3, target: DVec3) -> ViewParams {
    let forward = (target - eye).normalize_or(DVec3::X);
    ViewParams {
        game_pixel_grid: false,
        size: [1280, 800],
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

#[test]
#[ignore]
fn load_profile_big_map() {
    let path = PathBuf::from(
        std::env::var("HP_PROFILE_MAP").unwrap_or_else(|_| "E:/hp-scratch/light-fix/map/hokai_d.vmf".to_owned()),
    );
    let Some(root) = real_gmod() else { return };
    if !path.is_file() {
        return;
    }
    hp_app::register_translations();
    hp_assets::register_translations();
    let s = Sampler::start();
    phase(&s, "process", Instant::now());
    let t = Instant::now();
    let Some(mut gfx) = gfx() else { return };
    phase(&s, "gfx", t);
    let settings = match std::env::var("HP_PROFILE_SETTINGS") {
        Ok(p) => hp_app::settings::Settings::load_or_default(std::path::Path::new(&p)),
        Err(_) => real_settings(&root),
    };
    let mut am = AssetManager::new(&settings);
    assert!(am.wait_until_mounted(Duration::from_secs(120)));
    let ctx = egui::Context::default();
    am.begin_frame(&ctx, Some(&mut gfx));
    phase(&s, "mounted", Instant::now());
    let t = Instant::now();
    let warm = view(DVec3::new(-100.0, -100.0, 100.0), DVec3::ZERO);
    gfx.renderer.render_view(1, &warm, &Overlay::default());
    gfx.renderer.render_view(1, &ViewParams { mode: RenderMode::Lit, ..warm }, &Overlay::default());
    phase(&s, "warm render", t);

    let t0 = Instant::now();
    let t = Instant::now();
    let mut doc = if std::env::var("HP_PROFILE_REMAP").is_ok() {
        let mut map = hp_vmf::load(&path).expect("load");
        let mut stock: Vec<String> = am
            .models()
            .iter()
            .map(|e| e.name.to_ascii_lowercase())
            .filter(|n| {
                n.ends_with(".mdl") && (n.starts_with("models/props") != std::env::var("HP_PROFILE_OTHER").is_ok())
            })
            .collect();
        stock.sort();
        stock.dedup();
        let offset: usize = std::env::var("HP_PROFILE_REMAP").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let mut assigned: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for e in &mut map.entities {
            let Some(m) = e.get("model").map(str::to_ascii_lowercase) else { continue };
            if !m.ends_with(".mdl") || stock.is_empty() {
                continue;
            }
            let next = assigned.len();
            let to =
                assigned.entry(m).or_insert_with(|| stock[(next * 7919 + offset * 1601) % stock.len()].clone()).clone();
            e.set("model", to);
        }
        eprintln!("remapped {} models onto {} stock models", assigned.len(), stock.len());
        Document::from_map(map)
    } else {
        Document::open(&path).expect("open")
    };
    let tm = hp_vmf::last_load_timings();
    eprintln!("vmf timings {tm:?}");
    phase(&s, "document", t);
    {
        let sc = doc.scene();
        let (mut sides, mut mat, mut disp, mut dispv, mut geomv, mut meshv, mut faces, mut extra) =
            (0, 0, 0, 0, 0, 0, 0, 0);
        for n in sc.solids().values() {
            sides += n.solid.sides.len();
            for sd in &n.solid.sides {
                mat += sd.material.capacity();
                extra += sd.extra_props.len() + sd.extra_blocks.len();
                if let Some(d) = &sd.disp {
                    disp += 1;
                    dispv += d.normals.len() * 24 * 3 + d.distances.len() * 8 * 2 + d.triangle_tags.len() * 4;
                }
            }
            faces += n.geom.faces.len();
            geomv += n.geom.faces.iter().map(|f| f.vertices.capacity() * 24).sum::<usize>();
            for m in n.disp.iter().flatten() {
                meshv += m.positions.capacity() * 24 * 3 + m.alphas.capacity() * 8 + m.indices.capacity() * 4;
            }
        }
        let (mut props, mut pbytes) = (0, 0);
        for e in sc.entities().values() {
            props += e.entity.props.len();
            pbytes += e.entity.props.iter().map(|(k, v)| k.capacity() + v.capacity() + 48).sum::<usize>();
        }
        eprintln!(
            "DOC sides {sides} side_struct {} MB mat {} MB disp {disp} dispinfo {} MB geom faces {faces} verts {} MB dispmesh {} MB props {props} {} MB extra {extra}",
            (sides * size_of::<hp_vmf::Side>()) >> 20,
            mat >> 20,
            dispv >> 20,
            geomv >> 20,
            meshv >> 20,
            pbytes >> 20
        );
    }
    let t = Instant::now();
    let opts = hp_editor::PickOptions { include_entities: false, max_distance: 1.0, ..Default::default() };
    let ray = hp_geom::Ray::new(DVec3::new(1.0e9, 1.0e9, 1.0e9), DVec3::Z);
    let _ = doc.pick_ray(&ray, &opts, &|_| None);
    phase(&s, "pick warm", t);

    let o = SceneOptions {
        show_models: true,
        show_helpers: true,
        show_tool_textures: true,
        lighting_preview: false,
        model_lods: false,
    };
    let mut b = hp_geom::Aabb::EMPTY;
    for n in doc.scene().solids().values() {
        if !n.bounds.is_empty() {
            b = b.union(&n.bounds);
        }
    }
    let c = b.center();
    let eye = b.min + b.size() * DVec3::new(0.05, 0.05, 0.8);
    let v = view(eye, DVec3::new(c.x, c.y, b.min.z + b.size().z * 0.15));

    let t = Instant::now();
    am.begin_frame(&ctx, Some(&mut gfx));
    am.sync_scene(&mut doc, &mut gfx, &o);
    phase(&s, "first sync", t);
    let t = Instant::now();
    gfx.renderer.flush();
    phase(&s, "first flush", t);
    let t = Instant::now();
    gfx.renderer.render_view(1, &v, &Overlay::default());
    phase(&s, "first render", t);
    let t = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut frames = 0u32;
    let mut geometry_done = None;
    let mut longest = Duration::ZERO;
    let mut last_print = Instant::now();
    loop {
        let ft = Instant::now();
        am.begin_frame(&ctx, Some(&mut gfx));
        am.sync_scene(&mut doc, &mut gfx, &o);
        gfx.renderer.render_view(1, &v, &Overlay::default());
        frames += 1;
        longest = longest.max(ft.elapsed());
        if geometry_done.is_none() && am.solids_to_upload() == 0 {
            geometry_done = Some(t.elapsed());
            phase(&s, "geometry", t);
        }
        let st = am.stats();
        if last_print.elapsed() > Duration::from_secs(3) {
            last_print = Instant::now();
            let (ws, pv) = mem();
            eprintln!(
                "  t={:.1}s frames {frames} queued {} uploads {} tex_loading {} models_loading {} ws {:.0} priv {:.0} heap {:.0}",
                t.elapsed().as_secs_f64(),
                st.queued_jobs,
                st.pending_uploads,
                st.textures_loading,
                st.models_loading,
                mb(ws),
                mb(pv),
                mb(LIVE.load(Ordering::Relaxed))
            );
        }
        if st.queued_jobs == 0
            && st.pending_uploads == 0
            && st.textures_loading == 0
            && st.models_loading == 0
            && st.materials_loading == 0
            && am.solids_to_upload() == 0
        {
            break;
        }
        assert!(Instant::now() < deadline, "not settled {st:?}");
    }
    gfx.renderer.flush();
    phase(&s, "settled", t);
    eprintln!("longest frame {longest:?}");
    eprintln!("frames {frames} total open to settled {:?}", t0.elapsed());
    eprintln!("asset stats {:?}", am.stats());
    let rs = gfx.renderer.stats();
    eprintln!("render stats {rs:?}");
    eprintln!("approx gpu {:.0} MB", mb(rs.approx_gpu_bytes));
    for _ in 0..30 {
        am.begin_frame(&ctx, Some(&mut gfx));
        am.sync_scene(&mut doc, &mut gfx, &o);
        gfx.renderer.render_view(1, &v, &Overlay::default());
    }
    std::thread::sleep(Duration::from_millis(500));
    phase(&s, "steady", Instant::now());
    let (w, h, px) = gfx.renderer.read_view_rgba8(1).expect("readback");
    let out = PathBuf::from(
        std::env::var("HP_PROFILE_PNG").unwrap_or_else(|_| "E:/hp-scratch/agent-perf-load/view.png".to_owned()),
    );
    let _ = std::fs::create_dir_all(out.parent().unwrap());
    image::save_buffer(&out, &px, w, h, image::ColorType::Rgba8).expect("png");
}
