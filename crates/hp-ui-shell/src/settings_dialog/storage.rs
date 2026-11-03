use egui::RichText;
use hp_app::extend::{SettingsPage, page_order};
use hp_app::storage::{Entry, Risk, format_bytes};
use hp_app::{AppState, t, tf};

use crate::common::heading;

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(StoragePage)]
}

struct StoragePage;

impl SettingsPage for StoragePage {
    fn id(&self) -> &'static str {
        "storage"
    }

    fn label(&self) -> String {
        t!("storage.title").to_owned()
    }

    fn icon(&self) -> &'static str {
        "💾"
    }

    fn order(&self) -> i32 {
        page_order::GENERAL + 70
    }

    fn can_reset(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        storage_ui(ui, state);
    }
}

fn compile_busy(state: &AppState) -> bool {
    state.runtime.compile.progress.is_some() || state.runtime.compile.background.is_some()
}

pub(crate) fn storage_ui(ui: &mut egui::Ui, state: &mut AppState) {
    state.runtime.storage.poll();
    if state.runtime.storage.report.is_none() && !state.runtime.storage.busy() {
        state.runtime.storage.start_scan();
    }
    if state.runtime.storage.busy() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
    }
    heading(ui, t!("storage.heading"));
    ui.label(t!("storage.intro"));
    ui.add_space(6.0);
    header(ui, state);
    ui.add_space(6.0);
    results(ui, state);
    list(ui, state);
    confirm_window(ui.ctx(), state);
}

fn header(ui: &mut egui::Ui, state: &mut AppState) {
    let busy = state.runtime.storage.busy();
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!busy, egui::Button::new(t!("storage.refresh"))).clicked() {
            state.runtime.storage.start_scan();
        }
        let (count, freeable) = selection_totals(state);
        let label = if count == 0 {
            t!("storage.delete_selected_none").to_owned()
        } else {
            tf!("storage.delete_selected", count, format_bytes(freeable))
        };
        if ui.add_enabled(!busy && count > 0, egui::Button::new(label)).clicked() {
            let ids: Vec<String> = state.runtime.storage.selected.iter().cloned().collect();
            state.runtime.storage.confirm = Some(ids);
        }
        if ui.add_enabled(!busy, egui::Button::new(t!("storage.select_all"))).clicked()
            && let Some(report) = &state.runtime.storage.report
        {
            let ids: Vec<String> =
                report.entries.iter().filter(|e| e.freeable() > 0).map(|e| e.id.to_owned()).collect();
            state.runtime.storage.selected.extend(ids);
        }
        if ui.add_enabled(!busy, egui::Button::new(t!("storage.select_none"))).clicked() {
            state.runtime.storage.selected.clear();
        }
    });
    ui.add_space(4.0);
    if let Some((done, total, files)) = state.runtime.storage.progress() {
        let key = if state.runtime.storage.deleting() { "storage.deleting" } else { "storage.scanning" };
        let fraction = if total == 0 { 0.0 } else { done as f32 / total as f32 };
        ui.add(egui::ProgressBar::new(fraction).desired_width(360.0).text(tf!(key, done, total, files)));
    } else if let Some(report) = &state.runtime.storage.report {
        ui.label(RichText::new(tf!("storage.total", format_bytes(report.total_bytes), report.total_files)).strong());
    }
}

fn selection_totals(state: &AppState) -> (usize, u64) {
    let rt = &state.runtime.storage;
    let Some(report) = &rt.report else { return (0, 0) };
    let picked = report.entries.iter().filter(|e| rt.selected.contains(e.id));
    picked.fold((0, 0), |(n, b), e| (n + 1, b + e.freeable()))
}

fn results(ui: &mut egui::Ui, state: &mut AppState) {
    if let Some(notice) = state.runtime.storage.notice.clone() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(notice).color(ui.visuals().warn_fg_color));
            if ui.small_button(t!("storage.result.dismiss")).clicked() {
                state.runtime.storage.notice = None;
            }
        });
    }
    let Some(summary) = &state.runtime.storage.summary else { return };
    let mut hide = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(t!("storage.result.title")).strong());
            if ui.small_button(t!("storage.result.dismiss")).clicked() {
                hide = true;
            }
        });
        if let Some(why) = &summary.failure {
            ui.label(RichText::new(why).color(ui.visuals().error_fg_color));
        } else {
            ui.label(tf!("storage.result.freed", format_bytes(summary.freed), summary.deleted));
        }
        if !summary.errors.is_empty() {
            ui.label(
                RichText::new(tf!("storage.result.errors", summary.errors.len())).color(ui.visuals().warn_fg_color),
            );
            egui::ScrollArea::vertical().max_height(110.0).id_salt("hp_shell_storage_errors").show(ui, |ui| {
                for e in &summary.errors {
                    ui.label(RichText::new(format!("{}: {}", e.path.display(), e.error)).small().weak());
                }
            });
        }
    });
    if hide {
        state.runtime.storage.summary = None;
    }
    ui.add_space(6.0);
}

fn first_existing(entry: &Entry) -> Option<&std::path::Path> {
    entry.paths.iter().map(std::path::PathBuf::as_path).find(|p| p.is_dir())
}

fn risk_label(risk: Risk) -> Option<&'static str> {
    match risk {
        Risk::Safe => None,
        Risk::Recovery => Some("storage.risk.recovery"),
        Risk::UserData => Some("storage.risk.user"),
    }
}

