use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use egui::{RichText, Ui};
use hp_app::{AppState, Severity, t, tf};
use hp_sniffer::SnifferSettings;
use hp_sniffer::session::{Level, Outcome, Phase, SnifferHandle};

const MAX_UI_LOG: usize = 6000;

pub const TESTED_BUILD: &str = "x86-64 branch, build 2026.09.17 (10175), Steam build 26229091";

pub fn is_running(state: &AppState) -> bool {
    state.runtime.lab.sniffer.handle.as_ref().is_some_and(|h| !h.is_finished())
}

pub fn start(state: &mut AppState, ctx: Option<&egui::Context>) -> Result<(), String> {
    if is_running(state) {
        return Err("a sniff is already running".to_owned());
    }
    let settings = state.settings.sniffer.clone();
    let repaint = ctx.cloned().map(|c| Box::new(move || c.request_repaint()) as Box<dyn Fn() + Send + Sync>);
    let handle = SnifferHandle::start(settings, repaint)?;
    let rt = &mut state.runtime.lab.sniffer;
    rt.handle = Some(Arc::new(handle));
    rt.log.clear();
    rt.log_cursor = 0;
    rt.outcome = None;
    rt.error = None;
    rt.opened = None;
    rt.follow_log = true;
    state.log(Severity::Info, t!("sniffer.started").to_owned());
    Ok(())
}

pub fn cancel(state: &mut AppState) {
    if let Some(h) = &state.runtime.lab.sniffer.handle {
        h.cancel();
    }
}

pub fn pump(state: &mut AppState, ctx: &egui::Context) {
    let mut finished_now = false;
    let rt = &mut state.runtime.lab.sniffer;
    let Some(handle) = rt.handle.clone() else { return };
    let (lines, next) = handle.log_since(rt.log_cursor);
    if !lines.is_empty() {
        rt.log.extend(lines);
        if rt.log.len() > MAX_UI_LOG {
            let cut = rt.log.len() - MAX_UI_LOG;
            rt.log.drain(..cut);
        }
    }
    rt.log_cursor = next;
    if handle.is_finished() && rt.outcome.is_none() && rt.error.is_none() {
        match handle.outcome() {
            Some(Ok(o)) => {
                rt.outcome = Some(o.clone());
                if state.settings.sniffer.open_when_done && rt.opened.as_ref() != Some(&o.vmf_path) {
                    rt.open_vmf = Some(o.vmf_path.clone());
                    rt.open_content = o.content_dir.clone();
                    rt.opened = Some(o.vmf_path.clone());
                }
                finished_now = true;
            }
            Some(Err(e)) => {
                rt.error = Some(e);
                finished_now = true;
            }
            None => {}
        }
    } else if !handle.is_finished() {
        ctx.request_repaint_after(Duration::from_millis(250));
    }
    if finished_now {
        let rt = &state.runtime.lab.sniffer;
        if let Some(o) = &rt.outcome {
            let msg = tf!("sniffer.finished", o.vmf_path.display(), o.report.vmf.solids, o.report.vmf.entities);
            state.notify(Severity::Success, msg);
        } else if let Some(e) = &rt.error {
            let status = rt.handle.as_ref().map(|h| h.status().phase);
            let msg = if status == Some(Phase::Cancelled) {
                t!("sniffer.cancelled").to_owned()
            } else {
                tf!("sniffer.failed", e)
            };
            state.notify(if status == Some(Phase::Cancelled) { Severity::Info } else { Severity::Error }, msg);
        }
    }
}

