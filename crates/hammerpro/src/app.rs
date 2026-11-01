use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::Receiver;
use hp_app::settings::Settings;
use hp_app::sidecar::Sidecar;
use hp_app::state::{AppState, AssetStatus, Gfx, RightTab};
use hp_app::{Action, Severity, Tool, ToolFlow, ToolId, ViewInput, t, tf};

use crate::Cli;
use crate::autosave::{self, Autosaver};
use crate::crash::{self, PendingCrash};
use crate::files::{self, PendingOp};
use crate::keyboard::{AppCapture, KeyRouter};
use crate::script::{self, Command, ScriptPos, Step};
use crate::tabs::{OpenTarget, TabAction, TabId, Tabs};

struct NullTool(ToolId);

impl Tool for NullTool {
    fn id(&self) -> ToolId {
        self.0
    }
    fn view_input(&mut self, _state: &mut AppState, _input: &ViewInput<'_>) -> ToolFlow {
        ToolFlow::Ignored
    }
}

struct ScreenshotJob {
    path: PathBuf,
    frames: u64,
    requested: bool,
    done: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone, Debug)]
pub(crate) struct RecoverPrompt {
    pub autosave: PathBuf,
    pub original: PathBuf,
}

const OPTIONS_ROW_HEIGHT: f32 = 30.0;
const MAX_DROPPED_MAPS: usize = 16;
const IDLE_GAP: f32 = 0.1;

pub struct HammerProApp {
    pub(crate) state: AppState,
    pub(crate) tools: Vec<Box<dyn Tool>>,
    tools_available: Vec<ToolId>,
    pub(crate) shell: hp_ui_shell::Shell,
    pub(crate) viewports: hp_ui_viewports::Viewports,
    pub(crate) textures: hp_ui_textures::TextureBrowser,
    pub(crate) texture_dialogs: hp_ui_textures::TextureDialogs,
    pub(crate) entity_panel: hp_ui_entities::EntityPanel,
    pub(crate) models: hp_ui_entities::ModelBrowser,
    pub(crate) entity_dialogs: hp_ui_entities::EntityDialogs,
    pub(crate) visgroups: hp_ui_mapping::VisgroupPanel,
    pub(crate) outliner: hp_ui_mapping::Outliner,
    pub(crate) compile: hp_ui_mapping::CompileDialog,
    pub(crate) map_dialogs: hp_ui_mapping::MapDialogs,
    log_rx: Receiver<(Severity, String)>,
    autosaver: Autosaver,
    discord: crate::discord::DiscordPresence,
    pub(crate) pending: Option<PendingOp>,
    pub(crate) allow_close: bool,
    pub(crate) hollow_prompt: Option<crate::prompts::HollowPrompt>,
    pub(crate) recover_prompt: Option<RecoverPrompt>,
    pub(crate) crash_prompt: Option<PendingCrash>,
    pub(crate) compile_exit_prompt: bool,
    background: hp_ui_shell::background::BackgroundImage,
    settings_saved_at: Instant,
    screenshot: Option<ScreenshotJob>,
    last_frame: Instant,
    start: Instant,
    avg_frame_time: f32,
    worst_frame: f32,
    title: String,
    native_frames_at_start: bool,
    pub(crate) central_size: egui::Vec2,
    pub(crate) clipboard: Option<arboard::Clipboard>,
    pub(crate) paste_event_text: Option<String>,
    pub(crate) pkg: crate::dispatch::Packages,
    pub(crate) not_implemented_seen: HashSet<Action>,
    sidecar_serial: u64,
    keys: KeyRouter,
    reported_assets: Option<AssetStatus>,
    reported_fgd_classes: usize,
    emergency_saved: bool,
    initial_open: Option<PathBuf>,
    first_frame: bool,
    script: Option<std::collections::VecDeque<Step>>,
    script_command: Option<Command>,
    script_idle: Option<(Instant, u64)>,
    pub(crate) persist_settings: bool,
    pub(crate) opener: crate::opening::Opener,
    pub(crate) startup: crate::timing::StartupReport,
    pub(crate) open_report: Option<crate::timing::OpenReport>,
    pub(crate) last_sync: std::time::Duration,
    pub(crate) sync_count: u64,
    pub(crate) open_longest_frame: std::time::Duration,
    pub(crate) bench: Option<crate::opening::Bench>,
    pub(crate) tabs: Tabs,
    pub(crate) open_target: Option<OpenTarget>,
    pub(crate) open_queue: VecDeque<(PathBuf, files::OpenKind, OpenTarget)>,
    pub(crate) close_queue: VecDeque<TabId>,
    pub(crate) close_keep: Option<TabId>,
    pub(crate) exit_answered: HashSet<TabId>,
    pub(crate) published_tabs: Option<(crate::tabs::PublishedTabs, Option<PathBuf>)>,
    pub(crate) tab_bar_action: Option<TabAction>,
    pub(crate) unsaved_asked: Option<(PendingOp, TabId)>,
    dropped_maps: Vec<PathBuf>,
}

fn register_all_translations() {
    hp_app::register_translations();
    hp_assets::register_translations();
    hp_ui_shell::register_translations();
    hp_ui_viewports::register_translations();
    hp_tools_select::register_translations();
    hp_tools_create::register_translations();
    hp_tools_mesh::register_translations();
    hp_ui_textures::register_translations();
    hp_ui_entities::register_translations();
    hp_ui_mapping::register_translations();
    hp_app::i18n::register(TR);
    hp_app::i18n::register(crate::tabs::TR);
    crate::dispatch::register_translations();
}

fn make_tools() -> (Vec<Box<dyn Tool>>, Vec<ToolId>) {
    let mut available: Vec<Box<dyn Tool>> = Vec::new();
    available.extend(hp_tools_select::tools());
    available.extend(hp_tools_create::tools());
    available.extend(hp_tools_mesh::tools());
    available.push(hp_ui_textures::texture_tool());
    crate::dispatch::package_tools(&mut available);
    let ids: Vec<ToolId> = ToolId::ALL.iter().copied().filter(|id| available.iter().any(|t| t.id() == *id)).collect();
    let tools = ToolId::ALL
        .iter()
        .map(|&id| match available.iter().position(|t| t.id() == id) {
            Some(i) => available.swap_remove(i),
            None => Box::new(NullTool(id)) as Box<dyn Tool>,
        })
        .collect();
    (tools, ids)
}

fn tool_index(id: ToolId) -> usize {
    ToolId::ALL.iter().position(|&t| t == id).unwrap_or(0)
}

pub(crate) fn dock_rank(tab: RightTab) -> u8 {
    match tab {
        RightTab::Tool => 0,
        RightTab::Textures => 1,
        RightTab::Visgroups => 2,
        RightTab::Entity => 3,
        RightTab::Package(id) if id == crate::dispatch::PREFABS_TAB => 4,
        RightTab::Models => 5,
        RightTab::Outliner => 6,
        RightTab::Package(_) => 7,
    }
}

pub(crate) fn open_folder(path: &std::path::Path) {
    hp_app::os::open_folder(path, true);
}

