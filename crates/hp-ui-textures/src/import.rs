use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use hp_app::state::ImportOptions;
use hp_app::{AppState, Severity, t, tf, theme};

use crate::util::{PendingPick, folder_label};

const FRAME_BUDGET: Duration = Duration::from_millis(30);

type ImportWorker =
    (std::sync::mpsc::Receiver<(PathBuf, Vec<Result<String, String>>)>, std::sync::Arc<std::sync::atomic::AtomicBool>);

struct ImportJob {
    worker: Option<ImportWorker>,
    done: usize,
    queue: VecDeque<PathBuf>,
    total: usize,
    opts: ImportOptions,
    created: Vec<String>,
    failed: Vec<(PathBuf, String)>,
    current: Option<PathBuf>,
}

pub struct ImportDialog {
    files: Vec<PathBuf>,
    dest: usize,
    surface_prop: String,
    mipmaps: bool,
    overwrite: bool,
    job: Option<ImportJob>,
    pending_folder: Option<PendingPick<PathBuf>>,
    target: Option<(PathBuf, PathBuf, String)>,
    escape: crate::util::DialogEscape,
}

impl Default for ImportDialog {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            dest: 0,
            surface_prop: String::new(),
            mipmaps: true,
            overwrite: false,
            job: None,
            pending_folder: None,
            target: None,
            escape: crate::util::DialogEscape::default(),
        }
    }
}

impl ImportDialog {
    pub fn is_active(&self) -> bool {
        !self.files.is_empty() || self.job.is_some()
    }

    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    pub fn open(&mut self, files: Vec<PathBuf>) {
        if self.job.is_none() {
            self.files = crate::util::sorted_paths(files);
        }
    }

    pub fn set_destination(&mut self, index: usize) {
        self.dest = index;
    }

    fn target(&mut self, state: &AppState) -> Option<(PathBuf, String)> {
        let dir = state.settings.game.custom_texture_dirs.get(self.dest)?;
        if let Some((p, dest, prefix)) = &self.target
            && crate::util::same_path(p, &dir.path)
        {
            return Some((dest.clone(), prefix.clone()));
        }
        let layout = crate::folders::layout_of(state, dir);
        let (dest, prefix) = crate::folders::import_target(&dir.path, &layout);
        self.target = Some((dir.path.clone(), dest.clone(), prefix.clone()));
        Some((dest, prefix))
    }

    pub fn start(&mut self, state: &mut AppState) -> bool {
        let Some((dest_dir, prefix)) = self.target(state) else { return false };
        if self.files.is_empty() {
            return false;
        }
        let sp = self.surface_prop.trim();
        let opts = ImportOptions {
            dest_dir,
            prefix,
            surface_prop: (!sp.is_empty()).then(|| sp.to_owned()),
            generate_mipmaps: self.mipmaps,
            overwrite: self.overwrite,
        };
        let files = std::mem::take(&mut self.files);
        let worker = state.assets.image_converter().and_then(|convert| {
            let (tx, rx) = std::sync::mpsc::channel();
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (list, opts, stop2) = (files.clone(), opts.clone(), stop.clone());
            std::thread::Builder::new()
                .name("hp-image-import".to_owned())
                .spawn(move || {
                    for file in list {
                        if stop2.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        let results = convert(std::slice::from_ref(&file), &opts);
                        if tx.send((file, results)).is_err() {
                            break;
                        }
                    }
                })
                .ok()
                .map(|_| (rx, stop))
        });
        self.job = Some(ImportJob {
            worker,
            done: 0,
            total: files.len(),
            queue: files.into(),
            opts,
            created: Vec::new(),
            failed: Vec::new(),
            current: None,
        });
        true
    }

