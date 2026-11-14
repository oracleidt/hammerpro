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
mod gpu_recovery;
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
    pub backend: Option<hp_app::settings::GraphicsBackend>,
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
            "--backend" => match args.next().as_deref().map(hp_app::settings::GraphicsBackend::parse) {
                Some(Some(b)) => cli.backend = Some(b),
                _ => eprintln!("--backend takes auto, vulkan or dx12"),
            },
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

fn forced_backend(choice: hp_app::settings::GraphicsBackend) -> Option<wgpu::Backend> {
    match choice {
        hp_app::settings::GraphicsBackend::Auto => None,
        hp_app::settings::GraphicsBackend::Vulkan => Some(wgpu::Backend::Vulkan),
        hp_app::settings::GraphicsBackend::Dx12 => Some(wgpu::Backend::Dx12),
    }
}

fn backend_has_adapter(backend: wgpu::Backend) -> bool {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::from(backend),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let future = instance.enumerate_adapters(wgpu::Backends::from(backend));
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(adapters) => !adapters.is_empty(),
        std::task::Poll::Pending => true,
    }
}

fn instance_backends(forced: Option<wgpu::Backend>) -> (wgpu::Backends, Option<wgpu::Backend>) {
    if let Some(env) = wgpu::Backends::from_env() {
        return (env, None);
    }
    match forced {
        Some(b) if backend_has_adapter(b) => (wgpu::Backends::from(b), Some(b)),
        Some(b) => {
            log::warn!(
                "graphics API {b:?} was asked for but no graphics adapter offers it, the automatic choice is used"
            );
            (wgpu::Backends::PRIMARY, None)
        }
        None => (wgpu::Backends::PRIMARY, None),
    }
}

fn dx12_shader_compiler(choice: hp_app::settings::Dx12Compiler) -> wgpu::Dx12Compiler {
    match choice {
        hp_app::settings::Dx12Compiler::Fxc => wgpu::Dx12Compiler::Fxc,
        hp_app::settings::Dx12Compiler::Dxc => wgpu::Dx12Compiler::default_dynamic_dxc(),
    }
}

fn wgpu_options(
    transparent: bool,
    forced: Option<wgpu::Backend>,
    compiler: hp_app::settings::Dx12Compiler,
) -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    if std::env::var_os("WGPU_DX12_COMPILER").is_none() {
        setup.instance_descriptor.backend_options.dx12.shader_compiler = dx12_shader_compiler(compiler);
    }
    let (backends, forced) = instance_backends(forced);
    setup.instance_descriptor.backends = backends;
    if transparent {
        setup.instance_descriptor.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
    }
    setup.native_adapter_selector =
        Some(Arc::new(move |adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
            pick_adapter(adapters, surface, transparent, forced)
        }));
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

fn slow_dx12(backend: wgpu::Backend) -> bool {
    backend == wgpu::Backend::Dx12
}

fn pick_adapter(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
    want_transparent: bool,
    forced: Option<wgpu::Backend>,
) -> Result<wgpu::Adapter, String> {
    let compiler = hp_render::pipeline_cache::dx12_compiler();
    let transparent = |a: &wgpu::Adapter| {
        want_transparent
            && surface.is_some_and(|s| {
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
        let other_api = forced.is_some_and(|f| f != info.backend);
        (other_api, kind, slow_dx12(info.backend), !transparent(a), api)
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
    let shader_compiler = match (info.backend, compiler.as_deref()) {
        (wgpu::Backend::Dx12, Some(c)) => format!(", shader compiler: {c}"),
        _ => String::new(),
    };
    log::info!(
        "graphics adapter: {} ({:?}, {:?}), transparent windows: {}{}{}",
        info.name,
        info.backend,
        info.device_type,
        if can { "yes" } else { "no (square window corners)" },
        shader_compiler,
        if forced.is_some() { ", chosen in the settings or on the command line" } else { "" }
    );
    if slow_dx12(info.backend) && compiler.as_deref() == Some("fxc") {
        log::info!(
            "graphics: DirectX 12 compiles shaders with FXC, the first start on this graphics card is slow (later starts reuse the shader cache)"
        );
    }
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

    let saved = (!cli.is_automated())
        .then(|| hp_app::settings::Settings::load_or_default(&hp_app::settings::Settings::default_path()));
    let native_frames = saved.as_ref().is_some_and(|s| s.ui.native_frames);
    let backend = cli.backend.or(saved.as_ref().map(|s| s.render.graphics_backend)).unwrap_or_default();
    let dx12_compiler = saved.as_ref().map(|s| s.render.dx12_compiler).unwrap_or_default();
    hp_render::pipeline_cache::set_dx12_shader_cache(Some(
        hp_app::settings::Settings::cache_dir().join(hp_render::pipeline_cache::DX12_SHADER_DIR),
    ));
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
        wgpu_options: wgpu_options(true, forced_backend(backend), dx12_compiler),
        system_font_fallback: false,
        ..Default::default()
    };

    eframe::run_native("HammerPro", options, Box::new(move |cc| Ok(Box::new(app::HammerProApp::new(cc, cli, log_rx)))))
}
