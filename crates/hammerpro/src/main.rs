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
mod bspc_optimize;
mod bspc_parallax;
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
mod perf_report;
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

#[cfg(windows)]
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
#[used]
pub static NvOptimusEnablement: u32 = 1;

#[cfg(windows)]
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
#[used]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

const GRAPHICS_FALLBACK_ARG: &str = "--graphics-fallback";

static WINDOW_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
    pub graphics_fallback: Option<String>,
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
            GRAPHICS_FALLBACK_ARG => cli.graphics_fallback = args.next().filter(|v| !v.trim().is_empty()),
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

fn backend_adapters(backend: wgpu::Backend) -> Option<Vec<wgpu::DeviceType>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::from(backend),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let future = instance.enumerate_adapters(wgpu::Backends::from(backend));
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(adapters) => Some(adapters.iter().map(|a| a.get_info().device_type).collect()),
        std::task::Poll::Pending => None,
    }
}

fn backend_has_adapter(backend: wgpu::Backend) -> bool {
    backend_adapters(backend).is_none_or(|a| !a.is_empty())
}

fn vulkan_wins_alone(vulkan: Option<&[wgpu::DeviceType]>, adapter_named: bool) -> bool {
    !adapter_named && vulkan.is_some_and(|types| types.contains(&wgpu::DeviceType::DiscreteGpu))
}

fn automatic_backends(named: bool) -> wgpu::Backends {
    if !cfg!(windows) || named {
        return wgpu::Backends::PRIMARY;
    }
    let started = std::time::Instant::now();
    let vulkan = backend_adapters(wgpu::Backend::Vulkan);
    if vulkan_wins_alone(vulkan.as_deref(), named) {
        log::info!(
            "graphics: Vulkan offers a dedicated GPU (checked in {} ms), DirectX 12 adapters are not opened",
            started.elapsed().as_millis()
        );
        return wgpu::Backends::VULKAN;
    }
    wgpu::Backends::PRIMARY
}

fn instance_backends(forced: Option<wgpu::Backend>, named: bool) -> (wgpu::Backends, Option<wgpu::Backend>) {
    if let Some(env) = wgpu::Backends::from_env() {
        return (env, None);
    }
    match forced {
        Some(b) if backend_has_adapter(b) => (wgpu::Backends::from(b), Some(b)),
        Some(b) => {
            log::warn!(
                "graphics API {b:?} was asked for but no graphics adapter offers it, the automatic choice is used"
            );
            (automatic_backends(named), None)
        }
        None => (automatic_backends(named), None),
    }
}