    pub fn step(&mut self, state: &mut AppState) -> bool {
        let Some(job) = &mut self.job else { return false };
        if let Some((rx, _)) = &job.worker {
            while let Ok((file, results)) = rx.try_recv() {
                job.done += 1;
                job.queue.retain(|f| *f != file);
                if results.is_empty() {
                    job.failed.push((file.clone(), t!("tex.import.no_result").to_owned()));
                }
                for r in results {
                    match r {
                        Ok(name) if !name.is_empty() => job.created.push(name),
                        Ok(_) => job.failed.push((file.clone(), t!("tex.import.no_result").to_owned())),
                        Err(e) => job.failed.push((file.clone(), e)),
                    }
                }
                job.current = job.queue.front().cloned();
            }
            if job.done < job.total {
                return false;
            }
        }
        let start = Instant::now();
        while let Some(file) = job.queue.pop_front() {
            let results = state.assets.import_images(std::slice::from_ref(&file), &job.opts);
            if results.is_empty() {
                job.failed.push((file.clone(), t!("tex.import.no_result").to_owned()));
            }
            for r in results {
                match r {
                    Ok(name) if !name.is_empty() => job.created.push(name),
                    Ok(_) => job.failed.push((file.clone(), t!("tex.import.no_result").to_owned())),
                    Err(e) => job.failed.push((file.clone(), e)),
                }
            }
            job.current = job.queue.front().cloned();
            if start.elapsed() >= FRAME_BUDGET {
                break;
            }
        }
        if !job.queue.is_empty() {
            return false;
        }
        let Some(job) = self.job.take() else { return false };
        for (file, e) in &job.failed {
            state.log(Severity::Error, tf!("tex.import.file_failed", file.display(), e));
        }
        if job.created.is_empty() {
            state.notify(Severity::Error, tf!("tex.import.none", job.failed.len()));
        } else {
            let sev = if job.failed.is_empty() { Severity::Success } else { Severity::Warning };
            state.notify(sev, tf!("tex.import.done", job.created.len(), job.failed.len()));
            if let Some(first) = job.created.first() {
                state.use_material(first);
            }
            state.assets.remount(&state.settings);
        }
        true
    }

