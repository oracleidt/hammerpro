mod background;
mod batch;
mod compilers;
mod compilers_ui;
mod devaddon;
mod expert;
mod history_ui;
mod launch;
mod mcp_view;
mod notices;
mod options_ui;
mod plan;
mod progressive;
mod run;
mod settings_ui;
mod snapshot;
mod stats_ui;
mod target_ui;
mod tools;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Sense, Ui};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::runtime::{CompileBadge, CompileProgress};
use hp_app::settings::{CompilePresetSetting, Settings};
use hp_app::state::{AssetStatus, ModelState, Pointfile};
use hp_app::{Action, AppState, Severity, t, tf, theme};
use hp_compile::history::{self, HistoryEntry, StepTime};
use hp_compile::knowledge::Fix;
use hp_compile::{CompileEvent, CompileHandle, LogLocation, Severity as LogSeverity, StepKind};
use hp_editor::compile_export::{CompileExport, VisibilityExport};
use hp_editor::{FaceRef, Id, ObjRef, SelectOp};

pub(crate) use notices::notice_text;
pub use compilers_ui::{
    compiler_commands, compiler_list, compiler_name, compiler_problems, compiler_tools,
    settings_page_ui as compilers_settings_ui, valve_flag_text, valve_group_text, valve_managed_text,
};
pub use options_ui::{compile_options_ui, effective_step_flags};
pub use plan::content_args as compile_content_args;
pub(crate) use plan::{PrepInput, prepare};
pub(crate) use run::{Effect, LogView, RunState, StepStatus, Summary};
pub(crate) use snapshot::{CompileSidecar, PrepError, Prepared, SnapshotInput, SnapshotOutput, Target};

use crate::native_window;
use crate::util::{
    center_views_on, enter_pressed, escape_pressed, fmt_duration_ms, icon_title, plural, select_and_center,
};

pub fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("hp_compile_viewport")
}

fn place_switch(ui: &mut Ui, state: &mut AppState, separate: bool) {
    if ui.ctx().embed_viewports() && !separate {
        return;
    }
    if separate && !state.settings.ui.native_frames {
        return;
    }
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (icon, label, tip) = if separate {
                ("⊟", t!("compile.attach"), t!("compile.attach_tip"))
            } else {
                ("🗗", t!("compile.detach"), t!("compile.detach_tip"))
            };
            if ui.button(format!("{icon}  {label}")).on_hover_text(tip).clicked() {
                state.settings.compile.separate_window = !separate;
                state.settings_dirty = true;
            }
        });
    });
}

pub(crate) fn window_id() -> egui::Id {
    egui::Id::unique("hp_compile_window")
}

pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        let s = abs.to_string_lossy().replace('\\', "/");
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(a) == norm(b)
}

const MODEL_WAIT: Duration = Duration::from_secs(10);
const ASSET_WAIT: Duration = Duration::from_secs(120);
const SAVE_WAIT: Duration = Duration::from_secs(600);
const EVENTS_PER_FRAME: usize = 20_000;

pub(crate) fn step_name(kind: StepKind) -> &'static str {
    match kind {
        StepKind::Bsp => t!("compile.step.bsp"),
        StepKind::Vis => t!("compile.visibility"),
        StepKind::Rad => t!("compile.step.rad"),
        StepKind::Pack => t!("compile.step.pack"),
        StepKind::CopyToMaps => t!("compile.step.copy"),
        StepKind::LaunchGame => t!("compile.step.launch"),
        StepKind::Custom => t!("compile.step.custom"),
    }
}

fn log_color(s: LogSeverity) -> Color32 {
    match s {
        LogSeverity::Info => theme::info(),
        LogSeverity::Success => theme::success(),
        LogSeverity::Warning => theme::warning(),
        LogSeverity::Error => theme::error(),
    }
}

pub(crate) fn load_pointfile(state: &mut AppState, path: &Path) -> bool {
    match hp_compile::read_pointfile(path) {
        Ok(points) if !points.is_empty() => {
            let points: Vec<DVec3> = points.into_iter().map(DVec3::from_array).collect();
            let cursor = crate::dialogs::leak_entity_index(points.len());
            state.pointfile = Some(Pointfile { path: path.to_path_buf(), points, cursor });
            state.notify(Severity::Warning, t!("compile.leak_loaded"));
            true
        }
        Ok(_) => {
            state.notify(Severity::Warning, tf!("compile.leak_empty", path.display()));
            false
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("compile.leak_load_failed", e));
            false
        }
    }
}

