#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]
#![recursion_limit = "256"]

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! println {
    () => {{
        let _ = ::std::io::Write::write_all(&mut ::std::io::stdout(), b"\n");
    }};
    ($($arg:tt)*) => {{
        let _ = ::std::io::Write::write_fmt(&mut ::std::io::stdout(), format_args!("{}\n", format_args!($($arg)*)));
    }};
}

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = ::std::io::Write::write_fmt(&mut ::std::io::stdout(), format_args!($($arg)*));
    }};
}

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! eprintln {
    () => {{
        let _ = ::std::io::Write::write_all(&mut ::std::io::stderr(), b"\n");
    }};
    ($($arg:tt)*) => {{
        let _ = ::std::io::Write::write_fmt(&mut ::std::io::stderr(), format_args!("{}\n", format_args!($($arg)*)));
    }};
}

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! eprint {
    ($($arg:tt)*) => {{
        let _ = ::std::io::Write::write_fmt(&mut ::std::io::stderr(), format_args!($($arg)*));
    }};
}

mod app;
mod autosave;
mod batch;
mod blender;
mod bspc_cli;
mod bspc_game;
mod bspc_instances;
mod collaboration;
mod crash;
mod discord;
mod dispatch;
mod export;
mod files;
mod gpu_guard;
mod icon;
mod keyboard;
mod livelink;
mod logger;
mod native_crash;
mod opening;
mod panel_layout;
mod particle_exit;
mod perf;
mod primitive_memory;
mod primitive_stats;
mod prompts;
mod restart;
mod script;
mod tabs;
#[cfg(test)]
mod tests_undo_move;
mod timing;
mod update;
mod vram;

use std::path::PathBuf;
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: native_crash::GuardedAlloc = native_crash::GuardedAlloc;

#[derive(Clone, Debug, Default)]
pub struct Cli {
    pub open: Option<PathBuf>,
    pub lang: Option<hp_app::i18n::Lang>,
    pub screenshot: Option<PathBuf>,
    pub script: Option<PathBuf>,
    pub frames: u64,
    pub maximize_view: Option<usize>,
    pub tool: Option<hp_app::ToolId>,
    pub width: f32,
    pub height: f32,
    pub bench_open: Option<PathBuf>,
    pub restore: Option<PathBuf>,
    pub no_save_settings: bool,
}

impl Cli {
    pub fn is_automated(&self) -> bool {
        self.script.is_some() || self.screenshot.is_some() || self.bench_open.is_some()
    }
}

fn parse_cli() -> Cli {
    let mut cli = Cli { frames: 90, width: 1600.0, height: 960.0, ..Cli::default() };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lang" => {
                cli.lang = match args.next().as_deref() {
                    Some("fr") => Some(hp_app::i18n::Lang::Fr),
                    Some("en") => Some(hp_app::i18n::Lang::En),
                    Some("ru") => Some(hp_app::i18n::Lang::Ru),
                    _ => None,
                }
            }
            "--screenshot" => cli.screenshot = args.next().map(PathBuf::from),
            "--script" => cli.script = args.next().map(PathBuf::from),
            "--bench-open" => cli.bench_open = args.next().map(PathBuf::from),
            "--no-save-settings" => cli.no_save_settings = true,
            restart::RESTORE_ARG => cli.restore = args.next().map(PathBuf::from),
            "--frames" => cli.frames = args.next().and_then(|v| v.parse().ok()).unwrap_or(90),
            "--maximize-view" => cli.maximize_view = args.next().and_then(|v| v.parse().ok()),
            "--size" => {
                if let Some((w, h)) = args.next().as_deref().and_then(|s| s.split_once('x')) {
                    cli.width = w.parse().unwrap_or(cli.width);
                    cli.height = h.parse().unwrap_or(cli.height);
                }
            }
            "--tool" => {
                cli.tool = args.next().and_then(|name| {
                    hp_app::ToolId::ALL.into_iter().find(|t| format!("{t:?}").eq_ignore_ascii_case(&name))
                })
            }
            other if !other.starts_with("--") => cli.open = Some(PathBuf::from(other)),
            other => eprintln!("unknown argument: {other}"),
        }
    }
    cli
}

fn wgpu_options(transparent: bool) -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY);
    if transparent {
        setup.instance_descriptor.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
        setup.native_adapter_selector = Some(Arc::new(pick_adapter));
    }
    setup.device_descriptor = Arc::new(|adapter: &wgpu::Adapter| wgpu::DeviceDescriptor {
        label: Some("hammerpro device"),
        required_features: (adapter.features()
            & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::POLYGON_MODE_LINE))
            | hp_render::pipeline_cache::wanted_features(adapter)
            | hp_render::profile::wanted_features(adapter),
        required_limits: adapter.limits(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        ..Default::default()
    });
    egui_wgpu::WgpuConfiguration { wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup), ..Default::default() }
}