fn wanted_adapter(setting: Option<&str>) -> Option<String> {
    std::env::var("HP_ADAPTER")
        .ok()
        .or_else(|| setting.map(str::to_owned))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
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
    wanted: Option<String>,
    fallback_from: Option<String>,
) -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    if std::env::var_os("WGPU_DX12_COMPILER").is_none() {
        setup.instance_descriptor.backend_options.dx12.shader_compiler = dx12_shader_compiler(compiler);
    }
    let (backends, forced) = instance_backends(forced, wanted.is_some());
    setup.instance_descriptor.backends = backends;
    if transparent {
        setup.instance_descriptor.backend_options.dx12.presentation_system = wgpu::Dx12SwapchainKind::DxgiFromVisual;
    }
    setup.native_adapter_selector =
        Some(Arc::new(move |adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
            let picked = pick_adapter(adapters, surface, transparent, forced, wanted.as_deref());
            if let Ok(adapter) = &picked {
                record_choice(adapters, adapter, wanted.as_deref(), fallback_from.clone());
            }
            picked
        }));
    setup.device_descriptor = Arc::new(|adapter: &wgpu::Adapter| wgpu::DeviceDescriptor {
        label: Some("hammerpro device"),
        required_features: (adapter.features()
            & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::POLYGON_MODE_LINE))
            | hp_render::pipeline_cache::wanted_features(adapter)
            | hp_render::profile::wanted_features(adapter),
        required_limits: adapter.limits(),
        memory_hints: hp_render::memory::memory_hints(),
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
    wanted: Option<&str>,
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
        let kind = adapter_rank(adapter_kind(&info));
        let api = match info.backend {
            wgpu::Backend::Vulkan | wgpu::Backend::Metal => 0,
            wgpu::Backend::Dx12 => 1,
            wgpu::Backend::Gl => 2,
            _ => 3,
        };
        let other_api = forced.is_some_and(|f| f != info.backend);
        (other_api, kind, slow_dx12(info.backend), !transparent(a), api)
    };
    let named = |a: &wgpu::Adapter| wanted.is_some_and(|w| adapter_matches(w, &a.get_info()));
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
    let reason = if named(best) {
        "the graphics card asked for in the settings or HP_ADAPTER"
    } else {
        "the most powerful card that can draw to the window (dedicated before integrated, software last)"
    };
    log::info!(
        "graphics adapter: {} ({:?}, {:?}), transparent windows: {}{}{}, reason: {reason}",
        info.name,
        info.backend,
        info.device_type,
        if can { "yes" } else { "no (square window corners)" },
        shader_compiler,
        if forced.is_some() { ", graphics API chosen in the settings or on the command line" } else { "" }
    );
    if slow_dx12(info.backend) && compiler.as_deref() == Some("fxc") {
        log::info!(
            "graphics: DirectX 12 compiles shaders with FXC, the first start on this graphics card is slow (later starts reuse the shader cache)"
        );
    }
    Ok(best.clone())
}

fn adapter_kind(info: &wgpu::AdapterInfo) -> hp_app::runtime::adapter::AdapterKind {
    use hp_app::runtime::adapter::{AdapterKind, kind_of};
    let kind = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => AdapterKind::Dedicated,
        wgpu::DeviceType::IntegratedGpu => AdapterKind::Integrated,
        wgpu::DeviceType::VirtualGpu => AdapterKind::Virtual,
        wgpu::DeviceType::Cpu => AdapterKind::Software,
        wgpu::DeviceType::Other => AdapterKind::Other,
    };
    kind_of(&info.name, kind)
}

fn adapter_rank(kind: hp_app::runtime::adapter::AdapterKind) -> u8 {
    use hp_app::runtime::adapter::AdapterKind;
    match kind {
        AdapterKind::Dedicated => 0,
        AdapterKind::Integrated => 1,
        AdapterKind::Virtual => 2,
        AdapterKind::Other => 3,
        AdapterKind::Software => 4,
    }
}

fn adapter_matches(wanted: &str, info: &wgpu::AdapterInfo) -> bool {
    hp_app::runtime::adapter::name_matches(wanted, &info.name, adapter_kind(info))
}

fn adapter_entry(info: &wgpu::AdapterInfo) -> hp_app::runtime::adapter::AdapterEntry {
    let driver = [info.driver.trim(), info.driver_info.trim()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    hp_app::runtime::adapter::AdapterEntry {
        name: info.name.clone(),
        kind: adapter_kind(info),
        backend: format!("{:?}", info.backend),
        driver,
    }
}

fn record_choice(adapters: &[wgpu::Adapter], chosen: &wgpu::Adapter, wanted: Option<&str>, fallback: Option<String>) {
    let info = chosen.get_info();
    let all: Vec<_> = adapters.iter().map(|a| adapter_entry(&a.get_info())).collect();
    let wanted_found = wanted.is_some_and(|w| adapter_matches(w, &info));
    let chosen = adapter_entry(&info);
    let notice = hp_app::runtime::adapter::notice_for(&chosen, &all, wanted_found);
    if let Some(w) = wanted.filter(|_| !wanted_found) {
        log::warn!("graphics: no graphics adapter matches \"{w}\", {} is used", chosen.name);
    }
    for a in &all {
        log::info!("graphics adapter seen: {} ({}, {}, driver {})", a.name, a.kind.id(), a.backend, a.driver);
    }
    hp_app::runtime::adapter::set_choice(hp_app::runtime::adapter::AdapterChoice {
        chosen,
        notice,
        all,
        wanted: wanted.map(str::to_owned),
        wanted_found,
        fallback_from: fallback,
    });
}

fn compile_step_name(arg: &str) -> Option<&'static str> {
    match arg {
        bspc_cli::BSPC_ARG => Some("BSP and visibility"),
        bspc_cli::RAD_ARG => Some("lighting"),
        bspc_cli::CUBEMAPS_ARG => Some("cubemaps"),
        dispatch::lighting::child::LIGHT_ARG => Some("lighting preview"),
        _ => None,
    }
}

