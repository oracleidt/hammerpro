use std::collections::HashMap;
use std::path::PathBuf;

use egui::RichText;
use hp_app::runtime::McpClientRow;
use hp_app::{AppState, Severity, t, tf};
use hp_mcp::install::{self, Client, Env, Status};
use hp_ui_shell::jobs::{Job, Poll};

use crate::app::HammerProApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    InstallAll,
    Install(Client),
    Verify(Client),
    Remove(Client),
}

const STATUS_WIDTH: f32 = 260.0;

type Results = Vec<(Client, Status)>;

#[derive(Default)]
pub(crate) struct McpWindow {
    env: Option<Env>,
    statuses: HashMap<Client, Option<Status>>,
    job: Option<(Task, Job<Results>)>,
}

fn this_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(&p).ok().or(Some(p)))
        .map(|p| strip_verbatim(&p))
        .unwrap_or_else(|| PathBuf::from("hammerpro"))
}

fn strip_verbatim(p: &std::path::Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => p.to_path_buf(),
    }
}

fn status_text(status: Option<&Status>) -> (String, bool) {
    match status {
        None => (t!("mcp.status.unchecked").to_owned(), false),
        Some(Status::NotInstalled) => (t!("mcp.status.not_installed").to_owned(), false),
        Some(Status::NotConfigured) => (t!("mcp.status.not_configured").to_owned(), false),
        Some(Status::Configured) => (t!("mcp.status.configured").to_owned(), true),
        Some(Status::OtherExe(c)) => (tf!("mcp.status.other_exe", c), false),
        Some(Status::Error(e)) => (tf!("mcp.status.error", e), false),
    }
}

impl McpWindow {
    fn env(&mut self) -> &Env {
        self.env.get_or_insert_with(|| Env::detect(this_exe()))
    }

    pub(crate) fn refresh(&mut self, state: &mut AppState) {
        if self.job.is_some() {
            return;
        }
        let env = self.env().clone();
        for c in Client::ALL {
            let status = match c {
                Client::ClaudeCode if install::find_cli(&env, "claude").is_some() => {
                    self.statuses.get(&c).cloned().flatten()
                }
                Client::ClaudeCode => Some(Status::NotInstalled),
                _ => Some(install::status(&env, c)),
            };
            self.statuses.insert(c, status);
        }
        self.publish(state);
    }

    pub(crate) fn install_all(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.start(ctx, state, Task::InstallAll);
    }

    fn start(&mut self, ctx: &egui::Context, state: &mut AppState, task: Task) {
        if self.job.is_some() {
            return;
        }
        let env = self.env().clone();
        let job = Job::spawn(ctx, "mcp-install", move || match task {
            Task::InstallAll => install::install_all(&env),
            Task::Install(c) => vec![(c, install::install(&env, c))],
            Task::Verify(c) => vec![(c, install::status(&env, c))],
            Task::Remove(c) => vec![(c, install::uninstall(&env, c))],
        });
        self.job = Some((task, job));
        state.runtime.mcp.install_running = true;
        self.publish(state);
    }

    pub(crate) fn poll(&mut self, state: &mut AppState) {
        let Some((task, job)) = &self.job else { return };
        let task = *task;
        let results = match job.poll() {
            Poll::Pending => return,
            Poll::Ready(r) => r,
            Poll::Failed => Vec::new(),
        };
        self.job = None;
        state.runtime.mcp.install_running = false;
        for (c, s) in &results {
            self.statuses.insert(*c, Some(s.clone()));
        }
        if task == Task::InstallAll {
            let summary: Vec<String> = results
                .iter()
                .filter(|(_, s)| *s != Status::NotInstalled)
                .map(|(c, s)| format!("{} {}", c.label(), if s.is_configured() { "OK" } else { "-" }))
                .collect();
            let text = if summary.is_empty() { t!("mcp.status.not_installed").to_owned() } else { summary.join(", ") };
            let ok = results.iter().any(|(_, s)| s.is_configured());
            state.notify(if ok { Severity::Success } else { Severity::Warning }, tf!("mcp.install_done", text));
        }
        self.publish(state);
    }

    fn publish(&self, state: &mut AppState) {
        state.runtime.mcp.install_rows = Client::ALL
            .iter()
            .map(|&c| {
                let s = self.statuses.get(&c).cloned().flatten();
                let (text, ok) = status_text(s.as_ref());
                McpClientRow {
                    id: c.id(),
                    label: c.label(),
                    status: s.is_some().then_some(text),
                    ok,
                    present: s.as_ref() != Some(&Status::NotInstalled),
                }
            })
            .collect();
    }
}

pub(crate) fn show(app: &mut HammerProApp, ctx: &egui::Context) {
    if !app.state.runtime.mcp.show_window {
        return;
    }
    let mut open = true;
    let mut toggle = false;
    let mut task = None;
    hp_app::chrome::Window::new(t!("mcp.window_title"))
        .id(egui::Id::unique("hp_mcp_window"))
        .open(&mut open)
        .resizable(true)
        .default_width(660.0)
        .collapsible(false)
        .show(ctx, |ui| {
            let win = &mut app.pkg.mcp.window;
            let state = &mut app.state;
            server_section(ui, state, &mut toggle);
            ui.add_space(6.0);
            ui.separator();
            apps_section(ui, win, state, &mut task);
            ui.add_space(6.0);
            ui.separator();
            manual_section(ui, win);
            ui.add_space(4.0);
            let docs = format!("{}/blob/HEAD/docs/MCP.md", hp_ui_shell::help::REPOSITORY_URL);
            ui.hyperlink_to(t!("mcp.docs"), docs);
        });
    if !open {
        app.state.runtime.mcp.show_window = false;
    }
    if toggle {
        if app.pkg.mcp.server.is_some() {
            app.stop_mcp();
        } else {
            app.start_mcp(ctx);
        }
    }
    if let Some(task) = task {
        app.pkg.mcp.window.start(ctx, &mut app.state, task);
    }
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().color(ui.visuals().strong_text_color()));
}