impl HammerProApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cli: Cli, log_rx: Receiver<(Severity, String)>) -> Self {
        let startup = crate::timing::StartupReport::new();
        register_all_translations();
        let automated = cli.is_automated();
        hp_compile::sandbox::set_sandboxed(automated);
        let settings_path = Settings::default_path();
        let first_run = !settings_path.exists();
        let mut settings = if automated {
            Settings { language_chosen: true, ..Settings::default() }
        } else {
            Settings::load_or_default(&settings_path)
        };
        if automated {
            settings.edit.autosave_minutes = 0;
            settings.compile.copy_to_maps = false;
            settings.compile.launch_game = false;
        }
        settings.sync_game();
        hp_app::i18n::set_lang(cli.lang.unwrap_or(settings.language));
        hp_app::theme::set_accent(settings.ui.accent_color);
        hp_app::theme::apply(&cc.egui_ctx, settings.ui.theme, settings.ui.ui_scale);
        cc.egui_ctx.options_mut(|o| o.quit_shortcuts.clear());
        #[cfg(windows)]
        {
            use wgpu::rwh::{HasWindowHandle as _, RawWindowHandle};
            if let Ok(handle) = cc.window_handle()
                && let RawWindowHandle::Win32(h) = handle.as_raw()
            {
                hp_ui_mapping::native_window::set_main_window(h.hwnd.get());
                hp_ui_mapping::native_window::prepare_window(h.hwnd.get());
            }
        }