fn main() -> eframe::Result<()> {
    if std::env::args()
        .nth(1)
        .is_some_and(|arg| arg == bspc_cli::BSPC_ARG || arg == bspc_cli::RAD_ARG || arg == bspc_cli::CUBEMAPS_ARG)
    {
        let settings = hp_app::settings::Settings::load_or_default(&hp_app::settings::Settings::default_path());
        hp_fs::configure_scan(settings.scan.to_config());
    }
    if let Some(step) = std::env::args().nth(1).as_deref().and_then(compile_step_name) {
        native_crash::mark_compile_step(step);
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
    if std::env::args().nth(1).as_deref() == Some(bspc_cli::CUBEMAPS_ARG) {
        let args: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(bspc_cli::run_cubemaps(&args));
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
    hp_sched::configure_global();
    let cli = parse_cli();
    let log_rx = logger::init(!cli.is_automated());
    if !cli.is_automated() {
        crash::install_hook();
    } else if let Some(dir) = std::env::var_os("HAMMERPRO_CRASH_DIR") {
        crash::install_hook_at(std::path::PathBuf::from(dir));
    }

    let saved = (!cli.is_automated())
        .then(|| hp_app::settings::Settings::load_or_default(&hp_app::settings::Settings::default_path()));
    let native_frames = saved.as_ref().is_some_and(|s| s.ui.native_frames);
    let backend = cli.backend.or(saved.as_ref().map(|s| s.render.graphics_backend)).unwrap_or_default();
    let wanted = wanted_adapter(saved.as_ref().map(|s| s.render.graphics_adapter.as_str()));
    let fallback_from = cli.graphics_fallback.clone();
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
        wgpu_options: wgpu_options(true, forced_backend(backend), dx12_compiler, wanted, fallback_from.clone()),
        system_font_fallback: false,
        ..Default::default()
    };

    let result = eframe::run_native(
        "HammerPro",
        options,
        Box::new(move |cc| {
            WINDOW_STARTED.store(true, std::sync::atomic::Ordering::Release);
            Ok(Box::new(app::HammerProApp::new(cc, cli, log_rx)))
        }),
    );
    if let Err(e) = &result
        && !WINDOW_STARTED.load(std::sync::atomic::Ordering::Acquire)
        && fallback_from.is_none()
        && cfg!(windows)
    {
        let used = hp_app::runtime::adapter::choice().map(|c| c.chosen.backend);
        let other = fallback_backend(used.as_deref(), backend);
        let from = used.unwrap_or_else(|| forced_backend(backend).map_or("Vulkan".to_owned(), |b| format!("{b:?}")));
        log::error!("graphics: the window could not start with {from} ({e}), starting again with {}", other.id());
        if relaunch_with_backend(other, &from) {
            return Ok(());
        }
    }
    result
}

fn fallback_backend(used: Option<&str>, asked: hp_app::settings::GraphicsBackend) -> hp_app::settings::GraphicsBackend {
    use hp_app::settings::GraphicsBackend;
    match used {
        Some(b) if b.eq_ignore_ascii_case("dx12") => GraphicsBackend::Vulkan,
        Some(_) => GraphicsBackend::Dx12,
        None if asked == GraphicsBackend::Dx12 => GraphicsBackend::Vulkan,
        None => GraphicsBackend::Dx12,
    }
}

fn args_with_backend(
    mut args: impl Iterator<Item = String>,
    backend: hp_app::settings::GraphicsBackend,
    from: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--backend" || arg == GRAPHICS_FALLBACK_ARG {
            args.next();
            continue;
        }
        out.push(arg);
    }
    out.extend([
        "--backend".to_owned(),
        backend.id().to_ascii_lowercase(),
        GRAPHICS_FALLBACK_ARG.to_owned(),
        from.to_owned(),
    ]);
    out
}

