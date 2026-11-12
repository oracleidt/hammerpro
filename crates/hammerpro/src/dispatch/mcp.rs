use std::sync::Arc;
use std::time::{Duration, Instant};

use hp_app::i18n::TrTable;
use hp_app::settings::CompilePresetSetting;
use hp_app::{Action, Severity, t, tf};
use hp_mcp::protocol::ToolResult;
use hp_mcp::server::{McpServer, ServerConfig, ToolRequest};
use serde_json::{Value, json};

use super::Package;
use crate::app::HammerProApp;

mod automation;
mod describe;
#[cfg(test)]
mod harness;
mod place_feedback;
mod place_ops;
mod place_scan;
#[cfg(test)]
mod place_tests;
mod place_ui;
#[cfg(test)]
mod place_ui_tests;
mod place_world;
mod schema;
mod semantic;
#[cfg(test)]
mod semantic_tests;
mod tools;
mod tools_actions;
mod tools_assets;
mod tools_blender;
mod tools_combine;
mod tools_compile;
mod tools_compilers;
mod tools_content;
mod tools_crash;
mod tools_curve;
mod tools_detail;
mod tools_discord;
mod tools_disp;
mod tools_disp_level;
mod tools_docs;
mod tools_edit;
mod tools_entities;
mod tools_fgd;
mod tools_fog;
mod tools_func_detail;
mod tools_games;
mod tools_gpu;
mod tools_grids;
mod tools_groups;
mod tools_health;
mod tools_help;
mod tools_history;
mod tools_instances;
mod tools_loops;
mod tools_media;
mod tools_meshdisp;
mod tools_optimize;
mod tools_overlays;
mod tools_particles;
mod tools_paths;
mod tools_pipe;
mod tools_place;
pub(crate) use place_scan::{MapScanBuilder, ScanMemory, run_map_scan_since};
#[cfg(test)]
pub(crate) use place_scan::{prepare_map_scan, run_map_scan};
pub(crate) use place_ui::{
    PlaceReport, Sheet, UiPlace, auto_settle_created, nearest_wall, place as place_objects, real_dims,
    settle as settle_objects, sheet_job_model, sheet_job_selection,
};
#[cfg(test)]
pub(crate) use place_world::pin_shape;
pub(crate) use tools_place::mesh_drop;
mod tools_road;
mod tools_roadside;
mod tools_settings;
mod tools_shader;
mod tools_sky;
mod tools_slice;
mod tools_slope;
mod tools_sniffer;
mod tools_storage;
mod tools_team;
mod tools_team_access;
mod tools_terrain;
mod tools_toolopts;
mod tools_ui;
mod tools_update;
mod tools_views;
mod tools_vtf;
mod tools_weld;
mod window;

pub(crate) use semantic::local_path;

pub(crate) fn script_setting(
    state: &mut hp_app::AppState,
    ctx: &egui::Context,
    path: &str,
    value: &str,
) -> Result<(), String> {
    tools_settings::script_set(state, ctx, path, value)
}

pub(crate) fn tool_specs() -> Vec<hp_mcp::tools::ToolSpec> {
    tools::all_specs().copied().collect()
}

use tools::{AppOp, Outcome};

pub(crate) struct Mcp;

enum Answer {
    Now(ToolResult),
    Later(automation::UiOp),
    Worker(tools::Worker),
}

pub(crate) fn inject_input(app: &mut HammerProApp, ctx: &egui::Context, raw: &mut egui::RawInput) {
    app.pkg.mcp.ui.inject(ctx, raw);
}

const CALLS_PER_FRAME: usize = 8;
const WORKER_FALLBACK: Duration = Duration::from_millis(250);
const RECORD_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug)]
struct CompileRestore {
    preset: (CompilePresetSetting, CompilePresetSetting),
    launch: (bool, bool),
}

#[derive(Default)]
pub(crate) struct State {
    server: Option<McpServer>,
    started_once: bool,
    focused: bool,
    record_written: Option<Instant>,
    compile_restore: Option<CompileRestore>,
    window: window::McpWindow,
    ui: automation::UiAutomation,
    workers: Vec<(tools::Worker, ToolRequest)>,
}