pub(crate) fn leaking_entity(state: &AppState, line: Option<&LogLocation>, last_point: Option<DVec3>) -> Option<Id> {
    let scene = state.doc.scene();
    let near = |pos: Option<DVec3>, filter: &dyn Fn(&hp_vmf::Entity) -> bool| {
        scene
            .entities()
            .values()
            .filter(|n| filter(&n.entity) && state.doc.is_entity_visible(n.entity.id))
            .filter_map(|n| {
                let o = n.entity.origin()?;
                Some((n.entity.id, pos.map_or(0.0, |p| p.distance_squared(o))))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    };
    if let Some(class) = line.and_then(|l| l.entity_class.as_deref()) {
        let pos = line.and_then(|l| l.position).map(DVec3::from_array).or(last_point);
        if let Some(id) = near(pos, &|e| e.classname.eq_ignore_ascii_case(class)) {
            return Some(id);
        }
    }
    let pos = line.and_then(|l| l.position).map(DVec3::from_array).or(last_point)?;
    near(Some(pos), &|_| true)
}

pub(crate) fn goto_location(state: &mut AppState, loc: &LogLocation) -> bool {
    let scene = state.doc.scene();
    if let Some(b) = loc.brush_id
        && scene.solid(b).is_some()
    {
        select_and_center(state, &[ObjRef::Solid(b)]);
        return true;
    }
    if let Some(side) = loc.side_id {
        let face = scene
            .solids()
            .values()
            .find_map(|n| n.solid.sides.iter().position(|s| s.id == side).map(|i| FaceRef::new(n.solid.id, i)));
        if let Some(f) = face {
            state.doc.set_selection(&[ObjRef::Solid(f.solid)]);
            state.doc.select_faces(&[f], SelectOp::Replace);
            crate::util::request_center(state);
            return true;
        }
    }
    if let Some(n) = loc.entity_id {
        let scene = state.doc.scene();
        let by_index = (n as usize).checked_sub(1).and_then(|i| scene.entities().get_index(i)).map(|(id, _)| *id);
        let found = by_index.or_else(|| scene.entity(n).map(|e| e.entity.id));
        if let Some(id) = found {
            select_and_center(state, &[ObjRef::Entity(id)]);
            return true;
        }
    }
    if let Some(class) = &loc.entity_class {
        let pos = loc.position.map(DVec3::from_array);
        let scene = state.doc.scene();
        let best = scene
            .entities()
            .values()
            .filter(|n| n.entity.classname.eq_ignore_ascii_case(class))
            .min_by(|a, b| {
                let d = |e: &hp_vmf::Entity| match (pos, e.origin()) {
                    (Some(p), Some(o)) => p.distance_squared(o),
                    _ => 0.0,
                };
                d(&a.entity).total_cmp(&d(&b.entity))
            })
            .map(|n| n.entity.id);
        if let Some(id) = best {
            select_and_center(state, &[ObjRef::Entity(id)]);
            return true;
        }
    }
    if let Some(p) = loc.position {
        center_views_on(state, DVec3::from_array(p));
        return true;
    }
    false
}

fn location_text(loc: &LogLocation) -> String {
    if let Some(b) = loc.brush_id {
        return tf!("compile.goto_brush", b);
    }
    if let Some(s) = loc.side_id {
        return tf!("compile.goto_side", s);
    }
    if let Some(e) = loc.entity_id {
        return tf!("compile.goto_entity", e);
    }
    if let Some(c) = &loc.entity_class {
        return tf!("compile.goto_entity", c);
    }
    if let Some(p) = loc.position {
        return tf!("compile.goto_position", hp_app::widgets::fmt_vec3(DVec3::from_array(p)));
    }
    String::new()
}

enum Pending {
    SaveAs { since: Instant },
    Assets { input: Box<SnapshotInput>, since: Instant },
}

#[derive(Clone, Debug, PartialEq)]
struct ProbeKey {
    root: Option<PathBuf>,
    game_exe: Option<PathBuf>,
    maps_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct ProbeResult {
    root: Option<PathBuf>,
    resolved: Option<tools::Resolved>,
    error: Option<String>,
    missing: Vec<PathBuf>,
}

#[derive(Default)]
struct ToolProbe {
    key: Option<ProbeKey>,
    rx: Option<Receiver<ProbeResult>>,
    result: Option<ProbeResult>,
    serial: u64,
}

impl ToolProbe {
    fn update(&mut self, ctx: &egui::Context, state: &AppState) {
        let s = &state.settings.compile;
        let key = ProbeKey {
            root: game_root(state),
            game_exe: s.game_exe_path.clone(),
            maps_dir: s.maps_dir_override.clone(),
        };
        if self.key.as_ref() != Some(&key) && self.rx.is_none() {
            let settings = s.clone();
            let (tx, rx) = mpsc::channel();
            let ctx = ctx.clone();
            let k = key.clone();
            let spawned = std::thread::Builder::new().name("hp-compile-probe".into()).spawn(move || {
                let root = k.root.or_else(hp_fs::detect_gmod_root);
                let (resolved, error) = match &root {
                    Some(r) => match tools::resolve(r, &settings) {
                        Ok(res) => (Some(res), None),
                        Err(e) => (None, Some(e)),
                    },
                    None => (None, None),
                };
                let missing = resolved.as_ref().map(|r| r.tools.missing_files()).unwrap_or_default();
                let _ = tx.send(ProbeResult { root, resolved, error, missing });
                ctx.request_repaint();
            });
            self.rx = spawned.ok().map(|_| rx);
            self.key = Some(key);
        }
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.result = Some(r);
                    self.rx = None;
                    self.serial += 1;
                }
                Err(TryRecvError::Disconnected) => self.rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }
}

pub(crate) fn game_root(state: &AppState) -> Option<PathBuf> {
    if !state.settings.game.is_gmod()
        && let Some(dir) = state.assets.fs().and_then(|fs| fs.game_dir().map(Path::to_path_buf))
        && !dir.join("..").join("garrysmod").join("gameinfo.txt").is_file()
        && !dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("garrysmod"))
    {
        return Some(dir);
    }
    state.assets.fs().and_then(|fs| fs.root().map(Path::to_path_buf)).or_else(|| state.settings.game.gmod_root.clone())
}

struct Current {
    target: Target,
    record: hp_compile::incremental::BuildRecord,
    choice: Option<hp_compile::incremental::Choice>,
    modes: hp_compile::StepModes,
    preset: String,
    bsp: PathBuf,
    history_file: Option<PathBuf>,
    release: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Log,
    Stats,
    History,
    Expert,
    Compilers,
}

#[derive(Default)]
pub struct CompileDialog {
    run: Option<RunState>,
    run_id: u64,
    job: Option<CompileHandle>,
    pending: Option<Pending>,
    prep: Option<Receiver<SnapshotOutput>>,
    current: Option<Current>,
    pack_wait: Option<(snapshot::PackJob, Vec<String>, Instant)>,
    log_view: LogView,
    probe: ToolProbe,
    browse: Option<(settings_ui::BrowseTarget, Receiver<Option<PathBuf>>)>,
    no_auto_scroll: bool,
    collapse_settings: bool,
    tab: Tab,
    history: history_ui::HistoryView,
    background: background::Background,
    preview: settings_ui::PreviewCache,
    open_on_tab: Option<Tab>,
    batch: batch::BatchDialog,
    progressive: progressive::Progressive,
    ctx: Option<egui::Context>,
    expert: expert::ExpertView,
    external_name: Option<String>,
    stats: stats_ui::StatsView,
    native: native_window::NativeWindow,
    os_builder: Option<egui::ViewportBuilder>,
}

impl CompileDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.job.is_some() || self.prep.is_some() || matches!(self.pending, Some(Pending::Assets { .. }))
    }

    fn waiting_for_save(&self) -> bool {
        matches!(self.pending, Some(Pending::SaveAs { .. }))
    }

    pub fn progress(&self) -> Option<f32> {
        if !self.is_running() {
            return None;
        }
        Some(self.run.as_ref().map_or(0.0, RunState::overall_progress))
    }

    pub fn cancel_compile(&mut self, state: &mut AppState) {
        if self.is_running() || self.pending.is_some() {
            self.cancel(state);
        }
        self.background.cancel();
        self.progressive.cancel();
    }

    pub fn start_now(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.is_running() || self.pending.is_some() {
            state.notify(Severity::Info, t!("compile.already_running"));
            return;
        }
        self.start(ctx, state);
    }

    fn compiled_map_is_open(&self, state: &AppState) -> bool {
        let Some(run) = &self.run else { return false };
        if let Some(serial) = run.doc_serial {
            return serial == state.sidecar.serial();
        }
        match (run.vmf.as_deref(), state.doc.path()) {
            (Some(c), Some(open)) => same_file(c, open),
            _ => false,
        }
    }

    pub fn batch_running(&self) -> bool {
        self.batch.is_running()
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.batch.show(ctx, state);
        self.poll(ctx, state);
        let busy = self.is_running() || self.pending.is_some();
        self.background.tick(ctx, state, busy);
        self.publish_runtime(state);
        if busy {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if state.ui.show_compile {
            self.probe.update(ctx, state);
        }
        if state.runtime.compile.show_history {
            state.runtime.compile.show_history = false;
            state.ui.show_compile = true;
            self.open_on_tab = Some(Tab::History);
        }
        if let Some(b) = state.runtime.compile.browse_request.take() {
            use hp_app::runtime::CompileBrowse;
            self.browse(
                ctx,
                match b {
                    CompileBrowse::GameExe => settings_ui::BrowseTarget::GameExe,
                    CompileBrowse::MapsDir => settings_ui::BrowseTarget::MapsDir,
                },
            );
        }
        self.poll_browse(state);
        if !state.ui.show_compile {
            if self.waiting_for_save() {
                self.cancel(state);
            }
            return;
        }
        if let Some(tab) = self.open_on_tab.take() {
            self.tab = tab;
        }
        self.preview.compiles = self.run_id;
        self.preview.update(ctx, state, &self.probe, busy);

        let open = if state.settings.compile.separate_window && !ctx.embed_viewports() {
            self.separate_window(ctx, state)
        } else {
            self.native.forget();
            self.os_builder = None;
            self.embedded_window(ctx, state)
        };
        if !open {
            self.native.forget();
            self.os_builder = None;
            state.ui.show_compile = false;
            if self.waiting_for_save() {
                self.cancel(state);
            }
        }
    }

    fn embedded_window(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let typing = ctx.text_edit_focused();
        let mut open = true;
        let response = hp_app::chrome::Window::new(icon_title("⚙", t!("compile.title")))
            .id(window_id())
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(900.0, 680.0)))
            .open(&mut open)
            .default_size([900.0, 680.0])
            .min_size([600.0, 440.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                place_switch(ui, state, false);
                self.window_ui(ui, state)
            });
        if let Some(r) = &response {
            if escape_pressed(ctx, &r.response) {
                open = false;
            } else if typing || ctx.text_edit_focused() || self.is_running() || self.pending.is_some() {
                let _ = enter_pressed(ctx, &r.response);
            } else if enter_pressed(ctx, &r.response) {
                self.start(ctx, state);
            }
        }
        open
    }

    fn separate_window(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let native = state.settings.ui.native_frames;
        let builder = self.os_builder.get_or_insert_with(|| {
            native_window::builder(
                ctx,
                t!("compile.title"),
                egui::vec2(900.0, 680.0),
                egui::vec2(600.0, 440.0),
                state.settings.compile.window.as_ref(),
                native,
            )
        });
        *builder = hp_app::chrome::builder(builder.clone(), native);
        let builder = builder.clone();
        let title = builder.title.clone().unwrap_or_default();
        let mut open = true;
        ctx.show_viewport_immediate(viewport_id(), builder, |ui, _class| {
            let ctx = ui.ctx().clone();
            let restore = state.settings.compile.window.clone();
            if let Some(now) = self.native.follow(&ctx, &title, restore.as_ref(), Some(native_window::COMPILE_WINDOW))
                && restore.as_ref() != Some(&now)
            {
                state.settings.compile.window = Some(now);
                state.settings_dirty = true;
            }
            let typing = ctx.text_edit_focused();
            let spec = hp_app::chrome::ChromeSpec::dialog(t!("compile.title"), native);
            let ((), frame) = hp_app::chrome::dialog(ui, spec, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    place_switch(ui, state, true);
                    self.window_ui(ui, state);
                });
            });
            match frame.clicked {
                Some(hp_app::chrome::ChromeButton::Place) => {
                    state.settings.compile.separate_window = false;
                    state.settings_dirty = true;
                }
                Some(hp_app::chrome::ChromeButton::Close) => open = false,
                _ => {}
            }
            if !open || ctx.input(|i| i.viewport().close_requested()) || crate::util::viewport_escape_pressed(&ctx) {
                open = false;
            } else if typing || ctx.text_edit_focused() || self.is_running() || self.pending.is_some() {
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                self.start(&ctx, state);
            }
        });
        open
    }

    fn publish_runtime(&self, state: &mut AppState) {
        let progress = if self.is_running() || self.pending.is_some() {
            let run = self.run.as_ref();
            let step = run
                .and_then(|r| r.steps.iter().find(|s| s.status == StepStatus::Running))
                .map(|s| step_name(s.kind).to_owned())
                .unwrap_or_else(|| self.status_text());
            Some(CompileProgress {
                step,
                fraction: run.map_or(0.0, RunState::overall_progress),
                elapsed_s: run.map_or(0.0, |r| r.started.elapsed().as_secs_f32()),
            })
        } else {
            None
        };
        state.runtime.compile.progress = progress;
        state.runtime.compile.background = self.progressive.progress().or_else(|| self.background.progress());
    }

    fn export_of(
        s: &hp_app::settings::CompileSettings,
        visibility: &hp_app::settings::VisibilitySettings,
    ) -> CompileExport {
        CompileExport {
            visibility: if s.compile_hidden {
                VisibilityExport::Everything
            } else {
                VisibilityExport::HammerCompatible
            },
            cordons: s.compile_cordon_only,
            seal_material: visibility.cordon_material().to_owned(),
        }
    }

    pub(crate) fn start(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.is_running() || self.pending.is_some() {
            return;
        }
        self.background.cancel();
        self.progressive.cancel();
        self.run_id += 1;
        self.collapse_settings = true;
        self.tab = Tab::Log;
        let mut run = RunState::preparing(&state.map_title());
        run.doc_serial = Some(state.sidecar.serial());
        self.run = Some(run);
        let s = &state.settings.compile;
        if s.save_before_compile {
            if state.doc.path().is_none() {
                self.log(None, LogSeverity::Info, t!("compile.waiting_save_as").to_owned());
                state.request(Action::SaveMapAs);
                self.pending = Some(Pending::SaveAs { since: Instant::now() });
                return;
            }
            if state.doc.is_dirty() {
                state.request(Action::SaveMap);
            }
        }
        self.collect(ctx, state);
    }

    pub(crate) fn cancel(&mut self, state: &mut AppState) {
        if let Some(job) = &self.job {
            job.cancel();
            return;
        }
        if self.prep.take().is_some() || self.pending.take().is_some() {
            self.log(None, LogSeverity::Warning, t!("compile.cancelled").to_owned());
            if let Some(run) = &mut self.run {
                run.abort(true);
            }
            self.current = None;
            state.log(Severity::Warning, t!("compile.cancelled"));
        }
    }

    fn log(&mut self, step: Option<usize>, severity: LogSeverity, text: String) {
        if let Some(run) = &mut self.run {
            run.push_log(step, severity, text, None);
        }
    }

    fn fail(&mut self, state: &mut AppState, message: String) {
        self.log(None, LogSeverity::Error, message.clone());
        if let Some(run) = &mut self.run {
            run.abort(false);
        }
        self.pending = None;
        self.prep = None;
        self.current = None;
        state.runtime.compile.badge = Some(CompileBadge::Failed);
        state.notify(Severity::Error, message);
    }

    fn collect(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let mut s = state.settings.compile.clone();
        let runs_map = s.launch_game;
        let deferred_launch = launch::launches_itself(&state.settings);
        if deferred_launch {
            s.launch_game = false;
        }
        let launch_flags = if s.launch_game { launch::launch_flags(state, false) } else { Vec::new() };
        let t0 = Instant::now();
        let snapshot = state.doc.compile_snapshot(&Self::export_of(&s, &state.settings.visibility));
        let snapshot_ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.background.note_snapshot_cost(snapshot_ms);
        let target = match snapshot::target_for(
            state.doc.path(),
            snapshot.is_cordoned(),
            &Settings::cache_dir().join("untitled"),
        ) {
            Ok(t) => t,
            Err(e) => {
                self.fail(state, e);
                return;
            }
        };
        if let Some(run) = &mut self.run {
            run.map_name.clone_from(&target.name);
            run.vmf = Some(target.vmf());
        }
        state.runtime.launch.compile_launch_deferred = deferred_launch;
        state.runtime.launch.compile_launch_map = runs_map.then(|| target.name.clone());
        let previous = state.sidecar.get::<CompileSidecar>().record(&target.name);
        let key = hp_app::sidecar::map_key(target.source.as_deref());
        let input = SnapshotInput {
            history_file: s.history.then(|| history_file(&key)),
            snapshot,
            target,
            settings: s,
            gmod_root: game_root(state),
            fs: state.assets.fs().cloned(),
            mounting: state.assets.status() == AssetStatus::Mounting,
            previous,
            cache_dir: Settings::cache_dir().join("pack"),
            content_dir: Settings::cache_dir().join("compile_content"),
            builds_dir: Some(Settings::cache_dir().join("builds")),
            release: false,
            background: false,
            dry_run: false,
            snapshot_ms,
            launch_flags,
        };
        self.spawn_prep(ctx, state, input);
    }

    fn spawn_prep(&mut self, ctx: &egui::Context, state: &mut AppState, input: SnapshotInput) {
        self.log(None, LogSeverity::Info, t!("compile.preparing").to_owned());
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-compile-prep".into()).spawn(move || {
            let out = snapshot::prepare_snapshot(input);
            let _ = tx.send(out);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.prep = Some(rx),
            Err(e) => self.fail(state, tf!("compile.err_thread", e)),
        }
    }

    fn poll(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.ctx.is_none() {
            self.ctx = Some(ctx.clone());
        }
        match self.pending.take() {
            Some(Pending::SaveAs { since }) => {
                if state.doc.path().is_some() && !state.doc.is_dirty() {
                    self.collect(ctx, state);
                } else if since.elapsed() > SAVE_WAIT {
                    self.fail(state, t!("compile.err_save_failed").to_owned());
                } else {
                    self.pending = Some(Pending::SaveAs { since });
                }
            }
            Some(Pending::Assets { mut input, since }) => {
                if state.assets.status() != AssetStatus::Mounting || since.elapsed() > ASSET_WAIT {
                    input.mounting = false;
                    input.fs = state.assets.fs().cloned();
                    self.spawn_prep(ctx, state, *input);
                } else {
                    self.pending = Some(Pending::Assets { input, since });
                }
            }
            None => {}
        }

        if let Some(rx) = &self.prep {
            match rx.try_recv() {
                Ok(out) => {
                    self.prep = None;
                    self.on_prepared(ctx, state, out);
                }
                Err(TryRecvError::Disconnected) => {
                    self.prep = None;
                    self.fail(state, t!("compile.err_prep_crashed").to_owned());
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        self.poll_pack_wait(state);
        self.history.poll();
        self.progressive.poll(state);

        if let Some(job) = &self.job {
            let mut events = Vec::new();
            let mut disconnected = false;
            loop {
                match job.events.try_recv() {
                    Ok(ev) => {
                        events.push(ev);
                        if events.len() >= EVENTS_PER_FRAME {
                            ctx.request_repaint();
                            break;
                        }
                    }
                    Err(e) => {
                        disconnected = e.is_disconnected();
                        break;
                    }
                }
            }
            for ev in events {
                self.handle_event(ev, state);
            }
            let finished = self.run.as_ref().is_some_and(|r| r.summary.is_some());
            if finished {
                self.job = None;
            } else if disconnected {
                self.job = None;
                self.fail(state, t!("compile.err_prep_crashed").to_owned());
            }
        }
    }

    fn poll_pack_wait(&mut self, state: &mut AppState) {
        let Some((_, models, since)) = &self.pack_wait else { return };
        let loading = models.iter().any(|m| matches!(state.assets.model_info(m), ModelState::Loading));
        if loading && since.elapsed() < MODEL_WAIT {
            return;
        }
        let Some((mut job, models, _)) = self.pack_wait.take() else { return };
        for m in &models {
            if let ModelState::Ready(info) = state.assets.model_info(m) {
                job.materials.extend(info.materials.iter().map(|x| crate::util::normalize_material(x)));
            }
        }
        job.materials.sort_unstable();
        job.materials.dedup();
        let _ = std::thread::Builder::new().name("hp-compile-pack".into()).spawn(move || {
            snapshot::run_pack_job(job);
        });
    }

    fn on_prepared(&mut self, ctx: &egui::Context, state: &mut AppState, out: SnapshotOutput) {
        for (sev, msg) in out.messages {
            self.log(None, sev, msg);
        }
        match out.result {
            Ok(p) => self.launch_prepared(ctx, state, p),
            Err(PrepError::NeedsMount(input)) => {
                self.log(None, LogSeverity::Info, t!("compile.waiting_assets").to_owned());
                self.pending = Some(Pending::Assets { input, since: Instant::now() });
            }
            Err(PrepError::Failed(e)) => self.fail(state, e),
        }
    }

    fn launch_prepared(&mut self, ctx: &egui::Context, state: &mut AppState, p: Prepared) {
        let Prepared { plan, workdir, target, record, choice, pack, release } = p;
        let key = hp_app::sidecar::map_key(target.source.as_deref());
        if let Some(run) = &mut self.run {
            run.set_plan(&plan);
        }
        self.current = Some(Current {
            bsp: plan.bsp_path(),
            modes: plan.modes,
            preset: format!("{:?}", state.settings.compile.preset),
            history_file: state.settings.compile.history.then(|| history_file(&key)),
            target,
            record,
            choice,
            release,
        });
        if let Some(job) = pack {
            let models = job.models.clone();
            self.pack_wait = Some((job, models, Instant::now()));
        }
        let ctx = ctx.clone();
        let handle = hp_compile::start_with_waker(plan, &workdir, move || ctx.request_repaint());
        self.job = Some(handle);
        state.runtime.compile.badge = None;
        state.log(Severity::Info, tf!("compile.console_started", state.map_title()));
    }

    pub(crate) fn handle_event(&mut self, event: CompileEvent, state: &mut AppState) {
        let same_map = self.compiled_map_is_open(state);
        let Some(run) = &mut self.run else { return };
        let map = run.map_name.clone();
        match run.apply(event) {
            Some(Effect::StepFinished { kind, success, duration_ms }) => {
                let text = if success {
                    tf!("compile.console_step_ok", step_name(kind), fmt_duration_ms(duration_ms))
                } else {
                    tf!("compile.console_step_failed", step_name(kind), fmt_duration_ms(duration_ms))
                };
                state.log(if success { Severity::Success } else { Severity::Error }, text);
            }
            Some(Effect::Leak(path)) => {
                state.runtime.compile.badge = Some(CompileBadge::Leak);
                let leak_line = run.leak_entity.clone();
                if state.settings.compile.load_pointfile_on_leak && same_map && load_pointfile(state, &path) {
                    select_leaking_entity(state, leak_line.as_ref());
                }
            }
            Some(Effect::Finished(summary)) => {
                self.job = None;
                self.pack_wait = None;
                let (severity, text) = summary_text(&summary, &map);
                state.notify(severity, text);
                self.finish(state, summary, same_map);
            }
            None => {}
        }
    }

    fn finish(&mut self, state: &mut AppState, summary: Summary, same_map: bool) {
        let Some(cur) = self.current.take() else { return };
        if summary.leaked {
            state.runtime.compile.badge = Some(CompileBadge::Leak);
        } else if !summary.success && !summary.cancelled {
            state.runtime.compile.badge = Some(CompileBadge::Failed);
        }
        let quick = matches!(cur.modes.rad, hp_compile::RadMode::Off | hp_compile::RadMode::Draft);
        let s = &state.settings.compile;
        if summary.success
            && same_map
            && !cur.release
            && s.uses_builtin_compiler()
            && s.progressive
            && s.copy_to_maps
            && quick
            && let Some(ctx) = self.ctx.clone()
        {
            let job = progressive::Job {
                snapshot_vmf: cur.target.vmf(),
                map_name: cur.target.name.clone(),
                dir: progressive::work_dir(&hp_app::sidecar::map_key(cur.target.source.as_deref())),
                settings: s.clone(),
                root: game_root(state),
                fs: state.assets.fs().cloned(),
            };
            self.progressive.start(&ctx, job);
        }
        if summary.success
            && same_map
            && !cur.release
            && let Some((size, mtime)) = hp_compile::incremental::file_stamp(&cur.bsp)
        {
            let mut record = cur.record.clone();
            record.bsp_size = size;
            record.bsp_mtime_ns = mtime;
            let mut sc: CompileSidecar = state.sidecar.get();
            sc.builds.insert(cur.target.name.clone(), record.encode());
            state.sidecar.set(&sc);
        }
        let Some(file) = cur.history_file else { return };
        let Some(run) = &self.run else { return };
        if summary.cancelled {
            return;
        }
        let entry = HistoryEntry {
            when: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            preset: cur.preset,
            modes: cur.modes,
            auto: cur.choice.is_some(),
            steps: run
                .steps
                .iter()
                .filter(|s| matches!(s.status, StepStatus::Done | StepStatus::Failed))
                .filter(|s| !run.restored.contains(&s.index))
                .map(|s| StepTime {
                    kind: s.kind,
                    mode: snapshot::mode_name(s.kind, &cur.modes).to_owned(),
                    ms: s.duration_ms.unwrap_or(0),
                    ok: s.status == StepStatus::Done,
                })
                .collect(),
            total_ms: summary.total_ms,
            success: summary.success,
            leaked: summary.leaked,
            portalclusters: run.portalclusters,
            numportals: run.numportals,
            background: false,
        };
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("hp-compile-history".into()).spawn(move || {
            let earlier = history::load(&file);
            let slow = history::slow_steps(&earlier, &entry);
            let _ = history::append(&file, &entry);
            let _ = tx.send(slow);
        });
        self.history.pending_slow = Some(rx);
    }

    #[cfg(test)]
    pub(crate) fn begin_test_run(&mut self, plan: &hp_compile::CompilePlan) {
        self.run_id += 1;
        let mut run = RunState::preparing("gm_test");
        run.set_plan(plan);
        run.vmf = Some(plan.vmf_path.clone());
        self.run = Some(run);
    }

    fn poll_browse(&mut self, state: &mut AppState) {
        let Some((target, rx)) = &self.browse else { return };
        match rx.try_recv() {
            Ok(path) => {
                if let Some(p) = path {
                    target.apply(&mut state.settings.compile, p);
                    state.settings_dirty = true;
                }
                self.browse = None;
            }
            Err(TryRecvError::Disconnected) => self.browse = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    fn browse(&mut self, ctx: &egui::Context, target: settings_ui::BrowseTarget) {
        if self.browse.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let title = target.title();
        let filter = t!("compile.exe_filter").to_owned();
        let folder = target.is_folder();
        let spawned = std::thread::Builder::new().name("hp-compile-browse".into()).spawn(move || {
            let dialog = rfd::FileDialog::new().set_title(title);
            let picked = if folder { dialog.pick_folder() } else { dialog.add_filter(filter, &["exe"]).pick_file() };
            let _ = tx.send(picked);
            ctx.request_repaint();
        });
        if spawned.is_ok() {
            self.browse = Some((target, rx));
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        crate::util::dialog_bottom_bar(ui, "hp_compile_bottom", |ui| self.bottom_bar(ui, state));
        crate::util::dialog_body(ui, |ui| {
            let running = self.is_running() || self.pending.is_some();
            target_ui::target_game_ui(ui, state, running);
            self.settings_ui(ui, state);
            ui.separator();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Log, t!("compile.tab_log"));
                ui.selectable_value(&mut self.tab, Tab::Stats, t!("compilestats.tab"));
                ui.selectable_value(&mut self.tab, Tab::History, t!("compile.tab_history"));
                ui.selectable_value(&mut self.tab, Tab::Expert, t!("compile.tab_expert"));
                ui.selectable_value(&mut self.tab, Tab::Compilers, t!("compilers.tab"));
            });
            let running = self.is_running() || self.pending.is_some();
            match self.tab {
                Tab::History => self.history.ui(ui, state),
                Tab::Stats if self.run.is_some() => {
                    let same_map = self.compiled_map_is_open(state);
                    if let Some(run) = &self.run {
                        self.stats.update(run, self.run_id, running);
                        self.stats.ui(ui, state, run, self.run_id, same_map);
                    }
                }
                Tab::Expert => self.expert.ui(ui, state, running),
                Tab::Compilers => {
                    egui::ScrollArea::vertical().id_salt("hp_compile_compilers_scroll").show(ui, |ui| {
                        compilers_ui::editor_ui(ui, state, running);
                    });
                }
                Tab::Log if self.run.is_some() => {
                    self.steps_ui(ui);
                    self.summary_ui(ui, state);
                    self.log_ui(ui, state);
                }
                Tab::Log | Tab::Stats => {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new(tf!("compile.idle_hint", key_label(egui::Key::F9))).weak());
                    });
                }
            }
        });
    }

    fn steps_ui(&mut self, ui: &mut Ui) {
        let Some(run) = &self.run else { return };
        if run.steps.is_empty() {
            if run.summary.is_none() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(self.status_text());
                });
            }
            return;
        }
        let mut copy: Option<String> = None;
        let mut step_filter: Option<Option<usize>> = None;
        egui::Grid::new("hp_compile_steps").num_columns(4).spacing([10.0, 4.0]).striped(true).show(ui, |ui| {
            for step in &run.steps {
                match step.status {
                    StepStatus::Pending => ui.label(RichText::new("○").weak()),
                    StepStatus::Running => ui.spinner(),
                    StepStatus::Done => ui.label(RichText::new("✔").color(theme::success())),
                    StepStatus::Failed => ui.label(RichText::new("❌").color(theme::error())),
                    StepStatus::Skipped => ui.label(RichText::new("⊗").weak()),
                }
                .on_hover_text(match step.status {
                    StepStatus::Pending => t!("compile.status.pending"),
                    StepStatus::Running => t!("compile.status.running"),
                    StepStatus::Done => t!("compile.status.done"),
                    StepStatus::Failed => t!("compile.status.failed"),
                    StepStatus::Skipped => t!("compile.status.skipped"),
                });
                let name_color = match step.status {
                    StepStatus::Pending | StepStatus::Skipped => ui.visuals().weak_text_color(),
                    _ => ui.visuals().text_color(),
                };
                let filtered = self.log_view.filter.step == Some(step.index);
                let mut name_text = RichText::new(step_name(step.kind)).color(name_color);
                if filtered {
                    name_text = name_text.underline().strong();
                }
                let name = ui.add(egui::Label::new(name_text).sense(Sense::click())).on_hover_text(format!(
                    "{}\n{}",
                    t!("compile.step_filter_tip"),
                    step.command_line
                ));
                if name.clicked() {
                    step_filter = Some(if filtered { None } else { Some(step.index) });
                }
                name.context_menu(|ui| {
                    if ui.button(t!("compile.copy_command")).clicked() {
                        copy = Some(step.command_line.clone());
                        ui.close();
                    }
                });
                let text = match (&step.phase, step.status) {
                    (Some(p), StepStatus::Running) => format!("{p}  {:.0}%", step.fraction * 100.0),
                    (_, StepStatus::Running) => format!("{:.0}%", step.fraction * 100.0),
                    (_, StepStatus::Failed) if step.kind == StepKind::Bsp && run.leak_file.is_some() => {
                        t!("compile.status.leaked").to_owned()
                    }
                    (_, StepStatus::Failed) => match step.exit_code {
                        Some(c) => tf!("compile.exit_code", c),
                        None => t!("compile.status.failed").to_owned(),
                    },
                    _ => String::new(),
                };
                let fill = match step.status {
                    StepStatus::Failed => theme::bar_fill(hp_app::Severity::Error),
                    StepStatus::Done => theme::bar_fill(hp_app::Severity::Success),
                    _ => theme::bar_fill(hp_app::Severity::Info),
                };
                ui.add(
                    hp_app::widgets::progress_bar(step.fraction)
                        .desired_width(300.0)
                        .desired_height(16.0)
                        .fill(fill)
                        .text(text),
                );
                let time = match (step.status, step.started, step.duration_ms) {
                    (StepStatus::Running, Some(t), _) => {
                        fmt_duration_ms(u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX))
                    }
                    (_, _, Some(ms)) => fmt_duration_ms(ms),
                    _ => String::new(),
                };
                ui.label(RichText::new(time).monospace().weak());
                ui.end_row();
            }
        });
        if let Some(c) = copy {
            ui.ctx().copy_text(c);
        }
        if let Some(f) = step_filter {
            self.log_view.filter.step = f;
        }
    }

    fn summary_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let same_map = self.compiled_map_is_open(state);
        let Some(run) = &self.run else { return };
        let Some(summary) = run.summary else { return };
        let (severity, text) = summary_text(&summary, &run.map_name);
        let leak_file = run.leak_file.clone().filter(|_| same_map);
        let counts = counts_text(run.warnings, run.errors);
        let color = severity.color();
        let slow = self.history.slow.clone();
        self.stats.update(run, self.run_id, false);
        let hint_count = (!self.stats.stats.is_empty()).then_some(self.stats.hints.len());
        let mut open_stats = false;
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(color.gamma_multiply(0.15))
            .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.6)))
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(severity.icon()).color(color).size(16.0));
                    ui.label(RichText::new(text).color(color).strong());
                    ui.label(RichText::new(counts).weak());
                    if let Some(path) = leak_file {
                        let loaded = state.pointfile.as_ref().is_some_and(|p| p.path == path);
                        if !loaded && ui.button(t!("compile.load_leak")).clicked() {
                            load_pointfile(state, &path);
                        }
                    }
                    if let Some(n) = hint_count
                        && ui.button(tf!("compilestats.open", n)).on_hover_text(t!("compilestats.open_tip")).clicked()
                    {
                        open_stats = true;
                    }
                });
                for (kind, mode, ms, median) in &slow {
                    ui.label(
                        RichText::new(tf!(
                            "compile.slow_step",
                            step_name(*kind),
                            mode,
                            fmt_duration_ms(*ms),
                            fmt_duration_ms(*median)
                        ))
                        .color(theme::warning()),
                    );
                }
            });
        ui.add_space(4.0);
        if open_stats {
            self.tab = Tab::Stats;
        }
    }

    fn log_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let same_map = self.compiled_map_is_open(state);
        let Some(run) = &self.run else { return };
        let (warnings, errors) = (run.warnings, run.errors);
        let mut copy_all = false;
        ui.horizontal(|ui| {
            let f = &mut self.log_view.filter;
            if hp_app::widgets::icon_toggle(ui, "ℹ", f.info, t!("compile.filter_info")).clicked() {
                f.info = !f.info;
            }
            if ui
                .add(egui::Button::selectable(
                    f.warnings,
                    RichText::new(format!("⚠ {warnings}")).color(theme::warning()),
                ))
                .on_hover_text(t!("compile.filter_warnings"))
                .clicked()
            {
                f.warnings = !f.warnings;
            }
            if ui
                .add(egui::Button::selectable(f.errors, RichText::new(format!("❌ {errors}")).color(theme::error())))
                .on_hover_text(t!("compile.filter_errors"))
                .clicked()
            {
                f.errors = !f.errors;
            }
            if let Some(index) = f.step {
                let name = run.steps.iter().find(|s| s.index == index).map_or("", |s| step_name(s.kind));
                if ui.button(format!("{name}  ✖")).on_hover_text(t!("compile.clear_step_filter")).clicked() {
                    f.step = None;
                }
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut auto = !self.no_auto_scroll;
                if ui.checkbox(&mut auto, t!("compile.auto_scroll")).changed() {
                    self.no_auto_scroll = !auto;
                }
                if ui.button("📋").on_hover_text(t!("compile.copy_all")).clicked() {
                    copy_all = true;
                }
                hp_app::widgets::search_box(ui, &mut self.log_view.filter.search, t!("compile.search_log"));
            });
        });
        self.log_view.update(&run.log, self.run_id);
        if copy_all {
            ui.ctx().copy_text(run.log_text());
        }

        let row_h = log_row_height(ui);
        self.log_view.row_height = row_h;
        let mut goto: Option<LogLocation> = None;
        let mut problems: Vec<&run::LogLine> =
            run.log.iter().filter(|l| matches!(l.severity, LogSeverity::Error | LogSeverity::Warning)).collect();
        problems.sort_by_key(|l| l.severity != LogSeverity::Error);
        problems.dedup_by(|a, b| a.text == b.text);
        if !problems.is_empty() {
            let errors = problems.iter().filter(|l| l.severity == LogSeverity::Error).count();
            egui::CollapsingHeader::new(
                RichText::new(tf!("compile.problem_list", errors, problems.len() - errors)).strong(),
            )
            .id_salt(("hp_compile_problems", self.run_id))
            .default_open(errors > 0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("hp_compile_problem_list").max_height(150.0).show(ui, |ui| {
                    for line in problems.iter().take(500) {
                        let icon = if line.severity == LogSeverity::Error { "❌" } else { "⚠" };
                        let mut text = RichText::new(format!("{icon} {}", line.text.trim()))
                            .small()
                            .color(log_color(line.severity));
                        let target = line.location.as_ref().filter(|_| same_map);
                        ui.horizontal(|ui| {
                            if let Some(loc) = target {
                                if ui
                                    .add(egui::Button::new(RichText::new(t!("compile.goto")).small()).small())
                                    .on_hover_text(tf!("compile.problem_goto", location_text(loc)))
                                    .clicked()
                                {
                                    goto = Some(loc.clone());
                                }
                                text = text.underline();
                                let resp = ui
                                    .add(egui::Label::new(text).sense(Sense::click()))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .on_hover_text(tf!("compile.problem_goto", location_text(loc)));
                                if resp.clicked() {
                                    goto = Some(loc.clone());
                                }
                            } else {
                                ui.label(text);
                            }
                        });
                    }
                });
            });
        }
        let mut fix: Option<(Fix, Option<LogLocation>)> = None;
        let other_map_tip = tf!("compile.other_map", run.map_name);
        egui::Frame::new().fill(ui.visuals().extreme_bg_color).corner_radius(3.0).inner_margin(4.0).show(ui, |ui| {
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            let rows = &self.log_view.rows;
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            let char_w = ui.painter().layout_no_wrap("0".to_owned(), font, egui::Color32::WHITE).size().x;
            let widest = rows
                .iter()
                .filter_map(|&k| run.log.get(k as usize))
                .map(|l| l.text.len() as f32 * char_w + if l.knowledge.is_some() { 200.0 } else { 0.0 })
                .fold(0.0f32, f32::max);
            egui::ScrollArea::both()
                .id_salt("hp_compile_log")
                .auto_shrink([false, false])
                .min_scrolled_height(80.0)
                .stick_to_bottom(!self.no_auto_scroll)
                .show_rows(ui, row_h, rows.len(), |ui, range| {
                    ui.set_min_width(widest + 16.0);
                    let avail = ui.available_width();
                    let width = if avail.is_finite() { avail.max(widest + 16.0) } else { widest + 16.0 };
                    for i in range {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, row_h), Sense::hover());
                        let Some(line) = rows.get(i).and_then(|&k| run.log.get(k as usize)) else { continue };
                        let mut row = ui.new_child(
                            egui::UiBuilder::new()
                                .id_salt(("hp_compile_log_row", i))
                                .max_rect(rect)
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        log_row(&mut row, line, same_map, &other_map_tip, &mut goto, &mut fix);
                    }
                });
        });
        if let Some(loc) = goto
            && !goto_location(state, &loc)
        {
            state.notify(Severity::Warning, t!("compile.not_found"));
        }
        if let Some((f, loc)) = fix {
            self.apply_fix(ui.ctx(), state, f, loc.as_ref(), same_map);
        }
    }

    fn apply_fix(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        fix: Fix,
        loc: Option<&LogLocation>,
        same: bool,
    ) {
        match fix {
            Fix::GoToLocation => {
                if !same || !loc.is_some_and(|l| goto_location(state, l)) {
                    state.notify(Severity::Warning, t!("compile.not_found"));
                }
            }
            Fix::OpenProblems => state.request(Action::CheckProblems),
            Fix::OpenDocs(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            Fix::LoadLeak => {
                let file = self.run.as_ref().and_then(|r| r.leak_file.clone());
                match file.filter(|_| same) {
                    Some(f) => {
                        if load_pointfile(state, &f) {
                            let line = self.run.as_ref().and_then(|r| r.leak_entity.clone());
                            select_leaking_entity(state, line.as_ref());
                        }
                    }
                    None => state.request(Action::LoadPointfile),
                }
            }
        }
    }

    fn bottom_bar(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        let running = self.is_running() || self.pending.is_some();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t!("common.close")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                state.ui.show_compile = false;
            }
            if running {
                if ui
                    .add(egui::Button::new(RichText::new(format!("⏹ {}", t!("common.cancel"))).color(theme::error())))
                    .on_hover_text(t!("compile.cancel_tip"))
                    .clicked()
                {
                    self.cancel(state);
                }
            } else {
                let label = state
                    .runtime
                    .compile
                    .auto_label
                    .clone()
                    .filter(|_| settings_ui::smart_applies(&state.settings.compile))
                    .unwrap_or_else(|| t!("compile.compile").to_owned());
                let button = egui::Button::new(RichText::new(label).strong().color(theme::on_accent()))
                    .fill(theme::accent())
                    .min_size(egui::vec2(110.0, 24.0));
                if ui.add(button).on_hover_text(tf!("compile.compile_tip", key_label(egui::Key::Enter))).clicked() {
                    self.start(&ctx, state);
                }
            }
            let cordoned = state.settings.compile.compile_cordon_only && state.doc.cordons_in_use();
            if cordoned {
                ui.label(RichText::new(format!("⚠ {}", t!("compile.cordon"))).color(theme::warning()))
                    .on_hover_text(t!("compile.cordon_only_tip"));
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                if running {
                    ui.spinner();
                    if let Some(run) = &self.run {
                        ui.add(
                            hp_app::widgets::progress_bar(run.overall_progress())
                                .desired_width(140.0)
                                .show_percentage(),
                        );
                    }
                }
                ui.add(egui::Label::new(RichText::new(self.status_text()).weak()).truncate());
            });
        });
    }

    fn status_text(&self) -> String {
        if self.waiting_for_save() {
            return t!("compile.waiting_save_short").to_owned();
        }
        if matches!(self.pending, Some(Pending::Assets { .. })) {
            return t!("compile.waiting_assets").to_owned();
        }
        if self.prep.is_some() {
            return t!("compile.preparing").to_owned();
        }
        if let Some(run) = &self.run {
            if let Some(step) = run.steps.iter().find(|s| s.status == StepStatus::Running) {
                let elapsed = fmt_duration_ms(u64::try_from(run.started.elapsed().as_millis()).unwrap_or(u64::MAX));
                return tf!("compile.status_running", step_name(step.kind), elapsed);
            }
            if let Some(s) = run.summary {
                return summary_text(&s, &run.map_name).1;
            }
        }
        t!("status.ready").to_owned()
    }
}