fn list(ui: &mut egui::Ui, state: &mut AppState) {
    let busy = state.runtime.storage.busy();
    let Some(report) = state.runtime.storage.report.clone() else { return };
    let mut toggle: Option<(&'static str, bool)> = None;
    let mut delete_one: Option<&'static str> = None;
    egui::ScrollArea::vertical().id_salt("hp_shell_storage_list").auto_shrink([false, false]).show(ui, |ui| {
        for entry in &report.entries {
            let mut checked = state.runtime.storage.selected.contains(entry.id);
            let can_delete = !busy && entry.freeable() > 0;
            ui.horizontal(|ui| {
                if ui.add_enabled(can_delete, egui::Checkbox::without_text(&mut checked)).changed() {
                    toggle = Some((entry.id, checked));
                }
                ui.label(RichText::new(t!(entry.name_key)).strong());
                if let Some(key) = risk_label(entry.risk) {
                    ui.label(RichText::new(t!(key)).small().color(ui.visuals().warn_fg_color));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(can_delete, egui::Button::new(t!("storage.delete"))).clicked() {
                        delete_one = Some(entry.id);
                    }
                    let folder = first_existing(entry);
                    if ui.add_enabled(folder.is_some(), egui::Button::new(t!("storage.open_folder"))).clicked()
                        && let Some(p) = folder
                    {
                        hp_app::os::open_folder(p, false);
                    }
                    ui.label(RichText::new(format_bytes(entry.bytes)).strong());
                });
            });
            ui.label(RichText::new(t!(entry.desc_key)).small().weak());
            ui.horizontal(|ui| {
                let fraction = report.fraction(entry.bytes);
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .desired_width((ui.available_width() - 170.0).max(120.0))
                        .text(format!("{:.1}%", f64::from(fraction) * 100.0)),
                );
                let mut detail =
                    if entry.bytes == 0 { t!("storage.empty").to_owned() } else { tf!("storage.files", entry.files) };
                if entry.in_use > 0 {
                    detail = format!("{detail}, {}", tf!("storage.in_use_count", entry.in_use));
                }
                ui.label(RichText::new(detail).small().weak());
            });
            if let Some(first) = entry.paths.first() {
                let shown = if entry.paths.len() > 1 {
                    tf!("storage.more_paths", first.display(), entry.paths.len() - 1)
                } else {
                    first.display().to_string()
                };
                let all: Vec<String> = entry.paths.iter().map(|p| p.display().to_string()).collect();
                ui.add(egui::Label::new(RichText::new(shown).small().monospace().weak()).truncate()).on_hover_text(
                    all.join(
                        "
",
                    ),
                );
            }
            ui.separator();
        }
    });
    if let Some((id, on)) = toggle {
        let selected = &mut state.runtime.storage.selected;
        if on {
            selected.insert(id.to_owned());
        } else {
            selected.remove(id);
        }
    }
    if let Some(id) = delete_one {
        state.runtime.storage.confirm = Some(vec![id.to_owned()]);
    }
}

fn confirm_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(ids) = state.runtime.storage.confirm.clone() else { return };
    let Some(report) = state.runtime.storage.report.clone() else {
        state.runtime.storage.confirm = None;
        return;
    };
    let entries: Vec<&Entry> = ids.iter().filter_map(|id| report.entry(id)).collect();
    let freed: u64 = entries.iter().map(|e| e.freeable()).sum();
    let busy = compile_busy(state) && entries.iter().any(|e| e.uses_compile);
    let mut decision: Option<bool> = None;
    egui::Window::new(t!("storage.confirm.title"))
        .collapsible(false)
        .resizable(false)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.label(t!("storage.confirm.lead"));
            ui.add_space(6.0);
            for e in &entries {
                ui.label(
                    RichText::new(tf!("storage.confirm.row", t!(e.name_key), format_bytes(e.freeable()))).strong(),
                );
                for p in &e.paths {
                    ui.label(RichText::new(p.display().to_string()).small().monospace().weak());
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new(tf!("storage.confirm.total", format_bytes(freed))).strong());
            if entries.iter().any(|e| e.risk == Risk::UserData) {
                ui.add_space(4.0);
                ui.label(RichText::new(t!("storage.confirm.warn_user")).color(ui.visuals().error_fg_color));
            }
            if entries.iter().any(|e| e.risk == Risk::Recovery) {
                ui.add_space(4.0);
                ui.label(RichText::new(t!("storage.confirm.warn_recovery")).color(ui.visuals().warn_fg_color));
            }
            if busy {
                ui.add_space(4.0);
                ui.label(RichText::new(t!("storage.compile_busy")).color(ui.visuals().warn_fg_color));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Button::new(t!("storage.confirm.yes"))).clicked() {
                    decision = Some(true);
                }
                if ui.button(t!("storage.confirm.cancel")).clicked() {
                    decision = Some(false);
                }
            });
        });
    match decision {
        Some(true) => {
            let compile = compile_busy(state);
            if let Err(e) = state.runtime.storage.start_delete(ids, compile) {
                state.runtime.storage.notice = Some(e);
                state.runtime.storage.confirm = None;
            }
        }
        Some(false) => state.runtime.storage.confirm = None,
        None => {}
    }
}