const HANDLED: &[Action] = &[Action::ToggleMcpServer, Action::McpInstall, Action::McpPanel];

impl Package for Mcp {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::ToggleMcpServer => {
                if app.pkg.mcp.server.is_some() {
                    app.stop_mcp();
                } else {
                    app.start_mcp(ctx);
                }
            }
            Action::McpPanel => {
                app.state.runtime.mcp.show_window = true;
                app.pkg.mcp.window.refresh(&mut app.state);
            }
            Action::McpInstall => {
                app.state.runtime.mcp.show_window = true;
                app.pkg.mcp.window.refresh(&mut app.state);
                app.pkg.mcp.window.install_all(ctx, &mut app.state);
            }
            _ => return false,
        }
        true
    }

    fn init(&self, app: &mut HammerProApp) {
        app.state.runtime.mcp.tools = tools::all_specs().map(|t| (t.name.to_owned(), t.title.to_owned())).collect();
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.mcp_tick(ctx);
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        window::show(app, ctx);
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

impl HammerProApp {
    pub(crate) fn start_mcp(&mut self, ctx: &egui::Context) {
        if self.pkg.mcp.server.is_some() {
            return;
        }
        tools::set_worker_waker(ctx);
        let mut config = ServerConfig::new(self.state.map_title());
        config.tools = tools::all_definitions();
        let waker = ctx.clone();
        config.waker = Some(Arc::new(move || {
            waker.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::RequestPaintWhileHidden);
            waker.request_repaint();
        }));
        match McpServer::start(config) {
            Ok(server) => {
                let port = server.port();
                let rt = &mut self.state.runtime.mcp;
                rt.running = true;
                rt.port = Some(port);
                rt.clients = 0;
                rt.calls = 0;
                rt.error = None;
                self.pkg.mcp.server = Some(server);
                self.pkg.mcp.record_written = Some(Instant::now());
                self.state.notify(Severity::Info, tf!("mcp.started", port));
            }
            Err(e) => {
                log::warn!("MCP server: {e}");
                self.state.runtime.mcp.error = Some(e.to_string());
                self.state.notify(Severity::Warning, tf!("mcp.start_failed", e));
            }
        }
    }

    pub(crate) fn stop_mcp(&mut self) {
        let Some(mut server) = self.pkg.mcp.server.take() else { return };
        server.stop();
        let rt = &mut self.state.runtime.mcp;
        rt.running = false;
        rt.port = None;
        rt.clients = 0;
        self.state.notify(Severity::Info, t!("mcp.stopped"));
    }

    fn mcp_tick(&mut self, ctx: &egui::Context) {
        if !self.pkg.mcp.started_once {
            self.pkg.mcp.started_once = true;
            if self.state.settings.mcp.start_with_app {
                self.start_mcp(ctx);
            }
        }
        self.restore_compile_settings();
        self.pkg.mcp.window.poll(&mut self.state);
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(false);
        let gained = focused && !self.pkg.mcp.focused;
        self.pkg.mcp.focused = focused;
        let title = self.state.map_title();
        let Some(server) = self.pkg.mcp.server.as_mut() else { return };
        let title_changed = server.record().map_title != title;
        let due = self.pkg.mcp.record_written.is_none_or(|t| t.elapsed() >= RECORD_INTERVAL);
        if gained || (title_changed && due) {
            if let Err(e) = server.update_record(&title, gained) {
                log::warn!("MCP instance record: {e}");
            }
            self.pkg.mcp.record_written = Some(Instant::now());
        } else if title_changed {
            ctx.request_repaint_after(RECORD_INTERVAL);
        }
        let rt = &mut self.state.runtime.mcp;
        rt.clients = server.clients();
        rt.calls = server.calls();
        let mut requests = Vec::new();
        while requests.len() < CALLS_PER_FRAME {
            match server.try_recv() {
                Some(r) => requests.push(r),
                None => break,
            }
        }
        if requests.len() == CALLS_PER_FRAME {
            ctx.request_repaint();
        }
        for request in requests {
            let name = request.name.clone();
            let answer = self.run_mcp_tool(ctx, &name, &request.arguments);
            self.state.runtime.mcp.last_tool = Some(name.clone());
            match answer {
                Answer::Now(result) => {
                    if result.is_error {
                        log::info!("MCP {name}: {}", result.first_text());
                    }
                    request.respond(result);
                }
                Answer::Later(op) => self.start_ui_op(ctx, &name, op, request),
                Answer::Worker(w) => self.pkg.mcp.workers.push((w, request)),
            }
        }
        self.poll_ui_ops(ctx);
        self.poll_workers(ctx);
    }

    fn poll_workers(&mut self, ctx: &egui::Context) {
        let workers = std::mem::take(&mut self.pkg.mcp.workers);
        for (w, request) in workers {
            match w.poll(&mut self.state) {
                Some(result) => {
                    if result.is_error {
                        log::info!("MCP {}: {}", request.name, result.first_text());
                    }
                    request.respond(result);
                }
                None => self.pkg.mcp.workers.push((w, request)),
            }
        }
        if !self.pkg.mcp.workers.is_empty() {
            ctx.request_repaint_after(WORKER_FALLBACK);
        }
    }

    fn start_ui_op(&mut self, ctx: &egui::Context, name: &str, op: automation::UiOp, request: ToolRequest) {
        let slot = Arc::new(std::sync::Mutex::new(Some(request)));
        let reply_slot = slot.clone();
        let tool = name.to_owned();
        let reply: automation::Reply = Box::new(move |result: ToolResult| {
            if result.is_error {
                log::info!("MCP {tool}: {}", result.first_text());
            }
            if let Some(request) = reply_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                request.respond(result);
            }
        });
        let mut ui = std::mem::take(&mut self.pkg.mcp.ui);
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ui.start(self, ctx, op, reply)));
        self.pkg.mcp.ui = ui;
        if run.is_err() {
            if self.state.doc.in_transaction() {
                self.state.doc.cancel();
            }
            if let Some(request) = slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                request.respond(ToolResult::error(format!("{name}: internal error (see the Hammer Pro log)")));
            }
        }
    }

    fn poll_ui_ops(&mut self, ctx: &egui::Context) {
        let mut ui = std::mem::take(&mut self.pkg.mcp.ui);
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ui.poll(self, ctx)));
        if run.is_err() {
            log::warn!("MCP: a waiting call failed (see the log)");
        }
        self.pkg.mcp.ui = ui;
    }

    fn run_mcp_tool(&mut self, ctx: &egui::Context, name: &str, arguments: &Value) -> Answer {
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match tools::run(&mut self.state, name, arguments) {
                Outcome::Done(r) => Answer::Now(r),
                Outcome::App(op) => Answer::Now(self.run_app_op(ctx, op)),
                Outcome::AppFn(f) => {
                    let answer = Answer::Now((f.0)(self, ctx));
                    self.state.doc.end_selection_burst();
                    answer
                }
                Outcome::Ui(op) => Answer::Later(op),
                Outcome::Worker(w) => Answer::Worker(w),
            }
        }));
        run.unwrap_or_else(|_| {
            if self.state.doc.in_transaction() {
                self.state.doc.cancel();
            }
            Answer::Now(ToolResult::error(format!("{name}: internal error (see the Hammer Pro log)")))
        })
    }

    fn run_app_op(&mut self, ctx: &egui::Context, op: AppOp) -> ToolResult {
        match op {
            AppOp::Compile { preset, launch } => self.mcp_compile(ctx, preset, launch),
            AppOp::Open(path) => {
                self.request_open(ctx, path.clone());
                let waiting = self.pending.is_some();
                ToolResult::json(json!({
                    "opening": path.display().to_string(),
                    "note": if waiting {
                        "Hammer Pro asks the user to save or discard the changes of the current map first."
                    } else {
                        "The map loads in the background: call get_map_info to see it."
                    },
                }))
            }
        }
    }

    fn mcp_compile(&mut self, ctx: &egui::Context, preset: Option<CompilePresetSetting>, launch: bool) -> ToolResult {
        if self.compile.is_running() {
            return ToolResult::error("compile_map: a compile is already running (get_compile_status)");
        }
        self.restore_compile_settings();
        let s = &mut self.state.settings.compile;
        let restore =
            CompileRestore { preset: (s.preset, preset.unwrap_or(s.preset)), launch: (s.launch_game, launch) };
        s.preset = restore.preset.1;
        s.launch_game = launch;
        self.compile.start_now(ctx, &mut self.state);
        let started = self.compile.is_running();
        let chosen = format!("{:?}", restore.preset.1).to_ascii_lowercase();
        self.pkg.mcp.compile_restore = Some(restore);
        if !started {
            self.restore_compile_settings();
            return ToolResult::error("compile_map: the compile did not start (see the Hammer Pro console)");
        }
        ToolResult::json(json!({
            "started": true,
            "preset": chosen,
            "launch_game": launch,
            "note": "Poll get_compile_status until running is false.",
        }))
    }

    fn restore_compile_settings(&mut self) {
        if self.compile.is_running() {
            return;
        }
        let Some(r) = self.pkg.mcp.compile_restore.take() else { return };
        let s = &mut self.state.settings.compile;
        if s.preset == r.preset.1 {
            s.preset = r.preset.0;
        }
        if s.launch_game == r.launch.1 {
            s.launch_game = r.launch.0;
        }
        self.state.settings_dirty = true;
    }
}

