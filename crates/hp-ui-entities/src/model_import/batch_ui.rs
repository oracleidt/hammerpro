use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_meshio::UpAxis;
use hp_meshio::smd::Collision;

use super::batch::{
    BatchEntry, BatchOptions, FileResult, Formats, GridOptions, MAX_THREADS, Status, Summary, collect_files, names_for,
    run, scan,
};
use super::{SURFACES, Up, content_root, lod_options, remount_after_write};

enum Event {
    File(Box<FileResult>),
    Finished(Vec<FileResult>, Summary),
}

enum Stage {
    Setup,
    Scanning(Receiver<Vec<BatchEntry>>, Arc<AtomicUsize>, usize, Arc<AtomicBool>),
    Running(Receiver<Event>, Arc<AtomicUsize>, usize, Arc<AtomicBool>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub class: &'static str,
    pub items: Vec<(String, (DVec3, DVec3))>,
    pub grid: GridOptions,
}

pub struct ModelBatchDialog {
    open: bool,
    stage: Stage,
    inputs: Vec<PathBuf>,
    recursive: bool,
    formats: Formats,
    opts: BatchOptions,
    up: Up,
    place: bool,
    grid: GridOptions,
    picking: Option<Receiver<Vec<PathBuf>>>,
    files: Vec<PathBuf>,
    entries: Vec<BatchEntry>,
    results: Vec<FileResult>,
    summary: Option<Summary>,
    error: Option<String>,
    placement: Option<Placement>,
    scan_pending: bool,
}

impl Default for ModelBatchDialog {
    fn default() -> Self {
        Self {
            open: false,
            stage: Stage::Setup,
            inputs: Vec::new(),
            recursive: true,
            formats: Formats::default(),
            opts: BatchOptions::default(),
            up: Up::Auto,
            place: false,
            grid: GridOptions::default(),
            picking: None,
            files: Vec::new(),
            entries: Vec::new(),
            results: Vec::new(),
            summary: None,
            error: None,
            placement: None,
            scan_pending: false,
        }
    }
}

impl ModelBatchDialog {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn busy(&self) -> bool {
        !matches!(self.stage, Stage::Setup)
    }

    pub fn take_placement(&mut self) -> Option<Placement> {
        self.placement.take()
    }

    pub fn add_inputs(&mut self, paths: Vec<PathBuf>) {
        for p in paths {
            if !self.inputs.contains(&p) {
                self.inputs.push(p);
            }
        }
        self.open = true;
        self.refresh_files();
        self.scan_pending = true;
    }

    fn refresh_files(&mut self) {
        self.files = collect_files(&self.inputs, self.recursive, self.formats);
        self.entries.clear();
        self.results.clear();
        self.summary = None;
    }

    fn sync_options(&mut self, state: &AppState) {
        self.opts.convert.import.up = match self.up {
            Up::Auto => None,
            Up::Y => Some(UpAxis::Y),
            Up::Z => Some(UpAxis::Z),
        };
        self.opts.convert.lod = lod_options(state.settings.lab.model_lod);
        self.opts.convert.keep_params = state.settings.lab.model_keep_params;
        self.opts.threads = self.opts.threads.clamp(1, MAX_THREADS);
    }