fn pick_adapter(adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>) -> Result<wgpu::Adapter, String> {
    let transparent = |a: &wgpu::Adapter| {
        surface.is_some_and(|s| {
            s.get_capabilities(a).alpha_modes.iter().any(|m| {
                matches!(m, wgpu::CompositeAlphaMode::PreMultiplied | wgpu::CompositeAlphaMode::PostMultiplied)
            })
        })
    };
    let rank = |a: &wgpu::Adapter| {
        let info = a.get_info();
        let kind = match info.device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            wgpu::DeviceType::VirtualGpu => 2,
            wgpu::DeviceType::Other => 3,
            wgpu::DeviceType::Cpu => 4,
        };
        let api = match info.backend {
            wgpu::Backend::Vulkan | wgpu::Backend::Metal => 0,
            wgpu::Backend::Dx12 => 1,
            wgpu::Backend::Gl => 2,
            _ => 3,
        };
        (!transparent(a), kind, api)
    };
    let wanted = std::env::var("HP_ADAPTER").ok().map(|v| v.to_ascii_lowercase()).filter(|v| !v.is_empty());
    let named = |a: &wgpu::Adapter| {
        wanted.as_deref().is_some_and(|w| {
            let info = a.get_info();
            info.name.to_ascii_lowercase().contains(w)
                || (w == "integrated" && info.device_type == wgpu::DeviceType::IntegratedGpu)
                || (w == "cpu" && info.device_type == wgpu::DeviceType::Cpu)
        })
    };
    let best = adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .min_by_key(|a| (!named(a), rank(a)))
        .ok_or_else(|| "no graphics adapter can draw to the window".to_owned())?;
    let can = transparent(best);
    hp_app::chrome::set_transparency_available(can);
    let info = best.get_info();
    log::info!(
        "graphics adapter: {} ({:?}, {:?}), transparent windows: {}",
        info.name,
        info.backend,
        info.device_type,
        if can { "yes" } else { "no (square window corners)" }
    );
    Ok(best.clone())
}

fn main() -> eframe::Result<()> {
    if std::env::args().nth(1).is_some_and(|arg| arg == bspc_cli::BSPC_ARG || arg == bspc_cli::RAD_ARG) {
        let settings = hp_app::settings::Settings::load_or_default(&hp_app::settings::Settings::default_path());
        hp_fs::configure_scan(settings.scan.to_config());
    }
    if std::env::args().nth(1).as_deref() == Some(hp_mcp::BRIDGE_ARG) {
        let options = hp_mcp::bridge::BridgeOptions {
            tools: dispatch::mcp::tool_specs(),
            ..hp_mcp::bridge::BridgeOptions::from_env()
        };
        std::process::exit(hp_mcp::bridge::run_stdio_with(options));
    }
    if std::env::args().nth(1).as_deref() == Some(bspc_cli::BSPC_ARG) {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(bspc_cli::run(&args));
    }
    if std::env::args().nth(1).as_deref() == Some(bspc_cli::RAD_ARG) {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(bspc_cli::run_rad(&args));
    }
    if std::env::args().nth(1).as_deref() == Some(batch::IMPORT_ARG) {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(batch::run_cli(&args));
    }
    if std::env::args().nth(1).as_deref() == Some(dispatch::lighting::child::LIGHT_ARG) {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(dispatch::lighting::child::run(&args));
    }
    if std::env::args().nth(1).as_deref() == Some(hp_update::APPLY_ARG) {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(update::run_helper(&args));
    }
    timing::init_process_start();
    let cli = parse_cli();
    let log_rx = logger::init(!cli.is_automated());
    if !cli.is_automated() {
        crash::install_hook();
    }

    let native_frames = !cli.is_automated()
        && hp_app::settings::Settings::load_or_default(&hp_app::settings::Settings::default_path()).ui.native_frames;
    let margin = if native_frames { 0.0 } else { 2.0 * hp_app::chrome::MARGIN };
    let viewport = egui::ViewportBuilder::default()
        .with_title("Hammer Pro")
        .with_inner_size([cli.width + margin, cli.height + margin])
        .with_min_inner_size([960.0 + margin, 600.0 + margin])
        .with_icon(icon::app_icon())
        .with_drag_and_drop(true)
        .with_decorations(native_frames)
        .with_fullscreen(false)
        .with_transparent(true);

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: wgpu_options(true),
        system_font_fallback: false,
        ..Default::default()
    };

    eframe::run_native("HammerPro", options, Box::new(move |cc| Ok(Box::new(app::HammerProApp::new(cc, cli, log_rx)))))
}
