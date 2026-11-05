use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Duration;

use egui::{RichText, Ui};
use hp_app::settings::{CompilePresetSetting, CompileSettings, Settings};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_compile::CompileEvent;

use super::plan::{self, PrepInput};
use super::snapshot::{self, used_assets};
use crate::util::{fmt_duration_ms, icon_title};

const MAX_VMF_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
enum RowStatus {
    Waiting,
    Running { step: String, fraction: f32 },
    Done { ms: u64 },
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug)]
struct Row {
    vmf: PathBuf,
    preset: CompilePresetSetting,
    status: RowStatus,
}

enum BatchEvent {
    Row(usize, RowStatus),
    Done,
}

pub(crate) struct BatchDialog {
    rows: Vec<Row>,
    worker: Option<(Receiver<BatchEvent>, Arc<AtomicBool>)>,
    pick: Option<Receiver<Option<Vec<PathBuf>>>>,
    copy_to_maps: bool,
}

impl Default for BatchDialog {
    fn default() -> Self {
        Self { rows: Vec::new(), worker: None, pick: None, copy_to_maps: true }
    }
}

impl BatchDialog {
    pub(crate) fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    pub(crate) fn cancel(&mut self) {
        if let Some((_, cancel)) = &self.worker {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.poll(state);
        if self.worker.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if !state.runtime.compile.show_batch {
            return;
        }
        let mut open = true;
        hp_app::chrome::Window::new(icon_title("📋", t!("compile.batch.title")))
            .id(egui::Id::unique("hp_compile_batch"))
            .open(&mut open)
            .default_size([640.0, 360.0])
            .resizable(true)
            .show(ctx, |ui| self.ui(ui, state));
        if !open {
            state.runtime.compile.show_batch = false;
        }
    }

    fn poll(&mut self, state: &mut AppState) {
        if let Some(rx) = &self.pick {
            match rx.try_recv() {
                Ok(picked) => {
                    for p in picked.unwrap_or_default() {
                        if !self.rows.iter().any(|r| super::same_file(&r.vmf, &p)) {
                            let preset = state.settings.compile.preset;
                            self.rows.push(Row { vmf: p, preset, status: RowStatus::Waiting });
                        }
                    }
                    self.pick = None;
                }
                Err(TryRecvError::Disconnected) => self.pick = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let Some((rx, _)) = &self.worker else { return };
        let mut done = false;
        loop {
            match rx.try_recv() {
                Ok(BatchEvent::Row(i, status)) => {
                    if let Some(r) = self.rows.get_mut(i) {
                        r.status = status;
                    }
                }
                Ok(BatchEvent::Done) | Err(TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if done {
            self.worker = None;
            let failed = self.rows.iter().filter(|r| matches!(r.status, RowStatus::Failed(_))).count();
            let ok = self.rows.iter().filter(|r| matches!(r.status, RowStatus::Done { .. })).count();
            let severity = if failed > 0 { Severity::Warning } else { Severity::Success };
            state.notify(severity, tf!("compile.batch.finished", ok, failed));
        }
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let running = self.worker.is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(!running && self.pick.is_none(), egui::Button::new(t!("compile.batch.add"))).clicked() {
                self.pick_maps(ui.ctx(), state);
            }
            if ui.add_enabled(!running, egui::Button::new(t!("compile.batch.clear_done"))).clicked() {
                self.rows.retain(|r| !matches!(r.status, RowStatus::Done { .. }));
            }
            ui.checkbox(&mut self.copy_to_maps, t!("compile.copy_to_maps"));
        });
        ui.separator();
        let mut remove = None;
        egui::ScrollArea::vertical().id_salt("hp_compile_batch_rows").max_height(ui.available_height() - 40.0).show(
            ui,
            |ui| {
                egui::Grid::new("hp_compile_batch_grid").num_columns(4).striped(true).spacing([10.0, 4.0]).show(
                    ui,
                    |ui| {
                        for (i, row) in self.rows.iter_mut().enumerate() {
                            let name =
                                row.vmf.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                            ui.label(name).on_hover_text(row.vmf.display().to_string());
                            ui.add_enabled_ui(!running, |ui| {
                                egui::ComboBox::from_id_salt(("hp_compile_batch_preset", i))
                                    .selected_text(preset_name(row.preset))
                                    .show_ui(ui, |ui| {
                                        for p in [
                                            CompilePresetSetting::Draft,
                                            CompilePresetSetting::Normal,
                                            CompilePresetSetting::Final,
                                        ] {
                                            ui.selectable_value(&mut row.preset, p, preset_name(p));
                                        }
                                    });
                            });
                            match &row.status {
                                RowStatus::Waiting => ui.label(RichText::new(t!("compile.status.pending")).weak()),
                                RowStatus::Running { step, fraction } => ui.add(
                                    hp_app::widgets::progress_bar(*fraction).desired_width(180.0).text(step.as_str()),
                                ),
                                RowStatus::Done { ms } => ui.label(
                                    RichText::new(tf!("compile.batch.done", fmt_duration_ms(*ms)))
                                        .color(theme::success()),
                                ),
                                RowStatus::Failed(e) => ui
                                    .label(RichText::new(t!("compile.status.failed")).color(theme::error()))
                                    .on_hover_text(e.as_str()),
                                RowStatus::Cancelled => ui.label(RichText::new(t!("compile.cancelled")).weak()),
                            };
                            if ui
                                .add_enabled(!running, egui::Button::new("✖").small())
                                .on_hover_text(t!("common.remove"))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                    },
                );
            },
        );
        if let Some(i) = remove {
            self.rows.remove(i);
        }
        ui.separator();
        ui.horizontal(|ui| {
            if running {
                if ui.button(format!("⏹ {}", t!("common.cancel"))).clicked() {
                    self.cancel();
                }
            } else if ui
                .add_enabled(!self.rows.is_empty(), egui::Button::new(format!("▶ {}", t!("compile.batch.run"))))
                .clicked()
            {
                self.run(ui.ctx(), state);
            }
            ui.label(RichText::new(tf!("compile.batch.count", self.rows.len())).weak());
        });
    }

    fn pick_maps(&mut self, ctx: &egui::Context, state: &AppState) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let dir = state.doc.path().and_then(Path::parent).map(Path::to_path_buf);
        let filter = t!("compile.batch.vmf_filter").to_owned();
        let spawned = std::thread::Builder::new().name("hp-compile-batch-pick".into()).spawn(move || {
            let mut d = rfd::FileDialog::new().add_filter(filter, &["vmf"]);
            if let Some(dir) = dir {
                d = d.set_directory(dir);
            }
            let _ = tx.send(d.pick_files());
            ctx.request_repaint();
        });
        if spawned.is_ok() {
            self.pick = Some(rx);
        }
    }

    fn run(&mut self, ctx: &egui::Context, state: &AppState) {
        for r in &mut self.rows {
            if !matches!(r.status, RowStatus::Done { .. }) {
                r.status = RowStatus::Waiting;
            }
        }
        let jobs: Vec<(usize, PathBuf, CompilePresetSetting)> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.status == RowStatus::Waiting)
            .map(|(i, r)| (i, r.vmf.clone(), r.preset))
            .collect();
        let mut settings = state.settings.compile.clone();
        settings.launch_game = false;
        settings.copy_to_maps = self.copy_to_maps;
        let root = super::game_root(state);
        let fs = state.assets.fs().cloned();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let c2 = Arc::clone(&cancel);
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-compile-batch".into()).spawn(move || {
            for (i, vmf, preset) in jobs {
                if c2.load(Ordering::Relaxed) {
                    let _ = tx.send(BatchEvent::Row(i, RowStatus::Cancelled));
                    continue;
                }
                let mut s = settings.clone();
                plan::apply_preset(&mut s, preset);
                let status = compile_one(&vmf, s, root.clone(), fs.clone(), &c2, &tx, i, &repaint);
                let _ = tx.send(BatchEvent::Row(i, status));
                repaint.request_repaint();
            }
            let _ = tx.send(BatchEvent::Done);
            repaint.request_repaint();
        });
        if spawned.is_ok() {
            self.worker = Some((rx, cancel));
        }
    }
}

fn preset_name(p: CompilePresetSetting) -> &'static str {
    match p {
        CompilePresetSetting::Draft => t!("compile.preset.draft"),
        CompilePresetSetting::Normal => t!("compile.preset.normal"),
        CompilePresetSetting::Final => t!("compile.preset.final"),
        CompilePresetSetting::Custom => t!("compile.preset.custom"),
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_one(
    vmf: &Path,
    settings: CompileSettings,
    root: Option<PathBuf>,
    fs: Option<hp_fs::GameFs>,
    cancel: &AtomicBool,
    tx: &Sender<BatchEvent>,
    row: usize,
    repaint: &egui::Context,
) -> RowStatus {
    let t0 = std::time::Instant::now();
    let send = |step: String, fraction: f32| {
        let _ = tx.send(BatchEvent::Row(row, RowStatus::Running { step, fraction }));
        repaint.request_repaint();
    };
    send(t!("compile.preparing").to_owned(), 0.0);
    let target = match snapshot::target_for(
        Some(vmf),
        false,
        &Settings::cache_dir().join("untitled"),
        settings.bsp_beside_vmf,
    ) {
        Ok(t) => t,
        Err(e) => return RowStatus::Failed(e),
    };
    if std::fs::metadata(vmf).map(|m| m.len()).unwrap_or(u64::MAX) > MAX_VMF_BYTES {
        return RowStatus::Failed(tf!("compile.batch.too_large", vmf.display()));
    }
    let mut map = match hp_vmf::load(vmf) {
        Ok(m) => m,
        Err(e) => return RowStatus::Failed(e.to_string()),
    };
    let (materials, models) = used_assets(&map);
    let dir = &target.build_dir;
    if let Err(e) = std::fs::create_dir_all(dir) {
        return RowStatus::Failed(format!("{}: {e}", dir.display()));
    }
    let copy = target.vmf();
    let staged = if settings.bsp_beside_vmf {
        if let Some(parent) = target.source.as_deref().and_then(Path::parent) {
            super::compilers::point_instances_at_the_map_folder(&mut map, parent);
        }
        hp_vmf::save_with(&map, &copy, &hp_vmf::SaveOptions { backup: false, sync: false })
            .map(|_| 0)
            .map_err(|e| e.to_string())
    } else {
        std::fs::copy(vmf, &copy).map_err(|e| e.to_string())
    };
    if let Err(e) = staged {
        return RowStatus::Failed(format!("{}: {e}", copy.display()));
    }
    let output_settings = settings.clone();
    let out = plan::prepare(PrepInput {
        vmf: copy,
        settings,
        gmod_root: root,
        fs,
        materials,
        models,
        cache_dir: Settings::cache_dir().join("pack"),
        content_dir: Settings::cache_dir().join("compile_content"),
    });
    let (mut plan, workdir) = match out.result {
        Ok(p) => p,
        Err(e) => return RowStatus::Failed(e),
    };
    snapshot::publish_bsp_step(&mut plan, &target, &output_settings);
    let steps = plan.steps.len().max(1) as f32;
    let kinds: Vec<hp_compile::StepKind> = plan.steps.iter().map(|s| s.kind).collect();
    let handle = hp_compile::start(plan, &workdir);
    let mut current = 0usize;
    let mut last_error = String::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            handle.cancel();
        }
        match handle.events.recv_timeout(Duration::from_millis(100)) {
            Ok(CompileEvent::StepStarted { index, kind, .. }) => {
                current = index;
                send(super::step_name(kind).to_owned(), index as f32 / steps);
            }
            Ok(CompileEvent::Progress { step, fraction, .. }) => {
                let kind = kinds.get(step).copied().unwrap_or(hp_compile::StepKind::Custom);
                send(super::step_name(kind).to_owned(), (current as f32 + fraction) / steps);
            }
            Ok(CompileEvent::Notice { notice, .. }) if notice.severity() == hp_compile::Severity::Error => {
                last_error = super::notice_text(&notice);
            }
            Ok(CompileEvent::Finished { success, cancelled, leaked, .. }) => {
                handle.wait();
                return if cancelled {
                    RowStatus::Cancelled
                } else if success {
                    RowStatus::Done { ms: u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX) }
                } else if leaked {
                    RowStatus::Failed(t!("compile.batch.leaked").to_owned())
                } else {
                    RowStatus::Failed(last_error)
                };
            }
            Ok(_) => {}
            Err(mpsc_timeout) if mpsc_timeout.is_timeout() => {}
            Err(_) => return RowStatus::Failed(t!("compile.err_prep_crashed").to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_rows_fail_cleanly_without_a_game() {
        crate::register_translations();
        let dir = crate::test_scratch("compile_batch");
        let vmf = dir.join("gm_batch.vmf");
        std::fs::write(&vmf, "world { \"id\" \"1\" \"classname\" \"worldspawn\" }").expect("vmf");
        let (tx, rx) = mpsc::channel();
        let ctx = egui::Context::default();
        let cancel = AtomicBool::new(false);
        let status =
            compile_one(&vmf, CompileSettings::default(), Some(dir.join("no_game")), None, &cancel, &tx, 0, &ctx);
        assert!(matches!(status, RowStatus::Failed(_)), "{status:?}");
        assert!(rx.try_iter().count() >= 1, "progress was reported");
        assert!(dir.join(snapshot::BUILD_DIR).join("gm_batch.vmf").is_file());
        let bad = compile_one(&dir.join("my map.vmf"), CompileSettings::default(), None, None, &cancel, &tx, 0, &ctx);
        assert!(matches!(bad, RowStatus::Failed(_)));
    }

    #[test]
    fn smoke_batch_window() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.runtime.compile.show_batch = true;
        let mut d = BatchDialog::default();
        assert!(d.copy_to_maps);
        d.rows.push(Row {
            vmf: PathBuf::from("C:/maps/gm_a.vmf"),
            preset: CompilePresetSetting::Draft,
            status: RowStatus::Waiting,
        });
        d.rows.push(Row {
            vmf: PathBuf::from("C:/maps/gm_b.vmf"),
            preset: CompilePresetSetting::Final,
            status: RowStatus::Running { step: "Lighting".into(), fraction: 0.5 },
        });
        d.rows.push(Row {
            vmf: PathBuf::from("C:/maps/gm_c.vmf"),
            preset: CompilePresetSetting::Normal,
            status: RowStatus::Failed("x".into()),
        });
        hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(state.runtime.compile.show_batch);
    }
}
