use std::sync::atomic::{AtomicU8, Ordering};

use hp_app::AppState;
use hp_app::runtime::adapter::AdapterChoice;

use crate::app::HammerProApp;

static PRESENT_MODE: AtomicU8 = AtomicU8::new(0);

pub(crate) fn note_present_mode(mode: wgpu::PresentMode) {
    let code = match mode {
        wgpu::PresentMode::AutoVsync => 1,
        wgpu::PresentMode::AutoNoVsync => 2,
        wgpu::PresentMode::Fifo => 3,
        wgpu::PresentMode::FifoRelaxed => 4,
        wgpu::PresentMode::Immediate => 5,
        wgpu::PresentMode::Mailbox => 6,
    };
    PRESENT_MODE.store(code, Ordering::Relaxed);
}

pub(crate) fn present_mode_name() -> &'static str {
    match PRESENT_MODE.load(Ordering::Relaxed) {
        1 => "AutoVsync (Fifo, waits for the screen)",
        2 => "AutoNoVsync (Mailbox or Immediate)",
        3 => "Fifo",
        4 => "FifoRelaxed",
        5 => "Immediate",
        6 => "Mailbox",
        _ => "not set yet",
    }
}

pub(crate) struct ViewLine {
    pub index: usize,
    pub kind: &'static str,
    pub size: Option<(f32, f32)>,
    pub mode: String,
    pub maximized: bool,
}

pub(crate) struct ReportData {
    pub version: String,
    pub adapter: Option<AdapterChoice>,
    pub adapter_name: String,
    pub graphics_api: String,
    pub present_mode: &'static str,
    pub frame_cap: String,
    pub window: (f32, f32, f32),
    pub views: Vec<ViewLine>,
    pub map: String,
    pub fps: f32,
    pub frame_ms: f32,
    pub phases: Vec<(&'static str, f32)>,
    pub frame_total_ms: f32,
    pub render_cpu: [f32; 3],
    pub gpu: Option<[f32; 5]>,
    pub vram: (u64, u64, u64),
    pub working_set: u64,
    pub quality: String,
}

fn mb(bytes: u64) -> u64 {
    bytes >> 20
}

pub(crate) fn format_report(d: &ReportData) -> String {
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };
    line("Hammer Pro performance report".to_owned());
    line(format!("version: {}", d.version));
    match &d.adapter {
        Some(c) => {
            let a = &c.chosen;
            let driver = if a.driver.is_empty() { "unknown".to_owned() } else { a.driver.clone() };
            line(format!("graphics card: {} ({}, {}, driver {driver})", a.name, a.kind.id(), a.backend));
            let others: Vec<String> = c
                .all
                .iter()
                .filter(|o| !(o.name == a.name && o.backend == a.backend))
                .map(|o| format!("{} ({}, {})", o.name, o.kind.id(), o.backend))
                .collect();
            if !others.is_empty() {
                line(format!("other adapters: {}", others.join("; ")));
            }
            if let Some(n) = &c.notice {
                line(format!("adapter notice: {}", n.id()));
            }
            if let Some(w) = &c.wanted {
                line(format!("adapter asked for: {w}{}", if c.wanted_found { "" } else { " (not found)" }));
            }
            if let Some(f) = &c.fallback_from {
                line(format!("graphics fallback: {f} could not start"));
            }
        }
        None => line(format!("graphics card: {} ({})", d.adapter_name, d.graphics_api)),
    }
    line(format!("present mode: {}", d.present_mode));
    line(format!("frame cap: {}", d.frame_cap));
    line(format!("window: {:.0} x {:.0} px (scale {:.2})", d.window.0, d.window.1, d.window.2));
    for v in &d.views {
        let size = v.size.map_or("hidden".to_owned(), |(w, h)| format!("{w:.0} x {h:.0}"));
        line(format!(
            "view {}: {} {size}, {}{}",
            v.index + 1,
            v.kind,
            v.mode,
            if v.maximized { ", maximized" } else { "" }
        ));
    }
    line(format!("map: {}", d.map));
    line(format!("fps: {:.1}, 3D frame {:.2} ms", d.fps, d.frame_ms));
    let phases: Vec<String> =
        d.phases.iter().filter(|(_, ms)| *ms >= 0.05).map(|(n, ms)| format!("{n} {ms:.2}")).collect();
    line(format!("last frame CPU: {:.2} ms ({})", d.frame_total_ms, phases.join(", ")));
    let [prepare, record, submit] = d.render_cpu;
    line(format!("3D render CPU: prepare {prepare:.2}, record {record:.2}, submit {submit:.2} ms"));
    match d.gpu {
        Some([total, shadows, scene, post, reflection]) => line(format!(
            "GPU: total {total:.2} ms (shadows {shadows:.2}, scene {scene:.2}, post {post:.2}, water reflection {reflection:.2})"
        )),
        None => line("GPU: no timestamp queries on this graphics card".to_owned()),
    }
    let (usage, budget, card) = d.vram;
    line(format!("graphics memory: {} MB used, budget {} MB, card {} MB", mb(usage), mb(budget), mb(card)));
    line(format!("RAM: {} MB working set", mb(d.working_set)));
    line(format!("quality: {}", d.quality));
    out
}