        let mut assets = hp_assets::AssetManager::new(&settings);
        if !automated {
            assets.set_thumbnail_cache(Some(Settings::cache_dir().join("thumbs")));
            assets.set_live_reload(true);
        }
        let mut state = AppState::new(settings, Box::new(assets));
        if !automated {
            state.sidecar = Sidecar::open(Some(&Sidecar::default_dir()), None);
        }
        files::apply_doc_options(&mut state);
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            crate::gpu_guard::install(&rs.device);
            state.gfx = Some(Gfx::new(rs));
        }
        state.ui.show_first_run = first_run && !automated;
        if let Some(tool) = cli.tool {
            state.tool = tool;
        }
        state.ui.maximized_view = cli.maximize_view;
        state.console.info(format!("Hammer Pro {}", env!("CARGO_PKG_VERSION")));

        let (mut tools, tools_available) = make_tools();
        let active = tool_index(state.tool);
        tools[active].activate(&mut state);

        let screenshot = cli.screenshot.clone().map(|path| ScreenshotJob {
            path,
            frames: cli.frames.max(2),
            requested: false,
            done: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        let crash_prompt = if automated {
            None
        } else {
            let previous_log = Settings::log_dir().join("hammerpro.old.log");
            crash::begin_session(
                &Settings::crash_dir(),
                std::process::id(),
                Some(&previous_log),
                &hp_mcp::registry::pid_alive,
            )
        };

        let native_frames_at_start = state.settings.ui.native_frames;
        let mut app = Self {
            state,
            tools,
            tools_available,
            shell: hp_ui_shell::Shell::new(),
            viewports: hp_ui_viewports::Viewports::new(),
            textures: hp_ui_textures::TextureBrowser::new(),
            texture_dialogs: hp_ui_textures::TextureDialogs::new(),
            entity_panel: hp_ui_entities::EntityPanel::new(),
            models: hp_ui_entities::ModelBrowser::new(),
            entity_dialogs: hp_ui_entities::EntityDialogs::new(),
            visgroups: hp_ui_mapping::VisgroupPanel::new(),
            outliner: hp_ui_mapping::Outliner::new(),
            compile: hp_ui_mapping::CompileDialog::new(),
            map_dialogs: hp_ui_mapping::MapDialogs::new(),
            log_rx,
            autosaver: Autosaver::new(),
            discord: crate::discord::DiscordPresence::new(!automated),
            pending: None,
            allow_close: false,
            hollow_prompt: None,
            recover_prompt: None,
            crash_prompt,
            compile_exit_prompt: false,
            background: hp_ui_shell::background::BackgroundImage::default(),
            settings_saved_at: Instant::now(),
            screenshot,
            last_frame: Instant::now(),
            start: Instant::now(),
            avg_frame_time: 0.0,
            worst_frame: 0.0,
            title: String::new(),
            native_frames_at_start,
            central_size: egui::vec2(1200.0, 800.0),
            clipboard: arboard::Clipboard::new().ok(),
            paste_event_text: None,
            pkg: crate::dispatch::Packages::default(),
            not_implemented_seen: HashSet::new(),
            sidecar_serial: 0,
            keys: KeyRouter::default(),
            reported_assets: None,
            reported_fgd_classes: 0,
            emergency_saved: false,
            initial_open: cli.open,
            first_frame: true,
            script: cli.script.as_ref().map(|path| match std::fs::read_to_string(path) {
                Ok(text) => match script::parse(&text) {
                    Ok(cmds) => script::expand(cmds),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(2);
                    }
                },
                Err(e) => {
                    eprintln!("cannot read script {}: {e}", path.display());
                    std::process::exit(2);
                }
            }),
            script_command: None,
            script_idle: None,
            persist_settings: !automated,
            opener: crate::opening::Opener::default(),
            startup,
            open_report: None,
            last_sync: std::time::Duration::ZERO,
            sync_count: 0,
            open_longest_frame: std::time::Duration::ZERO,
            bench: cli.bench_open.clone().map(crate::opening::Bench::new),
            tabs: Tabs::default(),
            open_target: None,
            open_queue: VecDeque::new(),
            close_queue: VecDeque::new(),
            close_keep: None,
            exit_answered: HashSet::new(),
            published_tabs: None,
            tab_bar_action: None,
            unsaved_asked: None,
            dropped_maps: Vec::new(),
        };
        if let Some(bench) = &app.bench {
            app.initial_open = Some(bench.map().to_path_buf());
        }
        crate::dispatch::init(&mut app);
        app.startup.mark("app");
        app
    }

    pub(crate) fn active_tool_action(&mut self, action: Action) -> bool {
        let i = tool_index(self.state.tool);
        self.tools[i].action(&mut self.state, action)
    }

    pub(crate) fn has_tool(&self, id: ToolId) -> bool {
        self.tools_available.contains(&id)
    }

    pub(crate) fn deactivate_tool(&mut self) {
        let i = tool_index(self.state.tool);
        self.unrecorded_selection(|app| app.tools[i].deactivate(&mut app.state));
    }

    pub(crate) fn activate_tool(&mut self) {
        let i = tool_index(self.state.tool);
        self.unrecorded_selection(|app| app.tools[i].activate(&mut app.state));
    }

    fn unrecorded_selection(&mut self, f: impl FnOnce(&mut Self)) {
        let on = self.state.doc.records_selection();
        self.state.doc.set_record_selection(false);
        f(self);
        self.state.doc.set_record_selection(on);
    }

    fn show_restored_selection(&mut self) {
        if !self.state.doc.take_selection_restored() {
            return;
        }
        let sel = self.state.doc.selection();
        let (faces, objects) = (!sel.faces.is_empty(), !sel.objects.is_empty());
        let tool = self.state.tool;
        if faces && !matches!(tool, ToolId::Texture | ToolId::Displacement) && self.has_tool(ToolId::Texture) {
            self.switch_tool(ToolId::Texture);
        } else if !faces && objects && tool == ToolId::Texture {
            let objs = self.state.doc.selected_objects();
            self.switch_tool(ToolId::Select);
            self.unrecorded_selection(|app| app.state.doc.select(&objs, hp_editor::SelectOp::Replace));
        }
    }

    pub(crate) fn viewports_size_2d(&self) -> egui::Vec2 {
        [hp_app::ViewKind::Top, hp_app::ViewKind::Front, hp_app::ViewKind::Side]
            .into_iter()
            .find_map(|k| self.viewports.view_size(k))
            .unwrap_or(self.central_size * 0.5)
    }

    pub(crate) fn switch_tool(&mut self, id: ToolId) {
        if id == self.state.tool {
            let i = tool_index(id);
            self.tools[i].reactivate(&mut self.state);
            return;
        }
        let old = tool_index(self.state.tool);
        let new = tool_index(id);
        self.unrecorded_selection(|app| {
            app.tools[old].deactivate(&mut app.state);
            app.state.tool = id;
            app.tools[new].activate(&mut app.state);
        });
        if self.tools[new].wants_side_panel() {
            self.state.ui.right_tab = RightTab::Tool;
        } else if self.state.ui.right_tab == RightTab::Tool {
            self.state.ui.right_tab = RightTab::Textures;
        }
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.pending.is_some()
            || self.hollow_prompt.is_some()
            || self.recover_prompt.is_some()
            || self.crash_prompt.is_some()
            || self.compile_exit_prompt
            || self.state.ui.show_first_run
            || self.state.ui.show_command_palette
    }

    fn app_capture(&self) -> AppCapture {
        AppCapture {
            dialog: self.shell.dialog_open(&self.state) || self.modal_open(),
            keyboard: self.viewports.captures_keyboard() || self.shell.capturing_shortcut(),
            context_menu: self.viewports.is_context_menu_open(),
        }
    }

    fn begin_frame(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.worst_frame = self.worst_frame.max(elapsed);
        let dt = elapsed.min(0.25);
        if elapsed > 0.0 && elapsed < IDLE_GAP {
            self.avg_frame_time =
                if self.avg_frame_time == 0.0 { elapsed } else { self.avg_frame_time * 0.9 + elapsed * 0.1 };
        }
        self.state.apply_selection_undo_setting();
        let f = &mut self.state.frame;
        f.dt = dt;
        f.time = now.duration_since(self.start).as_secs_f64();
        f.index += 1;
        if self.avg_frame_time > 0.0 {
            f.fps = 1.0 / self.avg_frame_time;
        }

        if self.state.gfx.is_none()
            && let Some(rs) = frame.wgpu_render_state()
        {
            crate::gpu_guard::install(&rs.device);
            self.state.gfx = Some(Gfx::new(rs));
        }

        while let Ok((severity, text)) = self.log_rx.try_recv() {
            self.state.console.push(severity, text);
        }
        self.report_gpu_errors();

        let AppState { assets, gfx, .. } = &mut self.state;
        assets.begin_frame(ctx, gfx.as_mut());
        self.report_assets();
        self.poll_open(ctx);

        let openable = |e: &std::ffi::OsStr| ["vmf", "vmx", "vmm", "map"].iter().any(|x| e.eq_ignore_ascii_case(x));
        self.dropped_maps = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| p.extension().is_some_and(openable))
                .take(MAX_DROPPED_MAPS)
                .collect()
        });
        if !self.state.settings.workflow.open_in_new_tab {
            self.dropped_maps.truncate(1);
        }

        if self.first_frame {
            self.first_frame = false;
            if let Some(path) = self.initial_open.take() {
                self.state.requested_open = Some(path);
            } else {
                files::frame_map(&mut self.state, self.central_size);
            }
        }
        crate::dispatch::tick(self, ctx);
    }

    fn report_assets(&mut self) {
        for (severity, text) in self.state.assets.take_messages() {
            self.state.log(severity, text);
        }
        let status = self.state.assets.status();
        if self.reported_assets.as_ref() != Some(&status) {
            if status == AssetStatus::Ready {
                let materials = self.state.assets.materials().len();
                let models = self.state.assets.models().len();
                self.state.log(Severity::Info, tf!("app.content_ready", materials, models));
            }
            self.reported_assets = Some(status);
        }
        let classes = self.state.assets.fgd().map_or(0, |f| f.len());
        if classes != self.reported_fgd_classes {
            if classes > 0 {
                self.state.log(Severity::Info, tf!("app.fgd_loaded", classes));
            }
            self.reported_fgd_classes = classes;
        }
    }

    fn route_keys(&mut self, ctx: &egui::Context) {
        let capture = self.app_capture();
        let i = tool_index(self.state.tool);
        let routed = self.keys.after_ui(ctx, &mut self.state, self.tools[i].as_mut(), capture);
        self.paste_event_text = routed.paste_text;
        for action in routed.actions {
            self.state.request(action);
        }
    }

    fn window(&mut self, ui: &mut egui::Ui) {
        let native = self.state.settings.ui.native_frames;
        hp_app::chrome::sync_decorations(ui.ctx(), native, self.native_frames_at_start);
        let dirty = self.state.doc.is_dirty();
        let mut spec =
            hp_app::chrome::ChromeSpec::main(format!("{} - Hammer Pro", files::map_display_name(&self.state)), native);
        spec.dirty = dirty;
        let file = match self.state.doc.path() {
            Some(p) => p.display().to_string(),
            None => t!("shell.menu.not_saved_yet").to_owned(),
        };
        spec.title_tooltip = Some(if dirty { format!("{file}\n{}", t!("shell.menu.unsaved_changes")) } else { file });
        let mut chrome = hp_app::chrome::Chrome::begin(ui, spec);
        let menus_end = chrome.title_bar(ui, |ui| self.shell.title_menus(ui, &mut self.state));
        if let Some(x) = menus_end {
            chrome.set_title_left(x);
        }
        let in_title = menus_end.is_some();
        chrome.body(ui, |ui| self.layout(ui, !in_title));
        chrome.end(ui);
    }

    fn layout(&mut self, ui: &mut egui::Ui, menu_bar: bool) {
        egui::Panel::top("hp_top").show(ui, |ui| {
            if menu_bar {
                self.shell.menu_bar(ui, &mut self.state);
            }
            self.shell.toolbar(ui, &mut self.state);
            let i = tool_index(self.state.tool);
            ui.separator();
            ui.horizontal(|ui| {
                ui.set_min_height(OPTIONS_ROW_HEIGHT);
                ui.label(egui::RichText::new(t!(self.state.tool.label_key())).strong());
                ui.separator();
                if self.tools[i].wants_side_panel() {
                    ui.label(egui::RichText::new(t!("app.options_in_dock")).weak());
                } else {
                    self.tools[i].options_ui(ui, &mut self.state);
                }
            });
        });

        let tool_hint = {
            let i = tool_index(self.state.tool);
            self.tools[i].status_hint(&self.state)
        };
        let compile_hint = self.compile.is_running().then(|| t!("menu.compiling").to_owned());
        let pick_hint = self.viewports.pick_hint(&self.state);
        if self.state.settings.workflow.show_status_bar {
            egui::Panel::bottom("hp_status").show(ui, |ui| {
                self.shell.status_bar(ui, &mut self.state, compile_hint.or(pick_hint).or(tool_hint));
            });
        }

        self.shell.tool_strip(ui, &mut self.state);

        if self.state.settings.ui.show_console {
            egui::Panel::bottom("hp_console").resizable(true).default_size(150.0).min_size(60.0).show(ui, |ui| {
                self.shell.console(ui, &mut self.state);
            });
        }

        self.dock(ui);

        egui::Panel::top("hp_map_tabs").exact_size(crate::tabs::TAB_BAR_HEIGHT).show(ui, |ui| {
            self.map_tabs_ui(ui);
        });

        {
            let opts = self.state.scene_options();
            let AppState { assets, doc, gfx, .. } = &mut self.state;
            if let Some(gfx) = gfx.as_mut() {
                let start = Instant::now();
                assets.sync_scene(doc, gfx, &opts);
                self.last_sync = start.elapsed();
                self.sync_count += 1;
            }
        }
        crate::dispatch::lighting::scene_synced(self);

        egui::CentralPanel::no_frame().show(ui, |ui| {
            self.central_size = ui.available_size();
            let i = tool_index(self.state.tool);
            let (state, tools) = (&mut self.state, &mut self.tools);
            self.viewports.ui(ui, state, tools[i].as_mut());
            self.shell.loading_overlay(ui, &mut self.state);
        });
    }

    fn dock_tabs(&self) -> Vec<(RightTab, String)> {
        let ui = &self.state.settings.ui;
        let mut tabs = Vec::new();
        let i = tool_index(self.state.tool);
        let mut push = |tab: RightTab, key: &str| tabs.push((tab, t!(key).to_owned()));
        if self.tools[i].wants_side_panel() {
            push(RightTab::Tool, self.tools[i].panel_label_key());
        }
        if ui.show_texture_browser {
            push(RightTab::Textures, "dock.textures");
        }
        if ui.show_entity_properties {
            push(RightTab::Entity, "dock.entity");
        }
        if ui.show_model_browser {
            push(RightTab::Models, "dock.models");
        }
        if ui.show_visgroups {
            push(RightTab::Visgroups, "dock.visgroups");
        }
        if ui.show_outliner {
            push(RightTab::Outliner, "dock.outliner");
        }
        for tab in crate::dispatch::dock_tabs(self) {
            tabs.push((RightTab::Package(tab.id), tab.label));
        }
        tabs.sort_by_key(|(tab, _)| dock_rank(*tab));
        tabs
    }

    fn dock(&mut self, ui: &mut egui::Ui) {
        let tabs = self.dock_tabs();
        if tabs.is_empty() {
            return;
        }
        if !tabs.iter().any(|(t, _)| *t == self.state.ui.right_tab) {
            self.state.ui.right_tab = tabs[0].0;
        }
        egui::Panel::right("hp_dock").resizable(true).default_size(380.0).min_size(260.0).show(ui, |ui| {
            self.shell.selection_bar(ui, &mut self.state);
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for (tab, label) in &tabs {
                    let selected = self.state.ui.right_tab == *tab;
                    if ui.add(egui::Button::selectable(selected, label.as_str())).clicked() {
                        self.state.ui.right_tab = *tab;
                    }
                }
            });
            ui.separator();
            match self.state.ui.right_tab {
                RightTab::Tool => {
                    let i = tool_index(self.state.tool);
                    egui::ScrollArea::vertical().id_salt("hp_tool_panel").show(ui, |ui| {
                        self.tools[i].options_ui(ui, &mut self.state);
                    });
                }
                RightTab::Textures => self.textures.ui(ui, &mut self.state),
                RightTab::Entity => self.entity_panel.ui(ui, &mut self.state),
                RightTab::Models => self.models.ui(ui, &mut self.state),
                RightTab::Visgroups => self.visgroups.ui(ui, &mut self.state),
                RightTab::Outliner => self.outliner.ui(ui, &mut self.state),
                RightTab::Package(id) => crate::dispatch::dock_ui(self, ui, id),
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.shell.dialogs(ctx, &mut self.state);
        self.texture_dialogs.show(ctx, &mut self.state);
        self.entity_dialogs.show(ctx, &mut self.state);
        self.map_dialogs.show(ctx, &mut self.state);
        self.compile.show(ctx, &mut self.state);
        crate::dispatch::dialogs(self, ctx);
        self.unsaved_prompt(ctx);
        self.hollow_dialog(ctx);
        self.compile_exit_dialog(ctx);
        self.recover_dialog(ctx);
        self.crash_dialog(ctx);
        self.shell.toasts(ctx, &mut self.state);
    }

    pub(crate) fn run_pending(&mut self, ctx: &egui::Context, op: PendingOp) {
        let here = OpenTarget::Replace(self.tabs.active_id());
        match op {
            PendingOp::New => {
                self.opener.cancel();
                self.open_queue.clear();
                self.deactivate_tool();
                files::new_map(&mut self.state, self.central_size);
                self.tabs.touch();
                self.activate_tool();
            }
            PendingOp::NewTab => self.new_tab(),
            PendingOp::OpenTab(path) => self.start_open(ctx, &path, files::OpenKind::Map, OpenTarget::NewTab),
            PendingOp::Open(path) => self.start_open(ctx, &path, files::OpenKind::Map, here),
            PendingOp::Recover { autosave, original } => {
                self.start_open(ctx, &autosave, files::OpenKind::Recovered { original }, here);
            }
            PendingOp::Exit => {
                self.exit_answered.insert(self.tabs.active_id());
                self.continue_exit(ctx);
            }
            PendingOp::Close => self.finish_close(),
        }
    }

    pub(crate) fn offer_recovery(&mut self, path: &std::path::Path) {
        if self.persist_settings
            && let Some(autosave) = autosave::recovery_candidate(&Settings::autosave_dir(), path)
        {
            self.recover_prompt = Some(RecoverPrompt { autosave, original: path.to_path_buf() });
        }
    }

    pub(crate) fn guard_unsaved(&mut self, ctx: &egui::Context, op: PendingOp) {
        if op == PendingOp::Exit {
            self.exit_answered.clear();
            self.continue_exit(ctx);
        } else if self.state.doc.is_dirty() {
            self.pending = Some(op);
        } else {
            self.run_pending(ctx, op);
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.allow_close {
            return;
        }
        if self.compile.is_running() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.compile_exit_prompt = true;
        } else if self.any_map_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.pending.is_none() {
                self.exit_answered.clear();
                self.continue_exit(ctx);
            }
        }
    }

    pub(crate) fn confirm_exit_while_compiling(&mut self, ctx: &egui::Context) {
        self.compile_exit_prompt = false;
        self.compile.cancel_compile(&mut self.state);
        self.guard_unsaved(ctx, PendingOp::Exit);
    }

    fn end_frame(&mut self, ctx: &egui::Context) {
        let holes: Vec<egui::Rect> = if self.state.settings.ui.background_hide_3d {
            self.viewports
                .view_rects()
                .iter()
                .flatten()
                .filter(|(kind, _)| *kind == hp_app::ViewKind::Perspective)
                .map(|(_, rect)| *rect)
                .collect()
        } else {
            Vec::new()
        };
        self.background.paint(ctx, &self.state.settings.ui, &holes);
        if let Some(tool) = self.state.requested_tool.take() {
            self.switch_tool(tool);
        }
        let actions = std::mem::take(&mut self.state.requested_actions);
        for action in actions {
            self.execute(ctx, action);
        }
        self.paste_event_text = None;
        if let Some(action) = self.tab_bar_action.take()
            && self.pending.is_none()
        {
            self.tab_action(ctx, action);
        }
        if let Some(path) = self.state.requested_open.take() {
            self.request_open(ctx, path);
        }
        for path in std::mem::take(&mut self.dropped_maps) {
            if self.pending.is_some() {
                break;
            }
            self.request_open(ctx, path);
        }
        self.tabs_tick(ctx);
        self.show_restored_selection();
        if !ctx.input(|i| i.pointer.any_down()) {
            self.state.doc.end_selection_burst();
        }

        self.handle_close_request(ctx);

        if self.state.sidecar.serial() != self.sidecar_serial {
            self.sidecar_serial = self.state.sidecar.serial();
            crate::dispatch::map_loaded(self);
        }

        let title = format!(
            "{}{} - Hammer Pro",
            files::map_display_name(&self.state),
            if self.state.doc.is_dirty() { "*" } else { "" }
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }

        self.autosaver.tick(&mut self.state, &self.tabs);
        self.discord.tick(&mut self.state);

        if self.state.settings_dirty && self.settings_saved_at.elapsed().as_secs_f32() > 2.0 {
            self.save_settings();
        }

        if matches!(self.state.assets.status(), AssetStatus::Mounting) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        self.screenshot_tick(ctx);
    }

    pub(crate) fn save_settings(&mut self) {
        if !self.persist_settings {
            self.state.settings_dirty = false;
            return;
        }
        self.state.settings.keybindings = self.state.keys.to_settings();
        if let Err(e) = self.state.settings.save(&Settings::default_path()) {
            self.state.log(Severity::Warning, tf!("app.settings_save_failed", e));
        }
        self.state.settings_dirty = false;
        self.settings_saved_at = Instant::now();
    }

    fn report_gpu_errors(&mut self) {
        let (errors, more, lost) = crate::gpu_guard::take();
        for e in errors {
            self.state.log(Severity::Error, tf!("app.gpu_error", e));
        }
        if more > 0 {
            self.state.log(Severity::Warning, tf!("app.gpu_errors_more", more));
        }
        if let Some(reason) = lost {
            self.state.log(Severity::Error, tf!("app.gpu_lost", reason));
            self.gpu_lost_exit(&reason);
        }
    }

    fn gpu_lost_exit(&mut self, reason: &str) {
        if !self.persist_settings {
            eprintln!("the graphics device was lost ({reason}): the run ends");
            std::process::exit(4);
        }
        match crash::write_device_lost_report(&Settings::crash_dir(), reason) {
            Ok(path) => eprintln!("crash report written to {}", path.display()),
            Err(e) => eprintln!("cannot write the crash report: {e}"),
        }
        self.emergency_save();
        self.save_settings();
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title(t!("app.gpu_lost_title"))
            .set_description(tf!("app.gpu_lost", reason))
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
        std::process::exit(4);
    }

    fn emergency_save(&mut self) {
        if self.emergency_saved || !self.persist_settings || !self.any_map_dirty() {
            return;
        }
        self.emergency_saved = true;
        let dir = Settings::autosave_dir();
        let settings = &self.state.settings;
        if self.state.doc.is_dirty() {
            let (doc, cameras) = (&self.state.doc, &self.state.cameras);
            let saved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                autosave::save_doc_now(doc, cameras, settings, &dir)
            }));
            match saved {
                Ok(Ok(path)) => {
                    eprintln!("emergency save written to {}", path.display());
                    if let Some(report) = crash::last_report() {
                        crash::add_emergency_save(&Settings::crash_dir(), &report, &path, self.state.doc.path());
                    }
                }
                Ok(Err(e)) => eprintln!("emergency save failed: {e}"),
                Err(_) => eprintln!("emergency save failed"),
            }
        }
        for (_, parked) in self.tabs.parked().filter(|(_, p)| p.doc.is_dirty()) {
            let saved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                autosave::save_doc_now(&parked.doc, &parked.cameras, settings, &dir)
            }));
            match saved {
                Ok(Ok(path)) => eprintln!("emergency save written to {}", path.display()),
                Ok(Err(e)) => eprintln!("emergency save failed: {e}"),
                Err(_) => eprintln!("emergency save failed"),
            }
        }
    }

    fn screenshot_tick(&mut self, ctx: &egui::Context) {
        let Some(job) = self.screenshot.as_mut() else { return };
        if job.done.load(std::sync::atomic::Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.allow_close = true;
            return;
        }
        ctx.request_repaint();
        if !job.requested && self.state.frame.index >= job.frames {
            job.requested = true;
            let path = job.path.clone();
            let done = job.done.clone();
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |image| {
                let [w, h] = image.size;
                let mut rgba = Vec::with_capacity(w * h * 4);
                for c in &image.pixels {
                    rgba.extend_from_slice(&c.to_array());
                }
                match image::save_buffer(&path, &rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8) {
                    Ok(()) => eprintln!("screenshot saved to {}", path.display()),
                    Err(e) => eprintln!("screenshot failed: {e}"),
                }
                done.store(true, std::sync::atomic::Ordering::Relaxed);
                ctx2.request_repaint();
            });
        }
    }

    fn frame(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let frame_start = Instant::now();
        let ctx = ui.ctx().clone();
        self.begin_frame(&ctx, frame);
        self.window(ui);
        self.dialogs(&ctx);
        self.route_keys(&ctx);
        self.end_frame(&ctx);
        self.run_script_command(&ctx);
        self.keep_script_running(&ctx);
        self.after_frame(&ctx, frame_start.elapsed());
    }
}

