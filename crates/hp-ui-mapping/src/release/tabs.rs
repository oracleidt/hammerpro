use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;

use egui::{Color32, RichText, Ui};
use hp_app::runtime::CaptureState;
use hp_app::settings::ReleaseMode;
use hp_app::theme;
use hp_app::{Action, AppState, t, tf};
use hp_compile::Severity as LogSeverity;

use super::checklist::{Fix, Status};
use super::pipeline::{NavOutcome, Stage, fmt_bytes};
use super::publish::{self, PublishKind};
use super::{PublishJob, ReleaseWindow, Tab};

pub(super) fn log_color(s: LogSeverity) -> Color32 {
    match s {
        LogSeverity::Error => theme::error(),
        LogSeverity::Warning => theme::warning(),
        LogSeverity::Success => theme::success(),
        _ => theme::info(),
    }
}

pub(super) fn log_view(ui: &mut Ui, id: &str, lines: &[(LogSeverity, String)], height: f32) {
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .max_height(height.max(80.0))
        .stick_to_bottom(true)
        .show_rows(ui, row_h, lines.len(), |ui, range| {
            for (sev, text) in &lines[range] {
                ui.add(egui::Label::new(RichText::new(text).monospace().color(log_color(*sev))).extend());
            }
        });
}

impl ReleaseWindow {
    pub(super) fn build_tab(&mut self, ui: &mut Ui, state: &mut AppState) {
        let running = self.running();
        let Some((map, _vmf)) = Self::map_of(state) else {
            ui.label(t!("release.save_first"));
            if ui.button(t!("release.save_now")).clicked() {
                state.request(Action::SaveMapAs);
            }
            return;
        };
        let out = self.out_root(state).unwrap_or_default();
        egui::Grid::new("hp_release_paths").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(t!("release.map"));
            ui.label(RichText::new(&map).strong());
            ui.end_row();
            ui.label(t!("release.output"));
            let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("release.open_folder")).clicked() {
                    super::open_folder(&out);
                }
                if self.side.output_dir.is_some()
                    && ui.add_enabled(!running, egui::Button::new(t!("release.default_folder"))).clicked()
                {
                    self.side.output_dir = None;
                    self.save_sidecar(state);
                }
                if ui.add_enabled(!running, egui::Button::new(t!("release.change_folder"))).clicked()
                    && self.pick_folder.is_none()
                {
                    let (tx, rx) = mpsc::channel();
                    let start = out.parent().map(Path::to_path_buf);
                    let _ = std::thread::Builder::new().name("hp-release-folder".into()).spawn(move || {
                        let mut d = rfd::FileDialog::new();
                        if let Some(s) = start {
                            d = d.set_directory(s);
                        }
                        let _ = tx.send(d.pick_folder());
                    });
                    self.pick_folder = Some(rx);
                }
                let w = ui.available_width().max(40.0);
                super::report::cell(
                    ui,
                    w,
                    18.0,
                    egui::Label::new(RichText::new(out.display().to_string()).monospace()).truncate(),
                )
                .on_hover_text(out.display().to_string());
            });
            ui.end_row();
        });
        ui.add_space(4.0);
        let s = &mut state.settings.content;
        let mut changed = false;
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal(|ui| {
                ui.label(t!("release.mode"));
                for (mode, key, tip) in [
                    (ReleaseMode::WorkshopAddon, "release.mode_addon", "release.mode_addon_tip"),
                    (ReleaseMode::SelfContained, "release.mode_bsp", "release.mode_bsp_tip"),
                    (ReleaseMode::ServerKit, "release.mode_server", "release.mode_server_tip"),
                ] {
                    changed |= ui.radio_value(&mut s.mode, mode, t!(key)).on_hover_text(t!(tip)).changed();
                }
            });
            egui::Grid::new("hp_release_options").num_columns(2).spacing([24.0, 2.0]).show(ui, |ui| {
                changed |= ui.checkbox(&mut s.compile_first, t!("release.opt_compile")).changed();
                changed |= ui
                    .checkbox(&mut s.compress_pakfile, t!("release.opt_compress"))
                    .on_hover_text(t!("release.opt_compress_tip"))
                    .changed();
                ui.end_row();
                changed |= ui.checkbox(&mut s.keep_cubemaps, t!("release.opt_keep_cubemaps")).changed();
                changed |= ui.checkbox(&mut s.write_gma, t!("release.opt_gma")).changed();
                ui.end_row();
                changed |= ui
                    .checkbox(&mut s.pack_workshop, t!("release.opt_workshop"))
                    .on_hover_text(t!("release.opt_workshop_tip"))
                    .changed();
                changed |= ui
                    .checkbox(&mut s.pack_mounted, t!("release.opt_mounted"))
                    .on_hover_text(t!("release.opt_mounted_tip"))
                    .changed();
                ui.end_row();
                changed |= ui
                    .checkbox(&mut s.game_session, t!("release.opt_game"))
                    .on_hover_text(t!("release.opt_game_tip"))
                    .changed();
                changed |= ui.checkbox(&mut s.open_folder, t!("release.opt_open_folder")).changed();
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.separator();
            changed |= ui
                .checkbox(&mut s.protect, t!("release.opt_protect"))
                .on_hover_text(t!("release.opt_protect_tip"))
                .changed();
            if s.protect {
                ui.indent("hp_protect", |ui| {
                    ui.horizontal(|ui| {
                        ui.label(t!("release.protect_watermark"));
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut s.protect_watermark)
                                    .hint_text(t!("release.protect_watermark_hint"))
                                    .desired_width(220.0),
                            )
                            .changed();
                    });
                    changed |= ui
                        .checkbox(&mut s.protect_strip_original_faces, t!("release.opt_protect_faces"))
                        .on_hover_text(t!("release.opt_protect_faces_tip"))
                        .changed();
                    changed |= ui
                        .checkbox(&mut s.protect_randomize, t!("release.opt_protect_randomize"))
                        .on_hover_text(t!("release.opt_protect_randomize_tip"))
                        .changed();
                    ui.label(RichText::new(t!("release.protect_layout_title")).strong());
                    ui.indent("hp_protect_layout", |ui| {
                        changed |= ui
                            .checkbox(&mut s.protect_shuffle_lumps, t!("release.opt_protect_shuffle"))
                            .on_hover_text(t!("release.opt_protect_shuffle_tip"))
                            .changed();
                        changed |= ui
                            .checkbox(&mut s.protect_junk_gaps, t!("release.opt_protect_gaps"))
                            .on_hover_text(t!("release.opt_protect_gaps_tip"))
                            .changed();
                        changed |= ui
                            .checkbox(&mut s.protect_decoys, t!("release.opt_protect_decoys"))
                            .on_hover_text(t!("release.opt_protect_decoys_tip"))
                            .changed();
                        changed |= ui
                            .checkbox(&mut s.protect_scramble_header, t!("release.opt_protect_header"))
                            .on_hover_text(t!("release.opt_protect_header_tip"))
                            .changed();
                        changed |= ui
                            .checkbox(&mut s.protect_shuffle_game_lumps, t!("release.opt_protect_gamelumps"))
                            .on_hover_text(t!("release.opt_protect_gamelumps_tip"))
                            .changed();
                        changed |= ui
                            .checkbox(&mut s.protect_random_pak, t!("release.opt_protect_pak"))
                            .on_hover_text(t!("release.opt_protect_pak_tip"))
                            .changed();
                        changed |= ui
                            .checkbox(&mut s.protect_reorder_tables, t!("release.opt_protect_tables"))
                            .on_hover_text(t!("release.opt_protect_tables_tip"))
                            .changed();
                        changed |= ui
                            .checkbox(&mut s.protect_reorder_world, t!("release.opt_protect_world"))
                            .on_hover_text(t!("release.opt_protect_world_tip"))
                            .changed();
                        ui.horizontal(|ui| {
                            ui.label(t!("release.protect_seed"));
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut s.protect_layout_seed)
                                        .hint_text(t!("release.protect_seed_hint"))
                                        .desired_width(220.0),
                                )
                                .on_hover_text(t!("release.protect_seed_tip"))
                                .changed();
                        });
                    });
                    changed |= ui
                        .checkbox(&mut s.protect_lock, t!("release.opt_protect_lock"))
                        .on_hover_text(t!("release.opt_protect_lock_tip"))
                        .changed();
                    if s.protect_lock {
                        ui.indent("hp_protect_lock", |ui| {
                            ui.horizontal(|ui| {
                                ui.label(t!("release.protect_lock_key"));
                                changed |= ui
                                    .add(
                                        egui::TextEdit::singleline(&mut s.protect_lock_key)
                                            .password(true)
                                            .hint_text(t!("release.protect_lock_key_hint"))
                                            .desired_width(220.0),
                                    )
                                    .changed();
                            });
                            changed |= ui
                                .checkbox(&mut s.protect_lock_block, t!("release.opt_protect_block"))
                                .on_hover_text(t!("release.opt_protect_block_tip"))
                                .changed();
                            if s.protect_lock_key.trim().is_empty() {
                                ui.label(
                                    RichText::new(t!("release.protect_lock_nokey")).small().color(theme::warning()),
                                );
                            }
                        });
                    }
                    ui.label(RichText::new(t!("release.protect_note")).small().color(theme::warning()));
                });
            }
        });
        if changed {
            state.settings_dirty = true;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if running {
                if ui.button(t!("release.cancel")).clicked() {
                    self.cancel();
                }
                ui.spinner();
            } else {
                let build = ui.add(egui::Button::new(RichText::new(t!("release.build")).strong()));
                if build.clicked() {
                    if state.settings.content.game_session {
                        self.confirm_session = true;
                    } else {
                        self.start(ui.ctx(), state, false);
                    }
                }
                if ui.button(t!("release.analyze")).on_hover_text(t!("release.analyze_tip")).clicked() {
                    self.start(ui.ctx(), state, true);
                }
            }
        });
        self.stages_ui(ui, state);
        if let Some(result) = &self.summary {
            ui.add_space(4.0);
            match result {
                Ok(s) => {
                    let nav = match s.nav {
                        NavOutcome::None => t!("release.nav_none"),
                        NavOutcome::Shipped { .. } => t!("release.nav_shipped"),
                        NavOutcome::Stale => t!("release.nav_stale_short"),
                    };
                    ui.colored_label(
                        theme::success(),
                        tf!(
                            "release.summary",
                            s.pak_files,
                            fmt_bytes(s.pak_bytes),
                            s.addon_files,
                            fmt_bytes(s.gma_bytes),
                            nav,
                            crate::util::fmt_duration_ms(s.duration_ms)
                        ),
                    );
                    if s.up_to_date > 0 {
                        ui.label(tf!("release.summary_up_to_date", s.up_to_date));
                    }
                    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                    ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(t!("release.open_folder")).clicked() {
                            super::open_folder(&s.addon_dir);
                        }
                        let w = ui.available_width().max(40.0);
                        super::report::cell(
                            ui,
                            w,
                            18.0,
                            egui::Label::new(RichText::new(s.bsp.display().to_string()).monospace().weak()).truncate(),
                        )
                        .on_hover_text(s.bsp.display().to_string());
                    });
                }
                Err(e) => {
                    ui.colored_label(theme::error(), e);
                    if self.graph.as_ref().is_some_and(|g| !g.missing.is_empty())
                        && ui.button(t!("release.show_missing")).clicked()
                    {
                        self.tab = Tab::Report;
                    }
                }
            }
        }
        ui.separator();
        let h = ui.available_height();
        log_view(ui, "hp_release_log", &self.log, h);
    }

    fn stages_ui(&mut self, ui: &mut Ui, state: &AppState) {
        if self.job.is_none() && self.stages_done.is_empty() {
            return;
        }
        let s = &state.settings.content;
        let analyze = self.job.as_ref().is_some_and(|j| j.analyze_only);
        let mut stages = vec![Stage::Dependencies];
        if !analyze {
            if s.compile_first {
                stages.push(Stage::Compile);
            }
            if s.game_session {
                stages.push(Stage::GameSession);
            }
            stages.extend([Stage::Pack, Stage::Nav, Stage::Content]);
            if s.write_gma {
                stages.push(Stage::Gma);
            }
        }
        ui.horizontal_wrapped(|ui| {
            for st in stages {
                let (glyph, color) = if self.stages_done.contains(&st) {
                    ("✔", theme::success())
                } else if self.stage == Some(st) {
                    ("▶", theme::warning())
                } else {
                    ("○", theme::info())
                };
                ui.label(RichText::new(format!("{glyph} {}", st.label())).color(color));
            }
        });
        if self.job.is_some() {
            let text = match (self.stage, self.compile_step) {
                (Some(Stage::Compile), Some((kind, _))) => {
                    format!("{} : {}", Stage::Compile.label(), crate::compile::step_name(kind))
                }
                (Some(st), _) => st.label().to_owned(),
                _ => String::new(),
            };
            ui.add(egui::ProgressBar::new(self.progress).text(text).animate(true));
        }
    }

    pub(super) fn checklist_tab(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            if ui.add_enabled(self.facts_job.is_none(), egui::Button::new(t!("release.refresh"))).clicked() {
                self.refresh_facts(ui.ctx(), state, true);
            }
            if self.facts_job.is_some() {
                ui.spinner();
            }
            ui.label(RichText::new(t!("release.checklist_hint")).weak());
        });
        ui.separator();
        let Some(rows) = self.rows.clone() else {
            ui.spinner();
            return;
        };
        egui::ScrollArea::vertical().id_salt("hp_release_checklist").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("hp_release_rows").num_columns(4).striped(true).spacing([10.0, 6.0]).show(ui, |ui| {
                for r in &rows {
                    let (glyph, color) = match r.status {
                        Status::Ok => ("✔", theme::success()),
                        Status::Warn => ("⚠", theme::warning()),
                        Status::Fail => ("✖", theme::error()),
                        Status::Unknown => ("?", theme::info()),
                    };
                    ui.label(RichText::new(glyph).color(color).strong());
                    ui.label(&r.label);
                    ui.label(RichText::new(&r.detail).weak());
                    if let Some(fix) = r.fix {
                        let key = match fix {
                            Fix::Build => "release.fix_build",
                            Fix::Capture => "release.fix_capture",
                            Fix::Addon => "release.fix_addon",
                            Fix::GameSession => "release.fix_game",
                            Fix::Report => "release.fix_report",
                        };
                        if ui.add_enabled(!self.running(), egui::Button::new(t!(key))).clicked() {
                            self.apply_fix(ui, state, fix);
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn apply_fix(&mut self, ui: &Ui, state: &mut AppState, fix: Fix) {
        match fix {
            Fix::Build => {
                state.settings.content.compile_first = true;
                state.settings_dirty = true;
                self.tab = Tab::Build;
                self.start(ui.ctx(), state, false);
            }
            Fix::Capture => state.runtime.content.capture = CaptureState::Requested,
            Fix::Addon => self.tab = Tab::Addon,
            Fix::GameSession => {
                state.settings.content.game_session = true;
                state.settings_dirty = true;
                self.tab = Tab::Build;
            }
            Fix::Report => {
                self.tab = Tab::Report;
                if self.graph.is_none() {
                    self.start(ui.ctx(), state, true);
                }
            }
        }
    }

    pub(super) fn addon_tab(&mut self, ui: &mut Ui, state: &mut AppState) {
        let map = Self::map_of(state).map(|m| m.0).unwrap_or_else(|| state.map_title().to_ascii_lowercase());
        let mut changed = false;
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            egui::Grid::new("hp_release_addon").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(t!("release.addon_title"));
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut self.side.title).hint_text(&map).desired_width(260.0))
                    .changed();
                ui.end_row();
                ui.label(t!("release.addon_type"));
                ui.label(RichText::new("map").monospace());
                ui.end_row();
                ui.label(t!("release.addon_tags"));
                ui.vertical(|ui| {
                    for row in hp_gma::addon_json::ADDON_TAGS.chunks(5) {
                        ui.horizontal(|ui| {
                            for tag in row {
                                let mut on = self.side.tags.iter().any(|t| t == tag);
                                let full = self.side.tags.len() >= hp_gma::addon_json::MAX_TAGS && !on;
                                if ui.add_enabled(!full, egui::Checkbox::new(&mut on, *tag)).changed() {
                                    if on {
                                        self.side.tags.push((*tag).to_owned());
                                    } else {
                                        self.side.tags.retain(|t| t != tag);
                                    }
                                    changed = true;
                                }
                            }
                        });
                    }
                });
                ui.end_row();
                ui.label(t!("release.addon_ignore"));
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut self.ignore_text)
                            .desired_rows(3)
                            .desired_width(260.0)
                            .hint_text("*.psd"),
                    )
                    .on_hover_text(t!("release.addon_ignore_tip"))
                    .changed()
                {
                    self.side.ignore = self
                        .ignore_text
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .take(256)
                        .map(str::to_owned)
                        .collect();
                    changed = true;
                }
                ui.end_row();
                ui.label(t!("release.addon_description"));
                changed |= ui
                    .add(egui::TextEdit::multiline(&mut self.side.description).desired_rows(4).desired_width(260.0))
                    .changed();
                ui.end_row();
            });
            for p in self.addon(&map).problems() {
                ui.colored_label(theme::error(), super::checklist::addon_problem_text(&p));
            }
            let ui = &mut cols[1];
            ui.label(RichText::new(t!("release.thumbnail")).strong());
            match &self.thumb_tex {
                Some(tex) => {
                    let size = egui::vec2(256.0, 256.0);
                    let r = ui.add(egui::Image::new((tex.id(), size)));
                    let inset = egui::Rect::from_min_size(r.rect.min, size).shrink(size.x * 0.1);
                    ui.painter().rect_stroke(
                        inset,
                        0.0,
                        egui::Stroke::new(1.0, Color32::from_white_alpha(90)),
                        egui::StrokeKind::Inside,
                    );
                }
                None => {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(256.0, 256.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        t!("release.no_thumbnail"),
                        egui::FontId::proportional(13.0),
                        theme::info(),
                    );
                }
            }
            ui.horizontal(|ui| {
                if ui.button(t!("release.capture")).on_hover_text(t!("release.capture_tip")).clicked() {
                    state.runtime.content.capture = CaptureState::Requested;
                }
                if ui
                    .add_enabled(
                        self.pick_image.is_none() && self.image_job.is_none(),
                        egui::Button::new(t!("release.choose_image")),
                    )
                    .clicked()
                {
                    let (tx, rx) = mpsc::channel();
                    let filter = t!("release.image_filter").to_owned();
                    let _ = std::thread::Builder::new().name("hp-release-pick".into()).spawn(move || {
                        let _ = tx.send(
                            rfd::FileDialog::new()
                                .add_filter(filter, &["png", "jpg", "jpeg", "tga", "bmp"])
                                .pick_file(),
                        );
                    });
                    self.pick_image = Some(rx);
                }
                if self.thumb.is_some() && ui.button(t!("release.clear_image")).clicked() {
                    self.thumb = None;
                    self.thumb_tex = None;
                    self.facts_key = None;
                }
            });
            ui.label(RichText::new(t!("release.thumbnail_hint")).weak());
        });
        if changed {
            self.save_sidecar(state);
            self.facts_key = None;
        }
    }

    pub(super) fn publish_tab(&mut self, ui: &mut Ui, state: &mut AppState) {
        let Some((map, vmf)) = Self::map_of(state) else {
            ui.label(t!("release.save_first"));
            return;
        };
        let out = self.out_root(state).unwrap_or_default();
        let folder = self.addon_folder(&map);
        let gma = out.join(format!("{folder}.gma"));
        let icon = out.join(format!("{folder}_icon.jpg"));
        let known = self.facts.is_some();
        let exe = self.facts.as_ref().and_then(|f| f.gmpublish.clone());
        let has_gma = self.facts.as_ref().is_some_and(|f| f.gma);
        let has_icon = self.facts.as_ref().is_some_and(|f| f.icon);
        egui::Grid::new("hp_release_publish").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(t!("release.workshop_item"));
            match self.side.workshop_id {
                Some(id) => ui.label(RichText::new(id.to_string()).monospace()),
                None => ui.label(RichText::new(t!("release.not_published")).weak()),
            };
            ui.end_row();
            ui.label(t!("release.gma_file"));
            ui.label(RichText::new(gma.display().to_string()).monospace().color(if has_gma {
                theme::info()
            } else {
                theme::error()
            }));
            ui.end_row();
            ui.label(t!("release.icon_file"));
            ui.label(RichText::new(icon.display().to_string()).monospace().color(if has_icon {
                theme::info()
            } else {
                theme::warning()
            }));
            ui.end_row();
            ui.label(t!("release.change_note"));
            if ui
                .add(egui::TextEdit::multiline(&mut self.side.change_note).desired_rows(3).desired_width(380.0))
                .changed()
            {
                self.save_sidecar(state);
            }
            ui.end_row();
        });
        let ready = exe.is_some() && has_gma && self.publishing.is_none() && !self.running();
        ui.horizontal(|ui| {
            match self.side.workshop_id {
                Some(id) => {
                    if ui.add_enabled(ready, egui::Button::new(t!("release.publish_update"))).clicked() {
                        self.confirm_publish = Some(PublishKind::Update(id));
                    }
                }
                None => {
                    if ui.add_enabled(ready && has_icon, egui::Button::new(t!("release.publish_create"))).clicked() {
                        self.confirm_publish = Some(PublishKind::Create);
                    }
                }
            }
            if let Some(job) = &self.publishing {
                if ui.button(t!("release.cancel")).clicked() {
                    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                ui.spinner();
            }
        });
        if known && exe.is_none() {
            ui.colored_label(theme::warning(), t!("release.no_gmpublish"));
        } else if known && !has_gma {
            ui.label(RichText::new(t!("release.build_first")).weak());
        }
        let _ = vmf;
        ui.separator();
        let h = ui.available_height();
        log_view(ui, "hp_release_publish_log", &self.publish_log, h);
    }

    pub(super) fn modals(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.confirm_session {
            let modal = egui::Modal::new(egui::Id::unique("hp_release_confirm_game")).show(ctx, |ui| {
                ui.set_max_width(440.0);
                ui.heading(t!("release.confirm_game_title"));
                ui.label(t!("release.confirm_game_text"));
                ui.add_space(8.0);
                let mut answer = None;
                ui.horizontal(|ui| {
                    if ui.button(t!("release.confirm_game_start")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(t!("release.cancel")).clicked() {
                        answer = Some(false);
                    }
                });
                answer
            });
            let answer = modal.inner.or_else(|| modal.should_close().then_some(false));
            if let Some(go) = answer {
                self.confirm_session = false;
                if go {
                    self.start(ctx, state, false);
                }
            }
        }
        if let Some(kind) = self.confirm_publish {
            let modal = egui::Modal::new(egui::Id::unique("hp_release_confirm_publish")).show(ctx, |ui| {
                ui.set_max_width(440.0);
                ui.heading(t!("release.confirm_publish_title"));
                ui.label(match kind {
                    PublishKind::Create => t!("release.confirm_publish_create").to_owned(),
                    PublishKind::Update(id) => tf!("release.confirm_publish_update", id),
                });
                ui.add_space(8.0);
                let mut answer = None;
                ui.horizontal(|ui| {
                    if ui.button(t!("release.confirm_publish_go")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(t!("release.cancel")).clicked() {
                        answer = Some(false);
                    }
                });
                answer
            });
            let answer = modal.inner.or_else(|| modal.should_close().then_some(false));
            if let Some(go) = answer {
                self.confirm_publish = None;
                if go {
                    self.start_publish(ctx, state, kind);
                }
            }
        }
    }

    fn start_publish(&mut self, ctx: &egui::Context, state: &mut AppState, kind: PublishKind) {
        let Some((map, vmf)) = Self::map_of(state) else { return };
        let Some(out) = self.out_root(state) else { return };
        let folder = self.addon_folder(&map);
        let gma = out.join(format!("{folder}.gma"));
        let icon = out.join(format!("{folder}_icon.jpg"));
        let Some(exe) = state
            .assets
            .fs()
            .and_then(|fs| fs.root().map(Path::to_path_buf))
            .or_else(|| state.settings.game.gmod_root.clone())
            .and_then(|r| publish::gmpublish_exe(&r))
        else {
            return;
        };
        let args = publish::args(kind, &gma, &icon, &self.side.change_note);
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let repaint = ctx.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || repaint.request_repaint());
        self.publish_log.clear();
        self.publish_log.push((LogSeverity::Info, tf!("release.publish_started", exe.display())));
        let spawned = std::thread::Builder::new()
            .name("hp-release-publish".into())
            .spawn(move || publish::run(exe, args, vmf, tx, c2, wake));
        if spawned.is_ok() {
            self.publishing = Some(PublishJob { rx, cancel });
        }
    }
}