pub fn phase_key(p: Phase) -> &'static str {
    match p {
        Phase::Idle => "sniffer.phase.idle",
        Phase::Preflight => "sniffer.phase.preflight",
        Phase::Querying => "sniffer.phase.querying",
        Phase::Launching => "sniffer.phase.launching",
        Phase::WaitingGame => "sniffer.phase.waiting_game",
        Phase::WaitingMap => "sniffer.phase.waiting_map",
        Phase::Capturing => "sniffer.phase.capturing",
        Phase::Verifying => "sniffer.phase.verifying",
        Phase::Rebuilding => "sniffer.phase.rebuilding",
        Phase::Decompiling => "sniffer.phase.decompiling",
        Phase::Extracting => "sniffer.phase.extracting",
        Phase::Finishing => "sniffer.phase.finishing",
        Phase::Done => "sniffer.phase.done",
        Phase::Failed => "sniffer.phase.failed",
        Phase::Cancelled => "sniffer.phase.cancelled",
    }
}

pub fn summary_lines(o: &Outcome) -> Vec<String> {
    let r = &o.report;
    let b = &r.bsp_counts;
    let v = &r.vmf;
    let mut out = Vec::new();
    let source = match r.geometry.as_str() {
        "map file" => t!("sniffer.sum.src_file"),
        "game memory" => t!("sniffer.sum.src_memory"),
        _ => t!("sniffer.sum.src_entities"),
    };
    out.push(tf!("sniffer.sum.geometry", source));
    if r.memory_only && r.geometry == "game memory" {
        out.push(tf!("sniffer.sum.mem_only", r.memory_entities));
    } else if r.memory_only {
        out.push(tf!("sniffer.sum.memory_only", r.memory_entities));
    } else {
        out.push(tf!(
            "sniffer.sum.entities",
            r.memory_entities,
            r.file_entities,
            if r.entities_identical { t!("common.yes") } else { t!("common.no") }
        ));
        out.push(tf!("sniffer.sum.lumps", r.verified_lumps, r.verified_bytes));
    }
    let m = &r.cm;
    let mem = |x: usize| if m.found { x.to_string() } else { "-".to_owned() };
    out.push(tf!("sniffer.sum.brushes", mem(m.brushes), b.brushes, v.solids, b.displacements));
    out.push(tf!("sniffer.sum.sides", mem(m.sides), b.brush_sides, v.sides));
    out.push(tf!("sniffer.sum.nodraw", mem(m.nodraw), b.nodraw_sides, v.nodraw_sides, v.tool_sides, v.sky_sides));
    let mr = &r.memory;
    if mr.found && mr.render_found {
        out.push(tf!(
            "sniffer.sum.render",
            mr.faces,
            mr.texinfo,
            mr.displacements,
            mr.props,
            mr.overlays,
            mr.cubemaps,
            mr.area_portals
        ));
    }
    if !r.cross_check.is_empty() {
        let different = r.cross_check.iter().filter(|c| c.memory != c.file).count();
        out.push(tf!("sniffer.sum.cross", r.cross_check.len(), different));
    }
    if r.unprotect.protected() {
        let u = &r.unprotect;
        out.push(tf!(
            "sniffer.sum.protected",
            u.garbage_names,
            u.restored_from_patch,
            u.guessed,
            u.kept_custom,
            u.unresolved
        ));
    }
    if r.merge.file_used {
        let m = &r.merge;
        out.push(tf!(
            "sniffer.sum.fill",
            m.displacements_from_file,
            m.displacements,
            m.props_from_file,
            m.props,
            m.cubemaps_from_file,
            m.cubemaps,
            m.overlays_from_file,
            m.overlays,
            m.sides_from_file,
            m.sides,
            m.pak_files
        ));
    }
    if m.found && !r.memory_only {
        out.push(tf!(
            "sniffer.sum.cm",
            m.compared.brush_mismatch,
            m.compared.side_plane_mismatch,
            m.compared.material_mismatch_brushes,
            m.compared.box_mismatch
        ));
    }
    out.push(tf!("sniffer.sum.props", v.props, v.overlays, v.cubemaps, v.connections));
    out.push(tf!("sniffer.sum.assets", r.assets.pak_files, r.assets.content_files, r.assets.stock_skipped));
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickKind {
    Output,
    Game,
}

#[derive(Default)]
pub struct SnifferWindow {
    pick: Option<(PickKind, Receiver<Option<PathBuf>>)>,
    copied: bool,
}

fn open_folder(path: &std::path::Path) {
    let target = if path.is_dir() { path.to_path_buf() } else { path.parent().map(PathBuf::from).unwrap_or_default() };
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(target).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

impl SnifferWindow {
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.poll_pick(state);
        if !state.runtime.lab.sniffer.open {
            return;
        }
        let mut open = true;
        let title = format!("📡  {}", t!("sniffer.title"));
        hp_app::chrome::Window::new(title)
            .id(egui::Id::unique("lab_bsp_sniffer"))
            .open(&mut open)
            .default_size([640.0, 700.0])
            .min_size([420.0, 320.0])
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.ui(ui, ctx, state));
            });
        if !open {
            state.runtime.lab.sniffer.open = false;
        }
        self.consent(ctx, state);
    }

    fn consent(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let rt = &mut state.runtime.lab.sniffer;
        if !state.settings.sniffer.consent_seen {
            rt.consent_open = true;
        }
        if !rt.consent_open {
            return;
        }
        let mut accepted = false;
        let mut declined = false;
        egui::Window::new(RichText::new(format!("⚠  {}", t!("sniffer.consent.title"))).strong())
            .id(egui::Id::unique("lab_bsp_sniffer_consent"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(t!("sniffer.consent.body1")).strong());
                ui.add_space(6.0);
                ui.label(t!("sniffer.consent.body2"));
                ui.add_space(6.0);
                ui.label(t!("sniffer.consent.body3"));
                ui.add_space(6.0);
                ui.label(RichText::new(t!("sniffer.consent.body4")).color(hp_app::theme::warning()));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if hp_app::widgets::primary_button(ui, t!("sniffer.consent.accept")).clicked() {
                        accepted = true;
                    }
                    if ui.button(t!("sniffer.consent.decline")).clicked() {
                        declined = true;
                    }
                });
            });
        if accepted {
            state.settings.sniffer.consent_seen = true;
            state.settings_dirty = true;
            state.runtime.lab.sniffer.consent_open = false;
        }
        if declined {
            state.runtime.lab.sniffer.consent_open = false;
            state.runtime.lab.sniffer.open = false;
        }
    }

    fn poll_pick(&mut self, state: &mut AppState) {
        let Some((kind, rx)) = &self.pick else { return };
        match rx.try_recv() {
            Ok(picked) => {
                let kind = *kind;
                self.pick = None;
                if let Some(p) = picked {
                    let text = p.display().to_string();
                    match kind {
                        PickKind::Output => state.settings.sniffer.output_dir = text,
                        PickKind::Game => state.settings.sniffer.gmod_dir = text,
                    }
                    state.settings_dirty = true;
                }
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => self.pick = None,
        }
    }

    fn start_pick(&mut self, kind: PickKind, current: &str, ctx: &egui::Context) {
        if self.pick.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let mut dialog = rfd::FileDialog::new();
        if !current.trim().is_empty() {
            dialog = dialog.set_directory(current.trim());
        }
        let spawned = std::thread::Builder::new().name("hp-sniffer-pick".into()).spawn(move || {
            let _ = tx.send(dialog.pick_folder());
        });
        if spawned.is_ok() {
            self.pick = Some((kind, rx));
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        egui::Frame::group(ui.style()).stroke(egui::Stroke::new(1.5, hp_app::theme::warning())).show(ui, |ui| {
            ui.label(
                RichText::new(format!("⚠  {}", t!("sniffer.warning.title"))).strong().color(hp_app::theme::warning()),
            );
            ui.label(t!("sniffer.warning.body"));
            ui.add_space(2.0);
            ui.label(RichText::new(tf!("sniffer.only_gmod", TESTED_BUILD)).strong());
        });
        ui.add_space(6.0);

        let running = is_running(state);
        let s: &mut SnifferSettings = &mut state.settings.sniffer;
        let mut dirty = false;
        ui.horizontal(|ui| {
            ui.label(t!("sniffer.address"));
            let r = ui.add_enabled(
                !running,
                egui::TextEdit::singleline(&mut s.address).hint_text("141.94.238.112:27015").desired_width(260.0),
            );
            r.clone().on_hover_text(t!("sniffer.address_tip"));
            dirty |= r.changed();
        });
        let auth = ui.add_enabled(
            !running,
            egui::Checkbox::new(&mut s.authorized, RichText::new(t!("sniffer.authorized")).strong()),
        );
        dirty |= auth.changed();
        auth.on_hover_text(t!("sniffer.authorized_tip"));

        let ready = s.authorized && s.consent_seen && s.parsed_address().is_ok();
        ui.horizontal(|ui| {
            let sniff = ui.add_enabled(
                ready && !running,
                egui::Button::new(RichText::new(format!("📡 {}", t!("sniffer.sniff"))).strong())
                    .min_size(egui::vec2(120.0, 28.0)),
            );
            if sniff.clicked() {
                self.copied = false;
                dirty = true;
                state.settings_dirty = true;
                if let Err(e) = start(state, Some(ctx)) {
                    state.notify(Severity::Error, tf!("sniffer.failed", e));
                }
            }
            let s = &state.settings.sniffer;
            if !ready && !running {
                let why = if !s.authorized {
                    t!("sniffer.need_authorized").to_owned()
                } else if let Err(e) = s.parsed_address() {
                    e
                } else {
                    t!("sniffer.need_consent").to_owned()
                };
                ui.label(RichText::new(why).weak());
            }
            if ui.add_enabled(running, egui::Button::new(t!("sniffer.cancel"))).clicked() {
                cancel(state);
            }
        });
        if dirty {
            state.settings_dirty = true;
        }

        ui.add_space(6.0);
        self.status_ui(ui, state);
        self.result_ui(ui, state);
        ui.add_space(6.0);
        self.settings_ui(ui, ctx, state);
        self.log_ui(ui, state);
    }

    fn status_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let rt = &state.runtime.lab.sniffer;
        let Some(handle) = &rt.handle else {
            ui.label(RichText::new(t!("sniffer.idle")).weak());
            return;
        };
        let st = handle.status();
        let color = match st.phase {
            Phase::Done => hp_app::theme::success(),
            Phase::Failed => hp_app::theme::error(),
            Phase::Cancelled => hp_app::theme::warning(),
            _ => hp_app::theme::text(),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t!(phase_key(st.phase))).strong().color(color));
            ui.label(RichText::new(format!("{:.0} s", st.elapsed.as_secs_f32())).weak());
            if let Some(pid) = st.game_pid {
                ui.label(RichText::new(tf!("sniffer.pid", pid)).weak());
            }
            if let Some(map) = &st.map {
                ui.label(RichText::new(tf!("sniffer.map", map)).weak());
            }
        });
        ui.add(hp_app::widgets::progress_bar(st.progress).show_percentage());
        if !st.detail.is_empty() {
            ui.label(RichText::new(&st.detail).weak());
        }
        if let Some(e) = &rt.error
            && st.phase == Phase::Failed
        {
            ui.label(RichText::new(e).color(hp_app::theme::error()));
        }
    }

    fn result_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let Some(o) = state.runtime.lab.sniffer.outcome.clone() else { return };
        ui.add_space(6.0);
        hp_app::widgets::section(ui, t!("sniffer.result"));
        for line in summary_lines(&o) {
            ui.label(line);
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("VMF: {}", o.vmf_path.display())).weak());
        });
        ui.horizontal(|ui| {
            if ui.button(t!("sniffer.open_vmf")).clicked() {
                state.runtime.lab.sniffer.open_vmf = Some(o.vmf_path.clone());
            }
            if ui.button(t!("sniffer.show_folder")).clicked() {
                open_folder(&o.vmf_path);
            }
            if ui.button(if self.copied { t!("sniffer.copied") } else { t!("sniffer.copy_report") }).clicked() {
                ui.ctx().copy_text(o.report.render_text());
                self.copied = true;
            }
        });
    }

    fn settings_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        let running = is_running(state);
        let mut dirty = false;
        let mut want_pick: Option<(PickKind, String)> = None;
        let open = state.runtime.lab.sniffer.show_settings;
        let header = egui::CollapsingHeader::new(RichText::new(t!("sniffer.settings")).strong())
            .id_salt("sniffer_settings")
            .open(Some(open))
            .show(ui, |ui| {
                let s = &mut state.settings.sniffer;
                ui.add_enabled_ui(!running, |ui| {
                    ui.label(RichText::new(t!("sniffer.group.output")).strong());
                    egui::Grid::new("sniffer_out").num_columns(3).show(ui, |ui| {
                        ui.label(t!("sniffer.output_dir"));
                        dirty |= ui
                            .add_sized(
                                [330.0, 22.0],
                                egui::TextEdit::singleline(&mut s.output_dir)
                                    .hint_text(hp_sniffer::settings::default_output_dir().display().to_string()),
                            )
                            .changed();
                        if ui.button("...").clicked() {
                            want_pick = Some((PickKind::Output, s.output_dir.clone()));
                        }
                        ui.end_row();
                        ui.label(t!("sniffer.file_name"));
                        dirty |= ui
                            .add_sized([330.0, 22.0], egui::TextEdit::singleline(&mut s.file_name_template))
                            .on_hover_text(t!("sniffer.file_name_tip"))
                            .changed();
                        ui.end_row();
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("sniffer.group.game")).strong());
                    egui::Grid::new("sniffer_game").num_columns(3).show(ui, |ui| {
                        ui.label(t!("sniffer.gmod_dir"));
                        dirty |= ui
                            .add_sized(
                                [330.0, 22.0],
                                egui::TextEdit::singleline(&mut s.gmod_dir).hint_text(t!("sniffer.auto")),
                            )
                            .changed();
                        if ui.button("...").clicked() {
                            want_pick = Some((PickKind::Game, s.gmod_dir.clone()));
                        }
                        ui.end_row();
                        ui.label(t!("sniffer.launch_options"));
                        dirty |= ui
                            .add_sized([330.0, 22.0], egui::TextEdit::singleline(&mut s.launch_options))
                            .on_hover_text(t!("sniffer.launch_options_tip"))
                            .changed();
                        ui.end_row();
                        ui.label(t!("sniffer.insecure"));
                        let before = s.insecure;
                        egui::ComboBox::from_id_salt("sniffer_insecure")
                            .selected_text(insecure_label(s.insecure))
                            .show_ui(ui, |ui| {
                                for m in hp_sniffer::InsecureMode::ALL {
                                    ui.selectable_value(&mut s.insecure, m, insecure_label(m));
                                }
                            })
                            .response
                            .on_hover_text(t!("sniffer.insecure_tip"));
                        dirty |= before != s.insecure;
                        ui.end_row();
                    });
                    dirty |= ui.checkbox(&mut s.attach_running, t!("sniffer.attach_running")).changed();
                    egui::Grid::new("sniffer_timeouts").num_columns(2).show(ui, |ui| {
                        for (label, value, min, max) in [
                            (t!("sniffer.process_timeout"), &mut s.process_timeout_secs, 10, 3600),
                            (t!("sniffer.load_timeout"), &mut s.load_timeout_secs, 30, 7200),
                            (t!("sniffer.settle"), &mut s.settle_secs, 0, 300),
                            (t!("sniffer.poll"), &mut s.poll_secs, 1, 60),
                        ] {
                            ui.label(label);
                            dirty |= ui.add(egui::DragValue::new(value).range(min..=max).suffix(" s")).changed();
                            ui.end_row();
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("sniffer.group.content")).strong());
                    dirty |= ui.checkbox(&mut s.save_models, t!("sniffer.save_models")).changed();
                    dirty |= ui.checkbox(&mut s.save_materials, t!("sniffer.save_materials")).changed();
                    dirty |= ui.checkbox(&mut s.save_sounds, t!("sniffer.save_sounds")).changed();
                    dirty |= ui.checkbox(&mut s.save_other, t!("sniffer.save_other")).changed();
                    dirty |= ui.checkbox(&mut s.only_non_stock, t!("sniffer.only_non_stock")).changed();
                    dirty |= ui.checkbox(&mut s.keep_bsp, t!("sniffer.keep_bsp")).changed();
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("sniffer.group.decompile")).strong());
                    let d = &mut s.decompile;
                    ui.horizontal_wrapped(|ui| {
                        dirty |= ui.checkbox(&mut d.brushes, t!("sniffer.dc.brushes")).changed();
                        dirty |= ui.checkbox(&mut d.displacements, t!("sniffer.dc.displacements")).changed();
                        dirty |= ui.checkbox(&mut d.entities, t!("sniffer.dc.entities")).changed();
                        dirty |= ui.checkbox(&mut d.brush_entities, t!("sniffer.dc.brush_entities")).changed();
                        dirty |= ui.checkbox(&mut d.static_props, t!("sniffer.dc.static_props")).changed();
                        dirty |= ui.checkbox(&mut d.overlays, t!("sniffer.dc.overlays")).changed();
                        dirty |= ui.checkbox(&mut d.cubemaps, t!("sniffer.dc.cubemaps")).changed();
                        dirty |= ui.checkbox(&mut d.areaportals, t!("sniffer.dc.areaportals")).changed();
                        dirty |= ui.checkbox(&mut d.occluders, t!("sniffer.dc.occluders")).changed();
                        dirty |= ui.checkbox(&mut d.group_details, t!("sniffer.dc.group_details")).changed();
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("sniffer.group.behavior")).strong());
                    dirty |= ui.checkbox(&mut s.kill_game, t!("sniffer.kill_game")).changed();
                    dirty |= ui.checkbox(&mut s.auto_disconnect, t!("sniffer.auto_disconnect")).changed();
                    dirty |= ui.checkbox(&mut s.open_when_done, t!("sniffer.open_when_done")).changed();
                });
            });
        let _ = ctx;
        if header.header_response.clicked() {
            state.runtime.lab.sniffer.show_settings = !open;
        }
        if dirty {
            state.settings.sniffer = state.settings.sniffer.clone().clamped();
            state.settings_dirty = true;
        }
        if let Some((kind, current)) = want_pick {
            self.start_pick(kind, &current, ui.ctx());
        }
    }

    fn log_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let open = state.runtime.lab.sniffer.show_log;
        let header = egui::CollapsingHeader::new(RichText::new(t!("sniffer.log")).strong())
            .id_salt("sniffer_log")
            .open(Some(open))
            .show(ui, |ui| {
                let rt = &state.runtime.lab.sniffer;
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .stick_to_bottom(rt.follow_log)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for l in &rt.log {
                            let color = match l.level {
                                Level::Info => hp_app::theme::text_weak(),
                                Level::Warn => hp_app::theme::warning(),
                                Level::Error => hp_app::theme::error(),
                            };
                            ui.label(
                                RichText::new(format!("[{:6.1}] {}", l.seconds, l.text))
                                    .monospace()
                                    .size(11.0)
                                    .color(color),
                            );
                        }
                    });
                if ui.button(t!("sniffer.copy_log")).clicked() {
                    let text: String = rt.log.iter().map(|l| format!("[{:.1}] {}\n", l.seconds, l.text)).collect();
                    ui.ctx().copy_text(text);
                }
            });
        if header.header_response.clicked() {
            state.runtime.lab.sniffer.show_log = !open;
        }
    }
}