impl HammerProApp {
    fn resolve_script_pos(&self, ctx: &egui::Context, pos: &ScriptPos) -> Option<egui::Pos2> {
        let rects = self.viewports.view_rects();
        match pos {
            ScriptPos::Window(p) => Some(*p),
            ScriptPos::ViewFraction { view, x, y } => {
                let (_, rect) = rects.get(*view).copied().flatten()?;
                Some(egui::pos2(rect.left() + rect.width() * x, rect.top() + rect.height() * y))
            }
            ScriptPos::World2d { kind, x, y } => {
                let (_, rect) = rects.iter().flatten().find(|(k, _)| k == kind).copied()?;
                let view = hp_app::ViewInfo {
                    index: 0,
                    kind: *kind,
                    rect,
                    cam3d: self.state.cameras.cam3d,
                    cam2d: self.state.cameras.cam2d(*kind),
                    pixels_per_point: ctx.pixels_per_point(),
                    grid: self.state.grid_size(),
                };
                view.world_to_screen(kind.from_2d(glam::DVec2::new(*x, *y), 0.0))
            }
        }
    }

    fn inject_script_input(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let Some(steps) = self.script.as_mut() else { return };
        if let Some((until, first_frame)) = self.script_idle {
            let now = Instant::now();
            if now < until {
                ctx.request_repaint_after(until - now);
                return;
            }
            let frames = self.state.frame.index.saturating_sub(first_frame);
            eprintln!("script: idle over, {frames} frame(s) drawn meanwhile");
            self.script_idle = None;
        }
        ctx.request_repaint();
        let Some(step) = steps.pop_front() else { return };
        match step {
            Step::Idle => {}
            Step::Sleep(ms) => {
                let until = Instant::now() + std::time::Duration::from_millis(u64::from(ms));
                self.script_idle = Some((until, self.state.frame.index + 1));
                ctx.request_repaint_after(std::time::Duration::from_millis(u64::from(ms)));
            }
            Step::App(cmd) => self.script_command = Some(cmd),
            Step::Events(events) => {
                for ev in &events {
                    let pos = script::event_pos(ev).and_then(|p| self.resolve_script_pos(ctx, p));
                    raw_input.events.push(egui::Event::ModifiersChanged(script::event_mods(ev)));
                    raw_input.events.extend(script::to_event(ev, pos));
                }
            }
        }
    }