    fn pick(&mut self, ctx: &egui::Context, folder: bool) {
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new();
            let picked = if folder {
                dialog.pick_folders()
            } else {
                dialog.add_filter(t!("entities.mi_filter"), &["fbx", "obj", "gltf", "glb"]).pick_files()
            };
            let _ = tx.send(picked.unwrap_or_default());
            repaint.request_repaint();
        });
        self.picking = Some(rx);
    }

    fn start_scan(&mut self, ctx: &egui::Context, state: &AppState) {
        self.sync_options(state);
        let files = self.files.clone();
        let opts = self.opts.clone();
        let root = content_root(state);
        let (tx, rx) = channel();
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (d, c, repaint) = (done.clone(), cancel.clone(), ctx.clone());
        let total = files.len();
        std::thread::spawn(move || {
            let _ = tx.send(scan(&files, &opts, root.as_deref(), &c, &d));
            repaint.request_repaint();
        });
        self.stage = Stage::Scanning(rx, done, total, cancel);
    }

    fn start_run(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(root) = content_root(state) else {
            self.error = Some(t!("entities.mi_no_game").to_owned());
            return;
        };
        self.error = None;
        self.sync_options(state);
        if hp_compile::sandbox::is_sandboxed() {
            state.log(Severity::Info, tf!("entities.mi_sandboxed", root.display()));
        }
        let files = self.files.clone();
        let names = names_for(&files, &self.opts.template, &self.opts.prefix);
        let opts = self.opts.clone();
        let (tx, rx) = channel();
        let done = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (d, c, repaint) = (done.clone(), cancel.clone(), ctx.clone());
        let total = files.len();
        self.results.clear();
        self.summary = None;
        std::thread::spawn(move || {
            let tx_file = tx.clone();
            let progress = move |r: &FileResult| {
                d.fetch_add(1, Ordering::SeqCst);
                let _ = tx_file.send(Event::File(Box::new(r.clone())));
                repaint.request_repaint();
            };
            let (list, summary) = run(&files, &names, &opts, &root, &c, &progress);
            let _ = tx.send(Event::Finished(list, summary));
        });
        self.stage = Stage::Running(rx, done, total, cancel);
    }

    fn poll(&mut self, state: &mut AppState) {
        if let Some(rx) = &self.picking {
            match rx.try_recv() {
                Ok(paths) => {
                    self.picking = None;
                    if !paths.is_empty() {
                        self.add_inputs(paths);
                    }
                }
                Err(TryRecvError::Disconnected) => self.picking = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        match std::mem::replace(&mut self.stage, Stage::Setup) {
            Stage::Scanning(rx, done, total, cancel) => match rx.try_recv() {
                Ok(entries) => self.entries = entries,
                Err(TryRecvError::Empty) => self.stage = Stage::Scanning(rx, done, total, cancel),
                Err(TryRecvError::Disconnected) => {}
            },
            Stage::Running(rx, done, total, cancel) => {
                let mut finished = false;
                loop {
                    match rx.try_recv() {
                        Ok(Event::File(r)) => self.results.push(*r),
                        Ok(Event::Finished(list, summary)) => {
                            self.finish(state, list, summary);
                            finished = true;
                            break;
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            finished = true;
                            break;
                        }
                    }
                }
                if !finished {
                    self.stage = Stage::Running(rx, done, total, cancel);
                }
            }
            Stage::Setup => {}
        }
    }

    fn finish(&mut self, state: &mut AppState, list: Vec<FileResult>, summary: Summary) {
        for r in &list {
            let severity = match r.status {
                Status::Ok => Severity::Info,
                Status::Warning | Status::Skipped => Severity::Warning,
                Status::Error => Severity::Error,
            };
            state.log(severity, r.line());
        }
        if summary.converted() > 0 {
            remount_after_write(state);
        }
        let severity = if summary.errors > 0 { Severity::Warning } else { Severity::Success };
        state.notify(severity, summary.text());
        if self.place && summary.converted() > 0 {
            let items: Vec<(String, (DVec3, DVec3))> =
                list.iter().filter_map(|r| r.converted.as_ref().map(|c| (c.model.clone(), c.bounds))).collect();
            let class = if self.opts.convert.static_prop { "prop_static" } else { "prop_physics" };
            self.placement = Some(Placement { class, items, grid: self.grid });
        }
        self.results = list;
        self.summary = Some(summary);
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.poll(state);
        if self.scan_pending && matches!(self.stage, Stage::Setup) {
            self.scan_pending = false;
            if !self.files.is_empty() {
                self.start_scan(ctx, state);
            }
        }
        if !self.open {
            return;
        }
        let mut open = true;
        hp_app::chrome::Window::new(t!("entities.mb_title"))
            .id(egui::Id::unique("hp_model_batch"))
            .open(&mut open)
            .collapsible(false)
            .default_width(760.0)
            .show(ctx, |ui| self.ui(ctx, ui, state));
        if !open && !self.busy() {
            self.open = false;
        }
    }

    fn ui(&mut self, ctx: &egui::Context, ui: &mut Ui, state: &mut AppState) {
        let idle = matches!(self.stage, Stage::Setup) && self.picking.is_none();
        ui.add_enabled_ui(idle, |ui| {
            ui.horizontal(|ui| {
                if ui.button(t!("entities.mb_pick_files")).clicked() {
                    self.pick(ctx, false);
                }
                if ui.button(t!("entities.mb_pick_folder")).clicked() {
                    self.pick(ctx, true);
                }
                if ui.button(t!("entities.mb_clear")).clicked() {
                    self.inputs.clear();
                    self.refresh_files();
                }
                ui.label(RichText::new(tf!("entities.mb_inputs", self.files.len())).weak());
            });
            let mut changed = ui.checkbox(&mut self.recursive, t!("entities.mb_recursive")).changed();
            ui.horizontal(|ui| {
                ui.label(t!("entities.mb_formats"));
                changed |= ui.checkbox(&mut self.formats.obj, "OBJ").changed();
                changed |= ui.checkbox(&mut self.formats.fbx, "FBX").changed();
                changed |= ui.checkbox(&mut self.formats.gltf, "glTF / GLB").changed();
            });
            if changed {
                self.refresh_files();
                if !self.files.is_empty() {
                    self.start_scan(ctx, state);
                }
            }
            self.options_ui(ui, state);
        });
        match &self.stage {
            Stage::Scanning(_, done, total, cancel) => {
                let (d, n) = (done.load(Ordering::SeqCst), *total);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tf!("entities.mb_scanning", d, n));
                    if ui.button(t!("entities.mb_cancel")).clicked() {
                        cancel.store(true, Ordering::SeqCst);
                    }
                });
            }
            Stage::Running(_, done, total, cancel) => {
                let (d, n) = (done.load(Ordering::SeqCst), (*total).max(1));
                ui.horizontal(|ui| {
                    ui.add(egui::ProgressBar::new(d as f32 / n as f32).desired_width(360.0).text(tf!(
                        "entities.mb_running",
                        d,
                        n
                    )));
                    if ui.button(t!("entities.mb_cancel")).clicked() {
                        cancel.store(true, Ordering::SeqCst);
                    }
                });
            }
            Stage::Setup => {}
        }
        ui.separator();
        self.table_ui(ui);
        if let Some(e) = &self.error {
            ui.colored_label(theme::error(), e);
        }
        if let Some(s) = &self.summary {
            ui.label(RichText::new(s.text()).strong());
        }
        ui.label(RichText::new(t!("entities.mb_where")).small().weak());
        let idle = matches!(self.stage, Stage::Setup);
        ui.horizontal(|ui| {
            let ready = idle && !self.files.is_empty();
            let label = tf!("entities.mb_start", self.files.len());
            if ui.add_enabled(ready, egui::Button::new(RichText::new(label).strong())).clicked() {
                self.start_run(ctx, state);
            }
            if ui.add_enabled(ready, egui::Button::new(t!("entities.mb_scan"))).clicked() {
                self.start_scan(ctx, state);
            }
            if self.files.is_empty() && !self.inputs.is_empty() {
                ui.colored_label(theme::warning(), t!("entities.mb_no_files"));
            }
            if ui.add_enabled(idle, egui::Button::new(t!("entities.mb_close"))).clicked() {
                self.open = false;
            }
        });
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        egui::Grid::new("hp_mb_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(t!("entities.mb_template"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.opts.template).desired_width(160.0))
                    .on_hover_text(t!("entities.mb_template_tip"));
                ui.label(t!("entities.mb_prefix"));
                ui.add(egui::TextEdit::singleline(&mut self.opts.prefix).desired_width(90.0));
            });
            ui.end_row();
            ui.label(t!("entities.mi_kind"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.opts.convert.static_prop, true, "prop_static");
                ui.radio_value(&mut self.opts.convert.static_prop, false, t!("entities.mi_physics"));
            });
            ui.end_row();
            ui.label(t!("entities.mi_scale"));
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.opts.convert.import.scale)
                        .range(0.001..=1000.0)
                        .speed(0.01)
                        .max_decimals(3),
                );
                ui.checkbox(&mut self.opts.convert.import.convert_units, t!("entities.mi_units"))
                    .on_hover_text(t!("entities.mi_units_tip"));
            });
            ui.end_row();
            ui.label(t!("entities.mi_up"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.up, Up::Auto, t!("entities.mi_up_auto"));
                ui.radio_value(&mut self.up, Up::Y, "Y");
                ui.radio_value(&mut self.up, Up::Z, "Z");
            });
            ui.end_row();
            ui.label(t!("entities.mi_origin"));
            ui.checkbox(&mut self.opts.convert.import.center, t!("entities.mi_center"));
            ui.end_row();
            ui.label(t!("entities.surface_prop"));
            egui::ComboBox::from_id_salt("hp_mb_surface")
                .selected_text(&self.opts.convert.surface_prop)
                .height(360.0)
                .show_ui(ui, |ui| {
                    for sp in SURFACES {
                        ui.selectable_value(&mut self.opts.convert.surface_prop, (*sp).to_owned(), *sp);
                    }
                });
            ui.end_row();
            ui.label(t!("entities.mi_collision"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.opts.convert.collision, Collision::Pieces, t!("entities.mi_coll_pieces"));
                ui.radio_value(&mut self.opts.convert.collision, Collision::Box, t!("entities.mi_coll_box"));
                ui.radio_value(&mut self.opts.convert.collision, Collision::None, t!("entities.mi_coll_none"));
            });
            ui.end_row();
            if !self.opts.convert.static_prop {
                ui.label(t!("entities.mass"));
                ui.add(egui::DragValue::new(&mut self.opts.convert.mass).range(0.1..=50000.0).suffix(" kg"));
                ui.end_row();
            }
            ui.label(t!("entities.mb_threads"));
            ui.add(egui::DragValue::new(&mut self.opts.threads).range(1..=MAX_THREADS));
            ui.end_row();
        });
        if hp_app::widgets::model_lod_ui(ui, &mut state.settings.lab.model_lod) {
            state.settings_dirty = true;
        }
        if ui
            .checkbox(&mut state.settings.lab.model_keep_params, t!("entities.mi_keep_params"))
            .on_hover_text(t!("entities.mi_keep_params_tip"))
            .changed()
        {
            state.settings_dirty = true;
        }
        ui.checkbox(&mut self.opts.overwrite, t!("entities.mb_overwrite"))
            .on_hover_text(t!("entities.mb_overwrite_tip"));
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.place, t!("entities.mb_place"));
            if self.place {
                ui.label(t!("entities.mb_columns"));
                ui.add(egui::DragValue::new(&mut self.grid.columns).range(1..=256));
                ui.label(t!("entities.mb_gap"));
                ui.add(egui::DragValue::new(&mut self.grid.gap).range(0.0..=16384.0).speed(1.0));
            }
        });
    }

    fn table_ui(&self, ui: &mut Ui) {
        let names = names_for(&self.files, &self.opts.template, &self.opts.prefix);
        egui::ScrollArea::vertical().max_height(260.0).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("hp_mb_table").num_columns(6).striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                for h in
                    ["entities.mb_col_file", "entities.mb_col_tris", "entities.mb_col_size", "entities.mb_col_name"]
                {
                    ui.label(RichText::new(hp_app::i18n::tr(h)).strong());
                }
                ui.label(RichText::new(t!("entities.mb_col_warnings")).strong());
                ui.label(RichText::new(t!("entities.mb_col_result")).strong());
                ui.end_row();
                for (i, path) in self.files.iter().enumerate() {
                    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                    let entry = self.entries.iter().find(|e| e.path == *path);
                    ui.label(file).on_hover_text(path.display().to_string());
                    match entry {
                        Some(e) if e.error.is_none() => {
                            ui.label(e.triangles.to_string());
                            ui.label(format!("{:.1} x {:.1} x {:.1}", e.size.x, e.size.y, e.size.z));
                        }
                        _ => {
                            ui.label("-");
                            ui.label("-");
                        }
                    }
                    let name = names.get(i).cloned().unwrap_or_default();
                    if entry.is_some_and(|e| e.exists) {
                        ui.colored_label(theme::warning(), name).on_hover_text(t!("entities.mi_exists"));
                    } else {
                        ui.label(name);
                    }
                    match entry {
                        Some(e) => match &e.error {
                            Some(err) => cell(ui, err, theme::error()),
                            None if e.warnings.is_empty() => {
                                ui.label("");
                            }
                            None => cell(ui, &e.warnings.join("; "), theme::warning()),
                        },
                        None => {
                            ui.label("");
                        }
                    }
                    match self.results.iter().find(|r| r.path == *path) {
                        Some(r) => {
                            let color = match r.status {
                                Status::Ok => theme::success(),
                                Status::Warning | Status::Skipped => theme::warning(),
                                Status::Error => theme::error(),
                            };
                            let text = hp_app::i18n::tr(match r.status {
                                Status::Ok => "entities.mb_ok",
                                Status::Warning => "entities.mb_warning",
                                Status::Error => "entities.mb_error",
                                Status::Skipped => "entities.mb_skipped",
                            });
                            ui.colored_label(color, text).on_hover_text(r.line());
                        }
                        None => {
                            ui.label("");
                        }
                    }
                    ui.end_row();
                }
            });
        });
    }
}

fn cell(ui: &mut Ui, text: &str, color: egui::Color32) {
    ui.scope(|ui| {
        ui.set_max_width(300.0);
        ui.add(egui::Label::new(RichText::new(text).color(color)).truncate()).on_hover_text(text);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_collects_inputs_and_opens() {
        crate::register_translations();
        let dir = std::env::temp_dir().join(format!("hp_batch_ui_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.obj"), "x").unwrap();
        let mut d = ModelBatchDialog::default();
        assert!(!d.is_open() && !d.busy());
        d.add_inputs(vec![dir.clone(), dir.clone()]);
        assert!(d.is_open());
        assert_eq!(d.inputs.len(), 1);
        assert_eq!(d.files.len(), 1);
        d.formats.obj = false;
        d.refresh_files();
        assert!(d.files.is_empty());
        assert!(d.take_placement().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