fn frame_cap_text(app: &HammerProApp) -> String {
    let r = &app.state.settings.render;
    let cap = app.pkg.render.frame_cap();
    let refresh = app.pkg.render.refresh_hz().map_or("unknown".to_owned(), |hz| format!("{hz} Hz"));
    let vsync = if r.vsync { "vertical sync on" } else { "vertical sync off" };
    let limit = if cap > 0 {
        format!("{cap} FPS")
    } else if r.vsync {
        "screen refresh".to_owned()
    } else {
        "none".to_owned()
    };
    format!(
        "{limit} ({vsync}, screen {refresh}, max_fps {}, background {}, cap at refresh {})",
        r.max_fps,
        r.unfocused_fps,
        if r.frame_cap_refresh { "on" } else { "off" }
    )
}

fn map_text(state: &AppState) -> String {
    let s = state.doc.stats();
    let props = state.doc.scene().entities().values().filter(|n| hp_app::actions::is_model_prop(&n.entity)).count();
    let name = state
        .doc
        .path()
        .and_then(|p| p.file_name())
        .map_or("untitled".to_owned(), |n| n.to_string_lossy().into_owned());
    format!(
        "{name}: brushes {}, faces {}, entities {}, props {props}, displacements {}, materials {}",
        s.solids, s.faces, s.entities, s.displacements, s.unique_materials
    )
}

fn view_lines(app: &HammerProApp) -> Vec<ViewLine> {
    let state = &app.state;
    let rects = app.viewports.view_rects();
    let logical = app.viewports.logical_rects();
    let r = &state.settings.render;
    (0..4)
        .map(|i| {
            let view = hp_ui_viewports::layout::quadrant_view(&state.settings.ui.quadrants, i);
            let rect = match view {
                hp_app::QuadrantView::Logical => logical[i],
                hp_app::QuadrantView::View(_) => rects[i].map(|(_, r)| r),
            };
            let kind = crate::dispatch::mcp::automation::quadrant_view_name(view);
            let mode = match view.kind() {
                Some(hp_app::ViewKind::Perspective) => format!(
                    "{:?}, render scale {:.0}%, msaa {}",
                    state.settings.view.render_mode,
                    r.scale() * 100.0,
                    if state.settings.view.msaa { "on" } else { "off" }
                ),
                Some(_) => format!("2D wireframe, displacements {:?}", r.displacements_2d_of(i)),
                None => "logical view".to_owned(),
            };
            ViewLine {
                index: i,
                kind,
                size: rect.map(|r| (r.width(), r.height())),
                mode,
                maximized: state.ui.maximized_view == Some(i),
            }
        })
        .collect()
}