    fn keep_script_running(&self, ctx: &egui::Context) {
        if self.script.as_ref().is_none_or(|s| s.is_empty()) {
            return;
        }
        match self.script_idle {
            Some((until, _)) => ctx.request_repaint_after(until.saturating_duration_since(Instant::now())),
            None => ctx.request_repaint(),
        }
    }

    fn run_script_command(&mut self, ctx: &egui::Context) {
        let Some(cmd) = self.script_command.take() else { return };
        eprintln!("script: {cmd:?}");
        match cmd {
            Command::Action(id) => match hp_app::Action::from_id(&id) {
                Some(a) => self.state.request(a),
                None => eprintln!("script: unknown action {id}"),
            },
            Command::Tool(name) => {
                match ToolId::ALL.into_iter().find(|t| format!("{t:?}").eq_ignore_ascii_case(&name)) {
                    Some(t) => self.state.set_tool(t),
                    None => eprintln!("script: unknown tool {name}"),
                }
            }
            Command::Open(path) if self.tabs.find_map(&self.state, &path).is_some() => {
                if let Some(id) = self.tabs.find_map(&self.state, &path) {
                    self.switch_to_tab(id);
                }
            }
            Command::Open(path) => {
                let target =
                    if self.state.settings.workflow.open_in_new_tab && !crate::tabs::is_pristine(&self.state.doc) {
                        OpenTarget::NewTab
                    } else {
                        OpenTarget::Replace(self.tabs.active_id())
                    };
                self.start_open(ctx, &path, files::OpenKind::Map, target);
                if let Some(loaded) = self.opener.wait(std::time::Duration::from_secs(120)) {
                    self.finish_open(loaded);
                }
            }
            Command::Material(name) => self.state.use_material(&name),
            Command::Background(opacity, path) => {
                self.state.settings.ui.background_image = path;
                self.state.settings.ui.background_opacity = opacity;
            }
            Command::ScreenshotWindow { name, path } => match hp_app::native_window::capture_window(&name) {
                Some(Ok(image)) => {
                    let [w, h] = image.size;
                    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                    match image::save_buffer(&path, &rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8) {
                        Ok(()) => eprintln!("script: screenshot of {name} saved to {}", path.display()),
                        Err(e) => eprintln!("script: screenshot of {name} failed: {e}"),
                    }
                }
                Some(Err(e)) => eprintln!("script: screenshot of {name} failed: {e}"),
                None => eprintln!("script: {name} is not open as a window of its own"),
            },
            Command::Screenshot(path) => {
                ctx.request_screenshot(move |image| {
                    let [w, h] = image.size;
                    let mut rgba = Vec::with_capacity(w * h * 4);
                    for c in &image.pixels {
                        rgba.extend_from_slice(&c.to_array());
                    }
                    match image::save_buffer(&path, &rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8) {
                        Ok(()) => eprintln!("script: screenshot saved to {}", path.display()),
                        Err(e) => eprintln!("script: screenshot failed: {e}"),
                    }
                });
            }
            Command::Log(text) => eprintln!("script: {text}"),
            Command::Save(path) => {
                files::store_view_in_doc(&mut self.state);
                match self.state.doc.save(&path) {
                    Ok(()) => eprintln!("script: saved {}", path.display()),
                    Err(e) => eprintln!("script: save failed: {e}"),
                }
            }
            Command::CustomFolder { path, prefix } => {
                let prefix = prefix.unwrap_or_else(|| hp_fs::suggest_prefix(&path));
                self.state.settings.game.custom_texture_dirs.push(hp_app::settings::CustomDirSetting {
                    path,
                    prefix,
                    enabled: true,
                });
                self.state.assets.remount(&self.state.settings);
            }
            Command::Stats => {
                let s = self.state.doc.stats();
                eprintln!(
                    "script: stats fps={:.1} dt_ms={:.2} worst_ms={:.1} solids={} faces={} entities={} selected={} assets={:?}",
                    self.state.frame.fps,
                    self.avg_frame_time * 1000.0,
                    std::mem::take(&mut self.worst_frame) * 1000.0,
                    s.solids,
                    s.faces,
                    s.entities,
                    self.state.doc.selected_objects().len(),
                    self.state.assets.status()
                );
                if let Some(gfx) = self.state.gfx.as_ref() {
                    eprintln!("script: render {:?}", gfx.renderer.stats());
                    eprintln!("script: particles {}", crate::dispatch::lab::particle_stats(self));
                    eprintln!(
                        "script: cubemaps {:?} player_sources={:?}",
                        gfx.renderer.cubemap_stats(),
                        self.state.assets.play_cubemap_sources()
                    );
                }
                let styles = &self.state.runtime.lab.player.light_styles;
                if self.state.runtime.lab.player.active && !styles.styles.is_empty() {
                    let list: Vec<String> = styles.styles.iter().map(|(s, p, v)| format!("{s}:{v:.3}:{p}")).collect();
                    eprintln!("script: light styles t={:.3} {}", styles.time, list.join(" "));
                }
            }
            Command::History => {
                let doc = &self.state.doc;
                let mut b = hp_geom::Aabb::EMPTY;
                for o in doc.selected_objects() {
                    if let Some(ob) = doc.object_bounds(o, &|e| self.state.entity_bounds(e)) {
                        b = b.union(&ob);
                    }
                }
                eprintln!(
                    "script: history undo={:?} redo={:?} open_tx={} selection_bounds={:?}",
                    doc.undo_labels(),
                    doc.redo_labels(),
                    doc.in_transaction(),
                    (!b.is_empty()).then_some((b.min, b.max))
                );
            }
            Command::Camera { pos, yaw, pitch } => {
                let cam = &mut self.state.cameras.cam3d;
                cam.pos = pos;
                cam.yaw_deg = yaw;
                cam.pitch_deg = pitch;
            }
            Command::Player { eye, yaw, pitch } => {
                let p = &mut self.state.runtime.lab.player;
                p.feet = eye - glam::DVec3::Z * hp_ui_viewports::player::EYE;
                p.velocity = glam::DVec3::ZERO;
                p.noclip = true;
                p.crouched = false;
                p.duck = 0.0;
                let cam = &mut self.state.cameras.cam3d;
                cam.pos = eye;
                cam.yaw_deg = yaw;
                cam.pitch_deg = pitch;
            }
            Command::SelectId(id) => {
                let obj = if self.state.doc.scene().entity(id).is_some() {
                    hp_editor::ObjRef::Entity(id)
                } else {
                    hp_editor::ObjRef::Solid(id)
                };
                self.state.doc.select(&[obj], hp_editor::SelectOp::Replace);
            }
            Command::Grid(g) => self.state.settings.edit.grid_size = g,
            Command::ToolOption { key, value } => {
                let i = tool_index(self.state.tool);
                if !self.tools[i].set_option(&mut self.state, &key, &value) {
                    eprintln!("script: tool {:?} has no option {key} = {value}", self.state.tool);
                }
            }
            Command::AssertTabs(n) => {
                let got = self.tabs.len();
                if got != n {
                    eprintln!("script: ASSERTION FAILED: expected {n} open maps, got {got}");
                    std::process::exit(3);
                }
                eprintln!("script: assert ok ({n} open maps)");
            }
            Command::AssertSelected(n) => {
                let got = self.state.doc.selected_objects().len();
                if got != n {
                    eprintln!("script: ASSERTION FAILED: expected {n} selected objects, got {got}");
                    std::process::exit(3);
                }
                eprintln!("script: assert ok ({n} selected)");
            }
            Command::ImportModel { path, confirm, scale } => {
                crate::dispatch::lab::import_model_file(self, ctx, path, confirm, scale);
            }
            Command::Theme(theme) => {
                self.state.settings.ui.theme = theme;
                hp_app::theme::apply(ctx, theme, self.state.settings.ui.ui_scale);
            }
            Command::Setting { path, value } => {
                match crate::dispatch::mcp::script_setting(&mut self.state, ctx, &path, &value) {
                    Ok(()) => eprintln!("script: setting {path} = {value}"),
                    Err(e) => eprintln!("script: setting {path}: {e}"),
                }
            }
            Command::WindowSize(w, h) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            }
            Command::WindowMaximize(on) => ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(on)),
            Command::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => {}
        }
    }
}