fn insecure_label(m: hp_sniffer::InsecureMode) -> &'static str {
    match m {
        hp_sniffer::InsecureMode::Auto => t!("sniffer.insecure.auto"),
        hp_sniffer::InsecureMode::Always => t!("sniffer.insecure.always"),
        hp_sniffer::InsecureMode::Never => t!("sniffer.insecure.never"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::i18n::{Lang, has_key, tr_in};

    const PHASES: [Phase; 15] = [
        Phase::Idle,
        Phase::Preflight,
        Phase::Querying,
        Phase::Launching,
        Phase::WaitingGame,
        Phase::WaitingMap,
        Phase::Capturing,
        Phase::Verifying,
        Phase::Rebuilding,
        Phase::Decompiling,
        Phase::Extracting,
        Phase::Finishing,
        Phase::Done,
        Phase::Failed,
        Phase::Cancelled,
    ];

    #[test]
    fn every_phase_has_a_label_in_both_languages() {
        crate::register_translations();
        for p in PHASES {
            let key = phase_key(p);
            assert!(has_key(key), "{key}");
            assert_ne!(tr_in(key, Lang::En), tr_in(key, Lang::Fr), "{key} is not translated");
        }
    }

    #[test]
    fn the_whole_panel_is_translated() {
        crate::register_translations();
        for (key, en, fr) in crate::tr_sniffer::TR {
            assert!(!en.is_empty() && !fr.is_empty(), "{key}");
            assert!(!en.contains('\u{2014}') && !fr.contains('\u{2014}') && !en.contains('\u{2013}'), "{key}");
            let holes = |s: &str| (0..8).filter(|n| s.contains(&format!("{{{n}}}"))).count();
            assert_eq!(holes(en), holes(fr), "{key} has different placeholders");
        }
    }

    #[test]
    fn the_summary_names_the_memory_counts() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut report = hp_sniffer::report::SniffReport::default();
        report.cm.found = true;
        report.cm.brushes = 8135;
        report.cm.sides = 56446;
        report.cm.nodraw = 28152;
        report.bsp_counts.brushes = 8135;
        report.vmf.solids = 9521;
        report.memory_entities = 2983;
        report.file_entities = 2983;
        report.entities_identical = true;
        report.memory = hp_sniffer::report::MemoryRecovery {
            found: true,
            render_found: true,
            faces: 33049,
            texinfo: 7654,
            displacements: 1386,
            props: 4104,
            overlays: 109,
            cubemaps: 307,
            area_portals: 125,
            ..hp_sniffer::report::MemoryRecovery::default()
        };
        report.map_file_mode = "never".to_owned();
        report.merge = hp_sniffer::merge::MergeStats {
            file_used: true,
            displacements: 4,
            displacements_from_file: 3,
            ..Default::default()
        };
        report.memory_only = true;
        report.geometry = "game memory".to_owned();
        let outcome = Outcome {
            vmf_path: PathBuf::from("a.vmf"),
            bsp_path: None,
            content_dir: None,
            report_path: PathBuf::from("a.txt"),
            report,
        };
        let lines = summary_lines(&outcome);
        assert!(lines.iter().any(|l| l.contains("8135") && l.contains("9521")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("28152")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("2983")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("33049") && l.contains("1386") && l.contains("4104")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("No map file was read")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("Filled in from the map file: displacement data 3 of 4")), "{lines:?}");
        assert!(!lines.iter().any(|l| l.contains("Game memory against the map file")), "{lines:?}");
    }

    #[test]
    fn starting_without_consent_is_refused() {
        let mut state = hp_app::testing::test_state();
        assert!(!is_running(&state));
        assert!(start(&mut state, None).is_err());
        cancel(&mut state);
        let ctx = egui::Context::default();
        pump(&mut state, &ctx);
        assert!(state.runtime.lab.sniffer.handle.is_none());
    }
}