pub(crate) fn report(app: &HammerProApp, ctx: &egui::Context) -> String {
    let state = &app.state;
    let p = &state.runtime.render.perf;
    let s = &p.sample;
    let ppp = ctx.pixels_per_point();
    let screen = ctx.content_rect();
    let commit = option_env!("HP_BUILD_COMMIT").map(|c| format!(", {c}")).unwrap_or_default();
    let g = &state.runtime.gpu;
    let mode = state.settings.render.performance_mode;
    let data = ReportData {
        version: format!("{} ({}{commit})", env!("CARGO_PKG_VERSION"), env!("HP_BUILD_DATE")),
        adapter: hp_app::runtime::adapter::choice(),
        adapter_name: p.adapter.clone(),
        graphics_api: p.graphics_api.clone(),
        present_mode: present_mode_name(),
        frame_cap: frame_cap_text(app),
        window: (screen.width() * ppp, screen.height() * ppp, ppp),
        views: view_lines(app),
        map: map_text(state),
        fps: state.frame.fps,
        frame_ms: s.frame_ms,
        phases: hp_app::runtime::FRAME_PHASES.iter().copied().zip(p.frame_phases.ms).collect(),
        frame_total_ms: p.frame_phases.total_ms,
        render_cpu: [s.prepare_ms, s.record_ms, s.submit_ms],
        gpu: s.gpu_supported.then_some([
            s.gpu_total_ms + s.gpu_reflection_ms,
            s.gpu_shadows_ms,
            s.gpu_scene_ms,
            s.gpu_post_ms,
            s.gpu_reflection_ms,
        ]),
        vram: (g.usage, g.budget, g.card),
        working_set: s.working_set,
        quality: format!(
            "performance mode {}, level {}, graphics memory {}",
            mode.id(),
            p.level(mode).id(),
            state.settings.render.graphics_memory.id()
        ),
    };
    format_report(&data)
}

#[cfg(test)]
mod tests {
    use hp_app::runtime::adapter::{AdapterEntry, AdapterKind, AdapterNotice};

    use super::*;

    #[test]
    fn report_is_short_and_has_every_section() {
        let chosen = AdapterEntry {
            name: "Intel(R) UHD Graphics".to_owned(),
            kind: AdapterKind::Integrated,
            backend: "Dx12".to_owned(),
            driver: "31.0.101".to_owned(),
        };
        let nvidia = AdapterEntry {
            name: "NVIDIA GeForce RTX 3060".to_owned(),
            kind: AdapterKind::Dedicated,
            backend: "Vulkan".to_owned(),
            driver: String::new(),
        };
        let data = ReportData {
            version: "2.3.3 (2026-11-23)".to_owned(),
            adapter: Some(AdapterChoice {
                chosen: chosen.clone(),
                notice: Some(AdapterNotice::Integrated { dedicated: nvidia.name.clone() }),
                all: vec![chosen, nvidia],
                ..Default::default()
            }),
            adapter_name: String::new(),
            graphics_api: String::new(),
            present_mode: "AutoVsync",
            frame_cap: "screen refresh".to_owned(),
            window: (1600.0, 900.0, 1.0),
            views: vec![ViewLine {
                index: 0,
                kind: "3d",
                size: Some((800.0, 450.0)),
                mode: "Textured".to_owned(),
                maximized: false,
            }],
            map: "gm_test.vmf: brushes 10".to_owned(),
            fps: 60.0,
            frame_ms: 4.0,
            phases: vec![("panels", 1.5), ("views", 0.01)],
            frame_total_ms: 2.0,
            render_cpu: [0.1, 0.2, 0.3],
            gpu: None,
            vram: (512 << 20, 4096 << 20, 6144 << 20),
            working_set: 900 << 20,
            quality: "full".to_owned(),
        };
        let text = format_report(&data);
        for needle in [
            "version: 2.3.3",
            "graphics card: Intel(R) UHD Graphics (integrated, Dx12, driver 31.0.101)",
            "other adapters: NVIDIA GeForce RTX 3060 (dedicated, Vulkan)",
            "adapter notice: integrated_while_dedicated_exists",
            "present mode: AutoVsync",
            "frame cap: screen refresh",
            "window: 1600 x 900",
            "view 1: 3d 800 x 450, Textured",
            "map: gm_test.vmf",
            "last frame CPU: 2.00 ms (panels 1.50)",
            "GPU: no timestamp queries",
            "graphics memory: 512 MB used, budget 4096 MB, card 6144 MB",
        ] {
            assert!(text.contains(needle), "{needle} missing in:\n{text}");
        }
        assert!(text.lines().count() <= 24, "{text}");
        assert!(!text.contains("views 0.01"));
    }

    #[test]
    fn present_mode_is_named() {
        note_present_mode(wgpu::PresentMode::AutoVsync);
        assert!(present_mode_name().starts_with("AutoVsync"));
    }
}