impl eframe::App for HammerProApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.frame(ui, frame)));
        hp_app::native_window::upload_textures(ui.ctx());
        if let Err(payload) = result {
            self.emergency_save();
            std::panic::resume_unwind(payload);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.inject_script_input(ctx, raw_input);
        crate::dispatch::mcp::inject_input(self, ctx, raw_input);
        let keep_tabs = self.shell.capturing_shortcut() || self.modal_open();
        self.keys.before_frame(ctx, raw_input, keep_tabs, self.app_capture());
    }

    fn on_exit(&mut self) {
        if self.persist_settings {
            crash::end_session(&Settings::crash_dir(), std::process::id());
        }
        self.save_settings();
        files::flush_sidecar(&mut self.state);
        for (_, parked) in self.tabs.parked_mut() {
            if let Err(e) = parked.sidecar.flush() {
                log::warn!("cannot write the sidecar of {}: {e}", files::doc_display_name(&parked.doc));
            }
        }
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if !self.state.settings.ui.native_frames && hp_app::chrome::transparency_available() {
            return [0.0; 4];
        }
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}

impl Drop for HammerProApp {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.emergency_save();
        }
    }
}

pub const TR: hp_app::i18n::TrTable = &[
    ("dock.textures", "Textures", "Textures"),
    ("dock.entity", "Entity", "Entité"),
    ("dock.models", "Models", "Modèles"),
    ("dock.visgroups", "Visgroups", "Visgroups"),
    ("dock.outliner", "Outliner", "Arborescence"),
    ("menu.compiling", "Compiling...", "Compilation..."),
    ("hollow.thickness", "Wall thickness", "Épaisseur des murs"),
    ("hollow.hint", "Negative values hollow outward.", "Une valeur négative creuse vers l'extérieur."),
    ("app.need_two", "Select at least two objects", "Sélectionnez au moins deux objets"),
    ("app.need_two_solids", "Select at least two brushes", "Sélectionnez au moins deux brushes"),
    ("app.nothing_selected", "Nothing is selected", "Rien n'est sélectionné"),
    ("app.need_solids", "Select one or more brushes first", "Sélectionnez d'abord un ou plusieurs brushes"),
    ("app.paste_failed", "Nothing to paste: {0}", "Rien à coller : {0}"),
    ("app.op_failed", "{0} failed: {1}", "{0} a échoué : {1}"),
    ("app.pointfile_loaded", "Pointfile loaded: {0} points", "Pointfile chargé : {0} points"),
    ("app.pointfile_failed", "Could not read the pointfile: {0}", "Impossible de lire le pointfile : {0}"),
    ("app.pointfile_filter", "Leak pointfile", "Pointfile de fuite"),
    (
        "app.options_in_dock",
        "Tool options are in the right panel.",
        "Les options de l'outil sont dans le panneau de droite.",
    ),
    (
        "app.load_problems_one",
        "Opened {0} with 1 problem. See the console.",
        "{0} ouvert avec 1 problème. Voir la console.",
    ),
    (
        "app.load_problems_many",
        "Opened {0} with {1} problems. See the console.",
        "{0} ouvert avec {1} problèmes. Voir la console.",
    ),
    (
        "app.load_damaged",
        "{0} is damaged: part of the map is missing. It was opened as a new document: use Save as to keep it under another name. Details in the console.",
        "{0} est endommagé : une partie de la map manque. Elle a été ouverte comme un nouveau document : utilisez Enregistrer sous pour la garder sous un autre nom. Détails dans la console.",
    ),
    ("app.title_damaged", "{0} (damaged)", "{0} (endommagé)"),
    (
        "app.load.unterminated",
        "The file ends inside a quoted string: it is truncated or corrupt and part of the map is missing",
        "Le fichier se termine au milieu d'une chaîne entre guillemets : il est tronqué ou corrompu et une partie de la map manque",
    ),
    (
        "app.load.unclosed_one",
        "The file ends with 1 unclosed block: it is truncated and part of the map is missing",
        "Le fichier se termine avec 1 bloc non fermé : il est tronqué et une partie de la map manque",
    ),
    (
        "app.load.unclosed_many",
        "The file ends with {0} unclosed blocks: it is truncated and part of the map is missing",
        "Le fichier se termine avec {0} blocs non fermés : il est tronqué et une partie de la map manque",
    ),
    ("app.load.stray_one", "1 unexpected closing brace was ignored", "1 accolade fermante inattendue a été ignorée"),
    (
        "app.load.stray_many",
        "{0} unexpected closing braces were ignored",
        "{0} accolades fermantes inattendues ont été ignorées",
    ),
    ("app.load.no_value_one", "1 key without a value was ignored", "1 clé sans valeur a été ignorée"),
    ("app.load.no_value_many", "{0} keys without a value were ignored", "{0} clés sans valeur ont été ignorées"),
    (
        "app.load.unreadable_one",
        "1 value could not be read and is kept unchanged (the brush using it is invalid)",
        "1 valeur illisible est conservée telle quelle (le brush qui l'utilise est invalide)",
    ),
    (
        "app.load.unreadable_many",
        "{0} values could not be read and are kept unchanged (the brushes using them are invalid)",
        "{0} valeurs illisibles sont conservées telles quelles (les brushes qui les utilisent sont invalides)",
    ),
    ("app.load.examples", "{0}: {1}", "{0} : {1}"),
    (
        "app.load.no_id_one",
        "1 object had no valid id and got a new one",
        "1 objet n'avait pas d'identifiant valide et en a reçu un nouveau",
    ),
    (
        "app.load.no_id_many",
        "{0} objects had no valid id and got a new one",
        "{0} objets n'avaient pas d'identifiant valide et en ont reçu un nouveau",
    ),
    (
        "app.load.damaged_note",
        "The map was opened as a new document so that saving does not overwrite the damaged file: use Save as",
        "La map a été ouverte comme un nouveau document pour que l'enregistrement n'écrase pas le fichier endommagé : utilisez Enregistrer sous",
    ),
    ("app.imported_one", "Imported 1 object from {0}", "1 objet importé depuis {0}"),
    ("app.imported_many", "Imported {0} objects from {1}", "{0} objets importés depuis {1}"),
    ("app.import_empty", "{0} has no objects to import", "{0} ne contient aucun objet à importer"),
    ("app.import_failed", "Could not import {0}: {1}", "Impossible d'importer {0} : {1}"),
    (
        "app.recovered",
        "Autosave {0} opened. Save to keep it.",
        "Sauvegarde automatique {0} ouverte. Enregistrez pour la conserver.",
    ),
    ("app.recover_open", "Open the autosave", "Ouvrir la sauvegarde automatique"),
    ("app.recover_keep", "Keep the saved map", "Garder la map enregistrée"),
    ("app.recover_detail", "Autosave: {0}", "Sauvegarde automatique : {0}"),
    ("app.autosave_folder", "Open the autosave folder", "Ouvrir le dossier des sauvegardes automatiques"),
    ("app.crash_title", "Hammer Pro closed unexpectedly", "Hammer Pro s'est fermé de façon inattendue"),
    (
        "app.crash_body",
        "A crash report was saved. To report the problem, send this file (or copy its text into your message).",
        "Un rapport de plantage a été enregistré. Pour signaler le problème, envoyez ce fichier (ou copiez son texte dans votre message).",
    ),
    ("app.crash_report_label", "Report file:", "Fichier du rapport :"),
    (
        "app.crash_saved",
        "Your unsaved changes were saved to {0}.",
        "Vos modifications non enregistrées ont été sauvegardées dans {0}.",
    ),
    ("app.crash_open_save", "Open the saved changes", "Ouvrir les modifications sauvegardées"),
    ("app.crash_open_folder", "Show the report file", "Afficher le fichier du rapport"),
    (
        "app.crash_open_folder_tip",
        "Open its folder with the report selected",
        "Ouvre son dossier avec le rapport sélectionné",
    ),
    ("app.crash_copy", "Copy the report", "Copier le rapport"),
    (
        "app.crash_copy_tip",
        "Copy the text of the report, to paste it in a message",
        "Copie le texte du rapport, pour le coller dans un message",
    ),
    ("app.crash_copied", "Report copied to the clipboard.", "Rapport copié dans le presse-papiers."),
    (
        "app.crash_copy_failed",
        "The report file cannot be read (moved or deleted?).",
        "Le fichier du rapport est illisible (déplacé ou supprimé ?).",
    ),
    ("app.gpu_error", "Graphics error (one frame was skipped): {0}", "Erreur graphique (une image a été sautée) : {0}"),
    (
        "app.gpu_errors_more",
        "{0} more graphics errors (see the log file)",
        "{0} autres erreurs graphiques (voir le fichier journal)",
    ),
    (
        "app.gpu_lost",
        "The graphics device was lost ({0}), usually a reset of the graphics driver. Hammer Pro cannot draw any more and closes: your modified maps were saved, start it again to open them.",
        "Le périphérique graphique a été perdu ({0}), en général une réinitialisation du pilote graphique. Hammer Pro ne peut plus dessiner et se ferme : vos maps modifiées ont été sauvegardées, relancez-le pour les ouvrir.",
    ),
    ("app.gpu_lost_title", "Graphics device lost", "Périphérique graphique perdu"),
    ("app.compile_exit_title", "A compile is running", "Une compilation est en cours"),
    (
        "app.compile_exit_body",
        "Quitting stops the compile. Quit anyway?",
        "Quitter arrête la compilation. Quitter quand même ?",
    ),
    ("app.compile_exit_quit", "Stop and quit", "Arrêter et quitter"),
    (
        "app.content_ready",
        "Game content ready: {0} materials, {1} models",
        "Contenu du jeu prêt : {0} matériaux, {1} modèles",
    ),
    ("app.fgd_loaded", "Entity definitions loaded: {0} classes", "Définitions d'entités chargées : {0} classes"),
    ("app.started", "Hammer Pro started in {0} ms", "Hammer Pro a démarré en {0} ms"),
    ("app.map_filter", "Quake style map (Valve 220)", "Map de style Quake (Valve 220)"),
    (
        "app.map_imported",
        "Imported from a .map file (brushes: {0}, entities: {1})",
        "Importé d'un fichier .map (brushes : {0}, entités : {1})",
    ),
    (
        "app.map_skipped",
        "{0} blocks or faces of the .map file could not be used and were skipped",
        "{0} blocs ou faces du fichier .map étaient inutilisables et ont été ignorés",
    ),
    (
        "app.map_save_as",
        "Imported map: Save as to keep it as a VMF",
        "Map importée : utilisez Enregistrer sous pour la garder en VMF",
    ),
    ("app.settings_save_failed", "Could not save the settings: {0}", "Impossible d'enregistrer les paramètres : {0}"),
    (
        "app.sidecar_failed",
        "Could not save the map editor data: {0}",
        "Impossible d'enregistrer les données d'édition de la map : {0}",
    ),
    (
        "app.err.empty_clipboard",
        "the clipboard does not contain map objects",
        "le presse-papiers ne contient aucun objet de map",
    ),
    ("app.err.not_found", "the object no longer exists", "l'objet n'existe plus"),
    ("app.err.group_not_found", "group {0} not found", "groupe {0} introuvable"),
    ("app.err.visgroup_not_found", "visgroup {0} not found", "visgroup {0} introuvable"),
    ("app.err.invalid", "invalid value ({0})", "valeur invalide ({0})"),
    ("app.err.nothing", "nothing to do ({0})", "rien à faire ({0})"),
    ("app.err.busy", "another edit is in progress", "une autre modification est en cours"),
    ("app.err.no_path", "the map has not been saved yet", "la map n'a pas encore été enregistrée"),
];