fn relaunch_with_backend(backend: hp_app::settings::GraphicsBackend, from: &str) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let args = args_with_backend(std::env::args().skip(1), backend, from);
    match std::process::Command::new(exe).args(args).spawn() {
        Ok(_) => true,
        Err(e) => {
            log::error!("graphics: cannot start Hammer Pro again with {}: {e}", backend.id());
            false
        }
    }
}

#[cfg(test)]
mod backend_tests {
    use super::*;

    #[test]
    fn a_failed_graphics_start_tries_the_other_api_once() {
        use hp_app::settings::GraphicsBackend;
        assert_eq!(fallback_backend(Some("Vulkan"), GraphicsBackend::Auto), GraphicsBackend::Dx12);
        assert_eq!(fallback_backend(Some("Dx12"), GraphicsBackend::Auto), GraphicsBackend::Vulkan);
        assert_eq!(fallback_backend(None, GraphicsBackend::Dx12), GraphicsBackend::Vulkan);
        assert_eq!(fallback_backend(None, GraphicsBackend::Auto), GraphicsBackend::Dx12);
        let args = ["map.vmf", "--backend", "vulkan", "--lang", "fr"].map(str::to_owned);
        assert_eq!(
            args_with_backend(args.into_iter(), GraphicsBackend::Dx12, "Vulkan"),
            ["map.vmf", "--lang", "fr", "--backend", "dx12", GRAPHICS_FALLBACK_ARG, "Vulkan"].map(str::to_owned)
        );
    }

    #[test]
    fn the_automatic_choice_never_takes_a_software_adapter_when_a_real_card_exists() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: automatic_backends(false),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = pollster::block_on(instance.enumerate_adapters(automatic_backends(false)));
        let Ok(chosen) = pick_adapter(&adapters, None, false, None, None) else { return };
        let kinds: Vec<_> = adapters.iter().map(|a| adapter_entry(&a.get_info())).collect();
        let chosen = adapter_entry(&chosen.get_info());
        eprintln!("adapters: {kinds:?}, automatic choice: {chosen:?}");
        let best = kinds.iter().map(|a| adapter_rank(a.kind)).min().unwrap_or(u8::MAX);
        assert_eq!(adapter_rank(chosen.kind), best, "the most powerful kind of card is chosen");
        if kinds.iter().any(|a| a.kind != hp_app::runtime::adapter::AdapterKind::Software) {
            assert_ne!(chosen.kind, hp_app::runtime::adapter::AdapterKind::Software);
        }
    }

    #[test]
    fn software_adapters_rank_last() {
        use hp_app::runtime::adapter::AdapterKind;
        assert!(adapter_rank(AdapterKind::Dedicated) < adapter_rank(AdapterKind::Integrated));
        assert!(adapter_rank(AdapterKind::Other) < adapter_rank(AdapterKind::Software));
    }

    #[test]
    fn vulkan_alone_only_with_a_dedicated_gpu_and_no_named_adapter() {
        use wgpu::DeviceType::{Cpu, DiscreteGpu, IntegratedGpu};
        assert!(vulkan_wins_alone(Some(&[IntegratedGpu, DiscreteGpu]), false));
        assert!(!vulkan_wins_alone(Some(&[DiscreteGpu]), true));
        assert!(!vulkan_wins_alone(Some(&[IntegratedGpu, Cpu]), false));
        assert!(!vulkan_wins_alone(Some(&[]), false));
        assert!(!vulkan_wins_alone(None, false));
    }
}