pub(crate) fn log_row_height(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Monospace).max(ui.spacing().interact_size.y).max(16.0)
}

fn log_row(
    ui: &mut Ui,
    line: &run::LogLine,
    same_map: bool,
    other_map_tip: &str,
    goto: &mut Option<LogLocation>,
    fix: &mut Option<(Fix, Option<LogLocation>)>,
) {
    if let Some(k) = line.knowledge {
        let tip = notices::knowledge_text(k);
        let label = notices::fix_label(k.fix);
        let button = ui.add(egui::Button::new(RichText::new(format!("? {label}")).small()).small()).on_hover_text(tip);
        if button.clicked() {
            *fix = Some((k.fix, line.location.clone()));
        }
    }
    if let Some(loc) = line.location.as_ref().filter(|_| same_map)
        && ui
            .add(egui::Button::new(RichText::new(t!("compile.goto")).small()).small())
            .on_hover_text(tf!("compile.problem_goto", location_text(loc)))
            .clicked()
    {
        *goto = Some(loc.clone());
    }
    let mut text = RichText::new(&line.text).monospace().color(log_color(line.severity));
    let clickable = line.location.is_some() && same_map;
    if clickable {
        text = text.underline();
    }
    let mut label = egui::Label::new(text).extend();
    if clickable {
        label = label.sense(Sense::click());
    }
    let resp = ui.add(label);
    let Some(loc) = &line.location else { return };
    if !same_map {
        resp.on_hover_text(other_map_tip);
        return;
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(location_text(loc));
    if resp.clicked() {
        *goto = Some(loc.clone());
    }
}

fn select_leaking_entity(state: &mut AppState, line: Option<&LogLocation>) {
    let last = state.pointfile.as_ref().and_then(|p| p.points.last().copied());
    if let Some(id) = leaking_entity(state, line, last) {
        state.doc.set_selection(&[ObjRef::Entity(id)]);
    }
}

pub(crate) fn history_file(map_key: &str) -> PathBuf {
    Settings::cache_dir().join("compile_history").join(format!("{map_key}.jsonl"))
}

fn summary_text(s: &Summary, map: &str) -> (Severity, String) {
    let time = fmt_duration_ms(s.total_ms);
    if s.cancelled {
        (Severity::Warning, t!("compile.cancelled").to_owned())
    } else if s.leaked {
        (Severity::Error, tf!("compile.summary_leaked", map))
    } else if s.success {
        (Severity::Success, tf!("compile.summary_ok", map, time))
    } else {
        (Severity::Error, tf!("compile.summary_failed", map))
    }
}

fn counts_text(warnings: usize, errors: usize) -> String {
    format!(
        "{}, {}",
        tf!(plural(warnings, "compile.warning_count_one", "compile.warning_count"), warnings),
        tf!(plural(errors, "compile.error_count_one", "compile.error_count"), errors)
    )
}

pub(crate) fn is_iteration_preset(p: CompilePresetSetting) -> bool {
    matches!(p, CompilePresetSetting::Draft | CompilePresetSetting::Normal)
}

#[cfg(test)]
mod tests;