fn server_section(ui: &mut egui::Ui, state: &AppState, toggle: &mut bool) {
    let mcp = &state.runtime.mcp;
    section_title(ui, t!("mcp.server"));
    ui.horizontal(|ui| {
        let text = match (mcp.running, mcp.port) {
            (true, Some(port)) => {
                let clients = if mcp.clients == 1 {
                    t!("shell.mcp.client_one").to_owned()
                } else {
                    tf!("shell.mcp.client_many", mcp.clients)
                };
                tf!("mcp.on", port, clients)
            }
            _ => t!("mcp.off").to_owned(),
        };
        ui.label(RichText::new(text).monospace());
        let label = if mcp.running { t!("mcp.switch_off") } else { t!("mcp.switch_on") };
        if ui.button(label).clicked() {
            *toggle = true;
        }
    });
    if let Some(e) = &mcp.error {
        ui.label(RichText::new(tf!("mcp.start_failed", e)).small());
    }
    ui.label(RichText::new(t!("mcp.server_note")).small().weak());
}

fn apps_section(ui: &mut egui::Ui, win: &mut McpWindow, state: &AppState, task: &mut Option<Task>) {
    section_title(ui, t!("mcp.apps"));
    let busy = win.job.is_some();
    ui.horizontal(|ui| {
        let b =
            ui.add_enabled(!busy, egui::Button::new(RichText::new(format!("⬇ {}", t!("mcp.install_all"))).strong()));
        if b.on_hover_text(t!("mcp.install_all_tip")).clicked() {
            *task = Some(Task::InstallAll);
        }
        if busy {
            hp_app::widgets::paced_spinner(ui, 14.0);
            ui.label(RichText::new(t!("mcp.working")).weak());
        }
    });
    ui.add_space(4.0);
    let env = win.env().clone();
    egui::Grid::new("hp_mcp_clients").num_columns(3).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
        for row in &state.runtime.mcp.install_rows {
            let Some(client) = Client::ALL.into_iter().find(|c| c.id() == row.id) else { continue };
            let mark = if row.ok { "✔" } else { "  " };
            ui.label(RichText::new(format!("{mark} {}", row.label)).strong());
            let (text, _) = status_text(win.statuses.get(&client).and_then(Option::as_ref));
            let r = ui.scope(|ui| {
                ui.set_width(STATUS_WIDTH);
                ui.add(egui::Label::new(RichText::new(text).small()).wrap())
            });
            let r = r.inner;
            if let Some(path) = install::config_path(&env, client) {
                r.on_hover_text(tf!("mcp.config_file", path.display()));
            }
            ui.horizontal(|ui| {
                let other = matches!(win.statuses.get(&client), Some(Some(Status::OtherExe(_))));
                let install_label = if other { t!("mcp.update") } else { t!("mcp.install") };
                let enabled = !busy && row.present;
                if ui.add_enabled(enabled && !row.ok, egui::Button::new(install_label)).clicked() {
                    *task = Some(Task::Install(client));
                }
                if ui.add_enabled(enabled, egui::Button::new(t!("mcp.verify"))).clicked() {
                    *task = Some(Task::Verify(client));
                }
                let configured = row.ok || other;
                if ui.add_enabled(enabled && configured, egui::Button::new(t!("mcp.remove"))).clicked() {
                    *task = Some(Task::Remove(client));
                }
            });
            ui.end_row();
        }
    });
    ui.label(RichText::new(t!("mcp.restart_note")).small().weak());
}

fn manual_section(ui: &mut egui::Ui, win: &mut McpWindow) {
    let exe = win.env().exe.clone();
    egui::CollapsingHeader::new(t!("mcp.manual")).id_salt("hp_mcp_manual").show(ui, |ui| {
        ui.label(RichText::new(t!("mcp.manual_note")).small().weak());
        for client in Client::ALL {
            let snippet = install::snippet(client, &exe);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(client.label()).strong());
                if ui.small_button(t!("mcp.copy")).clicked() {
                    ui.ctx().copy_text(snippet.clone());
                }
            });
            let mut text = snippet.as_str();
            ui.add(
                egui::TextEdit::multiline(&mut text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(1)
                    .desired_width(f32::INFINITY),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_prefixes_are_removed() {
        assert_eq!(
            strip_verbatim(std::path::Path::new(r"\\?\C:\x\hammerpro.exe")),
            PathBuf::from(r"C:\x\hammerpro.exe")
        );
        assert_eq!(strip_verbatim(std::path::Path::new(r"C:\x\a.exe")), PathBuf::from(r"C:\x\a.exe"));
        assert_eq!(strip_verbatim(std::path::Path::new(r"\\?\UNC\srv\a.exe")), PathBuf::from(r"\\?\UNC\srv\a.exe"));
    }

    #[test]
    fn every_status_has_a_text() {
        hp_app::register_translations();
        crate::dispatch::register_translations();
        for s in [
            None,
            Some(Status::NotInstalled),
            Some(Status::NotConfigured),
            Some(Status::Configured),
            Some(Status::OtherExe("x".into())),
            Some(Status::Error("e".into())),
        ] {
            let (text, ok) = status_text(s.as_ref());
            assert!(!text.is_empty() && !text.starts_with("mcp."), "{text}");
            assert_eq!(ok, s == Some(Status::Configured));
        }
    }
}