const TR: TrTable = &[
    ("mcp.started", "MCP server on (local port {0})", "Serveur MCP actif (port local {0})"),
    ("mcp.stopped", "MCP server off", "Serveur MCP inactif"),
    ("mcp.start_failed", "The MCP server could not start: {0}", "Le serveur MCP n'a pas pu démarrer : {0}"),
    ("mcp.window_title", "Hammer Pro MCP", "Hammer Pro MCP"),
    ("mcp.server", "Server of this window", "Serveur de cette fenêtre"),
    ("mcp.on", "On, local port {0}, {1}", "Actif, port local {0}, {1}"),
    ("mcp.off", "Off", "Inactif"),
    ("mcp.switch_on", "Switch on", "Activer"),
    ("mcp.switch_off", "Switch off", "Désactiver"),
    (
        "mcp.server_note",
        "AI apps reach this window through \"hammerpro --mcp\", which they start themselves once installed below. Each window has its own server, off by default.",
        "Les applications d'IA atteignent cette fenêtre via \"hammerpro --mcp\", qu'elles lancent elles-mêmes une fois installé ci-dessous. Chaque fenêtre a son propre serveur, inactif par défaut.",
    ),
    ("mcp.apps", "AI apps", "Applications d'IA"),
    ("mcp.install_all", "Install in Claude, Codex, Gemini", "Installer dans Claude, Codex, Gemini"),
    (
        "mcp.install_all_tip",
        "Adds the hammer-pro server to Claude Code, Claude Desktop, Codex and Gemini CLI (in this order, when they are on this computer), then checks each one",
        "Ajoute le serveur hammer-pro à Claude Code, Claude Desktop, Codex et Gemini CLI (dans cet ordre, s'ils sont sur cet ordinateur), puis vérifie chacun",
    ),
    ("mcp.working", "Working...", "En cours..."),
    ("mcp.install", "Install", "Installer"),
    ("mcp.update", "Update", "Mettre à jour"),
    ("mcp.verify", "Verify", "Vérifier"),
    ("mcp.remove", "Remove", "Retirer"),
    ("mcp.status.not_installed", "Not installed on this computer", "Pas installé sur cet ordinateur"),
    ("mcp.status.not_configured", "Installed, not configured", "Installé, non configuré"),
    ("mcp.status.configured", "Installed and configured", "Installé et configuré"),
    (
        "mcp.status.other_exe",
        "Configured for another copy of Hammer Pro ({0}): click Update",
        "Configuré pour une autre copie de Hammer Pro ({0}) : cliquez sur Mettre à jour",
    ),
    ("mcp.status.error", "Error: {0}", "Erreur : {0}"),
    ("mcp.status.unchecked", "Not checked yet: click Verify", "Pas encore vérifié : cliquez sur Vérifier"),
    (
        "mcp.restart_note",
        "Restart the AI app (or start a new session) after installing so it sees the new server.",
        "Redémarrez l'application d'IA (ou ouvrez une nouvelle session) après l'installation pour qu'elle voie le nouveau serveur.",
    ),
    ("mcp.config_file", "File: {0}", "Fichier : {0}"),
    ("mcp.manual", "Manual setup", "Configuration manuelle"),
    (
        "mcp.manual_note",
        "Add this to the app yourself if you prefer (or if the automatic install cannot edit its file).",
        "Ajoutez ceci vous-même à l'application si vous préférez (ou si l'installation automatique ne peut pas modifier son fichier).",
    ),
    ("mcp.copy", "Copy", "Copier"),
    ("mcp.copied", "Copied to the clipboard", "Copié dans le presse-papiers"),
    ("mcp.docs", "MCP documentation", "Documentation MCP"),
    ("mcp.install_done", "MCP install: {0}", "Installation MCP : {0}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_go_through_the_bridge_and_the_window_to_the_map() {
        let dir = std::env::temp_dir().join(format!("hp_mcp_dispatch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut state = hp_app::testing::test_state();
        let solids = state.doc.stats().solids;
        let mut config = ServerConfig::new(state.map_title());
        config.dir = dir.clone();
        let server = McpServer::start(config).expect("server");

        let client_dir = dir.clone();
        let client = std::thread::spawn(move || {
            let mut input = Vec::new();
            for m in [
                json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"create_brush","arguments":{"min":[0,0,0],"max":[64,64,64],"shape":"wedge"}}}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_map_info","arguments":{}}}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"undo","arguments":{}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"create_brush","arguments":{"min":[0,0,0],"max":[0,0,0]}}}),
            ] {
                hp_mcp::protocol::write_message(&mut input, &m).unwrap();
            }
            let options = hp_mcp::bridge::BridgeOptions {
                dir: client_dir,
                pinned: None,
                call_timeout: Duration::from_secs(30),
                connect_timeout: Duration::from_secs(3),
                tools: tools::all_specs().copied().collect(),
            };
            let mut out = Vec::new();
            hp_mcp::bridge::run(std::io::Cursor::new(input), &mut out, options).unwrap();
            String::from_utf8(out)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .collect::<Vec<_>>()
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut served = 0;
        while served < 4 && Instant::now() < deadline {
            match server.try_recv() {
                Some(req) => {
                    let result = match tools::run(&mut state, &req.name, &req.arguments) {
                        Outcome::Done(r) => r,
                        Outcome::App(op) => ToolResult::error(format!("{op:?}")),
                        Outcome::AppFn(f) => ToolResult::error(format!("{f:?}")),
                        Outcome::Ui(op) => ToolResult::error(format!("{op:?}")),
                        Outcome::Worker(w) => w.wait(&mut state, Duration::from_secs(60)),
                    };
                    req.respond(result);
                    served += 1;
                }
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        let answers = client.join().unwrap();
        assert_eq!(answers.len(), 5, "{answers:?}");
        let created = &answers[1]["result"];
        assert_eq!(created["isError"], false, "{created}");
        assert_eq!(answers[2]["result"]["structuredContent"]["stats"]["solids"], solids as u64 + 1);
        assert_eq!(answers[2]["result"]["structuredContent"]["undo"], "MCP: create_brush");
        assert_eq!(answers[3]["result"]["structuredContent"]["undone"], "MCP: create_brush");
        assert_eq!(answers[4]["result"]["isError"], true);
        assert_eq!(state.doc.stats().solids, solids, "undone");
        assert!(state.doc.can_redo());
        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty());
        for &(key, _, fr) in TR {
            assert!(key.starts_with("mcp."), "{key}");
            for (i, c) in fr.char_indices() {
                if matches!(c, ':' | '?' | '!') && i > 0 {
                    assert!(fr[..i].ends_with(' '), "{key}: {fr}");
                }
            }
        }
    }
}