#[cfg(test)]
mod tests {
    #[test]
    fn binary_table_is_clean() {
        assert!(hp_app::i18n::lint_table(super::TR).is_empty());
    }

    #[test]
    fn every_text_is_registered_once_without_doubled_words() {
        super::register_all_translations();
        let conflicts = hp_app::i18n::conflicting_keys();
        assert!(conflicts.is_empty(), "keys registered twice with different texts: {conflicts:?}");
        let doubled: Vec<String> = hp_app::i18n::entries()
            .into_iter()
            .filter_map(|(k, en, fr)| {
                hp_app::i18n::doubled_word(en).or_else(|| hp_app::i18n::doubled_word(fr)).map(|w| format!("{k}: {w}"))
            })
            .collect();
        assert!(doubled.is_empty(), "doubled words: {doubled:#?}");
    }

    fn tool_frame(ctx: &egui::Context, tools: &mut [Box<dyn super::Tool>], state: &mut super::AppState) {
        let i = super::tool_index(state.tool);
        state.frame.index += 1;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::Panel::right("hp_dock").show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("hp_tool_panel").show(ui, |ui| tools[i].options_ui(ui, state));
            });
        });
        out.textures_delta.clear();
        let view = hp_app::view::ViewInfo {
            index: 0,
            kind: hp_app::ViewKind::Perspective,
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            cam3d: hp_app::view::Camera3d::default(),
            cam2d: hp_app::view::Camera2d::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        };
        for view in [view, hp_app::view::ViewInfo { index: 1, kind: hp_app::ViewKind::Top, ..view }] {
            let mut overlay = hp_render::Overlay::default();
            tools[i].overlay(state, &view, &mut overlay);
            let _ = tools[i].retained_overlay_version(state, &view);
            tools[i].retained_overlay(state, &view, &mut overlay);
            let _ = tools[i].cursor(state, &view, view.rect.center());
        }
        let _ = tools[i].status_hint(state);
    }

    fn switch(tools: &mut [Box<dyn super::Tool>], state: &mut super::AppState, id: super::ToolId) {
        if id == state.tool {
            return;
        }
        tools[super::tool_index(state.tool)].deactivate(state);
        state.tool = id;
        tools[super::tool_index(id)].activate(state);
    }

    #[test]
    fn every_tool_switches_with_the_uv_editor_in_its_window() {
        use hp_app::ToolId;
        use hp_editor::{ObjRef, SelectOp};
        super::register_all_translations();
        let (mut tools, ids) = super::make_tools();
        assert!(ids.contains(&ToolId::Texture) && ids.contains(&ToolId::Displacement));
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        state.doc.select(&[ObjRef::Solid(2)], SelectOp::Replace);
        switch(&mut tools, &mut state, ToolId::Texture);
        assert!(!state.doc.selected_faces().is_empty(), "the solid's faces are selected");
        let texture = super::tool_index(ToolId::Texture);
        for round in 0..4 {
            assert!(tools[texture].set_option(&mut state, "uv_place", "window"));
            for _ in 0..3 {
                tool_frame(&ctx, &mut tools, &mut state);
            }
            for &id in &ids {
                switch(&mut tools, &mut state, id);
                if id == ToolId::Displacement && round % 2 == 1 {
                    state.doc.select(&[ObjRef::Solid(2)], SelectOp::Replace);
                    tools[super::tool_index(id)].set_option(&mut state, "create", "3");
                }
                for _ in 0..3 {
                    tool_frame(&ctx, &mut tools, &mut state);
                }
                let _ = tools[super::tool_index(id)].cancel(&mut state);
                tool_frame(&ctx, &mut tools, &mut state);
                switch(&mut tools, &mut state, ToolId::Texture);
                tool_frame(&ctx, &mut tools, &mut state);
            }
            match round {
                0 => {
                    let faces = state.doc.selected_faces();
                    let _ = state.doc.create_displacements(&faces, 2);
                }
                1 => {
                    state.doc.undo();
                    state.doc.undo();
                    state.doc.redo();
                }
                _ => state.doc = hp_editor::Document::default(),
            }
            tool_frame(&ctx, &mut tools, &mut state);
        }
        assert!(tools[texture].set_option(&mut state, "uv_place", "panel"));
        tool_frame(&ctx, &mut tools, &mut state);
        switch(&mut tools, &mut state, ToolId::Displacement);
        tool_frame(&ctx, &mut tools, &mut state);
    }

    #[test]
    fn french_punctuation_has_a_space() {
        for &(key, _, fr) in super::TR {
            for (i, c) in fr.char_indices() {
                if matches!(c, ':' | '?' | '!') && i > 0 {
                    let before = fr[..i].chars().last();
                    assert!(before.is_some_and(char::is_whitespace), "{key}: {fr}");
                }
            }
        }
    }
}