    fn cancel_job(&mut self, state: &mut AppState) {
        if let Some(job) = self.job.take() {
            if let Some((_, stop)) = &job.worker {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            state.notify(Severity::Warning, tf!("tex.import.cancelled", job.created.len(), job.total));
            if !job.created.is_empty() {
                state.assets.remount(&state.settings);
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(req) = crate::shared::take_import(ctx) {
            self.open(req.0);
        }
        if let Some(p) = &self.pending_folder
            && let Some(result) = p.poll()
        {
            self.pending_folder = None;
            if let Some(path) = result {
                let added = crate::folders::add_custom_dir(&mut state.settings, &path);
                state.settings_dirty = true;
                self.dest = added.index();
                state.assets.remount(&state.settings);
            }
        }
        if self.job.is_some() {
            self.step(state);
            ctx.request_repaint();
        }
        if !self.is_active() {
            return;
        }
        let mut open = true;
        self.escape.begin(ctx);
        let response = hp_app::chrome::Window::new(t!("tex.import.title"))
            .id(egui::Id::unique("hp_textures_import"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| self.ui(ui, state));
        let escape = self.escape.update(ctx, response.as_ref().map(|r| &r.response));
        if !open || escape {
            if self.job.is_some() {
                self.cancel_job(state);
            }
            self.files.clear();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if let Some(job) = &self.job {
            let done = job.total - job.queue.len();
            let frac = if job.total == 0 { 1.0 } else { done as f32 / job.total as f32 };
            ui.label(tf!("tex.import.progress", done, job.total));
            ui.add(hp_app::widgets::progress_bar(frac).show_percentage().desired_width(ui.available_width()));
            if let Some(cur) = &job.current {
                ui.weak(cur.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            }
            let mut cancel = false;
            ui.horizontal(|ui| cancel = ui.button(t!("common.cancel")).clicked());
            if cancel {
                self.cancel_job(state);
            }
            return;
        }

        ui.label(tf!("tex.import.count", self.files.len()));
        egui::ScrollArea::vertical().max_height(90.0).id_salt("hp_import_files").show(ui, |ui| {
            for f in self.files.iter().take(500) {
                ui.weak(f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
                    .on_hover_text(f.display().to_string());
            }
        });
        ui.separator();

        let dirs_len = state.settings.game.custom_texture_dirs.len();
        egui::Grid::new("hp_import_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(t!("tex.import.dest"));
            ui.horizontal(|ui| {
                if dirs_len == 0 {
                    ui.colored_label(theme::warning(), t!("tex.import.no_folder"));
                } else {
                    self.dest = self.dest.min(dirs_len - 1);
                    let dirs = &state.settings.game.custom_texture_dirs;
                    let current = dirs.get(self.dest).map(|d| folder_label(&d.path)).unwrap_or_default();
                    egui::ComboBox::from_id_salt("hp_import_dest").selected_text(current).width(200.0).show_ui(
                        ui,
                        |ui| {
                            for (i, d) in dirs.iter().enumerate() {
                                ui.selectable_value(&mut self.dest, i, folder_label(&d.path))
                                    .on_hover_text(d.path.display().to_string());
                            }
                        },
                    );
                }
                let picking = self.pending_folder.is_some();
                if ui.add_enabled(!picking, egui::Button::new(t!("tex.import.new_folder"))).clicked() {
                    let title = t!("tex.folders.pick_title").to_owned();
                    self.pending_folder = Some(PendingPick::spawn(ui.ctx(), move || {
                        rfd::FileDialog::new().set_title(title).pick_folder()
                    }));
                }
            });
            ui.end_row();

            ui.label(t!("tex.import.surfaceprop"));
            let shown =
                if self.surface_prop.is_empty() { t!("common.none").to_owned() } else { self.surface_prop.clone() };
            egui::ComboBox::from_id_salt("hp_import_sp").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.surface_prop, String::new(), t!("common.none"));
                for sp in crate::util::SURFACE_PROPS {
                    ui.selectable_value(&mut self.surface_prop, (*sp).to_owned(), *sp);
                }
            });
            ui.end_row();

            ui.label("");
            ui.checkbox(&mut self.mipmaps, t!("tex.import.mipmaps"));
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut self.overwrite, t!("tex.import.overwrite"));
            ui.end_row();
        });

        let target = self.target(state);
        if let Some((dest, prefix)) = &target {
            ui.weak(tf!("tex.import.target", prefix, dest.display()));
        }
        ui.add_space(4.0);
        let can_start = target.is_some() && !self.files.is_empty();
        ui.horizontal(|ui| {
            let go = ui
                .add_enabled(can_start, egui::Button::new(egui::RichText::new(t!("tex.import.start")).strong()))
                .on_hover_text(hp_app::actions::key_label(egui::Key::Enter));
            let enter = can_start
                && ui.memory(|m| m.focused().is_none())
                && crate::util::is_top_dialog(ui.ctx(), ui.layer_id())
                && crate::util::take_key(ui.ctx(), egui::Modifiers::NONE, egui::Key::Enter) > 0;
            if go.clicked() || enter {
                self.start(state);
            }
            let first = self.files.first().cloned();
            if ui
                .add_enabled(first.is_some(), egui::Button::new(t!("tex.import.shader")))
                .on_hover_text(t!("tex.import.shader_tip"))
                .clicked()
                && let Some(file) = first
            {
                state.runtime.shader.open(hp_app::runtime::ShaderRequest { path: Some(file), ..Default::default() });
            }
            if ui.button(t!("common.cancel")).clicked() {
                self.files.clear();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batched_import_with_mock_assets() {
        let mut state = hp_app::testing::test_state();
        let dir = std::env::temp_dir().join("hp_ui_textures_import_dest");
        crate::folders::add_custom_dir(&mut state.settings, &dir);
        let mut d = ImportDialog::default();
        d.open(vec![PathBuf::from("b.png"), PathBuf::from("a.png"), PathBuf::from("a.png")]);
        assert!(d.is_active());
        assert_eq!(d.files.len(), 2);
        assert!(d.start(&mut state));
        assert!(d.is_running());
        let mut frames = 0;
        while !d.step(&mut state) {
            frames += 1;
            assert!(frames < 100);
        }
        assert!(!d.is_active());
        assert_eq!(state.current_material, "a");
        assert!(state.settings.browser.recent_materials.first().is_some_and(|m| m == "a"));
    }

    #[test]
    fn needs_a_destination() {
        let mut state = hp_app::testing::test_state();
        let mut d = ImportDialog::default();
        d.open(vec![PathBuf::from("a.png")]);
        assert!(!d.start(&mut state));
        assert!(!d.is_running());
    }
}
